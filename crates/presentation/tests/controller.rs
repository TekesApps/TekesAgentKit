use async_trait::async_trait;
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tekes_agent_contract::*;
use tekes_agent_presentation::*;
use tekes_agent_session::*;
use tokio::sync::Notify;
#[derive(Clone, Default)]
struct Connector {
    search: bool,
    sent: Arc<Notify>,
    release: Arc<Notify>,
    calls: Arc<AtomicUsize>,
    requests: Arc<std::sync::Mutex<Vec<(String, Value)>>>,
    reject_discard: Arc<AtomicBool>,
    hold_fork: Arc<AtomicBool>,
    bounded: Arc<AtomicBool>,
    hold_bounded: Arc<AtomicBool>,
    bounded_entered: Arc<Notify>,
    release_bounded: Arc<Notify>,
    pages: Arc<std::sync::Mutex<std::collections::VecDeque<Value>>>,
    catalog_owner: Arc<std::sync::Mutex<Option<String>>>,
}
struct Endpoint {
    connector: Connector,
    host: HostDescription,
}
#[async_trait]
impl EndpointConnector for Connector {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        let host = serde_json::from_value(
            json!({"protocolVersion":4,"product":{"name":"test","version":"0.9.0"},"capabilities":["session-journal","session-inventory-sync","session-control-sync","actionable-sync"],"cwd":"/work","home":"/home","attachedSessions":0,"canOpenPath":false}),
        )?;
        Ok(Arc::new(Endpoint {
            connector: self.clone(),
            host,
        }))
    }
}
#[async_trait]
impl SessionEndpoint for Endpoint {
    fn identity(&self) -> &str {
        "test"
    }
    fn host(&self) -> &HostDescription {
        &self.host
    }
    fn capabilities(&self) -> EndpointCapabilities {
        let mut caps = EndpointCapabilities::methods(&[
            "session.prompt",
            "session.create",
            "commands/run",
            "session.fork",
            "session.discard",
            "session.cancel",
            "session.rename",
            "subagents.list",
            "subagents.prompt",
            "subagents.cancel",
        ]);
        if self.connector.bounded.load(Ordering::SeqCst) {
            caps.methods.insert("subagents.history".into());
        }
        if self.connector.search {
            caps.methods.insert("session.search".into());
        }
        caps.supports_ephemeral_fork = true;
        caps
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        use futures_util::{StreamExt, stream};
        let value = match target {
            StreamTarget::SessionInventory => {
                json!({"type":"baseline","generation":"g","snapshot":{"items":[]}})
            }
            StreamTarget::SessionControl => {
                json!({"type":"baseline","generation":"g","snapshot":{"queues":[],"jobs":[],"projections":[]}})
            }
            StreamTarget::Actionables => json!({"type":"reset","generation":"g"}),
            StreamTarget::SessionJournal { address, .. } => {
                json!({"type":"snapshot","snapshot":{"sessionId":address.session_id,"generation":"g","throughSequence":-1,"windowLimit":50,"entries":[],"hasMoreBefore":false}})
            }
            _ => unreachable!(),
        };
        Ok(Box::pin(
            stream::once(async { Ok(value) }).chain(stream::pending()),
        ))
    }
    async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
        unreachable!()
    }
    async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
        Ok(())
    }
    async fn call(&self, method: &str, payload: Value, _: &str) -> Result<Value> {
        self.connector
            .requests
            .lock()
            .unwrap()
            .push((method.into(), payload.clone()));
        if method == "session.search" {
            return Ok(
                json!({"items":[{"sessionId":"a","snippet":payload["query"],"extra":"retained"}],"hasMore":true}),
            );
        }
        if method == "subagents.history" {
            if self.connector.hold_bounded.load(Ordering::SeqCst) {
                self.connector.bounded_entered.notify_one();
                self.connector.release_bounded.notified().await;
            }
            let page = self
                .connector
                .pages
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected bounded read");
            return if page["error"] == true {
                Err(Error::Remote(json!({"code":"history-stale"})))
            } else {
                Ok(page)
            };
        }
        if method == "subagents.list" {
            let owner = self
                .connector
                .catalog_owner
                .lock()
                .unwrap()
                .clone()
                .unwrap_or("a".into());
            return Ok(
                json!({"parentAvailable":payload["parentSessionId"] == owner,"entries":[{"kind":"child","id":"child","mode":"continuable"},{"kind":"child","id":"bounded","mode":"one-shot"}]}),
            );
        }
        if method == "session.fork" {
            if self.connector.hold_fork.load(Ordering::SeqCst) {
                self.connector.sent.notify_one();
                self.connector.release.notified().await;
            }
            return Ok(json!({"sessionId":"scratch"}));
        }
        if method == "session.discard" && self.connector.reject_discard.load(Ordering::SeqCst) {
            return Err(Error::Remote(json!({"code":"still-running"})));
        }
        if method == "session.prompt" {
            self.connector.calls.fetch_add(1, Ordering::SeqCst);
            self.connector.sent.notify_one();
            self.connector.release.notified().await;
        }
        Ok(json!({"accepted":true}))
    }
    async fn close(&self) {}
}
fn destination(id: &str) -> ComposerDestination {
    ComposerDestination::run(RunScope::main("test", id))
}
async fn setup() -> (Connector, Arc<SessionRuntime>, Arc<RunController>) {
    let connector = Connector::default();
    let runtime = Arc::new(SessionRuntime::spawn("test", Arc::new(connector.clone())));
    runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
    let controller = Arc::new(RunController::new(runtime.clone()).unwrap());
    controller.open(RunScope::main("test", "a")).await.unwrap();
    (connector, runtime, controller)
}
#[tokio::test]
async fn admission_is_captured_even_when_selection_changes_while_awaiting_transport() {
    let (connector, runtime, controller) = setup().await;
    controller
        .edit(
            &destination("a"),
            ComposerDraft {
                text: "input A".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let ticket = controller
        .prepare(&destination("a"), PromptMode::Queue)
        .unwrap();
    let submitting = controller.clone();
    let task = tokio::spawn(async move { submitting.dispatch(ticket, None).await });
    connector.sent.notified().await;
    controller.select(destination("b")).unwrap();
    controller
        .edit(
            &destination("b"),
            ComposerDraft {
                text: "new B".into(),
                ..Default::default()
            },
        )
        .unwrap();
    connector.release.notify_one();
    task.await.unwrap().unwrap();
    assert_eq!(controller.snapshot().draft.text, "new B");
    assert!(controller.snapshot().submissions.is_empty());
    assert!(controller.snapshot().transcript.items.is_empty());
    controller.select(destination("a")).unwrap();
    assert_eq!(
        controller.snapshot().submissions[0].phase,
        SubmissionPhase::Admitted
    );
    assert_eq!(connector.calls.load(Ordering::SeqCst), 1);
    drop(controller);
    assert_eq!(runtime.snapshot().connection, ConnectionState::Ready);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn cancelled_dispatched_task_becomes_unknown_and_cannot_automatically_resubmit() {
    let (connector, runtime, controller) = setup().await;
    controller
        .edit(
            &destination("a"),
            ComposerDraft {
                text: "input".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let ticket = controller
        .prepare(&destination("a"), PromptMode::Queue)
        .unwrap();
    let id = ticket.id().to_owned();
    let submitting = controller.clone();
    let task = tokio::spawn(async move { submitting.dispatch(ticket, None).await });
    connector.sent.notified().await;
    task.abort();
    assert!(task.await.is_err());
    assert_eq!(
        controller.snapshot().submissions[0].phase,
        SubmissionPhase::Unknown
    );
    assert_eq!(controller.snapshot().draft.text, "input");
    assert!(!controller.snapshot().can_submit);
    assert!(
        controller
            .prepare(&destination("a"), PromptMode::Queue)
            .is_err()
    );
    assert_eq!(connector.calls.load(Ordering::SeqCst), 1);
    controller.acknowledge_reconciliation(&id).unwrap();
    let next = controller
        .prepare(&destination("a"), PromptMode::Queue)
        .unwrap();
    controller.cancel_prepared(next).unwrap();
    runtime.stop().await.unwrap();
}
struct RejectPreparation;
#[async_trait]
impl SubmissionPreparation for RejectPreparation {
    async fn prepare(
        &self,
        _: &SessionRuntime,
        _: &PreparedSubmission,
        _: &RunScope,
    ) -> Result<()> {
        Err(unsupported("host preparation rejected"))
    }
}
#[tokio::test]
async fn pre_dispatch_host_failure_restores_draft_without_transport_or_unknown_lock() {
    let (connector, runtime, controller) = setup().await;
    controller
        .edit(
            &destination("a"),
            ComposerDraft {
                text: "input".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let ticket = controller
        .prepare(&destination("a"), PromptMode::Queue)
        .unwrap();
    assert!(
        controller
            .dispatch_with(ticket, None, &RejectPreparation)
            .await
            .is_err()
    );
    assert_eq!(
        controller.snapshot().submissions[0].phase,
        SubmissionPhase::Failed
    );
    assert_eq!(controller.snapshot().draft.text, "input");
    assert!(controller.snapshot().can_submit);
    assert_eq!(connector.calls.load(Ordering::SeqCst), 0);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn child_control_checks_fresh_parent_ownership_and_never_routes_to_session_prompt() {
    let (connector, runtime, _) = setup().await;
    assert!(
        runtime
            .prompt_child("a", "bounded", PromptMode::Queue, vec![], "one")
            .await
            .is_err()
    );
    assert!(runtime.cancel_child("a", "a", "self").await.is_err());
    assert!(
        runtime
            .cancel_child("other", "child", "lost-parent")
            .await
            .is_err()
    );
    assert!(
        runtime
            .prompt_child(
                "a",
                "child",
                PromptMode::Queue,
                vec![json!({"type":"file"})],
                "file"
            )
            .await
            .is_err()
    );
    runtime
        .prompt_child(
            "a",
            "child",
            PromptMode::Queue,
            vec![json!({"type":"text","text":"hello"})],
            "good",
        )
        .await
        .unwrap();
    runtime.cancel_child("a", "child", "cancel").await.unwrap();
    let requests = connector.requests.lock().unwrap().clone();
    assert!(
        !requests
            .iter()
            .any(|(m, _)| m == "session.prompt" || m == "session.cancel")
    );
    assert_eq!(
        requests
            .iter()
            .filter(|(m, _)| m == "subagents.prompt")
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .filter(|(m, _)| m == "subagents.cancel")
            .count(),
        1
    );
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn history_lease_release_preserves_other_consumers_and_legacy_owner() {
    let (_, runtime, _) = setup().await;
    let first = runtime.acquire_history("child").await.unwrap();
    let second = runtime.acquire_history("child").await.unwrap();
    first.release().await.unwrap();
    assert!(runtime.inspect(|s| s.sessions["child"].journal_ready));
    second.release().await.unwrap();
    assert!(!runtime.inspect(|s| s.sessions["child"].journal_ready));
    let lease = runtime.acquire_history("a").await.unwrap();
    runtime.close_session("a").await.unwrap();
    assert!(runtime.inspect(|s| s.sessions["a"].journal_ready));
    lease.release().await.unwrap();
    assert!(!runtime.inspect(|s| s.sessions["a"].journal_ready));
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn side_chat_failed_discard_and_failed_query_preserve_pointer_until_confirmed_absence() {
    let (connector, runtime, _) = setup().await;
    let side = SideChatController::new(runtime.clone());
    let chat = side
        .create(RunScope::main("test", "a"), Some("Parent"), "fork")
        .await
        .unwrap();
    assert_eq!(chat.title, "SideChat · Parent");
    let requests = connector.requests.lock().unwrap().clone();
    assert!(
        requests
            .iter()
            .any(|(m, v)| m == "session.fork" && v["ephemeral"] == true && v["sessionId"] == "a")
    );
    connector.reject_discard.store(true, Ordering::SeqCst);
    assert!(side.close("scratch", "discard").await.is_err());
    let failed = side.snapshot().chats["scratch"].clone();
    assert_eq!(failed.phase, SideChatPhase::Ready);
    assert!(!side.reconcile(&failed, Err("query failed".into())));
    assert!(side.snapshot().chats.contains_key("scratch"));
    assert!(side.reconcile(&failed, Ok(false)));
    assert!(side.snapshot().chats.is_empty());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn cancelled_fork_is_unknown_blocks_duplicate_and_retains_originating_parent() {
    let (connector, runtime, _) = setup().await;
    connector.hold_fork.store(true, Ordering::SeqCst);
    let side = Arc::new(SideChatController::new(runtime.clone()));
    let mut changes = side.subscribe();
    assert!(!changes.has_changed().unwrap());
    let worker = side.clone();
    let task = tokio::spawn(async move {
        worker
            .create(RunScope::main("test", "a"), None, "fork")
            .await
    });
    connector.sent.notified().await;
    changes.changed().await.unwrap();
    assert!(
        changes
            .borrow_and_update()
            .is_creating_for(&RunScope::main("test", "a"))
    );
    task.abort();
    let _ = task.await;
    changes.changed().await.unwrap();
    assert!(
        changes
            .borrow_and_update()
            .unknown_creation_for(&RunScope::main("test", "a"))
            .is_some()
    );
    assert_eq!(side.snapshot().unknown_creations.len(), 1);
    assert!(
        side.create(RunScope::main("test", "a"), None, "duplicate")
            .await
            .is_err()
    );
    connector.release.notify_one();
    side.reconcile_creation(&RunScope::main("test", "a"), None)
        .unwrap();
    assert!(side.snapshot().unknown_creations.is_empty());
    changes.changed().await.unwrap();
    assert!(changes.borrow_and_update().unknown_creations.is_empty());
    // Reading and rejected no-op reconciliation must not cause a UI wake loop.
    let _ = side.snapshot();
    assert!(
        side.reconcile_creation(&RunScope::main("test", "a"), None)
            .is_err()
    );
    assert!(!changes.has_changed().unwrap());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn nested_child_controller_uses_direct_owner_and_rejects_other_destinations() {
    let (connector, runtime, _) = setup().await;
    *connector.catalog_owner.lock().unwrap() = Some("nested".into());
    let parent = RunScope {
        endpoint_id: "test".into(),
        thread_id: "a".into(),
        session_id: "nested".into(),
    };
    let scope = RunScope {
        session_id: "child".into(),
        ..parent.clone()
    };
    let target = ComposerDestination::run(scope.clone());
    let _lease = runtime.acquire_history("child").await.unwrap();
    let controller = RunController::for_child(runtime.clone(), &parent, "child").unwrap();
    assert!(controller.select(destination("a")).is_err());
    assert!(
        controller
            .edit_text(&destination("a"), "wrong".into())
            .is_err()
    );
    assert!(
        controller
            .edit(&destination("a"), ComposerDraft::default())
            .is_err()
    );
    assert!(
        controller
            .prepare(&destination("a"), PromptMode::Queue)
            .is_err()
    );
    controller.refresh_child_catalog(&scope).await.unwrap();
    controller
        .edit_text(&target, "nested child input".into())
        .unwrap();
    let ticket = controller.prepare(&target, PromptMode::Queue).unwrap();
    controller.dispatch(ticket, None).await.unwrap();
    let requests = connector.requests.lock().unwrap().clone();
    let prompts: Vec<_> = requests
        .iter()
        .filter(|(m, _)| m == "subagents.prompt")
        .collect();
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0].1["parentSessionId"], "nested");
    assert!(
        requests
            .iter()
            .filter(|(m, _)| m == "subagents.list")
            .all(|(_, p)| p["parentSessionId"] == "nested")
    );
    assert!(!requests.iter().any(|(m, _)| m == "session.prompt"));
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn bounded_history_requires_child_projection_and_independent_history_has_own_pointer() {
    let (_, runtime, _) = setup().await;
    assert!(
        ChildRun::open(
            runtime.clone(),
            RunScope::main("test", "a"),
            "bounded",
            ChildHistoryLayout::SameSession,
            None
        )
        .await
        .is_err()
    );
    let child = ChildRun::open(
        runtime.clone(),
        RunScope::main("test", "a"),
        "child",
        ChildHistoryLayout::IndependentSessions,
        None,
    )
    .await
    .unwrap();
    assert!(!child.transcript().await.unwrap().is_stale);
    drop(child);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn bounded_child_pages_preserve_verified_prefix_and_reset_changed_history() {
    fn page(cut: i64, rev: &str, entries: Vec<Value>, more: bool) -> Value {
        json!({"parentSessionId":"a","childSessionId":"bounded","throughSequence":cut,
            "historyRevision":rev.repeat(64),"entries":entries,"hasMoreBefore":more,"runOpen":false,
            "sourceCall":{"callId":"task","name":"task","arguments":{"task_name":"audit"},"phase":"done"}})
    }
    fn message(seq: i64, text: &str) -> Value {
        json!({"event":{"seq":seq,"time":100,"type":"assistant/message","data":{
            "turn":1,"step":seq+1,"message":{"id":format!("m{seq}"),"content":[{"type":"text","text":text}]}}}})
    }
    let (connector, runtime, _) = setup().await;
    connector.bounded.store(true, Ordering::SeqCst);
    let head = page(5, "a", vec![message(5, "new")], true);
    connector.pages.lock().unwrap().extend([
        head.clone(),
        head.clone(),
        page(5, "a", vec![message(1, "old")], false),
        page(8, "b", vec![message(6, "newest")], true),
        head,
        page(8, "c", vec![message(8, "rewritten")], false),
        page(20, "d", vec![message(20, "far ahead")], true),
        page(8, "c", vec![message(8, "rewritten")], false),
    ]);
    let child = ChildRun::open(
        runtime.clone(),
        RunScope::main("test", "a"),
        "bounded",
        ChildHistoryLayout::SameSession,
        None,
    )
    .await
    .unwrap();
    assert!(child.transcript().await.unwrap().has_more_before);
    assert!(child.load_earlier().await.unwrap());
    let grown = child.transcript().await.unwrap();
    assert_eq!(
        grown
            .items
            .iter()
            .map(|i| i.text.as_str())
            .collect::<Vec<_>>(),
        ["old", "new", "newest"]
    );
    assert!(!grown.has_more_before);
    assert_eq!(grown.tools[0].arguments["task_name"], "audit");
    let replaced = child.transcript().await.unwrap();
    assert_eq!(replaced.items.len(), 1);
    assert_eq!(replaced.items[0].text, "rewritten");
    assert!(!child.load_earlier().await.unwrap());
    let gap = child.transcript().await.unwrap();
    assert_eq!(gap.items.len(), 1);
    assert_eq!(gap.items[0].text, "far ahead");
    assert!(gap.has_more_before);
    let requests = connector.requests.lock().unwrap().clone();
    let earlier = &requests
        .iter()
        .find(|(m, p)| m == "subagents.history" && p["beforeSequence"] == 5)
        .unwrap()
        .1;
    assert_eq!(earlier["throughSequence"], 5);
    assert_eq!(earlier["historyRevision"], "a".repeat(64));
    // Child bounded reads never mount a child journal or consume its parent's transcript.
    assert!(!runtime.inspect(|s| s.sessions.contains_key("bounded")));
    drop(child);
    runtime.stop().await.unwrap();
}

fn bounded_source_page(phase: &str) -> Value {
    json!({
        "parentSessionId":"a",
        "childSessionId":"bounded",
        "throughSequence":5,
        "historyRevision":"e".repeat(64),
        "entries":[{
            "event":{"seq":5,"time":100,"type":"assistant/message","data":{
                "turn":1,"step":6,"message":{"id":"history-entry","content":[{"type":"text","text":"independent history"}]}
            }}
        }],
        "hasMoreBefore":false,
        "runOpen":false,
        "sourceCall":{"callId":"task-source-7","name":"task","arguments":{"task_name":"audit","mode":"bounded"},"phase":phase}
    })
}

#[tokio::test]
async fn bounded_source_call_phases_and_identity_are_projected_through_public_child_run() {
    let (connector, runtime, _) = setup().await;
    connector.bounded.store(true, Ordering::SeqCst);

    let cases = [
        ("error", "error"),
        ("stopped", "aborted"),
        ("running", "running"),
        ("done", "completed"),
        ("future-phase", "completed"),
    ];
    let mut pages = Vec::new();
    for (phase, _) in cases {
        // ChildRun::open reads the initial page; transcript() then refreshes through
        // the same public RPC path while the revision and history remain unchanged.
        pages.extend([bounded_source_page(phase), bounded_source_page(phase)]);
    }
    connector.pages.lock().unwrap().extend(pages);

    for (phase, expected_status) in cases {
        let child = ChildRun::open(
            runtime.clone(),
            RunScope::main("test", "a"),
            "bounded",
            ChildHistoryLayout::SameSession,
            None,
        )
        .await
        .unwrap();
        let transcript = child.transcript().await.unwrap();

        let expected_turn_id =
            json!(["test", "bounded", "task-source", "task-source-7"]).to_string();
        assert_eq!(transcript.turns[0].id, expected_turn_id);
        assert_eq!(transcript.turns[0].status, expected_status, "phase {phase}");
        assert_eq!(transcript.tools[0].turn_id, transcript.turns[0].id);
        assert_eq!(
            transcript.tools[0].id,
            format!("{}:call", transcript.turns[0].id)
        );
        assert_eq!(transcript.tools[0].name, "task");
        assert_eq!(
            transcript.tools[0].arguments,
            json!({"task_name":"audit","mode":"bounded"})
        );
        assert_eq!(transcript.tools[0].phase, phase);
        assert_eq!(transcript.items.len(), 1);
        assert_eq!(transcript.items[0].text, "independent history");
    }

    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn bounded_source_call_running_to_terminal_keeps_identity_and_history_entries() {
    let (connector, runtime, _) = setup().await;
    connector.bounded.store(true, Ordering::SeqCst);

    for (terminal_phase, expected_status) in [("error", "error"), ("stopped", "aborted")] {
        connector.pages.lock().unwrap().extend([
            bounded_source_page("running"),
            bounded_source_page("running"),
            bounded_source_page(terminal_phase),
        ]);
        let child = ChildRun::open(
            runtime.clone(),
            RunScope::main("test", "a"),
            "bounded",
            ChildHistoryLayout::SameSession,
            None,
        )
        .await
        .unwrap();

        let running = child.transcript().await.unwrap();
        assert_eq!(running.turns[0].status, "running");
        let running_turn_id = running.turns[0].id.clone();
        let running_tool_id = running.tools[0].id.clone();
        let running_items = running.items.clone();

        let terminal = child.transcript().await.unwrap();
        assert_eq!(terminal.turns[0].id, running_turn_id);
        assert_eq!(
            terminal.turns[0].status, expected_status,
            "phase {terminal_phase}"
        );
        assert_eq!(terminal.tools[0].id, running_tool_id);
        assert_eq!(terminal.tools[0].turn_id, running_turn_id);
        assert_eq!(terminal.tools[0].phase, terminal_phase);
        assert_eq!(terminal.tools[0].name, "task");
        assert_eq!(
            terminal.tools[0].arguments,
            json!({"task_name":"audit","mode":"bounded"})
        );
        assert_eq!(terminal.items, running_items);
        assert_eq!(terminal.items[0].text, "independent history");
    }

    runtime.stop().await.unwrap();
}

#[cfg(feature = "acceptance-source-witness")]
#[tokio::test]
async fn bounded_head_witness_pairs_one_projection_with_the_exact_raw_response() {
    let (connector, runtime, _) = setup().await;
    connector.bounded.store(true, Ordering::SeqCst);
    for phase in ["error", "stopped"] {
        connector
            .pages
            .lock()
            .unwrap()
            .extend([bounded_source_page(phase), bounded_source_page(phase)]);
        let child = ChildRun::open(
            runtime.clone(),
            RunScope::main("test", "a"),
            "bounded",
            ChildHistoryLayout::SameSession,
            None,
        )
        .await
        .unwrap();
        let before = connector
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|(method, _)| method == "subagents.history")
            .count();

        let (transcript, witness) = child
            .transcript_with_bounded_source_witness()
            .await
            .unwrap();
        let after = connector
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|(method, _)| method == "subagents.history")
            .count();
        let wire_head_request = connector
            .requests
            .lock()
            .unwrap()
            .iter()
            .rfind(|(method, _)| method == "subagents.history")
            .unwrap()
            .1
            .clone();

        assert_eq!(after, before + 1, "witness must not perform a second read");
        assert!(connector.pages.lock().unwrap().is_empty());
        assert_eq!(witness.parent_scope, RunScope::main("test", "a"));
        assert_eq!(witness.child_id, "bounded");
        assert_eq!(witness.page_size, 50);
        assert_eq!(witness.head_request.parent_session_id, "a");
        assert_eq!(witness.head_request.child_session_id, "bounded");
        assert_eq!(witness.head_request.through_sequence, None);
        assert_eq!(witness.head_request.before_sequence, None);
        assert_eq!(
            wire_head_request,
            serde_json::to_value(&witness.head_request).unwrap()
        );
        assert_eq!(
            witness.raw_head.source_call.as_ref().unwrap()["phase"],
            phase
        );
        assert_eq!(
            witness.evidence_scope,
            "built-in-bounded-head-only-same-local-read"
        );
        assert!(!witness.server_transactional_freshness_proven);
        assert_eq!(transcript.items.len(), 1);
        assert_eq!(transcript.items[0].text, "independent history");
        assert_eq!(
            transcript.turns[0].status,
            if phase == "error" { "error" } else { "aborted" }
        );
    }
    runtime.stop().await.unwrap();
}

#[cfg(feature = "acceptance-source-witness")]
#[tokio::test]
async fn bounded_head_witness_rejects_other_owners_without_falling_back() {
    struct CustomSource;
    #[async_trait]
    impl ChildHistorySource for CustomSource {
        async fn read(&self, _: &RunScope, _: &str) -> Result<TranscriptSnapshot> {
            Ok(TranscriptSnapshot::default())
        }
    }

    let (connector, runtime, _) = setup().await;
    connector.bounded.store(true, Ordering::SeqCst);
    let custom = ChildRun::open(
        runtime.clone(),
        RunScope::main("test", "a"),
        "bounded",
        ChildHistoryLayout::SameSession,
        Some(Arc::new(CustomSource)),
    )
    .await
    .unwrap();
    assert!(
        custom
            .transcript_with_bounded_source_witness()
            .await
            .is_err()
    );

    let independent = ChildRun::open(
        runtime.clone(),
        RunScope::main("test", "a"),
        "child",
        ChildHistoryLayout::IndependentSessions,
        None,
    )
    .await
    .unwrap();
    assert!(
        independent
            .transcript_with_bounded_source_witness()
            .await
            .is_err()
    );
    assert!(connector.pages.lock().unwrap().is_empty());
    assert!(
        !connector
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|(method, _)| method == "subagents.history")
    );
    runtime.stop().await.unwrap();
}

#[cfg(feature = "acceptance-source-witness")]
#[tokio::test]
async fn bounded_head_witness_rejects_aggregated_pages_and_retains_the_projection() {
    fn page(entries: Vec<Value>, more: bool) -> Value {
        json!({"parentSessionId":"a","childSessionId":"bounded","throughSequence":5,
            "historyRevision":"f".repeat(64),"entries":entries,"hasMoreBefore":more,
            "runOpen":false,"sourceCall":null})
    }
    fn message(seq: i64, text: &str) -> Value {
        json!({"event":{"seq":seq,"time":100,"type":"assistant/message","data":{
            "turn":1,"step":seq+1,"message":{"id":format!("m{seq}"),"content":[{"type":"text","text":text}]}}}})
    }

    let (connector, runtime, _) = setup().await;
    connector.bounded.store(true, Ordering::SeqCst);
    let head = page(vec![message(5, "new")], false);
    connector.pages.lock().unwrap().extend([
        page(vec![message(5, "new")], true),
        page(vec![message(1, "old")], true),
        head.clone(),
        head,
    ]);
    let child = ChildRun::open(
        runtime.clone(),
        RunScope::main("test", "a"),
        "bounded",
        ChildHistoryLayout::SameSession,
        None,
    )
    .await
    .unwrap();
    assert!(child.load_earlier().await.unwrap());
    assert!(
        child
            .transcript_with_bounded_source_witness()
            .await
            .is_err()
    );

    // A normal read still sees the untouched cumulative cache and its aggregated pagination bit.
    let retained = child.transcript().await.unwrap();
    assert_eq!(
        retained
            .items
            .iter()
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        ["old", "new"]
    );
    assert!(retained.has_more_before);
    assert!(connector.pages.lock().unwrap().is_empty());
    runtime.stop().await.unwrap();
}

#[cfg(feature = "acceptance-source-witness")]
#[tokio::test]
async fn bounded_head_witness_rejects_a_natural_increment_that_retains_old_entries() {
    fn page(cut: i64, revision: &str, seq: i64, text: &str) -> Value {
        json!({"parentSessionId":"a","childSessionId":"bounded","throughSequence":cut,
            "historyRevision":revision.repeat(64),"entries":[{"event":{"seq":seq,"time":100,
                "type":"assistant/message","data":{"turn":1,"step":seq+1,"message":{
                    "id":format!("m{seq}"),"content":[{"type":"text","text":text}]}}}}],
            "hasMoreBefore":false,"runOpen":false,"sourceCall":null})
    }

    let (connector, runtime, _) = setup().await;
    connector.bounded.store(true, Ordering::SeqCst);
    let old_head = page(5, "a", 5, "old head");
    let new_head = page(6, "b", 6, "new head");
    let verified_prefix = page(5, "a", 5, "old head");
    connector.pages.lock().unwrap().extend([
        old_head,
        new_head.clone(),
        verified_prefix.clone(),
        new_head,
        verified_prefix,
    ]);
    let child = ChildRun::open(
        runtime.clone(),
        RunScope::main("test", "a"),
        "bounded",
        ChildHistoryLayout::SameSession,
        None,
    )
    .await
    .unwrap();
    assert!(
        child
            .transcript_with_bounded_source_witness()
            .await
            .is_err()
    );

    let retained = child.transcript().await.unwrap();
    assert_eq!(
        retained
            .items
            .iter()
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        ["old head", "new head"]
    );
    assert!(connector.pages.lock().unwrap().is_empty());
    runtime.stop().await.unwrap();
}

#[cfg(feature = "acceptance-source-witness")]
#[tokio::test]
async fn bounded_head_witness_propagates_read_failure_without_returning_stale_proof() {
    let (connector, runtime, _) = setup().await;
    connector.bounded.store(true, Ordering::SeqCst);
    connector
        .pages
        .lock()
        .unwrap()
        .extend([bounded_source_page("running"), json!({"error":true})]);
    let child = ChildRun::open(
        runtime.clone(),
        RunScope::main("test", "a"),
        "bounded",
        ChildHistoryLayout::SameSession,
        None,
    )
    .await
    .unwrap();
    assert!(
        child
            .transcript_with_bounded_source_witness()
            .await
            .is_err()
    );
    assert!(connector.pages.lock().unwrap().is_empty());
    runtime.stop().await.unwrap();
}

#[cfg(feature = "acceptance-source-witness")]
#[tokio::test]
async fn bounded_head_witness_rejects_a_response_crossing_connection_epochs() {
    let (connector, runtime, _) = setup().await;
    connector.bounded.store(true, Ordering::SeqCst);
    connector
        .pages
        .lock()
        .unwrap()
        .push_back(bounded_source_page("running"));
    let child = Arc::new(
        ChildRun::open(
            runtime.clone(),
            RunScope::main("test", "a"),
            "bounded",
            ChildHistoryLayout::SameSession,
            None,
        )
        .await
        .unwrap(),
    );
    connector.hold_bounded.store(true, Ordering::SeqCst);
    let reading = {
        let child = child.clone();
        tokio::spawn(async move { child.transcript_with_bounded_source_witness().await })
    };
    connector.bounded_entered.notified().await;

    runtime.reconnect().await.unwrap();
    connector.release_bounded.notify_one();
    assert!(reading.await.unwrap().is_err());
    runtime.stop().await.unwrap();
}

#[cfg(feature = "acceptance-source-witness")]
#[tokio::test]
async fn bounded_paged_witness_records_earlier_pages_without_extra_reads_and_head_only_stays_strict()
 {
    fn page(entries: Vec<Value>, more: bool) -> Value {
        json!({"parentSessionId":"a","childSessionId":"bounded","throughSequence":5,
            "historyRevision":"f".repeat(64),"entries":entries,"hasMoreBefore":more,
            "runOpen":false,"sourceCall":null})
    }
    fn message(seq: i64) -> Value {
        json!({"event":{"seq":seq,"time":100+seq,"type":"assistant/message","data":{
            "turn":1,"step":seq+1,"message":{"id":format!("m{seq}"),"content":[
                {"type":"text","text":format!("line {seq}")}]}}}})
    }

    let (connector, runtime, _) = setup().await;
    connector.bounded.store(true, Ordering::SeqCst);
    let head = page(vec![message(5)], true);
    connector.pages.lock().unwrap().extend([
        page(vec![message(5)], true),
        head.clone(),
        page(vec![message(3), message(4)], true),
        head.clone(),
        head,
    ]);
    let child = ChildRun::open_with_page_size(
        runtime.clone(),
        RunScope::main("test", "a"),
        "bounded",
        ChildHistoryLayout::SameSession,
        None,
        3,
    )
    .await
    .unwrap();

    let before_refresh = connector
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|(method, _)| method == "subagents.history")
        .count();
    let (_, head_only_chain) = child
        .transcript_with_bounded_paged_source_witness()
        .await
        .unwrap();
    let after_refresh = connector
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|(method, _)| method == "subagents.history")
        .count();
    assert_eq!(after_refresh, before_refresh + 1);
    assert_eq!(head_only_chain.receipts.len(), 2);

    assert!(child.load_earlier().await.unwrap());
    let before_paged_refresh = connector
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|(method, _)| method == "subagents.history")
        .count();
    let (transcript, witness) = child
        .transcript_with_bounded_paged_source_witness()
        .await
        .unwrap();
    let after_paged_refresh = connector
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|(method, _)| method == "subagents.history")
        .count();
    assert_eq!(after_paged_refresh, before_paged_refresh + 1);
    assert_eq!(
        witness
            .receipts
            .iter()
            .map(|receipt| receipt.operation.clone())
            .collect::<Vec<_>>(),
        [
            BoundedChildPagedSourceOperation::InitialHead,
            BoundedChildPagedSourceOperation::RefreshHead,
            BoundedChildPagedSourceOperation::Earlier,
            BoundedChildPagedSourceOperation::RefreshHead,
        ]
    );
    assert_eq!(witness.projected_entries.len(), 3);
    assert_eq!(
        transcript
            .items
            .iter()
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        ["line 3", "line 4", "line 5"]
    );
    assert!(
        child
            .transcript_with_bounded_source_witness()
            .await
            .is_err(),
        "the existing head-only API must continue rejecting accumulated older rows"
    );
    assert!(connector.pages.lock().unwrap().is_empty());
    runtime.stop().await.unwrap();
}

#[cfg(feature = "acceptance-source-witness")]
#[tokio::test]
async fn bounded_paged_witness_keeps_prefix_validation_out_of_the_merged_entries() {
    fn page(cut: i64, revision: &str, entries: Vec<Value>, more: bool) -> Value {
        json!({"parentSessionId":"a","childSessionId":"bounded","throughSequence":cut,
            "historyRevision":revision.repeat(64),"entries":entries,"hasMoreBefore":more,
            "runOpen":false,"sourceCall":null})
    }
    fn message(seq: i64) -> Value {
        json!({"event":{"seq":seq,"time":100+seq,"type":"assistant/message","data":{
            "turn":1,"step":seq+1,"message":{"id":format!("m{seq}"),"content":[
                {"type":"text","text":format!("line {seq}")}]}}}})
    }
    let (connector, runtime, _) = setup().await;
    connector.bounded.store(true, Ordering::SeqCst);
    connector.pages.lock().unwrap().extend([
        page(5, "a", vec![message(5)], true),
        page(5, "a", vec![message(5)], true),
        page(8, "b", vec![message(6), message(7), message(8)], false),
        page(5, "a", vec![message(4), message(5)], true),
    ]);
    let child = ChildRun::open(
        runtime.clone(),
        RunScope::main("test", "a"),
        "bounded",
        ChildHistoryLayout::SameSession,
        None,
    )
    .await
    .unwrap();
    child.transcript().await.unwrap();
    let (transcript, witness) = child
        .transcript_with_bounded_paged_source_witness()
        .await
        .unwrap();
    assert_eq!(witness.receipts.len(), 4);
    assert_eq!(
        witness.receipts[2].operation,
        BoundedChildPagedSourceOperation::RefreshHead
    );
    assert_eq!(
        witness.receipts[3].operation,
        BoundedChildPagedSourceOperation::PrefixValidation
    );
    assert!(!witness.receipts[3].merged);
    assert_eq!(witness.projected_entries.len(), 4);
    assert_eq!(
        witness
            .projected_entries
            .iter()
            .map(|entry| entry.event.seq)
            .collect::<Vec<_>>(),
        [5, 6, 7, 8]
    );
    assert_eq!(transcript.items.len(), 4);
    assert!(connector.pages.lock().unwrap().is_empty());
    runtime.stop().await.unwrap();
}

#[cfg(feature = "acceptance-source-witness")]
#[tokio::test]
async fn bounded_paged_witness_resets_to_a_real_head_after_stale_prefix_or_gap() {
    fn page(cut: i64, revision: &str, seq: i64) -> Value {
        json!({"parentSessionId":"a","childSessionId":"bounded","throughSequence":cut,
            "historyRevision":revision.repeat(64),"entries":[{"event":{"seq":seq,"time":100,
                "type":"assistant/message","data":{"turn":1,"step":seq+1,"message":{
                    "id":format!("m{seq}"),"content":[{"type":"text","text":format!("line {seq}")}]}}}}],
            "hasMoreBefore":true,"runOpen":false,"sourceCall":null})
    }
    for (validation, expected_prefix_operation) in [
        (
            json!({"error":true}),
            BoundedChildPagedSourceOperation::PrefixValidation,
        ),
        (
            page(5, "a", 1),
            BoundedChildPagedSourceOperation::PrefixValidation,
        ),
    ] {
        let (connector, runtime, _) = setup().await;
        connector.bounded.store(true, Ordering::SeqCst);
        connector
            .pages
            .lock()
            .unwrap()
            .extend([page(5, "a", 5), page(20, "b", 20), validation]);
        let child = ChildRun::open(
            runtime.clone(),
            RunScope::main("test", "a"),
            "bounded",
            ChildHistoryLayout::SameSession,
            None,
        )
        .await
        .unwrap();
        let (transcript, witness) = child
            .transcript_with_bounded_paged_source_witness()
            .await
            .unwrap();
        assert_eq!(
            witness.receipts[0].operation,
            BoundedChildPagedSourceOperation::RefreshHead
        );
        assert!(witness.receipts[0].reset);
        assert_eq!(witness.receipts[1].operation, expected_prefix_operation);
        assert_eq!(witness.projected_entries.len(), 1);
        assert_eq!(witness.projected_entries[0].event.seq, 20);
        assert_eq!(transcript.items.len(), 1);
        assert_eq!(transcript.items[0].text, "line 20");
        runtime.stop().await.unwrap();
    }
}

#[cfg(feature = "acceptance-source-witness")]
#[tokio::test]
async fn bounded_paged_witness_rejects_a_refresh_that_crosses_epochs_but_retains_cached_page_epochs()
 {
    let (connector, runtime, _) = setup().await;
    connector.bounded.store(true, Ordering::SeqCst);
    connector.pages.lock().unwrap().extend([
        bounded_source_page("running"),
        bounded_source_page("running"),
    ]);
    let child = Arc::new(
        ChildRun::open(
            runtime.clone(),
            RunScope::main("test", "a"),
            "bounded",
            ChildHistoryLayout::SameSession,
            None,
        )
        .await
        .unwrap(),
    );
    connector.hold_bounded.store(true, Ordering::SeqCst);
    let read = {
        let child = child.clone();
        tokio::spawn(async move { child.transcript_with_bounded_paged_source_witness().await })
    };
    connector.bounded_entered.notified().await;
    runtime.reconnect().await.unwrap();
    connector.release_bounded.notify_one();
    assert!(read.await.unwrap().is_err());

    connector.hold_bounded.store(false, Ordering::SeqCst);
    connector
        .pages
        .lock()
        .unwrap()
        .push_back(bounded_source_page("running"));
    let (_, witness) = child
        .transcript_with_bounded_paged_source_witness()
        .await
        .unwrap();
    assert_eq!(witness.receipts.len(), 2);
    assert_ne!(
        witness.receipts[0].read_epoch,
        witness.receipts[1].read_epoch
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn attachment_context_commit_checks_epoch_with_authority_and_preserves_typing() {
    let (_, runtime, controller) = setup().await;
    let baseline = ComposerContextBaseline::read(&controller.snapshot()).unwrap();
    let epoch = runtime.inspect(|s| s.connection_epoch);
    runtime.reconnect().await.unwrap();
    runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
    let artifact = ComposerArtifact::Attachment {
        id: "file".into(),
        name: "file.txt".into(),
        content: json!({"type":"file","receiptId":"r"}),
        facts: Some(AttachmentFacts {
            attachment_id: None,
            byte_count: 3,
            media_type: None,
        }),
    };
    assert!(
        !controller
            .patch_context_at_epoch(&baseline, vec![artifact.clone()], vec![], epoch)
            .unwrap()
    );
    controller
        .edit_text(&baseline.selection.destination, "新正文".into())
        .unwrap();
    let epoch = runtime.inspect(|s| s.connection_epoch);
    assert!(
        controller
            .patch_context_at_epoch(&baseline, vec![artifact], vec![], epoch)
            .unwrap()
    );
    assert_eq!(controller.snapshot().draft.text, "新正文");
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn headless_search_uses_the_shared_reader_and_preserves_the_run_draft() {
    let connector = Connector {
        search: true,
        ..Default::default()
    };
    let runtime = Arc::new(SessionRuntime::spawn("test", Arc::new(connector.clone())));
    runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
    let run = RunController::new(runtime.clone()).unwrap();
    run.open(RunScope::main("test", "a")).await.unwrap();
    run.edit_text(&destination("a"), "独立草稿".into()).unwrap();
    let reader = SidebarSearchController::new(runtime.clone()).unwrap();
    let mut changes = reader.subscribe();
    reader.set_query("中文检索").unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while reader.snapshot().result.is_none() {
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    let found = reader.snapshot();
    assert_eq!(
        found.hits[0].address,
        SessionAddress {
            endpoint_id: "test".into(),
            session_id: "a".into()
        }
    );
    assert_eq!(found.hits[0].hit.snippet, "中文检索");
    assert_eq!(
        found.unresolved.len(),
        1,
        "the empty inventory is not fabricated from a hit"
    );
    assert_eq!(run.snapshot().draft.text, "独立草稿");
    let encoded = serde_json::to_value(found.as_ref()).unwrap();
    assert_eq!(encoded["input"]["phase"]["complete"]["hasMore"], true);
    let roundtrip: SidebarSearchPublication = serde_json::from_value(encoded).unwrap();
    assert_eq!(&roundtrip, found.as_ref());
    let epoch = runtime.inspect(|state| state.connection_epoch);
    assert!(
        runtime
            .search_sessions_at_epoch(epoch + 1, "query", "wrong-epoch")
            .await
            .is_err()
    );
    let requests = connector.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].0, "session.search");
    runtime.stop().await.unwrap();
}
