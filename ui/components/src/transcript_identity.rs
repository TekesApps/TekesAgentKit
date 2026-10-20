//! Message identity remains scoped to its original RunScope.
use tekes_agent_presentation::TranscriptMessageKey;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TranscriptMessageIdentity {
    Submission(String),
    NativeEntry(String),
    /// A journal item or temporary preview. Not proof of a durable native anchor.
    Item(String),
}
impl From<TranscriptMessageKey<'_>> for TranscriptMessageIdentity {
    fn from(value: TranscriptMessageKey<'_>) -> Self {
        match value {
            TranscriptMessageKey::Submission(id) => Self::Submission(id.into()),
            TranscriptMessageKey::NativeEntry(id) => Self::NativeEntry(id.into()),
            TranscriptMessageKey::Item(id) => Self::Item(id.into()),
        }
    }
}
