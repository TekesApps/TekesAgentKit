use std::{sync::Arc, time::Duration};
use tekes_agent_components_ui::*;
use tekes_agent_ui_bindings::{RunBinding, SidebarBinding};
use tekes_agent_ui_demo::{
    component_demo::{ComponentDemo, DemoCommand},
    fixture::Fixture,
};
use tekes_agent_web::SharedAgent;
use tekes_ui_controls::composition::*;
async fn until(source: &SidebarBinding, predicate: impl Fn() -> bool) {
    let mut changes = source.subscribe();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !predicate() {
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn original_pin_membership_and_durable_layout_preferences_reach_both_consumers_without_navigation()
 {
    let directory = std::env::temp_dir().join(format!(
        "agentkit-sidebar-layout-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut agent = SharedAgent::default();
    agent
        .open_thread_preferences(&directory.join("preferences.json"), "isolated-test")
        .unwrap();
    agent
        .register(
            "fixture".into(),
            "native".into(),
            "Fixture".into(),
            Arc::new(Fixture::default()),
        )
        .unwrap();
    let agent = Arc::new(agent);
    let runtime = agent.endpoint("fixture").unwrap().runtime.clone();
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let binding = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    binding
        .edit_text(
            &ComposerDestination::run(binding.scope().clone()),
            "置顶操作保留的草稿".into(),
        )
        .unwrap();
    let source = SidebarBinding::new(agent.clone(), "fixture").unwrap();
    until(&source, || source.snapshot().loaded).await;
    for id in ["a", "b"] {
        let frame = source.snapshot();
        let row = frame
            .rows
            .iter()
            .find(|r| r.address.session_id == id)
            .unwrap();
        let pin = row.actions.items.iter().find(|a| a.id == "pin").unwrap();
        source
            .perform(&row.address, pin, None, &format!("pin-{id}"))
            .await
            .unwrap();
        until(&source, || {
            source
                .snapshot()
                .rows
                .iter()
                .any(|r| r.address.session_id == id && r.pin.as_ref().is_some_and(|p| p.pinned))
        })
        .await;
    }
    let mut view = ComponentDemo::new([binding]).unwrap();
    view.set_sidebar_source(source.snapshot()).unwrap();
    view.set_sidebar_preferences(&source.preferences().unwrap(), true)
        .unwrap();
    view.sync().unwrap();
    let root = view.sidebar_id();
    let address = view.binding().scope().address();
    let sidebar = view.tree.get(root).unwrap().component::<Sidebar>().unwrap();
    let row = sidebar.row_id(&address).unwrap();
    let widget = view
        .tree
        .get(row)
        .unwrap()
        .component::<ButtonContent>()
        .unwrap() as *const _ as usize;
    let moved = sidebar.move_pin_change(&address, 1).unwrap();
    let revision = source.snapshot().revision;
    source.set_preferences(&moved).unwrap();
    until(&source, || source.snapshot().revision > revision).await;
    assert!(source.set_preferences(&moved).is_err());
    view.set_sidebar_preferences(&source.preferences().unwrap(), true)
        .unwrap();
    view.set_sidebar_source(source.snapshot()).unwrap();
    view.sync().unwrap();
    let mut second = Sidebar::new(997, SidebarLabels::default());
    tekes_agent_ui_consumer_check::sync_observed_sidebar(&mut second, &source).unwrap();
    assert_eq!(second.preferences(), &source.preferences().unwrap());
    let source_frame = source.snapshot();
    let rows = ordered_sidebar_rows(
        &source_frame.rows,
        &source.preferences().unwrap().preferences,
        None,
    )
    .unwrap();
    assert_eq!(rows[0].address.session_id, "b");
    assert_eq!(rows[1].address.session_id, "a");
    let header = view
        .tree
        .get(root)
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .pinned_header_id()
        .unwrap();
    let commands = view
        .accessibility_action(header, None, AccessibilityAction::Press)
        .unwrap();
    let [DemoCommand::SidebarPreferences(change)] = commands.as_slice() else {
        panic!("original section callback")
    };
    source.set_preferences(change).unwrap();
    assert!(!source.preferences().unwrap().preferences.pinned_expanded);
    view.set_sidebar_preferences(&source.preferences().unwrap(), true)
        .unwrap();
    assert_eq!(
        view.tree
            .get(row)
            .unwrap()
            .component::<ButtonContent>()
            .unwrap() as *const _ as usize,
        widget
    );
    assert_eq!(view.editor().input.text, "置顶操作保留的草稿");
    assert_eq!(view.selected(), 0);
    assert!(view.binding().current_run().unwrap().submissions.is_empty());
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("preferences.json")).unwrap())
            .unwrap();
    assert_eq!(saved["version"], 3);
    assert_eq!(
        saved["scopes"]["isolated-test"]["sidebar"]["preferences"]["pinnedExpanded"],
        false
    );
    drop(view);
    drop(source);
    runtime.stop().await.unwrap();
    drop(agent);
    // Only this test's owned temporary data is removed; no installed preferences.
    std::fs::remove_dir_all(directory).unwrap();
}
