use tekes_agent_components_ui::{AgentText, COMPOSER, Intent, RunSurface, SEND};
use tekes_agent_contract::PromptMode;
use tekes_agent_presentation::{ComposerDestination, RunController, RunScope};
use tekes_ui_controls::{ControlKey, composition::ContentEvent as E};
fn input(ui: &mut RunSurface) -> &mut tekes_ui_controls::TextInput {
    &mut ui
        .tree
        .get_mut(COMPOSER)
        .unwrap()
        .component_mut::<AgentText>()
        .unwrap()
        .field
        .surface
        .input
}
fn edit(ui: &mut RunSurface, c: &RunController, event: E) -> Vec<Intent> {
    let intents = ui.event(&event);
    for i in &intents {
        if let Intent::Edit { destination, text } = i {
            let mut d = c.snapshot().draft.clone();
            d.text = text.clone();
            c.edit(destination, d).unwrap();
        }
    }
    intents
}
fn send(ui: &mut RunSurface) {
    ui.tree.focus(Some(SEND), true);
    assert!(
        ui.event(&E::Key {
            key: ControlKey::Enter,
            down: true
        })
        .iter()
        .any(|i| matches!(i, Intent::Send { .. }))
    );
}
#[tokio::test]
async fn canonical_publication_versions_reject_a_late_snapshot_for_the_same_document() {
    let (runtime, c) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut ui = RunSurface::new();
    let old = c.snapshot();
    ui.sync(&old).unwrap();
    let destination = old.selection.as_ref().unwrap().destination.clone();
    let mut d = old.draft.clone();
    d.text = "new authority".into();
    c.edit(&destination, d).unwrap();
    let next = c.snapshot();
    assert!(next.revision > old.revision);
    ui.sync(&next).unwrap();
    assert!(ui.sync(&old).is_err());
    assert_eq!(ui.editor().input.text, "new authority");
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn source_refresh_keeps_selection_and_undo_and_confirmed_clear_is_a_transaction() {
    let (runtime, c) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut ui = RunSurface::new();
    ui.sync(&c.snapshot()).unwrap();
    ui.tree.focus(Some(COMPOSER), true);
    edit(
        &mut ui,
        &c,
        E::Commit {
            text: "中文😀".into(),
            replacement: None,
        },
    );
    ui.sync(&c.snapshot()).unwrap();
    input(&mut ui).select(2, false);
    let baseline = ui.editor().input.external_baseline();
    ui.sync(&c.snapshot()).unwrap();
    assert_eq!(ui.editor().input.external_baseline(), baseline);
    send(&mut ui);
    let destination = c.snapshot().selection.as_ref().unwrap().destination.clone();
    let ticket = c.prepare(&destination, PromptMode::Queue).unwrap();
    let clear = c.snapshot();
    ui.sync(&clear).unwrap();
    assert!(ui.editor().input.text.is_empty());
    assert!(input(&mut ui).undo());
    assert_eq!(ui.editor().input.text, "中文😀");
    assert_eq!(ui.editor().input.selection(), 2..2);
    ui.sync(&clear).unwrap();
    assert_eq!(ui.editor().input.text, "中文😀");
    c.cancel_prepared(ticket).unwrap();
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn an_old_clear_receipt_cannot_erase_local_input_typed_after_preparation() {
    let (runtime, c) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut ui = RunSurface::new();
    ui.sync(&c.snapshot()).unwrap();
    ui.tree.focus(Some(COMPOSER), true);
    edit(
        &mut ui,
        &c,
        E::Commit {
            text: "first".into(),
            replacement: None,
        },
    );
    ui.sync(&c.snapshot()).unwrap();
    send(&mut ui);
    let destination = c.snapshot().selection.as_ref().unwrap().destination.clone();
    let ticket = c.prepare(&destination, PromptMode::Queue).unwrap();
    let clear = c.snapshot();
    // Deliberately delay the clear delivery while the original editor receives input.
    ui.tree.focus(Some(COMPOSER), true);
    ui.event(&E::Commit {
        text: " later".into(),
        replacement: None,
    });
    let local = ui.editor().input.text.clone();
    assert!(ui.sync(&clear).is_err());
    assert_eq!(ui.editor().input.text, local);
    c.cancel_prepared(ticket).unwrap();
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn a_b_a_uses_retained_editors_and_rejects_old_a_backfill_after_return() {
    let (runtime, c) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut ui = RunSurface::new();
    ui.sync(&c.snapshot()).unwrap();
    ui.tree.focus(Some(COMPOSER), true);
    edit(
        &mut ui,
        &c,
        E::Commit {
            text: "A draft".into(),
            replacement: None,
        },
    );
    let old_a = c.snapshot();
    ui.sync(&old_a).unwrap();
    c.select(ComposerDestination::run(RunScope::main("fixture", "b")))
        .unwrap();
    ui.sync(&c.snapshot()).unwrap();
    ui.tree.focus(Some(COMPOSER), true);
    edit(
        &mut ui,
        &c,
        E::Commit {
            text: "B draft".into(),
            replacement: None,
        },
    );
    ui.sync(&c.snapshot()).unwrap();
    assert!(ui.sync(&old_a).is_err());
    assert_eq!(ui.editor().input.text, "B draft");
    c.select(old_a.selection.as_ref().unwrap().destination.clone())
        .unwrap();
    ui.sync(&c.snapshot()).unwrap();
    assert_eq!(ui.editor().input.text, "A draft");
    assert_eq!(
        ui.editor().input.external_value().unwrap().version,
        c.snapshot().revision
    );
    assert!(ui.sync(&old_a).is_err());
    assert!(input(&mut ui).undo());
    assert!(ui.editor().input.text.is_empty());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn unchanged_authority_during_preedit_does_not_hide_the_unmark_edit_intent() {
    let (runtime, c) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut ui = RunSurface::new();
    ui.sync(&c.snapshot()).unwrap();
    ui.tree.focus(Some(COMPOSER), true);
    assert!(
        ui.event(&E::Preedit {
            text: "ni".into(),
            selection: 2..2,
            clauses: std::iter::once(0..2).collect(),
            replacement: None
        })
        .is_empty()
    );
    ui.sync(&c.snapshot()).unwrap();
    let intents = edit(&mut ui, &c, E::Unmark);
    assert!(
        intents
            .iter()
            .any(|i| matches!(i,Intent::Edit{text,..} if text=="ni"))
    );
    ui.sync(&c.snapshot()).unwrap();
    assert_eq!(c.snapshot().draft.text, "ni");
    runtime.stop().await.unwrap();
}
