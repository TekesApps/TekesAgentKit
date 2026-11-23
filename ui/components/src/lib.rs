//! Agent-owned composition of public TekesUI controls. No endpoint or Run state machine.
pub mod artifact;
pub mod changes;
pub mod child_run;
pub mod composer;
pub mod files;
pub mod ids;
pub mod layout_mode;
pub mod settings;
pub mod shell;
pub mod sidebar;
pub mod sidechat;
pub mod summary;
pub mod tasks_queue;
pub mod transcript;

pub use artifact::{ArtifactIntent, ArtifactViewKind, ArtifactViewModel, ArtifactViewerSurface};
pub use changes::{ChangeRowModel, ChangesIntent, ChangesViewModel, WorkspaceChangesSurface};
pub use child_run::{ChildRunIntent, ChildRunSurface, ChildRunSurfaceStatus};
pub use composer::{ComposerIntent, ComposerSurface, RunSetupIntent, RunSetupToolbar};
pub use files::{FileTreeRowModel, FilesIntent, WorkspaceFilesSurface};
pub use ids::*;
pub use layout_mode::{COMPACT_WIDTH_BREAKPOINT, ResponsiveWidthMode};
pub use settings::{
    HarnessSettingsIntent, HarnessSettingsSurface, HarnessSettingsViewModel, SettingFieldKindModel,
    SettingFieldModel,
};
pub use shell::AgentContainerShell;
pub use sidebar::{SidebarFilterSelection, SidebarIntent, SidebarRowModel, SidebarSurface};
pub use sidechat::{SideChatIntent, SideChatSurface, SideChatSurfaceAccess};
pub use summary::{SummaryIntent, SummarySurface};
pub use tasks_queue::{
    QueuedPromptViewModel, TaskRowViewModel, TasksQueueIntent, TasksQueueSurface,
    TasksQueueViewModel,
};
pub use transcript::{TranscriptIntent, TranscriptReader, TranscriptSurface};

use std::collections::BTreeMap;
use tekes_agent_presentation::{ComposerDestination, RunScope, RunSnapshot};
use tekes_ui_controls::{
    ButtonTextRenderer, ExternalSelection, ExternalUndo, ReadingController, TextSurface,
    composition::*, layout::MeasuredChild,
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
        let field = if transcript {
            let mut field =
                TextFieldContent::from_surface("会话记录", TextSurface::new("", true, true));
            field.reading = Some(ReadingController::default());
            field.viewport = TextViewport::Fill;
            field.show_metadata = false;
            field
        } else {
            TextFieldContent::new("消息", "")
        };
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
        self.field.arrange(t, f, c);
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
                    .surface()
                    .draw(host, s, p.palette(), self.focused, false);
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
        Some(self.field.surface())
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
            StackContent::horizontal(
                Spacing::Tight.value(),
                buttons
                    .into_iter()
                    .map(|(id, label)| Content::new(id, Button::new(label)))
                    .collect(),
            ),
        );
        let tree = ContentHost::new(Content::new(
            ROOT,
            StackContent::vertical(
                Spacing::Regular.value(),
                vec![
                    Content::new(STATUS, TextContent::new("正在连接")),
                    Content::new(TRANSCRIPT, AgentText::new(true)),
                    Content::new(COMPOSER, AgentText::new(false)),
                    actions,
                ],
            )
            .with_flexible_child(TRANSCRIPT),
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
        self.tree
            .get(COMPOSER)
            .unwrap()
            .component::<AgentText>()
            .unwrap()
            .field
            .surface()
    }
    pub fn composing(&self) -> bool {
        self.editor().marked().is_some()
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
            .component_mut::<Button>()
            .unwrap()
            .set_disabled(!on);
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
        if !self.composing() && self.editor().text() != s.draft.text {
            let editor = self.text_mut(COMPOSER);
            let _ = editor.field.replace_value(
                &s.draft.text,
                ExternalSelection::Preserve,
                ExternalUndo::Reset,
            );
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
        if field.text() != text {
            let _ = field.replace_value(&text, ExternalSelection::Preserve, ExternalUndo::Reset);
        }
        let status_text = format!(
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
        self.tree
            .get_mut(STATUS)
            .unwrap()
            .component_mut::<TextContent>()
            .unwrap()
            .set_text(status_text);
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
        if !self.composing() && self.editor().text() != self.committed {
            self.committed = self.editor().text().to_owned();
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
                .component::<Button>()
                .unwrap()
                .activations();
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
        self.text_mut(TRANSCRIPT).field.follow_latest();
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

pub(crate) mod ui_helpers {
    use tekes_ui_controls::{
        ButtonTextRenderer, ButtonVariant,
        composition::{
            Button, Component, ComponentId, CompositionError, Content, ContentRole,
            ContentSemantics, ContentTheme, CurveSubtreeCapability, IconContent, Lifecycle,
            PanelContent, StackContent, TextContent,
        },
        layout::{Insets, MeasuredChild},
    };
    use tekes_ui_scene::{Quad, Rect, Scene, Size};
    use tekes_ui_tokens::Spacing;

    pub(crate) const CONVERSATION_COLUMN_MAX_WIDTH: f32 = 770.0;

    /// Centers a single child constrained to `max_width` inside the container with outer padding.
    #[derive(Debug)]
    pub(crate) struct CenteredColumn {
        max_width: f32,
        horizontal_padding: f32,
        children: Vec<Content>,
        arranged_child: Option<(ComponentId, u64)>,
    }

    impl CenteredColumn {
        pub(crate) fn content(
            id: ComponentId,
            max_width: f32,
            horizontal_padding: f32,
            child: Content,
        ) -> Content {
            Content::new(
                id,
                Self {
                    max_width,
                    horizontal_padding,
                    children: vec![child],
                    arranged_child: None,
                },
            )
        }

        fn inner_width(&self, total_width: f32) -> f32 {
            if total_width.is_finite() {
                (total_width - self.horizontal_padding * 2.0)
                    .max(1.0)
                    .min(self.max_width)
            } else {
                self.max_width
            }
        }
    }

    impl Component for CenteredColumn {
        fn lifecycle(&mut self, event: Lifecycle) {
            if event == Lifecycle::Unmounted {
                self.arranged_child = None;
            }
        }

        fn host_reveal_passthrough(&self) -> bool {
            self.children.len() == 1
                && self.arranged_child.is_some_and(|identity| {
                    let child = &self.children[0];
                    (child.id(), child.mount_identity()) == identity
                })
        }

        fn fixed_slots(&self) -> bool {
            true
        }

        fn validate(&self) -> Result<(), CompositionError> {
            if self.children.len() != 1 {
                Err(CompositionError::InvalidSlotCount)
            } else {
                Ok(())
            }
        }

        fn curve_subtree_capability(&self) -> CurveSubtreeCapability {
            CurveSubtreeCapability::Recursive
        }

        fn has_stable_hooks(&self) -> bool {
            true
        }

        fn supports_prepared_lifecycle(&self) -> bool {
            true
        }

        fn ignores_pointer_actions(&self) -> bool {
            true
        }

        fn semantics(&self) -> ContentSemantics {
            ContentSemantics::new(ContentRole::Group, "")
        }

        fn children(&self) -> &[Content] {
            &self.children
        }

        fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
            Some(&mut self.children)
        }

        fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
            self.arranged_child = None;
            let inner_w = self.inner_width(available.width);
            let child = self.children[0].measure(text, Size::new(inner_w, available.height));
            let outer_w = if available.width.is_finite() {
                available.width.max(0.0)
            } else {
                child.size.width.min(self.max_width) + self.horizontal_padding * 2.0
            };
            MeasuredChild::new(Size::new(outer_w, child.size.height))
        }

        fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
            let inner_w = self.inner_width(frame.size.width).min(frame.size.width);
            let x = frame.min_x() + ((frame.size.width - inner_w) * 0.5).floor().max(0.0);
            let child_frame = Rect::new(x, frame.min_y(), inner_w, frame.size.height);
            self.children[0].arrange(text, child_frame, clip);
            self.arranged_child = self
                .children
                .first()
                .filter(|child| !child.frame().is_empty())
                .map(|child| (child.id(), child.mount_identity()));
        }
    }

    /// Soft borderless tonal user-message bubble aligned with Swift Tekes `MetalNativeInputBubbleView`.
    #[derive(Debug)]
    pub(crate) struct UserBubble {
        max_width: f32,
        padding: Insets,
        corner_radius: f32,
        children: Vec<Content>,
        arranged_child: Option<(ComponentId, u64)>,
    }

    impl UserBubble {
        pub(crate) fn content(id: ComponentId, child: Content) -> Content {
            Content::new(
                id,
                Self {
                    max_width: 460.0,
                    padding: Insets::new(10.0, 14.0, 10.0, 14.0),
                    corner_radius: 12.0,
                    children: vec![child],
                    arranged_child: None,
                },
            )
        }

        fn width_cap(&self, available_width: f32) -> f32 {
            if !available_width.is_finite() {
                return self.max_width;
            }
            let ratio_cap = (available_width * 0.78).max(200.0).min(available_width);
            available_width.min(self.max_width).min(ratio_cap).max(1.0)
        }
    }

    impl Component for UserBubble {
        fn lifecycle(&mut self, event: Lifecycle) {
            if event == Lifecycle::Unmounted {
                self.arranged_child = None;
            }
        }

        fn host_reveal_passthrough(&self) -> bool {
            self.children.len() == 1
                && self.arranged_child.is_some_and(|identity| {
                    let child = &self.children[0];
                    (child.id(), child.mount_identity()) == identity
                })
        }

        fn fixed_slots(&self) -> bool {
            true
        }

        fn validate(&self) -> Result<(), CompositionError> {
            if self.children.len() != 1 {
                Err(CompositionError::InvalidSlotCount)
            } else {
                Ok(())
            }
        }

        fn curve_subtree_capability(&self) -> CurveSubtreeCapability {
            CurveSubtreeCapability::Recursive
        }

        fn has_stable_hooks(&self) -> bool {
            true
        }

        fn supports_prepared_lifecycle(&self) -> bool {
            true
        }

        fn ignores_pointer_actions(&self) -> bool {
            true
        }

        fn semantics(&self) -> ContentSemantics {
            ContentSemantics::new(ContentRole::Group, "")
        }

        fn children(&self) -> &[Content] {
            &self.children
        }

        fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
            Some(&mut self.children)
        }

        fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
            self.arranged_child = None;
            let cap_w = self.width_cap(available.width);
            let pad_h = self.padding.left + self.padding.right;
            let pad_v = self.padding.top + self.padding.bottom;
            let inner_max_w = (cap_w - pad_h).max(1.0);
            let child = self.children[0].measure(text, Size::new(inner_max_w, available.height));
            let text_w = child.size.width.ceil().min(inner_max_w);
            let bubble_w = (text_w + pad_h).min(cap_w);
            let bubble_h = child.size.height + pad_v;
            MeasuredChild::new(Size::new(bubble_w, bubble_h))
        }

        fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
            let inner_w = (frame.size.width - self.padding.left - self.padding.right).max(0.0);
            let inner_h = (frame.size.height - self.padding.top - self.padding.bottom).max(0.0);
            let child_frame = Rect::new(
                frame.min_x() + self.padding.left,
                frame.min_y() + self.padding.top,
                inner_w,
                inner_h,
            );
            self.children[0].arrange(text, child_frame, clip);
            self.arranged_child = self
                .children
                .first()
                .filter(|child| !child.frame().is_empty())
                .map(|child| (child.id(), child.mount_identity()));
        }

        fn paint(
            &mut self,
            scene: &mut Scene,
            _: &mut dyn ButtonTextRenderer,
            frame: Rect,
            _: Rect,
            theme: ContentTheme,
        ) {
            let palette = theme.palette();
            let fill = palette
                .color("user-message-surface")
                .or_else(|| palette.color("surface-2"))
                .or_else(|| palette.color("surface-1"))
                .or_else(|| palette.color("surface-0"))
                .unwrap_or_else(|| tekes_ui_tokens::Rgba::new(0.94, 0.94, 0.96, 1.0));
            scene.push_quad(Quad::new(frame, fill).with_corner_radius(self.corner_radius));
        }
    }

    pub(crate) fn heading_text(text: impl Into<String>) -> TextContent {
        TextContent::heading(text)
    }

    pub(crate) fn label_text(text: impl Into<String>) -> TextContent {
        TextContent::label(text)
    }

    pub(crate) fn caption_text(text: impl Into<String>) -> TextContent {
        TextContent::caption(text)
    }

    pub(crate) fn secondary_text(text: impl Into<String>) -> TextContent {
        TextContent::secondary(text)
    }

    pub(crate) fn plain_icon(id: ComponentId, icon_name: &str) -> Content {
        let icon = IconContent::new(icon_name)
            .unwrap_or_else(|_| IconContent::new("tekes-file-generic").expect("fallback icon"));
        Content::new(id, icon)
    }

    pub(crate) fn icon_button(
        icon_id: ComponentId,
        icon_name: &str,
        accessible_name: &str,
    ) -> Button {
        let mut btn =
            Button::new(plain_icon(icon_id, icon_name)).with_accessible_name(accessible_name);
        btn.set_variant(ButtonVariant::Quiet);
        btn.set_transparent_chrome(true);
        btn.set_toolbar_icon_slot(true);
        btn
    }

    pub(crate) fn icon_text_button(
        stack_id: ComponentId,
        icon_id: ComponentId,
        text_id: ComponentId,
        icon_name: &str,
        label: impl Into<String>,
    ) -> Button {
        let text_str = label.into();
        let row = Content::new(
            stack_id,
            StackContent::horizontal(
                Spacing::Tight.value(),
                vec![
                    plain_icon(icon_id, icon_name),
                    Content::new(text_id, label_text(text_str.clone())),
                ],
            ),
        );
        let mut btn = Button::new(row).with_accessible_name(text_str);
        btn.set_variant(ButtonVariant::Quiet);
        btn.set_content_alignment(tekes_ui_tokens::proposal::layout::MainAxisDistribution::Start);
        btn
    }

    pub(crate) fn icon_row(
        row_id: ComponentId,
        icon_id: ComponentId,
        text_id: ComponentId,
        icon_name: &str,
        label: impl Into<String>,
    ) -> Content {
        Content::new(
            row_id,
            StackContent::horizontal(
                Spacing::Tight.value(),
                vec![
                    plain_icon(icon_id, icon_name),
                    Content::new(text_id, TextContent::new(label)),
                ],
            )
            .with_flexible_child(text_id),
        )
    }

    pub(crate) fn secondary_icon_row(
        row_id: ComponentId,
        icon_id: ComponentId,
        text_id: ComponentId,
        icon_name: &str,
        label: impl Into<String>,
    ) -> Content {
        Content::new(
            row_id,
            StackContent::horizontal(
                Spacing::Tight.value(),
                vec![
                    plain_icon(icon_id, icon_name),
                    Content::new(text_id, secondary_text(label)),
                ],
            )
            .with_flexible_child(text_id),
        )
    }

    pub(crate) fn card_panel(id: ComponentId, child: Content, padding: Insets) -> Content {
        let mut panel =
            PanelContent::new(child).expect("PanelContent children must satisfy curve rules");
        let _ = panel.set_padding(Some(padding));
        Content::new(id, panel)
    }
}
