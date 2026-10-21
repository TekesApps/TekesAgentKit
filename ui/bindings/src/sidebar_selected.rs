//! Selected conversation is read independently of the displayed inventory page.
use crate::{SidebarBinding, sidebar::inventory_row};
use std::sync::Arc;
use tekes_agent_components_ui::{SidebarInput, SidebarRow};
use tekes_agent_contract::{Result, SessionAddress, require};
use tekes_agent_session::ConnectionState;
use tekes_agent_web::{SharedAgent, SharedEndpoint, ThreadInventoryRead};
#[derive(Clone)]
pub struct SidebarSelectedRead {
    agent: Arc<SharedAgent>,
    endpoint: Arc<SharedEndpoint>,
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
        let facts = self
            .agent
            .read_thread_inventory(&self.endpoint, &address.session_id)?;
        let row = match (&facts.item, &facts.actions) {
            (Some(item), Some(actions)) => {
                Some(inventory_row(&address.endpoint_id, item, actions.clone())?)
            }
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
    /// Merge only an ordinary, active, authoritative selected row. Search and
    /// global pins retain their independent membership; no filter is cleared.
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
