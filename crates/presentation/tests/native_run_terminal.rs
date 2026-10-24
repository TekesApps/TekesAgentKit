use serde_json::{Value, json};
use tekes_agent_contract::NativeConversationSnapshot;
use tekes_agent_presentation::{RunScope, TranscriptSnapshot};
use tekes_agent_session::SessionProjection;

fn marker(record: &str, run: &str, phase: &str, aborted: bool) -> Value {
    json!({
        "id": record,
        "type": "custom",
        "customType": "agentkit.run.v1",
        "data": {"id": run, "phase": phase, "aborted": aborted}
    })
}

fn assistant(record: &str, stop_reason: &str) -> Value {
    json!({
        "id": record,
        "type": "message",
        "message": {
            "role": "assistant",
            "timestamp": 1,
            "stopReason": stop_reason,
            "content": [{"type":"text", "text":record}]
        }
    })
}

fn transcript(entries: Vec<Value>) -> TranscriptSnapshot {
    transcript_with_streaming(entries, false)
}

fn transcript_with_streaming(entries: Vec<Value>, is_streaming: bool) -> TranscriptSnapshot {
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

fn run(entries: Vec<Value>) -> Vec<Value> {
    let mut records = vec![marker("started", "run", "started", false)];
    records.extend(entries);
    records.push(marker("settled", "run", "settled", false));
    records
}

#[test]
fn explicit_run_error_remains_failed_after_a_normal_stop() {
    let snapshot = transcript(run(vec![
        assistant("failed", "error"),
        assistant("stop", "stop"),
    ]));
    assert_eq!(snapshot.turns.len(), 1);
    assert_eq!(snapshot.turns[0].status, "error");
    assert_eq!(snapshot.turns[0].id, snapshot.items[0].turn_id);
    assert_eq!(
        snapshot.items.len(),
        2,
        "source rows retain their identities"
    );
}

#[test]
fn explicit_run_error_remains_failed_after_tool_use() {
    let snapshot = transcript(run(vec![
        assistant("failed", "error"),
        assistant("tool", "toolUse"),
    ]));
    assert_eq!(snapshot.turns[0].status, "error");
    assert_eq!(snapshot.items.len(), 2);
}

#[test]
fn explicit_run_aborted_reason_survives_a_later_normal_stop() {
    let snapshot = transcript(run(vec![
        assistant("cancelled", "aborted"),
        assistant("stop", "stop"),
    ]));
    assert_eq!(snapshot.turns[0].status, "aborted");
}

#[test]
fn terminal_evidence_does_not_leak_into_a_later_explicit_run() {
    let mut entries = run(vec![
        assistant("failed", "error"),
        assistant("stop", "stop"),
    ]);
    entries.push(marker("next-started", "next", "started", false));
    entries.push(assistant("next-stop", "stop"));
    entries.push(marker("next-settled", "next", "settled", false));
    let snapshot = transcript(entries);
    assert_eq!(snapshot.turns.len(), 2);
    assert_eq!(snapshot.turns[0].status, "error");
    assert_eq!(snapshot.turns[1].status, "completed");
    assert_ne!(snapshot.turns[0].id, snapshot.turns[1].id);
}

#[test]
fn failure_evidence_does_not_settle_a_still_running_explicit_run() {
    for reason in ["error", "aborted"] {
        let snapshot = transcript_with_streaming(
            vec![
                marker("started", "run", "started", false),
                assistant("terminal-message", reason),
            ],
            true,
        );
        assert_eq!(snapshot.turns[0].status, "running");
        assert_eq!(snapshot.turns[0].ended_at, None);
    }
}

#[test]
fn cancellation_wins_over_failure_in_either_order_and_at_the_run_boundary() {
    for reasons in [["aborted", "error"], ["error", "aborted"]] {
        let snapshot = transcript(run(vec![
            assistant("first", reasons[0]),
            assistant("second", reasons[1]),
            assistant("last", "stop"),
        ]));
        assert_eq!(snapshot.turns[0].status, "aborted");
        assert!(snapshot.items.iter().all(|item| !item.is_final));
    }
    let snapshot = transcript(vec![
        marker("started", "run", "started", false),
        assistant("failure", "error"),
        marker("settled", "run", "settled", true),
    ]);
    assert_eq!(snapshot.turns[0].status, "aborted");
}

#[test]
fn error_text_without_a_failure_stop_is_not_terminal_evidence() {
    let mut message = assistant("normal", "stop");
    message["message"]["errorMessage"] = json!("retained diagnostic error text");
    let snapshot = transcript(run(vec![message]));
    assert_eq!(snapshot.turns[0].status, "completed");
    assert!(snapshot.items[0].is_final);
}
