//! Agent machine-message composition. TekesUI owns text, controls and geometry.
use crate::TranscriptMessageIdentity as Key;
use tekes_agent_presentation::{
    RunScope, TranscriptBlockContent, TranscriptItem, TranscriptItemKind, TranscriptOutput,
    TranscriptSection, TranscriptToolCall, TranscriptToolRowState, TranscriptToolRowWording,
};
use tekes_ui_controls::{
    ButtonTextRenderer, DraftRevision, ExternalSelection, ExternalUndo, ExternalValueResult,
    composition::*,
    extended::{Axis, Stack},
    layout::{AlignmentOverride, MeasuredChild, layout_measured_children},
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::{Spacing, proposal};

/// Host copy hooks. Defaults are only fallback copy, never a private visual theme.
#[derive(Clone, Copy, Debug)]
pub enum OutputMessageText {
    Group,
    Body,
    Reasoning,
    ReasoningEnded,
    Attachment,
    Unknown,
    AggregateOrder,
    System,
    Error,
    Notice,
    Empty,
    MissingTool,
    Expand,
    Collapse,
}
#[derive(Clone, Copy, Debug)]
pub struct OutputMessageLabels {
    pub text: fn(OutputMessageText) -> String,
    pub tool: fn(&TranscriptToolCall) -> (String, String),
}
impl Default for OutputMessageLabels {
    fn default() -> Self {
        Self {
            text: |kind| {
                match kind {
                    OutputMessageText::Group => "机器消息",
                    OutputMessageText::Body => "机器正文",
                    OutputMessageText::Reasoning => "推理",
                    OutputMessageText::ReasoningEnded => "思考过程",
                    OutputMessageText::Attachment => "附件（暂不支持预览）",
                    OutputMessageText::Unknown => "未支持的内容类型（原始数据）",
                    OutputMessageText::AggregateOrder => "来源仅提供聚合内容，未提供块间顺序",
                    OutputMessageText::System => "系统消息",
                    OutputMessageText::Error => "错误",
                    OutputMessageText::Notice => "会话提示",
                    OutputMessageText::Empty => "来源未提供可显示正文",
                    OutputMessageText::MissingTool => "工具详情尚未回传",
                    OutputMessageText::Expand => "展开",
                    OutputMessageText::Collapse => "收起",
                }
                .into()
            },
            tool: |t| {
                let producer_title = t.presentation_title();
                let label = tool_display_name(&t.name);
                let result = t.presentation_result_text();
                let row = t.presentation_row(TranscriptToolRowWording {
                    label: &label,
                    stopped: "已停止",
                    title_hint: Some(&producer_title),
                    result: result.as_deref(),
                    ..Default::default()
                });
                let title = row.text();
                let mut details = format!(
                    "调用输入\n{}\n\n结果\n{}\n\n调用视图\n{}\n\n结果视图\n{}",
                    t.arguments,
                    t.result
                        .as_ref()
                        .map_or_else(|| "尚未回传".into(), ToString::to_string),
                    t.call_view
                        .as_ref()
                        .map_or_else(|| "未提供".into(), ToString::to_string),
                    t.result_view
                        .as_ref()
                        .map_or_else(|| "未提供".into(), ToString::to_string)
                );
                if let Some(error) = t.result_error.as_ref().filter(|value| value.is_object()) {
                    details.push_str(&format!("\n\n结果错误\n{error}"));
                }
                let phase = match row.state {
                    TranscriptToolRowState::Running => "doing",
                    TranscriptToolRowState::Ok => "done",
                    TranscriptToolRowState::Error => "error",
                    TranscriptToolRowState::Stopped => "stopped",
                };
                (format!("{title} · {phase}"), details)
            },
        }
    }
}

fn tool_display_name(name: &str) -> String {
    let action = name
        .starts_with("mcp__")
        .then(|| name.rsplit("__").next())
        .flatten();
    let name = match action {
        Some("dxf_context") => "read drawing context",
        Some("dxf_move") => "move drawing entities",
        Some("dxf_delete") => "delete drawing entities",
        Some("dxf_create_line") => "draw line",
        Some("dxf_change_text") => "edit drawing text",
        Some(action) => action,
        None => name,
    }
    .replace('_', " ");
    let mut chars = name.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + chars.as_str()
    })
}

/// Component slot within one message; never a durable reading anchor. Block
/// positions have only the source's current message-local ordering guarantee.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    Block(usize, OutputContentKind),
    Tool(usize, String),
    AggregateReasoning,
    AggregateBody,
    AggregateAttachment(usize),
    MessageKind,
    Failure,
    Empty,
    Coverage,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputContentKind {
    Text,
    Reasoning,
    Tool,
    Attachment,
    Unknown,
    Coverage,
    Status,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PartFacts {
    pub(crate) slot: Slot,
    pub(crate) kind: OutputContentKind,
    pub(crate) title: String,
    pub(crate) text: String,
}
impl PartFacts {
    fn disclosure(&self) -> bool {
        !matches!(
            self.kind,
            OutputContentKind::Text | OutputContentKind::Coverage | OutputContentKind::Status
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputContentIds {
    pub root: ComponentId,
    pub body: ComponentId,
    pub disclosure: Option<ComponentId>,
    pub kind: OutputContentKind,
}
#[derive(Debug)]
pub(crate) struct OutputPart {
    facts: PartFacts,
    ids: OutputContentIds,
    rows: StackContent,
    hidden: Option<Content>,
    labels: OutputMessageLabels,
    trailing: bool,
    live_reasoning: bool,
    bounded_details: bool,
}
impl OutputPart {
    pub(crate) fn slot(&self) -> &Slot {
        &self.facts.slot
    }
    pub(crate) fn ids(&self) -> OutputContentIds {
        self.ids
    }

    pub(crate) fn new(
        root: ComponentId,
        facts: PartFacts,
        labels: OutputMessageLabels,
        bounded_details: bool,
    ) -> Self {
        let ids = OutputContentIds {
            root,
            body: ComponentId(root.0 + 2),
            disclosure: facts.disclosure().then_some(ComponentId(root.0 + 1)),
            kind: facts.kind,
        };
        let mut field = TextFieldContent::read_only_content(
            (labels.text)(OutputMessageText::Body),
            "",
            if matches!(
                facts.kind,
                OutputContentKind::Coverage | OutputContentKind::Status
            ) {
                proposal::TextRole::UiCaption
            } else {
                proposal::TextRole::UiBody
            },
        );
        if bounded_details && ids.disclosure.is_some() {
            field.surface.scroll.set_scroll_policy(
                tekes_ui_controls::ScrollPolicy::Disabled,
                tekes_ui_controls::ScrollPolicy::Auto,
            );
            field.reading = Some(tekes_ui_controls::ReadingController::default());
        }
        let body = Content::new(ids.body, field);
        let (children, hidden) = if let Some(button) = ids.disclosure {
            (
                vec![
                    Content::new(button, ButtonContent::new(&facts.title))
                        .with_label_layout(tekes_ui_text::LabelOptions::single_line(
                            tekes_ui_text::LabelOverflow::TailEllipsis,
                        ))
                        .expect("output disclosure uses the original SDK button"),
                ],
                Some(body),
            )
        } else {
            (vec![body], None)
        };
        Self {
            facts,
            ids,
            rows: StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children,
            },
            hidden,
            labels,
            trailing: false,
            live_reasoning: false,
            bounded_details,
        }
    }
    pub(crate) fn set_trailing(&mut self, trailing: bool) {
        self.trailing = trailing;
    }
    fn field_mut(&mut self) -> &mut TextFieldContent {
        self.hidden
            .as_mut()
            .unwrap_or_else(|| self.rows.children.last_mut().unwrap())
            .component_mut::<TextFieldContent>()
            .unwrap()
    }
    fn configure_detail_viewport(&mut self, text: &mut dyn ButtonTextRenderer) {
        if !self.bounded_details || self.ids.disclosure.is_none() {
            return;
        }
        let scale = text
            .text_surface_host()
            .map_or(1., |host| host.type_scale());
        // ClientKit's embedded-detail design bounds. The original SDK owns
        // shaping, scrolling, reflow anchors, selection and native text services.
        let minimum = if self.facts.kind == OutputContentKind::Reasoning && self.live_reasoning {
            72. * scale
        } else {
            0.
        };
        self.field_mut().viewport = TextViewport::Grow {
            min: minimum,
            max: 240. * scale,
        };
    }
    pub(crate) fn update(&mut self, publication: u64, facts: PartFacts) {
        let id = self.ids.body;
        let field = self.field_mut();
        let baseline = field.surface.input.external_baseline();
        let outcome = field.surface.apply_external_value(
            &baseline,
            &DraftRevision {
                identity: format!("output-content:{}", id.0),
                version: publication,
                payload: facts.text.clone(),
            },
            ExternalSelection::Preserve,
            ExternalUndo::Preserve,
        );
        // These fields are exclusively read-only and owned by this message. A
        // failed external transaction is an invariant violation, not a partial UI update.
        assert!(matches!(
            outcome,
            ExternalValueResult::Applied { .. } | ExternalValueResult::Unchanged
        ));
        self.facts = facts;
        self.refresh_header();
    }
    fn refresh_header(&mut self) {
        if self.ids.disclosure.is_some() {
            let button = self.rows.children[0]
                .component_mut::<ButtonContent>()
                .unwrap();
            button.label = format!(
                "{} · {}",
                (self.labels.text)(if self.hidden.is_none() {
                    OutputMessageText::Collapse
                } else {
                    OutputMessageText::Expand
                }),
                self.facts.title
            );
            button.button.state.selected = self.hidden.is_none();
        }
    }
    pub(crate) fn toggle(&mut self) {
        if let Some(body) = self.hidden.take() {
            self.rows.children.push(body);
        } else {
            self.hidden = self.rows.children.pop();
        }
        self.refresh_header();
    }
}
fn arrange_content(
    children: &mut [Content],
    text: &mut dyn ButtonTextRenderer,
    frame: Rect,
    clip: Rect,
    gap: f32,
    trailing: bool,
) {
    let measured: Vec<_> = children
        .iter_mut()
        .map(|c| c.measure(text, frame.size))
        .collect();
    let placed = layout_measured_children(
        frame,
        Axis::Vertical,
        &measured,
        gap,
        proposal::layout::COLUMN,
        AlignmentOverride {
            cross_axis: Some(if trailing {
                proposal::layout::CrossAxisAlignment::Trailing
            } else {
                proposal::layout::CrossAxisAlignment::Leading
            }),
            ..Default::default()
        },
    );
    for (child, placed) in children.iter_mut().zip(placed) {
        child.arrange(text, placed.frame, clip);
    }
}
impl Component for OutputPart {
    fn children(&self) -> &[Content] {
        &self.rows.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.rows.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        self.configure_detail_viewport(text);
        if self.trailing && self.hidden.is_none() && self.rows.children.len() > 1 {
            let field = self
                .rows
                .children
                .last()
                .unwrap()
                .component::<TextFieldContent>()
                .unwrap();
            let body_width = text
                .measure(
                    &field.surface.input.text,
                    field.surface.role,
                    available.width,
                )
                .width
                .ceil()
                .clamp(0., available.width);
            let header_width = self.rows.children[0].measure(text, available).size.width;
            self.rows.measure(
                text,
                Size::new(body_width.max(header_width), available.height),
            )
        } else {
            self.rows.measure(text, available)
        }
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        self.configure_detail_viewport(text);
        arrange_content(
            &mut self.rows.children,
            text,
            frame,
            clip,
            Spacing::Tight.value(),
            self.trailing,
        );
    }
    fn semantics(&self) -> ContentSemantics {
        let mut semantics = ContentSemantics::new(
            ContentRole::Group,
            if self.facts.title.is_empty() {
                (self.labels.text)(OutputMessageText::Body)
            } else {
                self.facts.title.clone()
            },
        );
        semantics.expanded = self.ids.disclosure.map(|_| self.hidden.is_none());
        semantics
    }
}

/// One machine message. Stable source keys, canonical block order and existing
/// tool DTOs feed this component; it does not infer settlement or read offsets.
/// Markdown is currently displayed as source prose. Opaque blocks remain explicit
/// raw-data disclosures until the corresponding rich-content adapters are provided.
#[derive(Debug)]
pub struct OutputMessage {
    root: ComponentId,
    scope: RunScope,
    key: Option<Key>,
    publication: u64,
    epoch: u64,
    item: Option<TranscriptItem>,
    facts: Vec<PartFacts>,
    rows: StackContent,
    next: u64,
    labels: OutputMessageLabels,
    bounded_details: bool,
}
impl OutputMessage {
    pub fn new(namespace: u32, scope: RunScope) -> Self {
        Self::with_labels(namespace, scope, OutputMessageLabels::default())
    }
    pub fn with_labels(namespace: u32, scope: RunScope, labels: OutputMessageLabels) -> Self {
        Self {
            root: ComponentId((namespace as u64) << 32),
            scope,
            key: None,
            publication: 0,
            epoch: 0,
            item: None,
            facts: vec![],
            rows: StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Regular.value(),
                },
                flexible_child: None,
                children: vec![],
            },
            next: 16,
            labels,
            bounded_details: false,
        }
    }
    /// Experimental bounded reasoning/tool disclosures using original SDK
    /// viewports. Configure before publishing. Standard RunPort keeps the inline
    /// path until TekesUI #53's strict nested-reading gate is satisfied.
    pub fn with_bounded_details(
        namespace: u32,
        scope: RunScope,
        labels: OutputMessageLabels,
    ) -> Self {
        let mut view = Self::with_labels(namespace, scope, labels);
        view.bounded_details = true;
        view
    }
    pub fn id(&self) -> ComponentId {
        self.root
    }
    pub fn scope(&self) -> &RunScope {
        &self.scope
    }
    pub fn content_ids(&self) -> Vec<OutputContentIds> {
        self.rows
            .children
            .iter()
            .map(|c| c.component::<OutputPart>().unwrap().ids)
            .collect()
    }
    /// Missing tool DTOs stay unresolved, never resolved through another turn or
    /// the currently selected session. None is valid for a session-wide notice.
    pub fn update(
        &mut self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        output: &TranscriptOutput<'_>,
        section: Option<&TranscriptSection<'_>>,
    ) -> Result<(), &'static str> {
        let (key, facts) = self.check_update(scope, publication, epoch, output, section)?;
        let mut old = std::mem::take(&mut self.rows.children);
        for f in &facts {
            let mut content = if let Some(i) = old.iter().position(|c| {
                self.reuses_part(
                    output,
                    &facts,
                    &c.component::<OutputPart>().unwrap().facts,
                    f,
                )
            }) {
                old.remove(i)
            } else {
                let id = ComponentId(self.root.0 + self.next);
                self.next += 3;
                Content::new(
                    id,
                    OutputPart::new(id, f.clone(), self.labels, self.bounded_details),
                )
            };
            let part = content.component_mut::<OutputPart>().unwrap();
            part.live_reasoning = output.item.is_provisional && !output.item.reasoning_ended;
            part.update(publication, f.clone());
            self.rows.children.push(content);
        }
        self.key = Some(key);
        self.publication = publication;
        self.epoch = epoch;
        self.item = Some(output.item.clone());
        self.facts = facts;
        Ok(())
    }
    pub(crate) fn check_update(
        &self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        output: &TranscriptOutput<'_>,
        section: Option<&TranscriptSection<'_>>,
    ) -> Result<(Key, Vec<PartFacts>), &'static str> {
        if scope != &self.scope {
            return Err("output message scope mismatch");
        }
        if publication == 0 || publication < self.publication || epoch < self.epoch {
            return Err("stale output message publication");
        }
        if !output.item.is_visible || output.item.kind == TranscriptItemKind::User {
            return Err("output is not a visible machine message");
        }
        if section.is_some_and(|s| s.id != output.item.turn_id) {
            return Err("output tool section mismatch");
        }
        let key = Key::from(output.key);
        if self.key.as_ref().is_some_and(|old| old != &key) {
            let receipt = output.reconciliation_key().map(Key::from);
            if !self
                .key
                .as_ref()
                .is_some_and(|old| matches!(old, Key::Stream(_)) && Some(old) == receipt.as_ref())
                || !self.item.as_ref().is_some_and(|old| {
                    old.kind == TranscriptItemKind::Assistant && old.is_provisional
                })
            {
                return Err("output message identity changed");
            }
        }
        let facts = parts(output, section, self.labels)?;
        if publication == self.publication
            && (epoch != self.epoch
                || self.item.as_ref() != Some(output.item)
                || self.facts != facts)
        {
            return Err("conflicting output message publication");
        }
        let new_count = facts
            .iter()
            .filter(|f| {
                !self
                    .facts
                    .iter()
                    .any(|old| self.reuses_part(output, &facts, old, f))
            })
            .count();
        if self
            .next
            .checked_add(new_count as u64 * 3)
            .is_none_or(|v| v >= u32::MAX as u64)
        {
            return Err("output message namespace exhausted");
        }
        Ok((key, facts))
    }
    /// A stable message may change between aggregate and ordered encodings.
    /// Only its sole body/reasoning role has an unambiguous semantic target;
    /// multiple blocks never acquire an association through text or position.
    fn reuses_part(
        &self,
        output: &TranscriptOutput<'_>,
        facts: &[PartFacts],
        old: &PartFacts,
        new: &PartFacts,
    ) -> bool {
        if old.slot == new.slot {
            return true;
        }
        self.item.as_ref().is_some_and(|previous| {
            previous.kind == TranscriptItemKind::Assistant
                && output.item.kind == TranscriptItemKind::Assistant
                && previous.content_blocks.is_some() != output.item.content_blocks.is_some()
        }) && old.kind == new.kind
            && self
                .facts
                .iter()
                .filter(|part| part.kind == new.kind)
                .count()
                == 1
            && facts.iter().filter(|part| part.kind == new.kind).count() == 1
            && matches!(
                (&old.slot, &new.slot),
                (Slot::AggregateBody, Slot::Block(_, OutputContentKind::Text))
                    | (Slot::Block(_, OutputContentKind::Text), Slot::AggregateBody)
                    | (
                        Slot::AggregateReasoning,
                        Slot::Block(_, OutputContentKind::Reasoning)
                    )
                    | (
                        Slot::Block(_, OutputContentKind::Reasoning),
                        Slot::AggregateReasoning
                    )
            )
    }
    /// UI-local disclosure only. The host drains the original ContentHost action
    /// batch exactly once; retired button IDs cannot target a replacement block.
    pub fn handle_actions(&mut self, actions: &[ContentAction]) -> bool {
        let mut changed = false;
        for a in actions
            .iter()
            .filter(|a| a.kind == ContentActionKind::Activate)
        {
            if let Some(part) = self.rows.children.iter_mut().find_map(|c| {
                let part = c.component_mut::<OutputPart>().unwrap();
                (part.ids.disclosure == Some(a.target)).then_some(part)
            }) {
                part.toggle();
                changed = true;
            }
        }
        changed
    }
    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let id = self.root;
        {
            let mut host = ContentHost::new(Content::new(id, self))?;
            host.set_action_events(true);
            Ok(host)
        }
    }
}
impl Component for OutputMessage {
    fn active(&self) -> bool {
        !self.rows.children.is_empty()
    }
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
        arrange_content(
            &mut self.rows.children,
            text,
            frame,
            clip,
            Spacing::Regular.value(),
            false,
        );
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(
            ContentRole::Group,
            (self.labels.text)(OutputMessageText::Group),
        )
    }
}
fn parts(
    output: &TranscriptOutput<'_>,
    section: Option<&TranscriptSection<'_>>,
    labels: OutputMessageLabels,
) -> Result<Vec<PartFacts>, &'static str> {
    let item = output.item;
    let mut result = vec![];
    let status = match item.kind {
        TranscriptItemKind::System => Some(OutputMessageText::System),
        TranscriptItemKind::Error => Some(OutputMessageText::Error),
        TranscriptItemKind::Notice => Some(OutputMessageText::Notice),
        _ => None,
    };
    if let Some(status) = status {
        result.push(PartFacts {
            slot: Slot::MessageKind,
            kind: OutputContentKind::Status,
            title: String::new(),
            text: (labels.text)(status),
        });
    }
    if item.kind == TranscriptItemKind::Assistant
        && let Some(error) = item.failure_message()
    {
        result.push(PartFacts {
            slot: Slot::Failure,
            kind: OutputContentKind::Status,
            title: String::new(),
            text: format!("{}：{error}", (labels.text)(OutputMessageText::Error)),
        });
    }
    if let Some(blocks) = &item.content_blocks {
        let mut previous = None;
        for block in blocks {
            if previous.is_some_and(|i| i >= block.index) {
                return Err("invalid machine content block order");
            }
            previous = Some(block.index);
            let (kind, title, text) = match &block.content {
                TranscriptBlockContent::Text { text } => {
                    (OutputContentKind::Text, String::new(), text.clone())
                }
                TranscriptBlockContent::Reasoning { text } => (
                    OutputContentKind::Reasoning,
                    (labels.text)(if item.reasoning_ended {
                        OutputMessageText::ReasoningEnded
                    } else {
                        OutputMessageText::Reasoning
                    }),
                    text.clone(),
                ),
                TranscriptBlockContent::ToolCall { tool_id } => {
                    let tool = section
                        .and_then(|s| s.tool_for_block(block))
                        .filter(|t| t.turn_id == item.turn_id);
                    let (title, text) = tool.map_or_else(
                        || {
                            (
                                format!(
                                    "{} · {tool_id}",
                                    (labels.text)(OutputMessageText::MissingTool)
                                ),
                                tool_id.clone(),
                            )
                        },
                        labels.tool,
                    );
                    (OutputContentKind::Tool, title, text)
                }
                TranscriptBlockContent::Attachment { content } => (
                    OutputContentKind::Attachment,
                    (labels.text)(OutputMessageText::Attachment),
                    content.to_string(),
                ),
                TranscriptBlockContent::Unknown { content } => (
                    OutputContentKind::Unknown,
                    (labels.text)(OutputMessageText::Unknown),
                    content.to_string(),
                ),
            };
            result.push(PartFacts {
                slot: if let TranscriptBlockContent::ToolCall { tool_id } = &block.content {
                    Slot::Tool(block.index, tool_id.clone())
                } else {
                    Slot::Block(block.index, kind)
                },
                kind,
                title,
                text,
            });
        }
    } else {
        if !item.reasoning.is_empty() && !item.text.is_empty() {
            result.push(PartFacts {
                slot: Slot::Coverage,
                kind: OutputContentKind::Coverage,
                title: String::new(),
                text: (labels.text)(OutputMessageText::AggregateOrder),
            });
        }
        if !item.reasoning.is_empty() {
            result.push(PartFacts {
                slot: Slot::AggregateReasoning,
                kind: OutputContentKind::Reasoning,
                title: (labels.text)(if item.reasoning_ended {
                    OutputMessageText::ReasoningEnded
                } else {
                    OutputMessageText::Reasoning
                }),
                text: item.reasoning.clone(),
            });
        }
        if !item.text.is_empty() {
            result.push(PartFacts {
                slot: Slot::AggregateBody,
                kind: OutputContentKind::Text,
                title: String::new(),
                text: item.text.clone(),
            });
        }
        for (i, attachment) in item.attachments.iter().enumerate() {
            result.push(PartFacts {
                slot: Slot::AggregateAttachment(i),
                kind: OutputContentKind::Attachment,
                title: (labels.text)(OutputMessageText::Attachment),
                text: attachment.to_string(),
            });
        }
    }
    // An explicitly empty canonical assistant container is not missing data.
    // Keep legacy/aggregate-only and status lanes visible; never infer emptiness
    // from a sibling tool, matching prose, clock or settlement state.
    let known_empty = item.kind == TranscriptItemKind::Assistant
        && item.text.is_empty()
        && item.reasoning.is_empty()
        && item.attachments.is_empty()
        && item.content_blocks.as_ref().is_some_and(Vec::is_empty);
    if result.is_empty() && !known_empty {
        result.push(PartFacts {
            slot: Slot::Empty,
            kind: OutputContentKind::Status,
            title: String::new(),
            text: (labels.text)(OutputMessageText::Empty),
        });
    }
    Ok(result)
}
