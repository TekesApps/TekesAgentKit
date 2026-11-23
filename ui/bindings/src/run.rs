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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelCatalogEntry {
    pub provider: String,
    pub id: String,
    pub label: String,
    pub efforts: Vec<String>,
    pub default_effort: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalModeOption {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug)]
struct RunUiState {
    endpoint_id: String,
    default_cwd: String,
    live_kernel_tools: bool,
    filter_mode: SidebarFilterMode,
    search_query: String,
    selected_harness: String,
    selected_provider: Option<String>,
    selected_model: String,
    selected_preset: String,
    selected_approval_value: Option<String>,
    selected_effort: String,
    models_catalog: Vec<ModelCatalogEntry>,
    approval_options: Vec<ApprovalModeOption>,
    local_pins: BTreeSet<String>,
    local_titles: BTreeMap<String, String>,
    local_archived: BTreeSet<String>,
    host_summary_sections: Vec<(String, Vec<(String, String)>)>,
    last_error: Option<String>,
    revision: u64,
}

pub const AVAILABLE_HARNESSES: [&str; 3] = ["Tekes", "Pi", "DSH"];
pub const AVAILABLE_MODELS: [&str; 3] = ["claude-sonnet-4", "gpt-5", "gemini-2.5-pro"];
pub const AVAILABLE_PRESETS: [&str; 3] = ["Workspace Write", "Read Only", "Full Access"];
pub const AVAILABLE_EFFORTS: [&str; 3] = ["Low", "Medium", "High"];

fn effort_label(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "none" => "None".to_owned(),
        "minimal" => "Minimal".to_owned(),
        "low" => "Low".to_owned(),
        "medium" => "Medium".to_owned(),
        "high" => "High".to_owned(),
        "xhigh" | "extra_high" | "extra-high" => "Extra High".to_owned(),
        other if !other.is_empty() => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
        _ => "Medium".to_owned(),
    }
}

fn effort_id_from_label(label: &str, catalog_efforts: &[String]) -> Option<String> {
    if label.trim().eq_ignore_ascii_case("Default") {
        return None;
    }
    if let Some(exact) = catalog_efforts.iter().find(|raw| {
        raw.eq_ignore_ascii_case(label) || effort_label(raw).eq_ignore_ascii_case(label)
    }) {
        return Some(exact.clone());
    }
    match label.trim().to_ascii_lowercase().as_str() {
        "extra high" => Some("xhigh".to_owned()),
        "" => None,
        other => Some(other.to_owned()),
    }
}

fn approval_label_for(value: &str, title: Option<&str>) -> String {
    match value {
        "workspace-write" => "Workspace Write".to_owned(),
        "read-only" => "Read Only".to_owned(),
        "danger-full-access" => "Full Access".to_owned(),
        "allow-tools" => "Allow Tools".to_owned(),
        "ask-tools" => "Ask Every Time".to_owned(),
        "deny-tools" => "Deny Tools".to_owned(),
        _ => title
            .filter(|t| !t.trim().is_empty())
            .unwrap_or(value)
            .to_owned(),
    }
}

fn parse_models_catalog(
    val: &Value,
) -> (
    Vec<ModelCatalogEntry>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    let groups = val
        .get("groups")
        .or_else(|| val.get("providers"))
        .and_then(Value::as_array);
    let mut entries = Vec::new();
    if let Some(groups) = groups {
        for group in groups {
            let provider_id = group
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            if let Some(models) = group.get("models").and_then(Value::as_array) {
                for model in models {
                    let Some(id) = model.get("id").and_then(Value::as_str) else {
                        continue;
                    };
                    if model.get("enabled").and_then(Value::as_bool) == Some(false) {
                        continue;
                    }
                    let label = model
                        .get("name")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                        .unwrap_or(id)
                        .to_owned();
                    let efforts = model
                        .get("reasoning")
                        .and_then(|r| r.get("efforts"))
                        .and_then(Value::as_array)
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|e| {
                                    e.get("id").and_then(Value::as_str).or_else(|| e.as_str())
                                })
                                .map(str::to_owned)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let default_effort = model
                        .get("reasoning")
                        .and_then(|r| r.get("defaultEffort"))
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    entries.push(ModelCatalogEntry {
                        provider: provider_id.clone(),
                        id: id.to_owned(),
                        label,
                        efforts,
                        default_effort,
                    });
                }
            }
        }
    }
    let current = val.get("current").or_else(|| val.get("selected"));
    let cur_provider = current
        .and_then(|c| c.get("provider"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let cur_model = current
        .and_then(|c| c.get("model"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let cur_effort = current
        .and_then(|c| c.get("reasoningEffort"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    (entries, cur_provider, cur_model, cur_effort)
}

fn parse_approval_policy(
    policy: &Value,
    current_mode: Option<&str>,
) -> (Vec<ApprovalModeOption>, Option<String>, Option<String>) {
    let list = policy
        .get("threadLevel")
        .and_then(Value::as_array)
        .filter(|arr| !arr.is_empty())
        .or_else(|| {
            policy
                .get("serverLevel")
                .and_then(|s| s.get("options"))
                .and_then(Value::as_array)
        });
    let mut options = Vec::new();
    if let Some(arr) = list {
        for opt in arr {
            let Some(value) = opt.get("value").and_then(Value::as_str) else {
                continue;
            };
            let label = approval_label_for(value, opt.get("title").and_then(Value::as_str));
            options.push(ApprovalModeOption {
                value: value.to_owned(),
                label,
            });
        }
    }
    let server_cur = policy
        .get("serverLevel")
        .and_then(|s| s.get("currentValue"))
        .and_then(Value::as_str);
    let desired_val = current_mode
        .filter(|s| !s.is_empty())
        .or(server_cur)
        .or_else(|| {
            options
                .iter()
                .find(|o| o.value == "workspace-write")
                .map(|o| o.value.as_str())
        })
        .or_else(|| options.first().map(|o| o.value.as_str()));
    let chosen = desired_val.and_then(|val| options.iter().find(|o| o.value == val));
    let chosen_val = chosen.map(|o| o.value.clone());
    let chosen_label = chosen.map(|o| o.label.clone());
    (options, chosen_val, chosen_label)
}

struct RunSubmissionPreparation {
    provider: Option<String>,
    model: String,
    effort_id: Option<String>,
    approval_mode: Option<String>,
}

#[async_trait::async_trait]
impl tekes_agent_presentation::SubmissionPreparation for RunSubmissionPreparation {
    async fn prepare(
        &self,
        runtime: &SessionRuntime,
        _ticket: &tekes_agent_presentation::PreparedSubmission,
        scope: &RunScope,
    ) -> Result<()> {
        let caps = runtime.inspect(|s| s.endpoint_capabilities.clone());
        if caps.supports("session.selectModel")
            && let Some(provider) = &self.provider
            && !provider.is_empty()
            && !self.model.is_empty()
        {
            let mut payload = json!({
                "sessionId": scope.session_id,
                "provider": provider,
                "model": self.model,
            });
            if let Some(effort) = &self.effort_id {
                payload["reasoningEffort"] = json!(effort);
            }
            let rpc = format!("select-model-{}", uuid::Uuid::new_v4());
            runtime.call("session.selectModel", payload, &rpc).await?;
        }
        if caps.supports("approvals.select")
            && let Some(mode) = &self.approval_mode
            && !mode.is_empty()
        {
            let rpc = format!("approvals-select-{}", uuid::Uuid::new_v4());
            runtime
                .call(
                    "approvals.select",
                    json!({
                        "sessionId": scope.session_id,
                        "mode": mode,
                    }),
                    &rpc,
                )
                .await?;
        }
        Ok(())
    }
}

/// Coordinates one `SessionRuntime` + `RunController` with UI containers via `UiWakeNotifier`.
pub struct RunWorkbenchBinding {
    runtime: Arc<SessionRuntime>,
    controller: Arc<RunController>,
    handle: Handle,
    wake: Arc<dyn UiWakeNotifier>,
    ui: Arc<Mutex<RunUiState>>,
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
        let lower_endpoint = endpoint_id.to_ascii_lowercase();
        let selected_harness = if lower_endpoint.contains("pi") {
            AVAILABLE_HARNESSES[1].to_owned()
        } else if lower_endpoint.contains("dsh") {
            AVAILABLE_HARNESSES[2].to_owned()
        } else {
            AVAILABLE_HARNESSES[0].to_owned()
        };
        Arc::new(Self {
            runtime,
            controller,
            handle,
            wake,
            ui: Arc::new(Mutex::new(RunUiState {
                endpoint_id,
                default_cwd: "/fixture/tekes-agentkit".to_owned(),
                live_kernel_tools: false,
                filter_mode: SidebarFilterMode::All,
                search_query: String::new(),
                selected_harness,
                selected_provider: None,
                selected_model: AVAILABLE_MODELS[0].to_owned(),
                selected_preset: AVAILABLE_PRESETS[0].to_owned(),
                selected_approval_value: Some("workspace-write".to_owned()),
                selected_effort: AVAILABLE_EFFORTS[1].to_owned(),
                models_catalog: Vec::new(),
                approval_options: Vec::new(),
                local_pins: BTreeSet::new(),
                local_titles: BTreeMap::new(),
                local_archived: BTreeSet::new(),
                host_summary_sections: Vec::new(),
                last_error: None,
                revision: 1,
            })),
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

    pub fn set_default_cwd(&self, cwd: impl Into<String>) {
        self.ui.lock().expect("run ui lock").default_cwd = cwd.into();
    }

    pub fn default_cwd(&self) -> String {
        self.ui.lock().expect("run ui lock").default_cwd.clone()
    }

    pub fn set_live_kernel_tools(&self, enabled: bool) {
        self.ui.lock().expect("run ui lock").live_kernel_tools = enabled;
    }

    pub fn available_models(&self) -> Vec<String> {
        let ui = self.ui.lock().expect("run ui lock");
        if ui.models_catalog.is_empty() {
            AVAILABLE_MODELS.iter().map(|s| (*s).to_owned()).collect()
        } else {
            ui.models_catalog.iter().map(|m| m.id.clone()).collect()
        }
    }

    pub fn available_efforts(&self) -> Vec<String> {
        let ui = self.ui.lock().expect("run ui lock");
        if let Some(entry) = ui.models_catalog.iter().find(|m| {
            m.id == ui.selected_model
                && ui
                    .selected_provider
                    .as_deref()
                    .is_none_or(|p| p == m.provider)
        }) {
            if entry.efforts.is_empty() {
                vec!["Default".to_owned()]
            } else {
                entry.efforts.iter().map(|e| effort_label(e)).collect()
            }
        } else {
            AVAILABLE_EFFORTS.iter().map(|s| (*s).to_owned()).collect()
        }
    }

    pub fn available_presets(&self) -> Vec<String> {
        let ui = self.ui.lock().expect("run ui lock");
        if ui.approval_options.is_empty() {
            AVAILABLE_PRESETS.iter().map(|s| (*s).to_owned()).collect()
        } else {
            ui.approval_options
                .iter()
                .map(|o| o.label.clone())
                .collect()
        }
    }

    pub fn refresh_controls(&self) {
        let runtime = Arc::clone(&self.runtime);
        let controller = Arc::clone(&self.controller);
        let ui = Arc::clone(&self.ui);
        let wake = Arc::clone(&self.wake);
        self.handle.spawn(async move {
            refresh_controls_async(runtime, controller, ui, wake).await;
        });
    }

    pub fn refresh_controls_blocking(&self) {
        let runtime = Arc::clone(&self.runtime);
        let controller = Arc::clone(&self.controller);
        let ui = Arc::clone(&self.ui);
        let wake = Arc::clone(&self.wake);
        self.handle.block_on(async move {
            refresh_controls_async(runtime, controller, ui, wake).await;
        });
    }

    fn current_preparation(&self) -> RunSubmissionPreparation {
        let ui = self.ui.lock().expect("run ui lock");
        let efforts = ui
            .models_catalog
            .iter()
            .find(|m| {
                m.id == ui.selected_model
                    && ui
                        .selected_provider
                        .as_deref()
                        .is_none_or(|p| p == m.provider)
            })
            .map(|m| m.efforts.as_slice())
            .unwrap_or(&[]);
        RunSubmissionPreparation {
            provider: ui.selected_provider.clone(),
            model: ui.selected_model.clone(),
            effort_id: effort_id_from_label(&ui.selected_effort, efforts),
            approval_mode: ui.selected_approval_value.clone(),
        }
    }

    pub fn select_session(&self, session_id: &str) {
        let endpoint_id = self.endpoint_id();
        let scope = RunScope::main(endpoint_id, session_id);
        let dest = ComposerDestination::run(scope);
        let _ = self.controller.select(dest);
        let runtime = Arc::clone(&self.runtime);
        let controller = Arc::clone(&self.controller);
        let ui = Arc::clone(&self.ui);
        let session_id = session_id.to_owned();
        let wake = Arc::clone(&self.wake);
        self.handle.spawn(async move {
            let _ = runtime.open_session(&session_id).await;
            refresh_controls_async(runtime, controller, ui, Arc::clone(&wake)).await;
            wake.wake();
        });
        self.bump_ui();
    }

    pub fn create_session(&self, cwd: &str) {
        let runtime = Arc::clone(&self.runtime);
        let controller = Arc::clone(&self.controller);
        let ui = Arc::clone(&self.ui);
        let wake = Arc::clone(&self.wake);
        let cwd = cwd.to_owned();
        self.handle.spawn(async move {
            if let Err(err) =
                create_session_async(runtime, controller, Arc::clone(&ui), Arc::clone(&wake), cwd)
                    .await
            {
                eprintln!("AgentKit create session failed: {err:#}");
                ui.lock().expect("run ui lock").last_error = Some(err.to_string());
                wake.wake();
            }
        });
    }

    pub fn create_session_blocking(&self, cwd: &str) -> Result<String> {
        let runtime = Arc::clone(&self.runtime);
        let controller = Arc::clone(&self.controller);
        let ui = Arc::clone(&self.ui);
        let wake = Arc::clone(&self.wake);
        let cwd = cwd.to_owned();
        self.handle
            .block_on(create_session_async(runtime, controller, ui, wake, cwd))
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
        let prep = self.current_preparation();
        let controller = Arc::clone(&self.controller);
        let ui = Arc::clone(&self.ui);
        let wake = Arc::clone(&self.wake);
        self.handle.spawn(async move {
            match controller.dispatch_with(ticket, None, &prep).await {
                Ok(_) => {
                    let mut guard = ui.lock().expect("run ui lock");
                    guard.last_error = None;
                    guard.revision = guard.revision.saturating_add(1);
                }
                Err(err) => {
                    eprintln!("AgentKit submit failed: {err:#}");
                    let mut guard = ui.lock().expect("run ui lock");
                    guard.last_error = Some(err.to_string());
                    guard.revision = guard.revision.saturating_add(1);
                }
            }
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

    fn commit_draft_setting(&self, key: &str, value: Value) {
        let snap = self.controller.snapshot();
        if let Some(token) = &snap.selection {
            let mut settings = snap.draft.settings.clone();
            settings.insert(key.to_owned(), value);
            self.controller.commit_settings(token, settings, false);
        }
        self.wake.wake();
    }

    pub fn selected_harness(&self) -> String {
        self.ui
            .lock()
            .expect("run ui lock")
            .selected_harness
            .clone()
    }

    pub fn set_harness(&self, index: usize) {
        let snap = self.controller.snapshot();
        // Established threads lock their Harness identity once transcript turns exist.
        if snap.run.is_some() && !snap.transcript.items.is_empty() {
            return;
        }
        if let Some(harness) = AVAILABLE_HARNESSES.get(index) {
            {
                let mut guard = self.ui.lock().expect("run ui lock");
                guard.selected_harness = (*harness).to_owned();
                guard.revision = guard.revision.saturating_add(1);
            }
            self.commit_draft_setting("harness", json!(*harness));
        }
    }

    pub fn selected_model(&self) -> String {
        self.ui.lock().expect("run ui lock").selected_model.clone()
    }

    pub fn set_model(&self, index: usize) {
        let models = self.available_models();
        let Some(model) = models.get(index).cloned() else {
            return;
        };
        {
            let mut guard = self.ui.lock().expect("run ui lock");
            guard.selected_model = model.clone();
            if let Some(entry) = guard.models_catalog.iter().find(|m| m.id == model).cloned() {
                guard.selected_provider = Some(entry.provider.clone());
                if entry.efforts.is_empty() {
                    guard.selected_effort = "Default".to_owned();
                } else {
                    let cur_valid = effort_id_from_label(&guard.selected_effort, &entry.efforts)
                        .is_some_and(|raw| entry.efforts.iter().any(|e| e == &raw));
                    if !cur_valid {
                        let next_raw = entry
                            .default_effort
                            .as_deref()
                            .or_else(|| entry.efforts.first().map(String::as_str))
                            .unwrap_or("medium");
                        guard.selected_effort = effort_label(next_raw);
                    }
                }
            }
            guard.revision = guard.revision.saturating_add(1);
        }
        self.commit_draft_setting("model", json!(model));
        self.sync_selected_model_to_session();
    }

    pub fn selected_preset(&self) -> String {
        self.ui.lock().expect("run ui lock").selected_preset.clone()
    }

    pub fn set_preset(&self, index: usize) {
        let presets = self.available_presets();
        let Some(preset) = presets.get(index).cloned() else {
            return;
        };
        let approval_value = {
            let mut guard = self.ui.lock().expect("run ui lock");
            guard.selected_preset = preset.clone();
            let raw_val = guard
                .approval_options
                .get(index)
                .map(|o| o.value.clone())
                .or_else(|| match preset.as_str() {
                    "Workspace Write" => Some("workspace-write".to_owned()),
                    "Read Only" => Some("read-only".to_owned()),
                    "Full Access" => Some("danger-full-access".to_owned()),
                    _ => None,
                });
            guard.selected_approval_value = raw_val.clone();
            guard.revision = guard.revision.saturating_add(1);
            raw_val
        };
        self.commit_draft_setting("preset", json!(preset));
        if let Some(mode) = approval_value
            && let Some(scope) = self.current_scope()
            && self
                .runtime
                .inspect(|s| s.endpoint_capabilities.supports("approvals.select"))
        {
            let runtime = Arc::clone(&self.runtime);
            let wake = Arc::clone(&self.wake);
            self.handle.spawn(async move {
                let rpc = format!("approvals-select-{}", uuid::Uuid::new_v4());
                let _ = runtime
                    .call(
                        "approvals.select",
                        json!({ "sessionId": scope.session_id, "mode": mode }),
                        &rpc,
                    )
                    .await;
                wake.wake();
            });
        }
    }

    pub fn selected_effort(&self) -> String {
        self.ui.lock().expect("run ui lock").selected_effort.clone()
    }

    pub fn set_effort(&self, index: usize) {
        let efforts = self.available_efforts();
        let Some(effort) = efforts.get(index).cloned() else {
            return;
        };
        let raw_effort = {
            let mut guard = self.ui.lock().expect("run ui lock");
            guard.selected_effort = effort.clone();
            guard.revision = guard.revision.saturating_add(1);
            let catalog_efforts = guard
                .models_catalog
                .iter()
                .find(|m| m.id == guard.selected_model)
                .map(|m| m.efforts.as_slice())
                .unwrap_or(&[]);
            effort_id_from_label(&effort, catalog_efforts)
        };
        if let Some(raw) = raw_effort {
            self.commit_draft_setting("reasoningEffort", json!(raw));
        }
        self.sync_selected_model_to_session();
    }

    fn sync_selected_model_to_session(&self) {
        let Some(scope) = self.current_scope() else {
            return;
        };
        if !self
            .runtime
            .inspect(|s| s.endpoint_capabilities.supports("session.selectModel"))
        {
            return;
        }
        let prep = self.current_preparation();
        let Some(provider) = prep.provider else {
            return;
        };
        if provider.is_empty() || prep.model.is_empty() {
            return;
        }
        let runtime = Arc::clone(&self.runtime);
        let wake = Arc::clone(&self.wake);
        self.handle.spawn(async move {
            let mut payload = json!({
                "sessionId": scope.session_id,
                "provider": provider,
                "model": prep.model,
            });
            if let Some(effort) = prep.effort_id {
                payload["reasoningEffort"] = json!(effort);
            }
            let rpc = format!("select-model-{}", uuid::Uuid::new_v4());
            let _ = runtime.call("session.selectModel", payload, &rpc).await;
            wake.wake();
        });
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

    pub fn connection_epoch(&self) -> u64 {
        self.runtime.inspect(|state| state.connection_epoch)
    }

    fn verify_queue_epoch(&self, expected_epoch: u64) -> Result<()> {
        self.runtime.inspect(|state| {
            tekes_agent_contract::require(
                state.connection == tekes_agent_session::ConnectionState::Ready
                    && state.connection_epoch == expected_epoch,
                "队列变更被拒绝：连接状态未就绪或 connectionEpoch 已过期",
            )
        })
    }

    /// Epoch-fenced queued-prompt removal (`session.updateQueue` Remove).
    pub fn remove_queued_prompt_at_epoch(
        &self,
        expected_epoch: u64,
        session_id: &str,
        item_id: &str,
    ) -> Result<()> {
        if let Err(err) = self.verify_queue_epoch(expected_epoch) {
            self.ui.lock().expect("run ui lock").last_error = Some(err.to_string());
            self.bump_ui();
            return Err(err);
        }
        let runtime = Arc::clone(&self.runtime);
        let runtime_check = Arc::clone(&self.runtime);
        let wake = Arc::clone(&self.wake);
        let sid = session_id.to_owned();
        let iid = item_id.to_owned();
        let rpc_id = format!("queue-rm-{}", uuid::Uuid::new_v4());
        let ui_ref = self.runtime.clone();
        let _ = ui_ref;
        self.handle.spawn(async move {
            let _ = runtime
                .call_optional_at_epoch_with_validation(
                    expected_epoch,
                    "session.updateQueue",
                    json!({
                        "sessionId": sid,
                        "itemId": iid,
                        "action": tekes_agent_contract::QueueAction::Remove,
                    }),
                    &rpc_id,
                    move || {
                        runtime_check.inspect(|state| {
                            tekes_agent_contract::require(
                                state.connection == tekes_agent_session::ConnectionState::Ready
                                    && state.connection_epoch == expected_epoch,
                                "stale connection epoch at queue removal admission",
                            )
                        })
                    },
                )
                .await;
            wake.wake();
        });
        self.bump_ui();
        Ok(())
    }

    /// Blocking variant of `remove_queued_prompt_at_epoch` for deterministic verification.
    pub fn remove_queued_prompt_at_epoch_blocking(
        &self,
        expected_epoch: u64,
        session_id: &str,
        item_id: &str,
    ) -> Result<()> {
        if let Err(err) = self.verify_queue_epoch(expected_epoch) {
            self.ui.lock().expect("run ui lock").last_error = Some(err.to_string());
            self.bump_ui();
            return Err(err);
        }
        let runtime = Arc::clone(&self.runtime);
        let runtime_check = Arc::clone(&self.runtime);
        let sid = session_id.to_owned();
        let iid = item_id.to_owned();
        let rpc_id = format!("queue-rm-{}", uuid::Uuid::new_v4());
        let res = self.handle.block_on(async move {
            runtime
                .call_optional_at_epoch_with_validation(
                    expected_epoch,
                    "session.updateQueue",
                    json!({
                        "sessionId": sid,
                        "itemId": iid,
                        "action": tekes_agent_contract::QueueAction::Remove,
                    }),
                    &rpc_id,
                    move || {
                        runtime_check.inspect(|state| {
                            tekes_agent_contract::require(
                                state.connection == tekes_agent_session::ConnectionState::Ready
                                    && state.connection_epoch == expected_epoch,
                                "stale connection epoch at queue removal admission",
                            )
                        })
                    },
                )
                .await
        });
        if let Err(err) = &res {
            self.ui.lock().expect("run ui lock").last_error = Some(err.to_string());
        } else {
            self.ui.lock().expect("run ui lock").last_error = None;
        }
        self.bump_ui();
        res.map(|_| ())
    }

    /// Epoch-fenced queued-prompt text edit (`session.updateQueue` Edit).
    pub fn edit_queued_prompt_at_epoch(
        &self,
        expected_epoch: u64,
        session_id: &str,
        item_id: &str,
        new_text: String,
    ) -> Result<()> {
        if let Err(err) = self.verify_queue_epoch(expected_epoch) {
            self.ui.lock().expect("run ui lock").last_error = Some(err.to_string());
            self.bump_ui();
            return Err(err);
        }
        let runtime = Arc::clone(&self.runtime);
        let runtime_check = Arc::clone(&self.runtime);
        let wake = Arc::clone(&self.wake);
        let sid = session_id.to_owned();
        let iid = item_id.to_owned();
        let rpc_id = format!("queue-edit-{}", uuid::Uuid::new_v4());
        let action = tekes_agent_contract::QueueAction::Edit {
            content: vec![json!({ "type": "text", "text": new_text })],
        };
        self.handle.spawn(async move {
            let _ = runtime
                .call_optional_at_epoch_with_validation(
                    expected_epoch,
                    "session.updateQueue",
                    json!({
                        "sessionId": sid,
                        "itemId": iid,
                        "action": action,
                    }),
                    &rpc_id,
                    move || {
                        runtime_check.inspect(|state| {
                            tekes_agent_contract::require(
                                state.connection == tekes_agent_session::ConnectionState::Ready
                                    && state.connection_epoch == expected_epoch,
                                "stale connection epoch at queue edit admission",
                            )
                        })
                    },
                )
                .await;
            wake.wake();
        });
        self.bump_ui();
        Ok(())
    }

    /// Blocking variant of `edit_queued_prompt_at_epoch` for deterministic verification.
    pub fn edit_queued_prompt_at_epoch_blocking(
        &self,
        expected_epoch: u64,
        session_id: &str,
        item_id: &str,
        new_text: String,
    ) -> Result<()> {
        if let Err(err) = self.verify_queue_epoch(expected_epoch) {
            self.ui.lock().expect("run ui lock").last_error = Some(err.to_string());
            self.bump_ui();
            return Err(err);
        }
        let runtime = Arc::clone(&self.runtime);
        let runtime_check = Arc::clone(&self.runtime);
        let sid = session_id.to_owned();
        let iid = item_id.to_owned();
        let rpc_id = format!("queue-edit-{}", uuid::Uuid::new_v4());
        let action = tekes_agent_contract::QueueAction::Edit {
            content: vec![json!({ "type": "text", "text": new_text })],
        };
        let res = self.handle.block_on(async move {
            runtime
                .call_optional_at_epoch_with_validation(
                    expected_epoch,
                    "session.updateQueue",
                    json!({
                        "sessionId": sid,
                        "itemId": iid,
                        "action": action,
                    }),
                    &rpc_id,
                    move || {
                        runtime_check.inspect(|state| {
                            tekes_agent_contract::require(
                                state.connection == tekes_agent_session::ConnectionState::Ready
                                    && state.connection_epoch == expected_epoch,
                                "stale connection epoch at queue edit admission",
                            )
                        })
                    },
                )
                .await
        });
        if let Err(err) = &res {
            self.ui.lock().expect("run ui lock").last_error = Some(err.to_string());
        } else {
            self.ui.lock().expect("run ui lock").last_error = None;
        }
        self.bump_ui();
        res.map(|_| ())
    }
}

async fn refresh_controls_async(
    runtime: Arc<SessionRuntime>,
    controller: Arc<RunController>,
    ui: Arc<Mutex<RunUiState>>,
    wake: Arc<dyn UiWakeNotifier>,
) {
    let caps = runtime.inspect(|s| s.endpoint_capabilities.clone());
    let session_id =
        controller
            .snapshot()
            .selection
            .as_ref()
            .and_then(|token| match &token.destination {
                ComposerDestination::Run { scope } => Some(scope.session_id.clone()),
                _ => None,
            });

    if caps.supports("session.models") || caps.supports("models.list") {
        let (method, payload) = if let Some(sid) = &session_id
            && caps.supports("session.models")
        {
            ("session.models", json!({ "sessionId": sid }))
        } else {
            ("models.list", json!({}))
        };
        let rpc_id = format!("models-list-{}", uuid::Uuid::new_v4());
        if let Ok(val) = runtime.call(method, payload, &rpc_id).await {
            let (entries, cur_provider, cur_model, cur_effort) = parse_models_catalog(&val);
            let mut guard = ui.lock().expect("run ui lock");
            if !entries.is_empty() {
                let matched = cur_model
                    .as_deref()
                    .and_then(|mid| {
                        entries.iter().find(|e| {
                            e.id == mid
                                && cur_provider.as_deref().is_none_or(|pid| e.provider == pid)
                        })
                    })
                    .or_else(|| {
                        entries.iter().find(|e| {
                            e.id == guard.selected_model
                                && guard
                                    .selected_provider
                                    .as_deref()
                                    .is_none_or(|pid| e.provider == pid)
                        })
                    })
                    .or_else(|| entries.first())
                    .cloned();
                guard.models_catalog = entries;
                if let Some(entry) = matched {
                    guard.selected_provider = Some(entry.provider.clone());
                    guard.selected_model = entry.id.clone();
                    if let Some(eff) = cur_effort
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .or(entry.default_effort.as_deref())
                        .or_else(|| entry.efforts.first().map(String::as_str))
                    {
                        guard.selected_effort = effort_label(eff);
                    } else if entry.efforts.is_empty() {
                        guard.selected_effort = "Default".to_owned();
                    }
                }
                guard.revision = guard.revision.saturating_add(1);
            }
        }
    }

    if caps.supports("approvals.policy") {
        let policy_payload = session_id
            .as_deref()
            .map_or_else(|| json!({}), |sid| json!({ "sessionId": sid }));
        let rpc_id = format!("approvals-policy-{}", uuid::Uuid::new_v4());
        if let Ok(policy) = runtime
            .call("approvals.policy", policy_payload, &rpc_id)
            .await
        {
            let mode_str = if let Some(sid) = &session_id
                && caps.supports("approvals.mode")
                && let Ok(mode_val) = runtime
                    .call(
                        "approvals.mode",
                        json!({ "sessionId": sid }),
                        &format!("approvals-mode-{}", uuid::Uuid::new_v4()),
                    )
                    .await
            {
                mode_val
                    .get("mode")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            } else {
                None
            };
            let fallback_selected = ui
                .lock()
                .expect("run ui lock")
                .selected_approval_value
                .clone();
            let chosen_mode = if session_id.is_some() {
                mode_str.as_deref().or(fallback_selected.as_deref())
            } else {
                fallback_selected.as_deref()
            };
            let (options, sel_val, sel_label) = parse_approval_policy(&policy, chosen_mode);
            if !options.is_empty() {
                let mut guard = ui.lock().expect("run ui lock");
                guard.approval_options = options;
                if let Some(val) = sel_val {
                    guard.selected_approval_value = Some(val);
                }
                if let Some(label) = sel_label {
                    guard.selected_preset = label;
                }
                guard.revision = guard.revision.saturating_add(1);
            }
        }
    }

    wake.wake();
}

async fn create_session_async(
    runtime: Arc<SessionRuntime>,
    controller: Arc<RunController>,
    ui: Arc<Mutex<RunUiState>>,
    wake: Arc<dyn UiWakeNotifier>,
    cwd: String,
) -> Result<String> {
    let (endpoint_id, live_tools, prep) = {
        let guard = ui.lock().expect("run ui lock");
        let efforts = guard
            .models_catalog
            .iter()
            .find(|m| {
                m.id == guard.selected_model
                    && guard
                        .selected_provider
                        .as_deref()
                        .is_none_or(|p| p == m.provider)
            })
            .map(|m| m.efforts.as_slice())
            .unwrap_or(&[]);
        (
            guard.endpoint_id.clone(),
            guard.live_kernel_tools,
            RunSubmissionPreparation {
                provider: guard.selected_provider.clone(),
                model: guard.selected_model.clone(),
                effort_id: effort_id_from_label(&guard.selected_effort, efforts),
                approval_mode: guard.selected_approval_value.clone(),
            },
        )
    };
    let rpc_id = format!("create-{}", uuid::Uuid::new_v4());
    let session_id = if live_tools {
        let res = runtime
            .call(
                "session.create",
                json!({
                    "execution": {
                        "cwd": cwd,
                        "read_roots": [cwd],
                        "writable_roots": [cwd],
                        "toolchain_roots": ["/bin", "/usr/bin", "/opt/homebrew/bin", "/usr/local/bin"],
                        "network": false,
                        "allowed_tools": ["read", "write", "edit", "shell"],
                        "mcp": []
                    }
                }),
                &rpc_id,
            )
            .await?;
        tekes_agent_contract::string(&res, "sessionId")?.to_owned()
    } else {
        runtime.create_session(&cwd, &rpc_id).await?
    };
    let caps = runtime.inspect(|s| s.endpoint_capabilities.clone());
    if caps.supports("session.selectModel")
        && let Some(provider) = &prep.provider
        && !provider.is_empty()
        && !prep.model.is_empty()
    {
        let mut payload = json!({
            "sessionId": session_id,
            "provider": provider,
            "model": prep.model,
        });
        if let Some(effort) = &prep.effort_id {
            payload["reasoningEffort"] = json!(effort);
        }
        let _ = runtime
            .call(
                "session.selectModel",
                payload,
                &format!("create-model-{}", uuid::Uuid::new_v4()),
            )
            .await;
    }
    if caps.supports("approvals.select")
        && let Some(mode) = &prep.approval_mode
        && !mode.is_empty()
    {
        let _ = runtime
            .call(
                "approvals.select",
                json!({ "sessionId": session_id, "mode": mode }),
                &format!("create-approval-{}", uuid::Uuid::new_v4()),
            )
            .await;
    }
    let scope = RunScope::main(endpoint_id, session_id.clone());
    let _ = controller.open(scope.clone()).await;
    let _ = controller.select(ComposerDestination::run(scope));
    refresh_controls_async(runtime, controller, Arc::clone(&ui), Arc::clone(&wake)).await;
    Ok(session_id)
}
