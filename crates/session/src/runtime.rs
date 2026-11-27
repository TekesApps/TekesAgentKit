use crate::{ConnectionState, MemoryProjectionStore, SessionRecoveryOutcome};
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
    task::{Context, Poll},
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
type OperationResult = Option<(
    Option<(String, u64)>,
    Option<RecoveryAdmission>,
    Reply<Value>,
    Result<Value>,
)>;

#[derive(Clone, Copy, Debug)]
struct SuspendedJournal {
    suspend_id: u64,
    closed_while_suspended: bool,
    was_running: bool,
}
#[derive(Default)]
struct RecoveryTracking {
    unresolved: BTreeSet<String>,
    pending: BTreeSet<String>,
}
type SharedRecoveryTracking = Arc<std::sync::Mutex<RecoveryTracking>>;
/// The guard travels with the result until the actor consumes it. Cancellation
/// or discarding a completed task cannot turn an admitted mutation into permission
/// to replay it on the next connection.
struct RecoveryAdmission {
    tracking: SharedRecoveryTracking,
    ids: Vec<String>,
}
impl RecoveryAdmission {
    fn start(tracking: &SharedRecoveryTracking, ids: Vec<String>) -> Result<Self> {
        {
            let mut state = tracking.lock().expect("recovery tracking lock");
            require(
                !ids.iter().any(|id| state.pending.contains(id)),
                "session recovery is already pending",
            )?;
            state.pending.extend(ids.iter().cloned());
            state.unresolved.extend(ids.iter().cloned());
        }
        Ok(Self {
            tracking: tracking.clone(),
            ids,
        })
    }
    fn confirm(self, outcome: &SessionRecoveryOutcome) {
        let mut state = self.tracking.lock().expect("recovery tracking lock");
        for id in &outcome.recovered_session_ids {
            state.unresolved.remove(id);
        }
        state
            .unresolved
            .extend(outcome.unresolved_session_ids.iter().cloned());
        drop(state);
    }
}
impl Drop for RecoveryAdmission {
    fn drop(&mut self) {
        let mut state = self.tracking.lock().expect("recovery tracking lock");
        for id in &self.ids {
            state.pending.remove(id);
        }
    }
}

enum EarlierValue {
    Journal(HistoryRequest, HistoryPage),
    Native(bool),
}
struct EarlierResult {
    id: String,
    connection_epoch: u64,
    journal_epoch: u64,
    reply: Reply<bool>,
    result: Result<EarlierValue>,
}
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
    handle: tokio::runtime::Handle,
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
            store: RwLock::new(RevisionedProjection {
                state: store,
                revision: 0,
            }),
            publish,
        });
        let handle = tokio::runtime::Handle::current();
        let task = handle.spawn(run(connector, receiver, shared.clone()));
        Self {
            commands,
            shared,
            revision,
            abort: task.abort_handle(),
            handle,
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
    /// The process-local input revision and state are read under the same lock.
    /// This is a projection cache key, not a source/transport/CAS revision.
    pub fn inspect_with_revision<T>(
        &self,
        read: impl FnOnce(u64, &MemoryProjectionStore) -> T,
    ) -> T {
        self.shared.read_with_revision(read)
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
        self.acquire_history_inner(id, false).await
    }
    /// Keeps a view's observation alive after a scoped history failure. The
    /// failure remains in the projection; this does not admit history writes.
    /// Global connection failures and changed epochs still fail acquisition.
    pub async fn acquire_history_view(self: &Arc<Self>, id: &str) -> Result<SessionHistoryLease> {
        self.acquire_history_inner(id, true).await
    }
    async fn acquire_history_inner(
        self: &Arc<Self>,
        id: &str,
        allow_history_failure: bool,
    ) -> Result<SessionHistoryLease> {
        require(!id.is_empty(), "empty history session")?;
        let owner = rpc_id();
        let mut lease = SessionHistoryLease {
            runtime: self.clone(),
            handle: self.handle.clone(),
            id: id.into(),
            owner: owner.clone(),
            released: false,
        };
        let owners = self.history_owners_for(id);
        let mut owners = owners.lock().await;
        owners.insert(owner.clone());
        let epoch = self.inspect(|state| state.connection_epoch);
        if let Err(error) = self.open_history(id).await {
            let scoped_failure = allow_history_failure
                && self.inspect(|state| {
                    state.connection == ConnectionState::Ready
                        && state.connection_epoch == epoch
                        && state.inventory_ready
                        && state.control_ready
                        && state.sessions.get(id).is_some_and(|session| {
                            session.summary.is_some()
                                && matches!(&error, Error::Transport(message)
                                if session.journal_error.as_ref() == Some(message))
                        })
                });
            if !scoped_failure {
                owners.remove(&owner);
                let should_close = owners.is_empty();
                drop(owners);
                lease.released = true;
                if should_close {
                    let (reply, result) = oneshot::channel();
                    let _ = self.send(Command::Close(id.into(), reply), result).await;
                }
                return Err(error);
            }
        }
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

#[derive(Default)]
struct JournalStreams {
    entries: Vec<(String, u64, FrameStream)>,
    cursor: usize,
}

impl JournalStreams {
    fn insert(&mut self, id: String, epoch: u64, frames: FrameStream) {
        if let Some(entry) = self.entries.iter_mut().find(|(sid, _, _)| sid == &id) {
            entry.1 = epoch;
            entry.2 = frames;
        } else {
            self.entries.push((id, epoch, frames));
        }
    }

    /// Remove and synchronously drop the underlying `FrameStream` so peer cleanup hooks
    /// (such as `LogicalStream::drop` enqueuing mux close) happen before `Command::Close`
    /// replies `Ok(())`.
    fn remove(&mut self, id: &str) {
        self.entries.retain(|(sid, _, _)| sid != id);
    }

    fn poll_next_item(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Option<(String, u64, Result<Value>)>> {
        let len = self.entries.len();
        if len == 0 {
            return Poll::Pending;
        }
        let start = self.cursor % len;
        for offset in 0..len {
            let idx = (start + offset) % len;
            match self.entries[idx].2.poll_next_unpin(cx) {
                Poll::Ready(Some(frame)) => {
                    let id = &self.entries[idx].0;
                    let epoch = self.entries[idx].1;
                    self.cursor = idx.wrapping_add(1);
                    return Poll::Ready(Some((format!("journal:{id}"), epoch, frame)));
                }
                Poll::Ready(None) => {
                    let (id, epoch, _) = self.entries.swap_remove(idx);
                    self.cursor = idx;
                    return Poll::Ready(Some((
                        format!("journal:{id}"),
                        epoch,
                        Err(Error::Disconnected),
                    )));
                }
                Poll::Pending => {}
            }
        }
        self.cursor = self.cursor.wrapping_add(1);
        Poll::Pending
    }
}

async fn next_tagged_frame(
    streams: &mut SelectAll<TaggedStream>,
    journals: &mut JournalStreams,
    prefer_journals: bool,
) -> Option<(String, u64, Result<Value>)> {
    std::future::poll_fn(|cx| {
        if prefer_journals {
            if let Poll::Ready(item) = journals.poll_next_item(cx) {
                return Poll::Ready(item);
            }
            match streams.poll_next_unpin(cx) {
                Poll::Ready(Some(item)) => Poll::Ready(Some(item)),
                Poll::Ready(None) if journals.entries.is_empty() => Poll::Ready(None),
                Poll::Ready(None) | Poll::Pending => Poll::Pending,
            }
        } else {
            match streams.poll_next_unpin(cx) {
                Poll::Ready(Some(item)) => Poll::Ready(Some(item)),
                Poll::Ready(None) => match journals.poll_next_item(cx) {
                    Poll::Ready(item) => Poll::Ready(item),
                    Poll::Pending if journals.entries.is_empty() => Poll::Ready(None),
                    Poll::Pending => Poll::Pending,
                },
                Poll::Pending => journals.poll_next_item(cx),
            }
        }
    })
    .await
}
/// Revision notifications carry no history payload; reads hold only a short synchronous lock.
struct RevisionedProjection {
    state: MemoryProjectionStore,
    revision: u64,
}
struct SharedProjection {
    store: RwLock<RevisionedProjection>,
    publish: watch::Sender<u64>,
}
impl SharedProjection {
    fn read<T>(&self, read: impl FnOnce(&MemoryProjectionStore) -> T) -> T {
        self.read_with_revision(|_, state| read(state))
    }
    fn read_with_revision<T>(&self, read: impl FnOnce(u64, &MemoryProjectionStore) -> T) -> T {
        let projection = self.store.read().expect("projection lock poisoned");
        read(projection.revision, &projection.state)
    }
    fn update<T>(&self, write: impl FnOnce(&mut MemoryProjectionStore) -> T) -> T {
        let mut projection = self.store.write().expect("projection lock poisoned");
        let result = write(&mut projection.state);
        projection.revision = projection
            .revision
            .checked_add(1)
            .expect("projection revision exhausted");
        drop(projection);
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

#[derive(Default)]
struct ConnectionInterest {
    wanted: BTreeSet<String>,
    deferred_closes: BTreeSet<String>,
    suspended_journals: std::collections::BTreeMap<String, SuspendedJournal>,
    unconfirmed_archives: std::collections::BTreeMap<String, bool>,
}

impl ConnectionInterest {
    fn reconcile_suspended_on_disconnect(&mut self, store: &Arc<SharedProjection>) {
        // Reconnects abort in-flight archive/discard RPCs without replaying them, so any
        // session whose journal was suspended for an unconfirmed lifecycle mutation must
        // wait for the next complete authoritative baseline before reopening or retiring.
        for (id, suspended) in std::mem::take(&mut self.suspended_journals) {
            let was_running = suspended.was_running
                || store.read(|s| s.sessions.get(&id).is_some_and(session_has_active_run));
            if !suspended.closed_while_suspended {
                self.deferred_closes.remove(&id);
                self.wanted.insert(id.clone());
                self.unconfirmed_archives.insert(id, was_running);
            } else if was_running {
                self.wanted.insert(id.clone());
                self.deferred_closes.insert(id.clone());
                self.unconfirmed_archives.insert(id, true);
            }
        }
    }
}

async fn run(
    connector: Arc<dyn EndpointConnector>,
    mut commands: mpsc::Receiver<Command>,
    store: Arc<SharedProjection>,
) {
    let mut interest = ConnectionInterest::default();
    // Unknown/omitted recovery outcomes cannot be replayed by a later automatic
    // stream reconnect. An explicit sessions.recover call can reconcile them.
    let unresolved_recovery = Arc::new(std::sync::Mutex::new(RecoveryTracking::default()));
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
                    Some(Command::Close(id, reply)) => {
                        close_while_disconnected(
                            id,
                            reply,
                            &mut interest.wanted,
                            &mut interest.deferred_closes,
                            &mut interest.unconfirmed_archives,
                            &store,
                        );
                    }
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
                    let result = connected(
                        endpoint.clone(),
                        &mut commands,
                        &store,
                        &mut interest,
                        &unresolved_recovery,
                        epoch,
                    )
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
                interest.reconcile_suspended_on_disconnect(&store);
                failures = 0;
                continue;
            }
            Err(error) => {
                interest.reconcile_suspended_on_disconnect(&store);
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
                            Some(Command::Close(id, reply)) => {
                                close_while_disconnected(
                                    id,
                                    reply,
                                    &mut interest.wanted,
                                    &mut interest.deferred_closes,
                                    &mut interest.unconfirmed_archives,
                                    &store,
                                );
                            }
                            Some(command) => reject(command),
                            None => return,
                        }
                    }
                }
            }
        }
    }
}
fn session_has_active_run(session: &crate::SessionProjection) -> bool {
    has_unsettled_run(session)
        || session
            .summary
            .as_ref()
            .and_then(|summary| summary["running"].as_bool())
            == Some(true)
}

fn has_unsettled_run(session: &crate::SessionProjection) -> bool {
    if session.run_open {
        return true;
    }
    session
        .entries
        .values()
        .rev()
        .find_map(|entry| match entry.event.kind.as_str() {
            "turn/start" => Some(true),
            "turn/end" => Some(false),
            _ => None,
        })
        .unwrap_or(false)
}

fn session_authoritatively_archived_or_absent(state: &MemoryProjectionStore, id: &str) -> bool {
    if !state.ready() {
        return false;
    }
    state
        .archived_session_ids
        .iter()
        .any(|archived| archived == id)
        || match state
            .sessions
            .get(id)
            .and_then(|session| session.summary.as_ref())
        {
            None => true,
            Some(summary) => summary["status"].as_str() == Some("archived"),
        }
}

fn close_while_disconnected(
    id: String,
    reply: Reply<()>,
    wanted: &mut BTreeSet<String>,
    deferred_closes: &mut BTreeSet<String>,
    unconfirmed_archives: &mut std::collections::BTreeMap<String, bool>,
    store: &Arc<SharedProjection>,
) {
    if wanted.contains(&id)
        && (unconfirmed_archives.get(&id).copied().unwrap_or(false)
            || store.read(|s| s.sessions.get(&id).is_some_and(session_has_active_run)))
    {
        deferred_closes.insert(id);
    } else {
        unconfirmed_archives.remove(&id);
        deferred_closes.remove(&id);
        wanted.remove(&id);
        store.update(|s| s.reset_journal(&id));
    }
    let _ = reply.send(Ok(()));
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
    store: &Arc<SharedProjection>,
    interest: &mut ConnectionInterest,
    unresolved_recovery: &SharedRecoveryTracking,
    epoch: u64,
) -> Result<bool> {
    let ConnectionInterest {
        wanted,
        deferred_closes,
        suspended_journals,
        unconfirmed_archives,
    } = interest;
    let connection_epoch = store.read(|state| state.connection_epoch);
    let mut operations: tokio::task::JoinSet<OperationResult> = tokio::task::JoinSet::new();
    let mut earlier_reads = tokio::task::JoinSet::<EarlierResult>::new();
    let mut earlier_inflight = BTreeSet::<(String, u64)>::new();
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
    let mut journals = JournalStreams::default();
    let mut permanent_journal_errors = BTreeSet::<String>::new();
    let mut next_suspend_id: u64 = 0;
    let mut next_journal_epoch = epoch * 1_000_000;
    let mut failed_deferred_closes = Vec::new();
    for id in wanted.iter() {
        if unconfirmed_archives.contains_key(id) {
            continue;
        }
        next_journal_epoch += 1;
        match add_journal(
            endpoint.as_ref(),
            store,
            &mut journals,
            id,
            next_journal_epoch,
        )
        .await
        {
            Ok(()) => {
                journal_epochs.insert(id.clone(), next_journal_epoch);
            }
            Err(error) => {
                if matches!(&error, Error::Remote(_)) {
                    record_session_read_error(store, id, &error);
                    permanent_journal_errors.insert(id.clone());
                    if deferred_closes.contains(id) {
                        failed_deferred_closes.push(id.clone());
                    }
                } else {
                    record_session_error(store, id, &error);
                    if matches!(&error, Error::Disconnected | Error::Transport(_)) {
                        return Err(error);
                    }
                }
            }
        }
    }
    for id in failed_deferred_closes {
        deferred_closes.remove(&id);
        wanted.remove(&id);
    }
    let mut journal_retries = std::collections::BTreeMap::<String, usize>::new();
    let mut prefer_journals = false;
    let mut baselines_ready = false;
    let baseline_deadline = tokio::time::sleep(Duration::from_secs(30));
    tokio::pin!(baseline_deadline);
    loop {
        // A browser owns observation, not the active run's journal. Close only after
        // the journal itself reports a terminal state, including after reconnect.
        // Open/Close and frames share this actor, so a reattached observer cannot
        // race a background watcher that clears its newly acquired projection.
        let finished: Vec<_> = deferred_closes
            .iter()
            .filter(|id| {
                store.read(|s| {
                    s.sessions
                        .get(*id)
                        .is_some_and(|session| session.journal_ready && !session.run_open)
                })
            })
            .cloned()
            .collect();
        for id in finished {
            deferred_closes.remove(&id);
            wanted.remove(&id);
            journal_epochs.remove(&id);
            journals.remove(&id);
            store.update(|s| s.reset_journal(&id));
        }
        prefer_journals = !prefer_journals;
        tokio::select! {
            completed = earlier_reads.join_next(), if !earlier_reads.is_empty() => {
                let fetched = match completed {
                    Some(Ok(fetched)) => fetched,
                    _ => return Err(Error::Transport("history read task ended".into())),
                };
                earlier_inflight.remove(&(fetched.id.clone(), fetched.journal_epoch));
                let current = store.read(|state| {
                    state.ready()
                        && state.connection_epoch == fetched.connection_epoch
                        && journal_epochs.get(&fetched.id) == Some(&fetched.journal_epoch)
                        && state.sessions.get(&fetched.id)
                            .is_some_and(|session| session.journal_ready && session.journal_error.is_none())
                });
                let result = if !current {
                    Err(Error::Protocol("stale history page result".into()))
                } else {
                    match fetched.result {
                        Ok(EarlierValue::Native(loaded)) => Ok(loaded),
                        Ok(EarlierValue::Journal(request, page)) => {
                            store.update(|state| {
                                state.sessions.get_mut(&fetched.id)
                                    .ok_or_else(|| Error::Protocol("session closed during history read".into()))?
                                    .apply_page(page, &request)
                            })
                        }
                        Err(error) => Err(error),
                    }
                };
                let _ = fetched.reply.send(result);
            }
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
                if let Some(Ok(Some((removed,recovery_admission,reply,mut result))))=completed {
                    if let Some(admission) = recovery_admission {
                        let outcome = recovery_outcome(connection_epoch, admission.ids.clone(), &result, Vec::new());
                        if result.is_ok() && let Some(error) = &outcome.error {
                            result = Err(Error::Protocol(error.clone()));
                        }
                        remember_recovery(store, admission, outcome);
                    }
                    if let Some((id, suspend_id)) = removed {
                        let suspended = if suspended_journals
                            .get(&id)
                            .is_some_and(|entry| entry.suspend_id == suspend_id)
                        {
                            suspended_journals.remove(&id)
                        } else {
                            None
                        };
                        if result.is_ok() {
                            deferred_closes.remove(&id);
                        } else if let Some(suspended) = suspended
                            && !journal_epochs.contains_key(&id)
                        {
                            let should_restore = if !suspended.closed_while_suspended {
                                true
                            } else {
                                !permanent_journal_errors.contains(&id)
                                    && (suspended.was_running
                                        || store.read(|s| {
                                            s.sessions.get(&id).is_some_and(session_has_active_run)
                                        }))
                            };
                            if should_restore {
                                next_journal_epoch += 1;
                                match add_journal(
                                    endpoint.as_ref(),
                                    store,
                                    &mut journals,
                                    &id,
                                    next_journal_epoch,
                                )
                                .await
                                {
                                    Ok(()) => {
                                        wanted.insert(id.clone());
                                        journal_epochs.insert(id.clone(), next_journal_epoch);
                                        if suspended.closed_while_suspended {
                                            deferred_closes.insert(id);
                                        } else {
                                            deferred_closes.remove(&id);
                                        }
                                    }
                                    Err(error) => {
                                        if matches!(&error, Error::Remote(_)) {
                                            record_session_read_error(store, &id, &error);
                                            permanent_journal_errors.insert(id.clone());
                                            if !suspended.closed_while_suspended {
                                                deferred_closes.remove(&id);
                                                wanted.insert(id);
                                            }
                                        } else {
                                            record_session_error(store, &id, &error);
                                            if matches!(
                                                &error,
                                                Error::Disconnected | Error::Transport(_)
                                            ) {
                                                if suspended.closed_while_suspended {
                                                    wanted.insert(id.clone());
                                                    deferred_closes.insert(id);
                                                } else {
                                                    deferred_closes.remove(&id);
                                                    wanted.insert(id);
                                                }
                                                let _ = reply.send(result);
                                                return Err(error);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    let _=reply.send(result);
                }
            },
            _ = &mut baseline_deadline, if !baselines_ready => return Err(Error::Timeout),
            frame = next_tagged_frame(&mut streams, &mut journals, prefer_journals) => {
                let Some((name, frame_epoch, frame)) = frame else { return Err(Error::Disconnected); };
                if let Some(id) = name.strip_prefix("journal:") {
                    if journal_epochs.get(id) != Some(&frame_epoch) { continue; }
                } else if name == "inventory" || name == "workspace" {
                    if frame_epoch != inventory_epoch { continue; }
                } else if frame_epoch != epoch { continue; }
                let is_live = frame.as_ref().is_ok_and(|frame| frame["type"] == "event");
                let result = frame.and_then(|frame| store.update(|s| {
                    let result = s.apply(&name, frame);
                    if !baselines_ready { s.connection = ConnectionState::Connecting; }
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
                            journals.remove(id);
                            store.update(|s| {
                                s.reset_journal(id);
                                if let Some(session) = s.sessions.get_mut(id) { session.journal_error = Some(error.to_string()); }
                            });
                            next_journal_epoch += 1;
                            match add_journal(endpoint.as_ref(), store, &mut journals, id, next_journal_epoch).await {
                                Ok(()) => {
                                    journal_epochs.insert(id.to_owned(), next_journal_epoch);
                                    continue;
                                }
                                Err(error) => {
                                    if matches!(&error, Error::Remote(_)) {
                                        record_session_read_error(store, id, &error);
                                        permanent_journal_errors.insert(id.to_owned());
                                        if deferred_closes.remove(id) { wanted.remove(id); }
                                    } else {
                                        record_session_error(store, id, &error);
                                        if matches!(&error, Error::Disconnected | Error::Transport(_)) {
                                            return Err(error);
                                        }
                                    }
                                }
                            }
                        } else if matches!(&error, Error::Remote(_)) {
                            record_session_read_error(store, id, &error);
                            permanent_journal_errors.insert(id.to_owned());
                            if deferred_closes.remove(id) { wanted.remove(id); }
                        } else {
                            record_session_error(store, id, &error);
                            if matches!(&error, Error::Disconnected | Error::Transport(_)) {
                                return Err(error);
                            }
                        }
                        journals.remove(id);
                        journal_epochs.remove(id);
                        continue;
                    }
                    return Err(error);
                }
                if is_live && let Some(id) = name.strip_prefix("journal:") { journal_retries.remove(id); }
                if store.read(MemoryProjectionStore::ready) && !baselines_ready {
                    let pending_archives: Vec<String> = unconfirmed_archives.keys().cloned().collect();
                    for id in pending_archives {
                        if store.read(|state| session_authoritatively_archived_or_absent(state, &id)) {
                            unconfirmed_archives.remove(&id);
                            deferred_closes.remove(&id);
                            wanted.remove(&id);
                            journal_epochs.remove(&id);
                            journals.remove(&id);
                            permanent_journal_errors.remove(&id);
                            store.update(|s| {
                                s.reset_journal(&id);
                                if let Some(session) = s.sessions.get_mut(&id) {
                                    session.journal_error = None;
                                }
                            });
                            continue;
                        }
                        if !wanted.contains(&id) || journal_epochs.contains_key(&id) {
                            unconfirmed_archives.remove(&id);
                            continue;
                        }
                        next_journal_epoch += 1;
                        match add_journal(
                            endpoint.as_ref(),
                            store,
                            &mut journals,
                            &id,
                            next_journal_epoch,
                        )
                        .await
                        {
                            Ok(()) => {
                                unconfirmed_archives.remove(&id);
                                journal_epochs.insert(id, next_journal_epoch);
                            }
                            Err(error) => {
                                if matches!(&error, Error::Remote(_)) {
                                    unconfirmed_archives.remove(&id);
                                    record_session_read_error(store, &id, &error);
                                    permanent_journal_errors.insert(id.clone());
                                    if deferred_closes.remove(&id) {
                                        wanted.remove(&id);
                                    }
                                } else {
                                    record_session_error(store, &id, &error);
                                    if matches!(&error, Error::Disconnected | Error::Transport(_)) {
                                        return Err(error);
                                    }
                                    unconfirmed_archives.remove(&id);
                                }
                            }
                        }
                    }
                    if endpoint.supports_recovery() {
                        let interested: Vec<_> = store.read(|state| wanted.difference(deferred_closes).filter(|id| {
                            !state.archived_session_ids.contains(*id)
                                && state.sessions.get(*id).and_then(|session| session.summary.as_ref())
                                    .is_some_and(|summary| summary["status"] != "archived")
                        }).cloned().collect());
                        let (not_retried, ids): (Vec<_>, Vec<_>) = {
                            let tracking = unresolved_recovery.lock().expect("recovery tracking lock");
                            interested.into_iter().partition(|id| tracking.unresolved.contains(id))
                        };
                        if !ids.is_empty() {
                            let admission = RecoveryAdmission::start(unresolved_recovery, ids.clone())?;
                            publish_pending_recovery(store, connection_epoch, &ids);
                            let result = endpoint.call("sessions.recover", json!({"sessionIds":ids}), &rpc_id()).await;
                            remember_recovery(store, admission, recovery_outcome(connection_epoch, ids, &result, not_retried));
                        } else if !not_retried.is_empty() {
                            store.update(|state| state.recovery = Some(SessionRecoveryOutcome {
                                connection_epoch, unresolved_session_ids: not_retried.clone(), not_retried_session_ids: not_retried,
                                error: Some("prior recovery remains unresolved; it was not retried automatically".into()),
                                ..Default::default()
                            }));
                        }
                    }
                    baselines_ready = true;
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
                            match add_journal(endpoint.as_ref(), store, &mut journals, &id, next_journal_epoch).await {
                                Ok(()) => { store.update(|s| {s.reset_journal(&id); if let Some(session)=s.sessions.get_mut(&id) {session.journal_error=None;}}); wanted.insert(id.clone()); entry.insert(next_journal_epoch); Ok(()) }
                                Err(e) => Err(e),
                            }
                        } else { Ok(()) };
                        if result.is_ok() {
                            deferred_closes.remove(&id);
                            permanent_journal_errors.remove(&id);
                            suspended_journals.remove(&id);
                            unconfirmed_archives.remove(&id);
                        }
                        let _ = reply.send(result);
                    }
                    Command::Close(id, reply) => {
                        if let Some(suspended) = suspended_journals.get_mut(&id) {
                            suspended.closed_while_suspended = true;
                            if store.read(|s| {
                                s.sessions.get(&id).is_some_and(session_has_active_run)
                            }) {
                                suspended.was_running = true;
                            }
                            let _ = reply.send(Ok(()));
                            continue;
                        }
                        if let Some(was_running) = unconfirmed_archives.get(&id).copied() {
                            if was_running
                                || store.read(|s| s.sessions.get(&id).is_some_and(session_has_active_run))
                            {
                                deferred_closes.insert(id);
                            } else {
                                unconfirmed_archives.remove(&id);
                                deferred_closes.remove(&id);
                                wanted.remove(&id);
                                journal_epochs.remove(&id);
                                journals.remove(&id);
                                store.update(|s| s.reset_journal(&id));
                            }
                            let _ = reply.send(Ok(()));
                            continue;
                        }
                        if wanted.contains(&id)
                            && !permanent_journal_errors.contains(&id)
                            && store.read(|s| s.sessions.get(&id).is_some_and(has_unsettled_run))
                        {
                            deferred_closes.insert(id);
                            let _ = reply.send(Ok(()));
                            continue;
                        }
                        deferred_closes.remove(&id);
                        wanted.remove(&id);
                        journal_epochs.remove(&id);
                        journals.remove(&id);
                        store.update(|s| s.reset_journal(&id));
                        let _ = reply.send(Ok(()));
                    }
                    Command::Observe(id,path,reply)=>{
                        let endpoint=endpoint.clone();
                        operations.spawn(async move {let result=endpoint.observe_files(&id,&path).await;let _=reply.send(result);None});
                    }
                    Command::Earlier(id, reply) => {
                        let active_journal_epoch = journal_epochs.get(&id).copied();
                        let Some(journal_epoch) = active_journal_epoch else {
                            let _ = reply.send(Err(Error::Protocol("history reader is not open".into())));
                            continue;
                        };
                        if earlier_inflight.contains(&(id.clone(), journal_epoch)) {
                            let _ = reply.send(Err(Error::Protocol("history page already pending".into())));
                            continue;
                        }
                        if earlier_inflight.len() >= 64 {
                            let _ = reply.send(Err(Error::Protocol("too many pending history page requests".into())));
                            continue;
                        }
                        let native = endpoint.content_kind() == SessionContentKind::NativeConversation;
                        let request = match history_request(store, &id, native) {
                            Ok(request) => request,
                            Err(error) => { let _ = reply.send(Err(error)); continue; }
                        };
                        if request.is_none() && !native {
                            let _ = reply.send(Ok(false));
                            continue;
                        }
                        let endpoint = endpoint.clone();
                        earlier_inflight.insert((id.clone(), journal_epoch));
                        earlier_reads.spawn(async move {
                            let result = if let Some(request) = request {
                                endpoint.history_page(request.clone()).await.map(|page| EarlierValue::Journal(request, page))
                            } else {
                                endpoint.load_earlier_conversation(&id, 50).await.map(EarlierValue::Native)
                            };
                            EarlierResult {
                                id, connection_epoch, journal_epoch, reply, result,
                            }
                        });
                    }
                    Command::Call { method, payload, rpc_id, expected_epoch, mut reply } => {
                        if !store.read(|state|state.ready() && expected_epoch.is_none_or(|epoch|epoch==state.connection_epoch)) || operations.len() >= 64 { let _ = reply.send(Err(Error::Disconnected)); continue; }
                        let recovery_ids = if method == "sessions.recover" && endpoint.supports_recovery() {
                            match requested_recovery_ids(&payload) {
                                Ok(ids) => Some(ids),
                                Err(error) => { let _ = reply.send(Err(error)); continue; }
                            }
                        } else { None };
                        let endpoint = endpoint.clone();
                        let removed=if matches!(method.as_str(),"session.discard"|"session.archive") {
                            let id=payload["sessionId"].as_str().unwrap_or("").to_owned();
                            if wanted.remove(&id) {
                                let was_deferred = deferred_closes.remove(&id);
                                let was_running = store.read(|s| s.sessions.get(&id).is_some_and(session_has_active_run));
                                journal_epochs.remove(&id);
                                journals.remove(&id);
                                store.update(|s|s.reset_journal(&id));
                                next_suspend_id = next_suspend_id.saturating_add(1);
                                suspended_journals.insert(id.clone(), SuspendedJournal {
                                    suspend_id: next_suspend_id,
                                    closed_while_suspended: was_deferred,
                                    was_running,
                                });
                                Some((id, next_suspend_id))
                            } else {None}
                        } else {None};
                        let recovery_tracking = unresolved_recovery.clone();
                        let recovery_store = store.clone();
                        operations.spawn(async move {
                            let recovery_admission = if let Some(ids) = recovery_ids {
                                match RecoveryAdmission::start(&recovery_tracking, ids) {
                                    Ok(admission) => {
                                        publish_pending_recovery(&recovery_store, connection_epoch, &admission.ids);
                                        Some(admission)
                                    }
                                    Err(error) => return Some((removed, None, reply, Err(error))),
                                }
                            } else { None };
                            let result = if method == "session.search" {
                                // Cancel only this specified read operation. Losing a mutation
                                // receiver must never be interpreted as cancelling remote work.
                                tokio::select! { biased;
                                    _ = reply.closed() => return None,
                                    result = invoke(endpoint.as_ref(), &method, payload, &rpc_id) => result,
                                }
                            } else { invoke(endpoint.as_ref(), &method, payload, &rpc_id).await };
                            Some((removed,recovery_admission,reply,result))
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
    journals: &mut JournalStreams,
    id: &str,
    epoch: u64,
) -> Result<()> {
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
    journals.insert(id.to_owned(), epoch, frames);
    Ok(())
}
fn history_request(
    store: &SharedProjection,
    id: &str,
    native: bool,
) -> Result<Option<HistoryRequest>> {
    store.read(|s| -> Result<Option<HistoryRequest>> {
        require(s.ready(), "endpoint not ready")?;
        let session = s
            .sessions
            .get(id)
            .ok_or_else(|| Error::Protocol("session not opened".into()))?;
        require(
            session.journal_ready && session.journal_error.is_none(),
            "journal not ready",
        )?;
        if native || !session.has_more_before || session.history_base_sequence < 0 {
            return Ok(None);
        }
        Ok(Some(HistoryRequest {
            session_id: id.into(),
            through_sequence: session.history_through_sequence,
            before_sequence: Some(session.history_base_sequence),
            max_messages: 50,
        }))
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

fn requested_recovery_ids(payload: &Value) -> Result<Vec<String>> {
    let mut seen = BTreeSet::new();
    array(payload, "sessionIds")?
        .iter()
        .map(|value| {
            let id = value
                .as_str()
                .ok_or_else(|| Error::Protocol("recovery session ID must be text".into()))?;
            require(
                !id.is_empty() && seen.insert(id.to_owned()),
                "duplicate or empty recovery session ID",
            )?;
            Ok(id.to_owned())
        })
        .collect()
}
fn recovery_outcome(
    epoch: u64,
    requested: Vec<String>,
    result: &Result<Value>,
    not_retried: Vec<String>,
) -> SessionRecoveryOutcome {
    let parsed = match result {
        Ok(receipt) => (|| {
            let ids = array(receipt, "recoveredSessionIds")?;
            let mut seen = BTreeSet::new();
            ids.iter()
                .map(|value| {
                    let id = value.as_str().ok_or_else(|| {
                        Error::Protocol("recovered session ID must be text".into())
                    })?;
                    require(
                        requested.iter().any(|requested| requested == id)
                            && seen.insert(id.to_owned()),
                        "recovery receipt has an unrequested or duplicate session ID",
                    )?;
                    Ok(id.to_owned())
                })
                .collect::<Result<Vec<_>>>()
        })(),
        Err(error) => Err(Error::Protocol(error.to_string())),
    };
    let (recovered, error) = match parsed {
        Ok(ids) => (ids, None),
        Err(error) => (Vec::new(), Some(error.to_string())),
    };
    let mut unresolved: Vec<_> = requested
        .iter()
        .filter(|id| !recovered.contains(id))
        .cloned()
        .collect();
    unresolved.extend(not_retried.iter().cloned());
    SessionRecoveryOutcome {
        connection_epoch: epoch,
        requested_session_ids: requested,
        recovered_session_ids: recovered,
        unresolved_session_ids: unresolved,
        not_retried_session_ids: not_retried,
        error,
    }
}
fn publish_pending_recovery(store: &SharedProjection, epoch: u64, ids: &[String]) {
    store.update(|state| {
        state.recovery = Some(SessionRecoveryOutcome {
            connection_epoch: epoch,
            requested_session_ids: ids.to_vec(),
            unresolved_session_ids: ids.to_vec(),
            error: Some("recovery request was admitted; its outcome is not yet confirmed".into()),
            ..Default::default()
        })
    });
}
fn remember_recovery(
    store: &SharedProjection,
    admission: RecoveryAdmission,
    outcome: SessionRecoveryOutcome,
) {
    admission.confirm(&outcome);
    store.update(|state| state.recovery = Some(outcome));
}

/// Drop releases only this consumer. Runtime shutdown does not require a live executor.
pub struct SessionHistoryLease {
    runtime: Arc<SessionRuntime>,
    handle: tokio::runtime::Handle,
    id: String,
    owner: String,
    released: bool,
}
impl SessionHistoryLease {
    pub fn session_id(&self) -> &str {
        &self.id
    }
    pub async fn release(mut self) -> Result<()> {
        self.released = true;
        self.runtime.release_history(&self.id, &self.owner).await
    }
}
impl Drop for SessionHistoryLease {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        self.released = true;
        let runtime = self.runtime.clone();
        let id = self.id.clone();
        let owner = self.owner.clone();
        self.handle.spawn(async move {
            let _ = runtime.release_history(&id, &owner).await;
        });
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

#[cfg(test)]
mod projection_revision_tests {
    use super::*;
    fn shared() -> SharedProjection {
        let (publish, _) = watch::channel(0);
        SharedProjection {
            store: RwLock::new(RevisionedProjection {
                state: MemoryProjectionStore::new("revision-test"),
                revision: 0,
            }),
            publish,
        }
    }
    #[test]
    fn revision_and_authority_remain_paired_across_concurrent_updates() {
        let shared = Arc::new(shared());
        let writers: Vec<_> = (0..4)
            .map(|_| {
                let shared = shared.clone();
                std::thread::spawn(move || {
                    for _ in 0..128 {
                        shared.update(|state| state.connection_epoch += 1);
                        shared.read_with_revision(|revision, state| {
                            assert_eq!(revision, state.connection_epoch)
                        });
                    }
                })
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }
        shared.read_with_revision(|revision, state| {
            assert_eq!(revision, 512);
            assert_eq!(state.connection_epoch, 512);
        });
    }
    #[test]
    fn partial_failed_update_invalidates_cached_authority() {
        let shared = shared();
        let result: Result<()> = shared.update(|state| {
            state.last_error = Some("visible error".into());
            Err(Error::Disconnected)
        });
        assert!(result.is_err());
        shared.read_with_revision(|revision, state| {
            assert_eq!(revision, 1);
            assert_eq!(state.last_error.as_deref(), Some("visible error"));
        });
    }
}
