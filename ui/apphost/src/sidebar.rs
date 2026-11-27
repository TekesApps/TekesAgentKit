//! Session Sidebar `AppHost` container emitting navigation entries on selection.

use crate::{AgentContainerAdapter, ManagedAgentContainer, SharedAgentWorkbench};
use anyhow::Result;
use tekes_agent_components_ui::{
    ResponsiveWidthMode, SIDEBAR_SHELL_ID, SidebarFilterSelection, SidebarIntent, SidebarRowModel,
    SidebarSurface,
};
use tekes_agent_contract::SessionAddress;
use tekes_agent_ui_bindings::SidebarFilterMode;
use tekes_ui_app_host::LocalContainer;
use tekes_ui_controls::composition::{ContentActionRecord, ContentHost};

struct SidebarAdapter {
    surface: SidebarSurface,
}

impl SidebarAdapter {
    fn new() -> Self {
        Self {
            surface: SidebarSurface::new(),
        }
    }
}

impl AgentContainerAdapter for SidebarAdapter {
    fn sync_from_workbench(
        &mut self,
        host: &mut ContentHost,
        workbench: &SharedAgentWorkbench,
        width_mode: ResponsiveWidthMode,
    ) {
        let Some(run) = workbench.run_binding() else {
            return;
        };
        let rows: Vec<SidebarRowModel> = run
            .sidebar_rows()
            .into_iter()
            .map(|r| SidebarRowModel {
                session_id: r.session_id,
                title: r.title,
                folder: r.folder,
                activity: r.activity,
                running: r.running,
                pinned: r.pinned,
                archived: r.archived,
            })
            .collect();
        let selected_sid = run.current_scope().map(|s| s.session_id);
        let filter = match run.sidebar_filter() {
            SidebarFilterMode::All => SidebarFilterSelection::All,
            SidebarFilterMode::Pinned => SidebarFilterSelection::Pinned,
        };
        self.surface
            .sync(host, &rows, selected_sid.as_deref(), filter, width_mode);
    }

    fn handle_actions(
        &mut self,
        host: &mut ContentHost,
        workbench: &SharedAgentWorkbench,
        records: Vec<ContentActionRecord>,
    ) {
        let Some(run) = workbench.run_binding() else {
            return;
        };
        let selected_sid = run.current_scope().map(|s| s.session_id);
        for intent in self
            .surface
            .handle_actions(host, selected_sid.as_deref(), &records)
        {
            match intent {
                SidebarIntent::SelectSession(session_id) => {
                    let endpoint_id = run.endpoint_id();
                    run.select_session(&session_id);
                    workbench.set_active_session(SessionAddress {
                        endpoint_id,
                        session_id,
                    });
                }
                SidebarIntent::CreateSession => {
                    run.create_session(&run.default_cwd());
                }
                SidebarIntent::SearchChanged(q) => {
                    run.set_search_query(q);
                }
                SidebarIntent::FilterChanged(mode) => {
                    run.set_sidebar_filter(match mode {
                        SidebarFilterSelection::All => SidebarFilterMode::All,
                        SidebarFilterSelection::Pinned => SidebarFilterMode::Pinned,
                    });
                }
                SidebarIntent::TogglePin(session_id) => {
                    run.toggle_pin(&session_id);
                }
                SidebarIntent::RenameSession { session_id, title } => {
                    run.rename_session(&session_id, title);
                }
                SidebarIntent::ArchiveSession(session_id) => {
                    run.archive_session(&session_id);
                }
                SidebarIntent::UnarchiveSession(session_id) => {
                    run.unarchive_session(&session_id);
                }
                SidebarIntent::OpenSettings => {
                    workbench.bump_revision();
                }
            }
        }
    }
}

pub struct SessionSidebarContainer;

impl SessionSidebarContainer {
    pub fn build(id: impl Into<String>, workbench: SharedAgentWorkbench) -> Result<LocalContainer> {
        let mut container = ManagedAgentContainer::build_local(
            id,
            SIDEBAR_SHELL_ID,
            SidebarSurface::build_content(),
            workbench,
            SidebarAdapter::new(),
            true,
        )?;
        container.with_host(&mut SidebarSurface::bind_context_menu);
        Ok(container
            .switcher_icon("tekes-side-bar")
            .content_padding(0.0))
    }
}
