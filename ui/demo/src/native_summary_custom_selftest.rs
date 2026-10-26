//! Owned host row fixture; SDK controls carry events, Summary admits their source.
use super::*;
use crate::acceptance_script::{Action, run_script};
use serde_json::json;
use std::rc::Rc;
use tekes_agent_components_ui::{
    SummaryContextInput, SummaryCustomRowSpec, SummaryCustomRowToken, SummaryOwner, SummaryRow,
    SummarySection, SummaryView, install_summary_custom_row,
};
use tekes_ui_controls::composition::{
    AccessibilityAction, ButtonContent, Component, Content, ContentResponse, ContentSemantics,
    StackContent, TextContent,
};
use tekes_ui_controls::layout::MeasuredChild;
use tekes_ui_controls::{
    ButtonTextRenderer,
    extended::{Axis, Stack},
};
const SECTION: &str = "host:repository";
const ROW: &str = "controls";
#[derive(Debug)]
struct HostButton {
    button: ButtonContent,
    source: SummaryCustomRowToken,
    pending: Vec<SummaryCustomRowToken>,
    marker: Rc<()>,
}
impl HostButton {
    fn capture(&mut self, response: ContentResponse) -> ContentResponse {
        if response.activated {
            self.pending.push(self.source.clone());
        }
        response
    }
}
impl Component for HostButton {
    fn measure(
        &mut self,
        t: &mut dyn ButtonTextRenderer,
        s: tekes_ui_scene::Size,
    ) -> MeasuredChild {
        self.button.measure(t, s)
    }
    fn arrange(&mut self, t: &mut dyn ButtonTextRenderer, f: Rect, c: Rect) {
        self.button.arrange(t, f, c)
    }
    fn paint(
        &mut self,
        s: &mut Scene,
        t: &mut dyn ButtonTextRenderer,
        f: Rect,
        c: Rect,
        theme: tekes_ui_controls::composition::ContentTheme,
    ) {
        self.button.paint(s, t, f, c, theme)
    }
    fn event(&mut self, e: &ContentEvent) -> ContentResponse {
        let r = self.button.event(e);
        self.capture(r)
    }
    fn accessibility_action(
        &mut self,
        p: Option<usize>,
        a: &AccessibilityAction,
    ) -> ContentResponse {
        let r = self.button.accessibility_action(p, a);
        self.capture(r)
    }
    fn focusable(&self) -> bool {
        self.button.focusable()
    }
    fn set_focus(&mut self, f: bool, v: bool) {
        self.button.set_focus(f, v)
    }
    fn semantics(&self) -> ContentSemantics {
        self.button.semantics()
    }
    fn focus_decoration_above_siblings(&self) -> bool {
        self.button.focus_decoration_above_siblings()
    }
}
pub(super) struct Probe {
    phase: u8,
    started: Instant,
    revision: u64,
    token: Option<SummaryCustomRowToken>,
    held: Option<SummaryCustomRowToken>,
    original_row: Option<ComponentId>,
    marker: usize,
    height: f32,
    callbacks: usize,
    draft: Option<String>,
    items: usize,
    submissions: usize,
}
impl Probe {
    pub fn new() -> Self {
        run_script(vec![(20000, Action::Close)]);
        Self {
            phase: 0,
            started: Instant::now(),
            revision: 0,
            token: None,
            held: None,
            original_row: None,
            marker: 0,
            height: 0.,
            callbacks: 0,
            draft: None,
            items: 0,
            submissions: 0,
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
    fn view_mut(host: &mut Host) -> &mut SummaryView {
        let id = host.view.summary_id();
        host.view.tree.get_mut(id).unwrap().component_mut().unwrap()
    }
    fn publish(&mut self, host: &mut Host, workspace: &str) {
        self.revision += 1;
        host.view
            .set_workspace_summary(SummaryContextInput {
                revision: self.revision,
                owner: SummaryOwner::Workspace {
                    id: workspace.into(),
                },
                facts: None,
                ready: true,
                host_sections: vec![SummarySection {
                    id: SECTION.into(),
                    rows: vec![SummaryRow {
                        id: ROW.into(),
                        title: "仓库控件".into(),
                        value: json!(workspace),
                        action: None,
                    }],
                }],
                custom_rows: vec![SummaryCustomRowSpec {
                    section: SECTION.into(),
                    row: ROW.into(),
                    content_key: "owned-host-controls".into(),
                }],
            })
            .unwrap();
    }
    fn source(host: &Host) -> SummaryCustomRowToken {
        Self::view(host).custom_row_token(SECTION, ROW).unwrap()
    }
    fn install(&mut self, host: &mut Host, namespace: u64) {
        let source = Self::source(host);
        let id = ComponentId(namespace << 32);
        let root = host.view.summary_id();
        install_summary_custom_row(
            &mut host.view.tree,
            root,
            &source,
            Content::new(
                id,
                StackContent {
                    model: Stack {
                        axis: Axis::Vertical,
                        gap: tekes_ui_tokens::Spacing::Regular.value(),
                    },
                    flexible_child: None,
                    children: vec![
                        Content::new(
                            ComponentId(id.0 + 1),
                            HostButton {
                                button: ButtonContent::new("宿主查看操作"),
                                source: source.clone(),
                                pending: vec![],
                                marker: Rc::new(()),
                            },
                        ),
                        Content::new(ComponentId(id.0 + 2), TextContent::new("宿主自定义内容")),
                    ],
                },
            ),
        )
        .unwrap();
        self.token = Some(source);
    }
    fn stack<'a>(&self, host: &'a Host) -> &'a StackContent {
        Self::view(host)
            .custom_row_component(self.token.as_ref().unwrap())
            .unwrap()
    }
    fn marker(&self, host: &Host) -> usize {
        Rc::as_ptr(
            &self.stack(host).children[0]
                .component::<HostButton>()
                .unwrap()
                .marker,
        ) as usize
    }
    fn button(&self, host: &Host) -> ComponentId {
        self.stack(host).children[0].id()
    }
    fn click(host: &Host, id: ComponentId) -> Action {
        let p = host
            .view
            .tree
            .get(id)
            .unwrap()
            .visible()
            .expect("original host control visible")
            .center();
        Action::Click {
            x: p.x as f64,
            y: p.y as f64,
        }
    }
    fn drain(&mut self, host: &mut Host) -> bool {
        let source = self.token.as_ref().unwrap();
        let stack = Self::view_mut(host)
            .custom_row_component_mut::<StackContent>(source)
            .unwrap();
        let pending = std::mem::take(
            &mut stack.children[0]
                .component_mut::<HostButton>()
                .unwrap()
                .pending,
        );
        if pending.is_empty() {
            return false;
        }
        assert_eq!(pending.len(), 1);
        for source in pending {
            Self::view(host)
                .validate_custom_row_action(&source)
                .unwrap();
            self.callbacks += 1;
            self.held = Some(source);
        }
        true
    }
    fn refresh_source(&mut self, host: &mut Host, grow: bool) {
        let source = Self::source(host);
        let stack = Self::view_mut(host)
            .custom_row_component_mut::<StackContent>(&source)
            .unwrap();
        stack.children[0]
            .component_mut::<HostButton>()
            .unwrap()
            .source = source.clone();
        if grow {
            stack.children[1]
                .component_mut::<TextContent>()
                .unwrap()
                .text = "宿主回传改变自定义行高度，仍由原SDK测量。\n".repeat(10);
        }
        self.token = Some(source);
    }
    pub fn advance(&mut self, host: &mut Host, window: &Window) {
        assert!(
            self.started.elapsed() < Duration::from_secs(20),
            "custom Summary deadline phase {}",
            self.phase
        );
        let Some(summary) = host
            .view
            .tree
            .get(host.view.summary_id())
            .and_then(|node| node.component::<SummaryView>())
        else {
            return;
        };
        let old = self.phase;
        match self.phase {
            0 if window.is_active() => {
                let Some(input) = summary.displayed_context_input() else {
                    return;
                };
                self.revision = input.revision;
                let run = host.view.binding().current_run().unwrap();
                self.draft = Some(run.draft.text.clone());
                self.items = run.transcript.items.len();
                self.submissions = run.submissions.len();
                self.publish(host, "workspace-a");
                self.phase = 1;
            }
            1 => {
                self.install(host, 0xffd0_0001);
                self.phase = 2;
            }
            2 => {
                self.original_row = summary.row_id(SECTION, ROW);
                self.height = summary.section_layouts()[0].body.unwrap().size.height;
                self.marker = self.marker(host);
                run_script(vec![(0, Self::click(host, self.button(host)))]);
                self.phase = 3;
            }
            3 => {
                if !self.drain(host) {
                    return;
                }
                assert_eq!(self.callbacks, 1);
                self.publish(host, "workspace-a");
                self.refresh_source(host, true);
                self.phase = 4;
            }
            4 => {
                assert_eq!(summary.row_id(SECTION, ROW), self.original_row);
                assert_eq!(self.marker(host), self.marker);
                assert!(summary.section_layouts()[0].body.unwrap().size.height > self.height + 80.);
                assert!(
                    summary
                        .validate_custom_row_action(self.held.as_ref().unwrap())
                        .is_err()
                );
                run_script(vec![(
                    0,
                    Self::click(host, summary.section_header_id(SECTION).unwrap()),
                )]);
                self.phase = 5;
            }
            5 if summary.section_expanded(SECTION) == Some(false) => {
                assert!(
                    !host
                        .view
                        .tree
                        .semantics()
                        .iter()
                        .any(|node| node.id.0 >> 32 == 0xffd0_0001)
                );
                assert!(
                    summary
                        .validate_custom_row_action(self.token.as_ref().unwrap())
                        .is_err()
                );
                run_script(vec![(
                    0,
                    Self::click(host, summary.section_header_id(SECTION).unwrap()),
                )]);
                self.phase = 6;
            }
            6 if summary.section_expanded(SECTION) == Some(true) => {
                assert!(
                    summary
                        .validate_custom_row_action(self.token.as_ref().unwrap())
                        .is_err(),
                    "close/open cannot revive a held callback"
                );
                self.refresh_source(host, false);
                assert_eq!(self.marker(host), self.marker);
                self.phase = 7;
            }
            7 => {
                run_script(vec![(0, Self::click(host, self.button(host)))]);
                self.phase = 8;
            }
            8 => {
                if !self.drain(host) {
                    return;
                }
                assert_eq!(self.callbacks, 2);
                self.publish(host, "workspace-b");
                self.phase = 9;
            }
            9 => {
                assert_ne!(summary.row_id(SECTION, ROW), self.original_row);
                assert!(
                    summary
                        .validate_custom_row_action(self.held.as_ref().unwrap())
                        .is_err()
                );
                assert!(
                    host.view
                        .tree
                        .get(ComponentId((0xffd0_0001u64 << 32) + 1))
                        .is_none()
                );
                self.install(host, 0xffd0_0002);
                self.phase = 10;
            }
            10 => {
                run_script(vec![(0, Self::click(host, self.button(host)))]);
                self.phase = 11;
            }
            11 => {
                if !self.drain(host) {
                    return;
                }
                assert_eq!(self.callbacks, 3);
                let run = host.view.binding().current_run().unwrap();
                assert_eq!(Some(&run.draft.text), self.draft.as_ref());
                assert_eq!(run.transcript.items.len(), self.items);
                assert_eq!(run.submissions.len(), self.submissions);
                run_script(vec![(0, Action::Close)]);
                self.phase = 12;
            }
            _ => {}
        }
        if old != self.phase {
            eprintln!("NATIVE_SUMMARY_CUSTOM_PHASE {}", self.phase);
            window.request_redraw();
        }
    }
    pub fn receipt(&self) -> serde_json::Value {
        assert_eq!(self.phase, 12);
        assert_eq!(self.callbacks, 3);
        json!({"nativeWindow":true,"originalSdkHostControls":true,"eventTimeHostSource":true,"sameOwnerControlRetained":true,"dynamicHeightMeasuredBySdk":true,"collapseHiddenFromAx":true,"disclosureAbaRetiresCallback":true,"ownerChangeRetiresContent":true,"hostCallbacks":3,"noSyntheticRunScope":true,"noSubmission":true,"realGitOperation":false,"realHarness":false,"physicalIme":false,"voiceOver":false,"fullRepositoryRows":false})
    }
}
