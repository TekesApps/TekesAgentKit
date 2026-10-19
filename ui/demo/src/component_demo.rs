//! Demo host layout for real Composer/Indicators, not a replacement RunView.
//! The absent structured Transcript is explicit until upstream #22/#23 land.
use std::{collections::BTreeMap, sync::Arc};
use tekes_agent_components_ui::{
    Composer, ComposerAffordances, ComposerEntry, ComposerIds, ComposerIntent, Indicators,
    SidePanel, SidePanelInput, SidePanelIntent, SidePanelLabels, SidePanelTab, SidePanelTabKind,
    Sidebar, SidebarInput, SidebarIntent, SidebarLabels, SummaryInput, SummaryIntent,
    SummaryLabels, SummaryView,
};
use tekes_agent_contract::{PromptMode, Result, SessionAddress, require, unsupported};
use tekes_agent_presentation::{
    ComposerDestination, PreparedSubmission, RunScope, SideChat, SideChatSnapshot,
};
use tekes_agent_ui_bindings::{BoundSideChatRun, RunBinding, SideChatBinding, SidebarPublication};
use tekes_ui_controls::{
    ButtonTextRenderer, TextSurface,
    composition::*,
    extended::{Axis, Stack},
    layout::MeasuredChild,
};
use tekes_ui_scene::{Rect, Scene, Size};
use tekes_ui_tokens::{Palette, Spacing};

const ROOT: ComponentId = ComponentId(1);
const NAV: ComponentId = ComponentId(2);
const NOTICE: ComponentId = ComponentId(3);
const DIAGNOSTIC: ComponentId = ComponentId(4);
const MAIN: ComponentId = ComponentId(5);
const SIDEBAR_COLUMN: ComponentId = ComponentId(6);
const PANEL_COLUMN: ComponentId = ComponentId(7);
const MAIN_COLUMN: ComponentId = ComponentId(8);
const SIDEBAR_NAMESPACE: u32 = 40;
const PANEL_NAMESPACE: u32 = 41;
pub const SELECT_A: ComponentId = ComponentId(100);
pub const SELECT_B: ComponentId = ComponentId(101);
pub const RECONNECT: ComponentId = ComponentId(102);

pub enum DemoCommand {
    CreateSideChat(PanelRequest),
    OpenSideChat {
        request: PanelRequest,
        chat: SideChat,
    },
    CloseSideChat(SideChat),
    OpenSession(NavigationRequest),
    Sidebar(SidebarIntent),
    Summary(SummaryIntent),
    Send {
        binding: Arc<RunBinding>,
        ticket: PreparedSubmission,
    },
    Cancel {
        binding: Arc<RunBinding>,
        destination: ComposerDestination,
    },
    Reconnect,
    OpenEntry {
        destination: ComposerDestination,
        entry: ComposerEntry,
    },
}

pub struct ComponentDemo {
    pub tree: ContentHost,
    bindings: Vec<Arc<RunBinding>>,
    selected: usize,
    retained: Vec<Option<Content>>,
    navigation_revision: u64,
    sidebar_source: Option<Arc<SidebarPublication>>,
    sidebar_presented: Option<SidebarInput>,
    panel_revision: u64,
    side_chats: Option<Arc<SideChatBinding>>,
    panel_selections: BTreeMap<RunScope, SidePanelTabKind>,
    panel_runs: BTreeMap<(RunScope, String), PanelRun>,
    next_panel_namespace: u32,
    panel_navigation_revision: u64,
}
struct PanelRun {
    binding: Arc<RunBinding>,
    ids: ComposerIds,
}
#[derive(Clone, Debug)]
pub struct PanelRequest {
    parent: RunScope,
    main_revision: u64,
    panel_revision: u64,
}
impl PanelRequest {
    pub fn parent(&self) -> &RunScope {
        &self.parent
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavigationRequest {
    address: SessionAddress,
    revision: u64,
}
impl NavigationRequest {
    pub fn address(&self) -> &SessionAddress {
        &self.address
    }
}
// This fixed column width is demo-host window layout, not a Kit control or an
// alternative layout engine. Child measurement/arrangement stays in public Content.
#[derive(Debug)]
struct DemoColumn {
    width: Option<f32>,
    children: Vec<Content>,
}
impl Component for DemoColumn {
    fn children(&self) -> &[Content] {
        &self.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        let width = self.width.unwrap_or(available.width).min(available.width);
        self.children[0].measure(text, Size::new(width, available.height));
        MeasuredChild::new(Size::new(width, available.height))
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        self.children[0].arrange(text, frame, clip);
    }
}
impl ComponentDemo {
    pub fn new(bindings: impl IntoIterator<Item = Arc<RunBinding>>) -> Result<Self> {
        let bindings: Vec<_> = bindings.into_iter().collect();
        require(
            !bindings.is_empty()
                && bindings.iter().enumerate().all(|(index, binding)| {
                    bindings[..index]
                        .iter()
                        .all(|other| other.scope() != binding.scope())
                }),
            "demo needs distinct run sources",
        )?;
        let pair = Self::new_pair(0);
        let main = Content::new(
            MAIN,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Regular.value(),
                },
                flexible_child: Some(DIAGNOSTIC),
                children: vec![
                    Content::new(
                        NAV,
                        StackContent {
                            model: Stack {
                                axis: Axis::Horizontal,
                                gap: Spacing::Tight.value(),
                            },
                            flexible_child: None,
                            children: [
                                (SELECT_A, "会话 A"),
                                (SELECT_B, "会话 B"),
                                (RECONNECT, "重新连接"),
                            ]
                            .into_iter()
                            .map(|(id, label)| Content::new(id, ButtonContent::new(label)))
                            .collect(),
                        },
                    ),
                    Content::new(
                        NOTICE,
                        TextContent::new("正式 Composer / Indicators 组件验证"),
                    ),
                    Content::new(
                        DIAGNOSTIC,
                        TextContent::new("Transcript 待 TekesUI #22 / #23；此处不是消息视图。"),
                    ),
                    pair,
                ],
            },
        );
        let sidebar = Sidebar::new(SIDEBAR_NAMESPACE, SidebarLabels::default());
        let panel = SidePanel::new(PANEL_NAMESPACE, SidePanelLabels::default(), 8);
        let mut tree = ContentHost::new(Content::new(
            ROOT,
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Regular.value(),
                },
                flexible_child: Some(MAIN_COLUMN),
                children: vec![
                    Content::new(
                        SIDEBAR_COLUMN,
                        DemoColumn {
                            width: Some(180.),
                            children: vec![Content::new(sidebar.id(), sidebar)],
                        },
                    ),
                    Content::new(
                        MAIN_COLUMN,
                        DemoColumn {
                            width: None,
                            children: vec![main],
                        },
                    ),
                    Content::new(
                        PANEL_COLUMN,
                        DemoColumn {
                            width: Some(240.),
                            children: vec![Content::new(panel.id(), panel)],
                        },
                    ),
                ],
            },
        ))
        .map_err(|_| unsupported("component demo mount"))?;
        tree.set_action_events(true);
        let retained = (0..bindings.len()).map(|_| None).collect();
        let mut view = Self {
            tree,
            bindings,
            selected: 0,
            retained,
            navigation_revision: 0,
            sidebar_source: None,
            sidebar_presented: None,
            panel_revision: 0,
            side_chats: None,
            panel_selections: BTreeMap::new(),
            panel_runs: BTreeMap::new(),
            next_panel_namespace: 50_000,
            panel_navigation_revision: 0,
        };
        view.sync()?;
        Ok(view)
    }
    fn new_pair(index: usize) -> Content {
        let ids = Self::ids_for(index);
        Self::run_controls(ids)
    }
    fn run_controls(ids: ComposerIds) -> Content {
        Content::new(
            ComponentId(ids.root.0 + 16),
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Regular.value(),
                },
                flexible_child: None,
                children: vec![
                    Content::new(ComponentId(ids.root.0 + 17), Indicators::default()),
                    Content::new(ids.root, Composer::new(ids)),
                ],
            },
        )
    }
    fn pair_id(index: usize) -> ComponentId {
        ComponentId(Self::ids_for(index).root.0 + 16)
    }
    fn indicator_id(index: usize) -> ComponentId {
        ComponentId(Self::ids_for(index).root.0 + 17)
    }
    fn ids_for(index: usize) -> ComposerIds {
        assert!(
            index < (50_000 - 101) / 2,
            "demo component namespaces exhausted"
        );
        ComposerIds::new((100 + index as u64 * 2) << 32)
    }
    pub fn ids(&self) -> ComposerIds {
        Self::ids_for(self.selected)
    }
    pub fn selected(&self) -> usize {
        self.selected
    }
    pub fn binding(&self) -> &Arc<RunBinding> {
        &self.bindings[self.selected]
    }
    pub fn sidebar_id(&self) -> ComponentId {
        ComponentId((SIDEBAR_NAMESPACE as u64) << 32)
    }
    pub fn panel_id(&self) -> ComponentId {
        ComponentId((PANEL_NAMESPACE as u64) << 32)
    }
    pub fn summary_id(&self) -> ComponentId {
        ComponentId((101 + self.selected as u64 * 2) << 32)
    }
    pub fn set_sidebar_source(&mut self, source: Arc<SidebarPublication>) -> Result<()> {
        require(
            self.sidebar_source
                .as_ref()
                .is_none_or(|old| source.revision >= old.revision),
            "stale sidebar source",
        )?;
        self.sidebar_source = Some(source);
        Ok(())
    }
    pub fn bindings(&self) -> &[Arc<RunBinding>] {
        &self.bindings
    }
    pub fn set_side_chat_binding(&mut self, binding: Arc<SideChatBinding>) {
        self.side_chats = Some(binding);
    }
    pub fn panel_composer_ids(&self) -> Option<ComposerIds> {
        let parent = self.binding().scope();
        let SidePanelTabKind::SideChat { session_id } = self.panel_selections.get(parent)? else {
            return None;
        };
        self.panel_runs
            .get(&(parent.clone(), session_id.clone()))
            .map(|run| run.ids)
            .filter(|ids| self.tree.get(ids.root).is_some())
    }
    fn composing_editor(&self) -> Option<ComponentId> {
        std::iter::once(self.ids())
            .chain(self.panel_composer_ids())
            .find_map(|ids| {
                self.tree
                    .get(ids.root)
                    .and_then(|node| node.component::<Composer>())
                    .filter(|composer| composer.is_composing())
                    .map(|_| ids.editor)
            })
    }
    fn panel_request(&mut self, parent: RunScope) -> PanelRequest {
        self.panel_navigation_revision += 1;
        PanelRequest {
            parent,
            main_revision: self.navigation_revision,
            panel_revision: self.panel_navigation_revision,
        }
    }
    pub fn has_side_chat_run(&self, chat: &SideChat) -> bool {
        self.panel_runs
            .contains_key(&(chat.parent.clone(), chat.scope.session_id.clone()))
    }
    pub fn install_side_chat_run(
        &mut self,
        request: PanelRequest,
        run: BoundSideChatRun,
    ) -> Result<bool> {
        let owner = self
            .side_chats
            .as_ref()
            .ok_or_else(|| unsupported("SideChat owner unavailable"))?;
        let chat = run.chat();
        require(
            chat.parent == request.parent,
            "SideChat result parent mismatch",
        )?;
        if owner.pointer(&chat.parent, &chat.scope.session_id).is_err() {
            return Ok(false);
        }
        let key = (chat.parent.clone(), chat.scope.session_id.clone());
        if !self.panel_runs.contains_key(&key) {
            let namespace = self.next_panel_namespace;
            self.next_panel_namespace = namespace
                .checked_add(1)
                .filter(|next| *next <= 65_535)
                .ok_or_else(|| unsupported("demo pane namespaces exhausted"))?;
            self.panel_runs.insert(
                key,
                PanelRun {
                    binding: run.binding().clone(),
                    ids: ComposerIds::new((namespace as u64) << 32),
                },
            );
        }
        let selected = self.binding().scope() == &request.parent
            && self.navigation_revision == request.main_revision
            && self.panel_navigation_revision == request.panel_revision
            && !self.composing();
        if selected {
            self.panel_selections.insert(
                request.parent,
                SidePanelTabKind::SideChat {
                    session_id: chat.scope.session_id.clone(),
                },
            );
        }
        self.sync()?;
        if selected && let Some(ids) = self.panel_composer_ids() {
            self.tree.focus(Some(ids.editor), true);
        }
        Ok(selected)
    }
    pub fn editor(&self) -> &TextSurface {
        self.tree
            .get(self.ids().root)
            .expect("mounted composer")
            .component::<Composer>()
            .expect("Composer")
            .editor()
    }
    pub fn composing(&self) -> bool {
        self.composing_editor().is_some()
    }
    pub fn set_notice(&mut self, text: impl Into<String>) {
        self.tree
            .get_mut(NOTICE)
            .expect("notice")
            .component_mut::<TextContent>()
            .expect("text")
            .text = text.into();
    }
    pub fn sync(&mut self) -> Result<()> {
        let binding = self.binding().clone();
        let snapshot = binding
            .current_run()
            .ok_or_else(|| unsupported("demo needs a present main run"))?;
        let ids = self.ids();
        binding.sync_composer(
            self.tree
                .get_mut(ids.root)
                .expect("composer")
                .component_mut::<Composer>()
                .expect("Composer"),
            ComposerAffordances::default(),
        )?;
        self.tree
            .get_mut(Self::indicator_id(self.selected))
            .expect("indicators")
            .component_mut::<Indicators>()
            .expect("Indicators")
            .update(&snapshot)
            .map_err(unsupported)?;
        for (index, id) in [SELECT_A, SELECT_B].into_iter().enumerate() {
            let composing = self.composing();
            let button = &mut self
                .tree
                .get_mut(id)
                .expect("navigation")
                .component_mut::<ButtonContent>()
                .expect("button")
                .button;
            button.state.selected = index == self.selected;
            button.state.disabled = composing || index >= self.bindings.len();
        }
        self.tree
            .get_mut(DIAGNOSTIC)
            .expect("diagnostic")
            .component_mut::<TextContent>()
            .expect("text")
            .text = format!(
            "Transcript 待 TekesUI #22 / #23；此处不是消息视图。\n回传诊断：{} 条内容，{} 条流式内容，历史{}同步。",
            snapshot.transcript.items.len(),
            snapshot
                .transcript
                .items
                .iter()
                .filter(|item| item.is_provisional)
                .count(),
            if snapshot.transcript.is_stale {
                "待"
            } else {
                "已"
            }
        );
        if let Some(source) = &self.sidebar_source {
            let selected = Some(SessionAddress {
                endpoint_id: binding.scope().endpoint_id.clone(),
                session_id: binding.scope().session_id.clone(),
            });
            let next = SidebarInput {
                revision: self
                    .sidebar_presented
                    .as_ref()
                    .map_or(1, |old| old.revision + 1),
                rows: source.rows.clone(),
                selected,
            };
            if self
                .sidebar_presented
                .as_ref()
                .is_none_or(|old| old.rows != next.rows || old.selected != next.selected)
            {
                let id = self.sidebar_id();
                self.tree
                    .get_mut(id)
                    .expect("sidebar")
                    .component_mut::<Sidebar>()
                    .expect("Sidebar")
                    .update(&next)
                    .map_err(unsupported)?;
                self.sidebar_presented = Some(next);
            }
        }
        let panel_id = self.panel_id();
        let parent = binding.scope().clone();
        let wanted = self
            .panel_selections
            .entry(parent.clone())
            .or_insert(SidePanelTabKind::Summary)
            .clone();
        self.panel_revision += 1;
        let mut input = if let Some(owner) = &self.side_chats {
            let snapshot = owner.snapshot();
            self.panel_runs.retain(|(parent, id), _| {
                snapshot
                    .chats
                    .get(id)
                    .is_some_and(|chat| &chat.parent == parent)
            });
            owner.input(
                self.panel_revision,
                &parent,
                vec![SidePanelTab::summary("Summary")],
                Some(wanted),
            )?
        } else {
            SidePanelInput {
                revision: self.panel_revision,
                parent: parent.clone(),
                tabs: vec![SidePanelTab::summary("Summary")],
                selected: Some(SidePanelTabKind::Summary),
                side_chats: SideChatSnapshot::default(),
                supports_side_chat: None,
            }
        };
        if input.selected.is_none() {
            // A confirmed lost scratch pointer never becomes a new conversation.
            self.tree.focus(None, false);
            input.selected = Some(SidePanelTabKind::Summary);
        }
        let selected = input.selected.clone().expect("Summary fallback");
        self.panel_selections
            .insert(parent.clone(), selected.clone());
        self.tree
            .get_mut(panel_id)
            .expect("panel")
            .component_mut::<SidePanel>()
            .expect("SidePanel")
            .update(&input)
            .map_err(unsupported)?;
        match selected {
            SidePanelTabKind::Summary => {
                let summary_id = self.summary_id();
                if self.tree.get(summary_id).is_none() {
                    let summary =
                        SummaryView::new(101 + self.selected as u32 * 2, SummaryLabels::default());
                    let panel = self
                        .tree
                        .get_mut(panel_id)
                        .expect("panel")
                        .component_mut::<SidePanel>()
                        .expect("SidePanel");
                    let token = panel
                        .content_token(&SidePanelTabKind::Summary)
                        .expect("Summary token");
                    panel
                        .install_content(&token, Content::new(summary_id, summary))
                        .map_err(unsupported)?;
                }
                self.tree
                    .get_mut(summary_id)
                    .expect("summary")
                    .component_mut::<SummaryView>()
                    .expect("SummaryView")
                    .update(&SummaryInput {
                        revision: snapshot.revision,
                        scope: parent,
                        facts: snapshot.summary.clone(),
                        host_sections: vec![],
                    })
                    .map_err(unsupported)?;
            }
            SidePanelTabKind::SideChat { session_id } => {
                if let Some(run) = self.panel_runs.get(&(parent, session_id.clone())) {
                    let ids = run.ids;
                    let binding = run.binding.clone();
                    if let Some(snapshot) = binding.current_run() {
                        if self.tree.get(ids.root).is_none() {
                            let panel = self
                                .tree
                                .get_mut(panel_id)
                                .expect("panel")
                                .component_mut::<SidePanel>()
                                .expect("SidePanel");
                            let token = panel
                                .content_token(&SidePanelTabKind::SideChat { session_id })
                                .expect("scratch token");
                            panel
                                .install_content(&token, Self::run_controls(ids))
                                .map_err(unsupported)?;
                        }
                        let gate = binding.access().composer_gate();
                        self.tree
                            .get_mut(ids.root)
                            .expect("scratch composer")
                            .component_mut::<Composer>()
                            .expect("Composer")
                            .update_gated(&snapshot, ComposerAffordances::default(), gate)
                            .map_err(unsupported)?;
                        self.tree
                            .get_mut(ComponentId(ids.root.0 + 17))
                            .expect("scratch indicators")
                            .component_mut::<Indicators>()
                            .expect("Indicators")
                            .update(&snapshot)
                            .map_err(unsupported)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub fn select(&mut self, index: usize) -> Result<()> {
        require(index < self.bindings.len(), "unknown demo session")?;
        require(
            !self.composing(),
            "finish or cancel composition before switching sessions",
        )?;
        self.navigation_revision = self
            .navigation_revision
            .checked_add(1)
            .ok_or_else(|| unsupported("navigation revision exhausted"))?;
        if index == self.selected {
            self.tree.focus(Some(self.ids().editor), true);
            return Ok(());
        }
        self.tree.focus(None, false);
        self.retained[self.selected] = Some(
            self.tree
                .remove(Self::pair_id(self.selected))
                .map_err(|_| unsupported("detach component pair"))?,
        );
        let content = self.retained[index]
            .take()
            .unwrap_or_else(|| Self::new_pair(index));
        self.tree
            .insert(MAIN, content)
            .map_err(|_| unsupported("restore component pair"))?;
        self.selected = index;
        self.tree
            .reorder(MAIN, &[NAV, NOTICE, DIAGNOSTIC, Self::pair_id(index)])
            .map_err(|_| unsupported("demo layout order"))?;
        self.sync()?;
        self.tree.focus(Some(self.ids().editor), true);
        Ok(())
    }
    pub fn complete_navigation(
        &mut self,
        request: NavigationRequest,
        binding: Arc<RunBinding>,
    ) -> Result<bool> {
        require(
            binding.scope()
                == &RunScope::main(&request.address.endpoint_id, &request.address.session_id),
            "navigation result target mismatch",
        )?;
        let disappeared = self.sidebar_source.as_ref().is_some_and(|source| {
            source.loaded
                && source.error.is_none()
                && !source.rows.iter().any(|row| row.address == request.address)
        });
        if request.revision != self.navigation_revision {
            return Ok(false);
        }
        if self.composing() || disappeared {
            self.navigation_revision = self
                .navigation_revision
                .checked_add(1)
                .ok_or_else(|| unsupported("navigation revision exhausted"))?;
            return Ok(false);
        }
        let index = if let Some(index) = self
            .bindings
            .iter()
            .position(|old| old.scope() == binding.scope())
        {
            index
        } else {
            self.retained.push(None);
            self.bindings.push(binding);
            self.bindings.len() - 1
        };
        self.select(index)?;
        Ok(true)
    }
    fn blocked_pointer(&self, event: &ContentEvent) -> bool {
        if !self.composing() {
            return false;
        }
        let position = match event {
            ContentEvent::PointerDown(position) => Some(*position),
            ContentEvent::Pointer(pointer)
                if pointer.phase == PointerPhase::Down && pointer.is_primary() =>
            {
                Some(pointer.position)
            }
            _ => None,
        };
        position.is_some_and(|position| {
            !self
                .tree
                .get(self.composing_editor().expect("composing owner"))
                .expect("editor")
                .frame()
                .contains(position)
        })
    }
    pub fn event(&mut self, event: &ContentEvent) -> Result<Vec<DemoCommand>> {
        if self.blocked_pointer(event) {
            return Ok(vec![]);
        }
        self.tree.event(event);
        self.drain_commands()
    }
    pub fn event_with_text(
        &mut self,
        event: &ContentEvent,
        text: &mut dyn ButtonTextRenderer,
    ) -> Result<Vec<DemoCommand>> {
        if self.blocked_pointer(event) {
            return Ok(vec![]);
        }
        self.tree
            .event_with_text(event, text)
            .map_err(|_| unsupported("demo text input geometry"))?;
        self.drain_commands()
    }
    pub fn accessibility_action(
        &mut self,
        target: ComponentId,
        part: Option<usize>,
        action: AccessibilityAction,
    ) -> Result<Vec<DemoCommand>> {
        if self
            .composing_editor()
            .is_some_and(|editor| editor != target)
        {
            return Ok(vec![]);
        }
        self.tree.accessibility_action(target, part, action);
        self.drain_commands()
    }
    pub fn drain_commands(&mut self) -> Result<Vec<DemoCommand>> {
        let actions = self.tree.drain_actions();
        let binding = self.binding().clone();
        let ids = self.ids();
        let mut composers = vec![(ids, binding.clone())];
        if let Some(ids) = self.panel_composer_ids()
            && let Some(SidePanelTabKind::SideChat { session_id }) =
                self.panel_selections.get(binding.scope())
            && let Some(run) = self
                .panel_runs
                .get(&(binding.scope().clone(), session_id.clone()))
        {
            composers.push((ids, run.binding.clone()));
        }
        let intents: Vec<_> = composers
            .into_iter()
            .flat_map(|(ids, binding)| {
                self.tree
                    .get_mut(ids.root)
                    .expect("mounted composer")
                    .component_mut::<Composer>()
                    .expect("Composer")
                    .take_intents(&actions)
                    .into_iter()
                    .map(move |intent| (binding.clone(), intent))
            })
            .collect();
        let mut commands = vec![];
        let sidebar_intents = self
            .tree
            .get(self.sidebar_id())
            .expect("sidebar")
            .component::<Sidebar>()
            .expect("Sidebar")
            .take_intents(&actions);
        let summary_intents = self
            .tree
            .get(self.summary_id())
            .and_then(|node| node.component::<SummaryView>())
            .map(|summary| summary.take_intents(&actions))
            .unwrap_or_default();
        let panel_intents = self
            .tree
            .get(self.panel_id())
            .expect("panel")
            .component::<SidePanel>()
            .expect("SidePanel")
            .take_intents(&actions);
        for intent in sidebar_intents {
            if let SidebarIntent::Select { address } = intent {
                if let Some(index) = self.bindings.iter().position(|binding| {
                    binding.scope().endpoint_id == address.endpoint_id
                        && binding.scope().session_id == address.session_id
                }) {
                    self.select(index)?;
                } else {
                    self.navigation_revision = self
                        .navigation_revision
                        .checked_add(1)
                        .ok_or_else(|| unsupported("navigation revision exhausted"))?;
                    commands.push(DemoCommand::OpenSession(NavigationRequest {
                        address,
                        revision: self.navigation_revision,
                    }));
                }
            } else {
                commands.push(DemoCommand::Sidebar(intent));
            }
        }
        commands.extend(summary_intents.into_iter().map(DemoCommand::Summary));
        for intent in panel_intents {
            let Some(owner) = self.side_chats.clone() else {
                continue;
            };
            match intent {
                SidePanelIntent::CreateSideChat { parent } => {
                    commands.push(DemoCommand::CreateSideChat(self.panel_request(parent)));
                }
                SidePanelIntent::Close {
                    parent,
                    tab: SidePanelTabKind::SideChat { session_id },
                } => {
                    self.panel_navigation_revision += 1;
                    commands.push(DemoCommand::CloseSideChat(
                        owner.pointer(&parent, &session_id)?,
                    ));
                }
                SidePanelIntent::Select { parent, tab } if &parent == self.binding().scope() => {
                    self.tree.focus(None, false);
                    self.panel_selections
                        .insert(parent.clone(), tab.kind.clone());
                    let request = self.panel_request(parent.clone());
                    if let SidePanelTabKind::SideChat { session_id } = tab.kind {
                        let chat = owner.pointer(&parent, &session_id)?;
                        if !self.has_side_chat_run(&chat) {
                            commands.push(DemoCommand::OpenSideChat { request, chat });
                        }
                    }
                }
                _ => {}
            }
        }

        for (binding, intent) in intents {
            match intent {
                ComposerIntent::Edit { destination, text } => {
                    binding.edit_text(&destination, text)?
                }
                ComposerIntent::Send { destination } => {
                    let ticket = binding.prepare_send(&destination, PromptMode::Queue)?;
                    commands.push(DemoCommand::Send {
                        binding: binding.clone(),
                        ticket,
                    });
                }
                ComposerIntent::Cancel { scope } => commands.push(DemoCommand::Cancel {
                    binding: binding.clone(),
                    destination: ComposerDestination::run(scope),
                }),
                ComposerIntent::OpenEntry { destination, entry } => {
                    commands.push(DemoCommand::OpenEntry { destination, entry })
                }
            }
        }
        for action in actions
            .iter()
            .filter(|action| action.kind == ContentActionKind::Activate)
        {
            match action.target {
                SELECT_A => self.select(0)?,
                SELECT_B => self.select(1)?,
                RECONNECT if !self.composing() => commands.push(DemoCommand::Reconnect),
                _ => {}
            }
        }
        self.sync()?;
        Ok(commands)
    }
    pub fn render(
        &mut self,
        scene: &mut Scene,
        text: &mut dyn ButtonTextRenderer,
        frame: Rect,
        clip: Rect,
        palette: Palette,
    ) -> std::result::Result<(), CompositionError> {
        self.tree.render(scene, text, frame, clip, palette)
    }
}
