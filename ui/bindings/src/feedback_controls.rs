//! Original-binding bridge for public feedback controls. No host-side CAS policy.
use crate::{FeedbackFrame, FeedbackOutcome, FeedbackPreparation, FeedbackRequest, RunBinding};
use std::sync::{Arc, Weak};
use tekes_agent_components_ui::{
    FeedbackControlResult, FeedbackControlState, FeedbackGesture, FeedbackGestureAction,
    TurnFeedback,
};
use tekes_agent_contract::Result;
use tekes_agent_presentation::FeedbackTargetCapture;

impl FeedbackFrame {
    pub fn controls(
        &self,
        source: &Arc<RunBinding>,
        target: &FeedbackTargetCapture,
    ) -> Result<FeedbackControlState> {
        crate::feedback_write::validate(source, self, target, None)?;
        Ok(FeedbackControlState {
            target: target.clone(),
            publication: self.publication(),
            connection_epoch: self.connection_epoch(),
            current: self.records().get(target.message_id()).cloned(),
            put_available: self.put_advertised(),
            delete_available: self.delete_advertised(),
        })
    }
}
enum Operation {
    Ignored,
    Read,
    Write(Box<FeedbackRequest>),
    Unchanged(FeedbackFrame),
    Failed(String),
}
/// Prepared synchronously at the gesture's origin. Hosts may move it to an executor
/// without looking up the latest selected binding or reconstructing payloads.
pub struct FeedbackUiRequest {
    source: Arc<RunBinding>,
    gesture: FeedbackGesture,
    operation: Operation,
}
pub enum FeedbackUiAdoption {
    Ignored,
    Adopted(Option<FeedbackFrame>),
}
pub struct FeedbackUiCompletion {
    source: Weak<RunBinding>,
    ignored: bool,
    gesture: FeedbackGesture,
    result: FeedbackControlResult,
    frame: Option<FeedbackFrame>,
}
impl RunBinding {
    pub fn prepare_feedback_gesture(
        self: &Arc<Self>,
        frame: Option<&FeedbackFrame>,
        gesture: FeedbackGesture,
    ) -> FeedbackUiRequest {
        let operation = if !gesture.claim() {
            Operation::Ignored
        } else if gesture.target().scope() != self.scope() {
            Operation::Failed("feedback gesture belongs to another source".into())
        } else {
            match gesture.action() {
                FeedbackGestureAction::Refresh => Operation::Read,
                FeedbackGestureAction::Change(change) => match frame
                    .ok_or_else(|| "feedback has not been loaded".to_string())
                    .and_then(|frame| {
                        self.prepare_feedback(frame, gesture.target(), change.clone())
                            .map_err(|error| error.to_string())
                    }) {
                    Ok(FeedbackPreparation::Request(request)) => Operation::Write(request),
                    Ok(FeedbackPreparation::Unchanged(frame)) => Operation::Unchanged(frame),
                    Err(error) => Operation::Failed(error),
                },
            }
        };
        FeedbackUiRequest {
            source: self.clone(),
            gesture,
            operation,
        }
    }
}
impl FeedbackUiRequest {
    pub async fn execute(self) -> FeedbackUiCompletion {
        let outcome = match self.operation {
            Operation::Ignored => {
                return FeedbackUiCompletion {
                    source: Arc::downgrade(&self.source),
                    ignored: true,
                    gesture: self.gesture,
                    result: FeedbackControlResult::NeedsRefresh(String::new()),
                    frame: None,
                };
            }
            Operation::Read => match self.source.read_feedback().await {
                Ok(frame) => FeedbackOutcome::Applied(frame),
                Err(error) => FeedbackOutcome::NeedsRefresh(error.to_string()),
            },
            Operation::Write(request) => request.execute().await,
            Operation::Unchanged(frame) => FeedbackOutcome::Applied(frame),
            Operation::Failed(error) => FeedbackOutcome::NeedsRefresh(error),
        };
        let conflict = matches!(&outcome, FeedbackOutcome::Conflict(_));
        let (result, frame) = match outcome {
            FeedbackOutcome::Applied(frame) | FeedbackOutcome::Conflict(frame) => {
                match frame.controls(&self.source, self.gesture.target()) {
                    Ok(state) => (
                        if conflict {
                            FeedbackControlResult::Conflict(state)
                        } else {
                            FeedbackControlResult::Applied(state)
                        },
                        Some(frame),
                    ),
                    Err(error) => (FeedbackControlResult::NeedsRefresh(error.to_string()), None),
                }
            }
            FeedbackOutcome::Rejected { message, frame } => {
                let state = frame
                    .as_ref()
                    .and_then(|frame| frame.controls(&self.source, self.gesture.target()).ok());
                let frame = state.as_ref().and(frame);
                (FeedbackControlResult::Rejected { state, message }, frame)
            }
            FeedbackOutcome::NeedsRefresh(error) => {
                (FeedbackControlResult::NeedsRefresh(error), None)
            }
        };
        FeedbackUiCompletion {
            source: Arc::downgrade(&self.source),
            ignored: false,
            gesture: self.gesture,
            result,
            frame,
        }
    }
}
impl FeedbackUiCompletion {
    /// A stale/closed mount never adopts a result or replaces a newer draft.
    /// None also invalidates a host's old frame after an uncertain completion.
    pub fn apply(mut self, control: &mut TurnFeedback) -> FeedbackUiAdoption {
        if self.frame.as_ref().is_some_and(|frame| {
            self.source
                .upgrade()
                .is_none_or(|source| frame.controls(&source, self.gesture.target()).is_err())
        }) {
            self.frame = None;
            self.result = FeedbackControlResult::NeedsRefresh(
                "feedback completion was superseded before adoption".into(),
            );
        }
        if self.ignored || !control.complete(&self.gesture, self.result) {
            FeedbackUiAdoption::Ignored
        } else {
            FeedbackUiAdoption::Adopted(self.frame)
        }
    }
}
