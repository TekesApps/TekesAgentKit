//! Evaluation targets use public assistant identity, never display placeholders.
use crate::{TranscriptMessageKey, TranscriptOutput, TranscriptSection, TranscriptSourcePosition};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurnFeedbackTarget<'a> {
    pub turn_id: &'a str,
    pub message_id: &'a str,
    pub response: TranscriptOutput<'a>,
}
impl<'a> TurnFeedbackTarget<'a> {
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
