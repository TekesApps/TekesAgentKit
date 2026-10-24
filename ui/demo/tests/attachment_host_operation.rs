use std::{fs, path::PathBuf, sync::Arc, time::Duration};
use tekes_agent_presentation::{AttachmentTransferPhase, ComposerDestination};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::RunBinding;
use tekes_agent_ui_demo::{
    attachment_files::select_and_upload,
    component_demo::{ComponentDemo, SELECT_B},
    fixture::Fixture,
};
use tekes_ui_controls::composition::AccessibilityAction;

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

struct OwnedFile(PathBuf);

impl OwnedFile {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "agentkit-host-operation-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("host-operation.txt");
        fs::write(&path, b"host operation proof").unwrap();
        Self(path)
    }
}

impl Drop for OwnedFile {
    fn drop(&mut self) {
        if let Some(parent) = self.0.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }
}

async fn setup(fixture: Fixture) -> (Arc<SessionRuntime>, ComponentDemo) {
    let runtime = Arc::new(SessionRuntime::spawn("fixture", Arc::new(fixture)));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let a = Arc::new(
        RunBinding::open_main(
            runtime.clone(),
            tekes_agent_presentation::RunScope::main("fixture", "a"),
        )
        .await
        .unwrap(),
    );
    let b = Arc::new(
        RunBinding::open_main(
            runtime.clone(),
            tekes_agent_presentation::RunScope::main("fixture", "b"),
        )
        .await
        .unwrap(),
    );
    let mut view = ComponentDemo::new([a, b]).unwrap();
    view.set_file_picker_available(true);
    view.sync().unwrap();
    (runtime, view)
}

#[tokio::test]
async fn shared_host_operation_turns_picker_cancel_into_terminal_completion() {
    let fixture = Fixture::default().with_uploads();
    let (runtime, mut view) = setup(fixture.clone()).await;
    let binding = view.binding().clone();
    let request = view
        .begin_file_selection(
            binding.clone(),
            ComposerDestination::run(binding.scope().clone()),
        )
        .unwrap();

    let completion = select_and_upload(request.clone(), None).await;
    assert!(completion.report().is_none());
    assert!(completion.files().is_empty());
    assert!(matches!(
        &request.snapshot().phase,
        AttachmentTransferPhase::Cancelled
    ));
    assert!(completion.apply(&mut view).unwrap());
    assert!(!view.upload_pending(&binding));
    assert_eq!(fixture.upload_calls(), 0);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn shared_host_operation_marks_read_errors_and_applies_failure_to_original_request() {
    let fixture = Fixture::default().with_uploads();
    let (runtime, mut view) = setup(fixture.clone()).await;
    let binding = view.binding().clone();
    let request = view
        .begin_file_selection(
            binding.clone(),
            ComposerDestination::run(binding.scope().clone()),
        )
        .unwrap();
    let missing = std::env::temp_dir().join(format!(
        "agentkit-host-operation-missing-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));

    let completion = select_and_upload(request.clone(), Some(vec![missing])).await;
    assert!(completion.report().is_none());
    assert!(completion.files().is_empty());
    assert!(matches!(
        &request.snapshot().phase,
        AttachmentTransferPhase::Failed { .. }
    ));
    assert!(completion.apply(&mut view).unwrap());
    assert!(!view.upload_pending(&binding));
    assert_eq!(fixture.upload_calls(), 0);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn shared_host_completion_keeps_source_identity_after_navigation() {
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let fixture = Fixture::default()
        .with_uploads()
        .with_upload_gate(gate.clone());
    let (runtime, mut view) = setup(fixture.clone()).await;
    let source = view.bindings()[0].clone();
    let other = view.bindings()[1].clone();
    let request = view
        .begin_file_selection(
            source.clone(),
            ComposerDestination::run(source.scope().clone()),
        )
        .unwrap();
    let file = OwnedFile::new();

    let task = tokio::spawn(select_and_upload(
        request.clone(),
        Some(vec![file.0.clone()]),
    ));
    tokio::time::timeout(Duration::from_secs(3), async {
        while fixture.upload_calls() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(view.upload_pending(&source));
    view.accessibility_action(SELECT_B, None, AccessibilityAction::Press)
        .unwrap();
    other
        .edit_text(
            &ComposerDestination::run(other.scope().clone()),
            "other session draft".into(),
        )
        .unwrap();

    gate.add_permits(1);
    let completion = task.await.unwrap();
    assert!(completion.report().is_some());
    assert!(Arc::ptr_eq(completion.request(), &request));
    assert!(completion.apply(&mut view).unwrap());
    assert_eq!(view.selected(), 1);
    assert_eq!(
        other.current_run().unwrap().draft.text,
        "other session draft"
    );
    assert!(other.current_run().unwrap().draft.artifacts.is_empty());
    assert_eq!(source.current_run().unwrap().draft.artifacts.len(), 1);
    assert_eq!(fixture.upload_calls(), 1);
    runtime.stop().await.unwrap();
}
