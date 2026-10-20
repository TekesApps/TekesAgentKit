//! Candidate-only native host diagnostic over the actual original RunView.
//! The navigation is programmatic; preedit enters through the AppKit bridge.
//! This is not an interactive indicator rail or physical IME acceptance.
use super::*;
use tekes_agent_components_ui::{RunTurnNavigationKind, RunView};
use tekes_ui_controls::{
    ReadingDocumentVersion, ScrollRevealOptions, ScrollRevealPlacement,
    composition::{ContentRevealStatus, ScrollContent},
};
use tekes_ui_platform_macos::selftest::{Action, run_script};
pub(super) struct Probe {
    phase: u8,
    started: Instant,
    version: Option<ReadingDocumentVersion>,
    revision: u64,
    baseline: Option<tekes_ui_controls::TextEditBaseline>,
    editor: usize,
    turn: Option<String>,
    submissions: usize,
    latest_after: Option<Instant>,
}
impl Probe {
    pub fn new() -> Self {
        Self {
            phase: 0,
            started: Instant::now(),
            version: None,
            revision: 0,
            baseline: None,
            editor: 0,
            turn: None,
            submissions: 0,
            latest_after: None,
        }
    }
    fn root(host: &Host) -> ComponentId {
        ComponentId(host.view.ids().root.0 + 16)
    }
    fn run(host: &Host) -> &RunView {
        host.view
            .tree
            .get(Self::root(host))
            .unwrap()
            .component::<RunView>()
            .unwrap()
    }
    fn scroll(host: &Host) -> &ScrollContent {
        host.view
            .tree
            .get(Self::run(host).ids().viewport)
            .unwrap()
            .component::<ScrollContent>()
            .unwrap()
    }
    fn check_editor(&self, host: &Host) {
        let view = Self::run(host);
        assert_eq!(view.composer().editor() as *const _ as usize, self.editor);
        assert_eq!(host.view.tree.focused(), Some(view.ids().composer.editor));
        assert!(view.composer().is_composing());
        assert_eq!(
            &view.composer().editor().input.external_baseline(),
            self.baseline.as_ref().unwrap()
        );
        assert_eq!(
            host.view.binding().current_run().unwrap().submissions.len(),
            self.submissions
        );
    }
    pub fn before_render(&mut self, host: &mut Host, window: &Window) {
        assert!(
            self.started.elapsed() < Duration::from_secs(30),
            "native item navigation deadline, phase {}",
            self.phase
        );
        if self.version.is_some() {
            assert_eq!(
                Self::run(host).displayed_snapshot().unwrap().revision,
                self.revision,
                "diagnostic does not exercise canonical stream updates"
            );
        }
        // Hold this diagnostic frame for its asynchronous window capture;
        // this timer is not part of application scroll/navigation policy.
        if self.phase == 5 && self.latest_after.is_some_and(|time| Instant::now() < time) {
            return;
        }
        match self.phase {
            1 | 5 => {
                let view = Self::run(host);
                let target = view.turn_targets().unwrap().pop().unwrap();
                let request = view.prepare_turn_navigation(&target).unwrap();
                assert_eq!(
                    view.claim_turn_navigation(&request).unwrap(),
                    RunTurnNavigationKind::Latest
                );
                assert_eq!(
                    Self::scroll(host).reading_document_version(),
                    self.version.as_ref()
                );
                host.view
                    .tree
                    .get_mut(request.viewport())
                    .unwrap()
                    .component_mut::<ScrollContent>()
                    .unwrap()
                    .follow_latest();
                self.phase = if self.phase == 1 { 2 } else { 6 };
            }
            3 if Self::run(host).composer().is_composing() => {
                let view = Self::run(host);
                self.baseline = Some(view.composer().editor().input.external_baseline());
                self.editor = view.composer().editor() as *const _ as usize;
                self.submissions = host.view.binding().current_run().unwrap().submissions.len();
                let target = view.turn_targets().unwrap().remove(0);
                self.turn = Some(target.turn_id().to_owned());
                let request = view.prepare_turn_navigation(&target).unwrap();
                let RunTurnNavigationKind::TurnStart(item) =
                    view.claim_turn_navigation(&request).unwrap()
                else {
                    panic!("first turn must be an item")
                };
                let result = host
                    .view
                    .tree
                    .reveal_scroll_reading_item(
                        request.viewport(),
                        self.version.as_ref().unwrap(),
                        item,
                        ScrollRevealOptions {
                            placement: ScrollRevealPlacement::Centered,
                            animated: true,
                            ..Default::default()
                        },
                        &mut host.text,
                    )
                    .unwrap();
                assert!(matches!(
                    result.status,
                    ContentRevealStatus::Animating | ContentRevealStatus::Completed
                ));
                self.phase = 4;
                self.check_editor(host);
            }
            _ => {}
        }
        if matches!(self.phase, 1 | 5) {
            window.request_redraw();
        }
    }
    pub fn after_render(&mut self, host: &mut Host, window: &Window) {
        match self.phase {
            0 => {
                let view = Self::run(host);
                let Some(snapshot) = view.displayed_snapshot() else {
                    return;
                };
                self.revision = snapshot.revision;
                let viewport = view.ids().viewport;
                let items = view
                    .transcript()
                    .unwrap()
                    .row_keys()
                    .iter()
                    .map(|key| view.transcript().unwrap().row_id(key).unwrap())
                    .collect();
                let key = tekes_agent_components_ui::ComposerDestination::run(view.scope().clone())
                    .storage_key();
                let (version, ()) = host
                    .view
                    .tree
                    .set_scroll_reading_document(viewport, key, self.revision, items, |_| {})
                    .unwrap();
                self.version = Some(version);
                self.phase = 1;
                run_script(vec![(30000, Action::Close)]);
                window.request_redraw();
            }
            2 => {
                assert!(Self::scroll(host).is_following_latest());
                let editor = Self::run(host).ids().composer.editor;
                let point = host.view.tree.get(editor).unwrap().frame().center();
                self.phase = 3;
                run_script(vec![
                    (
                        100,
                        Action::Click {
                            x: point.x as f64,
                            y: point.y as f64,
                        },
                    ),
                    (200, Action::Preedit("中文组字".into())),
                    (400, Action::WakeRedraw),
                ]);
            }
            4 => {
                self.check_editor(host);
                let result = host.view.tree.scroll_reading_reveal_result().unwrap();
                if result.status == ContentRevealStatus::Animating {
                    window.request_redraw();
                    return;
                }
                assert_eq!(result.status, ContentRevealStatus::Completed);
                assert!(
                    Self::run(host)
                        .turn_targets()
                        .unwrap()
                        .iter()
                        .any(|t| Some(t.turn_id()) == self.turn.as_deref()
                            && t.is_visible() == Some(true))
                );
                assert!(!Self::scroll(host).is_following_latest());
                self.phase = 5;
                self.latest_after = Some(Instant::now() + Duration::from_millis(900));
                if let Ok(path) = std::env::var("AGENT_ITEM_NAV_CAPTURE") {
                    run_script(vec![
                        (100, Action::Capture { path }),
                        (1200, Action::WakeRedraw),
                    ]);
                } else {
                    window.request_redraw();
                }
            }
            6 => {
                self.check_editor(host);
                assert!(Self::scroll(host).is_following_latest());
                self.phase = 7;
                let mut actions = vec![];
                if let Ok(path) = std::env::var("AGENT_ITEM_NAV_LATEST_CAPTURE") {
                    actions.push((100, Action::Capture { path }));
                }
                actions.push((500, Action::Close));
                run_script(actions);
            }
            _ => {}
        }
    }
    pub fn receipt(&self, host: &Host) -> serde_json::Value {
        assert_eq!(self.phase, 7);
        self.check_editor(host);
        serde_json::json!({"nativeWindow":true,"actualRunView":true,"originalBackend":true,"secondPublicConsumer":false,
            "programmaticItemNavigation":true,"clientkitTurnStartAlignment":false,"initialAndExplicitLatest":true,"syntheticAppKitPreeditPreserved":true,
            "originalEditorAndBaseline":true,"noSubmission":true,"realHarness":false,"physicalIme":false,"interactiveRail":false,"voiceOver":false})
    }
}
