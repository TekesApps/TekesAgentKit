//! Public SEP inventory -> existing presentation -> retained original native Summary.
use super::*;
use tekes_agent_components_ui::{SummaryActionRow, SummarySubagentIndicator, SummaryView};
use tekes_agent_ui_demo::fixture::Fixture;
use tekes_ui_platform_macos::selftest::{Action, run_script};
const CHILD: &str = "b437cf34-5661-45e2-a674-113b27c92f89";
pub(super) struct Probe {
    fixture: Fixture,
    phase: u8,
    started: Instant,
    row: Option<ComponentId>,
    call_id: String,
    control: usize,
    editor: usize,
    baseline: Option<tekes_ui_controls::TextEditBaseline>,
}
impl Probe {
    pub fn new(fixture: Fixture) -> Self {
        run_script(vec![(20000, Action::Close)]);
        Self {
            fixture,
            phase: 0,
            started: Instant::now(),
            row: None,
            call_id: String::new(),
            control: 0,
            editor: 0,
            baseline: None,
        }
    }
    fn observed(host: &Host, status: &str) -> bool {
        let matches = |s: &tekes_agent_components_ui::SummaryFacts| {
            s.subagents
                .iter()
                .any(|c| c.session_id.as_deref() == Some(CHILD) && c.status == status)
        };
        host.view
            .binding()
            .current_run()
            .unwrap()
            .summary
            .as_ref()
            .is_some_and(matches)
            && host
                .view
                .tree
                .get(host.view.summary_id())
                .and_then(|c| c.component::<SummaryView>())
                .and_then(SummaryView::displayed_context_input)
                .and_then(|s| s.facts.as_ref())
                .is_some_and(matches)
    }
    fn publish(&self, status: &str) {
        self.fixture
            .publish_inventory_summary(
                serde_json::json!({"sessionId":CHILD,"title":"Summary child","status":status,"running":status=="running","blank":true,"updatedAt":self.phase}),
            )
            .unwrap();
    }
    fn check(&self, host: &Host, status: &str, indicator: Option<SummarySubagentIndicator>) {
        let run = host.view.binding().current_run().unwrap();
        let summary = run.summary.as_ref().unwrap();
        let child = summary
            .subagents
            .iter()
            .find(|s| s.session_id.as_deref() == Some(CHILD))
            .unwrap();
        assert_eq!(child.status, status);
        assert_eq!(child.indicator(), indicator);
        assert!(
            run.transcript
                .tools
                .iter()
                .find(|t| t.id == self.call_id)
                .is_some_and(|t| t.phase == "done")
        );
        let view = host
            .view
            .tree
            .get(host.view.summary_id())
            .unwrap()
            .component::<SummaryView>()
            .unwrap();
        assert_eq!(view.action_id("subagents", &self.call_id), self.row);
        let control = host
            .view
            .tree
            .get(self.row.unwrap())
            .unwrap()
            .component::<SummaryActionRow>()
            .unwrap()
            .button();
        assert_eq!(control as *const _ as usize, self.control);
        assert!(control.label.contains(status));
        assert_eq!(host.view.tree.focused(), Some(host.view.ids().editor));
        assert_eq!(host.view.editor() as *const _ as usize, self.editor);
        assert_eq!(
            Some(host.view.editor().input.external_baseline()),
            self.baseline
        );
        assert!(host.view.editor().input.marked.is_some());
        assert!(run.submissions.is_empty());
    }
    pub fn advance(&mut self, host: &mut Host, window: &Window) {
        assert!(
            self.started.elapsed() < Duration::from_secs(20),
            "child status deadline phase {}",
            self.phase
        );
        let old = self.phase;
        match self.phase {
            0 if window.is_active() && Self::observed(host, "notStarted") => {
                let summary = host
                    .view
                    .tree
                    .get(host.view.summary_id())
                    .unwrap()
                    .component::<SummaryView>()
                    .unwrap();
                self.call_id = host
                    .view
                    .binding()
                    .current_run()
                    .unwrap()
                    .summary
                    .as_ref()
                    .unwrap()
                    .subagents
                    .iter()
                    .find(|child| child.session_id.as_deref() == Some(CHILD))
                    .unwrap()
                    .id
                    .clone();
                self.row = summary.action_id("subagents", &self.call_id);
                self.control = host
                    .view
                    .tree
                    .get(self.row.unwrap())
                    .unwrap()
                    .component::<SummaryActionRow>()
                    .unwrap()
                    .button() as *const _ as usize;
                let p = host
                    .view
                    .tree
                    .get(host.view.ids().editor)
                    .unwrap()
                    .visible()
                    .unwrap()
                    .center();
                run_script(vec![
                    (
                        0,
                        Action::Click {
                            x: p.x.into(),
                            y: p.y.into(),
                        },
                    ),
                    (
                        40,
                        Action::PreeditRange {
                            text: "子状态回传期间中文组字".into(),
                            selection: 1..3,
                            replacement: None,
                        },
                    ),
                ]);
                self.phase = 1;
            }
            1 if host.view.editor().input.marked.is_some() => {
                self.editor = host.view.editor() as *const _ as usize;
                self.baseline = Some(host.view.editor().input.external_baseline());
                self.publish("running");
                self.phase = 2;
            }
            2 if Self::observed(host, "running") => {
                self.check(host, "running", Some(SummarySubagentIndicator::Running));
                self.publish("failed");
                self.phase = 3;
            }
            3 if Self::observed(host, "failed") => {
                self.check(host, "failed", Some(SummarySubagentIndicator::Failed));
                self.publish("completed");
                self.phase = 4;
            }
            4 if Self::observed(host, "completed") => {
                self.check(host, "completed", None);
                assert!(
                    !host.runtime.snapshot().sessions[CHILD].journal_ready,
                    "no child history hydration"
                );
                run_script(vec![(0, Action::Close)]);
                self.phase = 5;
            }
            _ => {}
        }
        if old != self.phase {
            eprintln!("NATIVE_SUMMARY_CHILD_STATUS_PHASE {}", self.phase);
            window.request_redraw();
        }
    }
    pub fn receipt(&self) -> serde_json::Value {
        assert_eq!(self.phase, 5);
        serde_json::json!({"nativeWindow":true,"publicInventoryBackhaul":true,"childStatusNotToolPhase":true,
            "runningFailedCompleted":true,"completedHasNoSuccessIndicator":true,"retainedOriginalRow":true,
            "originalComposerAndMarkedBaseline":true,"noSubmission":true,"noChildHydration":true,
            "statusDotRendered":false,"realHarness":false,"physicalIme":false,"syntheticPreedit":true,"voiceOver":false})
    }
}
