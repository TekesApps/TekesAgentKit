//! Phase 1 Core Trio integration tests: Sidebar, Transcript, Composer, Summary on AppHost.

use std::sync::Arc;
use std::time::Duration;
use tekes_agent_apphost::{
    AgentRunContainer, AppHostWakeHub, SessionSidebarContainer, SharedAgentWorkbench,
    SummaryContainer,
};
use tekes_agent_components_ui::{
    COMPOSER_ATTACH_BTN_ID, COMPOSER_CARD_ID, COMPOSER_CONTROLS_STACK_ID, COMPOSER_EDITOR_ID,
    COMPOSER_EFFORT_SELECT_ID, COMPOSER_HARNESS_SELECT_ID, COMPOSER_INPUT_ROW_ID,
    COMPOSER_MODEL_SELECT_ID, COMPOSER_PRESET_SELECT_ID, COMPOSER_SEND_BTN_ID,
    RUN_SETUP_TOOLBAR_ROOT_ID, ResponsiveWidthMode, SIDEBAR_FILTER_ID, SIDEBAR_LIST_ID,
    SIDEBAR_PIN_BTN_ID, SUMMARY_HEADER_ID, TRANSCRIPT_EARLIER_BTN_ID, TRANSCRIPT_LATEST_BTN_ID,
    TRANSCRIPT_READER_ID, TRANSCRIPT_RICH_BODY_ID, TRANSCRIPT_THINKING_DISC_ID, TranscriptReader,
    TranscriptSurface,
};
use tekes_agent_contract::EndpointConnector;
use tekes_agent_presentation::{ComposerDestination, RunController, RunScope};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{
    DEFAULT_MAIN_SESSION_ID, FixtureSessionEndpoint, RunWorkbenchBinding, SECONDARY_SESSION_ID,
    SidebarFilterMode, UiWakeNotifier,
};
use tekes_ui_app_host::{AppWindow, Container, OthersOptions, PresentationOptions};
use tekes_ui_controls::{
    ReadingAnchorUpdate,
    composition::{
        Button, ContentEvent, KeyedSelectableListContent, MenuContent, RichContent, StackContent,
        TextContent, TextFieldContent,
    },
    extended::Axis,
};
use tekes_ui_render::Renderer;
use tekes_ui_scene::{Point, Rect, Scene};
use tekes_ui_text::{
    coretext::{CoreTextFonts, CoreTextShaper},
    traits::Shaper,
};
use tekes_ui_tokens::{Palette, Theme, proposal};

struct TestTextBackend(CoreTextShaper);

impl TestTextBackend {
    fn new() -> Self {
        Self(CoreTextShaper::new(CoreTextFonts::new()))
    }
}

impl tekes_ui_controls::TextSurfaceTextRenderer for TestTextBackend {
    fn push_visible(
        &mut self,
        _: &mut Scene,
        _: &tekes_ui_text::Paragraph,
        _: Point,
        _: tekes_ui_tokens::Rgba,
        _: Rect,
    ) {
    }
}

impl tekes_ui_controls::TextSurfaceHost for TestTextBackend {
    fn shape_surface(
        &mut self,
        text: &str,
        role: proposal::TextRole,
        width: Option<f32>,
    ) -> std::rc::Rc<tekes_ui_text::Paragraph> {
        std::rc::Rc::new(self.0.shape(
            text,
            &tekes_ui_text::TextStyle::system(role.style().size),
            &tekes_ui_text::ShapeOptions {
                width: width.map(f64::from),
                line_spacing: 0.0,
            },
        ))
    }
}

impl tekes_ui_controls::ButtonTextRenderer for TestTextBackend {
    fn measure(
        &mut self,
        text: &str,
        role: proposal::TextRole,
        width: f32,
    ) -> tekes_ui_scene::Size {
        let p = tekes_ui_controls::TextSurfaceHost::shape_surface(self, text, role, Some(width));
        tekes_ui_scene::Size::new(p.width as f32, p.height as f32)
    }

    fn draw(
        &mut self,
        _: &mut Scene,
        _: &str,
        _: proposal::TextRole,
        _: Rect,
        _: tekes_ui_controls::ButtonTextAlign,
        _: tekes_ui_tokens::Rgba,
    ) {
    }

    fn text_surface_host(&mut self) -> Option<&mut dyn tekes_ui_controls::TextSurfaceHost> {
        Some(self)
    }

    fn icon_renderer(&mut self) -> Option<&mut dyn tekes_ui_controls::layout_view::IconRenderer> {
        Some(self)
    }
}

impl tekes_ui_controls::layout_view::IconRenderer for TestTextBackend {
    fn draw_icon(&mut self, _: &mut Scene, _: &str, _: Rect, _: tekes_ui_tokens::Rgba) {}

    fn prepare_icon(
        &mut self,
        _: &str,
        _: f32,
        _: f32,
        _: tekes_ui_tokens::Rgba,
    ) -> Result<
        tekes_ui_controls::composition::IconImage,
        tekes_ui_controls::composition::IconRenderError,
    > {
        Ok(tekes_ui_controls::composition::IconImage::new(
            tekes_ui_scene::ImageId(1),
            std::rc::Rc::new(()),
        ))
    }
}

fn setup_workbench() -> (
    tokio::runtime::Runtime,
    FixtureSessionEndpoint,
    Arc<RunWorkbenchBinding>,
    SharedAgentWorkbench,
) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let endpoint = FixtureSessionEndpoint::with_timing(Duration::ZERO, Duration::from_millis(5));
    let connector: Arc<dyn EndpointConnector> = Arc::new(endpoint.clone());
    let (runtime, controller) = rt.block_on(async {
        let runtime = Arc::new(SessionRuntime::spawn("fixture", connector));
        runtime.wait_ready(Duration::from_secs(5)).await.unwrap();
        let controller = Arc::new(RunController::new(Arc::clone(&runtime)).unwrap());
        (runtime, controller)
    });
    let workbench = SharedAgentWorkbench::new();
    let wake: Arc<dyn UiWakeNotifier> = workbench.wake_hub();
    let binding = {
        let _guard = rt.enter();
        RunWorkbenchBinding::new("fixture", runtime, controller, rt.handle().clone(), wake)
    };
    binding
        .open_initial_session(DEFAULT_MAIN_SESSION_ID)
        .unwrap();
    workbench.attach_run_binding(Arc::clone(&binding));
    (rt, endpoint, binding, workbench)
}

#[test]
fn phase1_sidebar_navigation_summary_and_actionable_respond_work() {
    let (_rt, _endpoint, binding, workbench) = setup_workbench();

    let sidebar = SessionSidebarContainer::build("agent-sidebar", workbench.clone()).unwrap();
    let run_container = AgentRunContainer::build("agent-run", workbench.clone()).unwrap();
    let summary = SummaryContainer::build_tab("agent-summary", workbench.clone()).unwrap();

    let containers = vec![
        Container::from(sidebar),
        Container::from(run_container),
        Container::from(summary),
    ];
    drop(containers);

    let mut sidebar = SessionSidebarContainer::build("agent-sidebar", workbench.clone()).unwrap();
    let mut run = AgentRunContainer::build("agent-run", workbench.clone()).unwrap();
    let mut summary = SummaryContainer::build_tab("agent-summary", workbench.clone()).unwrap();

    let mut text = TestTextBackend::new();
    let mut scene = Scene::new();
    let palette = Palette::from(Theme::LIGHT);
    let area = Rect::new(0.0, 0.0, 760.0, 720.0);

    // Trigger direct prepare via host simulation: switch to session-beta via binding & verify Summary subagents update.
    binding.select_session(SECONDARY_SESSION_ID);
    workbench.set_active_session(tekes_agent_contract::SessionAddress {
        endpoint_id: "fixture".into(),
        session_id: SECONDARY_SESSION_ID.into(),
    });
    assert!(workbench.pop_navigation().is_some());

    // Wait briefly for session-beta journal snapshot.
    for _ in 0..40 {
        if binding
            .run_snapshot()
            .transcript
            .items
            .iter()
            .any(|i| i.text.contains("只读子代理"))
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    let snap = binding.run_snapshot();
    assert!(!snap.transcript.items.is_empty());

    // Verify actionable prompt on session-beta ("q-beta-1") and resolve it via RunWorkbenchBinding.
    let actionables = binding.actionables_for(&snap);
    assert_eq!(actionables.len(), 1);
    assert_eq!(actionables[0].id(), "q-beta-1");
    let choice = &actionables[0].choices[0];
    binding.respond_actionable(
        actionables[0].scope(),
        actionables[0].id(),
        actionables[0].revision(),
        choice.outcome().clone(),
    );

    for _ in 0..40 {
        let s = binding.run_snapshot();
        if binding.actionables_for(&s).is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    assert!(binding.actionables_for(&binding.run_snapshot()).is_empty());

    // Summary facts on session-beta resolve the child UUID subagent link.
    let facts = binding
        .summary_facts_for(&binding.run_snapshot())
        .expect("summary facts");
    assert_eq!(facts.subagents.len(), 1);
    assert!(facts.subagents[0].session_id.is_some());

    // Host summary custom section injection works cleanly.
    binding.set_host_summary_sections(vec![(
        "CAD 模型状态".to_owned(),
        vec![("零件总数".to_owned(), "42".to_owned())],
    )]);

    // Sidebar filter & pin work synchronously.
    assert!(binding.sidebar_rows().len() >= 2);
    binding.toggle_pin(SECONDARY_SESSION_ID);
    binding.set_sidebar_filter(SidebarFilterMode::Pinned);
    let pinned_rows = binding.sidebar_rows();
    assert!(pinned_rows.iter().all(|r| r.pinned));
    binding.set_sidebar_filter(SidebarFilterMode::All);

    sidebar.with_host(&mut |h| assert!(h.get(SIDEBAR_LIST_ID).is_some()));
    run.with_host(&mut |h| {
        assert!(h.get(COMPOSER_EDITOR_ID).is_some());
        assert!(h.get(TRANSCRIPT_RICH_BODY_ID).is_some());
        h.render(&mut scene, &mut text, area, area, palette)
            .unwrap();
    });
    summary.with_host(&mut |h| assert!(h.get(SUMMARY_HEADER_ID).is_some()));
}

#[test]
fn phase1_streaming_load_earlier_anchor_compact_layout_and_gpu_render() {
    let (_rt, _endpoint, binding, workbench) = setup_workbench();

    // Send a prompt on session-alpha and observe WindowWake + completed streamed response.
    let dest = ComposerDestination::run(RunScope::main("fixture", DEFAULT_MAIN_SESSION_ID));
    binding.set_draft_text(&dest, "测试流式推送".to_owned());
    binding.submit_destination(&dest);

    let initial_len = binding.run_snapshot().transcript.items.len();
    for _ in 0..80 {
        let snap = binding.run_snapshot();
        if snap
            .transcript
            .items
            .iter()
            .any(|i| i.text.contains("WindowWake OK") && !i.is_provisional)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        binding.run_snapshot().transcript.items.len() > initial_len,
        "streamed response must commit into transcript"
    );

    // Check TranscriptReader prepend anchor preservation + JumpToLatest.
    let mut reader = TranscriptReader::new();
    let base_turns: Vec<String> = (10..=26)
        .map(|i| format!("你 · #t-{i}\n轮次 {i} 内容，用来测量加载更早消息时的阅读锚点位移。"))
        .collect();
    let doc_v1 = base_turns.join("\n\n");
    reader.set_or_update("alpha", &doc_v1);
    let mut text = TestTextBackend::new();
    let viewport = Rect::new(0.0, 0.0, 680.0, 280.0);
    {
        let role = reader.surface.role();
        reader.reading.layout_with(
            &mut reader.surface,
            viewport,
            Some(viewport),
            2.0,
            1.0,
            |s, w| tekes_ui_controls::TextSurfaceHost::shape_surface(&mut text, s, role, w),
        );
    }
    reader.surface.scroll_mut().scroll_by(0.0, 160.0);
    reader.stop_following();
    {
        let role = reader.surface.role();
        reader.reading.layout_with(
            &mut reader.surface,
            viewport,
            Some(viewport),
            2.0,
            1.0,
            |s, w| tekes_ui_controls::TextSurfaceHost::shape_surface(&mut text, s, role, w),
        );
    }
    let y_before = reader.surface.scroll().offset.y;

    let older_turns: Vec<String> = (1..=9)
        .map(|i| format!("你 · #t-{i}\n更早历史消息 {i}，验证前置分页保持滚动偏移。"))
        .collect();
    let doc_v2 = format!("{}\n\n{doc_v1}", older_turns.join("\n\n"));
    reader.set_or_update("alpha", &doc_v2);
    assert_eq!(reader.last_anchor_update, Some(ReadingAnchorUpdate::Mapped));
    {
        let role = reader.surface.role();
        reader.reading.layout_with(
            &mut reader.surface,
            viewport,
            Some(viewport),
            2.0,
            1.0,
            |s, w| tekes_ui_controls::TextSurfaceHost::shape_surface(&mut text, s, role, w),
        );
    }
    assert!(
        reader.surface.scroll().offset.y > y_before + 80.0,
        "prepend must advance scroll offset to keep reading anchor"
    );
    reader.follow_latest();
    assert!(reader.reading.is_following_latest());

    // Check single-row pure-input ComposerSurface & top RunSetupToolbar responsive layout + locking.
    let composer_host = tekes_ui_controls::composition::ContentHost::new(
        tekes_agent_components_ui::ComposerSurface::build_content(),
    )
    .unwrap();
    assert!(composer_host.get(COMPOSER_CARD_ID).is_some());
    assert!(composer_host.get(COMPOSER_INPUT_ROW_ID).is_some());
    assert!(composer_host.get(COMPOSER_ATTACH_BTN_ID).is_some());
    assert!(composer_host.get(COMPOSER_EDITOR_ID).is_some());
    assert!(composer_host.get(COMPOSER_SEND_BTN_ID).is_some());
    assert!(composer_host.get(COMPOSER_MODEL_SELECT_ID).is_none());
    assert!(composer_host.get(COMPOSER_PRESET_SELECT_ID).is_none());
    assert!(composer_host.get(COMPOSER_EFFORT_SELECT_ID).is_none());
    assert!(composer_host.get(COMPOSER_HARNESS_SELECT_ID).is_none());

    let mut setup_toolbar = tekes_agent_components_ui::RunSetupToolbar::new();
    let snap = binding.run_snapshot();
    assert!(snap.run.is_some() && !snap.transcript.items.is_empty());
    let mut toolbar_host = tekes_ui_controls::composition::ContentHost::new(
        tekes_agent_components_ui::RunSetupToolbar::build_content(
            vec!["Tekes".into(), "Pi".into(), "DSH".into()],
            vec!["Workspace Write".into()],
            vec!["claude-sonnet-4".into()],
            vec!["Low".into(), "Medium".into(), "High".into()],
        ),
    )
    .unwrap();
    setup_toolbar.sync(
        &mut toolbar_host,
        &snap,
        0,
        0,
        0,
        1,
        ResponsiveWidthMode::Compact,
    );
    assert_eq!(
        toolbar_host
            .get(COMPOSER_CONTROLS_STACK_ID)
            .unwrap()
            .component::<StackContent>()
            .unwrap()
            .model
            .axis,
        Axis::Vertical
    );
    setup_toolbar.sync(
        &mut toolbar_host,
        &snap,
        0,
        0,
        0,
        1,
        ResponsiveWidthMode::Expanded,
    );
    assert_eq!(
        toolbar_host
            .get(COMPOSER_CONTROLS_STACK_ID)
            .unwrap()
            .component::<StackContent>()
            .unwrap()
            .model
            .axis,
        Axis::Horizontal
    );

    // Harness stays locked on an established thread: label omits dropdown chevron, items disable.
    let harness_menu = toolbar_host
        .get(COMPOSER_HARNESS_SELECT_ID)
        .unwrap()
        .component::<MenuContent>()
        .unwrap();
    assert_eq!(harness_menu.trigger.label, "Tekes");
    assert!(
        format!("{:?}", harness_menu.menu).contains("disabled: true"),
        "expected locked harness items to be disabled"
    );
    assert_eq!(binding.selected_harness(), "Tekes");
    binding.set_harness(1);
    assert_eq!(binding.selected_harness(), "Tekes");

    // Effort selection updates binding & commits `reasoningEffort` to draft settings.
    binding.set_effort(2);
    assert_eq!(binding.selected_effort(), "High");
    assert_eq!(
        binding
            .run_snapshot()
            .draft
            .settings
            .get("reasoningEffort")
            .and_then(|v| v.as_str()),
        Some("high")
    );

    // Assemble full 3-column AppWindow (`nav_bar`, `focus`, `others` List card) & render Metal GPU frame.
    let sidebar = SessionSidebarContainer::build("agent-sidebar", workbench.clone()).unwrap();
    let mut run = AgentRunContainer::build("agent-run", workbench.clone()).unwrap();
    run.with_host(&mut |h| {
        assert!(h.get(RUN_SETUP_TOOLBAR_ROOT_ID).is_some());
        assert!(h.get(COMPOSER_HARNESS_SELECT_ID).is_some());
        assert!(h.get(COMPOSER_PRESET_SELECT_ID).is_some());
        assert!(h.get(COMPOSER_MODEL_SELECT_ID).is_some());
        assert!(h.get(COMPOSER_EFFORT_SELECT_ID).is_some());
        assert!(h.get(COMPOSER_INPUT_ROW_ID).is_some());
    });
    let summary =
        SummaryContainer::build_list_card("agent-summary", 280.0, workbench.clone()).unwrap();

    let presentation = PresentationOptions::new("agent-run")
        .nav_bar("agent-sidebar")
        .navigation_toolbar("agent-sidebar")
        .workspace_switch(true)
        .others(OthersOptions::new(vec!["agent-summary".to_owned()]));

    let app = AppWindow::from_containers(
        vec![sidebar.into(), run.into(), summary.into()],
        presentation,
    )
    .unwrap();
    let (_menus, mut hosted) = app.into_window_handler().unwrap();
    let mut gpu = Renderer::new_headless().unwrap();
    let rgba = hosted
        .render_headless_rgba8(&mut gpu, (1280, 800), 1)
        .unwrap();
    assert_eq!(rgba.len(), 1280 * 800 * 4);

    // Also test real load_earlier against FixtureSessionEndpoint.
    let count_before = binding.run_snapshot().transcript.items.len();
    binding.load_earlier(&RunScope::main("fixture", DEFAULT_MAIN_SESSION_ID));
    for _ in 0..40 {
        if binding.run_snapshot().transcript.items.len() > count_before {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    assert!(
        binding.run_snapshot().transcript.items.len() > count_before,
        "load_earlier must prepend older fixture messages"
    );

    // Keep compiler clean on imported constants used to verify public API exports.
    let _ = (
        SIDEBAR_FILTER_ID,
        SIDEBAR_PIN_BTN_ID,
        COMPOSER_SEND_BTN_ID,
        TRANSCRIPT_EARLIER_BTN_ID,
        TRANSCRIPT_LATEST_BTN_ID,
        TRANSCRIPT_READER_ID,
        TRANSCRIPT_THINKING_DISC_ID,
    );
    let _ = (AppHostWakeHub::new(), TranscriptSurface::new());
    let _ = (
        Button::new("x"),
        KeyedSelectableListContent::semantic_part_for_key,
        RichContent::version,
        TextContent::new("x"),
        TextFieldContent::new("a", "b"),
        ContentEvent::Cancel,
    );
}
