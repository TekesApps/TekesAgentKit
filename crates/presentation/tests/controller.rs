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
    sent: Arc<Notify>,
    release: Arc<Notify>,
    calls: Arc<AtomicUsize>,
    requests: Arc<std::sync::Mutex<Vec<(String, Value)>>>,
    reject_discard: Arc<AtomicBool>,
    hold_fork: Arc<AtomicBool>,
    bounded: Arc<AtomicBool>,
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
        if method == "subagents.history" {
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
