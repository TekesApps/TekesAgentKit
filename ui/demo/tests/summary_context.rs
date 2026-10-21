use tekes_agent_components_ui::*;
use tekes_ui_controls::composition::*;
fn workspace(revision: u64, id: &str) -> SummaryContextInput {
    SummaryContextInput {
        revision,
        owner: SummaryOwner::Workspace { id: id.into() },
        facts: None,
        ready: true,
        host_sections: vec![SummarySection {
            id: "host:workspace".into(),
            rows: vec![SummaryRow {
                id: "name".into(),
                title: "工作区".into(),
                value: serde_json::json!(id),
                action: None,
            }],
        }],
    }
}
fn view(host: &ContentHost, root: ComponentId) -> &SummaryView {
    host.get(root).unwrap().component().unwrap()
}
fn view_mut(host: &mut ContentHost, root: ComponentId) -> &mut SummaryView {
    host.get_mut(root).unwrap().component_mut().unwrap()
}
#[test]
fn workspace_rows_need_no_run_and_retain_original_text_and_disclosure() {
    let mut input = workspace(1, "workspace-a");
    let mut summary = SummaryView::new(32801, Default::default());
    tekes_agent_ui_consumer_check::publish_summary_context(&mut summary, &input).unwrap();
    let root = summary.id();
    let row = summary.row_id("host:workspace", "name").unwrap();
    let header = summary.section_header_id("host:workspace").unwrap();
    let mut host = summary.into_host().unwrap();
    assert!(view(&host, root).displayed_input().is_none());
    assert_eq!(view(&host, root).displayed_context_input(), Some(&input));
    host.accessibility_action(row, None, AccessibilityAction::SetSelection(0..3));
    assert!(host.text_surface(row).unwrap().input.read_only);
    assert_eq!(
        host.text_surface(row).unwrap().input.selected_text(),
        "工作区"
    );
    input.revision += 1;
    view_mut(&mut host, root).update_context(&input).unwrap();
    assert_eq!(
        view(&host, root).row_id("host:workspace", "name"),
        Some(row)
    );
    assert_eq!(
        host.text_surface(row).unwrap().input.selected_text(),
        "工作区"
    );
    host.accessibility_action(header, None, AccessibilityAction::Press);
    assert_eq!(
        view(&host, root).section_expanded("host:workspace"),
        Some(false)
    );
    input.revision += 1;
    input.host_sections[0].rows[0].value = serde_json::json!("更新后的工作区");
    view_mut(&mut host, root).update_context(&input).unwrap();
    assert_eq!(
        view(&host, root).section_header_id("host:workspace"),
        Some(header)
    );
    assert!(!host.semantics().iter().any(|node| node.id == row));
    let mut b = workspace(4, "workspace-b");
    view_mut(&mut host, root).update_context(&b).unwrap();
    assert_ne!(
        view(&host, root).row_id("host:workspace", "name"),
        Some(row)
    );
    assert_eq!(
        view(&host, root).section_expanded("host:workspace"),
        Some(false)
    );
    b.revision += 1;
    b.host_sections.clear();
    view_mut(&mut host, root).update_context(&b).unwrap();
    assert!(
        host.semantics()
            .iter()
            .any(|node| node.semantics.name == "暂无 Summary 内容")
    );
    b.revision += 1;
    b.ready = false;
    view_mut(&mut host, root).update_context(&b).unwrap();
    assert!(
        host.semantics()
            .iter()
            .any(|node| node.semantics.name == "Summary 暂不可用")
    );
    let actions = host.drain_actions();
    assert!(view(&host, root).take_intents(&actions).is_empty());
}
#[test]
fn context_switch_retires_run_actions_and_workspace_rejects_session_authority_atomically() {
    let scope = RunScope::main("fixture", "source");
    let legacy = SummaryInput {
        revision: 1,
        scope: scope.clone(),
        facts: None,
        host_sections: vec![SummarySection {
            id: "host:run".into(),
            rows: vec![SummaryRow {
                id: "file".into(),
                title: "产物".into(),
                value: serde_json::Value::Null,
                action: Some(SummaryAction::OpenArtifact {
                    scope: scope.clone(),
                    path: "a.txt".into(),
                }),
            }],
        }],
    };
    let mut summary = SummaryView::new(32802, Default::default());
    summary.update(&legacy).unwrap();
    let root = summary.id();
    let action = summary.action_id("host:run", "file").unwrap();
    let mut host = summary.into_host().unwrap();
    host.accessibility_action(action, None, AccessibilityAction::Press);
    let old = host.drain_actions();
    assert_eq!(view(&host, root).take_intents(&old).len(), 1);
    let input = workspace(2, "workspace");
    view_mut(&mut host, root).update_context(&input).unwrap();
    assert!(view(&host, root).take_intents(&old).is_empty());
    let row = view(&host, root).row_id("host:workspace", "name");
    let mut invalid = input.clone();
    invalid.revision += 1;
    invalid.host_sections[0].rows[0].action = Some(SummaryAction::OpenArtifact {
        scope: scope.clone(),
        path: "a.txt".into(),
    });
    assert!(view_mut(&mut host, root).update_context(&invalid).is_err());
    invalid.host_sections = input.host_sections.clone();
    invalid.facts = Some(SummaryFacts {
        scope,
        tasks: vec![],
        subagents: vec![],
        artifacts: vec![],
        usage: None,
    });
    assert!(view_mut(&mut host, root).update_context(&invalid).is_err());
    assert_eq!(view(&host, root).displayed_context_input(), Some(&input));
    assert_eq!(view(&host, root).row_id("host:workspace", "name"), row);
    let mut conflict = input.clone();
    conflict.ready = false;
    assert!(view_mut(&mut host, root).update_context(&conflict).is_err());
    assert!(view_mut(&mut host, root).update(&legacy).is_err());
    let mut restored = legacy;
    restored.revision = 3;
    view_mut(&mut host, root).update(&restored).unwrap();
    assert_eq!(view(&host, root).displayed_input(), Some(&restored));
    assert_ne!(
        view(&host, root).action_id("host:run", "file"),
        Some(action)
    );
    assert!(view(&host, root).take_intents(&old).is_empty());
}
