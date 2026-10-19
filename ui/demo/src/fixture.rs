//! Deterministic, local SEP authority fixture. No model, credentials or harness-private reads.
use async_trait::async_trait;
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tekes_agent_contract::*;
use tekes_agent_presentation::{RunController, RunScope};
use tekes_agent_session::SessionRuntime;
use tokio::sync::broadcast;
/// Display-only receipt is removed before prompt admission in the native proof.
/// No filesystem/upload authority is implied by this isolated fixture.
pub fn composer_context_fixture() -> (
    Vec<tekes_agent_presentation::ComposerArtifact>,
    Vec<tekes_agent_presentation::ComposerAnnotation>,
) {
    use tekes_agent_presentation::{ComposerAnnotation, ComposerArtifact};
    (
        vec![
            ComposerArtifact::Attachment {
                id: "fixture-file".into(),
                name: "草稿附件.png".into(),
                content: json!({"type":"file","receiptId":"DISPLAY_ONLY_NOT_UPLOADED"}),
                facts: None,
            },
            ComposerArtifact::Reference {
                id: "fixture-reference".into(),
                mention: Some("上下文引用说明".into()),
                path: None,
                excerpt: Some("这段引用随草稿保留".into()),
            },
        ],
        vec![ComposerAnnotation {
            id: "fixture-annotation".into(),
            source: "回答 · turn 1".into(),
            excerpt: "中文引用 👩🏽‍💻".into(),
            comment: "请解释这段内容".into(),
            source_locator: None,
        }],
    )
}
type MutationReceipts = BTreeMap<String, (String, Value, Value)>;
#[derive(Clone)]
pub struct Fixture {
    sessions: Arc<Mutex<BTreeMap<String, Line>>>,
    connects: Arc<AtomicUsize>,
    forks: Arc<AtomicUsize>,
    side_chats: bool,
    children: Arc<Mutex<BTreeMap<String, (String, String)>>>,
    parent_available: Arc<AtomicBool>,
    catalog_fails: Arc<AtomicBool>,
    lifecycle: bool,
    models: bool,
    uploads: bool,
    upload_calls: Arc<AtomicUsize>,
    upload_gate: Option<Arc<tokio::sync::Semaphore>>,
    upload_fail_at: Option<usize>,
    presets: Option<bool>,
    inventory_sender: broadcast::Sender<Value>,
    workspace_sender: broadcast::Sender<Value>,
    mutations: Arc<Mutex<MutationReceipts>>,
}
struct Line {
    rows: Vec<HistoryEntry>,
    sender: broadcast::Sender<Value>,
    cancel: Arc<AtomicBool>,
    busy: bool,
    turn: i64,
    ephemeral: bool,
    title: String,
    archived: bool,
    model_selection: Value,
}
fn entry(seq: i64, kind: &str, data: Value) -> HistoryEntry {
    serde_json::from_value(json!({"event":{"seq":seq,"time":seq+1,"type":kind,"data":data}}))
        .unwrap()
}
fn append(line: &mut Line, kind: &str, data: Value) {
    let row = entry(line.rows.len() as i64, kind, data);
    line.rows.push(row.clone());
    let _ = line.sender.send(json!({"type":"event","entry":row}));
}
fn summary(id: &str, line: &Line) -> Value {
    json!({"sessionId":id,"title":line.title,"status":if line.archived {"archived"} else {"active"},"running":line.busy,"blank":line.rows.is_empty(),"updatedAt":line.rows.len()})
}
fn inventory_stream(
    baseline: Value,
    receiver: broadcast::Receiver<Value>,
    generation: String,
) -> FrameStream {
    let updates = stream::unfold(receiver, move |mut receiver| {
        let generation = generation.clone();
        async move {
            let result = receiver
                .recv()
                .await
                .map(|mut value| {
                    value["generation"] = json!(generation);
                    value
                })
                .map_err(|_| Error::Disconnected);
            Some((result, receiver))
        }
    });
    Box::pin(stream::once(async { Ok(baseline) }).chain(updates))
}
impl Default for Fixture {
    fn default() -> Self {
        let mut sessions = BTreeMap::new();
        for id in ["a", "b"] {
            let (sender, _) = broadcast::channel(256);
            let mut line = Line {
                rows: vec![],
                sender,
                cancel: Arc::new(AtomicBool::new(false)),
                busy: false,
                turn: 30,
                ephemeral: false,
                title: format!("会话 {id}"),
                archived: false,
                model_selection: json!({"provider":"fixture","model":"initial"}),
            };
            for n in 1..=30 {
                append(&mut line, "turn/start", json!({"turn":n}));
                append(
                    &mut line,
                    "user/message",
                    json!({"turn":n,"id":format!("{id}-u{n}"),"content":[{"type":"text","text":format!("会话 {id} · 历史问题 {n}")}]}),
                );
                append(
                    &mut line,
                    "assistant/message",
                    json!({"turn":n,"step":1,"message":{"id":format!("{id}-m{n}"),"content":[{"type":"text","text":format!("历史回答 {n}：这是一段用于验证阅读位置与分页的中文记录。")}]}}),
                );
                append(
                    &mut line,
                    "turn/end",
                    json!({"turn":n,"reason":"completed"}),
                );
            }
            sessions.insert(id.into(), line);
        }
        Self {
            sessions: Arc::new(Mutex::new(sessions)),
            connects: Arc::new(AtomicUsize::new(0)),
            forks: Arc::new(AtomicUsize::new(0)),
            side_chats: false,
            children: Arc::new(Mutex::new(BTreeMap::new())),
            parent_available: Arc::new(AtomicBool::new(true)),
            catalog_fails: Arc::new(AtomicBool::new(false)),
            lifecycle: false,
            models: false,
            uploads: false,
            upload_calls: Arc::new(AtomicUsize::new(0)),
            upload_gate: None,
            upload_fail_at: None,
            presets: None,
            inventory_sender: broadcast::channel(256).0,
            workspace_sender: broadcast::channel(256).0,
            mutations: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }
}
impl Fixture {
    pub fn with_uploads(mut self) -> Self {
        self.uploads = true;
        self
    }
    pub fn with_upload_gate(mut self, gate: Arc<tokio::sync::Semaphore>) -> Self {
        self.upload_gate = Some(gate);
        self
    }
    pub fn with_upload_failure(mut self, index: usize) -> Self {
        self.upload_fail_at = Some(index);
        self
    }
    pub fn upload_calls(&self) -> usize {
        self.upload_calls.load(Ordering::SeqCst)
    }
    pub fn with_presets(mut self, legacy_roster: bool) -> Self {
        self.presets = Some(legacy_roster);
        self
    }
    pub fn creation_receipts(&self) -> Vec<Value> {
        self.mutations
            .lock()
            .unwrap()
            .values()
            .filter(|(method, _, _)| method == "session.create")
            .map(|(_, payload, _)| payload.clone())
            .collect()
    }
    pub fn with_models(mut self) -> Self {
        self.models = true;
        self
    }
    pub fn with_lifecycle(mut self) -> Self {
        self.lifecycle = true;
        self
    }
    fn record(&self, method: &str, payload: &Value, rpc: &str, value: Value) -> Value {
        self.mutations
            .lock()
            .unwrap()
            .insert(rpc.into(), (method.into(), payload.clone(), value.clone()));
        value
    }
    pub fn with_children(self) -> Self {
        let mut children = self.children.lock().unwrap();
        let mut sessions = self.sessions.lock().unwrap();
        for (id, parent, mode) in [
            ("child-a", "a", "continuable"),
            ("child-b", "b", "continuable"),
            ("nested-a", "child-a", "continuable"),
            ("bounded-b", "b", "one-shot"),
        ] {
            let mut line = Line {
                rows: vec![],
                sender: broadcast::channel(256).0,
                cancel: Arc::new(AtomicBool::new(false)),
                busy: false,
                turn: 30,
                ephemeral: false,
                title: format!("会话 {id}"),
                archived: false,
                model_selection: json!({"provider":"fixture","model":"initial"}),
            };
            for turn in 1..=30 {
                append(&mut line, "turn/start", json!({"turn":turn}));
                append(
                    &mut line,
                    "user/message",
                    json!({"turn":turn,"id":format!("{id}-u{turn}"),"content":[{"type":"text","text":format!("{id} · 子历史 {turn}")}]}),
                );
                append(
                    &mut line,
                    "assistant/message",
                    json!({"turn":turn,"step":1,"message":{"id":format!("{id}-m{turn}"),"content":[{"type":"text","text":"子代理历史回答"}]}}),
                );
                append(
                    &mut line,
                    "turn/end",
                    json!({"turn":turn,"reason":"completed"}),
                );
            }
            children.insert(id.into(), (parent.into(), mode.into()));
            sessions.insert(id.into(), line);
        }
        drop(sessions);
        drop(children);
        self
    }
    pub fn set_child_mode(&self, id: &str, mode: &str) {
        self.children
            .lock()
            .unwrap()
            .get_mut(id)
            .expect("fixture child")
            .1 = mode.into();
    }
    pub fn set_parent_available(&self, available: bool) {
        self.parent_available.store(available, Ordering::SeqCst);
    }
    pub fn set_catalog_fails(&self, fails: bool) {
        self.catalog_fails.store(fails, Ordering::SeqCst);
    }
    pub fn with_side_chats(mut self) -> Self {
        self.side_chats = true;
        self
    }
    /// Explicit owned demo data, emitted through the same journal contract.
    pub fn with_demo_summary(self) -> Self {
        {
            let mut sessions = self.sessions.lock().unwrap();
            for (id, line) in sessions.iter_mut() {
                append(
                    line,
                    "todo/write",
                    json!({"todos":[
                        {"content":format!("验证会话 {id} 的数据回传"),"status":"completed"},
                        {"content":"接入正式 Transcript","status":"pending"}
                    ]}),
                );
            }
        }
        self
    }
}
struct Endpoint {
    fixture: Fixture,
    host: HostDescription,
    generation: String,
}
#[async_trait]
impl EndpointConnector for Fixture {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let mut endpoint = Endpoint {
            fixture: self.clone(),
            generation: format!("fixture-{}", self.connects.fetch_add(1, Ordering::SeqCst)),
            host: serde_json::from_value(
                json!({"protocolVersion":4,"product":{"name":"Local SEP fixture","version":"1"},"capabilities":["session-journal","session-inventory-sync","session-control-sync","actionable-sync"],"cwd":"/fixture","home":"/fixture","attachedSessions":0,"canOpenPath":false}),
            )?,
        };
        if self.lifecycle {
            endpoint.host.capabilities.insert("workspace-sync".into());
        }
        Ok(Arc::new(endpoint))
    }
}
#[async_trait]
impl SessionEndpoint for Endpoint {
    fn identity(&self) -> &str {
        "fixture"
    }
    fn host(&self) -> &HostDescription {
        &self.host
    }
    fn capabilities(&self) -> EndpointCapabilities {
        let mut caps = EndpointCapabilities::methods(&["session.prompt", "session.cancel"]);
        if self.fixture.uploads {
            caps.methods
                .extend(["session.uploadFile".into(), "attachments.policy".into()]);
        }
        if let Some(legacy) = self.fixture.presets {
            caps.methods.extend([
                "session.create".into(),
                if legacy {
                    "presets.list"
                } else {
                    "session.initialPresets"
                }
                .into(),
            ]);
        }
        if self.fixture.models {
            caps.methods
                .extend(["session.models".into(), "session.selectModel".into()]);
        }
        if self.fixture.side_chats {
            caps.methods
                .extend(["session.fork".into(), "session.discard".into()]);
            caps.supports_ephemeral_fork = true;
        }
        if !self.fixture.children.lock().unwrap().is_empty() {
            caps.methods.extend([
                "subagents.list".into(),
                "subagents.prompt".into(),
                "subagents.cancel".into(),
                "subagents.history".into(),
            ]);
        }
        if self.fixture.lifecycle {
            caps.supports_archive_session = true;
            caps.supports_unarchive_session = true;
            caps.methods.extend([
                "session.rename".into(),
                "session.fork".into(),
                "session.archive".into(),
                "session.unarchive".into(),
            ]);
        }
        caps
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        let g = &self.generation;
        let baseline = match target {
            StreamTarget::SessionInventory => {
                let sessions = self.fixture.sessions.lock().unwrap();
                let receiver = self.fixture.inventory_sender.subscribe();
                let items: Vec<_> = sessions
                    .iter()
                    .filter(|(_, line)| !line.ephemeral)
                    .map(|(id, line)| summary(id, line))
                    .collect();
                let baseline = json!({"type":"baseline","generation":g,"snapshot":{"items":items}});
                return Ok(inventory_stream(baseline, receiver, g.clone()));
            }
            StreamTarget::WorkspaceInventory if self.fixture.lifecycle => {
                let sessions = self.fixture.sessions.lock().unwrap();
                let receiver = self.fixture.workspace_sender.subscribe();
                let ids: Vec<_> = sessions
                    .iter()
                    .filter(|(_, line)| line.archived)
                    .map(|(id, _)| id)
                    .collect();
                let baseline = json!({"type":"baseline","generation":g,"snapshot":{"items":[],"archivedSessionIds":ids}});
                return Ok(inventory_stream(baseline, receiver, g.clone()));
            }
            StreamTarget::SessionControl => {
                json!({"type":"baseline","generation":g,"snapshot":{"queues":[],"jobs":[],"projections":[]}})
            }
            StreamTarget::Actionables => json!({"type":"reset","generation":g}),
            StreamTarget::SessionJournal { address, .. } => {
                let sessions = self.fixture.sessions.lock().unwrap();
                let line = sessions
                    .get(&address.session_id)
                    .ok_or_else(|| unsupported("fixture session"))?;
                let receiver = line.sender.subscribe();
                let start = line.rows.len().saturating_sub(40);
                let baseline = json!({"type":"snapshot","snapshot":{"sessionId":address.session_id,"generation":g,"throughSequence":line.rows.len() as i64-1,"windowLimit":40,"entries":line.rows[start..],"hasMoreBefore":start>0}});
                let id = address.session_id;
                let generation = g.clone();
                let updates = stream::unfold(receiver, move |mut receiver| {
                    let generation = generation.clone();
                    let id = id.clone();
                    async move {
                        let result = receiver
                            .recv()
                            .await
                            .map(|mut v| {
                                v["generation"] = json!(generation);
                                v["sessionId"] = json!(id);
                                v
                            })
                            .map_err(|_| Error::Disconnected);
                        Some((result, receiver))
                    }
                });
                return Ok(Box::pin(
                    stream::once(async { Ok(baseline) }).chain(updates),
                ));
            }
            _ => return Err(unsupported("fixture stream")),
        };
        Ok(Box::pin(
            stream::once(async { Ok(baseline) }).chain(stream::pending()),
        ))
    }
    async fn history_page(&self, r: HistoryRequest) -> Result<HistoryPage> {
        r.validate()?;
        let sessions = self.fixture.sessions.lock().unwrap();
        let line = sessions
            .get(&r.session_id)
            .ok_or_else(|| unsupported("fixture session"))?;
        let end = r.before_sequence.unwrap_or(r.through_sequence + 1).max(0) as usize;
        require(end <= line.rows.len(), "fixture history cut")?;
        let start = end.saturating_sub(r.max_messages);
        Ok(HistoryPage {
            session_id: r.session_id,
            through_sequence: r.through_sequence,
            entries: line.rows[start..end].to_vec(),
            has_more_before: start > 0,
        })
    }
    async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
        Err(unsupported("fixture actionable"))
    }
    async fn call(&self, method: &str, p: Value, rpc: &str) -> Result<Value> {
        if self.fixture.uploads && method == "attachments.policy" {
            return Ok(json!({"inlineMediaTypes":[],"maxAttachmentBytes":8388608,
                "maxAttachmentsPerMessage":4,"maxMessageAttachmentBytes":16777216}));
        }
        if self.fixture.uploads && method == "session.uploadFile" {
            let index = self.fixture.upload_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(gate) = &self.fixture.upload_gate {
                gate.acquire()
                    .await
                    .map_err(|_| unsupported("upload gate closed"))?
                    .forget();
            }
            if self.fixture.upload_fail_at == Some(index) {
                return Err(unsupported("fixture upload outcome unknown"));
            }
            let data = string(&p, "data")?;
            // Fixture computes decoded length of transport-produced base64 only.
            let bytes = data.len() / 4 * 3 - data.bytes().rev().take_while(|c| *c == b'=').count();
            return Ok(self.fixture.record(
                method,
                &p,
                rpc,
                json!({
                "receiptId":format!("receipt-{index}"),"file":{
                    "attachmentId":format!("file-{index}"),"name":p["name"],"bytes":bytes}}),
            ));
        }
        if let Some(legacy) = self.fixture.presets {
            if method
                == if legacy {
                    "presets.list"
                } else {
                    "session.initialPresets"
                }
            {
                require(
                    !self.fixture.catalog_fails.load(Ordering::SeqCst),
                    "fixture preset catalog unavailable",
                )?;
                return Ok(if legacy {
                    json!({"presets":[{"id":"coding","name":"编程","isDefault":true},{"id":"broken","name":"不可用","broken":"缺少配置"}]})
                } else {
                    json!({"presets":[{"id":"coding","title":"编程","configurationText":"SOURCE_NOT_PROMPT"},{"id":"broken","title":"不可用","unavailableReason":"缺少配置"}],"defaultID":"coding"})
                });
            }
            if method == "session.create" {
                if let Some((old_method, old_payload, result)) =
                    self.fixture.mutations.lock().unwrap().get(rpc)
                {
                    require(
                        old_method == method && old_payload == &p,
                        "fixture create identity changed",
                    )?;
                    return Ok(result.clone());
                }
                let id = string(&p, "sessionId")?;
                require(
                    p["agentPreset"].is_null() || p["agentPreset"] == "coding",
                    "fixture initial preset unavailable",
                )?;
                let mut sessions = self.fixture.sessions.lock().unwrap();
                require(
                    !sessions.contains_key(id),
                    "fixture create identity collision",
                )?;
                let line = Line {
                    rows: vec![],
                    sender: broadcast::channel(256).0,
                    cancel: Arc::new(AtomicBool::new(false)),
                    busy: false,
                    turn: 0,
                    ephemeral: false,
                    title: "预设新会话".into(),
                    archived: false,
                    model_selection: json!({"provider":"fixture","model":"initial"}),
                };
                let _ = self
                    .fixture
                    .inventory_sender
                    .send(json!({"type":"upsert","session":summary(id,&line)}));
                sessions.insert(id.into(), line);
                return Ok(self
                    .fixture
                    .record(method, &p, rpc, json!({"sessionId":id})));
            }
        }
        if method == "subagents.list" {
            if self.fixture.catalog_fails.load(Ordering::SeqCst) {
                return Err(Error::Transport("fixture catalog unavailable".into()));
            }
            let parent = string(&p, "parentSessionId")?;
            let entries: Vec<_> = self
                .fixture
                .children
                .lock()
                .unwrap()
                .iter()
                .filter(|(_, (owner, _))| owner == parent)
                .map(|(id, (_, mode))| json!({"kind":"child","id":id,"mode":mode}))
                .collect();
            let available = self.fixture.parent_available.load(Ordering::SeqCst)
                && self
                    .fixture
                    .sessions
                    .lock()
                    .unwrap()
                    .get(parent)
                    .is_some_and(|line| !line.archived);
            return Ok(json!({"entries":entries,"parentAvailable":available}));
        }
        if method == "subagents.history" {
            let request: BoundedChildHistoryRequest = serde_json::from_value(p)?;
            request.validate()?;
            require(
                self.fixture
                    .children
                    .lock()
                    .unwrap()
                    .get(&request.child_session_id)
                    .is_some_and(|(owner, _)| owner == &request.parent_session_id),
                "bounded child owner mismatch",
            )?;
            let sessions = self.fixture.sessions.lock().unwrap();
            let child = &sessions[&request.child_session_id];
            let cut = request
                .through_sequence
                .unwrap_or(child.rows.len() as i64 - 1);
            require(cut < child.rows.len() as i64, "bounded fixture cut")?;
            let revision = format!("{:064x}", cut + 1);
            if request
                .history_revision
                .as_ref()
                .is_some_and(|expected| expected != &revision)
            {
                return Err(Error::Remote(json!({"code":"history-stale"})));
            }
            let end = request
                .before_sequence
                .unwrap_or(cut + 1)
                .min(cut + 1)
                .max(0) as usize;
            let start = end.saturating_sub(request.max_messages);
            return Ok(serde_json::to_value(BoundedChildHistoryPage {
                parent_session_id: request.parent_session_id,
                child_session_id: request.child_session_id,
                through_sequence: cut,
                history_revision: revision,
                entries: child.rows[start..end].to_vec(),
                has_more_before: start > 0,
                run_open: child.busy,
                source_call: None,
            })?);
        }
        let (method, p) = if matches!(method, "subagents.prompt" | "subagents.cancel") {
            let parent = string(&p, "parentSessionId")?;
            let child = string(&p, "childSessionId")?;
            require(
                self.fixture.parent_available.load(Ordering::SeqCst)
                    && self
                        .fixture
                        .children
                        .lock()
                        .unwrap()
                        .get(child)
                        .is_some_and(|(owner, mode)| owner == parent && mode == "continuable"),
                "child control ownership or continuation unavailable",
            )?;
            let mut payload = p.clone();
            payload["sessionId"] = json!(child);
            (
                if method == "subagents.prompt" {
                    "session.prompt"
                } else {
                    "session.cancel"
                },
                payload,
            )
        } else {
            (method, p)
        };
        let id = string(&p, "sessionId")?.to_owned();
        let mut sessions = self.fixture.sessions.lock().unwrap();
        if self.fixture.lifecycle
            && matches!(
                method,
                "session.rename" | "session.fork" | "session.archive" | "session.unarchive"
            )
            && let Some((old_method, old_payload, result)) =
                self.fixture.mutations.lock().unwrap().get(rpc)
        {
            require(
                old_method == method && old_payload == &p,
                "fixture RPC identity payload changed",
            )?;
            return Ok(result.clone());
        }
        let line = sessions
            .get_mut(&id)
            .ok_or_else(|| unsupported("fixture session"))?;
        if method == "session.models" && self.fixture.models {
            return Ok(
                json!({"current":line.model_selection,"routable":true,"routableProviders":["fixture"],"groups":[{"id":"fixture","name":"Fixture provider","models":[{"id":"initial","name":"初始模型"},{"id":"chosen","name":"显式选择模型"}]},{"id":"unroutable","name":"不可路由","models":[{"id":"disabled","name":"不可用模型"}]}],"failures":[]}),
            );
        }
        if method == "session.selectModel" && self.fixture.models {
            require(
                p["provider"] == "fixture"
                    && matches!(p["model"].as_str(), Some("initial" | "chosen")),
                "fixture model unavailable",
            )?;
            line.model_selection = json!({"provider":p["provider"],"model":p["model"]});
            if let Some(effort) = p["reasoningEffort"].as_str() {
                line.model_selection["reasoningEffort"] = json!(effort);
            }
            let selection = line.model_selection.clone();
            append(line, "model/selection", selection.clone());
            return Ok(json!({"selected":selection}));
        }
        if method == "session.rename" && self.fixture.lifecycle {
            let title = string(&p, "title")?.trim();
            require(!title.is_empty(), "empty fixture title")?;
            line.title = title.into();
            if !line.ephemeral {
                let _ = self
                    .fixture
                    .inventory_sender
                    .send(json!({"type":"upsert","session":summary(&id,line)}));
            }
            return Ok(self.fixture.record(method, &p, rpc, json!({"title":title})));
        }
        if matches!(method, "session.archive" | "session.unarchive") && self.fixture.lifecycle {
            require(!line.busy, "fixture archive requires settled run")?;
            line.archived = method == "session.archive";
            let _ = self
                .fixture
                .inventory_sender
                .send(json!({"type":"upsert","session":summary(&id,line)}));
            let ids: Vec<_> = sessions
                .iter()
                .filter(|(_, line)| line.archived)
                .map(|(id, _)| id)
                .collect();
            let _ = self
                .fixture
                .workspace_sender
                .send(json!({"type":"archived","sessionIds":ids}));
            return Ok(self.fixture.record(method, &p, rpc, json!({"ok":true})));
        }
        if method == "session.fork" && (self.fixture.side_chats || self.fixture.lifecycle) {
            let ephemeral = p["ephemeral"].as_bool().unwrap_or(false);
            let end = if let Some(sequence) = p["atSeq"].as_u64() {
                (sequence as usize + 1).min(line.rows.len())
            } else if line.busy {
                line.rows
                    .iter()
                    .rposition(|row| row.event.kind == "turn/end")
                    .map_or(0, |index| index + 1)
            } else {
                line.rows.len()
            };
            let rows = line.rows[..end].to_vec();
            let turn = line.turn;
            let title = line.title.clone();
            let next = self.fixture.forks.fetch_add(1, Ordering::SeqCst) + 1;
            let child = format!("{}-{next}", if ephemeral { "scratch" } else { "fork" });
            require(
                !sessions.contains_key(&child),
                "fixture fork identity collision",
            )?;
            sessions.insert(
                child.clone(),
                Line {
                    rows,
                    sender: broadcast::channel(256).0,
                    cancel: Arc::new(AtomicBool::new(false)),
                    busy: false,
                    turn,
                    ephemeral,
                    title,
                    archived: false,
                    model_selection: json!({"provider":"fixture","model":"initial"}),
                },
            );
            if !ephemeral {
                let _ = self
                    .fixture
                    .inventory_sender
                    .send(json!({"type":"upsert","session":summary(&child,&sessions[&child])}));
            }
            let result = json!({"sessionId":child});
            return Ok(if self.fixture.lifecycle {
                self.fixture.record(method, &p, rpc, result)
            } else {
                result
            });
        }
        if method == "session.discard" && self.fixture.side_chats {
            require(line.ephemeral, "fixture only discards ephemeral sessions")?;
            if line.busy {
                return Err(Error::Remote(json!({"code":"session-running"})));
            }
            sessions.remove(&id);
            return Ok(json!({"discarded":true}));
        }
        if method == "session.cancel" {
            line.cancel.store(true, Ordering::SeqCst);
            return Ok(json!({"cancelled":true}));
        }
        require(method == "session.prompt", "fixture method")?;
        require(!line.busy, "fixture is busy; one active input only")?;
        let content = p["content"].clone();
        line.busy = true;
        line.turn += 1;
        line.cancel = Arc::new(AtomicBool::new(false));
        let cancel = line.cancel.clone();
        let turn = line.turn;
        append(line, "turn/start", json!({"turn":turn}));
        append(
            line,
            "user/message",
            json!({"turn":turn,"id":rpc,"source":{"kind":"user","rpcId":rpc},"content":content}),
        );
        let fixture = self.fixture.clone();
        tokio::spawn(async move {
            let mut text = String::new();
            for chunk in [
                "你好，",
                "这是 ",
                "AgentKit ",
                "通过 SEP ",
                "逐段回传的 ",
                "中文响应。",
                "\n可在回传中切换会话、向上阅读或停止。",
            ] {
                tokio::time::sleep(Duration::from_millis(200)).await;
                if cancel.load(Ordering::SeqCst) {
                    break;
                }
                text.push_str(chunk);
                let sessions = fixture.sessions.lock().unwrap();
                let line = &sessions[&id];
                let _=line.sender.send(json!({"type":"transient","event":{"seq":0,"time":1,"type":"assistant/chunk","data":{"turn":turn,"step":1,"chunk":{"type":"text-delta","text":chunk}}}}));
            }
            let mut sessions = fixture.sessions.lock().unwrap();
            let line = sessions.get_mut(&id).unwrap();
            append(
                line,
                "assistant/message",
                json!({"turn":turn,"step":1,"message":{"id":format!("{id}-m{turn}"),"content":[{"type":"text","text":text}]}}),
            );
            append(
                line,
                "turn/end",
                json!({"turn":turn,"reason":if cancel.load(Ordering::SeqCst){"interrupted"}else{"completed"}}),
            );
            line.busy = false;
        });
        Ok(json!({"accepted":true}))
    }
    async fn close(&self) {}
}
pub async fn setup() -> Result<(Arc<SessionRuntime>, Arc<RunController>)> {
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(Fixture::default()),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await?;
    let controller = Arc::new(RunController::new(runtime.clone())?);
    controller.open(RunScope::main("fixture", "a")).await?;
    tokio::time::sleep(Duration::from_millis(40)).await;
    Ok((runtime, controller))
}
