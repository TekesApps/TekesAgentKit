//! Independent DSH SessionEndpoint adapter for the authenticated Remote HTTP/mux protocol.
mod extensions;
mod history;
mod multipart;
mod queue;
mod remote;
use async_trait::async_trait;
use futures_util::{StreamExt, stream};
pub use history::decode_records;
use remote::Remote;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
    time::Duration,
};
use tekes_agent_contract::*;
use tokio::sync::{Mutex, broadcast};
#[derive(Clone)]
pub struct DshConfiguration {
    pub identity: String,
    pub base_url: reqwest::Url,
    pub authorization: Option<(String, String)>,
    pub supported_versions: BTreeSet<String>,
    pub timeout: Duration,
}
impl DshConfiguration {
    pub fn new(identity: impl Into<String>, url: &str) -> Result<Self> {
        let mut url =
            reqwest::Url::parse(url).map_err(|_| Error::Protocol("invalid DSH URL".into()))?;
        require(
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "invalid DSH origin",
        )?;
        if !url.path().ends_with('/') {
            url.set_path(&format!("{}/", url.path()));
        }
        Ok(Self {
            identity: identity.into(),
            base_url: url,
            authorization: None,
            supported_versions: ["0.2.0-rc.2".into(), "0.2.1-rc.2".into()].into(),
            timeout: Duration::from_secs(30),
        })
    }
}
#[derive(Clone)]
pub struct DshConnector {
    config: DshConfiguration,
}
impl DshConnector {
    pub fn new(config: DshConfiguration) -> Self {
        Self { config }
    }
}
#[derive(Clone)]
struct Correlation {
    client: String,
    event: String,
    kind: &'static str,
}
pub struct DshSessionEndpoint {
    identity: String,
    host: HostDescription,
    remote: Arc<Remote>,
    generation: String,
    actionables: Arc<Mutex<BTreeMap<String, Correlation>>>,
    resolved: broadcast::Sender<Value>,
    child_addresses: Mutex<BTreeMap<String, Value>>,
}
#[async_trait]
impl EndpointConnector for DshConnector {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        require(
            !self.config.identity.is_empty(),
            "empty DSH endpoint identity",
        )?;
        let remote = Arc::new(Remote::connect(self.config.clone()).await?);
        let version = remote.probe_version().await?;
        let mut events = remote.open("$events", json!({})).await?;
        let ready = tokio::time::timeout(self.config.timeout, events.next())
            .await
            .map_err(|_| Error::Timeout)?
            .ok_or(Error::Disconnected)??;
        require(ready["type"] == "ready", "DSH events did not begin ready")?;
        string(&ready, "clientId")?;
        let home = string(&ready["host"], "home")?.to_owned();
        drop(events);
        let (resolved, _) = broadcast::channel(256);
        let host = HostDescription {
            protocol_version: 3,
            product: Product {
                name: "DeepSeek Harness".into(),
                version,
            },
            capabilities: [
                "workspace-sync",
                "session-inventory-sync",
                "session-control-sync",
                "actionable-sync",
                "session-journal",
                "models",
                "management",
            ]
            .map(String::from)
            .into(),
            cwd: home.clone(),
            home,
            provider: None,
            model: None,
            attached_sessions: 0,
            can_open_path: false,
        };
        Ok(Arc::new(DshSessionEndpoint {
            identity: self.config.identity.clone(),
            host,
            remote,
            generation: uuid::Uuid::new_v4().to_string(),
            actionables: Arc::new(Mutex::new(BTreeMap::new())),
            resolved,
            child_addresses: Mutex::new(BTreeMap::new()),
        }))
    }
}
fn rpc() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn address(id: &str) -> Value {
    json!({"kind":"session","sessionId":id})
}
struct FollowState {
    remote: Arc<Remote>,
    raw: FrameStream,
    target: StreamTarget,
    generation: String,
    client: Option<String>,
    began: bool,
    pending: VecDeque<Value>,
    actionables: Arc<Mutex<BTreeMap<String, Correlation>>>,
    assistant: history::AssistantStream,
    resolved: broadcast::Receiver<Value>,
}
impl FollowState {
    async fn normalize(&mut self, value: Value) -> Result<()> {
        queue::reject_reserved_anywhere(&value)?;
        let generation = &self.generation;
        match &self.target {
            StreamTarget::WorkspaceInventory => {
                let kind = string(&value, "type")?;
                require(
                    self.began || kind == "baseline",
                    "workspace delta before baseline",
                )?;
                require(
                    !self.began || kind != "baseline",
                    "duplicate workspace baseline",
                )?;
                self.began = true;
                let frame = match kind {
                    "baseline" | "replace" => {
                        json!({"type":kind,"generation":generation,"snapshot":value["value"]})
                    }
                    "order" => {
                        json!({"type":"reorder","generation":generation,"workspaceIds":value["workspaceIds"]})
                    }
                    "archived" => {
                        json!({"type":"archived","generation":generation,"sessionIds":value["archivedSessionIds"]})
                    }
                    "pinned" => {
                        json!({"type":"pinned","generation":generation,"sessionIds":value["pinnedSessionIds"]})
                    }
                    "upsert" | "remove" => {
                        let mut frame = value;
                        frame["generation"] = json!(generation);
                        frame
                    }
                    _ => return Err(Error::Protocol("unknown workspace frame".into())),
                };
                self.pending.push_back(frame);
            }

            StreamTarget::SessionJournal {
                address,
                max_messages,
            } => {
                let id = &address.session_id;
                match string(&value, "type")? {
                    "snapshot" => {
                        require(!self.began, "duplicate DSH journal snapshot")?;
                        self.began = true;
                        let entries = decode_records(array(&value, "records")?)?;
                        let projections = value
                            .get("projections")
                            .filter(|v| !v.is_null())
                            .map(queue::normalize_projection)
                            .transpose()?
                            .flatten();
                        let snapshot = JournalSnapshot {
                            session_id: id.clone(),
                            generation: generation.clone(),
                            through_sequence: integer(&value, "cursor")?,
                            window_limit: *max_messages,
                            entries,
                            has_more_before: boolean(&value, "hasMore")?,
                            projections: projections.map(serde_json::from_value).transpose()?,
                            history_revision: None,
                        };
                        snapshot.validate()?;
                        self.pending
                            .push_back(json!({"type":"snapshot","snapshot":snapshot}));
                        if let Some(opening) = value.get("assistantStream") {
                            for event in self.assistant.open(opening)? {
                                self.pending.push_back(json!({"type":"transient","generation":generation,"sessionId":id,"event":event}));
                            }
                        }
                    }
                    "event" => {
                        require(self.began, "DSH event before snapshot")?;
                        for entry in decode_records(&[value])? {
                            self.pending.push_back(json!({"type":"event","generation":generation,"sessionId":id,"entry":entry}));
                        }
                    }
                    "projection" => {
                        require(self.began, "DSH projection before snapshot")?;
                        let seq = integer(&value, "seq")?;
                        let key = string(&value, "key")?;
                        let projections =
                            queue::normalize_projection_delta(key, &value["value"], seq)?;
                        self.pending.push_back(json!({"type":"projection-block","generation":generation,"sessionId":id,"projections":projections}));
                    }
                    "assistant-stream" => {
                        require(self.began, "assistant stream before snapshot")?;
                        for event in self.assistant.receive(&value["frame"])? {
                            self.pending.push_back(json!({"type":"transient","generation":generation,"sessionId":id,"event":event}));
                        }
                    }
                    _ => return Err(Error::Protocol("unknown DSH follow frame".into())),
                }
            }
            StreamTarget::SessionControl => match string(&value, "type")? {
                "baseline" => {
                    require(!self.began, "duplicate DSH control baseline")?;
                    self.began = true;
                    let baseline = &value["value"];
                    let states = |key: &str, field: &str| -> Result<Vec<Value>> {
                        let Some(values) = baseline.get(key) else {
                            return Ok(Vec::new());
                        };
                        let map = values
                            .as_object()
                            .ok_or_else(|| Error::Protocol("invalid DSH control map".into()))?;
                        Ok(map
                            .iter()
                            .map(|(id, value)| json!({"sessionId":id,field:value}))
                            .collect())
                    };
                    let projection_states = || -> Result<Vec<Value>> {
                        let Some(values) = baseline.get("projections") else {
                            return Ok(Vec::new());
                        };
                        let map = values.as_object().ok_or_else(|| {
                            Error::Protocol("invalid DSH control projection map".into())
                        })?;
                        let mut states = Vec::new();
                        for (id, value) in map {
                            if let Some(projections) = queue::normalize_projection(value)? {
                                states.push(json!({"sessionId":id,"projections":projections}));
                            }
                        }
                        Ok(states)
                    };
                    self.pending.push_back(json!({"type":"baseline","generation":generation,"snapshot":{"queues":states("queues","items")?,"jobs":states("jobs","jobs")?,"projections":projection_states()?}}));
                }
                "queue" | "jobs" => {
                    require(self.began, "control delta before baseline")?;
                    let kind = string(&value, "type")?;
                    let field = if kind == "queue" { "items" } else { "jobs" };
                    self.pending.push_back(json!({"type":kind,"generation":generation,"state":{"sessionId":value["sessionId"],field:value[field]}}));
                }
                "projection" => {
                    require(self.began, "projection before baseline")?;
                    let projections = queue::normalize_projection_delta(
                        string(&value, "key")?,
                        &value["value"],
                        integer(&value, "seq")?,
                    )?;
                    self.pending.push_back(json!({"type":"projection","generation":generation,"state":{"sessionId":value["sessionId"],"projections":projections}}));
                }
                _ => return Err(Error::Protocol("unknown DSH control frame".into())),
            },
            StreamTarget::SessionInventory | StreamTarget::Actionables => {
                match string(&value, "type")? {
                    "ready" => {
                        require(!self.began, "duplicate DSH event readiness")?;
                        self.began = true;
                        self.client = Some(string(&value, "clientId")?.into());
                        if self.target == StreamTarget::SessionInventory {
                            let mut snapshot = self
                                .remote
                                .call("session/list", json!({"_request":{}}), &rpc())
                                .await?;
                            queue::reject_reserved_anywhere(&snapshot)?;
                            let mut cursor = snapshot
                                .get("nextCursor")
                                .and_then(Value::as_str)
                                .map(String::from);
                            let mut seen = BTreeSet::new();
                            while let Some(next) = cursor {
                                require(seen.insert(next.clone()), "DSH inventory cursor cycle")?;
                                let page = self
                                    .remote
                                    .call(
                                        "session/list",
                                        json!({"_request":{"cursor":next}}),
                                        &rpc(),
                                    )
                                    .await?;
                                queue::reject_reserved_anywhere(&page)?;
                                snapshot["items"]
                                    .as_array_mut()
                                    .ok_or_else(|| Error::Protocol("invalid inventory".into()))?
                                    .extend(array(&page, "items")?.clone());
                                cursor = page
                                    .get("nextCursor")
                                    .and_then(Value::as_str)
                                    .map(String::from);
                            }
                            queue::normalize_inventory_snapshot(&mut snapshot)?;
                            self.pending.push_back(json!({"type":"baseline","generation":generation,"snapshot":snapshot}));
                        } else {
                            self.pending
                                .push_back(json!({"type":"reset","generation":generation}));
                        }
                    }
                    "emit" => {
                        require(self.began, "DSH emit before ready")?;
                        if self.target == StreamTarget::SessionInventory {
                            let args = array(&value, "args")?;
                            let event = string(&value, "event")?;
                            let frame = match (event, args.as_slice()) {
                                ("api-session/added", [session]) => {
                                    let mut session = session.clone();
                                    queue::normalize_inventory_row(&mut session)?;
                                    Some(
                                        json!({"type":"upsert","generation":generation,"session":session}),
                                    )
                                }
                                ("api-session/removed", [id]) => Some(
                                    json!({"type":"remove","generation":generation,"sessionId":id}),
                                ),
                                ("api-session/status", [id, running]) => Some(
                                    json!({"type":"status","generation":generation,"sessionId":id,"running":running}),
                                ),
                                ("api-session/plane", [id, time]) => Some(
                                    json!({"type":"plane","generation":generation,"sessionId":id,"updatedAt":time}),
                                ),
                                ("api-session/error", [id, message]) => Some(
                                    json!({"type":"agent-error","generation":generation,"sessionId":id,"message":message}),
                                ),
                                _ => None,
                            };
                            if let Some(frame) = frame {
                                self.pending.push_back(frame);
                            }
                        }
                    }
                    "waterfall" => {
                        let client = self
                            .client
                            .as_ref()
                            .ok_or_else(|| Error::Protocol("waterfall before ready".into()))?;
                        let event = string(&value, "event")?;
                        let event_id = string(&value, "eventId")?;
                        if matches!(event, "approval/request" | "user-questions/request") {
                            if self.target == StreamTarget::Actionables {
                                let agent = string(&value, "agentId")?;
                                self.actionables.lock().await.insert(
                                    event_id.into(),
                                    Correlation {
                                        client: client.clone(),
                                        event: event_id.into(),
                                        kind: if event == "approval/request" {
                                            "approval"
                                        } else {
                                            "question"
                                        },
                                    },
                                );
                                let mut payload = value["request"].clone();
                                payload["sessionId"] = json!(agent);
                                self.pending.push_back(json!({"type":"upsert","generation":generation,"actionable":{"id":event_id,"sessionId":agent,"revision":0,"kind":if event=="approval/request"{"approval"}else{"question"},"payload":payload}}));
                            }
                        } else {
                            self.remote.call("$events/result",json!({"clientId":client,"eventId":event_id,"outcome":{"kind":"next"}}),&rpc()).await?;
                        }
                    }
                    "cancel" => {
                        if self.target == StreamTarget::Actionables {
                            let id = string(&value, "eventId")?;
                            self.actionables.lock().await.remove(id);
                            self.pending.push_back(json!({"type":"resolved","generation":generation,"actionableId":id,"revision":0}));
                        }
                    }
                    _ => return Err(Error::Protocol("unknown DSH remote event".into())),
                }
            }
            StreamTarget::SessionConversation { .. } => {
                return Err(unsupported("DSH native conversation"));
            }
        }
        Ok(())
    }
}
#[async_trait]
impl SessionEndpoint for DshSessionEndpoint {
    fn identity(&self) -> &str {
        &self.identity
    }
    fn host(&self) -> &HostDescription {
        &self.host
    }
    fn capabilities(&self) -> EndpointCapabilities {
        let mut caps = EndpointCapabilities::methods(&[
            "session.create",
            "session.rename",
            "session.fork",
            "session.archive",
            "session.unarchive",
            "session.prompt",
            "session.cancel",
            "session.updateQueue",
            "session.models",
            "models.list",
            "session.selectModel",
            "session.locator",
            "session.files.stat",
            "session.files.read",
            "session.files.readBytes",
            "session.uploadFile",
            "attachments.policy",
            "subagents.prompt",
            "subagents.cancel",
        ]);
        caps.methods.extend(
            extensions::ALIASES
                .iter()
                .chain(extensions::DIRECT_ROUTES)
                .map(|s| (*s).into()),
        );
        caps.supports_archive_session = true;
        caps.supports_unarchive_session = true;
        caps.can_fork_completed_turn = true;
        caps.supports_file_observation = true;
        caps
    }
    fn supports_events(&self) -> bool {
        true
    }
    fn sequenced_queue_projection_key(&self) -> Option<&'static str> {
        Some(SEQUENCED_QUEUE_PROJECTION_KEY)
    }
    async fn events(&self) -> Result<FrameStream> {
        let raw = self.remote.open("$events", json!({})).await?;
        let generation = self.generation.clone();
        let remote = self.remote.clone();
        Ok(Box::pin(stream::try_unfold(
            (raw, None::<String>, generation, remote),
            |(mut raw, mut client, generation, remote)| async move {
                let value = raw.next().await.ok_or(Error::Disconnected)??;
                queue::reject_reserved_anywhere(&value)?;
                let frame = if client.is_none() {
                    require(value["type"] == "ready", "endpoint events need readiness")?;
                    client = Some(string(&value, "clientId")?.into());
                    json!({"type":"baseline","generation":generation,"snapshot":value})
                } else {
                    // Observation never owns a waterfall. Explicitly continue so this peer cannot stall it.
                    if value["type"] == "waterfall" {
                        remote.call("$events/result",json!({"clientId":client,"eventId":value["eventId"],"outcome":{"kind":"next"}}),&rpc()).await?;
                    }
                    json!({"type":"event","generation":generation,"event":value})
                };
                Ok(Some((frame, (raw, client, generation, remote))))
            },
        )))
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        let (endpoint, args) = match &target {
            StreamTarget::WorkspaceInventory => ("workspace/follow", json!({})),
            StreamTarget::SessionInventory | StreamTarget::Actionables => ("$events", json!({})),
            StreamTarget::SessionControl => ("session/control", json!({})),
            StreamTarget::SessionJournal {
                address: session,
                max_messages,
            } => (
                "session/follow",
                json!({"request":{"address":self.history_address(&session.session_id).await?,"maxMessages":max_messages,"assistantStream":true}}),
            ),
            StreamTarget::SessionConversation { .. } => {
                return Err(unsupported("DSH native conversation"));
            }
        };
        let raw = self.remote.open(endpoint, args).await?;
        let state = FollowState {
            remote: self.remote.clone(),
            raw,
            target,
            generation: self.generation.clone(),
            client: None,
            began: false,
            pending: VecDeque::new(),
            actionables: self.actionables.clone(),
            assistant: history::AssistantStream::default(),
            resolved: self.resolved.subscribe(),
        };
        Ok(Box::pin(stream::try_unfold(
            state,
            |mut state| async move {
                loop {
                    if let Some(value) = state.pending.pop_front() {
                        return Ok(Some((value, state)));
                    }
                    tokio::select! {
                        value=state.raw.next()=>{let value=value.ok_or(Error::Disconnected)??;state.normalize(value).await?;}
                        value=state.resolved.recv(),if state.target==StreamTarget::Actionables=>{let mut value=value.map_err(|_|Error::Disconnected)?;value["generation"]=json!(state.generation);return Ok(Some((value,state)));}
                    }
                }
            },
        )))
    }
    async fn history_page(&self, request: HistoryRequest) -> Result<HistoryPage> {
        request.validate()?;
        if request.before_sequence == Some(-1) {
            return Ok(HistoryPage {
                session_id: request.session_id,
                through_sequence: request.through_sequence,
                entries: Vec::new(),
                has_more_before: false,
            });
        }
        let mut query = json!({"address":self.history_address(&request.session_id).await?,"throughSeq":request.through_sequence,"maxMessages":request.max_messages});
        if let Some(before) = request.before_sequence {
            query["beforeSeq"] = json!(before);
        }
        let result = self
            .remote
            .call("session/page", json!({"request":query}), &rpc())
            .await?;
        queue::reject_reserved_anywhere(&result)?;
        let page = HistoryPage {
            session_id: request.session_id.clone(),
            through_sequence: request.through_sequence,
            entries: decode_records(array(&result, "records")?)?,
            has_more_before: boolean(&result, "hasMore")?,
        };
        page.validate(&request)?;
        Ok(page)
    }
    async fn respond(&self, id: &str, revision: i64, outcome: Value) -> Result<()> {
        require(revision == 0, "stale DSH actionable revision")?;
        let correlation = self
            .actionables
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| unsupported("actionable is no longer pending"))?;
        let outcome = remote_actionable_outcome(outcome, correlation.kind);
        self.remote.call("$events/result",json!({"clientId":correlation.client,"eventId":correlation.event,"outcome":{"kind":"result","value":outcome}}),&rpc()).await?;
        self.actionables.lock().await.remove(id);
        let _ = self
            .resolved
            .send(json!({"type":"resolved","actionableId":id,"revision":0}));
        Ok(())
    }
    async fn locator(&self, id: &str) -> Result<SessionLocator> {
        let mut cursor: Option<String> = None;
        let mut seen = BTreeSet::new();
        let row = loop {
            let result=self.remote.call("session/list",json!({"_request":cursor.as_ref().map(|c|json!({"cursor":c})).unwrap_or(json!({}))}),&rpc()).await?;
            queue::reject_reserved_anywhere(&result)?;
            if let Some(row) = array(&result, "items")?
                .iter()
                .find(|row| row["sessionId"] == id)
            {
                break row.clone();
            }
            cursor = result["nextCursor"].as_str().map(String::from);
            let Some(next) = &cursor else {
                return Err(unsupported("session is absent from inventory"));
            };
            require(seen.insert(next.clone()), "DSH inventory cursor cycle")?;
        };
        let projections = self
            .remote
            .call(
                "session/projections",
                json!({"request":{"sessionId":id}}),
                &rpc(),
            )
            .await?;
        queue::reject_remote_projection(&projections)?;
        let working_directory = sequenced_working_directory(&projections)?;
        Ok(SessionLocator {
            address: SessionAddress {
                endpoint_id: self.identity.clone(),
                session_id: id.into(),
            },
            harness: "dsh".into(),
            cwd: row["cwd"].as_str().map(String::from),
            working_directory,
            native_session_file: None,
            native_session_id: Some(id.into()),
            history_source: "DSH session/follow + session/page".into(),
        })
    }
    async fn observe_files(&self, id: &str, path: &str) -> Result<FrameStream> {
        let raw = self
            .remote
            .open(
                "workspaceFiles/changes",
                json!({"workspaceFileScopeId":id,"path":path}),
            )
            .await?;
        Ok(Box::pin(stream::try_unfold(
            (raw, false),
            |(mut raw, mut ready)| async move {
                let frame = raw.next().await.ok_or(Error::Disconnected)??;
                validate_file_watch(&frame)?;
                require(
                    ready || frame["kind"] == "ready",
                    "file changes before ready",
                )?;
                require(
                    !ready || frame["kind"] != "ready",
                    "duplicate file readiness",
                )?;
                ready = true;
                Ok(Some((frame, (raw, ready))))
            },
        )))
    }
    async fn file_stat(&self, id: &str, path: &str) -> Result<SessionFileStat> {
        Ok(serde_json::from_value(
            self.remote
                .call(
                    "workspaceFiles/stat",
                    json!({"workspaceFileScopeId":id,"path":path}),
                    &rpc(),
                )
                .await?,
        )?)
    }
    async fn file_read_bytes(
        &self,
        id: &str,
        path: &str,
        offset: u64,
        length: usize,
    ) -> Result<SessionFileBytePage> {
        require(length > 0 && length <= 4194304, "invalid byte page")?;
        Ok(serde_json::from_value(self.remote.call("workspaceFiles/readBytes",json!({"workspaceFileScopeId":id,"path":path,"options":{"range":{"offset":offset,"length":length}}}),&rpc()).await?)?)
    }
    async fn file_read_text(
        &self,
        id: &str,
        path: &str,
        offset: u64,
        limit: usize,
    ) -> Result<SessionFileTextPage> {
        require(
            offset >= 1 && limit > 0 && limit <= 10000,
            "invalid text page",
        )?;
        Ok(serde_json::from_value(self.remote.call("workspaceFiles/read",json!({"workspaceFileScopeId":id,"path":path,"range":{"offset":offset,"limit":limit}}),&rpc()).await?)?)
    }
    async fn call(&self, method: &str, mut payload: Value, rpc_id: &str) -> Result<Value> {
        if method == "subagents.list" || method == "subagents/list" {
            return self
                .child_catalog(string(&payload, "parentSessionId")?, rpc_id)
                .await;
        }
        if matches!(method, "subagents.prompt" | "subagents.cancel") {
            let parent = string(&payload, "parentSessionId")?;
            let child = string(&payload, "childSessionId")?;
            let catalog = self.child_catalog(parent, rpc_id).await?;
            require(
                catalog["parentAvailable"] == true
                    && array(&catalog, "entries")?.iter().any(|row| {
                        row["id"] == child && row["kind"] == "child" && row["mode"] == "continuable"
                    }),
                "child is not continuable under this parent",
            )?;
            if method == "subagents.cancel" {
                return self.remote.call("subagents/interruptByParent",json!({"parentSessionId":parent,"childSessionId":child,"mode":"continuable"}),rpc_id).await;
            }
            require(
                !array(&payload, "content")?
                    .iter()
                    .any(|part| part["type"] == "file"),
                "child continuation does not support files",
            )?;
            payload["delivery"] = payload.get("mode").cloned().unwrap_or(json!("queue"));
            payload["requestId"] = json!(rpc_id);
            payload["mode"] = json!("continuable");
            let receipt = self
                .remote
                .call("subagents/prompt", json!({"request":payload}), rpc_id)
                .await?;
            require(
                receipt["messageId"]
                    .as_str()
                    .is_some_and(|id| !id.is_empty()),
                "invalid child admission receipt",
            )?;
            return Ok(json!({"accepted":true,"rpcId":rpc_id,"messageId":receipt["messageId"]}));
        }
        if let Some((endpoint, args)) = extensions::route(method, &payload)? {
            let result = self.remote.call(&endpoint, args, rpc_id).await?;
            return extensions::normalize_feedback_reply(method, result);
        }
        let endpoint = match method {
            "session.uploadFile" => {
                use base64::Engine;
                let data = base64::engine::general_purpose::STANDARD
                    .decode(string(&payload, "data")?)
                    .map_err(|_| Error::Protocol("invalid upload base64".into()))?;
                require(
                    !data.is_empty() && data.len() <= 8 * 1024 * 1024,
                    "upload limit exceeded",
                )?;
                return self
                    .remote
                    .upload(
                        string(&payload, "sessionId")?,
                        string(&payload, "name")?,
                        data,
                    )
                    .await;
            }
            "attachments.policy" => {
                let mut policy = json!({"inlineMediaTypes":["image/png","image/jpeg","image/webp","image/gif"],"maxAttachmentBytes":5242880,"maxAttachmentsPerMessage":20,"maxMessageAttachmentBytes":104857600,"maxImagePixels":40000000,"maxImageDimension":2000});
                if let Some(id) = payload["sessionId"].as_str() {
                    let value = self
                        .remote
                        .call(
                            "session/projections",
                            json!({"request":{"sessionId":id}}),
                            rpc_id,
                        )
                        .await?;
                    queue::reject_remote_projection(&value)?;
                    let limits = &value["values"]["imageLimits"];
                    if value["kind"] != "migration-required" && limits.is_object() {
                        policy = json!({"inlineMediaTypes":limits["mediaTypes"],"maxAttachmentBytes":limits["maxImageBytes"],"maxAttachmentsPerMessage":limits["maxImagesPerMessage"],"maxMessageAttachmentBytes":limits["maxMessageImageBytes"],"maxImagePixels":limits["maxImagePixels"],"maxImageDimension":limits["maxImageDimension"]});
                    }
                }
                return Ok(policy);
            }
            "session.create" => {
                require(
                    payload.get("execution").is_none(),
                    "Kernel execution configuration cannot authorize a DSH session",
                )?;
                "session/create"
            }
            "session.rename" => "session/rename",
            "session.fork" => {
                require(
                    payload["ephemeral"] != true,
                    "DSH does not support disposable forks without session/discard",
                )?;
                let mut fork = self
                    .remote
                    .call("session/fork", json!({"request":payload}), rpc_id)
                    .await?;
                let child = string(&fork, "sessionId")?.to_owned();
                // Like ClientKit: a completed-turn fork may inherit unconsumed inbox entries.
                // Preserve the created pointer even if best-effort cleanup fails; never retry the fork.
                if let Err(error) = self.clear_inherited_queue(&child).await {
                    fork["queueCleanupWarning"] = json!(error.to_string());
                }
                return Ok(fork);
            }
            "session.discard" => {
                return Err(unsupported("DSH runtime has no session/discard route"));
            }
            "session.prompt" => {
                payload["delivery"] = payload.get("mode").cloned().unwrap_or(json!("queue"));
                payload["requestId"] = json!(rpc_id);
                "session/prompt"
            }
            "session.cancel" => "session/cancel",
            "session.updateQueue" => "session/updateQueue",
            "session.selectModel" => "session/selectModel",
            "models.list" | "session.models" => {
                let catalog = self
                    .remote
                    .call("session/modelCatalog", json!({}), rpc_id)
                    .await?;
                let mut current = catalog["default"].clone();
                if method == "session.models" {
                    let projections = self
                        .remote
                        .call(
                            "session/projections",
                            json!({"request":{"sessionId":string(&payload,"sessionId")?}}),
                            rpc_id,
                        )
                        .await?;
                    queue::reject_remote_projection(&projections)?;
                    let next = &projections["values"]["modelSelection"]["next"];
                    if projections["kind"] != "migration-required" && next.is_object() {
                        current = next.clone();
                    }
                }
                let routable = array(&catalog, "routableProviders")?
                    .iter()
                    .any(|provider| *provider == current["provider"]);
                return Ok(
                    json!({"current":current,"routable":routable,"groups":catalog["groups"],"failures":catalog["failures"],"defaultSelection":catalog["default"],"routableProviders":catalog["routableProviders"]}),
                );
            }
            "session.archive" | "session.unarchive" => {
                return self
                    .remote
                    .call(
                        if method == "session.archive" {
                            "workspace/archiveSession"
                        } else {
                            "workspace/unarchiveSession"
                        },
                        json!({"request":payload}),
                        rpc_id,
                    )
                    .await;
            }
            "dsh.call" => {
                return self
                    .remote
                    .call(
                        string(&payload, "endpoint")?,
                        payload["args"].clone(),
                        rpc_id,
                    )
                    .await;
            }
            _ => return Err(unsupported(&format!("DSH method {method}"))),
        };
        self.remote
            .call(endpoint, json!({"request":payload}), rpc_id)
            .await
    }
    async fn close(&self) {
        self.remote.close();
        self.actionables.lock().await.clear();
    }
}

impl DshSessionEndpoint {
    async fn clear_inherited_queue(&self, id: &str) -> Result<()> {
        let mut stream = self
            .remote
            .open(
                "session/follow",
                json!({"request":{"address":address(id),"maxMessages":1}}),
            )
            .await?;
        let snapshot = tokio::time::timeout(Duration::from_secs(30), stream.next())
            .await
            .map_err(|_| Error::Timeout)?
            .ok_or(Error::Disconnected)??;
        drop(stream);
        require(
            snapshot["type"] == "snapshot",
            "fork queue needs opening snapshot",
        )?;
        for item_id in pending_inbox_ids(&snapshot["projections"])? {
            match self
                .remote
                .call(
                    "session/updateQueue",
                    json!({"request":{"sessionId":id,"itemId":item_id,"action":{"kind":"remove"}}}),
                    &rpc(),
                )
                .await
            {
                Ok(_) => {}
                Err(Error::Remote(error)) if error["code"] == "session/queue-item-not-found" => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}
impl DshSessionEndpoint {
    async fn history_address(&self, id: &str) -> Result<Value> {
        if let Some(address) = self.child_addresses.lock().await.get(id).cloned() {
            return Ok(address);
        }
        let mut cursor: Option<String> = None;
        let mut seen = BTreeSet::new();
        loop {
            let page = self.remote.call("session/list", json!({"_request":cursor.as_ref().map(|c|json!({"cursor":c})).unwrap_or(json!({}))}), &rpc()).await?;
            queue::reject_reserved_anywhere(&page)?;
            if let Some(row) = array(&page, "items")?.iter().find(|r| r["sessionId"] == id) {
                if let Some(parent) = row["parentSessionId"]
                    .as_str()
                    .filter(|_| row["origin"] == "subagent")
                {
                    self.child_catalog(parent, &rpc()).await?;
                    return self
                        .child_addresses
                        .lock()
                        .await
                        .get(id)
                        .cloned()
                        .ok_or_else(|| unsupported("child is missing from parent catalog"));
                }
                return Ok(address(id));
            }
            cursor = page["nextCursor"].as_str().map(str::to_owned);
            let Some(next) = &cursor else {
                return Ok(address(id));
            };
            require(seen.insert(next.clone()), "DSH inventory cursor cycle")?;
        }
    }
    async fn child_catalog(&self, parent: &str, rpc_id: &str) -> Result<Value> {
        require(!parent.is_empty(), "empty parent session")?;
        let projection = self
            .remote
            .call(
                "session/projections",
                json!({"request":{"sessionId":parent}}),
                rpc_id,
            )
            .await?;
        queue::reject_remote_projection(&projection)?;
        require(
            projection["kind"] == "sequenced"
                || (self.host.product.version == "0.2.0-rc.2"
                    && projection.get("kind").is_none()
                    && projection["asOfSeq"].as_i64().is_some_and(|seq| seq >= -1)),
            "subagent catalog projection unavailable",
        )?;
        let entries = array(&projection["values"], "subagentCatalog")?;
        let mut normalized = Vec::new();
        for entry in entries {
            string(entry, "id")?;
            let mut entry = entry.clone();
            entry["kind"] = json!("child");
            if matches!(
                entry["mode"].as_str(),
                Some("continuable" | "one-shot" | "unknown")
            ) {
                self.child_addresses.lock().await.insert(string(&entry, "id")?.into(), json!({"kind":"subagent","parentSessionId":parent,"childSessionId":entry["id"],"mode":entry["mode"]}));
            }
            normalized.push(entry);
        }
        // Availability is a live Agent fact, independent of running and durable catalog presence.
        let mut cursor: Option<String> = None;
        let mut seen = BTreeSet::new();
        let available = loop {
            let page = self.remote.call("session/list", json!({"_request":cursor.as_ref().map(|c|json!({"cursor":c})).unwrap_or(json!({}))}), rpc_id).await?;
            queue::reject_reserved_anywhere(&page)?;
            if let Some(row) = array(&page, "items")?
                .iter()
                .find(|r| r["sessionId"] == parent)
            {
                break row["agentAvailable"].as_bool().unwrap_or(false);
            }
            cursor = page["nextCursor"].as_str().map(str::to_owned);
            let Some(next) = &cursor else {
                break false;
            };
            require(seen.insert(next.clone()), "DSH inventory cursor cycle")?;
        };
        Ok(json!({"entries":normalized,"parentAvailable":available}))
    }
}
/// Shared Kernel-style envelopes are addressed by session; DSH waterfall
/// handlers consume the answer body or approval word, as ClientKit does.
/// Native DSH shapes and unknown kinds remain lossless.
fn remote_actionable_outcome(mut outcome: Value, kind: &str) -> Value {
    let Some(object) = outcome.as_object_mut() else {
        return outcome;
    };
    if kind == "question" && object.contains_key("answer") && !object.contains_key("answers") {
        return object.remove("answer").expect("present answer");
    }
    if kind == "approval"
        && object.contains_key("approvalId")
        && object.get("outcome").is_some_and(Value::is_string)
    {
        return object.remove("outcome").expect("present outcome");
    }
    outcome
}

fn pending_inbox_ids(projections: &Value) -> Result<Vec<String>> {
    let mut ids = Vec::new();
    let mut seen = BTreeSet::new();
    let inbox = &projections["values"]["inbox"];
    for key in ["next-turn", "next-step"] {
        if let Some(items) = inbox.get(key) {
            for item in items
                .as_array()
                .ok_or_else(|| Error::Protocol("invalid fork inbox".into()))?
            {
                let id = string(item, "id")?;
                require(
                    !id.is_empty() && seen.insert(id.to_owned()),
                    "invalid or duplicate fork inbox id",
                )?;
                ids.push(id.to_owned());
            }
        }
    }
    Ok(ids)
}

/// Migration cache hints are not sequenced authority and must not drive current directory state.
fn sequenced_working_directory(projections: &Value) -> Result<Option<String>> {
    if projections.is_null() || projections["kind"] == "migration-required" {
        return Ok(None);
    }
    if let Some(kind) = projections.get("kind") {
        require(kind == "sequenced", "unknown DSH projection authority")?;
    }
    require(
        integer(projections, "asOfSeq")? >= -1,
        "invalid DSH projection cut",
    )?;
    match projections["values"].get("workingDirectory") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(path)) if !path.is_empty() => Ok(Some(path.clone())),
        _ => Err(Error::Protocol(
            "invalid working-directory projection".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };
    use tokio_tungstenite::{accept_async, tungstenite::Message};

    async fn read_http_request(stream: &mut TcpStream) -> (String, Vec<u8>) {
        let mut bytes = Vec::new();
        while !bytes.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).await.unwrap();
            bytes.push(byte[0]);
        }
        let header_end = bytes.len();
        let headers = String::from_utf8(bytes.clone()).unwrap();
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().unwrap())
            })
            .unwrap_or_default();
        bytes.resize(header_end + content_length, 0);
        stream.read_exact(&mut bytes[header_end..]).await.unwrap();
        let request_line = headers.lines().next().unwrap().to_owned();
        (request_line, bytes[header_end..].to_vec())
    }

    async fn serve_http(mut stream: TcpStream, baseline: Value) {
        let (request_line, body) = read_http_request(&mut stream).await;
        let (status, value) = if request_line.starts_with("GET /api/tekes/harness-version ") {
            (
                "200 OK",
                json!({"product":"@deepseek-ai/dsh","source":"@deepseek-ai/dsh-app-boot#getDshRuntimeVersion","version":"0.2.1-rc.2"}),
            )
        } else if request_line.starts_with("POST /api/session/list ") {
            let request: Value = serde_json::from_slice(&body).unwrap();
            (
                "200 OK",
                json!({"type":"server-response","rpcId":request["rpcId"],"result":{"ok":true,"value":baseline}}),
            )
        } else {
            ("404 Not Found", json!({"error":"unexpected request"}))
        };
        let body = value.to_string();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    }

    async fn serve_events(stream: TcpStream, added: Value) {
        let Ok(mut socket) = accept_async(stream).await else {
            return;
        };
        while let Some(Ok(Message::Text(text))) = socket.next().await {
            let Ok(frame) = serde_json::from_str::<Value>(&text) else {
                return;
            };
            if frame["type"] == "open" && frame["endpoint"] == "$events" {
                let stream_id = frame["streamId"].as_str().unwrap();
                for value in [
                    json!({"type":"ready","clientId":"mock-client"}),
                    json!({"type":"emit","event":"api-session/added","args":[added]}),
                ] {
                    let outgoing = json!({"type":"item","streamId":stream_id,"value":value});
                    if socket
                        .send(Message::Text(outgoing.to_string().into()))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            } else if frame["type"] == "cancel" {
                return;
            }
        }
    }

    async fn serve_mock(baseline: Value, added: Value) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                let mut prefix = [0; 128];
                let request = tokio::time::timeout(Duration::from_secs(2), async {
                    loop {
                        let count = stream.peek(&mut prefix).await.unwrap_or_default();
                        let request = String::from_utf8_lossy(&prefix[..count]);
                        if request.contains("\r\n") {
                            break request.into_owned();
                        }
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .expect("mock request line arrives");
                let baseline = baseline.clone();
                let added = added.clone();
                if request.starts_with("GET /api/remote.mux ") {
                    tokio::spawn(serve_events(stream, added));
                } else {
                    tokio::spawn(serve_http(stream, baseline));
                }
            }
        });
        (format!("http://{address}/"), task)
    }

    #[tokio::test]
    async fn pinned_dsh_session_summaries_survive_rpc_baseline_and_added_event() {
        let child = json!({
            "sessionId":"child",
            "updatedAt":1700000000100_i64,
            "agentAvailable":false,
            "formatStatus":"current",
            "running":false,
            "blank":false,
            "parentSessionId":"parent",
            "origin":"subagent",
            "cwd":"/work/child",
            "projections":{"kind":"sequenced","asOfSeq":12,"values":{"sessionListMetadata":{"blank":false,"lastPromptAt":1700000000100_i64}}}
        });
        let parent_only_fork = json!({
            "sessionId":"fork-without-origin",
            "updatedAt":1700000000200_i64,
            "agentAvailable":false,
            "running":false,
            "blank":false,
            "parentSessionId":"parent",
            "cwd":"/work/fork"
        });
        let no_origin = json!({
            "sessionId":"ordinary",
            "updatedAt":1700000000300_i64,
            "agentAvailable":true,
            "running":true,
            "blank":false,
            "cwd":"/work"
        });
        // The supported 0.2.0-rc.2 public summary has no formatStatus field.
        // Exercise that actual shape, rather than merely relabeling a mock runtime version.
        let legacy_child = json!({
            "sessionId":"legacy-child", "updatedAt":1700000000050_i64,
            "agentAvailable":false, "running":false, "blank":false,
            "parentSessionId":"parent", "origin":"subagent", "cwd":"/work/legacy-child"
        });
        let baseline = json!({"items":[child.clone(),legacy_child.clone(),parent_only_fork.clone(),no_origin.clone()]});
        let added = json!({
            "sessionId":"live-child",
            "updatedAt":1700000000400_i64,
            "agentAvailable":true,
            "formatStatus":"current",
            "running":true,
            "blank":false,
            "parentSessionId":"parent",
            "origin":"subagent",
            "cwd":"/work/live-child",
            "projections":{"kind":"sequenced","asOfSeq":19,"values":{"sessionListMetadata":{"blank":false,"lastPromptAt":1700000000400_i64}}}
        });
        let (url, server) = serve_mock(baseline, added.clone()).await;
        let mut config = DshConfiguration::new("dsh-test", &url).unwrap();
        config.timeout = Duration::from_secs(2);
        let remote = Arc::new(Remote::connect(config).await.unwrap());
        let (resolved, _) = broadcast::channel(8);
        let endpoint = DshSessionEndpoint {
            identity: "dsh-test".into(),
            host: HostDescription {
                protocol_version: 3,
                product: Product {
                    name: "DeepSeek Harness".into(),
                    version: "0.2.1-rc.2".into(),
                },
                capabilities: BTreeSet::new(),
                cwd: "/work".into(),
                home: "/work".into(),
                provider: None,
                model: None,
                attached_sessions: 0,
                can_open_path: false,
            },
            remote,
            generation: "wire-test-generation".into(),
            actionables: Arc::new(Mutex::new(BTreeMap::new())),
            resolved,
            child_addresses: Mutex::new(BTreeMap::new()),
        };
        let mut frames = endpoint
            .follow(StreamTarget::SessionInventory)
            .await
            .unwrap();
        let first = tokio::time::timeout(Duration::from_secs(2), frames.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(first["type"], "baseline");
        assert_eq!(
            first["snapshot"]["items"],
            json!([child, legacy_child, parent_only_fork, no_origin])
        );
        let second = tokio::time::timeout(Duration::from_secs(2), frames.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(second["type"], "upsert");
        assert_eq!(second["session"], added);
        assert!(second["session"].get("ephemeral").is_none());
        assert!(first["snapshot"]["items"][1].get("formatStatus").is_none());
        assert!(first["snapshot"]["items"][3].get("origin").is_none());
        assert!(first["snapshot"]["items"][2].get("origin").is_none());
        assert!(
            first["snapshot"]["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|item| item.get("ephemeral").is_none())
        );
        drop(frames);
        drop(endpoint);
        server.abort();
    }

    #[test]
    fn actionable_responses_unwrap_shared_envelopes_by_registered_kind() {
        let answer = json!({"answers":[{"id":"q","selected":["继续"],"custom":""}]});
        let envelope = json!({"sessionId":"s","answer":answer});
        assert_eq!(
            remote_actionable_outcome(envelope.clone(), "question"),
            answer
        );
        assert_eq!(
            remote_actionable_outcome(answer.clone(), "question"),
            answer
        );
        for word in ["allowed-once", "rejected"] {
            assert_eq!(
                remote_actionable_outcome(
                    json!({"sessionId":"s","approvalId":"p","outcome":word}),
                    "approval"
                ),
                json!(word)
            );
            assert_eq!(
                remote_actionable_outcome(json!(word), "approval"),
                json!(word)
            );
        }
        assert_eq!(
            remote_actionable_outcome(envelope.clone(), "unknown"),
            envelope
        );
        let native = json!({"answers":[],"answer":"not-an-envelope"});
        assert_eq!(
            remote_actionable_outcome(native.clone(), "question"),
            native
        );
        let unrelated = json!({"outcome":"rejected"});
        assert_eq!(
            remote_actionable_outcome(unrelated.clone(), "approval"),
            unrelated
        );
        let wrong_kind = json!({"approvalId":"p","outcome":"allowed-once"});
        assert_eq!(
            remote_actionable_outcome(wrong_kind.clone(), "question"),
            wrong_kind
        );
    }
    #[test]
    fn inherited_inbox_keeps_order_and_rejects_bad_ids() {
        assert_eq!(pending_inbox_ids(&json!({"values":{"inbox":{"next-turn":[{"id":"first"}],"next-step":[{"id":"second"}]}}})).unwrap(),["first","second"]);
        assert!(pending_inbox_ids(&json!({"values":{"inbox":{"next-turn":[{}]}}})).is_err());
        assert!(pending_inbox_ids(&json!({"values":{"inbox":{"next-turn":[{"id":"same"}],"next-step":[{"id":"same"}]}}})).is_err());
    }
    #[test]
    fn directory_uses_sequenced_authority_never_migration_hints() {
        assert_eq!(
            sequenced_working_directory(
                &json!({"kind":"sequenced","asOfSeq":10,"values":{"workingDirectory":"/next"}})
            )
            .unwrap(),
            Some("/next".into())
        );
        assert_eq!(
            sequenced_working_directory(
                &json!({"asOfSeq":0,"values":{"workingDirectory":"/legacy"}})
            )
            .unwrap(),
            Some("/legacy".into())
        );
        assert_eq!(
            sequenced_working_directory(
                &json!({"kind":"migration-required","values":{"workingDirectory":"/stale"}})
            )
            .unwrap(),
            None
        );
        assert!(
            sequenced_working_directory(
                &json!({"kind":"cached","asOfSeq":9000,"values":{"workingDirectory":"/stale"}})
            )
            .is_err()
        );
        assert!(sequenced_working_directory(&json!({"kind":"unknown","asOfSeq":0})).is_err());
        assert!(sequenced_working_directory(&json!({"kind":"sequenced","values":{}})).is_err());
    }
}
