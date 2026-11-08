use serde_json::json;
use tekes_agent_presentation::*;

fn report() -> TranscriptToolReport {
    serde_json::from_value(json!({"verb":"read"})).unwrap()
}
fn wording() -> TranscriptToolReportWording {
    TranscriptToolReportWording {
        subject_phrase: |verb, subject| format!("{subject}：{verb}"),
        verb: str::to_owned,
        magnitude: |count, object| format!("{count} {object}"),
        join: |left, right| format!("{left} · {right}"),
    }
}
#[test]
fn a_bare_verb_or_note_cannot_replace_the_calls_action() {
    let mut value = report();
    assert!(!value.names_the_call());
    value.note = Some("a provider note".into());
    value.result_count = Some(0);
    value.result_object = Some(String::new());
    value.subject = Some(String::new());
    assert!(value.row_text(wording()).is_none());
    value.result_object = Some("file".into());
    assert_eq!(
        value.row_text(wording()).as_deref(),
        Some("read · 0 file · a provider note")
    );
}
#[test]
fn complete_phrase_callbacks_keep_word_order_and_verbatim_report_parts() {
    let mut value = report();
    value.verb = "future_extension".into();
    value.subject = Some("文件👩🏽‍💻 e\u{301}".into());
    value.result_count = Some(120);
    value.result_object = Some("future_noun".into());
    value.note = Some("原始备注\n第二行".into());
    assert_eq!(
        value.row_text(wording()).as_deref(),
        Some("文件👩🏽‍💻 e\u{301}：future_extension · 120 future_noun · 原始备注\n第二行")
    );
    value.subject = Some(" ".into());
    assert!(value.names_the_call()); // Source uses isEmpty, not trimmed emptiness.
}
#[test]
fn partial_deltas_fall_back_but_a_complete_zero_pair_is_authoritative() {
    let workspace = TranscriptToolChangeCount {
        additions: 20,
        deletions: 8,
    };
    let mut value = report();
    value.added_lines = Some(0);
    assert!(!value.names_the_call());
    assert_eq!(
        TranscriptToolChangeCount::select(Some(&value), Some(workspace)),
        Some(workspace)
    );
    value.removed_lines = Some(0);
    assert!(value.names_the_call());
    assert_eq!(value.row_text(wording()).as_deref(), Some("read"));
    assert_eq!(
        TranscriptToolChangeCount::select(Some(&value), Some(workspace)),
        Some(TranscriptToolChangeCount {
            additions: 0,
            deletions: 0
        })
    );
    value.added_lines = Some(-3);
    assert_eq!(value.change_count().unwrap().additions, -3);
    assert_eq!(TranscriptToolChangeCount::select(None, None), None);
}
#[test]
fn report_wording_uses_the_existing_row_success_and_failure_precedence() {
    let mut value = report();
    value.subject = Some("owned.rs".into());
    let action = value.row_text(wording());
    let mut tool: TranscriptToolCall = serde_json::from_value(json!({"id":"call","turnId":"turn","name":"read","arguments":{"path":"source.rs"},"phase":"doing","result":{"text":"private output"}})).unwrap();
    for (phase, expected) in [
        ("doing", "Read · source.rs"),
        ("done", "owned.rs：read"),
        ("error", "Read · explicit failure"),
        ("stopped", "Read · stopped"),
    ] {
        tool.phase = phase.into();
        let before = tool.clone();
        let row = tool.presentation_row(TranscriptToolRowWording {
            label: "Read",
            stopped: "stopped",
            result: Some("explicit failure"),
            settled_action: action.as_deref(),
            ..Default::default()
        });
        assert_eq!(row.text(), expected);
        assert_eq!(tool, before);
    }
    tool.name = "shell".into();
    tool.phase = "done".into();
    tool.result = Some(json!({"text":"body\n[exit code: 2]"}));
    let row = tool.presentation_row(TranscriptToolRowWording {
        label: "Shell",
        settled_action: action.as_deref(),
        ..Default::default()
    });
    assert_eq!(row.state, TranscriptToolRowState::Error);
    assert_ne!(row.title, "owned.rs：read");
    // Delta selection is independent of row success: Source renders it even on an error.
    value.added_lines = Some(1);
    value.removed_lines = Some(2);
    assert_eq!(
        TranscriptToolChangeCount::select(Some(&value), None),
        Some(TranscriptToolChangeCount {
            additions: 1,
            deletions: 2
        })
    );
}
#[test]
fn report_json_keeps_open_keys_and_rejects_coerced_count_types() {
    let value: TranscriptToolReport=serde_json::from_value(json!({"verb":"future_verb","subject":"a","futureField":true,"addedLines":1,"removedLines":2})).unwrap();
    let encoded = serde_json::to_value(&value).unwrap();
    assert_eq!(
        encoded,
        json!({"verb":"future_verb","subject":"a","addedLines":1,"removedLines":2})
    );
    assert_eq!(
        serde_json::from_value::<TranscriptToolReport>(encoded).unwrap(),
        value
    );
    for count in [json!(true), json!("3"), json!(1.5)] {
        assert!(
            serde_json::from_value::<TranscriptToolReport>(
                json!({"verb":"read","resultCount":count})
            )
            .is_err()
        );
    }
}
