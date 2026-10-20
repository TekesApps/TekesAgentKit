//! ClientKit SessionFeedback contract. Versions are opaque per-message CAS values.
use crate::{Result, require};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SessionMessageFeedbackRating {
    #[serde(alias = "up")]
    Positive,
    #[serde(alias = "down")]
    Negative,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionMessageFeedback {
    pub message_id: String,
    pub rating: SessionMessageFeedbackRating,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at_milliseconds: Option<f64>,
}
impl SessionMessageFeedback {
    pub fn validate(&self) -> Result<()> {
        require(
            !self.message_id.is_empty(),
            "feedback message identity is empty",
        )?;
        require(!self.version.is_empty(), "feedback CAS version is empty")?;
        require(
            self.updated_at_milliseconds.is_none_or(f64::is_finite),
            "feedback timestamp is not finite",
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SessionMessageFeedbackList {
    pub items: Vec<SessionMessageFeedback>,
}
impl SessionMessageFeedbackList {
    /// Missing/malformed/duplicate records are errors, never an empty success.
    pub fn read(value: serde_json::Value) -> Result<Self> {
        let list: Self = serde_json::from_value(value)?;
        let mut ids = BTreeSet::new();
        for item in &list.items {
            item.validate()?;
            require(
                ids.insert(&item.message_id),
                "duplicate feedback message identity",
            )?;
        }
        Ok(list)
    }
    pub fn get(&self, message_id: &str) -> Option<&SessionMessageFeedback> {
        self.items.iter().find(|item| item.message_id == message_id)
    }
}
