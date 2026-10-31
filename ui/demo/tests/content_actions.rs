use std::time::Duration;
use tekes_agent_components_ui::{
    AgentText, CANCEL, COMPOSER, EARLIER, Intent, RunSurface, SEND, SESSION_B,
};
use tekes_agent_contract::PromptMode;
use tekes_agent_presentation::{ComposerDestination, RunController, RunScope};
use tekes_ui_controls::composition::{AccessibilityAction as A, ContentEvent as E};
async fn until(c: &RunController, p: impl Fn() -> bool) {
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if p() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let _ = c;
}
#[tokio::test]
async fn queued_send_is_frozen_before_sync_switches_from_a_to_b_and_drains_once() {
    let (runtime, c) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let a = ComposerDestination::run(RunScope::main("fixture", "a"));
    let b = ComposerDestination::run(RunScope::main("fixture", "b"));
    let mut draft = c.snapshot().draft.clone();
    draft.text = "A typed action".into();
    c.edit(&a, draft).unwrap();
    let mut ui = RunSurface::new();
    ui.sync(&c.snapshot()).unwrap();
    ui.tree.accessibility_action(SEND, None, A::Press);
    c.select(b).unwrap();
    ui.sync(&c.snapshot()).unwrap();
    let intents = ui.drain_intents();
    assert_eq!(intents, vec![Intent::Send { destination: a }]);
    assert!(ui.drain_intents().is_empty());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn actual_running_cancel_and_earlier_keep_original_scope_and_event_order_after_switch() {
    let (runtime, c) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    until(&c, || c.snapshot().can_submit).await;
    let a = RunScope::main("fixture", "a");
    let mut draft = c.snapshot().draft.clone();
    draft.text = "running".into();
    c.edit(&ComposerDestination::run(a.clone()), draft).unwrap();
    let ticket = c
        .prepare(&ComposerDestination::run(a.clone()), PromptMode::Queue)
        .unwrap();
    c.dispatch(ticket, None).await.unwrap();
    until(&c, || {
        c.snapshot().run.as_ref().is_some_and(|r| r.can_cancel)
    })
    .await;
    let mut ui = RunSurface::new();
    ui.sync(&c.snapshot()).unwrap();
    assert!(c.snapshot().transcript.has_more_before);
    ui.tree.accessibility_action(EARLIER, None, A::Press);
    ui.tree.accessibility_action(CANCEL, None, A::Press);
    c.select(ComposerDestination::run(RunScope::main("fixture", "b")))
        .unwrap();
    ui.sync(&c.snapshot()).unwrap();
    assert_eq!(
        ui.drain_intents(),
        vec![
            Intent::Earlier { scope: a.clone() },
            Intent::Cancel { scope: a }
        ]
    );
    assert!(ui.drain_intents().is_empty());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn actual_ax_facade_is_immediate_never_polls_counter_and_preserves_ime_barrier() {
    let (runtime, c) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let a = ComposerDestination::run(RunScope::main("fixture", "a"));
    let mut draft = c.snapshot().draft.clone();
    draft.text = "A".into();
    c.edit(&a, draft).unwrap();
    let mut ui = RunSurface::new();
    ui.sync(&c.snapshot()).unwrap();
    ui.tree
        .get_mut(SEND)
        .unwrap()
        .component_mut::<tekes_ui_controls::composition::Button>()
        .unwrap()
        .activations = 1000;
    assert!(ui.drain_intents().is_empty());
    assert_eq!(
        ui.accessibility_action(SEND, None, A::Press),
        vec![Intent::Send { destination: a }]
    );
    assert!(ui.drain_intents().is_empty());
    ui.tree.focus(Some(COMPOSER), true);
    ui.event(&E::Preedit {
        text: "preedit".into(),
        selection: 7..7,
        clauses: vec![],
        replacement: None,
    });
    let before = ui.editor().input.text.clone();
    assert!(ui.accessibility_action(SEND, None, A::Press).is_empty());
    assert!(
        ui.accessibility_action(SESSION_B, None, A::Press)
            .is_empty()
    );
    assert!(ui.composing());
    assert_eq!(ui.editor().input.text, before);
    assert_eq!(ui.tree.focused(), Some(COMPOSER));
    assert!(
        ui.tree
            .get(COMPOSER)
            .unwrap()
            .component::<AgentText>()
            .unwrap()
            .field
            .surface
            .input
            .marked
            .is_some()
    );
    runtime.stop().await.unwrap();
}
