//! `WorkspaceChangesContainer` (`Working Tree` vs `Turn Changes`) with `(workspace_id, revision)`
//! fencing, unified diff citations, and capability-gated Git commit.

use crate::{
    AgentContainerAdapter, FOCUS_SWITCH_COMMAND, ManagedAgentContainer, SharedAgentWorkbench,
};
use anyhow::Result;
use tekes_agent_components_ui::{
    CHANGES_SHELL_ID, ChangeRowModel, ChangesIntent, ChangesViewModel, ResponsiveWidthMode,
    WorkspaceChangesSurface,
};
use tekes_agent_contract::{ArtifactSelectionReference, Utf16SelectionRange};
use tekes_agent_ui_bindings::{
    ChangesViewMode, ComposerQuoteRequest, WorkbenchCommand, WorkspaceBinding, sha256_prefixed,
};
use tekes_ui_app_host::{ContainerToolbarHost, ContainerToolbarPosition, LocalContainer};
use tekes_ui_app_protocol::ToolbarItem;
use tekes_ui_controls::composition::{ContentActionRecord, ContentHost};

struct WorkspaceChangesAdapter {
    surface: WorkspaceChangesSurface,
}

impl WorkspaceChangesAdapter {
    fn new() -> Self {
        Self {
            surface: WorkspaceChangesSurface::new(),
        }
    }

    fn view_model(ws: &WorkspaceBinding) -> ChangesViewModel {
        let snap = ws.snapshot_changes();
        match snap.mode {
            ChangesViewMode::WorkingTree => ChangesViewModel {
                workspace_id: snap.workspace_id,
                branch: snap.branch,
                git_revision: snap.git_revision,
                mode_index: 0,
                files: snap
                    .working_files
                    .iter()
                    .map(|f| ChangeRowModel {
                        path: f.path.clone(),
                        label: format!("{} (+{} -{})", f.path, f.additions, f.deletions),
                    })
                    .collect(),
                selected_path: snap.selected_path,
                diff_revision: snap.selected_diff_revision,
                diff_text: snap.selected_diff_text,
                can_commit: snap.can_commit,
                commit_status: snap.last_commit_status,
            },
            ChangesViewMode::TurnChanges => {
                let mut files = Vec::new();
                let mut active_diff = String::new();
                let mut rev = snap.git_revision.clone();
                if let Some(tc) = &snap.turn_changes {
                    rev = format!("turn-r{}", tc.revision);
                    for f in &tc.files {
                        let add = f.additions.unwrap_or(0);
                        let del = f.deletions.unwrap_or(0);
                        files.push(ChangeRowModel {
                            path: f.path.clone(),
                            label: format!("[本轮] {} (+{} -{})", f.path, add, del),
                        });
                        if (snap.selected_path.as_deref() == Some(f.path.as_str())
                            || active_diff.is_empty())
                            && let Some(u) = &f.unified_diff
                        {
                            active_diff = u.clone();
                        }
                    }
                }
                ChangesViewModel {
                    workspace_id: snap.workspace_id,
                    branch: snap.branch,
                    git_revision: snap.git_revision,
                    mode_index: 1,
                    files,
                    selected_path: snap.selected_path,
                    diff_revision: rev,
                    diff_text: active_diff,
                    can_commit: snap.can_commit,
                    commit_status: snap.last_commit_status,
                }
            }
        }
    }
}

impl AgentContainerAdapter for WorkspaceChangesAdapter {
    fn sync_from_workbench(
        &mut self,
        host: &mut ContentHost,
        workbench: &SharedAgentWorkbench,
        width_mode: ResponsiveWidthMode,
    ) {
        let Some(ws) = workbench.workspace_binding() else {
            return;
        };
        let model = Self::view_model(&ws);
        self.surface.sync(host, &model, width_mode);
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
        let model = Self::view_model(&ws);
        let active_session = workbench
            .run_binding()
            .and_then(|r| r.current_scope())
            .map(|s| s.session_id);

        for intent in self.surface.handle_actions(host, &model, &records) {
            match intent {
                ChangesIntent::SelectMode(idx) => {
                    let mode = if idx == 1 {
                        ChangesViewMode::TurnChanges
                    } else {
                        ChangesViewMode::WorkingTree
                    };
                    ws.set_changes_mode(mode, active_session.as_deref());
                }
                ChangesIntent::SelectFile(path) => {
                    ws.select_change_and_load_diff(path);
                }
                ChangesIntent::QuoteDiff {
                    path,
                    revision,
                    excerpt,
                } => {
                    let digest = sha256_prefixed(excerpt.as_bytes());
                    let utf16_len = excerpt.encode_utf16().count().min(120) as u64;
                    if let Ok(reference) = ArtifactSelectionReference::capture(
                        format!("{}:{path}", ws.workspace_id()),
                        path.clone(),
                        "git-diff".to_owned(),
                        vec!["diff".into()],
                        revision,
                        digest,
                        &excerpt,
                        Utf16SelectionRange {
                            location: 0,
                            length: utf16_len,
                        },
                        32,
                        32,
                    ) {
                        let req = ComposerQuoteRequest {
                            destination: workbench
                                .run_binding()
                                .and_then(|r| r.current_destination()),
                            reference,
                            comment: format!("引用 {path} 变更 Diff"),
                        };
                        if let Some(run) = workbench.run_binding() {
                            workbench.set_latest_quote(req.reference.clone());
                            run.apply_quote(req);
                        } else {
                            workbench.enqueue_command(WorkbenchCommand::QuoteToComposer(req));
                        }
                    }
                }
                ChangesIntent::Commit { message } => {
                    let _ = ws.commit_selected(&message);
                }
            }
        }
    }
}

pub struct WorkspaceChangesContainer;

impl WorkspaceChangesContainer {
    pub fn build(id: impl Into<String>, workbench: SharedAgentWorkbench) -> Result<LocalContainer> {
        let container = ManagedAgentContainer::build_local(
            id,
            CHANGES_SHELL_ID,
            WorkspaceChangesSurface::build_content(),
            workbench,
            WorkspaceChangesAdapter::new(),
            false,
        )?;
        Ok(container
            .title("变更与 Diff")
            .switcher_icon("tekes-tab-diffs")
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
