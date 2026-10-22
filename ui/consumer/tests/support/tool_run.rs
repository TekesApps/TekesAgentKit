use super::*;

fn call(id: &str) -> TranscriptToolCall {
    TranscriptToolCall {
        id: id.into(),
        turn_id: "turn".into(),
        name: "read".into(),
        phase: "doing".into(),
        arguments: r#"{"path":"src/中文.rs"}"#.parse().unwrap(),
        result: Some(r#"{"text":"原始输出\n"}"#.parse().unwrap()),
        call_view: None,
        result_view: None,
        result_meta: None,
        result_error: None,
        source_position: None,
        result_position: None,
    }
}
fn group(host: &ContentHost, root: ComponentId) -> &ToolRun {
    host.get(root).unwrap().component::<ToolRun>().unwrap()
}
fn group_mut(host: &mut ContentHost, root: ComponentId) -> &mut ToolRun {
    host.get_mut(root)
        .unwrap()
        .component_mut::<ToolRun>()
        .unwrap()
}
fn activate(host: &mut ContentHost, root: ComponentId, button: ComponentId) -> Vec<ContentAction> {
    host.accessibility_action(button, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    group_mut(host, root).handle_actions(&actions);
    actions
}

#[test]
fn public_group_retains_member_fields_and_rejects_hidden_copy_actions() {
    let scope = RunScope::main("second-product", "group");
    let mut tools = vec![call("a"), call("b")];
    let mut view = ToolRun::new(
        65200,
        65201..65300,
        scope.clone(),
        Default::default(),
        Default::default(),
    )
    .unwrap();
    view.update(&scope, 1, 1, &tools).unwrap();
    let root = view.id();
    let header = view.disclosure_id().unwrap();
    let member = view.tool_activity("a").unwrap().id();
    let disclosure = view
        .tool_activity("a")
        .unwrap()
        .content_ids()
        .unwrap()
        .disclosure
        .unwrap();
    let mut host = view.into_host().unwrap();
    assert!(host.get(member).is_none());
    activate(&mut host, root, header);
    activate(&mut host, root, disclosure);
    let sections = group(&host, root)
        .tool_activity("a")
        .unwrap()
        .tool_detail_sections();
    let input = sections[0].body;
    let output = sections[1];
    let owner = host.text_surface(input).unwrap() as *const _ as usize;
    let actions = activate(&mut host, root, output.copy);
    let copy = group(&host, root)
        .take_copy_intents(&actions)
        .pop()
        .unwrap();
    assert_eq!(copy.scope(), &scope);
    assert_eq!(copy.text(), "原始输出\n");
    assert!(copy.claim());
    assert!(!copy.clone().claim());
    activate(&mut host, root, header);
    assert!(group(&host, root).copy_action_ids().is_empty());
    assert!(group(&host, root).take_copy_intents(&actions).is_empty());
    activate(&mut host, root, header);
    assert_eq!(
        host.text_surface(input).unwrap() as *const _ as usize,
        owner
    );
    tools.push(call("c"));
    group_mut(&mut host, root)
        .update(&scope, 2, 1, &tools)
        .unwrap();
    assert_eq!(group(&host, root).disclosure_id(), Some(header));
    assert!(group(&host, root).is_expanded());
    assert_eq!(group(&host, root).tool_activity("a").unwrap().id(), member);
    assert_eq!(
        host.text_surface(input).unwrap() as *const _ as usize,
        owner
    );
    let summary = group(&host, root).summary().cloned();
    assert!(
        group_mut(&mut host, root)
            .update(&RunScope::main("other", "group"), 3, 1, &tools)
            .is_err()
    );
    assert!(
        group_mut(&mut host, root)
            .update(&scope, 1, 1, &tools)
            .is_err()
    );
    let mut duplicate = tools.clone();
    duplicate.push(tools[0].clone());
    assert!(
        group_mut(&mut host, root)
            .update(&scope, 3, 1, &duplicate)
            .is_err()
    );
    assert_eq!(group(&host, root).summary(), summary.as_ref());
    tools.remove(0);
    group_mut(&mut host, root)
        .update(&scope, 3, 1, &tools)
        .unwrap();
    assert_ne!(group(&host, root).disclosure_id(), Some(header));
    assert!(!group(&host, root).is_expanded());
    tools.insert(0, call("a"));
    group_mut(&mut host, root)
        .update(&scope, 4, 1, &tools)
        .unwrap();
    assert_ne!(group(&host, root).tool_activity("a").unwrap().id(), member);
}
