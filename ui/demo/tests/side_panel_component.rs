use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tekes_agent_components_ui::*;
use tekes_agent_presentation::*;
use tekes_ui_controls::composition::*;
#[derive(Debug)]
struct PaneLifetime {
    children: Vec<Content>,
    dropped: Arc<AtomicBool>,
}
impl Drop for PaneLifetime {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}
impl Component for PaneLifetime {
    fn measure(
        &mut self,
        text: &mut dyn tekes_ui_controls::ButtonTextRenderer,
        available: tekes_ui_scene::Size,
    ) -> tekes_ui_controls::layout::MeasuredChild {
        self.children[0].measure(text, available)
    }
    fn arrange(
        &mut self,
        text: &mut dyn tekes_ui_controls::ButtonTextRenderer,
        frame: tekes_ui_scene::Rect,
        clip: tekes_ui_scene::Rect,
    ) {
        self.children[0].arrange(text, frame, clip);
    }
    fn children(&self) -> &[Content] {
        &self.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.children)
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, "owned pane")
    }
}
fn input(parent: RunScope) -> SidePanelInput {
    SidePanelInput {
        revision: 1,
        parent,
        projection_ready: true,
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
fn nested_child_parent_is_content_identity_and_foreign_parents_cannot_replace_it() {
    let parent = RunScope::main("fixture", "a");
    let mut model = input(parent.clone());
    let mut tab =
        SidePanelTab::subagent("nested", "Nested", ChildHistoryLayout::IndependentSessions);
    tab.child_parent = Some(parent.clone());
    model.tabs.push(tab.clone());
    model.selected = Some(tab.kind.clone());
    let mut panel = SidePanel::new(74, SidePanelLabels::default(), 4);
    panel.update(&model).unwrap();
    let token = panel.content_token(&tab.kind).unwrap();
    panel
        .install_content(
            &token,
            Content::new(ComponentId(4600), TextContent::new("old receiving parent")),
        )
        .unwrap();
    let root = panel.id();
    let mut host = panel.into_host().unwrap();
    model.revision = 2;
    model.tabs[1].child_parent = Some(RunScope {
        session_id: "direct-parent".into(),
        ..parent.clone()
    });
    panel_mut(&mut host, root).update(&model).unwrap();
    assert!(host.get(ComponentId(4600)).is_none());
    assert!(
        panel_mut(&mut host, root)
            .install_content(
                &token,
                Content::new(ComponentId(4601), TextContent::new("late stale body"))
            )
            .is_err()
    );
    model.revision = 3;
    model.tabs[1].child_parent = Some(RunScope::main("fixture", "foreign-thread"));
    assert!(panel_mut(&mut host, root).update(&model).is_err());
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
    let mut b = input(RunScope {
        session_id: "b".into(),
        ..a.parent.clone()
    });
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
async fn same_thread_tabs_retain_the_editor_but_thread_switch_retires_it_and_keeps_the_presentation_draft()
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
    let dropped = Arc::new(AtomicBool::new(false));
    view.install_content(
        &token,
        Content::new(
            ComponentId(1999),
            PaneLifetime {
                children: vec![Content::new(ids.root, composer)],
                dropped: dropped.clone(),
            },
        ),
    )
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
    assert!(
        !dropped.load(Ordering::SeqCst),
        "same-thread tab switch retains its exact UI"
    );
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
    assert!(
        dropped.load(Ordering::SeqCst),
        "changing thread evicts cached pane UI"
    );
    model.revision = 5;
    panel_mut(&mut host, root).update(&model).unwrap();
    assert!(
        host.get(ids.root).is_none(),
        "returning does not resurrect the retired editor"
    );
    let fresh = panel_mut(&mut host, root).content_token(&tab.kind).unwrap();
    let fresh_ids = ComposerIds::new(6000);
    let mut composer = Composer::new(fresh_ids);
    composer
        .update(&controller.snapshot(), ComposerAffordances::default())
        .unwrap();
    panel_mut(&mut host, root)
        .install_content(&fresh, Content::new(fresh_ids.root, composer))
        .unwrap();
    assert_eq!(
        host.get(fresh_ids.root)
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
    let mut other = input(RunScope {
        session_id: "b".into(),
        ..model.parent.clone()
    });
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
fn awaiting_projection_retires_all_content_without_empty_or_stale_body_and_never_revives_old_tokens()
 {
    let mut model = input(RunScope::main("fixture", "a"));
    let child = SidePanelTab::subagent("child", "Child", ChildHistoryLayout::IndependentSessions);
    model.tabs.push(child.clone());
    model.supports_side_chat = Some(true);
    let mut view = SidePanel::new(76, Default::default(), 4);
    view.update(&model).unwrap();
    let root = view.id();
    let summary = view.content_token(&SidePanelTabKind::Summary).unwrap();
    let child_token = view.content_token(&child.kind).unwrap();
    let header = view.tab_id(&child.kind).unwrap();
    let first = Arc::new(AtomicBool::new(false));
    let second = Arc::new(AtomicBool::new(false));
    for (token, id, flag) in [
        (&summary, 5000, first.clone()),
        (&child_token, 5100, second.clone()),
    ] {
        view.install_content(
            token,
            Content::new(
                ComponentId(id),
                PaneLifetime {
                    children: vec![Content::new(
                        ComponentId(id + 1),
                        TextContent::new("canonical pane"),
                    )],
                    dropped: flag,
                },
            ),
        )
        .unwrap();
    }
    let mut host = view.into_host().unwrap();
    let create = mounted_panel(&host, root).create_side_chat_id();
    model.revision = 2;
    model.projection_ready = false;
    panel_mut(&mut host, root).update(&model).unwrap();
    assert!(first.load(Ordering::SeqCst) && second.load(Ordering::SeqCst));
    let names: Vec<_> = host
        .semantics()
        .into_iter()
        .map(|node| node.semantics.name)
        .collect();
    assert!(!names.iter().any(|name| matches!(
        name.as_str(),
        "canonical pane" | "未打开标签" | "正在准备面板内容"
    )));
    assert!(
        mounted_panel(&host, root)
            .content_token(&SidePanelTabKind::Summary)
            .is_none()
    );
    assert!(
        mounted_panel(&host, root)
            .content_token(&child.kind)
            .is_none()
    );
    assert!(activate(&mut host, root, create).is_empty());
    assert!(
        matches!(
            activate(&mut host, root, header).as_slice(),
            [SidePanelIntent::Select { .. }]
        ),
        "navigation metadata remains usable without granting body authority"
    );
    model.revision = 3;
    model.projection_ready = true;
    panel_mut(&mut host, root).update(&model).unwrap();
    assert!(
        panel_mut(&mut host, root)
            .install_content(
                &summary,
                Content::new(ComponentId(5200), TextContent::new("late result"))
            )
            .is_err()
    );
    let fresh = mounted_panel(&host, root)
        .content_token(&SidePanelTabKind::Summary)
        .unwrap();
    panel_mut(&mut host, root)
        .install_content(
            &fresh,
            Content::new(ComponentId(5300), TextContent::new("fresh parent")),
        )
        .unwrap();
    assert!(host.get(ComponentId(5300)).is_some());
    let mut conflict = model.clone();
    conflict.projection_ready = false;
    assert!(panel_mut(&mut host, root).update(&conflict).is_err());
    assert!(
        host.get(ComponentId(5300)).is_some(),
        "a conflicting revision must not mutate accepted content"
    );
}

#[tokio::test]
async fn second_host_passes_parent_projection_explicitly_without_treating_it_as_a_connection_state()
{
    let (runtime, _) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let parent = tekes_agent_ui_bindings::RunBinding::open_main(
        runtime.clone(),
        RunScope::main("fixture", "a"),
    )
    .await
    .unwrap();
    let owner = tekes_agent_ui_bindings::SideChatBinding::new(runtime.clone());
    let mut view = SidePanel::new(77, Default::default(), 4);
    tekes_agent_ui_consumer_check::sync_scratch_tabs_with_projection(
        &mut view, &owner, &parent, 1, false,
    )
    .unwrap();
    assert!(!view.displayed_input().unwrap().projection_ready);
    assert!(view.content_token(&SidePanelTabKind::Summary).is_none());
    tekes_agent_ui_consumer_check::sync_scratch_tabs_with_projection(
        &mut view, &owner, &parent, 2, true,
    )
    .unwrap();
    assert!(view.content_token(&SidePanelTabKind::Summary).is_some());
    runtime.stop().await.unwrap();
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

struct PanelText;
impl tekes_ui_controls::ButtonTextRenderer for PanelText {
    fn measure(
        &mut self,
        s: &str,
        _: tekes_ui_tokens::proposal::TextRole,
        _: f32,
    ) -> tekes_ui_scene::Size {
        tekes_ui_scene::Size::new(s.chars().count() as f32 * 8., 20.)
    }
    fn draw(
        &mut self,
        _: &mut tekes_ui_scene::Scene,
        _: &str,
        _: tekes_ui_tokens::proposal::TextRole,
        _: tekes_ui_scene::Rect,
        _: tekes_ui_controls::ButtonTextAlign,
        _: tekes_ui_tokens::Rgba,
    ) {
    }
}
#[test]
fn host_clearance_and_regular_body_inset_preserve_content_and_reject_invalid_layout() {
    use tekes_ui_scene::{Rect, Size};
    let mut panel = SidePanel::new(96, Default::default(), 2);
    let model = input(RunScope::main("fixture", "a"));
    panel.update(&model).unwrap();
    let token = panel.content_token(&SidePanelTabKind::Summary).unwrap();
    panel
        .install_content(
            &token,
            Content::new(ComponentId(9900), TextContent::new("canonical body")),
        )
        .unwrap();
    let root = panel.id();
    let mut host = panel.into_host().unwrap();
    let pane = Rect::new(80., 40., 420., 620.);
    let mut text = PanelText;
    host.get_mut(root).unwrap().measure(&mut text, pane.size);
    host.get_mut(root).unwrap().arrange(&mut text, pane, pane);
    let initial = mounted_panel(&host, root).layout_geometry().unwrap();
    let owned = host.get(ComponentId(9900)).unwrap() as *const _ as usize;
    tekes_agent_ui_consumer_check::set_panel_layout(panel_mut(&mut host, root), 32.).unwrap();
    assert!(mounted_panel(&host, root).layout_geometry().is_none());
    for invalid in [-1., f32::NAN, f32::INFINITY] {
        assert!(
            tekes_agent_ui_consumer_check::set_panel_layout(panel_mut(&mut host, root), invalid)
                .is_err()
        );
    }
    host.get_mut(root).unwrap().measure(&mut text, pane.size);
    host.get_mut(root).unwrap().arrange(&mut text, pane, pane);
    let layout = mounted_panel(&host, root).layout_geometry().unwrap();
    assert_eq!(layout.canvas.min_y(), pane.min_y() + 32.);
    assert_eq!(layout.body.min_y(), initial.body.min_y() + 32.);
    assert_eq!(layout.body.max_y(), initial.body.max_y());
    let slot = host
        .get(mounted_panel(&host, root).content_slot_id())
        .unwrap()
        .frame();
    let inset = tekes_ui_tokens::Spacing::Regular.value();
    assert_eq!(layout.body.min_x(), slot.min_x() + inset);
    assert_eq!(layout.body.max_x(), slot.max_x() - inset);
    let child_frame = host.get(ComponentId(9900)).unwrap().frame();
    assert_eq!(child_frame.min_x(), layout.body.min_x());
    assert_eq!(child_frame.min_y(), layout.body.min_y());
    assert!(child_frame.max_y() <= layout.body.max_y());
    assert_eq!(
        host.get(ComponentId(9900)).unwrap() as *const _ as usize,
        owned
    );
    // Pure layout inputs do not replace content/mount authority.
    panel_mut(&mut host, root)
        .install_content(
            &token,
            Content::new(ComponentId(9901), TextContent::new("same mount")),
        )
        .unwrap();
    assert!(host.get(ComponentId(9901)).is_some());
    tekes_agent_ui_consumer_check::set_panel_layout(panel_mut(&mut host, root), 10_000.).unwrap();
    host.get_mut(root).unwrap().arrange(&mut text, pane, pane);
    let collapsed = mounted_panel(&host, root).layout_geometry().unwrap();
    assert_eq!(collapsed.canvas.size.height, 0.);
    assert_eq!(collapsed.visible_body, None);
    assert!(collapsed.body.size.width >= 0. && collapsed.body.size.height >= 0.);
    assert_eq!(
        host.get_mut(root)
            .unwrap()
            .measure(&mut text, Size::new(420., 620.))
            .size,
        Size::new(420., 620.)
    );
}
