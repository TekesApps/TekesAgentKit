use tekes_agent_components_ui::*;
use tekes_ui_controls::composition::*;
fn row(
    endpoint: &str,
    id: &str,
    path: Option<&str>,
    folder: Option<&str>,
    pin: bool,
) -> SidebarRow {
    SidebarRow {
        primary_folder_id: folder.map(str::to_owned),
        folder_path: path.map(str::to_owned),
        address: SessionAddress {
            endpoint_id: endpoint.into(),
            session_id: id.into(),
        },
        title: id.into(),
        activity: ThreadActivity {
            status: "idle".into(),
            is_stale: false,
        },
        pin: Some(ThreadPinPresentation { pinned: pin }),
        actions: ThreadActionsPresentation {
            endpoint: endpoint.into(),
            session_id: id.into(),
            label: "操作".into(),
            items: vec![],
        },
    }
}
fn grouped() -> SidebarPreferencesSnapshot {
    SidebarPreferencesSnapshot {
        revision: 1,
        preferences: SidebarPreferences {
            group_by_folder: true,
            ..Default::default()
        },
    }
}
#[test]
fn grouping_uses_original_folder_identity_and_applies_filter_before_pin_partition() {
    let rows = vec![
        row("pi", "z", Some("/first"), Some("id"), false),
        row("pi", "pin", Some("/pin"), None, true),
        row("dsh", "a", Some("/first"), Some("id"), false),
        row("pi", "b", Some("/renamed"), Some("id"), false),
        row("pi", "none", None, None, false),
    ];
    let preferences = grouped().preferences;
    let outline = sidebar_outline(&rows, &preferences, None, None).unwrap();
    assert_eq!(outline.pinned[0].address.session_id, "pin");
    assert_eq!(outline.folders.len(), 3);
    assert_eq!(
        outline.folders[0]
            .rows
            .iter()
            .map(|r| r.address.session_id.as_str())
            .collect::<Vec<_>>(),
        ["z", "b"]
    );
    assert_ne!(outline.folders[0].key, outline.folders[1].key);
    assert_eq!(outline.folders[0].path, Some("/first"));
    let key = rows[2].folder_key();
    let filtered = sidebar_outline(&rows, &preferences, None, Some(&key)).unwrap();
    assert!(filtered.pinned.is_empty());
    assert_eq!(filtered.folders.len(), 1);
    assert_eq!(filtered.folders[0].rows[0].address.session_id, "a");
    let mut sorted = preferences.clone();
    sorted.sort = SidebarSort::TitleAscending;
    let outline = sidebar_outline(&rows, &sorted, Some(str::cmp), None).unwrap();
    assert_eq!(outline.folders[0].rows[0].address.session_id, "a");
    assert_eq!(
        outline.folders[1]
            .rows
            .iter()
            .map(|r| r.address.session_id.as_str())
            .collect::<Vec<_>>(),
        ["b", "z"]
    );
    assert_eq!(
        sidebar_folder_title(Some("/work/中文目录"), "无目录"),
        "中文目录"
    );
    assert_eq!(sidebar_folder_title(None, "无目录"), "无目录");
}

#[test]
fn folder_filter_is_shared_by_pins_and_sessions_and_scoped_to_endpoint() {
    let rows = vec![
        row("pi", "pin-pi", Some("/pi"), Some("shared-id"), true),
        row("pi", "ordinary-pi", Some("/pi"), Some("shared-id"), false),
        row("dsh", "pin-dsh", Some("/dsh"), Some("shared-id"), true),
        row(
            "dsh",
            "ordinary-dsh",
            Some("/dsh"),
            Some("shared-id"),
            false,
        ),
    ];
    let preferences = grouped().preferences;
    let filter = rows[0].folder_key();
    let filtered = sidebar_outline(&rows, &preferences, None, Some(&filter)).unwrap();
    assert_eq!(
        filtered
            .pinned
            .iter()
            .map(|row| row.address.session_id.as_str())
            .collect::<Vec<_>>(),
        ["pin-pi"]
    );
    assert_eq!(filtered.folders.len(), 1);
    assert_eq!(
        filtered.folders[0].rows[0].address.session_id,
        "ordinary-pi"
    );

    let cleared = sidebar_outline(&rows, &preferences, None, None).unwrap();
    assert_eq!(cleared.pinned.len(), 2);
    assert_eq!(
        cleared
            .folders
            .iter()
            .flat_map(|folder| folder
                .rows
                .iter()
                .map(|row| row.address.session_id.as_str()))
            .collect::<Vec<_>>(),
        ["ordinary-pi", "ordinary-dsh"]
    );
}
#[test]
fn original_folder_disclosure_keeps_rows_and_retires_old_callbacks_without_navigation() {
    let input = SidebarInput {
        folder_filter: None,
        search_help: None,
        revision: 1,
        loaded: true,
        error: None,
        selected: None,
        rows: vec![
            row("pi", "a", Some("/work/a"), Some("one"), false),
            row("pi", "b", Some("/work/b"), None, false),
            row("pi", "pin", Some("/elsewhere"), None, true),
        ],
    };
    let key = input.rows[0].folder_key();
    let mut sidebar = Sidebar::new(711, SidebarLabels::default());
    sidebar.update_preferences(&grouped(), true).unwrap();
    sidebar.update(&input).unwrap();
    let root = sidebar.id();
    let header = sidebar.folder_header_id(&key).unwrap();
    let leaf = sidebar.row_id(&input.rows[0].address).unwrap();
    let mut host = sidebar.into_host().unwrap();
    let original = host
        .get(leaf)
        .unwrap()
        .component::<ButtonContent>()
        .unwrap() as *const _ as usize;
    host.accessibility_action(header, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    let sidebar = host.get(root).unwrap().component::<Sidebar>().unwrap();
    assert!(!sidebar.preferences_current(&grouped(), true));
    let changes = sidebar.take_preference_intents(&actions);
    assert_eq!(changes.len(), 1);
    assert_eq!(
        changes[0].value.collapsed_folders.as_slice(),
        std::slice::from_ref(&key)
    );
    assert!(sidebar.take_intents(&actions).is_empty());
    let closed = SidebarPreferencesSnapshot {
        revision: 2,
        preferences: changes[0].value.clone(),
    };
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update_preferences(&closed, true)
        .unwrap();
    assert!(!host.semantics().iter().any(|node| node.id == leaf));
    let sidebar = host.get(root).unwrap().component::<Sidebar>().unwrap();
    assert!(sidebar.take_preference_intents(&actions).is_empty());
    assert_ne!(sidebar.folder_header_id(&key), Some(header));
    assert_eq!(
        host.get(leaf)
            .unwrap()
            .component::<ButtonContent>()
            .unwrap() as *const _ as usize,
        original
    );
    let mut flat = closed;
    flat.revision += 1;
    flat.preferences.group_by_folder = false;
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update_preferences(&flat, true)
        .unwrap();
    assert!(host.semantics().iter().any(|node| node.id == leaf));
    assert_eq!(
        host.get(leaf)
            .unwrap()
            .component::<ButtonContent>()
            .unwrap() as *const _ as usize,
        original
    );
    flat.revision += 1;
    flat.preferences.group_by_folder = true;
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update_preferences(&flat, true)
        .unwrap();
    assert!(
        !host.semantics().iter().any(|node| node.id == leaf),
        "grouping remembers its own collapsed folder"
    );
}
#[test]
fn filter_and_primary_folder_metadata_are_atomic_and_never_retarget_a_session() {
    let mut input = SidebarInput {
        folder_filter: None,
        search_help: None,
        revision: 1,
        loaded: true,
        error: None,
        selected: None,
        rows: vec![
            row("pi", "a", Some("/work/a"), Some("one"), false),
            row("pi", "b", Some("/work/b"), None, false),
            row("pi", "pin", None, None, true),
        ],
    };
    let mut sidebar = Sidebar::new(712, SidebarLabels::default());
    sidebar.update_preferences(&grouped(), true).unwrap();
    sidebar.update(&input).unwrap();
    let a = sidebar.row_id(&input.rows[0].address).unwrap();
    let b = sidebar.row_id(&input.rows[1].address).unwrap();
    let pin = sidebar.row_id(&input.rows[2].address).unwrap();
    let key = input.rows[0].folder_key();
    let header = sidebar.folder_header_id(&key).unwrap();
    input.revision += 1;
    input.rows[0].folder_path = Some("/renamed/folder".into());
    sidebar.update(&input).unwrap();
    assert_eq!(sidebar.row_id(&input.rows[0].address), Some(a));
    assert_eq!(sidebar.folder_header_id(&key), Some(header));
    let mut conflict = input.clone();
    conflict.folder_filter = Some(key.clone());
    assert!(sidebar.update(&conflict).is_err());
    assert_eq!(sidebar.displayed_input(), Some(&input));
    input.revision += 1;
    input.folder_filter = Some(key);
    sidebar.update(&input).unwrap();
    assert_eq!(sidebar.row_id(&input.rows[1].address), None);
    assert_eq!(sidebar.row_id(&input.rows[0].address), Some(a));
    assert_eq!(sidebar.row_id(&input.rows[2].address), None);
    let root = sidebar.id();
    let mut host = sidebar.into_host().unwrap();
    let stale_actions = [ContentAction {
        target: pin,
        kind: ContentActionKind::Activate,
        source: ContentActionSource::Programmatic,
    }];
    assert!(
        host.get(root)
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .take_intents(&stale_actions)
            .is_empty()
    );
    let sidebar = host
        .get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap();
    input.revision += 1;
    input.folder_filter = None;
    sidebar.update(&input).unwrap();
    assert!(sidebar.row_id(&input.rows[2].address).is_some());
    assert_ne!(
        sidebar.row_id(&input.rows[1].address),
        Some(b),
        "filtered leaf callbacks retire"
    );
    let mut invalid = input.clone();
    invalid.revision += 1;
    invalid.rows[0].primary_folder_id = Some(String::new());
    assert!(sidebar.update(&invalid).is_err());
    assert_eq!(sidebar.displayed_input(), Some(&input));
}

#[tokio::test]
async fn folder_publications_and_filter_preserve_composition_while_header_navigation_is_blocked() {
    use std::sync::Arc;
    use tekes_agent_ui_bindings::{RunBinding, SidebarPublication};
    use tekes_agent_ui_demo::component_demo::ComponentDemo;
    let (runtime, _) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let binding = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let mut view = ComponentDemo::new([binding]).unwrap();
    let source = SidebarPublication {
        observation: Default::default(),
        activity_at: Default::default(),
        name_corpus: None,
        revision: 1,
        loaded: true,
        error: None,
        rows: vec![
            row("fixture", "a", Some("/original/a"), Some("one"), false),
            row("fixture", "b", Some("/original/b"), Some("two"), false),
        ],
    };
    let key = source.rows[0].folder_key();
    view.set_sidebar_source(Arc::new(source)).unwrap();
    view.sync().unwrap();
    view.tree
        .get_mut(view.sidebar_id())
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update_preferences(&grouped(), true)
        .unwrap();
    view.tree.focus(Some(view.ids().editor), false);
    assert!(
        view.event(&ContentEvent::Preedit {
            text: "中文组字".into(),
            selection: 1..3,
            clauses: vec![],
            replacement: None
        })
        .unwrap()
        .is_empty()
    );
    let editor = format!(
        "{:?}",
        (
            &view.editor().input.text,
            view.editor().input.selection(),
            &view.editor().input.marked,
            view.editor().input.external_baseline()
        )
    );
    let focus = view.tree.focused();
    assert!(
        view.select(0).is_err(),
        "composition still owns actual session navigation"
    );
    view.tree
        .get_mut(view.sidebar_id())
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .reconcile_navigation(&SidebarSelectionNavigation {
            revision: 1,
            selected: Some(RunScope::main("fixture", "a").address()),
        })
        .unwrap();
    view.set_sidebar_folder_filter(Some(key.clone())).unwrap();
    let header = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .folder_header_id(&key)
        .unwrap();
    assert!(
        view.accessibility_action(header, None, AccessibilityAction::Press)
            .unwrap()
            .is_empty()
    );
    let sidebar = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap();
    assert!(
        sidebar
            .preferences()
            .preferences
            .collapsed_folders
            .is_empty()
    );
    assert!(
        view.tree
            .semantics()
            .iter()
            .find(|node| node.id == header)
            .unwrap()
            .semantics
            .expanded
            .unwrap()
    );
    assert_eq!(
        format!(
            "{:?}",
            (
                &view.editor().input.text,
                view.editor().input.selection(),
                &view.editor().input.marked,
                view.editor().input.external_baseline()
            )
        ),
        editor
    );
    assert_eq!(view.tree.focused(), focus);
    assert_eq!(view.selected(), 0);
    assert!(view.binding().current_run().unwrap().submissions.is_empty());
    runtime.stop().await.unwrap();
}
