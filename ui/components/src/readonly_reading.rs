//! Read-only Agent publication over the original SDK reading transaction.
use crate::{ReadOnlyTranscript, ReadOnlyTranscriptUpdate};
use tekes_ui_controls::{
    ReadingDocumentVersion, ReadingUpdateError,
    composition::{ComponentId, ContentHost},
};
#[derive(Debug, PartialEq, Eq)]
pub enum ReadOnlyReadingError {
    Agent(&'static str),
    Reading(ReadingUpdateError),
}
/// Preflight the complete pane before advancing the original SDK document.
/// Status formatting already ran while preparing the plan, outside this callback.
/// Display revision is independent of provider publication/read order.
pub fn publish_readonly_reading_update(
    host: &mut ContentHost,
    root: ComponentId,
    version: &ReadingDocumentVersion,
    revision: u64,
    plan: &ReadOnlyTranscriptUpdate<'_>,
) -> Result<ReadingDocumentVersion, ReadOnlyReadingError> {
    let pane = host
        .get(root)
        .and_then(|n| n.component::<ReadOnlyTranscript>())
        .ok_or(ReadOnlyReadingError::Agent(
            "read-only reading owner is not mounted",
        ))?;
    pane.validate_prepared_update(plan)
        .map_err(ReadOnlyReadingError::Agent)?;
    let viewport = pane.ids().viewport;
    host.update_scroll_reading_document(viewport, version, revision, plan.item_ids(), |host| {
        host.get_mut(root)
            .unwrap()
            .component_mut::<ReadOnlyTranscript>()
            .unwrap()
            .apply_prepared_update(plan)
            .expect("complete same-turn read-only preflight; no intervening event");
    })
    .map(|(version, ())| version)
    .map_err(ReadOnlyReadingError::Reading)
}
