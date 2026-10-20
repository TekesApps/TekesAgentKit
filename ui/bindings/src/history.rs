//! Per-mount read receipts. Canonical history and page merging stay in SessionRuntime/ChildRun.
use super::{PaneAccess, RunBinding};
use std::sync::{Arc, Mutex};
use tekes_agent_contract::{Result, require};
use tekes_agent_presentation::RunScope;
use tokio::sync::watch;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryPagePhase {
    Idle,
    Loading,
    Loaded {
        extended: bool,
    },
    Failed {
        message: String,
    },
    /// Dropping a request/future does not prove that its backend read was cancelled.
    Interrupted,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryPageSnapshot {
    pub scope: RunScope,
    pub revision: u64,
    pub connection_epoch: u64,
    pub request_epoch: Option<u64>,
    /// Also observe the binding's runtime/pointer publications. A disconnected or
    /// expired receipt must not drive another pane's loading/error indicator.
    pub is_current: bool,
    pub phase: HistoryPagePhase,
}
struct PageState {
    revision: u64,
    epoch: Option<u64>,
    active: Option<u64>,
    phase: HistoryPagePhase,
}
#[derive(Clone)]
pub(super) struct HistoryPages {
    state: Arc<Mutex<PageState>>,
    changes: watch::Sender<u64>,
}
impl Default for HistoryPages {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(PageState {
                revision: 0,
                epoch: None,
                active: None,
                phase: HistoryPagePhase::Idle,
            })),
            changes: watch::channel(0).0,
        }
    }
}
pub(super) struct HistoryPageLease {
    pages: HistoryPages,
    epoch: u64,
    id: u64,
    finished: bool,
}
impl HistoryPageLease {
    fn current(&self, binding: &RunBinding) -> bool {
        let state = self.pages.state.lock().unwrap();
        state.active == Some(self.id)
            && state.epoch == Some(self.epoch)
            && binding.access() != PaneAccess::Missing
            && binding
                .runtime
                .inspect(|store| store.ready() && store.connection_epoch == self.epoch)
    }
    fn finish(&mut self, phase: HistoryPagePhase) {
        let mut state = self.pages.state.lock().unwrap();
        if state.active == Some(self.id) && state.epoch == Some(self.epoch) {
            state.phase = phase;
            state.active = None;
            state.revision = state.revision.saturating_add(1);
            self.pages.changes.send_replace(state.revision);
        }
        self.finished = true;
    }
}
impl Drop for HistoryPageLease {
    fn drop(&mut self) {
        if !self.finished {
            self.finish(HistoryPagePhase::Interrupted);
        }
    }
}
/// One captured gesture, consumed once. It owns its original binding; no lookup
/// through current host navigation occurs on dispatch or completion.
pub struct HistoryPageRequest {
    binding: Arc<RunBinding>,
    lease: HistoryPageLease,
}
impl HistoryPageRequest {
    pub fn scope(&self) -> &RunScope {
        self.binding.scope()
    }
    pub async fn load(self) -> Result<bool> {
        self.binding.load_history_page(self.lease).await
    }
}
impl RunBinding {
    pub fn history_page_state(&self) -> HistoryPageSnapshot {
        let (epoch, ready) = self
            .runtime
            .inspect(|store| (store.connection_epoch, store.ready()));
        let state = self.history_pages.state.lock().unwrap();
        HistoryPageSnapshot {
            scope: self.scope.clone(),
            revision: state.revision,
            connection_epoch: epoch,
            request_epoch: state.epoch,
            is_current: ready
                && self.access() != PaneAccess::Missing
                && state.epoch.is_none_or(|captured| captured == epoch),
            phase: state.phase.clone(),
        }
    }
    /// Complements existing runtime/pointer subscriptions; does not replace them.
    pub fn subscribe_history_pages(&self) -> watch::Receiver<u64> {
        self.history_pages.changes.subscribe()
    }
    pub fn begin_history_page(self: &Arc<Self>) -> Result<HistoryPageRequest> {
        let lease = self.begin_history_lease()?;
        Ok(HistoryPageRequest {
            binding: self.clone(),
            lease,
        })
    }
    pub(super) fn begin_history_lease(&self) -> Result<HistoryPageLease> {
        require(
            self.access() != PaneAccess::Missing,
            "history pointer missing",
        )?;
        let epoch = self.runtime.inspect(|store| {
            require(store.ready(), "history endpoint not ready")?;
            Ok::<_, tekes_agent_contract::Error>(store.connection_epoch)
        })?;
        let mut state = self.history_pages.state.lock().unwrap();
        require(
            !(state.epoch == Some(epoch) && state.active.is_some()),
            "history page already loading",
        )?;
        let id = state
            .revision
            .checked_add(1)
            .ok_or_else(|| tekes_agent_contract::unsupported("history revision exhausted"))?;
        state.revision = id;
        state.epoch = Some(epoch);
        state.active = Some(id);
        state.phase = HistoryPagePhase::Loading;
        self.history_pages.changes.send_replace(id);
        Ok(HistoryPageLease {
            pages: self.history_pages.clone(),
            epoch,
            id,
            finished: false,
        })
    }
    pub(super) async fn load_history_page(&self, mut lease: HistoryPageLease) -> Result<bool> {
        let result: Result<bool> = async {
            require(
                lease.current(self),
                "history request expired before dispatch",
            )?;
            let extended = if let Some(child) = &self.child {
                child.load_earlier().await?
            } else {
                self.runtime.load_earlier(&self.scope.session_id).await?
            };
            require(lease.current(self), "history source changed during read")?;
            Ok(extended)
        }
        .await;
        lease.finish(match &result {
            Ok(extended) => HistoryPagePhase::Loaded {
                extended: *extended,
            },
            Err(error) => HistoryPagePhase::Failed {
                message: error.to_string(),
            },
        });
        result
    }
}
