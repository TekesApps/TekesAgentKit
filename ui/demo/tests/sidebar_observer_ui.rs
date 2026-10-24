use std::{sync::Arc, time::Duration};
use tekes_agent_components_ui::{Sidebar, SidebarLabels, SidebarObserverLabels};
use tekes_agent_contract::Result;
use tekes_agent_presentation::{
    RunScope, SidebarObserverEndpointInput, SidebarObserverIntent, SidebarObserverKey,
    SidebarObserverPhase, SidebarObserverSourceKey, SidebarObserverStatusInput,
};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::RunBinding;
use tekes_agent_ui_demo::{
    component_demo::ComponentDemo, fixture::Fixture, sidebar_host::SidebarHostBinding,
};
use tekes_agent_web::SharedAgent;
use tekes_ui_controls::composition::{ContentAction, ContentActionKind, ContentActionSource};

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

async fn runtime(agent: &Arc<SharedAgent>, endpoint: &str) -> Arc<SessionRuntime> {
    let runtime = agent.endpoint(endpoint).unwrap().runtime.clone();
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    runtime
}

#[tokio::test]
async fn component_demo_installs_only_original_host_observer_packets() -> Result<()> {
    let mut shared = SharedAgent::default();
    shared.register(
        "endpoint-a".into(),
        "native".into(),
        "fixture-a".into(),
        Arc::new(
            Fixture::default()
                .with_endpoint_id("endpoint-a")
                .with_lifecycle(),
        ),
    )?;
    let agent = Arc::new(shared);
    let runtime = runtime(&agent, "endpoint-a").await;
    let run = Arc::new(RunBinding::open_main(runtime, RunScope::main("endpoint-a", "a")).await?);
    let first = Arc::new(SidebarHostBinding::new(agent.clone(), ["endpoint-a"])?);
    loaded(&first).await;

    let mut view = ComponentDemo::new(vec![run])?;
    let sidebar_id = view.sidebar_id();
    view.set_sidebar_reader(Arc::new(tekes_agent_ui_bindings::SidebarBinding::new(
        agent.clone(),
        "endpoint-a",
    )?))?;
    // A legacy publication has no observer authorization key and cannot install
    // a Retry control, even when its row data is otherwise valid.
    let legacy = tekes_agent_ui_bindings::SidebarBinding::new(agent.clone(), "endpoint-a")?;
    let mut legacy_updates = legacy.subscribe();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if legacy.snapshot().loaded {
                break;
            }
            legacy_updates.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    view.set_sidebar_source(legacy.snapshot())?;
    view.sync()?;
    assert!(
        view.tree
            .get(sidebar_id)
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .observer_retry_id("endpoint-a")
            .is_none()
    );

    view.set_sidebar_host_reader(first.clone())?;
    let first_publication = first.snapshot();
    view.set_sidebar_host_source(first_publication.clone())?;
    view.sync()?;
    let sidebar = view
        .tree
        .get(sidebar_id)
        .unwrap()
        .component::<Sidebar>()
        .unwrap();
    assert!(sidebar.observer_retry_id("endpoint-a").is_none());
    assert_eq!(first_publication.observer_status().endpoints.len(), 1);
    assert_eq!(
        first_publication.observer_status().endpoints[0]
            .key
            .endpoint_id(),
        "endpoint-a"
    );

    let replacement = Arc::new(SidebarHostBinding::new(agent, ["endpoint-a"])?);
    loaded(&replacement).await;
    let replacement_key = replacement.snapshot().observer_status().endpoints[0]
        .key
        .clone();
    assert!(first.restart_observer_for(&replacement_key).is_err());
    view.set_sidebar_host_reader(replacement.clone())?;
    assert!(
        view.set_sidebar_host_source(first_publication).is_err(),
        "a packet from the retired host owner must be rejected"
    );
    let current = replacement.snapshot();
    let mut projected = current.as_publication();
    projected.revision += 1;
    projected.error = Some("unrelated fixture display error".into());
    let projected_packet = current.with_restore_fixture_projection(&projected)?;
    assert_eq!(
        projected_packet.inventory_error_for_display(),
        projected.error,
        "a healthy observer sidecar cannot hide an independent projection error"
    );
    view.set_sidebar_host_source(current.clone())?;
    view.sync()?;
    assert!(
        view.tree
            .get(sidebar_id)
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .observer_retry_id("endpoint-a")
            .is_none()
    );
    Ok(())
}

fn failed_status(
    source_owner: &Arc<()>,
    child_owner: &Arc<()>,
    endpoint: &str,
    generation: u64,
    revision: u64,
) -> Result<SidebarObserverStatusInput> {
    let key = SidebarObserverKey::from_owners(source_owner, child_owner, endpoint, generation)?;
    Ok(SidebarObserverStatusInput {
        source: SidebarObserverSourceKey::from_owner(source_owner),
        revision,
        endpoints: vec![SidebarObserverEndpointInput {
            key,
            revision,
            observation: tekes_agent_presentation::SidebarObservation {
                generation,
                phase: SidebarObserverPhase::Failed,
                converged: true,
            },
            error: Some("fixture observer failure".into()),
        }],
    })
}

#[test]
fn failed_observer_retry_is_typed_single_use_and_generation_scoped() -> Result<()> {
    let source_owner = Arc::new(());
    let child_owner = Arc::new(());
    let mut sidebar = Sidebar::new(187, SidebarLabels::default());
    let first = failed_status(&source_owner, &child_owner, "endpoint-a", 3, 1)?;
    sidebar
        .bind_observer_source(&first, SidebarObserverLabels::default())
        .unwrap();
    let retry = sidebar
        .observer_retry_id("endpoint-a")
        .expect("failed observer exposes Retry");
    let action = || ContentAction {
        target: retry,
        kind: ContentActionKind::Activate,
        source: ContentActionSource::Programmatic,
    };
    let intents = sidebar.take_observer_intents(&[action(), action()]);
    assert_eq!(
        intents.len(),
        1,
        "duplicate clicks share one pending intent"
    );
    assert!(matches!(
        &intents[0],
        SidebarObserverIntent::Retry { key } if key == &first.endpoints[0].key
    ));
    assert!(sidebar.take_observer_intents(&[action()]).is_empty());

    let next = failed_status(&source_owner, &child_owner, "endpoint-a", 4, 2)?;
    sidebar.update_observer_source(&next).unwrap();
    assert!(sidebar.take_observer_intents(&[action()]).is_empty());
    assert_ne!(sidebar.observer_retry_id("endpoint-a"), Some(retry));
    Ok(())
}
