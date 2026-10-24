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
fn page_is_five_filtered_ordinary_rows_plus_matching_pins_and_selected_outside_page() {
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
fn search_is_bounded_without_selection_supplement_and_folder_filter_applies_to_pins() {
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
    assert_eq!(out.sessions.len(), 5);
    assert!(facts.has_more);
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
    assert!(out.pinned.is_empty());
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

fn grow(sidebar: &mut Sidebar) -> SidebarPageRequest {
    let control = sidebar.page_button_id().unwrap();
    let request = sidebar
        .take_page_requests(&[ContentAction {
            target: control,
            kind: ContentActionKind::Activate,
            source: ContentActionSource::Programmatic,
        }])
        .pop()
        .unwrap();
    assert!(sidebar.load_more(&request).unwrap());
    request
}

#[test]
fn filter_and_sort_restart_five_but_refresh_grouping_and_pins_keep_expanded_page() {
    let mut input = input();
    let mut sidebar = Sidebar::new(784, Default::default());
    sidebar.set_title_comparator(|a, b| a.cmp(b));
    sidebar.update(&input).unwrap();
    let stale = grow(&mut sidebar);
    input.revision += 1;
    sidebar.update(&input).unwrap();
    assert_eq!(sidebar.paging().limit, 10);
    let mut prefs = SidebarPreferencesSnapshot {
        revision: 1,
        preferences: SidebarPreferences {
            group_by_folder: true,
            ..Default::default()
        },
    };
    sidebar.update_preferences(&prefs, true).unwrap();
    assert_eq!(sidebar.paging().limit, 10);
    prefs.revision += 1;
    prefs.preferences.pinned_expanded = false;
    sidebar.update_preferences(&prefs, true).unwrap();
    assert_eq!(sidebar.paging().limit, 10);
    input.revision += 1;
    input.folder_filter = Some(row(1, false).folder_key());
    sidebar.update(&input).unwrap();
    assert_eq!(sidebar.paging().limit, 5);
    assert!(!sidebar.load_more(&stale).unwrap());
    grow(&mut sidebar);
    prefs.revision += 1;
    prefs.preferences.sort = SidebarSort::TitleDescending;
    sidebar.update_preferences(&prefs, true).unwrap();
    assert_eq!(sidebar.paging().limit, 5);
    grow(&mut sidebar);
    sidebar.update_preferences(&prefs, true).unwrap();
    assert_eq!(sidebar.paging().limit, 10);
    assert!(tekes_agent_ui_consumer_check::restart_sidebar_page(&mut sidebar).unwrap());
    assert!(!sidebar.reset_paging().unwrap());
    assert_eq!(sidebar.paging().limit, 5);
}

#[test]
fn conflicting_filter_publication_and_unsupported_sort_preserve_page_and_controls() {
    let mut input = input();
    let mut sidebar = Sidebar::new(785, Default::default());
    sidebar.update(&input).unwrap();
    grow(&mut sidebar);
    let control = sidebar.page_button_id();
    input.folder_filter = Some(row(1, false).folder_key());
    assert!(sidebar.update(&input).is_err());
    let prefs = SidebarPreferencesSnapshot {
        revision: 1,
        preferences: SidebarPreferences {
            sort: SidebarSort::TitleAscending,
            ..Default::default()
        },
    };
    assert!(sidebar.update_preferences(&prefs, true).is_err());
    assert_eq!(sidebar.paging().limit, 10);
    assert_eq!(sidebar.page_button_id(), control);
    assert_eq!(sidebar.displayed_input().unwrap().folder_filter, None);
}

#[tokio::test]
async fn demo_repeated_filter_resets_page_without_touching_chinese_preedit_or_focus() {
    use std::sync::Arc;
    use tekes_agent_ui_bindings::{RunBinding, SidebarPublication};
    use tekes_agent_ui_demo::component_demo::ComponentDemo;
    let (runtime, _) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let binding = Arc::new(
        RunBinding::open_main(runtime, RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let mut view = ComponentDemo::new([binding]).unwrap();
    let input = input();
    let filter = Some(row(1, false).folder_key());
    view.set_sidebar_source(Arc::new(SidebarPublication {
        observation: Default::default(),
        activity_at: Default::default(),
        name_corpus: None,
        revision: 1,
        loaded: true,
        error: None,
        rows: input.rows,
    }))
    .unwrap();
    view.sync().unwrap();
    view.tree.focus(Some(view.ids().editor), false);
    view.event(&ContentEvent::Preedit {
        text: "中文组字".into(),
        selection: 1..3,
        clauses: vec![],
        replacement: None,
    })
    .unwrap();
    let before = format!(
        "{:?}",
        (
            &view.editor().input.text,
            view.editor().input.selection(),
            &view.editor().input.marked,
            view.editor().input.external_baseline()
        )
    );
    let focus = view.tree.focused();
    for _ in 0..2 {
        let id = view.sidebar_id();
        let sidebar = view
            .tree
            .get_mut(id)
            .unwrap()
            .component_mut::<Sidebar>()
            .unwrap();
        grow(sidebar);
        assert_eq!(sidebar.paging().limit, 10);
        view.set_sidebar_folder_filter(filter.clone()).unwrap();
        assert_eq!(
            view.tree
                .get(id)
                .unwrap()
                .component::<Sidebar>()
                .unwrap()
                .paging()
                .limit,
            5
        );
        assert_eq!(view.tree.focused(), focus);
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
            before
        );
        assert!(view.binding().current_run().unwrap().submissions.is_empty());
    }
}
