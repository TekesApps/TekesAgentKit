//! Native host of the actual public Composer/Indicators and RunBinding.
//! This is a component integration window, not the final RunView or Transcript.
use super::native::{TextBackend, input_event, publish_content, window_renderer};
use std::{
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
use tekes_agent_components_ui::{SidePanel, Sidebar, SidebarIntent};
use tekes_agent_contract::{Result, require};
use tekes_agent_presentation::{PreparedSubmission, RunScope, SubmissionPreparation};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{
    BoundSideChatRun, RunBinding, SideChatBinding, SidebarActionOutcome, SidebarBinding,
};
use tekes_agent_ui_demo::component_demo::{
    ComponentDemo, DemoCommand, NavigationRequest, PanelRequest, RECONNECT,
};
use tekes_agent_web::SharedAgent;
use tekes_ui_controls::composition::{AccessibilityAction, ComponentId, ContentEvent};
use tekes_ui_platform_macos::{
    App, AppConfig, Event, MenuBar, Size, Window, WindowConfig, WindowHandler,
};
use tekes_ui_render::Renderer;
use tekes_ui_scene::{GlyphAtlas, Rect, Scene};
use tekes_ui_text::coretext::{CoreTextFonts, CoreTextRasterizer, CoreTextShaper};
use tekes_ui_tokens::{Palette, Theme};

struct NoSettings;
#[async_trait::async_trait]
impl SubmissionPreparation for NoSettings {
    async fn prepare(
        &self,
        _: &SessionRuntime,
        ticket: &PreparedSubmission,
        _: &RunScope,
    ) -> Result<()> {
        require(
            ticket.settings().is_empty(),
            "demo has no host settings adapter",
        )
    }
}
struct Host {
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
    failures: Vec<String>,
    focus_before_deactivate: Option<ComponentId>,
    _owned_preferences: Option<OwnedPreferencesDirectory>,
    sidebar_error: Option<String>,
    side_chat_test_started: bool,
    side_chat_test_session: Option<String>,
    task_test_requested: bool,
    task_test_started: bool,
}
enum HostResult {
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
    Sidebar(std::result::Result<SidebarActionOutcome, String>),
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
    fn commands(&mut self, result: Result<Vec<DemoCommand>>) {
        let commands = match result {
            Ok(commands) => commands,
            Err(error) => {
                self.failure(error.to_string());
                return;
            }
        };
        for command in commands {
            let sender = self.sender.clone();
            let wake = self.wake.clone();
            let runtime = self.runtime.clone();
            let sidebar = self.sidebar.clone();
            let side_chats = self.side_chats.clone();
            self.executor.spawn(async move {
                let special = match command {
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
                        // Mutation entry UIs beyond local pins are still explicit
                        // host callbacks in this integration window.
                        if !matches!(definition.id.as_str(), "pin" | "unpin" | "copySessionId") {
                            HostResult::Operation(Err(
                                "此验证宿主尚未接入该会话操作的交互；未执行请求".into(),
                            ))
                        } else {
                            let rpc_id = format!(
                                "demo:{}:{}:{}",
                                std::process::id(),
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_nanos(),
                                definition.id
                            );
                            HostResult::Sidebar(
                                sidebar
                                    .perform(&address, &definition, None, &rpc_id)
                                    .await
                                    .map_err(|error| error.to_string()),
                            )
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
                                binding.dispatch_prepared_with(ticket, &NoSettings).await
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
                            DemoCommand::Sidebar(_)
                            | DemoCommand::OpenTask { .. }
                            | DemoCommand::CreateSideChat(_)
                            | DemoCommand::OpenSideChat { .. }
                            | DemoCommand::CloseSideChat(_)
                            | DemoCommand::OpenSession(_)
                            | DemoCommand::Summary(_) => unreachable!("handled above"),
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
        while let Ok(result) = self.results.try_recv() {
            match result {
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
                | HostResult::Sidebar(Err(error))
                | HostResult::Opened {
                    binding: Err(error),
                    ..
                } => self.failure(error),
                HostResult::Sidebar(Ok(SidebarActionOutcome::Copy { text, .. })) => {
                    window.set_clipboard_text(&text);
                }
                HostResult::Sidebar(Ok(SidebarActionOutcome::Mutation { outcome, .. })) => {
                    if let Some(warning) = outcome.warning {
                        self.view.set_notice(warning);
                    }
                }
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
                HostResult::Operation(Ok(())) => {}
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
    }
    fn observe(&mut self, binding: &RunBinding, window: &Window) {
        let wake = window.wake_proxy();
        self.observers.push(
            self.executor
                .spawn(tekes_agent_ui_demo::notifications::forward(
                    binding.subscribe_run().expect("main run"),
                    move || wake.wake() != tekes_ui_platform_macos::WakeResult::Closed,
                )),
        );
    }
    fn close(&mut self, window: &Window) {
        if self.selftest {
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
                if self.selftest {
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
    let executor = tokio::runtime::Runtime::new().unwrap();
    let (connector, kind, scopes, fixture): (
        Arc<dyn tekes_agent_contract::EndpointConnector>,
        _,
        _,
        _,
    ) = if let Some(index) = args.iter().position(|arg| arg == "--endpoint-config") {
        assert!(!selftest, "native selftest uses isolated fixture only");
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
            Arc::new(
                tekes_agent_ui_demo::fixture::Fixture::default()
                    .with_demo_summary()
                    .with_side_chats(),
            ),
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
    let side_chats = Arc::new(SideChatBinding::new(runtime.clone()));
    view.set_side_chat_binding(side_chats.clone());
    view.set_sidebar_source(sidebar.snapshot()).unwrap();
    view.sync().unwrap();
    let fonts = CoreTextFonts::new();
    let (sender, results) = mpsc::channel();
    let mut host = Host {
        view,
        sidebar,
        side_chats,
        _owned_preferences: owned_preferences,
        sidebar_error: None,
        side_chat_test_started: false,
        side_chat_test_session: None,
        task_test_requested: false,
        task_test_started: false,
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
        failures: vec![],
        focus_before_deactivate: None,
    };
    if let Some(index) = args.iter().position(|arg| arg == "--offscreen") {
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
        println!(
            "{}",
            serde_json::json!({
                "offscreen": true, "components": true, "nativeInteraction": false,
                "transcriptView": false, "isStale": snapshot.transcript.is_stale,
                "transcriptItems": snapshot.transcript.items.len(),
                "finalAssistantItems": snapshot.transcript.items.iter().filter(|item| item.kind == tekes_agent_presentation::TranscriptItemKind::Assistant && item.is_final).count(),
            "inventoryRows": host.sidebar.snapshot().rows.len(),
            "sharedRuntime": Arc::ptr_eq(host.sidebar.runtime(), &host.runtime),
            "sidebarMounted": host.view.tree.get(host.view.sidebar_id()).is_some(),
            "summaryMounted": host.view.tree.get(host.view.summary_id()).is_some(),
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
    script.push((2200, Action::Close));
    run_script(script);
}
