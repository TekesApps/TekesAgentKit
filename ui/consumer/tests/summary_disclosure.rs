//! Real AgentKit Sidebar public consumer; execution receipts live outside source.
use tekes_agent_components_ui::*;
use tekes_ui_controls::composition::*;
#[path = "support/disclosure_host.rs"]
mod disclosure_host;

fn pinned_input() -> SidebarInput {
    SidebarInput {
        folder_filter: None,
        search_help: None,
        revision: 1,
        selected: None,
        loaded: true,
        error: None,
        rows: vec![SidebarRow {
            primary_folder_id: None,
            folder_path: None,
            address: RunScope::main("consumer", "pinned").address(),
            title: "原会话".into(),
            activity: ThreadActivity {
                status: "idle".into(),
                is_stale: false,
            },
            pin: Some(ThreadPinPresentation { pinned: true }),
            actions: ThreadActionsPresentation {
                endpoint: "consumer".into(),
                session_id: "pinned".into(),
                label: "操作".into(),
                items: vec![],
            },
        }],
    }
}

#[test]
fn sidebar_uses_owned_disclosure_records_and_retains_trigger_and_leaf_mounts() {
    let input = pinned_input();
    let initial = SidebarPreferencesSnapshot::default();
    let mut sidebar = Sidebar::new(6701, Default::default());
    sidebar.update_preferences(&initial, true).unwrap();
    sidebar.update(&input).unwrap();
    let root = sidebar.id();
    let header = sidebar.pinned_header_id().unwrap();
    let leaf = sidebar.row_id(&input.rows[0].address).unwrap();
    let mut host = sidebar.into_host().unwrap();
    disclosure_host::render(&mut host);
    let trigger_mount = host.get(header).unwrap().mount_identity();
    let leaf_mount = host.get(leaf).unwrap().mount_identity();
    let response = host.accessibility_action(header, None, AccessibilityAction::Press);
    let records = host.drain_action_records();
    assert!(
        response.activated,
        "first Sidebar trigger Press: response={response:?}, records={records:?}"
    );
    assert!(response.consumed, "SDK owned activation consumes the Press");
    assert_eq!(records.len(), 1);
    assert!(matches!(
        &records[0].payload,
        Some(ContentActionPayload::DisclosureExpansion(_))
    ));
    assert!(
        host.drain_actions().is_empty(),
        "rich and legacy queues are the same once-only queue"
    );
    assert!(
        host.get(root)
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .preferences_current(&initial, true),
        "request does not eagerly toggle the business projection"
    );
    let change = host
        .get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .take_preference_intents(&records);
    assert_eq!(change.len(), 1);
    assert_eq!(change[0].expected, initial);
    assert!(!change[0].value.pinned_expanded);
    assert!(
        host.get_mut(root)
            .unwrap()
            .component_mut::<Sidebar>()
            .unwrap()
            .take_preference_intents(&records)
            .is_empty()
    );
    assert!(!host.semantics().iter().any(|node| node.id == leaf));
    // A rejected persistence write republishes the authoritative same snapshot.
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update_preferences(&initial, true)
        .unwrap();
    assert!(
        host.get_mut(root)
            .unwrap()
            .component_mut::<Sidebar>()
            .unwrap()
            .take_preference_intents(&records)
            .is_empty()
    );
    // Even a rejected callback uses the Host's mutable-access boundary, which
    // withdraws its input layout lease. Arrange after that access, before Press.
    disclosure_host::render(&mut host);
    assert_eq!(host.get(header).unwrap().mount_identity(), trigger_mount);
    assert_eq!(host.get(leaf).unwrap().mount_identity(), leaf_mount);
    let response = host.accessibility_action(header, None, AccessibilityAction::Press);
    let fresh = host.drain_action_records();
    assert!(
        response.activated,
        "republished Sidebar trigger Press: response={response:?}, records={fresh:?}"
    );
    assert!(response.consumed, "SDK owned activation consumes the Press");
    let mut wrong_mount = fresh[0].clone();
    wrong_mount.mount += 1;
    assert!(
        host.get_mut(root)
            .unwrap()
            .component_mut::<Sidebar>()
            .unwrap()
            .take_preference_intents(&[wrong_mount])
            .is_empty()
    );
    assert_eq!(
        host.get_mut(root)
            .unwrap()
            .component_mut::<Sidebar>()
            .unwrap()
            .take_preference_intents(&fresh)
            .len(),
        1
    );
    let accepted = SidebarPreferencesSnapshot {
        revision: initial.revision + 1,
        preferences: change[0].value.clone(),
    };
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update_preferences(&accepted, false)
        .unwrap();
    assert_eq!(host.get(header).unwrap().mount_identity(), trigger_mount);
    assert!(
        host.get_mut(root)
            .unwrap()
            .component_mut::<Sidebar>()
            .unwrap()
            .take_preference_intents(&fresh)
            .is_empty()
    );
    assert!(
        host.get_mut(root)
            .unwrap()
            .component_mut::<Sidebar>()
            .unwrap()
            .update_preferences(&initial, true)
            .is_err(),
        "old CAS publication cannot roll back a newer revision"
    );
    assert_eq!(host.get(leaf).unwrap().mount_identity(), leaf_mount);
}

#[test]
fn unready_sdk_layout_defers_before_claim_and_explicit_ready_reveal_is_once_only() {
    let mut input = pinned_input();
    input.selected = Some(input.rows[0].address.clone());
    let mut sidebar = Sidebar::new(6702, Default::default());
    sidebar
        .update_preferences(&SidebarPreferencesSnapshot::default(), true)
        .unwrap();
    sidebar.update(&input).unwrap();
    sidebar
        .reconcile_navigation(&SidebarSelectionNavigation {
            revision: 1,
            selected: input.selected.clone(),
        })
        .unwrap();
    let root = sidebar.id();
    // Logical captures remain source facts; runtime readiness is separate.
    let target = sidebar.visibility_target().unwrap();
    assert!(!sidebar.visibility_reveal_ready());
    let mut host = sidebar.into_host().unwrap();
    disclosure_host::render(&mut host);
    assert!(
        host.get(root)
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .visibility_reveal_ready()
    );
    let (version, ()) = host
        .set_scroll_reading_document(
            target.viewport(),
            "ac5-ready-navigation",
            1,
            vec![target.item()],
            |_| {},
        )
        .unwrap();
    disclosure_host::render(&mut host);
    // An authoritative same-input update retires the actual arrange capability.
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update(&input)
        .unwrap();
    assert!(
        !host
            .get(root)
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .visibility_reveal_ready()
    );
    assert!(matches!(
        reveal_sidebar_selection(
            &mut host,
            &target,
            &version,
            Default::default(),
            &mut disclosure_host::Text
        ),
        Err(SidebarRevealError::Selection(_))
    ));
    // Normal rendering makes the same unconsumed capture ready. This explicit
    // call is not an automatic replay of a failed SDK plan/commit command.
    disclosure_host::render(&mut host);
    assert!(
        host.get(root)
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .visibility_reveal_ready()
    );
    reveal_sidebar_selection(
        &mut host,
        &target,
        &version,
        Default::default(),
        &mut disclosure_host::Text,
    )
    .unwrap();
    assert!(matches!(
        reveal_sidebar_selection(
            &mut host,
            &target,
            &version,
            Default::default(),
            &mut disclosure_host::Text
        ),
        Err(SidebarRevealError::Selection(_))
    ));
    assert!(
        host.get(root)
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .visibility_target()
            .is_none()
    );
}

#[test]
fn entirely_offscreen_folder_reveals_original_leaf_without_losing_business_proof() {
    let template = pinned_input().rows.into_iter().next().unwrap();
    let rows: Vec<_> = (0..20)
        .map(|index| {
            let mut row = template.clone();
            let id = format!("folder-{index:02}");
            row.address = RunScope::main("consumer", id.clone()).address();
            row.title = id.clone();
            row.primary_folder_id = Some(id.clone());
            row.folder_path = Some(format!("/original/{id}"));
            row.pin = None;
            row.actions.session_id = id;
            row
        })
        .collect();
    let selected = rows.last().unwrap().address.clone();
    let folder = rows.last().unwrap().folder_key();
    let input = SidebarInput {
        folder_filter: None,
        search_help: None,
        revision: 1,
        selected: Some(selected),
        loaded: true,
        error: None,
        rows,
    };
    let mut sidebar = Sidebar::new(6703, Default::default());
    let preferences = SidebarPreferencesSnapshot {
        revision: 1,
        preferences: SidebarPreferences {
            group_by_folder: true,
            ..Default::default()
        },
    };
    sidebar.update_preferences(&preferences, true).unwrap();
    sidebar.update(&input).unwrap();
    sidebar
        .reconcile_navigation(&SidebarSelectionNavigation {
            revision: 1,
            selected: input.selected.clone(),
        })
        .unwrap();
    let root = sidebar.id();
    let header = sidebar.folder_header_id(&folder).unwrap();
    let target = sidebar.visibility_target().unwrap();
    let mut host = sidebar.into_host().unwrap();
    disclosure_host::render(&mut host);
    let (version, ()) = host
        .set_scroll_reading_document(
            target.viewport(),
            "ac5-whole-section-offscreen",
            1,
            vec![target.item()],
            |_| {},
        )
        .unwrap();
    disclosure_host::render(&mut host);
    assert!(
        host.get(header).unwrap().frame().min_y() > 300.,
        "the whole original target section starts below the viewport"
    );
    assert!(host.get(target.item()).unwrap().visible().is_none());
    let leaf_mount = host.get(target.item()).unwrap().mount_identity();
    let header_mount = host.get(header).unwrap().mount_identity();
    assert!(
        host.get(root)
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .visibility_reveal_ready()
    );
    reveal_sidebar_selection(
        &mut host,
        &target,
        &version,
        Default::default(),
        &mut disclosure_host::Text,
    )
    .unwrap();
    disclosure_host::render(&mut host);
    assert_eq!(
        host.get(target.item()).unwrap().visible(),
        Some(host.get(target.item()).unwrap().frame())
    );
    assert!(
        host.get(target.viewport())
            .unwrap()
            .component::<ScrollContent>()
            .unwrap()
            .scroll
            .offset
            .y
            > 0.
    );
    assert_eq!(
        host.get(target.item()).unwrap().mount_identity(),
        leaf_mount
    );
    assert_eq!(host.get(header).unwrap().mount_identity(), header_mount);
    assert!(
        host.get(root)
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .visibility_target()
            .is_none()
    );
}
