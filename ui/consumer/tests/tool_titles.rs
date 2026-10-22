//! Independent product consumes the same original tool disclosure and title facts.
use tekes_agent_components_ui::*;
use tekes_ui_controls::composition::*;
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
    let mut host = activity.into_host().unwrap();
    let label = &host
        .get(ids.disclosure.unwrap())
        .unwrap()
        .component::<ButtonContent>()
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
    assert!(host.focus(Some(ids.body), true));
    host.get_mut(ids.body)
        .unwrap()
        .component_mut::<TextFieldContent>()
        .unwrap()
        .surface
        .input
        .select_range(0..2);
    let owner = host.text_surface(ids.body).unwrap() as *const _ as usize;
    let selection = host.text_surface(ids.body).unwrap().input.selection();
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
        host.text_surface(ids.body).unwrap() as *const _ as usize,
        owner
    );
    assert_eq!(
        host.text_surface(ids.body).unwrap().input.selection(),
        selection
    );
    assert_eq!(host.focused(), Some(ids.body));
    let label = &host
        .get(ids.disclosure.unwrap())
        .unwrap()
        .component::<ButtonContent>()
        .unwrap()
        .label;
    assert!(label.contains("Read · 读取 a.rs · 12 行 · done"));
    assert!(!label.contains("new raw result"));
    assert!(
        host.text_surface(ids.body)
            .unwrap()
            .input
            .text
            .contains("new raw result")
    );
    assert!(
        !host
            .text_surface(ids.body)
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
        .component::<ButtonContent>()
        .unwrap()
        .label;
    assert!(label.contains("Read · exit failure output · error"));
    assert_eq!(
        host.text_surface(ids.body).unwrap() as *const _ as usize,
        owner
    );
    assert_eq!(
        host.text_surface(ids.body).unwrap().input.selection(),
        selection
    );
    assert_eq!(host.focused(), Some(ids.body));
    assert!(
        host.text_surface(ids.body)
            .unwrap()
            .input
            .text
            .contains("exit failure output")
    );
    assert!(
        host.text_surface(ids.body)
            .unwrap()
            .input
            .text
            .contains("结果 · 失败\nexit failure output")
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
        .component::<ButtonContent>()
        .unwrap()
        .label;
    assert!(label.contains("Shell · 检查文件 · error"));
    assert!(!label.contains("owned output"));
    assert_eq!(tool, original);
    assert_eq!(tool.phase, "done");
}
