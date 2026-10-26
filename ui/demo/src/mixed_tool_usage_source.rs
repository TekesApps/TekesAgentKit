//! Raw public-source evidence paired with the original mixed-tool RunBinding.
//! This module is opt-in acceptance instrumentation, not product behavior.
use serde_json::{Value, json};
use tekes_agent_contract::{Result, require};
use tekes_agent_ui_bindings::RunBinding;

pub(super) fn capture(binding: &RunBinding, turn_id: &str) -> Result<Value> {
    let witness = binding.refresh_with_source_witness()?;
    require(
        witness.projection_succeeded(),
        "usage witness used degraded projection",
    )?;
    let scope = binding.scope();
    let snapshot = witness.snapshot();
    let turn = snapshot
        .transcript
        .turns
        .iter()
        .find(|turn| turn.id == turn_id)
        .ok_or_else(|| {
            tekes_agent_contract::Error::Protocol("usage witness canonical turn missing".into())
        })?;
    let authority = witness.authority();
    require(
        authority.endpoint_id == scope.endpoint_id,
        "usage witness authority endpoint mismatch",
    )?;
    let session = authority.sessions.get(&scope.session_id).ok_or_else(|| {
        tekes_agent_contract::Error::Protocol("usage witness session source missing".into())
    })?;
    let source_kind;
    let mut reported_sources = Vec::new();
    let mut live_sources = Vec::new();
    let mut raw_turn = Value::Null;
    let mut turn_started = false;
    if let Some(native) = &session.native_conversation {
        source_kind = "nativeConversation";
        let records: Vec<String> = snapshot
            .transcript
            .items
            .iter()
            .filter(|item| item.turn_id == turn_id)
            .filter_map(|item| item.record_id.clone())
            .collect();
        for entry in &native.entries {
            if entry["type"] == "message"
                && entry["message"]["role"] == "assistant"
                && entry["message"]["usage"].is_object()
                && records
                    .iter()
                    .any(|record| entry["id"].as_str() == Some(record.as_str()))
            {
                reported_sources.push(json!({"recordId":entry["id"],"kind":"message",
                    "usage":entry["message"]["usage"],"entry":entry}));
            }
        }
        // Pi's frozen contract exposes persisted message usage in turn.usage;
        // it does not expose live or reported usage in this presentation.
    } else {
        source_kind = "journal";
        let numeric_turn = serde_json::from_str::<Vec<String>>(turn_id)
            .ok()
            .filter(|parts| parts.len() == 3)
            .and_then(|parts| {
                parts[2]
                    .strip_prefix("turn:")
                    .and_then(|number| number.parse::<i64>().ok())
            });
        let number = numeric_turn.ok_or_else(|| {
            tekes_agent_contract::Error::Protocol(
                "journal canonical turn identity is not parseable".into(),
            )
        })?;
        for entry in session
            .entries
            .values()
            .filter(|entry| entry.event.data["turn"].as_i64() == Some(number))
        {
            let event = &entry.event;
            let data = &event.data;
            if event.kind == "turn/start" {
                turn_started = true;
            }
            if matches!(
                event.kind.as_str(),
                "assistant/message" | "assistant/attempt"
            ) {
                let usage = if data["usage"].is_object() {
                    &data["usage"]
                } else {
                    data["stream"]
                        .as_array()
                        .and_then(|records| {
                            records.iter().rev().find_map(|record| {
                                (record["type"] == "chunk" && record["chunk"]["type"] == "usage")
                                    .then_some(&record["chunk"]["usage"])
                            })
                        })
                        .unwrap_or(&Value::Null)
                };
                reported_sources.push(json!({"sequence":event.seq,"kind":event.kind,
                    "turn":number,"step":data["step"],"recordId":data["message"]["id"],
                    "usage":usage,"outputKnown":usage["outputTokens"].as_u64().is_some()
                        || usage["output"].as_u64().is_some(),"event":event}));
            }
            if event.kind == "turn/end" {
                raw_turn = json!({"sequence":event.seq,"kind":event.kind,"turn":number,
                    "usage":data["usage"],"reason":data["reason"],"event":event});
            }
        }
        for (key, preview) in &session.previews {
            if key
                .split_once(':')
                .and_then(|(turn, _)| turn.parse::<i64>().ok())
                == Some(number)
                && let Some(count) = preview.output_tokens
            {
                live_sources.push(json!({"previewKey":key,"outputTokens":count,
                    "presentationOrder":preview.presentation_order,"preview":preview}));
            }
        }
    }
    let reported = &turn.reported_usage;
    let live = &turn.live_usage;
    let canonical_items: Vec<_> = snapshot
        .transcript
        .items
        .iter()
        .filter(|item| item.turn_id == turn_id)
        .map(|item| {
            json!({"id":item.id,"recordId":item.record_id,"turnId":item.turn_id,
            "kind":item.kind,"sourceSequence":item.source_sequence})
        })
        .collect();
    let receipt = json!({
        "revision":snapshot.revision,"connectionEpoch":snapshot.connection_epoch,
        "endpointId":scope.endpoint_id,"threadId":scope.thread_id,"sessionId":scope.session_id,
        "turnId":turn.id,"sourceKind":source_kind,"projectionSucceeded":witness.projection_succeeded(),
        "authority":{"endpointId":authority.endpoint_id,"connectionEpoch":authority.connection_epoch,
            "sessionKey":scope.session_id},
        "snapshot":&**snapshot,
        "rawSession":session,
        "canonicalItems":canonical_items,
        "canonical":turn,
        "rawTurn":raw_turn,"reportedSources":reported_sources,"liveSources":live_sources,
        "turnStarted":turn_started,
        "canonicalAbsence":{"reportedUsage":reported.is_none(),"liveUsage":live.is_none()},
        "absenceContract":"docs/session-endpoints.md:37-38"
    });
    Ok(receipt)
}
