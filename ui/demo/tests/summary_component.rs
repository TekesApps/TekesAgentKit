use tekes_agent_components_ui::*;
use tekes_agent_presentation::*;
use tekes_ui_controls::composition::*;
fn facts(parent: RunScope) -> SummaryFacts {
    SummaryFacts {
        scope: parent.clone(),
        tasks: vec![AgentTask {
            id: "task".into(),
            scope: RunScope {
                endpoint_id: parent.endpoint_id.clone(),
                thread_id: parent.thread_id.clone(),
                session_id: "task-child".into(),
            },
            name: "Source task name".into(),
            status: "running".into(),
            kind: "agent".into(),
        }],
        subagents: vec![SummarySubagent {
            id: "source-tool".into(),
            title: "Child title".into(),
            session_id: Some("child".into()),
            status: "completed".into(),
        }],
        artifacts: vec![SummaryArtifact {
            id: "/owned/proof.txt".into(),
            source: Some("source-file".into()),
        }],
        usage: Some(SummaryUsage {
            input_tokens: 0,
            cached_input_tokens: 0,
            cache_creation_input_tokens: 0,
            output_tokens: 0,
            cost_usd: Some(0.4),
            is_cost_complete: false,
        }),
    }
}
fn activate(host: &mut ContentHost, root: ComponentId, target: ComponentId) -> Vec<SummaryIntent> {
    host.accessibility_action(target, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    host.get(root)
        .unwrap()
        .component::<SummaryView>()
        .unwrap()
        .take_intents(&actions)
}
#[test]
fn actual_shared_sections_keep_actions_scoped_and_distinguish_known_zero_from_unknown_cost() {
    let parent = RunScope::main("native", "parent");
    let facts = facts(parent.clone());
    let mut view = SummaryView::new(90, SummaryLabels::default());
    let root = view.id();
    view.update(&SummaryInput {
        revision: 1,
        scope: parent.clone(),
        facts: Some(facts.clone()),
        host_sections: Vec::new(),
    })
    .unwrap();
    let task = view.action_id("tasks", "task").unwrap();
    let child = view.action_id("subagents", "source-tool").unwrap();
    let artifact = view.action_id("artifacts", "/owned/proof.txt").unwrap();
    let mut host = view.into_host().unwrap();
    let sections = facts.sections(&parent);
    for (section, row, id) in [
        ("tasks", "task", task),
        ("subagents", "source-tool", child),
        ("artifacts", "/owned/proof.txt", artifact),
    ] {
        let expected = sections
            .iter()
            .find(|s| s.id == section)
            .unwrap()
            .rows
            .iter()
            .find(|r| r.id == row)
            .unwrap()
            .action
            .clone()
            .unwrap();
        assert_eq!(
            activate(&mut host, root, id),
            vec![SummaryIntent {
                scope: parent.clone(),
                action: expected
            }]
        );
    }
    let text: Vec<_> = host
        .semantics()
        .into_iter()
        .map(|node| node.semantics.name)
        .collect();
    assert!(text.iter().any(|text| text == "output · 0"));
    assert!(text.iter().any(|text| text == "cost · 未知"));
    assert!(
        !text.iter().any(|text| text.contains("0.4")),
        "incomplete cost was presented as known"
    );
}
#[test]
fn invalid_late_and_duplicate_host_sections_preserve_existing_summary_until_valid_replacement() {
    let parent = RunScope::main("native", "parent");
    let model = SummaryInput {
        revision: 5,
        scope: parent.clone(),
        facts: Some(facts(parent.clone())),
        host_sections: Vec::new(),
    };
    let mut view = SummaryView::new(91, SummaryLabels::default());
    view.update(&model).unwrap();
    let root = view.id();
    let task = view.action_id("tasks", "task").unwrap();
    let mut host = view.into_host().unwrap();
    let mut invalid = model.clone();
    invalid.revision = 4;
    assert!(
        host.get_mut(root)
            .unwrap()
            .component_mut::<SummaryView>()
            .unwrap()
            .update(&invalid)
            .is_err()
    );
    invalid.revision = 6;
    invalid.facts = Some(facts(RunScope::main("pi", "parent")));
    assert!(
        host.get_mut(root)
            .unwrap()
            .component_mut::<SummaryView>()
            .unwrap()
            .update(&invalid)
            .is_err()
    );
    invalid = model.clone();
    invalid.revision = 6;
    invalid.host_sections = vec![model.facts.as_ref().unwrap().sections(&parent)[0].clone()];
    assert!(
        host.get_mut(root)
            .unwrap()
            .component_mut::<SummaryView>()
            .unwrap()
            .update(&invalid)
            .is_err()
    );
    assert_eq!(activate(&mut host, root, task).len(), 1);
    let empty = SummaryInput {
        revision: 6,
        scope: parent,
        facts: None,
        host_sections: Vec::new(),
    };
    host.get_mut(root)
        .unwrap()
        .component_mut::<SummaryView>()
        .unwrap()
        .update(&empty)
        .unwrap();
    assert!(activate(&mut host, root, task).is_empty());
}
#[test]
fn real_summary_component_mounts_inside_a_domain_panel_slot_and_retains_shared_actions() {
    let parent = RunScope::main("native", "parent");
    let mut summary = SummaryView::new(92, SummaryLabels::default());
    summary
        .update(&SummaryInput {
            revision: 1,
            scope: parent.clone(),
            facts: Some(facts(parent.clone())),
            host_sections: Vec::new(),
        })
        .unwrap();
    let summary_id = summary.id();
    let task = summary.action_id("tasks", "task").unwrap();
    let mut panel = SidePanel::new(93, SidePanelLabels::default(), 4);
    panel
        .update(&SidePanelInput {
            revision: 1,
            parent: parent.clone(),
            tabs: vec![SidePanelTab::summary("Summary")],
            selected: Some(SidePanelTabKind::Summary),
            side_chats: SideChatSnapshot::default(),
            supports_side_chat: Some(false),
        })
        .unwrap();
    let token = panel.content_token(&SidePanelTabKind::Summary).unwrap();
    panel
        .install_content(&token, Content::new(summary_id, summary))
        .unwrap();
    let mut host = panel.into_host().unwrap();
    let intents = activate(&mut host, summary_id, task);
    assert!(
        matches!(intents.as_slice(),[SummaryIntent{scope,action:SummaryAction::OpenTask{task_id,..}}] if scope==&parent && task_id=="task")
    );
}
