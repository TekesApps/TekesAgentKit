//! Opt-in real transport proof through public control actions; not native UI acceptance.
use std::sync::Arc;
use tekes_agent_components_ui::{FeedbackGesture, TurnFeedback};
use tekes_agent_contract::{Result, require, unsupported};
use tekes_agent_presentation::{TranscriptDocument, TurnFeedbackTarget};
use tekes_agent_ui_bindings::{FeedbackFrame, FeedbackUiAdoption, RunBinding};
use tekes_ui_controls::composition::*;

pub fn control(host: &mut ContentHost) -> &mut TurnFeedback {
    host.get_mut(ComponentId(191_u64 << 32))
        .unwrap()
        .component_mut::<TurnFeedback>()
        .unwrap()
}
fn activate(host: &mut ContentHost, id: ComponentId) -> Option<FeedbackGesture> {
    host.accessibility_action(id, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    control(host).take_intents(&actions).pop()
}
async fn change(
    source: &Arc<RunBinding>,
    host: &mut ContentHost,
    frame: FeedbackFrame,
    id: ComponentId,
) -> Result<FeedbackFrame> {
    let gesture = activate(host, id).ok_or_else(|| unsupported("feedback gesture unavailable"))?;
    match source
        .prepare_feedback_gesture(Some(&frame), gesture)
        .execute()
        .await
        .apply(control(host))
    {
        FeedbackUiAdoption::Adopted(Some(frame)) => Ok(frame),
        _ => Err(unsupported("feedback control result was not adopted")),
    }
}
pub async fn start(source: &Arc<RunBinding>) -> Result<(ContentHost, FeedbackFrame)> {
    let before = source
        .current_run()
        .ok_or_else(|| unsupported("run unavailable"))?;
    let document = TranscriptDocument::read(&before.transcript)?;
    let target = document
        .sections
        .iter()
        .rev()
        .find_map(TurnFeedbackTarget::read)
        .ok_or_else(|| unsupported("feedback target unavailable"))?
        .capture(source.scope())?;
    let frame = source.read_feedback().await?;
    require(
        frame.records().get(target.message_id()).is_none(),
        "feedback controls proof requires isolated unrated target",
    )?;
    let mut ui = TurnFeedback::new(191, Default::default());
    ui.update(frame.controls(source, &target)?)
        .map_err(unsupported)?;
    let positive = ui.ids().positive;
    let mut host = ui.into_host().unwrap();
    let mut frame = change(source, &mut host, frame, positive).await?;
    for note in [
        "\n\u{feff} 中文评价备注 \u{feff}\n",
        " \n\u{feff}",
        "中文评价备注",
    ] {
        let id = control(&mut host).ids().edit_note;
        require(
            activate(&mut host, id).is_none(),
            "open note unexpectedly dispatched",
        )?;
        require(control(&mut host).is_open(), "note editor did not open")?;
        let editor = control(&mut host).ids().editor;
        host.focus(Some(editor), true);
        let length = control(&mut host)
            .editor()
            .unwrap()
            .input
            .text
            .encode_utf16()
            .count();
        host.accessibility_action(editor, None, AccessibilityAction::SetSelection(0..length));
        host.drain_actions();
        host.event(&ContentEvent::Commit {
            text: note.into(),
            replacement: None,
        });
        let actions = host.drain_actions();
        require(
            control(&mut host).take_intents(&actions).is_empty(),
            "editing automatically saved",
        )?;
        let save = control(&mut host).ids().save;
        frame = change(source, &mut host, frame, save).await?;
        let expected = note.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
        require(
            frame
                .records()
                .get(target.message_id())
                .unwrap()
                .note
                .as_deref()
                == (!expected.is_empty()).then_some(expected),
            "note was not normalized or committed",
        )?;
    }
    let note = control(&mut host).ids().edit_note;
    activate(&mut host, note);
    require(
        control(&mut host).editor().unwrap().input.text == "中文评价备注",
        "committed note did not reopen",
    )?;
    require(
        source.current_run().unwrap().draft == before.draft,
        "feedback changed Composer draft",
    )?;
    Ok((host, frame))
}
pub async fn finish(
    source: &Arc<RunBinding>,
    host: &mut ContentHost,
    frame: FeedbackFrame,
) -> Result<serde_json::Value> {
    require(
        control(host)
            .editor()
            .is_some_and(|editor| editor.input.text == "中文评价备注"),
        "theme render changed the note editor",
    )?;
    let close = control(host).ids().close;
    activate(host, close);
    require(!control(host).is_open(), "note did not close")?;
    let positive = control(host).ids().positive;
    let frame = change(source, host, frame, positive).await?;
    let id = control(host)
        .state()
        .unwrap()
        .target
        .message_id()
        .to_string();
    require(
        frame.records().get(&id).is_none()
            && source.read_feedback().await?.records().get(&id).is_none(),
        "feedback was not deleted",
    )?;
    Ok(
        serde_json::json!({"publicControls":true,"semanticActions":true,"committedNoteReopened":true,"trimmedChineseNote":true,"blankClearsNote":true,"sameRatingDeletes":true,"freshReadAfterDelete":true,"draftPreserved":true,"lightDarkRendered":true,"nativeInteraction":false,"physicalIme":false,"embeddedTranscript":false}),
    )
}
