use serde_json::{Value, json};
use tekes_agent_contract::*;
use tekes_agent_presentation::*;
use tekes_agent_session::*;
fn journal() -> TranscriptSnapshot {
    let rows = vec![
        ("turn/start", json!({"turn":1})),
        (
            "user/message",
            json!({"turn":1,"id":"u","content":[{"type":"text","text":"input"}]}),
        ),
        (
            "assistant/message",
            json!({"turn":1,"step":1,"message":{"id":"a","content":[{"type":"text","text":"before"},{"type":"tool-call","id":"c","name":"read","arguments":{"path":"file"}}]}}),
        ),
        (
            "tool/call",
            json!({"turn":1,"callId":"c","name":"read","arguments":{"path":"file"}}),
        ),
        (
            "tool/result",
            json!({"turn":1,"message":{"source":{"callId":"c"},"content":[{"type":"text","text":"result"}]}}),
        ),
        (
            "assistant/message",
            json!({"turn":1,"step":2,"message":{"id":"z","content":[{"type":"text","text":"after"}]}}),
        ),
        ("turn/end", json!({"turn":1,"reason":"completed"})),
    ];
    let entries: Vec<HistoryEntry> = rows
        .into_iter()
        .enumerate()
        .map(|(seq, (kind, data))| {
            serde_json::from_value(json!({"event":{"type":kind,"seq":seq,"time":1,"data":data}}))
                .unwrap()
        })
        .collect();
    let mut session = SessionProjection::default();
    session
        .apply_snapshot(JournalSnapshot {
            session_id: "a".into(),
            generation: "g".into(),
            through_sequence: 6,
            window_limit: 50,
            entries,
            has_more_before: false,
            projections: None,
            history_revision: None,
        })
        .unwrap();
    TranscriptSnapshot::read(&RunScope::main("e", "a"), &session).unwrap()
}
fn native(entries: Vec<Value>, preview: Option<Value>) -> TranscriptSnapshot {
    let session = SessionProjection {
        journal_ready: true,
        native_conversation: Some(NativeConversationSnapshot {
            session_id: "a".into(),
            generation: "g".into(),
            leaf_id: entries
                .last()
                .and_then(|e| e["id"].as_str())
                .map(str::to_owned),
            entries,
            has_more_before: false,
            is_streaming: preview.is_some(),
            state: json!({}),
        }),
        native_events: preview.into_iter().collect(),
        ..Default::default()
    };
    TranscriptSnapshot::read(&RunScope::main("e", "a"), &session).unwrap()
}
fn native_entries() -> Vec<Value> {
    vec![
        json!({"type":"message","id":"u","message":{"role":"user","timestamp":1,"content":"input"}}),
        json!({"type":"message","id":"a","message":{"role":"assistant","timestamp":2,"content":[{"type":"thinking","thinking":"reasoning"},{"type":"text","text":"before"},{"type":"toolCall","id":"c","name":"read","arguments":{"path":"file"}}]}}),
        json!({"type":"message","id":"r","message":{"role":"toolResult","toolCallId":"c","timestamp":3,"content":[{"type":"text","text":"result"}]}}),
        json!({"type":"message","id":"z","message":{"role":"assistant","timestamp":4,"stopReason":"stop","content":[{"type":"text","text":"after"}]}}),
    ]
}
fn kinds(order: &TranscriptEventOrder<'_>) -> Vec<&'static str> {
    order
        .ordered
        .iter()
        .map(|event| match event {
            TranscriptOrderedEvent::Message(_) => "message",
            TranscriptOrderedEvent::ToolCall(_) => "call",
            TranscriptOrderedEvent::ToolResult(_) => "result",
        })
        .collect()
}
#[test]
fn journal_keeps_first_call_and_separate_result_position_without_clock_sorting() {
    let snapshot = journal();
    assert_eq!(snapshot.tools.len(), 1);
    assert_eq!(
        snapshot.tools[0].source_position,
        Some(TranscriptSourcePosition::Journal {
            sequence: 2,
            content_index: Some(1)
        })
    );
    assert_eq!(
        snapshot.tools[0].result_position,
        Some(TranscriptSourcePosition::Journal {
            sequence: 4,
            content_index: None
        })
    );
    let doc = TranscriptDocument::read(&snapshot).unwrap();
    let order = doc.sections[0].event_order().unwrap();
    assert_eq!(
        kinds(&order),
        vec!["message", "message", "call", "result", "message"]
    );
    assert!(order.unplaced.is_empty());
    assert!(
        matches!(order.ordered[1],TranscriptOrderedEvent::Message(item) if item.text=="before")
    );
    assert!(matches!(order.ordered[4],TranscriptOrderedEvent::Message(item) if item.text=="after"));
}
#[test]
fn native_entry_order_and_block_index_survive_paging_without_becoming_identity() {
    let entries = native_entries();
    let first = native(entries.clone(), None);
    let mut prefixed = vec![
        json!({"type":"message","id":"older-u","message":{"role":"user","timestamp":0,"content":"older"}}),
    ];
    prefixed.extend(entries);
    let next = native(prefixed, None);
    let first_doc = TranscriptDocument::read(&first).unwrap();
    let next_doc = TranscriptDocument::read(&next).unwrap();
    assert_eq!(first_doc.sections[0].id, next_doc.sections[1].id);
    assert_eq!(first.tools[0].id, next.tools[0].id);
    assert_eq!(
        first.tools[0].source_position,
        Some(TranscriptSourcePosition::Native {
            entry_id: "a".into(),
            entry_ordinal: 1,
            content_index: Some(2)
        })
    );
    assert_eq!(
        next.tools[0].source_position,
        Some(TranscriptSourcePosition::Native {
            entry_id: "a".into(),
            entry_ordinal: 2,
            content_index: Some(2)
        })
    );
    assert_eq!(
        kinds(&first_doc.sections[0].event_order().unwrap()),
        vec!["message", "message", "call", "result", "message"]
    );
    assert_eq!(
        kinds(&first_doc.sections[0].event_order().unwrap()),
        kinds(&next_doc.sections[1].event_order().unwrap())
    );
}
#[test]
fn native_stream_preview_is_explicitly_unplaced_not_a_fabricated_entry() {
    let snapshot = native(
        native_entries()[..2].to_vec(),
        Some(
            json!({"type":"message_update","message":{"role":"assistant","timestamp":2,"content":[{"type":"text","text":"streaming"}]}}),
        ),
    );
    let doc = TranscriptDocument::read(&snapshot).unwrap();
    let order = doc.sections[0].event_order().unwrap();
    assert_eq!(order.unplaced.len(), 1);
    assert!(
        matches!(order.unplaced[0],TranscriptOrderedEvent::Message(item) if item.is_provisional && item.text=="streaming")
    );
}
#[test]
fn old_json_and_inconsistent_positions_never_acquire_guessed_order() {
    let original = journal();
    let mut value = serde_json::to_value(&original).unwrap();
    for item in value["items"].as_array_mut().unwrap() {
        item.as_object_mut().unwrap().remove("sourcePosition");
    }
    for tool in value["tools"].as_array_mut().unwrap() {
        tool.as_object_mut().unwrap().remove("sourcePosition");
        tool.as_object_mut().unwrap().remove("resultPosition");
    }
    let legacy: TranscriptSnapshot = serde_json::from_value(value).unwrap();
    let doc = TranscriptDocument::read(&legacy).unwrap();
    let order = doc.sections[0].event_order().unwrap();
    assert!(order.ordered.is_empty());
    assert_eq!(order.unplaced.len(), 5);
    let mut bad = original.clone();
    bad.tools[0].source_position = bad.items[0].source_position.clone();
    assert!(
        TranscriptDocument::read(&bad).unwrap().sections[0]
            .event_order()
            .is_err()
    );
    bad = original.clone();
    bad.items[0].source_position = Some(TranscriptSourcePosition::Native {
        entry_id: "u".into(),
        entry_ordinal: 0,
        content_index: None,
    });
    assert!(
        TranscriptDocument::read(&bad).unwrap().sections[0]
            .event_order()
            .is_err()
    );
    bad = original;
    bad.items[0].source_position = Some(TranscriptSourcePosition::Journal {
        sequence: -1,
        content_index: None,
    });
    assert!(
        TranscriptDocument::read(&bad).unwrap().sections[0]
            .event_order()
            .is_err()
    );
}
