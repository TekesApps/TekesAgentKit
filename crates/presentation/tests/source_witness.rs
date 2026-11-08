#![cfg(feature = "acceptance-source-witness")]

use async_trait::async_trait;
use futures_util::stream;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tekes_agent_contract::*;
use tekes_agent_presentation::*;
use tekes_agent_session::*;
use tokio::sync::mpsc;

const ENDPOINT: &str = "witness-endpoint";
const PARENT: &str = "10000000-0000-4000-8000-000000000001";
const CHILD: &str = "20000000-0000-4000-8000-000000000002";
const FOREIGN_CHILD: &str = "30000000-0000-4000-8000-000000000003";

#[derive(Clone, Default)]
struct Fixture {
    streams: Arc<Mutex<BTreeMap<String, mpsc::UnboundedSender<Value>>>>,
}

impl Fixture {
    async fn send(&self, key: &str, frame: Value) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(sender) = self.streams.lock().unwrap().get(key).cloned()
                    && sender.send(frame.clone()).is_ok()
                {
                    return;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("fixture stream was not opened");
    }
}

struct Endpoint {
    fixture: Fixture,
    host: HostDescription,
}

#[async_trait]
impl EndpointConnector for Fixture {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        let host: HostDescription = serde_json::from_value(json!({
            "protocolVersion":4,
            "product":{"name":"source-witness-fixture","version":"1"},
            "capabilities":["session-journal","session-inventory-sync","session-control-sync","actionable-sync"],
            "cwd":"/fixture","home":"/fixture","attachedSessions":0,"canOpenPath":false
        }))?;
        Ok(Arc::new(Endpoint {
            fixture: self.clone(),
            host,
        }))
    }
}

#[async_trait]
impl SessionEndpoint for Endpoint {
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
        let (key, initial) = match target {
            StreamTarget::SessionInventory => (
                "inventory".to_owned(),
                json!({"type":"baseline","generation":"inventory-g1","snapshot":{"items":[
                    {"sessionId":PARENT,"status":"idle","running":false,"blank":false,"updatedAt":1},
                    {"sessionId":CHILD,"status":"idle","running":false,"blank":false,"updatedAt":1}
                ]}}),
            ),
            StreamTarget::SessionControl => (
                "control".to_owned(),
                json!({"type":"baseline","generation":"control-g1","snapshot":{"queues":[],"jobs":[],"projections":[]}}),
            ),
            StreamTarget::Actionables => (
                "actionables".to_owned(),
                json!({"type":"reset","generation":"actionables-g1"}),
            ),
            StreamTarget::SessionJournal { address, .. } => {
                let id = address.session_id;
                let entries = if id == PARENT {
                    parent_history()
                } else {
                    vec![]
                };
                (
                    format!("journal:{id}"),
                    json!({"type":"snapshot","snapshot":{
                        "sessionId":id,"generation":"journal-g1","throughSequence":entries.last().map(|e| e["event"]["seq"].as_i64().unwrap_or(-1)).unwrap_or(-1),
                        "windowLimit":50,"entries":entries,"hasMoreBefore":false,"historyRevision":"source-witness-v1"
                    }}),
                )
            }
            StreamTarget::WorkspaceInventory | StreamTarget::SessionConversation { .. } => {
                return Err(unsupported("unexpected fixture stream"));
            }
        };
        let (sender, receiver) = mpsc::unbounded_channel();
        self.fixture
            .streams
            .lock()
            .unwrap()
            .insert(key, sender.clone());
        sender.send(initial).map_err(|_| Error::Disconnected)?;
        Ok(Box::pin(stream::unfold(
            receiver,
            |mut receiver| async move { receiver.recv().await.map(|frame| (Ok(frame), receiver)) },
        )))
    }
    async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
        Err(unsupported("fixture history pagination"))
    }
    async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
        Ok(())
    }
    async fn call(&self, _: &str, _: Value, _: &str) -> Result<Value> {
        Ok(json!({"accepted":true}))
    }
    async fn close(&self) {}
}

fn parent_history() -> Vec<Value> {
    vec![
        event(
            0,
            "user/message",
            json!({"turn":1,"message":{"content":[{"type":"text","text":"parent input"}]}}),
        ),
        event(1, "turn/start", json!({"turn":1})),
        event(
            2,
            "tool/call",
            json!({"turn":1,"step":1,"callId":"child-call","name":"subagent","arguments":{"description":"actual child"}}),
        ),
        event(
            3,
            "tool/result",
            json!({"turn":1,"step":1,"message":{"source":{"callId":"child-call"},"content":[{"type":"text","text":format!("Started child {CHILD}")}]}}),
        ),
        event(
            4,
            "tool/call",
            json!({"turn":1,"step":1,"callId":"foreign-call","name":"subagent","arguments":{"description":"candidate from another endpoint"}}),
        ),
        event(
            5,
            "tool/result",
            json!({"turn":1,"step":1,"message":{"source":{"callId":"foreign-call"},"content":[{"type":"text","text":format!("Started child {FOREIGN_CHILD}")}]}}),
        ),
        event(
            6,
            "assistant/message",
            json!({"turn":1,"step":1,"message":{"id":"answer","content":[{"type":"text","text":"parent answer"}]}}),
        ),
        event(7, "turn/end", json!({"turn":1,"reason":"completed"})),
    ]
}

fn event(seq: i64, kind: &str, data: Value) -> Value {
    json!({"event":{"type":kind,"seq":seq,"time":100+seq,"data":data}})
}

fn main_scope() -> RunScope {
    RunScope::main(ENDPOINT, PARENT)
}

async fn setup() -> (Fixture, Arc<SessionRuntime>, RunController) {
    let fixture = Fixture::default();
    let runtime = Arc::new(SessionRuntime::spawn(ENDPOINT, Arc::new(fixture.clone())));
    runtime
        .wait_ready(Duration::from_secs(2))
        .await
        .unwrap_or_else(|e| {
            panic!(
                "runtime readiness: {e}; {:?}",
                runtime.inspect(Clone::clone)
            )
        });
    let controller = RunController::new(runtime.clone()).unwrap();
    controller.open(main_scope()).await.unwrap();
    (fixture, runtime, controller)
}

async fn wait_until(mut predicate: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while !predicate() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("projection did not converge");
}

#[tokio::test]
async fn capture_pairs_the_published_summary_with_its_exact_authority_and_keeps_old_witness_immutable()
 {
    let (fixture, runtime, controller) = setup().await;
    let published = controller.subscribe();
    let first = controller.refresh_with_source_witness();
    assert!(Arc::ptr_eq(first.snapshot(), &published.borrow()));
    assert!(first.projection_succeeded());
    let snapshot = first.snapshot();
    let authority = first.authority();
    assert_eq!(snapshot.connection_epoch, authority.connection_epoch);
    assert_eq!(
        snapshot.selection.as_ref().unwrap().destination,
        ComposerDestination::run(main_scope())
    );
    assert_eq!(snapshot.summary.as_ref().unwrap().scope, main_scope());
    let canonical =
        SummaryFacts::read_with_store(&main_scope(), authority, &snapshot.transcript).unwrap();
    assert_eq!(snapshot.summary.as_ref(), Some(&canonical));
    let child = canonical
        .subagents
        .iter()
        .find(|row| row.title == "actual child")
        .unwrap();
    assert_eq!(child.session_id.as_deref(), Some(CHILD));
    assert_eq!(child.status, "idle");
    assert_eq!(child.indicator(), None);
    let foreign = canonical
        .subagents
        .iter()
        .find(|row| row.title == "candidate from another endpoint")
        .unwrap();
    assert_eq!(
        foreign.session_id, None,
        "a UUID candidate absent from this endpoint must not become a child pointer"
    );
    assert!(authority.sessions.contains_key(CHILD));
    assert!(!authority.sessions.contains_key(FOREIGN_CHILD));

    fixture.send("inventory", json!({"type":"upsert","generation":"inventory-g1","session":{"sessionId":CHILD,"status":"failed","running":false,"blank":false,"updatedAt":2}})).await;
    wait_until(|| {
        runtime.inspect(|s| {
            s.sessions
                .get(CHILD)
                .and_then(|p| p.summary.as_ref())
                .is_some_and(|s| s["status"] == "failed")
        })
    })
    .await;
    let second = controller.refresh_with_source_witness();
    assert!(second.projection_succeeded());
    assert_eq!(
        second.authority().connection_epoch,
        authority.connection_epoch
    );
    assert_eq!(
        second
            .snapshot()
            .summary
            .as_ref()
            .unwrap()
            .subagents
            .iter()
            .find(|row| row.title == "actual child")
            .unwrap()
            .status,
        "failed"
    );
    assert_eq!(
        first
            .authority()
            .sessions
            .get(CHILD)
            .unwrap()
            .summary
            .as_ref()
            .unwrap()["status"],
        "idle"
    );
    assert_eq!(
        first
            .snapshot()
            .summary
            .as_ref()
            .unwrap()
            .subagents
            .iter()
            .find(|row| row.title == "actual child")
            .unwrap()
            .status,
        "idle"
    );

    let old_epoch = second.authority().connection_epoch;
    runtime.reconnect().await.unwrap();
    runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
    let third = controller.refresh_with_source_witness();
    assert!(third.projection_succeeded());
    assert!(third.authority().connection_epoch > old_epoch);
    assert_eq!(
        third.snapshot().connection_epoch,
        third.authority().connection_epoch
    );
    assert_eq!(
        second.authority().connection_epoch,
        old_epoch,
        "later reconnect must not mutate the captured source"
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn malformed_public_journal_projection_reports_degraded_and_retains_content_as_stale() {
    let (fixture, runtime, controller) = setup().await;
    let before = controller.refresh_with_source_witness();
    assert!(before.projection_succeeded());
    let prior = before.snapshot().clone();
    assert!(!prior.transcript.items.is_empty());

    fixture
        .send(
            "journal:10000000-0000-4000-8000-000000000001",
            json!({"type":"event","generation":"journal-g1","sessionId":PARENT,"entry":event(8,"turn/end",json!({"turn":2,"reason":"completed","promotedMessageID":"missing"}))}),
        )
        .await;
    wait_until(|| {
        runtime.inspect(|s| s.sessions.get(PARENT).is_some_and(|p| p.tail_sequence == 8))
    })
    .await;
    let degraded = controller.refresh_with_source_witness();
    assert!(
        !degraded.projection_succeeded(),
        "malformed canonical Summary input must use the controller's existing degraded path"
    );
    assert_eq!(degraded.snapshot().transcript.items, prior.transcript.items);
    assert_eq!(degraded.snapshot().summary, prior.summary);
    assert!(degraded.snapshot().transcript.is_stale);
    assert!(!degraded.snapshot().can_submit);
    assert_eq!(
        degraded
            .authority()
            .sessions
            .get(PARENT)
            .unwrap()
            .tail_sequence,
        8
    );
    runtime.stop().await.unwrap();
}
