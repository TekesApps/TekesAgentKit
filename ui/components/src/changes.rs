//! Workspace Changes surface (`Working Tree` vs `Turn Changes`), unified diff preview via
//! `RichContent`, Diff citation capture, and capability-gated Git commit controls.

use crate::ResponsiveWidthMode;
use crate::ids::*;
use std::collections::BTreeMap;
use tekes_ui_controls::{
    ButtonVariant, ScrollArea,
    composition::{
        Button, ComponentId, Content, ContentActionKind, ContentActionPayload, ContentActionRecord,
        ContentHost, ListDisplayUpdate, ListIntentKind, ListRowDisplay, RichContent, RichDocument,
        ScrollContent, SegmentedControlContent, SelectableListContent, StackContent, TextContent,
        TextFieldContent,
    },
    extended::{
        Axis, KeyedListItem, KeyedListSnapshot, ListBudget, ListDatasetKey, ListItemKey, Stack,
    },
};
use tekes_ui_tokens::Spacing;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeRowModel {
    pub path: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangesViewModel {
    pub workspace_id: String,
    pub branch: String,
    pub git_revision: String,
    pub mode_index: usize,
    pub files: Vec<ChangeRowModel>,
    pub selected_path: Option<String>,
    pub diff_revision: String,
    pub diff_text: String,
    pub can_commit: bool,
    pub commit_status: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangesIntent {
    SelectMode(usize),
    SelectFile(String),
    QuoteDiff {
        path: String,
        revision: String,
        excerpt: String,
    },
    Commit {
        message: String,
    },
}

#[derive(Debug)]
pub struct WorkspaceChangesSurface {
    next_key: u64,
    path_to_key: BTreeMap<String, ListItemKey>,
    key_to_path: BTreeMap<ListItemKey, String>,
    last_list_sig: String,
    last_diff_sig: String,
    diff_revision_counter: u64,
}

impl Default for WorkspaceChangesSurface {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceChangesSurface {
    pub fn new() -> Self {
        Self {
            next_key: 1,
            path_to_key: BTreeMap::new(),
            key_to_path: BTreeMap::new(),
            last_list_sig: String::new(),
            last_diff_sig: String::new(),
            diff_revision_counter: 1,
        }
    }

    fn budget() -> ListBudget {
        ListBudget {
            max_items: 256,
            max_display_items: 256,
            max_utf8_bytes: 256_000,
            max_staging_units: 32_000,
        }
    }

    pub fn build_content() -> Content {
        let mode_seg = SegmentedControlContent::new(
            "变更视图模式",
            vec!["工作区变更".into(), "本轮变更".into()],
        );

        let initial_snap = KeyedListSnapshot {
            dataset: ListDatasetKey(3),
            items: Vec::new(),
            display_items: Vec::new(),
        };
        let mut initial_rows = Vec::new();
        let list = SelectableListContent::keyed(
            "变更文件清单",
            &initial_snap,
            &mut initial_rows,
            Self::budget(),
        )
        .expect("empty changes list");
        let mut list_scroll = ScrollContent::new(
            ScrollArea::default(),
            Content::new(CHANGES_FILES_LIST_ID, list),
        );
        list_scroll.viewport_height = Some(120.0);

        let diff_doc = RichDocument::from_markdown("```diff\n+ 等待加载 Diff…\n```");
        let diff_rich =
            RichContent::new("workspace-diff", 1, diff_doc).expect("valid diff RichContent");
        let diff_scroll = ScrollContent::new(
            ScrollArea::default(),
            Content::new(CHANGES_DIFF_RICH_ID, diff_rich),
        );

        let quote_btn = Button::new("引用 Diff");
        let mut commit_input = TextFieldContent::new("提交说明", "");
        commit_input.show_metadata = false;
        commit_input.set_placeholder(Some("输入提交信息 (capability-gated)…".to_owned()));
        let mut commit_btn = Button::new("提交");
        commit_btn.variant = ButtonVariant::Primary;

        let commit_row = Content::new(
            CHANGES_COMMIT_ROW_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: Some(CHANGES_COMMIT_INPUT_ID),
                children: vec![
                    Content::new(CHANGES_QUOTE_BTN_ID, quote_btn),
                    Content::new(CHANGES_COMMIT_INPUT_ID, commit_input),
                    Content::new(CHANGES_COMMIT_BTN_ID, commit_btn),
                ],
            },
        );

        Content::new(
            CHANGES_ROOT_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: Some(CHANGES_DIFF_SCROLL_ID),
                children: vec![
                    Content::new(CHANGES_HEADER_ID, TextContent::new("工作区变更")),
                    Content::new(CHANGES_MODE_SEG_ID, mode_seg),
                    Content::new(CHANGES_FILES_SCROLL_ID, list_scroll),
                    Content::new(CHANGES_DIFF_SCROLL_ID, diff_scroll),
                    commit_row,
                ],
            },
        )
    }

    fn key_for(&mut self, path: &str) -> ListItemKey {
        if let Some(k) = self.path_to_key.get(path) {
            return *k;
        }
        let k = ListItemKey(self.next_key);
        self.next_key += 1;
        self.path_to_key.insert(path.to_owned(), k);
        self.key_to_path.insert(k, path.to_owned());
        k
    }

    pub fn sync(
        &mut self,
        host: &mut ContentHost,
        model: &ChangesViewModel,
        _width_mode: ResponsiveWidthMode,
    ) {
        if let Some(header) = host
            .get_mut(CHANGES_HEADER_ID)
            .and_then(|c| c.component_mut::<TextContent>())
        {
            let status_note = model
                .commit_status
                .as_deref()
                .map(|s| format!(" · {s}"))
                .unwrap_or_default();
            header.text = format!(
                "变更 · {}@{} ({}) · {} 项{status_note}",
                model.workspace_id,
                model.branch,
                model.git_revision,
                model.files.len(),
            );
        }

        if let Some(seg) = host
            .get_mut(CHANGES_MODE_SEG_ID)
            .and_then(|c| c.component_mut::<SegmentedControlContent>())
        {
            let _ = seg.set_selected(model.mode_index.min(1));
        }

        let mut sig = String::new();
        let mut list_items = Vec::with_capacity(model.files.len());
        let mut display_items = Vec::with_capacity(model.files.len());
        let mut rows = Vec::with_capacity(model.files.len());

        for item in &model.files {
            let key = self.key_for(&item.path);
            sig.push_str(&format!("{}:{}|", key.0, item.label));
            list_items.push(KeyedListItem {
                key,
                name: item.label.clone(),
            });
            display_items.push(key);
            rows.push(ListRowDisplay {
                key,
                row_id: ComponentId(CHANGES_ROW_BASE + key.0),
                display: ListDisplayUpdate::DefaultLabel,
            });
        }

        if sig != self.last_list_sig {
            let snap = KeyedListSnapshot {
                dataset: ListDatasetKey(3),
                items: list_items,
                display_items,
            };
            if host
                .replace_keyed_list(CHANGES_FILES_LIST_ID, &snap, &mut rows, None)
                .is_ok()
            {
                self.last_list_sig = sig;
            }
        }

        let sel_key = model
            .selected_path
            .as_deref()
            .and_then(|p| self.path_to_key.get(p).copied());
        let _ = host.set_keyed_list_selection(CHANGES_FILES_LIST_ID, sel_key);

        let diff_sig = format!(
            "{}:{}:{}",
            model.selected_path.as_deref().unwrap_or(""),
            model.diff_revision,
            model.diff_text
        );
        if diff_sig != self.last_diff_sig {
            let md = if model.diff_text.trim().is_empty() {
                "当前无 Diff 内容。".to_owned()
            } else {
                format!("```diff\n{}\n```", model.diff_text)
            };
            let next_rev = self.diff_revision_counter.saturating_add(1);
            let doc = RichDocument::from_markdown(&md);
            if let Some(rich) = host
                .get_mut(CHANGES_DIFF_RICH_ID)
                .and_then(|c| c.component_mut::<RichContent>())
            {
                let expected = rich.version();
                if rich.update(&expected, next_rev, doc).is_ok() {
                    self.diff_revision_counter = next_rev;
                    self.last_diff_sig = diff_sig;
                }
            }
        }

        if let Some(quote_btn) = host
            .get_mut(CHANGES_QUOTE_BTN_ID)
            .and_then(|c| c.component_mut::<Button>())
        {
            quote_btn.button.state.disabled = model.diff_text.trim().is_empty();
        }
        if let Some(commit_btn) = host
            .get_mut(CHANGES_COMMIT_BTN_ID)
            .and_then(|c| c.component_mut::<Button>())
        {
            commit_btn.button.state.disabled = !model.can_commit;
        }
    }

    pub fn handle_actions(
        &mut self,
        host: &ContentHost,
        model: &ChangesViewModel,
        records: &[ContentActionRecord],
    ) -> Vec<ChangesIntent> {
        let mut out = Vec::new();
        for record in records {
            if let Some(ContentActionPayload::SegmentSelection(seg)) = &record.payload
                && record.action.target == CHANGES_MODE_SEG_ID
            {
                out.push(ChangesIntent::SelectMode(seg.index()));
                continue;
            }
            if let Some(ContentActionPayload::ListIntent(list)) = &record.payload
                && record.action.target == CHANGES_FILES_LIST_ID
                && matches!(
                    list.kind,
                    ListIntentKind::SelectionChanged | ListIntentKind::Activated
                )
            {
                if let Some(path) = self.key_to_path.get(&list.key) {
                    out.push(ChangesIntent::SelectFile(path.clone()));
                }
                continue;
            }
            if matches!(record.action.kind, ContentActionKind::Activate) {
                match record.action.target {
                    id if id == CHANGES_QUOTE_BTN_ID => {
                        if let Some(path) = &model.selected_path
                            && !model.diff_text.trim().is_empty()
                        {
                            out.push(ChangesIntent::QuoteDiff {
                                path: path.clone(),
                                revision: model.diff_revision.clone(),
                                excerpt: model.diff_text.clone(),
                            });
                        }
                    }
                    id if id == CHANGES_COMMIT_BTN_ID && model.can_commit => {
                        let msg = host
                            .get(CHANGES_COMMIT_INPUT_ID)
                            .and_then(|c| c.component::<TextFieldContent>())
                            .map(|f| f.text().trim().to_owned())
                            .filter(|s| !s.is_empty())
                            .unwrap_or_else(|| "chore: commit workspace changes".to_owned());
                        out.push(ChangesIntent::Commit { message: msg });
                    }
                    _ => {}
                }
            }
        }
        out
    }
}
