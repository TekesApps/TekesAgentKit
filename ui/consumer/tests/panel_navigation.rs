//! Independent controlled host: navigation policy belongs to the host, pane
//! mounting and retired-content fencing remain in the public AgentKit component.
use tekes_agent_components_ui::{
    RunScope, SidePanel, SidePanelInput, SidePanelIntent, SidePanelTab, SidePanelTabKind,
};
use tekes_ui_controls::composition::*;

#[test]
fn close_empty_and_reopen_use_original_controls_and_retire_the_original_body() {
    let mut panel = SidePanel::new(63000, Default::default(), 4);
    let root = panel.id();
    let close = panel.close_id();
    let summary = SidePanelTabKind::Summary;
    let mut input = SidePanelInput {
        revision: 1,
        parent: RunScope::main("independent", "a"),
        projection_ready: true,
        tabs: vec![SidePanelTab::summary("Summary")],
        selected: Some(summary.clone()),
        side_chats: Default::default(),
        supports_side_chat: None,
    };
    panel.update(&input).unwrap();
    let old = panel.content_token(&summary).unwrap();
    let body = ComponentId(74000u64 << 32);
    panel
        .install_content(&old, Content::new(body, TextContent::new("original body")))
        .unwrap();
    let mut host = panel.into_host().unwrap();
    assert!(host.get(body).is_some());
    assert!(
        host.accessibility_action(close, None, AccessibilityAction::Press)
            .activated
    );
    // Drain/capture the original callback before changing the controlled input.
    let actions = host.drain_actions();
    let intents = host
        .get(root)
        .unwrap()
        .component::<SidePanel>()
        .unwrap()
        .take_intents(&actions);
    assert_eq!(
        intents,
        vec![SidePanelIntent::Close {
            parent: input.parent.clone(),
            tab: summary.clone()
        }]
    );
    input.revision += 1;
    input.tabs.clear();
    input.selected = None;
    host.get_mut(root)
        .unwrap()
        .component_mut::<SidePanel>()
        .unwrap()
        .update(&input)
        .unwrap();
    assert!(host.get(body).is_none());
    let panel = host.get(root).unwrap().component::<SidePanel>().unwrap();
    assert!(panel.content_token(&summary).is_none());
    assert!(panel.displayed_input().unwrap().tabs.is_empty());
    assert!(
        !host
            .accessibility_action(close, None, AccessibilityAction::Press)
            .activated
    );
    assert!(host.drain_actions().is_empty());

    // Explicit host navigation reopens Summary. It cannot revive a closed mount.
    input.revision += 1;
    input.tabs.push(SidePanelTab::summary("Summary"));
    input.selected = Some(summary.clone());
    let panel = host
        .get_mut(root)
        .unwrap()
        .component_mut::<SidePanel>()
        .unwrap();
    panel.update(&input).unwrap();
    assert!(
        panel
            .install_content(&old, Content::new(body, TextContent::new("late old body")))
            .is_err()
    );
    let fresh = panel.content_token(&summary).unwrap();
    let next = ComponentId(body.0 + 1);
    panel
        .install_content(&fresh, Content::new(next, TextContent::new("fresh body")))
        .unwrap();
    assert!(host.get(body).is_none());
    assert!(host.get(next).is_some());
    assert!(
        host.get(root)
            .unwrap()
            .component::<SidePanel>()
            .unwrap()
            .displayed_input()
            .unwrap()
            .selected
            == Some(summary)
    );
}
