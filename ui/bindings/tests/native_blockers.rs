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
}

struct EndpointState {
    rpcs: Vec<(String, Value)>,
    journal_txs: BTreeMap<String, mpsc::UnboundedSender<Result<Value>>>,
    journal_txs_by_conn: BTreeMap<(usize, String), mpsc::UnboundedSender<Result<Value>>>,
    journal_entries: BTreeMap<String, Vec<Value>>,
    journal_opens: Vec<(usize, String)>,
    journal_closes: Vec<(usize, String)>,
    inventory_tx: Option<mpsc::UnboundedSender<Result<Value>>>,
    running: BTreeMap<String, bool>,
    reject_cancel: bool,
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
                reject_cancel: false,
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
        Ok(Arc::new(TestSessionEndpoint {
            conn,
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
impl SessionEndpoint for TestSessionEndpoint {
    fn identity(&self) -> &str {
        "blocker-test"
    }
    fn host(&self) -> &HostDescription {
        &self.host
    }
    fn capabilities(&self) -> EndpointCapabilities {
        EndpointCapabilities::methods(&[
            "session.prompt",
            "session.cancel",
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
                let items = vec![
                    json!({"sessionId": "a", "running": guard.running["a"], "blank": false, "updatedAt": 10}),
                    json!({"sessionId": "b", "running": guard.running["b"], "blank": false, "updatedAt": 20}),
                    json!({"sessionId": "c", "running": guard.running["c"], "blank": false, "updatedAt": 30}),
                ];
                guard.inventory_tx = Some(tx);
                let baseline =
                    json!({"type": "baseline", "generation": "g", "snapshot": {"items": items}});
                let tail = stream::unfold(rx, |mut rx| async move {
                    rx.recv().await.map(|item| (item, rx))
                });
                Ok(Box::pin(stream::once(async { Ok(baseline) }).chain(tail)))
            }
            StreamTarget::SessionControl => {
                let baseline = json!({"type": "baseline", "generation": "g", "snapshot": {"queues": [], "jobs": [], "projections": []}});
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
                if self
                    .source
                    .cancel_disconnect_before_reply
                    .swap(false, Ordering::AcqRel)
                {
                    self.source.cancel_entered.notify_waiters();
                    self.source.drop_connection();
                    return Err(Error::Disconnected);
                }
                if self.source.inner.lock().unwrap().reject_cancel {
                    return Err(Error::Remote("cancel rejected".into()));
                }
                Ok(json!({ "cancelled": true }))
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
        drop(watch_guard);

        let b_res = opener.join().unwrap();
        selector.join().unwrap();
        let _ = b_res;
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
