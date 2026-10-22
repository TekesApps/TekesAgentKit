//! Historical input is an explicit host callback, not a guessed endpoint RPC.
use crate::TranscriptMessageIdentity;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tekes_agent_presentation::{RunScope, TranscriptItem};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoricalInputAction {
    Edit,
    Rewind,
}
/// Install only when the endpoint advertises historical editing and the host
/// has a handler. The standard ClientKit thread uses Edit; Rewind is explicit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoricalInputControl {
    pub action: HistoricalInputAction,
    pub label: String,
}
/// Original whole input, including attachment/source facts. This requests the
/// host's editing flow; it neither rewrites history nor replaces a draft.
#[derive(Clone, Debug)]
pub struct HistoricalInputIntent {
    pub(crate) scope: RunScope,
    pub(crate) message: TranscriptMessageIdentity,
    pub(crate) input: TranscriptItem,
    pub(crate) publication: u64,
    pub(crate) epoch: u64,
    pub(crate) action: HistoricalInputAction,
    pub(crate) claimed: Arc<AtomicBool>,
}
impl HistoricalInputIntent {
    pub fn scope(&self) -> &RunScope {
        &self.scope
    }
    pub fn message(&self) -> &TranscriptMessageIdentity {
        &self.message
    }
    pub fn input(&self) -> &TranscriptItem {
        &self.input
    }
    pub fn turn_id(&self) -> &str {
        &self.input.turn_id
    }
    pub fn publication(&self) -> u64 {
        self.publication
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn action(&self) -> HistoricalInputAction {
        self.action
    }
    /// Clones share one callback admission. A binding must independently check
    /// current pointer, selection, epoch, capabilities and original source.
    pub fn claim(&self) -> bool {
        !self.claimed.swap(true, Ordering::AcqRel)
    }
}
