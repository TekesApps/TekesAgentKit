use serde_json::json;
use tekes_agent_presentation::*;

fn call() -> TranscriptToolCall {
    serde_json::from_value(json!({"id":"call","turnId":"turn","name":"read","phase":"doing","arguments":{"path":"src/a.rs"}})).unwrap()
}
fn words() -> TranscriptToolReportWording {
    TranscriptToolReportWording {
        subject_phrase: |verb, subject| format!("{subject}: {verb}"),
        verb: str::to_owned,
        magnitude: |n, object| format!("{n} {object}"),
        join: |a, b| format!("{a} · {b}"),
    }
}
fn report(added: Option<i64>, removed: Option<i64>) -> TranscriptToolReport {
    TranscriptToolReport {
        verb: "read".into(),
        subject: Some("owned.rs".into()),
        result_count: None,
        result_object: None,
        note: None,
        added_lines: added,
        removed_lines: removed,
    }
}
#[test]
fn utf16_summary_and_delta_spans_share_one_exact_source_string() {
    let header = call().presentation_header(
        TranscriptToolRowWording {
            label: "工具👩🏽‍💻",
            progress: Some("中文 e\u{301}"),
            ..Default::default()
        },
        &TranscriptToolRowInputs {
            report: None,
            workspace_change_count: Some(TranscriptToolChangeCount {
                additions: 12,
                deletions: 3,
            }),
        },
        words(),
    );
    assert_eq!(header.text, "工具👩🏽‍💻 · 中文 e\u{301}    +12 −3");
    assert_eq!(
        header.spans,
        vec![
            TranscriptToolHeaderSpan {
                start_utf16: 9,
                length_utf16: 8,
                role: TranscriptToolHeaderRole::Secondary
            },
            TranscriptToolHeaderSpan {
                start_utf16: 21,
                length_utf16: 3,
                role: TranscriptToolHeaderRole::DiffAdded
            },
            TranscriptToolHeaderSpan {
                start_utf16: 25,
                length_utf16: 2,
                role: TranscriptToolHeaderRole::DiffRemoved
            },
        ]
    );
    assert_eq!(
        (header.tool_id.as_str(), header.turn_id.as_str()),
        ("call", "turn")
    );
    let value = serde_json::to_value(&header).unwrap();
    assert_eq!(value["spans"][0]["startUtf16"], 9);
    assert_eq!(
        serde_json::from_value::<TranscriptToolHeader>(value).unwrap(),
        header
    );
}
#[test]
fn complete_zero_pair_wins_but_partial_report_uses_workspace_without_phase_gating() {
    let mut tool = call();
    tool.phase = "done".into();
    let mut input = TranscriptToolRowInputs {
        report: Some(report(Some(0), Some(0))),
        workspace_change_count: Some(TranscriptToolChangeCount {
            additions: 9,
            deletions: 10,
        }),
    };
    let header = tool.presentation_header(
        TranscriptToolRowWording {
            label: "Read",
            ..Default::default()
        },
        &input,
        words(),
    );
    assert_eq!(header.text, "owned.rs: read    +0 −0");
    assert_eq!(header.spans.len(), 2);
    assert_eq!(
        header.change_count,
        Some(TranscriptToolChangeCount {
            additions: 0,
            deletions: 0
        })
    );
    input.report = Some(report(Some(0), None));
    tool.phase = "error".into();
    tool.result = Some(json!({"text":"failure\nprivate detail"}));
    let header = tool.presentation_header(
        TranscriptToolRowWording {
            label: "Read",
            result: Some("failure\nprivate detail"),
            ..Default::default()
        },
        &input,
        words(),
    );
    assert_eq!(header.text, "Read · failure    +9 −10");
    assert_eq!(header.row.state, TranscriptToolRowState::Error);
    assert!(
        header
            .spans
            .iter()
            .all(|s| s.role != TranscriptToolHeaderRole::Secondary)
    );
    tool.phase = "stopped".into();
    let header = tool.presentation_header(
        TranscriptToolRowWording {
            label: "Read",
            stopped: "Stopped",
            ..Default::default()
        },
        &input,
        words(),
    );
    assert_eq!(header.text, "Read · Stopped    +9 −10");
    assert_eq!(input.report.as_ref().unwrap().removed_lines, None);
}
#[test]
fn absent_and_signed_counts_preserve_source_values_and_action_authority() {
    let mut tool = call();
    let inputs = TranscriptToolRowInputs {
        report: Some(report(Some(-1), Some(-2))),
        workspace_change_count: None,
    };
    let header = tool.presentation_header(
        TranscriptToolRowWording {
            label: "Read",
            ..Default::default()
        },
        &inputs,
        words(),
    );
    assert!(header.text.starts_with("Read · src/a.rs")); // Running report cannot settle the title.
    assert!(header.text.ends_with("    +-1 −-2"));
    tool.phase = "done".into();
    let header = tool.presentation_header(
        TranscriptToolRowWording {
            label: "Read",
            ..Default::default()
        },
        &TranscriptToolRowInputs::default(),
        words(),
    );
    assert_eq!(header.text, header.row.text());
    assert!(header.change_count.is_none());
    assert!(
        header
            .spans
            .iter()
            .all(|s| s.role == TranscriptToolHeaderRole::Secondary)
    );
}
