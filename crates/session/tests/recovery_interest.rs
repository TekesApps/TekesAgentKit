use async_trait::async_trait;
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;
use tekes_agent_contract::*;
use tekes_agent_session::*;

#[derive(Clone)]
struct RecoveryConnector {
    connects: Arc<AtomicUsize>,
    calls: Arc<Mutex<Vec<Value>>>,
    outcome: Arc<Mutex<Outcome>>,
    drop_inventory: Arc<tokio::sync::Notify>,
    request_started: Arc<tokio::sync::Notify>,
}
#[derive(Clone)]
enum Outcome {
    All,
    Partial,
    Timeout,
    Duplicate,
    Foreign,
    Pending,
}
struct RecoveryEndpoint {
    host: HostDescription,
    source: RecoveryConnector,
    connection: usize,
}
impl RecoveryConnector {
    fn new(outcome: Outcome) -> Self {
        Self {
            connects: Arc::new(AtomicUsize::new(0)),
            calls: Arc::new(Mutex::new(Vec::new())),
            outcome: Arc::new(Mutex::new(outcome)),
            drop_inventory: Arc::new(tokio::sync::Notify::new()),
            request_started: Arc::new(tokio::sync::Notify::new()),
        }
    }
    fn requests(&self) -> Vec<Value> {
        self.calls.lock().unwrap().clone()
    }
}
#[async_trait]
impl EndpointConnector for RecoveryConnector {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        let connection = self.connects.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(Arc::new(RecoveryEndpoint {connection,host:serde_json::from_value(json!({"protocolVersion":4,"product":{"name":"Kernel","version":"0.9.22"},"capabilities":["session-journal","actionable-sync","session-control-sync","session-inventory-sync"],"cwd":"/fixture","home":"/fixture","attachedSessions":0,"canOpenPath":false})).unwrap(),source:self.clone()}))
    }
}
#[async_trait]
impl SessionEndpoint for RecoveryEndpoint {
    fn identity(&self) -> &str {
        "recovery-fixture"
    }
    fn host(&self) -> &HostDescription {
        &self.host
    }
    fn supports_recovery(&self) -> bool {
        true
    }
    fn capabilities(&self) -> EndpointCapabilities {
        EndpointCapabilities::methods(&["sessions.recover"])
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        let drops = matches!(target, StreamTarget::SessionInventory) && self.connection == 1;
        let frame = match target {
            StreamTarget::SessionInventory => {
                json!({"type":"baseline","generation":"g","snapshot":{"items":[
                    {"sessionId":"a","running":false,"blank":false,"updatedAt":1},
                    {"sessionId":"b","running":false,"blank":false,"updatedAt":1},
                    {"sessionId":"idle-inventory-only","running":false,"blank":false,"updatedAt":1},
                    {"sessionId":"detached","running":true,"blank":false,"updatedAt":1},
                    {"sessionId":"archived","running":false,"blank":false,"updatedAt":1,"status":"archived"}
                ]}})
            }
            StreamTarget::SessionControl => {
                json!({"type":"baseline","generation":"g","snapshot":{"queues":[],"jobs":[],"projections":[]}})
            }
            StreamTarget::Actionables => json!({"type":"reset","generation":"g"}),
            StreamTarget::SessionJournal { address, .. } => {
                json!({"type":"snapshot","snapshot":{"sessionId":address.session_id,"generation":"g","throughSequence":0,"windowLimit":50,"entries":[{"event":{"type":if address.session_id=="detached" {"turn/start"} else {"user/message"},"seq":0,"time":1,"data":{"text":"fixture"}}}],"hasMoreBefore":false}})
            }
            _ => return Err(unsupported("fixture stream")),
        };
        if drops {
            let drop_inventory = self.source.drop_inventory.clone();
            return Ok(Box::pin(stream::once(async { Ok(frame) }).chain(
                stream::once(async move {
                    drop_inventory.notified().await;
                    Err(Error::Disconnected)
                }),
            )));
        }
        Ok(Box::pin(
            stream::once(async { Ok(frame) }).chain(stream::pending()),
        ))
    }
    async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
        Err(unsupported("fixture history"))
    }
    async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
        Err(unsupported("fixture respond"))
    }
    async fn call(&self, method: &str, payload: Value, _: &str) -> Result<Value> {
        assert_eq!(method, "sessions.recover");
        self.source.calls.lock().unwrap().push(payload.clone());
        self.source.request_started.notify_one();
        let outcome = self.source.outcome.lock().unwrap().clone();
        match outcome {
            Outcome::All => Ok(json!({"recoveredSessionIds":payload["sessionIds"]})),
            Outcome::Partial => Ok(json!({"recoveredSessionIds":["a"]})),
            Outcome::Timeout => Err(Error::Timeout),
            Outcome::Duplicate => Ok(json!({"recoveredSessionIds":["a","a"]})),
            Outcome::Foreign => Ok(json!({"recoveredSessionIds":["not-requested"]})),
            Outcome::Pending => std::future::pending().await,
        }
    }
    async fn close(&self) {}
}
async fn runtime(source: &RecoveryConnector) -> Arc<SessionRuntime> {
    let r = Arc::new(SessionRuntime::spawn(
        "recovery-fixture",
        Arc::new(source.clone()),
    ));
    r.wait_ready(Duration::from_secs(2)).await.unwrap();
    r
}
#[tokio::test]
async fn inventory_and_first_history_read_never_release_execution() {
    let source = RecoveryConnector::new(Outcome::All);
    let r = runtime(&source).await;
    assert!(source.requests().is_empty());
    let lease = r.acquire_history("a").await.unwrap();
    assert!(
        source.requests().is_empty(),
        "history read is not activation"
    );
    lease.release().await.unwrap();
    r.reconnect().await.unwrap();
    assert!(
        source.requests().is_empty(),
        "closed interest cannot be reconstructed from the inventory"
    );
    assert!(r.inspect(|s| s.recovery.is_none()));
    r.stop().await.unwrap();
}
#[tokio::test]
async fn reconnect_recovers_only_live_interest_not_inventory_archives_or_detached_journals() {
    let source = RecoveryConnector::new(Outcome::All);
    let r = runtime(&source).await;
    let a = r.acquire_history("a").await.unwrap();
    let archive = r.acquire_history("archived").await.unwrap();
    let detached = r.acquire_history("detached").await.unwrap();
    detached.release().await.unwrap();
    assert!(r.inspect(|s| s.sessions["detached"].journal_ready && s.sessions["detached"].run_open));
    r.reconnect().await.unwrap();
    assert_eq!(source.requests(), vec![json!({"sessionIds":["a"]})]);
    let outcome = r.inspect(|s| s.recovery.clone().unwrap());
    assert_eq!(outcome.connection_epoch, r.snapshot().connection_epoch);
    assert_eq!(outcome.recovered_session_ids, ["a"]);
    assert!(outcome.unresolved_session_ids.is_empty());
    a.release().await.unwrap();
    archive.release().await.unwrap();
    r.stop().await.unwrap();
}
#[tokio::test]
async fn partial_receipt_keeps_omitted_ids_unreleased_and_does_not_retry_them() {
    let source = RecoveryConnector::new(Outcome::Partial);
    let r = runtime(&source).await;
    let a = r.acquire_history("a").await.unwrap();
    let b = r.acquire_history("b").await.unwrap();
    r.reconnect().await.unwrap();
    let receipt = r.inspect(|s| s.recovery.clone().unwrap());
    assert_eq!(receipt.requested_session_ids, ["a", "b"]);
    assert_eq!(receipt.recovered_session_ids, ["a"]);
    assert_eq!(receipt.unresolved_session_ids, ["b"]);
    assert!(receipt.error.is_none());
    *source.outcome.lock().unwrap() = Outcome::All;
    r.reconnect().await.unwrap();
    assert_eq!(
        source.requests(),
        vec![json!({"sessionIds":["a","b"]}), json!({"sessionIds":["a"]})]
    );
    assert_eq!(
        r.inspect(|s| s.recovery.as_ref().unwrap().not_retried_session_ids.clone()),
        ["b"]
    );
    a.release().await.unwrap();
    b.release().await.unwrap();
    r.stop().await.unwrap();
}
#[tokio::test]
async fn failed_or_malformed_recovery_is_visible_without_reconnect_or_automatic_retry() {
    for outcome in [Outcome::Timeout, Outcome::Duplicate, Outcome::Foreign] {
        let source = RecoveryConnector::new(outcome);
        let r = runtime(&source).await;
        let a = r.acquire_history("a").await.unwrap();
        r.reconnect().await.unwrap();
        let receipt = r.inspect(|s| s.recovery.clone().unwrap());
        assert_eq!(receipt.recovered_session_ids, Vec::<String>::new());
        assert_eq!(receipt.unresolved_session_ids, ["a"]);
        assert!(receipt.error.is_some());
        assert_eq!(r.snapshot().connection, ConnectionState::Ready);
        assert_eq!(source.connects.load(Ordering::SeqCst), 2);
        r.reconnect().await.unwrap();
        assert_eq!(
            source.requests().len(),
            1,
            "reconnecting observation is not permission to resend unknown recovery"
        );
        assert_eq!(
            r.inspect(|s| s.recovery.as_ref().unwrap().not_retried_session_ids.clone()),
            ["a"]
        );
        *source.outcome.lock().unwrap() = Outcome::All;
        let explicit = r
            .call(
                "sessions.recover",
                json!({"sessionIds":["a"]}),
                "explicit-reconcile",
            )
            .await
            .unwrap();
        assert_eq!(explicit["recoveredSessionIds"], json!(["a"]));
        assert!(r.inspect(|s| s.recovery.as_ref().unwrap().error.is_none()));
        r.reconnect().await.unwrap();
        assert_eq!(
            source.requests().len(),
            3,
            "a confirmed explicit reconciliation restores later reconnect recovery"
        );
        a.release().await.unwrap();
        r.stop().await.unwrap();
    }
}

async fn pending_recovery_is_not_replayed(reconnect_explicitly: bool) {
    let source = RecoveryConnector::new(Outcome::Pending);
    let r = runtime(&source).await;
    let lease = r.acquire_history("a").await.unwrap();
    let original_epoch = r.snapshot().connection_epoch;
    let recovering_runtime = r.clone();
    let pending = tokio::spawn(async move {
        recovering_runtime
            .call_at_epoch(
                original_epoch,
                "sessions.recover",
                json!({"sessionIds":["a"]}),
                "pending-manual",
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if source.requests().len() == 1 {
                break;
            }
            source.request_started.notified().await;
        }
    })
    .await
    .unwrap();
    let admitted = r.inspect(|state| state.recovery.clone().unwrap());
    assert_eq!(admitted.requested_session_ids, ["a"]);
    assert_eq!(admitted.unresolved_session_ids, ["a"]);
    assert!(admitted.recovered_session_ids.is_empty());
    assert!(admitted.error.is_some());
    assert!(
        r.call(
            "sessions.recover",
            json!({"sessionIds":["a"]}),
            "overlap-is-rejected"
        )
        .await
        .is_err()
    );
    assert_eq!(
        source.requests().len(),
        1,
        "an overlapping pending recovery is not another dispatch"
    );
    // The old RPC stays pending; only a new dispatch could observe this success mode.
    *source.outcome.lock().unwrap() = Outcome::All;
    if reconnect_explicitly {
        r.reconnect().await.unwrap();
    } else {
        source.drop_inventory.notify_one();
        let mut changes = r.subscribe();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if source.connects.load(Ordering::SeqCst) >= 2
                    && r.inspect(|state| {
                        state.connection == ConnectionState::Ready
                            && state.connection_epoch != original_epoch
                    })
                {
                    break;
                }
                changes.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
    }
    assert!(pending.await.unwrap().is_err());
    assert_eq!(
        source.requests(),
        vec![json!({"sessionIds":["a"]})],
        "a request that reached the endpoint before losing its receipt is never replayed by reconnect"
    );
    let receipt = r.inspect(|state| state.recovery.clone().unwrap());
    assert_eq!(receipt.connection_epoch, r.snapshot().connection_epoch);
    assert_eq!(receipt.not_retried_session_ids, ["a"]);
    assert_eq!(receipt.unresolved_session_ids, ["a"]);
    lease.release().await.unwrap();
    r.stop().await.unwrap();
}
#[tokio::test]
async fn stream_failure_while_recovery_rpc_is_pending_never_replays_the_admitted_request() {
    pending_recovery_is_not_replayed(false).await;
}
#[tokio::test]
async fn explicit_reconnect_while_recovery_rpc_is_pending_never_replays_the_admitted_request() {
    pending_recovery_is_not_replayed(true).await;
}
#[tokio::test]
async fn failed_epoch_preflight_does_not_suppress_the_first_real_recovery_dispatch() {
    let source = RecoveryConnector::new(Outcome::All);
    let r = runtime(&source).await;
    let lease = r.acquire_history("a").await.unwrap();
    let epoch = r.snapshot().connection_epoch;
    assert!(
        r.call_at_epoch(
            epoch + 1,
            "sessions.recover",
            json!({"sessionIds":["a"]}),
            "stale-not-admitted"
        )
        .await
        .is_err()
    );
    assert!(source.requests().is_empty());
    r.reconnect().await.unwrap();
    assert_eq!(source.requests(), vec![json!({"sessionIds":["a"]})]);
    assert!(r.inspect(|state| {
        state
            .recovery
            .as_ref()
            .unwrap()
            .not_retried_session_ids
            .is_empty()
    }));
    lease.release().await.unwrap();
    r.stop().await.unwrap();
}

type JournalTxMap =
    std::collections::BTreeMap<(usize, String), tokio::sync::mpsc::UnboundedSender<Result<Value>>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Conn2JournalFailure {
    None,
    TransientDisconnected,
    PermanentRemote,
}

#[derive(Clone)]
struct ControlledReconnectConnector {
    connects: Arc<AtomicUsize>,
    connect2_gate: Arc<tokio::sync::Notify>,
    hold_inventory_conn2: Arc<std::sync::atomic::AtomicBool>,
    inventory_follow_entered: Arc<tokio::sync::Notify>,
    inventory_follow_release: Arc<tokio::sync::Notify>,
    conn2_a_failure: Arc<Mutex<Conn2JournalFailure>>,
    followed_journals: Arc<Mutex<Vec<(usize, String)>>>,
    closed_journals: Arc<Mutex<Vec<(usize, String)>>>,
    recovery_calls: Arc<Mutex<Vec<Value>>>,
    drop_inventory: Arc<tokio::sync::Notify>,
    journal_tx: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<Result<Value>>>>>,
    journal_txs: Arc<Mutex<JournalTxMap>>,
    active_run: Arc<std::sync::atomic::AtomicBool>,
}

impl ControlledReconnectConnector {
    fn new() -> Self {
        Self {
            connects: Arc::new(AtomicUsize::new(0)),
            connect2_gate: Arc::new(tokio::sync::Notify::new()),
            hold_inventory_conn2: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            inventory_follow_entered: Arc::new(tokio::sync::Notify::new()),
            inventory_follow_release: Arc::new(tokio::sync::Notify::new()),
            conn2_a_failure: Arc::new(Mutex::new(Conn2JournalFailure::None)),
            followed_journals: Arc::new(Mutex::new(Vec::new())),
            closed_journals: Arc::new(Mutex::new(Vec::new())),
            recovery_calls: Arc::new(Mutex::new(Vec::new())),
            drop_inventory: Arc::new(tokio::sync::Notify::new()),
            journal_tx: Arc::new(Mutex::new(None)),
            journal_txs: Arc::new(Mutex::new(std::collections::BTreeMap::new())),
            active_run: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
}

struct ControlledReconnectEndpoint {
    host: HostDescription,
    source: ControlledReconnectConnector,
    connection: usize,
}

#[async_trait]
impl EndpointConnector for ControlledReconnectConnector {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        let connection = self.connects.fetch_add(1, Ordering::SeqCst) + 1;
        if connection == 2 {
            self.connect2_gate.notified().await;
        }
        Ok(Arc::new(ControlledReconnectEndpoint {
            connection,
            host: serde_json::from_value(json!({
                "protocolVersion": 4,
                "product": {"name": "Kernel", "version": "0.9.23"},
                "capabilities": [
                    "session-journal",
                    "actionable-sync",
                    "session-control-sync",
                    "session-inventory-sync"
                ],
                "cwd": "/fixture",
                "home": "/fixture",
                "attachedSessions": 0,
                "canOpenPath": false
            }))
            .unwrap(),
            source: self.clone(),
        }))
    }
}

#[async_trait]
impl SessionEndpoint for ControlledReconnectEndpoint {
    fn identity(&self) -> &str {
        "controlled-reconnect"
    }
    fn host(&self) -> &HostDescription {
        &self.host
    }
    fn supports_recovery(&self) -> bool {
        true
    }
    fn capabilities(&self) -> EndpointCapabilities {
        EndpointCapabilities::methods(&["sessions.recover"])
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        match target {
            StreamTarget::SessionInventory => {
                if self.connection == 2 && self.source.hold_inventory_conn2.load(Ordering::SeqCst) {
                    self.source.inventory_follow_entered.notify_waiters();
                    self.source.inventory_follow_release.notified().await;
                }
                let frame = json!({
                    "type": "baseline",
                    "generation": "g",
                    "snapshot": {
                        "items": [
                            {"sessionId": "a", "running": self.source.active_run.load(Ordering::SeqCst), "blank": false, "updatedAt": 1},
                            {"sessionId": "b", "running": false, "blank": false, "updatedAt": 1},
                            {"sessionId": "idle", "running": false, "blank": false, "updatedAt": 1}
                        ]
                    }
                });
                if self.connection == 1 {
                    let drop_inv = self.source.drop_inventory.clone();
                    return Ok(Box::pin(stream::once(async { Ok(frame) }).chain(
                        stream::once(async move {
                            drop_inv.notified().await;
                            Err(Error::Disconnected)
                        }),
                    )));
                }
                Ok(Box::pin(
                    stream::once(async { Ok(frame) }).chain(stream::pending()),
                ))
            }
            StreamTarget::SessionControl => {
                let frame = json!({"type": "baseline", "generation": "g", "snapshot": {"queues": [], "jobs": [], "projections": []}});
                Ok(Box::pin(
                    stream::once(async { Ok(frame) }).chain(stream::pending()),
                ))
            }
            StreamTarget::Actionables => {
                let frame = json!({"type": "reset", "generation": "g"});
                Ok(Box::pin(
                    stream::once(async { Ok(frame) }).chain(stream::pending()),
                ))
            }
            StreamTarget::SessionJournal { address, .. } => {
                if self.connection == 2 && address.session_id == "a" {
                    let failure = *self.source.conn2_a_failure.lock().unwrap();
                    match failure {
                        Conn2JournalFailure::TransientDisconnected => {
                            return Err(Error::Disconnected);
                        }
                        Conn2JournalFailure::PermanentRemote => {
                            return Err(Error::Remote(json!({"code": "session-quarantined"})));
                        }
                        Conn2JournalFailure::None => {}
                    }
                }
                struct JournalDrop {
                    conn: usize,
                    sid: String,
                    closed_journals: Arc<Mutex<Vec<(usize, String)>>>,
                }
                impl Drop for JournalDrop {
                    fn drop(&mut self) {
                        self.closed_journals
                            .lock()
                            .unwrap()
                            .push((self.conn, self.sid.clone()));
                    }
                }
                self.source
                    .followed_journals
                    .lock()
                    .unwrap()
                    .push((self.connection, address.session_id.clone()));
                let running =
                    address.session_id == "a" && self.source.active_run.load(Ordering::SeqCst);
                let text = if address.session_id == "b" {
                    "hist-b"
                } else {
                    "hi"
                };
                let entries = if running {
                    vec![
                        json!({"event": {"type": "user/message", "seq": 0, "time": 1, "data": {"text": text}}}),
                        json!({"event": {"type": "turn/start", "seq": 1, "time": 2, "data": {"turn": 1}}}),
                    ]
                } else {
                    vec![
                        json!({"event": {"type": "user/message", "seq": 0, "time": 1, "data": {"text": text}}}),
                    ]
                };
                let through = entries.len().saturating_sub(1);
                let snapshot = json!({
                    "type": "snapshot",
                    "snapshot": {
                        "sessionId": address.session_id,
                        "generation": "g",
                        "throughSequence": through,
                        "windowLimit": 50,
                        "entries": entries,
                        "hasMoreBefore": false
                    }
                });
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                *self.source.journal_tx.lock().unwrap() = Some(tx.clone());
                self.source
                    .journal_txs
                    .lock()
                    .unwrap()
                    .insert((self.connection, address.session_id.clone()), tx);
                let guard = JournalDrop {
                    conn: self.connection,
                    sid: address.session_id,
                    closed_journals: self.source.closed_journals.clone(),
                };
                let tail = stream::unfold((rx, guard), |(mut rx, guard)| async move {
                    rx.recv().await.map(|item| (item, (rx, guard)))
                });
                Ok(Box::pin(stream::once(async { Ok(snapshot) }).chain(tail)))
            }
            _ => Err(unsupported("controlled stream")),
        }
    }
    async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
        Err(unsupported("controlled history"))
    }
    async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
        Err(unsupported("controlled respond"))
    }
    async fn call(&self, method: &str, payload: Value, _: &str) -> Result<Value> {
        assert_eq!(method, "sessions.recover");
        self.source
            .recovery_calls
            .lock()
            .unwrap()
            .push(payload.clone());
        Ok(json!({"recoveredSessionIds": payload["sessionIds"]}))
    }
    async fn close(&self) {}
}

#[tokio::test]
async fn releasing_last_idle_observer_while_disconnected_suppresses_follow_and_recovery_on_reconnect()
 {
    let source = ControlledReconnectConnector::new();
    let r = Arc::new(SessionRuntime::spawn(
        "controlled-reconnect",
        Arc::new(source.clone()),
    ));
    r.wait_ready(Duration::from_secs(2)).await.unwrap();

    let lease_a = r.acquire_history("a").await.unwrap();
    assert!(r.inspect(|s| s.sessions["a"].journal_ready));

    // Trigger disconnect and hold reconnect behind connect2_gate.
    source.drop_inventory.notify_one();
    let mut sub = r.subscribe();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if r.inspect(|s| s.connection != ConnectionState::Ready) {
                break;
            }
            sub.changed().await.unwrap();
        }
    })
    .await
    .unwrap();

    // Release last idle observer while disconnected -> must return Ok(()) and remove from wanted.
    lease_a.release().await.unwrap();

    // Unblock reconnect and wait for Ready.
    source.connect2_gate.notify_one();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if source.connects.load(Ordering::SeqCst) >= 2
                && r.inspect(|s| s.connection == ConnectionState::Ready)
            {
                break;
            }
            sub.changed().await.unwrap();
        }
    })
    .await
    .unwrap();

    let follows_conn2: Vec<_> = source
        .followed_journals
        .lock()
        .unwrap()
        .iter()
        .filter(|(conn, sid)| *conn == 2 && sid == "a")
        .cloned()
        .collect();
    assert!(
        follows_conn2.is_empty(),
        "released idle session A must not be re-followed on reconnect: {follows_conn2:?}"
    );
    assert!(
        source.recovery_calls.lock().unwrap().is_empty(),
        "released idle session A must not be recovered on reconnect"
    );
    assert!(!r.inspect(|s| s.sessions.get("a").is_some_and(|sess| sess.journal_ready)));
    r.stop().await.unwrap();
}

async fn wait_store(r: &SessionRuntime, pred: impl Fn(&MemoryProjectionStore) -> bool) {
    let mut sub = r.subscribe();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if r.inspect(&pred) {
                return;
            }
            sub.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn two_observers_keep_stream_on_partial_release_and_settle_running_session_before_close() {
    let source = ControlledReconnectConnector::new();
    source.active_run.store(true, Ordering::SeqCst);
    let r = Arc::new(SessionRuntime::spawn(
        "controlled-reconnect",
        Arc::new(source.clone()),
    ));
    r.wait_ready(Duration::from_secs(2)).await.unwrap();

    let lease1 = r.acquire_history("a").await.unwrap();
    let lease2 = r.acquire_history("a").await.unwrap();

    // Releasing first observer leaves second observer active and receiving events.
    lease1.release().await.unwrap();
    assert!(r.inspect(|s| s.sessions["a"].journal_ready && s.sessions["a"].run_open));

    let tx = source.journal_tx.lock().unwrap().clone().unwrap();
    tx.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {
            "event": {
                "type": "assistant/message",
                "seq": 2,
                "time": 3,
                "data": {"turn": 1, "step": 1, "message": {"id": "m1", "content": [{"type": "text", "text": "still-alive"}]}}
            }
        }
    })))
    .unwrap();
    wait_store(&r, |s| {
        s.sessions.get("a").is_some_and(|sess| {
            sess.entries
                .iter()
                .any(|(_, e)| e.event.data["message"]["content"][0]["text"] == "still-alive")
        })
    })
    .await;

    // Releasing last observer while A is still running defers close until A settles.
    lease2.release().await.unwrap();
    assert!(r.inspect(|s| s.sessions["a"].journal_ready && s.sessions["a"].run_open));

    tx.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {
            "event": {
                "type": "turn/end",
                "seq": 3,
                "time": 4,
                "data": {"turn": 1, "reason": "completed"}
            }
        }
    })))
    .unwrap();

    wait_store(&r, |s| {
        s.sessions
            .get("a")
            .is_some_and(|sess| !sess.journal_ready && !sess.run_open)
    })
    .await;
    r.stop().await.unwrap();
}

#[tokio::test]
async fn releasing_active_session_during_reconnect_handshake_waits_for_real_turn_end_while_b_is_ready()
 {
    let source = ControlledReconnectConnector::new();
    source.active_run.store(true, Ordering::SeqCst);
    let r = Arc::new(SessionRuntime::spawn(
        "controlled-reconnect",
        Arc::new(source.clone()),
    ));
    r.wait_ready(Duration::from_secs(2)).await.unwrap();

    let lease_a = r.acquire_history("a").await.unwrap();
    assert!(r.inspect(|s| s.sessions["a"].journal_ready && s.sessions["a"].run_open));

    source.hold_inventory_conn2.store(true, Ordering::SeqCst);
    source.drop_inventory.notify_one();
    wait_store(&r, |s| s.connection != ConnectionState::Ready).await;

    // Unblock connect2 so runtime enters follow(SessionInventory) handshake and pauses there.
    let entered = source.inventory_follow_entered.notified();
    source.connect2_gate.notify_one();
    tokio::time::timeout(Duration::from_secs(2), entered)
        .await
        .unwrap();
    assert!(r.inspect(|s| {
        s.connection == ConnectionState::Connecting
            && s.sessions["a"].run_open
            && !s.sessions["a"].journal_ready
    }));

    // Release last observer of active A during the blocked inventory handshake.
    let release_handle = tokio::spawn(async move { lease_a.release().await });
    tokio::time::sleep(Duration::from_millis(25)).await;

    // Complete handshake and immediately acquire B.
    source.inventory_follow_release.notify_one();
    wait_store(&r, |s| s.connection == ConnectionState::Ready).await;
    let lease_b = r.acquire_history("b").await.unwrap();
    assert!(r.inspect(|s| {
        s.sessions["b"].journal_ready
            && s.sessions["b"]
                .entries
                .values()
                .all(|e| e.event.data["text"] == "hist-b")
    }));

    // Active A must be reopened on conn 2 and NOT closed before its real turn/end arrives.
    assert!(
        source
            .followed_journals
            .lock()
            .unwrap()
            .contains(&(2, "a".to_owned()))
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !source
            .closed_journals
            .lock()
            .unwrap()
            .contains(&(2, "a".to_owned())),
        "active A journal must stay open across handshake close until real turn/end arrives"
    );
    assert!(r.inspect(|s| s.sessions["a"].journal_ready && s.sessions["a"].run_open));

    // Deliver real turn/end on conn 2 A journal -> runtime closes A journal cleanly afterward.
    let tx_a = source.journal_txs.lock().unwrap()[&(2, "a".to_owned())].clone();
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {
            "event": {
                "type": "turn/end",
                "seq": 2,
                "time": 4,
                "data": {"turn": 1, "reason": "completed"}
            }
        }
    })))
    .unwrap();
    wait_store(&r, |s| {
        s.sessions
            .get("a")
            .is_some_and(|sess| !sess.journal_ready && !sess.run_open)
    })
    .await;
    assert!(r.inspect(|s| {
        s.sessions["a"]
            .entries
            .values()
            .any(|e| e.event.kind == "turn/end")
    }));
    tokio::time::timeout(Duration::from_secs(2), async {
        while !source
            .closed_journals
            .lock()
            .unwrap()
            .contains(&(2, "a".to_owned()))
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    release_handle.await.unwrap().unwrap();
    assert!(r.inspect(|s| s.sessions["b"].journal_ready));
    lease_b.release().await.unwrap();
    r.stop().await.unwrap();
}

#[tokio::test]
async fn releasing_idle_session_during_reconnect_handshake_closes_immediately_and_second_observer_survives()
 {
    let source = ControlledReconnectConnector::new();
    source.active_run.store(true, Ordering::SeqCst);
    let r = Arc::new(SessionRuntime::spawn(
        "controlled-reconnect",
        Arc::new(source.clone()),
    ));
    r.wait_ready(Duration::from_secs(2)).await.unwrap();

    let lease_a1 = r.acquire_history("a").await.unwrap();
    let lease_a2 = r.acquire_history("a").await.unwrap();
    let lease_idle = r.acquire_history("idle").await.unwrap();

    source.hold_inventory_conn2.store(true, Ordering::SeqCst);
    source.drop_inventory.notify_one();
    wait_store(&r, |s| s.connection != ConnectionState::Ready).await;

    let entered = source.inventory_follow_entered.notified();
    source.connect2_gate.notify_one();
    tokio::time::timeout(Duration::from_secs(2), entered)
        .await
        .unwrap();

    // Release last observer of idle session AND one of two observers of running session A.
    let rel_idle = tokio::spawn(async move { lease_idle.release().await });
    let rel_a1 = tokio::spawn(async move { lease_a1.release().await });
    tokio::time::sleep(Duration::from_millis(25)).await;

    source.inventory_follow_release.notify_one();
    wait_store(&r, |s| s.connection == ConnectionState::Ready).await;
    rel_idle.await.unwrap().unwrap();
    rel_a1.await.unwrap().unwrap();

    // Idle session must close promptly on conn 2 while second observer keeps A alive through turn/end.
    tokio::time::timeout(Duration::from_secs(2), async {
        while !source
            .closed_journals
            .lock()
            .unwrap()
            .contains(&(2, "idle".to_owned()))
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(!r.inspect(|s| s.sessions["idle"].journal_ready));

    let tx_a = source.journal_txs.lock().unwrap()[&(2, "a".to_owned())].clone();
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {
            "event": {
                "type": "turn/end",
                "seq": 2,
                "time": 4,
                "data": {"turn": 1, "reason": "completed"}
            }
        }
    })))
    .unwrap();
    wait_store(&r, |s| {
        s.sessions["a"].journal_ready && !s.sessions["a"].run_open
    })
    .await;
    assert!(
        !source
            .closed_journals
            .lock()
            .unwrap()
            .contains(&(2, "a".to_owned())),
        "second observer must keep A stream open even after turn/end"
    );

    lease_a2.release().await.unwrap();
    wait_store(&r, |s| !s.sessions["a"].journal_ready).await;
    r.stop().await.unwrap();
}

#[tokio::test]
async fn double_reconnect_transient_journal_error_keeps_active_a_until_conn3_turn_end_while_b_is_ready()
 {
    let source = ControlledReconnectConnector::new();
    source.active_run.store(true, Ordering::SeqCst);
    *source.conn2_a_failure.lock().unwrap() = Conn2JournalFailure::TransientDisconnected;
    let r = Arc::new(SessionRuntime::spawn(
        "controlled-reconnect",
        Arc::new(source.clone()),
    ));
    r.wait_ready(Duration::from_secs(2)).await.unwrap();

    let lease_a = r.acquire_history("a").await.unwrap();
    assert!(r.inspect(|s| s.sessions["a"].journal_ready && s.sessions["a"].run_open));

    source.drop_inventory.notify_one();
    wait_store(&r, |s| s.connection != ConnectionState::Ready).await;
    lease_a.release().await.unwrap();

    // Unblock conn 2; conn 2's add_journal("a") fails transiently with Error::Disconnected,
    // causing runtime to reconnect automatically on conn 3 while retaining active A in wanted.
    source.connect2_gate.notify_one();
    wait_store(&r, |s| {
        s.connection == ConnectionState::Ready && source.connects.load(Ordering::SeqCst) >= 3
    })
    .await;

    let lease_b = r.acquire_history("b").await.unwrap();
    assert!(r.inspect(|s| {
        s.sessions["b"].journal_ready
            && s.sessions["b"]
                .entries
                .values()
                .all(|e| e.event.data["text"] == "hist-b")
    }));
    assert!(
        source
            .followed_journals
            .lock()
            .unwrap()
            .contains(&(3, "a".to_owned())),
        "conn 3 must reopen active A after conn 2 transient drop"
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !source
            .closed_journals
            .lock()
            .unwrap()
            .contains(&(3, "a".to_owned())),
        "active A must stay open on conn 3 until real turn/end arrives"
    );
    assert!(r.inspect(|s| s.sessions["a"].journal_ready && s.sessions["a"].run_open));

    let tx_a = source.journal_txs.lock().unwrap()[&(3, "a".to_owned())].clone();
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {
            "event": {
                "type": "turn/end",
                "seq": 2,
                "time": 4,
                "data": {"turn": 1, "reason": "completed"}
            }
        }
    })))
    .unwrap();
    wait_store(&r, |s| {
        s.sessions
            .get("a")
            .is_some_and(|sess| !sess.journal_ready && !sess.run_open)
    })
    .await;
    assert!(r.inspect(|s| {
        s.sessions["a"]
            .entries
            .values()
            .any(|e| e.event.kind == "turn/end")
    }));
    tokio::time::timeout(Duration::from_secs(2), async {
        while !source
            .closed_journals
            .lock()
            .unwrap()
            .contains(&(3, "a".to_owned()))
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(r.inspect(|s| s.sessions["b"].journal_ready));
    lease_b.release().await.unwrap();
    r.stop().await.unwrap();
}

#[tokio::test]
async fn permanent_remote_journal_error_retires_deferred_active_a_without_blocking_b() {
    let source = ControlledReconnectConnector::new();
    source.active_run.store(true, Ordering::SeqCst);
    *source.conn2_a_failure.lock().unwrap() = Conn2JournalFailure::PermanentRemote;
    let r = Arc::new(SessionRuntime::spawn(
        "controlled-reconnect",
        Arc::new(source.clone()),
    ));
    r.wait_ready(Duration::from_secs(2)).await.unwrap();

    let lease_a = r.acquire_history("a").await.unwrap();
    source.drop_inventory.notify_one();
    wait_store(&r, |s| s.connection != ConnectionState::Ready).await;
    lease_a.release().await.unwrap();

    source.connect2_gate.notify_one();
    wait_store(&r, |s| {
        s.connection == ConnectionState::Ready && source.connects.load(Ordering::SeqCst) == 2
    })
    .await;
    assert!(r.inspect(|s| {
        !s.sessions["a"].journal_ready
            && s.sessions["a"]
                .journal_error
                .as_deref()
                .is_some_and(|e| e.contains("session-quarantined"))
    }));

    let lease_b = r.acquire_history("b").await.unwrap();
    assert!(r.inspect(|s| s.sessions["b"].journal_ready));

    // Reconnecting again must NOT retry permanently rejected deferred A, while B recovers.
    r.reconnect().await.unwrap();
    let conn3_follows: Vec<_> = source
        .followed_journals
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, _)| *c == 3)
        .cloned()
        .collect();
    assert_eq!(conn3_follows, vec![(3, "b".to_owned())]);
    assert!(r.inspect(|s| s.sessions["b"].journal_ready && !s.sessions["a"].journal_ready));
    lease_b.release().await.unwrap();
    r.stop().await.unwrap();
}

#[tokio::test]
async fn releasing_history_lease_synchronously_drops_journal_stream_before_returning() {
    let source = ControlledReconnectConnector::new();
    let r = Arc::new(SessionRuntime::spawn(
        "controlled-reconnect",
        Arc::new(source.clone()),
    ));
    r.wait_ready(Duration::from_secs(2)).await.unwrap();

    let lease_a = r.acquire_history("a").await.unwrap();
    assert!(r.inspect(|s| s.sessions["a"].journal_ready));
    assert!(source.closed_journals.lock().unwrap().is_empty());

    lease_a.release().await.unwrap();
    assert!(
        source
            .closed_journals
            .lock()
            .unwrap()
            .contains(&(1, "a".to_owned())),
        "FrameStream must drop synchronously inside Command::Close before release().await returns"
    );
    assert!(r.inspect(|s| !s.sessions.get("a").is_some_and(|sess| sess.journal_ready)));
    r.stop().await.unwrap();
}

type JournalSenderMap = Arc<
    Mutex<
        std::collections::BTreeMap<
            (usize, String),
            tokio::sync::mpsc::UnboundedSender<Result<Value>>,
        >,
    >,
>;

#[derive(Clone)]
struct ArchiveSwitchConnector {
    connects: Arc<AtomicUsize>,
    followed_journals: Arc<Mutex<Vec<(usize, String)>>>,
    closed_journals: Arc<Mutex<Vec<(usize, String)>>>,
    recover_calls: Arc<Mutex<Vec<Value>>>,
    running_a: Arc<std::sync::atomic::AtomicBool>,
    park_archive: Arc<std::sync::atomic::AtomicBool>,
    archive_entered: Arc<tokio::sync::Notify>,
    archive_release: Arc<tokio::sync::Notify>,
    defer_archive_mutation: Arc<std::sync::atomic::AtomicBool>,
    archive_should_succeed: Arc<std::sync::atomic::AtomicBool>,
    archived_a: Arc<std::sync::atomic::AtomicBool>,
    fail_journal_a: Arc<std::sync::atomic::AtomicBool>,
    archive_calls: Arc<AtomicUsize>,
    inventory_tx: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<Result<Value>>>>>,
    journal_txs: JournalSenderMap,
    control_txs: Arc<
        Mutex<std::collections::BTreeMap<usize, tokio::sync::mpsc::UnboundedSender<Result<Value>>>>,
    >,
}

impl ArchiveSwitchConnector {
    fn new(running_a: bool, archive_should_succeed: bool) -> Self {
        Self {
            connects: Arc::new(AtomicUsize::new(0)),
            followed_journals: Arc::new(Mutex::new(Vec::new())),
            closed_journals: Arc::new(Mutex::new(Vec::new())),
            recover_calls: Arc::new(Mutex::new(Vec::new())),
            running_a: Arc::new(std::sync::atomic::AtomicBool::new(running_a)),
            park_archive: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            archive_entered: Arc::new(tokio::sync::Notify::new()),
            archive_release: Arc::new(tokio::sync::Notify::new()),
            defer_archive_mutation: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            archive_should_succeed: Arc::new(std::sync::atomic::AtomicBool::new(
                archive_should_succeed,
            )),
            archived_a: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            fail_journal_a: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            archive_calls: Arc::new(AtomicUsize::new(0)),
            inventory_tx: Arc::new(Mutex::new(None)),
            journal_txs: Arc::new(Mutex::new(std::collections::BTreeMap::new())),
            control_txs: Arc::new(Mutex::new(std::collections::BTreeMap::new())),
        }
    }
}

struct ArchiveSwitchEndpoint {
    connection: usize,
    host: HostDescription,
    source: ArchiveSwitchConnector,
}

#[async_trait]
impl EndpointConnector for ArchiveSwitchConnector {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        let connection = self.connects.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(Arc::new(ArchiveSwitchEndpoint {
            connection,
            host: serde_json::from_value(json!({
                "protocolVersion": 4,
                "product": {"name": "Kernel", "version": "0.9.24"},
                "capabilities": ["session-journal", "actionable-sync", "session-control-sync", "session-inventory-sync"],
                "cwd": "/fixture",
                "home": "/fixture",
                "attachedSessions": 0,
                "canOpenPath": false
            }))
            .unwrap(),
            source: self.clone(),
        }))
    }
}

#[async_trait]
impl SessionEndpoint for ArchiveSwitchEndpoint {
    fn identity(&self) -> &str {
        "archive-switch"
    }
    fn host(&self) -> &HostDescription {
        &self.host
    }
    fn supports_recovery(&self) -> bool {
        true
    }
    fn capabilities(&self) -> EndpointCapabilities {
        EndpointCapabilities::methods(&["sessions.recover", "session.archive"])
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        match target {
            StreamTarget::SessionInventory => {
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                *self.source.inventory_tx.lock().unwrap() = Some(tx);
                let running = self.source.running_a.load(Ordering::Acquire);
                let mut items = Vec::new();
                if !self.source.archived_a.load(Ordering::Acquire) {
                    items.push(json!({"sessionId": "a", "running": running, "blank": false, "updatedAt": 10}));
                }
                items.push(
                    json!({"sessionId": "b", "running": false, "blank": false, "updatedAt": 20}),
                );
                let baseline = json!({
                    "type": "baseline",
                    "generation": "g",
                    "snapshot": {
                        "items": items
                    }
                });
                let tail = stream::unfold(rx, |mut rx| async move {
                    rx.recv().await.map(|item| (item, rx))
                });
                Ok(Box::pin(stream::once(async { Ok(baseline) }).chain(tail)))
            }
            StreamTarget::SessionControl => {
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                self.source
                    .control_txs
                    .lock()
                    .unwrap()
                    .insert(self.connection, tx);
                let baseline = json!({"type": "baseline", "generation": "g", "snapshot": {"queues": [], "jobs": [], "projections": []}});
                let tail = stream::unfold(rx, |mut rx| async move {
                    rx.recv().await.map(|item| (item, rx))
                });
                Ok(Box::pin(stream::once(async { Ok(baseline) }).chain(tail)))
            }
            StreamTarget::Actionables => {
                let reset = json!({"type": "reset", "generation": "g"});
                Ok(Box::pin(
                    stream::once(async { Ok(reset) }).chain(stream::pending()),
                ))
            }
            StreamTarget::SessionJournal { address, .. } => {
                struct DropGuard {
                    conn: usize,
                    sid: String,
                    closed: Arc<Mutex<Vec<(usize, String)>>>,
                }
                impl Drop for DropGuard {
                    fn drop(&mut self) {
                        self.closed
                            .lock()
                            .unwrap()
                            .push((self.conn, self.sid.clone()));
                    }
                }
                let sid = address.session_id;
                self.source
                    .followed_journals
                    .lock()
                    .unwrap()
                    .push((self.connection, sid.clone()));
                if sid == "a"
                    && (self.source.archived_a.load(Ordering::Acquire)
                        || self.source.fail_journal_a.load(Ordering::Acquire))
                {
                    return Err(Error::Remote("stream-open-failed".into()));
                }
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                self.source
                    .journal_txs
                    .lock()
                    .unwrap()
                    .insert((self.connection, sid.clone()), tx);
                let running = sid == "a" && self.source.running_a.load(Ordering::Acquire);
                let entries = if running {
                    vec![
                        json!({"event": {"type": "turn/start", "seq": 0, "time": 1, "data": {"turn": 1}}}),
                    ]
                } else {
                    vec![
                        json!({"event": {"type": "user/message", "seq": 0, "time": 1, "data": {"text": "idle"}}}),
                    ]
                };
                let snap = json!({
                    "type": "snapshot",
                    "snapshot": {
                        "sessionId": sid,
                        "generation": "g",
                        "throughSequence": 0,
                        "windowLimit": 50,
                        "entries": entries,
                        "hasMoreBefore": false
                    }
                });
                let guard = DropGuard {
                    conn: self.connection,
                    sid,
                    closed: self.source.closed_journals.clone(),
                };
                let tail = stream::unfold((rx, guard), |(mut rx, guard)| async move {
                    rx.recv().await.map(|item| (item, (rx, guard)))
                });
                Ok(Box::pin(stream::once(async { Ok(snap) }).chain(tail)))
            }
            _ => Err(unsupported("unsupported stream")),
        }
    }
    async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
        Err(unsupported("unsupported history"))
    }
    async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
        Err(unsupported("unsupported respond"))
    }
    async fn call(&self, method: &str, payload: Value, _: &str) -> Result<Value> {
        match method {
            "session.archive" => {
                self.source.archive_calls.fetch_add(1, Ordering::SeqCst);
                let should_succeed = self.source.archive_should_succeed.load(Ordering::Acquire);
                let deferred = self.source.defer_archive_mutation.load(Ordering::Acquire);
                if should_succeed && !deferred {
                    self.source.archived_a.store(true, Ordering::Release);
                }
                if self.source.park_archive.swap(false, Ordering::AcqRel) {
                    let release = self.source.archive_release.notified();
                    tokio::pin!(release);
                    release.as_mut().enable();
                    self.source.archive_entered.notify_one();
                    release.await;
                }
                if should_succeed {
                    Ok(json!({ "archivedSessionIds": ["a"] }))
                } else {
                    Err(Error::Remote("session-running".into()))
                }
            }
            "sessions.recover" => {
                self.source
                    .recover_calls
                    .lock()
                    .unwrap()
                    .push(payload.clone());
                Ok(json!({ "recoveredSessionIds": payload["sessionIds"] }))
            }
            other => Err(unsupported(other)),
        }
    }
    async fn close(&self) {}
}

#[tokio::test]
async fn rejected_archive_after_switch_observes_running_turn_then_drops_from_reconnect_and_recover()
{
    let source = ArchiveSwitchConnector::new(true, false);
    let r = Arc::new(SessionRuntime::spawn(
        "archive-switch",
        Arc::new(source.clone()),
    ));
    r.wait_ready(Duration::from_secs(2)).await.unwrap();

    let lease_a = r.acquire_history("a").await.unwrap();
    assert!(r.inspect(|s| s.sessions["a"].journal_ready && s.sessions["a"].run_open));

    let entered = source.archive_entered.notified();
    let archive_task = tokio::spawn({
        let r = r.clone();
        async move { r.archive("a", "rpc-archive-a").await }
    });
    entered.await;

    // Switch away to B while archive("a") is in flight: lease_a drops, B becomes ready.
    lease_a.release().await.unwrap();
    let lease_b = r.acquire_history("b").await.unwrap();
    assert!(r.inspect(|s| s.sessions["b"].journal_ready && !s.sessions["a"].journal_ready));

    // Release rejected archive response: A's journal reopens ONLY under deferred close to receive turn/end.
    source.archive_release.notify_one();
    assert!(archive_task.await.unwrap().is_err());
    wait_store(&r, |s| {
        s.sessions["a"].journal_ready && s.sessions["a"].run_open
    })
    .await;

    // Deliver A's turn/end -> A retires immediately, while B remains ready.
    source.running_a.store(false, Ordering::Release);
    let tx_a = source.journal_txs.lock().unwrap()[&(1, "a".to_owned())].clone();
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {"event": {"type": "turn/end", "seq": 1, "time": 2, "data": {"turn": 1, "reason": "completed"}}}
    })))
    .unwrap();
    wait_store(&r, |s| !s.sessions["a"].journal_ready).await;
    assert!(r.inspect(|s| s.sessions["b"].journal_ready));

    // Reconnect: ONLY B is opened on mux and requested in sessions.recover.
    source.recover_calls.lock().unwrap().clear();
    r.reconnect().await.unwrap();
    let conn2_journals: Vec<_> = source
        .followed_journals
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, _)| *c == 2)
        .map(|(_, sid)| sid.clone())
        .collect();
    assert_eq!(conn2_journals, vec!["b".to_owned()]);
    let conn2_recovers = source.recover_calls.lock().unwrap().clone();
    assert_eq!(conn2_recovers, vec![json!({ "sessionIds": ["b"] })]);
    assert!(r.inspect(|s| s.sessions["b"].journal_ready && !s.sessions["a"].journal_ready));

    lease_b.release().await.unwrap();
    r.stop().await.unwrap();
}

#[tokio::test]
async fn rejected_archive_after_switch_on_idle_session_does_not_reopen_journal() {
    let source = ArchiveSwitchConnector::new(false, false);
    let r = Arc::new(SessionRuntime::spawn(
        "archive-switch",
        Arc::new(source.clone()),
    ));
    r.wait_ready(Duration::from_secs(2)).await.unwrap();

    let lease_a = r.acquire_history("a").await.unwrap();
    let entered = source.archive_entered.notified();
    let archive_task = tokio::spawn({
        let r = r.clone();
        async move { r.archive("a", "rpc-archive-a").await }
    });
    entered.await;

    lease_a.release().await.unwrap();
    let lease_b = r.acquire_history("b").await.unwrap();
    let opens_before = source.followed_journals.lock().unwrap().len();
    source.archive_release.notify_one();
    assert!(archive_task.await.unwrap().is_err());

    r.reconnect().await.unwrap();
    let new_opens = source.followed_journals.lock().unwrap()[opens_before..].to_vec();
    assert_eq!(new_opens, vec![(2, "b".to_owned())]);
    assert_eq!(
        source.recover_calls.lock().unwrap().last(),
        Some(&json!({ "sessionIds": ["b"] }))
    );
    assert!(r.inspect(|s| !s.sessions["a"].journal_ready && s.sessions["b"].journal_ready));

    lease_b.release().await.unwrap();
    r.stop().await.unwrap();
}

#[tokio::test]
async fn rejected_archive_while_staying_on_session_restores_journal() {
    let source = ArchiveSwitchConnector::new(true, false);
    let r = Arc::new(SessionRuntime::spawn(
        "archive-switch",
        Arc::new(source.clone()),
    ));
    r.wait_ready(Duration::from_secs(2)).await.unwrap();

    let lease_a = r.acquire_history("a").await.unwrap();
    let entered = source.archive_entered.notified();
    let archive_task = tokio::spawn({
        let r = r.clone();
        async move { r.archive("a", "rpc-archive-a").await }
    });
    entered.await;
    assert!(r.inspect(|s| !s.sessions["a"].journal_ready));

    source.archive_release.notify_one();
    assert!(archive_task.await.unwrap().is_err());
    wait_store(&r, |s| {
        s.sessions["a"].journal_ready && s.sessions["a"].run_open
    })
    .await;

    source.recover_calls.lock().unwrap().clear();
    r.reconnect().await.unwrap();
    let conn2_journals: Vec<_> = source
        .followed_journals
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, _)| *c == 2)
        .map(|(_, sid)| sid.clone())
        .collect();
    assert_eq!(conn2_journals, vec!["a".to_owned()]);
    assert_eq!(
        source.recover_calls.lock().unwrap().as_slice(),
        &[json!({ "sessionIds": ["a"] })]
    );

    lease_a.release().await.unwrap();
    r.stop().await.unwrap();
}

#[tokio::test]
async fn accepted_archive_after_switch_preserves_other_sessions_journal() {
    let source = ArchiveSwitchConnector::new(true, true);
    let r = Arc::new(SessionRuntime::spawn(
        "archive-switch",
        Arc::new(source.clone()),
    ));
    r.wait_ready(Duration::from_secs(2)).await.unwrap();

    let lease_a = r.acquire_history("a").await.unwrap();
    let entered = source.archive_entered.notified();
    let archive_task = tokio::spawn({
        let r = r.clone();
        async move { r.archive("a", "rpc-archive-a").await }
    });
    entered.await;

    lease_a.release().await.unwrap();
    let lease_b = r.acquire_history("b").await.unwrap();
    source.archive_release.notify_one();
    assert!(archive_task.await.unwrap().is_ok());

    r.reconnect().await.unwrap();
    let conn2_journals: Vec<_> = source
        .followed_journals
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, _)| *c == 2)
        .map(|(_, sid)| sid.clone())
        .collect();
    assert_eq!(conn2_journals, vec!["b".to_owned()]);
    assert_eq!(
        source.recover_calls.lock().unwrap().last(),
        Some(&json!({ "sessionIds": ["b"] }))
    );
    assert!(r.inspect(|s| s.sessions["b"].journal_ready && !s.sessions["a"].journal_ready));

    lease_b.release().await.unwrap();
    r.stop().await.unwrap();
}

#[tokio::test]
async fn archive_in_flight_explicit_reconnect_restores_journal_and_recovery_without_replay() {
    let source = ArchiveSwitchConnector::new(true, false);
    let r = Arc::new(SessionRuntime::spawn(
        "archive-switch",
        Arc::new(source.clone()),
    ));
    r.wait_ready(Duration::from_secs(2)).await.unwrap();

    let lease_a = r.acquire_history("a").await.unwrap();
    assert!(r.inspect(|s| s.sessions["a"].journal_ready && s.sessions["a"].run_open));

    let entered = source.archive_entered.notified();
    let archive_task = tokio::spawn({
        let r = r.clone();
        async move { r.archive("a", "rpc-archive-a").await }
    });
    entered.await;
    assert!(r.inspect(|s| !s.sessions["a"].journal_ready));

    source.recover_calls.lock().unwrap().clear();
    r.reconnect().await.unwrap();
    assert!(matches!(
        archive_task.await.unwrap(),
        Err(Error::Disconnected)
    ));
    source.archive_release.notify_one();

    wait_store(&r, |s| {
        s.sessions["a"].journal_ready && s.sessions["a"].run_open
    })
    .await;
    let conn2_journals: Vec<_> = source
        .followed_journals
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, _)| *c == 2)
        .map(|(_, sid)| sid.clone())
        .collect();
    assert_eq!(conn2_journals, vec!["a".to_owned()]);
    assert_eq!(
        source.recover_calls.lock().unwrap().as_slice(),
        &[json!({ "sessionIds": ["a"] })]
    );
    assert_eq!(source.archive_calls.load(Ordering::SeqCst), 1);

    source.running_a.store(false, Ordering::Release);
    let tx_a = source.journal_txs.lock().unwrap()[&(2, "a".to_owned())].clone();
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {"event": {"type": "turn/end", "seq": 1, "time": 2, "data": {"turn": 1, "reason": "completed"}}}
    })))
    .unwrap();
    wait_store(&r, |s| !s.sessions["a"].run_open).await;

    lease_a.release().await.unwrap();
    r.stop().await.unwrap();
}

#[tokio::test]
async fn archive_in_flight_transport_drop_restores_journal_and_recovery_without_replay() {
    let source = ArchiveSwitchConnector::new(true, false);
    let r = Arc::new(SessionRuntime::spawn(
        "archive-switch",
        Arc::new(source.clone()),
    ));
    r.wait_ready(Duration::from_secs(2)).await.unwrap();

    let lease_a = r.acquire_history("a").await.unwrap();
    assert!(r.inspect(|s| s.sessions["a"].journal_ready && s.sessions["a"].run_open));

    let entered = source.archive_entered.notified();
    let archive_task = tokio::spawn({
        let r = r.clone();
        async move { r.archive("a", "rpc-archive-a").await }
    });
    entered.await;
    assert!(r.inspect(|s| !s.sessions["a"].journal_ready));

    source.recover_calls.lock().unwrap().clear();
    let ctrl_tx = source.control_txs.lock().unwrap()[&1].clone();
    ctrl_tx.send(Err(Error::Disconnected)).unwrap();

    assert!(matches!(
        archive_task.await.unwrap(),
        Err(Error::Disconnected)
    ));
    source.archive_release.notify_one();

    wait_store(&r, |s| {
        s.sessions["a"].journal_ready && s.sessions["a"].run_open
    })
    .await;
    let conn2_journals: Vec<_> = source
        .followed_journals
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, _)| *c == 2)
        .map(|(_, sid)| sid.clone())
        .collect();
    assert_eq!(conn2_journals, vec!["a".to_owned()]);
    assert_eq!(
        source.recover_calls.lock().unwrap().as_slice(),
        &[json!({ "sessionIds": ["a"] })]
    );
    assert_eq!(source.archive_calls.load(Ordering::SeqCst), 1);

    source.running_a.store(false, Ordering::Release);
    let tx_a = source.journal_txs.lock().unwrap()[&(2, "a".to_owned())].clone();
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {"event": {"type": "turn/end", "seq": 1, "time": 2, "data": {"turn": 1, "reason": "completed"}}}
    })))
    .unwrap();
    wait_store(&r, |s| !s.sessions["a"].run_open).await;

    lease_a.release().await.unwrap();
    r.stop().await.unwrap();
}

#[tokio::test]
async fn archive_in_flight_switch_running_then_transport_drop_observes_turn_end_then_closes() {
    let source = ArchiveSwitchConnector::new(true, false);
    let r = Arc::new(SessionRuntime::spawn(
        "archive-switch",
        Arc::new(source.clone()),
    ));
    r.wait_ready(Duration::from_secs(2)).await.unwrap();

    let lease_a = r.acquire_history("a").await.unwrap();
    assert!(r.inspect(|s| s.sessions["a"].journal_ready && s.sessions["a"].run_open));

    let entered = source.archive_entered.notified();
    let archive_task = tokio::spawn({
        let r = r.clone();
        async move { r.archive("a", "rpc-archive-a").await }
    });
    entered.await;

    lease_a.release().await.unwrap();
    let lease_b = r.acquire_history("b").await.unwrap();
    assert!(r.inspect(|s| s.sessions["b"].journal_ready && !s.sessions["a"].journal_ready));

    source.recover_calls.lock().unwrap().clear();
    let ctrl_tx = source.control_txs.lock().unwrap()[&1].clone();
    ctrl_tx.send(Err(Error::Disconnected)).unwrap();

    assert!(matches!(
        archive_task.await.unwrap(),
        Err(Error::Disconnected)
    ));
    source.archive_release.notify_one();

    wait_store(&r, |s| {
        s.sessions["b"].journal_ready && s.sessions["a"].journal_ready && s.sessions["a"].run_open
    })
    .await;
    assert_eq!(
        source.recover_calls.lock().unwrap().as_slice(),
        &[json!({ "sessionIds": ["b"] })]
    );
    assert_eq!(source.archive_calls.load(Ordering::SeqCst), 1);

    source.running_a.store(false, Ordering::Release);
    let tx_a = source.journal_txs.lock().unwrap()[&(2, "a".to_owned())].clone();
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {"event": {"type": "turn/end", "seq": 1, "time": 2, "data": {"turn": 1, "reason": "completed"}}}
    })))
    .unwrap();
    wait_store(&r, |s| !s.sessions["a"].journal_ready).await;

    source.recover_calls.lock().unwrap().clear();
    r.reconnect().await.unwrap();
    let conn3_journals: Vec<_> = source
        .followed_journals
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, _)| *c == 3)
        .map(|(_, sid)| sid.clone())
        .collect();
    assert_eq!(conn3_journals, vec!["b".to_owned()]);
    assert_eq!(
        source.recover_calls.lock().unwrap().as_slice(),
        &[json!({ "sessionIds": ["b"] })]
    );

    lease_b.release().await.unwrap();
    r.stop().await.unwrap();
}

#[tokio::test]
async fn archive_in_flight_switch_idle_then_transport_drop_never_reopens_idle_a() {
    let source = ArchiveSwitchConnector::new(false, false);
    let r = Arc::new(SessionRuntime::spawn(
        "archive-switch",
        Arc::new(source.clone()),
    ));
    r.wait_ready(Duration::from_secs(2)).await.unwrap();

    let lease_a = r.acquire_history("a").await.unwrap();
    let entered = source.archive_entered.notified();
    let archive_task = tokio::spawn({
        let r = r.clone();
        async move { r.archive("a", "rpc-archive-a").await }
    });
    entered.await;

    lease_a.release().await.unwrap();
    let lease_b = r.acquire_history("b").await.unwrap();
    let opens_before = source.followed_journals.lock().unwrap().len();
    source.recover_calls.lock().unwrap().clear();

    let ctrl_tx = source.control_txs.lock().unwrap()[&1].clone();
    ctrl_tx.send(Err(Error::Disconnected)).unwrap();

    assert!(matches!(
        archive_task.await.unwrap(),
        Err(Error::Disconnected)
    ));
    source.archive_release.notify_one();
    wait_store(&r, |s| s.sessions["b"].journal_ready).await;

    let new_opens = source.followed_journals.lock().unwrap()[opens_before..].to_vec();
    assert_eq!(new_opens, vec![(2, "b".to_owned())]);
    assert_eq!(
        source.recover_calls.lock().unwrap().as_slice(),
        &[json!({ "sessionIds": ["b"] })]
    );
    assert!(r.inspect(|s| !s.sessions["a"].journal_ready && s.sessions["b"].journal_ready));

    lease_b.release().await.unwrap();
    r.stop().await.unwrap();
}

#[tokio::test]
async fn archive_in_flight_succeeds_then_transport_drop_drops_wanted_and_never_reopens_or_recovers()
{
    let source = ArchiveSwitchConnector::new(false, true);
    let r = Arc::new(SessionRuntime::spawn(
        "archive-switch",
        Arc::new(source.clone()),
    ));
    r.wait_ready(Duration::from_secs(2)).await.unwrap();

    let lease_a = r.acquire_history("a").await.unwrap();
    assert!(r.inspect(|s| s.sessions["a"].journal_ready));

    let entered = source.archive_entered.notified();
    let archive_task = tokio::spawn({
        let r = r.clone();
        async move { r.archive("a", "rpc-archive-a").await }
    });
    entered.await;
    assert!(source.archived_a.load(Ordering::Acquire));

    source.recover_calls.lock().unwrap().clear();
    let ctrl_tx = source.control_txs.lock().unwrap()[&1].clone();
    ctrl_tx.send(Err(Error::Disconnected)).unwrap();
    assert!(matches!(
        archive_task.await.unwrap(),
        Err(Error::Disconnected)
    ));
    source.archive_release.notify_one();

    wait_store(&r, |s| {
        s.connection == ConnectionState::Ready
            && s.connection_epoch >= 2
            && s.sessions
                .get("a")
                .and_then(|p| p.summary.as_ref())
                .is_none()
    })
    .await;

    // Neither connection 2 nor connection 3 may attempt to open session-journal(a) or recover A.
    r.reconnect().await.unwrap();
    r.wait_ready(Duration::from_secs(2)).await.unwrap();

    let post_conn1_opens: Vec<_> = source
        .followed_journals
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, _)| *c >= 2)
        .cloned()
        .collect();
    assert!(
        post_conn1_opens.is_empty(),
        "unexpected opens after archive: {post_conn1_opens:?}"
    );
    assert!(source.recover_calls.lock().unwrap().is_empty());
    assert!(r.inspect(|s| {
        s.sessions
            .get("a")
            .and_then(|p| p.journal_error.as_ref())
            .is_none()
    }));
    assert_eq!(source.archive_calls.load(Ordering::SeqCst), 1);

    drop(lease_a);
    r.stop().await.unwrap();
}

#[tokio::test]
async fn remote_journal_error_on_active_session_never_retires_wanted_without_archive() {
    let source = ArchiveSwitchConnector::new(false, false);
    let r = Arc::new(SessionRuntime::spawn(
        "archive-switch",
        Arc::new(source.clone()),
    ));
    r.wait_ready(Duration::from_secs(2)).await.unwrap();

    let lease_a = r.acquire_history("a").await.unwrap();
    wait_store(&r, |s| s.sessions["a"].journal_ready).await;

    let tx_a = source.journal_txs.lock().unwrap()[&(1, "a".to_owned())].clone();
    tx_a.send(Err(Error::Remote("stream-open-failed".into())))
        .unwrap();
    wait_store(&r, |s| s.sessions["a"].journal_error.is_some()).await;
    assert!(r.inspect(|s| {
        s.sessions["a"].summary.is_some() && !s.archived_session_ids.iter().any(|id| id == "a")
    }));

    source.recover_calls.lock().unwrap().clear();
    r.reconnect().await.unwrap();
    wait_store(&r, |s| {
        s.sessions["a"].journal_ready && s.sessions["a"].journal_error.is_none()
    })
    .await;
    assert_eq!(
        source.recover_calls.lock().unwrap().as_slice(),
        &[json!({ "sessionIds": ["a"] })]
    );

    lease_a.release().await.unwrap();
    r.stop().await.unwrap();
}

#[tokio::test]
async fn archive_committed_after_reconnect_ready_retires_live_journal_and_never_reopens() {
    let source = ArchiveSwitchConnector::new(false, true);
    source.defer_archive_mutation.store(true, Ordering::Release);
    let r = Arc::new(SessionRuntime::spawn(
        "archive-switch",
        Arc::new(source.clone()),
    ));
    r.wait_ready(Duration::from_secs(2)).await.unwrap();

    let lease_a = r.acquire_history("a").await.unwrap();
    wait_store(&r, |s| s.sessions["a"].journal_ready).await;

    let entered = source.archive_entered.notified();
    let archive_task = tokio::spawn({
        let r = r.clone();
        async move { r.archive("a", "rpc-archive-a").await }
    });
    entered.await;
    assert!(!source.archived_a.load(Ordering::Acquire));

    source.recover_calls.lock().unwrap().clear();
    let ctrl_tx = source.control_txs.lock().unwrap()[&1].clone();
    ctrl_tx.send(Err(Error::Disconnected)).unwrap();
    assert!(matches!(
        archive_task.await.unwrap(),
        Err(Error::Disconnected)
    ));

    // Conn 2 reaches Ready before Kernel processes session.archive(a), so A legitimately opens & recovers on conn 2.
    wait_store(&r, |s| {
        s.connection == ConnectionState::Ready
            && s.connection_epoch >= 2
            && s.sessions["a"].journal_ready
    })
    .await;
    assert_eq!(
        source.recover_calls.lock().unwrap().as_slice(),
        &[json!({ "sessionIds": ["a"] })]
    );

    // Now the in-flight HTTP archive commits against Kernel and emits inventory remove(a) on conn 2.
    source.recover_calls.lock().unwrap().clear();
    source.archived_a.store(true, Ordering::Release);
    source
        .inventory_tx
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .send(Ok(json!({
            "type": "remove",
            "generation": "g",
            "sessionId": "a"
        })))
        .unwrap();
    source.archive_release.notify_one();

    wait_store(&r, |s| {
        !s.sessions["a"].journal_ready
            && s.sessions["a"].journal_error.is_none()
            && s.sessions["a"].summary.is_none()
    })
    .await;

    // Subsequent reconnect must NOT open or recover A.
    r.reconnect().await.unwrap();
    wait_store(&r, |s| {
        s.connection == ConnectionState::Ready && s.connection_epoch >= 3
    })
    .await;

    let conn3_opens: Vec<_> = source
        .followed_journals
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, _)| *c >= 3)
        .cloned()
        .collect();
    assert!(
        conn3_opens.is_empty(),
        "unexpected conn3 opens: {conn3_opens:?}"
    );
    assert!(source.recover_calls.lock().unwrap().is_empty());
    assert_eq!(source.archive_calls.load(Ordering::SeqCst), 1);

    drop(lease_a);
    r.stop().await.unwrap();
}

#[tokio::test]
async fn archive_committed_offline_after_second_outage_retires_before_conn3_journal_open() {
    let source = ArchiveSwitchConnector::new(false, true);
    source.defer_archive_mutation.store(true, Ordering::Release);
    let r = Arc::new(SessionRuntime::spawn(
        "archive-switch",
        Arc::new(source.clone()),
    ));
    r.wait_ready(Duration::from_secs(2)).await.unwrap();

    let lease_a = r.acquire_history("a").await.unwrap();
    let lease_b = r.acquire_history("b").await.unwrap();
    wait_store(&r, |s| {
        s.sessions["a"].journal_ready && s.sessions["b"].journal_ready
    })
    .await;

    let entered = source.archive_entered.notified();
    let archive_task = tokio::spawn({
        let r = r.clone();
        async move { r.archive("a", "rpc-archive-a").await }
    });
    entered.await;
    assert!(!source.archived_a.load(Ordering::Acquire));

    // Outage 1: drop conn 1 before Kernel commits session.archive(a).
    source.recover_calls.lock().unwrap().clear();
    let ctrl_tx1 = source.control_txs.lock().unwrap()[&1].clone();
    ctrl_tx1.send(Err(Error::Disconnected)).unwrap();
    assert!(matches!(
        archive_task.await.unwrap(),
        Err(Error::Disconnected)
    ));

    // Conn 2 becomes Ready while A is still active on Kernel: B opens normally and A opens + recovers.
    wait_store(&r, |s| {
        s.connection == ConnectionState::Ready
            && s.connection_epoch >= 2
            && s.sessions["a"].journal_ready
            && s.sessions["b"].journal_ready
    })
    .await;
    let conn2_opens: Vec<_> = source
        .followed_journals
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, _)| *c == 2)
        .map(|(_, sid)| sid.clone())
        .collect();
    assert!(conn2_opens.contains(&"a".to_owned()));
    assert!(conn2_opens.contains(&"b".to_owned()));
    assert_eq!(source.recover_calls.lock().unwrap().len(), 1);
    assert!(
        source.recover_calls.lock().unwrap()[0]["sessionIds"]
            .as_array()
            .unwrap()
            .contains(&json!("a"))
    );

    // Outage 2: drop conn 2, then archive commits offline before conn 3 connects.
    source.recover_calls.lock().unwrap().clear();
    let ctrl_tx2 = source.control_txs.lock().unwrap()[&2].clone();
    ctrl_tx2.send(Err(Error::Disconnected)).unwrap();
    wait_store(&r, |s| s.connection != ConnectionState::Ready).await;
    source.archived_a.store(true, Ordering::Release);
    source.archive_release.notify_one();

    // Conn 3 becomes Ready: A must be retired from conn 3's baseline with ZERO open or recover calls.
    wait_store(&r, |s| {
        s.connection == ConnectionState::Ready
            && s.connection_epoch >= 3
            && s.sessions["b"].journal_ready
    })
    .await;
    let conn3_opens: Vec<_> = source
        .followed_journals
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, _)| *c == 3)
        .map(|(_, sid)| sid.clone())
        .collect();
    assert_eq!(
        conn3_opens,
        vec!["b".to_owned()],
        "unexpected conn3 opens (A should not open): {conn3_opens:?}"
    );
    assert_eq!(
        source.recover_calls.lock().unwrap().as_slice(),
        &[json!({ "sessionIds": ["b"] })]
    );
    assert!(r.inspect(|s| {
        !s.sessions["a"].journal_ready
            && s.sessions["a"].journal_error.is_none()
            && s.sessions["a"].summary.is_none()
    }));

    // Conn 4 reconnect also never opens or recovers A.
    source.recover_calls.lock().unwrap().clear();
    r.reconnect().await.unwrap();
    wait_store(&r, |s| {
        s.connection == ConnectionState::Ready
            && s.connection_epoch >= 4
            && s.sessions["b"].journal_ready
    })
    .await;
    let post_conn2_a_opens: Vec<_> = source
        .followed_journals
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, sid)| *c >= 3 && sid == "a")
        .cloned()
        .collect();
    assert!(
        post_conn2_a_opens.is_empty(),
        "unexpected post-archive A opens: {post_conn2_a_opens:?}"
    );
    assert_eq!(
        source.recover_calls.lock().unwrap().as_slice(),
        &[json!({ "sessionIds": ["b"] })]
    );
    assert_eq!(source.archive_calls.load(Ordering::SeqCst), 1);

    drop(lease_a);
    drop(lease_b);
    r.stop().await.unwrap();
}

#[tokio::test]
async fn archive_committed_offline_after_reselecting_session_on_conn2_retires_without_conn3_open() {
    let source = ArchiveSwitchConnector::new(false, true);
    source.defer_archive_mutation.store(true, Ordering::Release);
    let r = Arc::new(SessionRuntime::spawn(
        "archive-switch",
        Arc::new(source.clone()),
    ));
    r.wait_ready(Duration::from_secs(2)).await.unwrap();

    let lease_a = r.acquire_history("a").await.unwrap();
    wait_store(&r, |s| s.sessions["a"].journal_ready).await;

    let entered = source.archive_entered.notified();
    let archive_task = tokio::spawn({
        let r = r.clone();
        async move { r.archive("a", "rpc-archive-a").await }
    });
    entered.await;
    assert!(!source.archived_a.load(Ordering::Acquire));

    // Outage 1: drop conn 1 before Kernel commits session.archive(a).
    source.recover_calls.lock().unwrap().clear();
    let ctrl_tx1 = source.control_txs.lock().unwrap()[&1].clone();
    ctrl_tx1.send(Err(Error::Disconnected)).unwrap();
    assert!(matches!(
        archive_task.await.unwrap(),
        Err(Error::Disconnected)
    ));

    // Conn 2 becomes Ready while A is still active on Kernel: A opens & recovers cleanly.
    wait_store(&r, |s| {
        s.connection == ConnectionState::Ready
            && s.connection_epoch >= 2
            && s.sessions["a"].journal_ready
    })
    .await;

    // On conn 2, user temporarily switches to B (idle close releases A's history), then reselects A.
    drop(lease_a);
    wait_store(&r, |s| !s.sessions["a"].journal_ready).await;
    let lease_b = r.acquire_history("b").await.unwrap();
    wait_store(&r, |s| s.sessions["b"].journal_ready).await;
    assert!(
        source
            .closed_journals
            .lock()
            .unwrap()
            .contains(&(2, "a".to_owned()))
    );

    drop(lease_b);
    let lease_a = r.acquire_history("a").await.unwrap();
    wait_store(&r, |s| {
        s.sessions["a"].journal_ready && !s.sessions["b"].journal_ready
    })
    .await;
    let conn2_a_opens = source
        .followed_journals
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, sid)| *c == 2 && sid == "a")
        .count();
    assert_eq!(conn2_a_opens, 2);

    // Outage 2: drop conn 2, then archive commits offline before conn 3 connects.
    source.recover_calls.lock().unwrap().clear();
    let ctrl_tx2 = source.control_txs.lock().unwrap()[&2].clone();
    ctrl_tx2.send(Err(Error::Disconnected)).unwrap();
    wait_store(&r, |s| s.connection != ConnectionState::Ready).await;
    source.archived_a.store(true, Ordering::Release);
    source.archive_release.notify_one();

    // Conn 3 becomes Ready: A must be retired from conn 3's baseline with ZERO open/recover calls.
    wait_store(&r, |s| {
        s.connection == ConnectionState::Ready && s.connection_epoch >= 3
    })
    .await;
    let conn3_a_opens: Vec<_> = source
        .followed_journals
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, sid)| *c == 3 && sid == "a")
        .cloned()
        .collect();
    assert!(
        conn3_a_opens.is_empty(),
        "unexpected conn3 opens for A after reselection: {conn3_a_opens:?}"
    );
    assert!(source.recover_calls.lock().unwrap().is_empty());
    assert!(r.inspect(|s| {
        !s.sessions["a"].journal_ready
            && s.sessions["a"].journal_error.is_none()
            && s.sessions["a"].summary.is_none()
    }));

    // Conn 4 reconnect also never opens/recovers A or surfaces journal_error.
    r.reconnect().await.unwrap();
    wait_store(&r, |s| {
        s.connection == ConnectionState::Ready && s.connection_epoch >= 4
    })
    .await;
    let post_conn2_a_opens: Vec<_> = source
        .followed_journals
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, sid)| *c >= 3 && sid == "a")
        .cloned()
        .collect();
    assert!(
        post_conn2_a_opens.is_empty(),
        "unexpected post-archive A opens: {post_conn2_a_opens:?}"
    );
    assert!(source.recover_calls.lock().unwrap().is_empty());
    assert!(r.inspect(|s| {
        !s.sessions["a"].journal_ready
            && s.sessions["a"].journal_error.is_none()
            && s.sessions["a"].summary.is_none()
    }));
    assert_eq!(source.archive_calls.load(Ordering::SeqCst), 1);

    drop(lease_a);
    r.stop().await.unwrap();
}
