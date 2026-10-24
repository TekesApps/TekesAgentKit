//! Pane input and mutations over the existing SideChatController authority.
use crate::RunBinding;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tekes_agent_components_ui::{SidePanelInput, SidePanelTab, SidePanelTabKind};
use tekes_agent_contract::{Result, require, unsupported};
use tekes_agent_presentation::{RunScope, SideChat, SideChatController, SideChatSnapshot};
use tekes_agent_session::SessionRuntime;
use tokio::sync::watch;

/// A typed role proof: a scratch run was opened through the pointer's owning
/// controller, not obtained from a main-run cache with the same session ID.
pub struct BoundSideChatRun {
    chat: SideChat,
    binding: Arc<RunBinding>,
    owner: Arc<SideChatController>,
}
impl BoundSideChatRun {
    pub fn chat(&self) -> &SideChat {
        &self.chat
    }
    pub fn binding(&self) -> &Arc<RunBinding> {
        &self.binding
    }
}

/// Own one per endpoint runtime, not per tab. No second pointer/mutation store.
pub struct SideChatBinding {
    runtime: Arc<SessionRuntime>,
    endpoint_id: String,
    owner: Arc<SideChatController>,
    // Retain only existing canonical bindings, not another draft/pointer store.
    // Parent participates in authority even though the scratch scope is main.
    runs: Mutex<BTreeMap<(RunScope, RunScope), Arc<RunBinding>>>,
}
impl SideChatBinding {
    pub fn new(runtime: Arc<SessionRuntime>) -> Self {
        let endpoint_id = runtime.inspect(|state| state.endpoint_id.clone());
        Self {
            owner: Arc::new(SideChatController::new(runtime.clone())),
            runtime,
            endpoint_id,
            runs: Mutex::new(BTreeMap::new()),
        }
    }
    fn retire_missing_runs(&self) {
        self.runs
            .lock()
            .expect("SideChat runs lock")
            .retain(|_, run| run.access() != crate::PaneAccess::Missing);
    }
    pub fn owner(&self) -> &Arc<SideChatController> {
        &self.owner
    }
    /// Endpoint whose SideChat pointer/controller this binding owns.
    pub fn endpoint_id(&self) -> &str {
        &self.endpoint_id
    }
    /// A run is accepted only by the controller that opened it while that
    /// controller still owns the exact pointer captured by the run.
    pub fn owns_run(&self, run: &BoundSideChatRun) -> bool {
        Arc::ptr_eq(&self.owner, &run.owner)
            && self
                .pointer(&run.chat.parent, &run.chat.scope.session_id)
                .is_ok_and(|current| current == run.chat)
    }
    pub fn snapshot(&self) -> SideChatSnapshot {
        self.owner.snapshot()
    }
    pub fn subscribe(&self) -> watch::Receiver<SideChatSnapshot> {
        self.owner.subscribe()
    }
    pub fn support_snapshot(&self) -> Option<bool> {
        self.owner.support_snapshot()
    }
    /// Host supplies tab navigation, summary/task/child tabs and revision. Scratch
    /// membership and status come only from the existing controller snapshot.
    pub fn input(
        &self,
        revision: u64,
        parent: &RunScope,
        mut tabs: Vec<SidePanelTab>,
        selected: Option<SidePanelTabKind>,
    ) -> Result<SidePanelInput> {
        parent.validate()?;
        require(
            parent.endpoint_id == self.runtime.inspect(|state| state.endpoint_id.clone()),
            "SideChat endpoint mismatch",
        )?;
        require(
            tabs.iter()
                .all(|tab| !matches!(tab.kind, SidePanelTabKind::SideChat { .. })),
            "scratch tabs come from pointer authority",
        )?;
        let snapshot = self.snapshot();
        tabs.extend(
            snapshot
                .chats
                .values()
                .filter(|chat| chat.parent == *parent)
                .map(SidePanelTab::side_chat),
        );
        let selected = selected.filter(|kind| tabs.iter().any(|tab| &tab.kind == kind));
        Ok(SidePanelInput {
            revision,
            parent: parent.clone(),
            projection_ready: true,
            tabs,
            selected,
            side_chats: snapshot,
            supports_side_chat: self.support_snapshot(),
        })
    }
    pub async fn create(
        &self,
        parent: RunScope,
        title: Option<&str>,
        rpc_id: &str,
    ) -> Result<SideChat> {
        self.owner.create(parent, title, rpc_id).await
    }
    pub fn pointer(&self, parent: &RunScope, id: &str) -> Result<SideChat> {
        let chat = self
            .snapshot()
            .chats
            .get(id)
            .cloned()
            .ok_or_else(|| unsupported("SideChat pointer missing"))?;
        require(chat.parent == *parent, "SideChat parent mismatch")?;
        Ok(chat)
    }
    pub async fn close(&self, expected: &SideChat, rpc_id: &str) -> Result<()> {
        let result = self.owner.close_owned(expected, rpc_id).await;
        self.retire_missing_runs();
        result
    }
    pub async fn open(&self, expected: &SideChat) -> Result<BoundSideChatRun> {
        let current = self.pointer(&expected.parent, &expected.scope.session_id)?;
        require(current.scope == expected.scope, "SideChat scope changed")?;
        self.retire_missing_runs();
        let key = (current.parent.clone(), current.scope.clone());
        let retained = self
            .runs
            .lock()
            .expect("SideChat runs lock")
            .get(&key)
            .cloned();
        if let Some(binding) = retained {
            return Ok(BoundSideChatRun {
                chat: current,
                binding,
                owner: self.owner.clone(),
            });
        }
        let opened = Arc::new(
            RunBinding::open_side_chat(self.runtime.clone(), self.owner.clone(), &current).await?,
        );
        // Concurrent opens converge only after the original history baseline;
        // closing/reconciliation still fences the winner's public controls.
        let mut runs = self.runs.lock().expect("SideChat runs lock");
        require(
            opened.access() != crate::PaneAccess::Missing,
            "SideChat pointer retired while mounting",
        )?;
        let binding = runs.entry(key).or_insert(opened).clone();
        Ok(BoundSideChatRun {
            chat: current,
            binding,
            owner: self.owner.clone(),
        })
    }
    /// Host input must be an authoritative read result. Errors do not mean absence.
    pub fn reconcile(
        &self,
        expected: &SideChat,
        exists: std::result::Result<bool, String>,
    ) -> bool {
        let accepted = self.owner.reconcile(expected, exists);
        self.retire_missing_runs();
        accepted
    }
    pub fn reconcile_creation(
        &self,
        parent: &RunScope,
        discovered: Option<SideChat>,
    ) -> Result<()> {
        self.owner.reconcile_creation(parent, discovered)
    }
}
