//! Actual native/GPU layout over an owned local SEP. Wheel and preedit are
//! synthetic; the page travels through the public binding and original request.
use super::*;
use tekes_agent_components_ui::RunView;
use tekes_ui_controls::composition::{ContentReadingAnchorUpdate, ScrollContent};
use tekes_ui_platform_macos::selftest::{Action, run_script};

pub(super) struct Probe {
    gate: Arc<tokio::sync::Semaphore>,
    phase: u8,
    started: Instant,
    anchor: Option<ComponentId>,
    y: f32,
    count: usize,
    baseline: Option<tekes_ui_controls::TextEditBaseline>,
    editor: Option<ComponentId>,
    latest_turn: Option<String>,
}
impl Probe {
    pub fn new(gate: Arc<tokio::sync::Semaphore>) -> Self {
        run_script(vec![(20_000, Action::Close)]);
        Self {
            gate,
            phase: 0,
            started: Instant::now(),
            anchor: None,
            y: 0.,
            count: 0,
            baseline: None,
            editor: None,
            latest_turn: None,
        }
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
    fn unchanged(&self, host: &Host) {
        let anchor = self.anchor.unwrap();
        assert_eq!(Self::anchor(host), anchor);
        let y = Self::relative_y(host, anchor);
        assert!(
            (y - self.y).abs() <= 1.,
            "reading anchor {anchor:?}: {} -> {y}; result={:?}",
            self.y,
            Self::scroll(host).reading_result()
        );
        assert!(!Self::scroll(host).is_following_latest());
        assert_eq!(host.view.tree.focused(), self.editor);
        assert_eq!(
            host.view.editor().input.external_baseline(),
            *self.baseline.as_ref().unwrap()
        );
        assert!(host.view.editor().input.marked.is_some());
    }
    pub fn advance(&mut self, host: &mut Host, window: &Window) {
        assert!(
            self.started.elapsed() < Duration::from_secs(20),
            "reading deadline phase {}",
            self.phase
        );
        let previous_phase = self.phase;
        match self.phase {
            0 if window.is_active() => {
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
                // Admit the explicit page before preedit disables its control.
                let button = host.view.history_page_button().unwrap();
                host.view
                    .tree
                    .accessibility_action(button, None, AccessibilityAction::Press);
                let commands = host.view.drain_commands();
                let commands = commands.expect("original page intent");
                assert_eq!(
                    commands
                        .iter()
                        .filter(|c| matches!(c, DemoCommand::ReadHistoryPage(_)))
                        .count(),
                    1
                );
                host.commands(Ok(commands));
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
                let point = host
                    .view
                    .tree
                    .get(ids.editor)
                    .unwrap()
                    .visible()
                    .unwrap()
                    .center();
                run_script(vec![
                    (
                        0,
                        Action::Click {
                            x: point.x as f64,
                            y: point.y as f64,
                        },
                    ),
                    (
                        150,
                        Action::PreeditRange {
                            text: "分页期间中文组字".into(),
                            selection: 8..8,
                            replacement: None,
                        },
                    ),
                ]);
                self.editor = Some(ids.editor);
                self.phase = 1;
            }
            1 if host.view.editor().input.marked.is_some() => {
                self.anchor = Some(Self::anchor(host));
                self.y = Self::relative_y(host, self.anchor.unwrap());
                self.baseline = Some(host.view.editor().input.external_baseline());
                assert!(!Self::scroll(host).is_following_latest());
                self.gate.add_permits(1);
                self.phase = 2;
            }
            2 if host
                .view
                .binding()
                .current_run()
                .unwrap()
                .transcript
                .items
                .len()
                > self.count =>
            {
                self.unchanged(host);
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
                run_script(vec![(0, Action::Close)]);
            }
            _ => {}
        }
        // Native text callbacks and binding observers wake the next frame.
        // Do not busy-render while waiting for activation, input or the page.
        if self.phase != previous_phase {
            window.request_redraw();
        }
    }
    pub fn receipt(&self) -> serde_json::Value {
        assert_eq!(self.phase, 4, "native reading probe did not complete");
        serde_json::json!({"nativeWindow":true,"publicHistoryPage":true,
            "prependAnchorPreserved":true,"upwardReadingPreserved":true,
            "focusAndMarkedBaselinePreserved":true,"samePublicationRefresh":true,
            "noSubmission":true,"syntheticWheel":true,"syntheticPreedit":true,
            "realHarness":false,"physicalIme":false,"voiceOver":false})
    }
}
