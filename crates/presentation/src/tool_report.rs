//! Tool-owned report vocabulary; hosts supply copy and workspace diff facts.
use serde::{Deserialize, Serialize};

/// Explicit producer/host report, matching ClientKit's report value. This is
/// never recovered from stdout, private tool metadata, or another call.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptToolReport {
    /// Open vocabulary, including keys introduced by a newer producer.
    pub verb: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_object: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_lines: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub removed_lines: Option<i64>,
}

/// One complete line-delta pair. Zero and signed source values are retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptToolChangeCount {
    pub additions: i64,
    pub deletions: i64,
}
impl TranscriptToolChangeCount {
    /// The call's own complete delta wins, including (0, 0). The fallback is a
    /// host's current workspace diff and may describe later edits to this file.
    /// Like ClientKit's renderedChangeCount, this choice is not phase-gated.
    pub fn select(report: Option<&TranscriptToolReport>, workspace: Option<Self>) -> Option<Self> {
        report
            .and_then(TranscriptToolReport::change_count)
            .or(workspace)
    }
}

/// Full localized phrases, not independent translated words. Hosts own copy,
/// plural/compact-count formatting and word order; unknown keys reach them intact.
#[derive(Clone, Copy, Debug)]
pub struct TranscriptToolReportWording {
    pub subject_phrase: fn(verb: &str, subject: &str) -> String,
    pub verb: fn(verb: &str) -> String,
    pub magnitude: fn(count: i64, object: &str) -> String,
    pub join: fn(left: &str, right: &str) -> String,
}
impl TranscriptToolReport {
    pub fn change_count(&self) -> Option<TranscriptToolChangeCount> {
        Some(TranscriptToolChangeCount {
            additions: self.added_lines?,
            deletions: self.removed_lines?,
        })
    }
    /// A bare verb belongs to group vocabulary; a note alone does not name the
    /// call. Preserve the source's nonempty (rather than trim/normalize) rule.
    pub fn names_the_call(&self) -> bool {
        self.subject.as_ref().is_some_and(|value| !value.is_empty())
            || (self.result_count.is_some()
                && self
                    .result_object
                    .as_ref()
                    .is_some_and(|value| !value.is_empty()))
            || self.change_count().is_some()
    }
    /// Feed this action to presentation_row's existing settled_action slot.
    /// That shared model retains success/error/stopped/running precedence; this
    /// report formatter does not establish execution or settlement authority.
    pub fn row_text(&self, wording: TranscriptToolReportWording) -> Option<String> {
        if !self.names_the_call() {
            return None;
        }
        let mut text = self
            .subject
            .as_ref()
            .filter(|value| !value.is_empty())
            .map_or_else(
                || (wording.verb)(&self.verb),
                |subject| (wording.subject_phrase)(&self.verb, subject),
            );
        if let (Some(count), Some(object)) = (self.result_count, &self.result_object)
            && !object.is_empty()
        {
            text = (wording.join)(&text, &(wording.magnitude)(count, object));
        }
        if let Some(note) = self.note.as_ref().filter(|value| !value.is_empty()) {
            text = (wording.join)(&text, note);
        }
        Some(text)
    }
}
