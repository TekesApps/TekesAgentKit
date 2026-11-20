//! Session Sidebar surface using `SearchFieldContent`, `SegmentedControlContent`,
//! passive `KeyedSelectableListContent` session rows, and an action toolbar (New/Pin/Rename/Archive).

use crate::ResponsiveWidthMode;
use crate::ids::*;
use std::collections::BTreeMap;
use tekes_ui_controls::{
    ButtonVariant, ScrollArea,
    composition::{
        Button, ComponentId, Content, ContentActionKind, ContentActionPayload, ContentActionRecord,
        ContentHost, KeyedSelectableListContent, ListDisplayUpdate, ListIntentKind, ListRowDisplay,
        ScrollContent, SearchCommandKind, SearchFieldContent, SegmentedControlContent,
        SelectableListContent, StackContent, TextFieldContent,
    },
    extended::{
        Axis, KeyedListItem, KeyedListSnapshot, ListBudget, ListDatasetKey, ListItemKey, Stack,
    },
};
use tekes_ui_tokens::Spacing;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SidebarFilterSelection {
    #[default]
    All,
    Pinned,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarRowModel {
    pub session_id: String,
    pub title: String,
    pub folder: String,
    pub activity: String,
    pub running: bool,
    pub pinned: bool,
    pub archived: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarIntent {
    SelectSession(String),
    CreateSession,
    SearchChanged(String),
    FilterChanged(SidebarFilterSelection),
    TogglePin(String),
    RenameSession { session_id: String, title: String },
    ArchiveSession(String),
}

#[derive(Debug)]
pub struct SidebarSurface {
    next_key: u64,
    id_to_key: BTreeMap<String, ListItemKey>,
    key_to_id: BTreeMap<ListItemKey, String>,
    last_sig: String,
    last_selected_session: Option<String>,
    last_mode: ResponsiveWidthMode,
    search_query: String,
}

impl Default for SidebarSurface {
    fn default() -> Self {
        Self::new()
    }
}

impl SidebarSurface {
    pub fn new() -> Self {
        Self {
            next_key: 1,
            id_to_key: BTreeMap::new(),
            key_to_id: BTreeMap::new(),
            last_sig: String::new(),
            last_selected_session: None,
            last_mode: ResponsiveWidthMode::Expanded,
            search_query: String::new(),
        }
    }

    fn budget() -> ListBudget {
        ListBudget {
            max_items: 512,
            max_display_items: 512,
            max_utf8_bytes: 512_000,
            max_staging_units: 64_000,
        }
    }

    pub fn build_content() -> Content {
        let mut new_btn = Button::new("新建对话");
        new_btn.variant = ButtonVariant::Primary;
        let pin_btn = Button::new("置顶");
        let archive_btn = Button::new("归档");

        let top_bar = Content::new(
            SIDEBAR_TOP_BAR_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![
                    Content::new(SIDEBAR_NEW_BTN_ID, new_btn),
                    Content::new(SIDEBAR_PIN_BTN_ID, pin_btn),
                    Content::new(SIDEBAR_ARCHIVE_BTN_ID, archive_btn),
                ],
            },
        );

        let filter = SegmentedControlContent::new("会话筛选", vec!["全部".into(), "置顶".into()]);

        let mut search = SearchFieldContent::new("搜索会话", "");
        search.set_placeholder(Some("搜索会话标题 / 目录…".to_owned()));

        let initial_snap = KeyedListSnapshot {
            dataset: ListDatasetKey(1),
            items: Vec::new(),
            display_items: Vec::new(),
        };
        let mut initial_rows = Vec::new();
        let list = SelectableListContent::keyed(
            "会话列表",
            &initial_snap,
            &mut initial_rows,
            Self::budget(),
        )
        .expect("initial empty keyed list");

        let list_scroll =
            ScrollContent::new(ScrollArea::default(), Content::new(SIDEBAR_LIST_ID, list));

        let mut rename_input = TextFieldContent::new("会话名称", "");
        rename_input.show_metadata = false;
        rename_input.set_placeholder(Some("输入新名称…".to_owned()));
        let rename_btn = Button::new("重命名");
        let rename_row = Content::new(
            SIDEBAR_RENAME_ROW_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: Some(SIDEBAR_RENAME_INPUT_ID),
                children: vec![
                    Content::new(SIDEBAR_RENAME_INPUT_ID, rename_input),
                    Content::new(SIDEBAR_RENAME_BTN_ID, rename_btn),
                ],
            },
        );

        Content::new(
            SIDEBAR_ROOT_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: Some(SIDEBAR_LIST_SCROLL_ID),
                children: vec![
                    top_bar,
                    Content::new(SIDEBAR_FILTER_ID, filter),
                    Content::new(SIDEBAR_SEARCH_ID, search),
                    Content::new(SIDEBAR_LIST_SCROLL_ID, list_scroll),
                    rename_row,
                ],
            },
        )
    }

    fn key_for(&mut self, session_id: &str) -> ListItemKey {
        if let Some(existing) = self.id_to_key.get(session_id) {
            return *existing;
        }
        let key = ListItemKey(self.next_key);
        self.next_key += 1;
        self.id_to_key.insert(session_id.to_owned(), key);
        self.key_to_id.insert(key, session_id.to_owned());
        key
    }

    pub fn sync(
        &mut self,
        host: &mut ContentHost,
        rows: &[SidebarRowModel],
        selected_session_id: Option<&str>,
        filter: SidebarFilterSelection,
        width_mode: ResponsiveWidthMode,
    ) {
        self.last_mode = width_mode;

        if let Some(seg) = host
            .get_mut(SIDEBAR_FILTER_ID)
            .and_then(|c| c.component_mut::<SegmentedControlContent>())
        {
            let target_idx = match filter {
                SidebarFilterSelection::All => 0,
                SidebarFilterSelection::Pinned => 1,
            };
            let _ = seg.set_selected(target_idx);
        }

        let mut sig = String::new();
        let mut list_items = Vec::with_capacity(rows.len());
        let mut display_items = Vec::with_capacity(rows.len());
        let mut display_rows = Vec::with_capacity(rows.len());

        for row in rows {
            let key = self.key_for(&row.session_id);
            let pin_tag = if row.pinned { "★ " } else { "" };
            let arch_tag = if row.archived { "[已归档] " } else { "" };
            let act_tag = if row.running {
                "[运行中] "
            } else if row.activity == "waiting" {
                "[待确认] "
            } else {
                ""
            };
            let label = format!("{pin_tag}{arch_tag}{act_tag}{} · {}", row.title, row.folder);
            sig.push_str(&format!("{}:{}|", key.0, label));
            list_items.push(KeyedListItem { key, name: label });
            display_items.push(key);
            display_rows.push(ListRowDisplay {
                key,
                row_id: ComponentId(SIDEBAR_ROW_BASE + key.0),
                display: ListDisplayUpdate::DefaultLabel,
            });
        }

        if sig != self.last_sig {
            let snap = KeyedListSnapshot {
                dataset: ListDatasetKey(1),
                items: list_items,
                display_items,
            };
            if host
                .replace_keyed_list(SIDEBAR_LIST_ID, &snap, &mut display_rows, None)
                .is_ok()
            {
                self.last_sig = sig;
            }
        }

        let sel_owned = selected_session_id.map(str::to_owned);
        if self.last_selected_session != sel_owned || !self.last_sig.is_empty() {
            let key = sel_owned
                .as_deref()
                .and_then(|id| self.id_to_key.get(id).copied());
            let _ = host.set_keyed_list_selection(SIDEBAR_LIST_ID, key);
            self.last_selected_session = sel_owned;
        }

        let has_selection = selected_session_id.is_some();
        for btn_id in [
            SIDEBAR_PIN_BTN_ID,
            SIDEBAR_ARCHIVE_BTN_ID,
            SIDEBAR_RENAME_BTN_ID,
        ] {
            if let Some(btn) = host
                .get_mut(btn_id)
                .and_then(|c| c.component_mut::<Button>())
            {
                btn.button.state.disabled = !has_selection;
            }
        }
    }

    pub fn handle_actions(
        &mut self,
        host: &ContentHost,
        selected_session_id: Option<&str>,
        records: &[ContentActionRecord],
    ) -> Vec<SidebarIntent> {
        let mut out = Vec::new();
        if let Some(search) = host
            .get(SIDEBAR_SEARCH_ID)
            .and_then(|c| c.component::<SearchFieldContent>())
        {
            let current = search.query().to_owned();
            if current != self.search_query {
                self.search_query = current.clone();
                out.push(SidebarIntent::SearchChanged(current));
            }
        }

        for record in records {
            if let Some(ContentActionPayload::Search(search)) = &record.payload
                && record.action.target == SIDEBAR_SEARCH_ID
            {
                let q = match search.kind {
                    SearchCommandKind::Submit => search.query.clone(),
                    SearchCommandKind::Clear => String::new(),
                };
                self.search_query = q.clone();
                out.push(SidebarIntent::SearchChanged(q));
                continue;
            }
            if let Some(ContentActionPayload::SegmentSelection(seg)) = &record.payload
                && record.action.target == SIDEBAR_FILTER_ID
            {
                out.push(SidebarIntent::FilterChanged(if seg.index() == 1 {
                    SidebarFilterSelection::Pinned
                } else {
                    SidebarFilterSelection::All
                }));
                continue;
            }
            if let Some(ContentActionPayload::ListIntent(list)) = &record.payload
                && record.action.target == SIDEBAR_LIST_ID
                && matches!(
                    list.kind,
                    ListIntentKind::SelectionChanged | ListIntentKind::Activated
                )
            {
                if let Some(sid) = self.key_to_id.get(&list.key) {
                    out.push(SidebarIntent::SelectSession(sid.clone()));
                }
                continue;
            }
            if matches!(record.action.kind, ContentActionKind::Activate) {
                match record.action.target {
                    id if id == SIDEBAR_NEW_BTN_ID => out.push(SidebarIntent::CreateSession),
                    id if id == SIDEBAR_PIN_BTN_ID => {
                        if let Some(sid) = selected_session_id {
                            out.push(SidebarIntent::TogglePin(sid.to_owned()));
                        }
                    }
                    id if id == SIDEBAR_ARCHIVE_BTN_ID => {
                        if let Some(sid) = selected_session_id {
                            out.push(SidebarIntent::ArchiveSession(sid.to_owned()));
                        }
                    }
                    id if id == SIDEBAR_RENAME_BTN_ID => {
                        if let Some(sid) = selected_session_id
                            && let Some(field) = host
                                .get(SIDEBAR_RENAME_INPUT_ID)
                                .and_then(|c| c.component::<TextFieldContent>())
                        {
                            let title = field.text().trim().to_owned();
                            if !title.is_empty() {
                                out.push(SidebarIntent::RenameSession {
                                    session_id: sid.to_owned(),
                                    title,
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        out
    }

    pub fn row_count(&self, host: &ContentHost) -> usize {
        host.get(SIDEBAR_LIST_ID)
            .and_then(|c| c.component::<KeyedSelectableListContent>())
            .map_or(0, |l| l.model().snapshot().display_items.len())
    }
}
