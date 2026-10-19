use tekes_agent_components_ui::*;
use tekes_agent_presentation::*;
use tekes_ui_controls::composition::*;
fn input(parent: RunScope) -> SidePanelInput {
    SidePanelInput {
        revision: 1,
        parent,
        tabs: vec![SidePanelTab::summary("Summary")],
        selected: Some(SidePanelTabKind::Summary),
        side_chats: SideChatSnapshot::default(),
        supports_side_chat: Some(false),
    }
}
fn mounted_panel(host: &ContentHost, id: ComponentId) -> &SidePanel {
    host.get(id).unwrap().component::<SidePanel>().unwrap()
}
fn panel_mut(host: &mut ContentHost, id: ComponentId) -> &mut SidePanel {
    host.get_mut(id)
        .unwrap()
        .component_mut::<SidePanel>()
        .unwrap()
}
#[test]
fn confirmed_scratch_absence_retires_inactive_parent_content_and_install_tokens() {
    let parent = RunScope::main("fixture", "a");
    let chat = SideChat {
        parent: parent.clone(),
        scope: RunScope::main("fixture", "scratch"),
        title: "Scratch".into(),
        phase: SideChatPhase::Ready,
        error: None,
    };
    let tab = SidePanelTab::side_chat(&chat);
    let mut a = input(parent);
    a.tabs.push(tab.clone());
    a.selected = Some(tab.kind.clone());
    a.side_chats
        .chats
        .insert(chat.scope.session_id.clone(), chat.clone());
    let mut panel = SidePanel::new(72, SidePanelLabels::default(), 4);
    panel.update(&a).unwrap();
    let root = panel.id();
    let token = panel.content_token(&tab.kind).unwrap();
    panel
        .install_content(
            &token,
            Content::new(ComponentId(4500), TextContent::new("retained scratch")),
        )
        .unwrap();
    let mut host = panel.into_host().unwrap();
    let mut b = input(RunScope::main("fixture", "b"));
    b.revision = 2;
    b.side_chats = a.side_chats.clone();
    panel_mut(&mut host, root).update(&b).unwrap();
    assert!(host.get(ComponentId(4500)).is_none());
    // While a is offscreen an authoritative absence arrives in the global snapshot.
    b.revision = 3;
    b.side_chats.chats.clear();
    panel_mut(&mut host, root).update(&b).unwrap();
    a.revision = 4;
    // Simulate a later new pointer with the same tab header. Its body must not
    // resurrect content or accept a callback from the discarded incarnation.
    panel_mut(&mut host, root).update(&a).unwrap();
    assert!(host.get(ComponentId(4500)).is_none());
    assert!(
        panel_mut(&mut host, root)
            .install_content(
                &token,
                Content::new(ComponentId(4501), TextContent::new("late old result"))
            )
            .is_err()
    );
}
fn activate(host: &mut ContentHost, id: ComponentId, target: ComponentId) -> Vec<SidePanelIntent> {
    host.accessibility_action(target, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    mounted_panel(host, id).take_intents(&actions)
}
#[tokio::test]
async fn exact_kit_composer_instances_survive_tab_and_parent_switches_but_old_install_tokens_do_not()
 {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let parent = RunScope::main("fixture", "a");
    let task = AgentTask {
        id: "task-a".into(),
        scope: parent.clone(),
        name: "Task A".into(),
        status: "running".into(),
        kind: "task".into(),
    };
    let tab = SidePanelTab::task(&task);
    let mut model = input(parent.clone());
    model.tabs.push(tab.clone());
    model.selected = Some(tab.kind.clone());
    let mut view = SidePanel::new(70, SidePanelLabels::default(), 4);
    view.update(&model).unwrap();
    let root = view.id();
    let token = view.content_token(&tab.kind).unwrap();
    let ids = ComposerIds::new(2000);
    let mut composer = Composer::new(ids);
    composer
        .update(&controller.snapshot(), ComposerAffordances::default())
        .unwrap();
    view.install_content(&token, Content::new(ids.root, composer))
        .unwrap();
    let mut host = view.into_host().unwrap();
    host.focus(Some(ids.editor), true);
    host.event(&ContentEvent::Commit {
        text: "中文 pane draft".into(),
        replacement: None,
    });
    let actions = host.drain_actions();
    let intents = host
        .get_mut(ids.root)
        .unwrap()
        .component_mut::<Composer>()
        .unwrap()
        .take_intents(&actions);
    let [ComposerIntent::Edit { destination, text }] = intents.as_slice() else {
        panic!("draft callback");
    };
    controller.edit_text(destination, text.clone()).unwrap();
    host.focus(None, false);
    model.revision = 2;
    model.selected = Some(SidePanelTabKind::Summary);
    panel_mut(&mut host, root).update(&model).unwrap();
    assert!(host.get(ids.root).is_none());
    model.revision = 3;
    model.selected = Some(tab.kind.clone());
    panel_mut(&mut host, root).update(&model).unwrap();
    assert_eq!(
        host.get(ids.root)
            .unwrap()
            .component::<Composer>()
            .unwrap()
            .editor()
            .input
            .text,
        "中文 pane draft"
    );
    let mut b = input(RunScope::main("fixture", "b"));
    b.revision = 4;
    panel_mut(&mut host, root).update(&b).unwrap();
    assert!(host.get(ids.root).is_none());
    model.revision = 5;
    panel_mut(&mut host, root).update(&model).unwrap();
    assert_eq!(
        host.get(ids.root)
            .unwrap()
            .component::<Composer>()
            .unwrap()
            .editor()
            .input
            .text,
        "中文 pane draft"
    );
    assert!(
        panel_mut(&mut host, root)
            .install_content(
                &token,
                Content::new(
                    ComponentId(3000),
                    TextContent::new("late old parent result")
                )
            )
            .is_err()
    );
    runtime.stop().await.unwrap();
}
#[test]
fn sidechat_close_and_create_follow_existing_pending_and_unknown_authority() {
    let parent = RunScope::main("native", "parent");
    let chat = SideChat {
        parent: parent.clone(),
        scope: RunScope::main("native", "ephemeral"),
        title: "Owned SideChat".into(),
        phase: SideChatPhase::Ready,
        error: None,
    };
    let tab = SidePanelTab::side_chat(&chat);
    let mut model = input(parent.clone());
    model.tabs.push(tab.clone());
    model.selected = Some(tab.kind.clone());
    model.side_chats.chats.insert("ephemeral".into(), chat);
    model.supports_side_chat = Some(true);
    let mut view = SidePanel::new(71, SidePanelLabels::default(), 4);
    let root = view.id();
    let close = view.close_id();
    let create = view.create_side_chat_id();
    view.update(&model).unwrap();
    let mut host = view.into_host().unwrap();
    assert_eq!(
        activate(&mut host, root, close),
        vec![SidePanelIntent::Close {
            parent: parent.clone(),
            tab: tab.kind.clone()
        }]
    );
    assert_eq!(
        activate(&mut host, root, create),
        vec![SidePanelIntent::CreateSideChat {
            parent: parent.clone()
        }]
    );
    model.revision = 2;
    model.side_chats.chats.get_mut("ephemeral").unwrap().phase = SideChatPhase::Unknown;
    model.side_chats.unknown_creations.insert(
        serde_json::to_string(&parent).unwrap(),
        "creation outcome unknown".into(),
    );
    panel_mut(&mut host, root).update(&model).unwrap();
    assert!(activate(&mut host, root, close).is_empty());
    assert!(activate(&mut host, root, create).is_empty());
    let mut premature = model.clone();
    premature.revision = 3;
    premature.tabs.pop();
    premature.selected = Some(SidePanelTabKind::Summary);
    assert!(
        panel_mut(&mut host, root).update(&premature).is_err(),
        "unknown discard silently removed pane"
    );
    premature.side_chats.chats.remove("ephemeral");
    panel_mut(&mut host, root).update(&premature).unwrap();
    assert!(mounted_panel(&host, root).tab_id(&tab.kind).is_none());
    premature.revision = 4;
    premature.supports_side_chat = None;
    premature.side_chats.unknown_creations.clear();
    panel_mut(&mut host, root).update(&premature).unwrap();
    assert!(activate(&mut host, root, create).is_empty());
    assert!(
        !host
            .semantics()
            .iter()
            .any(|node| node.semantics.name.contains("不支持临时 SideChat")),
        "unknown capability was presented as unsupported"
    );
}
#[test]
fn child_source_layout_is_explicit_captured_and_invalidates_old_content_on_change() {
    let parent = RunScope::main("native", "parent");
    let tab = SidePanelTab::subagent("bounded", "Child", ChildHistoryLayout::SameSession);
    let mut model = input(parent.clone());
    model.tabs.push(tab.clone());
    model.selected = Some(tab.kind.clone());
    let mut view = SidePanel::new(72, SidePanelLabels::default(), 4);
    let root = view.id();
    view.update(&model).unwrap();
    let id = view.tab_id(&tab.kind).unwrap();
    let token = view.content_token(&tab.kind).unwrap();
    let mut host = view.into_host().unwrap();
    let captured = activate(&mut host, root, id);
    assert_eq!(
        captured,
        vec![SidePanelIntent::Select {
            parent: parent.clone(),
            tab: tab.clone()
        }]
    );
    model.revision = 2;
    model.tabs[1].child_history_layout = Some(ChildHistoryLayout::IndependentSessions);
    panel_mut(&mut host, root).update(&model).unwrap();
    assert!(
        panel_mut(&mut host, root)
            .install_content(
                &token,
                Content::new(ComponentId(3200), TextContent::new("old bounded read"))
            )
            .is_err()
    );
    let [SidePanelIntent::Select { tab: old, .. }] = captured.as_slice() else {
        panic!("child selection");
    };
    assert_eq!(
        old.child_history_layout,
        Some(ChildHistoryLayout::SameSession)
    );
    let mut invalid = model.clone();
    invalid.revision = 3;
    invalid.tabs[1].child_history_layout = None;
    assert!(panel_mut(&mut host, root).update(&invalid).is_err());
}
#[test]
fn duplicate_foreign_task_and_foreign_chat_inputs_fail_before_mutating_mounted_tabs() {
    let parent = RunScope::main("native", "parent");
    let model = input(parent.clone());
    let mut view = SidePanel::new(73, SidePanelLabels::default(), 4);
    view.update(&model).unwrap();
    let summary = view.tab_id(&SidePanelTabKind::Summary).unwrap();
    let mut invalid = model.clone();
    invalid.revision = 2;
    invalid.tabs.push(model.tabs[0].clone());
    assert!(view.update(&invalid).is_err());
    invalid = model.clone();
    invalid.revision = 2;
    invalid.tabs.push(SidePanelTab::task(&AgentTask {
        id: "foreign".into(),
        scope: RunScope::main("pi", "other"),
        name: "foreign".into(),
        status: "running".into(),
        kind: "task".into(),
    }));
    assert!(view.update(&invalid).is_err());
    invalid = model.clone();
    invalid.revision = 2;
    invalid.tabs.push(SidePanelTab::side_chat(&SideChat {
        parent: RunScope::main("native", "other"),
        scope: RunScope::main("native", "ephemeral"),
        title: "foreign".into(),
        phase: SideChatPhase::Ready,
        error: None,
    }));
    assert!(view.update(&invalid).is_err());
    assert_eq!(view.tab_id(&SidePanelTabKind::Summary), Some(summary));
}
#[test]
fn removed_and_reopened_tabs_and_evicted_parent_views_reject_old_mounts() {
    let mut model = input(RunScope::main("native", "a"));
    let mut view = SidePanel::new(74, SidePanelLabels::default(), 1);
    view.update(&model).unwrap();
    let token = view.content_token(&SidePanelTabKind::Summary).unwrap();
    model.revision = 2;
    model.tabs.clear();
    model.selected = None;
    view.update(&model).unwrap();
    model.revision = 3;
    model.tabs.push(SidePanelTab::summary("Summary"));
    model.selected = Some(SidePanelTabKind::Summary);
    view.update(&model).unwrap();
    assert!(
        view.install_content(
            &token,
            Content::new(ComponentId(3400), TextContent::new("closed tab result"))
        )
        .is_err()
    );
    let a = view.content_token(&SidePanelTabKind::Summary).unwrap();
    let mut other = input(RunScope::main("native", "b"));
    other.revision = 4;
    view.update(&other).unwrap();
    model.revision = 5;
    view.update(&model).unwrap();
    assert!(
        view.install_content(
            &a,
            Content::new(ComponentId(3401), TextContent::new("evicted parent result"))
        )
        .is_err()
    );
}

#[test]
fn task_identity_is_stable_while_shared_resolution_retires_changed_or_missing_session_content() {
    let parent = RunScope::main("native", "parent");
    let mut state = TaskPanelState::default();
    let first = state.begin(parent.clone(), "task").unwrap();
    let task = |session: &str| AgentTask {
        id: "task".into(),
        scope: RunScope {
            endpoint_id: "native".into(),
            thread_id: "parent".into(),
            session_id: session.into(),
        },
        name: "Producer task name".into(),
        status: "running".into(),
        kind: "agent".into(),
    };
    assert!(state.commit(&first, Ok(Some(task("old-session")))));
    let tab = SidePanelTab::task_from_state(&state, "Loading task").unwrap();
    let mut model = input(parent.clone());
    model.tabs.push(tab.clone());
    model.selected = Some(tab.kind.clone());
    let mut panel = SidePanel::new(75, SidePanelLabels::default(), 4);
    panel.update(&model).unwrap();
    let root = panel.id();
    let header_id = panel.tab_id(&tab.kind).unwrap();
    let old_mount = panel.content_token(&tab.kind).unwrap();
    panel
        .install_content(
            &old_mount,
            Content::new(ComponentId(3600), TextContent::new("Old source body")),
        )
        .unwrap();
    let mut host = panel.into_host().unwrap();
    let next = state.begin(parent.clone(), "task").unwrap();
    model.revision = 2;
    model.tabs[1] = SidePanelTab::task_from_state(&state, "Loading task").unwrap();
    panel_mut(&mut host, root).update(&model).unwrap();
    assert_eq!(model.tabs[1].kind, tab.kind);
    assert_eq!(
        mounted_panel(&host, root).tab_id(&tab.kind),
        Some(header_id)
    );
    assert!(host.get(ComponentId(3600)).is_none());
    assert!(
        mounted_panel(&host, root)
            .content_token(&tab.kind)
            .is_none()
    );
    assert!(!state.commit(&first, Ok(Some(task("old-session")))));
    assert!(state.commit(&next, Ok(Some(task("new-session")))));
    model.revision = 3;
    model.tabs[1] = SidePanelTab::task_from_state(&state, "Loading task").unwrap();
    panel_mut(&mut host, root).update(&model).unwrap();
    assert_eq!(
        mounted_panel(&host, root).tab_id(&tab.kind),
        Some(header_id)
    );
    assert_eq!(
        model.tabs[1].task_scope.as_ref().unwrap().session_id,
        "new-session"
    );
    assert!(
        panel_mut(&mut host, root)
            .install_content(
                &old_mount,
                Content::new(ComponentId(3601), TextContent::new("Late old pointer"))
            )
            .is_err()
    );
    let missing = state.begin(parent, "task").unwrap();
    assert!(state.commit(&missing, Ok(None)));
    model.revision = 4;
    model.tabs[1] = SidePanelTab::task_from_state(&state, "Loading task").unwrap();
    panel_mut(&mut host, root).update(&model).unwrap();
    assert_eq!(
        mounted_panel(&host, root).tab_id(&tab.kind),
        Some(header_id)
    );
    assert!(
        mounted_panel(&host, root)
            .content_token(&tab.kind)
            .is_none()
    );
    assert!(
        host.semantics()
            .iter()
            .any(|node| node.semantics.name == "任务 Run 暂不可用")
    );
}
