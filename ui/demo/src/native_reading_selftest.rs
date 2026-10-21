//! Actual native/GPU layout over an owned local SEP. Wheel and preedit are
//! synthetic; the page travels through the public binding and original request.
use super::*;
use tekes_agent_components_ui::RunView;
use tekes_ui_controls::composition::{ContentReadingAnchorUpdate, ScrollContent};
use tekes_ui_platform_macos::selftest::{Action, run_script};

pub(super) struct Probe {
    gate: Option<Arc<tokio::sync::Semaphore>>,
    phase: u8,
    started: Instant,
    anchor: Option<ComponentId>,
    y: f32,
    count: usize,
    loaded_count: usize,
    a_draft: String,
    b_draft: String,
    baseline: Option<tekes_ui_controls::TextEditBaseline>,
    editor: Option<ComponentId>,
    latest_turn: Option<String>,
    a_editor: usize,
    b_anchor: Option<ComponentId>,
    b_y: f32,
    epoch: u64,
    a_key: Option<tekes_agent_components_ui::TranscriptContentRowKey>,
    page_selection: std::ops::Range<usize>,
}
impl Probe {
    pub fn new(gate: Option<Arc<tokio::sync::Semaphore>>) -> Self {
        run_script(vec![(20_000, Action::Close)]);
        Self {
            gate: gate.clone(),
            phase: 0,
            started: Instant::now(),
            anchor: None,
            y: 0.,
            count: 0,
            loaded_count: 0,
            a_draft: if gate.is_some() {
                "分页期间中文组字"
            } else {
                "A reading draft"
            }
            .into(),
            b_draft: if gate.is_some() {
                ""
            } else {
                "B reading draft"
            }
            .into(),
            baseline: None,
            editor: None,
            latest_turn: None,
            a_editor: 0,
            b_anchor: None,
            b_y: 0.,
            epoch: 0,
            a_key: None,
            page_selection: 0..0,
        }
    }
    pub fn prepare_harness(view: &ComponentDemo) {
        assert_eq!(view.bindings().len(), 2);
        for (index, (binding, draft)) in view
            .bindings()
            .iter()
            .zip(["A reading draft", "B reading draft"])
            .enumerate()
        {
            let run = binding.current_run().unwrap();
            assert!(
                run.draft.text.is_empty(),
                "reading probe needs owned empty drafts"
            );
            assert!(
                !run.transcript.items.is_empty(),
                "seed both owned histories"
            );
            if index == 0 {
                assert!(run.transcript.has_more_before, "seed pageable A history");
            }
            binding
                .edit_text(
                    &tekes_agent_presentation::ComposerDestination::run(binding.scope().clone()),
                    draft.into(),
                )
                .unwrap();
        }
    }
    fn admit_page(host: &mut Host, use_control: bool) {
        if !use_control {
            // Explicit host read on the original public binding. Pressing the
            // paging button legitimately changes keyboard focus to that button;
            // this path measures publication while the Composer keeps focus.
            let request = host.view.binding().begin_history_page().unwrap();
            host.commands(Ok(vec![DemoCommand::ReadHistoryPage(request)]));
            return;
        }
        let button = host.view.history_page_button().unwrap();
        host.view
            .tree
            .accessibility_action(button, None, AccessibilityAction::Press);
        let commands = host.view.drain_commands().expect("original page intent");
        assert_eq!(
            commands
                .iter()
                .filter(|c| matches!(c, DemoCommand::ReadHistoryPage(_)))
                .count(),
            1
        );
        host.commands(Ok(commands));
    }
    fn scroll(host: &Host) -> &ScrollContent {
        let run = host
            .view
            .tree
            .get(ComponentId(host.view.ids().root.0 + 16))
            .unwrap()
            .component::<RunView>()
            .unwrap();
        host.view
            .tree
            .get(run.ids().viewport)
            .unwrap()
            .component::<ScrollContent>()
            .unwrap()
    }
    fn anchor(host: &Host) -> ComponentId {
        match Self::scroll(host)
            .reading_result()
            .expect("completed native reading layout")
            .anchor
        {
            ContentReadingAnchorUpdate::Preserved(id)
            | ContentReadingAnchorUpdate::Fallback(Some(id)) => id,
            ref other => panic!("expected actual reading item: {other:?}"),
        }
    }
    fn relative_y(host: &Host, item: ComponentId) -> f32 {
        let viewport = host
            .view
            .tree
            .get(ComponentId(host.view.ids().root.0 + 16))
            .unwrap()
            .component::<RunView>()
            .unwrap()
            .ids()
            .viewport;
        host.view.tree.get(item).unwrap().frame().min_y()
            - host
                .view
                .tree
                .get(viewport)
                .unwrap()
                .visible()
                .unwrap()
                .min_y()
    }
    fn reading_unchanged(&self, host: &Host) {
        let anchor = self.anchor.unwrap();
        let run = host
            .view
            .tree
            .get(ComponentId(host.view.ids().root.0 + 16))
            .unwrap()
            .component::<RunView>()
            .unwrap();
        assert_eq!(
            Self::anchor(host),
            anchor,
            "expected key {:?}, current id {:?}, reading={:?}, source={:?}, displayed={:?}",
            self.a_key,
            self.a_key
                .as_ref()
                .and_then(|key| run.transcript().unwrap().row_id(key)),
            Self::scroll(host).reading_result(),
            host.view.binding().current_run().map(|s| (
                s.revision,
                s.connection_epoch,
                s.transcript.is_stale
            )),
            run.displayed_snapshot().map(|s| (
                s.revision,
                s.connection_epoch,
                s.transcript.is_stale
            ))
        );
        let y = Self::relative_y(host, anchor);
        assert!(
            (y - self.y).abs() <= 1.,
            "reading anchor {anchor:?}: {} -> {y}; result={:?}",
            self.y,
            Self::scroll(host).reading_result()
        );
        assert!(!Self::scroll(host).is_following_latest());
    }
    fn unchanged(&self, host: &Host) {
        self.reading_unchanged(host);
        assert_eq!(host.view.tree.focused(), self.editor);
        if self.gate.is_some() {
            assert_eq!(
                host.view.editor().input.external_baseline(),
                *self.baseline.as_ref().unwrap()
            );
        } else {
            // An authority acknowledgement or native blur can retire an opaque
            // editing baseline without editing text. Check user state and the
            // retained owner; do not treat that token as a serialized draft.
            assert_eq!(host.view.editor() as *const _ as usize, self.a_editor);
            assert_eq!(host.view.editor().input.text, self.a_draft);
            assert_eq!(host.view.editor().input.selection(), self.page_selection);
        }
        assert_eq!(
            host.view.editor().input.marked.is_some(),
            self.gate.is_some()
        );
    }
    fn click(host: &Host, id: ComponentId) -> Action {
        let node = host.view.tree.get(id).unwrap();
        let point = node.frame().center();
        assert!(node.visible().is_some_and(|clip| clip.contains(point)));
        Action::Click {
            x: point.x as f64,
            y: point.y as f64,
        }
    }
    fn restored_a(&self, host: &Host) {
        self.reading_unchanged(host);
        assert_eq!(host.view.editor() as *const _ as usize, self.a_editor);
        assert_eq!(host.view.editor().input.text, self.a_draft);
        assert!(host.view.editor().input.marked.is_none());
        assert!(
            host.view
                .binding()
                .current_run()
                .unwrap()
                .submissions
                .is_empty()
        );
    }
    fn displayed_run(host: &Host) -> &tekes_agent_presentation::RunSnapshot {
        host.view
            .tree
            .get(ComponentId(host.view.ids().root.0 + 16))
            .unwrap()
            .component::<RunView>()
            .unwrap()
            .displayed_snapshot()
            .unwrap()
    }
    pub fn advance(&mut self, host: &mut Host, window: &Window) {
        assert!(
            self.started.elapsed() < Duration::from_secs(20),
            "reading deadline phase {}",
            self.phase
        );
        let previous_phase = self.phase;
        match self.phase {
            0 if window.is_active()
                && (self.gate.is_some() || host.view.editor().input.text == self.a_draft) =>
            {
                self.a_editor = host.view.editor() as *const _ as usize;
                let ids = host.view.ids();
                let viewport = host
                    .view
                    .tree
                    .get(ComponentId(ids.root.0 + 16))
                    .unwrap()
                    .component::<RunView>()
                    .unwrap()
                    .ids()
                    .viewport;
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
                    delta: tekes_ui_scene::Point::new(0., 420.),
                    precise: true,
                    begin: true,
                    end: true,
                    momentum: false,
                    time: 1.,
                });
                self.count = host
                    .view
                    .binding()
                    .current_run()
                    .unwrap()
                    .transcript
                    .items
                    .len();
                self.latest_turn = host
                    .view
                    .binding()
                    .current_run()
                    .unwrap()
                    .transcript
                    .turns
                    .last()
                    .map(|turn| turn.id.clone());
                if self.gate.is_some() {
                    Self::admit_page(host, true);
                    run_script(vec![
                        (0, Self::click(host, ids.editor)),
                        (
                            150,
                            Action::PreeditRange {
                                text: self.a_draft.clone(),
                                selection: 8..8,
                                replacement: None,
                            },
                        ),
                    ]);
                }
                self.editor = Some(ids.editor);
                self.phase = 1;
            }
            1 if self.gate.is_none() || host.view.editor().input.marked.is_some() => {
                self.anchor = Some(Self::anchor(host));
                let content = host
                    .view
                    .tree
                    .get(ComponentId(host.view.ids().root.0 + 16))
                    .unwrap()
                    .component::<RunView>()
                    .unwrap()
                    .transcript()
                    .unwrap();
                self.a_key = content
                    .row_keys()
                    .iter()
                    .find(|key| content.row_id(key) == self.anchor)
                    .cloned();
                self.y = Self::relative_y(host, self.anchor.unwrap());
                self.baseline = Some(host.view.editor().input.external_baseline());
                self.page_selection = host.view.editor().input.selection();
                assert!(!Self::scroll(host).is_following_latest());
                if let Some(gate) = &self.gate {
                    gate.add_permits(1);
                } else {
                    Self::admit_page(host, false);
                }
                self.phase = 2;
            }
            2 if window.is_active()
                && Self::displayed_run(host).transcript.items.len() > self.count =>
            {
                self.unchanged(host);
                self.loaded_count = Self::displayed_run(host).transcript.items.len();
                assert!(matches!(
                    host.view.binding().history_page_state().phase,
                    tekes_agent_ui_bindings::HistoryPagePhase::Loaded { extended: true }
                ));
                // Same-provider-revision control refreshes must also use a new
                // display revision and retain the original editor transaction.
                for _ in 0..3 {
                    host.view.sync().expect("same-publication refresh");
                }
                self.phase = 3;
            }
            3 => {
                self.unchanged(host);
                let snapshot = host.view.binding().current_run().unwrap();
                assert!(snapshot.submissions.is_empty(), "page must not submit");
                assert_eq!(
                    snapshot.transcript.turns.last().map(|turn| &turn.id),
                    self.latest_turn.as_ref()
                );
                self.phase = 4;
                if self.gate.is_some() {
                    run_script(vec![(0, Action::Unmark)]);
                }
            }
            4 if window.is_active()
                && !host.view.composing()
                && host.view.binding().current_run().unwrap().draft.text == self.a_draft =>
            {
                self.reading_unchanged(host);
                run_script(vec![(
                    0,
                    Self::click(host, tekes_agent_ui_demo::component_demo::SELECT_B),
                )]);
                self.phase = 5;
            }
            5 if host.view.selected() == 1 => {
                assert_eq!(
                    host.view.editor().input.text,
                    self.b_draft,
                    "B cannot inherit A draft"
                );
                let viewport = host
                    .view
                    .tree
                    .get(ComponentId(host.view.ids().root.0 + 16))
                    .unwrap()
                    .component::<RunView>()
                    .unwrap()
                    .ids()
                    .viewport;
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
                    delta: tekes_ui_scene::Point::new(0., 180.),
                    precise: true,
                    begin: true,
                    end: true,
                    momentum: false,
                    time: 2.,
                });
                self.phase = 6;
            }
            6 if window.is_active() => {
                self.b_anchor = Some(Self::anchor(host));
                self.b_y = Self::relative_y(host, self.b_anchor.unwrap());
                assert_ne!(self.b_anchor, self.anchor);
                run_script(vec![(
                    0,
                    Self::click(host, tekes_agent_ui_demo::component_demo::SELECT_A),
                )]);
                self.phase = 7;
            }
            7 if host.view.selected() == 0 => {
                self.restored_a(host);
                self.epoch = host.view.binding().current_run().unwrap().connection_epoch;
                host.commands(Ok(vec![DemoCommand::Reconnect]));
                self.phase = 8;
            }
            8 if window.is_active()
                && Self::displayed_run(host).connection_epoch > self.epoch
                && !Self::displayed_run(host).transcript.is_stale =>
            {
                self.restored_a(host);
                run_script(vec![(
                    0,
                    Self::click(host, tekes_agent_ui_demo::component_demo::SELECT_B),
                )]);
                self.phase = 9;
            }
            9 if window.is_active() && host.view.selected() == 1 => {
                assert_eq!(Self::anchor(host), self.b_anchor.unwrap());
                assert!((Self::relative_y(host, self.b_anchor.unwrap()) - self.b_y).abs() <= 1.);
                assert_eq!(host.view.editor().input.text, self.b_draft);
                assert!(
                    host.view
                        .binding()
                        .current_run()
                        .unwrap()
                        .submissions
                        .is_empty()
                );
                run_script(vec![(
                    0,
                    Self::click(host, tekes_agent_ui_demo::component_demo::SELECT_A),
                )]);
                self.phase = 10;
            }
            10 if host.view.selected() == 0 => {
                self.restored_a(host);
                self.phase = 11;
                run_script(vec![(0, Action::Close)]);
            }
            _ => {}
        }
        // Native text callbacks and binding observers wake the next frame.
        // Do not busy-render while waiting for activation, input or the page.
        if self.phase != previous_phase {
            eprintln!("NATIVE_READING_PHASE {}", self.phase);
            window.request_redraw();
        }
    }
    pub fn receipt(&self) -> serde_json::Value {
        assert_eq!(self.phase, 11, "native reading probe did not complete");
        serde_json::json!({"nativeWindow":true,"publicHistoryPage":true,
            "prependAnchorPreserved":true,"upwardReadingPreserved":true,
            "focusAndMarkedBaselinePreserved":self.gate.is_some(),"samePublicationRefresh":true,
            "focusAndEditorBaselinePreserved":self.gate.is_some(),"markedTextPreserved":self.gate.is_some(),
            "focusAndTextStatePreserved":true,"opaqueBaselinePreserved":self.gate.is_some(),
            "retainedSessionReadingRestored":true,"independentBReadingRestored":true,
            "reconnectReadingRestored":true,"draftIsolation":true,"sameEditorOwner":true,
            "noSubmission":true,"syntheticWheel":true,"syntheticPreedit":self.gate.is_some(),
            "realHarness":self.gate.is_none(),"readingAnchorAcceptance":true,
            "nativePagingControl":self.gate.is_some(),"publicPageRequest":true,
            "loadedItemsBefore":self.count,"loadedItemsAfter":self.loaded_count,"physicalIme":false,"voiceOver":false})
    }
}
