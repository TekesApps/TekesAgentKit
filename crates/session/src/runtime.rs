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

type Reply<T> = oneshot::Sender<Result<T>>;
enum Command {
    Open(String, Reply<()>),
    Close(String, Reply<()>),
    Earlier(String, Reply<bool>),
    Call {
        method: String,
        payload: Value,
        rpc_id: String,
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
    pub async fn open_session(&self, id: &str) -> Result<()> {
        let (reply, result) = oneshot::channel();
        self.send(Command::Open(id.into(), reply), result).await?;
        self.wait_for(Duration::from_secs(30), |s| {
            s.sessions.get(id).is_some_and(|s| s.journal_ready)
        })
        .await
    }
    pub async fn close_session(&self, id: &str) -> Result<()> {
        let (reply, result) = oneshot::channel();
        self.send(Command::Close(id.into(), reply), result).await
    }
    pub async fn load_earlier(&self, id: &str) -> Result<bool> {
        let (reply, result) = oneshot::channel();
        self.send(Command::Earlier(id.into(), reply), result).await
    }
    pub async fn call(&self, method: &str, payload: Value, rpc_id: &str) -> Result<Value> {
        let (reply, result) = oneshot::channel();
        self.send(
            Command::Call {
                method: method.into(),
                payload,
                rpc_id: rpc_id.into(),
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
        let result = self.call("session.create", json!({"execution":{"cwd":cwd,"read_roots":[],"writable_roots":[],"toolchain_roots":[],"network":false,"allowed_tools":[],"mcp":[]}}), rpc_id).await?;
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
                } else {
                    epoch += 1;
                    store.update(|s| s.begin_connection(endpoint.host().clone()));
                    let result =
                        connected(&*endpoint, &mut commands, &store, &mut wanted, epoch).await;
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
        Command::Open(_, reply) | Command::Close(_, reply) | Command::Respond { reply, .. } => {
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
    endpoint: &dyn SessionEndpoint,
    commands: &mut mpsc::Receiver<Command>,
    store: &SharedProjection,
    wanted: &mut BTreeSet<String>,
    epoch: u64,
) -> Result<bool> {
    let mut streams = SelectAll::new();
    for (name, target) in [
        ("inventory", StreamTarget::SessionInventory),
        ("control", StreamTarget::SessionControl),
        ("actionables", StreamTarget::Actionables),
    ] {
        streams.push(tagged(name.into(), epoch, endpoint.follow(target).await?));
    }
    let mut journal_epochs = std::collections::BTreeMap::new();
    let mut journal_cancellations = std::collections::BTreeMap::new();
    let mut next_journal_epoch = epoch * 1_000_000;
    for id in wanted.iter() {
        next_journal_epoch += 1;
        let cancellation = add_journal(endpoint, &mut streams, id, next_journal_epoch).await?;
        journal_cancellations.insert(id.clone(), cancellation);
        journal_epochs.insert(id.clone(), next_journal_epoch);
    }
    let mut journal_retries = std::collections::BTreeMap::<String, usize>::new();
    let mut recovered = false;
    let baseline_deadline = tokio::time::sleep(Duration::from_secs(30));
    tokio::pin!(baseline_deadline);
    loop {
        tokio::select! {
            _ = &mut baseline_deadline, if !recovered => return Err(Error::Timeout),
            frame = streams.next() => {
                let Some((name, frame_epoch, frame)) = frame else { return Err(Error::Disconnected); };
                if let Some(id) = name.strip_prefix("journal:") {
                    if journal_epochs.get(id) != Some(&frame_epoch) { continue; }
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
                        if *attempts < 5 {
                            *attempts += 1;
                            if let Some(cancellation) = journal_cancellations.remove(id) { cancellation.abort(); }
                            store.update(|s| {
                                s.reset_journal(id);
                                if let Some(session) = s.sessions.get_mut(id) { session.journal_error = Some(error.to_string()); }
                            });
                            next_journal_epoch += 1;
                            let cancellation = add_journal(endpoint, &mut streams, id, next_journal_epoch).await?;
                            journal_cancellations.insert(id.to_owned(), cancellation);
                            journal_epochs.insert(id.to_owned(), next_journal_epoch);
                            continue;
                        }
                    }
                    return Err(error);
                }
                if is_live && let Some(id) = name.strip_prefix("journal:") { journal_retries.remove(id); }
                if store.read(MemoryProjectionStore::ready) && !recovered {
                    let ids: Vec<_> = store.read(|s| s.sessions.iter().filter(|(_, s)| s.summary.is_some()).map(|(id, _)| id.clone()).collect());
                    if !ids.is_empty() { endpoint.call("sessions.recover", json!({"sessionIds":ids}), &rpc_id()).await?; }
                    recovered = true;
                    store.update(|s| { s.connection = ConnectionState::Ready; s.last_error = None; });
                }
            }
            command = commands.recv() => {
                let Some(command) = command else { return Ok(true); };
                match command {
                    Command::Stop(reply) => { store.update(|s| { s.disconnect(); s.connection = ConnectionState::Stopped; }); let _ = reply.send(Ok(())); return Ok(true); }
                    Command::Reconnect(reply) => { store.update(MemoryProjectionStore::disconnect); let _ = reply.send(Ok(())); return Ok(false); }
                    Command::Open(id, reply) => {
                        if !store.read(MemoryProjectionStore::ready) { let _ = reply.send(Err(Error::Disconnected)); continue; }
                        let result = if wanted.contains(&id) { Ok(()) } else {
                            next_journal_epoch += 1;
                            match add_journal(endpoint, &mut streams, &id, next_journal_epoch).await {
                                Ok(cancellation) => { store.update(|s| s.reset_journal(&id)); wanted.insert(id.clone()); journal_cancellations.insert(id.clone(), cancellation); journal_epochs.insert(id, next_journal_epoch); Ok(()) }
                                Err(e) => Err(e),
                            }
                        };
                        let _ = reply.send(result);
                    }
                    Command::Close(id, reply) => {
                        wanted.remove(&id); journal_epochs.remove(&id); store.update(|s| s.reset_journal(&id));
                        if let Some(cancellation) = journal_cancellations.remove(&id) { cancellation.abort(); }
                        let _ = reply.send(Ok(()));
                    }
                    Command::Earlier(id, reply) => { let result = load_earlier(endpoint, store, &id).await; let _ = reply.send(result); }
                    Command::Call { method, payload, rpc_id, reply } => {
                        let result = if store.read(MemoryProjectionStore::ready) { endpoint.call(&method, payload, &rpc_id).await } else { Err(Error::Disconnected) };
                        let _ = reply.send(result);
                    }
                    Command::Respond { id, revision, outcome, reply } => {
                        let valid = store.read(|s| s.ready() && s.actionables.get(&id).is_some_and(|a| integer(a, "revision").ok() == Some(revision)));
                        let result = if valid { endpoint.respond(&id, revision, outcome).await } else { Err(Error::Protocol("stale or missing actionable revision".into())) };
                        let _ = reply.send(result);
                    }
                }
            }
        }
    }
}
async fn add_journal(
    endpoint: &dyn SessionEndpoint,
    streams: &mut SelectAll<TaggedStream>,
    id: &str,
    epoch: u64,
) -> Result<futures_util::future::AbortHandle> {
    let frames = endpoint
        .follow(StreamTarget::SessionJournal {
            address: JournalAddress {
                session_id: id.into(),
            },
            max_messages: 50,
        })
        .await?;
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
