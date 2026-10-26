use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use tekes_agent_components_ui::{RunScope, Sidebar, SidebarLabels, SidebarSelectionNavigation};
use tekes_agent_contract::SessionAddress;
use tekes_agent_presentation::SidebarSearchPhase;
use tekes_agent_ui_bindings::{
    RunBinding, SidebarBinding, SidebarMultiEndpointBinding, SidebarMultiSearchBinding,
};
use tekes_agent_ui_demo::fixture::Fixture;
use tekes_agent_web::SharedAgent;

fn address(endpoint: &str, session: &str) -> SessionAddress {
    SessionAddress {
        endpoint_id: endpoint.into(),
        session_id: session.into(),
    }
}

fn summary(id: &str, title: &str, cwd: &str, fields: Value) -> Value {
    let mut result = json!({
        "sessionId": id,
        "title": title,
        "cwd": cwd,
        "updatedAt": 5_000.0,
        "running": false,
        "blank": false
    });
    for (key, value) in fields.as_object().unwrap() {
        result[key.as_str()] = value.clone();
    }
    result
}

async fn wait_mixed_inventory(source: &SidebarMultiEndpointBinding) {
    let mut updates = source.subscribe();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            let snapshot = source.snapshot();
            let visible: BTreeSet<_> = snapshot
                .rows
                .iter()
                .map(|row| row.address.clone())
                .collect();
            let corpus_ready = snapshot
                .name_corpora
                .get("endpoint-a")
                .is_some_and(|corpus| {
                    [
                        "a",
                        "b",
                        "ephemeral",
                        "fork",
                        "visible-c",
                        "visible-d",
                        "visible-e",
                    ]
                    .into_iter()
                    .all(|id| {
                        corpus
                            .documents
                            .iter()
                            .any(|document| document.address == address("endpoint-a", id))
                    })
                });
            let rows_ready = [
                address("endpoint-a", "b"),
                address("endpoint-a", "fork"),
                address("endpoint-a", "visible-c"),
                address("endpoint-a", "visible-d"),
                address("endpoint-a", "visible-e"),
                address("endpoint-b", "a"),
                address("endpoint-b", "b"),
            ]
            .into_iter()
            .all(|row| visible.contains(&row));
            if snapshot.loaded && corpus_ready && rows_ready {
                break;
            }
            updates.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}

async fn wait_single(source: &SidebarBinding) {
    let mut updates = source.subscribe();
    tokio::time::timeout(Duration::from_secs(4), async {
        while !source.snapshot().loaded {
            updates.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}

async fn wait_search(source: &SidebarMultiSearchBinding) {
    let mut updates = source.subscribe();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            let publication = source.snapshot();
            if publication.children.len() == 2
                && publication.children.iter().all(|child| {
                    matches!(
                        child.publication.input.phase,
                        SidebarSearchPhase::Complete { .. }
                    )
                })
            {
                break;
            }
            updates.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn mixed_endpoint_classification_filters_search_and_catalog_without_deleting_history() {
    let fixture_a = Fixture::default()
        .with_endpoint_id("endpoint-a")
        .with_session_search();
    fixture_a.install_unannounced_session("ephemeral", "needle ephemeral");
    fixture_a.install_unannounced_session("fork", "needle parent-only fork");
    for id in ["visible-c", "visible-d", "visible-e"] {
        fixture_a.install_unannounced_session(id, "needle visible session");
    }

    let fixture_b = Fixture::default()
        .with_endpoint_id("endpoint-b")
        .with_session_search();
    let mut agent = SharedAgent::default();
    agent
        .register(
            "endpoint-a".into(),
            "fixture".into(),
            "A".into(),
            Arc::new(fixture_a.clone()),
        )
        .unwrap();
    agent
        .register(
            "endpoint-b".into(),
            "fixture".into(),
            "B".into(),
            Arc::new(fixture_b.clone()),
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

    // The seeded `a` session has real canonical transcript history. Its explicit
    // subagent classification removes it from Sidebar membership only.
    for (id, title, cwd, fields) in [
        (
            "a",
            "hidden child",
            "/hidden-child",
            json!({"origin":"subagent","parentSessionId":"b"}),
        ),
        (
            "b",
            "needle shared A",
            "/shared-a",
            json!({"origin":"main","ephemeral":false}),
        ),
        (
            "ephemeral",
            "hidden ephemeral",
            "/hidden-ephemeral",
            json!({"origin":"main","ephemeral":true}),
        ),
        (
            "fork",
            "needle parent-only fork",
            "/fork-only",
            json!({"parentSessionId":"b"}),
        ),
        ("visible-c", "needle visible c", "/visible-c", json!({})),
        ("visible-d", "needle visible d", "/visible-d", json!({})),
        ("visible-e", "needle visible e", "/visible-e", json!({})),
    ] {
        fixture_a
            .publish_inventory_summary(summary(id, title, cwd, fields))
            .unwrap();
    }
    fixture_b
        .publish_inventory_summary(summary(
            "b",
            "needle shared B",
            "/shared-b",
            json!({"origin":"main","ephemeral":false}),
        ))
        .unwrap();

    let single_a = SidebarBinding::new(agent.clone(), "endpoint-a").unwrap();
    let aggregate =
        SidebarMultiEndpointBinding::new(agent.clone(), ["endpoint-a", "endpoint-b"]).unwrap();
    wait_mixed_inventory(&aggregate).await;

    let snapshot = aggregate.snapshot();
    wait_single(&single_a).await;
    let retained_a = snapshot.name_corpora.get("endpoint-a").unwrap();
    for id in ["a", "ephemeral", "fork"] {
        assert!(
            retained_a
                .documents
                .iter()
                .any(|doc| { doc.address == address("endpoint-a", id) })
        );
    }
    assert_eq!(
        retained_a
            .documents
            .iter()
            .find(|doc| doc.address == address("endpoint-a", "a"))
            .unwrap()
            .title,
        "hidden child"
    );
    assert_eq!(
        retained_a
            .documents
            .iter()
            .find(|doc| doc.address == address("endpoint-a", "ephemeral"))
            .unwrap()
            .title,
        "hidden ephemeral"
    );
    assert_eq!(
        snapshot
            .rows
            .iter()
            .filter(|row| row.address.endpoint_id == "endpoint-a")
            .map(|row| row.address.session_id.as_str())
            .collect::<BTreeSet<_>>(),
        ["b", "fork", "visible-c", "visible-d", "visible-e"].into()
    );
    assert_eq!(
        snapshot
            .rows
            .iter()
            .filter(|row| row.address.session_id == "b")
            .count(),
        2,
        "same session ID on separate endpoints remains two full addresses"
    );

    let catalog = snapshot.folder_catalog().unwrap();
    let catalog_paths: BTreeSet<_> = catalog
        .folders
        .iter()
        .filter_map(|folder| folder.path.as_deref())
        .collect();
    assert!(catalog_paths.contains(&"/fork-only"));
    assert!(!catalog_paths.contains(&"/hidden-child"));
    assert!(!catalog_paths.contains(&"/hidden-ephemeral"));

    // Provider hits deliberately include both hidden retained records. They
    // cannot mint rows; local FTS also sees their titles only in the corpus.
    fixture_a.set_search_result("hidden", json!({"items":[],"hasMore":false}));
    fixture_b.set_search_result("hidden", json!({"items":[],"hasMore":false}));
    fixture_a.set_search_result(
        "needle",
        json!({"items":[
            {"sessionId":"a","snippet":"child body"},
            {"sessionId":"ephemeral","snippet":"ephemeral body"},
            {"sessionId":"b","snippet":"shared A body"},
            {"sessionId":"fork","snippet":"fork body"},
            {"sessionId":"visible-c","snippet":"c body"},
            {"sessionId":"visible-d","snippet":"d body"},
            {"sessionId":"visible-e","snippet":"e body"}
        ],"hasMore":false}),
    );
    fixture_b.set_search_result(
        "needle",
        json!({"items":[{"sessionId":"b","snippet":"shared B body"}],"hasMore":false}),
    );
    let runtimes = ["endpoint-a", "endpoint-b"]
        .into_iter()
        .map(|endpoint| agent.endpoint(endpoint).unwrap().runtime.clone())
        .collect();
    let search = SidebarMultiSearchBinding::new(runtimes).unwrap();
    search.set_query("hidden").unwrap();
    wait_search(&search).await;
    let local_only = snapshot
        .project_multi_search_for_display_with_sort(
            &search.snapshot(),
            &snapshot.input(None),
            tekes_agent_components_ui::SidebarSort::RecentActivity,
        )
        .unwrap();
    assert!(local_only.help.local_results_ready);
    assert!(
        local_only.input.rows.is_empty(),
        "local title matches in retained child records cannot create Sidebar rows"
    );

    search.set_query("needle").unwrap();
    wait_search(&search).await;

    let hidden_child = address("endpoint-a", "a");
    let single_selected = single_a.read_selected(&hidden_child, 6).unwrap();
    assert!(!single_selected.has_active_row(&single_a));
    let mut single_display = single_a.snapshot().input(Some(hidden_child.clone()));
    assert!(!single_selected.project_into(&single_a, &mut single_display, 6));
    assert!(
        single_display
            .rows
            .iter()
            .all(|row| row.address != hidden_child)
    );

    let selected = aggregate.read_selected(&hidden_child, 7).unwrap();
    assert!(selected.is_current(&aggregate));
    assert!(!selected.has_active_row(&aggregate));
    let mut display = snapshot.input(Some(hidden_child.clone()));
    let search_projection = snapshot
        .project_multi_search_for_display_with_sort(
            &search.snapshot(),
            &display,
            tekes_agent_components_ui::SidebarSort::RecentActivity,
        )
        .unwrap();
    display = search_projection.input.clone();
    assert!(
        search_projection.unresolved.is_empty(),
        "known active children excluded from Sidebar must not be reported as missing inventory"
    );
    assert!(!selected.project_into(&aggregate, &mut display, 7));

    let displayed: BTreeSet<_> = display.rows.iter().map(|row| row.address.clone()).collect();
    assert_eq!(displayed.len(), 6);
    assert_eq!(
        displayed,
        [
            address("endpoint-a", "b"),
            address("endpoint-a", "fork"),
            address("endpoint-a", "visible-c"),
            address("endpoint-a", "visible-d"),
            address("endpoint-a", "visible-e"),
            address("endpoint-b", "b"),
        ]
        .into()
    );
    assert!(!displayed.contains(&hidden_child));
    assert!(!displayed.contains(&address("endpoint-a", "ephemeral")));

    let mut sidebar = Sidebar::new(9981, SidebarLabels::default());
    let publication = search.snapshot();
    sidebar
        .replace_multi_search_source(search.owner(), &publication)
        .unwrap();
    sidebar.update(&display).unwrap();
    sidebar
        .set_multi_search_projection_help(&search_projection.help)
        .unwrap();
    assert!(sidebar.paging().has_more);
    assert!(!sidebar.paging().selected_supplemented);
    assert!(sidebar.row_id(&hidden_child).is_none());
    sidebar
        .reconcile_navigation(&SidebarSelectionNavigation {
            revision: display.revision,
            selected: Some(hidden_child.clone()),
        })
        .unwrap();
    assert!(
        tekes_agent_ui_consumer_check::sidebar_visibility_target(&sidebar).is_none(),
        "a filtered retained child cannot become a navigation reveal target"
    );

    // Membership exclusion did not prune the canonical record or its transcript.
    let endpoint_a = agent.endpoint("endpoint-a").unwrap();
    let facts = agent.read_thread_inventory(&endpoint_a, "a").unwrap();
    assert!(facts.item.is_some());
    let child_run = RunBinding::open_main(
        endpoint_a.runtime.clone(),
        RunScope::main("endpoint-a", "a"),
    )
    .await
    .unwrap();
    let tekes_agent_ui_bindings::PaneSnapshot::Run { snapshot, .. } =
        child_run.snapshot().await.unwrap()
    else {
        panic!("the canonical child transcript remains readable");
    };
    assert!(
        snapshot.transcript.items.iter().any(|item| {
            item.text.contains("历史问题") || item.text.contains("历史回答")
        })
    );

    for endpoint in ["endpoint-a", "endpoint-b"] {
        agent
            .endpoint(endpoint)
            .unwrap()
            .runtime
            .stop()
            .await
            .unwrap();
    }
}
