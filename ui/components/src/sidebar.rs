//! Agent session inventory and shared actions. No local pin database or action policy.
use crate::{
    SidebarSearchInput, SidebarSearchIntent, SidebarSearchKey, SidebarSearchLabels,
    sidebar_search::{SearchFooter, SearchInputView},
};
use std::collections::{BTreeMap, BTreeSet};
use tekes_agent_contract::{
    SessionAddress, ThreadActionDefinition, ThreadActionsPresentation, ThreadActivity,
    ThreadPinPresentation,
};
use tekes_ui_controls::{
    ButtonTextRenderer, ScrollArea,
    composition::*,
    extended::{Axis, Stack},
    layout::MeasuredChild,
    paint::ButtonVariant,
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::Spacing;

#[derive(Clone, Debug, PartialEq)]
pub struct SidebarRow {
    pub address: SessionAddress,
    pub title: String,
    pub activity: ThreadActivity,
    pub pin: Option<ThreadPinPresentation>,
    pub actions: ThreadActionsPresentation,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SidebarInput {
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
    Row(SessionAddress),
    Select(SessionAddress),
    Status(SessionAddress),
    Action(SessionAddress, String),
}

/// Mountable sidebar. All dynamic IDs live in the supplied unique upper-32-bit namespace.
/// Deliver and drain host actions before update(), just as for other public content controls.
#[derive(Debug)]
pub struct Sidebar {
    root: ComponentId,
    body: StackContent,
    labels: SidebarLabels,
    ids: BTreeMap<Key, ComponentId>,
    next: u64,
    end: u64,
    revision: u64,
    displayed: Option<SidebarInput>,
    search: Option<SidebarSearchInput>,
    search_owner: Option<String>,
    search_labels: SidebarSearchLabels,
    search_retry: Option<(SidebarSearchKey, ComponentId)>,
    search_pending: bool,
    search_seen: BTreeMap<(String, String), (u64, u64)>,
    bindings: BTreeMap<ComponentId, SidebarIntent>,
    pending: BTreeSet<(SessionAddress, String)>,
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
            ],
        };
        Self {
            root: ComponentId(base),
            body,
            labels,
            ids: BTreeMap::new(),
            next: base + 16,
            end: base | u32::MAX as u64,
            revision: 0,
            displayed: None,
            search: None,
            search_owner: None,
            search_labels,
            search_retry: None,
            search_pending: false,
            search_seen: BTreeMap::new(),
            bindings: BTreeMap::new(),
            pending: BTreeSet::new(),
        }
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
    pub fn displayed_search(&self) -> Option<&SidebarSearchInput> {
        self.search.as_ref()
    }
    pub fn search_retry_id(&self) -> Option<ComponentId> {
        self.search_retry.as_ref().map(|(_, id)| *id)
    }
    /// Product copy is a host input; all paint and sizing remain library defaults.
    pub fn set_search_labels(&mut self, labels: SidebarSearchLabels) -> Result<(), &'static str> {
        self.search_labels = labels;
        if let Some(input) = self.search.clone() {
            self.apply_search(&input)?;
        }
        Ok(())
    }
    fn search_footer_mut(&mut self) -> &mut SearchFooter {
        self.body.children[1]
            .component_mut::<SearchFooter>()
            .unwrap()
    }
    /// Search is independent from inventory restoration. Every displayed result
    /// belongs to its exact query attempt; no old raw Retry gesture is retargeted.
    pub fn update_search(&mut self, input: &SidebarSearchInput) -> Result<(), &'static str> {
        if self.search_owner.is_some() {
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
            self.next
                .checked_add(1)
                .filter(|next| *next <= self.end)
                .ok_or("sidebar ID namespace exhausted")?
        } else {
            self.next
        };
        let target = retry.map(|key| retained.unwrap_or((key, ComponentId(self.next))));
        if target != self.search_retry {
            self.search_pending = false;
        }
        let pending = self.search_pending;
        let text = input.notice(&self.search_labels);
        let labels = self.search_labels.clone();
        let text_id = ComponentId(self.root.0 + 9);
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
        self.next = next;
        self.search_retry = target;
        if let Some(key) = &input.key {
            self.search_seen.insert(
                (key.owner.clone(), key.endpoint_id.clone()),
                (key.connection_epoch, key.generation),
            );
        }
        self.search = Some(input.clone());
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
    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let mut host = ContentHost::new(Content::new(self.root, self))?;
        host.set_action_events(true);
        Ok(host)
    }
    pub fn row_id(&self, address: &SessionAddress) -> Option<ComponentId> {
        self.ids.get(&Key::Select(address.clone())).copied()
    }
    pub fn action_id(&self, address: &SessionAddress, action: &str) -> Option<ComponentId> {
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
        if input.revision == 0 || input.revision < self.revision {
            return Err("stale or unversioned sidebar input");
        }
        if self.displayed.as_ref().is_some_and(|last| {
            last.revision == input.revision
                && (last.loaded != input.loaded
                    || last.error != input.error
                    || last.rows != input.rows)
        }) {
            return Err("conflicting sidebar publication");
        }
        let visible_rows = if input.loaded {
            input.rows.as_slice()
        } else {
            &[]
        };
        let mut addresses = BTreeSet::new();
        let mut keys = BTreeSet::new();
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
            if input.loaded {
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
                if input.loaded && input.selected.as_ref() == Some(&row.address) {
                    keys.insert(Key::Action(row.address.clone(), action.id.clone()));
                }
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

        let old = std::mem::take(&mut self.rows_mut().rows.children);
        let mut old: BTreeMap<_, _> = old.into_iter().map(|row| (row.id(), row)).collect();
        let mut children = Vec::new();
        let mut bindings = BTreeMap::new();
        for row in visible_rows {
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
            let mut status = (self.labels.activity)(&row.activity);
            if row.pin.as_ref().is_some_and(|pin| pin.pinned) {
                status = format!("{} · {status}", self.labels.pinned);
            }
            stack.children[1]
                .component_mut::<TextContent>()
                .expect("row activity")
                .text = status;
            bindings.insert(
                select_id,
                SidebarIntent::Select {
                    address: row.address.clone(),
                },
            );
            children.push(content);
        }
        if children.is_empty() {
            children.push(Content::new(
                ComponentId(self.root.0 + 5),
                TextContent::new(if input.loaded {
                    &self.labels.empty
                } else {
                    &self.labels.restoring
                }),
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
        self.ids = ids;
        self.next = next;
        self.bindings = bindings;
        self.revision = input.revision;
        self.displayed = Some(input.clone());
        Ok(())
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
        // ClientKit anchors the search footer to the bottom even when its
        // preferred height exceeds the pane. The list then has zero height;
        // the footer's Retry stays reachable while its top is ancestor-clipped.
        let footer = self.body.children[1].measure(text, frame.size).size.height;
        let body = Rect::new(
            frame.min_x(),
            frame.min_y(),
            frame.size.width,
            (frame.size.height - footer).max(0.),
        );
        let footer = Rect::new(
            frame.min_x(),
            frame.max_y() - footer,
            frame.size.width,
            footer,
        );
        let pane_clip = frame.intersection(&clip).unwrap_or_default();
        let body_clip = body.intersection(&pane_clip).unwrap_or_default();
        self.body.children[0].arrange(text, body, body_clip);
        self.body.children[1].arrange(text, footer, pane_clip);
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.labels.name)
    }
}
