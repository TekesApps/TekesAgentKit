//! Core Agent Run containers (`AgentRunContainer` combining Transcript + Composer,
//! plus split `TranscriptContainer` and `ComposerContainer` for custom layouts).

use crate::{AgentContainerAdapter, ManagedAgentContainer, SharedAgentWorkbench};
use anyhow::Result;
use tekes_agent_components_ui::{
    ComposerIntent, ComposerSurface, RUN_CONTAINER_SHELL_ID, RUN_CONTAINER_STACK_ID,
    ResponsiveWidthMode, TRANSCRIPT_ROOT_ID, TranscriptIntent, TranscriptSurface,
};
use tekes_agent_presentation::ActionablePrompt;
use tekes_agent_ui_bindings::{AVAILABLE_HARNESSES, ArtifactOpenRequest, WorkbenchCommand};
use tekes_ui_app_host::{
    LocalContainer, LocalToolbarAction, LocalToolbarGroup, LocalToolbarGroupOverflow,
    LocalToolbarItem, LocalToolbarMenu, LocalToolbarTone,
};
use tekes_ui_controls::composition::{
    Content, ContentActionRecord, ContentHost, MenuEntry, StackContent,
};
use tekes_ui_tokens::Spacing;

pub const FOCUS_SWITCH_COMMAND: u64 = 9001;
pub const RUN_TOOLBAR_SETUP_GROUP_ID: u64 = 9100;
pub const RUN_TOOLBAR_HARNESS_CMD: u64 = 9101;
pub const RUN_TOOLBAR_APPROVAL_CMD: u64 = 9102;
pub const RUN_TOOLBAR_MODEL_CMD: u64 = 9103;
pub const RUN_TOOLBAR_EFFORT_CMD: u64 = 9104;

struct AgentRunAdapter {
    transcript: TranscriptSurface,
    composer: ComposerSurface,
    has_toolbar: bool,
    transcript_only: bool,
    composer_only: bool,
}

impl AgentRunAdapter {
    fn new(has_toolbar: bool, transcript_only: bool, composer_only: bool) -> Self {
        Self {
            transcript: TranscriptSurface::new(),
            composer: ComposerSurface::new(),
            has_toolbar,
            transcript_only,
            composer_only,
        }
    }

    fn catalog_index(catalog: &[&str], name: &str) -> usize {
        catalog.iter().position(|item| *item == name).unwrap_or(0)
    }
}

fn approval_icon_and_tone(preset_name: &str) -> (&'static str, LocalToolbarTone) {
    match preset_name {
        "Read Only" | "read-only" | "Deny Tools" | "deny-tools" => {
            ("tekes-access-read", LocalToolbarTone::Secondary)
        }
        "Full Access" | "danger-full-access" | "Allow Tools" | "allow-tools" => {
            ("tekes-access-full", LocalToolbarTone::Warning)
        }
        _ => ("tekes-access-workspace", LocalToolbarTone::Secondary),
    }
}

fn catalog_menu_entries(catalog: &[&str], selected_idx: usize, disabled: bool) -> Vec<MenuEntry> {
    catalog
        .iter()
        .enumerate()
        .map(|(idx, &label)| MenuEntry::Item {
            value: label.to_owned(),
            label: label.to_owned(),
            checked: Some(idx == selected_idx),
            disabled,
        })
        .collect()
}

impl AgentContainerAdapter for AgentRunAdapter {
    fn sync_from_workbench(
        &mut self,
        host: &mut ContentHost,
        workbench: &SharedAgentWorkbench,
        width_mode: ResponsiveWidthMode,
    ) {
        workbench.pump_composer_requests();
        let Some(run) = workbench.run_binding() else {
            return;
        };
        let snapshot = run.run_snapshot();
        if !self.composer_only {
            let active_session = run.current_scope().map(|scope| scope.session_id);
            let session_error = run.error_for_session(active_session.as_deref());
            self.transcript.set_session_error(session_error);
            let actionables: Vec<ActionablePrompt> = run.actionables_for(&snapshot);
            self.transcript
                .sync(host, &snapshot, &actionables, width_mode);
        }
        if !self.transcript_only {
            self.composer.sync(host, &snapshot, width_mode);
        }
    }

    fn drain_deferred_intents(&mut self, host: &mut ContentHost, workbench: &SharedAgentWorkbench) {
        if self.composer_only {
            return;
        }
        let Some(run) = workbench.run_binding() else {
            return;
        };
        for intent in self.transcript.drain_link_actions(host) {
            if let TranscriptIntent::OpenLink(link) = intent
                && let Some(path) = link.strip_prefix("doc:")
            {
                workbench.enqueue_command(WorkbenchCommand::OpenArtifact(ArtifactOpenRequest {
                    workspace_id: workbench
                        .active_workspace_id()
                        .unwrap_or_else(|| "ws-main".to_owned()),
                    path: path.to_owned(),
                    revision: None,
                    line_hint: None,
                    source_scope: run.current_scope(),
                }));
            }
        }
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
        let snapshot = run.run_snapshot();

        if !self.composer_only {
            for intent in self.transcript.handle_actions(host, &snapshot, &records) {
                match intent {
                    TranscriptIntent::LoadEarlier(scope) => run.load_earlier(&scope),
                    TranscriptIntent::RespondActionable {
                        scope,
                        actionable_id,
                        revision,
                        choice_index,
                    } => {
                        let actionables = run.actionables_for(&snapshot);
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
                    TranscriptIntent::OpenLink(link) => {
                        if let Some(path) = link.strip_prefix("doc:") {
                            workbench.enqueue_command(WorkbenchCommand::OpenArtifact(
                                ArtifactOpenRequest {
                                    workspace_id: workbench
                                        .active_workspace_id()
                                        .unwrap_or_else(|| "ws-main".to_owned()),
                                    path: path.to_owned(),
                                    revision: None,
                                    line_hint: None,
                                    source_scope: run.current_scope(),
                                },
                            ));
                        }
                    }
                    TranscriptIntent::Reconnect => run.reconnect(),
                    TranscriptIntent::CopyLatest
                    | TranscriptIntent::FeedbackLatest { .. }
                    | TranscriptIntent::ForkLatest => {
                        workbench.bump_revision();
                    }
                }
            }
        }

        if !self.transcript_only {
            for intent in self.composer.handle_actions(host, &snapshot, &records) {
                match intent {
                    ComposerIntent::EditText { destination, text } => {
                        run.set_draft_text(&destination, text);
                    }
                    ComposerIntent::Send { destination } => {
                        run.submit_destination(&destination);
                    }
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
                    } => {
                        run.remove_artifact(&destination, &artifact_id);
                    }
                    ComposerIntent::RemoveAnnotation {
                        destination,
                        annotation_id,
                    } => {
                        run.remove_annotation(&destination, &annotation_id);
                    }
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
    }

    fn toolbar_items(&mut self, workbench: &SharedAgentWorkbench) -> Option<Vec<LocalToolbarItem>> {
        self.has_toolbar
            .then(|| AgentRunContainer::build_toolbar_items(workbench))
    }

    fn handle_toolbar_action(
        &mut self,
        _host: &mut ContentHost,
        workbench: &SharedAgentWorkbench,
        action: &LocalToolbarAction,
    ) -> Result<bool> {
        if !self.has_toolbar {
            return Ok(false);
        }
        Ok(AgentRunContainer::handle_toolbar_action(workbench, action))
    }
}

pub struct AgentRunContainer;

impl AgentRunContainer {
    pub fn build(id: impl Into<String>, workbench: SharedAgentWorkbench) -> Result<LocalContainer> {
        let root = Content::new(
            RUN_CONTAINER_STACK_ID,
            StackContent::vertical(
                Spacing::Regular.value(),
                vec![
                    TranscriptSurface::build_content(),
                    ComposerSurface::build_content(),
                ],
            )
            .with_flexible_child(TRANSCRIPT_ROOT_ID),
        );
        let container = ManagedAgentContainer::build_local(
            id,
            RUN_CONTAINER_SHELL_ID,
            root,
            workbench,
            AgentRunAdapter::new(true, false, false),
            false,
        )?;
        Ok(container
            .title("Conversation")
            .switcher_icon("tekes-side-chat")
            .content_padding(0.0))
    }

    pub fn build_toolbar_items(workbench: &SharedAgentWorkbench) -> Vec<LocalToolbarItem> {
        let Some(run) = workbench.run_binding() else {
            return Vec::new();
        };
        let snapshot = run.run_snapshot();
        let harness_locked = snapshot.run.is_some() || !snapshot.transcript.items.is_empty();

        let available_presets = run.available_presets();
        let available_models = run.available_models();
        let available_efforts = run.available_efforts();
        let preset_refs: Vec<&str> = available_presets.iter().map(String::as_str).collect();
        let model_refs: Vec<&str> = available_models.iter().map(String::as_str).collect();
        let effort_refs: Vec<&str> = available_efforts.iter().map(String::as_str).collect();

        let harness_idx =
            AgentRunAdapter::catalog_index(&AVAILABLE_HARNESSES, &run.selected_harness());
        let preset_idx = AgentRunAdapter::catalog_index(&preset_refs, &run.selected_preset());
        let model_idx = AgentRunAdapter::catalog_index(&model_refs, &run.selected_model());
        let effort_idx = AgentRunAdapter::catalog_index(&effort_refs, &run.selected_effort());

        let harness_name = AVAILABLE_HARNESSES
            .get(harness_idx)
            .copied()
            .unwrap_or("Tekes");
        let preset_name = preset_refs
            .get(preset_idx)
            .copied()
            .unwrap_or("Workspace Write");
        let model_name = model_refs
            .get(model_idx)
            .copied()
            .unwrap_or("claude-sonnet-4");
        let effort_name = effort_refs.get(effort_idx).copied().unwrap_or("Medium");

        let (approval_icon, approval_tone) = approval_icon_and_tone(preset_name);
        vec![LocalToolbarItem::Group(
            LocalToolbarGroup::new(
                RUN_TOOLBAR_SETUP_GROUP_ID,
                vec![
                    LocalToolbarItem::Menu(
                        LocalToolbarMenu::new(
                            RUN_TOOLBAR_HARNESS_CMD,
                            "",
                            catalog_menu_entries(&AVAILABLE_HARNESSES, harness_idx, harness_locked),
                        )
                        .icon("tekes-app-server")
                        .tooltip(if harness_locked {
                            format!("Harness: {harness_name} (locked for this thread)")
                        } else {
                            format!("Harness: {harness_name}")
                        })
                        .dropdown_indicator(false)
                        .tone(LocalToolbarTone::Secondary),
                    ),
                    LocalToolbarItem::Menu(
                        LocalToolbarMenu::new(
                            RUN_TOOLBAR_APPROVAL_CMD,
                            "",
                            catalog_menu_entries(&preset_refs, preset_idx, false),
                        )
                        .icon(approval_icon)
                        .tooltip(format!("Approval: {preset_name}"))
                        .dropdown_indicator(false)
                        .tone(approval_tone),
                    ),
                    LocalToolbarItem::Menu(
                        LocalToolbarMenu::new(
                            RUN_TOOLBAR_MODEL_CMD,
                            model_name,
                            catalog_menu_entries(&model_refs, model_idx, false),
                        )
                        .tooltip(format!("Model: {model_name}"))
                        .dropdown_indicator(false)
                        .tone(LocalToolbarTone::Secondary),
                    ),
                    LocalToolbarItem::Menu(
                        LocalToolbarMenu::new(
                            RUN_TOOLBAR_EFFORT_CMD,
                            effort_name,
                            catalog_menu_entries(&effort_refs, effort_idx, false),
                        )
                        .tooltip(format!("Reasoning Effort: {effort_name}"))
                        .dropdown_indicator(false)
                        .tone(LocalToolbarTone::Secondary),
                    ),
                ],
            )
            .divided(true)
            .label("Conversation setup")
            .tooltip("Conversation setup")
            .overflow(LocalToolbarGroupOverflow::Together),
        )]
    }

    pub fn handle_toolbar_action(
        workbench: &SharedAgentWorkbench,
        action: &LocalToolbarAction,
    ) -> bool {
        let (Some(run), Some(value)) = (workbench.run_binding(), action.menu_value()) else {
            return false;
        };
        match action.id {
            RUN_TOOLBAR_HARNESS_CMD => {
                let snapshot = run.run_snapshot();
                let harness_locked =
                    snapshot.run.is_some() || !snapshot.transcript.items.is_empty();
                if !harness_locked
                    && let Some(idx) = AVAILABLE_HARNESSES.iter().position(|&item| item == value)
                {
                    run.set_harness(idx);
                    workbench.bump_revision();
                }
                true
            }
            RUN_TOOLBAR_APPROVAL_CMD => {
                let presets = run.available_presets();
                if let Some(idx) = presets.iter().position(|item| item == value) {
                    run.set_preset(idx);
                    workbench.bump_revision();
                }
                true
            }
            RUN_TOOLBAR_MODEL_CMD => {
                let models = run.available_models();
                if let Some(idx) = models.iter().position(|item| item == value) {
                    run.set_model(idx);
                    workbench.bump_revision();
                }
                true
            }
            RUN_TOOLBAR_EFFORT_CMD => {
                let efforts = run.available_efforts();
                if let Some(idx) = efforts.iter().position(|item| item == value) {
                    run.set_effort(idx);
                    workbench.bump_revision();
                }
                true
            }
            _ => false,
        }
    }

    pub fn follow_latest_in_host(host: &mut ContentHost) {
        TranscriptSurface::follow_latest(host);
    }
}

pub struct TranscriptContainer;

impl TranscriptContainer {
    pub fn build(id: impl Into<String>, workbench: SharedAgentWorkbench) -> Result<LocalContainer> {
        let container = ManagedAgentContainer::build_local(
            id,
            tekes_ui_controls::composition::ComponentId(11),
            TranscriptSurface::build_content(),
            workbench,
            AgentRunAdapter::new(false, true, false),
            false,
        )?;
        Ok(container
            .title("Conversation")
            .switcher_icon("tekes-side-chat")
            .content_padding(0.0))
    }
}

pub struct ComposerContainer;

impl ComposerContainer {
    pub fn build(id: impl Into<String>, workbench: SharedAgentWorkbench) -> Result<LocalContainer> {
        let container = ManagedAgentContainer::build_local(
            id,
            tekes_ui_controls::composition::ComponentId(12),
            ComposerSurface::build_content(),
            workbench,
            AgentRunAdapter::new(false, false, true),
            false,
        )?;
        Ok(container
            .title("Composer")
            .switcher_icon("tekes-send")
            .content_padding(0.0))
    }
}
