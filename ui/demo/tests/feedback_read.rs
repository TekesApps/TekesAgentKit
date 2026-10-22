use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tekes_agent_contract::{HostServices, HostedConnector, Result};
use tekes_agent_presentation::RunScope;
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::RunBinding;
use tekes_agent_ui_demo::fixture::Fixture;

struct FeedbackService {
    advertised: bool,
    delay_first: bool,
    calls: AtomicUsize,
    reply: Mutex<Value>,
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
}
fn service(delay_first: bool, advertised: bool) -> Arc<FeedbackService> {
    Arc::new(FeedbackService {
        advertised,
        delay_first,
        calls: AtomicUsize::new(0),
        reply: Mutex::new(
            json!({"items":[{"messageId":"m","rating":"positive","version":"opaque:1","note":"已提交备注"}]}),
        ),
        entered: Default::default(),
        release: Default::default(),
    })
}
#[async_trait::async_trait]
impl HostServices for FeedbackService {
    fn methods(&self) -> BTreeSet<String> {
        if self.advertised {
            ["feedback.list".into()].into()
        } else {
            Default::default()
        }
    }
    async fn call(
        &self,
        endpoint: &str,
        method: &str,
        payload: Value,
        _rpc: &str,
    ) -> Result<Value> {
        assert_eq!(endpoint, "fixture");
        assert_eq!(method, "feedback.list");
        assert_eq!(payload, json!({"sessionId":"a"}));
        let call = self.calls.fetch_add(1, Ordering::AcqRel);
        if call == 0 && self.delay_first {
            self.entered.notify_one();
            self.release.notified().await;
        }
        Ok(self.reply.lock().unwrap().clone())
    }
}
async fn setup(service: Arc<FeedbackService>) -> (Arc<SessionRuntime>, Arc<RunBinding>) {
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(HostedConnector {
            endpoint: Arc::new(Fixture::default().with_children()),
            services: service,
        }),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let binding = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    (runtime, binding)
}
#[tokio::test]
async fn committed_feedback_is_bound_to_original_binding_latest_read_and_connection() {
    let service = service(false, true);
    let (runtime, binding) = setup(service.clone()).await;
    let first = binding.read_feedback().await.unwrap();
    assert!(first.is_current(&binding));
    assert!(!first.put_advertised() && !first.delete_advertised());
    assert_eq!(first.records().get("m").unwrap().version, "opaque:1");
    assert_eq!(
        first.records().get("m").unwrap().note.as_deref(),
        Some("已提交备注")
    );
    let other = Arc::new(
        RunBinding::open_main(runtime.clone(), binding.scope().clone())
            .await
            .unwrap(),
    );
    assert!(
        !first.is_current(&other),
        "same scope cannot substitute another binding"
    );
    let second = binding.read_feedback().await.unwrap();
    assert!(second.publication() > first.publication());
    assert!(!first.is_current(&binding));
    assert!(second.is_current(&binding));
    runtime.reconnect().await.unwrap();
    assert!(!second.is_current(&binding));
    let renewed = binding.read_feedback().await.unwrap();
    assert!(renewed.connection_epoch() > second.connection_epoch());
    assert_eq!(service.calls.load(Ordering::Acquire), 3);
    runtime.stop().await.unwrap();
    assert!(!renewed.is_current(&binding));
}
#[tokio::test]
async fn an_older_read_cannot_publish_after_a_newer_read() {
    let service = service(true, true);
    let (runtime, binding) = setup(service.clone()).await;
    let original = binding.clone();
    let old = tokio::spawn(async move { original.read_feedback().await });
    tokio::time::timeout(Duration::from_secs(3), service.entered.notified())
        .await
        .unwrap();
    let fresh = tokio::time::timeout(Duration::from_secs(3), binding.read_feedback())
        .await
        .unwrap()
        .unwrap();
    service.release.notify_one();
    assert!(old.await.unwrap().is_err());
    assert!(fresh.is_current(&binding));
    assert_eq!(service.calls.load(Ordering::Acquire), 2);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn feedback_reads_crossing_reconnect_are_not_replayed_or_adopted() {
    let service = service(true, true);
    let (runtime, binding) = setup(service.clone()).await;
    let original = binding.clone();
    let old = tokio::spawn(async move { original.read_feedback().await });
    tokio::time::timeout(Duration::from_secs(3), service.entered.notified())
        .await
        .unwrap();
    runtime.reconnect().await.unwrap();
    service.release.notify_one();
    assert!(
        tokio::time::timeout(Duration::from_secs(3), old)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    assert_eq!(service.calls.load(Ordering::Acquire), 1);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn malformed_feedback_is_an_error_and_a_later_explicit_empty_read_is_known_empty() {
    let service = service(false, true);
    let (runtime, binding) = setup(service.clone()).await;
    *service.reply.lock().unwrap() = json!({"items":[{"messageId":"m","rating":"positive","version":"v"},{"messageId":"m","rating":"negative","version":"v2"}]});
    assert!(binding.read_feedback().await.is_err());
    *service.reply.lock().unwrap() = json!({"items":[]});
    let fresh = binding.read_feedback().await.unwrap();
    assert!(fresh.is_current(&binding));
    assert!(fresh.records().items.is_empty());
    assert_eq!(service.calls.load(Ordering::Acquire), 2);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn an_unadvertised_feedback_method_never_reaches_io() {
    let service = service(false, false);
    let (runtime, binding) = setup(service.clone()).await;
    assert!(binding.read_feedback().await.is_err());
    assert_eq!(service.calls.load(Ordering::Acquire), 0);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn bounded_child_history_is_not_treated_as_an_independent_feedback_session() {
    let service = service(false, true);
    let (runtime, _binding) = setup(service.clone()).await;
    let child = tekes_agent_ui_bindings::ChildPaneBinding::open(
        runtime.clone(),
        RunScope::main("fixture", "b"),
        "bounded-b",
        tekes_agent_presentation::ChildHistoryLayout::SameSession,
        None,
    )
    .await
    .unwrap();
    assert!(child.binding().read_feedback().await.is_err());
    assert_eq!(
        service.calls.load(Ordering::Acquire),
        0,
        "never substitute the parent session for a bounded child"
    );
    runtime.stop().await.unwrap();
}

use tekes_agent_components_ui::{TranscriptContent, TurnFeedback};
use tekes_agent_ui_demo::component_demo::{ComponentDemo, DemoCommand, FeedbackInitialRequest};
use tekes_ui_controls::composition::ComponentId;
fn initial(view: &mut ComponentDemo) -> FeedbackInitialRequest {
    let commands = view.take_background_commands();
    assert_eq!(
        commands.len(),
        1,
        "one initial metadata read per source/epoch"
    );
    match commands.into_iter().next().unwrap() {
        DemoCommand::ReadInitialFeedback(request) => *request,
        _ => panic!("initial feedback command"),
    }
}
fn original_control(view: &ComponentDemo) -> ComponentId {
    let root = view.transcript_content_id(view.ids()).unwrap();
    view.tree
        .get(root)
        .unwrap()
        .component::<TranscriptContent>()
        .unwrap()
        .feedback_controls()
        .next()
        .unwrap()
        .0
}
fn feedback(view: &ComponentDemo, id: ComponentId) -> &TurnFeedback {
    view.tree
        .get(id)
        .unwrap()
        .component::<TurnFeedback>()
        .unwrap()
}
fn mounted(binding: &Arc<RunBinding>) -> ComponentDemo {
    let mut view = ComponentDemo::new([binding.clone()]).unwrap();
    view.enable_transcript_content().unwrap();
    view
}
#[tokio::test]
async fn host_initial_read_adopts_committed_note_once_and_reloads_after_reconnect() {
    let service = service(false, true);
    let (runtime, binding) = setup(service.clone()).await;
    let mut view = mounted(&binding);
    let id = original_control(&view);
    let message = feedback(&view, id)
        .target()
        .unwrap()
        .message_id()
        .to_string();
    *service.reply.lock().unwrap() = json!({"items":[{"messageId":message,"rating":"positive","version":"opaque:1","note":"原已提交备注"}]});
    let request = initial(&mut view);
    for _ in 0..3 {
        view.sync().unwrap();
    }
    assert!(view.take_background_commands().is_empty());
    assert!(feedback(&view, id).state().is_none());
    let draft = binding.current_run().unwrap().draft.clone();
    view.complete_initial_feedback(request.execute().await)
        .unwrap();
    assert_eq!(
        feedback(&view, id)
            .state()
            .unwrap()
            .current
            .as_ref()
            .unwrap()
            .note
            .as_deref(),
        Some("原已提交备注")
    );
    assert_eq!(binding.current_run().unwrap().draft, draft);
    assert_eq!(service.calls.load(Ordering::Acquire), 1);
    runtime.reconnect().await.unwrap();
    view.sync().unwrap();
    let request = initial(&mut view);
    *service.reply.lock().unwrap() = json!({"items":[]});
    view.complete_initial_feedback(request.execute().await)
        .unwrap();
    assert!(feedback(&view, id).state().unwrap().current.is_none());
    assert!(!feedback(&view, id).needs_refresh());
    assert_eq!(service.calls.load(Ordering::Acquire), 2);
    assert!(view.take_background_commands().is_empty());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn host_initial_read_failure_keeps_history_and_has_no_sync_retry_loop() {
    let service = service(false, true);
    let (runtime, binding) = setup(service.clone()).await;
    let mut view = mounted(&binding);
    let id = original_control(&view);
    let draft = binding.current_run().unwrap().draft.clone();
    *service.reply.lock().unwrap() = json!({"items":"invalid"});
    let request = initial(&mut view);
    view.complete_initial_feedback(request.execute().await)
        .unwrap();
    assert!(feedback(&view, id).state().is_none());
    assert!(feedback(&view, id).diagnostic().is_some());
    assert!(feedback(&view, id).needs_refresh());
    for _ in 0..3 {
        view.sync().unwrap();
    }
    assert!(view.take_background_commands().is_empty());
    assert_eq!(service.calls.load(Ordering::Acquire), 1);
    assert_eq!(binding.current_run().unwrap().draft, draft);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn queued_old_epoch_read_does_not_issue_io_or_replace_the_reconnected_frame() {
    let service = service(false, true);
    let (runtime, binding) = setup(service.clone()).await;
    let mut view = mounted(&binding);
    let old = initial(&mut view);
    runtime.reconnect().await.unwrap();
    view.sync().unwrap();
    let fresh = initial(&mut view);
    view.complete_initial_feedback(fresh.execute().await)
        .unwrap();
    let id = original_control(&view);
    let state = feedback(&view, id).state().unwrap().clone();
    view.complete_initial_feedback(old.execute().await).unwrap();
    assert_eq!(feedback(&view, id).state(), Some(&state));
    assert_eq!(service.calls.load(Ordering::Acquire), 1);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn explicit_refresh_cancels_queued_initial_io_and_recovers_a_failed_read() {
    use tekes_ui_controls::composition::AccessibilityAction;
    let service = service(false, true);
    let (runtime, binding) = setup(service.clone()).await;
    let mut view = mounted(&binding);
    let warm = initial(&mut view);
    view.complete_initial_feedback(warm.execute().await)
        .unwrap();
    runtime.reconnect().await.unwrap();
    view.sync().unwrap();
    let queued = initial(&mut view);
    let id = original_control(&view);
    let refresh = feedback(&view, id).ids().refresh;
    let mut commands = view
        .accessibility_action(refresh, None, AccessibilityAction::Press)
        .unwrap();
    assert_eq!(commands.len(), 1);
    let DemoCommand::Feedback(request) = commands.remove(0) else {
        panic!("refresh request")
    };
    *service.reply.lock().unwrap() = json!({"items":"invalid"});
    view.complete_feedback(request.execute().await).unwrap();
    assert!(feedback(&view, id).needs_refresh());
    assert!(feedback(&view, id).diagnostic().is_some());
    view.complete_initial_feedback(queued.execute().await)
        .unwrap();
    assert_eq!(
        service.calls.load(Ordering::Acquire),
        2,
        "cancelled initial read never issues IO"
    );
    *service.reply.lock().unwrap() = json!({"items":[]});
    let refresh = feedback(&view, id).ids().refresh;
    let mut commands = view
        .accessibility_action(refresh, None, AccessibilityAction::Press)
        .unwrap();
    assert_eq!(commands.len(), 1);
    let DemoCommand::Feedback(request) = commands.remove(0) else {
        panic!("recovery request")
    };
    view.complete_feedback(request.execute().await).unwrap();
    assert!(feedback(&view, id).state().unwrap().current.is_none());
    assert!(!feedback(&view, id).needs_refresh());
    assert!(feedback(&view, id).notice().is_empty());
    assert!(view.take_background_commands().is_empty());
    assert_eq!(service.calls.load(Ordering::Acquire), 3);
    let old_epoch = binding.feedback_read_epoch().unwrap();
    runtime.reconnect().await.unwrap();
    assert!(binding.read_feedback_at_epoch(old_epoch).await.is_err());
    assert_eq!(
        service.calls.load(Ordering::Acquire),
        3,
        "old epoch cannot follow reconnect"
    );
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn in_flight_initial_completion_cannot_replace_the_reconnected_record() {
    let service = service(true, true);
    let (runtime, binding) = setup(service.clone()).await;
    let mut view = mounted(&binding);
    let request = initial(&mut view);
    let execution = tokio::spawn(request.execute());
    tokio::time::timeout(Duration::from_secs(3), service.entered.notified())
        .await
        .unwrap();
    runtime.reconnect().await.unwrap();
    view.sync().unwrap();
    let fresh = initial(&mut view);
    *service.reply.lock().unwrap() = json!({"items":[]});
    view.complete_initial_feedback(fresh.execute().await)
        .unwrap();
    let id = original_control(&view);
    let current = feedback(&view, id).state().unwrap().clone();
    service.release.notify_one();
    view.complete_initial_feedback(execution.await.unwrap())
        .unwrap();
    assert_eq!(feedback(&view, id).state(), Some(&current));
    assert!(feedback(&view, id).diagnostic().is_none());
    assert_eq!(service.calls.load(Ordering::Acquire), 2);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn hidden_source_initial_receipt_never_adopts_into_the_selected_session() {
    let service = service(false, true);
    let (runtime, a) = setup(service.clone()).await;
    let b = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "b"))
            .await
            .unwrap(),
    );
    let mut view = ComponentDemo::new([a.clone(), b.clone()]).unwrap();
    view.enable_transcript_content().unwrap();
    let a_id = original_control(&view);
    let request = initial(&mut view);
    let a_draft = a.current_run().unwrap().draft.clone();
    let b_draft = b.current_run().unwrap().draft.clone();
    view.select(1).unwrap();
    let b_id = original_control(&view);
    assert_ne!(a_id, b_id);
    view.complete_initial_feedback(request.execute().await)
        .unwrap();
    assert!(feedback(&view, b_id).state().is_none());
    assert_eq!(view.selected(), 1);
    assert_eq!(a.current_run().unwrap().draft, a_draft);
    assert_eq!(b.current_run().unwrap().draft, b_draft);
    view.select(0).unwrap();
    assert_eq!(original_control(&view), a_id);
    assert!(feedback(&view, a_id).state().is_some());
    assert_eq!(service.calls.load(Ordering::Acquire), 1);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn dropped_host_initial_request_never_reads_the_endpoint() {
    let service = service(false, true);
    let (runtime, binding) = setup(service.clone()).await;
    let mut view = mounted(&binding);
    let request = initial(&mut view);
    drop(view);
    let _completion = request.execute().await;
    assert_eq!(service.calls.load(Ordering::Acquire), 0);
    runtime.stop().await.unwrap();
}
