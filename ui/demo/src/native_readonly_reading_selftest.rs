//! Owned bounded SEP → retained read-only pane → original SDK reading/GPU.
use super::*;
use crate::acceptance_script::{Action, run_script};
use tekes_agent_components_ui::{ReadOnlyTranscript, RunPort, TranscriptContent, TurnFootnote};
use tekes_ui_controls::composition::{ContentReadingAnchorUpdate, ScrollContent};
pub(super) struct Probe {
    phase: u8,
    started: Instant,
    count: usize,
    loaded: usize,
    item: Option<ComponentId>,
    y: f32,
    revision: u64,
    child: Option<String>,
    cross_copy: bool,
    readonly_copy: bool,
    writable_copy: bool,
    editor: Option<ComponentId>,
    editor_owner: usize,
    editor_baseline: Option<tekes_ui_controls::TextEditBaseline>,
    main_draft: String,
    expected_copy: String,
    capture: Option<String>,
}
impl Probe {
    pub fn new(child: Option<String>) -> Self {
        run_script(vec![(25_000, Action::Close)]);
        let cross_copy =
            child.is_none() && std::env::var("TEKES_SELFTEST").as_deref() == Ok("text");
        let capture = if cross_copy {
            std::env::var("AGENT_CROSS_PANE_COPY_CAPTURE").ok()
        } else {
            None
        };
        if let Some(path) = &capture {
            assert!(
                std::path::Path::new(path).is_absolute() && !std::path::Path::new(path).exists(),
                "use a fresh absolute native capture"
            );
        }
        Self {
            cross_copy,
            readonly_copy: false,
            writable_copy: false,
            editor: None,
            editor_owner: 0,
            editor_baseline: None,
            main_draft: String::new(),
            expected_copy: String::new(),
            capture,
            phase: 0,
            started: Instant::now(),
            count: 0,
            loaded: 0,
            item: None,
            y: 0.,
            revision: 0,
            child,
        }
    }
    fn pane(host: &Host) -> &ReadOnlyTranscript {
        host.view
            .tree
            .get(host.view.bounded_child_view_id().unwrap())
            .unwrap()
            .component()
            .unwrap()
    }
    fn scroll(host: &Host) -> &ScrollContent {
        host.view
            .tree
            .get(Self::pane(host).ids().viewport)
            .unwrap()
            .component()
            .unwrap()
    }
    fn anchor(host: &Host) -> ComponentId {
        match Self::scroll(host).reading_result().unwrap().anchor {
            ContentReadingAnchorUpdate::Preserved(id)
            | ContentReadingAnchorUpdate::Fallback(Some(id)) => id,
            ref other => panic!("bounded reading item {other:?}"),
        }
    }
    fn relative_y(host: &Host, item: ComponentId) -> f32 {
        host.view.tree.get(item).unwrap().frame().min_y()
            - host
                .view
                .tree
                .get(Self::pane(host).ids().viewport)
                .unwrap()
                .visible()
                .unwrap()
                .min_y()
    }
    fn read(host: &mut Host, page: bool) {
        let ids = Self::pane(host).ids();
        let button = if page {
            Self::pane(host).paging_button_id()
        } else {
            ids.refresh
        };
        host.view
            .tree
            .accessibility_action(button, None, AccessibilityAction::Press);
        let commands = host.view.drain_commands().unwrap();
        assert!(
            !commands.is_empty(),
            "original bounded read control must issue a read"
        );
        host.commands(Ok(commands));
    }
    fn unchanged(&self, host: &Host) {
        assert_eq!(Self::anchor(host), self.item.unwrap());
        assert!(
            (Self::relative_y(host, self.item.unwrap()) - self.y).abs() <= 1.,
            "bounded anchor moved"
        );
        assert_eq!(host.view.editor().input.text, "主会话中文草稿");
        assert!(
            host.view
                .binding()
                .current_run()
                .unwrap()
                .submissions
                .is_empty()
        );
        assert!(host.view.panel_composer_ids().is_none());
        assert!(!Self::pane(host).content().feedback_enabled());
    }
    fn output_copy(host: &Host, content: &TranscriptContent) -> Option<(ComponentId, String)> {
        content
            .children()
            .iter()
            .filter_map(|node| node.component::<TurnFootnote>())
            .find_map(|footer| {
                let id = footer.copy_button_id()?;
                host.view.tree.get(id)?.visible()?;
                Some((id, footer.facts()?.turn_id))
            })
    }
    fn preserve_editor(&self, host: &Host) {
        let editor = self.editor.unwrap();
        assert_eq!(host.view.tree.focused(), Some(editor));
        let surface = host.view.tree.text_surface(editor).unwrap();
        assert_eq!(surface as *const _ as usize, self.editor_owner);
        assert!(surface.input.marked.is_some());
        assert_eq!(
            Some(surface.input.external_baseline()),
            self.editor_baseline
        );
        assert!(
            host.view
                .binding()
                .current_run()
                .unwrap()
                .submissions
                .is_empty()
        );
    }
    fn click_copy(&self, host: &Host, id: ComponentId) {
        let point = host.view.tree.get(id).unwrap().visible().unwrap().center();
        run_script(vec![(
            0,
            Action::Click {
                x: point.x.into(),
                y: point.y.into(),
            },
        )]);
    }
    pub fn advance(&mut self, host: &mut Host, window: &Window) {
        if self.phase == 12 || (self.phase == 5 && !self.cross_copy) {
            return;
        }
        assert!(
            self.started.elapsed() < Duration::from_secs(25),
            "bounded reading deadline {}",
            self.phase
        );
        let old = self.phase;
        match self.phase {
            0 if window.is_active() => {
                host.view
                    .select(if self.child.is_some() { 0 } else { 1 })
                    .unwrap();
                let binding = host.view.binding();
                binding
                    .edit_text(
                        &tekes_agent_components_ui::ComposerDestination::run(
                            binding.scope().clone(),
                        ),
                        "主会话中文草稿".into(),
                    )
                    .unwrap();
                host.open_fixture_child(
                    self.child.as_deref().unwrap_or("bounded-b"),
                    ChildHistoryLayout::SameSession,
                );
                self.phase = 1;
            }
            1 if host.view.bounded_child_view_id().is_some() && window.is_active() => {
                let pane = Self::pane(host);
                self.count = pane.content().row_keys().len();
                let viewport = pane.ids().viewport;
                assert!(Self::scroll(host).reading_document_version().is_some());
                assert!(Self::pane(host).paging_facts().unwrap().has_more_before);
                let position = host
                    .view
                    .tree
                    .get(viewport)
                    .unwrap()
                    .visible()
                    .unwrap()
                    .center();
                host.view.tree.event(&ContentEvent::Scroll {
                    position,
                    delta: tekes_ui_scene::Point::new(0., 160.),
                    precise: true,
                    begin: true,
                    end: true,
                    momentum: false,
                    time: 1.,
                });
                self.phase = 2;
            }
            2 => {
                self.item = Some(Self::anchor(host));
                self.y = Self::relative_y(host, self.item.unwrap());
                self.revision = Self::scroll(host)
                    .reading_document_version()
                    .unwrap()
                    .revision();
                Self::read(host, true);
                self.phase = 3;
            }
            3 if Self::scroll(host)
                .reading_document_version()
                .unwrap()
                .revision()
                > self.revision
                && !host
                    .view
                    .tree
                    .get(Self::pane(host).ids().refresh)
                    .unwrap()
                    .component::<tekes_ui_controls::composition::ButtonContent>()
                    .unwrap()
                    .button
                    .state
                    .disabled =>
            {
                eprintln!(
                    "NATIVE_BOUNDED_PAGE_STATE count={} before={} status={:?}",
                    Self::pane(host).content().row_keys().len(),
                    self.count,
                    host.view
                        .tree
                        .get(Self::pane(host).ids().status)
                        .unwrap()
                        .component::<tekes_ui_controls::composition::TextContent>()
                        .unwrap()
                        .text
                );
                if self.child.is_some() {
                    // A one-shot Native child may contain one assistant message;
                    // its older page is real turn metadata, not another UI row.
                    assert!(!Self::pane(host).paging_facts().unwrap().has_more_before);
                    assert!(Self::pane(host).content().row_keys().len() >= self.count);
                } else {
                    assert!(Self::pane(host).content().row_keys().len() > self.count);
                }
                self.unchanged(host);
                self.loaded = Self::pane(host).content().row_keys().len();
                assert!(
                    Self::scroll(host)
                        .reading_document_version()
                        .unwrap()
                        .revision()
                        > self.revision
                );
                self.revision = Self::scroll(host)
                    .reading_document_version()
                    .unwrap()
                    .revision();
                Self::read(host, false);
                self.phase = 4;
            }
            4 if Self::scroll(host)
                .reading_document_version()
                .unwrap()
                .revision()
                > self.revision
                && !host
                    .view
                    .tree
                    .get(Self::pane(host).ids().refresh)
                    .unwrap()
                    .component::<tekes_ui_controls::composition::ButtonContent>()
                    .unwrap()
                    .button
                    .state
                    .disabled =>
            {
                self.unchanged(host);
                assert_eq!(Self::pane(host).content().row_keys().len(), self.loaded);
                if self.cross_copy {
                    let editor = host.view.ids().editor;
                    self.editor = Some(editor);
                    let point = host
                        .view
                        .tree
                        .get(editor)
                        .unwrap()
                        .visible()
                        .unwrap()
                        .center();
                    run_script(vec![
                        (
                            0,
                            Action::Click {
                                x: point.x.into(),
                                y: point.y.into(),
                            },
                        ),
                        (
                            100,
                            Action::PreeditRange {
                                text: "跨面板中文组字".into(),
                                selection: 1..3,
                                replacement: None,
                            },
                        ),
                    ]);
                    self.phase = 6;
                } else {
                    self.phase = 5;
                    run_script(vec![(0, Action::Close)]);
                }
            }
            6 if host.view.editor().input.marked.is_some() => {
                self.editor_owner = host.view.editor() as *const _ as usize;
                self.editor_baseline = Some(host.view.editor().input.external_baseline());
                let Some((copy, _)) = Self::output_copy(host, Self::pane(host).content()) else {
                    return;
                };
                self.expected_copy = "子代理历史回答".into();
                assert_eq!(std::env::var("TEKES_SELFTEST").as_deref(), Ok("text"));
                window.set_clipboard_text("");
                self.click_copy(host, copy);
                self.phase = 7;
            }
            7 => {
                self.preserve_editor(host);
                if window.clipboard_text().as_deref() != Some(self.expected_copy.as_str()) {
                    return;
                }
                assert_eq!(Self::anchor(host), self.item.unwrap());
                assert!((Self::relative_y(host, self.item.unwrap()) - self.y).abs() <= 1.);
                assert_eq!(Self::pane(host).content().row_keys().len(), self.loaded);
                for id in [
                    Self::pane(host).ids().refresh,
                    Self::pane(host).paging_button_id(),
                ] {
                    assert!(
                        host.view
                            .accessibility_action(id, None, AccessibilityAction::Press)
                            .unwrap()
                            .is_empty(),
                        "copy admission must not grant read/page"
                    );
                }
                self.readonly_copy = true;
                self.main_draft = host.view.editor().input.text.clone();
                run_script(vec![(0, Action::Unmark)]);
                self.phase = 8;
            }
            8 if host.view.editor().input.marked.is_none() => {
                assert_eq!(
                    host.view.binding().current_run().unwrap().draft.text,
                    self.main_draft
                );
                host.open_fixture_child("child-b", ChildHistoryLayout::IndependentSessions);
                self.phase = 9;
            }
            9 if host.view.panel_child_controls_ready() => {
                let editor = host.view.panel_composer_ids().unwrap().editor;
                let Some(point) = host
                    .view
                    .tree
                    .get(editor)
                    .and_then(|node| node.visible())
                    .map(|frame| frame.center())
                else {
                    return;
                };
                self.editor = Some(editor);
                run_script(vec![
                    (
                        0,
                        Action::Click {
                            x: point.x.into(),
                            y: point.y.into(),
                        },
                    ),
                    (
                        100,
                        Action::PreeditRange {
                            text: "子窗口中文组字".into(),
                            selection: 1..3,
                            replacement: None,
                        },
                    ),
                ]);
                self.phase = 10;
            }
            10 if host
                .view
                .tree
                .text_surface(self.editor.unwrap())
                .is_some_and(|editor| editor.input.marked.is_some()) =>
            {
                let editor = host.view.tree.text_surface(self.editor.unwrap()).unwrap();
                self.editor_owner = editor as *const _ as usize;
                self.editor_baseline = Some(editor.input.external_baseline());
                let run = host
                    .view
                    .tree
                    .get(ComponentId(host.view.ids().root.0 + 16))
                    .unwrap()
                    .component::<RunPort>()
                    .unwrap();
                let Some((copy, turn_id)) = Self::output_copy(host, run.transcript().unwrap())
                else {
                    return;
                };
                let snapshot = host.view.binding().current_run().unwrap();
                self.expected_copy = snapshot
                    .transcript
                    .items
                    .iter()
                    .filter(|item| {
                        item.turn_id == turn_id
                            && item.kind == tekes_agent_components_ui::TranscriptItemKind::Assistant
                    })
                    .map(|item| item.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(!self.expected_copy.is_empty() && self.expected_copy != "子代理历史回答");
                window.set_clipboard_text("");
                self.click_copy(host, copy);
                self.phase = 11;
            }
            11 => {
                self.preserve_editor(host);
                if window.clipboard_text().as_deref() != Some(self.expected_copy.as_str()) {
                    return;
                }
                assert_eq!(
                    host.view.binding().current_run().unwrap().draft.text,
                    self.main_draft
                );
                let parent = host.view.binding().scope().clone();
                let child = host
                    .view
                    .cached_child_source(
                        &parent,
                        &parent,
                        "child-b",
                        ChildHistoryLayout::IndependentSessions,
                    )
                    .unwrap();
                assert!(child.binding().current_run().unwrap().draft.text.is_empty());
                assert!(
                    child
                        .binding()
                        .current_run()
                        .unwrap()
                        .submissions
                        .is_empty()
                );
                self.writable_copy = true;
                let mut actions = vec![];
                if let Some(path) = &self.capture {
                    actions.push((300, Action::Capture { path: path.clone() }));
                }
                actions.push((700, Action::Close));
                run_script(actions);
                self.phase = 12;
            }
            _ => {}
        }
        if old != self.phase {
            eprintln!("NATIVE_READONLY_READING_PHASE {}", self.phase);
            window.request_redraw();
        }
    }
    pub fn receipt(&self) -> serde_json::Value {
        assert_eq!(self.phase, if self.cross_copy { 12 } else { 5 });
        serde_json::json!({"nativeWindow":true,"realHarness":self.child.is_some(),"visibleRowsExtended":self.loaded>self.count,"historyExhausted":self.child.is_some(),"pageSize":if self.child.is_some(){1}else{50},"gpu":true,"boundedPublicSep":true,"originalPageAndRefreshControls":true,"prependAnchorPreserved":true,"refreshAnchorPreserved":true,"mainDraftPreserved":true,"readOnlyNoComposer":true,"noSubmission":true,"rowCountBefore":self.count,"rowCountAfter":self.loaded,"crossPaneCopyRequested":self.cross_copy,"readonlyCopyDuringMainPreedit":self.readonly_copy,"mainCopyDuringChildPreedit":self.writable_copy,"originalEditorOwnerAndMarkedBaseline":self.readonly_copy && self.writable_copy,"privateClipboardReadback":self.readonly_copy && self.writable_copy,"readPageStillBlockedDuringPreedit":self.readonly_copy,"syntheticPreedit":self.cross_copy,"physicalIme":false,"voiceOver":false})
    }
}
