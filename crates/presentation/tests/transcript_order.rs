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

#[test]
fn message_blocks_reference_only_their_own_section_tool_without_reordering_text() {
    let snapshot = journal();
    let doc = TranscriptDocument::read(&snapshot).unwrap();
    let section = &doc.sections[0];
    let blocks = section.outputs[0].item.content_blocks.as_ref().unwrap();
    assert_eq!(
        blocks[0].content,
        TranscriptBlockContent::Text {
            text: "before".into()
        }
    );
    assert!(std::ptr::eq(
        section.tool_for_block(&blocks[1]).unwrap(),
        &snapshot.tools[0]
    ));
    let foreign = TranscriptContentBlock {
        index: 1,
        content: TranscriptBlockContent::ToolCall {
            tool_id: "foreign-scope-call".into(),
        },
    };
    assert!(section.tool_for_block(&foreign).is_none());
    let mut legacy = serde_json::to_value(&snapshot).unwrap();
    for item in legacy["items"].as_array_mut().unwrap() {
        item.as_object_mut().unwrap().remove("contentBlocks");
    }
    let legacy: TranscriptSnapshot = serde_json::from_value(legacy).unwrap();
    assert!(
        legacy
            .items
            .iter()
            .all(|item| item.content_blocks.is_none())
    );
}
#[test]
fn native_stream_has_ordered_content_but_no_fabricated_tool_or_durable_position() {
    let snapshot = native(
        native_entries()[..1].to_vec(),
        Some(
            json!({"type":"message_update","message":{"role":"assistant","timestamp":2,"content":[
                {"type":"thinking","thinking":"分析"},{"type":"text","text":"前文"},{"type":"toolCall","id":"stream-only-call","name":"read","arguments":{}},{"type":"text","text":"后文"}
            ]}}),
        ),
    );
    let doc = TranscriptDocument::read(&snapshot).unwrap();
    let section = &doc.sections[0];
    let item = section.outputs[0].item;
    let blocks = item.content_blocks.as_ref().unwrap();
    assert_eq!(blocks.len(), 4);
    assert!(item.is_provisional && item.source_position.is_none());
    assert_eq!(
        blocks[0].content,
        TranscriptBlockContent::Reasoning {
            text: "分析".into()
        }
    );
    assert_eq!(
        blocks[1].content,
        TranscriptBlockContent::Text {
            text: "前文".into()
        }
    );
    assert!(section.tool_for_block(&blocks[2]).is_none());
    assert_eq!(
        blocks[3].content,
        TranscriptBlockContent::Text {
            text: "后文".into()
        }
    );
    assert_eq!(item.text, "前文后文"); // Legacy display summary stays compatible.
    assert!(snapshot.tools.is_empty());
}

#[test]
fn pure_attachment_input_is_structured_and_conflicting_legacy_text_stays_unordered() {
    fn read(text: Option<&str>, content: Value) -> TranscriptSnapshot {
        let mut data = json!({"turn":1,"id":"u","content":content});
        if let Some(text) = text {
            data["text"] = json!(text);
        }
        let event: HistoryEntry = serde_json::from_value(
            json!({"event":{"seq":0,"type":"user/message","time":1,"data":data}}),
        )
        .unwrap();
        let mut session = SessionProjection::default();
        session
            .apply_snapshot(JournalSnapshot {
                session_id: "a".into(),
                generation: "g".into(),
                through_sequence: 0,
                window_limit: 50,
                entries: vec![event],
                has_more_before: false,
                projections: None,
                history_revision: None,
            })
            .unwrap();
        TranscriptSnapshot::read(&RunScope::main("e", "a"), &session).unwrap()
    }
    let source = read(None, json!([{"type":"file","id":"f","name":"file.txt"}]));
    let doc = TranscriptDocument::read(&source).unwrap();
    assert_eq!(doc.sections[0].inputs.len(), 1);
    assert!(source.items[0].text.is_empty());
    assert!(matches!(
        source.items[0].content_blocks.as_ref().unwrap()[0].content,
        TranscriptBlockContent::Attachment { .. }
    ));
    let conflict = read(
        Some("legacy different"),
        json!([{"type":"text","text":"structured"}]),
    );
    assert_eq!(conflict.items[0].text, "legacy different");
    assert!(conflict.items[0].content_blocks.is_none());
}

#[test]
fn durable_native_message_key_does_not_shift_when_same_timestamp_history_is_prepended() {
    let retained = vec![
        json!({"type":"message","id":"new-user","message":{"role":"user","timestamp":8,"content":"same input"}}),
        json!({"type":"message","id":"new-answer","message":{"role":"assistant","timestamp":9,"content":"same answer","stopReason":"stop"}}),
    ];
    let before = native(retained.clone(), None);
    let mut expanded = vec![
        json!({"type":"message","id":"old-user","message":{"role":"user","timestamp":8,"content":"same input"}}),
        json!({"type":"message","id":"old-answer","message":{"role":"assistant","timestamp":9,"content":"same answer","stopReason":"stop"}}),
    ];
    expanded.extend(retained);
    let after = native(expanded, None);
    let old = TranscriptDocument::read(&before).unwrap();
    let next = TranscriptDocument::read(&after).unwrap();
    assert_eq!(
        old.sections[0].outputs[0].item.record_id,
        next.sections[1].outputs[0].item.record_id
    );
    assert_eq!(
        old.sections[0].outputs[0].key,
        next.sections[1].outputs[0].key
    );
    assert_ne!(
        old.sections[0].outputs[0].key,
        next.sections[0].outputs[0].key
    );
}

#[test]
fn native_preview_does_not_claim_a_durable_entry_or_guess_an_alias_from_equal_content() {
    let user = json!({"type":"message","id":"u","message":{"role":"user","timestamp":1,"content":"input"}});
    let message =
        json!({"role":"assistant","timestamp":2,"content":"same answer","stopReason":"stop"});
    let live = native(
        vec![user.clone()],
        Some(json!({"type":"message_update","message":message})),
    );
    let recorded = native(
        vec![
            user,
            json!({"type":"message","id":"answer-entry","message":message}),
        ],
        None,
    );
    let before = TranscriptDocument::read(&live).unwrap();
    let after = TranscriptDocument::read(&recorded).unwrap();
    assert_eq!(
        before.sections[0].outputs[0].item.id,
        after.sections[0].outputs[0].item.id
    );
    assert!(matches!(
        before.sections[0].outputs[0].key,
        TranscriptMessageKey::Item(_)
    ));
    assert_eq!(
        after.sections[0].outputs[0].key,
        TranscriptMessageKey::NativeEntry("answer-entry")
    );
    assert_ne!(
        before.sections[0].outputs[0].key,
        after.sections[0].outputs[0].key
    );
    assert!(before.sections[0].outputs[0].item.source_position.is_none());
}

#[test]
fn durable_native_input_key_survives_a_receipt_discovered_in_an_earlier_page() {
    use sha2::{Digest, Sha256};
    let entries = vec![
        json!({"type":"message","id":"u","message":{"role":"user","timestamp":1,"content":"input"}}),
    ];
    let before = native(entries.clone(), None);
    let mut expanded = vec![
        json!({"type":"custom","id":"receipt-entry","customType":"agentkit.inputReceipt.v1","data":{"rpcId":"rpc","timestamp":1,"hash":format!("{:x}",Sha256::digest(b"input"))}}),
    ];
    expanded.extend(entries);
    let after = native(expanded, None);
    assert!(before.items[0].reconciliation_id.is_none());
    assert_eq!(after.items[0].reconciliation_id.as_deref(), Some("rpc"));
    let a = TranscriptDocument::read(&before).unwrap();
    let b = TranscriptDocument::read(&after).unwrap();
    assert_eq!(a.sections[0].inputs[0].key, b.sections[0].inputs[0].key);
    assert_eq!(a.sections[0].inputs[0].reconciliation_key(), None);
    assert_eq!(
        b.sections[0].inputs[0].reconciliation_key(),
        Some(TranscriptMessageKey::Submission("rpc"))
    );
}

#[test]
fn pi_stream_receipt_preserves_canonical_entry_and_suppresses_late_preview() {
    let scope = RunScope::main("e", "a");
    let user = json!({"type":"message","id":"u","message":{"role":"user","timestamp":1,"content":"input"}});
    let final_message = json!({"role":"assistant","timestamp":2,"stopReason":"stop","content":[{"type":"text","text":"answer"}]});
    let preview =
        json!({"type":"message_update","agentkitStreamId":"stream-one","message":final_message});
    let mut session = SessionProjection {
        journal_ready: true,
        native_conversation: Some(NativeConversationSnapshot {
            session_id: "a".into(),
            generation: "g".into(),
            leaf_id: Some("u".into()),
            entries: vec![user],
            has_more_before: false,
            is_streaming: true,
            state: json!({}),
        }),
        native_events: vec![preview].into(),
        ..Default::default()
    };
    let live = TranscriptSnapshot::read(&scope, &session).unwrap();
    let live_doc = TranscriptDocument::read(&live).unwrap();
    let live_output = &live_doc.sections[0].outputs[0];
    assert!(matches!(live_output.key, TranscriptMessageKey::Stream(_)));
    assert_eq!(live_output.reconciliation_key(), None);
    let stream_key = live_output.key;
    let history = session.native_conversation.as_mut().unwrap();
    history
        .entries
        .push(json!({"type":"message","id":"durable","message":final_message}));
    history.leaf_id = Some("durable".into());
    session
        .native_reading_identities
        .push(NativeMessageReadingIdentity {
            stream_id: "stream-one".into(),
            entry_id: "durable".into(),
        });
    let settled = TranscriptSnapshot::read(&scope, &session).unwrap();
    let doc = TranscriptDocument::read(&settled).unwrap();
    assert_eq!(doc.sections[0].outputs.len(), 1);
    let output = &doc.sections[0].outputs[0];
    assert_eq!(output.key, TranscriptMessageKey::NativeEntry("durable"));
    assert_eq!(output.reconciliation_key(), Some(stream_key));
    session.native_events.clear();
    session.native_reading_identities.clear();
    let restored = TranscriptSnapshot::read(&scope, &session).unwrap();
    let doc = TranscriptDocument::read(&restored).unwrap();
    assert_eq!(
        doc.sections[0].outputs[0].key,
        TranscriptMessageKey::NativeEntry("durable")
    );
    assert_eq!(doc.sections[0].outputs[0].reconciliation_key(), None);
}

fn call_before_response() -> TranscriptSnapshot {
    let rows = vec![
        ("turn/start", json!({"turn":1})),
        (
            "user/message",
            json!({"turn":1,"id":"input","content":[{"type":"text","text":"输入"}]}),
        ),
        (
            "tool/call",
            json!({"turn":1,"step":1,"callId":"z","name":"read","arguments":{}}),
        ),
        (
            "tool/call",
            json!({"turn":1,"step":1,"callId":"a","name":"read","arguments":{}}),
        ),
        (
            "assistant/message",
            json!({"turn":1,"step":1,"message":{"id":"response","content":[{"type":"reasoning","text":"思考"},{"type":"text","text":"调用之前的说明"}]}}),
        ),
    ];
    let entries:Vec<_>=rows.into_iter().enumerate().map(|(seq,(kind,data))|json!({"event":{"type":kind,"seq":seq,"time":100-seq,"data":data}})).collect();
    let mut store = MemoryProjectionStore::new("e");
    store.apply("journal:a",json!({"type":"snapshot","snapshot":{"sessionId":"a","generation":"g","throughSequence":4,"windowLimit":50,"hasMoreBefore":false,"entries":entries}})).unwrap();
    store.apply("journal:a",json!({"type":"transient","generation":"g","sessionId":"a","event":{"type":"assistant/chunk","seq":9999,"time":0,"data":{"turn":1,"step":2,"chunk":{"type":"reasoning-delta","delta":"下一步思考"}}}})).unwrap();
    TranscriptSnapshot::read(&RunScope::main("e", "a"), &store.sessions["a"]).unwrap()
}
#[test]
fn display_order_anchors_delayed_commit_before_calls_and_places_preview_without_rewriting_protocol()
{
    let snapshot = call_before_response();
    let original = snapshot.clone();
    let doc = TranscriptDocument::read(&snapshot).unwrap();
    let section = &doc.sections[0];
    let first_id = json!(["e", "a", "tool:z"]).to_string();
    let second_id = json!(["e", "a", "tool:a"]).to_string();
    let raw = section.event_order().unwrap();
    assert!(matches!(raw.ordered[1],TranscriptOrderedEvent::ToolCall(tool) if tool.id==first_id));
    assert_eq!(raw.unplaced.len(), 1);
    let display = section
        .display_order(&RunScope::main("e", "a"), &snapshot.thinking_sources)
        .unwrap();
    assert!(display.unplaced.is_empty());
    assert!(
        matches!(display.ordered[1],TranscriptOrderedEvent::Message(row) if row.text=="调用之前的说明" && row.source_sequence==Some(4))
    );
    assert!(
        matches!(display.ordered[2],TranscriptOrderedEvent::ToolCall(tool) if tool.id==first_id)
    );
    assert!(
        matches!(display.ordered[3],TranscriptOrderedEvent::ToolCall(tool) if tool.id==second_id)
    );
    assert!(
        matches!(display.ordered[4],TranscriptOrderedEvent::Message(row) if row.is_provisional && row.source_position.is_none())
    );
    assert_eq!(section.event_order().unwrap(), raw);
    assert_eq!(snapshot, original);
}
#[test]
fn display_sources_require_scope_and_valid_order_and_legacy_never_guesses_from_time() {
    let snapshot = call_before_response();
    let doc = TranscriptDocument::read(&snapshot).unwrap();
    let section = &doc.sections[0];
    assert!(
        section
            .display_order(&RunScope::main("other", "a"), &snapshot.thinking_sources)
            .is_err()
    );
    let mut sources = (*snapshot.thinking_sources).clone();
    sources.by_id.values_mut().next().unwrap().arrival_order = -1;
    assert!(
        section
            .display_order(&RunScope::main("e", "a"), &sources)
            .is_err()
    );
    let legacy = section
        .display_order(
            &RunScope::main("e", "a"),
            &TranscriptThinkingSources::default(),
        )
        .unwrap();
    let raw = section.event_order().unwrap();
    assert_eq!(legacy.ordered, raw.ordered);
    assert_eq!(legacy.unplaced, raw.unplaced);
}
#[test]
fn native_display_order_rebases_after_paging_but_preview_never_acquires_entry_identity() {
    let entries = native_entries()[..2].to_vec();
    let preview = json!({"type":"message_update","message":{"role":"assistant","timestamp":2,"content":[{"type":"text","text":"streaming"}]}});
    let first = native(entries.clone(), Some(preview.clone()));
    let mut prefixed = vec![
        json!({"type":"message","id":"older-u","message":{"role":"user","timestamp":0,"content":"older"}}),
    ];
    prefixed.extend(entries);
    let next = native(prefixed, Some(preview));
    let a = TranscriptDocument::read(&first).unwrap();
    let b = TranscriptDocument::read(&next).unwrap();
    let before = a.sections[0]
        .display_order(&RunScope::main("e", "a"), &first.thinking_sources)
        .unwrap();
    let after = b.sections[1]
        .display_order(&RunScope::main("e", "a"), &next.thinking_sources)
        .unwrap();
    assert!(before.unplaced.is_empty() && after.unplaced.is_empty());
    for order in [&before, &after] {
        assert!(
            matches!(order.ordered.last(),Some(TranscriptOrderedEvent::Message(row)) if row.is_provisional && row.record_id.is_none() && row.source_position.is_none())
        );
    }
    assert_eq!(first.tools[0].id, next.tools[0].id);
    assert_eq!(a.sections[0].outputs[0].key, b.sections[1].outputs[0].key);
}
