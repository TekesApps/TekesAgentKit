//! Occupied folder choices are derived before search, filtering and paging.
use crate::{SessionAddress, SidebarFolderIdentity, SidebarFolderKey, SidebarRow};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarFolderOption {
    pub key: SidebarFolderKey,
    /// MAX(path), ignoring missing values, as in ClientKit's folder query.
    pub path: Option<String>,
}

/// Use canonical effective activity, not title order or a page's membership.
/// Missing/nonfinite clocks and duplicate addresses are rejected, never guessed.
pub fn sidebar_folder_catalog(
    rows: &[SidebarRow],
    activity_at: &BTreeMap<SessionAddress, f64>,
) -> Result<Vec<SidebarFolderOption>, &'static str> {
    let mut seen = BTreeSet::new();
    let mut folders: BTreeMap<SidebarFolderKey, (Option<String>, f64)> = BTreeMap::new();
    for row in rows {
        if !seen.insert(&row.address) || row.address.session_id.is_empty() {
            return Err("duplicate or empty Sidebar catalog identity");
        }
        let key = row.folder_key();
        key.validate()
            .map_err(|_| "invalid Sidebar catalog folder")?;
        let time = activity_at
            .get(&row.address)
            .copied()
            .filter(|t| t.is_finite())
            .ok_or("missing or invalid Sidebar catalog activity")?;
        let entry = folders.entry(key).or_insert((None, time));
        entry.0 = entry.0.take().max(row.folder_path.clone());
        entry.1 = entry.1.max(time);
    }
    let mut ordered: Vec<_> = folders.into_iter().collect();
    ordered.sort_by(|(a, (_, at)), (b, (_, bt))| {
        // SQL compares equal zero values equally too; NaN was rejected above.
        bt.partial_cmp(at)
            .unwrap()
            .then_with(|| order_key(a).cmp(&order_key(b)))
            .then_with(|| a.cmp(b))
    });
    Ok(ordered
        .into_iter()
        .map(|(key, (path, _))| SidebarFolderOption { key, path })
        .collect())
}
fn order_key(key: &SidebarFolderKey) -> String {
    // Comparison only. Routing/membership always retain the typed key, including
    // Id("path:x") versus Path("x"), which a concatenated SQL key can conflate.
    let identity = match &key.identity {
        SidebarFolderIdentity::Id(id) => id.clone(),
        SidebarFolderIdentity::Path(path) => format!("path:{path}"),
        SidebarFolderIdentity::None => "none".into(),
    };
    format!("{}\u{1f}{identity}", key.endpoint_id)
}
