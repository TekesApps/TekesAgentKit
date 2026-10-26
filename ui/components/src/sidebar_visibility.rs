//! Business capture for TekesUI's focus-preserving stable-item reveal.
//! The host passes these original IDs to the library; no pixel or focus repair.
use crate::{SessionAddress, Sidebar};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tekes_ui_controls::composition::{ComponentId, ContentHost};
#[derive(Clone, Debug)]
pub struct SidebarVisibilityTarget {
    pub(crate) owner: Arc<()>,
    pub(crate) root: ComponentId,
    pub(crate) viewport: ComponentId,
    pub(crate) item: ComponentId,
    pub(crate) address: SessionAddress,
    pub(crate) navigation: u64,
    pub(crate) inventory: u64,
    pub(crate) disclosure: u64,
    pub(crate) claimed: Arc<AtomicBool>,
}
impl SidebarVisibilityTarget {
    pub fn root(&self) -> ComponentId {
        self.root
    }
    pub fn viewport(&self) -> ComponentId {
        self.viewport
    }
    pub fn item(&self) -> ComponentId {
        self.item
    }
    pub fn address(&self) -> &SessionAddress {
        &self.address
    }
    pub fn navigation_revision(&self) -> u64 {
        self.navigation
    }
    pub fn inventory_revision(&self) -> u64 {
        self.inventory
    }
    /// Validate immediately before the original library command. No focus or
    /// scroll is changed by this claim; a rejected dispatch is not auto-retried.
    pub fn claim(&self, host: &ContentHost) -> Result<(), &'static str> {
        host.get(self.root)
            .and_then(|n| n.component::<Sidebar>())
            .ok_or("Sidebar unmounted")?
            .claim_visibility(self)
    }
    pub(crate) fn once(&self) -> bool {
        !self.claimed.swap(true, Ordering::AcqRel)
    }
}
