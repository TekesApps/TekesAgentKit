//! The ClientKit-style original-input host hook; no history mutation or new RPC.
use crate::{PaneAccess, RunBinding};
use tekes_agent_components_ui::{HistoricalInputIntent, TranscriptMessageIdentity};
use tekes_agent_contract::{Result, require, unsupported};
use tekes_agent_presentation::{
    ComposerDestination, TranscriptDelivery, TranscriptDocument, TranscriptSnapshot,
};
use tekes_agent_session::ConnectionState;

impl RunBinding {
    /// Actual public endpoint capability, constrained to this mounted Run.
    /// Handler registration and native IME/pane barriers remain host/UI inputs.
    pub fn historical_input_available(&self) -> bool {
        self.access() == PaneAccess::Ready
            && self.controller.is_some()
            && self.runtime.inspect(|state| {
                state.connection == ConnectionState::Ready
                    && state.endpoint_capabilities.can_edit_historical_input
            })
    }
    /// Synchronous callback admission against the original binding. The callback
    /// opens the host's editing flow; it is not an endpoint dispatch or draft
    /// replacement. Any later mutation needs its own public contract/validation.
    pub fn dispatch_historical_input<T>(
        &self,
        intent: &HistoricalInputIntent,
        callback: impl FnOnce(&HistoricalInputIntent) -> T,
    ) -> Result<T> {
        require(intent.claim(), "historical input gesture already claimed")?;
        require(
            intent.scope() == &self.scope,
            "historical input scope mismatch",
        )?;
        self.require_controls(&ComposerDestination::run(self.scope.clone()))?;
        let run = self
            .current_run()
            .ok_or_else(|| unsupported("historical input Run missing"))?;
        require(
            run.selection.as_ref().is_some_and(|selection| {
                selection.destination == ComposerDestination::run(self.scope.clone())
            }),
            "historical input selection changed",
        )?;
        require(
            run.connection_epoch == intent.epoch()
                && run.revision >= intent.publication()
                && !run.transcript.is_stale,
            "historical input publication expired",
        )?;
        let state = self.runtime.snapshot();
        require(
            state.connection == ConnectionState::Ready
                && state.connection_epoch == intent.epoch()
                && state.endpoint_capabilities.can_edit_historical_input,
            "historical input unavailable or epoch expired",
        )?;
        let session = state
            .sessions
            .get(&self.scope.session_id)
            .ok_or_else(|| unsupported("historical input session missing"))?;
        let canonical = TranscriptSnapshot::read(&self.scope, session)?;
        require(!canonical.is_stale, "historical input history is stale")?;
        let document = TranscriptDocument::read(&canonical)?;
        let original = document
            .sections
            .iter()
            .flat_map(|section| &section.inputs)
            .find(|input| TranscriptMessageIdentity::from(input.key) == *intent.message())
            .ok_or_else(|| unsupported("historical input source missing"))?;
        require(
            original.item == intent.input()
                && matches!(original.footnote, TranscriptDelivery::Recorded { .. })
                && !original.item.is_provisional
                && !original.item.turn_id.is_empty(),
            "historical input source changed",
        )?;
        Ok(callback(intent))
    }
}
