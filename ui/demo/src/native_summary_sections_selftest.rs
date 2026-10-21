//! Owned-fixture native Summary disclosure probe; normal pointer callbacks and
//! a real presentation publication, without endpoint mutation or a UI mirror.
use super::*;
use tekes_agent_components_ui::{SummaryUsageRowIds, SummaryView};
use tekes_ui_controls::composition::{ButtonContent, TextContent, TextFieldContent};
use tekes_ui_platform_macos::selftest::{Action, run_script};
const DRAFT: &str = "Summary 折叠期间的新草稿";
pub(super) struct Probe {
    started: Instant,
    phase: u8,
    deadline: bool,
    header: Option<ComponentId>,
    row: Option<ComponentId>,
    widget: usize,
    usage: Option<SummaryUsageRowIds>,
    usage_widget: usize,
    revision: u64,
    items: usize,
    submissions: usize,
    text_started: Option<Instant>,
    text_owner: usize,
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
            usage: None,
            usage_widget: 0,
            revision: 0,
            items: 0,
            submissions: 0,
            text_started: None,
            text_owner: 0,
        }
    }
    fn summary(view: &ComponentDemo) -> Option<&SummaryView> {
        view.tree.get(view.summary_id())?.component::<SummaryView>()
    }
    fn widget(view: &ComponentDemo, row: ComponentId) -> usize {
        let node = view.tree.get(row).unwrap();
        if let Some(button) = node.component::<ButtonContent>() {
            button as *const _ as usize
        } else if let Some(text) = node.component::<TextFieldContent>() {
            text as *const _ as usize
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
                let Some(usage) = summary.usage_row_ids("output") else {
                    return;
                };
                self.usage = Some(usage);
                self.usage_widget = Self::widget(view, usage.value);
                assert_eq!(
                    view.tree
                        .get(usage.value)
                        .unwrap()
                        .component::<TextContent>()
                        .unwrap()
                        .text,
                    "1,234"
                );
                let cost = summary.usage_row_ids("cost").unwrap();
                assert_eq!(
                    view.tree
                        .get(cost.value)
                        .unwrap()
                        .component::<TextContent>()
                        .unwrap()
                        .text,
                    "未知"
                );
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
                run_script(vec![(
                    100,
                    Self::click(view, summary.section_header_id("usage").unwrap()),
                )]);
            }
            4 if summary.section_expanded("usage") == Some(false) => {
                let ids = self.usage.unwrap();
                assert!(
                    !view
                        .tree
                        .semantics()
                        .iter()
                        .any(|node| node.id == ids.title || node.id == ids.value)
                );
                assert_eq!(Self::widget(view, ids.value), self.usage_widget);
                self.phase = 5;
                run_script(vec![(
                    100,
                    Self::click(view, summary.section_header_id("usage").unwrap()),
                )]);
            }
            5 if summary.section_expanded("usage") == Some(true) => {
                let ids = self.usage.unwrap();
                assert_eq!(summary.usage_row_ids("output"), Some(ids));
                assert_eq!(Self::widget(view, ids.value), self.usage_widget);
                let row = view.tree.get(ids.row).unwrap().frame();
                let title = view.tree.get(ids.title).unwrap().frame();
                let value = view.tree.get(ids.value).unwrap().frame();
                assert!(title.max_x() < value.min_x());
                assert!(
                    (row.max_x() - value.max_x() - 2. * tekes_ui_tokens::Spacing::Regular.value())
                        .abs()
                        < 0.1
                );
                assert!(
                    view.tree
                        .semantics()
                        .iter()
                        .any(|node| node.id == ids.value && node.semantics.name == "1,234")
                );
                self.text_owner = view
                    .tree
                    .get(ids.title)
                    .unwrap()
                    .component::<TextFieldContent>()
                    .unwrap() as *const _ as usize;
                self.text_started = Some(Instant::now());
                self.phase = 6;
                run_script(vec![
                    (100, Self::click(view, ids.title)),
                    (
                        200,
                        Action::AXSelect {
                            component: ids.title.0,
                            range: 0..2,
                        },
                    ),
                    (350, Action::Text("不应写入".into())),
                    (450, Action::Preedit("不应组字".into())),
                    (
                        650,
                        Action::ExpectAXText {
                            component: ids.title.0,
                            text: "输出".into(),
                            selection: 0..2,
                            selected: "输出".into(),
                            query: 0..2,
                            substring: Some("输出".into()),
                        },
                    ),
                    (900, Action::WakeRedraw),
                ]);
            }
            6 if self
                .text_started
                .is_some_and(|start| start.elapsed() >= Duration::from_millis(850)) =>
            {
                let id = self.usage.unwrap().title;
                let field = view
                    .tree
                    .get(id)
                    .unwrap()
                    .component::<TextFieldContent>()
                    .unwrap();
                assert_eq!(view.tree.focused(), Some(id));
                assert_eq!(field.surface.input.selected_text(), "输出");
                assert!(field.surface.input.read_only && field.surface.input.marked.is_none());
                assert_eq!(field.surface.input.text, "输出");
                self.revision = input.revision;
                self.phase = 7;
                view.binding()
                    .edit_text(
                        &tekes_agent_components_ui::ComposerDestination::run(input.scope.clone()),
                        format!("{DRAFT}追加"),
                    )
                    .unwrap();
            }
            7 if input.revision > self.revision => {
                let id = self.usage.unwrap().title;
                let field = view
                    .tree
                    .get(id)
                    .unwrap()
                    .component::<TextFieldContent>()
                    .unwrap();
                assert_eq!(field as *const _ as usize, self.text_owner);
                assert_eq!(view.tree.focused(), Some(id));
                assert_eq!(field.surface.input.selected_text(), "输出");
                let run = view.binding().current_run().unwrap();
                assert_eq!(run.draft.text, format!("{DRAFT}追加"));
                assert_eq!(run.submissions.len(), self.submissions);
                self.phase = 8;
                let mut actions = vec![];
                if let Ok(path) = std::env::var("AGENT_SUMMARY_SECTIONS_CAPTURE") {
                    actions.push((
                        0,
                        Action::Capture {
                            path: format!("{path}.selectable.png"),
                        },
                    ));
                }
                actions.push((100, Action::Close));
                run_script(actions);
            }
            _ => {}
        }
    }
    pub fn receipt(&self, view: &ComponentDemo) -> serde_json::Value {
        assert_eq!(self.phase, 8, "incomplete Summary probe");
        assert_eq!(Self::widget(view, self.row.unwrap()), self.widget);
        serde_json::json!({"nativeWindow":true,"pointerCollapseExpand":true,"collapsedCanonicalUpdate":true,
            "originalRowsRetained":true,"hiddenFromAccessibility":true,"noSubmission":true,
            "runtimeDraft":true,"usageSelectableTitle":true,"nativeTextAndAxSelection":true,"readonlyMutationRejected":true,"selectedTextRetainedAfterPublication":true,"systemClipboardExercised":false,"usageColumns":true,"usagePointerCollapseExpand":true,"usageControlsRetained":true,
            "usageFromControlProjection":true,"knownCount":"1,234","unknownCost":true,"realHarness":false,"physicalIme":false,"voiceOver":false,"fullLayoutParity":false})
    }
}
