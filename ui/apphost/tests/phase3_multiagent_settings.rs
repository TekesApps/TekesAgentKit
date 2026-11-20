//! Phase 3 multi-agent containers (`TasksQueue`, `ChildRun`, `SideChat`), `AgentSettingsOwner`,
//! and CAS/epoch-fenced `HarnessSettingsContainer` acceptance tests.

use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tekes_agent_apphost::{
    AgentRunContainer, ChildRunContainer, HarnessSettingsContainer, SessionSidebarContainer,
    SharedAgentWorkbench, SideChatContainer, SummaryContainer, TasksQueueContainer,
    build_agent_settings_owner,
};
use tekes_agent_components_ui::{
    CHILD_RUN_HEADER_ID, CHILD_RUN_READER_ID, CHILD_RUN_ROOT_ID, CHILD_RUN_STATUS_ID,
    COMPOSER_EDITOR_ID, SETTINGS_BANNER_ID, SETTINGS_FIELDS_STACK_ID, SETTINGS_HEADER_ID,
    SIDECHAT_ACCESS_BADGE_ID, SIDECHAT_EDITOR_ID, SIDECHAT_HEADER_ID, SIDECHAT_RICH_ID,
    TASKS_ERROR_ID, TASKS_HEADER_ID, TASKS_JOBS_SECTION_ID, TASKS_QUEUE_SECTION_ID,
    TranscriptReader,
};
use tekes_agent_contract::EndpointConnector;
use tekes_agent_presentation::{RunController, RunScope};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{
    CHILD_SUBAGENT_SESSION_ID, ChildRunBinding, ChildRunStatus, DEFAULT_MAIN_SESSION_ID,
    FixtureSessionEndpoint, HarnessSettingsBinding, MASKED_SECRET_DISPLAY, RunWorkbenchBinding,
    SECONDARY_SESSION_ID, SideChatAccess, SideChatBinding, UiWakeNotifier, project_queued_prompts,
    project_tasks,
};
use tekes_ui_app_host::{
    AppWindow, ContainerMultiplicity, ContainerOptions, ContainerPresentation, OthersOptions,
    OthersTab, PresentationOptions, PresentationState,
};
use tekes_ui_controls::composition::{
    Component, ComponentId, Content, ContentEvent, ContentHost, IconImage, IconRenderError,
    StackContent, TextContent, TextFieldContent, WithIconRenderer,
};
use tekes_ui_controls::extended::{Axis, Stack};
use tekes_ui_controls::layout_view::IconRenderer;
use tekes_ui_render::Renderer;
use tekes_ui_scene::{Rect, Scene};
use tekes_ui_tokens::{Palette, Rgba, Theme};

struct TestIcons;
impl IconRenderer for TestIcons {
    fn draw_icon(&mut self, _: &mut Scene, _: &str, _: Rect, _: Rgba) {}
    fn prepare_icon(
        &mut self,
        _: &str,
        _: f32,
        _: f32,
        _: Rgba,
    ) -> Result<IconImage, IconRenderError> {
        Ok(IconImage::new(
            tekes_ui_scene::ImageId(1),
            std::rc::Rc::new(()),
        ))
    }
}

fn layout_host_for_focus(host: &mut ContentHost) {
    let dummy_root = Content::new(
        ComponentId(1),
        StackContent {
            model: Stack {
                axis: Axis::Vertical,
                gap: 0.0,
            },
            flexible_child: None,
            children: Vec::new(),
        },
    );
    let dummy = tekes_ui_app_host::LocalContainer::new("dummy", dummy_root).unwrap();
    let app =
        AppWindow::from_containers(vec![dummy.into()], PresentationOptions::new("dummy")).unwrap();
    let (_menus, mut hosted) = app.into_window_handler().unwrap();
    let mut icons = TestIcons;
    let mut renderer = WithIconRenderer::new(hosted.text_host_mut(), &mut icons);
    let area = Rect::new(0.0, 0.0, 480.0, 720.0);
    host.render(
        &mut Scene::new(),
        &mut renderer,
        area,
        area,
        Palette::from(Theme::LIGHT),
    )
    .unwrap();
}

fn settle(rt: &tokio::runtime::Runtime, ms: u64) {
    rt.block_on(async {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    });
}

fn setup_phase3_workbench() -> (
    tokio::runtime::Runtime,
    FixtureSessionEndpoint,
    Arc<SessionRuntime>,
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
    let run_binding = {
        let _guard = rt.enter();
        RunWorkbenchBinding::new(
            "fixture",
            Arc::clone(&runtime),
            controller,
            rt.handle().clone(),
            wake,
        )
    };
    run_binding
        .open_initial_session(DEFAULT_MAIN_SESSION_ID)
        .unwrap();
    workbench.attach_run_binding(Arc::clone(&run_binding));
    (rt, endpoint, runtime, run_binding, workbench)
}

fn host_contains_text(host: &ContentHost, id: ComponentId, needle: &str) -> bool {
    host.get(id)
        .and_then(|n| n.component::<TextContent>())
        .is_some_and(|t| t.text.contains(needle))
}

fn stack_children_count(host: &ContentHost, stack_id: ComponentId) -> usize {
    host.get(stack_id)
        .and_then(|n| n.component::<StackContent>())
        .map_or(0, |s| s.children.len())
}

fn any_text_field_in_host(host: &ContentHost, root: ComponentId) -> bool {
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let Some(node) = host.get(id) else {
            continue;
        };
        if node.component::<TextFieldContent>().is_some() {
            return true;
        }
        if let Some(s) = node.component::<StackContent>() {
            for child in &s.children {
                stack.push(child.id());
            }
        }
    }
    false
}

#[test]
fn phase3_stacked_others_list_epoch_queue_and_child_run_missing() {
    let (rt, endpoint, runtime, run_binding, workbench) = setup_phase3_workbench();

    // 1. Stacked Summary (280pt) + TasksQueue (220pt) inside AppHost OthersTab::List.
    let sidebar = SessionSidebarContainer::build("agent-sidebar", workbench.clone()).unwrap();
    let run_container = AgentRunContainer::build("agent-run", workbench.clone()).unwrap();
    let summary_card =
        SummaryContainer::build_list_card("agent-summary", 280.0, workbench.clone()).unwrap();
    let mut tasks_card =
        TasksQueueContainer::build_list_card("agent-tasks", 220.0, workbench.clone()).unwrap();

    tasks_card.with_host(&mut |h| {
        assert!(host_contains_text(h, TASKS_HEADER_ID, "队列 1"));
        assert!(stack_children_count(h, TASKS_QUEUE_SECTION_ID) >= 1);
        assert!(stack_children_count(h, TASKS_JOBS_SECTION_ID) >= 1);
    });

    // 2. Test epoch-fenced queue edit & remove.
    let epoch = run_binding.connection_epoch();
    let stale_err = run_binding.edit_queued_prompt_at_epoch_blocking(
        epoch.saturating_add(77),
        DEFAULT_MAIN_SESSION_ID,
        "queue-1",
        "forbidden stale edit".to_owned(),
    );
    assert!(stale_err.is_err());

    let mut tasks_after_err =
        TasksQueueContainer::build_tab("agent-tasks-check", workbench.clone()).unwrap();
    tasks_after_err.with_host(&mut |h| {
        assert!(host_contains_text(h, TASKS_ERROR_ID, "连接"));
    });

    // Valid epoch mutates queue and projects cleanly.
    run_binding
        .edit_queued_prompt_at_epoch_blocking(
            epoch,
            DEFAULT_MAIN_SESSION_ID,
            "queue-1",
            "阶段3 已修订排队指令".to_owned(),
        )
        .unwrap();
    settle(&rt, 25);
    let queued = project_queued_prompts(&run_binding.run_snapshot());
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].text, "阶段3 已修订排队指令");

    run_binding
        .remove_queued_prompt_at_epoch_blocking(epoch, DEFAULT_MAIN_SESSION_ID, "queue-1")
        .unwrap();
    settle(&rt, 25);
    assert!(project_queued_prompts(&run_binding.run_snapshot()).is_empty());

    // Switch to secondary session containing the subagent job -> OpenChildRun projection.
    run_binding.select_session(SECONDARY_SESSION_ID);
    settle(&rt, 40);
    let sec_snap = run_binding.run_snapshot();
    let sec_facts = run_binding.summary_facts_for(&sec_snap);
    let tasks = project_tasks(&sec_snap, sec_facts.as_ref());
    assert!(
        tasks
            .iter()
            .any(|t| t.child_session_id.as_deref() == Some(CHILD_SUBAGENT_SESSION_ID)),
        "expected subagent child task row in secondary session: {tasks:?}"
    );

    // 3. Test read-only ChildRunContainer (Multiple factory), LoadEarlier, zero composer, & Missing.
    let parent_scope = RunScope::main("fixture", SECONDARY_SESSION_ID);
    let child_binding = ChildRunBinding::open_for_child(
        Arc::clone(&runtime),
        parent_scope,
        CHILD_SUBAGENT_SESSION_ID,
        rt.handle().clone(),
        workbench.wake_hub(),
    )
    .unwrap();
    child_binding.refresh_blocking().unwrap();
    workbench.attach_child_run_binding("child-audit-1", Arc::clone(&child_binding));

    let view1 = child_binding.view_snapshot();
    assert_eq!(view1.status, ChildRunStatus::Ready);
    assert!(
        view1.snapshot.transcript.has_more_before,
        "32-turn child must start paginated"
    );
    let initial_turns = view1.snapshot.transcript.turns.len();
    assert!(child_binding.load_earlier_blocking().unwrap());
    let view2 = child_binding.view_snapshot();
    assert!(
        view2.snapshot.transcript.turns.len() > initial_turns,
        "child LoadEarlier should prepend turns"
    );

    let mut child_container = ChildRunContainer::build("child-audit-1", workbench.clone()).unwrap();
    child_container.with_host(&mut |h| {
        assert!(host_contains_text(
            h,
            CHILD_RUN_HEADER_ID,
            CHILD_SUBAGENT_SESSION_ID
        ));
        assert!(host_contains_text(h, CHILD_RUN_STATUS_ID, "只读轨迹已就绪"));
        assert!(
            h.get(CHILD_RUN_READER_ID)
                .unwrap()
                .component::<TranscriptReader>()
                .is_some()
        );
        // Critical client boundary: ChildRun must NEVER contain an editable TextFieldContent!
        assert!(!any_text_field_in_host(h, CHILD_RUN_ROOT_ID));
    });

    // Expire child session on endpoint -> Missing state surfaces cleanly without panic.
    endpoint.expire_child_session(CHILD_SUBAGENT_SESSION_ID);
    settle(&rt, 40);
    let mut expired_child_container =
        ChildRunContainer::build("child-audit-1", workbench.clone()).unwrap();
    expired_child_container.with_host(&mut |h| {
        assert!(host_contains_text(h, CHILD_RUN_STATUS_ID, "Missing"));
    });

    // Verify Summary(280) + TasksQueue(220) render inside OthersTab::List on GPU Metal.
    let child_factory = ChildRunContainer::factory("child-run", workbench.clone());
    let containers = vec![
        sidebar.into(),
        run_container.into(),
        summary_card.into(),
        tasks_card.into(),
        child_container.into(),
    ];
    let mut others_opts = OthersOptions::new(vec![
        "agent-summary".to_owned(),
        "agent-tasks".to_owned(),
        "child-audit-1".to_owned(),
    ]);
    others_opts.selected = Some(OthersTab::List);
    let opts = PresentationOptions::new("agent-run")
        .nav_bar("agent-sidebar")
        .navigation_toolbar("agent-sidebar")
        .workspace_switch(true)
        .others(others_opts);
    let state = PresentationState::new(&containers, opts.clone()).unwrap();
    assert!(state.tabs().contains(&OthersTab::List));
    let app = AppWindow::from_containers(containers, opts)
        .unwrap()
        .container_factories(vec![child_factory])
        .unwrap();
    let (_menus, mut hosted) = app.into_window_handler().unwrap();
    let mut gpu = Renderer::new_headless().unwrap();
    let rgba = hosted
        .render_headless_rgba8(&mut gpu, (1280, 820), 1)
        .unwrap();
    assert_eq!(rgba.len(), 1280 * 820 * 4);
    assert!(
        rgba.chunks_exact(4)
            .any(|px| px[0] != rgba[0] || px[1] != rgba[1])
    );
    let _ = (
        ContainerOptions::default(),
        ContainerMultiplicity::Multiple,
        ContainerPresentation::Tab,
        Rect::default(),
    );
}

#[test]
fn phase3_sidechat_isolation_ime_preedit_and_settings_cas_masking() {
    let (rt, endpoint, runtime, run_binding, workbench) = setup_phase3_workbench();

    // Seed main run draft to prove SideChat never mutates or clobbers it.
    let main_dest = run_binding.current_destination().unwrap();
    run_binding.set_draft_text(&main_dest, "主会话草稿文本保持不变".to_owned());

    // 1. Create & test SideChatBinding + SideChatContainer (Multiple factory).
    let side_binding = SideChatBinding::new(
        Arc::clone(&runtime),
        rt.handle().clone(),
        workbench.wake_hub(),
    );
    workbench.attach_sidechat_binding("sidechat-1", Arc::clone(&side_binding));

    let parent_scope = RunScope::main("fixture", DEFAULT_MAIN_SESSION_ID);
    let chat = side_binding
        .create_for_parent_blocking(parent_scope, Some("方案对比"))
        .unwrap();
    assert_ne!(chat.scope.session_id, DEFAULT_MAIN_SESSION_ID);

    let side_view = side_binding.view_snapshot();
    assert_eq!(side_view.access, SideChatAccess::Available);

    let mut side_container = SideChatContainer::build("sidechat-1", workbench.clone()).unwrap();
    let mut run_container = AgentRunContainer::build("agent-run", workbench.clone()).unwrap();

    // Inject synthetic CJK Preedit into SideChat editor via ContentHost::event.
    side_container.with_host(&mut |h| {
        layout_host_for_focus(h);
        assert!(host_contains_text(h, SIDECHAT_ACCESS_BADGE_ID, "Available"));
        assert!(host_contains_text(h, SIDECHAT_HEADER_ID, "SideChat"));
        assert!(h.get(SIDECHAT_RICH_ID).is_some());
        assert!(h.focus(Some(SIDECHAT_EDITOR_ID), true));
        let resp = h.event(&ContentEvent::Preedit {
            text: "正在编辑临时分支".to_owned(),
            selection: 8..8,
            clauses: std::iter::once(0..8).collect(),
            replacement: None,
        });
        assert!(resp.consumed);
        let field = h
            .get(SIDECHAT_EDITOR_ID)
            .unwrap()
            .component::<TextFieldContent>()
            .unwrap();
        assert!(Component::composing(field));
    });

    // Main Composer must still carry only its original draft & non-composing state.
    run_container.with_host(&mut |h| {
        let main_field = h
            .get(COMPOSER_EDITOR_ID)
            .unwrap()
            .component::<TextFieldContent>()
            .unwrap();
        assert_eq!(main_field.text(), "主会话草稿文本保持不变");
        assert!(!Component::composing(main_field));
    });

    // Send SideChat prompt & then close/discard cleanly.
    side_binding.set_draft_text("仅在分支验证的提示词".to_owned());
    side_binding.send_prompt_blocking().unwrap();
    settle(&rt, 80);
    assert_eq!(
        run_binding.run_snapshot().draft.text,
        "主会话草稿文本保持不变"
    );

    side_binding.close_owned_blocking().unwrap();
    let mut side_closed = SideChatContainer::build("sidechat-1", workbench.clone()).unwrap();
    side_closed.with_host(&mut |h| {
        assert!(host_contains_text(h, SIDECHAT_ACCESS_BADGE_ID, "Closed"));
    });

    // 2. Test AgentSettingsOwner (TekesUI SettingsWindow) + HarnessSettingsContainer (CAS + Mask + ReadOnly).
    let settings_dir =
        std::env::temp_dir().join(format!("tekes-agentkit-p3-settings-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&settings_dir);
    let settings_file = settings_dir.join("settings.toml");
    let settings_owner = build_agent_settings_owner(settings_file).unwrap();

    let hs_binding = HarnessSettingsBinding::new(
        Arc::clone(&runtime),
        rt.handle().clone(),
        workbench.wake_hub(),
    );
    hs_binding.refresh_blocking().unwrap();
    workbench.attach_harness_settings_binding(Arc::clone(&hs_binding));

    let hs_snap = hs_binding.snapshot();
    assert!(hs_snap.loaded);
    assert_eq!(hs_snap.revision, 1);
    let secret_field = hs_snap.fields.iter().find(|f| f.key == "apiToken").unwrap();
    assert!(secret_field.is_secret);
    assert_eq!(secret_field.display_value, MASKED_SECRET_DISPLAY);

    let mut hs_initial =
        HarnessSettingsContainer::build_tab("harness-settings", workbench.clone()).unwrap();
    hs_initial.with_host(&mut |h| {
        assert!(host_contains_text(h, SETTINGS_HEADER_ID, "CAS rev 1"));
        let serialized_stack = format!(
            "{:?}",
            h.get(SETTINGS_FIELDS_STACK_ID)
                .unwrap()
                .component::<StackContent>()
                .unwrap()
        );
        assert!(!serialized_stack.contains("sk-fixture-secret"));
        assert!(serialized_stack.contains("••••••••"));
    });

    // Valid CAS mutate advances rev 1 -> 2.
    let mut patch = BTreeMap::new();
    patch.insert("approvalPolicy".to_owned(), json!("auto-read"));
    let new_rev = hs_binding
        .apply_patch_at_revision_blocking(patch.clone(), Some(1))
        .unwrap();
    assert_eq!(new_rev, 2);

    // Stale expectedRevision = 1 MUST trigger conflict banner & conflict_revision == Some(2).
    let mut stale_patch = BTreeMap::new();
    stale_patch.insert("approvalPolicy".to_owned(), json!("strict"));
    let conflict = hs_binding.apply_patch_at_revision_blocking(stale_patch, Some(1));
    assert!(conflict.is_err());
    assert_eq!(hs_binding.snapshot().conflict_revision, Some(2));

    let mut hs_conflict =
        HarnessSettingsContainer::build_tab("harness-settings-conflict", workbench.clone())
            .unwrap();
    hs_conflict.with_host(&mut |h| {
        assert!(host_contains_text(h, SETTINGS_BANNER_ID, "CAS 版本冲突"));
    });

    // Read-only mode propagates on refresh and blocks mutations.
    endpoint.set_settings_read_only(true);
    hs_binding.refresh_blocking().unwrap();
    assert!(hs_binding.snapshot().read_only);
    let ro_err = hs_binding.apply_patch_at_revision_blocking(patch, None);
    assert!(ro_err.is_err());

    let mut hs_readonly =
        HarnessSettingsContainer::build_tab("harness-settings", workbench.clone()).unwrap();
    hs_readonly.with_host(&mut |h| {
        assert!(host_contains_text(h, SETTINGS_HEADER_ID, "[只读]"));
        assert!(host_contains_text(h, SETTINGS_BANNER_ID, "只读"));
    });

    // Mount SideChat factory + HarnessSettings + shared_settings into AppWindow cleanly.
    let side_factory = SideChatContainer::factory("sidechat", workbench.clone());
    let summary_tab = SummaryContainer::build_tab("agent-summary", workbench).unwrap();
    let app = AppWindow::from_containers(
        vec![run_container.into(), summary_tab.into(), hs_readonly.into()],
        PresentationOptions::new("agent-run")
            .workspace_switch(true)
            .others(OthersOptions::new(vec![
                "agent-summary".to_owned(),
                "harness-settings".to_owned(),
            ])),
    )
    .unwrap()
    .shared_settings(settings_owner)
    .container_factories(vec![side_factory])
    .unwrap();
    let (_menus, mut hosted) = app.into_window_handler().unwrap();
    let mut gpu = Renderer::new_headless().unwrap();
    let rgba = hosted
        .render_headless_rgba8(&mut gpu, (1200, 780), 1)
        .unwrap();
    assert_eq!(rgba.len(), 1200 * 780 * 4);

    let _ = std::fs::remove_dir_all(&settings_dir);
}
