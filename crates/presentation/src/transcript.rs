use crate::RunScope;
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
    pub started_at: Option<f64>,
    pub ended_at: Option<f64>,
    pub usage: TokenUsage,
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
    pub is_visible: bool,
    pub is_final: bool,
    pub is_provisional: bool,
    pub reasoning_ended: bool,
    pub source_sequence: Option<i64>,
    pub time: Option<f64>,
    pub metadata: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptToolCall {
    pub id: String,
    pub turn_id: String,
    pub name: String,
    pub arguments: Value,
    pub result: Option<Value>,
    pub phase: String,
    pub call_view: Option<Value>,
    pub result_view: Option<Value>,
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
fn scoped(scope: &RunScope, suffix: &str) -> String {
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
impl TranscriptSnapshot {
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
        is_visible: true,
        is_final: false,
        is_provisional: false,
        reasoning_ended: true,
        source_sequence: seq,
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
    let mut opening_inputs = std::collections::BTreeSet::new();
    for entry in session.entries.values() {
        let event = &entry.event;
        let data = &event.data;
        let number = data["turn"].as_i64().unwrap_or(current);
        let turn_id = scoped(scope, &format!("turn:{number}"));
        let step = data["step"].as_i64().unwrap_or(0);
        let seq = Some(event.seq);
        let time = Some(event.time);
        match event.kind.as_str() {
            "turn/start" => {
                current = number;
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
                    row.text = body(&data["message"]["content"]);
                    row.reasoning = text_blocks(&data["message"]["content"], "reasoning");
                    row.attachments = attachments(&data["message"]["content"]);
                    row.record_id = Some(
                        data["message"]["id"]
                            .as_str()
                            .map(String::from)
                            .unwrap_or_else(|| format!("assistant-{}", event.seq)),
                    );
                    message_usage.insert(
                        (turn_id.clone(), row.record_id.clone().unwrap()),
                        usage(&data["usage"]),
                    );
                    if let Some(blocks) = data["message"]["content"].as_array() {
                        for block in blocks.iter().filter(|b| b["type"] == "tool-call") {
                            admit_tool(
                                &mut out,
                                scope,
                                &turn_id,
                                block["id"].as_str().unwrap_or(""),
                                block["name"].as_str().unwrap_or(""),
                                block["arguments"].clone(),
                                None,
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
                } else {
                    row.text = body(&data["message"]["content"]);
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
                    entry.view.clone(),
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
                admit_tool(&mut out, scope, &turn_id, call, call, Value::Null, None);
                let tool = out
                    .tools
                    .iter_mut()
                    .find(|t| t.id == scoped(scope, &format!("tool:{call}")))
                    .expect("admitted");
                tool.result = Some(message.clone());
                tool.result_view = entry.view.clone();
                tool.phase = if data["error"]["code"] == "interrupted" {
                    "stopped"
                } else if data["error"].is_object()
                    || message["isError"] == true
                    || first["isError"] == true
                    || data["meta"]["exitCode"].as_i64().is_some_and(|n| n != 0)
                {
                    "error"
                } else {
                    "done"
                }
                .into();
            }
            "turn/end" => {
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
    for (key, preview) in &session.previews {
        let Some((number, step)) = key.split_once(':') else {
            continue;
        };
        let turn_id = scoped(scope, &format!("turn:{number}"));
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
fn admit_tool(
    out: &mut TranscriptSnapshot,
    scope: &RunScope,
    turn: &str,
    id: &str,
    name: &str,
    arguments: Value,
    view: Option<Value>,
) {
    let key = scoped(scope, &format!("tool:{id}"));
    if let Some(tool) = out.tools.iter_mut().find(|t| t.id == key) {
        if !arguments.is_null() {
            tool.arguments = arguments;
        }
        if view.is_some() {
            tool.call_view = view;
        }
        if !name.is_empty() && name != id {
            tool.name = name.into();
        }
    } else {
        out.tools.push(TranscriptToolCall {
            id: key,
            turn_id: turn.into(),
            name: name.into(),
            arguments,
            result: None,
            phase: "doing".into(),
            call_view: view,
            result_view: None,
        });
    }
}

fn native(scope: &RunScope, session: &SessionProjection) -> Result<TranscriptSnapshot> {
    let snapshot = session
        .native_conversation
        .as_ref()
        .ok_or_else(|| Error::Protocol("missing native snapshot".into()))?;
    snapshot.validate()?;
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
    for entry in &snapshot.entries {
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
                row.text = body(&message["content"]);
                row.attachments = attachments(&message["content"]);
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
                row.text = body(&message["content"]);
                row.reasoning = text_blocks(&message["content"], "thinking");
                row.attachments = attachments(&message["content"]);
                row.metadata = json!({"stopReason":message["stopReason"]});
                let u = usage(&message["usage"]);
                let turn = turn(&mut out, &current, time);
                turn.usage.input_tokens = turn.usage.input_tokens.saturating_add(u.input_tokens);
                turn.usage.output_tokens = turn.usage.output_tokens.saturating_add(u.output_tokens);
                if let Some(blocks) = message["content"].as_array() {
                    for block in blocks.iter().filter(|b| b["type"] == "toolCall") {
                        admit_tool(
                            &mut out,
                            scope,
                            &current,
                            block["id"].as_str().unwrap_or(""),
                            block["name"].as_str().unwrap_or(""),
                            block["arguments"].clone(),
                            None,
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
                    None,
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
        // RPC message_update carries the complete current message, so the retained tail is sufficient.
        if let Some(event) = session.native_events.iter().rev().find(|e| {
            matches!(e["type"].as_str(), Some("message_update" | "message_start"))
                && e["message"]["role"] == "assistant"
        }) {
            let message = &event["message"];
            let timestamp = message["timestamp"].as_f64().unwrap_or(0.0).to_bits();
            let base = format!("{timestamp:x}");
            let mut occurrence = *occurrences.get(&base).unwrap_or(&0);
            if out.items.iter().any(|i| {
                i.kind == TranscriptItemKind::Assistant && i.time == message["timestamp"].as_f64()
            }) {
                occurrence = occurrence.saturating_sub(1);
            }
            let mut row = item(
                scope,
                &format!("native-assistant:{base}:{occurrence}"),
                &current,
                TranscriptItemKind::Assistant,
                None,
                message["timestamp"].as_f64(),
            );
            row.text = body(&message["content"]);
            row.reasoning = text_blocks(&message["content"], "thinking");
            row.is_provisional = true;
            row.reasoning_ended = event["assistantMessageEvent"]["type"] == "thinking_end";
            upsert(&mut out, row);
        }
    }
    Ok(out)
}
