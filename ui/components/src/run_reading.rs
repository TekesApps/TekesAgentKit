//! Paired Agent publication inside the original library reading transaction.
use crate::{RunPort, RunPortUpdate};
use tekes_ui_controls::{
    ExternalValueConflict, ExternalValueResult, ReadingDocumentVersion, ReadingUpdateError,
    composition::{ComponentId, ContentHost},
};

#[derive(Debug, PartialEq, Eq)]
pub enum RunReadingError {
    Agent(&'static str),
    Editor(ExternalValueConflict),
    Reading(ReadingUpdateError),
}

/// Publish one prepared Run frame. `version` must be the original host token;
/// `revision` is a strictly increasing display revision, not a provider clock
/// (availability can change without a new provider publication).
///
/// Validates the Agent plan and controlled editor write before the SDK can
/// advance its document. No event, yield or application callback intervenes.
/// Stale/foreign tokens leave both Transcript and Composer untouched. Geometry,
/// reading intent, fallback and compensation belong entirely to TekesUI.
pub fn publish_run_reading_update(
    host: &mut ContentHost,
    root: ComponentId,
    version: &ReadingDocumentVersion,
    revision: u64,
    plan: &RunPortUpdate<'_>,
) -> Result<ReadingDocumentVersion, RunReadingError> {
    let run = host
        .get(root)
        .and_then(|node| node.component::<RunPort>())
        .ok_or(RunReadingError::Agent(
            "RunPort reading owner is not mounted",
        ))?;
    run.validate_prepared_update(plan)
        .map_err(RunReadingError::Agent)?;
    if let Some(request) = plan.controlled_draft()
        && let ExternalValueResult::Conflict(conflict) =
            run.composer().editor().check_external_value(
                request.expected(),
                request.value(),
                request.selection().clone(),
                request.undo(),
            )
    {
        return Err(RunReadingError::Editor(conflict));
    }
    let viewport = run.ids().viewport;
    host.update_scroll_reading_document(viewport, version, revision, plan.item_ids(), |host| {
        host.get_mut(root)
            .unwrap()
            .component_mut::<RunPort>()
            .unwrap()
            .apply_prepared_update(plan)
            .expect("same-turn complete preflight; no event or mutation intervenes");
    })
    .map(|(version, ())| version)
    .map_err(RunReadingError::Reading)
}
