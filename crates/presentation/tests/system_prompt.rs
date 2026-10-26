use serde_json::{Value, json};
use tekes_agent_contract::*;
use tekes_agent_presentation::*;
use tekes_agent_session::SessionProjection;
fn scope() -> RunScope {
    RunScope::main("sep", "s")
}
fn read(rows: Vec<(&str, Value, Value, bool)>) -> TranscriptSnapshot {
    let entries: Vec<HistoryEntry> = rows
        .into_iter()
        .enumerate()
        .map(|(seq, (kind, data, metadata, model))| {
            let mut event = json!({"type":kind,"seq":seq,"time":seq+1,"data":data});
            event
                .as_object_mut()
                .unwrap()
                .extend(metadata.as_object().unwrap().clone());
            serde_json::from_value(json!({"event":event,"isModelOnly":model})).unwrap()
        })
        .collect();
    let mut session = SessionProjection::default();
    session
        .apply_snapshot(JournalSnapshot {
            session_id: "s".into(),
            generation: "g".into(),
            through_sequence: entries.last().unwrap().event.seq,
            window_limit: 50,
            entries,
            has_more_before: false,
            projections: None,
            history_revision: None,
        })
        .unwrap();
    TranscriptSnapshot::read(&scope(), &session).unwrap()
}
fn event(kind: &str, data: Value) -> (&str, Value, Value, bool) {
    (kind, data, json!({}), false)
}
fn prompt(turn: i64, step: i64, text: &str) -> Value {
    json!({"turn":turn,"step":step,"message":{"content":[{"type":"text","text":text},{"type":"metadata","text":"not prompt"}]}})
}
fn base() -> Vec<(&'static str, Value, Value, bool)> {
    vec![
        event("turn/start", json!({"turn":1})),
        event(
            "user/message",
            json!({"turn":1,"id":"u","content":[{"type":"text","text":"question"}]}),
        ),
        event("step/start", json!({"turn":1,"step":1})),
    ]
}
fn cards(snapshot: &TranscriptSnapshot) -> Vec<&TranscriptItem> {
    snapshot
        .items
        .iter()
        .filter(|item| item.is_visible && item.system_prompt().is_some())
        .collect()
}

#[test]
fn initial_prompt_uses_source_role_and_anchor_before_input_without_rewriting_record_position() {
    let mut rows = base();
    rows.push((
        "system/message",
        prompt(1, 1, "initial"),
        json!({"surfaceOp":"append"}),
        false,
    ));
    rows.push(event(
        "request/header",
        json!({"turn":1,"step":1,"reason":"initial"}),
    ));
    rows.push(event("turn/end", json!({"turn":1,"reason":"completed"})));
    let snapshot = read(rows);
    let card = cards(&snapshot)[0];
    assert_eq!(cards(&snapshot).len(), 1);
    assert_eq!(card.text, "initial");
    assert_eq!(
        card.system_prompt(),
        Some(TranscriptSystemPrompt {
            placement: TranscriptSystemPromptPlacement::Initial,
            anchor_sequence: 0
        })
    );
    assert_eq!(card.source_sequence, Some(3));
    assert_eq!(
        card.source_position,
        Some(TranscriptSourcePosition::Journal {
            sequence: 3,
            content_index: None
        })
    );
    let doc = TranscriptDocument::read(&snapshot).unwrap();
    let order = doc.sections[0]
        .display_order(&scope(), &snapshot.thinking_sources)
        .unwrap();
    assert!(matches!(order.ordered[0],TranscriptOrderedEvent::Message(item) if item.id==card.id));
    let planes = doc.sections[0]
        .planes(&scope(), &snapshot.thinking_sources, false)
        .unwrap();
    assert_eq!(
        planes.items[&card.id].body,
        Some(TranscriptPlane::InitialPrompt)
    );
    assert!(!planes.has_activity);
    assert_eq!(
        planes.activity,
        TranscriptActivityPresentation::Settled {
            default_expanded: false
        }
    );
}
#[test]
fn append_update_owns_same_step_header_but_later_step_gets_its_own_prompt_card() {
    let mut rows = base();
    rows.push((
        "system/message",
        prompt(1, 1, "first"),
        json!({"surfaceOp":"append"}),
        false,
    ));
    rows.push(event("request/header", json!({"reason":"initial"})));
    rows.push((
        "system/message",
        prompt(1, 1, "updated"),
        json!({"surfaceOp":"append"}),
        false,
    ));
    rows.push(event("request/header", json!({"reason":"change"})));
    rows.push(event("step/start", json!({"turn":1,"step":2})));
    rows.push(event("request/header", json!({"reason":"next"})));
    let snapshot = read(rows);
    let cards = cards(&snapshot);
    assert_eq!(
        cards
            .iter()
            .map(|row| row.text.as_str())
            .collect::<Vec<_>>(),
        ["first", "updated", "updated"]
    );
    assert_eq!(
        cards[1].system_prompt().unwrap().placement,
        TranscriptSystemPromptPlacement::Update
    );
    assert_eq!(
        cards[2].system_prompt().unwrap(),
        TranscriptSystemPrompt {
            placement: TranscriptSystemPromptPlacement::Inline,
            anchor_sequence: 7
        }
    );
}
#[test]
fn model_rewrite_changes_effective_prompt_without_erasing_human_append_history() {
    let mut rows = base();
    rows.push((
        "system/message",
        prompt(1, 1, "original"),
        json!({"surfaceOp":"append"}),
        false,
    ));
    rows.push(event("request/header", json!({"reason":"initial"})));
    rows.push((
        "system/message",
        prompt(1, 1, "model replacement"),
        json!({"surfaceOp":{"op":"replace","startSeq":3,"endSeq":3}}),
        false,
    ));
    rows.push(event("request/header", json!({"reason":"change"})));
    let snapshot = read(rows);
    let cards = cards(&snapshot);
    assert_eq!(
        cards
            .iter()
            .map(|row| row.text.as_str())
            .collect::<Vec<_>>(),
        ["original", "model replacement"]
    );
    assert_eq!(cards[0].source_sequence, Some(3));
    assert_eq!(cards[1].source_sequence, Some(6));
}
#[test]
fn replacement_of_replacement_uses_model_position_and_non_system_removal_restores_survivor() {
    let mut rows = base();
    rows.push((
        "system/message",
        prompt(1, 1, "A"),
        json!({"surfaceOp":"append"}),
        false,
    ));
    rows.push((
        "system/message",
        prompt(1, 1, "B"),
        json!({"surfaceOp":"append"}),
        false,
    ));
    rows.push((
        "system/message",
        prompt(1, 1, "C"),
        json!({"surfaceOp":{"op":"replace","startSeq":4,"endSeq":4}}),
        true,
    ));
    rows.push((
        "developer/message",
        json!({"turn":99,"step":99}),
        json!({"surfaceOp":{"op":"replace","startSeq":5,"endSeq":5}}),
        false,
    ));
    rows.push(event("request/header", json!({"reason":"initial"})));
    let snapshot = read(rows);
    let cards = cards(&snapshot);
    assert_eq!(
        cards
            .iter()
            .map(|row| row.text.as_str())
            .collect::<Vec<_>>(),
        ["A", "B", "A"]
    );
    assert!(
        cards
            .iter()
            .all(|card| card.turn_id == snapshot.turns[0].id)
    );
}
#[test]
fn unknown_replacement_bounds_never_invent_effective_prompt_from_a_clipped_window() {
    let rows = vec![
        event("user/message", json!({"turn":1,"id":"u","text":"input"})),
        (
            "system/message",
            prompt(1, 1, "visible"),
            json!({"surfaceOp":"append"}),
            false,
        ),
        (
            "system/message",
            prompt(1, 1, "unproven"),
            json!({"surfaceOp":{"op":"replace","startSeq":0,"endSeq":0}}),
            false,
        ),
        event("request/header", json!({"reason":"initial"})),
    ];
    let snapshot = read(rows);
    assert_eq!(
        cards(&snapshot)
            .iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>(),
        ["visible"]
    );
    assert_ne!(
        cards(&snapshot)[0].system_prompt().unwrap().placement,
        TranscriptSystemPromptPlacement::Initial
    );
}
#[test]
fn unchanged_change_header_is_suppressed_and_session_prompt_has_no_fabricated_turn() {
    let mut rows = base();
    rows.push((
        "system/message",
        prompt(1, 1, "prompt"),
        json!({"surfaceOp":"append"}),
        false,
    ));
    rows.push(event("request/header", json!({"reason":"initial"})));
    rows.push(event("request/header", json!({"reason":"change"})));
    rows.push(event("turn/end", json!({"turn":1,"reason":"completed"})));
    rows.push(event("request/header", json!({"reason":"next"})));
    let snapshot = read(rows);
    let doc = TranscriptDocument::read(&snapshot).unwrap();
    assert_eq!(snapshot.turns.len(), 1);
    assert_eq!(cards(&snapshot).len(), 2);
    assert_eq!(doc.session_notices.len(), 1);
    assert!(doc.session_notices[0].item.turn_id.is_empty());
}

#[test]
fn known_inline_prompt_before_input_participates_in_segmented_activity_without_claiming_initial() {
    let rows = vec![
        event("turn/start", json!({"turn":1})),
        event("step/start", json!({"turn":1,"step":2})),
        event("system/message", prompt(1, 2, "inline")),
        event(
            "user/message",
            json!({"turn":1,"id":"later","text":"input","source":{"kind":"user","mode":"steer"}}),
        ),
        event("turn/end", json!({"turn":1,"reason":"completed"})),
    ];
    let snapshot = read(rows);
    let doc = TranscriptDocument::read(&snapshot).unwrap();
    let card = cards(&snapshot)[0];
    assert_eq!(
        card.system_prompt().unwrap().placement,
        TranscriptSystemPromptPlacement::Inline
    );
    let planes = doc.sections[0]
        .planes(&scope(), &snapshot.thinking_sources, false)
        .unwrap();
    assert_eq!(planes.items[&card.id].body, Some(TranscriptPlane::Activity));
    assert_eq!(
        planes.activity,
        TranscriptActivityPresentation::BetweenInputs
    );
}

#[test]
fn empty_rewrite_with_reversed_model_positions_does_not_discard_a_surviving_prompt() {
    let mut rows = base();
    rows.push((
        "system/message",
        prompt(1, 1, "A"),
        json!({"surfaceOp":"append"}),
        false,
    ));
    rows.push((
        "system/message",
        prompt(1, 1, "B"),
        json!({"surfaceOp":"append"}),
        false,
    ));
    // Sequence bounds refer to model-surface positions. The public envelope
    // permits this direction; the source fold removes no node in an empty range.
    rows.push((
        "system/message",
        prompt(1, 1, ""),
        json!({"surfaceOp":{"op":"replace","startSeq":4,"endSeq":3}}),
        true,
    ));
    rows.push(event("request/header", json!({"reason":"initial"})));
    let snapshot = read(rows);
    assert_eq!(
        cards(&snapshot)
            .iter()
            .map(|row| row.text.as_str())
            .collect::<Vec<_>>(),
        ["A", "B", "B"]
    );
}

#[test]
fn unknown_ignorable_surface_metadata_stays_opaque_and_cannot_drive_the_prompt_fold() {
    let mut rows = base();
    rows.push((
        "system/message",
        prompt(1, 1, "A"),
        json!({"surfaceOp":"append"}),
        false,
    ));
    rows.push(event("request/header", json!({"reason":"initial"})));
    rows.push((
        "vendor/opaque",
        json!({}),
        json!({"ignorable":true,"surfaceOp":{"startSeq":"opaque","endSeq":false}}),
        false,
    ));
    rows.push(event("request/header", json!({"reason":"change"})));
    let snapshot = read(rows);
    assert_eq!(
        cards(&snapshot)
            .iter()
            .map(|row| row.text.as_str())
            .collect::<Vec<_>>(),
        ["A"]
    );
}

#[test]
fn prompt_cards_sharing_an_initial_anchor_keep_source_version_order_not_lexical_ids() {
    let mut rows = base();
    rows.push(event("system/message", prompt(1, 1, "first")));
    for _ in 4..10 {
        rows.push(event("request/header", json!({"reason":"change"})));
    }
    rows.push(event("system/message", prompt(1, 1, "second")));
    let snapshot = read(rows);
    let doc = TranscriptDocument::read(&snapshot).unwrap();
    let order = doc.sections[0]
        .display_order(&scope(), &snapshot.thinking_sources)
        .unwrap();
    let actual: Vec<_> = order
        .ordered
        .iter()
        .filter_map(|event| match event {
            TranscriptOrderedEvent::Message(row)
                if row
                    .system_prompt()
                    .is_some_and(|p| p.placement == TranscriptSystemPromptPlacement::Initial) =>
            {
                row.source_sequence
            }
            _ => None,
        })
        .collect();
    assert_eq!(actual, [3, 10]);
}

#[test]
fn opening_input_after_machine_uses_identity_proof_and_does_not_become_inner_history() {
    let snapshot = read(vec![
        event("turn/start", json!({"turn":1})),
        event("step/start", json!({"turn":1,"step":2})),
        event("system/message", prompt(1, 2, "inline before opening")),
        event(
            "user/message",
            json!({"turn":1,"id":"opening","text":"input"}),
        ),
        event("turn/end", json!({"turn":1,"reason":"completed"})),
    ]);
    let doc = TranscriptDocument::read(&snapshot).unwrap();
    assert_eq!(
        doc.sections[0].inputs[0].item.input_role(),
        Some(TranscriptInputRole::Opening)
    );
    assert_eq!(
        doc.sections[0]
            .planes(&scope(), &snapshot.thinking_sources, false)
            .unwrap()
            .activity,
        TranscriptActivityPresentation::Settled {
            default_expanded: false
        }
    );
}
#[test]
fn consecutive_inputs_have_one_bound_opening_identity_and_inner_even_without_machine_content() {
    let snapshot = read(vec![
        event("turn/start", json!({"turn":1})),
        event(
            "user/message",
            json!({"turn":1,"id":"first","text":"first"}),
        ),
        event(
            "user/message",
            json!({"turn":1,"id":"second","text":"second"}),
        ),
        event("turn/end", json!({"turn":1,"reason":"completed"})),
    ]);
    let doc = TranscriptDocument::read(&snapshot).unwrap();
    assert_eq!(
        doc.sections[0]
            .inputs
            .iter()
            .map(|row| row.item.input_role())
            .collect::<Vec<_>>(),
        [
            Some(TranscriptInputRole::Opening),
            Some(TranscriptInputRole::Inner)
        ]
    );
    assert_eq!(
        doc.sections[0]
            .planes(&scope(), &snapshot.thinking_sources, false)
            .unwrap()
            .activity,
        TranscriptActivityPresentation::BetweenInputs
    );
    let mut copied = doc.sections[0].inputs[0].item.clone();
    copied.id = "other".into();
    assert_eq!(copied.input_role(), None);
    let mut copied = doc.sections[0].inputs[0].item.clone();
    copied.turn_id = "other".into();
    assert_eq!(copied.input_role(), None);
}
