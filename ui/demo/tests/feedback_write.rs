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
