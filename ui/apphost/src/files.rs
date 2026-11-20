//! `WorkspaceFilesContainer` (`Others` or `Focus`-swappable) using `WorkspaceFilesSurface` and
//! `WorkspaceBinding`, routing open/attach intents across containers via `SharedAgentWorkbench`.

use crate::{
    AgentContainerAdapter, FOCUS_SWITCH_COMMAND, ManagedAgentContainer, SharedAgentWorkbench,
};
use anyhow::Result;
use tekes_agent_components_ui::{
    FILES_SHELL_ID, FileTreeRowModel, FilesIntent, ResponsiveWidthMode, WorkspaceFilesSurface,
};
use tekes_agent_ui_bindings::{ArtifactOpenRequest, WorkbenchCommand};
use tekes_ui_app_host::{ContainerToolbarHost, ContainerToolbarPosition, LocalContainer};
use tekes_ui_app_protocol::ToolbarItem;
use tekes_ui_controls::composition::{ContentActionRecord, ContentHost};

struct WorkspaceFilesAdapter {
    surface: WorkspaceFilesSurface,
}

impl WorkspaceFilesAdapter {
    fn new() -> Self {
        Self {
            surface: WorkspaceFilesSurface::new(),
        }
    }
}

impl AgentContainerAdapter for WorkspaceFilesAdapter {
    fn sync_from_workbench(
        &mut self,
        host: &mut ContentHost,
        workbench: &SharedAgentWorkbench,
        width_mode: ResponsiveWidthMode,
    ) {
        let Some(ws) = workbench.workspace_binding() else {
            return;
        };
        let items: Vec<FileTreeRowModel> = ws
            .visible_tree_items()
            .into_iter()
            .map(|i| FileTreeRowModel {
                path: i.path,
                parent_path: i.parent_path,
                name: i.name,
                is_dir: i.is_dir,
            })
            .collect();
        let selected = ws.selected_file();
        self.surface.sync(
            host,
            &ws.workspace_id(),
            &items,
            selected.as_deref(),
            width_mode,
        );
    }

    fn handle_actions(
        &mut self,
        host: &mut ContentHost,
        workbench: &SharedAgentWorkbench,
        records: Vec<ContentActionRecord>,
    ) {
        let Some(ws) = workbench.workspace_binding() else {
            return;
        };
        let selected = ws.selected_file();
        for intent in self
            .surface
            .handle_actions(host, selected.as_deref(), &records)
        {
            match intent {
                FilesIntent::SelectFile(path) => {
                    ws.select_file(Some(path));
                }
                FilesIntent::OpenFile(path) => {
                    ws.select_file(Some(path.clone()));
                    ws.load_artifact_blocking(&path);
                    workbench.set_artifact_target("default", path.clone());
                    workbench.enqueue_command(WorkbenchCommand::OpenArtifact(
                        ArtifactOpenRequest {
                            workspace_id: ws.workspace_id(),
                            path,
                            revision: None,
                            line_hint: None,
                            source_scope: workbench.run_binding().and_then(|r| r.current_scope()),
                        },
                    ));
                }
                FilesIntent::AttachFile(path) => {
                    if let Some(run) = workbench.run_binding() {
                        run.attach_file_path(None, path);
                    } else {
                        workbench.enqueue_command(WorkbenchCommand::AttachFileToComposer {
                            destination: None,
                            path,
                        });
                    }
                }
                FilesIntent::SearchChanged(q) => {
                    ws.set_file_search_query(q);
                }
                FilesIntent::Refresh => {
                    let _ = ws.refresh_all_blocking();
                }
            }
        }
    }
}

pub struct WorkspaceFilesContainer;

impl WorkspaceFilesContainer {
    pub fn build(id: impl Into<String>, workbench: SharedAgentWorkbench) -> Result<LocalContainer> {
        let container = ManagedAgentContainer::build_local(
            id,
            FILES_SHELL_ID,
            WorkspaceFilesSurface::build_content(),
            workbench,
            WorkspaceFilesAdapter::new(),
            false,
        )?;
        Ok(container
            .title("工作区文件")
            .switcher_icon("tekes-files")
            .toolbar_host(ContainerToolbarHost::Container {
                position: ContainerToolbarPosition::Top,
            })
            .toolbar(vec![ToolbarItem {
                id: FOCUS_SWITCH_COMMAND,
                title: "Focus 切换".into(),
                enabled: true,
                geometry_dependent: false,
            }])
            .focus_switch(FOCUS_SWITCH_COMMAND))
    }
}
