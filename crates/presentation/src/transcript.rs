use crate::transcript_content::{ContentDialect, content_blocks};
use crate::{RunScope, TranscriptContentBlock, TranscriptSourcePosition};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use tekes_agent_contract::{Error, Result, require};
use tekes_agent_session::SessionProjection;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptTurn {
    pub id: String,
    pub status: String,
    pub input: String,
    /// Source Unix milliseconds; consumers convert only for presentation.
    pub started_at: Option<f64>,
    /// Source Unix milliseconds, absent until a public settlement supplies it.
    pub ended_at: Option<f64>,
    pub usage: TokenUsage,
    /// Exact public endpoint boundary, admitted only from a durable settlement record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork_boundary: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub footnote_actions: Vec<tekes_agent_contract::ThreadActionDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_usage: Option<tekes_agent_contract::TranscriptLiveUsage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reported_usage: Option<tekes_agent_contract::TranscriptReportedUsage>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TranscriptItemKind {
    User,
    Assistant,
    System,
    Error,
    Notice,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptItem {
    pub id: String,
    pub record_id: Option<String>,
    pub reconciliation_id: Option<String>,
    pub turn_id: String,
    pub kind: TranscriptItemKind,
    pub text: String,
    pub reasoning: String,
    pub attachments: Vec<Value>,
    /// None means this source exposes only aggregate/legacy content, not proven block order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_blocks: Option<Vec<TranscriptContentBlock>>,
    pub is_visible: bool,
    pub is_final: bool,
    pub is_provisional: bool,
    pub reasoning_ended: bool,
    pub source_sequence: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_position: Option<TranscriptSourcePosition>,
    pub time: Option<f64>,
    pub metadata: Value,
}
impl TranscriptItem {
    /// Explicit committed failure text, never inferred from prose or an idle run.
    pub fn failure_message(&self) -> Option<&str> {
        if !self.is_visible || self.is_provisional {
            return None;
        }
        let message = match self.kind {
            TranscriptItemKind::Error => Some(self.text.as_str()),
            TranscriptItemKind::Assistant if self.metadata["stopReason"] == "error" => {
                self.metadata["errorMessage"].as_str()
            }
            _ => None,
        };
        message.filter(|text| !text.trim().is_empty())
    }
}
/// The latest source turn explicitly failed. A missing message remains unknown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptFailure<'a> {
    pub turn_id: &'a str,
    pub message: Option<&'a str>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptToolCall {
    pub id: String,
    pub turn_id: String,
    pub name: String,
    pub arguments: Value,
    pub result: Option<Value>,
    /// Original public tool/result metadata, not inferred from output text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_meta: Option<Value>,
    /// Original event error field, including unknown shapes. Only a structured
    /// object contributes to the canonical failure/stopped classification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_error: Option<Value>,
    pub phase: String,
    pub call_view: Option<Value>,
    pub result_view: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_position: Option<TranscriptSourcePosition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_position: Option<TranscriptSourcePosition>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSnapshot {
    pub turns: Vec<TranscriptTurn>,
    pub items: Vec<TranscriptItem>,
    pub tools: Vec<TranscriptToolCall>,
    pub has_more_before: bool,
    pub is_stale: bool,
    pub diagnostics: Vec<String>,
}
pub(crate) fn scoped(scope: &RunScope, suffix: &str) -> String {
    json!([scope.endpoint_id, scope.session_id, suffix]).to_string()
}
fn text_blocks(value: &Value, kind: &str) -> String {
    value
        .as_array()
        .map(|blocks| {
            blocks
                .iter()
                .filter(|v| v["type"] == kind)
                .filter_map(|v| {
                    v[if kind == "thinking" {
                        "thinking"
                    } else {
                        "text"
                    }]
                    .as_str()
                })
                .collect::<String>()
        })
        .unwrap_or_default()
}
fn body(value: &Value) -> String {
    value
        .as_str()
        .map(String::from)
        .unwrap_or_else(|| text_blocks(value, "text"))
}
fn attachments(content: &Value) -> Vec<Value> {
    content
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|v| matches!(v["type"].as_str(), Some("image" | "file" | "audio")))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}
fn usage(value: &Value) -> TokenUsage {
    TokenUsage {
        input_tokens: value["inputTokens"]
            .as_u64()
            .or_else(|| value["input"].as_u64())
            .unwrap_or(0),
        output_tokens: value["outputTokens"]
            .as_u64()
            .or_else(|| value["output"].as_u64())
            .unwrap_or(0),
    }
}
fn output_count(value: &Value) -> Option<u64> {
    value["outputTokens"]
        .as_u64()
        .or_else(|| value["output"].as_u64())
}
/// DSH exposes the same authoritative usage in completed assistant streams and messages.
fn response_usage(data: &Value) -> &Value {
    if data["usage"].is_object() {
        return &data["usage"];
    }
    data["stream"]
        .as_array()
        .and_then(|records| {
            records.iter().rev().find_map(|record| {
                (record["type"] == "chunk" && record["chunk"]["type"] == "usage")
                    .then_some(&record["chunk"]["usage"])
            })
        })
        .unwrap_or(&Value::Null)
}
impl TranscriptSnapshot {
    /// Current canonical settlement only: old failures remain historical after
    /// a newer running/completed/aborted turn, without clearing their records.
    pub fn latest_failure(&self) -> Option<TranscriptFailure<'_>> {
        let turn = self.turns.last().filter(|turn| turn.status == "error")?;
        Some(TranscriptFailure {
            turn_id: &turn.id,
            message: self
                .items
                .iter()
                .rev()
                .filter(|item| item.turn_id == turn.id)
                .find_map(TranscriptItem::failure_message),
        })
    }
    /// Replay already validated authority. Rebuilds only this selected session; never advances a cursor.
    pub fn read(scope: &RunScope, session: &SessionProjection) -> Result<Self> {
        let mut snapshot = if session.native_conversation.is_some() {
            native(scope, session)?
        } else {
            journal(scope, session)?
        };
        snapshot.has_more_before = session
            .native_conversation
            .as_ref()
            .map(|v| v.has_more_before)
            .unwrap_or(session.has_more_before);
        snapshot.is_stale = !session.journal_ready;
        Ok(snapshot)
    }
    pub fn reconciliation_ids(&self) -> impl Iterator<Item = &str> {
        self.items
            .iter()
            .filter(|item| item.kind == TranscriptItemKind::User && !item.is_provisional)
            .filter_map(|item| item.reconciliation_id.as_deref())
    }
}
fn turn<'a>(
    out: &'a mut TranscriptSnapshot,
    id: &str,
    time: Option<f64>,
) -> &'a mut TranscriptTurn {
    if !out.turns.iter().any(|t| t.id == id) {
        out.turns.push(TranscriptTurn {
            id: id.into(),
            status: "running".into(),
            input: String::new(),
            started_at: time,
            ended_at: None,
            usage: TokenUsage::default(),
            fork_boundary: None,
            footnote_actions: Vec::new(),
            live_usage: None,
            reported_usage: None,
        });
    }
    out.turns
        .iter_mut()
        .find(|t| t.id == id)
        .expect("inserted turn")
}
fn item(
    scope: &RunScope,
    key: &str,
    turn: &str,
    kind: TranscriptItemKind,
    seq: Option<i64>,
    time: Option<f64>,
) -> TranscriptItem {
    TranscriptItem {
        id: scoped(scope, key),
        record_id: None,
        reconciliation_id: None,
        turn_id: turn.into(),
        kind,
        text: String::new(),
        reasoning: String::new(),
        attachments: Vec::new(),
        content_blocks: None,
        is_visible: true,
        is_final: false,
        is_provisional: false,
        reasoning_ended: true,
        source_sequence: seq,
        source_position: seq.map(|sequence| TranscriptSourcePosition::Journal {
            sequence,
            content_index: None,
        }),
        time,
        metadata: Value::Null,
    }
}
fn upsert(out: &mut TranscriptSnapshot, value: TranscriptItem) {
    if let Some(old) = out.items.iter_mut().find(|old| old.id == value.id) {
        *old = value;
    } else {
        out.items.push(value);
    }
}
fn settle(
    out: &mut TranscriptSnapshot,
    id: &str,
    reason: &str,
    time: Option<f64>,
    promoted: Option<&str>,
) -> Result<()> {
    turn(out, id, time).status = reason.into();
    turn(out, id, time).ended_at = time;
    for tool in out
        .tools
        .iter_mut()
        .filter(|t| t.turn_id == id && t.phase == "doing")
    {
        tool.phase = "stopped".into();
    }
    out.items.retain(|i| i.turn_id != id || !i.is_provisional);
    if reason == "aborted"
        && let Some(index) = out
            .items
            .iter()
            .rposition(|i| i.turn_id == id && i.kind == TranscriptItemKind::Error)
        && out.items[index].metadata["classification"] == "transport"
    {
        out.items.remove(index);
    }
    if reason == "completed" {
        let eligible = |i: &&TranscriptItem| {
            i.turn_id == id
                && i.kind == TranscriptItemKind::Assistant
                && i.metadata["sessionFinal"] != false
                && i.metadata["hasToolCalls"] != true
                && i.metadata["stopReason"] != "toolUse"
        };
        let candidate = if let Some(promoted) = promoted {
            let candidate = out
                .items
                .iter()
                .filter(eligible)
                .find(|i| i.record_id.as_deref() == Some(promoted));
            require(
                candidate.is_some(),
                "promoted assistant is outside loaded history",
            )?;
            candidate.map(|i| i.id.clone())
        } else {
            out.items
                .iter()
                .rev()
                .filter(eligible)
                .find(|i| i.metadata["sessionFinal"] == true)
                .or_else(|| {
                    out.items.iter().rev().find(|i| {
                        i.turn_id == id
                            && i.kind == TranscriptItemKind::Assistant
                            && i.metadata["sessionFinal"] != false
                            && i.metadata["hasToolCalls"] != true
                            && i.metadata["stopReason"] != "toolUse"
                    })
                })
                .map(|i| i.id.clone())
        };
        if let Some(candidate) = candidate
            && let Some(item) = out.items.iter_mut().find(|i| i.id == candidate)
        {
            item.is_final = true;
        }
    }
    Ok(())
}
fn journal(scope: &RunScope, session: &SessionProjection) -> Result<TranscriptSnapshot> {
    let mut out = TranscriptSnapshot::default();
    let mut current = 0;
    let mut message_usage = BTreeMap::<(String, String), TokenUsage>::new();
    let mut total_usage = BTreeMap::new();
    let mut reported_output = BTreeMap::new();
    let mut opened_turns = std::collections::BTreeSet::new();
    let mut authoritative_output = BTreeMap::new();
    let mut opening_inputs = std::collections::BTreeSet::new();
    for entry in session.entries.values() {
        let event = &entry.event;
        let data = &event.data;
        let number = data["turn"].as_i64().unwrap_or(current);
        let turn_id = scoped(scope, &format!("turn:{number}"));
        let step = data["step"].as_i64().unwrap_or(0);
        let seq = Some(event.seq);
        let time = Some(event.time);
        // Account independently of surface visibility: model-only messages still consumed tokens.
        if matches!(
            event.kind.as_str(),
            "assistant/message" | "assistant/attempt"
        ) {
            let key = (
                turn_id.clone(),
                data["message"]["id"]
                    .as_str()
                    .map(String::from)
                    .unwrap_or_else(|| format!("{}:{}", event.kind, event.seq)),
            );
            let value = response_usage(data);
            reported_output.insert(key.clone(), output_count(value).is_some());
            message_usage.insert(key, usage(value));
            turn(&mut out, &turn_id, time);
        }
        match event.kind.as_str() {
            "turn/start" => {
                // A durable turn start proves the worker is running. Derive
                // recovery on replay too, including notices from older Kernels.
                for row in &mut out.items {
                    if row.kind == TranscriptItemKind::Notice
                        && row.metadata["classification"] == "worker_launch"
                        && row.metadata["status"] == "error"
                    {
                        row.metadata["status"] = json!("recovered");
                        row.metadata["recoveredAtSequence"] = json!(event.seq);
                        row.metadata["recoveredAt"] = json!(time);
                    }
                }
                current = number;
                opened_turns.insert(turn_id.clone());
                turn(&mut out, &turn_id, time);
            }
            "user/message" | "assistant/message" | "system/message" | "developer/message"
            | "response/error" => {
                if entry.is_model_only == Some(true)
                    || event
                        .metadata
                        .get("surfaceOp")
                        .is_some_and(|v| v.get("startSeq").is_some())
                {
                    continue;
                }
                if let Some(op) = event.metadata.get("surfaceOp").filter(|v| v.is_object()) {
                    let (start, end) = (
                        op["start"].as_i64().unwrap_or(-1),
                        op["end"].as_i64().unwrap_or(-1),
                    );
                    require(
                        out.items.iter().any(|i| i.source_sequence == Some(start))
                            && out.items.iter().any(|i| i.source_sequence == Some(end)),
                        "surface replacement is outside loaded history",
                    )?;
                    out.items
                        .retain(|i| !i.source_sequence.is_some_and(|n| n >= start && n <= end));
                }
                turn(&mut out, &turn_id, time);
                let assistant = event.kind == "assistant/message";
                let key = if assistant {
                    format!("response:{number}:{step}")
                } else {
                    format!("surface:{}", event.seq)
                };
                let kind = match event.kind.as_str() {
                    "user/message" => TranscriptItemKind::User,
                    "assistant/message" => TranscriptItemKind::Assistant,
                    "response/error" => TranscriptItemKind::Error,
                    _ => TranscriptItemKind::System,
                };
                let mut row = item(scope, &key, &turn_id, kind, seq, time);
                row.metadata = json!({"sessionFinal":data["sessionFinal"],"hasToolCalls":data["message"]["content"].as_array().is_some_and(|a|a.iter().any(|b|b["type"]=="tool-call")),"classification":data["classification"],"source":data["source"],"sourceEventSeqs":event.metadata.get("sourceEventSeqs")});
                if assistant {
                    // Preserve a proven public feedback ID separately from the
                    // legacy record_id fallback used only for presentation.
                    row.metadata["feedbackMessageId"] = data["message"]["id"]
                        .as_str()
                        .filter(|id| !id.is_empty())
                        .map_or(Value::Null, |id| json!(id));
                    row.text = body(&data["message"]["content"]);
                    row.reasoning = text_blocks(&data["message"]["content"], "reasoning");
                    row.attachments = attachments(&data["message"]["content"]);
                    row.content_blocks =
                        content_blocks(scope, &data["message"]["content"], ContentDialect::Journal);
                    row.record_id = Some(
                        data["message"]["id"]
                            .as_str()
                            .map(String::from)
                            .unwrap_or_else(|| format!("assistant-{}", event.seq)),
                    );
                    if let Some(blocks) = data["message"]["content"].as_array() {
                        for (index, block) in blocks
                            .iter()
                            .enumerate()
                            .filter(|(_, b)| b["type"] == "tool-call")
                        {
                            admit_tool(
                                &mut out,
                                scope,
                                &turn_id,
                                block["id"].as_str().unwrap_or(""),
                                block["name"].as_str().unwrap_or(""),
                                block["arguments"].clone(),
                                ToolSource {
                                    view: None,
                                    position: Some(TranscriptSourcePosition::Journal {
                                        sequence: event.seq,
                                        content_index: Some(index),
                                    }),
                                },
                            );
                        }
                    }
                } else if event.kind == "user/message" {
                    let authored = data["source"]["kind"]
                        .as_str()
                        .is_none_or(|kind| kind == "user");
                    row.text = data["text"]
                        .as_str()
                        .map(String::from)
                        .unwrap_or_else(|| body(&data["content"]));
                    row.record_id = data["id"].as_str().map(String::from);
                    row.reconciliation_id = data["source"]["rpcId"]
                        .as_str()
                        .or_else(|| data["submission"]["rpcId"].as_str())
                        .map(String::from);
                    row.attachments = attachments(&data["content"]);
                    let source = if data["content"].is_null() {
                        &data["text"]
                    } else {
                        &data["content"]
                    };
                    // Conflicting legacy display text and structured content cannot prove one order.
                    if data["text"].as_str().is_none_or(|text| {
                        data["content"].is_null() || text == body(&data["content"])
                    }) {
                        row.content_blocks = content_blocks(scope, source, ContentDialect::Journal);
                    }
                    if !authored {
                        row.metadata["hidden"] = json!(true);
                        row.is_visible = false;
                    }
                    if authored
                        && data["source"]["mode"] != "steer"
                        && opening_inputs.insert(turn_id.clone())
                    {
                        turn(&mut out, &turn_id, time).input = row.text.clone();
                    }
                } else if event.kind == "response/error" {
                    row.text = data["message"].as_str().unwrap_or("").into();
                    row.content_blocks =
                        content_blocks(scope, &data["message"], ContentDialect::Journal);
                } else {
                    row.text = body(&data["message"]["content"]);
                    row.content_blocks =
                        content_blocks(scope, &data["message"]["content"], ContentDialect::Journal);
                    row.metadata["hidden"] = json!(true);
                    row.is_visible = false;
                }
                upsert(&mut out, row);
            }
            "tool/call" => {
                admit_tool(
                    &mut out,
                    scope,
                    &turn_id,
                    data["callId"].as_str().unwrap_or(""),
                    data["name"].as_str().unwrap_or(""),
                    data["arguments"].clone(),
                    ToolSource {
                        view: entry.view.clone(),
                        position: Some(TranscriptSourcePosition::Journal {
                            sequence: event.seq,
                            content_index: None,
                        }),
                    },
                );
            }
            "tool/result" => {
                let message = &data["message"];
                let first = &message["content"][0];
                let call = message["source"]["callId"]
                    .as_str()
                    .or_else(|| first["toolCallId"].as_str())
                    .or_else(|| first["callId"].as_str())
                    .unwrap_or("");
                admit_tool(
                    &mut out,
                    scope,
                    &turn_id,
                    call,
                    call,
                    Value::Null,
                    ToolSource::default(),
                );
                let tool = out
                    .tools
                    .iter_mut()
                    .find(|t| t.id == scoped(scope, &format!("tool:{call}")))
                    .expect("admitted");
                tool.result = Some(message.clone());
                tool.result_meta = data.get("meta").filter(|value| !value.is_null()).cloned();
                tool.result_error = data.get("error").filter(|value| !value.is_null()).cloned();
                tool.result_view = entry.view.clone();
                tool.result_position = Some(TranscriptSourcePosition::Journal {
                    sequence: event.seq,
                    content_index: None,
                });
                tool.phase = crate::tool_view::result_phase(data, entry.view.as_ref()).into();
            }
            "turn/end" => {
                if let Some(count) = output_count(&data["usage"]) {
                    authoritative_output.insert(turn_id.clone(), count);
                }
                let reason = data["reason"]
                    .as_str()
                    .or_else(|| data["reason"]["kind"].as_str())
                    .unwrap_or("completed");
                if data["reason"].is_null() {
                    out.diagnostics
                        .push(format!("turn/end {} omitted reason", event.seq));
                }
                if data["usage"]["inputTokens"].as_u64().is_some()
                    && data["usage"]["outputTokens"].as_u64().is_some()
                {
                    total_usage.insert(turn_id.clone(), usage(&data["usage"]));
                }
                settle(
                    &mut out,
                    &turn_id,
                    reason,
                    time,
                    data["promotedMessageID"].as_str(),
                )?;
                turn(&mut out, &turn_id, time).fork_boundary = Some(json!({"atSeq":event.seq}));
                if reason == "error"
                    && data["reason"]["error"]["code"] != "AUTH"
                    && let Some(error) = data["reason"]["error"]["message"].as_str()
                {
                    let mut row = item(
                        scope,
                        &format!("turn-error:{}", event.seq),
                        &turn_id,
                        TranscriptItemKind::Error,
                        seq,
                        time,
                    );
                    row.text = error.into();
                    upsert(&mut out, row);
                }
            }
            "session/notice" => {
                let mut row = item(
                    scope,
                    &format!("notice:{}", event.seq),
                    "",
                    TranscriptItemKind::Notice,
                    seq,
                    time,
                );
                row.text = data["message"].as_str().unwrap_or("").into();
                row.content_blocks =
                    content_blocks(scope, &data["message"], ContentDialect::Journal);
                row.metadata = json!({"classification":data["classification"],
                    "operation":data["operation"], "status":data["severity"]});
                out.items.push(row);
            }
            _ => {}
        }
    }
    for turn in &mut out.turns {
        turn.usage = total_usage.get(&turn.id).cloned().unwrap_or_else(|| {
            message_usage
                .iter()
                .filter(|((id, _), _)| id == &turn.id)
                .fold(TokenUsage::default(), |mut a, (_, b)| {
                    a.input_tokens = a.input_tokens.saturating_add(b.input_tokens);
                    a.output_tokens = a.output_tokens.saturating_add(b.output_tokens);
                    a
                })
        });
    }
    let output_known = |id: &str| {
        reported_output
            .iter()
            .filter(|((turn, _), _)| turn == id)
            .all(|(_, reported)| *reported)
    };
    for turn in &mut out.turns {
        if let Some(count) = authoritative_output.get(&turn.id) {
            turn.reported_usage = Some(tekes_agent_contract::TranscriptReportedUsage {
                output_tokens: *count,
                is_complete: true,
            });
        } else {
            let known = message_usage.iter().filter(|((id, record), _)| {
                id == &turn.id && reported_output.get(&(id.clone(), record.clone())) == Some(&true)
            });
            let mut reports = known.peekable();
            if reports.peek().is_some() {
                let count = reports.fold(0u64, |total, (_, usage)| {
                    total.saturating_add(usage.output_tokens)
                });
                turn.reported_usage = Some(tekes_agent_contract::TranscriptReportedUsage {
                    output_tokens: count,
                    is_complete: opened_turns.contains(&turn.id) && output_known(&turn.id),
                });
            }
        }
    }
    if session.run_open && session.journal_ready {
        for turn in &mut out.turns {
            if turn.status == "running"
                && output_known(&turn.id)
                && message_usage.keys().any(|(id, _)| id == &turn.id)
            {
                turn.live_usage = Some(tekes_agent_contract::TranscriptLiveUsage {
                    output_tokens: turn.usage.output_tokens,
                });
            }
        }
    }
    for (key, preview) in &session.previews {
        let Some((number, step)) = key.split_once(':') else {
            continue;
        };
        let turn_id = scoped(scope, &format!("turn:{number}"));
        if session.run_open
            && session.journal_ready
            && let Some(count) = preview.output_tokens
        {
            let turn = turn(&mut out, &turn_id, None);
            if turn.status == "running" && output_known(&turn_id) {
                let live =
                    turn.live_usage
                        .get_or_insert(tekes_agent_contract::TranscriptLiveUsage {
                            output_tokens: turn.usage.output_tokens,
                        });
                live.output_tokens = live.output_tokens.saturating_add(count);
            }
        }
        let mut row = item(
            scope,
            &format!("response:{number}:{step}"),
            &turn_id,
            TranscriptItemKind::Assistant,
            None,
            None,
        );
        row.text = preview.text.clone();
        row.reasoning = preview.reasoning.clone();
        row.reasoning_ended = preview.reasoning_ended;
        row.is_provisional = true;
        upsert(&mut out, row);
    }
    out.diagnostics.truncate(32);
    Ok(out)
}
#[derive(Default)]
struct ToolSource {
    view: Option<Value>,
    position: Option<TranscriptSourcePosition>,
}
fn admit_tool(
    out: &mut TranscriptSnapshot,
    scope: &RunScope,
    turn: &str,
    id: &str,
    name: &str,
    arguments: Value,
    source: ToolSource,
) {
    let ToolSource { view, position } = source;
    let key = scoped(scope, &format!("tool:{id}"));
    if let Some(tool) = out.tools.iter_mut().find(|t| t.id == key) {
        if tool.source_position.is_none() {
            tool.source_position = position;
        }
        if !arguments.is_null() {
            tool.arguments = arguments;
        }
        if view.is_some() {
            tool.call_view = view;
        }
        if !name.is_empty() && name != id && !(tool.name == "subagent" && name == "tekes_subagent")
        {
            tool.name = name.into();
        }
    } else {
        out.tools.push(TranscriptToolCall {
            result_meta: None,
            result_error: None,
            id: key,
            turn_id: turn.into(),
            name: name.into(),
            arguments,
            result: None,
            phase: "doing".into(),
            call_view: view,
            result_view: None,
            source_position: position,
            result_position: None,
        });
    }
}

fn native(scope: &RunScope, session: &SessionProjection) -> Result<TranscriptSnapshot> {
    let snapshot = session
        .native_conversation
        .as_ref()
        .ok_or_else(|| Error::Protocol("missing native snapshot".into()))?;
    snapshot.validate()?;
    tekes_agent_contract::validate_native_reading_identities(
        snapshot,
        &session.native_reading_identities,
    )?;
    let reading_ids: BTreeMap<_, _> = session
        .native_reading_identities
        .iter()
        .map(|identity| (identity.entry_id.as_str(), identity.stream_id.as_str()))
        .collect();
    let mut out = TranscriptSnapshot::default();
    let mut current = scoped(scope, "native:partial");
    let mut occurrences = BTreeMap::<String, usize>::new();
    let mut receipts = BTreeMap::<String, Vec<String>>::new();
    let mut running_marker = false;
    for entry in &snapshot.entries {
        if matches!(
            entry["customType"].as_str(),
            Some("agentkit.inputReceipt.v1" | "tekes.inputReceipt.v1")
        ) && let (Some(timestamp), Some(hash), Some(id)) = (
            entry["data"]["timestamp"].as_f64(),
            entry["data"]["hash"].as_str(),
            entry["data"]["rpcId"].as_str(),
        ) {
            receipts
                .entry(format!("{:x}:{hash}", timestamp.to_bits()))
                .or_default()
                .push(id.into());
        }
    }
    for (entry_ordinal, entry) in snapshot.entries.iter().enumerate() {
        if matches!(
            entry["customType"].as_str(),
            Some("agentkit.run.v1" | "tekes.run.v1")
        ) {
            let data = &entry["data"];
            if data["phase"] == "started" {
                current = scoped(
                    scope,
                    &format!("native-run:{}", data["id"].as_str().unwrap_or("partial")),
                );
                running_marker = true;
            } else if data["phase"] == "settled" {
                let reason = if data["aborted"] == true {
                    "aborted"
                } else if out
                    .items
                    .iter()
                    .rev()
                    .find(|i| i.turn_id == current && i.kind == TranscriptItemKind::Assistant)
                    .is_some_and(|i| i.metadata["stopReason"] == "error")
                {
                    "error"
                } else {
                    "completed"
                };
                settle(&mut out, &current, reason, data["timestamp"].as_f64(), None)?;
                if let Some(entry_id) = entry["id"].as_str() {
                    turn(&mut out, &current, None).fork_boundary =
                        Some(json!({"entryId":entry_id}));
                }
                running_marker = false;
            }
            continue;
        }
        if entry["type"] != "message" {
            continue;
        }
        let message = &entry["message"];
        let role = message["role"].as_str().unwrap_or("");
        let record = entry["id"].as_str().unwrap_or("");
        if role == "user" && !running_marker {
            current = scoped(scope, &format!("native-turn:{record}"));
        }
        let time = message["timestamp"].as_f64();
        turn(&mut out, &current, time);
        match role {
            "user" => {
                let mut row = item(
                    scope,
                    &format!("native-user:{record}"),
                    &current,
                    TranscriptItemKind::User,
                    None,
                    time,
                );
                row.record_id = Some(record.into());
                row.source_position = native_position(entry, entry_ordinal, None);
                row.text = body(&message["content"]);
                row.attachments = attachments(&message["content"]);
                row.content_blocks =
                    content_blocks(scope, &message["content"], ContentDialect::Native);
                let key = format!(
                    "{:x}:{:x}",
                    message["timestamp"].as_f64().unwrap_or(0.0).to_bits(),
                    Sha256::digest(
                        message["content"]
                            .as_str()
                            .map(String::from)
                            .unwrap_or_else(|| message["content"]
                                .as_array()
                                .map(|a| a
                                    .iter()
                                    .filter(|b| b["type"] == "text")
                                    .filter_map(|b| b["text"].as_str())
                                    .collect::<Vec<_>>()
                                    .join("\n"))
                                .unwrap_or_default())
                            .as_bytes()
                    )
                );
                if let Some(ids) = receipts.get_mut(&key)
                    && !ids.is_empty()
                {
                    row.reconciliation_id = Some(ids.remove(0));
                }
                if turn(&mut out, &current, time).input.is_empty() {
                    turn(&mut out, &current, time).input = row.text.clone();
                }
                out.items.push(row);
            }
            "assistant" => {
                let timestamp = message["timestamp"].as_f64().unwrap_or(0.0).to_bits();
                let key = format!("{timestamp:x}");
                let occurrence = occurrences.entry(key.clone()).or_default();
                let key = format!("native-assistant:{key}:{occurrence}");
                *occurrence += 1;
                let mut row = item(
                    scope,
                    &key,
                    &current,
                    TranscriptItemKind::Assistant,
                    None,
                    time,
                );
                row.record_id = Some(record.into());
                row.source_position = native_position(entry, entry_ordinal, None);
                row.text = body(&message["content"]);
                row.reasoning = text_blocks(&message["content"], "thinking");
                row.attachments = attachments(&message["content"]);
                row.content_blocks =
                    content_blocks(scope, &message["content"], ContentDialect::Native);
                row.reasoning_ended = message["stopReason"]
                    .as_str()
                    .is_some_and(|reason| !reason.is_empty());
                row.metadata = json!({"stopReason":message["stopReason"]});
                if message["stopReason"] == "error"
                    && let Some(error) = message["errorMessage"]
                        .as_str()
                        .filter(|text| !text.trim().is_empty())
                {
                    row.metadata["errorMessage"] = json!(error);
                }

                if let Some(id) = reading_ids.get(record) {
                    row.metadata["nativeStreamId"] =
                        json!(scoped(scope, &format!("pi-stream:{id}")));
                }

                let u = usage(&message["usage"]);
                let turn = turn(&mut out, &current, time);
                turn.usage.input_tokens = turn.usage.input_tokens.saturating_add(u.input_tokens);
                turn.usage.output_tokens = turn.usage.output_tokens.saturating_add(u.output_tokens);
                if let Some(blocks) = message["content"].as_array() {
                    for (index, block) in blocks
                        .iter()
                        .enumerate()
                        .filter(|(_, b)| b["type"] == "toolCall")
                    {
                        let (name, arguments) = native_tool_semantic(
                            block["name"].as_str().unwrap_or(""),
                            block["arguments"].clone(),
                        );
                        admit_tool(
                            &mut out,
                            scope,
                            &current,
                            block["id"].as_str().unwrap_or(""),
                            &name,
                            arguments,
                            ToolSource {
                                view: None,
                                position: native_position(entry, entry_ordinal, Some(index)),
                            },
                        );
                    }
                }
                out.items.push(row);
            }
            "toolResult" => {
                let call = message["toolCallId"].as_str().unwrap_or(record);
                admit_tool(
                    &mut out,
                    scope,
                    &current,
                    call,
                    message["toolName"].as_str().unwrap_or(call),
                    Value::Null,
                    ToolSource::default(),
                );
                if let Some(tool) = out
                    .tools
                    .iter_mut()
                    .find(|t| t.id == scoped(scope, &format!("tool:{call}")))
                {
                    tool.phase = if message["isError"] == true {
                        "error"
                    } else {
                        "done"
                    }
                    .into();
                    tool.result = Some(message.clone());
                    tool.result_position = native_position(entry, entry_ordinal, None);
                }
            }
            _ => {}
        }
    }
    // A following user message establishes that the preceding native turn is no longer active.
    let ids: Vec<_> = out.turns.iter().map(|t| t.id.clone()).collect();
    for id in &ids {
        if (id != &current || !snapshot.is_streaming)
            && out
                .turns
                .iter()
                .any(|t| t.id == *id && t.status == "running")
        {
            let last = out
                .items
                .iter()
                .rev()
                .find(|i| i.turn_id == *id && i.kind == TranscriptItemKind::Assistant);
            let reason = match last.and_then(|i| i.metadata["stopReason"].as_str()) {
                Some("aborted") => "aborted",
                Some("error") => "error",
                _ => "completed",
            };
            settle(&mut out, id, reason, None, None)?;
        }
    }
    if snapshot.is_streaming {
        // Updates carry the whole active message. An assistant end attached to
        // its authoritative snapshot retires that preview even during tool work.
        if let Some(event) = session
            .native_events
            .iter()
            .rev()
            .find(|e| {
                matches!(
                    e["type"].as_str(),
                    Some("message_update" | "message_start" | "message_end")
                ) && e["message"]["role"] == "assistant"
            })
            .filter(|event| event["type"] != "message_end")
        {
            let stream_id = event["agentkitStreamId"]
                .as_str()
                .filter(|id| !id.is_empty());
            // A receipt can precede an already-buffered update. Never resurrect
            // a preview explicitly known to have become a loaded native entry.
            if stream_id.is_some_and(|id| reading_ids.values().any(|known| *known == id)) {
                return Ok(out);
            }
            let message = &event["message"];
            let timestamp = message["timestamp"].as_f64().unwrap_or(0.0).to_bits();
            let base = format!("{timestamp:x}");
            // A timestamp is not proof that this preview is a committed row.
            // Reserve the next occurrence rather than overwriting stored data.
            let occurrence = *occurrences.get(&base).unwrap_or(&0);
            let mut row = item(
                scope,
                &stream_id
                    .map(|id| format!("native-stream:{id}"))
                    .unwrap_or_else(|| format!("native-assistant:{base}:{occurrence}")),
                &current,
                TranscriptItemKind::Assistant,
                None,
                message["timestamp"].as_f64(),
            );
            row.text = body(&message["content"]);
            row.content_blocks = content_blocks(scope, &message["content"], ContentDialect::Native);
            row.reasoning = text_blocks(&message["content"], "thinking");
            row.is_provisional = true;
            if let Some(id) = stream_id {
                row.metadata["nativeStreamId"] = json!(scoped(scope, &format!("pi-stream:{id}")));
            }
            row.reasoning_ended = event["assistantMessageEvent"]["type"] == "thinking_end";
            upsert(&mut out, row);
        }
    }
    Ok(out)
}

fn native_tool_semantic(name: &str, mut arguments: Value) -> (String, Value) {
    if name == "tekes_subagent" && arguments["action"] == "create" {
        if let Some(label) = arguments["label"].as_str().map(str::to_owned) {
            arguments["description"] = Value::String(label);
        }
        ("subagent".into(), arguments)
    } else {
        (name.into(), arguments)
    }
}

fn native_position(
    entry: &Value,
    entry_ordinal: usize,
    content_index: Option<usize>,
) -> Option<TranscriptSourcePosition> {
    entry["id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .map(|entry_id| TranscriptSourcePosition::Native {
            entry_id: entry_id.into(),
            entry_ordinal,
            content_index,
        })
}
