//! Owned-fixture native Summary disclosure probe; normal pointer callbacks and
//! a real presentation publication, without endpoint mutation or a UI mirror.
use super::*;
use tekes_agent_components_ui::SummaryView;
use tekes_ui_controls::composition::{ButtonContent, TextContent};
use tekes_ui_platform_macos::selftest::{Action, run_script};
const DRAFT: &str = "Summary 折叠期间的新草稿";
pub(super) struct Probe {
    started: Instant,
    phase: u8,
    deadline: bool,
    header: Option<ComponentId>,
    row: Option<ComponentId>,
    widget: usize,
    revision: u64,
    items: usize,
    submissions: usize,
}
impl Probe {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            phase: 0,
            deadline: false,
            header: None,
            row: None,
            widget: 0,
            revision: 0,
            items: 0,
            submissions: 0,
        }
    }
    fn summary(view: &ComponentDemo) -> Option<&SummaryView> {
        view.tree.get(view.summary_id())?.component::<SummaryView>()
    }
    fn widget(view: &ComponentDemo, row: ComponentId) -> usize {
        let node = view.tree.get(row).unwrap();
        if let Some(button) = node.component::<ButtonContent>() {
            button as *const _ as usize
        } else {
            node.component::<TextContent>().unwrap() as *const _ as usize
        }
    }
    fn click(view: &ComponentDemo, id: ComponentId) -> Action {
        let point = view.tree.get(id).unwrap().frame().center();
        Action::Click {
            x: point.x as f64,
            y: point.y as f64,
        }
    }
    pub fn advance(&mut self, host: &Host, _: &Window) {
        if !self.deadline {
            run_script(vec![(15000, Action::Close)]);
            self.deadline = true;
        }
        assert!(
            self.started.elapsed() < Duration::from_secs(15),
            "Summary disclosure deadline phase {}",
            self.phase
        );
        let view = &host.view;
        let Some(summary) = Self::summary(view) else {
            return;
        };
        let Some(input) = summary.displayed_input() else {
            return;
        };
        let Some(task) = input.facts.as_ref().and_then(|facts| facts.tasks.first()) else {
            return;
        };
        match self.phase {
            0 => {
                let Some(header) = summary.section_header_id("tasks") else {
                    return;
                };
                let row = summary.row_id("tasks", &task.id).unwrap();
                assert_eq!(summary.section_expanded("tasks"), Some(true));
                assert!(view.tree.semantics().iter().any(|node| node.id == row));
                self.header = Some(header);
                self.row = Some(row);
                self.widget = Self::widget(view, row);
                self.revision = input.revision;
                let run = view.binding().current_run().unwrap();
                self.items = run.transcript.items.len();
                self.submissions = run.submissions.len();
                self.phase = 1;
                run_script(vec![(100, Self::click(view, header))]);
            }
            1 if summary.section_expanded("tasks") == Some(false) => {
                assert!(
                    !view
                        .tree
                        .semantics()
                        .iter()
                        .any(|node| Some(node.id) == self.row)
                );
                assert!(
                    summary
                        .section_layouts()
                        .iter()
                        .any(|layout| layout.section_id == "tasks" && layout.body.is_none())
                );
                self.phase = 2;
                view.binding()
                    .edit_text(
                        &tekes_agent_components_ui::ComposerDestination::run(input.scope.clone()),
                        DRAFT.into(),
                    )
                    .unwrap();
            }
            2 if input.revision > self.revision => {
                assert_eq!(summary.section_expanded("tasks"), Some(false));
                assert_eq!(summary.section_header_id("tasks"), self.header);
                assert_eq!(summary.row_id("tasks", &task.id), self.row);
                assert_eq!(Self::widget(view, self.row.unwrap()), self.widget);
                self.phase = 3;
                let mut actions = vec![];
                if let Ok(path) = std::env::var("AGENT_SUMMARY_SECTIONS_CAPTURE") {
                    actions.push((0, Action::Capture { path }));
                }
                actions.push((200, Self::click(view, self.header.unwrap())));
                run_script(actions);
            }
            3 if summary.section_expanded("tasks") == Some(true) => {
                assert!(
                    view.tree
                        .semantics()
                        .iter()
                        .any(|node| Some(node.id) == self.row)
                );
                assert_eq!(Self::widget(view, self.row.unwrap()), self.widget);
                let run = view.binding().current_run().unwrap();
                assert_eq!(run.draft.text, DRAFT);
                assert_eq!(run.transcript.items.len(), self.items);
                assert_eq!(run.submissions.len(), self.submissions);
                self.phase = 4;
                run_script(vec![(100, Action::Close)]);
            }
            _ => {}
        }
    }
    pub fn receipt(&self, view: &ComponentDemo) -> serde_json::Value {
        assert_eq!(self.phase, 4, "incomplete Summary probe");
        assert_eq!(Self::widget(view, self.row.unwrap()), self.widget);
        serde_json::json!({"nativeWindow":true,"pointerCollapseExpand":true,"collapsedCanonicalUpdate":true,
            "originalRowsRetained":true,"hiddenFromAccessibility":true,"noSubmission":true,
            "runtimeDraft":true,"realHarness":false,"physicalIme":false,"voiceOver":false,"fullLayoutParity":false})
    }
}
