//! Owned protocol history + original disclosure controls and SDK reflow.
use super::*;
use crate::acceptance_script::{Action, run_script};
use tekes_agent_components_ui::{
    OutputContentIds, OutputContentKind, OutputMessageFragment, RunPort, ToolActivity,
    TranscriptActivitySegmentEnd, TranscriptContentRowKey,
};
use tekes_ui_controls::composition::{ButtonContent, ScrollContent, TextFieldContent};
use tekes_ui_controls::{ScrollRevealOptions, ScrollRevealPlacement, extended::Axis};
pub(super) struct Probe {
    phase: u8,
    activity: Option<ComponentId>,
    opening_input: Option<ComponentId>,
    segmented_history: bool,
    prompt_history: bool,
    prompt_stage: u8,
    prompt: Option<OutputContentIds>,
    prompt_owner: usize,
    tool_text: Option<ComponentId>,
    tool_owner: usize,
    reason_owner: usize,
    started: Instant,
    tool: Option<OutputContentIds>,
    reason: Option<OutputContentIds>,
    tool_row: Option<ComponentId>,
    target: Option<ComponentId>,
    y: f32,
    baseline: Option<tekes_ui_controls::TextEditBaseline>,
    editor: usize,
    resized_at: Option<Instant>,
}
impl Probe {
    pub fn new(prompt_history: bool) -> Self {
        run_script(vec![(25000, Action::Close)]);
        Self {
            phase: 0,
            activity: None,
            opening_input: None,
            segmented_history: false,
            prompt_history,
            prompt_stage: 0,
            prompt: None,
            prompt_owner: 0,
            tool_text: None,
            tool_owner: 0,
            reason_owner: 0,
            started: Instant::now(),
            tool: None,
            reason: None,
            tool_row: None,
            target: None,
            y: 0.,
            baseline: None,
            editor: 0,
            resized_at: None,
        }
    }
    fn run(host: &Host) -> &RunPort {
        host.view
            .tree
            .get(ComponentId(host.view.ids().root.0 + 16))
            .unwrap()
            .component()
            .unwrap()
    }
    fn viewport(host: &Host) -> &ScrollContent {
        host.view
            .tree
            .get(Self::run(host).ids().viewport)
            .unwrap()
            .component()
            .unwrap()
    }
    fn reveal(host: &mut Host, target: ComponentId) {
        let viewport = Self::run(host).ids().viewport;
        let version = Self::viewport(host)
            .reading_document_version()
            .unwrap()
            .clone();
        host.view
            .tree
            .reveal_scroll_reading_item(
                viewport,
                &version,
                target,
                ScrollRevealOptions {
                    placement: ScrollRevealPlacement::Start(Axis::Vertical),
                    animated: false,
                    ..Default::default()
                },
                &mut host.text,
            )
            .unwrap();
    }
    fn click(host: &Host, id: ComponentId) -> Action {
        let node = host.view.tree.get(id).unwrap();
        let point = node
            .visible()
            .expect("original native control must be visible")
            .center();
        Action::Click {
            x: point.x as f64,
            y: point.y as f64,
        }
    }
    fn y(&self, host: &Host) -> f32 {
        host.view
            .tree
            .get(self.target.unwrap())
            .unwrap()
            .frame()
            .min_y()
            - host
                .view
                .tree
                .get(Self::run(host).ids().viewport)
                .unwrap()
                .visible()
                .unwrap()
                .min_y()
    }
    fn unchanged(&self, host: &Host) {
        if self.prompt_history {
            assert_eq!(self.prompt_stage, 3);
            assert_eq!(
                host.view
                    .tree
                    .get(self.prompt.unwrap().body)
                    .unwrap()
                    .component::<TextFieldContent>()
                    .unwrap() as *const _ as usize,
                self.prompt_owner
            );
        }
        use tekes_ui_controls::composition::ContentReadingAnchorUpdate;
        match Self::viewport(host).reading_result().unwrap().anchor {
            ContentReadingAnchorUpdate::Preserved(id)
            | ContentReadingAnchorUpdate::Fallback(Some(id)) => assert_eq!(Some(id), self.target),
            ref other => panic!("missing original reflow anchor: {other:?}"),
        }
        assert!(
            (self.y(host) - self.y).abs() <= 1.,
            "resize/type-scale moved reading target"
        );
        assert!(!Self::viewport(host).is_following_latest());
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
    pub fn advance(&mut self, host: &mut Host, window: &Window) {
        assert!(
            self.started.elapsed() < Duration::from_secs(25),
            "disclosure/reflow deadline phase {}",
            self.phase
        );
        let old = self.phase;
        if self.prompt_history && self.prompt_stage < 3 {
            if !window.is_active() {
                return;
            }
            match self.prompt_stage {
                0 => {
                    let run = Self::run(host);
                    let Some(content) = run.transcript() else {
                        return;
                    };
                    let snapshot = run.displayed_snapshot().unwrap();
                    let doc =
                        tekes_agent_components_ui::TranscriptDocument::read(&snapshot.transcript)
                            .unwrap();
                    let section = doc.sections.iter().rev().nth(3).unwrap();
                    let prompt = section.outputs.iter().find(|row| row.item.system_prompt().is_some_and(|source| source.placement == tekes_agent_components_ui::TranscriptSystemPromptPlacement::Initial)).unwrap();
                    let prompt_key = TranscriptContentRowKey::Message(prompt.key.into());
                    let input_key = TranscriptContentRowKey::Message(section.inputs[0].key.into());
                    let position = |key| {
                        content
                            .row_keys()
                            .iter()
                            .position(|row| row == key)
                            .unwrap()
                    };
                    assert!(position(&prompt_key) < position(&input_key));
                    let row = content.row_id(&prompt_key).unwrap();
                    let part = content
                        .message_content_ids(&prompt.key.into())
                        .into_iter()
                        .find(|part| part.kind == OutputContentKind::SystemPrompt)
                        .unwrap();
                    assert!(host.view.tree.get(part.body).is_none());
                    self.prompt = Some(part);
                    Self::reveal(host, row);
                    self.prompt_stage = 1;
                }
                1 => {
                    let button = self.prompt.unwrap().disclosure.unwrap();
                    if host.view.tree.get(button).unwrap().visible().is_none() {
                        window.request_redraw();
                        return;
                    }
                    run_script(vec![(0, Self::click(host, button))]);
                    self.prompt_stage = 2;
                }
                2 => {
                    let Some(node) = host.view.tree.get(self.prompt.unwrap().body) else {
                        return;
                    };
                    self.prompt_owner =
                        node.component::<TextFieldContent>().unwrap() as *const _ as usize;
                    self.prompt_stage = 3;
                }
                _ => unreachable!(),
            }
            window.request_redraw();
            return;
        }
        match self.phase {
            0 if window.is_active() => {
                let run = Self::run(host);
                let Some(content) = run.transcript() else {
                    return;
                };
                let Some((tool_row, tool)) = content.row_keys().iter().find_map(|key| {
                    if !matches!(key, TranscriptContentRowKey::Tool(_)) {
                        return None;
                    }
                    let id = content.row_id(key).unwrap();
                    let TranscriptContentRowKey::Tool(tool_id) = key else {
                        return None;
                    };
                    let member = content.tool_activity_id(tool_id)?;
                    host.view
                        .tree
                        .get(member)
                        .unwrap()
                        .component::<ToolActivity>()
                        .unwrap()
                        .content_ids()
                        .map(|tool| (id, tool))
                }) else {
                    let Some(key)=content.row_keys().iter().rev().filter(|key| matches!(key,TranscriptContentRowKey::ActivityDisclosure(_)) || matches!(key,TranscriptContentRowKey::ActivitySegment(segment) if matches!(segment.end,TranscriptActivitySegmentEnd::BeforeInput(_)))).nth(3).cloned() else {return;};
                    let button = content.row_id(&key).unwrap();
                    let turn = match &key {
                        TranscriptContentRowKey::ActivityDisclosure(turn) => turn.as_str(),
                        TranscriptContentRowKey::ActivitySegment(segment) => {
                            self.segmented_history = true;
                            segment.turn_id.as_str()
                        }
                        _ => unreachable!(),
                    };
                    let snapshot = run.displayed_snapshot().unwrap();
                    let doc =
                        tekes_agent_components_ui::TranscriptDocument::read(&snapshot.transcript)
                            .unwrap();
                    let input = doc
                        .sections
                        .iter()
                        .find(|section| section.id == turn)
                        .unwrap()
                        .inputs
                        .first()
                        .unwrap();
                    self.opening_input = content.message_id(input.key);
                    assert!(
                        !host
                            .view
                            .tree
                            .get(button)
                            .unwrap()
                            .component::<ButtonContent>()
                            .unwrap()
                            .button
                            .state
                            .selected,
                        "normal history starts collapsed"
                    );
                    self.activity = Some(button);
                    Self::reveal(host, button);
                    self.phase = 100;
                    window.request_redraw();
                    return;
                };
                let (reason_row, reason) = content
                    .row_keys()
                    .iter()
                    .find_map(|key| {
                        let id = content.row_id(key).unwrap();
                        host.view
                            .tree
                            .get(id)
                            .unwrap()
                            .component::<OutputMessageFragment>()?
                            .content_ids()
                            .into_iter()
                            .find(|part| part.kind == OutputContentKind::Reasoning)
                            .map(|part| (id, part))
                    })
                    .expect("owned normalized reasoning block");
                let tool_member = content
                    .row_keys()
                    .iter()
                    .find_map(|key| {
                        let TranscriptContentRowKey::Tool(id) = key else {
                            return None;
                        };
                        content.tool_activity_id(id)
                    })
                    .unwrap();
                self.tool_text = Some(
                    host.view
                        .tree
                        .get(tool_member)
                        .unwrap()
                        .component::<ToolActivity>()
                        .unwrap()
                        .tool_detail_sections()[1]
                        .body,
                );
                self.tool = Some(tool);
                self.reason = Some(reason);
                self.tool_row = Some(tool_row);
                Self::reveal(host, reason_row);
                self.phase = 1;
            }
            100 if window.is_active() => {
                run_script(vec![(0, Self::click(host, self.activity.unwrap()))]);
                self.phase = 101;
            }
            101 if Self::run(host)
                .transcript()
                .unwrap()
                .row_keys()
                .iter()
                .any(|key| matches!(key, TranscriptContentRowKey::Tool(_))) =>
            {
                assert!(
                    host.view
                        .tree
                        .get(self.activity.unwrap())
                        .unwrap()
                        .component::<ButtonContent>()
                        .unwrap()
                        .button
                        .state
                        .selected
                );
                self.phase = 0;
            }
            1 => {
                run_script(vec![(
                    0,
                    Self::click(host, self.reason.as_ref().unwrap().disclosure.unwrap()),
                )]);
                self.phase = 2;
            }
            2 if host
                .view
                .tree
                .get(self.reason.as_ref().unwrap().body)
                .is_some() =>
            {
                Self::reveal(host, self.tool_row.unwrap());
                self.phase = 3;
            }
            3 => {
                run_script(vec![(
                    0,
                    Self::click(host, self.tool.as_ref().unwrap().disclosure.unwrap()),
                )]);
                self.phase = 4;
            }
            4 if host
                .view
                .tree
                .get(self.tool.as_ref().unwrap().body)
                .is_some() =>
            {
                let target = self
                    .opening_input
                    .expect("original opening input for selected history");
                self.target = Some(target);
                Self::reveal(host, target);
                self.phase = 5;
            }
            5 => {
                self.y = self.y(host);
                run_script(vec![
                    (0, Self::click(host, host.view.ids().editor)),
                    (
                        40,
                        Action::PreeditRange {
                            text: "窗口重排期间中文组字".into(),
                            selection: 2..5,
                            replacement: None,
                        },
                    ),
                ]);
                self.phase = 6;
            }
            6 if window.is_active() && Self::run(host).composer().is_composing() => {
                self.baseline = Some(host.view.editor().input.external_baseline());
                self.editor = host.view.editor() as *const _ as usize;
                self.tool_owner = host
                    .view
                    .tree
                    .get(self.tool_text.unwrap())
                    .unwrap()
                    .component::<TextFieldContent>()
                    .unwrap() as *const _ as usize;
                self.reason_owner = host
                    .view
                    .tree
                    .get(self.reason.as_ref().unwrap().body)
                    .unwrap()
                    .component::<TextFieldContent>()
                    .unwrap() as *const _ as usize;
                self.unchanged(host);
                self.resized_at = Some(Instant::now());
                run_script(vec![
                    (0, Action::BeginLiveResize),
                    (20, Action::Resize { w: 1000., h: 740. }),
                    (80, Action::EndLiveResize),
                    (200, Action::WakeRedraw),
                ]);
                self.phase = 7;
            }
            7 if (window.size().width - 1000.).abs() < 1.
                && self
                    .resized_at
                    .is_some_and(|time| time.elapsed() >= Duration::from_millis(150)) =>
            {
                self.unchanged(host);
                self.resized_at = Some(Instant::now());
                run_script(vec![
                    (0, Action::BeginLiveResize),
                    (20, Action::Resize { w: 1400., h: 860. }),
                    (80, Action::EndLiveResize),
                    (200, Action::WakeRedraw),
                ]);
                self.phase = 8;
            }
            8 if (window.size().width - 1400.).abs() < 1.
                && self
                    .resized_at
                    .is_some_and(|time| time.elapsed() >= Duration::from_millis(150)) =>
            {
                self.unchanged(host);
                host.text.type_scale = 1.4;
                self.phase = 9;
            }
            9 => {
                self.unchanged(host);
                host.text.type_scale = 1.;
                self.phase = 10;
            }
            10 => {
                self.unchanged(host);
                assert!(
                    host.view
                        .tree
                        .get(self.tool.as_ref().unwrap().body)
                        .is_some()
                );
                assert!(
                    host.view
                        .tree
                        .get(self.reason.as_ref().unwrap().body)
                        .is_some()
                );
                run_script(vec![(0, Self::click(host, self.activity.unwrap()))]);
                self.phase = 11;
            }
            11 if host.view.tree.get(self.tool_text.unwrap()).is_none()
                && host
                    .view
                    .tree
                    .get(self.reason.as_ref().unwrap().body)
                    .is_none() =>
            {
                self.unchanged(host);
                assert!(
                    !host
                        .view
                        .tree
                        .get(self.activity.unwrap())
                        .unwrap()
                        .component::<ButtonContent>()
                        .unwrap()
                        .button
                        .state
                        .selected
                );
                run_script(vec![(0, Self::click(host, self.activity.unwrap()))]);
                self.phase = 12;
            }
            12 if host.view.tree.get(self.tool_text.unwrap()).is_some()
                && host
                    .view
                    .tree
                    .get(self.reason.as_ref().unwrap().body)
                    .is_some() =>
            {
                self.unchanged(host);
                assert_eq!(
                    host.view
                        .tree
                        .get(self.tool_text.unwrap())
                        .unwrap()
                        .component::<TextFieldContent>()
                        .unwrap() as *const _ as usize,
                    self.tool_owner
                );
                assert_eq!(
                    host.view
                        .tree
                        .get(self.reason.as_ref().unwrap().body)
                        .unwrap()
                        .component::<TextFieldContent>()
                        .unwrap() as *const _ as usize,
                    self.reason_owner
                );
                run_script(vec![(0, Action::Unmark), (100, Action::Close)]);
                self.phase = 13;
            }
            _ => {}
        }
        if old != self.phase {
            eprintln!("NATIVE_DISCLOSURE_READING_PHASE {}", self.phase);
            window.request_redraw();
        }
    }
    pub fn receipt(&self) -> serde_json::Value {
        assert_eq!(self.phase, 13);
        serde_json::json!({"nativeWindow":true,"ownedProtocolHistory":true,"originalToolAndReasoningClicks":true,"normalActivityInitiallyCollapsed":true,"inputBoundedHistory":self.segmented_history,"initialPromptBeforeInputAndOriginalDisclosure":self.prompt_history && self.prompt_stage == 3,"activityExpandCollapseReopenClicks":true,"originalToolAndReasoningTextOwnersRetained":true,"expandedDetailsRetained":true,"liveResize":true,"typeScaleReflow":true,"readingAnchorPreserved":true,"originalEditorFocusAndChineseMarked":true,"noSubmission":true,"realHarness":false,"physicalIme":false,"physicalResizeDrag":false,"voiceOver":false})
    }
}
