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
    repositories: bool,
    capture: Option<String>,
    indentation: Option<Arc<std::sync::atomic::AtomicU32>>,
    indented_ids: Option<tekes_agent_components_ui::SummaryDefaultRowIds>,
    indented_owner: usize,
    indented_left: f32,
    metadata_right: f32,
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
            repositories: false,
            capture: None,
            indentation: None,
            indented_ids: None,
            indented_owner: 0,
            indented_left: 0.,
            metadata_right: 0.,
        }
    }
    pub fn with_repository_dividers() -> Self {
        let mut probe = Self::new();
        probe.repositories = true;
        probe.capture = std::env::var("AGENT_SUMMARY_DIVIDER_CAPTURE").ok();
        if let Some(path) = &probe.capture {
            assert!(
                std::path::Path::new(path).is_absolute() && !std::path::Path::new(path).exists(),
                "use a new absolute divider screenshot path"
            );
        }
        probe
    }
    pub fn with_indentation(level: Arc<std::sync::atomic::AtomicU32>) -> Self {
        let mut probe = Self::new();
        probe.indentation = Some(level);
        probe.capture = std::env::var("AGENT_SUMMARY_INDENT_CAPTURE").ok();
        if let Some(path) = &probe.capture {
            assert!(
                std::path::Path::new(path).is_absolute() && !std::path::Path::new(path).exists(),
                "use a fresh absolute indentation screenshot"
            );
        }
        probe
    }
    fn section(&self) -> &'static str {
        if self.repositories {
            "repository:workspace"
        } else {
            SECTION
        }
    }
    fn view(host: &Host) -> Option<&SummaryView> {
        host.view.tree.get(host.view.summary_id())?.component()
    }
    fn input(&mut self, id: &str) -> SummaryContextInput {
        self.revision += 1;
        let mut input = SummaryContextInput {
            custom_rows: vec![],
            revision: self.revision,
            owner: SummaryOwner::Workspace { id: id.into() },
            facts: None,
            ready: true,
            host_sections: vec![SummarySection {
                id: self.section().into(),
                rows: vec![SummaryRow {
                    id: "name".into(),
                    title: "工作区".into(),
                    value: json!(id),
                    action: None,
                }],
            }],
        };
        if self.repositories {
            input.host_sections.insert(
                0,
                SummarySection {
                    id: "repository:first".into(),
                    rows: vec![SummaryRow {
                        id: "branch".into(),
                        title: "首个仓库".into(),
                        value: json!("main"),
                        action: None,
                    }],
                },
            );
        }
        input
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
        // All publication/geometry/IME assertions have finished. Closing a
        // native window may legitimately retire its editing authority; do not
        // turn teardown notifications into another publication observation.
        if self.phase == 9 || (self.phase == 8 && !self.repositories && self.indentation.is_none())
        {
            return;
        }
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
                self.original_row = summary.row_id(self.section(), "name");
                let header = summary.section_header_id(self.section()).unwrap();
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
                    (
                        200,
                        if self.repositories {
                            let divider = summary
                                .section_layouts()
                                .iter()
                                .find(|layout| layout.section_id == self.section())
                                .unwrap()
                                .divider
                                .unwrap();
                            Action::Click {
                                x: divider.center().x.into(),
                                y: divider.center().y.into(),
                            }
                        } else {
                            Self::click(host, header)
                        },
                    ),
                ]);
                self.phase = 2;
            }
            2 if summary.section_expanded(self.section()) == Some(false) => {
                assert!(host.view.editor().input.marked.is_some());
                self.baseline = Some(host.view.editor().input.external_baseline());
                if let Some(level) = &self.indentation {
                    level.store(2, std::sync::atomic::Ordering::Relaxed);
                }
                let mut input = self.input("workspace-a");
                if self.repositories {
                    let layout = summary
                        .section_layouts()
                        .into_iter()
                        .find(|layout| layout.section_id == self.section())
                        .unwrap();
                    assert!(layout.divider.is_some() && !layout.expanded);
                    input.host_sections.swap(0, 1);
                }
                input
                    .host_sections
                    .iter_mut()
                    .find(|section| section.id == self.section())
                    .unwrap()
                    .rows[0]
                    .value = json!("更新后的工作区 A");
                host.view.set_workspace_summary(input).unwrap();
                self.phase = 3;
            }
            3 => {
                assert_eq!(summary.row_id(self.section(), "name"), self.original_row);
                assert_eq!(summary.section_expanded(self.section()), Some(false));
                if self.repositories {
                    assert!(summary.section_layouts()[0].divider.is_none());
                }
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
                assert_ne!(summary.row_id(self.section(), "name"), self.original_row);
                assert_eq!(summary.section_expanded(self.section()), Some(false));
                let header = summary.section_header_id(self.section()).unwrap();
                if self.repositories {
                    assert!(
                        summary
                            .section_layouts()
                            .into_iter()
                            .find(|layout| layout.section_id == self.section())
                            .unwrap()
                            .divider
                            .is_some()
                    );
                }
                run_script(vec![(0, Self::click(host, header))]);
                self.phase = 5;
            }
            5 if summary.section_expanded(self.section()) == Some(true) => {
                let id = summary.row_id(self.section(), "name").unwrap();
                let field = host.view.tree.text_surface(id).unwrap();
                assert!(field.input.read_only);
                assert_eq!(field.input.text, "工作区");
                let ids = summary.default_row_ids(self.section(), "name").unwrap();
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
                assert!(summary.action_id(self.section(), "name").is_none());
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
                if let Some(level) = &self.indentation {
                    self.indented_ids = Some(ids);
                    self.indented_owner =
                        host.view.tree.text_surface(ids.title).unwrap() as *const _ as usize;
                    self.indented_left = title.min_x();
                    self.metadata_right = meta.max_x();
                    let scale = host.text.text_surface_host().unwrap().type_scale();
                    assert!(
                        (title.min_x()
                            - band.min_x()
                            - 2. * tekes_ui_tokens::Spacing::Regular.value()
                            - 28. * scale)
                            .abs()
                            < 0.1
                    );
                    level.store(1, std::sync::atomic::Ordering::Relaxed);
                    self.publish(host, "workspace-b");
                    self.phase = 20;
                } else {
                    let mut empty = self.input("workspace-b");
                    empty.host_sections.clear();
                    host.view.set_workspace_summary(empty).unwrap();
                    self.phase = 6;
                }
            }
            20 => {
                let ids = summary.default_row_ids(self.section(), "name").unwrap();
                assert_eq!(Some(ids), self.indented_ids);
                assert_eq!(
                    host.view.tree.text_surface(ids.title).unwrap() as *const _ as usize,
                    self.indented_owner
                );
                let title = host.view.tree.get(ids.title).unwrap().frame();
                let meta = host.view.tree.get(ids.trailing).unwrap().frame();
                let scale = host.text.text_surface_host().unwrap().type_scale();
                assert!((self.indented_left - title.min_x() - 14. * scale).abs() < 0.1);
                assert!((meta.max_x() - self.metadata_right).abs() < 0.1);
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
                if self.repositories || self.indentation.is_some() {
                    self.publish(host, "workspace-c");
                } else {
                    run_script(vec![(0, Action::Close)]);
                }
                self.phase = 8;
            }
            8 if self.repositories || self.indentation.is_some() => {
                if self.repositories {
                    let layouts = summary.section_layouts();
                    assert!(layouts[0].divider.is_none());
                    assert!(layouts[1].divider.is_some());
                }
                let mut actions = vec![];
                if let Some(path) = &self.capture {
                    actions.push((300, Action::Capture { path: path.clone() }));
                }
                actions.push((700, Action::Close));
                run_script(actions);
                self.phase = 9;
            }
            _ => {}
        }
        if old != self.phase {
            eprintln!("NATIVE_SUMMARY_CONTEXT_PHASE {}", self.phase);
            window.request_redraw();
        }
    }
    pub fn receipt(&self) -> serde_json::Value {
        assert_eq!(
            self.phase,
            if self.repositories || self.indentation.is_some() {
                9
            } else {
                8
            }
        );
        json!({"nativeWindow":true,"explicitWorkspaceOwner":true,"noSyntheticRunScope":true,"hostRowsWithoutSessionFacts":true,"originalHeaderClicks":true,"sameOwnerRowsRetained":true,"ownerChangeRetiresIds":true,"collapseRetained":true,"emptyAndUnavailableDistinct":true,"mainDraftAndTranscriptUnchanged":true,"noSubmission":true,"realHarness":false,"physicalIme":false,"voiceOver":false,"fullUnselectedHostLayout":false,"ordinaryMetadataFirstLine":true,"titleAndMetadataSeparate":true,"originalComposerFocusAndMarkedBaseline":true,"syntheticPreedit":true,"explicitIndentLevels":self.indentation.is_some(),"indentUpdateRetainsOriginalField":self.indentation.is_some(),"indentDoesNotMoveMetadata":self.indentation.is_some(),"repositoryDividerRules":self.repositories,"dividerBandOriginalDisclosure":self.repositories,"sameOwnerOrderChangesDivider":self.repositories})
    }
}
