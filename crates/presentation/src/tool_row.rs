//! Collapsed tool-row selection, shared by renderer consumers. No execution state.
use crate::TranscriptToolCall;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TranscriptToolRowState {
    Running,
    Ok,
    Error,
    Stopped,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptToolRowModel {
    pub title: String,
    pub summary: Option<String>,
    pub error_summary: Option<String>,
    pub state: TranscriptToolRowState,
}
/// Display inputs supplied by the existing projection/card adapter. Wording is
/// host-localizable; `settled_action` is a formatted tool-owned report, never stdout.
#[derive(Clone, Copy, Debug, Default)]
pub struct TranscriptToolRowWording<'a> {
    pub label: &'a str,
    pub stopped: &'a str,
    pub title_hint: Option<&'a str>,
    pub detail: Option<&'a str>,
    pub progress: Option<&'a str>,
    pub result: Option<&'a str>,
    pub settled_action: Option<&'a str>,
}
impl TranscriptToolRowModel {
    pub fn visible_summary(&self) -> Option<&str> {
        if self.state == TranscriptToolRowState::Error {
            self.error_summary.as_deref().or(self.summary.as_deref())
        } else {
            self.summary.as_deref()
        }
    }
    pub fn text(&self) -> String {
        self.visible_summary()
            .filter(|text| !text.is_empty())
            .map_or_else(
                || self.title.clone(),
                |text| format!("{} · {text}", self.title),
            )
    }
}
impl TranscriptToolCall {
    /// Selects the same collapsed title/summary/error precedence as ClientKit's
    /// TranscriptToolRowModel. `phase` remains canonical; raw result flags only
    /// distinguish an explicit tool error from an exit-status-only failure.
    pub fn presentation_row(
        &self,
        wording: TranscriptToolRowWording<'_>,
    ) -> TranscriptToolRowModel {
        use TranscriptToolRowState as State;
        let state = match self.phase.as_str() {
            "interrupted" | "stopped" => State::Stopped,
            "error" => State::Error,
            "done" => State::Ok,
            _ => State::Running,
        };
        if state == State::Ok
            && let Some(action) = first_line(wording.settled_action)
        {
            return TranscriptToolRowModel {
                title: action,
                summary: None,
                error_summary: None,
                state,
            };
        }
        let hint = first_line(wording.title_hint).filter(|hint| {
            hint.to_lowercase() != self.name.to_lowercase()
                && hint.to_lowercase() != wording.label.to_lowercase()
        });
        let summary = if state == State::Stopped {
            first_line(Some(wording.stopped))
        } else {
            (state == State::Running)
                .then(|| first_line(wording.progress))
                .flatten()
                .or_else(|| first_line(wording.detail))
                .or(hint)
                .or_else(|| argument_summary(&self.name, &self.arguments))
        };
        let explicitly_failed = self.result_error.as_ref().is_some_and(Value::is_object)
            || self.result.as_ref().is_some_and(|result| {
                result["isError"] == true
                    || result["content"]
                        .as_array()
                        .is_some_and(|parts| parts.iter().any(|part| part["isError"] == true))
            });
        TranscriptToolRowModel {
            title: wording.label.into(),
            summary,
            error_summary: (state == State::Error && explicitly_failed)
                .then(|| first_line(wording.result))
                .flatten(),
            state,
        }
    }
}
fn first_line(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    let first = value
        .split([
            '\n', '\r', '\u{b}', '\u{c}', '\u{85}', '\u{2028}', '\u{2029}',
        ])
        .next()?;
    let normalized = first.split_whitespace().collect::<Vec<_>>().join(" ");
    (!normalized.is_empty()).then_some(normalized)
}
fn argument_summary(name: &str, arguments: &Value) -> Option<String> {
    let decoded = arguments
        .as_str()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok());
    let value = decoded.as_ref().unwrap_or(arguments);
    let Some(object) = value.as_object() else {
        return first_line(arguments.as_str());
    };
    let family = name.split('.').next().unwrap_or(name);
    let keys: &[&str] = match family {
        "web_search" | "grep" | "glob" | "tool_search" => {
            if let Some(queries) = object.get("queries").and_then(Value::as_array)
                && queries.iter().all(Value::is_string)
            {
                let summary = queries
                    .iter()
                    .filter_map(|query| first_line(query.as_str()))
                    .collect::<Vec<_>>()
                    .join(", ");
                if !summary.is_empty() {
                    return Some(summary);
                }
            }
            &["query", "pattern", "url"]
        }
        "shell" | "bash" | "pwsh" => &["description", "command", "script"],
        "read" | "read_image" | "web_fetch" | "fetch" => &["path", "file_path", "url"],
        "write" | "edit" | "str_replace_editor" => &["path", "file_path"],
        "run_code" => &["description"],
        _ => &["description", "title", "query", "path", "url"],
    };
    for key in keys {
        if let Some(value) = first_line(object.get(*key).and_then(Value::as_str)) {
            return Some(value);
        }
    }
    let mut keys = object.keys().collect::<Vec<_>>();
    keys.sort();
    for key in keys {
        if let Some(value) = first_line(object[key].as_str()) {
            return Some(value);
        }
    }
    first_line(arguments.as_str()).or_else(|| first_line(Some(&arguments.to_string())))
}
