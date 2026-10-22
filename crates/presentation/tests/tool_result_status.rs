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
        event(
            1,
            "tool/call",
            json!({"turn":1,"callId":"c","name":"shell","arguments":{"command":"owned test"}}),
        ),
    ]
}
fn result(extra: Value, view: Option<Value>) -> HistoryEntry {
    let mut data = json!({"turn":1,"message":{"source":{"callId":"c"},"isError":false,"content":[{"type":"text","text":"owned output"}]}});
    for (k, v) in extra.as_object().unwrap() {
        data[k] = v.clone();
    }
    let mut e = event(2, "tool/result", data);
    e.view = view;
    e
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
fn snapshot(extra: Value, view: Option<Value>) -> TranscriptSnapshot {
    let mut entries = prefix();
    entries.push(result(extra, view));
    read(&session(entries))
}
#[test]
fn public_durable_failure_markers_do_not_show_completed_tool_success() {
    for (label, meta) in [
        ("exit", json!({"exitCode":7})),
        ("negative exit", json!({"exitCode":-1})),
        ("signal", json!({"signal":"SIGTERM"})),
        ("provided empty signal", json!({"signal":""})),
        ("timeout", json!({"timedOut":true})),
        (
            "persistent exited",
            json!({"sessionStatus":{"kind":"exited","exitCode":3}}),
        ),
        (
            "persistent signal",
            json!({"sessionStatus":{"kind":"exited","signal":"SIGINT"}}),
        ),
    ] {
        let s = snapshot(json!({"meta":meta}), None);
        assert_eq!(s.tools[0].phase, "error", "{label}");
    }
}
#[test]
fn any_content_error_is_observed_and_interrupted_takes_priority_over_all_failure_markers() {
    let message = json!({"source":{"callId":"c"},"isError":false,"content":[{"type":"text","text":"first"},{"type":"text","text":"second failed","isError":true}]});
    assert_eq!(
        snapshot(json!({"message":message}), None).tools[0].phase,
        "error"
    );
    let s = snapshot(
        json!({"message":message,"error":{"code":"interrupted"},"meta":{"exitCode":9,"timedOut":true}}),
        Some(json!({"for":"result","view":{"signal":"SIGTERM"}})),
    );
    assert_eq!(s.tools[0].phase, "stopped");
}
#[test]
fn matching_result_view_failure_is_used_without_treating_call_or_unknown_views_as_result_authority()
{
    for view in [
        json!({"for":"result","view":{"exitCode":8}}),
        json!({"for":"result","view":{"signal":"SIGTERM"}}),
    ] {
        let s = snapshot(json!({}), Some(view.clone()));
        assert_eq!(s.tools[0].phase, "error");
        assert_eq!(s.tools[0].result_view, Some(view));
    }
    for view in [
        json!({"for":"call","view":{"exitCode":9}}),
        json!({"for":"future","view":{"signal":"future"}}),
        json!({"exitCode":7}),
        json!({"for":"result","view":{"exitCode":"7","signal":false}}),
    ] {
        let s = snapshot(json!({}), Some(view.clone()));
        assert_eq!(s.tools[0].phase, "done");
        assert_eq!(
            s.tools[0].result_view,
            Some(view),
            "unknown raw facts must remain available"
        );
    }
}
#[test]
fn malformed_or_negative_markers_do_not_invent_failures_from_output_text() {
    for extra in [
        json!({"error":false}),
        json!({"error":null}),
        json!({"error":"failed"}),
        json!({"error":[]}),
        json!({"meta":{"exitCode":0,"signal":null,"timedOut":false}}),
        json!({"meta":{"exitCode":"2","signal":false,"timedOut":1}}),
        json!({"meta":{"sessionStatus":{"kind":"running","exitCode":2,"signal":"SIGTERM"}}}),
        json!({"meta":{"sessionStatus":{"kind":"exited","exitCode":0}}}),
    ] {
        assert_eq!(snapshot(extra, None).tools[0].phase, "done");
    }
    assert_eq!(snapshot(json!({"error":{}}), None).tools[0].phase, "error");
}
#[test]
fn live_and_history_replay_preserve_failure_raw_result_identity_and_independent_turn_settlement() {
    let end = event(3, "turn/end", json!({"turn":1,"reason":"completed"}));
    let result = result(
        json!({"meta":{"timedOut":true}}),
        Some(json!({"for":"result","view":{"title":"Timed out"}})),
    );
    let mut live = session(prefix());
    let before = read(&live);
    assert_eq!(before.tools[0].phase, "doing");
    live.apply_live(result.clone()).unwrap();
    live.apply_live(end.clone()).unwrap();
    let live = read(&live);
    let mut entries = prefix();
    entries.push(result.clone());
    entries.push(end);
    let replay = read(&session(entries));
    assert_eq!(live.tools, replay.tools);
    assert_eq!(live.tools[0].id, before.tools[0].id);
    assert_eq!(live.tools[0].phase, "error");
    assert_eq!(
        live.tools[0].result,
        Some(result.event.data["message"].clone())
    );
    assert_eq!(
        live.turns[0].status, "completed",
        "tool failure does not decide the whole turn"
    );
}

#[test]
fn numeric_exit_status_matches_clientkit_exact_json_integer_semantics() {
    assert_eq!(
        snapshot(json!({"meta":{"exitCode":1.0}}), None).tools[0].phase,
        "error"
    );
    assert_eq!(
        snapshot(
            json!({"meta":{"sessionStatus":{"kind":"exited","exitCode":2.0}}}),
            None
        )
        .tools[0]
            .phase,
        "error"
    );
    assert_eq!(
        snapshot(
            json!({}),
            Some(json!({"for":"result","view":{"exitCode":3.0}}))
        )
        .tools[0]
            .phase,
        "error"
    );
    for code in [
        json!(0.0),
        json!(-0.0),
        json!(0.5),
        json!(1e30),
        json!(i64::MAX),
        json!("1"),
    ] {
        assert_eq!(
            snapshot(json!({"meta":{"exitCode":code}}), None).tools[0].phase,
            "done"
        );
    }
    assert_eq!(
        snapshot(json!({"meta":{"exitCode":i64::MIN}}), None).tools[0].phase,
        "error"
    );
}

#[test]
fn result_metadata_and_error_roundtrip_while_legacy_snapshots_keep_optional_defaults() {
    let meta = json!({"exitCode":4,"activitySummary":{"verb":"read","subject":"a.rs"},"future":[1,{"opaque":true}]});
    let error =
        json!({"name":"OwnedToolFailure","code":"owned_failure","future":{"opaque":"retained"}});
    let s = snapshot(json!({"meta":meta,"error":error}), None);
    assert_eq!(s.tools[0].phase, "error");
    assert_eq!(s.tools[0].result_meta, Some(meta.clone()));
    assert_eq!(s.tools[0].result_error, Some(error.clone()));
    let mut encoded = serde_json::to_value(&s).unwrap();
    assert_eq!(encoded["tools"][0]["resultMeta"], meta);
    assert_eq!(encoded["tools"][0]["resultError"], error);
    let restored: TranscriptSnapshot = serde_json::from_value(encoded.clone()).unwrap();
    assert_eq!(restored.tools, s.tools);
    for t in encoded["tools"].as_array_mut().unwrap() {
        t.as_object_mut().unwrap().remove("resultMeta");
        t.as_object_mut().unwrap().remove("resultError");
    }
    let legacy: TranscriptSnapshot = serde_json::from_value(encoded).unwrap();
    assert!(legacy.tools[0].result_meta.is_none() && legacy.tools[0].result_error.is_none());
    assert_eq!(legacy.tools[0].id, s.tools[0].id);
    assert_eq!(legacy.tools[0].phase, "error");
}
#[test]
fn unknown_nonnull_error_and_metadata_shapes_remain_opaque_without_becoming_failure_signals() {
    for (meta, error) in [
        (json!([1, "future"]), json!(false)),
        (json!("future metadata"), json!(["future error"])),
        (json!(false), json!("not a structured error")),
    ] {
        let s = snapshot(json!({"meta":meta,"error":error}), None);
        assert_eq!(s.tools[0].phase, "done");
        assert_eq!(s.tools[0].result_meta, Some(meta));
        assert_eq!(s.tools[0].result_error, Some(error));
    }
    let s = snapshot(json!({"meta":null,"error":null}), None);
    assert!(s.tools[0].result_meta.is_none() && s.tools[0].result_error.is_none());
    let encoded = serde_json::to_value(&s.tools[0]).unwrap();
    assert!(encoded.get("resultMeta").is_none() && encoded.get("resultError").is_none());
}
