//! Evaluation targets use public assistant identity, never display placeholders.
use crate::{RunScope, TranscriptItem};
use crate::{TranscriptMessageKey, TranscriptOutput, TranscriptSection, TranscriptSourcePosition};
use tekes_agent_contract::{
    Result, SessionMessageFeedback, SessionMessageFeedbackRating, require, unsupported,
};

#[derive(Clone, Debug, PartialEq)]
pub enum FeedbackChange {
    SetRating(Option<SessionMessageFeedbackRating>),
    ToggleRating(SessionMessageFeedbackRating),
    /// None removes the note, not the rating. Editor trimming is a UI concern.
    SetNote(Option<String>),
}
#[derive(Clone, Debug, PartialEq)]
pub enum FeedbackWritePlan {
    Unchanged,
    Put {
        message_id: String,
        rating: SessionMessageFeedbackRating,
        if_version: Option<String>,
        note: Option<String>,
    },
    Delete {
        message_id: String,
        if_version: String,
    },
}
impl FeedbackWritePlan {
    pub fn method(&self) -> Option<&'static str> {
        match self {
            Self::Unchanged => None,
            Self::Put { .. } => Some("feedback.put"),
            Self::Delete { .. } => Some("feedback.delete"),
        }
    }
    pub fn payload(&self, session_id: &str) -> Option<serde_json::Value> {
        use serde_json::json;
        match self {
            Self::Unchanged => None,
            Self::Put {
                message_id,
                rating,
                if_version,
                note,
            } => Some(
                json!({"sessionId":session_id,"messageId":message_id,"rating":rating,"ifVersion":if_version,"note":note}),
            ),
            Self::Delete {
                message_id,
                if_version,
            } => {
                Some(json!({"sessionId":session_id,"messageId":message_id,"ifVersion":if_version}))
            }
        }
    }
}
/// Plan only from successfully loaded committed feedback, never optimistic UI state.
pub fn plan_feedback_change(
    message_id: &str,
    current: Option<&SessionMessageFeedback>,
    change: FeedbackChange,
) -> Result<FeedbackWritePlan> {
    require(!message_id.is_empty(), "feedback message identity is empty")?;
    if let Some(current) = current {
        current.validate()?;
        require(
            current.message_id == message_id,
            "feedback current record identity mismatch",
        )?;
    }
    let (rating, note) = match change {
        FeedbackChange::SetRating(rating) => (rating, current.and_then(|value| value.note.clone())),
        FeedbackChange::ToggleRating(rating) => (
            if current.is_some_and(|value| value.rating == rating) {
                None
            } else {
                Some(rating)
            },
            None,
        ),
        FeedbackChange::SetNote(note) => {
            let Some(current) = current else {
                return if note.is_none() {
                    Ok(FeedbackWritePlan::Unchanged)
                } else {
                    Err(unsupported("choose a rating before editing its note"))
                };
            };
            if note.is_none() && current.note.is_none() {
                return Ok(FeedbackWritePlan::Unchanged);
            }
            (Some(current.rating), note)
        }
    };
    Ok(match rating {
        Some(rating) => FeedbackWritePlan::Put {
            message_id: message_id.into(),
            rating,
            if_version: current.map(|value| value.version.clone()),
            note,
        },
        None => match current {
            Some(current) => FeedbackWritePlan::Delete {
                message_id: message_id.into(),
                if_version: current.version.clone(),
            },
            None => FeedbackWritePlan::Unchanged,
        },
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct FeedbackTargetCapture {
    scope: RunScope,
    turn_id: String,
    message_id: String,
    response: TranscriptItem,
}
impl FeedbackTargetCapture {
    pub fn scope(&self) -> &RunScope {
        &self.scope
    }
    pub fn turn_id(&self) -> &str {
        &self.turn_id
    }
    pub fn message_id(&self) -> &str {
        &self.message_id
    }
    pub fn matches(&self, target: TurnFeedbackTarget<'_>) -> bool {
        let old = &self.response;
        let next = target.response.item;
        let same_source = matches!(
            (&old.source_position, &next.source_position),
            (
                Some(TranscriptSourcePosition::Native { .. }),
                Some(TranscriptSourcePosition::Native { .. })
            ) | (
                Some(TranscriptSourcePosition::Journal { .. }),
                Some(TranscriptSourcePosition::Journal { .. })
            )
        );
        self.turn_id == target.turn_id
            && self.message_id == target.message_id
            && same_source
            && old.text == next.text
            && old.reasoning == next.reasoning
            && old.attachments == next.attachments
            && old.content_blocks == next.content_blocks
            && old.reasoning_ended == next.reasoning_ended
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurnFeedbackTarget<'a> {
    pub turn_id: &'a str,
    pub message_id: &'a str,
    pub response: TranscriptOutput<'a>,
}
impl<'a> TurnFeedbackTarget<'a> {
    pub fn capture(&self, scope: &RunScope) -> Result<FeedbackTargetCapture> {
        scope.validate()?;
        Ok(FeedbackTargetCapture {
            scope: scope.clone(),
            turn_id: self.turn_id.into(),
            message_id: self.message_id.into(),
            response: self.response.item.clone(),
        })
    }
    /// Selection facts only. Dispatch must recheck scope, pointer, epoch,
    /// capability and the current committed feedback CAS version.
    pub fn read(section: &'a TranscriptSection<'a>) -> Option<Self> {
        let response = section.final_response()?;
        let record = response
            .item
            .record_id
            .as_deref()
            .filter(|id| !id.is_empty())?;
        let message_id = match (&response.key, &response.item.source_position) {
            (
                TranscriptMessageKey::NativeEntry(entry),
                Some(TranscriptSourcePosition::Native { entry_id, .. }),
            ) if *entry == entry_id && record == *entry => record,
            (_, Some(TranscriptSourcePosition::Journal { .. }))
                if response.item.metadata["feedbackMessageId"].as_str() == Some(record) =>
            {
                record
            }
            _ => return None,
        };
        Some(Self {
            turn_id: section.id,
            message_id,
            response,
        })
    }
}
