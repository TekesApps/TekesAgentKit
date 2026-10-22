//! Owned SEP facts -> original presentation/RunPort -> native temporal aside.
use super::*;
use tekes_agent_components_ui::{OutputUsage, RunPort, TranscriptContentRowKey, TurnFootnote};
use tekes_agent_ui_demo::fixture::Fixture;
use tekes_ui_controls::composition::TextContent;
use tekes_ui_platform_macos::selftest::{Action, run_script};
pub(super) struct Probe {
    fixture: Fixture,
    phase: u8,
    started: Instant,
    footer: Option<ComponentId>,
    info: Option<ComponentId>,
    original_info: usize,
    original_editor: usize,
    baseline: Option<tekes_ui_controls::TextEditBaseline>,
    observed_clock: f64,
    domain_revision: u64,
    revealed: bool,
    capture: Option<String>,
}
impl Probe {
    pub fn new(fixture: Fixture) -> Self {
        run_script(vec![(20000, Action::Close)]);
        let capture = std::env::var("AGENT_FOOTNOTE_TIME_CAPTURE").ok();
        if let Some(path) = &capture {
            assert!(
                std::path::Path::new(path).is_absolute() && !std::path::Path::new(path).exists(),
                "use a new absolute capture path"
            );
        }
        Self {
            fixture,
            phase: 0,
            started: Instant::now(),
            footer: None,
            info: None,
            original_info: 0,
            original_editor: 0,
            baseline: None,
            observed_clock: 0.,
            domain_revision: 0,
            revealed: false,
            capture,
        }
    }
    fn footer<'a>(&self, host: &'a Host) -> &'a TurnFootnote {
        host.view
            .tree
            .get(self.footer.unwrap())
            .unwrap()
            .component::<TurnFootnote>()
            .unwrap()
    }
    fn empty_history_aside_hidden(host: &Host) {
        let snapshot = host.view.binding().current_run().unwrap();
        let turn = snapshot
            .transcript
            .turns
            .iter()
            .find(|turn| turn.input == "会话 a · 历史问题 30")
            .unwrap();
        assert!(turn.ended_at.is_some() && turn.footnote_actions.is_empty());
        let run = host
            .view
            .tree
            .get(ComponentId(host.view.ids().root.0 + 16))
            .unwrap()
            .component::<RunPort>()
            .unwrap();
        let transcript = run.transcript().unwrap();
        let id = transcript
            .row_id(&TranscriptContentRowKey::TurnFootnote(turn.id.clone()))
            .unwrap();
        let footer = transcript
            .children()
            .iter()
            .find(|node| node.id() == id)
            .unwrap()
            .component::<TurnFootnote>()
            .unwrap();
        assert!(!footer.active() && footer.information_id().is_none());
        assert!(!footer.facts().unwrap().display.has_output);
        assert!(footer.copy_button_id().is_none());
    }
    fn unchanged(&self, host: &Host) {
        assert_eq!(host.view.tree.focused(), Some(host.view.ids().editor));
        assert_eq!(
            host.view.editor() as *const _ as usize,
            self.original_editor
        );
        assert_eq!(
            Some(host.view.editor().input.external_baseline()),
            self.baseline
        );
        assert!(host.view.editor().input.marked.is_some());
        assert_eq!(self.footer(host).information_id(), self.info);
        let info = host.view.tree.get(self.info.unwrap()).unwrap();
        assert!(
            info.visible().is_some(),
            "actual footnote must remain in the native viewport"
        );
        assert_eq!(
            info.component::<TextContent>().unwrap() as *const _ as usize,
            self.original_info
        );
        assert!(
            host.view
                .binding()
                .current_run()
                .unwrap()
                .submissions
                .is_empty()
        );
    }
    pub fn advance(&mut self, host: &mut Host, window: &Window) {
        assert!(
            self.started.elapsed() < Duration::from_secs(20),
            "footnote time deadline phase {}",
            self.phase
        );
        let old = self.phase;
        match self.phase {
            0 if window.is_active() => {
                Self::empty_history_aside_hidden(host);
                let snapshot = host.view.binding().current_run().unwrap();
                let Some(turn) = snapshot
                    .transcript
                    .turns
                    .iter()
                    .find(|t| t.input == "计时验证输入")
                else {
                    return;
                };
                let root = ComponentId(host.view.ids().root.0 + 16);
                let run = host
                    .view
                    .tree
                    .get(root)
                    .unwrap()
                    .component::<RunPort>()
                    .unwrap();
                let Some(footer) = run
                    .transcript()
                    .unwrap()
                    .row_id(&TranscriptContentRowKey::TurnFootnote(turn.id.clone()))
                else {
                    return;
                };
                self.footer = Some(footer);
                let facts = self.footer(host).facts().unwrap();
                if !facts.display.is_live_owner {
                    return;
                }
                self.info = self.footer(host).information_id();
                let info = host.view.tree.get(self.info.unwrap()).unwrap();
                if info.visible().is_none() && !self.revealed {
                    let target = run.turn_targets().unwrap().pop().unwrap();
                    let row = run.turn_rail().unwrap().row_id(target.turn_id()).unwrap();
                    host.view
                        .tree
                        .accessibility_action(row, None, AccessibilityAction::Press);
                    let commands = host.view.drain_commands().unwrap();
                    host.commands(Ok(commands));
                    self.revealed = true;
                    window.request_redraw();
                    return;
                }
                assert!(info.visible().is_some());
                assert!(!facts.text().contains("running") && !facts.text().contains("0 tokens"));
                assert!(
                    host.footnote_wake.is_some(),
                    "visible live footnote uses an owned host clock wake"
                );
                self.original_info = info.component::<TextContent>().unwrap() as *const _ as usize;
                self.original_editor = host.view.editor() as *const _ as usize;
                let point = host
                    .view
                    .tree
                    .get(host.view.ids().editor)
                    .unwrap()
                    .visible()
                    .unwrap()
                    .center();
                run_script(vec![
                    (
                        0,
                        Action::Click {
                            x: point.x.into(),
                            y: point.y.into(),
                        },
                    ),
                    (
                        100,
                        Action::PreeditRange {
                            text: "计时更新期间中文组字".into(),
                            selection: 1..3,
                            replacement: None,
                        },
                    ),
                ]);
                self.phase = 1;
            }
            1 if host.view.editor().input.marked.is_some() => {
                self.baseline = Some(host.view.editor().input.external_baseline());
                self.domain_revision = host.view.binding().current_run().unwrap().revision;
                self.observed_clock = self.footer(host).facts().unwrap().display.now_ms.unwrap();
                self.phase = 2;
            }
            2 => {
                self.unchanged(host);
                let revision = host.view.binding().current_run().unwrap().revision;
                let clock = self.footer(host).facts().unwrap().display.now_ms.unwrap();
                if revision != self.domain_revision {
                    self.domain_revision = revision;
                    self.observed_clock = clock;
                    return;
                }
                if clock <= self.observed_clock {
                    return;
                }
                assert_eq!(
                    host.view
                        .tree
                        .get(self.info.unwrap())
                        .unwrap()
                        .component::<TextContent>()
                        .unwrap()
                        .text,
                    self.footer(host).facts().unwrap().text()
                );
                self.fixture.publish_timed_usage(0).unwrap();
                self.phase = 3;
            }
            3 => {
                self.unchanged(host);
                if self.footer(host).facts().unwrap().output_usage
                    != Some(OutputUsage::Live { output_tokens: 0 })
                {
                    return;
                }
                assert!(
                    !self
                        .footer(host)
                        .facts()
                        .unwrap()
                        .text()
                        .contains("0 tokens")
                );
                self.fixture.publish_timed_usage(1200).unwrap();
                self.phase = 4;
            }
            4 => {
                self.unchanged(host);
                if self.footer(host).facts().unwrap().output_usage
                    != Some(OutputUsage::Live {
                        output_tokens: 1200,
                    })
                {
                    return;
                }
                assert!(
                    self.footer(host)
                        .facts()
                        .unwrap()
                        .text()
                        .contains("1.2k tokens")
                );
                self.fixture.finish_timed_turn().unwrap();
                self.phase = 5;
            }
            5 => {
                self.unchanged(host);
                Self::empty_history_aside_hidden(host);
                let facts = self.footer(host).facts().unwrap();
                if facts.status != "completed" {
                    return;
                }
                assert!(!facts.display.is_live_owner);
                assert!(facts.display.finished_text.is_some());
                let expected = tekes_agent_ui_demo::footnote_host::day_clock(
                    facts.ended_at.unwrap(),
                    facts.display.now_ms,
                )
                .unwrap();
                assert_eq!(facts.text(), expected);
                assert_eq!(
                    host.view
                        .tree
                        .get(self.info.unwrap())
                        .unwrap()
                        .component::<TextContent>()
                        .unwrap()
                        .text,
                    expected
                );
                assert!(!expected.contains("completed") && !expected.contains("tokens"));
                assert!(
                    host.footnote_wake.is_none(),
                    "settled footnote retires only its owned host wake"
                );
                let mut actions = vec![];
                if let Some(path) = &self.capture {
                    actions.push((300, Action::Capture { path: path.clone() }));
                }
                actions.push((700, Action::Close));
                run_script(actions);
                self.phase = 6;
            }
            _ => {}
        }
        if old != self.phase {
            eprintln!("NATIVE_FOOTNOTE_TIME_PHASE {}", self.phase);
            window.request_redraw();
        }
    }
    pub fn receipt(&self) -> serde_json::Value {
        assert_eq!(self.phase, 6);
        serde_json::json!({"nativeWindow":true,"ownedSepBackhaul":true,"displayClockWithoutDomainPublication":true,
            "onlyLiveOwnerTicks":true,"zeroTokensOmitted":true,"compactPositiveTokens":true,"settledCalendarTimestamp":true,
            "originalInformationAndComposer":true,"focusAndMarkedBaseline":true,"ownedWakeRetired":true,
            "emptyHistoricalOutputAsideHidden":true,"timestampDoesNotInventOutput":true,
            "noSubmission":true,"syntheticPreedit":true,"physicalIme":false,"realHarness":false,"voiceOver":false,"fullFootnoteLayoutParity":false})
    }
}
