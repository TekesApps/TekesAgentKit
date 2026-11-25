//! Structured Tekes-aligned Transcript surface over `RunSnapshot`, `TranscriptDocument`,
//! `ScrollContent` + `ContentReading`, centered `770 pt` conversation column, borderless tonal
//! user bubbles, quiet inline activity disclosures, per-turn `RichContent`, subtle footnote
//! icon actions, and rounded `PanelContent` approval cards.

use crate::ResponsiveWidthMode;
use crate::ids::*;
use crate::ui_helpers::{
    CONVERSATION_COLUMN_MAX_WIDTH, CenteredColumn, UserBubble, card_panel, icon_button, label_text,
    secondary_icon_row, secondary_text,
};
use std::collections::{BTreeMap, BTreeSet};
use tekes_agent_presentation::{
    ActionablePrompt, ConnectionPhase, RunScope, RunSnapshot, TranscriptDocument,
    TranscriptItemKind, TranscriptToolReportWording, TranscriptToolRowInputs,
    TranscriptToolRowWording,
};
use tekes_ui_controls::{
    ButtonFramePolicy, ButtonTextRenderer, ButtonVariant, ReadingAnchorUpdate, ReadingController,
    ReadingDocumentVersion, ScrollArea, TextRangeMapping, TextSurface, TextSurfacePresentation,
    composition::{
        AccessibilityAction, Button, Component, ComponentId, Content, ContentActionKind,
        ContentActionPayload, ContentActionRecord, ContentEvent, ContentHost, ContentResponse,
        ContentRole, ContentSemantics, ContentTheme, CurveSubtreeCapability, RichContent,
        RichDocument, ScrollContent, SearchCommandKind, SearchFieldContent, StackContent,
        TextContent,
    },
    layout::{Insets, MeasuredChild},
};
use tekes_ui_scene::{Rect, Scene, Size};
use tekes_ui_tokens::{Spacing, proposal, proposal::layout::MainAxisDistribution};

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
    Reconnect,
    CopyLatest,
    FeedbackLatest {
        positive: bool,
    },
    ForkLatest,
}

/// Borderless scrollable reading surface driven by `ReadingController` so prepend pagination
/// preserves scroll anchors cleanly across AppHost renders and slot swaps (used by `ChildRun`).
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
        surface.set_presentation(TextSurfacePresentation::Content);
        surface.set_role(proposal::TextRole::UiBody);
        let mut reading = ReadingController::default();
        reading.follow_latest(&mut surface);
        Self {
            surface,
            reading,
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
        if self.doc_text.trim().is_empty() {
            MeasuredChild::new(Size::new(available.width, 0.0))
        } else {
            MeasuredChild::new(Size::new(available.width, available.height.max(120.0)))
        }
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
                .draw(host, scene, theme.palette(), self.focused, false);
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
        ContentSemantics::new(ContentRole::Text, "Conversation transcript")
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct TurnToolModel {
    icon_name: String,
    header: String,
    running: bool,
    detail_sections: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TurnSignature {
    dest_key: String,
    user_text: String,
    reasoning_parts: Vec<String>,
    tools: Vec<TurnToolModel>,
    assistant_markdown: String,
    error_text: Option<String>,
    is_latest_settled: bool,
    expanded: bool,
}

#[derive(Clone, Debug)]
struct TurnRenderModel {
    turn_key: String,
    user_text: String,
    reasoning_parts: Vec<String>,
    tools: Vec<TurnToolModel>,
    assistant_markdown: String,
    error_text: Option<String>,
    is_latest_settled: bool,
}

impl TurnRenderModel {
    fn signature(&self, dest_key: &str, expanded: bool) -> TurnSignature {
        TurnSignature {
            dest_key: dest_key.to_owned(),
            user_text: self.user_text.clone(),
            reasoning_parts: self.reasoning_parts.clone(),
            tools: self.tools.clone(),
            assistant_markdown: self.assistant_markdown.clone(),
            error_text: self.error_text.clone(),
            is_latest_settled: self.is_latest_settled,
            expanded,
        }
    }

    fn activity_title(&self) -> String {
        let has_reason = !self.reasoning_parts.is_empty();
        let tool_count = self.tools.len();
        let any_running = self.tools.iter().any(|t| t.running);
        if has_reason && tool_count > 0 {
            let prefix = if any_running {
                "Thinking"
            } else {
                "Thought process"
            };
            format!(
                "{prefix} · {tool_count} tool{}",
                if tool_count == 1 { "" } else { "s" }
            )
        } else if tool_count > 0 {
            let verb = if any_running { "Working · " } else { "Used " };
            format!(
                "{verb}{tool_count} tool{}",
                if tool_count == 1 { "" } else { "s" }
            )
        } else if has_reason {
            "Thought process".to_owned()
        } else {
            String::new()
        }
    }
}

#[derive(Debug)]
pub struct TranscriptSurface {
    selected_key: String,
    last_dest_key: String,
    next_turn_slot: u64,
    next_tool_id: u64,
    turn_key_to_slot: BTreeMap<String, u64>,
    doc_revisions: BTreeMap<String, u64>,
    seen_doc_keys: BTreeSet<String>,
    doc_version: Option<ReadingDocumentVersion>,
    mounted_turn_sigs: BTreeMap<String, TurnSignature>,
    mounted_turn_order: Vec<String>,
    turn_toggle_ids: BTreeMap<String, ComponentId>,
    turn_rich_ids: BTreeMap<String, ComponentId>,
    mounted_rich_ids: Vec<ComponentId>,
    activity_toggle_targets: BTreeMap<ComponentId, String>,
    expanded_activity_turns: BTreeSet<String>,
    search_query: String,
    search_open: bool,
    rich_doc_revision: u64,
    last_notice_sig: String,
    last_has_earlier: bool,
    last_following: bool,
    session_error: Option<String>,
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
            last_dest_key: String::new(),
            next_turn_slot: 0,
            next_tool_id: TRANSCRIPT_TOOL_ROW_BASE,
            turn_key_to_slot: BTreeMap::new(),
            doc_revisions: BTreeMap::new(),
            seen_doc_keys: BTreeSet::new(),
            doc_version: None,
            mounted_turn_sigs: BTreeMap::new(),
            mounted_turn_order: Vec::new(),
            turn_toggle_ids: BTreeMap::new(),
            turn_rich_ids: BTreeMap::new(),
            mounted_rich_ids: Vec::new(),
            activity_toggle_targets: BTreeMap::new(),
            expanded_activity_turns: BTreeSet::new(),
            search_query: String::new(),
            search_open: false,
            rich_doc_revision: 1,
            last_notice_sig: String::new(),
            last_has_earlier: false,
            last_following: true,
            session_error: None,
            choice_bindings: BTreeMap::new(),
            rendered_actionables_sig: String::new(),
        }
    }

    pub fn set_session_error(&mut self, error: Option<String>) {
        self.session_error = error;
    }

    pub fn build_content() -> Content {
        let search_row = Content::new(
            TRANSCRIPT_SEARCH_ROW_ID,
            StackContent::horizontal(Spacing::Tight.value(), Vec::new()),
        );

        let header = Content::new(
            TRANSCRIPT_HEADER_ID,
            StackContent::vertical(Spacing::Tight.value(), Vec::new()),
        );

        let turns_stack = Content::new(
            TRANSCRIPT_TURNS_STACK_ID,
            StackContent::vertical(24.0, Vec::new()),
        );

        let actionables_stack = Content::new(
            TRANSCRIPT_ACTIONABLES_STACK_ID,
            StackContent::vertical(Spacing::Regular.value(), Vec::new()),
        );

        let scroll_inner = Content::new(
            TRANSCRIPT_SCROLL_INNER_ID,
            StackContent::vertical(18.0, vec![header, turns_stack, actionables_stack]),
        );

        let centered_column = CenteredColumn::content(
            TRANSCRIPT_SCROLL_COLUMN_ID,
            CONVERSATION_COLUMN_MAX_WIDTH,
            0.0,
            scroll_inner,
        );

        let scroll_content = Content::new(
            TRANSCRIPT_SCROLL_ID,
            ScrollContent::new(ScrollArea::default(), centered_column)
                .scrollbar(true)
                .content_insets(Insets::new(16.0, 20.0, 16.0, 20.0)),
        );

        let latest_slot = Content::new(
            TRANSCRIPT_LATEST_SLOT_ID,
            StackContent::horizontal(0.0, Vec::new()),
        );

        Content::new(
            TRANSCRIPT_ROOT_ID,
            StackContent::vertical(
                Spacing::Tight.value(),
                vec![search_row, scroll_content, latest_slot],
            )
            .with_flexible_child(TRANSCRIPT_SCROLL_ID),
        )
    }

    pub fn set_search_open(&mut self, host: &mut ContentHost, open: bool) {
        self.search_open = open;
        let mounted = host.get(TRANSCRIPT_SEARCH_ID).is_some();
        if open && !mounted {
            let mut search = SearchFieldContent::new("Search messages", &self.search_query);
            search.set_placeholder(Some("Find in conversation…".to_owned()));
            if let Some(row) = host
                .get_mut(TRANSCRIPT_SEARCH_ROW_ID)
                .and_then(|c| c.component_mut::<StackContent>())
            {
                row.set_flexible_child(Some(TRANSCRIPT_SEARCH_ID));
            }
            replace_stack_children(
                host,
                TRANSCRIPT_SEARCH_ROW_ID,
                vec![Content::new(TRANSCRIPT_SEARCH_ID, search)],
            );
            let _ = host.focus(Some(TRANSCRIPT_SEARCH_ID), true);
        } else if !open && mounted {
            if let Some(row) = host
                .get_mut(TRANSCRIPT_SEARCH_ROW_ID)
                .and_then(|c| c.component_mut::<StackContent>())
            {
                row.set_flexible_child(None);
            }
            replace_stack_children(host, TRANSCRIPT_SEARCH_ROW_ID, Vec::new());
        }
    }

    fn slot_for_turn(&mut self, turn_key: &str) -> u64 {
        if let Some(&slot) = self.turn_key_to_slot.get(turn_key) {
            return slot;
        }
        let slot = self.next_turn_slot;
        self.next_turn_slot = self.next_turn_slot.saturating_add(1);
        self.turn_key_to_slot.insert(turn_key.to_owned(), slot);
        slot
    }

    fn next_doc_revision(&mut self, reading_key: &str) -> u64 {
        let entry = self
            .doc_revisions
            .entry(reading_key.to_owned())
            .or_insert(0);
        *entry = entry.saturating_add(1).max(1);
        *entry
    }

    fn release_turn_metadata(&mut self, turn_key: &str) {
        if let Some(toggle_id) = self.turn_toggle_ids.remove(turn_key) {
            self.activity_toggle_targets.remove(&toggle_id);
        }
        self.turn_rich_ids.remove(turn_key);
    }

    fn build_turn_content(
        &mut self,
        dest_key: &str,
        turn: &TurnRenderModel,
        expanded: bool,
    ) -> Content {
        let slot = self.slot_for_turn(&turn.turn_key);
        let base = TRANSCRIPT_TURN_BASE + slot * 32;
        let root_id = ComponentId(base);
        let mut children = Vec::new();

        if !turn.user_text.is_empty() {
            let bubble = UserBubble::content(
                ComponentId(base + 2),
                Content::new(
                    ComponentId(base + 3),
                    TextContent::new(turn.user_text.clone()),
                ),
            );
            let row = StackContent::horizontal(0.0, vec![bubble])
                .into_content_with_distribution(ComponentId(base + 1), MainAxisDistribution::End);
            children.push(row);
        }

        let activity_title = turn.activity_title();
        if !activity_title.is_empty() {
            let toggle_id = ComponentId(base + 5);
            self.turn_toggle_ids
                .insert(turn.turn_key.clone(), toggle_id);
            self.activity_toggle_targets
                .insert(toggle_id, turn.turn_key.clone());
            let chevron = if expanded { "⌄" } else { "›" };
            let title_label = format!("{activity_title}  {chevron}");
            let toggle_inner = StackContent::horizontal(
                0.0,
                vec![Content::new(
                    ComponentId(base + 6),
                    secondary_text(title_label),
                )],
            )
            .into_content_with_padding(ComponentId(base + 10), Insets::new(3.0, 6.0, 3.0, 0.0));
            let mut toggle_btn = Button::new(toggle_inner)
                .with_accessible_name(activity_title)
                .with_frame_policy(ButtonFramePolicy::Exact);
            toggle_btn.set_variant(ButtonVariant::Quiet);
            toggle_btn.set_transparent_chrome(true);
            let toggle_row = Content::new(
                ComponentId(base + 4),
                StackContent::horizontal(0.0, vec![Content::new(toggle_id, toggle_btn)]),
            );
            children.push(toggle_row);

            if expanded {
                let mut detail_children = Vec::new();
                if !turn.reasoning_parts.is_empty() {
                    detail_children.push(Content::new(
                        ComponentId(base + 8),
                        secondary_text(turn.reasoning_parts.join("\n\n")),
                    ));
                }
                for tool in &turn.tools {
                    let row_id = ComponentId(self.next_tool_id);
                    let icon_id = ComponentId(self.next_tool_id + 1);
                    let text_id = ComponentId(self.next_tool_id + 2);
                    self.next_tool_id = self.next_tool_id.saturating_add(4);
                    detail_children.push(secondary_icon_row(
                        row_id,
                        icon_id,
                        text_id,
                        &tool.icon_name,
                        tool.header.clone(),
                    ));
                    if !tool.detail_sections.is_empty() {
                        let mut sec_rows = Vec::new();
                        for (title, body) in &tool.detail_sections {
                            let sec_id = ComponentId(self.next_tool_id);
                            self.next_tool_id = self.next_tool_id.saturating_add(2);
                            sec_rows.push(Content::new(
                                sec_id,
                                secondary_text(format!("{title}: {body}")),
                            ));
                        }
                        let sec_stack_id = ComponentId(self.next_tool_id);
                        self.next_tool_id = self.next_tool_id.saturating_add(2);
                        detail_children.push(
                            StackContent::vertical(4.0, sec_rows).into_content_with_padding(
                                sec_stack_id,
                                Insets::new(0.0, 0.0, 4.0, 20.0),
                            ),
                        );
                    }
                }
                let detail_stack = StackContent::vertical(6.0, detail_children)
                    .into_content_with_padding(
                        ComponentId(base + 7),
                        Insets::new(2.0, 0.0, 6.0, 12.0),
                    );
                children.push(detail_stack);
            }
        }

        if !turn.assistant_markdown.is_empty() {
            self.rich_doc_revision = self.rich_doc_revision.saturating_add(1).max(1);
            let rich_id = ComponentId(base + 16);
            let doc = RichDocument::from_markdown(&turn.assistant_markdown);
            let rich = RichContent::new(
                format!("turn-rich-{dest_key}-{}", turn.turn_key),
                self.rich_doc_revision,
                doc,
            )
            .expect("valid turn RichContent");
            self.turn_rich_ids.insert(turn.turn_key.clone(), rich_id);
            children.push(Content::new(rich_id, rich));
        }

        if let Some(err) = &turn.error_text {
            children.push(Content::new(
                ComponentId(base + 17),
                secondary_text(err.clone()),
            ));
        }

        if turn.is_latest_settled {
            let copy_btn = icon_button(TRANSCRIPT_COPY_ICON_ID, "tekes-copy", "Copy");
            let up_btn = icon_button(TRANSCRIPT_UP_ICON_ID, "tekes-thumbs-up", "Good response");
            let down_btn =
                icon_button(TRANSCRIPT_DOWN_ICON_ID, "tekes-thumbs-down", "Bad response");
            let fork_btn = icon_button(TRANSCRIPT_FORK_ICON_ID, "tekes-fork", "Fork thread");
            let footnote_row = Content::new(
                TRANSCRIPT_FOOTNOTE_ROW_ID,
                StackContent::horizontal(
                    Spacing::Tight.value(),
                    vec![
                        Content::new(TRANSCRIPT_COPY_BTN_ID, copy_btn),
                        Content::new(TRANSCRIPT_UP_BTN_ID, up_btn),
                        Content::new(TRANSCRIPT_DOWN_BTN_ID, down_btn),
                        Content::new(TRANSCRIPT_FORK_BTN_ID, fork_btn),
                    ],
                ),
            );
            children.push(footnote_row);
        }

        Content::new(root_id, StackContent::vertical(10.0, children))
    }

    fn sync_header_slot(&mut self, host: &mut ContentHost, snapshot: &RunSnapshot) {
        let notice_text = match snapshot.connection.phase {
            ConnectionPhase::Failed => Some("Connection lost. Messages could not be synced."),
            ConnectionPhase::Degraded => Some("Connection degraded (read-only)."),
            ConnectionPhase::Reconnecting => Some("Reconnecting to runtime…"),
            ConnectionPhase::Stopped => Some("Session connection stopped."),
            ConnectionPhase::Connecting | ConnectionPhase::Connected => None,
        };
        let has_earlier = snapshot.transcript.has_more_before;
        let sig = format!("{notice_text:?}:{:?}:{has_earlier}", self.session_error);
        if sig == self.last_notice_sig {
            return;
        }
        self.last_notice_sig = sig;
        self.last_has_earlier = has_earlier;

        let mut children = Vec::new();
        if let Some(msg) = notice_text {
            let mut reconnect_btn = Button::new("Reconnect");
            reconnect_btn.set_variant(ButtonVariant::Panel);
            let row = StackContent::horizontal(
                Spacing::Tight.value(),
                vec![
                    Content::new(TRANSCRIPT_NOTICE_TEXT_ID, secondary_text(msg)),
                    Content::new(TRANSCRIPT_RECONNECT_BTN_ID, reconnect_btn),
                ],
            )
            .with_flexible_child(TRANSCRIPT_NOTICE_TEXT_ID)
            .into_content_with_distribution(
                TRANSCRIPT_NOTICE_SLOT_ID,
                MainAxisDistribution::SpaceBetween,
            );
            children.push(card_panel(
                TRANSCRIPT_NOTICE_PANEL_ID,
                row,
                Insets::new(8.0, 12.0, 8.0, 12.0),
            ));
        }

        if let Some(err) = &self.session_error {
            children.push(card_panel(
                TRANSCRIPT_ERROR_PANEL_ID,
                Content::new(TRANSCRIPT_STATUS_ID, secondary_text(err.clone())),
                Insets::new(8.0, 12.0, 8.0, 12.0),
            ));
        }

        if has_earlier {
            let mut earlier_btn = Button::new("↑ Load earlier messages");
            earlier_btn.set_variant(ButtonVariant::Quiet);
            earlier_btn.set_transparent_chrome(true);
            let row = StackContent::horizontal(
                0.0,
                vec![Content::new(TRANSCRIPT_EARLIER_BTN_ID, earlier_btn)],
            )
            .into_content_with_distribution(
                TRANSCRIPT_EARLIER_ROW_ID,
                MainAxisDistribution::Center,
            );
            children.push(row);
        }

        replace_stack_children(host, TRANSCRIPT_HEADER_ID, children);
    }

    fn sync_jump_pill(&mut self, host: &mut ContentHost) {
        let following = Self::is_following_latest(host).unwrap_or(true);
        if following == self.last_following {
            return;
        }
        self.last_following = following;
        if following {
            replace_stack_children(host, TRANSCRIPT_LATEST_SLOT_ID, Vec::new());
        } else {
            let btn = icon_button(
                TRANSCRIPT_LATEST_ICON_ID,
                "tekes-jump-bottom",
                "Jump to latest",
            );
            let row =
                StackContent::horizontal(0.0, vec![Content::new(TRANSCRIPT_LATEST_BTN_ID, btn)])
                    .into_content_with_distribution(ComponentId(139), MainAxisDistribution::End);
            let aligned =
                CenteredColumn::content(ComponentId(148), CONVERSATION_COLUMN_MAX_WIDTH, 20.0, row);
            replace_stack_children(host, TRANSCRIPT_LATEST_SLOT_ID, vec![aligned]);
        }
    }

    fn sync_turns(&mut self, host: &mut ContentHost, snapshot: &RunSnapshot, dest_key: &str) {
        let turns = collect_turn_models(snapshot, &self.search_query);
        let reading_key = format!("transcript:{dest_key}:q:{}", self.search_query);

        let mut desired_keys = Vec::with_capacity(turns.len());
        let mut desired_sigs = BTreeMap::new();
        let mut turn_ids = Vec::with_capacity(turns.len());
        let mut expanded_flags = Vec::with_capacity(turns.len());

        for turn in &turns {
            let slot = self.slot_for_turn(&turn.turn_key);
            let expanded = self.expanded_activity_turns.contains(&turn.turn_key);
            let sig = turn.signature(dest_key, expanded);
            desired_keys.push(turn.turn_key.clone());
            desired_sigs.insert(turn.turn_key.clone(), sig);
            turn_ids.push(ComponentId(TRANSCRIPT_TURN_BASE + slot * 32));
            expanded_flags.push(expanded);
        }

        let switched_doc = self.selected_key != reading_key || self.doc_version.is_none();
        if switched_doc {
            let stale_keys: Vec<String> = self.mounted_turn_sigs.keys().cloned().collect();
            for key in stale_keys {
                self.release_turn_metadata(&key);
            }
            self.mounted_turn_sigs.clear();
            self.mounted_turn_order.clear();

            let mut built_turns = Vec::with_capacity(turns.len());
            for (turn, &expanded) in turns.iter().zip(&expanded_flags) {
                built_turns.push(self.build_turn_content(dest_key, turn, expanded));
            }
            let rev = self.next_doc_revision(&reading_key);
            if let Ok((ver, ())) = host.set_scroll_reading_document(
                TRANSCRIPT_SCROLL_ID,
                reading_key.clone(),
                rev,
                turn_ids,
                move |h| {
                    replace_stack_children(h, TRANSCRIPT_TURNS_STACK_ID, built_turns);
                },
            ) {
                self.doc_version = Some(ver);
                self.selected_key = reading_key.clone();
                self.last_dest_key = dest_key.to_owned();
                self.mounted_turn_sigs = desired_sigs;
                self.mounted_turn_order = desired_keys;
                self.mounted_rich_ids = self
                    .mounted_turn_order
                    .iter()
                    .filter_map(|k| self.turn_rich_ids.get(k).copied())
                    .collect();
                if self.seen_doc_keys.insert(reading_key) {
                    Self::follow_latest(host);
                }
            }
            return;
        }

        let desired_set: BTreeSet<&str> = desired_keys.iter().map(String::as_str).collect();
        let removed_keys: Vec<String> = self
            .mounted_turn_order
            .iter()
            .filter(|k| !desired_set.contains(k.as_str()))
            .cloned()
            .collect();

        let mut changed_indices = Vec::new();
        for (idx, key) in desired_keys.iter().enumerate() {
            let same = self
                .mounted_turn_sigs
                .get(key)
                .is_some_and(|prev| prev == &desired_sigs[key]);
            if !same {
                changed_indices.push(idx);
            }
        }
        let order_changed = self.mounted_turn_order != desired_keys;

        if removed_keys.is_empty() && changed_indices.is_empty() && !order_changed {
            return;
        }

        let mut remove_ids = Vec::new();
        for key in &removed_keys {
            self.release_turn_metadata(key);
            if let Some(&slot) = self.turn_key_to_slot.get(key) {
                remove_ids.push(ComponentId(TRANSCRIPT_TURN_BASE + slot * 32));
            }
        }
        let mut insert_contents = Vec::with_capacity(changed_indices.len());
        for &idx in &changed_indices {
            let turn = &turns[idx];
            if self.mounted_turn_sigs.contains_key(&turn.turn_key) {
                self.release_turn_metadata(&turn.turn_key);
                let slot = self.slot_for_turn(&turn.turn_key);
                remove_ids.push(ComponentId(TRANSCRIPT_TURN_BASE + slot * 32));
            }
            insert_contents.push(self.build_turn_content(dest_key, turn, expanded_flags[idx]));
        }

        let Some(expected) = self.doc_version.clone() else {
            return;
        };
        let rev = self.next_doc_revision(&reading_key);
        let reorder_ids = turn_ids.clone();
        if let Ok((ver, ())) = host.update_scroll_reading_document(
            TRANSCRIPT_SCROLL_ID,
            &expected,
            rev,
            turn_ids,
            move |h| {
                for id in remove_ids {
                    let _ = h.remove(id);
                }
                for content in insert_contents {
                    let _ = h.insert(TRANSCRIPT_TURNS_STACK_ID, content);
                }
                let _ = h.reorder(TRANSCRIPT_TURNS_STACK_ID, &reorder_ids);
            },
        ) {
            self.doc_version = Some(ver);
            self.mounted_turn_sigs = desired_sigs;
            self.mounted_turn_order = desired_keys;
            self.mounted_rich_ids = self
                .mounted_turn_order
                .iter()
                .filter_map(|k| self.turn_rich_ids.get(k).copied())
                .collect();
        }
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

        if let Some(search) = host
            .get(TRANSCRIPT_SEARCH_ID)
            .and_then(|c| c.component::<SearchFieldContent>())
        {
            self.search_query = search.query().to_owned();
        }

        self.sync_header_slot(host, snapshot);
        self.sync_turns(host, snapshot, &dest_key);
        self.sync_jump_pill(host);
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
            let stack_id = alloc_id();
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
                btn.set_variant(if choice.id == "allow" || choice.id == "option-0" {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Panel
                });
                buttons.push(Content::new(btn_id, btn));
            }

            let mut card_children = vec![Content::new(title_id, label_text(&prompt.title))];
            if !prompt.detail.trim().is_empty() {
                card_children.push(Content::new(detail_id, secondary_text(&prompt.detail)));
            }
            card_children.push(Content::new(
                buttons_row_id,
                StackContent::horizontal(Spacing::Tight.value(), buttons),
            ));

            let body = Content::new(
                stack_id,
                StackContent::vertical(Spacing::Tight.value(), card_children),
            );
            cards.push(card_panel(
                card_id,
                body,
                Insets::new(12.0, 14.0, 12.0, 14.0),
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
        let mut toggled_turn = false;

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
            if let Some(ContentActionPayload::Search(search)) = &record.payload
                && record.action.target == TRANSCRIPT_SEARCH_ID
            {
                self.search_query = match search.kind {
                    SearchCommandKind::Submit => search.query.clone(),
                    SearchCommandKind::Clear => String::new(),
                };
                continue;
            }
            if matches!(record.action.kind, ContentActionKind::Activate) {
                if let Some(turn_key) = self
                    .activity_toggle_targets
                    .get(&record.action.target)
                    .cloned()
                {
                    if !self.expanded_activity_turns.remove(&turn_key) {
                        self.expanded_activity_turns.insert(turn_key);
                    }
                    toggled_turn = true;
                    continue;
                }
                match record.action.target {
                    id if id == TRANSCRIPT_EARLIER_BTN_ID => {
                        if let Some(tekes_agent_presentation::ComposerDestination::Run { scope }) =
                            snapshot.selection.as_ref().map(|s| &s.destination)
                        {
                            out.push(TranscriptIntent::LoadEarlier(scope.clone()));
                        }
                    }
                    id if id == TRANSCRIPT_LATEST_BTN_ID => {
                        Self::follow_latest(host);
                        self.sync_jump_pill(host);
                    }
                    id if id == TRANSCRIPT_RECONNECT_BTN_ID => {
                        out.push(TranscriptIntent::Reconnect)
                    }
                    id if id == TRANSCRIPT_COPY_BTN_ID => out.push(TranscriptIntent::CopyLatest),
                    id if id == TRANSCRIPT_UP_BTN_ID => {
                        out.push(TranscriptIntent::FeedbackLatest { positive: true });
                    }
                    id if id == TRANSCRIPT_DOWN_BTN_ID => {
                        out.push(TranscriptIntent::FeedbackLatest { positive: false });
                    }
                    id if id == TRANSCRIPT_FORK_BTN_ID => out.push(TranscriptIntent::ForkLatest),
                    _ => {}
                }
            }
        }

        if toggled_turn {
            let dest_key = snapshot
                .selection
                .as_ref()
                .map(|s| s.destination.storage_key())
                .unwrap_or_else(|| self.last_dest_key.clone());
            self.sync_turns(host, snapshot, &dest_key);
        }

        out
    }

    pub fn drain_link_actions(&self, host: &mut ContentHost) -> Vec<TranscriptIntent> {
        let mut out = Vec::new();
        for &rich_id in &self.mounted_rich_ids {
            let has_links = host
                .get(rich_id)
                .and_then(|c| c.component::<RichContent>())
                .is_some_and(|rich| !rich.document().links().is_empty());
            if !has_links {
                continue;
            }
            if let Some(rich) = host
                .get_mut(rich_id)
                .and_then(|c| c.component_mut::<RichContent>())
            {
                for link in rich.drain_link_actions() {
                    out.push(TranscriptIntent::OpenLink(link.target));
                }
            }
        }
        out
    }

    pub fn follow_latest(host: &mut ContentHost) {
        if let Some(scroll) = host
            .get_mut(TRANSCRIPT_SCROLL_ID)
            .and_then(|c| c.component_mut::<ScrollContent>())
        {
            scroll.follow_latest();
        }
    }

    pub fn stop_following(host: &mut ContentHost) {
        if let Some(scroll) = host
            .get_mut(TRANSCRIPT_SCROLL_ID)
            .and_then(|c| c.component_mut::<ScrollContent>())
        {
            scroll.stop_following();
        }
    }

    pub fn scroll_offset(host: &ContentHost) -> Option<f32> {
        host.get(TRANSCRIPT_SCROLL_ID)
            .and_then(|c| c.component::<ScrollContent>())
            .map(|s| s.offset().y)
    }

    pub fn is_following_latest(host: &ContentHost) -> Option<bool> {
        host.get(TRANSCRIPT_SCROLL_ID)
            .and_then(|c| c.component::<ScrollContent>())
            .map(|s| s.is_following_latest())
    }

    pub fn mounted_turn_count(&self, host: &ContentHost) -> usize {
        host.get(TRANSCRIPT_TURNS_STACK_ID)
            .and_then(|c| c.component::<StackContent>())
            .map_or(0, |stack| stack.children().len())
    }

    pub fn mounted_turn_ids(&self, host: &ContentHost) -> Vec<ComponentId> {
        host.get(TRANSCRIPT_TURNS_STACK_ID)
            .and_then(|c| c.component::<StackContent>())
            .map(|stack| stack.children().iter().map(Content::id).collect())
            .unwrap_or_default()
    }

    pub fn activity_toggle_id_for_turn_key(&self, turn_key: &str) -> Option<ComponentId> {
        self.turn_toggle_ids.get(turn_key).copied()
    }

    pub fn is_turn_activity_expanded(&self, turn_key: &str) -> bool {
        self.expanded_activity_turns.contains(turn_key)
    }

    pub fn mounted_turn_keys(&self) -> &[String] {
        &self.mounted_turn_order
    }
}

fn tool_wording<'a>(label: &'a str) -> TranscriptToolRowWording<'a> {
    TranscriptToolRowWording {
        label,
        stopped: "Stopped",
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

fn tool_icon_name(tool_name: &str) -> &'static str {
    let base = tool_name
        .split('.')
        .next()
        .unwrap_or(tool_name)
        .to_ascii_lowercase();
    match base.as_str() {
        "shell" | "bash" | "pwsh" | "sh" | "zsh" | "exec" => "tekes-shell",
        "read" | "view" | "read_file" | "cat" | "list" | "ls" => "tekes-read",
        "write" | "edit" | "write_file" | "edit_file" | "patch" => "tekes-write",
        "web_search" | "search" => "tekes-web-search",
        "web_fetch" | "fetch" => "tekes-web-fetch",
        "terminal" | "command" | "run" => "tekes-command",
        "subagent" | "agent" | "task" => "tekes-task",
        _ => "tekes-tools",
    }
}

fn truncate_tool_detail(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_owned();
    }
    let mut out: String = trimmed.chars().take(max_chars).collect();
    out.push_str("\n…");
    out
}

fn collect_turn_models(snapshot: &RunSnapshot, search_query: &str) -> Vec<TurnRenderModel> {
    let query = search_query.trim().to_lowercase();
    let matches_query = |text: &str| query.is_empty() || text.to_lowercase().contains(&query);

    let Ok(doc) = TranscriptDocument::read(&snapshot.transcript) else {
        let mut fallback_turns = Vec::new();
        let mut current: Option<TurnRenderModel> = None;
        let mut idx = 0usize;
        for item in &snapshot.transcript.items {
            if !item.is_visible {
                continue;
            }
            if !matches_query(&item.text) {
                continue;
            }
            match item.kind {
                TranscriptItemKind::User => {
                    if let Some(prev) = current.take() {
                        fallback_turns.push(prev);
                    }
                    let key = if item.id.is_empty() {
                        format!("fallback-turn-{idx}")
                    } else {
                        item.id.clone()
                    };
                    idx += 1;
                    current = Some(TurnRenderModel {
                        turn_key: key,
                        user_text: item.text.clone(),
                        reasoning_parts: Vec::new(),
                        tools: Vec::new(),
                        assistant_markdown: String::new(),
                        error_text: None,
                        is_latest_settled: false,
                    });
                }
                TranscriptItemKind::Error => {
                    let entry = current.get_or_insert_with(|| {
                        idx += 1;
                        TurnRenderModel {
                            turn_key: format!("fallback-turn-{idx}"),
                            user_text: String::new(),
                            reasoning_parts: Vec::new(),
                            tools: Vec::new(),
                            assistant_markdown: String::new(),
                            error_text: None,
                            is_latest_settled: false,
                        }
                    });
                    entry.error_text = Some(item.text.clone());
                }
                _ => {
                    let entry = current.get_or_insert_with(|| {
                        idx += 1;
                        TurnRenderModel {
                            turn_key: format!("fallback-turn-{idx}"),
                            user_text: String::new(),
                            reasoning_parts: Vec::new(),
                            tools: Vec::new(),
                            assistant_markdown: String::new(),
                            error_text: None,
                            is_latest_settled: false,
                        }
                    });
                    if !item.reasoning.trim().is_empty() {
                        entry.reasoning_parts.push(item.reasoning.trim().to_owned());
                    }
                    if !item.text.trim().is_empty() {
                        if !entry.assistant_markdown.is_empty() {
                            entry.assistant_markdown.push_str("\n\n");
                        }
                        entry.assistant_markdown.push_str(&item.text);
                    }
                }
            }
        }
        if let Some(last) = current.take() {
            fallback_turns.push(last);
        }
        if !query.is_empty() && fallback_turns.is_empty() {
            fallback_turns.push(TurnRenderModel {
                turn_key: "__search-empty__".to_owned(),
                user_text: String::new(),
                reasoning_parts: Vec::new(),
                tools: Vec::new(),
                assistant_markdown: String::new(),
                error_text: Some(format!("No messages matching \"{search_query}\".")),
                is_latest_settled: false,
            });
        }
        return fallback_turns;
    };

    let mut models = Vec::new();
    for (idx, notice) in doc.session_notices.iter().enumerate() {
        if notice.item.text.trim().is_empty() || !matches_query(&notice.item.text) {
            continue;
        }
        models.push(TurnRenderModel {
            turn_key: format!("__notice_{idx}__"),
            user_text: String::new(),
            reasoning_parts: Vec::new(),
            tools: Vec::new(),
            assistant_markdown: String::new(),
            error_text: Some(notice.item.text.clone()),
            is_latest_settled: false,
        });
    }

    let mut provisional_flags = Vec::new();
    for (idx, section) in doc.sections.iter().enumerate() {
        let turn_key = if section.id.trim().is_empty() {
            format!("turn-{idx}")
        } else {
            section.id.to_owned()
        };

        let user_text = section
            .inputs
            .iter()
            .map(|i| i.item.text.trim())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n");

        let mut tools = Vec::new();
        for tool in &section.tools {
            let header = tool.presentation_header(
                tool_wording(&tool.name),
                &TranscriptToolRowInputs::default(),
                report_wording(),
            );
            let details = tool.presentation_details(
                tekes_agent_presentation::TranscriptToolDetailWording::default(),
            );
            let detail_sections = details
                .sections
                .into_iter()
                .filter_map(|s| {
                    let trimmed = s.text.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some((s.title, truncate_tool_detail(trimmed, 1200)))
                    }
                })
                .collect();
            tools.push(TurnToolModel {
                icon_name: tool_icon_name(&tool.name).to_owned(),
                header: header.text,
                running: details.running,
                detail_sections,
            });
        }

        let mut reasoning_parts = Vec::new();
        let mut assistant_chunks = Vec::new();
        let mut error_chunks = Vec::new();
        let mut has_provisional = false;
        for output in &section.outputs {
            if output.item.is_provisional {
                has_provisional = true;
            }
            if output.item.kind == TranscriptItemKind::Error {
                if !output.item.text.trim().is_empty() {
                    error_chunks.push(output.item.text.trim().to_owned());
                }
                continue;
            }
            if !output.item.reasoning.trim().is_empty() {
                reasoning_parts.push(output.item.reasoning.trim().to_owned());
            }
            if !output.item.text.trim().is_empty() {
                assistant_chunks.push(output.item.text.clone());
            }
        }
        let assistant_markdown = assistant_chunks.join("\n\n");
        let error_text = if error_chunks.is_empty() {
            None
        } else {
            Some(error_chunks.join("\n"))
        };

        if !query.is_empty() {
            let hit = matches_query(&user_text)
                || reasoning_parts.iter().any(|r| matches_query(r))
                || tools.iter().any(|t| {
                    matches_query(&t.header)
                        || t.detail_sections
                            .iter()
                            .any(|(title, body)| matches_query(title) || matches_query(body))
                })
                || matches_query(&assistant_markdown)
                || error_text.as_deref().is_some_and(matches_query);
            if !hit {
                continue;
            }
        }

        if user_text.is_empty()
            && reasoning_parts.is_empty()
            && tools.is_empty()
            && assistant_markdown.is_empty()
            && error_text.is_none()
        {
            continue;
        }

        provisional_flags.push(has_provisional);
        models.push(TurnRenderModel {
            turn_key,
            user_text,
            reasoning_parts,
            tools,
            assistant_markdown,
            error_text,
            is_latest_settled: false,
        });
    }

    if query.is_empty()
        && let (Some(last), Some(&is_provisional)) = (models.last_mut(), provisional_flags.last())
        && !last.assistant_markdown.is_empty()
        && !is_provisional
    {
        last.is_latest_settled = true;
    }

    if !query.is_empty() && models.is_empty() {
        models.push(TurnRenderModel {
            turn_key: "__search-empty__".to_owned(),
            user_text: String::new(),
            reasoning_parts: Vec::new(),
            tools: Vec::new(),
            assistant_markdown: String::new(),
            error_text: Some(format!("No messages matching \"{search_query}\".")),
            is_latest_settled: false,
        });
    }

    models
}

pub(crate) fn format_transcript_body(snapshot: &RunSnapshot) -> String {
    let models = collect_turn_models(snapshot, "");
    let mut chunks = Vec::new();
    for m in models {
        if !m.user_text.is_empty() {
            chunks.push(format!("You\n{}", m.user_text));
        }
        if !m.assistant_markdown.is_empty() {
            chunks.push(m.assistant_markdown);
        }
        if let Some(err) = m.error_text {
            chunks.push(err);
        }
    }
    chunks.join("\n\n")
}
