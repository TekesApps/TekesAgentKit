//! Agent Composer's optional status row, outside the rounded input card.
//! Facts and refresh authority arrive explicitly; the SDK owns controls/input.
use crate::ComposerDestination;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tekes_ui_controls::{
    ButtonTextRenderer,
    composition::*,
    extended::Axis,
    layout::{AlignmentOverride, ContainerLayout, Insets, MeasuredChild, layout_measured_children},
    paint::ButtonVariant,
};
use tekes_ui_scene::{Rect, Scene, Size};
use tekes_ui_tokens::{Spacing, proposal};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComposerNoticeState {
    pub text: String,
    pub can_refresh: bool,
    pub refreshing: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComposerNoticeInput {
    /// Monotonic display revision for this row's lifetime, independent of RPC identity.
    pub revision: u64,
    /// Connection authority captured with the notice facts, not sampled at click time.
    pub connection_epoch: u64,
    pub destination: ComposerDestination,
    pub notice: Option<ComposerNoticeState>,
    pub enabled: bool,
}
impl ComposerNoticeInput {
    fn refresh_enabled(&self) -> bool {
        self.enabled
            && self
                .notice
                .as_ref()
                .is_some_and(|n| n.can_refresh && !n.refreshing)
    }
}
#[derive(Clone, Debug)]
pub struct ComposerNoticeLabels {
    pub name: String,
    pub refresh: String,
}
impl Default for ComposerNoticeLabels {
    fn default() -> Self {
        Self {
            name: "消息输入提示".into(),
            refresh: "刷新".into(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct ComposerNoticeRefresh {
    input: ComposerNoticeInput,
    owner: Arc<()>,
    claimed: Arc<AtomicBool>,
}
impl ComposerNoticeRefresh {
    pub fn source(&self) -> &ComposerNoticeInput {
        &self.input
    }
}
#[derive(Debug)]
struct RefreshAction {
    button: Button,
    source: Option<(ComposerNoticeInput, Arc<()>)>,
    pending: Vec<ComposerNoticeRefresh>,
}
impl RefreshAction {
    fn capture(&mut self, response: ContentResponse) -> ContentResponse {
        if response.activated
            && let Some((input, owner)) = &self.source
            && input.refresh_enabled()
        {
            self.pending.push(ComposerNoticeRefresh {
                input: input.clone(),
                owner: owner.clone(),
                claimed: Arc::new(AtomicBool::new(false)),
            });
        }
        response
    }
}
impl Component for RefreshAction {
    fn measure(&mut self, t: &mut dyn ButtonTextRenderer, s: Size) -> MeasuredChild {
        self.button.measure(t, s)
    }
    fn arrange(&mut self, t: &mut dyn ButtonTextRenderer, f: Rect, c: Rect) {
        self.button.arrange(t, f, c)
    }
    fn paint(
        &mut self,
        s: &mut Scene,
        t: &mut dyn ButtonTextRenderer,
        f: Rect,
        c: Rect,
        theme: ContentTheme,
    ) {
        self.button.paint(s, t, f, c, theme)
    }
    fn event(&mut self, e: &ContentEvent) -> ContentResponse {
        let r = self.button.event(e);
        self.capture(r)
    }
    fn accessibility_action(
        &mut self,
        p: Option<usize>,
        a: &AccessibilityAction,
    ) -> ContentResponse {
        let r = self.button.accessibility_action(p, a);
        self.capture(r)
    }
    fn focusable(&self) -> bool {
        self.button.focusable()
    }
    fn set_focus(&mut self, f: bool, v: bool) {
        self.button.set_focus(f, v)
    }
    fn semantics(&self) -> ContentSemantics {
        self.button.semantics()
    }
    fn focus_decoration_above_siblings(&self) -> bool {
        self.button.focus_decoration_above_siblings()
    }
}
#[derive(Debug)]
pub struct ComposerNotice {
    root: ComponentId,
    children: Vec<Content>,
    retained_refresh: Option<Content>,
    input: Option<ComposerNoticeInput>,
    revision: u64,
    owner: Arc<()>,
    labels: ComposerNoticeLabels,
}
impl ComposerNotice {
    pub fn new(namespace: u32, labels: ComposerNoticeLabels) -> Self {
        let base = (namespace as u64) << 32;
        let mut button = Button::new(&labels.refresh);
        button.variant = ButtonVariant::Quiet;
        button.button.state.disabled = true;
        Self {
            root: ComponentId(base),
            children: vec![Content::new(
                ComponentId(base + 1),
                TextContent {
                    text: String::new(),
                    role: proposal::TextRole::UiCaption,
                },
            )],
            retained_refresh: Some(
                Content::new(
                    ComponentId(base + 2),
                    RefreshAction {
                        button,
                        source: None,
                        pending: vec![],
                    },
                )
                .preserving_focus(),
            ),
            input: None,
            revision: 0,
            owner: Arc::new(()),
            labels,
        }
    }
    pub fn id(&self) -> ComponentId {
        self.root
    }
    pub fn text_id(&self) -> ComponentId {
        ComponentId(self.root.0 + 1)
    }
    pub fn refresh_id(&self) -> ComponentId {
        ComponentId(self.root.0 + 2)
    }
    pub fn displayed_input(&self) -> Option<&ComposerNoticeInput> {
        self.input.as_ref()
    }
    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        ContentHost::new(Content::new(self.root, self))
    }
    fn action_mut(&mut self) -> &mut RefreshAction {
        self.retained_refresh
            .as_mut()
            .unwrap_or_else(|| self.children.last_mut().unwrap())
            .component_mut::<RefreshAction>()
            .unwrap()
    }
    /// Retire a destination's presentation without releasing the original controls.
    pub(crate) fn clear(&mut self) {
        if self.children.len() == 2 {
            self.retained_refresh = self.children.pop();
        }
        self.input = None;
        self.owner = Arc::new(());
        self.action_mut().source = None;
    }
    pub fn update(&mut self, input: &ComposerNoticeInput) -> Result<(), &'static str> {
        input
            .destination
            .validate()
            .map_err(|_| "invalid notice destination")?;
        if input.revision == 0
            || input.revision < self.revision
            || (input.revision == self.revision && self.input.as_ref() != Some(input))
        {
            return Err("stale or conflicting Composer notice input");
        }
        if self.input.as_ref() != Some(input) {
            self.owner = Arc::new(());
        }
        let show_refresh = input.notice.as_ref().is_some_and(|n| n.can_refresh);
        if show_refresh {
            if let Some(button) = self.retained_refresh.take() {
                self.children.push(button);
            }
        } else if self.children.len() == 2 {
            self.retained_refresh = self.children.pop();
        }
        self.children[0]
            .component_mut::<TextContent>()
            .unwrap()
            .text = input
            .notice
            .as_ref()
            .map_or_else(String::new, |n| n.text.clone());
        let owner = self.owner.clone();
        let action = self.action_mut();
        action.button.button.state.disabled = !input.refresh_enabled();
        action.source = Some((input.clone(), owner));
        self.revision = input.revision;
        self.input = Some(input.clone());
        Ok(())
    }
    /// Immutable event-time requests; drain before updating the row. Stale
    /// requests remain inspectable but cannot be claimed against a new frame.
    pub fn take_refreshes(&mut self) -> Vec<ComposerNoticeRefresh> {
        std::mem::take(&mut self.action_mut().pending)
    }
    pub fn claim_refresh(&self, request: &ComposerNoticeRefresh) -> Result<(), &'static str> {
        if !Arc::ptr_eq(&self.owner, &request.owner)
            || self.input.as_ref() != Some(&request.input)
            || !request.input.refresh_enabled()
        {
            return Err("Composer notice refresh source retired");
        }
        request
            .claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "Composer notice refresh already claimed")?;
        Ok(())
    }
    fn bounds(frame: Rect, clip: Option<Rect>) -> ContainerLayout {
        ContainerLayout::new(
            frame,
            Insets::ZERO,
            Insets::new(0., Spacing::Relaxed.value(), 0., Spacing::Relaxed.value()),
            clip,
        )
    }
    fn measurements(
        &mut self,
        text: &mut dyn ButtonTextRenderer,
        width: f32,
    ) -> Vec<MeasuredChild> {
        let width = width.max(0.);
        let button = (self.children.len() == 2)
            .then(|| self.children[1].measure(text, Size::new(width, 1e8)));
        let gap = if button.is_some() {
            Spacing::Regular.value()
        } else {
            0.
        };
        let label_width =
            (width - button.as_ref().map_or(0., |b| b.size.width.min(width)) - gap).max(0.);
        let mut label = self.children[0].measure(text, Size::new(label_width, 1e8));
        label.size.width = label.size.width.min(label_width);
        label.size.height = label.size.height.max(
            text.measure("Mg", proposal::TextRole::UiCaption, label_width)
                .height,
        );
        let mut rows = vec![label];
        if let Some(mut button) = button {
            button.size.width = button.size.width.min(width);
            rows.push(button);
        }
        rows
    }
}
impl Component for ComposerNotice {
    fn active(&self) -> bool {
        self.input.as_ref().is_some_and(|i| i.notice.is_some())
    }
    fn children(&self) -> &[Content] {
        &self.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        if !self.active() {
            return MeasuredChild::new(Size::default());
        }
        let bounds = Self::bounds(Rect::new(0., 0., available.width, available.height), None);
        let rows = self.measurements(text, bounds.content().size.width);
        MeasuredChild::new(Size::new(
            available.width,
            rows.iter().map(|m| m.size.height).fold(0., f32::max),
        ))
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        if !self.active() {
            return;
        }
        let bounds = Self::bounds(frame, Some(clip));
        let rows = self.measurements(text, bounds.content().size.width);
        let positions = layout_measured_children(
            bounds.content(),
            Axis::Horizontal,
            &rows,
            if rows.len() == 2 {
                Spacing::Regular.value()
            } else {
                0.
            },
            proposal::layout::ROW,
            AlignmentOverride {
                main_axis: Some(proposal::layout::MainAxisDistribution::SpaceBetween),
                cross_axis: Some(proposal::layout::CrossAxisAlignment::Center),
            },
        );
        for (node, position) in self.children.iter_mut().zip(positions) {
            node.arrange(
                text,
                position.frame,
                bounds.visible_clip().unwrap_or_default(),
            );
        }
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.labels.name)
    }
}
