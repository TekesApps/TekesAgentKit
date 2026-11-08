use serde_json::{Value, json};
use tekes_agent_contract::NativeConversationSnapshot;
use tekes_agent_presentation::thread_activity;
use tekes_agent_session::{ConnectionState, MemoryProjectionStore, SessionProjection};

fn user(id: &str) -> Value {
    json!({"id":id,"type":"message","message":{"role":"user","content":[{"type":"text","text":"input"}]}})
}
fn assistant(id: &str, reason: &str) -> Value {
    json!({"id":id,"type":"message","message":{"role":"assistant","stopReason":reason,"errorMessage":"not itself proof","content":[]}})
}
fn marker(id: &str, run: &str, phase: &str, aborted: bool) -> Value {
    json!({"id":id,"type":"custom","customType":"agentkit.run.v1","data":{"id":run,"phase":phase,"aborted":aborted}})
}
fn failed_run() -> Vec<Value> {
    vec![
        marker("start", "run", "started", false),
        user("user"),
        assistant("error", "error"),
        marker("end", "run", "settled", false),
    ]
}
fn store(entries: Vec<Value>) -> MemoryProjectionStore {
    let mut store = MemoryProjectionStore::new("pi");
    store.connection = ConnectionState::Ready;
    store.inventory_ready = true;
    store.control_ready = true;
    store.actionables_ready = true;
    store.sessions.insert(
        "child".into(),
        SessionProjection {
            summary: Some(json!({"sessionId":"child","blank":true,"running":false})),
            journal_ready: true,
            native_conversation: Some(NativeConversationSnapshot {
                session_id: "child".into(),
                generation: "g".into(),
                leaf_id: None,
                entries,
                has_more_before: false,
                is_streaming: false,
                state: json!({}),
            }),
            ..Default::default()
        },
    );
    store
}

#[test]
fn explicit_native_failure_overrides_blank_without_rewriting_the_inventory_or_history() {
    let s = store(failed_run());
    let before = serde_json::to_value(&s.sessions["child"]).unwrap();
    let activity = thread_activity(&s, "child");
    assert_eq!(activity.status, "failed");
    assert!(!activity.is_stale);
    assert_eq!(serde_json::to_value(&s.sessions["child"]).unwrap(), before);
}

#[test]
fn a_new_user_or_started_marker_cannot_resurrect_old_native_or_journal_failure() {
    for next in [
        user("new-user"),
        marker("new-start", "next", "started", false),
    ] {
        let mut entries = failed_run();
        entries.push(next);
        let mut s = store(entries);
        let entry = serde_json::from_value(
            json!({"event":{"seq":1,"time":1,"type":"turn/end","data":{"reason":"error"}}}),
        )
        .unwrap();
        s.sessions
            .get_mut("child")
            .unwrap()
            .entries
            .insert(1, entry);
        assert_eq!(thread_activity(&s, "child").status, "notStarted");
    }
}

#[test]
fn clientkit_explicit_run_accumulates_failure_and_gives_cancellation_precedence() {
    let mut entries = failed_run();
    entries.insert(3, assistant("later-stop", "stop"));
    assert_eq!(
        thread_activity(&store(entries.clone()), "child").status,
        "failed"
    );
    entries.insert(4, assistant("cancelled", "aborted"));
    assert_eq!(
        thread_activity(&store(entries), "child").status,
        "cancelled"
    );
    let entries = vec![
        marker("start", "run", "started", false),
        marker("end", "run", "settled", true),
    ];
    assert_eq!(
        thread_activity(&store(entries), "child").status,
        "cancelled"
    );
}

#[test]
fn legacy_native_failures_are_bounded_by_the_latest_authored_user() {
    let mut entries = vec![user("u"), assistant("a", "error")];
    assert_eq!(
        thread_activity(&store(entries.clone()), "child").status,
        "failed"
    );
    entries.push(user("new"));
    assert_eq!(
        thread_activity(&store(entries), "child").status,
        "notStarted"
    );
    let mut s = store(vec![assistant("clipped", "error")]);
    s.sessions
        .get_mut("child")
        .unwrap()
        .native_conversation
        .as_mut()
        .unwrap()
        .has_more_before = true;
    assert_eq!(thread_activity(&s, "child").status, "notStarted");
}

#[test]
fn waiting_running_and_queued_inputs_keep_their_original_precedence() {
    let mut s = store(failed_run());
    s.sessions.get_mut("child").unwrap().run_open = true;
    assert_eq!(thread_activity(&s, "child").status, "running");
    s.actionables
        .insert("question".into(), json!({"sessionId":"child"}));
    assert_eq!(thread_activity(&s, "child").status, "waitingForInput");
    s.actionables.clear();
    s.sessions.get_mut("child").unwrap().run_open = false;
    s.sessions
        .get_mut("child")
        .unwrap()
        .queue
        .push(json!({"text":"queued"}));
    assert_eq!(thread_activity(&s, "child").status, "queued");
    s.sessions.get_mut("child").unwrap().queue.clear();
    s.sessions
        .get_mut("child")
        .unwrap()
        .native_conversation
        .as_mut()
        .unwrap()
        .is_streaming = true;
    assert_eq!(thread_activity(&s, "child").status, "notStarted");
}

#[test]
fn unreadable_native_frames_do_not_supply_terminal_evidence_and_explicit_status_stays_authoritative()
 {
    for bad in 0..4 {
        let mut s = store(failed_run());
        let child = s.sessions.get_mut("child").unwrap();
        match bad {
            0 => child.journal_ready = false,
            1 => child.journal_error = Some("read failed".into()),
            2 => child.native_conversation.as_mut().unwrap().session_id = "other".into(),
            _ => child
                .native_conversation
                .as_mut()
                .unwrap()
                .generation
                .clear(),
        }
        assert_eq!(thread_activity(&s, "child").status, "notStarted");
    }
    let mut s = store(failed_run());
    s.sessions
        .get_mut("child")
        .unwrap()
        .summary
        .as_mut()
        .unwrap()["status"] = json!("completed");
    assert_eq!(thread_activity(&s, "child").status, "completed");
}

#[test]
fn missing_mismatched_or_future_run_markers_do_not_cross_an_unproven_boundary() {
    for entries in [
        vec![
            user("u"),
            assistant("e", "error"),
            marker("end", "missing", "settled", false),
        ],
        vec![
            marker("start", "first", "started", false),
            assistant("e", "error"),
            marker("end", "other", "settled", false),
        ],
        vec![
            marker("start", "run", "started", false),
            assistant("e", "error"),
        ],
        vec![
            marker("start", "run", "started", false),
            assistant("e", "error"),
            marker("future", "run", "future", false),
        ],
    ] {
        assert_eq!(
            thread_activity(&store(entries), "child").status,
            "notStarted"
        );
    }
}

#[test]
fn nonfailure_stop_reasons_and_error_text_do_not_manufacture_terminal_success_or_failure() {
    for reason in ["stop", "toolUse", "", "future"] {
        let s = store(vec![user("u"), assistant("a", reason)]);
        assert_eq!(
            thread_activity(&s, "child").status,
            "notStarted",
            "{reason}"
        );
    }
}
