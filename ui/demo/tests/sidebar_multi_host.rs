use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tekes_agent_components_ui::{SidePanel, Sidebar};
use tekes_agent_contract::{SessionAddress, ThreadActionDefinition};
use tekes_agent_presentation::RunScope;
use tekes_agent_ui_bindings::{RunBinding, SideChatBinding, SidebarActionOutcome, SidebarBinding};
use tekes_agent_ui_demo::{
    component_demo::{ComponentDemo, DemoCommand},
    fixture::Fixture,
    sidebar_host::SidebarHostBinding,
};
use tekes_agent_web::SharedAgent;
use tekes_ui_controls::composition::AccessibilityAction;

fn address(endpoint: &str) -> SessionAddress {
    SessionAddress {
        endpoint_id: endpoint.into(),
        session_id: "a".into(),
    }
}

async fn loaded(source: &SidebarHostBinding) {
    let mut changes = source.subscribe();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if source.snapshot().loaded {
                break;
            }
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn component_demo_uses_multi_inventory_action_routing_and_owner_fences() {
    let mut agent = SharedAgent::default();
    for endpoint in ["endpoint-a", "endpoint-b"] {
        agent
            .register(
                endpoint.into(),
                "native".into(),
                endpoint.into(),
                Arc::new(
                    Fixture::default()
                        .with_endpoint_id(endpoint)
                        .with_lifecycle(),
                ),
            )
            .unwrap();
    }
    let agent = Arc::new(agent);
    let runtimes: BTreeMap<_, _> = ["endpoint-a", "endpoint-b"]
        .into_iter()
        .map(|id| {
            let runtime = agent.endpoint(id).unwrap().runtime.clone();
            (id.to_owned(), runtime)
        })
        .collect();
    for runtime in runtimes.values() {
        runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    }
    let runs = vec![
        Arc::new(
            RunBinding::open_main(
                runtimes["endpoint-a"].clone(),
                RunScope::main("endpoint-a", "a"),
            )
            .await
            .unwrap(),
        ),
        Arc::new(
            RunBinding::open_main(
                runtimes["endpoint-b"].clone(),
                RunScope::main("endpoint-b", "a"),
            )
            .await
            .unwrap(),
        ),
    ];
    let navigation_target_binding = runs[1].clone();
    let host =
        Arc::new(SidebarHostBinding::new(agent.clone(), ["endpoint-a", "endpoint-b"]).unwrap());
    loaded(&host).await;
    let first_publication = host.snapshot();
    let corpora = first_publication
        .as_multi_publication()
        .unwrap()
        .name_corpora;
    assert_eq!(corpora.len(), 2);
    for (endpoint_id, corpus) in &corpora {
        assert_eq!(&corpus.endpoint_id, endpoint_id);
        assert_eq!(corpus.endpoint_domain, ["endpoint-a", "endpoint-b"]);
        assert!(
            corpus
                .documents
                .iter()
                .all(|document| &document.address.endpoint_id == endpoint_id)
        );
    }
    let mut forged = first_publication.as_publication();
    forged.revision += 1;
    forged.rows[0].title = "forged source row".into();
    assert!(
        first_publication
            .with_restore_fixture_projection(&forged)
            .is_err()
    );
    let mut restricted = first_publication.as_publication();
    restricted.observation.generation += 1;
    assert!(
        first_publication
            .with_restore_fixture_projection(&restricted)
            .is_err()
    );
    restricted.observation = first_publication.observation;
    restricted.revision += 1;
    restricted.loaded = false;
    restricted.rows[0].activity.is_stale = true;
    assert!(
        first_publication
            .with_restore_fixture_projection(&restricted)
            .is_ok()
    );
    let legacy_single = Arc::new(SidebarBinding::new(agent.clone(), "endpoint-a").unwrap());
    let mut legacy_updates = legacy_single.subscribe();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if legacy_single.snapshot().loaded {
                break;
            }
            legacy_updates.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    let legacy_publication = legacy_single.snapshot();
    let duplicated: Vec<_> = first_publication
        .rows
        .iter()
        .filter(|row| row.address.session_id == "a")
        .collect();
    assert_eq!(duplicated.len(), 2);
    assert_ne!(
        duplicated[0].address.endpoint_id,
        duplicated[1].address.endpoint_id
    );
    let folders = first_publication.folder_catalog().unwrap();
    assert_eq!(
        folders.folders.len(),
        2,
        "same path remains endpoint-qualified"
    );

    let mut view = ComponentDemo::new(runs.clone()).unwrap();
    view.set_sidebar_reader(legacy_single).unwrap();
    view.set_sidebar_source(legacy_publication.clone()).unwrap();
    view.sync().unwrap();
    view.set_sidebar_host_reader(host.clone()).unwrap();
    view.set_sidebar_host_source(first_publication.clone())
        .unwrap();
    view.set_side_chat_binding(Arc::new(SideChatBinding::new(
        runtimes["endpoint-a"].clone(),
    )));
    view.sync().unwrap();
    let displayed = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .displayed_input()
        .unwrap();
    assert_eq!(
        displayed
            .rows
            .iter()
            .filter(|row| row.address.session_id == "a")
            .count(),
        2
    );

    let target = address("endpoint-b");
    let read = host.read_selected(&target, 7).unwrap();
    let mut page = first_publication
        .as_publication()
        .input(Some(target.clone()));
    page.rows.retain(|row| row.address != target);
    assert!(
        read.project_into(&host, &mut page, 7),
        "selected row comes from its original endpoint"
    );
    assert!(page.rows.iter().any(|row| row.address == target));
    view.select(1).unwrap();
    assert_eq!(view.sidebar_visibility_target().unwrap().address(), &target);
    assert_eq!(
        view.tree
            .get(view.panel_id())
            .unwrap()
            .component::<SidePanel>()
            .unwrap()
            .displayed_input()
            .unwrap()
            .supports_side_chat,
        Some(false),
        "the first endpoint's SideChat owner is explicitly unavailable for endpoint B"
    );

    let mut navigation_view = ComponentDemo::new(vec![runs[0].clone()]).unwrap();
    navigation_view
        .set_sidebar_host_reader(host.clone())
        .unwrap();
    navigation_view
        .set_sidebar_host_source(first_publication.clone())
        .unwrap();
    navigation_view.sync().unwrap();
    let select = navigation_view
        .tree
        .get(navigation_view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .row_id(&target)
        .unwrap();
    let old_navigation = navigation_view
        .accessibility_action(select, None, AccessibilityAction::Press)
        .unwrap()
        .into_iter()
        .find_map(|command| match command {
            DemoCommand::OpenSession(request) => Some(request),
            _ => None,
        })
        .expect("second endpoint selection starts a host navigation request");

    let component = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap();
    let archive: ThreadActionDefinition = component
        .displayed_input()
        .unwrap()
        .rows
        .iter()
        .find(|row| row.address == target)
        .unwrap()
        .actions
        .items
        .iter()
        .find(|action| action.id == "archive" && action.enabled)
        .unwrap()
        .clone();
    // Capture the exact shared definition. Menu gesture dispatch has its own
    // component suite; this fixture verifies the host boundary and actual IO.
    let command = (target.clone(), archive);
    let token = view.begin_sidebar_action(command.0.clone(), command.1.id.clone());
    let outcome = host
        .perform_action(&command.0, &command.1, None, "multi-host-archive")
        .await
        .unwrap();
    let mutation = match outcome {
        SidebarActionOutcome::Mutation { outcome, .. } => outcome,
        other => panic!("unexpected action: {other:?}"),
    };
    let changed = tokio::time::timeout(Duration::from_secs(5), async {
        let mut changes = host.subscribe();
        loop {
            let publication = host.snapshot();
            if !publication.rows.iter().any(|row| row.address == target) {
                break publication;
            }
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert!(
        changed
            .rows
            .iter()
            .any(|row| row.address == address("endpoint-a")),
        "same session ID at endpoint A remains untouched"
    );

    let copy_target = address("endpoint-a");
    let copy_definition = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .displayed_input()
        .unwrap()
        .rows
        .iter()
        .find(|row| row.address == copy_target)
        .unwrap()
        .actions
        .items
        .iter()
        .find(|action| action.id == "copySessionId" && action.enabled)
        .unwrap()
        .clone();
    let copy_address = copy_target;
    view.tree
        .get_mut(view.sidebar_id())
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .set_pending(&copy_address, &copy_definition.id, true);
    let copy_token = view.begin_sidebar_action(copy_address.clone(), copy_definition.id.clone());
    let copy_pending = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .is_pending(&copy_address, &copy_definition.id);
    assert!(copy_pending, "old owner has a locally pending action");
    assert!(matches!(
        host.perform_action(&copy_address, &copy_definition, None, "multi-host-copy")
            .await
            .unwrap(),
        SidebarActionOutcome::Copy { .. }
    ));

    let old_read = host.read_selected(&address("endpoint-a"), 8).unwrap();
    let replacement =
        Arc::new(SidebarHostBinding::new(agent.clone(), ["endpoint-a", "endpoint-b"]).unwrap());
    loaded(&replacement).await;
    view.set_sidebar_host_reader(replacement.clone()).unwrap();
    let replacement_publication = replacement.snapshot();
    view.set_sidebar_host_source(replacement_publication.clone())
        .unwrap();
    view.sync().unwrap();
    navigation_view
        .set_sidebar_host_reader(replacement.clone())
        .unwrap();
    navigation_view
        .set_sidebar_host_source(replacement_publication.clone())
        .unwrap();
    navigation_view.sync().unwrap();
    assert!(
        !navigation_view
            .complete_navigation(old_navigation, navigation_target_binding)
            .unwrap(),
        "owner replacement retires a pending navigation result"
    );
    assert_eq!(
        navigation_view
            .current_binding()
            .unwrap()
            .scope()
            .endpoint_id,
        "endpoint-a",
        "stale navigation cannot change the selected run"
    );
    let sidebar = view
        .tree
        .get_mut(view.sidebar_id())
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap();
    assert!(!sidebar.is_pending(&copy_address, &copy_definition.id));
    sidebar.set_pending(&copy_address, &copy_definition.id, true);
    view.finish_sidebar_action(&copy_token);
    assert!(
        view.tree
            .get(view.sidebar_id())
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .is_pending(&copy_address, &copy_definition.id),
        "late old-owner completion cannot clear new owner's pending gesture"
    );
    assert!(
        !old_read.is_current(&replacement),
        "old selected read is owner-fenced"
    );
    assert!(
        view.set_sidebar_host_source(first_publication).is_err(),
        "old publication cannot overwrite replacement revision 1"
    );
    assert!(
        view.set_sidebar_source(legacy_publication).is_err(),
        "late legacy single-endpoint source cannot replace the multi-endpoint host"
    );
    assert!(
        view.tree
            .get(view.sidebar_id())
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .displayed_input()
            .unwrap()
            .rows
            .iter()
            .any(|row| row.address == address("endpoint-a"))
    );
    assert!(
        view.apply_sidebar_outcome(&token, &mutation)
            .unwrap()
            .is_none(),
        "old action completion cannot navigate after owner replacement"
    );
}
