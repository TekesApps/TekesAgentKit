//! User prose lane; generic text/selection/IME/painting remain in TekesUI.
use tekes_agent_presentation::{
    RunScope, TranscriptDelivery, TranscriptInput, TranscriptItemKind, TranscriptMessageKey,
};
use tekes_ui_controls::{
    ButtonTextRenderer, DraftRevision, ExternalSelection, ExternalUndo, ExternalValueResult,
    composition::*,
    extended::Axis,
    feedback_view::SurfaceView,
    layout::{AlignmentOverride, MeasuredChild, layout_measured_children},
};
use tekes_ui_scene::{Rect, Scene, Size};
use tekes_ui_tokens::{Spacing, proposal};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Key {
    Submission(String),
    NativeEntry(String),
    Item(String),
}
impl From<TranscriptMessageKey<'_>> for Key {
    fn from(value: TranscriptMessageKey<'_>) -> Self {
        match value {
            TranscriptMessageKey::Submission(s) => Self::Submission(s.into()),
            TranscriptMessageKey::NativeEntry(s) => Self::NativeEntry(s.into()),
            TranscriptMessageKey::Item(s) => Self::Item(s.into()),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct InputMessageIds {
    pub root: ComponentId,
    pub bubble: ComponentId,
    pub body: ComponentId,
    pub footnote: ComponentId,
}
impl InputMessageIds {
    pub fn new(namespace: u32) -> Self {
        let base = (namespace as u64) << 32;
        Self {
            root: ComponentId(base),
            bubble: ComponentId(base + 1),
            body: ComponentId(base + 2),
            footnote: ComponentId(base + 3),
        }
    }
}
#[derive(Debug)]
struct Bubble {
    surface: SurfaceView,
    children: Vec<Content>,
}
impl Bubble {
    fn insets(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> (f32, f32) {
        let frame = Rect::new(
            0.,
            0.,
            available.width,
            available.height.max(available.width),
        );
        let layout = self.surface.render_at_scale(
            &mut Scene::new(),
            frame,
            None,
            tekes_ui_tokens::Palette::from(tekes_ui_tokens::Theme::LIGHT),
            true,
            text.backing_scale(),
        );
        (
            layout.content.min_x() - frame.min_x(),
            layout.content.min_y() - frame.min_y(),
        )
    }
}
impl Component for Bubble {
    fn children(&self) -> &[Content] {
        &self.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        let (x, y) = self.insets(text, available);
        let max_width = (available.width - 2. * x).max(0.);
        let field = self.children[0].component::<TextFieldContent>().unwrap();
        let width = text
            .measure(&field.surface.input.text, field.surface.role, max_width)
            .width
            .ceil()
            .clamp(0., max_width);
        let body = self.children[0]
            .measure(text, Size::new(width, available.height))
            .size;
        MeasuredChild::new(Size::new(
            (body.width + 2. * x).min(available.width),
            body.height + 2. * y,
        ))
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        let layout = self.surface.render_at_scale(
            &mut Scene::new(),
            frame,
            Some(clip),
            tekes_ui_tokens::Palette::from(tekes_ui_tokens::Theme::LIGHT),
            true,
            text.backing_scale(),
        );
        self.children[0].arrange(text, layout.content, clip);
    }
    fn paint(
        &mut self,
        scene: &mut Scene,
        text: &mut dyn ButtonTextRenderer,
        frame: Rect,
        clip: Rect,
        theme: ContentTheme,
    ) {
        self.surface.render_at_scale(
            scene,
            frame,
            Some(clip),
            theme.palette,
            true,
            text.backing_scale(),
        );
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, "用户输入")
    }
}

/// Independently mountable plain user-message body and external footnote.
/// This is a Transcript row, not a complete Transcript or RunView. Unsupported
/// attachments/structured blocks return an error before modifying the row.
#[derive(Debug)]
pub struct InputMessage {
    ids: InputMessageIds,
    scope: RunScope,
    key: Option<Key>,
    publication: u64,
    epoch: u64,
    delivery: Option<TranscriptDelivery>,
    children: Vec<Content>,
}
impl InputMessage {
    pub fn new(namespace: u32, scope: RunScope) -> Self {
        let ids = InputMessageIds::new(namespace);
        Self {
            ids,
            scope,
            key: None,
            publication: 0,
            epoch: 0,
            delivery: None,
            children: vec![
                Content::new(
                    ids.bubble,
                    Bubble {
                        surface: SurfaceView::default(),
                        children: vec![Content::new(
                            ids.body,
                            TextFieldContent::read_only_content(
                                "用户消息正文",
                                "",
                                proposal::TextRole::UiBody,
                            ),
                        )],
                    },
                ),
                Content::new(
                    ids.footnote,
                    TextContent {
                        text: String::new(),
                        role: proposal::TextRole::UiCaption,
                    },
                ),
            ],
        }
    }
    pub fn ids(&self) -> InputMessageIds {
        self.ids
    }
    pub fn scope(&self) -> &RunScope {
        &self.scope
    }
    pub fn update(
        &mut self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        input: &TranscriptInput<'_>,
    ) -> Result<(), &'static str> {
        self.update_with(
            scope,
            publication,
            epoch,
            input,
            |delivery| match delivery {
                TranscriptDelivery::Pending => "发送中".into(),
                TranscriptDelivery::Recorded { .. } => "已发送".into(),
            },
        )
    }
    /// Hosts format the existing delivery/time fact without inventing a clock.
    pub fn update_with(
        &mut self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        input: &TranscriptInput<'_>,
        format: impl FnOnce(TranscriptDelivery) -> String,
    ) -> Result<(), &'static str> {
        if scope != &self.scope {
            return Err("input message scope mismatch");
        }
        if publication == 0 || publication < self.publication || epoch < self.epoch {
            return Err("stale input message publication");
        }
        if input.item.kind != TranscriptItemKind::User || !input.item.is_visible {
            return Err("input message is not a visible user row");
        }
        if !input.item.attachments.is_empty()
            || !input.item.reasoning.is_empty()
            || input.item.content_blocks.as_ref().is_some_and(|blocks| {
                blocks.iter().any(|block| {
                    !matches!(
                        block.content,
                        tekes_agent_presentation::TranscriptBlockContent::Text { .. }
                    )
                })
            })
        {
            return Err("input message requires structured content composition");
        }
        let next = Key::from(input.key);
        let migrated = self.key.as_ref().is_some_and(|old| {
            matches!(old, Key::Submission(_))
                && matches!(next, Key::NativeEntry(_))
                && input.reconciliation_key().map(Key::from).as_ref() == Some(old)
        });
        if self.key.as_ref().is_some_and(|old| old != &next) && !migrated {
            return Err("input message identity changed");
        }
        let previous_text = &self.children[0].component::<Bubble>().unwrap().children[0]
            .component::<TextFieldContent>()
            .unwrap()
            .surface
            .input
            .text;
        if publication == self.publication
            && (epoch != self.epoch
                || previous_text != &input.item.text
                || self.delivery != Some(input.footnote)
                || self.key.as_ref() != Some(&next))
        {
            return Err("conflicting input message publication");
        }
        let footnote = format(input.footnote);
        let field = self.children[0].component_mut::<Bubble>().unwrap().children[0]
            .component_mut::<TextFieldContent>()
            .unwrap();
        let baseline = field.surface.input.external_baseline();
        let value = DraftRevision {
            identity: format!("input-message:{}", self.ids.root.0),
            version: publication,
            payload: input.item.text.clone(),
        };
        if !matches!(
            field.surface.apply_external_value(
                &baseline,
                &value,
                ExternalSelection::Preserve,
                ExternalUndo::Preserve
            ),
            ExternalValueResult::Applied { .. } | ExternalValueResult::Unchanged
        ) {
            return Err("input message text transaction rejected");
        }
        self.children[1]
            .component_mut::<TextContent>()
            .unwrap()
            .text = footnote;
        self.key = Some(next);
        self.publication = publication;
        self.epoch = epoch;
        self.delivery = Some(input.footnote);
        Ok(())
    }
    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let root = self.ids.root;
        ContentHost::new(Content::new(root, self))
    }
}
impl Component for InputMessage {
    fn children(&self) -> &[Content] {
        &self.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        let a = self.children[0].measure(text, available).size;
        let b = self.children[1].measure(text, available).size;
        MeasuredChild::new(Size::new(
            available.width,
            a.height + Spacing::Tight.value() + b.height,
        ))
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        let sizes: Vec<_> = self
            .children
            .iter_mut()
            .map(|child| child.measure(text, frame.size))
            .collect();
        let placed = layout_measured_children(
            frame,
            Axis::Vertical,
            &sizes,
            Spacing::Tight.value(),
            proposal::layout::COLUMN,
            AlignmentOverride {
                cross_axis: Some(proposal::layout::CrossAxisAlignment::Trailing),
                ..Default::default()
            },
        );
        for (child, position) in self.children.iter_mut().zip(placed) {
            child.arrange(text, position.frame, clip);
        }
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, "用户消息")
    }
}
