use crate::SidebarRow;
use std::cmp::Ordering;
pub use tekes_agent_presentation::{
    SidebarPreferenceChange, SidebarPreferences, SidebarPreferencesSnapshot, SidebarSort,
    compare_sidebar_identity,
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
            .then_with(|| compare_sidebar_identity(&a.address, &b.address))
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
            order.then_with(|| compare_sidebar_identity(&a.address, &b.address))
        });
    }
    pinned.extend(sessions);
    Ok(pinned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ThreadActionsPresentation, ThreadActivity, ThreadPinPresentation};
    use tekes_agent_contract::SessionAddress;

    fn row(endpoint: &str, session: &str, pinned: bool) -> SidebarRow {
        SidebarRow {
            primary_folder_id: None,
            folder_path: None,
            address: SessionAddress {
                endpoint_id: endpoint.into(),
                session_id: session.into(),
            },
            title: "same title".into(),
            activity: ThreadActivity {
                status: "idle".into(),
                is_stale: false,
            },
            pin: pinned.then_some(ThreadPinPresentation { pinned }),
            actions: ThreadActionsPresentation {
                endpoint: endpoint.into(),
                session_id: session.into(),
                label: "actions".into(),
                items: vec![],
            },
        }
    }

    fn localized_equal(_: &str, _: &str) -> Ordering {
        Ordering::Equal
    }

    #[test]
    fn title_ties_use_durable_identity_across_endpoint_lengths_without_merging_same_session_ids() {
        let rows = vec![
            row("ab", "same-id", false),
            row("z", "same-id", false),
            row("中", "same-id", false),
        ];
        let prefs = SidebarPreferences {
            sort: SidebarSort::TitleAscending,
            ..SidebarPreferences::default()
        };

        let ordered = ordered_sidebar_rows(&rows, &prefs, Some(localized_equal)).unwrap();
        assert_eq!(
            ordered
                .iter()
                .map(|row| row.address.endpoint_id.as_str())
                .collect::<Vec<_>>(),
            ["z", "ab", "中"]
        );
        assert_eq!(ordered[0].address.session_id, ordered[1].address.session_id);
        assert_ne!(ordered[0].address, ordered[1].address);
    }

    #[test]
    fn pinned_identity_tie_break_keeps_explicit_pin_order_first() {
        let rows = vec![row("z", "same-id", true), row("ab", "same-id", true)];
        let addresses = rows
            .iter()
            .map(|row| row.address.clone())
            .collect::<Vec<_>>();
        let prefs = SidebarPreferences {
            pin_order: addresses.clone(),
            ..SidebarPreferences::default()
        };

        let ordered = ordered_sidebar_rows(&rows, &prefs, None).unwrap();
        assert_eq!(
            ordered.iter().map(|row| &row.address).collect::<Vec<_>>(),
            addresses.iter().collect::<Vec<_>>()
        );

        let prefs = SidebarPreferences::default();
        let ordered = ordered_sidebar_rows(&rows, &prefs, None).unwrap();
        assert_eq!(ordered[0].address.endpoint_id, "z");
        assert_eq!(ordered[1].address.endpoint_id, "ab");
    }
}
