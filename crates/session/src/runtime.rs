use crate::{ConnectionState, MemoryProjectionStore};
use futures_util::{
    StreamExt,
    stream::{self, BoxStream, SelectAll},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    sync::{
        Arc, RwLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tekes_agent_contract::*;
use tokio::sync::{mpsc, oneshot, watch};
static NEXT_RPC: AtomicU64 = AtomicU64::new(1);
fn rpc_id() -> String {
    format!(
        "agentkit-rpc-{}-{}",
        std::process::id(),
        NEXT_RPC.fetch_add(1, Ordering::Relaxed)
    )
}

/// Wait for bounded-channel capacity before the caller's final local admission.
/// The synchronous check and enqueue have no intervening await; this does not
/// retract an already enqueued operation when a host owner later closes.
async fn enqueue_after_validation<T: Send>(
    sender: &mpsc::Sender<T>,
    command: T,
    validate: impl FnOnce() -> Result<()> + Send,
) -> Result<()> {
    let permit = sender.reserve().await.map_err(|_| Error::Disconnected)?;
    validate()?;
    permit.send(command);
    Ok(())
}

type HistoryOwners =
    std::sync::Mutex<std::collections::BTreeMap<String, Arc<tokio::sync::Mutex<BTreeSet<String>>>>>;
type Reply<T> = oneshot::Sender<Result<T>>;
type OperationResult = Option<(Option<String>, Reply<Value>, Result<Value>)>;
enum Command {
    Open(String, Reply<()>),
    Close(String, Reply<()>),
    Earlier(String, Reply<bool>),
    Observe(String, String, Reply<FrameStream>),
    RefreshInventory {
        expected_epoch: u64,
        reply: Reply<()>,
    },
    Call {
        method: String,
        payload: Value,
        rpc_id: String,
        expected_epoch: Option<u64>,
        reply: Reply<Value>,
    },
    Respond {
        id: String,
        revision: i64,
        outcome: Value,
        reply: Reply<()>,
    },
    Reconnect(Reply<()>),
    Stop(Reply<()>),
}
/// Owns readers and recovery. Snapshot observers never access harness-private files.
pub struct SessionRuntime {
    commands: mpsc::Sender<Command>,
    shared: Arc<SharedProjection>,
    revision: watch::Receiver<u64>,
    abort: tokio::task::AbortHandle,
    history_owners: HistoryOwners,
}
impl Drop for SessionRuntime {
    fn drop(&mut self) {
        self.abort.abort();
    }
}
impl SessionRuntime {
    pub fn spawn(endpoint_id: impl Into<String>, connector: Arc<dyn EndpointConnector>) -> Self {
        let (commands, receiver) = mpsc::channel(64);
        let store = MemoryProjectionStore::new(endpoint_id);
        let (publish, revision) = watch::channel(0);
        let shared = Arc::new(SharedProjection {
            store: RwLock::new(store),
            publish,
        });
        let task = tokio::spawn(run(connector, receiver, shared.clone()));
        Self {
            commands,
            shared,
            revision,
            abort: task.abort_handle(),
            history_owners: std::sync::Mutex::new(Default::default()),
        }
    }
    /// Explicit immutable copy. Use inspect for bounded reads without copying history.
    pub fn snapshot(&self) -> Arc<MemoryProjectionStore> {
        self.shared.read(|s| Arc::new(s.clone()))
    }
    pub fn inspect<T>(&self, read: impl FnOnce(&MemoryProjectionStore) -> T) -> T {
        self.shared.read(read)
    }
    pub fn subscribe(&self) -> ProjectionSubscription {
        ProjectionSubscription {
            shared: self.shared.clone(),
            revision: self.revision.clone(),
        }
    }
    pub async fn wait_ready(&self, timeout: Duration) -> Result<()> {
        self.wait_for(timeout, |s| s.connection == ConnectionState::Ready)
            .await
    }
    async fn wait_for(
        &self,
        timeout: Duration,
        predicate: impl Fn(&MemoryProjectionStore) -> bool,
    ) -> Result<()> {
        let mut subscription = self.subscribe();
        tokio::time::timeout(timeout, async {
            loop {
                subscription.revision.borrow_and_update();
                let outcome = subscription.inspect(|current| {
                    if predicate(current) {
                        Some(Ok(()))
                    } else if let ConnectionState::Failed(e) = &current.connection {
                        Some(Err(Error::Transport(e.clone())))
                    } else if current.connection == ConnectionState::Stopped {
                        Some(Err(Error::Disconnected))
                    } else {
                        None
                    }
                });
                if let Some(outcome) = outcome {
                    return outcome;
                }
                subscription
                    .changed()
                    .await
                    .map_err(|_| Error::Disconnected)?;
            }
        })
        .await
        .map_err(|_| Error::Timeout)?
    }
    async fn send<T>(&self, command: Command, result: oneshot::Receiver<Result<T>>) -> Result<T> {
        self.commands
            .send(command)
            .await
            .map_err(|_| Error::Disconnected)?;
        result.await.map_err(|_| Error::Disconnected)?
    }
    fn history_owners_for(&self, id: &str) -> Arc<tokio::sync::Mutex<BTreeSet<String>>> {
        // Keep the per-session lock identity stable across empty→open transitions. A failed
        // pointer can wait for its own baseline without blocking a healthy session's lease.
        self.history_owners
            .lock()
            .expect("history owners lock")
            .entry(id.into())
            .or_default()
            .clone()
    }
    pub async fn open_session(&self, id: &str) -> Result<()> {
        let owners = self.history_owners_for(id);
        let mut owners = owners.lock().await;
        self.open_history(id).await?;
        owners.insert("legacy".into());
        Ok(())
    }
    async fn open_history(&self, id: &str) -> Result<()> {
        let (reply, result) = oneshot::channel();
        self.send(Command::Open(id.into(), reply), result).await?;
        self.wait_for(Duration::from_secs(30), |s| {
            s.sessions
                .get(id)
                .is_some_and(|s| s.journal_ready || s.journal_error.is_some())
        })
        .await?;
        self.inspect(|s| {
            let session = s.sessions.get(id).ok_or(Error::Disconnected)?;
            if let Some(error) = &session.journal_error {
                Err(Error::Transport(error.clone()))
            } else if session.journal_ready {
                Ok(())
            } else {
                Err(Error::Transport("session unavailable".into()))
            }
        })
    }
    pub async fn observe_files(&self, id: &str, path: &str) -> Result<FrameStream> {
        let (reply, result) = oneshot::channel();
        self.send(Command::Observe(id.into(), path.into(), reply), result)
            .await
    }
    pub async fn close_session(&self, id: &str) -> Result<()> {
        self.release_history(id, "legacy").await
    }
    async fn release_history(&self, id: &str, owner: &str) -> Result<()> {
        let owners = self.history_owners_for(id);
        let mut owners = owners.lock().await;
        if !owners.remove(owner) || !owners.is_empty() {
            return Ok(());
        }
        let (reply, result) = oneshot::channel();
        self.send(Command::Close(id.into(), reply), result).await
    }
    /// Each mounted consumer owns a distinct lease; releasing one never closes another's stream.
    pub async fn acquire_history(self: &Arc<Self>, id: &str) -> Result<SessionHistoryLease> {
        require(!id.is_empty(), "empty history session")?;
        let owner = rpc_id();
        let lease = SessionHistoryLease {
            runtime: self.clone(),
            id: id.into(),
            owner: owner.clone(),
        };
        let owners = self.history_owners_for(id);
        let mut owners = owners.lock().await;
        owners.insert(owner);
        self.open_history(id).await?;
        Ok(lease)
    }
    pub async fn load_earlier(&self, id: &str) -> Result<bool> {
        let previous_native_count = self.inspect(|s| {
            s.sessions
                .get(id)
                .and_then(|s| s.native_conversation.as_ref())
                .map(|s| s.entries.len())
        });
        let (reply, result) = oneshot::channel();
        let loaded = self
            .send(Command::Earlier(id.into(), reply), result)
            .await?;
        if loaded && let Some(previous) = previous_native_count {
            self.wait_for(Duration::from_secs(30), |s| {
                s.sessions
                    .get(id)
                    .and_then(|s| s.native_conversation.as_ref())
                    .is_some_and(|s| s.entries.len() > previous || !s.has_more_before)
            })
            .await?;
        }
        Ok(loaded)
    }
    pub async fn locator(&self, id: &str) -> Result<SessionLocator> {
        let mut locator: SessionLocator = serde_json::from_value(
            self.call("session.locator", json!({"sessionId":id}), &rpc_id())
                .await?,
        )?;
        require(
            locator.address.endpoint_id == self.inspect(|s| s.endpoint_id.clone())
                && locator.address.session_id == id,
            "session locator address mismatch",
        )?;
        if locator.cwd.is_none() {
            locator.cwd = self.inspect(|s| {
                s.sessions
                    .get(id)
                    .and_then(|s| s.summary.as_ref())
                    .and_then(|s| s["cwd"].as_str())
                    .map(String::from)
            });
        }
        Ok(locator)
    }
    pub async fn file_stat(&self, id: &str, path: &str) -> Result<SessionFileStat> {
        Ok(serde_json::from_value(
            self.call(
                "session.files.stat",
                json!({"sessionId":id,"path":path}),
                &rpc_id(),
            )
            .await?,
        )?)
    }
    pub async fn file_read_bytes(
        &self,
        id: &str,
        path: &str,
        offset: u64,
        length: usize,
    ) -> Result<SessionFileBytePage> {
        require(length > 0 && length <= 4194304, "invalid byte page length")?;
        let page: SessionFileBytePage = serde_json::from_value(
            self.call(
                "session.files.readBytes",
                json!({"sessionId":id,"path":path,"offset":offset,"limit":length}),
                &rpc_id(),
            )
            .await?,
        )?;
        page.validate(offset, length)?;
        Ok(page)
    }
    pub async fn file_read_text(
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
        let page: SessionFileTextPage = serde_json::from_value(
            self.call(
                "session.files.read",
                json!({"sessionId":id,"path":path,"offset":offset,"limit":limit}),
                &rpc_id(),
            )
            .await?,
        )?;
        page.validate(offset, limit)?;
        Ok(page)
    }
    /// Acquire a consistent, bounded file through the endpoint; never interpret a remote path locally.
    pub async fn file_read_all(&self, id: &str, path: &str, maximum: usize) -> Result<Vec<u8>> {
        use base64::Engine;
        require(maximum > 0, "file byte bound required")?;
        let mut data = Vec::new();
        let mut first: Option<SessionFileBytePage> = None;
        loop {
            let page = self
                .file_read_bytes(id, path, data.len() as u64, 262144)
                .await?;
            if let Some(previous) = &first {
                require(
                    page.version == previous.version
                        && page.absolute_path == previous.absolute_path
                        && page.bytes == previous.bytes,
                    "file changed between pages",
                )?;
            } else {
                first = Some(page.clone());
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&page.data)
                .map_err(|_| Error::Protocol("invalid file byte encoding".into()))?;
            require(
                bytes.len() <= maximum.saturating_sub(data.len()),
                "file exceeds byte bound",
            )?;
            data.extend_from_slice(&bytes);
            if page.eof {
                return Ok(data);
            }
        }
    }
    /// Read-only host-wide search over the advertised public SEP extension.
    /// Fence both actor admission and response delivery to the original connection.
    pub async fn search_sessions_at_epoch(
        &self,
        epoch: u64,
        query: &str,
        rpc_id: &str,
    ) -> Result<SessionSearchResults> {
        let allowed = || {
            self.inspect(|state| {
                state.connection == ConnectionState::Ready
                    && state.connection_epoch == epoch
                    && state.endpoint_capabilities.supports("session.search")
            })
        };
        require(allowed(), "session search source unavailable or changed")?;
        let value = self
            .call_at_epoch(epoch, "session.search", json!({"query":query}), rpc_id)
            .await?;
        require(allowed(), "session search source unavailable or changed")?;
        let result = SessionSearchResults::read(value)?;
        let unknown = self.inspect(|state| {
            result.items.iter().any(|hit| {
                state
                    .sessions
                    .get(&hit.session_id)
                    .is_none_or(|session| session.summary.is_none())
            })
        });
        if unknown {
            self.refresh_inventory_at_epoch(epoch).await?;
            require(allowed(), "session search source unavailable or changed")?;
        }
        Ok(result)
    }
    /// Reopen only the public inventory/workspace cursors, atomically publishing
    /// their validated baseline pair. Overlapping requests drain one flight.
    /// Journal readers, connection epoch and remote execution are unchanged.
    pub async fn refresh_inventory_at_epoch(&self, expected_epoch: u64) -> Result<()> {
        let (reply, result) = oneshot::channel();
        self.send(
            Command::RefreshInventory {
                expected_epoch,
                reply,
            },
            result,
        )
        .await
    }
    pub async fn call(&self, method: &str, payload: Value, rpc_id: &str) -> Result<Value> {
        self.call_context(None, method, payload, rpc_id).await
    }
    pub(crate) async fn call_with_validation(
        &self,
        method: &str,
        payload: Value,
        rpc_id: &str,
        validate: impl FnOnce() -> Result<()> + Send,
    ) -> Result<Value> {
        let (reply, result) = oneshot::channel();
        enqueue_after_validation(
            &self.commands,
            Command::Call {
                method: method.into(),
                payload,
                rpc_id: rpc_id.into(),
                expected_epoch: None,
                reply,
            },
            validate,
        )
        .await?;
        result.await.map_err(|_| Error::Disconnected)?
    }
    /// Fence at actor dispatch, not just at the caller's earlier snapshot check.
    /// A queued operation cannot cross a reconnect and target the new endpoint.
    pub async fn call_at_epoch(
        &self,
        expected_epoch: u64,
        method: &str,
        payload: Value,
        rpc_id: &str,
    ) -> Result<Value> {
        self.call_context(Some(expected_epoch), method, payload, rpc_id)
            .await
    }
    /// Validate the caller's owner after channel backpressure while retaining
    /// the connection fence at actor dispatch. Successful validation is the
    /// local admission boundary; later observer closure does not cancel it.
    pub async fn call_at_epoch_with_validation(
        &self,
        expected_epoch: u64,
        method: &str,
        payload: Value,
        rpc_id: &str,
        validate: impl FnOnce() -> Result<()> + Send,
    ) -> Result<Value> {
        let (reply, result) = oneshot::channel();
        enqueue_after_validation(
            &self.commands,
            Command::Call {
                method: method.into(),
                payload,
                rpc_id: rpc_id.into(),
                expected_epoch: Some(expected_epoch),
                reply,
            },
            validate,
        )
        .await?;
        result.await.map_err(|_| Error::Disconnected)?
    }
    async fn call_context(
        &self,
        expected_epoch: Option<u64>,
        method: &str,
        payload: Value,
        rpc_id: &str,
    ) -> Result<Value> {
        let (reply, result) = oneshot::channel();
        self.send(
            Command::Call {
                method: method.into(),
                payload,
                rpc_id: rpc_id.into(),
                expected_epoch,
                reply,
            },
            result,
        )
        .await
    }
    pub async fn create_session(&self, cwd: &str, rpc_id: &str) -> Result<String> {
        require(
            cwd.starts_with('/') && !cwd.contains('\0'),
            "execution cwd must be absolute",
        )?;
        let result = self
            .call("session.create", json!({"cwd":cwd}), rpc_id)
            .await?;
        Ok(string(&result, "sessionId")?.into())
    }
    /// Caller retains this rpc_id on an uncertain outcome; the runtime never retries a mutation.
    pub async fn prompt(&self, id: &str, text: &str, rpc_id: &str) -> Result<()> {
        let result = self
            .call(
                "session.prompt",
                json!({"sessionId":id,"mode":"queue","content":[{"type":"text","text":text}]}),
                rpc_id,
            )
            .await?;
        require(result["accepted"] == true, "prompt was not accepted")
    }
    pub async fn cancel(&self, id: &str, rpc_id: &str) -> Result<()> {
        self.call("session.cancel", json!({"sessionId":id}), rpc_id)
            .await?;
        Ok(())
    }
    pub async fn respond(&self, id: &str, revision: i64, outcome: Value) -> Result<()> {
        let (reply, result) = oneshot::channel();
        self.send(
            Command::Respond {
                id: id.into(),
                revision,
                outcome,
                reply,
            },
            result,
        )
        .await
    }
    pub async fn reconnect(&self) -> Result<()> {
        let (reply, result) = oneshot::channel();
        self.send(Command::Reconnect(reply), result).await?;
        self.wait_ready(Duration::from_secs(30)).await
    }
    pub async fn stop(&self) -> Result<()> {
        let (reply, result) = oneshot::channel();
        self.send(Command::Stop(reply), result).await
    }
}
// Each stream emits a terminal error so a clean EOF cannot leave a false-ready projection.
type TaggedStream = BoxStream<'static, (String, u64, Result<Value>)>;
fn tagged(name: String, epoch: u64, frames: FrameStream) -> TaggedStream {
    let end = stream::once(async { Err(Error::Disconnected) });
    Box::pin(
        frames
            .chain(end)
            .map(move |frame| (name.clone(), epoch, frame)),
    )
}
fn inventory_reader(
    name: &str,
    epoch: u64,
    frames: FrameStream,
) -> (futures_util::future::AbortHandle, TaggedStream) {
    let (handle, registration) = futures_util::future::AbortHandle::new_pair();
    let frames = futures_util::stream::Abortable::new(frames, registration);
    (handle, tagged(name.into(), epoch, Box::pin(frames)))
}
/// Revision notifications carry no history payload; reads hold only a short synchronous lock.
struct SharedProjection {
    store: RwLock<MemoryProjectionStore>,
    publish: watch::Sender<u64>,
}
impl SharedProjection {
    fn read<T>(&self, read: impl FnOnce(&MemoryProjectionStore) -> T) -> T {
        read(&self.store.read().expect("projection lock poisoned"))
    }
    fn update<T>(&self, write: impl FnOnce(&mut MemoryProjectionStore) -> T) -> T {
        let result = write(&mut self.store.write().expect("projection lock poisoned"));
        self.publish
            .send_modify(|version| *version = version.wrapping_add(1));
        result
    }
}
#[derive(Clone)]
pub struct ProjectionSubscription {
    shared: Arc<SharedProjection>,
    revision: watch::Receiver<u64>,
}
impl ProjectionSubscription {
    pub fn inspect<T>(&self, read: impl FnOnce(&MemoryProjectionStore) -> T) -> T {
        self.shared.read(read)
    }
    /// Explicit immutable snapshot at the current revision.
    pub fn borrow_and_update(&mut self) -> Arc<MemoryProjectionStore> {
        self.revision.borrow_and_update();
        self.shared.read(|s| Arc::new(s.clone()))
    }
    pub async fn changed(&mut self) -> std::result::Result<(), watch::error::RecvError> {
        self.revision.changed().await
    }
}

async fn run(
    connector: Arc<dyn EndpointConnector>,
    mut commands: mpsc::Receiver<Command>,
    store: Arc<SharedProjection>,
) {
    let mut wanted = BTreeSet::new();
    let mut failures = 0u32;
    let mut epoch = 0u64;
    'connections: loop {
        store.update(MemoryProjectionStore::disconnect);
        let connect = connector.connect();
        tokio::pin!(connect);
        let endpoint = loop {
            tokio::select! {
                endpoint = &mut connect => break endpoint,
                command = commands.recv() => match command {
                    Some(Command::Stop(reply)) => { store.update(|s| s.connection = ConnectionState::Stopped); let _ = reply.send(Ok(())); return; }
                    Some(Command::Reconnect(reply)) => { let _ = reply.send(Ok(())); }
                    Some(command) => reject(command),
                    None => return,
                }
            }
        };
        let connected_at = tokio::time::Instant::now();
        let result = match endpoint {
            Ok(endpoint) => {
                if !store.read(|s| endpoint.identity() == s.endpoint_id) {
                    Err(Error::Protocol(
                        "connector endpoint identity mismatch".into(),
                    ))
                } else if endpoint
                    .sequenced_queue_projection_key()
                    .is_some_and(|key| key != SEQUENCED_QUEUE_PROJECTION_KEY)
                {
                    endpoint.close().await;
                    Err(Error::Protocol(
                        "unsupported sequenced queue projection key".into(),
                    ))
                } else {
                    let queue_projection_key = endpoint.sequenced_queue_projection_key();
                    epoch += 1;
                    store.update(|s| {
                        s.begin_connection(endpoint.host().clone());
                        s.configure_sequenced_queue_projection(queue_projection_key);
                        s.endpoint_capabilities = endpoint.capabilities();
                    });
                    let result =
                        connected(endpoint.clone(), &mut commands, &store, &mut wanted, epoch)
                            .await;
                    endpoint.close().await;
                    result
                }
            }
            Err(error) => Err(error),
        };
        match result {
            Ok(true) => return,
            Ok(false) => {
                failures = 0;
                continue;
            }
            Err(error) => {
                // Long healthy connections reset the failure budget; repeated bad baselines do not.
                if connected_at.elapsed() >= Duration::from_secs(30) {
                    failures = 0;
                }
                failures += 1;
                store.update(|s| {
                    s.disconnect();
                    s.connection = if failures >= 5 {
                        ConnectionState::Failed(error.to_string())
                    } else {
                        ConnectionState::Connecting
                    };
                    s.last_error = Some(error.to_string());
                });
                let delay = tokio::time::sleep(Duration::from_millis(100 * (1 << failures.min(5))));
                tokio::pin!(delay);
                loop {
                    tokio::select! {
                        _ = &mut delay, if failures < 5 => continue 'connections,
                        command = commands.recv() => match command {
                            Some(Command::Reconnect(reply)) => { failures = 0; let _ = reply.send(Ok(())); continue 'connections; }
                            Some(Command::Stop(reply)) => { store.update(|s| s.connection = ConnectionState::Stopped); let _ = reply.send(Ok(())); return; }
                            Some(command) => reject(command),
                            None => return,
                        }
                    }
                }
            }
        }
    }
}
fn reject(command: Command) {
    match command {
        Command::Open(_, reply)
        | Command::Close(_, reply)
        | Command::Respond { reply, .. }
        | Command::RefreshInventory { reply, .. } => {
            let _ = reply.send(Err(Error::Disconnected));
        }
        Command::Observe(_, _, reply) => {
            let _ = reply.send(Err(Error::Disconnected));
        }
        Command::Earlier(_, reply) => {
            let _ = reply.send(Err(Error::Disconnected));
        }
        Command::Call { reply, .. } => {
            let _ = reply.send(Err(Error::Disconnected));
        }
        Command::Reconnect(reply) | Command::Stop(reply) => {
            let _ = reply.send(Err(Error::Disconnected));
        }
    }
}
async fn connected(
    endpoint: Arc<dyn SessionEndpoint>,
    commands: &mut mpsc::Receiver<Command>,
    store: &SharedProjection,
    wanted: &mut BTreeSet<String>,
    epoch: u64,
) -> Result<bool> {
    let connection_epoch = store.read(|state| state.connection_epoch);
    let mut operations: tokio::task::JoinSet<OperationResult> = tokio::task::JoinSet::new();
    let mut inventory_refreshes = tokio::task::JoinSet::new();
    let mut inventory_waiters: Vec<Reply<()>> = vec![];
    let mut inventory_dirty = false;
    let mut inventory_epoch = epoch;
    let mut inventory_cancellations = Vec::new();
    let mut streams = SelectAll::new();
    for (name, target) in [
        ("inventory", StreamTarget::SessionInventory),
        ("control", StreamTarget::SessionControl),
        ("actionables", StreamTarget::Actionables),
    ] {
        let frames = endpoint.follow(target).await?;
        if name == "inventory" {
            let (cancel, stream) = inventory_reader(name, inventory_epoch, frames);
            inventory_cancellations.push(cancel);
            streams.push(stream);
        } else {
            streams.push(tagged(name.into(), epoch, frames));
        }
    }
    if endpoint.supports_events() {
        streams.push(tagged("events".into(), epoch, endpoint.events().await?));
    }
    if endpoint.host().capabilities.contains("workspace-sync") {
        let frames = endpoint.follow(StreamTarget::WorkspaceInventory).await?;
        let (cancel, stream) = inventory_reader("workspace", inventory_epoch, frames);
        inventory_cancellations.push(cancel);
        streams.push(stream);
    }
    let mut journal_epochs = std::collections::BTreeMap::new();
    let mut journal_cancellations = std::collections::BTreeMap::new();
    let mut next_journal_epoch = epoch * 1_000_000;
    for id in wanted.iter() {
        next_journal_epoch += 1;
        match add_journal(
            endpoint.as_ref(),
            store,
            &mut streams,
            id,
            next_journal_epoch,
        )
        .await
        {
            Ok(cancellation) => {
                journal_cancellations.insert(id.clone(), cancellation);
                journal_epochs.insert(id.clone(), next_journal_epoch);
            }
            Err(error) => record_session_error(store, id, &error),
        }
    }
    let mut journal_retries = std::collections::BTreeMap::<String, usize>::new();
    let mut recovered = false;
    let baseline_deadline = tokio::time::sleep(Duration::from_secs(30));
    tokio::pin!(baseline_deadline);
    loop {
        tokio::select! {
            refreshed = inventory_refreshes.join_next(), if !inventory_refreshes.is_empty() => {
                let result = match refreshed {
                    Some(Ok(result)) => result,
                    _ => Err(Error::Transport("inventory refresh task ended".into())),
                };
                let result = result.and_then(|fresh: crate::inventory_refresh::InventoryRefresh| {
                    let next = inventory_epoch.checked_add(1).ok_or_else(|| unsupported("inventory reader epoch exhausted"))?;
                    store.update(|state| {
                        require(state.connection == ConnectionState::Ready && state.connection_epoch == connection_epoch, "inventory refresh source changed")?;
                        state.replace_inventory_baselines(fresh.workspace.as_ref().map(|cursor| cursor.baseline.clone()), fresh.inventory.baseline.clone())
                    })?;
                    inventory_epoch = next;
                    for cancel in inventory_cancellations.drain(..) { cancel.abort(); }
                    let mut cursors = vec![("inventory", fresh.inventory)];
                    if let Some(workspace) = fresh.workspace { cursors.push(("workspace", workspace)); }
                    for (name, cursor) in cursors {
                        let (cancel, stream) = inventory_reader(name, inventory_epoch, cursor.frames);
                        inventory_cancellations.push(cancel);
                        streams.push(stream);
                    }
                    Ok(())
                });
                match result {
                    Ok(()) if inventory_dirty => {
                        inventory_dirty = false;
                        inventory_refreshes.spawn(crate::inventory_refresh::read(endpoint.clone()));
                    }
                    Ok(()) => { for reply in inventory_waiters.drain(..) { let _ = reply.send(Ok(())); } }
                    Err(error) => {
                        inventory_dirty = false;
                        let message = format!("inventory refresh failed: {error}");
                        for reply in inventory_waiters.drain(..) { let _ = reply.send(Err(Error::Protocol(message.clone()))); }
                    }
                }
            },
            completed = operations.join_next(), if !operations.is_empty() => {
                if let Some(Ok(Some((removed,reply,result))))=completed {
                    if result.is_err() && let Some(id)=removed {
                        next_journal_epoch+=1;
                        if let Ok(cancellation)=add_journal(endpoint.as_ref(),store,&mut streams,&id,next_journal_epoch).await {
                            wanted.insert(id.clone());journal_epochs.insert(id.clone(),next_journal_epoch);journal_cancellations.insert(id,cancellation);
                        }
                    }
                    let _=reply.send(result);
                }
            },
            _ = &mut baseline_deadline, if !recovered => return Err(Error::Timeout),
            frame = streams.next() => {
                let Some((name, frame_epoch, frame)) = frame else { return Err(Error::Disconnected); };
                if let Some(id) = name.strip_prefix("journal:") {
                    if journal_epochs.get(id) != Some(&frame_epoch) { continue; }
                } else if name == "inventory" || name == "workspace" {
                    if frame_epoch != inventory_epoch { continue; }
                } else if frame_epoch != epoch { continue; }
                let is_live = frame.as_ref().is_ok_and(|frame| frame["type"] == "event");
                let result = frame.and_then(|frame| store.update(|s| {
                    let result = s.apply(&name, frame);
                    if !recovered { s.connection = ConnectionState::Connecting; }
                    result
                }));
                if let Err(error) = result {
                    if let Some(id) = name.strip_prefix("journal:") {
                        let attempts = journal_retries.entry(id.to_owned()).or_default();
                        // A peer's explicit per-stream error is terminal for this history
                        // read (for example, a quarantined session file). Reopening it
                        // immediately cannot repair the source and can monopolize the
                        // connected loop. Keep the failure scoped to this session; the
                        // user can explicitly reopen it after repairing the source.
                        let retryable = !matches!(&error, Error::Remote(_));
                        if retryable && *attempts < 5 {
                            *attempts += 1;
                            if let Some(cancellation) = journal_cancellations.remove(id) { cancellation.abort(); }
                            store.update(|s| {
                                s.reset_journal(id);
                                if let Some(session) = s.sessions.get_mut(id) { session.journal_error = Some(error.to_string()); }
                            });
                            next_journal_epoch += 1;
                            match add_journal(endpoint.as_ref(), store, &mut streams, id, next_journal_epoch).await {
                                Ok(cancellation) => {
                                    journal_cancellations.insert(id.to_owned(), cancellation);
                                    journal_epochs.insert(id.to_owned(), next_journal_epoch);
                                    continue;
                                }
                                Err(error) => record_session_error(store, id, &error),
                            }
                        } else if matches!(&error, Error::Remote(_)) {
                            record_session_read_error(store, id, &error);
                        } else { record_session_error(store, id, &error); }
                        if let Some(cancellation) = journal_cancellations.remove(id) { cancellation.abort(); }
                        journal_epochs.remove(id);
                        continue;
                    }
                    return Err(error);
                }
                if is_live && let Some(id) = name.strip_prefix("journal:") { journal_retries.remove(id); }
                if store.read(MemoryProjectionStore::ready) && !recovered {
                    let ids: Vec<_> = store.read(|s| s.sessions.iter().filter(|(_, s)| s.summary.is_some()).map(|(id, _)| id.clone()).collect());
                    if !ids.is_empty() && endpoint.supports_recovery() { endpoint.call("sessions.recover", json!({"sessionIds":ids}), &rpc_id()).await?; }
                    recovered = true;
                    store.update(|s| { s.connection = ConnectionState::Ready; s.last_error = None; });
                }
            }
            command = commands.recv() => {
                let Some(command) = command else { return Ok(true); };
                match command {
                    Command::Stop(reply) => { store.update(|s| { s.disconnect(); s.connection = ConnectionState::Stopped; }); let _ = reply.send(Ok(())); return Ok(true); }
                    Command::Reconnect(reply) => { store.update(MemoryProjectionStore::disconnect); let _ = reply.send(Ok(())); return Ok(false); }
                    Command::RefreshInventory { expected_epoch, reply } => {
                        if !store.read(|state| state.connection == ConnectionState::Ready && state.connection_epoch == expected_epoch)
                            || inventory_waiters.len() >= 64 {
                            let _ = reply.send(Err(Error::Disconnected)); continue;
                        }
                        inventory_waiters.push(reply);
                        if inventory_refreshes.is_empty() {
                            inventory_refreshes.spawn(crate::inventory_refresh::read(endpoint.clone()));
                        } else { inventory_dirty = true; }
                    }
                    Command::Open(id, reply) => {
                        if !store.read(MemoryProjectionStore::ready) { let _ = reply.send(Err(Error::Disconnected)); continue; }
                        let result = if let std::collections::btree_map::Entry::Vacant(entry) = journal_epochs.entry(id.clone()) {
                            next_journal_epoch += 1;
                            match add_journal(endpoint.as_ref(), store, &mut streams, &id, next_journal_epoch).await {
                                Ok(cancellation) => { store.update(|s| {s.reset_journal(&id); if let Some(session)=s.sessions.get_mut(&id) {session.journal_error=None;}}); wanted.insert(id.clone()); journal_cancellations.insert(id.clone(), cancellation); entry.insert(next_journal_epoch); Ok(()) }
                                Err(e) => Err(e),
                            }
                        } else { Ok(()) };
                        let _ = reply.send(result);
                    }
                    Command::Close(id, reply) => {
                        wanted.remove(&id); journal_epochs.remove(&id); store.update(|s| s.reset_journal(&id));
                        if let Some(cancellation) = journal_cancellations.remove(&id) { cancellation.abort(); }
                        let _ = reply.send(Ok(()));
                    }
                    Command::Observe(id,path,reply)=>{
                        let endpoint=endpoint.clone();
                        operations.spawn(async move {let result=endpoint.observe_files(&id,&path).await;let _=reply.send(result);None});
                    }
                    Command::Earlier(id, reply) => { let result = load_earlier(endpoint.as_ref(), store, &id).await; let _ = reply.send(result); }
                    Command::Call { method, payload, rpc_id, expected_epoch, mut reply } => {
                        if !store.read(|state|state.ready() && expected_epoch.is_none_or(|epoch|epoch==state.connection_epoch)) || operations.len() >= 64 { let _ = reply.send(Err(Error::Disconnected)); continue; }
                        let endpoint = endpoint.clone();
                        let removed=if matches!(method.as_str(),"session.discard"|"session.archive") {
                            let id=payload["sessionId"].as_str().unwrap_or("").to_owned();
                            if wanted.remove(&id) {
                                journal_epochs.remove(&id);if let Some(cancellation)=journal_cancellations.remove(&id) {cancellation.abort();}
                                store.update(|s|s.reset_journal(&id));Some(id)
                            } else {None}
                        } else {None};
                        operations.spawn(async move {
                            let result = if method == "session.search" {
                                // Cancel only this specified read operation. Losing a mutation
                                // receiver must never be interpreted as cancelling remote work.
                                tokio::select! { biased;
                                    _ = reply.closed() => return None,
                                    result = invoke(endpoint.as_ref(), &method, payload, &rpc_id) => result,
                                }
                            } else { invoke(endpoint.as_ref(), &method, payload, &rpc_id).await };
                            Some((removed,reply,result))
                        });
                    }
                    Command::Respond { id, revision, outcome, reply } => {
                        let valid = store.read(|s| s.ready() && s.actionables.get(&id).is_some_and(|a| integer(a, "revision").ok() == Some(revision)));
                        if !valid || operations.len() >= 64 { let _ = reply.send(Err(Error::Protocol("stale or missing actionable revision".into()))); continue; }
                        let endpoint = endpoint.clone();
                        operations.spawn(async move { let result = endpoint.respond(&id, revision, outcome).await; let _ = reply.send(result); None });
                    }
                }
            }
        }
    }
}
fn record_session_error(store: &SharedProjection, id: &str, error: &Error) {
    store.update(|s| {
        s.reset_journal(id);
        s.sessions.entry(id.into()).or_default().journal_error = Some(error.to_string());
    });
}
fn record_session_read_error(store: &SharedProjection, id: &str, error: &Error) {
    // A remote per-stream failure does not invalidate the last complete projection.
    // Keep it inspectable while exposing the current history error to consumers.
    store.update(|s| {
        s.sessions.entry(id.into()).or_default().journal_error = Some(error.to_string());
    });
}
async fn add_journal(
    endpoint: &dyn SessionEndpoint,
    store: &SharedProjection,
    streams: &mut SelectAll<TaggedStream>,
    id: &str,
    epoch: u64,
) -> Result<futures_util::future::AbortHandle> {
    let address = JournalAddress {
        session_id: id.into(),
    };
    let target = match endpoint.content_kind() {
        SessionContentKind::Journal => StreamTarget::SessionJournal {
            address,
            max_messages: 50,
        },
        SessionContentKind::NativeConversation => StreamTarget::SessionConversation {
            address,
            // Re-fetch the reader's loaded window from authority. A new transport
            // must not reset it to 50 and silently drop paged/visible messages.
            max_messages: store.read(|s| {
                s.sessions
                    .get(id)
                    .and_then(|session| session.native_conversation.as_ref())
                    // Pi's window is measured in message entries; custom
                    // metadata also occupies `entries` but must not expand
                    // the amount of history requested on reconnect.
                    .map(|snapshot| {
                        snapshot
                            .entries
                            .iter()
                            .filter(|entry| entry["type"] == "message")
                            .count()
                    })
                    .unwrap_or(50)
                    .max(50)
            }),
        },
    };
    let frames = endpoint.follow(target).await?;
    let (handle, registration) = futures_util::future::AbortHandle::new_pair();
    let frames = futures_util::stream::Abortable::new(frames, registration);
    streams.push(tagged(format!("journal:{id}"), epoch, Box::pin(frames)));
    Ok(handle)
}
async fn load_earlier(
    endpoint: &dyn SessionEndpoint,
    store: &SharedProjection,
    id: &str,
) -> Result<bool> {
    if endpoint.content_kind() == SessionContentKind::NativeConversation {
        return endpoint.load_earlier_conversation(id, 50).await;
    }
    let request = store.read(|s| -> Result<Option<HistoryRequest>> {
        require(s.ready(), "endpoint not ready")?;
        let session = s
            .sessions
            .get(id)
            .ok_or_else(|| Error::Protocol("session not opened".into()))?;
        require(session.journal_ready, "journal not ready")?;
        if !session.has_more_before || session.history_base_sequence < 0 {
            return Ok(None);
        }
        Ok(Some(HistoryRequest {
            session_id: id.into(),
            through_sequence: session.history_through_sequence,
            before_sequence: Some(session.history_base_sequence),
            max_messages: 50,
        }))
    })?;
    let Some(request) = request else {
        return Ok(false);
    };
    let page = endpoint.history_page(request.clone()).await?;
    store.update(|s| {
        s.sessions
            .get_mut(id)
            .expect("session exists")
            .apply_page(page, &request)
    })
}

async fn invoke(
    endpoint: &dyn SessionEndpoint,
    method: &str,
    payload: Value,
    rpc_id: &str,
) -> Result<Value> {
    match method {
        "endpoint.capabilities" => Ok(serde_json::to_value(endpoint.capabilities())?),
        "session.locator" => Ok(serde_json::to_value(
            endpoint.locator(string(&payload, "sessionId")?).await?,
        )?),
        "session.files.stat" => Ok(serde_json::to_value(
            endpoint
                .file_stat(string(&payload, "sessionId")?, string(&payload, "path")?)
                .await?,
        )?),
        "session.files.readBytes" => Ok(serde_json::to_value(
            endpoint
                .file_read_bytes(
                    string(&payload, "sessionId")?,
                    string(&payload, "path")?,
                    payload["offset"].as_u64().unwrap_or(0),
                    payload["limit"].as_u64().unwrap_or(262144) as usize,
                )
                .await?,
        )?),
        "session.files.read" => Ok(serde_json::to_value(
            endpoint
                .file_read_text(
                    string(&payload, "sessionId")?,
                    string(&payload, "path")?,
                    payload["offset"].as_u64().unwrap_or(1),
                    payload["limit"].as_u64().unwrap_or(200) as usize,
                )
                .await?,
        )?),
        _ => endpoint.call(method, payload, rpc_id).await,
    }
}

/// Drop releases only this consumer. Runtime shutdown does not require a live executor.
pub struct SessionHistoryLease {
    runtime: Arc<SessionRuntime>,
    id: String,
    owner: String,
}
impl SessionHistoryLease {
    pub fn session_id(&self) -> &str {
        &self.id
    }
    pub async fn release(self) -> Result<()> {
        self.runtime.release_history(&self.id, &self.owner).await
    }
}
impl Drop for SessionHistoryLease {
    fn drop(&mut self) {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let runtime = self.runtime.clone();
            let id = self.id.clone();
            let owner = self.owner.clone();
            handle.spawn(async move {
                let _ = runtime.release_history(&id, &owner).await;
            });
        }
    }
}

#[cfg(test)]
mod admission_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[tokio::test]
    async fn revoked_owner_is_checked_after_channel_backpressure_without_enqueue() {
        let (sender, mut receiver) = mpsc::channel(1);
        sender.send(1_u8).await.unwrap();
        let alive = AtomicBool::new(true);
        let checks = AtomicUsize::new(0);
        let pending = enqueue_after_validation(&sender, 2, || {
            checks.fetch_add(1, Ordering::SeqCst);
            require(alive.load(Ordering::SeqCst), "owner retired")
        });
        tokio::pin!(pending);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut pending)
                .await
                .is_err()
        );
        assert_eq!(checks.load(Ordering::SeqCst), 0);
        alive.store(false, Ordering::SeqCst);
        assert_eq!(receiver.recv().await, Some(1));
        assert!(pending.await.is_err());
        assert_eq!(checks.load(Ordering::SeqCst), 1);
        assert!(receiver.try_recv().is_err());
        sender.try_send(3).unwrap();
        assert_eq!(
            receiver.recv().await,
            Some(3),
            "rejected owner releases its reserved slot"
        );
    }

    #[tokio::test]
    async fn accepted_owner_is_validated_once_and_closed_channel_never_validates() {
        let (sender, mut receiver) = mpsc::channel(1);
        let checks = AtomicUsize::new(0);
        enqueue_after_validation(&sender, 7_u8, || {
            checks.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(receiver.recv().await, Some(7));
        drop(receiver);
        assert!(
            enqueue_after_validation(&sender, 8, || {
                checks.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .await
            .is_err()
        );
        assert_eq!(checks.load(Ordering::SeqCst), 1);
    }
}
