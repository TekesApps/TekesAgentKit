//! Owned fixture: retiring/rebuilding panel UI leaves the real Run/draft alive.
use super::*;
use crate::acceptance_script::{Action, run_script};
use tekes_agent_components_ui::{
    Composer, ComposerIds, PanelContentToken, RunPort, SidePanelTabKind,
};
use tekes_ui_controls::{ReadingDocumentVersion, TextEditBaseline, composition::ScrollContent};
const DRAFT: &str = "面板独立草稿";
pub(super) struct Probe {
    phase: u8,
    started: Instant,
    ids: Option<ComposerIds>,
    document: Option<ComponentId>,
    rebuilt_document: Option<ComponentId>,
    version: Option<ReadingDocumentVersion>,
    token: Option<PanelContentToken>,
    baseline: Option<TextEditBaseline>,
    main_editor: usize,
    other_draft: String,
    chat: Option<String>,
}
impl Probe {
    pub fn new() -> Self {
        run_script(vec![(25_000, Action::Close)]);
        Self {
            phase: 0,
            started: Instant::now(),
            ids: None,
            document: None,
            rebuilt_document: None,
            version: None,
            token: None,
            baseline: None,
            main_editor: 0,
            other_draft: String::new(),
            chat: None,
        }
    }
    fn click(host: &Host, id: ComponentId) -> Action {
        let point = host.view.tree.get(id).unwrap().visible().unwrap().center();
        Action::Click {
            x: point.x as f64,
            y: point.y as f64,
        }
    }
    fn panel(host: &Host) -> &SidePanel {
        host.view
            .tree
            .get(host.view.panel_id())
            .unwrap()
            .component::<SidePanel>()
            .unwrap()
    }
    fn run<'a>(&self, host: &'a Host) -> &'a RunPort {
        host.view
            .tree
            .get(ComponentId(self.ids.unwrap().root.0 + 16))
            .unwrap()
            .component::<RunPort>()
            .unwrap()
    }
    fn version(&self, host: &Host) -> ReadingDocumentVersion {
        host.view
            .tree
            .get(self.run(host).ids().viewport)
            .unwrap()
            .component::<ScrollContent>()
            .unwrap()
            .reading_document_version()
            .unwrap()
            .clone()
    }
    fn main_marked(&self, host: &Host) {
        assert_eq!(host.view.editor() as *const _ as usize, self.main_editor);
        assert_eq!(host.view.tree.focused(), Some(host.view.ids().editor));
        assert!(host.view.editor().input.marked.is_some());
        assert_eq!(
            Some(host.view.editor().input.external_baseline()),
            self.baseline
        );
    }
    fn pointer_alive(&self, host: &Host) {
        assert!(
            host.side_chats
                .pointer(host.view.bindings()[0].scope(), self.chat.as_ref().unwrap())
                .is_ok()
        );
    }
    fn pane_draft(&self, host: &Host) {
        let snapshot = self.run(host).displayed_snapshot().unwrap();
        assert_eq!(snapshot.draft.text, DRAFT);
        assert!(snapshot.submissions.is_empty());
        assert_eq!(
            host.view
                .tree
                .get(self.ids.unwrap().root)
                .unwrap()
                .component::<Composer>()
                .unwrap()
                .editor()
                .input
                .text,
            DRAFT
        );
        self.pointer_alive(host);
    }
    pub fn advance(&mut self, host: &mut Host, window: &Window) {
        assert!(
            self.started.elapsed() < Duration::from_secs(25),
            "panel projection deadline phase {}",
            self.phase
        );
        if !window.is_active() {
            return;
        }
        let old = self.phase;
        match self.phase {
            0 if !host
                .view
                .tree
                .get(Self::panel(host).create_side_chat_id())
                .unwrap()
                .component::<tekes_ui_controls::composition::ButtonContent>()
                .unwrap()
                .button
                .state
                .disabled =>
            {
                self.other_draft = host.view.bindings()[1]
                    .current_run()
                    .unwrap()
                    .draft
                    .text
                    .clone();
                self.main_editor = host.view.editor() as *const _ as usize;
                run_script(vec![(
                    0,
                    Self::click(host, Self::panel(host).create_side_chat_id()),
                )]);
                self.phase = 1;
            }
            1 if host.view.panel_composer_ids().is_some() => {
                let ids = host.view.panel_composer_ids().unwrap();
                self.ids = Some(ids);
                run_script(vec![
                    (0, Self::click(host, ids.editor)),
                    (100, Action::Text(DRAFT.into())),
                ]);
                self.phase = 2;
            }
            2 if self
                .run(host)
                .displayed_snapshot()
                .is_some_and(|snapshot| snapshot.draft.text == DRAFT) =>
            {
                self.document = host.view.transcript_content_id(self.ids.unwrap());
                self.version = Some(self.version(host));
                let SidePanelTabKind::SideChat { session_id } = Self::panel(host)
                    .displayed_input()
                    .unwrap()
                    .selected
                    .clone()
                    .unwrap()
                else {
                    panic!("real scratch tab");
                };
                self.chat = Some(session_id.clone());
                self.token =
                    Self::panel(host).content_token(&SidePanelTabKind::SideChat { session_id });
                run_script(vec![
                    (0, Self::click(host, host.view.ids().editor)),
                    (
                        100,
                        Action::PreeditRange {
                            text: "主输入组字".into(),
                            selection: 1..3,
                            replacement: None,
                        },
                    ),
                ]);
                self.phase = 3;
            }
            3 if host.view.editor().input.marked.is_some() => {
                self.baseline = Some(host.view.editor().input.external_baseline());
                let root = host.view.panel_id();
                host.view
                    .tree
                    .get_mut(root)
                    .unwrap()
                    .component_mut::<SidePanel>()
                    .unwrap()
                    .set_layout(tekes_agent_components_ui::SidePanelLayout {
                        content_top_inset: 28.,
                    })
                    .unwrap();
                let parent = host.view.binding().scope().clone();
                host.view
                    .set_panel_parent_projection_ready(&parent, false)
                    .unwrap();
                self.phase = 4;
            }
            4 => {
                self.main_marked(host);
                self.pointer_alive(host);
                assert!(
                    !Self::panel(host)
                        .displayed_input()
                        .unwrap()
                        .projection_ready
                );
                let slot = host
                    .view
                    .tree
                    .get(Self::panel(host).content_slot_id())
                    .unwrap();
                assert!(
                    !host
                        .view
                        .tree
                        .semantics()
                        .iter()
                        .any(|node| node.parent == Some(slot.id()))
                );
                assert!(host.view.tree.get(self.ids.unwrap().root).is_none());
                let parent = host.view.binding().scope().clone();
                host.view
                    .set_panel_parent_projection_ready(&parent, true)
                    .unwrap();
                self.phase = 5;
            }
            5 => {
                self.main_marked(host);
                self.pane_draft(host);
                let panel = Self::panel(host);
                let geometry = panel.layout_geometry().unwrap();
                assert_eq!(geometry.canvas.min_y() - geometry.pane.min_y(), 28.);
                let slot_frame = host.view.tree.get(panel.content_slot_id()).unwrap().frame();
                assert_eq!(
                    geometry.body.min_x() - slot_frame.min_x(),
                    tekes_ui_tokens::Spacing::Regular.value()
                );
                assert_eq!(
                    geometry.body.min_y() - slot_frame.min_y(),
                    tekes_ui_tokens::Spacing::Regular.value()
                );
                assert_ne!(Some(self.version(host)), self.version);
                self.rebuilt_document = host.view.transcript_content_id(self.ids.unwrap());
                assert_ne!(self.rebuilt_document, self.document);
                assert!(host.view.tree.get(self.document.unwrap()).is_none());
                let root = host.view.panel_id();
                assert!(
                    host.view
                        .tree
                        .get_mut(root)
                        .unwrap()
                        .component_mut::<SidePanel>()
                        .unwrap()
                        .install_content(
                            self.token.as_ref().unwrap(),
                            tekes_ui_controls::composition::Content::new(
                                ComponentId(123456789),
                                tekes_ui_controls::composition::TextContent::new(
                                    "late retired content"
                                )
                            )
                        )
                        .is_err()
                );
                run_script(vec![(0, Action::Unmark)]);
                self.phase = 6;
            }
            6 if host.view.editor().input.marked.is_none()
                && host
                    .view
                    .binding()
                    .current_run()
                    .unwrap()
                    .draft
                    .text
                    .contains("主输入组字") =>
            {
                self.pane_draft(host);
                run_script(vec![(
                    0,
                    Self::click(host, tekes_agent_ui_demo::component_demo::SELECT_B),
                )]);
                self.phase = 7;
            }
            7 if host.view.selected() == 1 => {
                assert!(host.view.tree.get(self.ids.unwrap().root).is_none());
                assert_eq!(host.view.editor().input.text, self.other_draft);
                self.pointer_alive(host);
                run_script(vec![(
                    0,
                    Self::click(host, tekes_agent_ui_demo::component_demo::SELECT_A),
                )]);
                self.phase = 8;
            }
            8 if host.view.selected() == 0 => {
                self.pane_draft(host);
                assert_ne!(
                    host.view.transcript_content_id(self.ids.unwrap()),
                    self.rebuilt_document
                );
                assert!(host.view.editor().input.text.contains("主输入组字"));
                assert!(
                    host.view
                        .binding()
                        .current_run()
                        .unwrap()
                        .submissions
                        .is_empty()
                );
                let mut actions = vec![];
                if let Ok(path) = std::env::var("AGENT_PANEL_PROJECTION_CAPTURE") {
                    actions.push((0, Action::Capture { path }));
                }
                actions.push((500, Action::Close));
                run_script(actions);
                self.phase = 9;
            }
            _ => {}
        }
        if old != self.phase {
            eprintln!("NATIVE_PANEL_PROJECTION_PHASE {}", self.phase);
            window.request_redraw();
        }
    }
    pub fn receipt(&self) -> serde_json::Value {
        assert_eq!(self.phase, 9);
        serde_json::json!({"nativeWindow":true,"hostTopInsetAndBodySpacing":true,"originalSideChatCreate":true,"projectionWaitingBodyEmpty":true,"retiredContentTokenRejected":true,"mainFocusAndMarkedBaselinePreserved":true,"freshPaneAndReadingAuthority":true,"threadChangeEvictsPane":true,"draftsPreserved":true,"scratchPointerAlive":true,"noSubmission":true,"physicalIme":false,"voiceOver":false,"persistentReadingCheckpoint":false})
    }
}
