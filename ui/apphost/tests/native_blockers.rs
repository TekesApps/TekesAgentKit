use async_trait::async_trait;
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tekes_agent_apphost::{
    AgentRunContainer, AppHostWakeHub, SessionSidebarContainer, SharedAgentWorkbench,
    TasksQueueContainer,
};
use tekes_agent_components_ui::{
    COMPOSER_SEND_BTN_ID, ComposerIntent, ComposerSurface, RUN_CONTAINER_STACK_ID,
    ResponsiveWidthMode, SIDEBAR_CONTEXT_MENU_ID, SIDEBAR_LIST_ID, SIDEBAR_RENAME_BTN_ID,
    SIDEBAR_RENAME_INPUT_ID, SidebarFilterSelection, SidebarIntent, SidebarRowModel,
    SidebarSurface, TASKS_ERROR_ID, TRANSCRIPT_ERROR_PANEL_ID, TRANSCRIPT_ROOT_ID,
    TRANSCRIPT_STATUS_ID, TasksQueueSurface, TasksQueueViewModel, TranscriptSurface,
};
use tekes_agent_contract::{
    EndpointCapabilities, EndpointConnector, Error, FrameStream, HistoryPage, HistoryRequest,
    HostDescription, SessionEndpoint, StreamTarget, unsupported,
};
use tekes_agent_presentation::{ComposerDestination, RunController, RunScope};
use tekes_agent_session::{MemoryProjectionStore, SessionRuntime};
use tekes_agent_ui_bindings::{
    DEFAULT_MAIN_SESSION_ID, FixtureSessionEndpoint, RunWorkbenchBinding, UiWakeNotifier,
};
use tekes_ui_app_host::{AppWindow, PresentationOptions};
use tekes_ui_controls::{
    ControlKey,
    composition::{
        AccessibilityAction, Component, Content, ContentActionSource, ContentEvent, ContentHost,
        ContentKeyEvent, ContentRole, InputModifiers, KeyedSelectableListContent, MenuContent,
        PointerButton, PointerEvent, PointerModifiers, PointerPhase, StackContent, TextContent,
    },
};
use tekes_ui_scene::{Point, Rect, Scene};
use tekes_ui_text::{
    coretext::{CoreTextFonts, CoreTextShaper},
    traits::Shaper,
};
use tekes_ui_tokens::{Palette, Spacing, Theme, proposal};
use tokio::sync::{Notify, mpsc};

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

#[test]
fn blocker1_pointer_stop_after_rendered_turn_never_clears_geometry_and_cancels_at_epoch() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let endpoint = FixtureSessionEndpoint::with_timing(Duration::ZERO, Duration::from_millis(250));
    let runtime = rt.block_on(async {
        let runtime = Arc::new(SessionRuntime::spawn("fixture", Arc::new(endpoint.clone())));
        runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
        runtime
    });
    let controller =
        rt.block_on(async { Arc::new(RunController::new(Arc::clone(&runtime)).unwrap()) });
    let wake: Arc<dyn UiWakeNotifier> = AppHostWakeHub::new();
    let binding = RunWorkbenchBinding::new(
        "fixture",
        Arc::clone(&runtime),
        controller,
        rt.handle().clone(),
        wake,
    );
    binding
        .open_initial_session(DEFAULT_MAIN_SESSION_ID)
        .unwrap();

    // Mount TranscriptSurface + ComposerSurface in one ContentHost matching AgentRunContainer.
    let root = Content::new(
        RUN_CONTAINER_STACK_ID,
        StackContent::vertical(
            Spacing::Regular.value(),
            vec![
                TranscriptSurface::build_content(),
                ComposerSurface::build_content(),
            ],
        )
        .with_flexible_child(TRANSCRIPT_ROOT_ID),
    );
    let mut host = ContentHost::new(root).unwrap();
    host.set_action_events(true);
    let mut transcript = TranscriptSurface::new();
    let mut composer = ComposerSurface::new();

    let mut text = TestTextBackend::new();
    let mut scene = Scene::new();
    let palette = Palette::from(Theme::LIGHT);
    let area = Rect::new(0.0, 0.0, 960.0, 680.0);

    // Send a turn so an active running stream starts with rendered assistant RichContent present.
    let dest = ComposerDestination::run(RunScope::main("fixture", DEFAULT_MAIN_SESSION_ID));
    binding.set_draft_text(&dest, "trigger running turn".into());
    binding.submit_destination(&dest);

    for _ in 0..80 {
        let snap = binding.run_snapshot();
        if snap.run.as_ref().is_some_and(|r| r.can_cancel)
            && snap.transcript.items.iter().any(|i| i.is_provisional)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    let snap = binding.run_snapshot();
    assert!(snap.run.as_ref().is_some_and(|r| r.can_cancel));

    transcript.sync(&mut host, &snap, &[], ResponsiveWidthMode::Expanded);
    composer.sync(&mut host, &snap, ResponsiveWidthMode::Expanded);
    host.render(&mut scene, &mut text, area, area, palette)
        .unwrap();

    let stop_rect = host
        .focus_targets()
        .into_iter()
        .find(|(id, _)| *id == COMPOSER_SEND_BTN_ID)
        .map(|(_, rect)| rect)
        .expect("Stop button focus target bounds must be present");
    let center = Point::new(
        stop_rect.origin.x + stop_rect.size.width * 0.5,
        stop_rect.origin.y + stop_rect.size.height * 0.5,
    );

    // Simulate LocalContainer event -> on_action sequence WITHOUT any intermediate render.
    let mut cancel_intent = None;
    for phase in [PointerPhase::Move, PointerPhase::Down, PointerPhase::Up] {
        let button = if matches!(phase, PointerPhase::Move) {
            None
        } else {
            Some(PointerButton::Primary)
        };
        let resp = host
            .event_with_text(
                &ContentEvent::Pointer(PointerEvent {
                    phase,
                    position: center,
                    button,
                    modifiers: PointerModifiers::default(),
                    click_count: u32::from(!matches!(phase, PointerPhase::Move)),
                }),
                &mut text,
            )
            .unwrap();
        let records = host.drain_action_records();
        let _ = transcript.handle_actions(&mut host, &snap, &records);
        for intent in composer.handle_actions(&host, &snap, &records) {
            if let ComposerIntent::Cancel {
                scope,
                connection_epoch,
            } = intent
            {
                cancel_intent = Some((scope, connection_epoch));
            }
        }
        if matches!(phase, PointerPhase::Down | PointerPhase::Up) {
            assert!(
                resp.consumed || resp.activated,
                "pointer phase {phase:?} must NOT be dropped by geometry invalidation"
            );
        }
        assert!(
            host.geometry_snapshot().is_some(),
            "handle_actions must NOT invalidate ContentHost geometry during pointer sequence"
        );
    }

    let (scope, epoch) = cancel_intent.expect("PointerUp on Stop must emit ComposerIntent::Cancel");
    binding.cancel_scope_at_epoch(epoch, &scope);

    for _ in 0..80 {
        if !binding
            .run_snapshot()
            .run
            .as_ref()
            .is_some_and(|r| r.can_cancel)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    assert!(
        !binding
            .run_snapshot()
            .run
            .as_ref()
            .is_some_and(|r| r.can_cancel),
        "Cancel must end the running state cleanly"
    );
    rt.block_on(runtime.stop()).unwrap();
}

#[derive(Clone)]
struct ErrorScopeEndpoint {
    inner: Arc<Mutex<ErrorScopeState>>,
    cancel_gate_a: Arc<Notify>,
    cancel_gate_entered: Arc<Notify>,
    cancel_ok_entered: Arc<Notify>,
    cancel_ok_release: Arc<Notify>,
}

struct ErrorScopeState {
    rpcs: Vec<(String, Value)>,
    journal_txs: BTreeMap<String, mpsc::UnboundedSender<tekes_agent_contract::Result<Value>>>,
    journal_entries_a: Vec<Value>,
    inventory_tx: Option<mpsc::UnboundedSender<tekes_agent_contract::Result<Value>>>,
    custom_items: Option<Vec<Value>>,
    archived_ids: Vec<String>,
    running_a: bool,
    reject_cancel_a: bool,
    transport_error_cancel_a: bool,
    reject_cancel_b: bool,
    reject_archive: bool,
    reject_unarchive: bool,
    gate_cancel_a: bool,
    gate_ok_a: bool,
    fail_create: bool,
}

impl ErrorScopeEndpoint {
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(ErrorScopeState {
                rpcs: Vec::new(),
                journal_txs: BTreeMap::new(),
                journal_entries_a: vec![
                    json!({"event": {"type": "user/message", "seq": 0, "time": 1, "data": {"text": "init"}}}),
                ],
                inventory_tx: None,
                custom_items: None,
                archived_ids: Vec::new(),
                running_a: false,
                reject_cancel_a: true,
                transport_error_cancel_a: false,
                reject_cancel_b: false,
                reject_archive: false,
                reject_unarchive: false,
                gate_cancel_a: false,
                gate_ok_a: false,
                fail_create: false,
            })),
            cancel_gate_a: Arc::new(Notify::new()),
            cancel_gate_entered: Arc::new(Notify::new()),
            cancel_ok_entered: Arc::new(Notify::new()),
            cancel_ok_release: Arc::new(Notify::new()),
        }
    }

    fn drop_connection(&self) {
        if let Some(tx) = self.inner.lock().unwrap().inventory_tx.take() {
            let _ = tx.send(Err(Error::Disconnected));
        }
    }
}

struct ErrorScopeSessionEndpoint {
    host: HostDescription,
    source: ErrorScopeEndpoint,
}

#[async_trait]
impl EndpointConnector for ErrorScopeEndpoint {
    async fn connect(&self) -> tekes_agent_contract::Result<Arc<dyn SessionEndpoint>> {
        Ok(Arc::new(ErrorScopeSessionEndpoint {
            host: serde_json::from_value(json!({
                "protocolVersion": 4,
                "product": {"name": "Kernel", "version": "0.9.23"},
                "capabilities": [
                    "session-journal",
                    "actionable-sync",
                    "session-control-sync",
                    "session-inventory-sync"
                ],
                "cwd": "/fixture",
                "home": "/fixture",
                "attachedSessions": 0,
                "canOpenPath": false
            }))
            .unwrap(),
            source: self.clone(),
        }))
    }
}

#[async_trait]
impl SessionEndpoint for ErrorScopeSessionEndpoint {
    fn identity(&self) -> &str {
        "error-scope"
    }
    fn host(&self) -> &HostDescription {
        &self.host
    }
    fn capabilities(&self) -> EndpointCapabilities {
        EndpointCapabilities::methods(&[
            "session.cancel",
            "session.create",
            "session.prompt",
            "session.rename",
            "session.archive",
            "session.unarchive",
        ])
    }
    async fn follow(&self, target: StreamTarget) -> tekes_agent_contract::Result<FrameStream> {
        match target {
            StreamTarget::SessionInventory => {
                let (tx, rx) = mpsc::unbounded_channel();
                let (running_a, custom_items) = {
                    let mut guard = self.source.inner.lock().unwrap();
                    guard.inventory_tx = Some(tx);
                    (guard.running_a, guard.custom_items.clone())
                };
                let items = custom_items.unwrap_or_else(|| {
                    vec![
                        json!({"sessionId": "a", "running": running_a, "blank": false, "updatedAt": 10}),
                        json!({"sessionId": "b", "running": false, "blank": false, "updatedAt": 20}),
                    ]
                });
                let baseline = json!({
                    "type": "baseline",
                    "generation": "g",
                    "snapshot": {
                        "items": items
                    }
                });
                let tail = stream::unfold(rx, |mut rx| async move {
                    rx.recv().await.map(|item| (item, rx))
                });
                Ok(Box::pin(stream::once(async { Ok(baseline) }).chain(tail)))
            }
            StreamTarget::SessionControl => {
                let baseline = json!({
                    "type": "baseline",
                    "generation": "g",
                    "snapshot": {"queues": [], "jobs": [], "projections": []}
                });
                Ok(Box::pin(
                    stream::once(async { Ok(baseline) }).chain(stream::pending()),
                ))
            }
            StreamTarget::Actionables => {
                let reset = json!({"type": "reset", "generation": "g"});
                Ok(Box::pin(
                    stream::once(async { Ok(reset) }).chain(stream::pending()),
                ))
            }
            StreamTarget::SessionJournal { address, .. } => {
                let sid = address.session_id.clone();
                let (tx, rx) = mpsc::unbounded_channel();
                let entries = {
                    let mut guard = self.source.inner.lock().unwrap();
                    guard.journal_txs.insert(sid.clone(), tx);
                    if sid == "a" {
                        guard.journal_entries_a.clone()
                    } else {
                        vec![
                            json!({"event": {"type": "user/message", "seq": 0, "time": 1, "data": {"text": "init"}}}),
                        ]
                    }
                };
                let through = entries
                    .last()
                    .and_then(|e| e["event"]["seq"].as_i64())
                    .unwrap_or(0);
                let snap = json!({
                    "type": "snapshot",
                    "snapshot": {
                        "sessionId": sid,
                        "generation": "g",
                        "throughSequence": through,
                        "windowLimit": 50,
                        "entries": entries,
                        "hasMoreBefore": false
                    }
                });
                let tail = stream::unfold(rx, |mut rx| async move {
                    rx.recv().await.map(|item| (item, rx))
                });
                Ok(Box::pin(stream::once(async { Ok(snap) }).chain(tail)))
            }
            _ => Err(unsupported("unsupported stream")),
        }
    }
    async fn history_page(&self, _: HistoryRequest) -> tekes_agent_contract::Result<HistoryPage> {
        Err(unsupported("unsupported history"))
    }
    async fn respond(&self, _: &str, _: i64, _: Value) -> tekes_agent_contract::Result<()> {
        Ok(())
    }
    async fn call(
        &self,
        method: &str,
        payload: Value,
        _rpc_id: &str,
    ) -> tekes_agent_contract::Result<Value> {
        self.source
            .inner
            .lock()
            .unwrap()
            .rpcs
            .push((method.to_owned(), payload.clone()));
        match method {
            "session.cancel" => {
                let sid = payload["sessionId"].as_str().unwrap_or("");
                let (gate, gate_ok, reject, transport_err) = {
                    let guard = self.source.inner.lock().unwrap();
                    (
                        sid == "a" && guard.gate_cancel_a,
                        sid == "a" && guard.gate_ok_a,
                        (sid == "a" && guard.reject_cancel_a)
                            || (sid == "b" && guard.reject_cancel_b),
                        sid == "a" && guard.transport_error_cancel_a,
                    )
                };
                if gate {
                    let release = self.source.cancel_gate_a.notified();
                    tokio::pin!(release);
                    release.as_mut().enable();
                    self.source.cancel_gate_entered.notify_one();
                    release.await;
                } else if gate_ok {
                    let release = self.source.cancel_ok_release.notified();
                    tokio::pin!(release);
                    release.as_mut().enable();
                    self.source.cancel_ok_entered.notify_one();
                    release.await;
                }
                if transport_err {
                    return Err(Error::Transport("receipt dropped".into()));
                }
                if reject {
                    return Err(Error::Remote(format!("cancel rejected ({sid})").into()));
                }
                Ok(json!({ "cancelled": true }))
            }
            "session.create" => {
                if self.source.inner.lock().unwrap().fail_create {
                    return Err(Error::Remote("endpoint create rejected".into()));
                }
                Ok(json!({ "sessionId": "c" }))
            }
            "session.archive" => {
                let sid = payload["sessionId"].as_str().unwrap_or("").to_owned();
                let mut guard = self.source.inner.lock().unwrap();
                if guard.reject_archive {
                    return Err(Error::Remote("archive rejected".into()));
                }
                if let Some(items) = guard.custom_items.as_mut() {
                    items.retain(|item| item["sessionId"] != sid);
                }
                if !guard.archived_ids.iter().any(|id| id == &sid) {
                    guard.archived_ids.push(sid);
                }
                Ok(json!({ "archivedSessionIds": guard.archived_ids.clone() }))
            }
            "session.unarchive" => {
                let sid = payload["sessionId"].as_str().unwrap_or("").to_owned();
                let mut guard = self.source.inner.lock().unwrap();
                if guard.reject_unarchive || !guard.archived_ids.iter().any(|id| id == &sid) {
                    return Err(Error::Remote("session-not-found".into()));
                }
                guard.archived_ids.retain(|id| id != &sid);
                if let Some(items) = guard.custom_items.as_mut()
                    && !items.iter().any(|item| item["sessionId"] == sid)
                {
                    items.push(json!({
                        "sessionId": sid,
                        "running": false,
                        "blank": false,
                        "updatedAt": 25,
                        "projections": {"asOfSeq": 10, "values": {"title": "Acceptance B"}}
                    }));
                }
                Ok(json!({ "sessionId": sid }))
            }
            _ => Ok(json!({ "accepted": true })),
        }
    }
    async fn close(&self) {}
}

async fn wait_store(runtime: &SessionRuntime, pred: impl Fn(&MemoryProjectionStore) -> bool) {
    let mut sub = runtime.subscribe();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if runtime.inspect(&pred) {
                return;
            }
            sub.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}

struct PersistentSurfaces {
    transcript_host: ContentHost,
    transcript: TranscriptSurface,
    tasks_host: ContentHost,
    tasks: TasksQueueSurface,
}

impl PersistentSurfaces {
    fn new() -> Self {
        Self {
            transcript_host: ContentHost::new(TranscriptSurface::build_content()).unwrap(),
            transcript: TranscriptSurface::new(),
            tasks_host: ContentHost::new(TasksQueueSurface::build_content()).unwrap(),
            tasks: TasksQueueSurface::new(),
        }
    }

    fn sync_and_read(
        &mut self,
        workbench: &SharedAgentWorkbench,
    ) -> ((bool, Option<String>), Option<String>) {
        let run = workbench.run_binding().expect("run binding attached");
        let snapshot = run.run_snapshot();
        let active_session = run.current_scope().map(|scope| scope.session_id);
        let session_error = run.error_for_session(active_session.as_deref());
        self.transcript.set_session_error(session_error.clone());
        let actionables = run.actionables_for(&snapshot);
        self.transcript.sync(
            &mut self.transcript_host,
            &snapshot,
            &actionables,
            ResponsiveWidthMode::Expanded,
        );
        self.tasks.sync(
            &mut self.tasks_host,
            &TasksQueueViewModel {
                epoch: run.connection_epoch(),
                queued: Vec::new(),
                tasks: Vec::new(),
                error: session_error,
            },
            ResponsiveWidthMode::Expanded,
        );
        let has_panel = self
            .transcript_host
            .get(TRANSCRIPT_ERROR_PANEL_ID)
            .is_some();
        let tr_err = self
            .transcript_host
            .get(TRANSCRIPT_STATUS_ID)
            .and_then(|node| node.component::<TextContent>())
            .map(|txt| txt.text().to_owned());
        let tq_err = self
            .tasks_host
            .get(TASKS_ERROR_ID)
            .and_then(|node| node.component::<TextContent>())
            .map(|txt| txt.text().to_owned());
        ((has_panel, tr_err), tq_err)
    }
}

fn inspect_rendered_errors(
    workbench: &SharedAgentWorkbench,
    persistent: &mut PersistentSurfaces,
) -> ((bool, Option<String>), Option<String>) {
    let mut run_container = AgentRunContainer::build("agent-run", workbench.clone()).unwrap();
    let mut tasks_container =
        TasksQueueContainer::build_tab("agent-tasks", workbench.clone()).unwrap();

    let mut has_panel = false;
    let mut tr_err = None;
    run_container.with_host(&mut |host| {
        has_panel = host.get(TRANSCRIPT_ERROR_PANEL_ID).is_some();
        tr_err = host
            .get(TRANSCRIPT_STATUS_ID)
            .and_then(|node| node.component::<TextContent>())
            .map(|txt| txt.text().to_owned());
    });
    let mut tq_err = None;
    tasks_container.with_host(&mut |host| {
        tq_err = host
            .get(TASKS_ERROR_ID)
            .and_then(|node| node.component::<TextContent>())
            .map(|txt| txt.text().to_owned());
    });

    let persistent_res = persistent.sync_and_read(workbench);
    assert_eq!(
        ((has_panel, tr_err.clone()), tq_err.clone()),
        persistent_res,
        "AppHost containers and persistent incremental surfaces must agree"
    );
    ((has_panel, tr_err), tq_err)
}

#[test]
fn session_cancel_error_isolated_in_transcript_and_tasks_queue_both_orders() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let endpoint = ErrorScopeEndpoint::new();
    let runtime = rt.block_on(async {
        let runtime = Arc::new(SessionRuntime::spawn(
            "error-scope",
            Arc::new(endpoint.clone()),
        ));
        runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
        runtime
    });
    let controller =
        rt.block_on(async { Arc::new(RunController::new(Arc::clone(&runtime)).unwrap()) });
    let workbench = SharedAgentWorkbench::new();
    let wake: Arc<dyn UiWakeNotifier> = workbench.wake_hub();
    let binding = {
        let _guard = rt.enter();
        RunWorkbenchBinding::new(
            "error-scope",
            Arc::clone(&runtime),
            controller,
            rt.handle().clone(),
            wake,
        )
    };
    binding.open_initial_session("a").unwrap();
    workbench.attach_run_binding(Arc::clone(&binding));

    // Keep session A open/cancellable and prepare surface observer pair.
    let tx_a = endpoint.inner.lock().unwrap().journal_txs["a"].clone();
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {"event": {"type": "turn/start", "seq": 1, "time": 2, "data": {"turn": 1}}}
    })))
    .unwrap();
    rt.block_on(wait_store(&runtime, |s| {
        s.sessions.get("a").is_some_and(|sess| sess.run_open)
    }));

    let mut surfaces = PersistentSurfaces::new();

    // Order 1: A cancel fails while A is active, then switch to B, then back to A.
    let epoch = runtime.inspect(|s| s.connection_epoch);
    let scope_a = RunScope::main("error-scope", "a");
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    for _ in 0..50 {
        if binding.session_error("a").is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let ((has_panel, tr_err), tq_err) = inspect_rendered_errors(&workbench, &mut surfaces);
    assert!(has_panel);
    assert!(tr_err.is_some_and(|t| t.contains("Cancel failed")));
    assert!(tq_err.is_some_and(|t| t.contains("Cancel failed")));

    binding.select_session("b");
    rt.block_on(wait_store(&runtime, |s| {
        s.sessions.get("b").is_some_and(|sess| sess.journal_ready)
    }));
    let ((has_panel_b, tr_err_b), tq_err_b) = inspect_rendered_errors(&workbench, &mut surfaces);
    assert!(!has_panel_b);
    assert_eq!(tr_err_b, None);
    assert_eq!(tq_err_b.as_deref(), Some(""));

    binding.select_session("a");
    let ((has_panel_a2, tr_err_a2), tq_err_a2) = inspect_rendered_errors(&workbench, &mut surfaces);
    assert!(has_panel_a2);
    assert!(tr_err_a2.is_some_and(|t| t.contains("Cancel failed")));
    assert!(tq_err_a2.is_some_and(|t| t.contains("Cancel failed")));

    // Order 2: Switch to B BEFORE A's gated cancel rejects (and while B has its own error).
    binding.clear_session_error("a");
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.gate_cancel_a = true;
        guard.reject_cancel_a = true;
    }
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    binding.select_session("b");
    let scope_b = RunScope::main("error-scope", "b");
    binding.cancel_scope_at_epoch(epoch + 99, &scope_b);
    for _ in 0..50 {
        if binding.session_error("b").is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let ((_, _), b_err_opt) = inspect_rendered_errors(&workbench, &mut surfaces);
    let b_err_before = b_err_opt.unwrap();
    assert!(b_err_before.contains("Cancel failed"));
    assert!(!b_err_before.contains("cancel rejected (a)"));

    {
        endpoint.inner.lock().unwrap().gate_cancel_a = false;
    }
    endpoint.cancel_gate_a.notify_waiters();
    for _ in 0..50 {
        if binding.session_error("a").is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let ((_, tr_b_after_a_fail), tq_b_after_a_fail) =
        inspect_rendered_errors(&workbench, &mut surfaces);
    assert_eq!(tr_b_after_a_fail.as_deref(), Some(b_err_before.as_str()));
    assert_eq!(tq_b_after_a_fail.as_deref(), Some(b_err_before.as_str()));

    // Late A cancel success must NOT clear B's error, while clearing A's own error.
    {
        endpoint.inner.lock().unwrap().reject_cancel_a = false;
    }
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    for _ in 0..50 {
        if binding.session_error("a").is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let ((_, tr_b_after_a_ok), tq_b_after_a_ok) =
        inspect_rendered_errors(&workbench, &mut surfaces);
    assert_eq!(tr_b_after_a_ok.as_deref(), Some(b_err_before.as_str()));
    assert_eq!(tq_b_after_a_ok.as_deref(), Some(b_err_before.as_str()));

    // Also verify when B has no error, late A failure while B selected leaves B clean, and A visible.
    binding.clear_session_error("b");
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.gate_cancel_a = true;
        guard.reject_cancel_a = true;
    }
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    std::thread::sleep(Duration::from_millis(20));
    endpoint.inner.lock().unwrap().gate_cancel_a = false;
    endpoint.cancel_gate_a.notify_waiters();
    for _ in 0..50 {
        if binding.session_error("a").is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let ((has_panel_b_clean, tr_b_clean), tq_b_clean) =
        inspect_rendered_errors(&workbench, &mut surfaces);
    assert!(!has_panel_b_clean);
    assert_eq!(tr_b_clean, None);
    assert_eq!(tq_b_clean.as_deref(), Some(""));

    binding.select_session("a");
    let ((has_panel_a3, tr_err_a3), tq_err_a3) = inspect_rendered_errors(&workbench, &mut surfaces);
    assert!(has_panel_a3);
    assert!(tr_err_a3.is_some_and(|t| t.contains("cancel rejected (a)")));
    assert!(tq_err_a3.is_some_and(|t| t.contains("cancel rejected (a)")));

    // True endpoint-level failure (`create_session`) surfaces in active session containers and clears.
    binding.clear_session_error("a");
    endpoint.inner.lock().unwrap().fail_create = true;
    binding.create_session("/fixture");
    for _ in 0..50 {
        if binding.last_error().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let ((has_panel_ep, tr_err_ep), tq_err_ep) = inspect_rendered_errors(&workbench, &mut surfaces);
    assert!(has_panel_ep);
    assert!(tr_err_ep.is_some_and(|t| t.contains("endpoint create rejected")));
    assert!(tq_err_ep.is_some_and(|t| t.contains("endpoint create rejected")));
    binding.clear_error();
    let ((has_panel_ep_cleared, tr_err_ep_cleared), tq_err_ep_cleared) =
        inspect_rendered_errors(&workbench, &mut surfaces);
    assert!(!has_panel_ep_cleared);
    assert_eq!(tr_err_ep_cleared, None);
    assert_eq!(tq_err_ep_cleared.as_deref(), Some(""));

    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn same_turn_stop_retry_clears_previous_error_in_transcript_and_tasks_queue() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let endpoint = ErrorScopeEndpoint::new();
    let runtime = rt.block_on(async {
        let runtime = Arc::new(SessionRuntime::spawn(
            "error-scope",
            Arc::new(endpoint.clone()),
        ));
        runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
        runtime
    });
    let controller =
        rt.block_on(async { Arc::new(RunController::new(Arc::clone(&runtime)).unwrap()) });
    let workbench = SharedAgentWorkbench::new();
    let wake: Arc<dyn UiWakeNotifier> = workbench.wake_hub();
    let binding = {
        let _guard = rt.enter();
        RunWorkbenchBinding::new(
            "error-scope",
            Arc::clone(&runtime),
            controller,
            rt.handle().clone(),
            wake,
        )
    };
    binding.open_initial_session("b").unwrap();
    {
        endpoint.inner.lock().unwrap().reject_cancel_b = true;
    }
    binding.cancel_scope_at_epoch(999, &RunScope::main("error-scope", "b"));
    for _ in 0..50 {
        if binding.error_for_session(Some("b")).is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    binding.select_session("a");
    rt.block_on(wait_store(&runtime, |s| {
        s.sessions.get("a").is_some_and(|sess| sess.journal_ready)
    }));
    workbench.attach_run_binding(Arc::clone(&binding));

    let tx_a = endpoint.inner.lock().unwrap().journal_txs["a"].clone();
    endpoint.inner.lock().unwrap().running_a = true;
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {"event": {"type": "turn/start", "seq": 1, "time": 2, "data": {"turn": 1}}}
    })))
    .unwrap();
    rt.block_on(wait_store(&runtime, |s| {
        s.sessions.get("a").is_some_and(|sess| sess.run_open)
    }));

    let mut surfaces = PersistentSurfaces::new();
    let scope_a = RunScope::main("error-scope", "a");

    // 1. Stale-epoch Stop shows error in both panels -> same-turn new-epoch Stop retry clears both panels.
    let old_epoch = runtime.inspect(|s| s.connection_epoch);
    endpoint.drop_connection();
    rt.block_on(wait_store(&runtime, |s| {
        s.connection != tekes_agent_session::ConnectionState::Ready
    }));
    rt.block_on(wait_store(&runtime, |s| {
        s.connection == tekes_agent_session::ConnectionState::Ready
            && s.connection_epoch > old_epoch
            && s.sessions.get("a").is_some_and(|sess| sess.journal_ready)
    }));
    let new_epoch = runtime.inspect(|s| s.connection_epoch);
    let tx_a2 = endpoint.inner.lock().unwrap().journal_txs["a"].clone();
    tx_a2
        .send(Ok(json!({
            "type": "event",
            "generation": "g",
            "sessionId": "a",
            "entry": {"event": {"type": "turn/start", "seq": 1, "time": 2, "data": {"turn": 1}}}
        })))
        .unwrap();
    rt.block_on(wait_store(&runtime, |s| s.sessions["a"].run_open));

    binding.cancel_scope_at_epoch(old_epoch, &scope_a);
    for _ in 0..50 {
        if binding.error_for_session(Some("a")).is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let ((has_panel, tr_err), tq_err) = inspect_rendered_errors(&workbench, &mut surfaces);
    assert!(has_panel);
    assert!(tr_err.is_some_and(|t| t.contains("Cancel failed")));
    assert!(tq_err.is_some_and(|t| t.contains("Cancel failed")));

    endpoint.inner.lock().unwrap().reject_cancel_a = false;
    binding.cancel_scope_at_epoch(new_epoch, &scope_a);
    for _ in 0..50 {
        if binding.error_for_session(Some("a")).is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let ((has_panel_cleared, tr_cleared), tq_cleared) =
        inspect_rendered_errors(&workbench, &mut surfaces);
    assert!(!has_panel_cleared);
    assert_eq!(tr_cleared, None);
    assert_eq!(tq_cleared.as_deref(), Some(""));

    // 2. Same-turn 1st Stop explicit rejection -> aborted replay + 2nd Stop HTTP-only loss clears both panels,
    // stays clear after B -> A, while B keeps its own error.
    endpoint.inner.lock().unwrap().reject_cancel_a = true;
    binding.cancel_scope_at_epoch(new_epoch, &scope_a);
    for _ in 0..50 {
        if binding.error_for_session(Some("a")).is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let ((has_panel_rej, tr_rej), tq_rej) = inspect_rendered_errors(&workbench, &mut surfaces);
    assert!(has_panel_rej);
    assert!(tr_rej.is_some_and(|t| t.contains("cancel rejected (a)")));
    assert!(tq_rej.is_some_and(|t| t.contains("cancel rejected (a)")));

    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.reject_cancel_a = false;
        guard.transport_error_cancel_a = true;
        guard.gate_cancel_a = true;
    }
    binding.cancel_scope_at_epoch(new_epoch, &scope_a);
    std::thread::sleep(Duration::from_millis(20));
    endpoint.inner.lock().unwrap().running_a = false;
    {
        endpoint.inner.lock().unwrap().journal_entries_a = vec![
            json!({"event": {"type": "user/message", "seq": 0, "time": 1, "data": {"text": "init"}}}),
            json!({"event": {"type": "turn/start", "seq": 1, "time": 2, "data": {"turn": 1}}}),
            json!({"event": {"type": "turn/end", "seq": 2, "time": 3, "data": {"turn": 1, "reason": "aborted"}}}),
        ];
    }
    tx_a2
        .send(Ok(json!({
            "type": "event",
            "generation": "g",
            "sessionId": "a",
            "entry": {"event": {"type": "turn/end", "seq": 2, "time": 3, "data": {"turn": 1, "reason": "aborted"}}}
        })))
        .unwrap();
    rt.block_on(wait_store(&runtime, |s| !s.sessions["a"].run_open));
    endpoint.inner.lock().unwrap().gate_cancel_a = false;
    endpoint.cancel_gate_a.notify_waiters();
    for _ in 0..50 {
        if binding.error_for_session(Some("a")).is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let ((has_panel_after, tr_after), tq_after) =
        inspect_rendered_errors(&workbench, &mut surfaces);
    assert!(!has_panel_after);
    assert_eq!(tr_after, None);
    assert_eq!(tq_after.as_deref(), Some(""));

    // Switch B -> A: B still shows its own error, A remains clean.
    binding.select_session("b");
    rt.block_on(wait_store(&runtime, |s| s.sessions["b"].journal_ready));
    let ((has_panel_b, tr_b), tq_b) = inspect_rendered_errors(&workbench, &mut surfaces);
    assert!(has_panel_b);
    assert!(tr_b.is_some_and(|t| t.contains("Cancel failed")));
    assert!(tq_b.is_some_and(|t| t.contains("Cancel failed")));

    binding.select_session("a");
    rt.block_on(wait_store(&runtime, |s| {
        s.sessions.get("a").is_some_and(|sess| sess.journal_ready)
    }));
    let ((has_panel_a2, tr_a2), tq_a2) = inspect_rendered_errors(&workbench, &mut surfaces);
    assert!(!has_panel_a2);
    assert_eq!(tr_a2, None);
    assert_eq!(tq_a2.as_deref(), Some(""));

    // 3. Control: stale-epoch Stop without retry preserves error across natural completion in both panels.
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.running_a = true;
        guard.journal_entries_a.push(
            json!({"event": {"type": "turn/start", "seq": 3, "time": 4, "data": {"turn": 2}}}),
        );
    }
    let tx_a3 = endpoint.inner.lock().unwrap().journal_txs["a"].clone();
    tx_a3
        .send(Ok(json!({
            "type": "event",
            "generation": "g",
            "sessionId": "a",
            "entry": {"event": {"type": "turn/start", "seq": 3, "time": 4, "data": {"turn": 2}}}
        })))
        .unwrap();
    rt.block_on(wait_store(&runtime, |s| s.sessions["a"].run_open));
    binding.cancel_scope_at_epoch(old_epoch, &scope_a);
    for _ in 0..50 {
        if binding.error_for_session(Some("a")).is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    endpoint.inner.lock().unwrap().running_a = false;
    tx_a3
        .send(Ok(json!({
            "type": "event",
            "generation": "g",
            "sessionId": "a",
            "entry": {"event": {"type": "turn/end", "seq": 4, "time": 5, "data": {"turn": 2, "reason": "completed"}}}
        })))
        .unwrap();
    rt.block_on(wait_store(&runtime, |s| !s.sessions["a"].run_open));
    std::thread::sleep(Duration::from_millis(30));
    let ((has_panel_ctrl, tr_ctrl), tq_ctrl) = inspect_rendered_errors(&workbench, &mut surfaces);
    assert!(has_panel_ctrl);
    assert!(tr_ctrl.is_some_and(|t| t.contains("Cancel failed")));
    assert!(tq_ctrl.is_some_and(|t| t.contains("Cancel failed")));

    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn overlapping_same_turn_stop_late_rejection_does_not_resurrect_error_in_transcript_and_tasks_queue()
 {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let endpoint = ErrorScopeEndpoint::new();
    let runtime = rt.block_on(async {
        let runtime = Arc::new(SessionRuntime::spawn(
            "error-scope",
            Arc::new(endpoint.clone()),
        ));
        runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
        runtime
    });
    let controller =
        rt.block_on(async { Arc::new(RunController::new(Arc::clone(&runtime)).unwrap()) });
    let workbench = SharedAgentWorkbench::new();
    let wake: Arc<dyn UiWakeNotifier> = workbench.wake_hub();
    let binding = {
        let _guard = rt.enter();
        RunWorkbenchBinding::new(
            "error-scope",
            Arc::clone(&runtime),
            controller,
            rt.handle().clone(),
            wake,
        )
    };
    binding.open_initial_session("b").unwrap();
    {
        endpoint.inner.lock().unwrap().reject_cancel_b = true;
    }
    binding.cancel_scope_at_epoch(999, &RunScope::main("error-scope", "b"));
    for _ in 0..50 {
        if binding.error_for_session(Some("b")).is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    binding.select_session("a");
    rt.block_on(wait_store(&runtime, |s| {
        s.sessions.get("a").is_some_and(|sess| sess.journal_ready)
    }));
    workbench.attach_run_binding(Arc::clone(&binding));

    let tx_a = endpoint.inner.lock().unwrap().journal_txs["a"].clone();
    endpoint.inner.lock().unwrap().running_a = true;
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {"event": {"type": "turn/start", "seq": 1, "time": 2, "data": {"turn": 1}}}
    })))
    .unwrap();
    rt.block_on(wait_store(&runtime, |s| s.sessions["a"].run_open));

    let mut surfaces = PersistentSurfaces::new();
    let scope_a = RunScope::main("error-scope", "a");
    let epoch = runtime.inspect(|s| s.connection_epoch);

    // 1. Overlap order: Stop #1 parked (will reject), Stop #2 succeeds + turn aborts -> release Stop #1 -> both panels remain clean.
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.gate_cancel_a = true;
        guard.reject_cancel_a = true;
    }
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    rt.block_on(endpoint.cancel_gate_entered.notified());

    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.gate_cancel_a = false;
        guard.gate_ok_a = true;
        guard.reject_cancel_a = false;
        guard.running_a = false;
        guard.journal_entries_a = vec![
            json!({"event": {"type": "user/message", "seq": 0, "time": 1, "data": {"text": "init"}}}),
            json!({"event": {"type": "turn/start", "seq": 1, "time": 2, "data": {"turn": 1}}}),
            json!({"event": {"type": "turn/end", "seq": 2, "time": 3, "data": {"turn": 1, "reason": "aborted"}}}),
        ];
    }
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    rt.block_on(endpoint.cancel_ok_entered.notified());
    endpoint.inner.lock().unwrap().gate_ok_a = false;
    endpoint.cancel_ok_release.notify_one();
    tx_a.send(Ok(json!({
        "type": "event",
        "generation": "g",
        "sessionId": "a",
        "entry": {"event": {"type": "turn/end", "seq": 2, "time": 3, "data": {"turn": 1, "reason": "aborted"}}}
    })))
    .unwrap();
    rt.block_on(wait_store(&runtime, |s| !s.sessions["a"].run_open));
    std::thread::sleep(Duration::from_millis(20));
    let before_release = inspect_rendered_errors(&workbench, &mut surfaces);
    assert_eq!(before_release, ((false, None), Some(String::new())));

    endpoint.cancel_gate_a.notify_one();
    std::thread::sleep(Duration::from_millis(30));
    let after_late_release = inspect_rendered_errors(&workbench, &mut surfaces);
    assert_eq!(after_late_release, ((false, None), Some(String::new())));

    // Switch B -> A: B retains its own error, A stays clean.
    binding.select_session("b");
    rt.block_on(wait_store(&runtime, |s| s.sessions["b"].journal_ready));
    let ((has_panel_b, tr_b), tq_b) = inspect_rendered_errors(&workbench, &mut surfaces);
    assert!(has_panel_b);
    assert!(tr_b.is_some_and(|t| t.contains("Cancel failed")));
    assert!(tq_b.is_some_and(|t| t.contains("Cancel failed")));

    binding.select_session("a");
    rt.block_on(wait_store(&runtime, |s| s.sessions["a"].journal_ready));
    let after_switch_back = inspect_rendered_errors(&workbench, &mut surfaces);
    assert_eq!(after_switch_back, ((false, None), Some(String::new())));

    // 2. Ordered control: Turn 2 Stop #1 rejects first -> both panels show failure -> Stop #2 succeeds -> both panels clear.
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.running_a = true;
        guard.reject_cancel_a = true;
        guard.journal_entries_a.push(
            json!({"event": {"type": "turn/start", "seq": 3, "time": 4, "data": {"turn": 2}}}),
        );
    }
    let tx_a2 = endpoint.inner.lock().unwrap().journal_txs["a"].clone();
    tx_a2
        .send(Ok(json!({
            "type": "event",
            "generation": "g",
            "sessionId": "a",
            "entry": {"event": {"type": "turn/start", "seq": 3, "time": 4, "data": {"turn": 2}}}
        })))
        .unwrap();
    rt.block_on(wait_store(&runtime, |s| s.sessions["a"].run_open));

    binding.cancel_scope_at_epoch(epoch, &scope_a);
    for _ in 0..50 {
        if binding.error_for_session(Some("a")).is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let ((has_panel_ord, tr_ord), tq_ord) = inspect_rendered_errors(&workbench, &mut surfaces);
    assert!(has_panel_ord);
    assert!(tr_ord.is_some_and(|t| t.contains("cancel rejected (a)")));
    assert!(tq_ord.is_some_and(|t| t.contains("cancel rejected (a)")));

    endpoint.inner.lock().unwrap().reject_cancel_a = false;
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    for _ in 0..50 {
        if binding.error_for_session(Some("a")).is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let after_ordered = inspect_rendered_errors(&workbench, &mut surfaces);
    assert_eq!(after_ordered, ((false, None), Some(String::new())));

    // 3. Adjacent ("between") permutation on Turn 3: Click 1 (parked) -> Click 2 (parked) -> Recv 1 rejects -> Recv 2 succeeds -> both panels clear & B -> A stays clean.
    endpoint.inner.lock().unwrap().running_a = false;
    endpoint.inner.lock().unwrap().journal_entries_a.push(
        json!({"event": {"type": "turn/end", "seq": 4, "time": 5, "data": {"turn": 2, "reason": "aborted"}}}),
    );
    tx_a2
        .send(Ok(json!({
            "type": "event",
            "generation": "g",
            "sessionId": "a",
            "entry": {"event": {"type": "turn/end", "seq": 4, "time": 5, "data": {"turn": 2, "reason": "aborted"}}}
        })))
        .unwrap();
    rt.block_on(wait_store(&runtime, |s| !s.sessions["a"].run_open));
    endpoint.inner.lock().unwrap().running_a = true;
    endpoint
        .inner
        .lock()
        .unwrap()
        .journal_entries_a
        .push(json!({"event": {"type": "turn/start", "seq": 5, "time": 6, "data": {"turn": 3}}}));
    tx_a2
        .send(Ok(json!({
            "type": "event",
            "generation": "g",
            "sessionId": "a",
            "entry": {"event": {"type": "turn/start", "seq": 5, "time": 6, "data": {"turn": 3}}}
        })))
        .unwrap();
    rt.block_on(wait_store(&runtime, |s| s.sessions["a"].run_open));

    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.gate_cancel_a = true;
        guard.reject_cancel_a = true;
    }
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    rt.block_on(endpoint.cancel_gate_entered.notified());

    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.gate_cancel_a = false;
        guard.gate_ok_a = true;
        guard.reject_cancel_a = false;
    }
    binding.cancel_scope_at_epoch(epoch, &scope_a);
    rt.block_on(endpoint.cancel_ok_entered.notified());

    endpoint.cancel_gate_a.notify_one();
    for _ in 0..200 {
        if binding.error_for_session(Some("a")).is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let ((has_panel_bt1, tr_bt1), tq_bt1) = inspect_rendered_errors(&workbench, &mut surfaces);
    assert!(has_panel_bt1);
    assert!(tr_bt1.is_some_and(|t| t.contains("cancel rejected (a)")));
    assert!(tq_bt1.is_some_and(|t| t.contains("cancel rejected (a)")));

    endpoint.cancel_ok_release.notify_one();
    for _ in 0..200 {
        if binding.error_for_session(Some("a")).is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    endpoint.inner.lock().unwrap().running_a = false;
    endpoint.inner.lock().unwrap().journal_entries_a.push(
        json!({"event": {"type": "turn/end", "seq": 6, "time": 7, "data": {"turn": 3, "reason": "aborted"}}}),
    );
    tx_a2
        .send(Ok(json!({
            "type": "event",
            "generation": "g",
            "sessionId": "a",
            "entry": {"event": {"type": "turn/end", "seq": 6, "time": 7, "data": {"turn": 3, "reason": "aborted"}}}
        })))
        .unwrap();
    rt.block_on(wait_store(&runtime, |s| !s.sessions["a"].run_open));
    let after_between = inspect_rendered_errors(&workbench, &mut surfaces);
    assert_eq!(after_between, ((false, None), Some(String::new())));

    binding.select_session("b");
    rt.block_on(wait_store(&runtime, |s| s.sessions["b"].journal_ready));
    let ((has_panel_b2, tr_b2), tq_b2) = inspect_rendered_errors(&workbench, &mut surfaces);
    assert!(has_panel_b2);
    assert!(tr_b2.is_some_and(|t| t.contains("Cancel failed")));
    assert!(tq_b2.is_some_and(|t| t.contains("Cancel failed")));

    binding.select_session("a");
    rt.block_on(wait_store(&runtime, |s| s.sessions["a"].journal_ready));
    let after_b_to_a = inspect_rendered_errors(&workbench, &mut surfaces);
    assert_eq!(after_b_to_a, ((false, None), Some(String::new())));

    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn session_sidebar_container_renders_confirmed_titles_matches_search_and_routes_explicit_archive_unarchive()
 {
    let endpoint = ErrorScopeEndpoint::new();
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.custom_items = Some(vec![
            json!({"sessionId": "a", "running": false, "blank": false, "updatedAt": 10, "projections": {"asOfSeq": 1, "values": {"title": "Acceptance A"}}}),
            json!({"sessionId": "b", "running": false, "blank": false, "updatedAt": 20, "projections": {"asOfSeq": 10, "values": {"title": "Acceptance B"}}}),
            json!({"sessionId": "c", "running": false, "blank": false, "updatedAt": 5, "status": "archived", "projections": {"asOfSeq": 2, "values": {"title": "Archived C"}}}),
        ]);
        guard.archived_ids = vec!["c".to_owned()];
    }

    let rt = tokio::runtime::Runtime::new().unwrap();
    let runtime = rt.block_on(async {
        let runtime = Arc::new(SessionRuntime::spawn(
            "error-scope",
            Arc::new(endpoint.clone()),
        ));
        runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
        runtime
    });
    let controller =
        rt.block_on(async { Arc::new(RunController::new(Arc::clone(&runtime)).unwrap()) });
    let workbench = SharedAgentWorkbench::new();
    let binding = {
        let _guard = rt.enter();
        RunWorkbenchBinding::new(
            "error-scope",
            Arc::clone(&runtime),
            controller,
            rt.handle().clone(),
            workbench.wake_hub(),
        )
    };
    binding.open_initial_session("a").unwrap();
    workbench.attach_run_binding(Arc::clone(&binding));

    // Mount only public AppHost containers and wire AppWindow navigation.
    let main = AgentRunContainer::build("run", workbench.clone()).unwrap();
    let sidebar = SessionSidebarContainer::build("sidebar", workbench.clone()).unwrap();
    let _app = AppWindow::from_containers(
        vec![sidebar.into(), main.into()],
        PresentationOptions::new("run").nav_bar("sidebar"),
    )
    .unwrap();

    // Drive SidebarSurface on ContentHost to verify rendered row titles, search hit, and context menu actions.
    let mut host = ContentHost::new(SidebarSurface::build_content()).unwrap();
    host.set_window_active(true);
    host.set_action_events(true);
    let mut surface = SidebarSurface::new();
    let mut text = TestTextBackend::new();

    let sync_sidebar =
        |host: &mut ContentHost, surface: &mut SidebarSurface, text: &mut TestTextBackend| {
            let rows: Vec<SidebarRowModel> = binding
                .sidebar_rows()
                .into_iter()
                .map(|r| SidebarRowModel {
                    session_id: r.session_id,
                    title: r.title,
                    folder: r.folder,
                    activity: r.activity,
                    running: r.running,
                    pinned: r.pinned,
                    archived: r.archived,
                })
                .collect();
            let selected = binding.current_scope().map(|s| s.session_id);
            surface.sync(
                host,
                &rows,
                selected.as_deref(),
                SidebarFilterSelection::All,
                ResponsiveWidthMode::Expanded,
            );
            let rect = Rect::new(0.0, 0.0, 320.0, 640.0);
            host.render(
                &mut Scene::new(),
                text,
                rect,
                rect,
                Palette::from(Theme::LIGHT),
            )
            .unwrap();
        };

    sync_sidebar(&mut host, &mut surface, &mut text);
    let menu_binding = host
        .bind_context_menu(SIDEBAR_LIST_ID, SIDEBAR_CONTEXT_MENU_ID)
        .unwrap();
    let visible_names = |host: &ContentHost| -> Vec<String> {
        host.get(SIDEBAR_LIST_ID)
            .and_then(|c| c.component::<KeyedSelectableListContent>())
            .unwrap()
            .semantic_parts()
            .into_iter()
            .map(|p| p.semantics.name)
            .collect()
    };
    let names = visible_names(&host);
    assert!(
        names.iter().any(|n| n == "Acceptance B"),
        "names: {names:?}"
    );

    binding.select_session("b");
    rt.block_on(wait_store(&runtime, |s| s.sessions["b"].journal_ready));
    sync_sidebar(&mut host, &mut surface, &mut text);

    let trigger_menu_item = |host: &mut ContentHost,
                             surface: &mut SidebarSurface,
                             text: &mut TestTextBackend,
                             label: &str| {
        host.request_context_menu(menu_binding, ContentActionSource::Accessibility)
            .unwrap();
        let rect = Rect::new(0.0, 0.0, 320.0, 640.0);
        host.render(
            &mut Scene::new(),
            text,
            rect,
            rect,
            Palette::from(Theme::LIGHT),
        )
        .unwrap();
        let part_id = host
            .get(SIDEBAR_CONTEXT_MENU_ID)
            .and_then(|c| c.component::<MenuContent>())
            .unwrap()
            .semantic_parts()
            .into_iter()
            .find(|p| p.semantics.role == ContentRole::MenuItem && p.semantics.name == label)
            .unwrap_or_else(|| panic!("expected MenuItem `{label}`"))
            .index;
        let pick_resp = host.accessibility_action(
            SIDEBAR_CONTEXT_MENU_ID,
            Some(part_id),
            AccessibilityAction::Press,
        );
        assert!(pick_resp.activated);
        let records = host.drain_action_records();
        let selected = binding.current_scope().map(|s| s.session_id);
        for intent in surface.handle_actions(host, selected.as_deref(), &records) {
            match intent {
                SidebarIntent::ArchiveSession(sid) => binding.archive_session(&sid),
                SidebarIntent::UnarchiveSession(sid) => binding.unarchive_session(&sid),
                _ => {}
            }
        }
    };

    // Archive B, restore B externally, reconnect -> B reappears active -> menu "Archive" sends session.archive to B.
    trigger_menu_item(&mut host, &mut surface, &mut text, "Archive");
    for _ in 0..80 {
        if endpoint
            .inner
            .lock()
            .unwrap()
            .rpcs
            .iter()
            .any(|(m, p)| m == "session.archive" && p["sessionId"] == "b")
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    rt.block_on(runtime.reconnect()).unwrap();
    rt.block_on(runtime.unarchive("b", "ext-restore")).unwrap();
    rt.block_on(runtime.reconnect()).unwrap();
    binding.select_session("b");
    rt.block_on(wait_store(&runtime, |s| s.sessions["b"].journal_ready));
    sync_sidebar(&mut host, &mut surface, &mut text);
    assert!(visible_names(&host).iter().any(|n| n == "Acceptance B"));

    let rpc_start = endpoint.inner.lock().unwrap().rpcs.len();
    trigger_menu_item(&mut host, &mut surface, &mut text, "Archive");
    for _ in 0..80 {
        if endpoint.inner.lock().unwrap().rpcs[rpc_start..]
            .iter()
            .any(|(m, p)| m == "session.archive" && p["sessionId"] == "b")
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let after_rpcs = endpoint.inner.lock().unwrap().rpcs[rpc_start..].to_vec();
    assert!(
        after_rpcs
            .iter()
            .any(|(m, p)| m == "session.archive" && p["sessionId"] == "b")
    );
    assert!(!after_rpcs.iter().any(|(m, _)| m == "session.unarchive"));

    // Selecting authoritatively archived row C flips context menu to "Unarchive" and routes to session.unarchive.
    binding.select_session("c");
    rt.block_on(wait_store(&runtime, |s| s.sessions["c"].journal_ready));
    sync_sidebar(&mut host, &mut surface, &mut text);
    assert!(
        visible_names(&host)
            .iter()
            .any(|n| n == "Archived C (Archived)")
    );
    let c_rpc_start = endpoint.inner.lock().unwrap().rpcs.len();
    trigger_menu_item(&mut host, &mut surface, &mut text, "Unarchive");
    for _ in 0..80 {
        if endpoint.inner.lock().unwrap().rpcs[c_rpc_start..]
            .iter()
            .any(|(m, p)| m == "session.unarchive" && p["sessionId"] == "c")
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let c_rpcs = endpoint.inner.lock().unwrap().rpcs[c_rpc_start..].to_vec();
    assert!(
        c_rpcs
            .iter()
            .any(|(m, p)| m == "session.unarchive" && p["sessionId"] == "c")
    );
    assert!(!c_rpcs.iter().any(|(m, _)| m == "session.archive"));

    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn sidebar_secondary_click_targets_clicked_row_preserving_selection_and_cancels_when_row_disappears()
 {
    let endpoint = ErrorScopeEndpoint::new();
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.custom_items = Some(vec![
            json!({"sessionId": "a", "running": false, "blank": false, "updatedAt": 10, "projections": {"asOfSeq": 1, "values": {"title": "Acceptance A"}}}),
            json!({"sessionId": "b", "running": false, "blank": false, "updatedAt": 20, "status": "archived", "projections": {"asOfSeq": 2, "values": {"title": "Archived B"}}}),
            json!({"sessionId": "c", "running": false, "blank": false, "updatedAt": 30, "projections": {"asOfSeq": 3, "values": {"title": "Acceptance C"}}}),
        ]);
        guard.archived_ids = vec!["b".to_owned()];
    }
    let rt = tokio::runtime::Runtime::new().unwrap();
    let runtime = rt.block_on(async {
        let runtime = Arc::new(SessionRuntime::spawn(
            "error-scope",
            Arc::new(endpoint.clone()),
        ));
        runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
        runtime
    });
    let controller =
        rt.block_on(async { Arc::new(RunController::new(Arc::clone(&runtime)).unwrap()) });
    let workbench = SharedAgentWorkbench::new();
    let binding = {
        let _guard = rt.enter();
        RunWorkbenchBinding::new(
            "error-scope",
            Arc::clone(&runtime),
            controller,
            rt.handle().clone(),
            workbench.wake_hub(),
        )
    };
    binding.open_initial_session("a").unwrap();
    workbench.attach_run_binding(Arc::clone(&binding));

    let main = AgentRunContainer::build("run", workbench.clone()).unwrap();
    let sidebar = SessionSidebarContainer::build("sidebar", workbench).unwrap();
    let _app = AppWindow::from_containers(
        vec![sidebar.into(), main.into()],
        PresentationOptions::new("run").nav_bar("sidebar"),
    )
    .unwrap();

    let mut host = ContentHost::new(SidebarSurface::build_content()).unwrap();
    host.set_window_active(true);
    host.set_action_events(true);
    SidebarSurface::bind_context_menu(&mut host);
    let mut surface = SidebarSurface::new();
    let mut text = TestTextBackend::new();
    let rect = Rect::new(0.0, 0.0, 320.0, 640.0);

    let sync_sidebar =
        |host: &mut ContentHost, surface: &mut SidebarSurface, text: &mut TestTextBackend| {
            let rows: Vec<SidebarRowModel> = binding
                .sidebar_rows()
                .into_iter()
                .map(|r| SidebarRowModel {
                    session_id: r.session_id,
                    title: r.title,
                    folder: r.folder,
                    activity: r.activity,
                    running: r.running,
                    pinned: r.pinned,
                    archived: r.archived,
                })
                .collect();
            let selected = binding.current_scope().map(|s| s.session_id);
            surface.sync(
                host,
                &rows,
                selected.as_deref(),
                SidebarFilterSelection::All,
                ResponsiveWidthMode::Expanded,
            );
            host.render(
                &mut Scene::new(),
                text,
                rect,
                rect,
                Palette::from(Theme::LIGHT),
            )
            .unwrap();
        };

    let row_center = |host: &ContentHost, label: &str| -> Point {
        let frame = host
            .get(SIDEBAR_LIST_ID)
            .and_then(|n| n.component::<KeyedSelectableListContent>())
            .unwrap()
            .semantic_parts()
            .into_iter()
            .find(|p| p.semantics.name == label)
            .unwrap_or_else(|| panic!("missing row `{label}`"))
            .frame;
        Point::new(
            frame.origin.x + frame.size.width * 0.5,
            frame.origin.y + frame.size.height * 0.5,
        )
    };

    let dispatch_row_menu = |host: &mut ContentHost,
                             surface: &mut SidebarSurface,
                             text: &mut TestTextBackend,
                             row_label: &str,
                             expected_sid: &str,
                             item_label: &str|
     -> Vec<SidebarIntent> {
        let point = row_center(host, row_label);
        let resp = host
            .event_with_text(
                &ContentEvent::Pointer(PointerEvent {
                    position: point,
                    phase: PointerPhase::Down,
                    button: Some(PointerButton::Secondary),
                    modifiers: PointerModifiers::default(),
                    click_count: 1,
                }),
                text,
            )
            .unwrap();
        assert!(
            resp.consumed,
            "secondary click on `{row_label}` must consume"
        );
        host.render(
            &mut Scene::new(),
            text,
            rect,
            rect,
            Palette::from(Theme::LIGHT),
        )
        .unwrap();
        let menu_id = surface
            .open_context_menu_id(host)
            .expect("row context menu must open");
        assert_eq!(Some(menu_id), surface.row_context_menu_id(expected_sid));
        let part_id = host
            .get(menu_id)
            .and_then(|c| c.component::<MenuContent>())
            .unwrap()
            .semantic_parts()
            .into_iter()
            .find(|p| p.semantics.role == ContentRole::MenuItem && p.semantics.name == item_label)
            .unwrap_or_else(|| panic!("expected menu item `{item_label}` on `{expected_sid}`"))
            .index;
        let pick_resp =
            host.accessibility_action(menu_id, Some(part_id), AccessibilityAction::Press);
        assert!(pick_resp.activated);
        let records = host.drain_action_records();
        let selected = binding.current_scope().map(|s| s.session_id);
        surface.handle_actions(host, selected.as_deref(), &records)
    };

    sync_sidebar(&mut host, &mut surface, &mut text);
    assert_eq!(binding.current_scope().unwrap().session_id, "a");

    // 1. While A is selected, right-clicking archived B opens B's menu ("Unarchive") and dispatches to B.
    let intents = dispatch_row_menu(
        &mut host,
        &mut surface,
        &mut text,
        "Archived B (Archived)",
        "b",
        "Unarchive",
    );
    assert_eq!(
        intents,
        vec![SidebarIntent::UnarchiveSession("b".to_owned())]
    );
    assert_eq!(binding.current_scope().unwrap().session_id, "a");
    let rpc_start = endpoint.inner.lock().unwrap().rpcs.len();
    binding.unarchive_session("b");
    for _ in 0..80 {
        if endpoint.inner.lock().unwrap().rpcs[rpc_start..]
            .iter()
            .any(|(m, p)| m == "session.unarchive" && p["sessionId"] == "b")
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    rt.block_on(runtime.reconnect()).unwrap();
    sync_sidebar(&mut host, &mut surface, &mut text);

    // 2. While A is selected, right-clicking active C -> Archive targets C, NOT A.
    let rpc_start_c = endpoint.inner.lock().unwrap().rpcs.len();
    let intents_c = dispatch_row_menu(
        &mut host,
        &mut surface,
        &mut text,
        "Acceptance C",
        "c",
        "Archive",
    );
    assert_eq!(
        intents_c,
        vec![SidebarIntent::ArchiveSession("c".to_owned())]
    );
    for intent in intents_c {
        if let SidebarIntent::ArchiveSession(sid) = intent {
            binding.archive_session(&sid);
        }
    }
    for _ in 0..80 {
        if endpoint.inner.lock().unwrap().rpcs[rpc_start_c..]
            .iter()
            .any(|(m, p)| m == "session.archive" && p["sessionId"] == "c")
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let c_rpcs = endpoint.inner.lock().unwrap().rpcs[rpc_start_c..].to_vec();
    assert!(
        c_rpcs
            .iter()
            .any(|(m, p)| m == "session.archive" && p["sessionId"] == "c")
    );
    assert!(
        !c_rpcs
            .iter()
            .any(|(m, p)| m == "session.archive" && p["sessionId"] == "a")
    );
    assert_eq!(binding.current_scope().unwrap().session_id, "a");
    rt.block_on(runtime.reconnect()).unwrap();
    sync_sidebar(&mut host, &mut surface, &mut text);

    // 3. While A is selected, Pin / Unpin and Rename on visible B target B while keeping A selected.
    let pin_intents = dispatch_row_menu(
        &mut host,
        &mut surface,
        &mut text,
        "Archived B (Archived)",
        "b",
        "Pin / Unpin",
    );
    assert_eq!(pin_intents, vec![SidebarIntent::TogglePin("b".to_owned())]);
    assert_eq!(binding.current_scope().unwrap().session_id, "a");

    let rename_open = dispatch_row_menu(
        &mut host,
        &mut surface,
        &mut text,
        "Archived B (Archived)",
        "b",
        "Rename",
    );
    assert!(rename_open.is_empty());
    host.render(
        &mut Scene::new(),
        &mut text,
        rect,
        rect,
        Palette::from(Theme::LIGHT),
    )
    .unwrap();
    host.get_mut(SIDEBAR_RENAME_INPUT_ID)
        .and_then(|n| n.component_mut::<tekes_ui_controls::composition::TextFieldContent>())
        .unwrap()
        .insert_text("B Renamed Off Selection", None);
    let save_resp =
        host.accessibility_action(SIDEBAR_RENAME_BTN_ID, None, AccessibilityAction::Press);
    assert!(save_resp.activated);
    let save_records = host.drain_action_records();
    let rename_intents = surface.handle_actions(&mut host, Some("a"), &save_records);
    assert_eq!(
        rename_intents,
        vec![SidebarIntent::RenameSession {
            session_id: "b".to_owned(),
            title: "B Renamed Off Selection".to_owned(),
        }]
    );
    assert_eq!(binding.current_scope().unwrap().session_id, "a");

    // 4. Empty space right-click inside SIDEBAR_LIST_ID does NOT target selected A.
    let list_frame = host.get(SIDEBAR_LIST_ID).unwrap().frame();
    let empty_point = Point::new(
        list_frame.origin.x + list_frame.size.width * 0.5,
        list_frame.origin.y + list_frame.size.height - 10.0,
    );
    let _ = host.event_with_text(
        &ContentEvent::Pointer(PointerEvent {
            position: empty_point,
            phase: PointerPhase::Down,
            button: Some(PointerButton::Secondary),
            modifiers: PointerModifiers::default(),
            click_count: 1,
        }),
        &mut text,
    );
    host.render(
        &mut Scene::new(),
        &mut text,
        rect,
        rect,
        Palette::from(Theme::LIGHT),
    )
    .unwrap();
    if host
        .get(SIDEBAR_CONTEXT_MENU_ID)
        .and_then(|c| c.component::<MenuContent>())
        .is_some_and(MenuContent::is_open)
    {
        let part_id = host
            .get(SIDEBAR_CONTEXT_MENU_ID)
            .and_then(|c| c.component::<MenuContent>())
            .unwrap()
            .semantic_parts()
            .into_iter()
            .find(|p| p.semantics.role == ContentRole::MenuItem && p.semantics.name == "Archive")
            .unwrap()
            .index;
        let _ = host.accessibility_action(
            SIDEBAR_CONTEXT_MENU_ID,
            Some(part_id),
            AccessibilityAction::Press,
        );
        let empty_records = host.drain_action_records();
        let empty_intents = surface.handle_actions(&mut host, Some("a"), &empty_records);
        assert!(
            empty_intents.is_empty(),
            "empty-space pointer menu must not hit A: {empty_intents:?}"
        );
    }

    // 5. If clicked row B disappears after menu/rename opened, action cancels (never falls back to A).
    let open_rename = dispatch_row_menu(
        &mut host,
        &mut surface,
        &mut text,
        "Archived B (Archived)",
        "b",
        "Rename",
    );
    assert!(open_rename.is_empty());
    host.get_mut(SIDEBAR_RENAME_INPUT_ID)
        .and_then(|n| n.component_mut::<tekes_ui_controls::composition::TextFieldContent>())
        .unwrap()
        .insert_text("Should Cancel", None);
    let only_a = vec![SidebarRowModel {
        session_id: "a".to_owned(),
        title: "Acceptance A".to_owned(),
        folder: "默认".to_owned(),
        activity: "idle".to_owned(),
        running: false,
        pinned: false,
        archived: false,
    }];
    surface.sync(
        &mut host,
        &only_a,
        Some("a"),
        SidebarFilterSelection::All,
        ResponsiveWidthMode::Expanded,
    );
    assert!(host.get(SIDEBAR_RENAME_INPUT_ID).is_none());
    assert!(surface.row_context_menu_id("b").is_none());

    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn list_context_menu_pins_or_retires_target_across_accessibility_keyboard_and_pointer_flows() {
    let endpoint = ErrorScopeEndpoint::new();
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.custom_items = Some(vec![
            json!({"sessionId": "a", "running": false, "blank": false, "updatedAt": 30, "projections": {"asOfSeq": 1, "values": {"title": "Acceptance A"}}}),
            json!({"sessionId": "b", "running": false, "blank": false, "updatedAt": 20, "projections": {"asOfSeq": 1, "values": {"title": "Acceptance B"}}}),
            json!({"sessionId": "c", "running": false, "blank": false, "updatedAt": 10, "projections": {"asOfSeq": 1, "values": {"title": "Acceptance C"}}}),
        ]);
    }
    let rt = tokio::runtime::Runtime::new().unwrap();
    let runtime = rt.block_on(async {
        let runtime = Arc::new(SessionRuntime::spawn(
            "error-scope",
            Arc::new(endpoint.clone()),
        ));
        runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
        runtime
    });
    let controller =
        rt.block_on(async { Arc::new(RunController::new(Arc::clone(&runtime)).unwrap()) });
    let workbench = SharedAgentWorkbench::new();
    let binding = {
        let _guard = rt.enter();
        RunWorkbenchBinding::new(
            "error-scope",
            Arc::clone(&runtime),
            controller,
            rt.handle().clone(),
            workbench.wake_hub(),
        )
    };
    binding.open_initial_session("a").unwrap();
    workbench.attach_run_binding(Arc::clone(&binding));

    let main = AgentRunContainer::build("run", workbench.clone()).unwrap();
    let sidebar = SessionSidebarContainer::build("sidebar", workbench).unwrap();
    let _app = AppWindow::from_containers(
        vec![sidebar.into(), main.into()],
        PresentationOptions::new("run").nav_bar("sidebar"),
    )
    .unwrap();

    let mut host = ContentHost::new(SidebarSurface::build_content()).unwrap();
    host.set_window_active(true);
    host.set_action_events(true);
    let mut surface = SidebarSurface::new();
    let mut text = TestTextBackend::new();
    let rect = Rect::new(0.0, 0.0, 320.0, 640.0);

    let sync_sidebar =
        |host: &mut ContentHost, surface: &mut SidebarSurface, text: &mut TestTextBackend| {
            let rows: Vec<SidebarRowModel> = binding
                .sidebar_rows()
                .into_iter()
                .map(|r| SidebarRowModel {
                    session_id: r.session_id,
                    title: r.title,
                    folder: r.folder,
                    activity: r.activity,
                    running: r.running,
                    pinned: r.pinned,
                    archived: r.archived,
                })
                .collect();
            let selected = binding.current_scope().map(|s| s.session_id);
            surface.sync(
                host,
                &rows,
                selected.as_deref(),
                SidebarFilterSelection::All,
                ResponsiveWidthMode::Expanded,
            );
            host.render(
                &mut Scene::new(),
                text,
                rect,
                rect,
                Palette::from(Theme::LIGHT),
            )
            .unwrap();
        };

    let trigger_archive_item =
        |host: &mut ContentHost,
         surface: &mut SidebarSurface,
         menu_id: tekes_ui_controls::composition::ComponentId| {
            let part_id = host
                .get(menu_id)
                .and_then(|c| c.component::<MenuContent>())
                .unwrap()
                .semantic_parts()
                .into_iter()
                .find(|p| {
                    p.semantics.role == ContentRole::MenuItem && p.semantics.name == "Archive"
                })
                .unwrap()
                .index;
            let resp =
                host.accessibility_action(menu_id, Some(part_id), AccessibilityAction::Press);
            assert!(resp.activated);
            let records = host.drain_action_records();
            let selected = binding.current_scope().map(|s| s.session_id);
            surface.handle_actions(host, selected.as_deref(), &records)
        };

    // Match SessionSidebarContainer order: sync first, bind_context_menu after.
    sync_sidebar(&mut host, &mut surface, &mut text);
    SidebarSurface::bind_context_menu(&mut host);

    // 1. Accessibility ShowMenu on A without list focus -> switch to C + sync retires stale menu and NEVER archives C.
    let list_keys = host
        .get(SIDEBAR_LIST_ID)
        .and_then(|c| c.component::<KeyedSelectableListContent>())
        .map(|l| (l.focused_key(), l.selected_key()))
        .unwrap();
    assert!(list_keys.0.is_none() && list_keys.1.is_some());
    assert!(
        host.accessibility_action(SIDEBAR_LIST_ID, None, AccessibilityAction::ShowMenu)
            .consumed
    );
    host.render(
        &mut Scene::new(),
        &mut text,
        rect,
        rect,
        Palette::from(Theme::LIGHT),
    )
    .unwrap();
    assert_eq!(
        surface.open_context_menu_id(&host),
        Some(SIDEBAR_CONTEXT_MENU_ID)
    );
    binding.select_session("c");
    assert_eq!(binding.current_scope().unwrap().session_id, "c");
    sync_sidebar(&mut host, &mut surface, &mut text);
    assert_eq!(surface.open_context_menu_id(&host), None);
    let _ = host.drain_action_records();

    // Subsequent ShowMenu on C still opens cleanly and targets C.
    assert!(
        host.accessibility_action(SIDEBAR_LIST_ID, None, AccessibilityAction::ShowMenu)
            .consumed
    );
    host.render(
        &mut Scene::new(),
        &mut text,
        rect,
        rect,
        Palette::from(Theme::LIGHT),
    )
    .unwrap();
    assert_eq!(
        surface.open_context_menu_id(&host),
        Some(SIDEBAR_CONTEXT_MENU_ID)
    );
    let c_intents = trigger_archive_item(&mut host, &mut surface, SIDEBAR_CONTEXT_MENU_ID);
    assert_eq!(
        c_intents,
        vec![SidebarIntent::ArchiveSession("c".to_owned())]
    );

    // 2. Stable ShowMenu on A (selection stays A) archives A.
    binding.select_session("a");
    sync_sidebar(&mut host, &mut surface, &mut text);
    assert!(
        host.accessibility_action(SIDEBAR_LIST_ID, None, AccessibilityAction::ShowMenu)
            .consumed
    );
    host.render(
        &mut Scene::new(),
        &mut text,
        rect,
        rect,
        Palette::from(Theme::LIGHT),
    )
    .unwrap();
    let stable_intents = trigger_archive_item(&mut host, &mut surface, SIDEBAR_CONTEXT_MENU_ID);
    assert_eq!(
        stable_intents,
        vec![SidebarIntent::ArchiveSession("a".to_owned())]
    );

    // 3. Keyboard Shift+F10 on focused row A -> switch selection to C + sync keeps focus on A -> archives A.
    sync_sidebar(&mut host, &mut surface, &mut text);
    let _ = host.focus(Some(SIDEBAR_LIST_ID), true);
    let shift_f10 = ContentKeyEvent {
        key: None,
        down: true,
        repeat: false,
        modifiers: InputModifiers {
            shift: true,
            ..Default::default()
        },
        native_code: 109,
        characters: "\u{f70d}".into(),
        characters_ignoring_modifiers: "\u{f70d}".into(),
    };
    assert!(host.event(&ContentEvent::KeyInput(shift_f10)).consumed);
    host.render(
        &mut Scene::new(),
        &mut text,
        rect,
        rect,
        Palette::from(Theme::LIGHT),
    )
    .unwrap();
    assert_eq!(
        surface.open_context_menu_id(&host),
        Some(SIDEBAR_CONTEXT_MENU_ID)
    );
    binding.select_session("c");
    sync_sidebar(&mut host, &mut surface, &mut text);
    assert_eq!(
        surface.open_context_menu_id(&host),
        Some(SIDEBAR_CONTEXT_MENU_ID)
    );
    let kbd_intents = trigger_archive_item(&mut host, &mut surface, SIDEBAR_CONTEXT_MENU_ID);
    assert_eq!(
        kbd_intents,
        vec![SidebarIntent::ArchiveSession("a".to_owned())]
    );

    // 4. Pointer right-click on non-current row B -> switch selection to C + sync -> archives B.
    let frame_b = host
        .get(SIDEBAR_LIST_ID)
        .and_then(|n| n.component::<KeyedSelectableListContent>())
        .unwrap()
        .semantic_parts()
        .into_iter()
        .find(|p| p.semantics.name == "Acceptance B")
        .unwrap()
        .frame;
    let point_b = Point::new(
        frame_b.origin.x + frame_b.size.width * 0.5,
        frame_b.origin.y + frame_b.size.height * 0.5,
    );
    assert!(
        host.event(&ContentEvent::Pointer(PointerEvent {
            position: point_b,
            phase: PointerPhase::Down,
            button: Some(PointerButton::Secondary),
            modifiers: PointerModifiers::default(),
            click_count: 1,
        }))
        .consumed
    );
    host.render(
        &mut Scene::new(),
        &mut text,
        rect,
        rect,
        Palette::from(Theme::LIGHT),
    )
    .unwrap();
    let row_b_menu = surface.row_context_menu_id("b").unwrap();
    assert_eq!(surface.open_context_menu_id(&host), Some(row_b_menu));
    binding.select_session("c");
    sync_sidebar(&mut host, &mut surface, &mut text);
    let ptr_intents = trigger_archive_item(&mut host, &mut surface, row_b_menu);
    assert_eq!(
        ptr_intents,
        vec![SidebarIntent::ArchiveSession("b".to_owned())]
    );

    assert!(
        !endpoint
            .inner
            .lock()
            .unwrap()
            .rpcs
            .iter()
            .any(|(m, _)| m == "session.archive")
    );
    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn list_context_menu_dismiss_refocus_reopen_targets_new_focused_row_after_background_sync() {
    let endpoint = ErrorScopeEndpoint::new();
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.custom_items = Some(vec![
            json!({"sessionId": "a", "running": false, "blank": false, "updatedAt": 30, "projections": {"asOfSeq": 1, "values": {"title": "Acceptance A"}}}),
            json!({"sessionId": "b", "running": false, "blank": false, "updatedAt": 20, "projections": {"asOfSeq": 1, "values": {"title": "Acceptance B"}}}),
            json!({"sessionId": "c", "running": false, "blank": false, "updatedAt": 10, "projections": {"asOfSeq": 1, "values": {"title": "Acceptance C"}}}),
        ]);
    }
    let rt = tokio::runtime::Runtime::new().unwrap();
    let runtime = rt.block_on(async {
        let runtime = Arc::new(SessionRuntime::spawn(
            "error-scope",
            Arc::new(endpoint.clone()),
        ));
        runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
        runtime
    });
    let controller =
        rt.block_on(async { Arc::new(RunController::new(Arc::clone(&runtime)).unwrap()) });
    let workbench = SharedAgentWorkbench::new();
    let binding = {
        let _guard = rt.enter();
        RunWorkbenchBinding::new(
            "error-scope",
            Arc::clone(&runtime),
            controller,
            rt.handle().clone(),
            workbench.wake_hub(),
        )
    };
    binding.open_initial_session("a").unwrap();
    workbench.attach_run_binding(Arc::clone(&binding));

    let main = AgentRunContainer::build("run", workbench.clone()).unwrap();
    let sidebar = SessionSidebarContainer::build("sidebar", workbench.clone()).unwrap();
    let _app = AppWindow::from_containers(
        vec![sidebar.into(), main.into()],
        PresentationOptions::new("run").nav_bar("sidebar"),
    )
    .unwrap();

    let mut host = ContentHost::new(SidebarSurface::build_content()).unwrap();
    host.set_window_active(true);
    host.set_action_events(true);
    let mut surface = SidebarSurface::new();
    let mut text = TestTextBackend::new();
    let rect = Rect::new(0.0, 0.0, 320.0, 640.0);

    let sync_sidebar =
        |host: &mut ContentHost, surface: &mut SidebarSurface, text: &mut TestTextBackend| {
            let rows: Vec<SidebarRowModel> = binding
                .sidebar_rows()
                .into_iter()
                .map(|r| SidebarRowModel {
                    session_id: r.session_id,
                    title: r.title,
                    folder: r.folder,
                    activity: r.activity,
                    running: r.running,
                    pinned: r.pinned,
                    archived: r.archived,
                })
                .collect();
            let selected = binding.current_scope().map(|s| s.session_id);
            surface.sync(
                host,
                &rows,
                selected.as_deref(),
                SidebarFilterSelection::All,
                ResponsiveWidthMode::Expanded,
            );
            host.render(
                &mut Scene::new(),
                text,
                rect,
                rect,
                Palette::from(Theme::LIGHT),
            )
            .unwrap();
        };

    sync_sidebar(&mut host, &mut surface, &mut text);
    SidebarSurface::bind_context_menu(&mut host);

    // 1. Open list menu #1 on selected A (no focused row).
    assert!(
        host.accessibility_action(SIDEBAR_LIST_ID, None, AccessibilityAction::ShowMenu)
            .consumed
    );
    host.render(
        &mut Scene::new(),
        &mut text,
        rect,
        rect,
        Palette::from(Theme::LIGHT),
    )
    .unwrap();
    assert_eq!(
        surface.open_context_menu_id(&host),
        Some(SIDEBAR_CONTEXT_MENU_ID)
    );
    let first_archive_idx = host
        .get(SIDEBAR_CONTEXT_MENU_ID)
        .and_then(|c| c.component::<MenuContent>())
        .unwrap()
        .semantic_parts()
        .into_iter()
        .find(|p| p.semantics.role == ContentRole::MenuItem && p.semantics.name == "Archive")
        .unwrap()
        .index;

    // 2. Publish unrelated workbench background note -> advances revision -> sync with menu #1 open on A.
    let prev_rev = workbench.sync_background_revision();
    let sender = workbench.publish_background_note_sender();
    sender.publish("Unrelated background note");
    assert!(workbench.sync_background_revision() > prev_rev);
    sync_sidebar(&mut host, &mut surface, &mut text);
    assert_eq!(
        surface.open_context_menu_id(&host),
        Some(SIDEBAR_CONTEXT_MENU_ID)
    );

    // 3. Dismiss menu #1 via Escape WITHOUT syncing sidebar afterward.
    let _ = host.drain_action_records();
    let esc = ContentEvent::KeyInput(ContentKeyEvent {
        key: Some(ControlKey::Escape),
        down: true,
        repeat: false,
        modifiers: InputModifiers::default(),
        native_code: 53,
        characters: "\u{1b}".into(),
        characters_ignoring_modifiers: "\u{1b}".into(),
    });
    assert!(host.event(&esc).consumed);
    assert_eq!(surface.open_context_menu_id(&host), None);
    let esc_records = host.drain_action_records();
    assert!(
        surface
            .handle_actions(&mut host, Some("a"), &esc_records)
            .is_empty()
    );

    // 4. Move AX focus ONLY to C while binding selection remains A (no sync in between).
    let c_item_idx = host
        .get(SIDEBAR_LIST_ID)
        .and_then(|c| c.component::<KeyedSelectableListContent>())
        .unwrap()
        .semantic_parts()
        .into_iter()
        .find(|p| p.semantics.name == "Acceptance C")
        .unwrap()
        .index;
    let _ = host.accessibility_action(
        SIDEBAR_LIST_ID,
        Some(c_item_idx),
        AccessibilityAction::Focus,
    );
    let focus_records = host.drain_action_records();
    assert!(
        surface
            .handle_actions(&mut host, Some("a"), &focus_records)
            .is_empty()
    );
    assert_eq!(binding.current_scope().unwrap().session_id, "a");

    // 5. Reopen list menu #2, verify fresh Archive token != first_archive_idx, also test sync while menu #2 is open!
    assert!(
        host.accessibility_action(SIDEBAR_LIST_ID, None, AccessibilityAction::ShowMenu)
            .consumed
    );
    host.render(
        &mut Scene::new(),
        &mut text,
        rect,
        rect,
        Palette::from(Theme::LIGHT),
    )
    .unwrap();
    assert_eq!(
        surface.open_context_menu_id(&host),
        Some(SIDEBAR_CONTEXT_MENU_ID)
    );
    let next_archive_idx = host
        .get(SIDEBAR_CONTEXT_MENU_ID)
        .and_then(|c| c.component::<MenuContent>())
        .unwrap()
        .semantic_parts()
        .into_iter()
        .find(|p| p.semantics.role == ContentRole::MenuItem && p.semantics.name == "Archive")
        .unwrap()
        .index;
    assert_ne!(first_archive_idx, next_archive_idx);

    // Even if another background sync lands while menu #2 is open on focused C, menu #2 stays open and keeps C.
    sender.publish("Second background note");
    sync_sidebar(&mut host, &mut surface, &mut text);
    assert_eq!(
        surface.open_context_menu_id(&host),
        Some(SIDEBAR_CONTEXT_MENU_ID)
    );

    // Stale first token must never activate anything.
    let stale = host.accessibility_action(
        SIDEBAR_CONTEXT_MENU_ID,
        Some(first_archive_idx),
        AccessibilityAction::Press,
    );
    assert!(!stale.activated);
    let stale_records = host.drain_action_records();
    assert!(
        surface
            .handle_actions(&mut host, Some("a"), &stale_records)
            .is_empty()
    );

    // Fresh token activates Archive for C ONLY (NOT A!).
    let press = host.accessibility_action(
        SIDEBAR_CONTEXT_MENU_ID,
        Some(next_archive_idx),
        AccessibilityAction::Press,
    );
    assert!(press.activated);
    let press_records = host.drain_action_records();
    let intents = surface.handle_actions(&mut host, Some("a"), &press_records);
    assert_eq!(intents, vec![SidebarIntent::ArchiveSession("c".to_owned())]);

    // 6. Verify removing focused/selected target under filter retires open menu.
    assert!(
        host.accessibility_action(SIDEBAR_LIST_ID, None, AccessibilityAction::ShowMenu)
            .consumed
    );
    host.render(
        &mut Scene::new(),
        &mut text,
        rect,
        rect,
        Palette::from(Theme::LIGHT),
    )
    .unwrap();
    assert_eq!(
        surface.open_context_menu_id(&host),
        Some(SIDEBAR_CONTEXT_MENU_ID)
    );
    let only_a = vec![SidebarRowModel {
        session_id: "a".to_owned(),
        title: "Acceptance A".to_owned(),
        folder: "默认".to_owned(),
        activity: "idle".to_owned(),
        running: false,
        pinned: false,
        archived: false,
    }];
    surface.sync(
        &mut host,
        &only_a,
        Some("a"),
        SidebarFilterSelection::Pinned,
        ResponsiveWidthMode::Expanded,
    );
    assert_eq!(surface.open_context_menu_id(&host), None);

    rt.block_on(runtime.stop()).unwrap();
}

#[test]
fn list_context_menu_same_open_refocus_preserves_original_target_and_bridge_sync_retires() {
    let endpoint = ErrorScopeEndpoint::new();
    {
        let mut guard = endpoint.inner.lock().unwrap();
        guard.custom_items = Some(vec![
            json!({"sessionId": "a", "running": false, "blank": false, "updatedAt": 30, "projections": {"asOfSeq": 1, "values": {"title": "Acceptance A"}}}),
            json!({"sessionId": "b", "running": false, "blank": false, "updatedAt": 20, "projections": {"asOfSeq": 1, "values": {"title": "Acceptance B"}}}),
            json!({"sessionId": "c", "running": false, "blank": false, "updatedAt": 10, "projections": {"asOfSeq": 1, "values": {"title": "Acceptance C"}}}),
        ]);
    }
    let rt = tokio::runtime::Runtime::new().unwrap();
    let runtime = rt.block_on(async {
        let runtime = Arc::new(SessionRuntime::spawn(
            "error-scope",
            Arc::new(endpoint.clone()),
        ));
        runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
        runtime
    });
    let controller =
        rt.block_on(async { Arc::new(RunController::new(Arc::clone(&runtime)).unwrap()) });
    let workbench = SharedAgentWorkbench::new();
    let binding = {
        let _guard = rt.enter();
        RunWorkbenchBinding::new(
            "error-scope",
            Arc::clone(&runtime),
            controller,
            rt.handle().clone(),
            workbench.wake_hub(),
        )
    };
    binding.open_initial_session("a").unwrap();
    workbench.attach_run_binding(Arc::clone(&binding));

    let main = AgentRunContainer::build("run", workbench.clone()).unwrap();
    let sidebar = SessionSidebarContainer::build("sidebar", workbench.clone()).unwrap();
    let _app = AppWindow::from_containers(
        vec![sidebar.into(), main.into()],
        PresentationOptions::new("run").nav_bar("sidebar"),
    )
    .unwrap();

    for with_background_sync in [false, true] {
        let mut host = ContentHost::new(SidebarSurface::build_content()).unwrap();
        host.set_window_active(true);
        host.set_action_events(true);
        let mut surface = SidebarSurface::new();
        let mut text = TestTextBackend::new();
        let rect = Rect::new(0.0, 0.0, 320.0, 640.0);

        let sync_sidebar =
            |host: &mut ContentHost, surface: &mut SidebarSurface, text: &mut TestTextBackend| {
                let rows: Vec<SidebarRowModel> = binding
                    .sidebar_rows()
                    .into_iter()
                    .map(|r| SidebarRowModel {
                        session_id: r.session_id,
                        title: r.title,
                        folder: r.folder,
                        activity: r.activity,
                        running: r.running,
                        pinned: r.pinned,
                        archived: r.archived,
                    })
                    .collect();
                let selected = binding.current_scope().map(|s| s.session_id);
                surface.sync(
                    host,
                    &rows,
                    selected.as_deref(),
                    SidebarFilterSelection::All,
                    ResponsiveWidthMode::Expanded,
                );
                host.render(
                    &mut Scene::new(),
                    text,
                    rect,
                    rect,
                    Palette::from(Theme::LIGHT),
                )
                .unwrap();
            };

        sync_sidebar(&mut host, &mut surface, &mut text);
        SidebarSurface::bind_context_menu(&mut host);

        // Open menu for selected A, dispatch initial actions before first render too.
        assert!(
            host.accessibility_action(SIDEBAR_LIST_ID, None, AccessibilityAction::ShowMenu)
                .consumed
        );
        let open_records = host.drain_action_records();
        assert!(
            surface
                .handle_actions(&mut host, Some("a"), &open_records)
                .is_empty()
        );
        host.render(
            &mut Scene::new(),
            &mut text,
            rect,
            rect,
            Palette::from(Theme::LIGHT),
        )
        .unwrap();
        assert_eq!(
            surface.open_context_menu_id(&host),
            Some(SIDEBAR_CONTEXT_MENU_ID)
        );
        let first_archive_idx = host
            .get(SIDEBAR_CONTEXT_MENU_ID)
            .and_then(|c| c.component::<MenuContent>())
            .unwrap()
            .semantic_parts()
            .into_iter()
            .find(|p| p.semantics.role == ContentRole::MenuItem && p.semantics.name == "Archive")
            .unwrap()
            .index;

        if with_background_sync {
            workbench
                .publish_background_note_sender()
                .publish("Background note during open A menu");
            sync_sidebar(&mut host, &mut surface, &mut text);
            assert_eq!(
                surface.open_context_menu_id(&host),
                Some(SIDEBAR_CONTEXT_MENU_ID)
            );
        }

        // Move AX focus ONLY to C while the same menu remains open (no sync afterwards).
        let c_item_idx = host
            .get(SIDEBAR_LIST_ID)
            .and_then(|c| c.component::<KeyedSelectableListContent>())
            .unwrap()
            .semantic_parts()
            .into_iter()
            .find(|p| p.semantics.name == "Acceptance C")
            .unwrap()
            .index;
        let focus_resp = host.accessibility_action(
            SIDEBAR_LIST_ID,
            Some(c_item_idx),
            AccessibilityAction::Focus,
        );
        assert!(focus_resp.consumed && focus_resp.changed);
        let focus_records = host.drain_action_records();
        assert!(
            surface
                .handle_actions(&mut host, Some("a"), &focus_records)
                .is_empty()
        );
        host.render(
            &mut Scene::new(),
            &mut text,
            rect,
            rect,
            Palette::from(Theme::LIGHT),
        )
        .unwrap();
        assert_eq!(
            surface.open_context_menu_id(&host),
            Some(SIDEBAR_CONTEXT_MENU_ID)
        );

        // Activating the still-open menu's original Archive item MUST target A, NEVER C!
        let press = host.accessibility_action(
            SIDEBAR_CONTEXT_MENU_ID,
            Some(first_archive_idx),
            AccessibilityAction::Press,
        );
        assert!(press.activated);
        let press_records = host.drain_action_records();
        let intents = surface.handle_actions(&mut host, Some("a"), &press_records);
        assert_eq!(intents, vec![SidebarIntent::ArchiveSession("a".to_owned())]);

        // If sync runs after AX refocus to C while A's menu is open, the stale menu must retire.
        assert!(
            host.accessibility_action(SIDEBAR_LIST_ID, None, AccessibilityAction::ShowMenu)
                .consumed
        );
        let reopen_records = host.drain_action_records();
        assert!(
            surface
                .handle_actions(&mut host, Some("a"), &reopen_records)
                .is_empty()
        );
        let b_item_idx = host
            .get(SIDEBAR_LIST_ID)
            .and_then(|c| c.component::<KeyedSelectableListContent>())
            .unwrap()
            .semantic_parts()
            .into_iter()
            .find(|p| p.semantics.name == "Acceptance B")
            .unwrap()
            .index;
        let _ = host.accessibility_action(
            SIDEBAR_LIST_ID,
            Some(b_item_idx),
            AccessibilityAction::Focus,
        );
        sync_sidebar(&mut host, &mut surface, &mut text);
        assert_eq!(surface.open_context_menu_id(&host), None);
    }

    rt.block_on(runtime.stop()).unwrap();
}
