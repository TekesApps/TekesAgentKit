use std::{sync::Arc, time::Duration};
use tekes_agent_contract::{PromptMode, Result};
use tekes_agent_presentation::{
    AgentTask, ComposerDestination, PreparedSubmission, RunScope, SubmissionPreparation,
};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{PaneAccess, TaskPaneBinding};
use tekes_agent_ui_demo::fixture::Fixture;
use tokio::sync::Notify;

#[derive(Clone)]
struct SlowHistory {
    fixture: Fixture,
    entered: Arc<Notify>,
    release: Arc<Notify>,
    fail_baseline: bool,
}
struct SlowEndpoint {
    inner: Arc<dyn tekes_agent_contract::SessionEndpoint>,
    connector: SlowHistory,
}
#[async_trait::async_trait]
impl tekes_agent_contract::EndpointConnector for SlowHistory {
    async fn connect(&self) -> Result<Arc<dyn tekes_agent_contract::SessionEndpoint>> {
        Ok(Arc::new(SlowEndpoint {
            inner: self.fixture.connect().await?,
            connector: self.clone(),
        }))
    }
}
#[async_trait::async_trait]
impl tekes_agent_contract::SessionEndpoint for SlowEndpoint {
    fn identity(&self) -> &str {
        self.inner.identity()
    }
    fn host(&self) -> &tekes_agent_contract::HostDescription {
        self.inner.host()
    }
    fn capabilities(&self) -> tekes_agent_contract::EndpointCapabilities {
        self.inner.capabilities()
    }
    async fn follow(
        &self,
        target: tekes_agent_contract::StreamTarget,
    ) -> Result<tekes_agent_contract::FrameStream> {
        if matches!(&target, tekes_agent_contract::StreamTarget::SessionJournal { address, .. } if address.session_id == "b")
        {
            self.connector.entered.notify_one();
            self.connector.release.notified().await;
            if self.connector.fail_baseline {
                return Err(tekes_agent_contract::Error::Transport(
                    "old task history failed".into(),
                ));
            }
        }
        self.inner.follow(target).await
    }
    async fn history_page(
        &self,
        request: tekes_agent_contract::HistoryRequest,
    ) -> Result<tekes_agent_contract::HistoryPage> {
        self.inner.history_page(request).await
    }
    async fn call(
        &self,
        method: &str,
        params: serde_json::Value,
        rpc: &str,
    ) -> Result<serde_json::Value> {
        self.inner.call(method, params, rpc).await
    }
    async fn respond(&self, session: &str, id: i64, response: serde_json::Value) -> Result<()> {
        self.inner.respond(session, id, response).await
    }
    async fn close(&self) {
        self.inner.close().await;
    }
}

fn row(session: &str) -> AgentTask {
    AgentTask {
        id: "task".into(),
        scope: RunScope {
            endpoint_id: "fixture".into(),
            thread_id: "a".into(),
            session_id: session.into(),
        },
        name: "Current task".into(),
        status: "running".into(),
        kind: "task".into(),
    }
}
async fn setup() -> (Arc<SessionRuntime>, Arc<TaskPaneBinding>) {
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(Fixture::default()),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let pane = Arc::new(TaskPaneBinding::new(runtime.clone()));
    (runtime, pane)
}
#[tokio::test]
async fn task_status_updates_keep_exact_composer_but_pointer_aba_retires_callbacks() {
    let (runtime, pane) = setup().await;
    let token = pane.begin(RunScope::main("fixture", "a"), "task").unwrap();
    assert!(pane.open_resolved().await.unwrap().is_none());
    assert!(pane.commit(&token, Ok(Some(row("a")))));
    let first = pane.open_resolved().await.unwrap().unwrap();
    let destination = ComposerDestination::run(first.scope().clone());
    first.edit_text(&destination, "任务草稿".into()).unwrap();
    let mut updated = row("a");
    updated.status = "completed".into();
    updated.name = "Updated title".into();
    assert!(pane.commit(&token, Ok(Some(updated))));
    assert!(Arc::ptr_eq(
        &first,
        &pane.open_resolved().await.unwrap().unwrap()
    ));
    assert_eq!(first.current_run().unwrap().draft.text, "任务草稿");
    let old_tail = runtime.inspect(|state| state.sessions["a"].tail_sequence);
    let ticket = first.prepare_send(&destination, PromptMode::Queue).unwrap();
    assert!(pane.commit(&token, Ok(Some(row("b")))));
    let second = pane.open_resolved().await.unwrap().unwrap();
    assert_eq!(first.access(), PaneAccess::Missing);
    assert_eq!(second.scope().session_id, "b");
    assert_eq!(second.current_run().unwrap().draft.text, "");
    assert!(pane.commit(&token, Ok(Some(row("a")))));
    let third = pane.open_resolved().await.unwrap().unwrap();
    assert_eq!(
        first.access(),
        PaneAccess::Missing,
        "A→B→A does not revive old callbacks"
    );
    assert_eq!(second.access(), PaneAccess::Missing);
    assert!(!Arc::ptr_eq(&first, &third));
    assert!(
        first
            .dispatch_prepared_with(ticket, &Prepare)
            .await
            .is_err()
    );
    assert_eq!(
        runtime.inspect(|state| state.sessions["a"].tail_sequence),
        old_tail
    );
    assert!(first.edit_text(&destination, "stale".into()).is_err());
    pane.cancel();
    assert_eq!(third.access(), PaneAccess::Missing);
    runtime.stop().await.unwrap();
}
struct Prepare;
#[async_trait::async_trait]
impl SubmissionPreparation for Prepare {
    async fn prepare(
        &self,
        _: &SessionRuntime,
        _: &PreparedSubmission,
        _: &RunScope,
    ) -> Result<()> {
        Ok(())
    }
}
struct HoldPrepare {
    entered: Arc<Notify>,
    release: Arc<Notify>,
}
#[async_trait::async_trait]
impl SubmissionPreparation for HoldPrepare {
    async fn prepare(
        &self,
        _: &SessionRuntime,
        _: &PreparedSubmission,
        _: &RunScope,
    ) -> Result<()> {
        self.entered.notify_one();
        self.release.notified().await;
        Ok(())
    }
}
#[tokio::test]
async fn changed_task_pointer_during_host_preparation_cannot_admit_the_old_click() {
    let (runtime, pane) = setup().await;
    let token = pane.begin(RunScope::main("fixture", "a"), "task").unwrap();
    pane.commit(&token, Ok(Some(row("a"))));
    let run = pane.open_resolved().await.unwrap().unwrap();
    let target = ComposerDestination::run(run.scope().clone());
    run.edit_text(&target, "不能迟到发送".into()).unwrap();
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let prepare = HoldPrepare {
        entered: entered.clone(),
        release: release.clone(),
    };
    let tail = runtime.inspect(|state| state.sessions["a"].tail_sequence);
    let sending =
        tokio::spawn(async move { run.send_with(&target, PromptMode::Queue, &prepare).await });
    tokio::time::timeout(Duration::from_secs(3), entered.notified())
        .await
        .unwrap();
    pane.commit(&token, Ok(None));
    release.notify_one();
    assert!(sending.await.unwrap().is_err());
    assert_eq!(
        runtime.inspect(|state| state.sessions["a"].tail_sequence),
        tail
    );
    assert!(pane.open_resolved().await.unwrap().is_none());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn loading_missing_error_foreign_and_cancel_never_fall_back_to_parent_run() {
    let (runtime, pane) = setup().await;
    assert!(pane.begin(RunScope::main("other", "a"), "task").is_err());
    let parent = RunScope::main("fixture", "a");
    let old = pane.begin(parent.clone(), "task").unwrap();
    let current = pane.begin(parent.clone(), "task").unwrap();
    assert!(!pane.commit(&old, Ok(Some(row("b")))));
    assert!(pane.open_resolved().await.unwrap().is_none());
    let mut foreign = row("b");
    foreign.scope.thread_id = "other".into();
    assert!(!pane.commit(&current, Ok(Some(foreign))));
    assert!(pane.commit(&current, Ok(None)));
    assert!(pane.snapshot().resolved && pane.snapshot().value.is_none());
    assert!(pane.open_resolved().await.unwrap().is_none());
    assert!(pane.commit(&current, Err("authoritative read unavailable".into())));
    assert!(pane.snapshot().error.is_some());
    assert!(pane.open_resolved().await.unwrap().is_none());
    pane.commit(&current, Ok(Some(row("b"))));
    let run = pane.open_resolved().await.unwrap().unwrap();
    assert_eq!(
        pane.tab("pending").unwrap().task_scope,
        Some(row("b").scope)
    );
    drop(pane);
    assert_eq!(
        run.access(),
        PaneAccess::Missing,
        "dropping owner revokes external callback authority without an Arc cycle"
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn public_task_source_mounts_real_controls_keeps_drafts_and_rejects_main_run_substitution() {
    use tekes_agent_components_ui::SidePanel;
    use tekes_agent_ui_bindings::RunBinding;
    use tekes_agent_ui_demo::component_demo::{ComponentDemo, DemoCommand, SELECT_A, SELECT_B};
    use tekes_ui_controls::composition::*;
    let (runtime, _) = setup().await;
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
    let source = Arc::new(TaskPaneBinding::for_run(&a));
    let mut view = ComponentDemo::new([a.clone(), b]).unwrap();
    let token = source.begin(a.scope().clone(), "task").unwrap();
    source.commit(&token, Ok(Some(row("a"))));
    view.open_task_source(source.clone()).unwrap();
    let mut commands = view.take_background_commands();
    assert_eq!(commands.len(), 1);
    let DemoCommand::OpenTask {
        parent,
        task_id,
        source: command_source,
    } = commands.remove(0)
    else {
        panic!("task history read")
    };
    assert!(Arc::ptr_eq(&source, &command_source));
    assert!(
        !view
            .install_task_run(parent.clone(), task_id.clone(), a.clone())
            .unwrap()
    );
    let run = source.open_resolved().await.unwrap().unwrap();
    assert!(
        view.install_task_run(parent.clone(), task_id.clone(), run.clone())
            .unwrap()
    );
    let ids = view.panel_composer_ids().unwrap();
    assert_ne!(ids, view.ids());
    view.tree.focus(Some(ids.editor), true);
    assert!(
        view.event(&ContentEvent::Commit {
            text: "同一目标共享草稿".into(),
            replacement: None
        })
        .unwrap()
        .is_empty()
    );
    assert_eq!(run.current_run().unwrap().draft.text, "同一目标共享草稿");
    assert_eq!(a.current_run().unwrap().draft.text, "同一目标共享草稿");
    let press = |view: &mut ComponentDemo, target| {
        view.accessibility_action(target, None, AccessibilityAction::Press)
            .unwrap()
    };
    assert!(press(&mut view, SELECT_B).is_empty());
    assert!(view.panel_composer_ids().is_none());
    assert!(press(&mut view, SELECT_A).is_empty());
    assert_eq!(view.panel_composer_ids(), Some(ids));
    // Revocation also removes the retained controls; it doesn't show a blank
    // parent conversation while the host resolves the next task pointer.
    source.commit(&token, Ok(None));
    view.sync().unwrap();
    assert!(view.tree.get(ids.root).is_none());
    assert!(
        !view
            .install_task_run(parent.clone(), task_id.clone(), run.clone())
            .unwrap()
    );
    source.commit(&token, Ok(Some(row("a"))));
    view.sync().unwrap();
    let mut commands = view.take_background_commands();
    let DemoCommand::OpenTask {
        source: new_source, ..
    } = commands.remove(0)
    else {
        panic!("reappearance reads a new source")
    };
    let replacement = new_source.open_resolved().await.unwrap().unwrap();
    assert!(!Arc::ptr_eq(&run, &replacement));
    view.install_task_run(parent.clone(), task_id, replacement.clone())
        .unwrap();
    assert_ne!(view.panel_composer_ids(), Some(ids));
    let tail = runtime.inspect(|state| state.sessions["a"].tail_sequence);
    let close = view
        .tree
        .get(view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap()
        .close_id();
    assert!(
        press(&mut view, close).is_empty(),
        "closing task tab is local navigation, not cancellation/discard"
    );
    assert_eq!(replacement.access(), PaneAccess::Missing);
    assert!(view.panel_composer_ids().is_none());
    assert_eq!(
        runtime.inspect(|state| state.sessions["a"].tail_sequence),
        tail
    );
    let refresh = source.begin(a.scope().clone(), "task").unwrap();
    source.commit(&refresh, Ok(Some(row("a"))));
    view.open_task_source(source.clone()).unwrap();
    source
        .begin(RunScope::main("fixture", "b"), "other-task")
        .unwrap();
    view.sync().unwrap();
    assert!(
        view.panel_composer_ids().is_none(),
        "source retargeting retires the old tab"
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn slow_history_baseline_from_an_old_load_cannot_install_after_same_task_is_reselected() {
    for fail_baseline in [false, true] {
        let connector = SlowHistory {
            fixture: Fixture::default(),
            entered: Arc::new(Notify::new()),
            release: Arc::new(Notify::new()),
            fail_baseline,
        };
        let runtime = Arc::new(SessionRuntime::spawn(
            "fixture",
            Arc::new(connector.clone()),
        ));
        runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
        let source = Arc::new(TaskPaneBinding::new(runtime.clone()));
        let parent = RunScope::main("fixture", "a");
        let first = source.begin(parent.clone(), "task").unwrap();
        source.commit(&first, Ok(Some(row("b"))));
        let opening_source = source.clone();
        let opening = tokio::spawn(async move { opening_source.open_resolved().await });
        tokio::time::timeout(Duration::from_secs(3), connector.entered.notified())
            .await
            .unwrap();
        let second = source.begin(parent, "task").unwrap();
        source.commit(&second, Ok(Some(row("b"))));
        connector.release.notify_one();
        assert!(
            tokio::time::timeout(Duration::from_secs(3), opening)
                .await
                .unwrap()
                .unwrap()
                .unwrap()
                .is_none()
        );
        assert!(!source.commit(&first, Ok(None)));
        runtime.stop().await.unwrap();
    }
}

#[tokio::test]
async fn same_destination_task_shares_parent_draft_admission_and_survives_parent_mount_drop() {
    use tekes_agent_ui_bindings::RunBinding;
    let (runtime, _) = setup().await;
    let parent = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let destination = ComposerDestination::run(parent.scope().clone());
    parent
        .edit_text(&destination, "同一目标的原草稿".into())
        .unwrap();
    let source = TaskPaneBinding::for_run(&parent);
    let token = source.begin(parent.scope().clone(), "task").unwrap();
    assert!(source.commit(&token, Ok(Some(row("a")))));
    let task = source.open_resolved().await.unwrap().unwrap();
    assert_eq!(task.current_run().unwrap().draft.text, "同一目标的原草稿");
    task.edit_text(&destination, "Task 编辑同步给原 Run".into())
        .unwrap();
    assert_eq!(
        parent.current_run().unwrap().draft.text,
        "Task 编辑同步给原 Run"
    );
    let ticket = task.prepare_send(&destination, PromptMode::Queue).unwrap();
    assert!(
        parent
            .prepare_send(&destination, PromptMode::Queue)
            .is_err()
    );
    task.cancel_prepared(ticket).unwrap();
    assert_eq!(
        parent.current_run().unwrap().draft.text,
        "Task 编辑同步给原 Run"
    );
    let old_epoch = task.current_run().unwrap().connection_epoch;
    let mut reconnect_changes = task.subscribe_run().unwrap();
    drop(parent);
    runtime.reconnect().await.unwrap();
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    // Observe the retained controller directly: no UI refresh may repair it.
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let frame = reconnect_changes.borrow_and_update().clone();
            if frame.connection_epoch > old_epoch && !frame.connection.history_is_stale {
                assert_eq!(frame.draft.text, "Task 编辑同步给原 Run");
                break;
            }
            reconnect_changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    task.send_with(&destination, PromptMode::Queue, &Prepare)
        .await
        .unwrap();
    let mut changes = task.subscribe_run().unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let frame = changes.borrow_and_update().clone();
            if frame
                .transcript
                .items
                .iter()
                .any(|item| item.text == "Task 编辑同步给原 Run")
            {
                break;
            }
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    source.cancel();
    assert_eq!(task.access(), PaneAccess::Missing);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn public_consumer_reuses_exact_task_target_without_replacing_its_presentation() {
    use tekes_agent_ui_bindings::RunBinding;
    use tekes_agent_ui_consumer_check::open_task_from_original_run;
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(Fixture::default().with_children()),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let source = TaskPaneBinding::new(runtime.clone());
    let canonical = RunBinding::open_task(runtime.clone(), &row("child-a"))
        .await
        .unwrap();
    canonical.refresh_child_catalog().await.unwrap();
    let destination = ComposerDestination::run(canonical.scope().clone());
    canonical
        .edit_text(&destination, "目标 B 已有草稿".into())
        .unwrap();
    let token = source
        .begin(RunScope::main("fixture", "a"), "task")
        .unwrap();
    source.commit(&token, Ok(Some(row("child-a"))));
    let wrong = RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "child-a"))
        .await
        .unwrap();
    assert!(
        open_task_from_original_run(&source, &wrong).await.is_err(),
        "session alone is not full scope"
    );
    let foreign_runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(Fixture::default().with_children()),
    ));
    foreign_runtime
        .wait_ready(Duration::from_secs(3))
        .await
        .unwrap();
    let foreign = RunBinding::open_task(foreign_runtime.clone(), &row("child-a"))
        .await
        .unwrap();
    assert!(
        open_task_from_original_run(&source, &foreign)
            .await
            .is_err()
    );
    let task = open_task_from_original_run(&source, &canonical)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(task.current_run().unwrap().draft.text, "目标 B 已有草稿");
    let other = RunBinding::open_task(runtime.clone(), &row("child-a"))
        .await
        .unwrap();
    assert!(
        open_task_from_original_run(&source, &other).await.is_err(),
        "live mount must not silently switch draft owner"
    );
    task.edit_text(&destination, "共享后的草稿".into()).unwrap();
    assert_eq!(canonical.current_run().unwrap().draft.text, "共享后的草稿");
    let ticket = task.prepare_send(&destination, PromptMode::Queue).unwrap();
    source.cancel();
    assert_eq!(task.access(), PaneAccess::Missing);
    assert!(task.dispatch_prepared_with(ticket, &Prepare).await.is_err());
    assert_eq!(canonical.access(), PaneAccess::Ready);
    assert_eq!(canonical.current_run().unwrap().draft.text, "共享后的草稿");
    let next = canonical
        .prepare_send(&destination, PromptMode::Queue)
        .unwrap();
    canonical.cancel_prepared(next).unwrap();
    foreign_runtime.stop().await.unwrap();
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn shared_parent_pointer_aba_keeps_draft_and_nested_task_retains_ancestor_authority() {
    use tekes_agent_ui_bindings::RunBinding;
    let (runtime, _) = setup().await;
    let parent = RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
        .await
        .unwrap();
    let destination = ComposerDestination::run(parent.scope().clone());
    parent.edit_text(&destination, "父目标草稿".into()).unwrap();
    let source = TaskPaneBinding::for_run(&parent);
    let token = source.begin(parent.scope().clone(), "task").unwrap();
    source.commit(&token, Ok(Some(row("a"))));
    let first = source.open_resolved().await.unwrap().unwrap();
    let nested_source = TaskPaneBinding::for_run(&first);
    let nested_token = nested_source.begin(parent.scope().clone(), "task").unwrap();
    nested_source.commit(&nested_token, Ok(Some(row("a"))));
    let nested = nested_source.open_resolved().await.unwrap().unwrap();
    source.commit(&token, Ok(Some(row("b"))));
    let other = source.open_resolved().await.unwrap().unwrap();
    assert_eq!(other.current_run().unwrap().draft.text, "");
    assert_eq!(nested.access(), PaneAccess::Missing);
    assert!(nested.edit_text(&destination, "禁止旧控制".into()).is_err());
    source.commit(&token, Ok(Some(row("a"))));
    let restored = source.open_resolved().await.unwrap().unwrap();
    assert_eq!(first.access(), PaneAccess::Missing);
    assert_eq!(nested.access(), PaneAccess::Missing);
    assert_eq!(restored.current_run().unwrap().draft.text, "父目标草稿");
    source.cancel();
    assert_eq!(parent.access(), PaneAccess::Ready);
    assert_eq!(parent.current_run().unwrap().draft.text, "父目标草稿");
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn task_alias_retains_sidechat_lifetime_and_never_promotes_bounded_history() {
    use tekes_agent_presentation::ChildHistoryLayout;
    use tekes_agent_ui_bindings::{ChildPaneBinding, RunBinding, SideChatBinding};
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(Fixture::default().with_children().with_side_chats()),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let parent = RunScope::main("fixture", "a");
    let chats = SideChatBinding::new(runtime.clone());
    let pointer = chats
        .create(parent.clone(), None, "task-alias-chat")
        .await
        .unwrap();
    let scratch = chats.open(&pointer).await.unwrap();
    let task_row = AgentTask {
        scope: scratch.binding().scope().clone(),
        ..row("a")
    };
    let source = TaskPaneBinding::for_run(scratch.binding());
    let token = source.begin(task_row.scope.clone(), "task").unwrap();
    source.commit(&token, Ok(Some(task_row)));
    let alias = source.open_resolved().await.unwrap().unwrap();
    assert_eq!(alias.access(), PaneAccess::Ready);
    chats.close(&pointer, "task-alias-close").await.unwrap();
    assert_eq!(alias.access(), PaneAccess::Missing);
    assert!(
        source.open_resolved_from(scratch.binding()).await.is_err(),
        "a retired parent cannot reopen a task"
    );
    let bounded = ChildPaneBinding::open(
        runtime.clone(),
        parent,
        "child-a",
        ChildHistoryLayout::SameSession,
        None,
    )
    .await
    .unwrap();
    let source = TaskPaneBinding::for_run(bounded.binding());
    let token = source
        .begin(bounded.binding().scope().clone(), "task")
        .unwrap();
    source.commit(&token, Ok(Some(row("child-a"))));
    assert!(
        source.open_resolved().await.is_err(),
        "read-only source cannot become an empty writable Task"
    );
    // Main source stays unaffected by alias retirement.
    let main = RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
        .await
        .unwrap();
    assert_eq!(main.access(), PaneAccess::Ready);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn child_open_cannot_install_after_its_task_parent_retires_during_history_baseline() {
    use tekes_agent_presentation::ChildHistoryLayout;
    use tekes_agent_ui_bindings::ChildPaneBinding;
    let connector = SlowHistory {
        fixture: Fixture::default(),
        entered: Arc::new(Notify::new()),
        release: Arc::new(Notify::new()),
        fail_baseline: false,
    };
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(connector.clone()),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let source = TaskPaneBinding::new(runtime.clone());
    let token = source
        .begin(RunScope::main("fixture", "a"), "task")
        .unwrap();
    source.commit(&token, Ok(Some(row("a"))));
    let parent = source.open_resolved().await.unwrap().unwrap();
    let opening = tokio::spawn(async move {
        ChildPaneBinding::for_run(&parent, "b", ChildHistoryLayout::IndependentSessions, None).await
    });
    tokio::time::timeout(Duration::from_secs(3), connector.entered.notified())
        .await
        .unwrap();
    source.cancel();
    connector.release.notify_one();
    assert!(
        tokio::time::timeout(Duration::from_secs(3), opening)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    runtime.stop().await.unwrap();
}
