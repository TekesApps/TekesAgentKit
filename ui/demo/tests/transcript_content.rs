use serde_json::json;
use tekes_agent_components_ui::*;
use tekes_agent_presentation::TokenUsage;
use tekes_ui_controls::composition::*;
fn scope() -> RunScope {
    RunScope::main("fixture", "a")
}
fn message(id: &str, kind: TranscriptItemKind, sequence: i64) -> TranscriptItem {
    TranscriptItem {
        id: id.into(),
        record_id: None,
        reconciliation_id: None,
        turn_id: "turn".into(),
        kind,
        text: id.into(),
        reasoning: String::new(),
        attachments: vec![],
        content_blocks: None,
        is_visible: true,
        is_final: true,
        is_provisional: false,
        reasoning_ended: true,
        source_sequence: Some(sequence),
        source_position: Some(TranscriptSourcePosition::Journal {
            sequence,
            content_index: None,
        }),
        time: Some(sequence as f64),
        metadata: json!({}),
    }
}
fn turn() -> TranscriptTurn {
    TranscriptTurn {
        id: "turn".into(),
        status: "completed".into(),
        input: String::new(),
        started_at: None,
        ended_at: None,
        usage: TokenUsage::default(),
        fork_boundary: Some(json!({"atSeq":9})),
        footnote_actions: vec![],
        live_usage: None,
        reported_usage: None,
    }
}
fn snapshot() -> TranscriptSnapshot {
    TranscriptSnapshot {
        items: vec![
            message("input", TranscriptItemKind::User, 0),
            message("output", TranscriptItemKind::Assistant, 2),
        ],
        turns: vec![turn()],
        ..Default::default()
    }
}
fn tool() -> TranscriptToolCall {
    TranscriptToolCall {
        id: "tool".into(),
        turn_id: "turn".into(),
        name: "read".into(),
        arguments: json!({"path":"a.rs"}),
        result: Some(json!({"text":"result"})),
        phase: "done".into(),
        call_view: None,
        result_view: None,
        source_position: Some(TranscriptSourcePosition::Journal {
            sequence: 1,
            content_index: None,
        }),
        result_position: Some(TranscriptSourcePosition::Journal {
            sequence: 3,
            content_index: None,
        }),
    }
}
fn content() -> TranscriptContent {
    TranscriptContent::new(1000, 1001..2000, scope(), Default::default()).unwrap()
}
fn root(host: &mut ContentHost, id: ComponentId) -> &mut TranscriptContent {
    host.get_mut(id)
        .unwrap()
        .component_mut::<TranscriptContent>()
        .unwrap()
}
fn key(id: &str) -> TranscriptContentRowKey {
    TranscriptContentRowKey::Message(TranscriptMessageIdentity::Item(id.into()))
}
#[test]
fn output_copy_uses_the_original_final_message_and_remains_local_when_mutations_are_disabled() {
    let mut source = snapshot();
    let answer = source
        .items
        .iter_mut()
        .find(|item| item.id == "output")
        .unwrap();
    answer.is_final = true;
    answer.reasoning = "private reasoning".into();
    let mut view = content();
    view.update(&scope(), 1, 1, &source, false).unwrap();
    let footer = view
        .row_id(&TranscriptContentRowKey::TurnFootnote("turn".into()))
        .unwrap();
    let id = view.id();
    let mut host = view.into_host().unwrap();
    let button = host
        .get(footer)
        .unwrap()
        .component::<TurnFootnote>()
        .unwrap()
        .copy_button_id()
        .unwrap();
    assert!(
        !host
            .get(button)
            .unwrap()
            .component::<ButtonContent>()
            .unwrap()
            .button
            .state
            .disabled
    );
    host.accessibility_action(button, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    assert!(
        root(&mut host, id).handle_actions(&actions).is_empty(),
        "copy is not an endpoint mutation"
    );
    let copies = root(&mut host, id).take_copy_intents(&actions);
    assert_eq!(copies.len(), 1);
    assert_eq!(copies[0].scope(), &scope());
    assert_eq!(
        copies[0].message(),
        &TranscriptMessageIdentity::Item("output".into())
    );
    assert_eq!(copies[0].text(), "output");
    source
        .items
        .iter_mut()
        .find(|item| item.id == "output")
        .unwrap()
        .text = "updated".into();
    assert!(
        root(&mut host, id)
            .update(&scope(), 1, 1, &source, false)
            .is_err()
    );
    assert_eq!(
        host.get(footer)
            .unwrap()
            .component::<TurnFootnote>()
            .unwrap()
            .copy_button_id(),
        Some(button)
    );
    root(&mut host, id)
        .update(&scope(), 2, 1, &source, false)
        .unwrap();
    assert!(root(&mut host, id).take_copy_intents(&actions).is_empty());
    assert_eq!(copies[0].text(), "output");
    assert!(copies[0].claim());
    assert!(!copies[0].clone().claim());
    let mut second = message("second", TranscriptItemKind::Assistant, 9);
    second.is_final = true;
    source.items.push(second);
    root(&mut host, id)
        .update(&scope(), 3, 1, &source, false)
        .unwrap();
    assert!(
        host.get(footer)
            .unwrap()
            .component::<TurnFootnote>()
            .unwrap()
            .copy_button_id()
            .is_none()
    );
}
#[test]
fn journal_events_compose_once_with_a_turn_aside_and_cumulative_tool_result() {
    let mut source = snapshot();
    source.tools.push(tool());
    let mut view = content();
    view.update(&scope(), 1, 1, &source, true).unwrap();
    assert_eq!(
        view.row_keys(),
        &[
            key("input"),
            TranscriptContentRowKey::Tool("tool".into()),
            key("output"),
            TranscriptContentRowKey::TurnFootnote("turn".into())
        ]
    );
    assert!(!view.capabilities().keyed_reading_transactions && !view.capabilities().rich_content);
    let root_id = view.id();
    let tool_id = view
        .row_id(&TranscriptContentRowKey::Tool("tool".into()))
        .unwrap();
    let mut host = view.into_host().unwrap();
    let ids = host
        .get(tool_id)
        .unwrap()
        .component::<ToolActivity>()
        .unwrap()
        .content_ids()
        .unwrap();
    host.accessibility_action(ids.disclosure.unwrap(), None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    assert!(root(&mut host, root_id).handle_actions(&actions).is_empty());
    assert!(
        host.text_surface(ids.body)
            .unwrap()
            .input
            .text
            .contains("result")
    );
    host.focus(Some(ids.body), true);
    host.get_mut(ids.body)
        .unwrap()
        .component_mut::<TextFieldContent>()
        .unwrap()
        .surface
        .input
        .select_range(0..2);
    source.tools[0].result = Some(json!({"text":"updated result"}));
    root(&mut host, root_id)
        .update(&scope(), 2, 1, &source, true)
        .unwrap();
    assert_eq!(host.focused(), Some(ids.body));
    assert_eq!(host.text_surface(ids.body).unwrap().input.selection(), 0..2);
    assert!(
        host.text_surface(ids.body)
            .unwrap()
            .input
            .text
            .contains("updated result")
    );
}
#[test]
fn tool_inside_a_message_is_not_rendered_again_as_an_independent_activity() {
    let mut source = snapshot();
    source.items[1].content_blocks = Some(vec![
        TranscriptContentBlock {
            index: 0,
            content: TranscriptBlockContent::Text {
                text: "before".into(),
            },
        },
        TranscriptContentBlock {
            index: 1,
            content: TranscriptBlockContent::ToolCall {
                tool_id: "tool".into(),
            },
        },
    ]);
    let mut call = tool();
    call.source_position = Some(TranscriptSourcePosition::Journal {
        sequence: 2,
        content_index: Some(1),
    });
    source.tools.push(call);
    let mut view = content();
    view.update(&scope(), 1, 1, &source, true).unwrap();
    assert!(
        !view
            .row_keys()
            .contains(&TranscriptContentRowKey::Tool("tool".into()))
    );
    let id = view
        .message_id(TranscriptMessageKey::Item("output"))
        .unwrap();
    let host = view.into_host().unwrap();
    assert_eq!(
        host.get(id)
            .unwrap()
            .component::<OutputMessage>()
            .unwrap()
            .content_ids()
            .len(),
        2
    );
}
#[test]
fn unknown_call_location_uses_known_result_location_without_fabricating_a_call_order() {
    let mut source = snapshot();
    let mut call = tool();
    call.source_position = None;
    source.tools.push(call);
    let mut view = content();
    view.update(&scope(), 1, 1, &source, true).unwrap();
    assert_eq!(
        view.row_keys(),
        &[
            key("input"),
            key("output"),
            TranscriptContentRowKey::Tool("tool".into()),
            TranscriptContentRowKey::Unplaced("turn".into()),
            TranscriptContentRowKey::TurnFootnote("turn".into())
        ]
    );
}
#[test]
fn same_native_entry_survives_window_prefix_expansion_and_raw_item_id_changes() {
    let mut retained = message("window:0", TranscriptItemKind::Assistant, 0);
    retained.source_position = Some(TranscriptSourcePosition::Native {
        entry_id: "retained".into(),
        entry_ordinal: 0,
        content_index: None,
    });
    let mut source = TranscriptSnapshot {
        items: vec![retained.clone()],
        turns: vec![turn()],
        ..Default::default()
    };
    let mut view = content();
    view.update(&scope(), 1, 1, &source, true).unwrap();
    let original = view
        .message_id(TranscriptMessageKey::NativeEntry("retained"))
        .unwrap();
    let root_id = view.id();
    let mut host = view.into_host().unwrap();
    let body = host
        .get(original)
        .unwrap()
        .component::<OutputMessage>()
        .unwrap()
        .content_ids()[0]
        .body;
    let mut older = retained.clone();
    older.source_position = Some(TranscriptSourcePosition::Native {
        entry_id: "older".into(),
        entry_ordinal: 0,
        content_index: None,
    });
    retained.id = "window:1".into();
    retained.source_position = Some(TranscriptSourcePosition::Native {
        entry_id: "retained".into(),
        entry_ordinal: 1,
        content_index: None,
    });
    source.items = vec![older, retained];
    root(&mut host, root_id)
        .update(&scope(), 2, 1, &source, true)
        .unwrap();
    assert_eq!(
        root(&mut host, root_id).message_id(TranscriptMessageKey::NativeEntry("retained")),
        Some(original)
    );
    assert_eq!(
        host.get(original)
            .unwrap()
            .component::<OutputMessage>()
            .unwrap()
            .content_ids()[0]
            .body,
        body
    );
}
#[test]
fn only_an_explicit_submission_receipt_can_reuse_the_optimistic_input_widget() {
    for receipt in [true, false] {
        let mut input = message("echo", TranscriptItemKind::User, 0);
        input.is_provisional = true;
        input.reconciliation_id = Some("rpc".into());
        input.source_position = None;
        let mut source = TranscriptSnapshot {
            items: vec![input.clone()],
            ..Default::default()
        };
        let mut view = content();
        view.update(&scope(), 1, 1, &source, true).unwrap();
        let old = view
            .message_id(TranscriptMessageKey::Submission("rpc"))
            .unwrap();
        input.id = "durable".into();
        input.is_provisional = false;
        input.reconciliation_id = receipt.then(|| "rpc".into());
        input.source_position = Some(TranscriptSourcePosition::Native {
            entry_id: "entry".into(),
            entry_ordinal: 0,
            content_index: None,
        });
        source.items = vec![input];
        view.update(&scope(), 2, 1, &source, true).unwrap();
        let new = view
            .message_id(TranscriptMessageKey::NativeEntry("entry"))
            .unwrap();
        assert_eq!(old == new, receipt);
    }
}
#[test]
fn invalid_later_input_never_partially_replaces_preceding_output_or_consumes_namespaces() {
    let mut source = snapshot();
    source.items.swap(0, 1);
    source.items[0].source_position = Some(TranscriptSourcePosition::Journal {
        sequence: 0,
        content_index: None,
    });
    source.items[1].source_position = Some(TranscriptSourcePosition::Journal {
        sequence: 2,
        content_index: None,
    });
    let mut view = content();
    view.update(&scope(), 1, 1, &source, true).unwrap();
    let id = view
        .message_id(TranscriptMessageKey::Item("output"))
        .unwrap();
    let root_id = view.id();
    let mut host = view.into_host().unwrap();
    let body = host
        .get(id)
        .unwrap()
        .component::<OutputMessage>()
        .unwrap()
        .content_ids()[0]
        .body;
    source.items[0].text = "must not appear".into();
    source.items[1].content_blocks = Some(vec![
        TranscriptContentBlock {
            index: 0,
            content: TranscriptBlockContent::Text { text: "a".into() },
        },
        TranscriptContentBlock {
            index: 0,
            content: TranscriptBlockContent::Text { text: "b".into() },
        },
    ]);
    assert!(
        root(&mut host, root_id)
            .update(&scope(), 2, 1, &source, true)
            .is_err()
    );
    assert_eq!(host.text_surface(body).unwrap().input.text, "output");
    source.items[1].content_blocks = None;
    root(&mut host, root_id)
        .update(&scope(), 2, 1, &source, true)
        .unwrap();
    assert_eq!(
        host.text_surface(body).unwrap().input.text,
        "must not appear"
    );
}
#[test]
fn namespace_exhaustion_and_stale_scope_epoch_publications_keep_the_last_readable_tree() {
    let mut source = TranscriptSnapshot {
        items: vec![message("output", TranscriptItemKind::Assistant, 0)],
        ..Default::default()
    };
    let mut view = TranscriptContent::new(10, 11..12, scope(), Default::default()).unwrap();
    view.update(&scope(), 2, 2, &source, true).unwrap();
    let ids = view.row_keys().to_vec();
    assert!(
        view.update(&RunScope::main("fixture", "b"), 3, 2, &source, true)
            .is_err()
    );
    assert!(view.update(&scope(), 1, 2, &source, true).is_err());
    assert!(view.update(&scope(), 3, 1, &source, true).is_err());
    source
        .items
        .push(message("extra", TranscriptItemKind::User, 1));
    assert!(view.update(&scope(), 3, 2, &source, true).is_err());
    assert_eq!(view.row_keys(), ids);
}
#[test]
fn unknown_footnote_operation_survives_clipping_and_cannot_rearm_on_reconnect() {
    let mut target = turn();
    target.footnote_actions = vec![tekes_agent_contract::ThreadActionDefinition {
        id: "forkTurn:turn".into(),
        label: "Fork".into(),
        icon: "fork".into(),
        interaction: "execute".into(),
        enabled: true,
        disabled_reason: None,
        copy_text: None,
    }];
    let source = TranscriptSnapshot {
        turns: vec![target],
        ..Default::default()
    };
    let mut view = content();
    view.update(&scope(), 1, 1, &source, true).unwrap();
    let footer = view
        .row_id(&TranscriptContentRowKey::TurnFootnote("turn".into()))
        .unwrap();
    let id = view.id();
    let mut host = view.into_host().unwrap();
    let button = host
        .get(footer)
        .unwrap()
        .component::<TurnFootnote>()
        .unwrap()
        .action_id("forkTurn:turn")
        .unwrap();
    host.accessibility_action(button, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    let intent = root(&mut host, id).handle_actions(&actions).pop().unwrap();
    intent.complete(TurnActionResult::Unknown("disconnected".into()));
    root(&mut host, id)
        .update(&scope(), 2, 1, &TranscriptSnapshot::default(), true)
        .unwrap();
    assert_eq!(root(&mut host, id).retained_operation_count(), 1);
    root(&mut host, id)
        .update(&scope(), 3, 2, &source, true)
        .unwrap();
    assert_eq!(
        root(&mut host, id).row_id(&TranscriptContentRowKey::TurnFootnote("turn".into())),
        Some(footer)
    );
    let fresh = host
        .get(footer)
        .unwrap()
        .component::<TurnFootnote>()
        .unwrap()
        .action_id("forkTurn:turn")
        .unwrap();
    assert_ne!(fresh, button);
    host.accessibility_action(fresh, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    assert!(root(&mut host, id).handle_actions(&actions).is_empty());
}
#[test]
fn session_notices_and_unplaced_preview_never_acquire_a_fake_turn_or_durable_alias() {
    let mut source = snapshot();
    let mut notice = message("notice", TranscriptItemKind::Notice, 4);
    notice.turn_id.clear();
    notice.source_position = None;
    source.items.push(notice);
    let mut preview = message("preview", TranscriptItemKind::Assistant, 5);
    preview.source_position = None;
    preview.text = "output".into();
    preview.is_provisional = true;
    source.items.push(preview);
    let mut view = content();
    view.update(&scope(), 1, 1, &source, true).unwrap();
    assert_eq!(view.row_keys()[0], TranscriptContentRowKey::SessionNotices);
    assert!(
        view.message_id(TranscriptMessageKey::Item("notice"))
            .is_some()
    );
    assert!(
        view.message_id(TranscriptMessageKey::Item("preview"))
            .is_some()
    );
    assert_ne!(
        view.message_id(TranscriptMessageKey::Item("preview")),
        view.message_id(TranscriptMessageKey::Item("output"))
    );
    assert_eq!(
        view.row_keys()
            .iter()
            .filter(|key| matches!(key, TranscriptContentRowKey::TurnFootnote(_)))
            .count(),
        1
    );
}

#[test]
fn a_tool_with_no_result_does_not_disappear_when_only_a_result_position_is_present() {
    let mut source = snapshot();
    let mut pending = tool();
    pending.source_position = None;
    pending.result = None;
    pending.phase = "running".into();
    source.tools.push(pending);
    let mut view = content();
    view.update(&scope(), 1, 1, &source, true).unwrap();
    assert!(
        view.row_id(&TranscriptContentRowKey::Tool("tool".into()))
            .is_some()
    );
    assert!(
        view.row_keys()
            .iter()
            .any(|key| matches!(key, TranscriptContentRowKey::Unplaced(_)))
    );
}

#[test]
fn two_durable_entries_cannot_both_claim_one_optimistic_widget() {
    let mut echo = message("echo", TranscriptItemKind::User, 0);
    echo.is_provisional = true;
    echo.reconciliation_id = Some("rpc".into());
    echo.source_position = None;
    let mut source = TranscriptSnapshot {
        items: vec![echo.clone()],
        ..Default::default()
    };
    let mut view = TranscriptContent::new(1000, 1001..1003, scope(), Default::default()).unwrap();
    view.update(&scope(), 1, 1, &source, true).unwrap();
    let original = view
        .message_id(TranscriptMessageKey::Submission("rpc"))
        .unwrap();
    let mut one = echo;
    one.id = "one".into();
    one.is_provisional = false;
    one.source_position = Some(TranscriptSourcePosition::Native {
        entry_id: "one".into(),
        entry_ordinal: 0,
        content_index: None,
    });
    let mut two = one.clone();
    two.id = "two".into();
    two.source_position = Some(TranscriptSourcePosition::Native {
        entry_id: "two".into(),
        entry_ordinal: 1,
        content_index: None,
    });
    source.items = vec![one.clone(), two];
    assert!(view.update(&scope(), 2, 1, &source, true).is_err());
    assert_eq!(
        view.message_id(TranscriptMessageKey::Submission("rpc")),
        Some(original)
    );
    source.items = vec![one];
    view.update(&scope(), 2, 1, &source, true).unwrap();
    assert_eq!(
        view.message_id(TranscriptMessageKey::NativeEntry("one")),
        Some(original)
    );
}

fn feedback_snapshot() -> TranscriptSnapshot {
    let mut source = snapshot();
    let answer = source.items.iter_mut().find(|i| i.id == "output").unwrap();
    answer.record_id = Some("public-output".into());
    answer.metadata = json!({"feedbackMessageId":"public-output"});
    source
}
#[test]
fn feedback_mounts_only_for_a_proven_final_response_and_retires_corrected_payload_ids() {
    let mut source = feedback_snapshot();
    let mut view = content();
    view.configure_feedback(Some(Default::default())).unwrap();
    view.update(&scope(), 1, 1, &source, true).unwrap();
    let feedback = view.feedback_controls().next().unwrap().0;
    let keys = view.row_keys();
    let pos = keys
        .iter()
        .position(|k| matches!(k, TranscriptContentRowKey::TurnFeedback(_)))
        .unwrap();
    assert!(matches!(
        &keys[pos - 1],
        TranscriptContentRowKey::TurnFootnote(_)
    ));
    source.items[1].time = Some(200.);
    view.update(&scope(), 2, 1, &source, true).unwrap();
    assert_eq!(view.feedback_controls().next().unwrap().0, feedback);
    source.items[1].text = "corrected response".into();
    view.update(&scope(), 3, 1, &source, true).unwrap();
    assert_ne!(view.feedback_controls().next().unwrap().0, feedback);
    source.items[1].metadata = json!({});
    view.update(&scope(), 4, 1, &source, true).unwrap();
    assert_eq!(view.feedback_controls().count(), 0);
}
#[test]
fn composing_feedback_preflights_response_replacement_and_keeps_the_original_text_field() {
    let mut source = feedback_snapshot();
    let mut view = content();
    view.configure_feedback(Some(Default::default())).unwrap();
    view.update(&scope(), 1, 1, &source, true).unwrap();
    let feedback = view.feedback_controls().next().unwrap().0;
    let target = view
        .feedback_controls()
        .next()
        .unwrap()
        .1
        .target()
        .unwrap()
        .clone();
    view.feedback_mut(feedback)
        .unwrap()
        .update(FeedbackControlState {
            target,
            publication: 1,
            connection_epoch: 1,
            current: Some(
                serde_json::from_value(
                    json!({"messageId":"public-output","rating":"positive","version":"v"}),
                )
                .unwrap(),
            ),
            put_available: true,
            delete_available: true,
        })
        .unwrap();
    let id = view.id();
    let mut host = view.into_host().unwrap();
    let note = root(&mut host, id)
        .feedback_mut(feedback)
        .unwrap()
        .ids()
        .edit_note;
    host.accessibility_action(note, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    assert!(
        root(&mut host, id)
            .take_feedback_intents(&actions)
            .is_empty()
    );
    let editor = root(&mut host, id)
        .feedback_mut(feedback)
        .unwrap()
        .ids()
        .editor;
    host.focus(Some(editor), true);
    host.event(&ContentEvent::Preedit {
        text: "组字".into(),
        selection: 2..2,
        clauses: vec![],
        replacement: None,
    });
    let actions = host.drain_actions();
    root(&mut host, id).take_feedback_intents(&actions);
    source.items[1].text = "corrected response".into();
    assert!(
        root(&mut host, id)
            .update(&scope(), 2, 1, &source, true)
            .is_err()
    );
    assert_eq!(
        root(&mut host, id).composing_feedback_editor(),
        Some(editor)
    );
    host.event(&ContentEvent::Commit {
        text: "中文".into(),
        replacement: None,
    });
    let actions = host.drain_actions();
    root(&mut host, id).take_feedback_intents(&actions);
    root(&mut host, id)
        .update(&scope(), 2, 1, &source, true)
        .unwrap();
    assert_ne!(
        root(&mut host, id).feedback_controls().next().unwrap().0,
        feedback
    );
}
