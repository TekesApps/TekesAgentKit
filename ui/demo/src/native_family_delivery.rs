//! Owned offscreen integration: original controls and original host completion handler.
//! The manifest is an assertion oracle only; it never installs tasks or panes.
use super::*;
use serde_json::{Value, json};
use tekes_agent_components_ui::{FamilyCatalog, SidePanelTabKind};
use tekes_agent_contract::SessionAddress;
use tekes_agent_presentation::ComposerDestination;
use tekes_agent_ui_demo::{component_demo::FAMILY_REFRESH, family_seed_manifest::FamilySeedCase};

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
fn wait(host: &mut Host, predicate: impl Fn(&Host) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        host.drain_results(None);
        sync(host);
        assert!(
            host.failures.is_empty(),
            "family host errors: {:?}",
            host.failures
        );
        if predicate(host) {
            return;
        }
        assert!(Instant::now() < deadline, "family completion deadline");
        tick(host);
    }
}
fn press(host: &mut Host, id: ComponentId) {
    assert!(
        host.view.tree.get(id).is_some(),
        "original control must be mounted"
    );
    let commands = host
        .view
        .accessibility_action(id, None, AccessibilityAction::Press);
    host.commands(commands);
    sync(host);
}
fn select(host: &mut Host, parent: &RunScope) {
    let address = SessionAddress {
        endpoint_id: parent.endpoint_id.clone(),
        session_id: parent.session_id.clone(),
    };
    let deadline = Instant::now() + Duration::from_secs(15);
    let id = loop {
        sync(host);
        let sidebar = host
            .view
            .tree
            .get(host.view.sidebar_id())
            .unwrap()
            .component::<Sidebar>()
            .unwrap();
        if let Some(id) = sidebar.row_id(&address) {
            break id;
        }
        if let Some(more) = sidebar.page_button_id() {
            press(host, more);
        }
        assert!(
            Instant::now() < deadline,
            "owned parent row absent: {parent:?}"
        );
        tick(host);
    };
    press(host, id);
    assert_eq!(host.view.current_binding().unwrap().scope(), parent);
}
fn child_button(host: &Host, child: &str) -> ComponentId {
    host.view
        .tree
        .get(host.view.family_catalog_id().unwrap())
        .unwrap()
        .component::<FamilyCatalog>()
        .unwrap()
        .child_button_id(child)
        .expect("actual catalog child")
}
fn tab(host: &Host, kind: &SidePanelTabKind) -> ComponentId {
    host.view
        .tree
        .get(host.view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap()
        .tab_id(kind)
        .expect("original panel tab")
}

fn hold_original_open(host: &mut Host, button: ComponentId) -> HostResult {
    let commands = host
        .view
        .accessibility_action(button, None, AccessibilityAction::Press)
        .unwrap();
    assert!(matches!(
        commands.as_slice(),
        [DemoCommand::OpenChild { .. }]
    ));
    host.commands(Ok(commands));
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match host.results.try_recv() {
            Ok(result @ HostResult::ChildOpened { result: Ok(_), .. }) => break result,
            Ok(HostResult::ChildOpened {
                result: Err(error), ..
            }) => panic!("held child source failed: {error}"),
            Ok(other) => host.apply_result(other, None),
            Err(_) => {
                assert!(
                    Instant::now() < deadline,
                    "held original child result deadline"
                );
                tick(host);
            }
        }
    }
}

pub(super) fn run(host: &mut Host, seeds: &[FamilySeedCase]) -> Value {
    host.view.enable_family_catalog().unwrap();
    let mut cases = Vec::new();
    for seed in seeds {
        select(host, &seed.parent);
        let binding = host.view.current_binding().unwrap().clone();
        let draft = format!(
            "家族验收草稿:{}:{}",
            seed.endpoint_id, seed.parent.session_id
        );
        binding
            .edit_text(
                &ComposerDestination::run(seed.parent.clone()),
                draft.clone(),
            )
            .unwrap();
        sync(host);
        press(host, FAMILY_REFRESH);
        wait(host, |host| {
            host.view
                .current_family_catalog_source()
                .unwrap()
                .snapshot()
                .catalog
                .as_ref()
                .is_some_and(|catalog| catalog.child(&seed.child_id).is_some())
        });
        let catalog = host
            .view
            .current_family_catalog_source()
            .unwrap()
            .snapshot();
        assert_eq!(
            catalog
                .catalog
                .as_ref()
                .unwrap()
                .child(&seed.child_id)
                .unwrap()
                .mode,
            seed.catalog_mode
        );
        let summary = binding
            .current_run()
            .unwrap()
            .summary
            .clone()
            .expect("actual parent Summary");
        if let Some(task) = &seed.task {
            let actual = summary
                .tasks
                .iter()
                .find(|row| row.id == task.id)
                .expect("canonical parent Task");
            assert_eq!(actual, task);
            assert!(
                actual.independent_run().is_none(),
                "parent Task must not acquire independent Run"
            );
        }
        if let Some(id) = &seed.summary_subagent_id {
            assert!(
                summary.subagents.iter().any(|row| &row.id == id
                    && row.session_id.as_deref() == Some(seed.child_id.as_str()))
            );
        }
        let button = child_button(host, &seed.child_id);
        // Hold a real dispatched completion, switch via the original Sidebar,
        // then deliver it through the exact same production result handler.
        let held = hold_original_open(host, button);
        let parent_delivery = match &held {
            HostResult::ChildOpened { request, .. } => request.delivery_receipt(),
            _ => unreachable!(),
        };
        let alternate = seeds
            .iter()
            .find(|row| row.endpoint_id == seed.endpoint_id && row.parent != seed.parent)
            .unwrap();
        select(host, &alternate.parent);
        host.apply_result(held, None);
        sync(host);
        assert!(!host.view.child_pane_active(&seed.parent, &seed.child_id));
        assert!(
            !host
                .view
                .child_pane_active(&alternate.parent, &seed.child_id)
        );
        select(host, &seed.parent);
        let button = child_button(host, &seed.child_id);
        press(host, button);
        wait(host, |host| {
            host.view.child_pane_active(&seed.parent, &seed.child_id)
        });
        let layout = host
            .sidebar
            .child_history_layout(&seed.endpoint_id)
            .unwrap();
        let source = host
            .view
            .cached_child_source(&seed.parent, &seed.parent, &seed.child_id, layout)
            .unwrap();
        assert_eq!(source.parent(), &seed.parent);
        assert_eq!(source.child_id(), seed.child_id);
        let pane = host.executor.block_on(source.snapshot()).unwrap();
        let (scope, items, readonly) = match pane {
            PaneSnapshot::ReadOnlyChild {
                scope, transcript, ..
            } => (scope, transcript.items.len(), true),
            PaneSnapshot::Run { snapshot, .. } => (
                snapshot.run.as_ref().unwrap().scope.clone(),
                snapshot.transcript.items.len(),
                false,
            ),
            PaneSnapshot::Missing { .. } => panic!("actual child missing"),
        };
        assert_eq!(scope.endpoint_id, seed.endpoint_id);
        assert_eq!(scope.session_id, seed.child_id);
        assert!(items > 0, "actual child history must contain data");
        let mounted = if seed.endpoint_id == "native" {
            assert!(readonly && host.view.panel_composer_ids().is_none());
            host.view
                .bounded_child_content_id()
                .is_some_and(|id| host.view.tree.get(id).is_some())
        } else {
            assert!(!readonly);
            host.view
                .panel_composer_ids()
                .is_some_and(|ids| host.view.tree.get(ids.root).is_some())
        };
        assert!(mounted, "actual child content must be mounted");
        let summary_tab = tab(host, &SidePanelTabKind::Summary);
        press(host, summary_tab);
        assert!(!host.view.child_pane_active(&seed.parent, &seed.child_id));
        if let Some(task) = &seed.task {
            let summary_view = host
                .view
                .tree
                .get(host.view.summary_id())
                .unwrap()
                .component::<tekes_agent_components_ui::SummaryView>()
                .unwrap();
            let row = summary_view
                .row_id("tasks", &task.id)
                .expect("actual parent Task row");
            assert!(host.view.tree.get(row).is_some());
            assert!(summary_view.action_id("tasks", &task.id).is_none());
        }
        let child_tab = tab(
            host,
            &SidePanelTabKind::Subagent {
                child_id: seed.child_id.clone(),
            },
        );
        press(host, child_tab);
        assert!(host.view.child_pane_active(&seed.parent, &seed.child_id));
        // Capture a second real OpenChild completion while the original tab
        // exists, close that tab, then deliver the result after its tombstone.
        let held_after_close = hold_original_open(host, child_button(host, &seed.child_id));
        let close_delivery = match &held_after_close {
            HostResult::ChildOpened { request, .. } => request.delivery_receipt(),
            _ => unreachable!(),
        };
        let close = host
            .view
            .tree
            .get(host.view.panel_id())
            .unwrap()
            .component::<SidePanel>()
            .unwrap()
            .close_id();
        press(host, close);
        host.apply_result(held_after_close, None);
        sync(host);
        assert!(
            host.failures.is_empty(),
            "retired child delivery leaked an error"
        );
        assert!(
            host.view
                .cached_child_source(&seed.parent, &seed.parent, &seed.child_id, layout)
                .is_none()
        );
        assert!(!host.view.child_pane_active(&seed.parent, &seed.child_id));
        assert!(
            host.view
                .tree
                .get(host.view.panel_id())
                .unwrap()
                .component::<SidePanel>()
                .unwrap()
                .displayed_input()
                .unwrap()
                .tabs
                .iter()
                .all(|tab| tab.kind
                    != SidePanelTabKind::Subagent {
                        child_id: seed.child_id.clone()
                    })
        );
        assert_eq!(binding.current_run().unwrap().draft.text, draft);
        cases.push(json!({"endpointId":seed.endpoint_id,"parent":seed.parent,"childId":seed.child_id,
            "canonicalSourceMatched":true,"openedViaOriginalControl":true,"returnedViaOriginalTab":true,
            "closedViaOriginalControl":true,"lateParentResultDidNotStealSelection":true,"lateClosedResultRejected":true,"parentDraftPreserved":true,
            "childScope":scope,"historyItems":items,"childContentMounted":mounted,"readOnly":readonly,
            "catalogFrame":{"parent":catalog.parent,"epoch":catalog.connection_epoch,
                "generation":catalog.generation,"revision":catalog.revision,
                "entry":catalog.catalog.as_ref().unwrap().child(&seed.child_id)},
            "parentTaskRowMountedWithoutOpenAction": seed.task.is_some(),
            "canonicalTask":seed.task,"canonicalSummarySubagentId":seed.summary_subagent_id,
            "heldParentDelivery":parent_delivery,"heldClosedDelivery":close_delivery}));
    }
    assert!(host.failures.is_empty());
    json!({"accepted":true,"nativeInteraction":false,"physicalIme":false,"cases":cases,
        "sameTaskRowDynamicPointerProven":false,"desktopVisibilityProven":false})
}
