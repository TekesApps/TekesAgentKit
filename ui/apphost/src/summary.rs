//! Run Summary container supporting both `OthersTab::List { height }` cards and full `Tab` mode.

use crate::{AgentContainerAdapter, ManagedAgentContainer, SharedAgentWorkbench};
use anyhow::Result;
use tekes_agent_components_ui::{
    ResponsiveWidthMode, SUMMARY_SHELL_ID, SummaryIntent, SummarySurface,
};
use tekes_agent_ui_bindings::{
    ArtifactOpenRequest, ChildRunOpenRequest, SideChatOpenRequest, WorkbenchCommand,
};
use tekes_ui_app_host::{ContainerPresentation, LocalContainer};
use tekes_ui_controls::composition::{ContentActionRecord, ContentHost};

struct SummaryAdapter {
    surface: SummarySurface,
}

impl SummaryAdapter {
    fn new() -> Self {
        Self {
            surface: SummarySurface::new(),
        }
    }
}

impl AgentContainerAdapter for SummaryAdapter {
    fn sync_from_workbench(
        &mut self,
        host: &mut ContentHost,
        workbench: &SharedAgentWorkbench,
        width_mode: ResponsiveWidthMode,
    ) {
        let Some(run) = workbench.run_binding() else {
            return;
        };
        let snapshot = run.run_snapshot();
        let facts = run.summary_facts_for(&snapshot);
        let host_sections = run.host_summary_sections();
        self.surface
            .sync(host, facts.as_ref(), &host_sections, width_mode);
    }

    fn handle_actions(
        &mut self,
        host: &mut ContentHost,
        workbench: &SharedAgentWorkbench,
        records: Vec<ContentActionRecord>,
    ) {
        for intent in self.surface.handle_actions(host, &records) {
            match intent {
                SummaryIntent::OpenArtifact { scope, path } => {
                    workbench.enqueue_command(WorkbenchCommand::OpenArtifact(
                        ArtifactOpenRequest {
                            workspace_id: workbench
                                .active_workspace_id()
                                .unwrap_or_else(|| "ws-main".to_owned()),
                            path,
                            revision: None,
                            line_hint: None,
                            source_scope: Some(scope),
                        },
                    ));
                }
                SummaryIntent::OpenSubagent {
                    parent,
                    child_id,
                    title,
                } => {
                    workbench.enqueue_command(WorkbenchCommand::OpenChildRun(
                        ChildRunOpenRequest {
                            scope: parent,
                            task_id: child_id.clone(),
                            child_session_id: child_id,
                            title,
                        },
                    ));
                }
                SummaryIntent::OpenSideChat { parent, title } => {
                    workbench.enqueue_command(WorkbenchCommand::OpenSideChat(
                        SideChatOpenRequest {
                            parent_scope: parent,
                            sidechat_id: format!("sidechat-{}", uuid_stub()),
                            title,
                        },
                    ));
                }
            }
        }
    }
}

fn uuid_stub() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

pub struct SummaryContainer;

impl SummaryContainer {
    pub fn build_tab(
        id: impl Into<String>,
        workbench: SharedAgentWorkbench,
    ) -> Result<LocalContainer> {
        let container = ManagedAgentContainer::build_local(
            id,
            SUMMARY_SHELL_ID,
            SummarySurface::build_content(),
            workbench,
            SummaryAdapter::new(),
            false,
        )?;
        Ok(container
            .title("运行摘要")
            .switcher_icon("tekes-tab-summary")
            .presentation(ContainerPresentation::Tab))
    }

    pub fn build_list_card(
        id: impl Into<String>,
        height: f32,
        workbench: SharedAgentWorkbench,
    ) -> Result<LocalContainer> {
        let container = ManagedAgentContainer::build_local(
            id,
            SUMMARY_SHELL_ID,
            SummarySurface::build_content(),
            workbench,
            SummaryAdapter::new(),
            false,
        )?;
        Ok(container
            .title("运行摘要")
            .switcher_icon("tekes-tab-summary")
            .list_height(height))
    }
}
