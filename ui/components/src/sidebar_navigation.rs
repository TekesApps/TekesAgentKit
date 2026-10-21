//! Explicit host navigation, separate from inventory and durable preferences.
use crate::{SessionAddress, SidebarFolderKey, SidebarInput};
use std::collections::BTreeSet;
/// One monotonically versioned navigation authority for a Sidebar lifetime.
/// Publish the matching selection before reconciling this event. Repeated
/// inventory frames are not navigation and cannot reopen a manual disclosure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarSelectionNavigation {
    pub revision: u64,
    pub selected: Option<SessionAddress>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NavigationReveal {
    pub folders: BTreeSet<SidebarFolderKey>,
    pub pinned: bool,
}
#[derive(Clone, Debug)]
pub(crate) enum DisclosureTarget {
    Folder(SidebarFolderKey),
    Pinned,
}
#[derive(Clone, Debug)]
pub(crate) struct PendingNavigation {
    pub event: SidebarSelectionNavigation,
    pub suppressed_folders: BTreeSet<SidebarFolderKey>,
    pub suppressed_pin: bool,
}
impl PendingNavigation {
    pub fn new(event: SidebarSelectionNavigation) -> Self {
        Self {
            event,
            suppressed_folders: BTreeSet::new(),
            suppressed_pin: false,
        }
    }
    pub fn suppress(&mut self, target: &DisclosureTarget) {
        match target {
            DisclosureTarget::Folder(key) => {
                self.suppressed_folders.insert(key.clone());
            }
            DisclosureTarget::Pinned => self.suppressed_pin = true,
        }
    }
}
pub(crate) fn plan_reveal(
    input: &SidebarInput,
    pending: Option<&PendingNavigation>,
    current: &NavigationReveal,
) -> (NavigationReveal, bool) {
    let mut next = current.clone();
    let Some(pending) = pending else {
        return (next, false);
    };
    let event = &pending.event;
    // A changed host selection cancels the old request, even before restoration.
    if event.selected != input.selected || event.selected.is_none() {
        return (next, true);
    }
    if !input.loaded {
        return (next, false);
    }
    let Some(row) = input
        .rows
        .iter()
        .find(|row| Some(&row.address) == event.selected.as_ref())
    else {
        return (next, false);
    };
    if row.pin.as_ref().is_some_and(|pin| pin.pinned) {
        if !pending.suppressed_pin {
            next.pinned = true;
        }
    } else {
        let key = row.folder_key();
        if !pending.suppressed_folders.contains(&key)
            && input
                .folder_filter
                .as_ref()
                .is_none_or(|filter| filter == &key)
        {
            next.folders.insert(key);
        }
    }
    (next, true)
}
