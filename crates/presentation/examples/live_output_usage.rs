//! Second public-API consumer for provider-reported live output usage.
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
        generation: "demo".into(),
        through_sequence: 0,
        window_limit: 50,
        entries: vec![entry(0, "turn/start", json!({"turn":1}))],
        has_more_before: false,
        projections: None,
        history_revision: None,
    })?;
    for (index, count) in [0, 12, 38].into_iter().enumerate() {
        session.apply_live(entry(
            index as i64 + 1,
            "assistant/chunk",
            json!({"turn":1,"step":1,"chunk":{"type":"usage","usage":{"outputTokens":count}}}),
        ))?;
        let view = TranscriptSnapshot::read(&RunScope::main("public-consumer", "demo"), &session)?;
        let value = serde_json::to_value(&view.turns[0])?;
        assert_eq!(value["liveUsage"]["outputTokens"], count);
        println!("{}", value["liveUsage"]);
    }
    session.apply_live(entry(4, "assistant/message", json!({"turn":1,"step":1,"message":{"id":"response-1","content":[]},"usage":{"outputTokens":38}})))?;
    session.apply_live(entry(5, "assistant/message", json!({"turn":1,"step":2,"message":{"id":"response-2","content":[]},"usage":{"outputTokens":7}})))?;
    session.apply_live(entry(6, "turn/end", json!({"turn":1,"reason":"completed"})))?;
    let restored: SessionProjection = serde_json::from_value(serde_json::to_value(session)?)?;
    let history = TranscriptSnapshot::read(&RunScope::main("public-consumer", "demo"), &restored)?;
    let report = history.turns[0].reported_usage.as_ref().unwrap();
    assert_eq!(report.output_tokens, 45);
    assert!(report.is_complete);
    println!("{}", serde_json::to_string(report)?);
    Ok(())
}
