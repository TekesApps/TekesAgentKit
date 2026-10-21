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

#[test]
fn host_sections_precede_providers_and_collapsed_rows_keep_their_owner_without_accepting_actions() {
    let scope = RunScope::main("native", "summary");
    let mut input = SummaryInput {
        revision: 1,
        scope: scope.clone(),
        facts: Some(facts(scope.clone())),
        host_sections: vec![
            SummarySection {
                id: "repository:one".into(),
                rows: vec![SummaryRow {
                    id: "branch".into(),
                    title: "main".into(),
                    value: serde_json::Value::Null,
                    action: None,
                }],
            },
            SummarySection {
                id: "empty-host".into(),
                rows: vec![],
            },
        ],
    };
    let mut view = SummaryView::new(94, Default::default());
    view.update(&input).unwrap();
    assert_eq!(
        view.section_order(),
        [
            "repository:one",
            "tasks",
            "subagents",
            "sources",
            "artifacts",
            "usage"
        ]
    );
    let root = view.id();
    let header = view.section_header_id("tasks").unwrap();
    let task = view.action_id("tasks", "task").unwrap();
    let mut host = view.into_host().unwrap();
    host.accessibility_action(task, None, AccessibilityAction::Press);
    let old_action = host.drain_actions();
    assert_eq!(
        host.get(task)
            .unwrap()
            .component::<SummaryActionRow>()
            .unwrap()
            .button()
            .activations,
        1
    );
    assert!(activate(&mut host, root, header).is_empty());
    let view = host.get(root).unwrap().component::<SummaryView>().unwrap();
    assert_eq!(view.section_expanded("tasks"), Some(false));
    assert!(view.take_intents(&old_action).is_empty());
    assert!(activate(&mut host, root, task).is_empty());
    assert!(!host.semantics().iter().any(|node| node.id == task));
    assert_eq!(
        host.semantics()
            .iter()
            .find(|node| node.id == header)
            .unwrap()
            .semantics
            .expanded,
        Some(false)
    );
    input.revision += 1;
    input.facts.as_mut().unwrap().tasks[0].status = "completed".into();
    host.get_mut(root)
        .unwrap()
        .component_mut::<SummaryView>()
        .unwrap()
        .update(&input)
        .unwrap();
    let view = host.get(root).unwrap().component::<SummaryView>().unwrap();
    assert_eq!(view.section_header_id("tasks"), Some(header));
    assert_eq!(view.action_id("tasks", "task"), Some(task));
    assert_eq!(view.section_expanded("tasks"), Some(false));
    activate(&mut host, root, header);
    assert!(host.semantics().iter().any(|node| node.id == task));
    assert_eq!(
        host.get(task)
            .unwrap()
            .component::<SummaryActionRow>()
            .unwrap()
            .button()
            .activations,
        1
    );
    assert_eq!(activate(&mut host, root, task).len(), 1);
}

#[test]
fn collapse_preference_survives_absence_and_scope_changes_but_old_row_actions_do_not() {
    let scope = RunScope::main("native", "one");
    let mut input = SummaryInput {
        revision: 1,
        scope: scope.clone(),
        facts: Some(facts(scope)),
        host_sections: vec![],
    };
    let mut view = SummaryView::new(95, Default::default());
    view.update(&input).unwrap();
    let root = view.id();
    let header = view.section_header_id("tasks").unwrap();
    let row = view.action_id("tasks", "task").unwrap();
    let mut host = view.into_host().unwrap();
    host.accessibility_action(row, None, AccessibilityAction::Press);
    let captured = host.drain_actions();
    activate(&mut host, root, header);
    input.revision += 1;
    input.facts.as_mut().unwrap().tasks.clear();
    host.get_mut(root)
        .unwrap()
        .component_mut::<SummaryView>()
        .unwrap()
        .update(&input)
        .unwrap();
    assert_eq!(
        host.get(root)
            .unwrap()
            .component::<SummaryView>()
            .unwrap()
            .section_expanded("tasks"),
        None
    );
    input.revision += 1;
    input.scope = RunScope::main("native", "two");
    input.facts = Some(facts(input.scope.clone()));
    host.get_mut(root)
        .unwrap()
        .component_mut::<SummaryView>()
        .unwrap()
        .update(&input)
        .unwrap();
    let view = host.get(root).unwrap().component::<SummaryView>().unwrap();
    assert_eq!(view.section_expanded("tasks"), Some(false));
    assert_ne!(view.action_id("tasks", "task"), Some(row));
    assert!(view.take_intents(&captured).is_empty());
}

#[test]
fn conflicting_publications_preserve_disclosure_and_empty_is_distinct_from_unavailable() {
    let scope = RunScope::main("native", "status");
    let input = SummaryInput {
        revision: 1,
        scope: scope.clone(),
        facts: Some(facts(scope.clone())),
        host_sections: vec![],
    };
    let mut view = SummaryView::new(96, Default::default());
    view.update(&input).unwrap();
    let root = view.id();
    let header = view.section_header_id("tasks").unwrap();
    let mut host = view.into_host().unwrap();
    activate(&mut host, root, header);
    let mut conflicting = input.clone();
    conflicting.facts.as_mut().unwrap().tasks.clear();
    assert!(
        host.get_mut(root)
            .unwrap()
            .component_mut::<SummaryView>()
            .unwrap()
            .update(&conflicting)
            .is_err()
    );
    assert_eq!(
        host.get(root)
            .unwrap()
            .component::<SummaryView>()
            .unwrap()
            .section_expanded("tasks"),
        Some(false)
    );
    let empty = SummaryInput {
        revision: 2,
        scope: scope.clone(),
        facts: Some(SummaryFacts {
            scope: scope.clone(),
            tasks: vec![],
            subagents: vec![],
            artifacts: vec![],
            usage: None,
        }),
        host_sections: vec![],
    };
    host.get_mut(root)
        .unwrap()
        .component_mut::<SummaryView>()
        .unwrap()
        .update(&empty)
        .unwrap();
    assert!(
        host.semantics()
            .iter()
            .any(|node| node.semantics.name == "暂无 Summary 内容")
    );
    let unavailable = SummaryInput {
        revision: 3,
        facts: None,
        ..empty
    };
    host.get_mut(root)
        .unwrap()
        .component_mut::<SummaryView>()
        .unwrap()
        .update(&unavailable)
        .unwrap();
    assert!(
        host.semantics()
            .iter()
            .any(|node| node.semantics.name == "Summary 暂不可用")
    );
}

#[test]
fn a_changed_task_pointer_requires_a_new_gesture_and_never_retargets_an_old_action() {
    let scope = RunScope::main("native", "pointer");
    let mut input = SummaryInput {
        revision: 1,
        scope: scope.clone(),
        facts: Some(facts(scope)),
        host_sections: vec![],
    };
    let mut view = SummaryView::new(97, Default::default());
    view.update(&input).unwrap();
    let root = view.id();
    let header = view.section_header_id("tasks").unwrap();
    let old = view.action_id("tasks", "task").unwrap();
    let mut host = view.into_host().unwrap();
    host.accessibility_action(old, None, AccessibilityAction::Press);
    let captured = host.drain_actions();
    input.revision += 1;
    input.facts.as_mut().unwrap().tasks[0].scope.session_id = "replacement-child".into();
    host.get_mut(root)
        .unwrap()
        .component_mut::<SummaryView>()
        .unwrap()
        .update(&input)
        .unwrap();
    let view = host.get(root).unwrap().component::<SummaryView>().unwrap();
    assert!(view.take_intents(&captured).is_empty());
    assert_eq!(view.section_header_id("tasks"), Some(header));
    let current = view.action_id("tasks", "task").unwrap();
    assert_ne!(old, current);
    assert!(
        matches!(activate(&mut host, root, current).as_slice(), [SummaryIntent { action: SummaryAction::OpenTask { scope, .. }, .. }] if scope.session_id == "replacement-child")
    );
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
    assert!(text.iter().any(|text| text == "输出"));
    assert!(text.iter().any(|text| text == "0"));
    assert!(text.iter().any(|text| text == "成本"));
    assert!(text.iter().any(|text| text == "未知"));
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
            projection_ready: true,
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

#[test]
fn composed_header_keeps_one_full_accessible_name_focus_and_disclosure_after_refresh() {
    let labels = SummaryLabels {
        section: |id| format!("{id}：完整中文标题 English 👩🏽‍💻 e\u{301} ").repeat(12),
        ..Default::default()
    };
    let title = (labels.section)("repository");
    let mut input = SummaryInput {
        revision: 1,
        scope: RunScope::main("consumer", "summary-header"),
        facts: None,
        host_sections: vec![SummarySection {
            id: "repository".into(),
            rows: vec![SummaryRow {
                id: "branch".into(),
                title: "main".into(),
                value: Default::default(),
                action: None,
            }],
        }],
    };
    let mut view = SummaryView::new(209, labels);
    view.update(&input).unwrap();
    let root = view.id();
    let header = view.section_header_id("repository").unwrap();
    let mut host = view.into_host().unwrap();
    assert!(!host.focus(Some(header), true));
    for expanded in [true, false] {
        let nodes: Vec<_> = host
            .semantics()
            .into_iter()
            .filter(|node| node.semantics.name == title)
            .collect();
        assert_eq!(
            nodes.len(),
            1,
            "the compound title is decorative, not a second AX element"
        );
        assert_eq!(nodes[0].id, header);
        assert_eq!(nodes[0].semantics.expanded, Some(expanded));
        assert_eq!(host.focused(), None);
        if expanded {
            assert!(activate(&mut host, root, header).is_empty());
            input.revision += 1;
            input.host_sections[0].rows[0].title = "updated".into();
            host.get_mut(root)
                .unwrap()
                .component_mut::<SummaryView>()
                .unwrap()
                .update(&input)
                .unwrap();
            assert_eq!(
                host.get(root)
                    .unwrap()
                    .component::<SummaryView>()
                    .unwrap()
                    .section_header_id("repository"),
                Some(header)
            );
        }
    }
}

#[test]
fn usage_formatting_matches_source_precision_and_preserves_integer_and_unknown_facts() {
    let cases = [
        (
            "total",
            serde_json::json!(u64::MAX),
            "18,446,744,073,709,551,615",
        ),
        ("input", serde_json::json!(1234567), "1,234,567"),
        ("output", serde_json::json!(0), "0"),
        ("cache", serde_json::json!(0), "0"),
        ("hit-rate", serde_json::json!(0.123456), "12.3%"),
        ("hit-rate", serde_json::json!(0), "0.0%"),
        ("hit-rate", serde_json::Value::Null, "—"),
        ("cost", serde_json::json!(0), "US$0.00"),
        ("cost", serde_json::json!(0.1234567), "US$0.123457"),
        ("cost", serde_json::json!(12.34), "US$12.34"),
        ("cost", serde_json::json!(12345.1234), "US$12,345.1234"),
        ("cost", serde_json::json!(-0.0), "US$0.00"),
        ("cost", serde_json::Value::Null, "未知"),
        ("cost", serde_json::json!(-1), "未知"),
        ("cache", serde_json::json!(-1), "未知"),
        ("input", serde_json::json!(1.5), "未知"),
        ("hit-rate", serde_json::json!(1.5), "未知"),
    ];
    for (id, value, expected) in cases {
        let row = SummaryRow {
            id: id.into(),
            title: id.into(),
            value,
            action: None,
        };
        assert_eq!(SummaryUsageRowText::read(&row).value, expected, "{id}");
    }
}

#[test]
fn usage_slots_retain_original_controls_and_follow_disclosure_and_run_identity() {
    let scope = RunScope::main("native", "usage");
    let mut input = SummaryInput {
        revision: 1,
        scope: scope.clone(),
        facts: Some(facts(scope)),
        host_sections: vec![],
    };
    let mut view = SummaryView::new(210, Default::default());
    view.update(&input).unwrap();
    let root = view.id();
    let ids = view.usage_row_ids("output").unwrap();
    let header = view.section_header_id("usage").unwrap();
    assert_eq!(view.action_id("usage", "output"), None);
    let mut host = view.into_host().unwrap();
    let original = host
        .get(ids.value)
        .unwrap()
        .component::<TextContent>()
        .unwrap() as *const _;
    activate(&mut host, root, header);
    assert!(!host.semantics().iter().any(|node| node.id == ids.value));
    input.revision += 1;
    input
        .facts
        .as_mut()
        .unwrap()
        .usage
        .as_mut()
        .unwrap()
        .output_tokens = 2345;
    host.get_mut(root)
        .unwrap()
        .component_mut::<SummaryView>()
        .unwrap()
        .update(&input)
        .unwrap();
    let view = host.get(root).unwrap().component::<SummaryView>().unwrap();
    assert_eq!(view.usage_row_ids("output"), Some(ids));
    assert_eq!(view.section_expanded("usage"), Some(false));
    assert_eq!(
        host.get(ids.value)
            .unwrap()
            .component::<TextContent>()
            .unwrap() as *const _,
        original
    );
    assert_eq!(
        host.get(ids.value)
            .unwrap()
            .component::<TextContent>()
            .unwrap()
            .text,
        "2,345"
    );
    activate(&mut host, root, header);
    assert!(
        host.semantics()
            .iter()
            .any(|node| node.id == ids.value && node.semantics.name == "2,345")
    );
    input.revision += 1;
    input.scope = RunScope::main("native", "other-usage");
    input.facts.as_mut().unwrap().scope = input.scope.clone();
    input.facts.as_mut().unwrap().tasks.clear();
    host.get_mut(root)
        .unwrap()
        .component_mut::<SummaryView>()
        .unwrap()
        .update(&input)
        .unwrap();
    assert_ne!(
        host.get(root)
            .unwrap()
            .component::<SummaryView>()
            .unwrap()
            .usage_row_ids("output"),
        Some(ids)
    );
    assert!(host.get(ids.value).is_none());
}

#[test]
fn passive_summary_uses_the_original_readonly_text_owner_across_updates_and_disclosure() {
    let mut input = SummaryInput {
        revision: 1,
        scope: RunScope::main("native", "passive"),
        facts: None,
        host_sections: vec![SummarySection {
            id: "host:info".into(),
            rows: vec![SummaryRow {
                id: "source".into(),
                title: "来源中文👩🏽‍💻".into(),
                value: serde_json::Value::Null,
                action: None,
            }],
        }],
    };
    let mut view = SummaryView::new(211, Default::default());
    view.update(&input).unwrap();
    let root = view.id();
    let field = view.passive_text_id("host:info", "source").unwrap();
    let header = view.section_header_id("host:info").unwrap();
    assert_eq!(view.action_id("host:info", "source"), None);
    let mut host = view.into_host().unwrap();
    let owner = host
        .get(field)
        .unwrap()
        .component::<TextFieldContent>()
        .unwrap() as *const _;
    host.accessibility_action(field, None, AccessibilityAction::SetSelection(0..2));
    let text = &host
        .get(field)
        .unwrap()
        .component::<TextFieldContent>()
        .unwrap()
        .surface
        .input;
    assert!(text.read_only);
    assert_eq!(text.selected_text(), "来源");
    host.accessibility_action(
        field,
        None,
        AccessibilityAction::ReplaceSelectedText("bad".into()),
    );
    assert_eq!(
        host.get(field)
            .unwrap()
            .component::<TextFieldContent>()
            .unwrap()
            .surface
            .input
            .text,
        "来源中文👩🏽‍💻"
    );
    activate(&mut host, root, header);
    assert!(!host.semantics().iter().any(|node| node.id == field));
    input.revision += 1;
    input.host_sections[0].rows[0].title.push_str("追加");
    host.get_mut(root)
        .unwrap()
        .component_mut::<SummaryView>()
        .unwrap()
        .update(&input)
        .unwrap();
    let text = host
        .get(field)
        .unwrap()
        .component::<TextFieldContent>()
        .unwrap();
    assert_eq!(text as *const _, owner);
    assert_eq!(text.surface.input.selected_text(), "来源");
    let pending = host.drain_actions();
    assert!(
        host.get(root)
            .unwrap()
            .component::<SummaryView>()
            .unwrap()
            .take_intents(&pending)
            .is_empty()
    );
    activate(&mut host, root, header);
    assert!(
        host.semantics()
            .iter()
            .any(|node| node.id == field
                && node.semantics.value.as_deref() == Some("来源中文👩🏽‍💻追加"))
    );
    input.revision += 1;
    input.scope = RunScope::main("native", "replacement");
    host.get_mut(root)
        .unwrap()
        .component_mut::<SummaryView>()
        .unwrap()
        .update(&input)
        .unwrap();
    assert_ne!(
        host.get(root)
            .unwrap()
            .component::<SummaryView>()
            .unwrap()
            .passive_text_id("host:info", "source"),
        Some(field)
    );
    assert!(host.get(field).is_none());
}

#[test]
fn summary_action_label_preflight_is_atomic_and_each_format_is_captured_once() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static METAS: AtomicUsize = AtomicUsize::new(0);
    static FORMATS: AtomicUsize = AtomicUsize::new(0);
    let labels = SummaryLabels {
        trailing: std::sync::Arc::new(|_, _| {
            Some(format!("meta {}", METAS.fetch_add(1, Ordering::SeqCst)))
        }),
        row: |_, row| {
            if row.title == "reject" {
                String::new()
            } else {
                format!("{} {}", row.title, FORMATS.fetch_add(1, Ordering::SeqCst))
            }
        },
        ..Default::default()
    };
    let scope = RunScope::main("consumer", "preflight");
    let mut input = SummaryInput {
        revision: 1,
        scope: scope.clone(),
        facts: None,
        host_sections: vec![SummarySection {
            id: "host".into(),
            rows: vec![SummaryRow {
                id: "action".into(),
                title: "valid".into(),
                value: Default::default(),
                action: Some(SummaryAction::OpenArtifact {
                    scope,
                    path: "file".into(),
                }),
            }],
        }],
    };
    let mut summary = SummaryView::new(49100, labels);
    summary.update(&input).unwrap();
    assert_eq!(FORMATS.load(Ordering::SeqCst), 1);
    assert_eq!(METAS.load(Ordering::SeqCst), 1);
    let ids = summary.default_row_ids("host", "action").unwrap();
    let root = summary.id();
    let mut host = summary.into_host().unwrap();
    assert_eq!(
        host.get(ids.title)
            .unwrap()
            .component::<TextContent>()
            .unwrap()
            .text,
        "valid 0"
    );
    input.revision += 1;
    input.host_sections[0].rows[0].title = "reject".into();
    let summary = host
        .get_mut(root)
        .unwrap()
        .component_mut::<SummaryView>()
        .unwrap();
    assert_eq!(summary.update(&input), Err("empty Summary action label"));
    assert_eq!(summary.default_row_ids("host", "action"), Some(ids));
    assert_eq!(summary.displayed_input().unwrap().revision, 1);
    assert_eq!(
        host.get(ids.row)
            .unwrap()
            .component::<SummaryActionRow>()
            .unwrap()
            .button()
            .label,
        "valid 0"
    );
}

#[test]
fn default_summary_titles_do_not_leak_raw_execution_status_or_invent_metadata() {
    let labels = SummaryLabels::default();
    for section in ["tasks", "subagents", "sources", "artifacts"] {
        let row = SummaryRow {
            id: "r".into(),
            title: "原始标题".into(),
            value: "running".into(),
            action: None,
        };
        assert_eq!((labels.row)(section, &row), "原始标题");
        assert_eq!((labels.trailing)(section, &row), None);
    }
}
