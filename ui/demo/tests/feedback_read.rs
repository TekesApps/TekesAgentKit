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
