//! Phase 2 Workspace & Inspector Containers tests: Files (`TreeContent` DFS pre-order + Search +
//! Attach), Changes (`WorkingTree` vs `TurnChanges` + revision fencing + capability-gated Commit),
//! `ArtifactViewerContainer` (`Multiple` factory, UTF-16 `ArtifactSelectionReference`, PNG Metal GPU
//! upload, Binary/Truncated/Error), and `cap-std` confined `ReadOnlyFsWorkspaceEndpoint`.

use std::sync::Arc;
use std::time::Duration;
use tekes_agent_apphost::{
    AgentRunContainer, ArtifactViewerContainer, ArtifactViewerProvider, SharedAgentWorkbench,
    WorkspaceChangesContainer, WorkspaceFilesContainer,
};
use tekes_agent_components_ui::{
    ARTIFACT_BODY_SLOT_ID, ARTIFACT_CODE_EDITOR_ID, ARTIFACT_HEADER_ID, ARTIFACT_IMAGE_ID,
    ARTIFACT_MARKDOWN_RICH_ID, CHANGES_COMMIT_BTN_ID, CHANGES_HEADER_ID, CHANGES_QUOTE_BTN_ID,
    COMPOSER_CHIPS_ROW_ID, FILES_HEADER_ID, FILES_TREE_ID,
};
use tekes_agent_contract::{
    ArtifactSelectionReference, ArtifactSelectionResolution, EndpointConnector,
    Utf16SelectionRange, WorkspaceMethod, WorkspaceServiceEndpoint,
};
use tekes_agent_presentation::RunController;
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{
    ChangesViewMode, DEFAULT_MAIN_SESSION_ID, DeterministicWorkspaceEndpoint,
    FixtureSessionEndpoint, LoadedArtifact, LoadedArtifactContent, ReadOnlyFsWorkspaceEndpoint,
    RunWorkbenchBinding, UiWakeNotifier, WorkspaceBinding,
};
use tekes_ui_app_host::{AppWindow, OthersOptions, PresentationOptions};
use tekes_ui_controls::composition::{
    Button, ComponentId, Content, ImageContent, ImageLoadStatus, RichContent, TextContent,
    TextFieldContent, TreeContent,
};
use tekes_ui_render::Renderer;

struct TestCadProvider;

impl ArtifactViewerProvider for TestCadProvider {
    fn supports_path(&self, path: &str) -> bool {
        path.ends_with(".cadspec")
    }

    fn build_custom_content(&self, artifact: &LoadedArtifact) -> Option<(Content, String)> {
        if let LoadedArtifactContent::CodeText { text, .. } = &artifact.content {
            let summary = format!("CAD 模型定制视图: {} ({} bytes)", artifact.path, text.len());
            Some((
                Content::new(ComponentId(1401), TextContent::new(summary.clone())),
                summary,
            ))
        } else {
            None
        }
    }
}

fn setup_phase2_workbench() -> (
    tokio::runtime::Runtime,
    Arc<DeterministicWorkspaceEndpoint>,
    Arc<WorkspaceBinding>,
    Arc<RunWorkbenchBinding>,
    SharedAgentWorkbench,
) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let sep = FixtureSessionEndpoint::with_timing(Duration::ZERO, Duration::from_millis(5));
    let connector: Arc<dyn EndpointConnector> = Arc::new(sep);
    let (runtime, controller) = rt.block_on(async {
        let runtime = Arc::new(SessionRuntime::spawn("fixture", connector));
        runtime.wait_ready(Duration::from_secs(5)).await.unwrap();
        let controller = Arc::new(RunController::new(Arc::clone(&runtime)).unwrap());
        (runtime, controller)
    });

    let workbench = SharedAgentWorkbench::new();
    let wake: Arc<dyn UiWakeNotifier> = workbench.wake_hub();
    let run_binding = {
        let _guard = rt.enter();
        RunWorkbenchBinding::new(
            "fixture",
            runtime,
            controller,
            rt.handle().clone(),
            Arc::clone(&wake),
        )
    };
    run_binding
        .open_initial_session(DEFAULT_MAIN_SESSION_ID)
        .unwrap();
    workbench.attach_run_binding(Arc::clone(&run_binding));

    let wse = Arc::new(DeterministicWorkspaceEndpoint::new("ws-main"));
    let wse_dyn: Arc<dyn WorkspaceServiceEndpoint> =
        Arc::clone(&wse) as Arc<dyn WorkspaceServiceEndpoint>;
    let ws_binding = WorkspaceBinding::new("ws-main", wse_dyn, rt.handle().clone(), wake);
    ws_binding.refresh_all_blocking().unwrap();
    workbench.attach_workspace_binding(Arc::clone(&ws_binding));

    (rt, wse, ws_binding, run_binding, workbench)
}

#[test]
fn phase2_files_changes_fencing_and_capability_gated_commit() {
    let (_rt, wse, ws, run_binding, workbench) = setup_phase2_workbench();

    // 1. Files container mounts DFS pre-order tree and search updates tree cleanly.
    let mut files = WorkspaceFilesContainer::build("workspace-files", workbench.clone()).unwrap();
    files.with_host(&mut |h| {
        assert!(h.get(FILES_HEADER_ID).is_some());
        let tree = h
            .get(FILES_TREE_ID)
            .unwrap()
            .component::<TreeContent>()
            .unwrap();
        assert!(tree.selected_key().is_some());
    });
    ws.set_file_search_query("architecture".into());
    assert_eq!(ws.visible_tree_items().len(), 2);
    ws.set_file_search_query(String::new());
    assert!(ws.visible_tree_items().len() >= 6);

    // Attach file -> Composer chip appears in AgentRunContainer.
    run_binding.attach_file_path(None, "docs/architecture.md".to_owned());
    assert_eq!(run_binding.run_snapshot().draft.artifacts.len(), 1);

    let mut run_container = AgentRunContainer::build("agent-run", workbench.clone()).unwrap();
    run_container.with_host(&mut |h| {
        let chips_text = format!("{:?}", h.get(COMPOSER_CHIPS_ROW_ID).unwrap());
        assert!(chips_text.contains("architecture.md"));
    });

    // 2. Changes container: Working Tree vs Turn Changes + (workspace_id, revision) fencing + Commit gate.
    ws.set_changes_mode(ChangesViewMode::TurnChanges, Some(DEFAULT_MAIN_SESSION_ID));
    let turn_snap = ws.snapshot_changes();
    assert_eq!(turn_snap.mode, ChangesViewMode::TurnChanges);
    assert!(turn_snap.turn_changes.is_some());
    ws.set_changes_mode(ChangesViewMode::WorkingTree, Some(DEFAULT_MAIN_SESSION_ID));

    // Stale revision/workspace diff updates must be fenced out!
    ws.select_change_and_load_diff("src/lib.rs".into());
    assert!(!ws.apply_diff_if_current("ws-other", "r1", "src/lib.rs", "stale other ws"));
    assert!(!ws.apply_diff_if_current("ws-main", "r999", "src/lib.rs", "stale rev"));
    assert!(ws.apply_diff_if_current("ws-main", "r1", "src/lib.rs", "+ current diff"));

    // Capability-gated Commit: disabled when mutations are disabled.
    wse.set_mutations_enabled(false);
    ws.refresh_all_blocking().unwrap();
    let mut changes =
        WorkspaceChangesContainer::build("workspace-changes", workbench.clone()).unwrap();
    changes.with_host(&mut |h| {
        assert!(h.get(CHANGES_HEADER_ID).is_some());
        assert!(h.get(CHANGES_QUOTE_BTN_ID).is_some());
        let commit_btn = h
            .get(CHANGES_COMMIT_BTN_ID)
            .unwrap()
            .component::<Button>()
            .unwrap();
        assert!(commit_btn.button.state.disabled);
    });

    // Re-enable mutations -> commit succeeds & status updates.
    wse.set_mutations_enabled(true);
    ws.refresh_all_blocking().unwrap();
    assert!(ws.can_commit());
    ws.commit_selected("test: verify Phase 2 git commit gate")
        .unwrap();
    assert_eq!(
        wse.committed_messages(),
        vec!["test: verify Phase 2 git commit gate".to_owned()]
    );
}

#[test]
fn phase2_artifact_multiple_factory_quote_staleness_png_gpu_and_cap_std_fs() {
    let (_rt, wse, ws, run_binding, workbench) = setup_phase2_workbench();

    // 1. Code artifact -> capture UTF-16 ArtifactSelectionReference -> Exact -> advance_file_revision -> Stale.
    let code_art = ws.load_artifact_blocking("src/lib.rs");
    let LoadedArtifactContent::CodeText { text, .. } = &code_art.content else {
        panic!("expected CodeText for src/lib.rs");
    };
    let needle = "mount_agent_workbench";
    let byte_idx = text.find(needle).unwrap();
    let utf16_loc = text[..byte_idx].encode_utf16().count() as u64;
    let utf16_len = needle.encode_utf16().count() as u64;

    let quote = ArtifactSelectionReference::capture(
        format!("ws-main:{}", code_art.path),
        code_art.path.clone(),
        "file".to_owned(),
        vec!["content".into()],
        code_art.revision.clone(),
        code_art.digest.clone(),
        text,
        Utf16SelectionRange {
            location: utf16_loc,
            length: utf16_len,
        },
        16,
        16,
    )
    .unwrap();
    assert_eq!(quote.quote, needle);
    assert_eq!(
        ws.resolve_quote_reference(&quote),
        ArtifactSelectionResolution::Exact
    );
    run_binding.apply_quote(tekes_agent_ui_bindings::ComposerQuoteRequest {
        destination: None,
        reference: quote.clone(),
        comment: "检查此函数签名".into(),
    });
    assert_eq!(run_binding.run_snapshot().draft.annotations.len(), 1);

    // Advance file revision -> resolves to Stale!
    wse.advance_file_revision("src/lib.rs", "pub fn renamed_workbench() {}\n");
    ws.load_artifact_blocking("src/lib.rs");
    assert_eq!(
        ws.resolve_quote_reference(&quote),
        ArtifactSelectionResolution::Stale
    );

    // 2. Verify Binary, Truncated, Error, Markdown, & Custom provider artifact states.
    assert!(matches!(
        ws.load_artifact_blocking("bin/firmware.bin").content,
        LoadedArtifactContent::Binary { .. }
    ));
    assert!(matches!(
        ws.load_artifact_blocking("logs/huge.log").content,
        LoadedArtifactContent::Truncated {
            size: 16_777_216,
            ..
        }
    ));
    assert!(matches!(
        ws.load_artifact_blocking("missing/no_such_file.rs").content,
        LoadedArtifactContent::Error(_)
    ));

    let provider: Arc<dyn ArtifactViewerProvider> = Arc::new(TestCadProvider);

    // Build custom instance via ArtifactViewerContainer::build on .cadspec.
    workbench.set_artifact_target("viewer-cad", "specs/chassis.cadspec");
    let mut cad_viewer = ArtifactViewerContainer::build(
        "viewer-cad",
        workbench.clone(),
        Some(Arc::clone(&provider)),
    )
    .unwrap();
    cad_viewer.with_host(&mut |h| {
        assert!(h.get(ARTIFACT_BODY_SLOT_ID).is_some());
        let txt = h
            .get(ComponentId(1401))
            .unwrap()
            .component::<TextContent>()
            .unwrap()
            .text
            .clone();
        assert!(txt.contains("CAD 模型定制视图"));
    });

    // Build markdown instance.
    workbench.set_artifact_target("viewer-md", "docs/architecture.md");
    let mut md_viewer =
        ArtifactViewerContainer::build("viewer-md", workbench.clone(), None).unwrap();
    md_viewer.with_host(&mut |h| {
        assert!(
            h.get(ARTIFACT_MARKDOWN_RICH_ID)
                .unwrap()
                .component::<RichContent>()
                .is_some()
        );
    });

    // Build PNG image instance and verify ImageContent mounts cleanly before GPU render.
    workbench.set_artifact_target("viewer-png", "assets/preview.png");
    let mut png_viewer =
        ArtifactViewerContainer::build("viewer-png", workbench.clone(), None).unwrap();
    png_viewer.with_host(&mut |h| {
        assert!(
            h.get(ARTIFACT_IMAGE_ID)
                .unwrap()
                .component::<ImageContent>()
                .is_some()
        );
    });
    let files_container =
        WorkspaceFilesContainer::build("workspace-files", workbench.clone()).unwrap();
    let changes_container =
        WorkspaceChangesContainer::build("workspace-changes", workbench.clone()).unwrap();
    let run_container = AgentRunContainer::build("agent-run", workbench.clone()).unwrap();

    // Register ContainerFactory (Multiple) WITHOUT duplicating its ID in OthersOptions.groups!
    let artifact_factory =
        ArtifactViewerContainer::factory("artifact-viewer", workbench.clone(), Some(provider));

    let presentation = PresentationOptions::new("agent-run")
        .workspace_switch(true)
        .others(OthersOptions::new(vec![
            "workspace-files".to_owned(),
            "workspace-changes".to_owned(),
            "viewer-png".to_owned(),
        ]));

    let app = AppWindow::from_containers(
        vec![
            run_container.into(),
            files_container.into(),
            changes_container.into(),
            png_viewer.into(),
        ],
        presentation,
    )
    .unwrap()
    .container_factories(vec![artifact_factory])
    .unwrap();

    let (_menus, mut hosted) = app.into_window_handler().unwrap();
    let mut gpu = Renderer::new_headless().unwrap();
    let rgba = hosted
        .render_headless_rgba8(&mut gpu, (1280, 800), 1)
        .unwrap();
    assert_eq!(rgba.len(), 1280 * 800 * 4);

    // 3. Also verify standalone code viewer editor mounts properly.
    workbench.set_artifact_target("viewer-code", "src/lib.rs");
    let mut code_viewer =
        ArtifactViewerContainer::build("viewer-code", workbench.clone(), None).unwrap();
    code_viewer.with_host(&mut |h| {
        assert!(h.get(ARTIFACT_HEADER_ID).is_some());
        assert!(
            h.get(ARTIFACT_CODE_EDITOR_ID)
                .unwrap()
                .component::<TextFieldContent>()
                .is_some()
        );
    });

    // Keep ImageContent / ImageLoadStatus exercised cleanly too.
    let _ = (ImageContent::loading, ImageLoadStatus::Ready);

    // 4. Verify ReadOnlyFsWorkspaceEndpoint cap-std confinement & path traversal rejection.
    let temp_dir =
        std::env::temp_dir().join(format!("tekes-agentkit-capstd-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(temp_dir.join("sub")).unwrap();
    std::fs::write(temp_dir.join("sub/hello.txt"), b"cap-std confined ok").unwrap();

    let fs_ep = ReadOnlyFsWorkspaceEndpoint::open("ws-fs", &temp_dir).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let traversal_err = rt.block_on(fs_ep.call(
        WorkspaceMethod::FilesRead,
        serde_json::json!({ "workspaceId": "ws-fs", "path": "../etc/passwd" }),
        "rpc-trav",
    ));
    assert!(traversal_err.is_err(), "parent traversal must be rejected");

    let valid_read = rt
        .block_on(fs_ep.call(
            WorkspaceMethod::FilesRead,
            serde_json::json!({ "workspaceId": "ws-fs", "path": "sub/hello.txt" }),
            "rpc-ok",
        ))
        .unwrap();
    assert_eq!(valid_read["encoding"], "base64");

    let _ = std::fs::remove_dir_all(&temp_dir);
}
