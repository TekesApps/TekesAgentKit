use async_trait::async_trait;
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tekes_agent_contract::*;
use tekes_agent_presentation::{
    PreparedSubmission, RunScope, SubmissionPhase, SubmissionPreparation,
};
use tekes_agent_session::*;
use tekes_agent_ui_bindings::{RunBinding, StandaloneComposerBinding};
use tokio::sync::mpsc;

const ENDPOINT: &str = "standalone-composer-fixture";
const SESSION: &str = "10000000-0000-4000-8000-000000000001";
const OTHER_SESSION: &str = "20000000-0000-4000-8000-000000000002";

#[derive(Default)]
struct Fixture {
    senders: Arc<Mutex<Vec<mpsc::UnboundedSender<Value>>>>,
    calls: Arc<Mutex<Vec<String>>>,
    delayed: Arc<AtomicBool>,
    fail: Arc<AtomicBool>,
    gate: Arc<tokio::sync::Notify>,
}

struct FixtureEndpoint {
    fixture: Arc<Fixture>,
    host: HostDescription,
}

#[async_trait]
impl EndpointConnector for Fixture {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        Ok(Arc::new(FixtureEndpoint {
            fixture: Arc::new(Fixture {
                senders: self.senders.clone(),
                calls: self.calls.clone(),
                delayed: self.delayed.clone(),
                fail: self.fail.clone(),
                gate: self.gate.clone(),
            }),
            host: serde_json::from_value(json!({
                "protocolVersion":4,
                "product":{"name":"standalone-composer-fixture","version":"1"},
                "capabilities":["session-journal","session-inventory-sync","session-control-sync","actionable-sync"],
                "cwd":"/fixture","home":"/fixture","attachedSessions":0,"canOpenPath":false
            }))?,
        }))
    }
}

#[async_trait]
impl SessionEndpoint for FixtureEndpoint {
    fn identity(&self) -> &str {
        ENDPOINT
    }
    fn host(&self) -> &HostDescription {
        &self.host
    }
    fn capabilities(&self) -> EndpointCapabilities {
        EndpointCapabilities::methods(&["session.prompt"])
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        let frame = match target {
            StreamTarget::WorkspaceInventory => {
                json!({"type":"baseline","generation":"w1","snapshot":{"items":[]}})
            }
            StreamTarget::SessionInventory => {
                json!({"type":"baseline","generation":"i1","snapshot":{"items":[{"sessionId":SESSION,"status":"idle","running":false,"blank":false,"updatedAt":1},{"sessionId":OTHER_SESSION,"status":"idle","running":false,"blank":false,"updatedAt":1}]}})
            }
            StreamTarget::SessionControl => {
                json!({"type":"baseline","generation":"c1","snapshot":{"queues":[],"jobs":[],"projections":[]}})
            }
            StreamTarget::Actionables => json!({"type":"reset","generation":"a1"}),
            StreamTarget::SessionJournal { address, .. } => {
                json!({"type":"snapshot","snapshot":{"sessionId":address.session_id,"generation":"j1","throughSequence":-1,"windowLimit":50,"entries":[],"hasMoreBefore":false}})
            }
            _ => return Err(unsupported("unexpected fixture stream")),
        };
        let (tx, rx) = mpsc::unbounded_channel();
        self.fixture.senders.lock().unwrap().push(tx);
        Ok(Box::pin(
            stream::once(async move { Ok(frame) })
                .chain(stream::unfold(rx, |mut rx| async move {
                    rx.recv().await.map(|frame| (Ok(frame), rx))
                }))
                .boxed(),
        ))
    }
    async fn history_page(&self, request: HistoryRequest) -> Result<HistoryPage> {
        Ok(HistoryPage {
            session_id: request.session_id,
            through_sequence: request.through_sequence,
            entries: vec![],
            has_more_before: false,
        })
    }
    async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
        Ok(())
    }
    async fn call(&self, method: &str, _: Value, rpc_id: &str) -> Result<Value> {
        assert_eq!(method, "session.prompt");
        self.fixture.calls.lock().unwrap().push(rpc_id.into());
        if self.fixture.delayed.load(Ordering::Acquire) {
            self.fixture.gate.notified().await;
        }
        if self.fixture.fail.load(Ordering::Acquire) {
            return Err(Error::Timeout);
        }
        Ok(json!({"accepted":true}))
    }
    async fn close(&self) {}
}

async fn open() -> (Arc<SessionRuntime>, Arc<RunBinding>) {
    let (runtime, run, _) = open_fixture().await;
    (runtime, run)
}
async fn open_fixture() -> (Arc<SessionRuntime>, Arc<RunBinding>, Arc<Fixture>) {
    let fixture = Arc::new(Fixture::default());
    let runtime = Arc::new(SessionRuntime::spawn(ENDPOINT, fixture.clone()));
    runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
    let run = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main(ENDPOINT, SESSION))
            .await
            .unwrap(),
    );
    (runtime, run, fixture)
}

struct NoSettings;
#[async_trait]
impl SubmissionPreparation for NoSettings {
    async fn prepare(
        &self,
        _: &SessionRuntime,
        _: &PreparedSubmission,
        _: &RunScope,
    ) -> Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn dispatched_result_keeps_original_identity_after_detach_without_resending() {
    let (runtime, run, fixture) = open_fixture().await;
    fixture.delayed.store(true, Ordering::Release);
    fixture.fail.store(true, Ordering::Release);
    let composer = StandaloneComposerBinding::open(run.clone()).unwrap();
    let first = Arc::new(composer.mount().unwrap());
    first.edit_text("原始发送".into()).unwrap();
    let ticket = first.prepare_send(PromptMode::Queue).unwrap();
    let id = ticket.id().to_owned();
    let sending = first.clone();
    let dispatch =
        tokio::spawn(async move { sending.dispatch_prepared_with(ticket, &NoSettings).await });
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if !fixture.calls.lock().unwrap().is_empty() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    first.detach();
    let second = first.reattach().unwrap();
    second.edit_text("后续草稿".into()).unwrap();
    assert!(second.prepare_send(PromptMode::Queue).is_err());
    fixture.gate.notify_one();
    assert!(dispatch.await.unwrap().is_err());
    let submissions = second.submissions().unwrap();
    assert_eq!(submissions.len(), 1);
    assert_eq!(submissions[0].id, id);
    assert_eq!(submissions[0].phase, SubmissionPhase::Unknown);
    assert_eq!(second.draft().unwrap().text, "后续草稿");
    assert_eq!(fixture.calls.lock().unwrap().as_slice(), &[id.clone()]);
    // The explicit host reconciliation callback is exercised here against a
    // synthetic peer; this is not evidence of actual harness history recovery.
    assert!(second.acknowledge_reconciliation("another-run-id").is_err());
    second.acknowledge_reconciliation(&id).unwrap();
    let ticket = second.prepare_send(PromptMode::Queue).unwrap();
    assert_ne!(ticket.id(), id);
    second.cancel_prepared(ticket).unwrap();
    assert_eq!(fixture.calls.lock().unwrap().len(), 1);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn detached_mount_reopens_the_same_destination_draft_settings_and_owner() {
    let (runtime, run) = open().await;
    let composer = StandaloneComposerBinding::open(run).unwrap();
    let first = composer.mount().unwrap();
    let destination = first.destination();
    first.edit_text("保留的草稿".into()).unwrap();
    let request = first.begin_settings().unwrap();
    assert_eq!(request.mount_id(), first.id());
    let mut settings = BTreeMap::new();
    settings.insert("model".into(), json!("fixture-model"));
    assert!(request.commit(settings).unwrap());

    assert!(first.detach());
    assert!(first.snapshot().is_err());
    let second = first.reattach().unwrap();
    assert_ne!(first.id(), second.id());
    assert_eq!(second.destination(), destination);
    let draft = second.draft().unwrap();
    assert_eq!(draft.text, "保留的草稿");
    assert_eq!(draft.settings.get("model"), Some(&json!("fixture-model")));
    drop(second);
    drop(first);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn asynchronous_settings_reply_from_detached_mount_is_rejected() {
    let (runtime, run) = open().await;
    let composer = StandaloneComposerBinding::open(run).unwrap();
    let mount = composer.mount().unwrap();
    let request = mount.begin_settings().unwrap();
    mount.detach();
    assert!(request.destination().is_err());
    assert!(
        request
            .commit(BTreeMap::from([("model".into(), json!("late"))]))
            .is_err()
    );
    let newer = mount.reattach().unwrap();
    assert!(!newer.draft().unwrap().settings.contains_key("model"));
    drop(newer);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn sibling_mounts_share_submission_identity_while_other_runs_keep_separate_drafts() {
    let (runtime, run) = open().await;
    let composer = StandaloneComposerBinding::open(run).unwrap();
    let first = composer.mount().unwrap();
    let sibling = composer.mount().unwrap();
    first.edit_text("原Run草稿".into()).unwrap();
    let ticket = first.prepare_send(PromptMode::Queue).unwrap();
    let original_id = ticket.id().to_owned();
    assert_eq!(sibling.submissions().unwrap()[0].id, original_id);
    assert!(first.detach());
    assert_eq!(sibling.submissions().unwrap()[0].id, original_id);

    let other_run = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main(ENDPOINT, OTHER_SESSION))
            .await
            .unwrap(),
    );
    let other_composer = StandaloneComposerBinding::open(other_run).unwrap();
    let other_mount = other_composer.mount().unwrap();
    other_mount.edit_text("另一个Run".into()).unwrap();
    assert_eq!(other_mount.draft().unwrap().text, "另一个Run");
    assert_eq!(sibling.draft().unwrap().text, "");
    assert_eq!(sibling.submissions().unwrap()[0].id, original_id);
    sibling.cancel_prepared(ticket).unwrap();

    drop(other_mount);
    drop(sibling);
    drop(first);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn detach_wakes_old_waiter_and_reattach_gets_a_live_publication_stream() {
    let (runtime, run) = open().await;
    let composer = StandaloneComposerBinding::open(run).unwrap();
    let first = composer.mount().unwrap();
    let mut updates = first.subscribe().unwrap();
    assert_eq!(
        updates
            .current()
            .unwrap()
            .selection
            .as_ref()
            .unwrap()
            .destination,
        first.destination()
    );
    let waiting = tokio::spawn(async move { updates.changed().await });
    tokio::task::yield_now().await;
    first.detach();
    assert!(
        tokio::time::timeout(Duration::from_secs(1), waiting)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );

    let second = first.reattach().unwrap();
    let updates = second.subscribe().unwrap();
    assert_eq!(
        updates
            .current()
            .unwrap()
            .selection
            .as_ref()
            .unwrap()
            .destination,
        second.destination()
    );
    drop(second);
    drop(first);
    runtime.stop().await.unwrap();
}
