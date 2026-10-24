use tekes_agent_components_ui::*;
use tekes_ui_controls::composition::*;
fn row(id: &str, pin: bool) -> SidebarRow {
    SidebarRow {
        primary_folder_id: Some(id.into()),
        folder_path: Some(format!("/original/{id}")),
        address: RunScope::main("fixture", id).address(),
        title: id.into(),
        activity: ThreadActivity {
            status: "idle".into(),
            is_stale: false,
        },
        pin: Some(ThreadPinPresentation { pinned: pin }),
        actions: ThreadActionsPresentation {
            endpoint: "fixture".into(),
            session_id: id.into(),
            label: "操作".into(),
            items: vec![],
        },
    }
}
fn input() -> SidebarInput {
    SidebarInput {
        folder_filter: None,
        search_help: None,
        revision: 1,
        loaded: true,
        error: None,
        selected: Some(row("a", false).address),
        rows: vec![row("a", false), row("b", false), row("pin", true)],
    }
}
fn preferences() -> SidebarPreferencesSnapshot {
    SidebarPreferencesSnapshot {
        revision: 1,
        preferences: SidebarPreferences {
            group_by_folder: true,
            pinned_expanded: false,
            collapsed_folders: vec![row("a", false).folder_key(), row("b", false).folder_key()],
            ..Default::default()
        },
    }
}
fn setup() -> (
    ContentHost,
    ComponentId,
    SidebarInput,
    SidebarPreferencesSnapshot,
) {
    let input = input();
    let value = preferences();
    let mut sidebar = Sidebar::new(730, SidebarLabels::default());
    sidebar.update_preferences(&value, true).unwrap();
    sidebar.update(&input).unwrap();
    let root = sidebar.id();
    (sidebar.into_host().unwrap(), root, input, value)
}
fn visible(host: &ContentHost, id: ComponentId) -> bool {
    host.semantics().iter().any(|node| node.id == id)
}
#[test]
fn explicit_navigation_opens_once_without_persisting_and_manual_noop_collapse_stays_closed() {
    let (mut host, root, mut input, value) = setup();
    let key = input.rows[0].folder_key();
    let leaf = host
        .get(root)
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .row_id(&input.rows[0].address)
        .unwrap();
    let ptr = host
        .get(leaf)
        .unwrap()
        .component::<ButtonContent>()
        .unwrap() as *const _ as usize;
    assert!(!visible(&host, leaf));
    let event = SidebarSelectionNavigation {
        revision: 1,
        selected: input.selected.clone(),
    };
    assert!(
        host.get_mut(root)
            .unwrap()
            .component_mut::<Sidebar>()
            .unwrap()
            .reconcile_navigation(&event)
            .unwrap()
    );
    assert!(visible(&host, leaf));
    let sidebar = host.get(root).unwrap().component::<Sidebar>().unwrap();
    assert_eq!(sidebar.preferences(), &value);
    assert!(sidebar.preferences_current(&value, true));
    let header = sidebar.folder_header_id(&key).unwrap();
    host.accessibility_action(header, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    let changes = host
        .get(root)
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .take_preference_intents(&actions);
    assert_eq!(changes.len(), 1);
    assert_eq!(
        changes[0].value, value.preferences,
        "closing a navigation override does not toggle an already-collapsed durable preference"
    );
    assert!(
        !host
            .get(root)
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .preferences_current(&value, true)
    );
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update_preferences(&value, true)
        .unwrap();
    assert!(!visible(&host, leaf));
    let sidebar = host.get(root).unwrap().component::<Sidebar>().unwrap();
    assert_ne!(sidebar.folder_header_id(&key), Some(header));
    assert!(sidebar.take_preference_intents(&actions).is_empty());
    assert!(
        !host
            .get_mut(root)
            .unwrap()
            .component_mut::<Sidebar>()
            .unwrap()
            .reconcile_navigation(&event)
            .unwrap()
    );
    input.revision += 1;
    input.rows[0].title = "ordinary refresh".into();
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update(&input)
        .unwrap();
    assert!(!visible(&host, leaf));
    assert!(
        host.get_mut(root)
            .unwrap()
            .component_mut::<Sidebar>()
            .unwrap()
            .reconcile_navigation(&SidebarSelectionNavigation {
                revision: 2,
                ..event
            })
            .unwrap()
    );
    assert!(visible(&host, leaf));
    assert_eq!(
        host.get(leaf)
            .unwrap()
            .component::<ButtonContent>()
            .unwrap() as *const _ as usize,
        ptr
    );
}
#[test]
fn pending_restore_is_fenced_by_selection_and_filter_and_does_not_manufacture_rows() {
    let mut input = input();
    input.loaded = false;
    input.rows.clear();
    let value = preferences();
    let mut sidebar = Sidebar::new(731, SidebarLabels::default());
    sidebar.update_preferences(&value, true).unwrap();
    sidebar.update(&input).unwrap();
    let event = SidebarSelectionNavigation {
        revision: 4,
        selected: input.selected.clone(),
    };
    assert!(!sidebar.reconcile_navigation(&event).unwrap());
    assert_eq!(sidebar.pending_navigation(), Some(event.clone()));
    input.revision += 1;
    input.loaded = true;
    sidebar.update(&input).unwrap();
    assert_eq!(sidebar.pending_navigation(), Some(event.clone()));
    assert!(sidebar.row_id(&row("a", false).address).is_none());
    input.revision += 1;
    input.rows = vec![row("a", false), row("b", false)];
    input.folder_filter = Some(row("b", false).folder_key());
    sidebar.update(&input).unwrap();
    assert!(sidebar.pending_navigation().is_none());
    assert!(sidebar.row_id(&row("a", false).address).is_none());
    input.revision += 1;
    input.folder_filter = None;
    sidebar.update(&input).unwrap();
    let leaf = sidebar.row_id(&row("a", false).address).unwrap();
    let host = sidebar.into_host().unwrap();
    assert!(
        !visible(&host, leaf),
        "changing a filter is not another navigation event"
    );
}
#[test]
fn old_conflicting_and_wrong_destination_navigation_cannot_reopen_a_section() {
    let (mut host, root, mut input, value) = setup();
    let event = SidebarSelectionNavigation {
        revision: 5,
        selected: input.selected.clone(),
    };
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .reconcile_navigation(&event)
        .unwrap();
    input.revision += 1;
    input.selected = Some(row("b", false).address);
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update(&input)
        .unwrap();
    for stale in [
        SidebarSelectionNavigation {
            revision: 4,
            selected: input.selected.clone(),
        },
        SidebarSelectionNavigation {
            revision: 5,
            selected: input.selected.clone(),
        },
        SidebarSelectionNavigation {
            revision: 6,
            selected: event.selected.clone(),
        },
    ] {
        assert!(
            host.get_mut(root)
                .unwrap()
                .component_mut::<Sidebar>()
                .unwrap()
                .reconcile_navigation(&stale)
                .is_err()
        );
    }
    let sidebar = host.get(root).unwrap().component::<Sidebar>().unwrap();
    let b = sidebar.row_id(&input.rows[1].address).unwrap();
    assert_eq!(sidebar.preferences(), &value);
    assert!(!visible(&host, b));
    assert!(
        host.get_mut(root)
            .unwrap()
            .component_mut::<Sidebar>()
            .unwrap()
            .reconcile_navigation(&SidebarSelectionNavigation {
                revision: 6,
                selected: input.selected.clone()
            })
            .unwrap()
    );
    assert!(visible(&host, b));
}
#[test]
fn pinned_navigation_obeys_folder_filter_and_preserves_pin_expansion_preference() {
    let (mut host, root, mut input, value) = setup();
    let pin = host
        .get(root)
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .row_id(&input.rows[2].address)
        .unwrap();
    input.revision += 1;
    input.selected = Some(row("pin", true).address);
    input.folder_filter = Some(row("b", false).folder_key());
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update(&input)
        .unwrap();
    assert!(!visible(&host, pin));
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .reconcile_navigation(&SidebarSelectionNavigation {
            revision: 1,
            selected: input.selected.clone(),
        })
        .unwrap();
    assert!(!visible(&host, pin));
    assert!(
        host.get(root)
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .visibility_target()
            .is_none()
    );

    input.revision += 1;
    input.folder_filter = Some(input.rows[2].folder_key());
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update(&input)
        .unwrap();
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .reconcile_navigation(&SidebarSelectionNavigation {
            revision: 2,
            selected: input.selected.clone(),
        })
        .unwrap();
    let pin = host
        .get(root)
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .row_id(&input.rows[2].address)
        .unwrap();
    assert!(visible(&host, pin));
    let sidebar = host.get(root).unwrap().component::<Sidebar>().unwrap();
    assert_eq!(
        sidebar.visibility_target().unwrap().address(),
        &input.rows[2].address
    );
    assert_eq!(sidebar.preferences(), &value);
    let header = sidebar.pinned_header_id().unwrap();
    host.accessibility_action(header, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    let changes = host
        .get(root)
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .take_preference_intents(&actions);
    assert_eq!(changes[0].value, value.preferences);
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update_preferences(&value, true)
        .unwrap();
    assert!(!visible(&host, pin));
}

#[test]
fn manual_folder_choice_supersedes_an_unresolved_navigation_before_the_row_arrives() {
    let value = preferences();
    let mut input = input();
    input.rows = vec![row("b", false)];
    input.rows[0].primary_folder_id = Some("a".into());
    let mut sidebar = Sidebar::new(732, SidebarLabels::default());
    sidebar.update_preferences(&value, true).unwrap();
    sidebar.update(&input).unwrap();
    let event = SidebarSelectionNavigation {
        revision: 1,
        selected: input.selected.clone(),
    };
    assert!(!sidebar.reconcile_navigation(&event).unwrap());
    let root = sidebar.id();
    let key = row("a", false).folder_key();
    let mut host = sidebar.into_host().unwrap();
    // Expand and then collapse this known group while the selected leaf is absent.
    let mut current = value;
    for _ in 0..2 {
        let header = host
            .get(root)
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .folder_header_id(&key)
            .unwrap();
        host.accessibility_action(header, None, AccessibilityAction::Press);
        let actions = host.drain_actions();
        let change = host
            .get(root)
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .take_preference_intents(&actions)
            .pop()
            .unwrap();
        current = SidebarPreferencesSnapshot {
            revision: current.revision + 1,
            preferences: change.value,
        };
        host.get_mut(root)
            .unwrap()
            .component_mut::<Sidebar>()
            .unwrap()
            .update_preferences(&current, true)
            .unwrap();
    }
    input.revision += 1;
    input.rows.push(row("a", false));
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update(&input)
        .unwrap();
    let sidebar = host.get(root).unwrap().component::<Sidebar>().unwrap();
    assert!(sidebar.pending_navigation().is_none());
    let leaf = sidebar.row_id(&row("a", false).address).unwrap();
    assert!(!visible(&host, leaf));
    assert!(
        !host
            .get_mut(root)
            .unwrap()
            .component_mut::<Sidebar>()
            .unwrap()
            .reconcile_navigation(&event)
            .unwrap()
    );
    assert!(!visible(&host, leaf));
}
#[test]
fn preference_failure_rolls_back_and_unrelated_publications_keep_navigation_expansion() {
    let (mut host, root, input, mut value) = setup();
    let event = SidebarSelectionNavigation {
        revision: 1,
        selected: input.selected.clone(),
    };
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .reconcile_navigation(&event)
        .unwrap();
    let key = input.rows[0].folder_key();
    let leaf = host
        .get(root)
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .row_id(&input.rows[0].address)
        .unwrap();
    value.revision += 1;
    value
        .preferences
        .pin_order
        .push(input.rows[2].address.clone());
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update_preferences(&value, true)
        .unwrap();
    assert!(visible(&host, leaf));
    let header = host
        .get(root)
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .folder_header_id(&key)
        .unwrap();
    host.accessibility_action(header, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    host.get(root)
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .take_preference_intents(&actions);
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update_preferences(&value, true)
        .unwrap();
    assert!(!visible(&host, leaf));
    // An explicit expansion would persist a changed value. An unchanged source
    // after a failed write must restore the authoritative closed state.
    let header = host
        .get(root)
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .folder_header_id(&key)
        .unwrap();
    host.accessibility_action(header, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    let change = host
        .get(root)
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .take_preference_intents(&actions)
        .pop()
        .unwrap();
    assert_ne!(change.value, value.preferences);
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update_preferences(&value, true)
        .unwrap();
    assert!(!visible(&host, leaf));
}

#[tokio::test]
async fn actual_demo_navigation_uses_existing_revision_and_never_turns_sync_into_navigation() {
    use std::sync::Arc;
    use tekes_agent_ui_bindings::{RunBinding, SidebarPublication};
    use tekes_agent_ui_demo::component_demo::{ComponentDemo, DemoCommand};
    let (runtime, _) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let a = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let b = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "b"))
            .await
            .unwrap(),
    );
    a.edit_text(
        &ComposerDestination::run(a.scope().clone()),
        "会话a草稿".into(),
    )
    .unwrap();
    b.edit_text(
        &ComposerDestination::run(b.scope().clone()),
        "会话b草稿".into(),
    )
    .unwrap();
    let mut view = ComponentDemo::new([a, b]).unwrap();
    let mut source = SidebarPublication {
        observation: Default::default(),
        activity_at: Default::default(),
        name_corpus: None,
        revision: 1,
        loaded: true,
        error: None,
        rows: input().rows,
    };
    view.set_sidebar_source(Arc::new(source.clone())).unwrap();
    view.sync().unwrap();
    let value = preferences();
    view.set_sidebar_preferences(&value, true).unwrap();
    let key = source.rows[0].folder_key();
    let leaf = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .row_id(&source.rows[0].address)
        .unwrap();
    assert!(!visible(&view.tree, leaf));
    view.sync().unwrap();
    assert!(!visible(&view.tree, leaf));
    view.select(0).unwrap();
    assert!(visible(&view.tree, leaf));
    let header = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .folder_header_id(&key)
        .unwrap();
    let commands = view
        .accessibility_action(header, None, AccessibilityAction::Press)
        .unwrap();
    assert_eq!(commands.len(), 1);
    let DemoCommand::SidebarPreferences(change) = &commands[0] else {
        panic!("only disclosure preference command is expected")
    };
    assert_eq!(change.value, value.preferences);
    view.set_sidebar_preferences(&value, true).unwrap();
    assert!(!visible(&view.tree, leaf));
    source.revision += 1;
    source.rows[0].title = "普通库存更新".into();
    view.set_sidebar_source(Arc::new(source)).unwrap();
    view.sync().unwrap();
    assert!(!visible(&view.tree, leaf));
    assert_eq!(view.editor().input.text, "会话a草稿");
    view.select(1).unwrap();
    assert_eq!(view.editor().input.text, "会话b草稿");
    view.select(0).unwrap();
    assert!(visible(&view.tree, leaf));
    assert_eq!(view.editor().input.text, "会话a草稿");
    assert_eq!(
        view.tree
            .get(view.sidebar_id())
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .preferences(),
        &value
    );
    assert!(view.binding().current_run().unwrap().submissions.is_empty());
    runtime.stop().await.unwrap();
}
