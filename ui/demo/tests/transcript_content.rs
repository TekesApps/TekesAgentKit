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
        result_meta: None,
        result_error: None,
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
// These cases inspect historical Activity controls after an explicit user selection.
fn expand_activity(
    view: &mut TranscriptContent,
    source: &TranscriptSnapshot,
    publication: u64,
    epoch: u64,
    available: bool,
) {
    view.set_activity_expanded("turn", true).unwrap();
    view.update(&scope(), publication, epoch, source, available)
        .unwrap();
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
fn rail_targets_reference_actual_turn_rows_and_exclude_blank_turns_and_session_notices() {
    let mut source = snapshot();
    source.turns[0].input = "中文输入".into();
    source.tools.push(tool());
    let mut blank = turn();
    blank.id = "blank".into();
    blank.input = " \n\t".into();
    source.turns.push(blank);
    let mut notice = message("notice", TranscriptItemKind::Notice, 4);
    notice.turn_id.clear();
    source.items.push(notice);
    let mut view = content();
    view.update(&scope(), 1, 5, &source, false).unwrap();
    expand_activity(&mut view, &source, 1, 5, false);
    let targets = view.turn_targets().unwrap();
    assert_eq!(targets.len(), 1);
    let target = &targets[0];
    assert_eq!(target.scope(), &scope());
    assert_eq!((target.publication(), target.connection_epoch()), (1, 5));
    assert_eq!(target.turn_id(), "turn");
    assert_eq!(target.input(), "中文输入");
    assert_eq!(target.status(), "completed");
    assert_eq!(target.is_visible(), None);
    for key in [
        key("input"),
        key("output"),
        TranscriptContentRowKey::Tool("tool".into()),
        TranscriptContentRowKey::TurnFootnote("turn".into()),
    ] {
        assert!(target.row_ids().contains(&view.row_id(&key).unwrap()));
    }
    assert!(
        !target
            .row_ids()
            .contains(&view.row_id(&key("notice")).unwrap())
    );
    assert!(
        !target.row_ids().contains(
            &view
                .row_id(&TranscriptContentRowKey::TurnFootnote("blank".into()))
                .unwrap()
        )
    );
    view.validate_turn_target(target).unwrap();
}

#[test]
fn rail_target_capture_rejects_other_trees_publications_epochs_and_clipped_turns() {
    let mut source = snapshot();
    source.turns[0].input = "原输入".into();
    let mut view = content();
    view.update(&scope(), 1, 5, &source, false).unwrap();
    let captured = view.turn_targets().unwrap().remove(0);
    let mut same_ids = content();
    same_ids.update(&scope(), 1, 5, &source, false).unwrap();
    assert_eq!(
        same_ids.turn_targets().unwrap()[0].row_ids(),
        captured.row_ids()
    );
    assert!(same_ids.validate_turn_target(&captured).is_err());
    let mut other = TranscriptContent::new(2100, 2101..2200, scope(), Default::default()).unwrap();
    other.update(&scope(), 1, 5, &source, false).unwrap();
    assert!(other.validate_turn_target(&captured).is_err());
    view.update(&scope(), 2, 5, &source, false).unwrap();
    assert!(view.validate_turn_target(&captured).is_err());
    let captured = view.turn_targets().unwrap().remove(0);
    view.update(&scope(), 3, 6, &source, false).unwrap();
    assert!(view.validate_turn_target(&captured).is_err());
    let captured = view.turn_targets().unwrap().remove(0);
    view.update(&scope(), 4, 6, &TranscriptSnapshot::default(), false)
        .unwrap();
    assert!(view.turn_targets().unwrap().is_empty());
    assert!(view.validate_turn_target(&captured).is_err());
}

#[test]
fn rail_layout_uses_source_track_gutter_and_document_coordinates() {
    use tekes_ui_scene::Rect;
    let pane = Rect::new(20., 40., 900., 900.);
    let regular = TurnRailGeometry::for_run(pane, RunColumnLayout::default(), 3);
    assert_eq!(regular.rail, Rect::new(20., 40., 65., 900.));
    assert_eq!(regular.track, Rect::new(28., 475., 42., 30.));
    assert!(!regular.centers_ticks);
    assert_eq!(
        regular.tick_rect(0, TurnRailGeometry::RESTING_LENGTH),
        Some(Rect::new(16., 4., 6., 2.))
    );
    assert_eq!(
        regular.tick_rect(2, f32::MAX),
        Some(Rect::new(16., 24., 26., 2.))
    );
    assert_eq!(regular.tick_rect(3, 6.), None);
    let overflow = TurnRailGeometry::for_run(pane, RunColumnLayout::default(), 100);
    assert_eq!(overflow.document_size.height, 1000.);
    assert_eq!(overflow.track, Rect::new(28., 175., 42., 630.));
    let narrow = TurnRailGeometry::for_run(
        Rect::new(0., 0., 400., 100.),
        RunColumnLayout::document_pane(0.01, Some(0.45)),
        1,
    );
    assert_eq!(narrow.rail.size.width, 4.);
    assert!(narrow.centers_ticks);
    assert_eq!(narrow.tick_rect(0, 26.), Some(Rect::new(0., 4., 4., 2.)));
    assert_eq!(narrow.tick_rect(0, f32::NAN).unwrap().size.width, 0.);
    let full = TurnRailGeometry::for_run(
        Rect::new(0., 0., 1600., 900.),
        RunColumnLayout::FullWidth,
        0,
    );
    assert_eq!(full.rail.size.width, 415.);
    assert_eq!(full.track.size.height, 0.);
    let empty = TurnRailGeometry::for_run(
        Rect::new(0., 0., f32::NAN, -3.),
        RunColumnLayout::default(),
        1,
    );
    assert_eq!(empty.rail.size.width, 0.);
    assert_eq!(empty.track.size.height, 0.);
}
#[test]
fn output_copy_captures_the_original_turn_and_remains_local_when_mutations_are_disabled() {
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
        copies[0].source(),
        &TranscriptCopySource::TurnOutput {
            turn_id: "turn".into(),
            item_ids: vec!["output".into()]
        }
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
            .is_some()
    );
    assert!(
        tekes_agent_presentation::TranscriptDocument::read(&source)
            .unwrap()
            .sections[0]
            .final_response()
            .is_none(),
        "copy aggregation must not weaken ambiguous response authority"
    );
}
#[test]
fn journal_events_compose_once_with_a_turn_aside_and_cumulative_tool_result() {
    let mut source = snapshot();
    source.tools.push(tool());
    let mut view = content();
    view.update(&scope(), 1, 1, &source, true).unwrap();
    expand_activity(&mut view, &source, 1, 1, true);
    assert_eq!(
        view.row_keys(),
        &[
            key("input"),
            TranscriptContentRowKey::ActivityDisclosure("turn".into()),
            TranscriptContentRowKey::Tool("tool".into()),
            key("output"),
            TranscriptContentRowKey::TurnFootnote("turn".into())
        ]
    );
    assert!(!view.capabilities().keyed_reading_transactions && !view.capabilities().rich_content);
    let root_id = view.id();
    let tool_id = view.tool_activity_id("tool").unwrap();
    let mut host = view.into_host().unwrap();
    let ids = host
        .get(tool_id)
        .unwrap()
        .component::<ToolActivity>()
        .unwrap()
        .content_ids()
        .unwrap();
    let output_body = host
        .get(tool_id)
        .unwrap()
        .component::<ToolActivity>()
        .unwrap()
        .tool_detail_sections()[1]
        .body;
    host.accessibility_action(ids.disclosure.unwrap(), None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    assert!(root(&mut host, root_id).handle_actions(&actions).is_empty());
    assert!(
        host.text_surface(output_body)
            .unwrap()
            .input
            .text
            .contains("result")
    );
    host.focus(Some(output_body), true);
    host.get_mut(output_body)
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
    assert_eq!(host.focused(), Some(output_body));
    assert_eq!(
        host.text_surface(output_body).unwrap().input.selection(),
        8..10 // SDK Preserve follows the originally selected "re" after the prefix insertion.
    );
    assert!(
        host.text_surface(output_body)
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
    assert_eq!(
        view.message_content_ids(&TranscriptMessageIdentity::Item("output".into()))
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
    expand_activity(&mut view, &source, 1, 1, true);
    assert_eq!(
        view.row_keys(),
        &[
            key("input"),
            TranscriptContentRowKey::ActivityDisclosure("turn".into()),
            TranscriptContentRowKey::Tool("tool".into()),
            TranscriptContentRowKey::Unplaced("turn".into()),
            key("output"),
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
    expand_activity(&mut view, &source, 1, 1, true);
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
    expand_activity(&mut view, &source, 1, 1, true);
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
    assert!(
        !view
            .row_keys()
            .iter()
            .any(|key| matches!(key, TranscriptContentRowKey::TurnFeedback(_)))
    );
    assert_eq!(
        view.row_id(&TranscriptContentRowKey::TurnFeedback("turn".into())),
        Some(feedback)
    );
    let old_positive = view.feedback_controls().next().unwrap().1.ids().positive;
    source.items[1].time = Some(200.);
    view.update(&scope(), 2, 1, &source, true).unwrap();
    assert_eq!(view.feedback_controls().next().unwrap().0, feedback);
    source.items[1].text = "corrected response".into();
    view.update(&scope(), 3, 1, &source, true).unwrap();
    assert_eq!(view.feedback_controls().next().unwrap().0, feedback);
    assert_ne!(
        view.feedback_controls().next().unwrap().1.ids().positive,
        old_positive
    );
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
    assert_eq!(
        root(&mut host, id).feedback_controls().next().unwrap().0,
        feedback
    );
    assert_ne!(
        root(&mut host, id)
            .feedback_controls()
            .next()
            .unwrap()
            .1
            .ids()
            .editor,
        editor
    );
}

#[test]
fn explicit_stream_receipt_retains_output_row_text_surface_and_selection() {
    let mut item = message("preview", TranscriptItemKind::Assistant, 0);
    item.source_position = None;
    item.is_provisional = true;
    item.metadata = json!({"nativeStreamId":"stream-one"});
    item.content_blocks = Some(vec![TranscriptContentBlock {
        index: 0,
        content: TranscriptBlockContent::Text {
            text: "中文 streaming".into(),
        },
    }]);
    let mut source = TranscriptSnapshot {
        items: vec![item],
        turns: vec![turn()],
        ..Default::default()
    };
    let mut view = content();
    view.update(&scope(), 1, 1, &source, true).unwrap();
    expand_activity(&mut view, &source, 1, 1, true);
    let original = view
        .message_id(TranscriptMessageKey::Stream("stream-one"))
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
    host.focus(Some(body), true);
    host.get_mut(body)
        .unwrap()
        .component_mut::<TextFieldContent>()
        .unwrap()
        .surface
        .input
        .select_range(0..2);
    source.items[0].id = "history-window-id".into();
    source.items[0].is_provisional = false;
    source.items[0].source_position = Some(TranscriptSourcePosition::Native {
        entry_id: "committed".into(),
        entry_ordinal: 1,
        content_index: None,
    });
    root(&mut host, root_id)
        .update(&scope(), 2, 1, &source, true)
        .unwrap();
    assert_eq!(
        root(&mut host, root_id).message_id(TranscriptMessageKey::NativeEntry("committed")),
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
    assert_eq!(host.focused(), Some(body));
    assert_eq!(host.text_surface(body).unwrap().input.selection(), 0..2);
    // A fresh history read may omit the process-local receipt: native identity wins.
    source.items[0].metadata = json!({});
    root(&mut host, root_id)
        .update(&scope(), 3, 1, &source, true)
        .unwrap();
    assert_eq!(
        root(&mut host, root_id).message_id(TranscriptMessageKey::NativeEntry("committed")),
        Some(original)
    );
}

#[test]
fn equal_payload_without_receipt_does_not_alias_and_ambiguous_receipts_are_atomic() {
    let mut item = message("preview", TranscriptItemKind::Assistant, 0);
    item.is_provisional = true;
    item.source_position = None;
    item.metadata = json!({"nativeStreamId":"stream-one"});
    let mut source = TranscriptSnapshot {
        items: vec![item],
        ..Default::default()
    };
    let mut view = content();
    view.update(&scope(), 1, 1, &source, true).unwrap();
    let old = view
        .message_id(TranscriptMessageKey::Stream("stream-one"))
        .unwrap();
    let mut duplicate = source.items[0].clone();
    duplicate.id = "other-preview".into();
    source.items.push(duplicate);
    assert!(view.update(&scope(), 2, 1, &source, true).is_err());
    assert_eq!(
        view.message_id(TranscriptMessageKey::Stream("stream-one")),
        Some(old)
    );
    source.items.pop();
    source.items[0].is_provisional = false;
    source.items[0].source_position = Some(TranscriptSourcePosition::Native {
        entry_id: "committed".into(),
        entry_ordinal: 0,
        content_index: None,
    });
    source.items[0].metadata = json!({});
    view.update(&scope(), 2, 1, &source, true).unwrap();
    assert_ne!(
        view.message_id(TranscriptMessageKey::NativeEntry("committed")),
        Some(old)
    );
}

#[test]
fn journal_single_body_retains_text_owner_across_aggregate_and_structured_history() {
    let mut item = message("response:1:1", TranscriptItemKind::Assistant, 1);
    item.text = "ENDPOINT_".into();
    item.is_provisional = true;
    let mut source = TranscriptSnapshot {
        items: vec![item],
        ..Default::default()
    };
    let mut view = content();
    view.update(&scope(), 1, 1, &source, true).unwrap();
    let row = view
        .message_id(TranscriptMessageKey::Item("response:1:1"))
        .unwrap();
    let root_id = view.id();
    let mut host = view.into_host().unwrap();
    let body = host
        .get(row)
        .unwrap()
        .component::<OutputMessage>()
        .unwrap()
        .content_ids()[0]
        .body;
    host.focus(Some(body), true);
    host.get_mut(body)
        .unwrap()
        .component_mut::<TextFieldContent>()
        .unwrap()
        .surface
        .input
        .select_range(0..4);
    source.items[0].text = "ENDPOINT_REPLY_OK".into();
    source.items[0].is_provisional = false;
    source.items[0].content_blocks = Some(vec![TranscriptContentBlock {
        index: 7,
        content: TranscriptBlockContent::Text {
            text: "ENDPOINT_REPLY_OK".into(),
        },
    }]);
    root(&mut host, root_id)
        .update(&scope(), 2, 1, &source, true)
        .unwrap();
    assert_eq!(
        host.get(row)
            .unwrap()
            .component::<OutputMessage>()
            .unwrap()
            .content_ids()[0]
            .body,
        body
    );
    assert_eq!(host.focused(), Some(body));
    assert_eq!(host.text_surface(body).unwrap().input.selection(), 0..4);
    // A compatible legacy read keeps the same semantic sole body as well.
    source.items[0].content_blocks = None;
    root(&mut host, root_id)
        .update(&scope(), 3, 1, &source, true)
        .unwrap();
    assert_eq!(
        host.get(row)
            .unwrap()
            .component::<OutputMessage>()
            .unwrap()
            .content_ids()[0]
            .body,
        body
    );
    assert_eq!(host.text_surface(body).unwrap().input.selection(), 0..4);
    // Aggregate text cannot identify either of two structured text blocks.
    source.items[0].content_blocks = Some(vec![
        TranscriptContentBlock {
            index: 0,
            content: TranscriptBlockContent::Text {
                text: "ENDPOINT_".into(),
            },
        },
        TranscriptContentBlock {
            index: 1,
            content: TranscriptBlockContent::Text {
                text: "REPLY_OK".into(),
            },
        },
    ]);
    root(&mut host, root_id)
        .update(&scope(), 4, 1, &source, true)
        .unwrap();
    assert!(
        host.get(row)
            .unwrap()
            .component::<OutputMessage>()
            .unwrap()
            .content_ids()
            .iter()
            .all(|ids| ids.body != body)
    );
}

#[test]
fn sole_aggregate_reasoning_retains_its_disclosure_and_text_owner_when_order_arrives() {
    let mut item = message("response", TranscriptItemKind::Assistant, 1);
    item.text.clear();
    item.reasoning = "thinking".into();
    let mut source = TranscriptSnapshot {
        items: vec![item],
        ..Default::default()
    };
    let mut view = content();
    view.update(&scope(), 1, 1, &source, true).unwrap();
    let row = view
        .row_id(&TranscriptContentRowKey::MessageFragment(
            TranscriptMessageIdentity::Item("response".into()),
            OutputMessageFragmentKind::Reasoning,
        ))
        .unwrap();
    let root_id = view.id();
    let mut host = view.into_host().unwrap();
    let ids = host
        .get(row)
        .unwrap()
        .component::<OutputMessageFragment>()
        .unwrap()
        .content_ids()
        .into_iter()
        .find(|ids| ids.kind == OutputContentKind::Reasoning)
        .unwrap();
    host.accessibility_action(ids.disclosure.unwrap(), None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    root(&mut host, root_id).handle_actions(&actions);
    assert!(host.get(ids.body).is_some());
    source.items[0].reasoning = "completed thought".into();
    source.items[0].content_blocks = Some(vec![TranscriptContentBlock {
        index: 4,
        content: TranscriptBlockContent::Reasoning {
            text: "completed thought".into(),
        },
    }]);
    root(&mut host, root_id)
        .update(&scope(), 2, 1, &source, true)
        .unwrap();
    let retained = host
        .get(row)
        .unwrap()
        .component::<OutputMessageFragment>()
        .unwrap()
        .content_ids()[0];
    assert_eq!(retained.root, ids.root);
    assert_eq!(retained.body, ids.body);
    assert_eq!(retained.disclosure, ids.disclosure);
    assert_eq!(
        host.text_surface(ids.body).unwrap().input.text,
        "completed thought"
    );
}

#[test]
fn prepared_update_predicts_retained_and_new_rows_without_allocating_or_mutating() {
    let source = snapshot();
    let mut view = content();
    let plan = view.prepare_update(&scope(), 1, 1, &source, true).unwrap();
    let predicted = plan.rows().to_vec();
    assert!(view.row_keys().is_empty());
    assert_eq!(
        view.prepare_update(&scope(), 1, 1, &source, true)
            .unwrap()
            .rows(),
        predicted
    );
    view.apply_prepared_update(&plan).unwrap();
    assert!(
        predicted
            .iter()
            .all(|(key, id)| view.row_id(key) == Some(*id))
    );
    let mut prefixed = source.clone();
    let mut older = message("older", TranscriptItemKind::Assistant, 0);
    older.turn_id = "older-turn".into();
    prefixed.items.insert(0, older);
    let next = view
        .prepare_update(&scope(), 2, 1, &prefixed, true)
        .unwrap();
    assert!(
        predicted
            .iter()
            .all(|(key, id)| next.rows().contains(&(key.clone(), *id)))
    );
    let expected = next.rows().to_vec();
    view.apply_prepared_update(&next).unwrap();
    assert!(
        expected
            .iter()
            .all(|(key, id)| view.row_id(key) == Some(*id))
    );
    let retained = view.row_keys().to_vec();
    assert!(view.apply_prepared_update(&plan).is_err());
    assert_eq!(view.row_keys(), retained);
}

#[test]
fn prepared_update_validates_before_publication_and_respects_explicit_aliases() {
    let mut source = snapshot();
    source.items.remove(0);
    source.items[0].source_position = None;
    source.items[0].is_provisional = true;
    source.items[0].metadata = json!({"nativeStreamId":"live"});
    let mut view = content();
    view.update(&scope(), 1, 1, &source, true).unwrap();
    expand_activity(&mut view, &source, 1, 1, true);
    let row = view
        .message_id(TranscriptMessageKey::Stream("live"))
        .unwrap();
    source.items[0].is_provisional = false;
    source.items[0].source_position = Some(TranscriptSourcePosition::Native {
        entry_id: "committed".into(),
        entry_ordinal: 0,
        content_index: None,
    });
    let plan = view.prepare_update(&scope(), 2, 1, &source, true).unwrap();
    assert!(plan.rows().contains(&(
        TranscriptContentRowKey::Message(TranscriptMessageIdentity::NativeEntry(
            "committed".into()
        )),
        row
    )));
    view.apply_prepared_update(&plan).unwrap();
    source.items[0].content_blocks = Some(vec![
        TranscriptContentBlock {
            index: 0,
            content: TranscriptBlockContent::Text { text: "a".into() },
        },
        TranscriptContentBlock {
            index: 0,
            content: TranscriptBlockContent::Text { text: "b".into() },
        },
    ]);
    assert!(view.prepare_update(&scope(), 3, 1, &source, true).is_err());
    assert_eq!(
        view.message_id(TranscriptMessageKey::NativeEntry("committed")),
        Some(row)
    );
    assert!(
        view.prepare_update(&RunScope::main("fixture", "other"), 3, 1, &source, true)
            .is_err()
    );
}

#[test]
fn preview_follows_clientkit_agentkit_bridge_join_and_settlement_rules() {
    let mut source = snapshot();
    source.turns[0].input = "原输入\n完整保留".into();
    source.turns[0].status = "running".into();
    source.items[1].text = "".into();
    source.items[1].reasoning = "不能冒充输出的推理".into();
    source.items[1].is_final = false;
    let mut view = content();
    view.update(&scope(), 1, 1, &source, false).unwrap();
    let target = view.turn_targets().unwrap().remove(0);
    assert_eq!(target.preview().input, source.turns[0].input);
    assert_eq!(target.preview().output_text("运行中"), Some("运行中"));
    assert!(!target.preview().has_final_answer);
    assert!(!target.preview().is_settled);
    source.items[1].text = "第一段".into();
    let mut output = message("second", TranscriptItemKind::Assistant, 4);
    output.text = "第二段".into();
    source.items.push(output);
    source
        .items
        .push(message("错误不混入预览", TranscriptItemKind::Error, 5));
    source.tools.push(tool());
    source.turns[0].status = "completed".into();
    view.update(&scope(), 2, 1, &source, false).unwrap();
    assert!(view.validate_turn_target(&target).is_err());
    let next = view.turn_targets().unwrap().remove(0);
    assert_eq!(next.preview().output, "第一段\n第二段");
    assert!(next.preview().has_final_answer && next.preview().is_settled);
    source
        .items
        .retain(|item| item.kind != TranscriptItemKind::Assistant);
    view.update(&scope(), 3, 1, &source, false).unwrap();
    let empty = view.turn_targets().unwrap().remove(0);
    assert_eq!(empty.preview().output_text("运行中"), None);
    assert!(!empty.preview().has_final_answer);
}

#[test]
fn preview_prefix_keeps_240_extended_graphemes_and_cross_message_crlf() {
    for (texts, expected) in [
        (
            vec![format!("{}e\u{301}截断", "👩🏽‍💻".repeat(239))],
            format!("{}e\u{301}", "👩🏽‍💻".repeat(239)),
        ),
        (
            vec![format!("{}\r", "中".repeat(239)), "X".into()],
            format!("{}\r\n", "中".repeat(239)),
        ),
        (vec!["".into(), "最终回复".into()], "\n最终回复".into()),
    ] {
        let mut source = snapshot();
        source.turns[0].input = "原回合输入".into();
        source
            .items
            .retain(|item| item.kind != TranscriptItemKind::Assistant);
        for (i, text) in texts.into_iter().enumerate() {
            let mut item = message(
                &format!("out-{i}"),
                TranscriptItemKind::Assistant,
                10 + i as i64,
            );
            item.text = text;
            source.items.push(item);
        }
        let mut view = content();
        view.update(&scope(), 1, 1, &source, false).unwrap();
        assert_eq!(view.turn_targets().unwrap()[0].preview().output, expected);
    }
}

#[test]
fn output_aside_presence_uses_formal_visible_content_not_reasoning_tools_or_a_timestamp() {
    for (block, expected) in [
        (
            TranscriptBlockContent::Text {
                text: String::new(),
            },
            false,
        ),
        (
            TranscriptBlockContent::Text {
                text: "正文".into(),
            },
            true,
        ),
        (
            TranscriptBlockContent::Reasoning {
                text: "推理".into(),
            },
            false,
        ),
        (
            TranscriptBlockContent::ToolCall {
                tool_id: "tool".into(),
            },
            false,
        ),
        (
            TranscriptBlockContent::Attachment {
                content: json!({"type":"image","mimeType":"image/png"}),
            },
            true,
        ),
    ] {
        for (visible, provisional, final_answer) in [
            (true, false, true),
            (false, false, true),
            (true, true, true),
            (true, false, false),
        ] {
            let mut source = snapshot();
            source.turns[0].ended_at = Some(13000.);
            source.tools.push(tool());
            let output = &mut source.items[1];
            output.text.clear();
            output.reasoning = "reasoning alone is not the output lane".into();
            output.content_blocks = Some(vec![TranscriptContentBlock {
                index: 0,
                content: block.clone(),
            }]);
            output.is_visible = visible;
            output.is_provisional = provisional;
            output.is_final = final_answer;
            let mut view = TranscriptContent::new(
                49930,
                49931..49960,
                scope(),
                TranscriptContentLabels {
                    footnote: TurnFootnoteLabels {
                        finished_timestamp: |_, _| Some("源结束时间".into()),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .unwrap();
            view.update(&scope(), 1, 1, &source, false).unwrap();
            let id = view
                .row_id(&TranscriptContentRowKey::TurnFootnote("turn".into()))
                .unwrap();
            let footer = view
                .children()
                .iter()
                .find(|node| node.id() == id)
                .unwrap()
                .component::<TurnFootnote>()
                .unwrap();
            let shown = expected && visible && !provisional && final_answer;
            assert_eq!(footer.facts().unwrap().display.has_output, shown);
            assert_eq!(
                footer.active(),
                shown,
                "{block:?}, visible={visible}, provisional={provisional}, final={final_answer}"
            );
            assert!(
                footer.copy_button_id().is_none(),
                "presence alone must not fabricate prose copy"
            );
        }
    }
}

#[test]
fn hiding_the_aside_without_changing_the_response_keeps_feedback_ime_until_commit() {
    let mut source = feedback_snapshot();
    source.items[1].text.clear();
    source.turns[0]
        .footnote_actions
        .push(tekes_agent_contract::ThreadActionDefinition {
            id: "forkTurn:turn".into(),
            label: "分叉".into(),
            icon: "fork".into(),
            interaction: "execute".into(),
            enabled: true,
            disabled_reason: None,
            copy_text: None,
        });
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
    let unchanged_response = source.items[1].clone();
    source.turns[0].footnote_actions.clear();
    assert_eq!(source.items[1], unchanged_response);
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
    assert_eq!(root(&mut host, id).feedback_controls().count(), 0);
    assert!(host.get(editor).is_none());
    assert!(host.get(feedback).is_none());
}

#[test]
fn feedback_uses_the_same_aside_gate_while_final_response_authority_stays_independent() {
    for has_image in [false, true] {
        for has_fork in [false, true] {
            let mut source = feedback_snapshot();
            source.items[1].text.clear();
            if has_image {
                source.items[1].content_blocks = Some(vec![TranscriptContentBlock {
                    index: 0,
                    content: TranscriptBlockContent::Attachment {
                        content: json!({"type":"image","mimeType":"image/png"}),
                    },
                }]);
            }
            if has_fork {
                source.turns[0].footnote_actions.push(
                    tekes_agent_contract::ThreadActionDefinition {
                        id: "forkTurn:turn".into(),
                        label: "分叉".into(),
                        icon: "fork".into(),
                        interaction: "execute".into(),
                        enabled: false,
                        disabled_reason: Some("当前只读".into()),
                        copy_text: None,
                    },
                );
            }
            let document = TranscriptDocument::read(&source).unwrap();
            assert!(
                tekes_agent_presentation::TurnFeedbackTarget::read(&document.sections[0]).is_some(),
                "display never changes canonical response proof"
            );
            let mut view = content();
            view.configure_feedback(Some(Default::default())).unwrap();
            view.update(&scope(), 1, 1, &source, false).unwrap();
            assert_eq!(
                view.feedback_controls().count(),
                usize::from(has_image || has_fork)
            );
            assert!(view.take_copy_intents(&[]).is_empty());
        }
    }
}

#[test]
fn grouped_aside_keeps_one_reading_item_and_rejects_old_feedback_completion_after_retarget() {
    let mut source = feedback_snapshot();
    let mut view = content();
    view.configure_feedback(Some(Default::default())).unwrap();
    let prepared = view.prepare_update(&scope(), 1, 1, &source, true).unwrap();
    assert!(
        !prepared
            .rows()
            .iter()
            .any(|(key, _)| matches!(key, TranscriptContentRowKey::TurnFeedback(_)))
    );
    view.update(&scope(), 1, 1, &source, true).unwrap();
    let footer = view
        .row_id(&TranscriptContentRowKey::TurnFootnote("turn".into()))
        .unwrap();
    let (feedback, control) = view.feedback_controls().next().unwrap();
    let target = control.target().unwrap().clone();
    let old_refresh = control.ids().refresh;
    let old_positive = control.ids().positive;
    assert!(
        view.take_feedback_intents(&[ContentAction {
            target: old_refresh,
            kind: ContentActionKind::Activate,
            source: ContentActionSource::Programmatic,
        }])
        .is_empty(),
        "cold hidden refresh cannot dispatch"
    );
    view.feedback_mut(feedback)
        .unwrap()
        .initial_read_failed("owned initial read failure".into());
    let gesture = view
        .take_feedback_intents(&[ContentAction {
            target: old_refresh,
            kind: ContentActionKind::Activate,
            source: ContentActionSource::Programmatic,
        }])
        .pop()
        .unwrap()
        .1;
    source.items[1].text = "new canonical response".into();
    view.update(&scope(), 2, 1, &source, true).unwrap();
    assert_eq!(
        view.row_id(&TranscriptContentRowKey::TurnFootnote("turn".into())),
        Some(footer)
    );
    assert_eq!(view.feedback_controls().next().unwrap().0, feedback);
    let control = view.feedback_mut(feedback).unwrap();
    assert_ne!(control.ids().positive, old_positive);
    assert!(!control.complete(
        &gesture,
        FeedbackControlResult::Applied(FeedbackControlState {
            target,
            publication: 1,
            connection_epoch: 1,
            current: None,
            put_available: true,
            delete_available: true,
        })
    ));
    assert!(
        view.take_feedback_intents(&[ContentAction {
            target: old_positive,
            kind: ContentActionKind::Activate,
            source: ContentActionSource::Programmatic,
        }])
        .is_empty()
    );
    source.items[1].metadata = json!({});
    view.update(&scope(), 3, 1, &source, true).unwrap();
    assert!(
        view.row_id(&TranscriptContentRowKey::TurnFeedback("turn".into()))
            .is_none()
    );
    source.items[1].metadata = json!({"feedbackMessageId":"public-output"});
    view.update(&scope(), 4, 1, &source, true).unwrap();
    assert_ne!(
        view.feedback_controls().next().unwrap().1.ids().positive,
        old_positive
    );
}

#[test]
fn adjacent_tools_keep_original_members_when_prepend_split_and_merge_change_the_group() {
    let mut s = TranscriptSnapshot {
        turns: vec![turn()],
        ..Default::default()
    };
    let mut first = tool();
    first.id = "first".into();
    first.result_position = None;
    let mut second = first.clone();
    second.id = "second".into();
    second.source_position = Some(TranscriptSourcePosition::Journal {
        sequence: 3,
        content_index: None,
    });
    s.tools = vec![first.clone(), second.clone()];
    let mut view = content();
    view.update(&scope(), 1, 1, &s, true).unwrap();
    expand_activity(&mut view, &s, 1, 1, true);
    let owner = view.tool_run("first").unwrap().id();
    assert_eq!(view.tool_run("second").unwrap().id(), owner);
    assert!(!view.tool_run("first").unwrap().is_expanded());
    let first_id = view.tool_activity_id("first").unwrap();
    let second_id = view.tool_activity_id("second").unwrap();
    let toggle = view.tool_run("first").unwrap().disclosure_id().unwrap();
    let root_id = view.id();
    let mut host = view.into_host().unwrap();
    host.accessibility_action(toggle, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    root(&mut host, root_id).handle_actions(&actions);
    assert!(
        root(&mut host, root_id)
            .tool_run("first")
            .unwrap()
            .is_expanded()
    );
    assert!(host.get(first_id).is_some());
    assert!(host.get(second_id).is_some());
    let mut prepended = first.clone();
    prepended.id = "prepended".into();
    prepended.source_position = Some(TranscriptSourcePosition::Journal {
        sequence: 0,
        content_index: None,
    });
    s.tools.insert(0, prepended);
    root(&mut host, root_id)
        .update(&scope(), 2, 1, &s, true)
        .unwrap();
    assert_eq!(
        root(&mut host, root_id).tool_run("prepended").unwrap().id(),
        owner
    );
    assert_eq!(
        root(&mut host, root_id).tool_activity_id("second"),
        Some(second_id)
    );
    assert_ne!(
        root(&mut host, root_id)
            .tool_run("first")
            .unwrap()
            .disclosure_id(),
        Some(toggle)
    );
    s.items = vec![message("prose", TranscriptItemKind::Assistant, 2)];
    s.items[0].is_final = false;
    root(&mut host, root_id)
        .update(&scope(), 3, 1, &s, true)
        .unwrap();
    assert_ne!(
        root(&mut host, root_id).tool_run("first").unwrap().id(),
        root(&mut host, root_id).tool_run("second").unwrap().id()
    );
    assert_eq!(
        root(&mut host, root_id).tool_activity_id("first"),
        Some(first_id)
    );
    assert_eq!(
        root(&mut host, root_id).tool_activity_id("second"),
        Some(second_id)
    );
    s.items.clear();
    root(&mut host, root_id)
        .update(&scope(), 4, 1, &s, true)
        .unwrap();
    assert_eq!(
        root(&mut host, root_id).tool_run("second").unwrap().id(),
        owner
    );
    assert_eq!(
        root(&mut host, root_id).tool_activity_id("second"),
        Some(second_id)
    );
}
#[test]
fn zero_node_assistant_is_transparent_but_unplaced_tools_never_imply_adjacency() {
    let mut a = tool();
    a.id = "a".into();
    a.result_position = None;
    let mut b = a.clone();
    b.id = "b".into();
    b.source_position = Some(TranscriptSourcePosition::Journal {
        sequence: 3,
        content_index: None,
    });
    let mut empty = message("empty", TranscriptItemKind::Assistant, 2);
    empty.text.clear();
    empty.content_blocks = Some(vec![]);
    let mut s = TranscriptSnapshot {
        tools: vec![a, b],
        items: vec![empty],
        turns: vec![turn()],
        ..Default::default()
    };
    let mut view = content();
    view.update(&scope(), 1, 1, &s, true).unwrap();
    expand_activity(&mut view, &s, 1, 1, true);
    assert_eq!(
        view.tool_run("a").unwrap().summary().unwrap().tool_ids,
        ["a", "b"]
    );
    s.tools[1].source_position = None;
    view.update(&scope(), 2, 1, &s, true).unwrap();
    assert_ne!(
        view.tool_run("a").unwrap().id(),
        view.tool_run("b").unwrap().id()
    );
}

#[test]
fn native_host_transcript_uses_shared_url_projection_in_group_previews() {
    let mut first = tool();
    first.id = "fetch-a".into();
    first.name = "fetch".into();
    first.phase = "doing".into();
    first.arguments = json!({"url":"https://user:secret@EXAMPLE.org/a/../b/?query=secret#secret"});
    first.result_position = None;
    let mut second = first.clone();
    second.id = "fetch-b".into();
    second.source_position = Some(TranscriptSourcePosition::Journal {
        sequence: 2,
        content_index: None,
    });
    second.arguments = json!({"url":"relative/a/./b/"});
    let snapshot = TranscriptSnapshot {
        tools: vec![first, second],
        turns: vec![turn()],
        ..Default::default()
    };
    let mut view = content();
    view.update(&scope(), 1, 1, &snapshot, true).unwrap();
    expand_activity(&mut view, &snapshot, 1, 1, true);
    let group = view.tool_run("fetch-a").unwrap();
    assert_eq!(
        group.summary().unwrap().active_summaries,
        ["Fetch · EXAMPLE.org/a/../b", "Fetch · relative/a/./b"]
    );
    let header = group.disclosure_id().unwrap();
    let host = view.into_host().unwrap();
    let label = &host
        .get(header)
        .unwrap()
        .component::<ButtonContent>()
        .unwrap()
        .label;
    assert!(label.contains("EXAMPLE.org/a/../b"));
    assert!(!label.contains("secret"));
}

#[test]
fn transcript_routes_one_explicit_header_adapter_to_independent_and_embedded_tools() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static REV: AtomicU64 = AtomicU64::new(1);
    let adapter = ToolHeaderAdapter {
        revision: |_| REV.load(Ordering::SeqCst),
        inputs: |scope, tool| TranscriptToolRowInputs {
            report: Some(TranscriptToolReport {
                verb: "read".into(),
                subject: Some(format!("{}:{}", scope.session_id, tool.id)),
                result_count: None,
                result_object: None,
                note: None,
                added_lines: Some(if REV.load(Ordering::SeqCst) == 1 {
                    0
                } else {
                    2
                }),
                removed_lines: Some(0),
            }),
            workspace_change_count: Some(TranscriptToolChangeCount {
                additions: 99,
                deletions: 99,
            }),
        },
        name: None,
        stopped: || "Stopped".into(),
        report_wording: TranscriptToolReportWording {
            subject_phrase: |v, s| format!("{s}: {v}"),
            verb: str::to_owned,
            magnitude: |n, o| format!("{n} {o}"),
            join: |a, b| format!("{a} · {b}"),
        },
    };
    let mut s = snapshot();
    let independent = tool();
    let mut embedded = tool();
    embedded.id = "embedded".into();
    embedded.result_position = Some(TranscriptSourcePosition::Journal {
        sequence: 4,
        content_index: None,
    });
    embedded.source_position = Some(TranscriptSourcePosition::Journal {
        sequence: 2,
        content_index: Some(1),
    });
    s.items[1].content_blocks = Some(vec![
        TranscriptContentBlock {
            index: 0,
            content: TranscriptBlockContent::Text {
                text: "output".into(),
            },
        },
        TranscriptContentBlock {
            index: 1,
            content: TranscriptBlockContent::ToolCall {
                tool_id: "embedded".into(),
            },
        },
    ]);
    s.tools = vec![independent, embedded];
    let mut view = TranscriptContent::new(
        65600,
        65601..65700,
        scope(),
        TranscriptContentLabels::default().with_tool_header(adapter),
    )
    .unwrap();
    view.update(&scope(), 1, 1, &s, true).unwrap();
    expand_activity(&mut view, &s, 1, 1, true);
    assert_eq!(
        view.tool_run("tool")
            .unwrap()
            .tool_activity("tool")
            .unwrap()
            .tool_header()
            .unwrap()
            .text,
        "a:tool: read    +0 −0"
    );
    assert!(view.tool_run("embedded").is_none());
    let old = view.prepare_update(&scope(), 1, 1, &s, true).unwrap();
    REV.store(2, Ordering::SeqCst);
    assert!(view.apply_prepared_update(&old).is_err());
    assert!(
        view.tool_run("tool")
            .unwrap()
            .tool_activity("tool")
            .unwrap()
            .tool_header()
            .unwrap()
            .text
            .ends_with("+0 −0")
    );
    let current = view.prepare_update(&scope(), 1, 1, &s, true).unwrap();
    view.apply_prepared_update(&current).unwrap();
    assert!(
        view.tool_run("tool")
            .unwrap()
            .tool_activity("tool")
            .unwrap()
            .tool_header()
            .unwrap()
            .text
            .ends_with("+2 −0")
    );
    REV.store(1, Ordering::SeqCst);
    assert!(view.update(&scope(), 1, 1, &s, true).is_err());
    REV.store(2, Ordering::SeqCst);
    let output = view
        .row_id(&TranscriptContentRowKey::MessageFragment(
            TranscriptMessageIdentity::Item("output".into()),
            OutputMessageFragmentKind::Tools,
        ))
        .unwrap();
    let host = view.into_host().unwrap();
    assert_eq!(
        host.get(output)
            .unwrap()
            .component::<OutputMessageFragment>()
            .unwrap()
            .tool_headers()[0]
            .text,
        "a:embedded: read    +2 −0"
    );
}

#[test]
fn foreign_message_controller_cannot_borrow_transcript_scope_or_partially_advance_a_plan() {
    let source = snapshot();
    let mut view = content();
    view.update(&scope(), 1, 1, &source, true).unwrap();
    let plan = view.prepare_update(&scope(), 2, 1, &source, true).unwrap();
    let keys = view.row_keys().to_vec();
    let index = keys
        .iter()
        .position(|value| value == &key("output"))
        .unwrap();
    let id = view.row_id(&key("output")).unwrap();
    let foreign_scope = RunScope::main("foreign", "other");
    let mut foreign = OutputMessage::new((id.0 >> 32) as u32, foreign_scope.clone());
    let doc = TranscriptDocument::read(&source).unwrap();
    foreign
        .update(
            &foreign_scope,
            1,
            1,
            &doc.sections[0].outputs[0],
            Some(&doc.sections[0]),
        )
        .unwrap();
    let original = std::mem::replace(
        &mut view.children_mut().unwrap()[index],
        Content::new(id, foreign),
    );
    assert!(view.validate_prepared_update(&plan).is_err());
    assert!(view.update(&scope(), 2, 1, &source, true).is_err());
    assert_eq!(view.row_keys(), keys);
    view.children_mut().unwrap()[index] = original;
    view.validate_prepared_update(&plan).unwrap();
    view.apply_prepared_update(&plan).unwrap();
    assert_eq!(view.row_keys(), keys);
}
