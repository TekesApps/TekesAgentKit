use serde_json::json;
use std::{sync::Arc, time::Duration};
use tekes_agent_components_ui::{SidebarInput, SidebarSearchPhase};
use tekes_agent_contract::SessionAddress;
use tekes_agent_contract::ThreadPinPresentation;
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
async fn provider_membership_filters_same_id_pins_per_endpoint_once_and_clear_restores_both() {
    let (_agent, inventory, search, fixture_a, fixture_b) = setup(true, true).await;
    fixture_a.set_search_result(
        "query",
        json!({"items":[{"sessionId":"a","snippet":"A pin hit"}],"hasMore":false}),
    );
    fixture_b.set_search_result("query", json!({"items":[],"hasMore":false}));
    search.set_query("query").unwrap();
    wait_search(&search, complete).await;

    let mut snapshot = inventory.snapshot().as_ref().clone();
    for endpoint in ["endpoint-a", "endpoint-b"] {
        let pin = snapshot
            .rows
            .iter_mut()
            .find(|row| row.address == address(endpoint, "a"))
            .expect("both endpoint inventories contain the same session ID");
        pin.pin = Some(ThreadPinPresentation { pinned: true });
    }
    let displayed = snapshot.input(Some(address("endpoint-a", "a")));
    let projection = snapshot
        .project_multi_search_for_display(&search.snapshot(), &displayed)
        .unwrap();
    assert_eq!(
        projection
            .input
            .rows
            .iter()
            .filter(|row| row.address == address("endpoint-a", "a"))
            .count(),
        1,
        "a provider hit for a pin appears exactly once"
    );
    assert!(
        !projection
            .input
            .rows
            .iter()
            .any(|row| row.address == address("endpoint-b", "a")),
        "the same session ID on another endpoint is not a provider hit there"
    );
    assert_eq!(projection.input.selected, displayed.selected);

    search.set_query("").unwrap();
    let cleared = search.snapshot();
    let restored = snapshot
        .project_multi_search_for_display(&cleared, &displayed)
        .unwrap();
    assert_eq!(
        restored
            .input
            .rows
            .iter()
            .filter(|row| row.address.session_id == "a")
            .count(),
        2,
        "clearing the query restores the independent pins for both full addresses"
    );
    assert!(restored.input.rows.iter().all(|row| {
        row.address.session_id != "a" || row.pin.as_ref().is_some_and(|pin| pin.pinned)
    }));
    assert_eq!(restored.input.selected, displayed.selected);
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

    // A later canonical inventory can join an originally unresolved provider
    // hit without rewriting that original search publication's unresolved list.
    let original_search = search.snapshot();
    let mut joined = snapshot.as_ref().clone();
    joined.revision += 1;
    let mut outside = joined
        .rows
        .iter()
        .find(|row| row.address == address("endpoint-a", "a"))
        .unwrap()
        .clone();
    outside.address = address("endpoint-a", "outside");
    outside.actions.session_id = "outside".into();
    joined.activity_at.insert(outside.address.clone(), 0.0);
    joined.rows.push(outside);
    let recovered = joined
        .project_multi_search_for_display(&original_search, &joined.input(None))
        .unwrap();
    assert!(recovered.unresolved.is_empty());
    assert!(recovered.help.results_ready);
    let mut sidebar = tekes_agent_components_ui::Sidebar::new(9992, Default::default());
    sidebar
        .replace_multi_search_source(search.owner(), &original_search)
        .unwrap();
    sidebar.update(&recovered.input).unwrap();
    sidebar
        .set_multi_search_projection_help(&recovered.help)
        .unwrap();
    assert_eq!(
        sidebar.search_snippet(&address("endpoint-a", "outside")),
        Some("unknown")
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

#[tokio::test]
async fn multi_name_fts_preserves_full_addresses_when_every_provider_is_unsupported() {
    use tekes_agent_components_ui::{Sidebar, SidebarLabels};
    let (agent, inventory, search, fixture_a, fixture_b) = setup(false, false).await;
    for fixture in [&fixture_a, &fixture_b] {
        fixture.publish_inventory_summary(json!({"sessionId":"a","title":"Café design 中文","updatedAt":1000,"running":false,"blank":false})).unwrap();
    }
    let mut changes = inventory.subscribe();
    tokio::time::timeout(Duration::from_secs(4), async {
        while inventory
            .snapshot()
            .rows
            .iter()
            .filter(|row| row.title == "Café design 中文")
            .count()
            != 2
        {
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    search.set_query("cafe des").unwrap();
    wait_search(&search, complete).await;
    let snapshot = inventory.snapshot();
    let publication = search.snapshot();
    let projected = snapshot
        .project_multi_search_for_display(&publication, &snapshot.input(None))
        .unwrap();
    let addresses: std::collections::BTreeSet<_> = projected
        .input
        .rows
        .iter()
        .map(|row| row.address.clone())
        .collect();
    assert_eq!(
        addresses,
        [address("endpoint-a", "a"), address("endpoint-b", "a")].into()
    );
    assert!(projected.help.local_results_ready);
    assert!(!projected.help.results_ready);
    assert!(projected.snippets.is_empty());
    let mut sidebar = Sidebar::new(9991, SidebarLabels::default());
    sidebar
        .replace_multi_search_source(&publication.input.key.as_ref().unwrap().owner, &publication)
        .unwrap();
    sidebar.update(&projected.input).unwrap();
    sidebar
        .set_multi_search_projection_help(&projected.help)
        .unwrap();
    for address in addresses {
        assert!(sidebar.row_id(&address).is_some());
    }
    use tekes_agent_ui_bindings::RunBinding;
    use tekes_agent_ui_demo::{component_demo::ComponentDemo, sidebar_host::SidebarHostBinding};
    let run = Arc::new(
        RunBinding::open_main(
            agent.endpoint("endpoint-a").unwrap().runtime.clone(),
            tekes_agent_presentation::RunScope::main("endpoint-a", "a"),
        )
        .await
        .unwrap(),
    );
    let host = Arc::new(SidebarHostBinding::new(agent, ["endpoint-a", "endpoint-b"]).unwrap());
    let mut updates = host.subscribe();
    tokio::time::timeout(Duration::from_secs(4), async {
        while !host.snapshot().loaded {
            updates.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    let mut view = ComponentDemo::new([run]).unwrap();
    view.set_sidebar_host_reader(host.clone()).unwrap();
    view.set_sidebar_host_source(host.snapshot()).unwrap();
    view.bind_sidebar_multi_search_publication(search.owner(), &publication)
        .unwrap();
    view.sync().unwrap();
    let mounted = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap();
    assert_eq!(mounted.displayed_input().unwrap().rows.len(), 2);
    assert!(mounted.row_id(&address("endpoint-a", "a")).is_some());
    assert!(mounted.row_id(&address("endpoint-b", "a")).is_some());
}
