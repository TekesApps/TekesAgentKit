use serde_json::json;
use tekes_agent_presentation::*;
use unicode_segmentation::UnicodeSegmentation;

fn call(id: &str, name: &str, phase: &str) -> TranscriptToolCall {
    TranscriptToolCall {
        id: id.into(),
        turn_id: "turn".into(),
        name: name.into(),
        phase: phase.into(),
        arguments: json!({}),
        result: None,
        call_view: None,
        result_view: None,
        result_meta: None,
        result_error: None,
        source_position: None,
        result_position: None,
    }
}

#[test]
fn compact_previews_use_only_semantic_fields_and_preserve_full_source() {
    for (name, args, expected) in [
        (
            "ns.bash",
            json!({"command":"  cargo   test\nsecret"}),
            "工具 · cargo test",
        ),
        (
            "read",
            json!({"path":"/repo/src/中文.rs"}),
            "工具 · src/中文.rs",
        ),
        (
            "search",
            json!({"queries":[" 中文  查询\nsecret", "next"]}),
            "工具 · 中文 查询, next",
        ),
        (
            "fetch",
            json!({"url":"https://user:secret@example.org/a%20b/%E4%B8%AD?q=secret#secret"}),
            "工具 · example.org/a b/中",
        ),
        (
            "open",
            json!({"url":"relative/%E4%B8%AD?q=secret"}),
            "工具 · relative/中",
        ),
        (
            "unknown",
            json!({"apiKey":"secret", "command":"secret", "body":"secret"}),
            "已知标题",
        ),
        (
            "unknown",
            json!({"description":"显式摘要", "body":"secret"}),
            "工具 · 显式摘要",
        ),
    ] {
        let mut tool = call("one", name, "doing");
        tool.arguments = args;
        let before = tool.clone();
        assert_eq!(
            tool.continuous_summary("工具", Some("已知标题"), "后备"),
            expected
        );
        assert_eq!(tool, before);
    }
    let mut tool = call("one", "bash", "doing");
    tool.arguments = json!("{\"cmd\":\"echo parsed\"}");
    assert_eq!(
        tool.continuous_summary("Shell", None, "fallback"),
        "Shell · echo parsed"
    );
    tool.arguments = json!("malformed secret");
    assert_eq!(
        tool.continuous_summary("Shell", None, "fallback"),
        "fallback"
    );
}

#[test]
fn preview_bound_counts_extended_graphemes_including_cjk_and_zwj() {
    let tool = call("one", "unknown", "doing");
    for glyph in ["中", "e\u{301}", "👩🏽‍💻"] {
        for count in [95, 96, 97] {
            let title = glyph.repeat(count);
            let preview = tool.continuous_summary("tool", Some(&title), "fallback");
            assert_eq!(preview.graphemes(true).count(), count.min(96));
            assert_eq!(
                preview,
                if count > 96 {
                    glyph.repeat(95) + "…"
                } else {
                    title
                }
            );
        }
    }
}

#[test]
fn canonical_failure_precedence_and_first_success_name_order_are_shared() {
    let mut tools = vec![
        call("running", "search", "doing"),
        call("read1", "read", "done"),
        call("write", "write", "done"),
        call("read2", "read", "done"),
        call("failed", "bash", "done"),
        call("stopped", "bash", "stopped"),
    ];
    tools[4].result = Some(json!({"text":"body\n[exit code: 2]"}));
    let before = tools.clone();
    let summary = TranscriptToolRunSummary::read(
        &tools.iter().collect::<Vec<_>>(),
        |s| s.into(),
        |t| format!("active:{}", t.id),
    )
    .unwrap();
    assert_eq!(
        summary.tool_ids,
        tools.iter().map(|t| t.id.clone()).collect::<Vec<_>>()
    );
    assert_eq!(
        (
            summary.active_count,
            summary.failed_count,
            summary.stopped_count
        ),
        (1, 1, 1)
    );
    assert_eq!(summary.active_summaries, ["active:running"]);
    assert_eq!(
        summary.completed,
        vec![
            TranscriptToolNameCount {
                name: "read".into(),
                count: 2
            },
            TranscriptToolNameCount {
                name: "write".into(),
                count: 1
            }
        ]
    );
    assert_eq!(tools, before);
}

#[test]
fn invalid_membership_is_rejected_instead_of_reordered_or_deduplicated() {
    assert!(TranscriptToolRunSummary::read(&[], str::to_owned, |_| String::new()).is_err());
    let one = call("one", "read", "doing");
    let mut other = one.clone();
    assert!(
        TranscriptToolRunSummary::read(&[&one, &other], str::to_owned, |_| String::new()).is_err()
    );
    other.id = "two".into();
    other.turn_id = "other".into();
    assert!(
        TranscriptToolRunSummary::read(&[&one, &other], str::to_owned, |_| String::new()).is_err()
    );
    other.turn_id = "turn".into();
    other.id.clear();
    assert!(
        TranscriptToolRunSummary::read(&[&one, &other], str::to_owned, |_| String::new()).is_err()
    );
}
