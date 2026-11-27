//! Session Sidebar surface aligned with Tekes: navigation list with icons, minimal icon
//! header actions (`+` new thread & collapsible Search), macOS context menu (`Pin`, `Rename`,
//! `Archive`), and bottom `Settings` action.

use crate::ResponsiveWidthMode;
use crate::ids::*;
use crate::ui_helpers::{caption_text, heading_text, icon_button, icon_row, icon_text_button};
use std::collections::BTreeMap;
use tekes_ui_controls::{
    ButtonVariant, ScrollArea,
    composition::{
        Button, ComponentId, Content, ContentActionKind, ContentActionPayload, ContentActionRecord,
        ContentActionSource, ContentHost, KeyedSelectableListContent, ListDisplayUpdate,
        ListIntentKind, ListRowDisplay, MenuContent, MenuEntry, MenuSelection, ScrollContent,
        SearchCommandKind, SearchFieldContent, SelectableListContent, StackContent,
        TextFieldContent, TextSubmitPolicy,
    },
    extended::{KeyedListItem, KeyedListSnapshot, ListBudget, ListDatasetKey, ListItemKey},
    layout::Insets,
};
use tekes_ui_tokens::{Spacing, proposal::layout::MainAxisDistribution};

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
    UnarchiveSession(String),
    OpenSettings,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RowContextMenuState {
    session_id: String,
    row_id: ComponentId,
    row_mount: u64,
    menu_id: ComponentId,
    archived: bool,
}

#[derive(Debug)]
pub struct SidebarSurface {
    next_key: u64,
    id_to_key: BTreeMap<String, ListItemKey>,
    key_to_id: BTreeMap<ListItemKey, String>,
    row_archived: BTreeMap<String, bool>,
    row_context_menus: BTreeMap<ComponentId, RowContextMenuState>,
    last_row_sigs: BTreeMap<ListItemKey, String>,
    last_sig: String,
    last_selected_session: Option<String>,
    last_menu_archived: Option<bool>,
    open_list_menu_target: Option<String>,
    last_fallback_menu_target: Option<String>,
    last_mode: ResponsiveWidthMode,
    search_query: String,
    search_open: bool,
    renaming_session_id: Option<String>,
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
            row_archived: BTreeMap::new(),
            row_context_menus: BTreeMap::new(),
            last_row_sigs: BTreeMap::new(),
            last_sig: String::new(),
            last_selected_session: None,
            last_menu_archived: None,
            open_list_menu_target: None,
            last_fallback_menu_target: None,
            last_mode: ResponsiveWidthMode::Expanded,
            search_query: String::new(),
            search_open: false,
            renaming_session_id: None,
        }
    }

    fn context_menu_entries(archived: bool) -> Vec<MenuEntry> {
        vec![
            MenuEntry::item("pin", "Pin / Unpin"),
            MenuEntry::item("rename", "Rename"),
            if archived {
                MenuEntry::item("unarchive", "Unarchive")
            } else {
                MenuEntry::item("archive", "Archive")
            },
        ]
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
        let new_btn = icon_button(SIDEBAR_NEW_ICON_ID, "tekes-new", "New thread");
        let search_toggle = icon_button(SIDEBAR_SEARCH_ICON_ID, "tekes-search", "Search threads");

        let top_actions = Content::new(
            SIDEBAR_TOP_ACTIONS_ID,
            StackContent::horizontal(
                Spacing::Micro.value(),
                vec![
                    Content::new(SIDEBAR_NEW_BTN_ID, new_btn),
                    Content::new(SIDEBAR_SEARCH_TOGGLE_BTN_ID, search_toggle),
                ],
            ),
        );

        let top_bar = StackContent::horizontal(
            Spacing::Tight.value(),
            vec![
                Content::new(SIDEBAR_BRAND_LABEL_ID, heading_text("Tekes")),
                top_actions,
            ],
        )
        .into_content_with_distribution(SIDEBAR_TOP_BAR_ID, MainAxisDistribution::SpaceBetween);

        let search_slot = Content::new(
            SIDEBAR_SEARCH_SLOT_ID,
            StackContent::vertical(0.0, Vec::new()),
        );

        let section_caption = Content::new(SIDEBAR_SECTION_CAPTION_ID, caption_text("Projects"));

        let initial_snap = KeyedListSnapshot {
            dataset: ListDatasetKey(1),
            items: Vec::new(),
            display_items: Vec::new(),
        };
        let mut initial_rows = Vec::new();
        let mut list = SelectableListContent::keyed(
            "Sessions",
            &initial_snap,
            &mut initial_rows,
            Self::budget(),
        )
        .expect("initial empty keyed list");
        list.set_navigation_layout(true);
        list.set_navigation_selection_fill_only(true);

        let list_scroll =
            ScrollContent::new(ScrollArea::default(), Content::new(SIDEBAR_LIST_ID, list))
                .scrollbar(true);

        let rename_slot = Content::new(
            SIDEBAR_RENAME_ROW_ID,
            StackContent::horizontal(Spacing::Tight.value(), Vec::new()),
        );

        let settings_btn = icon_text_button(
            ComponentId(624),
            SIDEBAR_SETTINGS_ICON_ID,
            ComponentId(625),
            "tekes-settings",
            "Settings",
        );

        let context_menu = Content::new(
            SIDEBAR_CONTEXT_MENU_ID,
            MenuContent::context_menu("Thread actions", Self::context_menu_entries(false))
                .expect("valid sidebar context menu entries"),
        );
        let row_context_menus = Content::new(
            SIDEBAR_CONTEXT_MENUS_ID,
            StackContent::vertical(0.0, Vec::new()),
        );

        StackContent::vertical(
            Spacing::Regular.value(),
            vec![
                top_bar,
                search_slot,
                section_caption,
                Content::new(SIDEBAR_LIST_SCROLL_ID, list_scroll),
                rename_slot,
                Content::new(SIDEBAR_SETTINGS_BTN_ID, settings_btn),
                context_menu,
                row_context_menus,
            ],
        )
        .with_flexible_child(SIDEBAR_LIST_SCROLL_ID)
        .into_content_with_padding(SIDEBAR_ROOT_ID, Insets::new(12.0, 12.0, 12.0, 12.0))
    }

    pub fn bind_context_menu(host: &mut ContentHost) {
        let _ = host.bind_context_menu(SIDEBAR_LIST_ID, SIDEBAR_CONTEXT_MENU_ID);
    }

    /// Context menu `ComponentId` bound to `session_id`'s rendered row, if currently mounted.
    pub fn row_context_menu_id(&self, session_id: &str) -> Option<ComponentId> {
        self.row_context_menus
            .values()
            .find(|state| state.session_id == session_id)
            .map(|state| state.menu_id)
    }

    /// Currently open context menu `ComponentId` (row-targeted or list-level), if any.
    pub fn open_context_menu_id(&self, host: &ContentHost) -> Option<ComponentId> {
        self.row_context_menus
            .keys()
            .copied()
            .chain(std::iter::once(SIDEBAR_CONTEXT_MENU_ID))
            .find(|menu_id| {
                host.get(*menu_id)
                    .and_then(|node| node.component::<MenuContent>())
                    .is_some_and(MenuContent::is_open)
            })
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

    fn set_search_visible(&self, host: &mut ContentHost, visible: bool) {
        let is_mounted = host.get(SIDEBAR_SEARCH_ID).is_some();
        if visible && !is_mounted {
            let mut search = SearchFieldContent::new("Search threads", &self.search_query);
            search.set_placeholder(Some("Search threads…".to_owned()));
            replace_stack_children(
                host,
                SIDEBAR_SEARCH_SLOT_ID,
                vec![Content::new(SIDEBAR_SEARCH_ID, search)],
            );
        } else if !visible && is_mounted {
            replace_stack_children(host, SIDEBAR_SEARCH_SLOT_ID, Vec::new());
        }
    }

    fn set_rename_visible(&self, host: &mut ContentHost, visible: bool, initial_title: &str) {
        let is_mounted = host.get(SIDEBAR_RENAME_INPUT_ID).is_some();
        if visible && !is_mounted {
            let mut input = TextFieldContent::new("Thread name", initial_title);
            input.show_metadata = false;
            input.submit_policy = TextSubmitPolicy::Enter;
            input.set_placeholder(Some("Rename thread…".to_owned()));
            let mut save_btn = Button::new("Save");
            save_btn.set_variant(ButtonVariant::Primary);
            let mut cancel_btn = Button::new("×").with_accessible_name("Cancel rename");
            cancel_btn.set_variant(ButtonVariant::Quiet);
            if let Some(slot) = host
                .get_mut(SIDEBAR_RENAME_ROW_ID)
                .and_then(|c| c.component_mut::<StackContent>())
            {
                slot.set_flexible_child(Some(SIDEBAR_RENAME_INPUT_ID));
            }
            replace_stack_children(
                host,
                SIDEBAR_RENAME_ROW_ID,
                vec![
                    Content::new(SIDEBAR_RENAME_INPUT_ID, input),
                    Content::new(SIDEBAR_RENAME_BTN_ID, save_btn),
                    Content::new(SIDEBAR_RENAME_CANCEL_BTN_ID, cancel_btn),
                ],
            );
            let _ = host.focus(Some(SIDEBAR_RENAME_INPUT_ID), true);
        } else if !visible && is_mounted {
            if let Some(slot) = host
                .get_mut(SIDEBAR_RENAME_ROW_ID)
                .and_then(|c| c.component_mut::<StackContent>())
            {
                slot.set_flexible_child(None);
            }
            replace_stack_children(host, SIDEBAR_RENAME_ROW_ID, Vec::new());
        }
    }

    fn resolve_list_context_target(
        &self,
        host: &ContentHost,
        selected_session_id: Option<&str>,
    ) -> Option<String> {
        host.get(SIDEBAR_LIST_ID)
            .and_then(|c| c.component::<KeyedSelectableListContent>())
            .and_then(|list| list.focused_key().or_else(|| list.selected_key()))
            .and_then(|key| self.key_to_id.get(&key))
            .filter(|sid| self.row_archived.contains_key(*sid))
            .cloned()
            .or_else(|| {
                selected_session_id
                    .filter(|sid| self.row_archived.contains_key(*sid))
                    .map(str::to_owned)
            })
    }

    fn retire_list_context_menu(host: &mut ContentHost) {
        if host
            .bind_context_menu(SIDEBAR_LIST_ID, SIDEBAR_CONTEXT_MENU_ID)
            .is_err()
            && let Some(menu) = host
                .get_mut(SIDEBAR_CONTEXT_MENU_ID)
                .and_then(|c| c.component_mut::<MenuContent>())
        {
            menu.close();
        }
    }

    pub fn sync(
        &mut self,
        host: &mut ContentHost,
        rows: &[SidebarRowModel],
        selected_session_id: Option<&str>,
        _filter: SidebarFilterSelection,
        width_mode: ResponsiveWidthMode,
    ) {
        self.last_mode = width_mode;

        let list_menu_open_before = host
            .get(SIDEBAR_CONTEXT_MENU_ID)
            .and_then(|c| c.component::<MenuContent>())
            .is_some_and(MenuContent::is_open);
        if list_menu_open_before {
            if self.open_list_menu_target.is_none() {
                self.open_list_menu_target = self
                    .resolve_list_context_target(host, self.last_selected_session.as_deref())
                    .or_else(|| self.last_fallback_menu_target.clone());
            }
        } else {
            self.open_list_menu_target = None;
        }

        let show_search = self.search_open || !self.search_query.is_empty();
        self.set_search_visible(host, show_search);

        let mut sig = String::new();
        let mut new_row_sigs = BTreeMap::new();
        let mut list_items = Vec::with_capacity(rows.len());
        let mut display_items = Vec::with_capacity(rows.len());
        let mut display_rows = Vec::with_capacity(rows.len());

        self.row_archived.clear();
        for row in rows {
            self.row_archived
                .insert(row.session_id.clone(), row.archived);
            let key = self.key_for(&row.session_id);
            let icon_name = if row.running {
                "tekes-progress"
            } else if row.pinned {
                "tekes-pinned"
            } else if row.archived {
                "tekes-archive"
            } else {
                "tekes-side-chat"
            };
            let title = if row.archived {
                format!("{} (Archived)", row.title)
            } else {
                row.title.clone()
            };
            let row_sig = format!("{}:{}:{icon_name}:{}", key.0, row.folder, title);
            sig.push_str(&row_sig);
            sig.push('|');
            list_items.push(KeyedListItem {
                key,
                name: title.clone(),
            });
            display_items.push(key);
            let display = if self.last_row_sigs.get(&key) == Some(&row_sig) {
                ListDisplayUpdate::Keep
            } else {
                let decor_base = SIDEBAR_ROW_DECOR_BASE + key.0 * 4;
                ListDisplayUpdate::Replace(icon_row(
                    ComponentId(decor_base),
                    ComponentId(decor_base + 1),
                    ComponentId(decor_base + 2),
                    icon_name,
                    title,
                ))
            };
            new_row_sigs.insert(key, row_sig);
            display_rows.push(ListRowDisplay {
                key,
                row_id: ComponentId(SIDEBAR_ROW_BASE + key.0),
                display,
            });
        }

        let mut list_replaced = false;
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
                self.last_row_sigs = new_row_sigs;
                list_replaced = true;
            }
        }

        let sel_owned = selected_session_id.map(str::to_owned);
        if list_replaced || self.last_selected_session != sel_owned {
            let key = sel_owned
                .as_deref()
                .and_then(|id| self.id_to_key.get(id).copied());
            let _ = host.set_keyed_list_selection(SIDEBAR_LIST_ID, key);
            self.last_selected_session = sel_owned;
        }

        if let Some(sid) = self.renaming_session_id.as_deref()
            && !self.row_archived.contains_key(sid)
        {
            self.renaming_session_id = None;
            self.set_rename_visible(host, false, "");
        }

        let obsolete_menus: Vec<ComponentId> = self
            .row_context_menus
            .iter()
            .filter_map(|(menu_id, state)| {
                (!self.row_archived.contains_key(&state.session_id)).then_some(*menu_id)
            })
            .collect();
        for menu_id in obsolete_menus {
            self.row_context_menus.remove(&menu_id);
            let _ = host.remove(menu_id);
        }

        for row in rows {
            let Some(&key) = self.id_to_key.get(&row.session_id) else {
                continue;
            };
            let row_id = ComponentId(SIDEBAR_ROW_BASE + key.0);
            let menu_id = ComponentId(SIDEBAR_ROW_CONTEXT_MENU_BASE + key.0);
            let Some(row_mount) = host.get(row_id).map(|node| node.mount_identity()) else {
                continue;
            };
            if let Some(existing) = self.row_context_menus.get_mut(&menu_id) {
                if existing.archived != row.archived
                    && let Some(menu) = host
                        .get_mut(menu_id)
                        .and_then(|c| c.component_mut::<MenuContent>())
                    && menu
                        .set_entries(Self::context_menu_entries(row.archived))
                        .is_ok()
                {
                    existing.archived = row.archived;
                }
                if (existing.row_mount != row_mount
                    || existing.row_id != row_id
                    || existing.session_id != row.session_id)
                    && host.bind_context_menu(row_id, menu_id).is_ok()
                {
                    existing.session_id = row.session_id.clone();
                    existing.row_id = row_id;
                    existing.row_mount = row_mount;
                }
            } else if let Ok(menu) = MenuContent::context_menu(
                "Thread actions",
                Self::context_menu_entries(row.archived),
            ) && host
                .insert(SIDEBAR_CONTEXT_MENUS_ID, Content::new(menu_id, menu))
                .is_ok()
            {
                if host.bind_context_menu(row_id, menu_id).is_ok() {
                    self.row_context_menus.insert(
                        menu_id,
                        RowContextMenuState {
                            session_id: row.session_id.clone(),
                            row_id,
                            row_mount,
                            menu_id,
                            archived: row.archived,
                        },
                    );
                } else {
                    let _ = host.remove(menu_id);
                }
            }
        }

        let fallback_target = self.resolve_list_context_target(host, selected_session_id);
        let fallback_archived = fallback_target
            .as_deref()
            .and_then(|sid| self.row_archived.get(sid).copied())
            .unwrap_or(false);
        let list_menu_open_after = host
            .get(SIDEBAR_CONTEXT_MENU_ID)
            .and_then(|c| c.component::<MenuContent>())
            .is_some_and(MenuContent::is_open);
        if list_menu_open_after {
            let target_stale = self.open_list_menu_target.as_ref() != fallback_target.as_ref()
                || self
                    .open_list_menu_target
                    .as_deref()
                    .is_none_or(|sid| !self.row_archived.contains_key(sid))
                || self.last_menu_archived != Some(fallback_archived);
            if target_stale {
                Self::retire_list_context_menu(host);
                self.open_list_menu_target = None;
            }
        } else {
            self.open_list_menu_target = None;
        }
        self.last_fallback_menu_target = fallback_target;
        if self.last_menu_archived != Some(fallback_archived)
            && let Some(menu) = host
                .get_mut(SIDEBAR_CONTEXT_MENU_ID)
                .and_then(|c| c.component_mut::<MenuContent>())
            && menu
                .set_entries(Self::context_menu_entries(fallback_archived))
                .is_ok()
        {
            self.last_menu_archived = Some(fallback_archived);
        }
    }

    fn dispatch_menu_action(
        &mut self,
        host: &mut ContentHost,
        sid: &str,
        sel: &MenuSelection,
        out: &mut Vec<SidebarIntent>,
    ) {
        let Some(&archived) = self.row_archived.get(sid) else {
            return;
        };
        match sel.value_path() {
            Some([action]) if action == "pin" => {
                out.push(SidebarIntent::TogglePin(sid.to_owned()));
            }
            Some([action]) if action == "rename" => {
                self.renaming_session_id = Some(sid.to_owned());
                self.set_rename_visible(host, true, "");
            }
            Some([action]) if action == "archive" => {
                out.push(SidebarIntent::ArchiveSession(sid.to_owned()));
            }
            Some([action]) if action == "unarchive" => {
                out.push(SidebarIntent::UnarchiveSession(sid.to_owned()));
            }
            Some(_) => {}
            None => match sel.index() {
                0 => out.push(SidebarIntent::TogglePin(sid.to_owned())),
                1 => {
                    self.renaming_session_id = Some(sid.to_owned());
                    self.set_rename_visible(host, true, "");
                }
                2 => {
                    if archived {
                        out.push(SidebarIntent::UnarchiveSession(sid.to_owned()));
                    } else {
                        out.push(SidebarIntent::ArchiveSession(sid.to_owned()));
                    }
                }
                _ => {}
            },
        }
    }

    fn submit_rename(&mut self, host: &mut ContentHost, out: &mut Vec<SidebarIntent>) {
        if let Some(sid) = self
            .renaming_session_id
            .as_deref()
            .filter(|sid| self.row_archived.contains_key(*sid))
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
        self.renaming_session_id = None;
        self.set_rename_visible(host, false, "");
    }

    pub fn handle_actions(
        &mut self,
        host: &mut ContentHost,
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
                if q.is_empty() {
                    self.search_open = false;
                }
                out.push(SidebarIntent::SearchChanged(q));
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
            if let Some(ContentActionPayload::MenuSelection(sel)) = &record.payload {
                if let Some(state) = self.row_context_menus.get(&record.action.target).cloned() {
                    let request_matches = sel.context_request().is_some_and(|req| {
                        req.owner() == state.row_id && req.owner_mount() == state.row_mount
                    });
                    let row_still_mounted = host
                        .get(state.row_id)
                        .is_some_and(|node| node.mount_identity() == state.row_mount);
                    if request_matches && row_still_mounted {
                        self.dispatch_menu_action(host, &state.session_id, sel, &mut out);
                    }
                    continue;
                }
                if record.action.target == SIDEBAR_CONTEXT_MENU_ID {
                    if sel
                        .context_request()
                        .is_some_and(|req| req.source() == ContentActionSource::Pointer)
                    {
                        continue;
                    }
                    let _ = selected_session_id;
                    let target_sid = self
                        .open_list_menu_target
                        .take()
                        .filter(|sid| self.row_archived.contains_key(sid))
                        .or_else(|| {
                            host.get(SIDEBAR_LIST_ID)
                                .and_then(|c| c.component::<KeyedSelectableListContent>())
                                .and_then(|list| {
                                    if let Some(focused) = list.focused_key() {
                                        self.key_to_id
                                            .get(&focused)
                                            .filter(|sid| self.row_archived.contains_key(*sid))
                                            .cloned()
                                    } else {
                                        list.selected_key()
                                            .and_then(|selected| self.key_to_id.get(&selected))
                                            .filter(|sid| {
                                                self.row_archived.contains_key(*sid)
                                                    && self.last_fallback_menu_target.as_deref()
                                                        == Some(sid.as_str())
                                            })
                                            .cloned()
                                    }
                                })
                        });
                    if let Some(sid) = target_sid {
                        self.dispatch_menu_action(host, &sid, sel, &mut out);
                    }
                    continue;
                }
            }
            if record.action.target == SIDEBAR_RENAME_INPUT_ID
                && matches!(record.action.kind, ContentActionKind::Submit)
            {
                self.submit_rename(host, &mut out);
                continue;
            }
            if matches!(record.action.kind, ContentActionKind::Activate) {
                match record.action.target {
                    id if id == SIDEBAR_NEW_BTN_ID => out.push(SidebarIntent::CreateSession),
                    id if id == SIDEBAR_SEARCH_TOGGLE_BTN_ID => {
                        self.search_open = !self.search_open;
                        if !self.search_open && !self.search_query.is_empty() {
                            self.search_query.clear();
                            out.push(SidebarIntent::SearchChanged(String::new()));
                        }
                        self.set_search_visible(host, self.search_open);
                        if self.search_open {
                            let _ = host.focus(Some(SIDEBAR_SEARCH_ID), true);
                        }
                    }
                    id if id == SIDEBAR_RENAME_BTN_ID => self.submit_rename(host, &mut out),
                    id if id == SIDEBAR_RENAME_CANCEL_BTN_ID => {
                        self.renaming_session_id = None;
                        self.set_rename_visible(host, false, "");
                    }
                    id if id == SIDEBAR_SETTINGS_BTN_ID => out.push(SidebarIntent::OpenSettings),
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
