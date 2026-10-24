use std::{sync::Arc, time::Duration};
use tekes_agent_presentation::RunScope;
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::RunBinding;
use tekes_agent_ui_demo::{
    component_demo::{ComponentDemo, DemoCommand},
    fixture::Fixture,
};
use tekes_ui_controls::composition::AccessibilityAction;
async fn setup(fixture: Fixture) -> (Arc<SessionRuntime>, ComponentDemo) {
    let runtime = Arc::new(SessionRuntime::spawn("fixture", Arc::new(fixture)));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let a = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let b = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "b"))
            .await
            .unwrap(),
    );
    (runtime, ComponentDemo::new([a, b]).unwrap())
}
fn open(
    view: &mut ComponentDemo,
) -> (
    Arc<RunBinding>,
    tekes_agent_components_ui::ComposerChoiceRequest,
) {
    let mut commands = view
        .accessibility_action(view.ids().model, None, AccessibilityAction::Press)
        .unwrap();
    let DemoCommand::ReadModels { binding, request } = commands.remove(0) else {
        panic!()
    };
    (binding, request)
}
#[tokio::test]
async fn reconnect_fences_an_inflight_public_model_read_without_retry() {
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let fixture = Fixture::default()
        .with_models()
        .with_model_read_gate(gate.clone());
    let (runtime, mut view) = setup(fixture.clone()).await;
    let (binding, _) = open(&mut view);
    let read = tokio::spawn(async move { binding.model_choices(1, "gated-model-read").await });
    tokio::time::timeout(Duration::from_secs(3), async {
        while fixture.model_read_calls() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    runtime.reconnect().await.unwrap();
    gate.add_permits(1);
    assert!(read.await.unwrap().is_err());
    assert_eq!(fixture.model_read_calls(), 1);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn late_catalog_success_or_error_cannot_reopen_old_epoch_or_close_a_new_gesture() {
    let (runtime, mut view) = setup(Fixture::default().with_models()).await;
    let (binding, request) = open(&mut view);
    let old = binding.model_choices(1, "old-model-read").await.unwrap();
    runtime.reconnect().await.unwrap();
    assert!(!view.complete_model_catalog(&request, Ok(old)).unwrap());
    assert!(view.model_choice_request().is_none());
    let (binding, new_request) = open(&mut view);
    let new = binding.model_choices(1, "new-model-read").await.unwrap();
    assert!(view.complete_model_catalog(&new_request, Ok(new)).unwrap());
    let high = view.reasoning_choice_id("high").unwrap();
    assert!(
        !view
            .complete_model_catalog(&request, Err("late previous failure".into()))
            .unwrap()
    );
    assert_eq!(view.reasoning_choice_id("high"), Some(high));
    assert_eq!(view.model_choice_request(), Some(&new_request));
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn reconnect_retires_visible_reasoning_buttons_without_changing_drafts() {
    let (runtime, mut view) = setup(Fixture::default().with_models()).await;
    let (binding, request) = open(&mut view);
    let source = binding
        .model_choices(1, "visible-model-read")
        .await
        .unwrap();
    view.complete_model_catalog(&request, Ok(source)).unwrap();
    let high = view.reasoning_choice_id("high").unwrap();
    let draft = binding.current_run().unwrap().draft.clone();
    runtime.reconnect().await.unwrap();
    view.sync().unwrap();
    assert!(view.model_choice_request().is_none());
    assert!(view.reasoning_choice_id("high").is_none());
    view.accessibility_action(high, None, AccessibilityAction::Press)
        .unwrap();
    assert_eq!(binding.current_run().unwrap().draft, draft);
    runtime.stop().await.unwrap();
}
