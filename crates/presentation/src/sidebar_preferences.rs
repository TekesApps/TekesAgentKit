//! Agent-owned Sidebar display preferences, separate from pin membership.
use serde::{Deserialize, Serialize};
use tekes_agent_contract::{Result, SessionAddress, require};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SidebarSort {
    #[default]
    RecentActivity,
    TitleAscending,
    TitleDescending,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarPreferences {
    pub sort: SidebarSort,
    pub pin_order: Vec<SessionAddress>,
    pub pinned_expanded: bool,
}
impl Default for SidebarPreferences {
    fn default() -> Self {
        Self {
            sort: SidebarSort::default(),
            pin_order: vec![],
            pinned_expanded: true,
        }
    }
}
impl SidebarPreferences {
    pub fn validate(&self) -> Result<()> {
        let mut seen = std::collections::BTreeSet::new();
        for address in &self.pin_order {
            require(
                !address.endpoint_id.is_empty()
                    && !address.session_id.is_empty()
                    && seen.insert(address),
                "invalid or duplicate Sidebar pin order",
            )?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarPreferencesSnapshot {
    pub revision: u64,
    pub preferences: SidebarPreferences,
}
impl Default for SidebarPreferencesSnapshot {
    fn default() -> Self {
        Self {
            revision: 1,
            preferences: SidebarPreferences::default(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarPreferenceChange {
    pub expected: SidebarPreferencesSnapshot,
    pub value: SidebarPreferences,
}
