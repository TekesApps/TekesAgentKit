use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tekes_agent_components_ui::ComposerDestination;
use tekes_agent_contract::PromptMode;
use tekes_agent_presentation::RunScope;
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{
    MODEL_SELECTION_SETTING, ModelChoices, ModelSelectionPreparation, RunBinding,
};
use tekes_agent_ui_demo::{
    component_demo::{ComponentDemo, DemoCommand, SELECT_B},
    fixture::Fixture,
};
use tekes_ui_controls::composition::*;

fn id(provider: &str, model: &str) -> String {
    json!([provider, model]).to_string()
}
fn catalog() -> Value {
    json!({"current":{"provider":"old","model":"missing"},"groups":[{"id":"route","models":[{"id":"a:b","name":"模型"}]},{"id":"disabled","models":[{"id":"m"}]}],"routableProviders":["route"],"failures":[{"provider":"disabled","message":"供应商不可用"}]})
}
#[test]
fn public_catalog_keeps_framed_identity_unknown_current_failures_and_explicit_routes() {
    let raw = catalog();
    let parsed = ModelChoices::read(raw.clone(), 1, true).unwrap();
    assert_eq!(parsed.raw, raw);
    assert_eq!(parsed.catalog.selected, Some(id("old", "missing")));
    assert!(parsed.selection(&id("disabled", "m")).is_err());
    assert_eq!(
        parsed.selection(&id("route", "a:b")).unwrap(),
        json!({"provider":"route","model":"a:b"})
    );
    let mut unknown = catalog();
    unknown.as_object_mut().unwrap().remove("routableProviders");
    assert!(
        ModelChoices::read(unknown, 1, true)
            .unwrap()
            .catalog
            .choices
            .iter()
            .all(|c| !c.enabled)
    );
    assert!(
        ModelChoices::read(catalog(), 1, false)
            .unwrap()
            .catalog
            .choices
            .iter()
            .all(|c| !c.enabled)
    );
    let mut duplicate = catalog();
    duplicate["groups"][0]["models"] = json!([{"id":"a"},{"id":"a"}]);
    assert!(ModelChoices::read(duplicate, 1, true).is_err());
    let legacy = json!({"current":{"provider":"ready","model":"m"},"routable":true,"groups":[{"id":"ready","models":[{"id":"m"}]},{"id":"offline","models":[{"id":"m"}]}],"failures":[{"id":"offline","name":"Offline","message":"unavailable"}]});
    let parsed = ModelChoices::read(legacy, 1, true).unwrap();
    assert!(parsed.selection(&id("ready", "m")).is_ok());
    assert!(parsed.selection(&id("offline", "m")).is_err());
    let mut reasoning = parsed.raw.clone();
    reasoning["current"]["reasoningEffort"] = json!("high");
    assert_eq!(
        ModelChoices::read(reasoning, 1, true)
            .unwrap()
            .selection(&id("ready", "m"))
            .unwrap()["reasoningEffort"],
        "high",
        "reselecting the current model preserves its explicit reasoning selection"
    );
    let empty =
        json!({"current":{"provider":"","model":""},"routable":false,"groups":[],"failures":[]});
    assert!(
        ModelChoices::read(empty, 1, true)
            .unwrap()
            .catalog
            .selected
            .is_none()
    );
}
async fn setup() -> (Arc<SessionRuntime>, [Arc<RunBinding>; 2]) {
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(Fixture::default().with_models()),
    ));
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
    (runtime, [a, b])
}
fn open(
    view: &mut ComponentDemo,
) -> (
    Arc<RunBinding>,
    tekes_agent_components_ui::ComposerChoiceRequest,
) {
    let commands = view
        .accessibility_action(view.ids().model, None, AccessibilityAction::Press)
        .unwrap();
    let mut commands = commands.into_iter();
    let DemoCommand::ReadModels { binding, request } = commands.next().unwrap() else {
        panic!("model read")
    };
    assert!(commands.next().is_none());
    (binding, request)
}
#[tokio::test]
async fn demo_catalog_and_choice_use_original_binding_and_settings_apply_before_one_prompt() {
    let (runtime, bindings) = setup().await;
    let a = bindings[0].clone();
    let b = bindings[1].clone();
    a.begin_settings(&ComposerDestination::run(a.scope().clone()))
        .unwrap()
        .commit(std::collections::BTreeMap::from([(
            MODEL_SELECTION_SETTING.into(),
            json!({"provider":"fixture","model":"chosen","reasoningEffort":"high"}),
        )]))
        .unwrap();
    let mut view = ComponentDemo::new(bindings).unwrap();
    let (source, request) = open(&mut view);
    let options = source.model_choices(1, "model-catalog-test").await.unwrap();
    assert!(view.complete_model_catalog(&request, Ok(options)).unwrap());
    let disabled = view.model_choice_id(&id("unroutable", "disabled")).unwrap();
    assert!(
        view.accessibility_action(disabled, None, AccessibilityAction::Press)
            .unwrap()
            .is_empty()
    );
    let chosen = view.model_choice_id(&id("fixture", "chosen")).unwrap();
    view.accessibility_action(chosen, None, AccessibilityAction::Press)
        .unwrap();
    assert!(view.model_choice_request().is_none());
    assert!(
        view.tree
            .semantics()
            .iter()
            .any(|n| n.semantics.name.contains("发送时应用"))
    );
    assert_eq!(
        a.current_run().unwrap().draft.settings[MODEL_SELECTION_SETTING],
        json!({"provider":"fixture","model":"chosen","reasoningEffort":"high"})
    );
    assert!(b.current_run().unwrap().draft.settings.is_empty());
    assert_eq!(
        runtime
            .call("session.models", json!({"sessionId":"a"}), "before-send")
            .await
            .unwrap()["current"]["model"],
        "initial",
        "a UI choice alone does not claim applied configuration"
    );
    let destination = ComposerDestination::run(a.scope().clone());
    a.edit_text(&destination, "带模型选择的消息".into())
        .unwrap();
    a.send_with(&destination, PromptMode::Queue, &ModelSelectionPreparation)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let state = runtime.snapshot();
    let events = &state.sessions["a"].entries;
    let selected = events
        .values()
        .find(|e| e.event.kind == "model/selection" && e.event.data["model"] == "chosen")
        .unwrap();
    let inputs: Vec<_> = events
        .values()
        .filter(|e| {
            e.event.kind == "user/message" && e.event.data.to_string().contains("带模型选择的消息")
        })
        .collect();
    assert_eq!(inputs.len(), 1);
    assert!(selected.event.seq < inputs[0].event.seq);
    a.cancel(&destination).await.unwrap();
    let b_destination = ComposerDestination::run(b.scope().clone());
    let request = b.begin_settings(&b_destination).unwrap();
    request
        .commit(std::collections::BTreeMap::from([(
            MODEL_SELECTION_SETTING.into(),
            json!({"provider":"unroutable","model":"disabled"}),
        )]))
        .unwrap();
    b.edit_text(&b_destination, "不可路由时不得发送".into())
        .unwrap();
    assert!(
        b.send_with(
            &b_destination,
            PromptMode::Queue,
            &ModelSelectionPreparation
        )
        .await
        .is_err()
    );
    assert_eq!(b.current_run().unwrap().draft.text, "不可路由时不得发送");
    assert!(
        !runtime.snapshot().sessions["b"]
            .entries
            .values()
            .any(|e| e.event.kind == "user/message"
                && e.event.data.to_string().contains("不可路由时不得发送"))
    );
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn late_catalog_or_failure_cannot_reopen_a_new_navigation_or_discard_its_draft() {
    let (runtime, bindings) = setup().await;
    let mut view = ComponentDemo::new(bindings).unwrap();
    let (source, request) = open(&mut view);
    let options = source.model_choices(1, "late-catalog").await.unwrap();
    view.accessibility_action(SELECT_B, None, AccessibilityAction::Press)
        .unwrap();
    assert!(!view.complete_model_catalog(&request, Ok(options)).unwrap());
    assert!(
        !view
            .complete_model_catalog(&request, Err("迟到读取失败".into()))
            .unwrap()
    );
    assert!(view.model_choice_request().is_none());
    assert_eq!(view.selected(), 1);
    let (_, current) = open(&mut view);
    assert!(
        view.complete_model_catalog(&current, Err("当前目录失败".into()))
            .unwrap()
    );
    assert!(
        view.tree
            .semantics()
            .iter()
            .any(|n| n.semantics.name == "当前目录失败")
    );
    runtime.stop().await.unwrap();
}
