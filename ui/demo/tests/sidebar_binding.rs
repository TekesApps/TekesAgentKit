use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tekes_agent_components_ui::{Sidebar, SidebarIntent, SidebarLabels};
use tekes_agent_contract::*;
use tekes_agent_presentation::RunScope;
use tekes_agent_ui_bindings::{
    RunBinding, SidebarActionOutcome, SidebarBinding, SidebarPublication,
};
use tekes_agent_ui_demo::component_demo::{ComponentDemo, DemoCommand};
use tekes_agent_ui_demo::fixture::Fixture;
use tekes_agent_web::SharedAgent;
use tekes_ui_controls::composition::*;

struct OwnedDirectory(PathBuf);
impl OwnedDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "agentkit-sidebar-binding-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for OwnedDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
async fn until(binding: &SidebarBinding, predicate: impl Fn(&SidebarPublication) -> bool) {
    let mut changes = binding.subscribe();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !predicate(&binding.snapshot()) {
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}
fn address(id: &str) -> SessionAddress {
    SessionAddress {
        endpoint_id: "fixture".into(),
        session_id: id.into(),
    }
}

#[tokio::test]
async fn sidebar_consumes_shared_inventory_and_pins_broadcast_without_a_second_runtime_or_ui_pin_store()
 {
    let directory = OwnedDirectory::new();
    let path = directory.0.join("thread-preferences.json");
    let mut agent = SharedAgent::default();
    agent
        .open_thread_preferences(&path, "sidebar-test")
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
    let run = RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
        .await
        .unwrap();
    let first = SidebarBinding::new(agent.clone(), "fixture").unwrap();
    let second = SidebarBinding::new(agent, "fixture").unwrap();
    until(&first, |snapshot| snapshot.loaded).await;
    until(&second, |snapshot| snapshot.loaded).await;
    assert!(Arc::ptr_eq(first.runtime(), &runtime));
    assert_eq!(first.snapshot().rows.len(), 2);
    assert_eq!(first.snapshot().rows[0].title, "会话 a");
    let revision = run.current_run().unwrap().revision;
    let sidebar = Sidebar::new(93, SidebarLabels::default());
    let id = sidebar.id();
    let mut host = sidebar.into_host().unwrap();
    let view = host
        .get_mut(id)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap();
    view.update(&first.snapshot().input(Some(address("a"))))
        .unwrap();
    let pin = view.action_id(&address("a"), "pin").unwrap();
    host.accessibility_action(pin, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    let intents = host
        .get(id)
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .take_intents(&actions);
    let [
        SidebarIntent::Action {
            address: target,
            definition,
        },
    ] = intents.as_slice()
    else {
        panic!("scoped pin callback")
    };
    assert_eq!(target, &address("a"));
    let outcome = first
        .perform(target, definition, None, "pin-a-one")
        .await
        .unwrap();
    assert!(
        matches!(outcome, SidebarActionOutcome::Mutation { outcome, .. } if !outcome.clear_selection && outcome.navigation.is_none())
    );
    until(&first, |snapshot| {
        snapshot.rows[0].pin.as_ref().is_some_and(|pin| pin.pinned)
    })
    .await;
    until(&second, |snapshot| {
        snapshot.rows[0].pin.as_ref().is_some_and(|pin| pin.pinned)
    })
    .await;
    // A replay keeps the captured pin action/RPC identity, even though the fresh
    // menu now says unpin. The shared dispatcher owns replay semantics.
    first
        .perform(target, definition, None, "pin-a-one")
        .await
        .unwrap();
    assert_eq!(
        first
            .snapshot()
            .rows
            .iter()
            .filter(|row| row.pin.as_ref().is_some_and(|pin| pin.pinned))
            .count(),
        1
    );
    assert!(
        first.snapshot().rows[0]
            .actions
            .items
            .iter()
            .any(|item| item.id == "unpin" && item.enabled)
    );
    assert_eq!(
        first.snapshot().input(Some(address("a"))).selected,
        Some(address("a"))
    );
    assert_eq!(
        run.current_run().unwrap().revision,
        revision,
        "local preference changes don't mutate Run presentation"
    );
    drop(first);
    drop(second);
    assert!(
        run.current_run().unwrap().can_submit,
        "dropping inventory observers doesn't close Run history"
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn copy_uses_fresh_shared_value_and_mutations_recheck_capabilities_and_endpoint() {
    let mut agent = SharedAgent::default();
    agent.set_session_url_scheme("tekes-demo").unwrap();
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
    let binding = SidebarBinding::new(agent, "fixture").unwrap();
    until(&binding, |snapshot| snapshot.loaded).await;
    let menu = binding.snapshot().rows[0].actions.clone();
    let mut copy = menu
        .items
        .iter()
        .find(|item| item.id == "copySessionId")
        .unwrap()
        .clone();
    copy.copy_text = Some("stale-or-forged-value".into());
    assert_eq!(
        binding
            .perform(&address("a"), &copy, None, "")
            .await
            .unwrap(),
        SidebarActionOutcome::Copy {
            address: address("a"),
            text: "tekes-demo://native/a".into()
        }
    );
    let mut fork = menu
        .items
        .iter()
        .find(|item| item.id == "fork")
        .unwrap()
        .clone();
    fork.enabled = true;
    assert!(
        binding
            .perform(&address("a"), &fork, None, "fork-unavailable")
            .await
            .is_err()
    );
    let mut foreign = address("a");
    foreign.endpoint_id = "other".into();
    assert!(binding.perform(&foreign, &copy, None, "").await.is_err());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn sidebar_navigation_fences_late_open_results_and_keeps_each_summary_parent() {
    let mut agent = SharedAgent::default();
    agent
        .register(
            "fixture".into(),
            "native".into(),
            "Fixture".into(),
            Arc::new(Fixture::default().with_demo_summary()),
        )
        .unwrap();
    let agent = Arc::new(agent);
    let runtime = agent.endpoint("fixture").unwrap().runtime.clone();
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let a = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let b = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "b"))
            .await
            .unwrap(),
    );
    let source = SidebarBinding::new(agent, "fixture").unwrap();
    until(&source, |snapshot| snapshot.loaded).await;
    let mut view = ComponentDemo::new([a]).unwrap();
    view.set_sidebar_source(source.snapshot()).unwrap();
    view.sync().unwrap();
    let a_summary = view.summary_id();
    let row = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .row_id(&address("b"))
        .unwrap();
    let commands: [DemoCommand; 1] = view
        .accessibility_action(row, None, AccessibilityAction::Press)
        .unwrap()
        .try_into()
        .ok()
        .expect("one navigation");
    let [DemoCommand::OpenSession(old)] = commands else {
        panic!("navigation")
    };
    view.select(0).unwrap();
    assert!(
        !view.complete_navigation(old, b.clone()).unwrap(),
        "a newer selection retires the earlier async open"
    );
    assert_eq!(view.binding().scope().session_id, "a");
    let commands: [DemoCommand; 1] = view
        .accessibility_action(row, None, AccessibilityAction::Press)
        .unwrap()
        .try_into()
        .ok()
        .expect("one navigation");
    let [DemoCommand::OpenSession(disappeared)] = commands else {
        panic!("navigation")
    };
    let mut removed = (*source.snapshot()).clone();
    removed.revision += 1;
    removed.rows.retain(|row| row.address.session_id != "b");
    view.set_sidebar_source(Arc::new(removed.clone())).unwrap();
    view.sync().unwrap();
    assert!(
        !view
            .complete_navigation(disappeared.clone(), b.clone())
            .unwrap(),
        "confirmed inventory absence cannot mount an old session body"
    );
    let mut restored = (*source.snapshot()).clone();
    restored.revision = removed.revision + 1;
    view.set_sidebar_source(Arc::new(restored)).unwrap();
    view.sync().unwrap();
    assert!(
        !view.complete_navigation(disappeared, b.clone()).unwrap(),
        "a rejected completion stays retired after the pointer returns"
    );
    let restored_row = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .row_id(&address("b"))
        .unwrap();
    assert_ne!(
        row, restored_row,
        "retired row callbacks cannot reactivate the returned pointer"
    );
    let commands: [DemoCommand; 1] = view
        .accessibility_action(restored_row, None, AccessibilityAction::Press)
        .unwrap()
        .try_into()
        .ok()
        .expect("one navigation");
    let [DemoCommand::OpenSession(request)] = commands else {
        panic!("navigation")
    };
    assert!(view.complete_navigation(request, b).unwrap());
    assert_eq!(view.binding().scope().session_id, "b");
    assert_ne!(view.summary_id(), a_summary);
    assert!(view.tree.get(a_summary).is_none());
    assert_eq!(
        view.binding()
            .current_run()
            .unwrap()
            .summary
            .as_ref()
            .unwrap()
            .tasks[0]
            .name,
        "验证会话 b 的数据回传"
    );
    view.select(0).unwrap();
    assert_eq!(view.summary_id(), a_summary);
    assert!(view.tree.get(a_summary).is_some());
    assert_eq!(
        view.binding()
            .current_run()
            .unwrap()
            .summary
            .as_ref()
            .unwrap()
            .tasks[0]
            .name,
        "验证会话 a 的数据回传"
    );
    runtime.stop().await.unwrap();
}
