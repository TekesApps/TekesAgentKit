//! Pi 1.3 native stdio RPC adapter. Native session-tree records never masquerade as journal events.
mod files;
mod rpc;
use async_trait::async_trait;
use futures_util::{StreamExt, stream};
use rpc::Rpc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tekes_agent_contract::*;
use tokio::{
    io::AsyncWriteExt,
    sync::{Mutex, broadcast},
};

#[derive(Clone)]
pub struct PiConfiguration {
    pub identity: String,
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub environment: BTreeMap<String, String>,
    pub data_root: PathBuf,
    pub extensions: Vec<PathBuf>,
    /// Explicit host authority for resuming existing native Pi session files. No global scan.
    pub session_roots: Vec<PathBuf>,
    pub supported_versions: BTreeSet<String>,
}
impl PiConfiguration {
    pub fn new(executable: PathBuf, data_root: PathBuf) -> Self {
        Self {
            identity: "pi".into(),
            executable,
            arguments: Vec::new(),
            environment: BTreeMap::new(),
            data_root,
            extensions: Vec::new(),
            session_roots: Vec::new(),
            supported_versions: ["1.3.0".into()].into(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Receipt {
    digest: String,
    accepted: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Row {
    id: String,
    cwd: PathBuf,
    file: PathBuf,
    title: Option<String>,
    archived: bool,
    parent: Option<String>,
    ephemeral: bool,
    receipts: BTreeMap<String, Receipt>,
}
#[derive(Default, Serialize, Deserialize)]
struct Manifest {
    sessions: BTreeMap<String, Row>,
}
#[derive(Clone)]
struct PendingActionable {
    session: String,
    native_id: String,
    request: Value,
    value: Value,
    rpc: std::sync::Weak<Rpc>,
}
struct Context {
    config: PiConfiguration,
    manifest: Mutex<Manifest>,
    processes: Mutex<BTreeMap<String, Arc<Rpc>>>,
    actionables: Arc<Mutex<BTreeMap<String, PendingActionable>>>,
    notifications: broadcast::Sender<Value>,
}
#[derive(Clone)]
pub struct PiConnector {
    context: Arc<Context>,
}
impl PiConnector {
    pub async fn new(mut config: PiConfiguration) -> Result<Self> {
        require(
            config.executable.is_absolute() && config.data_root.is_absolute(),
            "Pi executable/data root must be explicit absolute paths",
        )?;
        tokio::fs::create_dir_all(config.data_root.join("sessions"))
            .await
            .map_err(io_error)?;
        tokio::fs::create_dir_all(config.data_root.join("agent"))
            .await
            .map_err(io_error)?;
        config.data_root = config.data_root.canonicalize().map_err(io_error)?;
        for root in &mut config.session_roots {
            require(root.is_absolute(), "Pi session roots must be absolute")?;
            *root = root.canonicalize().map_err(io_error)?;
            require(root.is_dir(), "Pi session root must be directory")?;
        }
        let file = config.data_root.join("agentkit-sessions.json");
        let manifest: Manifest = match tokio::fs::read(&file).await {
            Ok(bytes) => {
                require(bytes.len() <= 16 * 1024 * 1024, "Pi registry exceeds limit")?;
                serde_json::from_slice(&bytes)?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Manifest::default(),
            Err(e) => return Err(io_error(e)),
        };
        for (id, row) in &manifest.sessions {
            require(
                id == &row.id
                    && uuid::Uuid::parse_str(id).is_ok()
                    && row.cwd.is_absolute()
                    && row.file.is_absolute()
                    && !row
                        .file
                        .components()
                        .any(|c| matches!(c, std::path::Component::ParentDir))
                    && (row.file.starts_with(config.data_root.join("sessions"))
                        || config
                            .session_roots
                            .iter()
                            .any(|root| row.file.starts_with(root))),
                "invalid owned Pi session pointer",
            )?;
        }
        let (notifications, _) = broadcast::channel(1024);
        Ok(Self {
            context: Arc::new(Context {
                config,
                manifest: Mutex::new(manifest),
                processes: Mutex::new(BTreeMap::new()),
                notifications,
                actionables: Arc::new(Mutex::new(BTreeMap::new())),
            }),
        })
    }
}
fn io_error(error: std::io::Error) -> Error {
    Error::Transport(error.to_string())
}
impl Context {
    async fn row(&self, id: &str) -> Result<Row> {
        self.manifest
            .lock()
            .await
            .sessions
            .get(id)
            .cloned()
            .ok_or_else(|| {
                Error::Remote(
                    json!({"code":"session-not-found","message":"Unknown owned Pi session"}),
                )
            })
    }
    async fn save(&self, manifest: &Manifest) -> Result<()> {
        let temporary = self
            .config
            .data_root
            .join(format!(".registry-{}.tmp", uuid::Uuid::new_v4()));
        let bytes = serde_json::to_vec(manifest)?;
        let mut options = tokio::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&temporary).await.map_err(io_error)?;
        file.write_all(&bytes).await.map_err(io_error)?;
        file.sync_all().await.map_err(io_error)?;
        drop(file);
        tokio::fs::rename(
            &temporary,
            self.config.data_root.join("agentkit-sessions.json"),
        )
        .await
        .map_err(io_error)?;
        let directory = self.config.data_root.clone();
        tokio::task::spawn_blocking(move || std::fs::File::open(directory)?.sync_all())
            .await
            .map_err(|_| Error::Disconnected)?
            .map_err(io_error)
    }
    async fn process(self: &Arc<Self>, id: &str) -> Result<Arc<Rpc>> {
        let row = self.row(id).await?;
        require(!row.archived, "session is archived")?;
        let mut processes = self.processes.lock().await;
        if let Some(rpc) = processes.get(id).filter(|rpc| rpc.alive()) {
            return Ok(rpc.clone());
        }
        let rpc = Rpc::start(&self.config, &row.cwd, Some(&row.file)).await?;
        let mut events = rpc.events();
        let sender = self.notifications.clone();
        let id_owned = id.to_owned();
        let pending_actionables = self.actionables.clone();
        let weak_rpc = Arc::downgrade(&rpc);
        tokio::spawn(async move {
            while let Ok(event) = events.recv().await {
                if event["type"] == "extension_ui_request"
                    && matches!(
                        event["method"].as_str(),
                        Some("confirm" | "select" | "input" | "editor")
                    )
                    && let Some(native_id) = event["id"].as_str()
                {
                    let key = format!("{id_owned}:{native_id}");
                    let value = actionable(&event, &id_owned);
                    pending_actionables.lock().await.insert(
                        key,
                        PendingActionable {
                            session: id_owned.clone(),
                            native_id: native_id.into(),
                            request: event.clone(),
                            value: value.clone(),
                            rpc: weak_rpc.clone(),
                        },
                    );
                    let _ = sender.send(json!({"actionable": value}));
                }
                let _ = sender.send(json!({"sessionId":id_owned,"event":event}));
                if event["type"] == "agentkit/transport-closed" {
                    let mut pending = pending_actionables.lock().await;
                    let ids: Vec<_> = pending
                        .iter()
                        .filter(|(_, item)| item.session == id_owned)
                        .map(|(id, _)| id.clone())
                        .collect();
                    for id in ids {
                        pending.remove(&id);
                        let _ = sender.send(json!({"resolved":id}));
                    }
                    break;
                }
            }
        });
        // Formal RPC readiness, rather than assuming the spawned process is usable.
        rpc.request("get_state", json!({})).await?;
        processes.insert(id.into(), rpc.clone());
        Ok(rpc)
    }
    async fn summaries(&self) -> Vec<Value> {
        self.manifest
            .lock()
            .await
            .sessions
            .values()
            .filter(|row| !row.archived)
            .map(summary)
            .collect()
    }
    async fn notify_row(&self, id: &str) {
        if let Ok(row) = self.row(id).await {
            let _ = self.notifications.send(json!({"summary":summary(&row)}));
        }
    }
}
fn summary(row: &Row) -> Value {
    let updated = std::fs::metadata(&row.file)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
        * 1000.0;
    json!({"sessionId":row.id,"updatedAt":updated,"running":false,"blank":!row.file.exists(),"cwd":row.cwd,"parentSessionId":row.parent,"ephemeral":row.ephemeral,"projections":{"asOfSeq":0,"values":{"title":row.title}}})
}
#[async_trait]
impl EndpointConnector for PiConnector {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        let c = &self.context.config;
        let mut command = tokio::process::Command::new(&c.executable);
        command
            .args(&c.arguments)
            .arg("--version")
            .env_clear()
            .envs(&c.environment)
            .kill_on_drop(true);
        let output = tokio::time::timeout(Duration::from_secs(15), command.output())
            .await
            .map_err(|_| Error::Timeout)?
            .map_err(io_error)?;
        require(
            output.status.success() && output.stdout.len() <= 65536,
            "Pi version probe failed",
        )?;
        let version = String::from_utf8(output.stdout)
            .map_err(|_| Error::Protocol("invalid Pi version".into()))?
            .trim()
            .to_owned();
        require(
            c.supported_versions.contains(&version),
            "unsupported Pi runtime version",
        )?;
        let host = HostDescription {
            protocol_version: 3,
            product: Product {
                name: "Pi".into(),
                version,
            },
            capabilities: [
                "session-inventory-sync",
                "projected-stream",
                "session-control-sync",
                "actionable-sync",
                "models",
                "management",
            ]
            .map(String::from)
            .into(),
            cwd: c.data_root.to_string_lossy().into(),
            home: c.environment.get("HOME").cloned().unwrap_or_default(),
            provider: None,
            model: None,
            attached_sessions: 0,
            can_open_path: false,
        };
        Ok(Arc::new(PiSessionEndpoint {
            context: self.context.clone(),
            generation: uuid::Uuid::new_v4().to_string(),
            host,
            windows: Arc::new(Mutex::new(BTreeMap::new())),
        }))
    }
}
pub struct PiSessionEndpoint {
    context: Arc<Context>,
    generation: String,
    host: HostDescription,
    windows: Arc<Mutex<BTreeMap<String, usize>>>,
}
impl PiSessionEndpoint {
    async fn conversation(&self, id: &str, initial: bool) -> Result<Value> {
        let rpc = self.context.process(id).await?;
        conversation_frame(
            &rpc,
            id,
            &self.generation,
            *self.windows.lock().await.get(id).unwrap_or(&50),
            initial,
        )
        .await
    }
}
async fn conversation_frame(
    rpc: &Rpc,
    id: &str,
    generation: &str,
    limit: usize,
    initial: bool,
) -> Result<Value> {
    let entries = rpc.request("get_entries", json!({})).await?;
    let state = rpc.request("get_state", json!({})).await?;
    let leaf_id = entries["leafId"].as_str().map(String::from);
    let (entries, more) = active_branch(&entries, limit)?;
    let snapshot = NativeConversationSnapshot {
        session_id: id.into(),
        generation: generation.into(),
        leaf_id,
        entries,
        has_more_before: more,
        is_streaming: boolean(&state, "isStreaming")?,
        state,
    };
    snapshot.validate()?;
    Ok(
        json!({"type":if initial {"conversation"} else {"conversation-update"},"generation":generation,"snapshot":snapshot}),
    )
}
/// Reconstruct active ancestry; get_entries includes inactive branches, which are not current conversation.
pub fn active_branch(result: &Value, max_messages: usize) -> Result<(Vec<Value>, bool)> {
    require(max_messages > 0, "invalid conversation window")?;
    let all = array(result, "entries")?;
    let mut by_id = BTreeMap::new();
    for entry in all {
        let id = string(entry, "id")?;
        require(
            by_id.insert(id, entry).is_none(),
            "duplicate Pi native entry",
        )?;
    }
    let mut cursor = result.get("leafId").and_then(Value::as_str);
    let mut seen = BTreeSet::new();
    let mut branch = Vec::new();
    while let Some(id) = cursor {
        require(seen.insert(id), "Pi session ancestry cycle")?;
        let entry = by_id
            .get(id)
            .ok_or_else(|| Error::Protocol("missing Pi ancestor".into()))?;
        branch.push((*entry).clone());
        cursor = entry.get("parentId").and_then(Value::as_str);
    }
    branch.reverse();
    let messages: Vec<_> = branch
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry["type"] == "message")
        .map(|(i, _)| i)
        .collect();
    if messages.len() <= max_messages {
        return Ok((branch, false));
    }
    let start = messages[messages.len() - max_messages];
    Ok((branch[start..].to_vec(), true))
}
#[async_trait]
impl SessionEndpoint for PiSessionEndpoint {
    fn identity(&self) -> &str {
        &self.context.config.identity
    }
    fn host(&self) -> &HostDescription {
        &self.host
    }
    fn content_kind(&self) -> SessionContentKind {
        SessionContentKind::NativeConversation
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        let generation = self.generation.clone();
        match target {
            StreamTarget::SessionJournal { .. } => {
                Err(unsupported("Pi uses native conversation, not journal"))
            }
            StreamTarget::SessionConversation {
                address,
                max_messages,
            } => {
                require(max_messages > 0, "invalid maxMessages")?;
                let id = address.session_id;
                let rpc = self.context.process(&id).await?;
                let events = rpc.events();
                self.windows.lock().await.insert(id.clone(), max_messages);
                let baseline = self.conversation(&id, true).await?;
                let windows = self.windows.clone();
                let updates = stream::unfold(
                    (events, rpc, id, generation, windows),
                    |(mut events, rpc, id, generation, windows)| async move {
                        let event = match events.recv().await {
                            Ok(v) => v,
                            Err(_) => {
                                return Some((
                                    Err(Error::Disconnected),
                                    (events, rpc, id, generation, windows),
                                ));
                            }
                        };
                        let kind = event["type"].as_str().unwrap_or("");
                        let value = if kind == "agentkit/transport-closed" {
                            Err(Error::Disconnected)
                        } else if matches!(
                            kind,
                            "agent_start"
                                | "agent_settled"
                                | "message_end"
                                | "session_switch"
                                | "session_fork"
                                | "auto_compaction_end"
                                | "agentkit/refresh"
                        ) {
                            let limit = *windows.lock().await.get(&id).unwrap_or(&50);
                            conversation_frame(&rpc, &id, &generation, limit, false).await
                        } else {
                            Ok(
                                json!({"type":"native-event","generation":generation,"sessionId":id,"event":event}),
                            )
                        };
                        Some((value, (events, rpc, id, generation, windows)))
                    },
                );
                Ok(Box::pin(
                    stream::once(async { Ok(baseline) }).chain(updates),
                ))
            }
            target => {
                let events = self.context.notifications.subscribe();
                let context = self.context.clone();
                let baseline = match &target {
                    StreamTarget::SessionInventory => {
                        json!({"type":"baseline","generation":generation,"snapshot":{"items":context.summaries().await}})
                    }
                    StreamTarget::SessionControl => {
                        json!({"type":"baseline","generation":generation,"snapshot":{"queues":[],"jobs":[],"projections":[]}})
                    }
                    StreamTarget::Actionables => json!({"type":"reset","generation":generation}),
                    _ => unreachable!(),
                };
                let mut initial = vec![baseline];
                if target == StreamTarget::Actionables {
                    initial.extend(context.actionables.lock().await.values().map(|item| json!({"type":"upsert","generation":generation,"actionable":item.value})));
                }
                let updates = stream::unfold(
                    (events, target, generation),
                    |(mut events, target, generation)| async move {
                        loop {
                            let event = match events.recv().await {
                                Ok(v) => v,
                                Err(_) => {
                                    return Some((
                                        Err(Error::Disconnected),
                                        (events, target, generation),
                                    ));
                                }
                            };
                            let frame = match &target {
                                StreamTarget::SessionInventory
                                    if event.get("removed").is_some() =>
                                {
                                    Some(
                                        json!({"type":"remove","generation":generation,"sessionId":event["removed"]}),
                                    )
                                }
                                StreamTarget::SessionInventory
                                    if event.get("summary").is_some() =>
                                {
                                    Some(
                                        json!({"type":"upsert","generation":generation,"session":event["summary"]}),
                                    )
                                }
                                StreamTarget::SessionInventory
                                    if matches!(
                                        event["event"]["type"].as_str(),
                                        Some("agent_start" | "agent_settled")
                                    ) =>
                                {
                                    Some(
                                        json!({"type":"status","generation":generation,"sessionId":event["sessionId"],"running":event["event"]["type"]=="agent_start"}),
                                    )
                                }
                                StreamTarget::Actionables if event.get("actionable").is_some() => {
                                    Some(
                                        json!({"type":"upsert","generation":generation,"actionable":event["actionable"]}),
                                    )
                                }
                                StreamTarget::Actionables if event.get("resolved").is_some() => {
                                    Some(
                                        json!({"type":"resolved","generation":generation,"actionableId":event["resolved"],"revision":0}),
                                    )
                                }
                                _ => None,
                            };
                            if let Some(frame) = frame {
                                return Some((Ok(frame), (events, target, generation)));
                            }
                        }
                    },
                );
                Ok(Box::pin(
                    stream::iter(initial.into_iter().map(Ok)).chain(updates),
                ))
            }
        }
    }
    async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
        Err(unsupported("Pi has native ancestry pagination"))
    }
    async fn load_earlier_conversation(&self, id: &str, count: usize) -> Result<bool> {
        let rpc = self.context.process(id).await?;
        let mut windows = self.windows.lock().await;
        let limit = windows
            .get_mut(id)
            .ok_or_else(|| unsupported("conversation is not open"))?;
        let (_, more) = active_branch(&rpc.request("get_entries", json!({})).await?, *limit)?;
        if more {
            *limit = limit.saturating_add(count);
        }
        drop(windows);
        if more {
            rpc.refresh();
        }
        // Refresh through native RPC event listener is driven explicitly by follow's refresh source below.
        Ok(more)
    }
    async fn locator(&self, id: &str) -> Result<SessionLocator> {
        let row = self.context.row(id).await?;
        let state = self
            .context
            .process(id)
            .await?
            .request("get_state", json!({}))
            .await?;
        Ok(SessionLocator {
            address: SessionAddress {
                endpoint_id: self.identity().into(),
                session_id: id.into(),
            },
            harness: "pi".into(),
            cwd: Some(row.cwd.to_string_lossy().into()),
            working_directory: Some(row.cwd.to_string_lossy().into()),
            native_session_file: state["sessionFile"]
                .as_str()
                .map(String::from)
                .or_else(|| Some(row.file.to_string_lossy().into())),
            native_session_id: state["sessionId"].as_str().map(String::from),
            history_source: "Pi get_entries active ancestry".into(),
        })
    }
    async fn file_stat(&self, id: &str, path: &str) -> Result<SessionFileStat> {
        let row = self.context.row(id).await?;
        let path = path.to_owned();
        tokio::task::spawn_blocking(move || {
            let page = files::read(&row.cwd, &path, 0, 1)?;
            Ok(SessionFileStat {
                absolute_path: page.absolute_path,
                version: page.version,
                bytes: page.bytes,
            })
        })
        .await
        .map_err(|_| Error::Disconnected)?
    }
    async fn file_read_bytes(
        &self,
        id: &str,
        path: &str,
        offset: u64,
        length: usize,
    ) -> Result<SessionFileBytePage> {
        let row = self.context.row(id).await?;
        let path = path.to_owned();
        tokio::task::spawn_blocking(move || files::read(&row.cwd, &path, offset, length))
            .await
            .map_err(|_| Error::Disconnected)?
    }
    async fn file_read_text(
        &self,
        id: &str,
        path: &str,
        offset: u64,
        limit: usize,
    ) -> Result<SessionFileTextPage> {
        let row = self.context.row(id).await?;
        let path = path.to_owned();
        tokio::task::spawn_blocking(move || files::text(&row.cwd, &path, offset, limit))
            .await
            .map_err(|_| Error::Disconnected)?
    }
    async fn respond(&self, id: &str, revision: i64, outcome: Value) -> Result<()> {
        require(revision == 0, "stale Pi question revision")?;
        let item = self
            .context
            .actionables
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| unsupported("Pi actionable is no longer pending"))?;
        let rpc = item
            .rpc
            .upgrade()
            .filter(|rpc| rpc.alive())
            .ok_or(Error::Disconnected)?;
        rpc.ui_response(
            &item.native_id,
            actionable_response(&outcome, &item.request)?,
        )?;
        self.context.actionables.lock().await.remove(id);
        let _ = self.context.notifications.send(json!({"resolved":id}));
        Ok(())
    }
    async fn call(&self, method: &str, payload: Value, rpc_id: &str) -> Result<Value> {
        match method {
            "session.create" => {
                require(
                    payload.get("execution").is_none(),
                    "Kernel execution configuration cannot authorize a Pi session",
                )?;
                let cwd = payload["cwd"]
                    .as_str()
                    .ok_or_else(|| unsupported("explicit Pi cwd required"))?;
                let cwd = Path::new(cwd).canonicalize().map_err(io_error)?;
                require(cwd.is_dir(), "cwd must be directory")?;
                let id = payload["sessionId"]
                    .as_str()
                    .map(String::from)
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                require(
                    uuid::Uuid::parse_str(&id).is_ok(),
                    "invalid Pi session identity",
                )?;
                let mut manifest = self.context.manifest.lock().await;
                if let Some(row) = manifest.sessions.get(&id) {
                    require(row.cwd == cwd, "session cwd conflict")?;
                    return Ok(json!({"sessionId":id,"created":false}));
                }
                manifest.sessions.insert(
                    id.clone(),
                    Row {
                        id: id.clone(),
                        cwd,
                        file: self
                            .context
                            .config
                            .data_root
                            .join("sessions")
                            .join(format!("{id}.jsonl")),
                        title: None,
                        archived: false,
                        parent: None,
                        ephemeral: false,
                        receipts: BTreeMap::new(),
                    },
                );
                self.context.save(&manifest).await?;
                drop(manifest);
                self.context.notify_row(&id).await;
                Ok(json!({"sessionId":id,"created":true}))
            }
            "session.adopt" => {
                let cwd = Path::new(string(&payload, "cwd")?)
                    .canonicalize()
                    .map_err(io_error)?;
                require(cwd.is_dir(), "cwd must be directory")?;
                let file = Path::new(string(&payload, "sessionFile")?)
                    .canonicalize()
                    .map_err(io_error)?;
                require(
                    file.is_file()
                        && self
                            .context
                            .config
                            .session_roots
                            .iter()
                            .any(|root| file.starts_with(root)),
                    "native session file requires explicit host session root",
                )?;
                let mut manifest = self.context.manifest.lock().await;
                if let Some(row) = manifest.sessions.values().find(|row| row.file == file) {
                    require(row.cwd == cwd, "session cwd conflict")?;
                    return Ok(json!({"sessionId":row.id,"created":false}));
                }
                let id = payload["sessionId"]
                    .as_str()
                    .map(String::from)
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                require(
                    uuid::Uuid::parse_str(&id).is_ok() && !manifest.sessions.contains_key(&id),
                    "invalid or conflicting session identity",
                )?;
                // Native RPC validates the file. AgentKit never decodes a private session log to create history.
                let rpc = Rpc::start(&self.context.config, &cwd, Some(&file)).await?;
                let state = rpc.request("get_state", json!({})).await;
                rpc.close();
                let state = state?;
                let actual = Path::new(string(&state, "sessionFile")?)
                    .canonicalize()
                    .map_err(io_error)?;
                require(actual == file, "Pi resumed a different native session file")?;
                manifest.sessions.insert(
                    id.clone(),
                    Row {
                        id: id.clone(),
                        cwd,
                        file,
                        title: state["sessionName"].as_str().map(String::from),
                        archived: false,
                        parent: None,
                        ephemeral: false,
                        receipts: BTreeMap::new(),
                    },
                );
                self.context.save(&manifest).await?;
                drop(manifest);
                self.context.notify_row(&id).await;
                Ok(json!({"sessionId":id,"created":true}))
            }
            "models.list" => {
                let rpc =
                    Rpc::start(&self.context.config, &self.context.config.data_root, None).await?;
                let result = rpc.request("get_available_models", json!({})).await;
                rpc.close();
                result
            }
            "session.listArchived" => Ok(
                json!({"items":self.context.manifest.lock().await.sessions.values().filter(|row|row.archived).map(summary).collect::<Vec<_>>()}),
            ),
            "session.archive" | "session.unarchive" => {
                let id = string(&payload, "sessionId")?;
                self.context.row(id).await?;
                let archived = method == "session.archive";
                if archived && let Some(rpc) = self.context.processes.lock().await.remove(id) {
                    rpc.close();
                }
                let mut manifest = self.context.manifest.lock().await;
                manifest.sessions.get_mut(id).expect("owned row").archived = archived;
                self.context.save(&manifest).await?;
                drop(manifest);
                if archived {
                    let _ = self.context.notifications.send(json!({"removed":id}));
                } else {
                    self.context.notify_row(id).await;
                }
                Ok(json!({"sessionId":id,"archived":archived}))
            }
            _ => {
                let id = string(&payload, "sessionId")?;
                let rpc = self.context.process(id).await?;
                match method {
                    "session.models" => rpc.request("get_available_models", json!({})).await,
                    "session.selectModel" => {
                        rpc.request(
                            "set_model",
                            json!({
                                "provider":string(&payload,"provider")?,
                                "modelId":string(&payload,"model")?
                            }),
                        )
                        .await
                    }
                    "session.cancel" => rpc.request("abort", json!({})).await,
                    "session.rename" => {
                        let title = string(&payload, "title")?;
                        rpc.request("set_session_name", json!({"name":title}))
                            .await?;
                        let mut manifest = self.context.manifest.lock().await;
                        manifest.sessions.get_mut(id).expect("owned row").title =
                            Some(title.into());
                        self.context.save(&manifest).await?;
                        drop(manifest);
                        self.context.notify_row(id).await;
                        Ok(json!({"sessionId":id,"title":title}))
                    }
                    "session.prompt" => {
                        require(!rpc_id.is_empty(), "prompt rpcId required")?;
                        let mut text = Vec::new();
                        for item in array(&payload, "content")? {
                            require(
                                item["type"] == "text",
                                "Pi prompt currently requires text content",
                            )?;
                            text.push(string(item, "text")?);
                        }
                        let message = text.join("\n");
                        let digest =
                            format!("{:x}", Sha256::digest(payload.to_string().as_bytes()));
                        let mut manifest = self.context.manifest.lock().await;
                        let row = manifest.sessions.get_mut(id).expect("owned row");
                        if let Some(receipt) = row.receipts.get(rpc_id) {
                            require(receipt.digest == digest, "prompt identity content conflict")?;
                            return if receipt.accepted {
                                Ok(json!({"accepted":true,"duplicate":true}))
                            } else {
                                Err(unsupported(
                                    "uncertain Pi prompt; reconcile native history before retry",
                                ))
                            };
                        }
                        row.receipts.insert(
                            rpc_id.into(),
                            Receipt {
                                digest,
                                accepted: false,
                            },
                        );
                        self.context.save(&manifest).await?;
                        drop(manifest);
                        let result=rpc.request("prompt",json!({
                            "message":message,
                            "streamingBehavior":if payload["mode"]=="steer" {"steer"} else {"followUp"}
                        })).await?;
                        let disposition = result["disposition"].as_str().unwrap_or("");
                        require(
                            matches!(disposition, "started" | "queued" | "handled"),
                            "Pi did not admit prompt",
                        )?;
                        let mut manifest = self.context.manifest.lock().await;
                        manifest
                            .sessions
                            .get_mut(id)
                            .expect("owned row")
                            .receipts
                            .get_mut(rpc_id)
                            .expect("receipt")
                            .accepted = true;
                        self.context.save(&manifest).await?;
                        Ok(json!({"accepted":true,"native":result}))
                    }
                    "session.history.export" => rpc.request("get_entries", json!({})).await,
                    "pi.getState" => rpc.request("get_state", json!({})).await,
                    "pi.getTree" => rpc.request("get_tree", json!({})).await,
                    "pi.getMessages" => rpc.request("get_messages", json!({})).await,
                    _ => Err(unsupported(&format!("Pi method {method}"))),
                }
            }
        }
    }
    async fn close(&self) {
        let mut processes = self.context.processes.lock().await;
        for rpc in processes.values() {
            rpc.close();
        }
        processes.clear();
        self.context.actionables.lock().await.clear();
    }
}

fn actionable(record: &Value, session: &str) -> Value {
    let method = record["method"].as_str().unwrap_or("");
    let id = record["id"].as_str().unwrap_or("");
    let payload = if method == "confirm" {
        json!({"sessionId":session,"approvalId":id,"toolName":record["title"].as_str().unwrap_or("Pi confirmation"),"reason":record["message"]})
    } else {
        let options: Vec<_> = record["options"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|v| v.is_string())
            .map(|v| json!({"label":v}))
            .collect();
        json!({"sessionId":session,"questions":[{"id":id,"question":record["title"].as_str().unwrap_or("Pi input"),"detail":record.get("placeholder").or_else(||record.get("prefill")),"options":options,"multiSelect":false,"allowCustom":method!="select","allowNotes":false,"isSecret":false}]})
    };
    json!({"id":format!("{session}:{id}"),"sessionId":session,"revision":0,"kind":if method=="confirm"{"approval"}else{"question"},"payload":payload})
}
fn actionable_response(outcome: &Value, request: &Value) -> Result<Value> {
    if outcome["cancelled"] == true {
        return Ok(json!({"cancelled":true}));
    }
    if request["method"] == "confirm" {
        if let Some(value) = outcome["confirmed"].as_bool() {
            return Ok(json!({"confirmed":value}));
        }
        if let Some(word) = outcome["outcome"]
            .as_str()
            .filter(|word| matches!(*word, "allowed-once" | "rejected"))
        {
            return Ok(json!({"confirmed":word=="allowed-once"}));
        }
        return Err(unsupported("invalid Pi confirmation answer"));
    }
    let answer = outcome.get("answer").unwrap_or(outcome);
    let row = answer["answers"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["id"] == request["id"]));
    let value = outcome["value"]
        .as_str()
        .or_else(|| {
            row.and_then(|row| {
                row["custom"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .or_else(|| {
                        row["selected"]
                            .as_array()
                            .and_then(|v| v.first())
                            .and_then(Value::as_str)
                    })
            })
        })
        .ok_or_else(|| unsupported("missing Pi answer"))?;
    if request["method"] == "select" {
        require(
            request["options"]
                .as_array()
                .is_some_and(|options| options.iter().any(|option| option == value)),
            "invalid Pi selection",
        )?;
    }
    Ok(json!({"value":value}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn active_branch_excludes_siblings_and_pages_by_messages() {
        let tree = json!({"leafId":"c","entries":[
            {"id":"root","parentId":null,"type":"custom"},
            {"id":"a","parentId":"root","type":"message"},
            {"id":"sibling","parentId":"a","type":"message"},
            {"id":"b","parentId":"a","type":"message"},
            {"id":"c","parentId":"b","type":"custom"}
        ]});
        let (entries, more) = active_branch(&tree, 1).unwrap();
        assert!(more);
        assert_eq!(
            entries
                .iter()
                .map(|e| e["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["b", "c"]
        );
        assert_eq!(active_branch(&tree, 10).unwrap().0.len(), 4);
        for bad in [
            json!({"leafId":"a","entries":[{"id":"a","parentId":"a"}]}),
            json!({"leafId":"missing","entries":[]}),
            json!({"entries":[{"id":"a"},{"id":"a"}]}),
        ] {
            assert!(active_branch(&bad, 50).is_err());
        }
    }
    #[test]
    fn native_question_responses_validate_selection_and_confirmation() {
        let request = json!({"method":"select","id":"q","options":["yes","no"]});
        assert_eq!(
            actionable_response(
                &json!({"answer":{"answers":[{"id":"q","selected":["yes"]}]}}),
                &request
            )
            .unwrap(),
            json!({"value":"yes"})
        );
        assert!(actionable_response(&json!({"value":"other"}), &request).is_err());
        assert_eq!(
            actionable_response(&json!({"cancelled":true}), &request).unwrap(),
            json!({"cancelled":true})
        );
        assert!(
            actionable_response(&json!({"confirmed":"true"}), &json!({"method":"confirm"}))
                .is_err()
        );
        assert_eq!(
            actionable_response(
                &json!({"outcome":"allowed-once"}),
                &json!({"method":"confirm"})
            )
            .unwrap(),
            json!({"confirmed":true})
        );
    }
}
