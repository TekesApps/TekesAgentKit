use serde_json::json;
use tekes_agent_presentation::*;
fn tool() -> TranscriptToolCall {
    serde_json::from_value(json!({"id":"c","turnId":"t","name":"shell","arguments":{"command":"echo owned\nsecond line"},"phase":"doing"})).unwrap()
}
fn wording() -> TranscriptToolRowWording<'static> {
    TranscriptToolRowWording {
        label: "Shell",
        stopped: "已停止",
        ..Default::default()
    }
}
#[test]
fn running_progress_detail_hint_arguments_and_stopped_follow_source_precedence() {
    let mut t = tool();
    let mut w = wording();
    assert_eq!(t.presentation_row(w).text(), "Shell · echo owned");
    w.title_hint = Some("  shell  ");
    assert_eq!(t.presentation_row(w).summary.as_deref(), Some("echo owned"));
    w.title_hint = Some("action\nnot inline");
    assert_eq!(t.presentation_row(w).summary.as_deref(), Some("action"));
    w.detail = Some("detail");
    w.progress = Some("  active  progress\nignored");
    assert_eq!(
        t.presentation_row(w).summary.as_deref(),
        Some("active progress")
    );
    t.phase = "done".into();
    assert_eq!(t.presentation_row(w).summary.as_deref(), Some("detail"));
    t.phase = "stopped".into();
    assert_eq!(t.presentation_row(w).text(), "Shell · 已停止");
    assert_eq!(t.presentation_row(w).state, TranscriptToolRowState::Stopped);
}
#[test]
fn successful_report_is_authoritative_and_stdout_never_replaces_the_action() {
    let mut t = tool();
    t.phase = "done".into();
    let mut w = wording();
    w.result = Some("arbitrary stdout");
    assert_eq!(t.presentation_row(w).text(), "Shell · echo owned");
    w.settled_action = Some("Read owned.rs · 12 lines");
    assert_eq!(t.presentation_row(w).text(), "Read owned.rs · 12 lines");
    t.phase = "doing".into();
    assert_eq!(t.presentation_row(w).text(), "Shell · echo owned");
}
#[test]
fn canonical_failures_use_result_including_public_meta_exit_failures() {
    let mut t = tool();
    t.phase = "error".into();
    t.result = Some(json!({"isError":false,"content":[{"text":"output"}]}));
    t.result_meta = Some(json!({"exitCode":2}));
    let mut w = wording();
    w.result = Some("failure line\nfull error");
    let row = t.presentation_row(w);
    assert_eq!(row.state, TranscriptToolRowState::Error);
    assert_eq!(row.text(), "Shell · failure line");
    for marker in 0..3 {
        t.result_error = None;
        t.result = Some(json!({"isError":false,"content":[]}));
        if marker == 0 {
            t.result_error = Some(json!({}));
        }
        if marker == 1 {
            t.result.as_mut().unwrap()["isError"] = json!(true);
        }
        if marker == 2 {
            t.result.as_mut().unwrap()["content"] = json!([{"isError":true}]);
        }
        assert_eq!(t.presentation_row(w).text(), "Shell · failure line");
    }
    t.phase = "stopped".into();
    assert_eq!(t.presentation_row(w).visible_summary(), Some("已停止"));
}
#[test]
fn semantic_arguments_encoded_json_and_stable_fallback_match_replay() {
    let mut t = tool();
    t.name = "web_search.remote".into();
    t.arguments = json!({"query":"fallback","queries":[" first\nsecond","  ","other"]});
    assert_eq!(
        t.presentation_row(wording()).summary.as_deref(),
        Some("first, other")
    );
    t.name = "read".into();
    t.arguments = json!("{\"file_path\":\"a.rs\",\"z\":\"last\"}");
    assert_eq!(
        t.presentation_row(wording()).summary.as_deref(),
        Some("a.rs")
    );
    t.name = "unknown".into();
    t.arguments = json!({"z":"last","a":"first"});
    assert_eq!(
        t.presentation_row(wording()).summary.as_deref(),
        Some("first")
    );
    let original = t.clone();
    let row = t.presentation_row(wording());
    assert_eq!(
        serde_json::from_value::<TranscriptToolRowModel>(serde_json::to_value(&row).unwrap())
            .unwrap(),
        row
    );
    assert_eq!(t, original);
}
#[test]
fn legacy_shell_exit_marker_is_failed_without_promoting_stdout_to_error_summary() {
    let mut t = tool();
    t.phase = "done".into();
    t.arguments = json!({"command":"test -f owned.rs","description":"检查文件"});
    t.result = Some(json!({"content":[{"type":"text","text":"ordinary output\n[exit code: 2]"}]}));
    let w = TranscriptToolRowWording {
        label: "Shell",
        title_hint: Some("检查文件"),
        result: Some("ordinary output\n[exit code: 2]"),
        ..Default::default()
    };
    let row = t.presentation_row(w);
    assert_eq!(row.state, TranscriptToolRowState::Error);
    assert_eq!(row.text(), "Shell · 检查文件");
    assert!(row.error_summary.is_none());
}

#[test]
fn legacy_exit_failure_does_not_require_valid_arguments_or_a_persistent_shell_card() {
    let mut t = tool();
    t.phase = "done".into();
    t.arguments = json!({"command":"owned"});
    t.result = Some(json!({"content":[{"text":"out\n[exit code: 2]"}]}));
    assert!(t.presentation_card().is_none());
    assert_eq!(
        t.presentation_row(wording()).state,
        TranscriptToolRowState::Error
    );
    t.arguments = json!("malformed");
    assert_eq!(
        t.presentation_row(wording()).state,
        TranscriptToolRowState::Error
    );
}
