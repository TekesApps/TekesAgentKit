//! Continuous tool-run display facts. Adjacency comes from the canonical owner.
use crate::{TranscriptToolCall, TranscriptToolRowState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptToolNameCount {
    pub name: String,
    pub count: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptToolRunSummary {
    pub turn_id: String,
    pub tool_ids: Vec<String>,
    pub active_summaries: Vec<String>,
    pub active_count: usize,
    pub failed_count: usize,
    pub stopped_count: usize,
    /// First occurrence order of successful tool display names.
    pub completed: Vec<TranscriptToolNameCount>,
}
impl TranscriptToolRunSummary {
    /// Does not invent adjacency or sort calls. The transcript planner supplies
    /// one known adjacent run; a standalone consumer supplies that same contract.
    pub fn read(
        tools: &[&TranscriptToolCall],
        name: impl Fn(&str) -> String,
        preview: impl Fn(&TranscriptToolCall) -> String,
    ) -> Result<Self, &'static str> {
        let first = tools.first().ok_or("empty continuous tool run")?;
        let mut ids = BTreeSet::new();
        let mut result = Self {
            turn_id: first.turn_id.clone(),
            tool_ids: vec![],
            active_summaries: vec![],
            active_count: 0,
            failed_count: 0,
            stopped_count: 0,
            completed: vec![],
        };
        for tool in tools {
            if tool.id.is_empty() || tool.turn_id != first.turn_id || !ids.insert(&tool.id) {
                return Err("invalid continuous tool membership");
            }
            result.tool_ids.push(tool.id.clone());
            match tool.presentation_state() {
                TranscriptToolRowState::Running => {
                    result.active_count += 1;
                    let text = preview(tool);
                    if !text.is_empty() {
                        result.active_summaries.push(text);
                    }
                }
                TranscriptToolRowState::Error => result.failed_count += 1,
                TranscriptToolRowState::Stopped => result.stopped_count += 1,
                TranscriptToolRowState::Ok => {
                    let name = name(&tool.name);
                    if let Some(count) =
                        result.completed.iter_mut().find(|entry| entry.name == name)
                    {
                        count.count += 1;
                    } else {
                        result
                            .completed
                            .push(TranscriptToolNameCount { name, count: 1 });
                    }
                }
            }
        }
        Ok(result)
    }
}
impl TranscriptToolCall {
    /// Same bounded, allowlisted action preview as ClientKit continuousSummary.
    /// Never falls back to arbitrary JSON fields or the raw argument payload.
    pub fn continuous_summary(
        &self,
        label: &str,
        row_title: Option<&str>,
        fallback: &str,
    ) -> String {
        use crate::tool_row::first_line;
        let decoded = self
            .arguments
            .as_str()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok());
        let args = decoded.as_ref().unwrap_or(&self.arguments);
        let value = |keys: &[&str]| {
            keys.iter()
                .find_map(|key| first_line(args.get(*key).and_then(serde_json::Value::as_str)))
        };
        let family = self.name.rsplit('.').next().unwrap_or(&self.name);
        let detail = value(&["description", "title"]).or_else(|| match family {
            "shell" | "bash" | "pwsh" | "exec_command" | "run_command" => {
                value(&["command", "cmd", "script"])
            }
            "read" | "read_file" | "read_image" | "write" | "write_file" | "edit"
            | "str_replace_editor" => value(&["path", "file_path"]).map(|path| {
                let components = path
                    .split('/')
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>();
                components[components.len().saturating_sub(2)..].join("/")
            }),
            "web_search" | "grep" | "glob" | "tool_search" | "search" => {
                value(&["query", "pattern"]).or_else(|| {
                    let values = args.get("queries")?.as_array()?;
                    values.iter().all(serde_json::Value::is_string).then(|| {
                        values
                            .iter()
                            .filter_map(|value| first_line(value.as_str()))
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                })
            }
            "web_fetch" | "fetch" | "open" => value(&["url"]).and_then(|raw| preview_url(&raw)),
            _ => None,
        });
        let title = first_line(row_title)
            .or_else(|| first_line(Some(fallback)))
            .unwrap_or_else(|| label.into());
        let text = detail.map_or(title, |value| format!("{label} · {value}"));
        let mut chars = text.graphemes(true);
        let prefix = chars.by_ref().take(95).collect::<String>();
        match (chars.next(), chars.next()) {
            (Some(last), None) => prefix + last,
            (Some(_), Some(_)) => prefix + "…",
            (None, _) => prefix,
        }
    }
}
fn preview_url(raw: &str) -> Option<String> {
    let path = |url: &url::Url| {
        percent_encoding::percent_decode_str(url.path())
            .decode_utf8_lossy()
            .into_owned()
    };
    if let Ok(url) = url::Url::parse(raw) {
        return Some(format!("{}{}", url.host_str().unwrap_or(""), path(&url)));
    }
    let base = url::Url::parse("https://relative.invalid/").ok()?;
    let url = base.join(raw).ok()?;
    if raw.starts_with("//") {
        Some(format!("{}{}", url.host_str().unwrap_or(""), path(&url)))
    } else if raw.starts_with('/') {
        Some(path(&url))
    } else {
        let path = path(&url);
        Some(path.strip_prefix('/').unwrap_or(&path).into())
    }
}
