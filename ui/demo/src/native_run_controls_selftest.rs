//! Owned-session native Send/Cancel/provider-error/recovery probe, normal callbacks only.
use super::*;
use crate::acceptance_script::{Action, run_script};
use tekes_agent_components_ui::{
    IndicatorFacts, Indicators, OutputContentKind, OutputMessage, TranscriptContent,
};
use tekes_agent_presentation::{RunSnapshot, TranscriptItemKind};
pub(super) const CANCEL: &str = "ENDPOINT_CANCEL_NATIVE_PROOF";
const ERROR: &str = "ENDPOINT_ERROR_NATIVE_PROOF";
const RECOVER: &str = "ENDPOINT_RECOVER_NATIVE_PROOF";
const OTHER: &str = "另一会话取消测试草稿";
pub(super) struct Probe {
    started: Instant,
    phase: u8,
    deadline: bool,
    epoch: u64,
    reported_error: Option<String>,
}
impl Probe {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            phase: 0,
            deadline: false,
            epoch: 0,
            reported_error: None,
        }
    }
    pub fn prepare(view: &ComponentDemo) {
        assert_eq!(view.bindings().len(), 2);
        for (index, binding) in view.bindings().iter().enumerate() {
            let run = binding.current_run().unwrap();
            assert!(
                run.transcript.items.is_empty() && run.draft.text.is_empty(),
                "fresh owned sessions required"
            );
            binding
                .edit_text(
                    &tekes_agent_presentation::ComposerDestination::run(binding.scope().clone()),
                    if index == 0 { CANCEL } else { OTHER }.into(),
                )
                .unwrap();
        }
    }
    fn click(view: &ComponentDemo, id: ComponentId) -> Action {
        let node = view.tree.get(id).unwrap();
        let p = node.frame().center();
        assert!(
            node.visible().is_some_and(|rect| rect.contains(p)),
            "native run control clipped {id:?}"
        );
        Action::Click {
            x: p.x as f64,
            y: p.y as f64,
        }
    }
    fn post(&mut self, phase: u8, actions: Vec<Action>) {
        self.phase = phase;
        eprintln!(
            "NATIVE_RUN_CONTROL_PHASE {phase} elapsed_ms={}",
            self.started.elapsed().as_millis()
        );
        run_script(actions.into_iter().map(|action| (0, action)).collect());
    }
    fn status(run: &RunSnapshot) -> Option<&str> {
        run.transcript.turns.last().map(|turn| turn.status.as_str())
    }
    fn users(run: &RunSnapshot) -> Vec<&str> {
        run.transcript
            .items
            .iter()
            .filter(|item| {
                item.kind == TranscriptItemKind::User
                    && item.metadata["source"]["kind"]
                        .as_str()
                        .is_none_or(|kind| kind == "user")
            })
            .map(|item| item.text.as_str())
            .collect()
    }
    fn facts(view: &ComponentDemo) -> Option<&IndicatorFacts> {
        view.tree
            .get(ComponentId(view.ids().root.0 + 17))?
            .component::<Indicators>()?
            .facts()
    }
    fn check(&self, view: &ComponentDemo, count: usize) {
        let run = view.binding().current_run().unwrap();
        let users = Self::users(&run);
        assert_eq!(users.len(), count, "no replay or duplicate echo");
        assert_eq!(users[0], CANCEL);
        if count >= 2 {
            assert_eq!(users[1], ERROR);
        }
        if count >= 3 {
            assert_eq!(users[2], RECOVER);
        }
        assert!(run.draft.text.is_empty());
        let other = view.bindings()[1].current_run().unwrap();
        assert_eq!(other.draft.text, OTHER);
        assert!(Self::users(&other).is_empty());
    }
    pub fn advance(&mut self, host: &Host, window: &Window) {
        window.set_animating(false);
        if !self.deadline {
            self.deadline = true;
            run_script(vec![(
                Duration::from_secs(45)
                    .saturating_sub(self.started.elapsed())
                    .as_millis() as u64,
                Action::WakeRedraw,
            )]);
        }
        assert!(
            self.started.elapsed() < Duration::from_secs(45),
            "native run controls timed out phase {}",
            self.phase
        );
        // Like the stream probe, wait for a real activation. Do not start
        // another action phase while an unrelated window is active.
        // The original deadline still fails if activation/focus never returns.
        if !window.is_active() {
            return;
        }
        let view = &host.view;
        let run = view.binding().current_run().unwrap();
        match self.phase {
            0 if run.can_submit && view.editor().input.text == CANCEL => {
                self.post(1, vec![Self::click(view, view.ids().send)])
            }
            1 if run.run.as_ref().is_some_and(|run| run.can_cancel)
                && run.transcript.items.iter().any(|item| {
                    item.is_provisional
                        && item.kind == TranscriptItemKind::Assistant
                        && item.text.contains("CANCEL_PENDING")
                }) =>
            {
                let facts = IndicatorFacts::read(&run);
                if Self::facts(view) != Some(&facts) {
                    return;
                }
                assert_eq!(facts.run, Some(tekes_agent_presentation::RunPhase::Running));
                assert!(
                    !view
                        .tree
                        .get(view.ids().cancel)
                        .unwrap()
                        .component::<tekes_ui_controls::composition::Button>()
                        .unwrap()
                        .button
                        .state
                        .disabled
                );
                self.post(2, vec![Self::click(view, view.ids().cancel)]);
            }
            2 if Self::status(&run) == Some("aborted")
                && run.run.as_ref().is_some_and(|run| !run.can_cancel)
                && run.can_submit
                && !run.transcript.is_stale =>
            {
                self.check(view, 1);
                self.epoch = run.connection_epoch;
                self.post(3, vec![Self::click(view, RECONNECT)]);
            }
            3 if run.connection_epoch > self.epoch
                && !run.transcript.is_stale
                && run.can_submit =>
            {
                self.check(view, 1);
                assert_eq!(Self::status(&run), Some("aborted"));
                self.post(40, vec![Self::click(view, view.ids().editor)]);
            }
            40 if view.tree.focused() == Some(view.ids().editor) => {
                self.post(
                    4,
                    vec![
                        Action::Text(ERROR.into()),
                        Action::ExpectText {
                            text: ERROR.into(),
                            marked: false,
                        },
                    ],
                );
            }
            4 if run.can_submit && run.draft.text == ERROR && view.editor().input.text == ERROR => {
                self.post(5, vec![Self::click(view, view.ids().send)]);
            }
            5 if Self::status(&run) == Some("error")
                && run.run.as_ref().is_some_and(|run| !run.can_cancel)
                && !run.transcript.is_stale =>
            {
                let facts = IndicatorFacts::read(&run);
                if Self::facts(view) != Some(&facts) {
                    return;
                }
                self.check(view, 2);
                let reported = run
                    .transcript
                    .latest_failure()
                    .and_then(|failure| failure.message)
                    .expect("public source provides a failure explanation");
                assert!(facts.last_turn_failed);
                assert_eq!(
                    facts.run_error.as_deref(),
                    Some(reported),
                    "mounted Indicators must expose the canonical public failure"
                );
                let content = view
                    .tree
                    .get(view.transcript_content_id(view.ids()).unwrap())
                    .unwrap()
                    .component::<TranscriptContent>()
                    .unwrap();
                let Some((body, text, selection)) = content
                    .row_keys()
                    .iter()
                    .filter_map(|key| content.row_id(key))
                    .filter_map(|id| view.tree.get(id)?.component::<OutputMessage>())
                    .flat_map(OutputMessage::content_ids)
                    .filter(|ids| {
                        matches!(
                            ids.kind,
                            OutputContentKind::Status | OutputContentKind::Text
                        )
                    })
                    .find_map(|ids| {
                        let surface = view.tree.text_surface(ids.body)?;
                        let text = surface.input.text.clone();
                        text.contains(reported).then_some((
                            ids.body,
                            text,
                            surface.input.selection(),
                        ))
                    })
                else {
                    return;
                };
                let start = text[..text.find(reported).unwrap()].encode_utf16().count();
                assert_eq!(
                    selection.start, selection.end,
                    "new error owner has no user selection"
                );
                self.reported_error = Some(reported.into());
                self.post(
                    60,
                    vec![
                        Action::ExpectAXText {
                            component: body.0,
                            text,
                            selection,
                            selected: String::new(),
                            query: start..start + reported.encode_utf16().count(),
                            substring: Some(reported.into()),
                        },
                        Self::click(view, view.ids().editor),
                    ],
                );
            }
            60 if view.tree.focused() == Some(view.ids().editor) => {
                self.post(
                    6,
                    vec![
                        Action::Text(RECOVER.into()),
                        Action::ExpectText {
                            text: RECOVER.into(),
                            marked: false,
                        },
                    ],
                );
            }
            6 if run.can_submit
                && run.draft.text == RECOVER
                && view.editor().input.text == RECOVER =>
            {
                self.post(7, vec![Self::click(view, view.ids().send)]);
            }
            7 if Self::users(&run).len() == 3
                && Self::status(&run) == Some("completed")
                && run.transcript.items.iter().any(|item| {
                    run.transcript
                        .turns
                        .last()
                        .is_some_and(|turn| item.turn_id == turn.id)
                        && item.is_final
                        && item.kind == TranscriptItemKind::Assistant
                        && item.text == "ENDPOINT_REPLY_OK"
                })
                && run.can_submit =>
            {
                let facts = IndicatorFacts::read(&run);
                if Self::facts(view) != Some(&facts) {
                    return;
                }
                self.check(view, 3);
                assert!(
                    facts.run_error.is_none(),
                    "a new successful turn must clear prior failure"
                );
                assert!(
                    run.transcript
                        .items
                        .iter()
                        .any(|item| item.is_final && item.text == "ENDPOINT_REPLY_OK")
                );
                self.epoch = run.connection_epoch;
                self.post(8, vec![Self::click(view, RECONNECT)]);
            }
            8 if run.connection_epoch > self.epoch
                && !run.transcript.is_stale
                && run.can_submit =>
            {
                self.check(view, 3);
                assert_eq!(Self::status(&run), Some("completed"));
                assert!(IndicatorFacts::read(&run).run_error.is_none());
                self.post(9, vec![Action::Close]);
            }
            _ => {}
        }
        window.set_animating(false);
    }
    pub fn receipt(&self, view: &ComponentDemo) -> serde_json::Value {
        assert_eq!(self.phase, 9);
        self.check(view, 3);
        serde_json::json!({"nativeWindow":true,"nativePointer":true,"nativeTextServices":true,"activePreviewBeforeCancel":true,"normalCancelCallback":true,"authoritativeAbortedTurn":true,"cancelReconnectNoReplay":true,"providerFailureInIndicators":true,"providerFailureInTranscript":true,"nativeFailureTextQuery":true,"reportedError":self.reported_error,"explicitRecovery":true,"priorFailureCleared":true,"oneSubmissionPerSend":true,"draftIsolation":true,"recoveryReconnectNoReplay":true,"stateDrivenCallbacks":true,"syntheticPreedit":false,"physicalIme":false,"voiceOver":false,"readingAnchorAcceptance":false})
    }
}
