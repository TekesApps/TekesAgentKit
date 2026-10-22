use serde_json::json;
use tekes_agent_contract::*;
use tekes_agent_presentation::*;
use tekes_agent_session::SessionProjection;
fn snapshot(closed: bool) -> TranscriptSnapshot {
    let mut events = vec![
        ("turn/start", json!({"turn":1})),
        (
            "user/message",
            json!({"turn":1,"id":"u","content":[{"type":"text","text":"input"}]}),
        ),
        (
            "tool/call",
            json!({"turn":1,"step":1,"callId":"call","name":"read","arguments":{}}),
        ),
        (
            "assistant/message",
            json!({"turn":1,"step":1,"sessionFinal":false,"message":{"id":"n","content":[{"type":"reasoning","text":"process"},{"type":"text","text":"narration"}]}}),
        ),
        (
            "assistant/message",
            json!({"turn":1,"step":2,"sessionFinal":true,"message":{"id":"f","content":[{"type":"reasoning","text":"retained final process"},{"type":"text","text":"answer"}]}}),
        ),
        (
            "assistant/message",
            json!({"turn":1,"step":3,"sessionFinal":false,"message":{"id":"tail","content":[{"type":"text","text":"commentary"}]}}),
        ),
    ];
    if closed {
        events.push(("turn/end", json!({"turn":1,"reason":"completed"})));
    }
    let entries: Vec<HistoryEntry> = events
        .into_iter()
        .enumerate()
        .map(|(seq, (kind, data))| {
            serde_json::from_value(
                json!({"event":{"type":kind,"seq":seq,"time":seq+1,"data":data}}),
            )
            .unwrap()
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
    TranscriptSnapshot::read(&RunScope::main("sep", "s"), &session).unwrap()
}
#[test]
fn final_body_and_retained_reasoning_have_distinct_planes_without_reclassifying_source_kind() {
    let source = snapshot(true);
    let original = source.clone();
    let doc = TranscriptDocument::read(&source).unwrap();
    let planes = doc.sections[0]
        .planes(&RunScope::main("sep", "s"), &source.thinking_sources, false)
        .unwrap();
    assert!(planes.has_activity && planes.has_output);
    for row in source
        .items
        .iter()
        .filter(|row| row.kind == TranscriptItemKind::Assistant)
    {
        let facts = &planes.items[&row.id];
        match row.text.as_str() {
            "narration" => {
                assert_eq!(facts.body, Some(TranscriptPlane::Activity));
                assert_eq!(facts.is_narration, Some(true));
                assert!(facts.has_reasoning);
            }
            "answer" => {
                assert_eq!(facts.body, Some(TranscriptPlane::Output));
                assert_eq!(facts.is_narration, Some(false));
                assert!(facts.has_reasoning);
            }
            "commentary" => {
                assert_eq!(facts.body, Some(TranscriptPlane::Activity));
                assert_eq!(facts.is_narration, Some(false));
            }
            _ => panic!("unexpected fixture"),
        }
    }
    assert_eq!(source, original);
}
#[test]
fn a_live_final_candidate_does_not_settle_its_activity_owner() {
    let source = snapshot(false);
    let doc = TranscriptDocument::read(&source).unwrap();
    let live = doc.sections[0]
        .planes(&RunScope::main("sep", "s"), &source.thinking_sources, true)
        .unwrap();
    assert!(live.is_live_owner && live.has_activity);
    assert!(!live.has_output);
    let candidate = source
        .items
        .iter()
        .find(|row| row.text == "answer")
        .unwrap();
    assert_eq!(
        live.items[&candidate.id].body,
        Some(TranscriptPlane::Activity)
    );
    let retained = doc.sections[0]
        .planes(&RunScope::main("sep", "s"), &source.thinking_sources, false)
        .unwrap();
    assert!(!retained.is_live_owner);
}
#[test]
fn missing_frame_provenance_is_explicit_and_foreign_or_settled_live_owners_fail() {
    let source = snapshot(true);
    let doc = TranscriptDocument::read(&source).unwrap();
    let scope = RunScope::main("sep", "s");
    assert!(
        doc.sections[0]
            .planes(&scope, &source.thinking_sources, true)
            .is_err()
    );
    assert!(
        doc.sections[0]
            .planes(
                &RunScope::main("other", "s"),
                &source.thinking_sources,
                false
            )
            .is_err()
    );
    let legacy = doc.sections[0]
        .planes(&scope, &TranscriptThinkingSources::default(), false)
        .unwrap();
    let narration = source
        .items
        .iter()
        .find(|row| row.text == "narration")
        .unwrap();
    assert_eq!(legacy.items[&narration.id].is_narration, None);
    assert_eq!(
        legacy.items[&narration.id].body,
        Some(TranscriptPlane::Activity)
    );
}

#[test]
fn unowned_maps_never_correlate_frames_and_empty_owned_frames_are_rejected() {
    let source = snapshot(true);
    let doc = TranscriptDocument::read(&source).unwrap();
    let scope = RunScope::main("sep", "s");
    let narration = source
        .items
        .iter()
        .find(|row| row.text == "narration")
        .unwrap();
    let mut unowned = (*source.thinking_sources).clone();
    unowned.scope = None;
    let planes = doc.sections[0].planes(&scope, &unowned, false).unwrap();
    assert_eq!(planes.items[&narration.id].is_narration, None);
    for id in [&narration.id, &source.tools[0].id] {
        let mut invalid = (*source.thinking_sources).clone();
        invalid.by_id.get_mut(id).unwrap().response_frame_id.clear();
        assert!(doc.sections[0].planes(&scope, &invalid, false).is_err());
    }
}
