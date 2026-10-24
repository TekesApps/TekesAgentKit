//! Explicit offscreen proof of the original Native child's paged read controls.
use super::*;
use tekes_agent_components_ui::{
    ReadOnlyTranscript, ReadOnlyTranscriptIntent, ReadOnlyTranscriptRead,
};
use tekes_agent_ui_bindings::BoundedChildPagedPaneWitness;

struct Proof {
    parent: RunScope,
    child: String,
    marker: String,
    parent_binding: Arc<RunBinding>,
    draft: String,
    selection: Option<tekes_agent_presentation::SelectionToken>,
    source: Arc<ChildPaneBinding>,
    root: ComponentId,
    source_id: String,
    rows: std::collections::BTreeMap<String, u64>,
}

impl Proof {
    fn capture(&self, host: &mut Host, earlier: bool) -> ReadOnlyTranscriptIntent {
        let (refresh, more) = host
            .view
            .bounded_child_buttons()
            .expect("original read controls");
        let commands = press(host, if earlier { more } else { refresh });
        let [
            DemoCommand::ReadChild {
                source,
                panel_parent,
                page,
                read_intent,
            },
        ] = commands.as_slice()
        else {
            panic!(
                "one original ReadOnlyTranscript command required: {}",
                commands.len()
            );
        };
        assert!(Arc::ptr_eq(source, &self.source));
        assert_eq!(panel_parent, &self.parent);
        assert_eq!(*page, earlier);
        let intent = read_intent.clone().expect("original read intent");
        assert_eq!(intent.scope(), self.source.binding().scope());
        assert_eq!(
            intent.read(),
            if earlier {
                ReadOnlyTranscriptRead::Earlier
            } else {
                ReadOnlyTranscriptRead::Refresh
            }
        );
        intent
    }

    fn read(&self, host: &mut Host, earlier: bool) -> (Value, BoundedChildPagedPaneWitness) {
        let intent = self.capture(host, earlier);
        let origin = json!({
            "read":if earlier { "earlier" } else { "refresh" },
            "scope":intent.scope(),"publication":intent.publication(),
            "connectionEpoch":intent.connection_epoch()
        });
        let witness = host
            .executor
            .block_on(self.source.read_intent_with_paged_source_witness(intent))
            .expect("original intent same-read cumulative witness");
        (origin, witness)
    }

    fn install(&self, host: &mut Host, witness: &BoundedChildPagedPaneWitness) {
        host.apply_result(
            HostResult::ChildRead {
                panel_parent: self.parent.clone(),
                source: self.source.clone(),
                result: Ok(witness.snapshot.clone()),
                catalog_error: None,
            },
            None,
        );
        assert!(
            host.failures.is_empty(),
            "host installation: {:?}",
            host.failures
        );
        // apply_result uses the original view installer, which synchronously updates
        // the mounted component. Do not request a second source snapshot here.
        assert!(host.view.child_pane_active(&self.parent, &self.child));
        assert_eq!(host.view.bounded_child_view_id(), Some(self.root));
        let (source, snapshot) = host
            .view
            .acceptance_child_pane_snapshot(&self.parent, &self.child)
            .expect("mounted original source cache");
        assert!(Arc::ptr_eq(source, &self.source));
        assert_eq!(
            pane_snapshot_json(snapshot),
            pane_snapshot_json(&witness.snapshot)
        );
        self.parent_preserved(host);
    }

    fn parent_preserved(&self, host: &Host) {
        assert!(Arc::ptr_eq(
            host.view.current_binding().unwrap(),
            &self.parent_binding
        ));
        let run = self.parent_binding.current_run().unwrap();
        assert_eq!(run.draft.text, self.draft);
        assert_eq!(run.selection, self.selection);
    }

    fn frame(
        &mut self,
        host: &mut Host,
        phase: &str,
        intent: Value,
        witness: &BoundedChildPagedPaneWitness,
        old_earlier: Option<Value>,
    ) {
        self.install(host, witness);
        let mounted = host
            .view
            .tree
            .get(self.root)
            .unwrap()
            .component::<ReadOnlyTranscript>()
            .unwrap();
        assert_eq!(mounted.content().scope(), self.source.binding().scope());
        let row_keys: Vec<_> = mounted
            .content()
            .row_keys()
            .iter()
            .map(|key| format!("{key:?}"))
            .collect();
        let row_ids: Vec<_> = mounted
            .content()
            .row_keys()
            .iter()
            .map(|key| mounted.content().row_id(key).expect("mounted row ID").0)
            .collect();
        assert!(!row_keys.is_empty());
        for (key, id) in row_keys.iter().zip(&row_ids) {
            if let Some(previous) = self.rows.get(key) {
                assert_eq!(previous, id, "retained reading row changed ID");
            }
            self.rows.insert(key.clone(), *id);
        }
        let snapshot = pane_snapshot_json(&witness.snapshot);
        let mut frame = json!({
            "phase":phase,"parent":self.parent,"childId":self.child,"marker":self.marker,
            "sourceInstance":self.source_id,"intent":intent,
            "witness":{"snapshot":snapshot,"source":witness.source},
            "mountedSnapshot":snapshot,
            "mounted":{"control":"original-ReadOnlyTranscript","transcriptId":self.root.0,
                "rowKeys":row_keys,"rowIds":row_ids,"transcript":snapshot["transcript"],
                "parentDraft":self.draft,"parentSelection":self.selection,
                "sourceIdentity":self.source_id,"parentBindingPreserved":true}
        });
        if let Some(old) = old_earlier {
            frame["oldEarlier"] = old;
        }
        use std::io::Write;
        println!("AGENTKIT_NATIVE_PAGED_SOURCE {frame}");
        std::io::stdout().flush().expect("flush paged source frame");
    }
}

pub(super) fn run(
    host: &mut Host,
    manifest: &Value,
    parent_binding: Arc<RunBinding>,
    draft: String,
    source: Arc<ChildPaneBinding>,
) -> Value {
    let parent = scope_from_manifest(manifest);
    let child = manifest["childId"].as_str().expect("child ID").to_owned();
    let marker = manifest["marker"]
        .as_str()
        .expect("child marker")
        .to_owned();
    let earlier_reads = manifest["expectedEarlierReads"]
        .as_u64()
        .expect("public page plan length");
    assert!(
        (1..=16).contains(&earlier_reads),
        "bounded public page plan required"
    );
    assert_eq!(parent.endpoint_id, "native");
    let mut proof = Proof {
        selection: parent_binding.current_run().unwrap().selection.clone(),
        source_id: format!("{:p}", Arc::as_ptr(&source)),
        root: host
            .view
            .bounded_child_view_id()
            .expect("original readonly pane"),
        parent,
        child,
        marker,
        parent_binding,
        draft,
        source,
        rows: Default::default(),
    };
    // Opening the original pane may still have a scheduled ordinary initial read.
    // Drain it before activating an explicit control; its real reads stay in the chain.
    wait(host, "initial readonly read", |host| {
        host.view
            .tree
            .get(proof.root)
            .and_then(|node| node.component::<ReadOnlyTranscript>())
            .is_some_and(|pane| {
                host.view
                    .tree
                    .get(pane.ids().refresh)
                    .and_then(|node| {
                        node.component::<tekes_ui_controls::composition::ButtonContent>()
                    })
                    .is_some_and(|button| !button.button.state.disabled)
            })
    });
    let (intent, head) = proof.read(host, false);
    assert_eq!(head.source.page_size, 1);
    assert!(head.source.has_more_before && head.source.run_open);
    let original_epoch = head.source.connection_epoch;
    proof.frame(host, "running_head", intent, &head, None);

    let stale = proof.capture(host, true);
    let captured_epoch = stale.connection_epoch();
    let runtime = host
        .sidebar
        .runtime_for(&proof.parent.endpoint_id)
        .unwrap()
        .clone();
    host.executor.block_on(async {
        runtime.reconnect().await.unwrap();
        runtime.wait_ready(Duration::from_secs(10)).await.unwrap();
    });
    let current_epoch = runtime.inspect(|state| state.connection_epoch);
    assert!(current_epoch > original_epoch);
    let error = host
        .executor
        .block_on(proof.source.read_intent_with_paged_source_witness(stale))
        .err()
        .expect("old Earlier must be rejected")
        .to_string();
    assert!(
        error.contains("epoch expired"),
        "unexpected old intent rejection: {error}"
    );
    // Use the original error-completion lane to clear the gesture's loading state.
    // The expected rejection is evidence, not an unexpected host failure.
    host.view
        .child_read_failed(&proof.parent, &proof.source, error.clone())
        .unwrap();
    let background = host.view.take_background_commands();
    host.commands(Ok(background));
    wait(host, "reconnected readonly controls", |host| {
        host.view
            .tree
            .get(proof.root)
            .and_then(|node| node.component::<ReadOnlyTranscript>())
            .is_some_and(|pane| {
                host.view
                    .tree
                    .get(pane.ids().refresh)
                    .and_then(|node| {
                        node.component::<tekes_ui_controls::composition::ButtonContent>()
                    })
                    .is_some_and(|button| !button.button.state.disabled)
            })
    });
    let (intent, refreshed) = proof.read(host, false);
    assert_eq!(refreshed.source.connection_epoch, current_epoch);
    assert!(
        refreshed
            .source
            .receipts
            .iter()
            .any(|receipt| receipt.read_epoch == original_epoch)
    );
    proof.frame(host,"reconnected_refresh",intent,&refreshed,Some(json!({
        "rejected":true,"capturedEpoch":captured_epoch,"currentEpoch":current_epoch,"error":error
    })));
    for index in 1..=earlier_reads {
        let (intent, witness) = proof.read(host, true);
        assert!(witness.source.run_open);
        assert_eq!(witness.source.has_more_before, index < earlier_reads);
        proof.frame(
            host,
            &format!("reconnected_page_{index}"),
            intent,
            &witness,
            None,
        );
    }
    // The strict independent validator releases the held fifth request only
    // after every original Earlier matches the frozen public page plan.
    let deadline = Instant::now() + Duration::from_secs(45);
    let settled = loop {
        let (intent, witness) = proof.read(host, false);
        let settled = witness
            .source
            .source_call
            .as_ref()
            .is_some_and(|call| call["phase"] == "error")
            && !witness.source.run_open;
        if settled {
            break (intent, witness);
        }
        proof.install(host, &witness);
        assert!(
            Instant::now() < deadline,
            "provider did not settle after paged frames"
        );
        tick(host);
    };
    assert!(
        settled
            .1
            .source
            .receipts
            .iter()
            .any(|receipt| receipt.operation
                == tekes_agent_presentation::BoundedChildPagedSourceOperation::PrefixValidation
                && !receipt.merged)
    );
    proof.frame(host, "settled_refresh", settled.0, &settled.1, None);
    let close = host
        .view
        .tree
        .get(host.view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap()
        .close_id();
    let commands = press(host, close);
    host.commands(Ok(commands));
    sync(host);
    assert!(!host.view.child_pane_active(&proof.parent, &proof.child));
    assert_eq!(proof.source.binding().access(), PaneAccess::Missing);
    assert!(
        host.executor
            .block_on(
                proof
                    .source
                    .binding()
                    .snapshot_with_bounded_paged_source_witness()
            )
            .is_err()
    );
    proof.parent_preserved(host);
    json!({"accepted":true,"evidenceScope":"actual-native-paged-child-original-intents",
        "parent":proof.parent,"childId":proof.child,"childScope":proof.source.binding().scope(),
        "openedViaOriginalControl":true,"closedViaOriginalControl":true,"sameSource":true,
        "oldEarlierRejected":true,"publicPagePlanMatched":true,"earlierReadCount":earlier_reads,
        "parentDraftPreserved":true,"parentSelectionPreserved":true,"retiredWitnessRejected":true,
        "desktopInteraction":false,"physicalIme":false,"readingAnchorAcceptance":false})
}
