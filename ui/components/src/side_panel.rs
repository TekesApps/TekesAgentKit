//! Scoped Agent tabs and retained content. Task/child/SideChat execution stays in presentation.
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use tekes_agent_presentation::{
    AgentTask, ChildHistoryLayout, RunScope, SideChat, SideChatPhase, SideChatSnapshot,
    TaskPanelState,
};
use tekes_ui_controls::{
    ButtonTextRenderer,
    composition::*,
    extended::{Axis, Stack},
    layout::MeasuredChild,
    paint::ButtonVariant,
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::Spacing;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SidePanelTabKind {
    Summary,
    Task { task_id: String },
    Subagent { child_id: String },
    SideChat { session_id: String },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidePanelTab {
    pub kind: SidePanelTabKind,
    pub title: String,
    /// Explicit adapter policy; never open bounded Native history as an independent session.
    pub child_history_layout: Option<ChildHistoryLayout>,
    /// Actual receiving parent, which may be nested under the panel's thread.
    pub child_parent: Option<RunScope>,
    /// Resolved task pointer is content data, never part of the task tab's identity.
    pub task_scope: Option<RunScope>,
    /// Optional existing load state, including loading/missing/error. Never a second task store.
    pub task_state: Option<Box<TaskPanelState>>,
}
impl SidePanelTab {
    pub fn summary(title: impl Into<String>) -> Self {
        Self {
            kind: SidePanelTabKind::Summary,
            title: title.into(),
            child_history_layout: None,
            child_parent: None,
            task_scope: None,
            task_state: None,
        }
    }
    pub fn task(task: &AgentTask) -> Self {
        Self {
            kind: SidePanelTabKind::Task {
                task_id: task.id.clone(),
            },
            title: task.name.clone(),
            child_history_layout: None,
            child_parent: None,
            task_scope: Some(task.scope.clone()),
            task_state: None,
        }
    }
    pub fn task_from_state(
        state: &TaskPanelState,
        pending_title: impl Into<String>,
    ) -> Result<Self, &'static str> {
        let task_id = state
            .active_task_id()
            .ok_or("task load has no active identity")?;
        Ok(Self {
            kind: SidePanelTabKind::Task {
                task_id: task_id.into(),
            },
            title: state
                .value
                .as_ref()
                .map(|task| task.name.clone())
                .unwrap_or_else(|| pending_title.into()),
            child_history_layout: None,
            child_parent: None,
            task_scope: state.value.as_ref().map(|task| task.scope.clone()),
            task_state: Some(Box::new(state.clone())),
        })
    }
    pub fn subagent(
        child_id: impl Into<String>,
        title: impl Into<String>,
        layout: ChildHistoryLayout,
    ) -> Self {
        Self {
            kind: SidePanelTabKind::Subagent {
                child_id: child_id.into(),
            },
            title: title.into(),
            child_history_layout: Some(layout),
            child_parent: None,
            task_scope: None,
            task_state: None,
        }
    }
    pub fn side_chat(chat: &SideChat) -> Self {
        Self {
            kind: SidePanelTabKind::SideChat {
                session_id: chat.scope.session_id.clone(),
            },
            title: chat.title.clone(),
            child_history_layout: None,
            child_parent: None,
            task_scope: None,
            task_state: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidePanelInput {
    /// One monotonically versioned window/panel publication, including tab navigation.
    pub revision: u64,
    pub parent: RunScope,
    /// The navigation target is known, but its canonical parent projection may
    /// still be pending. Waiting hides the body and grants no content token.
    pub projection_ready: bool,
    pub tabs: Vec<SidePanelTab>,
    pub selected: Option<SidePanelTabKind>,
    /// Complete snapshot from the same SideChatController across retained parents,
    /// including uncertain/pending operations. Absence retires scratch UI caches.
    pub side_chats: SideChatSnapshot,
    pub supports_side_chat: Option<bool>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidePanelIntent {
    Select {
        parent: RunScope,
        tab: SidePanelTab,
    },
    Close {
        parent: RunScope,
        tab: SidePanelTabKind,
    },
    CreateSideChat {
        parent: RunScope,
    },
}
#[derive(Clone, Debug)]
pub struct SidePanelLabels {
    pub name: String,
    pub empty: String,
    pub awaiting_content: String,
    pub close: String,
    pub create_side_chat: String,
    pub creating: String,
    pub closing: String,
    pub unknown_close: String,
    pub unsupported_side_chat: String,
    pub loading_task: String,
    pub missing_task: String,
}
impl Default for SidePanelLabels {
    fn default() -> Self {
        Self {
            name: "Agent 侧面板".into(),
            empty: "未打开标签".into(),
            awaiting_content: "正在准备面板内容".into(),
            close: "关闭当前标签".into(),
            create_side_chat: "新建 SideChat".into(),
            creating: "正在创建 SideChat".into(),
            closing: "正在关闭 SideChat".into(),
            unknown_close: "SideChat 关闭结果未知，请先对账".into(),
            unsupported_side_chat: "当前端点不支持临时 SideChat".into(),
            loading_task: "正在解析任务".into(),
            missing_task: "任务 Run 暂不可用".into(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    parent: RunScope,
    tab: SidePanelTabKind,
}
#[derive(Clone, Debug)]
struct TabMount {
    id: ComponentId,
    epoch: u64,
    layout: Option<ChildHistoryLayout>,
    task_scope: Option<RunScope>,
    child_parent: Option<RunScope>,
}
impl TabMount {
    fn same_source(&self, tab: &SidePanelTab) -> bool {
        self.layout == tab.child_history_layout
            && self.task_scope == tab.task_scope
            && self.child_parent == tab.child_parent
    }
}

/// UI mount lifetime only. Async payloads must first pass existing TaskPanelState/child fences.
#[derive(Clone, Debug)]
pub struct PanelContentToken {
    key: Key,
    context: u64,
    mount: u64,
}

/// A composable pane with domain tabs and a content slot. It does not create another Run store.
/// Hosts mount Kit content under the returned token, drain actions before updates, and clear
/// focus/finish composition before changing a mounted pane. IDs use a unique u32 namespace.
/// Same-thread tab changes retain instances; changing endpoint/thread evicts UI.
#[derive(Debug)]
pub struct SidePanel {
    root: ComponentId,
    body: StackContent,
    labels: SidePanelLabels,
    input: Option<SidePanelInput>,
    mounts: BTreeMap<Key, TabMount>,
    retained: BTreeMap<Key, Content>,
    mounted: Option<Key>,
    mounted_custom: bool,
    bindings: BTreeMap<ComponentId, SidePanelIntent>,
    next: u64,
    epoch: u64,
    context: u64,
    parent_order: VecDeque<RunScope>,
    retained_parent_limit: usize,
}
impl SidePanel {
    /// `retained_parent_limit` bounds parent-scope caches within the current
    /// endpoint/thread only. Crossing that boundary always retires the old UI.
    pub fn new(namespace: u32, labels: SidePanelLabels, retained_parent_limit: usize) -> Self {
        let base = (namespace as u64) << 32;
        let mut close = ButtonContent::new(&labels.close);
        close.button.state.disabled = true;
        let mut create = ButtonContent::new(&labels.create_side_chat);
        create.button.state.disabled = true;
        Self {
            root: ComponentId(base),
            body: StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Regular.value(),
                },
                flexible_child: Some(ComponentId(base + 5)),
                children: vec![
                    Content::new(
                        ComponentId(base + 1),
                        StackContent {
                            model: Stack {
                                axis: Axis::Horizontal,
                                gap: Spacing::Tight.value(),
                            },
                            flexible_child: None,
                            children: Vec::new(),
                        },
                    ),
                    Content::new(
                        ComponentId(base + 2),
                        StackContent {
                            model: Stack {
                                axis: Axis::Horizontal,
                                gap: Spacing::Tight.value(),
                            },
                            flexible_child: None,
                            children: vec![
                                Content::new(ComponentId(base + 3), close),
                                Content::new(ComponentId(base + 4), create),
                            ],
                        },
                    ),
                    Content::new(
                        ComponentId(base + 5),
                        StackContent {
                            model: Stack {
                                axis: Axis::Vertical,
                                gap: Spacing::Regular.value(),
                            },
                            flexible_child: None,
                            children: vec![Content::new(
                                ComponentId(base + 6),
                                TextContent::new(&labels.empty),
                            )],
                        },
                    ),
                ],
            },
            labels,
            input: None,
            mounts: BTreeMap::new(),
            retained: BTreeMap::new(),
            mounted: None,
            mounted_custom: false,
            bindings: BTreeMap::new(),
            next: base + 16,
            epoch: 0,
            context: 0,
            parent_order: VecDeque::new(),
            retained_parent_limit: retained_parent_limit.max(1),
        }
    }
    pub fn id(&self) -> ComponentId {
        self.root
    }
    pub fn displayed_input(&self) -> Option<&SidePanelInput> {
        self.input.as_ref()
    }
    pub fn content_slot_id(&self) -> ComponentId {
        ComponentId(self.root.0 + 5)
    }
    pub fn close_id(&self) -> ComponentId {
        ComponentId(self.root.0 + 3)
    }
    pub fn create_side_chat_id(&self) -> ComponentId {
        ComponentId(self.root.0 + 4)
    }
    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let mut host = ContentHost::new(Content::new(self.root, self))?;
        host.set_action_events(true);
        Ok(host)
    }
    pub fn tab_id(&self, tab: &SidePanelTabKind) -> Option<ComponentId> {
        let parent = &self.input.as_ref()?.parent;
        self.mounts
            .get(&Key {
                parent: parent.clone(),
                tab: tab.clone(),
            })
            .map(|mount| mount.id)
    }
    fn slot(&mut self) -> &mut StackContent {
        self.body.children[2]
            .component_mut::<StackContent>()
            .expect("panel content slot")
    }
    fn stash(&mut self) {
        let content = self.slot().children.pop();
        if self.mounted_custom
            && let (Some(key), Some(content)) = (self.mounted.take(), content)
        {
            self.retained.insert(key, content);
        }
        self.mounted = None;
        self.mounted_custom = false;
    }
    pub fn update(&mut self, input: &SidePanelInput) -> Result<(), &'static str> {
        if input.revision == 0
            || self.input.as_ref().is_some_and(|old| {
                input.revision < old.revision || (input.revision == old.revision && input != old)
            })
        {
            return Err("stale or unversioned side-panel input");
        }
        input
            .parent
            .validate()
            .map_err(|_| "invalid panel parent")?;
        let mut keys = BTreeSet::new();
        for tab in &input.tabs {
            let valid = match &tab.kind {
                SidePanelTabKind::Summary => {
                    tab.child_history_layout.is_none()
                        && tab.child_parent.is_none()
                        && tab.task_scope.is_none()
                        && tab.task_state.is_none()
                }
                SidePanelTabKind::Task { task_id } => {
                    let scope_valid = tab.task_scope.as_ref().is_none_or(|scope| {
                        scope.validate().is_ok()
                            && scope.endpoint_id == input.parent.endpoint_id
                            && scope.thread_id == input.parent.thread_id
                    });
                    let state_valid = match &tab.task_state {
                        Some(state) => {
                            state.active_task_id() == Some(task_id.as_str())
                                && state.parent_scope() == Some(&input.parent)
                                && state.value.as_ref().map(|task| &task.scope)
                                    == tab.task_scope.as_ref()
                        }
                        None => tab.task_scope.is_some(),
                    };
                    !task_id.is_empty()
                        && scope_valid
                        && state_valid
                        && tab.child_history_layout.is_none()
                        && tab.child_parent.is_none()
                }
                SidePanelTabKind::Subagent { child_id } => {
                    !child_id.is_empty()
                        && child_id != &input.parent.session_id
                        && tab.child_history_layout.is_some()
                        && tab.child_parent.as_ref().is_none_or(|parent| {
                            parent.validate().is_ok()
                                && parent.endpoint_id == input.parent.endpoint_id
                                && parent.thread_id == input.parent.thread_id
                                && parent.session_id != *child_id
                        })
                        && tab.task_scope.is_none()
                        && tab.task_state.is_none()
                }
                SidePanelTabKind::SideChat { session_id } => {
                    input.side_chats.chats.get(session_id).is_some_and(|chat| {
                        chat.parent == input.parent
                            && chat.scope.endpoint_id == input.parent.endpoint_id
                            && chat.scope.session_id == *session_id
                            && chat.scope.validate().is_ok()
                            && chat.scope.session_id != input.parent.session_id
                            && chat.scope.thread_id == chat.scope.session_id
                    }) && tab.child_history_layout.is_none()
                        && tab.child_parent.is_none()
                        && tab.task_scope.is_none()
                        && tab.task_state.is_none()
                }
            };
            if !valid
                || !keys.insert(Key {
                    parent: input.parent.clone(),
                    tab: tab.kind.clone(),
                })
            {
                return Err("invalid, foreign or duplicate Agent tab");
            }
        }
        if input.selected.as_ref().is_some_and(|tab| {
            !keys.contains(&Key {
                parent: input.parent.clone(),
                tab: tab.clone(),
            })
        }) {
            return Err("selected Agent tab is not open");
        }
        if let Some(previous) = &self.input
            && previous.parent == input.parent
        {
            for tab in &previous.tabs {
                if let SidePanelTabKind::SideChat { session_id } = &tab.kind
                    && !keys.contains(&Key {
                        parent: input.parent.clone(),
                        tab: tab.kind.clone(),
                    })
                    && input
                        .side_chats
                        .chats
                        .get(session_id)
                        .is_some_and(|chat| chat.parent == input.parent)
                {
                    return Err("retain SideChat until shared controller confirms discard");
                }
            }
        }
        let changed_parent = self
            .input
            .as_ref()
            .is_none_or(|old| old.parent != input.parent);
        let changed_thread = self.input.as_ref().is_some_and(|old| {
            old.parent.endpoint_id != input.parent.endpoint_id
                || old.parent.thread_id != input.parent.thread_id
        });
        let changed_projection = self
            .input
            .as_ref()
            .is_some_and(|old| old.projection_ready != input.projection_ready);
        let needed = input
            .tabs
            .iter()
            .filter(|tab| {
                changed_thread
                    || !self.mounts.contains_key(&Key {
                        parent: input.parent.clone(),
                        tab: tab.kind.clone(),
                    })
            })
            .count() as u64;
        let source_changes = input
            .tabs
            .iter()
            .filter(|tab| {
                changed_thread
                    || self
                        .mounts
                        .get(&Key {
                            parent: input.parent.clone(),
                            tab: tab.kind.clone(),
                        })
                        .is_none_or(|old| !old.same_source(tab))
            })
            .count() as u64;
        let next = self
            .next
            .checked_add(needed)
            .filter(|next| *next <= (self.root.0 | u32::MAX as u64))
            .ok_or("panel ID namespace exhausted")?;
        let epoch = self
            .epoch
            .checked_add(source_changes)
            .ok_or("panel mount epoch exhausted")?;
        let context = self
            .context
            .checked_add(u64::from(changed_parent || changed_projection))
            .ok_or("panel context exhausted")?;
        // Keep the active subtree mounted across ordinary facts updates; otherwise retain its exact instance.
        let next_key = input
            .selected
            .as_ref()
            .filter(|_| input.projection_ready)
            .map(|tab| Key {
                parent: input.parent.clone(),
                tab: tab.clone(),
            });
        let same_content = self.mounted == next_key
            && next_key.as_ref().is_some_and(|key| {
                self.mounts.get(key).is_some_and(|old| {
                    input
                        .tabs
                        .iter()
                        .find(|tab| tab.kind == key.tab)
                        .is_some_and(|tab| old.same_source(tab))
                })
            });
        if !same_content {
            self.stash();
        }
        if changed_thread {
            self.mounts.clear();
            self.retained.clear();
            self.parent_order.clear();
        } else if !input.projection_ready {
            self.retained.clear();
        }
        let live = |key: &Key| match &key.tab {
            SidePanelTabKind::SideChat { session_id } => input
                .side_chats
                .chats
                .get(session_id)
                .is_some_and(|chat| chat.parent == key.parent),
            _ => true,
        };
        self.mounts
            .retain(|key, _| live(key) && (key.parent != input.parent || keys.contains(key)));
        self.retained
            .retain(|key, _| live(key) && (key.parent != input.parent || keys.contains(key)));
        for tab in &input.tabs {
            let key = Key {
                parent: input.parent.clone(),
                tab: tab.kind.clone(),
            };
            if self
                .mounts
                .get(&key)
                .is_none_or(|old| !old.same_source(tab))
            {
                self.retained.remove(&key);
                let id = match self.mounts.get(&key) {
                    Some(old) => old.id,
                    None => {
                        let id = ComponentId(self.next);
                        self.next += 1;
                        id
                    }
                };
                self.epoch += 1;
                self.mounts.insert(
                    key,
                    TabMount {
                        id,
                        epoch: self.epoch,
                        layout: tab.child_history_layout,
                        task_scope: tab.task_scope.clone(),
                        child_parent: tab.child_parent.clone(),
                    },
                );
            }
        }
        debug_assert_eq!(self.next, next);
        debug_assert_eq!(self.epoch, epoch);
        self.context = context;
        if changed_parent {
            self.parent_order.retain(|parent| parent != &input.parent);
            self.parent_order.push_back(input.parent.clone());
            while self.parent_order.len() > self.retained_parent_limit {
                let removed = self.parent_order.pop_front().expect("retained parent");
                self.mounts.retain(|key, _| key.parent != removed);
                self.retained.retain(|key, _| key.parent != removed);
            }
        }
        let header = self.body.children[0]
            .component_mut::<StackContent>()
            .expect("panel tab strip");
        let old = std::mem::take(&mut header.children);
        let mut old: BTreeMap<_, _> = old
            .into_iter()
            .map(|content| (content.id(), content))
            .collect();
        let mut bindings = BTreeMap::new();
        for tab in &input.tabs {
            let id = self.mounts[&Key {
                parent: input.parent.clone(),
                tab: tab.kind.clone(),
            }]
                .id;
            let mut content = old
                .remove(&id)
                .unwrap_or_else(|| Content::new(id, ButtonContent::new("")));
            let button = content
                .component_mut::<ButtonContent>()
                .expect("tab button");
            button.button.state.selected = input.selected.as_ref() == Some(&tab.kind);
            button.label = tab.title.clone();
            button.variant = ButtonVariant::Quiet;
            header.children.push(content);
            bindings.insert(
                id,
                SidePanelIntent::Select {
                    parent: input.parent.clone(),
                    tab: tab.clone(),
                },
            );
        }
        let close_enabled = input.selected.as_ref().is_some_and(|tab| match tab {
            SidePanelTabKind::SideChat { session_id } => {
                input.side_chats.chats.get(session_id).is_some_and(|chat| {
                    input.projection_ready && chat.phase == SideChatPhase::Ready
                })
            }
            _ => true,
        });
        let create_enabled = input.projection_ready
            && input.supports_side_chat == Some(true)
            && !input.side_chats.is_creating_for(&input.parent)
            && input
                .side_chats
                .unknown_creation_for(&input.parent)
                .is_none();
        let toolbar = self.body.children[1]
            .component_mut::<StackContent>()
            .expect("panel controls");
        toolbar.children[0]
            .component_mut::<ButtonContent>()
            .expect("close tab")
            .button
            .state
            .disabled = !close_enabled;
        toolbar.children[1]
            .component_mut::<ButtonContent>()
            .expect("create SideChat")
            .button
            .state
            .disabled = !create_enabled;
        if close_enabled {
            bindings.insert(
                ComponentId(self.root.0 + 3),
                SidePanelIntent::Close {
                    parent: input.parent.clone(),
                    tab: input.selected.clone().expect("selected close target"),
                },
            );
        }
        if create_enabled {
            bindings.insert(
                ComponentId(self.root.0 + 4),
                SidePanelIntent::CreateSideChat {
                    parent: input.parent.clone(),
                },
            );
        }
        if !input.projection_ready {
            self.slot().children.clear();
            self.slot().flexible_child = None;
        } else if !same_content || !self.mounted_custom {
            if let Some(key) = &next_key
                && let Some(content) = self.retained.remove(key)
            {
                self.slot().flexible_child = Some(content.id());
                self.slot().children = vec![content];
                self.mounted_custom = true;
            } else {
                let selected = input
                    .tabs
                    .iter()
                    .find(|tab| input.selected.as_ref() == Some(&tab.kind));
                let task_status = selected
                    .and_then(|tab| tab.task_state.as_ref())
                    .filter(|state| state.value.is_none());
                let message = if let Some(state) = task_status {
                    state.error.as_deref().unwrap_or(if state.resolved {
                        &self.labels.missing_task
                    } else {
                        &self.labels.loading_task
                    })
                } else if next_key.is_some() {
                    &self.labels.awaiting_content
                } else {
                    &self.labels.empty
                };
                let placeholder =
                    Content::new(ComponentId(self.root.0 + 6), TextContent::new(message));
                self.slot().flexible_child = Some(placeholder.id());
                self.slot().children = vec![placeholder];
            }
        }
        let mut messages = Vec::new();
        if let Some(error) = input.side_chats.unknown_creation_for(&input.parent) {
            messages.push(error.to_owned());
        } else if input.side_chats.is_creating_for(&input.parent) {
            messages.push(self.labels.creating.clone());
        }
        if let Some(SidePanelTabKind::SideChat { session_id }) = input.selected.as_ref()
            && let Some(chat) = input.side_chats.chats.get(session_id)
        {
            match chat.phase {
                SideChatPhase::Ready => messages.extend(chat.error.iter().cloned()),
                SideChatPhase::Closing => messages.push(self.labels.closing.clone()),
                SideChatPhase::Unknown => messages.push(
                    chat.error
                        .clone()
                        .unwrap_or_else(|| self.labels.unknown_close.clone()),
                ),
            }
        }
        if input.supports_side_chat == Some(false) {
            messages.push(self.labels.unsupported_side_chat.clone());
        }
        let message =
            (input.projection_ready && !messages.is_empty()).then(|| messages.join(" · "));
        if self.body.children.len() == 4 {
            self.body.children.pop();
        }
        if let Some(message) = message {
            self.body.children.push(Content::new(
                ComponentId(self.root.0 + 7),
                TextContent::new(message),
            ));
        }
        self.mounted = next_key;
        self.input = Some(input.clone());
        self.bindings = bindings;
        Ok(())
    }
    pub fn content_token(&self, tab: &SidePanelTabKind) -> Option<PanelContentToken> {
        if !self.input.as_ref()?.projection_ready {
            return None;
        }
        let key = Key {
            parent: self.input.as_ref()?.parent.clone(),
            tab: tab.clone(),
        };
        let mount = self.mounts.get(&key)?;
        if matches!(tab, SidePanelTabKind::Task { .. }) && mount.task_scope.is_none() {
            return None;
        }
        Some(PanelContentToken {
            key,
            context: self.context,
            mount: mount.epoch,
        })
    }
    /// Mount actual Kit views with disjoint IDs. This never substitutes parent history for a child.
    pub fn install_content(
        &mut self,
        token: &PanelContentToken,
        content: Content,
    ) -> Result<(), &'static str> {
        if self.context != token.context
            || self
                .input
                .as_ref()
                .is_none_or(|input| !input.projection_ready || input.parent != token.key.parent)
            || self
                .mounts
                .get(&token.key)
                .is_none_or(|mount| mount.epoch != token.mount)
        {
            return Err("retired Agent pane content token");
        }
        if content.id().0 >> 32 == self.root.0 >> 32 {
            return Err("pane content must use its own component namespace");
        }
        if self.mounted.as_ref() == Some(&token.key) {
            self.slot().flexible_child = Some(content.id());
            self.slot().children = vec![content];
            self.mounted_custom = true;
        } else {
            self.retained.insert(token.key.clone(), content);
        }
        Ok(())
    }
    pub fn take_intents(&self, actions: &[ContentAction]) -> Vec<SidePanelIntent> {
        actions
            .iter()
            .filter(|action| action.kind == ContentActionKind::Activate)
            .filter_map(|action| self.bindings.get(&action.target).cloned())
            .collect()
    }
}
impl Component for SidePanel {
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
