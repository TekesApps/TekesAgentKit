use serde_json::json;
use tekes_agent_presentation::*;

fn tool() -> TranscriptToolCall {
    serde_json::from_value(json!({"id":"call","turnId":"turn","name":"read","arguments":"  original input\n\n","phase":"doing"})).unwrap()
}
#[test]
fn live_and_settled_output_keep_input_and_stable_section_kinds() {
    let mut call = tool();
    let before = call.clone();
    let live = call.presentation_details(Default::default());
    assert!(live.running);
    assert_eq!(live.sections.len(), 2);
    assert_eq!(live.sections[0].kind, TranscriptToolDetailKind::Input);
    assert_eq!(live.sections[0].copy_text, "  original input\n\n");
    assert_eq!(live.sections[0].text, "  original input\n");
    assert_eq!(live.sections[1].kind, TranscriptToolDetailKind::Output);
    assert_eq!(live.sections[1].text, "");
    assert_eq!(call, before);
    call.phase = "done".into();
    let done = call.presentation_details(TranscriptToolDetailWording {
        fallback_output: "Completed with no output",
        ..Default::default()
    });
    assert!(!done.running);
    assert_eq!(done.sections[0], live.sections[0]);
    assert_eq!(done.sections[1].text, "Completed with no output");
}
#[test]
fn output_accumulates_exact_nonblank_values_in_result_progress_detail_order() {
    let mut call = tool();
    call.result = Some(json!({"content":[{"type":"text","text":"结果\n"}]}));
    let wording = TranscriptToolDetailWording {
        progress: Some("结果\n"),
        progress_detail: Some("  后续进度\n"),
        ..Default::default()
    };
    for phase in ["doing", "done", "interrupted"] {
        call.phase = phase.into();
        let details = call.presentation_details(wording);
        assert_eq!(details.sections[1].copy_text, "结果\n\n  后续进度\n");
        assert_eq!(details.sections[1].text, "结果\n\n  后续进度");
    }
    call.result = None;
    let details = call.presentation_details(TranscriptToolDetailWording {
        progress: Some(" \n"),
        progress_detail: Some("\t"),
        fallback_output: "fallback",
        ..Default::default()
    });
    assert_eq!(details.sections[1].text, "fallback");
}
#[test]
fn empty_input_copy_is_localizable_without_reformatting_nonempty_arguments() {
    let mut call = tool();
    call.arguments = json!(" \n");
    for (phase, expected) in [("will", "接收"), ("doing", "缺少")] {
        call.phase = phase.into();
        assert_eq!(
            call.presentation_details(TranscriptToolDetailWording {
                receiving_input: "接收",
                input_unavailable: "缺少",
                ..Default::default()
            })
            .sections[0]
                .text,
            expected
        );
    }
    let original = " { \"z\": 1, \"a\": 2 } \n";
    call.arguments = json!(original);
    assert_eq!(
        call.presentation_details(Default::default()).sections[0].copy_text,
        original
    );
}
#[test]
fn absent_views_are_omitted_and_present_views_keep_json_and_order() {
    let mut call = tool();
    call.call_view = Some(json!({"future":{"nested":true}}));
    call.result_view = Some(json!({"for":"result","view":{"title":"原回传"}}));
    let details = call.presentation_details(Default::default());
    assert_eq!(
        details.sections.iter().map(|s| s.kind).collect::<Vec<_>>(),
        vec![
            TranscriptToolDetailKind::Input,
            TranscriptToolDetailKind::Output,
            TranscriptToolDetailKind::CallView,
            TranscriptToolDetailKind::ResultView,
        ]
    );
    for (section, source) in details.sections[2..]
        .iter()
        .zip([&call.call_view, &call.result_view])
    {
        assert_eq!(section.language.as_deref(), Some("json"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&section.copy_text).unwrap(),
            *source.as_ref().unwrap()
        );
    }
}
#[test]
fn canonical_failure_and_legacy_exit_follow_the_actual_view_bridge() {
    let mut call = tool();
    call.name = "shell".into();
    call.phase = "done".into();
    call.result = Some(json!({"content":[{"text":"body\n[exit code: 2]"}]}));
    assert_eq!(
        call.presentation_details(Default::default()).sections[1].title,
        "Output"
    );
    call.phase = "error".into();
    let details = call.presentation_details(Default::default());
    assert!(!details.running);
    assert_eq!(details.sections[1].title, "Output · Failed");
    call.phase = "doing".into();
    assert!(call.presentation_details(Default::default()).running);
    call.result_error = Some(json!({"name":"Failure","code":"owned"}));
    assert!(!call.presentation_details(Default::default()).running);
    for phase in ["interrupted", "stopped"] {
        call.phase = phase.into();
        let details = call.presentation_details(Default::default());
        assert!(!details.running);
        assert_eq!(details.sections[1].title, "Output");
    }
}

#[test]
fn delivered_empty_output_is_distinct_from_an_absent_result() {
    let mut call = tool();
    call.phase = "done".into();
    let wording = TranscriptToolDetailWording {
        fallback_output: "display title",
        ..Default::default()
    };
    assert_eq!(
        call.presentation_details(wording).sections[1].text,
        "display title"
    );
    call.result = Some(json!({"content":[]}));
    assert_eq!(call.presentation_details(wording).sections[1].copy_text, "");
}

#[test]
fn structured_error_without_a_message_keeps_its_source_summary() {
    let mut call = tool();
    call.phase = "error".into();
    call.result = None;
    call.result_error = Some(json!({"name":"OwnedFailure","code":"owned"}));
    assert_eq!(
        call.presentation_result_text().as_deref(),
        Some("OwnedFailure: owned")
    );
    let details = call.presentation_details(Default::default());
    assert_eq!(details.sections[1].copy_text, "OwnedFailure: owned");
    assert_eq!(details.sections[1].title, "Output · Failed");
}
