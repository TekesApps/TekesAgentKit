use async_trait::async_trait;
use serde_json::Value;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use tekes_agent_components_ui::RunSurface;
use tekes_agent_contract::*;
use tekes_agent_presentation::{
    ComposerDestination, RunController, RunScope, RunSnapshot, TranscriptItemKind,
};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_demo::fixture::Fixture;

async fn until(controller: &RunController, predicate: impl Fn(&RunSnapshot) -> bool) {
    let mut changes = controller.subscribe();
    tokio::time::timeout(Duration::from_secs(4), async {
        while !predicate(&controller.snapshot()) {
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}

// Test-only fault injection delegates all authority and mutation behavior to the real fixture.
struct FailFirstPage {
    fixture: Fixture,
    requests: Arc<Mutex<Vec<HistoryRequest>>>,
}
struct FaultEndpoint {
    inner: Arc<dyn SessionEndpoint>,
    requests: Arc<Mutex<Vec<HistoryRequest>>>,
}
#[async_trait]
impl EndpointConnector for FailFirstPage {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        Ok(Arc::new(FaultEndpoint {
            inner: self.fixture.connect().await?,
            requests: self.requests.clone(),
        }))
    }
}
#[async_trait]
impl SessionEndpoint for FaultEndpoint {
    fn identity(&self) -> &str {
        self.inner.identity()
    }
    fn host(&self) -> &HostDescription {
        self.inner.host()
    }
    fn capabilities(&self) -> EndpointCapabilities {
        self.inner.capabilities()
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        self.inner.follow(target).await
    }
    async fn history_page(&self, request: HistoryRequest) -> Result<HistoryPage> {
        let fail = {
            let mut requests = self.requests.lock().unwrap();
            requests.push(request.clone());
            requests.len() == 1
        };
        if fail {
            return Err(Error::Timeout);
        }
        self.inner.history_page(request).await
    }
    async fn respond(&self, id: &str, revision: i64, outcome: Value) -> Result<()> {
        self.inner.respond(id, revision, outcome).await
    }
    async fn call(&self, method: &str, payload: Value, rpc: &str) -> Result<Value> {
        self.inner.call(method, payload, rpc).await
    }
    async fn close(&self) {
        self.inner.close().await;
    }
}

#[tokio::test]
async fn failed_earlier_keeps_the_visible_prefix_and_retry_uses_the_same_frozen_cut() {
    let requests = Arc::new(Mutex::new(vec![]));
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(FailFirstPage {
            fixture: Fixture::default(),
            requests: requests.clone(),
        }),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let controller = RunController::new(runtime.clone()).unwrap();
    let scope = RunScope::main("fixture", "a");
    controller.open(scope.clone()).await.unwrap();
    until(&controller, |s| {
        !s.transcript.is_stale && s.transcript.has_more_before
    })
    .await;
    let destination = ComposerDestination::run(scope.clone());
    let mut draft = controller.snapshot().draft.clone();
    draft.text = "分页失败也要保留草稿".into();
    controller.edit(&destination, draft).unwrap();
    let before = controller.snapshot();
    let mut surface = RunSurface::new();
    surface.sync(&before).unwrap();
    assert!(matches!(
        controller.load_earlier(&scope).await,
        Err(Error::Timeout)
    ));
    let failed = controller.snapshot();
    assert_eq!(failed.transcript, before.transcript);
    assert_eq!(failed.draft, before.draft);
    assert!(!failed.transcript.is_stale);
    surface.sync(&failed).unwrap();
    assert_eq!(surface.editor().input.text, before.draft.text);

    // No automatic retry; the explicit retry must retain both cut and frontier.
    assert_eq!(requests.lock().unwrap().len(), 1);
    assert!(controller.load_earlier(&scope).await.unwrap());
    until(&controller, |s| {
        s.transcript.items.len() > before.transcript.items.len()
    })
    .await;
    let requests = requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].through_sequence, requests[1].through_sequence);
    assert_eq!(requests[0].before_sequence, requests[1].before_sequence);
    let after = controller.snapshot();
    surface.sync(&after).unwrap();
    assert_eq!(after.draft, before.draft);
    assert_eq!(surface.editor().input.text, before.draft.text);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn closing_a_notification_target_during_stream_does_not_stop_run_settlement() {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    until(&controller, |s| s.can_submit).await;
    let destination = ComposerDestination::run(RunScope::main("fixture", "a"));
    let mut draft = controller.snapshot().draft.clone();
    draft.text = "关闭观察者后仍完成本次输入".into();
    controller.edit(&destination, draft).unwrap();
    let ticket = controller.prepare(&destination, PromptMode::Queue).unwrap();
    let live = Arc::new(AtomicBool::new(true));
    let target = live.clone();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let forwarder = tokio::spawn(tekes_agent_ui_demo::notifications::forward(
        controller.subscribe(),
        move || {
            if !target.load(Ordering::SeqCst) {
                return false;
            }
            tx.send(()).is_ok()
        },
    ));
    controller.dispatch(ticket, None).await.unwrap();
    until(&controller, |s| {
        s.transcript.items.iter().any(|i| {
            i.kind == TranscriptItemKind::Assistant && i.is_provisional && !i.text.is_empty()
        })
    })
    .await;
    live.store(false, Ordering::SeqCst);
    tokio::time::timeout(Duration::from_secs(3), forwarder)
        .await
        .unwrap()
        .unwrap();
    while rx.recv().await.is_some() {}
    until(&controller, |s| {
        s.run.as_ref().is_some_and(|r| !r.can_cancel)
            && !s.transcript.items.iter().any(|i| i.is_provisional)
    })
    .await;
    assert_eq!(
        controller
            .snapshot()
            .transcript
            .items
            .iter()
            .filter(|i| i.text == "关闭观察者后仍完成本次输入")
            .count(),
        1
    );
    assert!(rx.try_recv().is_err());
    runtime.stop().await.unwrap();
}
