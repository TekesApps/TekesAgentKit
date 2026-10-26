//! Original-source, one-attempt actionable responses; canonical lifetime stays in runtime.
use crate::{PaneAccess, RunBinding};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tekes_agent_components_ui::ActionableResponseIntent;
use tekes_agent_contract::{Result, require, unsupported};
use tekes_agent_presentation::{ActionablePrompt, ComposerDestination};
pub struct ActionableResponseRequest {
    binding: Arc<RunBinding>,
    prompt: Arc<ActionablePrompt>,
    choice_id: String,
    epoch: u64,
    attempted: AtomicBool,
}
impl RunBinding {
    pub fn begin_actionable_response(
        self: &Arc<Self>,
        intent: &ActionableResponseIntent,
    ) -> Result<ActionableResponseRequest> {
        self.require_controls(&ComposerDestination::run(intent.prompt.scope().clone()))?;
        require(
            self.scope() == intent.prompt.scope(),
            "actionable source mismatch",
        )?;
        require(
            intent.prompt.choice(&intent.choice_id).is_some(),
            "actionable response unavailable",
        )?;
        let (epoch, valid) = self.runtime.inspect(|store| {
            (
                store.connection_epoch,
                store.ready()
                    && store.actionables.get(intent.prompt.id()) == Some(intent.prompt.source()),
            )
        });
        require(
            valid && epoch == intent.epoch,
            "actionable changed before capture",
        )?;
        // Reparse canonical source, so caller-mutated display choices cannot
        // invent a response or widen the endpoint's actionable vocabulary.
        let canonical =
            ActionablePrompt::read(self.scope().clone(), intent.prompt.source().clone())?;
        require(
            canonical == *intent.prompt,
            "actionable choices differ from source",
        )?;
        Ok(ActionableResponseRequest {
            binding: self.clone(),
            prompt: intent.prompt.clone(),
            choice_id: intent.choice_id.clone(),
            epoch,
            attempted: AtomicBool::new(false),
        })
    }
}
impl ActionableResponseRequest {
    pub fn prompt(&self) -> &Arc<ActionablePrompt> {
        &self.prompt
    }
    pub fn is_current(&self) -> bool {
        self.binding.access() == PaneAccess::Ready
            && self.binding.runtime.inspect(|store| {
                store.ready()
                    && store.connection_epoch == self.epoch
                    && store.actionables.get(self.prompt.id()) == Some(self.prompt.source())
            })
    }
    pub async fn respond(&self) -> Result<()> {
        require(self.is_current(), "actionable response source changed")?;
        require(
            self.attempted
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok(),
            "actionable response already attempted",
        )?;
        let controller = self
            .binding
            .controller
            .as_ref()
            .ok_or_else(|| unsupported("read-only actionable source"))?;
        controller
            .respond(
                self.prompt.scope(),
                self.prompt.id(),
                self.prompt.revision(),
                self.prompt
                    .choice(&self.choice_id)
                    .expect("captured choice")
                    .outcome()
                    .clone(),
            )
            .await
    }
}
