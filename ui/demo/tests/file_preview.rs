use serde_json::json;
use std::{sync::Arc, time::Duration};
use tekes_agent_components_ui::{
    ComposerContextAction, ComposerContextIntent, ComposerContextItem,
};
use tekes_agent_presentation::{ComposerContextBaseline, ComposerDestination, RunScope};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{AttachmentFile, RunBinding};
use tekes_agent_ui_demo::{
    component_demo::{ComponentDemo, SELECT_A, SELECT_B},
    file_preview_host::StagedFilePreview,
    fixture::Fixture,
};
use tekes_ui_controls::composition::AccessibilityAction;
async fn setup(fixture: Fixture) -> (Arc<SessionRuntime>, [Arc<RunBinding>; 2]) {
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
    let file = AttachmentFile::new(
        "中文.txt".into(),
        "text/plain".into(),
        Arc::from(&b"private bytes"[..]),
    )
    .unwrap();
    a.begin_attachments(&ComposerDestination::run(a.scope().clone()))
        .unwrap()
        .upload(&[file])
        .await
        .unwrap();
    (runtime, [a, b])
}
fn intent(binding: &RunBinding) -> ComposerContextIntent {
    let baseline =
        Arc::new(ComposerContextBaseline::read(&binding.current_run().unwrap()).unwrap());
    ComposerContextIntent {
        item: ComposerContextItem::Artifact(baseline.artifacts[0].clone()),
        baseline,
        action: ComposerContextAction::Open,
    }
}
#[tokio::test]
async fn public_read_preserves_bytes_and_private_staging_lifetime() {
    let fixture = Fixture::default().with_uploads();
    let (runtime, [a, b]) = setup(fixture.clone()).await;
    let request = a.begin_file_preview(&intent(&a)).unwrap();
    a.edit_text(request.destination(), "继续写中文".into())
        .unwrap();
    let preview = request.read().await.unwrap();
    assert_eq!(preview.bytes(), b"private bytes");
    assert_eq!(preview.name(), "中文.txt");
    assert_eq!(preview.media_type(), Some("text/plain"));
    assert!(!format!("{preview:?}").contains("private bytes"));
    let staged = StagedFilePreview::stage(&preview).unwrap();
    let path = staged.path().to_path_buf();
    let parent = path.parent().unwrap().to_path_buf();
    assert_eq!(std::fs::read(&path).unwrap(), b"private bytes");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&parent).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    drop(staged);
    assert!(!path.exists());
    assert!(!parent.exists());
    assert!(b.begin_file_preview(&intent(&a)).is_err());
    assert_eq!(fixture.file_read_calls(), 1);
    assert_eq!(a.current_run().unwrap().draft.text, "继续写中文");
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn context_aba_rejects_late_bytes_without_retry() {
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let fixture = Fixture::default()
        .with_uploads()
        .with_file_read_gate(gate.clone());
    let (runtime, [a, _]) = setup(fixture.clone()).await;
    let request = a.begin_file_preview(&intent(&a)).unwrap();
    let task = tokio::spawn(async move { request.read().await });
    tokio::time::timeout(Duration::from_secs(3), async {
        while fixture.file_read_calls() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let baseline = ComposerContextBaseline::read(&a.current_run().unwrap()).unwrap();
    let artifacts = baseline.artifacts.clone();
    assert!(a.patch_context(&baseline, vec![], vec![]).unwrap());
    let changed = ComposerContextBaseline::read(&a.current_run().unwrap()).unwrap();
    assert!(a.patch_context(&changed, artifacts, vec![]).unwrap());
    gate.add_permits(1);
    assert!(task.await.unwrap().is_err());
    assert_eq!(fixture.file_read_calls(), 1);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn malformed_responses_never_produce_preview_bytes() {
    for response in [
        json!({"attachment":{"attachmentId":"wrong","bytes":13},"data":"cHJpdmF0ZSBieXRlcw=="}),
        json!({"attachment":{"attachmentId":"file-0","bytes":13},"data":"invalid base64"}),
        json!({"attachment":{"attachmentId":"file-0","bytes":12},"data":"cHJpdmF0ZSBieXRlcw=="}),
    ] {
        let fixture = Fixture::default()
            .with_uploads()
            .with_file_read_override(response);
        let (runtime, [a, _]) = setup(fixture.clone()).await;
        assert!(
            a.begin_file_preview(&intent(&a))
                .unwrap()
                .read()
                .await
                .is_err()
        );
        assert_eq!(fixture.file_read_calls(), 1);
        runtime.stop().await.unwrap();
    }
}
#[tokio::test]
async fn host_navigation_aba_cancels_viewer_and_allows_new_explicit_gesture() {
    let (runtime, [a, b]) = setup(Fixture::default().with_uploads()).await;
    let mut view = ComponentDemo::new([a.clone(), b]).unwrap();
    view.set_file_preview_available(true);
    view.sync().unwrap();
    let request = view.begin_file_preview(a.clone(), &intent(&a)).unwrap();
    assert!(view.begin_file_preview(a.clone(), &intent(&a)).is_err());
    view.accessibility_action(SELECT_B, None, AccessibilityAction::Press)
        .unwrap();
    view.accessibility_action(SELECT_A, None, AccessibilityAction::Press)
        .unwrap();
    assert!(!view.complete_file_preview(&request, None).unwrap());
    let next = view.begin_file_preview(a.clone(), &intent(&a)).unwrap();
    view.set_file_preview_available(false);
    assert!(!view.complete_file_preview(&next, None).unwrap());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn legacy_receipt_does_not_guess_a_read_identity() {
    let fixture = Fixture::default().with_uploads();
    let (runtime, [a, _]) = setup(fixture.clone()).await;
    let baseline = ComposerContextBaseline::read(&a.current_run().unwrap()).unwrap();
    let mut artifacts = baseline.artifacts.clone();
    if let tekes_agent_presentation::ComposerArtifact::Attachment {
        facts: Some(facts), ..
    } = &mut artifacts[0]
    {
        facts.attachment_id = None;
    }
    assert!(a.patch_context(&baseline, artifacts, vec![]).unwrap());
    assert!(a.begin_file_preview(&intent(&a)).is_err());
    assert_eq!(fixture.file_read_calls(), 0);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn composition_prevents_a_late_viewer_without_sending_or_clearing_draft() {
    use tekes_ui_controls::composition::ContentEvent;
    let (runtime, [a, b]) = setup(Fixture::default().with_uploads()).await;
    let mut view = ComponentDemo::new([a.clone(), b]).unwrap();
    view.set_file_preview_available(true);
    view.sync().unwrap();
    let request = view.begin_file_preview(a.clone(), &intent(&a)).unwrap();
    view.tree.focus(Some(view.ids().editor), true);
    view.event(&ContentEvent::Preedit {
        text: "中文组字".into(),
        selection: 4..4,
        clauses: vec![],
        replacement: None,
    })
    .unwrap();
    assert!(view.composing());
    assert!(!view.complete_file_preview(&request, None).unwrap());
    assert_eq!(a.current_run().unwrap().draft.artifacts.len(), 1);
    assert!(view.begin_file_preview(a.clone(), &intent(&a)).is_err());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn actual_preview_button_routes_through_composer_intent_to_one_read_command() {
    use tekes_agent_ui_demo::component_demo::DemoCommand;
    let (runtime, [a, b]) = setup(Fixture::default().with_uploads()).await;
    let mut view = ComponentDemo::new([a, b]).unwrap();
    view.set_file_preview_available(true);
    view.sync().unwrap();
    let id = view
        .tree
        .semantics()
        .iter()
        .find(|n| n.semantics.name == "预览附件")
        .unwrap()
        .id;
    let mut commands = view
        .accessibility_action(id, None, AccessibilityAction::Press)
        .unwrap();
    assert_eq!(commands.len(), 1);
    let DemoCommand::ReadFilePreview { request } = commands.remove(0) else {
        panic!("read command")
    };
    assert_eq!(request.read().await.unwrap().bytes(), b"private bytes");
    assert!(
        view.accessibility_action(id, None, AccessibilityAction::Press)
            .unwrap()
            .is_empty()
    );
    assert!(view.complete_file_preview(&request, None).unwrap());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn forged_baseline_with_current_revision_cannot_replace_attachment_read_identity() {
    let fixture = Fixture::default().with_uploads();
    let (runtime, [a, _]) = setup(fixture.clone()).await;
    let real = intent(&a);
    assert!(real.baseline.context_revision > 0);
    let mut forged = real.baseline.as_ref().clone();
    if let tekes_agent_presentation::ComposerArtifact::Attachment {
        facts: Some(facts), ..
    } = &mut forged.artifacts[0]
    {
        facts.attachment_id = Some("another-file-in-the-same-session".into());
    }
    let forged = ComposerContextIntent {
        item: ComposerContextItem::Artifact(forged.artifacts[0].clone()),
        baseline: Arc::new(forged),
        action: ComposerContextAction::Open,
    };
    assert!(a.begin_file_preview(&forged).is_err());
    assert_eq!(fixture.file_read_calls(), 0);
    assert_eq!(
        a.begin_file_preview(&real)
            .unwrap()
            .read()
            .await
            .unwrap()
            .bytes(),
        b"private bytes"
    );
    runtime.stop().await.unwrap();
}

async fn wait_read(fixture: &Fixture) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while fixture.file_read_calls() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn reconnect_rejects_inflight_read_and_old_capture_without_an_automatic_second_rpc() {
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let fixture = Fixture::default()
        .with_uploads()
        .with_file_read_gate(gate.clone());
    let (runtime, [a, _]) = setup(fixture.clone()).await;
    let request = Arc::new(a.begin_file_preview(&intent(&a)).unwrap());
    let task = {
        let request = request.clone();
        tokio::spawn(async move { request.read().await })
    };
    wait_read(&fixture).await;
    runtime.reconnect().await.unwrap();
    gate.add_permits(1);
    assert!(task.await.unwrap().is_err());
    assert!(!request.is_current());
    assert!(request.read().await.is_err());
    assert_eq!(fixture.file_read_calls(), 1);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn closed_sidechat_rejects_inflight_preview_without_reading_or_mutating_parent() {
    use tekes_agent_ui_bindings::{PaneAccess, SideChatBinding};
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let fixture = Fixture::default()
        .with_uploads()
        .with_side_chats()
        .with_file_read_gate(gate.clone());
    let (runtime, [parent, _]) = setup(fixture.clone()).await;
    let parent_before = parent.current_run().unwrap().draft.clone();
    let owner = SideChatBinding::new(runtime.clone());
    let chat = owner
        .create(parent.scope().clone(), Some("preview"), "preview-chat")
        .await
        .unwrap();
    let run = owner.open(&chat).await.unwrap();
    let binding = run.binding().clone();
    let file = AttachmentFile::new(
        "侧会话.txt".into(),
        "text/plain".into(),
        Arc::from(&b"child bytes"[..]),
    )
    .unwrap();
    binding
        .begin_attachments(&ComposerDestination::run(binding.scope().clone()))
        .unwrap()
        .upload(&[file])
        .await
        .unwrap();
    let request = binding.begin_file_preview(&intent(&binding)).unwrap();
    let task = tokio::spawn(async move { request.read().await });
    wait_read(&fixture).await;
    owner.close(&chat, "close-during-preview").await.unwrap();
    assert_eq!(binding.access(), PaneAccess::Missing);
    gate.add_permits(1);
    assert!(task.await.unwrap().is_err());
    assert_eq!(fixture.file_read_calls(), 1);
    assert_eq!(parent.current_run().unwrap().draft, parent_before);
    runtime.stop().await.unwrap();
}
