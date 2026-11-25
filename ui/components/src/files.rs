//! Workspace Files tree & search surface aligned with Tekes: search box, clean header
//! with icon refresh action, SVG file/folder icons in `TreeContent`, direct click-to-open,
//! and right-click context menu (`Open in Viewer` / `Attach to Conversation`).

use crate::ResponsiveWidthMode;
use crate::ids::*;
use crate::ui_helpers::{icon_button, icon_row, label_text};
use std::collections::BTreeMap;
use tekes_ui_controls::{
    ScrollArea,
    composition::{
        ComponentId, Content, ContentActionKind, ContentActionPayload, ContentActionRecord,
        ContentHost, MenuContent, MenuEntry, ScrollContent, SearchCommandKind, SearchFieldContent,
        StackContent, TextContent, TreeContent, TreeContentNode, TreeDisplayUpdate, TreeIntentKind,
    },
    extended::{KeyedTreeNode, TreeDatasetKey, TreeNodeKey},
};
use tekes_ui_tokens::{Spacing, proposal::layout::MainAxisDistribution};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileTreeRowModel {
    pub path: String,
    pub parent_path: Option<String>,
    pub name: String,
    pub is_dir: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FilesIntent {
    SelectFile(String),
    OpenFile(String),
    AttachFile(String),
    SearchChanged(String),
    Refresh,
}

#[derive(Debug)]
pub struct WorkspaceFilesSurface {
    next_key: u64,
    path_to_key: BTreeMap<String, TreeNodeKey>,
    key_to_path: BTreeMap<TreeNodeKey, String>,
    key_is_dir: BTreeMap<TreeNodeKey, bool>,
    last_sig: String,
    search_query: String,
    last_mode: ResponsiveWidthMode,
}

impl Default for WorkspaceFilesSurface {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceFilesSurface {
    pub fn new() -> Self {
        Self {
            next_key: 1,
            path_to_key: BTreeMap::new(),
            key_to_path: BTreeMap::new(),
            key_is_dir: BTreeMap::new(),
            last_sig: String::new(),
            search_query: String::new(),
            last_mode: ResponsiveWidthMode::Expanded,
        }
    }

    pub fn build_content() -> Content {
        let mut search = SearchFieldContent::new("Search workspace", "");
        search.set_placeholder(Some("Search workspace paths".to_owned()));

        let refresh_btn = icon_button(FILES_REFRESH_ICON_ID, "tekes-refresh", "Refresh files");
        let header_row = StackContent::horizontal(
            Spacing::Tight.value(),
            vec![
                Content::new(FILES_HEADER_ID, label_text("Workspace")),
                Content::new(FILES_REFRESH_BTN_ID, refresh_btn),
            ],
        )
        .with_flexible_child(FILES_HEADER_ID)
        .into_content_with_distribution(FILES_HEADER_ROW_ID, MainAxisDistribution::SpaceBetween);

        let tree = TreeContent::new("Workspace tree", TreeDatasetKey(1), Vec::new())
            .expect("empty TreeContent valid");
        let tree_scroll =
            ScrollContent::new(ScrollArea::default(), Content::new(FILES_TREE_ID, tree))
                .scrollbar(true);

        let context_menu = Content::new(
            FILES_CONTEXT_MENU_ID,
            MenuContent::context_menu(
                "File actions",
                vec![
                    MenuEntry::item("open", "Open in Viewer"),
                    MenuEntry::item("attach", "Attach to Conversation"),
                ],
            )
            .expect("valid files context menu"),
        );

        Content::new(
            FILES_ROOT_ID,
            StackContent::vertical(
                Spacing::Regular.value(),
                vec![
                    Content::new(FILES_SEARCH_ID, search),
                    header_row,
                    Content::new(FILES_TREE_SCROLL_ID, tree_scroll),
                    context_menu,
                ],
            )
            .with_flexible_child(FILES_TREE_SCROLL_ID),
        )
    }

    pub fn bind_context_menu(host: &mut ContentHost) {
        let _ = host.bind_context_menu(FILES_TREE_ID, FILES_CONTEXT_MENU_ID);
    }

    fn key_for(&mut self, path: &str) -> TreeNodeKey {
        if let Some(k) = self.path_to_key.get(path) {
            return *k;
        }
        let k = TreeNodeKey(self.next_key);
        self.next_key += 1;
        self.path_to_key.insert(path.to_owned(), k);
        self.key_to_path.insert(k, path.to_owned());
        k
    }

    fn file_icon(item: &FileTreeRowModel) -> &'static str {
        if item.is_dir {
            return "tekes-folder";
        }
        if item.name.ends_with(".rs")
            || item.name.ends_with(".ts")
            || item.name.ends_with(".tsx")
            || item.name.ends_with(".cadspec")
            || item.name.ends_with(".py")
        {
            "tekes-file-code"
        } else if item.name.ends_with(".md")
            || item.name.ends_with(".toml")
            || item.name.ends_with(".json")
            || item.name.ends_with(".txt")
        {
            "tekes-file-text"
        } else if item.name.ends_with(".png")
            || item.name.ends_with(".jpg")
            || item.name.ends_with(".gif")
        {
            "tekes-image"
        } else {
            "tekes-file-generic"
        }
    }

    pub fn sync(
        &mut self,
        host: &mut ContentHost,
        workspace_id: &str,
        items: &[FileTreeRowModel],
        selected_path: Option<&str>,
        width_mode: ResponsiveWidthMode,
    ) {
        self.last_mode = width_mode;

        if let Some(header) = host
            .get_mut(FILES_HEADER_ID)
            .and_then(|c| c.component_mut::<TextContent>())
        {
            let next_title = if workspace_id.is_empty() {
                "Workspace".to_owned()
            } else {
                workspace_id.to_owned()
            };
            if header.text() != next_title {
                header.set_text(next_title);
            }
        }

        let mut sig = String::new();
        let mut tree_nodes = Vec::with_capacity(items.len());
        self.key_is_dir.clear();

        for item in items {
            let key = self.key_for(&item.path);
            let parent = item.parent_path.as_deref().map(|p| self.key_for(p));
            self.key_is_dir.insert(key, item.is_dir);
            let icon_name = Self::file_icon(item);
            sig.push_str(&format!(
                "{}:{:?}:{icon_name}:{}|",
                key.0, parent, item.name
            ));
            let decor = FILES_ROW_DECOR_BASE + key.0 * 4;
            tree_nodes.push(TreeContentNode {
                row_id: ComponentId(FILES_ROW_BASE + key.0),
                node: KeyedTreeNode {
                    key,
                    parent,
                    label: item.name.clone(),
                    disabled: false,
                    initially_expanded: true,
                },
                display: TreeDisplayUpdate::Replace(icon_row(
                    ComponentId(decor),
                    ComponentId(decor + 1),
                    ComponentId(decor + 2),
                    icon_name,
                    item.name.clone(),
                )),
            });
        }

        if sig != self.last_sig
            && host
                .replace_tree_nodes(FILES_TREE_ID, TreeDatasetKey(1), tree_nodes)
                .is_ok()
        {
            self.last_sig = sig;
        }

        if let Some(sel) = selected_path
            && let Some(key) = self.path_to_key.get(sel).copied()
            && let Some(tree) = host
                .get_mut(FILES_TREE_ID)
                .and_then(|c| c.component_mut::<TreeContent>())
        {
            tree.select_key(key);
        }
    }

    pub fn handle_actions(
        &mut self,
        host: &mut ContentHost,
        selected_path: Option<&str>,
        records: &[ContentActionRecord],
    ) -> Vec<FilesIntent> {
        let mut out = Vec::new();
        if let Some(search) = host
            .get(FILES_SEARCH_ID)
            .and_then(|c| c.component::<SearchFieldContent>())
        {
            let q = search.query().to_owned();
            if q != self.search_query {
                self.search_query = q.clone();
                out.push(FilesIntent::SearchChanged(q));
            }
        }

        for record in records {
            if let Some(ContentActionPayload::Search(search)) = &record.payload
                && record.action.target == FILES_SEARCH_ID
            {
                let q = match search.kind {
                    SearchCommandKind::Submit => search.query.clone(),
                    SearchCommandKind::Clear => String::new(),
                };
                self.search_query = q.clone();
                out.push(FilesIntent::SearchChanged(q));
                continue;
            }
            if let Some(ContentActionPayload::TreeIntent(intent)) = &record.payload
                && record.action.target == FILES_TREE_ID
            {
                if let TreeIntentKind::ExpansionChanged { expanded } = intent.kind {
                    if let Some(tree) = host
                        .get_mut(FILES_TREE_ID)
                        .and_then(|c| c.component_mut::<TreeContent>())
                    {
                        tree.set_expanded_key(intent.key, expanded);
                    }
                    continue;
                }
                if let Some(path) = self.key_to_path.get(&intent.key).cloned() {
                    let is_dir = self.key_is_dir.get(&intent.key).copied().unwrap_or(false);
                    match intent.kind {
                        TreeIntentKind::SelectionChanged => {
                            out.push(FilesIntent::SelectFile(path));
                        }
                        TreeIntentKind::Activated if !is_dir => {
                            out.push(FilesIntent::OpenFile(path));
                        }
                        _ => {}
                    }
                }
                continue;
            }
            if let Some(ContentActionPayload::MenuSelection(sel)) = &record.payload
                && record.action.target == FILES_CONTEXT_MENU_ID
            {
                if let Some(path) = selected_path
                    && !self
                        .path_to_key
                        .get(path)
                        .and_then(|k| self.key_is_dir.get(k))
                        .copied()
                        .unwrap_or(false)
                {
                    match sel.index() {
                        0 => out.push(FilesIntent::OpenFile(path.to_owned())),
                        1 => out.push(FilesIntent::AttachFile(path.to_owned())),
                        _ => {}
                    }
                }
                continue;
            }
            if record.action.target == FILES_REFRESH_BTN_ID
                && matches!(record.action.kind, ContentActionKind::Activate)
            {
                out.push(FilesIntent::Refresh);
            }
        }
        out
    }
}
