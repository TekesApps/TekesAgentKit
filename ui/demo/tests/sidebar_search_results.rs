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
    assert!(!first.input.loaded);
    assert_eq!(first.unresolved, vec![address("b"), address("a")]);
    assert!(first.snippets.is_empty());
    result.unresolved = first.unresolved;
    source.rows = inventory().rows;
    let next = source.project_search("fixture", &result, None, 2).unwrap();
    assert!(next.input.loaded);
    assert!(next.unresolved.is_empty());
    assert_eq!(next.input.rows.len(), 2);
    result.input.phase = SidebarSearchPhase::Searching;
    assert!(
        !source
            .project_search("fixture", &result, None, 3)
            .unwrap()
            .input
            .loaded
    );
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
