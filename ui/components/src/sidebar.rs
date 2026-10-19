//! Agent session inventory and shared actions. No local pin database or action policy.
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
    pub actions: String,
    pub pinned: String,
    pub activity: fn(&ThreadActivity) -> String,
}
impl Default for SidebarLabels {
    fn default() -> Self {
        Self {
            name: "会话".into(),
            empty: "暂无会话".into(),
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
    bindings: BTreeMap<ComponentId, SidebarIntent>,
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
                    TextContent::new(&labels.empty),
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
        Self {
            root: ComponentId(base),
            body,
            labels,
            ids: BTreeMap::new(),
            next: base + 16,
            end: base | u32::MAX as u64,
            revision: 0,
            bindings: BTreeMap::new(),
        }
    }
    pub fn id(&self) -> ComponentId {
        self.root
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
            keys.extend([
                Key::Row(row.address.clone()),
                Key::Select(row.address.clone()),
                Key::Status(row.address.clone()),
            ]);
            let mut action_ids = BTreeSet::new();
            for action in &row.actions.items {
                if action.id.is_empty() || !action_ids.insert(&action.id) {
                    return Err("invalid or duplicate shared action identity");
                }
                if input.selected.as_ref() == Some(&row.address) {
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
        for row in &input.rows {
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
                TextContent::new(&self.labels.empty),
            ));
        }
        self.rows_mut().rows.children = children;
        let actions = self.body.children[1]
            .component_mut::<ListContent>()
            .expect("shared action panel");
        let old = std::mem::take(&mut actions.rows.children);
        let mut old: BTreeMap<_, _> = old
            .into_iter()
            .map(|content| (content.id(), content))
            .collect();
        if let Some(row) = input
            .rows
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
                button.button.state.disabled = !definition.enabled;
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
        Ok(())
    }
    /// Copy/rename are host gestures; execute goes to the shared dispatcher with stable RPC identity.
    /// No preference or inventory facts change until the next authoritative input.
    pub fn take_intents(&self, actions: &[ContentAction]) -> Vec<SidebarIntent> {
        actions
            .iter()
            .filter(|action| action.kind == ContentActionKind::Activate)
            .filter_map(|action| self.bindings.get(&action.target).cloned())
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
        self.body.measure(text, available)
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        self.body.arrange(text, frame, clip);
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.labels.name)
    }
}
