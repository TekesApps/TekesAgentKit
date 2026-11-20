//! UI-friendly bindings over `SessionRuntime`, `RunController`, `ActionablePrompt`,
//! `SummaryFacts`, and Sidebar session inventory/preferences.

use crate::UiWakeNotifier;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};
use tekes_agent_contract::{ComposerAnnotation, PromptMode, Result, SessionAddress};
use tekes_agent_presentation::{
    ActionablePrompt, ComposerArtifact, ComposerContextBaseline, ComposerDestination,
    RunController, RunScope, RunSnapshot, SummaryFacts, TranscriptDocument, thread_activity,
};
use tekes_agent_session::SessionRuntime;
use tokio::runtime::Handle;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SidebarFilterMode {
    #[default]
    All,
    Pinned,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarSessionRow {
    pub session_id: String,
    pub endpoint_id: String,
    pub title: String,
    pub folder: String,
    pub activity: String,
    pub running: bool,
    pub pinned: bool,
    pub archived: bool,
    pub updated_at_order: u64,
    pub is_child: bool,
}

#[derive(Clone, Debug)]
struct RunUiState {
    endpoint_id: String,
    filter_mode: SidebarFilterMode,
    search_query: String,
    selected_model: String,
    selected_preset: String,
    local_pins: BTreeSet<String>,
    local_titles: BTreeMap<String, String>,
    local_archived: BTreeSet<String>,
    host_summary_sections: Vec<(String, Vec<(String, String)>)>,
    last_error: Option<String>,
    revision: u64,
}

pub const AVAILABLE_MODELS: [&str; 3] = ["claude-sonnet-4", "gpt-5", "gemini-2.5-pro"];
pub const AVAILABLE_PRESETS: [&str; 3] = ["标准均衡", "深度思考", "极速响应"];

/// Coordinates one `SessionRuntime` + `RunController` with UI containers via `UiWakeNotifier`.
pub struct RunWorkbenchBinding {
    runtime: Arc<SessionRuntime>,
    controller: Arc<RunController>,
    handle: Handle,
    wake: Arc<dyn UiWakeNotifier>,
    ui: Mutex<RunUiState>,
    _watcher: tokio::task::JoinHandle<()>,
}

impl Drop for RunWorkbenchBinding {
    fn drop(&mut self) {
        self._watcher.abort();
    }
}

impl std::fmt::Debug for RunWorkbenchBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunWorkbenchBinding")
            .field("endpoint_id", &self.endpoint_id())
            .finish_non_exhaustive()
    }
}

impl RunWorkbenchBinding {
    pub fn new(
        endpoint_id: impl Into<String>,
        runtime: Arc<SessionRuntime>,
        controller: Arc<RunController>,
        handle: Handle,
        wake: Arc<dyn UiWakeNotifier>,
    ) -> Arc<Self> {
        let endpoint_id = endpoint_id.into();
        let mut run_rx = controller.subscribe();
        let mut store_rx = runtime.subscribe();
        let wake_clone = Arc::clone(&wake);
        let watcher = handle.spawn(async move {
            loop {
                tokio::select! {
                    changed = run_rx.changed() => {
                        if changed.is_err() {
                            break;
                        }
                        wake_clone.wake();
                    }
                    changed = store_rx.changed() => {
                        if changed.is_err() {
                            break;
                        }
                        wake_clone.wake();
                    }
                }
            }
        });
        Arc::new(Self {
            runtime,
            controller,
            handle,
            wake,
            ui: Mutex::new(RunUiState {
                endpoint_id,
                filter_mode: SidebarFilterMode::All,
                search_query: String::new(),
                selected_model: AVAILABLE_MODELS[0].to_owned(),
                selected_preset: AVAILABLE_PRESETS[0].to_owned(),
                local_pins: BTreeSet::new(),
                local_titles: BTreeMap::new(),
                local_archived: BTreeSet::new(),
                host_summary_sections: Vec::new(),
                last_error: None,
                revision: 1,
            }),
            _watcher: watcher,
        })
    }

    pub fn runtime(&self) -> &Arc<SessionRuntime> {
        &self.runtime
    }

    pub fn controller(&self) -> &Arc<RunController> {
        &self.controller
    }

    pub fn handle(&self) -> &Handle {
        &self.handle
    }

    pub fn endpoint_id(&self) -> String {
        self.ui.lock().expect("run ui lock").endpoint_id.clone()
    }

    pub fn ui_revision(&self) -> u64 {
        let run_rev = self.controller.snapshot().revision;
        let ui_rev = self.ui.lock().expect("run ui lock").revision;
        run_rev.saturating_add(ui_rev)
    }

    fn bump_ui(&self) {
        {
            let mut guard = self.ui.lock().expect("run ui lock");
            guard.revision = guard.revision.saturating_add(1);
        }
        self.wake.wake();
    }

    pub fn last_error(&self) -> Option<String> {
        self.ui.lock().expect("run ui lock").last_error.clone()
    }

    pub fn clear_error(&self) {
        self.ui.lock().expect("run ui lock").last_error = None;
        self.bump_ui();
    }

    pub fn set_host_summary_sections(&self, sections: Vec<(String, Vec<(String, String)>)>) {
        self.ui.lock().expect("run ui lock").host_summary_sections = sections;
        self.bump_ui();
    }

    pub fn host_summary_sections(&self) -> Vec<(String, Vec<(String, String)>)> {
        self.ui
            .lock()
            .expect("run ui lock")
            .host_summary_sections
            .clone()
    }

    pub fn run_snapshot(&self) -> Arc<RunSnapshot> {
        self.controller.snapshot()
    }

    pub fn current_scope(&self) -> Option<RunScope> {
        self.controller
            .snapshot()
            .selection
            .as_ref()
            .and_then(|token| match &token.destination {
                ComposerDestination::Run { scope } => Some(scope.clone()),
                _ => None,
            })
    }

    pub fn current_destination(&self) -> Option<ComposerDestination> {
        self.controller
            .snapshot()
            .selection
            .as_ref()
            .map(|token| token.destination.clone())
    }

    pub fn open_initial_session(&self, session_id: &str) -> Result<()> {
        let scope = RunScope::main(self.endpoint_id(), session_id);
        let controller = Arc::clone(&self.controller);
        self.handle.block_on(async move {
            controller.open(scope.clone()).await?;
            controller.select(ComposerDestination::run(scope))?;
            Ok(())
        })
    }

    pub fn select_session(&self, session_id: &str) {
        let endpoint_id = self.endpoint_id();
        let scope = RunScope::main(endpoint_id, session_id);
        let dest = ComposerDestination::run(scope.clone());
        let _ = self.controller.select(dest);
        let controller = Arc::clone(&self.controller);
        let wake = Arc::clone(&self.wake);
        self.handle.spawn(async move {
            let _ = controller.open(scope).await;
            wake.wake();
        });
        self.bump_ui();
    }

    pub fn create_session(&self, cwd: &str) {
        let runtime = Arc::clone(&self.runtime);
        let controller = Arc::clone(&self.controller);
        let endpoint_id = self.endpoint_id();
        let wake = Arc::clone(&self.wake);
        let cwd = cwd.to_owned();
        let rpc_id = format!("create-{}", uuid::Uuid::new_v4());
        self.handle.spawn(async move {
            if let Ok(session_id) = runtime.create_session(&cwd, &rpc_id).await {
                let scope = RunScope::main(endpoint_id, session_id);
                let _ = controller.open(scope.clone()).await;
                let _ = controller.select(ComposerDestination::run(scope));
                wake.wake();
            }
        });
    }

    pub fn set_draft_text(&self, destination: &ComposerDestination, text: String) {
        let _ = self.controller.edit_text(destination, text);
        self.wake.wake();
    }

    pub fn attach_file_path(&self, destination: Option<ComposerDestination>, path: String) {
        let snapshot = self.controller.snapshot();
        let Some(target) =
            destination.or_else(|| snapshot.selection.as_ref().map(|t| t.destination.clone()))
        else {
            return;
        };
        if let Some(baseline) = ComposerContextBaseline::read(&snapshot)
            && baseline.selection.destination == target
        {
            let mut artifacts = baseline.artifacts.clone();
            let id = format!("file:{}", uuid::Uuid::new_v4());
            artifacts.push(ComposerArtifact::Reference {
                id,
                mention: None,
                path: Some(path),
                excerpt: None,
            });
            let _ =
                self.controller
                    .patch_context(&baseline, artifacts, baseline.annotations.clone());
        } else {
            let mut draft = snapshot.draft.clone();
            draft.artifacts.push(ComposerArtifact::Reference {
                id: format!("file:{}", uuid::Uuid::new_v4()),
                mention: None,
                path: Some(path),
                excerpt: None,
            });
            let _ = self.controller.edit(&target, draft);
        }
        self.bump_ui();
    }

    pub fn apply_quote(&self, request: crate::ComposerQuoteRequest) {
        let snapshot = self.controller.snapshot();
        let Some(target) = request
            .destination
            .or_else(|| snapshot.selection.as_ref().map(|t| t.destination.clone()))
        else {
            return;
        };
        let annotation = ComposerAnnotation {
            id: format!("ann-{}", uuid::Uuid::new_v4()),
            source: request.reference.artifact_id.clone(),
            excerpt: request.reference.quote.clone(),
            comment: request.comment,
            source_locator: None,
            artifact_reference: Some(Box::new(request.reference)),
        };
        if let Some(baseline) = ComposerContextBaseline::read(&snapshot)
            && baseline.selection.destination == target
        {
            let mut annotations = baseline.annotations.clone();
            annotations.push(annotation);
            let _ =
                self.controller
                    .patch_context(&baseline, baseline.artifacts.clone(), annotations);
        } else {
            let mut draft = snapshot.draft.clone();
            draft.annotations.push(annotation);
            let _ = self.controller.edit(&target, draft);
        }
        self.bump_ui();
    }

    pub fn remove_artifact(&self, destination: &ComposerDestination, artifact_id: &str) {
        let snapshot = self.controller.snapshot();
        if let Some(baseline) = ComposerContextBaseline::read(&snapshot)
            && &baseline.selection.destination == destination
        {
            let artifacts: Vec<_> = baseline
                .artifacts
                .iter()
                .filter(|item| item.id() != artifact_id)
                .cloned()
                .collect();
            let _ =
                self.controller
                    .patch_context(&baseline, artifacts, baseline.annotations.clone());
            self.bump_ui();
        }
    }

    pub fn remove_annotation(&self, destination: &ComposerDestination, annotation_id: &str) {
        let snapshot = self.controller.snapshot();
        if let Some(baseline) = ComposerContextBaseline::read(&snapshot)
            && &baseline.selection.destination == destination
        {
            let annotations: Vec<_> = baseline
                .annotations
                .iter()
                .filter(|item| item.id != annotation_id)
                .cloned()
                .collect();
            let _ =
                self.controller
                    .patch_context(&baseline, baseline.artifacts.clone(), annotations);
            self.bump_ui();
        }
    }

    pub fn submit_destination(&self, destination: &ComposerDestination) {
        let Ok(ticket) = self.controller.prepare(destination, PromptMode::Queue) else {
            return;
        };
        let controller = Arc::clone(&self.controller);
        let wake = Arc::clone(&self.wake);
        self.handle.spawn(async move {
            let _ = controller.dispatch(ticket, None).await;
            wake.wake();
        });
        self.bump_ui();
    }

    pub fn cancel_scope(&self, scope: &RunScope) {
        let controller = Arc::clone(&self.controller);
        let scope = scope.clone();
        let wake = Arc::clone(&self.wake);
        self.handle.spawn(async move {
            let _ = controller.cancel(&scope).await;
            wake.wake();
        });
    }

    pub fn load_earlier(&self, scope: &RunScope) {
        let controller = Arc::clone(&self.controller);
        let scope = scope.clone();
        let wake = Arc::clone(&self.wake);
        self.handle.spawn(async move {
            let _ = controller.load_earlier(&scope).await;
            wake.wake();
        });
    }

    pub fn reconnect(&self) {
        let runtime = Arc::clone(&self.runtime);
        let wake = Arc::clone(&self.wake);
        self.handle.spawn(async move {
            let _ = runtime.reconnect().await;
            wake.wake();
        });
    }

    pub fn respond_actionable(
        &self,
        scope: &RunScope,
        actionable_id: &str,
        revision: i64,
        outcome: Value,
    ) {
        let controller = Arc::clone(&self.controller);
        let scope = scope.clone();
        let id = actionable_id.to_owned();
        let wake = Arc::clone(&self.wake);
        self.handle.spawn(async move {
            let _ = controller.respond(&scope, &id, revision, outcome).await;
            wake.wake();
        });
    }

    pub fn sidebar_filter(&self) -> SidebarFilterMode {
        self.ui.lock().expect("run ui lock").filter_mode
    }

    pub fn set_sidebar_filter(&self, mode: SidebarFilterMode) {
        self.ui.lock().expect("run ui lock").filter_mode = mode;
        self.bump_ui();
    }

    pub fn search_query(&self) -> String {
        self.ui.lock().expect("run ui lock").search_query.clone()
    }

    pub fn set_search_query(&self, query: String) {
        self.ui.lock().expect("run ui lock").search_query = query;
        self.bump_ui();
    }

    pub fn toggle_pin(&self, session_id: &str) {
        let mut guard = self.ui.lock().expect("run ui lock");
        if !guard.local_pins.remove(session_id) {
            guard.local_pins.insert(session_id.to_owned());
        }
        guard.revision = guard.revision.saturating_add(1);
        drop(guard);
        self.wake.wake();
    }

    pub fn rename_session(&self, session_id: &str, title: String) {
        if title.trim().is_empty() {
            return;
        }
        {
            let mut guard = self.ui.lock().expect("run ui lock");
            guard
                .local_titles
                .insert(session_id.to_owned(), title.clone());
            guard.revision = guard.revision.saturating_add(1);
        }
        let runtime = Arc::clone(&self.runtime);
        let sid = session_id.to_owned();
        let wake = Arc::clone(&self.wake);
        self.handle.spawn(async move {
            let rpc = format!("rename-{}", uuid::Uuid::new_v4());
            let _ = runtime.rename(&sid, &title, &rpc).await;
            wake.wake();
        });
        self.wake.wake();
    }

    pub fn archive_session(&self, session_id: &str) {
        {
            let mut guard = self.ui.lock().expect("run ui lock");
            if !guard.local_archived.remove(session_id) {
                guard.local_archived.insert(session_id.to_owned());
            }
            guard.revision = guard.revision.saturating_add(1);
        }
        let runtime = Arc::clone(&self.runtime);
        let sid = session_id.to_owned();
        let archived = self
            .ui
            .lock()
            .expect("run ui lock")
            .local_archived
            .contains(&sid);
        let wake = Arc::clone(&self.wake);
        self.handle.spawn(async move {
            let rpc = format!("archive-{}", uuid::Uuid::new_v4());
            if archived {
                let _ = runtime.archive(&sid, &rpc).await;
            } else {
                let _ = runtime.unarchive(&sid, &rpc).await;
            }
            wake.wake();
        });
        self.wake.wake();
    }

    pub fn selected_model(&self) -> String {
        self.ui.lock().expect("run ui lock").selected_model.clone()
    }

    pub fn set_model(&self, index: usize) {
        if let Some(model) = AVAILABLE_MODELS.get(index) {
            {
                let mut guard = self.ui.lock().expect("run ui lock");
                guard.selected_model = (*model).to_owned();
                guard.revision = guard.revision.saturating_add(1);
            }
            let snap = self.controller.snapshot();
            if let Some(token) = &snap.selection {
                let mut settings = snap.draft.settings.clone();
                settings.insert("model".into(), json!(*model));
                self.controller.commit_settings(token, settings, false);
            }
            self.wake.wake();
        }
    }

    pub fn selected_preset(&self) -> String {
        self.ui.lock().expect("run ui lock").selected_preset.clone()
    }

    pub fn set_preset(&self, index: usize) {
        if let Some(preset) = AVAILABLE_PRESETS.get(index) {
            {
                let mut guard = self.ui.lock().expect("run ui lock");
                guard.selected_preset = (*preset).to_owned();
                guard.revision = guard.revision.saturating_add(1);
            }
            let snap = self.controller.snapshot();
            if let Some(token) = &snap.selection {
                let mut settings = snap.draft.settings.clone();
                settings.insert("preset".into(), json!(*preset));
                self.controller.commit_settings(token, settings, false);
            }
            self.wake.wake();
        }
    }

    pub fn sidebar_rows(&self) -> Vec<SidebarSessionRow> {
        let store = self.runtime.snapshot();
        let ui = self.ui.lock().expect("run ui lock");
        let query = ui.search_query.trim().to_lowercase();
        let mut rows = Vec::new();
        for (id, projection) in &store.sessions {
            let Some(summary) = projection.summary.as_ref() else {
                continue;
            };
            let is_child = summary
                .get("parentSessionId")
                .and_then(Value::as_str)
                .is_some();
            if is_child {
                continue;
            }
            let title = ui
                .local_titles
                .get(id)
                .cloned()
                .or_else(|| summary["title"].as_str().map(str::to_owned))
                .unwrap_or_else(|| id.clone());
            let folder = summary["folder"]
                .as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or("默认")
                .to_owned();
            let archived = store.archived_session_ids.contains(id)
                || ui.local_archived.contains(id)
                || summary["status"].as_str() == Some("archived");
            let pinned = store.pinned_session_ids.contains(id) || ui.local_pins.contains(id);
            if matches!(ui.filter_mode, SidebarFilterMode::Pinned) && !pinned {
                continue;
            }
            if !query.is_empty()
                && !title.to_lowercase().contains(&query)
                && !id.to_lowercase().contains(&query)
                && !folder.to_lowercase().contains(&query)
            {
                continue;
            }
            let activity = thread_activity(&store, id);
            let running =
                activity.status == "running" || summary["running"].as_bool() == Some(true);
            let updated_at_order = summary["updatedAt"].as_f64().unwrap_or(0.0).max(0.0) as u64;
            rows.push(SidebarSessionRow {
                session_id: id.clone(),
                endpoint_id: store.endpoint_id.clone(),
                title,
                folder,
                activity: activity.status,
                running,
                pinned,
                archived,
                updated_at_order,
                is_child,
            });
        }
        rows.sort_by(|a, b| {
            b.pinned
                .cmp(&a.pinned)
                .then_with(|| b.updated_at_order.cmp(&a.updated_at_order))
                .then_with(|| a.session_id.cmp(&b.session_id))
        });
        rows
    }

    pub fn actionables_for(&self, snapshot: &RunSnapshot) -> Vec<ActionablePrompt> {
        let Some(run) = snapshot.run.as_ref() else {
            return Vec::new();
        };
        run.actionables
            .iter()
            .filter_map(|raw| ActionablePrompt::read(run.scope.clone(), raw.clone()).ok())
            .collect()
    }

    pub fn summary_facts_for(&self, snapshot: &RunSnapshot) -> Option<SummaryFacts> {
        let scope = match snapshot.selection.as_ref()?.destination.clone() {
            ComposerDestination::Run { scope } => scope,
            _ => return None,
        };
        if let Some(facts) = &snapshot.summary {
            return Some(facts.clone());
        }
        let store = self.runtime.snapshot();
        SummaryFacts::read_with_store(&scope, &store, &snapshot.transcript).ok()
    }

    pub fn transcript_sections_text<'a>(
        &self,
        snapshot: &'a RunSnapshot,
    ) -> Option<TranscriptDocument<'a>> {
        TranscriptDocument::read(&snapshot.transcript).ok()
    }

    pub fn active_session_address(&self) -> Option<SessionAddress> {
        self.current_scope().map(|scope| scope.address())
    }
}
