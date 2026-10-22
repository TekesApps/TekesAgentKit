use serde_json::{Value, json};
use tekes_agent_contract::*;
use tekes_agent_presentation::*;
use tekes_agent_session::*;
fn scope() -> RunScope {
    RunScope::main("sep", "s")
}
fn event(seq: i64, kind: &str, data: Value) -> Value {
    json!({"event":{"type":kind,"seq":seq,"time":1000-seq,"data":data}})
}
fn authority(entries: Vec<Value>, more: bool) -> MemoryProjectionStore {
    let mut store = MemoryProjectionStore::new("sep");
    let cut = entries.last().unwrap()["event"]["seq"].as_i64().unwrap();
    store.apply("journal:s", json!({"type":"snapshot","snapshot":{"sessionId":"s","generation":"g","throughSequence":cut,"windowLimit":50,"entries":entries,"hasMoreBefore":more}})).unwrap();
    store
}
fn transient(store: &mut MemoryProjectionStore, seq: i64, kind: &str, presentation: Option<Value>) {
    let mut data = json!({"turn":1,"step":1,"chunk":{"type":kind,"delta":"中文推理"}});
    if let Some(value) = presentation {
        data["presentation"] = value;
    }
    store.apply("journal:s", json!({"type":"transient","generation":"g","sessionId":"s","event":{"type":"assistant/chunk","seq":seq,"time":0,"data":data}})).unwrap();
}
fn live(store: &mut MemoryProjectionStore, entry: Value) {
    store
        .apply(
            "journal:s",
            json!({"type":"event","generation":"g","sessionId":"s","entry":entry}),
        )
        .unwrap();
}
fn run(store: &mut MemoryProjectionStore) -> RunSnapshot {
    store.connection = ConnectionState::Ready;
    store.inventory_ready = true;
    store.control_ready = true;
    store.actionables_ready = true;
    let mut view = PresentationStore::default();
    view.select(ComposerDestination::run(scope())).unwrap();
    view.refresh(store).unwrap()
}
fn assistant(snapshot: &RunSnapshot) -> &TranscriptItem {
    snapshot
        .transcript
        .items
        .iter()
        .find(|row| row.kind == TranscriptItemKind::Assistant)
        .unwrap()
}
#[test]
fn transient_uses_durable_anchor_not_transport_ordinal_and_full_presentation_keeps_end_signal() {
    let mut store = authority(vec![event(10, "turn/start", json!({"turn":1}))], false);
    transient(&mut store, 9999, "reasoning-delta", None);
    let before = run(&mut store);
    let item = assistant(&before);
    let source = &before.transcript.thinking_sources.by_id[&item.id];
    assert_eq!(source.arrival_order, 11);
    assert_eq!(store.sessions["s"].tail_sequence, 10);
    assert_eq!(
        before
            .thinking_frame(&item.turn_id, TranscriptThinkingMode::None, 1, 1)
            .unwrap()
            .display_for(&item.id),
        Some(TranscriptReasoningDisplay::Placeholder { dots: 2 })
    );
    transient(
        &mut store,
        0,
        "reasoning-end",
        Some(json!({"text":"","reasoning":"已结束的推理"})),
    );
    let after = run(&mut store);
    let item = assistant(&after);
    assert!(item.reasoning_ended);
    assert_eq!(
        after.transcript.thinking_sources.by_id[&item.id].arrival_order,
        11
    );
    assert_eq!(
        after
            .thinking_frame(&item.turn_id, TranscriptThinkingMode::Compact, 2, 2)
            .unwrap()
            .display_for(&item.id),
        Some(TranscriptReasoningDisplay::Disclosure { active: false })
    );
}
#[test]
fn same_step_call_anchors_committed_message_and_replaces_hidden_thinking() {
    let mut store = authority(vec![event(0, "turn/start", json!({"turn":1}))], false);
    transient(&mut store, 200, "reasoning-delta", None);
    live(
        &mut store,
        event(
            1,
            "tool/call",
            json!({"turn":1,"step":1,"callId":"call","name":"read","arguments":{}}),
        ),
    );
    let preview = run(&mut store);
    let row = assistant(&preview);
    assert_eq!(
        preview
            .thinking_frame(&row.turn_id, TranscriptThinkingMode::None, 1, 1)
            .unwrap()
            .display_for(&row.id),
        Some(TranscriptReasoningDisplay::Hidden)
    );
    live(
        &mut store,
        event(
            2,
            "assistant/message",
            json!({"turn":1,"step":1,"message":{"id":"answer","content":[{"type":"reasoning","text":"过程"}]}}),
        ),
    );
    let committed = run(&mut store);
    let row = assistant(&committed);
    assert!(!row.is_provisional);
    let sources = &committed.transcript.thinking_sources;
    assert_eq!(sources.by_id[&row.id].arrival_order, 1);
    assert_eq!(
        sources.by_id[&row.id].response_frame_id,
        sources.by_id[&committed.transcript.tools[0].id].response_frame_id
    );
}
#[test]
fn late_chunks_cannot_replace_a_committed_response_or_closed_turn() {
    let mut store = authority(
        vec![
            event(0, "turn/start", json!({"turn":1})),
            event(
                1,
                "assistant/message",
                json!({"turn":1,"step":1,"message":{"id":"answer","content":[{"type":"reasoning","text":"保留的过程"},{"type":"text","text":"最终结果"}]}}),
            ),
        ],
        false,
    );
    transient(&mut store, 99, "reasoning-delta", None);
    let committed = run(&mut store);
    assert!(!assistant(&committed).is_provisional);
    assert_eq!(assistant(&committed).text, "最终结果");
    live(
        &mut store,
        event(2, "turn/end", json!({"turn":1,"reason":"completed"})),
    );
    transient(&mut store, 0, "text-delta", None);
    let final_run = run(&mut store);
    let row = assistant(&final_run);
    assert!(row.is_final);
    assert_eq!(row.text, "最终结果");
    assert!(!row.is_provisional);
    assert_eq!(final_run.transcript.thinking_sources.live_turn_id, None);
    assert_eq!(
        final_run
            .thinking_frame(&row.turn_id, TranscriptThinkingMode::None, 1, 3)
            .unwrap()
            .display_for(&row.id),
        Some(TranscriptReasoningDisplay::Disclosure { active: false })
    );
}
#[test]
fn history_prepend_preserves_captured_preview_anchor_and_context() {
    let mut store = authority(
        vec![
            event(2, "turn/start", json!({"turn":1})),
            event(3, "user/message", json!({"turn":1,"content":"当前输入"})),
        ],
        true,
    );
    transient(&mut store, 700, "reasoning-delta", None);
    let before = run(&mut store);
    let id = assistant(&before).id.clone();
    let expected = before.transcript.thinking_sources.by_id[&id].clone();
    let request = HistoryRequest {
        session_id: "s".into(),
        through_sequence: 3,
        before_sequence: Some(2),
        max_messages: 50,
    };
    let page=serde_json::from_value(json!({"sessionId":"s","throughSequence":3,"entries":[event(0,"turn/start",json!({"turn":0})),event(1,"turn/end",json!({"turn":0,"reason":"completed"}))],"hasMoreBefore":false})).unwrap();
    store
        .sessions
        .get_mut("s")
        .unwrap()
        .apply_page(page, &request)
        .unwrap();
    let after = run(&mut store);
    assert_eq!(after.transcript.thinking_sources.by_id[&id], expected);
    assert_eq!(expected.arrival_order, 4);
    assert_eq!(store.sessions["s"].tail_sequence, 3);
}
fn native() -> MemoryProjectionStore {
    let mut store = MemoryProjectionStore::new("sep");
    store.apply("journal:s",json!({"type":"conversation","generation":"g","snapshot":{"sessionId":"s","generation":"g","entries":[{"id":"u","type":"message","message":{"role":"user","timestamp":5000,"content":"input"}},{"id":"a","type":"message","message":{"role":"assistant","timestamp":-10,"stopReason":"toolUse","content":[{"type":"thinking","thinking":"旧过程"},{"type":"toolCall","id":"call","name":"read","arguments":{}}]}}],"leafId":"a","hasMoreBefore":false,"isStreaming":true,"state":{}}})).unwrap();
    store
}
#[test]
fn pi_uses_public_ancestry_and_lifecycle_not_clock_or_transport_buffer_position() {
    let mut store = native();
    let event = json!({"type":"message_update","agentkitStreamId":"stream","assistantMessageEvent":{"type":"thinking_delta"},"message":{"role":"assistant","timestamp":-1000,"content":[{"type":"thinking","thinking":"新过程"}]}});
    store
        .apply(
            "journal:s",
            json!({"type":"native-event","generation":"g","sessionId":"s","event":event}),
        )
        .unwrap();
    let before = run(&mut store);
    let row = before
        .transcript
        .items
        .iter()
        .find(|row| row.is_provisional)
        .unwrap();
    let key = row.id.clone();
    let fact = before.transcript.thinking_sources.by_id[&key].clone();
    assert_eq!(fact.arrival_order, 2);
    assert_eq!(
        before
            .thinking_frame(&row.turn_id, TranscriptThinkingMode::None, 1, 1)
            .unwrap()
            .display_for(&key),
        Some(TranscriptReasoningDisplay::Placeholder { dots: 2 })
    );
    for _ in 0..200 {
        store.apply("journal:s",json!({"type":"native-event","generation":"g","sessionId":"s","event":{"type":"heartbeat"}})).unwrap();
    }
    let after = run(&mut store);
    assert_eq!(after.transcript.thinking_sources.by_id[&key], fact);
    let committed = after
        .transcript
        .items
        .iter()
        .find(|row| row.record_id.as_deref() == Some("a"))
        .unwrap();
    let tool = &after.transcript.tools[0];
    assert_eq!(
        after.transcript.thinking_sources.by_id[&committed.id].arrival_order,
        1
    );
    assert_eq!(
        after.transcript.thinking_sources.by_id[&committed.id].response_frame_id,
        after.transcript.thinking_sources.by_id[&tool.id].response_frame_id
    );
}
#[test]
fn legacy_missing_sources_and_foreign_scope_fail_without_host_reconstruction() {
    let mut store = authority(vec![event(0, "turn/start", json!({"turn":1}))], false);
    transient(&mut store, 9, "reasoning-delta", None);
    let mut snapshot = run(&mut store);
    let row = assistant(&snapshot).clone();
    *snapshot.transcript.thinking_sources = Default::default();
    assert!(
        snapshot
            .thinking_frame(&row.turn_id, TranscriptThinkingMode::None, 1, 0)
            .is_err()
    );
    snapshot.transcript.thinking_sources.scope = Some(RunScope::main("sep", "other"));
    assert!(
        snapshot
            .thinking_frame(&row.turn_id, TranscriptThinkingMode::None, 1, 0)
            .is_err()
    );
}

#[test]
fn native_live_value_outlives_bounded_log_and_empty_text_start_does_not_reactivate_thinking() {
    let mut store = native();
    let message = json!({"role":"assistant","timestamp":-1000,"content":[{"type":"thinking","thinking":"保留的当前过程"}]});
    for kind in ["thinking_delta", "thinking_end", "text_start"] {
        store.apply("journal:s",json!({"type":"native-event","generation":"g","sessionId":"s","event":{"type":"message_update","agentkitStreamId":"stream","assistantMessageEvent":{"type":kind},"message":message}})).unwrap();
    }
    for _ in 0..300 {
        store.apply("journal:s",json!({"type":"native-event","generation":"g","sessionId":"s","event":{"type":"heartbeat"}})).unwrap();
    }
    assert_eq!(store.sessions["s"].native_events.len(), 256);
    assert!(
        !store.sessions["s"]
            .native_events
            .iter()
            .any(|event| event["type"] == "message_update")
    );
    let snapshot = run(&mut store);
    let row = snapshot
        .transcript
        .items
        .iter()
        .find(|row| row.is_provisional)
        .unwrap();
    assert_eq!(row.reasoning, "保留的当前过程");
    assert!(row.reasoning_ended);
    assert_eq!(
        snapshot
            .thinking_frame(&row.turn_id, TranscriptThinkingMode::Compact, 1, 3)
            .unwrap()
            .display_for(&row.id),
        Some(TranscriptReasoningDisplay::Disclosure { active: false })
    );
    // A foreign old lifecycle end cannot retire this stream.
    store.apply("journal:s",json!({"type":"native-event","generation":"g","sessionId":"s","event":{"type":"message_end","agentkitStreamId":"old","message":message}})).unwrap();
    assert!(
        run(&mut store)
            .transcript
            .items
            .iter()
            .any(|row| row.is_provisional)
    );
    store.apply("journal:s",json!({"type":"native-event","generation":"g","sessionId":"s","event":{"type":"message_end","agentkitStreamId":"stream","message":message}})).unwrap();
    assert!(
        !run(&mut store)
            .transcript
            .items
            .iter()
            .any(|row| row.is_provisional)
    );
}

#[test]
fn a_started_new_turn_and_a_sealed_step_reject_old_previews_without_cursor_changes() {
    let mut store = authority(
        vec![
            event(0, "turn/start", json!({"turn":1})),
            event(1, "turn/start", json!({"turn":2})),
        ],
        false,
    );
    transient(&mut store, 99, "reasoning-delta", None);
    assert!(
        !run(&mut store)
            .transcript
            .items
            .iter()
            .any(|row| row.is_provisional)
    );
    let mut store = authority(
        vec![
            event(0, "turn/start", json!({"turn":1})),
            event(1, "step/end", json!({"turn":1,"step":1})),
        ],
        false,
    );
    transient(&mut store, 99, "text-delta", None);
    assert!(
        !run(&mut store)
            .transcript
            .items
            .iter()
            .any(|row| row.is_provisional)
    );
    assert_eq!(store.sessions["s"].tail_sequence, 1);
}
#[test]
fn native_lifecycle_start_and_disconnect_retire_only_current_projection() {
    let mut store = native();
    let message = json!({"role":"assistant","timestamp":0,"content":[{"type":"thinking","thinking":"当前过程"}]});
    for (kind, stream, delta) in [
        ("message_start", "one", "start"),
        ("message_update", "one", "thinking_end"),
        ("message_start", "two", "start"),
        ("message_update", "two", "thinking_delta"),
    ] {
        store.apply("journal:s",json!({"type":"native-event","generation":"g","sessionId":"s","event":{"type":kind,"agentkitStreamId":stream,"assistantMessageEvent":{"type":delta},"message":message}})).unwrap();
    }
    store.apply("journal:s",json!({"type":"native-event","generation":"g","sessionId":"s","event":{"type":"message_update","agentkitStreamId":"one","assistantMessageEvent":{"type":"thinking_delta"},"message":{"role":"assistant","content":[{"type":"thinking","thinking":"迟到旧内容"}]}}})).unwrap();
    let snapshot = run(&mut store);
    let row = snapshot
        .transcript
        .items
        .iter()
        .find(|row| row.is_provisional)
        .unwrap();
    assert_eq!(row.reasoning, "当前过程");
    assert!(!row.reasoning_ended);
    let history = store.sessions["s"].native_conversation.clone();
    store.disconnect();
    assert!(store.sessions["s"].native_assistant.is_none());
    assert!(store.sessions["s"].native_events.is_empty());
    assert_eq!(store.sessions["s"].native_conversation, history);
    assert!(
        !TranscriptSnapshot::read(&scope(), &store.sessions["s"])
            .unwrap()
            .items
            .iter()
            .any(|row| row.is_provisional)
    );
}
