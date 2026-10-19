use serde::{Deserialize, Serialize};

/// Provider-reported cumulative output tokens for the current turn, never inferred from text.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptLiveUsage {
    pub output_tokens: u64,
}

pub fn live_usage_typescript() -> &'static str {
    "export interface TranscriptLiveUsage {\n  outputTokens: number;\n}\nexport interface TranscriptReportedUsage {\n  outputTokens: number;\n  isComplete: boolean;\n}\n"
}

/// Sum of actual durable response reports, with explicit coverage of the loaded turn.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptReportedUsage {
    pub output_tokens: u64,
    pub is_complete: bool,
}
