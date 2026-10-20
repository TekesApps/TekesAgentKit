//! Read-only committed feedback publications over an original RunBinding.
use crate::{PaneAccess, RunBinding};
use std::sync::{Arc, Weak, atomic::Ordering};
use tekes_agent_contract::{Result, require, unsupported};
pub use tekes_agent_contract::{
    SessionMessageFeedback, SessionMessageFeedbackList, SessionMessageFeedbackRating,
};
use tekes_agent_presentation::RunScope;
pub use tekes_agent_presentation::TurnFeedbackTarget;
use tekes_agent_session::ConnectionState;

#[derive(Clone, Debug)]
pub struct FeedbackFrame {
    source: Weak<RunBinding>,
    scope: RunScope,
    publication: u64,
    connection_epoch: u64,
    records: SessionMessageFeedbackList,
    put_advertised: bool,
    delete_advertised: bool,
}
impl FeedbackFrame {
    pub fn scope(&self) -> &RunScope {
        &self.scope
    }
    pub fn publication(&self) -> u64 {
        self.publication
    }
    pub fn connection_epoch(&self) -> u64 {
        self.connection_epoch
    }
    pub fn records(&self) -> &SessionMessageFeedbackList {
        &self.records
    }
    pub fn put_advertised(&self) -> bool {
        self.put_advertised
    }
    pub fn delete_advertised(&self) -> bool {
        self.delete_advertised
    }
    /// Display metadata is not mutation authority. A future dispatcher must
    /// revalidate the target and CAS again immediately before IO.
    pub fn is_current(&self, source: &Arc<RunBinding>) -> bool {
        self.source
            .upgrade()
            .is_some_and(|original| Arc::ptr_eq(&original, source))
            && source.scope() == &self.scope
            && source.access() == PaneAccess::Ready
            && source.controller.is_some()
            && source.feedback_reads.load(Ordering::Acquire) == self.publication
            && source.runtime.inspect(|state| {
                state.connection == ConnectionState::Ready
                    && state.connection_epoch == self.connection_epoch
                    && state.sessions.contains_key(&self.scope.session_id)
                    && state.endpoint_capabilities.supports("feedback.list")
                    && state.endpoint_capabilities.supports("feedback.put") == self.put_advertised
                    && state.endpoint_capabilities.supports("feedback.delete")
                        == self.delete_advertised
            })
    }
}
impl RunBinding {
    pub async fn read_feedback(self: &Arc<Self>) -> Result<FeedbackFrame> {
        require(
            self.access() == PaneAccess::Ready && self.controller.is_some(),
            "feedback requires an independently addressable ready run",
        )?;
        let state = self.runtime.snapshot();
        require(
            state.connection == ConnectionState::Ready
                && state.sessions.contains_key(&self.scope.session_id)
                && state.endpoint_capabilities.supports("feedback.list"),
            "feedback read is unavailable",
        )?;
        // Allocate before await: completion order must never become publication order.
        let publication = self
            .feedback_reads
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                value.checked_add(1)
            })
            .map_err(|_| unsupported("feedback publication exhausted"))?
            + 1;
        let raw = self
            .runtime
            .call(
                "feedback.list",
                serde_json::json!({"sessionId":self.scope.session_id}),
                &format!("feedback-read:{}", uuid::Uuid::new_v4()),
            )
            .await?;
        let frame = FeedbackFrame {
            source: Arc::downgrade(self),
            scope: self.scope.clone(),
            publication,
            connection_epoch: state.connection_epoch,
            records: SessionMessageFeedbackList::read(raw)?,
            put_advertised: state.endpoint_capabilities.supports("feedback.put"),
            delete_advertised: state.endpoint_capabilities.supports("feedback.delete"),
        };
        require(
            frame.is_current(self),
            "feedback read source or connection changed",
        )?;
        Ok(frame)
    }
}
