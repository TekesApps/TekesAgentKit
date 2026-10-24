//! Selected conversation is read independently of the displayed inventory page.
use crate::sidebar::SidebarObserverOwner;
use crate::{
    SidebarBinding, SidebarMultiEndpointBinding, SidebarMultiOwner,
    sidebar::{inventory_row, sidebar_row_is_member},
};
use std::sync::Arc;
use tekes_agent_components_ui::{SidebarInput, SidebarRow};
use tekes_agent_contract::{Result, SessionAddress, require};
use tekes_agent_session::ConnectionState;
use tekes_agent_web::{SharedAgent, SharedEndpoint, ThreadInventoryRead};
#[derive(Clone)]
pub struct SidebarSelectedRead {
    agent: Arc<SharedAgent>,
    endpoint: Arc<SharedEndpoint>,
    observer_owner: Arc<SidebarObserverOwner>,
    observer_generation: u64,
    generation: u64,
    facts: ThreadInventoryRead,
    row: Option<SidebarRow>,
}
impl SidebarBinding {
    /// One bounded memory read. No journal lease, provider RPC, inventory refresh
    /// or private file access. Generation belongs to the host's selection scope.
    pub fn read_selected(
        &self,
        address: &SessionAddress,
        selection_generation: u64,
    ) -> Result<SidebarSelectedRead> {
        require(
            selection_generation > 0,
            "unversioned Sidebar selection read",
        )?;
        require(
            self.endpoint
                .runtime
                .inspect(|s| s.endpoint_id == address.endpoint_id),
            "Sidebar selection endpoint mismatch",
        )?;
        let observer_owner = self.observer_owner();
        let observer_generation = self.observer_generation();
        let facts = self
            .agent
            .read_thread_inventory(&self.endpoint, &address.session_id)?;
        let row = match (&facts.item, &facts.actions) {
            (Some(item), Some(actions)) if sidebar_row_is_member(item) => {
                Some(inventory_row(&address.endpoint_id, item, actions.clone())?)
            }
            (Some(_), Some(_)) => None,
            (None, None) => None,
            _ => {
                return Err(tekes_agent_contract::unsupported(
                    "incomplete Sidebar selected facts",
                ));
            }
        };
        Ok(SidebarSelectedRead {
            agent: self.agent.clone(),
            endpoint: self.endpoint.clone(),
            observer_owner,
            observer_generation,
            generation: selection_generation,
            facts,
            row,
        })
    }
}
impl SidebarSelectedRead {
    pub fn address(&self) -> &SessionAddress {
        &self.facts.address
    }
    pub fn selection_generation(&self) -> u64 {
        self.generation
    }
    pub fn is_current(&self, source: &SidebarBinding) -> bool {
        Arc::ptr_eq(&self.endpoint, &source.endpoint)
            && Arc::ptr_eq(&self.agent, &source.agent)
            && source.owns_observer(&self.observer_owner, self.observer_generation)
            && source
                .agent
                .read_thread_inventory(&source.endpoint, &self.facts.address.session_id)
                .is_ok_and(|facts| facts == self.facts)
    }
    /// Navigation validity is separate from list membership: a folder filter,
    /// search or a global pin must not forbid opening an active conversation.
    pub fn has_active_row(&self, source: &SidebarBinding) -> bool {
        self.facts.inventory_ready
            && self.facts.connection == ConnectionState::Ready
            && self.row.is_some()
            && self.is_current(source)
    }
    /// Merge only an ordinary, active, authoritative selected row. Pin rows
    /// require the full inventory publication; this bounded read cannot mint
    /// a pinned partition or bypass search/folder membership. No filter clears.
    pub fn project_into(
        &self,
        source: &SidebarBinding,
        display: &mut SidebarInput,
        selection_generation: u64,
    ) -> bool {
        if !(self.generation == selection_generation
            && display.selected.as_ref() == Some(self.address())
            && self.is_current(source))
        {
            return false;
        }
        if !display.loaded
            || !self.facts.inventory_ready
            || self.facts.connection != ConnectionState::Ready
            || display
                .search_help
                .as_ref()
                .is_some_and(|help| help.source.key.is_some())
            || display.rows.iter().any(|r| &r.address == self.address())
        {
            return false;
        }
        let Some(row) = &self.row else {
            return false;
        };
        if row.pin.as_ref().is_some_and(|p| p.pinned)
            || display
                .folder_filter
                .as_ref()
                .is_some_and(|key| key != &row.folder_key())
        {
            return false;
        }
        display.rows.push(row.clone());
        true
    }
}

/// Selected read tied to one aggregate Sidebar instance. The inner single-endpoint
/// receipt retains its original agent/endpoint/facts freshness checks; the owner
/// token adds the aggregate-instance fence that those checks cannot express.
pub struct SidebarMultiSelectedRead {
    inner: SidebarSelectedRead,
    owner: Arc<SidebarMultiOwner>,
}
impl SidebarMultiEndpointBinding {
    /// Read from the endpoint named by the complete address, never from the
    /// first or currently selected endpoint.
    pub fn read_selected(
        &self,
        address: &SessionAddress,
        selection_generation: u64,
    ) -> Result<SidebarMultiSelectedRead> {
        let source = self.binding_for(&address.endpoint_id).ok_or_else(|| {
            tekes_agent_contract::unsupported("selected endpoint is outside the fixed set")
        })?;
        let inner = source.read_selected(address, selection_generation)?;
        Ok(SidebarMultiSelectedRead {
            inner,
            owner: self.owner(),
        })
    }
}
impl SidebarMultiSelectedRead {
    pub fn address(&self) -> &SessionAddress {
        self.inner.address()
    }
    pub fn selection_generation(&self) -> u64 {
        self.inner.selection_generation()
    }
    pub fn is_current(&self, source: &SidebarMultiEndpointBinding) -> bool {
        if !source.owns(&self.owner) {
            return false;
        }
        source
            .binding_for(&self.address().endpoint_id)
            .is_some_and(|binding| self.inner.is_current(binding))
    }
    pub fn has_active_row(&self, source: &SidebarMultiEndpointBinding) -> bool {
        if !source.owns(&self.owner) {
            return false;
        }
        source
            .binding_for(&self.address().endpoint_id)
            .is_some_and(|binding| self.inner.has_active_row(binding))
    }
    /// Projects with the existing endpoint-local freshness/filter checks, after
    /// verifying this read belongs to the same fixed-set aggregate instance.
    pub fn project_into(
        &self,
        source: &SidebarMultiEndpointBinding,
        display: &mut SidebarInput,
        selection_generation: u64,
    ) -> bool {
        if !source.owns(&self.owner) {
            return false;
        }
        source
            .binding_for(&self.address().endpoint_id)
            .is_some_and(|binding| {
                self.inner
                    .project_into(binding, display, selection_generation)
            })
    }
}
