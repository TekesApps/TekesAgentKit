//! Explicit mutating proof for isolated fixture sessions, not rating UI.
use crate::component_demo::ComponentDemo;
use serde_json::{Value, json};
use std::sync::Arc;
use tekes_agent_contract::{Result, SessionMessageFeedbackRating as Rating, require, unsupported};
use tekes_agent_presentation::{TranscriptDocument, TurnFeedbackTarget};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{
    FeedbackChange as Change, FeedbackFrame, FeedbackOutcome, FeedbackPreparation,
    FeedbackTargetCapture, RunBinding, SessionMessageFeedback,
};

async fn change(
    source: &Arc<RunBinding>,
    frame: &FeedbackFrame,
    target: &FeedbackTargetCapture,
    change: Change,
) -> Result<FeedbackFrame> {
    match source.prepare_feedback(frame, target, change)? {
        FeedbackPreparation::Unchanged(frame) => Ok(frame),
        FeedbackPreparation::Request(request) => match request.execute().await {
            FeedbackOutcome::Applied(frame) => Ok(frame),
            other => Err(unsupported(&format!(
                "feedback proof expected applied: {other:?}"
            ))),
        },
    }
}
pub async fn run(view: &ComponentDemo, runtime: &Arc<SessionRuntime>) -> Result<Value> {
    let source = view.binding().clone();
    let before = source
        .current_run()
        .ok_or_else(|| unsupported("feedback run unavailable"))?;
    let other_draft = view.bindings()[1].current_run().unwrap().draft.clone();
    let document = TranscriptDocument::read(&before.transcript)?;
    let target = document
        .sections
        .iter()
        .rev()
        .find_map(TurnFeedbackTarget::read)
        .ok_or_else(|| unsupported("no proven final feedback target"))?
        .capture(source.scope())?;
    let initial = source.read_feedback().await?;
    require(
        initial.records().get(target.message_id()).is_none(),
        "mutating proof requires an unrated isolated target",
    )?;
    let FeedbackPreparation::Request(request) =
        source.prepare_feedback(&initial, &target, Change::ToggleRating(Rating::Positive))?
    else {
        return Err(unsupported("feedback create request unavailable"));
    };
    require(
        !request.rpc_id().is_empty(),
        "feedback RPC identity missing",
    )?;
    let external_rpc = format!("feedback-proof-other:{}", request.rpc_id());
    require(
        source
            .prepare_feedback(&initial, &target, Change::ToggleRating(Rating::Positive))
            .is_err(),
        "duplicate frame admission",
    )?;
    let FeedbackOutcome::Applied(created) = request.execute().await else {
        return Err(unsupported("feedback create did not apply"));
    };
    let version = created
        .records()
        .get(target.message_id())
        .unwrap()
        .version
        .clone();
    // Simulate an independent writer through the public endpoint, without using
    // this binding's publication lane. The stale CAS must not overwrite it.
    let external: SessionMessageFeedback = serde_json::from_value(
        runtime
            .call(
                "feedback.put",
                json!({
        "sessionId":source.scope().session_id,"messageId":target.message_id(),"rating":"negative",
        "ifVersion":version,"note":"external CAS note"}),
                &external_rpc,
            )
            .await?,
    )?;
    external.validate()?;
    let FeedbackPreparation::Request(request) =
        source.prepare_feedback(&created, &target, Change::SetRating(Some(Rating::Positive)))?
    else {
        return Err(unsupported("stale CAS request unavailable"));
    };
    let FeedbackOutcome::Conflict(current) = request.execute().await else {
        return Err(unsupported("stale feedback CAS did not expose current"));
    };
    require(
        current.records().get(target.message_id()) == Some(&external),
        "conflict lost authoritative current",
    )?;
    let noted = change(
        &source,
        &current,
        &target,
        Change::SetNote(Some("AgentKit 中文备注".into())),
    )
    .await?;
    let preserved = change(
        &source,
        &noted,
        &target,
        Change::SetRating(Some(Rating::Positive)),
    )
    .await?;
    require(
        preserved
            .records()
            .get(target.message_id())
            .unwrap()
            .note
            .as_deref()
            == Some("AgentKit 中文备注"),
        "rating set lost note",
    )?;
    require(
        preserved
            .records()
            .get(target.message_id())
            .unwrap()
            .updated_at_milliseconds
            .is_some(),
        "backend feedback timestamp missing",
    )?;
    // A same-value set is valid even if the backend retains its opaque version.
    let preserved = change(
        &source,
        &preserved,
        &target,
        Change::SetRating(Some(Rating::Positive)),
    )
    .await?;
    let replaced = change(
        &source,
        &preserved,
        &target,
        Change::ToggleRating(Rating::Negative),
    )
    .await?;
    require(
        replaced
            .records()
            .get(target.message_id())
            .unwrap()
            .note
            .is_none(),
        "new judgment did not clear old note",
    )?;
    let deleted = change(
        &source,
        &replaced,
        &target,
        Change::ToggleRating(Rating::Negative),
    )
    .await?;
    require(
        deleted.records().get(target.message_id()).is_none(),
        "same rating toggle did not delete",
    )?;
    let restored = source.read_feedback().await?;
    require(
        restored.is_current(&source) && restored.records().get(target.message_id()).is_none(),
        "fresh feedback read did not confirm deletion",
    )?;
    require(
        source.current_run().unwrap().draft == before.draft
            && view.bindings()[1].current_run().unwrap().draft == other_draft,
        "feedback changed a composer draft",
    )?;
    Ok(
        json!({"mutated":true,"ratingControls":false,"nativeInteraction":false,"originalScope":true,
        "duplicateFrameRejected":true,"casConflictCurrent":true,"noteSave":true,"setRatingPreservesNote":true,
        "newJudgmentClearsNote":true,"sameRatingDeletes":true,"sameValueSet":true,"serverTimestamp":true,
        "opaqueVersions":true,"freshReadAfterDelete":true,"draftsPreserved":true}),
    )
}
