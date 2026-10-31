//! Independent product consumes the same original tool disclosure and title facts.
use tekes_agent_components_ui::*;
use tekes_ui_controls::composition::*;

#[test]
fn independent_host_reports_use_shared_precedence_and_keep_original_section_owners() {
    // Explicit host facts. These are not extracted from a tool's private metadata.
    fn report(scope: &RunScope, tool: &TranscriptToolCall) -> Option<TranscriptToolReport> {
        (scope == &RunScope::main("public-consumer", "tool-report")
            && tool.id == "reported"
            && tool.turn_id == "turn")
            .then(|| TranscriptToolReport {
                verb: "edit".into(),
                subject: Some("owned.rs".into()),
                result_count: Some(2),
                result_object: Some("file".into()),
                note: Some("已确认".into()),
                added_lines: Some(0),
                removed_lines: Some(0),
            })
    }
    fn header(scope: &RunScope, tool: &TranscriptToolCall) -> (String, String) {
        let report = report(scope, tool);
        let action = report.as_ref().and_then(|report| {
            report.row_text(TranscriptToolReportWording {
                subject_phrase: |_, subject| format!("修改 {subject}"),
                verb: str::to_owned,
                magnitude: |count, object| {
                    if object == "file" {
                        format!("{count} 个文件")
                    } else {
                        format!("{count} {object}")
                    }
                },
                join: |left, right| format!("{left} · {right}"),
            })
        });
        let result = tool.presentation_result_text();
        let row = tool.presentation_row(TranscriptToolRowWording {
            label: "Edit",
            stopped: "Stopped",
            result: result.as_deref(),
            settled_action: action.as_deref(),
            ..Default::default()
        });
        let count = TranscriptToolChangeCount::select(
            report.as_ref(),
            (scope == &RunScope::main("public-consumer", "tool-report")).then_some(
                TranscriptToolChangeCount {
                    additions: 20,
                    deletions: 9,
                },
            ),
        );
        // Plain label text only; SDK semantic runs/colors remain #25's responsibility.
        (
            count.map_or_else(
                || row.text(),
                |count| {
                    format!(
                        "{}    +{} −{}",
                        row.text(),
                        count.additions,
                        count.deletions
                    )
                },
            ),
            String::new(),
        )
    }
    fn sections(
        scope: &RunScope,
        tool: &TranscriptToolCall,
        _: OutputMessageLabels,
    ) -> TranscriptToolDetails {
        let mut facts = tool.presentation_details(Default::default());
        facts.sections[1].title = format!("{} · {}", scope.endpoint_id, facts.sections[1].title);
        facts
    }
    let scope = RunScope::main("public-consumer", "tool-report");
    let mut tool = TranscriptToolCall {
        id: "reported".into(), turn_id: "turn".into(), name: "edit".into(), phase: "doing".into(),
        arguments: r#"{"path":"owned.rs"}"#.parse().unwrap(),
        result_meta: Some(r#"{"didDo":{"verb":"forged","subject":"not authoritative","addedLines":999,"removedLines":999}}"#.parse().unwrap()),
        result: None, result_error: None, call_view: None, result_view: None, source_position: None, result_position: None,
    };
    let mut labels = ToolActivityLabels::default();
    labels.details.scoped_tool = Some(header);
    labels.details.tool_sections = Some(sections);
    let mut activity = ToolActivity::new(735, scope.clone(), labels.clone());
    activity
        .update(&scope, 1, 1, &tool, ToolActivityPlacement::Call)
        .unwrap();
    let root = activity.id();
    let ids = activity.content_ids().unwrap();
    let fields = activity.tool_detail_sections();
    let mut host = activity.into_host().unwrap();
    let title = |host: &ContentHost| {
        host.get(ids.disclosure.unwrap())
            .unwrap()
            .component::<Button>()
            .unwrap()
            .label
            .clone()
    };
    assert!(title(&host).contains("Edit · owned.rs    +0 −0"));
    assert!(!title(&host).contains("修改") && !title(&host).contains("999"));
    host.accessibility_action(ids.disclosure.unwrap(), None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    host.get_mut(root)
        .unwrap()
        .component_mut::<ToolActivity>()
        .unwrap()
        .handle_actions(&actions);
    let owner = host.text_surface(fields[0].body).unwrap() as *const _ as usize;
    assert_eq!(
        host.get(fields[1].body)
            .unwrap()
            .component::<TextFieldContent>()
            .unwrap()
            .field
            .label,
        "public-consumer · Output"
    );
    assert!(host.focus(Some(fields[0].body), true));
    tool.phase = "done".into();
    tool.result = Some(r#"{"text":"stdout cannot name this action"}"#.parse().unwrap());
    host.get_mut(root)
        .unwrap()
        .component_mut::<ToolActivity>()
        .unwrap()
        .update(&scope, 2, 1, &tool, ToolActivityPlacement::Call)
        .unwrap();
    assert!(title(&host).contains("修改 owned.rs · 2 个文件 · 已确认    +0 −0"));
    assert!(!title(&host).contains("stdout"));
    assert_eq!(
        host.text_surface(fields[0].body).unwrap() as *const _ as usize,
        owner
    );
    assert_eq!(host.focused(), Some(fields[0].body));
    assert_eq!(
        host.text_surface(fields[1].body).unwrap().input.text,
        "stdout cannot name this action"
    );
    tool.phase = "error".into();
    tool.result = Some(r#"{"text":"explicit failure"}"#.parse().unwrap());
    host.get_mut(root)
        .unwrap()
        .component_mut::<ToolActivity>()
        .unwrap()
        .update(&scope, 3, 1, &tool, ToolActivityPlacement::Call)
        .unwrap();
    assert!(title(&host).contains("Edit · explicit failure    +0 −0"));
    assert!(!title(&host).contains("修改"));
    // A different explicit source has no report; its current-workspace fallback is separate.
    tool.id = "without-report".into();
    tool.phase = "done".into();
    let mut activity = ToolActivity::new(736, scope.clone(), labels.clone());
    activity
        .update(&scope, 1, 1, &tool, ToolActivityPlacement::Call)
        .unwrap();
    let id = activity.content_ids().unwrap().disclosure.unwrap();
    let host = activity.into_host().unwrap();
    assert!(
        host.get(id)
            .unwrap()
            .component::<Button>()
            .unwrap()
            .label
            .contains("+20 −9")
    );
    tool.id = "reported".into();
    let foreign = RunScope::main("other-endpoint", "tool-report");
    let mut activity = ToolActivity::new(737, foreign.clone(), labels);
    activity
        .update(&foreign, 1, 1, &tool, ToolActivityPlacement::Call)
        .unwrap();
    let id = activity.content_ids().unwrap().disclosure.unwrap();
    let root = activity.id();
    let fields = activity.tool_detail_sections();
    let mut host = activity.into_host().unwrap();
    let title = &host.get(id).unwrap().component::<Button>().unwrap().label;
    assert!(
        !title.contains("修改")
            && !title.contains("已确认")
            && !title.contains("+20")
            && !title.contains("+0")
    );
    host.accessibility_action(id, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    host.get_mut(root)
        .unwrap()
        .component_mut::<ToolActivity>()
        .unwrap()
        .handle_actions(&actions);
    assert_eq!(
        host.get(fields[1].body)
            .unwrap()
            .component::<TextFieldContent>()
            .unwrap()
            .field
            .label,
        "other-endpoint · Output"
    );
}
#[test]
fn independent_tool_activity_consumes_producer_views_without_losing_focus_selection_or_source() {
    let scope = RunScope::main("public-consumer", "tool-title");
    let mut tool = TranscriptToolCall {
        result_meta: None,
        result_error: None,
        id: "call".into(),
        turn_id: "turn".into(),
        name: "read".into(),
        arguments: r#"{"path":"a.rs","payload":"not the headline"}"#.parse().unwrap(),
        result: None,
        phase: "doing".into(),
        call_view: Some(r#"{"for":"call","view":{"title":"读取 a.rs"}}"#.parse().unwrap()),
        result_view: None,
        source_position: None,
        result_position: None,
    };
    let before = tool.clone();
    let live_details = tool.presentation_details(TranscriptToolDetailWording::default());
    assert!(live_details.running);
    assert_eq!(
        live_details
            .sections
            .iter()
            .map(|s| s.kind)
            .collect::<Vec<_>>(),
        vec![
            TranscriptToolDetailKind::Input,
            TranscriptToolDetailKind::Output,
            TranscriptToolDetailKind::CallView,
        ]
    );
    assert!(live_details.sections[1].copy_text.is_empty());
    let mut activity = ToolActivity::new(733, scope.clone(), Default::default());
    activity
        .update(&scope, 1, 1, &tool, ToolActivityPlacement::Call)
        .unwrap();
    let root = activity.id();
    let ids = activity.content_ids().unwrap();
    let sections = activity.tool_detail_sections();
    let input = sections[0].body;
    let output = sections[1].body;
    let mut host = activity.into_host().unwrap();
    let label = &host
        .get(ids.disclosure.unwrap())
        .unwrap()
        .component::<Button>()
        .unwrap()
        .label;
    assert!(label.contains("Read · 读取 a.rs · doing"));
    assert!(!label.contains("not the headline"));
    assert_eq!(tool, before);
    host.accessibility_action(ids.disclosure.unwrap(), None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    assert!(
        host.get_mut(root)
            .unwrap()
            .component_mut::<ToolActivity>()
            .unwrap()
            .handle_actions(&actions)
    );
    assert!(host.focus(Some(input), true));
    host.get_mut(input)
        .unwrap()
        .component_mut::<TextFieldContent>()
        .unwrap()
        .surface
        .input
        .select_range(0..2);
    let owner = host.text_surface(input).unwrap() as *const _ as usize;
    let selection = host.text_surface(input).unwrap().input.selection();
    tool.phase = "done".into();
    tool.result = Some(
        r#"{"text":"new raw result is not a guessed title"}"#
            .parse()
            .unwrap(),
    );
    tool.result_view = Some(
        r#"{"for":"result","view":{"title":"读取 a.rs · 12 行"}}"#
            .parse()
            .unwrap(),
    );
    let settled_details = tool.presentation_details(TranscriptToolDetailWording {
        progress: Some("new raw result is not a guessed title"),
        ..Default::default()
    });
    assert!(!settled_details.running);
    assert_eq!(settled_details.sections[0], live_details.sections[0]);
    assert_eq!(
        settled_details.sections[1].copy_text,
        "new raw result is not a guessed title"
    );
    assert_eq!(
        settled_details.sections[3].kind,
        TranscriptToolDetailKind::ResultView
    );
    host.get_mut(root)
        .unwrap()
        .component_mut::<ToolActivity>()
        .unwrap()
        .update(&scope, 2, 1, &tool, ToolActivityPlacement::Call)
        .unwrap();
    assert_eq!(
        host.get(root)
            .unwrap()
            .component::<ToolActivity>()
            .unwrap()
            .content_ids(),
        Some(ids)
    );
    assert_eq!(
        host.text_surface(input).unwrap() as *const _ as usize,
        owner
    );
    assert_eq!(
        host.text_surface(input).unwrap().input.selection(),
        selection
    );
    assert_eq!(host.focused(), Some(input));
    let label = &host
        .get(ids.disclosure.unwrap())
        .unwrap()
        .component::<Button>()
        .unwrap()
        .label;
    assert!(label.contains("Read · 读取 a.rs · 12 行 · done"));
    assert!(!label.contains("new raw result"));
    assert!(
        host.text_surface(output)
            .unwrap()
            .input
            .text
            .contains("new raw result")
    );
    assert!(
        !host
            .text_surface(output)
            .unwrap()
            .input
            .text
            .contains("\"text\":")
    );
    // A canonical failed phase remains authoritative even when a process
    // failure was reported with isError:false by the underlying tool.
    tool.phase = "error".into();
    tool.result_meta = Some(r#"{"exitCode":2,"future":{"opaque":"retained"}}"#.parse().unwrap());
    tool.result_error = Some(
        r#"{"name":"OwnedToolFailure","code":"owned_failure"}"#
            .parse()
            .unwrap(),
    );
    tool.result = Some(
        r#"{"isError":false,"content":[{"type":"text","text":"exit failure output"}]}"#
            .parse()
            .unwrap(),
    );
    tool.result_view = Some(
        r#"{"for":"result","view":{"title":"读取失败","exitCode":2}}"#
            .parse()
            .unwrap(),
    );
    host.get_mut(root)
        .unwrap()
        .component_mut::<ToolActivity>()
        .unwrap()
        .update(&scope, 3, 1, &tool, ToolActivityPlacement::Call)
        .unwrap();
    let label = &host
        .get(ids.disclosure.unwrap())
        .unwrap()
        .component::<Button>()
        .unwrap()
        .label;
    assert!(label.contains("Read · exit failure output · error"));
    assert_eq!(
        host.text_surface(input).unwrap() as *const _ as usize,
        owner
    );
    assert_eq!(
        host.text_surface(input).unwrap().input.selection(),
        selection
    );
    assert_eq!(host.focused(), Some(input));
    assert!(
        host.text_surface(output)
            .unwrap()
            .input
            .text
            .contains("exit failure output")
    );
    assert_eq!(
        host.get(output)
            .unwrap()
            .component::<TextFieldContent>()
            .unwrap()
            .field
            .label,
        "结果 · 失败"
    );

    assert_eq!(
        host.get(root)
            .unwrap()
            .component::<ToolActivity>()
            .unwrap()
            .content_ids(),
        Some(ids)
    );
}
#[test]
fn independent_legacy_terminal_card_exposes_exit_failure_without_changing_source_phase() {
    let scope = RunScope::main("public-consumer", "legacy-terminal");
    let tool = TranscriptToolCall {
        id: "legacy-call".into(), turn_id: "turn".into(), name: "shell".into(), phase: "done".into(),
        arguments: r#"{"command":"test -f owned.rs","description":"检查文件"}"#.parse().unwrap(),
        call_view: Some(r#"{"for":"call","view":{"card":"terminal","description":"检查文件"}}"#.parse().unwrap()),
        result: Some(r#"{"isError":false,"content":[{"type":"text","text":"owned output\n[exit code: 2]"}]}"#.parse().unwrap()),
        result_meta: None, result_error: None, result_view: None, source_position: None, result_position: None,
    };
    let original = tool.clone();
    let Some(TranscriptToolCardModel::Terminal(card)) = tool.presentation_card() else {
        panic!("missing public terminal facts")
    };
    assert!(card.exit_failed());
    assert_eq!(card.output.as_deref(), Some("owned output"));
    let mut activity = ToolActivity::new(734, scope.clone(), Default::default());
    activity
        .update(&scope, 1, 1, &tool, ToolActivityPlacement::Call)
        .unwrap();
    let ids = activity.content_ids().unwrap();
    let host = activity.into_host().unwrap();
    let label = &host
        .get(ids.disclosure.unwrap())
        .unwrap()
        .component::<Button>()
        .unwrap()
        .label;
    assert!(label.contains("Shell · 检查文件 · error"));
    assert!(!label.contains("owned output"));
    assert_eq!(tool, original);
    assert_eq!(tool.phase, "done");
}
