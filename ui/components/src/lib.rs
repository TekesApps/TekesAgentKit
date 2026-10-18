//! Agent-owned composition of public TekesUI controls. No endpoint or Run state machine.
use std::collections::BTreeMap;
use tekes_agent_presentation::{ComposerDestination, RunScope, RunSnapshot};
use tekes_ui_controls::{
    ButtonTextRenderer, ReadingController, TextSurface,
    composition::*,
    extended::{Axis, Stack},
    layout::MeasuredChild,
};
use tekes_ui_scene::{Rect, Scene, Size};
use tekes_ui_tokens::{Palette, Spacing};

pub const ROOT: ComponentId = ComponentId(1);
pub const STATUS: ComponentId = ComponentId(2);
pub const TRANSCRIPT: ComponentId = ComponentId(3);
pub const COMPOSER: ComponentId = ComponentId(4);
pub const ACTIONS: ComponentId = ComponentId(5);
pub const SEND: ComponentId = ComponentId(6);
pub const CANCEL: ComponentId = ComponentId(7);
pub const EARLIER: ComponentId = ComponentId(8);
pub const LATEST: ComponentId = ComponentId(9);
pub const RECONNECT: ComponentId = ComponentId(10);
pub const SESSION_A: ComponentId = ComponentId(11);
pub const SESSION_B: ComponentId = ComponentId(12);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Intent {
    Edit {
        destination: ComposerDestination,
        text: String,
    },
    Send {
        destination: ComposerDestination,
    },
    Cancel {
        scope: RunScope,
    },
    Earlier {
        scope: RunScope,
    },
    Reconnect,
    Select(&'static str),
}

/// Agent layout role. Editing, selection, IME and scrolling delegate to TekesUI unchanged.
#[derive(Debug)]
pub struct AgentText {
    pub field: TextFieldContent,
    transcript: bool,
    focused: bool,
}
impl AgentText {
    fn new(transcript: bool) -> Self {
        let mut field = TextFieldContent::new(if transcript { "会话记录" } else { "消息" }, "");
        if transcript {
            field.surface = TextSurface::new("", true, true);
            field.reading = Some(ReadingController::default());
        }
        Self {
            field,
            transcript,
            focused: false,
        }
    }
}
impl Component for AgentText {
    fn measure(&mut self, _: &mut dyn ButtonTextRenderer, a: Size) -> MeasuredChild {
        MeasuredChild::new(Size::new(
            a.width,
            if self.transcript { a.height } else { 78. },
        ))
    }
    fn arrange(&mut self, t: &mut dyn ButtonTextRenderer, f: Rect, c: Rect) {
        if self.transcript {
            let scale = t.backing_scale();
            if let Some(host) = t.text_surface_host() {
                let type_scale = host.type_scale();
                let role = self.field.surface.role;
                self.field.reading.as_mut().unwrap().layout_with(
                    &mut self.field.surface,
                    f,
                    Some(c),
                    scale,
                    type_scale,
                    |s, w| host.shape_surface(s, role, w),
                );
            }
        } else {
            self.field.arrange(t, f, c);
        }
    }
    fn paint(
        &mut self,
        s: &mut Scene,
        t: &mut dyn ButtonTextRenderer,
        f: Rect,
        c: Rect,
        p: ContentTheme,
    ) {
        if self.transcript {
            if let Some(host) = t.text_surface_host() {
                self.field
                    .surface
                    .draw(host, s, p.palette, self.focused, false);
            }
        } else {
            self.field.paint(s, t, f, c, p);
        }
    }
    fn event(&mut self, e: &ContentEvent) -> ContentResponse {
        self.field.event(e)
    }
    fn focusable(&self) -> bool {
        self.field.focusable()
    }
    fn set_focus(&mut self, f: bool, v: bool) {
        self.focused = f;
        self.field.set_focus(f, v)
    }
    fn composing(&self) -> bool {
        self.field.composing()
    }
    fn requires_text_services(&self) -> bool {
        true
    }
    fn text_surface(&self) -> Option<&TextSurface> {
        Some(&self.field.surface)
    }
    fn semantic_frame(&self, f: Rect) -> Rect {
        self.field.semantic_frame(f)
    }
    fn semantics(&self) -> ContentSemantics {
        self.field.semantics()
    }
    fn accessibility_action(
        &mut self,
        p: Option<usize>,
        a: &AccessibilityAction,
    ) -> ContentResponse {
        self.field.accessibility_action(p, a)
    }
}

pub struct RunSurface {
    pub tree: ContentHost,
    selected: Option<ComposerDestination>,
    editors: BTreeMap<String, Content>,
    transcripts: BTreeMap<String, Content>,
    counts: BTreeMap<ComponentId, usize>,
    committed: String,
}
impl Default for RunSurface {
    fn default() -> Self {
        Self::new()
    }
}
impl RunSurface {
    pub fn new() -> Self {
        let buttons = [
            (SESSION_A, "会话 A"),
            (SESSION_B, "会话 B"),
            (EARLIER, "更早记录"),
            (LATEST, "回到最新"),
            (RECONNECT, "重新连接"),
            (CANCEL, "停止"),
            (SEND, "发送"),
        ];
        let actions = Content::new(
            ACTIONS,
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: buttons
                    .into_iter()
                    .map(|(id, label)| Content::new(id, ButtonContent::new(label)))
                    .collect(),
            },
        );
        let tree = ContentHost::new(Content::new(
            ROOT,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Regular.value(),
                },
                flexible_child: Some(TRANSCRIPT),
                children: vec![
                    Content::new(STATUS, TextContent::new("正在连接")),
                    Content::new(TRANSCRIPT, AgentText::new(true)),
                    Content::new(COMPOSER, AgentText::new(false)),
                    actions,
                ],
            },
        ))
        .expect("unique Agent component IDs");
        Self {
            tree,
            selected: None,
            editors: BTreeMap::new(),
            transcripts: BTreeMap::new(),
            counts: BTreeMap::new(),
            committed: String::new(),
        }
    }
    pub fn editor(&self) -> &TextSurface {
        &self
            .tree
            .get(COMPOSER)
            .unwrap()
            .component::<AgentText>()
            .unwrap()
            .field
            .surface
    }
    pub fn composing(&self) -> bool {
        self.editor().input.marked.is_some()
    }
    pub fn transcript(&self) -> &AgentText {
        self.tree
            .get(TRANSCRIPT)
            .unwrap()
            .component::<AgentText>()
            .unwrap()
    }
    fn text_mut(&mut self, id: ComponentId) -> &mut AgentText {
        self.tree
            .get_mut(id)
            .unwrap()
            .component_mut::<AgentText>()
            .unwrap()
    }
    fn enabled(&mut self, id: ComponentId, on: bool) {
        self.tree
            .get_mut(id)
            .unwrap()
            .component_mut::<ButtonContent>()
            .unwrap()
            .button
            .state
            .disabled = !on;
    }
    pub fn sync(&mut self, s: &RunSnapshot) -> Result<(), &'static str> {
        let destination = s.selection.as_ref().map(|v| v.destination.clone());
        if self.selected != destination {
            if self.composing() {
                return Err(
                    "finish or cancel IME composition before selecting a different session",
                );
            }
            self.tree.focus(None, false);
            let editor = self.tree.remove(COMPOSER).expect("mounted editor");
            let transcript = self.tree.remove(TRANSCRIPT).expect("mounted transcript");
            if let Some(old) = &self.selected {
                self.editors.insert(old.storage_key(), editor);
                self.transcripts.insert(old.storage_key(), transcript);
            }
            let key = destination
                .as_ref()
                .map(ComposerDestination::storage_key)
                .unwrap_or_default();
            let editor = self
                .editors
                .remove(&key)
                .unwrap_or_else(|| Content::new(COMPOSER, AgentText::new(false)));
            let transcript = self
                .transcripts
                .remove(&key)
                .unwrap_or_else(|| Content::new(TRANSCRIPT, AgentText::new(true)));
            self.tree.insert(ROOT, transcript).unwrap();
            self.tree.insert(ROOT, editor).unwrap();
            self.tree
                .reorder(ROOT, &[STATUS, TRANSCRIPT, COMPOSER, ACTIONS])
                .unwrap();
            self.selected = destination;
            self.committed = s.draft.text.clone();
        }
        // Remote deltas never replace preedit, cursor or undo state. A changed authoritative
        // draft (e.g. accepted submit) updates only when there is no active composition.
        if !self.composing() && self.editor().input.text != s.draft.text {
            let editor = self.text_mut(COMPOSER);
            editor.field.surface.input = tekes_ui_controls::TextInput::new(&s.draft.text, false);
        }
        self.committed = s.draft.text.clone();
        let text = s
            .transcript
            .items
            .iter()
            .filter(|i| i.is_visible)
            .map(|i| {
                format!(
                    "{}\n{}",
                    match i.kind {
                        tekes_agent_presentation::TranscriptItemKind::User => "你",
                        _ => "Agent",
                    },
                    i.text
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let field = &mut self.text_mut(TRANSCRIPT).field;
        if field.surface.input.text != text {
            field.surface.input.text = text;
            let len = field.surface.input.len();
            field.surface.input.anchor = field.surface.input.anchor.min(len);
            field.surface.input.caret = field.surface.input.caret.min(len);
        }
        self.tree
            .get_mut(STATUS)
            .unwrap()
            .component_mut::<TextContent>()
            .unwrap()
            .text = format!(
            "连接：{:?}  ·  {}{}",
            s.connection.phase,
            self.selected
                .as_ref()
                .map(|d| match d {
                    ComposerDestination::Run { scope } => scope.session_id.as_str(),
                    _ => "新会话",
                })
                .unwrap_or("未选择"),
            if s.transcript.is_stale {
                " · 历史待同步"
            } else {
                ""
            }
        );
        self.enabled(
            SEND,
            s.can_submit && !self.composing() && !s.run.as_ref().is_some_and(|r| r.can_cancel),
        );
        self.enabled(CANCEL, s.run.as_ref().is_some_and(|r| r.can_cancel));
        self.enabled(EARLIER, s.transcript.has_more_before);
        self.enabled(SESSION_A, !self.composing());
        self.enabled(SESSION_B, !self.composing());
        Ok(())
    }
    pub fn event(&mut self, e: &ContentEvent) -> Vec<Intent> {
        // Explicit Send only. Raw Enter is never promoted into submission (IME safety).
        if self.composing()
            && matches!(e,ContentEvent::PointerDown(p) if !self.tree.get(COMPOSER).unwrap().frame().contains(*p))
        {
            return vec![];
        }
        self.tree.event(e);
        let mut out = vec![];
        if !self.composing() && self.editor().input.text != self.committed {
            self.committed = self.editor().input.text.clone();
            if let Some(destination) = &self.selected {
                out.push(Intent::Edit {
                    destination: destination.clone(),
                    text: self.committed.clone(),
                });
            }
        }
        for id in [
            SEND, CANCEL, EARLIER, LATEST, RECONNECT, SESSION_A, SESSION_B,
        ] {
            let count = self
                .tree
                .get(id)
                .unwrap()
                .component::<ButtonContent>()
                .unwrap()
                .activations;
            let old = self.counts.insert(id, count).unwrap_or(0);
            if count > old {
                match id {
                    SEND if !self.composing() => {
                        if let Some(destination) = &self.selected {
                            out.push(Intent::Send {
                                destination: destination.clone(),
                            });
                        }
                    }
                    CANCEL | EARLIER => {
                        if let Some(ComposerDestination::Run { scope }) = &self.selected {
                            out.push(if id == CANCEL {
                                Intent::Cancel {
                                    scope: scope.clone(),
                                }
                            } else {
                                Intent::Earlier {
                                    scope: scope.clone(),
                                }
                            });
                        }
                    }
                    RECONNECT => out.push(Intent::Reconnect),
                    SESSION_A if !self.composing() => out.push(Intent::Select("a")),
                    SESSION_B if !self.composing() => out.push(Intent::Select("b")),
                    LATEST => self.follow_latest(),
                    _ => {}
                }
            }
        }
        out
    }
    pub fn follow_latest(&mut self) {
        let t = self.text_mut(TRANSCRIPT);
        t.field
            .reading
            .as_mut()
            .unwrap()
            .follow_latest(&mut t.field.surface);
    }
    pub fn render(
        &mut self,
        s: &mut Scene,
        t: &mut dyn ButtonTextRenderer,
        f: Rect,
        c: Rect,
        p: Palette,
    ) -> Result<(), CompositionError> {
        self.tree.render(s, t, f, c, p)
    }
}
