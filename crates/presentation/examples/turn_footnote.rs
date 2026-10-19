//! A second public-API consumer: only a durable settlement supplies a fork boundary.
use serde_json::json;
use tekes_agent_contract::{HistoryEntry, JournalSnapshot};
use tekes_agent_presentation::{RunScope, TranscriptSnapshot};
use tekes_agent_session::SessionProjection;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut session = SessionProjection::default();
    let entry = |seq, kind, data| -> HistoryEntry {
        serde_json::from_value(json!({"event":{"seq":seq,"type":kind,"time":1,"data":data}}))
            .unwrap()
    };
    session.apply_snapshot(JournalSnapshot {
        session_id: "demo".into(),
        generation: "g".into(),
        through_sequence: 0,
        window_limit: 50,
        entries: vec![entry(0, "turn/start", json!({"turn":1}))],
        has_more_before: false,
        projections: None,
        history_revision: None,
    })?;
    let scope = RunScope::main("consumer", "demo");
    assert!(
        TranscriptSnapshot::read(&scope, &session)?.turns[0]
            .fork_boundary
            .is_none()
    );
    session.apply_live(entry(1, "turn/end", json!({"turn":1,"reason":"completed"})))?;
    let restored: SessionProjection = serde_json::from_value(serde_json::to_value(session)?)?;
    let view = TranscriptSnapshot::read(&scope, &restored)?;
    assert_eq!(view.turns[0].fork_boundary, Some(json!({"atSeq":1})));
    println!("TURN_FOOTNOTE_BOUNDARY_RESTORED_OK");
    Ok(())
}
