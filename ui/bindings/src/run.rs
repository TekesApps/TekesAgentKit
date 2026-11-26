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
    ActionablePrompt, ComposerArtifact, ComposerDestination, RunController, RunScope, RunSnapshot,
    SummaryFacts, TranscriptDocument, thread_activity,
};
use tekes_agent_session::{ConnectionState, SessionHistoryLease, SessionRuntime};
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
struct SessionControlState {
    selected_provider: Option<String>,
    selected_model: String,
    selected_preset: String,
    selected_approval_value: Option<String>,
    selected_effort: String,
    models_catalog: Vec<ModelCatalogEntry>,
    approval_options: Vec<ApprovalModeOption>,
    model_generation: u64,
    approval_generation: u64,
}

impl Default for SessionControlState {
    fn default() -> Self {
        Self {
            selected_provider: None,
            selected_model: AVAILABLE_MODELS[0].to_owned(),
            selected_preset: AVAILABLE_PRESETS[0].to_owned(),
            selected_approval_value: Some("workspace-write".to_owned()),
            selected_effort: AVAILABLE_EFFORTS[1].to_owned(),
            models_catalog: Vec::new(),
            approval_options: Vec::new(),
            model_generation: 0,
            approval_generation: 0,
        }
    }
}

impl SessionControlState {
    fn to_preparation(&self) -> RunSubmissionPreparation {
        let efforts = self
            .models_catalog
            .iter()
            .find(|m| {
                m.id == self.selected_model
                    && self
                        .selected_provider
                        .as_deref()
                        .is_none_or(|p| p == m.provider)
            })
            .map(|m| m.efforts.as_slice())
            .unwrap_or(&[]);
        RunSubmissionPreparation {
            provider: self.selected_provider.clone(),
            model: self.selected_model.clone(),
            effort_id: effort_id_from_label(&self.selected_effort, efforts),
            approval_mode: self.selected_approval_value.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct CancelTurnTarget {
    turn_start_seq: i64,
    turn_id: Option<Value>,
}

#[derive(Clone, Debug, PartialEq)]
struct UncertainCancel {
    op_id: u64,
    target: CancelTurnTarget,
}

#[derive(Clone, Debug, PartialEq)]
struct ConfirmedCancel {
    op_id: u64,
    target: CancelTurnTarget,
}

#[derive(Default)]
struct SessionCancelTracking {
    in_flight_ops: BTreeSet<u64>,
    confirmed: Vec<ConfirmedCancel>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SessionErrorKind {
    Cancel,
    Other,
}

#[derive(Clone, Debug, PartialEq)]
struct SessionErrorEntry {
    seq: u64,
    kind: SessionErrorKind,
    cancel_op_id: Option<u64>,
    cancel_target: Option<CancelTurnTarget>,
    message: String,
    uncertain_cancel: Option<UncertainCancel>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TurnTerminal {
    Pending,
    ConfirmedAborted,
    ConfirmedOther,
}

struct RunUiState {
    endpoint_id: String,
    default_cwd: String,
    live_kernel_tools: bool,
    filter_mode: SidebarFilterMode,
    search_query: String,
    selected_harness: String,
    default_controls: SessionControlState,
    session_controls: BTreeMap<String, SessionControlState>,
    desired_history_session: Option<String>,
    history_lease: Option<SessionHistoryLease>,
    history_lease_generation: u64,
    published_lease_generation: u64,
    history_acquire_attempted: Option<(u64, u64, String)>,
    dropped: bool,
    local_pins: BTreeSet<String>,
    local_titles: BTreeMap<String, String>,
    local_archived: BTreeSet<String>,
    host_summary_sections: Vec<(String, Vec<(String, String)>)>,
    last_error: Option<String>,
    session_errors: BTreeMap<String, SessionErrorEntry>,
    next_error_seq: u64,
    next_cancel_op_id: u64,
    cancel_tracking: BTreeMap<String, SessionCancelTracking>,
    revision: u64,
}

impl RunUiState {
    fn begin_cancel_op(&mut self, session_id: &str) -> (u64, u64) {
        self.next_cancel_op_id = self.next_cancel_op_id.saturating_add(1);
        let op_id = self.next_cancel_op_id;
        let start_error_seq = self
            .session_errors
            .get(session_id)
            .map_or(0, |entry| entry.seq);
        self.cancel_tracking
            .entry(session_id.to_owned())
            .or_default()
            .in_flight_ops
            .insert(op_id);
        (op_id, start_error_seq)
    }

    fn record_confirmed_cancel(
        &mut self,
        session_id: &str,
        op_id: u64,
        cancel_target: &Option<CancelTurnTarget>,
    ) {
        let Some(target) = cancel_target else {
            return;
        };
        let Some(tracking) = self.cancel_tracking.get_mut(session_id) else {
            return;
        };
        if !tracking.in_flight_ops.iter().any(|&id| id < op_id) {
            return;
        }
        if let Some(existing) = tracking.confirmed.iter_mut().find(|c| &c.target == target) {
            existing.op_id = existing.op_id.max(op_id);
        } else {
            tracking.confirmed.push(ConfirmedCancel {
                op_id,
                target: target.clone(),
            });
        }
    }

    fn is_superseded_by_confirmed_cancel(
        &self,
        session_id: &str,
        op_id: u64,
        cancel_target: &Option<CancelTurnTarget>,
    ) -> bool {
        let Some(target) = cancel_target else {
            return false;
        };
        self.cancel_tracking
            .get(session_id)
            .is_some_and(|tracking| {
                tracking
                    .confirmed
                    .iter()
                    .any(|c| c.op_id > op_id && &c.target == target)
            })
    }

    fn finish_cancel_op(&mut self, session_id: &str, op_id: u64) {
        let Some(tracking) = self.cancel_tracking.get_mut(session_id) else {
            return;
        };
        tracking.in_flight_ops.remove(&op_id);
        if let Some(&min_in_flight) = tracking.in_flight_ops.first() {
            tracking.confirmed.retain(|c| c.op_id > min_in_flight);
        } else {
            self.cancel_tracking.remove(session_id);
        }
    }

    fn controls_for_read(&self, session_id: Option<&str>) -> &SessionControlState {
        session_id
            .and_then(|sid| self.session_controls.get(sid))
            .unwrap_or(&self.default_controls)
    }

    fn controls_for_mut(&mut self, session_id: Option<&str>) -> &mut SessionControlState {
        if let Some(sid) = session_id {
            let defaults = self.default_controls.clone();
            self.session_controls
                .entry(sid.to_owned())
                .or_insert(defaults)
        } else {
            &mut self.default_controls
        }
    }
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
    selection_publish: Arc<Mutex<()>>,
    _watcher: tokio::task::JoinHandle<()>,
}

impl Drop for RunWorkbenchBinding {
    fn drop(&mut self) {
        self._watcher.abort();
        let old_lease = self.ui.lock().ok().and_then(|mut guard| {
            guard.dropped = true;
            guard.desired_history_session = None;
            guard.history_acquire_attempted = None;
            guard.history_lease_generation = guard.history_lease_generation.saturating_add(1);
            guard.history_lease.take()
        });
        drop(old_lease);
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
        let lower_endpoint = endpoint_id.to_ascii_lowercase();
        let selected_harness = if lower_endpoint.contains("pi") {
            AVAILABLE_HARNESSES[1].to_owned()
        } else if lower_endpoint.contains("dsh") {
            AVAILABLE_HARNESSES[2].to_owned()
        } else {
            AVAILABLE_HARNESSES[0].to_owned()
        };
        let ui = Arc::new(Mutex::new(RunUiState {
            endpoint_id,
            default_cwd: "/fixture/tekes-agentkit".to_owned(),
            live_kernel_tools: false,
            filter_mode: SidebarFilterMode::All,
            search_query: String::new(),
            selected_harness,
            default_controls: SessionControlState::default(),
            session_controls: BTreeMap::new(),
            desired_history_session: None,
            history_lease: None,
            history_lease_generation: 0,
            published_lease_generation: 0,
            history_acquire_attempted: None,
            dropped: false,
            local_pins: BTreeSet::new(),
            local_titles: BTreeMap::new(),
            local_archived: BTreeSet::new(),
            host_summary_sections: Vec::new(),
            last_error: None,
            session_errors: BTreeMap::new(),
            next_error_seq: 0,
            next_cancel_op_id: 0,
            cancel_tracking: BTreeMap::new(),
            revision: 1,
        }));
        let selection_publish = Arc::new(Mutex::new(()));
        let mut run_rx = controller.subscribe();
        let mut store_rx = runtime.subscribe();
        let wake_clone = Arc::clone(&wake);
        let runtime_w = Arc::clone(&runtime);
        let controller_w = Arc::clone(&controller);
        let ui_weak = Arc::downgrade(&ui);
        let handle_w = handle.clone();
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
                        if let Some(ui_strong) = ui_weak.upgrade() {
                            try_spawn_history_acquire(
                                &runtime_w,
                                &controller_w,
                                &ui_strong,
                                &wake_clone,
                                &handle_w,
                            );
                            reconcile_uncertain_cancels(&runtime_w, &ui_strong);
                        } else {
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
            ui,
            selection_publish,
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

    pub fn session_error(&self, session_id: &str) -> Option<String> {
        self.ui
            .lock()
            .expect("run ui lock")
            .session_errors
            .get(session_id)
            .map(|entry| entry.message.clone())
    }

    /// Returns the active error to display for `session_id`: session-scoped failures
    /// first, falling back to endpoint-level failures (`last_error`).
    pub fn error_for_session(&self, session_id: Option<&str>) -> Option<String> {
        let guard = self.ui.lock().expect("run ui lock");
        session_id
            .and_then(|sid| {
                guard
                    .session_errors
                    .get(sid)
                    .map(|entry| entry.message.clone())
            })
            .or_else(|| guard.last_error.clone())
    }

    pub fn clear_session_error(&self, session_id: &str) {
        let removed = self
            .ui
            .lock()
            .expect("run ui lock")
            .session_errors
            .remove(session_id)
            .is_some();
        if removed {
            self.bump_ui();
        }
    }

    fn active_session_id(&self) -> Option<String> {
        self.current_scope().map(|scope| scope.session_id)
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

    /// Synchronously selects `session_id` and opens its history lease.
    ///
    /// Atomic on error: if acquiring history fails (for example,
    /// `Error::Disconnected`), the previous selection, existing history lease,
    /// and subscription intent remain unchanged without scheduling a deferred
    /// open. Use [`Self::select_session`] for non-blocking selection with
    /// automatic history acquisition on reconnect.
    pub fn open_initial_session(&self, session_id: &str) -> Result<()> {
        let scope = RunScope::main(self.endpoint_id(), session_id);
        scope.validate()?;
        let runtime = Arc::clone(&self.runtime);
        let sid = session_id.to_owned();
        let start_lease_gen = {
            let guard = self.ui.lock().expect("run ui lock");
            if guard.dropped {
                return Err(tekes_agent_contract::Error::Disconnected);
            }
            guard.history_lease_generation
        };
        let lease = self
            .handle
            .block_on(async move { runtime.acquire_history(&sid).await })?;
        let epoch = self.runtime.inspect(|s| s.connection_epoch);
        let pub_guard = self
            .selection_publish
            .lock()
            .expect("selection publish lock");
        let (lease_gen, prev_desired, prev_attempted, prev_lease) = {
            let mut guard = self.ui.lock().expect("run ui lock");
            if guard.dropped || guard.history_lease_generation != start_lease_gen {
                drop(guard);
                drop(pub_guard);
                let _ = self.handle.block_on(lease.release());
                return Err(tekes_agent_contract::Error::Disconnected);
            }
            let prev_desired = guard.desired_history_session.clone();
            let prev_attempted = guard.history_acquire_attempted.clone();
            guard.history_lease_generation = guard.history_lease_generation.saturating_add(1);
            let lease_gen = guard.history_lease_generation;
            guard.desired_history_session = Some(session_id.to_owned());
            guard.history_acquire_attempted = Some((lease_gen, epoch, session_id.to_owned()));
            guard.revision = guard.revision.saturating_add(1);
            let prev_lease = guard.history_lease.replace(lease);
            (lease_gen, prev_desired, prev_attempted, prev_lease)
        };
        if let Err(err) = publish_latest_selection_locked(&pub_guard, &self.ui, &self.controller) {
            let rolled_back_lease = {
                let mut guard = self.ui.lock().expect("run ui lock");
                if guard.history_lease_generation == lease_gen {
                    guard.desired_history_session = prev_desired;
                    guard.history_acquire_attempted = prev_attempted;
                    guard.published_lease_generation =
                        guard.published_lease_generation.max(lease_gen);
                    let current = guard.history_lease.take();
                    guard.history_lease = prev_lease;
                    current
                } else {
                    prev_lease
                }
            };
            drop(pub_guard);
            if let Some(rolled_back) = rolled_back_lease {
                let _ = self.handle.block_on(rolled_back.release());
            }
            return Err(err);
        }
        drop(pub_guard);
        drop(prev_lease);
        reconcile_uncertain_cancels(&self.runtime, &self.ui);
        self.wake.wake();
        Ok(())
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
        let active_sid = self.active_session_id();
        let ui = self.ui.lock().expect("run ui lock");
        let controls = ui.controls_for_read(active_sid.as_deref());
        if controls.models_catalog.is_empty() {
            AVAILABLE_MODELS.iter().map(|s| (*s).to_owned()).collect()
        } else {
            controls
                .models_catalog
                .iter()
                .map(|m| m.id.clone())
                .collect()
        }
    }

    pub fn available_efforts(&self) -> Vec<String> {
        let active_sid = self.active_session_id();
        let ui = self.ui.lock().expect("run ui lock");
        let controls = ui.controls_for_read(active_sid.as_deref());
        if let Some(entry) = controls.models_catalog.iter().find(|m| {
            m.id == controls.selected_model
                && controls
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
        let active_sid = self.active_session_id();
        let ui = self.ui.lock().expect("run ui lock");
        let controls = ui.controls_for_read(active_sid.as_deref());
        if controls.approval_options.is_empty() {
            AVAILABLE_PRESETS.iter().map(|s| (*s).to_owned()).collect()
        } else {
            controls
                .approval_options
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
        let target_session = self.active_session_id();
        self.handle.spawn(async move {
            refresh_controls_async(runtime, controller, ui, wake, target_session).await;
        });
    }

    pub fn refresh_controls_blocking(&self) {
        let runtime = Arc::clone(&self.runtime);
        let controller = Arc::clone(&self.controller);
        let ui = Arc::clone(&self.ui);
        let wake = Arc::clone(&self.wake);
        let target_session = self.active_session_id();
        self.handle.block_on(async move {
            refresh_controls_async(runtime, controller, ui, wake, target_session).await;
        });
    }

    fn preparation_for_destination(
        &self,
        destination: &ComposerDestination,
    ) -> RunSubmissionPreparation {
        let session_id = match destination {
            ComposerDestination::Run { scope } => Some(scope.session_id.as_str()),
            _ => None,
        };
        let ui = self.ui.lock().expect("run ui lock");
        ui.controls_for_read(session_id).to_preparation()
    }

    /// Updates selection immediately and schedules history acquisition when ready.
    ///
    /// Unlike [`Self::open_initial_session`], selection made while disconnected
    /// is preserved and acquires the selected session's history lease as soon as
    /// `SessionRuntime` becomes `Ready`.
    pub fn select_session(&self, session_id: &str) {
        let (need_acquire, old_lease) = {
            let mut guard = self.ui.lock().expect("run ui lock");
            if guard.dropped {
                return;
            }
            let scope = RunScope::main(guard.endpoint_id.clone(), session_id);
            if scope.validate().is_err() {
                return;
            }
            guard.history_lease_generation = guard.history_lease_generation.saturating_add(1);
            guard.desired_history_session = Some(session_id.to_owned());
            guard.history_acquire_attempted = None;
            guard.revision = guard.revision.saturating_add(1);
            let same_lease = guard
                .history_lease
                .as_ref()
                .is_some_and(|lease| lease.session_id() == session_id);
            let old_lease = if same_lease {
                None
            } else {
                guard.history_lease.take()
            };
            (!same_lease, old_lease)
        };
        drop(old_lease);
        let pub_guard = self
            .selection_publish
            .lock()
            .expect("selection publish lock");
        let _ = publish_latest_selection_locked(&pub_guard, &self.ui, &self.controller);
        drop(pub_guard);
        if need_acquire {
            try_spawn_history_acquire(
                &self.runtime,
                &self.controller,
                &self.ui,
                &self.wake,
                &self.handle,
            );
        } else {
            let runtime = Arc::clone(&self.runtime);
            let controller = Arc::clone(&self.controller);
            let ui = Arc::clone(&self.ui);
            let sid = session_id.to_owned();
            let wake = Arc::clone(&self.wake);
            self.handle.spawn(async move {
                refresh_controls_async(runtime, controller, ui, Arc::clone(&wake), Some(sid)).await;
                wake.wake();
            });
        }
        self.wake.wake();
    }

    pub fn create_session(&self, cwd: &str) {
        let runtime = Arc::clone(&self.runtime);
        let controller = Arc::clone(&self.controller);
        let ui = Arc::clone(&self.ui);
        let selection_publish = Arc::clone(&self.selection_publish);
        let wake = Arc::clone(&self.wake);
        let cwd = cwd.to_owned();
        self.handle.spawn(async move {
            match create_session_async(
                runtime,
                controller,
                Arc::clone(&ui),
                selection_publish,
                Arc::clone(&wake),
                cwd,
            )
            .await
            {
                Ok(_) => {
                    let mut guard = ui.lock().expect("run ui lock");
                    guard.last_error = None;
                    guard.revision = guard.revision.saturating_add(1);
                    drop(guard);
                    wake.wake();
                }
                Err(err) => {
                    eprintln!("AgentKit create session failed: {err:#}");
                    let mut guard = ui.lock().expect("run ui lock");
                    guard.last_error = Some(err.to_string());
                    guard.revision = guard.revision.saturating_add(1);
                    drop(guard);
                    wake.wake();
                }
            }
        });
    }

    pub fn create_session_blocking(&self, cwd: &str) -> Result<String> {
        let runtime = Arc::clone(&self.runtime);
        let controller = Arc::clone(&self.controller);
        let ui = Arc::clone(&self.ui);
        let selection_publish = Arc::clone(&self.selection_publish);
        let wake = Arc::clone(&self.wake);
        let cwd = cwd.to_owned();
        self.handle.block_on(create_session_async(
            runtime,
            controller,
            ui,
            selection_publish,
            wake,
            cwd,
        ))
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
        let artifact = ComposerArtifact::Reference {
            id: format!("file:{}", uuid::Uuid::new_v4()),
            mention: None,
            path: Some(path),
            excerpt: None,
        };
        if self.controller.append_artifact(&target, artifact).is_ok() {
            self.bump_ui();
        }
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
        if self
            .controller
            .append_annotation(&target, annotation)
            .is_ok()
        {
            self.bump_ui();
        }
    }

    pub fn remove_artifact(&self, destination: &ComposerDestination, artifact_id: &str) {
        if self
            .controller
            .remove_artifact(destination, artifact_id)
            .unwrap_or(false)
        {
            self.bump_ui();
        }
    }

    pub fn remove_annotation(&self, destination: &ComposerDestination, annotation_id: &str) {
        if self
            .controller
            .remove_annotation(destination, annotation_id)
            .unwrap_or(false)
        {
            self.bump_ui();
        }
    }

    pub fn submit_destination(&self, destination: &ComposerDestination) {
        let Ok(ticket) = self.controller.prepare(destination, PromptMode::Queue) else {
            return;
        };
        let prep = self.preparation_for_destination(destination);
        let target_session = match destination {
            ComposerDestination::Run { scope } => Some(scope.session_id.clone()),
            _ => None,
        };
        let start_error_seq = target_session
            .as_deref()
            .map(|sid| current_session_error_seq(&self.ui, sid))
            .unwrap_or(0);
        let controller = Arc::clone(&self.controller);
        let ui = Arc::clone(&self.ui);
        let wake = Arc::clone(&self.wake);
        self.handle.spawn(async move {
            match controller.dispatch_with(ticket, None, &prep).await {
                Ok(_) => {
                    if let Some(sid) = &target_session {
                        clear_session_error_if_not_newer(&ui, sid, start_error_seq);
                    } else {
                        let mut guard = ui.lock().expect("run ui lock");
                        guard.last_error = None;
                        guard.revision = guard.revision.saturating_add(1);
                    }
                }
                Err(err) => {
                    eprintln!("AgentKit submit failed: {err:#}");
                    let msg = err.to_string();
                    if let Some(sid) = &target_session {
                        record_session_error(&ui, sid, Some(msg));
                    } else {
                        let mut guard = ui.lock().expect("run ui lock");
                        guard.last_error = Some(msg);
                        guard.revision = guard.revision.saturating_add(1);
                    }
                }
            }
            wake.wake();
        });
        self.bump_ui();
    }

    pub fn cancel_scope(&self, scope: &RunScope) {
        let expected_epoch = self.runtime.inspect(|s| s.connection_epoch);
        self.cancel_scope_at_epoch(expected_epoch, scope);
    }

    pub fn cancel_scope_at_epoch(&self, expected_epoch: u64, scope: &RunScope) {
        let controller = Arc::clone(&self.controller);
        let runtime = Arc::clone(&self.runtime);
        let scope = scope.clone();
        let ui = Arc::clone(&self.ui);
        let wake = Arc::clone(&self.wake);
        let (epoch_matched, cancel_target) = self.runtime.inspect(|state| {
            let epoch_matched = state.connection == ConnectionState::Ready
                && state.connection_epoch == expected_epoch;
            let target = active_turn_target(state, &scope.session_id);
            (epoch_matched, target)
        });
        let (op_id, start_error_seq) = self
            .ui
            .lock()
            .expect("run ui lock")
            .begin_cancel_op(&scope.session_id);
        self.handle.spawn(async move {
            struct CancelOpGuard {
                ui: Arc<Mutex<RunUiState>>,
                session_id: String,
                op_id: u64,
            }
            impl Drop for CancelOpGuard {
                fn drop(&mut self) {
                    self.ui
                        .lock()
                        .expect("run ui lock")
                        .finish_cancel_op(&self.session_id, self.op_id);
                }
            }
            let _op_guard = CancelOpGuard {
                ui: Arc::clone(&ui),
                session_id: scope.session_id.clone(),
                op_id,
            };
            match controller.cancel_at_epoch(expected_epoch, &scope).await {
                Ok(()) => {
                    clear_superseded_cancel_error(&ui, &scope.session_id, op_id, &cancel_target);
                }
                Err(err) => {
                    let message = format!("Cancel failed: {err}");
                    let is_unconfirmed = matches!(
                        &err,
                        tekes_agent_contract::Error::Disconnected
                            | tekes_agent_contract::Error::Transport(_)
                            | tekes_agent_contract::Error::Timeout
                    );
                    if is_unconfirmed
                        && epoch_matched
                        && let Some(target) = cancel_target.clone()
                    {
                        match runtime
                            .inspect(|s| inspect_turn_terminal(s, &scope.session_id, &target))
                        {
                            TurnTerminal::ConfirmedAborted => {
                                clear_superseded_cancel_error(
                                    &ui,
                                    &scope.session_id,
                                    op_id,
                                    &cancel_target,
                                );
                            }
                            TurnTerminal::ConfirmedOther => {
                                record_cancel_error_if_not_newer(
                                    &ui,
                                    &scope.session_id,
                                    op_id,
                                    start_error_seq,
                                    Some(target),
                                    message,
                                    None,
                                );
                            }
                            TurnTerminal::Pending => {
                                let inserted_uncertain = record_cancel_error_if_not_newer(
                                    &ui,
                                    &scope.session_id,
                                    op_id,
                                    start_error_seq,
                                    Some(target.clone()),
                                    message,
                                    Some(UncertainCancel { op_id, target }),
                                );
                                if inserted_uncertain {
                                    reconcile_uncertain_cancels(&runtime, &ui);
                                }
                            }
                        }
                    } else {
                        record_cancel_error_if_not_newer(
                            &ui,
                            &scope.session_id,
                            op_id,
                            start_error_seq,
                            cancel_target,
                            message,
                            None,
                        );
                    }
                }
            }
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
            self.commit_draft_setting_on(Some(token), &snap.draft.settings, key, value);
        }
        self.wake.wake();
    }

    fn commit_draft_setting_on(
        &self,
        token: Option<&tekes_agent_presentation::SelectionToken>,
        base_settings: &BTreeMap<String, Value>,
        key: &str,
        value: Value,
    ) {
        if let Some(token) = token {
            let mut settings = base_settings.clone();
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
        let active_sid = self.active_session_id();
        self.ui
            .lock()
            .expect("run ui lock")
            .controls_for_read(active_sid.as_deref())
            .selected_model
            .clone()
    }

    pub fn set_model(&self, index: usize) {
        let models = self.available_models();
        let Some(model) = models.get(index).cloned() else {
            return;
        };
        let snap = self.controller.snapshot();
        let target_scope = snap.selection.as_ref().and_then(|t| match &t.destination {
            ComposerDestination::Run { scope } => Some(scope.clone()),
            _ => None,
        });
        let target_sid = target_scope.as_ref().map(|s| s.session_id.clone());
        let expected_epoch = self.runtime.inspect(|s| s.connection_epoch);
        let prep = {
            let mut guard = self.ui.lock().expect("run ui lock");
            let controls = guard.controls_for_mut(target_sid.as_deref());
            controls.selected_model = model.clone();
            controls.model_generation = controls.model_generation.saturating_add(1);
            if let Some(entry) = controls
                .models_catalog
                .iter()
                .find(|m| m.id == model)
                .cloned()
            {
                controls.selected_provider = Some(entry.provider.clone());
                if entry.efforts.is_empty() {
                    controls.selected_effort = "Default".to_owned();
                } else {
                    let cur_valid = effort_id_from_label(&controls.selected_effort, &entry.efforts)
                        .is_some_and(|raw| entry.efforts.iter().any(|e| e == &raw));
                    if !cur_valid {
                        let next_raw = entry
                            .default_effort
                            .as_deref()
                            .or_else(|| entry.efforts.first().map(String::as_str))
                            .unwrap_or("medium");
                        controls.selected_effort = effort_label(next_raw);
                    }
                }
            }
            let prep = controls.to_preparation();
            guard.revision = guard.revision.saturating_add(1);
            prep
        };
        self.commit_draft_setting_on(
            snap.selection.as_ref(),
            &snap.draft.settings,
            "model",
            json!(model),
        );
        if let Some(scope) = target_scope {
            self.sync_selected_model_to_session(scope, expected_epoch, prep);
        }
    }

    pub fn selected_preset(&self) -> String {
        let active_sid = self.active_session_id();
        self.ui
            .lock()
            .expect("run ui lock")
            .controls_for_read(active_sid.as_deref())
            .selected_preset
            .clone()
    }

    pub fn set_preset(&self, index: usize) {
        let presets = self.available_presets();
        let Some(preset) = presets.get(index).cloned() else {
            return;
        };
        let snap = self.controller.snapshot();
        let target_scope = snap.selection.as_ref().and_then(|t| match &t.destination {
            ComposerDestination::Run { scope } => Some(scope.clone()),
            _ => None,
        });
        let target_sid = target_scope.as_ref().map(|s| s.session_id.clone());
        let expected_epoch = self.runtime.inspect(|s| s.connection_epoch);
        let approval_value = {
            let mut guard = self.ui.lock().expect("run ui lock");
            let controls = guard.controls_for_mut(target_sid.as_deref());
            controls.selected_preset = preset.clone();
            controls.approval_generation = controls.approval_generation.saturating_add(1);
            let raw_val = controls
                .approval_options
                .get(index)
                .map(|o| o.value.clone())
                .or_else(|| match preset.as_str() {
                    "Workspace Write" => Some("workspace-write".to_owned()),
                    "Read Only" => Some("read-only".to_owned()),
                    "Full Access" => Some("danger-full-access".to_owned()),
                    _ => None,
                });
            controls.selected_approval_value = raw_val.clone();
            guard.revision = guard.revision.saturating_add(1);
            raw_val
        };
        self.commit_draft_setting_on(
            snap.selection.as_ref(),
            &snap.draft.settings,
            "preset",
            json!(preset),
        );
        if let Some(mode) = approval_value
            && let Some(scope) = target_scope
            && self
                .runtime
                .inspect(|s| s.endpoint_capabilities.supports("approvals.select"))
        {
            let runtime = Arc::clone(&self.runtime);
            let runtime_check = Arc::clone(&self.runtime);
            let wake = Arc::clone(&self.wake);
            self.handle.spawn(async move {
                let rpc = format!("approvals-select-{}", uuid::Uuid::new_v4());
                let _ = runtime
                    .call_optional_at_epoch_with_validation(
                        expected_epoch,
                        "approvals.select",
                        json!({ "sessionId": scope.session_id, "mode": mode }),
                        &rpc,
                        move || {
                            runtime_check.inspect(|state| {
                                tekes_agent_contract::require(
                                    state.connection == ConnectionState::Ready
                                        && state.connection_epoch == expected_epoch,
                                    "stale connection epoch at approval update",
                                )
                            })
                        },
                    )
                    .await;
                wake.wake();
            });
        }
    }

    pub fn selected_effort(&self) -> String {
        let active_sid = self.active_session_id();
        self.ui
            .lock()
            .expect("run ui lock")
            .controls_for_read(active_sid.as_deref())
            .selected_effort
            .clone()
    }

    pub fn set_effort(&self, index: usize) {
        let efforts = self.available_efforts();
        let Some(effort) = efforts.get(index).cloned() else {
            return;
        };
        let snap = self.controller.snapshot();
        let target_scope = snap.selection.as_ref().and_then(|t| match &t.destination {
            ComposerDestination::Run { scope } => Some(scope.clone()),
            _ => None,
        });
        let target_sid = target_scope.as_ref().map(|s| s.session_id.clone());
        let expected_epoch = self.runtime.inspect(|s| s.connection_epoch);
        let (raw_effort, prep) = {
            let mut guard = self.ui.lock().expect("run ui lock");
            let controls = guard.controls_for_mut(target_sid.as_deref());
            controls.selected_effort = effort.clone();
            controls.model_generation = controls.model_generation.saturating_add(1);
            let catalog_efforts = controls
                .models_catalog
                .iter()
                .find(|m| m.id == controls.selected_model)
                .map(|m| m.efforts.as_slice())
                .unwrap_or(&[]);
            let raw = effort_id_from_label(&effort, catalog_efforts);
            let prep = controls.to_preparation();
            guard.revision = guard.revision.saturating_add(1);
            (raw, prep)
        };
        if let Some(raw) = raw_effort {
            self.commit_draft_setting_on(
                snap.selection.as_ref(),
                &snap.draft.settings,
                "reasoningEffort",
                json!(raw),
            );
        }
        if let Some(scope) = target_scope {
            self.sync_selected_model_to_session(scope, expected_epoch, prep);
        }
    }

    fn sync_selected_model_to_session(
        &self,
        scope: RunScope,
        expected_epoch: u64,
        prep: RunSubmissionPreparation,
    ) {
        if !self
            .runtime
            .inspect(|s| s.endpoint_capabilities.supports("session.selectModel"))
        {
            return;
        }
        let Some(provider) = prep.provider else {
            return;
        };
        if provider.is_empty() || prep.model.is_empty() {
            return;
        }
        let runtime = Arc::clone(&self.runtime);
        let runtime_check = Arc::clone(&self.runtime);
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
            let _ = runtime
                .call_at_epoch_with_validation(
                    expected_epoch,
                    "session.selectModel",
                    payload,
                    &rpc,
                    move || {
                        runtime_check.inspect(|state| {
                            tekes_agent_contract::require(
                                state.connection == ConnectionState::Ready
                                    && state.connection_epoch == expected_epoch,
                                "stale connection epoch at model update",
                            )
                        })
                    },
                )
                .await;
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
            record_session_error(&self.ui, session_id, Some(err.to_string()));
            self.wake.wake();
            return Err(err);
        }
        let start_error_seq = current_session_error_seq(&self.ui, session_id);
        let runtime = Arc::clone(&self.runtime);
        let runtime_check = Arc::clone(&self.runtime);
        let ui = Arc::clone(&self.ui);
        let wake = Arc::clone(&self.wake);
        let sid = session_id.to_owned();
        let iid = item_id.to_owned();
        let rpc_id = format!("queue-rm-{}", uuid::Uuid::new_v4());
        self.handle.spawn(async move {
            let outcome = runtime
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
                .err()
                .map(|err| err.to_string());
            match outcome {
                Some(err) => record_session_error(&ui, &sid, Some(err)),
                None => clear_session_error_if_not_newer(&ui, &sid, start_error_seq),
            }
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
            record_session_error(&self.ui, session_id, Some(err.to_string()));
            self.wake.wake();
            return Err(err);
        }
        let start_error_seq = current_session_error_seq(&self.ui, session_id);
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
        match &res {
            Ok(_) => clear_session_error_if_not_newer(&self.ui, session_id, start_error_seq),
            Err(err) => record_session_error(&self.ui, session_id, Some(err.to_string())),
        }
        self.wake.wake();
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
            record_session_error(&self.ui, session_id, Some(err.to_string()));
            self.wake.wake();
            return Err(err);
        }
        let start_error_seq = current_session_error_seq(&self.ui, session_id);
        let runtime = Arc::clone(&self.runtime);
        let runtime_check = Arc::clone(&self.runtime);
        let ui = Arc::clone(&self.ui);
        let wake = Arc::clone(&self.wake);
        let sid = session_id.to_owned();
        let iid = item_id.to_owned();
        let rpc_id = format!("queue-edit-{}", uuid::Uuid::new_v4());
        let action = tekes_agent_contract::QueueAction::Edit {
            content: vec![json!({ "type": "text", "text": new_text })],
        };
        self.handle.spawn(async move {
            let outcome = runtime
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
                .err()
                .map(|err| err.to_string());
            match outcome {
                Some(err) => record_session_error(&ui, &sid, Some(err)),
                None => clear_session_error_if_not_newer(&ui, &sid, start_error_seq),
            }
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
            record_session_error(&self.ui, session_id, Some(err.to_string()));
            self.wake.wake();
            return Err(err);
        }
        let start_error_seq = current_session_error_seq(&self.ui, session_id);
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
        match &res {
            Ok(_) => clear_session_error_if_not_newer(&self.ui, session_id, start_error_seq),
            Err(err) => record_session_error(&self.ui, session_id, Some(err.to_string())),
        }
        self.wake.wake();
        res.map(|_| ())
    }
}

fn publish_latest_selection_locked(
    _pub_guard: &std::sync::MutexGuard<'_, ()>,
    ui: &Mutex<RunUiState>,
    controller: &RunController,
) -> Result<()> {
    loop {
        let (target_gen, dest) = {
            let guard = ui.lock().expect("run ui lock");
            if guard.dropped {
                return Err(tekes_agent_contract::Error::Disconnected);
            }
            if guard.published_lease_generation == guard.history_lease_generation {
                return Ok(());
            }
            let dest = guard.desired_history_session.as_ref().map(|sid| {
                ComposerDestination::run(RunScope::main(guard.endpoint_id.clone(), sid))
            });
            (guard.history_lease_generation, dest)
        };
        if let Some(dest) = dest {
            controller.select(dest)?;
        }
        let mut guard = ui.lock().expect("run ui lock");
        guard.published_lease_generation = guard.published_lease_generation.max(target_gen);
        if guard.dropped || guard.history_lease_generation == target_gen {
            return Ok(());
        }
    }
}

fn current_session_error_seq(ui: &Mutex<RunUiState>, session_id: &str) -> u64 {
    ui.lock()
        .expect("run ui lock")
        .session_errors
        .get(session_id)
        .map_or(0, |entry| entry.seq)
}

fn record_session_error(ui: &Mutex<RunUiState>, session_id: &str, error: Option<String>) {
    let mut guard = ui.lock().expect("run ui lock");
    match error {
        Some(message) => {
            guard.next_error_seq = guard.next_error_seq.saturating_add(1);
            let seq = guard.next_error_seq;
            guard.session_errors.insert(
                session_id.to_owned(),
                SessionErrorEntry {
                    seq,
                    kind: SessionErrorKind::Other,
                    cancel_op_id: None,
                    cancel_target: None,
                    message,
                    uncertain_cancel: None,
                },
            );
        }
        None => {
            guard.session_errors.remove(session_id);
        }
    }
    guard.revision = guard.revision.saturating_add(1);
}

fn clear_session_error_if_not_newer(
    ui: &Mutex<RunUiState>,
    session_id: &str,
    start_error_seq: u64,
) {
    let mut guard = ui.lock().expect("run ui lock");
    let can_clear = guard
        .session_errors
        .get(session_id)
        .is_some_and(|entry| entry.seq <= start_error_seq);
    if can_clear {
        guard.session_errors.remove(session_id);
        guard.revision = guard.revision.saturating_add(1);
    }
}

fn is_older_same_turn_cancel(
    entry: &SessionErrorEntry,
    op_id: u64,
    cancel_target: &Option<CancelTurnTarget>,
) -> bool {
    entry.kind == SessionErrorKind::Cancel
        && cancel_target.is_some()
        && entry.cancel_target == *cancel_target
        && entry
            .cancel_op_id
            .is_some_and(|existing_op_id| existing_op_id < op_id)
}

fn record_cancel_error_if_not_newer(
    ui: &Mutex<RunUiState>,
    session_id: &str,
    op_id: u64,
    start_error_seq: u64,
    cancel_target: Option<CancelTurnTarget>,
    message: String,
    uncertain_cancel: Option<UncertainCancel>,
) -> bool {
    let mut guard = ui.lock().expect("run ui lock");
    if guard.is_superseded_by_confirmed_cancel(session_id, op_id, &cancel_target) {
        return false;
    }
    if guard.session_errors.get(session_id).is_some_and(|entry| {
        entry.seq > start_error_seq && !is_older_same_turn_cancel(entry, op_id, &cancel_target)
    }) {
        return false;
    }
    guard.next_error_seq = guard.next_error_seq.saturating_add(1);
    let seq = guard.next_error_seq;
    let has_uncertain = uncertain_cancel.is_some();
    guard.session_errors.insert(
        session_id.to_owned(),
        SessionErrorEntry {
            seq,
            kind: SessionErrorKind::Cancel,
            cancel_op_id: Some(op_id),
            cancel_target,
            message,
            uncertain_cancel,
        },
    );
    guard.revision = guard.revision.saturating_add(1);
    has_uncertain
}

fn active_turn_target(
    store: &tekes_agent_session::MemoryProjectionStore,
    session_id: &str,
) -> Option<CancelTurnTarget> {
    let session = store.sessions.get(session_id)?;
    if !session.run_open {
        return None;
    }
    session
        .entries
        .values()
        .rev()
        .find_map(|entry| match entry.event.kind.as_str() {
            "turn/start" => Some(Some(CancelTurnTarget {
                turn_start_seq: entry.event.seq,
                turn_id: entry
                    .event
                    .data
                    .get("turn")
                    .filter(|v| !v.is_null())
                    .cloned(),
            })),
            "turn/end" => Some(None),
            _ => None,
        })
        .flatten()
}

fn clear_superseded_cancel_error(
    ui: &Mutex<RunUiState>,
    session_id: &str,
    op_id: u64,
    cancel_target: &Option<CancelTurnTarget>,
) -> bool {
    let mut guard = ui.lock().expect("run ui lock");
    let should_clear = guard.session_errors.get(session_id).is_some_and(|entry| {
        entry
            .uncertain_cancel
            .as_ref()
            .is_some_and(|uc| uc.op_id == op_id)
            || is_older_same_turn_cancel(entry, op_id, cancel_target)
    });
    if should_clear {
        guard.session_errors.remove(session_id);
        guard.revision = guard.revision.saturating_add(1);
    }
    guard.record_confirmed_cancel(session_id, op_id, cancel_target);
    should_clear
}

fn inspect_turn_terminal(
    store: &tekes_agent_session::MemoryProjectionStore,
    session_id: &str,
    target: &CancelTurnTarget,
) -> TurnTerminal {
    if store.connection != ConnectionState::Ready {
        return TurnTerminal::Pending;
    }
    let Some(session) = store.sessions.get(session_id) else {
        return TurnTerminal::Pending;
    };
    if !session.journal_ready || session.journal_error.is_some() {
        return TurnTerminal::Pending;
    }
    for (_, entry) in session.entries.range((target.turn_start_seq + 1)..) {
        match entry.event.kind.as_str() {
            "turn/end" => {
                if let Some(expected_turn) = &target.turn_id
                    && entry.event.data.get("turn") != Some(expected_turn)
                {
                    return TurnTerminal::ConfirmedOther;
                }
                let reason = entry.event.data["reason"]
                    .as_str()
                    .or_else(|| entry.event.data["reason"]["kind"].as_str());
                return if reason == Some("aborted") {
                    TurnTerminal::ConfirmedAborted
                } else {
                    TurnTerminal::ConfirmedOther
                };
            }
            "turn/start" => return TurnTerminal::ConfirmedOther,
            _ => {}
        }
    }
    TurnTerminal::Pending
}

fn reconcile_uncertain_cancels(runtime: &SessionRuntime, ui: &Mutex<RunUiState>) -> bool {
    let pending: Vec<(String, UncertainCancel)> = {
        let guard = ui.lock().expect("run ui lock");
        guard
            .session_errors
            .iter()
            .filter_map(|(sid, entry)| {
                entry
                    .uncertain_cancel
                    .as_ref()
                    .map(|uc| (sid.clone(), uc.clone()))
            })
            .collect()
    };
    if pending.is_empty() {
        return false;
    }
    let evaluated: Vec<(String, UncertainCancel, TurnTerminal)> = runtime.inspect(|store| {
        pending
            .into_iter()
            .map(|(sid, uc)| {
                let terminal = inspect_turn_terminal(store, &sid, &uc.target);
                (sid, uc, terminal)
            })
            .collect()
    });
    let mut guard = ui.lock().expect("run ui lock");
    let mut changed = false;
    for (sid, uc, terminal) in evaluated {
        if terminal == TurnTerminal::Pending {
            continue;
        }
        let matches = guard
            .session_errors
            .get(&sid)
            .is_some_and(|entry| entry.uncertain_cancel.as_ref() == Some(&uc));
        if !matches {
            continue;
        }
        match terminal {
            TurnTerminal::ConfirmedAborted => {
                guard.session_errors.remove(&sid);
                guard.record_confirmed_cancel(&sid, uc.op_id, &Some(uc.target));
                changed = true;
            }
            TurnTerminal::ConfirmedOther => {
                if let Some(entry) = guard.session_errors.get_mut(&sid) {
                    entry.uncertain_cancel = None;
                }
            }
            TurnTerminal::Pending => {}
        }
    }
    if changed {
        guard.revision = guard.revision.saturating_add(1);
    }
    changed
}

fn try_spawn_history_acquire(
    runtime: &Arc<SessionRuntime>,
    controller: &Arc<RunController>,
    ui: &Arc<Mutex<RunUiState>>,
    wake: &Arc<dyn UiWakeNotifier>,
    handle: &Handle,
) {
    let (is_ready, epoch) =
        runtime.inspect(|s| (s.connection == ConnectionState::Ready, s.connection_epoch));
    if !is_ready {
        return;
    }
    let (sid, lease_gen) = {
        let mut guard = ui.lock().expect("run ui lock");
        if guard.dropped {
            return;
        }
        let Some(sid) = guard.desired_history_session.clone() else {
            return;
        };
        let has_lease = guard
            .history_lease
            .as_ref()
            .is_some_and(|lease| lease.session_id() == sid);
        let lease_gen = guard.history_lease_generation;
        let key = (lease_gen, epoch, sid.clone());
        if has_lease
            || guard.published_lease_generation != lease_gen
            || guard.history_acquire_attempted.as_ref() == Some(&key)
        {
            return;
        }
        guard.history_acquire_attempted = Some(key);
        (sid, lease_gen)
    };
    let runtime = Arc::clone(runtime);
    let controller = Arc::clone(controller);
    let ui = Arc::clone(ui);
    let wake = Arc::clone(wake);
    handle.spawn(async move {
        if let Ok(lease) = runtime.acquire_history(&sid).await {
            let (installed, displaced) = {
                let mut guard = ui.lock().expect("run ui lock");
                if !guard.dropped
                    && guard.history_lease_generation == lease_gen
                    && guard.desired_history_session.as_deref() == Some(sid.as_str())
                {
                    let old = guard.history_lease.replace(lease);
                    guard.revision = guard.revision.saturating_add(1);
                    (true, old)
                } else {
                    (false, Some(lease))
                }
            };
            drop(displaced);
            reconcile_uncertain_cancels(&runtime, &ui);
            if installed {
                refresh_controls_async(runtime, controller, ui, Arc::clone(&wake), Some(sid)).await;
            }
        }
        wake.wake();
    });
}

fn current_selected_session_id(controller: &RunController) -> Option<String> {
    controller
        .snapshot()
        .selection
        .as_ref()
        .and_then(|token| match &token.destination {
            ComposerDestination::Run { scope } => Some(scope.session_id.clone()),
            _ => None,
        })
}

async fn refresh_controls_async(
    runtime: Arc<SessionRuntime>,
    controller: Arc<RunController>,
    ui: Arc<Mutex<RunUiState>>,
    wake: Arc<dyn UiWakeNotifier>,
    target_session: Option<String>,
) {
    let caps = runtime.inspect(|s| s.endpoint_capabilities.clone());
    let start_epoch = runtime.inspect(|s| s.connection_epoch);
    let session_id = target_session.or_else(|| current_selected_session_id(&controller));
    let (start_model_gen, start_approval_gen) = {
        let guard = ui.lock().expect("run ui lock");
        let controls = guard.controls_for_read(session_id.as_deref());
        (controls.model_generation, controls.approval_generation)
    };
    let mut should_wake = false;

    if caps.supports("session.models") || caps.supports("models.list") {
        let (method, payload) = if let Some(sid) = &session_id
            && caps.supports("session.models")
        {
            ("session.models", json!({ "sessionId": sid }))
        } else {
            ("models.list", json!({}))
        };
        let rpc_id = format!("models-list-{}", uuid::Uuid::new_v4());
        if let Ok(val) = runtime.call(method, payload, &rpc_id).await
            && (start_epoch == 0 || runtime.inspect(|s| s.connection_epoch == start_epoch))
        {
            let (entries, cur_provider, cur_model, cur_effort) = parse_models_catalog(&val);
            if !entries.is_empty() {
                let active_now = current_selected_session_id(&controller);
                let mut guard = ui.lock().expect("run ui lock");
                let cur_controls = guard.controls_for_read(session_id.as_deref()).clone();
                if cur_controls.model_generation == start_model_gen {
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
                                e.id == cur_controls.selected_model
                                    && cur_controls
                                        .selected_provider
                                        .as_deref()
                                        .is_none_or(|pid| e.provider == pid)
                            })
                        })
                        .or_else(|| entries.first())
                        .cloned();
                    let controls = guard.controls_for_mut(session_id.as_deref());
                    controls.models_catalog = entries;
                    if let Some(entry) = matched {
                        controls.selected_provider = Some(entry.provider.clone());
                        controls.selected_model = entry.id.clone();
                        if let Some(eff) = cur_effort
                            .as_deref()
                            .filter(|s| !s.is_empty())
                            .or(entry.default_effort.as_deref())
                            .or_else(|| entry.efforts.first().map(String::as_str))
                        {
                            controls.selected_effort = effort_label(eff);
                        } else if entry.efforts.is_empty() {
                            controls.selected_effort = "Default".to_owned();
                        }
                    }
                    if session_id.is_none() || active_now == session_id {
                        guard.revision = guard.revision.saturating_add(1);
                        should_wake = true;
                    }
                }
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
            && (start_epoch == 0 || runtime.inspect(|s| s.connection_epoch == start_epoch))
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
            if start_epoch == 0 || runtime.inspect(|s| s.connection_epoch == start_epoch) {
                let fallback_selected = {
                    let guard = ui.lock().expect("run ui lock");
                    guard
                        .controls_for_read(session_id.as_deref())
                        .selected_approval_value
                        .clone()
                };
                let chosen_mode = if session_id.is_some() {
                    mode_str.as_deref().or(fallback_selected.as_deref())
                } else {
                    fallback_selected.as_deref()
                };
                let (options, sel_val, sel_label) = parse_approval_policy(&policy, chosen_mode);
                if !options.is_empty() {
                    let active_now = current_selected_session_id(&controller);
                    let mut guard = ui.lock().expect("run ui lock");
                    if guard
                        .controls_for_read(session_id.as_deref())
                        .approval_generation
                        == start_approval_gen
                    {
                        let controls = guard.controls_for_mut(session_id.as_deref());
                        controls.approval_options = options;
                        if let Some(val) = sel_val {
                            controls.selected_approval_value = Some(val);
                        }
                        if let Some(label) = sel_label {
                            controls.selected_preset = label;
                        }
                        if session_id.is_none() || active_now == session_id {
                            guard.revision = guard.revision.saturating_add(1);
                            should_wake = true;
                        }
                    }
                }
            }
        }
    }

    if should_wake || session_id.is_none() {
        wake.wake();
    }
}

async fn create_session_async(
    runtime: Arc<SessionRuntime>,
    controller: Arc<RunController>,
    ui: Arc<Mutex<RunUiState>>,
    selection_publish: Arc<Mutex<()>>,
    wake: Arc<dyn UiWakeNotifier>,
    cwd: String,
) -> Result<String> {
    let (endpoint_id, live_tools, default_controls_snap, prep) = {
        let guard = ui.lock().expect("run ui lock");
        (
            guard.endpoint_id.clone(),
            guard.live_kernel_tools,
            guard.default_controls.clone(),
            guard.default_controls.to_preparation(),
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
    {
        let mut guard = ui.lock().expect("run ui lock");
        guard
            .session_controls
            .entry(session_id.clone())
            .or_insert(default_controls_snap);
    }
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
    let epoch = runtime.inspect(|s| s.connection_epoch);
    let (lease_gen, old_lease) = {
        let mut guard = ui.lock().expect("run ui lock");
        if guard.dropped {
            return Err(tekes_agent_contract::Error::Disconnected);
        }
        RunScope::main(endpoint_id, session_id.clone()).validate()?;
        guard.history_lease_generation = guard.history_lease_generation.saturating_add(1);
        let lease_gen = guard.history_lease_generation;
        guard.desired_history_session = Some(session_id.clone());
        guard.history_acquire_attempted = Some((lease_gen, epoch, session_id.clone()));
        guard.revision = guard.revision.saturating_add(1);
        let old = if guard
            .history_lease
            .as_ref()
            .is_some_and(|l| l.session_id() == session_id)
        {
            None
        } else {
            guard.history_lease.take()
        };
        (lease_gen, old)
    };
    drop(old_lease);
    {
        let pub_guard = selection_publish.lock().expect("selection publish lock");
        let _ = publish_latest_selection_locked(&pub_guard, &ui, &controller);
    }
    if let Ok(lease) = runtime.acquire_history(&session_id).await {
        let displaced = {
            let mut guard = ui.lock().expect("run ui lock");
            if !guard.dropped
                && guard.history_lease_generation == lease_gen
                && guard.desired_history_session.as_deref() == Some(session_id.as_str())
            {
                guard.history_lease.replace(lease)
            } else {
                Some(lease)
            }
        };
        drop(displaced);
    }
    refresh_controls_async(
        runtime,
        controller,
        Arc::clone(&ui),
        Arc::clone(&wake),
        Some(session_id.clone()),
    )
    .await;
    Ok(session_id)
}
