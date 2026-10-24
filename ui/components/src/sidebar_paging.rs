//! Bounded ordinary inventory; folder membership is shared by pins and rows.
use crate::{
    SessionAddress, SidebarFolderKey, SidebarOutline, SidebarPreferences, SidebarRow,
    SidebarTitleComparator, ordered_sidebar_rows, sidebar_outline,
};
use std::collections::BTreeSet;
use std::sync::Arc;
use tekes_ui_controls::composition::ComponentId;
/// Matches ClientKit's SidebarPaging.pageSize. This pages local authoritative
/// inventory, never hides a remote cursor or fabricates a successful read.
pub const SIDEBAR_PAGE_SIZE: usize = 5;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SidebarPagingFacts {
    pub limit: usize,
    pub ordinary_count: usize,
    pub has_more: bool,
    pub selected_supplemented: bool,
}
/// Captured from the original Show more control; cannot be constructed by hosts.
#[derive(Clone, Debug)]
pub struct SidebarPageRequest {
    pub(crate) owner: Arc<()>,
    pub(crate) control: ComponentId,
    pub(crate) limit: usize,
}
pub fn sidebar_paged_outline<'a>(
    rows: &'a [SidebarRow],
    preferences: &SidebarPreferences,
    compare: Option<SidebarTitleComparator>,
    filter: Option<&SidebarFolderKey>,
    selected: Option<&SessionAddress>,
    limit: usize,
    searching: bool,
) -> Result<(SidebarOutline<'a>, SidebarPagingFacts), &'static str> {
    if limit == 0 {
        return Err("zero Sidebar page limit");
    }
    let ordered = ordered_sidebar_rows(rows, preferences, compare)?;
    let ordinary: Vec<_> = ordered
        .into_iter()
        .filter(|r| {
            !r.pin.as_ref().is_some_and(|p| p.pinned)
                && filter.is_none_or(|key| key == &r.folder_key())
        })
        .collect();
    let mut members: BTreeSet<_> = ordinary
        .iter()
        .take(if searching { usize::MAX } else { limit })
        .map(|r| &r.address)
        .collect();
    let supplement = !searching
        && selected
            .is_some_and(|id| !members.contains(id) && ordinary.iter().any(|r| &r.address == id));
    if supplement {
        members.insert(selected.unwrap());
    }
    let mut outline = sidebar_outline(rows, preferences, compare, filter)?;
    outline.sessions.retain(|r| members.contains(&r.address));
    for folder in &mut outline.folders {
        folder.rows.retain(|r| members.contains(&r.address));
    }
    outline.folders.retain(|folder| !folder.rows.is_empty());
    Ok((
        outline,
        SidebarPagingFacts {
            limit,
            ordinary_count: ordinary.len(),
            has_more: !searching && ordinary.len() > limit,
            selected_supplemented: supplement,
        },
    ))
}
