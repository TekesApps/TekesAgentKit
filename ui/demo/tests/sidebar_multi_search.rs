use serde_json::json;
use std::{sync::Arc, time::Duration};
use tekes_agent_components_ui::{SidebarInput, SidebarSearchPhase};
use tekes_agent_contract::SessionAddress;
use tekes_agent_presentation::SidebarMultiSearchPublication;
use tekes_agent_ui_bindings::{
    SidebarMultiEndpointBinding, SidebarMultiPublication, SidebarMultiSearchBinding,
};
use tekes_agent_ui_demo::fixture::Fixture;
use tekes_agent_web::SharedAgent;

fn address(endpoint: &str, session: &str) -> SessionAddress {
    SessionAddress {
        endpoint_id: endpoint.into(),
        session_id: session.into(),
    }
}

async fn setup(
    search_a: bool,
    search_b: bool,
) -> (
    Arc<SharedAgent>,
    SidebarMultiEndpointBinding,
    SidebarMultiSearchBinding,
    Fixture,
    Fixture,
) {
    let mut agent = SharedAgent::default();
    let fixture_a = Fixture::default().with_endpoint_id("endpoint-a");
    let fixture_b = Fixture::default().with_endpoint_id("endpoint-b");
    let fixture_a = if search_a {
        fixture_a.with_session_search()
    } else {
        fixture_a
    };
    let fixture_b = if search_b {
        fixture_b.with_session_search()
    } else {
        fixture_b
    };
    agent
        .register(
            "endpoint-a".into(),
            "native".into(),
            "A".into(),
            Arc::new(fixture_a.clone().with_lifecycle()),
        )
        .unwrap();
    agent
        .register(
            "endpoint-b".into(),
            "native".into(),
            "B".into(),
            Arc::new(fixture_b.clone().with_lifecycle()),
        )
        .unwrap();
    let agent = Arc::new(agent);
    for endpoint in ["endpoint-a", "endpoint-b"] {
        agent
            .endpoint(endpoint)
            .unwrap()
            .runtime
            .wait_ready(Duration::from_secs(3))
            .await
            .unwrap();
    }
    let inventory =
        SidebarMultiEndpointBinding::new(agent.clone(), ["endpoint-a", "endpoint-b"]).unwrap();
    let mut inventory_updates = inventory.subscribe();
    tokio::time::timeout(Duration::from_secs(4), async {
        while !inventory.snapshot().loaded {
            inventory_updates.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    let runtimes = ["endpoint-a", "endpoint-b"]
        .into_iter()
        .map(|endpoint| agent.endpoint(endpoint).unwrap().runtime.clone())
        .collect();
    let search = SidebarMultiSearchBinding::new(runtimes).unwrap();
    (agent, inventory, search, fixture_a, fixture_b)
}

async fn wait_search(
    source: &SidebarMultiSearchBinding,
    predicate: impl Fn(&SidebarMultiSearchPublication) -> bool,
) {
    let mut changes = source.subscribe();
    tokio::time::timeout(Duration::from_secs(4), async {
        while !predicate(&source.snapshot()) {
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}

fn complete(publication: &SidebarMultiSearchPublication) -> bool {
    publication.children.iter().all(|child| {
        matches!(
            &child.publication.input.phase,
            SidebarSearchPhase::Complete { .. } | SidebarSearchPhase::Failed { .. }
        )
    })
}

fn display(publication: &SidebarMultiPublication) -> SidebarInput {
    let selected = Some(address("endpoint-b", "a"));
    let mut input = publication.input(selected);
    input.folder_filter = Some(tekes_agent_presentation::SidebarFolderKey {
        endpoint_id: "endpoint-a".into(),
        identity: tekes_agent_presentation::SidebarFolderIdentity::Path("/fixture".into()),
    });
    input
}

#[tokio::test]
async fn projects_duplicate_session_ids_in_global_inventory_order_and_preserves_display_state() {
    let (_agent, inventory, search, fixture_a, fixture_b) = setup(true, true).await;
    fixture_a.set_search_result(
        "query",
        json!({"items":[{"sessionId":"b","snippet":"A b"},{"sessionId":"a","snippet":"A a"}],"hasMore":false}),
    );
    fixture_b.set_search_result(
        "query",
        json!({"items":[{"sessionId":"a","snippet":"B a"},{"sessionId":"b","snippet":"B b"}],"hasMore":false}),
    );
    search.set_query("query").unwrap();
    wait_search(&search, complete).await;

    let inventory_snapshot = inventory.snapshot();
    let display = display(&inventory_snapshot);
    let projected = inventory_snapshot
        .project_multi_search_for_display(&search.snapshot(), &display)
        .unwrap();
    let expected: Vec<_> = inventory_snapshot
        .rows
        .iter()
        .filter(|row| row.address.session_id == "a" || row.address.session_id == "b")
        .map(|row| row.address.clone())
        .collect();
    assert_eq!(
        projected
            .input
            .rows
            .iter()
            .map(|row| row.address.clone())
            .collect::<Vec<_>>(),
        expected,
        "global inventory order must win over each provider's hit order"
    );
    assert_eq!(projected.input.selected, display.selected);
    assert_eq!(projected.input.folder_filter, display.folder_filter);
    assert!(projected.input.search_help.is_none());
    assert_eq!(projected.snippets[&address("endpoint-a", "a")], "A a");
    assert_eq!(projected.snippets[&address("endpoint-b", "a")], "B a");
    assert_eq!(projected.children.len(), 2);
    assert!(projected.children.values().all(|child| {
        child.aggregate_key == search.snapshot().input.key
            && child.publication.input.key.as_ref().is_some_and(|key| {
                child
                    .publication
                    .hits
                    .iter()
                    .all(|hit| key.endpoint_id == hit.address.endpoint_id)
            })
    }));
    assert_eq!(
        projected
            .input
            .rows
            .iter()
            .filter(|row| row.address.session_id == "a")
            .count(),
        2,
        "the same session ID on two endpoints remains two full addresses"
    );
}

#[tokio::test]
async fn one_failed_endpoint_does_not_hide_another_endpoints_success() {
    let (_agent, inventory, search, fixture_a, fixture_b) = setup(true, true).await;
    fixture_b.set_search_failure(true);
    fixture_a.set_search_result(
        "query",
        json!({"items":[{"sessionId":"a","snippet":"visible"}],"hasMore":false}),
    );
    search.set_query("query").unwrap();
    wait_search(&search, |publication| {
        publication.children.iter().all(|child| {
            matches!(
                &child.publication.input.phase,
                SidebarSearchPhase::Complete { .. } | SidebarSearchPhase::Failed { .. }
            )
        })
    })
    .await;

    let projected = inventory
        .snapshot()
        .project_multi_search_for_display(&search.snapshot(), &inventory.snapshot().input(None))
        .unwrap();
    assert_eq!(projected.snippets[&address("endpoint-a", "a")], "visible");
    assert!(
        projected
            .input
            .rows
            .iter()
            .any(|row| { row.address == address("endpoint-a", "a") })
    );
    assert!(matches!(
        &projected.children["endpoint-b"].publication.input.phase,
        SidebarSearchPhase::Failed {
            retryable: true,
            ..
        }
    ));
    assert!(projected.input.search_help.is_none());
}

#[tokio::test]
async fn unresolved_hits_are_reported_without_fabricated_rows_and_clear_restores_inventory() {
    let (_agent, inventory, search, fixture_a, fixture_b) = setup(true, true).await;
    fixture_a.set_search_result(
        "query",
        json!({"items":[{"sessionId":"a","snippet":"known"},{"sessionId":"outside","snippet":"unknown"}],"hasMore":true}),
    );
    fixture_b.set_search_result(
        "query",
        json!({"items":[{"sessionId":"b","snippet":"known b"}],"hasMore":false}),
    );
    search.set_query("query").unwrap();
    wait_search(&search, complete).await;
    let snapshot = inventory.snapshot();
    let projection = snapshot
        .project_multi_search_for_display(&search.snapshot(), &snapshot.input(None))
        .unwrap();
    assert_eq!(
        projection.unresolved,
        vec![address("endpoint-a", "outside")]
    );
    assert!(
        !projection
            .input
            .rows
            .iter()
            .any(|row| row.address.session_id == "outside")
    );
    assert!(
        projection
            .input
            .rows
            .iter()
            .any(|row| { row.address == address("endpoint-b", "b") })
    );

    search.set_query("").unwrap();
    let cleared = search.snapshot();
    assert!(cleared.input.key.is_none());
    let restored = snapshot
        .project_multi_search_for_display(&cleared, &snapshot.input(None))
        .unwrap();
    assert_eq!(restored.input.rows, snapshot.rows);
    assert!(restored.snippets.is_empty());
    assert!(restored.unresolved.is_empty());
    assert!(restored.children.values().all(|child| {
        child.aggregate_key.is_none()
            && child.publication.input.key.is_none()
            && child.publication.input.phase == SidebarSearchPhase::Idle
    }));
}

#[tokio::test]
async fn rejects_duplicate_and_wrong_endpoint_children() {
    let (_agent, inventory, search, fixture_a, fixture_b) = setup(true, true).await;
    fixture_a.set_search_result("query", json!({"items":[],"hasMore":false}));
    fixture_b.set_search_result("query", json!({"items":[],"hasMore":false}));
    search.set_query("query").unwrap();
    wait_search(&search, complete).await;
    let publication = search.snapshot();
    let display = inventory.snapshot().input(None);

    let mut duplicate = publication.as_ref().clone();
    duplicate.children.push(duplicate.children[0].clone());
    assert!(
        inventory
            .snapshot()
            .project_multi_search_for_display(&duplicate, &display)
            .is_err()
    );

    let mut wrong_endpoint = publication.as_ref().clone();
    wrong_endpoint.children[0].endpoint_id = "outside".into();
    assert!(
        inventory
            .snapshot()
            .project_multi_search_for_display(&wrong_endpoint, &display)
            .is_err()
    );

    let mut wrong_query = publication.as_ref().clone();
    wrong_query.children[0].publication.query = "different".into();
    assert!(
        inventory
            .snapshot()
            .project_multi_search_for_display(&wrong_query, &display)
            .is_err()
    );
}

#[tokio::test]
async fn demo_consumes_aggregate_query_and_dispatches_original_failed_endpoint_retry() {
    use tekes_agent_components_ui::{Sidebar, SidebarMultiSearchStatus};
    use tekes_agent_presentation::RunScope;
    use tekes_agent_ui_bindings::RunBinding;
    use tekes_agent_ui_demo::{
        component_demo::{ComponentDemo, DemoCommand},
        sidebar_host::SidebarHostBinding,
    };
    use tekes_ui_controls::composition::AccessibilityAction;

    let (agent, _inventory, search, fixture_a, fixture_b) = setup(true, true).await;
    fixture_a.set_search_result(
        "query",
        json!({"items":[{"sessionId":"a","snippet":"A hit"}],"hasMore":false}),
    );
    fixture_b.set_search_failure(true);
    let binding = Arc::new(
        RunBinding::open_main(
            agent.endpoint("endpoint-a").unwrap().runtime.clone(),
            RunScope::main("endpoint-a", "a"),
        )
        .await
        .unwrap(),
    );
    let host = Arc::new(SidebarHostBinding::new(agent, ["endpoint-a", "endpoint-b"]).unwrap());
    let mut inventory_changes = host.subscribe();
    tokio::time::timeout(Duration::from_secs(4), async {
        while !host.snapshot().loaded {
            inventory_changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    let mut view = ComponentDemo::new(vec![binding]).unwrap();
    view.set_sidebar_host_reader(host.clone()).unwrap();
    view.set_sidebar_host_source(host.snapshot()).unwrap();
    view.sync().unwrap();
    search.set_query("query").unwrap();
    wait_search(&search, complete).await;
    view.bind_sidebar_multi_search_publication(search.owner(), &search.snapshot())
        .unwrap();
    let sidebar = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap();
    let rows = &sidebar.displayed_input().unwrap().rows;
    assert!(
        rows.iter()
            .any(|row| row.address == address("endpoint-a", "a"))
    );
    assert!(
        !rows
            .iter()
            .any(|row| row.address.endpoint_id == "endpoint-b")
    );
    assert!(sidebar.displayed_input().unwrap().search_help.is_none());
    assert_eq!(
        sidebar.search_snippet(&address("endpoint-a", "a")),
        Some("A hit")
    );
    let footer = sidebar.search_footer_id();
    let retry = view
        .tree
        .get(footer)
        .unwrap()
        .component::<SidebarMultiSearchStatus>()
        .unwrap()
        .retry_id("endpoint-b")
        .unwrap();
    let commands = view
        .accessibility_action(retry, None, AccessibilityAction::Press)
        .unwrap();
    let intent = commands
        .into_iter()
        .find_map(|command| match command {
            DemoCommand::SidebarMultiSearch(intent) => Some(intent),
            _ => None,
        })
        .expect("original SDK retry control must emit a multi-search intent");
    let tekes_agent_presentation::SidebarMultiSearchIntent::Retry { child_key, .. } = &intent;
    assert_eq!(child_key.endpoint_id, "endpoint-b");
    fixture_b.set_search_failure(false);
    fixture_b.set_search_result(
        "query",
        json!({"items":[{"sessionId":"b","snippet":"B recovered"}],"hasMore":false}),
    );
    search.retry(&intent).unwrap();
    wait_search(&search, |publication| {
        publication.children.iter().all(|child| {
            matches!(
                child.publication.input.phase,
                SidebarSearchPhase::Complete { .. }
            )
        })
    })
    .await;
    view.update_sidebar_multi_search_publication(search.owner(), &search.snapshot())
        .unwrap();
    let sidebar = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap();
    assert!(
        sidebar
            .displayed_input()
            .unwrap()
            .rows
            .iter()
            .any(|row| row.address == address("endpoint-b", "b"))
    );
    assert_eq!(
        sidebar.search_snippet(&address("endpoint-b", "b")),
        Some("B recovered")
    );
    assert!(
        search.retry(&intent).is_err(),
        "an old retry cannot replay the new child attempt"
    );
    search.set_query("").unwrap();
    view.update_sidebar_multi_search_publication(search.owner(), &search.snapshot())
        .unwrap();
    let sidebar = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap();
    assert_eq!(
        sidebar.displayed_input().unwrap().rows.len(),
        host.snapshot().rows.len()
    );
    assert!(
        sidebar
            .search_snippet(&address("endpoint-b", "b"))
            .is_none()
    );
}
