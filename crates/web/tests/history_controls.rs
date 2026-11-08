use async_trait::async_trait;
use futures_util::stream::{self, StreamExt};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tekes_agent_contract::*;
use tekes_agent_web::SharedAgent;
use tokio::sync::mpsc;

type Sender = mpsc::UnboundedSender<Result<Value>>;
#[derive(Default)]
struct Fixture {
    fail_initial_history: bool,
    streams: Mutex<BTreeMap<String, Sender>>,
    writes: Mutex<Vec<(String, i64, Value)>>,
}
impl Fixture {
    fn send(&self, plane: &str, frame: Result<Value>) {
        self.streams
            .lock()
            .unwrap()
            .get(plane)
            .unwrap()
            .send(frame)
            .unwrap();
    }
}
struct Connector(Arc<Fixture>);
struct Endpoint {
    fixture: Arc<Fixture>,
    host: HostDescription,
}
#[async_trait]
impl EndpointConnector for Connector {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        Ok(Arc::new(Endpoint {
            fixture: self.0.clone(),
            host: serde_json::from_value(json!({
                "protocolVersion":4,"product":{"name":"controlled-history-fault","version":"1"},
                "capabilities":["session-journal","session-inventory-sync","session-control-sync","actionable-sync"],
                "cwd":"/work","home":"/home","attachedSessions":0,"canOpenPath":false
            }))?,
        }))
    }
}
fn summary(status: &str) -> Value {
    json!({"sessionId":"a","name":"A","running":true,"blank":false,"updatedAt":1,"status":status})
}
fn action(id: &str, kind: &str, revision: i64) -> Value {
    json!({"type":"upsert","generation":"actions","actionable":{"id":id,"sessionId":"a","kind":kind,"revision":revision,"payload":{"title":"Confirm"}}})
}
#[async_trait]
impl SessionEndpoint for Endpoint {
    fn identity(&self) -> &str {
        "fault"
    }
    fn host(&self) -> &HostDescription {
        &self.host
    }
    fn capabilities(&self) -> EndpointCapabilities {
        EndpointCapabilities::methods(&["session.cancel", "session.prompt"])
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        let (plane, initial) = match target {
            StreamTarget::SessionInventory => (
                "inventory",
                vec![
                    json!({"type":"baseline","generation":"inventory","snapshot":{"items":[summary("active")]}}),
                ],
            ),
            StreamTarget::SessionControl => (
                "control",
                vec![
                    json!({"type":"baseline","generation":"control","snapshot":{"queues":[],"jobs":[],"projections":[]}}),
                ],
            ),
            StreamTarget::Actionables => (
                "actions",
                vec![
                    json!({"type":"reset","generation":"actions"}),
                    action("approval", "approval", 4),
                    action("question", "question", 7),
                ],
            ),
            StreamTarget::SessionJournal { address, .. } => {
                assert_eq!(address.session_id, "a");
                (
                    "journal",
                    vec![
                        json!({"type":"snapshot","snapshot":{"sessionId":"a","generation":"journal","throughSequence":0,"windowLimit":50,"entries":[{"event":{"type":"turn/start","seq":0,"time":1,"data":{"turn":1}}}],"hasMoreBefore":false}}),
                    ],
                )
            }
            _ => return Err(unsupported("unexpected fixture stream")),
        };
        let (tx, rx) = mpsc::unbounded_channel();
        self.fixture
            .streams
            .lock()
            .unwrap()
            .insert(plane.into(), tx);
        let initial: Vec<Result<Value>> = if plane == "journal" && self.fixture.fail_initial_history
        {
            vec![Err(Error::Remote(
                json!({"category":"internal","code":"stream-open-failed","message":"initial history unavailable"}),
            ))]
        } else {
            initial.into_iter().map(Ok).collect()
        };
        Ok(Box::pin(
            stream::iter(initial).chain(stream::unfold(rx, |mut rx| async {
                rx.recv().await.map(|frame| (frame, rx))
            })),
        ))
    }
    async fn close(&self) {}
    async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
        Err(unsupported("fixture paging"))
    }
    async fn respond(&self, id: &str, revision: i64, outcome: Value) -> Result<()> {
        self.fixture
            .writes
            .lock()
            .unwrap()
            .push((id.into(), revision, outcome));
        Ok(())
    }
    async fn call(&self, method: &str, payload: Value, rpc_id: &str) -> Result<Value> {
        match method {
            "endpoint.capabilities" => Ok(serde_json::to_value(self.capabilities())?),
            "sessions.recover" => Ok(json!({})),
            "session.cancel" => {
                assert_eq!(payload["sessionId"], "a");
                self.fixture
                    .writes
                    .lock()
                    .unwrap()
                    .push((method.into(), 0, json!(rpc_id)));
                self.fixture.send("inventory",Ok(json!({"type":"status","generation":"inventory","sessionId":"a","running":false})));
                Ok(json!({}))
            }
            _ => Err(unsupported("unexpected fixture operation")),
        }
    }
}

#[tokio::test]
async fn initial_history_failure_keeps_web_observation_and_controls_but_strict_history_still_fails()
{
    let fixture = Arc::new(Fixture {
        fail_initial_history: true,
        ..Fixture::default()
    });
    let mut agent = SharedAgent::default();
    agent
        .register(
            "fault".into(),
            "tekes".into(),
            "Fault fixture".into(),
            Arc::new(Connector(fixture.clone())),
        )
        .unwrap();
    let agent = Arc::new(agent);
    let runtime = agent.endpoint("fault").unwrap().runtime.clone();
    runtime.wait_ready(Duration::from_secs(5)).await.unwrap();
    let (tx, mut rx) = mpsc::channel(16);
    let observer = agent.clone();
    let task = tokio::spawn(async move {
        observer
            .follow(
                &json!({"endpoint":"fault","target":"view","sessionId":"a"}),
                tx,
            )
            .await
    });
    let degraded = view(&mut rx, |v| v["run"]["connection"]["phase"] == "degraded").await;
    assert_eq!(degraded["run"]["canCancel"], true);
    assert_eq!(degraded["run"]["canSubmit"], false);
    assert!(
        degraded["run"]["actionables"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["responseAvailable"] == true)
    );
    assert!(!runtime.snapshot().sessions["a"].journal_ready);
    assert!(
        runtime.acquire_history("a").await.is_err(),
        "strict history acquisition must retain the failure"
    );
    agent.request(&json!({"op":"respond","endpoint":"fault","actionableId":"approval","expectedRevision":4,"outcome":{"approved":true}})).await.unwrap();
    agent.request(&json!({"op":"call","endpoint":"fault","method":"session.cancel","rpcId":"initial-fault-cancel","payload":{"sessionId":"a"}})).await.unwrap();
    view(&mut rx, |v| v["run"]["canCancel"] == false).await;
    assert_eq!(fixture.writes.lock().unwrap().len(), 2);
    assert!(
        !task.is_finished(),
        "history failure must not end the Web observation"
    );
    task.abort();
    let _ = task.await;
    runtime.stop().await.unwrap();
}
async fn view(rx: &mut mpsc::Receiver<Value>, predicate: impl Fn(&Value) -> bool) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let value = rx.recv().await.expect("view ended");
            eprintln!(
                "public view phase={} cancel={} conn={} actions={} error={}",
                value["run"]["phase"],
                value["run"]["canCancel"],
                value["run"]["connection"]["phase"],
                value["run"]["actionables"].as_array().map_or(0, Vec::len),
                value["run"]["connection"]["message"]
            );
            if predicate(&value) {
                break value;
            }
        }
    })
    .await
    .expect("expected public view publication")
}
#[tokio::test]
async fn journal_failure_preserves_web_cancel_and_current_replies_without_authorizing_stale_or_archived()
 {
    let fixture = Arc::new(Fixture::default());
    let mut agent = SharedAgent::default();
    agent
        .register(
            "fault".into(),
            "tekes".into(),
            "Fault fixture".into(),
            Arc::new(Connector(fixture.clone())),
        )
        .unwrap();
    let agent = Arc::new(agent);
    let runtime = agent.endpoint("fault").unwrap().runtime.clone();
    runtime.wait_ready(Duration::from_secs(5)).await.unwrap();
    let (tx, mut rx) = mpsc::channel(16);
    let observer = agent.clone();
    let task = tokio::spawn(async move {
        observer
            .follow(
                &json!({"endpoint":"fault","target":"view","sessionId":"a"}),
                tx,
            )
            .await
    });
    view(&mut rx, |v| {
        v["run"]["canCancel"] == true
            && v["run"]["actionables"]
                .as_array()
                .is_some_and(|a| a.len() == 2)
    })
    .await;
    fixture.send("journal",Err(Error::Remote(json!({"category":"internal","code":"stream-open-failed","message":"controlled history failure"}))));
    let degraded = view(&mut rx, |v| v["run"]["connection"]["phase"] == "degraded").await;
    assert_eq!(degraded["run"]["canCancel"], true);
    assert_eq!(degraded["run"]["canSubmit"], false);
    assert!(
        degraded["run"]["actionables"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["responseAvailable"] == true)
    );
    assert_eq!(
        runtime.snapshot().connection,
        tekes_agent_session::ConnectionState::Ready
    );
    let reply = |id: &str, revision: i64| json!({"op":"respond","endpoint":"fault","actionableId":id,"expectedRevision":revision,"outcome":{"fixture":"accepted"}});
    assert!(agent.request(&reply("question", 6)).await.is_err());
    assert!(fixture.writes.lock().unwrap().is_empty());
    fixture.send(
        "inventory",
        Ok(json!({"type":"upsert","generation":"inventory","session":summary("archived")})),
    );
    let archived = view(&mut rx, |v| v["run"]["phase"] == "archived").await;
    assert!(
        archived["run"]["actionables"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["responseAvailable"] == false)
    );
    let cancel = json!({"op":"call","endpoint":"fault","method":"session.cancel","rpcId":"one-explicit-cancel","payload":{"sessionId":"a"}});
    assert!(agent.request(&reply("question", 7)).await.is_err());
    assert!(agent.request(&cancel).await.is_err());
    assert!(fixture.writes.lock().unwrap().is_empty());
    fixture.send(
        "inventory",
        Ok(json!({"type":"upsert","generation":"inventory","session":summary("active")})),
    );
    view(&mut rx, |v| {
        v["run"]["canCancel"] == true && v["run"]["actionables"][0]["responseAvailable"] == true
    })
    .await;
    agent.request(&reply("approval", 4)).await.unwrap();
    agent.request(&reply("question", 7)).await.unwrap();
    fixture.send("actions", Ok(action("question", "question", 8)));
    view(&mut rx, |v| {
        v["run"]["actionables"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == "question" && a["revision"] == 8)
    })
    .await;
    assert!(agent.request(&reply("question", 7)).await.is_err());
    agent.request(&reply("question", 8)).await.unwrap();
    agent.request(&cancel).await.unwrap();
    view(&mut rx, |v| v["run"]["canCancel"] == false).await;
    assert!(
        !runtime.snapshot().sessions["a"].run_open,
        "inventory must retire the cached running flag after history failure"
    );
    let writes = fixture.writes.lock().unwrap().clone();
    assert_eq!(
        writes
            .iter()
            .map(|(name, rev, _)| (name.as_str(), *rev))
            .collect::<Vec<_>>(),
        vec![
            ("approval", 4),
            ("question", 7),
            ("question", 8),
            ("session.cancel", 0)
        ]
    );
    assert_eq!(writes.last().unwrap().2, "one-explicit-cancel");
    task.abort();
    let _ = task.await;
    runtime.stop().await.unwrap();
}
