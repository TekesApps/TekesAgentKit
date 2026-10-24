//! Agent machine-message composition. TekesUI owns text, controls and geometry.
mod fragment;
use crate::TranscriptMessageIdentity as Key;
use crate::tool_detail_content::{ToolDetailContent, ToolDetailFacts};
pub use fragment::*;
use tekes_agent_presentation::{
    RunScope, TranscriptBlockContent, TranscriptItem, TranscriptItemKind, TranscriptOutput,
    TranscriptSection, TranscriptToolCall, TranscriptToolDetailWording, TranscriptToolRowState,
    TranscriptToolRowWording,
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
    SystemPrompt,
    SystemPromptUpdate,
    Error,
    Notice,
    Empty,
    MissingTool,
    Expand,
    Collapse,
    ToolInput,
    ToolOutput,
    ToolFailed,
    ToolCallView,
    ToolResultView,
    ToolReceivingInput,
    ToolInputUnavailable,
    CopyDetail,
    Thinking,
}
pub type ScopedToolFormatter = fn(&RunScope, &TranscriptToolCall) -> (String, String);
pub type ToolHeaderInputsProvider =
    fn(&RunScope, &TranscriptToolCall) -> tekes_agent_presentation::TranscriptToolRowInputs;

/// Hosts supply explicit facts and copy. Shared presentation owns report/count
/// precedence and UTF-16 business spans; this is not a second row state machine.
#[derive(Clone, Copy, Debug)]
pub struct ToolHeaderAdapter {
    pub inputs: ToolHeaderInputsProvider,
    /// Monotonic display-source revision within a connection epoch. Providers
    /// must read one immutable UI-turn snapshot and perform no I/O or mutation.
    /// Advancing this revision refreshes headers without changing Run authority;
    /// it does not authorize changing legacy detail/section payloads.
    pub revision: fn(&RunScope) -> u64,
    pub name: Option<fn(&str) -> String>,
    pub stopped: fn() -> String,
    pub report_wording: tekes_agent_presentation::TranscriptToolReportWording,
}

#[derive(Clone, Copy, Debug)]
pub struct OutputMessageLabels {
    pub text: fn(OutputMessageText) -> String,
    /// Pure tool-local formatting; external lookups must use `scoped_tool`.
    pub tool: fn(&TranscriptToolCall) -> (String, String),
    pub scoped_tool: Option<ScopedToolFormatter>,
    /// When set, the shared structured header supersedes the legacy title;
    /// legacy detail formatting and original structured detail owners remain.
    pub tool_header: Option<ToolHeaderAdapter>,
    pub tool_run: crate::ToolRunLabels,
    /// Structured expansion. Set None to retain a custom `tool` text body.
    pub tool_sections: Option<
        fn(
            &RunScope,
            &TranscriptToolCall,
            OutputMessageLabels,
        ) -> tekes_agent_presentation::TranscriptToolDetails,
    >,
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
                    OutputMessageText::SystemPrompt => "系统提示词",
                    OutputMessageText::SystemPromptUpdate => "系统提示词更新",
                    OutputMessageText::Error => "错误",
                    OutputMessageText::Notice => "会话提示",
                    OutputMessageText::Empty => "来源未提供可显示正文",
                    OutputMessageText::MissingTool => "工具详情尚未回传",
                    OutputMessageText::Expand => "展开",
                    OutputMessageText::Collapse => "收起",
                    OutputMessageText::ToolInput => "调用输入",
                    OutputMessageText::ToolOutput => "结果",
                    OutputMessageText::ToolFailed => "失败",
                    OutputMessageText::ToolCallView => "调用视图",
                    OutputMessageText::ToolResultView => "结果视图",
                    OutputMessageText::ToolReceivingInput => "正在接收输入",
                    OutputMessageText::ToolInputUnavailable => "输入不可用",
                    OutputMessageText::CopyDetail => "复制",
                    OutputMessageText::Thinking => "思考中",
                }
                .into()
            },
            tool_sections: Some(default_tool_sections),
            scoped_tool: None,
            tool_header: None,
            tool_run: Default::default(),
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
                let details = t.presentation_details(TranscriptToolDetailWording {
                    input: "调用输入",
                    output: "结果",
                    failed: "失败",
                    call_view: "调用视图",
                    result_view: "结果视图",
                    receiving_input: "正在接收输入",
                    input_unavailable: "输入不可用",
                    fallback_output: &producer_title,
                    ..Default::default()
                });
                let details = details
                    .sections
                    .iter()
                    .map(|section| format!("{}\n{}", section.title, section.text))
                    .collect::<Vec<_>>()
                    .join("\n\n");
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

fn default_tool_sections(
    _scope: &RunScope,
    t: &TranscriptToolCall,
    labels: OutputMessageLabels,
) -> tekes_agent_presentation::TranscriptToolDetails {
    let title = t.presentation_title();
    t.presentation_details(TranscriptToolDetailWording {
        input: &(labels.text)(OutputMessageText::ToolInput),
        output: &(labels.text)(OutputMessageText::ToolOutput),
        failed: &(labels.text)(OutputMessageText::ToolFailed),
        call_view: &(labels.text)(OutputMessageText::ToolCallView),
        result_view: &(labels.text)(OutputMessageText::ToolResultView),
        receiving_input: &(labels.text)(OutputMessageText::ToolReceivingInput),
        input_unavailable: &(labels.text)(OutputMessageText::ToolInputUnavailable),
        fallback_output: &title,
        ..Default::default()
    })
}
pub(crate) fn tool_header_revision(labels: OutputMessageLabels, scope: &RunScope) -> u64 {
    labels
        .tool_header
        .map_or(0, |adapter| (adapter.revision)(scope))
}
pub(crate) fn tool_text(
    scope: &RunScope,
    tool: &TranscriptToolCall,
    labels: OutputMessageLabels,
) -> (
    String,
    String,
    Option<tekes_agent_presentation::TranscriptToolHeader>,
) {
    let (legacy_title, details) = labels
        .scoped_tool
        .map_or_else(|| (labels.tool)(tool), |make| make(scope, tool));
    let header = labels.tool_header.map(|adapter| {
        let inputs = (adapter.inputs)(scope, tool);
        let name = adapter.name.unwrap_or(tool_display_name)(&tool.name);
        let producer_title = tool.presentation_title();
        let result = tool.presentation_result_text();
        tool.presentation_header(
            TranscriptToolRowWording {
                label: &name,
                stopped: &(adapter.stopped)(),
                title_hint: Some(&producer_title),
                result: result.as_deref(),
                ..Default::default()
            },
            &inputs,
            adapter.report_wording,
        )
    });
    let title = header
        .as_ref()
        .map_or(legacy_title, |header| header.text.clone());
    (title, details, header)
}
pub(crate) fn tool_details_facts(
    scope: &RunScope,
    t: &TranscriptToolCall,
    labels: OutputMessageLabels,
) -> Option<ToolDetailFacts> {
    labels.tool_sections.map(|make| ToolDetailFacts {
        tool_id: t.id.clone(),
        turn_id: t.turn_id.clone(),
        details: make(scope, t, labels),
    })
}
pub(crate) fn validate_tool_details(facts: &PartFacts) -> Result<(), &'static str> {
    if let Some(tool) = &facts.tool {
        use tekes_agent_presentation::TranscriptToolDetailKind::*;
        let kinds = tool
            .details
            .sections
            .iter()
            .map(|s| s.kind)
            .collect::<Vec<_>>();
        if !matches!(
            kinds.as_slice(),
            [Input, Output]
                | [Input, Output, CallView]
                | [Input, Output, ResultView]
                | [Input, Output, CallView, ResultView]
        ) {
            return Err("invalid tool detail section order");
        }
    }
    Ok(())
}

pub(crate) fn tool_display_name(name: &str) -> String {
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
    ThinkingStatus,
    AggregateBody,
    SystemPrompt,
    AggregateAttachment(usize),
    /// Receipt attachment position, never a native content-block index.
    AdmittedFile(usize),
    MessageKind,
    Failure,
    Empty,
    Coverage,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputContentKind {
    SystemPrompt,
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
    pub(crate) tool: Option<ToolDetailFacts>,
    pub(crate) tool_header: Option<tekes_agent_presentation::TranscriptToolHeader>,
}
impl PartFacts {
    pub(crate) fn matches_header_update(&self, next: &Self) -> bool {
        self == next
            || (self.slot == next.slot
                && self.kind == next.kind
                && self.text == next.text
                && self.tool == next.tool
                && self
                    .tool_header
                    .as_ref()
                    .zip(next.tool_header.as_ref())
                    .is_some_and(|(a, b)| {
                        a.tool_id == b.tool_id
                            && a.turn_id == b.turn_id
                            && a.tool_fingerprint == b.tool_fingerprint
                    }))
    }
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
    thinking_text_version: u64,
}
impl OutputPart {
    pub(crate) fn tool_header(&self) -> Option<&tekes_agent_presentation::TranscriptToolHeader> {
        self.facts.tool_header.as_ref()
    }
    pub(crate) fn slot(&self) -> &Slot {
        &self.facts.slot
    }
    pub(crate) fn ids(&self) -> OutputContentIds {
        self.ids
    }
    pub(crate) fn matches_facts(&self, facts: &PartFacts) -> bool {
        &self.facts == facts
    }
    pub(crate) fn matches_header_update(&self, facts: &PartFacts) -> bool {
        self.facts.matches_header_update(facts)
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
        let body = if facts.tool.is_some() {
            Content::new(ids.body, ToolDetailContent::new(bounded_details))
        } else {
            Content::new(ids.body, field)
        };
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
            thinking_text_version: 0,
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
        if !self.bounded_details || self.ids.disclosure.is_none() || self.facts.tool.is_some() {
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
    pub(crate) fn update(
        &mut self,
        publication: u64,
        epoch: u64,
        facts: PartFacts,
        alloc: &mut dyn FnMut() -> ComponentId,
    ) {
        if self.facts.tool.is_some() != facts.tool.is_some() {
            self.ids.body = alloc();
            let body = if facts.tool.is_some() {
                Content::new(self.ids.body, ToolDetailContent::new(self.bounded_details))
            } else {
                let mut field = TextFieldContent::read_only_content(
                    (self.labels.text)(OutputMessageText::Body),
                    "",
                    proposal::TextRole::UiBody,
                );
                if self.bounded_details {
                    field.surface.scroll.set_scroll_policy(
                        tekes_ui_controls::ScrollPolicy::Disabled,
                        tekes_ui_controls::ScrollPolicy::Auto,
                    );
                    field.reading = Some(tekes_ui_controls::ReadingController::default());
                }
                Content::new(self.ids.body, field)
            };
            if let Some(hidden) = &mut self.hidden {
                *hidden = body;
            } else {
                *self.rows.children.last_mut().unwrap() = body;
            }
        }
        if let Some(tool) = &facts.tool {
            let copy_label = (self.labels.text)(OutputMessageText::CopyDetail);
            self.body_mut()
                .component_mut::<ToolDetailContent>()
                .unwrap()
                .update(publication, epoch, tool, &copy_label, alloc);
            self.facts = facts;
            self.refresh_header();
            return;
        }
        // A passive Thinking status can change during the same Run publication.
        // Its read-only SDK value has its own monotonic widget version; source
        // publication/epoch still gate the parent and every business action.
        let text_version = if facts.slot == Slot::ThinkingStatus {
            if self.thinking_text_version == 0 || self.facts.text != facts.text {
                self.thinking_text_version = self
                    .thinking_text_version
                    .checked_add(1)
                    .expect("preflighted thinking text version");
            }
            self.thinking_text_version
        } else {
            publication
        };
        let id = self.ids.body;
        let field = self.field_mut();
        let baseline = field.surface.input.external_baseline();
        let outcome = field.surface.apply_external_value(
            &baseline,
            &DraftRevision {
                identity: format!("output-content:{}", id.0),
                version: text_version,
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
    fn body_mut(&mut self) -> &mut Content {
        self.hidden
            .as_mut()
            .unwrap_or_else(|| self.rows.children.last_mut().unwrap())
    }
    fn tool_details(&self) -> Option<&ToolDetailContent> {
        self.hidden
            .as_ref()
            .or_else(|| self.rows.children.last())
            .and_then(|node| node.component::<ToolDetailContent>())
    }
    pub(crate) fn tool_detail_sections(&self) -> Vec<crate::ToolDetailSectionIds> {
        self.tool_details()
            .map_or_else(Vec::new, ToolDetailContent::ids)
    }
    pub(crate) fn copy_ids(&self) -> Vec<ComponentId> {
        if self.hidden.is_some() {
            vec![]
        } else {
            self.tool_details()
                .map_or_else(Vec::new, ToolDetailContent::copy_ids)
        }
    }
    pub(crate) fn copy_intents(
        &self,
        scope: &RunScope,
        actions: &[ContentAction],
    ) -> Vec<crate::TranscriptCopyIntent> {
        if self.hidden.is_some() {
            vec![]
        } else {
            self.tool_details()
                .map_or_else(Vec::new, |view| view.copy_intents(scope, actions))
        }
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
        if self.trailing
            && self.hidden.is_none()
            && self.rows.children.len() > 1
            && self.facts.tool.is_none()
        {
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
    state: OutputMessageState,
    split_fragments: std::collections::BTreeMap<ComponentId, Vec<ComponentId>>,
    rows: StackContent,
}
// Canonical identity/provenance and update admission do not own a widget tree.
// The same controller operates on the actual retained SDK nodes supplied by
// its composition. This is the single message algorithm for standalone and
// future turn-level activity/output layouts, not a parallel state machine.
#[derive(Debug)]
struct OutputMessageState {
    root: ComponentId,
    scope: RunScope,
    key: Option<Key>,
    publication: u64,
    epoch: u64,
    header_revision: u64,
    item: Option<TranscriptItem>,
    facts: Vec<PartFacts>,
    next: u64,
    labels: OutputMessageLabels,
    bounded_details: bool,
    transcript_partition: bool,
    thinking: Option<ThinkingStamp>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct ThinkingStamp {
    revision: u64,
    source_fingerprint: String,
    display: tekes_agent_presentation::TranscriptReasoningDisplay,
}
impl OutputMessage {
    pub(crate) fn for_transcript(mut self) -> Self {
        self.state.transcript_partition = true;
        self
    }
    /// Current prose adapter's actual body node text. Rich/opaque content and
    /// failures remain unresolved; never hide their diagnostics by text ranking.
    pub(crate) fn narration_nodes(
        scope: &RunScope,
        output: &TranscriptOutput<'_>,
        section: &TranscriptSection<'_>,
        labels: OutputMessageLabels,
    ) -> Result<Option<Vec<String>>, &'static str> {
        if output.item.failure_message().is_some()
            || !output.item.attachments.is_empty()
            || output.item.content_blocks.iter().flatten().any(|block| {
                matches!(
                    block.content,
                    TranscriptBlockContent::Attachment { .. }
                        | TranscriptBlockContent::Unknown { .. }
                )
            })
        {
            return Ok(None);
        }
        Ok(Some(
            parts(scope, output, Some(section), labels)?
                .into_iter()
                .filter(|part| part.kind == OutputContentKind::Text)
                .map(|part| part.text)
                .collect(),
        ))
    }
    pub fn new(namespace: u32, scope: RunScope) -> Self {
        Self::with_labels(namespace, scope, OutputMessageLabels::default())
    }
    pub fn with_labels(namespace: u32, scope: RunScope, labels: OutputMessageLabels) -> Self {
        Self {
            state: OutputMessageState {
                root: ComponentId((namespace as u64) << 32),
                scope,
                key: None,
                publication: 0,
                epoch: 0,
                header_revision: 0,
                item: None,
                facts: vec![],
                next: 16,
                labels,
                bounded_details: false,
                transcript_partition: false,
                thinking: None,
            },
            split_fragments: Default::default(),
            rows: StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Regular.value(),
                },
                flexible_child: None,
                children: vec![],
            },
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
        view.state.bounded_details = true;
        view
    }
    pub fn id(&self) -> ComponentId {
        self.state.root
    }
    pub fn scope(&self) -> &RunScope {
        &self.state.scope
    }
    pub fn content_ids(&self) -> Vec<OutputContentIds> {
        self.part_nodes()
            .into_iter()
            .map(|node| node.component::<OutputPart>().unwrap().ids)
            .collect()
    }
    /// Read-only captured headers, including members of a collapsed group.
    /// Semantic runs are facts; styled inline rendering awaits SDK #25.
    pub fn tool_headers(&self) -> Vec<&tekes_agent_presentation::TranscriptToolHeader> {
        self.part_nodes()
            .into_iter()
            .filter_map(|node| node.component::<OutputPart>()?.tool_header())
            .collect()
    }
    /// Structured body groups expose their original section owners here;
    /// `OutputContentIds::body` itself is a group for a structured tool.
    pub fn tool_detail_sections(&self) -> Vec<crate::ToolDetailSectionIds> {
        self.part_nodes()
            .into_iter()
            .flat_map(|node| {
                node.component::<OutputPart>()
                    .unwrap()
                    .tool_detail_sections()
            })
            .collect()
    }
    pub fn copy_action_ids(&self) -> Vec<ComponentId> {
        self.visible_part_nodes()
            .into_iter()
            .flat_map(|node| node.component::<OutputPart>().unwrap().copy_ids())
            .collect()
    }
    pub fn take_copy_intents(&self, actions: &[ContentAction]) -> Vec<crate::TranscriptCopyIntent> {
        self.visible_part_nodes()
            .into_iter()
            .flat_map(|node| {
                node.component::<OutputPart>()
                    .unwrap()
                    .copy_intents(&self.state.scope, actions)
            })
            .collect()
    }
    fn part_nodes(&self) -> Vec<&Content> {
        part_nodes(&self.rows.children)
    }
    fn visible_part_nodes(&self) -> Vec<&Content> {
        visible_part_nodes(&self.rows.children)
    }
    pub fn tool_run(&self, tool_id: &str) -> Option<&crate::ToolRun> {
        self.rows
            .children
            .iter()
            .filter_map(|node| node.component::<crate::ToolRun>())
            .find(|group| {
                group
                    .summary()
                    .is_some_and(|m| m.tool_ids.iter().any(|id| id == tool_id))
            })
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
        if !self.split_fragments.is_empty() {
            return Err("split output requires its original Transcript fragments");
        }
        if scope != &self.state.scope {
            return Err("output message scope mismatch");
        }
        self.state.update_display(
            &mut self.rows.children,
            publication,
            epoch,
            output,
            section,
            None,
        )
    }
    /// Install the shared live-thinking decision for this original message.
    /// Once controlled, callers must keep supplying a matching frame; legacy
    /// refresh cannot accidentally restore hidden reasoning or old actions.
    pub fn update_with_thinking(
        &mut self,
        publication: u64,
        epoch: u64,
        output: &TranscriptOutput<'_>,
        section: Option<&TranscriptSection<'_>>,
        thinking: &tekes_agent_presentation::TranscriptThinkingFrame,
    ) -> Result<(), &'static str> {
        if !self.split_fragments.is_empty() {
            return Err("split output requires its original Transcript fragments");
        }
        let stamp = self
            .state
            .thinking_stamp(publication, epoch, output, thinking)?;
        self.state.update_display(
            &mut self.rows.children,
            publication,
            epoch,
            output,
            section,
            Some(stamp),
        )
    }
    pub(crate) fn check_update_with_thinking(
        &self,
        publication: u64,
        epoch: u64,
        output: &TranscriptOutput<'_>,
        section: Option<&TranscriptSection<'_>>,
        thinking: &tekes_agent_presentation::TranscriptThinkingFrame,
    ) -> Result<(), &'static str> {
        if !self.split_fragments.is_empty() {
            return Err("split output requires its original Transcript fragments");
        }
        let stamp = self
            .state
            .thinking_stamp(publication, epoch, output, thinking)?;
        self.state.check_update_display(
            self.rows.children.iter(),
            publication,
            epoch,
            output,
            section,
            Some(&stamp),
        )?;
        Ok(())
    }

    pub(crate) fn check_update(
        &self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        output: &TranscriptOutput<'_>,
        section: Option<&TranscriptSection<'_>>,
    ) -> Result<(Key, Vec<PartFacts>, u64), &'static str> {
        if !self.split_fragments.is_empty() {
            return Err("split output requires its original Transcript fragments");
        }
        if scope != &self.state.scope {
            return Err("output message scope mismatch");
        }
        self.state.check_update_display(
            self.rows.children.iter(),
            publication,
            epoch,
            output,
            section,
            None,
        )
    }

    /// UI-local disclosure only. The host drains the original ContentHost action
    /// batch exactly once; retired button IDs cannot target a replacement block.
    pub fn handle_actions(&mut self, actions: &[ContentAction]) -> bool {
        handle_part_actions(&mut self.rows.children, actions)
    }

    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let id = self.state.root;
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
            (self.state.labels.text)(OutputMessageText::Group),
        )
    }
}
impl OutputMessageState {
    fn source_parts(
        &self,
        output: &TranscriptOutput<'_>,
        section: Option<&TranscriptSection<'_>>,
    ) -> Result<Vec<PartFacts>, &'static str> {
        if self.transcript_partition {
            transcript_parts(&self.scope, output, section, self.labels)
        } else {
            parts(&self.scope, output, section, self.labels)
        }
    }
    fn thinking_stamp(
        &self,
        publication: u64,
        epoch: u64,
        output: &TranscriptOutput<'_>,
        thinking: &tekes_agent_presentation::TranscriptThinkingFrame,
    ) -> Result<ThinkingStamp, &'static str> {
        if !thinking.matches_source(&self.scope, &output.item.turn_id, publication, epoch) {
            return Err("thinking frame source mismatch");
        }
        let decision = thinking.display_for(&output.item.id);
        if decision.is_none() && !output.item.display_reasoning().trim().is_empty() {
            return Err("thinking frame is missing reasoning source");
        }
        let stamp = ThinkingStamp {
            revision: thinking.display_revision(),
            source_fingerprint: thinking.source_fingerprint().into(),
            display: decision
                .unwrap_or(tekes_agent_presentation::TranscriptReasoningDisplay::Hidden),
        };
        Ok(stamp)
    }
    fn update_display(
        &mut self,
        nodes: &mut Vec<Content>,
        publication: u64,
        epoch: u64,
        output: &TranscriptOutput<'_>,
        section: Option<&TranscriptSection<'_>>,
        thinking: Option<ThinkingStamp>,
    ) -> Result<(), &'static str> {
        let (key, facts, header_revision) = self.check_update_display(
            nodes.iter(),
            publication,
            epoch,
            output,
            section,
            thinking.as_ref(),
        )?;
        let mut old = vec![];
        let mut old_groups = vec![];
        for mut node in std::mem::take(nodes) {
            if let Some(group) = node.component_mut::<crate::ToolRun>() {
                old.extend(group.take_members());
                old_groups.push(node);
            } else {
                old.push(node);
            }
        }
        let mut installed = vec![];
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
            part.live_reasoning = thinking.as_ref().map_or(
                output.item.is_provisional && !output.item.reasoning_ended,
                |stamp| {
                    matches!(
                        stamp.display,
                        tekes_agent_presentation::TranscriptReasoningDisplay::Disclosure {
                            active: true
                        }
                    )
                },
            );
            let base = self.root.0;
            let next = &mut self.next;
            part.update(publication, epoch, f.clone(), &mut || {
                let id = ComponentId(base + *next);
                *next += 1;
                id
            });
            installed.push(content);
        }
        let mut pending = installed.into_iter();
        for range in tool_part_runs(&facts, section)? {
            let run: Vec<_> = pending.by_ref().take(range.len()).collect();
            if run.len() < 2 {
                nodes.extend(run);
                continue;
            }
            let ids: Vec<_> = run
                .iter()
                .map(|node| {
                    node.component::<OutputPart>()
                        .unwrap()
                        .facts
                        .tool
                        .as_ref()
                        .unwrap()
                        .tool_id
                        .as_str()
                })
                .collect();
            let tools = ids
                .iter()
                .map(|id| {
                    section
                        .unwrap()
                        .tools
                        .iter()
                        .copied()
                        .find(|tool| tool.id == *id && tool.turn_id == output.item.turn_id)
                        .unwrap()
                })
                .collect::<Vec<_>>();
            let model = self.labels.tool_run.summary(&self.scope, &tools)?;
            let retained = old_groups.iter().position(|node| {
                node.component::<crate::ToolRun>()
                    .unwrap()
                    .summary()
                    .is_some_and(|old| {
                        old.turn_id == model.turn_id
                            && old.tool_ids.iter().any(|id| model.tool_ids.contains(id))
                    })
            });
            let mut node = retained
                .map(|index| old_groups.remove(index))
                .unwrap_or_else(|| {
                    let id = ComponentId(self.root.0 + self.next);
                    self.next += 1;
                    Content::new(
                        id,
                        crate::ToolRun::at_root(id, self.scope.clone(), self.labels.tool_run),
                    )
                });
            let base = self.root.0;
            let next = &mut self.next;
            node.component_mut::<crate::ToolRun>().unwrap().install(
                publication,
                epoch,
                model,
                run,
                &mut || {
                    let id = ComponentId(base + *next);
                    *next += 1;
                    id
                },
            );
            nodes.push(node);
        }
        self.key = Some(key);
        self.publication = publication;
        self.epoch = epoch;
        self.item = Some(output.item.clone());
        self.facts = facts;
        self.header_revision = header_revision;
        self.thinking = thinking;
        Ok(())
    }
    fn check_update_display<'nodes>(
        &self,
        nodes: impl IntoIterator<Item = &'nodes Content>,
        publication: u64,
        epoch: u64,
        output: &TranscriptOutput<'_>,
        section: Option<&TranscriptSection<'_>>,
        thinking: Option<&ThinkingStamp>,
    ) -> Result<(Key, Vec<PartFacts>, u64), &'static str> {
        let scope = &self.scope;
        if publication == 0 || publication < self.publication || epoch < self.epoch {
            return Err("stale output message publication");
        }
        if !output.item.is_visible || output.item.kind == TranscriptItemKind::User {
            return Err("output is not a visible machine message");
        }
        if section.is_some_and(|s| s.id != output.item.turn_id) {
            return Err("output tool section mismatch");
        }
        if let Some(old) = &self.thinking {
            let Some(next) = thinking else {
                return Err("controlled thinking frame is required");
            };
            if epoch == self.epoch {
                if next.revision < old.revision {
                    return Err("stale thinking display revision");
                }
                if publication == self.publication
                    && (next.source_fingerprint != old.source_fingerprint
                        || (next.revision == old.revision && next != old))
                {
                    return Err("conflicting thinking display frame");
                }
            }
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
        let header_revision = tool_header_revision(self.labels, scope);
        if epoch == self.epoch && header_revision < self.header_revision {
            return Err("stale tool header revision");
        }
        let facts = thinking_parts(self.source_parts(output, section)?, thinking, self.labels);
        let thinking_advanced = thinking.is_some_and(|next| {
            self.thinking
                .as_ref()
                .is_none_or(|old| next.revision > old.revision)
        });
        let prior_display = if thinking_advanced {
            Some(thinking_parts(
                self.source_parts(output, section)?,
                self.thinking.as_ref(),
                self.labels,
            ))
        } else {
            None
        };
        if header_revision != tool_header_revision(self.labels, scope) {
            return Err("tool header inputs changed during preparation");
        }
        if publication == self.publication
            && (epoch != self.epoch
                || self.item.as_ref() != Some(output.item)
                || (self.facts != facts
                    && !(header_revision > self.header_revision
                        && self.facts.len() == facts.len()
                        && self
                            .facts
                            .iter()
                            .zip(&facts)
                            .all(|(old, new)| old.matches_header_update(new)))
                    && !prior_display.as_ref().is_some_and(|prior| {
                        &self.facts == prior
                            || (header_revision > self.header_revision
                                && self.facts.len() == prior.len()
                                && self
                                    .facts
                                    .iter()
                                    .zip(prior)
                                    .all(|(old, new)| old.matches_header_update(new)))
                    })))
        {
            return Err("conflicting output message publication");
        }
        let nodes: Vec<_> = nodes.into_iter().collect();
        for next in facts
            .iter()
            .filter(|part| part.slot == Slot::ThinkingStatus)
        {
            if part_nodes_iter(nodes.iter().copied())
                .into_iter()
                .filter_map(|node| node.component::<OutputPart>())
                .any(|part| {
                    part.facts.slot == Slot::ThinkingStatus
                        && part.thinking_text_version == u64::MAX
                        && part.facts.text != next.text
                })
            {
                return Err("thinking text version exhausted");
            }
        }
        for range in tool_part_runs(&facts, section)? {
            if range.len() > 1 {
                let tools = facts[range]
                    .iter()
                    .map(|part| {
                        let id = &part.tool.as_ref().unwrap().tool_id;
                        section
                            .unwrap()
                            .tools
                            .iter()
                            .copied()
                            .find(|tool| &tool.id == id)
                            .unwrap()
                    })
                    .collect::<Vec<_>>();
                self.labels.tool_run.summary(&self.scope, &tools)?;
            }
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
            .checked_add(new_count as u64 * 3 + facts.len() as u64 * 32)
            .is_none_or(|v| v >= u32::MAX as u64)
        {
            return Err("output message namespace exhausted");
        }
        Ok((key, facts, header_revision))
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
        if let (Slot::Tool(_, a), Slot::Tool(_, b)) = (&old.slot, &new.slot)
            && a == b
            && self
                .facts
                .iter()
                .filter(|part| matches!(&part.slot,Slot::Tool(_,id) if id==a))
                .count()
                == 1
            && facts
                .iter()
                .filter(|part| matches!(&part.slot,Slot::Tool(_,id) if id==b))
                .count()
                == 1
        {
            return true;
        }
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
}

fn handle_part_actions(nodes: &mut [Content], actions: &[ContentAction]) -> bool {
    fn toggle(part: &mut OutputPart, actions: &[ContentAction]) -> bool {
        let id = part.ids.disclosure;
        let count = actions
            .iter()
            .filter(|a| a.kind == ContentActionKind::Activate && id == Some(a.target))
            .count();
        for _ in 0..count {
            part.toggle();
        }
        count > 0
    }
    let mut changed = false;
    for node in nodes {
        if let Some(group) = node.component_mut::<crate::ToolRun>() {
            changed |= group.handle_disclosure(actions);
            if group.members_visible() {
                for node in group.member_nodes_mut() {
                    changed |= toggle(node.component_mut::<OutputPart>().unwrap(), actions);
                }
            }
        } else {
            changed |= toggle(node.component_mut::<OutputPart>().unwrap(), actions);
        }
    }
    changed
}

fn visible_part_nodes(nodes: &[Content]) -> Vec<&Content> {
    nodes
        .iter()
        .flat_map(|node| {
            if let Some(group) = node.component::<crate::ToolRun>() {
                if group.members_visible() {
                    group.member_nodes().collect()
                } else {
                    vec![]
                }
            } else {
                vec![node]
            }
        })
        .collect()
}

fn part_nodes(nodes: &[Content]) -> Vec<&Content> {
    part_nodes_iter(nodes.iter())
}
fn part_nodes_iter<'a>(nodes: impl IntoIterator<Item = &'a Content>) -> Vec<&'a Content> {
    nodes
        .into_iter()
        .flat_map(|node| {
            if let Some(group) = node.component::<crate::ToolRun>() {
                group.member_nodes().collect()
            } else {
                vec![node]
            }
        })
        .collect()
}

fn thinking_parts(
    facts: Vec<PartFacts>,
    thinking: Option<&ThinkingStamp>,
    labels: OutputMessageLabels,
) -> Vec<PartFacts> {
    use tekes_agent_presentation::TranscriptReasoningDisplay;
    let Some(thinking) = thinking else {
        return facts;
    };
    // ClientKit presents one reasoning detail before this message's content,
    // even when the protocol interleaves multiple reasoning blocks. Keep source
    // DTOs/copy text intact and give the retained detail one stable local owner.
    let reasoning: String = facts
        .iter()
        .filter(|part| part.kind == OutputContentKind::Reasoning)
        .map(|part| part.text.as_str())
        .collect();
    let mut result: Vec<_> = facts
        .into_iter()
        .filter(|part| {
            part.kind != OutputContentKind::Reasoning
                && (part.kind != OutputContentKind::Coverage
                    || (!reasoning.trim().is_empty()
                        && matches!(
                            thinking.display,
                            TranscriptReasoningDisplay::Disclosure { .. }
                        )))
        })
        .collect();
    if reasoning.trim().is_empty() {
        return result;
    }
    let part = match thinking.display {
        TranscriptReasoningDisplay::Hidden => return result,
        TranscriptReasoningDisplay::Placeholder { dots } => PartFacts {
            slot: Slot::ThinkingStatus,
            kind: OutputContentKind::Status,
            title: String::new(),
            text: format!(
                "{}{}",
                (labels.text)(OutputMessageText::Thinking),
                ".".repeat(dots as usize)
            ),
            tool: None,
            tool_header: None,
        },
        TranscriptReasoningDisplay::Disclosure { active } => PartFacts {
            slot: Slot::AggregateReasoning,
            kind: OutputContentKind::Reasoning,
            title: (labels.text)(if active {
                OutputMessageText::Thinking
            } else {
                OutputMessageText::ReasoningEnded
            }),
            text: reasoning.trim().to_owned(),
            tool: None,
            tool_header: None,
        },
    };
    result.insert(0, part);
    result
}
fn input_between_tools(
    section: Option<&TranscriptSection<'_>>,
    first: &str,
    second: &str,
) -> Result<bool, &'static str> {
    section.map_or(Ok(false), |section| {
        section
            .has_input_between_tools(first, second)
            .map(|boundary| boundary == Some(true))
            .map_err(|_| "invalid embedded tool source order")
    })
}
// One partition for preflight, original node construction and parent planning.
fn tool_part_runs(
    facts: &[PartFacts],
    section: Option<&TranscriptSection<'_>>,
) -> Result<Vec<std::ops::Range<usize>>, &'static str> {
    let unique = |part: &PartFacts| {
        part.tool.as_ref().is_some_and(|tool| {
            facts
                .iter()
                .filter(|candidate| {
                    candidate
                        .tool
                        .as_ref()
                        .is_some_and(|value| value.tool_id == tool.tool_id)
                })
                .count()
                == 1
        })
    };
    let mut result = vec![];
    let mut start = 0;
    while start < facts.len() {
        let mut end = start + 1;
        if unique(&facts[start]) {
            while end < facts.len() {
                if !unique(&facts[end]) {
                    break;
                }
                let previous = facts[end - 1].tool.as_ref().unwrap();
                let next = facts[end].tool.as_ref().unwrap();
                if input_between_tools(section, &previous.tool_id, &next.tool_id)? {
                    break;
                }
                end += 1;
            }
        }
        result.push(start..end);
        start = end;
    }
    Ok(result)
}
fn transcript_parts(
    scope: &RunScope,
    output: &TranscriptOutput<'_>,
    section: Option<&TranscriptSection<'_>>,
    labels: OutputMessageLabels,
) -> Result<Vec<PartFacts>, &'static str> {
    let mut facts = parts(scope, output, section, labels)?;
    facts.retain(|part| {
        let Slot::Tool(_, id) = &part.slot else {
            return true;
        };
        // Canonical calls have one independent ToolActivity across message
        // updates. Only unresolved references remain message-owned content.
        !section.is_some_and(|section| section.tools.iter().any(|tool| &tool.id == id))
    });
    Ok(facts)
}
fn parts(
    scope: &RunScope,
    output: &TranscriptOutput<'_>,
    section: Option<&TranscriptSection<'_>>,
    labels: OutputMessageLabels,
) -> Result<Vec<PartFacts>, &'static str> {
    let item = output.item;
    let mut result = vec![];
    if let Some(prompt) = item.system_prompt() {
        return Ok(vec![PartFacts {
            tool: None,
            tool_header: None,
            slot: Slot::SystemPrompt,
            kind: OutputContentKind::SystemPrompt,
            title: (labels.text)(
                if prompt.placement
                    == tekes_agent_presentation::TranscriptSystemPromptPlacement::Update
                {
                    OutputMessageText::SystemPromptUpdate
                } else {
                    OutputMessageText::SystemPrompt
                },
            ),
            text: item.text.clone(),
        }]);
    }
    let status = match item.kind {
        TranscriptItemKind::System => Some(OutputMessageText::System),
        TranscriptItemKind::Error => Some(OutputMessageText::Error),
        TranscriptItemKind::Notice => Some(OutputMessageText::Notice),
        _ => None,
    };
    if let Some(status) = status {
        result.push(PartFacts {
            tool: None,
            tool_header: None,
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
            tool: None,
            tool_header: None,
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
            let mut tool_details = None;
            let mut tool_header = None;
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
                    tool_details = tool.and_then(|tool| tool_details_facts(scope, tool, labels));
                    let (title, text, header) = tool.map_or_else(
                        || {
                            (
                                format!(
                                    "{} · {tool_id}",
                                    (labels.text)(OutputMessageText::MissingTool)
                                ),
                                tool_id.clone(),
                                None,
                            )
                        },
                        |tool| tool_text(scope, tool, labels),
                    );
                    tool_header = header;
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
                tool: tool_details,
                tool_header,
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
                tool: None,
                tool_header: None,
                slot: Slot::Coverage,
                kind: OutputContentKind::Coverage,
                title: String::new(),
                text: (labels.text)(OutputMessageText::AggregateOrder),
            });
        }
        if !item.reasoning.is_empty() {
            result.push(PartFacts {
                tool: None,
                tool_header: None,
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
                tool: None,
                tool_header: None,
                slot: Slot::AggregateBody,
                kind: OutputContentKind::Text,
                title: String::new(),
                text: item.text.clone(),
            });
        }
        for (i, attachment) in item.attachments.iter().enumerate() {
            result.push(PartFacts {
                tool: None,
                tool_header: None,
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
            tool: None,
            tool_header: None,
            slot: Slot::Empty,
            kind: OutputContentKind::Status,
            title: String::new(),
            text: (labels.text)(OutputMessageText::Empty),
        });
    }
    for facts in &result {
        validate_tool_details(facts)?;
    }
    Ok(result)
}
