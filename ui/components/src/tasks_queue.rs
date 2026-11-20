//! `TasksQueueSurface`: renders queued prompts (with epoch-fenced inline edit & remove)
//! and background/subagent tasks (`AgentTask` / `jobs`), emitting `OpenChildRun` intents.

use crate::ResponsiveWidthMode;
use crate::ids::*;
use tekes_agent_presentation::RunScope;
use tekes_ui_controls::{
    ButtonVariant, DraftRevision, ExternalSelection, ExternalUndo, ScrollArea,
    composition::{
        Button, Component, ComponentId, Content, ContentActionKind, ContentActionRecord,
        ContentHost, ScrollContent, StackContent, TextContent, TextFieldContent,
    },
    extended::{Axis, Stack},
};
use tekes_ui_tokens::Spacing;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueuedPromptViewModel {
    pub item_id: String,
    pub session_id: String,
    pub text: String,
    pub connection_epoch: u64,
    pub can_mutate: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskRowViewModel {
    pub id: String,
    pub title: String,
    pub status: String,
    pub canonical_status: String,
    pub kind: String,
    pub parent_scope: Option<RunScope>,
    pub child_session_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TasksQueueViewModel {
    pub epoch: u64,
    pub queued: Vec<QueuedPromptViewModel>,
    pub tasks: Vec<TaskRowViewModel>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TasksQueueIntent {
    EditQueued {
        item_id: String,
        session_id: String,
        epoch: u64,
        text: String,
    },
    RemoveQueued {
        item_id: String,
        session_id: String,
        epoch: u64,
    },
    OpenChildRun {
        parent_scope: RunScope,
        task_id: String,
        child_session_id: String,
        title: String,
    },
}

#[derive(Debug, Default)]
pub struct TasksQueueSurface {
    last_sig: String,
    last_error_sig: Option<String>,
    selected_queue_item_id: Option<String>,
    synced_rev: u64,
    open_targets: Vec<(ComponentId, TaskRowViewModel)>,
}

impl TasksQueueSurface {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn build_content() -> Content {
        let header = Content::new(
            TASKS_HEADER_ID,
            TextContent::new("排队消息与任务 · 等待运行快照..."),
        );
        let error = Content::new(TASKS_ERROR_ID, TextContent::new(""));

        let mut edit_field = TextFieldContent::new("修改排队消息", "");
        edit_field.show_metadata = false;
        let edit_input = Content::new(TASKS_QUEUE_EDIT_INPUT_ID, edit_field);
        let mut apply_btn = Button::new("更新排队项");
        apply_btn.variant = ButtonVariant::Primary;
        let apply = Content::new(TASKS_QUEUE_APPLY_BTN_ID, apply_btn);
        let mut rm_btn = Button::new("移除");
        rm_btn.variant = ButtonVariant::Quiet;
        let remove = Content::new(TASKS_QUEUE_REMOVE_BTN_ID, rm_btn);

        let edit_row = Content::new(
            TASKS_QUEUE_EDIT_ROW_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: Some(TASKS_QUEUE_EDIT_INPUT_ID),
                children: vec![edit_input, apply, remove],
            },
        );

        let queue_section = Content::new(
            TASKS_QUEUE_SECTION_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: Vec::new(),
            },
        );
        let jobs_section = Content::new(
            TASKS_JOBS_SECTION_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: Vec::new(),
            },
        );

        let scroll = Content::new(
            TASKS_SCROLL_ID,
            ScrollContent::new(
                ScrollArea::default(),
                Content::new(
                    TASKS_STACK_ID,
                    StackContent {
                        model: Stack {
                            axis: Axis::Vertical,
                            gap: Spacing::Tight.value(),
                        },
                        flexible_child: None,
                        children: vec![queue_section, jobs_section],
                    },
                ),
            ),
        );

        Content::new(
            TASKS_ROOT_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: Some(TASKS_SCROLL_ID),
                children: vec![header, error, edit_row, scroll],
            },
        )
    }

    fn set_external_editor_value(
        &mut self,
        host: &mut ContentHost,
        identity: &str,
        new_text: &str,
    ) {
        if let Some(node) = host.get_mut(TASKS_QUEUE_EDIT_INPUT_ID)
            && let Some(field) = node.component_mut::<TextFieldContent>()
            && !field.composing()
            && field.text() != new_text
        {
            let baseline = field.surface.input().external_baseline();
            self.synced_rev = self.synced_rev.saturating_add(1).max(1);
            let _ = field.surface.apply_external_value(
                &baseline,
                &DraftRevision {
                    identity: identity.to_owned(),
                    version: self.synced_rev,
                    payload: new_text.to_owned(),
                },
                ExternalSelection::Preserve,
                ExternalUndo::Transaction,
            );
        }
    }

    pub fn sync(
        &mut self,
        host: &mut ContentHost,
        model: &TasksQueueViewModel,
        mode: ResponsiveWidthMode,
    ) {
        let mode_str = match mode {
            ResponsiveWidthMode::Compact => "Compact",
            ResponsiveWidthMode::Expanded => "Expanded",
        };
        let header_text = format!(
            "队列 {} · 任务 {} ({mode_str} · epoch {})",
            model.queued.len(),
            model.tasks.len(),
            model.epoch
        );
        if let Some(node) = host.get_mut(TASKS_HEADER_ID)
            && let Some(txt) = node.component_mut::<TextContent>()
            && txt.text != header_text
        {
            txt.text = header_text;
        }

        if self.last_error_sig != model.error {
            self.last_error_sig = model.error.clone();
            if let Some(node) = host.get_mut(TASKS_ERROR_ID)
                && let Some(txt) = node.component_mut::<TextContent>()
            {
                txt.text = model.error.clone().unwrap_or_default();
            }
        }

        let first_queued = model.queued.first();
        let active_item = self
            .selected_queue_item_id
            .as_deref()
            .and_then(|id| model.queued.iter().find(|q| q.item_id == id))
            .or(first_queued);

        if let Some(item) = active_item.cloned() {
            self.selected_queue_item_id = Some(item.item_id.clone());
            self.set_external_editor_value(host, &item.item_id, &item.text);
        } else {
            self.selected_queue_item_id = None;
            self.set_external_editor_value(host, "empty", "");
        }

        let can_mutate = active_item.is_some_and(|q| q.can_mutate);
        for btn_id in [TASKS_QUEUE_APPLY_BTN_ID, TASKS_QUEUE_REMOVE_BTN_ID] {
            if let Some(node) = host.get_mut(btn_id)
                && let Some(btn) = node.component_mut::<Button>()
            {
                btn.button.state.disabled = !can_mutate;
            }
        }

        let sig = format!("{model:?}");
        if self.last_sig == sig {
            return;
        }
        self.last_sig = sig;

        let mut queue_children = Vec::new();
        if model.queued.is_empty() {
            queue_children.push(Content::new(
                ComponentId(TASKS_QUEUE_ROW_BASE),
                TextContent::new("排队消息: 暂无等待发送项"),
            ));
        } else {
            for (idx, q) in model.queued.iter().enumerate() {
                queue_children.push(Content::new(
                    ComponentId(TASKS_QUEUE_ROW_BASE + idx as u64),
                    TextContent::new(format!("[{}] {}", q.item_id, q.text)),
                ));
            }
        }
        replace_stack_children(host, TASKS_QUEUE_SECTION_ID, queue_children);

        self.open_targets.clear();
        let mut task_children = Vec::new();
        if model.tasks.is_empty() {
            task_children.push(Content::new(
                ComponentId(TASKS_TASK_ROW_BASE),
                TextContent::new("后台任务 / 子代理: 暂无记录"),
            ));
        } else {
            for (idx, task) in model.tasks.iter().enumerate() {
                let row_id = ComponentId(TASKS_TASK_ROW_BASE + idx as u64);
                let text_id = ComponentId(TASKS_TASK_ROW_BASE + 250 + idx as u64);
                let text = Content::new(
                    text_id,
                    TextContent::new(format!(
                        "[{} · {}] {}",
                        task.canonical_status, task.kind, task.title
                    )),
                );
                let mut row_children = vec![text];
                if task.child_session_id.is_some() {
                    let btn_id = ComponentId(TASKS_TASK_OPEN_BTN_BASE + idx as u64);
                    let mut btn = Button::new("查看子代理");
                    btn.variant = ButtonVariant::Quiet;
                    row_children.push(Content::new(btn_id, btn));
                    self.open_targets.push((btn_id, task.clone()));
                }
                task_children.push(Content::new(
                    row_id,
                    StackContent {
                        model: Stack {
                            axis: Axis::Horizontal,
                            gap: Spacing::Tight.value(),
                        },
                        flexible_child: Some(text_id),
                        children: row_children,
                    },
                ));
            }
        }
        replace_stack_children(host, TASKS_JOBS_SECTION_ID, task_children);
    }

    pub fn handle_actions(
        &self,
        host: &ContentHost,
        model: &TasksQueueViewModel,
        records: &[ContentActionRecord],
    ) -> Vec<TasksQueueIntent> {
        let mut intents = Vec::new();
        let active_item = self
            .selected_queue_item_id
            .as_deref()
            .and_then(|id| model.queued.iter().find(|q| q.item_id == id))
            .or_else(|| model.queued.first());

        for rec in records {
            if !matches!(rec.action.kind, ContentActionKind::Activate) {
                continue;
            }
            if rec.action.target == TASKS_QUEUE_APPLY_BTN_ID
                && let Some(item) = active_item
            {
                let text = host
                    .get(TASKS_QUEUE_EDIT_INPUT_ID)
                    .and_then(|n| n.component::<TextFieldContent>())
                    .map(|f| f.text().to_owned())
                    .unwrap_or_else(|| item.text.clone());
                intents.push(TasksQueueIntent::EditQueued {
                    item_id: item.item_id.clone(),
                    session_id: item.session_id.clone(),
                    epoch: model.epoch,
                    text,
                });
            } else if rec.action.target == TASKS_QUEUE_REMOVE_BTN_ID
                && let Some(item) = active_item
            {
                intents.push(TasksQueueIntent::RemoveQueued {
                    item_id: item.item_id.clone(),
                    session_id: item.session_id.clone(),
                    epoch: model.epoch,
                });
            } else if let Some((_, task)) = self
                .open_targets
                .iter()
                .find(|(btn_id, _)| *btn_id == rec.action.target)
                && let (Some(parent), Some(child_id)) = (&task.parent_scope, &task.child_session_id)
            {
                intents.push(TasksQueueIntent::OpenChildRun {
                    parent_scope: parent.clone(),
                    task_id: task.id.clone(),
                    child_session_id: child_id.clone(),
                    title: task.title.clone(),
                });
            }
        }
        intents
    }
}
