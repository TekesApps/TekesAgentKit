//! Strictly read-only `ChildRunSurface` for bounded/subagent execution traces.
//! Never mounts a writable `Composer` or `TextFieldContent` editor.

use crate::ResponsiveWidthMode;
use crate::ids::*;
use crate::transcript::{TranscriptReader, format_transcript_body};
use crate::ui_helpers::{caption_text, icon_button, label_text};
use tekes_agent_presentation::{RunScope, RunSnapshot};
use tekes_ui_controls::{
    ButtonVariant,
    composition::{
        Button, Content, ContentActionKind, ContentActionRecord, ContentHost, StackContent,
        TextContent,
    },
    extended::{Axis, Stack},
};
use tekes_ui_tokens::{Spacing, proposal::layout::MainAxisDistribution};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChildRunSurfaceStatus {
    Loading,
    Ready,
    Missing(String),
}

#[derive(Clone, Debug)]
pub enum ChildRunIntent {
    LoadEarlier,
    Refresh,
}

#[derive(Debug, Default)]
pub struct ChildRunSurface {
    last_header: String,
    last_status: String,
}

impl ChildRunSurface {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn build_content() -> Content {
        let header = Content::new(CHILD_RUN_HEADER_ID, label_text("Subagent trace"));
        let status = Content::new(CHILD_RUN_STATUS_ID, caption_text("Loading…"));

        let mut earlier_btn = Button::new("↑ Earlier");
        earlier_btn.variant = ButtonVariant::Quiet;
        earlier_btn.set_transparent_chrome(true);
        earlier_btn.button.state.disabled = true;
        let earlier = Content::new(CHILD_RUN_EARLIER_BTN_ID, earlier_btn);

        let refresh = Content::new(
            CHILD_RUN_REFRESH_BTN_ID,
            icon_button(CHILD_RUN_REFRESH_ICON_ID, "tekes-refresh", "Refresh trace"),
        );

        let actions_row = Content::new(
            CHILD_RUN_ACTIONS_ROW_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![earlier, refresh],
            },
        );

        let header_row = StackContent {
            model: Stack {
                axis: Axis::Horizontal,
                gap: Spacing::Tight.value(),
            },
            flexible_child: Some(CHILD_RUN_HEADER_ID),
            children: vec![header, actions_row],
        }
        .into_content_with_distribution(
            CHILD_RUN_HEADER_ROW_ID,
            MainAxisDistribution::SpaceBetween,
        );

        let reader = Content::new(CHILD_RUN_READER_ID, TranscriptReader::new());

        Content::new(
            CHILD_RUN_ROOT_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Regular.value(),
                },
                flexible_child: Some(CHILD_RUN_READER_ID),
                children: vec![header_row, status, reader],
            },
        )
    }

    pub fn sync(
        &mut self,
        host: &mut ContentHost,
        child_scope: &RunScope,
        status: &ChildRunSurfaceStatus,
        snapshot: &RunSnapshot,
        can_load_earlier: bool,
        _mode: ResponsiveWidthMode,
    ) {
        let header_text = format!("Subagent · {}", child_scope.session_id);
        if self.last_header != header_text {
            self.last_header = header_text.clone();
            if let Some(node) = host.get_mut(CHILD_RUN_HEADER_ID)
                && let Some(txt) = node.component_mut::<TextContent>()
            {
                txt.text = header_text;
            }
        }

        let status_text = match status {
            ChildRunSurfaceStatus::Loading => "Loading subagent trace…".to_owned(),
            ChildRunSurfaceStatus::Ready => "只读轨迹已就绪 · Read-only".to_owned(),
            ChildRunSurfaceStatus::Missing(reason) => format!("Missing · {reason}"),
        };
        if self.last_status != status_text {
            self.last_status = status_text.clone();
            if let Some(node) = host.get_mut(CHILD_RUN_STATUS_ID)
                && let Some(txt) = node.component_mut::<TextContent>()
            {
                txt.text = status_text;
            }
        }

        if let Some(node) = host.get_mut(CHILD_RUN_EARLIER_BTN_ID)
            && let Some(btn) = node.component_mut::<Button>()
        {
            btn.button.state.disabled =
                !(can_load_earlier && matches!(status, ChildRunSurfaceStatus::Ready));
        }

        let body = match status {
            ChildRunSurfaceStatus::Missing(reason) => {
                format!("Subagent trace unavailable:\n{reason}")
            }
            ChildRunSurfaceStatus::Loading if snapshot.transcript.turns.is_empty() => {
                "Loading subagent execution history…".to_owned()
            }
            _ => format_transcript_body(snapshot),
        };

        if let Some(node) = host.get_mut(CHILD_RUN_READER_ID)
            && let Some(reader) = node.component_mut::<TranscriptReader>()
        {
            reader.set_or_update(&child_scope.session_id, &body);
        }
    }

    pub fn handle_actions(&self, records: &[ContentActionRecord]) -> Vec<ChildRunIntent> {
        let mut intents = Vec::new();
        for rec in records {
            if !matches!(rec.action.kind, ContentActionKind::Activate) {
                continue;
            }
            if rec.action.target == CHILD_RUN_EARLIER_BTN_ID {
                intents.push(ChildRunIntent::LoadEarlier);
            } else if rec.action.target == CHILD_RUN_REFRESH_BTN_ID {
                intents.push(ChildRunIntent::Refresh);
            }
        }
        intents
    }
}
