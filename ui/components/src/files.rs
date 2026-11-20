//! Workspace Files tree & search surface backed by passive `TreeContent` and `SearchFieldContent`.

use crate::ResponsiveWidthMode;
use crate::ids::*;
use std::collections::BTreeMap;
use tekes_ui_controls::{
    ButtonVariant, ScrollArea,
    composition::{
        Button, ComponentId, Content, ContentActionKind, ContentActionPayload, ContentActionRecord,
        ContentHost, ScrollContent, SearchCommandKind, SearchFieldContent, StackContent,
        TextContent, TreeContent, TreeContentNode, TreeDisplayUpdate, TreeIntentKind,
    },
    extended::{Axis, KeyedTreeNode, Stack, TreeDatasetKey, TreeNodeKey},
};
use tekes_ui_tokens::Spacing;

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
        let mut search = SearchFieldContent::new("搜索工作区文件", "");
        search.set_placeholder(Some("按路径或文件名过滤…".to_owned()));

        let tree = TreeContent::new("工作区目录树", TreeDatasetKey(1), Vec::new())
            .expect("empty TreeContent valid");
        let tree_scroll =
            ScrollContent::new(ScrollArea::default(), Content::new(FILES_TREE_ID, tree));

        let mut open_btn = Button::new("在 Viewer 打开");
        open_btn.variant = ButtonVariant::Primary;
        let attach_btn = Button::new("附加到对话");
        let mut refresh_btn = Button::new("刷新");
        refresh_btn.variant = ButtonVariant::Quiet;

        let actions = Content::new(
            FILES_ACTIONS_ROW_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![
                    Content::new(FILES_OPEN_BTN_ID, open_btn),
                    Content::new(FILES_ATTACH_BTN_ID, attach_btn),
                    Content::new(FILES_REFRESH_BTN_ID, refresh_btn),
                ],
            },
        );

        Content::new(
            FILES_ROOT_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: Some(FILES_TREE_SCROLL_ID),
                children: vec![
                    Content::new(FILES_HEADER_ID, TextContent::new("工作区文件")),
                    Content::new(FILES_SEARCH_ID, search),
                    Content::new(FILES_TREE_SCROLL_ID, tree_scroll),
                    actions,
                ],
            },
        )
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
            header.text = format!("工作区文件 ({}) · {} 项", workspace_id, items.len());
        }

        let mut sig = String::new();
        let mut tree_nodes = Vec::with_capacity(items.len());
        self.key_is_dir.clear();

        for item in items {
            let key = self.key_for(&item.path);
            let parent = item.parent_path.as_deref().map(|p| self.key_for(p));
            self.key_is_dir.insert(key, item.is_dir);
            let label = if item.is_dir {
                format!("▸ {}/", item.name)
            } else {
                item.name.clone()
            };
            sig.push_str(&format!("{}:{:?}:{}|", key.0, parent, label));
            tree_nodes.push(TreeContentNode {
                row_id: ComponentId(FILES_ROW_BASE + key.0),
                node: KeyedTreeNode {
                    key,
                    parent,
                    label,
                    disabled: false,
                    initially_expanded: true,
                },
                display: TreeDisplayUpdate::DefaultLabel,
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

        let selected_is_file = selected_path
            .and_then(|p| self.path_to_key.get(p))
            .is_some_and(|k| !self.key_is_dir.get(k).copied().unwrap_or(false));

        for id in [FILES_OPEN_BTN_ID, FILES_ATTACH_BTN_ID] {
            if let Some(btn) = host.get_mut(id).and_then(|c| c.component_mut::<Button>()) {
                btn.button.state.disabled = !selected_is_file;
            }
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
            if matches!(record.action.kind, ContentActionKind::Activate) {
                match record.action.target {
                    id if id == FILES_OPEN_BTN_ID => {
                        if let Some(p) = selected_path {
                            out.push(FilesIntent::OpenFile(p.to_owned()));
                        }
                    }
                    id if id == FILES_ATTACH_BTN_ID => {
                        if let Some(p) = selected_path {
                            out.push(FilesIntent::AttachFile(p.to_owned()));
                        }
                    }
                    id if id == FILES_REFRESH_BTN_ID => {
                        out.push(FilesIntent::Refresh);
                    }
                    _ => {}
                }
            }
        }
        out
    }
}
