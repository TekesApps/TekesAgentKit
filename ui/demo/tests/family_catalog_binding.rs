use std::{sync::Arc, time::Duration};
use tekes_agent_components_ui::{
    FamilyCatalog, FamilyCatalogInput, FamilyCatalogIntent, FamilyCatalogLabels,
};
use tekes_agent_presentation::RunScope;
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{FamilyCatalogBinding, RunBinding};
use tekes_agent_ui_demo::fixture::Fixture;
use tekes_ui_controls::composition::*;

async fn setup() -> (Fixture, Arc<SessionRuntime>, Arc<RunBinding>) {
    let fixture = Fixture::default().with_children();
    let runtime = Arc::new(SessionRuntime::spawn("fixture", Arc::new(fixture.clone())));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let parent = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    (fixture, runtime, parent)
}

fn intent_from(input: &FamilyCatalogInput) -> FamilyCatalogIntent {
    let mut widget = FamilyCatalog::new(94001, FamilyCatalogLabels::default());
    widget.update(input).unwrap();
    let root = widget.id();
    let mut host = ContentHost::new(Content::new(root, widget)).unwrap();
    host.set_action_events(true);
    let child = host
        .get(root)
        .unwrap()
        .component::<FamilyCatalog>()
        .unwrap()
        .child_button_id("child-a")
        .expect("fixture catalog contains child-a");
    host.accessibility_action(child, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    host.get(root)
        .unwrap()
        .component::<FamilyCatalog>()
        .unwrap()
        .take_intents(&actions)
        .into_iter()
        .next()
        .expect("original SDK button press yields a source-bound intent")
}

#[tokio::test]
async fn actual_fixture_catalog_refresh_admits_current_intent_and_revokes_it_on_refresh_failure() {
    let (fixture, runtime, parent) = setup().await;
    let binding = FamilyCatalogBinding::for_run(&parent);

    binding.refresh().await.unwrap();
    let first = binding.snapshot();
    assert_eq!(first.parent, *parent.scope());
    assert!(first.catalog.as_ref().unwrap().child("child-a").is_some());
    assert!(!first.refreshing);
    assert!(first.error.is_none());
    let intent = intent_from(&first);
    assert!(binding.accepts(&intent));

    fixture.set_catalog_fails(true);
    assert!(binding.refresh().await.is_err());
    let failed = binding.snapshot();
    assert!(failed.generation > first.generation);
    assert!(failed.catalog.is_none());
    assert!(failed.error.is_some());
    assert!(!binding.accepts(&intent));

    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn a_new_catalog_generation_rejects_an_intent_from_the_previous_source_frame() {
    let (_, runtime, parent) = setup().await;
    let binding = FamilyCatalogBinding::for_run(&parent);
    binding.refresh().await.unwrap();
    let previous = binding.snapshot();
    let old_intent = intent_from(&previous);
    assert!(binding.accepts(&old_intent));

    binding.refresh().await.unwrap();
    let current = binding.snapshot();
    assert!(current.generation > previous.generation);
    assert!(current.revision > previous.revision);
    assert!(!binding.accepts(&old_intent));
    assert!(binding.accepts(&intent_from(&current)));

    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn dropping_parent_or_stopping_connection_revokes_the_catalog_source() {
    let (_, runtime, parent) = setup().await;
    let binding = FamilyCatalogBinding::for_run(&parent);
    binding.refresh().await.unwrap();
    let intent = intent_from(&binding.snapshot());
    assert!(binding.accepts(&intent));

    drop(parent);
    let retired = binding.snapshot();
    assert!(retired.catalog.is_none());
    assert!(retired.error.is_some());
    assert!(!binding.accepts(&intent));

    // A separate live parent proves that connection loss also retires a loaded source.
    let replacement = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let connected = FamilyCatalogBinding::for_run(&replacement);
    connected.refresh().await.unwrap();
    let connected_intent = intent_from(&connected.snapshot());
    runtime.stop().await.unwrap();
    let disconnected = connected.snapshot();
    assert!(disconnected.catalog.is_none());
    assert!(disconnected.error.is_some());
    assert!(!connected.accepts(&connected_intent));
}

#[tokio::test]
async fn a_rebuilt_owner_rejects_previous_intent_even_when_scope_and_versions_repeat() {
    let (_, runtime, parent) = setup().await;
    let first = FamilyCatalogBinding::for_run(&parent);
    first.refresh().await.unwrap();
    let original = first.snapshot();
    let intent = intent_from(&original);
    let second = FamilyCatalogBinding::for_run(&parent);
    second.refresh().await.unwrap();
    let replacement = second.snapshot();
    assert_eq!(original.parent, replacement.parent);
    assert_eq!(original.connection_epoch, replacement.connection_epoch);
    assert_eq!(original.generation, replacement.generation);
    assert_eq!(original.revision, replacement.revision);
    assert_ne!(original.source, replacement.source);
    assert!(first.accepts(&intent));
    assert!(!second.accepts(&intent));
    assert!(second.belongs_to(&parent));
    runtime.stop().await.unwrap();
}
