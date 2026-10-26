use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tekes_agent_contract::{
    Error, HostServices, HostedConnector, Result, SessionMessageFeedbackRating as Rating,
};
use tekes_agent_presentation::{
    FeedbackChange as Change, FeedbackTargetCapture, RunScope, TranscriptDocument,
    TurnFeedbackTarget,
};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{
    FeedbackFrame, FeedbackOutcome, FeedbackPreparation, FeedbackRequest, RunBinding,
};
use tekes_agent_ui_demo::fixture::Fixture;

#[derive(Clone)]
enum Fault {
    TimeoutAfterApply,
    BadReceipt,
    Remote(Value),
    Receipt(Value),
}
#[derive(Default)]
struct State {
    records: BTreeMap<String, Value>,
    version: u64,
    calls: Vec<(String, Value, String)>,
    fault: Option<Fault>,
}
struct Service {
    state: Mutex<State>,
    writable: bool,
    delay: AtomicBool,
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
}
impl Service {
    fn writes(&self) -> Vec<(String, Value, String)> {
        self.state
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter(|(method, _, _)| method != "feedback.list")
            .cloned()
            .collect()
    }
}
#[async_trait::async_trait]
impl HostServices for Service {
    fn methods(&self) -> BTreeSet<String> {
        let mut methods: BTreeSet<String> = ["feedback.list".into()].into();
        if self.writable {
            methods.extend(["feedback.put".into(), "feedback.delete".into()]);
        }
        methods
    }
    async fn call(
        &self,
        endpoint: &str,
        method: &str,
        payload: Value,
        rpc_id: &str,
    ) -> Result<Value> {
        assert_eq!(endpoint, "fixture");
        assert_eq!(payload["sessionId"], "a");
        self.state
            .lock()
            .unwrap()
            .calls
            .push((method.into(), payload.clone(), rpc_id.into()));
        if method == "feedback.list" {
            return Ok(
                json!({"items":self.state.lock().unwrap().records.values().collect::<Vec<_>>()}),
            );
        }
        if self.delay.load(Ordering::Acquire) {
            self.entered.notify_one();
            self.release.notified().await;
        }
        let mut state = self.state.lock().unwrap();
        let fault = state.fault.take();
        if let Some(Fault::Remote(error)) = fault {
            return Err(Error::Remote(error));
        }
        let id = payload["messageId"].as_str().unwrap().to_string();
        let current = state.records.get(&id);
        if current.map(|value| &value["version"])
            != payload.get("ifVersion").filter(|value| !value.is_null())
        {
            return Err(Error::Remote(
                json!({"code":"version-conflict","details":{"current":current}}),
            ));
        }
        let result = if method == "feedback.put" {
            state.version += 1;
            let value = json!({"messageId":id,"rating":payload["rating"],"note":payload["note"],"version":format!("opaque:{}",state.version)});
            state.records.insert(id.clone(), value.clone());
            value
        } else {
            assert_eq!(method, "feedback.delete");
            state.records.remove(&id);
            Value::Null
        };
        match fault {
            Some(Fault::Receipt(value)) => Ok(value),
            Some(Fault::TimeoutAfterApply) => Err(Error::Timeout),
            Some(Fault::BadReceipt) => {
                let mut wrong = result;
                wrong["messageId"] = json!("foreign");
                Ok(wrong)
            }
            _ => Ok(result),
        }
    }
}
async fn setup(writable: bool) -> (Arc<SessionRuntime>, Arc<RunBinding>, Arc<Service>) {
    let service = Arc::new(Service {
        state: Mutex::new(State::default()),
        writable,
        delay: AtomicBool::new(false),
        entered: Default::default(),
        release: Default::default(),
    });
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(HostedConnector {
            endpoint: Arc::new(Fixture::default()),
            services: service.clone(),
        }),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let binding = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    (runtime, binding, service)
}
fn target(binding: &Arc<RunBinding>) -> FeedbackTargetCapture {
    let snapshot = binding.current_run().unwrap();
    let document = TranscriptDocument::read(&snapshot.transcript).unwrap();
    document
        .sections
        .iter()
        .rev()
        .find_map(TurnFeedbackTarget::read)
        .unwrap()
        .capture(binding.scope())
        .unwrap()
}
fn prepare(
    binding: &Arc<RunBinding>,
    frame: &FeedbackFrame,
    target: &FeedbackTargetCapture,
    change: Change,
) -> FeedbackRequest {
    let FeedbackPreparation::Request(request) =
        binding.prepare_feedback(frame, target, change).unwrap()
    else {
        panic!("write")
    };
    *request
}
async fn apply(request: FeedbackRequest) -> FeedbackFrame {
    let outcome = request.execute().await;
    let FeedbackOutcome::Applied(frame) = outcome else {
        panic!("applied, got {outcome:?}")
    };
    frame
}
#[tokio::test]
async fn explicit_cas_lifecycle_preserves_notes_and_same_rating_toggle_deletes_once() {
    let (runtime, binding, service) = setup(true).await;
    let target = target(&binding);
    let first = binding.read_feedback().await.unwrap();
    let request = prepare(
        &binding,
        &first,
        &target,
        Change::ToggleRating(Rating::Positive),
    );
    let rpc = request.rpc_id().to_string();
    assert!(
        binding
            .prepare_feedback(&first, &target, Change::ToggleRating(Rating::Negative))
            .is_err(),
        "cloned original frame cannot admit a second gesture"
    );
    let added = apply(request).await;
    assert!(added.is_current(&binding));
    assert!(!first.is_current(&binding));
    let noted = apply(prepare(
        &binding,
        &added,
        &target,
        Change::SetNote(Some("中文备注".into())),
    ))
    .await;
    let changed = apply(prepare(
        &binding,
        &noted,
        &target,
        Change::SetRating(Some(Rating::Negative)),
    ))
    .await;
    assert_eq!(
        changed
            .records()
            .get(target.message_id())
            .unwrap()
            .note
            .as_deref(),
        Some("中文备注")
    );
    let replaced = apply(prepare(
        &binding,
        &changed,
        &target,
        Change::ToggleRating(Rating::Positive),
    ))
    .await;
    assert!(
        replaced
            .records()
            .get(target.message_id())
            .unwrap()
            .note
            .is_none()
    );
    let deleted = apply(prepare(
        &binding,
        &replaced,
        &target,
        Change::ToggleRating(Rating::Positive),
    ))
    .await;
    assert!(deleted.records().items.is_empty());
    let writes = service.writes();
    assert_eq!(writes.len(), 5);
    assert_eq!(writes[0].2, rpc);
    assert!(writes[0].1["ifVersion"].is_null());
    assert_eq!(
        writes[2].1["ifVersion"],
        noted.records().get(target.message_id()).unwrap().version
    );
    assert_eq!(writes[4].0, "feedback.delete");
    assert_eq!(
        writes
            .iter()
            .map(|row| &row.2)
            .collect::<BTreeSet<_>>()
            .len(),
        5
    );
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn noops_and_missing_capabilities_do_not_claim_a_frame_or_write() {
    let (runtime, binding, service) = setup(false).await;
    let target = target(&binding);
    let frame = binding.read_feedback().await.unwrap();
    assert!(matches!(
        binding
            .prepare_feedback(&frame, &target, Change::SetNote(None))
            .unwrap(),
        FeedbackPreparation::Unchanged(_)
    ));
    assert!(matches!(
        binding
            .prepare_feedback(&frame, &target, Change::SetRating(None))
            .unwrap(),
        FeedbackPreparation::Unchanged(_)
    ));
    assert!(
        binding
            .prepare_feedback(&frame, &target, Change::SetNote(Some("note".into())))
            .is_err()
    );
    assert!(
        binding
            .prepare_feedback(&frame, &target, Change::ToggleRating(Rating::Positive))
            .is_err()
    );
    assert!(frame.is_current(&binding));
    assert!(service.writes().is_empty());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn conflict_adopts_current_record_or_authoritative_absence_without_resending() {
    let (runtime, binding, service) = setup(true).await;
    let target = target(&binding);
    let frame = binding.read_feedback().await.unwrap();
    service.state.lock().unwrap().records.insert(target.message_id().into(),json!({"messageId":target.message_id(),"rating":"negative","note":"other actor","version":"external"}));
    let outcome = prepare(
        &binding,
        &frame,
        &target,
        Change::ToggleRating(Rating::Positive),
    )
    .execute()
    .await;
    let FeedbackOutcome::Conflict(current) = outcome else {
        panic!("conflict")
    };
    assert!(current.is_current(&binding));
    assert_eq!(
        current.records().get(target.message_id()).unwrap().version,
        "external"
    );
    let noted = apply(prepare(&binding, &current, &target, Change::SetNote(None))).await;
    assert_eq!(
        noted.records().get(target.message_id()).unwrap().rating,
        Rating::Negative
    );
    service.state.lock().unwrap().records.clear();
    let outcome = prepare(
        &binding,
        &noted,
        &target,
        Change::SetRating(Some(Rating::Positive)),
    )
    .execute()
    .await;
    let FeedbackOutcome::Conflict(current) = outcome else {
        panic!("authoritative absence")
    };
    assert!(current.records().items.is_empty());
    assert_eq!(service.writes().len(), 3);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn stale_queued_requests_other_bindings_and_dropped_requests_never_write() {
    let (runtime, binding, service) = setup(true).await;
    let target = target(&binding);
    let frame = binding.read_feedback().await.unwrap();
    let other = Arc::new(
        RunBinding::open_main(runtime.clone(), binding.scope().clone())
            .await
            .unwrap(),
    );
    assert!(
        other
            .prepare_feedback(&frame, &target, Change::ToggleRating(Rating::Positive))
            .is_err()
    );
    let request = prepare(
        &binding,
        &frame,
        &target,
        Change::ToggleRating(Rating::Positive),
    );
    let newer = binding.read_feedback().await.unwrap();
    assert!(matches!(
        request.execute().await,
        FeedbackOutcome::Rejected { frame: None, .. }
    ));
    assert!(newer.is_current(&binding));
    drop(prepare(
        &binding,
        &newer,
        &target,
        Change::ToggleRating(Rating::Positive),
    ));
    assert!(!newer.is_current(&binding));
    let refreshed = binding.read_feedback().await.unwrap();
    assert!(refreshed.is_current(&binding));
    assert!(service.writes().is_empty());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn reconnect_before_io_rejects_and_reconnect_after_dispatch_needs_refresh() {
    let (runtime, binding, service) = setup(true).await;
    let target = target(&binding);
    let frame = binding.read_feedback().await.unwrap();
    let request = prepare(
        &binding,
        &frame,
        &target,
        Change::ToggleRating(Rating::Positive),
    );
    runtime.reconnect().await.unwrap();
    assert!(matches!(
        request.execute().await,
        FeedbackOutcome::Rejected { frame: None, .. }
    ));
    assert!(service.writes().is_empty());
    let frame = binding.read_feedback().await.unwrap();
    service.delay.store(true, Ordering::Release);
    let request = prepare(
        &binding,
        &frame,
        &target,
        Change::ToggleRating(Rating::Positive),
    );
    let pending = tokio::spawn(request.execute());
    tokio::time::timeout(Duration::from_secs(3), service.entered.notified())
        .await
        .unwrap();
    runtime.reconnect().await.unwrap();
    service.release.notify_one();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(3), pending)
            .await
            .unwrap()
            .unwrap(),
        FeedbackOutcome::NeedsRefresh(_)
    ));
    assert_eq!(service.writes().len(), 1);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn unknown_or_mismatched_receipts_require_a_read_and_never_auto_retry() {
    for fault in [Fault::TimeoutAfterApply, Fault::BadReceipt] {
        let (runtime, binding, service) = setup(true).await;
        let target = target(&binding);
        let frame = binding.read_feedback().await.unwrap();
        service.state.lock().unwrap().fault = Some(fault);
        assert!(matches!(
            prepare(
                &binding,
                &frame,
                &target,
                Change::ToggleRating(Rating::Positive)
            )
            .execute()
            .await,
            FeedbackOutcome::NeedsRefresh(_)
        ));
        assert!(!frame.is_current(&binding));
        assert_eq!(service.writes().len(), 1);
        let confirmed = binding.read_feedback().await.unwrap();
        assert!(confirmed.records().get(target.message_id()).is_some());
        let deleted = apply(prepare(
            &binding,
            &confirmed,
            &target,
            Change::ToggleRating(Rating::Positive),
        ))
        .await;
        assert!(deleted.records().items.is_empty());
        assert_eq!(
            service.writes().len(),
            2,
            "only a new explicit gesture after read performs another operation"
        );
        runtime.stop().await.unwrap();
    }
}
#[tokio::test]
async fn a_later_read_cannot_be_overwritten_by_an_older_write_completion() {
    let (runtime, binding, service) = setup(true).await;
    let target = target(&binding);
    let frame = binding.read_feedback().await.unwrap();
    service.delay.store(true, Ordering::Release);
    let pending = tokio::spawn(
        prepare(
            &binding,
            &frame,
            &target,
            Change::ToggleRating(Rating::Positive),
        )
        .execute(),
    );
    tokio::time::timeout(Duration::from_secs(3), service.entered.notified())
        .await
        .unwrap();
    let newer = binding.read_feedback().await.unwrap();
    service.release.notify_one();
    assert!(matches!(
        pending.await.unwrap(),
        FeedbackOutcome::NeedsRefresh(_)
    ));
    assert!(newer.is_current(&binding));
    assert!(newer.records().items.is_empty());
    let confirmed = binding.read_feedback().await.unwrap();
    assert!(confirmed.records().get(target.message_id()).is_some());
    assert_eq!(service.writes().len(), 1);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn malformed_conflicts_and_remote_timeouts_never_invent_empty_success() {
    for error in [
        json!({"code":"version-conflict","current":{"messageId":"foreign","rating":"positive","version":"v"}}),
        json!({"code":"timeout"}),
    ] {
        let (runtime, binding, service) = setup(true).await;
        let target = target(&binding);
        let frame = binding.read_feedback().await.unwrap();
        service.state.lock().unwrap().fault = Some(Fault::Remote(error));
        assert!(matches!(
            prepare(
                &binding,
                &frame,
                &target,
                Change::ToggleRating(Rating::Positive)
            )
            .execute()
            .await,
            FeedbackOutcome::NeedsRefresh(_)
        ));
        assert_eq!(service.writes().len(), 1);
        runtime.stop().await.unwrap();
    }
}
#[tokio::test]
async fn foreign_scope_and_changed_rendered_response_reject_before_claim_or_io() {
    let (runtime, binding, service) = setup(true).await;
    let frame = binding.read_feedback().await.unwrap();
    let snapshot = binding.current_run().unwrap();
    let document = TranscriptDocument::read(&snapshot.transcript).unwrap();
    let response = document
        .sections
        .iter()
        .rev()
        .find_map(TurnFeedbackTarget::read)
        .unwrap();
    let foreign = response.capture(&RunScope::main("fixture", "b")).unwrap();
    assert!(
        binding
            .prepare_feedback(&frame, &foreign, Change::ToggleRating(Rating::Positive))
            .is_err()
    );
    let mut older = snapshot.transcript.clone();
    older
        .items
        .iter_mut()
        .find(|item| item.id == response.response.item.id)
        .unwrap()
        .text = "older displayed content".into();
    let document = TranscriptDocument::read(&older).unwrap();
    let stale = document
        .sections
        .iter()
        .rev()
        .find_map(TurnFeedbackTarget::read)
        .unwrap()
        .capture(binding.scope())
        .unwrap();
    assert!(
        binding
            .prepare_feedback(&frame, &stale, Change::ToggleRating(Rating::Positive))
            .is_err()
    );
    assert!(frame.is_current(&binding));
    assert!(service.writes().is_empty());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn a_known_rejection_preserves_committed_record_without_optimistic_note() {
    let (runtime, binding, service) = setup(true).await;
    let target = target(&binding);
    service.state.lock().unwrap().records.insert(target.message_id().into(),json!({"messageId":target.message_id(),"rating":"positive","note":"old note","version":"old"}));
    let frame = binding.read_feedback().await.unwrap();
    service.state.lock().unwrap().fault = Some(Fault::Remote(json!({"code":"denied"})));
    let FeedbackOutcome::Rejected {
        frame: Some(frame), ..
    } = prepare(
        &binding,
        &frame,
        &target,
        Change::SetNote(Some("new note".into())),
    )
    .execute()
    .await
    else {
        panic!("known rejection")
    };
    assert!(frame.is_current(&binding));
    assert_eq!(
        frame
            .records()
            .get(target.message_id())
            .unwrap()
            .note
            .as_deref(),
        Some("old note")
    );
    assert_eq!(service.writes().len(), 1);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn explicit_native_delete_ack_is_accepted_but_false_is_not_fabricated_success() {
    for deleted in [true, false] {
        let (runtime, binding, service) = setup(true).await;
        let target = target(&binding);
        let frame = binding.read_feedback().await.unwrap();
        let created = apply(prepare(
            &binding,
            &frame,
            &target,
            Change::ToggleRating(Rating::Positive),
        ))
        .await;
        service.state.lock().unwrap().fault = Some(Fault::Receipt(json!({"deleted":deleted})));
        let outcome = prepare(&binding, &created, &target, Change::SetRating(None))
            .execute()
            .await;
        if deleted {
            let FeedbackOutcome::Applied(frame) = outcome else {
                panic!("explicit deleted ack")
            };
            assert!(frame.records().get(target.message_id()).is_none());
        } else {
            assert!(matches!(outcome, FeedbackOutcome::NeedsRefresh(_)));
        }
        assert_eq!(service.writes().len(), 2);
        runtime.stop().await.unwrap();
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn simultaneous_preparations_from_one_observed_frame_admit_one_request() {
    let (runtime, binding, service) = setup(true).await;
    let target = target(&binding);
    let frame = binding.read_feedback().await.unwrap();
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let mut tasks = vec![];
    for _ in 0..2 {
        let (binding, target, frame, barrier) = (
            binding.clone(),
            target.clone(),
            frame.clone(),
            barrier.clone(),
        );
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            binding.prepare_feedback(&frame, &target, Change::ToggleRating(Rating::Positive))
        }));
    }
    let mut admitted = vec![];
    for task in tasks {
        if let Ok(FeedbackPreparation::Request(request)) = task.await.unwrap() {
            admitted.push(request);
        }
    }
    assert_eq!(admitted.len(), 1);
    let applied = apply(*admitted.pop().unwrap()).await;
    assert!(applied.is_current(&binding));
    assert_eq!(service.writes().len(), 1);
    runtime.stop().await.unwrap();
}

use tekes_agent_components_ui::{FeedbackGesture, FeedbackIds, TurnFeedback};
use tekes_agent_ui_bindings::FeedbackUiAdoption;
use tekes_ui_controls::composition::{AccessibilityAction, ComponentId, ContentEvent, ContentHost};
fn control(host: &mut ContentHost, root: ComponentId) -> &mut TurnFeedback {
    host.get_mut(root)
        .unwrap()
        .component_mut::<TurnFeedback>()
        .unwrap()
}
fn activate(host: &mut ContentHost, root: ComponentId, id: ComponentId) -> Vec<FeedbackGesture> {
    host.accessibility_action(id, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    control(host, root).take_intents(&actions)
}
fn ids(host: &mut ContentHost, root: ComponentId) -> FeedbackIds {
    control(host, root).ids()
}
fn mounted(binding: &Arc<RunBinding>, frame: &FeedbackFrame) -> (ContentHost, ComponentId) {
    let mut c = TurnFeedback::new(190, Default::default());
    c.update(frame.controls(binding, &target(binding)).unwrap())
        .unwrap();
    let root = c.ids().root;
    (c.into_host().unwrap(), root)
}
async fn dispatch_ui(
    binding: &Arc<RunBinding>,
    frame: &mut Option<FeedbackFrame>,
    host: &mut ContentHost,
    root: ComponentId,
    gesture: FeedbackGesture,
) {
    match binding
        .prepare_feedback_gesture(frame.as_ref(), gesture)
        .execute()
        .await
        .apply(control(host, root))
    {
        FeedbackUiAdoption::Adopted(next) => *frame = next,
        FeedbackUiAdoption::Ignored => panic!("current mount must adopt its completion"),
    }
}
fn replace_note(host: &mut ContentHost, root: ComponentId, text: &str) {
    let id = ids(host, root).editor;
    host.focus(Some(id), true);
    assert!(host.focused_text().is_some());
    let len = control(host, root)
        .editor()
        .unwrap()
        .input
        .text
        .encode_utf16()
        .count();
    host.accessibility_action(id, None, AccessibilityAction::SetSelection(0..len));
    host.drain_actions();
    host.event(&ContentEvent::Commit {
        text: text.into(),
        replacement: None,
    });
    host.drain_actions();
}
#[tokio::test]
async fn feedback_controls_use_public_actions_and_upstream_ime_before_one_explicit_note_save() {
    let (runtime, binding, service) = setup(true).await;
    let initial = binding.read_feedback().await.unwrap();
    let (mut host, root) = mounted(&binding, &initial);
    let mut frame = Some(initial);
    let id = ids(&mut host, root).positive;
    let gesture = activate(&mut host, root, id).pop().unwrap();
    let duplicate = gesture.clone();
    let request = binding.prepare_feedback_gesture(frame.as_ref(), gesture);
    // A duplicate bridge cannot retire the real in-flight gesture or its pending UI.
    assert!(matches!(
        binding
            .prepare_feedback_gesture(frame.as_ref(), duplicate)
            .execute()
            .await
            .apply(control(&mut host, root)),
        FeedbackUiAdoption::Ignored
    ));
    assert!(control(&mut host, root).is_pending());
    match request.execute().await.apply(control(&mut host, root)) {
        FeedbackUiAdoption::Adopted(next) => frame = next,
        _ => panic!("apply"),
    }
    assert_eq!(service.writes().len(), 1);
    let id = ids(&mut host, root).edit_note;
    assert!(activate(&mut host, root, id).is_empty());
    replace_note(&mut host, root, " \n\u{feff}中文备注\u{feff} \n");
    let editor_id = ids(&mut host, root).editor;
    host.event(&ContentEvent::Preedit {
        text: "ni".into(),
        selection: 2..2,
        clauses: std::iter::once(0..2).collect(),
        replacement: None,
    });
    host.drain_actions();
    let actions = host.drain_actions();
    assert!(control(&mut host, root).take_intents(&actions).is_empty());
    assert!(control(&mut host, root).is_composing());
    let save = ids(&mut host, root).save;
    assert!(activate(&mut host, root, save).is_empty());
    let close = ids(&mut host, root).close;
    assert!(activate(&mut host, root, close).is_empty());
    assert!(control(&mut host, root).is_open());
    host.event(&ContentEvent::Commit {
        text: String::new(),
        replacement: None,
    });
    host.drain_actions();
    replace_note(&mut host, root, " \n\u{feff}中文备注\u{feff} \n");
    // Generic Submit/Enter never invokes the Agent save action.
    host.event(&ContentEvent::TextCommand("insertNewline:".into()));
    let actions = host.drain_actions();
    assert!(control(&mut host, root).take_intents(&actions).is_empty());
    assert_eq!(service.writes().len(), 1);
    assert_eq!(ids(&mut host, root).editor, editor_id);
    let save = ids(&mut host, root).save;
    let gesture = activate(&mut host, root, save).pop().unwrap();
    dispatch_ui(&binding, &mut frame, &mut host, root, gesture).await;
    assert!(!control(&mut host, root).is_open());
    assert_eq!(
        frame.as_ref().unwrap().records().items[0].note.as_deref(),
        Some("中文备注")
    );
    let positive = ids(&mut host, root).positive;
    let gesture = activate(&mut host, root, positive).pop().unwrap();
    dispatch_ui(&binding, &mut frame, &mut host, root, gesture).await;
    assert!(frame.as_ref().unwrap().records().items.is_empty());
    assert_eq!(service.writes().len(), 3);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn note_conflict_preserves_editor_and_unknown_outcome_requires_explicit_read() {
    let (runtime, binding, service) = setup(true).await;
    let target = target(&binding);
    let initial = binding.read_feedback().await.unwrap();
    let rated = apply(prepare(
        &binding,
        &initial,
        &target,
        Change::ToggleRating(Rating::Positive),
    ))
    .await;
    let (mut host, root) = mounted(&binding, &rated);
    let mut frame = Some(rated);
    let note = ids(&mut host, root).edit_note;
    activate(&mut host, root, note);
    replace_note(&mut host, root, "本地草稿");
    let editor_id = ids(&mut host, root).editor;
    let remote = json!({"messageId":target.message_id(),"rating":"negative","version":"other","note":"远端"});
    service
        .state
        .lock()
        .unwrap()
        .records
        .insert(target.message_id().into(), remote);
    let save = ids(&mut host, root).save;
    let gesture = activate(&mut host, root, save).pop().unwrap();
    dispatch_ui(&binding, &mut frame, &mut host, root, gesture).await;
    assert_eq!(
        control(&mut host, root).editor().unwrap().input.text,
        "本地草稿"
    );
    assert_eq!(ids(&mut host, root).editor, editor_id);
    assert_eq!(
        frame.as_ref().unwrap().records().items[0].rating,
        Rating::Negative
    );
    assert!(control(&mut host, root).notice().contains("别处"));
    service.state.lock().unwrap().fault = Some(Fault::TimeoutAfterApply);
    let save = ids(&mut host, root).save;
    let gesture = activate(&mut host, root, save).pop().unwrap();
    dispatch_ui(&binding, &mut frame, &mut host, root, gesture).await;
    assert!(frame.is_none());
    assert!(control(&mut host, root).needs_refresh());
    let count = service.writes().len();
    let save = ids(&mut host, root).save;
    assert!(activate(&mut host, root, save).is_empty());
    assert_eq!(service.writes().len(), count);
    let refresh = ids(&mut host, root).refresh;
    let gesture = activate(&mut host, root, refresh).pop().unwrap();
    dispatch_ui(&binding, &mut frame, &mut host, root, gesture).await;
    assert!(!control(&mut host, root).needs_refresh());
    assert!(control(&mut host, root).is_open());
    assert_eq!(
        control(&mut host, root).editor().unwrap().input.text,
        "本地草稿"
    );
    assert_eq!(
        frame.unwrap().records().items[0].note.as_deref(),
        Some("本地草稿")
    );
    assert_eq!(service.writes().len(), count);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn closing_during_note_save_retains_row_failure_and_foreign_mount_ignores_completion() {
    let (runtime, binding, service) = setup(true).await;
    let initial = binding.read_feedback().await.unwrap();
    let rated = apply(prepare(
        &binding,
        &initial,
        &target(&binding),
        Change::ToggleRating(Rating::Positive),
    ))
    .await;
    let (mut host, root) = mounted(&binding, &rated);
    let note = ids(&mut host, root).edit_note;
    activate(&mut host, root, note);
    replace_note(&mut host, root, "不可串草稿");
    service.state.lock().unwrap().fault = Some(Fault::Remote(json!({"code":"rejected"})));
    let save = ids(&mut host, root).save;
    let gesture = activate(&mut host, root, save).pop().unwrap();
    let request = binding.prepare_feedback_gesture(Some(&rated), gesture);
    let editor = ids(&mut host, root).editor;
    let owner = control(&mut host, root).editor().unwrap() as *const _ as usize;
    assert!(!control(&mut host, root).editor().unwrap().input.disabled);
    replace_note(&mut host, root, "保存期间继续编辑");
    assert!(control(&mut host, root).is_pending());
    assert_eq!(ids(&mut host, root).editor, editor);
    assert_eq!(
        control(&mut host, root).editor().unwrap() as *const _ as usize,
        owner
    );
    assert_eq!(
        control(&mut host, root).editor().unwrap().input.text,
        "保存期间继续编辑"
    );
    let save = ids(&mut host, root).save;
    assert!(
        activate(&mut host, root, save).is_empty(),
        "editing during Save does not admit a second request"
    );
    let close = ids(&mut host, root).close;
    activate(&mut host, root, close);
    assert!(!control(&mut host, root).is_open());
    let FeedbackUiAdoption::Adopted(Some(frame)) =
        request.execute().await.apply(control(&mut host, root))
    else {
        panic!("known rejection")
    };
    assert!(!control(&mut host, root).is_open());
    assert!(!control(&mut host, root).notice().is_empty());
    let positive = ids(&mut host, root).positive;
    let gesture = activate(&mut host, root, positive).pop().unwrap();
    let request = binding.prepare_feedback_gesture(Some(&frame), gesture);
    let mut new_mount = TurnFeedback::new(190, Default::default());
    assert!(matches!(
        request.execute().await.apply(&mut new_mount),
        FeedbackUiAdoption::Ignored
    ));
    assert!(new_mount.state().is_none());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn removed_rating_conflict_closes_note_without_resubmit_and_readonly_capabilities_stay_disabled()
 {
    let (runtime, binding, service) = setup(true).await;
    let initial = binding.read_feedback().await.unwrap();
    let rated = apply(prepare(
        &binding,
        &initial,
        &target(&binding),
        Change::ToggleRating(Rating::Positive),
    ))
    .await;
    let (mut host, root) = mounted(&binding, &rated);
    let mut frame = Some(rated);
    let note = ids(&mut host, root).edit_note;
    activate(&mut host, root, note);
    replace_note(&mut host, root, "保留到冲突");
    service.state.lock().unwrap().records.clear();
    let save = ids(&mut host, root).save;
    let gesture = activate(&mut host, root, save).pop().unwrap();
    dispatch_ui(&binding, &mut frame, &mut host, root, gesture).await;
    assert!(!control(&mut host, root).is_open());
    assert!(frame.unwrap().records().items.is_empty());
    assert!(control(&mut host, root).notice().contains("别处"));
    runtime.stop().await.unwrap();
    let (runtime, binding, service) = setup(false).await;
    let frame = binding.read_feedback().await.unwrap();
    let (mut host, root) = mounted(&binding, &frame);
    let positive = ids(&mut host, root).positive;
    assert!(activate(&mut host, root, positive).is_empty());
    assert!(service.writes().is_empty());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn repeated_control_frames_retain_ids_and_invalid_completions_cannot_poison_pending_target() {
    let (runtime, binding, _) = setup(true).await;
    let frame = binding.read_feedback().await.unwrap();
    let state = frame.controls(&binding, &target(&binding)).unwrap();
    let (mut host, root) = mounted(&binding, &frame);
    let initial = ids(&mut host, root);
    control(&mut host, root).update(state.clone()).unwrap();
    assert_eq!(ids(&mut host, root).positive, initial.positive);
    let gesture = activate(&mut host, root, initial.positive).pop().unwrap();
    let request = binding.prepare_feedback_gesture(Some(&frame), gesture.clone());
    let mut bad = state;
    bad.publication += 1;
    bad.current = Some(
        serde_json::from_value(json!({"messageId":"foreign","rating":"positive","version":"v"}))
            .unwrap(),
    );
    assert!(!control(&mut host, root).complete(
        &gesture,
        tekes_agent_components_ui::FeedbackControlResult::Conflict(bad)
    ));
    assert!(control(&mut host, root).is_pending());
    assert!(control(&mut host, root).state().unwrap().current.is_none());
    assert!(matches!(
        request.execute().await.apply(control(&mut host, root)),
        FeedbackUiAdoption::Adopted(Some(_))
    ));
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn cold_feedback_control_can_explicitly_load_without_treating_unread_as_absent() {
    let (runtime, binding, service) = setup(true).await;
    let mut ui = TurnFeedback::new(190, Default::default());
    ui.bind_target(target(&binding)).unwrap();
    let root = ui.ids().root;
    let mut host = ui.into_host().unwrap();
    let positive = ids(&mut host, root).positive;
    assert!(activate(&mut host, root, positive).is_empty());
    assert!(control(&mut host, root).state().is_none());
    let refresh = ids(&mut host, root).refresh;
    let gesture = activate(&mut host, root, refresh).pop().unwrap();
    let mut frame = None;
    dispatch_ui(&binding, &mut frame, &mut host, root, gesture).await;
    assert!(frame.is_some());
    assert!(control(&mut host, root).state().unwrap().current.is_none());
    assert!(service.writes().is_empty());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn fresh_authoritative_read_can_close_removed_rating_while_old_save_is_inflight() {
    let (runtime, binding, service) = setup(true).await;
    let frame = binding.read_feedback().await.unwrap();
    let rated = apply(prepare(
        &binding,
        &frame,
        &target(&binding),
        Change::ToggleRating(Rating::Positive),
    ))
    .await;
    let (mut host, root) = mounted(&binding, &rated);
    let note = ids(&mut host, root).edit_note;
    activate(&mut host, root, note);
    replace_note(&mut host, root, "旧保存");
    let save = ids(&mut host, root).save;
    let gesture = activate(&mut host, root, save).pop().unwrap();
    let request = binding.prepare_feedback_gesture(Some(&rated), gesture);
    service.delay.store(true, Ordering::Release);
    let execution = tokio::spawn(request.execute());
    service.entered.notified().await;
    service.state.lock().unwrap().records.clear();
    let fresh = binding.read_feedback().await.unwrap();
    let state = fresh.controls(&binding, &target(&binding)).unwrap();
    control(&mut host, root).update(state.clone()).unwrap();
    assert!(!control(&mut host, root).is_open());
    service.release.notify_one();
    assert!(matches!(
        execution.await.unwrap().apply(control(&mut host, root)),
        FeedbackUiAdoption::Ignored
    ));
    assert_eq!(control(&mut host, root).state(), Some(&state));
    assert_eq!(service.writes().len(), 2);
    runtime.stop().await.unwrap();
}

use tekes_agent_ui_demo::component_demo::{ComponentDemo, DemoCommand};
fn transcript_control(view: &ComponentDemo) -> ComponentId {
    let root = view.transcript_content_id(view.ids()).unwrap();
    view.tree
        .get(root)
        .unwrap()
        .component::<tekes_agent_components_ui::TranscriptContent>()
        .unwrap()
        .feedback_controls()
        .next()
        .unwrap()
        .0
}
fn demo_control(view: &ComponentDemo, id: ComponentId) -> &TurnFeedback {
    view.tree
        .get(id)
        .unwrap()
        .component::<TurnFeedback>()
        .unwrap()
}
fn demo_press(view: &mut ComponentDemo, id: ComponentId) -> Vec<DemoCommand> {
    view.accessibility_action(id, None, AccessibilityAction::Press)
        .unwrap()
}
fn feedback_request(
    commands: Vec<DemoCommand>,
) -> tekes_agent_ui_demo::component_demo::FeedbackDemoRequest {
    let mut commands = commands.into_iter();
    let Some(DemoCommand::Feedback(request)) = commands.next() else {
        panic!("feedback command")
    };
    assert!(commands.next().is_none());
    *request
}
async fn complete_demo(view: &mut ComponentDemo, commands: Vec<DemoCommand>) {
    let request = feedback_request(commands);
    view.complete_feedback(request.execute().await).unwrap();
}
#[tokio::test]
async fn transcript_feedback_mounted_run_routes_ime_save_and_hidden_original_receipt_without_touching_other_draft()
 {
    let (runtime, a, service) = setup(true).await;
    let b = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "b"))
            .await
            .unwrap(),
    );
    let b_draft = b.current_run().unwrap().draft.clone();
    let mut view = ComponentDemo::new([a.clone(), b.clone()]).unwrap();
    view.enable_transcript_content().unwrap();
    let id = transcript_control(&view);
    assert!(demo_control(&view, id).state().is_none());
    assert!(
        view.tree
            .get(demo_control(&view, id).ids().refresh)
            .is_none()
    );
    let mut requests = view.take_initial_feedback_requests();
    assert_eq!(requests.len(), 1);
    view.complete_initial_feedback(requests.remove(0).execute().await)
        .unwrap();
    assert!(demo_control(&view, id).state().is_some());
    let positive = demo_control(&view, id).ids().positive;
    let commands = demo_press(&mut view, positive);
    complete_demo(&mut view, commands).await;
    let note = demo_control(&view, id).ids().edit_note;
    assert!(demo_press(&mut view, note).is_empty());
    let editor = demo_control(&view, id).ids().editor;
    view.tree.focus(Some(editor), true);
    assert!(
        view.event(&ContentEvent::Preedit {
            text: "ni".into(),
            selection: 2..2,
            clauses: vec![],
            replacement: None
        })
        .unwrap()
        .is_empty()
    );
    assert!(view.composing());
    assert!(view.select(1).is_err());
    let send = view.ids().send;
    assert!(demo_press(&mut view, send).is_empty());
    let save = demo_control(&view, id).ids().save;
    assert!(demo_press(&mut view, save).is_empty());
    assert_eq!(service.writes().len(), 1);
    assert!(
        view.event(&ContentEvent::Commit {
            text: "中文备注".into(),
            replacement: None
        })
        .unwrap()
        .is_empty()
    );
    assert!(!view.composing());
    let save = demo_control(&view, id).ids().save;
    let request = feedback_request(demo_press(&mut view, save));
    view.select(1).unwrap();
    assert_eq!(view.selected(), 1);
    let other = transcript_control(&view);
    assert!(demo_control(&view, other).state().is_none());
    view.complete_feedback(request.execute().await).unwrap();
    assert_eq!(view.selected(), 1);
    assert!(demo_control(&view, other).state().is_none());
    assert_eq!(b.current_run().unwrap().draft, b_draft);
    view.select(0).unwrap();
    assert_eq!(transcript_control(&view), id);
    let state = demo_control(&view, id).state().unwrap();
    assert_eq!(
        state.current.as_ref().unwrap().note.as_deref(),
        Some("中文备注")
    );
    assert!(!demo_control(&view, id).is_open());
    let note = demo_control(&view, id).ids().edit_note;
    assert!(
        view.tree
            .get(note)
            .unwrap()
            .component::<tekes_ui_controls::composition::ButtonContent>()
            .unwrap()
            .button
            .state
            .selected,
        "committed note presence uses upstream selected semantics"
    );
    assert_eq!(service.writes().len(), 2);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn delayed_control_completion_cannot_adopt_a_frame_superseded_after_execute() {
    let (runtime, binding, service) = setup(true).await;
    let initial = binding.read_feedback().await.unwrap();
    let (mut host, root) = mounted(&binding, &initial);
    let positive = ids(&mut host, root).positive;
    let gesture = activate(&mut host, root, positive).pop().unwrap();
    let completion = binding
        .prepare_feedback_gesture(Some(&initial), gesture)
        .execute()
        .await;
    let fresh = binding.read_feedback().await.unwrap();
    assert!(fresh.records().items.len() == 1);
    assert!(matches!(
        completion.apply(control(&mut host, root)),
        FeedbackUiAdoption::Adopted(None)
    ));
    assert!(control(&mut host, root).needs_refresh());
    assert_eq!(service.writes().len(), 1);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn finished_feedback_ui_receipts_do_not_retain_a_closed_binding_or_its_history_lease() {
    let (runtime, binding, _) = setup(true).await;
    let frame = binding.read_feedback().await.unwrap();
    let (mut host, root) = mounted(&binding, &frame);
    let positive = ids(&mut host, root).positive;
    let gesture = activate(&mut host, root, positive).pop().unwrap();
    let completion = binding
        .prepare_feedback_gesture(Some(&frame), gesture)
        .execute()
        .await;
    let weak = Arc::downgrade(&binding);
    drop(binding);
    assert!(
        weak.upgrade().is_none(),
        "a completed UI receipt must not own the closed RunBinding"
    );
    assert!(matches!(
        completion.apply(control(&mut host, root)),
        FeedbackUiAdoption::Adopted(None)
    ));
    assert!(control(&mut host, root).needs_refresh());
    runtime.stop().await.unwrap();
}
