//! Pi 1.3/1.4 native stdio RPC adapter. Native session-tree records never masquerade as journal events.
mod attachments;
mod extensions;
mod files;
mod goal_driver;
mod management;
mod models;
mod permissions;
mod queue;
mod reading;
mod rpc;
mod search;
mod subagents;
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
            supported_versions: ["1.3.0".into(), "1.4.0".into()].into(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Receipt {
    digest: String,
    accepted: bool,
    #[serde(default)]
    queued: bool,
    #[serde(default)]
    cancelled: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Row {
    id: String,
    cwd: PathBuf,
    file: PathBuf,
    title: Option<String>,
    archived: bool,
    parent: Option<String>,
    #[serde(default)]
    subagent_parent: Option<String>,
    #[serde(default)]
    model_selection: Option<Value>,
    ephemeral: bool,
    receipts: BTreeMap<String, Receipt>,
    #[serde(default)]
    materialized: bool,
    #[serde(default)]
    native_session_id: Option<String>,
    #[serde(default)]
    workspace_id: Option<String>,
    #[serde(default)]
    queue: Vec<Value>,
    #[serde(default)]
    control_revision: i64,
    #[serde(default)]
    goal_revision: i64,
    #[serde(default)]
    goal: Option<Value>,
    #[serde(default)]
    feedback: BTreeMap<String, Value>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct Manifest {
    sessions: BTreeMap<String, Row>,
    #[serde(default)]
    child_admissions: BTreeMap<String, subagents::ChildAdmission>,
    #[serde(default)]
    workspaces: BTreeMap<String, Value>,
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
    _root_lock: std::fs::File,
    manifest: Mutex<Manifest>,
    processes: Mutex<BTreeMap<String, Arc<Rpc>>>,
    actionables: Arc<Mutex<BTreeMap<String, PendingActionable>>>,
    notifications: broadcast::Sender<Value>,
    gates: Mutex<BTreeMap<String, Arc<tokio::sync::Semaphore>>>,
    armed_goals: Mutex<BTreeSet<String>>,
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
        let root_lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(config.data_root.join(".agentkit.lock"))
            .map_err(io_error)?;
        root_lock
            .try_lock()
            .map_err(|_| Error::Protocol("Pi data root is owned by another connector".into()))?;
        for root in &mut config.session_roots {
            require(root.is_absolute(), "Pi session roots must be absolute")?;
            *root = root.canonicalize().map_err(io_error)?;
            require(root.is_dir(), "Pi session root must be directory")?;
        }
        let file = config.data_root.join("agentkit-sessions.json");
        tokio::fs::write(
            config.data_root.join("agentkit-reading.mjs"),
            include_str!("../resources/agentkit-reading.mjs"),
        )
        .await
        .map_err(io_error)?;
        tokio::fs::write(
            config.data_root.join("agentkit-input-files.mjs"),
            include_str!("../resources/agentkit-input-files.mjs"),
        )
        .await
        .map_err(io_error)?;
        tokio::fs::write(
            config.data_root.join("agentkit-control.mjs"),
            include_str!("../resources/agentkit-control.mjs"),
        )
        .await
        .map_err(io_error)?;
        let mut manifest: Manifest = match tokio::fs::read(&file).await {
            Ok(bytes) => {
                require(bytes.len() <= 16 * 1024 * 1024, "Pi registry exceeds limit")?;
                serde_json::from_slice(&bytes)?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Manifest::default(),
            Err(e) => return Err(io_error(e)),
        };
        let scratch: Vec<_> = manifest
            .sessions
            .values()
            .filter(|row| row.ephemeral)
            .cloned()
            .collect();
        for row in scratch {
            require(
                row.file.starts_with(config.data_root.join("sessions")),
                "scratch outside owned storage",
            )?;
            if row.file.exists() {
                tokio::fs::remove_file(&row.file).await.map_err(io_error)?;
            }
            let uploads = config
                .data_root
                .join("uploads")
                .join(format!("{:x}", Sha256::digest(row.id.as_bytes())));
            if uploads.exists() {
                tokio::fs::remove_dir_all(uploads).await.map_err(io_error)?;
            }
            manifest.sessions.remove(&row.id);
        }
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
                _root_lock: root_lock,
                manifest: Mutex::new(manifest),
                processes: Mutex::new(BTreeMap::new()),
                notifications,
                gates: Mutex::new(BTreeMap::new()),
                armed_goals: Mutex::new(BTreeSet::new()),
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
    async fn validate_pointer(&self, row: &Row) -> Result<()> {
        match tokio::fs::symlink_metadata(&row.file).await {
            Ok(metadata) => {
                require(
                    metadata.is_file() && !metadata.file_type().is_symlink(),
                    "invalid native session file",
                )?;
                let actual = row.file.canonicalize().map_err(io_error)?;
                require(actual == row.file, "native session pointer was redirected")
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => require(
                !row.materialized
                    && row.native_session_id.is_none()
                    && row.parent.is_none()
                    && row.goal_revision == 0
                    && row.feedback.is_empty()
                    && (row.title.is_none() || row.subagent_parent.is_some())
                    && (row.receipts.is_empty() || row.subagent_parent.is_some())
                    && row.file.starts_with(self.config.data_root.join("sessions")),
                "native session file is missing; refusing to create replacement history",
            ),
            Err(error) => Err(io_error(error)),
        }
    }
    fn process<'a>(
        self: &'a Arc<Self>,
        id: &'a str,
    ) -> futures_util::future::BoxFuture<'a, Result<Arc<Rpc>>> {
        Box::pin(async move {
            let row = self.row(id).await?;
            require(!row.archived, "session is archived")?;
            let mut processes = self.processes.lock().await;
            if let Some(rpc) = processes.get(id).filter(|rpc| rpc.alive()) {
                return Ok(rpc.clone());
            }
            self.validate_pointer(&row).await?;
            let permission_file = self.permission_file(&row.id);
            let rpc = Rpc::start(
                &self.config,
                &row.cwd,
                Some(&row.file),
                Some(&permission_file),
            )
            .await?;
            let mut events = rpc.events();
            let readiness = async {
                if !row.materialized && row.subagent_parent.is_some() {
                    if let Some(title) = &row.title {
                        rpc.request("set_session_name", json!({"name":title}))
                            .await?;
                    }
                    if let Some(selection) = &row.model_selection {
                        rpc.request(
                            "set_model",
                            json!({"provider":selection["provider"],"modelId":selection["model"]}),
                        )
                        .await?;
                        if let Some(effort) = selection["reasoningEffort"].as_str() {
                            rpc.request("set_thinking_level", json!({"level":effort}))
                                .await?;
                        }
                    }
                }
                let state = rpc.request("get_state", json!({})).await?;
                let actual = PathBuf::from(string(&state, "sessionFile")?);
                require(
                    actual == row.file,
                    "Pi resumed a different native session file",
                )?;
                let native_id = string(&state, "sessionId")?;
                require(!native_id.is_empty(), "missing native session identity")?;
                if let Some(expected) = &row.native_session_id {
                    require(expected == native_id, "Pi native session identity changed")?;
                }
                // Blank sessions may not have a native file until their first persisted entry.
                if row.file.is_file() {
                    let mut manifest = self.manifest.lock().await;
                    let current = manifest
                        .sessions
                        .get_mut(id)
                        .ok_or_else(|| unsupported("session removed"))?;
                    current.materialized = true;
                    current.native_session_id = Some(native_id.into());
                    self.save(&manifest).await?;
                }
                Ok::<_, Error>(())
            }
            .await;
            if let Err(error) = readiness {
                rpc.close();
                return Err(error);
            }
            let sender = self.notifications.clone();
            let id_owned = id.to_owned();
            let weak_context = Arc::downgrade(self);
            let pending_actionables = self.actionables.clone();
            let weak_rpc = Arc::downgrade(&rpc);
            tokio::spawn(async move {
                while let Ok(event) = events.recv().await {
                    if let Some(rpc) = weak_rpc.upgrade()
                        && let Some(request) = rpc.host_request(&event)
                    {
                        if let Some(context) = weak_context.upgrade() {
                            tokio::spawn(context.handle_host_request(
                                id_owned.clone(),
                                rpc,
                                request,
                            ));
                        }
                        continue;
                    }
                    // Rpc::control consumes these private replies directly;
                    // public event streams already omit them.
                    if event["type"] == "extension_ui_request"
                        && event["method"] == "notify"
                        && event["message"]
                            .as_str()
                            .is_some_and(|text| text.starts_with("agentkit-control/v1:"))
                    {
                        continue;
                    }
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
                    if event["type"] == "agent_settled"
                        && let Some(context) = weak_context.upgrade()
                    {
                        let id = id_owned.clone();
                        let expected = weak_rpc.upgrade();
                        tokio::spawn(async move {
                            if let Some(expected) = expected {
                                let _ = context
                                    .dispatch_next_if_current(&id, Some(expected.clone()))
                                    .await;
                                context.schedule_goal(id, expected);
                            }
                        });
                    }
                    if event["type"] == "agentkit/transport-closed" {
                        let mut pending = pending_actionables.lock().await;
                        let ids: Vec<_> = pending
                            .iter()
                            .filter(|(_, item)| {
                                item.session == id_owned
                                    && std::sync::Weak::ptr_eq(&item.rpc, &weak_rpc)
                            })
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
            // The native file and session identity have been verified before exposing this process.
            processes.insert(id.into(), rpc.clone());
            Ok(rpc)
        })
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
    json!({"sessionId":row.id,"updatedAt":updated,"running":false,"blank":!row.file.exists(),"cwd":row.cwd,"parentSessionId":row.subagent_parent.as_ref().or(row.parent.as_ref()),"origin":row.subagent_parent.as_ref().map(|_|"subagent"),"workspaceId":row.workspace_id,"ephemeral":row.ephemeral,"projections":{"asOfSeq":row.control_revision,"values":{"title":row.title}}})
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
                "workspace-sync",
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
            lifetime: tokio::sync::watch::channel(false).0,
        }))
    }
}
pub struct PiSessionEndpoint {
    context: Arc<Context>,
    generation: String,
    host: HostDescription,
    windows: Arc<Mutex<BTreeMap<String, usize>>>,
    lifetime: tokio::sync::watch::Sender<bool>,
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
    let (entries, state) = rpc.session_state().await?;
    let identities: Vec<NativeMessageReadingIdentity> = serde_json::from_value(
        entries
            .get("readingIdentities")
            .cloned()
            .ok_or_else(|| Error::Protocol("Pi reading snapshot omitted identities".into()))?,
    )?;
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
    let loaded: std::collections::BTreeSet<_> = snapshot
        .entries
        .iter()
        .filter_map(|entry| entry["id"].as_str())
        .collect();
    let identities: Vec<_> = identities
        .into_iter()
        .filter(|identity| loaded.contains(identity.entry_id.as_str()))
        .collect();
    validate_native_reading_identities(&snapshot, &identities)?;
    Ok(
        json!({"type":if initial {"conversation"} else {"conversation-update"},"generation":generation,"snapshot":snapshot,"readingIdentities":identities}),
    )
}
/// Reconstruct active ancestry; get_entries includes inactive branches, which are not current conversation.
pub fn active_branch(result: &Value, max_messages: usize) -> Result<(Vec<Value>, bool)> {
    let (entries, more) = active_branch_window(result, max_messages)?;
    Ok((entries.into_iter().cloned().collect(), more))
}
fn active_branch_window(result: &Value, max_messages: usize) -> Result<(Vec<&Value>, bool)> {
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
        branch.push(*entry);
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
    Ok((branch.into_iter().skip(start).collect(), true))
}
fn close_on_lifetime(
    frames: FrameStream,
    lifetime: tokio::sync::watch::Receiver<bool>,
) -> FrameStream {
    Box::pin(stream::unfold(
        (frames, lifetime, false),
        |(mut frames, mut lifetime, finished)| async move {
            if finished {
                return None;
            }
            loop {
                if *lifetime.borrow() {
                    return Some((Err(Error::Disconnected), (frames, lifetime, true)));
                }
                tokio::select! {
                    biased;
                    changed = lifetime.changed() => {
                        if changed.is_err() || *lifetime.borrow() {
                            return Some((Err(Error::Disconnected), (frames, lifetime, true)));
                        }
                    }
                    frame = frames.next() => {
                        return frame.map(|frame| (frame, (frames, lifetime, false)));
                    }
                }
            }
        },
    ))
}
#[async_trait]
impl SessionEndpoint for PiSessionEndpoint {
    fn identity(&self) -> &str {
        &self.context.config.identity
    }
    fn host(&self) -> &HostDescription {
        &self.host
    }
    fn capabilities(&self) -> EndpointCapabilities {
        let mut caps = EndpointCapabilities::methods(&[
            "workspace.create",
            "workspace.rename",
            "workspace.relocate",
            "session.create",
            "session.adopt",
            "session.rename",
            "session.fork",
            "session.discard",
            "session.archive",
            "session.unarchive",
            "session.listArchived",
            "session.prompt",
            "session.updateQueue",
            "session.cancel",
            "session.history.export",
            "session.locator",
            "session.files.stat",
            "session.files.read",
            "session.files.readBytes",
            "pi.getState",
            "pi.getTree",
            "pi.getMessages",
            "commands.list",
            "commands.execute",
            "skills.list",
            "settings.get",
            "models.list",
            "models/list",
            "session.models",
            "session.selectModel",
            "commands/list",
            "commands/run",
            "commands/execute",
            "settings.describe",
            "tools.list",
            "skills/list",
            "questions.cancel",
            "approvals.policy",
            "approvals.mode",
            "approvals.select",
            "attachments.policy",
            "session.uploadFile",
            "session.fileAttachment",
            "session.attachment",
            "goals.get",
            "goals.create",
            "goals.edit",
            "goals.clear",
            "goals.pause",
            "goals.resume",
            "feedback.list",
            "feedback.put",
            "feedback.delete",
            "references.sessions",
            "references.files",
            "session.search",
            "sessions.recover",
            "session.initialPresets",
            "subagents.create",
            "subagents.list",
            "subagents.prompt",
            "subagents.cancel",
            "subagents.read",
        ]);
        caps.supports_archive_session = true;
        caps.supports_unarchive_session = true;
        caps.supports_ephemeral_fork = true;
        caps.can_fork_completed_turn = true;
        caps.supports_file_observation = true;
        caps
    }
    fn content_kind(&self) -> SessionContentKind {
        SessionContentKind::NativeConversation
    }
    fn supports_events(&self) -> bool {
        true
    }
    async fn events(&self) -> Result<FrameStream> {
        require(!*self.lifetime.borrow(), "endpoint is closed")?;
        let events = self.context.notifications.subscribe();
        let generation = self.generation.clone();
        let baseline = json!({"type":"baseline","generation":generation,"snapshot":{}});
        let updates = stream::try_unfold(
            (events, generation),
            |(mut events, generation)| async move {
                loop {
                    let event = events.recv().await.map_err(|_| Error::Disconnected)?;
                    if event.get("event").is_some()
                        && !matches!(
                            event["event"]["type"].as_str(),
                            Some(
                                "extension_ui_request"
                                    | "agent_start"
                                    | "agent_settled"
                                    | "agentkit/transport-closed"
                            )
                        )
                    {
                        continue;
                    }
                    if event["event"]["message"].as_str().is_some_and(|text| {
                        text.starts_with("agentkit-control/v1:")
                            || text.starts_with("agentkit-host/v1:")
                    }) {
                        continue;
                    }
                    return Ok(Some((
                        json!({"type":"event","generation":generation,"event":event}),
                        (events, generation),
                    )));
                }
            },
        );
        let frames: FrameStream = Box::pin(stream::once(async { Ok(baseline) }).chain(updates));
        Ok(close_on_lifetime(frames, self.lifetime.subscribe()))
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        require(!*self.lifetime.borrow(), "endpoint is closed")?;
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
                        let event = loop {
                            match events.recv().await {
                                Ok(v)
                                    if v["message"].as_str().is_some_and(|text| {
                                        text.starts_with("agentkit-control/v1:")
                                            || text.starts_with("agentkit-host/v1:")
                                    }) =>
                                {
                                    continue;
                                }
                                Ok(v) => break v,
                                Err(_) => {
                                    return Some((
                                        Err(Error::Disconnected),
                                        (events, rpc, id, generation, windows),
                                    ));
                                }
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
                            conversation_frame(&rpc, &id, &generation, limit, false)
                                .await
                                .map(|mut frame| {
                                    // Carry the actual cause in the same publication;
                                    // message_end must not leave an old preview active.
                                    frame["event"] = event;
                                    frame
                                })
                        } else {
                            Ok(
                                json!({"type":"native-event","generation":generation,"sessionId":id,"event":event}),
                            )
                        };
                        Some((value, (events, rpc, id, generation, windows)))
                    },
                );
                let frames: FrameStream =
                    Box::pin(stream::once(async { Ok(baseline) }).chain(updates));
                Ok(close_on_lifetime(frames, self.lifetime.subscribe()))
            }
            target => {
                let events = self.context.notifications.subscribe();
                let context = self.context.clone();
                let baseline = match &target {
                    StreamTarget::WorkspaceInventory => {
                        json!({"type":"baseline","generation":generation,"snapshot":context.workspace_snapshot().await})
                    }
                    StreamTarget::SessionInventory => {
                        json!({"type":"baseline","generation":generation,"snapshot":{"items":context.summaries().await}})
                    }
                    StreamTarget::SessionControl => {
                        let manifest = context.manifest.lock().await;
                        json!({"type":"baseline","generation":generation,"snapshot":{"queues":manifest.sessions.values().map(|row|json!({"sessionId":row.id,"items":row.queue})).collect::<Vec<_>>(),"jobs":[],"projections":manifest.sessions.values().map(|row|json!({"sessionId":row.id,"projections":{"asOfSeq":row.control_revision,"values":{"title":row.title,"goal":row.goal}}})).collect::<Vec<_>>()}})
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
                                StreamTarget::SessionControl if event.get("queue").is_some() => {
                                    Some(
                                        json!({"type":"queue","generation":generation,"state":event["queue"]}),
                                    )
                                }
                                StreamTarget::SessionControl
                                    if event.get("projection").is_some() =>
                                {
                                    Some(
                                        json!({"type":"projection","generation":generation,"state":event["projection"]}),
                                    )
                                }
                                StreamTarget::WorkspaceInventory
                                    if event.get("workspaces").is_some() =>
                                {
                                    Some(
                                        json!({"type":"replace","generation":generation,"snapshot":event["workspaces"]}),
                                    )
                                }
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
                let frames: FrameStream =
                    Box::pin(stream::iter(initial.into_iter().map(Ok)).chain(updates));
                Ok(close_on_lifetime(frames, self.lifetime.subscribe()))
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
        let entries = rpc.request("get_entries", json!({})).await?;
        let (_, more) = active_branch_window(&entries, *limit)?;
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
    async fn observe_files(&self, id: &str, path: &str) -> Result<FrameStream> {
        require(!*self.lifetime.borrow(), "endpoint is closed")?;
        let row = self.context.row(id).await?;
        let frames = files::observe(row.cwd, path, self.lifetime.subscribe()).await?;
        Ok(close_on_lifetime(frames, self.lifetime.subscribe()))
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
        require(!*self.lifetime.borrow(), "endpoint is closed")?;
        if matches!(
            method,
            "approvals.policy" | "approvals.mode" | "approvals.select"
        ) {
            return self.permission_call(method, &payload).await;
        }
        if !self.capabilities().supports(method) {
            return Err(unsupported(&format!("Pi method {method}")));
        }
        if method.starts_with("subagents.") {
            return self.context.subagent_call(method, &payload, rpc_id).await;
        }
        let _guard = if matches!(
            method,
            "session.prompt"
                | "session.fork"
                | "session.discard"
                | "session.archive"
                | "session.rename"
                | "session.selectModel"
                | "session.updateQueue"
        ) {
            let id = string(&payload, "sessionId")?;
            Some(
                self.context
                    .gate(id)
                    .await
                    .acquire_owned()
                    .await
                    .map_err(|_| Error::Disconnected)?,
            )
        } else {
            None
        };
        if let Some(result) = self.extension_call(method, &payload).await {
            return result;
        }
        if let Some(result) = self.attachment_call(method, &payload).await {
            return result;
        }
        if method == "session.updateQueue" {
            return self.context.update_queue(&payload).await;
        }
        if let Some(result) = self.management(method, &payload).await {
            return result;
        }
        match method {
            "session.create" => {
                require(
                    payload["agentPreset"].is_null() || payload["agentPreset"] == "default",
                    "Pi preset must be supplied by an explicit host backend",
                )?;
                require(
                    payload.get("execution").is_none(),
                    "Kernel execution configuration cannot authorize a Pi session",
                )?;
                let cwd = if let Some(workspace_id) = payload["workspaceId"].as_str() {
                    let manifest = self.context.manifest.lock().await;
                    let workspace = manifest
                        .workspaces
                        .get(workspace_id)
                        .ok_or_else(|| unsupported("unknown workspace"))?;
                    PathBuf::from(string(workspace, "path")?)
                } else {
                    PathBuf::from(string(&payload, "cwd")?)
                };
                let cwd = cwd.canonicalize().map_err(io_error)?;
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
                        subagent_parent: None,
                        model_selection: None,
                        ephemeral: false,
                        receipts: BTreeMap::new(),
                        materialized: false,
                        native_session_id: None,
                        workspace_id: payload["workspaceId"].as_str().map(String::from),
                        queue: Vec::new(),
                        control_revision: 0,
                        goal_revision: 0,
                        goal: None,
                        feedback: BTreeMap::new(),
                    },
                );
                self.context.save(&manifest).await?;
                drop(manifest);
                self.context.notify_row(&id).await;
                self.context.notify_workspaces().await;
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
                let permission_file = self.context.permission_file(&id);
                let rpc = Rpc::start(
                    &self.context.config,
                    &cwd,
                    Some(&file),
                    Some(&permission_file),
                )
                .await?;
                let state = rpc.request("get_state", json!({})).await;
                let metadata = rpc.request("get_entries", json!({})).await;
                rpc.close();
                let state = state?;
                let (goal_revision, goal, feedback) = extensions::recover_metadata(&metadata?)?;
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
                        subagent_parent: None,
                        model_selection: None,
                        ephemeral: false,
                        receipts: BTreeMap::new(),
                        materialized: true,
                        native_session_id: Some(string(&state, "sessionId")?.to_owned()),
                        workspace_id: payload["workspaceId"].as_str().map(String::from),
                        queue: Vec::new(),
                        control_revision: 0,
                        goal_revision,
                        goal,
                        feedback,
                    },
                );
                self.context.save(&manifest).await?;
                drop(manifest);
                self.context.notify_row(&id).await;
                self.context.notify_workspaces().await;
                Ok(json!({"sessionId":id,"created":true}))
            }
            "models.list" | "models/list" => {
                let rpc = Rpc::start(
                    &self.context.config,
                    &self.context.config.data_root,
                    None,
                    None,
                )
                .await?;
                let result = self.model_catalog(&rpc).await;
                rpc.close();
                result
            }
            "session.listArchived" => Ok(
                json!({"items":self.context.manifest.lock().await.sessions.values().filter(|row|row.archived).map(summary).collect::<Vec<_>>()}),
            ),
            "session.archive" | "session.unarchive" => {
                let id = string(&payload, "sessionId")?;
                self.context.row(id).await?;
                let row = self.context.row(id).await?;
                require(!row.ephemeral, "scratch sessions cannot be archived")?;
                let archived = method == "session.archive";
                if archived {
                    let active = self.context.processes.lock().await.get(id).cloned();
                    if let Some(rpc) = active.filter(|rpc| rpc.alive()) {
                        require(
                            rpc.request("get_state", json!({})).await?["isStreaming"] == false,
                            "cannot archive a running session",
                        )?;
                    }
                }
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
                self.context.notify_workspaces().await;
                if archived {
                    Ok(
                        json!({"archivedSessionIds":self.context.manifest.lock().await.sessions.values().filter(|r|r.archived).map(|r|r.id.clone()).collect::<Vec<_>>()}),
                    )
                } else {
                    Ok(json!({"sessionId":id}))
                }
            }
            _ => {
                let id = string(&payload, "sessionId")?;
                let rpc = self.context.process(id).await?;
                match method {
                    "session.models" => self.model_catalog(&rpc).await,
                    "session.selectModel" => {
                        let result = self.select_model(&rpc, &payload).await?;
                        let mut manifest = self.context.manifest.lock().await;
                        manifest
                            .sessions
                            .get_mut(id)
                            .expect("owned row")
                            .model_selection = Some(result["selected"].clone());
                        self.context.save(&manifest).await?;
                        Ok(result)
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
                        let seq = self
                            .context
                            .publish_projection(id, "title", json!(title))
                            .await?;
                        Ok(json!({"title":title,"seq":seq}))
                    }
                    "session.prompt" => self.context.direct_prompt(&rpc, &payload, rpc_id).await,
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
        self.lifetime.send_replace(true);
        let mut processes = self.context.processes.lock().await;
        for rpc in processes.values() {
            rpc.close();
        }
        processes.clear();
        self.context.actionables.lock().await.clear();
        self.context.armed_goals.lock().await.clear();
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
            json!({"leafId":"leaf","entries":[
                {"id":"old","parentId":"old","type":"message"},
                {"id":"leaf","parentId":"old","type":"message"}
            ]}),
            json!({"leafId":"leaf","entries":[
                {"id":"old","parentId":"missing","type":"message"},
                {"id":"leaf","parentId":"old","type":"message"}
            ]}),
        ] {
            assert!(active_branch(&bad, 50).is_err());
            assert!(active_branch(&bad, 1).is_err());
        }
    }
    #[tokio::test]
    async fn materialized_pointer_cannot_silently_become_a_new_or_redirected_session() {
        let root = std::env::temp_dir().join(format!("agentkit-pointer-{}", uuid::Uuid::new_v4()));
        let connector = PiConnector::new(PiConfiguration::new(
            PathBuf::from("/bin/false"),
            root.clone(),
        ))
        .await
        .unwrap();
        let root = connector.context.config.data_root.clone();
        assert_eq!(
            tokio::fs::read_to_string(root.join("agentkit-input-files.mjs"))
                .await
                .unwrap(),
            include_str!("../resources/agentkit-input-files.mjs")
        );
        let mut row: Row = serde_json::from_value(json!({"id":uuid::Uuid::new_v4().to_string(),"cwd":root,
            "file":root.join("sessions/native.jsonl"),"archived":false,"ephemeral":false,"receipts":{}})).unwrap();
        connector.context.validate_pointer(&row).await.unwrap();
        row.materialized = true;
        assert!(connector.context.validate_pointer(&row).await.is_err());
        assert!(!row.file.exists());
        tokio::fs::write(&row.file, b"owned native bytes")
            .await
            .unwrap();
        connector.context.validate_pointer(&row).await.unwrap();
        #[cfg(unix)]
        {
            tokio::fs::remove_file(&row.file).await.unwrap();
            let redirected = root.join("redirected.jsonl");
            tokio::fs::write(&redirected, b"different native bytes")
                .await
                .unwrap();
            std::os::unix::fs::symlink(&redirected, &row.file).unwrap();
            assert!(connector.context.validate_pointer(&row).await.is_err());
        }
        let endpoint = PiSessionEndpoint {
            context: connector.context.clone(),
            generation: "test".into(),
            host: serde_json::from_value(
                json!({"protocolVersion":3,"product":{"name":"Pi","version":"1.3.0"},
                "capabilities":[],"cwd":root,"home":root,"attachedSessions":0,"canOpenPath":false}),
            )
            .unwrap(),
            windows: Arc::new(Mutex::new(BTreeMap::new())),
            lifetime: tokio::sync::watch::channel(false).0,
        };
        endpoint.close().await;
        let error = endpoint
            .call("session.create", json!({"cwd":root}), "closed")
            .await
            .unwrap_err();
        assert!(error.to_string().contains("closed"));
        drop(endpoint);
        drop(connector);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
    #[tokio::test]
    async fn closing_endpoint_disconnects_pending_owned_streams_and_rejects_new_subscriptions() {
        let root = std::env::temp_dir().join(format!("agentkit-close-{}", uuid::Uuid::new_v4()));
        let connector = PiConnector::new(PiConfiguration::new(
            PathBuf::from("/bin/false"),
            root.clone(),
        ))
        .await
        .unwrap();
        let root = connector.context.config.data_root.clone();
        let endpoint = PiSessionEndpoint {
            context: connector.context.clone(),
            generation: "test-close".into(),
            host: serde_json::from_value(json!({
                "protocolVersion":3,
                "product":{"name":"Pi","version":"1.3.0"},
                "capabilities":[],"cwd":root,"home":root,
                "attachedSessions":0,"canOpenPath":false
            }))
            .unwrap(),
            windows: Arc::new(Mutex::new(BTreeMap::new())),
            lifetime: tokio::sync::watch::channel(false).0,
        };

        let mut streams = vec![endpoint.events().await.unwrap()];
        for target in [
            StreamTarget::WorkspaceInventory,
            StreamTarget::SessionInventory,
            StreamTarget::SessionControl,
            StreamTarget::Actionables,
        ] {
            streams.push(endpoint.follow(target).await.unwrap());
        }
        for stream in &mut streams {
            assert!(
                tokio::time::timeout(Duration::from_secs(1), stream.next())
                    .await
                    .unwrap()
                    .unwrap()
                    .is_ok()
            );
        }

        let nexts = streams
            .iter_mut()
            .map(|stream| Box::pin(stream.next()))
            .collect::<Vec<_>>();
        let mut pending = futures_util::future::join_all(nexts);
        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut pending)
                .await
                .is_err()
        );

        endpoint.close().await;
        endpoint.close().await;
        let disconnected = tokio::time::timeout(Duration::from_secs(1), &mut pending)
            .await
            .expect("close must wake all pending endpoint streams");
        drop(pending);
        assert_eq!(disconnected.len(), streams.len());
        for item in disconnected {
            assert!(
                item.expect("closed streams yield one terminal error")
                    .is_err()
            );
        }
        for stream in &mut streams {
            assert!(
                tokio::time::timeout(Duration::from_secs(1), stream.next())
                    .await
                    .unwrap()
                    .is_none()
            );
        }
        assert!(endpoint.events().await.is_err());
        for target in [
            StreamTarget::WorkspaceInventory,
            StreamTarget::SessionInventory,
            StreamTarget::SessionControl,
            StreamTarget::Actionables,
        ] {
            assert!(endpoint.follow(target).await.is_err());
        }

        drop(endpoint);
        drop(connector);
        tokio::fs::remove_dir_all(root).await.unwrap();
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
