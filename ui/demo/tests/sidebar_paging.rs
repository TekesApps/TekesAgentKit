use tekes_agent_components_ui::*;
use tekes_ui_controls::composition::*;
fn row(id: usize, pin: bool) -> SidebarRow {
    SidebarRow {
        primary_folder_id: None,
        folder_path: Some("/work".into()),
        address: RunScope::main("fixture", format!("s{id}")).address(),
        title: format!("{id}"),
        activity: ThreadActivity {
            status: "idle".into(),
            is_stale: false,
        },
        pin: Some(ThreadPinPresentation { pinned: pin }),
        actions: ThreadActionsPresentation {
            endpoint: "fixture".into(),
            session_id: format!("s{id}"),
            label: "操作".into(),
            items: vec![],
        },
    }
}
fn input() -> SidebarInput {
    SidebarInput {
        revision: 1,
        loaded: true,
        error: None,
        folder_filter: None,
        search_help: None,
        selected: Some(row(12, false).address),
        rows: (0..13).map(|i| row(i, i == 0)).collect(),
    }
}
#[test]
fn page_is_five_ordinary_plus_global_pins_and_selected_outside_the_page() {
    let input = input();
    let mut sidebar = Sidebar::new(780, Default::default());
    sidebar.update(&input).unwrap();
    assert_eq!(sidebar.paging().ordinary_count, 12);
    assert!(sidebar.paging().has_more);
    assert!(sidebar.paging().selected_supplemented);
    assert!(sidebar.row_id(&row(0, true).address).is_some());
    assert!(sidebar.row_id(&row(12, false).address).is_some());
    assert!(sidebar.row_id(&row(6, false).address).is_none());
    let root = sidebar.id();
    let more = sidebar.page_button_id().unwrap();
    let mut host = sidebar.into_host().unwrap();
    host.accessibility_action(more, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    let request = host
        .get(root)
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .take_page_requests(&actions)
        .pop()
        .unwrap();
    let sidebar = host
        .get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap();
    assert!(sidebar.load_more(&request).unwrap());
    assert!(!sidebar.load_more(&request).unwrap());
    assert_eq!(sidebar.paging().limit, 10);
    assert!(sidebar.row_id(&row(10, false).address).is_some());
    assert!(sidebar.row_id(&row(11, false).address).is_none());
    assert!(sidebar.take_page_requests(&actions).is_empty());
    let mut remount = Sidebar::new(780, Default::default());
    remount.update(&input).unwrap();
    assert!(!remount.load_more(&request).unwrap());
}
#[test]
fn ordinary_refresh_invalidates_page_callback_and_does_not_reset_requested_count() {
    let mut input = input();
    let mut sidebar = Sidebar::new(781, Default::default());
    sidebar.update(&input).unwrap();
    let root = sidebar.id();
    let mut host = sidebar.into_host().unwrap();
    let more = host
        .get(root)
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .page_button_id()
        .unwrap();
    host.accessibility_action(more, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    let request = host
        .get(root)
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .take_page_requests(&actions)
        .pop()
        .unwrap();
    let sidebar = host
        .get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap();
    input.revision += 1;
    sidebar.update(&input).unwrap();
    assert!(!sidebar.load_more(&request).unwrap());
    assert_eq!(sidebar.paging().limit, 5);
}
#[test]
fn search_is_not_repaged_or_supplemented_and_folder_filter_keeps_pins() {
    let input = input();
    let prefs = SidebarPreferences::default();
    let (out, facts) = sidebar_paged_outline(
        &input.rows,
        &prefs,
        None,
        None,
        input.selected.as_ref(),
        5,
        true,
    )
    .unwrap();
    assert_eq!(out.sessions.len(), 12);
    assert!(!facts.has_more);
    assert!(!facts.selected_supplemented);
    let key = SidebarFolderKey {
        endpoint_id: "fixture".into(),
        identity: SidebarFolderIdentity::Path("/other".into()),
    };
    let (out, facts) = sidebar_paged_outline(
        &input.rows,
        &prefs,
        None,
        Some(&key),
        input.selected.as_ref(),
        5,
        false,
    )
    .unwrap();
    assert_eq!(out.pinned.len(), 1);
    assert!(out.sessions.is_empty());
    assert!(!facts.selected_supplemented);
    assert!(sidebar_paged_outline(&input.rows, &prefs, None, None, None, 0, false).is_err());
}
#[test]
fn selected_supplement_preserves_folder_navigation_and_manual_collapse() {
    let mut input = input();
    input.rows[12].primary_folder_id = Some("outside-page".into());
    let key = input.rows[12].folder_key();
    let prefs = SidebarPreferencesSnapshot {
        revision: 1,
        preferences: SidebarPreferences {
            group_by_folder: true,
            collapsed_folders: vec![key.clone()],
            ..Default::default()
        },
    };
    let mut sidebar = Sidebar::new(782, Default::default());
    sidebar.update_preferences(&prefs, true).unwrap();
    sidebar.update(&input).unwrap();
    assert!(
        sidebar
            .reconcile_navigation(&SidebarSelectionNavigation {
                revision: 1,
                selected: input.selected.clone()
            })
            .unwrap()
    );
    assert!(sidebar.row_id(input.selected.as_ref().unwrap()).is_some());
    assert_eq!(sidebar.preferences(), &prefs);
    let root = sidebar.id();
    let header = sidebar.folder_header_id(&key).unwrap();
    let mut host = sidebar.into_host().unwrap();
    host.accessibility_action(header, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    let sidebar = host
        .get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap();
    assert_eq!(
        sidebar.take_preference_intents(&actions)[0].value,
        prefs.preferences
    );
    assert!(
        sidebar.visibility_target().is_none(),
        "manual disclosure invalidates a queued reveal before acknowledgement"
    );
    sidebar.update_preferences(&prefs, true).unwrap();
    assert!(
        !sidebar
            .reconcile_navigation(&SidebarSelectionNavigation {
                revision: 1,
                selected: input.selected
            })
            .unwrap()
    );
}
#[test]
fn visibility_is_explicit_once_and_fenced_by_inventory_disclosure_and_mount() {
    let mut input = input();
    let mut sidebar = Sidebar::new(783, Default::default());
    sidebar.update(&input).unwrap();
    assert!(sidebar.visibility_target().is_none());
    sidebar
        .reconcile_navigation(&SidebarSelectionNavigation {
            revision: 1,
            selected: input.selected.clone(),
        })
        .unwrap();
    let stale = sidebar.visibility_target().unwrap();
    input.revision += 1;
    sidebar.update(&input).unwrap();
    assert!(sidebar.claim_visibility(&stale).is_err());
    let target = sidebar.visibility_target().unwrap();
    assert_eq!(target.address(), input.selected.as_ref().unwrap());
    let mut other = Sidebar::new(783, Default::default());
    other.update(&input).unwrap();
    other
        .reconcile_navigation(&SidebarSelectionNavigation {
            revision: 1,
            selected: input.selected.clone(),
        })
        .unwrap();
    assert!(other.claim_visibility(&target).is_err());
    sidebar.claim_visibility(&target).unwrap();
    assert!(sidebar.claim_visibility(&target).is_err());
    assert!(sidebar.visibility_target().is_none());
    input.revision += 1;
    sidebar.update(&input).unwrap();
    assert!(sidebar.visibility_target().is_none());
    sidebar
        .reconcile_navigation(&SidebarSelectionNavigation {
            revision: 2,
            selected: input.selected.clone(),
        })
        .unwrap();
    assert!(sidebar.visibility_target().is_some());
}
