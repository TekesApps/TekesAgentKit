//! Original Summary label, native CoreText rendering and resident Composer.
use super::*;
use tekes_agent_components_ui::{
    SummaryContextInput, SummaryOwner, SummaryRow, SummarySection, SummaryView,
};
use tekes_ui_controls::composition::TextContent;
use tekes_ui_platform_macos::selftest::{Action, run_script};
const TITLE: &str = "工作区 Summary 👩🏽‍💻 e\u{301} · repository/非常长的分支名称与完整状态标题 · 历史与工具输出 · 完整源标题不可丢失";
pub(super) struct Probe {
    phase: u8,
    started: Instant,
    resized_at: Option<Instant>,
    baseline: Option<tekes_ui_controls::TextEditBaseline>,
    editor: usize,
    header: Option<ComponentId>,
    label: Option<ComponentId>,
}
impl Probe {
    pub fn new() -> Self {
        run_script(vec![(25000, Action::Close)]);
        Self {
            phase: 0,
            started: Instant::now(),
            resized_at: None,
            baseline: None,
            editor: 0,
            header: None,
            label: None,
        }
    }
    fn view(host: &Host) -> &SummaryView {
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
            .expect("original control visible")
            .center();
        Action::Click {
            x: p.x.into(),
            y: p.y.into(),
        }
    }
    fn unchanged(&self, host: &Host) {
        assert_eq!(host.view.tree.focused(), Some(host.view.ids().editor));
        assert_eq!(host.view.editor() as *const _ as usize, self.editor);
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
    fn label(&self, host: &Host) {
        let summary = Self::view(host);
        assert_eq!(summary.section_header_id(TITLE), self.header);
        assert_eq!(summary.section_header_label_id(TITLE), self.label);
        assert_eq!(
            host.view
                .tree
                .get(self.label.unwrap())
                .unwrap()
                .component::<TextContent>()
                .unwrap()
                .text,
            TITLE
        );
        let labels = host.view.tree.semantics();
        assert_eq!(
            labels.iter().filter(|n| n.semantics.name == TITLE).count(),
            1
        );
        assert!(!labels.iter().any(|n| Some(n.id) == self.label));
        let layout = host
            .text
            .last_label
            .as_ref()
            .expect("SDK label service exercised");
        assert_eq!(layout.source(), TITLE);
        assert_eq!(layout.paragraph().lines.len(), 1);
        assert!(!layout.elided_utf16().is_empty());
    }
    pub fn advance(&mut self, host: &mut Host, window: &Window) {
        assert!(
            self.started.elapsed() < Duration::from_secs(25),
            "Summary label deadline phase {}",
            self.phase
        );
        let old = self.phase;
        match self.phase {
            0 if window.is_active() => {
                run_script(vec![
                    (0, Self::click(host, host.view.ids().editor)),
                    (
                        40,
                        Action::PreeditRange {
                            text: "标题布局期间中文组字".into(),
                            selection: 2..5,
                            replacement: None,
                        },
                    ),
                ]);
                self.phase = 1;
            }
            1 if host.view.editor().input.marked.is_some() => {
                self.baseline = Some(host.view.editor().input.external_baseline());
                self.editor = host.view.editor() as *const _ as usize;
                let revision = Self::view(host).displayed_context_input().unwrap().revision + 1;
                host.view
                    .set_workspace_summary(SummaryContextInput {
                        revision,
                        owner: SummaryOwner::Workspace {
                            id: "label-probe".into(),
                        },
                        facts: None,
                        ready: true,
                        custom_rows: vec![],
                        host_sections: vec![SummarySection {
                            id: TITLE.into(),
                            rows: vec![SummaryRow {
                                id: "branch".into(),
                                title: "原 Summary 内容".into(),
                                value: serde_json::Value::Null,
                                action: None,
                            }],
                        }],
                    })
                    .unwrap();
                self.phase = 2;
            }
            2 => {
                self.header = Self::view(host).section_header_id(TITLE);
                self.label = Self::view(host).section_header_label_id(TITLE);
                self.label(host);
                self.unchanged(host);
                host.text.type_scale = 1.4;
                self.phase = 3;
            }
            3 => {
                self.label(host);
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
                self.label(host);
                self.unchanged(host);
                host.text.type_scale = 1.;
                run_script(vec![(0, Action::Unmark)]);
                self.phase = 5;
            }
            5 if host.view.editor().input.marked.is_none() => {
                self.label(host);
                run_script(vec![(0, Self::click(host, self.header.unwrap()))]);
                self.phase = 6;
            }
            6 if Self::view(host).section_expanded(TITLE) == Some(false) => {
                self.label(host);
                run_script(vec![(0, Self::click(host, self.header.unwrap()))]);
                self.phase = 7;
            }
            7 if Self::view(host).section_expanded(TITLE) == Some(true) => {
                self.label(host);
                assert!(
                    host.view
                        .binding()
                        .current_run()
                        .unwrap()
                        .submissions
                        .is_empty()
                );
                run_script(vec![(0, Action::Close)]);
                self.phase = 8;
            }
            _ => {}
        }
        if old != self.phase {
            eprintln!("NATIVE_SUMMARY_LABEL_PHASE {}", self.phase);
            window.request_redraw();
        }
    }
    pub fn receipt(&self) -> serde_json::Value {
        assert_eq!(self.phase, 8);
        serde_json::json!({"nativeWindow":true,"originalSdkLabel":true,"singleLineEllipsis":true,
            "fullSourceAndAxName":true,"originalHeaderRetained":true,"originalPointerDisclosure":true,
            "typeScaleAndResize":true,"composerFocusAndMarkedBaseline":true,"noSubmission":true,
            "syntheticPreedit":true,"physicalIme":false,"physicalResizeDrag":false,"voiceOver":false,
            "realHarness":false,"fullSummaryRowParity":false})
    }
}
