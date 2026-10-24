//! Production demo routing, not a bypass of its composition guard.
#![cfg(target_os = "macos")]
#[allow(dead_code)]
#[path = "support/menu_host.rs"]
mod paint;
use std::sync::Arc;
use tekes_agent_components_ui::*;
use tekes_agent_ui_bindings::RunBinding;
use tekes_agent_ui_demo::{component_demo::ComponentDemo, fixture};
use tekes_ui_controls::composition::*;

#[tokio::test]
async fn demo_allows_only_original_summary_disclosure_while_a_separate_composer_is_marked() {
    let (runtime, _) = fixture::setup().await.unwrap();
    let binding = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let mut demo = ComponentDemo::new([binding.clone()]).unwrap();
    let revision = demo
        .tree
        .get(demo.summary_id())
        .unwrap()
        .component::<SummaryView>()
        .unwrap()
        .displayed_context_input()
        .unwrap()
        .revision
        + 1;
    let source = SummaryContextInput {
        revision,
        owner: SummaryOwner::Workspace {
            id: "focus-workspace".into(),
        },
        facts: None,
        ready: true,
        custom_rows: vec![],
        host_sections: vec![SummarySection {
            id: "host:focus".into(),
            rows: vec![SummaryRow {
                id: "status".into(),
                title: "宿主工作区状态".into(),
                value: Default::default(),
                action: None,
            }],
        }],
    };
    demo.set_workspace_summary(source.clone()).unwrap();
    paint::paint(&mut demo.tree);
    let root = demo.summary_id();
    let header = demo
        .tree
        .get(root)
        .unwrap()
        .component::<SummaryView>()
        .unwrap()
        .section_header_id("host:focus")
        .unwrap();
    let editor = demo.ids().editor;
    assert!(demo.tree.focus(Some(editor), true));
    demo.event(&ContentEvent::Preedit {
        text: "原生组字路由".into(),
        selection: 1..3,
        replacement: None,
        clauses: vec![],
    })
    .unwrap();
    paint::paint(&mut demo.tree);
    let baseline = demo.editor().input.external_baseline();
    assert!(demo.editor().input.marked.is_some());
    let p = demo.tree.get(header).unwrap().visible().unwrap().center();
    assert!(
        demo.event(&ContentEvent::PointerDown(p))
            .unwrap()
            .is_empty()
    );
    assert!(demo.event(&ContentEvent::PointerUp(p)).unwrap().is_empty());
    assert_eq!(
        demo.tree
            .get(root)
            .unwrap()
            .component::<SummaryView>()
            .unwrap()
            .section_expanded("host:focus"),
        Some(false)
    );
    paint::paint(&mut demo.tree);
    assert!(
        demo.accessibility_action(header, None, AccessibilityAction::Press)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        demo.tree
            .get(root)
            .unwrap()
            .component::<SummaryView>()
            .unwrap()
            .section_expanded("host:focus"),
        Some(true)
    );
    paint::paint(&mut demo.tree);
    demo.accessibility_action(header, None, AccessibilityAction::Focus)
        .unwrap();
    demo.accessibility_action(demo.sidebar_id(), None, AccessibilityAction::Focus)
        .unwrap();
    assert_eq!(demo.tree.focused(), Some(editor));
    assert_eq!(demo.editor().input.external_baseline(), baseline);
    let mut unavailable = source;
    unavailable.revision += 1;
    unavailable.host_sections.clear();
    unavailable.ready = false;
    demo.set_workspace_summary(unavailable).unwrap();
    paint::paint(&mut demo.tree);
    assert!(demo.tree.get(header).is_none());
    demo.accessibility_action(header, None, AccessibilityAction::Press)
        .unwrap();
    demo.event(&ContentEvent::PointerDown(p)).unwrap();
    assert_eq!(demo.tree.focused(), Some(editor));
    assert_eq!(demo.editor().input.external_baseline(), baseline);
    assert!(binding.current_run().unwrap().submissions.is_empty());
}
