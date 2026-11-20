//! Structured Tekes-aligned Transcript surface over `RunSnapshot`, `TranscriptDocument`,
//! `ReadingController`, borderless `TextSurfacePresentation::Content`, per-turn `RichContent`,
//! subtle footnote icon actions, and rounded `PanelContent` approval cards.

use crate::ResponsiveWidthMode;
use crate::ids::*;
use crate::ui_helpers::{card_panel, icon_button, label_text, secondary_text};
use std::collections::BTreeMap;
use tekes_agent_presentation::{
    ActionablePrompt, ConnectionPhase, RunScope, RunSnapshot, TranscriptDocument,
    TranscriptItemKind, TranscriptToolReportWording, TranscriptToolRowInputs,
    TranscriptToolRowWording,
};
use tekes_ui_controls::{
    ButtonTextRenderer, ButtonVariant, ReadingAnchorUpdate, ReadingController,
    ReadingDocumentVersion, TextRangeMapping, TextSurface, TextSurfacePresentation,
    composition::{
        AccessibilityAction, Button, Component, ComponentId, Content, ContentActionKind,
        ContentActionPayload, ContentActionRecord, ContentEvent, ContentHost, ContentResponse,
        ContentRole, ContentSemantics, ContentTheme, CurveSubtreeCapability, DisclosureContent,
        RichContent, RichDocument, SearchCommandKind, SearchFieldContent, StackContent,
        TextContent,
    },
    extended::{Axis, Stack},
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
/// preserves scroll anchors cleanly across AppHost renders and slot swaps.
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

#[derive(Debug, Default)]
struct TranscriptSpacer {
    fill: bool,
}

impl Component for TranscriptSpacer {
    fn curve_subtree_capability(&self) -> CurveSubtreeCapability {
        CurveSubtreeCapability::Local
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

    fn measure(&mut self, _: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        MeasuredChild::new(Size::new(
            available.width,
            if self.fill { available.height } else { 0.0 },
        ))
    }

    fn paint(
        &mut self,
        _: &mut Scene,
        _: &mut dyn ButtonTextRenderer,
        _: Rect,
        _: Rect,
        _: ContentTheme,
    ) {
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
    search_open: bool,
    thinking_expanded: bool,
    cached_thinking_disc: Option<Content>,
    rich_doc_revision: u64,
    last_rich_source: String,
    last_user_bubble: String,
    last_activity_sig: String,
    last_notice_sig: String,
    last_has_earlier: bool,
    last_following: bool,
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
            search_open: false,
            thinking_expanded: false,
            cached_thinking_disc: None,
            rich_doc_revision: 1,
            last_rich_source: String::new(),
            last_user_bubble: String::new(),
            last_activity_sig: String::new(),
            last_notice_sig: String::new(),
            last_has_earlier: false,
            last_following: true,
            choice_bindings: BTreeMap::new(),
            rendered_actionables_sig: String::new(),
        }
    }

    pub fn build_content() -> Content {
        let header = Content::new(
            TRANSCRIPT_HEADER_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: 0.0,
                },
                flexible_child: None,
                children: Vec::new(),
            },
        );

        let search_row = Content::new(
            TRANSCRIPT_SEARCH_ROW_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: Vec::new(),
            },
        );

        let user_bubble_slot = Content::new(
            TRANSCRIPT_LATEST_USER_SLOT_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: 0.0,
                },
                flexible_child: None,
                children: Vec::new(),
            },
        );

        let rich_doc = RichDocument::from_markdown("");
        let rich_content = RichContent::new("transcript-rich-latest", 1, rich_doc)
            .expect("valid initial RichContent");

        let copy_btn = icon_button(TRANSCRIPT_COPY_ICON_ID, "tekes-copy", "Copy");
        let up_btn = icon_button(TRANSCRIPT_UP_ICON_ID, "tekes-thumbs-up", "Good response");
        let down_btn = icon_button(TRANSCRIPT_DOWN_ICON_ID, "tekes-thumbs-down", "Bad response");
        let fork_btn = icon_button(TRANSCRIPT_FORK_ICON_ID, "tekes-fork", "Fork thread");

        let footnote_row = Content::new(
            TRANSCRIPT_FOOTNOTE_ROW_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![
                    Content::new(TRANSCRIPT_COPY_BTN_ID, copy_btn),
                    Content::new(TRANSCRIPT_UP_BTN_ID, up_btn),
                    Content::new(TRANSCRIPT_DOWN_BTN_ID, down_btn),
                    Content::new(TRANSCRIPT_FORK_BTN_ID, fork_btn),
                ],
            },
        );

        let activity_disc = Content::new(
            TRANSCRIPT_THINKING_DISC_ID,
            DisclosureContent::new(
                TRANSCRIPT_THINKING_TRIGGER_ID,
                Content::new(ComponentId(113), secondary_text("Thought process")),
                Content::new(TRANSCRIPT_THINKING_BODY_ID, secondary_text("")),
                false,
            )
            .with_section_header_layout(),
        );

        let latest_turn = Content::new(
            TRANSCRIPT_LATEST_TURN_SLOT_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Regular.value(),
                },
                flexible_child: None,
                children: vec![
                    activity_disc,
                    Content::new(TRANSCRIPT_RICH_BODY_ID, rich_content),
                    footnote_row,
                ],
            },
        );

        let latest_slot = Content::new(
            TRANSCRIPT_LATEST_SLOT_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: 0.0,
                },
                flexible_child: None,
                children: Vec::new(),
            },
        );

        let actionables_stack = Content::new(
            TRANSCRIPT_ACTIONABLES_STACK_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Regular.value(),
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
                    user_bubble_slot,
                    latest_turn,
                    latest_slot,
                    actionables_stack,
                    Content::new(TRANSCRIPT_SPACER_ID, TranscriptSpacer { fill: true }),
                ],
            },
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
                row.flexible_child = Some(TRANSCRIPT_SEARCH_ID);
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
                row.flexible_child = None;
            }
            replace_stack_children(host, TRANSCRIPT_SEARCH_ROW_ID, Vec::new());
        }
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
        let sig = format!("{notice_text:?}:{has_earlier}");
        if sig == self.last_notice_sig {
            return;
        }
        self.last_notice_sig = sig;
        self.last_has_earlier = has_earlier;

        let mut children = Vec::new();
        if let Some(msg) = notice_text {
            let mut reconnect_btn = Button::new("Reconnect");
            reconnect_btn.variant = ButtonVariant::Panel;
            let row = StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: Some(TRANSCRIPT_NOTICE_TEXT_ID),
                children: vec![
                    Content::new(TRANSCRIPT_NOTICE_TEXT_ID, secondary_text(msg)),
                    Content::new(TRANSCRIPT_RECONNECT_BTN_ID, reconnect_btn),
                ],
            }
            .into_content_with_distribution(ComponentId(116), MainAxisDistribution::SpaceBetween);
            children.push(card_panel(
                TRANSCRIPT_NOTICE_PANEL_ID,
                row,
                Insets::new(8.0, 12.0, 8.0, 12.0),
            ));
        }

        if has_earlier {
            let mut earlier_btn = Button::new("↑ Load earlier messages");
            earlier_btn.variant = ButtonVariant::Quiet;
            earlier_btn.set_transparent_chrome(true);
            children.push(Content::new(TRANSCRIPT_EARLIER_BTN_ID, earlier_btn));
        }

        replace_stack_children(host, TRANSCRIPT_HEADER_ID, children);
    }

    fn sync_user_bubble(&mut self, host: &mut ContentHost, latest_user: &str) {
        if latest_user == self.last_user_bubble {
            return;
        }
        self.last_user_bubble = latest_user.to_owned();
        if latest_user.trim().is_empty() {
            replace_stack_children(host, TRANSCRIPT_LATEST_USER_SLOT_ID, Vec::new());
            return;
        }
        let bubble = card_panel(
            TRANSCRIPT_USER_BUBBLE_PANEL_ID,
            Content::new(
                TRANSCRIPT_USER_BUBBLE_TEXT_ID,
                TextContent::new(latest_user),
            ),
            Insets::new(8.0, 14.0, 8.0, 14.0),
        );
        let aligned = StackContent {
            model: Stack {
                axis: Axis::Horizontal,
                gap: 0.0,
            },
            flexible_child: None,
            children: vec![bubble],
        }
        .into_content_with_distribution(TRANSCRIPT_USER_BUBBLE_ROW_ID, MainAxisDistribution::End);
        replace_stack_children(host, TRANSCRIPT_LATEST_USER_SLOT_ID, vec![aligned]);
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
            let aligned = StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: 0.0,
                },
                flexible_child: None,
                children: vec![Content::new(TRANSCRIPT_LATEST_BTN_ID, btn)],
            }
            .into_content_with_distribution(ComponentId(139), MainAxisDistribution::End);
            replace_stack_children(host, TRANSCRIPT_LATEST_SLOT_ID, vec![aligned]);
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
                    TRANSCRIPT_LATEST_USER_SLOT_ID,
                    TRANSCRIPT_LATEST_TURN_SLOT_ID,
                    TRANSCRIPT_LATEST_SLOT_ID,
                    TRANSCRIPT_ACTIONABLES_STACK_ID,
                    TRANSCRIPT_SPACER_ID,
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

        self.sync_header_slot(host, snapshot);

        let parts = split_transcript_parts(snapshot, &self.search_query);

        let reader_key = format!("transcript:{dest_key}:filter:{}", self.search_query);
        if let Some(reader) = host
            .get_mut(TRANSCRIPT_READER_ID)
            .and_then(|c| c.component_mut::<TranscriptReader>())
        {
            reader.set_or_update(&reader_key, &parts.history_text);
        }
        let empty_history = parts.history_text.trim().is_empty();
        if let Some(spacer) = host
            .get_mut(TRANSCRIPT_SPACER_ID)
            .and_then(|c| c.component_mut::<TranscriptSpacer>())
        {
            spacer.fill = empty_history;
        }
        if let Some(root) = host
            .get_mut(TRANSCRIPT_ROOT_ID)
            .and_then(|c| c.component_mut::<StackContent>())
        {
            root.flexible_child = Some(if empty_history {
                TRANSCRIPT_SPACER_ID
            } else {
                TRANSCRIPT_READER_ID
            });
        }

        self.sync_user_bubble(host, &parts.latest_user);

        let has_activity = !parts.activity_detail.trim().is_empty();
        if has_activity {
            if host.get(TRANSCRIPT_THINKING_DISC_ID).is_none()
                && let Some(disc) = self.cached_thinking_disc.take()
            {
                let _ = host.insert(TRANSCRIPT_LATEST_TURN_SLOT_ID, disc);
                let _ = host.reorder(
                    TRANSCRIPT_LATEST_TURN_SLOT_ID,
                    &[
                        TRANSCRIPT_THINKING_DISC_ID,
                        TRANSCRIPT_RICH_BODY_ID,
                        TRANSCRIPT_FOOTNOTE_ROW_ID,
                    ],
                );
            }
        } else if host.get(TRANSCRIPT_THINKING_DISC_ID).is_some()
            && let Ok(disc) = host.remove(TRANSCRIPT_THINKING_DISC_ID)
        {
            self.cached_thinking_disc = Some(disc);
        }

        let activity_sig = format!(
            "{}:{}:{}",
            parts.activity_title, parts.activity_detail, parts.has_latest_assistant
        );
        if activity_sig != self.last_activity_sig {
            self.last_activity_sig = activity_sig;
            if let Some(label) = host
                .get_mut(ComponentId(113))
                .and_then(|c| c.component_mut::<TextContent>())
            {
                label.text = if parts.activity_title.is_empty() {
                    "Thought process".to_owned()
                } else {
                    parts.activity_title.clone()
                };
            }
            if let Some(body) = host
                .get_mut(TRANSCRIPT_THINKING_BODY_ID)
                .and_then(|c| c.component_mut::<TextContent>())
            {
                body.text = parts.activity_detail.clone();
            }
        }
        if let Some(disc) = host
            .get_mut(TRANSCRIPT_THINKING_DISC_ID)
            .and_then(|c| c.component_mut::<DisclosureContent>())
        {
            disc.set_expanded(self.thinking_expanded && !parts.activity_detail.is_empty());
        }

        if self.last_rich_source != parts.latest_markdown {
            let next_rev = self.rich_doc_revision.saturating_add(1).max(1);
            let doc = RichDocument::from_markdown(&parts.latest_markdown);
            if let Some(rich) = host
                .get_mut(TRANSCRIPT_RICH_BODY_ID)
                .and_then(|c| c.component_mut::<RichContent>())
            {
                let expected = rich.version();
                if rich.update(&expected, next_rev, doc).is_ok() {
                    self.rich_doc_revision = next_rev;
                    self.last_rich_source = parts.latest_markdown;
                }
            }
        }

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
                btn.variant = if choice.id == "allow" || choice.id == "option-0" {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Panel
                };
                buttons.push(Content::new(btn_id, btn));
            }

            let mut card_children = vec![Content::new(title_id, label_text(&prompt.title))];
            if !prompt.detail.trim().is_empty() {
                card_children.push(Content::new(detail_id, secondary_text(&prompt.detail)));
            }
            card_children.push(Content::new(
                buttons_row_id,
                StackContent {
                    model: Stack {
                        axis: Axis::Horizontal,
                        gap: Spacing::Tight.value(),
                    },
                    flexible_child: None,
                    children: buttons,
                },
            ));

            let body = Content::new(
                stack_id,
                StackContent {
                    model: Stack {
                        axis: Axis::Vertical,
                        gap: Spacing::Tight.value(),
                    },
                    flexible_child: None,
                    children: card_children,
                },
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
                if record.action.target == TRANSCRIPT_THINKING_DISC_ID {
                    self.thinking_expanded = intent.expanded();
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
            if matches!(record.action.kind, ContentActionKind::Activate) {
                match record.action.target {
                    id if id == TRANSCRIPT_EARLIER_BTN_ID => {
                        if let Some(tekes_agent_presentation::ComposerDestination::Run { scope }) =
                            snapshot.selection.as_ref().map(|s| &s.destination)
                        {
                            out.push(TranscriptIntent::LoadEarlier(scope.clone()));
                        }
                    }
                    id if id == TRANSCRIPT_LATEST_BTN_ID => {
                        if let Some(reader) = host
                            .get_mut(TRANSCRIPT_READER_ID)
                            .and_then(|c| c.component_mut::<TranscriptReader>())
                        {
                            reader.follow_latest();
                        }
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

struct TranscriptRenderParts {
    history_text: String,
    latest_user: String,
    latest_markdown: String,
    activity_title: String,
    activity_detail: String,
    has_latest_assistant: bool,
}

pub(crate) fn format_transcript_body(snapshot: &RunSnapshot) -> String {
    let parts = split_transcript_parts(snapshot, "");
    let mut chunks = Vec::new();
    if !parts.history_text.is_empty() {
        chunks.push(parts.history_text);
    }
    if !parts.latest_user.is_empty() {
        chunks.push(format!("You\n{}", parts.latest_user));
    }
    if !parts.latest_markdown.is_empty() {
        chunks.push(parts.latest_markdown);
    }
    chunks.join("\n\n")
}

fn split_transcript_parts(snapshot: &RunSnapshot, search_query: &str) -> TranscriptRenderParts {
    let query = search_query.trim().to_lowercase();
    let matches_query = |text: &str| query.is_empty() || text.to_lowercase().contains(&query);

    let Ok(doc) = TranscriptDocument::read(&snapshot.transcript) else {
        let fallback = snapshot
            .transcript
            .items
            .iter()
            .filter(|i| i.is_visible && matches_query(&i.text))
            .map(|i| match i.kind {
                TranscriptItemKind::User => format!("> {}", i.text),
                TranscriptItemKind::Error => format!("Error: {}", i.text),
                _ => i.text.clone(),
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        return TranscriptRenderParts {
            history_text: fallback,
            latest_user: String::new(),
            latest_markdown: String::new(),
            activity_title: String::new(),
            activity_detail: String::new(),
            has_latest_assistant: false,
        };
    };

    let section_count = doc.sections.len();
    let split_idx = if query.is_empty() && section_count > 0 {
        section_count - 1
    } else {
        section_count
    };

    let mut history_lines = Vec::new();
    for notice in &doc.session_notices {
        if matches_query(&notice.item.text) {
            history_lines.push(notice.item.text.clone());
        }
    }

    for section in &doc.sections[..split_idx] {
        for input in &section.inputs {
            if matches_query(&input.item.text) {
                history_lines.push(input.item.text.clone());
            }
        }
        for output in &section.outputs {
            if !output.item.text.trim().is_empty() && matches_query(&output.item.text) {
                history_lines.push(output.item.text.clone());
            }
        }
    }

    let mut latest_user = String::new();
    let mut latest_markdown = String::new();
    let mut tool_headers = Vec::new();
    let mut thinking_parts = Vec::new();

    if let Some(last) = doc.sections.get(split_idx) {
        latest_user = last
            .inputs
            .iter()
            .map(|i| i.item.text.trim())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n");

        for tool in &last.tools {
            let header = tool.presentation_header(
                tool_wording(&tool.name),
                &TranscriptToolRowInputs::default(),
                report_wording(),
            );
            tool_headers.push(header.text);
        }

        let mut assistant_chunks = Vec::new();
        for output in &last.outputs {
            if !output.item.reasoning.trim().is_empty() {
                thinking_parts.push(output.item.reasoning.trim().to_owned());
            }
            if !output.item.text.trim().is_empty() {
                assistant_chunks.push(output.item.text.clone());
            }
        }
        latest_markdown = assistant_chunks.join("\n\n");
    }

    let has_latest_assistant = !latest_markdown.is_empty();
    let mut activity_parts = Vec::new();
    if !thinking_parts.is_empty() {
        activity_parts.push(thinking_parts.join("\n"));
    }
    if !tool_headers.is_empty() {
        activity_parts.push(tool_headers.join("\n"));
    }
    let activity_detail = activity_parts.join("\n\n");

    let activity_title = if !tool_headers.is_empty() && !thinking_parts.is_empty() {
        format!(
            "Thought process · {} tool{}",
            tool_headers.len(),
            if tool_headers.len() == 1 { "" } else { "s" }
        )
    } else if !tool_headers.is_empty() {
        format!(
            "Used {} tool{}",
            tool_headers.len(),
            if tool_headers.len() == 1 { "" } else { "s" }
        )
    } else if !thinking_parts.is_empty() {
        "Thought process".to_owned()
    } else {
        String::new()
    };

    let history_text = if history_lines.is_empty() {
        if query.is_empty() {
            String::new()
        } else {
            format!("No messages matching \"{search_query}\".")
        }
    } else {
        history_lines.join("\n\n")
    };

    TranscriptRenderParts {
        history_text,
        latest_user,
        latest_markdown,
        activity_title,
        activity_detail,
        has_latest_assistant,
    }
}
