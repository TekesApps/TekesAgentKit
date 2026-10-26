use std::{sync::Arc, time::Duration};
use tekes_agent_presentation::{
    ComposerArtifact, ComposerContextBaseline, ComposerDestination, RunScope, SubmissionPlan,
};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{AttachmentFile, AttachmentOutcome, RunBinding};
use tekes_agent_ui_demo::fixture::Fixture;
use tokio::sync::Semaphore;
async fn setup(fixture: Fixture) -> (Arc<SessionRuntime>, [Arc<RunBinding>; 2]) {
    let runtime = Arc::new(SessionRuntime::spawn("fixture", Arc::new(fixture)));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let mut bindings = vec![];
    for id in ["a", "b"] {
        bindings.push(Arc::new(
            RunBinding::open_main(runtime.clone(), RunScope::main("fixture", id))
                .await
                .unwrap(),
        ));
    }
    (runtime, bindings.try_into().ok().unwrap())
}
fn destination(binding: &RunBinding) -> ComposerDestination {
    ComposerDestination::run(binding.scope().clone())
}
fn file() -> AttachmentFile {
    AttachmentFile::new(
        "中文.txt".into(),
        "text/plain".into(),
        Arc::from(&b"file bytes"[..]),
    )
    .unwrap()
}
async fn wait_call(fixture: &Fixture) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while fixture.upload_calls() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn original_run_receives_real_receipt_and_preserves_newer_text() {
    let gate = Arc::new(Semaphore::new(0));
    let fixture = Fixture::default()
        .with_uploads()
        .with_upload_gate(gate.clone());
    let (runtime, [a, b]) = setup(fixture.clone()).await;
    let request = Arc::new(a.begin_attachments(&destination(&a)).unwrap());
    let task = {
        let request = request.clone();
        tokio::spawn(async move { request.upload(&[file()]).await.unwrap() })
    };
    wait_call(&fixture).await;
    a.edit_text(&destination(&a), "上传期间继续写中文".into())
        .unwrap();
    b.edit_text(&destination(&b), "B 的草稿".into()).unwrap();
    gate.add_permits(1);
    let report = task.await.unwrap();
    assert_eq!(report.outcome, AttachmentOutcome::Attached);
    assert_eq!(report.destination, destination(&a));
    assert_eq!(report.receipts[0].file.bytes, 10);
    let snapshot = a.current_run().unwrap();
    assert_eq!(snapshot.draft.text, "上传期间继续写中文");
    assert_eq!(b.current_run().unwrap().draft.text, "B 的草稿");
    assert!(b.current_run().unwrap().draft.artifacts.is_empty());
    let SubmissionPlan::Prompt { content, .. } = snapshot.draft.compile(false).unwrap() else {
        panic!()
    };
    assert_eq!(
        content[1],
        serde_json::json!({"type":"file","receiptId":"receipt-0"})
    );
    assert!(
        !serde_json::to_string(&content)
            .unwrap()
            .contains("byteCount")
    );
    assert!(request.upload(&[file()]).await.is_err());
    assert_eq!(fixture.upload_calls(), 1);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn context_aba_detaches_receipts_instead_of_repopulating_new_draft() {
    let gate = Arc::new(Semaphore::new(0));
    let fixture = Fixture::default()
        .with_uploads()
        .with_upload_gate(gate.clone());
    let (runtime, [a, _]) = setup(fixture.clone()).await;
    let request = a.begin_attachments(&destination(&a)).unwrap();
    let task = tokio::spawn(async move { request.upload(&[file(), file()]).await.unwrap() });
    wait_call(&fixture).await;
    let baseline = ComposerContextBaseline::read(&a.current_run().unwrap()).unwrap();
    let reference = ComposerArtifact::Reference {
        id: "ref".into(),
        mention: Some("引用".into()),
        path: None,
        excerpt: None,
    };
    assert!(a.patch_context(&baseline, vec![reference], vec![]).unwrap());
    let changed = ComposerContextBaseline::read(&a.current_run().unwrap()).unwrap();
    assert!(a.patch_context(&changed, vec![], vec![]).unwrap());
    gate.add_permits(1);
    let report = task.await.unwrap();
    assert_eq!(report.outcome, AttachmentOutcome::Detached);
    assert_eq!(report.receipts.len(), 1);
    assert_eq!(
        fixture.upload_calls(),
        1,
        "second file must not upload after context changes"
    );
    assert!(a.current_run().unwrap().draft.artifacts.is_empty());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn partial_failure_keeps_known_receipt_and_never_reuploads() {
    let fixture = Fixture::default().with_uploads().with_upload_failure(1);
    let (runtime, [a, _]) = setup(fixture.clone()).await;
    let request = a.begin_attachments(&destination(&a)).unwrap();
    let report = request.upload(&[file(), file(), file()]).await.unwrap();
    assert!(matches!(
        report.outcome,
        AttachmentOutcome::Failed {
            uncertain: true,
            ..
        }
    ));
    assert_eq!(report.receipts.len(), 1);
    assert_eq!(fixture.upload_calls(), 2);
    assert!(a.current_run().unwrap().draft.artifacts.is_empty());
    assert!(request.upload(&[file()]).await.is_err());
    assert_eq!(fixture.upload_calls(), 2);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn count_policy_failure_has_no_upload_side_effect() {
    let fixture = Fixture::default().with_uploads();
    let (runtime, [a, _]) = setup(fixture.clone()).await;
    let request = a.begin_attachments(&destination(&a)).unwrap();
    let report = request.upload(&vec![file(); 5]).await.unwrap();
    assert!(matches!(
        report.outcome,
        AttachmentOutcome::Failed {
            uncertain: false,
            ..
        }
    ));
    assert_eq!(fixture.upload_calls(), 0);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn unsupported_endpoints_do_not_offer_picker_and_wrong_destination_is_rejected() {
    let (runtime, [a, b]) = setup(Fixture::default()).await;
    assert!(!a.supports_attachments());
    assert!(a.begin_attachments(&destination(&a)).is_err());
    assert!(a.begin_attachments(&destination(&b)).is_err());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn send_during_upload_does_not_attach_receipt_to_next_draft() {
    let gate = Arc::new(Semaphore::new(0));
    let fixture = Fixture::default()
        .with_uploads()
        .with_upload_gate(gate.clone());
    let (runtime, [a, _]) = setup(fixture.clone()).await;
    a.edit_text(&destination(&a), "先发送这条".into()).unwrap();
    let request = a.begin_attachments(&destination(&a)).unwrap();
    let task = tokio::spawn(async move { request.upload(&[file()]).await.unwrap() });
    wait_call(&fixture).await;
    let _ticket = a
        .prepare_send(&destination(&a), tekes_agent_contract::PromptMode::Queue)
        .unwrap();
    a.edit_text(&destination(&a), "下一条草稿".into()).unwrap();
    gate.add_permits(1);
    let report = task.await.unwrap();
    assert_eq!(report.outcome, AttachmentOutcome::Detached);
    assert_eq!(report.receipts.len(), 1);
    assert_eq!(a.current_run().unwrap().draft.text, "下一条草稿");
    assert!(a.current_run().unwrap().draft.artifacts.is_empty());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn reconnect_invalidates_picker_capture_without_uploading() {
    let fixture = Fixture::default().with_uploads();
    let (runtime, [a, _]) = setup(fixture.clone()).await;
    let request = a.begin_attachments(&destination(&a)).unwrap();
    runtime.reconnect().await.unwrap();
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    assert_eq!(
        request.upload(&[file()]).await.unwrap().outcome,
        AttachmentOutcome::Detached
    );
    assert_eq!(fixture.upload_calls(), 0);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn retired_sidechat_cannot_upload_or_fall_back_to_parent() {
    use tekes_agent_ui_bindings::SideChatBinding;
    let fixture = Fixture::default().with_uploads().with_side_chats();
    let (runtime, [parent, _]) = setup(fixture.clone()).await;
    let owner = SideChatBinding::new(runtime.clone());
    let chat = owner
        .create(parent.scope().clone(), None, "attachment-chat")
        .await
        .unwrap();
    let run = owner.open(&chat).await.unwrap();
    let binding = run.binding();
    let request = binding.begin_attachments(&destination(binding)).unwrap();
    owner.close(&chat, "close-attachment-chat").await.unwrap();
    assert_eq!(
        request.upload(&[file()]).await.unwrap().outcome,
        AttachmentOutcome::Detached
    );
    assert_eq!(fixture.upload_calls(), 0);
    assert!(parent.current_run().unwrap().draft.artifacts.is_empty());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn upload_watch_publishes_true_operation_stages_and_selection_cancel_never_cancels_network() {
    use tekes_agent_presentation::AttachmentTransferPhase as Phase;
    let gate = Arc::new(Semaphore::new(0));
    let fixture = Fixture::default()
        .with_uploads()
        .with_upload_gate(gate.clone());
    let (runtime, [a, _]) = setup(fixture.clone()).await;
    let request = Arc::new(a.begin_attachments(&destination(&a)).unwrap());
    let mut updates = request.subscribe();
    assert_eq!(updates.borrow().phase, Phase::Selecting);
    let task = {
        let request = request.clone();
        tokio::spawn(async move { request.upload(&[file()]).await.unwrap() })
    };
    wait_call(&fixture).await;
    updates.changed().await.unwrap();
    assert_eq!(
        updates.borrow().phase,
        Phase::Uploading { index: 1, total: 1 }
    );
    assert!(
        !request.cancel_selection(),
        "no fabricated network cancellation after upload starts"
    );
    gate.add_permits(1);
    let report = task.await.unwrap();
    assert_eq!(report.outcome, AttachmentOutcome::Attached);
    assert_eq!(request.snapshot().phase, Phase::Attached { count: 1 });
    let cancelled = a.begin_attachments(&destination(&a)).unwrap();
    assert!(cancelled.cancel_selection());
    assert!(!cancelled.cancel_selection());
    assert!(cancelled.upload(&[file()]).await.is_err());
    assert_eq!(cancelled.snapshot().phase, Phase::Cancelled);
    assert_eq!(fixture.upload_calls(), 1);
    runtime.stop().await.unwrap();
}
