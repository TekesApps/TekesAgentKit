use std::{sync::Arc, time::Duration};
use tekes_agent_components_ui::{ComposerIds, SidePanel};
use tekes_agent_contract::Result;
use tekes_agent_presentation::{
    AgentTask, ChildHistoryLayout, ComposerDestination, RunScope, SideChat, TranscriptSnapshot,
};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{
    AttachmentFile, AttachmentOutcome, AttachmentSelection, ChildPaneBinding, PaneAccess,
    RunBinding, SideChatBinding, TaskPaneBinding,
};
use tekes_agent_ui_demo::{
    component_demo::{ComponentDemo, DemoCommand, SELECT_A, SELECT_B},
    fixture::Fixture,
};
use tekes_ui_controls::composition::*;
use tokio::sync::Semaphore;
async fn setup(fixture: Fixture) -> (Arc<SessionRuntime>, Arc<SideChatBinding>, ComponentDemo) {
    let runtime = Arc::new(SessionRuntime::spawn("fixture", Arc::new(fixture)));
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
    let owner = Arc::new(SideChatBinding::new(runtime.clone()));
    let mut view = ComponentDemo::new([a, b]).unwrap();
    view.set_side_chat_binding(owner.clone());
    view.set_file_picker_available(true);
    view.sync().unwrap();
    (runtime, owner, view)
}
fn destination(binding: &RunBinding) -> ComposerDestination {
    ComposerDestination::run(binding.scope().clone())
}
fn press(view: &mut ComponentDemo, id: ComponentId) -> Vec<DemoCommand> {
    view.accessibility_action(id, None, AccessibilityAction::Press)
        .unwrap()
}
fn select_file(view: &mut ComponentDemo, ids: ComposerIds) -> Arc<AttachmentSelection> {
    let mut commands = press(view, ids.attachment);
    assert_eq!(commands.len(), 1);
    let DemoCommand::SelectFiles { request } = commands.remove(0) else {
        panic!("file selection")
    };
    request
}
fn file() -> AttachmentFile {
    AttachmentFile::new(
        "侧会话.txt".into(),
        "text/plain".into(),
        Arc::from(&b"pane proof"[..]),
    )
    .unwrap()
}
async fn chat(view: &mut ComponentDemo, owner: &SideChatBinding) -> (SideChat, Arc<RunBinding>) {
    let id = view
        .tree
        .get(view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap()
        .create_side_chat_id();
    let DemoCommand::CreateSideChat(request) = press(view, id).remove(0) else {
        panic!()
    };
    let chat = owner
        .create(
            request.parent().clone(),
            Some("Parent A"),
            "pane-upload-chat",
        )
        .await
        .unwrap();
    let run = owner.open(&chat).await.unwrap();
    let binding = run.binding().clone();
    assert!(view.install_side_chat_run(request, run).unwrap());
    (chat, binding)
}
async fn wait_upload(fixture: &Fixture, count: usize) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while fixture.upload_calls() < count {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
fn fixture() -> Fixture {
    Fixture::default()
        .with_uploads()
        .with_side_chats()
        .with_children()
}
#[tokio::test]
async fn sidechat_upload_keeps_parent_send_enabled_and_restores_its_status_after_navigation() {
    let (runtime, owner, mut view) = setup(fixture()).await;
    let (_, run) = chat(&mut view, &owner).await;
    let parent = view.binding().clone();
    parent
        .edit_text(&destination(&parent), "父草稿".into())
        .unwrap();
    run.edit_text(&destination(&run), "侧会话草稿".into())
        .unwrap();
    view.sync().unwrap();
    let ids = view.panel_composer_ids().unwrap();
    let request = select_file(&mut view, ids);
    assert!(view.upload_pending(&run));
    assert!(!view.upload_pending(&parent));
    assert!(
        view.tree
            .get(ids.send)
            .unwrap()
            .component::<ButtonContent>()
            .unwrap()
            .button
            .state
            .disabled
    );
    assert!(
        !view
            .tree
            .get(view.ids().send)
            .unwrap()
            .component::<ButtonContent>()
            .unwrap()
            .button
            .state
            .disabled
    );
    press(&mut view, SELECT_B);
    assert!(
        view.tree
            .semantics()
            .iter()
            .all(|n| !n.semantics.name.contains("正在选择或检查文件"))
    );
    let report = request.upload(&[file()]).await.unwrap();
    assert_eq!(report.outcome, AttachmentOutcome::Attached);
    view.complete_file_selection(&request, vec![file()], Ok(Some(report)))
        .unwrap();
    assert_eq!(view.selected(), 1);
    assert!(parent.current_run().unwrap().draft.artifacts.is_empty());
    assert_eq!(parent.current_run().unwrap().draft.text, "父草稿");
    assert_eq!(run.current_run().unwrap().draft.artifacts.len(), 1);
    assert_eq!(run.current_run().unwrap().draft.text, "侧会话草稿");
    press(&mut view, SELECT_A);
    assert!(
        view.tree
            .semantics()
            .iter()
            .any(|n| n.semantics.name.contains("附件已加入原会话草稿"))
    );
    let ids = view.panel_composer_ids().unwrap();
    let DemoCommand::Send { binding, ticket } = press(&mut view, ids.send).remove(0) else {
        panic!()
    };
    assert!(Arc::ptr_eq(&binding, &run));
    binding.cancel_prepared(ticket).unwrap();
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn closed_sidechat_keeps_late_receipt_in_a_separate_recovery_status() {
    let gate = Arc::new(Semaphore::new(0));
    let fixture = fixture().with_upload_gate(gate.clone());
    let (runtime, owner, mut view) = setup(fixture.clone()).await;
    let (chat, run) = chat(&mut view, &owner).await;
    let parent = view.binding().clone();
    let ids = view.panel_composer_ids().unwrap();
    let request = select_file(&mut view, ids);
    let task = {
        let request = request.clone();
        tokio::spawn(async move { request.upload(&[file()]).await.unwrap() })
    };
    wait_upload(&fixture, 1).await;
    owner.close(&chat, "close-during-upload").await.unwrap();
    view.sync().unwrap();
    assert_eq!(run.access(), PaneAccess::Missing);
    assert!(
        view.tree
            .semantics()
            .iter()
            .any(|n| n.semantics.name.contains("原来源已关闭"))
    );
    gate.add_permits(1);
    let report = task.await.unwrap();
    assert_eq!(report.outcome, AttachmentOutcome::Detached);
    assert_eq!(report.receipts.len(), 1);
    view.complete_file_selection(&request, vec![file()], Ok(Some(report)))
        .unwrap();
    assert_eq!(
        view.upload_report_for_request(request.request_id())
            .unwrap()
            .receipts
            .len(),
        1
    );
    assert!(parent.current_run().unwrap().draft.artifacts.is_empty());
    let dismiss = view
        .tree
        .semantics()
        .iter()
        .find(|n| n.semantics.name == "关闭附件状态")
        .unwrap()
        .id;
    press(&mut view, dismiss);
    assert!(
        view.upload_report_for_request(request.request_id())
            .is_none()
    );
    runtime.stop().await.unwrap();
}
fn row(parent: &RunScope) -> AgentTask {
    AgentTask {
        id: "task".into(),
        scope: parent.clone(),
        name: "任务附件".into(),
        status: "running".into(),
        kind: "task".into(),
    }
}
async fn task_run(view: &mut ComponentDemo, source: &Arc<TaskPaneBinding>) -> Arc<RunBinding> {
    view.open_task_source(source.clone()).unwrap();
    let mut commands = view.take_background_commands();
    let DemoCommand::OpenTask {
        parent, task_id, ..
    } = commands.remove(0)
    else {
        panic!()
    };
    let run = source.open_resolved().await.unwrap().unwrap();
    assert!(view.install_task_run(parent, task_id, run.clone()).unwrap());
    run
}
#[tokio::test]
async fn same_scope_task_replacement_uses_distinct_authority_and_keeps_old_receipt_separate() {
    let gate = Arc::new(Semaphore::new(0));
    let fixture = fixture().with_upload_gate(gate.clone());
    let (runtime, _, mut view) = setup(fixture.clone()).await;
    let parent = view.binding().clone();
    let source = Arc::new(TaskPaneBinding::for_run(&parent));
    let token = source.begin(parent.scope().clone(), "task").unwrap();
    source.commit(&token, Ok(Some(row(parent.scope()))));
    let old = task_run(&mut view, &source).await;
    let ids = view.panel_composer_ids().unwrap();
    let request = select_file(&mut view, ids);
    let operation = {
        let request = request.clone();
        tokio::spawn(async move { request.upload(&[file()]).await.unwrap() })
    };
    wait_upload(&fixture, 1).await;
    let token = source.begin(parent.scope().clone(), "task").unwrap();
    source.commit(&token, Ok(Some(row(parent.scope()))));
    let replacement = task_run(&mut view, &source).await;
    assert_eq!(old.scope(), replacement.scope());
    assert!(!Arc::ptr_eq(&old, &replacement));
    assert_eq!(old.access(), PaneAccess::Missing);
    let ids = view.panel_composer_ids().unwrap();
    let newer = select_file(&mut view, ids);
    assert_ne!(newer.request_id(), request.request_id());
    gate.add_permits(2);
    let report = operation.await.unwrap();
    assert_eq!(report.outcome, AttachmentOutcome::Detached);
    view.complete_file_selection(&request, vec![file()], Ok(Some(report)))
        .unwrap();
    assert!(
        parent.current_run().unwrap().draft.artifacts.is_empty(),
        "retired role must not attach its late receipt to the shared destination"
    );
    let report = newer.upload(&[file()]).await.unwrap();
    assert_eq!(report.outcome, AttachmentOutcome::Attached);
    view.complete_file_selection(&newer, vec![file()], Ok(Some(report)))
        .unwrap();
    assert!(
        view.upload_report(parent.scope()).is_none(),
        "same-address operation lookup must remain ambiguous"
    );
    assert_eq!(
        view.upload_report_for_request(request.request_id())
            .unwrap()
            .outcome,
        AttachmentOutcome::Detached
    );
    assert_eq!(
        view.upload_report_for_request(newer.request_id())
            .unwrap()
            .outcome,
        AttachmentOutcome::Attached
    );
    assert_eq!(replacement.current_run().unwrap().draft.artifacts.len(), 1);
    assert_eq!(parent.current_run().unwrap().draft.artifacts.len(), 1);
    runtime.stop().await.unwrap();
}
struct Bounded;
#[async_trait::async_trait]
impl tekes_agent_presentation::ChildHistorySource for Bounded {
    async fn read(&self, _: &RunScope, _: &str) -> Result<TranscriptSnapshot> {
        Ok(TranscriptSnapshot::default())
    }
}
#[tokio::test]
async fn independent_child_uses_its_own_upload_and_bounded_child_never_gets_a_picker() {
    let (runtime, _, mut view) = setup(fixture()).await;
    let parent = view.binding().clone();
    let source = Arc::new(
        ChildPaneBinding::for_run(
            &parent,
            "child-a",
            ChildHistoryLayout::IndependentSessions,
            None,
        )
        .await
        .unwrap(),
    );
    let request = view.begin_child_open(parent.scope()).unwrap();
    view.install_child_source(
        request,
        source.clone(),
        "子代理附件".into(),
        source.snapshot().await.unwrap(),
    )
    .unwrap();
    source.refresh_controls().await.unwrap();
    view.update_child_snapshot(parent.scope(), &source, source.snapshot().await.unwrap())
        .unwrap();
    view.take_background_commands();
    let run = source.binding().clone();
    let ids = view.panel_composer_ids().unwrap();
    let request = select_file(&mut view, ids);
    let report = request.upload(&[file()]).await.unwrap();
    assert_eq!(report.outcome, AttachmentOutcome::Attached);
    view.complete_file_selection(&request, vec![file()], Ok(Some(report)))
        .unwrap();
    assert_eq!(run.current_run().unwrap().draft.artifacts.len(), 1);
    assert!(parent.current_run().unwrap().draft.artifacts.is_empty());
    press(&mut view, SELECT_B);
    let parent = view.binding().clone();
    let bounded = Arc::new(
        ChildPaneBinding::for_run(
            &parent,
            "bounded-b",
            ChildHistoryLayout::SameSession,
            Some(Arc::new(Bounded)),
        )
        .await
        .unwrap(),
    );
    let request = view.begin_child_open(parent.scope()).unwrap();
    view.install_child_source(
        request,
        bounded.clone(),
        "有界只读".into(),
        bounded.snapshot().await.unwrap(),
    )
    .unwrap();
    assert!(view.panel_composer_ids().is_none());
    assert!(
        view.begin_file_selection(bounded.binding().clone(), destination(bounded.binding()))
            .is_err()
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn main_and_task_share_attachments_while_each_upload_keeps_its_original_context_fence() {
    let (runtime, _, mut view) = setup(fixture()).await;
    let parent = view.binding().clone();
    let source = Arc::new(TaskPaneBinding::for_run(&parent));
    let token = source.begin(parent.scope().clone(), "task").unwrap();
    source.commit(&token, Ok(Some(row(parent.scope()))));
    let run = task_run(&mut view, &source).await;
    let ids = view.ids();
    let main_request = select_file(&mut view, ids);
    let ids = view.panel_composer_ids().unwrap();
    let pane_request = select_file(&mut view, ids);
    assert_eq!(parent.scope(), run.scope());
    assert!(view.upload_pending(&parent) && view.upload_pending(&run));
    let report = main_request.upload(&[file()]).await.unwrap();
    view.complete_file_selection(&main_request, vec![file()], Ok(Some(report)))
        .unwrap();
    assert!(!view.upload_pending(&parent) && view.upload_pending(&run));
    assert_eq!(run.current_run().unwrap().draft.artifacts.len(), 1);
    let report = pane_request.upload(&[file()]).await.unwrap();
    assert_eq!(
        report.outcome,
        AttachmentOutcome::Detached,
        "original context baseline must not overwrite a newer shared attachment"
    );
    view.complete_file_selection(&pane_request, vec![file()], Ok(Some(report)))
        .unwrap();
    let main = view
        .upload_report_for_request(main_request.request_id())
        .unwrap();
    let pane = view
        .upload_report_for_request(pane_request.request_id())
        .unwrap();
    assert_ne!(main.request_id, pane.request_id);
    assert_eq!(parent.current_run().unwrap().draft.artifacts.len(), 1);
    assert_eq!(run.current_run().unwrap().draft.artifacts.len(), 1);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn child_refresh_and_preedit_never_open_a_file_picker() {
    let fixture = fixture();
    let (runtime, _, mut view) = setup(fixture.clone()).await;
    let parent = view.binding().clone();
    let source = Arc::new(
        ChildPaneBinding::for_run(
            &parent,
            "child-a",
            ChildHistoryLayout::IndependentSessions,
            None,
        )
        .await
        .unwrap(),
    );
    let request = view.begin_child_open(parent.scope()).unwrap();
    view.install_child_source(
        request,
        source.clone(),
        "子代理".into(),
        source.snapshot().await.unwrap(),
    )
    .unwrap();
    let run = source.binding().clone();
    assert!(
        view.begin_file_selection(run.clone(), destination(&run))
            .is_err()
    );
    source.refresh_controls().await.unwrap();
    view.update_child_snapshot(parent.scope(), &source, source.snapshot().await.unwrap())
        .unwrap();
    view.take_background_commands();
    let ids = view.panel_composer_ids().unwrap();
    view.tree.focus(Some(ids.editor), true);
    view.event(&ContentEvent::Preedit {
        text: "中文组字".into(),
        selection: 4..4,
        clauses: vec![],
        replacement: None,
    })
    .unwrap();
    assert!(view.composing());
    assert!(press(&mut view, ids.attachment).is_empty());
    assert!(press(&mut view, ids.send).is_empty());
    assert!(
        view.begin_file_selection(run.clone(), destination(&run))
            .is_err()
    );
    assert_eq!(fixture.upload_calls(), 0);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn one_shot_independent_child_uses_canonical_admission_gate_for_uploads() {
    let fixture = fixture();
    let (runtime, _, mut view) = setup(fixture.clone()).await;
    view.select(1).unwrap();
    let parent = view.binding().clone();
    let source = ChildPaneBinding::for_run(
        &parent,
        "bounded-b",
        ChildHistoryLayout::IndependentSessions,
        None,
    )
    .await
    .unwrap();
    source.refresh_controls().await.unwrap();
    let run = source.binding();
    assert!(!run.current_run().unwrap().can_submit);
    assert!(!run.supports_attachments());
    assert!(run.begin_attachments(&destination(run)).is_err());
    assert_eq!(fixture.upload_calls(), 0);
    runtime.stop().await.unwrap();
}
