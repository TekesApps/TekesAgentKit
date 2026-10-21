use tekes_agent_components_ui::*;
use tekes_agent_contract::{
    SessionAddress, ThreadActionsPresentation, ThreadActivity, ThreadPinPresentation,
};
use tekes_ui_controls::composition::*;
fn row(id: &str, title: &str, pinned: bool) -> SidebarRow {
    SidebarRow {
        primary_folder_id: None,
        folder_path: None,
        address: SessionAddress {
            endpoint_id: "p".into(),
            session_id: id.into(),
        },
        title: title.into(),
        activity: ThreadActivity {
            status: "idle".into(),
            is_stale: false,
        },
        pin: Some(ThreadPinPresentation { pinned }),
        actions: ThreadActionsPresentation {
            endpoint: "p".into(),
            session_id: id.into(),
            label: "actions".into(),
            items: vec![],
        },
    }
}
fn compare(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}
#[test]
fn ordering_keeps_pins_separate_and_title_ties_ascending_in_both_directions() {
    let rows = vec![
        row("z", "Beta", false),
        row("b", "alpha", false),
        row("a", "ALPHA", false),
        row("p2", "A", true),
        row("p1", "Z", true),
    ];
    let mut value = SidebarPreferences {
        sort: SidebarSort::TitleDescending,
        pin_order: vec![rows[3].address.clone()],
        ..Default::default()
    };
    assert!(ordered_sidebar_rows(&rows, &value, None).is_err());
    let ids = |value: &SidebarPreferences| {
        ordered_sidebar_rows(&rows, value, Some(compare))
            .unwrap()
            .into_iter()
            .map(|r| r.address.session_id.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&value), ["p2", "p1", "z", "a", "b"]);
    value.sort = SidebarSort::TitleAscending;
    assert_eq!(ids(&value), ["p2", "p1", "a", "b", "z"]);
    value.sort = SidebarSort::RecentActivity;
    assert_eq!(ids(&value), ["p2", "p1", "z", "b", "a"]);
}
#[test]
fn pinned_section_retains_original_row_controls_and_retires_old_preference_callbacks() {
    let input = SidebarInput {
        folder_filter: None,
        search_help: None,
        revision: 1,
        rows: vec![
            row("a", "A", true),
            row("b", "B", true),
            row("c", "C", false),
        ],
        selected: None,
        loaded: true,
        error: None,
    };
    let mut view = Sidebar::new(998, SidebarLabels::default());
    let root = view.id();
    let initial = SidebarPreferencesSnapshot::default();
    view.update_preferences(&initial, true).unwrap();
    view.update(&input).unwrap();
    let row = view.row_id(&input.rows[0].address).unwrap();
    let header = view.pinned_header_id().unwrap();
    let mut host = view.into_host().unwrap();
    let widget = host.get(row).unwrap().component::<ButtonContent>().unwrap() as *const _ as usize;
    host.accessibility_action(row, None, AccessibilityAction::Press);
    let old_row_actions = host.drain_actions();
    host.accessibility_action(header, None, AccessibilityAction::Press);
    let captured = host.drain_actions();
    let changes = host
        .get(root)
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .take_preference_intents(&captured);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].expected, initial);
    assert!(!changes[0].value.pinned_expanded);
    let collapsed = SidebarPreferencesSnapshot {
        revision: 2,
        preferences: changes[0].value.clone(),
    };
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update_preferences(&collapsed, true)
        .unwrap();
    let sidebar = host.get(root).unwrap().component::<Sidebar>().unwrap();
    assert!(sidebar.take_preference_intents(&captured).is_empty());
    assert!(sidebar.take_intents(&old_row_actions).is_empty());
    assert!(
        sidebar
            .move_pin_change(&input.rows[0].address, -1)
            .is_none()
    );
    let moved = sidebar.move_pin_change(&input.rows[0].address, 1).unwrap();
    assert_eq!(
        moved.value.pin_order,
        [input.rows[1].address.clone(), input.rows[0].address.clone()]
    );
    let expanded = SidebarPreferencesSnapshot {
        revision: 3,
        preferences: initial.preferences.clone(),
    };
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update_preferences(&expanded, true)
        .unwrap();
    assert_eq!(
        host.get(row).unwrap().component::<ButtonContent>().unwrap() as *const _ as usize,
        widget
    );
    let sidebar = host.get(root).unwrap().component::<Sidebar>().unwrap();
    assert!(sidebar.take_preference_intents(&captured).is_empty());
    assert_eq!(sidebar.displayed_input(), Some(&input));
    let fresh_header = sidebar.pinned_header_id().unwrap();
    host.accessibility_action(fresh_header, None, AccessibilityAction::Press);
    let before_revocation = host.drain_actions();
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update_preferences(&expanded, false)
        .unwrap();
    assert_ne!(
        host.get(root)
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .pinned_header_id(),
        Some(fresh_header)
    );
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update_preferences(&expanded, true)
        .unwrap();
    assert!(
        host.get(root)
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .take_preference_intents(&before_revocation)
            .is_empty()
    );
    assert!(
        host.get_mut(root)
            .unwrap()
            .component_mut::<Sidebar>()
            .unwrap()
            .update_preferences(&initial, true)
            .is_err()
    );
}
