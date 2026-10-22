//! Expanded tool sections shared by UI consumers; no execution or scroll state.
use crate::TranscriptToolCall;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TranscriptToolDetailKind {
    Input,
    Output,
    CallView,
    ResultView,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptToolDetailSection {
    /// Stable within the original tool identity, independent of publication.
    pub kind: TranscriptToolDetailKind,
    pub title: String,
    /// Display removes at most one final LF, matching the source View.
    pub text: String,
    /// Copy retains the complete original section text, including its final LF.
    pub copy_text: String,
    pub language: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptToolDetails {
    pub sections: Vec<TranscriptToolDetailSection>,
    pub running: bool,
}
/// Localized copy and producer-owned presentation facts. Progress is supplied
/// explicitly, never inferred from opaque harness metadata or a sibling call.
#[derive(Clone, Copy, Debug)]
pub struct TranscriptToolDetailWording<'a> {
    pub input: &'a str,
    pub output: &'a str,
    pub failed: &'a str,
    pub call_view: &'a str,
    pub result_view: &'a str,
    pub receiving_input: &'a str,
    pub input_unavailable: &'a str,
    pub fallback_output: &'a str,
    pub progress: Option<&'a str>,
    pub progress_detail: Option<&'a str>,
}
impl Default for TranscriptToolDetailWording<'_> {
    fn default() -> Self {
        Self {
            input: "Input",
            output: "Output",
            failed: "Failed",
            call_view: "Call view",
            result_view: "Result view",
            receiving_input: "Receiving input",
            input_unavailable: "Input unavailable",
            fallback_output: "",
            progress: None,
            progress_detail: None,
        }
    }
}
impl TranscriptToolCall {
    /// Matches ClientKit detailNodes/ioDetailNodes generic section ordering and
    /// output accumulation. Rich card/Markdown nodes are a separate capability.
    pub fn presentation_details(
        &self,
        wording: TranscriptToolDetailWording<'_>,
    ) -> TranscriptToolDetails {
        use TranscriptToolDetailKind as Kind;
        let failed = self.failed_for_presentation();
        let running = !matches!(self.phase.as_str(), "done" | "interrupted" | "stopped") && !failed;
        let arguments = self
            .arguments
            .as_str()
            .map_or_else(|| self.arguments.to_string(), str::to_owned);
        let input = if arguments.trim().is_empty() {
            if self.phase == "will" {
                wording.receiving_input
            } else {
                wording.input_unavailable
            }
            .into()
        } else {
            arguments
        };
        // A delivered empty content array is Some("") in the source projector.
        // Only an absent result may fall back to the tool's display title.
        let result = self
            .presentation_result_text()
            .or_else(|| self.result.as_ref().map(|_| String::new()));
        let mut output = Vec::new();
        for value in [result.as_deref(), wording.progress, wording.progress_detail]
            .into_iter()
            .flatten()
        {
            if !value.trim().is_empty() && !output.contains(&value) {
                output.push(value);
            }
        }
        let output = if output.is_empty() {
            if running {
                String::new()
            } else {
                result.unwrap_or_else(|| wording.fallback_output.into())
            }
        } else {
            output.join("\n")
        };
        let section =
            |kind, title: String, copy_text: String, language| TranscriptToolDetailSection {
                kind,
                title,
                text: copy_text.strip_suffix('\n').unwrap_or(&copy_text).into(),
                copy_text,
                language,
            };
        let mut sections = vec![
            section(Kind::Input, wording.input.into(), input, None),
            section(
                Kind::Output,
                if failed {
                    format!("{} · {}", wording.output, wording.failed)
                } else {
                    wording.output.into()
                },
                output,
                None,
            ),
        ];
        for (kind, title, value) in [
            (Kind::CallView, wording.call_view, self.call_view.as_ref()),
            (
                Kind::ResultView,
                wording.result_view,
                self.result_view.as_ref(),
            ),
        ] {
            if let Some(value) = value {
                sections.push(section(
                    kind,
                    title.into(),
                    value.to_string(),
                    Some("json".into()),
                ));
            }
        }
        TranscriptToolDetails { sections, running }
    }
}
