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
#[path = "support/menu_host.rs"]
mod menu_host;

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
#[tokio::test]
async fn rename_fork_archive_flow_uses_shared_results_preserves_drafts_and_clears_only_its_selection()
 {
    use tekes_agent_presentation::ComposerDestination;
    let mut agent = SharedAgent::default();
    agent
        .register(
            "fixture".into(),
            "native".into(),
            "Fixture".into(),
            Arc::new(
                Fixture::default()
                    .with_side_chats()
                    .with_lifecycle()
                    .with_children(),
            ),
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
    a.edit_text(
        &ComposerDestination::run(a.scope().clone()),
        "A 原草稿".into(),
    )
    .unwrap();
    let source = SidebarBinding::new(agent, "fixture").unwrap();
    until(&source, |state| state.loaded).await;
    let mut view = ComponentDemo::new([a.clone(), b.clone()]).unwrap();
    view.set_sidebar_source(source.snapshot()).unwrap();
    view.sync().unwrap();
    let (commands, _, _) = menu_host::select(&mut view, &address("a"), "rename");
    assert!(commands.is_empty());
    let ids = view.rename_ids().unwrap();
    assert!(
        view.accessibility_action(
            ids.editor,
            None,
            AccessibilityAction::SetValue(" 服务端新名称 ".into())
        )
        .unwrap()
        .is_empty()
    );
    let mut commands = view
        .accessibility_action(ids.submit, None, AccessibilityAction::Press)
        .unwrap();
    let DemoCommand::Rename(ticket) = commands.remove(0) else {
        panic!("rename ticket")
    };
    assert_eq!(
        source
            .snapshot()
            .rows
            .iter()
            .find(|row| row.address == address("a"))
            .unwrap()
            .title,
        "会话 a",
        "no optimistic local rename"
    );
    let result = source
        .perform(
            &ticket.address,
            &ticket.definition,
            Some(ticket.title.clone()),
            "rename-one",
        )
        .await
        .unwrap();
    assert!(
        matches!(result,SidebarActionOutcome::Mutation {outcome,..} if outcome.renamed_title.as_deref()==Some("服务端新名称"))
    );
    view.complete_rename(&ticket, Ok(())).unwrap();
    until(&source, |state| {
        state.rows.iter().any(|row| row.title == "服务端新名称")
    })
    .await;
    view.set_sidebar_source(source.snapshot()).unwrap();
    view.sync().unwrap();
    let (mut commands, menu, part) = menu_host::select(&mut view, &address("a"), "fork");
    assert!(
        view.accessibility_action(menu, Some(part), AccessibilityAction::Press)
            .unwrap()
            .is_empty(),
        "a consumed menu generation cannot emit another fork"
    );
    let DemoCommand::Sidebar(SidebarIntent::Action {
        address: origin,
        definition,
    }) = commands.remove(0)
    else {
        panic!("fork")
    };
    let token = view.begin_sidebar_action(origin.clone(), definition.id.clone());
    let SidebarActionOutcome::Mutation { outcome, .. } = source
        .perform(&origin, &definition, None, "fork-one")
        .await
        .unwrap()
    else {
        panic!("fork result")
    };
    view.finish_sidebar_action(&token);
    let request = view
        .apply_sidebar_outcome(&token, &outcome)
        .unwrap()
        .unwrap();
    let forked = request.address().clone();
    let run = Arc::new(
        RunBinding::open_main(
            runtime.clone(),
            RunScope::main("fixture", &forked.session_id),
        )
        .await
        .unwrap(),
    );
    assert!(view.complete_navigation(request, run).unwrap());
    assert_eq!(
        view.current_binding().unwrap().scope().session_id,
        forked.session_id
    );
    until(&source, |state| {
        state.rows.iter().any(|row| row.address == forked)
    })
    .await;
    view.set_sidebar_source(source.snapshot()).unwrap();
    view.sync().unwrap();
    let (mut commands, _, _) = menu_host::select(&mut view, &forked, "archive");
    let DemoCommand::Sidebar(SidebarIntent::Action {
        address: archive_address,
        definition,
    }) = commands.remove(0)
    else {
        panic!("archive")
    };
    let token = view.begin_sidebar_action(archive_address.clone(), definition.id.clone());
    let SidebarActionOutcome::Mutation { outcome, .. } = source
        .perform(&archive_address, &definition, None, "archive-one")
        .await
        .unwrap()
    else {
        panic!("archive outcome")
    };
    view.finish_sidebar_action(&token);
    view.apply_sidebar_outcome(&token, &outcome).unwrap();
    assert!(view.current_binding().is_none());
    assert!(view.tree.get(view.ids().root).is_none());
    assert!(
        view.tree.get(view.panel_id()).is_none(),
        "an archived selection is not a blank new conversation"
    );
    view.select(0).unwrap();
    assert_eq!(view.current_binding().unwrap().scope().session_id, "a");
    assert_eq!(a.current_run().unwrap().draft.text, "A 原草稿");
    assert_eq!(view.editor().input.text, "A 原草稿");
    let token = view.begin_sidebar_action(address("a"), "fork".into());
    view.select(1).unwrap();
    assert!(
        view.apply_sidebar_outcome(
            &token,
            &ThreadActionOutcome {
                navigation: Some(ThreadActionNavigation {
                    endpoint: "fixture".into(),
                    session_id: forked.session_id
                }),
                ..Default::default()
            }
        )
        .unwrap()
        .is_none(),
        "late fork cannot steal a newer selection"
    );
    let token = view.begin_sidebar_action(address("a"), "archive".into());
    view.apply_sidebar_outcome(
        &token,
        &ThreadActionOutcome {
            clear_selection: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(view.current_binding().unwrap().scope().session_id, "b");
    let child = tekes_agent_ui_bindings::ChildPaneBinding::for_run(
        &a,
        "child-a",
        tekes_agent_presentation::ChildHistoryLayout::IndependentSessions,
        None,
    )
    .await
    .unwrap();
    child.refresh_controls().await.unwrap();
    assert!(!child.refresh_key().unwrap().owner_archived);
    let definition = source
        .snapshot()
        .rows
        .iter()
        .find(|row| row.address == address("a"))
        .unwrap()
        .actions
        .items
        .iter()
        .find(|action| action.id == "archive")
        .unwrap()
        .clone();
    source
        .perform(&address("a"), &definition, None, "archive-parent")
        .await
        .unwrap();
    until(&source, |state| {
        !state.rows.iter().any(|row| row.address == address("a"))
    })
    .await;
    assert!(child.refresh_key().unwrap().owner_archived);
    child.refresh_controls().await.unwrap();
    assert!(!child.binding().current_run().unwrap().can_submit);
    runtime.stop().await.unwrap();
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
    let sidebar = Sidebar::new(93, SidebarLabels::default()).with_action_buttons();
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

#[derive(Clone)]
struct DelayedInventory {
    fixture: Fixture,
    gate: Arc<tokio::sync::Semaphore>,
}
struct DelayedEndpoint {
    inner: Arc<dyn SessionEndpoint>,
    gate: Arc<tokio::sync::Semaphore>,
}
#[async_trait::async_trait]
impl EndpointConnector for DelayedInventory {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        Ok(Arc::new(DelayedEndpoint {
            inner: self.fixture.connect().await?,
            gate: self.gate.clone(),
        }))
    }
}
#[async_trait::async_trait]
impl SessionEndpoint for DelayedEndpoint {
    fn identity(&self) -> &str {
        self.inner.identity()
    }
    fn host(&self) -> &HostDescription {
        self.inner.host()
    }
    fn capabilities(&self) -> EndpointCapabilities {
        self.inner.capabilities()
    }
    async fn call(
        &self,
        method: &str,
        payload: serde_json::Value,
        rpc_id: &str,
    ) -> Result<serde_json::Value> {
        self.inner.call(method, payload, rpc_id).await
    }
    async fn history_page(&self, request: HistoryRequest) -> Result<HistoryPage> {
        self.inner.history_page(request).await
    }
    async fn respond(&self, id: &str, epoch: i64, outcome: serde_json::Value) -> Result<()> {
        self.inner.respond(id, epoch, outcome).await
    }
    async fn close(&self) {
        self.inner.close().await;
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        use futures_util::StreamExt;
        let inventory = matches!(target, StreamTarget::SessionInventory);
        let stream = self.inner.follow(target).await?;
        if !inventory {
            return Ok(stream);
        }
        let gate = self.gate.clone();
        Ok(Box::pin(
            futures_util::stream::once(async move {
                gate.acquire_owned().await.unwrap().forget();
                stream
            })
            .flatten(),
        ))
    }
}

#[tokio::test]
async fn delayed_initial_baseline_is_not_restored_and_reconnect_keeps_last_readable_inventory() {
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let mut agent = SharedAgent::default();
    agent
        .register(
            "fixture".into(),
            "native".into(),
            "Delayed inventory".into(),
            Arc::new(DelayedInventory {
                fixture: Fixture::default(),
                gate: gate.clone(),
            }),
        )
        .unwrap();
    let agent = Arc::new(agent);
    let binding = SidebarBinding::new(agent.clone(), "fixture").unwrap();
    // The shared state stream emits before the inventory baseline. Wait for that
    // actual frame, not just the binding's constructor placeholder.
    until(&binding, |state| state.revision > 1).await;
    assert!(!binding.snapshot().loaded);
    assert!(binding.snapshot().rows.is_empty());
    let mut sidebar = Sidebar::new(36, SidebarLabels::default());
    sidebar.update(&binding.snapshot().input(None)).unwrap();
    assert!(!sidebar.displayed_input().unwrap().loaded);
    gate.add_permits(1);
    until(&binding, |state| state.loaded && !state.rows.is_empty()).await;
    sidebar.update(&binding.snapshot().input(None)).unwrap();
    let first = sidebar.row_id(&binding.snapshot().rows[0].address).unwrap();
    let runtime = binding.runtime();
    let old_epoch = runtime.inspect(|state| state.connection_epoch);
    let reconnect_runtime = runtime.clone();
    let reconnect = tokio::spawn(async move { reconnect_runtime.reconnect().await });
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let source = binding.snapshot();
            if runtime.inspect(|state| state.connection_epoch > old_epoch && !state.inventory_ready)
                && source.rows.iter().all(|row| row.activity.is_stale)
            {
                assert!(
                    source.loaded,
                    "reconnect erased completed initial restoration"
                );
                sidebar.update(&source.input(None)).unwrap();
                assert_eq!(sidebar.row_id(&source.rows[0].address), Some(first));
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    gate.add_permits(1);
    reconnect.await.unwrap().unwrap();
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    until(&binding, |state| {
        state.rows.iter().all(|row| !row.activity.is_stale)
    })
    .await;
    assert!(binding.snapshot().loaded);
    runtime.stop().await.unwrap();
}
