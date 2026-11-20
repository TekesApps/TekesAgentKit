//! `TasksQueueContainer`: supports `OthersTab::List { height }` cards stacked below `SummaryContainer`
//! as well as full `Tab` presentation, with epoch-fenced queue mutations and `OpenChildRun` intents.

use crate::{AgentContainerAdapter, ManagedAgentContainer, SharedAgentWorkbench};
use anyhow::Result;
use tekes_agent_components_ui::{
    QueuedPromptViewModel, ResponsiveWidthMode, TASKS_SHELL_ID, TaskRowViewModel, TasksQueueIntent,
    TasksQueueSurface, TasksQueueViewModel,
};
use tekes_agent_ui_bindings::{
    ChildRunOpenRequest, WorkbenchCommand, project_queued_prompts, project_tasks,
};
use tekes_ui_app_host::{ContainerPresentation, LocalContainer};
use tekes_ui_controls::composition::{ContentActionRecord, ContentHost};

struct TasksQueueAdapter {
    surface: TasksQueueSurface,
}

impl TasksQueueAdapter {
    fn new() -> Self {
        Self {
            surface: TasksQueueSurface::new(),
        }
    }

    fn build_model(workbench: &SharedAgentWorkbench) -> Option<TasksQueueViewModel> {
        let run = workbench.run_binding()?;
        let snapshot = run.run_snapshot();
        let facts = run.summary_facts_for(&snapshot);
        let queued = project_queued_prompts(&snapshot)
            .into_iter()
            .map(|q| QueuedPromptViewModel {
                item_id: q.item_id,
                session_id: q.session_id,
                text: q.text,
                connection_epoch: q.connection_epoch,
                can_mutate: q.can_mutate,
            })
            .collect();
        let tasks = project_tasks(&snapshot, facts.as_ref())
            .into_iter()
            .map(|t| TaskRowViewModel {
                id: t.id,
                title: t.title,
                status: t.status,
                canonical_status: t.canonical_status,
                kind: t.kind,
                parent_scope: t.parent_scope,
                child_session_id: t.child_session_id,
            })
            .collect();
        Some(TasksQueueViewModel {
            epoch: run.connection_epoch(),
            queued,
            tasks,
            error: run.last_error(),
        })
    }
}

impl AgentContainerAdapter for TasksQueueAdapter {
    fn sync_from_workbench(
        &mut self,
        host: &mut ContentHost,
        workbench: &SharedAgentWorkbench,
        width_mode: ResponsiveWidthMode,
    ) {
        if let Some(model) = Self::build_model(workbench) {
            self.surface.sync(host, &model, width_mode);
        }
    }

    fn handle_actions(
        &mut self,
        host: &mut ContentHost,
        workbench: &SharedAgentWorkbench,
        records: Vec<ContentActionRecord>,
    ) {
        let Some(model) = Self::build_model(workbench) else {
            return;
        };
        let Some(run) = workbench.run_binding() else {
            return;
        };
        for intent in self.surface.handle_actions(host, &model, &records) {
            match intent {
                TasksQueueIntent::EditQueued {
                    item_id,
                    session_id,
                    epoch,
                    text,
                } => {
                    let _ = run.edit_queued_prompt_at_epoch(epoch, &session_id, &item_id, text);
                }
                TasksQueueIntent::RemoveQueued {
                    item_id,
                    session_id,
                    epoch,
                } => {
                    let _ = run.remove_queued_prompt_at_epoch(epoch, &session_id, &item_id);
                }
                TasksQueueIntent::OpenChildRun {
                    parent_scope,
                    task_id,
                    child_session_id,
                    title,
                } => {
                    workbench.enqueue_command(WorkbenchCommand::OpenChildRun(
                        ChildRunOpenRequest {
                            scope: parent_scope,
                            task_id,
                            child_session_id,
                            title,
                        },
                    ));
                }
            }
        }
    }
}

pub struct TasksQueueContainer;

impl TasksQueueContainer {
    pub fn build_list_card(
        id: impl Into<String>,
        height: f32,
        workbench: SharedAgentWorkbench,
    ) -> Result<LocalContainer> {
        let container = ManagedAgentContainer::build_local(
            id,
            TASKS_SHELL_ID,
            TasksQueueSurface::build_content(),
            workbench,
            TasksQueueAdapter::new(),
            false,
        )?;
        Ok(container
            .title("Tasks & Queue")
            .switcher_icon("tekes-task")
            .list_height(height))
    }

    pub fn build_tab(
        id: impl Into<String>,
        workbench: SharedAgentWorkbench,
    ) -> Result<LocalContainer> {
        let container = ManagedAgentContainer::build_local(
            id,
            TASKS_SHELL_ID,
            TasksQueueSurface::build_content(),
            workbench,
            TasksQueueAdapter::new(),
            false,
        )?;
        Ok(container
            .title("Tasks & Queue")
            .switcher_icon("tekes-task")
            .presentation(ContainerPresentation::Tab))
    }
}
