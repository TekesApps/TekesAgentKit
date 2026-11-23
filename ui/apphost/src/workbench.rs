//! Shared workbench coordinator and `WindowWake` hub for `tekes-ui-app-host` containers.
//! Enforces zero-sibling-borrow coordination across `nav_bar`, `focus`, and `others`.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use tekes_agent_contract::{ArtifactSelectionReference, SessionAddress};
use tekes_agent_ui_bindings::{
    ArtifactOpenRequest, ChildRunBinding, ChildRunOpenRequest, ComposerQuoteRequest,
    HarnessSettingsBinding, RunWorkbenchBinding, SideChatBinding, SideChatOpenRequest,
    UiWakeNotifier, WorkbenchCommand, WorkspaceBinding,
};
use tekes_ui_app_host::navigation;
use tekes_ui_platform_macos::{WakeResult, WindowWake};

/// Thread-safe registry of active `WindowWake` handles; safe to clone into Tokio tasks.
#[derive(Debug, Default)]
pub struct AppHostWakeHub {
    wakes: Mutex<Vec<WindowWake>>,
}

impl AppHostWakeHub {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn register(&self, wake: WindowWake) {
        let mut guard = self.wakes.lock().expect("wake hub lock poisoned");
        guard.push(wake);
    }

    pub fn active_wake_count(&self) -> usize {
        self.wakes.lock().expect("wake hub lock poisoned").len()
    }
}

impl UiWakeNotifier for AppHostWakeHub {
    fn wake(&self) {
        let mut guard = self.wakes.lock().expect("wake hub lock poisoned");
        guard.retain(|proxy| !matches!(proxy.wake(), WakeResult::Closed));
    }
}

struct WorkbenchInner {
    revision: u64,
    active_session: Option<SessionAddress>,
    active_workspace_id: Option<String>,
    run: Option<Arc<RunWorkbenchBinding>>,
    workspace: Option<Arc<WorkspaceBinding>>,
    artifact_targets: BTreeMap<String, String>,
    latest_quote: Option<ArtifactSelectionReference>,
    pending_navigation: VecDeque<navigation::Entry>,
    next_navigation_seq: u64,
    seen_navigation_ids: BTreeSet<String>,
    pending_commands: VecDeque<WorkbenchCommand>,
    pending_quotes: VecDeque<ComposerQuoteRequest>,
    pending_artifacts: VecDeque<ArtifactOpenRequest>,
    pending_child_runs: VecDeque<ChildRunOpenRequest>,
    pending_sidechats: VecDeque<SideChatOpenRequest>,
    child_runs: BTreeMap<String, Arc<ChildRunBinding>>,
    sidechats: BTreeMap<String, Arc<SideChatBinding>>,
    harness_settings: Option<Arc<HarnessSettingsBinding>>,
    background_text_updates: BTreeMap<String, String>,
}

/// Shared UI-thread coordinator passed into all AgentKit `AppHost` containers in a window.
/// Callers must never hold a `SharedAgentWorkbench` borrow while calling into a sibling `ContentHost`.
#[derive(Clone)]
pub struct SharedAgentWorkbench {
    inner: Rc<RefCell<WorkbenchInner>>,
    wake_hub: Arc<AppHostWakeHub>,
    background_revision: Arc<std::sync::atomic::AtomicU64>,
    background_note: Arc<Mutex<Option<String>>>,
    last_observed_bg_revision: Rc<Cell<u64>>,
    last_observed_run_revision: Rc<Cell<u64>>,
    last_observed_ws_revision: Rc<Cell<u64>>,
    last_observed_aux_revision: Rc<Cell<u64>>,
}

impl std::fmt::Debug for SharedAgentWorkbench {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedAgentWorkbench")
            .field("revision", &self.revision())
            .field("has_run", &self.inner.borrow().run.is_some())
            .finish_non_exhaustive()
    }
}

impl Default for SharedAgentWorkbench {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedAgentWorkbench {
    pub fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(WorkbenchInner {
                revision: 1,
                active_session: None,
                active_workspace_id: Some("ws-main".to_owned()),
                run: None,
                workspace: None,
                artifact_targets: BTreeMap::new(),
                latest_quote: None,
                pending_navigation: VecDeque::new(),
                next_navigation_seq: 1,
                seen_navigation_ids: BTreeSet::new(),
                pending_commands: VecDeque::new(),
                pending_quotes: VecDeque::new(),
                pending_artifacts: VecDeque::new(),
                pending_child_runs: VecDeque::new(),
                pending_sidechats: VecDeque::new(),
                child_runs: BTreeMap::new(),
                sidechats: BTreeMap::new(),
                harness_settings: None,
                background_text_updates: BTreeMap::new(),
            })),
            wake_hub: AppHostWakeHub::new(),
            background_revision: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            background_note: Arc::new(Mutex::new(None)),
            last_observed_bg_revision: Rc::new(Cell::new(0)),
            last_observed_run_revision: Rc::new(Cell::new(0)),
            last_observed_ws_revision: Rc::new(Cell::new(0)),
            last_observed_aux_revision: Rc::new(Cell::new(0)),
        }
    }

    pub fn wake_hub(&self) -> Arc<AppHostWakeHub> {
        Arc::clone(&self.wake_hub)
    }

    pub fn register_wake(&self, wake: WindowWake) {
        self.wake_hub.register(wake);
    }

    pub fn notify_wakes(&self) {
        self.wake_hub.wake();
    }

    pub fn attach_run_binding(&self, binding: Arc<RunWorkbenchBinding>) {
        let scope = binding.current_scope();
        let rev = binding.ui_revision();
        self.last_observed_run_revision.set(rev);
        {
            let mut inner = self.inner.borrow_mut();
            if let Some(s) = scope {
                inner.active_session = Some(s.address());
            }
            inner.run = Some(binding);
            inner.revision = inner.revision.saturating_add(1);
        }
        self.notify_wakes();
    }

    pub fn run_binding(&self) -> Option<Arc<RunWorkbenchBinding>> {
        self.inner.borrow().run.clone()
    }

    pub fn attach_workspace_binding(&self, binding: Arc<WorkspaceBinding>) {
        let ws_id = binding.workspace_id();
        let rev = binding.ui_revision();
        self.last_observed_ws_revision.set(rev);
        {
            let mut inner = self.inner.borrow_mut();
            inner.active_workspace_id = Some(ws_id);
            inner.workspace = Some(binding);
            inner.revision = inner.revision.saturating_add(1);
        }
        self.notify_wakes();
    }

    pub fn workspace_binding(&self) -> Option<Arc<WorkspaceBinding>> {
        self.inner.borrow().workspace.clone()
    }

    pub fn set_artifact_target(&self, instance_id: impl Into<String>, path: impl Into<String>) {
        let path = path.into();
        {
            let mut inner = self.inner.borrow_mut();
            inner
                .artifact_targets
                .insert(instance_id.into(), path.clone());
            inner.artifact_targets.insert("default".to_owned(), path);
            inner.revision = inner.revision.saturating_add(1);
        }
        self.notify_wakes();
    }

    pub fn artifact_target(&self, instance_id: &str) -> Option<String> {
        let inner = self.inner.borrow();
        inner
            .artifact_targets
            .get(instance_id)
            .or_else(|| inner.artifact_targets.get("default"))
            .cloned()
    }

    pub fn pop_pending_artifact(&self) -> Option<ArtifactOpenRequest> {
        self.inner.borrow_mut().pending_artifacts.pop_front()
    }

    pub fn attach_child_run_binding(
        &self,
        instance_id: impl Into<String>,
        binding: Arc<ChildRunBinding>,
    ) {
        let key = instance_id.into();
        {
            let mut inner = self.inner.borrow_mut();
            inner.child_runs.insert(key, binding);
            inner.revision = inner.revision.saturating_add(1);
        }
        self.notify_wakes();
    }

    pub fn child_run_binding(&self, instance_id: &str) -> Option<Arc<ChildRunBinding>> {
        let inner = self.inner.borrow();
        inner
            .child_runs
            .get(instance_id)
            .or_else(|| inner.child_runs.get("default"))
            .cloned()
    }

    pub fn pop_pending_child_run(&self) -> Option<ChildRunOpenRequest> {
        self.inner.borrow_mut().pending_child_runs.pop_front()
    }

    pub fn attach_sidechat_binding(
        &self,
        instance_id: impl Into<String>,
        binding: Arc<SideChatBinding>,
    ) {
        let key = instance_id.into();
        {
            let mut inner = self.inner.borrow_mut();
            inner.sidechats.insert(key, binding);
            inner.revision = inner.revision.saturating_add(1);
        }
        self.notify_wakes();
    }

    pub fn sidechat_binding(&self, instance_id: &str) -> Option<Arc<SideChatBinding>> {
        let inner = self.inner.borrow();
        inner
            .sidechats
            .get(instance_id)
            .or_else(|| inner.sidechats.get("default"))
            .cloned()
    }

    pub fn pop_pending_sidechat(&self) -> Option<SideChatOpenRequest> {
        self.inner.borrow_mut().pending_sidechats.pop_front()
    }

    pub fn attach_harness_settings_binding(&self, binding: Arc<HarnessSettingsBinding>) {
        {
            let mut inner = self.inner.borrow_mut();
            inner.harness_settings = Some(binding);
            inner.revision = inner.revision.saturating_add(1);
        }
        self.notify_wakes();
    }

    pub fn harness_settings_binding(&self) -> Option<Arc<HarnessSettingsBinding>> {
        self.inner.borrow().harness_settings.clone()
    }

    pub fn set_latest_quote(&self, quote: ArtifactSelectionReference) {
        {
            let mut inner = self.inner.borrow_mut();
            inner.latest_quote = Some(quote);
            inner.revision = inner.revision.saturating_add(1);
        }
        self.notify_wakes();
    }

    pub fn latest_quote(&self) -> Option<ArtifactSelectionReference> {
        self.inner.borrow().latest_quote.clone()
    }

    /// Reconcile any background-published state or async `RunController` / `WorkspaceBinding` changes into UI revision.
    pub fn sync_background_revision(&self) -> u64 {
        let bg_rev = self
            .background_revision
            .load(std::sync::atomic::Ordering::Acquire);
        let (run_opt, ws_opt, settings_opt, child_list, side_list) = {
            let inner = self.inner.borrow();
            (
                inner.run.clone(),
                inner.workspace.clone(),
                inner.harness_settings.clone(),
                inner.child_runs.values().cloned().collect::<Vec<_>>(),
                inner.sidechats.values().cloned().collect::<Vec<_>>(),
            )
        };
        let run_rev = run_opt.as_ref().map_or(0, |r| r.ui_revision());
        let ws_rev = ws_opt.as_ref().map_or(0, |w| w.ui_revision());
        let mut aux_rev = settings_opt.as_ref().map_or(0, |s| s.ui_revision());
        for c in child_list {
            aux_rev = aux_rev.saturating_add(c.view_snapshot().revision);
        }
        for s in side_list {
            aux_rev = aux_rev.saturating_add(s.view_snapshot().revision);
        }

        let bg_changed = bg_rev != self.last_observed_bg_revision.get();
        let run_changed = run_rev != self.last_observed_run_revision.get();
        let ws_changed = ws_rev != self.last_observed_ws_revision.get();
        let aux_changed = aux_rev != self.last_observed_aux_revision.get();

        if bg_changed || run_changed || ws_changed || aux_changed {
            self.last_observed_bg_revision.set(bg_rev);
            self.last_observed_run_revision.set(run_rev);
            self.last_observed_ws_revision.set(ws_rev);
            self.last_observed_aux_revision.set(aux_rev);
            let mut inner = self.inner.borrow_mut();
            inner.revision = inner.revision.saturating_add(1);
            if bg_changed
                && let Some(note) = self
                    .background_note
                    .lock()
                    .expect("background note lock")
                    .clone()
            {
                inner
                    .background_text_updates
                    .insert("background_status".to_owned(), note);
            }
            if let Some(scope_addr) = run_opt.as_ref().and_then(|r| r.active_session_address()) {
                inner.active_session = Some(scope_addr);
            }
            return inner.revision;
        }
        self.inner.borrow().revision
    }

    /// Deliver any pending Composer quotes/attachments into the active `RunWorkbenchBinding`.
    pub fn pump_composer_requests(&self) {
        let run = self.run_binding();
        let (quotes, attachments) = {
            let mut inner = self.inner.borrow_mut();
            let quotes: Vec<_> = inner.pending_quotes.drain(..).collect();
            let mut remaining = VecDeque::new();
            let mut attachments = Vec::new();
            for cmd in inner.pending_commands.drain(..) {
                match cmd {
                    WorkbenchCommand::AttachFileToComposer { destination, path } => {
                        attachments.push((destination, path));
                    }
                    other => remaining.push_back(other),
                }
            }
            inner.pending_commands = remaining;
            (quotes, attachments)
        };
        if let Some(run) = run {
            for q in quotes {
                self.set_latest_quote(q.reference.clone());
                run.apply_quote(q);
            }
            for (dest, path) in attachments {
                run.attach_file_path(dest, path);
            }
        }
    }

    /// Publish a status/note from any background thread and wake registered `AppWindow` handlers.
    pub fn publish_background_note_sender(&self) -> BackgroundNotePublisher {
        BackgroundNotePublisher {
            wake_hub: Arc::clone(&self.wake_hub),
            background_revision: Arc::clone(&self.background_revision),
            background_note: Arc::clone(&self.background_note),
        }
    }

    pub fn revision(&self) -> u64 {
        self.inner.borrow().revision
    }

    pub fn bump_revision(&self) -> u64 {
        let rev = {
            let mut inner = self.inner.borrow_mut();
            inner.revision = inner.revision.saturating_add(1);
            inner.revision
        };
        self.notify_wakes();
        rev
    }

    pub fn background_status(&self) -> Option<String> {
        self.inner
            .borrow()
            .background_text_updates
            .get("background_status")
            .cloned()
    }

    pub fn active_session(&self) -> Option<SessionAddress> {
        self.inner.borrow().active_session.clone()
    }

    fn allocate_unique_nav_id(inner: &mut WorkbenchInner, base: String) -> String {
        if !base.is_empty() && inner.seen_navigation_ids.insert(base.clone()) {
            return base;
        }
        loop {
            let seq = inner.next_navigation_seq;
            inner.next_navigation_seq = inner.next_navigation_seq.saturating_add(1);
            let candidate = if base.is_empty() {
                format!("visit-{seq}")
            } else {
                format!("{base}#{seq}")
            };
            if inner.seen_navigation_ids.insert(candidate.clone()) {
                return candidate;
            }
        }
    }

    pub fn set_active_session(&self, address: SessionAddress) {
        {
            let mut inner = self.inner.borrow_mut();
            let changed = inner.active_session.as_ref() != Some(&address);
            inner.active_session = Some(address.clone());
            if changed {
                inner.revision = inner.revision.saturating_add(1);
                let nav_id = loop {
                    let seq = inner.next_navigation_seq;
                    inner.next_navigation_seq = inner.next_navigation_seq.saturating_add(1);
                    let candidate =
                        format!("visit-{seq}:{}:{}", address.endpoint_id, address.session_id);
                    if inner.seen_navigation_ids.insert(candidate.clone()) {
                        break candidate;
                    }
                };
                inner.pending_navigation.push_back(navigation::Entry {
                    id: nav_id,
                    route: "agent-session".to_owned(),
                    params: serde_json::json!({
                        "endpointId": address.endpoint_id,
                        "sessionId": address.session_id,
                    }),
                });
            }
        }
        self.notify_wakes();
    }

    pub fn active_workspace_id(&self) -> Option<String> {
        self.inner.borrow().active_workspace_id.clone()
    }

    pub fn set_active_workspace_id(&self, workspace_id: Option<String>) {
        {
            let mut inner = self.inner.borrow_mut();
            if inner.active_workspace_id != workspace_id {
                inner.active_workspace_id = workspace_id;
                inner.revision = inner.revision.saturating_add(1);
            }
        }
        self.notify_wakes();
    }

    pub fn enqueue_navigation(&self, mut entry: navigation::Entry) {
        {
            let mut inner = self.inner.borrow_mut();
            let base = std::mem::take(&mut entry.id);
            entry.id = Self::allocate_unique_nav_id(&mut inner, base);
            inner.pending_navigation.push_back(entry);
            inner.revision = inner.revision.saturating_add(1);
        }
        self.notify_wakes();
    }

    pub fn pop_navigation(&self) -> Option<navigation::Entry> {
        self.inner.borrow_mut().pending_navigation.pop_front()
    }

    pub fn enqueue_command(&self, command: WorkbenchCommand) {
        {
            let mut inner = self.inner.borrow_mut();
            match &command {
                WorkbenchCommand::QuoteToComposer(req) => {
                    inner.pending_quotes.push_back(req.clone());
                }
                WorkbenchCommand::OpenArtifact(req) => {
                    inner.pending_artifacts.push_back(req.clone());
                }
                WorkbenchCommand::OpenChildRun(req) => {
                    inner.pending_child_runs.push_back(req.clone());
                }
                WorkbenchCommand::OpenSideChat(req) => {
                    inner.pending_sidechats.push_back(req.clone());
                }
                _ => {}
            }
            inner.pending_commands.push_back(command);
            inner.revision = inner.revision.saturating_add(1);
        }
        self.notify_wakes();
    }

    pub fn drain_commands(&self) -> Vec<WorkbenchCommand> {
        self.inner.borrow_mut().pending_commands.drain(..).collect()
    }

    pub fn drain_composer_quotes(&self) -> Vec<ComposerQuoteRequest> {
        self.inner.borrow_mut().pending_quotes.drain(..).collect()
    }

    pub fn drain_artifact_opens(&self) -> Vec<ArtifactOpenRequest> {
        self.inner
            .borrow_mut()
            .pending_artifacts
            .drain(..)
            .collect()
    }

    pub fn drain_child_run_opens(&self) -> Vec<ChildRunOpenRequest> {
        self.inner
            .borrow_mut()
            .pending_child_runs
            .drain(..)
            .collect()
    }

    pub fn drain_sidechat_opens(&self) -> Vec<SideChatOpenRequest> {
        self.inner
            .borrow_mut()
            .pending_sidechats
            .drain(..)
            .collect()
    }
}

/// `Send + Sync` background publisher handle that updates shared state and calls `WindowWake`.
#[derive(Clone, Debug)]
pub struct BackgroundNotePublisher {
    wake_hub: Arc<AppHostWakeHub>,
    background_revision: Arc<std::sync::atomic::AtomicU64>,
    background_note: Arc<Mutex<Option<String>>>,
}

impl BackgroundNotePublisher {
    pub fn publish(&self, note: impl Into<String>) {
        *self
            .background_note
            .lock()
            .expect("background note lock poisoned") = Some(note.into());
        self.background_revision
            .fetch_add(1, std::sync::atomic::Ordering::Release);
        self.wake_hub.wake();
    }
}
