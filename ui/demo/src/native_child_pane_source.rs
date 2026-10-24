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
    assert!(matches!(
        parent.endpoint_id.as_str(),
        "native" | "pi" | "dsh"
    ));
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
    let expected_layout = if parent.endpoint_id == "native" {
        ChildHistoryLayout::SameSession
    } else {
        ChildHistoryLayout::IndependentSessions
    };
    assert_eq!(layout, expected_layout);
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
        matches!(open.as_slice(), [DemoCommand::OpenChild { request, direct_parent, child_id: id, layout: opened_layout, .. }]
        if *opened_layout == expected_layout
            && request.parent() == &parent
            && direct_parent == &parent
            && id == &child_id)
    );
    host.commands(Ok(open));
    wait(host, "original ChildPane open completion", |host| {
        host.view.child_pane_active(&parent, &child_id)
    });
    let source = host
        .view
        .cached_child_source(&parent, &parent, &child_id, expected_layout)
        .expect("original cached ChildPane source");
    assert_eq!(source.parent(), &parent);
    assert_eq!(source.child_id(), child_id);
    assert_eq!(source.binding().access(), PaneAccess::Ready);
    let child_scope = source.binding().scope().clone();
    assert_eq!(child_scope.endpoint_id, parent.endpoint_id);
    assert_eq!(child_scope.thread_id, parent.thread_id);
    assert_eq!(child_scope.session_id, child_id);

    if parent.endpoint_id == "native" {
        return run_native_readonly(
            host,
            parent,
            child_id,
            marker,
            parent_binding,
            parent_draft,
            source,
        );
    }
    assert_eq!(expected_layout, ChildHistoryLayout::IndependentSessions);

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

/// Native's original child control opens a read-only projection backed by a
/// single bounded head read. The exact resulting PaneSnapshot is admitted by
/// the same HostResult path used for normal child reads.
fn run_native_readonly(
    host: &mut Host,
    parent: RunScope,
    child_id: String,
    marker: String,
    parent_binding: Arc<RunBinding>,
    parent_draft: String,
    source: Arc<ChildPaneBinding>,
) -> Value {
    let child_scope = source.binding().scope().clone();
    assert_eq!(source.binding().access(), PaneAccess::Ready);
    let parent_selection = parent_binding
        .current_run()
        .expect("original parent Run")
        .selection
        .clone();
    let initial = host
        .executor
        .block_on(source.binding().snapshot_with_bounded_source_witness())
        .expect("Native original bounded child source witness");
    let PaneSnapshot::ReadOnlyChild {
        scope,
        publication,
        connection_epoch,
        transcript,
        ..
    } = &initial.snapshot
    else {
        panic!("Native child pane must remain read-only");
    };
    assert_eq!(scope, &child_scope);
    assert_eq!(initial.source.child_id, child_id);
    assert_eq!(initial.source.parent_scope, parent);
    assert_eq!(
        initial.source.page_size,
        initial.source.head_request.max_messages
    );
    assert_eq!(
        initial.source.head_request.parent_session_id,
        parent.session_id
    );
    assert_eq!(initial.source.head_request.child_session_id, child_id);
    assert_eq!(
        initial.source.evidence_scope,
        "built-in-bounded-head-only-same-local-read"
    );
    assert!(!initial.source.server_transactional_freshness_proven);
    let raw = initial
        .source
        .raw_head
        .source_call
        .as_ref()
        .expect("Native bounded head sourceCall");
    let call_id = raw["callId"].as_str().expect("sourceCall.callId");
    assert!(raw["arguments"].to_string().contains(&marker));
    assert_eq!(raw["phase"], "running");
    let turn_id = json!([
        child_scope.endpoint_id,
        child_scope.session_id,
        "task-source",
        call_id
    ])
    .to_string();
    let running_turn = transcript
        .turns
        .iter()
        .find(|turn| turn.id == turn_id)
        .expect("synthetic task turn bound to sourceCall.callId");
    assert_eq!(running_turn.status, "running");
    assert!(transcript.tools.iter().any(|tool| {
        tool.turn_id == turn_id && tool.id == format!("{turn_id}:call") && tool.phase == "running"
    }));
    let native_epoch = *connection_epoch;
    let running_publication = *publication;
    let running_frame = native_frame(
        "running",
        &initial.snapshot,
        &initial.source,
        &parent,
        &child_id,
        &marker,
        &turn_id,
    );

    // Admit this same snapshot through the host's original cached-pane result
    // handler, then inspect the mounted ReadOnlyTranscript tree.
    host.apply_result(
        HostResult::ChildRead {
            panel_parent: parent.clone(),
            source: source.clone(),
            result: Ok(initial.snapshot.clone()),
            catalog_error: None,
        },
        None,
    );
    sync(host);
    assert!(host.view.child_pane_active(&parent, &child_id));
    let (mounted_source, mounted_snapshot) = host
        .view
        .acceptance_child_pane_snapshot(&parent, &child_id)
        .expect("actual cached mounted child snapshot");
    assert!(Arc::ptr_eq(mounted_source, &source));
    let mounted_snapshot_json = pane_snapshot_json(mounted_snapshot);
    assert_eq!(mounted_snapshot_json, running_frame["snapshot"]);
    let mounted_id = host
        .view
        .bounded_child_view_id()
        .expect("original mounted Native read-only Transcript");
    let mounted = host
        .view
        .tree
        .get(mounted_id)
        .and_then(|node| node.component::<tekes_agent_components_ui::ReadOnlyTranscript>())
        .expect("mounted Native ReadOnlyTranscript component");
    assert_eq!(mounted.content().scope(), &child_scope);
    let mounted_rows = mounted.content().row_keys().len();
    assert!(
        mounted_rows > 0,
        "original mounted transcript has no reading rows"
    );
    let mounted_publication = publication;
    let mounted_epoch = *connection_epoch;
    emit(&json!({
        "phase":"running",
        "parent":parent,
        "childId":child_id,
        "marker":marker,
        "turnId":turn_id,
        "witness":running_frame,
        "mountedSnapshot":mounted_snapshot_json,
        "mounted":{
            "sourceScope":scope,
            "publication":mounted_publication,
            "connectionEpoch":mounted_epoch,
            "mountedContentRows":mounted_rows,
            "transcript":transcript,
            "readOnly":true,
            "mountedControl":"original-ReadOnlyTranscript"
        }
    }));

    let deadline = Instant::now() + Duration::from_secs(60);
    let (failed_frame, failed_snapshot) = loop {
        host.drain_results(None);
        sync(host);
        assert!(
            host.failures.is_empty(),
            "Native child read failures: {:?}",
            host.failures
        );
        let witness = host
            .executor
            .block_on(source.binding().snapshot_with_bounded_source_witness())
            .expect("Native bounded child source after provider release");
        let Some(raw) = witness.source.raw_head.source_call.as_ref() else {
            assert!(Instant::now() < deadline, "Native sourceCall disappeared");
            tick(host);
            continue;
        };
        if raw["phase"] == "error" {
            let PaneSnapshot::ReadOnlyChild {
                transcript,
                connection_epoch,
                publication: failed_publication,
                ..
            } = &witness.snapshot
            else {
                panic!("Native terminal pane was not read-only")
            };
            let terminal_turn = transcript
                .turns
                .iter()
                .find(|turn| turn.id == turn_id)
                .expect("projected synthetic task turn retained sourceCall identity");
            assert_eq!(terminal_turn.status, "error");
            assert!(transcript.tools.iter().any(|tool| tool.turn_id == turn_id
                && tool.id == format!("{turn_id}:call")
                && tool.phase == "error"));
            assert_eq!(
                witness.source.raw_head.source_call.as_ref().unwrap()["callId"],
                call_id
            );
            assert_eq!(witness.source.parent_scope, parent);
            assert_eq!(*connection_epoch, mounted_epoch);
            assert!(witness.source.raw_head.entries.len() > initial.source.raw_head.entries.len());
            assert!(
                witness
                    .source
                    .raw_head
                    .entries
                    .starts_with(&initial.source.raw_head.entries)
            );
            assert!(
                transcript
                    .items
                    .iter()
                    .any(|item| item.failure_message().is_some())
            );
            assert!(*failed_publication > running_publication);
            let frame = native_frame(
                "failed",
                &witness.snapshot,
                &witness.source,
                &parent,
                &child_id,
                &marker,
                &turn_id,
            );
            host.apply_result(
                HostResult::ChildRead {
                    panel_parent: parent.clone(),
                    source: source.clone(),
                    result: Ok(witness.snapshot.clone()),
                    catalog_error: None,
                },
                None,
            );
            sync(host);
            let (mounted_source, mounted_snapshot) = host
                .view
                .acceptance_child_pane_snapshot(&parent, &child_id)
                .expect("actual cached failed child snapshot");
            assert!(Arc::ptr_eq(mounted_source, &source));
            let mounted_snapshot_json = pane_snapshot_json(mounted_snapshot);
            assert_eq!(mounted_snapshot_json, frame["snapshot"]);
            break (frame, witness.snapshot);
        }
        assert!(
            Instant::now() < deadline,
            "Native bounded source did not publish phase error"
        );
        tick(host);
    };
    let PaneSnapshot::ReadOnlyChild {
        scope,
        publication,
        connection_epoch,
        transcript,
        ..
    } = &failed_snapshot
    else {
        panic!("Native failed pane snapshot")
    };
    assert_eq!(scope, &child_scope);
    assert_eq!(*connection_epoch, mounted_epoch);
    assert_eq!(failed_frame["serverTransactionalFreshnessProven"], false);
    let (mounted_source, mounted_snapshot) = host
        .view
        .acceptance_child_pane_snapshot(&parent, &child_id)
        .expect("actual cached mounted failed snapshot");
    assert!(Arc::ptr_eq(mounted_source, &source));
    let mounted_snapshot_json = pane_snapshot_json(mounted_snapshot);
    assert_eq!(mounted_snapshot_json, failed_frame["snapshot"]);
    let mounted_id = host
        .view
        .bounded_child_view_id()
        .expect("mounted failed child Transcript");
    let mounted = host
        .view
        .tree
        .get(mounted_id)
        .and_then(|node| node.component::<tekes_agent_components_ui::ReadOnlyTranscript>())
        .expect("mounted failed ReadOnlyTranscript");
    assert_eq!(mounted.content().scope(), &child_scope);
    let mounted_rows = mounted.content().row_keys().len();
    assert!(
        mounted_rows > 0,
        "failed original transcript has no reading rows"
    );
    emit(&json!({
        "phase":"failed",
        "parent":parent,
        "childId":child_id,
        "marker":marker,
        "turnId":turn_id,
        "witness":failed_frame,
        "mountedSnapshot":mounted_snapshot_json,
        "mounted":{"sourceScope":scope,"publication":publication,"connectionEpoch":connection_epoch,
            "mountedContentRows":mounted_rows,
            "transcript":transcript,"readOnly":true,"mountedControl":"original-ReadOnlyTranscript"}
    }));

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
    assert!(!host.view.child_pane_active(&parent, &child_id));
    assert_eq!(source.binding().access(), PaneAccess::Missing);
    assert!(
        host.executor
            .block_on(source.binding().snapshot_with_bounded_source_witness())
            .is_err()
    );
    assert!(Arc::ptr_eq(
        host.view.current_binding().unwrap(),
        &parent_binding
    ));
    assert_eq!(
        parent_binding.current_run().unwrap().draft.text,
        parent_draft
    );
    assert_eq!(
        parent_binding.current_run().unwrap().selection,
        parent_selection
    );
    json!({
        "accepted":true,
        "evidenceScope":"actual-native-readonly-child-pane-original-controls-and-bounded-head",
        "parent":parent,"childId":child_id,"childScope":child_scope,
        "runningPublication":running_frame["snapshot"]["publication"],
        "failedPublication":failed_frame["snapshot"]["publication"],
        "connectionEpoch":native_epoch,
        "readOnly":true,"headOnly":true,"serverTransactionalFreshnessProven":false,
        "openedViaOriginalControl":true,"sentViaOriginalComposer":false,
        "sentChildCommand":false,"closedViaOriginalControl":true,
        "parentDraftPreserved":parent_binding.current_run().unwrap().draft.text == parent_draft,
        "parentSelectionPreserved":parent_binding.current_run().unwrap().selection == parent_selection,
        "retiredWitnessRejected":true,"nativeInteraction":false,"physicalIme":false
    })
}

fn native_frame(
    phase: &str,
    snapshot: &PaneSnapshot,
    source: &tekes_agent_presentation::BoundedChildSourceWitness,
    parent: &RunScope,
    child_id: &str,
    marker: &str,
    turn_id: &str,
) -> Value {
    let PaneSnapshot::ReadOnlyChild { transcript, .. } = snapshot else {
        panic!("Native bounded source must pair a read-only PaneSnapshot")
    };
    assert_eq!(
        source.raw_head.source_call.as_ref().unwrap()["phase"],
        phase_as_raw(phase)
    );
    let turn = transcript
        .turns
        .iter()
        .find(|turn| turn.id == turn_id)
        .expect("synthetic task turn in original projection");
    assert_eq!(
        turn.status,
        if phase == "running" {
            "running"
        } else {
            "error"
        }
    );
    json!({
        "phase":phase,
        "snapshot":pane_snapshot_json(snapshot),
        "source":source,
        "parent":parent,"childId":child_id,"marker":marker,"turnId":turn_id,
        "readOnly":true,"headOnly":true,"serverTransactionalFreshnessProven":false
    })
}

fn pane_snapshot_json(snapshot: &PaneSnapshot) -> Value {
    let PaneSnapshot::ReadOnlyChild {
        scope,
        publication,
        connection_epoch,
        connection,
        transcript,
        ..
    } = snapshot
    else {
        panic!("Native bounded source must pair a read-only PaneSnapshot")
    };
    json!({
        "scope":scope,
        "publication":publication,
        "connectionEpoch":connection_epoch,
        "connection":connection,
        "transcript":transcript
    })
}

fn phase_as_raw(phase: &str) -> &'static str {
    match phase {
        "running" => "running",
        "failed" => "error",
        _ => unreachable!(),
    }
}
