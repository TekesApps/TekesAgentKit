#![cfg(feature = "acceptance-source-witness")]

use async_trait::async_trait;
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tekes_agent_contract::*;
use tekes_agent_presentation::{
    ChildHistoryLayout, ChildHistorySource, ComposerDestination, RunScope, TranscriptSnapshot,
};
use tekes_agent_session::*;
use tekes_agent_ui_bindings::{ChildPaneBinding, PaneAccess, RunBinding};
use tokio::sync::mpsc;

const ENDPOINT: &str = "child-pane-witness";
const THREAD: &str = "10000000-0000-4000-8000-000000000001";
const DIRECT_PARENT: &str = "20000000-0000-4000-8000-000000000002";
const CHILD: &str = "30000000-0000-4000-8000-000000000003";
const SECOND_CHILD: &str = "40000000-0000-4000-8000-000000000004";
const READ_ONLY_CHILD: &str = "50000000-0000-4000-8000-000000000005";

#[derive(Clone, Default)]
struct Fixture {
    inventory: Arc<Mutex<Option<mpsc::UnboundedSender<Value>>>>,
    journals: Arc<Mutex<BTreeMap<String, mpsc::UnboundedSender<Value>>>>,
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
            "product":{"name":"child-pane-witness","version":"1"},
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
        let journal_id = match &target {
            StreamTarget::SessionJournal { address, .. } => Some(address.session_id.clone()),
            _ => None,
        };
        let (initial, inventory) = match target {
            StreamTarget::SessionInventory => (
                json!({"type":"baseline","generation":"inventory-g1","snapshot":{"items":[
                    {"sessionId":THREAD,"status":"idle","running":false,"blank":false,"updatedAt":1},
                    {"sessionId":DIRECT_PARENT,"status":"idle","running":false,"blank":false,"updatedAt":1},
                    {"sessionId":CHILD,"status":"idle","running":false,"blank":false,"updatedAt":1},
                    {"sessionId":SECOND_CHILD,"status":"idle","running":false,"blank":false,"updatedAt":1},
                    {"sessionId":READ_ONLY_CHILD,"status":"idle","running":false,"blank":false,"updatedAt":1}
                ]}}),
                true,
            ),
            StreamTarget::SessionControl => (
                json!({"type":"baseline","generation":"control-g1","snapshot":{"queues":[],"jobs":[],"projections":[]}}),
                false,
            ),
            StreamTarget::Actionables => {
                (json!({"type":"reset","generation":"actionables-g1"}), false)
            }
            StreamTarget::SessionJournal { address, .. } => (
                json!({"type":"snapshot","snapshot":{"sessionId":address.session_id,"generation":"journal-g1","throughSequence":-1,"windowLimit":50,"entries":[],"hasMoreBefore":false,"historyRevision":"child-pane-witness-v1"}}),
                false,
            ),
            StreamTarget::WorkspaceInventory | StreamTarget::SessionConversation { .. } => {
                return Err(unsupported("unexpected fixture stream"));
            }
        };
        let (sender, receiver) = mpsc::unbounded_channel();
        if inventory {
            *self.fixture.inventory.lock().unwrap() = Some(sender.clone());
        }
        if let Some(id) = journal_id {
            self.fixture
                .journals
                .lock()
                .unwrap()
                .insert(id, sender.clone());
        }
        Ok(Box::pin(
            stream::once(async move { Ok(initial) })
                .chain(stream::unfold(
                    (receiver, sender),
                    |(mut receiver, sender)| async move {
                        receiver
                            .recv()
                            .await
                            .map(|frame| (Ok(frame), (receiver, sender)))
                    },
                ))
                .boxed(),
        ))
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

struct EmptyChildSource;

#[async_trait]
impl ChildHistorySource for EmptyChildSource {
    async fn read(&self, _: &RunScope, _: &str) -> Result<TranscriptSnapshot> {
        Err(unsupported("read-only test source"))
    }
}

fn main_scope() -> RunScope {
    RunScope::main(ENDPOINT, THREAD)
}

fn nested_scope(session_id: &str) -> RunScope {
    RunScope {
        endpoint_id: ENDPOINT.into(),
        thread_id: THREAD.into(),
        session_id: session_id.into(),
    }
}

async fn setup() -> (Fixture, Arc<SessionRuntime>, RunBinding) {
    let fixture = Fixture::default();
    let runtime = Arc::new(SessionRuntime::spawn(ENDPOINT, Arc::new(fixture.clone())));
    runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
    let root = RunBinding::open_main(runtime.clone(), main_scope())
        .await
        .unwrap();
    (fixture, runtime, root)
}

async fn wait_until(mut predicate: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while !predicate() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("fixture projection did not converge");
}

#[tokio::test]
async fn witness_uses_original_nested_child_binding_and_respects_lifetimes() {
    let (fixture, runtime, root) = setup().await;
    let direct_parent = ChildPaneBinding::for_run(
        &root,
        DIRECT_PARENT,
        ChildHistoryLayout::IndependentSessions,
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        direct_parent.binding().scope(),
        &nested_scope(DIRECT_PARENT)
    );
    direct_parent
        .binding()
        .edit_text(
            &ComposerDestination::run(nested_scope(DIRECT_PARENT)),
            "parent draft".into(),
        )
        .unwrap();
    let parent_before = direct_parent.binding().current_run().unwrap();

    let child = ChildPaneBinding::for_run(
        direct_parent.binding(),
        CHILD,
        ChildHistoryLayout::IndependentSessions,
        None,
    )
    .await
    .unwrap();
    assert_eq!(child.parent(), &nested_scope(DIRECT_PARENT));
    assert_eq!(child.binding().scope(), &nested_scope(CHILD));
    assert_ne!(child.binding().scope().thread_id, child.parent().session_id);
    assert_ne!(
        child.binding().scope().session_id,
        child.parent().session_id
    );
    assert_ne!(
        child.binding().scope().thread_id,
        child.binding().scope().session_id
    );

    let published = child.binding().subscribe_run().unwrap();
    let witness = child.binding().refresh_with_source_witness().unwrap();
    assert!(witness.projection_succeeded());
    assert!(Arc::ptr_eq(witness.snapshot(), &published.borrow().clone()));
    assert_eq!(
        witness.snapshot().selection.as_ref().unwrap().destination,
        ComposerDestination::run(nested_scope(CHILD))
    );
    assert_eq!(
        witness.snapshot().run.as_ref().unwrap().scope,
        nested_scope(CHILD)
    );
    assert_eq!(
        witness.snapshot().summary.as_ref().unwrap().scope,
        nested_scope(CHILD)
    );
    assert_eq!(child.binding().access(), PaneAccess::Ready);
    let parent_after = direct_parent.binding().current_run().unwrap();
    assert_eq!(parent_after.selection, parent_before.selection);
    assert_eq!(parent_after.draft, parent_before.draft);
    assert_eq!(parent_after.draft.text, "parent draft");

    fixture
        .inventory
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .send(json!({"type":"upsert","generation":"inventory-g1","session":{"sessionId":CHILD,"status":"failed","running":false,"blank":false,"updatedAt":2}}))
        .unwrap();
    wait_until(|| {
        runtime.inspect(|store| {
            store
                .sessions
                .get(CHILD)
                .and_then(|session| session.summary.as_ref())
                .is_some_and(|summary| summary["status"] == "failed")
        })
    })
    .await;
    assert_eq!(
        witness.authority().sessions[CHILD]
            .summary
            .as_ref()
            .unwrap()["status"],
        "idle"
    );
    assert_eq!(
        runtime.inspect(
            |store| store.sessions[CHILD].summary.as_ref().unwrap()["status"]
                .as_str()
                .unwrap()
                .to_owned()
        ),
        "failed"
    );

    let read_only = ChildPaneBinding::for_run(
        &root,
        READ_ONLY_CHILD,
        ChildHistoryLayout::SameSession,
        Some(Arc::new(EmptyChildSource)),
    )
    .await
    .unwrap();
    assert!(read_only.binding().refresh_with_source_witness().is_err());

    child.retire();
    assert_eq!(child.binding().access(), PaneAccess::Missing);
    assert!(child.binding().refresh_with_source_witness().is_err());

    let descendant = ChildPaneBinding::for_run(
        direct_parent.binding(),
        SECOND_CHILD,
        ChildHistoryLayout::IndependentSessions,
        None,
    )
    .await
    .unwrap();
    direct_parent.retire();
    assert_eq!(descendant.binding().access(), PaneAccess::Missing);
    assert!(descendant.binding().refresh_with_source_witness().is_err());
}

#[tokio::test]
async fn pane_witness_preserves_original_projection_failure() {
    let (fixture, runtime, root) = setup().await;
    let child =
        ChildPaneBinding::for_run(&root, CHILD, ChildHistoryLayout::IndependentSessions, None)
            .await
            .unwrap();
    assert!(
        child
            .binding()
            .refresh_with_source_witness()
            .unwrap()
            .projection_succeeded()
    );
    fixture.journals.lock().unwrap()[CHILD]
        .send(json!({
            "type":"event", "generation":"journal-g1", "sessionId":CHILD,
            "entry":{"event":{"seq":0,"type":"turn/end","time":100,
                "data":{"turn":1,"reason":"completed","promotedMessageID":"missing"}}}
        }))
        .unwrap();
    wait_until(|| runtime.inspect(|store| store.sessions[CHILD].entries.contains_key(&0))).await;
    let witness = child.binding().refresh_with_source_witness().unwrap();
    assert!(!witness.projection_succeeded());
    assert!(witness.snapshot().transcript.is_stale);
    assert!(!witness.snapshot().can_submit);
}
