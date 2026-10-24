use async_trait::async_trait;
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tekes_agent_components_ui::{Composer, ComposerAffordances, ComposerIds};
use tekes_agent_contract::*;
use tekes_agent_presentation::*;
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{PaneAccess, PaneSnapshot, RunBinding};
use tekes_agent_ui_demo::fixture::Fixture;
use tokio::sync::Notify;

#[derive(Clone, Default)]
struct Connector {
    fixture: Fixture,
    hold_cancel: Arc<AtomicBool>,
    entered: Arc<Notify>,
    release: Arc<Notify>,
    prompts: Arc<AtomicUsize>,
}
struct Endpoint {
    inner: Arc<dyn SessionEndpoint>,
    connector: Connector,
}
#[async_trait]
impl EndpointConnector for Connector {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        Ok(Arc::new(Endpoint {
            inner: self.fixture.connect().await?,
            connector: self.clone(),
        }))
    }
}
#[async_trait]
impl SessionEndpoint for Endpoint {
    fn identity(&self) -> &str {
        self.inner.identity()
    }
    fn host(&self) -> &HostDescription {
        self.inner.host()
    }
    fn capabilities(&self) -> EndpointCapabilities {
        let mut caps = self.inner.capabilities();
        caps.methods
            .extend(["session.fork".into(), "session.discard".into()]);
        caps.supports_ephemeral_fork = true;
        caps
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        self.inner.follow(target).await
    }
    async fn history_page(&self, request: HistoryRequest) -> Result<HistoryPage> {
        self.inner.history_page(request).await
    }
    async fn respond(&self, session: &str, id: i64, response: Value) -> Result<()> {
        self.inner.respond(session, id, response).await
    }
    async fn call(&self, method: &str, params: Value, rpc: &str) -> Result<Value> {
        if method == "session.prompt" {
            self.connector.prompts.fetch_add(1, Ordering::SeqCst);
        }
        match method {
            "session.fork" => Ok(json!({"sessionId":"b"})),
            "session.discard" => Ok(json!({"discarded":true})),
            "session.cancel" if self.connector.hold_cancel.load(Ordering::SeqCst) => {
                self.connector.entered.notify_one();
                self.connector.release.notified().await;
                Ok(json!({"accepted":true}))
            }
            _ => self.inner.call(method, params, rpc).await,
        }
    }
    async fn close(&self) {
        self.inner.close().await;
    }
}
async fn setup() -> (Connector, Arc<SessionRuntime>) {
    let connector = Connector::default();
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(connector.clone()),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    (connector, runtime)
}
async fn run_snapshot(binding: &RunBinding) -> Arc<RunSnapshot> {
    match binding.snapshot().await.unwrap() {
        PaneSnapshot::Run { snapshot, .. } => snapshot,
        _ => panic!("expected canonical Run snapshot"),
    }
}
#[tokio::test]
async fn mounted_main_sources_keep_independent_leases_and_page_existing_presentation() {
    let (_, runtime) = setup().await;
    let scope = RunScope::main("fixture", "a");
    let first = RunBinding::open_main(runtime.clone(), scope.clone())
        .await
        .unwrap();
    let second = RunBinding::open_main(runtime.clone(), scope).await.unwrap();
    let original = run_snapshot(&second).await.transcript.items.len();
    assert!(second.subscribe_run().is_some());
    assert!(second.subscribe_side_chat().is_none());
    let mut changes = second.subscribe_run().unwrap();
    drop(first);
    assert!(second.load_earlier().await.unwrap());
    tokio::time::timeout(Duration::from_secs(2), async {
        while run_snapshot(&second).await.transcript.items.len() <= original {
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert!(runtime.inspect(|s| s.sessions["a"].journal_ready));
    drop(second);
    tokio::time::timeout(Duration::from_secs(2), async {
        while runtime.inspect(|s| s.sessions["a"].journal_ready) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn sidechat_cancellation_publishes_unknown_and_confirmed_absence_removes_body() {
    let (connector, runtime) = setup().await;
    let owner = Arc::new(SideChatController::new(runtime.clone()));
    let chat = owner
        .create(RunScope::main("fixture", "a"), None, "fork")
        .await
        .unwrap();
    let binding = RunBinding::open_side_chat(runtime.clone(), owner.clone(), &chat)
        .await
        .unwrap();
    let (_, other_runtime) = setup().await;
    assert!(
        RunBinding::open_side_chat(other_runtime.clone(), owner.clone(), &chat)
            .await
            .is_err(),
        "same endpoint ID cannot substitute another authority"
    );
    other_runtime.stop().await.unwrap();
    let mut notifications = binding.subscribe_side_chat().unwrap();
    assert_eq!(binding.access(), PaneAccess::Ready);
    let mut foreign = chat.clone();
    foreign.parent = RunScope::main("fixture", "other");
    assert!(
        RunBinding::open_side_chat(runtime.clone(), owner.clone(), &foreign)
            .await
            .is_err()
    );
    connector.hold_cancel.store(true, Ordering::SeqCst);
    let close_owner = owner.clone();
    let closing = tokio::spawn(async move { close_owner.close("b", "close").await });
    connector.entered.notified().await;
    notifications.changed().await.unwrap();
    notifications.borrow_and_update();
    assert_eq!(binding.access(), PaneAccess::Closing);
    assert!(!binding.access().composer_gate().send);
    closing.abort();
    let _ = closing.await;
    notifications.changed().await.unwrap();
    notifications.borrow_and_update();
    assert_eq!(binding.access(), PaneAccess::Unknown);
    let destination = ComposerDestination::run(chat.scope.clone());
    binding.edit_text(&destination, "保留草稿".into()).unwrap();
    assert!(
        binding
            .send_with(&destination, PromptMode::Queue, &Preparation)
            .await
            .is_err()
    );
    assert!(binding.cancel(&destination).await.is_err());
    let unknown = owner.snapshot().chats["b"].clone();
    assert!(!owner.reconcile(&unknown, Err("offline".into())));
    assert_eq!(binding.access(), PaneAccess::Unknown);
    assert!(!notifications.has_changed().unwrap());
    assert!(owner.reconcile(&unknown, Ok(false)));
    assert!(matches!(
        binding.snapshot().await.unwrap(),
        PaneSnapshot::Missing { .. }
    ));
    assert!(binding.load_earlier().await.is_err());
    assert!(
        RunBinding::open_side_chat(runtime.clone(), owner, &chat)
            .await
            .is_err()
    );
    runtime.stop().await.unwrap();
}

struct Preparation;
#[async_trait]
impl SubmissionPreparation for Preparation {
    async fn prepare(
        &self,
        _: &SessionRuntime,
        _: &PreparedSubmission,
        _: &RunScope,
    ) -> Result<()> {
        Ok(())
    }
}

struct RemoveDuringPreparation {
    owner: Arc<SideChatController>,
    expected: SideChat,
}
#[async_trait]
impl SubmissionPreparation for RemoveDuringPreparation {
    async fn prepare(
        &self,
        _: &SessionRuntime,
        _: &PreparedSubmission,
        _: &RunScope,
    ) -> Result<()> {
        assert!(self.owner.reconcile(&self.expected, Ok(false)));
        Ok(())
    }
}
#[tokio::test]
async fn pointer_removed_during_host_preparation_never_reaches_prompt_admission() {
    let (connector, runtime) = setup().await;
    let owner = Arc::new(SideChatController::new(runtime.clone()));
    let chat = owner
        .create(RunScope::main("fixture", "a"), None, "fork")
        .await
        .unwrap();
    let binding = RunBinding::open_side_chat(runtime.clone(), owner.clone(), &chat)
        .await
        .unwrap();
    let target = ComposerDestination::run(chat.scope.clone());
    binding
        .edit_text(&target, "must not submit".into())
        .unwrap();
    assert!(
        binding
            .send_with(
                &target,
                PromptMode::Queue,
                &RemoveDuringPreparation {
                    owner,
                    expected: chat
                }
            )
            .await
            .is_err()
    );
    assert_eq!(binding.access(), PaneAccess::Missing);
    assert_eq!(connector.prompts.load(Ordering::SeqCst), 0);
    runtime.stop().await.unwrap();
}
struct BoundedSource;
#[async_trait]
impl ChildHistorySource for BoundedSource {
    async fn read(&self, parent: &RunScope, child: &str) -> Result<TranscriptSnapshot> {
        assert_eq!(parent.session_id, "a");
        assert_eq!(child, "bounded");
        Ok(TranscriptSnapshot {
            diagnostics: vec!["explicit bounded projection".into()],
            ..Default::default()
        })
    }
}
#[tokio::test]
async fn bounded_child_uses_explicit_projection_and_never_admits_main_run_controls() {
    let (_, runtime) = setup().await;
    let binding = RunBinding::open_child(
        runtime.clone(),
        RunScope::main("fixture", "a"),
        "bounded",
        ChildHistoryLayout::SameSession,
        Some(Arc::new(BoundedSource)),
    )
    .await
    .unwrap();
    match binding.snapshot().await.unwrap() {
        PaneSnapshot::ReadOnlyChild {
            transcript, scope, ..
        } => {
            assert_eq!(scope.session_id, "bounded");
            assert!(
                transcript.items.is_empty(),
                "must not show parent fixture messages"
            );
            assert_eq!(transcript.diagnostics, ["explicit bounded projection"]);
        }
        _ => panic!("bounded projection must be read-only"),
    }
    assert!(binding.subscribe_run().is_none());
    assert!(
        !runtime.inspect(|s| s.sessions.contains_key("bounded")),
        "must not open bounded ID as ordinary journal"
    );
    let target = ComposerDestination::run(binding.scope().clone());
    assert!(binding.edit_text(&target, "wrong source".into()).is_err());
    assert!(
        binding
            .send_with(&target, PromptMode::Queue, &Preparation)
            .await
            .is_err()
    );
    assert!(binding.cancel(&target).await.is_err());
    assert!(
        binding
            .sync_composer(
                &mut Composer::new(ComposerIds::new(5000)),
                ComposerAffordances::default()
            )
            .is_err()
    );
    assert!(
        binding.load_earlier().await.is_err(),
        "custom source owns its paging, no parent fallback"
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn main_binding_dispatches_existing_controller_and_observes_stream_and_cancel() {
    let (_, runtime) = setup().await;
    let binding = RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
        .await
        .unwrap();
    let target = ComposerDestination::run(binding.scope().clone());
    assert!(
        binding
            .edit_text(
                &ComposerDestination::run(RunScope::main("fixture", "b")),
                "wrong".into()
            )
            .is_err()
    );
    binding.edit_text(&target, "组件绑定测试".into()).unwrap();
    let mut changes = binding.subscribe_run().unwrap();
    binding
        .send_with(&target, PromptMode::Queue, &Preparation)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let snapshot = run_snapshot(&binding).await;
            if snapshot.run.as_ref().is_some_and(|r| r.can_cancel)
                && snapshot.transcript.items.iter().any(|i| i.is_provisional)
            {
                break;
            }
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    binding.cancel(&target).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while run_snapshot(&binding)
            .await
            .transcript
            .items
            .iter()
            .any(|i| i.is_provisional)
        {
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    let snapshot = run_snapshot(&binding).await;
    assert_eq!(
        snapshot
            .transcript
            .items
            .iter()
            .filter(|i| i.text == "组件绑定测试")
            .count(),
        1
    );
    assert!(snapshot.draft.text.is_empty());
    runtime.stop().await.unwrap();
}
