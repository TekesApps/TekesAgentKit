#![cfg(feature = "acceptance-source-witness")]

use async_trait::async_trait;
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
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
    bounded_pages: Arc<Mutex<VecDeque<Value>>>,
    bounded_requests: Arc<Mutex<Vec<BoundedChildHistoryRequest>>>,
    bounded_gate: Arc<Mutex<Option<Arc<BoundedReadGate>>>>,
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
        EndpointCapabilities::methods(&["session.prompt", "subagents.history"])
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
    async fn call(&self, method: &str, payload: Value, _: &str) -> Result<Value> {
        if method == "subagents.history" {
            let request: BoundedChildHistoryRequest = serde_json::from_value(payload)?;
            self.fixture.bounded_requests.lock().unwrap().push(request);
            let page = self
                .fixture
                .bounded_pages
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| Error::Protocol("unexpected bounded fixture read".into()))?;
            let gate = self.fixture.bounded_gate.lock().unwrap().take();
            if let Some(gate) = gate {
                gate.entered.notify_one();
                gate.release.notified().await;
            }
            if page["error"] == true {
                return Err(Error::Remote(json!({"code":"history-stale"})));
            }
            return Ok(page);
        }
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

#[derive(Default)]
struct BoundedReadGate {
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
}
fn bounded_page(phase: &str) -> Value {
    json!({
        "parentSessionId":THREAD,"childSessionId":READ_ONLY_CHILD,
        "throughSequence":0,"historyRevision":"a".repeat(64),
        "entries":[{"event":{"seq":0,"time":1,"type":"assistant/message","data":{
            "turn":7,"step":1,"message":{"id":"bounded-original-message","content":[{"type":"text","text":"只读子历史"}]}
        }}}],
        "hasMoreBefore":false,"runOpen":false,
        "sourceCall":{"callId":"original-bounded-call","name":"task","arguments":{"task_name":"只读来源"},"phase":phase}
    })
}

#[tokio::test]
async fn bounded_pane_witness_is_the_original_read_and_component_publication() {
    use tekes_agent_ui_bindings::PaneSnapshot;
    let (fixture, runtime, root) = setup().await;
    root.edit_text(&ComposerDestination::run(main_scope()), "验收父草稿".into())
        .unwrap();
    fixture.bounded_pages.lock().unwrap().extend([
        bounded_page("running"),
        bounded_page("running"),
        bounded_page("error"),
        bounded_page("stopped"),
    ]);
    let child = ChildPaneBinding::for_run(
        &root,
        READ_ONLY_CHILD,
        ChildHistoryLayout::SameSession,
        None,
    )
    .await
    .unwrap();
    let before = child.snapshot().await.unwrap();
    let PaneSnapshot::ReadOnlyChild {
        source: original_source,
        publication: before_publication,
        ..
    } = &before
    else {
        panic!("readonly source")
    };
    let requests_before = fixture.bounded_requests.lock().unwrap().len();
    let witness = child
        .binding()
        .snapshot_with_bounded_source_witness()
        .await
        .unwrap();
    assert_eq!(
        fixture.bounded_requests.lock().unwrap().len(),
        requests_before + 1,
        "witness cannot perform a second history query"
    );
    let PaneSnapshot::ReadOnlyChild {
        source,
        scope,
        publication,
        connection_epoch,
        transcript,
        ..
    } = &witness.snapshot
    else {
        panic!("original readonly publication")
    };
    assert_eq!(source, original_source);
    assert_eq!(scope, &nested_scope(READ_ONLY_CHILD));
    assert_eq!(*publication, before_publication + 1);
    assert!(*connection_epoch > 0);
    assert_eq!(transcript.turns[0].status, "error");
    assert_eq!(transcript.tools[0].phase, "error");
    assert_eq!(transcript.items[0].text, "只读子历史");
    assert_eq!(witness.source.parent_scope, main_scope());
    assert_eq!(witness.source.child_id, READ_ONLY_CHILD);
    assert_eq!(witness.source.page_size, 50);
    assert_eq!(
        witness.source.raw_head.source_call.as_ref().unwrap()["phase"],
        "error"
    );
    assert_eq!(
        witness.source.raw_head.source_call.as_ref().unwrap()["arguments"],
        json!({"task_name":"只读来源"})
    );
    let input = child
        .readonly_input(&witness.snapshot, false, None)
        .unwrap();
    assert_eq!(input.publication, *publication);
    assert!(
        std::ptr::eq(input.transcript, transcript),
        "component input must borrow this returned publication"
    );
    assert!(child.binding().current_run().is_none());
    assert!(
        child.binding().refresh_with_source_witness().is_err(),
        "controller witness must still reject readonly sources"
    );
    let next = child.snapshot().await.unwrap();
    let PaneSnapshot::ReadOnlyChild {
        publication: next_publication,
        transcript: next_transcript,
        ..
    } = next
    else {
        panic!("ordinary readonly read")
    };
    assert_eq!(next_publication, publication + 1);
    assert_eq!(next_transcript.turns[0].status, "aborted");
    assert_eq!(root.current_run().unwrap().draft.text, "验收父草稿");
    assert!(fixture.bounded_pages.lock().unwrap().is_empty());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn bounded_pane_witness_rejects_wrong_source_and_retired_pane_without_io() {
    let (fixture, runtime, root) = setup().await;
    assert!(root.snapshot_with_bounded_source_witness().await.is_err());
    let independent =
        ChildPaneBinding::for_run(&root, CHILD, ChildHistoryLayout::IndependentSessions, None)
            .await
            .unwrap();
    assert!(
        independent
            .binding()
            .snapshot_with_bounded_source_witness()
            .await
            .is_err()
    );
    let custom = ChildPaneBinding::for_run(
        &root,
        READ_ONLY_CHILD,
        ChildHistoryLayout::SameSession,
        Some(Arc::new(EmptyChildSource)),
    )
    .await
    .unwrap();
    assert!(
        custom
            .binding()
            .snapshot_with_bounded_source_witness()
            .await
            .is_err()
    );
    fixture
        .bounded_pages
        .lock()
        .unwrap()
        .push_back(bounded_page("done"));
    let builtin = ChildPaneBinding::for_run(
        &root,
        READ_ONLY_CHILD,
        ChildHistoryLayout::SameSession,
        None,
    )
    .await
    .unwrap();
    let calls = fixture.bounded_requests.lock().unwrap().len();
    builtin.retire();
    assert!(
        builtin
            .binding()
            .snapshot_with_bounded_source_witness()
            .await
            .is_err()
    );
    assert_eq!(fixture.bounded_requests.lock().unwrap().len(), calls);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn bounded_pane_witness_cannot_publish_after_retirement_during_read() {
    let (fixture, runtime, root) = setup().await;
    fixture
        .bounded_pages
        .lock()
        .unwrap()
        .extend([bounded_page("running"), bounded_page("error")]);
    let child = Arc::new(
        ChildPaneBinding::for_run(
            &root,
            READ_ONLY_CHILD,
            ChildHistoryLayout::SameSession,
            None,
        )
        .await
        .unwrap(),
    );
    let gate = Arc::new(BoundedReadGate::default());
    *fixture.bounded_gate.lock().unwrap() = Some(gate.clone());
    let binding = child.binding().clone();
    let pending = tokio::spawn(async move { binding.snapshot_with_bounded_source_witness().await });
    tokio::time::timeout(Duration::from_secs(2), gate.entered.notified())
        .await
        .unwrap();
    child.retire();
    gate.release.notify_one();
    assert!(
        tokio::time::timeout(Duration::from_secs(2), pending)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    assert_eq!(child.binding().access(), PaneAccess::Missing);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn bounded_pane_witness_rejects_read_error_instead_of_reusing_previous_head() {
    let (fixture, runtime, root) = setup().await;
    fixture
        .bounded_pages
        .lock()
        .unwrap()
        .extend([bounded_page("done"), json!({"error":true})]);
    let child = ChildPaneBinding::for_run(
        &root,
        READ_ONLY_CHILD,
        ChildHistoryLayout::SameSession,
        None,
    )
    .await
    .unwrap();
    assert!(
        child
            .binding()
            .snapshot_with_bounded_source_witness()
            .await
            .is_err()
    );
    assert!(fixture.bounded_pages.lock().unwrap().is_empty());
    runtime.stop().await.unwrap();
}
