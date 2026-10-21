//! Candidate-only native host diagnostic over the actual original RunView.
//! Original rail pointer activation returns to latest while AppKit preedit is active.
//! Ordinary turn Start and keyboard focus-ring visuals remain upstream gaps.
use super::*;
use tekes_agent_components_ui::{RunTurnNavigationKind, RunView};
use tekes_ui_controls::{
    ReadingDocumentVersion, ScrollRevealOptions, ScrollRevealPlacement,
    composition::{ContentRevealStatus, ScrollContent},
};
use tekes_ui_platform_macos::selftest::{Action, run_script};
pub(super) struct Probe {
    phase: u8,
    rail_version: Option<ReadingDocumentVersion>,
    rail_dispatches: usize,
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
            rail_version: None,
            rail_dispatches: 0,
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
            if Self::run(host).displayed_snapshot().unwrap().revision != self.revision {
                eprintln!(
                    "RAIL_CHANGED phase={} composing={} focus={:?} draft={:?} editor={:?} submissions={}",
                    self.phase,
                    Self::run(host).composer().is_composing(),
                    host.view.tree.focused(),
                    Self::run(host).displayed_snapshot().unwrap().draft.text,
                    Self::run(host).composer().editor().input.text,
                    host.view.binding().current_run().unwrap().submissions.len()
                );
            }
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
            1 => {
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
        if self.phase == 1 {
            window.request_redraw();
        }
    }
    pub fn after_render(&mut self, host: &mut Host, window: &Window) {
        // Let the original main reveal finish before issuing a different owner's reveal.
        if self.phase >= 2 && self.phase != 4 {
            let root = Self::root(host);
            let following = Self::scroll(host).is_following_latest();
            let run = host
                .view
                .tree
                .get_mut(root)
                .unwrap()
                .component_mut::<RunView>()
                .unwrap();
            run.update_rail_follow_target(following).unwrap();
            if let Some(request) = run.turn_rail_mut().unwrap().next_reveal() {
                run.turn_rail().unwrap().claim_reveal(&request).unwrap();
                let result = host.view.tree.reveal_scroll_reading_item(
                    request.viewport(),
                    self.rail_version.as_ref().unwrap(),
                    request.item(),
                    ScrollRevealOptions {
                        placement: match request.reason() {
                            tekes_agent_components_ui::TurnRailRevealReason::InitialLatest => {
                                ScrollRevealPlacement::Nearest
                            }
                            tekes_agent_components_ui::TurnRailRevealReason::FollowChanged => {
                                ScrollRevealPlacement::Centered
                            }
                        },
                        animated: false,
                        ..Default::default()
                    },
                    &mut host.text,
                );
                host.view
                    .tree
                    .get_mut(root)
                    .unwrap()
                    .component_mut::<RunView>()
                    .unwrap()
                    .turn_rail_mut()
                    .unwrap()
                    .finish_reveal(&request, result.is_ok());
                result.unwrap();
                window.request_redraw();
                return;
            }
        }
        if self.phase == 5 && self.latest_after.is_none_or(|time| Instant::now() >= time) {
            let view = Self::run(host);
            let target = view.turn_targets().unwrap().pop().unwrap();
            let rail = view.turn_rail().unwrap();
            let row = rail.row_id(target.turn_id()).unwrap();
            // Click the original currently visible clipped hit rectangle; a
            // full-row center may be outside the viewport's focus gutter.
            let point = host.view.tree.get(row).unwrap().visible().unwrap().center();
            assert!(view.is_nonfocusing_navigation_at(point));
            self.phase = 8;
            run_script(vec![(
                100,
                Action::Click {
                    x: point.x as f64,
                    y: point.y as f64,
                },
            )]);
        }
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
                let root = Self::root(host);
                host.view
                    .tree
                    .get_mut(root)
                    .unwrap()
                    .component_mut::<RunView>()
                    .unwrap()
                    .set_rail_navigation_gate(tekes_agent_components_ui::TurnRailNavigationGate {
                        turn_start: false,
                        latest: true,
                    })
                    .unwrap();
                let rail = Self::run(host).turn_rail().unwrap();
                let (rail_version, ()) = host
                    .view
                    .tree
                    .set_scroll_reading_document(
                        rail.viewport_id(),
                        "native-agent-rail",
                        rail.document_revision(),
                        rail.item_ids(),
                        |_| {},
                    )
                    .unwrap();
                self.rail_version = Some(rail_version);
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
                assert_eq!(self.rail_dispatches, 1);
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
            "programmaticItemNavigation":true,"interactiveLatestRail":true,"railPointerDispatches":self.rail_dispatches,"keyboardRingVisualAccepted":false,"clientkitTurnStartAlignment":false,"initialAndExplicitLatest":true,"syntheticAppKitPreeditPreserved":true,
            "originalEditorAndBaseline":true,"noSubmission":true,"realHarness":false,"physicalIme":false,"interactiveRail":true,"ordinaryTurnStart":false,"voiceOver":false})
    }
}

/// Candidate-only synchronous native command bridge. No scroll offset, focus
/// transfer, fallback, endpoint call or second editor is introduced here.
pub(super) fn dispatch(
    host: &mut Host,
    root: ComponentId,
    intent: &tekes_agent_components_ui::RunTurnNavigation,
) -> std::result::Result<(), String> {
    let probe = host
        .item_navigation_probe
        .as_ref()
        .ok_or("rail diagnostic inactive")?;
    if probe.phase != 8 || root != Probe::root(host) {
        return Err("unexpected rail command source".into());
    }
    let version = probe
        .version
        .as_ref()
        .ok_or("missing reading attachment")?
        .clone();
    let view = Probe::run(host);
    let kind = view.claim_turn_navigation(intent).map_err(str::to_owned)?;
    if kind != RunTurnNavigationKind::Latest {
        return Err("Start alignment unavailable in this candidate".into());
    }
    if Probe::scroll(host).reading_document_version() != Some(&version) {
        return Err("stale native reading attachment".into());
    }
    host.view
        .tree
        .get_mut(intent.viewport())
        .unwrap()
        .component_mut::<ScrollContent>()
        .unwrap()
        .follow_latest();
    let probe = host.item_navigation_probe.as_mut().unwrap();
    probe.rail_dispatches += 1;
    probe.phase = 6;
    Ok(())
}
