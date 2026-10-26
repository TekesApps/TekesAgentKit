use async_trait::async_trait;
use serde_json::Value;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tekes_agent_contract::{
    EndpointCapabilities, EndpointConnector, FrameStream, HistoryPage, HistoryRequest,
    HostDescription, PromptMode, Result, SessionEndpoint, StreamTarget,
};
use tekes_agent_presentation::{
    AgentTask, ChildHistoryLayout, ComposerDestination, PreparedSubmission, RunController,
    RunScope, SubmissionPreparation,
};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{ChildPaneBinding, PaneAccess, RunBinding, TaskPaneBinding};
use tekes_agent_ui_demo::fixture::Fixture;
use tokio::sync::Notify;

#[derive(Default)]
struct CatalogGate {
    held: AtomicBool,
    entered: Notify,
    release: Notify,
    prompts: AtomicUsize,
    requests: Mutex<Vec<(String, Value, String)>>,
}

struct GatedConnector {
    fixture: Fixture,
    gate: Arc<CatalogGate>,
    connections: Arc<AtomicUsize>,
    remove_prompt_after_first_connection: bool,
}

struct GatedEndpoint {
    inner: Arc<dyn SessionEndpoint>,
    gate: Arc<CatalogGate>,
    remove_prompt: bool,
}

#[async_trait]
impl EndpointConnector for GatedConnector {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        let connection = self.connections.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(GatedEndpoint {
            inner: self.fixture.connect().await?,
            gate: self.gate.clone(),
            remove_prompt: self.remove_prompt_after_first_connection && connection > 0,
        }))
    }
}

#[async_trait]
impl SessionEndpoint for GatedEndpoint {
    fn identity(&self) -> &str {
        self.inner.identity()
    }
    fn host(&self) -> &HostDescription {
        self.inner.host()
    }
    fn capabilities(&self) -> EndpointCapabilities {
        let mut capabilities = self.inner.capabilities();
        if self.remove_prompt {
            capabilities.methods.remove("session.prompt");
        }
        capabilities
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        self.inner.follow(target).await
    }
    async fn history_page(&self, request: HistoryRequest) -> Result<HistoryPage> {
        self.inner.history_page(request).await
    }
    async fn call(&self, method: &str, params: Value, rpc_id: &str) -> Result<Value> {
        if method == "subagents.list" && !self.gate.held.swap(true, Ordering::SeqCst) {
            self.gate.entered.notify_one();
            self.gate.release.notified().await;
        }
        if matches!(method, "subagents.prompt" | "session.prompt") {
            self.gate.prompts.fetch_add(1, Ordering::SeqCst);
            self.gate
                .requests
                .lock()
                .unwrap()
                .push((method.into(), params.clone(), rpc_id.into()));
        }
        self.inner.call(method, params, rpc_id).await
    }
    async fn respond(&self, session_id: &str, id: i64, response: Value) -> Result<()> {
        self.inner.respond(session_id, id, response).await
    }
    async fn close(&self) {
        self.inner.close().await
    }
}

struct Prepare;
#[async_trait]
impl SubmissionPreparation for Prepare {
    async fn prepare(
        &self,
        _: &SessionRuntime,
        _: &PreparedSubmission,
        _: &RunScope,
    ) -> Result<()> {
        Ok(())
    }
}

struct HoldPrepare {
    entered: Arc<Notify>,
    release: Arc<Notify>,
}

#[async_trait]
impl SubmissionPreparation for HoldPrepare {
    async fn prepare(
        &self,
        _: &SessionRuntime,
        _: &PreparedSubmission,
        _: &RunScope,
    ) -> Result<()> {
        self.entered.notify_one();
        self.release.notified().await;
        Ok(())
    }
}

fn task(session_id: &str) -> AgentTask {
    AgentTask {
        id: "task-owner".into(),
        scope: RunScope {
            endpoint_id: "fixture".into(),
            thread_id: "a".into(),
            session_id: session_id.into(),
        },
        name: "Child task".into(),
        status: "running".into(),
        kind: "task".into(),
    }
}

#[tokio::test]
async fn retired_task_owner_after_child_catalog_cannot_dispatch_prompt() {
    let fixture = Fixture::default().with_children();
    let gate = Arc::new(CatalogGate::default());
    gate.held.store(true, Ordering::SeqCst);
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(GatedConnector {
            fixture,
            gate: gate.clone(),
            connections: Arc::new(AtomicUsize::new(0)),
            remove_prompt_after_first_connection: false,
        }),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let main = RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
        .await
        .unwrap();
    let child = ChildPaneBinding::for_run(
        &main,
        "child-a",
        ChildHistoryLayout::IndependentSessions,
        None,
    )
    .await
    .unwrap();
    child.refresh_controls().await.unwrap();
    let owner = TaskPaneBinding::for_run(child.binding());
    let token = owner
        .begin(child.binding().scope().clone(), "task-owner")
        .unwrap();
    assert!(owner.commit(&token, Ok(Some(task("child-a")))));
    let binding = owner.open_resolved().await.unwrap().unwrap();
    let destination = ComposerDestination::run(binding.scope().clone());
    binding
        .edit_text(&destination, "原 owner 的中文草稿".into())
        .unwrap();
    let tail = runtime.inspect(|state| state.sessions["child-a"].tail_sequence);
    gate.held.store(false, Ordering::SeqCst);

    let send_binding = binding.clone();
    let prepare = Prepare;
    let send = tokio::spawn(async move {
        send_binding
            .send_with(&destination, PromptMode::Queue, &prepare)
            .await
    });
    tokio::time::timeout(Duration::from_secs(3), gate.entered.notified())
        .await
        .unwrap();
    assert!(
        owner.commit(&token, Ok(None)),
        "the original task pointer is retired while catalog is held"
    );
    gate.release.notify_one();

    assert!(
        send.await.unwrap().is_err(),
        "dispatch must recheck owner after the catalog await"
    );
    assert_eq!(
        gate.prompts.load(Ordering::SeqCst),
        0,
        "no prompt RPC may be emitted for the retired owner"
    );
    assert_eq!(
        runtime.inspect(|state| state.sessions["child-a"].tail_sequence),
        tail
    );
    assert_eq!(
        child.binding().current_run().unwrap().draft.text,
        "原 owner 的中文草稿"
    );
    assert_eq!(binding.access(), PaneAccess::Missing);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn prepared_main_run_survives_reconnect_and_other_destination_edit() {
    let gate = Arc::new(CatalogGate::default());
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(GatedConnector {
            fixture: Fixture::default(),
            gate: gate.clone(),
            connections: Arc::new(AtomicUsize::new(0)),
            remove_prompt_after_first_connection: false,
        }),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    runtime.open_session("a").await.unwrap();
    runtime.open_session("b").await.unwrap();
    let controller = Arc::new(RunController::new(runtime.clone()).unwrap());
    let first = RunScope::main("fixture", "a");
    let other = RunScope::main("fixture", "b");
    let first_destination = ComposerDestination::run(first.clone());
    let other_destination = ComposerDestination::run(other.clone());
    controller.select(first_destination.clone()).unwrap();
    controller
        .edit_text(&first_destination, "重连后首次发送".into())
        .unwrap();
    let before = runtime.inspect(|state| state.sessions["a"].tail_sequence);
    let ticket = controller
        .prepare(&first_destination, PromptMode::Queue)
        .unwrap();
    let rpc_id = ticket.id().to_owned();
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let prepare = HoldPrepare {
        entered: entered.clone(),
        release: release.clone(),
    };
    let dispatch_controller = controller.clone();
    let dispatch = tokio::spawn(async move {
        dispatch_controller
            .dispatch_with(ticket, None, &prepare)
            .await
    });
    tokio::time::timeout(Duration::from_secs(3), entered.notified())
        .await
        .unwrap();

    runtime.reconnect().await.unwrap();
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    controller.select(other_destination.clone()).unwrap();
    controller
        .edit_text(&other_destination, "另一会话草稿保留".into())
        .unwrap();
    release.notify_one();

    assert!(dispatch.await.unwrap().is_ok());
    let calls = gate.requests.lock().unwrap().clone();
    assert_eq!(
        calls.len(),
        1,
        "the original captured submission is sent only once"
    );
    assert_eq!(calls[0].0, "session.prompt");
    assert_eq!(calls[0].1["sessionId"], "a");
    assert_eq!(calls[0].2, rpc_id);
    assert!(runtime.inspect(|state| state.sessions["a"].tail_sequence) > before);
    assert_eq!(controller.snapshot().draft.text, "另一会话草稿保留");
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn reconnect_without_prompt_capability_never_emits_prompt_rpc() {
    let fixture = Fixture::default();
    let gate = Arc::new(CatalogGate::default());
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(GatedConnector {
            fixture,
            gate: gate.clone(),
            connections: Arc::new(AtomicUsize::new(0)),
            remove_prompt_after_first_connection: true,
        }),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    runtime.open_session("a").await.unwrap();
    let controller = Arc::new(RunController::new(runtime.clone()).unwrap());
    let scope = RunScope::main("fixture", "a");
    let destination = ComposerDestination::run(scope.clone());
    controller.select(destination.clone()).unwrap();
    controller
        .edit_text(&destination, "等待新连接能力确认".into())
        .unwrap();
    let before = runtime.inspect(|state| state.sessions["a"].tail_sequence);
    let ticket = controller.prepare(&destination, PromptMode::Queue).unwrap();
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let prepare = HoldPrepare {
        entered: entered.clone(),
        release: release.clone(),
    };
    let dispatch_controller = controller.clone();
    let dispatch = tokio::spawn(async move {
        dispatch_controller
            .dispatch_with(ticket, None, &prepare)
            .await
    });
    tokio::time::timeout(Duration::from_secs(3), entered.notified())
        .await
        .unwrap();

    runtime.reconnect().await.unwrap();
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    release.notify_one();

    assert!(dispatch.await.unwrap().is_err());
    assert_eq!(gate.prompts.load(Ordering::SeqCst), 0);
    assert_eq!(controller.snapshot().draft.text, "等待新连接能力确认");
    assert_eq!(
        runtime.inspect(|state| state.sessions["a"].tail_sequence),
        before
    );
    runtime.stop().await.unwrap();
}
