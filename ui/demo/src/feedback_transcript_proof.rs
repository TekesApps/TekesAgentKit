//! Opt-in proof through actual Transcript/RunPort routing and original-binding callbacks.
use crate::component_demo::{ComponentDemo, DemoCommand};
use tekes_agent_components_ui::{TranscriptContent, TurnFeedback};
use tekes_agent_contract::{Result, require, unsupported};
use tekes_ui_controls::composition::*;
fn control(view: &ComponentDemo, id: ComponentId) -> &TurnFeedback {
    view.tree
        .get(id)
        .unwrap()
        .component::<TurnFeedback>()
        .unwrap()
}
fn press(view: &mut ComponentDemo, id: ComponentId) -> Result<Vec<DemoCommand>> {
    view.accessibility_action(id, None, AccessibilityAction::Press)
}
async fn execute(view: &mut ComponentDemo, commands: Vec<DemoCommand>) -> Result<()> {
    let mut commands = commands.into_iter();
    let Some(DemoCommand::Feedback(request)) = commands.next() else {
        return Err(unsupported("expected a mounted feedback request"));
    };
    require(commands.next().is_none(), "unexpected extra command")?;
    view.complete_feedback(request.execute().await)
}
pub async fn run(view: &mut ComponentDemo) -> Result<serde_json::Value> {
    let source = view.binding().clone();
    let before = source.current_run().unwrap().draft.clone();
    let other = view.bindings()[1].current_run().unwrap().draft.clone();
    let root = view
        .transcript_content_id(view.ids())
        .ok_or_else(|| unsupported("Transcript is not installed"))?;
    let id = view
        .tree
        .get(root)
        .unwrap()
        .component::<TranscriptContent>()
        .unwrap()
        .feedback_controls()
        .last()
        .map(|(id, _)| id)
        .ok_or_else(|| unsupported("no proven final response control"))?;
    let message = control(view, id).target().unwrap().message_id().to_string();
    require(
        source
            .read_feedback()
            .await?
            .records()
            .get(&message)
            .is_none(),
        "proof requires an isolated unrated response",
    )?;
    let refresh = control(view, id).ids().refresh;
    let commands = press(view, refresh)?;
    execute(view, commands).await?;
    let positive = control(view, id).ids().positive;
    let commands = press(view, positive)?;
    execute(view, commands).await?;
    let note = control(view, id).ids().edit_note;
    require(
        press(view, note)?.is_empty(),
        "opening editor sent a command",
    )?;
    let editor = control(view, id).ids().editor;
    view.tree.focus(Some(editor), true);
    require(
        view.event(&ContentEvent::Commit {
            text: " \u{feff}Transcript 中文备注 \n".into(),
            replacement: None,
        })?
        .is_empty(),
        "typing sent a command",
    )?;
    let save = control(view, id).ids().save;
    let commands = press(view, save)?;
    execute(view, commands).await?;
    require(
        control(view, id)
            .state()
            .unwrap()
            .current
            .as_ref()
            .unwrap()
            .note
            .as_deref()
            == Some("Transcript 中文备注"),
        "mounted note was not committed",
    )?;
    let note = control(view, id).ids().edit_note;
    press(view, note)?;
    require(
        control(view, id).editor().unwrap().input.text == "Transcript 中文备注",
        "mounted editor did not recover note",
    )?;
    let close = control(view, id).ids().close;
    press(view, close)?;
    let positive = control(view, id).ids().positive;
    let commands = press(view, positive)?;
    execute(view, commands).await?;
    require(
        source
            .read_feedback()
            .await?
            .records()
            .get(&message)
            .is_none(),
        "mounted toggle did not delete feedback",
    )?;
    require(
        source.current_run().unwrap().draft == before
            && view.bindings()[1].current_run().unwrap().draft == other,
        "feedback changed a Composer draft",
    )?;
    Ok(
        serde_json::json!({"embeddedTranscript":true,"runPortRouting":true,"originalTarget":true,"coldRead":true,"savedChineseNote":true,"noteReopened":true,"sameRatingDeletes":true,"freshReadAfterDelete":true,"draftsPreserved":true,"nativeInteraction":false,"readingAnchorAcceptance":false}),
    )
}
