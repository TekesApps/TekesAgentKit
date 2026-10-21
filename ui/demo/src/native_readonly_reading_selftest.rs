//! Owned bounded SEP → retained read-only pane → original SDK reading/GPU.
use super::*;
use tekes_agent_components_ui::ReadOnlyTranscript;
use tekes_ui_controls::composition::{ContentReadingAnchorUpdate, ScrollContent};
use tekes_ui_platform_macos::selftest::{Action, run_script};
pub(super) struct Probe {
    phase: u8,
    started: Instant,
    count: usize,
    loaded: usize,
    item: Option<ComponentId>,
    y: f32,
    revision: u64,
    child: Option<String>,
}
impl Probe {
    pub fn new(child: Option<String>) -> Self {
        run_script(vec![(25_000, Action::Close)]);
        Self {
            phase: 0,
            started: Instant::now(),
            count: 0,
            loaded: 0,
            item: None,
            y: 0.,
            revision: 0,
            child,
        }
    }
    fn pane(host: &Host) -> &ReadOnlyTranscript {
        host.view
            .tree
            .get(host.view.bounded_child_view_id().unwrap())
            .unwrap()
            .component()
            .unwrap()
    }
    fn scroll(host: &Host) -> &ScrollContent {
        host.view
            .tree
            .get(Self::pane(host).ids().viewport)
            .unwrap()
            .component()
            .unwrap()
    }
    fn anchor(host: &Host) -> ComponentId {
        match Self::scroll(host).reading_result().unwrap().anchor {
            ContentReadingAnchorUpdate::Preserved(id)
            | ContentReadingAnchorUpdate::Fallback(Some(id)) => id,
            ref other => panic!("bounded reading item {other:?}"),
        }
    }
    fn relative_y(host: &Host, item: ComponentId) -> f32 {
        host.view.tree.get(item).unwrap().frame().min_y()
            - host
                .view
                .tree
                .get(Self::pane(host).ids().viewport)
                .unwrap()
                .visible()
                .unwrap()
                .min_y()
    }
    fn read(host: &mut Host, page: bool) {
        let ids = Self::pane(host).ids();
        let button = if page {
            Self::pane(host).paging_button_id()
        } else {
            ids.refresh
        };
        host.view
            .tree
            .accessibility_action(button, None, AccessibilityAction::Press);
        let commands = host.view.drain_commands().unwrap();
        assert!(
            !commands.is_empty(),
            "original bounded read control must issue a read"
        );
        host.commands(Ok(commands));
    }
    fn unchanged(&self, host: &Host) {
        assert_eq!(Self::anchor(host), self.item.unwrap());
        assert!(
            (Self::relative_y(host, self.item.unwrap()) - self.y).abs() <= 1.,
            "bounded anchor moved"
        );
        assert_eq!(host.view.editor().input.text, "主会话中文草稿");
        assert!(
            host.view
                .binding()
                .current_run()
                .unwrap()
                .submissions
                .is_empty()
        );
        assert!(host.view.panel_composer_ids().is_none());
        assert!(!Self::pane(host).content().feedback_enabled());
    }
    pub fn advance(&mut self, host: &mut Host, window: &Window) {
        assert!(
            self.started.elapsed() < Duration::from_secs(25),
            "bounded reading deadline {}",
            self.phase
        );
        let old = self.phase;
        match self.phase {
            0 if window.is_active() => {
                host.view
                    .select(if self.child.is_some() { 0 } else { 1 })
                    .unwrap();
                let binding = host.view.binding();
                binding
                    .edit_text(
                        &tekes_agent_components_ui::ComposerDestination::run(
                            binding.scope().clone(),
                        ),
                        "主会话中文草稿".into(),
                    )
                    .unwrap();
                host.open_fixture_child(
                    self.child.as_deref().unwrap_or("bounded-b"),
                    ChildHistoryLayout::SameSession,
                );
                self.phase = 1;
            }
            1 if host.view.bounded_child_view_id().is_some() && window.is_active() => {
                let pane = Self::pane(host);
                self.count = pane.content().row_keys().len();
                let viewport = pane.ids().viewport;
                assert!(Self::scroll(host).reading_document_version().is_some());
                assert!(Self::pane(host).paging_facts().unwrap().has_more_before);
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
                    delta: tekes_ui_scene::Point::new(0., 160.),
                    precise: true,
                    begin: true,
                    end: true,
                    momentum: false,
                    time: 1.,
                });
                self.phase = 2;
            }
            2 => {
                self.item = Some(Self::anchor(host));
                self.y = Self::relative_y(host, self.item.unwrap());
                self.revision = Self::scroll(host)
                    .reading_document_version()
                    .unwrap()
                    .revision();
                Self::read(host, true);
                self.phase = 3;
            }
            3 if Self::scroll(host)
                .reading_document_version()
                .unwrap()
                .revision()
                > self.revision
                && !host
                    .view
                    .tree
                    .get(Self::pane(host).ids().refresh)
                    .unwrap()
                    .component::<tekes_ui_controls::composition::ButtonContent>()
                    .unwrap()
                    .button
                    .state
                    .disabled =>
            {
                eprintln!(
                    "NATIVE_BOUNDED_PAGE_STATE count={} before={} status={:?}",
                    Self::pane(host).content().row_keys().len(),
                    self.count,
                    host.view
                        .tree
                        .get(Self::pane(host).ids().status)
                        .unwrap()
                        .component::<tekes_ui_controls::composition::TextContent>()
                        .unwrap()
                        .text
                );
                if self.child.is_some() {
                    // A one-shot Native child may contain one assistant message;
                    // its older page is real turn metadata, not another UI row.
                    assert!(!Self::pane(host).paging_facts().unwrap().has_more_before);
                    assert!(Self::pane(host).content().row_keys().len() >= self.count);
                } else {
                    assert!(Self::pane(host).content().row_keys().len() > self.count);
                }
                self.unchanged(host);
                self.loaded = Self::pane(host).content().row_keys().len();
                assert!(
                    Self::scroll(host)
                        .reading_document_version()
                        .unwrap()
                        .revision()
                        > self.revision
                );
                self.revision = Self::scroll(host)
                    .reading_document_version()
                    .unwrap()
                    .revision();
                Self::read(host, false);
                self.phase = 4;
            }
            4 if Self::scroll(host)
                .reading_document_version()
                .unwrap()
                .revision()
                > self.revision
                && !host
                    .view
                    .tree
                    .get(Self::pane(host).ids().refresh)
                    .unwrap()
                    .component::<tekes_ui_controls::composition::ButtonContent>()
                    .unwrap()
                    .button
                    .state
                    .disabled =>
            {
                self.unchanged(host);
                assert_eq!(Self::pane(host).content().row_keys().len(), self.loaded);
                self.phase = 5;
                run_script(vec![(0, Action::Close)]);
            }
            _ => {}
        }
        if old != self.phase {
            eprintln!("NATIVE_READONLY_READING_PHASE {}", self.phase);
            window.request_redraw();
        }
    }
    pub fn receipt(&self) -> serde_json::Value {
        assert_eq!(self.phase, 5);
        serde_json::json!({"nativeWindow":true,"realHarness":self.child.is_some(),"visibleRowsExtended":self.loaded>self.count,"historyExhausted":self.child.is_some(),"pageSize":if self.child.is_some(){1}else{50},"gpu":true,"boundedPublicSep":true,"originalPageAndRefreshControls":true,"prependAnchorPreserved":true,"refreshAnchorPreserved":true,"mainDraftPreserved":true,"readOnlyNoComposer":true,"noSubmission":true,"rowCountBefore":self.count,"rowCountAfter":self.loaded,"physicalIme":false,"voiceOver":false})
    }
}
