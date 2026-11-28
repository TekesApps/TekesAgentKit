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
use tekes_agent_presentation::{ComposerDestination, RunController, RunScope};
use tekes_agent_session::{MemoryProjectionStore, SessionRuntime};
use tekes_agent_ui_bindings::{
    ComposerQuoteRequest, NoopWakeNotifier, RunWorkbenchBinding, UiWakeNotifier,
};
use tokio::sync::{Notify, mpsc};

#[derive(Clone)]
struct BlockerTestEndpoint {
    inner: Arc<Mutex<EndpointState>>,
    model_gate_a: Arc<Notify>,
    approval_gate_a: Arc<Notify>,
    connect_count: Arc<AtomicUsize>,
    block_reconnect: Arc<AtomicBool>,
    reconnecting: Arc<Notify>,
    reconnect_permit: Arc<Notify>,
    hold_inventory_conn2: Arc<AtomicBool>,
    inventory_follow_entered: Arc<Notify>,
    inventory_follow_release: Arc<Notify>,
    hold_journal_b: Arc<AtomicBool>,
    journal_b_entered: Arc<Notify>,
    journal_b_release: Arc<Notify>,
    cancel_disconnect_before_reply: Arc<AtomicBool>,
    cancel_entered: Arc<Notify>,
    hold_cancel: Arc<AtomicBool>,
    cancel_gate_entered: Arc<Notify>,
    cancel_release: Arc<Notify>,
    park_reject_cancel: Arc<AtomicBool>,
    park_reject_entered: Arc<Notify>,
    park_reject_release: Arc<Notify>,
    park_archive: Arc<AtomicBool>,
    park_archive_entered: Arc<Notify>,
    park_archive_release: Arc<Notify>,
    defer_archive_mutation: Arc<AtomicBool>,
    supports_recovery: Arc<AtomicBool>,
}

type RenameGate = (Arc<Notify>, Arc<Notify>, std::result::Result<(), String>);

struct EndpointState {
    rpcs: Vec<(String, Value)>,
    journal_txs: BTreeMap<String, mpsc::UnboundedSender<Result<Value>>>,
    journal_txs_by_conn: BTreeMap<(usize, String), mpsc::UnboundedSender<Result<Value>>>,
    journal_entries: BTreeMap<String, Vec<Value>>,
    journal_opens: Vec<(usize, String)>,
    journal_closes: Vec<(usize, String)>,
    inventory_tx: Option<mpsc::UnboundedSender<Result<Value>>>,
    running: BTreeMap<String, bool>,
    custom_items: Option<Vec<Value>>,
    control_projections: Vec<Value>,
    archived_ids: Vec<String>,
    use_workspace_stream: bool,
    rename_seq: i64,
    reject_rename: bool,
    rename_gates: BTreeMap<String, RenameGate>,
    reject_archive: bool,
    reject_unarchive: bool,
    reject_cancel: bool,
    transport_error_cancel: bool,
    reject_queue_update: bool,
    model_delay_a: bool,
    approval_delay_a: bool,
    closed: Vec<String>,
}

impl BlockerTestEndpoint {
    fn new() -> Self {
        let mut running = BTreeMap::new();
        running.insert("a".to_owned(), false);
        running.insert("b".to_owned(), false);
        running.insert("c".to_owned(), false);
        let mut journal_entries = BTreeMap::new();
        journal_entries.insert(
            "a".to_owned(),
            vec![json!({"event": {"type": "user/message", "seq": 0, "time": 1, "data": {"text": "init-a"}}})],
        );
        journal_entries.insert(
            "b".to_owned(),
            vec![json!({"event": {"type": "user/message", "seq": 0, "time": 1, "data": {"text": "init-b"}}})],
        );
        journal_entries.insert(
            "c".to_owned(),
            vec![json!({"event": {"type": "user/message", "seq": 0, "time": 1, "data": {"text": "init-c"}}})],
        );
        Self {
            inner: Arc::new(Mutex::new(EndpointState {
                rpcs: Vec::new(),
                journal_txs: BTreeMap::new(),
                journal_txs_by_conn: BTreeMap::new(),
                journal_entries,
                journal_opens: Vec::new(),
                journal_closes: Vec::new(),
                inventory_tx: None,
                running,
                custom_items: None,
                control_projections: Vec::new(),
                archived_ids: Vec::new(),
                use_workspace_stream: false,
                rename_seq: 10,
                reject_rename: false,
                rename_gates: BTreeMap::new(),
                reject_archive: false,
                reject_unarchive: false,
                reject_cancel: false,
                transport_error_cancel: false,
                reject_queue_update: false,
                model_delay_a: false,
                approval_delay_a: false,
                closed: Vec::new(),
            })),
            model_gate_a: Arc::new(Notify::new()),
            approval_gate_a: Arc::new(Notify::new()),
            connect_count: Arc::new(AtomicUsize::new(0)),
            block_reconnect: Arc::new(AtomicBool::new(false)),
            reconnecting: Arc::new(Notify::new()),
            reconnect_permit: Arc::new(Notify::new()),
            hold_inventory_conn2: Arc::new(AtomicBool::new(false)),
            inventory_follow_entered: Arc::new(Notify::new()),
            inventory_follow_release: Arc::new(Notify::new()),
            hold_journal_b: Arc::new(AtomicBool::new(false)),
            journal_b_entered: Arc::new(Notify::new()),
            journal_b_release: Arc::new(Notify::new()),
            cancel_disconnect_before_reply: Arc::new(AtomicBool::new(false)),
            cancel_entered: Arc::new(Notify::new()),
            hold_cancel: Arc::new(AtomicBool::new(false)),
            cancel_gate_entered: Arc::new(Notify::new()),
            cancel_release: Arc::new(Notify::new()),
            park_reject_cancel: Arc::new(AtomicBool::new(false)),
            park_reject_entered: Arc::new(Notify::new()),
            park_reject_release: Arc::new(Notify::new()),
            park_archive: Arc::new(AtomicBool::new(false)),
            park_archive_entered: Arc::new(Notify::new()),
            park_archive_release: Arc::new(Notify::new()),
            defer_archive_mutation: Arc::new(AtomicBool::new(false)),
            supports_recovery: Arc::new(AtomicBool::new(false)),
        }
    }

    fn rpcs(&self) -> Vec<(String, Value)> {
        self.inner.lock().unwrap().rpcs.clone()
    }

    fn drop_connection(&self) {
        self.block_reconnect.store(true, Ordering::Release);
        let tx = self.inner.lock().unwrap().inventory_tx.take();
        if let Some(tx) = tx {
            let _ = tx.send(Err(Error::Disconnected));
        }
    }

    fn unblock_reconnect(&self) {
        self.block_reconnect.store(false, Ordering::Release);
        self.reconnect_permit.notify_waiters();
    }

    fn journal_opens(&self) -> Vec<(usize, String)> {
        self.inner.lock().unwrap().journal_opens.clone()
    }

    fn journal_closes(&self) -> Vec<(usize, String)> {
        self.inner.lock().unwrap().journal_closes.clone()
    }
}

struct TestSessionEndpoint {
    conn: usize,
    host: HostDescription,
    source: BlockerTestEndpoint,
}

#[async_trait]
impl EndpointConnector for BlockerTestEndpoint {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        let conn = self.connect_count.fetch_add(1, Ordering::SeqCst) + 1;
        if conn > 1 && self.block_reconnect.load(Ordering::Acquire) {
            self.reconnecting.notify_waiters();
            while self.block_reconnect.load(Ordering::Acquire) {
                self.reconnect_permit.notified().await;
            }
        }
        let mut caps = vec![
            "session-journal",
            "actionable-sync",
            "session-control-sync",
            "session-inventory-sync",
        ];
        if self.inner.lock().unwrap().use_workspace_stream {
            caps.push("workspace-sync");
        }
        Ok(Arc::new(TestSessionEndpoint {
            conn,
            host: serde_json::from_value(json!({
                "protocolVersion": 4,
                "product": {"name": "Kernel", "version": "0.9.23"},
                "capabilities": caps,
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
impl SessionEndpoint for TestSessionEndpoint {
    fn identity(&self) -> &str {
        "blocker-test"
    }
    fn host(&self) -> &HostDescription {
        &self.host
    }
    fn supports_recovery(&self) -> bool {
        self.source.supports_recovery.load(Ordering::Acquire)
    }
    fn capabilities(&self) -> EndpointCapabilities {
        EndpointCapabilities::methods(&[
            "session.prompt",
            "session.cancel",
            "session.updateQueue",
            "session.rename",
            "session.archive",
            "session.unarchive",
            "sessions.recover",
            "session.models",
            "session.selectModel",
            "approvals.policy",
            "approvals.mode",
            "approvals.select",
        ])
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        match target {
            StreamTarget::SessionInventory => {
                if self.conn == 2 && self.source.hold_inventory_conn2.load(Ordering::Acquire) {
                    self.source.inventory_follow_entered.notify_waiters();
                    self.source.inventory_follow_release.notified().await;
                }
                let (tx, rx) = mpsc::unbounded_channel();
                let mut guard = self.source.inner.lock().unwrap();
                let items = guard.custom_items.clone().unwrap_or_else(|| {
                    vec![
                        json!({"sessionId": "a", "running": guard.running["a"], "blank": false, "updatedAt": 10}),
                        json!({"sessionId": "b", "running": guard.running["b"], "blank": false, "updatedAt": 20}),
                        json!({"sessionId": "c", "running": guard.running["c"], "blank": false, "updatedAt": 30}),
                    ]
                });
                guard.inventory_tx = Some(tx);
                let baseline =
                    json!({"type": "baseline", "generation": "g", "snapshot": {"items": items}});
                let tail = stream::unfold(rx, |mut rx| async move {
                    rx.recv().await.map(|item| (item, rx))
                });
                Ok(Box::pin(stream::once(async { Ok(baseline) }).chain(tail)))
            }
            StreamTarget::WorkspaceInventory => {
                let guard = self.source.inner.lock().unwrap();
                let baseline = json!({
                    "type": "baseline",
                    "generation": "g",
                    "snapshot": {
                        "items": [],
                        "archivedSessionIds": guard.archived_ids.clone(),
                        "pinnedSessionIds": []
                    }
                });
                Ok(Box::pin(
                    stream::once(async { Ok(baseline) }).chain(stream::pending()),
                ))
            }
            StreamTarget::SessionControl => {
                let projections = self
                    .source
                    .inner
                    .lock()
                    .unwrap()
                    .control_projections
                    .clone();
                let baseline = json!({"type": "baseline", "generation": "g", "snapshot": {"queues": [], "jobs": [], "projections": projections}});
                Ok(Box::pin(
                    stream::once(async { Ok(baseline) }).chain(stream::pending()),
                ))
            }
            StreamTarget::Actionables => {
                let reset = json!({"type": "reset", "generation": "g"});
                Ok(Box::pin(
                    stream::once(async { Ok(reset) }).chain(stream::pending()),
                ))
            }
            StreamTarget::SessionJournal { address, .. } => {
                if address.session_id == "b" && self.source.hold_journal_b.load(Ordering::Acquire) {
                    self.source.journal_b_entered.notify_waiters();
                    self.source.journal_b_release.notified().await;
                }
                struct JournalDrop {
                    conn: usize,
                    sid: String,
                    inner: Arc<Mutex<EndpointState>>,
                }
                impl Drop for JournalDrop {
                    fn drop(&mut self) {
                        self.inner
                            .lock()
                            .unwrap()
                            .journal_closes
                            .push((self.conn, self.sid.clone()));
                    }
                }
                let sid = address.session_id.clone();
                let (tx, rx) = mpsc::unbounded_channel();
                let entries = {
                    let mut guard = self.source.inner.lock().unwrap();
                    guard.journal_opens.push((self.conn, sid.clone()));
                    guard.journal_txs.insert(sid.clone(), tx.clone());
                    guard
                        .journal_txs_by_conn
                        .insert((self.conn, sid.clone()), tx);
                    guard.journal_entries.get(&sid).cloned().unwrap_or_default()
                };
                let through = entries
                    .last()
                    .and_then(|e| e["event"]["seq"].as_i64())
                    .unwrap_or(0);
                let snap = json!({
                    "type": "snapshot",
                    "snapshot": {
                        "sessionId": sid,
                        "generation": "g",
                        "throughSequence": through,
                        "windowLimit": 50,
                        "entries": entries,
                        "hasMoreBefore": false
                    }
                });
                let guard = JournalDrop {
                    conn: self.conn,
                    sid: address.session_id.clone(),
                    inner: Arc::clone(&self.source.inner),
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
        Ok(())
    }
    async fn call(&self, method: &str, payload: Value, _rpc_id: &str) -> Result<Value> {
        self.source
            .inner
            .lock()
            .unwrap()
            .rpcs
            .push((method.to_owned(), payload.clone()));
        match method {
            "session.models" => {
                let sid = payload["sessionId"].as_str().unwrap_or("");
                if sid == "a" && self.source.inner.lock().unwrap().model_delay_a {
                    self.source.model_gate_a.notified().await;
                }
                let selected = if sid == "a" { "model-a" } else { "model-b" };
                Ok(json!({
                    "groups": [{
                        "id": "mock",
                        "models": [
                            {"id": "model-a", "name": "model-a", "reasoning": {"efforts": ["low", "medium", "high"], "defaultEffort": "medium"}},
                            {"id": "model-b", "name": "model-b", "reasoning": {"efforts": ["low", "medium", "high"], "defaultEffort": "high"}}
                        ]
                    }],
                    "current": {"provider": "mock", "model": selected, "reasoningEffort": if sid == "a" { "medium" } else { "high" }}
                }))
            }
            "approvals.policy" => Ok(json!({
                "threadLevel": [
                    {"value": "workspace-write", "title": "Workspace Write"},
                    {"value": "read-only", "title": "Read Only"},
                    {"value": "danger-full-access", "title": "Full Access"}
                ],
                "serverLevel": {"currentValue": "workspace-write", "options": []}
            })),
            "approvals.mode" => {
                let sid = payload["sessionId"].as_str().unwrap_or("");
                if sid == "a" && self.source.inner.lock().unwrap().approval_delay_a {
                    self.source.approval_gate_a.notified().await;
                }
                let mode = if sid == "a" {
                    "danger-full-access"
                } else {
                    "read-only"
                };
                Ok(json!({ "mode": mode }))
            }
            "session.cancel" => {
                if self.source.park_reject_cancel.swap(false, Ordering::AcqRel) {
                    let release = self.source.park_reject_release.notified();
                    tokio::pin!(release);
                    release.as_mut().enable();
                    self.source.park_reject_entered.notify_one();
                    release.await;
                    return Err(Error::Remote("cancel rejected".into()));
                }
                if self.source.hold_cancel.swap(false, Ordering::AcqRel) {
                    let release = self.source.cancel_release.notified();
                    tokio::pin!(release);
                    release.as_mut().enable();
                    self.source.cancel_gate_entered.notify_one();
                    release.await;
                }
                if self
                    .source
                    .cancel_disconnect_before_reply
                    .swap(false, Ordering::AcqRel)
                {
                    self.source.cancel_entered.notify_one();
                    self.source.drop_connection();
                    return Err(Error::Disconnected);
                }
                if self.source.inner.lock().unwrap().transport_error_cancel {
                    return Err(Error::Transport("receipt socket closed".into()));
                }
                if self.source.inner.lock().unwrap().reject_cancel {
                    return Err(Error::Remote("cancel rejected".into()));
                }
                Ok(json!({ "cancelled": true }))
            }
            "session.updateQueue" => {
                if self.source.inner.lock().unwrap().reject_queue_update {
                    return Err(Error::Remote("queue-item-not-found".into()));
                }
                Ok(json!({ "ok": true }))
            }
            "session.rename" => {
                let sid = payload["sessionId"].as_str().unwrap_or("").to_owned();
                let title = payload["title"].as_str().unwrap_or("").to_owned();
                let gate = {
                    self.source
                        .inner
                        .lock()
                        .unwrap()
                        .rename_gates
                        .remove(&title)
                };
                if let Some((entered, release, forced_outcome)) = gate {
                    let wait = release.notified();
                    tokio::pin!(wait);
                    wait.as_mut().enable();
                    entered.notify_waiters();
                    wait.await;
                    if let Err(err) = forced_outcome {
                        return Err(Error::Remote(err.into()));
                    }
                }
                let mut guard = self.source.inner.lock().unwrap();
                if guard.reject_rename {
                    return Err(Error::Remote("rename rejected".into()));
                }
                if title.len() > 256 {
                    return Err(Error::Remote("title-invalid: too long".into()));
                }
                guard.rename_seq += 1;
                let seq = guard.rename_seq;
                if let Some(items) = guard.custom_items.as_mut()
                    && let Some(item) = items.iter_mut().find(|item| item["sessionId"] == sid)
                {
                    item["projections"] = json!({
                        "asOfSeq": seq,
                        "values": { "title": title }
                    });
                }
                if let Some(tx) = guard.inventory_tx.clone() {
                    let updated = guard
                        .custom_items
                        .as_ref()
                        .and_then(|items| {
                            items.iter().find(|item| item["sessionId"] == sid).cloned()
                        })
                        .unwrap_or_else(|| {
                            json!({
                                "sessionId": sid,
                                "running": guard.running.get(&sid).copied().unwrap_or(false),
                                "blank": false,
                                "updatedAt": 20,
                                "projections": {
                                    "asOfSeq": seq,
                                    "values": { "title": title }
                                }
                            })
                        });
                    let _ = tx.send(Ok(json!({
                        "type": "upsert",
                        "generation": "g",
                        "session": updated
                    })));
                }
                Ok(json!({ "seq": seq, "title": title }))
            }
            "session.archive" => {
                let sid = payload["sessionId"].as_str().unwrap_or("").to_owned();
                let deferred = self.source.defer_archive_mutation.load(Ordering::Acquire);
                let reject_archive = {
                    let mut guard = self.source.inner.lock().unwrap();
                    if !deferred && !guard.reject_archive {
                        if let Some(items) = guard.custom_items.as_mut() {
                            items.retain(|item| item["sessionId"] != sid);
                        }
                        if !guard.archived_ids.iter().any(|id| id == &sid) {
                            guard.archived_ids.push(sid);
                        }
                    }
                    guard.reject_archive
                };
                if self.source.park_archive.swap(false, Ordering::AcqRel) {
                    let release = self.source.park_archive_release.notified();
                    tokio::pin!(release);
                    release.as_mut().enable();
                    self.source.park_archive_entered.notify_one();
                    release.await;
                }
                if reject_archive {
                    return Err(Error::Remote("archive rejected".into()));
                }
                let archived_ids = self.source.inner.lock().unwrap().archived_ids.clone();
                Ok(json!({ "archivedSessionIds": archived_ids }))
            }
            "sessions.recover" => Ok(json!({ "recoveredSessionIds": payload["sessionIds"] })),
            "session.unarchive" => {
                let sid = payload["sessionId"].as_str().unwrap_or("").to_owned();
                let mut guard = self.source.inner.lock().unwrap();
                if guard.reject_unarchive || !guard.archived_ids.iter().any(|id| id == &sid) {
                    return Err(Error::Remote(format!("session-not-found: {sid}").into()));
                }
                guard.archived_ids.retain(|id| id != &sid);
                if let Some(items) = guard.custom_items.as_mut()
                    && !items.iter().any(|item| item["sessionId"] == sid)
                {
                    items.push(json!({"sessionId": sid, "running": false, "blank": false, "updatedAt": 25}));
                }
                Ok(json!({ "sessionId": sid }))
            }
            "session.selectModel" | "approvals.select" => Ok(json!({ "ok": true })),
            "session.prompt" => Ok(json!({ "accepted": true })),
            _ => Ok(json!({})),
        }
    }
    async fn close(&self) {
        self.source
            .inner
            .lock()
            .unwrap()
            .closed
            .push("closed".into());
    }
}

fn setup_binding(
    endpoint: &BlockerTestEndpoint,
) -> (
    tokio::runtime::Runtime,
    Arc<SessionRuntime>,
    Arc<RunWorkbenchBinding>,
) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let handle = rt.handle().clone();
    let runtime = rt.block_on(async {
        let runtime = Arc::new(SessionRuntime::spawn(
            "blocker-test",
            Arc::new(endpoint.clone()),
        ));
        runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
        runtime
    });
    let controller =
        rt.block_on(async { Arc::new(RunController::new(Arc::clone(&runtime)).unwrap()) });
    let wake: Arc<dyn UiWakeNotifier> = Arc::new(NoopWakeNotifier);
    let binding = RunWorkbenchBinding::new(
        "blocker-test",
        Arc::clone(&runtime),
        controller,
        handle,
        wake,
    );
    (rt, runtime, binding)
}

#[derive(Default)]
struct CountingWake {
    count: AtomicUsize,
}

impl CountingWake {
    fn count(&self) -> usize {
        self.count.load(Ordering::Acquire)
    }

    fn wait_at_least(&self, target: usize) {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while self.count() < target {
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for wake count {target}, got {}",
                self.count()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl UiWakeNotifier for CountingWake {
    fn wake(&self) {
        self.count.fetch_add(1, Ordering::AcqRel);
    }
}

fn wait_until(name: &str, pred: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while !pred() {
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {name}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn setup_binding_with_wake(
    endpoint: &BlockerTestEndpoint,
) -> (
    tokio::runtime::Runtime,
    Arc<SessionRuntime>,
    Arc<RunWorkbenchBinding>,
    Arc<CountingWake>,
) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let handle = rt.handle().clone();
    let runtime = rt.block_on(async {
        let runtime = Arc::new(SessionRuntime::spawn(
            "blocker-test",
            Arc::new(endpoint.clone()),
        ));
        runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
        runtime
    });
    let controller =
        rt.block_on(async { Arc::new(RunController::new(Arc::clone(&runtime)).unwrap()) });
    let wake = Arc::new(CountingWake::default());
    let binding = RunWorkbenchBinding::new(
        "blocker-test",
        Arc::clone(&runtime),
        controller,
        handle,
        wake.clone(),
    );
    (rt, runtime, binding, wake)
}

async fn wait_store(runtime: &SessionRuntime, pred: impl Fn(&MemoryProjectionStore) -> bool) {
    let mut sub = runtime.subscribe();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if runtime.inspect(&pred) {
                return;
            }
            sub.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}

#[test]
fn blocker1_stale_epoch_and_failed_cancel_record_session_scoped_error() {
    let endpoint = BlockerTestEndpoint::new();
    let (rt, runtime, binding) = setup_binding(&endpoint);
    binding.open_initial_session("a").unwrap();

    // Start running turn on A so can_cancel becomes true.
    let tx_a = endpoint.inner.lock().unwrap().journal_txs["a"].clone();
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {
            "event": {"type": "turn/start", "seq": 1, "time": 2, "data": {"turn": 1}}
        }
    })))
    .unwrap();
    rt.block_on(wait_store(&runtime, |s| {
        s.sessions.get("a").is_some_and(|sess| sess.run_open)
    }));

    let scope_a = RunScope::main("blocker-test", "a");
    let current_epoch = runtime.inspect(|s| s.connection_epoch);
    // Cancel at stale epoch must NOT emit session.cancel and must record error on session A.
    binding.cancel_scope_at_epoch(current_epoch + 99, &scope_a);
    for _ in 0..40 {
        if binding.session_error("a").is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(binding.session_error("a").is_some());
    assert_eq!(binding.session_error("b"), None);
    assert!(
        !endpoint.rpcs().iter().any(|(m, _)| m == "session.cancel"),
        "stale epoch cancel must never call session.cancel"
    );

    // Endpoint failure on live cancel at current epoch must also surface on session A only.
    binding.clear_session_error("a");
    endpoint.inner.lock().unwrap().reject_cancel = true;
    binding.cancel_scope_at_epoch(current_epoch, &scope_a);
    for _ in 0..40 {
        if binding.session_error("a").is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let err_a = binding.session_error("a").unwrap();
    assert!(
        err_a.contains("cancel rejected"),
        "unexpected err_a: {err_a}"
    );
    assert_eq!(binding.session_error("b"), None);
    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn blocker2_background_draft_and_delayed_model_approval_isolation() {
    let endpoint = BlockerTestEndpoint::new();
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.model_delay_a = true;
        guard.approval_delay_a = true;
    }
    let (rt, runtime, binding) = setup_binding(&endpoint);
    binding.open_initial_session("a").unwrap();

    let dest_a = ComposerDestination::run(RunScope::main("blocker-test", "a"));
    let dest_b = ComposerDestination::run(RunScope::main("blocker-test", "b"));
    binding.set_draft_text(&dest_a, "draft-alpha".into());
    binding.set_draft_text(&dest_b, "draft-beta".into());

    // Part A: Select B, attach + quote onto background A -> neither side's text is overwritten.
    binding.select_session("b");
    binding.attach_file_path(Some(dest_a.clone()), "/tmp/bg.rs".into());
    binding.apply_quote(ComposerQuoteRequest {
        destination: Some(dest_a.clone()),
        reference: ArtifactSelectionReference {
            document_id: "doc-1".into(),
            artifact_id: "art-1".into(),
            artifact_type: "file".into(),
            field_path: vec!["src".into()],
            revision: "r1".into(),
            digest: "d1".into(),
            range: Utf16SelectionRange {
                location: 0,
                length: 4,
            },
            quote: "code".into(),
            context_before: String::new(),
            context_after: String::new(),
        },
        comment: "note".into(),
    });
    assert_eq!(binding.controller().snapshot().draft.text, "draft-beta");
    binding.select_session("a");
    let snap_a = binding.run_snapshot();
    assert_eq!(snap_a.draft.text, "draft-alpha");
    assert_eq!(snap_a.draft.artifacts.len(), 1);
    assert_eq!(snap_a.draft.annotations.len(), 1);

    // Part B: Trigger A's delayed controls while selected, then immediately switch to B.
    binding.refresh_controls();
    std::thread::sleep(Duration::from_millis(20));
    binding.select_session("b");

    // Wait for B's immediate controls (model-b + Read Only) to load.
    for _ in 0..80 {
        if binding.selected_model() == "model-b" && binding.selected_preset() == "Read Only" {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(binding.selected_model(), "model-b");
    assert_eq!(binding.selected_preset(), "Read Only");

    // Release A's delayed model-a and danger-full-access responses while B remains selected.
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.model_delay_a = false;
        guard.approval_delay_a = false;
    }
    endpoint.model_gate_a.notify_waiters();
    endpoint.approval_gate_a.notify_waiters();
    std::thread::sleep(Duration::from_millis(40));

    // B's active UI controls and submission settings MUST stay model-b and read-only.
    assert_eq!(binding.selected_model(), "model-b");
    assert_eq!(binding.selected_preset(), "Read Only");
    binding.submit_destination(&dest_b);
    for _ in 0..80 {
        if endpoint
            .rpcs()
            .iter()
            .any(|(m, p)| m == "session.prompt" && p["sessionId"] == "b")
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let rpcs = endpoint.rpcs();
    assert!(rpcs.iter().any(|(m, p)| {
        m == "session.selectModel" && p["sessionId"] == "b" && p["model"] == "model-b"
    }));
    assert!(rpcs.iter().any(|(m, p)| {
        m == "approvals.select" && p["sessionId"] == "b" && p["mode"] == "read-only"
    }));
    assert!(!rpcs.iter().any(|(m, p)| {
        (m == "session.selectModel" || m == "approvals.select")
            && p["sessionId"] == "b"
            && (p["model"] == "model-a" || p["mode"] == "danger-full-access")
    }));

    // Part C: Start A running while selected, switch to B (deferred close keeps A streaming until settled),
    // verify A does not bleed into B, and switching back to A shows 1 complete turn.
    binding.select_session("a");
    for _ in 0..40 {
        if runtime.inspect(|s| s.sessions.get("a").is_some_and(|sess| sess.journal_ready)) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let tx_a = endpoint.inner.lock().unwrap().journal_txs["a"].clone();
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {
            "event": {"type": "turn/start", "seq": 1, "time": 2, "data": {"turn": 1}}
        }
    })))
    .unwrap();
    rt.block_on(wait_store(&runtime, |s| {
        s.sessions.get("a").is_some_and(|sess| sess.run_open)
    }));

    binding.select_session("b");
    let assistant_ev = json!({"event": {"type": "assistant/message", "seq": 2, "time": 3, "data": {"turn": 1, "step": 1, "sessionFinal": true, "message": {"id": "ma-1", "content": [{"type": "text", "text": "alpha-only-reply"}]}}}});
    let end_ev = json!({"event": {"type": "turn/end", "seq": 3, "time": 4, "data": {"turn": 1, "reason": "completed"}}});
    {
        let mut guard = endpoint.inner.lock().unwrap();
        let entries = guard.journal_entries.get_mut("a").unwrap();
        entries.push(
            json!({"event": {"type": "turn/start", "seq": 1, "time": 2, "data": {"turn": 1}}}),
        );
        entries.push(assistant_ev.clone());
        entries.push(end_ev.clone());
    }
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": assistant_ev
    })))
    .unwrap();
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": end_ev
    })))
    .unwrap();
    rt.block_on(wait_store(&runtime, |s| {
        s.sessions
            .get("a")
            .is_some_and(|sess| !sess.run_open && !sess.journal_ready)
    }));
    assert!(
        !binding
            .run_snapshot()
            .transcript
            .items
            .iter()
            .any(|item| item.text.contains("alpha-only-reply")),
        "background A text must never bleed into selected B"
    );

    binding.select_session("a");
    for _ in 0..60 {
        if binding
            .run_snapshot()
            .transcript
            .items
            .iter()
            .any(|item| item.text.contains("alpha-only-reply"))
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        binding
            .run_snapshot()
            .transcript
            .items
            .iter()
            .filter(|item| item.text.contains("alpha-only-reply"))
            .count(),
        1
    );
    assert_eq!(binding.selected_model(), "model-a");
    assert_eq!(binding.selected_preset(), "Full Access");
    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn blocker3_dropping_binding_releases_visited_session_journals_on_shared_runtime() {
    let endpoint = BlockerTestEndpoint::new();
    let (rt, runtime, binding) = setup_binding(&endpoint);

    binding.open_initial_session("a").unwrap();
    assert!(runtime.inspect(|s| s.sessions["a"].journal_ready));

    binding.select_session("b");
    rt.block_on(wait_store(&runtime, |s| {
        s.sessions.get("b").is_some_and(|sess| sess.journal_ready)
            && !s.sessions.get("a").is_some_and(|sess| sess.journal_ready)
    }));

    // Drop binding outside Tokio task while shared SessionRuntime remains alive -> B journal must close.
    drop(binding);
    rt.block_on(wait_store(&runtime, |s| {
        !s.sessions.get("b").is_some_and(|sess| sess.journal_ready)
            && !s.sessions.get("a").is_some_and(|sess| sess.journal_ready)
    }));

    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn disconnected_switch_a_to_b_auto_loads_b_history_and_deltas_on_reconnect() {
    let endpoint = BlockerTestEndpoint::new();
    let (rt, runtime, binding) = setup_binding(&endpoint);
    binding.open_initial_session("a").unwrap();
    assert!(runtime.inspect(|s| s.sessions["a"].journal_ready));

    // Drop connection and wait for reconnect barrier.
    endpoint.drop_connection();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection != tekes_agent_session::ConnectionState::Ready
    }));
    rt.block_on(endpoint.reconnecting.notified());

    // Switch A -> B while disconnected; allow the spawned lease release to reach the disconnected
    // runtime before unblocking reconnect, and never call select_session("b") again.
    binding.select_session("b");
    assert_eq!(
        binding.current_scope().map(|s| s.session_id),
        Some("b".to_owned())
    );
    assert!(!runtime.inspect(|s| s.sessions.get("b").is_some_and(|sess| sess.journal_ready)));
    std::thread::sleep(Duration::from_millis(25));

    let epoch_before = runtime.inspect(|s| s.connection_epoch);
    let rpcs_before = endpoint.rpcs().len();
    endpoint.unblock_reconnect();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection == tekes_agent_session::ConnectionState::Ready
            && s.connection_epoch > epoch_before
            && s.sessions.get("b").is_some_and(|sess| sess.journal_ready)
    }));

    // Wait for B's snapshot to populate in binding WITHOUT another click/select_session.
    for _ in 0..50 {
        if binding
            .run_snapshot()
            .transcript
            .items
            .iter()
            .any(|item| item.text.contains("init-b"))
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        binding
            .run_snapshot()
            .transcript
            .items
            .iter()
            .any(|item| item.text.contains("init-b")),
        "B real history must load automatically after reconnect"
    );

    // Stream live delta onto B and verify it arrives cleanly on conn 2.
    let tx_b = endpoint.inner.lock().unwrap().journal_txs["b"].clone();
    tx_b.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "b",
        "entry": {
            "event": {
                "type": "assistant/message",
                "seq": 1,
                "time": 2,
                "data": {
                    "turn": 1,
                    "step": 1,
                    "sessionFinal": true,
                    "message": {"id": "mb-1", "content": [{"type": "text", "text": "live-after-reconnect-b"}]}
                }
            }
        }
    })))
    .unwrap();
    for _ in 0..50 {
        if binding
            .run_snapshot()
            .transcript
            .items
            .iter()
            .any(|item| item.text.contains("live-after-reconnect-b"))
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        binding
            .run_snapshot()
            .transcript
            .items
            .iter()
            .any(|item| item.text.contains("live-after-reconnect-b"))
    );

    // Idle A must NOT be reopened on conn 2; no prompt or cancel RPC re-sent.
    let opens = endpoint.journal_opens();
    assert!(opens.contains(&(2, "b".to_owned())));
    assert!(!opens.contains(&(2, "a".to_owned())));
    let closes = endpoint.journal_closes();
    assert!(closes.contains(&(1, "a".to_owned())));
    assert!(
        !endpoint.rpcs()[rpcs_before..]
            .iter()
            .any(|(m, _)| m == "session.prompt" || m == "session.cancel")
    );

    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn disconnected_switch_a_to_b_to_c_retains_only_c_subscription_on_reconnect() {
    let endpoint = BlockerTestEndpoint::new();
    let (rt, runtime, binding) = setup_binding(&endpoint);
    binding.open_initial_session("a").unwrap();

    endpoint.drop_connection();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection != tekes_agent_session::ConnectionState::Ready
    }));
    rt.block_on(endpoint.reconnecting.notified());

    binding.select_session("b");
    binding.select_session("c");
    std::thread::sleep(Duration::from_millis(25));

    let epoch_before = runtime.inspect(|s| s.connection_epoch);
    endpoint.unblock_reconnect();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection == tekes_agent_session::ConnectionState::Ready
            && s.connection_epoch > epoch_before
            && s.sessions.get("c").is_some_and(|sess| sess.journal_ready)
    }));

    std::thread::sleep(Duration::from_millis(40));
    let conn2_opens: Vec<String> = endpoint
        .journal_opens()
        .into_iter()
        .filter(|(conn, _)| *conn == 2)
        .map(|(_, sid)| sid)
        .collect();
    assert_eq!(conn2_opens, vec!["c".to_owned()]);
    assert!(!runtime.inspect(|s| s.sessions["a"].journal_ready));
    assert!(!runtime.inspect(|s| s.sessions["b"].journal_ready));
    assert!(runtime.inspect(|s| s.sessions["c"].journal_ready));
    for _ in 0..50 {
        if binding
            .run_snapshot()
            .transcript
            .items
            .iter()
            .any(|item| item.text.contains("init-c"))
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        binding
            .run_snapshot()
            .transcript
            .items
            .iter()
            .any(|item| item.text.contains("init-c"))
    );

    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn dropping_binding_while_disconnected_does_not_open_history_and_preserves_other_observer() {
    let endpoint = BlockerTestEndpoint::new();
    let (rt, runtime, binding) = setup_binding(&endpoint);
    binding.open_initial_session("a").unwrap();
    let other_lease_b = rt.block_on(runtime.acquire_history("b")).unwrap();
    assert!(runtime.inspect(|s| s.sessions["b"].journal_ready));

    endpoint.drop_connection();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection != tekes_agent_session::ConnectionState::Ready
    }));
    rt.block_on(endpoint.reconnecting.notified());

    binding.select_session("c");
    drop(binding);
    std::thread::sleep(Duration::from_millis(25));

    let epoch_before = runtime.inspect(|s| s.connection_epoch);
    endpoint.unblock_reconnect();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection == tekes_agent_session::ConnectionState::Ready
            && s.connection_epoch > epoch_before
            && s.sessions.get("b").is_some_and(|sess| sess.journal_ready)
    }));
    std::thread::sleep(Duration::from_millis(40));

    let conn2_opens: Vec<String> = endpoint
        .journal_opens()
        .into_iter()
        .filter(|(conn, _)| *conn == 2)
        .map(|(_, sid)| sid)
        .collect();
    assert_eq!(conn2_opens, vec!["b".to_owned()]);
    assert!(!runtime.inspect(|s| s.sessions["c"].journal_ready));
    assert!(!runtime.inspect(|s| s.sessions["a"].journal_ready));
    assert!(runtime.inspect(|s| s.sessions["b"].journal_ready));

    rt.block_on(other_lease_b.release()).unwrap();
    rt.block_on(wait_store(&runtime, |s| {
        !s.sessions.get("b").is_some_and(|sess| sess.journal_ready)
    }));
    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn open_initial_session_disconnected_fresh_binding_preserves_empty_selection_and_opens_nothing() {
    let endpoint = BlockerTestEndpoint::new();
    let (rt, runtime, binding) = setup_binding(&endpoint);

    endpoint.drop_connection();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection != tekes_agent_session::ConnectionState::Ready
    }));
    rt.block_on(endpoint.reconnecting.notified());

    let err = binding
        .open_initial_session("b")
        .expect_err("initial open during disconnect must fail");
    assert!(matches!(err, Error::Disconnected));
    assert!(binding.current_scope().is_none());

    let epoch_before = runtime.inspect(|s| s.connection_epoch);
    endpoint.unblock_reconnect();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection == tekes_agent_session::ConnectionState::Ready
            && s.connection_epoch > epoch_before
    }));
    std::thread::sleep(Duration::from_millis(50));

    let conn2_opens: Vec<_> = endpoint
        .journal_opens()
        .into_iter()
        .filter(|(conn, _)| *conn == 2)
        .collect();
    assert!(
        conn2_opens.is_empty(),
        "unexpected conn2 opens: {conn2_opens:?}"
    );
    assert!(!runtime.inspect(|s| s.sessions["b"].journal_ready));
    assert!(binding.current_scope().is_none());
    assert!(
        !binding
            .run_snapshot()
            .transcript
            .items
            .iter()
            .any(|item| item.text.contains("init-b"))
    );

    // Once reconnected, opening B succeeds with selection and lease consistent.
    binding.open_initial_session("b").unwrap();
    assert_eq!(
        binding.current_scope().map(|s| s.session_id),
        Some("b".to_owned())
    );
    assert!(runtime.inspect(|s| s.sessions["b"].journal_ready));
    assert!(
        binding
            .run_snapshot()
            .transcript
            .items
            .iter()
            .any(|item| item.text.contains("init-b"))
    );
    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn open_initial_session_disconnected_with_previous_selection_preserves_a_and_never_opens_b() {
    let endpoint = BlockerTestEndpoint::new();
    let (rt, runtime, binding) = setup_binding(&endpoint);
    binding.open_initial_session("a").unwrap();
    assert!(runtime.inspect(|s| s.sessions["a"].journal_ready));

    endpoint.drop_connection();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection != tekes_agent_session::ConnectionState::Ready
    }));
    rt.block_on(endpoint.reconnecting.notified());

    let err = binding
        .open_initial_session("b")
        .expect_err("initial open during disconnect must fail");
    assert!(matches!(err, Error::Disconnected));
    assert_eq!(
        binding.current_scope().map(|s| s.session_id),
        Some("a".to_owned())
    );

    let epoch_before = runtime.inspect(|s| s.connection_epoch);
    endpoint.unblock_reconnect();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection == tekes_agent_session::ConnectionState::Ready
            && s.connection_epoch > epoch_before
            && s.sessions.get("a").is_some_and(|sess| sess.journal_ready)
    }));
    std::thread::sleep(Duration::from_millis(50));

    let conn2_opens: Vec<String> = endpoint
        .journal_opens()
        .into_iter()
        .filter(|(conn, _)| *conn == 2)
        .map(|(_, sid)| sid)
        .collect();
    assert_eq!(conn2_opens, vec!["a".to_owned()]);
    assert!(runtime.inspect(|s| s.sessions["a"].journal_ready));
    assert!(!runtime.inspect(|s| s.sessions["b"].journal_ready));
    assert_eq!(
        binding.current_scope().map(|s| s.session_id),
        Some("a".to_owned())
    );
    assert!(
        binding
            .run_snapshot()
            .transcript
            .items
            .iter()
            .any(|item| item.text.contains("init-a"))
    );
    assert!(
        !binding
            .run_snapshot()
            .transcript
            .items
            .iter()
            .any(|item| item.text.contains("init-b"))
    );
    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn switching_from_running_a_to_b_during_reconnect_handshake_keeps_a_until_turn_end_and_shows_only_b()
 {
    let endpoint = BlockerTestEndpoint::new();
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.running.insert("a".to_owned(), true);
        guard.journal_entries.insert(
            "a".to_owned(),
            vec![
                json!({"event": {"type": "user/message", "seq": 0, "time": 1, "data": {"text": "init-a"}}}),
                json!({"event": {"type": "turn/start", "seq": 1, "time": 2, "data": {"turn": 1}}}),
            ],
        );
    }
    let (rt, runtime, binding) = setup_binding(&endpoint);
    binding.open_initial_session("a").unwrap();
    assert!(runtime.inspect(|s| s.sessions["a"].journal_ready && s.sessions["a"].run_open));

    endpoint.hold_inventory_conn2.store(true, Ordering::Release);
    endpoint.drop_connection();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection != tekes_agent_session::ConnectionState::Ready
    }));
    rt.block_on(endpoint.reconnecting.notified());
    endpoint.unblock_reconnect();
    rt.block_on(endpoint.inventory_follow_entered.notified());

    binding.select_session("b");
    assert_eq!(
        binding.current_scope().map(|s| s.session_id),
        Some("b".to_owned())
    );
    std::thread::sleep(Duration::from_millis(25));

    endpoint.inventory_follow_release.notify_one();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection == tekes_agent_session::ConnectionState::Ready
            && s.sessions.get("b").is_some_and(|sess| sess.journal_ready)
    }));

    for _ in 0..50 {
        if binding
            .run_snapshot()
            .transcript
            .items
            .iter()
            .any(|item| item.text.contains("init-b"))
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let snap = binding.run_snapshot();
    assert!(
        snap.transcript
            .items
            .iter()
            .any(|item| item.text.contains("init-b"))
    );
    assert!(
        !snap
            .transcript
            .items
            .iter()
            .any(|item| item.text.contains("init-a"))
    );

    assert!(endpoint.journal_opens().contains(&(2, "a".to_owned())));
    std::thread::sleep(Duration::from_millis(50));
    assert!(
        !endpoint.journal_closes().contains(&(2, "a".to_owned())),
        "A journal closed prematurely on conn 2: {:?}",
        endpoint.journal_closes()
    );
    assert!(runtime.inspect(|s| s.sessions["a"].journal_ready && s.sessions["a"].run_open));

    let tx_a = endpoint.inner.lock().unwrap().journal_txs_by_conn[&(2, "a".to_owned())].clone();
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
    rt.block_on(wait_store(&runtime, |s| {
        !s.sessions["a"].journal_ready && !s.sessions["a"].run_open
    }));
    for _ in 0..50 {
        if endpoint.journal_closes().contains(&(2, "a".to_owned())) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(endpoint.journal_closes().contains(&(2, "a".to_owned())));
    assert_eq!(
        binding.current_scope().map(|s| s.session_id),
        Some("b".to_owned())
    );
    assert!(runtime.inspect(|s| s.sessions["b"].journal_ready));
    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn overlapping_open_initial_session_b_and_select_session_c_keeps_selection_history_and_lease_on_c_across_reconnect()
 {
    let endpoint = BlockerTestEndpoint::new();
    let (rt, runtime, binding) = setup_binding(&endpoint);
    binding.open_initial_session("a").unwrap();
    assert!(runtime.inspect(|s| s.sessions["a"].journal_ready));

    endpoint.hold_journal_b.store(true, Ordering::Release);
    std::thread::scope(|scope| {
        let binding_b = &binding;
        let opener = scope.spawn(move || binding_b.open_initial_session("b"));
        rt.block_on(endpoint.journal_b_entered.notified());

        let rx = binding.controller().subscribe();
        let watch_guard = rx.borrow();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let binding_c = &binding;
        let selector = scope.spawn(move || {
            started_tx.send(()).unwrap();
            binding_c.select_session("c");
        });
        started_rx.recv().unwrap();
        std::thread::sleep(Duration::from_millis(30));

        endpoint.journal_b_release.notify_one();
        std::thread::sleep(Duration::from_millis(30));
        assert!(!opener.is_finished() && !selector.is_finished());
        assert!(
            !runtime.inspect(|s| s.sessions.get("c").is_some_and(|sess| sess.journal_ready)),
            "C must not acquire history before selection publication completes"
        );
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let binding_getter = &binding;
        scope.spawn(move || {
            let _ = binding_getter.session_error("a");
            let _ = binding_getter.error_for_session(Some("a"));
            let _ = binding_getter.last_error();
            let _ = binding_getter.endpoint_id();
            let _ = binding_getter.sidebar_rows();
            let _ = done_tx.send(());
        });
        done_rx
            .recv_timeout(Duration::from_millis(300))
            .expect("public getters must not deadlock while watch guard held");
        drop(watch_guard);

        let b_res = opener.join().unwrap();
        selector.join().unwrap();
        assert!(matches!(b_res, Err(Error::Disconnected)));
        assert!(
            endpoint.journal_closes().contains(&(1, "b".to_owned())),
            "stale B journal must already be closed once opener returns Err(Disconnected)"
        );
    });

    rt.block_on(wait_store(&runtime, |s| {
        s.sessions.get("c").is_some_and(|sess| sess.journal_ready)
            && !s.sessions.get("b").is_some_and(|sess| sess.journal_ready)
            && !s.sessions.get("a").is_some_and(|sess| sess.journal_ready)
    }));
    for _ in 0..50 {
        if binding
            .run_snapshot()
            .transcript
            .items
            .iter()
            .any(|item| item.text.contains("init-c"))
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        binding.current_scope().map(|s| s.session_id),
        Some("c".to_owned())
    );
    let snap = binding.run_snapshot();
    assert!(
        snap.transcript
            .items
            .iter()
            .any(|item| item.text.contains("init-c"))
    );
    assert!(
        !snap
            .transcript
            .items
            .iter()
            .any(|item| item.text.contains("init-b"))
    );

    // Reconnect and confirm C remains selected, open, and visible.
    let epoch_before = runtime.inspect(|s| s.connection_epoch);
    endpoint.drop_connection();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection != tekes_agent_session::ConnectionState::Ready
    }));
    rt.block_on(endpoint.reconnecting.notified());
    endpoint.unblock_reconnect();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection == tekes_agent_session::ConnectionState::Ready
            && s.connection_epoch > epoch_before
            && s.sessions.get("c").is_some_and(|sess| sess.journal_ready)
    }));
    std::thread::sleep(Duration::from_millis(40));
    let conn2_opens: Vec<String> = endpoint
        .journal_opens()
        .into_iter()
        .filter(|(conn, _)| *conn == 2)
        .map(|(_, sid)| sid)
        .collect();
    assert_eq!(conn2_opens, vec!["c".to_owned()]);
    assert_eq!(
        binding.current_scope().map(|s| s.session_id),
        Some("c".to_owned())
    );
    assert!(
        binding
            .run_snapshot()
            .transcript
            .items
            .iter()
            .any(|item| item.text.contains("init-c"))
    );
    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn interrupted_stop_receipt_reconciles_on_same_turn_aborted_and_preserves_rejections_other_turns_and_other_sessions()
 {
    // 1. Interrupted cancel receipt on Turn 1 clears when reconnect replays Turn 1 aborted,
    // without resending cancel or clearing B's error.
    let endpoint = BlockerTestEndpoint::new();
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.running.insert("a".to_owned(), true);
        guard.journal_entries.insert(
            "a".to_owned(),
            vec![
                json!({"event": {"type": "user/message", "seq": 0, "time": 1, "data": {"text": "init-a"}}}),
                json!({"event": {"type": "turn/start", "seq": 1, "time": 2, "data": {"turn": 1}}}),
            ],
        );
    }
    let (rt, runtime, binding) = setup_binding(&endpoint);
    binding.open_initial_session("a").unwrap();

    // Seed an independent error on session B via real rejection.
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.running.insert("b".to_owned(), true);
        guard.journal_entries.insert(
            "b".to_owned(),
            vec![
                json!({"event": {"type": "user/message", "seq": 0, "time": 1, "data": {"text": "init-b"}}}),
                json!({"event": {"type": "turn/start", "seq": 1, "time": 2, "data": {"turn": 1}}}),
            ],
        );
        guard.reject_cancel = true;
    }
    binding.select_session("b");
    rt.block_on(wait_store(&runtime, |s| s.sessions["b"].journal_ready));
    binding.cancel_scope(&RunScope::main("blocker-test", "b"));
    for _ in 0..50 {
        if binding.error_for_session(Some("b")).is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(binding.error_for_session(Some("b")).is_some());

    // Switch back to A and trigger Stop whose receipt is cut by disconnect.
    binding.select_session("a");
    rt.block_on(wait_store(&runtime, |s| s.sessions["a"].journal_ready));
    endpoint.inner.lock().unwrap().reject_cancel = false;
    endpoint
        .cancel_disconnect_before_reply
        .store(true, Ordering::Release);
    // Prepare authoritative Turn 1 aborted snapshot for conn 2 reconnect.
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.running.insert("a".to_owned(), false);
        guard.journal_entries.insert(
            "a".to_owned(),
            vec![
                json!({"event": {"type": "user/message", "seq": 0, "time": 1, "data": {"text": "init-a"}}}),
                json!({"event": {"type": "turn/start", "seq": 1, "time": 2, "data": {"turn": 1}}}),
                json!({"event": {"type": "turn/end", "seq": 2, "time": 3, "data": {"turn": 1, "reason": "aborted"}}}),
            ],
        );
    }

    let epoch1 = runtime.inspect(|s| s.connection_epoch);
    binding.cancel_scope_at_epoch(epoch1, &RunScope::main("blocker-test", "a"));
    for _ in 0..100 {
        if binding.error_for_session(Some("a")).is_some()
            && runtime.inspect(|s| s.connection != tekes_agent_session::ConnectionState::Ready)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        binding
            .error_for_session(Some("a"))
            .as_deref()
            .is_some_and(|e| e.contains("Cancel failed"))
    );

    endpoint.unblock_reconnect();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection == tekes_agent_session::ConnectionState::Ready
            && s.connection_epoch > epoch1
            && s.sessions["a"].journal_ready
    }));
    for _ in 0..50 {
        if binding.error_for_session(Some("a")).is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(binding.error_for_session(Some("a")), None);
    assert!(
        binding.error_for_session(Some("b")).is_some(),
        "reconciling A must not clear B's independent error"
    );
    let a_cancel_rpcs = endpoint
        .rpcs()
        .into_iter()
        .filter(|(m, p)| m == "session.cancel" && p["sessionId"] == "a")
        .count();
    assert_eq!(
        a_cancel_rpcs, 1,
        "must not resend session.cancel on reconnect"
    );

    // 2. If Turn 1 completes normally (not aborted), error stays visible even if Turn 2 later aborts.
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.running.insert("a".to_owned(), true);
        guard.journal_entries.insert(
            "a".to_owned(),
            vec![
                json!({"event": {"type": "user/message", "seq": 10, "time": 10, "data": {"text": "t1"}}}),
                json!({"event": {"type": "turn/start", "seq": 11, "time": 11, "data": {"turn": 1}}}),
            ],
        );
    }
    let epoch2 = runtime.inspect(|s| s.connection_epoch);
    endpoint.drop_connection();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection != tekes_agent_session::ConnectionState::Ready
    }));
    endpoint.unblock_reconnect();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection == tekes_agent_session::ConnectionState::Ready
            && s.connection_epoch > epoch2
            && s.sessions["a"].journal_ready
    }));
    endpoint
        .cancel_disconnect_before_reply
        .store(true, Ordering::Release);
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.running.insert("a".to_owned(), false);
        guard.journal_entries.insert(
            "a".to_owned(),
            vec![
                json!({"event": {"type": "user/message", "seq": 10, "time": 10, "data": {"text": "t1"}}}),
                json!({"event": {"type": "turn/start", "seq": 11, "time": 11, "data": {"turn": 1}}}),
                json!({"event": {"type": "turn/end", "seq": 12, "time": 12, "data": {"turn": 1, "reason": "completed"}}}),
                json!({"event": {"type": "turn/start", "seq": 13, "time": 13, "data": {"turn": 2}}}),
                json!({"event": {"type": "turn/end", "seq": 14, "time": 14, "data": {"turn": 2, "reason": "aborted"}}}),
            ],
        );
    }
    let epoch3 = runtime.inspect(|s| s.connection_epoch);
    binding.cancel_scope_at_epoch(epoch3, &RunScope::main("blocker-test", "a"));
    for _ in 0..100 {
        if binding.error_for_session(Some("a")).is_some()
            && runtime.inspect(|s| s.connection != tekes_agent_session::ConnectionState::Ready)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    endpoint.unblock_reconnect();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection == tekes_agent_session::ConnectionState::Ready
            && s.connection_epoch > epoch3
            && s.sessions["a"].journal_ready
    }));
    std::thread::sleep(Duration::from_millis(40));
    assert!(
        binding
            .error_for_session(Some("a"))
            .as_deref()
            .is_some_and(|e| e.contains("Cancel failed")),
        "completed Turn 1 error must remain visible even when Turn 2 aborted"
    );

    // 3. Explicit Remote rejection must NOT be cleared by an aborted turn/end on the same turn.
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.reject_cancel = true;
        guard.running.insert("a".to_owned(), true);
        guard.journal_entries.insert(
            "a".to_owned(),
            vec![
                json!({"event": {"type": "user/message", "seq": 20, "time": 20, "data": {"text": "t3"}}}),
                json!({"event": {"type": "turn/start", "seq": 21, "time": 21, "data": {"turn": 3}}}),
            ],
        );
    }
    binding.clear_session_error("a");
    let epoch4 = runtime.inspect(|s| s.connection_epoch);
    endpoint.drop_connection();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection != tekes_agent_session::ConnectionState::Ready
    }));
    endpoint.unblock_reconnect();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection == tekes_agent_session::ConnectionState::Ready
            && s.connection_epoch > epoch4
            && s.sessions["a"].journal_ready
    }));
    let epoch5 = runtime.inspect(|s| s.connection_epoch);
    binding.cancel_scope_at_epoch(epoch5, &RunScope::main("blocker-test", "a"));
    for _ in 0..50 {
        if binding.error_for_session(Some("a")).is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let tx_a = endpoint.inner.lock().unwrap().journal_txs["a"].clone();
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {
            "event": {"type": "turn/end", "seq": 22, "time": 22, "data": {"turn": 3, "reason": "aborted"}}
        }
    })))
    .unwrap();
    rt.block_on(wait_store(&runtime, |s| !s.sessions["a"].run_open));
    std::thread::sleep(Duration::from_millis(40));
    assert!(
        binding
            .error_for_session(Some("a"))
            .as_deref()
            .is_some_and(|e| e.contains("cancel rejected")),
        "explicit remote rejection must stay visible"
    );
    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn public_getters_return_while_watch_borrow_held_and_select_session_pending() {
    let endpoint = BlockerTestEndpoint::new();
    let (rt, runtime, binding) = setup_binding(&endpoint);
    binding.open_initial_session("a").unwrap();
    assert!(runtime.inspect(|s| s.sessions["a"].journal_ready));

    let rx = binding.controller().subscribe();
    std::thread::scope(|scope| {
        let held = rx.borrow();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let binding_c = &binding;
        let selector = scope.spawn(move || {
            started_tx.send(()).unwrap();
            binding_c.select_session("c");
        });
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        std::thread::sleep(Duration::from_millis(40));
        assert!(
            !selector.is_finished(),
            "select_session must wait on watch borrow rather than blocking other callers on ui lock"
        );
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let binding_getter = &binding;
        scope.spawn(move || {
            let err = binding_getter.session_error("a");
            let efs = binding_getter.error_for_session(Some("a"));
            let last = binding_getter.last_error();
            let ep = binding_getter.endpoint_id();
            let rows = binding_getter.sidebar_rows();
            done_tx.send((err, efs, last, ep, rows.len())).unwrap();
        });
        let (err, efs, last, ep, row_count) = done_rx
            .recv_timeout(Duration::from_millis(300))
            .expect("public getters must return immediately while watch guard is held");
        assert_eq!(err, None);
        assert_eq!(efs, None);
        assert_eq!(last, None);
        assert_eq!(ep, "blocker-test");
        assert_eq!(row_count, 3);
        drop(held);
        selector.join().unwrap();
    });

    rt.block_on(wait_store(&runtime, |s| {
        s.sessions.get("c").is_some_and(|sess| sess.journal_ready)
            && !s.sessions.get("a").is_some_and(|sess| sess.journal_ready)
    }));
    assert_eq!(
        binding.current_scope().map(|s| s.session_id),
        Some("c".to_owned())
    );
    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn stale_stop_reply_and_disconnect_reconcile_preserve_next_turn_cancel_rejection_queue_edit_failure_and_other_session_errors()
 {
    let endpoint = BlockerTestEndpoint::new();
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.running.insert("a".to_owned(), true);
        guard.journal_entries.insert(
            "a".to_owned(),
            vec![
                json!({"event": {"type": "user/message", "seq": 0, "time": 1, "data": {"text": "t1"}}}),
                json!({"event": {"type": "turn/start", "seq": 1, "time": 2, "data": {"turn": 1}}}),
            ],
        );
        guard.running.insert("b".to_owned(), true);
        guard.journal_entries.insert(
            "b".to_owned(),
            vec![
                json!({"event": {"type": "user/message", "seq": 0, "time": 1, "data": {"text": "init-b"}}}),
                json!({"event": {"type": "turn/start", "seq": 1, "time": 2, "data": {"turn": 1}}}),
            ],
        );
    }
    let (rt, runtime, binding) = setup_binding(&endpoint);

    // Record an independent error on B that must remain untouched.
    endpoint.inner.lock().unwrap().reject_cancel = true;
    binding.open_initial_session("b").unwrap();
    binding.cancel_scope(&RunScope::main("blocker-test", "b"));
    for _ in 0..50 {
        if binding.error_for_session(Some("b")).is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(binding.error_for_session(Some("b")).is_some());
    endpoint.inner.lock().unwrap().reject_cancel = false;

    // Select A on Turn 1 and issue Stop while holding session.cancel HTTP reply.
    binding.select_session("a");
    rt.block_on(wait_store(&runtime, |s| s.sessions["a"].journal_ready));
    endpoint.hold_cancel.store(true, Ordering::Release);
    binding.cancel_scope(&RunScope::main("blocker-test", "a"));
    rt.block_on(endpoint.cancel_gate_entered.notified());

    // Turn 1 aborts and Turn 2 starts on A while Turn 1's Stop HTTP reply is still held.
    let tx_a = endpoint.inner.lock().unwrap().journal_txs["a"].clone();
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {"event": {"type": "turn/end", "seq": 2, "time": 3, "data": {"turn": 1, "reason": "aborted"}}}
    })))
    .unwrap();
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {"event": {"type": "turn/start", "seq": 4, "time": 4, "data": {"turn": 2}}}
    })))
    .unwrap();
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.journal_entries.insert(
            "a".to_owned(),
            vec![
                json!({"event": {"type": "user/message", "seq": 0, "time": 1, "data": {"text": "t1"}}}),
                json!({"event": {"type": "turn/start", "seq": 1, "time": 2, "data": {"turn": 1}}}),
                json!({"event": {"type": "turn/end", "seq": 2, "time": 3, "data": {"turn": 1, "reason": "aborted"}}}),
                json!({"event": {"type": "turn/start", "seq": 4, "time": 4, "data": {"turn": 2}}}),
            ],
        );
    }
    rt.block_on(wait_store(&runtime, |s| {
        s.sessions["a"]
            .entries
            .values()
            .any(|e| e.event.kind == "turn/start" && e.event.seq == 4)
    }));

    // Scenario A: Turn 2 queue edit failure must survive late Turn 1 Stop Ok reply.
    endpoint.inner.lock().unwrap().reject_queue_update = true;
    let epoch = runtime.inspect(|s| s.connection_epoch);
    let q_res = binding.edit_queued_prompt_at_epoch_blocking(epoch, "a", "input:0", "new".into());
    assert!(q_res.is_err());
    let queue_err = binding.error_for_session(Some("a")).unwrap();
    assert!(queue_err.contains("queue-item-not-found"));

    endpoint.cancel_release.notify_one();
    std::thread::sleep(Duration::from_millis(40));
    assert_eq!(
        binding.error_for_session(Some("a")).as_deref(),
        Some(queue_err.as_str())
    );
    assert!(binding.error_for_session(Some("b")).is_some());

    // Scenario B: Turn 1 Stop held -> Turn 2 queue edit failure -> disconnect + aborted replay -> Turn 1 Ok must all preserve queue edit failure.
    binding.clear_session_error("a");
    let epoch2 = runtime.inspect(|s| s.connection_epoch);
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.running.insert("a".to_owned(), true);
        guard.journal_entries.insert(
            "a".to_owned(),
            vec![
                json!({"event": {"type": "user/message", "seq": 10, "time": 10, "data": {"text": "t3"}}}),
                json!({"event": {"type": "turn/start", "seq": 11, "time": 11, "data": {"turn": 3}}}),
            ],
        );
    }
    endpoint.drop_connection();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection != tekes_agent_session::ConnectionState::Ready
    }));
    endpoint.unblock_reconnect();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection == tekes_agent_session::ConnectionState::Ready
            && s.connection_epoch > epoch2
            && s.sessions["a"].journal_ready
    }));

    endpoint.hold_cancel.store(true, Ordering::Release);
    binding.cancel_scope(&RunScope::main("blocker-test", "a"));
    rt.block_on(endpoint.cancel_gate_entered.notified());

    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.journal_entries.insert(
            "a".to_owned(),
            vec![
                json!({"event": {"type": "user/message", "seq": 10, "time": 10, "data": {"text": "t3"}}}),
                json!({"event": {"type": "turn/start", "seq": 11, "time": 11, "data": {"turn": 3}}}),
                json!({"event": {"type": "turn/end", "seq": 12, "time": 12, "data": {"turn": 3, "reason": "aborted"}}}),
                json!({"event": {"type": "turn/start", "seq": 13, "time": 13, "data": {"turn": 4}}}),
            ],
        );
    }
    let epoch3 = runtime.inspect(|s| s.connection_epoch);
    let q_res2 =
        binding.edit_queued_prompt_at_epoch_blocking(epoch3, "a", "input:10", "new2".into());
    assert!(q_res2.is_err());

    endpoint.drop_connection();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection != tekes_agent_session::ConnectionState::Ready
    }));
    endpoint.unblock_reconnect();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection == tekes_agent_session::ConnectionState::Ready
            && s.connection_epoch > epoch3
            && s.sessions["a"].journal_ready
    }));
    endpoint.cancel_release.notify_one();
    std::thread::sleep(Duration::from_millis(40));
    assert!(
        binding
            .error_for_session(Some("a"))
            .as_deref()
            .is_some_and(|e| e.contains("queue-item-not-found")),
        "disconnect + stale Stop receipt must preserve queue edit error"
    );

    // Scenario C: Turn 3 delayed Stop -> Turn 4 cancel rejection -> Turn 3 Stop resolves / disconnects -> Turn 4 cancel rejection stays.
    binding.clear_session_error("a");
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.running.insert("a".to_owned(), true);
        guard.journal_entries.insert(
            "a".to_owned(),
            vec![
                json!({"event": {"type": "user/message", "seq": 20, "time": 20, "data": {"text": "t5"}}}),
                json!({"event": {"type": "turn/start", "seq": 21, "time": 21, "data": {"turn": 5}}}),
            ],
        );
    }
    let epoch4 = runtime.inspect(|s| s.connection_epoch);
    endpoint.drop_connection();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection != tekes_agent_session::ConnectionState::Ready
    }));
    endpoint.unblock_reconnect();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection == tekes_agent_session::ConnectionState::Ready
            && s.connection_epoch > epoch4
            && s.sessions["a"].journal_ready
    }));

    endpoint.hold_cancel.store(true, Ordering::Release);
    binding.cancel_scope(&RunScope::main("blocker-test", "a"));
    rt.block_on(endpoint.cancel_gate_entered.notified());

    let tx_a2 = endpoint.inner.lock().unwrap().journal_txs["a"].clone();
    tx_a2
        .send(Ok(json!({
            "type": "event",
            "generation": "g",
            "sessionId": "a",
            "entry": {"event": {"type": "turn/end", "seq": 22, "time": 22, "data": {"turn": 5, "reason": "aborted"}}}
        })))
        .unwrap();
    tx_a2
        .send(Ok(json!({
            "type": "event",
            "generation": "g",
            "sessionId": "a",
            "entry": {"event": {"type": "turn/start", "seq": 23, "time": 23, "data": {"turn": 6}}}
        })))
        .unwrap();
    rt.block_on(wait_store(&runtime, |s| {
        s.sessions["a"]
            .entries
            .values()
            .any(|e| e.event.kind == "turn/start" && e.event.seq == 23)
    }));

    endpoint.inner.lock().unwrap().reject_cancel = true;
    binding.cancel_scope(&RunScope::main("blocker-test", "a"));
    for _ in 0..50 {
        if binding.error_for_session(Some("a")).is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        binding
            .error_for_session(Some("a"))
            .as_deref()
            .is_some_and(|e| e.contains("cancel rejected"))
    );

    endpoint.cancel_release.notify_one();
    std::thread::sleep(Duration::from_millis(40));
    let rev_before = binding.ui_revision();
    let read_a = binding.session_error("a");
    let read_a2 = binding.error_for_session(Some("a"));
    assert_eq!(read_a, read_a2);
    assert!(
        read_a
            .as_deref()
            .is_some_and(|e| e.contains("cancel rejected"))
    );
    assert_eq!(
        binding.ui_revision(),
        rev_before,
        "read-only error getters must not bump ui_revision"
    );
    assert!(binding.error_for_session(Some("b")).is_some());

    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn stale_epoch_stop_clears_on_same_turn_retry_and_persists_without_retry() {
    let endpoint = BlockerTestEndpoint::new();
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.running.insert("a".to_owned(), true);
        guard.journal_entries.insert(
            "a".to_owned(),
            vec![
                json!({"event": {"type": "user/message", "seq": 0, "time": 1, "data": {"text": "t1"}}}),
                json!({"event": {"type": "turn/start", "seq": 1, "time": 2, "data": {"turn": 1}}}),
            ],
        );
    }
    let (rt, runtime, binding) = setup_binding(&endpoint);
    binding.open_initial_session("a").unwrap();
    rt.block_on(wait_store(&runtime, |s| {
        s.sessions["a"].journal_ready && s.sessions["a"].run_open
    }));

    // Reconnect on the same running turn and click Stop with the stale pre-reconnect epoch.
    let old_epoch = runtime.inspect(|s| s.connection_epoch);
    endpoint.drop_connection();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection != tekes_agent_session::ConnectionState::Ready
    }));
    endpoint.unblock_reconnect();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection == tekes_agent_session::ConnectionState::Ready
            && s.connection_epoch > old_epoch
            && s.sessions["a"].journal_ready
    }));
    let new_epoch = runtime.inspect(|s| s.connection_epoch);
    let scope_a = RunScope::main("blocker-test", "a");

    binding.cancel_scope_at_epoch(old_epoch, &scope_a);
    for _ in 0..50 {
        if binding.error_for_session(Some("a")).is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let stale_err = binding
        .error_for_session(Some("a"))
        .expect("stale-epoch click must record local disconnect error");
    assert!(stale_err.contains("disconnected"));
    assert_eq!(
        endpoint
            .rpcs()
            .iter()
            .filter(|(m, _)| m == "session.cancel")
            .count(),
        0
    );

    // Retry on the same active turn with new_epoch -> clears the stale-epoch error on A.
    binding.cancel_scope_at_epoch(new_epoch, &scope_a);
    for _ in 0..50 {
        if binding.error_for_session(Some("a")).is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let rev = binding.ui_revision();
    assert_eq!(binding.error_for_session(Some("a")), None);
    assert_eq!(binding.error_for_session(Some("b")), None);
    assert_eq!(
        binding.ui_revision(),
        rev,
        "read-only getter must not bump revision"
    );
    assert_eq!(
        endpoint
            .rpcs()
            .iter()
            .filter(|(m, _)| m == "session.cancel")
            .count(),
        1
    );

    // Control: stale-epoch click on Turn 2 with NO retry, turn completes naturally -> error stays.
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.running.insert("a".to_owned(), true);
        guard.journal_entries.insert(
            "a".to_owned(),
            vec![
                json!({"event": {"type": "user/message", "seq": 0, "time": 1, "data": {"text": "t1"}}}),
                json!({"event": {"type": "turn/start", "seq": 1, "time": 2, "data": {"turn": 1}}}),
                json!({"event": {"type": "turn/end", "seq": 2, "time": 3, "data": {"turn": 1, "reason": "aborted"}}}),
                json!({"event": {"type": "turn/start", "seq": 3, "time": 4, "data": {"turn": 2}}}),
            ],
        );
    }
    endpoint.drop_connection();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection != tekes_agent_session::ConnectionState::Ready
    }));
    endpoint.unblock_reconnect();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection == tekes_agent_session::ConnectionState::Ready
            && s.connection_epoch > new_epoch
            && s.sessions["a"].journal_ready
            && s.sessions["a"].run_open
    }));
    binding.cancel_scope_at_epoch(new_epoch, &scope_a);
    for _ in 0..50 {
        if binding.error_for_session(Some("a")).is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let stale_err2 = binding.error_for_session(Some("a")).unwrap();
    let tx_a = endpoint.inner.lock().unwrap().journal_txs["a"].clone();
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {"event": {"type": "turn/end", "seq": 4, "time": 5, "data": {"turn": 2, "reason": "completed"}}}
    })))
    .unwrap();
    rt.block_on(wait_store(&runtime, |s| !s.sessions["a"].run_open));
    std::thread::sleep(Duration::from_millis(40));
    assert_eq!(binding.error_for_session(Some("a")), Some(stale_err2));
    assert_eq!(
        endpoint
            .rpcs()
            .iter()
            .filter(|(m, _)| m == "session.cancel")
            .count(),
        1
    );

    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn same_turn_second_stop_http_loss_after_aborted_clears_previous_rejection_while_isolating_newer_and_no_retry_controls()
 {
    let endpoint = BlockerTestEndpoint::new();
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.running.insert("a".to_owned(), true);
        guard.journal_entries.insert(
            "a".to_owned(),
            vec![
                json!({"event": {"type": "user/message", "seq": 0, "time": 1, "data": {"text": "t1"}}}),
                json!({"event": {"type": "turn/start", "seq": 1, "time": 2, "data": {"turn": 1}}}),
            ],
        );
        guard.running.insert("b".to_owned(), true);
        guard.journal_entries.insert(
            "b".to_owned(),
            vec![
                json!({"event": {"type": "user/message", "seq": 0, "time": 1, "data": {"text": "tb"}}}),
                json!({"event": {"type": "turn/start", "seq": 1, "time": 2, "data": {"turn": 1}}}),
            ],
        );
    }
    let (rt, runtime, binding) = setup_binding(&endpoint);
    binding.open_initial_session("b").unwrap();
    endpoint.inner.lock().unwrap().reject_cancel = true;
    binding.cancel_scope(&RunScope::main("blocker-test", "b"));
    for _ in 0..50 {
        if binding.error_for_session(Some("b")).is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(binding.error_for_session(Some("b")).is_some());

    // 1. Immediate ConfirmedAborted path: 1st Stop rejected -> turn/end aborted -> 2nd Stop HTTP-only transport loss -> A clears.
    binding.select_session("a");
    rt.block_on(wait_store(&runtime, |s| {
        s.sessions["a"].journal_ready && s.sessions["a"].run_open
    }));
    let scope_a = RunScope::main("blocker-test", "a");
    let epoch = runtime.inspect(|s| s.connection_epoch);
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    for _ in 0..50 {
        if binding.error_for_session(Some("a")).is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        binding
            .error_for_session(Some("a"))
            .as_deref()
            .is_some_and(|e| e.contains("cancel rejected"))
    );

    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.reject_cancel = false;
        guard.transport_error_cancel = true;
    }
    endpoint.hold_cancel.store(true, Ordering::Release);
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    rt.block_on(endpoint.cancel_gate_entered.notified());

    let tx_a = endpoint.inner.lock().unwrap().journal_txs["a"].clone();
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {"event": {"type": "turn/end", "seq": 2, "time": 3, "data": {"turn": 1, "reason": "aborted"}}}
    })))
    .unwrap();
    endpoint
        .inner
        .lock()
        .unwrap()
        .running
        .insert("a".to_owned(), false);
    rt.block_on(wait_store(&runtime, |s| !s.sessions["a"].run_open));
    endpoint.cancel_release.notify_one();
    for _ in 0..50 {
        if binding.error_for_session(Some("a")).is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(binding.error_for_session(Some("a")), None);
    assert!(binding.error_for_session(Some("b")).is_some());

    // 2. Deferred Pending -> ConfirmedAborted path: 2nd Stop drops HTTP receipt BEFORE turn/end(aborted) arrives.
    endpoint
        .inner
        .lock()
        .unwrap()
        .running
        .insert("a".to_owned(), true);
    let tx_a2 = endpoint.inner.lock().unwrap().journal_txs["a"].clone();
    tx_a2
        .send(Ok(json!({
            "type": "event",
            "generation": "g",
            "sessionId": "a",
            "entry": {"event": {"type": "turn/start", "seq": 3, "time": 4, "data": {"turn": 2}}}
        })))
        .unwrap();
    rt.block_on(wait_store(&runtime, |s| s.sessions["a"].run_open));
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.reject_cancel = true;
        guard.transport_error_cancel = false;
    }
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    for _ in 0..50 {
        if binding.error_for_session(Some("a")).is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.reject_cancel = false;
        guard.transport_error_cancel = true;
    }
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    for _ in 0..50 {
        if binding
            .error_for_session(Some("a"))
            .as_deref()
            .is_some_and(|e| e.contains("receipt socket closed"))
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    tx_a2
        .send(Ok(json!({
            "type": "event",
            "generation": "g",
            "sessionId": "a",
            "entry": {"event": {"type": "turn/end", "seq": 4, "time": 5, "data": {"turn": 2, "reason": "aborted"}}}
        })))
        .unwrap();
    rt.block_on(wait_store(&runtime, |s| !s.sessions["a"].run_open));
    for _ in 0..50 {
        if binding.error_for_session(Some("a")).is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(binding.error_for_session(Some("a")), None);

    // 3. Control: Single explicit rejection without retry stays visible even when turn later aborts.
    tx_a2
        .send(Ok(json!({
            "type": "event",
            "generation": "g",
            "sessionId": "a",
            "entry": {"event": {"type": "turn/start", "seq": 5, "time": 6, "data": {"turn": 3}}}
        })))
        .unwrap();
    rt.block_on(wait_store(&runtime, |s| s.sessions["a"].run_open));
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.reject_cancel = true;
        guard.transport_error_cancel = false;
    }
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    for _ in 0..50 {
        if binding.error_for_session(Some("a")).is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    tx_a2
        .send(Ok(json!({
            "type": "event",
            "generation": "g",
            "sessionId": "a",
            "entry": {"event": {"type": "turn/end", "seq": 6, "time": 7, "data": {"turn": 3, "reason": "aborted"}}}
        })))
        .unwrap();
    rt.block_on(wait_store(&runtime, |s| !s.sessions["a"].run_open));
    std::thread::sleep(Duration::from_millis(40));
    assert!(
        binding
            .error_for_session(Some("a"))
            .as_deref()
            .is_some_and(|e| e.contains("cancel rejected"))
    );

    // 4. Newer queue-edit error stays preserved when 2nd Stop hits HTTP-only transport loss + ConfirmedAborted.
    binding.clear_session_error("a");
    tx_a2
        .send(Ok(json!({
            "type": "event",
            "generation": "g",
            "sessionId": "a",
            "entry": {"event": {"type": "turn/start", "seq": 7, "time": 8, "data": {"turn": 4}}}
        })))
        .unwrap();
    rt.block_on(wait_store(&runtime, |s| s.sessions["a"].run_open));
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.reject_cancel = false;
        guard.transport_error_cancel = true;
        guard.reject_queue_update = true;
    }
    endpoint.hold_cancel.store(true, Ordering::Release);
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    rt.block_on(endpoint.cancel_gate_entered.notified());
    let q_res = binding.edit_queued_prompt_at_epoch_blocking(epoch, "a", "input:0", "new".into());
    assert!(q_res.is_err());
    tx_a2
        .send(Ok(json!({
            "type": "event",
            "generation": "g",
            "sessionId": "a",
            "entry": {"event": {"type": "turn/end", "seq": 8, "time": 9, "data": {"turn": 4, "reason": "aborted"}}}
        })))
        .unwrap();
    rt.block_on(wait_store(&runtime, |s| !s.sessions["a"].run_open));
    endpoint.cancel_release.notify_one();
    std::thread::sleep(Duration::from_millis(40));
    assert!(
        binding
            .error_for_session(Some("a"))
            .as_deref()
            .is_some_and(|e| e.contains("queue-item-not-found")),
        "newer queue-edit failure must remain after HTTP-only Stop loss on ConfirmedAborted"
    );
    assert!(binding.error_for_session(Some("b")).is_some());

    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn overlapping_same_turn_stop_late_rejection_is_superseded_while_later_turns_and_single_rejections_preserve_errors()
 {
    let endpoint = BlockerTestEndpoint::new();
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.running.insert("a".to_owned(), true);
        guard.journal_entries.insert(
            "a".to_owned(),
            vec![
                json!({"event": {"type": "user/message", "seq": 0, "time": 1, "data": {"text": "t1"}}}),
                json!({"event": {"type": "turn/start", "seq": 1, "time": 2, "data": {"turn": 1}}}),
            ],
        );
        guard.running.insert("b".to_owned(), true);
        guard.journal_entries.insert(
            "b".to_owned(),
            vec![
                json!({"event": {"type": "user/message", "seq": 0, "time": 1, "data": {"text": "tb"}}}),
                json!({"event": {"type": "turn/start", "seq": 1, "time": 2, "data": {"turn": 1}}}),
            ],
        );
    }
    let (rt, runtime, binding, wake) = setup_binding_with_wake(&endpoint);
    binding.open_initial_session("b").unwrap();
    endpoint.inner.lock().unwrap().reject_cancel = true;
    binding.cancel_scope(&RunScope::main("blocker-test", "b"));
    wait_until("B cancel rejection visible", || {
        binding.error_for_session(Some("b")).is_some()
    });
    let b_err = binding.error_for_session(Some("b"));
    assert!(b_err.is_some());
    endpoint.inner.lock().unwrap().reject_cancel = false;

    binding.select_session("a");
    rt.block_on(wait_store(&runtime, |s| {
        s.sessions["a"].journal_ready && s.sessions["a"].run_open
    }));
    wait_until("session A controls settled", || {
        binding.selected_model() == "model-a" && binding.selected_preset() == "Full Access"
    });
    std::thread::sleep(Duration::from_millis(20));
    let scope_a = RunScope::main("blocker-test", "a");
    let epoch = runtime.inspect(|s| s.connection_epoch);

    // 1. Overlapping same-turn Stop #1 parked rejection + Stop #2 Ok(()) -> late Stop #1 rejection suppressed.
    endpoint.park_reject_cancel.store(true, Ordering::Release);
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    rt.block_on(endpoint.park_reject_entered.notified());
    assert_eq!(binding.error_for_session(Some("a")), None);

    let w_stop2 = wake.count();
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    wake.wait_at_least(w_stop2 + 1);
    assert_eq!(
        endpoint
            .rpcs()
            .iter()
            .filter(|(m, p)| m == "session.cancel" && p["sessionId"] == "a")
            .count(),
        2
    );
    let tx_a = endpoint.inner.lock().unwrap().journal_txs["a"].clone();
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {"event": {"type": "turn/end", "seq": 2, "time": 3, "data": {"turn": 1, "reason": "aborted"}}}
    })))
    .unwrap();
    endpoint
        .inner
        .lock()
        .unwrap()
        .running
        .insert("a".to_owned(), false);
    rt.block_on(wait_store(&runtime, |s| !s.sessions["a"].run_open));
    let w_stop1 = wake.count();
    endpoint.park_reject_release.notify_one();
    wake.wait_at_least(w_stop1 + 1);
    let rev = binding.ui_revision();
    assert_eq!(binding.session_error("a"), None);
    assert_eq!(binding.error_for_session(Some("a")), None);
    assert_eq!(binding.ui_revision(), rev);
    assert_eq!(binding.error_for_session(Some("b")), b_err);

    // 2. Overlapping Stop #1 parked rejection + Stop #2 HTTP loss with immediate ConfirmedAborted.
    endpoint
        .inner
        .lock()
        .unwrap()
        .running
        .insert("a".to_owned(), true);
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {"event": {"type": "turn/start", "seq": 3, "time": 4, "data": {"turn": 2}}}
    })))
    .unwrap();
    rt.block_on(wait_store(&runtime, |s| s.sessions["a"].run_open));
    endpoint.park_reject_cancel.store(true, Ordering::Release);
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    rt.block_on(endpoint.park_reject_entered.notified());

    endpoint.inner.lock().unwrap().transport_error_cancel = true;
    endpoint.hold_cancel.store(true, Ordering::Release);
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    rt.block_on(endpoint.cancel_gate_entered.notified());
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {"event": {"type": "turn/end", "seq": 4, "time": 5, "data": {"turn": 2, "reason": "aborted"}}}
    })))
    .unwrap();
    endpoint
        .inner
        .lock()
        .unwrap()
        .running
        .insert("a".to_owned(), false);
    rt.block_on(wait_store(&runtime, |s| !s.sessions["a"].run_open));
    let w2 = wake.count();
    endpoint.cancel_release.notify_one();
    wake.wait_at_least(w2 + 1);
    let w1 = wake.count();
    endpoint.park_reject_release.notify_one();
    wake.wait_at_least(w1 + 1);
    assert_eq!(binding.session_error("a"), None);
    assert_eq!(binding.error_for_session(Some("a")), None);

    // 3. Overlapping Stop #1 parked rejection + Stop #2 HTTP loss Pending -> deferred reconcile_uncertain_cancels.
    endpoint
        .inner
        .lock()
        .unwrap()
        .running
        .insert("a".to_owned(), true);
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {"event": {"type": "turn/start", "seq": 5, "time": 6, "data": {"turn": 3}}}
    })))
    .unwrap();
    rt.block_on(wait_store(&runtime, |s| s.sessions["a"].run_open));
    endpoint.park_reject_cancel.store(true, Ordering::Release);
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    rt.block_on(endpoint.park_reject_entered.notified());

    binding.cancel_scope_at_epoch(epoch, &scope_a);
    wait_until("Pending transport error recorded", || {
        binding
            .error_for_session(Some("a"))
            .as_deref()
            .is_some_and(|e| e.contains("receipt socket closed"))
    });
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {"event": {"type": "turn/end", "seq": 6, "time": 7, "data": {"turn": 3, "reason": "aborted"}}}
    })))
    .unwrap();
    rt.block_on(wait_store(&runtime, |s| !s.sessions["a"].run_open));
    wait_until("deferred reconcile cleared A error", || {
        binding.error_for_session(Some("a")).is_none()
    });
    let w3_rej = wake.count();
    endpoint.park_reject_release.notify_one();
    wake.wait_at_least(w3_rej + 1);
    assert_eq!(binding.session_error("a"), None);
    assert_eq!(binding.error_for_session(Some("a")), None);

    // 4. Turn 4 succeeds via overlapping Stops -> Turn 5 Stop #3 rejects -> Turn 5 rejection preserved even after Turn 4 Stop #1 releases.
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {"event": {"type": "turn/start", "seq": 7, "time": 8, "data": {"turn": 4}}}
    })))
    .unwrap();
    rt.block_on(wait_store(&runtime, |s| s.sessions["a"].run_open));
    endpoint.inner.lock().unwrap().transport_error_cancel = false;
    endpoint.park_reject_cancel.store(true, Ordering::Release);
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    rt.block_on(endpoint.park_reject_entered.notified());
    let w4_ok = wake.count();
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    wake.wait_at_least(w4_ok + 1);
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {"event": {"type": "turn/end", "seq": 8, "time": 9, "data": {"turn": 4, "reason": "aborted"}}}
    })))
    .unwrap();
    rt.block_on(wait_store(&runtime, |s| !s.sessions["a"].run_open));
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {"event": {"type": "turn/start", "seq": 9, "time": 10, "data": {"turn": 5}}}
    })))
    .unwrap();
    rt.block_on(wait_store(&runtime, |s| s.sessions["a"].run_open));
    endpoint.inner.lock().unwrap().reject_cancel = true;
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    wait_until("Turn 5 Stop #3 rejection visible", || {
        binding
            .error_for_session(Some("a"))
            .as_deref()
            .is_some_and(|e| e.contains("cancel rejected"))
    });
    let w4_rej = wake.count();
    endpoint.park_reject_release.notify_one();
    wake.wait_at_least(w4_rej + 1);
    assert!(
        binding
            .error_for_session(Some("a"))
            .as_deref()
            .is_some_and(|e| e.contains("cancel rejected"))
    );
    assert_eq!(binding.error_for_session(Some("b")), b_err);

    // 5. Adjacent-order ("between") permutation: Click 1 -> Click 2 -> Recv 1 rejects -> Recv 2 succeeds Ok(()) AND HTTP loss ConfirmedAborted.
    binding.clear_session_error("a");
    endpoint.inner.lock().unwrap().reject_cancel = false;
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {"event": {"type": "turn/end", "seq": 10, "time": 11, "data": {"turn": 5, "reason": "aborted"}}}
    })))
    .unwrap();
    rt.block_on(wait_store(&runtime, |s| !s.sessions["a"].run_open));
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {"event": {"type": "turn/start", "seq": 11, "time": 12, "data": {"turn": 6}}}
    })))
    .unwrap();
    rt.block_on(wait_store(&runtime, |s| s.sessions["a"].run_open));

    endpoint.park_reject_cancel.store(true, Ordering::Release);
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    rt.block_on(endpoint.park_reject_entered.notified());

    endpoint.hold_cancel.store(true, Ordering::Release);
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    rt.block_on(endpoint.cancel_gate_entered.notified());

    endpoint.park_reject_release.notify_one();
    wait_until("between Recv 1 rejection visible", || {
        binding
            .session_error("a")
            .as_deref()
            .is_some_and(|e| e.contains("cancel rejected"))
            && binding
                .error_for_session(Some("a"))
                .as_deref()
                .is_some_and(|e| e.contains("cancel rejected"))
    });

    endpoint.cancel_release.notify_one();
    wait_until("between Recv 2 clears error", || {
        binding.session_error("a").is_none() && binding.error_for_session(Some("a")).is_none()
    });
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {"event": {"type": "turn/end", "seq": 12, "time": 13, "data": {"turn": 6, "reason": "aborted"}}}
    })))
    .unwrap();
    rt.block_on(wait_store(&runtime, |s| !s.sessions["a"].run_open));
    assert_eq!(binding.session_error("a"), None);
    assert_eq!(binding.error_for_session(Some("a")), None);
    assert_eq!(binding.error_for_session(Some("b")), b_err);

    // 5b. "between" order when Stop #2 loses HTTP receipt after turn/end(aborted) is already visible (ConfirmedAborted).
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {"event": {"type": "turn/start", "seq": 13, "time": 14, "data": {"turn": 7}}}
    })))
    .unwrap();
    rt.block_on(wait_store(&runtime, |s| s.sessions["a"].run_open));
    endpoint.park_reject_cancel.store(true, Ordering::Release);
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    rt.block_on(endpoint.park_reject_entered.notified());

    endpoint.inner.lock().unwrap().transport_error_cancel = true;
    endpoint.hold_cancel.store(true, Ordering::Release);
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    rt.block_on(endpoint.cancel_gate_entered.notified());

    endpoint.park_reject_release.notify_one();
    wait_until("between HTTP-loss Recv 1 rejection visible", || {
        binding
            .error_for_session(Some("a"))
            .as_deref()
            .is_some_and(|e| e.contains("cancel rejected"))
    });

    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {"event": {"type": "turn/end", "seq": 14, "time": 15, "data": {"turn": 7, "reason": "aborted"}}}
    })))
    .unwrap();
    rt.block_on(wait_store(&runtime, |s| !s.sessions["a"].run_open));
    endpoint.cancel_release.notify_one();
    wait_until("between HTTP-loss ConfirmedAborted clears error", || {
        binding.session_error("a").is_none() && binding.error_for_session(Some("a")).is_none()
    });
    assert_eq!(binding.error_for_session(Some("b")), b_err);

    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn sidebar_title_uses_canonical_projections_and_replaces_or_reverts_local_rename() {
    let endpoint = BlockerTestEndpoint::new();
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.custom_items = Some(vec![
            json!({
                "sessionId": "a",
                "running": false,
                "blank": false,
                "updatedAt": 10,
                "title": "Legacy A"
            }),
            json!({
                "sessionId": "b",
                "running": false,
                "blank": false,
                "updatedAt": 20,
                "projections": {"asOfSeq": 10, "values": {"title": "Acceptance B"}}
            }),
            json!({
                "sessionId": "c",
                "running": false,
                "blank": false,
                "updatedAt": 30,
                "title": "Stale Legacy C",
                "name": "Stale Name C"
            }),
            json!({
                "sessionId": "d",
                "running": false,
                "blank": false,
                "updatedAt": 40
            }),
        ]);
        guard.control_projections = vec![json!({
            "sessionId": "c",
            "projections": {"asOfSeq": 5, "values": {"title": null}}
        })];
    }

    let rt = tokio::runtime::Runtime::new().unwrap();
    let runtime = rt.block_on(async {
        let runtime = Arc::new(SessionRuntime::spawn(
            "blocker-test",
            Arc::new(endpoint.clone()),
        ));
        runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
        runtime
    });
    let controller =
        rt.block_on(async { Arc::new(RunController::new(Arc::clone(&runtime)).unwrap()) });
    let wake = Arc::new(CountingWake::default());
    let binding = {
        let _guard = rt.enter();
        RunWorkbenchBinding::new(
            "blocker-test",
            Arc::clone(&runtime),
            controller,
            rt.handle().clone(),
            wake.clone(),
        )
    };
    binding.open_initial_session("a").unwrap();

    let rows = binding.sidebar_rows();
    let title_for = |id: &str| {
        binding
            .sidebar_rows()
            .into_iter()
            .find(|r| r.session_id == id)
            .map(|r| r.title)
    };
    assert_eq!(title_for("a").as_deref(), Some("Legacy A"));
    assert_eq!(title_for("b").as_deref(), Some("Acceptance B"));
    assert_eq!(title_for("c").as_deref(), Some("New conversation"));
    assert_eq!(title_for("d").as_deref(), Some("New conversation"));
    assert_eq!(rows.len(), 4);

    binding.set_search_query("Acceptance B".to_owned());
    let hits = binding.sidebar_rows();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].session_id, "b");
    binding.set_search_query(String::new());

    binding.rename_session("b", "B via binding".to_owned());
    wait_until("local rename RPC sent", || {
        endpoint.rpcs().iter().any(|(m, p)| {
            m == "session.rename" && p["sessionId"] == "b" && p["title"] == "B via binding"
        })
    });
    rt.block_on(runtime.rename("b", "B authoritative newer title", "ext-rename"))
        .unwrap();
    rt.block_on(runtime.reconnect()).unwrap();
    wait_until("reconnected title matches newer authority", || {
        title_for("b").as_deref() == Some("B authoritative newer title")
    });

    // Rejected rename must report session error and revert back to the authoritative title.
    endpoint.inner.lock().unwrap().reject_rename = true;
    binding.rename_session("b", "Rejected B Title".to_owned());
    wait_until("rejected rename records session error and reverts", || {
        binding
            .session_error("b")
            .as_deref()
            .is_some_and(|e| e.contains("Rename failed"))
            && title_for("b").as_deref() == Some("B authoritative newer title")
    });
    assert_eq!(binding.session_error("a"), None);

    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn sidebar_archive_and_unarchive_preserve_authoritative_state_and_route_explicitly() {
    let endpoint = BlockerTestEndpoint::new();
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.use_workspace_stream = true;
        guard.custom_items = Some(vec![
            json!({"sessionId": "a", "running": false, "blank": false, "updatedAt": 10}),
            json!({"sessionId": "b", "running": false, "blank": false, "updatedAt": 20, "projections": {"asOfSeq": 1, "values": {"title": "Restorable B"}}}),
        ]);
    }

    let rt = tokio::runtime::Runtime::new().unwrap();
    let runtime = rt.block_on(async {
        let runtime = Arc::new(SessionRuntime::spawn(
            "blocker-test",
            Arc::new(endpoint.clone()),
        ));
        runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
        runtime
    });
    let controller =
        rt.block_on(async { Arc::new(RunController::new(Arc::clone(&runtime)).unwrap()) });
    let binding = {
        let _guard = rt.enter();
        RunWorkbenchBinding::new(
            "blocker-test",
            Arc::clone(&runtime),
            controller,
            rt.handle().clone(),
            Arc::new(NoopWakeNotifier),
        )
    };
    binding.open_initial_session("a").unwrap();

    // 1. Archive B -> reconnect removes B from active inventory.
    binding.archive_session("b");
    wait_until("first archive RPC finished", || {
        endpoint
            .rpcs()
            .iter()
            .any(|(m, p)| m == "session.archive" && p["sessionId"] == "b")
    });
    rt.block_on(runtime.reconnect()).unwrap();
    assert!(
        !binding
            .sidebar_rows()
            .into_iter()
            .any(|r| r.session_id == "b")
    );

    // 2. Independent external unarchive + reconnect -> B returns as active (archived == false).
    rt.block_on(runtime.unarchive("b", "ext-restore")).unwrap();
    rt.block_on(runtime.reconnect()).unwrap();
    let b_row = binding
        .sidebar_rows()
        .into_iter()
        .find(|r| r.session_id == "b")
        .expect("restored B visible");
    assert!(!b_row.archived);

    // 3. Second archive_session MUST send session.archive (never session.unarchive).
    let prev_rpc_len = endpoint.rpcs().len();
    binding.archive_session("b");
    wait_until("second archive RPC sent", || {
        endpoint.rpcs()[prev_rpc_len..]
            .iter()
            .any(|(m, p)| m == "session.archive" && p["sessionId"] == "b")
    });
    assert!(
        !endpoint.rpcs()[prev_rpc_len..]
            .iter()
            .any(|(m, _)| m == "session.unarchive")
    );
    rt.block_on(runtime.reconnect()).unwrap();

    // 4. Restore again, then verify unarchive_session failure reports error and preserves active state,
    // and archive_session failure reports error and reverts optimistic archive state.
    rt.block_on(runtime.unarchive("b", "ext-restore-2"))
        .unwrap();
    rt.block_on(runtime.reconnect()).unwrap();
    binding.unarchive_session("b");
    wait_until("unarchive on active B surfaces session error", || {
        binding
            .session_error("b")
            .as_deref()
            .is_some_and(|e| e.contains("Unarchive failed"))
    });
    assert_eq!(binding.session_error("a"), None);

    endpoint.inner.lock().unwrap().reject_archive = true;
    binding.archive_session("b");
    wait_until(
        "rejected archive surfaces error and keeps B unarchived",
        || {
            binding
                .session_error("b")
                .as_deref()
                .is_some_and(|e| e.contains("Archive failed"))
                && binding
                    .sidebar_rows()
                    .into_iter()
                    .find(|r| r.session_id == "b")
                    .is_some_and(|r| !r.archived)
        },
    );

    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn rename_receipt_ordering_drops_stale_rejection_and_preserves_latest_error() {
    let endpoint = BlockerTestEndpoint::new();
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.custom_items = Some(vec![
            json!({"sessionId": "a", "running": false, "blank": false, "updatedAt": 10, "projections": {"asOfSeq": 1, "values": {"title": "Acceptance A"}}}),
            json!({"sessionId": "b", "running": false, "blank": false, "updatedAt": 20, "projections": {"asOfSeq": 10, "values": {"title": "Acceptance B"}}}),
        ]);
    }
    let old_invalid_entered = Arc::new(Notify::new());
    let old_invalid_release = Arc::new(Notify::new());
    let old_valid_entered = Arc::new(Notify::new());
    let old_valid_release = Arc::new(Notify::new());
    let before_new_rejection_entered = Arc::new(Notify::new());
    let before_new_rejection_release = Arc::new(Notify::new());
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.rename_gates.insert(
            "X".repeat(257),
            (
                Arc::clone(&old_invalid_entered),
                Arc::clone(&old_invalid_release),
                Err("title-invalid".to_owned()),
            ),
        );
        guard.rename_gates.insert(
            "Earlier valid title".to_owned(),
            (
                Arc::clone(&old_valid_entered),
                Arc::clone(&old_valid_release),
                Ok(()),
            ),
        );
        guard.rename_gates.insert(
            "Earlier before rejected correction".to_owned(),
            (
                Arc::clone(&before_new_rejection_entered),
                Arc::clone(&before_new_rejection_release),
                Ok(()),
            ),
        );
    }

    let (rt, runtime, binding) = setup_binding(&endpoint);
    binding.open_initial_session("a").unwrap();

    let wait_title = |expected: &str| {
        for _ in 0..80 {
            if binding
                .sidebar_rows()
                .into_iter()
                .find(|r| r.session_id == "b")
                .is_some_and(|r| r.title == expected)
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for title `{expected}`");
    };
    wait_title("Acceptance B");

    // Case 1: old invalid held -> newer valid succeeds -> delayed old rejection must NOT show Rename failed.
    let gate_1 = old_invalid_entered.notified();
    binding.rename_session("b", "X".repeat(257));
    rt.block_on(gate_1);
    binding.rename_session("b", "After invalid newer title".to_owned());
    wait_title("After invalid newer title");
    std::thread::sleep(Duration::from_millis(40));
    assert_eq!(binding.session_error("b"), None);

    old_invalid_release.notify_one();
    std::thread::sleep(Duration::from_millis(80));
    assert_eq!(binding.session_error("b"), None);
    assert_eq!(binding.session_error("a"), None);
    assert_eq!(binding.current_scope().unwrap().session_id, "a");
    wait_title("After invalid newer title");
    binding.set_search_query("After invalid newer title".to_owned());
    assert!(
        binding
            .sidebar_rows()
            .into_iter()
            .any(|r| r.session_id == "b" && r.title == "After invalid newer title")
    );
    binding.set_search_query(String::new());

    // Case 2: old valid held -> newer valid succeeds -> delayed old success keeps newest title & clean error.
    let gate_2 = old_valid_entered.notified();
    binding.rename_session("b", "Earlier valid title".to_owned());
    rt.block_on(gate_2);
    binding.rename_session("b", "After valid newer title".to_owned());
    wait_title("After valid newer title");
    old_valid_release.notify_one();
    std::thread::sleep(Duration::from_millis(80));
    assert_eq!(binding.session_error("b"), None);
    assert_eq!(binding.session_error("a"), None);
    wait_title("After valid newer title");

    // Case 3: earlier valid held -> latest invalid rejects (error recorded) -> old valid resolves -> latest error kept.
    let gate_3 = before_new_rejection_entered.notified();
    binding.rename_session("b", "Earlier before rejected correction".to_owned());
    rt.block_on(gate_3);
    binding.rename_session("b", "Y".repeat(257));
    for _ in 0..80 {
        if binding.session_error("b").is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let latest_err = binding.session_error("b");
    assert!(
        latest_err
            .as_deref()
            .is_some_and(|e| e.contains("Rename failed"))
    );

    before_new_rejection_release.notify_one();
    std::thread::sleep(Duration::from_millis(80));
    assert_eq!(binding.session_error("b"), latest_err);
    assert_eq!(binding.session_error("a"), None);
    assert_eq!(binding.current_scope().unwrap().session_id, "a");

    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn archive_in_flight_reconnect_and_ws_drop_preserve_selected_running_subscription() {
    let endpoint = BlockerTestEndpoint::new();
    endpoint.supports_recovery.store(true, Ordering::Release);
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.use_workspace_stream = true;
        guard.running.insert("a".to_owned(), true);
        guard.reject_archive = true;
        guard.journal_entries.insert(
            "a".to_owned(),
            vec![
                json!({"event": {"type": "turn/start", "seq": 0, "time": 1, "data": {"turn": 1}}}),
            ],
        );
        guard.custom_items = Some(vec![
            json!({"sessionId": "a", "running": true, "blank": false, "updatedAt": 10, "projections": {"asOfSeq": 1, "values": {"title": "Running A"}}}),
            json!({"sessionId": "b", "running": false, "blank": false, "updatedAt": 20, "projections": {"asOfSeq": 1, "values": {"title": "Idle B"}}}),
        ]);
    }
    let (rt, runtime, binding) = setup_binding(&endpoint);
    binding.open_initial_session("a").unwrap();
    rt.block_on(wait_store(&runtime, |s| {
        s.sessions["a"].journal_ready && s.sessions["a"].run_open
    }));

    // 1. Explicit reconnect while archive_session("a") is in-flight preserves lease & recovery and does not replay archive.
    let epoch_before_1 = runtime.inspect(|s| s.connection_epoch);
    endpoint.park_archive.store(true, Ordering::Release);
    let entered_1 = endpoint.park_archive_entered.notified();
    binding.archive_session("a");
    rt.block_on(entered_1);
    rt.block_on(runtime.reconnect()).unwrap();
    endpoint.park_archive_release.notify_one();

    rt.block_on(wait_store(&runtime, |s| {
        s.connection == tekes_agent_session::ConnectionState::Ready
            && s.connection_epoch > epoch_before_1
            && endpoint.connect_count.load(Ordering::Acquire) >= 2
            && s.sessions["a"].journal_ready
            && s.sessions["a"].run_open
    }));
    let archive_rpcs_1 = endpoint
        .rpcs()
        .iter()
        .filter(|(m, p)| m == "session.archive" && p["sessionId"] == "a")
        .count();
    assert_eq!(archive_rpcs_1, 1);
    assert_eq!(
        endpoint
            .rpcs()
            .iter()
            .rfind(|(m, _)| m == "sessions.recover")
            .map(|(_, p)| p.clone()),
        Some(json!({ "sessionIds": ["a"] }))
    );

    // 2. Transport / WS drop while archive_session("a") is in-flight also restores lease, recovery, and delivers final answer without B->A switch.
    let epoch_before_2 = runtime.inspect(|s| s.connection_epoch);
    endpoint.park_archive.store(true, Ordering::Release);
    let entered_2 = endpoint.park_archive_entered.notified();
    binding.archive_session("a");
    rt.block_on(entered_2);
    endpoint.drop_connection();
    endpoint.unblock_reconnect();

    rt.block_on(wait_store(&runtime, |s| {
        s.connection == tekes_agent_session::ConnectionState::Ready
            && s.connection_epoch > epoch_before_2
            && endpoint.connect_count.load(Ordering::Acquire) >= 3
            && s.sessions["a"].journal_ready
            && s.sessions["a"].run_open
    }));
    endpoint.park_archive_release.notify_one();
    let archive_rpcs_2 = endpoint
        .rpcs()
        .iter()
        .filter(|(m, p)| m == "session.archive" && p["sessionId"] == "a")
        .count();
    assert_eq!(archive_rpcs_2, 2);
    assert_eq!(
        endpoint
            .rpcs()
            .iter()
            .rfind(|(m, _)| m == "sessions.recover")
            .map(|(_, p)| p.clone()),
        Some(json!({ "sessionIds": ["a"] }))
    );

    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.running.insert("a".to_owned(), false);
        let tx_a = guard.journal_txs_by_conn[&(3, "a".to_owned())].clone();
        tx_a.send(Ok(json!({
            "type": "event",
            "generation": "g",
            "sessionId": "a",
            "entry": {"event": {"type": "assistant/message", "seq": 1, "time": 2, "data": {"turn": 1, "step": 1, "sessionFinal": true, "message": {"id": "ma-1", "content": [{"type": "text", "text": "final answer"}]}}}}
        })))
        .unwrap();
        tx_a.send(Ok(json!({
            "type": "event",
            "generation": "g",
            "sessionId": "a",
            "entry": {"event": {"type": "turn/end", "seq": 2, "time": 3, "data": {"turn": 1, "reason": "completed"}}}
        })))
        .unwrap();
    }

    rt.block_on(wait_store(&runtime, |s| !s.sessions["a"].run_open));
    wait_until(
        "RunWorkbenchBinding automatically receives final answer on A",
        || {
            let snap = binding.run_snapshot();
            binding.current_scope().is_some_and(|s| s.session_id == "a")
                && !snap.run.as_ref().is_some_and(|r| r.can_cancel)
                && snap
                    .transcript
                    .items
                    .iter()
                    .any(|item| item.text.contains("final answer"))
        },
    );
    assert!(
        binding
            .sidebar_rows()
            .into_iter()
            .find(|r| r.session_id == "a")
            .is_some_and(|r| !r.archived)
    );

    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn archive_success_receipt_lost_across_ws_drop_releases_lease_clears_error_and_never_reopens() {
    let endpoint = BlockerTestEndpoint::new();
    endpoint.supports_recovery.store(true, Ordering::Release);
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.use_workspace_stream = true;
        guard.custom_items = Some(vec![
            json!({"sessionId": "a", "running": false, "blank": false, "updatedAt": 10, "projections": {"asOfSeq": 1, "values": {"title": "Idle A"}}}),
            json!({"sessionId": "b", "running": false, "blank": false, "updatedAt": 20, "projections": {"asOfSeq": 1, "values": {"title": "Idle B"}}}),
        ]);
    }
    let (rt, runtime, binding) = setup_binding(&endpoint);
    binding.open_initial_session("a").unwrap();
    rt.block_on(wait_store(&runtime, |s| s.sessions["a"].journal_ready));

    endpoint.park_archive.store(true, Ordering::Release);
    let entered = endpoint.park_archive_entered.notified();
    binding.archive_session("a");
    rt.block_on(entered);
    assert!(
        endpoint
            .inner
            .lock()
            .unwrap()
            .archived_ids
            .contains(&"a".to_owned())
    );

    let epoch_before = runtime.inspect(|s| s.connection_epoch);
    endpoint.drop_connection();
    endpoint.unblock_reconnect();

    rt.block_on(wait_store(&runtime, |s| {
        s.connection == tekes_agent_session::ConnectionState::Ready
            && s.connection_epoch > epoch_before
            && endpoint.connect_count.load(Ordering::Acquire) >= 2
    }));
    endpoint.park_archive_release.notify_one();

    wait_until("uncertain archive error cleared and A removed", || {
        binding.session_error("a").is_none()
            && binding.error_for_session(Some("a")).is_none()
            && !binding
                .sidebar_rows()
                .into_iter()
                .any(|r| r.session_id == "a")
    });

    rt.block_on(runtime.reconnect()).unwrap();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection == tekes_agent_session::ConnectionState::Ready && s.connection_epoch >= 3
    }));

    let late_opens: Vec<_> = endpoint
        .journal_opens()
        .into_iter()
        .filter(|(c, _)| *c >= 2)
        .collect();
    assert!(
        late_opens.is_empty(),
        "unexpected late journal opens: {late_opens:?}"
    );
    let recover_rpcs: Vec<_> = endpoint
        .rpcs()
        .into_iter()
        .filter(|(m, _)| m == "sessions.recover")
        .collect();
    assert!(
        recover_rpcs.is_empty(),
        "unexpected sessions.recover: {recover_rpcs:?}"
    );
    assert_eq!(
        endpoint
            .rpcs()
            .iter()
            .filter(|(m, p)| m == "session.archive" && p["sessionId"] == "a")
            .count(),
        1
    );
    assert_eq!(binding.session_error("a"), None);

    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn archive_committed_after_reconnect_ready_clears_error_releases_lease_and_never_reopens() {
    let endpoint = BlockerTestEndpoint::new();
    endpoint.supports_recovery.store(true, Ordering::Release);
    endpoint
        .defer_archive_mutation
        .store(true, Ordering::Release);
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.use_workspace_stream = true;
        guard.custom_items = Some(vec![
            json!({"sessionId": "a", "running": false, "blank": false, "updatedAt": 10, "projections": {"asOfSeq": 1, "values": {"title": "Idle A"}}}),
            json!({"sessionId": "b", "running": false, "blank": false, "updatedAt": 20, "projections": {"asOfSeq": 1, "values": {"title": "Idle B"}}}),
        ]);
    }
    let (rt, runtime, binding) = setup_binding(&endpoint);
    binding.open_initial_session("a").unwrap();
    rt.block_on(wait_store(&runtime, |s| s.sessions["a"].journal_ready));

    endpoint.park_archive.store(true, Ordering::Release);
    let entered = endpoint.park_archive_entered.notified();
    binding.archive_session("a");
    rt.block_on(entered);
    assert!(endpoint.inner.lock().unwrap().archived_ids.is_empty());

    let epoch_before = runtime.inspect(|s| s.connection_epoch);
    endpoint.drop_connection();
    endpoint.unblock_reconnect();

    // Conn 2 becomes Ready while Kernel still holds A active, so A legitimately opens and recovers on conn 2.
    rt.block_on(wait_store(&runtime, |s| {
        s.connection == tekes_agent_session::ConnectionState::Ready
            && s.connection_epoch > epoch_before
            && endpoint.connect_count.load(Ordering::Acquire) >= 2
            && s.sessions["a"].journal_ready
    }));
    wait_until("transient archive error published on conn 2", || {
        binding.session_error("a").is_some()
    });

    // Now the in-flight archive request commits and Kernel emits inventory remove(a) on conn 2.
    {
        let mut guard = endpoint.inner.lock().unwrap();
        if let Some(items) = guard.custom_items.as_mut() {
            items.retain(|item| item["sessionId"] != "a");
        }
        guard.archived_ids.push("a".to_owned());
        guard
            .inventory_tx
            .as_ref()
            .unwrap()
            .send(Ok(json!({
                "type": "remove",
                "generation": "g",
                "sessionId": "a"
            })))
            .unwrap();
    }
    endpoint.park_archive_release.notify_one();

    wait_until(
        "late archive confirmation clears error and retires journal without switch",
        || {
            binding.session_error("a").is_none()
                && binding.error_for_session(Some("a")).is_none()
                && !binding
                    .sidebar_rows()
                    .into_iter()
                    .any(|r| r.session_id == "a")
                && runtime.inspect(|s| {
                    !s.sessions["a"].journal_ready && s.sessions["a"].journal_error.is_none()
                })
        },
    );

    // Next reconnect must NOT reopen A or produce stream-open-failed.
    let conn2_rpcs_len = endpoint.rpcs().len();
    rt.block_on(runtime.reconnect()).unwrap();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection == tekes_agent_session::ConnectionState::Ready && s.connection_epoch >= 3
    }));

    let conn3_opens: Vec<_> = endpoint
        .journal_opens()
        .into_iter()
        .filter(|(c, _)| *c >= 3)
        .collect();
    assert!(
        conn3_opens.is_empty(),
        "unexpected conn3 journal opens: {conn3_opens:?}"
    );
    let conn3_recover_rpcs: Vec<_> = endpoint
        .rpcs()
        .into_iter()
        .skip(conn2_rpcs_len)
        .filter(|(m, _)| m == "sessions.recover")
        .collect();
    assert!(
        conn3_recover_rpcs.is_empty(),
        "unexpected conn3 sessions.recover: {conn3_recover_rpcs:?}"
    );
    assert_eq!(
        endpoint
            .rpcs()
            .iter()
            .filter(|(m, p)| m == "session.archive" && p["sessionId"] == "a")
            .count(),
        1
    );
    assert_eq!(binding.session_error("a"), None);

    rt.block_on(runtime.stop()).unwrap();
}
