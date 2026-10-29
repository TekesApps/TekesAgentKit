use serde_json::{Value, json};
use tekes_agent_contract::{HistoryEntry, JournalSnapshot};
use tekes_agent_presentation::{RunScope, TranscriptSnapshot};
use tekes_agent_session::SessionProjection;
fn event(seq: i64, kind: &str, data: Value) -> HistoryEntry {
    serde_json::from_value(json!({"event":{"type":kind,"seq":seq,"time":100+seq,"data":data}}))
        .unwrap()
}
fn prefix() -> Vec<HistoryEntry> {
    vec![
        event(0, "turn/start", json!({"turn":1})),
        call(1, 1, "c", "owned"),
    ]
}
fn call(seq: i64, turn: i64, id: &str, label: &str) -> HistoryEntry {
    event(
        seq,
        "tool/call",
        json!({"turn":turn,"step":1,"callId":id,"name":"shell","arguments":{"command":label}}),
    )
}
fn result(seq: i64, turn: i64, id: &str, failed: bool) -> HistoryEntry {
    let mut e = event(
        seq,
        "tool/result",
        json!({"turn":turn,"step":1,"message":{"source":{"callId":id},"isError":failed,"content":[{"type":"text","text":"owned result"}]},"meta":{"owned":"result metadata"}}),
    );
    e.view = Some(json!({"for":"result","view":{"title":"owned result title"}}));
    e
}
fn end(seq: i64, turn: i64, reason: &str) -> HistoryEntry {
    event(seq, "turn/end", json!({"turn":turn,"reason":reason}))
}
fn session(entries: Vec<HistoryEntry>) -> SessionProjection {
    let mut p = SessionProjection::default();
    p.apply_snapshot(JournalSnapshot {
        session_id: "a".into(),
        generation: "g".into(),
        through_sequence: entries.last().unwrap().event.seq,
        window_limit: 50,
        entries,
        has_more_before: false,
        projections: None,
        history_revision: None,
    })
    .unwrap();
    p
}
fn read(p: &SessionProjection) -> TranscriptSnapshot {
    TranscriptSnapshot::read(&RunScope::main("e", "a"), p).unwrap()
}
#[test]
fn closed_pending_call_cannot_be_revived_by_late_success_or_error_on_live_or_replay() {
    for failed in [false, true] {
        let mut entries = prefix();
        entries.push(end(2, 1, "aborted"));
        let mut live = session(entries.clone());
        let sealed = read(&live);
        assert_eq!(sealed.tools[0].phase, "stopped");
        assert!(sealed.tools[0].result.is_none());
        let late = result(3, 1, "c", failed);
        live.apply_live(late.clone()).unwrap();
        entries.push(late);
        assert_eq!(read(&live).tools, sealed.tools);
        assert_eq!(read(&session(entries)).tools, sealed.tools);
        assert_eq!(read(&live).turns[0].status, "aborted");
    }
}
#[test]
fn settled_result_and_original_metadata_remain_sealed_after_turn_close() {
    let mut entries = prefix();
    entries.push(result(2, 1, "c", false));
    entries.push(end(3, 1, "completed"));
    let mut p = session(entries);
    let sealed = read(&p);
    assert_eq!(sealed.tools[0].phase, "done");
    p.apply_live(result(4, 1, "c", true)).unwrap();
    assert_eq!(read(&p).tools, sealed.tools);
}
#[test]
fn orphan_result_after_known_close_never_creates_a_tool_row() {
    let mut entries = prefix();
    entries.push(end(2, 1, "completed"));
    let mut p = session(entries);
    let sealed = read(&p);
    p.apply_live(result(3, 1, "unowned", false)).unwrap();
    assert_eq!(read(&p).tools, sealed.tools);
}
#[test]
fn later_tool_call_cannot_rewrite_a_settled_active_calls_arguments_name_or_view() {
    let mut entries = prefix();
    entries.push(result(2, 1, "c", false));
    let mut p = session(entries);
    let sealed = read(&p);
    let mut late = call(3, 1, "c", "must not overwrite");
    late.event.data["name"] = json!("different");
    late.view = Some(json!({"for":"call","view":{"title":"must not replace"}}));
    p.apply_live(late).unwrap();
    assert_eq!(read(&p).tools, sealed.tools);
}
#[test]
fn assistant_tool_references_do_not_rewrite_an_already_admitted_call() {
    let mut p = session(prefix());
    let owned = read(&p);
    p.apply_live(event(2,"assistant/message",json!({"turn":1,"step":1,"message":{"id":"narration","content":[{"type":"tool-call","id":"c","name":"different","arguments":{"command":"must not overwrite"}}]}}))).unwrap();
    assert_eq!(read(&p).tools, owned.tools);
}
#[test]
fn later_turn_and_result_only_window_without_a_close_boundary_still_use_real_results() {
    let mut entries = prefix();
    entries.push(end(2, 1, "aborted"));
    entries.push(event(3, "turn/start", json!({"turn":2})));
    entries.push(call(4, 2, "next", "new call"));
    entries.push(result(5, 2, "next", false));
    entries.push(end(6, 2, "completed"));
    let s = read(&session(entries));
    assert_eq!(s.tools.len(), 2);
    assert_eq!(s.tools[0].phase, "stopped");
    assert_eq!(s.tools[1].phase, "done");
    let mut partial = session(vec![result(20, 7, "outside-window", true)]);
    partial.has_more_before = true;
    partial.run_open = false;
    let s = read(&partial);
    assert_eq!(s.tools.len(), 1);
    assert_eq!(s.tools[0].phase, "error");
    assert!(s.tools[0].source_position.is_none());
    assert!(s.tools[0].result_position.is_some());
}
#[test]
fn explicit_new_call_admission_after_close_is_distinct_from_an_unowned_late_result() {
    let mut entries = prefix();
    entries.push(end(2, 1, "completed"));
    entries.push(call(3, 1, "new-explicit", "new source call"));
    entries.push(result(4, 1, "new-explicit", false));
    let s = read(&session(entries));
    assert_eq!(s.tools.len(), 2);
    assert_eq!(s.tools[0].phase, "stopped");
    assert_eq!(s.tools[1].phase, "done");
    assert_eq!(s.turns[0].status, "completed");
}
#[test]
fn pending_explicit_call_enriches_inline_admission_without_moving_its_position() {
    let mut p = session(vec![event(
        0,
        "assistant/message",
        json!({"turn":1,"step":1,"message":{"id":"narration","content":[{"type":"tool-call","id":"c","name":"shell","arguments":{"command":"initial"}}]}}),
    )]);
    let position = read(&p).tools[0].source_position.clone();
    let mut explicit = call(1, 1, "c", "expanded");
    explicit.view = Some(json!({"for":"call","view":{"title":"owned call title"}}));
    p.apply_live(explicit.clone()).unwrap();
    let s = read(&p);
    assert_eq!(s.tools.len(), 1);
    assert_eq!(s.tools[0].source_position, position);
    assert_eq!(s.tools[0].arguments["command"], "expanded");
    assert_eq!(s.tools[0].call_view, explicit.view);
}
#[test]
fn explicit_readmission_of_a_retired_id_has_fresh_source_facts() {
    let mut entries = prefix();
    entries.push(end(2, 1, "aborted"));
    entries.push(call(3, 2, "c", "fresh"));
    entries.push(result(4, 2, "c", false));
    let s = read(&session(entries));
    assert_eq!(s.tools.len(), 1);
    assert_eq!(s.tools[0].arguments["command"], "fresh");
    assert_eq!(s.tools[0].phase, "done");
    assert_eq!(
        serde_json::to_value(&s.tools[0].source_position).unwrap()["sequence"],
        3
    );
}
