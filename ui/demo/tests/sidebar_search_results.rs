use std::sync::Arc;
use tekes_agent_components_ui::*;
use tekes_agent_contract::{
    SessionAddress, SessionSearchHit, ThreadActionsPresentation, ThreadActivity,
};
use tekes_agent_ui_bindings::{SidebarPublication, SidebarSearchMatch, SidebarSearchPublication};

fn address(id: &str) -> SessionAddress {
    SessionAddress {
        endpoint_id: "fixture".into(),
        session_id: id.into(),
    }
}
fn row(id: &str) -> SidebarRow {
    SidebarRow {
        folder_path: None,
        address: address(id),
        title: format!("unrelated title {id}"),
        activity: ThreadActivity {
            status: "idle".into(),
            is_stale: false,
        },
        pin: None,
        actions: ThreadActionsPresentation {
            endpoint: "fixture".into(),
            session_id: id.into(),
            label: "操作".into(),
            items: vec![],
        },
    }
}
fn inventory() -> SidebarPublication {
    SidebarPublication {
        revision: 1,
        loaded: true,
        error: None,
        rows: vec![row("a"), row("c"), row("b")],
    }
}
fn search() -> SidebarSearchPublication {
    SidebarSearchPublication {
        input: SidebarSearchInput {
            revision: 1,
            key: Some(SidebarSearchKey {
                owner: "search-reader".into(),
                endpoint_id: "fixture".into(),
                connection_epoch: 1,
                generation: 1,
                query: "正文命中".into(),
            }),
            phase: SidebarSearchPhase::Complete { has_more: false },
        },
        query: "正文命中".into(),
        hits: ["b", "a"]
            .map(|id| SidebarSearchMatch {
                address: address(id),
                hit: SessionSearchHit {
                    session_id: id.into(),
                    snippet: format!("{id} 的正文摘要"),
                    extra: Default::default(),
                },
            })
            .into(),
        unresolved: vec![],
        result: None,
    }
}
#[test]
fn joins_body_only_hits_without_title_filter_or_provider_reordering_or_selection_change() {
    let source = inventory();
    let result = search();
    let projection = source
        .project_search("fixture", &result, Some(address("c")), 9)
        .unwrap();
    assert_eq!(
        projection
            .input
            .rows
            .iter()
            .map(|row| row.address.session_id.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    assert_eq!(projection.input.selected, Some(address("c")));
    assert_eq!(projection.input.revision, 9);
    assert!(projection.input.loaded);
    assert_eq!(projection.snippets[&address("b")], "b 的正文摘要");
    assert_eq!(source.rows.len(), 3);
    assert_eq!(result.hits[0].address, address("b"));
}
#[test]
fn unresolved_inventory_is_not_confirmed_empty_and_recovers_without_another_query() {
    let mut source = inventory();
    let mut result = search();
    source.rows.clear();
    let first = source.project_search("fixture", &result, None, 1).unwrap();
    assert!(first.input.loaded);
    assert!(!first.input.search_help.as_ref().unwrap().results_ready);
    assert_eq!(first.unresolved, vec![address("b"), address("a")]);
    assert!(first.snippets.is_empty());
    result.unresolved = first.unresolved;
    source.rows = inventory().rows;
    let next = source.project_search("fixture", &result, None, 2).unwrap();
    assert!(next.input.loaded);
    assert!(next.unresolved.is_empty());
    assert_eq!(next.input.rows.len(), 2);
    result.input.phase = SidebarSearchPhase::Searching;
    let waiting = source.project_search("fixture", &result, None, 3).unwrap();
    assert!(waiting.input.loaded);
    assert!(!waiting.input.search_help.as_ref().unwrap().results_ready);
    assert!(waiting.input.rows.is_empty());
    assert!(waiting.snippets.is_empty());
}
#[test]
fn rejects_cross_endpoint_duplicate_inconsistent_and_retained_clear_payloads() {
    let source = inventory();
    let mut invalid_inventory = source.clone();
    invalid_inventory.revision = 0;
    assert!(
        invalid_inventory
            .project_search("fixture", &search(), None, 1)
            .is_err()
    );
    let mut result = search();
    result.hits[0].address.endpoint_id = "other".into();
    assert!(source.project_search("fixture", &result, None, 1).is_err());
    result = search();
    result.hits.push(result.hits[0].clone());
    assert!(source.project_search("fixture", &result, None, 1).is_err());
    result = search();
    result.hits[0].hit.session_id = "a".into();
    assert!(source.project_search("fixture", &result, None, 1).is_err());
    result = search();
    result.input.key = None;
    result.query.clear();
    result.input.phase = SidebarSearchPhase::Idle;
    assert!(source.project_search("fixture", &result, None, 1).is_err());
}
#[tokio::test]
async fn demo_projects_full_publications_and_clearing_restores_inventory_without_navigation_or_draft_changes()
 {
    use tekes_agent_ui_bindings::RunBinding;
    use tekes_agent_ui_demo::component_demo::ComponentDemo;
    let (runtime, _) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let binding = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let mut view = ComponentDemo::new([binding]).unwrap();
    view.set_sidebar_source(Arc::new(inventory())).unwrap();
    view.sync().unwrap();
    let old_unmatched_row = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .row_id(&address("c"))
        .unwrap();
    view.tree.accessibility_action(
        old_unmatched_row,
        None,
        tekes_ui_controls::composition::AccessibilityAction::Press,
    );
    let mut result = search();
    view.bind_sidebar_search_publication("search-reader", &result)
        .unwrap();
    assert!(view.drain_commands().unwrap().is_empty());
    let sidebar = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap();
    assert!(sidebar.row_id(&address("c")).is_none());
    assert_eq!(sidebar.displayed_input().unwrap().rows.len(), 2);
    assert_eq!(
        view.sidebar_search_snippet(&address("a")),
        Some("a 的正文摘要")
    );
    let before = sidebar.displayed_input().unwrap().clone();
    let mut conflict = result.clone();
    conflict.hits.clear();
    assert!(
        view.update_sidebar_search_publication("search-reader", &conflict)
            .is_err()
    );
    assert_eq!(
        view.tree
            .get(view.sidebar_id())
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .displayed_input(),
        Some(&before)
    );
    result.input.revision += 1;
    result.input.key.as_mut().unwrap().generation += 1;
    result.query = "另一个查询".into();
    result.input.key.as_mut().unwrap().query = result.query.clone();
    result.hits.clear();
    result.input.phase = SidebarSearchPhase::Failed {
        message: "失败".into(),
        has_more: false,
        retryable: true,
    };
    view.update_sidebar_search_publication("search-reader", &result)
        .unwrap();
    assert_eq!(view.sidebar_search_snippet(&address("a")), None);
    assert!(
        view.tree
            .semantics()
            .iter()
            .any(|node| node.semantics.name == "搜索结果尚未就绪")
    );
    result.input.revision += 1;
    result.input.phase = SidebarSearchPhase::Complete { has_more: false };
    view.update_sidebar_search_publication("search-reader", &result)
        .unwrap();
    assert!(
        view.tree
            .semantics()
            .iter()
            .any(|node| node.semantics.name == "没有匹配的会话")
    );
    result.input.revision += 1;
    result.input.key = None;
    result.query = "  ".into();
    result.input.phase = SidebarSearchPhase::Idle;
    view.update_sidebar_search_publication("search-reader", &result)
        .unwrap();
    let sidebar = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap();
    assert_eq!(sidebar.displayed_input().unwrap().rows, inventory().rows);
    assert_ne!(sidebar.row_id(&address("c")), Some(old_unmatched_row));
    assert_eq!(
        sidebar.displayed_input().unwrap().selected,
        Some(address("a"))
    );
    assert_eq!(view.selected(), 0);
    assert!(view.editor().input.text.is_empty());
    runtime.stop().await.unwrap();
}

#[test]
fn help_is_an_atomic_query_scoped_part_of_the_row_publication() {
    use tekes_ui_controls::composition::*;
    let source = inventory();
    let publication = search();
    let projected = source
        .project_search("fixture", &publication, None, 11)
        .unwrap();
    let mut sidebar = Sidebar::new(994, SidebarLabels::default());
    sidebar
        .replace_search_source("search-reader", &publication.input)
        .unwrap();
    sidebar.update(&projected.input).unwrap();
    assert_eq!(sidebar.row_help(&address("a")), Some("a 的正文摘要"));
    let original = sidebar.displayed_input().unwrap().clone();
    let mut conflicting = original.clone();
    conflicting
        .search_help
        .as_mut()
        .unwrap()
        .snippets
        .insert(address("a"), "same revision must not retarget help".into());
    assert!(sidebar.update(&conflicting).is_err());
    assert_eq!(sidebar.displayed_input(), Some(&original));
    let mut foreign = original.clone();
    foreign.revision += 1;
    foreign
        .search_help
        .as_mut()
        .unwrap()
        .snippets
        .insert(address("not-displayed"), "unknown owner".into());
    assert!(sidebar.update(&foreign).is_err());
    assert_eq!(sidebar.displayed_input(), Some(&original));
    let mut pending_with_help = original.clone();
    pending_with_help.revision += 1;
    pending_with_help
        .search_help
        .as_mut()
        .unwrap()
        .results_ready = false;
    assert!(sidebar.update(&pending_with_help).is_err());
    assert_eq!(sidebar.displayed_input(), Some(&original));
    let mut next = publication.input.clone();
    next.revision += 1;
    next.key.as_mut().unwrap().generation += 1;
    next.key.as_mut().unwrap().query = "新查询".into();
    sidebar.update_search_from("search-reader", &next).unwrap();
    assert_eq!(
        sidebar.row_help(&address("a")),
        None,
        "new footer revokes old help even before replacement rows arrive"
    );
    let root = sidebar.id();
    let mut host = sidebar.into_host().unwrap();
    let row = host
        .get(root)
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .row_id(&address("a"))
        .unwrap();
    host.accessibility_action(row, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    assert_eq!(
        host.get(root)
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .take_intents(&actions),
        vec![SidebarIntent::Select {
            address: address("a")
        }]
    );
}

fn pinned_row(id: &str) -> SidebarRow {
    let mut value = row(id);
    value.folder_path = Some(format!("/original/{id}"));
    value.pin = Some(ThreadPinPresentation { pinned: true });
    value
}
#[test]
fn search_preserves_global_pins_while_result_coherence_is_independent_of_inventory() {
    let mut source = inventory();
    source.rows[1] = pinned_row("c");
    let mut result = search();
    let projected = source.project_search("fixture", &result, None, 1).unwrap();
    assert_eq!(projected.input.rows, source.rows);
    assert!(!projected.snippets.contains_key(&address("c")));
    result.input.phase = SidebarSearchPhase::Searching;
    let waiting = source.project_search("fixture", &result, None, 2).unwrap();
    assert!(waiting.input.loaded);
    assert_eq!(waiting.input.rows, vec![pinned_row("c")]);
    assert!(waiting.snippets.is_empty());
    assert!(!waiting.input.search_help.unwrap().results_ready);
    result.input.phase = SidebarSearchPhase::Complete { has_more: false };
    result.hits[0].address = address("unknown");
    result.hits[0].hit.session_id = "unknown".into();
    let unresolved = source.project_search("fixture", &result, None, 3).unwrap();
    assert_eq!(unresolved.unresolved, vec![address("unknown")]);
    assert_eq!(unresolved.input.rows, vec![pinned_row("c")]);
    assert!(unresolved.input.loaded);
    assert!(!unresolved.input.search_help.unwrap().results_ready);
    source.loaded = false;
    assert!(
        !source
            .project_search("fixture", &result, None, 4)
            .unwrap()
            .input
            .loaded
    );
}
#[tokio::test]
async fn original_pin_survives_query_loading_failure_empty_and_unknown_hit_recovery_without_editor_changes()
 {
    use tekes_agent_ui_bindings::RunBinding;
    use tekes_agent_ui_demo::component_demo::ComponentDemo;
    use tekes_ui_controls::composition::*;
    let (runtime, _) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let binding = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let mut view = ComponentDemo::new([binding]).unwrap();
    let mut source = inventory();
    source.rows[1] = pinned_row("c");
    view.set_sidebar_source(Arc::new(source.clone())).unwrap();
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
    let pin_id = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .row_id(&address("c"))
        .unwrap();
    let pin_ptr = view
        .tree
        .get(pin_id)
        .unwrap()
        .component::<ButtonContent>()
        .unwrap() as *const _ as usize;
    let mut result = search();
    view.bind_sidebar_search_publication("search-reader", &result)
        .unwrap();
    for phase in [
        SidebarSearchPhase::Searching,
        SidebarSearchPhase::Failed {
            message: "query unavailable".into(),
            has_more: false,
            retryable: true,
        },
        SidebarSearchPhase::Complete { has_more: false },
    ] {
        result.input.revision += 1;
        result.input.phase = phase;
        result.hits.clear();
        view.update_sidebar_search_publication("search-reader", &result)
            .unwrap();
        assert_eq!(
            view.tree
                .get(view.sidebar_id())
                .unwrap()
                .component::<Sidebar>()
                .unwrap()
                .row_id(&address("c")),
            Some(pin_id)
        );
        assert_eq!(
            view.tree
                .get(pin_id)
                .unwrap()
                .component::<ButtonContent>()
                .unwrap() as *const _ as usize,
            pin_ptr
        );
        assert_eq!(
            view.tree
                .get(view.sidebar_id())
                .unwrap()
                .component::<Sidebar>()
                .unwrap()
                .row_help(&address("c")),
            Some("/original/c")
        );
        assert!(view.drain_commands().unwrap().is_empty());
    }
    result.input.revision += 1;
    result.hits.push(SidebarSearchMatch {
        address: address("late"),
        hit: SessionSearchHit {
            session_id: "late".into(),
            snippet: "late current hit".into(),
            extra: Default::default(),
        },
    });
    view.update_sidebar_search_publication("search-reader", &result)
        .unwrap();
    assert!(
        view.tree
            .semantics()
            .iter()
            .any(|node| node.semantics.name == "搜索结果尚未就绪")
    );
    assert!(
        !view
            .tree
            .semantics()
            .iter()
            .any(|node| node.semantics.name == "没有匹配的会话")
    );
    source.revision += 1;
    source.rows.push(row("late"));
    view.set_sidebar_source(Arc::new(source)).unwrap();
    view.sync().unwrap();
    assert!(
        view.tree
            .get(view.sidebar_id())
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .row_id(&address("late"))
            .is_some()
    );
    assert_eq!(
        view.tree
            .get(pin_id)
            .unwrap()
            .component::<ButtonContent>()
            .unwrap() as *const _ as usize,
        pin_ptr
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
