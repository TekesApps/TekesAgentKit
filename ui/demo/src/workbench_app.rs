//! Topology A (Agent-First Workbench) desktop `AppWindow` assembly, supporting:
//! - Deterministic SEP + WSE fixture mode (`--offscreen`, `--theme light|dark`, `--swap-focus`)
//! - Live `KernelConnector` + confined `ReadOnlyFsWorkspaceEndpoint` (`--endpoint-config <json>`)
//! - macOS native AppKit/IME selftest (`--native-selftest` -> prints `SELFTEST_OK`)

use anyhow::{Context, Result, bail};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tekes_agent_apphost::{
    AgentRunContainer, ArtifactViewerContainer, ChildRunContainer, SessionSidebarContainer,
    SharedAgentWorkbench, SideChatContainer, SummaryContainer, WorkspaceChangesContainer,
    WorkspaceFilesContainer, build_agent_settings_owner,
};
use tekes_agent_contract::{EndpointConnector, WorkspaceServiceEndpoint};
use tekes_agent_endpoint_kernel::KernelConnector;
use tekes_agent_presentation::{RunController, RunScope};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{
    DEFAULT_MAIN_SESSION_ID, DeterministicWorkspaceEndpoint, FixtureSessionEndpoint,
    HarnessSettingsBinding, ReadOnlyFsWorkspaceEndpoint, RunWorkbenchBinding, UiWakeNotifier,
    WorkspaceBinding,
};
use tekes_ui_app_host::{
    AppWindow, Container, ContainerToolbarHost, NavigationOptions, OthersOptions, OthersTab,
    PresentationOptions, PresentationState, navigation,
};
use tekes_ui_render::Renderer;
use tekes_ui_tokens::Theme;

#[derive(Clone, Debug)]
pub struct EndpointJsonConfig {
    pub endpoint_id: String,
    pub url: String,
    pub bearer: String,
    pub sessions: Vec<String>,
    pub cwd: PathBuf,
}

impl EndpointJsonConfig {
    pub fn from_file(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed reading endpoint config {}", path.display()))?;
        let val: serde_json::Value = serde_json::from_str(&raw)?;
        let endpoint_id = val["endpointId"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or("native-ui")
            .to_owned();
        let url = val["url"]
            .as_str()
            .filter(|s| !s.is_empty())
            .context("endpoint-config missing non-empty `url`")?
            .to_owned();
        let bearer = if let Some(direct) = val["bearer"].as_str().filter(|s| !s.is_empty()) {
            direct.to_owned()
        } else if let Some(env_key) = val["bearerEnv"].as_str().filter(|s| !s.is_empty()) {
            std::env::var(env_key).with_context(|| format!("missing bearer env var `{env_key}`"))?
        } else {
            bail!("endpoint-config requires either `bearer` or `bearerEnv`");
        };
        let sessions = val["sessions"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let cwd = val["cwd"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        Ok(Self {
            endpoint_id,
            url,
            bearer,
            sessions,
            cwd,
        })
    }
}

#[derive(Clone, Debug)]
pub struct TopologyALaunchOptions {
    pub theme: Theme,
    pub swap_focus: bool,
    pub showcase: bool,
    pub component: Option<String>,
    pub offscreen: Option<PathBuf>,
    pub endpoint_config: Option<PathBuf>,
    pub native_selftest: bool,
    pub data_dir: Option<PathBuf>,
}

impl Default for TopologyALaunchOptions {
    fn default() -> Self {
        Self {
            theme: Theme::LIGHT,
            swap_focus: false,
            showcase: false,
            component: None,
            offscreen: None,
            endpoint_config: None,
            native_selftest: false,
            data_dir: None,
        }
    }
}

impl TopologyALaunchOptions {
    pub fn parse_args(args: &[String]) -> Result<Self> {
        let mut opts = Self::default();
        let mut idx = 1;
        while idx < args.len() {
            match args[idx].as_str() {
                "--offscreen" => {
                    idx += 1;
                    let path = args.get(idx).context("--offscreen expects a PNG path")?;
                    opts.offscreen = Some(PathBuf::from(path));
                }
                "--theme" => {
                    idx += 1;
                    let theme_str = args.get(idx).context("--theme expects light|dark")?;
                    opts.theme = match theme_str.as_str() {
                        "light" => Theme::LIGHT,
                        "dark" => Theme::DARK,
                        other => bail!("unsupported --theme value: {other}"),
                    };
                }
                "--swap-focus" => {
                    opts.swap_focus = true;
                }
                "--showcase" => {
                    opts.showcase = true;
                }
                "--component" => {
                    idx += 1;
                    let name = args
                        .get(idx)
                        .context("--component expects a component name")?;
                    crate::showcase::ShowcaseComponent::from_name(name)?;
                    opts.component = Some(name.clone());
                }
                "--endpoint-config" => {
                    idx += 1;
                    let path = args
                        .get(idx)
                        .context("--endpoint-config expects a JSON path")?;
                    opts.endpoint_config = Some(PathBuf::from(path));
                }
                "--native-selftest" => {
                    opts.native_selftest = true;
                }
                psn if psn.starts_with("-psn_") => {}
                unknown => {
                    bail!("unknown demo flag `{unknown}`");
                }
            }
            idx += 1;
        }
        Ok(opts)
    }
}

pub struct TopologyARuntime {
    pub executor: Arc<tokio::runtime::Runtime>,
    pub runtime: Arc<SessionRuntime>,
    pub run_binding: Arc<RunWorkbenchBinding>,
    pub workspace_binding: Arc<WorkspaceBinding>,
    pub workbench: SharedAgentWorkbench,
    pub data_root: PathBuf,
}

fn resolve_data_root(custom: Option<&PathBuf>) -> Result<PathBuf> {
    if let Some(dir) = custom {
        return Ok(dir.clone());
    }
    if let Some(dir) = std::env::var_os("TEKESUI_APPHOST_DATA_DIR") {
        let dir = PathBuf::from(dir);
        anyhow::ensure!(
            dir.is_absolute(),
            "TEKESUI_APPHOST_DATA_DIR must be an absolute path"
        );
        return Ok(dir);
    }
    Ok(std::env::temp_dir().join(format!("tekes-agent-ui-demo-{}", std::process::id())))
}

pub fn initialize_topology_a_runtime(options: &TopologyALaunchOptions) -> Result<TopologyARuntime> {
    let executor = Arc::new(tokio::runtime::Runtime::new()?);
    let data_root = resolve_data_root(options.data_dir.as_ref())?;
    std::fs::create_dir_all(&data_root)?;

    let workbench = SharedAgentWorkbench::new();
    let wake: Arc<dyn UiWakeNotifier> = workbench.wake_hub();

    struct EndpointInputs {
        endpoint_id: String,
        connector: Arc<dyn EndpointConnector>,
        wse_endpoint: Arc<dyn WorkspaceServiceEndpoint>,
        initial_session_id: Option<String>,
        ws_id: String,
    }

    let EndpointInputs {
        endpoint_id,
        connector,
        wse_endpoint,
        initial_session_id,
        ws_id,
    } = if let Some(cfg_path) = &options.endpoint_config {
        let cfg = EndpointJsonConfig::from_file(cfg_path)?;
        let kernel = KernelConnector::new(&cfg.endpoint_id, &cfg.url, cfg.bearer.clone())?;
        let fs_wse = ReadOnlyFsWorkspaceEndpoint::open("ws-kernel-fs", &cfg.cwd)?;
        EndpointInputs {
            endpoint_id: cfg.endpoint_id,
            connector: Arc::new(kernel),
            wse_endpoint: Arc::new(fs_wse),
            initial_session_id: cfg.sessions.first().cloned(),
            ws_id: "ws-kernel-fs".to_owned(),
        }
    } else {
        let sep = FixtureSessionEndpoint::with_timing(Duration::ZERO, Duration::from_millis(8));
        let d_wse = Arc::new(DeterministicWorkspaceEndpoint::new("ws-main"));
        EndpointInputs {
            endpoint_id: "fixture".to_owned(),
            connector: Arc::new(sep),
            wse_endpoint: d_wse as Arc<dyn WorkspaceServiceEndpoint>,
            initial_session_id: Some(DEFAULT_MAIN_SESSION_ID.to_owned()),
            ws_id: "ws-main".to_owned(),
        }
    };

    let (runtime, controller) = executor.block_on(async {
        let runtime = Arc::new(SessionRuntime::spawn(&endpoint_id, connector));
        runtime.wait_ready(Duration::from_secs(10)).await?;
        let controller = Arc::new(RunController::new(Arc::clone(&runtime))?);
        Ok::<_, anyhow::Error>((runtime, controller))
    })?;

    let run_binding = {
        let _guard = executor.enter();
        RunWorkbenchBinding::new(
            &endpoint_id,
            Arc::clone(&runtime),
            controller,
            executor.handle().clone(),
            Arc::clone(&wake),
        )
    };

    if let Some(sid) = initial_session_id {
        run_binding.open_initial_session(&sid)?;
    } else {
        let store = runtime.snapshot();
        if let Some(first_sid) = store.sessions.keys().next().cloned() {
            run_binding.open_initial_session(&first_sid)?;
        }
    }
    workbench.attach_run_binding(Arc::clone(&run_binding));

    let ws_binding = WorkspaceBinding::new(
        ws_id,
        wse_endpoint,
        executor.handle().clone(),
        Arc::clone(&wake),
    );
    let _ = ws_binding.refresh_all_blocking();
    workbench.attach_workspace_binding(Arc::clone(&ws_binding));

    let hs_binding = {
        let _guard = executor.enter();
        HarnessSettingsBinding::new(Arc::clone(&runtime), executor.handle().clone(), wake)
    };
    let _ = hs_binding.refresh_blocking();
    workbench.attach_harness_settings_binding(hs_binding);

    if options.endpoint_config.is_some() {
        executor.block_on(async {
            tokio::time::sleep(Duration::from_millis(80)).await;
        });
    }

    Ok(TopologyARuntime {
        executor,
        runtime,
        run_binding,
        workspace_binding: ws_binding,
        workbench,
        data_root,
    })
}

pub fn build_topology_a_containers_and_options(
    rt: &TopologyARuntime,
    swap_focus: bool,
) -> Result<(
    Vec<Container>,
    PresentationOptions,
    Vec<tekes_ui_app_host::ContainerFactory>,
)> {
    let wb = &rt.workbench;

    let sidebar = SessionSidebarContainer::build("agent-sidebar", wb.clone())?
        .toolbar_host(ContainerToolbarHost::Window);
    let run_container = AgentRunContainer::build("agent-run", wb.clone())?;
    let summary_tab = SummaryContainer::build_tab("agent-summary", wb.clone())?;
    let files_tab = WorkspaceFilesContainer::build("workspace-files", wb.clone())?;
    let changes_tab = WorkspaceChangesContainer::build("workspace-changes", wb.clone())?;

    let containers: Vec<Container> = vec![
        sidebar.into(),
        run_container.into(),
        summary_tab.into(),
        files_tab.into(),
        changes_tab.into(),
    ];

    let (focus_id, others_ids, selected_tab) = if swap_focus {
        (
            "workspace-files",
            vec![
                "agent-run".to_owned(),
                "agent-summary".to_owned(),
                "workspace-changes".to_owned(),
            ],
            OthersTab::Container("agent-run".to_owned()),
        )
    } else {
        (
            "agent-run",
            vec![
                "agent-summary".to_owned(),
                "workspace-files".to_owned(),
                "workspace-changes".to_owned(),
            ],
            OthersTab::Container("agent-summary".to_owned()),
        )
    };

    let mut others_opts = OthersOptions::new(others_ids);
    others_opts.selected = Some(selected_tab);

    let mut presentation = PresentationOptions::new(focus_id)
        .nav_bar("agent-sidebar")
        .navigation_toolbar("agent-sidebar")
        .workspace_switch(true)
        .others(others_opts);
    presentation.nav_width = 236.0;
    presentation.focus_fraction = 0.62;

    let factories = vec![
        ArtifactViewerContainer::factory("artifact-viewer", wb.clone(), None),
        ChildRunContainer::factory("child-run", wb.clone()),
        SideChatContainer::factory("sidechat", wb.clone()),
    ];

    Ok((containers, presentation, factories))
}

pub fn build_topology_a_app(
    rt: &TopologyARuntime,
    options: &TopologyALaunchOptions,
) -> Result<AppWindow> {
    let (containers, presentation, factories) =
        build_topology_a_containers_and_options(rt, options.swap_focus)?;
    let _ = PresentationState::new(&containers, presentation.clone())?;

    let settings_file = rt.data_root.join("settings/agentkit.toml");
    let settings_owner = build_agent_settings_owner(settings_file)?;

    let nav_dir = rt.data_root.join("navigation");
    std::fs::create_dir_all(&nav_dir)?;

    let initial_session = rt
        .run_binding
        .current_scope()
        .unwrap_or_else(|| RunScope::main(rt.run_binding.endpoint_id(), DEFAULT_MAIN_SESSION_ID));

    let app = AppWindow::from_containers(containers, presentation)?
        .theme(options.theme)
        .navigation(NavigationOptions {
            directory: nav_dir,
            history_id: "topology-a-workbench".to_owned(),
            initial_entries: vec![navigation::Entry {
                id: format!(
                    "visit-0:{}:{}",
                    initial_session.endpoint_id, initial_session.session_id
                ),
                route: "agent-session".to_owned(),
                params: json!({
                    "endpointId": initial_session.endpoint_id,
                    "sessionId": initial_session.session_id,
                }),
            }],
            view_command: None,
        })
        .shared_settings(settings_owner)
        .container_factories(factories)?;

    Ok(app)
}

pub fn render_topology_a_offscreen_png(
    app: AppWindow,
    path: &Path,
    width: u32,
    height: u32,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let (_menus, mut hosted) = app.into_window_handler()?;
    let mut gpu = Renderer::new_headless()?;
    let scale: u32 = 2;
    let rgba = hosted.render_headless_rgba8(&mut gpu, (width, height), scale)?;
    let file = std::fs::File::create(path)?;
    let mut encoder = png::Encoder::new(file, width * scale, height * scale);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&rgba)?;
    Ok(())
}
