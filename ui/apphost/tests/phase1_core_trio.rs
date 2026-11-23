//! Phase 1 Core Trio integration tests: Sidebar, Transcript, Composer, Summary on AppHost.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;
use tekes_agent_apphost::{
    AgentRunContainer, AppHostWakeHub, RUN_TOOLBAR_APPROVAL_CMD, RUN_TOOLBAR_EFFORT_CMD,
    RUN_TOOLBAR_HARNESS_CMD, RUN_TOOLBAR_MODEL_CMD, RUN_TOOLBAR_SETUP_GROUP_ID,
    SessionSidebarContainer, SharedAgentWorkbench, SummaryContainer,
};
use tekes_agent_components_ui::{
    COMPOSER_ATTACH_BTN_ID, COMPOSER_CARD_ID, COMPOSER_CONTROLS_STACK_ID, COMPOSER_EDITOR_ID,
    COMPOSER_EFFORT_SELECT_ID, COMPOSER_HARNESS_SELECT_ID, COMPOSER_INPUT_ROW_ID,
    COMPOSER_MODEL_SELECT_ID, COMPOSER_PRESET_SELECT_ID, COMPOSER_SEND_BTN_ID,
    RUN_SETUP_TOOLBAR_ROOT_ID, ResponsiveWidthMode, SIDEBAR_FILTER_ID, SIDEBAR_LIST_ID,
    SIDEBAR_PIN_BTN_ID, SUMMARY_HEADER_ID, TRANSCRIPT_EARLIER_BTN_ID, TRANSCRIPT_LATEST_BTN_ID,
    TRANSCRIPT_READER_ID, TRANSCRIPT_RICH_BODY_ID, TRANSCRIPT_ROOT_ID, TRANSCRIPT_SCROLL_ID,
    TRANSCRIPT_THINKING_DISC_ID, TRANSCRIPT_TURNS_STACK_ID, TranscriptReader, TranscriptSurface,
};
use tekes_agent_contract::{EndpointConnector, SessionAddress};
use tekes_agent_presentation::{ComposerDestination, RunController, RunScope};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{
    DEFAULT_MAIN_SESSION_ID, FixtureSessionEndpoint, RunWorkbenchBinding, SECONDARY_SESSION_ID,
    SidebarFilterMode, UiWakeNotifier,
};
use tekes_ui_app_host::{
    AppWindow, Container, LocalToolbarAction, LocalToolbarGroupOverflow, LocalToolbarItem,
    LocalToolbarTone, OthersOptions, PresentationOptions, navigation,
};
use tekes_ui_controls::{
    ReadingAnchorUpdate,
    composition::{
        AccessibilityAction, Button, Component, ComponentId, Content, ContentAction,
        ContentActionKind, ContentActionPayload, ContentActionRecord, ContentActionSource,
        ContentEvent, ContentHost, ContentReadingAnchorUpdate, ContentRole,
        KeyedSelectableListContent, MenuContent, MenuEntry, RichContent, ScrollContent,
        StackContent, TextContent, TextFieldContent,
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
        assert!(h.get(TRANSCRIPT_SCROLL_ID).is_some());
        let turn_count = h
            .get(TRANSCRIPT_TURNS_STACK_ID)
            .and_then(|c| c.component::<StackContent>())
            .map_or(0, |s| s.children().len());
        assert!(
            turn_count >= 1,
            "expected structured turns mounted in transcript scroll"
        );
        h.render(&mut scene, &mut text, area, area, palette)
            .unwrap();
    });
    summary.with_host(&mut |h| assert!(h.get(SUMMARY_HEADER_ID).is_some()));
}

#[test]
fn phase1_streaming_load_earlier_anchor_compact_layout_and_gpu_render() {
    let (_rt, _endpoint, binding, workbench) = setup_workbench();

    // Send a prompt on session-alpha after changing model/effort/preset toolbar settings
    // so RunSubmissionPreparation handles non-empty draft.settings cleanly.
    binding.set_model(1);
    binding.set_effort(2);
    binding.set_preset(1);
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
    assert_eq!(binding.last_error(), None);
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
            .axis(),
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
            .axis(),
        Axis::Horizontal
    );

    // Harness stays locked on an established thread: label omits dropdown chevron, items disable.
    let harness_menu = toolbar_host
        .get(COMPOSER_HARNESS_SELECT_ID)
        .unwrap()
        .component::<MenuContent>()
        .unwrap();
    assert_eq!(harness_menu.trigger().label(), "Tekes");
    assert!(
        format!("{harness_menu:?}").contains("disabled: true"),
        "expected locked harness items to be disabled"
    );
    assert_eq!(binding.selected_harness(), "Tekes");
    binding.set_harness(1);
    assert_eq!(binding.selected_harness(), "Tekes");

    // Verify Window Toolbar dynamic menus & actions on AgentRunContainer.
    fn menu_selection_payload(
        value: &str,
        text: &mut TestTextBackend,
    ) -> Option<ContentActionPayload> {
        let mut menu = MenuContent::new("toolbar-test", vec![value.to_owned()]);
        menu.set_entries(vec![MenuEntry::item(value, value)])
            .unwrap();
        let mut h = ContentHost::new(Content::new(ComponentId(1), menu)).unwrap();
        h.set_action_events(true);
        let open_resp = h.accessibility_action(ComponentId(1), None, AccessibilityAction::Press);
        assert!(open_resp.consumed);
        let rect = Rect::new(0.0, 0.0, 240.0, 240.0);
        h.render(
            &mut Scene::new(),
            text,
            rect,
            rect,
            Palette::from(Theme::LIGHT),
        )
        .unwrap();
        let part_id = h
            .get(ComponentId(1))
            .unwrap()
            .component::<MenuContent>()
            .unwrap()
            .semantic_parts()
            .into_iter()
            .find(|p| p.semantics.role == ContentRole::MenuItem)
            .expect("rendered MenuItem semantic part")
            .index;
        let pick_resp =
            h.accessibility_action(ComponentId(1), Some(part_id), AccessibilityAction::Press);
        assert!(pick_resp.activated);
        h.drain_action_records().into_iter().find_map(|r| r.payload)
    }

    binding.set_model(0);
    binding.set_preset(0);
    binding.set_effort(1);
    let initial_items = AgentRunContainer::build_toolbar_items(&workbench);
    assert_eq!(initial_items.len(), 1);
    match &initial_items[0] {
        LocalToolbarItem::Group(group) => {
            assert_eq!(group.id, RUN_TOOLBAR_SETUP_GROUP_ID);
            assert!(group.divided);
            assert_eq!(group.overflow, LocalToolbarGroupOverflow::Together);
            assert_eq!(group.label.as_deref(), Some("Conversation setup"));
            assert_eq!(group.items.len(), 4);

            match &group.items[0] {
                LocalToolbarItem::Menu(m) => {
                    assert_eq!(m.id, RUN_TOOLBAR_HARNESS_CMD);
                    assert_eq!(m.title, "");
                    assert!(!m.title.contains("Tekes"));
                    assert_eq!(m.icon.as_deref(), Some("tekes-app-server"));
                    assert!(!m.dropdown_indicator);
                    assert_eq!(m.tone, LocalToolbarTone::Secondary);
                    assert!(
                        m.entries
                            .iter()
                            .all(|e| matches!(e, MenuEntry::Item { disabled: true, .. }))
                    );
                }
                other => panic!("expected Harness menu segment, got {other:?}"),
            }
            match &group.items[1] {
                LocalToolbarItem::Menu(m) => {
                    assert_eq!(m.id, RUN_TOOLBAR_APPROVAL_CMD);
                    assert_eq!(m.title, "");
                    assert_eq!(m.icon.as_deref(), Some("tekes-access-workspace"));
                    assert!(!m.dropdown_indicator);
                    assert_eq!(m.tone, LocalToolbarTone::Secondary);
                }
                other => panic!("expected Approval menu segment, got {other:?}"),
            }
            match &group.items[2] {
                LocalToolbarItem::Menu(m) => {
                    assert_eq!(m.id, RUN_TOOLBAR_MODEL_CMD);
                    assert_eq!(m.title, "claude-sonnet-4");
                    assert!(!m.dropdown_indicator);
                    assert_eq!(m.tone, LocalToolbarTone::Secondary);
                }
                other => panic!("expected Model menu segment, got {other:?}"),
            }
            match &group.items[3] {
                LocalToolbarItem::Menu(m) => {
                    assert_eq!(m.id, RUN_TOOLBAR_EFFORT_CMD);
                    assert_eq!(m.title, "Medium");
                    assert!(!m.dropdown_indicator);
                    assert_eq!(m.tone, LocalToolbarTone::Secondary);
                }
                other => panic!("expected Effort menu segment, got {other:?}"),
            }
        }
        other => panic!("expected segmented toolbar group, got {other:?}"),
    }

    // Locked Harness ignores toolbar selection while keeping action handled.
    let harness_action = LocalToolbarAction {
        id: RUN_TOOLBAR_HARNESS_CMD,
        source: ContentActionSource::Pointer,
        payload: menu_selection_payload("Pi", &mut text),
    };
    assert!(AgentRunContainer::handle_toolbar_action(
        &workbench,
        &harness_action
    ));
    assert_eq!(binding.selected_harness(), "Tekes");

    // Effort selection updates binding & commits `reasoningEffort` to draft settings.
    let effort_action = LocalToolbarAction {
        id: RUN_TOOLBAR_EFFORT_CMD,
        source: ContentActionSource::Pointer,
        payload: menu_selection_payload("High", &mut text),
    };
    assert!(AgentRunContainer::handle_toolbar_action(
        &workbench,
        &effort_action
    ));
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

    // Model and Approval toolbar actions update binding & refreshed Window Toolbar segment icons/labels.
    let model_action = LocalToolbarAction {
        id: RUN_TOOLBAR_MODEL_CMD,
        source: ContentActionSource::Pointer,
        payload: menu_selection_payload("gpt-5", &mut text),
    };
    assert!(AgentRunContainer::handle_toolbar_action(
        &workbench,
        &model_action
    ));
    assert_eq!(binding.selected_model(), "gpt-5");

    let approval_read_only = LocalToolbarAction {
        id: RUN_TOOLBAR_APPROVAL_CMD,
        source: ContentActionSource::Pointer,
        payload: menu_selection_payload("Read Only", &mut text),
    };
    assert!(AgentRunContainer::handle_toolbar_action(
        &workbench,
        &approval_read_only
    ));
    assert_eq!(binding.selected_preset(), "Read Only");

    let refreshed_read_only = AgentRunContainer::build_toolbar_items(&workbench);
    let LocalToolbarItem::Group(ro_group) = &refreshed_read_only[0] else {
        panic!("expected toolbar group after read-only switch");
    };
    match &ro_group.items[1] {
        LocalToolbarItem::Menu(m) => {
            assert_eq!(m.icon.as_deref(), Some("tekes-access-read"));
            assert_eq!(m.tone, LocalToolbarTone::Secondary);
        }
        other => panic!("expected Approval segment, got {other:?}"),
    }
    assert_eq!(ro_group.items[2].title(), "gpt-5");
    assert_eq!(ro_group.items[3].title(), "High");

    let approval_full_access = LocalToolbarAction {
        id: RUN_TOOLBAR_APPROVAL_CMD,
        source: ContentActionSource::Pointer,
        payload: menu_selection_payload("Full Access", &mut text),
    };
    assert!(AgentRunContainer::handle_toolbar_action(
        &workbench,
        &approval_full_access
    ));
    assert_eq!(binding.selected_preset(), "Full Access");
    let refreshed_full = AgentRunContainer::build_toolbar_items(&workbench);
    let LocalToolbarItem::Group(full_group) = &refreshed_full[0] else {
        panic!("expected toolbar group after full-access switch");
    };
    match &full_group.items[1] {
        LocalToolbarItem::Menu(m) => {
            assert_eq!(m.icon.as_deref(), Some("tekes-access-full"));
            assert_eq!(m.tone, LocalToolbarTone::Warning);
        }
        other => panic!("expected Full Access Approval segment, got {other:?}"),
    }

    // Assemble full 3-column AppWindow (`nav_bar`, `focus`, `others` List card) & render Metal GPU frame.
    let sidebar = SessionSidebarContainer::build("agent-sidebar", workbench.clone()).unwrap();
    let mut run = AgentRunContainer::build("agent-run", workbench.clone()).unwrap();
    run.with_host(&mut |h| {
        assert!(h.get(TRANSCRIPT_ROOT_ID).is_some());
        assert!(h.get(COMPOSER_CARD_ID).is_some());
        assert!(h.get(COMPOSER_INPUT_ROW_ID).is_some());
        assert!(h.get(RUN_SETUP_TOOLBAR_ROOT_ID).is_none());
        assert!(h.get(COMPOSER_HARNESS_SELECT_ID).is_none());
        assert!(h.get(COMPOSER_PRESET_SELECT_ID).is_none());
        assert!(h.get(COMPOSER_MODEL_SELECT_ID).is_none());
        assert!(h.get(COMPOSER_EFFORT_SELECT_ID).is_none());
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
    let _ = TRANSCRIPT_RICH_BODY_ID;
}

#[test]
fn navbar_repeated_session_switches_emit_strictly_unique_navigation_ids() {
    let (_rt, _endpoint, binding, workbench) = setup_workbench();

    // Alternate alpha <-> beta repeatedly (7 switches); TekesUI navigation History::validate
    // rejects any duplicate Entry.id and locks input (`nav.host.input_open = false`).
    let sequence = [
        SECONDARY_SESSION_ID,
        DEFAULT_MAIN_SESSION_ID,
        SECONDARY_SESSION_ID,
        DEFAULT_MAIN_SESSION_ID,
        SECONDARY_SESSION_ID,
        DEFAULT_MAIN_SESSION_ID,
        SECONDARY_SESSION_ID,
    ];
    for sid in sequence {
        workbench.set_active_session(SessionAddress {
            endpoint_id: "fixture".into(),
            session_id: sid.into(),
        });
    }
    // Also enqueue an explicit duplicate Entry.id and ensure workbench deduplicates it cleanly.
    workbench.enqueue_navigation(navigation::Entry {
        id: "visit-1:fixture:session-beta".to_owned(),
        route: "agent-session".to_owned(),
        params: serde_json::json!({ "endpointId": "fixture", "sessionId": SECONDARY_SESSION_ID }),
    });

    let mut emitted_ids = Vec::new();
    while let Some(entry) = workbench.pop_navigation() {
        emitted_ids.push(entry.id);
    }
    assert_eq!(emitted_ids.len(), sequence.len() + 1);
    let unique: BTreeSet<&str> = emitted_ids.iter().map(String::as_str).collect();
    assert_eq!(
        unique.len(),
        emitted_ids.len(),
        "navigation Entry IDs must stay strictly unique across repeated switches: {emitted_ids:?}"
    );

    // Rapid binding session selection must never revert due to async open_session re-select.
    binding.select_session(SECONDARY_SESSION_ID);
    binding.select_session(DEFAULT_MAIN_SESSION_ID);
    binding.select_session(SECONDARY_SESSION_ID);
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(
        binding.current_scope().unwrap().session_id,
        SECONDARY_SESSION_ID
    );
}

#[test]
fn transcript_structural_reading_anchor_and_activity_toggle_work() {
    let (_rt, _endpoint, binding, _workbench) = setup_workbench();
    let mut surface = TranscriptSurface::new();
    let mut host = ContentHost::new(TranscriptSurface::build_content()).unwrap();
    host.set_action_events(true);

    let mut text = TestTextBackend::new();
    let mut scene = Scene::new();
    let palette = Palette::from(Theme::LIGHT);
    let area = Rect::new(0.0, 0.0, 960.0, 620.0);

    let snap = binding.run_snapshot();
    surface.sync(&mut host, &snap, &[], ResponsiveWidthMode::Expanded);
    host.render(&mut scene, &mut text, area, area, palette)
        .unwrap();

    let turns_before = surface.mounted_turn_count(&host);
    assert!(
        turns_before >= 5,
        "expected structured multi-turn conversation, got {turns_before}"
    );
    assert_eq!(TranscriptSurface::is_following_latest(&host), Some(true));
    assert!(
        TranscriptSurface::scroll_offset(&host).unwrap_or(0.0) > 0.0,
        "multi-turn transcript must start scrolled down to latest turn"
    );

    // Scroll up into history so ContentReading pauses auto-follow and anchors a visible turn.
    if let Some(scroll) = host
        .get_mut(TRANSCRIPT_SCROLL_ID)
        .and_then(|c| c.component_mut::<ScrollContent>())
    {
        scroll.scroll_by(0.0, -260.0);
        scroll.stop_following();
    }
    host.render(&mut scene, &mut text, area, area, palette)
        .unwrap();
    assert_eq!(TranscriptSurface::is_following_latest(&host), Some(false));
    let y_before = TranscriptSurface::scroll_offset(&host).unwrap();

    // Load earlier turns -> ContentReading preserves the reading anchor smoothly.
    binding.load_earlier(&RunScope::main("fixture", DEFAULT_MAIN_SESSION_ID));
    for _ in 0..40 {
        if binding.run_snapshot().transcript.turns.len() > turns_before {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    let snap_after = binding.run_snapshot();
    assert!(snap_after.transcript.turns.len() > turns_before);
    surface.sync(&mut host, &snap_after, &[], ResponsiveWidthMode::Expanded);
    host.render(&mut scene, &mut text, area, area, palette)
        .unwrap();

    assert!(surface.mounted_turn_count(&host) > turns_before);
    let reading_res = host
        .get(TRANSCRIPT_SCROLL_ID)
        .unwrap()
        .component::<ScrollContent>()
        .unwrap()
        .reading_result()
        .cloned()
        .unwrap();
    assert!(
        matches!(reading_res.anchor, ContentReadingAnchorUpdate::Preserved(_)),
        "expected ContentReading to preserve visible turn anchor, got {:?}",
        reading_res.anchor
    );
    assert!(
        TranscriptSurface::scroll_offset(&host).unwrap() > y_before,
        "prepending earlier turns must advance scroll offset to keep reading anchor"
    );

    // Clicking a turn's quiet activity button toggles expansion cleanly without full rebuild.
    let latest_key = surface.mounted_turn_keys().last().cloned().unwrap();
    let toggle_id = surface
        .activity_toggle_id_for_turn_key(&latest_key)
        .expect("latest fixture turn should expose an activity toggle");
    assert!(!surface.is_turn_activity_expanded(&latest_key));

    let toggle_record = ContentActionRecord {
        action: ContentAction {
            target: toggle_id,
            kind: ContentActionKind::Activate,
            source: ContentActionSource::Pointer,
        },
        mount: host.get(toggle_id).unwrap().mount_identity(),
        payload: None,
    };
    let intents = surface.handle_actions(&mut host, &snap_after, &[toggle_record]);
    assert!(intents.is_empty());
    assert!(surface.is_turn_activity_expanded(&latest_key));
    host.render(&mut scene, &mut text, area, area, palette)
        .unwrap();
    let expanded_has_tool_details = host
        .get(TRANSCRIPT_TURNS_STACK_ID)
        .and_then(|c| c.component::<StackContent>())
        .and_then(|s| s.children().last())
        .and_then(|turn| turn.component::<StackContent>())
        .is_some_and(|turn_stack| turn_stack.children().iter().any(|c| c.id().0 % 32 == 7));
    assert!(
        expanded_has_tool_details,
        "expanded activity disclosure must mount tool/reasoning detail stack"
    );

    let collapse_record = ContentActionRecord {
        action: ContentAction {
            target: toggle_id,
            kind: ContentActionKind::Activate,
            source: ContentActionSource::Pointer,
        },
        mount: host.get(toggle_id).unwrap().mount_identity(),
        payload: None,
    };
    let _ = surface.handle_actions(&mut host, &snap_after, &[collapse_record]);
    assert!(!surface.is_turn_activity_expanded(&latest_key));
    host.render(&mut scene, &mut text, area, area, palette)
        .unwrap();
}
