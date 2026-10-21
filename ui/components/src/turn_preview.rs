//! ClientKit's AgentKitTranscriptView + TranscriptTurnSummary read-model rules.
//! Plain preview facts are distinct from the upstream anchored content surface.
use tekes_agent_presentation::{TranscriptItem, TranscriptTurn};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptTurnPreview {
    pub input: String,
    /// First 240 extended grapheme clusters of the joined assistant text.
    pub output: String,
    pub has_final_answer: bool,
    /// Same normalized AgentKit status rule as ClientKit's AgentKit bridge.
    pub is_settled: bool,
}
impl TranscriptTurnPreview {
    pub const OUTPUT_CHARACTER_LIMIT: usize = 240;
    pub(crate) fn read(turn: &TranscriptTurn, outputs: &[&TranscriptItem]) -> Self {
        let joined = outputs
            .iter()
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        Self {
            input: turn.input.clone(),
            output: joined
                .graphemes(true)
                .take(Self::OUTPUT_CHARACTER_LIMIT)
                .collect(),
            has_final_answer: outputs.iter().any(|item| item.is_final),
            is_settled: turn.status != "running",
        }
    }
    /// Running placeholder belongs only to an empty, unsettled output. A
    /// completed empty turn must not acquire invented output or activity.
    pub fn output_text<'a>(&'a self, running_label: &'a str) -> Option<&'a str> {
        if !self.output.is_empty() {
            Some(&self.output)
        } else if !self.is_settled {
            Some(running_label)
        } else {
            None
        }
    }
}
