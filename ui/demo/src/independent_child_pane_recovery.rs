//! Acceptance-only proof of the original independent child RunPort controls.
//!
//! This exercises the mounted pane's own TranscriptPaging and the demo host's
//! reconnect command. It does not alter the component or endpoint state model.
use super::*;
use serde_json::{Value, json};
use tekes_agent_components_ui::{RunPort, TranscriptPaging};
use tekes_agent_ui_bindings::HistoryPagePhase;

const FRAME_PREFIX: &str = "AGENTKIT_CHILD_PANE_RECOVERY ";

fn history_rows(snapshot: &tekes_agent_presentation::RunSnapshot) -> Vec<Value> {
    snapshot
        .transcript
        .items
        .iter()
        .filter(|item| !item.is_provisional && item.source_position.is_some())
        .map(|item| {
            let mut position = serde_json::to_value(&item.source_position).unwrap();
            // Pi's entryOrdinal is relative to the loaded ancestry window;
            // the stable entryId and contentIndex retain the original record.
            if let Some(position) = position.as_object_mut() {
                position.remove("entryOrdinal");
            }
            json!({"id":item.id,"recordId":item.record_id,"kind":item.kind,
                "text":item.text,"sourceSequence":item.source_sequence,
                "sourceRecordPosition":position})
        })
        .collect()
}

fn paging_button(host: &Host, ids: tekes_agent_components_ui::ComposerIds) -> ComponentId {
    // ComponentDemo::run_controls installs RunPort paging at this namespace
    // derived from the exact mounted ComposerIds.
    let namespace = u32::try_from(ids.root.0 >> 32).expect("demo namespace fits u32");
    let root = ComponentId((4_000_000_u64 + u64::from(namespace)) << 32);
    host.view
        .tree
        .get(root)
        .and_then(|node| node.component::<TranscriptPaging>())
        .expect("mounted child's original TranscriptPaging component")
        .button_id()
}

fn displayed(
    host: &Host,
    ids: tekes_agent_components_ui::ComposerIds,
) -> tekes_agent_presentation::RunSnapshot {
    host.view
        .tree
        .get(ComponentId(ids.root.0 + 16))
        .and_then(|node| node.component::<RunPort>())
        .and_then(RunPort::displayed_snapshot)
        .cloned()
        .expect("mounted child RunPort publication")
}

fn mounted_scope(host: &Host, ids: tekes_agent_components_ui::ComposerIds) -> RunScope {
    host.view
        .tree
        .get(ComponentId(ids.root.0 + 16))
        .and_then(|node| node.component::<RunPort>())
        .expect("mounted child RunPort")
        .scope()
        .clone()
}

pub(super) fn run(
    host: &mut Host,
    parent: &RunScope,
    child_id: &str,
    parent_binding: &Arc<RunBinding>,
    parent_draft: &str,
    source: &Arc<ChildPaneBinding>,
) -> Value {
    assert_eq!(source.layout(), ChildHistoryLayout::IndependentSessions);
    assert_eq!(source.parent(), parent);
    assert_eq!(source.child_id(), child_id);
    assert!(Arc::ptr_eq(
        host.view
            .current_binding()
            .expect("selected parent binding"),
        parent_binding
    ));
    assert!(host.view.child_pane_active(parent, child_id));
    assert!(Arc::ptr_eq(
        &host
            .view
            .cached_child_source(
                parent,
                parent,
                child_id,
                ChildHistoryLayout::IndependentSessions,
            )
            .expect("original cached child source"),
        source
    ));

    let panel_ids = host
        .view
        .panel_composer_ids()
        .expect("original child Composer and RunPort");
    let child_scope = source.binding().scope().clone();
    assert_eq!(child_scope.endpoint_id, parent.endpoint_id);
    assert_eq!(child_scope.thread_id, parent.thread_id);
    assert_eq!(child_scope.session_id, child_id);
    let before = displayed(host, panel_ids);
    assert_eq!(mounted_scope(host, panel_ids), child_scope);
    let before_pages = source.binding().history_page_state();
    let page_root = ComponentId((4_000_000_u64 + (panel_ids.root.0 >> 32)) << 32);
    let paging_facts = host
        .view
        .tree
        .get(page_root)
        .and_then(|node| node.component::<TranscriptPaging>())
        .and_then(TranscriptPaging::facts)
        .expect("original child paging publication")
        .clone();
    let control = paging_button(host, panel_ids);
    let paging = host
        .view
        .tree
        .get(control)
        .and_then(|node| node.component::<tekes_ui_controls::composition::ButtonContent>())
        .expect("actual child-pane earlier control");
    assert!(
        !paging.button.state.disabled,
        "earlier control is available"
    );
    assert!(paging_facts.supported && paging_facts.source_ready && paging_facts.can_load());
    assert!(
        before.transcript.has_more_before,
        "public child has earlier history"
    );
    assert_eq!(before_pages.scope, child_scope);

    let commands = press(host, control);
    let [DemoCommand::ReadHistoryPage(request)] = commands.as_slice() else {
        panic!("mounted child earlier gesture did not route to its original page request");
    };
    assert_eq!(request.scope(), &child_scope);
    host.commands(Ok(commands));
    wait(host, "original independent child earlier page", |host| {
        let current = displayed(host, panel_ids);
        source.binding().history_page_state().phase == (HistoryPagePhase::Loaded { extended: true })
            && mounted_scope(host, panel_ids) == child_scope
            && current.transcript.items.len() > before.transcript.items.len()
    });
    let after = displayed(host, panel_ids);
    let before_rows = history_rows(&before);
    let after_rows = history_rows(&after);
    assert!(!before_rows.is_empty());
    assert!(after_rows.len() > before_rows.len());
    // DSH may expose additional original request/prompt context records between
    // existing message rows after its raw journal page is loaded. All prior
    // records must remain an unchanged ordered subsequence, with genuinely
    // older source rows before the original head; they need not be a suffix.
    let positions: Vec<_> = before_rows
        .iter()
        .map(|row| {
            after_rows
                .iter()
                .position(|candidate| candidate["id"] == row["id"])
                .expect("earlier page lost an original history record")
        })
        .collect();
    assert!(positions[0] > 0, "page did not prepend older records");
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    for (row, position) in before_rows.iter().zip(&positions) {
        assert_eq!(row, &after_rows[*position], "original record changed");
    }
    let unique: std::collections::BTreeSet<_> = after_rows
        .iter()
        .map(|row| row["id"].as_str().unwrap())
        .collect();
    assert_eq!(unique.len(), after_rows.len(), "duplicate history record");
    let after_pages = source.binding().history_page_state();
    assert_eq!(after_pages.scope, child_scope);
    assert_eq!(after_pages.connection_epoch, before_pages.connection_epoch);
    assert!(after.transcript.items.len() > before.transcript.items.len());
    assert!(Arc::ptr_eq(
        &host
            .view
            .cached_child_source(
                parent,
                parent,
                child_id,
                ChildHistoryLayout::IndependentSessions,
            )
            .expect("same child source after paging"),
        source
    ));
    assert_eq!(
        parent_binding.current_run().unwrap().draft.text,
        parent_draft
    );

    let old_epoch = after.connection_epoch;
    // Go through the same host command handler used by the demo reconnect key;
    // the handler reconnects the selected endpoint and publishes its completion.
    host.commands(Ok(vec![DemoCommand::Reconnect]));
    wait(
        host,
        "host reconnect and same mounted child source",
        |host| {
            host.view.child_pane_active(parent, child_id)
                && host.view.panel_composer_ids() == Some(panel_ids)
                && Arc::ptr_eq(
                    &host
                        .view
                        .cached_child_source(
                            parent,
                            parent,
                            child_id,
                            ChildHistoryLayout::IndependentSessions,
                        )
                        .expect("same child source during reconnect"),
                    source,
                )
                && source.binding().access() == PaneAccess::Ready
                && source.binding().history_page_state().connection_epoch > old_epoch
                && displayed(host, panel_ids).connection_epoch > old_epoch
        },
    );
    let reconnected_epoch = source.binding().history_page_state().connection_epoch;
    let witness = source
        .binding()
        .refresh_with_source_witness()
        .expect("same public child binding after host reconnect");
    assert!(witness.projection_succeeded());
    assert_eq!(witness.authority().endpoint_id, parent.endpoint_id);
    assert_eq!(witness.authority().connection_epoch, reconnected_epoch);
    assert!(witness.authority().sessions.contains_key(child_id));
    assert_eq!(witness.snapshot().connection_epoch, reconnected_epoch);
    let reconnected_rows = history_rows(witness.snapshot());
    assert_eq!(
        reconnected_rows, after_rows,
        "reconnect must retain original paged record identities/content/order"
    );
    assert_eq!(
        witness.snapshot().selection.as_ref().unwrap().destination,
        ComposerDestination::run(child_scope.clone())
    );
    assert_eq!(
        parent_binding.current_run().unwrap().draft.text,
        parent_draft
    );
    host.view.sync().unwrap();
    assert_eq!(
        displayed(host, panel_ids).connection_epoch,
        reconnected_epoch
    );

    let frame = json!({
        "accepted": true,
        "evidenceScope": "original-independent-child-RunPort-paging-and-host-reconnect",
        "pagination": {
            "originalControlPresent": true,
            "originalControlEnabled": true,
            "supportedByMountedBinding": paging_facts.supported,
            "requestedScope": child_scope,
            "connectionEpoch": before_pages.connection_epoch,
            "beforeRevision": before.revision,
            "afterRevision": after.revision,
            "beforeTranscriptItems": before.transcript.items.len(),
            "afterTranscriptItems": after.transcript.items.len(),
            "earlierPageLoaded": matches!(after_pages.phase, HistoryPagePhase::Loaded { extended: true }),
            "hasMoreAfter": after.transcript.has_more_before
        },
        "reconnect": {
            "dispatchedThroughDemoHost": true,
            "sameParent": true,
            "sameChildPointer": true,
            "sameMountedSource": true,
            "paneRemainedSelected": true,
            "epochBefore": old_epoch,
            "epochAfter": reconnected_epoch,
            "sourceReady": source.binding().access() == PaneAccess::Ready,
            "projectionSucceeded": witness.projection_succeeded(),
            "sourceContainsOriginalChild": witness.authority().sessions.contains_key(child_id),
            "transcriptItemsAfterReconnect": witness.snapshot().transcript.items.len()
        },
        "parentDraftPreserved": parent_binding.current_run().unwrap().draft.text == parent_draft,
        "historyRecords": {"before":before_rows,"afterPage":after_rows,
            "afterReconnect":reconnected_rows,"piWindowRelativeOrdinalExcluded":true},
        "nativeInteraction": false,
        "physicalIme": false
    });
    println!("{FRAME_PREFIX}{frame}");
    std::io::Write::flush(&mut std::io::stdout()).expect("flush child-pane recovery witness");
    frame
}
