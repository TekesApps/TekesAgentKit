use std::{sync::Arc, time::Duration};
use tekes_agent_components_ui::ComposerUploads;
use tekes_agent_presentation::{
    AttachmentTransferPhase as Phase, AttachmentTransferSnapshot, ComposerDestination, RunScope,
};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{AttachmentFile, AttachmentOutcome, RunBinding};
use tekes_agent_ui_demo::{
    component_demo::{ComponentDemo, DemoCommand, SELECT_A, SELECT_B},
    fixture::Fixture,
};
use tekes_ui_controls::composition::*;
fn snapshot(revision: u64, phase: Phase) -> Arc<AttachmentTransferSnapshot> {
    Arc::new(AttachmentTransferSnapshot {
        revision,
        request_id: "one".into(),
        destination: ComposerDestination::run(RunScope::main("fixture", "a")),
        phase,
    })
}
fn component(host: &mut ContentHost, root: ComponentId) -> &mut ComposerUploads {
    host.get_mut(root)
        .unwrap()
        .component_mut::<ComposerUploads>()
        .unwrap()
}
#[test]
fn status_rejects_foreign_and_old_facts_and_does_not_cancel_active_upload() {
    let mut status = ComposerUploads::new(95, Default::default());
    let root = status.id();
    let dismiss = status.dismiss_id();
    status.update(snapshot(1, Phase::Selecting)).unwrap();
    let mut host = status.into_host().unwrap();
    host.accessibility_action(dismiss, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    assert!(component(&mut host, root).take_intents(&actions).is_empty());
    component(&mut host, root)
        .update(snapshot(2, Phase::Uploading { index: 1, total: 2 }))
        .unwrap();
    assert!(
        host.semantics()
            .iter()
            .any(|n| n.semantics.name.contains("1/2"))
    );
    let mut foreign = (*snapshot(3, Phase::Attached { count: 2 })).clone();
    foreign.request_id = "other".into();
    assert!(
        !component(&mut host, root)
            .update(Arc::new(foreign))
            .unwrap()
    );
    assert!(
        !component(&mut host, root)
            .update(snapshot(1, Phase::Cancelled))
            .unwrap()
    );
    component(&mut host, root)
        .update(snapshot(
            3,
            Phase::Failed {
                completed: 1,
                uncertain: true,
                message: "失联".into(),
            },
        ))
        .unwrap();
    assert!(
        host.semantics()
            .iter()
            .any(|n| n.semantics.name.contains("不会自动重传"))
    );
    assert!(
        !component(&mut host, root)
            .update(snapshot(4, Phase::Selecting))
            .unwrap()
    );
    component(&mut host, root).set_interactions_enabled(false);
    host.accessibility_action(dismiss, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    assert!(component(&mut host, root).take_intents(&actions).is_empty());
    component(&mut host, root).set_interactions_enabled(true);
    host.accessibility_action(dismiss, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    let intents = component(&mut host, root).take_intents(&actions);
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0].snapshot.request_id, "one");
    assert!(component(&mut host, root).take_intents(&actions).is_empty());
}
async fn setup(fixture: Fixture) -> (Arc<SessionRuntime>, ComponentDemo) {
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
    let mut view = ComponentDemo::new([a, b]).unwrap();
    view.set_file_picker_available(true);
    view.sync().unwrap();
    (runtime, view)
}
fn press(view: &mut ComponentDemo, id: ComponentId) -> Vec<DemoCommand> {
    view.accessibility_action(id, None, AccessibilityAction::Press)
        .unwrap()
}
fn file() -> AttachmentFile {
    AttachmentFile::new(
        "中文.txt".into(),
        "text/plain".into(),
        Arc::from(&b"hello"[..]),
    )
    .unwrap()
}
#[tokio::test]
async fn captured_picker_gates_only_its_source_and_late_completion_does_not_steal_navigation() {
    let (runtime, mut view) = setup(Fixture::default().with_uploads()).await;
    let a = view.bindings()[0].clone();
    let b = view.bindings()[1].clone();
    a.edit_text(
        &ComposerDestination::run(a.scope().clone()),
        "A 草稿".into(),
    )
    .unwrap();
    view.sync().unwrap();
    let attachment = view.ids().attachment;
    let mut commands = press(&mut view, attachment);
    let DemoCommand::SelectFiles { request } = commands.remove(0) else {
        panic!()
    };
    assert!(view.upload_pending(&a));
    assert!(!view.upload_pending(&b));
    let send = view.ids().send;
    assert!(press(&mut view, send).is_empty());
    assert!(
        view.begin_file_selection(a.clone(), ComposerDestination::run(a.scope().clone()))
            .is_err()
    );
    assert!(press(&mut view, SELECT_B).is_empty());
    assert_eq!(view.selected(), 1);
    b.edit_text(
        &ComposerDestination::run(b.scope().clone()),
        "B 草稿".into(),
    )
    .unwrap();
    view.sync().unwrap();
    let report = request.upload(&[file()]).await.unwrap();
    assert_eq!(report.outcome, AttachmentOutcome::Attached);
    view.complete_file_selection(&request, vec![file()], Ok(Some(report)))
        .unwrap();
    assert_eq!(view.selected(), 1);
    assert_eq!(b.current_run().unwrap().draft.text, "B 草稿");
    assert!(b.current_run().unwrap().draft.artifacts.is_empty());
    assert!(
        view.tree
            .semantics()
            .iter()
            .all(|n| !n.semantics.name.contains("附件已加入原会话草稿"))
    );
    press(&mut view, SELECT_A);
    assert!(
        view.tree
            .semantics()
            .iter()
            .any(|n| n.semantics.name.contains("附件已加入原会话草稿"))
    );
    assert_eq!(a.current_run().unwrap().draft.text, "A 草稿");
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn selection_cancel_is_terminal_has_no_upload_and_stale_status_id_cannot_dismiss_next_request()
 {
    let fixture = Fixture::default().with_uploads();
    let (runtime, mut view) = setup(fixture.clone()).await;
    let binding = view.binding().clone();
    let target = ComposerDestination::run(binding.scope().clone());
    let request = view
        .begin_file_selection(binding.clone(), target.clone())
        .unwrap();
    assert!(request.cancel_selection());
    view.complete_file_selection(&request, vec![], Ok(None))
        .unwrap();
    assert!(!view.upload_pending(&binding));
    assert_eq!(fixture.upload_calls(), 0);
    assert!(request.upload(&[file()]).await.is_err());
    let dismiss = view
        .tree
        .semantics()
        .iter()
        .find(|n| n.semantics.name == "关闭附件状态")
        .unwrap()
        .id;
    press(&mut view, dismiss);
    let next = view.begin_file_selection(binding.clone(), target).unwrap();
    assert_ne!(next.request_id(), request.request_id());
    press(&mut view, dismiss);
    assert!(view.upload_pending(&binding));
    next.cancel_selection();
    view.complete_file_selection(&next, vec![], Ok(None))
        .unwrap();
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn chinese_preedit_cannot_open_a_file_picker_or_send() {
    let fixture = Fixture::default().with_uploads();
    let (runtime, mut view) = setup(fixture.clone()).await;
    let binding = view.binding().clone();
    let ids = view.ids();
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
        view.begin_file_selection(
            binding.clone(),
            ComposerDestination::run(binding.scope().clone())
        )
        .is_err()
    );
    assert_eq!(fixture.upload_calls(), 0);
    assert!(!view.upload_pending(&binding));
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn host_completion_rejects_mismatched_report_and_premature_success() {
    let (runtime, mut view) = setup(Fixture::default().with_uploads()).await;
    let binding = view.binding().clone();
    let request = view
        .begin_file_selection(
            binding.clone(),
            ComposerDestination::run(binding.scope().clone()),
        )
        .unwrap();
    assert!(
        view.complete_file_selection(&request, vec![], Ok(None))
            .is_err()
    );
    let mut report = request.upload(&[file()]).await.unwrap();
    report.destination = ComposerDestination::run(RunScope::main("fixture", "b"));
    assert!(
        view.complete_file_selection(&request, vec![file()], Ok(Some(report)))
            .is_err()
    );
    assert!(view.upload_report(binding.scope()).is_none());
    runtime.stop().await.unwrap();
}
