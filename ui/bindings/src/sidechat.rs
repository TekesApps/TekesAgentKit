//! Pane input and mutations over the existing SideChatController authority.
use crate::RunBinding;
use std::sync::Arc;
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
    owner: Arc<SideChatController>,
}
impl SideChatBinding {
    pub fn new(runtime: Arc<SessionRuntime>) -> Self {
        Self {
            owner: Arc::new(SideChatController::new(runtime.clone())),
            runtime,
        }
    }
    pub fn owner(&self) -> &Arc<SideChatController> {
        &self.owner
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
        self.owner.close_owned(expected, rpc_id).await
    }
    pub async fn open(&self, expected: &SideChat) -> Result<BoundSideChatRun> {
        let current = self.pointer(&expected.parent, &expected.scope.session_id)?;
        require(current.scope == expected.scope, "SideChat scope changed")?;
        let binding = Arc::new(
            RunBinding::open_side_chat(self.runtime.clone(), self.owner.clone(), &current).await?,
        );
        Ok(BoundSideChatRun {
            chat: current,
            binding,
        })
    }
    /// Host input must be an authoritative read result. Errors do not mean absence.
    pub fn reconcile(
        &self,
        expected: &SideChat,
        exists: std::result::Result<bool, String>,
    ) -> bool {
        self.owner.reconcile(expected, exists)
    }
    pub fn reconcile_creation(
        &self,
        parent: &RunScope,
        discovered: Option<SideChat>,
    ) -> Result<()> {
        self.owner.reconcile_creation(parent, discovered)
    }
}
