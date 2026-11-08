//! Transient attachment-operation facts, separate from canonical Run state.
use crate::ComposerDestination;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AttachmentTransferPhase {
    Selecting,
    Uploading {
        index: usize,
        total: usize,
    },
    Attached {
        count: usize,
    },
    Detached {
        completed: usize,
    },
    Failed {
        completed: usize,
        uncertain: bool,
        message: String,
    },
    Cancelled,
}
impl AttachmentTransferPhase {
    pub fn is_pending(&self) -> bool {
        matches!(self, Self::Selecting | Self::Uploading { .. })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentTransferSnapshot {
    pub revision: u64,
    pub request_id: String,
    pub destination: ComposerDestination,
    pub phase: AttachmentTransferPhase,
}
