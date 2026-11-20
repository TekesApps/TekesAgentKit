//! Ephemeral `SideChatSurface` with `PaneAccess` gating, isolated `TextFieldContent` draft/IME
//! state, and owned Close/Discard control.

use crate::ResponsiveWidthMode;
use crate::ids::*;
use crate::transcript::format_transcript_body;
use crate::ui_helpers::{caption_text, card_panel, heading_text, secondary_text};
use tekes_agent_presentation::RunSnapshot;
use tekes_ui_controls::{
    ButtonVariant, DraftRevision, ExternalSelection, ExternalUndo, ScrollArea,
    TextSurfacePresentation,
    composition::{
        Button, Component, Content, ContentActionKind, ContentActionRecord, ContentHost,
        RichContent, RichDocument, ScrollContent, StackContent, TextContent, TextFieldContent,
        TextSubmitPolicy, TextViewport,
    },
    extended::{Axis, Stack},
    layout::Insets,
};
use tekes_ui_tokens::{Spacing, proposal::layout::MainAxisDistribution};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SideChatSurfaceAccess {
    Available,
    Creating,
    Closing,
    Unsupported,
    Disconnected,
    Unknown(String),
    Closed,
}

#[derive(Clone, Debug)]
pub enum SideChatIntent {
    CreateBranch,
    CloseDiscard,
    EditDraft(String),
    SendPrompt,
}

#[derive(Debug, Default)]
pub struct SideChatSurface {
    last_header: String,
    last_access: String,
    last_error: String,
    last_body: String,
    rich_rev: u64,
    draft_rev: u64,
    last_draft_synced: String,
    active_session_id: Option<String>,
}

impl SideChatSurface {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn build_content() -> Content {
        let header = Content::new(SIDECHAT_HEADER_ID, heading_text("SideChat"));
        let access_badge = Content::new(SIDECHAT_ACCESS_BADGE_ID, caption_text("Available"));
        let error = Content::new(SIDECHAT_ERROR_ID, secondary_text(""));

        let mut create_btn = Button::new("New SideChat");
        create_btn.variant = ButtonVariant::Panel;
        let create = Content::new(SIDECHAT_CREATE_BTN_ID, create_btn);

        let mut close_btn = Button::new("Close");
        close_btn.variant = ButtonVariant::Quiet;
        close_btn.set_transparent_chrome(true);
        close_btn.button.state.disabled = true;
        let close = Content::new(SIDECHAT_CLOSE_BTN_ID, close_btn);

        let actions = Content::new(
            SIDECHAT_ACTIONS_ROW_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![access_badge, create, close],
            },
        );

        let header_row = StackContent {
            model: Stack {
                axis: Axis::Horizontal,
                gap: Spacing::Tight.value(),
            },
            flexible_child: Some(SIDECHAT_HEADER_ID),
            children: vec![header, actions],
        }
        .into_content_with_distribution(SIDECHAT_HEADER_ROW_ID, MainAxisDistribution::SpaceBetween);

        let rich = Content::new(
            SIDECHAT_RICH_ID,
            RichContent::new(
                "sidechat-doc",
                1,
                RichDocument::from_markdown(
                    "Start a **SideChat** branch to discuss an aside without touching the main run.",
                ),
            )
            .expect("valid sidechat RichContent"),
        );
        let scroll = Content::new(
            SIDECHAT_SCROLL_ID,
            ScrollContent::new(ScrollArea::default(), rich),
        );

        let mut field = TextFieldContent::new("SideChat 独立输入", "");
        field
            .surface
            .set_presentation(TextSurfacePresentation::Content);
        field.show_metadata = false;
        field.viewport = TextViewport::Grow {
            min: 40.0,
            max: 140.0,
        };
        field.submit_policy = TextSubmitPolicy::CommandEnter;
        field.set_placeholder(Some("Ask a side question…".to_owned()));
        let editor = Content::new(SIDECHAT_EDITOR_ID, field);

        let mut send_btn = Button::new("↑").with_accessible_name("Send SideChat prompt");
        send_btn.variant = ButtonVariant::Primary;
        send_btn.button.state.disabled = true;
        let send = Content::new(SIDECHAT_SEND_BTN_ID, send_btn);

        let composer_row = Content::new(
            SIDECHAT_COMPOSER_ROW_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: Some(SIDECHAT_EDITOR_ID),
                children: vec![editor, send],
            },
        );
        let composer_panel = card_panel(
            SIDECHAT_COMPOSER_PANEL_ID,
            composer_row,
            Insets::new(8.0, 12.0, 8.0, 12.0),
        );

        Content::new(
            SIDECHAT_ROOT_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Regular.value(),
                },
                flexible_child: Some(SIDECHAT_SCROLL_ID),
                children: vec![header_row, error, scroll, composer_panel],
            },
        )
    }

    pub fn editor_composing(&self, host: &ContentHost) -> bool {
        host.get(SIDECHAT_EDITOR_ID)
            .and_then(|n| n.component::<TextFieldContent>())
            .is_some_and(|f| f.composing())
    }

    fn apply_draft_to_editor(&mut self, host: &mut ContentHost, identity: &str, text: &str) {
        if let Some(node) = host.get_mut(SIDECHAT_EDITOR_ID)
            && let Some(field) = node.component_mut::<TextFieldContent>()
            && !field.composing()
            && field.text() != text
        {
            let baseline = field.surface.input().external_baseline();
            self.draft_rev = self.draft_rev.saturating_add(1).max(1);
            let _ = field.surface.apply_external_value(
                &baseline,
                &DraftRevision {
                    identity: identity.to_owned(),
                    version: self.draft_rev,
                    payload: text.to_owned(),
                },
                ExternalSelection::Preserve,
                ExternalUndo::Transaction,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn sync(
        &mut self,
        host: &mut ContentHost,
        title: Option<&str>,
        session_id: Option<&str>,
        access: &SideChatSurfaceAccess,
        run_snapshot: Option<&RunSnapshot>,
        error: Option<&str>,
        _mode: ResponsiveWidthMode,
    ) {
        let header_text = match (title, session_id) {
            (Some(t), Some(_)) => t.to_owned(),
            (Some(t), None) => t.to_owned(),
            _ => "SideChat".to_owned(),
        };
        if self.last_header != header_text {
            self.last_header = header_text.clone();
            if let Some(node) = host.get_mut(SIDECHAT_HEADER_ID)
                && let Some(txt) = node.component_mut::<TextContent>()
            {
                txt.text = header_text;
            }
        }

        let access_label = match access {
            SideChatSurfaceAccess::Available => "Available",
            SideChatSurfaceAccess::Creating => "Creating",
            SideChatSurfaceAccess::Closing => "Closing",
            SideChatSurfaceAccess::Unsupported => "Unsupported",
            SideChatSurfaceAccess::Disconnected => "Disconnected",
            SideChatSurfaceAccess::Unknown(_) => "Unknown",
            SideChatSurfaceAccess::Closed => "Closed",
        };
        if self.last_access != access_label {
            self.last_access = access_label.to_owned();
            if let Some(node) = host.get_mut(SIDECHAT_ACCESS_BADGE_ID)
                && let Some(txt) = node.component_mut::<TextContent>()
            {
                txt.text = access_label.to_owned();
            }
        }

        let err_str = match access {
            SideChatSurfaceAccess::Unknown(reason) => error.unwrap_or(reason.as_str()).to_owned(),
            _ => error.unwrap_or("").to_owned(),
        };
        if self.last_error != err_str {
            self.last_error = err_str.clone();
            if let Some(node) = host.get_mut(SIDECHAT_ERROR_ID)
                && let Some(txt) = node.component_mut::<TextContent>()
            {
                txt.text = err_str;
            }
        }

        let has_chat = session_id.is_some();
        let can_send = has_chat && matches!(access, SideChatSurfaceAccess::Available);
        let can_create = !has_chat
            && matches!(
                access,
                SideChatSurfaceAccess::Available | SideChatSurfaceAccess::Closed
            );
        if let Some(node) = host.get_mut(SIDECHAT_CREATE_BTN_ID)
            && let Some(btn) = node.component_mut::<Button>()
        {
            btn.button.state.disabled = !can_create;
        }
        if let Some(node) = host.get_mut(SIDECHAT_CLOSE_BTN_ID)
            && let Some(btn) = node.component_mut::<Button>()
        {
            btn.button.state.disabled = !can_send;
        }
        if let Some(node) = host.get_mut(SIDECHAT_SEND_BTN_ID)
            && let Some(btn) = node.component_mut::<Button>()
        {
            btn.button.state.disabled = !can_send;
        }

        let md = match (run_snapshot, access) {
            (Some(snap), _) => format_transcript_body(snap),
            (None, SideChatSurfaceAccess::Closed) => {
                "SideChat closed and discarded from the runtime; main run unaffected.".to_owned()
            }
            (None, SideChatSurfaceAccess::Unsupported) => {
                "SideChat is unsupported by the active harness.".to_owned()
            }
            (None, SideChatSurfaceAccess::Disconnected) => {
                "Connection unavailable; SideChat disabled.".to_owned()
            }
            (None, _) => {
                "Start a **SideChat** branch to discuss an aside without touching the main run."
                    .to_owned()
            }
        };
        if self.last_body != md {
            let next_rev = self.rich_rev.saturating_add(1).max(2);
            if let Some(node) = host.get_mut(SIDECHAT_RICH_ID)
                && let Some(rich) = node.component_mut::<RichContent>()
            {
                let expected = rich.version();
                if rich
                    .update(&expected, next_rev, RichDocument::from_markdown(&md))
                    .is_ok()
                {
                    self.rich_rev = next_rev;
                    self.last_body = md;
                }
            }
        }

        let new_sid = session_id.map(str::to_owned);
        let session_switched = self.active_session_id != new_sid;
        self.active_session_id = new_sid;
        if let Some(snap) = run_snapshot {
            let target_draft = snap.draft.text.clone();
            if !self.editor_composing(host)
                && (session_switched || self.last_draft_synced != target_draft)
            {
                self.last_draft_synced = target_draft.clone();
                let ident = session_id.unwrap_or("sidechat");
                self.apply_draft_to_editor(host, ident, &target_draft);
            }
        } else if session_switched && !self.editor_composing(host) {
            self.last_draft_synced.clear();
            self.apply_draft_to_editor(host, "closed", "");
        }
    }

    pub fn handle_actions(
        &self,
        host: &ContentHost,
        records: &[ContentActionRecord],
    ) -> Vec<SideChatIntent> {
        let mut intents = Vec::new();
        let composing = self.editor_composing(host);
        for rec in records {
            if matches!(rec.action.kind, ContentActionKind::Activate) {
                if rec.action.target == SIDECHAT_CREATE_BTN_ID {
                    intents.push(SideChatIntent::CreateBranch);
                } else if rec.action.target == SIDECHAT_CLOSE_BTN_ID {
                    intents.push(SideChatIntent::CloseDiscard);
                } else if rec.action.target == SIDECHAT_SEND_BTN_ID && !composing {
                    intents.push(SideChatIntent::SendPrompt);
                }
            }
        }
        if !composing
            && let Some(text) = host
                .get(SIDECHAT_EDITOR_ID)
                .and_then(|n| n.component::<TextFieldContent>())
                .map(|f| f.text().to_owned())
            && text != self.last_draft_synced
        {
            intents.push(SideChatIntent::EditDraft(text));
        }
        intents
    }
}
