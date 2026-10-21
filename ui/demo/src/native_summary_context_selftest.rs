//! Explicit workspace Summary over the original shared component. No fake Run.
use super::*;
use serde_json::json;
use tekes_agent_components_ui::{
    SummaryContextInput, SummaryOwner, SummaryRow, SummarySection, SummaryView,
};
use tekes_ui_controls::ButtonTextRenderer;
use tekes_ui_platform_macos::selftest::{Action, run_script};
const SECTION: &str = "host:workspace";
pub(super) struct Probe {
    phase: u8,
    started: Instant,
    revision: u64,
    original_row: Option<ComponentId>,
    draft: Option<String>,
    items: usize,
    submissions: usize,
    baseline: Option<tekes_ui_controls::TextEditBaseline>,
}
impl Probe {
    pub fn new() -> Self {
        run_script(vec![(20000, Action::Close)]);
        Self {
            phase: 0,
            started: Instant::now(),
            revision: 0,
            original_row: None,
            draft: None,
            items: 0,
            submissions: 0,
            baseline: None,
        }
    }
    fn view(host: &Host) -> Option<&SummaryView> {
        host.view.tree.get(host.view.summary_id())?.component()
    }
    fn input(&mut self, id: &str) -> SummaryContextInput {
        self.revision += 1;
        SummaryContextInput {
            custom_rows: vec![],
            revision: self.revision,
            owner: SummaryOwner::Workspace { id: id.into() },
            facts: None,
            ready: true,
            host_sections: vec![SummarySection {
                id: SECTION.into(),
                rows: vec![SummaryRow {
                    id: "name".into(),
                    title: "工作区".into(),
                    value: json!(id),
                    action: None,
                }],
            }],
        }
    }
    fn publish(&mut self, host: &mut Host, id: &str) {
        let input = self.input(id);
        host.view.set_workspace_summary(input).unwrap();
    }
    fn click(host: &Host, id: ComponentId) -> Action {
        let point = host
            .view
            .tree
            .get(id)
            .unwrap()
            .visible()
            .expect("original Summary header visible")
            .center();
        Action::Click {
            x: point.x as f64,
            y: point.y as f64,
        }
    }
    fn has_status(host: &Host, label: &str) -> bool {
        host.view
            .tree
            .semantics()
            .iter()
            .any(|node| node.semantics.name == label)
    }
    pub fn advance(&mut self, host: &mut Host, window: &Window) {
        assert!(
            self.started.elapsed() < Duration::from_secs(20),
            "workspace Summary deadline phase {}",
            self.phase
        );
        let Some(summary) = Self::view(host) else {
            return;
        };
        let Some(source) = summary.displayed_context_input() else {
            return;
        };
        if let Some(baseline) = &self.baseline {
            assert_eq!(&host.view.editor().input.external_baseline(), baseline);
            assert_eq!(host.view.tree.focused(), Some(host.view.ids().editor));
        }
        let old = self.phase;
        match self.phase {
            0 if window.is_active() => {
                self.revision = source.revision;
                let run = host.view.binding().current_run().unwrap();
                self.draft = Some(run.draft.text.clone());
                self.items = run.transcript.items.len();
                self.submissions = run.submissions.len();
                self.publish(host, "workspace-a");
                self.phase = 1;
            }
            1 => {
                assert!(matches!(&source.owner,SummaryOwner::Workspace{id} if id=="workspace-a"));
                assert!(source.facts.is_none() && summary.displayed_input().is_none());
                self.original_row = summary.row_id(SECTION, "name");
                let header = summary.section_header_id(SECTION).unwrap();
                run_script(vec![
                    (0, Self::click(host, host.view.ids().editor)),
                    (
                        100,
                        Action::PreeditRange {
                            text: "元数据更新期间中文组字".into(),
                            selection: 1..3,
                            replacement: None,
                        },
                    ),
                    (200, Self::click(host, header)),
                ]);
                self.phase = 2;
            }
            2 if summary.section_expanded(SECTION) == Some(false) => {
                assert!(host.view.editor().input.marked.is_some());
                self.baseline = Some(host.view.editor().input.external_baseline());
                let mut input = self.input("workspace-a");
                input.host_sections[0].rows[0].value = json!("更新后的工作区 A");
                host.view.set_workspace_summary(input).unwrap();
                self.phase = 3;
            }
            3 => {
                assert_eq!(summary.row_id(SECTION, "name"), self.original_row);
                assert_eq!(summary.section_expanded(SECTION), Some(false));
                assert!(
                    !host
                        .view
                        .tree
                        .semantics()
                        .iter()
                        .any(|node| Some(node.id) == self.original_row)
                );
                self.publish(host, "workspace-b");
                self.phase = 4;
            }
            4 => {
                assert!(matches!(&source.owner,SummaryOwner::Workspace{id} if id=="workspace-b"));
                assert_ne!(summary.row_id(SECTION, "name"), self.original_row);
                assert_eq!(summary.section_expanded(SECTION), Some(false));
                let header = summary.section_header_id(SECTION).unwrap();
                run_script(vec![(0, Self::click(host, header))]);
                self.phase = 5;
            }
            5 if summary.section_expanded(SECTION) == Some(true) => {
                let id = summary.row_id(SECTION, "name").unwrap();
                let field = host.view.tree.text_surface(id).unwrap();
                assert!(field.input.read_only);
                assert_eq!(field.input.text, "工作区");
                let ids = summary.default_row_ids(SECTION, "name").unwrap();
                let metadata = host.view.tree.get(ids.trailing).unwrap();
                assert_eq!(
                    metadata
                        .component::<tekes_ui_controls::composition::TextContent>()
                        .unwrap()
                        .text,
                    "workspace-b"
                );
                let title = host.view.tree.get(ids.title).unwrap().frame();
                let meta = metadata.frame();
                let band = host.view.tree.get(ids.row).unwrap().frame();
                assert!(summary.action_id(SECTION, "name").is_none());
                let line = host
                    .text
                    .measure("Mg", tekes_ui_tokens::proposal::TextRole::UiBody, 1e8)
                    .height;
                assert!(
                    (meta.max_x() - band.max_x() + 2. * tekes_ui_tokens::Spacing::Regular.value())
                        .abs()
                        < 0.1
                );
                assert!(
                    (meta.min_y() - title.min_y() - (line - meta.size.height) / 2.).abs() < 0.1
                );
                let mut empty = self.input("workspace-b");
                empty.host_sections.clear();
                host.view.set_workspace_summary(empty).unwrap();
                self.phase = 6;
            }
            6 if Self::has_status(host, "暂无 Summary 内容") => {
                let mut missing = self.input("workspace-b");
                missing.host_sections.clear();
                missing.ready = false;
                host.view.set_workspace_summary(missing).unwrap();
                self.phase = 7;
            }
            7 if Self::has_status(host, "Summary 暂不可用") => {
                assert!(source.owner.run_scope().is_none() && source.facts.is_none());
                assert!(summary.displayed_input().is_none());
                let run = host.view.binding().current_run().unwrap();
                assert_eq!(Some(&run.draft.text), self.draft.as_ref());
                assert_eq!(run.transcript.items.len(), self.items);
                assert_eq!(run.submissions.len(), self.submissions);
                run_script(vec![(0, Action::Close)]);
                self.phase = 8;
            }
            _ => {}
        }
        if old != self.phase {
            eprintln!("NATIVE_SUMMARY_CONTEXT_PHASE {}", self.phase);
            window.request_redraw();
        }
    }
    pub fn receipt(&self) -> serde_json::Value {
        assert_eq!(self.phase, 8);
        json!({"nativeWindow":true,"explicitWorkspaceOwner":true,"noSyntheticRunScope":true,"hostRowsWithoutSessionFacts":true,"originalHeaderClicks":true,"sameOwnerRowsRetained":true,"ownerChangeRetiresIds":true,"collapseRetained":true,"emptyAndUnavailableDistinct":true,"mainDraftAndTranscriptUnchanged":true,"noSubmission":true,"realHarness":false,"physicalIme":false,"voiceOver":false,"fullUnselectedHostLayout":false,"ordinaryMetadataFirstLine":true,"titleAndMetadataSeparate":true,"originalComposerFocusAndMarkedBaseline":true,"syntheticPreedit":true})
    }
}
