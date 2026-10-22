//! Native test host only. Advances on actual widget/committed state, never a
//! guessed network delay; every mutation still enters normal UI callbacks.
use super::*;
use tekes_agent_components_ui::{
    TranscriptContent, TranscriptContentRowKey, TurnFeedback, TurnFootnote,
};
use tekes_agent_presentation::{RunSnapshot, TranscriptItemKind};
use tekes_ui_controls::composition::{ComponentId, ContentHost};
use tekes_ui_platform_macos::selftest::{Action, run_script};

// Editor tickets can change when a new controlled draft publication is
// acknowledged. Observe semantic state, not equality of an old authority token.
#[derive(Debug, PartialEq, Eq)]
struct ComposerObservation {
    text: String,
    anchor: usize,
    caret: usize,
    marked: Option<std::ops::Range<usize>>,
    multiline: bool,
}
impl ComposerObservation {
    fn read(view: &ComponentDemo) -> Self {
        let input = &view.editor().input;
        Self {
            text: input.text.clone(),
            anchor: input.anchor,
            caret: input.caret,
            marked: input.marked.clone(),
            multiline: input.multiline,
        }
    }
}
pub(super) struct Probe {
    started: Instant,
    phase: u8,
    deadline_scheduled: bool,
    last_log: Instant,
    control: Option<ComponentId>,
    before: Vec<Arc<RunSnapshot>>,
    deleted_publication: u64,
    auxiliary_focus: Option<ComponentId>,
    composer_state: Option<ComposerObservation>,
    capture_directory: Option<std::path::PathBuf>,
    composer_owner: usize,
}
const NOTE: &str = "原生中文评价备注";
impl Probe {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            phase: 0,
            deadline_scheduled: false,
            last_log: Instant::now(),
            control: None,
            before: vec![],
            deleted_publication: 0,
            auxiliary_focus: None,
            composer_state: None,
            capture_directory: std::env::var_os("AGENT_FEEDBACK_CAPTURE_DIRECTORY").map(|path| {
                let path = std::path::PathBuf::from(path);
                assert!(
                    path.is_absolute() && path.is_dir(),
                    "use an existing absolute evidence directory"
                );
                path
            }),
            composer_owner: 0,
        }
    }
    fn control<'a>(&self, view: &'a ComponentDemo) -> Option<&'a TurnFeedback> {
        view.tree.get(self.control?)?.component::<TurnFeedback>()
    }
    fn ready(&self, view: &ComponentDemo) -> bool {
        self.control(view).is_some_and(|control| {
            !control.is_pending() && !control.needs_refresh() && control.state().is_some()
        })
    }
    fn check_aside_band(&self, view: &ComponentDemo) {
        let control = self.control(view).unwrap();
        assert!(!control.is_open());
        let content = view
            .tree
            .get(view.transcript_content_id(view.ids()).unwrap())
            .unwrap()
            .component::<TranscriptContent>()
            .unwrap();
        let mut inputs = 0;
        for key in content.row_keys() {
            let node = view.tree.get(content.row_id(key).unwrap()).unwrap();
            if let Some(input) = node.component::<tekes_agent_components_ui::InputMessage>() {
                let aside = view.tree.get(input.aside_id()).unwrap().frame();
                let info = view.tree.get(input.ids().footnote).unwrap().frame();
                if let Some(copy) = input.copy_button_id() {
                    let copy = view.tree.get(copy).unwrap().frame();
                    assert!((copy.max_x() - aside.max_x()).abs() < 0.5);
                    assert!(info.max_x() <= copy.min_x());
                    assert!((info.center().y - copy.center().y).abs() < 0.5);
                    inputs += 1;
                } else {
                    assert!((info.max_x() - aside.max_x()).abs() < 0.5);
                }
            }
        }
        assert!(inputs > 0, "owned source has a copyable original input");
        let key = TranscriptContentRowKey::TurnFootnote(control.target().unwrap().turn_id().into());
        let footer = view.tree.get(content.row_id(&key).unwrap()).unwrap();
        let footnote = footer.component::<TurnFootnote>().unwrap();
        assert_eq!(footnote.feedback().unwrap().ids().root, control.ids().root);
        assert!(
            !content
                .row_keys()
                .iter()
                .any(|key| matches!(key, TranscriptContentRowKey::TurnFeedback(_)))
        );
        let ids = control.ids();
        let mut band = vec![ids.positive, ids.negative];
        if control.state().unwrap().current.is_some() {
            band.push(ids.edit_note);
        }
        if let Some(copy) = footnote.copy_button_id() {
            band.insert(0, copy);
        }
        if let Some(info) = footnote.information_id() {
            band.push(info);
        }
        let rects: Vec<_> = band
            .iter()
            .map(|id| view.tree.get(*id).unwrap().frame())
            .collect();
        for pair in rects.windows(2) {
            assert!(pair[1].min_x() >= pair[0].max_x());
            assert!((pair[0].center().y - pair[1].center().y).abs() < 0.5);
        }
        assert!((rects[0].min_x() - footer.frame().min_x()).abs() < 0.5);
    }
    fn click(tree: &ContentHost, id: ComponentId) -> Action {
        let node = tree.get(id).expect("native feedback control is mounted");
        let point = node.frame().center();
        assert!(
            node.visible().is_some_and(|rect| rect.contains(point)),
            "native feedback phase target is clipped: {id:?}"
        );
        Action::Click {
            x: point.x as f64,
            y: point.y as f64,
        }
    }
    fn check_drafts_and_messages(&self, view: &ComponentDemo) {
        assert_eq!(
            view.editor().input.text,
            self.before[view.selected()].draft.text,
            "mounted Composer must display its own destination draft"
        );
        for (binding, before) in view.bindings().iter().zip(&self.before) {
            let after = binding.current_run().unwrap();
            assert_eq!(
                after.draft, before.draft,
                "feedback must not modify a Composer draft"
            );
            let users = |run: &RunSnapshot| {
                run.transcript
                    .items
                    .iter()
                    .filter(|item| item.kind == TranscriptItemKind::User)
                    .map(|item| item.id.clone())
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                users(&after),
                users(before),
                "native feedback/IME must not submit a prompt"
            );
        }
    }
    fn next(&mut self, phase: u8, steps: Vec<(u64, Action)>) {
        self.phase = phase;
        eprintln!(
            "NATIVE_FEEDBACK_PHASE {phase} elapsed_ms={}",
            self.started.elapsed().as_millis()
        );
        // advance() already waits for the actual native/committed precondition.
        // Post local actions in FIFO order without thread-sleep scheduling; the
        // public text bridge completes each action synchronously on the main queue.
        run_script(steps);
    }
    pub fn advance(&mut self, host: &Host, window: &Window) {
        // Native events and the original runtime callback wake each next frame.
        // A probe must not flood the main run loop with continuous draw requests.
        window.set_animating(false);
        if !self.deadline_scheduled {
            self.deadline_scheduled = true;
            let remaining = Duration::from_secs(45).saturating_sub(self.started.elapsed());
            run_script(vec![(remaining.as_millis() as u64, Action::WakeRedraw)]);
        }

        if self.phase == 0 || self.last_log.elapsed() > Duration::from_secs(3) {
            eprintln!(
                "NATIVE_FEEDBACK_WAIT phase={} elapsed_ms={} active={} selected={} mounted={} open={} pending={} composing={}",
                self.phase,
                self.started.elapsed().as_millis(),
                window.is_active(),
                host.view.selected(),
                self.control(&host.view).is_some(),
                self.control(&host.view).is_some_and(|c| c.is_open()),
                self.control(&host.view).is_some_and(|c| c.is_pending()),
                host.view.composing()
            );
            self.last_log = Instant::now();
        }

        assert!(
            self.started.elapsed() < Duration::from_secs(45),
            "native feedback timed out at phase {}",
            self.phase
        );
        let view = &host.view;
        match self.phase {
            0 if window.is_active() => {
                let Some(root) = view.transcript_content_id(view.ids()) else {
                    return;
                };
                let content = view
                    .tree
                    .get(root)
                    .unwrap()
                    .component::<TranscriptContent>()
                    .unwrap();
                let Some((id, control)) = content.feedback_controls().find(|(_, control)| {
                    view.tree.get(control.ids().refresh).is_some_and(|node| {
                        node.visible()
                            .is_some_and(|rect| rect.contains(node.frame().center()))
                    })
                }) else {
                    return;
                };
                self.control = Some(id);
                self.before = view
                    .bindings()
                    .iter()
                    .map(|binding| binding.current_run().unwrap())
                    .collect();
                assert!(self.before.len() >= 2, "two owned sessions required");
                assert!(
                    self.before.iter().all(|run| !run.draft.text.is_empty()),
                    "nonempty isolated drafts are required to verify the send barrier"
                );
                let refresh = control.ids().refresh;
                self.auxiliary_focus = view.tree.focused();
                assert_eq!(self.auxiliary_focus, Some(view.ids().editor));
                self.composer_state = Some(ComposerObservation::read(view));
                self.composer_owner = view.editor() as *const _ as usize;
                self.next(1, vec![(0, Self::click(&view.tree, refresh))]);
            }
            1 if self.ready(view) => {
                self.check_aside_band(view);
                let control = self.control(view).unwrap();
                assert!(
                    control.state().unwrap().current.is_none(),
                    "native feedback probe requires an unrated owned target"
                );
                assert!(view.tree.get(control.ids().edit_note).is_none());
                assert_eq!(view.tree.focused(), self.auxiliary_focus);
                assert_eq!(Some(ComposerObservation::read(view)), self.composer_state);
                self.next(
                    2,
                    vec![(0, Self::click(&view.tree, control.ids().positive))],
                );
            }
            2 if self.ready(view)
                && self
                    .control(view)
                    .unwrap()
                    .state()
                    .unwrap()
                    .current
                    .is_some() =>
            {
                let control = self.control(view).unwrap();
                assert_eq!(view.tree.focused(), self.auxiliary_focus);
                assert_eq!(view.editor() as *const _ as usize, self.composer_owner);
                assert_eq!(Some(ComposerObservation::read(view)), self.composer_state);
                assert_eq!(
                    control.state().unwrap().current.as_ref().unwrap().rating,
                    tekes_agent_contract::SessionMessageFeedbackRating::Positive
                );
                self.next(
                    3,
                    vec![(0, Self::click(&view.tree, control.ids().edit_note))],
                );
            }
            3 if self.control(view).is_some_and(|control| control.is_open()) => {
                assert!(
                    !view
                        .tree
                        .get(view.ids().send)
                        .unwrap()
                        .component::<tekes_ui_controls::composition::ButtonContent>()
                        .unwrap()
                        .button
                        .state
                        .disabled,
                    "send must be available before note composition"
                );
                let ids = self.control(view).unwrap().ids();
                self.next(
                    4,
                    vec![
                        (0, Self::click(&view.tree, ids.editor)),
                        (0, Action::Preedit("中文组字".into())),
                        (0, Self::click(&view.tree, ids.save)),
                        (0, Self::click(&view.tree, ids.close)),
                        (0, Self::click(&view.tree, view.ids().send)),
                        (
                            0,
                            Self::click(&view.tree, tekes_agent_ui_demo::component_demo::SELECT_B),
                        ),
                        (
                            0,
                            Action::ExpectText {
                                text: "中文组字".into(),
                                marked: true,
                            },
                        ),
                        (0, Action::Text(NOTE.into())),
                    ],
                );
            }
            4 if !view.composing()
                && self
                    .control(view)
                    .and_then(|control| control.editor())
                    .is_some_and(|editor| editor.input.text == NOTE) =>
            {
                assert_eq!(view.selected(), 0);
                self.check_drafts_and_messages(view);
                let control = self.control(view).unwrap();
                assert!(!control.is_pending());
                assert!(
                    control
                        .state()
                        .unwrap()
                        .current
                        .as_ref()
                        .unwrap()
                        .note
                        .is_none()
                );
                self.next(5, vec![(0, Self::click(&view.tree, control.ids().save))]);
            }
            5 if self.ready(view)
                && !self.control(view).unwrap().is_open()
                && self
                    .control(view)
                    .unwrap()
                    .state()
                    .unwrap()
                    .current
                    .as_ref()
                    .and_then(|record| record.note.as_deref())
                    == Some(NOTE) =>
            {
                let note = self.control(view).unwrap().ids().edit_note;
                self.check_aside_band(view);
                assert_eq!(
                    view.tree
                        .get(note)
                        .unwrap()
                        .component::<tekes_ui_controls::composition::ButtonContent>()
                        .unwrap()
                        .label,
                    NOTE
                );
                let mut actions = vec![];
                if let Some(directory) = &self.capture_directory {
                    let id: String = view
                        .binding()
                        .scope()
                        .endpoint_id
                        .chars()
                        .map(|c| {
                            if c.is_ascii_alphanumeric() || c == '-' {
                                c
                            } else {
                                '_'
                            }
                        })
                        .collect();
                    let path = directory.join(format!("{id}-feedback-note.png"));
                    assert!(!path.exists(), "do not overwrite acceptance screenshots");
                    actions.push((
                        0,
                        Action::Capture {
                            path: path.to_string_lossy().into_owned(),
                        },
                    ));
                }
                actions.push((
                    0,
                    Self::click(&view.tree, self.control(view).unwrap().ids().edit_note),
                ));
                self.next(6, actions);
            }
            6 if self.control(view).is_some_and(|control| control.is_open()) => {
                assert_eq!(
                    self.control(view).unwrap().editor().unwrap().input.text,
                    NOTE
                );
                self.next(
                    7,
                    vec![(
                        0,
                        Self::click(&view.tree, tekes_agent_ui_demo::component_demo::SELECT_B),
                    )],
                );
            }
            7 if view.selected() == 1 => {
                self.check_drafts_and_messages(view);
                self.next(
                    8,
                    vec![
                        (
                            0,
                            Action::ExpectText {
                                text: self.before[1].draft.text.clone(),
                                marked: false,
                            },
                        ),
                        (
                            0,
                            Self::click(&view.tree, tekes_agent_ui_demo::component_demo::SELECT_A),
                        ),
                    ],
                );
            }
            8 if view.selected() == 0
                && self.control(view).is_some_and(|control| control.is_open()) =>
            {
                assert_eq!(
                    self.control(view).unwrap().editor().unwrap().input.text,
                    NOTE
                );
                self.next(
                    9,
                    vec![
                        (
                            0,
                            Action::ExpectText {
                                text: self.before[0].draft.text.clone(),
                                marked: false,
                            },
                        ),
                        (
                            0,
                            Self::click(&view.tree, self.control(view).unwrap().ids().close),
                        ),
                    ],
                );
            }
            9 if self.ready(view) && !self.control(view).unwrap().is_open() => {
                self.next(
                    10,
                    vec![(
                        0,
                        Self::click(&view.tree, self.control(view).unwrap().ids().positive),
                    )],
                );
            }
            10 if self.ready(view)
                && self
                    .control(view)
                    .unwrap()
                    .state()
                    .unwrap()
                    .current
                    .is_none() =>
            {
                let control = self.control(view).unwrap();
                assert!(view.tree.get(control.ids().edit_note).is_none());
                self.deleted_publication = control.state().unwrap().publication;
                self.next(
                    11,
                    vec![(0, Self::click(&view.tree, control.ids().refresh))],
                );
            }
            11 if self.ready(view)
                && self.control(view).unwrap().state().unwrap().publication
                    > self.deleted_publication =>
            {
                assert!(
                    self.control(view)
                        .unwrap()
                        .state()
                        .unwrap()
                        .current
                        .is_none()
                );
                self.check_drafts_and_messages(view);
                self.next(12, vec![(0, Action::Close)]);
            }
            _ => {}
        }
        // The one deadline wake catches a missing callback; no polling/render loop.
        window.set_animating(false);
    }
    pub fn receipt(&self, view: &ComponentDemo) -> serde_json::Value {
        assert_eq!(self.phase, 12, "native feedback did not finish");
        self.check_drafts_and_messages(view);
        serde_json::json!({"nativeInteraction":true,"nativePointer":true,"nativeTextServices":true,"syntheticPreedit":true,"imeSendAndNavigationBlocked":true,"savedChineseNote":true,"noteReopened":true,"retainedAcrossSessionSwitch":true,"sameRatingDeletes":true,"freshReadAfterDelete":true,"draftsPreserved":true,"nonemptyComposerDrafts":true,"sendEnabledBeforeComposition":true,"noPromptSubmitted":true,"stateDrivenCallbacks":true,"auxiliaryFocusPreserved":true,"composerContentsAndOwnerPreserved":true,"opaqueBaselinePreserved":false,"unratedNoteHidden":true,"committedNoteLabel":true,"outputAsideSingleReadingBand":true,"inputAsideSingleTrailingBand":true,"physicalIme":false,"voiceOver":false,"readingAnchorAcceptance":false})
    }
}
