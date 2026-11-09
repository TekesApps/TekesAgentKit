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
