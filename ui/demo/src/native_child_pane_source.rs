//! Real independent child-pane source proof through the demo's original controls.
//!
//! The manifest chooses an already seeded public pointer and expected marker;
//! it does not install or synthesize a pane, run, or source publication.
use super::*;
use serde_json::{Value, json};
use tekes_agent_components_ui::{FamilyCatalog, SidePanel, Sidebar};
use tekes_agent_contract::SessionAddress;
use tekes_agent_presentation::{
    ChildHistoryLayout, ComposerDestination, RunScope, TranscriptInputRole, TranscriptItemKind,
};
use tekes_agent_ui_bindings::{ChildPaneBinding, PaneAccess, RunBinding};
use tekes_agent_ui_demo::component_demo::FAMILY_REFRESH;
use tekes_ui_controls::composition::{AccessibilityAction, ComponentId, ContentEvent};

fn tick(host: &mut Host) {
    host.executor.block_on(async {
        tokio::time::sleep(Duration::from_millis(10)).await;
    });
}

fn sync(host: &mut Host) {
    host.view
        .set_sidebar_host_source(host.sidebar.snapshot())
        .unwrap();
    host.view.sync().unwrap();
    let background = host.view.take_background_commands();
    host.commands(Ok(background));
}

fn wait(host: &mut Host, label: &str, predicate: impl Fn(&Host) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        host.drain_results(None);
        sync(host);
        assert!(
            host.failures.is_empty(),
            "child pane proof host errors: {:?}",
            host.failures
        );
        if predicate(host) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "child pane proof deadline: {label}"
        );
        tick(host);
    }
}

fn press(host: &mut Host, id: ComponentId) -> Vec<DemoCommand> {
    assert!(
        host.view.tree.get(id).is_some(),
        "original control absent: {id:?}"
    );
    host.view
        .accessibility_action(id, None, AccessibilityAction::Press)
        .unwrap()
}

fn sidebar_row(host: &mut Host, parent: &RunScope) {
    let address = SessionAddress {
        endpoint_id: parent.endpoint_id.clone(),
        session_id: parent.session_id.clone(),
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        sync(host);
        let sidebar = host
            .view
            .tree
            .get(host.view.sidebar_id())
            .unwrap()
            .component::<Sidebar>()
            .unwrap();
        if let Some(id) = sidebar.row_id(&address) {
            let commands = press(host, id);
            host.commands(Ok(commands));
            sync(host);
            assert_eq!(host.view.current_binding().unwrap().scope(), parent);
            return;
        }
        if let Some(more) = sidebar.page_button_id() {
            let commands = press(host, more);
            host.commands(Ok(commands));
        }
        assert!(
            Instant::now() < deadline,
            "parent row missing from original Sidebar: {parent:?}"
        );
        tick(host);
    }
}

fn scope_from_manifest(manifest: &Value) -> RunScope {
    serde_json::from_value(manifest["parent"].clone()).expect("manifest parent RunScope")
}

fn matching_user(snapshot: &tekes_agent_presentation::RunSnapshot, marker: &str) -> Option<String> {
    snapshot
        .transcript
        .items
        .iter()
        .find(|item| {
            item.kind == TranscriptItemKind::User
                && item.text == marker
                && item.is_visible
                && !item.is_provisional
                && item.source_position.is_some()
                && matches!(
                    item.input_role(),
                    Some(TranscriptInputRole::Opening | TranscriptInputRole::Inner)
                )
        })
        .map(|item| item.turn_id.clone())
}

fn run_turn<'a>(
    snapshot: &'a tekes_agent_presentation::RunSnapshot,
    turn_id: &str,
) -> Option<&'a tekes_agent_presentation::TranscriptTurn> {
    snapshot
        .transcript
        .turns
        .iter()
        .find(|turn| turn.id == turn_id)
}

fn emit(frame: &Value) {
    use std::io::Write;
    println!("AGENTKIT_CHILD_PANE_SOURCE {frame}");
    std::io::stdout()
        .flush()
        .expect("flush child pane source frame");
}

fn read_frame(source: &Arc<ChildPaneBinding>, phase: &str, turn_id: &str, marker: &str) -> Value {
    let witness = source
        .binding()
        .refresh_with_source_witness()
        .expect("original child pane source witness");
    let snapshot = witness.snapshot();
    assert_eq!(
        snapshot.connection_epoch,
        witness.authority().connection_epoch
    );
    assert_eq!(
        snapshot.connection_epoch,
        source.binding().current_run().unwrap().connection_epoch
    );
    assert_eq!(matching_user(snapshot, marker).as_deref(), Some(turn_id));
    let turn = run_turn(snapshot, turn_id).expect("marker turn in original child Transcript");
    match phase {
        "running" => {
            assert_eq!(turn.status, "running");
            assert!(turn.ended_at.is_none());
        }
        "failed" => {
            assert_eq!(turn.status, "error");
            assert!(turn.ended_at.is_some_and(f64::is_finite));
            assert!(snapshot.transcript.items.iter().any(|item| {
                item.turn_id == turn_id
                    && item.is_visible
                    && !item.is_provisional
                    && item.failure_message().is_some()
            }));
            let authority = witness.authority();
            let session = authority
                .sessions
                .get(source.child_id())
                .expect("child authority projection");
            assert!(!session.run_open, "failed child run is still open");
            assert!(
                session.queue.is_empty(),
                "child dispatch queue has not retired"
            );
        }
        _ => unreachable!("known proof phase"),
    }
    json!({
        "snapshot": snapshot.as_ref(),
        "authority": witness.authority(),
        "projectionSucceeded": witness.projection_succeeded(),
        "evidenceScope": "original-publication-and-same-memory-input",
        "serverTransactionalFreshnessProven": false
    })
}

fn find_parent(host: &Host, parent: &RunScope) -> Arc<RunBinding> {
    host.view
        .bindings()
        .iter()
        .find(|binding| binding.scope() == parent)
        .expect("manifest parent must be an original public host binding")
        .clone()
}

pub(super) fn run(host: &mut Host, manifest: &Value) -> Value {
    let parent = scope_from_manifest(manifest);
    let child_id = manifest["childId"]
        .as_str()
        .expect("manifest childId string")
        .to_owned();
    let marker = manifest["marker"]
        .as_str()
        .expect("manifest marker string")
        .to_owned();
    assert!(!child_id.is_empty() && !marker.is_empty());
    assert!(matches!(parent.endpoint_id.as_str(), "pi" | "dsh"));
    assert_ne!(parent.session_id, child_id);
    let parent_binding = find_parent(host, &parent);
    parent_binding
        .edit_text(
            &ComposerDestination::run(parent.clone()),
            format!("父草稿隔离:{}:{}", parent.endpoint_id, parent.session_id),
        )
        .expect("owned parent draft fixture");
    let parent_draft = parent_binding
        .current_run()
        .expect("parent Run")
        .draft
        .text
        .clone();

    sidebar_row(host, &parent);
    assert!(Arc::ptr_eq(
        host.view.current_binding().unwrap(),
        &parent_binding
    ));
    host.view
        .enable_family_catalog()
        .expect("original FamilyCatalog composition");
    sync(host);
    let source_catalog = host
        .view
        .current_family_catalog_source()
        .expect("original FamilyCatalog source");
    let refresh = press(host, FAMILY_REFRESH);
    host.commands(Ok(refresh));
    wait(host, "FamilyCatalog child", |host| {
        host.view
            .current_family_catalog_source()
            .is_some_and(|source| {
                source
                    .snapshot()
                    .catalog
                    .as_ref()
                    .is_some_and(|catalog| catalog.child(&child_id).is_some())
            })
            && host
                .view
                .family_catalog_id()
                .and_then(|id| host.view.tree.get(id))
                .and_then(|node| node.component::<FamilyCatalog>())
                .is_some_and(|catalog| catalog.child_button_id(&child_id).is_some())
    });
    assert!(Arc::ptr_eq(
        &source_catalog,
        &host.view.current_family_catalog_source().unwrap()
    ));
    let layout = host
        .sidebar
        .child_history_layout(&parent.endpoint_id)
        .unwrap();
    assert_eq!(layout, ChildHistoryLayout::IndependentSessions);
    let catalog = host
        .view
        .tree
        .get(host.view.family_catalog_id().unwrap())
        .unwrap()
        .component::<FamilyCatalog>()
        .unwrap();
    let child_button = catalog
        .child_button_id(&child_id)
        .expect("catalog child control");
    let open = press(host, child_button);
    assert!(
        matches!(open.as_slice(), [DemoCommand::OpenChild { request, direct_parent, child_id: id, layout: ChildHistoryLayout::IndependentSessions, .. }]
        if request.parent() == &parent && direct_parent == &parent && id == &child_id)
    );
    host.commands(Ok(open));
    wait(host, "original ChildPane open completion", |host| {
        host.view.child_pane_active(&parent, &child_id)
    });
    let source = host
        .view
        .cached_child_source(
            &parent,
            &parent,
            &child_id,
            ChildHistoryLayout::IndependentSessions,
        )
        .expect("original cached ChildPane source");
    assert_eq!(source.parent(), &parent);
    assert_eq!(source.child_id(), child_id);
    assert_eq!(source.binding().access(), PaneAccess::Ready);
    let child_scope = source.binding().scope().clone();
    assert_eq!(child_scope.endpoint_id, parent.endpoint_id);
    assert_eq!(child_scope.thread_id, parent.thread_id);
    assert_eq!(child_scope.session_id, child_id);

    let composer_ids = host
        .view
        .panel_composer_ids()
        .expect("original child pane Composer");
    let initial_child_snapshot = source
        .binding()
        .current_run()
        .expect("child Composer snapshot");
    assert_eq!(
        initial_child_snapshot
            .selection
            .as_ref()
            .unwrap()
            .destination,
        ComposerDestination::run(child_scope.clone())
    );
    assert!(
        initial_child_snapshot.draft.text.is_empty(),
        "fresh child Composer must start empty"
    );
    let focus = host
        .view
        .accessibility_action(composer_ids.editor, None, AccessibilityAction::Focus)
        .unwrap();
    host.commands(Ok(focus));
    assert_eq!(host.view.tree.focused(), Some(composer_ids.editor));
    let commit = host
        .view
        .event(&ContentEvent::Commit {
            text: marker.clone(),
            replacement: None,
        })
        .unwrap();
    host.commands(Ok(commit));
    sync(host);
    assert_eq!(source.binding().current_run().unwrap().draft.text, marker);
    wait(host, "mounted child Send readiness", |host| {
        source
            .binding()
            .current_run()
            .is_some_and(|snapshot| snapshot.can_submit)
            && host
                .view
                .tree
                .get(composer_ids.send)
                .and_then(|node| node.component::<tekes_ui_controls::composition::ButtonContent>())
                .is_some_and(|button| !button.button.state.disabled)
    });
    let commit_send = press(host, composer_ids.send);
    if !matches!(commit_send.as_slice(), [DemoCommand::Send { binding, .. }] if Arc::ptr_eq(binding, source.binding()))
    {
        eprintln!(
            "CHILD_PANE_SEND_REJECTED {}",
            json!({
                "snapshot": source.binding().current_run().unwrap().as_ref(),
                "commands": commit_send.iter().map(|command| match command {
                    DemoCommand::Send { .. } => "Send",
                    DemoCommand::ReadModels { .. } => "ReadModels",
                    DemoCommand::ReadInitialFeedback(_) => "ReadInitialFeedback",
                    _ => "other-original-command",
                }).collect::<Vec<_>>()
            })
        );
    }
    assert!(
        matches!(commit_send.as_slice(), [DemoCommand::Send { binding, .. }] if Arc::ptr_eq(binding, source.binding())),
        "original child Composer Send must retain the mounted child binding"
    );
    host.commands(Ok(commit_send));
    let turn_id = {
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            host.drain_results(None);
            sync(host);
            assert!(
                host.failures.is_empty(),
                "child pane send dispatch failed: {:?}",
                host.failures
            );
            let witness = source
                .binding()
                .refresh_with_source_witness()
                .expect("child source while provider holds");
            if let Some(turn_id) = matching_user(witness.snapshot(), &marker)
                && run_turn(witness.snapshot(), &turn_id)
                    .is_some_and(|turn| turn.status == "running")
            {
                break turn_id;
            }
            assert!(
                Instant::now() < deadline,
                "provider did not hold an original running child turn"
            );
            tick(host);
        }
    };
    let running = read_frame(&source, "running", &turn_id, &marker);
    let running_revision = running["snapshot"]["revision"]
        .as_u64()
        .expect("running publication revision");
    let connection_epoch = running["snapshot"]["connectionEpoch"]
        .as_u64()
        .expect("running source epoch");
    emit(
        &json!({"phase":"running","witness":running,"parent":&parent,"childId":&child_id,"marker":&marker}),
    );

    let deadline = Instant::now() + Duration::from_secs(60);
    let failed = loop {
        host.drain_results(None);
        sync(host);
        assert!(
            host.failures.is_empty(),
            "child pane host errors: {:?}",
            host.failures
        );
        let witness = source
            .binding()
            .refresh_with_source_witness()
            .expect("live child source after provider response");
        let Some(turn) = run_turn(witness.snapshot(), &turn_id) else {
            assert!(
                Instant::now() < deadline,
                "child turn vanished before failure settlement"
            );
            tick(host);
            continue;
        };
        if turn.status == "error"
            && witness
                .authority()
                .sessions
                .get(&child_id)
                .is_some_and(|session| session.queue.is_empty() && !session.run_open)
            && witness
                .snapshot()
                .transcript
                .items
                .iter()
                .any(|item| item.turn_id == turn_id && item.failure_message().is_some())
        {
            break read_frame(&source, "failed", &turn_id, &marker);
        }
        assert!(
            Instant::now() < deadline,
            "provider failure or child queue retirement did not publish"
        );
        tick(host);
    };
    let failed_revision = failed["snapshot"]["revision"]
        .as_u64()
        .expect("failed publication revision");
    assert!(
        failed_revision > running_revision,
        "failed source revision must advance"
    );
    assert_eq!(
        failed["snapshot"]["connectionEpoch"].as_u64(),
        Some(connection_epoch)
    );
    emit(
        &json!({"phase":"failed","witness":failed,"parent":&parent,"childId":&child_id,"marker":&marker}),
    );

    let close_id = host
        .view
        .tree
        .get(host.view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap()
        .close_id();
    let close = press(host, close_id);
    host.commands(Ok(close));
    sync(host);
    assert!(
        !host.view.child_pane_active(&parent, &child_id),
        "original close control must leave child pane"
    );
    assert_eq!(
        source.binding().access(),
        PaneAccess::Missing,
        "closed child pointer must retire"
    );
    let retired_witness_rejected = source.binding().refresh_with_source_witness().is_err();
    assert!(
        retired_witness_rejected,
        "retired original child pointer returned a witness"
    );
    assert!(Arc::ptr_eq(
        host.view.current_binding().unwrap(),
        &parent_binding
    ));
    assert_eq!(
        parent_binding.current_run().unwrap().draft.text,
        parent_draft
    );
    let parent_selection_preserved = host
        .view
        .current_binding()
        .is_some_and(|binding| binding.scope() == &parent);
    assert!(parent_selection_preserved);

    json!({
        "accepted": true,
        "evidenceScope": "actual-child-pane-original-controls-and-source",
        "parent": parent,
        "childId": child_id,
        "childScope": child_scope,
        "connectionEpoch": connection_epoch,
        "runningRevision": running_revision,
        "failedRevision": failed_revision,
        "openedViaOriginalControl": true,
        "sentViaOriginalComposer": true,
        "closedViaOriginalControl": true,
        "parentDraftPreserved": parent_binding.current_run().unwrap().draft.text == parent_draft,
        "parentSelectionPreserved": parent_selection_preserved,
        "retiredWitnessRejected": retired_witness_rejected,
        "nativeInteraction": false,
        "physicalIme": false,
        "focusMode": "offscreen-semantic-controls"
    })
}
