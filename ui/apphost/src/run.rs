//! Core Agent Run containers (`AgentRunContainer` combining Transcript + Composer,
//! plus split `TranscriptContainer` and `ComposerContainer` for custom layouts).

use crate::{AgentContainerAdapter, ManagedAgentContainer, SharedAgentWorkbench};
use anyhow::Result;
use tekes_agent_components_ui::{
    ComposerIntent, ComposerSurface, RUN_CONTAINER_SHELL_ID, RUN_CONTAINER_STACK_ID,
    ResponsiveWidthMode, RunSetupIntent, RunSetupToolbar, TRANSCRIPT_ROOT_ID, TranscriptIntent,
    TranscriptSurface,
};
use tekes_agent_presentation::ActionablePrompt;
use tekes_agent_ui_bindings::{
    AVAILABLE_EFFORTS, AVAILABLE_HARNESSES, AVAILABLE_MODELS, AVAILABLE_PRESETS,
    ArtifactOpenRequest, WorkbenchCommand,
};
use tekes_ui_app_host::LocalContainer;
use tekes_ui_controls::{
    composition::{Content, ContentActionRecord, ContentHost, StackContent},
    extended::{Axis, Stack},
};
use tekes_ui_tokens::Spacing;

pub const FOCUS_SWITCH_COMMAND: u64 = 9001;

struct AgentRunAdapter {
    setup_toolbar: RunSetupToolbar,
    transcript: TranscriptSurface,
    composer: ComposerSurface,
    has_setup_toolbar: bool,
    transcript_only: bool,
    composer_only: bool,
}

impl AgentRunAdapter {
    fn new(has_setup_toolbar: bool, transcript_only: bool, composer_only: bool) -> Self {
        Self {
            setup_toolbar: RunSetupToolbar::new(),
            transcript: TranscriptSurface::new(),
            composer: ComposerSurface::new(),
            has_setup_toolbar,
            transcript_only,
            composer_only,
        }
    }

    fn catalog_index(catalog: &[&str], name: &str) -> usize {
        catalog.iter().position(|item| *item == name).unwrap_or(0)
    }

    fn catalog_vec(catalog: &[&str]) -> Vec<String> {
        catalog.iter().map(|s| (*s).to_owned()).collect()
    }
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
        if self.has_setup_toolbar {
            let harness_idx = Self::catalog_index(&AVAILABLE_HARNESSES, &run.selected_harness());
            let preset_idx = Self::catalog_index(&AVAILABLE_PRESETS, &run.selected_preset());
            let model_idx = Self::catalog_index(&AVAILABLE_MODELS, &run.selected_model());
            let effort_idx = Self::catalog_index(&AVAILABLE_EFFORTS, &run.selected_effort());
            self.setup_toolbar.sync(
                host,
                &snapshot,
                harness_idx,
                preset_idx,
                model_idx,
                effort_idx,
                width_mode,
            );
        }
        if !self.composer_only {
            let actionables: Vec<ActionablePrompt> = run.actionables_for(&snapshot);
            self.transcript
                .sync(host, &snapshot, &actionables, width_mode);
        }
        if !self.transcript_only {
            self.composer.sync(host, &snapshot, width_mode);
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

        if self.has_setup_toolbar {
            for intent in self.setup_toolbar.handle_actions(&snapshot, &records) {
                match intent {
                    RunSetupIntent::SelectHarness(idx) => run.set_harness(idx),
                    RunSetupIntent::SelectPreset(idx) => run.set_preset(idx),
                    RunSetupIntent::SelectModel(idx) => run.set_model(idx),
                    RunSetupIntent::SelectEffort(idx) => run.set_effort(idx),
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
                    ComposerIntent::Cancel { scope } => run.cancel_scope(&scope),
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
}

pub struct AgentRunContainer;

impl AgentRunContainer {
    pub fn build(id: impl Into<String>, workbench: SharedAgentWorkbench) -> Result<LocalContainer> {
        let harnesses = AgentRunAdapter::catalog_vec(&AVAILABLE_HARNESSES);
        let presets = AgentRunAdapter::catalog_vec(&AVAILABLE_PRESETS);
        let models = AgentRunAdapter::catalog_vec(&AVAILABLE_MODELS);
        let efforts = AgentRunAdapter::catalog_vec(&AVAILABLE_EFFORTS);
        let root = Content::new(
            RUN_CONTAINER_STACK_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Regular.value(),
                },
                flexible_child: Some(TRANSCRIPT_ROOT_ID),
                children: vec![
                    RunSetupToolbar::build_content(harnesses, presets, models, efforts),
                    TranscriptSurface::build_content(),
                    ComposerSurface::build_content(),
                ],
            },
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
            .switcher_icon("tekes-side-chat"))
    }

    pub fn follow_latest_in_host(host: &mut ContentHost) {
        if let Some(reader) = host
            .get_mut(tekes_agent_components_ui::TRANSCRIPT_READER_ID)
            .and_then(|c| c.component_mut::<tekes_agent_components_ui::TranscriptReader>())
        {
            reader.follow_latest();
        }
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
            .switcher_icon("tekes-side-chat"))
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
        Ok(container.title("Composer").switcher_icon("tekes-send"))
    }
}
