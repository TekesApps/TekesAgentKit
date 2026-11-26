//! Isolated single-component showcase & `--component <name>` launcher for `tekes-agent-ui-demo`.
//! Allows inspecting each AgentKit surface (`Transcript`, `Composer`, `AgentRun`, `Sidebar`,
//! `Summary`, `Files`, `Changes`, `Tasks`, `Settings`) independently against live fixture state.

use crate::workbench_app::{TopologyALaunchOptions, TopologyARuntime};
use anyhow::{Result, bail};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;
use tekes_agent_apphost::{
    AgentContainerAdapter, AgentRunContainer, ComposerContainer, HarnessSettingsContainer,
    ManagedAgentContainer, SessionSidebarContainer, SharedAgentWorkbench, SummaryContainer,
    TasksQueueContainer, TranscriptContainer, WorkspaceChangesContainer, WorkspaceFilesContainer,
    build_agent_settings_owner,
};
use tekes_agent_components_ui::{
    ChangeRowModel, ChangesIntent, ChangesViewModel, ComposerIntent, ComposerSurface,
    FileTreeRowModel, FilesIntent, HarnessSettingsIntent, HarnessSettingsSurface,
    HarnessSettingsViewModel, QueuedPromptViewModel, ResponsiveWidthMode, SidebarFilterSelection,
    SidebarIntent, SidebarRowModel, SidebarSurface, SummaryIntent, SummarySurface,
    TaskRowViewModel, TasksQueueIntent, TasksQueueSurface, TasksQueueViewModel, TranscriptIntent,
    TranscriptSurface, WorkspaceChangesSurface, WorkspaceFilesSurface, replace_stack_children,
};
use tekes_agent_contract::SessionAddress;
use tekes_agent_presentation::ActionablePrompt;
use tekes_agent_ui_bindings::{
    ArtifactOpenRequest, ChangesViewMode, ChildRunOpenRequest, HarnessSettingKind,
    HarnessSettingsBinding, SidebarFilterMode, UiWakeNotifier, WorkbenchCommand,
    project_queued_prompts, project_tasks,
};
use tekes_ui_app_host::{
    AppWindow, ContainerToolbarHost, LocalContainer, LocalToolbarAction, LocalToolbarItem,
    PresentationOptions,
};
use tekes_ui_controls::{
    ScrollArea,
    composition::{
        ComponentId, Content, ContentActionPayload, ContentActionRecord, ContentHost, IconContent,
        ListDisplayUpdate, ListIntentKind, ListRowDisplay, ScrollContent, SelectableListContent,
        StackContent, TextContent,
    },
    extended::{KeyedListItem, KeyedListSnapshot, ListBudget, ListDatasetKey, ListItemKey},
    layout::Insets,
};
use tekes_ui_tokens::Spacing;

const SHOWCASE_NAV_ROOT_ID: ComponentId = ComponentId(8001);
const SHOWCASE_NAV_TITLE_ID: ComponentId = ComponentId(8002);
const SHOWCASE_NAV_SCROLL_ID: ComponentId = ComponentId(8003);
const SHOWCASE_NAV_LIST_ID: ComponentId = ComponentId(8004);
const SHOWCASE_NAV_ROW_BASE: u64 = 8200;

const SHOWCASE_STAGE_SHELL_ID: ComponentId = ComponentId(8100);
const SHOWCASE_STAGE_ROOT_ID: ComponentId = ComponentId(8101);
const SHOWCASE_STAGE_PAD_ID: ComponentId = ComponentId(8102);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShowcaseComponent {
    Transcript,
    Composer,
    AgentRun,
    Sidebar,
    Summary,
    Files,
    Changes,
    Tasks,
    Settings,
}

impl ShowcaseComponent {
    pub const ALL: [Self; 9] = [
        Self::Transcript,
        Self::Composer,
        Self::AgentRun,
        Self::Sidebar,
        Self::Summary,
        Self::Files,
        Self::Changes,
        Self::Tasks,
        Self::Settings,
    ];

    pub fn from_name(raw: &str) -> Result<Self> {
        let norm = raw.trim().to_ascii_lowercase();
        match norm.as_str() {
            "transcript" => Ok(Self::Transcript),
            "composer" => Ok(Self::Composer),
            "run" | "agent-run" | "agent_run" | "conversation" => Ok(Self::AgentRun),
            "sidebar" | "nav" => Ok(Self::Sidebar),
            "summary" => Ok(Self::Summary),
            "files" => Ok(Self::Files),
            "changes" | "review" => Ok(Self::Changes),
            "tasks" | "queue" => Ok(Self::Tasks),
            "settings" => Ok(Self::Settings),
            _ => bail!(
                "unknown --component `{raw}`; expected one of: transcript, composer, run, sidebar, summary, files, changes, tasks, settings"
            ),
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Transcript => "transcript",
            Self::Composer => "composer",
            Self::AgentRun => "run",
            Self::Sidebar => "sidebar",
            Self::Summary => "summary",
            Self::Files => "files",
            Self::Changes => "changes",
            Self::Tasks => "tasks",
            Self::Settings => "settings",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Transcript => "Transcript",
            Self::Composer => "Composer",
            Self::AgentRun => "Agent Run",
            Self::Sidebar => "Sidebar",
            Self::Summary => "Summary",
            Self::Files => "Files",
            Self::Changes => "Review / Changes",
            Self::Tasks => "Tasks & Queue",
            Self::Settings => "Harness Settings",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Transcript => "tekes-side-chat",
            Self::Composer => "tekes-send",
            Self::AgentRun => "tekes-flash",
            Self::Sidebar => "tekes-side-bar",
            Self::Summary => "tekes-tab-summary",
            Self::Files => "tekes-files",
            Self::Changes => "tekes-tab-diffs",
            Self::Tasks => "tekes-task",
            Self::Settings => "tekes-settings",
        }
    }

    fn list_key(self) -> ListItemKey {
        let idx = Self::ALL.iter().position(|c| *c == self).unwrap_or(0) as u64;
        ListItemKey(idx + 1)
    }

    fn from_list_key(key: ListItemKey) -> Option<Self> {
        key.0
            .checked_sub(1)
            .and_then(|idx| Self::ALL.get(idx as usize).copied())
    }
}

enum MountedSurface {
    Transcript(Box<TranscriptSurface>),
    Composer(Box<ComposerSurface>),
    AgentRun(Box<TranscriptSurface>, Box<ComposerSurface>),
    Sidebar(SidebarSurface),
    Summary(SummarySurface),
    Files(WorkspaceFilesSurface),
    Changes(WorkspaceChangesSurface),
    Tasks(TasksQueueSurface),
    Settings {
        surface: HarnessSettingsSurface,
        initialized: bool,
    },
}

impl MountedSurface {
    fn kind(&self) -> ShowcaseComponent {
        match self {
            Self::Transcript(_) => ShowcaseComponent::Transcript,
            Self::Composer(_) => ShowcaseComponent::Composer,
            Self::AgentRun(_, _) => ShowcaseComponent::AgentRun,
            Self::Sidebar(_) => ShowcaseComponent::Sidebar,
            Self::Summary(_) => ShowcaseComponent::Summary,
            Self::Files(_) => ShowcaseComponent::Files,
            Self::Changes(_) => ShowcaseComponent::Changes,
            Self::Tasks(_) => ShowcaseComponent::Tasks,
            Self::Settings { .. } => ShowcaseComponent::Settings,
        }
    }

    fn build(component: ShowcaseComponent) -> (Self, Content, Option<ComponentId>) {
        match component {
            ShowcaseComponent::Transcript => (
                Self::Transcript(Box::new(TranscriptSurface::new())),
                TranscriptSurface::build_content(),
                Some(tekes_agent_components_ui::TRANSCRIPT_ROOT_ID),
            ),
            ShowcaseComponent::Composer => {
                let padded = StackContent::vertical(0.0, vec![ComposerSurface::build_content()])
                    .into_content_with_padding(
                        SHOWCASE_STAGE_PAD_ID,
                        Insets::new(24.0, 0.0, 12.0, 0.0),
                    );
                (
                    Self::Composer(Box::new(ComposerSurface::new())),
                    padded,
                    None,
                )
            }
            ShowcaseComponent::AgentRun => {
                let stack = Content::new(
                    tekes_agent_components_ui::RUN_CONTAINER_STACK_ID,
                    StackContent::vertical(
                        Spacing::Regular.value(),
                        vec![
                            TranscriptSurface::build_content(),
                            ComposerSurface::build_content(),
                        ],
                    )
                    .with_flexible_child(tekes_agent_components_ui::TRANSCRIPT_ROOT_ID),
                );
                (
                    Self::AgentRun(
                        Box::new(TranscriptSurface::new()),
                        Box::new(ComposerSurface::new()),
                    ),
                    stack,
                    Some(tekes_agent_components_ui::RUN_CONTAINER_STACK_ID),
                )
            }
            ShowcaseComponent::Sidebar => (
                Self::Sidebar(SidebarSurface::new()),
                SidebarSurface::build_content(),
                Some(tekes_agent_components_ui::SIDEBAR_ROOT_ID),
            ),
            ShowcaseComponent::Summary => (
                Self::Summary(SummarySurface::new()),
                SummarySurface::build_content(),
                Some(tekes_agent_components_ui::SUMMARY_SCROLL_ID),
            ),
            ShowcaseComponent::Files => {
                let padded =
                    StackContent::vertical(0.0, vec![WorkspaceFilesSurface::build_content()])
                        .with_flexible_child(tekes_agent_components_ui::FILES_ROOT_ID)
                        .into_content_with_padding(
                            SHOWCASE_STAGE_PAD_ID,
                            Insets::new(16.0, 16.0, 16.0, 16.0),
                        );
                (
                    Self::Files(WorkspaceFilesSurface::new()),
                    padded,
                    Some(SHOWCASE_STAGE_PAD_ID),
                )
            }
            ShowcaseComponent::Changes => {
                let padded =
                    StackContent::vertical(0.0, vec![WorkspaceChangesSurface::build_content()])
                        .with_flexible_child(tekes_agent_components_ui::CHANGES_ROOT_ID)
                        .into_content_with_padding(
                            SHOWCASE_STAGE_PAD_ID,
                            Insets::new(16.0, 16.0, 16.0, 16.0),
                        );
                (
                    Self::Changes(WorkspaceChangesSurface::new()),
                    padded,
                    Some(SHOWCASE_STAGE_PAD_ID),
                )
            }
            ShowcaseComponent::Tasks => {
                let padded = StackContent::vertical(0.0, vec![TasksQueueSurface::build_content()])
                    .with_flexible_child(tekes_agent_components_ui::TASKS_ROOT_ID)
                    .into_content_with_padding(
                        SHOWCASE_STAGE_PAD_ID,
                        Insets::new(16.0, 16.0, 16.0, 16.0),
                    );
                (
                    Self::Tasks(TasksQueueSurface::new()),
                    padded,
                    Some(SHOWCASE_STAGE_PAD_ID),
                )
            }
            ShowcaseComponent::Settings => {
                let padded =
                    StackContent::vertical(0.0, vec![HarnessSettingsSurface::build_content()])
                        .with_flexible_child(tekes_agent_components_ui::SETTINGS_ROOT_ID)
                        .into_content_with_padding(
                            SHOWCASE_STAGE_PAD_ID,
                            Insets::new(16.0, 16.0, 16.0, 16.0),
                        );
                (
                    Self::Settings {
                        surface: HarnessSettingsSurface::new(),
                        initialized: false,
                    },
                    padded,
                    Some(SHOWCASE_STAGE_PAD_ID),
                )
            }
        }
    }
}

struct ShowcaseStageAdapter {
    selected: Rc<RefCell<ShowcaseComponent>>,
    mounted: Option<MountedSurface>,
}

impl ShowcaseStageAdapter {
    fn new(selected: Rc<RefCell<ShowcaseComponent>>) -> Self {
        Self {
            selected,
            mounted: None,
        }
    }

    fn ensure_mounted(&mut self, host: &mut ContentHost) -> &mut MountedSurface {
        let desired = *self.selected.borrow();
        let needs_swap = self
            .mounted
            .as_ref()
            .is_none_or(|current| current.kind() != desired);
        if needs_swap {
            let (surface, child, flex_id) = MountedSurface::build(desired);
            if let Some(root) = host
                .get_mut(SHOWCASE_STAGE_ROOT_ID)
                .and_then(|c| c.component_mut::<StackContent>())
            {
                root.set_flexible_child(flex_id);
            }
            replace_stack_children(host, SHOWCASE_STAGE_ROOT_ID, vec![child]);
            if matches!(desired, ShowcaseComponent::Sidebar) {
                SidebarSurface::bind_context_menu(host);
            } else if matches!(desired, ShowcaseComponent::Files) {
                WorkspaceFilesSurface::bind_context_menu(host);
            }
            self.mounted = Some(surface);
        }
        self.mounted.as_mut().expect("mounted showcase surface")
    }
}

impl AgentContainerAdapter for ShowcaseStageAdapter {
    fn sync_from_workbench(
        &mut self,
        host: &mut ContentHost,
        workbench: &SharedAgentWorkbench,
        width_mode: ResponsiveWidthMode,
    ) {
        workbench.pump_composer_requests();
        let mounted = self.ensure_mounted(host);
        match mounted {
            MountedSurface::Transcript(transcript) => {
                if let Some(run) = workbench.run_binding() {
                    let snapshot = run.run_snapshot();
                    let actionables: Vec<ActionablePrompt> = run.actionables_for(&snapshot);
                    transcript.sync(host, &snapshot, &actionables, width_mode);
                }
            }
            MountedSurface::Composer(composer) => {
                if let Some(run) = workbench.run_binding() {
                    let snapshot = run.run_snapshot();
                    composer.sync(host, &snapshot, width_mode);
                }
            }
            MountedSurface::AgentRun(transcript, composer) => {
                if let Some(run) = workbench.run_binding() {
                    let snapshot = run.run_snapshot();
                    let actionables: Vec<ActionablePrompt> = run.actionables_for(&snapshot);
                    transcript.sync(host, &snapshot, &actionables, width_mode);
                    composer.sync(host, &snapshot, width_mode);
                }
            }
            MountedSurface::Sidebar(sidebar) => {
                if let Some(run) = workbench.run_binding() {
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
                    sidebar.sync(host, &rows, selected_sid.as_deref(), filter, width_mode);
                }
            }
            MountedSurface::Summary(summary) => {
                if let Some(run) = workbench.run_binding() {
                    let snapshot = run.run_snapshot();
                    let facts = run.summary_facts_for(&snapshot);
                    let host_sections = run.host_summary_sections();
                    summary.sync(host, facts.as_ref(), &host_sections, width_mode);
                }
            }
            MountedSurface::Files(files) => {
                if let Some(ws) = workbench.workspace_binding() {
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
                    files.sync(
                        host,
                        &ws.workspace_id(),
                        &items,
                        selected.as_deref(),
                        width_mode,
                    );
                }
            }
            MountedSurface::Changes(changes) => {
                if let Some(ws) = workbench.workspace_binding() {
                    let model = build_changes_model(&ws);
                    changes.sync(host, &model, width_mode);
                }
            }
            MountedSurface::Tasks(tasks) => {
                if let Some(model) = build_tasks_model(workbench) {
                    tasks.sync(host, &model, width_mode);
                }
            }
            MountedSurface::Settings {
                surface,
                initialized,
            } => {
                let binding = if let Some(existing) = workbench.harness_settings_binding() {
                    Some(existing)
                } else if !*initialized {
                    *initialized = true;
                    workbench.run_binding().map(|run| {
                        let wake: Arc<dyn UiWakeNotifier> = workbench.wake_hub();
                        let binding = HarnessSettingsBinding::new(
                            Arc::clone(run.runtime()),
                            run.handle().clone(),
                            wake,
                        );
                        binding.refresh();
                        workbench.attach_harness_settings_binding(Arc::clone(&binding));
                        binding
                    })
                } else {
                    None
                };
                if let Some(binding) = binding {
                    let model = build_settings_model(&binding);
                    surface.sync(host, &model, width_mode);
                }
            }
        }
    }

    fn handle_actions(
        &mut self,
        host: &mut ContentHost,
        workbench: &SharedAgentWorkbench,
        records: Vec<ContentActionRecord>,
    ) {
        let Some(mounted) = self.mounted.as_mut() else {
            return;
        };
        match mounted {
            MountedSurface::Transcript(transcript) => {
                if let Some(run) = workbench.run_binding() {
                    let snapshot = run.run_snapshot();
                    let intents = transcript.handle_actions(host, &snapshot, &records);
                    apply_transcript_intents(workbench, &run, &snapshot, intents);
                }
            }
            MountedSurface::Composer(composer) => {
                if let Some(run) = workbench.run_binding() {
                    let snapshot = run.run_snapshot();
                    let intents = composer.handle_actions(host, &snapshot, &records);
                    apply_composer_intents(&run, intents);
                }
            }
            MountedSurface::AgentRun(transcript, composer) => {
                if let Some(run) = workbench.run_binding() {
                    let snapshot = run.run_snapshot();
                    let t_intents = transcript.handle_actions(host, &snapshot, &records);
                    apply_transcript_intents(workbench, &run, &snapshot, t_intents);
                    let c_intents = composer.handle_actions(host, &snapshot, &records);
                    apply_composer_intents(&run, c_intents);
                }
            }
            MountedSurface::Sidebar(sidebar) => {
                if let Some(run) = workbench.run_binding() {
                    let selected_sid = run.current_scope().map(|s| s.session_id);
                    for intent in sidebar.handle_actions(host, selected_sid.as_deref(), &records) {
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
                                run.create_session("/fixture/tekes-agentkit");
                            }
                            SidebarIntent::SearchChanged(q) => run.set_search_query(q),
                            SidebarIntent::FilterChanged(mode) => {
                                run.set_sidebar_filter(match mode {
                                    SidebarFilterSelection::All => SidebarFilterMode::All,
                                    SidebarFilterSelection::Pinned => SidebarFilterMode::Pinned,
                                });
                            }
                            SidebarIntent::TogglePin(sid) => run.toggle_pin(&sid),
                            SidebarIntent::RenameSession { session_id, title } => {
                                run.rename_session(&session_id, title);
                            }
                            SidebarIntent::ArchiveSession(sid) => run.archive_session(&sid),
                            SidebarIntent::OpenSettings => {
                                *self.selected.borrow_mut() = ShowcaseComponent::Settings;
                                workbench.bump_revision();
                            }
                        }
                    }
                }
            }
            MountedSurface::Summary(summary) => {
                for intent in summary.handle_actions(host, &records) {
                    if let SummaryIntent::OpenArtifact { scope, path } = intent {
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
                }
            }
            MountedSurface::Files(files) => {
                if let Some(ws) = workbench.workspace_binding() {
                    let selected = ws.selected_file();
                    for intent in files.handle_actions(host, selected.as_deref(), &records) {
                        match intent {
                            FilesIntent::SelectFile(path) | FilesIntent::OpenFile(path) => {
                                ws.select_file(Some(path));
                            }
                            FilesIntent::AttachFile(path) => {
                                if let Some(run) = workbench.run_binding() {
                                    run.attach_file_path(None, path);
                                }
                            }
                            FilesIntent::SearchChanged(q) => ws.set_file_search_query(q),
                            FilesIntent::Refresh => {
                                let _ = ws.refresh_all_blocking();
                            }
                        }
                    }
                }
            }
            MountedSurface::Changes(changes) => {
                if let Some(ws) = workbench.workspace_binding() {
                    let model = build_changes_model(&ws);
                    let active_session = workbench
                        .run_binding()
                        .and_then(|r| r.current_scope())
                        .map(|s| s.session_id);
                    for intent in changes.handle_actions(host, &model, &records) {
                        match intent {
                            ChangesIntent::SelectMode(idx) => {
                                ws.set_changes_mode(
                                    if idx == 1 {
                                        ChangesViewMode::TurnChanges
                                    } else {
                                        ChangesViewMode::WorkingTree
                                    },
                                    active_session.as_deref(),
                                );
                            }
                            ChangesIntent::SelectFile(path) => ws.select_change_and_load_diff(path),
                            ChangesIntent::Commit { message } => {
                                let _ = ws.commit_selected(&message);
                            }
                            ChangesIntent::QuoteDiff { .. } => {}
                        }
                    }
                }
            }
            MountedSurface::Tasks(tasks) => {
                if let (Some(model), Some(run)) =
                    (build_tasks_model(workbench), workbench.run_binding())
                {
                    for intent in tasks.handle_actions(host, &model, &records) {
                        match intent {
                            TasksQueueIntent::EditQueued {
                                item_id,
                                session_id,
                                epoch,
                                text,
                            } => {
                                let _ = run.edit_queued_prompt_at_epoch(
                                    epoch,
                                    &session_id,
                                    &item_id,
                                    text,
                                );
                            }
                            TasksQueueIntent::RemoveQueued {
                                item_id,
                                session_id,
                                epoch,
                            } => {
                                let _ =
                                    run.remove_queued_prompt_at_epoch(epoch, &session_id, &item_id);
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
            MountedSurface::Settings { surface, .. } => {
                if let Some(binding) = workbench.harness_settings_binding() {
                    for intent in surface.handle_actions(host, &records) {
                        match intent {
                            HarnessSettingsIntent::Refresh => binding.refresh(),
                            HarnessSettingsIntent::CycleEnum { key, next_value } => {
                                let mut patch = BTreeMap::new();
                                patch.insert(key, Value::String(next_value));
                                binding.apply_patch_async(patch);
                            }
                            HarnessSettingsIntent::ToggleBool { key, next_value } => {
                                let mut patch = BTreeMap::new();
                                patch.insert(key, Value::Bool(next_value));
                                binding.apply_patch_async(patch);
                            }
                            HarnessSettingsIntent::IncrementInt { key, next_value } => {
                                let mut patch = BTreeMap::new();
                                patch.insert(key, serde_json::json!(next_value));
                                binding.apply_patch_async(patch);
                            }
                        }
                    }
                }
            }
        }
    }

    fn toolbar_items(&mut self, workbench: &SharedAgentWorkbench) -> Option<Vec<LocalToolbarItem>> {
        matches!(*self.selected.borrow(), ShowcaseComponent::AgentRun)
            .then(|| AgentRunContainer::build_toolbar_items(workbench))
    }

    fn handle_toolbar_action(
        &mut self,
        _host: &mut ContentHost,
        workbench: &SharedAgentWorkbench,
        action: &LocalToolbarAction,
    ) -> Result<bool> {
        if !matches!(*self.selected.borrow(), ShowcaseComponent::AgentRun) {
            return Ok(false);
        }
        Ok(AgentRunContainer::handle_toolbar_action(workbench, action))
    }
}

fn apply_transcript_intents(
    workbench: &SharedAgentWorkbench,
    run: &tekes_agent_ui_bindings::RunWorkbenchBinding,
    snapshot: &tekes_agent_presentation::RunSnapshot,
    intents: Vec<TranscriptIntent>,
) {
    for intent in intents {
        match intent {
            TranscriptIntent::LoadEarlier(scope) => run.load_earlier(&scope),
            TranscriptIntent::RespondActionable {
                scope,
                actionable_id,
                revision,
                choice_index,
            } => {
                let actionables = run.actionables_for(snapshot);
                if let Some(prompt) = actionables.iter().find(|p| p.id() == actionable_id)
                    && let Some(choice) = prompt.choices.get(choice_index)
                {
                    run.respond_actionable(
                        &scope,
                        &actionable_id,
                        revision,
                        choice.outcome().clone(),
                    );
                }
            }
            TranscriptIntent::Reconnect => run.reconnect(),
            TranscriptIntent::OpenLink(_)
            | TranscriptIntent::CopyLatest
            | TranscriptIntent::FeedbackLatest { .. }
            | TranscriptIntent::ForkLatest => {
                workbench.bump_revision();
            }
        }
    }
}

fn apply_composer_intents(
    run: &tekes_agent_ui_bindings::RunWorkbenchBinding,
    intents: Vec<ComposerIntent>,
) {
    for intent in intents {
        match intent {
            ComposerIntent::EditText { destination, text } => {
                run.set_draft_text(&destination, text)
            }
            ComposerIntent::Send { destination } => run.submit_destination(&destination),
            ComposerIntent::Cancel {
                scope,
                connection_epoch,
            } => run.cancel_scope_at_epoch(connection_epoch, &scope),
            ComposerIntent::Reconnect => run.reconnect(),
            ComposerIntent::AttachDemoFile { destination } => {
                run.attach_file_path(
                    Some(destination),
                    "docs/tekesui-apphost-architecture.md".to_owned(),
                );
            }
            ComposerIntent::RemoveArtifact {
                destination,
                artifact_id,
            } => run.remove_artifact(&destination, &artifact_id),
            ComposerIntent::RemoveAnnotation {
                destination,
                annotation_id,
            } => run.remove_annotation(&destination, &annotation_id),
            ComposerIntent::RemoveQueued {
                item_id,
                session_id,
                epoch,
            } => {
                let _ = run.remove_queued_prompt_at_epoch(epoch, &session_id, &item_id);
            }
        }
    }
}

fn build_changes_model(ws: &tekes_agent_ui_bindings::WorkspaceBinding) -> ChangesViewModel {
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

fn build_tasks_model(workbench: &SharedAgentWorkbench) -> Option<TasksQueueViewModel> {
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
    let active_session = run.current_scope().map(|scope| scope.session_id);
    let error = run.error_for_session(active_session.as_deref());
    Some(TasksQueueViewModel {
        epoch: run.connection_epoch(),
        queued,
        tasks,
        error,
    })
}

fn build_settings_model(binding: &HarnessSettingsBinding) -> HarnessSettingsViewModel {
    let snap = binding.snapshot();
    let fields = snap
        .fields
        .into_iter()
        .map(|f| SettingFieldModelAlias {
            key: f.key,
            label: f.label,
            kind: match f.kind {
                HarnessSettingKind::Enum(opts) => SettingFieldKindAlias::Enum(opts),
                HarnessSettingKind::Integer => SettingFieldKindAlias::Integer,
                HarnessSettingKind::Boolean => SettingFieldKindAlias::Boolean,
                HarnessSettingKind::String => SettingFieldKindAlias::String,
                HarnessSettingKind::Secret => SettingFieldKindAlias::Secret,
                HarnessSettingKind::ReadOnly => SettingFieldKindAlias::ReadOnly,
            },
            display_value: f.display_value,
            writable: f.writable,
            is_secret: f.is_secret,
        })
        .collect();
    HarnessSettingsViewModel {
        loaded: snap.loaded,
        namespace_label: snap.namespace_label,
        revision: snap.revision,
        read_only: snap.read_only,
        fields,
        status: snap.last_status,
        error: snap.last_error,
        conflict_revision: snap.conflict_revision,
    }
}

use tekes_agent_components_ui::{
    SettingFieldKindModel as SettingFieldKindAlias, SettingFieldModel as SettingFieldModelAlias,
};

fn build_showcase_nav(
    selected: Rc<RefCell<ShowcaseComponent>>,
    workbench: SharedAgentWorkbench,
) -> Result<LocalContainer> {
    let budget = ListBudget {
        max_items: 64,
        max_display_items: 64,
        max_utf8_bytes: 64_000,
        max_staging_units: 8_000,
    };
    let mut list_items = Vec::new();
    let mut display_items = Vec::new();
    let mut display_rows = Vec::new();
    for (idx, comp) in ShowcaseComponent::ALL.iter().enumerate() {
        let key = comp.list_key();
        list_items.push(KeyedListItem {
            key,
            name: comp.label().to_owned(),
        });
        display_items.push(key);
        let base = SHOWCASE_NAV_ROW_BASE + (idx as u64) * 8;
        let icon = IconContent::new(comp.icon())
            .unwrap_or_else(|_| IconContent::new("tekes-file-generic").expect("fallback icon"));
        let row = Content::new(
            ComponentId(base + 1),
            StackContent::horizontal(
                Spacing::Tight.value(),
                vec![
                    Content::new(ComponentId(base + 2), icon),
                    Content::new(ComponentId(base + 3), TextContent::new(comp.label())),
                ],
            )
            .with_flexible_child(ComponentId(base + 3)),
        );
        display_rows.push(ListRowDisplay {
            key,
            row_id: ComponentId(base),
            display: ListDisplayUpdate::Replace(row),
        });
    }
    let snap = KeyedListSnapshot {
        dataset: ListDatasetKey(1),
        items: list_items,
        display_items,
    };
    let mut list = SelectableListContent::keyed("Components", &snap, &mut display_rows, budget)
        .map_err(|err| anyhow::anyhow!("showcase nav list init failed: {err:?}"))?;
    list.set_navigation_layout(true);
    list.set_navigation_selection_fill_only(true);
    let initial_key = selected.borrow().list_key();
    let _ = list.select_key(initial_key);

    let root = StackContent::vertical(
        Spacing::Regular.value(),
        vec![
            Content::new(SHOWCASE_NAV_TITLE_ID, TextContent::heading("Components")),
            Content::new(
                SHOWCASE_NAV_SCROLL_ID,
                ScrollContent::new(
                    ScrollArea::default(),
                    Content::new(SHOWCASE_NAV_LIST_ID, list),
                )
                .scrollbar(true),
            ),
        ],
    )
    .with_flexible_child(SHOWCASE_NAV_SCROLL_ID)
    .into_content_with_padding(SHOWCASE_NAV_ROOT_ID, Insets::new(12.0, 12.0, 12.0, 12.0));

    let sel_action = Rc::clone(&selected);
    let wb_action = workbench;
    Ok(LocalContainer::new("showcase-nav", root)?
        .switcher_icon("tekes-side-bar")
        .content_padding(0.0)
        .toolbar_host(ContainerToolbarHost::Window)
        .on_action(move |host, records| {
            for record in records {
                if let Some(ContentActionPayload::ListIntent(list)) = &record.payload
                    && record.action.target == SHOWCASE_NAV_LIST_ID
                    && matches!(
                        list.kind,
                        ListIntentKind::SelectionChanged | ListIntentKind::Activated
                    )
                    && let Some(comp) = ShowcaseComponent::from_list_key(list.key)
                    && *sel_action.borrow() != comp
                {
                    *sel_action.borrow_mut() = comp;
                    let _ = host.set_keyed_list_selection(SHOWCASE_NAV_LIST_ID, Some(list.key));
                    wb_action.bump_revision();
                }
            }
        }))
}

fn build_single_component_container(
    component: ShowcaseComponent,
    workbench: SharedAgentWorkbench,
) -> Result<(String, LocalContainer)> {
    let (id, container) = match component {
        ShowcaseComponent::Transcript => (
            "component-transcript".to_owned(),
            TranscriptContainer::build("component-transcript", workbench)?,
        ),
        ShowcaseComponent::Composer => (
            "component-composer".to_owned(),
            ComposerContainer::build("component-composer", workbench)?,
        ),
        ShowcaseComponent::AgentRun => (
            "component-agent-run".to_owned(),
            AgentRunContainer::build("component-agent-run", workbench)?,
        ),
        ShowcaseComponent::Sidebar => (
            "component-sidebar".to_owned(),
            SessionSidebarContainer::build("component-sidebar", workbench)?,
        ),
        ShowcaseComponent::Summary => (
            "component-summary".to_owned(),
            SummaryContainer::build_tab("component-summary", workbench)?,
        ),
        ShowcaseComponent::Files => (
            "component-files".to_owned(),
            WorkspaceFilesContainer::build("component-files", workbench)?,
        ),
        ShowcaseComponent::Changes => (
            "component-changes".to_owned(),
            WorkspaceChangesContainer::build("component-changes", workbench)?,
        ),
        ShowcaseComponent::Tasks => (
            "component-tasks".to_owned(),
            TasksQueueContainer::build_tab("component-tasks", workbench)?,
        ),
        ShowcaseComponent::Settings => (
            "component-settings".to_owned(),
            HarnessSettingsContainer::build_tab("component-settings", workbench)?,
        ),
    };
    Ok((id, container))
}

pub fn build_showcase_app(
    rt: &TopologyARuntime,
    options: &TopologyALaunchOptions,
) -> Result<AppWindow> {
    let settings_file = rt.data_root.join("settings/agentkit-showcase.toml");
    let settings_owner = build_agent_settings_owner(settings_file)?;

    let initial = match options.component.as_deref() {
        Some(name) => ShowcaseComponent::from_name(name)?,
        None => ShowcaseComponent::Transcript,
    };

    if !options.showcase {
        let (focus_id, container) =
            build_single_component_container(initial, rt.workbench.clone())?;
        return Ok(AppWindow::from_containers(
            vec![container.into()],
            PresentationOptions::new(focus_id),
        )?
        .app_name("Tekes")
        .window_title(format!("Tekes — {}", initial.label()))
        .theme(options.theme)
        .shared_settings(settings_owner));
    }

    let selected = Rc::new(RefCell::new(initial));
    let nav = build_showcase_nav(Rc::clone(&selected), rt.workbench.clone())?;
    let stage_root = Content::new(
        SHOWCASE_STAGE_ROOT_ID,
        StackContent::vertical(0.0, Vec::new()),
    );
    let stage = ManagedAgentContainer::build_local(
        "showcase-stage",
        SHOWCASE_STAGE_SHELL_ID,
        stage_root,
        rt.workbench.clone(),
        ShowcaseStageAdapter::new(selected),
        false,
    )?
    .title("Component Showcase")
    .switcher_icon("tekes-side-chat")
    .content_padding(0.0);

    let mut presentation = PresentationOptions::new("showcase-stage").nav_bar("showcase-nav");
    presentation.nav_width = 224.0;

    Ok(
        AppWindow::from_containers(vec![nav.into(), stage.into()], presentation)?
            .app_name("Tekes")
            .window_title("Tekes Component Showcase")
            .theme(options.theme)
            .shared_settings(settings_owner),
    )
}
