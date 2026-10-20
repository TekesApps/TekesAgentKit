//! Opt-in owned-session native host probe; no alternate renderer or state machine.
use super::*;
use tekes_agent_components_ui::{
    OutputContentKind, OutputMessage, TranscriptContent, TranscriptContentRowKey,
    TranscriptMessageIdentity,
};
use tekes_agent_presentation::{RunSnapshot, TranscriptDocument, TranscriptItemKind};
use tekes_ui_platform_macos::selftest::{Action, run_script};

pub(super) const INPUT: &str = "ENDPOINT_STREAM_NATIVE_PROOF";
const OTHER_DRAFT: &str = "另一会话保留草稿";
pub(super) struct Probe {
    started: Instant,
    phase: u8,
    deadline: bool,
    row: Option<ComponentId>,
    body: Option<ComponentId>,
    epoch: u64,
}
impl Probe {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            phase: 0,
            deadline: false,
            row: None,
            body: None,
            epoch: 0,
        }
    }
    pub fn prepare(view: &ComponentDemo) {
        assert_eq!(
            view.bindings().len(),
            2,
            "two fresh owned sessions required"
        );
        for (index, binding) in view.bindings().iter().enumerate() {
            let run = binding.current_run().unwrap();
            assert!(
                run.transcript.items.is_empty(),
                "stream probe requires blank owned sessions"
            );
            assert!(
                run.draft.text.is_empty(),
                "do not overwrite existing drafts"
            );
            binding
                .edit_text(
                    &tekes_agent_presentation::ComposerDestination::run(binding.scope().clone()),
                    if index == 0 { INPUT } else { OTHER_DRAFT }.into(),
                )
                .unwrap();
        }
    }
    fn post(&mut self, phase: u8, actions: Vec<Action>) {
        self.phase = phase;
        eprintln!(
            "NATIVE_STREAM_PHASE {phase} elapsed_ms={}",
            self.started.elapsed().as_millis()
        );
        run_script(actions.into_iter().map(|action| (0, action)).collect());
    }
    fn click(view: &ComponentDemo, id: ComponentId) -> Action {
        let node = view.tree.get(id).expect("stream target mounted");
        let point = node.frame().center();
        assert!(
            node.visible().is_some_and(|rect| rect.contains(point)),
            "stream target clipped {id:?}"
        );
        Action::Click {
            x: point.x as f64,
            y: point.y as f64,
        }
    }
    fn content(view: &ComponentDemo) -> &TranscriptContent {
        view.tree
            .get(view.transcript_content_id(view.ids()).unwrap())
            .unwrap()
            .component::<TranscriptContent>()
            .unwrap()
    }
    fn final_output(run: &RunSnapshot) -> Option<(TranscriptMessageIdentity, String)> {
        let document = TranscriptDocument::read(&run.transcript).unwrap();
        document
            .sections
            .iter()
            .find_map(|section| section.final_response())
            .map(|output| (output.key.into(), output.item.text.clone()))
    }
    fn check_final(&self, view: &ComponentDemo) {
        let run = view.binding().current_run().unwrap();
        let (key, text) = Self::final_output(&run).expect("completed source answer");
        assert_eq!(text, "ENDPOINT_REPLY_OK");
        assert_eq!(
            Self::content(view).row_id(&TranscriptContentRowKey::Message(key)),
            self.row,
            "canonical output must retain the live row"
        );
        let output = view
            .tree
            .get(self.row.unwrap())
            .unwrap()
            .component::<OutputMessage>()
            .unwrap();
        assert_eq!(
            output
                .content_ids()
                .iter()
                .find(|ids| ids.kind == OutputContentKind::Text)
                .unwrap()
                .body,
            self.body.unwrap(),
            "settlement must retain the native text owner"
        );
        assert_eq!(
            view.tree
                .text_surface(self.body.unwrap())
                .unwrap()
                .input
                .selection(),
            0..4,
            "stream settlement must retain selection"
        );
        let users: Vec<_> = run
            .transcript
            .items
            .iter()
            .filter(|item| {
                item.kind == TranscriptItemKind::User
                    && item.metadata["source"]["kind"]
                        .as_str()
                        .is_none_or(|kind| kind == "user")
            })
            .collect();
        eprintln!(
            "NATIVE_STREAM_INPUT_ROLES {:?}",
            run.transcript
                .items
                .iter()
                .filter(|item| item.kind == TranscriptItemKind::User)
                .map(|item| (
                    item.metadata["source"]["kind"].as_str(),
                    item.is_visible,
                    item.is_provisional
                ))
                .collect::<Vec<_>>()
        );
        assert_eq!(users.len(), 1, "one click must produce exactly one input");
        assert_eq!(users[0].text, INPUT);
        assert!(run.draft.text.is_empty());
        assert_eq!(
            view.bindings()[1].current_run().unwrap().draft.text,
            OTHER_DRAFT
        );
        assert!(
            !view.bindings()[1]
                .current_run()
                .unwrap()
                .transcript
                .items
                .iter()
                .any(|item| item.kind == TranscriptItemKind::User)
        );
    }
    fn query(&self) -> Action {
        Action::ExpectAXText {
            component: self.body.unwrap().0,
            text: "ENDPOINT_REPLY_OK".into(),
            selection: 0..4,
            selected: "ENDP".into(),
            query: 0..4,
            substring: Some("ENDP".into()),
        }
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
            "native stream timeout phase {}",
            self.phase
        );
        let view = &host.view;
        let run = view.binding().current_run().unwrap();
        match self.phase {
            0 if run.can_submit && view.editor().input.text == INPUT => {
                self.post(1, vec![Self::click(view, view.ids().send)]);
            }
            1 => {
                let doc = TranscriptDocument::read(&run.transcript).unwrap();
                let Some(output) = doc
                    .sections
                    .iter()
                    .flat_map(|section| &section.outputs)
                    .find(|output| {
                        output.item.is_provisional
                            && output.item.kind == TranscriptItemKind::Assistant
                            && output.item.text.starts_with("ENDPOINT_")
                    })
                else {
                    return;
                };
                let key: TranscriptMessageIdentity = output.key.into();
                let Some(row) = Self::content(view).row_id(&TranscriptContentRowKey::Message(key))
                else {
                    return;
                };
                let output = view
                    .tree
                    .get(row)
                    .unwrap()
                    .component::<OutputMessage>()
                    .unwrap();
                let Some(body) = output
                    .content_ids()
                    .iter()
                    .find(|ids| ids.kind == OutputContentKind::Text)
                    .map(|ids| ids.body)
                else {
                    return;
                };
                assert!(
                    view.tree
                        .text_surface(body)
                        .unwrap()
                        .input
                        .text
                        .starts_with("ENDPOINT_")
                );
                self.row = Some(row);
                self.body = Some(body);
                self.post(
                    2,
                    vec![
                        Self::click(view, body),
                        Action::AXSelect {
                            component: body.0,
                            range: 0..4,
                        },
                    ],
                );
            }
            2 if Self::final_output(&run).is_some_and(|(key, _)| {
                Self::content(view)
                    .row_id(&TranscriptContentRowKey::Message(key))
                    .is_some()
            }) =>
            {
                self.check_final(view);
                let body_node = view.tree.get(self.body.unwrap()).unwrap();
                eprintln!(
                    "NATIVE_STREAM_FINAL_BODY frame={:?} visible={:?} semantic={:?}",
                    body_node.frame(),
                    body_node.visible(),
                    view.tree
                        .semantics()
                        .into_iter()
                        .find(|node| node.id == self.body.unwrap())
                        .map(|node| node.visible)
                );
                assert_eq!(
                    view.tree.focused(),
                    self.body,
                    "stream publication must not steal text focus"
                );
                self.post(
                    3,
                    vec![
                        self.query(),
                        Action::ExpectText {
                            text: "ENDPOINT_REPLY_OK".into(),
                            marked: false,
                        },
                        Self::click(view, tekes_agent_ui_demo::component_demo::SELECT_B),
                    ],
                );
            }
            3 if view.selected() == 1 => {
                assert_eq!(view.editor().input.text, OTHER_DRAFT);
                self.post(
                    4,
                    vec![Self::click(
                        view,
                        tekes_agent_ui_demo::component_demo::SELECT_A,
                    )],
                );
            }
            4 if view.selected() == 0 => {
                self.check_final(view);
                self.epoch = run.connection_epoch;
                self.post(5, vec![self.query(), Self::click(view, RECONNECT)]);
            }
            5 if run.connection_epoch > self.epoch
                && !run.transcript.is_stale
                && Self::final_output(&run).is_some_and(|(key, _)| {
                    Self::content(view)
                        .row_id(&TranscriptContentRowKey::Message(key))
                        .is_some()
                }) =>
            {
                self.check_final(view);
                self.post(6, vec![self.query(), Action::Close]);
            }
            _ => {}
        }
        window.set_animating(false);
    }
    pub fn receipt(&self, view: &ComponentDemo) -> serde_json::Value {
        assert_eq!(self.phase, 6, "native stream probe incomplete");
        self.check_final(view);
        serde_json::json!({"nativeWindow":true,"nativePointer":true,"nativeTextServices":true,"nativeAxSelection":true,"activePreview":true,"sameOutputRow":true,"sameTextOwner":true,"selectionRetained":true,"focusRetainedDuringSettlement":true,"oneSubmission":true,"sessionDraftIsolation":true,"reconnectNoReplay":true,"stateDrivenCallbacks":true,"physicalIme":false,"voiceOver":false,"readingAnchorAcceptance":false})
    }
}
