//! Demo host layout for real Composer/Indicators, not a replacement RunView.
//! The absent structured Transcript is explicit until upstream #22/#23 land.
use std::{collections::BTreeMap, sync::Arc};
use tekes_agent_components_ui::{
    Composer, ComposerAffordances, ComposerCatalogPhase, ComposerChoiceCatalog,
    ComposerChoiceIntent, ComposerChoiceKind, ComposerChoiceRequest, ComposerChoices,
    ComposerEntry, ComposerIds, ComposerIntent, ComposerLabels, Indicators, RenameFailure,
    RenameIds, RenameIntent, RenameLabels, RenameSubmission, SidePanel, SidePanelInput,
    SidePanelIntent, SidePanelLabels, SidePanelTab, SidePanelTabKind, Sidebar, SidebarInput,
    SidebarIntent, SidebarLabels, SummaryInput, SummaryIntent, SummaryLabels, SummaryView,
    ThreadRename,
};
use tekes_agent_contract::{
    PromptMode, Result, SessionAddress, ThreadActionOutcome, require, unsupported,
};
use tekes_agent_presentation::{
    ChildHistoryLayout, ComposerDestination, PreparedSubmission, RunScope, SideChat,
    SideChatSnapshot, SummaryAction, TaskLoadToken,
};
use tekes_agent_ui_bindings::{
    BoundSideChatRun, ChildPaneBinding, ComposerSettingsRequest, MODEL_SELECTION_SETTING,
    ModelChoices, PaneAccess, PaneSnapshot, RunBinding, SideChatBinding, SidebarPublication,
    TaskPaneBinding,
};
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
const EMPTY_MAIN: ComponentId = ComponentId(9);
const SIDEBAR_NAMESPACE: u32 = 40;
const PANEL_NAMESPACE: u32 = 41;
const RENAME_NAMESPACE: u32 = 42;
const CHOICES_NAMESPACE: u32 = 43;
pub const SELECT_A: ComponentId = ComponentId(100);
pub const SELECT_B: ComponentId = ComponentId(101);
pub const RECONNECT: ComponentId = ComponentId(102);

pub enum DemoCommand {
    ReadModels {
        binding: Arc<RunBinding>,
        request: ComposerChoiceRequest,
    },
    Rename(RenameSubmission),
    OpenChild {
        request: PanelRequest,
        direct_parent: RunScope,
        child_id: String,
        title: String,
        layout: ChildHistoryLayout,
    },
    ReadChild {
        panel_parent: RunScope,
        source: Arc<ChildPaneBinding>,
        page: bool,
    },
    OpenTask {
        parent: RunScope,
        task_id: String,
        source: Arc<TaskPaneBinding>,
    },
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
    selection_cleared: bool,
    cleared_panel: Option<Content>,
    retained: Vec<Option<Content>>,
    navigation_revision: u64,
    sidebar_source: Option<Arc<SidebarPublication>>,
    sidebar_presented: Option<SidebarInput>,
    panel_revision: u64,
    side_chats: Option<Arc<SideChatBinding>>,
    panel_selections: BTreeMap<RunScope, SidePanelTabKind>,
    panel_runs: BTreeMap<(RunScope, String), PanelRun>,
    task_sources: BTreeMap<(RunScope, String), TaskSource>,
    task_runs: BTreeMap<(RunScope, String), PanelRun>,
    task_attempts: BTreeMap<(RunScope, String), RunScope>,
    child_panes: BTreeMap<(RunScope, String), ChildPane>,
    background_commands: Vec<DemoCommand>,
    next_panel_namespace: u32,
    panel_navigation_revision: u64,
    model_gesture: Option<ModelGesture>,
}
struct ModelGesture {
    request: ComposerChoiceRequest,
    settings: Option<ComposerSettingsRequest>,
    binding: Arc<RunBinding>,
    source: Option<ModelChoices>,
    navigation: u64,
    panel_navigation: u64,
}
struct PanelRun {
    binding: Arc<RunBinding>,
    ids: ComposerIds,
}
struct TaskSource {
    source: Arc<TaskPaneBinding>,
    token: Option<TaskLoadToken>,
}
struct ChildPane {
    source: Arc<ChildPaneBinding>,
    controls: PanelRun,
    title: String,
    snapshot: PaneSnapshot,
    refresh_key: Option<tekes_agent_ui_bindings::ChildRefreshKey>,
    reading: bool,
    read_error: Option<String>,
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
    created: bool,
}
#[derive(Clone, Debug)]
pub struct SidebarActionToken {
    address: SessionAddress,
    action: String,
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
        if let Some(child) = self.children.first_mut() {
            child.measure(text, Size::new(width, available.height));
        }
        MeasuredChild::new(Size::new(width, available.height))
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        if let Some(child) = self.children.first_mut() {
            child.arrange(text, frame, clip);
        }
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
        let rename = ThreadRename::new(RENAME_NAMESPACE, RenameLabels::default());
        let choices = ComposerChoices::new(CHOICES_NAMESPACE, Default::default());
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
                    Content::new(rename.ids().root, rename),
                    Content::new(choices.id(), choices),
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
            selection_cleared: false,
            cleared_panel: None,
            retained,
            navigation_revision: 0,
            sidebar_source: None,
            sidebar_presented: None,
            panel_revision: 0,
            side_chats: None,
            panel_selections: BTreeMap::new(),
            panel_runs: BTreeMap::new(),
            task_sources: BTreeMap::new(),
            task_runs: BTreeMap::new(),
            task_attempts: BTreeMap::new(),
            child_panes: BTreeMap::new(),
            model_gesture: None,
            background_commands: vec![],
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
    fn choices_id() -> ComponentId {
        ComponentId((CHOICES_NAMESPACE as u64) << 32)
    }
    pub fn model_choice_id(&self, choice: &str) -> Option<ComponentId> {
        self.tree
            .get(Self::choices_id())?
            .component::<ComposerChoices>()?
            .choice_id(choice)
    }
    pub fn model_choice_request(&self) -> Option<&ComposerChoiceRequest> {
        self.tree
            .get(Self::choices_id())?
            .component::<ComposerChoices>()?
            .request()
    }
    fn cancel_model_choices(&mut self) {
        if let Some(gesture) = self.model_gesture.take() {
            self.tree
                .get_mut(Self::choices_id())
                .unwrap()
                .component_mut::<ComposerChoices>()
                .unwrap()
                .dismiss(&gesture.request);
        }
    }
    fn model_gesture_current(&self, gesture: &ModelGesture) -> bool {
        !self.selection_cleared
            && gesture.navigation == self.navigation_revision
            && gesture.panel_navigation == self.panel_navigation_revision
            && gesture.binding.access() == PaneAccess::Ready
            && !self.composing()
    }
    fn begin_model_choices(
        &mut self,
        binding: Arc<RunBinding>,
        destination: ComposerDestination,
    ) -> Result<DemoCommand> {
        require(
            !self.composing(),
            "finish composition before opening model choices",
        )?;
        require(
            binding.supports_model_choices(),
            "model choices unavailable",
        )?;
        self.cancel_model_choices();
        let settings = binding.begin_settings(&destination)?;
        let request = self
            .tree
            .get_mut(Self::choices_id())
            .unwrap()
            .component_mut::<ComposerChoices>()
            .unwrap()
            .open(destination, ComposerChoiceKind::Model)
            .map_err(unsupported)?;
        self.model_gesture = Some(ModelGesture {
            request: request.clone(),
            settings: Some(settings),
            binding: binding.clone(),
            source: None,
            navigation: self.navigation_revision,
            panel_navigation: self.panel_navigation_revision,
        });
        Ok(DemoCommand::ReadModels { binding, request })
    }
    pub fn complete_model_catalog(
        &mut self,
        request: &ComposerChoiceRequest,
        result: std::result::Result<ModelChoices, String>,
    ) -> Result<bool> {
        let Some(gesture) = self.model_gesture.as_ref() else {
            return Ok(false);
        };
        if &gesture.request != request {
            return Ok(false);
        }
        if !self.model_gesture_current(gesture) {
            self.cancel_model_choices();
            return Ok(false);
        }
        let (catalog, source) = match result {
            Ok(mut source) => {
                if let Some(snapshot) = gesture.binding.current_run()
                    && let Some(value) = snapshot.draft.settings.get(MODEL_SELECTION_SETTING)
                    && let (Some(provider), Some(model)) =
                        (value["provider"].as_str(), value["model"].as_str())
                {
                    let id = serde_json::json!([provider, model]).to_string();
                    source.catalog.selected = Some(id.clone());
                    if let Some(effort) = value["reasoningEffort"].as_str()
                        && let Some(payload) = source.selections.get_mut(&id)
                    {
                        payload["reasoningEffort"] = serde_json::json!(effort);
                    }
                }
                (source.catalog.clone(), Some(source))
            }
            Err(error) => (
                ComposerChoiceCatalog {
                    revision: 1,
                    phase: ComposerCatalogPhase::Failed(error),
                    choices: vec![],
                    selected: None,
                },
                None,
            ),
        };
        let accepted = self
            .tree
            .get_mut(Self::choices_id())
            .unwrap()
            .component_mut::<ComposerChoices>()
            .unwrap()
            .update(request, catalog)
            .map_err(unsupported)?;
        if accepted {
            if let Some(failures) = source
                .as_ref()
                .and_then(|source| source.raw.get("failures"))
                .and_then(serde_json::Value::as_array)
                && !failures.is_empty()
            {
                self.set_notice(format!("部分供应商目录读取失败（{} 项）", failures.len()));
            }
            self.model_gesture.as_mut().unwrap().source = source;
        }
        Ok(accepted)
    }
    fn drain_model_choices(&mut self, actions: &[ContentAction]) -> Result<()> {
        let intents = self
            .tree
            .get_mut(Self::choices_id())
            .unwrap()
            .component_mut::<ComposerChoices>()
            .unwrap()
            .take_intents(actions);
        for intent in intents {
            match intent {
                ComposerChoiceIntent::Dismiss(_) => {
                    self.model_gesture = None;
                }
                ComposerChoiceIntent::Select(selection) => {
                    let Some(mut gesture) = self.model_gesture.take() else {
                        continue;
                    };
                    if gesture.request != selection.request || !self.model_gesture_current(&gesture)
                    {
                        self.tree
                            .get_mut(Self::choices_id())
                            .unwrap()
                            .component_mut::<ComposerChoices>()
                            .unwrap()
                            .dismiss(&gesture.request);
                        continue;
                    }
                    let result = (|| {
                        let source = gesture
                            .source
                            .as_ref()
                            .ok_or_else(|| unsupported("model catalog missing"))?;
                        require(
                            source.catalog.revision == selection.catalog_revision,
                            "model catalog revision changed",
                        )?;
                        let payload = source.selection(&selection.choice.id)?;
                        let mut settings = gesture
                            .binding
                            .current_run()
                            .ok_or_else(|| unsupported("model source missing"))?
                            .draft
                            .settings
                            .clone();
                        settings.insert(MODEL_SELECTION_SETTING.into(), payload);
                        let request = match gesture.settings.take() {
                            Some(request) => request,
                            None => gesture
                                .binding
                                .begin_settings(&selection.request.destination)?,
                        };
                        require(request.commit(settings)?, "model settings request obsolete")
                    })();
                    let failed = result.is_err();
                    self.tree
                        .get_mut(Self::choices_id())
                        .unwrap()
                        .component_mut::<ComposerChoices>()
                        .unwrap()
                        .complete(&selection, result.map_err(|e| e.to_string()));
                    if failed {
                        self.model_gesture = Some(gesture);
                    }
                }
            }
        }
        Ok(())
    }
    pub fn binding(&self) -> &Arc<RunBinding> {
        &self.bindings[self.selected]
    }
    pub fn current_binding(&self) -> Option<&Arc<RunBinding>> {
        (!self.selection_cleared).then(|| self.binding())
    }
    pub fn rename_ids(&self) -> Option<RenameIds> {
        let rename = self
            .tree
            .get(RenameIds::new(RENAME_NAMESPACE).root)?
            .component::<ThreadRename>()?;
        rename.is_open().then(|| rename.ids())
    }
    pub fn complete_rename(
        &mut self,
        ticket: &RenameSubmission,
        result: std::result::Result<(), RenameFailure>,
    ) -> Result<bool> {
        let root = RenameIds::new(RENAME_NAMESPACE).root;
        let current = self
            .tree
            .get(root)
            .unwrap()
            .component::<ThreadRename>()
            .unwrap()
            .is_current(ticket);
        if current && result.is_ok() {
            self.tree.focus(None, false);
        }
        let applied = self
            .tree
            .get_mut(root)
            .unwrap()
            .component_mut::<ThreadRename>()
            .unwrap()
            .complete(ticket, result);
        self.sync()?;
        Ok(applied)
    }
    pub fn begin_sidebar_action(
        &mut self,
        address: SessionAddress,
        action: String,
    ) -> SidebarActionToken {
        if action == "fork" {
            self.navigation_revision += 1;
        }
        SidebarActionToken {
            address,
            action,
            revision: self.navigation_revision,
        }
    }
    pub fn finish_sidebar_action(&mut self, token: &SidebarActionToken) {
        let id = self.sidebar_id();
        self.tree
            .get_mut(id)
            .unwrap()
            .component_mut::<Sidebar>()
            .unwrap()
            .set_pending(&token.address, &token.action, false);
    }
    pub fn apply_sidebar_outcome(
        &mut self,
        token: &SidebarActionToken,
        outcome: &ThreadActionOutcome,
    ) -> Result<Option<NavigationRequest>> {
        if let Some(warning) = &outcome.warning {
            self.set_notice(warning);
        }
        if token.revision != self.navigation_revision || self.composing() {
            return Ok(None);
        }
        if outcome.clear_selection
            && self.current_binding().is_some_and(|binding| {
                binding.scope().endpoint_id == token.address.endpoint_id
                    && binding.scope().session_id == token.address.session_id
            })
        {
            self.clear_selection()?;
        }
        if let Some(navigation) = &outcome.navigation {
            return Ok(Some(NavigationRequest {
                address: SessionAddress {
                    endpoint_id: navigation.endpoint.clone(),
                    session_id: navigation.session_id.clone(),
                },
                revision: token.revision,
                created: true,
            }));
        }
        Ok(None)
    }
    fn clear_selection(&mut self) -> Result<()> {
        if self.selection_cleared {
            return Ok(());
        }
        self.cancel_model_choices();
        self.tree.focus(None, false);
        self.retained[self.selected] = Some(
            self.tree
                .remove(Self::pair_id(self.selected))
                .map_err(|_| unsupported("detach selected run"))?,
        );
        self.cleared_panel = Some(
            self.tree
                .remove(self.panel_id())
                .map_err(|_| unsupported("detach selected panel"))?,
        );
        self.tree
            .insert(
                MAIN,
                Content::new(EMPTY_MAIN, TextContent::new("选择会话以继续")),
            )
            .map_err(|_| unsupported("mount empty selection"))?;
        self.selection_cleared = true;
        self.navigation_revision += 1;
        self.sync()
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
        self.active_panel_run()
            .map(|run| run.ids)
            .filter(|ids| self.tree.get(ids.root).is_some())
    }
    pub fn panel_child_controls_ready(&self) -> bool {
        let Some(binding) = self.current_binding() else {
            return false;
        };
        let parent = binding.scope();
        let Some(SidePanelTabKind::Subagent { child_id }) = self.panel_selections.get(parent)
        else {
            return false;
        };
        self.child_panes
            .get(&(parent.clone(), child_id.clone()))
            .is_some_and(|pane| !pane.reading && pane.read_error.is_none())
            && self.panel_composer_ids().is_some()
    }
    pub fn bounded_child_buttons(&self) -> Option<(ComponentId, ComponentId)> {
        let parent = self.binding().scope();
        let SidePanelTabKind::Subagent { child_id } = self.panel_selections.get(parent)? else {
            return None;
        };
        let pane = self.child_panes.get(&(parent.clone(), child_id.clone()))?;
        if !matches!(pane.snapshot, PaneSnapshot::ReadOnlyChild { .. }) {
            return None;
        }
        let refresh = ComponentId(pane.controls.ids.root.0 + 19);
        self.tree
            .get(refresh)
            .map(|_| (refresh, ComponentId(pane.controls.ids.root.0 + 20)))
    }
    fn active_panel_run(&self) -> Option<&PanelRun> {
        let parent = self.binding().scope();
        match self.panel_selections.get(parent)? {
            SidePanelTabKind::SideChat { session_id } => {
                self.panel_runs.get(&(parent.clone(), session_id.clone()))
            }
            SidePanelTabKind::Task { task_id } => {
                self.task_runs.get(&(parent.clone(), task_id.clone()))
            }
            SidePanelTabKind::Subagent { child_id } => self
                .child_panes
                .get(&(parent.clone(), child_id.clone()))
                .map(|pane| &pane.controls),
            _ => None,
        }
    }
    pub fn take_background_commands(&mut self) -> Vec<DemoCommand> {
        std::mem::take(&mut self.background_commands)
    }
    pub fn begin_child_open(&mut self, panel_parent: &RunScope) -> Result<PanelRequest> {
        require(
            !self.composing() && self.binding().scope() == panel_parent,
            "child navigation parent changed or composing",
        )?;
        Ok(self.panel_request(panel_parent.clone()))
    }
    pub fn child_request_current(&self, request: &PanelRequest) -> bool {
        !self.selection_cleared
            && self.binding().scope() == &request.parent
            && self.navigation_revision == request.main_revision
            && self.panel_navigation_revision == request.panel_revision
            && !self.composing()
    }
    pub fn child_pane_active(&self, panel_parent: &RunScope, child: &str) -> bool {
        !self.selection_cleared
            && self.binding().scope() == panel_parent
            && self.panel_selections.get(panel_parent)
                == Some(&SidePanelTabKind::Subagent {
                    child_id: child.into(),
                })
    }
    pub fn cached_child_source(
        &self,
        panel_parent: &RunScope,
        direct_parent: &RunScope,
        child_id: &str,
        layout: ChildHistoryLayout,
    ) -> Option<Arc<ChildPaneBinding>> {
        self.child_panes
            .get(&(panel_parent.clone(), child_id.into()))
            .filter(|pane| {
                pane.source.parent() == direct_parent
                    && pane.source.layout() == layout
                    && pane.source.binding().access() != PaneAccess::Missing
            })
            .map(|pane| pane.source.clone())
    }
    pub fn install_child_source(
        &mut self,
        request: PanelRequest,
        source: Arc<ChildPaneBinding>,
        title: String,
        snapshot: PaneSnapshot,
    ) -> Result<bool> {
        let panel_parent = request.parent.clone();
        let parent = self
            .bindings
            .iter()
            .find(|binding| binding.scope() == &panel_parent)
            .ok_or_else(|| unsupported("child panel parent not mounted"))?;
        require(
            source.uses_run(parent) && source.parent().thread_id == panel_parent.thread_id,
            "child source authority mismatch",
        )?;
        Self::validate_child_snapshot(&source, &snapshot)?;
        let key = (panel_parent.clone(), source.child_id().to_owned());
        if self.composing()
            && self.binding().scope() == &panel_parent
            && self.panel_selections.get(&panel_parent)
                == Some(&SidePanelTabKind::Subagent {
                    child_id: source.child_id().into(),
                })
            && self
                .child_panes
                .get(&key)
                .is_some_and(|pane| !Arc::ptr_eq(&pane.source, &source))
        {
            return Ok(false);
        }
        let same = self
            .child_panes
            .get(&key)
            .is_some_and(|pane| Arc::ptr_eq(&pane.source, &source));
        if same {
            self.child_panes.get_mut(&key).unwrap().title = title;
        } else {
            if let Some(old) = self.child_panes.remove(&key) {
                old.source.retire();
            }
            let ids = self.allocate_panel_ids()?;
            self.child_panes.insert(
                key,
                ChildPane {
                    controls: PanelRun {
                        binding: source.binding().clone(),
                        ids,
                    },
                    source: source.clone(),
                    title,
                    snapshot,
                    refresh_key: None,
                    reading: false,
                    read_error: None,
                },
            );
        }
        // Installing a result for an inactive parent caches its body without
        // taking over the active parent's navigation or focus.
        let selected = self.child_request_current(&request);
        if selected {
            self.panel_selections.insert(
                panel_parent,
                SidePanelTabKind::Subagent {
                    child_id: source.child_id().into(),
                },
            );
        }
        self.sync()?;
        Ok(selected)
    }
    fn validate_child_snapshot(source: &ChildPaneBinding, snapshot: &PaneSnapshot) -> Result<()> {
        require(
            source.binding().access() != PaneAccess::Missing,
            "child source retired",
        )?;
        let valid = match snapshot {
            PaneSnapshot::Run { snapshot, .. }
                if source.layout() == ChildHistoryLayout::IndependentSessions =>
            {
                snapshot
                    .run
                    .as_ref()
                    .is_some_and(|run| &run.scope == source.binding().scope())
            }
            PaneSnapshot::ReadOnlyChild { scope, .. }
                if source.layout() == ChildHistoryLayout::SameSession =>
            {
                scope == source.binding().scope()
            }
            _ => false,
        };
        require(valid, "child snapshot source mismatch")
    }
    pub fn update_child_snapshot(
        &mut self,
        panel_parent: &RunScope,
        source: &Arc<ChildPaneBinding>,
        snapshot: PaneSnapshot,
    ) -> Result<bool> {
        let Some(pane) = self
            .child_panes
            .get_mut(&(panel_parent.clone(), source.child_id().into()))
        else {
            return Ok(false);
        };
        if !Arc::ptr_eq(&pane.source, source) || source.binding().access() == PaneAccess::Missing {
            return Ok(false);
        }
        Self::validate_child_snapshot(source, &snapshot)?;
        pane.snapshot = snapshot;
        pane.reading = false;
        pane.read_error = None;
        self.sync()?;
        Ok(true)
    }
    pub fn child_read_failed(
        &mut self,
        panel_parent: &RunScope,
        source: &Arc<ChildPaneBinding>,
        error: String,
    ) -> Result<()> {
        if let Some(pane) = self
            .child_panes
            .get_mut(&(panel_parent.clone(), source.child_id().into()))
            && Arc::ptr_eq(&pane.source, source)
        {
            pane.reading = false;
            pane.read_error = Some(error);
        }
        self.sync()
    }
    /// Explicit host navigation for task rows supplied by its authoritative
    /// loader. Summary todo rows deliberately have no fabricated OpenTask action.
    pub fn open_task_source(&mut self, source: Arc<TaskPaneBinding>) -> Result<()> {
        require(!self.composing(), "finish composition before opening task")?;
        let state = source.snapshot();
        let parent = state
            .parent_scope()
            .ok_or_else(|| unsupported("task parent unresolved"))?
            .clone();
        let task_id = state
            .active_task_id()
            .ok_or_else(|| unsupported("task identity unresolved"))?
            .to_owned();
        let binding = self
            .bindings
            .iter()
            .find(|binding| binding.scope() == &parent)
            .ok_or_else(|| unsupported("task parent not mounted"))?;
        require(source.uses_run(binding), "task runtime authority mismatch")?;
        let key = (parent.clone(), task_id.clone());
        if let Some(old) = self.task_sources.get(&key)
            && !Arc::ptr_eq(&old.source, &source)
        {
            old.source.cancel();
            self.task_runs.remove(&key);
        }
        self.task_sources.insert(
            key.clone(),
            TaskSource {
                source,
                token: None,
            },
        );
        self.task_attempts.remove(&key);
        self.panel_selections
            .insert(parent, SidePanelTabKind::Task { task_id });
        self.panel_navigation_revision += 1;
        self.tree.focus(None, false);
        self.sync()
    }
    pub fn install_task_run(
        &mut self,
        parent: RunScope,
        task_id: String,
        run: Arc<RunBinding>,
    ) -> Result<bool> {
        let key = (parent, task_id);
        let Some(source) = self.task_sources.get(&key) else {
            return Ok(false);
        };
        if !source.source.owns_run(&run)
            || run.access() == PaneAccess::Missing
            || source
                .source
                .snapshot()
                .value
                .as_ref()
                .map(|task| &task.scope)
                != Some(run.scope())
        {
            return Ok(false);
        }
        if self
            .task_runs
            .get(&key)
            .is_none_or(|old| !Arc::ptr_eq(&old.binding, &run))
        {
            let ids = self.allocate_panel_ids()?;
            self.task_runs.insert(key, PanelRun { binding: run, ids });
        }
        self.sync()?;
        Ok(true)
    }
    fn allocate_panel_ids(&mut self) -> Result<ComposerIds> {
        let namespace = self.next_panel_namespace;
        self.next_panel_namespace = namespace
            .checked_add(1)
            .filter(|next| *next <= 65_535)
            .ok_or_else(|| unsupported("demo pane namespaces exhausted"))?;
        Ok(ComposerIds::new((namespace as u64) << 32))
    }
    fn composing_editor(&self) -> Option<ComponentId> {
        let rename = self
            .tree
            .get(RenameIds::new(RENAME_NAMESPACE).root)?
            .component::<ThreadRename>()?;
        if rename.is_composing() {
            return Some(rename.ids().editor);
        }
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
            let ids = self.allocate_panel_ids()?;
            self.panel_runs.insert(
                key,
                PanelRun {
                    binding: run.binding().clone(),
                    ids,
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
    fn sync_sidebar(&mut self) -> Result<()> {
        if let Some(source) = &self.sidebar_source {
            let selected = self.current_binding().map(|binding| SessionAddress {
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
        Ok(())
    }
    pub fn sync(&mut self) -> Result<()> {
        if self
            .model_gesture
            .as_ref()
            .is_some_and(|gesture| !self.model_gesture_current(gesture))
        {
            self.cancel_model_choices();
        }
        if self.selection_cleared {
            self.sync_sidebar()?;
            for id in [SELECT_A, SELECT_B] {
                let disabled = self.composing();
                let button = &mut self
                    .tree
                    .get_mut(id)
                    .unwrap()
                    .component_mut::<ButtonContent>()
                    .unwrap()
                    .button;
                button.state.selected = false;
                button.state.disabled = disabled;
            }
            self.tree
                .get_mut(DIAGNOSTIC)
                .unwrap()
                .component_mut::<TextContent>()
                .unwrap()
                .text = "未选择会话".into();
            return Ok(());
        }
        let binding = self.binding().clone();
        let snapshot = binding
            .current_run()
            .ok_or_else(|| unsupported("demo needs a present main run"))?;
        let ids = self.ids();
        self.tree
            .get_mut(ids.root)
            .unwrap()
            .component_mut::<Composer>()
            .unwrap()
            .set_labels(Self::composer_labels(&snapshot));
        binding.sync_composer(
            self.tree
                .get_mut(ids.root)
                .expect("composer")
                .component_mut::<Composer>()
                .expect("Composer"),
            ComposerAffordances {
                model: binding.supports_model_choices(),
                ..Default::default()
            },
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
        self.sync_sidebar()?;
        let panel_id = self.panel_id();
        let parent = binding.scope().clone();
        // Resolve task rows from each original parent's canonical SummaryFacts,
        // including inactive parents. A missing/error row revokes old callbacks.
        self.task_sources.retain(|(parent, id), task| {
            let state = task.source.snapshot();
            state.parent_scope() == Some(parent) && state.active_task_id() == Some(id.as_str())
        });
        for ((parent, task_id), task) in &self.task_sources {
            let Some(token) = &task.token else {
                continue;
            };
            let facts = self
                .bindings
                .iter()
                .find(|binding| binding.scope() == parent)
                .and_then(|binding| binding.current_run())
                .and_then(|snapshot| snapshot.summary.clone());
            let result = facts
                .map(|facts| facts.tasks.into_iter().find(|row| &row.id == task_id))
                .ok_or_else(|| "Task projection unavailable".into());
            task.source.commit(token, result);
        }
        self.task_runs
            .retain(|_, run| run.binding.access() != PaneAccess::Missing);
        self.task_attempts.retain(|key, _| {
            self.task_sources
                .get(key)
                .is_some_and(|source| source.source.snapshot().value.is_some())
        });
        let mut tabs = vec![SidePanelTab::summary("Summary")];
        for ((task_parent, _), task) in &self.task_sources {
            if task_parent == &parent {
                tabs.push(task.source.tab("Task")?);
            }
        }
        self.child_panes
            .retain(|_, pane| pane.source.binding().access() != PaneAccess::Missing);
        for ((panel_parent, _), pane) in &mut self.child_panes {
            if panel_parent == &parent {
                tabs.push(pane.source.tab(pane.title.clone()));
            }
            if let Some(key) = pane.source.refresh_key()
                && pane.refresh_key != Some(key)
                && !pane.reading
            {
                pane.refresh_key = Some(key);
                pane.reading = true;
                self.background_commands.push(DemoCommand::ReadChild {
                    panel_parent: panel_parent.clone(),
                    source: pane.source.clone(),
                    page: false,
                });
            }
        }
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
            owner.input(self.panel_revision, &parent, tabs, Some(wanted))?
        } else {
            let selected = tabs.iter().any(|tab| tab.kind == wanted).then_some(wanted);
            SidePanelInput {
                revision: self.panel_revision,
                parent: parent.clone(),
                tabs,
                selected,
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
                let key = (parent, session_id.clone());
                if let Some(run) = self.panel_runs.get(&key) {
                    self.sync_pane_controls(
                        SidePanelTabKind::SideChat { session_id },
                        run.ids,
                        run.binding.clone(),
                    )?;
                }
            }
            SidePanelTabKind::Task { task_id } => {
                let key = (parent.clone(), task_id.clone());
                if let Some(run) = self.task_runs.get(&key) {
                    self.sync_pane_controls(
                        SidePanelTabKind::Task { task_id },
                        run.ids,
                        run.binding.clone(),
                    )?;
                } else if let Some(source) = self.task_sources.get(&key)
                    && let Some(task) = source.source.snapshot().value
                    && self.task_attempts.get(&key) != Some(&task.scope)
                {
                    self.task_attempts.insert(key, task.scope);
                    self.background_commands.push(DemoCommand::OpenTask {
                        parent,
                        task_id,
                        source: source.source.clone(),
                    });
                }
            }
            SidePanelTabKind::Subagent { child_id } => {
                if let Some(pane) = self.child_panes.get(&(parent, child_id.clone())) {
                    let ids = pane.controls.ids;
                    let binding = pane.controls.binding.clone();
                    let reading = pane.reading;
                    let read_error = pane.read_error.clone();
                    let paging = pane.source.supports_pagination();
                    match &pane.snapshot {
                        PaneSnapshot::Run { .. } => {
                            self.sync_pane_controls(
                                SidePanelTabKind::Subagent { child_id },
                                ids,
                                binding.clone(),
                            )?;
                            if reading && let Some(snapshot) = binding.current_run() {
                                self.tree
                                    .get_mut(ids.root)
                                    .unwrap()
                                    .component_mut::<Composer>()
                                    .unwrap()
                                    .update_gated(
                                        &snapshot,
                                        ComposerAffordances::default(),
                                        tekes_agent_components_ui::ComposerControlGate::CLOSED,
                                    )
                                    .map_err(unsupported)?;
                            }
                            if self.tree.get(ComponentId(ids.root.0 + 19)).is_none() {
                                self.tree
                                    .get_mut(ComponentId(ids.root.0 + 16))
                                    .unwrap()
                                    .component_mut::<StackContent>()
                                    .unwrap()
                                    .children
                                    .push(Content::new(
                                        ComponentId(ids.root.0 + 19),
                                        ButtonContent::new("刷新子代理能力"),
                                    ));
                            }
                            self.tree
                                .get_mut(ComponentId(ids.root.0 + 19))
                                .unwrap()
                                .component_mut::<ButtonContent>()
                                .unwrap()
                                .button
                                .state
                                .disabled = reading;
                        }
                        PaneSnapshot::ReadOnlyChild { transcript, .. } => {
                            let mut text = format!(
                                "有界子历史：{} 项；{}。只读。\nTranscript 待 TekesUI #22 / #23。",
                                transcript.items.len(),
                                if transcript.is_stale || read_error.is_some() {
                                    "历史待同步"
                                } else {
                                    "历史已同步"
                                }
                            );
                            if let Some(error) = read_error {
                                text.push_str(&format!("\n读取失败，保留已加载历史：{error}"));
                            }
                            let more = transcript.has_more_before;
                            if self.tree.get(ComponentId(ids.root.0 + 18)).is_none() {
                                let panel = self
                                    .tree
                                    .get_mut(panel_id)
                                    .unwrap()
                                    .component_mut::<SidePanel>()
                                    .unwrap();
                                let token = panel
                                    .content_token(&SidePanelTabKind::Subagent { child_id })
                                    .ok_or_else(|| unsupported("bounded child token missing"))?;
                                panel
                                    .install_content(
                                        &token,
                                        Content::new(
                                            ComponentId(ids.root.0 + 16),
                                            StackContent {
                                                model: Stack {
                                                    axis: Axis::Vertical,
                                                    gap: Spacing::Regular.value(),
                                                },
                                                flexible_child: None,
                                                children: vec![
                                                    Content::new(
                                                        ComponentId(ids.root.0 + 18),
                                                        TextContent::new(text.clone()),
                                                    ),
                                                    Content::new(
                                                        ComponentId(ids.root.0 + 19),
                                                        ButtonContent::new("刷新子历史"),
                                                    ),
                                                    Content::new(
                                                        ComponentId(ids.root.0 + 20),
                                                        ButtonContent::new("加载更早子历史"),
                                                    ),
                                                ],
                                            },
                                        ),
                                    )
                                    .map_err(unsupported)?;
                            }
                            self.tree
                                .get_mut(ComponentId(ids.root.0 + 18))
                                .unwrap()
                                .component_mut::<TextContent>()
                                .unwrap()
                                .text = text;
                            self.tree
                                .get_mut(ComponentId(ids.root.0 + 20))
                                .unwrap()
                                .component_mut::<ButtonContent>()
                                .unwrap()
                                .button
                                .state
                                .disabled = reading || !more || !paging;
                            self.tree
                                .get_mut(ComponentId(ids.root.0 + 19))
                                .unwrap()
                                .component_mut::<ButtonContent>()
                                .unwrap()
                                .button
                                .state
                                .disabled = reading;
                        }
                        PaneSnapshot::Missing { .. } => {}
                    }
                }
            }
        }
        Ok(())
    }
    fn sync_pane_controls(
        &mut self,
        kind: SidePanelTabKind,
        ids: ComposerIds,
        binding: Arc<RunBinding>,
    ) -> Result<()> {
        let Some(snapshot) = binding.current_run() else {
            return Ok(());
        };
        if self.tree.get(ids.root).is_none() {
            let panel_id = self.panel_id();
            let panel = self
                .tree
                .get_mut(panel_id)
                .expect("panel")
                .component_mut::<SidePanel>()
                .expect("SidePanel");
            let token = panel
                .content_token(&kind)
                .ok_or_else(|| unsupported("pane content source unavailable"))?;
            panel
                .install_content(&token, Self::run_controls(ids))
                .map_err(unsupported)?;
        }
        self.tree
            .get_mut(ids.root)
            .expect("pane composer")
            .component_mut::<Composer>()
            .expect("Composer")
            .set_labels(Self::composer_labels(&snapshot));
        self.tree
            .get_mut(ids.root)
            .expect("pane composer")
            .component_mut::<Composer>()
            .expect("Composer")
            .update_gated(
                &snapshot,
                ComposerAffordances {
                    model: binding.supports_model_choices(),
                    ..Default::default()
                },
                binding.access().composer_gate(),
            )
            .map_err(unsupported)?;
        self.tree
            .get_mut(ComponentId(ids.root.0 + 17))
            .expect("pane indicators")
            .component_mut::<Indicators>()
            .expect("Indicators")
            .update(&snapshot)
            .map_err(unsupported)?;
        Ok(())
    }
    fn composer_labels(snapshot: &tekes_agent_presentation::RunSnapshot) -> ComposerLabels {
        let mut labels = ComposerLabels::default();
        if let Some(model) = snapshot
            .draft
            .settings
            .get(MODEL_SELECTION_SETTING)
            .and_then(|value| value.get("model"))
            .and_then(serde_json::Value::as_str)
        {
            labels.model = format!("模型 · {model}（发送时应用）");
        }
        labels
    }
    pub fn select(&mut self, index: usize) -> Result<()> {
        require(index < self.bindings.len(), "unknown demo session")?;
        require(
            !self.composing(),
            "finish or cancel composition before switching sessions",
        )?;
        self.cancel_model_choices();
        if self.selection_cleared {
            self.tree
                .remove(EMPTY_MAIN)
                .map_err(|_| unsupported("remove empty selection"))?;
            let pair = self.retained[self.selected]
                .take()
                .expect("retained archived run");
            self.tree
                .insert(MAIN, pair)
                .map_err(|_| unsupported("restore selected controls"))?;
            self.tree
                .insert(
                    PANEL_COLUMN,
                    self.cleared_panel.take().expect("retained panel"),
                )
                .map_err(|_| unsupported("restore selected panel"))?;
            self.selection_cleared = false;
        }
        self.navigation_revision = self
            .navigation_revision
            .checked_add(1)
            .ok_or_else(|| unsupported("navigation revision exhausted"))?;
        if index == self.selected {
            self.sync()?;
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
            .reorder(
                MAIN,
                &[
                    NAV,
                    NOTICE,
                    DIAGNOSTIC,
                    RenameIds::new(RENAME_NAMESPACE).root,
                    Self::choices_id(),
                    Self::pair_id(index),
                ],
            )
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
        if self.composing() || (disappeared && !request.created) {
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
        let mut composers = if self.selection_cleared {
            vec![]
        } else {
            vec![(ids, binding.clone())]
        };
        if let Some(ids) = self.panel_composer_ids()
            && let Some(run) = self.active_panel_run()
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
        self.drain_model_choices(&actions)?;
        let rename_root = RenameIds::new(RENAME_NAMESPACE).root;
        let rename_intents = self
            .tree
            .get_mut(rename_root)
            .unwrap()
            .component_mut::<ThreadRename>()
            .unwrap()
            .take_intents(&actions);
        for intent in rename_intents {
            match intent {
                RenameIntent::Submit(ticket) => commands.push(DemoCommand::Rename(*ticket)),
                RenameIntent::Dismiss => {
                    self.tree.focus(None, false);
                }
            }
        }
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
            .and_then(|node| node.component::<SidePanel>())
            .map(|panel| panel.take_intents(&actions))
            .unwrap_or_default();
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
                        created: false,
                    }));
                }
            } else {
                if let SidebarIntent::Action {
                    address,
                    definition,
                } = &intent
                {
                    if self
                        .tree
                        .get(self.sidebar_id())
                        .unwrap()
                        .component::<Sidebar>()
                        .unwrap()
                        .is_pending(address, &definition.id)
                    {
                        continue;
                    }
                    if definition.interaction == "rename" {
                        let title = self
                            .sidebar_source
                            .as_ref()
                            .and_then(|source| {
                                source.rows.iter().find(|row| &row.address == address)
                            })
                            .map(|row| row.title.clone())
                            .unwrap_or_default();
                        self.tree.focus(None, false);
                        self.tree
                            .get_mut(rename_root)
                            .unwrap()
                            .component_mut::<ThreadRename>()
                            .unwrap()
                            .open(address.clone(), definition.clone(), &title)
                            .map_err(unsupported)?;
                        let editor = self.rename_ids().unwrap().editor;
                        self.tree.focus(Some(editor), true);
                        continue;
                    }
                    let sidebar_id = self.sidebar_id();
                    self.tree
                        .get_mut(sidebar_id)
                        .unwrap()
                        .component_mut::<Sidebar>()
                        .unwrap()
                        .set_pending(address, &definition.id, true);
                }
                commands.push(DemoCommand::Sidebar(intent));
            }
        }
        for intent in summary_intents {
            if let SummaryAction::OpenTask { task_id, .. } = &intent.action {
                require(&intent.scope == binding.scope(), "Summary parent changed")?;
                let key = (intent.scope.clone(), task_id.clone());
                if !self.task_sources.contains_key(&key) {
                    let source = Arc::new(TaskPaneBinding::for_run(&binding));
                    let token = source.begin(intent.scope.clone(), task_id.clone())?;
                    self.task_sources.insert(
                        key.clone(),
                        TaskSource {
                            source,
                            token: Some(token),
                        },
                    );
                }
                self.panel_selections.insert(
                    intent.scope.clone(),
                    SidePanelTabKind::Task {
                        task_id: task_id.clone(),
                    },
                );
                self.task_attempts.remove(&key);
                self.panel_navigation_revision += 1;
                self.tree.focus(None, false);
            } else {
                commands.push(DemoCommand::Summary(intent));
            }
        }
        for intent in panel_intents {
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
                        self.side_chats
                            .as_ref()
                            .ok_or_else(|| unsupported("SideChat owner unavailable"))?
                            .pointer(&parent, &session_id)?,
                    ));
                }
                SidePanelIntent::Close {
                    parent,
                    tab: SidePanelTabKind::Task { task_id },
                } => {
                    let key = (parent.clone(), task_id);
                    if let Some(source) = self.task_sources.remove(&key) {
                        source.source.cancel();
                    }
                    self.task_runs.remove(&key);
                    self.task_attempts.remove(&key);
                    self.panel_selections
                        .insert(parent, SidePanelTabKind::Summary);
                    self.panel_navigation_revision += 1;
                    self.tree.focus(None, false);
                }
                SidePanelIntent::Close {
                    parent,
                    tab: SidePanelTabKind::Subagent { child_id },
                } => {
                    if let Some(pane) = self.child_panes.remove(&(parent.clone(), child_id)) {
                        pane.source.retire();
                    }
                    self.panel_selections
                        .insert(parent, SidePanelTabKind::Summary);
                    self.panel_navigation_revision += 1;
                    self.tree.focus(None, false);
                }
                SidePanelIntent::Select { parent, tab } if &parent == self.binding().scope() => {
                    self.tree.focus(None, false);
                    self.panel_selections
                        .insert(parent.clone(), tab.kind.clone());
                    let request = self.panel_request(parent.clone());
                    if let SidePanelTabKind::Task { task_id } = &tab.kind {
                        self.task_attempts
                            .remove(&(parent.clone(), task_id.clone()));
                    }
                    if let SidePanelTabKind::SideChat { session_id } = tab.kind {
                        let chat = self
                            .side_chats
                            .as_ref()
                            .ok_or_else(|| unsupported("SideChat owner unavailable"))?
                            .pointer(&parent, &session_id)?;
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
                    if entry == ComposerEntry::Model {
                        commands.push(self.begin_model_choices(binding.clone(), destination)?);
                    } else {
                        commands.push(DemoCommand::OpenEntry { destination, entry })
                    }
                }
            }
        }
        for action in actions
            .iter()
            .filter(|action| action.kind == ContentActionKind::Activate)
        {
            if let Some(SidePanelTabKind::Subagent { child_id }) =
                self.panel_selections.get(binding.scope())
                && let Some(pane) = self
                    .child_panes
                    .get_mut(&(binding.scope().clone(), child_id.clone()))
                && !pane.reading
                && (action.target == ComponentId(pane.controls.ids.root.0 + 19)
                    || action.target == ComponentId(pane.controls.ids.root.0 + 20))
            {
                pane.reading = true;
                commands.push(DemoCommand::ReadChild {
                    panel_parent: binding.scope().clone(),
                    source: pane.source.clone(),
                    page: action.target == ComponentId(pane.controls.ids.root.0 + 20),
                });
            }
            match action.target {
                SELECT_A => self.select(0)?,
                SELECT_B => self.select(1)?,
                RECONNECT if !self.composing() => commands.push(DemoCommand::Reconnect),
                _ => {}
            }
        }
        self.sync()?;
        commands.extend(self.take_background_commands());
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
