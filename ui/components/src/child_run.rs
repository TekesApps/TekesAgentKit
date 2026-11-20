//! Strictly read-only `ChildRunSurface` for bounded/subagent execution traces.
//! Never mounts a writable `Composer` or `TextFieldContent` editor.

use crate::ResponsiveWidthMode;
use crate::ids::*;
use crate::transcript::{TranscriptReader, format_transcript_body};
use tekes_agent_presentation::{RunScope, RunSnapshot};
use tekes_ui_controls::{
    ButtonVariant,
    composition::{
        Button, Content, ContentActionKind, ContentActionRecord, ContentHost, StackContent,
        TextContent,
    },
    extended::{Axis, Stack},
};
use tekes_ui_tokens::Spacing;

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
        let header = Content::new(
            CHILD_RUN_HEADER_ID,
            TextContent::new("只读子代理轨迹 (无写权限 / Read-Only)"),
        );
        let status = Content::new(CHILD_RUN_STATUS_ID, TextContent::new("加载中..."));

        let mut earlier_btn = Button::new("加载更早轨迹");
        earlier_btn.variant = ButtonVariant::Quiet;
        earlier_btn.button.state.disabled = true;
        let earlier = Content::new(CHILD_RUN_EARLIER_BTN_ID, earlier_btn);

        let mut refresh_btn = Button::new("刷新子代理");
        refresh_btn.variant = ButtonVariant::Quiet;
        let refresh = Content::new(CHILD_RUN_REFRESH_BTN_ID, refresh_btn);

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

        let reader = Content::new(CHILD_RUN_READER_ID, TranscriptReader::new());

        Content::new(
            CHILD_RUN_ROOT_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: Some(CHILD_RUN_READER_ID),
                children: vec![header, actions_row, status, reader],
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
        mode: ResponsiveWidthMode,
    ) {
        let mode_label = match mode {
            ResponsiveWidthMode::Compact => "Compact",
            ResponsiveWidthMode::Expanded => "Expanded",
        };
        let header_text = format!(
            "只读子代理 · {} ({mode_label} · {} turns)",
            child_scope.session_id,
            snapshot.transcript.turns.len()
        );
        if self.last_header != header_text {
            self.last_header = header_text.clone();
            if let Some(node) = host.get_mut(CHILD_RUN_HEADER_ID)
                && let Some(txt) = node.component_mut::<TextContent>()
            {
                txt.text = header_text;
            }
        }

        let status_text = match status {
            ChildRunSurfaceStatus::Loading => "状态: 正在读取子代理历史...".to_owned(),
            ChildRunSurfaceStatus::Ready => format!(
                "状态: 只读轨迹已就绪 (hasMore={})",
                if can_load_earlier { "是" } else { "否" }
            ),
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
                format!("子代理轨迹缺失 (Missing):\n{reason}")
            }
            ChildRunSurfaceStatus::Loading if snapshot.transcript.turns.is_empty() => {
                "正在获取子代理执行历史...".to_owned()
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
