use serde_json::json;
use tekes_agent_ui_bindings::ModelChoices;
fn id() -> String {
    json!(["provider", "model"]).to_string()
}
fn raw() -> serde_json::Value {
    json!({"current":{"provider":"provider","model":"model","reasoningEffort":"retired"},"routableProviders":["provider"],"groups":[{"id":"provider","models":[{"id":"model","reasoning":{"efforts":[{"id":"low","name":"低"},{"id":"high","name":"高"}],"defaultEffort":"low"}},{"id":"other"}]}]})
}
#[test]
fn current_unknown_effort_is_visible_but_not_selectable_and_levels_keep_identity() {
    let source = ModelChoices::read(raw(), 3, true).unwrap();
    let reasoning = source.reasoning_choices(&id(), None).unwrap().unwrap();
    assert_eq!(reasoning.catalog.revision, 3);
    assert_eq!(reasoning.catalog.selected.as_deref(), Some("retired"));
    assert!(reasoning.selection("retired").is_err());
    assert_eq!(
        reasoning.selection("high").unwrap(),
        json!({"provider":"provider","model":"model","reasoningEffort":"high"})
    );
    assert_eq!(
        source
            .reasoning_choices(&id(), Some("high"))
            .unwrap()
            .unwrap()
            .catalog
            .selected
            .as_deref(),
        Some("high")
    );
    assert!(
        source
            .reasoning_choices(&json!(["provider", "other"]).to_string(), None)
            .unwrap()
            .is_none()
    );
    let disabled = ModelChoices::read(raw(), 1, false)
        .unwrap()
        .reasoning_choices(&id(), None)
        .unwrap()
        .unwrap();
    assert!(disabled.catalog.choices.iter().all(|r| !r.enabled));
    assert!(disabled.selection("high").is_err());
}
#[test]
fn malformed_effort_metadata_is_rejected_and_default_is_not_invented() {
    for metadata in [
        json!({"efforts":[{"id":"high"},{"id":"high"}]}),
        json!({"efforts":[{"id":""}]}),
        json!({"efforts":[{"id":"high"}],"defaultEffort":"missing"}),
        json!({"efforts":"invalid"}),
    ] {
        let mut value = raw();
        value["groups"][0]["models"][0]["reasoning"] = metadata;
        assert!(
            ModelChoices::read(value, 1, true)
                .unwrap()
                .reasoning_choices(&id(), None)
                .is_err()
        );
    }
    let mut value = raw();
    value["current"]
        .as_object_mut()
        .unwrap()
        .remove("reasoningEffort");
    value["groups"][0]["models"][0]["reasoning"]
        .as_object_mut()
        .unwrap()
        .remove("defaultEffort");
    let source = ModelChoices::read(value, 1, true).unwrap();
    assert!(
        source
            .reasoning_choices(&id(), None)
            .unwrap()
            .unwrap()
            .catalog
            .selected
            .is_none()
    );
}
#[tokio::test]
async fn model_panel_reasoning_choice_is_draft_only_and_applies_before_one_send() {
    use std::{sync::Arc, time::Duration};
    use tekes_agent_contract::PromptMode;
    use tekes_agent_presentation::{ComposerDestination, RunScope};
    use tekes_agent_session::SessionRuntime;
    use tekes_agent_ui_bindings::{MODEL_SELECTION_SETTING, ModelSelectionPreparation, RunBinding};
    use tekes_agent_ui_demo::{
        component_demo::{ComponentDemo, DemoCommand},
        fixture::Fixture,
    };
    use tekes_ui_controls::composition::AccessibilityAction;
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
    let mut view = ComponentDemo::new([a.clone(), b.clone()]).unwrap();
    let mut commands = view
        .accessibility_action(view.ids().model, None, AccessibilityAction::Press)
        .unwrap();
    let DemoCommand::ReadModels { binding, request } = commands.remove(0) else {
        panic!()
    };
    let source = binding.model_choices(1, "reasoning-catalog").await.unwrap();
    assert!(view.complete_model_catalog(&request, Ok(source)).unwrap());
    let high = view.reasoning_choice_id("high").unwrap();
    view.accessibility_action(high, None, AccessibilityAction::Press)
        .unwrap();
    assert!(view.model_choice_request().is_none());
    assert!(view.reasoning_choice_id("high").is_none());
    let selected = json!({"provider":"fixture","model":"initial","reasoningEffort":"high"});
    assert_eq!(
        a.current_run().unwrap().draft.settings[MODEL_SELECTION_SETTING],
        selected
    );
    assert!(b.current_run().unwrap().draft.settings.is_empty());
    assert!(
        runtime
            .call(
                "session.models",
                json!({"sessionId":"a"}),
                "before-reasoning-send"
            )
            .await
            .unwrap()["current"]["reasoningEffort"]
            .is_null()
    );
    let dest = ComposerDestination::run(a.scope().clone());
    a.edit_text(&dest, "中文推理消息".into()).unwrap();
    a.send_with(&dest, PromptMode::Queue, &ModelSelectionPreparation)
        .await
        .unwrap();
    let snapshot = runtime.snapshot();
    let events = &snapshot.sessions["a"].entries;
    let model = events
        .values()
        .find(|e| e.event.kind == "model/selection")
        .unwrap();
    let input = events
        .values()
        .find(|e| {
            e.event.kind == "user/message" && e.event.data.to_string().contains("中文推理消息")
        })
        .unwrap();
    assert_eq!(model.event.data, selected);
    assert!(model.event.seq < input.event.seq);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn unavailable_reasoning_fails_before_model_mutation_or_prompt_and_restores_draft() {
    use std::{collections::BTreeMap, sync::Arc, time::Duration};
    use tekes_agent_contract::PromptMode;
    use tekes_agent_presentation::{ComposerDestination, RunScope};
    use tekes_agent_session::SessionRuntime;
    use tekes_agent_ui_bindings::{MODEL_SELECTION_SETTING, ModelSelectionPreparation, RunBinding};
    use tekes_agent_ui_demo::fixture::Fixture;
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
    let dest = ComposerDestination::run(a.scope().clone());
    a.edit_text(&dest, "不可用档位不应发送".into()).unwrap();
    a.begin_settings(&dest)
        .unwrap()
        .commit(BTreeMap::from([(
            MODEL_SELECTION_SETTING.into(),
            json!({"provider":"fixture","model":"initial","reasoningEffort":"removed"}),
        )]))
        .unwrap();
    assert!(
        a.send_with(&dest, PromptMode::Queue, &ModelSelectionPreparation)
            .await
            .is_err()
    );
    let state = runtime.snapshot();
    assert!(
        state.sessions["a"]
            .entries
            .values()
            .all(|e| e.event.kind != "model/selection"
                && !(e.event.kind == "user/message"
                    && e.event.data.to_string().contains("不可用档位不应发送")))
    );
    assert_eq!(a.current_run().unwrap().draft.text, "不可用档位不应发送");
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn reasoning_callbacks_obey_ime_navigation_and_changed_model_fences() {
    use std::{collections::BTreeMap, sync::Arc, time::Duration};
    use tekes_agent_presentation::{ComposerDestination, RunScope};
    use tekes_agent_session::SessionRuntime;
    use tekes_agent_ui_bindings::{MODEL_SELECTION_SETTING, RunBinding};
    use tekes_agent_ui_demo::{
        component_demo::{ComponentDemo, DemoCommand, SELECT_A, SELECT_B},
        fixture::Fixture,
    };
    use tekes_ui_controls::composition::{AccessibilityAction, ContentEvent};
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
    let mut view = ComponentDemo::new([a.clone(), b.clone()]).unwrap();
    let mut commands = view
        .accessibility_action(view.ids().model, None, AccessibilityAction::Press)
        .unwrap();
    let DemoCommand::ReadModels { binding, request } = commands.remove(0) else {
        panic!()
    };
    let source = binding.model_choices(1, "fence-catalog").await.unwrap();
    view.complete_model_catalog(&request, Ok(source)).unwrap();
    let high = view.reasoning_choice_id("high").unwrap();
    view.tree.focus(Some(view.ids().editor), true);
    view.event(&ContentEvent::Preedit {
        text: "组字".into(),
        selection: 2..2,
        clauses: vec![],
        replacement: None,
    })
    .unwrap();
    assert!(
        view.accessibility_action(high, None, AccessibilityAction::Press)
            .unwrap()
            .is_empty()
    );
    assert!(a.current_run().unwrap().draft.settings.is_empty());
    view.event(&ContentEvent::Commit {
        text: "组字".into(),
        replacement: None,
    })
    .unwrap();
    view.accessibility_action(SELECT_B, None, AccessibilityAction::Press)
        .unwrap();
    view.accessibility_action(SELECT_A, None, AccessibilityAction::Press)
        .unwrap();
    view.accessibility_action(high, None, AccessibilityAction::Press)
        .unwrap();
    assert!(a.current_run().unwrap().draft.settings.is_empty());
    assert!(b.current_run().unwrap().draft.settings.is_empty());
    let mut commands = view
        .accessibility_action(view.ids().model, None, AccessibilityAction::Press)
        .unwrap();
    let DemoCommand::ReadModels { binding, request } = commands.remove(0) else {
        panic!()
    };
    view.complete_model_catalog(
        &request,
        Ok(binding
            .model_choices(1, "changed-model-catalog")
            .await
            .unwrap()),
    )
    .unwrap();
    let high = view.reasoning_choice_id("high").unwrap();
    let dest = ComposerDestination::run(a.scope().clone());
    let newer = json!({"provider":"fixture","model":"chosen"});
    a.begin_settings(&dest)
        .unwrap()
        .commit(BTreeMap::from([(
            MODEL_SELECTION_SETTING.into(),
            newer.clone(),
        )]))
        .unwrap();
    view.accessibility_action(high, None, AccessibilityAction::Press)
        .unwrap();
    assert_eq!(
        a.current_run().unwrap().draft.settings[MODEL_SELECTION_SETTING],
        newer
    );
    runtime.stop().await.unwrap();
}
