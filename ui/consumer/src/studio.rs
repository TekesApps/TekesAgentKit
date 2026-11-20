//! Topology B (`DomainStudioApp`): public-API consumer owning CAD domain navigation & canvas
//! while hosting AgentKit `AgentRunContainer` in `Others` (swappable to `Focus` with zero rebuild),
//! plus domain-specific `CadSpecViewerProvider` injected into `ArtifactViewerContainer`.

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tekes_agent_apphost::{
    AgentRunContainer, ArtifactViewerContainer, ArtifactViewerProvider, ChildRunContainer,
    SharedAgentWorkbench, SideChatContainer, SummaryContainer, TasksQueueContainer,
    WorkspaceChangesContainer, WorkspaceFilesContainer, build_agent_settings_owner,
};
use tekes_agent_contract::{
    ArtifactSelectionReference, EndpointConnector, Utf16SelectionRange, WorkspaceServiceEndpoint,
};
use tekes_agent_presentation::{RunController, RunScope};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{
    ComposerQuoteRequest, DEFAULT_MAIN_SESSION_ID, DeterministicWorkspaceEndpoint,
    FixtureSessionEndpoint, LoadedArtifact, LoadedArtifactContent, RunWorkbenchBinding,
    UiWakeNotifier, WorkbenchCommand, WorkspaceBinding, sha256_prefixed,
};
use tekes_ui_app_host::{
    AppWindow, Container, ContainerFactory, ContainerToolbarHost, ContainerToolbarPosition,
    LocalContainer, NavigationOptions, OthersOptions, OthersTab, PresentationOptions,
    PresentationState, navigation,
};
use tekes_ui_app_protocol::ToolbarItem;
use tekes_ui_controls::{
    ButtonVariant, ScrollArea,
    composition::{
        Button, ComponentId, Content, ContentActionKind, ContentActionRecord, ContentHost,
        RichContent, RichDocument, ScrollContent, StackContent, TextContent, TreeContent,
        TreeContentNode, TreeDisplayUpdate,
    },
    extended::{Axis, KeyedTreeNode, Stack, TreeDatasetKey, TreeNodeKey},
};
use tekes_ui_render::Renderer;
use tekes_ui_tokens::{Spacing, Theme};

pub const DOMAIN_OUTLINER_SHELL_ID: ComponentId = ComponentId(2000);
pub const DOMAIN_OUTLINER_HEADER_ID: ComponentId = ComponentId(2001);
pub const DOMAIN_OUTLINER_TREE_SCROLL_ID: ComponentId = ComponentId(2002);
pub const DOMAIN_OUTLINER_TREE_ID: ComponentId = ComponentId(2003);

pub const DOMAIN_CANVAS_SHELL_ID: ComponentId = ComponentId(2050);
pub const DOMAIN_CANVAS_HEADER_ID: ComponentId = ComponentId(2051);
pub const DOMAIN_CANVAS_METRICS_ID: ComponentId = ComponentId(2052);
pub const DOMAIN_CANVAS_SPEC_RICH_ID: ComponentId = ComponentId(2053);
pub const DOMAIN_CANVAS_QUOTE_BTN_ID: ComponentId = ComponentId(2054);
pub const DOMAIN_CANVAS_OPEN_SPEC_BTN_ID: ComponentId = ComponentId(2055);
pub const DOMAIN_CANVAS_ACTIONS_ROW_ID: ComponentId = ComponentId(2056);

pub const DOMAIN_CAD_CUSTOM_SLOT_ID: ComponentId = ComponentId(1401);
pub const DOMAIN_FOCUS_CMD: u64 = 20_001;

/// Domain-owned `ArtifactViewerProvider` customizing `.cadspec` inspection inside AgentKit's viewer.
pub struct CadSpecViewerProvider;

impl ArtifactViewerProvider for CadSpecViewerProvider {
    fn supports_path(&self, path: &str) -> bool {
        path.ends_with(".cadspec")
    }

    fn build_custom_content(&self, artifact: &LoadedArtifact) -> Option<(Content, String)> {
        let LoadedArtifactContent::CodeText { text, .. } = &artifact.content else {
            return None;
        };
        let param_lines = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .take(6)
            .collect::<Vec<_>>()
            .join("\n");
        let summary = format!(
            "CAD 参数规范定制面板 · {} (rev {})\n{}",
            artifact.path, artifact.revision, param_lines
        );
        Some((
            Content::new(DOMAIN_CAD_CUSTOM_SLOT_ID, TextContent::new(summary)),
            format!("CAD 参数检验: {}", artifact.path),
        ))
    }
}

pub struct DomainStudioRuntime {
    pub executor: Arc<tokio::runtime::Runtime>,
    pub runtime: Arc<SessionRuntime>,
    pub run_binding: Arc<RunWorkbenchBinding>,
    pub workspace_binding: Arc<WorkspaceBinding>,
    pub workbench: SharedAgentWorkbench,
    pub data_root: PathBuf,
}

impl DomainStudioRuntime {
    pub fn setup(data_root: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&data_root)?;
        let executor = Arc::new(tokio::runtime::Runtime::new()?);
        let sep = FixtureSessionEndpoint::with_timing(Duration::ZERO, Duration::from_millis(8));
        let connector: Arc<dyn EndpointConnector> = Arc::new(sep);

        let (runtime, controller) = executor.block_on(async {
            let runtime = Arc::new(SessionRuntime::spawn("fixture", connector));
            runtime.wait_ready(Duration::from_secs(5)).await?;
            let controller = Arc::new(RunController::new(Arc::clone(&runtime))?);
            Ok::<_, anyhow::Error>((runtime, controller))
        })?;

        let workbench = SharedAgentWorkbench::new();
        let wake: Arc<dyn UiWakeNotifier> = workbench.wake_hub();
        let run_binding = {
            let _guard = executor.enter();
            RunWorkbenchBinding::new(
                "fixture",
                Arc::clone(&runtime),
                controller,
                executor.handle().clone(),
                Arc::clone(&wake),
            )
        };
        run_binding.open_initial_session(DEFAULT_MAIN_SESSION_ID)?;
        run_binding.set_host_summary_sections(vec![(
            "CAD Studio 状态".to_owned(),
            vec![
                ("当前装配体".to_owned(), "Chassis-MK4".to_owned()),
                ("规范文件".to_owned(), "specs/chassis.cadspec".to_owned()),
                ("公差校验".to_owned(), "ISO 2768-mK".to_owned()),
            ],
        )]);
        workbench.attach_run_binding(Arc::clone(&run_binding));

        let wse = Arc::new(DeterministicWorkspaceEndpoint::new("ws-main"));
        let ws_binding = WorkspaceBinding::new(
            "ws-main",
            wse as Arc<dyn WorkspaceServiceEndpoint>,
            executor.handle().clone(),
            wake,
        );
        ws_binding.refresh_all_blocking()?;
        workbench.attach_workspace_binding(Arc::clone(&ws_binding));

        Ok(Self {
            executor,
            runtime,
            run_binding,
            workspace_binding: ws_binding,
            workbench,
            data_root,
        })
    }
}

pub fn build_domain_outliner() -> Result<LocalContainer> {
    let nodes = vec![
        TreeContentNode {
            row_id: ComponentId(2010),
            node: KeyedTreeNode {
                key: TreeNodeKey(1),
                parent: None,
                label: "Chassis-MK4 总装模型".to_owned(),
                disabled: false,
                initially_expanded: true,
            },
            display: TreeDisplayUpdate::DefaultLabel,
        },
        TreeContentNode {
            row_id: ComponentId(2011),
            node: KeyedTreeNode {
                key: TreeNodeKey(2),
                parent: Some(TreeNodeKey(1)),
                label: "主车架 · specs/chassis.cadspec".to_owned(),
                disabled: false,
                initially_expanded: false,
            },
            display: TreeDisplayUpdate::DefaultLabel,
        },
        TreeContentNode {
            row_id: ComponentId(2012),
            node: KeyedTreeNode {
                key: TreeNodeKey(3),
                parent: Some(TreeNodeKey(1)),
                label: "悬架摇臂总成 (Al-7075-T6)".to_owned(),
                disabled: false,
                initially_expanded: false,
            },
            display: TreeDisplayUpdate::DefaultLabel,
        },
        TreeContentNode {
            row_id: ComponentId(2013),
            node: KeyedTreeNode {
                key: TreeNodeKey(4),
                parent: Some(TreeNodeKey(1)),
                label: "热管导流罩 (CFRP)".to_owned(),
                disabled: false,
                initially_expanded: false,
            },
            display: TreeDisplayUpdate::DefaultLabel,
        },
    ];
    let tree = TreeContent::new("CAD 结构树", TreeDatasetKey(201), nodes)
        .map_err(|err| anyhow::anyhow!("CAD outliner tree invalid: {err:?}"))?;
    let scroll = Content::new(
        DOMAIN_OUTLINER_TREE_SCROLL_ID,
        ScrollContent::new(
            ScrollArea::default(),
            Content::new(DOMAIN_OUTLINER_TREE_ID, tree),
        ),
    );
    let root = Content::new(
        DOMAIN_OUTLINER_SHELL_ID,
        StackContent {
            model: Stack {
                axis: Axis::Vertical,
                gap: Spacing::Tight.value(),
            },
            flexible_child: Some(DOMAIN_OUTLINER_TREE_SCROLL_ID),
            children: vec![
                Content::new(
                    DOMAIN_OUTLINER_HEADER_ID,
                    TextContent::new("CAD 部件大纲 · Topology B"),
                ),
                scroll,
            ],
        },
    );
    Ok(LocalContainer::new("domain-outliner", root)?
        .title("领域模型大纲")
        .switcher_icon("tekes-files")
        .toolbar_host(ContainerToolbarHost::Window))
}

pub fn build_domain_canvas(workbench: SharedAgentWorkbench) -> Result<LocalContainer> {
    let header = Content::new(
        DOMAIN_CANVAS_HEADER_ID,
        TextContent::new("CAD 参数化主视口 · Chassis-MK4 (Domain Focus)"),
    );
    let metrics = Content::new(
        DOMAIN_CANVAS_METRICS_ID,
        TextContent::new(
            "壁厚: 4.20 mm | 安全系数: 2.85 | 关键约束: ±0.05 mm | 目标协作者: AgentRun",
        ),
    );
    let spec_rich = Content::new(
        DOMAIN_CANVAS_SPEC_RICH_ID,
        RichContent::new(
            "domain-cad-preview",
            1,
            RichDocument::from_markdown(
                "### 参数化装配约束预览\n\n- **主材**: `Al-7075-T6`（阳极氧化）\n- **关联规范**: `specs/chassis.cadspec`\n- **形态 B 说明**: 领域主视口占据 `Focus`，Agent 作为副栏协同，可随时零重建互换。",
            ),
        )
        .map_err(|err| anyhow::anyhow!("domain canvas RichContent invalid: {err:?}"))?,
    );

    let mut quote_btn = Button::new("将公差规范引用到 Agent");
    quote_btn.variant = ButtonVariant::Primary;
    let open_spec_btn = Button::new("在 Viewer 检查 .cadspec");
    let actions = Content::new(
        DOMAIN_CANVAS_ACTIONS_ROW_ID,
        StackContent {
            model: Stack {
                axis: Axis::Horizontal,
                gap: Spacing::Tight.value(),
            },
            flexible_child: None,
            children: vec![
                Content::new(DOMAIN_CANVAS_QUOTE_BTN_ID, quote_btn),
                Content::new(DOMAIN_CANVAS_OPEN_SPEC_BTN_ID, open_spec_btn),
            ],
        },
    );

    let root = Content::new(
        DOMAIN_CANVAS_SHELL_ID,
        StackContent {
            model: Stack {
                axis: Axis::Vertical,
                gap: Spacing::Tight.value(),
            },
            flexible_child: Some(DOMAIN_CANVAS_SPEC_RICH_ID),
            children: vec![header, metrics, actions, spec_rich],
        },
    );

    let wb_action = workbench;
    Ok(LocalContainer::new("domain-canvas", root)?
        .title("领域主画布")
        .switcher_icon("tekes-tab-summary")
        .toolbar_host(ContainerToolbarHost::Container {
            position: ContainerToolbarPosition::Top,
        })
        .toolbar(vec![ToolbarItem {
            id: DOMAIN_FOCUS_CMD,
            title: "Focus 切换".into(),
            enabled: true,
            geometry_dependent: false,
        }])
        .focus_switch(DOMAIN_FOCUS_CMD)
        .on_action(
            move |_host: &mut ContentHost, records: Vec<ContentActionRecord>| {
                for rec in records {
                    if !matches!(rec.action.kind, ContentActionKind::Activate) {
                        continue;
                    }
                    if rec.action.target == DOMAIN_CANVAS_QUOTE_BTN_ID {
                        let quote_text = "wall_thickness_mm = 4.20\ntolerance_mm = 0.05";
                        if let Ok(reference) = ArtifactSelectionReference::capture(
                            "cad-chassis-doc".to_owned(),
                            "specs/chassis.cadspec".to_owned(),
                            "cadspec".to_owned(),
                            vec!["specs".to_owned(), "chassis".to_owned()],
                            "r1".to_owned(),
                            sha256_prefixed(quote_text.as_bytes()),
                            quote_text,
                            Utf16SelectionRange {
                                location: 0,
                                length: quote_text.encode_utf16().count() as u64,
                            },
                            16,
                            16,
                        ) {
                            wb_action.enqueue_command(WorkbenchCommand::QuoteToComposer(
                                ComposerQuoteRequest {
                                    destination: None,
                                    reference,
                                    comment: "来自 Domain Canvas 的装配约束引用".to_owned(),
                                },
                            ));
                        }
                    } else if rec.action.target == DOMAIN_CANVAS_OPEN_SPEC_BTN_ID {
                        wb_action.set_artifact_target("default", "specs/chassis.cadspec");
                    }
                }
            },
        ))
}

pub fn build_topology_b_containers_and_options(
    rt: &DomainStudioRuntime,
    promote_agent: bool,
) -> Result<(Vec<Container>, PresentationOptions, Vec<ContainerFactory>)> {
    let wb = &rt.workbench;

    let outliner = build_domain_outliner()?;
    let canvas = build_domain_canvas(wb.clone())?;
    let agent_run = AgentRunContainer::build("agent-run", wb.clone())?;
    let summary = SummaryContainer::build_list_card("agent-summary", 260.0, wb.clone())?;
    let tasks = TasksQueueContainer::build_list_card("agent-tasks", 200.0, wb.clone())?;
    let files = WorkspaceFilesContainer::build("workspace-files", wb.clone())?;
    let changes = WorkspaceChangesContainer::build("workspace-changes", wb.clone())?;

    let containers: Vec<Container> = vec![
        outliner.into(),
        canvas.into(),
        agent_run.into(),
        summary.into(),
        tasks.into(),
        files.into(),
        changes.into(),
    ];

    let (focus_id, others_ids, selected_tab) = if promote_agent {
        (
            "agent-run",
            vec![
                "domain-canvas".to_owned(),
                "agent-summary".to_owned(),
                "agent-tasks".to_owned(),
                "workspace-files".to_owned(),
                "workspace-changes".to_owned(),
            ],
            OthersTab::Container("domain-canvas".to_owned()),
        )
    } else {
        (
            "domain-canvas",
            vec![
                "agent-run".to_owned(),
                "agent-summary".to_owned(),
                "agent-tasks".to_owned(),
                "workspace-files".to_owned(),
                "workspace-changes".to_owned(),
            ],
            OthersTab::Container("agent-run".to_owned()),
        )
    };

    let mut others = OthersOptions::new(others_ids);
    others.selected = Some(selected_tab);

    let presentation = PresentationOptions::new(focus_id)
        .nav_bar("domain-outliner")
        .navigation_toolbar("domain-outliner")
        .workspace_switch(true)
        .others(others);

    let cad_provider: Arc<dyn ArtifactViewerProvider> = Arc::new(CadSpecViewerProvider);
    let factories = vec![
        ArtifactViewerContainer::factory("artifact-viewer", wb.clone(), Some(cad_provider)),
        ChildRunContainer::factory("child-run", wb.clone()),
        SideChatContainer::factory("sidechat", wb.clone()),
    ];

    Ok((containers, presentation, factories))
}

pub fn build_topology_b_app(rt: &DomainStudioRuntime, promote_agent: bool) -> Result<AppWindow> {
    let (containers, presentation, factories) =
        build_topology_b_containers_and_options(rt, promote_agent)?;
    let _ = PresentationState::new(&containers, presentation.clone())?;

    let settings_file = rt.data_root.join("settings/studio.toml");
    let settings_owner = build_agent_settings_owner(settings_file)?;
    let nav_dir = rt.data_root.join("navigation");
    std::fs::create_dir_all(&nav_dir)?;

    let scope = rt
        .run_binding
        .current_scope()
        .unwrap_or_else(|| RunScope::main("fixture", DEFAULT_MAIN_SESSION_ID));

    let app = AppWindow::from_containers(containers, presentation)?
        .theme(Theme::LIGHT)
        .navigation(NavigationOptions {
            directory: nav_dir,
            history_id: "topology-b-studio".to_owned(),
            initial_entries: vec![navigation::Entry {
                id: "cad-chassis-mk4".to_owned(),
                route: "domain-cad".to_owned(),
                params: serde_json::json!({ "model": "chassis-mk4", "sessionId": scope.session_id }),
            }],
            view_command: None,
        })
        .shared_settings(settings_owner)
        .container_factories(factories)?;

    Ok(app)
}

pub fn render_app_to_rgba8(app: AppWindow, width: u32, height: u32) -> Result<Vec<u8>> {
    let (_menus, mut hosted) = app.into_window_handler()?;
    let mut gpu = Renderer::new_headless()?;
    hosted.render_headless_rgba8(&mut gpu, (width, height), 1)
}
