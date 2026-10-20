//! User prose lane; generic text/selection/IME/painting remain in TekesUI.
use tekes_agent_presentation::{
    RunScope, TranscriptBlockContent, TranscriptDelivery, TranscriptInput, TranscriptItem,
    TranscriptItemKind,
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

use crate::TranscriptMessageIdentity as Key;
use crate::output_message::{OutputPart, PartFacts, Slot};
use crate::{OutputContentKind, OutputMessageLabels, OutputMessageText};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Clipboard capability captured from one displayed message. No endpoint call;
/// hosts use the captured text, never look up the newly selected session.
#[derive(Clone, Debug)]
pub struct TranscriptCopyIntent {
    scope: RunScope,
    message: Key,
    text: String,
    claimed: Arc<AtomicBool>,
}
impl TranscriptCopyIntent {
    pub fn scope(&self) -> &RunScope {
        &self.scope
    }
    pub fn message(&self) -> &Key {
        &self.message
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Clones share one clipboard write admission. No retry or network authority.
    pub fn claim(&self) -> bool {
        !self.claimed.swap(true, Ordering::AcqRel)
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
        if self.children[0]
            .component::<TextFieldContent>()
            .unwrap()
            .surface
            .input
            .text
            .is_empty()
        {
            &[]
        } else {
            &self.children
        }
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        if self.children[0]
            .component::<TextFieldContent>()
            .unwrap()
            .surface
            .input
            .text
            .is_empty()
        {
            None
        } else {
            Some(&mut self.children)
        }
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        if self.children[0]
            .component::<TextFieldContent>()
            .unwrap()
            .surface
            .input
            .text
            .is_empty()
        {
            return MeasuredChild::new(Size::new(0., 0.));
        }
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
        if self.children[0]
            .component::<TextFieldContent>()
            .unwrap()
            .surface
            .input
            .text
            .is_empty()
        {
            return;
        }
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
        if self.children[0]
            .component::<TextFieldContent>()
            .unwrap()
            .surface
            .input
            .text
            .is_empty()
        {
            return;
        }
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

/// One whole user message: attachments/raw details, prose bubble and
/// external footnote, all trailing. Rich text/image preview are separate capabilities.
#[derive(Debug)]
pub struct InputMessage {
    ids: InputMessageIds,
    scope: RunScope,
    key: Option<Key>,
    publication: u64,
    epoch: u64,
    delivery: Option<TranscriptDelivery>,
    children: Vec<Content>,
    last_item: Option<TranscriptItem>,
    details: Vec<PartFacts>,
    next_detail: u64,
    detail_labels: OutputMessageLabels,
    copy_id: Option<ComponentId>,
    copy_label: String,
}
impl InputMessage {
    pub fn new(namespace: u32, scope: RunScope) -> Self {
        Self::with_details_labels(namespace, scope, Self::default_details_labels())
    }
    pub fn default_details_labels() -> OutputMessageLabels {
        OutputMessageLabels {
            text: |kind| {
                if matches!(kind, OutputMessageText::Body) {
                    "用户附件或原始内容".into()
                } else {
                    (OutputMessageLabels::default().text)(kind)
                }
            },
            ..Default::default()
        }
    }
    pub fn with_details_labels(
        namespace: u32,
        scope: RunScope,
        detail_labels: OutputMessageLabels,
    ) -> Self {
        let ids = InputMessageIds::new(namespace);
        Self {
            ids,
            scope,
            key: None,
            publication: 0,
            epoch: 0,
            delivery: None,
            last_item: None,
            details: vec![],
            next_detail: 16,
            detail_labels,
            copy_id: None,
            copy_label: "复制输入".into(),
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
        let next = self.check_update(scope, publication, epoch, input)?;
        let footnote = format(input.footnote);
        let field = self
            .children
            .iter_mut()
            .find_map(|node| node.component_mut::<Bubble>())
            .unwrap()
            .children[0]
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
        let facts = input_details(input.item)?;
        let mut previous = std::mem::take(&mut self.children);
        let bubble = previous
            .iter()
            .position(|node| node.id() == self.ids.bubble)
            .unwrap();
        let bubble = previous.remove(bubble);
        let footer = previous
            .iter()
            .position(|node| node.id() == self.ids.footnote)
            .unwrap();
        let mut footer = previous.remove(footer);
        footer.component_mut::<TextContent>().unwrap().text = footnote;
        for facts in &facts {
            let mut node = previous
                .iter()
                .position(|node| {
                    node.component::<OutputPart>()
                        .is_some_and(|part| part.slot() == &facts.slot)
                })
                .map(|index| previous.remove(index))
                .unwrap_or_else(|| {
                    let id = ComponentId(self.ids.root.0 + self.next_detail);
                    self.next_detail += 3;
                    Content::new(id, OutputPart::new(id, facts.clone(), self.detail_labels))
                });
            let part = node.component_mut::<OutputPart>().unwrap();
            part.set_trailing(true);
            part.update(publication, facts.clone());
            self.children.push(node);
        }
        self.children.push(bubble);
        self.children.push(footer);
        let same_copy = self
            .last_item
            .as_ref()
            .is_some_and(|old| old.text == input.item.text)
            && self.key.as_ref() == Some(&next)
            && self.epoch == epoch;
        let old_copy = self
            .copy_id
            .and_then(|id| previous.into_iter().find(|node| node.id() == id));
        self.copy_id = None;
        if !input.item.text.is_empty() {
            let node = if same_copy { old_copy } else { None }.unwrap_or_else(|| {
                let id = ComponentId(self.ids.root.0 + self.next_detail);
                self.next_detail += 1;
                Content::new(id, ButtonContent::new(&self.copy_label))
            });
            self.copy_id = Some(node.id());
            self.children.push(node);
        }
        self.details = facts;
        self.last_item = Some(input.item.clone());
        self.key = Some(next);
        self.publication = publication;
        self.epoch = epoch;
        self.delivery = Some(input.footnote);
        Ok(())
    }
    pub(crate) fn check_update(
        &self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        input: &TranscriptInput<'_>,
    ) -> Result<Key, &'static str> {
        if scope != &self.scope {
            return Err("input message scope mismatch");
        }
        if publication == 0 || publication < self.publication || epoch < self.epoch {
            return Err("stale input message publication");
        }
        if input.item.kind != TranscriptItemKind::User || !input.item.is_visible {
            return Err("input message is not a visible user row");
        }
        let facts = input_details(input.item)?;
        let additions = facts
            .iter()
            .filter(|part| !self.details.iter().any(|old| old.slot == part.slot))
            .count() as u64;
        if self
            .next_detail
            .checked_add(additions * 3 + 1)
            .is_none_or(|end| end >= u32::MAX as u64)
        {
            return Err("input details namespace exhausted");
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
        let previous_text = &self
            .children
            .iter()
            .find_map(|node| node.component::<Bubble>())
            .unwrap()
            .children[0]
            .component::<TextFieldContent>()
            .unwrap()
            .surface
            .input
            .text;
        if publication == self.publication
            && (epoch != self.epoch
                || previous_text != &input.item.text
                || self.delivery != Some(input.footnote)
                || self.key.as_ref() != Some(&next)
                || self.last_item.as_ref() != Some(input.item))
        {
            return Err("conflicting input message publication");
        }
        Ok(next)
    }
    pub fn detail_ids(&self) -> Vec<crate::OutputContentIds> {
        self.children
            .iter()
            .filter_map(|node| node.component::<OutputPart>().map(OutputPart::ids))
            .collect()
    }
    pub fn copy_button_id(&self) -> Option<ComponentId> {
        self.copy_id
    }
    pub fn set_copy_label(&mut self, label: impl Into<String>) {
        self.copy_label = label.into();
        if let Some(id) = self.copy_id
            && let Some(node) = self.children.iter_mut().find(|node| node.id() == id)
        {
            node.component_mut::<ButtonContent>().unwrap().label = self.copy_label.clone();
        }
    }
    /// Copy is available for loaded text even offline and in a read-only pane.
    /// Empty/attachment-only inputs do not pretend to offer file copying.
    pub fn take_copy_intents(&self, actions: &[ContentAction]) -> Vec<TranscriptCopyIntent> {
        if self.copy_id.is_none()
            || !actions.iter().any(|action| {
                Some(action.target) == self.copy_id && action.kind == ContentActionKind::Activate
            })
        {
            return vec![];
        }
        vec![TranscriptCopyIntent {
            scope: self.scope.clone(),
            message: self.key.clone().unwrap(),
            text: self.last_item.as_ref().unwrap().text.clone(),
            claimed: Arc::new(AtomicBool::new(false)),
        }]
    }
    pub fn handle_actions(&mut self, actions: &[ContentAction]) -> bool {
        let mut changed = false;
        for node in &mut self.children {
            if let Some(part) = node.component_mut::<OutputPart>() {
                let target = part.ids().disclosure;
                for _ in actions
                    .iter()
                    .filter(|a| a.kind == ContentActionKind::Activate && target == Some(a.target))
                {
                    part.toggle();
                    changed = true;
                }
            }
        }
        changed
    }
    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let root = self.ids.root;
        let mut host = ContentHost::new(Content::new(root, self))?;
        host.set_action_events(true);
        Ok(host)
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
        let height = self
            .children
            .iter_mut()
            .map(|child| child.measure(text, available).size.height)
            .sum::<f32>()
            + Spacing::Tight.value() * self.children.len().saturating_sub(1) as f32;
        MeasuredChild::new(Size::new(available.width, height))
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

fn input_details(item: &TranscriptItem) -> Result<Vec<PartFacts>, &'static str> {
    let mut facts = vec![];
    let mut canonical_attachments = vec![];
    let mut text_blocks = String::new();
    let mut has_reasoning = false;
    if let Some(blocks) = &item.content_blocks {
        let mut previous = None;
        for block in blocks {
            if previous.is_some_and(|index| index >= block.index) {
                return Err("invalid input content block order");
            }
            previous = Some(block.index);
            let (kind, title, text) = match &block.content {
                TranscriptBlockContent::Text { text } => {
                    text_blocks.push_str(text);
                    continue;
                }
                TranscriptBlockContent::Attachment { content } => {
                    canonical_attachments.push(content);
                    (
                        OutputContentKind::Attachment,
                        content
                            .get("name")
                            .or_else(|| content.get("fileName"))
                            .and_then(|value| value.as_str())
                            .map_or_else(
                                || "附件（原始信息）".into(),
                                |name| format!("附件 · {name}"),
                            ),
                        content.to_string(),
                    )
                }
                TranscriptBlockContent::Reasoning { text } => {
                    has_reasoning = true;
                    (
                        OutputContentKind::Unknown,
                        "用户消息中的 reasoning 字段".into(),
                        text.clone(),
                    )
                }
                TranscriptBlockContent::ToolCall { tool_id } => (
                    OutputContentKind::Unknown,
                    "用户消息中的工具引用".into(),
                    tool_id.clone(),
                ),
                TranscriptBlockContent::Unknown { content } => (
                    OutputContentKind::Unknown,
                    "未支持的用户内容（原始信息）".into(),
                    content.to_string(),
                ),
            };
            facts.push(PartFacts {
                slot: Slot::Block(block.index, kind),
                kind,
                title,
                text,
            });
        }
        if !text_blocks.is_empty() && text_blocks != item.text {
            facts.push(PartFacts {
                slot: Slot::AggregateBody,
                kind: OutputContentKind::Unknown,
                title: "原始文本块（与聚合正文不同）".into(),
                text: text_blocks,
            });
        }
    }
    // Both fields represent the same message content. Match occurrences, so two
    // identical attachments remain two attachments rather than a value-set dedup.
    let mut matched = vec![false; canonical_attachments.len()];
    for (index, content) in item.attachments.iter().enumerate() {
        if let Some(i) = canonical_attachments
            .iter()
            .enumerate()
            .position(|(i, value)| !matched[i] && **value == *content)
        {
            matched[i] = true;
            continue;
        }
        facts.push(PartFacts {
            slot: Slot::AggregateAttachment(index),
            kind: OutputContentKind::Attachment,
            title: content
                .get("name")
                .or_else(|| content.get("fileName"))
                .and_then(|value| value.as_str())
                .map_or_else(
                    || "附件（原始信息）".into(),
                    |name| format!("附件 · {name}"),
                ),
            text: content.to_string(),
        });
    }
    if !item.reasoning.is_empty() && !has_reasoning {
        facts.push(PartFacts {
            slot: Slot::AggregateReasoning,
            kind: OutputContentKind::Unknown,
            title: "用户消息中的 reasoning 字段".into(),
            text: item.reasoning.clone(),
        });
    }
    Ok(facts)
}
