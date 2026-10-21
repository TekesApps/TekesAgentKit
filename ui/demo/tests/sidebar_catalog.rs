use std::collections::BTreeMap;
use std::sync::Arc;
use tekes_agent_components_ui::*;
use tekes_agent_ui_bindings::SidebarPublication;
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

fn source() -> SidebarPublication {
    let mut rows: Vec<_> = (0..13).map(|i| row(i, i == 12)).collect();
    for (i, row) in rows.iter_mut().enumerate() {
        row.folder_path = Some(format!("/folder-{i}"));
    }
    let activity_at = rows
        .iter()
        .enumerate()
        .map(|(i, r)| (r.address.clone(), i as f64))
        .collect();
    SidebarPublication {
        revision: 1,
        rows,
        activity_at,
        loaded: true,
        error: None,
    }
}
#[test]
fn full_catalog_includes_off_page_and_pin_only_folders_and_is_not_title_order() {
    let source = source();
    let before = source.clone();
    let catalog = tekes_agent_ui_consumer_check::sidebar_folder_choices(&source).unwrap();
    assert_eq!(catalog.folders.len(), 13);
    assert_eq!(catalog.folders[0].path.as_deref(), Some("/folder-12"));
    assert_eq!(catalog.folders[12].path.as_deref(), Some("/folder-0"));
    let mut sidebar = Sidebar::new(795, Default::default());
    sidebar.set_title_comparator(|a, b| a.cmp(b));
    sidebar
        .update_preferences(
            &SidebarPreferencesSnapshot {
                revision: 1,
                preferences: SidebarPreferences {
                    sort: SidebarSort::TitleAscending,
                    ..Default::default()
                },
            },
            true,
        )
        .unwrap();
    sidebar.update(&source.input(None)).unwrap();
    assert!(sidebar.row_id(&source.rows[9].address).is_none());
    assert!(
        catalog
            .folders
            .iter()
            .any(|f| f.key == source.rows[9].folder_key())
    );
    let mut input = source.input(None);
    input.revision += 1;
    input.folder_filter = Some(source.rows[0].folder_key());
    sidebar.update(&input).unwrap();
    assert_eq!(sidebar.paging().ordinary_count, 1);
    assert!(sidebar.row_id(&source.rows[12].address).is_some());
    assert_eq!(source.folder_catalog().unwrap(), catalog);
    assert_eq!(source, before);
}
#[test]
fn catalog_uses_max_path_and_activity_with_deterministic_folder_ties() {
    let mut rows: Vec<_> = (0..6).map(|i| row(i, false)).collect();
    rows[0].primary_folder_id = Some("same".into());
    rows[0].folder_path = Some("/z".into());
    rows[1].primary_folder_id = Some("same".into());
    rows[1].folder_path = Some("/a".into());
    rows[2].primary_folder_id = Some("path:/work".into());
    rows[2].folder_path = None;
    rows[3].folder_path = Some("/work".into());
    rows[4].folder_path = None;
    rows[5].address.endpoint_id = "other".into();
    let clocks: BTreeMap<_, _> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| (r.address.clone(), if i == 1 { 2.0 } else { 1.0 }))
        .collect();
    let catalog = sidebar_folder_catalog(&rows, &clocks).unwrap();
    assert_eq!(catalog.len(), 5);
    assert_eq!(catalog[0].key, rows[1].folder_key());
    assert_eq!(catalog[0].path.as_deref(), Some("/z"));
    assert_eq!(catalog[1].key, rows[4].folder_key()); // "none" before "path:..."
    assert_ne!(rows[2].folder_key(), rows[3].folder_key());
    rows.reverse();
    assert_eq!(sidebar_folder_catalog(&rows, &clocks).unwrap(), catalog);
}
#[test]
fn catalog_rejects_unknown_clocks_duplicate_rows_and_invalid_sources() {
    let mut source = source();
    source.activity_at.remove(&source.rows[0].address);
    assert!(source.folder_catalog().is_err());
    source
        .activity_at
        .insert(source.rows[0].address.clone(), f64::NAN);
    assert!(source.folder_catalog().is_err());
    source
        .activity_at
        .insert(source.rows[0].address.clone(), 0.0);
    source.rows.push(source.rows[0].clone());
    assert!(source.folder_catalog().is_err());
    source.rows.pop();
    source.rows[0].primary_folder_id = Some(String::new());
    assert!(source.folder_catalog().is_err());
    source.revision = 0;
    assert!(source.folder_catalog().is_err());
}
#[test]
fn restoration_withholds_choices_and_failure_preserves_last_readable_catalog() {
    let mut source = source();
    let expected = source.folder_catalog().unwrap().folders;
    source.loaded = false;
    let restoring = source.folder_catalog().unwrap();
    assert!(!restoring.loaded && restoring.folders.is_empty());
    source.loaded = true;
    source.error = Some("inventory observation ended".into());
    let failure = source.folder_catalog().unwrap();
    assert_eq!(failure.error, source.error);
    assert_eq!(failure.folders, expected);
}
#[tokio::test]
async fn demo_catalog_survives_display_filter_and_retains_source_revision_and_clock_only_updates() {
    use tekes_agent_ui_bindings::RunBinding;
    use tekes_agent_ui_demo::component_demo::ComponentDemo;
    let (runtime, _) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let binding = Arc::new(
        RunBinding::open_main(runtime, RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let mut view = ComponentDemo::new([binding]).unwrap();
    assert!(view.sidebar_folder_catalog().unwrap().is_none());
    let mut source = source();
    view.set_sidebar_source(Arc::new(source.clone())).unwrap();
    view.sync().unwrap();
    let before = view.sidebar_folder_catalog().unwrap().unwrap();
    view.set_sidebar_folder_filter(Some(source.rows[0].folder_key()))
        .unwrap();
    assert_eq!(view.sidebar_folder_catalog().unwrap().unwrap(), before);
    source.revision += 1;
    source
        .activity_at
        .insert(source.rows[0].address.clone(), 100.0);
    view.set_sidebar_source(Arc::new(source.clone())).unwrap();
    view.sync().unwrap();
    let after = view.sidebar_folder_catalog().unwrap().unwrap();
    assert_eq!(after.revision, source.revision);
    assert_eq!(after.folders[0].key, source.rows[0].folder_key());
    assert!(view.binding().current_run().unwrap().submissions.is_empty());
}
