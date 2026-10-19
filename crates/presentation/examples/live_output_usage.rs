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
    Ok(())
}
