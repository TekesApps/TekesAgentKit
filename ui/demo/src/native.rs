// Text bridge follows TekesUI's MIT-licensed public consumer example; no Gallery internals.
#[path = "native_key.rs"]
pub(super) mod native_key;
#[path = "native_pointer.rs"]
pub(super) mod native_pointer;
use std::{io::Write, rc::Rc, sync::Arc, time::Instant};
use tekes_agent_components_ui::{COMPOSER, Intent, RunSurface};
use tekes_agent_contract::PromptMode;
use tekes_agent_presentation::{ComposerDestination, RunController, RunScope};
use tekes_agent_session::SessionRuntime;
use tekes_ui_controls::{
    ButtonTextAlign, ButtonTextRenderer, ControlMotionContext, TextSurfaceHost,
    TextSurfaceTextRenderer, composition::ContentEvent,
};
use tekes_ui_platform_macos::{
    App, AppConfig, Event, KeyEventKind, MenuBar, ScrollPhase, Size, Window, WindowConfig,
    WindowHandler,
};
use tekes_ui_render::Renderer;
use tekes_ui_scene::{GlyphAtlas, Point, Rect, Scene, Size as SSize};
use tekes_ui_text::{
    coretext::{CoreTextFonts, CoreTextRasterizer, CoreTextShaper},
    model::{Paragraph, ShapeOptions},
    push::push_paragraph_visible,
    traits::Shaper,
};
use tekes_ui_tokens::{Palette, Theme, proposal};
pub(super) fn window_renderer(window: &Window) -> Renderer {
    // SAFETY: the public platform window owns a live CAMetalLayer for this renderer.
    unsafe {
        Renderer::new_for_metal_layer(
            window.metal_layer(),
            window.drawable_size(),
            window.scale() as f32,
        )
    }
    .expect("Metal renderer")
}
/// Common public-platform event adapter for both the legacy harness and formal
/// component demo. Text editing and native text-key ownership remain in TekesUI.
pub(super) fn input_event(
    e: Event,
    w: &Window,
    tree: &tekes_ui_controls::composition::ContentHost,
) -> Option<ContentEvent> {
    Some(match e {
        Event::Mouse(mouse) => native_pointer::translate(mouse),
        Event::TextKey(report) => native_key::translate(report),
        Event::Scroll(scroll) => ContentEvent::Scroll {
            position: Point::new(scroll.position.x as f32, scroll.position.y as f32),
            delta: Point::new(-scroll.delta.x as f32, -scroll.delta.y as f32),
            precise: scroll.precise,
            begin: matches!(scroll.phase, ScrollPhase::Began | ScrollPhase::MayBegin),
            end: matches!(scroll.phase, ScrollPhase::Ended | ScrollPhase::Cancelled),
            momentum: scroll.momentum_phase != ScrollPhase::None,
            time: scroll.timestamp,
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
        Event::TextCommand(command) => {
            if command == "copy:" {
                if let Some(surface) = tree.focused_text() {
                    w.set_clipboard_text(&surface.input.selected_text());
                }
                return None;
            }
            if command == "paste:" {
                return w.clipboard_text().map(|text| ContentEvent::Commit {
                    text,
                    replacement: None,
                });
            }
            ContentEvent::TextCommand(command)
        }
        Event::Key(key) => {
            if key.kind == KeyEventKind::Down && key.modifiers.command {
                let command = match key.key_code {
                    0 => Some("selectAll:"),
                    6 if key.modifiers.shift => Some("redo:"),
                    6 => Some("undo:"),
                    8 => Some("copy:"),
                    9 => Some("paste:"),
                    _ => None,
                };
                if let Some(command) = command {
                    return input_event(Event::TextCommand(command.into()), w, tree);
                }
            }
            ContentEvent::KeyInput(native_key::key(key))
        }
        Event::Focus(false) => ContentEvent::Cancel,
        _ => return None,
    })
}
pub(super) struct TextBackend {
    pub(super) shaper: CoreTextShaper,
    pub(super) rasterizer: CoreTextRasterizer,
    pub(super) scale: f32,
    pub(super) type_scale: f32,
    pub(super) atlas: GlyphAtlas,
    pub(super) started: std::time::Instant,
    pub(super) frame_requested: bool,
    pub(super) reduced_motion: bool,
    pub(super) caret_on: bool,
    pub(super) recent_labels: Vec<Rc<tekes_ui_text::LabelLayout>>,
    pub(super) recent_label_ink: Vec<(Rc<tekes_ui_text::LabelLayout>, tekes_ui_tokens::Rgba)>,
}
impl TextBackend {
    fn shape(&mut self, text: &str, role: proposal::TextRole, width: Option<f32>) -> Rc<Paragraph> {
        let style = tekes_agent_ui_demo::label_host::text_style(role, self.type_scale);
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
        if let Some(label) = self
            .recent_labels
            .iter()
            .rev()
            .find(|label| std::ptr::eq(label.paragraph(), p))
        {
            if self.recent_label_ink.len() == 128 {
                self.recent_label_ink.remove(0);
            }
            self.recent_label_ink.push((label.clone(), c));
        }
        self.push(s, p, o, c, clip)
    }
}
impl TextSurfaceHost for TextBackend {
    fn shape_label(
        &mut self,
        text: &str,
        role: proposal::TextRole,
        width: f32,
        options: tekes_ui_text::LabelOptions,
    ) -> Option<Rc<tekes_ui_text::LabelLayout>> {
        let layout = tekes_agent_ui_demo::label_host::shape_label(
            &mut self.shaper,
            text,
            role,
            width,
            self.type_scale,
            options,
        );
        // Bounded diagnostic receipts from the original SDK label service.
        if self.recent_labels.len() == 64 {
            self.recent_labels.remove(0);
        }
        self.recent_labels.push(layout.clone());
        Some(layout)
    }

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
    wake: Option<tekes_ui_platform_macos::WindowWake>,
    notifications: Option<tokio::task::JoinHandle<()>>,
    scopes: [RunScope; 2],
}
pub(super) fn text_state(
    s: &tekes_ui_controls::TextSurface,
) -> tekes_ui_platform_macos::TextInputState {
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
}
pub(super) fn publish_content(tree: &tekes_ui_controls::composition::ContentHost, w: &Window) {
    w.set_text_input_state(tree.focused_text().map(text_state));
    use tekes_ui_controls::composition::ContentRole as R;
    use tekes_ui_platform_macos::{
        AccessibilityKey as K, AccessibilityNode as N, AccessibilityRole as A,
    };
    let nodes = tree
        .semantics()
        .into_iter()
        .filter_map(|n| {
            let frame = n.visible?;
            let text = tree.text_surface(n.id);
            Some(N {
                key: K {
                    component: n.id.0,
                    part: None,
                },
                parent: n.parent.map(|p| K {
                    component: p.0,
                    part: None,
                }),
                role: match n.semantics.role {
                    R::TextInput if text.is_some_and(|s| s.input.multiline) => A::TextArea,
                    R::TextInput => A::TextField,
                    R::Button => A::Button,
                    R::Text => A::StaticText,
                    _ => A::Group,
                },
                frame: tekes_ui_platform_macos::Rect {
                    origin: tekes_ui_platform_macos::Point {
                        x: frame.min_x() as f64,
                        y: frame.min_y() as f64,
                    },
                    size: Size {
                        width: frame.size.width as f64,
                        height: frame.size.height as f64,
                    },
                },
                label: n.semantics.name,
                value: n.semantics.value,
                description: n.semantics.description,
                enabled: !n.semantics.disabled,
                focused: n.focused && w.is_active(),
                selected: n.semantics.selected,
                expanded: n.semantics.expanded,
                invalid: n.semantics.invalid,
                required: n.semantics.required,
                text_selection: text.map(|s| s.input.selection()),
                read_only: text.is_some_and(|s| s.input.read_only),
            })
        })
        .collect();
    w.set_accessibility_text_layouts(
        tree.semantics()
            .into_iter()
            .filter_map(|n| {
                let surface = tree.text_surface(n.id)?;
                let paragraph = surface.paragraph.as_ref()?;
                Some((
                    K {
                        component: n.id.0,
                        part: None,
                    },
                    tekes_ui_platform_macos::AccessibilityTextLayout {
                        state: text_state(surface),
                        lines: paragraph.lines.iter().map(|l| l.utf16.clone()).collect(),
                    },
                ))
            })
            .collect(),
    );
    w.set_accessibility_nodes(nodes);
}
impl Host {
    fn publish(&self, w: &Window) {
        publish_content(&self.surface.tree, w);
        // Opt-in review evidence only; no timer or additional redraw is started.
        if let Some(dir) = std::env::var_os("TEKESUI_COMPONENT_EVIDENCE") {
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).expect("review evidence directory");
            let snapshot = self.controller.snapshot();
            let surface = &self.surface.transcript().field.surface;
            let anchor = surface.hit(surface.inner.origin);
            let receipt = serde_json::json!({
                "elapsed_ms": self.text.started.elapsed().as_millis(),
                "redraw_count": w.redraw_count(), "animation_frames": w.animation_frame_count(),
                "animating": w.is_animating(),
                "active": w.is_active(), "focused": self.surface.tree.focused().map(|id| id.0),
                "screen": w.screen_info().map(|s| serde_json::json!({"name":s.name,"builtin":s.builtin,"scale":s.scale})),
                "draft": snapshot.draft.text, "items": snapshot.transcript.items.len(),
                "connection": format!("{:?}", snapshot.connection),
                "transcript_offset": surface.scroll.offset.y,
                "anchor_index": anchor, "anchor_y": anchor.map(|i| surface.caret_doc(i).min_y() + surface.origin().y),
                "anchor_tail": anchor.map(|i| surface.input.text.encode_utf16().skip(i).take(80).collect::<Vec<_>>()),
                "selection": surface.input.selection(),
            });
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join("frames.jsonl"))
                .expect("review frames");
            writeln!(file, "{receipt}").expect("review frame receipt");
        }
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
        self.text.frame_requested = false;
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
        // Only finite control transitions request display-link frames.
        w.set_animating(self.text.frame_requested);
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
        if let Some(task) = self.notifications.take() {
            task.abort();
        }
        w.set_animating(false);
        w.close();
        if std::env::var_os("TEKESUI_COMPONENT_EVIDENCE").is_some()
            && let Some(wake) = &self.wake
        {
            assert_eq!(wake.wake(), tekes_ui_platform_macos::WakeResult::Closed);
            assert_eq!(
                wake.clone().wake(),
                tekes_ui_platform_macos::WakeResult::Closed
            );
            eprintln!("AGENT_REVIEW_CLOSED original+clone=Closed");
        }
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
            let wake = self.wake.clone();
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
                            // The existing synchronous prepare owns the frozen
                            // draft and authoritative clear. Apply its versioned
                            // transaction before accepting another native input.
                            if let Err(error) = self.surface.sync(&controller.snapshot()) {
                                self.notice = Some(error.into());
                            }

                            self.executor.spawn(async move {
                                let result = controller
                                    .dispatch(ticket, None)
                                    .await
                                    .map(|_| ())
                                    .map_err(|e| e.to_string());
                                let _ = sender.send(result);
                                if let Some(wake) = wake {
                                    wake.wake();
                                }
                            });
                        }
                        Err(e) => self.notice = Some(e.to_string()),
                    }
                }
                Intent::Cancel { scope } => {
                    self.executor.spawn(async move {
                        let _ =
                            sender.send(controller.cancel(&scope).await.map_err(|e| e.to_string()));
                        if let Some(wake) = wake {
                            wake.wake();
                        }
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
                        if let Some(wake) = wake {
                            wake.wake();
                        }
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
                        if let Some(wake) = wake {
                            wake.wake();
                        }
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
                                if let Some(wake) = wake {
                                    wake.wake();
                                }
                            });
                        }
                        Err(e) => self.notice = Some(e.to_string()),
                    }
                }
            }
        }
    }
    fn dispatch(&mut self, e: Event, w: &Window) {
        let Some(event) = input_event(e, w, &self.surface.tree) else {
            return;
        };
        match self.surface.event_with_text(&event, &mut self.text) {
            Ok(actions) => self.intents(actions),
            Err(error) => self.notice = Some(format!("text input layout: {error:?}")),
        }
    }
}
impl WindowHandler for Host {
    fn handle(&mut self, e: Event, w: &mut Window) {
        match e {
            Event::Created => {
                assert!(w.set_text_key_events(true));
                if std::env::var_os("TEKESUI_AX_PROTOCOL_AUDIT").is_some() {
                    use crate::acceptance_script::{Action, run_script};
                    run_script(vec![
                        (
                            1000,
                            Action::AuditAXText(tekes_agent_components_ui::TRANSCRIPT.0),
                        ),
                        (1200, Action::AuditAXText(COMPOSER.0)),
                    ]);
                }
                self.gpu = Some(window_renderer(w));
                self.surface.sync(&self.controller.snapshot()).unwrap();
                self.surface.tree.focus(Some(COMPOSER), true);
                self.draw(w);
                let wake = w.wake_proxy();
                self.wake = Some(wake.clone());
                self.notifications = Some(self.executor.spawn(
                    tekes_agent_ui_demo::notifications::forward(
                        self.controller.subscribe(),
                        move || wake.wake() != tekes_ui_platform_macos::WakeResult::Closed,
                    ),
                ));
                if self.selftest {
                    native_script(&self.surface);
                    let wake = self.wake.clone().unwrap();
                    self.executor.spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(9)).await;
                        wake.wake();
                        // One bounded failure deadline for this opt-in selftest only.
                        tokio::time::sleep(std::time::Duration::from_secs(12)).await;
                        wake.wake();
                    });
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
            Event::Wake => {
                eprintln!(
                    "AGENT_REVIEW_WAKE elapsed_ms={}",
                    self.text.started.elapsed().as_millis()
                );
                // No polling timer: publication/completion notifications invalidate the UI.
                w.request_redraw();
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
                if !w.is_active() {
                    return;
                }
                if self.surface.composing() && target.component != COMPOSER.0 {
                    return;
                }
                use tekes_ui_controls::composition::{AccessibilityAction as A, ComponentId};
                let action = match action {
                    // This host mounts no Tree; disclosure is never a button action.
                    tekes_ui_platform_macos::AccessibilityAction::SetExpanded(_) => return,
                    tekes_ui_platform_macos::AccessibilityAction::Increment => A::Increment,
                    tekes_ui_platform_macos::AccessibilityAction::Decrement => A::Decrement,
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
                let intents = self.surface.accessibility_action(
                    ComponentId(target.component),
                    target.part,
                    action,
                );
                self.intents(intents);
                self.draw(w);
            }
            Event::Focus(active) => {
                if !active {
                    self.surface.event(&ContentEvent::Cancel);
                    self.surface.tree.focus(None, false);
                }
                self.publish(w);
                w.request_redraw();
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
        let configured =
            tekes_agent_ui_demo::endpoint_config::EndpointConfiguration::parse(&config, |name| {
                std::env::var(name).ok()
            })
            .expect("explicit endpoint configuration");
        let connected = executor
            .block_on(configured.connect())
            .expect("endpoint baseline");
        (connected.runtime, connected.controller, connected.scopes)
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
            recent_labels: Vec::new(),
            recent_label_ink: Vec::new(),
        },
        scene: Scene::new(),
        palette: Palette::from(Theme::LIGHT),
        executor,
        runtime,
        controller,
        notice: None,
        results,
        sender,
        wake: None,
        notifications: None,
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
        let snapshot = host.controller.snapshot();
        println!(
            "{}",
            serde_json::json!({
                "offscreen": true,
                "nativeInteraction": false,
                "isStale": snapshot.transcript.is_stale,
                "transcriptItems": snapshot.transcript.items.len(),
                "finalAssistantItems": snapshot.transcript.items.iter().filter(|item|
                    item.kind == tekes_agent_presentation::TranscriptItemKind::Assistant && item.is_final
                ).count(),
            })
        );
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
            placement: tekes_ui_platform_macos::WindowPlacement::Auto {
                need_120hz: false,
                size_check: false,
            },
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
    use crate::acceptance_script::{Action, run_script};
    use tekes_agent_components_ui::{CANCEL, EARLIER, RECONNECT, SEND, SESSION_A, SESSION_B};
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
