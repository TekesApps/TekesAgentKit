//! Native original Summary controls over public owned SEP input.
use super::*;
use tekes_agent_components_ui::{SummaryActionRow, SummaryDefaultRowIds, SummaryView};
use tekes_agent_ui_demo::fixture::Fixture;
use tekes_ui_controls::ButtonTextRenderer;
use tekes_ui_platform_macos::selftest::{Action, run_script};
use tekes_ui_tokens::{Spacing, proposal};
const CHILD: &str = "b437cf34-5661-45e2-a674-113b27c92f89";
pub(super) struct Probe {
    fixture: Fixture,
    phase: u8,
    started: Instant,
    resized_at: Option<Instant>,
    passive: Option<SummaryDefaultRowIds>,
    action: Option<SummaryDefaultRowIds>,
    field: usize,
    control: usize,
    editor: usize,
    baseline: Option<tekes_ui_controls::TextEditBaseline>,
    capture: Option<String>,
}
impl Probe {
    pub fn new(fixture: Fixture) -> Self {
        run_script(vec![(25000, Action::Close)]);
        let capture = std::env::var("AGENT_SUMMARY_ROW_CAPTURE").ok();
        if let Some(path) = &capture {
            assert!(
                !std::path::Path::new(path).exists(),
                "use a new capture path"
            );
        }
        Self {
            fixture,
            phase: 0,
            started: Instant::now(),
            resized_at: None,
            passive: None,
            action: None,
            field: 0,
            control: 0,
            editor: 0,
            baseline: None,
            capture,
        }
    }
    fn summary(host: &Host) -> &SummaryView {
        host.view
            .tree
            .get(host.view.summary_id())
            .unwrap()
            .component()
            .unwrap()
    }
    fn click(host: &Host, id: ComponentId) -> Action {
        let p = host
            .view
            .tree
            .get(id)
            .unwrap()
            .visible()
            .expect("original row/editor must be visible")
            .center();
        Action::Click {
            x: p.x.into(),
            y: p.y.into(),
        }
    }
    fn geometry(&self, host: &mut Host) {
        let line = host
            .text
            .measure("Mg", proposal::TextRole::UiBody, 1e8)
            .height;
        for ids in [self.passive.unwrap(), self.action.unwrap()] {
            let band = host.view.tree.get(ids.row).unwrap().frame();
            let title = host.view.tree.get(ids.title).unwrap().frame();
            assert!((title.min_x() - band.min_x() - 2. * Spacing::Regular.value()).abs() < 0.1);
            assert!(
                (band.size.height - title.size.height - 2. * Spacing::Regular.value()).abs() < 0.1
            );
            assert!(title.size.height > line, "owned title must wrap");
        }
        for (section, ids) in [
            ("tasks", self.passive.unwrap()),
            ("subagents", self.action.unwrap()),
        ] {
            let header = Self::summary(host)
                .section_header_label_id(section)
                .unwrap();
            assert!(
                (host.view.tree.get(header).unwrap().frame().min_x()
                    - host.view.tree.get(ids.title).unwrap().frame().min_x())
                .abs()
                    < 0.1
            );
        }
        let ids = Self::summary(host).usage_row_ids("total").unwrap();
        let band = host.view.tree.get(ids.row).unwrap().frame();
        let title = host.view.tree.get(ids.title).unwrap().frame();
        let value = host.view.tree.get(ids.value).unwrap().frame();
        assert!((value.max_x() - band.max_x() + 2. * Spacing::Regular.value()).abs() < 0.1);
        assert!((value.min_y() - title.min_y() - (line - value.size.height) / 2.).abs() < 0.1);
        let shaped = host.text.last_label.as_ref().unwrap();
        assert_eq!(shaped.paragraph().lines.len(), 1);
    }
    fn unchanged(&self, host: &Host) {
        assert_eq!(
            host.view
                .tree
                .get(self.action.unwrap().row)
                .unwrap()
                .component::<SummaryActionRow>()
                .unwrap()
                .button() as *const _ as usize,
            self.control
        );
        let field = host
            .view
            .tree
            .text_surface(self.passive.unwrap().title)
            .unwrap();
        assert_eq!(field as *const _ as usize, self.field);
        assert_eq!(field.input.selected_text(), "静态");
        assert_eq!(host.view.editor() as *const _ as usize, self.editor);
        assert_eq!(host.view.tree.focused(), Some(host.view.ids().editor));
        assert_eq!(
            Some(host.view.editor().input.external_baseline()),
            self.baseline
        );
        assert!(host.view.editor().input.marked.is_some());
        assert!(
            host.view
                .binding()
                .current_run()
                .unwrap()
                .submissions
                .is_empty()
        );
    }
    pub fn advance(&mut self, host: &mut Host, window: &Window) {
        assert!(
            self.started.elapsed() < Duration::from_secs(25),
            "row layout deadline phase {}",
            self.phase
        );
        let old = self.phase;
        match self.phase {
            0 if window.is_active() => {
                let run = host.view.binding().current_run().unwrap();
                let Some(facts) = &run.summary else {
                    return;
                };
                let Some(child) = facts
                    .subagents
                    .iter()
                    .find(|c| c.session_id.as_deref() == Some(CHILD))
                else {
                    return;
                };
                let Some(task) = facts.tasks.first() else {
                    return;
                };
                self.action = Self::summary(host).default_row_ids("subagents", &child.id);
                self.passive = Self::summary(host).default_row_ids("tasks", &task.id);
                if self.action.is_none() || self.passive.is_none() {
                    return;
                }
                self.geometry(host);
                self.control = host
                    .view
                    .tree
                    .get(self.action.unwrap().row)
                    .unwrap()
                    .component::<SummaryActionRow>()
                    .unwrap()
                    .button() as *const _ as usize;
                self.field = host
                    .view
                    .tree
                    .text_surface(self.passive.unwrap().title)
                    .unwrap() as *const _ as usize;
                run_script(vec![(0, Self::click(host, self.passive.unwrap().title))]);
                self.phase = 1;
            }
            1 if host.view.tree.focused() == Some(self.passive.unwrap().title) => {
                host.view
                    .accessibility_action(
                        self.passive.unwrap().title,
                        None,
                        AccessibilityAction::SetSelection(0..2),
                    )
                    .unwrap();
                assert_eq!(
                    host.view
                        .tree
                        .text_surface(self.passive.unwrap().title)
                        .unwrap()
                        .input
                        .selected_text(),
                    "静态"
                );
                run_script(vec![
                    (0, Self::click(host, host.view.ids().editor)),
                    (
                        40,
                        Action::PreeditRange {
                            text: "布局更新期间中文组字".into(),
                            selection: 1..3,
                            replacement: None,
                        },
                    ),
                ]);
                self.phase = 2;
            }
            2 if host.view.editor().input.marked.is_some() => {
                self.editor = host.view.editor() as *const _ as usize;
                self.baseline = Some(host.view.editor().input.external_baseline());
                self.fixture.publish_inventory_summary(serde_json::json!({"sessionId":CHILD,"title":"Summary child","status":"running","running":true,"blank":true,"updatedAt":2})).unwrap();
                self.phase = 3;
            }
            3 if Self::summary(host)
                .displayed_context_input()
                .and_then(|s| s.facts.as_ref())
                .is_some_and(|s| {
                    s.subagents
                        .iter()
                        .any(|c| c.status == "running" && c.session_id.as_deref() == Some(CHILD))
                }) =>
            {
                self.geometry(host);
                self.unchanged(host);
                self.resized_at = Some(Instant::now());
                run_script(vec![
                    (0, Action::BeginLiveResize),
                    (20, Action::Resize { w: 1000., h: 740. }),
                    (80, Action::EndLiveResize),
                    (200, Action::WakeRedraw),
                ]);
                self.phase = 4;
            }
            4 if self.resized_at.unwrap().elapsed() >= Duration::from_millis(180) => {
                assert!((window.size().width - 1000.).abs() < 1.);
                self.geometry(host);
                self.unchanged(host);
                host.text.type_scale = 1.4;
                self.phase = 5;
            }
            5 => {
                self.geometry(host);
                self.unchanged(host);
                let mut actions = vec![];
                if let Some(path) = &self.capture {
                    actions.push((300, Action::Capture { path: path.clone() }));
                }
                actions.push((700, Action::Close));
                run_script(actions);
                self.phase = 6;
            }
            _ => {}
        }
        if old != self.phase {
            eprintln!("NATIVE_SUMMARY_ROW_LAYOUT_PHASE {}", self.phase);
            window.request_redraw();
        }
    }
    pub fn receipt(&self) -> serde_json::Value {
        assert_eq!(self.phase, 6);
        serde_json::json!({"nativeWindow":true,"ownedSepFacts":true,"staticAndActionTitlesWrap":true,"doubleRegularInset":true,"headerTitleAnchorAligned":true,"metadataFirstLineAndSingleLine":true,
            "staticSelectionAndControlRetained":true,"originalActionRetained":true,"originalComposerFocusAndMarkedBaseline":true,"resizeAndTypeScale":true,
            "noSubmission":true,"syntheticPreedit":true,"physicalIme":false,"physicalResizeDrag":false,"voiceOver":false,"statusDotRendered":false,"fullDefaultRowParity":false,"pairedClientKitVisual":false,"realHarness":false})
    }
}
