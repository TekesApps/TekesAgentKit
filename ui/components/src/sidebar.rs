//! Agent session inventory and shared actions. No local pin database or action policy.
use crate::SidebarSelectionNavigation;
use crate::sidebar_navigation::{
    DisclosureTarget, NavigationReveal, PendingNavigation, plan_reveal,
};
use crate::summary_section::{Expansion, SectionHeaderKind, SummarySectionView};
use crate::{
    SidebarFolderKey, SidebarObserverLabels, SidebarPreferenceChange, SidebarPreferencesSnapshot,
    SidebarSearchInput, SidebarSearchIntent, SidebarSearchKey, SidebarSearchLabels, SidebarSort,
    SidebarTitleComparator, ordered_sidebar_rows, sidebar_folder_title,
    sidebar_search::{SearchFooter, SearchInputView},
};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
pub use tekes_agent_contract::{
    SessionAddress, ThreadActionDefinition, ThreadActionsPresentation, ThreadActivity,
    ThreadPinPresentation,
};
use tekes_ui_controls::{
    ButtonTextRenderer, ScrollArea,
    composition::*,
    extended::{Axis, Disclosure, Stack},
    layout::MeasuredChild,
    paint::ButtonVariant,
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::Spacing;

#[derive(Clone, Debug, PartialEq)]
pub struct SidebarRow {
    /// Original conversation folder from canonical inventory or a host Primary
    /// Folder adapter; never the current tool execution directory or routing ID.
    pub primary_folder_id: Option<String>,
    pub folder_path: Option<String>,
    pub address: SessionAddress,
    pub title: String,
    pub activity: ThreadActivity,
    pub pin: Option<ThreadPinPresentation>,
    pub actions: ThreadActionsPresentation,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SidebarInput {
    /// Common session membership filter applied before pin/display partitioning.
    pub folder_filter: Option<crate::SidebarFolderKey>,
    pub search_help: Option<crate::SidebarSearchHelp>,
    /// Monotonic publication revision from one host inventory authority.
    pub revision: u64,
    /// False until the first coherent inventory has restored. Rows supplied before
    /// restoration are not visible or actionable; empty is only confirmed when true.
    pub loaded: bool,
    /// Observation failure is independent of absence. Keep the last readable rows.
    pub error: Option<String>,
    /// Order is supplied by the consumer's grouping/sorting preferences, never by pin storage.
    pub rows: Vec<SidebarRow>,
    pub selected: Option<SessionAddress>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum SidebarIntent {
    Select {
        address: SessionAddress,
    },
    /// Execute/rename/copy interaction remains the exact shared definition. Recheck on dispatch.
    Action {
        address: SessionAddress,
        definition: ThreadActionDefinition,
    },
}
#[derive(Clone, Debug)]
pub struct SidebarLabels {
    pub name: String,
    pub empty: String,
    pub restoring: String,
    pub actions: String,
    pub pinned: String,
    pub no_folder: String,
    pub more: String,
    pub activity: fn(&ThreadActivity) -> String,
}
impl Default for SidebarLabels {
    fn default() -> Self {
        Self {
            name: "会话".into(),
            empty: "暂无会话".into(),
            restoring: "正在恢复会话…".into(),
            actions: "会话操作".into(),
            pinned: "已置顶".into(),
            no_folder: "无文件夹".into(),
            more: "显示更多会话".into(),
            activity: activity_label,
        }
    }
}
fn activity_label(activity: &ThreadActivity) -> String {
    if activity.is_stale {
        return "状态待更新".into();
    }
    match activity.status.as_str() {
        "running" => "执行中",
        "queued" => "排队中",
        "waitingForInput" => "等待输入",
        "completed" => "已完成",
        "notStarted" => "未运行",
        "failed" => "失败",
        "cancelled" => "已取消",
        "idle" => "空闲",
        _ => "状态未知",
    }
    .into()
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    More(u64),
    FolderRoot(SidebarFolderKey),
    FolderHeader(SidebarFolderKey, u64),
    FolderLabel(SidebarFolderKey, u64),
    FolderBody(SidebarFolderKey),
    PinnedHeader(u64),
    PinnedLabel(u64),
    Row(SessionAddress),
    Select(SessionAddress),
    Status(SessionAddress),
    Action(SessionAddress, String),
}

/// Mountable sidebar. All dynamic IDs live in the supplied unique upper-32-bit namespace.
/// Deliver and drain host actions before update(), just as for other public content controls.
#[derive(Debug)]
pub struct Sidebar {
    page_owner: std::sync::Arc<()>,
    visibility_honored: Cell<Option<u64>>,
    page_limit: usize,
    page_facts: crate::SidebarPagingFacts,
    page_request: Option<crate::SidebarPageRequest>,
    preferences: SidebarPreferencesSnapshot,
    preferences_bound: bool,
    preferences_generation: u64,
    preferences_writable: bool,
    title_comparator: Option<SidebarTitleComparator>,
    pinned_expansion: Expansion,
    folder_expansions: BTreeMap<SidebarFolderKey, Expansion>,
    preference_bindings: BTreeMap<ComponentId, SidebarPreferenceChange>,
    disclosure_targets: BTreeMap<ComponentId, DisclosureTarget>,
    navigation_reveal: RefCell<NavigationReveal>,
    disclosure_dirty: Cell<bool>,
    last_navigation: Option<SidebarSelectionNavigation>,
    pending_navigation: RefCell<Option<PendingNavigation>>,
    root: ComponentId,
    body: StackContent,
    labels: SidebarLabels,
    ids: BTreeMap<Key, ComponentId>,
    next: u64,
    end: u64,
    revision: u64,
    displayed: Option<SidebarInput>,
    search: Option<SidebarSearchInput>,
    multi_search_next: u64,
    multi_search_help: Option<(
        tekes_agent_presentation::SidebarMultiSearchInput,
        BTreeMap<SessionAddress, String>,
    )>,
    search_owner: Option<String>,
    search_labels: SidebarSearchLabels,
    search_retry: Option<(SidebarSearchKey, ComponentId)>,
    search_pending: bool,
    search_seen: BTreeMap<(String, String), (u64, u64)>,
    observer_next: u64,
    observer_labels: SidebarObserverLabels,
    bindings: BTreeMap<ComponentId, SidebarIntent>,
    pending: BTreeSet<(SessionAddress, String)>,
    session_menu: bool,
}
impl Sidebar {
    pub fn new(namespace: u32, labels: SidebarLabels) -> Self {
        let base = (namespace as u64) << 32;
        let list = Content::new(
            ComponentId(base + 2),
            ListContent::new(
                &labels.name,
                vec![Content::new(
                    ComponentId(base + 5),
                    TextContent::new(&labels.restoring),
                )],
            ),
        );
        let body = StackContent {
            model: Stack {
                axis: Axis::Vertical,
                gap: Spacing::Regular.value(),
            },
            flexible_child: Some(ComponentId(base + 1)),
            children: vec![
                Content::new(
                    ComponentId(base + 1),
                    ScrollContent::new(ScrollArea::default(), list),
                ),
                Content::new(
                    ComponentId(base + 3),
                    ListContent::new(&labels.actions, Vec::new()),
                ),
            ],
        };
        let search_labels = SidebarSearchLabels::default();
        let body = StackContent {
            model: Stack {
                axis: Axis::Vertical,
                gap: 0.,
            },
            flexible_child: Some(ComponentId(base + 7)),
            children: vec![
                Content::new(ComponentId(base + 7), body),
                Content::new(
                    ComponentId(base + 8),
                    SearchFooter::new(search_labels.name.clone()),
                ),
                Content::new(
                    ComponentId(base + 9),
                    crate::SidebarObserverStatus::from_id_range(
                        ComponentId(base + 9),
                        base + 0xFE00_0000,
                        base + 0xFF00_0000,
                        SidebarObserverLabels::default(),
                    )
                    .expect("observer ID range is valid"),
                ),
            ],
        };
        Self {
            page_owner: std::sync::Arc::new(()),
            visibility_honored: Cell::new(None),
            page_limit: crate::SIDEBAR_PAGE_SIZE,
            page_facts: Default::default(),
            page_request: None,
            preferences: SidebarPreferencesSnapshot::default(),
            preferences_bound: false,
            preferences_generation: 1,
            preferences_writable: false,
            title_comparator: None,
            pinned_expansion: std::rc::Rc::new(std::cell::RefCell::new(Disclosure {
                open: true,
                disabled: false,
            })),
            folder_expansions: BTreeMap::new(),
            preference_bindings: BTreeMap::new(),
            disclosure_targets: BTreeMap::new(),
            navigation_reveal: RefCell::new(NavigationReveal::default()),
            disclosure_dirty: Cell::new(false),
            last_navigation: None,
            pending_navigation: RefCell::new(None),
            root: ComponentId(base),
            body,
            labels,
            ids: BTreeMap::new(),
            next: base + 16,
            // Ordinary controls end before the observer and search status ranges.
            end: base | 0xFDFF_FFFF,
            revision: 0,
            displayed: None,
            search: None,
            multi_search_next: base + 0xFF00_0000,
            multi_search_help: None,
            search_owner: None,
            search_labels,
            search_retry: None,
            search_pending: false,
            search_seen: BTreeMap::new(),
            observer_next: base + 0xFE00_0000,
            observer_labels: SidebarObserverLabels::default(),
            bindings: BTreeMap::new(),
            pending: BTreeSet::new(),
            session_menu: true,
        }
    }
    pub fn preferences(&self) -> &SidebarPreferencesSnapshot {
        &self.preferences
    }
    pub fn preferences_writable(&self) -> bool {
        self.preferences_bound && self.preferences_writable
    }
    /// Explicitly select the default original compact SDK menu.
    /// Configure before mounting or publishing input; IDs remain in this namespace.
    pub fn with_session_menu(mut self) -> Self {
        assert!(
            self.displayed.is_none(),
            "configure Sidebar menu before update"
        );
        self.session_menu = true;
        self
    }
    /// Compatibility/diagnostic presentation for existing button-based hosts.
    /// This is not the ClientKit-aligned menu layout used by the native demo.
    pub fn with_action_buttons(mut self) -> Self {
        assert!(self.displayed.is_none(), "configure Sidebar before update");
        self.session_menu = false;
        self
    }
    fn refresh_session_menu(&mut self) -> Result<(), &'static str> {
        if !self.session_menu {
            return Ok(());
        }
        let root = ComponentId(self.root.0 + 12);
        let mut menu = self.body.children[0]
            .component_mut::<StackContent>()
            .unwrap()
            .children[1]
            .component_mut::<ListContent>()
            .unwrap()
            .rows
            .children
            .pop()
            .unwrap_or_else(|| {
                Content::new(
                    root,
                    crate::SidebarSessionMenu::at(
                        root,
                        crate::SidebarSessionMenuLabels {
                            name: self.labels.actions.clone(),
                            ..Default::default()
                        },
                    ),
                )
            });
        let result = menu
            .component_mut::<crate::SidebarSessionMenu>()
            .unwrap()
            .update_from_sidebar(self);
        self.body.children[0]
            .component_mut::<StackContent>()
            .unwrap()
            .children[1]
            .component_mut::<ListContent>()
            .unwrap()
            .rows
            .children = vec![menu];
        result
    }
    pub fn session_menu_id(&self) -> Option<ComponentId> {
        self.session_menu.then_some(ComponentId(self.root.0 + 12))
    }
    pub fn take_menu_intents(
        &self,
        records: &[ContentActionRecord],
    ) -> Vec<crate::SidebarSessionMenuIntent> {
        self.body.children[0]
            .component::<StackContent>()
            .unwrap()
            .children[1]
            .component::<ListContent>()
            .unwrap()
            .rows
            .children
            .first()
            .and_then(|content| content.component::<crate::SidebarSessionMenu>())
            .map(|menu| menu.take_intents(records))
            .unwrap_or_default()
    }
    fn pinned_open(&self) -> bool {
        self.preferences.preferences.pinned_expanded || self.navigation_reveal.borrow().pinned
    }
    fn folder_open(&self, key: &SidebarFolderKey) -> bool {
        !self.preferences.preferences.collapsed_folders.contains(key)
            || self.navigation_reveal.borrow().folders.contains(key)
    }
    pub fn pending_navigation(&self) -> Option<SidebarSelectionNavigation> {
        self.pending_navigation
            .borrow()
            .as_ref()
            .map(|pending| pending.event.clone())
    }
    /// True means a new event was consumed; false means waiting for its row or
    /// an idempotent duplicate. No preference write, routing or scroll occurs.
    pub fn reconcile_navigation(
        &mut self,
        event: &SidebarSelectionNavigation,
    ) -> Result<bool, &'static str> {
        if event.revision == 0
            || event
                .selected
                .as_ref()
                .is_some_and(|id| id.endpoint_id.is_empty() || id.session_id.is_empty())
        {
            return Err("invalid Sidebar navigation");
        }
        if let Some(last) = &self.last_navigation {
            if event.revision < last.revision {
                return Err("stale Sidebar navigation");
            }
            if event.revision == last.revision {
                return if event == last {
                    Ok(false)
                } else {
                    Err("conflicting Sidebar navigation")
                };
            }
        }
        if self
            .displayed
            .as_ref()
            .is_some_and(|input| input.selected != event.selected)
        {
            return Err("Sidebar navigation does not match current selection");
        }
        let old = (
            self.last_navigation.clone(),
            self.pending_navigation.borrow().clone(),
        );
        self.last_navigation = Some(event.clone());
        *self.pending_navigation.borrow_mut() = Some(PendingNavigation::new(event.clone()));
        if let Some(input) = self.displayed.clone()
            && let Err(error) = self.update(&input)
        {
            self.last_navigation = old.0;
            *self.pending_navigation.borrow_mut() = old.1;
            return Err(error);
        }
        Ok(self.pending_navigation.borrow().is_none())
    }
    pub fn set_title_comparator(&mut self, compare: SidebarTitleComparator) {
        self.title_comparator = Some(compare);
    }
    pub fn title_sort_supported(&self) -> bool {
        self.title_comparator.is_some()
    }
    /// Allows retained hosts to avoid even borrowing their tree mutably for an
    /// identical accepted input. A locally toggled disclosure still needs the
    /// next authoritative publication (including rollback after write failure).
    pub fn preferences_current(&self, value: &SidebarPreferencesSnapshot, writable: bool) -> bool {
        self.preferences_bound
            && !self.disclosure_dirty.get()
            && value == &self.preferences
            && writable == self.preferences_writable
            && self.pinned_expansion.borrow().open == self.pinned_open()
            && self
                .folder_expansions
                .iter()
                .all(|(key, expansion)| expansion.borrow().open == self.folder_open(key))
    }
    pub fn update_preferences(
        &mut self,
        value: &SidebarPreferencesSnapshot,
        writable: bool,
    ) -> Result<(), &'static str> {
        if self.preferences_current(value, writable) {
            return Ok(());
        }
        value
            .preferences
            .validate()
            .map_err(|_| "invalid Sidebar preferences")?;
        if value.revision == 0
            || (self.preferences_bound
                && (value.revision < self.preferences.revision
                    || (value.revision == self.preferences.revision && value != &self.preferences)))
        {
            return Err("stale or conflicting Sidebar preferences");
        }
        if value.preferences.sort != SidebarSort::RecentActivity && self.title_comparator.is_none()
        {
            return Err("Sidebar title sort requires a host localized comparator");
        }
        let dirty = self.disclosure_dirty.get();
        let generation = if dirty
            || !self.preferences_bound
            || value != &self.preferences
            || writable != self.preferences_writable
        {
            self.preferences_generation
                .checked_add(1)
                .ok_or("Sidebar preference generation exhausted")?
        } else {
            self.preferences_generation
        };
        let old = (
            self.preferences.clone(),
            self.preferences_bound,
            self.preferences_writable,
            self.preferences_generation,
        );
        let old_limit = self.page_limit;
        if value.preferences.sort != self.preferences.preferences.sort {
            self.page_limit = crate::SIDEBAR_PAGE_SIZE;
        }
        let old_reveal = self.navigation_reveal.borrow().clone();
        let mut reveal = old_reveal.clone();
        reveal.folders.retain(|key| {
            self.preferences.preferences.collapsed_folders.contains(key)
                == value.preferences.collapsed_folders.contains(key)
        });
        if self.preferences.preferences.pinned_expanded != value.preferences.pinned_expanded {
            reveal.pinned = false;
        }
        *self.navigation_reveal.borrow_mut() = reveal;
        self.preferences_generation = generation;
        self.preferences = value.clone();
        self.preferences_bound = true;
        self.preferences_writable = writable;
        self.disclosure_dirty.set(false);
        if let Some(input) = self.displayed.clone()
            && let Err(error) = self.update(&input)
        {
            (
                self.preferences,
                self.preferences_bound,
                self.preferences_writable,
                self.preferences_generation,
            ) = old;
            self.page_limit = old_limit;
            *self.navigation_reveal.borrow_mut() = old_reveal;
            self.disclosure_dirty.set(dirty);
            return Err(error);
        }
        Ok(())
    }
    pub fn pinned_header_id(&self) -> Option<ComponentId> {
        self.ids
            .get(&Key::PinnedHeader(self.preferences_generation))
            .copied()
    }
    pub fn folder_header_id(&self, key: &SidebarFolderKey) -> Option<ComponentId> {
        self.ids
            .get(&Key::FolderHeader(key.clone(), self.preferences_generation))
            .copied()
    }
    pub fn folder_body_id(&self, key: &SidebarFolderKey) -> Option<ComponentId> {
        self.ids.get(&Key::FolderBody(key.clone())).copied()
    }
    pub fn take_preference_intents(
        &self,
        actions: &[ContentAction],
    ) -> Vec<SidebarPreferenceChange> {
        actions
            .iter()
            .filter(|a| a.kind == ContentActionKind::Activate)
            .filter_map(|a| {
                let change = self.preference_bindings.get(&a.target)?.clone();
                let mut reveal = self.navigation_reveal.borrow_mut();
                let target = self.disclosure_targets.get(&a.target)?;
                if let Some(pending) = self.pending_navigation.borrow_mut().as_mut() {
                    pending.suppress(target);
                }
                match target {
                    DisclosureTarget::Folder(key) => {
                        reveal.folders.remove(key);
                    }
                    DisclosureTarget::Pinned => reveal.pinned = false,
                }
                self.disclosure_dirty.set(true);
                Some(change)
            })
            .collect()
    }
    /// The same original pin-order algorithm as ClientKit. Unknown stored IDs
    /// remain durable; a row must currently be pinned before offering its move.
    pub fn move_pin_change(
        &self,
        address: &SessionAddress,
        offset: isize,
    ) -> Option<SidebarPreferenceChange> {
        if !self.preferences_writable || !matches!(offset, -1 | 1) {
            return None;
        }
        let input = self.displayed.as_ref()?;
        if !input.loaded
            || !input
                .rows
                .iter()
                .any(|row| &row.address == address && row.pin.as_ref().is_some_and(|p| p.pinned))
        {
            return None;
        }
        let mut value = self.preferences.preferences.clone();
        let rows = ordered_sidebar_rows(&input.rows, &value, self.title_comparator).ok()?;
        for row in rows
            .into_iter()
            .filter(|r| r.pin.as_ref().is_some_and(|p| p.pinned))
        {
            if !value.pin_order.contains(&row.address) {
                value.pin_order.push(row.address.clone());
            }
        }
        let index = value.pin_order.iter().position(|id| id == address)?;
        let other = index
            .checked_add_signed(offset)
            .filter(|i| *i < value.pin_order.len())?;
        value.pin_order.swap(index, other);
        Some(SidebarPreferenceChange {
            expected: self.preferences.clone(),
            value,
        })
    }
    pub fn id(&self) -> ComponentId {
        self.root
    }
    /// The complete accepted host publication, including restoration/failure state.
    pub fn displayed_input(&self) -> Option<&SidebarInput> {
        self.displayed.as_ref()
    }
    pub fn inventory_region_id(&self) -> ComponentId {
        ComponentId(self.root.0 + 7)
    }
    pub fn inventory_viewport_id(&self) -> ComponentId {
        ComponentId(self.root.0 + 1)
    }
    pub fn search_footer_id(&self) -> ComponentId {
        ComponentId(self.root.0 + 8)
    }
    pub fn observer_footer_id(&self) -> ComponentId {
        ComponentId(self.root.0 + 9)
    }
    fn observer_status(&self) -> &crate::SidebarObserverStatus {
        self.body.children[2]
            .component::<crate::SidebarObserverStatus>()
            .expect("Sidebar observer footer")
    }
    fn observer_status_mut(&mut self) -> &mut crate::SidebarObserverStatus {
        self.body.children[2]
            .component_mut::<crate::SidebarObserverStatus>()
            .expect("Sidebar observer footer")
    }
    /// Install a coherent observer packet as a new source lifetime. The
    /// component's dynamic action high-water mark survives source replacement.
    pub fn bind_observer_source(
        &mut self,
        source: &tekes_agent_presentation::SidebarObserverStatusInput,
        labels: SidebarObserverLabels,
    ) -> Result<(), &'static str> {
        if self
            .observer_status()
            .displayed()
            .is_some_and(|old| old.source == source.source)
        {
            if self.observer_labels != labels {
                return Err("Sidebar observer labels changed within the same source");
            }
            return self.update_observer_source(source);
        }
        let next = self.observer_status().next_id().max(self.observer_next);
        let mut status = crate::SidebarObserverStatus::from_id_range(
            self.observer_footer_id(),
            next,
            self.root.0 + 0xFF00_0000,
            labels.clone(),
        )?;
        status.replace_source(source)?;
        self.observer_next = status.next_id();
        self.observer_labels = labels;
        self.body.children[2] = Content::new(status.id(), status);
        if let Some(input) = self.displayed.clone() {
            self.update(&input)?;
        }
        Ok(())
    }
    /// Update the current observer source without changing its identity.
    pub fn update_observer_source(
        &mut self,
        source: &tekes_agent_presentation::SidebarObserverStatusInput,
    ) -> Result<(), &'static str> {
        self.observer_status_mut().update_from(source)?;
        self.observer_next = self.observer_status().next_id();
        Ok(())
    }
    /// Retire the observer source and all of its raw actions immediately.
    /// The next source starts beyond every ID allocated by the retired source.
    pub fn clear_observer_source(&mut self) -> Result<(), &'static str> {
        let next = self.observer_status().next_id().max(self.observer_next);
        let status = crate::SidebarObserverStatus::from_id_range(
            self.observer_footer_id(),
            next,
            self.root.0 + 0xFF00_0000,
            self.observer_labels.clone(),
        )?;
        self.observer_next = next;
        self.body.children[2] = Content::new(status.id(), status);
        if let Some(input) = self.displayed.clone() {
            self.update(&input)?;
        }
        Ok(())
    }
    pub fn take_observer_intents(
        &mut self,
        actions: &[ContentAction],
    ) -> Vec<tekes_agent_presentation::SidebarObserverIntent> {
        self.observer_status_mut().take_intents(actions)
    }
    pub fn observer_retry_id(&self, endpoint: &str) -> Option<ComponentId> {
        self.observer_status().retry_id(endpoint)
    }
    pub fn displayed_search(&self) -> Option<&SidebarSearchInput> {
        self.search.as_ref()
    }
    fn multi_search_status(&self) -> Option<&crate::SidebarMultiSearchStatus> {
        self.body.children[1].component::<crate::SidebarMultiSearchStatus>()
    }
    fn multi_search_active(&self) -> bool {
        self.multi_search_status()
            .and_then(|status| status.displayed())
            .is_some_and(|source| source.input.key.is_some())
    }
    pub fn displayed_multi_search(
        &self,
    ) -> Option<&tekes_agent_presentation::SidebarMultiSearchPublication> {
        self.multi_search_status()?.displayed()
    }
    pub fn replace_multi_search_source(
        &mut self,
        owner: &str,
        source: &tekes_agent_presentation::SidebarMultiSearchPublication,
    ) -> Result<(), &'static str> {
        if let Some(status) =
            self.body.children[1].component_mut::<crate::SidebarMultiSearchStatus>()
        {
            status.replace_source(owner, source)?;
        } else {
            let mut status = crate::SidebarMultiSearchStatus::from_id_range(
                ComponentId(self.root.0 + 8),
                self.multi_search_next,
                self.root.0 | u32::MAX as u64,
                self.search_labels.clone(),
            )?;
            status.replace_source(owner, source)?;
            self.body.children[1] = Content::new(status.id(), status);
        }
        self.multi_search_next = self.multi_search_status().unwrap().next_id();
        self.search = None;
        self.search_owner = None;
        self.search_retry = None;
        self.search_pending = false;
        self.multi_search_help = None;
        if let Some(input) = self.displayed.clone() {
            self.update(&input)?;
        }
        Ok(())
    }
    pub fn update_multi_search_from(
        &mut self,
        owner: &str,
        source: &tekes_agent_presentation::SidebarMultiSearchPublication,
    ) -> Result<(), &'static str> {
        self.body.children[1]
            .component_mut::<crate::SidebarMultiSearchStatus>()
            .ok_or("multi-search source is not installed")?
            .update_from(owner, source)?;
        self.multi_search_next = self.multi_search_status().unwrap().next_id();
        if self
            .multi_search_help
            .as_ref()
            .is_some_and(|(old, _)| old != &source.input)
        {
            self.multi_search_help = None;
        }
        if let Some(input) = self.displayed.clone() {
            self.update(&input)?;
        }
        Ok(())
    }
    pub fn set_multi_search_help(
        &mut self,
        source: &tekes_agent_presentation::SidebarMultiSearchInput,
        snippets: &BTreeMap<SessionAddress, String>,
    ) -> Result<(), &'static str> {
        let current = self
            .displayed_multi_search()
            .ok_or("multi-search source is not installed")?;
        if &current.input != source || (source.key.is_none() && !snippets.is_empty()) {
            return Err("stale multi-search row help");
        }
        for (address, text) in snippets {
            if !current.children.iter().any(|child| {
                child.endpoint_id == address.endpoint_id
                    && child
                        .publication
                        .hits
                        .iter()
                        .any(|hit| &hit.address == address && &hit.hit.snippet == text)
            }) {
                return Err("row help is outside the original multi-search result");
            }
        }
        self.multi_search_help = Some((source.clone(), snippets.clone()));
        Ok(())
    }
    pub fn take_multi_search_intents(
        &mut self,
        actions: &[ContentAction],
    ) -> Vec<tekes_agent_presentation::SidebarMultiSearchIntent> {
        self.body.children[1]
            .component_mut::<crate::SidebarMultiSearchStatus>()
            .map_or_else(Vec::new, |status| status.take_intents(actions))
    }
    /// Accepted current-query metadata. The SDK owns visibility, dwell, clipping
    /// and semantic help on the original row; metadata alone is not UI support.
    pub fn search_snippet(&self, address: &SessionAddress) -> Option<&str> {
        let input = self.displayed.as_ref()?;
        if !input.loaded {
            return None;
        }
        if let Some(current) = self.displayed_multi_search() {
            let (source, snippets) = self.multi_search_help.as_ref()?;
            if source != &current.input
                || source.key.is_none()
                || !input.rows.iter().any(|row| &row.address == address)
            {
                return None;
            }
            return snippets.get(address).map(String::as_str);
        }
        let help = input.search_help.as_ref()?;
        if !help.results_ready || self.search.as_ref() != Some(&help.source) {
            return None;
        }
        help.snippets.get(address).map(String::as_str)
    }
    pub fn row_help(&self, address: &SessionAddress) -> Option<&str> {
        let input = self.displayed.as_ref()?;
        if !input.loaded {
            return None;
        }
        self.search_snippet(address).or_else(|| {
            input
                .rows
                .iter()
                .find(|row| &row.address == address)?
                .folder_path
                .as_deref()
        })
    }
    pub fn search_retry_id(&self) -> Option<ComponentId> {
        self.search_retry.as_ref().map(|(_, id)| *id)
    }
    /// Product copy is a host input; all paint and sizing remain library defaults.
    pub fn set_search_labels(&mut self, labels: SidebarSearchLabels) -> Result<(), &'static str> {
        self.search_labels = labels;
        if let Some(status) =
            self.body.children[1].component_mut::<crate::SidebarMultiSearchStatus>()
        {
            status.set_labels(self.search_labels.clone())?;
        }
        if let Some(input) = self.search.clone() {
            self.apply_search(&input)?;
        }
        Ok(())
    }
    fn search_footer_mut(&mut self) -> &mut SearchFooter {
        if self.body.children[1].component::<SearchFooter>().is_none() {
            if let Some(status) = self.multi_search_status() {
                self.multi_search_next = status.next_id();
            }
            self.body.children[1] = Content::new(
                ComponentId(self.root.0 + 8),
                SearchFooter::new(self.search_labels.name.clone()),
            );
            self.multi_search_help = None;
        }
        self.body.children[1]
            .component_mut::<SearchFooter>()
            .unwrap()
    }
    /// Search is independent from inventory restoration. Every displayed result
    /// belongs to its exact query attempt; no old raw Retry gesture is retargeted.
    pub fn update_search(&mut self, input: &SidebarSearchInput) -> Result<(), &'static str> {
        if self.search_owner.is_some() || self.multi_search_status().is_some() {
            return Err("bound sidebar search requires original source");
        }
        self.apply_search(input)
    }
    /// Explicit host navigation/mount barrier. A new controller may start at
    /// revision one; all prior view action IDs remain permanently retired.
    pub fn replace_search_source(
        &mut self,
        owner: &str,
        input: &SidebarSearchInput,
    ) -> Result<(), &'static str> {
        Self::validate_search_owner(owner, input)?;
        let old = (
            self.search.take(),
            self.search_retry.take(),
            self.search_pending,
            self.search_seen.clone(),
        );
        self.search_pending = false;
        self.search_seen.retain(|(source, _), _| source != owner);
        if let Err(error) = self.apply_search(input) {
            (
                self.search,
                self.search_retry,
                self.search_pending,
                self.search_seen,
            ) = old;
            return Err(error);
        }
        self.search_owner = Some(owner.into());
        Ok(())
    }
    pub fn update_search_from(
        &mut self,
        owner: &str,
        input: &SidebarSearchInput,
    ) -> Result<(), &'static str> {
        Self::validate_search_owner(owner, input)?;
        if self.search_owner.as_deref() != Some(owner) {
            return Err("sidebar search source replaced");
        }
        self.apply_search(input)
    }
    fn validate_search_owner(owner: &str, input: &SidebarSearchInput) -> Result<(), &'static str> {
        if owner.is_empty() || input.key.as_ref().is_some_and(|key| key.owner != owner) {
            return Err("sidebar search owner mismatch");
        }
        input.validate()
    }
    fn apply_search(&mut self, input: &SidebarSearchInput) -> Result<(), &'static str> {
        input.validate()?;
        if let Some(old) = &self.search {
            if input.revision < old.revision {
                return Err("stale sidebar search input");
            }
            if input.revision == old.revision && input != old {
                return Err("conflicting sidebar search publication");
            }
        }
        if let Some(key) = &input.key
            && let Some((epoch, generation)) = self
                .search_seen
                .get(&(key.owner.clone(), key.endpoint_id.clone()))
        {
            let current = self.search.as_ref().and_then(|input| input.key.as_ref());
            if key.connection_epoch < *epoch
                || key.generation < *generation
                || (key.generation == *generation && current != Some(key))
            {
                return Err("retired sidebar search attempt");
            }
        }
        let retry = input.retry_key().cloned();
        let retained = self
            .search_retry
            .as_ref()
            .filter(|(key, _)| Some(key) == retry.as_ref())
            .cloned();
        let next = if retry.is_some() && retained.is_none() {
            self.multi_search_next
                .checked_add(1)
                .filter(|next| *next <= (self.root.0 | u32::MAX as u64))
                .ok_or("sidebar search ID range exhausted")?
        } else {
            self.multi_search_next
        };
        let target =
            retry.map(|key| retained.unwrap_or((key, ComponentId(self.multi_search_next))));
        if target != self.search_retry {
            self.search_pending = false;
        }
        let pending = self.search_pending;
        let text = input.notice(&self.search_labels);
        let labels = self.search_labels.clone();
        let text_id = ComponentId(self.root.0 + 14);
        let footer = self.search_footer_mut();
        let old = std::mem::take(&mut footer.body.children);
        let mut old: BTreeMap<_, _> = old.into_iter().map(|node| (node.id(), node)).collect();
        if let Some(text) = &text {
            let mut node = old
                .remove(&text_id)
                .unwrap_or_else(|| Content::new(text_id, TextContent::new("")));
            node.component_mut::<TextContent>().unwrap().text = text.clone();
            footer.body.children.push(node);
        }
        if let Some((_, id)) = &target {
            let mut node = old
                .remove(id)
                .unwrap_or_else(|| Content::new(*id, ButtonContent::new("")));
            let button = node.component_mut::<ButtonContent>().unwrap();
            button.label = labels.retry;
            button.button.state.disabled = pending;
            footer.body.children.push(node);
        }
        footer.name = labels.name;
        footer.visible = text.is_some();
        self.multi_search_next = next;
        self.search_retry = target;
        if let Some(key) = &input.key {
            self.search_seen.insert(
                (key.owner.clone(), key.endpoint_id.clone()),
                (key.connection_epoch, key.generation),
            );
        }
        self.search = Some(input.clone());
        if let Some(inventory) = self.displayed.clone() {
            self.update(&inventory)?;
        }
        Ok(())
    }
    /// Local admission can only restrict the current source's retry capability.
    pub fn set_search_retry_pending(&mut self, key: &SidebarSearchKey, pending: bool) -> bool {
        let Some((source, id)) = &self.search_retry else {
            return false;
        };
        if source != key {
            return false;
        }
        let id = *id;
        self.search_pending = pending;
        if let Some(node) = self
            .search_footer_mut()
            .body
            .children
            .iter_mut()
            .find(|node| node.id() == id)
        {
            node.component_mut::<ButtonContent>()
                .unwrap()
                .button
                .state
                .disabled = pending;
        }
        true
    }
    /// Route this separate read-only callback to the original search source.
    /// The receiver must recheck the captured source/epoch/generation before IO.
    pub fn take_search_intents(&self, actions: &[ContentAction]) -> Vec<SidebarSearchIntent> {
        let Some((key, id)) = &self.search_retry else {
            return vec![];
        };
        if self.search_pending {
            return vec![];
        }
        actions
            .iter()
            .filter(|action| action.kind == ContentActionKind::Activate && action.target == *id)
            .map(|_| SidebarSearchIntent::Retry { key: key.clone() })
            .collect()
    }
    /// Local gesture admission only; it can restrict, never enable, a shared action.
    pub fn set_pending(&mut self, address: &SessionAddress, action: &str, pending: bool) {
        let key = (address.clone(), action.to_owned());
        if pending {
            self.pending.insert(key);
        } else {
            self.pending.remove(&key);
        }
        if self.session_menu {
            self.refresh_session_menu()
                .expect("previously validated session menu");
            return;
        }
        for (id, intent) in &self.bindings {
            if let SidebarIntent::Action {
                address: target,
                definition,
            } = intent
                && target == address
                && definition.id == action
                && let Some(content) = self.body.children[0]
                    .component_mut::<StackContent>()
                    .unwrap()
                    .children[1]
                    .component_mut::<ListContent>()
                    .unwrap()
                    .rows
                    .children
                    .iter_mut()
                    .find(|content| content.id() == *id)
            {
                content
                    .component_mut::<ButtonContent>()
                    .unwrap()
                    .button
                    .state
                    .disabled = pending || !definition.enabled;
            }
        }
    }
    pub fn is_pending(&self, address: &SessionAddress, action: &str) -> bool {
        self.pending.contains(&(address.clone(), action.into()))
    }
    /// Retire local action admission when the host replaces its inventory owner.
    /// This does not cancel remote operations; their completions must still be
    /// fenced by the host's original owner token.
    pub fn clear_pending_actions(&mut self) {
        for (address, action) in std::mem::take(&mut self.pending) {
            self.set_pending(&address, &action, false);
        }
    }
    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let mut host = ContentHost::new(Content::new(self.root, self))?;
        host.set_action_events(true);
        Ok(host)
    }
    pub fn row_id(&self, address: &SessionAddress) -> Option<ComponentId> {
        self.ids.get(&Key::Select(address.clone())).copied()
    }
    pub fn action_id(&self, address: &SessionAddress, action: &str) -> Option<ComponentId> {
        if self.session_menu {
            return None;
        }
        self.ids
            .get(&Key::Action(address.clone(), action.into()))
            .copied()
    }
    fn rows_mut(&mut self) -> &mut ListContent {
        self.body.children[0]
            .component_mut::<StackContent>()
            .unwrap()
            .children[0]
            .component_mut::<ScrollContent>()
            .expect("sidebar viewport")
            .children_mut()
            .expect("viewport content")[0]
            .component_mut::<ListContent>()
            .expect("sidebar inventory")
    }
    /// Validate all identity/capacity constraints before touching the mounted tree.
    pub fn update(&mut self, input: &SidebarInput) -> Result<(), &'static str> {
        if self.session_menu
            && (self.labels.actions.trim().is_empty()
                || input
                    .rows
                    .iter()
                    .flat_map(|row| &row.actions.items)
                    .any(|action| action.label.trim().is_empty()))
        {
            return Err("Sidebar session menu labels must not be empty");
        }
        if input.revision == 0 || input.revision < self.revision {
            return Err("stale or unversioned sidebar input");
        }
        if self.displayed.as_ref().is_some_and(|last| {
            last.revision == input.revision
                && (last.loaded != input.loaded
                    || last.error != input.error
                    || last.rows != input.rows
                    || last.search_help != input.search_help
                    || last.folder_filter != input.folder_filter)
        }) {
            return Err("conflicting sidebar publication");
        }
        let (reveal, consume_navigation) = plan_reveal(
            input,
            self.pending_navigation.borrow().as_ref(),
            &self.navigation_reveal.borrow(),
        );
        let generation =
            if self.disclosure_dirty.get() || reveal != *self.navigation_reveal.borrow() {
                self.preferences_generation
                    .checked_add(1)
                    .ok_or("Sidebar disclosure generation exhausted")?
            } else {
                self.preferences_generation
            };
        // ClientKit starts a new ordinary page when the folder filter changes.
        // Compute locally so a rejected publication cannot consume the old page.
        let page_limit = if self
            .displayed
            .as_ref()
            .is_some_and(|last| last.folder_filter != input.folder_filter)
        {
            crate::SIDEBAR_PAGE_SIZE
        } else {
            self.page_limit
        };
        let (outline, page_facts) = crate::sidebar_paged_outline(
            &input.rows,
            &self.preferences.preferences,
            self.title_comparator,
            input.folder_filter.as_ref(),
            input.selected.as_ref(),
            page_limit,
            self.search.as_ref().is_some_and(|s| s.key.is_some())
                || self.multi_search_active()
                || input
                    .search_help
                    .as_ref()
                    .is_some_and(|h| h.source.key.is_some()),
        )?;
        let more_key = Key::More(self.next);
        let show_more = input.loaded && page_facts.has_more;
        let visible_rows: Vec<_> = if input.loaded {
            outline
                .pinned
                .iter()
                .copied()
                .chain(outline.sessions.iter().copied())
                .chain(
                    outline
                        .folders
                        .iter()
                        .flat_map(|group| group.rows.iter().copied()),
                )
                .collect()
        } else {
            vec![]
        };
        let visible_addresses: BTreeSet<_> = visible_rows.iter().map(|row| &row.address).collect();
        let has_pins = visible_rows
            .iter()
            .any(|row| row.pin.as_ref().is_some_and(|pin| pin.pinned));
        let mut addresses = BTreeSet::new();
        let mut keys = BTreeSet::new();
        if show_more {
            keys.insert(more_key.clone());
        }
        for row in &input.rows {
            if row.address.endpoint_id.is_empty()
                || row.address.session_id.is_empty()
                || !addresses.insert(row.address.clone())
            {
                return Err("invalid or duplicate sidebar identity");
            }
            if row.actions.endpoint != row.address.endpoint_id
                || row.actions.session_id != row.address.session_id
            {
                return Err("sidebar actions belong to another session");
            }
            if visible_addresses.contains(&row.address) {
                keys.extend([
                    Key::Row(row.address.clone()),
                    Key::Select(row.address.clone()),
                    Key::Status(row.address.clone()),
                ]);
            }
            let mut action_ids = BTreeSet::new();
            for action in &row.actions.items {
                if action.id.is_empty() || !action_ids.insert(&action.id) {
                    return Err("invalid or duplicate shared action identity");
                }
                if visible_addresses.contains(&row.address)
                    && input.selected.as_ref() == Some(&row.address)
                {
                    keys.insert(Key::Action(row.address.clone(), action.id.clone()));
                }
            }
        }
        if let Some(help) = &input.search_help {
            help.source.validate()?;
            let key = help
                .source
                .key
                .as_ref()
                .ok_or("Sidebar help has no query source")?;
            if key.query.trim().is_empty()
                || help.source.phase == crate::SidebarSearchPhase::Idle
                || (!help.results_ready && !help.snippets.is_empty())
                || (help.results_ready
                    && !matches!(
                        help.source.phase,
                        crate::SidebarSearchPhase::Complete { .. }
                    ))
                || help.snippets.keys().any(|address| {
                    address.endpoint_id != key.endpoint_id
                        || !input.rows.iter().any(|row| &row.address == address)
                })
            {
                return Err("Sidebar help does not belong to its displayed rows");
            }
        }
        if has_pins {
            keys.insert(Key::PinnedHeader(generation));
            keys.insert(Key::PinnedLabel(generation));
        }
        if input.loaded {
            for group in &outline.folders {
                keys.extend([
                    Key::FolderRoot(group.key.clone()),
                    Key::FolderHeader(group.key.clone(), generation),
                    Key::FolderLabel(group.key.clone(), generation),
                    Key::FolderBody(group.key.clone()),
                ]);
            }
        }
        let needed = keys
            .iter()
            .filter(|key| !self.ids.contains_key(*key))
            .count() as u64;
        let next = self
            .next
            .checked_add(needed)
            .filter(|next| *next <= self.end)
            .ok_or("sidebar ID namespace exhausted")?;
        let mut assigned = self.next;
        let mut ids = self.ids.clone();
        ids.retain(|key, _| keys.contains(key));
        for key in keys {
            ids.entry(key).or_insert_with(|| {
                let id = ComponentId(assigned);
                assigned += 1;
                id
            });
        }

        *self.navigation_reveal.borrow_mut() = reveal;
        if consume_navigation {
            *self.pending_navigation.borrow_mut() = None;
        }
        let old = std::mem::take(&mut self.rows_mut().rows.children);
        let mut pinned_section = None;
        let mut folder_sections = BTreeMap::new();
        let mut retained = Vec::new();
        for mut node in old {
            if let Some(section) = node.component_mut::<SummarySectionView>() {
                retained.append(section.rows_mut());
                if section.section == "pinned" {
                    pinned_section = Some(node);
                } else {
                    folder_sections.insert(node.id(), node);
                }
            } else {
                retained.push(node);
            }
        }
        let mut old: BTreeMap<_, _> = retained.into_iter().map(|row| (row.id(), row)).collect();
        let mut children = Vec::new();
        let mut bindings = BTreeMap::new();
        let mut pinned_children = Vec::new();
        let mut folder_children: BTreeMap<SidebarFolderKey, Vec<Content>> = BTreeMap::new();
        let mut preference_bindings = BTreeMap::new();
        let mut disclosure_targets = BTreeMap::new();
        *self.pinned_expansion.borrow_mut() = Disclosure {
            open: self.pinned_open(),
            disabled: !self.preferences_writable,
        };
        for row in &visible_rows {
            let row_id = ids[&Key::Row(row.address.clone())];
            let select_id = ids[&Key::Select(row.address.clone())];
            let status_id = ids[&Key::Status(row.address.clone())];
            let mut content = old.remove(&row_id).unwrap_or_else(|| {
                Content::new(
                    row_id,
                    StackContent {
                        model: Stack {
                            axis: Axis::Vertical,
                            gap: Spacing::Tight.value(),
                        },
                        flexible_child: None,
                        children: vec![
                            Content::new(select_id, ButtonContent::new("")),
                            Content::new(status_id, TextContent::new("")),
                        ],
                    },
                )
            });
            let stack = content
                .component_mut::<StackContent>()
                .expect("sidebar row");
            let button = stack.children[0]
                .component_mut::<ButtonContent>()
                .expect("row selection");
            button.button.state.selected = input.selected.as_ref() == Some(&row.address);
            button.label = row.title.clone();
            button.variant = ButtonVariant::Quiet;
            // CANDIDATE_SIDEBAR_ROW_HELP_DECLARATION
            let mut status = (self.labels.activity)(&row.activity);
            if row.pin.as_ref().is_some_and(|pin| pin.pinned) {
                status = format!("{} · {status}", self.labels.pinned);
            }
            stack.children[1]
                .component_mut::<TextContent>()
                .expect("row activity")
                .text = status;
            let pinned = row.pin.as_ref().is_some_and(|pin| pin.pinned);
            let folder = row.folder_key();
            let active = if pinned {
                self.pinned_open()
            } else {
                !self.preferences.preferences.group_by_folder || self.folder_open(&folder)
            };
            if active {
                bindings.insert(
                    select_id,
                    SidebarIntent::Select {
                        address: row.address.clone(),
                    },
                );
            }
            if pinned {
                pinned_children.push(content);
            } else if self.preferences.preferences.group_by_folder {
                folder_children.entry(folder).or_default().push(content);
            } else {
                children.push(content);
            }
        }
        let folder_keys: BTreeSet<_> = if input.loaded {
            outline
                .folders
                .iter()
                .map(|group| group.key.clone())
                .collect()
        } else {
            BTreeSet::new()
        };
        self.folder_expansions
            .retain(|key, _| folder_keys.contains(key));
        if input.loaded {
            for group in &outline.folders {
                let key = &group.key;
                let expansion = self
                    .folder_expansions
                    .entry(key.clone())
                    .or_insert_with(|| {
                        std::rc::Rc::new(std::cell::RefCell::new(Disclosure::default()))
                    })
                    .clone();
                *expansion.borrow_mut() = Disclosure {
                    open: self.folder_open(key),
                    disabled: !self.preferences_writable,
                };
                let root = ids[&Key::FolderRoot(key.clone())];
                let header = ids[&Key::FolderHeader(key.clone(), generation)];
                let label = ids[&Key::FolderLabel(key.clone(), generation)];
                let body_id = ids[&Key::FolderBody(key.clone())];
                let mut section = folder_sections
                    .remove(&root)
                    .filter(|node| {
                        node.component::<SummarySectionView>().unwrap().children[0].id() == header
                    })
                    .unwrap_or_else(|| {
                        Content::new(
                            root,
                            SummarySectionView::new(
                                format!("folder:{key:?}"),
                                header,
                                label,
                                body_id,
                                expansion,
                                SectionHeaderKind::Sidebar,
                            ),
                        )
                    });
                let body = section.component_mut::<SummarySectionView>().unwrap();
                body.body_indent = Spacing::Regular.value();
                body.header_mut()
                    .set_title(sidebar_folder_title(group.path, &self.labels.no_folder));
                body.header_mut().set_enabled(self.preferences_writable);
                *body.rows_mut() = folder_children.remove(key).unwrap_or_default();
                if self.preferences_writable {
                    let mut value = self.preferences.preferences.clone();
                    value.collapsed_folders.retain(|folder| folder != key);
                    if self.folder_open(key) {
                        value.collapsed_folders.push(key.clone());
                        value.collapsed_folders.sort();
                    }
                    disclosure_targets.insert(header, DisclosureTarget::Folder(key.clone()));
                    preference_bindings.insert(
                        header,
                        SidebarPreferenceChange {
                            expected: self.preferences.clone(),
                            value,
                        },
                    );
                }
                children.push(section);
            }
        }
        if has_pins {
            let header = ids[&Key::PinnedHeader(generation)];
            let label = ids[&Key::PinnedLabel(generation)];
            let mut section = pinned_section
                .filter(|node| {
                    node.component::<SummarySectionView>().unwrap().children[0].id() == header
                })
                .unwrap_or_else(|| {
                    Content::new(
                        ComponentId(self.root.0 + 10),
                        SummarySectionView::new(
                            "pinned".into(),
                            header,
                            label,
                            ComponentId(self.root.0 + 11),
                            self.pinned_expansion.clone(),
                            SectionHeaderKind::Sidebar,
                        ),
                    )
                });
            let body = section.component_mut::<SummarySectionView>().unwrap();
            body.header_mut().set_title(self.labels.pinned.clone());
            body.header_mut().set_enabled(self.preferences_writable);
            *body.rows_mut() = pinned_children;
            if self.preferences_writable {
                let mut value = self.preferences.preferences.clone();
                value.pinned_expanded = !self.pinned_open();
                disclosure_targets.insert(header, DisclosureTarget::Pinned);
                preference_bindings.insert(
                    header,
                    SidebarPreferenceChange {
                        expected: self.preferences.clone(),
                        value,
                    },
                );
            }
            children.insert(0, section);
        }
        let page_request = if show_more {
            let control = ids[&more_key];
            children.push(Content::new(control, ButtonContent::new(&self.labels.more)));
            Some(crate::SidebarPageRequest {
                owner: self.page_owner.clone(),
                control,
                limit: page_limit,
            })
        } else {
            None
        };
        if children.is_empty() {
            children.push(Content::new(
                ComponentId(self.root.0 + 5),
                TextContent::new(
                    if self
                        .search
                        .as_ref()
                        .is_some_and(|search| search.key.is_some())
                    {
                        if input.loaded
                            && input
                                .search_help
                                .as_ref()
                                .is_none_or(|help| help.results_ready)
                            && self.search.as_ref().is_some_and(|search| {
                                matches!(
                                    search.phase,
                                    crate::SidebarSearchPhase::Complete { has_more: false }
                                )
                            })
                        {
                            &self.search_labels.no_results
                        } else {
                            &self.search_labels.pending_results
                        }
                    } else if self.multi_search_active() {
                        if input.loaded
                            && self.displayed_multi_search().is_some_and(|source| {
                                source.children.iter().all(|child| {
                                    child.publication.unresolved.is_empty()
                                        && matches!(
                                            child.publication.input.phase,
                                            crate::SidebarSearchPhase::Complete { has_more: false }
                                        )
                                })
                            })
                        {
                            &self.search_labels.no_results
                        } else {
                            &self.search_labels.pending_results
                        }
                    } else if input.loaded {
                        &self.labels.empty
                    } else {
                        &self.labels.restoring
                    },
                ),
            ));
        }
        // Unknown hits awaiting a public inventory join remain diagnostic,
        // including explicit display-only inputs with independently supplied rows.
        if has_pins
            && input
                .search_help
                .as_ref()
                .is_some_and(|help| !help.results_ready)
            && self
                .search
                .as_ref()
                .is_some_and(|search| !matches!(search.phase, crate::SidebarSearchPhase::Searching))
        {
            children.push(Content::new(
                ComponentId(self.root.0 + 5),
                TextContent::new(&self.search_labels.pending_results),
            ));
        }
        // Independent failure notice: an observation error never turns retained
        // inventory into an empty list or confirms an unrestored list as empty.
        if let Some(error) = &input.error {
            children.push(Content::new(
                ComponentId(self.root.0 + 6),
                TextContent::new(error),
            ));
        }
        self.rows_mut().rows.children = children;
        if !self.session_menu {
            let actions = self.body.children[0]
                .component_mut::<StackContent>()
                .unwrap()
                .children[1]
                .component_mut::<ListContent>()
                .expect("shared action panel");
            let pending = self.pending.clone();
            let old = std::mem::take(&mut actions.rows.children);
            let mut old: BTreeMap<_, _> = old
                .into_iter()
                .map(|content| (content.id(), content))
                .collect();
            if let Some(row) = visible_rows
                .iter()
                .find(|row| input.selected.as_ref() == Some(&row.address))
            {
                for definition in &row.actions.items {
                    let id = ids[&Key::Action(row.address.clone(), definition.id.clone())];
                    let mut content = old
                        .remove(&id)
                        .unwrap_or_else(|| Content::new(id, ButtonContent::new("")));
                    let button = content
                        .component_mut::<ButtonContent>()
                        .expect("shared action");
                    button.label = match &definition.disabled_reason {
                        Some(reason) if !definition.enabled => {
                            format!("{} · {reason}", definition.label)
                        }
                        _ => definition.label.clone(),
                    };
                    button.button.state.disabled = !definition.enabled
                        || pending.contains(&(row.address.clone(), definition.id.clone()));
                    if definition.enabled {
                        bindings.insert(
                            id,
                            SidebarIntent::Action {
                                address: row.address.clone(),
                                definition: definition.clone(),
                            },
                        );
                    }
                    actions.rows.children.push(content);
                }
            }
        }
        self.page_limit = page_limit;
        self.page_facts = page_facts;
        self.page_request = page_request;
        self.ids = ids;
        self.next = next;
        self.bindings = bindings;
        self.preference_bindings = preference_bindings;
        self.disclosure_targets = disclosure_targets;
        self.preferences_generation = generation;
        self.disclosure_dirty.set(false);
        self.revision = input.revision;
        self.displayed = Some(input.clone());
        self.refresh_session_menu()?;
        Ok(())
    }
    /// Only explicit navigation can produce a scroll target. Inventory refresh
    /// alone never steals the reader's position; collapsed/filtered rows wait.
    pub fn visibility_target(&self) -> Option<crate::SidebarVisibilityTarget> {
        let event = self.last_navigation.as_ref()?;
        let input = self.displayed.as_ref()?;
        if self.disclosure_dirty.get()
            || self.visibility_honored.get() == Some(event.revision)
            || self.pending_navigation.borrow().is_some()
            || input.selected != event.selected
        {
            return None;
        }
        let address = event.selected.as_ref()?;
        let item = self.row_id(address)?;
        if !self.bindings.contains_key(&item) {
            return None;
        }
        Some(crate::SidebarVisibilityTarget {
            owner: self.page_owner.clone(),
            root: self.root,
            viewport: ComponentId(self.root.0 + 1),
            item,
            address: address.clone(),
            navigation: event.revision,
            inventory: input.revision,
            disclosure: self.preferences_generation,
            claimed: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }
    pub fn claim_visibility(
        &self,
        target: &crate::SidebarVisibilityTarget,
    ) -> Result<(), &'static str> {
        let current = self
            .visibility_target()
            .ok_or("Sidebar visibility no longer pending")?;
        if !std::sync::Arc::ptr_eq(&target.owner, &self.page_owner)
            || target.item != current.item
            || target.address != current.address
            || target.navigation != current.navigation
            || target.inventory != current.inventory
            || target.disclosure != current.disclosure
        {
            return Err("stale or foreign Sidebar visibility target");
        }
        if !target.once() {
            return Err("Sidebar visibility already claimed");
        }
        self.visibility_honored.set(Some(target.navigation));
        Ok(())
    }
    pub fn paging(&self) -> &crate::SidebarPagingFacts {
        &self.page_facts
    }
    pub fn page_button_id(&self) -> Option<ComponentId> {
        self.page_request.as_ref().map(|r| r.control)
    }
    pub fn take_page_requests(&self, actions: &[ContentAction]) -> Vec<crate::SidebarPageRequest> {
        self.page_request
            .as_ref()
            .into_iter()
            .filter(|r| {
                actions
                    .iter()
                    .any(|a| a.target == r.control && a.kind == ContentActionKind::Activate)
            })
            .cloned()
            .collect()
    }
    /// Restart the ordinary page for an explicit repeated filter/sort choice.
    /// Refreshes and unrelated preference updates keep the current page count.
    pub fn reset_paging(&mut self) -> Result<bool, &'static str> {
        if self.page_limit == crate::SIDEBAR_PAGE_SIZE {
            return Ok(false);
        }
        let old = std::mem::replace(&mut self.page_limit, crate::SIDEBAR_PAGE_SIZE);
        if let Some(input) = self.displayed.clone()
            && let Err(error) = self.update(&input)
        {
            self.page_limit = old;
            return Err(error);
        }
        Ok(true)
    }
    /// Local presentation only; the inventory observer/RPC policy is unchanged.
    /// Foreign, stale, repeated and remounted control captures are rejected.
    pub fn load_more(&mut self, request: &crate::SidebarPageRequest) -> Result<bool, &'static str> {
        if !std::sync::Arc::ptr_eq(&request.owner, &self.page_owner)
            || self
                .page_request
                .as_ref()
                .is_none_or(|r| r.control != request.control || r.limit != request.limit)
        {
            return Ok(false);
        }
        let next = self
            .page_limit
            .checked_add(crate::SIDEBAR_PAGE_SIZE)
            .ok_or("Sidebar page limit exhausted")?;
        let old = std::mem::replace(&mut self.page_limit, next);
        if let Some(input) = self.displayed.clone()
            && let Err(error) = self.update(&input)
        {
            self.page_limit = old;
            return Err(error);
        }
        Ok(true)
    }
    /// Copy/rename are host gestures; execute goes to the shared dispatcher with stable RPC identity.
    /// No preference or inventory facts change until the next authoritative input.
    pub fn take_intents(&self, actions: &[ContentAction]) -> Vec<SidebarIntent> {
        actions
            .iter()
            .filter(|action| action.kind == ContentActionKind::Activate)
            .filter_map(|action| self.bindings.get(&action.target).cloned())
            .filter(|intent| match intent {
                SidebarIntent::Action {
                    address,
                    definition,
                } => !self
                    .pending
                    .contains(&(address.clone(), definition.id.clone())),
                _ => true,
            })
            .collect()
    }
}
impl Component for Sidebar {
    fn children(&self) -> &[Content] {
        &self.body.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.body.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        let measured = self.body.measure(text, available);
        MeasuredChild::new(Size::new(
            measured.size.width.min(available.width).max(0.),
            measured.size.height.min(available.height).max(0.),
        ))
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        // Keep both footers in a stable bottom stack. The observer footer is
        // last and is anchored to the pane bottom. Oversized footers are
        // clipped by the pane; this does not prove all actions are reachable
        // at arbitrary tiny window heights.
        let search_height = self.body.children[1].measure(text, frame.size).size.height;
        let observer_height = self.body.children[2].measure(text, frame.size).size.height;
        let footer_height = search_height + observer_height;
        let body = Rect::new(
            frame.min_x(),
            frame.min_y(),
            frame.size.width,
            (frame.size.height - footer_height).max(0.),
        );
        let search = Rect::new(
            frame.min_x(),
            frame.max_y() - footer_height,
            frame.size.width,
            search_height,
        );
        let observer = Rect::new(
            frame.min_x(),
            frame.max_y() - observer_height,
            frame.size.width,
            observer_height,
        );
        let pane_clip = frame.intersection(&clip).unwrap_or_default();
        let body_clip = body.intersection(&pane_clip).unwrap_or_default();
        self.body.children[0].arrange(text, body, body_clip);
        self.body.children[1].arrange(text, search, pane_clip);
        self.body.children[2].arrange(text, observer, pane_clip);
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.labels.name)
    }
}
