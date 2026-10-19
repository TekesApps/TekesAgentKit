use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tekes_agent_components_ui::*;
use tekes_agent_contract::{CreateSessionRequest, PromptMode};
use tekes_agent_presentation::{ComposerDestination, RunController, RunScope};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::InitialPresetChoices;
use tekes_agent_ui_demo::fixture::Fixture;
use tekes_ui_controls::composition::*;

fn raw() -> Value {
    json!({"presets":[{"id":"coding","title":"编程","description":"中文说明","configurationText":"SOURCE_NOT_PROMPT"},{"id":"broken","title":"不可用","unavailableReason":"缺少配置"}],"defaultID":"coding"})
}
fn destination() -> ComposerDestination {
    ComposerDestination::NewThread {
        endpoint_id: "fixture".into(),
        workspace_id: "workspace".into(),
    }
}
fn input() -> CreateSessionRequest {
    CreateSessionRequest {
        cwd: Some("/fixture".into()),
        workspace_id: Some("workspace".into()),
        session_id: Some("preset-created".into()),
        agent_preset: None,
    }
}
fn selection(source: &InitialPresetChoices) -> ComposerChoiceSelection {
    ComposerChoiceSelection {
        request: ComposerChoiceRequest {
            generation: 1,
            destination: destination(),
            kind: ComposerChoiceKind::Preset,
        },
        catalog_revision: source.catalog.revision,
        choice: source.catalog.choices[0].clone(),
    }
}
#[test]
fn initial_catalog_retains_source_and_default_without_inventing_a_selection() {
    let original = raw();
    let source = InitialPresetChoices::read(original.clone(), 1, true).unwrap();
    assert_eq!(source.raw, original);
    assert_eq!(source.default_id.as_deref(), Some("coding"));
    assert!(source.catalog.selected.is_none());
    assert!(!source.catalog.choices[1].enabled);
    assert!(
        InitialPresetChoices::read(raw(), 1, false)
            .unwrap()
            .catalog
            .choices
            .iter()
            .all(|c| !c.enabled)
    );
    let roster = json!({"presets":[{"id":"default","name":"默认","isDefault":true},{"id":"broken","broken":"损坏"}]});
    let parsed = InitialPresetChoices::read(roster, 1, true).unwrap();
    assert_eq!(parsed.default_id.as_deref(), Some("default"));
    assert_eq!(parsed.catalog.choices[0].title, "默认");
    assert!(!parsed.catalog.choices[1].enabled);
}
#[test]
fn malformed_duplicate_and_ambiguous_preset_sources_are_rejected() {
    for value in [
        json!({"presets":[{"id":"a"},{"id":"a"}]}),
        json!({"presets":[{"id":"a","broken":false}]}),
        json!({"presets":[{"id":"a"}],"defaultID":"missing"}),
        json!({"presets":[{"id":"a","isDefault":true},{"id":"b","isDefault":true}]}),
        json!({"presets":[{"id":"a","isDefault":true},{"id":"b"}],"defaultID":"b"}),
        json!({"presets":[{"id":"a","configurationText":{}}]}),
    ] {
        assert!(InitialPresetChoices::read(value, 1, true).is_err());
    }
    assert!(InitialPresetChoices::read(raw(), 0, true).is_err());
}
#[test]
fn create_inputs_reject_existing_runs_stale_forged_disabled_and_conflicting_choices() {
    let source = InitialPresetChoices::read(raw(), 1, true).unwrap();
    let chosen = selection(&source);
    let request = source.create_request(&chosen, input()).unwrap();
    assert_eq!(request.agent_preset.as_deref(), Some("coding"));
    assert_eq!(request.cwd, input().cwd);
    assert_eq!(request.session_id, input().session_id);
    let mut invalid = chosen.clone();
    invalid.request.destination = ComposerDestination::run(RunScope::main("fixture", "a"));
    assert!(source.create_request(&invalid, input()).is_err());
    invalid = chosen.clone();
    invalid.catalog_revision = 2;
    assert!(source.create_request(&invalid, input()).is_err());
    invalid = chosen.clone();
    invalid.choice.title = "伪造".into();
    assert!(source.create_request(&invalid, input()).is_err());
    invalid = chosen.clone();
    invalid.choice = source.catalog.choices[1].clone();
    invalid.choice.enabled = true;
    invalid.choice.disabled_reason = None;
    assert!(source.create_request(&invalid, input()).is_err());
    let mut wrong = input();
    wrong.workspace_id = Some("other".into());
    assert!(source.create_request(&chosen, wrong).is_err());
    let mut wrong = input();
    wrong.agent_preset = Some("other".into());
    assert!(source.create_request(&chosen, wrong).is_err());
}
#[tokio::test]
async fn failed_fresh_catalog_or_absent_capability_never_creates_or_selects_a_default() {
    let fixture = Fixture::default().with_presets(false);
    let runtime = Arc::new(SessionRuntime::spawn("fixture", Arc::new(fixture.clone())));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let source = InitialPresetChoices::load(&runtime, 1, "catalog")
        .await
        .unwrap();
    fixture.set_catalog_fails(true);
    assert!(
        InitialPresetChoices::revalidate_create(
            &runtime,
            &selection(&source),
            input(),
            "fresh-catalog"
        )
        .await
        .is_err()
    );
    assert!(fixture.creation_receipts().is_empty());
    runtime.stop().await.unwrap();
    assert!(
        InitialPresetChoices::load(&runtime, 2, "stopped")
            .await
            .is_err()
    );
    let unsupported = SessionRuntime::spawn("fixture", Arc::new(Fixture::default()));
    unsupported
        .wait_ready(Duration::from_secs(3))
        .await
        .unwrap();
    assert!(
        InitialPresetChoices::load(&unsupported, 1, "unsupported")
            .await
            .is_err()
    );
    unsupported.stop().await.unwrap();
}
#[tokio::test]
async fn public_picker_feeds_shared_new_thread_create_and_one_prompt_for_both_catalog_dialects() {
    for legacy in [false, true] {
        let fixture = Fixture::default().with_presets(legacy);
        let runtime = Arc::new(SessionRuntime::spawn("fixture", Arc::new(fixture.clone())));
        runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
        let source = InitialPresetChoices::load(&runtime, 1, "initial-presets")
            .await
            .unwrap();
        let controller = RunController::new(runtime.clone()).unwrap();
        controller.select(destination()).unwrap();
        controller
            .edit_text(&destination(), "预设创建后只发送一次".into())
            .unwrap();
        let mut view = ComposerChoices::new(97, Default::default());
        let request = view
            .open(destination(), ComposerChoiceKind::Preset)
            .unwrap();
        view.update(&request, source.catalog.clone()).unwrap();
        let root = view.id();
        let row = view.choice_id("coding").unwrap();
        let broken = view.choice_id("broken").unwrap();
        let mut host = view.into_host().unwrap();
        assert!(
            !host
                .semantics()
                .iter()
                .any(|n| n.semantics.name.contains("SOURCE_NOT_PROMPT"))
        );
        host.accessibility_action(broken, None, AccessibilityAction::Press);
        let actions = host.drain_actions();
        assert!(
            host.get_mut(root)
                .unwrap()
                .component_mut::<ComposerChoices>()
                .unwrap()
                .take_intents(&actions)
                .is_empty()
        );
        host.accessibility_action(row, None, AccessibilityAction::Press);
        host.accessibility_action(row, None, AccessibilityAction::Press);
        let actions = host.drain_actions();
        let intents = host
            .get_mut(root)
            .unwrap()
            .component_mut::<ComposerChoices>()
            .unwrap()
            .take_intents(&actions);
        assert_eq!(intents.len(), 1);
        let ComposerChoiceIntent::Select(chosen) = &intents[0] else {
            panic!("selection")
        };
        let create = InitialPresetChoices::revalidate_create(
            &runtime,
            chosen,
            input(),
            "preset-before-create",
        )
        .await
        .unwrap();
        let ticket = controller
            .prepare(&destination(), PromptMode::Queue)
            .unwrap();
        controller.dispatch(ticket, Some(create)).await.unwrap();
        tokio::time::timeout(Duration::from_secs(4), async {
            loop {
                let snapshot = controller.snapshot();
                if snapshot.can_submit
                    && snapshot
                        .transcript
                        .items
                        .iter()
                        .any(|i| i.kind == tekes_agent_presentation::TranscriptItemKind::Assistant)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(fixture.creation_receipts().len(), 1);
        assert_eq!(fixture.creation_receipts()[0]["agentPreset"], "coding");
        assert_eq!(
            controller
                .snapshot()
                .transcript
                .items
                .iter()
                .filter(|i| i.text == "预设创建后只发送一次")
                .count(),
            1
        );
        assert!(
            !controller
                .snapshot()
                .transcript
                .items
                .iter()
                .any(|i| i.text.contains("SOURCE_NOT_PROMPT"))
        );
        let mut other = (**chosen).clone();
        other.request.destination = ComposerDestination::NewThread {
            endpoint_id: "other".into(),
            workspace_id: "workspace".into(),
        };
        assert!(
            InitialPresetChoices::revalidate_create(&runtime, &other, input(), "other")
                .await
                .is_err()
        );
        runtime.stop().await.unwrap();
    }
}
