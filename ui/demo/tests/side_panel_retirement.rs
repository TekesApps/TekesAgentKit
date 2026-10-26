use tekes_agent_components_ui::*;
use tekes_agent_presentation::*;
use tekes_ui_controls::composition::*;

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

fn install_text(panel: &mut SidePanel, token: &PanelContentToken, id: u64, label: &str) {
    panel
        .install_content(
            token,
            Content::new(ComponentId(id), TextContent::new(label)),
        )
        .unwrap();
}

fn task_tab(parent: &RunScope, task_id: &str) -> SidePanelTab {
    SidePanelTab::task(&AgentTask {
        id: task_id.into(),
        scope: RunScope {
            session_id: format!("{task_id}-session"),
            ..parent.clone()
        },
        name: task_id.into(),
        status: "running".into(),
        kind: "task".into(),
    })
}

#[test]
fn retiring_active_content_drops_the_tree_rejects_old_token_and_accepts_fresh_token() {
    let mut model = input(RunScope::main("fixture", "parent"));
    let task = task_tab(&model.parent, "task");
    model.tabs.push(task.clone());
    model.selected = Some(task.kind.clone());

    let mut panel = SidePanel::new(130, SidePanelLabels::default(), 2);
    panel.update(&model).unwrap();
    let root = panel.id();
    let header = panel.tab_id(&task.kind).unwrap();
    let stale = panel.content_token(&task.kind).unwrap();
    install_text(&mut panel, &stale, 8000, "old task subtree");
    let mut host = panel.into_host().unwrap();

    assert!(host.get(ComponentId(8000)).is_some());
    model.revision = 2;
    model.tabs[1].title = "Task with updated facts".into();
    panel_mut(&mut host, root).update(&model).unwrap();
    assert!(host.get(ComponentId(8000)).is_some());
    assert!(panel_mut(&mut host, root).retire_content(&stale).is_ok());
    assert!(host.get(ComponentId(8000)).is_none());
    assert_eq!(mounted_panel(&host, root).tab_id(&task.kind), Some(header));
    assert!(
        host.semantics()
            .iter()
            .any(|node| node.semantics.name == SidePanelLabels::default().awaiting_content)
    );
    assert!(panel_mut(&mut host, root).retire_content(&stale).is_err());
    assert!(
        panel_mut(&mut host, root)
            .install_content(
                &stale,
                Content::new(ComponentId(8001), TextContent::new("late old subtree")),
            )
            .is_err()
    );

    let fresh = mounted_panel(&host, root)
        .content_token(&task.kind)
        .unwrap();
    install_text(
        panel_mut(&mut host, root),
        &fresh,
        8002,
        "fresh task subtree",
    );
    assert!(host.get(ComponentId(8002)).is_some());
}

#[test]
fn retiring_inactive_content_preserves_other_tab_and_cannot_resurrect_when_selected() {
    let mut model = input(RunScope::main("fixture", "parent"));
    let task = task_tab(&model.parent, "task");
    model.tabs.push(task.clone());
    let mut panel = SidePanel::new(131, SidePanelLabels::default(), 2);
    panel.update(&model).unwrap();
    let root = panel.id();
    let summary_kind = SidePanelTabKind::Summary;
    let summary = panel.content_token(&summary_kind).unwrap();
    let inactive = panel.content_token(&task.kind).unwrap();
    install_text(&mut panel, &summary, 8100, "summary subtree");
    install_text(&mut panel, &inactive, 8101, "inactive task subtree");
    let mut host = panel.into_host().unwrap();

    assert!(host.get(ComponentId(8100)).is_some());
    assert!(host.get(ComponentId(8101)).is_none());
    assert!(
        !host
            .semantics()
            .iter()
            .any(|node| node.semantics.name == "inactive task subtree")
    );
    assert!(panel_mut(&mut host, root).retire_content(&inactive).is_ok());
    assert!(host.get(ComponentId(8100)).is_some());
    assert!(host.get(ComponentId(8101)).is_none());
    assert!(
        panel_mut(&mut host, root)
            .install_content(
                &inactive,
                Content::new(ComponentId(8102), TextContent::new("late inactive subtree")),
            )
            .is_err()
    );

    // The other tab's token remains live after a scoped retirement.
    install_text(
        panel_mut(&mut host, root),
        &summary,
        8103,
        "updated summary subtree",
    );
    assert!(host.get(ComponentId(8103)).is_some());
    model.revision = 2;
    model.selected = Some(task.kind.clone());
    panel_mut(&mut host, root).update(&model).unwrap();
    assert!(host.get(ComponentId(8101)).is_none());
    assert!(host.get(ComponentId(8102)).is_none());
    assert!(
        !host
            .semantics()
            .iter()
            .any(|node| node.semantics.name == "inactive task subtree")
    );
    assert!(
        host.semantics()
            .iter()
            .any(|node| node.semantics.name == SidePanelLabels::default().awaiting_content)
    );
}

#[test]
fn foreign_panel_tokens_are_rejected_even_for_same_namespace_and_have_no_effect() {
    let model = input(RunScope::main("fixture", "parent"));
    let mut first = SidePanel::new(132, SidePanelLabels::default(), 2);
    let mut second = SidePanel::new(132, SidePanelLabels::default(), 2);
    first.update(&model).unwrap();
    second.update(&model).unwrap();
    let first_token = first.content_token(&SidePanelTabKind::Summary).unwrap();
    let second_token = second.content_token(&SidePanelTabKind::Summary).unwrap();
    install_text(&mut first, &first_token, 8200, "first panel content");
    install_text(&mut second, &second_token, 8201, "second panel content");
    let first_root = first.id();
    let second_root = second.id();
    let mut first_host = first.into_host().unwrap();
    let mut second_host = second.into_host().unwrap();

    assert!(
        panel_mut(&mut second_host, second_root)
            .retire_content(&first_token)
            .is_err()
    );
    assert!(
        panel_mut(&mut second_host, second_root)
            .install_content(
                &first_token,
                Content::new(ComponentId(8203), TextContent::new("foreign content")),
            )
            .is_err()
    );
    assert!(second_host.get(ComponentId(8203)).is_none());
    assert!(second_host.get(ComponentId(8201)).is_some());
    assert!(second_host.get(ComponentId(8200)).is_none());
    assert!(first_host.get(ComponentId(8200)).is_some());
    assert!(
        panel_mut(&mut first_host, first_root)
            .retire_content(&first_token)
            .is_ok()
    );
    assert!(first_host.get(ComponentId(8200)).is_none());
    // A rejected foreign token did not invalidate the rightful owner's token.
    install_text(
        panel_mut(&mut second_host, second_root),
        &second_token,
        8202,
        "second panel still mounted",
    );
    assert!(second_host.get(ComponentId(8202)).is_some());
}

#[test]
fn retirement_requires_current_context_and_also_works_without_custom_content() {
    let model = input(RunScope::main("fixture", "a"));
    let mut panel = SidePanel::new(133, SidePanelLabels::default(), 2);
    panel.update(&model).unwrap();
    let root = panel.id();
    let old_context = panel.content_token(&SidePanelTabKind::Summary).unwrap();
    install_text(&mut panel, &old_context, 8300, "old parent content");
    let mut host = panel.into_host().unwrap();

    let mut next_parent = input(RunScope::main("fixture", "b"));
    next_parent.revision = 2;
    panel_mut(&mut host, root).update(&next_parent).unwrap();
    let current = mounted_panel(&host, root)
        .content_token(&SidePanelTabKind::Summary)
        .unwrap();
    install_text(
        panel_mut(&mut host, root),
        &current,
        8301,
        "current parent content",
    );
    assert!(
        panel_mut(&mut host, root)
            .retire_content(&old_context)
            .is_err()
    );
    assert!(host.get(ComponentId(8301)).is_some());

    assert!(panel_mut(&mut host, root).retire_content(&current).is_ok());
    assert!(host.get(ComponentId(8301)).is_none());
    let no_custom = mounted_panel(&host, root)
        .content_token(&SidePanelTabKind::Summary)
        .unwrap();
    // A live token can retire its mount even when no custom subtree was installed.
    assert!(
        panel_mut(&mut host, root)
            .retire_content(&no_custom)
            .is_ok()
    );
    assert!(
        panel_mut(&mut host, root)
            .retire_content(&no_custom)
            .is_err()
    );
    next_parent.revision = 3;
    next_parent.selected = None;
    panel_mut(&mut host, root).update(&next_parent).unwrap();
    next_parent.revision = 4;
    next_parent.selected = Some(SidePanelTabKind::Summary);
    panel_mut(&mut host, root).update(&next_parent).unwrap();
    assert!(host.get(ComponentId(8300)).is_none());
    assert!(host.get(ComponentId(8301)).is_none());
    assert!(
        panel_mut(&mut host, root)
            .install_content(
                &no_custom,
                Content::new(ComponentId(8302), TextContent::new("stale no custom token")),
            )
            .is_err()
    );
}
