use crate::SidebarRow;
use std::cmp::Ordering;
pub use tekes_agent_presentation::{
    SidebarPreferenceChange, SidebarPreferences, SidebarPreferencesSnapshot, SidebarSort,
};
/// Host system capability, e.g. localizedCaseInsensitiveCompare on macOS.
/// Kit owns the direction, identity tie-break and pin ordering.
pub type SidebarTitleComparator = fn(&str, &str) -> Ordering;
pub fn ordered_sidebar_rows<'a>(
    rows: &'a [SidebarRow],
    preferences: &SidebarPreferences,
    compare: Option<SidebarTitleComparator>,
) -> Result<Vec<&'a SidebarRow>, &'static str> {
    preferences
        .validate()
        .map_err(|_| "invalid Sidebar preferences")?;
    if preferences.sort != SidebarSort::RecentActivity && compare.is_none() {
        return Err("Sidebar title sort requires a host localized comparator");
    }
    let mut pinned = Vec::new();
    let mut sessions = Vec::new();
    for row in rows {
        if row.pin.as_ref().is_some_and(|pin| pin.pinned) {
            pinned.push(row);
        } else {
            sessions.push(row);
        }
    }
    pinned.sort_by(|a, b| {
        let rank = |row: &SidebarRow| {
            preferences
                .pin_order
                .iter()
                .position(|id| id == &row.address)
                .unwrap_or(usize::MAX)
        };
        rank(a)
            .cmp(&rank(b))
            .then_with(|| a.address.cmp(&b.address))
    });
    if let Some(compare) = compare
        && preferences.sort != SidebarSort::RecentActivity
    {
        sessions.sort_by(|a, b| {
            let order = compare(&a.title, &b.title);
            let order = if preferences.sort == SidebarSort::TitleDescending {
                order.reverse()
            } else {
                order
            };
            order.then_with(|| a.address.cmp(&b.address))
        });
    }
    pinned.extend(sessions);
    Ok(pinned)
}
