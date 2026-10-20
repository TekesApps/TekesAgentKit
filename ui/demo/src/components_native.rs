//! Native host of the actual public Composer/Indicators and RunBinding.
//! This is a component integration window, not the final RunView or Transcript.
use super::native::{TextBackend, input_event, publish_content, window_renderer};
use std::{
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
use tekes_agent_components_ui::{
    RenameFailure, RenameSubmission, SidePanel, Sidebar, SidebarIntent,
};
use tekes_agent_contract::Result;
use tekes_agent_presentation::{ChildHistoryLayout, RunScope};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{
    BoundSideChatRun, ChildPaneBinding, InitialPresetChoices, ModelChoices,
    ModelSelectionPreparation, PaneAccess, PaneSnapshot, RunBinding, SideChatBinding,
    SidebarActionOutcome, SidebarBinding,
};
use tekes_agent_ui_demo::component_demo::{
    ComponentDemo, CreateSessionCommand, DemoCommand, NavigationRequest, PanelRequest, RECONNECT,
    SidebarActionToken,
};
use tekes_agent_web::SharedAgent;
use tekes_ui_controls::composition::{AccessibilityAction, Component, ComponentId, ContentEvent};
use tekes_ui_platform_macos::{
    App, AppConfig, Event, MenuBar, Size, Window, WindowConfig, WindowHandler,
};
use tekes_ui_render::Renderer;
use tekes_ui_scene::{GlyphAtlas, Rect, Scene};
use tekes_ui_text::coretext::{CoreTextFonts, CoreTextRasterizer, CoreTextShaper};
use tekes_ui_tokens::{Palette, Theme};

struct OwnedFilePreview {
    child: std::process::Child,
    _staged: tekes_agent_ui_demo::file_preview_host::StagedFilePreview,
}
impl Drop for OwnedFilePreview {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
struct Host {
    clipboard_writes: Vec<tekes_agent_components_ui::TranscriptCopyIntent>,
    file_viewer: Option<OwnedFilePreview>,
    view: ComponentDemo,
    text: TextBackend,
    scene: Scene,
    palette: Palette,
    gpu: Option<Renderer>,
    executor: tokio::runtime::Runtime,
    runtime: Arc<SessionRuntime>,
    sidebar: Arc<SidebarBinding>,
    side_chats: Arc<SideChatBinding>,
    sender: mpsc::Sender<HostResult>,
    results: mpsc::Receiver<HostResult>,
    wake: Option<tekes_ui_platform_macos::WindowWake>,
    observers: Vec<tokio::task::JoinHandle<()>>,
    selftest: bool,
    system_ime_selftest: bool,
    system_ime_initial_users: usize,
    failures: Vec<String>,
    focus_before_deactivate: Option<ComponentId>,
    _owned_preferences: Option<OwnedPreferencesDirectory>,
    sidebar_error: Option<String>,
    side_chat_test_started: bool,
    side_chat_test_session: Option<String>,
    task_test_requested: bool,
    task_test_started: bool,
    child_layout: ChildHistoryLayout,
    child_test_requested: bool,
    child_test_started: bool,
    bounded_test_requested: bool,
    bounded_test_started: bool,
    test_child_source: Option<Arc<ChildPaneBinding>>,
    test_bounded_source: Option<Arc<ChildPaneBinding>>,
    bounded_initial_items: usize,
    bounded_max_items: usize,
    bounded_rendered_items: usize,
    rename_rpc_prefix: String,
    actions_selftest: bool,
    turn_footnote_selftest: bool,
    rename_test_started: bool,
    fork_test_started: bool,
    archive_test_started: bool,
    empty_selection_seen: bool,
    fork_test_session: Option<String>,
    models_selftest: bool,
    model_test_stage: u8,
    context_selftest: bool,
    context_stage: u8,
    creation_selftest: bool,
    creation_stage: u8,
    creation_scope: Option<RunScope>,
    annotation_selftest: bool,
    annotation_stage: u8,
}
enum HostResult {
    TurnAction {
        token: SidebarActionToken,
        result: std::result::Result<tekes_agent_contract::ThreadActionOutcome, String>,
    },
    HistoryPageCompleted,
    Actionable {
        ids: tekes_agent_components_ui::ComposerIds,
        intent: tekes_agent_components_ui::ActionableResponseIntent,
        result: std::result::Result<(), String>,
    },
    FilePreview {
        request: Arc<tekes_agent_ui_bindings::FilePreviewRequest>,
        result:
            std::result::Result<tekes_agent_ui_demo::file_preview_host::StagedFilePreview, String>,
    },
    Files {
        request: Arc<tekes_agent_ui_bindings::AttachmentSelection>,
        files: Vec<tekes_agent_ui_bindings::AttachmentFile>,
        result: std::result::Result<Option<tekes_agent_ui_bindings::AttachmentReport>, String>,
    },
    InitialPresets {
        request: tekes_agent_components_ui::ComposerChoiceRequest,
        result: std::result::Result<InitialPresetChoices, String>,
    },
    Created {
        request: CreateSessionCommand,
        result: std::result::Result<Arc<RunBinding>, String>,
    },
    ModelCatalog {
        request: tekes_agent_components_ui::ComposerChoiceRequest,
        result: std::result::Result<ModelChoices, String>,
    },
    ChildOpened {
        request: PanelRequest,
        title: String,
        result: std::result::Result<(Arc<ChildPaneBinding>, PaneSnapshot), String>,
    },
    ChildRead {
        panel_parent: RunScope,
        source: Arc<ChildPaneBinding>,
        result: std::result::Result<PaneSnapshot, String>,
        catalog_error: Option<String>,
    },
    TaskOpened {
        parent: RunScope,
        task_id: String,
        result: std::result::Result<Option<Arc<RunBinding>>, String>,
    },
    SideChatOpened {
        request: PanelRequest,
        result: std::result::Result<BoundSideChatRun, String>,
    },
    Operation(std::result::Result<(), String>),
    Sidebar {
        token: SidebarActionToken,
        result: std::result::Result<SidebarActionOutcome, String>,
    },
    Renamed {
        ticket: RenameSubmission,
        result: std::result::Result<SidebarActionOutcome, RenameFailure>,
    },
    Opened {
        request: NavigationRequest,
        binding: std::result::Result<Arc<RunBinding>, String>,
    },
}
struct OwnedPreferencesDirectory(std::path::PathBuf);
impl OwnedPreferencesDirectory {
    fn new() -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "agentkit-native-demo-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&path).expect("owned demo preferences directory");
        Self(path)
    }
}
impl Drop for OwnedPreferencesDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
impl Host {
    fn verify_bounded_content(&mut self) {
        if !self.selftest {
            return;
        }
        let Some(root) = self.view.bounded_child_content_id() else {
            return;
        };
        let content = self
            .view
            .tree
            .get(root)
            .unwrap()
            .component::<tekes_agent_components_ui::TranscriptContent>()
            .unwrap();
        assert_eq!(content.scope().session_id, "bounded-b");
        assert!(self.view.panel_composer_ids().is_none());
        let messages = content
            .row_keys()
            .iter()
            .filter(|key| {
                matches!(
                    key,
                    tekes_agent_components_ui::TranscriptContentRowKey::Message(_)
                )
            })
            .count();
        assert!(
            messages > 0,
            "native child pane must render actual messages"
        );
        self.bounded_rendered_items = self.bounded_rendered_items.max(messages);
    }

    fn commands(&mut self, result: Result<Vec<DemoCommand>>) {
        let commands = match result {
            Ok(commands) => commands,
            Err(error) => {
                self.failure(error.to_string());
                return;
            }
        };
        for command in commands {
            if let DemoCommand::CopyTranscript(intent) = command {
                self.clipboard_writes.push(intent);
                continue;
            }
            let command = if let DemoCommand::Summary(ref intent) = command
                && let tekes_agent_presentation::SummaryAction::OpenSubagent { parent, child_id } =
                    &intent.action
            {
                let request = match self.view.begin_child_open(&intent.scope) {
                    Ok(request) => request,
                    Err(error) => {
                        self.failure(error.to_string());
                        continue;
                    }
                };
                DemoCommand::OpenChild {
                    request,
                    direct_parent: parent.clone(),
                    child_id: child_id.clone(),
                    title: child_id.clone(),
                    layout: self.child_layout,
                }
            } else {
                command
            };
            let sender = self.sender.clone();
            let wake = self.wake.clone();
            let runtime = self.runtime.clone();
            let sidebar = self.sidebar.clone();
            let side_chats = self.side_chats.clone();
            let cached_child = match &command {
                DemoCommand::OpenChild {
                    request,
                    direct_parent,
                    child_id,
                    layout,
                    ..
                } => self.view.cached_child_source(
                    request.parent(),
                    direct_parent,
                    child_id,
                    *layout,
                ),
                _ => None,
            };
            let sidebar_token = match &command {
                DemoCommand::TurnAction { binding, intent } => {
                    Some(self.view.begin_sidebar_action(
                        tekes_agent_contract::SessionAddress {
                            endpoint_id: binding.scope().endpoint_id.clone(),
                            session_id: binding.scope().session_id.clone(),
                        },
                        intent.action().id.clone(),
                    ))
                }
                DemoCommand::Sidebar(SidebarIntent::Action {
                    address,
                    definition,
                }) => Some(
                    self.view
                        .begin_sidebar_action(address.clone(), definition.id.clone()),
                ),
                _ => None,
            };
            let rename_rpc_prefix = self.rename_rpc_prefix.clone();
            // Construct the native dialog on the main UI thread; await it on
            // the existing executor. RFD is a demo-only host system adapter.
            if let DemoCommand::ReadFilePreview { request } = &command
                && !self.view.file_preview_current(request)
            {
                continue;
            }
            let picker = if let DemoCommand::SelectFiles { request } = &command {
                let mut updates = request.subscribe();
                let progress_wake = self.wake.clone();
                self.observers.push(self.executor.spawn(async move {
                    while updates.borrow().phase.is_pending() {
                        if updates.changed().await.is_err() {
                            break;
                        }
                        if let Some(wake) = &progress_wake {
                            wake.wake();
                        }
                    }
                }));
                Some(
                    rfd::AsyncFileDialog::new()
                        .set_title("选择附件文件")
                        .pick_files(),
                )
            } else {
                None
            };
            self.executor.spawn(async move {
                let special = match command {
                    DemoCommand::ReadHistoryPage(request) => {
                        // Success and failure are published only on the original
                        // binding, never as a global error on the selected pane.
                        let _ = request.load().await;
                        HostResult::HistoryPageCompleted
                    }
                    DemoCommand::RespondActionable {
                        request,
                        ids,
                        intent,
                    } => HostResult::Actionable {
                        ids,
                        intent,
                        result: request.respond().await.map_err(|error| error.to_string()),
                    },
                    DemoCommand::ReadFilePreview { request } => {
                        let result = match request.read().await {
                            Ok(preview) => tokio::task::spawn_blocking(move || {
                                tekes_agent_ui_demo::file_preview_host::StagedFilePreview::stage(
                                    &preview,
                                )
                                .map_err(|e| e.to_string())
                            })
                            .await
                            .unwrap_or_else(|e| Err(e.to_string())),
                            Err(error) => Err(error.to_string()),
                        };
                        HostResult::FilePreview { request, result }
                    }
                    DemoCommand::SelectFiles { request } => {
                        let selected = picker.expect("captured file picker").await;
                        let (files, result) = if let Some(handles) = selected {
                            let paths = handles
                                .iter()
                                .map(|file| file.path().to_path_buf())
                                .collect::<Vec<_>>();
                            match tokio::task::spawn_blocking(move || {
                                tekes_agent_ui_demo::attachment_files::read_selected_files(&paths)
                            })
                            .await
                            {
                                Ok(Ok(files)) => {
                                    let result = request
                                        .upload(&files)
                                        .await
                                        .map(Some)
                                        .map_err(|e| e.to_string());
                                    (files, result)
                                }
                                result => {
                                    let message = match result {
                                        Ok(Err(error)) => error.to_string(),
                                        Err(error) => error.to_string(),
                                        _ => unreachable!(),
                                    };
                                    request.fail_selection(message.clone());
                                    (vec![], Err(message))
                                }
                            }
                        } else {
                            request.cancel_selection();
                            (vec![], Ok(None))
                        };
                        HostResult::Files {
                            request,
                            files,
                            result,
                        }
                    }
                    DemoCommand::TurnAction { binding, intent } => {
                        let rpc = format!(
                            "{rename_rpc_prefix}:turn:{}:{}",
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_nanos(),
                            intent.action().id
                        );
                        let result = match sidebar
                            .turn_action_dispatcher()
                            .prepare(binding, &intent, rpc)
                        {
                            Ok(request) => request.execute().await,
                            Err(error) => Err(error),
                        }
                        .map_err(|error| error.to_string());
                        HostResult::TurnAction {
                            token: sidebar_token.expect("captured turn gesture"),
                            result,
                        }
                    }
                    DemoCommand::ReadInitialPresets { request } => {
                        let rpc =
                            format!("{rename_rpc_prefix}:initial-presets:{}", request.generation);
                        let result = InitialPresetChoices::load(&runtime, 1, &rpc)
                            .await
                            .map_err(|e| e.to_string());
                        HostResult::InitialPresets { request, result }
                    }
                    DemoCommand::CreateSession(request) => {
                        let result = async {
                            let created = request.creation().execute(&runtime).await?;
                            RunBinding::open_main(
                                runtime.clone(),
                                RunScope::main(
                                    request.creation().endpoint_id(),
                                    created.session_id,
                                ),
                            )
                            .await
                            .map(Arc::new)
                        }
                        .await
                        .map_err(|e: tekes_agent_contract::Error| e.to_string());
                        HostResult::Created { request, result }
                    }
                    DemoCommand::ReadModels { binding, request } => {
                        let rpc = format!("{rename_rpc_prefix}:models:{}", request.generation);
                        let result = binding
                            .model_choices(1, &rpc)
                            .await
                            .map_err(|e| e.to_string());
                        HostResult::ModelCatalog { request, result }
                    }
                    DemoCommand::Rename(ticket) => {
                        let rpc_id = format!(
                            "{rename_rpc_prefix}:{}:{}",
                            ticket.generation, ticket.attempt
                        );
                        let result = sidebar
                            .perform(
                                &ticket.address,
                                &ticket.definition,
                                Some(ticket.title.clone()),
                                &rpc_id,
                            )
                            .await
                            .map_err(|error| RenameFailure::from_error(&error));
                        HostResult::Renamed { ticket, result }
                    }
                    DemoCommand::OpenChild {
                        request,
                        direct_parent,
                        child_id,
                        title,
                        layout,
                    } => {
                        let result = async {
                            let source = if let Some(source) = cached_child {
                                source
                            } else {
                                Arc::new(
                                    ChildPaneBinding::open(
                                        runtime.clone(),
                                        direct_parent,
                                        child_id,
                                        layout,
                                        None,
                                    )
                                    .await?,
                                )
                            };
                            let snapshot = source.snapshot().await?;
                            Ok::<_, tekes_agent_contract::Error>((source, snapshot))
                        }
                        .await
                        .map_err(|error| error.to_string());
                        HostResult::ChildOpened {
                            request,
                            title,
                            result,
                        }
                    }
                    DemoCommand::ReadChild {
                        panel_parent,
                        source,
                        page,
                        read_intent,
                    } => {
                        let catalog_error = if !page
                            && source.layout() == ChildHistoryLayout::IndependentSessions
                        {
                            source
                                .refresh_controls()
                                .await
                                .err()
                                .map(|error| error.to_string())
                        } else {
                            None
                        };
                        let result = async {
                            if let Some(intent) = read_intent {
                                return source.read_intent(intent).await;
                            }
                            if page {
                                source.load_earlier().await?;
                            }
                            source.snapshot().await
                        }
                        .await
                        .map_err(|error| error.to_string());
                        HostResult::ChildRead {
                            panel_parent,
                            source,
                            result,
                            catalog_error,
                        }
                    }
                    DemoCommand::OpenTask {
                        parent,
                        task_id,
                        source,
                    } => HostResult::TaskOpened {
                        parent,
                        task_id,
                        result: source
                            .open_resolved()
                            .await
                            .map_err(|error| error.to_string()),
                    },
                    DemoCommand::CreateSideChat(request) => {
                        let title = sidebar
                            .snapshot()
                            .rows
                            .iter()
                            .find(|row| {
                                row.address.endpoint_id == request.parent().endpoint_id
                                    && row.address.session_id == request.parent().session_id
                            })
                            .map(|row| row.title.clone());
                        let rpc = format!(
                            "sidechat-create:{}:{}",
                            std::process::id(),
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_nanos()
                        );
                        let result = async {
                            let chat = side_chats
                                .create(request.parent().clone(), title.as_deref(), &rpc)
                                .await?;
                            side_chats.open(&chat).await
                        }
                        .await
                        .map_err(|error| error.to_string());
                        HostResult::SideChatOpened { request, result }
                    }
                    DemoCommand::OpenSideChat { request, chat } => HostResult::SideChatOpened {
                        request,
                        result: side_chats
                            .open(&chat)
                            .await
                            .map_err(|error| error.to_string()),
                    },
                    DemoCommand::CloseSideChat(expected) => {
                        let rpc = format!(
                            "sidechat-close:{}:{}",
                            std::process::id(),
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_nanos()
                        );
                        HostResult::Operation(
                            side_chats
                                .close(&expected, &rpc)
                                .await
                                .map_err(|error| error.to_string()),
                        )
                    }
                    DemoCommand::OpenSession(request) => {
                        let address = request.address();
                        let binding = RunBinding::open_main(
                            runtime.clone(),
                            RunScope::main(&address.endpoint_id, &address.session_id),
                        )
                        .await
                        .map(Arc::new)
                        .map_err(|error| error.to_string());
                        HostResult::Opened { request, binding }
                    }
                    DemoCommand::Sidebar(SidebarIntent::Action {
                        address,
                        definition,
                    }) => {
                        let rpc_id = format!(
                            "demo:{}:{}:{}",
                            std::process::id(),
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_nanos(),
                            definition.id
                        );
                        HostResult::Sidebar {
                            token: sidebar_token.expect("captured sidebar gesture"),
                            result: sidebar
                                .perform(&address, &definition, None, &rpc_id)
                                .await
                                .map_err(|error| error.to_string()),
                        }
                    }
                    DemoCommand::Sidebar(SidebarIntent::Select { .. }) => {
                        HostResult::Operation(Err("unexpected unbound navigation".into()))
                    }
                    DemoCommand::Summary(_) => HostResult::Operation(Err(
                        "此验证宿主尚未安装 Summary 内容跳转适配器".into(),
                    )),
                    command => {
                        let result = match command {
                            DemoCommand::Send { binding, ticket } => {
                                binding
                                    .dispatch_prepared_with(ticket, &ModelSelectionPreparation)
                                    .await
                            }
                            DemoCommand::Cancel {
                                binding,
                                destination,
                            } => binding.cancel(&destination).await,
                            DemoCommand::Reconnect => match runtime.reconnect().await {
                                Ok(()) => runtime.wait_ready(Duration::from_secs(5)).await,
                                Err(error) => Err(error),
                            },
                            DemoCommand::OpenEntry { .. } => {
                                Err(tekes_agent_contract::unsupported(
                                    "no component demo capability adapter",
                                ))
                            }
                            DemoCommand::OpenContext { .. } => {
                                Err(tekes_agent_contract::unsupported(
                                    "no preview or source reveal host adapter",
                                ))
                            }
                            DemoCommand::RespondActionable { .. }
                            | DemoCommand::ReadHistoryPage(_)
                            | DemoCommand::ReadFilePreview { .. }
                            | DemoCommand::SelectFiles { .. }
                            | DemoCommand::Sidebar(_)
                            | DemoCommand::TurnAction { .. }
                            | DemoCommand::ReadInitialPresets { .. }
                            | DemoCommand::CreateSession(_)
                            | DemoCommand::ReadModels { .. }
                            | DemoCommand::Rename(_)
                            | DemoCommand::OpenChild { .. }
                            | DemoCommand::ReadChild { .. }
                            | DemoCommand::OpenTask { .. }
                            | DemoCommand::CreateSideChat(_)
                            | DemoCommand::OpenSideChat { .. }
                            | DemoCommand::CloseSideChat(_)
                            | DemoCommand::OpenSession(_)
                            | DemoCommand::Summary(_)
                            | DemoCommand::CopyTranscript(_) => unreachable!("handled above"),
                        }
                        .map_err(|error| error.to_string());
                        HostResult::Operation(result)
                    }
                };
                let _ = sender.send(special);
                if let Some(wake) = wake {
                    wake.wake();
                }
            });
        }
    }
    fn failure(&mut self, error: String) {
        eprintln!("COMPONENT_DEMO_ERROR {error}");
        self.view.set_notice(&error);
        self.failures.push(error);
    }
    fn draw(&mut self, window: &Window) {
        for intent in self.clipboard_writes.drain(..) {
            if intent.claim() {
                window.set_clipboard_text(intent.text());
            }
        }
        if self
            .file_viewer
            .as_mut()
            .is_some_and(|viewer| viewer.child.try_wait().ok().flatten().is_some())
        {
            self.file_viewer = None;
        }
        while let Ok(result) = self.results.try_recv() {
            match result {
                HostResult::TurnAction { token, result } => {
                    if let Ok(outcome) = result
                        && let Ok(Some(request)) =
                            self.view.apply_turn_action_outcome(&token, &outcome)
                    {
                        self.commands(Ok(vec![DemoCommand::OpenSession(request)]));
                    }
                }
                HostResult::Actionable {
                    ids,
                    intent,
                    result,
                } => {
                    if let Err(error) = self.view.complete_actionable(ids, &intent, result) {
                        self.failure(error.to_string());
                    }
                }
                HostResult::FilePreview { request, result } => {
                    let error = result.as_ref().err().map(String::as_str);
                    match self.view.complete_file_preview(&request, error) {
                        Ok(true) => {
                            if let Ok(staged) = result {
                                match std::process::Command::new("/usr/bin/qlmanage")
                                    .arg("-p")
                                    .arg(staged.path())
                                    .stdin(std::process::Stdio::null())
                                    .stdout(std::process::Stdio::null())
                                    .stderr(std::process::Stdio::null())
                                    .spawn()
                                {
                                    Ok(child) => {
                                        self.file_viewer = Some(OwnedFilePreview {
                                            child,
                                            _staged: staged,
                                        })
                                    }
                                    Err(error) => {
                                        self.view.set_notice(format!("系统预览无法打开：{error}"))
                                    }
                                }
                            }
                        }
                        Ok(false) => {}
                        Err(error) => self.failure(error.to_string()),
                    }
                }
                HostResult::Files {
                    request,
                    files,
                    result,
                } => {
                    if let Err(error) = self.view.complete_file_selection(&request, files, result) {
                        self.failure(error.to_string());
                    }
                }
                HostResult::InitialPresets { request, result } => {
                    if let Err(error) = self.view.complete_initial_presets(&request, result) {
                        self.failure(error.to_string());
                    }
                }
                HostResult::Created { request, result } => {
                    let binding = result.as_ref().ok().cloned();
                    match self.view.complete_creation(&request, result) {
                        Ok(true) => {
                            if let Some(binding) = binding {
                                self.observe(&binding, window);
                            }
                        }
                        Ok(false) => {}
                        Err(error) => self.failure(error.to_string()),
                    }
                }
                HostResult::ModelCatalog { request, result } => {
                    if let Err(error) = self.view.complete_model_catalog(&request, result) {
                        self.failure(error.to_string());
                    }
                }
                HostResult::Renamed { ticket, result } => {
                    if let Err(error) = self.view.complete_rename(&ticket, result.map(|_| ())) {
                        self.failure(error.to_string());
                    }
                }
                HostResult::Sidebar { token, result } => {
                    self.view.finish_sidebar_action(&token);
                    match result {
                        Err(error) => self.failure(error),
                        Ok(SidebarActionOutcome::Copy { text, .. }) => {
                            window.set_clipboard_text(&text);
                        }
                        Ok(SidebarActionOutcome::Mutation { outcome, .. }) => {
                            match self.view.apply_sidebar_outcome(&token, &outcome) {
                                Ok(Some(request)) => {
                                    self.commands(Ok(vec![DemoCommand::OpenSession(request)]))
                                }
                                Ok(None) => {}
                                Err(error) => self.failure(error.to_string()),
                            }
                        }
                    }
                }
                HostResult::ChildOpened {
                    request,
                    title,
                    result: Ok((source, snapshot)),
                } => {
                    if self.selftest {
                        if source.child_id() == "child-b" {
                            self.test_child_source = Some(source.clone());
                        }
                        if source.child_id() == "bounded-b" {
                            self.test_bounded_source = Some(source.clone());
                            if let PaneSnapshot::ReadOnlyChild { transcript, .. } = &snapshot {
                                self.bounded_initial_items = transcript.items.len();
                                self.bounded_max_items = transcript.items.len();
                            }
                        }
                    }
                    let binding = source.binding().clone();
                    match self
                        .view
                        .install_child_source(request, source, title, snapshot)
                    {
                        Ok(_) => {
                            self.observe(&binding, window);
                            self.verify_bounded_content();
                        }
                        Err(error) => self.failure(error.to_string()),
                    }
                }
                HostResult::ChildOpened {
                    request,
                    result: Err(error),
                    ..
                } => {
                    if self.view.child_request_current(&request) {
                        self.failure(error);
                    }
                }
                HostResult::ChildRead {
                    panel_parent,
                    source,
                    result,
                    catalog_error,
                } => {
                    if source.binding().access() != PaneAccess::Missing {
                        let active = self
                            .view
                            .child_pane_active(&panel_parent, source.child_id());
                        if let Some(error) = catalog_error
                            && active
                        {
                            self.view.set_notice(error);
                        }
                        match result {
                            Ok(snapshot) => {
                                if self.selftest
                                    && source.child_id() == "bounded-b"
                                    && let PaneSnapshot::ReadOnlyChild { transcript, .. } =
                                        &snapshot
                                {
                                    self.bounded_max_items =
                                        self.bounded_max_items.max(transcript.items.len());
                                }
                                if let Err(error) = self.view.update_child_snapshot(
                                    &panel_parent,
                                    &source,
                                    snapshot,
                                ) {
                                    self.failure(error.to_string());
                                }
                                self.verify_bounded_content();
                            }
                            Err(error) => {
                                if let Err(reset) = self.view.child_read_failed(
                                    &panel_parent,
                                    &source,
                                    error.clone(),
                                ) {
                                    self.failure(reset.to_string());
                                }
                                if active {
                                    self.failure(error);
                                }
                            }
                        }
                    }
                }
                HostResult::TaskOpened {
                    parent,
                    task_id,
                    result: Ok(Some(binding)),
                } => match self.view.install_task_run(parent, task_id, binding.clone()) {
                    Ok(true) => self.observe(&binding, window),
                    Ok(false) => {}
                    Err(error) => self.failure(error.to_string()),
                },
                HostResult::TaskOpened {
                    result: Ok(None), ..
                } => {}
                HostResult::TaskOpened {
                    result: Err(error), ..
                } => self.failure(error),
                HostResult::SideChatOpened {
                    request,
                    result: Ok(run),
                } => {
                    let new = !self.view.has_side_chat_run(run.chat());
                    let binding = run.binding().clone();
                    match self.view.install_side_chat_run(request, run) {
                        Ok(_) if new && binding.current_run().is_some() => {
                            self.observe(&binding, window)
                        }
                        Ok(_) => {}
                        Err(error) => self.failure(error.to_string()),
                    }
                }
                HostResult::SideChatOpened {
                    result: Err(error), ..
                } => self.failure(error),
                HostResult::Operation(Err(error))
                | HostResult::Opened {
                    binding: Err(error),
                    ..
                } => self.failure(error),
                HostResult::Opened {
                    request,
                    binding: Ok(binding),
                } => {
                    let new = !self
                        .view
                        .bindings()
                        .iter()
                        .any(|old| old.scope() == binding.scope());
                    match self.view.complete_navigation(request, binding.clone()) {
                        Ok(true) if new => self.observe(&binding, window),
                        Ok(_) => {}
                        Err(error) => self.failure(error.to_string()),
                    }
                }
                HostResult::Operation(Ok(())) | HostResult::HistoryPageCompleted => {}
            }
        }
        let source = self.sidebar.snapshot();
        if source.error != self.sidebar_error {
            self.sidebar_error = source.error.clone();
            if let Some(error) = &source.error {
                self.failure(error.clone());
            }
        }
        if let Err(error) = self.view.set_sidebar_source(source) {
            self.failure(error.to_string());
        }
        if let Err(error) = self.view.sync() {
            self.failure(error.to_string());
        }
        let background = self.view.take_background_commands();
        self.commands(Ok(background));
        self.text.scale = window.scale() as f32;
        self.text.frame_requested = false;
        self.text.atlas.begin_frame();
        self.scene.clear();
        let clip = Rect::new(
            0.,
            0.,
            window.size().width as f32,
            window.size().height as f32,
        );
        self.view
            .render(
                &mut self.scene,
                &mut self.text,
                clip.inset(16.),
                clip,
                self.palette,
            )
            .expect("formal components layout");
        publish_content(&self.view.tree, window);
        if let Some(renderer) = &mut self.gpu {
            renderer
                .render(
                    &self.scene,
                    &mut self.text.atlas,
                    self.palette.color("surface-1").unwrap(),
                )
                .expect("component frame");
        }
        window.set_animating(self.text.frame_requested);
        if self.annotation_selftest {
            use tekes_ui_platform_macos::selftest::{Action, run_script};
            if self.annotation_stage == 0 && self.view.annotation_editor_ids().is_some() {
                self.annotation_stage = 1;
                let ids = self.view.annotation_editor_ids().unwrap();
                let click = |id| {
                    let p = self.view.tree.get(id).unwrap().frame().center();
                    Action::Click {
                        x: p.x as f64,
                        y: p.y as f64,
                    }
                };
                let mut script = vec![];
                if let Ok(path) = std::env::var("AGENT_ANNOTATION_CAPTURE") {
                    script.push((100, Action::Capture { path }));
                }
                script.extend([
                    (200, click(ids.editor)),
                    (300, Action::TextCommand("selectAll:")),
                    (400, Action::Text(String::new())),
                    (
                        500,
                        Action::ExpectText {
                            text: String::new(),
                            marked: false,
                        },
                    ),
                    (
                        700,
                        Action::PreeditRange {
                            text: "中文批注组字".into(),
                            selection: 6..6,
                            replacement: None,
                        },
                    ),
                    (900, click(self.view.ids().send)),
                    (1100, click(tekes_agent_ui_demo::component_demo::SELECT_B)),
                    (
                        1300,
                        Action::ExpectText {
                            text: "中文批注组字".into(),
                            marked: true,
                        },
                    ),
                    (1500, Action::Text("更新后的中文批注\n第二行 😀".into())),
                ]);
                run_script(script);
            } else if self.annotation_stage == 1
                && self.view.annotation_editor_ids().is_some()
                && !self.view.composing()
                && self
                    .view
                    .binding()
                    .current_run()
                    .unwrap()
                    .draft
                    .annotations
                    .first()
                    .is_some_and(|a| a.comment == "更新后的中文批注\n第二行 😀")
            {
                self.annotation_stage = 2;
                let close = self.view.annotation_editor_ids().unwrap().close;
                let p = self.view.tree.get(close).unwrap().frame().center();
                run_script(vec![(
                    100,
                    Action::Click {
                        x: p.x as f64,
                        y: p.y as f64,
                    },
                )]);
            } else if self.annotation_stage == 2 && self.view.annotation_editor_ids().is_none() {
                self.annotation_stage = 3;
                let click = |id| {
                    let p = self.view.tree.get(id).unwrap().frame().center();
                    Action::Click {
                        x: p.x as f64,
                        y: p.y as f64,
                    }
                };
                run_script(vec![
                    (100, click(self.view.ids().send)),
                    (400, click(tekes_agent_ui_demo::component_demo::SELECT_B)),
                ]);
            } else if self.annotation_stage == 3
                && self.view.selected() == 1
                && self.runtime.inspect(|s| {
                    s.sessions.get("a").is_some_and(|s| {
                        !s.run_open
                            && s.entries.values().any(|e| {
                                e.event.kind == "user/message"
                                    && e.event.data.to_string().contains("更新后的中文批注")
                            })
                    })
                })
            {
                self.annotation_stage = 4;
                run_script(vec![(200, Action::Close)]);
            }
        }
        if self.creation_selftest {
            use tekes_ui_platform_macos::selftest::{Action, run_script};
            if self.creation_stage == 0 && self.view.initial_preset_choice_id("coding").is_some() {
                self.creation_stage = 1;
                let p = self
                    .view
                    .tree
                    .get(self.view.initial_preset_choice_id("coding").unwrap())
                    .unwrap()
                    .frame()
                    .center();
                let mut script = vec![];
                if let Ok(path) = std::env::var("AGENT_CREATION_CAPTURE") {
                    script.push((100, Action::Capture { path }));
                }
                script.push((
                    300,
                    Action::Click {
                        x: p.x as f64,
                        y: p.y as f64,
                    },
                ));
                run_script(script);
            } else if self.creation_stage == 1
                && self.view.bindings().len() == 3
                && self.view.selected() == 2
            {
                self.creation_stage = 2;
                self.creation_scope = Some(self.view.binding().scope().clone());
                let ids = self.view.ids();
                let click = |id| {
                    let p = self.view.tree.get(id).unwrap().frame().center();
                    Action::Click {
                        x: p.x as f64,
                        y: p.y as f64,
                    }
                };
                run_script(vec![
                    (100, click(ids.editor)),
                    (200, Action::TextCommand("selectAll:")),
                    (300, Action::Text(String::new())),
                    (
                        400,
                        Action::ExpectText {
                            text: String::new(),
                            marked: false,
                        },
                    ),
                    (
                        600,
                        Action::PreeditRange {
                            text: "新会话组字".into(),
                            selection: 5..5,
                            replacement: None,
                        },
                    ),
                    (800, click(ids.send)),
                    (
                        1000,
                        Action::ExpectText {
                            text: "新会话组字".into(),
                            marked: true,
                        },
                    ),
                    (1200, Action::Text("新会话中文消息".into())),
                    (1500, click(ids.send)),
                    (1700, click(tekes_agent_ui_demo::component_demo::SELECT_A)),
                ]);
            } else if self.creation_stage == 2
                && self.view.selected() == 0
                && self.creation_scope.as_ref().is_some_and(|scope| {
                    self.runtime.inspect(|s| {
                        s.sessions.get(&scope.session_id).is_some_and(|s| {
                            !s.run_open
                                && s.entries.values().any(|e| {
                                    e.event.kind == "user/message"
                                        && e.event.data.to_string().contains("新会话中文消息")
                                })
                        })
                    })
                })
            {
                self.creation_stage = 3;
                run_script(vec![(200, Action::Close)]);
            }
        }
        if self.context_selftest {
            if self.context_stage == 1
                && self.view.bindings()[0]
                    .current_run()
                    .unwrap()
                    .draft
                    .artifacts
                    .len()
                    == 1
            {
                self.context_stage = 2;
                let send = self
                    .view
                    .tree
                    .get(self.view.ids().send)
                    .unwrap()
                    .frame()
                    .center();
                let b = self
                    .view
                    .tree
                    .get(tekes_agent_ui_demo::component_demo::SELECT_B)
                    .unwrap()
                    .frame()
                    .center();
                tekes_ui_platform_macos::selftest::run_script(vec![
                    (
                        100,
                        tekes_ui_platform_macos::selftest::Action::Click {
                            x: send.x as f64,
                            y: send.y as f64,
                        },
                    ),
                    (
                        400,
                        tekes_ui_platform_macos::selftest::Action::Click {
                            x: b.x as f64,
                            y: b.y as f64,
                        },
                    ),
                ]);
            } else if self.context_stage == 2
                && self.runtime.inspect(|s| {
                    s.sessions.get("a").is_some_and(|s| {
                        !s.run_open
                            && s.entries.values().any(|e| {
                                e.event.kind == "user/message"
                                    && e.event.data.to_string().contains("上下文消息")
                            })
                    })
                })
            {
                self.context_stage = 3;
                let a = self
                    .view
                    .tree
                    .get(tekes_agent_ui_demo::component_demo::SELECT_A)
                    .unwrap()
                    .frame()
                    .center();
                tekes_ui_platform_macos::selftest::run_script(vec![
                    (
                        100,
                        tekes_ui_platform_macos::selftest::Action::Click {
                            x: a.x as f64,
                            y: a.y as f64,
                        },
                    ),
                    (500, tekes_ui_platform_macos::selftest::Action::Close),
                ]);
            }
        }
        if self.models_selftest {
            let chosen = serde_json::json!(["fixture", "chosen"]).to_string();
            if self.model_test_stage == 0 && self.view.model_choice_id(&chosen).is_some() {
                self.model_test_stage = 1;
                let target = self.view.model_choice_id(&chosen).unwrap();
                let point = self.view.tree.get(target).unwrap().frame().center();
                let mut script = vec![];
                if let Ok(path) = std::env::var("AGENT_MODEL_CHOICES_CAPTURE") {
                    script.push((
                        100,
                        tekes_ui_platform_macos::selftest::Action::Capture { path },
                    ));
                }
                script.push((
                    300,
                    tekes_ui_platform_macos::selftest::Action::Click {
                        x: point.x as f64,
                        y: point.y as f64,
                    },
                ));
                tekes_ui_platform_macos::selftest::run_script(script);
            } else if self.model_test_stage == 1 && self.view.model_choice_request().is_none() {
                self.model_test_stage = 2;
                let ids = self.view.ids();
                let click = |id| {
                    let p = self.view.tree.get(id).unwrap().frame().center();
                    tekes_ui_platform_macos::selftest::Action::Click {
                        x: p.x as f64,
                        y: p.y as f64,
                    }
                };
                tekes_ui_platform_macos::selftest::run_script(vec![
                    (100, click(ids.editor)),
                    (
                        200,
                        tekes_ui_platform_macos::selftest::Action::Text("模型选择后的消息".into()),
                    ),
                    (500, click(ids.send)),
                ]);
            } else if self.model_test_stage == 2
                && self.runtime.inspect(|s| {
                    s.sessions.get("a").is_some_and(|s| {
                        !s.run_open
                            && s.entries.values().any(|e| {
                                e.event.kind == "user/message"
                                    && e.event.data.to_string().contains("模型选择后的消息")
                            })
                    })
                })
            {
                self.model_test_stage = 3;
                tekes_ui_platform_macos::selftest::run_script(vec![(
                    200,
                    tekes_ui_platform_macos::selftest::Action::Close,
                )]);
            }
        }
        if self.actions_selftest {
            if !self.rename_test_started && self.view.rename_ids().is_some() {
                self.rename_test_started = true;
                rename_actions_script(&self.view);
            }
            if self.rename_test_started
                && !self.fork_test_started
                && self.view.rename_ids().is_none()
                && self
                    .sidebar
                    .snapshot()
                    .rows
                    .iter()
                    .any(|row| row.address.session_id == "a" && row.title == "原生重命名")
            {
                self.fork_test_started = true;
                if self.turn_footnote_selftest {
                    let snapshot = self.view.binding().current_run().unwrap();
                    let footer_id = if let Some(root) =
                        self.view.transcript_content_id(self.view.ids())
                    {
                        let content = self
                            .view
                            .tree
                            .get(root)
                            .unwrap()
                            .component::<tekes_agent_components_ui::TranscriptContent>()
                            .unwrap();
                        content.row_keys().iter().find_map(|key|if matches!(key,tekes_agent_components_ui::TranscriptContentRowKey::TurnFootnote(_)) {content.row_id(key)}else{None}).unwrap()
                    } else {
                        ComponentDemo::turn_footnote_id()
                    };
                    let footer = self
                        .view
                        .tree
                        .get(footer_id)
                        .unwrap()
                        .component::<tekes_agent_components_ui::TurnFootnote>()
                        .unwrap();
                    let turn_id = footer.facts().unwrap().turn_id;
                    let action = &snapshot
                        .transcript
                        .turns
                        .iter()
                        .find(|turn| turn.id == turn_id)
                        .unwrap()
                        .footnote_actions[0];
                    let button = footer.action_id(&action.id).unwrap();
                    let point = self.view.tree.get(button).unwrap().frame().center();
                    tekes_ui_platform_macos::selftest::run_script(vec![(
                        200,
                        tekes_ui_platform_macos::selftest::Action::Click {
                            x: point.x as f64,
                            y: point.y as f64,
                        },
                    )]);
                } else {
                    click_thread_action_script(&self.view, "fork");
                }
            }
            if self.fork_test_started
                && !self.archive_test_started
                && self
                    .view
                    .current_binding()
                    .is_some_and(|binding| binding.scope().session_id.starts_with("fork-"))
            {
                self.archive_test_started = true;
                self.fork_test_session = self
                    .view
                    .current_binding()
                    .map(|binding| binding.scope().session_id.clone());
                click_thread_action_script(&self.view, "archive");
            }
            if self.archive_test_started
                && !self.empty_selection_seen
                && self.view.current_binding().is_none()
            {
                self.empty_selection_seen = true;
                restore_selection_script(&self.view);
            }
        }
        if self.selftest && !self.side_chat_test_started && self.view.panel_composer_ids().is_some()
        {
            self.side_chat_test_started = true;
            self.side_chat_test_session = self
                .side_chats
                .snapshot()
                .chats
                .values()
                .find(|chat| &chat.parent == self.view.binding().scope())
                .map(|chat| chat.scope.session_id.clone());
            side_chat_script(&self.view);
        }
        if self.selftest
            && self.side_chat_test_started
            && self.side_chats.snapshot().chats.is_empty()
        {
            if !self.task_test_requested {
                self.task_test_requested = true;
                // Host navigation explicitly opens a resolved fixture task.
                // Same-session todo rows have no Summary OpenTask action.
                let source = Arc::new(tekes_agent_ui_bindings::TaskPaneBinding::for_run(
                    self.view.binding(),
                ));
                let row = self
                    .view
                    .binding()
                    .current_run()
                    .unwrap()
                    .summary
                    .as_ref()
                    .unwrap()
                    .tasks[0]
                    .clone();
                let token = source
                    .begin(self.view.binding().scope().clone(), row.id.clone())
                    .unwrap();
                assert!(source.commit(&token, Ok(Some(row))));
                self.view.open_task_source(source).unwrap();
                let commands = self.view.take_background_commands();
                self.commands(Ok(commands));
            } else if !self.task_test_started && self.view.panel_composer_ids().is_some() {
                self.task_test_started = true;
                task_script(&self.view);
            }
        }
        if self.selftest
            && self.task_test_started
            && self.view.tree.get(self.view.summary_id()).is_some()
        {
            if !self.child_test_requested {
                self.child_test_requested = true;
                self.open_fixture_child("child-b", ChildHistoryLayout::IndependentSessions);
            } else if self.child_test_started && !self.bounded_test_requested {
                self.bounded_test_requested = true;
                self.open_fixture_child("bounded-b", ChildHistoryLayout::SameSession);
            }
        }
        if self.selftest
            && self.child_test_requested
            && !self.child_test_started
            && self.test_child_source.as_ref().is_some_and(|source| {
                source
                    .binding()
                    .current_run()
                    .is_some_and(|run| run.can_submit)
            })
            && self.view.panel_child_controls_ready()
        {
            self.child_test_started = true;
            child_script(&self.view);
        }
        if self.selftest
            && self.bounded_test_requested
            && !self.bounded_test_started
            && self.view.bounded_child_buttons().is_some_and(|(_, page)| {
                self.view
                    .tree
                    .get(page)
                    .and_then(|node| {
                        node.component::<tekes_ui_controls::composition::ButtonContent>()
                    })
                    .is_some_and(|button| !button.button.state.disabled)
            })
        {
            self.bounded_test_started = true;
            bounded_script(&self.view);
        }
    }
    fn open_fixture_child(&mut self, child_id: &str, layout: ChildHistoryLayout) {
        let parent = self.view.binding().scope().clone();
        let request = self.view.begin_child_open(&parent).unwrap();
        self.commands(Ok(vec![DemoCommand::OpenChild {
            request,
            direct_parent: parent,
            child_id: child_id.into(),
            title: child_id.into(),
            layout,
        }]));
    }
    fn observe(&mut self, binding: &RunBinding, window: &Window) {
        let wake = window.wake_proxy();
        if binding.subscribe_run().is_none() {
            let mut changes = binding.subscribe_runtime();
            self.observers.push(self.executor.spawn(async move {
                while changes.changed().await.is_ok() {
                    changes.borrow_and_update();
                    if wake.wake() == tekes_ui_platform_macos::WakeResult::Closed {
                        break;
                    }
                }
            }));
            return;
        }
        self.observers.push(
            self.executor
                .spawn(tekes_agent_ui_demo::notifications::forward(
                    binding.subscribe_run().expect("main run"),
                    move || wake.wake() != tekes_ui_platform_macos::WakeResult::Closed,
                )),
        );
    }
    fn close(&mut self, window: &Window) {
        if self.system_ime_selftest {
            assert!(
                self.failures.is_empty(),
                "IME probe failures: {:?}",
                self.failures
            );
            let state = self.runtime.snapshot();
            let inputs: Vec<_> = state.sessions["a"]
                .entries
                .values()
                .filter(|entry| entry.event.kind == "user/message")
                .collect();
            assert_eq!(
                inputs.len(),
                self.system_ime_initial_users + 1,
                "composition must not submit before commit"
            );
            assert_eq!(
                inputs
                    .iter()
                    .filter(|entry| entry.event.data.to_string().contains("你好"))
                    .count(),
                1
            );
            assert_eq!(self.view.binding().scope().session_id, "a");
            println!(
                "NATIVE_AGENT_SYSTEM_IME_PROBE_OK software-ime+pid-targeted-key-events+marked-text+send-barrier+navigation-barrier+commit+single-submit; physical-keyboard=false; candidate-window=false"
            );
        }
        if self.annotation_selftest {
            assert!(
                self.failures.is_empty(),
                "annotation errors: {:?}",
                self.failures
            );
            assert_eq!(self.annotation_stage, 4);
            let state = self.runtime.snapshot();
            let entries = &state.sessions["a"].entries;
            let inputs: Vec<_> = entries
                .values()
                .filter(|e| {
                    e.event.kind == "user/message"
                        && e.event.data.to_string().contains("更新后的中文批注")
                })
                .collect();
            assert_eq!(inputs.len(), 1);
            let text = inputs[0].event.data.to_string();
            assert!(text.contains("中文引用") && text.contains("批注正文"));
            assert!(!text.contains("中文批注组字"));
            assert!(
                self.view.bindings()[0]
                    .current_run()
                    .unwrap()
                    .draft
                    .annotations
                    .is_empty()
            );
            assert_eq!(
                self.view.bindings()[1].current_run().unwrap().draft.text,
                "B 注释隔离"
            );
            assert!(
                self.view.bindings()[1]
                    .current_run()
                    .unwrap()
                    .draft
                    .annotations
                    .is_empty()
            );
            println!(
                "NATIVE_AGENT_ANNOTATION_SELFTEST_OK public-editor+committed-comment+synthetic-ime-barrier+shared-draft+shared-compilation+single-admission+stream+destination-isolation; physical-ime=false; source-highlight=false; formal-transcript=false"
            );
        }
        if self.creation_selftest {
            assert!(
                self.failures.is_empty(),
                "creation errors: {:?}",
                self.failures
            );
            assert_eq!(self.creation_stage, 3);
            assert_eq!(
                self.view.bindings()[0].current_run().unwrap().draft.text,
                "A 原草稿"
            );
            let scope = self.creation_scope.as_ref().unwrap();
            let state = self.runtime.snapshot();
            let session = &state.sessions[&scope.session_id];
            assert_eq!(
                session
                    .entries
                    .values()
                    .filter(|e| e.event.kind == "user/message"
                        && e.event.data.to_string().contains("新会话中文消息"))
                    .count(),
                1
            );
            assert!(
                session
                    .entries
                    .values()
                    .any(|e| e.event.kind == "assistant/message")
            );
            println!(
                "NATIVE_AGENT_CREATION_SELFTEST_OK public-preset-panel+captured-session-rpc+shared-create+normal-composer+synthetic-ime-barrier+single-admission+stream+original-draft; physical-ime=false; formal-transcript=false"
            );
        }
        if self.context_selftest {
            assert!(
                self.failures.is_empty(),
                "context failures: {:?}",
                self.failures
            );
            assert_eq!(self.context_stage, 3);
            let state = self.runtime.snapshot();
            let users: Vec<_> = state.sessions["a"]
                .entries
                .values()
                .filter(|e| {
                    e.event.kind == "user/message"
                        && e.event.data.to_string().contains("上下文消息")
                })
                .collect();
            assert_eq!(users.len(), 1);
            let input = users[0].event.data.to_string();
            assert!(
                input.contains("上下文引用说明")
                    && input.contains("中文引用")
                    && input.contains("请解释这段内容")
            );
            assert!(!input.contains("DISPLAY_ONLY_NOT_UPLOADED"));
            let a = self.view.bindings()[0].current_run().unwrap();
            assert!(a.draft.artifacts.is_empty() && a.draft.annotations.is_empty());
            let b = self.view.bindings()[1].current_run().unwrap();
            assert_eq!(b.draft.text, "B 内容草稿");
            assert_eq!(b.draft.artifacts.len(), 1);
            assert_eq!(self.view.current_binding().unwrap().scope().session_id, "a");
            println!(
                "NATIVE_AGENT_CONTEXT_SELFTEST_OK public-cards+ime-barrier+remove-before-send+shared-compilation+single-admission+stream+destination-isolation; file-upload=false; annotation-editor=false; physical-ime=false"
            );
        }
        if self.models_selftest {
            assert!(
                self.failures.is_empty(),
                "model fixture failures: {:?}",
                self.failures
            );
            assert_eq!(self.model_test_stage, 3);
            let state = self.runtime.snapshot();
            let entries = &state.sessions["a"].entries;
            assert_eq!(
                entries
                    .values()
                    .filter(
                        |e| e.event.kind == "model/selection" && e.event.data["model"] == "chosen"
                    )
                    .count(),
                1
            );
            assert_eq!(
                entries
                    .values()
                    .filter(|e| e.event.kind == "user/message"
                        && e.event.data.to_string().contains("模型选择后的消息"))
                    .count(),
                1
            );
            assert!(
                self.view.bindings()[1]
                    .current_run()
                    .unwrap()
                    .draft
                    .settings
                    .is_empty()
            );
            println!(
                "NATIVE_AGENT_MODEL_CHOICES_SELFTEST_OK public-catalog+explicit-choice+shared-settings+send-preparation+single-admission+stream+destination-isolation; physical-ime=false; provider-switches=false"
            );
        }
        if self.actions_selftest {
            assert!(
                self.failures.is_empty(),
                "thread action failures: {:?}",
                self.failures
            );
            assert!(
                self.rename_test_started
                    && self.fork_test_started
                    && self.archive_test_started
                    && self.empty_selection_seen
            );
            assert_eq!(self.view.current_binding().unwrap().scope().session_id, "b");
            let state = self.runtime.snapshot();
            let fork = self.fork_test_session.as_ref().unwrap();
            assert!(state.archived_session_ids.contains(fork));
            assert_eq!(
                state
                    .sessions
                    .keys()
                    .filter(|id| id.starts_with("fork-"))
                    .count(),
                1
            );
            assert!(
                self.sidebar
                    .snapshot()
                    .rows
                    .iter()
                    .any(|row| row.address.session_id == "a" && row.title == "原生重命名")
            );
            assert!(
                !self
                    .sidebar
                    .snapshot()
                    .rows
                    .iter()
                    .any(|row| &row.address.session_id == fork)
            );
            assert_eq!(
                self.view.bindings()[0].current_run().unwrap().draft.text,
                "保留原会话草稿"
            );
            assert!(
                !state.sessions["a"]
                    .entries
                    .values()
                    .any(|entry| entry.event.kind == "user/message"
                        && entry.event.data.to_string().contains("原生重命名"))
            );
            if self.turn_footnote_selftest {
                println!(
                    "NATIVE_AGENT_TURN_FOOTNOTE_SELFTEST_OK native-pointer+shared-turn-action+fork-navigation+archive+draft-isolation; physical-ime=false; complete-transcript=false"
                );
            }
            println!(
                "NATIVE_AGENT_THREAD_ACTIONS_SELFTEST_OK rename-ime-barrier+shared-inventory+fork-navigation+archive-clear+restored-selection+draft-isolation; physical-ime=false; formal-run-view=false"
            );
        }
        if self.selftest {
            assert!(self.child_test_started && self.bounded_test_started);
            assert!(
                self.bounded_rendered_items > self.bounded_initial_items,
                "native paging must extend rendered child messages"
            );
            println!(
                "NATIVE_AGENT_READONLY_CHILD_CONTENT_SELFTEST_OK actual-child-messages+original-scope+native-pagination+no-composer; reading-anchor=false; physical-ime=false"
            );
            assert!(
                self.bounded_max_items > self.bounded_initial_items,
                "bounded paging reached the independent child prefix"
            );
            assert_eq!(
                self.test_child_source.as_ref().unwrap().binding().access(),
                PaneAccess::Missing
            );
            assert_eq!(
                self.test_bounded_source
                    .as_ref()
                    .unwrap()
                    .binding()
                    .access(),
                PaneAccess::Missing
            );
            assert!(
                self.task_test_started,
                "task pane mounted through its resolved source"
            );
            assert!(
                self.side_chat_test_started && self.side_chats.snapshot().chats.is_empty(),
                "scratch closed only after confirmed discard"
            );
            assert!(
                self.failures.is_empty(),
                "component host failures: {:?}",
                self.failures
            );
            assert!(
                self.view
                    .tree
                    .semantics()
                    .iter()
                    .any(|node| node.semantics.name.contains("验证会话 b 的数据回传")),
                "Summary uses the active session's actual journal facts"
            );
            println!(
                "NATIVE_AGENT_SIDEBAR_SUMMARY_SELFTEST_OK shared-inventory+shared-pin+sidebar-selection+summary-facts; full-run-view=false; physical-ime=false"
            );
            let snapshots: Vec<_> = self
                .view
                .bindings()
                .iter()
                .map(|binding| binding.current_run().expect("main run"))
                .collect();
            assert_eq!(snapshots[0].draft.text, "A 会话草稿");
            assert_eq!(snapshots[1].draft.text, "B 会话草稿");
            assert!(
                snapshots
                    .iter()
                    .all(|snapshot| !snapshot.transcript.is_stale)
            );
            let state = self.runtime.snapshot();
            let scratch_id = self
                .side_chat_test_session
                .as_ref()
                .expect("owned scratch identity");
            let scratch = &state.sessions[scratch_id];
            assert_eq!(
                scratch
                    .entries
                    .values()
                    .filter(|row| row.event.kind == "user/message"
                        && row.event.data.to_string().contains("SideChat 独立输入"))
                    .count(),
                1
            );
            assert!(
                !scratch
                    .entries
                    .values()
                    .any(|row| row.event.kind == "user/message"
                        && row.event.data.to_string().contains("SideChat 暂存草稿"))
            );
            assert!(
                scratch
                    .entries
                    .values()
                    .any(|row| row.event.kind == "assistant/message")
            );
            assert!(scratch.entries.values().any(
                |row| row.event.kind == "turn/end" && row.event.data["reason"] == "interrupted"
            ));
            assert!(!scratch.run_open);
            println!(
                "NATIVE_AGENT_SIDECHAT_SELFTEST_OK owned-create+separate-composer+send+stream+draft+cancel-discard+parent-isolation; transcript=false; physical-ime=false"
            );
            let entries = &state.sessions["a"].entries;
            let task_entries = &state.sessions["b"].entries;
            let child_entries = &state.sessions["child-b"].entries;
            let child_message = child_entries
                .values()
                .filter(|row| {
                    row.event.kind == "user/message"
                        && row.event.data.to_string().contains("子代理消息")
                })
                .collect::<Vec<_>>();
            assert_eq!(child_message.len(), 1);
            let child_turn = &child_message[0].event.data["turn"];
            assert!(
                child_entries
                    .values()
                    .any(|row| row.event.kind == "assistant/message"
                        && &row.event.data["turn"] == child_turn)
            );
            assert!(
                child_entries
                    .values()
                    .any(|row| row.event.kind == "turn/end"
                        && &row.event.data["turn"] == child_turn
                        && row.event.data["reason"] == "interrupted")
            );
            assert!(
                !child_entries
                    .values()
                    .any(|row| row.event.kind == "user/message"
                        && row.event.data.to_string().contains("子代理暂存草稿"))
            );
            assert!(
                !task_entries
                    .values()
                    .any(|row| row.event.kind == "user/message"
                        && row.event.data.to_string().contains("子代理消息"))
            );
            println!(
                "NATIVE_AGENT_CHILD_SELFTEST_OK direct-parent+independent-composer+single-admission+stream+cancel+isolated-drafts+bounded-read-only+paging+local-retirement; formal-transcript=false; physical-ime=false"
            );
            assert_eq!(
                task_entries
                    .values()
                    .filter(|row| row.event.kind == "user/message"
                        && row.event.data.to_string().contains("任务消息"))
                    .count(),
                1
            );
            assert!(
                !task_entries
                    .values()
                    .any(|row| row.event.kind == "user/message"
                        && row.event.data.to_string().contains("任务暂存草稿"))
            );
            let task_turn = &task_entries
                .values()
                .find(|row| {
                    row.event.kind == "user/message"
                        && row.event.data.to_string().contains("任务消息")
                })
                .unwrap()
                .event
                .data["turn"];
            assert!(
                task_entries
                    .values()
                    .any(|row| row.event.kind == "assistant/message"
                        && &row.event.data["turn"] == task_turn)
            );
            assert!(task_entries.values().any(|row| row.event.kind == "turn/end"
                && &row.event.data["turn"] == task_turn
                && row.event.data["reason"] == "interrupted"));
            assert!(!state.sessions["b"].run_open);
            println!(
                "NATIVE_AGENT_TASK_SELFTEST_OK resolved-task+independent-composer+single-admission+stream+cancel+parent-draft-isolation+close; transcript=false; physical-ime=false"
            );
            for text in ["中文消息", "停止演示"] {
                assert_eq!(
                    entries
                        .values()
                        .filter(|row| row.event.kind == "user/message"
                            && row.event.data.to_string().contains(text))
                        .count(),
                    1,
                    "one admission for {text}"
                );
            }
            assert!(!entries.values().any(|row| row.event.kind == "user/message"
                && row.event.data.to_string().contains("中文组字")));
            assert!(!state.sessions["a"].run_open);
            assert!(
                self.sidebar
                    .snapshot()
                    .rows
                    .iter()
                    .find(|row| row.address.session_id == "a")
                    .unwrap()
                    .pin
                    .as_ref()
                    .is_some_and(|pin| pin.pinned)
            );
            println!(
                "NATIVE_AGENT_COMPONENTS_SELFTEST_OK composer+indicators+run-binding+synthetic-text-services+send+stream+cancel+draft-isolation+reconnect; physical-ime=false; transcript=false; reading-anchor=false"
            );
        }
        for observer in self.observers.drain(..) {
            observer.abort();
        }
        window.set_animating(false);
        window.close();
    }
}
impl WindowHandler for Host {
    fn handle(&mut self, event: Event, window: &mut Window) {
        match event {
            Event::Created => {
                assert!(window.set_text_key_events(true));
                self.gpu = Some(window_renderer(window));
                let focused = self.view.tree.focus(Some(self.view.ids().editor), true);
                if self.selftest {
                    eprintln!(
                        "COMPONENT_FOCUS created={focused} id={:?}",
                        self.view.tree.focused()
                    );
                }
                let wake = window.wake_proxy();
                self.wake = Some(wake.clone());
                // Both retained source controllers publish, even while their pane
                // is offscreen. Forward notifications, never poll once per frame.
                for binding in self.view.bindings() {
                    let wake = wake.clone();
                    self.observers.push(self.executor.spawn(
                        tekes_agent_ui_demo::notifications::forward(
                            binding.subscribe_run().expect("main run"),
                            move || wake.wake() != tekes_ui_platform_macos::WakeResult::Closed,
                        ),
                    ));
                }
                let sidebar_wake = wake.clone();
                self.observers.push(self.executor.spawn(
                    tekes_agent_ui_demo::notifications::forward(
                        self.sidebar.subscribe(),
                        move || sidebar_wake.wake() != tekes_ui_platform_macos::WakeResult::Closed,
                    ),
                ));
                self.observers.push(self.executor.spawn(
                    tekes_agent_ui_demo::notifications::forward(
                        self.side_chats.subscribe(),
                        move || wake.wake() != tekes_ui_platform_macos::WakeResult::Closed,
                    ),
                ));
                self.draw(window);
                if self.selftest {
                    eprintln!("COMPONENT_FOCUS after-draw={:?}", self.view.tree.focused());
                }
                if self.selftest {
                    native_script(&self.view);
                }
                if self.system_ime_selftest {
                    system_ime_script(&self.view);
                }
                if self.actions_selftest {
                    click_thread_action_script(&self.view, "rename");
                    tekes_ui_platform_macos::selftest::run_script(vec![(
                        30000,
                        tekes_ui_platform_macos::selftest::Action::Close,
                    )]);
                }
                if self.models_selftest {
                    let target = self.view.ids().model;
                    let p = self.view.tree.get(target).unwrap().frame().center();
                    tekes_ui_platform_macos::selftest::run_script(vec![
                        (
                            100,
                            tekes_ui_platform_macos::selftest::Action::Click {
                                x: p.x as f64,
                                y: p.y as f64,
                            },
                        ),
                        (15000, tekes_ui_platform_macos::selftest::Action::Close),
                    ]);
                }
                if self.creation_selftest {
                    let p = self
                        .view
                        .tree
                        .get(tekes_agent_ui_demo::component_demo::NEW_SESSION)
                        .unwrap()
                        .frame()
                        .center();
                    tekes_ui_platform_macos::selftest::run_script(vec![
                        (
                            100,
                            tekes_ui_platform_macos::selftest::Action::Click {
                                x: p.x as f64,
                                y: p.y as f64,
                            },
                        ),
                        (20000, tekes_ui_platform_macos::selftest::Action::Close),
                    ]);
                }
                if self.annotation_selftest {
                    use tekes_agent_components_ui::{
                        Composer, ComposerContextAction, ComposerContextItem,
                    };
                    let (_, notes) = tekes_agent_ui_demo::fixture::composer_context_fixture();
                    let target = self
                        .view
                        .tree
                        .get(self.view.ids().root)
                        .unwrap()
                        .component::<Composer>()
                        .unwrap()
                        .context()
                        .unwrap()
                        .action_id(
                            &ComposerContextItem::Annotation(notes[0].clone()),
                            ComposerContextAction::EditAnnotation,
                        )
                        .unwrap();
                    let p = self.view.tree.get(target).unwrap().frame().center();
                    tekes_ui_platform_macos::selftest::run_script(vec![
                        (
                            100,
                            tekes_ui_platform_macos::selftest::Action::Click {
                                x: p.x as f64,
                                y: p.y as f64,
                            },
                        ),
                        (20000, tekes_ui_platform_macos::selftest::Action::Close),
                    ]);
                }
                if self.context_selftest {
                    self.context_stage = 1;
                    context_script(&self.view);
                }
            }
            Event::Resized {
                drawable_size,
                scale,
                ..
            } => {
                if let Some(renderer) = &mut self.gpu {
                    renderer.resize(drawable_size, scale as f32);
                }
                self.draw(window);
            }
            Event::Wake => window.request_redraw(),
            Event::Redraw | Event::AnimationFrame(_) => self.draw(window),
            Event::CaretBlink => {
                self.text.caret_on = !self.text.caret_on;
                self.draw(window);
            }
            Event::SystemMotionChanged(reduced) => {
                self.text.reduced_motion = reduced;
                self.draw(window);
            }
            Event::Focus(active) => {
                if self.selftest || self.context_selftest || self.system_ime_selftest {
                    eprintln!(
                        "COMPONENT_FOCUS active={active} id={:?}",
                        self.view.tree.focused()
                    );
                }
                if !active {
                    if let Some(target) = self.view.tree.focused() {
                        self.focus_before_deactivate = Some(target);
                    }
                    let commands = self.view.event(&ContentEvent::Cancel);
                    self.commands(commands);
                    self.view.tree.focus(None, false);
                } else if self.view.tree.focused().is_none()
                    && let Some(target) = self.focus_before_deactivate.take()
                {
                    // Window activation restores the last mounted native target;
                    // the public ContentHost still validates focus ownership.
                    self.view.tree.focus(Some(target), false);
                }
                publish_content(&self.view.tree, window);
                window.request_redraw();
            }
            Event::Accessibility { target, action } => {
                if !window.is_active() {
                    return;
                }
                let action = match action {
                    tekes_ui_platform_macos::AccessibilityAction::SetExpanded(_) => return,
                    tekes_ui_platform_macos::AccessibilityAction::Increment => {
                        AccessibilityAction::Increment
                    }
                    tekes_ui_platform_macos::AccessibilityAction::Decrement => {
                        AccessibilityAction::Decrement
                    }
                    tekes_ui_platform_macos::AccessibilityAction::Focus => {
                        AccessibilityAction::Focus
                    }
                    tekes_ui_platform_macos::AccessibilityAction::Press => {
                        AccessibilityAction::Press
                    }
                    tekes_ui_platform_macos::AccessibilityAction::SetValue(text) => {
                        AccessibilityAction::SetValue(text)
                    }
                    tekes_ui_platform_macos::AccessibilityAction::SetSelection(range) => {
                        AccessibilityAction::SetSelection(range)
                    }
                    tekes_ui_platform_macos::AccessibilityAction::ReplaceSelectedText(text) => {
                        AccessibilityAction::ReplaceSelectedText(text)
                    }
                };
                let commands = self.view.accessibility_action(
                    ComponentId(target.component),
                    target.part,
                    action,
                );
                self.commands(commands);
                self.draw(window);
            }
            Event::SystemAppearanceChanged {
                dark,
                increase_contrast,
            } => {
                self.palette = Palette::from(Theme {
                    appearance: if dark {
                        tekes_ui_tokens::Appearance::Dark
                    } else {
                        tekes_ui_tokens::Appearance::Light
                    },
                    contrast: if increase_contrast {
                        tekes_ui_tokens::Contrast::Increased
                    } else {
                        tekes_ui_tokens::Contrast::Standard
                    },
                });
                self.draw(window);
            }
            Event::CloseRequested => self.close(window),
            other => {
                if let Some(event) = input_event(other, window, &self.view.tree) {
                    if self.system_ime_selftest {
                        eprintln!(
                            "SYSTEM_IME_EVENT {event:?} composing={} focus={:?}",
                            self.view.composing(),
                            self.view.tree.focused()
                        );
                    }
                    let commands = self.view.event_with_text(&event, &mut self.text);
                    self.commands(commands);
                    self.draw(window);
                }
            }
        }
    }
}
pub fn run() {
    let args: Vec<_> = std::env::args().collect();
    let selftest = args.iter().any(|arg| arg == "--native-selftest");
    let system_ime_selftest = args.iter().any(|arg| arg == "--system-ime-selftest");
    let actions_selftest = args.iter().any(|arg| arg == "--thread-actions-selftest");
    let models_selftest = args.iter().any(|arg| arg == "--model-choices-selftest");
    let context_selftest = args.iter().any(|arg| arg == "--context-selftest");
    let creation_selftest = args.iter().any(|arg| arg == "--creation-selftest");
    let annotation_selftest = args.iter().any(|arg| arg == "--annotation-selftest");
    assert!(
        [
            selftest,
            system_ime_selftest,
            actions_selftest,
            models_selftest,
            context_selftest,
            creation_selftest,
            annotation_selftest
        ]
        .into_iter()
        .filter(|v| *v)
        .count()
            <= 1,
        "choose one native fixture selftest mode"
    );
    let executor = tokio::runtime::Runtime::new().unwrap();
    let (connector, kind, scopes, fixture): (
        Arc<dyn tekes_agent_contract::EndpointConnector>,
        _,
        _,
        _,
    ) = if let Some(index) = args.iter().position(|arg| arg == "--endpoint-config") {
        assert!(
            !selftest
                && !system_ime_selftest
                && !actions_selftest
                && !models_selftest
                && !context_selftest
                && !creation_selftest
                && !annotation_selftest,
            "native selftest uses isolated fixture only"
        );
        let value = serde_json::from_slice(
            &std::fs::read(args.get(index + 1).expect("config path")).expect("explicit config"),
        )
        .expect("config JSON");
        let configured =
            tekes_agent_ui_demo::endpoint_config::EndpointConfiguration::parse(&value, |name| {
                std::env::var(name).ok()
            })
            .expect("explicit endpoint configuration");
        let parts = executor
            .block_on(configured.into_connector())
            .expect("configured connector");
        (parts.connector, parts.kind, parts.scopes, false)
    } else {
        (
            Arc::new({
                let fixture = tekes_agent_ui_demo::fixture::Fixture::default()
                    .with_demo_summary()
                    .with_side_chats()
                    .with_children()
                    .with_lifecycle()
                    .with_presets(false)
                    .with_models();
                let fixture = if args.iter().any(|arg| arg == "--session-notice-fixture") {
                    fixture.with_session_notice()
                } else {
                    fixture
                };
                if args.iter().any(|arg| arg == "--actionable-fixture") {
                    fixture.publish_actionable(serde_json::json!({"id":"fixture-approval","sessionId":"a","revision":0,"kind":"approval","payload":{"sessionId":"a","approvalId":"synthetic-only","toolName":"本地测试审批","reason":"仅验证回调，不执行系统操作"}}));
                    fixture.publish_actionable(serde_json::json!({"id":"fixture-question","sessionId":"a","revision":0,"kind":"question","payload":{"sessionId":"a","questions":[{"id":"q","question":"选择测试回答","options":[{"label":"继续测试"},{"label":"稍后"}],"multiSelect":false,"allowCustom":false}]}}));
                }
                if args.iter().any(|arg| arg == "--file-attachments") {
                    fixture.with_uploads()
                } else {
                    fixture
                }
            }),
            "native",
            [
                RunScope::main("fixture", "a"),
                RunScope::main("fixture", "b"),
            ],
            true,
        )
    };
    let mut agent = SharedAgent::default();
    agent.set_session_url_scheme("tekes-demo").unwrap();
    let mut owned_preferences = None;
    if let Some(index) = args.iter().position(|arg| arg == "--thread-preferences") {
        let path = std::path::PathBuf::from(args.get(index + 1).expect("thread preferences path"));
        assert!(
            path.is_absolute(),
            "thread preferences path must be absolute"
        );
        agent
            .open_thread_preferences(&path, "agentkit-component-demo")
            .unwrap();
    } else if fixture {
        let directory = OwnedPreferencesDirectory::new();
        agent
            .open_thread_preferences(&directory.0.join("threads.json"), "agentkit-component-demo")
            .unwrap();
        owned_preferences = Some(directory);
    }
    let registration = executor.enter();
    agent
        .register(
            scopes[0].endpoint_id.clone(),
            kind.into(),
            "AgentKit demo".into(),
            connector,
        )
        .unwrap();
    drop(registration);
    let agent = Arc::new(agent);
    let runtime = agent
        .endpoint(&scopes[0].endpoint_id)
        .unwrap()
        .runtime
        .clone();
    let (bindings, sidebar) = executor.block_on(async {
        runtime.wait_ready(Duration::from_secs(30)).await.unwrap();
        let a = Arc::new(
            RunBinding::open_main(runtime.clone(), scopes[0].clone())
                .await
                .unwrap(),
        );
        let b = Arc::new(
            RunBinding::open_main(runtime.clone(), scopes[1].clone())
                .await
                .unwrap(),
        );
        let sidebar = Arc::new(SidebarBinding::new(agent, &scopes[0].endpoint_id).unwrap());
        let mut changes = sidebar.subscribe();
        tokio::time::timeout(Duration::from_secs(10), async {
            while !sidebar.snapshot().loaded && sidebar.snapshot().error.is_none() {
                changes.changed().await.unwrap();
            }
        })
        .await
        .expect("inventory baseline timeout");
        assert!(
            sidebar.snapshot().error.is_none(),
            "inventory observation failed"
        );
        ([a, b], sidebar)
    });
    let mut view = ComponentDemo::new(bindings).unwrap();
    view.set_file_picker_available(true);
    view.set_file_preview_available(std::path::Path::new("/usr/bin/qlmanage").is_file());
    if annotation_selftest {
        use tekes_agent_presentation::{ComposerContextBaseline, ComposerDestination};
        let binding = &view.bindings()[0];
        let (_, notes) = tekes_agent_ui_demo::fixture::composer_context_fixture();
        assert!(
            binding
                .patch_context(
                    &ComposerContextBaseline::read(&binding.current_run().unwrap()).unwrap(),
                    vec![],
                    notes
                )
                .unwrap()
        );
        binding
            .edit_text(
                &ComposerDestination::run(binding.scope().clone()),
                "批注正文".into(),
            )
            .unwrap();
        let b = &view.bindings()[1];
        b.edit_text(
            &ComposerDestination::run(b.scope().clone()),
            "B 注释隔离".into(),
        )
        .unwrap();
    }
    if creation_selftest {
        let binding = view.binding();
        binding
            .edit_text(
                &tekes_agent_presentation::ComposerDestination::run(binding.scope().clone()),
                "A 原草稿".into(),
            )
            .unwrap();
    }
    let create_cwd = args
        .iter()
        .position(|arg| arg == "--create-cwd")
        .map(|index| args.get(index + 1).expect("explicit creation cwd").clone())
        .or_else(|| fixture.then(|| "/fixture".into()));
    if let Some(cwd) = create_cwd {
        view.configure_creation(
            tekes_agent_contract::CreateSessionRequest {
                cwd: Some(cwd),
                workspace_id: None,
                session_id: None,
                agent_preset: None,
            },
            format!(
                "demo-create:{}:{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ),
        )
        .unwrap();
    }
    if context_selftest {
        use tekes_agent_presentation::{
            ComposerArtifact, ComposerContextBaseline, ComposerDestination,
        };
        let a = &view.bindings()[0];
        let (artifacts, annotations) = tekes_agent_ui_demo::fixture::composer_context_fixture();
        assert!(
            a.patch_context(
                &ComposerContextBaseline::read(&a.current_run().unwrap()).unwrap(),
                artifacts,
                annotations
            )
            .unwrap()
        );
        let b = &view.bindings()[1];
        b.edit_text(
            &ComposerDestination::run(b.scope().clone()),
            "B 内容草稿".into(),
        )
        .unwrap();
        assert!(
            b.patch_context(
                &ComposerContextBaseline::read(&b.current_run().unwrap()).unwrap(),
                vec![ComposerArtifact::Reference {
                    id: "b-context".into(),
                    mention: Some("B 资料".into()),
                    path: None,
                    excerpt: None
                }],
                vec![]
            )
            .unwrap()
        );
    }
    if actions_selftest {
        let binding = view.binding();
        binding
            .edit_text(
                &tekes_agent_presentation::ComposerDestination::run(binding.scope().clone()),
                "保留原会话草稿".into(),
            )
            .unwrap();
    }
    let side_chats = Arc::new(SideChatBinding::new(runtime.clone()));
    view.set_side_chat_binding(side_chats.clone());
    view.set_sidebar_source(sidebar.snapshot()).unwrap();
    view.sync().unwrap();
    if args.iter().any(|arg| arg == "--transcript-content") {
        assert!(
            !args.iter().any(|arg| matches!(
                arg.as_str(),
                "--input-message-proof" | "--output-message-proof" | "--turn-footnote-proof"
            )),
            "content-tree mode and independent row probes are separate hosts"
        );
        view.enable_transcript_content()
            .expect("canonical transcript content tree");
    }
    let fonts = CoreTextFonts::new();
    let (sender, results) = mpsc::channel();
    let mut turn_footnote_id = None;
    let mut output_message_id = None;
    let mut input_message_ids = None;
    if !args.iter().any(|arg| arg == "--offscreen") {
        if args.iter().any(|arg| arg == "--input-message-proof") {
            input_message_ids = Some(
                view.mount_input_message_proof()
                    .expect("canonical input message row"),
            );
        }
        if args.iter().any(|arg| arg == "--output-message-proof") {
            output_message_id = Some(
                view.mount_output_message_proof()
                    .expect("canonical machine message row"),
            );
        }
        if args.iter().any(|arg| arg == "--turn-footnote-proof") {
            turn_footnote_id = Some(
                view.mount_turn_footnote_proof()
                    .expect("turn footnote source"),
            );
        }
    }
    let system_ime_initial_users = if system_ime_selftest {
        runtime.snapshot().sessions["a"]
            .entries
            .values()
            .filter(|entry| entry.event.kind == "user/message")
            .count()
    } else {
        0
    };
    let mut host = Host {
        clipboard_writes: vec![],
        file_viewer: None,
        view,
        sidebar,
        side_chats,
        _owned_preferences: owned_preferences,
        sidebar_error: None,
        side_chat_test_started: false,
        side_chat_test_session: None,
        task_test_requested: false,
        task_test_started: false,
        child_layout: if kind == "native" {
            ChildHistoryLayout::SameSession
        } else {
            ChildHistoryLayout::IndependentSessions
        },
        child_test_requested: false,
        child_test_started: false,
        bounded_test_requested: false,
        bounded_test_started: false,
        test_child_source: None,
        test_bounded_source: None,
        bounded_initial_items: 0,
        bounded_max_items: 0,
        bounded_rendered_items: 0,
        rename_rpc_prefix: format!(
            "demo-rename:{}:{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ),
        actions_selftest,
        turn_footnote_selftest: actions_selftest
            && args.iter().any(|arg| {
                matches!(
                    arg.as_str(),
                    "--turn-footnote-proof" | "--transcript-content"
                )
            }),
        rename_test_started: false,
        fork_test_started: false,
        archive_test_started: false,
        empty_selection_seen: false,
        fork_test_session: None,
        models_selftest,
        model_test_stage: 0,
        context_selftest,
        context_stage: 0,
        creation_selftest,
        creation_stage: 0,
        creation_scope: None,
        annotation_selftest,
        annotation_stage: 0,
        text: TextBackend {
            shaper: CoreTextShaper::new(fonts.clone()),
            rasterizer: CoreTextRasterizer::new(fonts),
            scale: 1.,
            type_scale: 1.,
            atlas: GlyphAtlas::default(),
            started: Instant::now(),
            frame_requested: false,
            reduced_motion: false,
            caret_on: true,
        },
        scene: Scene::new(),
        palette: Palette::from(Theme::LIGHT),
        gpu: None,
        executor,
        runtime,
        sender,
        results,
        wake: None,
        observers: vec![],
        selftest,
        system_ime_selftest,
        system_ime_initial_users,
        failures: vec![],
        focus_before_deactivate: None,
    };
    if let Some(index) = args.iter().position(|arg| arg == "--offscreen") {
        let history_page_proof = if args.iter().any(|arg| arg == "--history-page-proof") {
            host.executor
                .block_on(tekes_agent_ui_demo::history_proof::run(
                    &mut host.view,
                    &host.runtime,
                ))
                .expect("public history page proof")
        } else {
            serde_json::Value::Null
        };
        let actionable_proof = if args.iter().any(|arg| arg == "--actionable-proof") {
            host.executor
                .block_on(tekes_agent_ui_demo::actionable_proof::run(
                    &mut host.view,
                    &host.runtime,
                    kind,
                ))
                .expect("public actionable proof")
        } else {
            serde_json::Value::Null
        };
        let mut model_proof = serde_json::Value::Null;
        let mut context_proof = serde_json::Value::Null;
        let mut annotation_proof = serde_json::Value::Null;
        assert!(
            !args.iter().any(|arg| arg == "--annotation-proof")
                || args.iter().any(|arg| arg == "--context-proof"),
            "annotation proof requires context proof"
        );
        let mut preset_proof = serde_json::Value::Null;
        let mut upload_proof = serde_json::Value::Null;
        if args.iter().any(|arg| arg == "--upload-proof") {
            upload_proof = host
                .executor
                .block_on(tekes_agent_ui_demo::upload_proof::run(
                    &mut host.view,
                    &host.runtime,
                ))
                .expect("public upload proof");
        }
        if args.iter().any(|arg| arg == "--model-choice-proof") {
            let commands = host
                .view
                .accessibility_action(host.view.ids().model, None, AccessibilityAction::Press)
                .unwrap();
            let DemoCommand::ReadModels { binding, request } =
                commands.into_iter().next().expect("model catalog intent")
            else {
                panic!("model catalog intent")
            };
            let options = host
                .executor
                .block_on(binding.model_choices(1, "offscreen-model-read"))
                .expect("public model catalog");
            let chosen = options
                .catalog
                .selected
                .as_ref()
                .and_then(|id| {
                    options
                        .catalog
                        .choices
                        .iter()
                        .find(|c| &c.id == id && c.enabled)
                })
                .or_else(|| options.catalog.choices.iter().find(|c| c.enabled))
                .expect("explicit synthetic model choice")
                .id
                .clone();
            let effort = options
                .reasoning_choices(&chosen, None)
                .expect("public reasoning metadata")
                .and_then(|choices| {
                    choices
                        .catalog
                        .choices
                        .iter()
                        .find(|row| row.enabled)
                        .map(|row| row.id.clone())
                });
            let count = options.catalog.choices.len();
            host.view
                .complete_model_catalog(&request, Ok(options))
                .unwrap();
            let target = host.view.model_choice_id(&chosen).unwrap();
            host.view
                .accessibility_action(target, None, AccessibilityAction::Press)
                .unwrap();
            if let Some(effort) = &effort {
                let commands = host
                    .view
                    .accessibility_action(host.view.ids().model, None, AccessibilityAction::Press)
                    .unwrap();
                let DemoCommand::ReadModels {
                    binding: source,
                    request,
                } = commands
                    .into_iter()
                    .next()
                    .expect("reasoning catalog intent")
                else {
                    panic!("model read")
                };
                let options = host
                    .executor
                    .block_on(source.model_choices(1, "offscreen-reasoning-read"))
                    .expect("fresh reasoning catalog");
                host.view
                    .complete_model_catalog(&request, Ok(options))
                    .unwrap();
                let target = host
                    .view
                    .reasoning_choice_id(effort)
                    .expect("public reasoning button");
                host.view
                    .accessibility_action(target, None, AccessibilityAction::Press)
                    .unwrap();
                assert_eq!(
                    source.current_run().unwrap().draft.settings
                        [tekes_agent_ui_bindings::MODEL_SELECTION_SETTING]["reasoningEffort"],
                    *effort
                );
            }
            let destination =
                tekes_agent_presentation::ComposerDestination::run(binding.scope().clone());
            let previous = binding
                .current_run()
                .unwrap()
                .transcript
                .items
                .iter()
                .filter(|i| {
                    i.kind == tekes_agent_presentation::TranscriptItemKind::Assistant && i.is_final
                })
                .count();
            binding
                .edit_text(&destination, "AgentKit explicit model-choice proof".into())
                .unwrap();
            host.executor.block_on(async {
                let mut changes = binding.subscribe_run().unwrap();
                binding
                    .send_with(
                        &destination,
                        tekes_agent_contract::PromptMode::Queue,
                        &ModelSelectionPreparation,
                    )
                    .await
                    .unwrap();
                tokio::time::timeout(Duration::from_secs(30), async {
                    loop {
                        let snapshot = changes.borrow_and_update().clone();
                        if snapshot.can_submit
                            && snapshot
                                .transcript
                                .items
                                .iter()
                                .filter(|i| {
                                    i.kind
                                        == tekes_agent_presentation::TranscriptItemKind::Assistant
                                        && i.is_final
                                })
                                .count()
                                > previous
                        {
                            break;
                        }
                        changes.changed().await.unwrap();
                    }
                })
                .await
                .expect("model-choice assistant settlement");
            });
            host.view.sync().unwrap();
            model_proof = serde_json::json!({"catalogChoices":count,"selected":chosen,"explicitSelection":true,"configurationReceipt":true,"promptSettled":true,"reasoningAvailable":effort.is_some(),"reasoningSelected":effort,"nativeInteraction":false});
        }
        if args.iter().any(|arg| arg == "--context-proof") {
            use tekes_agent_components_ui::{Composer, ComposerContextAction, ComposerContextItem};
            use tekes_agent_presentation::{
                ComposerArtifact, ComposerContextBaseline, ComposerDestination,
            };
            let binding = host.view.binding().clone();
            let (_, annotations) = tekes_agent_ui_demo::fixture::composer_context_fixture();
            let keep = ComposerArtifact::Reference {
                id: "keep".into(),
                mention: Some("上下文引用说明".into()),
                path: None,
                excerpt: None,
            };
            let remove = ComposerArtifact::Reference {
                id: "discard".into(),
                mention: Some("CONTEXT_NOT_SUBMITTED".into()),
                path: None,
                excerpt: None,
            };
            assert!(
                binding
                    .patch_context(
                        &ComposerContextBaseline::read(&binding.current_run().unwrap()).unwrap(),
                        vec![keep, remove.clone()],
                        annotations
                    )
                    .unwrap()
            );
            host.view.sync().unwrap();
            let target = host
                .view
                .tree
                .get(host.view.ids().root)
                .unwrap()
                .component::<Composer>()
                .unwrap()
                .context()
                .unwrap()
                .action_id(
                    &ComposerContextItem::Artifact(remove),
                    ComposerContextAction::Remove,
                )
                .unwrap();
            host.view
                .accessibility_action(target, None, AccessibilityAction::Press)
                .unwrap();
            if args.iter().any(|arg| arg == "--annotation-proof") {
                let original = binding.current_run().unwrap().draft.annotations[0].clone();
                let target = host
                    .view
                    .tree
                    .get(host.view.ids().root)
                    .unwrap()
                    .component::<Composer>()
                    .unwrap()
                    .context()
                    .unwrap()
                    .action_id(
                        &ComposerContextItem::Annotation(original.clone()),
                        ComposerContextAction::EditAnnotation,
                    )
                    .unwrap();
                host.view
                    .accessibility_action(target, None, AccessibilityAction::Press)
                    .unwrap();
                let ids = host.view.annotation_editor_ids().unwrap();
                host.view
                    .event(&ContentEvent::TextCommand("selectAll:".into()))
                    .unwrap();
                host.view
                    .event(&ContentEvent::Commit {
                        text: "请解释这段内容：更新后的批注\n第二行 😀".into(),
                        replacement: None,
                    })
                    .unwrap();
                let mut expected = original;
                expected.comment = "请解释这段内容：更新后的批注\n第二行 😀".into();
                assert_eq!(
                    binding.current_run().unwrap().draft.annotations[0],
                    expected
                );
                host.view
                    .accessibility_action(ids.close, None, AccessibilityAction::Press)
                    .unwrap();
                assert!(host.view.annotation_editor_ids().is_none());
            }
            let destination = ComposerDestination::run(binding.scope().clone());
            let previous = binding
                .current_run()
                .unwrap()
                .transcript
                .items
                .iter()
                .filter(|i| {
                    i.kind == tekes_agent_presentation::TranscriptItemKind::Assistant && i.is_final
                })
                .count();
            binding
                .edit_text(&destination, "AgentKit context UI proof".into())
                .unwrap();
            host.executor.block_on(async {
                let mut changes = binding.subscribe_run().unwrap();
                binding
                    .send_with(
                        &destination,
                        tekes_agent_contract::PromptMode::Queue,
                        &ModelSelectionPreparation,
                    )
                    .await
                    .unwrap();
                tokio::time::timeout(Duration::from_secs(30), async {
                    loop {
                        let snapshot = changes.borrow_and_update().clone();
                        if snapshot.can_submit
                            && snapshot
                                .transcript
                                .items
                                .iter()
                                .filter(|i| {
                                    i.kind
                                        == tekes_agent_presentation::TranscriptItemKind::Assistant
                                        && i.is_final
                                })
                                .count()
                                > previous
                        {
                            break;
                        }
                        changes.changed().await.unwrap();
                    }
                })
                .await
                .expect("context assistant settlement");
            });
            let snapshot = binding.current_run().unwrap();
            let input = snapshot
                .transcript
                .items
                .iter()
                .find(|i| {
                    i.kind == tekes_agent_presentation::TranscriptItemKind::User
                        && i.text.contains("AgentKit context UI proof")
                })
                .expect("restored context input");
            assert!(
                input.text.contains("上下文引用说明")
                    && input.text.contains("中文引用")
                    && input.text.contains("请解释这段内容")
            );
            assert!(!input.text.contains("CONTEXT_NOT_SUBMITTED"));
            if args.iter().any(|arg| arg == "--annotation-proof") {
                assert!(input.text.contains("更新后的批注") && input.text.contains("第二行"));
                annotation_proof = serde_json::json!({"componentEditor":true,"committedComment":true,"sourceRetained":true,"sharedCompilation":true,"promptSettled":true,"nativeInteraction":false,"physicalIME":false,"sourceHighlight":false});
            }
            assert!(snapshot.draft.artifacts.is_empty() && snapshot.draft.annotations.is_empty());
            host.view.sync().unwrap();
            context_proof = serde_json::json!({"mountedCards":true,"explicitRemoval":true,"referenceReturned":true,"annotationReturned":true,"promptSettled":true,"clearedDraft":true,"fileUpload":false,"nativeInteraction":false,"structuredTranscript":false});
        }
        if args.iter().any(|arg| arg == "--initial-preset-proof") {
            use tekes_agent_presentation::ComposerDestination;
            use tekes_agent_ui_bindings::MODEL_SELECTION_SETTING;
            let cwd_index = args
                .iter()
                .position(|arg| arg == "--initial-preset-cwd")
                .expect("explicit preset create cwd");
            let cwd = args.get(cwd_index + 1).expect("preset create cwd").clone();
            let original = host.view.binding().clone();
            let original_draft = original.current_run().unwrap().draft.clone();
            let models = host
                .executor
                .block_on(original.model_choices(1, "proof-creation-models"))
                .unwrap();
            let model = models
                .selection(
                    models
                        .catalog
                        .selected
                        .as_ref()
                        .expect("synthetic current model"),
                )
                .unwrap();
            host.view
                .configure_creation(
                    tekes_agent_contract::CreateSessionRequest {
                        cwd: Some(cwd),
                        workspace_id: None,
                        session_id: None,
                        agent_preset: None,
                    },
                    "proof-component-create",
                )
                .unwrap();
            let mut commands = host
                .view
                .accessibility_action(
                    tekes_agent_ui_demo::component_demo::NEW_SESSION,
                    None,
                    AccessibilityAction::Press,
                )
                .unwrap();
            let DemoCommand::ReadInitialPresets { request } = commands.remove(0) else {
                panic!("new session catalog intent")
            };
            let source = host
                .executor
                .block_on(InitialPresetChoices::load(
                    &host.runtime,
                    1,
                    "proof-creation-presets",
                ))
                .unwrap();
            // This synthetic proof explicitly chooses a row; production never auto-selects it.
            let chosen = source
                .catalog
                .choices
                .iter()
                .find(|row| row.enabled)
                .expect("selectable initial preset")
                .id
                .clone();
            let count = source.catalog.choices.len();
            assert!(
                host.view
                    .complete_initial_presets(&request, Ok(source))
                    .unwrap()
            );
            let target = host.view.initial_preset_choice_id(&chosen).unwrap();
            let mut commands = host
                .view
                .accessibility_action(target, None, AccessibilityAction::Press)
                .unwrap();
            let DemoCommand::CreateSession(command) = commands.remove(0) else {
                panic!("captured new session intent")
            };
            let binding = host.executor.block_on(async {
                let created = command.creation().execute(&host.runtime).await.unwrap();
                Arc::new(
                    RunBinding::open_main(
                        host.runtime.clone(),
                        RunScope::main(command.creation().endpoint_id(), created.session_id),
                    )
                    .await
                    .unwrap(),
                )
            });
            assert!(
                host.view
                    .complete_creation(&command, Ok(binding.clone()))
                    .unwrap()
            );
            assert_eq!(host.view.binding().scope(), binding.scope());
            let destination = ComposerDestination::run(binding.scope().clone());
            assert!(
                binding
                    .begin_settings(&destination)
                    .unwrap()
                    .commit(std::collections::BTreeMap::from([(
                        MODEL_SELECTION_SETTING.into(),
                        model
                    )]))
                    .unwrap()
            );
            binding
                .edit_text(&destination, "AgentKit initial preset UI proof".into())
                .unwrap();
            host.view.sync().unwrap();
            let mut commands = host
                .view
                .accessibility_action(host.view.ids().send, None, AccessibilityAction::Press)
                .unwrap();
            let DemoCommand::Send {
                binding: send_binding,
                ticket,
            } = commands.remove(0)
            else {
                panic!("normal created Composer send")
            };
            assert!(Arc::ptr_eq(&binding, &send_binding));
            host.executor.block_on(async {
                let mut changes = binding.subscribe_run().unwrap();
                binding
                    .dispatch_prepared_with(ticket, &ModelSelectionPreparation)
                    .await
                    .unwrap();
                tokio::time::timeout(Duration::from_secs(30), async {
                    loop {
                        let snapshot = binding.current_run().unwrap();
                        if snapshot.can_submit
                            && snapshot.transcript.items.iter().any(|i| {
                                i.kind == tekes_agent_presentation::TranscriptItemKind::Assistant
                                    && i.is_final
                            })
                        {
                            break;
                        }
                        changes.changed().await.unwrap();
                    }
                })
                .await
                .expect("created preset prompt settlement");
            });
            let snapshot = binding.current_run().unwrap();
            assert_ne!(binding.scope().session_id, original.scope().session_id);
            assert_eq!(
                snapshot
                    .transcript
                    .items
                    .iter()
                    .filter(
                        |i| i.kind == tekes_agent_presentation::TranscriptItemKind::User
                            && i.text == "AgentKit initial preset UI proof"
                    )
                    .count(),
                1
            );
            assert_eq!(original.current_run().unwrap().draft, original_draft);
            let returned = host
                .runtime
                .inspect(|s| {
                    s.sessions
                        .get(&binding.scope().session_id)
                        .and_then(|s| s.summary.clone())
                })
                .expect("created session inventory");
            let returned_preset = returned["initialPresetId"]
                .as_str()
                .or_else(|| returned["initialPresetID"].as_str())
                .or_else(|| returned["agentPreset"].as_str())
                .or_else(|| returned["identityProfile"].as_str())
                .map(str::to_owned)
                .or_else(|| {
                    host.runtime.inspect(|s| {
                        s.sessions
                            .get(&binding.scope().session_id)
                            .and_then(|s| s.projections.get("agentPreset"))
                            .and_then(|value| value.value.as_str().map(str::to_owned))
                    })
                });
            if let Some(id) = &returned_preset {
                assert_eq!(id, &chosen, "preset identity round trip");
            }
            if kind != "pi" {
                assert!(returned_preset.is_some(), "preset identity not published");
            }
            host.view.sync().unwrap();
            preset_proof = serde_json::json!({"catalogChoices":count,"explicitSelection":true,"createdSession":true,"selectedPreset":chosen,"presetIdentityReturned":returned_preset.is_some(),"singleInput":true,"promptSettled":true,"originalDraftPreserved":true,"nativeInteraction":false,"componentCreationEntry":true});
        }
        if args.iter().any(|arg| arg == "--input-message-proof") {
            input_message_ids = Some(
                host.view
                    .mount_input_message_proof()
                    .expect("canonical input message row"),
            );
        }
        if args.iter().any(|arg| arg == "--output-message-proof") {
            output_message_id = Some(
                host.view
                    .mount_output_message_proof()
                    .expect("canonical machine message row"),
            );
        }
        if args.iter().any(|arg| arg == "--turn-footnote-proof") {
            turn_footnote_id = Some(
                host.view
                    .mount_turn_footnote_proof()
                    .expect("turn footnote source"),
            );
        }
        let turn_fork_proof = if args.iter().any(|arg| arg == "--turn-fork-proof") {
            assert!(
                turn_footnote_id.is_some(),
                "turn fork proof requires --turn-footnote-proof"
            );
            host.executor
                .block_on(tekes_agent_ui_demo::turn_footnote_proof::run(
                    &mut host.view,
                    &host.runtime,
                    host.sidebar.turn_action_dispatcher(),
                ))
                .expect("shared turn fork proof")
        } else {
            serde_json::Value::Null
        };
        let feedback_read_proof = if args.iter().any(|arg| arg == "--feedback-read-proof") {
            host.executor
                .block_on(tekes_agent_ui_demo::feedback_read_proof::run(&host.view))
                .expect("committed feedback read")
        } else {
            serde_json::Value::Null
        };
        let bounded_child_proof = if args.iter().any(|arg| arg == "--bounded-child-proof") {
            let index = args
                .iter()
                .position(|arg| arg == "--bounded-child-id")
                .expect("explicit child ID required");
            let child = args.get(index + 1).expect("child ID argument");
            host.executor
                .block_on(tekes_agent_ui_demo::readonly_child_proof::run(
                    &mut host.view,
                    &host.runtime,
                    child,
                ))
                .expect("read-only child content proof")
        } else {
            serde_json::Value::Null
        };
        let clip = Rect::new(0., 0., 1000., 760.);
        host.text.atlas.begin_frame();
        host.view
            .render(
                &mut host.scene,
                &mut host.text,
                clip.inset(16.),
                clip,
                host.palette,
            )
            .unwrap();
        let bytes = Renderer::new_headless()
            .unwrap()
            .render_to_rgba8(
                &host.scene,
                &mut host.text.atlas,
                (1000, 760),
                1.,
                host.palette.color("surface-1").unwrap(),
            )
            .unwrap();
        let mut encoder = png::Encoder::new(
            std::fs::File::create(args.get(index + 1).expect("PNG path")).unwrap(),
            1000,
            760,
        );
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&bytes)
            .unwrap();
        let snapshot = host.view.binding().current_run().expect("main snapshot");
        let document = tekes_agent_presentation::TranscriptDocument::read(&snapshot.transcript)
            .expect("canonical Transcript lanes");
        let document_messages: usize = document.session_notices.len()
            + document
                .sections
                .iter()
                .map(|section| section.inputs.len() + section.outputs.len())
                .sum::<usize>();
        if args.iter().any(|arg| arg == "--session-notice-fixture") {
            assert!(
                fixture,
                "notice fixture requires the isolated fixture endpoint"
            );
            assert_eq!(document.session_notices.len(), 1);
            assert_eq!(
                document.session_notices[0].item.text,
                "Local worker launch notice fixture"
            );
        }
        let document_tools: usize = document
            .sections
            .iter()
            .map(|section| section.tools.len())
            .sum();
        assert_eq!(
            document_messages,
            snapshot
                .transcript
                .items
                .iter()
                .filter(|item| item.is_visible)
                .count()
        );
        assert_eq!(document_tools, snapshot.transcript.tools.len());
        let mut structured_messages = 0usize;
        let mut aggregate_messages = 0usize;
        let mut content_block_count = 0usize;
        let mut unresolved_tool_blocks = 0usize;
        let mut unknown_blocks = 0usize;
        for item in snapshot
            .transcript
            .items
            .iter()
            .filter(|item| item.is_visible)
        {
            if let Some(blocks) = &item.content_blocks {
                structured_messages += 1;
                content_block_count += blocks.len();
                for block in blocks {
                    use tekes_agent_presentation::TranscriptBlockContent as Block;
                    match &block.content {
                        Block::ToolCall { .. }
                            if !document.sections.iter().any(|section| {
                                section.id == item.turn_id
                                    && section.tool_for_block(block).is_some()
                            }) =>
                        {
                            unresolved_tool_blocks += 1;
                        }
                        Block::Unknown { .. } => unknown_blocks += 1,
                        _ => {}
                    }
                }
            } else {
                aggregate_messages += 1;
            }
        }
        assert_eq!(structured_messages + aggregate_messages, document_messages);
        let mut positioned_events = 0usize;
        let mut unplaced_events = 0usize;
        let mut unplaced_facts = Vec::new();
        for section in &document.sections {
            let order = section
                .event_order()
                .expect("canonical Transcript source order");
            positioned_events += order.ordered.len();
            unplaced_events += order.unplaced.len();
            for event in order.unplaced {
                use tekes_agent_presentation::TranscriptOrderedEvent as Event;
                unplaced_facts.push(match event {
                    Event::Message(item) => serde_json::json!({"kind":"message","id":item.id,"recordId":item.record_id,"provisional":item.is_provisional,"sourceSequence":item.source_sequence}),
                    Event::ToolCall(tool) => serde_json::json!({"kind":"toolCall","id":tool.id,"name":tool.name,"hasResult":tool.result.is_some(),"argumentsPresent":!tool.arguments.is_null(),"resultPosition":tool.result_position}),
                    Event::ToolResult(tool) => serde_json::json!({"kind":"toolResult","id":tool.id,"name":tool.name}),
                });
            }
        }
        let transcript_content_proof=host.view.transcript_content_id(host.view.ids()).map(|id| {
            let row=host.view.tree.get(id).unwrap();let content=row.component::<tekes_agent_components_ui::TranscriptContent>().unwrap();let frame=row.frame();
            let composer=host.view.tree.get(host.view.ids().root).unwrap().frame();
            assert!(composer.min_y()>=0. && composer.max_y()<=760., "content viewport must leave Composer inside the host window");
            let mut messages=0;let mut footnotes=0;let mut tools=0;let mut known_empty=0;let mut output_copies=0;
            for key in content.row_keys() {
                let node=host.view.tree.get(content.row_id(key).unwrap()).unwrap();
                if let Some(input)=node.component::<tekes_agent_components_ui::InputMessage>() {
                    let ids=input.ids();let bubble=host.view.tree.get(ids.bubble).unwrap().frame();let caption=host.view.tree.get(ids.footnote).unwrap().frame();
                    assert!((bubble.max_x()-frame.max_x()).abs()<0.5);assert!((caption.max_x()-frame.max_x()).abs()<0.5);
                    messages+=1;
                } else if let Some(output)=node.component::<tekes_agent_components_ui::OutputMessage>() {assert!((node.frame().min_x()-frame.min_x()).abs()<0.5);if !output.active(){assert!(output.content_ids().is_empty());known_empty+=1;}messages+=1;}
                else if let Some(footer)=node.component::<tekes_agent_components_ui::TurnFootnote>() {assert!((node.frame().min_x()-frame.min_x()).abs()<0.5);if let Some(copy)=footer.copy_button_id(){assert!((host.view.tree.get(copy).unwrap().frame().min_x()-frame.min_x()).abs()<0.5);output_copies+=1;}footnotes+=1;}
                else if node.component::<tekes_agent_components_ui::ToolActivity>().is_some() {assert!((node.frame().min_x()-frame.min_x()).abs()<0.5);tools+=1;}
            }
            assert_eq!(messages, snapshot.transcript.items.iter().filter(|item|item.is_visible).count());
            assert_eq!(output_copies, document.sections.iter().filter(|section|section.final_output().is_some()).count());
            serde_json::json!({"renderer":true,"rows":content.row_keys().len(),"knownEmptyContainersHidden":known_empty,"outputCopyControls":output_copies,"messages":messages,"footnotes":footnotes,"independentTools":tools,"userTrailing":true,"machineLeading":true,"genericViewport":true,"keyedReadingAcceptance":false,"richContent":false,"completeTranscript":false,"nativeInteraction":false})
        });
        let turn_footnote_proof=turn_footnote_id.map(|id| {
            let row=host.view.tree.get(id).unwrap();let frame=row.frame();
            for child in row.component::<tekes_agent_components_ui::TurnFootnote>().unwrap().children() {let f=host.view.tree.get(child.id()).unwrap().frame();assert!((f.min_x()-frame.min_x()).abs()<0.5);}
            let facts=row.component::<tekes_agent_components_ui::TurnFootnote>().unwrap().facts().unwrap();
            serde_json::json!({"leadingAside":true,"elapsedSeconds":facts.elapsed_seconds(),"startedAtMilliseconds":facts.started_at,"endedAtMilliseconds":facts.ended_at,"nativeInteraction":false,"completeTranscript":false})
        });
        let output_message_proof = output_message_id.map(|id| {
            let row = host.view.tree.get(id).unwrap();
            let frame = row.frame();
            let ids = row.component::<tekes_agent_components_ui::OutputMessage>().unwrap().content_ids();
            for part in &ids {
                let f = host.view.tree.get(part.root).unwrap().frame();
                assert!((f.min_x() - frame.min_x()).abs() < 0.5);
                if let Some(body) = host.view.tree.text_surface(part.body) { assert!(body.input.read_only); }
            }
            serde_json::json!({"leadingContent":true,"parts":ids.len(),"completeTranscript":false,"nativeInteraction":false})
        });
        let input_message_proof = input_message_ids.map(|ids| {
            let row = host.view.tree.get(ids.root).unwrap().frame();
            let bubble = host.view.tree.get(ids.bubble).unwrap().frame();
            let note = host.view.tree.get(ids.footnote).unwrap().frame();
            let body = host.view.tree.text_surface(ids.body).unwrap();
            assert!((bubble.max_x()-row.max_x()).abs()<0.5);
            assert!((note.max_x()-row.max_x()).abs()<0.5);
            assert!(note.min_y()>=bubble.max_y());
            assert!(body.input.read_only);
            assert_eq!(body.presentation, tekes_ui_controls::TextSurfacePresentation::Content);
            serde_json::json!({"rightBubble":true,"externalRightFootnote":true,"plainReadonlyContent":true,"bubbleWidth":bubble.size.width,"rowWidth":row.size.width,"completeTranscript":false,"nativeInteraction":false})
        });
        let run_view = host
            .view
            .tree
            .get(ComponentId(host.view.ids().root.0 + 16))
            .unwrap()
            .component::<tekes_agent_components_ui::RunView>()
            .expect("shared RunView mounted");
        let run_view_proof = serde_json::json!({"mounted":true,"transcriptMounted":run_view.transcript_id().is_some(),
            "sharedIntents":true,"ownsRuntime":false,"completeRunView":false,"keyedReadingAcceptance":false});
        println!(
            "{}",
            serde_json::json!({
                "runViewComposition":run_view_proof,"boundedChildProof":bounded_child_proof,"feedbackReadProof":feedback_read_proof,
                "offscreen": true, "components": true, "nativeInteraction": false,
                "transcriptView": false, "isStale": snapshot.transcript.is_stale,
                "transcriptItems": snapshot.transcript.items.len(),
                "documentProjection": {"sections":document.sections.len(),"visibleMessages":document_messages,"sessionNotices":document.session_notices.len(),"toolCalls":document_tools,"hasMoreBefore":document.has_more_before,"isStale":document.is_stale,"renderer":false,"structuredMessages":structured_messages,"aggregateMessages":aggregate_messages,"contentBlocks":content_block_count,"unresolvedToolBlocks":unresolved_tool_blocks,"unknownBlocks":unknown_blocks,"positionedEvents":positioned_events,"unplacedEvents":unplaced_events,"unplacedFacts":unplaced_facts},
                "finalAssistantItems": snapshot.transcript.items.iter().filter(|item| item.kind == tekes_agent_presentation::TranscriptItemKind::Assistant && item.is_final).count(),
            "inventoryRows": host.sidebar.snapshot().rows.len(),
            "sharedRuntime": Arc::ptr_eq(host.sidebar.runtime(), &host.runtime),
            "sidebarMounted": host.view.tree.get(host.view.sidebar_id()).is_some(),
            "summaryMounted": host.view.tree.get(host.view.summary_id()).is_some(),
            "modelChoiceProof":model_proof,
            "contextProof":context_proof,
            "annotationProof":annotation_proof,
            "initialPresetProof":preset_proof,
            "uploadProof":upload_proof,
            "actionableProof":actionable_proof,
            "historyPagingProof":history_page_proof,
            "inputMessageProof":input_message_proof, "outputMessageProof":output_message_proof, "turnFootnoteProof":turn_footnote_proof,"turnForkProof":turn_fork_proof,"transcriptContentProof":transcript_content_proof,
            })
        );
        host.executor.block_on(host.runtime.stop()).unwrap();
        return;
    }
    App::new(AppConfig {
        name: Some("AgentKit Components Demo".into()),
        ..Default::default()
    })
    .run(
        WindowConfig {
            title: "AgentKit · Composer / Indicators 验证".into(),
            size: Size {
                width: 1000.,
                height: 760.,
            },
            min_size: Some(Size {
                width: 820.,
                height: 520.,
            }),
            placement: tekes_ui_platform_macos::WindowPlacement::Auto {
                need_120hz: false,
                size_check: false,
            },
            ..Default::default()
        },
        MenuBar::default(),
        host,
    );
}
fn context_script(view: &ComponentDemo) {
    use tekes_agent_components_ui::{Composer, ComposerContextAction, ComposerContextItem};
    use tekes_ui_platform_macos::selftest::{Action, run_script};
    let ids = view.ids();
    let attachment = view.binding().current_run().unwrap().draft.artifacts[0].clone();
    let remove = view
        .tree
        .get(ids.root)
        .unwrap()
        .component::<Composer>()
        .unwrap()
        .context()
        .unwrap()
        .action_id(
            &ComposerContextItem::Artifact(attachment),
            ComposerContextAction::Remove,
        )
        .unwrap();
    let click = |id| {
        let p = view.tree.get(id).unwrap().frame().center();
        Action::Click {
            x: p.x as f64,
            y: p.y as f64,
        }
    };
    let mut script = vec![];
    if let Ok(path) = std::env::var("AGENT_CONTEXT_CAPTURE") {
        script.push((100, Action::Capture { path }));
    }
    script.extend([
        (200, click(ids.editor)),
        (300, Action::TextCommand("selectAll:")),
        (400, Action::Text(String::new())),
        (
            500,
            Action::ExpectText {
                text: String::new(),
                marked: false,
            },
        ),
        (
            700,
            Action::PreeditRange {
                text: "上下文组字".into(),
                selection: 5..5,
                replacement: None,
            },
        ),
        (900, click(remove)),
        (
            1100,
            Action::ExpectText {
                text: "上下文组字".into(),
                marked: true,
            },
        ),
        (1300, Action::Text("上下文消息".into())),
        (1600, click(remove)),
        (15000, Action::Close),
    ]);
    run_script(script);
}
/// Opt-in integration probe. The caller must verify the active macOS Simplified
/// Pinyin source; no Preedit/Text injection and no global input-source changes.
fn system_ime_script(view: &ComponentDemo) {
    use tekes_ui_platform_macos::selftest::{Action, run_script};
    let click = |id| {
        let point = view.tree.get(id).unwrap().frame().center();
        Action::Click {
            x: point.x as f64,
            y: point.y as f64,
        }
    };
    let ids = view.ids();
    let sidebar = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap();
    let other = sidebar
        .row_id(&tekes_agent_contract::SessionAddress {
            endpoint_id: view.binding().scope().endpoint_id.clone(),
            session_id: "b".into(),
        })
        .unwrap();
    run_script(vec![
        (300, click(ids.editor)),
        (600, Action::Log("SYSTEM_IME_TYPE_READY".into())),
        (
            2000,
            Action::ExpectText {
                text: "ni hao".into(),
                marked: true,
            },
        ),
        (2300, click(ids.send)),
        (
            2600,
            Action::ExpectText {
                text: "ni hao".into(),
                marked: true,
            },
        ),
        (2900, click(other)),
        (
            3200,
            Action::ExpectText {
                text: "ni hao".into(),
                marked: true,
            },
        ),
        (3500, Action::Log("SYSTEM_IME_COMMIT_READY".into())),
        (
            4500,
            Action::ExpectText {
                text: "你好".into(),
                marked: false,
            },
        ),
        (4800, click(ids.send)),
        (7000, Action::Close),
    ]);
}
fn native_script(view: &ComponentDemo) {
    use tekes_ui_platform_macos::selftest::{Action, run_script};
    let click = |id| {
        let point = view.tree.get(id).unwrap().frame().center();
        Action::Click {
            x: point.x as f64,
            y: point.y as f64,
        }
    };
    let ids = view.ids();
    let editor = click(ids.editor);
    let sidebar = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap();
    let address = tekes_agent_contract::SessionAddress {
        endpoint_id: view.binding().scope().endpoint_id.clone(),
        session_id: "a".into(),
    };
    let pin = sidebar
        .action_id(&address, "pin")
        .expect("owned fixture pin action");
    let select_a = sidebar.row_id(&address).unwrap();
    let mut address_b = address.clone();
    address_b.session_id = "b".into();
    let select_b = sidebar.row_id(&address_b).unwrap();
    let mut script = vec![
        (250, click(pin)),
        (400, editor.clone()),
        (500, Action::Preedit("中文组字".into())),
        (700, click(ids.send)),
        (
            850,
            Action::ExpectText {
                text: "中文组字".into(),
                marked: true,
            },
        ),
        (1000, Action::Text("中文消息".into())),
        (1200, click(ids.send)),
        (2800, editor.clone()),
        (3000, Action::Text("停止演示".into())),
        (3200, click(ids.send)),
        (3500, click(ids.cancel)),
        (4100, click(select_b)),
        (4300, Action::Text("B 会话草稿".into())),
        (4500, click(select_a)),
        (
            4700,
            Action::ExpectText {
                text: "".into(),
                marked: false,
            },
        ),
        (4900, Action::Text("A 会话草稿".into())),
        (5100, click(select_b)),
        (
            5300,
            Action::ExpectText {
                text: "B 会话草稿".into(),
                marked: false,
            },
        ),
        (5600, click(RECONNECT)),
    ];
    let panel = view
        .tree
        .get(view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap();
    script.push((6100, click(panel.create_side_chat_id())));
    // A bounded failure deadline also catches a missing creation/body notification.
    script.push((20000, Action::Close));
    run_script(script);
}
fn side_chat_script(view: &ComponentDemo) {
    use tekes_ui_platform_macos::selftest::{Action, run_script};
    let click = |id| {
        let point = view.tree.get(id).unwrap().frame().center();
        Action::Click {
            x: point.x as f64,
            y: point.y as f64,
        }
    };
    let ids = view.panel_composer_ids().expect("mounted scratch composer");
    let panel = view
        .tree
        .get(view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap();
    let mut script = vec![
        (100, click(ids.editor)),
        (200, Action::Text("SideChat 独立输入".into())),
        (400, click(ids.send)),
        (600, click(ids.editor)),
        (700, Action::Text("SideChat 暂存草稿".into())),
    ];
    if let Ok(path) = std::env::var("AGENT_COMPONENTS_CAPTURE") {
        script.push((800, Action::Capture { path }));
    }
    script.push((900, click(panel.close_id())));
    run_script(script);
}
fn task_script(view: &ComponentDemo) {
    use tekes_ui_platform_macos::selftest::{Action, run_script};
    let click = |id| {
        let point = view.tree.get(id).unwrap().frame().center();
        Action::Click {
            x: point.x as f64,
            y: point.y as f64,
        }
    };
    let ids = view.panel_composer_ids().expect("resolved task Composer");
    let panel = view
        .tree
        .get(view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap();
    let mut script = vec![
        (100, click(ids.editor)),
        (200, Action::Text("任务消息".into())),
        (400, click(ids.send)),
        (600, click(ids.editor)),
        (700, Action::Text("任务暂存草稿".into())),
        (900, click(ids.cancel)),
    ];
    if let Ok(path) = std::env::var("AGENT_TASK_CAPTURE") {
        script.push((1300, Action::Capture { path }));
    }
    script.push((1600, click(panel.close_id())));
    run_script(script);
}
fn child_script(view: &ComponentDemo) {
    use tekes_ui_platform_macos::selftest::{Action, run_script};
    let click = |id| {
        let point = view.tree.get(id).unwrap().frame().center();
        Action::Click {
            x: point.x as f64,
            y: point.y as f64,
        }
    };
    let ids = view.panel_composer_ids().unwrap();
    let close = view
        .tree
        .get(view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap()
        .close_id();
    let mut script = vec![
        (100, click(ids.editor)),
        (200, Action::Text("子代理消息".into())),
        (400, click(ids.send)),
        (600, click(ids.editor)),
        (700, Action::Text("子代理暂存草稿".into())),
        (900, click(ids.cancel)),
    ];
    if let Ok(path) = std::env::var("AGENT_CHILD_CAPTURE") {
        script.push((1300, Action::Capture { path }));
    }
    script.push((1600, click(close)));
    run_script(script);
}
fn bounded_script(view: &ComponentDemo) {
    use tekes_ui_platform_macos::selftest::{Action, run_script};
    let click = |id| {
        let point = view.tree.get(id).unwrap().frame().center();
        Action::Click {
            x: point.x as f64,
            y: point.y as f64,
        }
    };
    assert!(view.panel_composer_ids().is_none());
    let (refresh, page) = view.bounded_child_buttons().unwrap();
    let close = view
        .tree
        .get(view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap()
        .close_id();
    let mut script = vec![(200, click(page)), (600, click(refresh))];
    if let Ok(path) = std::env::var("AGENT_BOUNDED_CAPTURE") {
        script.push((1000, Action::Capture { path }));
    }
    script.push((1500, click(close)));
    script.push((2200, Action::Close));
    run_script(script);
}
fn click_thread_action_script(view: &ComponentDemo, action: &str) {
    use tekes_ui_platform_macos::selftest::{Action, run_script};
    let binding = view.current_binding().expect("selected action target");
    let address = tekes_agent_contract::SessionAddress {
        endpoint_id: binding.scope().endpoint_id.clone(),
        session_id: binding.scope().session_id.clone(),
    };
    let id = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .action_id(&address, action)
        .expect("supported fixture action");
    let point = view.tree.get(id).unwrap().frame().center();
    run_script(vec![(
        100,
        Action::Click {
            x: point.x as f64,
            y: point.y as f64,
        },
    )]);
}
fn rename_actions_script(view: &ComponentDemo) {
    use tekes_ui_platform_macos::selftest::{Action, run_script};
    let ids = view.rename_ids().unwrap();
    let field = view
        .tree
        .get(ids.editor)
        .unwrap()
        .component::<tekes_ui_controls::composition::TextFieldContent>()
        .unwrap();
    let original = field.surface.input.text.encode_utf16().count();
    let point = view.tree.get(ids.submit).unwrap().frame().center();
    run_script(vec![
        (
            200,
            Action::PreeditRange {
                text: "重命名组字".into(),
                selection: 5..5,
                replacement: Some(0..original),
            },
        ),
        (
            400,
            Action::Click {
                x: point.x as f64,
                y: point.y as f64,
            },
        ),
        (
            600,
            Action::ExpectText {
                text: "重命名组字".into(),
                marked: true,
            },
        ),
        (800, Action::Text("原生重命名".into())),
        (
            1000,
            Action::Click {
                x: point.x as f64,
                y: point.y as f64,
            },
        ),
    ]);
}
fn restore_selection_script(view: &ComponentDemo) {
    use tekes_ui_platform_macos::selftest::{Action, run_script};
    assert!(view.current_binding().is_none());
    let point = view
        .tree
        .get(tekes_agent_ui_demo::component_demo::SELECT_B)
        .unwrap()
        .frame()
        .center();
    let mut script = vec![];
    if let Ok(path) = std::env::var("AGENT_THREAD_ACTIONS_CAPTURE") {
        script.push((100, Action::Capture { path }));
    }
    script.push((
        500,
        Action::Click {
            x: point.x as f64,
            y: point.y as f64,
        },
    ));
    script.push((1200, Action::Close));
    run_script(script);
}
