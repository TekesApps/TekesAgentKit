#![cfg(feature = "acceptance-source-witness")]

use async_trait::async_trait;
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
    time::Duration,
};
use tekes_agent_components_ui::{ReadOnlyTranscript, ReadOnlyTranscriptRead};
use tekes_agent_contract::*;
use tekes_agent_presentation::{
    ChildHistoryLayout, ChildHistorySource, ComposerDestination, RunScope, TranscriptSnapshot,
};
use tekes_agent_session::*;
use tekes_agent_ui_bindings::{ChildPaneBinding, RunBinding};
use tekes_ui_controls::composition::AccessibilityAction;
use tokio::sync::mpsc;

const ENDPOINT: &str = "child-pane-witness";
const THREAD: &str = "10000000-0000-4000-8000-000000000001";
const CHILD: &str = "30000000-0000-4000-8000-000000000003";
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
                    {"sessionId":CHILD,"status":"idle","running":false,"blank":false,"updatedAt":1},
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

#[derive(Default)]
struct BoundedReadGate {
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
}

fn page_with_entry(seq: i64, more: bool, text: &str) -> Value {
    let page = json!({
        "parentSessionId": THREAD,
        "childSessionId": READ_ONLY_CHILD,
        "throughSequence": 1,
        "historyRevision": "a".repeat(64),
        "entries": [{"event":{"seq":seq,"time":1,"type":"assistant/message","data":{
            "turn":7,"step":seq+1,"message":{"id":format!("message-{seq}"),"content":[{"type":"text","text":text}]}
        }}}],
        "hasMoreBefore": more,
        "runOpen": false,
        "sourceCall": {"callId":"original-bounded-call","name":"task","arguments":{"task_name":"paged source"},"phase":"done"}
    });
    page
}

fn seed_pages(fixture: &Fixture) {
    fixture.bounded_pages.lock().unwrap().extend([
        page_with_entry(1, true, "head"),
        page_with_entry(0, false, "earlier"),
        page_with_entry(1, true, "head refreshed"),
        page_with_entry(1, true, "head only"),
    ]);
}

fn intent_from_readonly_transcript(
    child: &ChildPaneBinding,
    snapshot: &tekes_agent_ui_bindings::PaneSnapshot,
    read: ReadOnlyTranscriptRead,
) -> tekes_agent_components_ui::ReadOnlyTranscriptIntent {
    let mut view = ReadOnlyTranscript::standard(
        8_000,
        8_003..8_200,
        child.binding().scope().clone(),
        Default::default(),
    )
    .unwrap();
    view.update(child.readonly_input(snapshot, false, None).unwrap())
        .unwrap();
    let root = view.ids().root;
    let mut host = view.into_host().unwrap();
    let target = {
        let transcript = host
            .get(root)
            .unwrap()
            .component::<ReadOnlyTranscript>()
            .unwrap();
        match read {
            ReadOnlyTranscriptRead::Refresh => transcript.ids().refresh,
            ReadOnlyTranscriptRead::Earlier => transcript.paging_button_id(),
        }
    };
    host.accessibility_action(target, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    let intents = host
        .get_mut(root)
        .unwrap()
        .component_mut::<ReadOnlyTranscript>()
        .unwrap()
        .take_intents(&actions);
    assert_eq!(intents.len(), 1, "the original component admits one intent");
    assert_eq!(intents[0].read(), read);
    intents.into_iter().next().unwrap()
}

#[tokio::test]
async fn original_readonly_earlier_intent_returns_one_accumulated_publication() {
    use tekes_agent_presentation::BoundedChildPagedSourceOperation as Operation;
    use tekes_agent_ui_bindings::PaneSnapshot;

    let (fixture, runtime, root) = setup().await;
    root.edit_text(&ComposerDestination::run(main_scope()), "保留父草稿".into())
        .unwrap();
    let page = |sequence, text| {
        let mut value = page_with_entry(sequence, true, text);
        value["throughSequence"] = json!(2);
        value
    };
    fixture.bounded_pages.lock().unwrap().extend([
        page(2, "open head"),
        page(2, "initial displayed head"),
        page(1, "earlier"),
        page(2, "head after Earlier"),
        page(2, "head after reconnect"),
    ]);
    let child = ChildPaneBinding::for_run(
        &root,
        READ_ONLY_CHILD,
        ChildHistoryLayout::SameSession,
        None,
    )
    .await
    .unwrap();

    let initial = child.snapshot().await.unwrap();
    let initial_publication = match &initial {
        PaneSnapshot::ReadOnlyChild {
            publication,
            transcript,
            ..
        } => {
            assert!(transcript.has_more_before);
            *publication
        }
        _ => panic!("same read-only child publication"),
    };
    assert_eq!(fixture.bounded_requests.lock().unwrap().len(), 2);

    let earlier =
        intent_from_readonly_transcript(&child, &initial, ReadOnlyTranscriptRead::Earlier);
    let duplicate = earlier.clone();
    let earlier_witness = child
        .read_intent_with_paged_source_witness(earlier)
        .await
        .unwrap();
    assert_eq!(
        fixture.bounded_requests.lock().unwrap().len(),
        4,
        "the UI Earlier sends exactly one bounded page request; witness capture performs one head refresh"
    );
    {
        let requests = fixture.bounded_requests.lock().unwrap();
        assert_eq!(requests[2].through_sequence, Some(2));
        assert_eq!(requests[2].before_sequence, Some(2));
        assert_eq!(requests[2].max_messages, 50);
        assert_eq!(requests[3].before_sequence, None);
    }

    let PaneSnapshot::ReadOnlyChild {
        publication,
        transcript,
        ..
    } = &earlier_witness.snapshot
    else {
        panic!("same read-only child publication")
    };
    assert_eq!(*publication, initial_publication + 1);
    assert!(transcript.items.iter().any(|item| item.text == "earlier"));
    assert!(
        transcript
            .items
            .iter()
            .any(|item| item.text == "head after Earlier")
    );
    assert_eq!(
        earlier_witness
            .source
            .receipts
            .iter()
            .map(|receipt| receipt.operation.clone())
            .collect::<Vec<_>>(),
        vec![
            Operation::InitialHead,
            Operation::RefreshHead,
            Operation::Earlier,
            Operation::RefreshHead
        ]
    );
    assert!(earlier_witness.source.receipts[2].merged);
    assert_eq!(
        earlier_witness.source.receipts[2].request.before_sequence,
        Some(2)
    );
    assert_eq!(root.current_run().unwrap().draft.text, "保留父草稿");
    let request_count = fixture.bounded_requests.lock().unwrap().len();
    let duplicate_error = child
        .read_intent_with_paged_source_witness(duplicate)
        .await
        .err()
        .expect("a cloned UI gesture is admitted only once");
    assert!(duplicate_error.to_string().contains("already admitted"));
    assert_eq!(
        fixture.bounded_requests.lock().unwrap().len(),
        request_count
    );

    let stale_earlier = intent_from_readonly_transcript(
        &child,
        &earlier_witness.snapshot,
        ReadOnlyTranscriptRead::Earlier,
    );
    let old_epoch = runtime.inspect(|store| store.connection_epoch);
    runtime.reconnect().await.unwrap();
    runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
    let stale_error = child
        .read_intent_with_paged_source_witness(stale_earlier)
        .await
        .err()
        .expect("an Earlier gesture from an old epoch is rejected");
    assert!(stale_error.to_string().contains("epoch expired"));
    assert_eq!(
        fixture.bounded_requests.lock().unwrap().len(),
        request_count
    );

    let refresh = intent_from_readonly_transcript(
        &child,
        &earlier_witness.snapshot,
        ReadOnlyTranscriptRead::Refresh,
    );
    let refreshed = child
        .read_intent_with_paged_source_witness(refresh)
        .await
        .unwrap();
    assert_eq!(
        fixture.bounded_requests.lock().unwrap().len(),
        request_count + 1
    );
    assert!(refreshed.source.connection_epoch > old_epoch);
    assert!(
        refreshed
            .source
            .receipts
            .iter()
            .any(|receipt| receipt.read_epoch == old_epoch)
    );
    assert!(
        refreshed
            .source
            .projected_entries
            .iter()
            .any(|entry| entry.event.seq == 1)
    );
    assert_eq!(refreshed.source.parent_scope, main_scope());
    assert_eq!(refreshed.source.child_id, READ_ONLY_CHILD);
    assert_eq!(
        refreshed.source.receipts.last().unwrap().read_epoch,
        refreshed.source.connection_epoch
    );
    assert!(matches!(
        refreshed.snapshot,
        PaneSnapshot::ReadOnlyChild { .. }
    ));
    assert_eq!(root.current_run().unwrap().draft.text, "保留父草稿");
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn paged_witness_binds_cumulative_earlier_entries_to_one_publication() {
    use tekes_agent_presentation::BoundedChildPagedSourceOperation as Operation;
    use tekes_agent_ui_bindings::PaneSnapshot;

    let (fixture, runtime, root) = setup().await;
    root.edit_text(&ComposerDestination::run(main_scope()), "保留父草稿".into())
        .unwrap();
    seed_pages(&fixture);
    let child = ChildPaneBinding::for_run(
        &root,
        READ_ONLY_CHILD,
        ChildHistoryLayout::SameSession,
        None,
    )
    .await
    .unwrap();
    assert!(child.load_earlier().await.unwrap());
    let requests_before = fixture.bounded_requests.lock().unwrap().len();
    let witness = child
        .binding()
        .snapshot_with_bounded_paged_source_witness()
        .await
        .unwrap();
    assert_eq!(
        fixture.bounded_requests.lock().unwrap().len(),
        requests_before + 1
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
        panic!("same read-only child publication")
    };
    assert_eq!(scope, &nested_scope(READ_ONLY_CHILD));
    assert_eq!(*connection_epoch, witness.source.connection_epoch);
    assert_eq!(witness.source.parent_scope, main_scope());
    assert_eq!(witness.source.child_id, READ_ONLY_CHILD);
    assert_eq!(witness.source.page_size, 50);
    assert_eq!(witness.source.projected_entries.len(), 2);
    assert_eq!(witness.source.projected_entries[0].event.seq, 0);
    assert_eq!(witness.source.projected_entries[1].event.seq, 1);
    assert_eq!(transcript.items.len(), 2);
    assert!(transcript.items.iter().any(|item| item.text == "earlier"));
    assert!(
        transcript
            .items
            .iter()
            .any(|item| item.text == "head refreshed")
    );
    assert_eq!(
        witness
            .source
            .receipts
            .iter()
            .map(|receipt| receipt.operation.clone())
            .collect::<Vec<_>>(),
        vec![
            Operation::InitialHead,
            Operation::Earlier,
            Operation::RefreshHead
        ]
    );
    let earlier = &witness.source.receipts[1];
    assert_eq!(earlier.request.through_sequence, Some(1));
    assert_eq!(earlier.request.before_sequence, Some(1));
    assert!(earlier.merged);
    assert!(!witness.source.has_more_before);
    assert_eq!(
        fixture.bounded_requests.lock().unwrap()[requests_before].before_sequence,
        None,
        "witness reuses a single normal refresh, without a second query"
    );
    let input = child
        .readonly_input(&witness.snapshot, false, None)
        .unwrap();
    assert_eq!(input.source, Some(source));
    assert_eq!(input.publication, *publication);
    assert!(std::ptr::eq(input.transcript, transcript));
    assert_eq!(root.current_run().unwrap().draft.text, "保留父草稿");

    assert!(
        child
            .binding()
            .snapshot_with_bounded_source_witness()
            .await
            .is_err(),
        "head-only witness continues to reject accumulated history"
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn paged_witness_rejects_unsupported_sources_and_retired_panes() {
    let (fixture, runtime, root) = setup().await;
    assert!(
        root.snapshot_with_bounded_paged_source_witness()
            .await
            .is_err()
    );
    let independent =
        ChildPaneBinding::for_run(&root, CHILD, ChildHistoryLayout::IndependentSessions, None)
            .await
            .unwrap();
    assert!(
        independent
            .binding()
            .snapshot_with_bounded_paged_source_witness()
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
            .snapshot_with_bounded_paged_source_witness()
            .await
            .is_err()
    );
    fixture
        .bounded_pages
        .lock()
        .unwrap()
        .push_back(page_with_entry(1, false, "head"));
    let built_in = ChildPaneBinding::for_run(
        &root,
        READ_ONLY_CHILD,
        ChildHistoryLayout::SameSession,
        None,
    )
    .await
    .unwrap();
    built_in.retire();
    assert!(
        built_in
            .binding()
            .snapshot_with_bounded_paged_source_witness()
            .await
            .is_err()
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn paged_witness_cannot_publish_after_retirement_during_read_or_epoch_change() {
    let (fixture, runtime, root) = setup().await;
    fixture.bounded_pages.lock().unwrap().extend([
        page_with_entry(1, true, "head"),
        page_with_entry(1, true, "refresh"),
        page_with_entry(1, true, "refresh after reconnect"),
        page_with_entry(1, true, "stale gated refresh"),
        page_with_entry(1, true, "fresh connection refresh"),
    ]);
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
    let pending =
        tokio::spawn(async move { binding.snapshot_with_bounded_paged_source_witness().await });
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

    let second = ChildPaneBinding::for_run(
        &root,
        READ_ONLY_CHILD,
        ChildHistoryLayout::SameSession,
        None,
    )
    .await
    .unwrap();
    let gate = Arc::new(BoundedReadGate::default());
    *fixture.bounded_gate.lock().unwrap() = Some(gate.clone());
    let binding = second.binding().clone();
    let pending =
        tokio::spawn(async move { binding.snapshot_with_bounded_paged_source_witness().await });
    tokio::time::timeout(Duration::from_secs(2), gate.entered.notified())
        .await
        .unwrap();
    let old_epoch = runtime.inspect(|store| store.connection_epoch);
    runtime.reconnect().await.unwrap();
    assert!(runtime.inspect(|store| store.connection_epoch) > old_epoch);
    gate.release.notify_one();
    assert!(
        tokio::time::timeout(Duration::from_secs(2), pending)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
    let refreshed = second
        .binding()
        .snapshot_with_bounded_paged_source_witness()
        .await
        .unwrap();
    assert!(refreshed.source.connection_epoch > old_epoch);
    assert!(
        refreshed
            .source
            .receipts
            .iter()
            .any(|receipt| receipt.read_epoch == old_epoch)
    );
    assert_eq!(
        refreshed.source.receipts.last().unwrap().read_epoch,
        refreshed.source.connection_epoch,
        "a new publication labels cached pages with their original read epochs"
    );
    runtime.stop().await.unwrap();
}
