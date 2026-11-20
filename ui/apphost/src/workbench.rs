//! Shared workbench coordinator and `WindowWake` hub for `tekes-ui-app-host` containers.
//! Enforces zero-sibling-borrow coordination across `nav_bar`, `focus`, and `others`.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use tekes_agent_contract::SessionAddress;
use tekes_agent_ui_bindings::{
    ArtifactOpenRequest, ChildRunOpenRequest, ComposerQuoteRequest, SideChatOpenRequest,
    UiWakeNotifier, WorkbenchCommand,
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

#[derive(Debug)]
struct WorkbenchInner {
    revision: u64,
    active_session: Option<SessionAddress>,
    active_workspace_id: Option<String>,
    pending_navigation: VecDeque<navigation::Entry>,
    pending_commands: VecDeque<WorkbenchCommand>,
    pending_quotes: VecDeque<ComposerQuoteRequest>,
    pending_artifacts: VecDeque<ArtifactOpenRequest>,
    pending_child_runs: VecDeque<ChildRunOpenRequest>,
    pending_sidechats: VecDeque<SideChatOpenRequest>,
    background_text_updates: BTreeMapString,
}

type BTreeMapString = std::collections::BTreeMap<String, String>;

/// Shared UI-thread coordinator passed into all AgentKit `AppHost` containers in a window.
/// Callers must never hold a `SharedAgentWorkbench` borrow while calling into a sibling `ContentHost`.
#[derive(Clone, Debug)]
pub struct SharedAgentWorkbench {
    inner: Rc<RefCell<WorkbenchInner>>,
    wake_hub: Arc<AppHostWakeHub>,
    background_revision: Arc<std::sync::atomic::AtomicU64>,
    background_note: Arc<Mutex<Option<String>>>,
    last_observed_bg_revision: Rc<Cell<u64>>,
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
                active_workspace_id: None,
                pending_navigation: VecDeque::new(),
                pending_commands: VecDeque::new(),
                pending_quotes: VecDeque::new(),
                pending_artifacts: VecDeque::new(),
                pending_child_runs: VecDeque::new(),
                pending_sidechats: VecDeque::new(),
                background_text_updates: BTreeMapString::new(),
            })),
            wake_hub: AppHostWakeHub::new(),
            background_revision: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            background_note: Arc::new(Mutex::new(None)),
            last_observed_bg_revision: Rc::new(Cell::new(0)),
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

    /// Reconcile any background-published state into UI revision counter without blocking callers.
    pub fn sync_background_revision(&self) -> u64 {
        let bg_rev = self
            .background_revision
            .load(std::sync::atomic::Ordering::Acquire);
        if bg_rev != self.last_observed_bg_revision.get() {
            self.last_observed_bg_revision.set(bg_rev);
            let mut inner = self.inner.borrow_mut();
            inner.revision = inner.revision.saturating_add(1);
            if let Some(note) = self
                .background_note
                .lock()
                .expect("background note lock")
                .clone()
            {
                inner
                    .background_text_updates
                    .insert("background_status".to_owned(), note);
            }
            return inner.revision;
        }
        self.inner.borrow().revision
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

    pub fn set_active_session(&self, address: SessionAddress) {
        {
            let mut inner = self.inner.borrow_mut();
            let changed = inner.active_session.as_ref() != Some(&address);
            inner.active_session = Some(address.clone());
            if changed {
                inner.revision = inner.revision.saturating_add(1);
                inner.pending_navigation.push_back(navigation::Entry {
                    id: format!("session:{}:{}", address.endpoint_id, address.session_id),
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

    pub fn enqueue_navigation(&self, entry: navigation::Entry) {
        {
            let mut inner = self.inner.borrow_mut();
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
