//! Business text projection; no glyph layout, font/color values or input state.
use crate::{
    TranscriptToolCall, TranscriptToolChangeCount, TranscriptToolReport,
    TranscriptToolReportWording, TranscriptToolRowModel, TranscriptToolRowState,
    TranscriptToolRowWording,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Explicit host facts. Never recovered from tool-private metadata or stdout.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptToolRowInputs {
    pub report: Option<TranscriptToolReport>,
    pub workspace_change_count: Option<TranscriptToolChangeCount>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TranscriptToolHeaderRole {
    Secondary,
    DiffAdded,
    DiffRemoved,
}

/// UTF-16 source range, matching ClientKit's NSRange business spans. The text
/// SDK remains responsible for validating/shaping/mapping ranges when consumed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptToolHeaderSpan {
    pub start_utf16: usize,
    pub length_utf16: usize,
    pub role: TranscriptToolHeaderRole,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptToolHeader {
    pub tool_id: String,
    pub turn_id: String,
    /// Public source DTO only, excluding external reports/counts. Display
    /// revisions cannot change tool facts in the same Run publication.
    pub tool_fingerprint: String,
    pub row: TranscriptToolRowModel,
    pub text: String,
    pub spans: Vec<TranscriptToolHeaderSpan>,
    pub change_count: Option<TranscriptToolChangeCount>,
}

impl TranscriptToolCall {
    /// Own-report authority, failure/stopped precedence, complete delta
    /// selection and text construction stay shared across every consumer.
    pub fn presentation_header(
        &self,
        wording: TranscriptToolRowWording<'_>,
        inputs: &TranscriptToolRowInputs,
        report_wording: TranscriptToolReportWording,
    ) -> TranscriptToolHeader {
        let action = inputs
            .report
            .as_ref()
            .and_then(|r| r.row_text(report_wording));
        let row = self.presentation_row(TranscriptToolRowWording {
            settled_action: action.as_deref().or(wording.settled_action),
            ..wording
        });
        let change_count = TranscriptToolChangeCount::select(
            inputs.report.as_ref(),
            inputs.workspace_change_count,
        );
        let mut text = row.text();
        let mut spans = vec![];
        if row.state != TranscriptToolRowState::Error
            && let Some(summary) = row.visible_summary().filter(|s| !s.is_empty())
        {
            spans.push(TranscriptToolHeaderSpan {
                start_utf16: row.title.encode_utf16().count(),
                length_utf16: 3 + summary.encode_utf16().count(),
                role: TranscriptToolHeaderRole::Secondary,
            });
        }
        if let Some(count) = change_count {
            let added = format!("+{}", count.additions);
            let removed = format!("−{}", count.deletions);
            let start = text.encode_utf16().count() + 4;
            spans.push(TranscriptToolHeaderSpan {
                start_utf16: start,
                length_utf16: added.encode_utf16().count(),
                role: TranscriptToolHeaderRole::DiffAdded,
            });
            spans.push(TranscriptToolHeaderSpan {
                start_utf16: start + added.encode_utf16().count() + 1,
                length_utf16: removed.encode_utf16().count(),
                role: TranscriptToolHeaderRole::DiffRemoved,
            });
            text.push_str(&format!("    {added} {removed}"));
        }
        TranscriptToolHeader {
            tool_id: self.id.clone(),
            turn_id: self.turn_id.clone(),
            tool_fingerprint: format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(self).expect("public tool DTO serializes"))
            ),
            row,
            text,
            spans,
            change_count,
        }
    }
}
