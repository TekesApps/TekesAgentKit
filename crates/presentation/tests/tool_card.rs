use serde_json::{Value, json};
use tekes_agent_presentation::*;
fn tool(name: &str, args: Value, meta: Value) -> TranscriptToolCall {
    serde_json::from_value(json!({"id":"c","turnId":"t","name":name,"arguments":args,"phase":"done","resultMeta":meta,"result":{"content":[{"type":"text","text":"owned output"}]}})).unwrap()
}
fn terminal(t: &TranscriptToolCall) -> TranscriptToolTerminal {
    match t.presentation_card().unwrap() {
        TranscriptToolCardModel::Terminal(card) => card,
        other => panic!("expected terminal, got {other:?}"),
    }
}
#[test]
fn terminal_running_validation_persistence_and_background_receipt_are_distinct() {
    let mut t = tool(
        "shell.run",
        json!({"command":"echo owned","description":"检查","workdir":"/owned"}),
        Value::Null,
    );
    t.phase = "doing".into();
    let c = terminal(&t);
    assert!(c.running);
    assert!(c.output.is_none());
    assert_eq!(c.workdir.as_deref(), Some("/owned"));
    t.arguments.as_object_mut().unwrap().remove("description");
    assert!(terminal(&t).workdir.is_none());
    t.phase = "done".into();
    assert!(t.presentation_card().is_none());
    t.arguments["description"] = json!("检查");
    t.arguments["run_in_background"] = json!(true);
    t.result = Some(json!({"content":[{"text":"job receipt\n[exit code: 9]"}]}));
    let c = terminal(&t);
    assert_eq!(c.output.as_deref(), Some("job receipt\n[exit code: 9]"));
    assert!(c.exit_code.is_none());
    assert!(!c.exit_failed());
    t.arguments["run_in_background"] = json!("true");
    assert!(t.presentation_card().is_none());
}
#[test]
fn terminal_exit_markers_are_anchored_and_keep_stdout_separate_from_signals() {
    for (text, code, signal, output) in [
        ("body\n[exit code: 2]", Some(2), None, "body"),
        ("body\n[exit code: 2]\n", Some(2), None, "body"),
        ("body\n[exit code: 2]\u{b}", Some(2), None, "body"),
        ("body\n[exit code: 2]\u{c}", Some(2), None, "body"),
        (
            "body\n[killed by signal: SIGTERM]\u{c}",
            None,
            Some("SIGTERM"),
            "body",
        ),
        (
            "body\n[killed by signal: SIGTERM]",
            None,
            Some("SIGTERM"),
            "body",
        ),
        (
            "body\n[exit code: -2]",
            Some(0),
            None,
            "body\n[exit code: -2]",
        ),
        (
            "body\n[exit code: 2]\nnext",
            Some(0),
            None,
            "body\n[exit code: 2]\nnext",
        ),
        ("[exit code: 2]", Some(0), None, "[exit code: 2]"),
        ("body\n[exit code: ٢]", Some(0), None, "body"),
    ] {
        let mut t = tool(
            "bash",
            json!({"command":"owned","description":"检查"}),
            Value::Null,
        );
        t.result = Some(json!({"content":[{"text":text}]}));
        let c = terminal(&t);
        assert_eq!(c.exit_code, code, "{text}");
        assert_eq!(c.signal.as_deref(), signal);
        assert_eq!(c.output.as_deref(), Some(output));
    }
}
#[test]
fn structured_terminal_output_has_priority_and_explicit_errors_keep_generic_disclosure() {
    let mut t = tool(
        "pwsh",
        json!({"script":"owned","description":"检查"}),
        Value::Null,
    );
    for (status, expected) in [
        (json!(2), 2),
        (json!(1.8), 1),
        (json!(true), 1),
        (json!("2"), 0),
    ] {
        let object = json!({"output":"original output","exitCode":status});
        t.result = Some(json!({"content":[{"text":object.to_string()}]}));
        let c = terminal(&t);
        assert_eq!(c.exit_code, Some(expected));
        assert_eq!(c.output.as_deref(), Some("original output"));
    }
    t.result = Some(
        json!({"content":[{"text":json!({"output":"out","exitCode":2,"signal":"SIGKILL"}).to_string()}]}),
    );
    assert_eq!(terminal(&t).exit_code, None);
    t.result_error = Some(json!({"name":"OwnedError","code":"owned"}));
    assert!(t.presentation_card().is_none());
    t.result = Some(json!({"content":[]}));
    assert_eq!(
        t.presentation_result_text().as_deref(),
        Some("OwnedError: owned")
    );
}
#[test]
fn escalation_and_optional_fields_do_not_coerce_unknown_shapes() {
    let mut t = tool(
        "shell",
        json!({"command":"owned","description":"检查"}),
        Value::Null,
    );
    for (key, value) in [
        ("description", Value::Null),
        ("workdir", json!(2)),
        ("run_in_background", Value::Null),
        ("sandbox_permissions", json!("require_escalated")),
    ] {
        let before = t.arguments.clone();
        t.arguments[key] = value;
        assert!(t.presentation_card().is_none(), "{key}");
        t.arguments = before;
    }
    t.arguments["sandbox_permissions"] = json!("workspace-write");
    t.arguments["justification"] = json!("explicit reason");
    assert!(t.presentation_card().is_some());
    t.arguments["justification"] = json!("  ");
    assert!(t.presentation_card().is_none());
}
#[test]
fn search_preserves_file_order_line_numbers_paths_and_truncation() {
    let mut t = tool(
        "grep",
        json!({"pattern":"owned","include":"*.{rs,toml}"}),
        json!({"shape":"matches","truncated":true,"total":8,"files":[{"path":"z.rs","matches":[{"lineNumber":2,"line":"owned z"}]},{"path":"a.rs","matches":[]}]}),
    );
    let Some(TranscriptToolCardModel::Search {
        result: TranscriptToolSearch::Matches(files),
        truncated,
        total,
    }) = t.presentation_card()
    else {
        panic!()
    };
    assert!(truncated);
    assert_eq!(total, 8);
    assert_eq!(files[0].path, "z.rs");
    assert_eq!(files[0].matches[0].line_number, 2);
    for include in ["!*.rs", "a,b", "\u{a0}"] {
        t.arguments["include"] = json!(include);
        assert!(t.presentation_card().is_none());
    }
    t = tool(
        "glob",
        json!({"pattern":"*.rs"}),
        json!({"shape":"paths","truncated":false,"total":2,"paths":["z.rs","a.rs"]}),
    );
    let Some(TranscriptToolCardModel::Search {
        result: TranscriptToolSearch::Paths(paths),
        ..
    }) = t.presentation_card()
    else {
        panic!()
    };
    assert_eq!(paths, ["z.rs", "a.rs"]);
    t.result_meta.as_mut().unwrap()["total"] = json!(true);
    assert!(t.presentation_card().is_none());
}
#[test]
fn web_requires_typed_sources_queries_and_status_without_inventing_missing_values() {
    let mut t = tool(
        "web_search",
        json!({"queries":["owned query"]}),
        json!({"truncated":false,"sources":[{"url":"https://example.com","title":"Owned","snippet":"source"}],"answer":"answer"}),
    );
    let Some(TranscriptToolCardModel::Web {
        result: TranscriptToolWeb::Search { sources, answer },
        ..
    }) = t.presentation_card()
    else {
        panic!()
    };
    assert_eq!(sources[0].title.as_deref(), Some("Owned"));
    assert_eq!(answer.as_deref(), Some("answer"));
    t.result_meta.as_mut().unwrap()["sources"][0]["title"] = Value::Null;
    assert!(t.presentation_card().is_none());
    t = tool(
        "web_fetch",
        json!({"url":"https://requested.example"}),
        json!({"truncated":true,"url":"https://resolved.example","statusCode":404}),
    );
    let Some(TranscriptToolCardModel::Web {
        result: TranscriptToolWeb::Fetch { url, status_code },
        ..
    }) = t.presentation_card()
    else {
        panic!()
    };
    assert_eq!(url, "https://resolved.example");
    assert_eq!(status_code, 404);
    t.result_meta.as_mut().unwrap()["statusCode"] = json!(404.5);
    assert!(t.presentation_card().is_none());
}
#[test]
fn read_respects_source_line_numbers_bounds_and_optional_language() {
    let mut t = tool(
        "read",
        json!({"file_path":"a.rs","offset":2,"limit":4}),
        json!({"path":"/resolved/a.rs","offset":2,"totalLines":8,"lines":[{"number":2,"text":"a"},{"number":4,"text":"b"}],"lang":"rust"}),
    );
    let Some(TranscriptToolCardModel::Read(card)) = t.presentation_card() else {
        panic!()
    };
    assert_eq!(card.lines[1].number, 4);
    assert_eq!(card.path, "/resolved/a.rs");
    for bad in [1, 2, 9] {
        t.result_meta.as_mut().unwrap()["lines"][1]["number"] = json!(bad);
        assert!(t.presentation_card().is_none());
    }
    t.result_meta.as_mut().unwrap()["lines"][1]["number"] = json!(4);
    t.arguments["limit"] = json!(1.5);
    assert!(t.presentation_card().is_none());
}
#[test]
fn diff_distinguishes_intent_authoritative_result_and_generic_failure() {
    let mut t = tool(
        "edit",
        json!({"file_path":"a.rs","old_string":"old","new_string":"new"}),
        Value::Null,
    );
    assert!(t.presentation_card().is_none());
    t.phase = "doing".into();
    let Some(TranscriptToolCardModel::Diff { hunks }) = t.presentation_card() else {
        panic!()
    };
    assert_eq!(hunks[0].old_text.as_deref(), Some("old"));
    t.phase = "done".into();
    t.result_meta =
        Some(json!({"diffs":[{"path":"resolved.rs","oldText":null,"newText":"actual"}]}));
    let Some(TranscriptToolCardModel::Diff { hunks }) = t.presentation_card() else {
        panic!()
    };
    assert_eq!(hunks[0].new_text, "actual");
    assert_eq!(hunks[0].path, "resolved.rs");
    t.result.as_mut().unwrap()["isError"] = json!(true);
    assert!(t.presentation_card().is_none());
    t = tool(
        "write",
        json!({"path":"new.rs","content":"owned"}),
        Value::Null,
    );
    assert!(t.presentation_card().is_some());
    t = tool(
        "str_replace_editor",
        json!({"command":"create","path":"new.rs"}),
        Value::Null,
    );
    assert!(t.presentation_card().is_none());
    t.phase = "doing".into();
    assert!(t.presentation_card().is_some());
}
#[test]
fn all_variants_round_trip_and_unknown_or_failed_payloads_leave_raw_facts_untouched() {
    for (name, args, meta) in [
        (
            "shell",
            json!({"command":"owned","description":"检查"}),
            Value::Null,
        ),
        (
            "glob",
            json!({"pattern":"*.rs"}),
            json!({"shape":"paths","paths":[],"total":0,"truncated":false}),
        ),
        (
            "grep",
            json!({"pattern":"owned"}),
            json!({"shape":"matches","files":[],"total":0,"truncated":false}),
        ),
        (
            "web_search",
            json!({"queries":["owned"]}),
            json!({"sources":[],"truncated":false}),
        ),
        (
            "web_fetch",
            json!({"url":"https://example.com"}),
            json!({"url":"https://example.com","statusCode":200,"truncated":false}),
        ),
        (
            "write",
            json!({"path":"a.rs","content":"owned"}),
            Value::Null,
        ),
    ] {
        let t = tool(name, args, meta);
        let original = t.clone();
        let card = t.presentation_card().unwrap();
        assert_eq!(
            serde_json::from_value::<TranscriptToolCardModel>(serde_json::to_value(&card).unwrap())
                .unwrap(),
            card
        );
        assert_eq!(t, original);
    }
    let mut t = tool(
        "read",
        json!("{\"path\":\"owned.rs\"}"),
        json!({"path":"owned.rs","offset":1,"totalLines":0,"lines":[]}),
    );
    let original = t.clone();
    let card = t.presentation_card().unwrap();
    assert_eq!(
        serde_json::from_value::<TranscriptToolCardModel>(serde_json::to_value(&card).unwrap())
            .unwrap(),
        card
    );
    assert_eq!(t, original);
    t.name = "unknown".into();
    assert!(t.presentation_card().is_none());
    t.name = "read".into();
    t.result_meta = Some(json!({"future":true}));
    assert!(t.presentation_card().is_none());
    assert_eq!(t.result_meta, Some(json!({"future":true})));
    t.arguments = json!("malformed");
    assert!(t.presentation_card().is_none());
}

#[test]
fn canonical_meta_failure_uses_generic_disclosure_and_stopped_is_not_a_failed_call() {
    let mut t = tool(
        "shell",
        json!({"command":"owned","description":"检查"}),
        json!({"exitCode":2}),
    );
    t.phase = "error".into();
    assert!(t.presentation_card().is_none());
    t.phase = "stopped".into();
    t.result_error = Some(json!({"name":"Interrupted","code":"interrupted"}));
    assert!(t.presentation_card().is_some());
    assert_eq!(
        t.presentation_row(TranscriptToolRowWording {
            label: "Shell",
            stopped: "已停止",
            ..Default::default()
        })
        .state,
        TranscriptToolRowState::Stopped
    );
}

#[test]
fn card_numbers_follow_foundation_conversion_without_changing_sep_strict_numbers() {
    let mut t = tool(
        "glob",
        json!({"pattern":"owned"}),
        json!({"shape":"paths","paths":[],"truncated":false,"total":1e20}),
    );
    let Some(TranscriptToolCardModel::Search { total, .. }) = t.presentation_card() else {
        panic!()
    };
    assert_eq!(total, i64::MAX);
    t.result_meta.as_mut().unwrap()["total"] = json!(u64::MAX);
    assert!(t.presentation_card().is_none()); // NSNumber integerValue == -1, rejected by total>=0.
    let mut t = tool(
        "shell",
        json!({"command":"owned","description":"检查"}),
        Value::Null,
    );
    for (value, expected) in [
        (json!(i64::MAX), i64::MAX),
        (json!(u64::MAX), -1),
        (json!(1e20), i64::MAX),
        (json!(-1e20), i64::MIN),
    ] {
        t.result = Some(
            json!({"content":[{"text":json!({"output":"out","exitCode":value}).to_string()}]}),
        );
        assert_eq!(terminal(&t).exit_code, Some(expected));
    }
}
