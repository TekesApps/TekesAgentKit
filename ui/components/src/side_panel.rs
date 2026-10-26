//! Scoped Agent tabs and retained content. Task/child/SideChat execution stays in presentation.
use crate::{
    SidePanelLauncher, SidePanelLauncherInput, SidePanelLauncherIntent, SidePanelLauncherOwner,
    SidePanelLayout, SidePanelLayoutGeometry, side_panel_layout::AgentPanelBody,
};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};
use tekes_agent_presentation::{AgentTask, ChildHistoryLayout, RunScope, TaskPanelState};
pub use tekes_agent_presentation::{SideChat, SideChatPhase, SideChatSnapshot};
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
    Launch(SidePanelLauncherIntent),
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
    /// Legacy copy input, retained for source compatibility. Unsupported
    /// destinations are hidden/disabled rather than presented as an error.
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
/// The host owns the window/header allocation. Inline controls are retained
/// solely for the original diagnostic path; Host uses SidePanelNavigation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SidePanelHeaderPlacement {
    #[default]
    InlineDiagnostic,
    Host,
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
    owner: Arc<()>,
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
    content_owner: Arc<()>,
    root: ComponentId,
    body: StackContent,
    detached_chrome: Option<Vec<Content>>,
    layout: SidePanelLayout,
    geometry: Option<SidePanelLayoutGeometry>,
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
    launcher: Option<Content>,
    launcher_root: Option<ComponentId>,
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
            content_owner: Arc::new(()),
            root: ComponentId(base),
            launcher: None,
            launcher_root: None,
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
                        AgentPanelBody {
                            framed: false,
                            layout: None,
                            stack: StackContent {
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
                        },
                    ),
                ],
            },
            layout: SidePanelLayout::default(),
            detached_chrome: None,
            geometry: None,
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
    pub fn header_placement(&self) -> SidePanelHeaderPlacement {
        if self.detached_chrome.is_some() {
            SidePanelHeaderPlacement::Host
        } else {
            SidePanelHeaderPlacement::InlineDiagnostic
        }
    }
    /// Retains the exact pane body and mount tokens. Detached diagnostic controls
    /// have no mounted input/AX subtree or callback admission. The host mounts a
    /// disjoint SidePanelNavigation from the same input and owns toolbar placement.
    pub fn set_header_placement(&mut self, placement: SidePanelHeaderPlacement) {
        if self.header_placement() == placement {
            return;
        }
        match placement {
            SidePanelHeaderPlacement::Host => {
                self.detached_chrome = Some(self.body.children.drain(..2).collect());
            }
            SidePanelHeaderPlacement::InlineDiagnostic => {
                self.body
                    .children
                    .splice(0..0, self.detached_chrome.take().unwrap());
            }
        }
        self.geometry = None;
    }
    /// Layout-only host input. Does not replace pane content, tokens or editors.
    pub fn set_layout(&mut self, layout: SidePanelLayout) -> Result<(), &'static str> {
        layout.validate()?;
        if self.layout != layout {
            self.layout = layout;
            self.geometry = None;
        }
        Ok(())
    }
    pub fn layout_geometry(&self) -> Option<SidePanelLayoutGeometry> {
        self.geometry
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
    pub fn launcher(&self) -> Option<&SidePanelLauncher> {
        self.launcher
            .as_ref()
            .or_else(|| {
                self.body_content()
                    .component::<AgentPanelBody>()?
                    .stack
                    .children
                    .iter()
                    .find(|child| Some(child.id()) == self.launcher_root)
            })?
            .component::<SidePanelLauncher>()
    }
    fn detach_launcher(&mut self) -> Option<Content> {
        if let Some(content) = self.launcher.take() {
            return Some(content);
        }
        let id = self.launcher_root?;
        let slot = self.slot();
        let index = slot.children.iter().position(|child| child.id() == id)?;
        Some(slot.children.remove(index))
    }
    fn mount_launcher(&mut self) {
        let eligible = self.input.as_ref().is_some_and(|input| {
            input.projection_ready
                && input.selected.is_none()
                && self
                    .launcher()
                    .and_then(SidePanelLauncher::displayed_input)
                    .is_some_and(|launcher| {
                        launcher.owner == SidePanelLauncherOwner::Run(input.parent.clone())
                    })
        });
        if eligible && let Some(content) = self.launcher.take() {
            self.slot().flexible_child = Some(content.id());
            self.slot().children = vec![content];
        }
    }
    /// Publish after each panel update and before rendering, using a disjoint
    /// namespace. Hiding retires its controls; fresh capabilities re-enable them.
    /// Standalone workspace launchers need no fabricated RunScope.
    pub fn configure_launcher(
        &mut self,
        namespace: u32,
        input: &SidePanelLauncherInput,
    ) -> Result<(), &'static str> {
        let root = ComponentId((namespace as u64) << 32);
        if namespace == (self.root.0 >> 32) as u32
            || self.launcher_root.is_some_and(|old| old != root)
        {
            return Err("invalid or changed launcher namespace");
        }
        if self
            .input
            .as_ref()
            .is_none_or(|panel| input.owner != SidePanelLauncherOwner::Run(panel.parent.clone()))
        {
            return Err("launcher parent mismatch");
        }
        let fresh = SidePanelLauncher::new(namespace, self.labels.name.clone());
        self.launcher().unwrap_or(&fresh).validate_input(input)?;
        let mut content = self
            .detach_launcher()
            .unwrap_or_else(|| Content::new(root, fresh));
        let launcher = content.component_mut::<SidePanelLauncher>().unwrap();
        if self
            .input
            .as_ref()
            .is_some_and(|panel| panel.projection_ready && panel.selected.is_none())
        {
            launcher.update(input)?;
        } else {
            launcher.update_hidden(input)?;
        }
        self.launcher_root = Some(root);
        self.launcher = Some(content);
        self.mount_launcher();
        Ok(())
    }
    pub fn accepts_launcher_intent(&self, intent: &SidePanelLauncherIntent) -> bool {
        self.input.as_ref().is_some_and(|input| {
            input.projection_ready
                && input.selected.is_none()
                && intent.owner == SidePanelLauncherOwner::Run(input.parent.clone())
        }) && self
            .body
            .children
            .last()
            .unwrap()
            .component::<AgentPanelBody>()
            .is_some_and(|body| {
                body.stack
                    .children
                    .iter()
                    .any(|child| Some(child.id()) == self.launcher_root)
            })
            && self
                .launcher()
                .is_some_and(|launcher| launcher.accepts(intent))
    }
    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let mut host = ContentHost::new(Content::new(self.root, self))?;
        host.set_action_events(true);
        Ok(host)
    }
    pub fn tab_id(&self, tab: &SidePanelTabKind) -> Option<ComponentId> {
        if self.detached_chrome.is_some() {
            return None;
        }
        let parent = &self.input.as_ref()?.parent;
        self.mounts
            .get(&Key {
                parent: parent.clone(),
                tab: tab.clone(),
            })
            .map(|mount| mount.id)
    }
    fn body_content(&self) -> &Content {
        self.body
            .children
            .iter()
            .find(|node| node.id() == self.content_slot_id())
            .unwrap()
    }
    fn body_content_mut(&mut self) -> &mut Content {
        let id = self.content_slot_id();
        self.body
            .children
            .iter_mut()
            .find(|node| node.id() == id)
            .unwrap()
    }
    fn slot(&mut self) -> &mut StackContent {
        &mut self
            .body_content_mut()
            .component_mut::<AgentPanelBody>()
            .expect("panel content slot")
            .stack
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
        let keys = validated_keys(input, self.input.as_ref())?;
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
        if let Some(mut launcher) = self.detach_launcher() {
            if changed_parent || !input.projection_ready || input.selected.is_some() {
                launcher
                    .component_mut::<SidePanelLauncher>()
                    .unwrap()
                    .retire_controls();
            }
            self.launcher = Some(launcher);
        }
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
        let chrome = self
            .detached_chrome
            .as_mut()
            .unwrap_or(&mut self.body.children);
        let header = chrome[0]
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
        let chrome = self
            .detached_chrome
            .as_mut()
            .unwrap_or(&mut self.body.children);
        let toolbar = chrome[1]
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
        let message =
            (input.projection_ready && !messages.is_empty()).then(|| messages.join(" · "));
        let status_id = ComponentId(self.root.0 + 7);
        self.body.children.retain(|node| node.id() != status_id);
        if let Some(message) = message {
            self.body.children.push(Content::new(
                ComponentId(self.root.0 + 7),
                TextContent::new(message),
            ));
        }
        self.body_content_mut()
            .component_mut::<AgentPanelBody>()
            .unwrap()
            .framed = input.projection_ready && input.selected.is_some();
        self.geometry = None;
        self.mounted = next_key;
        self.input = Some(input.clone());
        self.bindings = bindings;
        self.mount_launcher();
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
            owner: self.content_owner.clone(),
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
        self.validate_content_token(token)?;
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
    fn validate_content_token(&self, token: &PanelContentToken) -> Result<(), &'static str> {
        if !Arc::ptr_eq(&self.content_owner, &token.owner)
            || self.context != token.context
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
        Ok(())
    }
    /// Retire one producer's mounted or retained UI when its owner changes,
    /// even if the next producer resolves the same logical tab and Run scope.
    /// Ordinary facts updates should use `update` and retain their controls.
    /// Hosts must finish composition and clear affected focus before this call.
    /// This UI token does not replace binding/producer admission checks.
    pub fn retire_content(&mut self, token: &PanelContentToken) -> Result<(), &'static str> {
        self.validate_content_token(token)?;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("panel mount epoch exhausted")?;
        let input = self.input.as_ref().expect("validated panel input").clone();
        if self.mounted.as_ref() == Some(&token.key) {
            // Drop immediately. Stashing would let the retired producer return
            // on the next tab selection with the same logical scope.
            self.slot().children.clear();
            self.slot().flexible_child = None;
            self.mounted_custom = false;
        }
        self.retained.remove(&token.key);
        self.mounts
            .get_mut(&token.key)
            .expect("validated mount")
            .epoch = epoch;
        self.epoch = epoch;
        self.geometry = None;
        // The unchanged, previously validated input needs no new IDs,
        // context, or mount epochs, so update cannot fail a fresh allocation
        // preflight after this target-only retirement.
        self.update(&input)
    }
    pub fn take_intents(&self, actions: &[ContentAction]) -> Vec<SidePanelIntent> {
        let mut intents: Vec<_> = actions
            .iter()
            .filter(|action| action.kind == ContentActionKind::Activate)
            .filter(|_| self.detached_chrome.is_none())
            .filter_map(|action| self.bindings.get(&action.target).cloned())
            .collect();
        if let Some(launcher) = self.launcher() {
            intents.extend(
                launcher
                    .take_intents(actions)
                    .into_iter()
                    .filter(|intent| self.accepts_launcher_intent(intent))
                    .map(SidePanelIntent::Launch),
            );
        }
        intents
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
        let _ = text;
        MeasuredChild::new(available)
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        let layout = self.layout.canvas(frame, clip);
        self.body.arrange(
            text,
            layout.content(),
            layout.visible_clip().unwrap_or_default(),
        );
        let body = self
            .body
            .children
            .last()
            .unwrap()
            .component::<AgentPanelBody>()
            .unwrap()
            .layout
            .unwrap();
        self.geometry = Some(SidePanelLayoutGeometry {
            pane: layout.frame(),
            canvas: layout.content(),
            body: body.content(),
            visible_body: body.visible_clip(),
        });
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.labels.name)
    }
}

fn validated_keys(
    input: &SidePanelInput,
    previous: Option<&SidePanelInput>,
) -> Result<BTreeSet<Key>, &'static str> {
    if input.revision == 0
        || previous.is_some_and(|old| {
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
    if let Some(previous) = previous
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
    Ok(keys)
}

pub(crate) fn validate_panel_navigation_input(
    input: &SidePanelInput,
    previous: Option<&SidePanelInput>,
) -> Result<(), &'static str> {
    validated_keys(input, previous).map(|_| ())
}
