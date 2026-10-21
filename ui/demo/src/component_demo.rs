//! Host window/navigation adapters around shared Agent components.
//! RunView is experimental; generic reading belongs to the published SDK.
use std::{collections::BTreeMap, sync::Arc};
use tekes_agent_components_ui::{
    AnnotationEditor, AnnotationEditorIds, AnnotationEditorIntent, Composer, ComposerAffordances,
    ComposerCatalogPhase, ComposerChoiceCatalog, ComposerChoiceIntent, ComposerChoiceKind,
    ComposerChoiceRequest, ComposerChoices, ComposerEntry, ComposerIds, ComposerIntent,
    ComposerLabels, RenameFailure, RenameIds, RenameIntent, RenameLabels, RenameSubmission,
    RunView, RunViewIds, SidePanel, SidePanelInput, SidePanelIntent, SidePanelLabels, SidePanelTab,
    SidePanelTabKind, Sidebar, SidebarInput, SidebarIntent, SidebarLabels, SummaryInput,
    SummaryIntent, SummaryLabels, SummaryView, ThreadRename,
};
use tekes_agent_contract::{
    PromptMode, Result, SessionAddress, ThreadActionOutcome, require, unsupported,
};
use tekes_agent_presentation::{
    ChildHistoryLayout, ComposerDestination, PreparedSubmission, RunScope, SideChat,
    SideChatSnapshot, SummaryAction, TaskLoadToken,
};
use tekes_agent_ui_bindings::{
    BoundSideChatRun, ChildPaneBinding, ComposerSettingsRequest, InitialPresetChoices,
    InitialSessionCreation, MODEL_SELECTION_SETTING, ModelChoices, PaneAccess, PaneSnapshot,
    RunBinding, SideChatBinding, SidebarBinding, SidebarPublication, TaskPaneBinding,
};
use tekes_ui_controls::{
    ButtonTextRenderer, TextSurface,
    composition::*,
    extended::{Axis, Stack},
    layout::MeasuredChild,
};
use tekes_ui_scene::{Rect, Scene, Size};
use tekes_ui_tokens::{Palette, Spacing};

const SIDEBAR_WIDTH: f32 = 180.;
// Test-host policy: both simultaneous Run panes must fit a reading column.
// This is not the product SidePanel's minimum width or collapse policy.
fn panel_width() -> f32 {
    tekes_agent_components_ui::RunColumnLayout::MINIMUM_CONTENT_WIDTH
        + 2. * tekes_agent_components_ui::RunColumnLayout::horizontal_inset()
}
pub const DEMO_CONTENT_INSET: f32 = Spacing::Loose.value();
const ROOT: ComponentId = ComponentId(1);
const NAV: ComponentId = ComponentId(2);
const NOTICE: ComponentId = ComponentId(3);
const DIAGNOSTIC: ComponentId = ComponentId(4);
const MAIN: ComponentId = ComponentId(5);
const SIDEBAR_COLUMN: ComponentId = ComponentId(6);
const PANEL_COLUMN: ComponentId = ComponentId(7);
const MAIN_COLUMN: ComponentId = ComponentId(8);
const MAIN_UPLOAD_HOST: ComponentId = ComponentId(12);
const PANEL_UPLOAD_HOST: ComponentId = ComponentId(13);
const EMPTY_MAIN: ComponentId = ComponentId(9);
const CREATE_PANEL: ComponentId = ComponentId(10);
const SIDEBAR_NAMESPACE: u32 = 40;
const PANEL_NAMESPACE: u32 = 41;
const RENAME_NAMESPACE: u32 = 42;
const CHOICES_NAMESPACE: u32 = 43;
const CREATE_NAMESPACE: u32 = 44;
const ANNOTATION_NAMESPACE: u32 = 45;
const REASONING_NAMESPACE: u32 = 46;
const SIDEBAR_OPTIONS_NAMESPACE: u32 = 47;
pub const SELECT_A: ComponentId = ComponentId(100);
pub const SELECT_B: ComponentId = ComponentId(101);
pub const RECONNECT: ComponentId = ComponentId(102);
pub const NEW_SESSION: ComponentId = ComponentId(103);
pub const CREATE_AUTOMATIC: ComponentId = ComponentId(104);

#[path = "component_file_preview.rs"]
mod file_preview;
#[path = "component_uploads.rs"]
mod uploads;
use file_preview::FilePreviewGesture;
use uploads::UploadGesture;
#[path = "component_feedback.rs"]
mod feedback;
pub use feedback::{FeedbackDemoCompletion, FeedbackDemoRequest};
pub enum DemoCommand {
    SidebarPreferences(tekes_agent_components_ui::SidebarPreferenceChange),
    SidebarSearch(tekes_agent_components_ui::SidebarSearchIntent),
    Feedback(Box<FeedbackDemoRequest>),
    CopyTranscript(tekes_agent_components_ui::TranscriptCopyIntent),
    JumpLatest {
        root: ComponentId,
        intent: Box<tekes_agent_components_ui::RunJumpLatest>,
    },
    TurnNavigation {
        root: ComponentId,
        intent: Box<tekes_agent_components_ui::RunTurnNavigation>,
    },
    TurnAction {
        binding: Arc<RunBinding>,
        intent: tekes_agent_components_ui::TurnFootnoteIntent,
    },
    ReadHistoryPage(tekes_agent_ui_bindings::HistoryPageRequest),
    RespondActionable {
        request: Arc<tekes_agent_ui_bindings::ActionableResponseRequest>,
        ids: ComposerIds,
        intent: tekes_agent_components_ui::ActionableResponseIntent,
    },
    ReadFilePreview {
        request: Arc<tekes_agent_ui_bindings::FilePreviewRequest>,
    },
    SelectFiles {
        request: Arc<tekes_agent_ui_bindings::AttachmentSelection>,
    },
    ReadInitialPresets {
        request: ComposerChoiceRequest,
    },
    CreateSession(CreateSessionCommand),
    OpenContext {
        binding: Arc<RunBinding>,
        intent: tekes_agent_components_ui::ComposerContextIntent,
    },
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
        read_intent: Option<tekes_agent_components_ui::ReadOnlyTranscriptIntent>,
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
    feedback_panes: BTreeMap<ComponentId, feedback::FeedbackPane>,
    feedback_completions: Vec<FeedbackDemoCompletion>,
    document_enabled: bool,
    document_roots: BTreeMap<ComponentId, ComponentId>,
    next_document_namespace: u32,
    file_preview_available: bool,
    preview_gesture: Option<FilePreviewGesture>,
    file_picker_available: bool,
    uploads: BTreeMap<String, UploadGesture>,
    next_upload_namespace: u32,
    pub tree: ContentHost,
    bindings: Vec<Arc<RunBinding>>,
    selected: usize,
    selection_cleared: bool,
    cleared_panel: Option<Content>,
    retained: Vec<Option<Content>>,
    navigation_revision: u64,
    sidebar_source: Option<Arc<SidebarPublication>>,
    sidebar_reader: Option<Arc<SidebarBinding>>,
    sidebar_folder_filter: Option<tekes_agent_components_ui::SidebarFolderKey>,
    sidebar_search_publication: Option<tekes_agent_ui_bindings::SidebarSearchPublication>,
    sidebar_search_presented: Option<tekes_agent_components_ui::SidebarSearchInput>,
    sidebar_presented: Option<SidebarInput>,
    panel_revision: u64,
    workspace_summary: Option<tekes_agent_components_ui::SummaryContextInput>,
    side_chats: Option<Arc<SideChatBinding>>,
    panel_selections: BTreeMap<RunScope, SidePanelTabKind>,
    panel_parent_projection: BTreeMap<RunScope, (bool, u64)>,
    panel_runs: BTreeMap<(RunScope, String), PanelRun>,
    task_sources: BTreeMap<(RunScope, String), TaskSource>,
    task_runs: BTreeMap<(RunScope, String), PanelRun>,
    task_attempts: BTreeMap<(RunScope, String), RunScope>,
    child_panes: BTreeMap<(RunScope, String), ChildPane>,
    background_commands: Vec<DemoCommand>,
    next_panel_namespace: u32,
    panel_navigation_revision: u64,
    model_gesture: Option<ModelGesture>,
    creation_inputs: Option<(tekes_agent_contract::CreateSessionRequest, String)>,
    creation_gesture: Option<CreationGesture>,
    creation_panel: Option<Content>,
    annotation_panel: Option<Content>,
    annotation_gesture: Option<AnnotationGesture>,
}
struct AnnotationGesture {
    binding: Arc<RunBinding>,
    navigation: u64,
    panel_navigation: u64,
}
struct CreationGesture {
    request: ComposerChoiceRequest,
    input: tekes_agent_contract::CreateSessionRequest,
    source: Option<InitialPresetChoices>,
    navigation: u64,
    attempt: u64,
    pending: Option<Arc<InitialSessionCreation>>,
}
#[derive(Clone)]
pub struct CreateSessionCommand {
    request: ComposerChoiceRequest,
    navigation: NavigationRequest,
    creation: Arc<InitialSessionCreation>,
}
impl CreateSessionCommand {
    pub fn creation(&self) -> &Arc<InitialSessionCreation> {
        &self.creation
    }
}
struct ModelGesture {
    request: ComposerChoiceRequest,
    settings: Option<ComposerSettingsRequest>,
    binding: Arc<RunBinding>,
    source: Option<ModelChoices>,
    reasoning: Option<(
        ComposerChoiceRequest,
        tekes_agent_ui_bindings::ReasoningChoices,
    )>,
    connection_epoch: u64,
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
    projection_generation: u64,
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
    /// Current demo uses the default reading policy. Hosts with different pane
    /// policies query their RunView.minimum_pane_width instead.
    pub fn minimum_window_width() -> f64 {
        (SIDEBAR_WIDTH
            + panel_width()
            + 2. * Spacing::Regular.value()
            + tekes_agent_components_ui::RunColumnLayout::MINIMUM_CONTENT_WIDTH
            + 2. * tekes_agent_components_ui::RunColumnLayout::horizontal_inset()
            + 2. * DEMO_CONTENT_INSET) as f64
    }
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
        let pair = Self::new_pair(0, bindings[0].scope());
        let rename = ThreadRename::new(RENAME_NAMESPACE, RenameLabels::default());
        let choices = ComposerChoices::new(CHOICES_NAMESPACE, Default::default());
        let reasoning = ComposerChoices::new(REASONING_NAMESPACE, Default::default());
        let annotation = AnnotationEditor::new(ANNOTATION_NAMESPACE, Default::default());
        let annotation_panel = Content::new(annotation.ids().root, annotation);
        let creation_choices = ComposerChoices::new(CREATE_NAMESPACE, Default::default());
        let creation_panel = Content::new(
            CREATE_PANEL,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![
                    Content::new(creation_choices.id(), creation_choices),
                    Content::new(CREATE_AUTOMATIC, ButtonContent::new("使用自动预设创建")),
                ],
            },
        );
        let main = Content::new(
            MAIN,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Regular.value(),
                },
                // RunView already owns a bounded viewport, including when the
                // optional transcript diagnostic is disabled. Giving spare
                // height to the notice would push its Composer below the window.
                flexible_child: Some(pair.id()),
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
                                (NEW_SESSION, "新会话"),
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
                        TextContent::new("持续阅读由 TekesUI 管理；完整原生验收仍在进行。"),
                    ),
                    Content::new(rename.ids().root, rename),
                    Content::new(choices.id(), choices),
                    Content::new(reasoning.id(), reasoning),
                    pair,
                ],
            },
        );
        let sidebar = Sidebar::new(SIDEBAR_NAMESPACE, SidebarLabels::default()).with_session_menu();
        let options = tekes_agent_components_ui::SidebarOptions::new(
            SIDEBAR_OPTIONS_NAMESPACE,
            tekes_agent_components_ui::SidebarOptionsLabels::default(),
        );
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
                            width: Some(SIDEBAR_WIDTH),
                            children: vec![Content::new(
                                ComponentId(15),
                                StackContent {
                                    model: Stack {
                                        axis: Axis::Vertical,
                                        gap: Spacing::Regular.value(),
                                    },
                                    flexible_child: Some(sidebar.id()),
                                    children: vec![
                                        Content::new(options.id(), options),
                                        Content::new(sidebar.id(), sidebar),
                                    ],
                                },
                            )],
                        },
                    ),
                    Content::new(
                        MAIN_COLUMN,
                        DemoColumn {
                            width: None,
                            children: vec![Content::new(
                                MAIN_UPLOAD_HOST,
                                StackContent {
                                    model: Stack {
                                        axis: Axis::Vertical,
                                        gap: Spacing::Regular.value(),
                                    },
                                    flexible_child: Some(MAIN),
                                    children: vec![main],
                                },
                            )],
                        },
                    ),
                    Content::new(
                        PANEL_COLUMN,
                        DemoColumn {
                            width: Some(panel_width()),
                            children: vec![Content::new(
                                PANEL_UPLOAD_HOST,
                                StackContent {
                                    model: Stack {
                                        axis: Axis::Vertical,
                                        gap: Spacing::Regular.value(),
                                    },
                                    flexible_child: Some(panel.id()),
                                    children: vec![Content::new(panel.id(), panel)],
                                },
                            )],
                        },
                    ),
                ],
            },
        ))
        .map_err(|_| unsupported("component demo mount"))?;
        tree.set_action_events(true);
        let retained = (0..bindings.len()).map(|_| None).collect();
        let mut view = Self {
            file_preview_available: false,
            preview_gesture: None,
            file_picker_available: false,
            uploads: BTreeMap::new(),
            next_upload_namespace: 2_000_000,
            tree,
            bindings,
            selected: 0,
            selection_cleared: false,
            cleared_panel: None,
            retained,
            navigation_revision: 0,
            sidebar_source: None,
            sidebar_reader: None,
            sidebar_folder_filter: None,
            sidebar_search_publication: None,
            sidebar_search_presented: None,
            sidebar_presented: None,
            panel_revision: 0,
            workspace_summary: None,
            side_chats: None,
            panel_selections: BTreeMap::new(),
            panel_parent_projection: BTreeMap::new(),
            panel_runs: BTreeMap::new(),
            task_sources: BTreeMap::new(),
            task_runs: BTreeMap::new(),
            task_attempts: BTreeMap::new(),
            child_panes: BTreeMap::new(),
            model_gesture: None,
            creation_inputs: None,
            creation_gesture: None,
            creation_panel: Some(creation_panel),
            annotation_panel: Some(annotation_panel),
            annotation_gesture: None,
            feedback_panes: BTreeMap::new(),
            feedback_completions: vec![],
            document_enabled: false,
            document_roots: BTreeMap::new(),
            next_document_namespace: 7_000_000,
            background_commands: vec![],
            next_panel_namespace: 50_000,
            panel_navigation_revision: 0,
        };
        view.sync()?;
        Ok(view)
    }
    fn new_pair(index: usize, scope: &RunScope) -> Content {
        let ids = Self::ids_for(index);
        Self::run_controls(ids, scope)
    }
    fn run_controls(ids: ComposerIds, scope: &RunScope) -> Content {
        let mut composer = Composer::new(ids)
            .with_context(1_000_000 + (ids.root.0 >> 32) as u32, Default::default())
            .expect("separate draft context namespace");
        composer.set_context_affordances(tekes_agent_components_ui::ComposerContextAffordances {
            edit_annotations: true,
            ..Default::default()
        });
        let view = RunView::new(
            RunViewIds {
                root: ComponentId(ids.root.0 + 16),
                indicators: ComponentId(ids.root.0 + 17),
                viewport: ComponentId(ids.root.0 + 21),
                composer: ids,
            },
            scope.clone(),
            composer,
            4_000_000 + (ids.root.0 >> 32) as u32,
            3_000_000 + (ids.root.0 >> 32) as u32,
        )
        .expect("separate RunView namespaces");
        Content::new(view.ids().root, view)
    }
    fn actionable_id(ids: ComposerIds) -> ComponentId {
        ComponentId((3_000_000 + (ids.root.0 >> 32)) << 32)
    }
    fn paging_id(ids: ComposerIds) -> ComponentId {
        ComponentId((4_000_000 + (ids.root.0 >> 32)) << 32)
    }
    pub fn history_page_button(&self) -> Option<ComponentId> {
        self.tree
            .get(Self::paging_id(self.ids()))?
            .component::<tekes_agent_components_ui::TranscriptPaging>()
            .map(tekes_agent_components_ui::TranscriptPaging::button_id)
    }
    /// A row-only public API probe. This does not install a Transcript or RunView.
    pub fn mount_input_message_proof(
        &mut self,
    ) -> Result<tekes_agent_components_ui::InputMessageIds> {
        let binding = self.binding().clone();
        let snapshot = binding
            .current_run()
            .ok_or_else(|| unsupported("input proof source missing"))?;
        let document = tekes_agent_presentation::TranscriptDocument::read(&snapshot.transcript)?;
        let input = document
            .sections
            .iter()
            .flat_map(|section| &section.inputs)
            .last()
            .ok_or_else(|| unsupported("input proof needs a canonical user message"))?;
        let mut message =
            tekes_agent_components_ui::InputMessage::new(6_000_000, binding.scope().clone());
        message
            .update(
                binding.scope(),
                snapshot.revision,
                snapshot.connection_epoch,
                input,
            )
            .map_err(unsupported)?;
        let ids = message.ids();
        let parent = ComponentId(self.ids().root.0 + 16);
        let composer = self.ids().root;
        self.tree
            .insert(parent, Content::new(ids.root, message))
            .map_err(|_| unsupported("input proof mount"))?;
        let mut order: Vec<_> = self
            .tree
            .get(parent)
            .unwrap()
            .component::<RunView>()
            .unwrap()
            .children()
            .iter()
            .map(Content::id)
            .filter(|id| *id != ids.root)
            .collect();
        let index = order.iter().position(|id| *id == composer).unwrap();
        order.insert(index, ids.root);
        self.tree
            .reorder(parent, &order)
            .map_err(|_| unsupported("input proof order"))?;
        Ok(ids)
    }
    /// Machine-row probe over the existing canonical document, not a Transcript.
    pub fn mount_output_message_proof(&mut self) -> Result<ComponentId> {
        let binding = self.binding().clone();
        let snapshot = binding
            .current_run()
            .ok_or_else(|| unsupported("output proof source missing"))?;
        let document = tekes_agent_presentation::TranscriptDocument::read(&snapshot.transcript)?;
        let (section, output) = document
            .sections
            .iter()
            .rev()
            .find_map(|section| section.outputs.last().map(|output| (section, output)))
            .ok_or_else(|| unsupported("output proof needs a machine message"))?;
        let mut message =
            tekes_agent_components_ui::OutputMessage::new(6_000_001, binding.scope().clone());
        message
            .update(
                binding.scope(),
                snapshot.revision,
                snapshot.connection_epoch,
                output,
                Some(section),
            )
            .map_err(unsupported)?;
        let id = message.id();
        let parent = ComponentId(self.ids().root.0 + 16);
        let composer = self.ids().root;
        self.tree
            .insert(parent, Content::new(id, message))
            .map_err(|_| unsupported("output proof mount"))?;
        let mut order: Vec<_> = self
            .tree
            .get(parent)
            .unwrap()
            .component::<RunView>()
            .unwrap()
            .children()
            .iter()
            .map(Content::id)
            .filter(|child| *child != id)
            .collect();
        let index = order.iter().position(|id| *id == composer).unwrap();
        order.insert(index, id);
        self.tree
            .reorder(parent, &order)
            .map_err(|_| unsupported("output proof order"))?;
        Ok(id)
    }
    pub fn enable_transcript_content(&mut self) -> Result<()> {
        self.document_enabled = true;
        self.sync()
    }
    pub fn transcript_content_id(&self, ids: ComposerIds) -> Option<ComponentId> {
        self.document_roots.get(&ids.root).copied()
    }
    fn sync_transcript_content(
        &mut self,
        ids: ComposerIds,
        binding: &Arc<RunBinding>,
        snapshot: &tekes_agent_presentation::RunSnapshot,
    ) -> Result<()> {
        if !self.document_enabled {
            return Ok(());
        }
        let available = binding.access() == PaneAccess::Ready && !self.composing();
        if ids.root == self.ids().root {
            self.tree
                .get_mut(MAIN)
                .unwrap()
                .component_mut::<StackContent>()
                .unwrap()
                .flexible_child = Some(ComponentId(ids.root.0 + 16));
        }
        let feedback_labels = self.feedback_labels(ids, binding);
        if let Some(root) = self.document_roots.get(&ids.root).copied() {
            self.tree
                .get_mut(root)
                .unwrap()
                .component_mut::<tekes_agent_components_ui::TranscriptContent>()
                .unwrap()
                .configure_feedback(feedback_labels)
                .map_err(unsupported)?;
            // Existing content was already published with Composer in the
            // original SDK reading transaction by sync_run_view. Do not write
            // Transcript a second time outside that transaction.
            self.attach_run_reading(ids, binding)?;
            return self.sync_feedback(ids, binding);
        }
        let namespace = self.next_document_namespace;
        let end = namespace
            .checked_add(10_000)
            .ok_or_else(|| unsupported("demo document namespace pool exhausted"))?;
        let mut content = tekes_agent_components_ui::TranscriptContent::new(
            namespace,
            namespace + 1..end,
            binding.scope().clone(),
            tekes_agent_components_ui::TranscriptContentLabels {
                feedback: feedback_labels,
                ..Default::default()
            },
        )
        .map_err(unsupported)?;
        content
            .update(
                binding.scope(),
                snapshot.revision,
                snapshot.connection_epoch,
                &snapshot.transcript,
                available,
            )
            .map_err(unsupported)?;
        let root = content.id();
        self.tree
            .get_mut(ComponentId(ids.root.0 + 16))
            .unwrap()
            .component_mut::<RunView>()
            .unwrap()
            .install_transcript(content)
            .map_err(unsupported)?;
        self.next_document_namespace = end;
        let run = self
            .tree
            .get_mut(ComponentId(ids.root.0 + 16))
            .unwrap()
            .component_mut::<RunView>()
            .unwrap();
        run.install_jump_to_latest(8_000_000 + (ids.root.0 >> 32) as u32)
            .map_err(unsupported)?;
        run.install_turn_rail(9_000_000 + (ids.root.0 >> 32) as u32)
            .map_err(unsupported)?;
        run.set_rail_navigation_gate(tekes_agent_components_ui::TurnRailNavigationGate {
            turn_start: true,
            latest: true,
        })
        .map_err(unsupported)?;
        self.document_roots.insert(ids.root, root);
        self.attach_run_reading(ids, binding)?;
        self.sync_feedback(ids, binding)
    }
    fn attach_run_reading(&mut self, ids: ComposerIds, binding: &Arc<RunBinding>) -> Result<()> {
        let root = ComponentId(ids.root.0 + 16);
        let run = self.tree.get(root).unwrap().component::<RunView>().unwrap();
        let viewport = run.ids().viewport;
        if self
            .tree
            .get(viewport)
            .unwrap()
            .component::<ScrollContent>()
            .unwrap()
            .reading_document_version()
            .is_some()
        {
            return Ok(());
        }
        let content = run
            .transcript()
            .ok_or_else(|| unsupported("reading Transcript missing"))?;
        let items = content
            .row_keys()
            .iter()
            .map(|key| content.row_id(key).unwrap())
            .collect();
        self.tree
            .set_scroll_reading_document(
                viewport,
                ComposerDestination::run(binding.scope().clone()).storage_key(),
                0,
                items,
                |_| (),
            )
            .map_err(|error| unsupported(&format!("Run reading attachment: {error:?}")))?;
        Ok(())
    }
    pub fn turn_footnote_id() -> ComponentId {
        ComponentId(6_000_002_u64 << 32)
    }
    /// Turn-aside probe, independently mounted outside both message bodies.
    pub fn mount_turn_footnote_proof(&mut self) -> Result<ComponentId> {
        let binding = self.binding().clone();
        let snapshot = binding
            .current_run()
            .ok_or_else(|| unsupported("footnote run missing"))?;
        let turn = snapshot
            .transcript
            .turns
            .last()
            .ok_or_else(|| unsupported("footnote turn missing"))?;
        let mut footer = tekes_agent_components_ui::TurnFootnote::new(
            6_000_002,
            binding.scope().clone(),
            Default::default(),
        );
        footer
            .update(
                binding.scope(),
                snapshot.revision,
                snapshot.connection_epoch,
                turn,
                binding.access() == PaneAccess::Ready && !self.composing(),
            )
            .map_err(unsupported)?;
        let id = footer.id();
        let parent = ComponentId(self.ids().root.0 + 16);
        let composer = self.ids().root;
        self.tree
            .insert(parent, Content::new(id, footer))
            .map_err(|_| unsupported("footnote mount"))?;
        let mut order: Vec<_> = self
            .tree
            .get(parent)
            .unwrap()
            .component::<RunView>()
            .unwrap()
            .children()
            .iter()
            .map(Content::id)
            .filter(|child| *child != id)
            .collect();
        let index = order.iter().position(|id| *id == composer).unwrap();
        order.insert(index, id);
        self.tree
            .reorder(parent, &order)
            .map_err(|_| unsupported("footnote order"))?;
        Ok(id)
    }
    fn sync_turn_footnote_proof(&mut self) -> Result<()> {
        let root = Self::turn_footnote_id();
        let Some(footer) = self
            .tree
            .get(root)
            .and_then(|c| c.component::<tekes_agent_components_ui::TurnFootnote>())
        else {
            return Ok(());
        };
        let turn_id = footer
            .facts()
            .ok_or_else(|| unsupported("footnote facts missing"))?
            .turn_id;
        let binding = self.binding().clone();
        require(
            footer.scope() == binding.scope(),
            "footnote mount scope mismatch",
        )?;
        let snapshot = binding
            .current_run()
            .ok_or_else(|| unsupported("footnote source missing"))?;
        let Some(turn) = snapshot
            .transcript
            .turns
            .iter()
            .find(|turn| turn.id == turn_id)
        else {
            self.tree
                .get_mut(root)
                .unwrap()
                .component_mut::<tekes_agent_components_ui::TurnFootnote>()
                .unwrap()
                .set_available(false);
            return Ok(());
        };
        let available = binding.access() == PaneAccess::Ready && !self.composing();
        self.tree
            .get_mut(root)
            .unwrap()
            .component_mut::<tekes_agent_components_ui::TurnFootnote>()
            .unwrap()
            .update(
                binding.scope(),
                snapshot.revision,
                snapshot.connection_epoch,
                turn,
                available,
            )
            .map_err(unsupported)
    }
    pub fn apply_turn_action_outcome(
        &mut self,
        token: &SidebarActionToken,
        outcome: &ThreadActionOutcome,
    ) -> Result<Option<NavigationRequest>> {
        if token.revision != self.navigation_revision
            || self.current_binding().is_none_or(|binding| {
                binding.scope().endpoint_id != token.address.endpoint_id
                    || binding.scope().session_id != token.address.session_id
            })
        {
            return Ok(None);
        }
        self.apply_sidebar_outcome(token, outcome)
    }
    fn sync_run_view(
        &mut self,
        ids: ComposerIds,
        binding: &Arc<RunBinding>,
        snapshot: &tekes_agent_presentation::RunSnapshot,
        affordances: ComposerAffordances,
        gate: tekes_agent_components_ui::ComposerControlGate,
    ) -> Result<()> {
        let mut receipt = binding.history_page_state();
        if receipt.connection_epoch != snapshot.connection_epoch {
            receipt.source_ready = false;
            receipt.is_current = false;
        }
        let supported = self
            .child_panes
            .values()
            .find(|pane| Arc::ptr_eq(pane.source.binding(), binding))
            .is_none_or(|pane| pane.source.supports_pagination());
        let controls =
            !self.composing() && binding.access() == PaneAccess::Ready && receipt.source_ready;
        let root = ComponentId(ids.root.0 + 16);
        let run = self.tree.get(root).unwrap().component::<RunView>().unwrap();
        let version = self
            .tree
            .get(run.ids().viewport)
            .and_then(|node| node.component::<ScrollContent>())
            .and_then(ScrollContent::reading_document_version)
            .cloned();
        let paging = receipt.controls(&snapshot.transcript, supported);
        if let Some(version) = version {
            let plan = run
                .prepare_update(snapshot, affordances, gate, paging, controls)
                .map_err(unsupported)?;
            let revision = version
                .revision()
                .checked_add(1)
                .ok_or_else(|| unsupported("Run display revision exhausted"))?;
            tekes_agent_components_ui::publish_run_reading_update(
                &mut self.tree,
                root,
                &version,
                revision,
                &plan,
            )
            .map_err(|error| unsupported(&format!("Run reading publication: {error:?}")))?;
            return Ok(());
        }
        self.tree
            .get_mut(root)
            .unwrap()
            .component_mut::<RunView>()
            .unwrap()
            .update(snapshot, affordances, gate, paging, controls)
            .map_err(unsupported)
    }
    #[cfg(test)]
    fn sync_paging(
        &mut self,
        ids: ComposerIds,
        binding: &Arc<RunBinding>,
        snapshot: &tekes_agent_presentation::RunSnapshot,
    ) -> Result<()> {
        let mut receipt = binding.history_page_state();
        if receipt.connection_epoch != snapshot.connection_epoch {
            receipt.source_ready = false;
            receipt.is_current = false;
        }
        let supported = self
            .child_panes
            .values()
            .find(|pane| Arc::ptr_eq(pane.source.binding(), binding))
            .is_none_or(|pane| pane.source.supports_pagination());
        self.tree
            .get_mut(ComponentId(ids.root.0 + 16))
            .expect("RunView")
            .component_mut::<RunView>()
            .unwrap()
            .update_paging(receipt.controls(&snapshot.transcript, supported))
            .map_err(unsupported)
    }
    #[cfg(test)]
    fn sync_actionable(
        &mut self,
        ids: ComposerIds,
        binding: &Arc<RunBinding>,
        snapshot: &tekes_agent_presentation::RunSnapshot,
    ) -> Result<()> {
        let prompt = snapshot
            .run
            .as_ref()
            .and_then(|run| run.actionables.first())
            .map(|value| {
                tekes_agent_presentation::ActionablePrompt::read(
                    binding.scope().clone(),
                    value.clone(),
                )
                .map(Arc::new)
            })
            .transpose()?;
        let epoch = snapshot.connection_epoch;
        let authority = binding.history_page_state();
        let controls = !self.composing()
            && binding.access() == PaneAccess::Ready
            && authority.source_ready
            && authority.connection_epoch == epoch;
        self.tree
            .get_mut(ComponentId(ids.root.0 + 16))
            .expect("RunView")
            .component_mut::<RunView>()
            .unwrap()
            .update_actionable(snapshot.revision, epoch, prompt, controls)
            .map_err(unsupported)?;
        Ok(())
    }
    pub fn actionable_choice_id(&self, ids: ComposerIds, choice: &str) -> Option<ComponentId> {
        self.tree
            .get(Self::actionable_id(ids))?
            .component::<tekes_agent_components_ui::ActionableCard>()?
            .choice_id(choice)
    }
    pub fn complete_actionable(
        &mut self,
        _ids: ComposerIds,
        intent: &tekes_agent_components_ui::ActionableResponseIntent,
        result: std::result::Result<(), String>,
    ) -> Result<bool> {
        let accepted = intent.complete(result);
        self.sync()?;
        Ok(accepted)
    }
    fn pair_id(index: usize) -> ComponentId {
        ComponentId(Self::ids_for(index).root.0 + 16)
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
    fn annotation_root() -> ComponentId {
        ComponentId((ANNOTATION_NAMESPACE as u64) << 32)
    }
    pub fn annotation_editor_ids(&self) -> Option<AnnotationEditorIds> {
        self.tree
            .get(Self::annotation_root())?
            .component::<AnnotationEditor>()
            .filter(|editor| editor.is_open())
            .map(AnnotationEditor::ids)
    }
    fn cancel_annotation(&mut self) -> Result<()> {
        if self.annotation_gesture.is_some() {
            self.tree
                .get_mut(Self::annotation_root())
                .unwrap()
                .component_mut::<AnnotationEditor>()
                .unwrap()
                .dismiss()
                .map_err(unsupported)?;
            self.annotation_panel = Some(
                self.tree
                    .remove(Self::annotation_root())
                    .map_err(|_| unsupported("detach annotation editor"))?,
            );
            self.annotation_gesture = None;
        }
        Ok(())
    }
    fn begin_annotation(
        &mut self,
        binding: Arc<RunBinding>,
        intent: &tekes_agent_components_ui::ComposerContextIntent,
    ) -> Result<()> {
        require(
            !self.composing() && binding.access() == PaneAccess::Ready,
            "annotation editing unavailable",
        )?;
        require(
            binding
                .current_run()
                .is_some_and(|snapshot| intent.baseline.matches_snapshot(&snapshot)),
            "annotation capture obsolete",
        )?;
        self.cancel_annotation()?;
        self.cancel_creation();
        self.cancel_model_choices();
        self.tree.focus(None, false);
        let mut panel = self
            .annotation_panel
            .take()
            .expect("retained annotation editor");
        if let Err(error) = panel
            .component_mut::<AnnotationEditor>()
            .unwrap()
            .open(intent)
        {
            self.annotation_panel = Some(panel);
            return Err(unsupported(error));
        }
        self.tree
            .insert(MAIN, panel)
            .map_err(|_| unsupported("mount annotation editor"))?;
        self.annotation_gesture = Some(AnnotationGesture {
            binding,
            navigation: self.navigation_revision,
            panel_navigation: self.panel_navigation_revision,
        });
        self.tree
            .reorder(
                MAIN,
                &[
                    NAV,
                    NOTICE,
                    DIAGNOSTIC,
                    RenameIds::new(RENAME_NAMESPACE).root,
                    Self::choices_id(),
                    Self::reasoning_id(),
                    Self::annotation_root(),
                    Self::pair_id(self.selected),
                ],
            )
            .map_err(|_| unsupported("annotation panel order"))?;
        self.tree
            .focus(Some(self.annotation_editor_ids().unwrap().editor), true);
        self.sync_annotation()
    }
    fn sync_annotation(&mut self) -> Result<()> {
        let Some(gesture) = self.annotation_gesture.as_ref() else {
            return Ok(());
        };
        let binding = gesture.binding.clone();
        if gesture.navigation != self.navigation_revision
            || gesture.panel_navigation != self.panel_navigation_revision
        {
            return self.cancel_annotation();
        }
        let editor = self
            .tree
            .get_mut(Self::annotation_root())
            .unwrap()
            .component_mut::<AnnotationEditor>()
            .unwrap();
        if let Some(snapshot) = binding.current_run() {
            editor
                .update_authority(&snapshot, binding.access() == PaneAccess::Ready)
                .map_err(unsupported)?;
        } else {
            editor.invalidate();
        }
        Ok(())
    }
    fn drain_annotation(&mut self, actions: &[ContentAction]) -> Result<()> {
        let Some(panel) = self.tree.get_mut(Self::annotation_root()) else {
            return Ok(());
        };
        let intents = panel
            .component_mut::<AnnotationEditor>()
            .unwrap()
            .take_intents(actions);
        for intent in intents {
            match intent {
                AnnotationEditorIntent::Change(change) => {
                    let binding = self.annotation_gesture.as_ref().unwrap().binding.clone();
                    let result = binding
                        .edit_annotation(&change)
                        .map_err(|error| error.to_string());
                    self.tree
                        .get_mut(Self::annotation_root())
                        .unwrap()
                        .component_mut::<AnnotationEditor>()
                        .unwrap()
                        .complete(&change, result);
                }
                AnnotationEditorIntent::Dismiss => self.cancel_annotation()?,
            }
        }
        Ok(())
    }
    pub fn configure_creation(
        &mut self,
        input: tekes_agent_contract::CreateSessionRequest,
        rpc_prefix: impl Into<String>,
    ) -> Result<()> {
        require(
            self.creation_gesture.is_none(),
            "creation gesture already open",
        )?;
        require(
            input.session_id.is_none() && input.agent_preset.is_none(),
            "demo creation inputs must preserve generated session and automatic preset",
        )?;
        require(
            input
                .cwd
                .as_ref()
                .is_some_and(|cwd| cwd.starts_with('/') && !cwd.contains('\0')),
            "demo needs explicit absolute creation cwd",
        )?;
        let prefix = rpc_prefix.into();
        require(!prefix.is_empty(), "empty creation RPC prefix")?;
        self.creation_inputs = Some((input, prefix));
        self.sync()
    }
    fn creation_choices_id() -> ComponentId {
        ComponentId((CREATE_NAMESPACE as u64) << 32)
    }
    pub fn initial_preset_choice_id(&self, choice: &str) -> Option<ComponentId> {
        self.tree
            .get(Self::creation_choices_id())?
            .component::<ComposerChoices>()?
            .choice_id(choice)
    }
    fn creation_current(&self, gesture: &CreationGesture) -> bool {
        gesture.navigation == self.navigation_revision
            && self.binding().supports_creation()
            && !self.composing()
    }
    fn cancel_creation(&mut self) {
        if let Some(gesture) = self.creation_gesture.take() {
            self.tree
                .get_mut(Self::creation_choices_id())
                .unwrap()
                .component_mut::<ComposerChoices>()
                .unwrap()
                .dismiss(&gesture.request);
            self.creation_panel = Some(
                self.tree
                    .remove(CREATE_PANEL)
                    .expect("mounted creation panel"),
            );
        }
    }
    fn begin_creation(&mut self) -> Result<DemoCommand> {
        require(
            !self.composing() && self.binding().supports_creation(),
            "creation unavailable",
        )?;
        let (input, _) = self
            .creation_inputs
            .clone()
            .ok_or_else(|| unsupported("host creation inputs missing"))?;
        self.cancel_model_choices();
        self.cancel_annotation()?;
        self.cancel_creation();
        self.tree.focus(None, false);
        self.tree
            .insert(
                MAIN,
                self.creation_panel.take().expect("retained creation panel"),
            )
            .map_err(|_| unsupported("mount creation choices"))?;
        let destination = ComposerDestination::NewThread {
            endpoint_id: self.binding().scope().endpoint_id.clone(),
            workspace_id: input
                .workspace_id
                .clone()
                .unwrap_or_else(|| "demo-create".into()),
        };
        let request = self
            .tree
            .get_mut(Self::creation_choices_id())
            .unwrap()
            .component_mut::<ComposerChoices>()
            .unwrap()
            .open(destination, ComposerChoiceKind::Preset)
            .map_err(unsupported)?;
        self.creation_gesture = Some(CreationGesture {
            request: request.clone(),
            input,
            source: None,
            navigation: self.navigation_revision,
            attempt: 0,
            pending: None,
        });
        let tail = if self.selection_cleared {
            EMPTY_MAIN
        } else {
            Self::pair_id(self.selected)
        };
        self.tree
            .reorder(
                MAIN,
                &[
                    NAV,
                    NOTICE,
                    DIAGNOSTIC,
                    RenameIds::new(RENAME_NAMESPACE).root,
                    Self::choices_id(),
                    Self::reasoning_id(),
                    CREATE_PANEL,
                    tail,
                ],
            )
            .map_err(|_| unsupported("creation panel order"))?;
        Ok(DemoCommand::ReadInitialPresets { request })
    }
    pub fn complete_initial_presets(
        &mut self,
        request: &ComposerChoiceRequest,
        result: std::result::Result<InitialPresetChoices, String>,
    ) -> Result<bool> {
        let Some(gesture) = self.creation_gesture.as_ref() else {
            return Ok(false);
        };
        if &gesture.request != request {
            return Ok(false);
        }
        if !self.creation_current(gesture) {
            self.cancel_creation();
            return Ok(false);
        }
        let (catalog, source) = match result {
            Ok(source) => (source.catalog.clone(), Some(source)),
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
            .get_mut(Self::creation_choices_id())
            .unwrap()
            .component_mut::<ComposerChoices>()
            .unwrap()
            .update(request, catalog)
            .map_err(unsupported)?;
        if accepted {
            self.creation_gesture.as_mut().unwrap().source = source;
        }
        Ok(accepted)
    }
    fn capture_creation(
        &mut self,
        selection: Option<&tekes_agent_components_ui::ComposerChoiceSelection>,
    ) -> Result<Option<DemoCommand>> {
        let Some(gesture) = self.creation_gesture.as_ref() else {
            return Ok(None);
        };
        if !self.creation_current(gesture) || gesture.pending.is_some() {
            return Ok(None);
        }
        if selection.is_some_and(|selection| selection.request != gesture.request) {
            return Ok(None);
        }
        let gesture = self.creation_gesture.as_mut().unwrap();
        gesture.attempt = gesture
            .attempt
            .checked_add(1)
            .ok_or_else(|| unsupported("creation attempt exhausted"))?;
        let rpc = format!(
            "{}:{}:{}",
            self.creation_inputs.as_ref().unwrap().1,
            gesture.request.generation,
            gesture.attempt
        );
        let creation = Arc::new(match selection {
            Some(selection) => gesture
                .source
                .as_ref()
                .ok_or_else(|| unsupported("initial preset source missing"))?
                .capture_creation(selection, gesture.input.clone(), rpc)?,
            None => InitialSessionCreation::automatic(
                gesture.request.destination.endpoint_id(),
                gesture.input.clone(),
                rpc,
            )?,
        });
        let navigation = NavigationRequest {
            address: SessionAddress {
                endpoint_id: creation.endpoint_id().into(),
                session_id: creation.session_id().into(),
            },
            revision: gesture.navigation,
            created: true,
        };
        gesture.pending = Some(creation.clone());
        self.tree
            .get_mut(Self::creation_choices_id())
            .unwrap()
            .component_mut::<ComposerChoices>()
            .unwrap()
            .set_host_pending(&gesture.request, true);
        Ok(Some(DemoCommand::CreateSession(CreateSessionCommand {
            request: gesture.request.clone(),
            navigation,
            creation,
        })))
    }
    fn drain_creation(&mut self, actions: &[ContentAction]) -> Result<Vec<DemoCommand>> {
        let Some(panel) = self.tree.get_mut(Self::creation_choices_id()) else {
            return Ok(vec![]);
        };
        let intents = panel
            .component_mut::<ComposerChoices>()
            .unwrap()
            .take_intents(actions);
        let mut commands = vec![];
        for intent in intents {
            match intent {
                ComposerChoiceIntent::Dismiss(_) => self.cancel_creation(),
                ComposerChoiceIntent::Select(selection) => {
                    if let Some(command) = self.capture_creation(Some(&selection))? {
                        commands.push(command);
                    }
                }
            }
        }
        Ok(commands)
    }
    pub fn complete_creation(
        &mut self,
        request: &CreateSessionCommand,
        result: std::result::Result<Arc<RunBinding>, String>,
    ) -> Result<bool> {
        let Some(gesture) = self.creation_gesture.as_ref() else {
            return Ok(false);
        };
        if gesture.request != request.request
            || !gesture
                .pending
                .as_ref()
                .is_some_and(|pending| Arc::ptr_eq(pending, &request.creation))
        {
            return Ok(false);
        }
        if !self.creation_current(gesture) {
            self.cancel_creation();
            return Ok(false);
        }
        match result {
            Ok(binding) => {
                require(
                    binding.scope()
                        == &RunScope::main(
                            &request.navigation.address.endpoint_id,
                            &request.navigation.address.session_id,
                        ),
                    "creation result target mismatch",
                )?;
                self.cancel_creation();
                self.complete_navigation(request.navigation.clone(), binding)
            }
            Err(error) => {
                // Retain the immutable IDs and pending barrier. No click creates
                // a replacement session after an uncertain result.
                self.set_notice(format!(
                    "创建未完成或结果未确认，请先核对会话列表；不会自动重试。{error}"
                ));
                Ok(false)
            }
        }
    }
    fn reasoning_id() -> ComponentId {
        ComponentId((REASONING_NAMESPACE as u64) << 32)
    }
    pub fn reasoning_choice_id(&self, effort: &str) -> Option<ComponentId> {
        self.tree
            .get(Self::reasoning_id())?
            .component::<ComposerChoices>()?
            .choice_id(effort)
    }
    fn dismiss_model_gesture(&mut self, gesture: &ModelGesture) {
        for (root, request) in [
            (Self::choices_id(), Some(&gesture.request)),
            (
                Self::reasoning_id(),
                gesture.reasoning.as_ref().map(|(request, _)| request),
            ),
        ] {
            if let Some(request) = request {
                self.tree
                    .get_mut(root)
                    .unwrap()
                    .component_mut::<ComposerChoices>()
                    .unwrap()
                    .dismiss(request);
            }
        }
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
            self.dismiss_model_gesture(&gesture);
        }
    }
    fn model_gesture_current(&self, gesture: &ModelGesture) -> bool {
        !self.selection_cleared
            && gesture.navigation == self.navigation_revision
            && gesture.panel_navigation == self.panel_navigation_revision
            && gesture.binding.supports_model_choices()
            && gesture
                .binding
                .subscribe_runtime()
                .inspect(|state| state.connection_epoch == gesture.connection_epoch)
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
        self.cancel_creation();
        self.cancel_annotation()?;
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
            reasoning: None,
            connection_epoch: binding
                .subscribe_runtime()
                .inspect(|state| state.connection_epoch),
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
            let reasoning = if let Some(source) = &source {
                if let Some(model_id) = &source.catalog.selected
                    && source.catalog.choices.iter().any(|row| &row.id == model_id)
                {
                    match source.reasoning_choices(model_id, None) {
                        Ok(reasoning) => reasoning,
                        Err(error) => {
                            self.set_notice(format!("推理档位不可用：{error}"));
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };
            let old = self.model_gesture.as_mut().unwrap().reasoning.take();
            let panel = self
                .tree
                .get_mut(Self::reasoning_id())
                .unwrap()
                .component_mut::<ComposerChoices>()
                .unwrap();
            if let Some((request, _)) = old {
                panel.dismiss(&request);
            }
            if let Some(reasoning) = reasoning {
                let reasoning_request = panel
                    .open(request.destination.clone(), ComposerChoiceKind::Reasoning)
                    .map_err(unsupported)?;
                panel
                    .update(&reasoning_request, reasoning.catalog.clone())
                    .map_err(unsupported)?;
                self.model_gesture.as_mut().unwrap().reasoning =
                    Some((reasoning_request, reasoning));
            }
            self.model_gesture.as_mut().unwrap().source = source;
        }
        Ok(accepted)
    }
    fn drain_model_choices(&mut self, actions: &[ContentAction]) -> Result<()> {
        for root in [Self::choices_id(), Self::reasoning_id()] {
            let intents = self
                .tree
                .get_mut(root)
                .unwrap()
                .component_mut::<ComposerChoices>()
                .unwrap()
                .take_intents(actions);
            for intent in intents {
                match intent {
                    ComposerChoiceIntent::Dismiss(_) => self.cancel_model_choices(),
                    ComposerChoiceIntent::Select(selection) => {
                        let Some(mut gesture) = self.model_gesture.take() else {
                            continue;
                        };
                        let reasoning = root == Self::reasoning_id();
                        let expected = if reasoning {
                            gesture.reasoning.as_ref().map(|(request, _)| request)
                        } else {
                            Some(&gesture.request)
                        };
                        if expected != Some(&selection.request)
                            || !self.model_gesture_current(&gesture)
                        {
                            self.dismiss_model_gesture(&gesture);
                            continue;
                        }
                        let result = (|| {
                            let payload = if reasoning {
                                let (_, source) = gesture
                                    .reasoning
                                    .as_ref()
                                    .ok_or_else(|| unsupported("reasoning catalog missing"))?;
                                require(
                                    source.catalog.revision == selection.catalog_revision,
                                    "reasoning catalog revision changed",
                                )?;
                                source.selection(&selection.choice.id)?
                            } else {
                                let source = gesture
                                    .source
                                    .as_ref()
                                    .ok_or_else(|| unsupported("model catalog missing"))?;
                                require(
                                    source.catalog.revision == selection.catalog_revision,
                                    "model catalog revision changed",
                                )?;
                                source.selection(&selection.choice.id)?
                            };
                            let mut settings = gesture
                                .binding
                                .current_run()
                                .ok_or_else(|| unsupported("model source missing"))?
                                .draft
                                .settings
                                .clone();
                            if reasoning
                                && let Some(current) = settings.get(MODEL_SELECTION_SETTING)
                            {
                                require(
                                    current["provider"] == payload["provider"]
                                        && current["model"] == payload["model"],
                                    "reasoning target changed with draft model",
                                )?;
                            }
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
                            .get_mut(root)
                            .unwrap()
                            .component_mut::<ComposerChoices>()
                            .unwrap()
                            .complete(&selection, result.map_err(|e| e.to_string()));
                        if failed {
                            self.model_gesture = Some(gesture);
                        } else {
                            self.dismiss_model_gesture(&gesture);
                        }
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
        if action == "fork" || action.starts_with("forkTurn:") {
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
        self.sync()?;
        self.reconcile_sidebar_selection()
    }
    pub fn set_sidebar_title_comparator(
        &mut self,
        compare: tekes_agent_components_ui::SidebarTitleComparator,
    ) {
        let id = self.sidebar_id();
        self.tree
            .get_mut(id)
            .unwrap()
            .component_mut::<Sidebar>()
            .unwrap()
            .set_title_comparator(compare);
    }
    pub fn set_sidebar_preferences(
        &mut self,
        value: &tekes_agent_components_ui::SidebarPreferencesSnapshot,
        writable: bool,
    ) -> Result<()> {
        let id = self.sidebar_id();
        let current = self
            .tree
            .get(id)
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .preferences_current(value, writable);
        if !current {
            self.tree
                .get_mut(id)
                .unwrap()
                .component_mut::<Sidebar>()
                .unwrap()
                .update_preferences(value, writable)
                .map_err(unsupported)?;
        }
        let title_sort_supported = self
            .tree
            .get(id)
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .title_sort_supported();
        self.tree
            .get_mut(self.sidebar_options_id())
            .unwrap()
            .component_mut::<tekes_agent_components_ui::SidebarOptions>()
            .unwrap()
            .update(value, writable, title_sort_supported)
            .map_err(unsupported)
    }
    pub fn sidebar_options_id(&self) -> ComponentId {
        ComponentId((SIDEBAR_OPTIONS_NAMESPACE as u64) << 32)
    }
    pub fn sidebar_id(&self) -> ComponentId {
        ComponentId((SIDEBAR_NAMESPACE as u64) << 32)
    }
    pub fn panel_id(&self) -> ComponentId {
        ComponentId((PANEL_NAMESPACE as u64) << 32)
    }
    fn panel_body_owns(&self, target: ComponentId) -> bool {
        let Some(panel) = self
            .tree
            .get(self.panel_id())
            .and_then(|node| node.component::<SidePanel>())
        else {
            return false;
        };
        let root = panel.content_slot_id();
        let nodes = self.tree.semantics();
        let mut cursor = Some(target);
        while let Some(id) = cursor {
            if id == root {
                return true;
            }
            cursor = nodes
                .iter()
                .find(|node| node.id == id)
                .and_then(|node| node.parent);
        }
        false
    }
    /// Host parent-projection availability is independent of connection state.
    /// Finish a composing pane before retiring it; a composing main editor is
    /// retained. Only the old pane's UI affordances and SDK focus are released.
    pub fn set_panel_parent_projection_ready(
        &mut self,
        parent: &RunScope,
        ready: bool,
    ) -> Result<()> {
        require(
            self.bindings
                .iter()
                .any(|binding| binding.scope() == parent),
            "unknown panel parent projection",
        )?;
        let (current, generation) = self
            .panel_parent_projection
            .get(parent)
            .copied()
            .unwrap_or((true, 0));
        if current == ready {
            return Ok(());
        }
        let generation = generation
            .checked_add(1)
            .ok_or_else(|| unsupported("panel projection generation exhausted"))?;
        if self
            .current_binding()
            .is_some_and(|binding| binding.scope() == parent)
            && !ready
        {
            let pane_binding = self.active_panel_run().map(|run| run.binding.clone());
            let pane_annotation = self.annotation_gesture.as_ref().is_some_and(|gesture| {
                pane_binding
                    .as_ref()
                    .is_some_and(|binding| Arc::ptr_eq(binding, &gesture.binding))
            });
            require(
                !(self
                    .composing_editor()
                    .is_some_and(|id| self.panel_body_owns(id))
                    || pane_annotation && self.composing()),
                "finish pane composition before retiring its parent projection",
            )?;
            if self.model_gesture.as_ref().is_some_and(|gesture| {
                pane_binding
                    .as_ref()
                    .is_some_and(|binding| Arc::ptr_eq(binding, &gesture.binding))
            }) {
                self.cancel_model_choices();
            }
            if pane_annotation {
                self.cancel_annotation()?;
            }
            if self
                .tree
                .focused()
                .is_some_and(|id| self.panel_body_owns(id))
            {
                self.tree.focus(None, false);
            }
        }
        self.panel_parent_projection
            .insert(parent.clone(), (ready, generation));
        self.sync()
    }
    fn panel_parent_is_projected(&self, parent: &RunScope) -> bool {
        self.panel_parent_projection
            .get(parent)
            .is_none_or(|value| value.0)
    }
    fn panel_projection_generation(&self, parent: &RunScope) -> u64 {
        self.panel_parent_projection
            .get(parent)
            .map_or(0, |value| value.1)
    }
    /// Explicit owned-demo host input; the shared component validates workspace
    /// rows without using the current session as their display/data authority.
    pub fn set_workspace_summary(
        &mut self,
        input: tekes_agent_components_ui::SummaryContextInput,
    ) -> Result<()> {
        if !matches!(
            input.owner,
            tekes_agent_components_ui::SummaryOwner::Workspace { .. }
        ) {
            return Err(unsupported(
                "workspace Summary needs an explicit workspace owner",
            ));
        }
        let id = self.summary_id();
        self.tree
            .get_mut(id)
            .and_then(|node| node.component_mut::<SummaryView>())
            .ok_or_else(|| unsupported("Summary is not mounted"))?
            .update_context(&input)
            .map_err(unsupported)?;
        self.workspace_summary = Some(input);
        Ok(())
    }
    pub fn summary_id(&self) -> ComponentId {
        ComponentId((101 + self.selected as u64 * 2) << 32)
    }
    pub fn bind_sidebar_search_source(
        &mut self,
        owner: &str,
        input: &tekes_agent_components_ui::SidebarSearchInput,
    ) -> Result<()> {
        self.tree
            .get_mut(self.sidebar_id())
            .unwrap()
            .component_mut::<Sidebar>()
            .unwrap()
            .replace_search_source(owner, input)
            .map_err(unsupported)?;
        self.sidebar_search_publication = None;
        Ok(())
    }
    pub fn update_sidebar_search_source(
        &mut self,
        owner: &str,
        input: &tekes_agent_components_ui::SidebarSearchInput,
    ) -> Result<()> {
        self.tree
            .get_mut(self.sidebar_id())
            .unwrap()
            .component_mut::<Sidebar>()
            .unwrap()
            .update_search_from(owner, input)
            .map_err(unsupported)?;
        self.sidebar_search_publication = None;
        Ok(())
    }
    pub fn set_sidebar_search(
        &mut self,
        input: &tekes_agent_components_ui::SidebarSearchInput,
    ) -> Result<()> {
        self.tree
            .get_mut(self.sidebar_id())
            .unwrap()
            .component_mut::<Sidebar>()
            .unwrap()
            .update_search(input)
            .map_err(unsupported)?;
        self.sidebar_search_publication = None;
        Ok(())
    }
    /// Publish the complete original reader result, not only its status footer.
    pub fn bind_sidebar_search_publication(
        &mut self,
        owner: &str,
        publication: &tekes_agent_ui_bindings::SidebarSearchPublication,
    ) -> Result<()> {
        self.check_sidebar_search_publication(publication)?;
        self.bind_sidebar_search_source(owner, &publication.input)?;
        self.sidebar_search_publication = Some(publication.clone());
        self.sync_sidebar()
    }
    pub fn update_sidebar_search_publication(
        &mut self,
        owner: &str,
        publication: &tekes_agent_ui_bindings::SidebarSearchPublication,
    ) -> Result<()> {
        if self.sidebar_search_publication.as_ref().is_some_and(|old| {
            old.input.revision == publication.input.revision && old != publication
        }) {
            return Err(unsupported("conflicting sidebar search result publication"));
        }
        self.check_sidebar_search_publication(publication)?;
        self.update_sidebar_search_source(owner, &publication.input)?;
        self.sidebar_search_publication = Some(publication.clone());
        self.sync_sidebar()
    }
    fn check_sidebar_search_publication(
        &self,
        publication: &tekes_agent_ui_bindings::SidebarSearchPublication,
    ) -> Result<()> {
        self.sidebar_source
            .as_ref()
            .ok_or_else(|| unsupported("sidebar inventory not installed"))?
            .project_search(&self.binding().scope().endpoint_id, publication, None, 1)
            .map(|_| ())
            .map_err(unsupported)
    }
    /// Help remains query-scoped metadata until the fixed SDK can present it.
    pub fn sidebar_search_snippet(&self, address: &SessionAddress) -> Option<&str> {
        self.tree
            .get(self.sidebar_id())?
            .component::<Sidebar>()?
            .search_snippet(address)
    }
    /// Full occupied-folder choices, independent of the current page/query/filter.
    pub fn sidebar_folder_catalog(
        &self,
    ) -> Result<Option<tekes_agent_ui_bindings::SidebarFolderCatalog>> {
        self.sidebar_source
            .as_ref()
            .map(|source| source.folder_catalog().map_err(unsupported))
            .transpose()
    }
    pub fn set_sidebar_folder_filter(
        &mut self,
        filter: Option<tekes_agent_components_ui::SidebarFolderKey>,
    ) -> Result<()> {
        if let Some(key) = &filter {
            key.validate()?;
        }
        if filter == self.sidebar_folder_filter {
            let sidebar = self.sidebar_id();
            self.tree
                .get_mut(sidebar)
                .and_then(|content| content.component_mut::<Sidebar>())
                .ok_or_else(|| unsupported("Sidebar is not mounted"))?
                .reset_paging()
                .map_err(unsupported)?;
        }
        let old = std::mem::replace(&mut self.sidebar_folder_filter, filter);
        if let Err(error) = self.sync_sidebar() {
            self.sidebar_folder_filter = old;
            return Err(error);
        }
        Ok(())
    }
    pub fn set_sidebar_reader(&mut self, source: Arc<SidebarBinding>) -> Result<()> {
        require(
            self.current_binding()
                .is_some_and(|b| b.shares_runtime(source.runtime())),
            "Sidebar selected reader belongs to another runtime",
        )?;
        self.sidebar_reader = Some(source);
        Ok(())
    }
    fn supplement_sidebar_selection(&self, display: &mut SidebarInput) -> Result<bool> {
        let Some(source) = &self.sidebar_reader else {
            return Ok(false);
        };
        let Some(address) = display.selected.as_ref() else {
            return Ok(false);
        };
        if display.rows.iter().any(|row| &row.address == address)
            || self
                .sidebar_search_publication
                .as_ref()
                .is_some_and(|s| s.input.key.is_some())
        {
            return Ok(false);
        }
        let generation = self
            .navigation_revision
            .checked_add(1)
            .ok_or_else(|| unsupported("Sidebar selected generation exhausted"))?;
        let read = source.read_selected(address, generation)?;
        Ok(read.project_into(source, display, generation))
    }
    pub fn set_sidebar_source(&mut self, source: Arc<SidebarPublication>) -> Result<()> {
        require(
            self.sidebar_source.as_ref().is_none_or(|old| {
                source.revision > old.revision || source.as_ref() == old.as_ref()
            }),
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
        let view = self
            .tree
            .get(ComponentId(pane.controls.ids.root.0 + 16))?
            .component::<tekes_agent_components_ui::ReadOnlyTranscript>()?;
        Some((view.ids().refresh, view.paging_button_id()))
    }
    pub fn bounded_child_view_id(&self) -> Option<ComponentId> {
        let (refresh, _) = self.bounded_child_buttons()?;
        let parent = self.binding().scope();
        let SidePanelTabKind::Subagent { child_id } = self.panel_selections.get(parent)? else {
            return None;
        };
        let pane = self.child_panes.get(&(parent.clone(), child_id.clone()))?;
        let view = self
            .tree
            .get(ComponentId(pane.controls.ids.root.0 + 16))?
            .component::<tekes_agent_components_ui::ReadOnlyTranscript>()?;
        (view.ids().refresh == refresh).then_some(view.ids().root)
    }
    pub fn bounded_child_content_id(&self) -> Option<ComponentId> {
        let root = self.bounded_child_view_id()?;
        self.tree
            .get(root)?
            .component::<tekes_agent_components_ui::ReadOnlyTranscript>()
            .map(|pane| pane.content_id())
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
            !self.composing()
                && self.binding().scope() == panel_parent
                && self.panel_parent_is_projected(panel_parent),
            "child navigation parent changed or composing",
        )?;
        Ok(self.panel_request(panel_parent.clone()))
    }
    pub fn child_request_current(&self, request: &PanelRequest) -> bool {
        !self.selection_cleared
            && self.panel_parent_is_projected(&request.parent)
            && self.panel_projection_generation(&request.parent) == request.projection_generation
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
            PaneSnapshot::ReadOnlyChild {
                scope, publication, ..
            } if source.layout() == ChildHistoryLayout::SameSession => {
                scope == source.binding().scope() && *publication > 0
            }
            _ => false,
        };
        require(valid, "child snapshot source mismatch")?;
        if let PaneSnapshot::ReadOnlyChild { transcript, .. } = snapshot {
            let document = tekes_agent_presentation::TranscriptDocument::read(transcript)?;
            for section in &document.sections {
                section.event_order()?;
            }
        }
        Ok(())
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
        let admission = (|| -> Result<bool> {
            if let (
                PaneSnapshot::ReadOnlyChild {
                    publication: old,
                    connection_epoch: old_epoch,
                    connection: old_connection,
                    transcript: old_transcript,
                    ..
                },
                PaneSnapshot::ReadOnlyChild {
                    publication: next,
                    connection_epoch: next_epoch,
                    connection: next_connection,
                    transcript: next_transcript,
                    ..
                },
            ) = (&pane.snapshot, &snapshot)
            {
                if next < old || next_epoch < old_epoch {
                    return Ok(false);
                }
                require(
                    next != old
                        || (next_epoch == old_epoch
                            && next_connection == old_connection
                            && next_transcript == old_transcript),
                    "conflicting read-only child frame",
                )?;
            }
            Self::validate_child_snapshot(source, &snapshot)?;
            if let PaneSnapshot::ReadOnlyChild {
                transcript: next, ..
            } = &snapshot
                && let PaneSnapshot::ReadOnlyChild {
                    transcript: old, ..
                } = &pane.snapshot
            {
                let before = tekes_agent_presentation::TranscriptDocument::read(old)?;
                let after = tekes_agent_presentation::TranscriptDocument::read(next)?;
                let inputs: std::collections::BTreeSet<_> = before
                    .sections
                    .iter()
                    .flat_map(|section| section.inputs.iter().map(|row| row.key))
                    .collect();
                let outputs: std::collections::BTreeSet<_> = before
                    .sections
                    .iter()
                    .flat_map(|section| section.outputs.iter().map(|row| row.key))
                    .chain(before.session_notices.iter().map(|row| row.key))
                    .collect();
                require(
                    after.sections.iter().all(|section| {
                        section.inputs.iter().all(|row| !outputs.contains(&row.key))
                            && section.outputs.iter().all(|row| !inputs.contains(&row.key))
                    }) && after
                        .session_notices
                        .iter()
                        .all(|row| !inputs.contains(&row.key)),
                    "read-only Transcript message lane changed",
                )?;
                if let Some(view) = self
                    .tree
                    .get(ComponentId(pane.controls.ids.root.0 + 16))
                    .and_then(|node| {
                        node.component::<tekes_agent_components_ui::ReadOnlyTranscript>()
                    })
                {
                    view.validate_update(&source.readonly_input(&snapshot, false, None)?)
                        .map_err(unsupported)?;
                }
            }
            Ok(true)
        })();
        match admission {
            Ok(false) => return Ok(false),
            Err(error) => {
                pane.reading = false;
                pane.read_error = Some(error.to_string());
                self.sync()?;
                return Err(error);
            }
            Ok(true) => {}
        }
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
        require(
            self.panel_parent_is_projected(&parent),
            "task parent projection is pending",
        )?;
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
        for root in self.document_roots.values() {
            if let Some(editor) = self
                .tree
                .get(*root)
                .and_then(|node| node.component::<tekes_agent_components_ui::TranscriptContent>())
                .and_then(|content| content.composing_feedback_editor())
            {
                return Some(editor);
            }
        }

        if let Some(editor) = self
            .tree
            .get(Self::annotation_root())
            .and_then(|node| node.component::<AnnotationEditor>())
            && editor.is_composing()
        {
            return Some(editor.ids().editor);
        }
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
        let projection_generation = self.panel_projection_generation(&parent);
        PanelRequest {
            parent,
            main_revision: self.navigation_revision,
            panel_revision: self.panel_navigation_revision,
            projection_generation,
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
            && self.panel_parent_is_projected(&request.parent)
            && self.panel_projection_generation(&request.parent) == request.projection_generation
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
            let mut next = SidebarInput {
                folder_filter: self.sidebar_folder_filter.clone(),
                search_help: None,
                revision: self
                    .sidebar_presented
                    .as_ref()
                    .map_or(1, |old| old.revision + 1),
                loaded: source.loaded,
                error: source.error.clone(),
                rows: source.rows.clone(),
                selected,
            };
            self.supplement_sidebar_selection(&mut next)?;
            if let Some(search) = &self.sidebar_search_publication {
                next = source
                    .project_search_for_display(&self.binding().scope().endpoint_id, search, &next)
                    .map_err(unsupported)?
                    .input;
            }
            let search_input = self
                .sidebar_search_publication
                .as_ref()
                .map(|s| s.input.clone());
            if self.sidebar_presented.as_ref().is_none_or(|old| {
                old.rows != next.rows
                    || old.selected != next.selected
                    || old.loaded != next.loaded
                    || old.error != next.error
                    || old.search_help != next.search_help
                    || old.folder_filter != next.folder_filter
            }) || self.sidebar_search_presented != search_input
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
                self.sidebar_search_presented = search_input;
            }
        }
        Ok(())
    }
    pub fn sync(&mut self) -> Result<()> {
        self.sync_file_preview();
        self.sync_upload_status()?;
        self.sync_annotation()?;
        if self
            .creation_gesture
            .as_ref()
            .is_some_and(|gesture| !self.creation_current(gesture))
        {
            self.cancel_creation();
        }
        let creation_disabled = self.creation_inputs.is_none()
            || !self.binding().supports_creation()
            || self.composing()
            || self
                .creation_gesture
                .as_ref()
                .is_some_and(|gesture| gesture.pending.is_some());
        self.tree
            .get_mut(NEW_SESSION)
            .unwrap()
            .component_mut::<ButtonContent>()
            .unwrap()
            .button
            .state
            .disabled = creation_disabled;
        if self.creation_gesture.is_some() {
            self.tree
                .get_mut(CREATE_AUTOMATIC)
                .unwrap()
                .component_mut::<ButtonContent>()
                .unwrap()
                .button
                .state
                .disabled = creation_disabled;
        }
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
        let upload_gate = self.upload_gate(&binding);
        let attachment_available = self.attachment_affordance(&binding);
        let context_affordances = self.context_affordances(&binding);
        self.tree
            .get_mut(ids.root)
            .unwrap()
            .component_mut::<Composer>()
            .unwrap()
            .set_context_affordances(context_affordances);
        self.sync_run_view(
            ids,
            &binding,
            &snapshot,
            ComposerAffordances {
                attachment: attachment_available,
                model: binding.supports_model_choices(),
                ..Default::default()
            },
            upload_gate,
        )?;
        self.sync_transcript_content(ids, &binding, &snapshot)?;
        self.sync_turn_footnote_proof()?;
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
            "阅读事务已接入；完整原生验收尚未完成。\n回传诊断：{} 条内容，{} 条流式内容，历史{}同步。",
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
                    read_intent: None,
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
                projection_ready: true,
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
        input.projection_ready = self.panel_parent_is_projected(&parent);
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
        if !input.projection_ready {
            return self.sync_upload_status();
        }
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
                let summary = self
                    .tree
                    .get_mut(summary_id)
                    .expect("summary")
                    .component_mut::<SummaryView>()
                    .expect("SummaryView");
                if let Some(context) = &self.workspace_summary {
                    summary.update_context(context).map_err(unsupported)?;
                } else {
                    summary
                        .update(&SummaryInput {
                            revision: snapshot.revision,
                            scope: parent,
                            facts: snapshot.summary.clone(),
                            host_sections: vec![],
                        })
                        .map_err(unsupported)?;
                }
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
                                    .component_mut::<RunView>()
                                    .unwrap()
                                    .append_accessory(Content::new(
                                        ComponentId(ids.root.0 + 19),
                                        ButtonContent::new("刷新子代理能力"),
                                    ))
                                    .map_err(unsupported)?;
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
                        PaneSnapshot::ReadOnlyChild { .. } => {
                            let input = pane.source.readonly_input(
                                &pane.snapshot,
                                reading,
                                read_error.as_deref(),
                            )?;
                            let root = ComponentId(ids.root.0 + 16);
                            if self
                                .tree
                                .get(root)
                                .and_then(|node| {
                                    node.component::<tekes_agent_components_ui::ReadOnlyTranscript>(
                                    )
                                })
                                .is_none()
                            {
                                let namespace = self.next_document_namespace;
                                let end = namespace
                                    .checked_add(10_000)
                                    .ok_or_else(|| unsupported("read-only row pool exhausted"))?;
                                let labels =
                                    tekes_agent_components_ui::ReadOnlyTranscriptLabels::default();
                                let content = tekes_agent_components_ui::TranscriptContent::new(
                                    namespace,
                                    namespace + 1..end,
                                    binding.scope().clone(),
                                    labels.content.clone(),
                                )
                                .map_err(unsupported)?;
                                let mut view = tekes_agent_components_ui::ReadOnlyTranscript::new(
                                    tekes_agent_components_ui::ReadOnlyTranscriptIds {
                                        root,
                                        status: ComponentId(ids.root.0 + 18),
                                        refresh: ComponentId(ids.root.0 + 19),
                                        viewport: ComponentId(ids.root.0 + 21),
                                    },
                                    binding.scope().clone(),
                                    5_000_000 + (ids.root.0 >> 32) as u32,
                                    content,
                                    labels,
                                )
                                .map_err(unsupported)?;
                                view.update(input).map_err(unsupported)?;
                                let content_id = view.content_id();
                                let panel = self
                                    .tree
                                    .get_mut(panel_id)
                                    .unwrap()
                                    .component_mut::<SidePanel>()
                                    .unwrap();
                                let token = panel
                                    .content_token(&SidePanelTabKind::Subagent { child_id })
                                    .ok_or_else(|| unsupported("read-only child token missing"))?;
                                panel
                                    .install_content(&token, Content::new(root, view))
                                    .map_err(unsupported)?;
                                self.next_document_namespace = end;
                                self.document_roots.insert(ids.root, content_id);
                                let pane = self
                                    .tree
                                    .get(root)
                                    .unwrap()
                                    .component::<tekes_agent_components_ui::ReadOnlyTranscript>()
                                    .unwrap();
                                let viewport = pane.ids().viewport;
                                let items = pane
                                    .content()
                                    .row_keys()
                                    .iter()
                                    .map(|key| pane.content().row_id(key).unwrap())
                                    .collect();
                                let key = format!(
                                    "agentkit-readonly:{}",
                                    ComposerDestination::run(pane.scope().clone()).storage_key()
                                );
                                self.tree
                                    .set_scroll_reading_document(viewport, key, 0, items, |_| ())
                                    .map_err(|error| {
                                        unsupported(&format!(
                                            "read-only reading attachment: {error:?}"
                                        ))
                                    })?;
                            } else {
                                let pane = self
                                    .tree
                                    .get(root)
                                    .unwrap()
                                    .component::<tekes_agent_components_ui::ReadOnlyTranscript>()
                                    .unwrap();
                                let viewport = pane.ids().viewport;
                                let plan = pane.prepare_update(input).map_err(unsupported)?;
                                let version = self
                                    .tree
                                    .get(viewport)
                                    .unwrap()
                                    .component::<ScrollContent>()
                                    .unwrap()
                                    .reading_document_version()
                                    .expect("retained read-only reading attachment")
                                    .clone();
                                let revision =
                                    version.revision().checked_add(1).ok_or_else(|| {
                                        unsupported("read-only display revision exhausted")
                                    })?;
                                tekes_agent_components_ui::publish_readonly_reading_update(
                                    &mut self.tree,
                                    root,
                                    &version,
                                    revision,
                                    &plan,
                                )
                                .map_err(|error| {
                                    unsupported(&format!("read-only reading update: {error:?}"))
                                })?;
                            }
                        }
                        PaneSnapshot::Missing { .. } => {}
                    }
                }
            }
        }
        self.sync_upload_status()?;
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
            // The shared panel evicted this UI. Keep its binding/draft, but
            // never reuse the old Transcript/editor authority on a fresh tree.
            self.document_roots.remove(&ids.root);
            self.feedback_panes.remove(&ids.root);
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
                .install_content(&token, Self::run_controls(ids, binding.scope()))
                .map_err(unsupported)?;
        }
        self.tree
            .get_mut(ids.root)
            .expect("pane composer")
            .component_mut::<Composer>()
            .expect("Composer")
            .set_labels(Self::composer_labels(&snapshot));
        let attachment = self.attachment_affordance(&binding);
        let upload_gate = self.upload_gate(&binding);
        let context_affordances = self.context_affordances(&binding);
        self.tree
            .get_mut(ids.root)
            .unwrap()
            .component_mut::<Composer>()
            .unwrap()
            .set_context_affordances(context_affordances);
        self.sync_run_view(
            ids,
            &binding,
            &snapshot,
            ComposerAffordances {
                attachment,
                model: binding.supports_model_choices(),
                ..Default::default()
            },
            upload_gate,
        )?;
        self.sync_transcript_content(ids, &binding, &snapshot)?;
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
    /// A native host with the explicit fixed SDK reading adapter can reveal
    /// this original target after layout. The published default pin does not
    /// silently focus a row to emulate that capability.
    pub fn sidebar_visibility_target(
        &self,
    ) -> Option<tekes_agent_components_ui::SidebarVisibilityTarget> {
        self.tree
            .get(self.sidebar_id())?
            .component::<Sidebar>()?
            .visibility_target()
    }
    fn reconcile_sidebar_selection(&mut self) -> Result<()> {
        let event = tekes_agent_components_ui::SidebarSelectionNavigation {
            revision: self.navigation_revision,
            selected: self
                .current_binding()
                .map(|binding| binding.scope().address()),
        };
        self.tree
            .get_mut(self.sidebar_id())
            .expect("sidebar")
            .component_mut::<Sidebar>()
            .expect("Sidebar")
            .reconcile_navigation(&event)
            .map_err(unsupported)?;
        Ok(())
    }
    pub fn select(&mut self, index: usize) -> Result<()> {
        require(index < self.bindings.len(), "unknown demo session")?;
        require(
            !self.composing(),
            "finish or cancel composition before switching sessions",
        )?;
        self.cancel_model_choices();
        self.cancel_annotation()?;
        self.cancel_creation();
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
            self.reconcile_sidebar_selection()?;
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
            .unwrap_or_else(|| Self::new_pair(index, self.bindings[index].scope()));
        self.tree
            .insert(MAIN, content)
            .map_err(|_| unsupported("restore component pair"))?;
        self.selected = index;
        self.tree
            .get_mut(MAIN)
            .unwrap()
            .component_mut::<StackContent>()
            .unwrap()
            .flexible_child = Some(Self::pair_id(index));
        self.tree
            .reorder(
                MAIN,
                &[
                    NAV,
                    NOTICE,
                    DIAGNOSTIC,
                    RenameIds::new(RENAME_NAMESPACE).root,
                    Self::choices_id(),
                    Self::reasoning_id(),
                    Self::pair_id(index),
                ],
            )
            .map_err(|_| unsupported("demo layout order"))?;
        self.sync()?;
        self.reconcile_sidebar_selection()?;
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
        let mut disappeared = self.sidebar_source.as_ref().is_some_and(|source| {
            source.loaded
                && source.error.is_none()
                && !source.rows.iter().any(|row| row.address == request.address)
        });
        if request.revision != self.navigation_revision {
            return Ok(false);
        }
        if let Some(reader) = &self.sidebar_reader {
            require(
                binding.shares_runtime(reader.runtime()),
                "navigation binding belongs to another Sidebar runtime",
            )?;
        }
        if let Some(reader) = &self.sidebar_reader {
            let generation = self
                .navigation_revision
                .checked_add(1)
                .ok_or_else(|| unsupported("Sidebar selected generation exhausted"))?;
            disappeared = !reader
                .read_selected(&request.address, generation)?
                .has_active_row(reader);
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
    fn composing_navigation_run(&self) -> Option<&RunView> {
        let editor = self.composing_editor()?;
        [Some(self.ids()), self.panel_composer_ids()]
            .into_iter()
            .flatten()
            .find_map(|ids| {
                let run = self
                    .tree
                    .get(ComponentId(ids.root.0 + 16))?
                    .component::<RunView>()?;
                (run.ids().composer.editor == editor && run.is_pane_presented()).then_some(run)
            })
    }
    fn blocked_pointer(&self, event: &ContentEvent) -> bool {
        if !self.composing() {
            return false;
        }
        // CANDIDATE_COMPOSER_PADDING_IME_QUERY
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
            if self.composing_navigation_run().is_some_and(|run| {
                run.is_nonfocusing_navigation_at(position)
                    || run.composer().is_nonfocusing_notice_refresh_at(position)
            }) {
                return false;
            }
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
            && !self.composing_navigation_run().is_some_and(|run| {
                run.is_nonfocusing_navigation_target(target)
                    || run.composer().is_nonfocusing_notice_refresh_target(target)
            })
        {
            return Ok(vec![]);
        }
        self.tree.accessibility_action(target, part, action);
        self.drain_commands()
    }
    pub fn drain_commands(&mut self) -> Result<Vec<DemoCommand>> {
        let records = self.tree.drain_action_records();
        let actions = records
            .iter()
            .map(|record| record.action)
            .collect::<Vec<_>>();
        let output_probe = ComponentId((6_000_001_u64) << 32);
        if let Some(row) = self
            .tree
            .get_mut(output_probe)
            .and_then(|c| c.component_mut::<tekes_agent_components_ui::OutputMessage>())
        {
            row.handle_actions(&actions);
        }
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
        let composing = self.composing();
        let routed: Vec<_> = composers
            .into_iter()
            .flat_map(|(ids, binding)| {
                let run = self
                    .tree
                    .get_mut(ComponentId(ids.root.0 + 16))
                    .unwrap()
                    .component_mut::<RunView>()
                    .unwrap();
                let intents = run.take_intents(&actions);
                intents
                    .into_iter()
                    .map(move |intent| (ids, binding.clone(), intent))
            })
            .collect();
        let mut intents = vec![];
        let mut commands = vec![];
        let mut local_navigation = false;
        for (ids, binding, intent) in routed {
            use tekes_agent_components_ui::RunViewIntent;
            match intent {
                RunViewIntent::Composer(intent) => intents.push((binding, intent)),
                RunViewIntent::JumpLatest(intent) => {
                    local_navigation = true;
                    commands.push(DemoCommand::JumpLatest {
                        root: ComponentId(ids.root.0 + 16),
                        intent,
                    });
                }
                RunViewIntent::TurnNavigation(intent) => {
                    local_navigation = true;
                    commands.push(DemoCommand::TurnNavigation {
                        root: ComponentId(ids.root.0 + 16),
                        intent,
                    });
                }
                RunViewIntent::Feedback { control, gesture } => {
                    commands.push(self.prepare_feedback(ids, &binding, control, *gesture)?)
                }
                RunViewIntent::CopyTranscript(intent) => {
                    commands.push(DemoCommand::CopyTranscript(intent))
                }
                RunViewIntent::TurnAction(intent) => commands.push(DemoCommand::TurnAction {
                    binding,
                    intent: *intent,
                }),
                RunViewIntent::LoadEarlier { scope } => {
                    require(&scope == binding.scope(), "history origin mismatch")?;
                    let request = binding.begin_history_page()?;
                    // The single full sync below publishes the loading receipt
                    // together with its captured canonical Run frame.
                    commands.push(DemoCommand::ReadHistoryPage(request));
                }
                RunViewIntent::Actionable(intent) => {
                    match binding.begin_actionable_response(&intent) {
                        Ok(request) => commands.push(DemoCommand::RespondActionable {
                            request: Arc::new(request),
                            ids,
                            intent,
                        }),
                        Err(error) => {
                            self.complete_actionable(ids, &intent, Err(error.to_string()))?;
                        }
                    }
                }
            }
        }
        if !composing
            && let Some(footer) = self
                .tree
                .get_mut(Self::turn_footnote_id())
                .and_then(|node| node.component_mut::<tekes_agent_components_ui::TurnFootnote>())
        {
            for intent in footer.take_intents(&actions) {
                commands.push(DemoCommand::TurnAction {
                    binding: binding.clone(),
                    intent,
                });
            }
        }
        self.drain_uploads(&actions)?;
        self.drain_annotation(&actions)?;
        self.drain_model_choices(&actions)?;
        commands.extend(self.drain_creation(&actions)?);
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
        let sidebar_id = self.sidebar_id();
        let requests = self
            .tree
            .get(sidebar_id)
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .take_page_requests(&actions);
        for request in requests {
            self.tree
                .get_mut(sidebar_id)
                .unwrap()
                .component_mut::<Sidebar>()
                .unwrap()
                .load_more(&request)
                .map_err(unsupported)?;
        }
        let mut sidebar_intents = self
            .tree
            .get(self.sidebar_id())
            .expect("sidebar")
            .component::<Sidebar>()
            .expect("Sidebar")
            .take_intents(&actions);
        if !composing {
            let menu_intents = self
                .tree
                .get(self.sidebar_id())
                .unwrap()
                .component::<Sidebar>()
                .unwrap()
                .take_menu_intents(&records);
            for intent in menu_intents {
                match intent {
                    tekes_agent_components_ui::SidebarSessionMenuIntent::Shared(intent) => {
                        sidebar_intents.push(intent)
                    }
                    tekes_agent_components_ui::SidebarSessionMenuIntent::Preferences(change) => {
                        commands.push(DemoCommand::SidebarPreferences(change))
                    }
                }
            }
        }
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
        let option_intents = self
            .tree
            .get(self.sidebar_options_id())
            .unwrap()
            .component::<tekes_agent_components_ui::SidebarOptions>()
            .unwrap()
            .take_intents(&records);
        if !composing {
            for intent in option_intents {
                if intent.reset_paging {
                    let sidebar = self.sidebar_id();
                    self.tree
                        .get_mut(sidebar)
                        .unwrap()
                        .component_mut::<Sidebar>()
                        .unwrap()
                        .reset_paging()
                        .map_err(unsupported)?;
                }
                commands.push(DemoCommand::SidebarPreferences(intent.change));
            }
        }
        commands.extend(
            self.tree
                .get(self.sidebar_id())
                .unwrap()
                .component::<Sidebar>()
                .unwrap()
                .take_search_intents(&actions)
                .into_iter()
                .map(DemoCommand::SidebarSearch),
        );
        commands.extend(
            self.tree
                .get(self.sidebar_id())
                .unwrap()
                .component::<Sidebar>()
                .unwrap()
                .take_preference_intents(&actions)
                .into_iter()
                .map(DemoCommand::SidebarPreferences),
        );
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
                ComposerIntent::Context(intent) => {
                    if intent.action == tekes_agent_components_ui::ComposerContextAction::Remove {
                        if !binding.remove_context(&intent)? {
                            self.set_notice("草稿内容已变化，请重新操作");
                        }
                    } else {
                        if intent.action
                            == tekes_agent_components_ui::ComposerContextAction::EditAnnotation
                        {
                            if let Err(error) = self.begin_annotation(binding.clone(), &intent) {
                                self.set_notice(error.to_string());
                            }
                        } else {
                            if matches!(&intent.item,tekes_agent_components_ui::ComposerContextItem::Artifact(tekes_agent_presentation::ComposerArtifact::Attachment {content,..}) if content["type"]=="file")
                            {
                                if self.preview_gesture.is_none() {
                                    match self.begin_file_preview(binding.clone(), &intent) {
                                        Ok(request) => {
                                            commands.push(DemoCommand::ReadFilePreview { request })
                                        }
                                        Err(error) => self.set_notice(error.to_string()),
                                    }
                                }
                            } else {
                                commands.push(DemoCommand::OpenContext {
                                    binding: binding.clone(),
                                    intent: *intent,
                                });
                            }
                        }
                    }
                }
                ComposerIntent::Send { destination } => {
                    if self.upload_pending(&binding) {
                        continue;
                    }
                    if self
                        .annotation_gesture
                        .as_ref()
                        .is_some_and(|gesture| Arc::ptr_eq(&gesture.binding, &binding))
                    {
                        self.cancel_annotation()?;
                    }
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
                    if self.upload_pending(&binding) {
                        continue;
                    }
                    if entry == ComposerEntry::Attachment {
                        let request = self.begin_file_selection(binding.clone(), destination)?;
                        commands.push(DemoCommand::SelectFiles { request });
                    } else if entry == ComposerEntry::Model {
                        commands.push(self.begin_model_choices(binding.clone(), destination)?);
                    } else {
                        commands.push(DemoCommand::OpenEntry { destination, entry })
                    }
                }
            }
        }
        if !composing && let Some(root) = self.bounded_child_content_id() {
            let content = self
                .tree
                .get(root)
                .unwrap()
                .component::<tekes_agent_components_ui::TranscriptContent>()
                .unwrap();
            commands.extend(
                content
                    .take_copy_intents(&actions)
                    .into_iter()
                    .map(DemoCommand::CopyTranscript),
            );
        }
        if !self.composing()
            && let Some(SidePanelTabKind::Subagent { child_id }) =
                self.panel_selections.get(binding.scope())
            && let Some(pane) = self
                .child_panes
                .get_mut(&(binding.scope().clone(), child_id.clone()))
            && !pane.reading
            && matches!(pane.snapshot, PaneSnapshot::ReadOnlyChild { .. })
        {
            let root = ComponentId(pane.controls.ids.root.0 + 16);
            if let Some(view) = self.tree.get_mut(root).and_then(|node| {
                node.component_mut::<tekes_agent_components_ui::ReadOnlyTranscript>()
            }) && let Some(intent) = view.take_intents(&actions).into_iter().next()
            {
                require(
                    intent.scope() == pane.source.binding().scope(),
                    "read-only history origin mismatch",
                )?;
                pane.reading = true;
                commands.push(DemoCommand::ReadChild {
                    panel_parent: binding.scope().clone(),
                    source: pane.source.clone(),
                    page: intent.read()
                        == tekes_agent_components_ui::ReadOnlyTranscriptRead::Earlier,
                    read_intent: Some(intent),
                });
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
                && matches!(pane.snapshot, PaneSnapshot::Run { .. })
                && action.target == ComponentId(pane.controls.ids.root.0 + 19)
            {
                pane.reading = true;
                commands.push(DemoCommand::ReadChild {
                    panel_parent: binding.scope().clone(),
                    source: pane.source.clone(),
                    page: false,
                    read_intent: None,
                });
            }
            match action.target {
                NEW_SESSION
                    if !self.composing()
                        && self.creation_inputs.is_some()
                        && self
                            .creation_gesture
                            .as_ref()
                            .is_none_or(|gesture| gesture.pending.is_none()) =>
                {
                    commands.push(self.begin_creation()?)
                }
                CREATE_AUTOMATIC => {
                    if let Some(command) = self.capture_creation(None)? {
                        commands.push(command);
                    }
                }
                SELECT_A => self.select(0)?,
                SELECT_B => self.select(1)?,
                RECONNECT if !self.composing() => commands.push(DemoCommand::Reconnect),
                _ => {}
            }
        }
        // Local navigation must dispatch against the event's installed frame.
        // A pure refresh also advances Run authority generation; defer that
        // refresh until the host has consumed this synchronous local command.
        if !local_navigation {
            self.sync()?;
        }
        commands.extend(self.take_background_commands());
        Ok(commands)
    }
    /// Only original, currently mounted Run panes; retained unmounted panes are excluded.
    pub fn mounted_run_roots(&self) -> Vec<ComponentId> {
        self.document_roots
            .keys()
            .filter_map(|id| {
                let root = ComponentId(id.0 + 16);
                self.tree
                    .get(root)
                    .and_then(|node| node.component::<RunView>())
                    .map(|_| root)
            })
            .collect()
    }
    pub fn is_main_run(&self, root: ComponentId) -> bool {
        root == ComponentId(self.ids().root.0 + 16)
    }
    pub fn render(
        &mut self,
        scene: &mut Scene,
        text: &mut dyn ButtonTextRenderer,
        frame: Rect,
        clip: Rect,
        palette: Palette,
    ) -> std::result::Result<(), CompositionError> {
        self.tree.render(scene, text, frame, clip, palette)?;
        // Read actual successful layout, not sticky follow mode or running state.
        for root in self.mounted_run_roots() {
            let run = self.tree.get(root).unwrap().component::<RunView>().unwrap();
            let context = if self.is_main_run(root) && run.is_pane_presented() {
                run.layout_geometry().and_then(|geometry| {
                    run.transcript()
                        .and_then(|transcript| self.tree.get(transcript.id()))
                        .map(
                            |document| tekes_agent_components_ui::RunTurnNavigationContext {
                                bottom_is_visible: document.frame().max_y()
                                    - geometry.document_bottom_clearance
                                    <= geometry.visible_document.max_y() + 40.,
                            },
                        )
                })
            } else {
                None
            };
            let presented = run.is_pane_presented();
            let following = self
                .tree
                .get(run.ids().viewport)
                .unwrap()
                .component::<ScrollContent>()
                .unwrap()
                .is_following_latest();
            self.tree
                .get_mut(root)
                .unwrap()
                .component_mut::<RunView>()
                .unwrap()
                .set_turn_navigation_context(context)
                .expect("rendered Run navigation context");
            if presented {
                self.tree
                    .get_mut(root)
                    .unwrap()
                    .component_mut::<RunView>()
                    .unwrap()
                    .update_rail_follow_target(following)
                    .expect("rendered rail follow target");
            }
        }
        // CANDIDATE_COMPOSER_PADDING_BINDING
        Ok(())
    }
}

#[cfg(test)]
mod frame_consistency_tests {
    use super::*;
    use crate::fixture::Fixture;
    use std::time::Duration;
    use tekes_agent_session::SessionRuntime;
    #[tokio::test]
    async fn revoked_pointer_after_frame_capture_disables_auxiliary_controls_without_rereading() {
        let runtime = Arc::new(SessionRuntime::spawn(
            "fixture",
            Arc::new(Fixture::default().with_side_chats()),
        ));
        runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
        let a = Arc::new(
            RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
                .await
                .unwrap(),
        );
        let b = Arc::new(
            RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "b"))
                .await
                .unwrap(),
        );
        let source = SideChatBinding::new(runtime.clone());
        let chat = source
            .create(a.scope().clone(), None, "frame-chat")
            .await
            .unwrap();
        let scratch = source.open(&chat).await.unwrap();
        let binding = scratch.binding().clone();
        let frame = binding.current_run().unwrap();
        let mut view = ComponentDemo::new([a, b]).unwrap();
        let ids = ComposerIds::new(999_999_u64 << 32);
        view.tree
            .insert(
                MAIN_COLUMN,
                ComponentDemo::run_controls(ids, binding.scope()),
            )
            .unwrap();
        view.sync_run_view(
            ids,
            &binding,
            &frame,
            Default::default(),
            tekes_agent_components_ui::ComposerControlGate::OPEN,
        )
        .unwrap();
        source.close(&chat, "frame-close").await.unwrap();
        assert!(binding.current_run().is_none());
        view.sync_actionable(ids, &binding, &frame).unwrap();
        view.sync_paging(ids, &binding, &frame).unwrap();
        let paging = view
            .tree
            .get(ComponentDemo::paging_id(ids))
            .unwrap()
            .component::<tekes_agent_components_ui::TranscriptPaging>()
            .unwrap();
        assert!(!paging.facts().unwrap().source_ready);
        runtime.stop().await.unwrap();
    }
    #[tokio::test]
    async fn old_frame_cannot_pair_with_a_new_epoch_page_receipt() {
        let runtime = Arc::new(SessionRuntime::spawn(
            "fixture",
            Arc::new(Fixture::default()),
        ));
        runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
        let a = Arc::new(
            RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
                .await
                .unwrap(),
        );
        let b = Arc::new(
            RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "b"))
                .await
                .unwrap(),
        );
        let frame = a.current_run().unwrap();
        let mut view = ComponentDemo::new([a.clone(), b]).unwrap();
        runtime.reconnect().await.unwrap();
        runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
        assert_ne!(
            a.history_page_state().connection_epoch,
            frame.connection_epoch
        );
        let ids = view.ids();
        view.sync_paging(ids, &a, &frame).unwrap();
        assert!(
            !view
                .tree
                .get(ComponentDemo::paging_id(ids))
                .unwrap()
                .component::<tekes_agent_components_ui::TranscriptPaging>()
                .unwrap()
                .facts()
                .unwrap()
                .source_ready
        );
        runtime.stop().await.unwrap();
    }
}
