//! Producer-owned SEP title selection; no tool execution or display state.
use crate::TranscriptToolCall;
use serde_json::Value;

impl TranscriptToolCall {
    /// The current ClientKit SEP projector's tool-row title choice. Public
    /// HistoryEntry views retain their {"for": "call" | "result", "view": ...}
    /// envelope here; only a matching envelope contributes producer wording.
    /// Complete raw facts remain unchanged for the separate expansion renderer.
    /// This is not the complete rich card/row model or a success signal.
    pub fn presentation_title(&self) -> String {
        let call = payload(self.call_view.as_ref(), "call");
        let result = payload(self.result_view.as_ref(), "result");
        let decoded = self
            .arguments
            .as_str()
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok());
        let arguments = decoded.as_ref().unwrap_or(&self.arguments);
        let terminal = call
            .and_then(|view| view.get("card"))
            .and_then(Value::as_str)
            == Some("terminal");
        let candidates = if terminal {
            [
                (field(call, "description"), false),
                (field(Some(arguments), "description"), false),
                (field(call, "title"), true),
                (field(Some(arguments), "command"), true),
                (Some(self.name.as_str()), false),
            ]
        } else {
            [
                (field(result, "title"), false),
                (field(call, "title"), false),
                (Some(self.name.as_str()), false),
                (None, false),
                (None, false),
            ]
        };
        candidates
            .into_iter()
            .find_map(|(value, first_line)| {
                let value = value?;
                // Swift split(whereSeparator: isNewline) omits empty subsequences,
                // but a whitespace-only first line remains an empty candidate.
                let value = if first_line {
                    value
                        .split(|c| {
                            matches!(
                                c,
                                '\n' | '\r'
                                    | '\u{b}'
                                    | '\u{c}'
                                    | '\u{85}'
                                    | '\u{2028}'
                                    | '\u{2029}'
                            )
                        })
                        .find(|line| !line.is_empty())
                        .unwrap_or("")
                } else {
                    value
                };
                let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
                (!normalized.is_empty()).then_some(normalized)
            })
            .unwrap_or_else(|| {
                if self.name.is_empty() {
                    "Preparing"
                } else {
                    "Tool"
                }
                .into()
            })
    }
}
fn payload<'a>(value: Option<&'a Value>, expected: &str) -> Option<&'a Value> {
    let value = value?;
    (value.get("for").and_then(Value::as_str) == Some(expected))
        .then(|| value.get("view"))
        .flatten()
}
fn field<'a>(value: Option<&'a Value>, name: &str) -> Option<&'a str> {
    value?.get(name)?.as_str()
}
