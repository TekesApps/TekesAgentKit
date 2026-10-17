use async_trait::async_trait;
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
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
        EndpointCapabilities::methods(&["session.prompt", "session.create", "commands/run"])
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
    async fn call(&self, method: &str, _: Value, _: &str) -> Result<Value> {
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
