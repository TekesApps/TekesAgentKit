use serde_json::json;
use tekes_agent_presentation::*;
fn item(id: &str, turn: &str, kind: TranscriptItemKind) -> TranscriptItem {
    TranscriptItem {
        id: id.into(),
        record_id: None,
        reconciliation_id: None,
        turn_id: turn.into(),
        kind,
        text: String::new(),
        reasoning: String::new(),
        attachments: vec![],
        content_blocks: None,
        is_visible: true,
        is_final: false,
        is_provisional: false,
        reasoning_ended: false,
        source_sequence: None,
        source_position: None,
        time: None,
        metadata: json!({}),
    }
}
fn turn(id: &str) -> TranscriptTurn {
    TranscriptTurn {
        id: id.into(),
        status: "running".into(),
        input: String::new(),
        started_at: None,
        ended_at: None,
        usage: TokenUsage::default(),
        fork_boundary: None,
        footnote_actions: vec![],
        live_usage: None,
        reported_usage: None,
    }
}
#[test]
fn feedback_uses_durable_native_identity_even_for_non_text_final_responses() {
    let mut completed = turn("t");
    completed.status = "completed".into();
    let mut response = item("temporary-display-id", "t", TranscriptItemKind::Assistant);
    response.record_id = Some("native-entry".into());
    response.is_final = true;
    response.attachments.push(json!({"type":"image"}));
    response.source_position = Some(TranscriptSourcePosition::Native {
        entry_id: "native-entry".into(),
        entry_ordinal: 3,
        content_index: None,
    });
    let mut source = TranscriptSnapshot {
        turns: vec![completed],
        items: vec![response],
        ..Default::default()
    };
    let document = TranscriptDocument::read(&source).unwrap();
    let target = TurnFeedbackTarget::read(&document.sections[0]).unwrap();
    assert_eq!(target.message_id, "native-entry");
    assert!(
        document.sections[0].final_output().is_none(),
        "rating an image is not copying invented prose"
    );
    source.items[0].record_id = Some("mismatched".into());
    let document = TranscriptDocument::read(&source).unwrap();
    assert!(TurnFeedbackTarget::read(&document.sections[0]).is_none());
}
#[test]
fn journal_feedback_requires_an_explicit_protocol_id_not_a_display_fallback() {
    use tekes_agent_contract::{HistoryEntry, JournalSnapshot};
    use tekes_agent_session::SessionProjection;
    for explicit in [false, true] {
        let mut message = json!({"content":[{"type":"text","text":"answer"}]});
        if explicit {
            message["id"] = json!("assistant-1");
        }
        let rows = [
            ("turn/start", json!({"turn":1})),
            (
                "assistant/message",
                json!({"turn":1,"step":1,"message":message}),
            ),
            ("turn/end", json!({"turn":1,"reason":"completed"})),
        ];
        let entries: Vec<HistoryEntry> = rows
            .into_iter()
            .enumerate()
            .map(|(seq, (kind, data))| {
                serde_json::from_value(
                    json!({"event":{"seq":seq,"time":1,"type":kind,"data":data}}),
                )
                .unwrap()
            })
            .collect();
        let mut projection = SessionProjection::default();
        projection
            .apply_snapshot(JournalSnapshot {
                session_id: "a".into(),
                generation: "g".into(),
                through_sequence: 2,
                window_limit: 50,
                entries,
                has_more_before: false,
                projections: None,
                history_revision: None,
            })
            .unwrap();
        let snapshot = TranscriptSnapshot::read(&RunScope::main("e", "a"), &projection).unwrap();
        let document = TranscriptDocument::read(&snapshot).unwrap();
        let section = &document.sections[0];
        assert_eq!(
            section.final_response().unwrap().item.record_id.as_deref(),
            Some("assistant-1")
        );
        assert_eq!(TurnFeedbackTarget::read(section).is_some(), explicit);
    }
}
#[test]
fn captured_feedback_response_survives_native_window_ordinals_but_not_changed_content() {
    let mut completed = turn("t");
    completed.status = "completed".into();
    let mut response = item("old-window-id", "t", TranscriptItemKind::Assistant);
    response.record_id = Some("entry".into());
    response.is_final = true;
    response.text = "answer".into();
    response.source_position = Some(TranscriptSourcePosition::Native {
        entry_id: "entry".into(),
        entry_ordinal: 0,
        content_index: None,
    });
    let mut source = TranscriptSnapshot {
        turns: vec![completed],
        items: vec![response],
        ..Default::default()
    };
    let document = TranscriptDocument::read(&source).unwrap();
    let captured = TurnFeedbackTarget::read(&document.sections[0])
        .unwrap()
        .capture(&RunScope::main("e", "s"))
        .unwrap();
    source.items[0].id = "new-window-id".into();
    if let Some(TranscriptSourcePosition::Native { entry_ordinal, .. }) =
        &mut source.items[0].source_position
    {
        *entry_ordinal = 100;
    }
    let document = TranscriptDocument::read(&source).unwrap();
    assert!(captured.matches(TurnFeedbackTarget::read(&document.sections[0]).unwrap()));
    source.items[0].text = "different answer".into();
    let document = TranscriptDocument::read(&source).unwrap();
    assert!(!captured.matches(TurnFeedbackTarget::read(&document.sections[0]).unwrap()));
}
#[test]
fn final_output_uses_shared_final_marker_instead_of_the_last_text_or_reasoning() {
    let mut completed = turn("t");
    completed.status = "completed".into();
    let mut answer = item("answer", "t", TranscriptItemKind::Assistant);
    answer.is_final = true;
    answer.text = "最终 **答案**\n保留原文".into();
    answer.reasoning = "不要复制的推理".into();
    let mut later = item("later", "t", TranscriptItemKind::Assistant);
    later.text = "后来的非最终文本".into();
    let source = TranscriptSnapshot {
        turns: vec![completed],
        items: vec![answer, later],
        ..Default::default()
    };
    let document = TranscriptDocument::read(&source).unwrap();
    let output = document.sections[0].final_output().unwrap();
    assert_eq!(output.key, TranscriptMessageKey::Item("answer"));
    assert_eq!(output.item.text, "最终 **答案**\n保留原文");
}
#[test]
fn incomplete_ambiguous_and_non_prose_outputs_have_no_turn_copy_target() {
    let mut completed = turn("t");
    completed.status = "completed".into();
    let mut answer = item("answer", "t", TranscriptItemKind::Assistant);
    answer.is_final = true;
    answer.text = "正文".into();
    let baseline = TranscriptSnapshot {
        turns: vec![completed],
        items: vec![answer],
        ..Default::default()
    };
    for case in [
        "live",
        "missing-turn",
        "unmarked",
        "provisional",
        "hidden",
        "reasoning-only",
        "ambiguous",
        "status",
    ] {
        let mut source = baseline.clone();
        match case {
            "live" => source.turns[0].status = "running".into(),
            "missing-turn" => source.turns.clear(),
            "unmarked" => source.items[0].is_final = false,
            "provisional" => source.items[0].is_provisional = true,
            "hidden" => source.items[0].is_visible = false,
            "reasoning-only" => {
                source.items[0].text.clear();
                source.items[0].reasoning = "推理".into();
            }
            "ambiguous" => {
                let mut second = source.items[0].clone();
                second.id = "second".into();
                source.items.push(second);
            }
            "status" => source.items[0].kind = TranscriptItemKind::Notice,
            _ => unreachable!(),
        }
        let document = TranscriptDocument::read(&source).unwrap();
        assert!(
            document
                .sections
                .iter()
                .all(|section| section.final_output().is_none()),
            "{case}"
        );
    }
}
#[test]
fn whole_input_bubble_keeps_attachments_and_pending_footnote_separate() {
    let mut input = item("echo", "local", TranscriptItemKind::User);
    input.reconciliation_id = Some("rpc".into());
    input.is_provisional = true;
    input.time = Some(123.0); // Even a bad host time cannot turn pending into recorded.
    input.text = "第一段\n\n- 列表\n\n```rust\nfn main() {}\n```".into();
    input
        .attachments
        .push(json!({"type":"file","id":"attachment"}));
    let snapshot = TranscriptSnapshot {
        items: vec![input.clone()],
        ..Default::default()
    };
    let doc = TranscriptDocument::read(&snapshot).unwrap();
    let row = &doc.sections[0].inputs[0];
    assert_eq!(row.key, TranscriptMessageKey::Submission("rpc"));
    assert_eq!(row.footnote, TranscriptDelivery::Pending);
    assert!(std::ptr::eq(row.item, &snapshot.items[0]));
    assert_eq!(row.item.text, input.text);
    assert_eq!(row.item.attachments.len(), 1);
    assert!(doc.sections[0].turn.is_none());
    let mut recorded = snapshot.clone();
    recorded.items[0].id = "server-record".into();
    recorded.items[0].turn_id = "canonical-turn".into();
    recorded.items[0].is_provisional = false;
    let next = TranscriptDocument::read(&recorded).unwrap();
    assert_eq!(row.key, next.sections[0].inputs[0].key);
    assert_eq!(
        next.sections[0].inputs[0].footnote,
        TranscriptDelivery::Recorded { time: Some(123.0) }
    );
    recorded.items[0].text.clear();
    assert_eq!(
        TranscriptDocument::read(&recorded).unwrap().sections[0]
            .inputs
            .len(),
        1
    );
}
#[test]
fn activity_output_and_shared_footnote_are_lossless_and_do_not_infer_settlement() {
    let mut answer = item("answer", "turn", TranscriptItemKind::Assistant);
    answer.reasoning = "思考中".into();
    answer.text = "流式正文".into();
    let tool = TranscriptToolCall {
        result_meta: None,
        result_error: None,
        id: "tool".into(),
        turn_id: "turn".into(),
        name: "read".into(),
        arguments: json!({"path":"file"}),
        result: Some(json!({"content":"result"})),
        phase: "done".into(),
        call_view: Some(json!({"kind":"file"})),
        result_view: None,
        source_position: None,
        result_position: None,
    };
    let snapshot = TranscriptSnapshot {
        turns: vec![turn("turn")],
        items: vec![answer],
        tools: vec![tool],
        ..Default::default()
    };
    let doc = TranscriptDocument::read(&snapshot).unwrap();
    let section = &doc.sections[0];
    assert!(section.inputs.is_empty());
    assert_eq!(section.outputs[0].item.reasoning, "思考中");
    assert_eq!(section.outputs[0].item.text, "流式正文");
    assert!(std::ptr::eq(section.tools[0], &snapshot.tools[0]));
    assert!(std::ptr::eq(
        section.output_footnote().unwrap(),
        &snapshot.turns[0]
    ));
    assert_eq!(section.output_footnote().unwrap().status, "running");
    assert!(section.output_footnote().unwrap().reported_usage.is_none());
}
#[test]
fn earlier_page_and_stream_settlement_keep_existing_message_keys() {
    let base = TranscriptSnapshot {
        turns: vec![turn("new")],
        items: vec![item("a", "new", TranscriptItemKind::Assistant)],
        has_more_before: true,
        ..Default::default()
    };
    let before = TranscriptDocument::read(&base).unwrap();
    let mut after = base.clone();
    after.turns.insert(0, turn("old"));
    after
        .items
        .insert(0, item("history", "old", TranscriptItemKind::User));
    after.items[1].text = "追加的中文".into();
    after.items[1].is_final = true;
    after.items[1].reasoning_ended = true;
    after.has_more_before = false;
    let next = TranscriptDocument::read(&after).unwrap();
    assert_eq!(before.sections[0].id, next.sections[1].id);
    assert_eq!(
        before.sections[0].outputs[0].key,
        next.sections[1].outputs[0].key
    );
    assert!(!next.has_more_before);
    assert!(next.sections[1].outputs[0].item.is_final);
}
#[test]
fn clipped_orphan_tools_and_notices_are_preserved_without_synthetic_turns() {
    let mut hidden = item("hidden", "orphan", TranscriptItemKind::Assistant);
    hidden.is_visible = false;
    let snapshot = TranscriptSnapshot {
        items: vec![hidden, item("notice", "orphan", TranscriptItemKind::Notice)],
        tools: vec![TranscriptToolCall {
            result_meta: None,
            result_error: None,
            id: "call".into(),
            turn_id: "only-tool".into(),
            name: "read".into(),
            arguments: json!({}),
            result: None,
            phase: "doing".into(),
            call_view: None,
            result_view: None,
            source_position: None,
            result_position: None,
        }],
        is_stale: true,
        diagnostics: vec!["partial".into()],
        ..Default::default()
    };
    let doc = TranscriptDocument::read(&snapshot).unwrap();
    assert_eq!(doc.sections.len(), 2);
    assert!(
        doc.sections
            .iter()
            .all(|s| s.turn.is_none() && s.output_footnote().is_none())
    );
    assert_eq!(doc.sections[0].outputs.len(), 1);
    assert_eq!(
        doc.sections[0].outputs[0].item.kind,
        TranscriptItemKind::Notice
    );
    assert_eq!(doc.sections[1].tools.len(), 1);
    assert!(doc.is_stale);
    assert_eq!(doc.diagnostics, &["partial"]);
}
#[test]
fn ambiguous_or_empty_identities_fail_instead_of_rebinding_reading_targets() {
    let mut snapshot = TranscriptSnapshot {
        items: vec![
            item("one", "turn", TranscriptItemKind::User),
            item("two", "turn", TranscriptItemKind::User),
        ],
        ..Default::default()
    };
    snapshot.items[0].reconciliation_id = Some("rpc".into());
    snapshot.items[1].reconciliation_id = Some("rpc".into());
    assert!(TranscriptDocument::read(&snapshot).is_err());
    snapshot.items[1].reconciliation_id = None;
    snapshot.items[1].id = "one".into();
    assert!(TranscriptDocument::read(&snapshot).is_err());
    snapshot.items[1].id.clear();
    assert!(TranscriptDocument::read(&snapshot).is_err());
    snapshot.items.clear();
    snapshot.turns = vec![turn("same"), turn("same")];
    assert!(TranscriptDocument::read(&snapshot).is_err());
}

#[test]
fn public_session_notices_are_not_rejected_or_assigned_to_a_fake_turn() {
    use tekes_agent_contract::{HistoryEntry, JournalSnapshot};
    use tekes_agent_session::SessionProjection;
    let notice: HistoryEntry = serde_json::from_value(json!({"event":{
        "type":"session/notice","seq":0,"time":100,
        "data":{"classification":"worker_launch","severity":"error","message":"Worker unavailable"}
    }}))
    .unwrap();
    let mut session = SessionProjection::default();
    session
        .apply_snapshot(JournalSnapshot {
            session_id: "a".into(),
            generation: "g".into(),
            through_sequence: 0,
            window_limit: 50,
            entries: vec![notice],
            has_more_before: false,
            projections: None,
            history_revision: None,
        })
        .unwrap();
    let scope = RunScope::main("e", "a");
    let source = TranscriptSnapshot::read(&scope, &session).unwrap();
    assert!(source.items[0].turn_id.is_empty());
    let document = TranscriptDocument::read(&source).expect("valid public session/notice");
    assert!(document.sections.is_empty());
    assert_eq!(document.session_notices.len(), 1);
    assert!(std::ptr::eq(
        document.session_notices[0].item,
        &source.items[0]
    ));
    assert_eq!(document.session_notices[0].item.metadata["status"], "error");
    session
        .apply_live(
            serde_json::from_value(
                json!({"event":{"type":"turn/start","seq":1,"time":101,"data":{"turn":1}}}),
            )
            .unwrap(),
        )
        .unwrap();
    let recovered = TranscriptSnapshot::read(&scope, &session).unwrap();
    let next = TranscriptDocument::read(&recovered).unwrap();
    assert_eq!(document.session_notices[0].key, next.session_notices[0].key);
    assert_eq!(next.session_notices[0].item.metadata["status"], "recovered");
    assert!(
        next.sections
            .iter()
            .all(|section| section.inputs.is_empty() && section.outputs.is_empty())
    );
    let malformed = TranscriptSnapshot {
        items: vec![item("user-without-turn", "", TranscriptItemKind::User)],
        ..Default::default()
    };
    assert!(TranscriptDocument::read(&malformed).is_err());
}

#[test]
fn native_entries_are_validated_independently_of_submission_and_item_namespaces() {
    let mut input = item("user", "turn", TranscriptItemKind::User);
    input.reconciliation_id = Some("rpc".into());
    input.source_position = Some(TranscriptSourcePosition::Native {
        entry_id: "entry".into(),
        entry_ordinal: 0,
        content_index: None,
    });
    let mut output = item("answer", "turn", TranscriptItemKind::Assistant);
    output.source_position = Some(TranscriptSourcePosition::Native {
        entry_id: "entry".into(),
        entry_ordinal: 1,
        content_index: None,
    });
    let mut snapshot = TranscriptSnapshot {
        items: vec![input, output],
        ..Default::default()
    };
    assert!(TranscriptDocument::read(&snapshot).is_err());
    if let Some(TranscriptSourcePosition::Native { entry_id, .. }) =
        &mut snapshot.items[1].source_position
    {
        *entry_id = "answer-entry".into();
    }
    let doc = TranscriptDocument::read(&snapshot).unwrap();
    assert_eq!(
        doc.sections[0].inputs[0].key,
        TranscriptMessageKey::NativeEntry("entry")
    );
    assert_eq!(
        doc.sections[0].inputs[0].reconciliation_key(),
        Some(TranscriptMessageKey::Submission("rpc"))
    );
    assert_eq!(
        doc.sections[0].outputs[0].key,
        TranscriptMessageKey::NativeEntry("answer-entry")
    );
    assert_ne!(
        doc.sections[0].outputs[0].key,
        TranscriptMessageKey::Item("answer-entry")
    );
    let mut duplicate_submission = snapshot.clone();
    let mut repeated = snapshot.items[0].clone();
    repeated.id = "another-user".into();
    repeated.source_position = Some(TranscriptSourcePosition::Native {
        entry_id: "another-user-entry".into(),
        entry_ordinal: 2,
        content_index: None,
    });
    duplicate_submission.items.push(repeated);
    assert!(TranscriptDocument::read(&duplicate_submission).is_err());
    if let Some(TranscriptSourcePosition::Native { entry_id, .. }) =
        &mut snapshot.items[0].source_position
    {
        entry_id.clear();
    }
    assert!(TranscriptDocument::read(&snapshot).is_err());
}

#[test]
fn document_rejects_duplicate_or_descending_visible_content_indices_but_keeps_gaps() {
    for indices in [[1, 1], [4, 1], [1, 4]] {
        let mut message = item("message", "turn", TranscriptItemKind::Assistant);
        message.content_blocks = Some(
            indices
                .into_iter()
                .map(|index| TranscriptContentBlock {
                    index,
                    content: TranscriptBlockContent::Text {
                        text: "body".into(),
                    },
                })
                .collect(),
        );
        let snapshot = TranscriptSnapshot {
            items: vec![message],
            ..Default::default()
        };
        assert_eq!(
            TranscriptDocument::read(&snapshot).is_ok(),
            indices == [1, 4]
        );
    }
}
#[test]
fn turn_action_catalog_cannot_have_empty_or_repeated_identities() {
    let action = tekes_agent_contract::ThreadActionDefinition {
        id: "fork".into(),
        label: "fork".into(),
        icon: "fork".into(),
        interaction: "execute".into(),
        enabled: true,
        disabled_reason: None,
        copy_text: None,
    };
    let mut snapshot = TranscriptSnapshot {
        turns: vec![turn("turn")],
        ..Default::default()
    };
    snapshot.turns[0].footnote_actions = vec![action.clone(), action.clone()];
    assert!(TranscriptDocument::read(&snapshot).is_err());
    snapshot.turns[0].footnote_actions = vec![action];
    assert!(TranscriptDocument::read(&snapshot).is_ok());
    snapshot.turns[0].footnote_actions[0].id.clear();
    assert!(TranscriptDocument::read(&snapshot).is_err());
}
