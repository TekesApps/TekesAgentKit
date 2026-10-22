//! Transient feedback gestures over committed records; no network or optimistic state.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tekes_agent_contract::{SessionMessageFeedback, SessionMessageFeedbackRating as Rating};
use tekes_agent_presentation::{FeedbackChange, FeedbackTargetCapture};
use tekes_ui_controls::{
    ButtonTextRenderer, TextSurface, TextSurfacePresentation,
    composition::*,
    extended::{Axis, Stack},
    layout::{AlignmentOverride, MeasuredChild, layout_measured_children},
    paint::ButtonVariant,
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::{Spacing, proposal};

#[derive(Clone, Debug, PartialEq)]
pub struct FeedbackControlState {
    pub target: FeedbackTargetCapture,
    pub publication: u64,
    pub connection_epoch: u64,
    pub current: Option<SessionMessageFeedback>,
    pub put_available: bool,
    pub delete_available: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub enum FeedbackGestureAction {
    Change(FeedbackChange),
    Refresh,
}
#[derive(Clone, Debug)]
pub struct FeedbackGesture {
    target: FeedbackTargetCapture,
    action: FeedbackGestureAction,
    admission: Arc<AtomicBool>,
}
impl FeedbackGesture {
    pub fn target(&self) -> &FeedbackTargetCapture {
        &self.target
    }
    pub fn action(&self) -> &FeedbackGestureAction {
        &self.action
    }
    /// A bridge may dispatch a captured gesture only once, including refresh/no-op.
    pub fn claim(&self) -> bool {
        !self.admission.swap(true, Ordering::AcqRel)
    }
    fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.admission, &other.admission)
    }
}
#[derive(Clone, Debug)]
pub enum FeedbackControlResult {
    Applied(FeedbackControlState),
    Conflict(FeedbackControlState),
    Rejected {
        state: Option<FeedbackControlState>,
        message: String,
    },
    NeedsRefresh(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedbackLabels {
    pub group: String,
    pub positive: String,
    pub negative: String,
    pub edit_note: String,
    pub note: String,
    pub editor_title: String,
    pub save: String,
    pub close: String,
    pub refresh: String,
    pub pending: String,
    pub unloaded: String,
    pub conflict: String,
    pub failure: String,
    pub uncertain: String,
}
impl Default for FeedbackLabels {
    fn default() -> Self {
        Self {
            group: "回复评价".into(),
            positive: "有帮助".into(),
            negative: "没有帮助".into(),
            edit_note: "备注".into(),
            note: "评价备注".into(),
            editor_title: "评价".into(),
            save: "保存".into(),
            close: "取消".into(),
            refresh: "刷新评价".into(),
            pending: "正在处理评价".into(),
            unloaded: "评价尚未加载".into(),
            conflict: "评价已在别处修改，已显示最新状态".into(),
            failure: "无法保存评价".into(),
            uncertain: "结果尚未确认，请刷新评价后再操作".into(),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct FeedbackIds {
    pub root: ComponentId,
    pub positive: ComponentId,
    pub negative: ComponentId,
    pub edit_note: ComponentId,
    pub editor: ComponentId,
    pub save: ComponentId,
    pub close: ComponentId,
    pub refresh: ComponentId,
    status: ComponentId,
    title: ComponentId,
}
/// One stable response per instance. Mount alongside its leading output footnote.
/// All editing, marked text, undo, layout styles and focus are TekesUI-owned.
#[derive(Debug)]
pub struct TurnFeedback {
    ids: FeedbackIds,
    next: u64,
    labels: FeedbackLabels,
    state: Option<FeedbackControlState>,
    target: Option<FeedbackTargetCapture>,
    rows: StackContent,
    pending: Option<FeedbackGesture>,
    needs_refresh: bool,
    editor_open: bool,
    notice: String,
    diagnostic: Option<String>,
}
impl TurnFeedback {
    pub fn new(namespace: u32, labels: FeedbackLabels) -> Self {
        let root = ComponentId((namespace as u64) << 32);
        let mut value = Self {
            ids: FeedbackIds {
                root,
                positive: root,
                negative: root,
                edit_note: root,
                editor: root,
                save: root,
                close: root,
                refresh: root,
                status: root,
                title: root,
            },
            next: 16,
            labels,
            state: None,
            target: None,
            rows: StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![],
            },
            pending: None,
            needs_refresh: false,
            editor_open: false,
            notice: String::new(),
            diagnostic: None,
        };
        value.rekey();
        value.render();
        value
    }
    /// Bind a proven response before the initial read. Cold/failed is never an
    /// empty committed record; only explicit Refresh is available until loaded.
    pub fn bind_target(&mut self, target: FeedbackTargetCapture) -> Result<(), &'static str> {
        if self.target.as_ref().is_some_and(|old| old != &target) {
            return Err("feedback target already bound");
        }
        self.target = Some(target);
        self.render();
        Ok(())
    }
    pub fn ids(&self) -> FeedbackIds {
        self.ids
    }
    pub fn target(&self) -> Option<&FeedbackTargetCapture> {
        self.target.as_ref()
    }
    pub fn invalidate_authority(&mut self) {
        if self.needs_refresh {
            return;
        }
        self.pending = None;
        self.needs_refresh = true;
        if self.state.is_some() {
            self.notice = self.labels.uncertain.clone();
        }
        self.rekey();
        self.render();
    }
    pub fn state(&self) -> Option<&FeedbackControlState> {
        self.state.as_ref()
    }
    pub fn is_open(&self) -> bool {
        self.editor_open
    }
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn needs_refresh(&self) -> bool {
        self.needs_refresh
    }
    pub fn diagnostic(&self) -> Option<&str> {
        self.diagnostic.as_deref()
    }
    pub fn notice(&self) -> &str {
        &self.notice
    }
    pub fn editor(&self) -> Option<&TextSurface> {
        self.rows
            .children
            .iter()
            .find(|n| n.id() == self.ids.editor)?
            .component::<TextFieldContent>()
            .map(|f| &f.surface)
    }
    pub fn is_composing(&self) -> bool {
        self.editor().is_some_and(|f| f.input.marked.is_some())
    }
    /// Only committed input. Never replaces an open editor's text/selection/undo.
    pub fn update(&mut self, state: FeedbackControlState) -> Result<(), &'static str> {
        validate_state(&state)?;
        if self
            .target
            .as_ref()
            .is_some_and(|target| target != &state.target)
        {
            return Err("feedback target changed");
        }
        if let Some(old) = &self.state {
            if old.target != state.target {
                return Err("feedback response changed; mount a new control");
            }
            if state.publication < old.publication || state.connection_epoch < old.connection_epoch
            {
                return Err("stale feedback controls");
            }
            if state.publication == old.publication && state != *old {
                return Err("conflicting feedback controls");
            }
        }
        if self.state.as_ref() == Some(&state) {
            return Ok(());
        }
        // A newer committed read supersedes this mount's old operation receipt.
        // The original request may finish, but cannot overwrite the fresh frame.
        self.pending = None;
        self.diagnostic = None;
        self.adopt(state);
        self.notice.clear();
        self.render();
        Ok(())
    }
    fn adopt(&mut self, state: FeedbackControlState) {
        if state.current.is_none() {
            self.editor_open = false;
        }
        self.target = Some(state.target.clone());
        self.state = Some(state);
        self.needs_refresh = false;
        self.rekey();
    }
    fn rekey(&mut self) {
        assert!(
            self.next < u32::MAX as u64 - 16,
            "feedback namespace exhausted"
        );
        let base = self.ids.root.0 + self.next;
        self.next += 16;
        self.ids.positive = ComponentId(base);
        self.ids.negative = ComponentId(base + 1);
        self.ids.edit_note = ComponentId(base + 2);
        // Preserve the mounted upstream editor during status/authority changes.
        if !self.editor_open {
            self.ids.editor = ComponentId(base + 3);
        }
        self.ids.save = ComponentId(base + 4);
        self.ids.close = ComponentId(base + 5);
        self.ids.refresh = ComponentId(base + 6);
        self.ids.status = ComponentId(base + 7);
        self.ids.title = ComponentId(base + 8);
    }
    pub fn complete(&mut self, gesture: &FeedbackGesture, result: FeedbackControlResult) -> bool {
        if !self.pending.as_ref().is_some_and(|p| p.same(gesture)) {
            return false;
        }
        let next = match &result {
            FeedbackControlResult::Applied(s) | FeedbackControlResult::Conflict(s) => Some(s),
            FeedbackControlResult::Rejected { state, .. } => state.as_ref(),
            _ => None,
        };
        if let Some(next) = next {
            if validate_state(next).is_err() {
                return false;
            }
            if self.target.as_ref() != Some(&next.target)
                || self.state.as_ref().is_some_and(|old| {
                    next.publication < old.publication
                        || next.connection_epoch < old.connection_epoch
                })
            {
                return false;
            }
        }
        self.pending = None;
        self.diagnostic = None;
        match result {
            FeedbackControlResult::Applied(state) => {
                if matches!(gesture.action(), FeedbackGestureAction::Change(_)) {
                    self.editor_open = false;
                }
                self.adopt(state);
                self.notice.clear();
            }
            FeedbackControlResult::Conflict(state) => {
                self.adopt(state);
                self.notice = self.labels.conflict.clone();
            }
            FeedbackControlResult::Rejected { state, message } => {
                if let Some(state) = state {
                    self.adopt(state)
                } else {
                    self.needs_refresh = true;
                    self.rekey()
                }
                self.diagnostic = Some(message);
                self.notice = self.labels.failure.clone();
            }
            FeedbackControlResult::NeedsRefresh(message) => {
                self.needs_refresh = true;
                self.rekey();
                self.diagnostic = Some(message);
                self.notice = self.labels.uncertain.clone();
            }
        }
        self.render();
        true
    }
    pub fn take_intents(&mut self, actions: &[ContentAction]) -> Vec<FeedbackGesture> {
        self.render();
        if self.target.is_none() || self.is_composing() {
            return vec![];
        }
        for action in actions
            .iter()
            .filter(|a| a.kind == ContentActionKind::Activate)
        {
            if action.target == self.ids.close && self.editor_open {
                self.editor_open = false;
                self.rekey();
                self.render();
                return vec![];
            }
            if self.pending.is_some() {
                continue;
            }
            if action.target == self.ids.refresh {
                let gesture = FeedbackGesture {
                    target: self.target.clone().unwrap(),
                    action: FeedbackGestureAction::Refresh,
                    admission: Arc::new(AtomicBool::new(false)),
                };
                self.pending = Some(gesture.clone());
                self.notice.clear();
                self.render();
                return vec![gesture];
            }
            let Some(state) = self.state.as_ref() else {
                continue;
            };
            let action = if self.needs_refresh {
                None
            } else if action.target == self.ids.edit_note
                && state.current.is_some()
                && state.put_available
                && !self.editor_open
            {
                self.editor_open = true;
                self.notice.clear();
                self.rekey();
                self.render();
                return vec![];
            } else if action.target == self.ids.save && self.editor_open && state.put_available {
                let text = self
                    .editor()
                    .unwrap()
                    .input
                    .text
                    .trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
                    .to_string();
                Some(FeedbackGestureAction::Change(FeedbackChange::SetNote(
                    (!text.is_empty()).then_some(text),
                )))
            } else if !self.editor_open
                && (action.target == self.ids.positive || action.target == self.ids.negative)
            {
                let rating = if action.target == self.ids.positive {
                    Rating::Positive
                } else {
                    Rating::Negative
                };
                let enabled = if state.current.as_ref().is_some_and(|v| v.rating == rating) {
                    state.delete_available
                } else {
                    state.put_available
                };
                enabled.then_some(FeedbackGestureAction::Change(FeedbackChange::ToggleRating(
                    rating,
                )))
            } else {
                None
            };
            if let Some(action) = action {
                let gesture = FeedbackGesture {
                    target: state.target.clone(),
                    action,
                    admission: Arc::new(AtomicBool::new(false)),
                };
                self.pending = Some(gesture.clone());
                self.notice.clear();
                self.render();
                return vec![gesture];
            }
        }
        vec![]
    }
    fn render(&mut self) {
        let mut old = std::mem::take(&mut self.rows.children);
        let mut leaves = Vec::new();
        for mut node in old.drain(..) {
            if let Some(row) = node.component_mut::<StackContent>() {
                leaves.append(&mut row.children);
            } else if let Some(footer) = node.component_mut::<FeedbackFooter>() {
                leaves.append(&mut footer.rows.children);
            } else {
                leaves.push(node);
            }
        }
        old = leaves;
        let current = self.state.as_ref().and_then(|s| s.current.as_ref());
        let busy = self.pending.is_some() || self.needs_refresh;
        for (id, label, rating) in [
            (self.ids.positive, &self.labels.positive, Rating::Positive),
            (self.ids.negative, &self.labels.negative, Rating::Negative),
        ] {
            let mut node = retained_button(&mut old, id, label);
            let button = node.component_mut::<ButtonContent>().unwrap();
            button.button.state.selected = current.is_some_and(|v| v.rating == rating);
            button.button.state.disabled = busy
                || self.editor_open
                || self.state.as_ref().is_none_or(|s| {
                    if current.is_some_and(|v| v.rating == rating) {
                        !s.delete_available
                    } else {
                        !s.put_available
                    }
                });
            self.rows.children.push(node);
        }
        if let Some(current) = current {
            let mut note = retained_button(
                &mut old,
                self.ids.edit_note,
                current.note.as_deref().unwrap_or(&self.labels.edit_note),
            );
            note.component_mut::<ButtonContent>()
                .unwrap()
                .button
                .state
                .selected = current.note.is_some();
            note.component_mut::<ButtonContent>()
                .unwrap()
                .button
                .state
                .disabled =
                busy || self.editor_open || self.state.as_ref().is_none_or(|s| !s.put_available);
            self.rows.children.push(note);
        }
        let rating_buttons = std::mem::take(&mut self.rows.children);
        self.rows
            .children
            .push(horizontal(ComponentId(self.ids.root.0 + 1), rating_buttons));
        if self.editor_open {
            self.rows.children.push(Content::new(
                self.ids.title,
                TextContent {
                    text: self.labels.editor_title.clone(),
                    role: proposal::TextRole::UiLabel,
                },
            ));
            let mut node = old
                .iter()
                .position(|n| n.id() == self.ids.editor)
                .map(|i| old.remove(i))
                .unwrap_or_else(|| {
                    let text = current.and_then(|v| v.note.as_deref()).unwrap_or("");
                    let mut field = TextFieldContent::new(&self.labels.note, text);
                    field.surface = TextSurface::new(text, true, false);
                    field.surface.presentation = TextSurfacePresentation::Content;
                    field.surface.set_role(proposal::TextRole::UiBody);
                    field.submit_policy = TextSubmitPolicy::None;
                    // Named Agent form geometry from ClientKit's note editor;
                    // shaping, inner scrolling, selection and IME stay in SDK.
                    field.viewport = TextViewport::Fixed(120.);
                    Content::new(self.ids.editor, field)
                });
            node.component_mut::<TextFieldContent>()
                .unwrap()
                .surface
                .input
                .disabled = false;
            self.rows.children.push(node);
            let status = if self.pending.is_some() {
                &self.labels.pending
            } else {
                &self.notice
            };
            if !status.is_empty() {
                self.rows.children.push(Content::new(
                    self.ids.status,
                    TextContent {
                        text: status.clone(),
                        role: proposal::TextRole::UiCaption,
                    },
                ));
            }
            let mut save = retained_button(&mut old, self.ids.save, &self.labels.save);
            save.component_mut::<ButtonContent>().unwrap().variant = ButtonVariant::Primary;
            save.component_mut::<ButtonContent>()
                .unwrap()
                .button
                .state
                .disabled =
                busy || self.is_composing() || self.state.as_ref().is_none_or(|s| !s.put_available);
            let mut close = retained_button(&mut old, self.ids.close, &self.labels.close);
            close
                .component_mut::<ButtonContent>()
                .unwrap()
                .button
                .state
                .disabled = self.is_composing();
            self.rows.children.push(Content::new(
                ComponentId(self.ids.root.0 + 2),
                FeedbackFooter {
                    rows: StackContent {
                        model: Stack {
                            axis: Axis::Horizontal,
                            gap: Spacing::Tight.value(),
                        },
                        flexible_child: None,
                        children: vec![close, save],
                    },
                },
            ));
        }
        let mut refresh = retained_button(&mut old, self.ids.refresh, &self.labels.refresh);
        refresh
            .component_mut::<ButtonContent>()
            .unwrap()
            .button
            .state
            .disabled = self.pending.is_some() || self.is_composing() || self.target.is_none();
        self.rows.children.push(refresh);
        let text = if self.pending.is_some() {
            &self.labels.pending
        } else if self.notice.is_empty() && self.state.is_none() {
            &self.labels.unloaded
        } else {
            &self.notice
        };
        if !text.is_empty() && !self.editor_open {
            self.rows.children.push(Content::new(
                self.ids.status,
                TextContent {
                    text: text.clone(),
                    role: proposal::TextRole::UiCaption,
                },
            ));
        }
    }
    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let root = self.ids.root;
        let mut host = ContentHost::new(Content::new(root, self))?;
        host.set_action_events(true);
        Ok(host)
    }
}
impl Component for TurnFeedback {
    fn children(&self) -> &[Content] {
        &self.rows.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.rows.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        self.render();
        let width = if self.editor_open {
            available.width.min(400.)
        } else {
            available.width
        };
        self.rows.measure(text, Size::new(width, available.height))
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        let width = if self.editor_open {
            frame.size.width.min(400.)
        } else {
            frame.size.width
        };
        self.rows.arrange(
            text,
            Rect::new(frame.min_x(), frame.min_y(), width, frame.size.height),
            clip,
        )
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.labels.group)
    }
}

/// Business form order/alignment only; original SDK buttons retain hit, focus,
/// keyboard and AX. This is not a popup/overlay implementation.
#[derive(Debug)]
struct FeedbackFooter {
    rows: StackContent,
}
impl Component for FeedbackFooter {
    fn children(&self) -> &[Content] {
        &self.rows.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.rows.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        let measured = self.rows.measure(text, available);
        MeasuredChild::new(Size::new(available.width, measured.size.height))
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        let measured = self
            .rows
            .children
            .iter_mut()
            .map(|c| c.measure(text, frame.size))
            .collect::<Vec<_>>();
        let placed = layout_measured_children(
            frame,
            Axis::Horizontal,
            &measured,
            self.rows.model.gap,
            proposal::layout::ROW,
            AlignmentOverride {
                main_axis: Some(proposal::layout::MainAxisDistribution::End),
                ..Default::default()
            },
        );
        for (child, placement) in self.rows.children.iter_mut().zip(placed) {
            child.arrange(text, placement.frame, clip);
        }
    }
}

fn retained_button(old: &mut Vec<Content>, id: ComponentId, label: &str) -> Content {
    let mut node = old
        .iter()
        .position(|n| n.id() == id)
        .map(|i| old.remove(i))
        .unwrap_or_else(|| Content::new(id, ButtonContent::new(label)).preserving_focus());
    node.component_mut::<ButtonContent>().unwrap().label = label.into();
    node
}

fn horizontal(id: ComponentId, children: Vec<Content>) -> Content {
    Content::new(
        id,
        StackContent {
            model: Stack {
                axis: Axis::Horizontal,
                gap: Spacing::Tight.value(),
            },
            flexible_child: None,
            children,
        },
    )
}

fn validate_state(state: &FeedbackControlState) -> Result<(), &'static str> {
    if state.publication == 0 {
        return Err("feedback publication is empty");
    }
    if let Some(record) = &state.current {
        record.validate().map_err(|_| "invalid feedback record")?;
        if record.message_id != state.target.message_id() {
            return Err("feedback identity mismatch");
        }
    }
    Ok(())
}
