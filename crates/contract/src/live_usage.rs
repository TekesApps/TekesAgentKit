use serde::{Deserialize, Serialize};

/// Provider-reported cumulative output tokens for the current turn, never inferred from text.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptLiveUsage {
    pub output_tokens: u64,
}

pub fn live_usage_typescript() -> &'static str {
    "export interface TranscriptLiveUsage {\n  outputTokens: number;\n}\n"
}
