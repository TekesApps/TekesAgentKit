use serde_json::{Value, json};
use tekes_agent_contract::NativeConversationSnapshot;
use tekes_agent_presentation::{RunScope, TranscriptSnapshot};
use tekes_agent_session::SessionProjection;

fn started(record: &str, run: Value) -> Value {
    json!({"id":record,"type":"custom","customType":"agentkit.run.v1","data":{"id":run,"phase":"started"}})
}

fn settled(record: &str, run: Value, timestamp: f64, aborted: bool) -> Value {
    json!({"id":record,"type":"custom","customType":"agentkit.run.v1","data":{"id":run,"phase":"settled","timestamp":timestamp,"aborted":aborted}})
}

fn user(record: &str) -> Value {
    json!({"id":record,"type":"message","message":{"role":"user","timestamp":1,"content":[{"type":"text","text":record}]}})
}

fn assistant(record: &str, reason: &str) -> Value {
    json!({"id":record,"type":"message","message":{"role":"assistant","timestamp":2,"stopReason":reason,"content":[{"type":"text","text":record}]}})
}

fn tool_call(record: &str) -> Value {
    json!({"id":record,"type":"message","message":{"role":"assistant","timestamp":2,"stopReason":"toolUse","content":[{"type":"toolCall","id":record,"name":record,"arguments":{}}]}})
}

fn transcript(entries: Vec<Value>, is_streaming: bool) -> TranscriptSnapshot {
    let session = SessionProjection {
        journal_ready: true,
        native_conversation: Some(NativeConversationSnapshot {
            session_id: "session".into(),
            generation: "generation".into(),
            leaf_id: entries
                .last()
                .and_then(|entry| entry["id"].as_str())
                .map(str::to_owned),
            entries,
            has_more_before: false,
            is_streaming,
            state: json!({}),
        }),
        ..Default::default()
    };
    TranscriptSnapshot::read(&RunScope::main("native", "session"), &session).unwrap()
}

fn turn_for<'a>(
    snapshot: &'a TranscriptSnapshot,
    item: &Value,
) -> &'a tekes_agent_presentation::TranscriptTurn {
    let id = snapshot
        .items
        .iter()
        .find(|row| row.record_id.as_deref() == item["id"].as_str())
        .unwrap()
        .turn_id
        .as_str();
    snapshot.turns.iter().find(|turn| turn.id == id).unwrap()
}

#[test]
fn explicit_start_without_settlement_stays_running_even_if_snapshot_is_not_streaming() {
    for content in [
        vec![user("u")],
        vec![
            user("u"),
            assistant("error", "error"),
            assistant("stop", "stop"),
        ],
    ] {
        let mut entries = vec![started("s", json!("run"))];
        entries.extend(content);
        let snapshot = transcript(entries, false);
        assert_eq!(snapshot.turns.len(), 1);
        assert_eq!(snapshot.turns[0].status, "running");
        assert_eq!(snapshot.turns[0].ended_at, None);
    }
}

#[test]
fn matching_settlement_closes_only_its_run_and_keeps_tool_ownership_separate() {
    let snapshot = transcript(
        vec![
            started("a-start", json!("A")),
            user("a-user"),
            tool_call("a-tool"),
            started("b-start", json!("B")),
            user("b-user"),
            tool_call("b-tool"),
            settled("a-end", json!("A"), 41.0, false),
        ],
        false,
    );
    let a = turn_for(&snapshot, &json!({"id":"a-user"}));
    let b = turn_for(&snapshot, &json!({"id":"b-user"}));
    assert_eq!(a.status, "completed");
    assert_eq!(a.ended_at, Some(41.0));
    assert_eq!(a.fork_boundary, Some(json!({"entryId":"a-end"})));
    assert_eq!(b.status, "running");
    let tool_a = snapshot
        .tools
        .iter()
        .find(|tool| tool.name == "a-tool")
        .unwrap();
    let tool_b = snapshot
        .tools
        .iter()
        .find(|tool| tool.name == "b-tool")
        .unwrap();
    assert_eq!(tool_a.turn_id, a.id);
    assert_eq!(tool_b.turn_id, b.id);
    assert_eq!(tool_a.phase, "stopped");
    assert_eq!(tool_b.phase, "doing");
    assert_eq!(b.ended_at, None);
    assert_eq!(b.fork_boundary, None);
}

#[test]
fn malformed_or_unknown_settlements_do_not_close_the_current_explicit_run() {
    for boundary in [
        json!({"id":"other","phase":"settled"}),
        json!({"phase":"settled"}),
        json!({"id":7,"phase":"settled"}),
    ] {
        let entries = vec![
            started("s", json!("run")),
            user("u"),
            json!({"id":"x","type":"custom","customType":"agentkit.run.v1","data":boundary}),
        ];
        let snapshot = transcript(entries, false);
        assert_eq!(snapshot.turns.len(), 1);
        assert_eq!(snapshot.turns[0].status, "running");
        assert_eq!(snapshot.turns[0].fork_boundary, None);
    }
    let snapshot = transcript(
        vec![
            started("s", json!("run")),
            user("u"),
            settled("unknown", json!("other"), 9.0, false),
        ],
        false,
    );
    assert_eq!(snapshot.turns[0].status, "running");
}

#[test]
fn post_settlement_user_remains_in_the_run_and_later_failure_is_accumulated() {
    let snapshot = transcript(
        vec![
            started("s", json!("run")),
            user("first"),
            settled("e", json!("run"), 10.0, false),
            user("later-user"),
            assistant("later-error", "error"),
        ],
        false,
    );
    let first = turn_for(&snapshot, &json!({"id":"first"}));
    let later = turn_for(&snapshot, &json!({"id":"later-user"}));
    assert_eq!(first.id, later.id);
    assert_eq!(first.input, "first");
    assert_eq!(
        snapshot
            .items
            .iter()
            .find(|item| item.record_id.as_deref() == Some("later-user"))
            .unwrap()
            .input_role(),
        Some(tekes_agent_presentation::TranscriptInputRole::Inner)
    );
    assert_eq!(first.status, "error");
    assert_eq!(first.fork_boundary, Some(json!({"entryId":"e"})));
}

#[test]
fn settlement_without_a_started_run_does_not_create_a_ghost_and_legacy_still_settles() {
    let snapshot = transcript(vec![settled("orphan", json!("orphan"), 1.0, false)], false);
    assert!(snapshot.turns.is_empty());
    let snapshot = transcript(
        vec![user("legacy-user"), assistant("legacy-stop", "stop")],
        false,
    );
    assert_eq!(snapshot.turns.len(), 1);
    assert_eq!(snapshot.turns[0].status, "completed");
}

#[test]
fn repeated_matching_settlements_use_the_last_boundary_and_any_aborted_settlement_wins() {
    let snapshot = transcript(
        vec![
            started("s", json!("run")),
            user("u"),
            settled("first", json!("run"), 10.0, true),
            settled("last", json!("run"), 20.0, false),
        ],
        false,
    );
    assert_eq!(snapshot.turns[0].status, "aborted");
    assert_eq!(snapshot.turns[0].ended_at, Some(20.0));
    assert_eq!(
        snapshot.turns[0].fork_boundary,
        Some(json!({"entryId":"last"}))
    );
}

#[test]
fn pre_scanned_settlement_pairs_with_a_later_start_and_empty_string_ids_are_valid() {
    let snapshot = transcript(
        vec![
            settled("before", json!("run"), 20.0, false),
            started("start", json!("run")),
            user("u"),
        ],
        false,
    );
    assert_eq!(snapshot.turns.len(), 1);
    assert_eq!(snapshot.turns[0].status, "completed");
    assert_eq!(snapshot.turns[0].ended_at, Some(20.0));
    assert_eq!(
        snapshot.turns[0].fork_boundary,
        Some(json!({"entryId":"before"}))
    );

    let snapshot = transcript(
        vec![
            started("empty-start", json!("")),
            user("empty-user"),
            settled("empty-end", json!(""), 3.0, false),
        ],
        false,
    );
    assert_eq!(snapshot.turns.len(), 1);
    assert_eq!(snapshot.turns[0].status, "completed");
}

#[test]
fn matching_settlement_completes_a_normal_run_end_to_end() {
    let snapshot = transcript(
        vec![
            started("s", json!("run")),
            user("u"),
            assistant("a", "stop"),
            settled("e", json!("run"), 5.0, false),
        ],
        false,
    );
    assert_eq!(snapshot.turns[0].status, "completed");
    assert_eq!(snapshot.turns[0].ended_at, Some(5.0));
    assert!(
        snapshot
            .items
            .iter()
            .find(|item| item.record_id.as_deref() == Some("a"))
            .unwrap()
            .is_final
    );
}

#[test]
fn marker_only_runs_do_not_create_visible_turns_or_navigation_targets() {
    for entries in [
        vec![started("s", json!("run"))],
        vec![
            started("s", json!("run")),
            settled("e", json!("run"), 2.0, false),
        ],
    ] {
        let snapshot = transcript(entries, false);
        assert!(snapshot.turns.is_empty());
        assert!(snapshot.items.is_empty());
    }
}

#[test]
fn invalid_started_ids_leave_original_legacy_message_grouping_intact() {
    let legacy = transcript(vec![user("u"), assistant("a", "stop")], false);
    for id in [Value::Null, json!(7)] {
        let snapshot = transcript(
            vec![started("invalid", id), user("u"), assistant("a", "stop")],
            false,
        );
        assert_eq!(snapshot.turns.len(), 1);
        assert_eq!(snapshot.turns[0].id, legacy.turns[0].id);
        assert_eq!(snapshot.turns[0].status, "completed");
    }
}
