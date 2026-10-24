//! ClientKit-aligned folder grouping and filtering of session rows.
use crate::{SidebarPreferences, SidebarRow, SidebarTitleComparator, ordered_sidebar_rows};
pub use tekes_agent_presentation::{SidebarFolderIdentity, SidebarFolderKey};
impl SidebarRow {
    pub fn folder_key(&self) -> SidebarFolderKey {
        SidebarFolderKey {
            endpoint_id: self.address.endpoint_id.clone(),
            identity: match &self.primary_folder_id {
                Some(id) => SidebarFolderIdentity::Id(id.clone()),
                None => match self.folder_path.as_ref().filter(|path| !path.is_empty()) {
                    Some(path) => SidebarFolderIdentity::Path(path.clone()),
                    None => SidebarFolderIdentity::None,
                },
            },
        }
    }
}
#[derive(Debug)]
pub struct SidebarFolderGroup<'a> {
    pub key: SidebarFolderKey,
    pub path: Option<&'a str>,
    pub rows: Vec<&'a SidebarRow>,
}
#[derive(Debug, Default)]
pub struct SidebarOutline<'a> {
    pub pinned: Vec<&'a SidebarRow>,
    pub folders: Vec<SidebarFolderGroup<'a>>,
    pub sessions: Vec<&'a SidebarRow>,
}
/// Groups appear in first-occurrence order after the shared sort. Folder titles
/// and paths are display data; grouping never substitutes a routing identity.
pub fn sidebar_outline<'a>(
    rows: &'a [SidebarRow],
    preferences: &SidebarPreferences,
    compare: Option<SidebarTitleComparator>,
    filter: Option<&SidebarFolderKey>,
) -> Result<SidebarOutline<'a>, &'static str> {
    if let Some(key) = filter {
        key.validate()
            .map_err(|_| "invalid Sidebar folder filter")?;
    }
    let mut result = SidebarOutline::default();
    for row in ordered_sidebar_rows(rows, preferences, compare)? {
        let key = row.folder_key();
        key.validate().map_err(|_| "invalid Sidebar row folder")?;
        // The folder is a common membership predicate. Pins are a display
        // partition, so apply the predicate before deciding which partition
        // receives the row.
        if filter.is_some_and(|selected| selected != &key) {
            continue;
        }
        if row.pin.as_ref().is_some_and(|pin| pin.pinned) {
            result.pinned.push(row);
            continue;
        }
        if preferences.group_by_folder {
            let index = match result.folders.iter().position(|group| group.key == key) {
                Some(index) => index,
                None => {
                    result.folders.push(SidebarFolderGroup {
                        key,
                        path: row.folder_path.as_deref(),
                        rows: vec![],
                    });
                    result.folders.len() - 1
                }
            };
            result.folders[index].rows.push(row);
        } else {
            result.sessions.push(row);
        }
    }
    Ok(result)
}
pub fn sidebar_folder_title(path: Option<&str>, no_folder: &str) -> String {
    match path.filter(|path| !path.is_empty()) {
        None => no_folder.into(),
        Some(path) => std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path)
            .into(),
    }
}
