//! A second host consumes captured turn copy through the public Kit only.
use tekes_agent_components_ui::*;
use tekes_ui_controls::composition::*;

fn source() -> TranscriptSnapshot {
    let item = TranscriptItem {
        id: "a".into(),
        record_id: None,
        reconciliation_id: None,
        turn_id: "turn".into(),
        kind: TranscriptItemKind::Assistant,
        text: "中文第一段".into(),
        reasoning: "不复制推理".into(),
        attachments: vec![],
        content_blocks: None,
        is_visible: true,
        is_final: false,
        is_provisional: false,
        reasoning_ended: true,
        source_sequence: None,
        source_position: None,
        time: None,
        metadata: Default::default(),
    };
    let mut empty = item.clone();
    empty.id = "empty".into();
    empty.text.clear();
    let mut hidden = item.clone();
    hidden.id = "hidden".into();
    hidden.text = "源投影文字".into();
    hidden.is_visible = false;
    let mut final_item = item.clone();
    final_item.id = "final".into();
    final_item.text = "最后一段".into();
    final_item.is_final = true;
    let mut notice = item.clone();
    notice.id = "notice".into();
    notice.kind = TranscriptItemKind::Notice;
    notice.text = "不复制通知".into();
    TranscriptSnapshot {
        turns: vec![TranscriptTurn {
            id: "turn".into(),
            status: "failed".into(),
            input: String::new(),
            started_at: None,
            ended_at: None,
            usage: Default::default(),
            fork_boundary: None,
            footnote_actions: vec![],
            live_usage: None,
            reported_usage: None,
        }],
        items: vec![item, empty, hidden, final_item, notice],
        ..Default::default()
    }
}
fn footer(host: &ContentHost, id: ComponentId) -> &TurnFootnote {
    host.get(id).unwrap().component::<TurnFootnote>().unwrap()
}

#[test]
fn second_host_copies_source_turn_order_without_granting_response_authority() {
    let mut source = source();
    let scope = RunScope::main("consumer", "original");
    let copy = TranscriptTurnCopy::read(&source, "turn").unwrap().unwrap();
    assert_eq!(copy.text(), "中文第一段\n\n源投影文字\n最后一段");
    assert_eq!(copy.item_ids(), &["a", "empty", "hidden", "final"]);
    assert!(
        TranscriptDocument::read(&source).unwrap().sections[0]
            .final_response()
            .is_none()
    );
    let mut view =
        TranscriptContent::new(49020, 49021..49100, scope.clone(), Default::default()).unwrap();
    view.update(&scope, 1, 1, &source, false).unwrap();
    let root = view.id();
    let row = view
        .row_id(&TranscriptContentRowKey::TurnFootnote("turn".into()))
        .unwrap();
    let mut host = view.into_host().unwrap();
    let original = host.get(row).unwrap().component::<TurnFootnote>().unwrap() as *const _ as usize;
    let old = footer(&host, row).copy_button_id().unwrap();
    assert!(
        host.accessibility_action(old, None, AccessibilityAction::Press)
            .activated
    );
    let actions = host.drain_actions();
    let intents = host
        .get(root)
        .unwrap()
        .component::<TranscriptContent>()
        .unwrap()
        .take_copy_intents(&actions);
    assert_eq!(intents.len(), 1);
    let captured = &intents[0];
    assert_eq!(captured.scope(), &scope);
    assert_eq!(captured.message(), None);
    assert_eq!(captured.text(), copy.text());
    assert_eq!(
        captured.source(),
        &TranscriptCopySource::TurnOutput {
            turn_id: "turn".into(),
            item_ids: vec!["a".into(), "empty".into(), "hidden".into(), "final".into()]
        }
    );
    // Same text with a different original source is a different captured action.
    source.items[2].id = "replacement-hidden".into();
    let view = host
        .get_mut(root)
        .unwrap()
        .component_mut::<TranscriptContent>()
        .unwrap();
    assert!(view.update(&scope, 1, 1, &source, false).is_err());
    assert_eq!(footer(&host, row).copy_button_id(), Some(old));
    host.get_mut(root)
        .unwrap()
        .component_mut::<TranscriptContent>()
        .unwrap()
        .update(&scope, 2, 1, &source, false)
        .unwrap();
    assert_ne!(footer(&host, row).copy_button_id(), Some(old));
    assert_eq!(footer(&host, row) as *const _ as usize, original);
    assert!(
        host.get(root)
            .unwrap()
            .component::<TranscriptContent>()
            .unwrap()
            .take_copy_intents(&actions)
            .is_empty()
    );
    assert_eq!(captured.text(), "中文第一段\n\n源投影文字\n最后一段");
    assert!(captured.claim());
    assert!(!captured.clone().claim());
    source.items[2].source_position = Some(TranscriptSourcePosition::Native {
        entry_id: "old-durable".into(),
        entry_ordinal: 0,
        content_index: None,
    });
    host.get_mut(root)
        .unwrap()
        .component_mut::<TranscriptContent>()
        .unwrap()
        .update(&scope, 3, 1, &source, false)
        .unwrap();
    let before_provenance = footer(&host, row).copy_button_id();
    source.items[2].source_position = Some(TranscriptSourcePosition::Native {
        entry_id: "new-durable".into(),
        entry_ordinal: 0,
        content_index: None,
    });
    host.get_mut(root)
        .unwrap()
        .component_mut::<TranscriptContent>()
        .unwrap()
        .update(&scope, 4, 1, &source, false)
        .unwrap();
    assert_ne!(
        footer(&host, row).copy_button_id(),
        before_provenance,
        "identical temporary item ID/text must not alias a different Native entry"
    );
    source.turns[0].status = "completed".into();
    source.items[0].is_final = true;
    assert!(TranscriptTurnCopy::read(&source, "turn").unwrap().is_some());
    assert!(
        TranscriptDocument::read(&source).unwrap().sections[0]
            .final_response()
            .is_none()
    );
    assert!(TranscriptTurnCopy::read(&source, "other").is_err());
    source.items[0].is_final = false;
    source.items[3].is_final = false;
    assert!(TranscriptTurnCopy::read(&source, "turn").unwrap().is_none());
    source.items[1].id = source.items[0].id.clone();
    assert!(TranscriptTurnCopy::read(&source, "turn").is_err());
}

#[test]
fn standalone_turn_copy_rejects_foreign_turn_and_never_exposes_controls_for_live_owner() {
    let mut source = source();
    let scope = RunScope::main("consumer", "original");
    let copy = TranscriptTurnCopy::read(&source, "turn").unwrap().unwrap();
    source.turns[0].status = "running".into();
    let mut footer = TurnFootnote::new(49110, scope.clone(), Default::default());
    let live = TurnFootnoteDisplay {
        revision: 1,
        is_live_owner: true,
        has_output: true,
        ..Default::default()
    };
    footer
        .update_with_turn_copy(&scope, 1, 1, &source.turns[0], false, Some(&copy), &live)
        .unwrap();
    assert!(footer.copy_button_id().is_none());
    let mut other = source.turns[0].clone();
    other.id = "other".into();
    assert!(
        footer
            .update_with_turn_copy(
                &scope,
                2,
                1,
                &other,
                false,
                Some(&copy),
                &Default::default()
            )
            .is_err()
    );
    assert!(footer.copy_button_id().is_none());
    source.turns[0].status = "aborted".into();
    footer
        .update_with_turn_copy(
            &scope,
            2,
            1,
            &source.turns[0],
            false,
            Some(&copy),
            &TurnFootnoteDisplay {
                revision: 2,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(footer.copy_button_id().is_some());
}
