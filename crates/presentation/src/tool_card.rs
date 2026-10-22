//! Typed expansion facts matching ClientKit's ToolCardModel; no renderer or file access.
use crate::TranscriptToolCall;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::OnceLock;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptToolSearchMatch {
    pub line_number: i64,
    pub line: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptToolSearchFile {
    pub path: String,
    pub matches: Vec<TranscriptToolSearchMatch>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "items", rename_all = "camelCase")]
pub enum TranscriptToolSearch {
    Matches(Vec<TranscriptToolSearchFile>),
    Paths(Vec<String>),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptToolWebSource {
    pub url: String,
    pub title: Option<String>,
    pub snippet: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TranscriptToolWeb {
    Search {
        sources: Vec<TranscriptToolWebSource>,
        answer: Option<String>,
    },
    Fetch {
        url: String,
        #[serde(rename = "statusCode")]
        status_code: i64,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptToolTerminal {
    pub command: String,
    pub description: Option<String>,
    pub workdir: Option<String>,
    pub output: Option<String>,
    pub exit_code: Option<i64>,
    pub signal: Option<String>,
    pub running: bool,
}
impl TranscriptToolTerminal {
    pub fn exit_failed(&self) -> bool {
        !self.running && (self.exit_code.is_some_and(|code| code != 0) || self.signal.is_some())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptToolDiffHunk {
    pub path: String,
    pub old_text: Option<String>,
    pub new_text: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptToolReadLine {
    pub number: i64,
    pub text: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptToolRead {
    pub path: String,
    pub offset: i64,
    pub lines: Vec<TranscriptToolReadLine>,
    pub total_lines: i64,
    pub lang: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TranscriptToolCardModel {
    Search {
        result: TranscriptToolSearch,
        truncated: bool,
        total: i64,
    },
    Web {
        result: TranscriptToolWeb,
        truncated: bool,
    },
    Terminal(TranscriptToolTerminal),
    Diff {
        hunks: Vec<TranscriptToolDiffHunk>,
    },
    Read(TranscriptToolRead),
}
impl TranscriptToolCardModel {
    pub fn exit_failed(&self) -> bool {
        matches!(self, Self::Terminal(terminal) if terminal.exit_failed())
    }
}
impl TranscriptToolCall {
    /// Textual public result content only. Media stays in the original message;
    /// this method does not create Markdown references or load attachments.
    pub fn presentation_result_text(&self) -> Option<String> {
        let result = self.result.as_ref()?;
        let text = if let Some(parts) = result["content"].as_array() {
            parts
                .iter()
                .filter_map(|part| part["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            result["text"].as_str().unwrap_or("").into()
        };
        if !text.is_empty() {
            return Some(text);
        }
        let error = self.result_error.as_ref()?;
        Some(format!(
            "{}: {}",
            error["name"].as_str()?,
            error["code"].as_str()?
        ))
    }
    pub(crate) fn explicit_tool_error(&self) -> bool {
        self.result_error.as_ref().is_some_and(Value::is_object)
            || self.result.as_ref().is_some_and(|result| {
                result["isError"] == true
                    || result["content"]
                        .as_array()
                        .is_some_and(|parts| parts.iter().any(|part| part["isError"] == true))
            })
    }
    pub(crate) fn failed_for_presentation(&self) -> bool {
        !matches!(self.phase.as_str(), "stopped" | "interrupted")
            && (self.phase == "error" || self.explicit_tool_error())
    }
    pub(crate) fn legacy_terminal_exit_failed(&self) -> bool {
        if !matches!(self.name.split('.').next(), Some("shell" | "bash" | "pwsh")) {
            return false;
        }
        self.presentation_result_text().is_some_and(|result| {
            let (_, code, signal) = parse_exit(&result);
            signal.is_some() || code.is_some_and(|code| code != 0)
        })
    }
    /// Derive only for an expanded disclosure. Unknown/malformed shapes return
    /// None and leave the original input/result disclosure authoritative.
    /// Meta fields narrow only for the documented ClientKit tool family.
    pub fn presentation_card(&self) -> Option<TranscriptToolCardModel> {
        let decoded = self
            .arguments
            .as_str()
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok());
        let args = decoded.as_ref().unwrap_or(&self.arguments);
        args.as_object()?;
        let meta = self.result_meta.as_ref().unwrap_or(&Value::Null);
        let settled = matches!(
            self.phase.as_str(),
            "done" | "error" | "stopped" | "interrupted"
        );
        let failed = self.failed_for_presentation();
        let family = self.name.split('.').next().unwrap_or(&self.name);
        match family {
            "shell" | "bash" | "pwsh" => terminal(
                args,
                settled,
                failed,
                self.presentation_result_text().as_deref(),
            )
            .map(TranscriptToolCardModel::Terminal),
            "grep" | "glob" if settled && !failed => search(family, args, meta),
            "web_search" | "web_fetch" if settled && !failed => web(family, args, meta),
            "read" if settled && !failed => read(args, meta).map(TranscriptToolCardModel::Read),
            "write" | "edit" | "str_replace_editor" => diff(family, args, settled, failed, meta)
                .map(|hunks| TranscriptToolCardModel::Diff { hunks }),
            _ => None,
        }
    }
}
fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key)?.as_str()
}
fn nonblank(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
}
fn optional_text(value: &Value, key: &str) -> Option<Option<String>> {
    match value.get(key) {
        None => Some(None),
        Some(value) => value.as_str().map(|s| Some(s.into())),
    }
}
// NSNumber.intValue differs from the SEP JSON Double -> Int(exactly:)
// reader. Preserve signed/unsigned integer conversion and saturating finite
// floating conversion; integer-only meta additionally rejects bool/fractions.
fn number_int_value(value: &Value) -> Option<i64> {
    value
        .as_bool()
        .map(i64::from)
        .or_else(|| value.as_i64())
        .or_else(|| value.as_u64().map(|number| number as i64))
        .or_else(|| {
            value
                .as_f64()
                .filter(|number| number.is_finite())
                .map(|number| number as i64)
        })
}
fn integer(value: &Value) -> Option<i64> {
    if value.is_boolean() {
        return None;
    }
    let number = value.as_f64()?;
    (number.is_finite() && number.trunc() == number)
        .then(|| number_int_value(value))
        .flatten()
}
fn valid_escalation(args: &Value) -> bool {
    if args.get("sandbox_permissions").is_none() && args.get("justification").is_none() {
        return true;
    }
    matches!(
        text(args, "sandbox_permissions"),
        Some("workspace-write" | "danger-full-access")
    ) && nonblank(text(args, "justification")).is_some()
}
fn terminal(
    args: &Value,
    settled: bool,
    failed: bool,
    result: Option<&str>,
) -> Option<TranscriptToolTerminal> {
    let command = nonblank(text(args, "command").or_else(|| text(args, "script")))?;
    let background = match args.get("run_in_background") {
        None => false,
        Some(value) => value.as_bool()?,
    };
    let workdir = optional_text(args, "workdir")?;
    if !valid_escalation(args) {
        return None;
    }
    let description = if args.get("description").is_some() {
        Some(nonblank(text(args, "description"))?.into())
    } else {
        None
    };
    let persistent = description.is_none();
    let mut card = TranscriptToolTerminal {
        command: command.into(),
        description,
        workdir: if persistent { None } else { workdir },
        output: None,
        exit_code: None,
        signal: None,
        running: !settled,
    };
    if !settled {
        return Some(card);
    }
    if failed || persistent {
        return None;
    }
    let result = result?;
    if background {
        card.output = Some(result.into());
        return Some(card);
    }
    if let Ok(object) = serde_json::from_str::<Value>(result)
        && let Some(output) = text(&object, "output")
    {
        card.output = Some(output.into());
        card.signal = text(&object, "signal").map(String::from);
        // ClientKit's structured terminal uses NSNumber.intValue (including bool
        // and fractional truncation), unlike its strict integer meta readers.
        card.exit_code = card
            .signal
            .is_none()
            .then(|| number_int_value(&object["exitCode"]).unwrap_or(0));
        return Some(card);
    }
    let (output, code, signal) = parse_exit(result);
    card.output = Some(output);
    card.exit_code = code;
    card.signal = signal;
    Some(card)
}
fn parse_exit(value: &str) -> (String, Option<i64>, Option<String>) {
    static SIGNAL: OnceLock<Regex> = OnceLock::new();
    static CODE: OnceLock<Regex> = OnceLock::new();
    // ICU's end anchor also accepts a final line terminator; retain the same
    // terminal marker boundary rather than searching arbitrary output lines.
    let signal = SIGNAL.get_or_init(|| {
        Regex::new(r"\n\[killed by signal: ([^\]\n]+)\](?:\r\n|[\n\r\x{85}\x{2028}\x{2029}])?\z")
            .expect("static signal regex")
    });
    if let Some(capture) = signal.captures(value) {
        return (
            value[..capture.get(0).unwrap().start()].into(),
            None,
            Some(capture[1].into()),
        );
    }
    let code = CODE.get_or_init(|| {
        Regex::new(r"\n\[exit code: (\d+)\](?:\r\n|[\n\r\x{85}\x{2028}\x{2029}])?\z")
            .expect("static code regex")
    });
    if let Some(capture) = code.captures(value) {
        return (
            value[..capture.get(0).unwrap().start()].into(),
            Some(capture[1].parse().unwrap_or(0)),
            None,
        );
    }
    (value.into(), Some(0), None)
}
fn trim_spaces(value: &str) -> &str {
    value.trim_matches(|ch: char| {
        ch.is_whitespace()
            && !matches!(
                ch,
                '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{85}' | '\u{2028}' | '\u{2029}'
            )
    })
}
fn search(tool: &str, args: &Value, meta: &Value) -> Option<TranscriptToolCardModel> {
    let pattern = text(args, "pattern")?;
    if (tool == "grep" && pattern.is_empty()) || (tool == "glob" && trim_spaces(pattern).is_empty())
    {
        return None;
    }
    if args.get("path").is_some() {
        nonblank(text(args, "path"))?;
    }
    if tool == "grep" && args.get("include").is_some() {
        let include = text(args, "include")?;
        if trim_spaces(include).is_empty() || include.starts_with('!') {
            return None;
        }
        let mut depth = 0usize;
        for ch in include.chars() {
            match ch {
                '{' => depth += 1,
                '}' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => return None,
                _ => {}
            }
        }
    }
    let truncated = meta["truncated"].as_bool()?;
    let total = integer(&meta["total"])?;
    if total < 0 {
        return None;
    }
    let result = if tool == "grep" {
        if meta["shape"] != "matches" {
            return None;
        }
        let files = meta["files"]
            .as_array()?
            .iter()
            .map(|file| {
                let path = text(file, "path")?.into();
                let matches = file["matches"]
                    .as_array()?
                    .iter()
                    .map(|m| {
                        let line_number = integer(&m["lineNumber"])?;
                        (line_number >= 1).then(|| {
                            Some(TranscriptToolSearchMatch {
                                line_number,
                                line: text(m, "line")?.into(),
                            })
                        })?
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(TranscriptToolSearchFile { path, matches })
            })
            .collect::<Option<Vec<_>>>()?;
        TranscriptToolSearch::Matches(files)
    } else {
        if meta["shape"] != "paths" {
            return None;
        }
        TranscriptToolSearch::Paths(
            meta["paths"]
                .as_array()?
                .iter()
                .map(|path| path.as_str().map(String::from))
                .collect::<Option<Vec<_>>>()?,
        )
    };
    Some(TranscriptToolCardModel::Search {
        result,
        truncated,
        total,
    })
}
fn web(tool: &str, args: &Value, meta: &Value) -> Option<TranscriptToolCardModel> {
    let truncated = meta["truncated"].as_bool()?;
    let result = if tool == "web_search" {
        let queries = args["queries"].as_array()?;
        if queries.is_empty()
            || queries
                .iter()
                .any(|query| nonblank(query.as_str()).is_none())
        {
            return None;
        }
        let sources = meta["sources"]
            .as_array()?
            .iter()
            .map(|source| {
                Some(TranscriptToolWebSource {
                    url: text(source, "url")?.into(),
                    title: optional_text(source, "title")?,
                    snippet: optional_text(source, "snippet")?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        TranscriptToolWeb::Search {
            sources,
            answer: optional_text(meta, "answer")?,
        }
    } else {
        nonblank(text(args, "url"))?;
        TranscriptToolWeb::Fetch {
            url: text(meta, "url")?.into(),
            status_code: integer(&meta["statusCode"])?,
        }
    };
    Some(TranscriptToolCardModel::Web { result, truncated })
}
fn read(args: &Value, meta: &Value) -> Option<TranscriptToolRead> {
    nonblank(text(args, "file_path").or_else(|| text(args, "path")))?;
    for key in ["offset", "limit"] {
        if let Some(value) = args.get(key)
            && integer(value)? < 1
        {
            return None;
        }
    }
    let offset = integer(&meta["offset"])?;
    let total_lines = integer(&meta["totalLines"])?;
    if offset < 1 || total_lines < 0 {
        return None;
    }
    let mut previous = offset - 1;
    let lines = meta["lines"]
        .as_array()?
        .iter()
        .map(|line| {
            let number = integer(&line["number"])?;
            if number < 1 || number <= previous || number > total_lines {
                return None;
            }
            previous = number;
            Some(TranscriptToolReadLine {
                number,
                text: text(line, "text")?.into(),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(TranscriptToolRead {
        path: text(meta, "path")?.into(),
        offset,
        total_lines,
        lines,
        lang: optional_text(meta, "lang")?,
    })
}
fn diff(
    tool: &str,
    args: &Value,
    settled: bool,
    failed: bool,
    meta: &Value,
) -> Option<Vec<TranscriptToolDiffHunk>> {
    let intended = intended_diff(tool, args)?;
    if !settled {
        return Some(vec![intended]);
    }
    if tool == "str_replace_editor" || failed {
        return None;
    }
    let Some(diffs) = meta["diffs"].as_array() else {
        return (tool == "write").then_some(vec![intended]);
    };
    if diffs.is_empty() {
        return (tool == "write").then_some(vec![intended]);
    }
    diffs
        .iter()
        .map(|diff| {
            Some(TranscriptToolDiffHunk {
                path: text(diff, "path")?.into(),
                new_text: text(diff, "newText")?.into(),
                old_text: match diff.get("oldText") {
                    None | Some(Value::Null) => None,
                    Some(value) => Some(value.as_str()?.into()),
                },
            })
        })
        .collect()
}
fn intended_diff(tool: &str, args: &Value) -> Option<TranscriptToolDiffHunk> {
    if tool == "str_replace_editor" {
        let path = nonblank(text(args, "path"))?.into();
        return match text(args, "command")? {
            "create" => Some(TranscriptToolDiffHunk {
                path,
                old_text: None,
                new_text: optional_text(args, "file_text")?.unwrap_or_default(),
            }),
            "str_replace" => Some(TranscriptToolDiffHunk {
                path,
                old_text: optional_text(args, "old_str")?,
                new_text: optional_text(args, "new_str")?.unwrap_or_default(),
            }),
            _ => None,
        };
    }
    let path = nonblank(text(args, "file_path").or_else(|| text(args, "path")))?;
    if !valid_escalation(args) {
        return None;
    }
    let (old_text, new_text) = if tool == "write" {
        (None, text(args, "content")?.into())
    } else {
        if args
            .get("replace_all")
            .is_some_and(|value| !value.is_boolean())
        {
            return None;
        }
        (
            text(args, "old_string")
                .filter(|value| !value.is_empty())
                .map(String::from),
            text(args, "new_string")?.into(),
        )
    };
    if tool == "edit" {
        text(args, "old_string")?;
    }
    Some(TranscriptToolDiffHunk {
        path: path.into(),
        old_text,
        new_text,
    })
}
