//! One explicit CAS operation. No automatic retry, optimistic rating or Run state.
use crate::{
    FeedbackChange, FeedbackFrame, FeedbackTargetCapture, RunBinding, SessionFeedbackFailure,
    SessionMessageFeedback,
};
use std::sync::Arc;
use tekes_agent_contract::{Error, Result, require, unsupported};
use tekes_agent_presentation::{
    ComposerDestination, FeedbackWritePlan, TranscriptDocument, TranscriptSnapshot,
    TurnFeedbackTarget, plan_feedback_change,
};

pub enum FeedbackPreparation {
    Unchanged(FeedbackFrame),
    Request(Box<FeedbackRequest>),
}
#[derive(Debug)]
pub enum FeedbackOutcome {
    Applied(FeedbackFrame),
    Conflict(FeedbackFrame),
    Rejected {
        message: String,
        frame: Option<FeedbackFrame>,
    },
    /// Retain the readout and explicitly refresh before another gesture.
    /// Includes transport loss and superseded completions; never auto-retry.
    NeedsRefresh(String),
}
pub struct FeedbackRequest {
    source: Arc<RunBinding>,
    frame: FeedbackFrame,
    target: FeedbackTargetCapture,
    plan: FeedbackWritePlan,
    rpc_id: String,
}
impl RunBinding {
    pub fn prepare_feedback(
        self: &Arc<Self>,
        frame: &FeedbackFrame,
        target: &FeedbackTargetCapture,
        change: FeedbackChange,
    ) -> Result<FeedbackPreparation> {
        validate(self, frame, target, None)?;
        let plan = plan_feedback_change(
            target.message_id(),
            frame.records().get(target.message_id()),
            change,
        )?;
        let Some(method) = plan.method() else {
            return Ok(FeedbackPreparation::Unchanged(frame.clone()));
        };
        validate(self, frame, target, Some(method))?;
        let frame = frame.reserve_write(self)?;
        Ok(FeedbackPreparation::Request(Box::new(FeedbackRequest {
            source: self.clone(),
            frame,
            target: target.clone(),
            plan,
            rpc_id: format!("feedback-write:{}", uuid::Uuid::new_v4()),
        })))
    }
}
impl FeedbackRequest {
    pub fn rpc_id(&self) -> &str {
        &self.rpc_id
    }
    pub fn scope(&self) -> &tekes_agent_presentation::RunScope {
        self.target.scope()
    }
    pub fn message_id(&self) -> &str {
        self.target.message_id()
    }
    pub async fn execute(self) -> FeedbackOutcome {
        let method = self.plan.method().expect("prepared write");
        if let Err(error) = validate(&self.source, &self.frame, &self.target, Some(method)) {
            return FeedbackOutcome::Rejected {
                message: error.to_string(),
                frame: None,
            };
        }
        let result = self
            .source
            .runtime
            .call_at_epoch(
                self.frame.connection_epoch(),
                method,
                self.plan
                    .payload(&self.scope().session_id)
                    .expect("prepared payload"),
                &self.rpc_id,
            )
            .await;
        if let Err(error) = validate(&self.source, &self.frame, &self.target, Some(method)) {
            return FeedbackOutcome::NeedsRefresh(format!(
                "feedback completion is no longer current: {error}"
            ));
        }
        let message_id = self.target.message_id();
        match result {
            Ok(value) => {
                let parsed: Result<Option<SessionMessageFeedback>> = (|| match &self.plan {
                    FeedbackWritePlan::Put { rating, note, .. } => {
                        let record: SessionMessageFeedback = serde_json::from_value(value)?;
                        record.validate()?;
                        require(
                            record.message_id == message_id
                                && record.rating == *rating
                                && record.note == *note,
                            "feedback receipt does not match requested change",
                        )?;
                        Ok(Some(record))
                    }
                    FeedbackWritePlan::Delete { .. } => {
                        require(
                            value.is_null()
                                || value.get("deleted").and_then(serde_json::Value::as_bool)
                                    == Some(true),
                            "invalid feedback delete receipt",
                        )?;
                        Ok(None)
                    }
                    FeedbackWritePlan::Unchanged => unreachable!(),
                })();
                match parsed.and_then(|record| self.frame.with_record(message_id, record)) {
                    Ok(frame) => FeedbackOutcome::Applied(frame),
                    Err(error) => FeedbackOutcome::NeedsRefresh(error.to_string()),
                }
            }
            Err(Error::Remote(error)) => match SessionFeedbackFailure::read(&error, message_id) {
                Ok(SessionFeedbackFailure::VersionConflict { current }) => {
                    match self.frame.with_record(message_id, current) {
                        Ok(frame) => FeedbackOutcome::Conflict(frame),
                        Err(error) => FeedbackOutcome::NeedsRefresh(error.to_string()),
                    }
                }
                Ok(SessionFeedbackFailure::Rejected { code }) => {
                    if matches!(
                        code.as_str(),
                        "timeout"
                            | "request-timeout"
                            | "disconnected"
                            | "transport"
                            | "io"
                            | "internal"
                            | "internal-error"
                            | "unknown-outcome"
                    ) {
                        FeedbackOutcome::NeedsRefresh(code)
                    } else {
                        FeedbackOutcome::Rejected {
                            message: code,
                            frame: Some(self.frame),
                        }
                    }
                }
                Err(error) => FeedbackOutcome::NeedsRefresh(error.to_string()),
            },
            Err(error) => FeedbackOutcome::NeedsRefresh(error.to_string()),
        }
    }
}
fn validate(
    source: &Arc<RunBinding>,
    frame: &FeedbackFrame,
    target: &FeedbackTargetCapture,
    method: Option<&str>,
) -> Result<()> {
    source.require_controls(&ComposerDestination::run(target.scope().clone()))?;
    require(
        target.scope() == source.scope() && frame.is_current(source),
        "feedback source or publication changed",
    )?;
    let state = source.runtime.snapshot();
    if let Some(method) = method {
        require(
            state.endpoint_capabilities.supports(method),
            "feedback mutation is unavailable",
        )?;
    }
    let session = state
        .sessions
        .get(&source.scope().session_id)
        .ok_or_else(|| unsupported("feedback session unavailable"))?;
    let transcript = TranscriptSnapshot::read(source.scope(), session)?;
    require(!transcript.is_stale, "feedback transcript is stale")?;
    let document = TranscriptDocument::read(&transcript)?;
    require(
        document
            .sections
            .iter()
            .filter_map(TurnFeedbackTarget::read)
            .any(|current| target.matches(current)),
        "feedback response target changed",
    )
}
