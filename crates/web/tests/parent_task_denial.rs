//! One synthetic public-service contract, with shared Web consumer JSON.
//! No Kernel, transport service, model, or browser draft is exercised here.
use async_trait::async_trait;
use futures_util::stream::{self, StreamExt};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tekes_agent_contract::*;
use tekes_agent_presentation::{AgentTask, RunPresentation, RunScope, TranscriptSnapshot};
use tekes_agent_web::SharedAgent;
use tokio::sync::mpsc;

type Sender = mpsc::UnboundedSender<Result<Value>>;
struct Fixture {
    input: Value,
    streams: Mutex<BTreeMap<String, Sender>>,
    responses: Mutex<Vec<(String, i64, Value)>>,
    calls: Mutex<Vec<String>>,
}
impl Fixture {
    fn send(&self, plane: &str, frame: Value) {
        self.streams.lock().unwrap()[plane].send(Ok(frame)).unwrap();
    }
    fn parent(&self) -> &str {
        self.input["identity"]["parentSessionId"].as_str().unwrap()
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
                "protocolVersion":4,"product":{"name":"j7-synthetic","version":"1"},
                "capabilities":["session-journal","session-inventory-sync","session-control-sync","actionable-sync"],
                "cwd":"/work","home":"/home","attachedSessions":0,"canOpenPath":false
            }))?,
        }))
    }
}
#[async_trait]
impl SessionEndpoint for Endpoint {
    fn identity(&self) -> &str {
        "native"
    }
    fn host(&self) -> &HostDescription {
        &self.host
    }
    fn capabilities(&self) -> EndpointCapabilities {
        EndpointCapabilities::methods(&["session.prompt", "session.cancel", "subagents.list"])
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        let input = &self.fixture.input;
        let (plane, frames) = match target {
            StreamTarget::SessionInventory => (
                "inventory",
                vec![json!({
                    "type":"baseline","generation":"inventory","snapshot":{"items":[{
                        "sessionId":self.fixture.parent(),"running":true,"blank":false,"updatedAt":1
                    }]}
                })],
            ),
            StreamTarget::SessionControl => (
                "control",
                vec![json!({
                    "type":"baseline","generation":"control","snapshot":{"queues":[],"jobs":[],"projections":[]}
                })],
            ),
            StreamTarget::Actionables => (
                "actions",
                vec![
                    json!({"type":"reset","generation":"actions"}),
                    json!({"type":"upsert","generation":"actions","actionable":input["sourceWitness"]["actionable"]}),
                ],
            ),
            StreamTarget::SessionJournal { address, .. } => {
                assert_eq!(address.session_id, self.fixture.parent());
                (
                    "journal",
                    vec![json!({"type":"snapshot","snapshot":{
                        "sessionId":self.fixture.parent(),"generation":"journal","throughSequence":5,
                        "windowLimit":50,"entries":input["sourceWitness"]["journalEntriesBefore"],"hasMoreBefore":false
                    }})],
                )
            }
            _ => return Err(unsupported("unexpected J7 fixture stream")),
        };
        let (tx, rx) = mpsc::unbounded_channel();
        self.fixture
            .streams
            .lock()
            .unwrap()
            .insert(plane.into(), tx);
        Ok(Box::pin(stream::iter(frames.into_iter().map(Ok)).chain(
            stream::unfold(rx, |mut rx| async {
                rx.recv().await.map(|frame| (frame, rx))
            }),
        )))
    }
    async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
        Err(unsupported("J7 has no paging"))
    }
    async fn close(&self) {}
    async fn respond(&self, id: &str, revision: i64, outcome: Value) -> Result<()> {
        let expected = &self.fixture.input["respond"];
        assert_eq!(id, expected["actionableId"].as_str().unwrap());
        assert_eq!(revision, expected["expectedRevision"].as_i64().unwrap());
        assert_eq!(outcome, expected["outcome"]);
        self.fixture
            .responses
            .lock()
            .unwrap()
            .push((id.into(), revision, outcome));
        // The Mock publishes the original parent's denied source, not a child failure.
        self.fixture.send("journal", json!({"type":"event","generation":"journal",
            "sessionId":self.fixture.parent(),"entry":self.fixture.input["sourceWitness"]["deniedEntry"]}));
        self.fixture.send(
            "actions",
            json!({"type":"resolved","generation":"actions",
            "actionableId":id,"revision":revision}),
        );
        Ok(())
    }
    async fn call(&self, method: &str, payload: Value, _: &str) -> Result<Value> {
        self.fixture.calls.lock().unwrap().push(method.into());
        match method {
            "subagents.list" => {
                assert_eq!(payload["parentSessionId"], self.fixture.parent());
                Ok(self.fixture.input["sourceWitness"]["publicCatalog"].clone())
            }
            _ => Err(unsupported("unexpected J7 fixture call")),
        }
    }
}

async fn family_at(agent: &SharedAgent, input: &Value, status: &str) -> Value {
    let runtime = &agent.endpoint("native").unwrap().runtime;
    let mut changes = runtime.subscribe();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            changes.borrow_and_update();
            let value = agent.request(&input["familyRequest"]).await.unwrap();
            let approval_present = runtime.inspect(|state| {
                state
                    .actionables
                    .contains_key(input["identity"]["actionableId"].as_str().unwrap())
            });
            if value["summary"]["tasks"][0]["status"] == status
                && approval_present == (status == "in_progress")
            {
                return value;
            }
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn parent_task_denial_preserves_public_identity_and_rejects_stale_authority() {
    let input: Value =
        serde_json::from_str(include_str!("fixtures/j7-parent-denial.json")).unwrap();
    let fixture = Arc::new(Fixture {
        input: input.clone(),
        streams: Mutex::default(),
        responses: Mutex::default(),
        calls: Mutex::default(),
    });
    let mut agent = SharedAgent::default();
    agent
        .register(
            "native".into(),
            "tekes".into(),
            "J7".into(),
            Arc::new(Connector(fixture.clone())),
        )
        .unwrap();
    let runtime = agent.endpoint("native").unwrap().runtime.clone();
    runtime.wait_ready(Duration::from_secs(5)).await.unwrap();
    runtime.open_session(fixture.parent()).await.unwrap();
    let before = family_at(&agent, &input, "in_progress").await;
    assert_eq!(before, input["familyBefore"]);
    let epoch = runtime.snapshot().connection_epoch;
    assert_eq!(
        epoch,
        input["identity"]["connectionEpoch"].as_u64().unwrap()
    );
    let scope = RunScope::main("native", fixture.parent());
    let authority = runtime.snapshot();
    let original_queue = authority.sessions[fixture.parent()].queue.clone();
    let original_jobs = authority.sessions[fixture.parent()].jobs.clone();
    let controls = RunPresentation::read(&authority, &scope, true);
    let actionable = input["identity"]["actionableId"].as_str().unwrap();
    assert!(controls.can_cancel);
    assert!(controls.can_respond(actionable, 11));
    let original_tool = TranscriptSnapshot::read(&scope, &authority.sessions[fixture.parent()])
        .unwrap()
        .tools[0]
        .clone();
    assert_eq!(original_tool.phase, "doing");
    assert_eq!(
        original_tool.id,
        input["identity"]["taskId"].as_str().unwrap()
    );
    let mut stale = input["respond"].clone();
    stale["expectedRevision"] = json!(10);
    assert!(agent.request(&stale).await.is_err());
    assert!(fixture.responses.lock().unwrap().is_empty());
    assert_eq!(agent.request(&input["respond"]).await.unwrap(), json!({}));
    let after = family_at(&agent, &input, "error").await;
    assert_eq!(after, input["familyAfter"]);
    assert_eq!(runtime.snapshot().connection_epoch, epoch);
    let authority = runtime.snapshot();
    let after_transcript =
        TranscriptSnapshot::read(&scope, &authority.sessions[fixture.parent()]).unwrap();
    let current_tool = &after_transcript.tools[0];
    assert_eq!(current_tool.id, original_tool.id);
    assert_eq!(current_tool.source_position, original_tool.source_position);
    assert_eq!(current_tool.arguments, original_tool.arguments);
    assert_eq!(current_tool.phase, "error");
    assert_eq!(
        current_tool.result_error,
        Some(json!({"code":"denied","name":"Denied"}))
    );
    assert_eq!(
        current_tool.result_meta.as_ref().unwrap()["kernel_outcome"],
        "denied"
    );
    assert_eq!(
        serde_json::to_value(&authority.sessions[fixture.parent()].entries[&6]).unwrap(),
        input["sourceWitness"]["deniedEntry"]
    );
    let task: AgentTask = serde_json::from_value(after["summary"]["tasks"][0].clone()).unwrap();
    assert_eq!(
        task.canonical_status(),
        input["expected"]["canonicalAfter"].as_str().unwrap()
    );
    assert_eq!(
        after["subagentCatalog"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["kind"] == "child")
            .count(),
        0
    );
    assert!(
        after["subagentCatalog"]["parentAvailable"]
            .as_bool()
            .unwrap()
    );
    let after_controls = RunPresentation::read(&authority, &scope, true);
    assert_eq!(after_controls.can_cancel, controls.can_cancel);
    assert!(!after_controls.can_respond(actionable, 11));
    assert_eq!(authority.sessions[fixture.parent()].queue, original_queue);
    assert_eq!(authority.sessions[fixture.parent()].jobs, original_jobs);
    assert!(agent.request(&input["respond"]).await.is_err());
    assert_eq!(
        fixture.responses.lock().unwrap().as_slice(),
        &[(actionable.into(), 11, input["respond"]["outcome"].clone())]
    );
    assert!(
        fixture
            .calls
            .lock()
            .unwrap()
            .iter()
            .all(|method| method == "subagents.list")
    );
    // Establish available authority immediately before disconnect so this check
    // cannot pass merely because the original denial already resolved its row.
    let mut pending = input["sourceWitness"]["actionable"].clone();
    pending["id"] = json!("disconnect-authority");
    pending["revision"] = json!(12);
    fixture.send(
        "actions",
        json!({"type":"upsert","generation":"actions","actionable":pending}),
    );
    let mut changes = runtime.subscribe();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            changes.borrow_and_update();
            if runtime.inspect(|state| {
                RunPresentation::read(state, &scope, true).can_respond("disconnect-authority", 12)
            }) {
                break;
            }
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    runtime.stop().await.unwrap();
    // The API has no expectedEpoch argument: test loss of old authority on disconnect,
    // not a fictitious service-side caller-epoch guard. Web owns its caller context.
    assert!(runtime.snapshot().actionables.is_empty());
    let mut disconnected = input["respond"].clone();
    disconnected["actionableId"] = json!("disconnect-authority");
    disconnected["expectedRevision"] = json!(12);
    assert!(agent.request(&disconnected).await.is_err());
    assert_eq!(fixture.responses.lock().unwrap().len(), 1);
}
