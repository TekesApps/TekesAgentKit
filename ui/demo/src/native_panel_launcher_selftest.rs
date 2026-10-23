//! Owned-fixture acceptance of the actual empty launcher; not the toolbar shim.
use super::*;
use crate::acceptance_script::{Action, run_script};
use tekes_agent_components_ui::{SidePanelDestination, SidePanelTabKind};
use tekes_ui_controls::composition::ButtonContent;

pub(super) struct Probe {
    phase: u8,
    started: Instant,
    main: Option<Arc<RunBinding>>,
    draft: String,
    submissions: usize,
    chats: usize,
    summary_button: Option<ComponentId>,
}
impl Probe {
    pub fn new() -> Self {
        run_script(vec![(25_000, Action::Close)]);
        Self {
            phase: 0,
            started: Instant::now(),
            main: None,
            draft: String::new(),
            submissions: 0,
            chats: 0,
            summary_button: None,
        }
    }
    fn panel(host: &Host) -> &SidePanel {
        host.view
            .tree
            .get(host.view.panel_id())
            .unwrap()
            .component::<SidePanel>()
            .unwrap()
    }
    fn selected(host: &Host) -> Option<SidePanelTabKind> {
        Self::panel(host)
            .displayed_input()
            .unwrap()
            .selected
            .clone()
    }
    fn button(host: &Host, destination: SidePanelDestination) -> ComponentId {
        Self::panel(host)
            .launcher()
            .unwrap()
            .button_id(destination)
            .unwrap()
    }
    fn click(host: &Host, id: ComponentId) {
        let point = host
            .view
            .tree
            .get(id)
            .unwrap()
            .visible()
            .expect("original visible launcher/control")
            .center();
        run_script(vec![(
            0,
            Action::Click {
                x: point.x.into(),
                y: point.y.into(),
            },
        )]);
    }
    fn chat_count(&self, host: &Host) -> usize {
        host.side_chats
            .snapshot()
            .chats
            .values()
            .filter(|chat| &chat.parent == self.main.as_ref().unwrap().scope())
            .count()
    }
    pub fn advance(&mut self, host: &mut Host, window: &Window) {
        assert!(
            self.started.elapsed() < Duration::from_secs(25),
            "launcher phase {}",
            self.phase
        );
        if !window.is_active() {
            return;
        }
        let old = self.phase;
        match self.phase {
            0 => {
                self.main = Some(host.view.binding().clone());
                self.submissions = self
                    .main
                    .as_ref()
                    .unwrap()
                    .current_run()
                    .unwrap()
                    .submissions
                    .len();
                self.chats = self.chat_count(host);
                Self::click(host, Self::panel(host).close_id());
                self.phase = 1;
            }
            1 if Self::selected(host).is_none() => {
                Self::click(host, host.view.ids().editor);
                self.phase = 2;
            }
            2 if host.view.tree.focused() == Some(host.view.ids().editor) => {
                run_script(vec![(
                    0,
                    Action::PreeditRange {
                        text: "Launcher中文组字".into(),
                        selection: 1..3,
                        replacement: None,
                    },
                )]);
                self.phase = 3;
            }
            3 if host.view.composing() => {
                let editor = host.view.ids().editor;
                let owner = host.view.tree.text_surface(editor).unwrap() as *const _ as usize;
                let baseline = host.view.editor().input.external_baseline();
                for destination in [
                    SidePanelDestination::Summary,
                    SidePanelDestination::SideChat,
                ] {
                    let id = Self::button(host, destination);
                    assert!(
                        host.view
                            .tree
                            .get(id)
                            .unwrap()
                            .component::<ButtonContent>()
                            .unwrap()
                            .button
                            .state
                            .disabled
                    );
                    assert!(
                        host.view
                            .accessibility_action(id, None, AccessibilityAction::Press)
                            .unwrap()
                            .is_empty()
                    );
                }
                assert_eq!(host.view.tree.focused(), Some(editor));
                assert_eq!(
                    host.view.tree.text_surface(editor).unwrap() as *const _ as usize,
                    owner
                );
                assert_eq!(host.view.editor().input.external_baseline(), baseline);
                self.draft = host.view.editor().input.text.clone();
                run_script(vec![(0, Action::Unmark)]);
                self.phase = 4;
            }
            4 if !host.view.composing()
                && self
                    .main
                    .as_ref()
                    .unwrap()
                    .current_run()
                    .unwrap()
                    .draft
                    .text
                    == self.draft =>
            {
                let id = Self::button(host, SidePanelDestination::Summary);
                self.summary_button = Some(id);
                Self::click(host, id);
                self.phase = 5;
            }
            5 if Self::selected(host) == Some(SidePanelTabKind::Summary) => {
                Self::click(host, Self::panel(host).close_id());
                self.phase = 6;
            }
            6 if Self::selected(host).is_none() => {
                Self::click(host, Self::button(host, SidePanelDestination::SideChat));
                self.phase = 7;
            }
            7 if matches!(
                Self::selected(host),
                Some(SidePanelTabKind::SideChat { .. })
            ) && host.view.panel_composer_ids().is_some() =>
            {
                assert_eq!(self.chat_count(host), self.chats + 1);
                assert_eq!(
                    self.main
                        .as_ref()
                        .unwrap()
                        .current_run()
                        .unwrap()
                        .draft
                        .text,
                    self.draft
                );
                Self::click(host, Self::panel(host).close_id());
                self.phase = 8;
            }
            8 if Self::selected(host).is_none() && self.chat_count(host) == self.chats => {
                assert_ne!(
                    Self::button(host, SidePanelDestination::Summary),
                    self.summary_button.unwrap()
                );
                assert!(host.view.tree.get(self.summary_button.unwrap()).is_none());
                let main = self.main.as_ref().unwrap();
                assert_eq!(host.view.binding().scope(), main.scope());
                assert_eq!(main.current_run().unwrap().draft.text, self.draft);
                assert_eq!(
                    main.current_run().unwrap().submissions.len(),
                    self.submissions
                );
                self.phase = 9;
                run_script(vec![(0, Action::Close)]);
            }
            _ => {}
        }
        if old != self.phase {
            eprintln!("NATIVE_PANEL_LAUNCHER_PHASE {}", self.phase);
            window.request_redraw();
        }
    }
    pub fn receipt(&self) -> serde_json::Value {
        assert_eq!(self.phase, 9, "launcher native acceptance incomplete");
        serde_json::json!({"nativeWindow":true,"originalLauncherButtons":true,"summaryOpenClose":true,"singleScratchCreateAndDiscard":true,"originalMainOwnerAndMarkedBaseline":true,"preeditNavigationBlocked":true,"legitimateUnmarkRetainsMainDraft":true,"retiredLauncherIdsRejected":true,"noSubmission":true,"ownedFixture":true,"syntheticPreedit":true,"realHarness":false,"physicalIme":false,"voiceOver":false,"icons":false,"processContainer":false})
    }
}
