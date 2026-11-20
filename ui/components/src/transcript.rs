//! Structured Transcript surface over `RunSnapshot`, `TranscriptDocument`, `ReadingController`,
//! `RichContent`, `DisclosureContent`, and inline `ActionablePrompt` cards.

use crate::ResponsiveWidthMode;
use crate::ids::*;
use std::collections::BTreeMap;
use tekes_agent_presentation::{
    ActionablePrompt, ConnectionPhase, RunPhase, RunScope, RunSnapshot, TranscriptDocument,
    TranscriptItemKind, TranscriptToolReportWording, TranscriptToolRowInputs,
    TranscriptToolRowWording,
};
use tekes_ui_controls::{
    ButtonTextRenderer, ButtonVariant, ReadingAnchorUpdate, ReadingController,
    ReadingDocumentVersion, ScrollArea, TextRangeMapping, TextSurface,
    composition::{
        AccessibilityAction, Button, Component, ComponentId, Content, ContentActionKind,
        ContentActionPayload, ContentActionRecord, ContentEvent, ContentHost, ContentResponse,
        ContentRole, ContentSemantics, ContentTheme, CurveSubtreeCapability, DisclosureContent,
        RichContent, RichDocument, ScrollContent, SearchCommandKind, SearchFieldContent,
        StackContent, TextContent,
    },
    extended::{Axis, Stack},
    layout::MeasuredChild,
};
use tekes_ui_scene::{Rect, Scene, Size};
use tekes_ui_tokens::{Spacing, proposal};

#[derive(Clone, Debug, PartialEq)]
pub enum TranscriptIntent {
    LoadEarlier(RunScope),
    RespondActionable {
        scope: RunScope,
        actionable_id: String,
        revision: i64,
        choice_index: usize,
    },
    OpenLink(String),
}

/// Scrollable reading surface driven by `ReadingController` so prepend pagination preserves
/// scroll anchors accurately across AppHost renders and slot swaps.
#[derive(Debug)]
pub struct TranscriptReader {
    pub surface: TextSurface,
    pub reading: ReadingController,
    doc_key: String,
    doc_revision: u64,
    doc_version: Option<ReadingDocumentVersion>,
    doc_text: String,
    focused: bool,
    pub last_anchor_update: Option<ReadingAnchorUpdate>,
}

impl Default for TranscriptReader {
    fn default() -> Self {
        Self::new()
    }
}

impl TranscriptReader {
    pub fn new() -> Self {
        let mut surface = TextSurface::new("", true, true);
        surface.set_role(proposal::TextRole::UiBody);
        Self {
            surface,
            reading: ReadingController::default(),
            doc_key: String::new(),
            doc_revision: 0,
            doc_version: None,
            doc_text: String::new(),
            focused: false,
            last_anchor_update: None,
        }
    }

    pub fn document_version(&self) -> Option<&ReadingDocumentVersion> {
        self.doc_version.as_ref()
    }

    pub fn follow_latest(&mut self) {
        self.reading.follow_latest(&mut self.surface);
    }

    pub fn stop_following(&mut self) {
        self.reading.stop_following(&mut self.surface);
    }

    pub fn set_or_update(&mut self, key: &str, new_text: &str) {
        if self.doc_key == key && self.doc_text == new_text {
            return;
        }
        let next_rev = self.doc_revision.saturating_add(1).max(1);
        if self.doc_key == key
            && let Some(expected) = &self.doc_version
            && !self.doc_text.is_empty()
            && new_text.len() > self.doc_text.len()
            && new_text.ends_with(&self.doc_text)
        {
            let prefix_len = new_text.len() - self.doc_text.len();
            let prefix_utf16 = new_text[..prefix_len].encode_utf16().count();
            let old_utf16 = self.doc_text.encode_utf16().count();
            if let Ok(update) = self.reading.update_document(
                &mut self.surface,
                expected,
                next_rev,
                new_text,
                &[TextRangeMapping {
                    old: 0..old_utf16,
                    new: prefix_utf16..(prefix_utf16 + old_utf16),
                }],
            ) {
                self.last_anchor_update = Some(update.anchor);
                self.doc_version = Some(update.version);
                self.doc_revision = next_rev;
                self.doc_text = new_text.to_owned();
                return;
            }
        }
        if self.doc_key == key
            && let Some(expected) = &self.doc_version
            && let Ok(update) =
                self.reading
                    .update_document(&mut self.surface, expected, next_rev, new_text, &[])
        {
            self.last_anchor_update = Some(update.anchor);
            self.doc_version = Some(update.version);
            self.doc_revision = next_rev;
            self.doc_text = new_text.to_owned();
            return;
        }

        if let Ok(ver) = self
            .reading
            .set_document(&mut self.surface, key, next_rev, new_text)
        {
            self.doc_key = key.to_owned();
            self.doc_revision = next_rev;
            self.doc_version = Some(ver);
            self.doc_text = new_text.to_owned();
            self.last_anchor_update = None;
        }
    }
}

impl Component for TranscriptReader {
    fn curve_subtree_capability(&self) -> CurveSubtreeCapability {
        CurveSubtreeCapability::Local
    }

    fn has_stable_hooks(&self) -> bool {
        true
    }

    fn supports_prepared_lifecycle(&self) -> bool {
        true
    }

    fn measure(&mut self, _: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        MeasuredChild::new(Size::new(available.width, available.height.max(120.0)))
    }

    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        let scale = text.backing_scale();
        let role = self.surface.role();
        if let Some(host) = text.text_surface_host() {
            let type_scale = host.type_scale();
            self.reading.layout_with(
                &mut self.surface,
                frame,
                Some(clip),
                scale,
                type_scale,
                |s, w| host.shape_surface(s, role, w),
            );
        }
    }

    fn paint(
        &mut self,
        scene: &mut Scene,
        text: &mut dyn ButtonTextRenderer,
        _frame: Rect,
        _clip: Rect,
        theme: ContentTheme,
    ) {
        if let Some(host) = text.text_surface_host() {
            self.surface
                .draw(host, scene, theme.palette, self.focused, false);
        }
    }

    fn event(&mut self, event: &ContentEvent) -> ContentResponse {
        match event {
            ContentEvent::Scroll { delta, .. } => {
                self.surface.scroll_mut().scroll_by(delta.x, delta.y);
                if delta.y < -1.0 {
                    self.reading.stop_following(&mut self.surface);
                }
                ContentResponse::consumed(true)
            }
            _ => ContentResponse::default(),
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool, _visible: bool) {
        self.focused = focused;
    }

    fn text_surface(&self) -> Option<&TextSurface> {
        Some(&self.surface)
    }

    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Text, "会话消息流")
    }

    fn accessibility_action(
        &mut self,
        _part: Option<usize>,
        _action: &AccessibilityAction,
    ) -> ContentResponse {
        ContentResponse::default()
    }
}

#[derive(Debug, Clone)]
struct ActionableChoiceBinding {
    scope: RunScope,
    actionable_id: String,
    revision: i64,
    choice_index: usize,
}

#[derive(Debug)]
pub struct TranscriptSurface {
    selected_key: String,
    cached_readers: BTreeMap<String, Content>,
    search_query: String,
    thinking_expanded: bool,
    tool_expanded: bool,
    rich_expanded: bool,
    rich_doc_revision: u64,
    last_rich_source: String,
    choice_bindings: BTreeMap<ComponentId, ActionableChoiceBinding>,
    rendered_actionables_sig: String,
}

impl Default for TranscriptSurface {
    fn default() -> Self {
        Self::new()
    }
}

impl TranscriptSurface {
    pub fn new() -> Self {
        Self {
            selected_key: String::new(),
            cached_readers: BTreeMap::new(),
            search_query: String::new(),
            thinking_expanded: false,
            tool_expanded: true,
            rich_expanded: true,
            rich_doc_revision: 1,
            last_rich_source: String::new(),
            choice_bindings: BTreeMap::new(),
            rendered_actionables_sig: String::new(),
        }
    }

    pub fn build_content() -> Content {
        let mut earlier_btn = Button::new("更早记录");
        earlier_btn.variant = ButtonVariant::Quiet;
        let mut latest_btn = Button::new("回到最新");
        latest_btn.variant = ButtonVariant::Quiet;

        let header = Content::new(
            TRANSCRIPT_HEADER_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: Some(TRANSCRIPT_STATUS_ID),
                children: vec![
                    Content::new(TRANSCRIPT_STATUS_ID, TextContent::new("正在连接…")),
                    Content::new(TRANSCRIPT_EARLIER_BTN_ID, earlier_btn),
                    Content::new(TRANSCRIPT_LATEST_BTN_ID, latest_btn),
                ],
            },
        );

        let mut search = SearchFieldContent::new("搜索会话消息", "");
        search.set_placeholder(Some("过滤消息关键字…".to_owned()));
        let search_row = Content::new(
            TRANSCRIPT_SEARCH_ROW_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: Some(TRANSCRIPT_SEARCH_ID),
                children: vec![Content::new(TRANSCRIPT_SEARCH_ID, search)],
            },
        );

        let thinking_disc = Content::new(
            TRANSCRIPT_THINKING_DISC_ID,
            DisclosureContent::new(
                TRANSCRIPT_THINKING_TRIGGER_ID,
                Content::new(ComponentId(113), TextContent::new("思考过程")),
                Content::new(TRANSCRIPT_THINKING_BODY_ID, TextContent::new("（暂无）")),
                false,
            )
            .with_section_header_layout(),
        );

        let tool_disc = Content::new(
            TRANSCRIPT_TOOL_DISC_ID,
            DisclosureContent::new(
                TRANSCRIPT_TOOL_TRIGGER_ID,
                Content::new(ComponentId(123), TextContent::new("工具执行报告")),
                Content::new(TRANSCRIPT_TOOL_BODY_ID, TextContent::new("（暂无）")),
                true,
            )
            .with_section_header_layout(),
        );

        let rich_doc = RichDocument::from_markdown("等待会话消息…");
        let rich_content = RichContent::new("transcript-rich-latest", 1, rich_doc)
            .expect("valid initial RichContent");
        let mut rich_scroll = ScrollContent::new(
            ScrollArea::default(),
            Content::new(TRANSCRIPT_RICH_BODY_ID, rich_content),
        );
        rich_scroll.viewport_height = Some(130.0);
        let rich_disc = Content::new(
            TRANSCRIPT_RICH_DISC_ID,
            DisclosureContent::new(
                TRANSCRIPT_RICH_TRIGGER_ID,
                Content::new(
                    ComponentId(134),
                    TextContent::new("最新 Markdown / 代码卡片"),
                ),
                Content::new(TRANSCRIPT_RICH_SCROLL_ID, rich_scroll),
                true,
            )
            .with_section_header_layout(),
        );

        let actionables_stack = Content::new(
            TRANSCRIPT_ACTIONABLES_STACK_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: Vec::new(),
            },
        );

        Content::new(
            TRANSCRIPT_ROOT_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: Some(TRANSCRIPT_READER_ID),
                children: vec![
                    header,
                    search_row,
                    Content::new(TRANSCRIPT_READER_ID, TranscriptReader::new()),
                    thinking_disc,
                    tool_disc,
                    rich_disc,
                    actionables_stack,
                ],
            },
        )
    }

    pub fn sync(
        &mut self,
        host: &mut ContentHost,
        snapshot: &RunSnapshot,
        actionables: &[ActionablePrompt],
        _width_mode: ResponsiveWidthMode,
    ) {
        let dest_key = snapshot
            .selection
            .as_ref()
            .map(|s| s.destination.storage_key())
            .unwrap_or_default();
        if self.selected_key != dest_key {
            if let Ok(old_reader) = host.remove(TRANSCRIPT_READER_ID)
                && !self.selected_key.is_empty()
            {
                self.cached_readers
                    .insert(self.selected_key.clone(), old_reader);
            }
            let next_reader = self
                .cached_readers
                .remove(&dest_key)
                .unwrap_or_else(|| Content::new(TRANSCRIPT_READER_ID, TranscriptReader::new()));
            let _ = host.insert(TRANSCRIPT_ROOT_ID, next_reader);
            let _ = host.reorder(
                TRANSCRIPT_ROOT_ID,
                &[
                    TRANSCRIPT_HEADER_ID,
                    TRANSCRIPT_SEARCH_ROW_ID,
                    TRANSCRIPT_READER_ID,
                    TRANSCRIPT_THINKING_DISC_ID,
                    TRANSCRIPT_TOOL_DISC_ID,
                    TRANSCRIPT_RICH_DISC_ID,
                    TRANSCRIPT_ACTIONABLES_STACK_ID,
                ],
            );
            self.selected_key = dest_key.clone();
        }

        if let Some(search) = host
            .get(TRANSCRIPT_SEARCH_ID)
            .and_then(|c| c.component::<SearchFieldContent>())
        {
            self.search_query = search.query().to_owned();
        }

        let phase_label = match snapshot.connection.phase {
            ConnectionPhase::Connecting => "连接中",
            ConnectionPhase::Connected => "已连接",
            ConnectionPhase::Reconnecting => "重连中",
            ConnectionPhase::Degraded => "降级只读",
            ConnectionPhase::Failed => "连接失败",
            ConnectionPhase::Stopped => "已停止",
        };
        let run_label = match snapshot.run.as_ref().map(|r| &r.phase) {
            Some(RunPhase::Running) => "运行中",
            Some(RunPhase::WaitingForInput) => "等待确认",
            Some(RunPhase::Idle) => "就绪",
            Some(RunPhase::Loading) => "加载中",
            Some(RunPhase::Interrupted) => "已中断",
            Some(RunPhase::Archived) => "已归档",
            _ => "未启动",
        };
        let session_label = snapshot
            .selection
            .as_ref()
            .map(|s| {
                s.destination.endpoint_id().to_owned()
                    + " · "
                    + match &s.destination {
                        tekes_agent_presentation::ComposerDestination::Run { scope } => {
                            scope.session_id.as_str()
                        }
                        _ => "新对话",
                    }
            })
            .unwrap_or_else(|| "未选择会话".to_owned());

        if let Some(status) = host
            .get_mut(TRANSCRIPT_STATUS_ID)
            .and_then(|c| c.component_mut::<TextContent>())
        {
            let stale_mark = if snapshot.transcript.is_stale {
                " · 历史待刷新"
            } else {
                ""
            };
            status.text = format!("{phase_label} | {run_label} | {session_label}{stale_mark}");
        }

        if let Some(earlier) = host
            .get_mut(TRANSCRIPT_EARLIER_BTN_ID)
            .and_then(|c| c.component_mut::<Button>())
        {
            earlier.button.state.disabled = !snapshot.transcript.has_more_before;
        }

        let (formatted_doc, latest_thinking, latest_tool_text, latest_rich_md) =
            format_transcript(snapshot, &self.search_query);

        let reader_key = format!("transcript:{dest_key}:filter:{}", self.search_query);
        if let Some(reader) = host
            .get_mut(TRANSCRIPT_READER_ID)
            .and_then(|c| c.component_mut::<TranscriptReader>())
        {
            reader.set_or_update(&reader_key, &formatted_doc);
        }

        if let Some(body) = host
            .get_mut(TRANSCRIPT_THINKING_BODY_ID)
            .and_then(|c| c.component_mut::<TextContent>())
        {
            body.text = if latest_thinking.is_empty() {
                "当前轮次无思考推理文本。".to_owned()
            } else {
                latest_thinking
            };
        }
        if let Some(disc) = host
            .get_mut(TRANSCRIPT_THINKING_DISC_ID)
            .and_then(|c| c.component_mut::<DisclosureContent>())
        {
            disc.set_expanded(self.thinking_expanded);
        }

        if let Some(body) = host
            .get_mut(TRANSCRIPT_TOOL_BODY_ID)
            .and_then(|c| c.component_mut::<TextContent>())
        {
            body.text = if latest_tool_text.is_empty() {
                "当前轮次无工具调用记录。".to_owned()
            } else {
                latest_tool_text
            };
        }
        if let Some(disc) = host
            .get_mut(TRANSCRIPT_TOOL_DISC_ID)
            .and_then(|c| c.component_mut::<DisclosureContent>())
        {
            disc.set_expanded(self.tool_expanded);
        }

        if self.last_rich_source != latest_rich_md {
            let next_rev = self.rich_doc_revision.saturating_add(1).max(1);
            let doc = RichDocument::from_markdown(&latest_rich_md);
            if let Some(rich) = host
                .get_mut(TRANSCRIPT_RICH_BODY_ID)
                .and_then(|c| c.component_mut::<RichContent>())
            {
                let expected = rich.version();
                if rich.update(&expected, next_rev, doc).is_ok() {
                    self.rich_doc_revision = next_rev;
                    self.last_rich_source = latest_rich_md;
                }
            }
        }
        if let Some(disc) = host
            .get_mut(TRANSCRIPT_RICH_DISC_ID)
            .and_then(|c| c.component_mut::<DisclosureContent>())
        {
            disc.set_expanded(self.rich_expanded);
        }

        self.sync_actionables(host, actionables);
    }

    fn sync_actionables(&mut self, host: &mut ContentHost, actionables: &[ActionablePrompt]) {
        let mut sig = String::new();
        for a in actionables {
            sig.push_str(a.id());
            sig.push(':');
            sig.push_str(&a.revision().to_string());
            sig.push(';');
        }
        if sig == self.rendered_actionables_sig {
            return;
        }
        self.rendered_actionables_sig = sig;
        self.choice_bindings.clear();

        let mut next_id = TRANSCRIPT_ACTIONABLE_BASE;
        let mut alloc_id = || {
            let id = ComponentId(next_id);
            next_id += 1;
            id
        };

        let mut cards = Vec::new();
        for prompt in actionables {
            let title_id = alloc_id();
            let detail_id = alloc_id();
            let buttons_row_id = alloc_id();
            let card_id = alloc_id();

            let mut buttons = Vec::new();
            for (choice_index, choice) in prompt.choices.iter().enumerate() {
                let btn_id = alloc_id();
                self.choice_bindings.insert(
                    btn_id,
                    ActionableChoiceBinding {
                        scope: prompt.scope().clone(),
                        actionable_id: prompt.id().to_owned(),
                        revision: prompt.revision(),
                        choice_index,
                    },
                );
                let mut btn = Button::new(choice.label.clone());
                if choice.id == "allow" || choice.id == "option-0" {
                    btn.variant = ButtonVariant::Primary;
                }
                buttons.push(Content::new(btn_id, btn));
            }

            let detail_str = if prompt.detail.trim().is_empty() {
                format!("待确认请求 #{}", prompt.id())
            } else {
                prompt.detail.clone()
            };

            cards.push(Content::new(
                card_id,
                StackContent {
                    model: Stack {
                        axis: Axis::Vertical,
                        gap: Spacing::Micro.value(),
                    },
                    flexible_child: None,
                    children: vec![
                        Content::new(
                            title_id,
                            TextContent::new(format!("[待确认] {}", prompt.title)),
                        ),
                        Content::new(detail_id, TextContent::new(detail_str)),
                        Content::new(
                            buttons_row_id,
                            StackContent {
                                model: Stack {
                                    axis: Axis::Horizontal,
                                    gap: Spacing::Tight.value(),
                                },
                                flexible_child: None,
                                children: buttons,
                            },
                        ),
                    ],
                },
            ));
        }

        replace_stack_children(host, TRANSCRIPT_ACTIONABLES_STACK_ID, cards);
    }

    pub fn handle_actions(
        &mut self,
        host: &mut ContentHost,
        snapshot: &RunSnapshot,
        records: &[ContentActionRecord],
    ) -> Vec<TranscriptIntent> {
        let mut out = Vec::new();
        for record in records {
            if let Some(binding) = self.choice_bindings.get(&record.action.target)
                && matches!(record.action.kind, ContentActionKind::Activate)
            {
                out.push(TranscriptIntent::RespondActionable {
                    scope: binding.scope.clone(),
                    actionable_id: binding.actionable_id.clone(),
                    revision: binding.revision,
                    choice_index: binding.choice_index,
                });
                continue;
            }
            if let Some(ContentActionPayload::DisclosureExpansion(intent)) = &record.payload {
                if let Some(disc) = host
                    .get_mut(record.action.target)
                    .and_then(|c| c.component_mut::<DisclosureContent>())
                {
                    let _ = disc.confirm(intent, true);
                }
                match record.action.target {
                    id if id == TRANSCRIPT_THINKING_DISC_ID => {
                        self.thinking_expanded = intent.expanded();
                    }
                    id if id == TRANSCRIPT_TOOL_DISC_ID => {
                        self.tool_expanded = intent.expanded();
                    }
                    id if id == TRANSCRIPT_RICH_DISC_ID => {
                        self.rich_expanded = intent.expanded();
                    }
                    _ => {}
                }
                continue;
            }
            if let Some(ContentActionPayload::Search(search)) = &record.payload
                && record.action.target == TRANSCRIPT_SEARCH_ID
            {
                self.search_query = match search.kind {
                    SearchCommandKind::Submit => search.query.clone(),
                    SearchCommandKind::Clear => String::new(),
                };
                continue;
            }
            match record.action.target {
                id if id == TRANSCRIPT_EARLIER_BTN_ID
                    && matches!(record.action.kind, ContentActionKind::Activate) =>
                {
                    if let Some(tekes_agent_presentation::ComposerDestination::Run { scope }) =
                        snapshot.selection.as_ref().map(|s| &s.destination)
                    {
                        out.push(TranscriptIntent::LoadEarlier(scope.clone()));
                    }
                }
                id if id == TRANSCRIPT_LATEST_BTN_ID
                    && matches!(record.action.kind, ContentActionKind::Activate) =>
                {
                    if let Some(reader) = host
                        .get_mut(TRANSCRIPT_READER_ID)
                        .and_then(|c| c.component_mut::<TranscriptReader>())
                    {
                        reader.follow_latest();
                    }
                }
                _ => {}
            }
        }

        if let Some(rich) = host
            .get_mut(TRANSCRIPT_RICH_BODY_ID)
            .and_then(|c| c.component_mut::<RichContent>())
        {
            for link in rich.drain_link_actions() {
                out.push(TranscriptIntent::OpenLink(link.target));
            }
        }
        out
    }

    pub fn scroll_offset(host: &ContentHost) -> Option<f32> {
        host.get(TRANSCRIPT_READER_ID)
            .and_then(|c| c.component::<TranscriptReader>())
            .map(|r| r.surface.scroll().offset.y)
    }

    pub fn is_following_latest(host: &ContentHost) -> Option<bool> {
        host.get(TRANSCRIPT_READER_ID)
            .and_then(|c| c.component::<TranscriptReader>())
            .map(|r| r.reading.is_following_latest())
    }
}

fn tool_wording<'a>(label: &'a str) -> TranscriptToolRowWording<'a> {
    TranscriptToolRowWording {
        label,
        stopped: "已停止",
        title_hint: None,
        detail: None,
        progress: None,
        result: None,
        settled_action: None,
    }
}

fn report_wording() -> TranscriptToolReportWording {
    TranscriptToolReportWording {
        subject_phrase: |verb, subject| format!("{verb} {subject}"),
        verb: |v| v.to_owned(),
        magnitude: |count, obj| format!("{count} {obj}"),
        join: |a, b| format!("{a} · {b}"),
    }
}

pub(crate) fn format_transcript_body(snapshot: &RunSnapshot) -> String {
    format_transcript(snapshot, "").0
}

fn format_transcript(
    snapshot: &RunSnapshot,
    search_query: &str,
) -> (String, String, String, String) {
    let query = search_query.trim().to_lowercase();
    let matches_query = |text: &str| query.is_empty() || text.to_lowercase().contains(&query);

    let Ok(doc) = TranscriptDocument::read(&snapshot.transcript) else {
        let fallback = snapshot
            .transcript
            .items
            .iter()
            .filter(|i| i.is_visible && matches_query(&i.text))
            .map(|i| {
                let role = match i.kind {
                    TranscriptItemKind::User => "你",
                    TranscriptItemKind::Assistant => "Agent",
                    TranscriptItemKind::Error => "错误",
                    _ => "系统",
                };
                format!("{role}\n{}", i.text)
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        return (
            fallback,
            String::new(),
            String::new(),
            "无结构化消息".to_owned(),
        );
    };

    let mut lines = Vec::new();
    let mut latest_thinking = String::new();
    let mut latest_tool_lines = Vec::new();
    let mut latest_markdown = String::new();

    for notice in &doc.session_notices {
        if matches_query(&notice.item.text) {
            lines.push(format!("[通知] {}", notice.item.text));
        }
    }

    for section in &doc.sections {
        for input in &section.inputs {
            if matches_query(&input.item.text) {
                lines.push(format!("你 · #{}\n{}", section.id, input.item.text));
            }
        }
        for tool in &section.tools {
            let header = tool.presentation_header(
                tool_wording(&tool.name),
                &TranscriptToolRowInputs::default(),
                report_wording(),
            );
            latest_tool_lines.push(header.text.clone());
            if matches_query(&header.text) {
                lines.push(format!("[工具] {}", header.text));
            }
        }
        for output in &section.outputs {
            if !output.item.reasoning.trim().is_empty() {
                latest_thinking = output.item.reasoning.clone();
            }
            if !output.item.text.trim().is_empty() {
                if output.item.kind == TranscriptItemKind::Assistant {
                    latest_markdown = output.item.text.clone();
                }
                if matches_query(&output.item.text) {
                    let prefix = match output.item.kind {
                        TranscriptItemKind::Assistant => "Agent",
                        TranscriptItemKind::Error => "错误",
                        _ => "系统",
                    };
                    lines.push(format!("{prefix}\n{}", output.item.text));
                }
            }
        }
        if let Some(turn) = section.output_footnote()
            && (turn.usage.input_tokens > 0 || turn.usage.output_tokens > 0)
        {
            lines.push(format!(
                "[轮次 {} · {} · tokens: in={} / out={}]",
                turn.id, turn.status, turn.usage.input_tokens, turn.usage.output_tokens
            ));
        }
    }

    let doc_text = if lines.is_empty() {
        if query.is_empty() {
            "暂无消息记录，请在下方输入框发起对话。".to_owned()
        } else {
            format!("未找到匹配“{search_query}”的消息。")
        }
    } else {
        lines.join("\n\n")
    };

    let rich_md = if latest_markdown.is_empty() {
        "暂无最新助手 Markdown 输出。".to_owned()
    } else {
        latest_markdown
    };

    let tools_summary = latest_tool_lines
        .into_iter()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");

    (doc_text, latest_thinking, tools_summary, rich_md)
}
