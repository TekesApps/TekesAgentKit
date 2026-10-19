use std::{sync::Arc, time::Duration};
use tekes_agent_components_ui::{COMPOSER, Intent, RunSurface};
use tekes_agent_contract::*;
use tekes_agent_presentation::*;
use tekes_ui_controls::{ControlKey, composition::ContentEvent};
use tekes_ui_scene::Point;
async fn until(c: &RunController, p: impl Fn(&RunSnapshot) -> bool) {
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if p(&c.snapshot()) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
fn edit(surface: &mut RunSurface, c: &RunController, event: ContentEvent) -> Vec<Intent> {
    let intents = surface.event(&event);
    for intent in &intents {
        if let Intent::Edit { destination, text } = intent {
            let mut draft = c.snapshot().draft.clone();
            draft.text = text.clone();
            c.edit(destination, draft).unwrap();
        }
    }
    intents
}
#[tokio::test]
async fn real_presentation_stream_cancel_reconnect_and_frozen_page() {
    let (runtime, c) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    until(&c, |s| s.can_submit).await;
    let original = c.snapshot().transcript.items.len();
    assert!(c.snapshot().transcript.has_more_before);
    c.load_earlier(&RunScope::main("fixture", "a"))
        .await
        .unwrap();
    until(&c, |s| s.transcript.items.len() > original).await;
    let mut draft = c.snapshot().draft.clone();
    draft.text = "确认流式回传".into();
    let destination = ComposerDestination::run(RunScope::main("fixture", "a"));
    c.edit(&destination, draft).unwrap();
    let ticket = c.prepare(&destination, PromptMode::Queue).unwrap();
    c.dispatch(ticket, None).await.unwrap();
    until(&c, |s| {
        s.transcript
            .items
            .iter()
            .any(|i| i.is_provisional && !i.text.is_empty())
    })
    .await;
    c.cancel(&RunScope::main("fixture", "a")).await.unwrap();
    until(&c, |s| {
        s.run.as_ref().is_some_and(|r| !r.can_cancel)
            && !s.transcript.items.iter().any(|i| i.is_provisional)
    })
    .await;
    assert_eq!(
        c.snapshot()
            .transcript
            .items
            .iter()
            .filter(|i| i.text == "确认流式回传")
            .count(),
        1,
        "echo must reconcile exactly once"
    );
    let before = c
        .snapshot()
        .transcript
        .items
        .iter()
        .rfind(|i| i.kind == TranscriptItemKind::Assistant)
        .unwrap()
        .text
        .clone();
    let epoch = runtime.snapshot().connection_epoch;
    runtime.reconnect().await.unwrap();
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    until(&c, |s| {
        runtime.snapshot().connection_epoch > epoch
            && !s.transcript.is_stale
            && s.transcript.items.iter().any(|i| i.text == before)
    })
    .await;
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn ime_is_not_a_submission_and_editor_instances_keep_destination_undo() {
    let (runtime, c) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    until(&c, |s| s.can_submit).await;
    let mut ui = RunSurface::new();
    ui.sync(&c.snapshot()).unwrap();
    assert!(ui.tree.focus(Some(COMPOSER), true));
    let intents = edit(
        &mut ui,
        &c,
        ContentEvent::Preedit {
            text: "zhongwen".into(),
            selection: 8..8,
            clauses: std::iter::once(0..8).collect(),
            replacement: None,
        },
    );
    assert!(intents.is_empty());
    assert!(c.snapshot().draft.text.is_empty());
    ui.sync(&c.snapshot()).unwrap();
    assert_eq!(ui.editor().input.text, "zhongwen");
    assert!(
        ui.event(&ContentEvent::Key {
            key: ControlKey::Enter,
            down: true
        })
        .is_empty()
    );
    assert!(
        ui.event(&ContentEvent::PointerDown(Point::new(-1., -1.)))
            .is_empty()
    );
    assert!(ui.composing());
    let intents = edit(
        &mut ui,
        &c,
        ContentEvent::Commit {
            text: "中文草稿 A".into(),
            replacement: None,
        },
    );
    assert!(!intents.iter().any(|i| matches!(i, Intent::Send { .. })));
    ui.sync(&c.snapshot()).unwrap();
    assert_eq!(c.snapshot().draft.text, "中文草稿 A");
    c.select(ComposerDestination::run(RunScope::main("fixture", "b")))
        .unwrap();
    ui.sync(&c.snapshot()).unwrap();
    ui.tree.focus(Some(COMPOSER), true);
    edit(
        &mut ui,
        &c,
        ContentEvent::Commit {
            text: "草稿 B".into(),
            replacement: None,
        },
    );
    ui.sync(&c.snapshot()).unwrap();
    c.select(ComposerDestination::run(RunScope::main("fixture", "a")))
        .unwrap();
    ui.sync(&c.snapshot()).unwrap();
    ui.tree.focus(Some(COMPOSER), true);
    assert_eq!(ui.editor().input.text, "中文草稿 A");
    edit(&mut ui, &c, ContentEvent::TextCommand("undo:".into()));
    assert!(
        ui.editor().input.text.is_empty(),
        "retained native undo must restore the original empty draft"
    );
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn submit_completion_never_clears_a_different_session_draft() {
    let (runtime, c) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    until(&c, |s| s.can_submit).await;
    let a = ComposerDestination::run(RunScope::main("fixture", "a"));
    let b = ComposerDestination::run(RunScope::main("fixture", "b"));
    c.edit(
        &a,
        ComposerDraft {
            text: "发送到 A".into(),
            ..Default::default()
        },
    )
    .unwrap();
    let ticket = c.prepare(&a, PromptMode::Queue).unwrap();
    c.select(b.clone()).unwrap();
    c.edit(
        &b,
        ComposerDraft {
            text: "仍属于 B".into(),
            ..Default::default()
        },
    )
    .unwrap();
    let worker: Arc<RunController> = c.clone();
    worker.dispatch(ticket, None).await.unwrap();
    assert_eq!(c.snapshot().draft.text, "仍属于 B");
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn ui_send_callback_carries_the_presented_destination() {
    use tekes_agent_components_ui::SEND;
    use tekes_ui_controls::composition::AccessibilityAction;
    let (runtime, c) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    until(&c, |s| s.can_submit).await;
    let a = ComposerDestination::run(RunScope::main("fixture", "a"));
    c.edit(
        &a,
        ComposerDraft {
            text: "A 的回调".into(),
            ..Default::default()
        },
    )
    .unwrap();
    let mut ui = RunSurface::new();
    ui.sync(&c.snapshot()).unwrap();
    let callback = ui
        .accessibility_action(SEND, None, AccessibilityAction::Press)
        .into_iter()
        .find_map(|i| match i {
            Intent::Send { destination } => Some(destination),
            _ => None,
        })
        .unwrap();
    c.select(ComposerDestination::run(RunScope::main("fixture", "b")))
        .unwrap();
    assert_eq!(
        callback, a,
        "host selection changes cannot retarget the emitted action"
    );
    runtime.stop().await.unwrap();
}
