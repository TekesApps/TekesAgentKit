// Text bridge follows TekesUI's MIT-licensed public consumer example; no Gallery internals.
use std::{rc::Rc, sync::Arc, time::Instant};
use tekes_agent_components_ui::{COMPOSER, Intent, RunSurface};
use tekes_agent_contract::PromptMode;
use tekes_agent_presentation::{ComposerDestination, RunController, RunScope};
use tekes_agent_session::SessionRuntime;
use tekes_ui_controls::{
    ButtonTextAlign, ButtonTextRenderer, ControlKey, ControlMotionContext, TextSurfaceHost,
    TextSurfaceTextRenderer, composition::ContentEvent,
};
use tekes_ui_platform_macos::{
    App, AppConfig, Event, KeyEventKind, MenuBar, MouseEventKind, ScrollPhase, Size, Window,
    WindowConfig, WindowHandler,
};
use tekes_ui_render::Renderer;
use tekes_ui_scene::{GlyphAtlas, Point, Rect, Scene, Size as SSize};
use tekes_ui_text::{
    coretext::{CoreTextFonts, CoreTextRasterizer, CoreTextShaper},
    model::{Paragraph, ShapeOptions},
    push::push_paragraph_visible,
    style::TextStyle,
    traits::Shaper,
};
use tekes_ui_tokens::{Palette, Theme, proposal};
struct TextBackend {
    shaper: CoreTextShaper,
    rasterizer: CoreTextRasterizer,
    scale: f32,
    type_scale: f32,
    atlas: GlyphAtlas,
    started: std::time::Instant,
    frame_requested: bool,
    reduced_motion: bool,
    caret_on: bool,
}
impl TextBackend {
    fn shape(&mut self, text: &str, role: proposal::TextRole, width: Option<f32>) -> Rc<Paragraph> {
        let m = role.style();
        let style = if proposal::text_family(role.id()) == Some("mono") {
            TextStyle::named("Menlo", m.size * self.type_scale)
        } else {
            TextStyle::system(m.size * self.type_scale)
        }
        .with_weight(m.weight);
        Rc::new(self.shaper.shape(
            text,
            &style,
            &ShapeOptions {
                width: width.map(f64::from),
                line_spacing: 0.,
            },
        ))
    }
    fn push(
        &mut self,
        s: &mut Scene,
        p: &Paragraph,
        o: Point,
        c: tekes_ui_tokens::Rgba,
        clip: Rect,
    ) {
        push_paragraph_visible(
            s,
            &mut self.atlas,
            &mut self.rasterizer,
            p,
            o,
            self.scale,
            c,
            clip.min_y()..clip.max_y(),
        );
    }
}
impl TextSurfaceTextRenderer for TextBackend {
    fn push_visible(
        &mut self,
        s: &mut Scene,
        p: &Paragraph,
        o: Point,
        c: tekes_ui_tokens::Rgba,
        clip: Rect,
    ) {
        self.push(s, p, o, c, clip)
    }
}
impl TextSurfaceHost for TextBackend {
    fn shape_surface(&mut self, s: &str, r: proposal::TextRole, w: Option<f32>) -> Rc<Paragraph> {
        self.shape(s, r, w)
    }
    fn type_scale(&self) -> f32 {
        self.type_scale
    }
    fn caret_visible(&self) -> bool {
        self.caret_on
    }
}
impl ButtonTextRenderer for TextBackend {
    fn measure(&mut self, s: &str, r: proposal::TextRole, w: f32) -> SSize {
        let p = self.shape(s, r, Some(w));
        SSize::new(p.width as f32, p.height as f32)
    }
    fn draw(
        &mut self,
        s: &mut Scene,
        text: &str,
        r: proposal::TextRole,
        rect: Rect,
        a: ButtonTextAlign,
        color: tekes_ui_tokens::Rgba,
    ) {
        let p = self.shape(text, r, Some(rect.size.width));
        let x = match a {
            ButtonTextAlign::Leading => rect.min_x(),
            ButtonTextAlign::Center => rect.min_x() + (rect.size.width - p.width as f32) * 0.5,
        };
        let y = rect.min_y() + (rect.size.height - p.height as f32) * 0.5;
        self.push(s, &p, Point::new(x.round(), y.round()), color, rect)
    }
    fn backing_scale(&self) -> f32 {
        self.scale
    }
    fn motion_context(&self) -> Option<ControlMotionContext> {
        Some(ControlMotionContext {
            now: self.started.elapsed(),
            reduced_motion: self.reduced_motion,
        })
    }
    fn request_animation_frame(&mut self) {
        self.frame_requested = true;
    }
    fn text_surface_host(&mut self) -> Option<&mut dyn TextSurfaceHost> {
        Some(self)
    }
}
struct Host {
    surface: RunSurface,
    gpu: Option<Renderer>,
    text: TextBackend,
    scene: Scene,
    palette: Palette,
    executor: tokio::runtime::Runtime,
    runtime: Arc<SessionRuntime>,
    controller: Arc<RunController>,
    notice: Option<String>,
    results: std::sync::mpsc::Receiver<Result<(), String>>,
    sender: std::sync::mpsc::Sender<Result<(), String>>,
    selftest: bool,
    scopes: [RunScope; 2],
}
impl Host {
    fn publish(&self, w: &Window) {
        w.set_text_input_state(self.surface.tree.focused_text().map(|s| {
            let r = s.caret();
            let origin = s.origin();
            tekes_ui_platform_macos::TextInputState {
                caret_index: s.input.caret,
                text: s.input.text.clone(),
                selection: s.input.selection(),
                marked: s.input.marked.clone(),
                caret_origin: tekes_ui_platform_macos::Point {
                    x: r.min_x() as f64,
                    y: r.min_y() as f64,
                },
                caret_size: Size {
                    width: r.size.width as f64,
                    height: r.size.height as f64,
                },
                read_only: s.input.read_only,
                carets: s
                    .paragraph
                    .as_ref()
                    .map(|p| {
                        p.lines
                            .iter()
                            .flat_map(|l| {
                                l.carets.iter().map(move |(index, x)| {
                                    (
                                        *index,
                                        tekes_ui_platform_macos::Point {
                                            x: f64::from(origin.x) + x,
                                            y: f64::from(origin.y) + l.top,
                                        },
                                        Size {
                                            width: 1.,
                                            height: l.height,
                                        },
                                    )
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            }
        }));
        use tekes_ui_controls::composition::ContentRole as R;
        use tekes_ui_platform_macos::{
            AccessibilityKey as K, AccessibilityNode as N, AccessibilityRole as A,
        };
        let nodes = self
            .surface
            .tree
            .semantics()
            .into_iter()
            .map(|n| {
                let text = self.surface.tree.text_surface(n.id);
                N {
                    key: K {
                        component: n.id.0,
                        part: None,
                    },
                    parent: n.parent.map(|p| K {
                        component: p.0,
                        part: None,
                    }),
                    role: match n.semantics.role {
                        R::TextInput => A::TextField,
                        R::Button => A::Button,
                        R::Text => A::StaticText,
                        _ => A::Group,
                    },
                    frame: tekes_ui_platform_macos::Rect {
                        origin: tekes_ui_platform_macos::Point {
                            x: n.frame.min_x() as f64,
                            y: n.frame.min_y() as f64,
                        },
                        size: Size {
                            width: n.frame.size.width as f64,
                            height: n.frame.size.height as f64,
                        },
                    },
                    label: n.semantics.name,
                    value: n.semantics.value,
                    description: n.semantics.description,
                    enabled: !n.semantics.disabled,
                    focused: n.focused,
                    selected: n.semantics.selected,
                    expanded: n.semantics.expanded,
                    invalid: n.semantics.invalid,
                    text_selection: text.map(|s| s.input.selection()),
                    read_only: text.is_some_and(|s| s.input.read_only),
                }
            })
            .collect();
        w.set_accessibility_nodes(nodes);
    }
    fn draw(&mut self, w: &Window) {
        while let Ok(result) = self.results.try_recv() {
            if let Err(e) = result {
                self.notice = Some(e);
            }
        }
        if let Err(e) = self.surface.sync(&self.controller.snapshot()) {
            self.notice = Some(e.into());
        }
        if let Some(notice) = &self.notice {
            self.surface
                .tree
                .get_mut(tekes_agent_components_ui::STATUS)
                .unwrap()
                .component_mut::<tekes_ui_controls::composition::TextContent>()
                .unwrap()
                .text
                .push_str(&format!(" · {notice}"));
        }
        self.text.scale = w.scale() as f32;
        self.text.atlas.begin_frame();
        self.scene.clear();
        let clip = Rect::new(0., 0., w.size().width as f32, w.size().height as f32);
        self.surface
            .render(
                &mut self.scene,
                &mut self.text,
                clip.inset(16.),
                clip,
                self.palette,
            )
            .expect("Agent surface layout");
        self.publish(w);
        if let Some(gpu) = &mut self.gpu {
            gpu.render(
                &self.scene,
                &mut self.text.atlas,
                self.palette.color("surface-1").unwrap(),
            )
            .expect("render Agent scene");
        }
    }
    fn close(&mut self, w: &Window) {
        if self.selftest {
            let snapshot = self.controller.snapshot();
            assert_eq!(snapshot.draft.text, "B 会话草稿");
            eprintln!(
                "SELFTEST_FINAL connection={:?} notice={:?} runtime={:?}",
                snapshot.connection,
                self.notice,
                self.runtime.inspect(|s| s
                    .sessions
                    .iter()
                    .map(|(id, x)| (
                        id.clone(),
                        x.journal_ready,
                        x.journal_error.clone(),
                        x.history_base_sequence,
                        x.tail_sequence
                    ))
                    .collect::<Vec<_>>())
            );
            assert!(!snapshot.transcript.is_stale);
            assert!(
                snapshot.transcript.items.len() > 20,
                "Earlier must load actual history"
            );
            let state = self.runtime.snapshot();
            let a = &state.sessions["a"];
            assert_eq!(
                a.entries
                    .values()
                    .filter(|r| r.event.kind == "user/message"
                        && r.event.data.to_string().contains("中文消息"))
                    .count(),
                1
            );
            assert_eq!(
                a.entries
                    .values()
                    .filter(|r| r.event.kind == "user/message"
                        && r.event.data.to_string().contains("停止演示"))
                    .count(),
                1
            );
            assert!(!a.run_open);
            println!(
                "NATIVE_AGENT_UI_SELFTEST_OK synthetic-text-services+button-send+stream+cancel+draft-switch+reconnect+history-data; physical-ime=false; prepend-anchor=false"
            );
        }
        w.set_animating(false);
        w.close();
    }
    fn maybe_finish_selftest(&mut self, w: &Window) {
        if !self.selftest || self.text.started.elapsed() < std::time::Duration::from_secs(8) {
            return;
        }
        let snapshot = self.controller.snapshot();
        let ready = snapshot.draft.text == "B 会话草稿"
            && !snapshot.transcript.is_stale
            && snapshot.transcript.items.len() > 20;
        if ready || self.text.started.elapsed() > std::time::Duration::from_secs(20) {
            self.close(w);
        }
    }
    fn intents(&mut self, intents: Vec<Intent>) {
        for intent in intents {
            let controller = self.controller.clone();
            let runtime = self.runtime.clone();
            let sender = self.sender.clone();
            match intent {
                Intent::Edit { destination, text } => {
                    let snapshot = controller.snapshot();
                    if snapshot
                        .selection
                        .as_ref()
                        .is_none_or(|s| s.destination != destination)
                    {
                        self.notice = Some("会话已切换，未覆盖另一会话的草稿".into());
                        continue;
                    }
                    let mut draft = snapshot.draft.clone();
                    draft.text = text;
                    if let Err(e) = controller.edit(&destination, draft) {
                        self.notice = Some(e.to_string());
                    }
                }
                Intent::Send { destination } => {
                    match controller.prepare(&destination, PromptMode::Queue) {
                        Ok(ticket) => {
                            self.executor.spawn(async move {
                                let result = controller
                                    .dispatch(ticket, None)
                                    .await
                                    .map(|_| ())
                                    .map_err(|e| e.to_string());
                                let _ = sender.send(result);
                            });
                        }
                        Err(e) => self.notice = Some(e.to_string()),
                    }
                }
                Intent::Cancel { scope } => {
                    self.executor.spawn(async move {
                        let _ =
                            sender.send(controller.cancel(&scope).await.map_err(|e| e.to_string()));
                    });
                }
                Intent::Earlier { scope } => {
                    self.executor.spawn(async move {
                        let _ = sender.send(
                            controller
                                .load_earlier(&scope)
                                .await
                                .map(|_| ())
                                .map_err(|e| e.to_string()),
                        );
                    });
                }
                Intent::Reconnect => {
                    self.executor.spawn(async move {
                        let r = async {
                            runtime.reconnect().await?;
                            runtime.wait_ready(std::time::Duration::from_secs(5)).await
                        }
                        .await;
                        let _ = sender.send(r.map_err(|e| e.to_string()));
                    });
                }
                Intent::Select(id) => {
                    // Selection/draft ownership changes synchronously. Only attachment is async.
                    let scope = self.scopes[usize::from(id == "b")].clone();
                    match controller.select(ComposerDestination::run(scope.clone())) {
                        Ok(_) => {
                            let id = scope.session_id.clone();
                            self.executor.spawn(async move {
                                let _ = sender.send(
                                    runtime.open_session(&id).await.map_err(|e| e.to_string()),
                                );
                            });
                        }
                        Err(e) => self.notice = Some(e.to_string()),
                    }
                }
            }
        }
    }
    fn dispatch(&mut self, e: Event, w: &Window) {
        let event = match e {
            Event::Mouse(m) => {
                if matches!(m.kind,MouseEventKind::Down{button,..}|MouseEventKind::Up{button,..}|MouseEventKind::Dragged{button} if button!=tekes_ui_platform_macos::MouseButton::Left)
                {
                    return;
                }
                let p = Point::new(m.position.x as f32, m.position.y as f32);
                match m.kind {
                    MouseEventKind::Down { .. } => ContentEvent::PointerDown(p),
                    MouseEventKind::Up { .. } => ContentEvent::PointerUp(p),
                    MouseEventKind::Exited => ContentEvent::PointerMove(Point::new(-1., -1.)),
                    _ => ContentEvent::PointerMove(p),
                }
            }
            Event::Scroll(s) => ContentEvent::Scroll {
                position: Point::new(s.position.x as f32, s.position.y as f32),
                delta: Point::new(-s.delta.x as f32, -s.delta.y as f32),
                precise: s.precise,
                begin: matches!(s.phase, ScrollPhase::Began | ScrollPhase::MayBegin),
                end: matches!(s.phase, ScrollPhase::Ended | ScrollPhase::Cancelled),
                momentum: s.momentum_phase != ScrollPhase::None,
                time: s.timestamp,
            },
            Event::TextCommit { text, replacement } => ContentEvent::Commit { text, replacement },
            Event::TextPreedit {
                text,
                selection,
                clauses,
                replacement,
            } => ContentEvent::Preedit {
                text,
                selection,
                clauses,
                replacement,
            },
            Event::TextUnmark => ContentEvent::Unmark,
            Event::TextCommand(c) => {
                if c == "copy:" {
                    if let Some(s) = self.surface.tree.focused_text() {
                        w.set_clipboard_text(&s.input.selected_text());
                    }
                    return;
                }
                if c == "paste:" {
                    if let Some(text) = w.clipboard_text() {
                        let actions = self.surface.event(&ContentEvent::Commit {
                            text,
                            replacement: None,
                        });
                        self.intents(actions);
                    }
                    return;
                }
                ContentEvent::TextCommand(c)
            }
            Event::Key(k) => {
                if k.kind == KeyEventKind::Down && k.modifiers.command {
                    let c = match k.key_code {
                        0 => Some("selectAll:"),
                        6 if k.modifiers.shift => Some("redo:"),
                        6 => Some("undo:"),
                        8 => Some("copy:"),
                        9 => Some("paste:"),
                        _ => None,
                    };
                    if let Some(c) = c {
                        return self.dispatch(Event::TextCommand(c.into()), w);
                    }
                }
                let key = match k.key_code {
                    36 | 76 => ControlKey::Enter,
                    49 => ControlKey::Space,
                    48 => ControlKey::Tab {
                        reverse: k.modifiers.shift,
                    },
                    53 => ControlKey::Escape,
                    123 => ControlKey::Left,
                    124 => ControlKey::Right,
                    125 => ControlKey::Down,
                    126 => ControlKey::Up,
                    _ => return,
                };
                ContentEvent::Key {
                    key,
                    down: k.kind == KeyEventKind::Down,
                }
            }
            Event::Focus(false) => ContentEvent::Cancel,
            _ => return,
        };
        let actions = self.surface.event(&event);
        self.intents(actions);
    }
}
impl WindowHandler for Host {
    fn handle(&mut self, e: Event, w: &mut Window) {
        match e {
            Event::Created => {
                // SAFETY: the public platform window owns a live CAMetalLayer for this renderer.
                self.gpu = Some(
                    unsafe {
                        Renderer::new_for_metal_layer(
                            w.metal_layer(),
                            w.drawable_size(),
                            w.scale() as f32,
                        )
                    }
                    .expect("Metal renderer"),
                );
                self.surface.sync(&self.controller.snapshot()).unwrap();
                self.surface.tree.focus(Some(COMPOSER), true);
                self.draw(w);
                // Explicit development-only frame pump: TKUI-001, not a production wake bridge.
                assert!(
                    w.set_animating(true),
                    "development frame pump needs macOS 14+"
                );
                if self.selftest {
                    native_script(&self.surface);
                }
            }
            Event::Resized {
                drawable_size,
                scale,
                ..
            } => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(drawable_size, scale as f32);
                }
                self.draw(w);
            }
            Event::AnimationFrame(_) => {
                self.draw(w);
                self.maybe_finish_selftest(w);
            }
            Event::Redraw => self.draw(w),
            Event::CaretBlink => {
                self.text.caret_on = !self.text.caret_on;
                self.draw(w);
            }
            Event::SystemMotionChanged(reduced) => {
                self.text.reduced_motion = reduced;
                self.draw(w);
            }
            Event::Accessibility { target, action } => {
                if self.surface.composing() && target.component != COMPOSER.0 {
                    return;
                }
                use tekes_ui_controls::composition::{AccessibilityAction as A, ComponentId};
                let action = match action {
                    tekes_ui_platform_macos::AccessibilityAction::Focus => A::Focus,
                    tekes_ui_platform_macos::AccessibilityAction::Press => A::Press,
                    tekes_ui_platform_macos::AccessibilityAction::SetValue(v) => A::SetValue(v),
                    tekes_ui_platform_macos::AccessibilityAction::SetSelection(v) => {
                        A::SetSelection(v)
                    }
                    tekes_ui_platform_macos::AccessibilityAction::ReplaceSelectedText(v) => {
                        A::ReplaceSelectedText(v)
                    }
                };
                self.surface.tree.accessibility_action(
                    ComponentId(target.component),
                    target.part,
                    action,
                );
                let intents = self
                    .surface
                    .event(&ContentEvent::PointerMove(Point::new(-1., -1.)));
                self.intents(intents);
                self.draw(w);
            }
            Event::CloseRequested => self.close(w),
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
                self.draw(w);
            }
            other => {
                self.dispatch(other, w);
                self.draw(w);
            }
        }
    }
}
pub fn run() {
    let args = std::env::args().collect::<Vec<_>>();
    let offscreen = args
        .iter()
        .position(|a| a == "--offscreen")
        .map(|i| args.get(i + 1).expect("--offscreen needs a path").clone());
    assert!(
        offscreen.is_some() || args.iter().any(|a| a == "--frame-pump"),
        "TKUI-001: native development demo requires explicit --frame-pump"
    );
    let executor = tokio::runtime::Runtime::new().unwrap();
    let (runtime, controller, scopes) = if let Some(i) =
        args.iter().position(|a| a == "--endpoint-config")
    {
        assert!(
            !args.iter().any(|a| a == "--native-selftest"),
            "native selftest uses owned fixture state only"
        );
        let config: serde_json::Value = serde_json::from_slice(
            &std::fs::read(args.get(i + 1).expect("config path")).expect("read explicit config"),
        )
        .expect("config JSON");
        let id = tekes_agent_contract::string(&config, "endpointId").expect("endpointId");
        let bearer =
            std::env::var(tekes_agent_contract::string(&config, "bearerEnv").expect("bearerEnv"))
                .expect("explicit bearer env missing");
        let connector = tekes_agent_endpoint_kernel::KernelConnector::new(
            id,
            tekes_agent_contract::string(&config, "url").expect("url"),
            bearer,
        )
        .expect("Native connector config");
        let sessions = config["sessions"]
            .as_array()
            .filter(|v| v.len() == 2)
            .expect("two explicit sessions");
        let scopes = [
            RunScope::main(id, sessions[0].as_str().expect("session ID")),
            RunScope::main(id, sessions[1].as_str().expect("session ID")),
        ];
        let (runtime, controller) = executor
            .block_on(async {
                let runtime = Arc::new(SessionRuntime::spawn(id, Arc::new(connector)));
                runtime
                    .wait_ready(std::time::Duration::from_secs(10))
                    .await?;
                let controller = Arc::new(RunController::new(runtime.clone())?);
                controller.open(scopes[0].clone()).await?;
                tokio::time::timeout(std::time::Duration::from_secs(5), async {
                    while controller.snapshot().transcript.is_stale {
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    }
                })
                .await
                .map_err(|_| tekes_agent_contract::Error::Timeout)?;
                Ok::<_, tekes_agent_contract::Error>((runtime, controller))
            })
            .expect("Native baseline");
        (runtime, controller, scopes)
    } else {
        let (runtime, controller) = executor
            .block_on(tekes_agent_ui_demo::fixture::setup())
            .expect("fixture baseline");
        (
            runtime,
            controller,
            [
                RunScope::main("fixture", "a"),
                RunScope::main("fixture", "b"),
            ],
        )
    };
    let fonts = CoreTextFonts::new();
    let (sender, results) = std::sync::mpsc::channel();
    let mut host = Host {
        surface: RunSurface::new(),
        gpu: None,
        text: TextBackend {
            shaper: CoreTextShaper::new(fonts.clone()),
            rasterizer: CoreTextRasterizer::new(fonts),
            scale: 2.,
            type_scale: 1.,
            atlas: GlyphAtlas::default(),
            started: Instant::now(),
            frame_requested: false,
            reduced_motion: false,
            caret_on: true,
        },
        scene: Scene::new(),
        palette: Palette::from(Theme::LIGHT),
        executor,
        runtime,
        controller,
        notice: None,
        results,
        sender,
        selftest: args.iter().any(|a| a == "--native-selftest"),
        scopes,
    };
    host.surface.sync(&host.controller.snapshot()).unwrap();
    host.surface.follow_latest();
    if let Some(path) = offscreen {
        host.text.scale = 1.;
        let clip = Rect::new(0., 0., 1000., 760.);
        host.text.atlas.begin_frame();
        host.surface
            .render(
                &mut host.scene,
                &mut host.text,
                clip.inset(16.),
                clip,
                host.palette,
            )
            .unwrap();
        let mut gpu = Renderer::new_headless().expect("offscreen renderer");
        let bytes = gpu
            .render_to_rgba8(
                &host.scene,
                &mut host.text.atlas,
                (1000, 760),
                1.,
                host.palette.color("surface-1").unwrap(),
            )
            .unwrap();
        let file = std::fs::File::create(path).unwrap();
        let mut encoder = png::Encoder::new(file, 1000, 760);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&bytes)
            .unwrap();
        host.executor.block_on(host.runtime.stop()).unwrap();
        return;
    }
    App::new(AppConfig {
        name: Some("AgentKit UI Demo".into()),
        ..Default::default()
    })
    .run(
        WindowConfig {
            title: "AgentKit · 本地协议演示".into(),
            size: Size {
                width: 1000.,
                height: 760.,
            },
            ..Default::default()
        },
        MenuBar::default(),
        host,
    );
}
fn native_script(surface: &RunSurface) {
    use tekes_agent_components_ui::{CANCEL, EARLIER, RECONNECT, SEND, SESSION_A, SESSION_B};
    use tekes_ui_platform_macos::selftest::{Action, run_script};
    let click = |id| {
        let p = surface.tree.get(id).unwrap().frame().center();
        Action::Click {
            x: p.x as f64,
            y: p.y as f64,
        }
    };
    let composer = {
        let p = surface.editor().inner.center();
        Action::Click {
            x: p.x as f64,
            y: p.y as f64,
        }
    };
    run_script(vec![
        (500, Action::Preedit("中文组字".into())),
        (
            800,
            Action::ExpectText {
                text: "中文组字".into(),
                marked: true,
            },
        ),
        (1000, Action::Text("中文消息".into())),
        (
            1200,
            Action::ExpectText {
                text: "中文消息".into(),
                marked: false,
            },
        ),
        (1400, click(SEND)),
        (3100, composer.clone()),
        (3200, Action::Text("停止演示".into())),
        (3400, click(SEND)),
        (3700, click(CANCEL)),
        (4300, click(SESSION_B)),
        (4500, composer.clone()),
        (4600, Action::Text("B 会话草稿".into())),
        (4800, click(SESSION_A)),
        (5000, composer.clone()),
        (
            5100,
            Action::ExpectText {
                text: "".into(),
                marked: false,
            },
        ),
        (5200, click(SESSION_B)),
        (5400, composer),
        (
            5500,
            Action::ExpectText {
                text: "B 会话草稿".into(),
                marked: false,
            },
        ),
        (5700, click(RECONNECT)),
        (6200, click(EARLIER)),
        (
            7200,
            Action::Log("input script delivered; awaiting fresh public state".into()),
        ),
    ]);
}
