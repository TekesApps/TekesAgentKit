use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tekes_agent_components_ui::{ActionableCard, ActionableResponseIntent};
use tekes_agent_presentation::{ActionablePrompt, RunScope};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::RunBinding;
use tekes_agent_ui_demo::{
    component_demo::{ComponentDemo, DemoCommand, SELECT_A, SELECT_B},
    fixture::Fixture,
};
use tekes_ui_controls::composition::{AccessibilityAction, ContentEvent};
fn approval(revision: i64) -> Value {
    json!({"id":"approve","sessionId":"a","revision":revision,"kind":"approval","payload":{"sessionId":"a","approvalId":"permit","toolName":"测试工具","reason":"仅本地 fixture"}})
}
fn question() -> Value {
    json!({"id":"question","sessionId":"a","revision":0,"kind":"question","payload":{"sessionId":"a","questions":[{"id":"q","question":"选择一个","options":[{"label":"中文"},{"label":"English"}],"multiSelect":false}]}})
}
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
fn press(
    view: &mut ComponentDemo,
    choice: &str,
) -> (
    Arc<tekes_agent_ui_bindings::ActionableResponseRequest>,
    tekes_agent_components_ui::ComposerIds,
    ActionableResponseIntent,
) {
    let ids = view.ids();
    let id = view.actionable_choice_id(ids, choice).unwrap();
    let mut commands = view
        .accessibility_action(id, None, AccessibilityAction::Press)
        .unwrap();
    assert_eq!(commands.len(), 1);
    let DemoCommand::RespondActionable {
        request,
        ids,
        intent,
    } = commands.remove(0)
    else {
        panic!("response")
    };
    (request, ids, intent)
}
async fn wait_card(view: &mut ComponentDemo, choice: &str) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            view.sync().unwrap();
            if view.actionable_choice_id(view.ids(), choice).is_some() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[test]
fn exact_shared_outcomes_and_unsupported_forms_do_not_invent_answers() {
    let scope = RunScope::main("fixture", "a");
    let p = ActionablePrompt::read(scope.clone(), approval(2)).unwrap();
    assert_eq!(
        p.choice("allow").unwrap().outcome(),
        &json!({"sessionId":"a","approvalId":"permit","outcome":"allowed-once"})
    );
    assert_eq!(p.choice("deny").unwrap().outcome()["outcome"], "rejected");
    assert!(p.choice("allowAlways").is_none());
    let p = ActionablePrompt::read(scope.clone(), question()).unwrap();
    assert_eq!(
        p.choice("option-0").unwrap().outcome(),
        &json!({"sessionId":"a","answer":{"answers":[{"id":"q","selected":["中文"]}]}})
    );
    for field in ["isSecret", "multiSelect"] {
        let mut q = question();
        q["payload"]["questions"][0][field] = json!(true);
        assert!(
            ActionablePrompt::read(scope.clone(), q)
                .unwrap()
                .choices
                .is_empty()
        );
    }
    for field in ["isSecret", "multiSelect"] {
        let mut malformed = question();
        malformed["payload"]["questions"][0][field] = json!("true");
        assert!(
            ActionablePrompt::read(scope.clone(), malformed)
                .unwrap()
                .choices
                .is_empty()
        );
    }
    let mut q = question();
    q["payload"]["questions"][0]["options"] = json!([{"label":"same"},{"label":"same"}]);
    assert!(
        ActionablePrompt::read(scope.clone(), q)
            .unwrap()
            .choices
            .is_empty()
    );
    let mut q = question();
    q["kind"] = json!("codex.mcpForm");
    assert!(
        ActionablePrompt::read(scope.clone(), q)
            .unwrap()
            .choices
            .is_empty()
    );
    let mut q = question();
    q["payload"]["sessionId"] = json!("b");
    assert!(ActionablePrompt::read(scope, q).is_err());
}
#[tokio::test]
async fn public_approval_entry_responds_once_to_original_scope_after_navigation() {
    let fixture = Fixture::default();
    fixture.publish_actionable(approval(0));
    let (runtime, mut view) = setup(fixture.clone()).await;
    wait_card(&mut view, "allow").await;
    let before = view.binding().current_run().unwrap().draft.clone();
    let (request, ids, intent) = press(&mut view, "allow");
    assert!(
        view.accessibility_action(
            view.actionable_choice_id(ids, "allow").unwrap(),
            None,
            AccessibilityAction::Press
        )
        .unwrap()
        .is_empty()
    );
    view.accessibility_action(SELECT_B, None, AccessibilityAction::Press)
        .unwrap();
    request.respond().await.unwrap();
    assert!(request.respond().await.is_err());
    view.complete_actionable(ids, &intent, Ok(())).unwrap();
    assert_eq!(view.selected(), 1);
    assert_eq!(fixture.response_calls(), 1);
    assert_eq!(fixture.responses()[0]["outcome"]["sessionId"], "a");
    assert!(
        view.binding()
            .current_run()
            .unwrap()
            .draft
            .settings
            .is_empty()
    );
    view.accessibility_action(SELECT_A, None, AccessibilityAction::Press)
        .unwrap();
    assert_eq!(view.binding().current_run().unwrap().draft, before);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn question_response_preserves_the_exact_option_and_waits_for_shared_resolution() {
    let fixture = Fixture::default();
    fixture.publish_actionable(question());
    let (runtime, mut view) = setup(fixture.clone()).await;
    wait_card(&mut view, "option-0").await;
    let (request, ids, intent) = press(&mut view, "option-0");
    request.respond().await.unwrap();
    view.complete_actionable(ids, &intent, Ok(())).unwrap();
    assert_eq!(
        fixture.responses()[0]["outcome"]["answer"]["answers"][0]["selected"],
        json!(["中文"])
    );
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            view.sync().unwrap();
            if view.actionable_choice_id(ids, "option-0").is_none() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn source_revision_reconnect_and_forged_choices_fail_before_rpc() {
    let fixture = Fixture::default();
    fixture.publish_actionable(approval(0));
    let (runtime, mut view) = setup(fixture.clone()).await;
    wait_card(&mut view, "allow").await;
    let (request, _, intent) = press(&mut view, "allow");
    let other = view.bindings()[1].clone();
    assert!(other.begin_actionable_response(&intent).is_err());
    let mut forged_prompt = (*intent.prompt).clone();
    forged_prompt.choices[0].id = "invented".into();
    let mut forged = intent.clone();
    forged.prompt = Arc::new(forged_prompt);
    forged.choice_id = "invented".into();
    assert!(view.binding().begin_actionable_response(&forged).is_err());
    fixture.publish_actionable(approval(1));
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if !request.is_current() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(request.respond().await.is_err());
    assert_eq!(fixture.response_calls(), 0);
    view.sync().unwrap();
    let (_, _, new_intent) = press(&mut view, "allow");
    runtime.reconnect().await.unwrap();
    assert!(
        view.binding()
            .begin_actionable_response(&new_intent)
            .is_err()
    );
    assert_eq!(fixture.response_calls(), 0);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn uncertain_response_is_inline_and_never_retries_or_claims_resolution() {
    let fixture = Fixture::default();
    fixture.publish_actionable(approval(0));
    fixture.set_response_fails(true);
    let (runtime, mut view) = setup(fixture.clone()).await;
    wait_card(&mut view, "deny").await;
    let (request, ids, intent) = press(&mut view, "deny");
    let error = request.respond().await.unwrap_err().to_string();
    view.complete_actionable(ids, &intent, Err(error)).unwrap();
    let id = view.actionable_choice_id(ids, "deny").unwrap();
    assert!(
        view.accessibility_action(id, None, AccessibilityAction::Press)
            .unwrap()
            .is_empty()
    );
    assert_eq!(fixture.response_calls(), 1);
    assert!(
        view.tree
            .semantics()
            .iter()
            .any(|n| n.semantics.name.contains("回答结果未确认"))
    );
    assert_eq!(
        view.binding()
            .current_run()
            .unwrap()
            .run
            .as_ref()
            .unwrap()
            .actionables
            .len(),
        1
    );
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn marked_composer_disables_actionable_mutations() {
    let fixture = Fixture::default();
    fixture.publish_actionable(approval(0));
    let (runtime, mut view) = setup(fixture.clone()).await;
    wait_card(&mut view, "allow").await;
    let id = view.actionable_choice_id(view.ids(), "allow").unwrap();
    view.tree.focus(Some(view.ids().editor), true);
    view.event(&ContentEvent::Preedit {
        text: "组字".into(),
        selection: 2..2,
        clauses: vec![],
        replacement: None,
    })
    .unwrap();
    assert!(view.composing());
    assert!(
        view.accessibility_action(id, None, AccessibilityAction::Press)
            .unwrap()
            .is_empty()
    );
    assert_eq!(fixture.response_calls(), 0);
    runtime.stop().await.unwrap();
}
#[test]
fn public_card_retires_old_ids_on_epoch_and_rejects_old_publications() {
    let prompt =
        Arc::new(ActionablePrompt::read(RunScope::main("fixture", "a"), approval(0)).unwrap());
    let mut card = ActionableCard::new(91, Default::default());
    card.update(1, 1, Some(prompt.clone()), true).unwrap();
    let old = card.choice_id("allow").unwrap();
    card.update(2, 2, Some(prompt.clone()), true).unwrap();
    assert_ne!(card.choice_id("allow"), Some(old));
    assert!(card.update(1, 1, Some(prompt), true).is_err());
}

#[tokio::test]
async fn late_failure_is_retained_in_original_offscreen_card_not_current_session() {
    let fixture = Fixture::default();
    fixture.publish_actionable(approval(0));
    fixture.set_response_fails(true);
    let (runtime, mut view) = setup(fixture).await;
    wait_card(&mut view, "allow").await;
    let (request, ids, intent) = press(&mut view, "allow");
    view.accessibility_action(SELECT_B, None, AccessibilityAction::Press)
        .unwrap();
    view.complete_actionable(
        ids,
        &intent,
        Err(request.respond().await.unwrap_err().to_string()),
    )
    .unwrap();
    assert!(
        view.tree
            .semantics()
            .iter()
            .all(|n| !n.semantics.name.contains("回答结果未确认"))
    );
    view.accessibility_action(SELECT_A, None, AccessibilityAction::Press)
        .unwrap();
    assert!(
        view.tree
            .semantics()
            .iter()
            .any(|n| n.semantics.name.contains("回答结果未确认"))
    );
    runtime.stop().await.unwrap();
}
#[test]
fn unrelated_publications_preserve_button_pointer_tracking() {
    use tekes_ui_controls::composition::Button;
    let prompt =
        Arc::new(ActionablePrompt::read(RunScope::main("fixture", "a"), approval(0)).unwrap());
    let mut card = ActionableCard::new(93, Default::default());
    card.update(1, 1, Some(prompt.clone()), true).unwrap();
    let root = card.id();
    let button = card.choice_id("allow").unwrap();
    let mut host = card.into_host().unwrap();
    host.get_mut(button)
        .unwrap()
        .component_mut::<Button>()
        .unwrap()
        .button
        .state
        .pressed = true;
    assert!(
        !host
            .get_mut(root)
            .unwrap()
            .component_mut::<ActionableCard>()
            .unwrap()
            .update(2, 1, Some(prompt), true)
            .unwrap()
    );
    assert!(
        host.get(button)
            .unwrap()
            .component::<Button>()
            .unwrap()
            .button
            .state
            .pressed
    );
}
