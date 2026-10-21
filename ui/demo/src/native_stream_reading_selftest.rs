//! Real harness streaming while reading old content; no alternate UI/scroll state.
use super::*;
use tekes_agent_components_ui::RunView;
use tekes_agent_presentation::{RunSnapshot, TranscriptDocument, TranscriptItemKind};
use tekes_ui_controls::{
    TextEditBaseline,
    composition::{ContentReadingAnchorUpdate, ScrollContent},
};
use tekes_ui_platform_macos::selftest::{Action, run_script};
const INPUT: &str = "ENDPOINT_READING_STREAM_PROOF";
const DRAFT: &str = "流式期间中文草稿";
const OTHER: &str = "另一会话独立草稿";
#[derive(Debug, PartialEq, Eq)]
struct SendObservation {
    active: bool,
    can_cancel: bool,
    submissions: usize,
    output: Option<(usize, bool)>,
    editor_bytes: usize,
}
pub(super) struct Probe {
    phase: u8,
    started: Instant,
    anchor: Option<ComponentId>,
    y: f32,
    baseline: Option<TextEditBaseline>,
    editor: usize,
    last_len: usize,
    updates: usize,
    epoch: u64,
    observed: Option<SendObservation>,
}
impl Probe {
    pub fn new() -> Self {
        run_script(vec![(40_000, Action::Close)]);
        Self {
            phase: 0,
            started: Instant::now(),
            anchor: None,
            y: 0.,
            baseline: None,
            editor: 0,
            last_len: 0,
            updates: 0,
            epoch: 0,
            observed: None,
        }
    }
    pub fn prepare(view: &ComponentDemo) {
        assert_eq!(view.bindings().len(), 2);
        for (index, binding) in view.bindings().iter().enumerate() {
            let snapshot = binding.current_run().unwrap();
            assert!(snapshot.draft.text.is_empty(), "owned empty drafts only");
            if index == 0 {
                assert!(
                    snapshot.transcript.turns.len() >= 10,
                    "seed enough history to read above the active turn"
                );
            }
            binding
                .edit_text(
                    &tekes_agent_components_ui::ComposerDestination::run(binding.scope().clone()),
                    if index == 0 { INPUT } else { OTHER }.into(),
                )
                .unwrap();
        }
    }
    fn run(host: &Host) -> &RunView {
        host.view
            .tree
            .get(ComponentId(host.view.ids().root.0 + 16))
            .unwrap()
            .component()
            .unwrap()
    }
    fn snapshot(host: &Host) -> &RunSnapshot {
        Self::run(host).displayed_snapshot().unwrap()
    }
    fn scroll(host: &Host) -> &ScrollContent {
        host.view
            .tree
            .get(Self::run(host).ids().viewport)
            .unwrap()
            .component()
            .unwrap()
    }
    fn output(host: &Host) -> Option<(String, bool)> {
        let doc = TranscriptDocument::read(&Self::snapshot(host).transcript).unwrap();
        doc.sections
            .iter()
            .flat_map(|s| &s.outputs)
            .find(|o| o.item.text.starts_with("READING_STREAM_BEGIN"))
            .map(|o| (o.item.text.clone(), o.item.is_provisional))
    }
    fn anchor(host: &Host) -> ComponentId {
        match Self::scroll(host).reading_result().unwrap().anchor {
            ContentReadingAnchorUpdate::Preserved(id)
            | ContentReadingAnchorUpdate::Fallback(Some(id)) => id,
            ref other => panic!("stream reading anchor {other:?}"),
        }
    }
    fn y(host: &Host, item: ComponentId) -> f32 {
        host.view.tree.get(item).unwrap().frame().min_y()
            - host
                .view
                .tree
                .get(Self::run(host).ids().viewport)
                .unwrap()
                .visible()
                .unwrap()
                .min_y()
    }
    fn reading(&self, host: &Host) {
        assert_eq!(
            Self::anchor(host),
            self.anchor.unwrap(),
            "stream must keep the original old item"
        );
        assert!(
            (Self::y(host, self.anchor.unwrap()) - self.y).abs() <= 1.,
            "stream moved the old item-local reading position"
        );
        assert!(!Self::scroll(host).is_following_latest());
        assert_eq!(host.view.editor() as *const _ as usize, self.editor);
    }
    fn once(host: &Host) {
        assert_eq!(
            Self::snapshot(host)
                .transcript
                .items
                .iter()
                .filter(|i| i.kind == TranscriptItemKind::User && i.text == INPUT)
                .count(),
            1,
            "one explicit send, including after reconnect"
        );
        assert_eq!(
            host.view.bindings()[1].current_run().unwrap().draft.text,
            OTHER
        );
    }
    fn click(host: &Host, id: ComponentId) -> Action {
        let node = host.view.tree.get(id).unwrap();
        let point = node.frame().center();
        assert!(node.visible().is_some_and(|r| r.contains(point)));
        Action::Click {
            x: point.x as f64,
            y: point.y as f64,
        }
    }
    pub fn advance(&mut self, host: &mut Host, window: &Window) {
        if self.phase == 2 {
            let snapshot = Self::snapshot(host);
            let state = SendObservation {
                active: window.is_active(),
                can_cancel: snapshot.run.as_ref().is_some_and(|run| run.can_cancel),
                submissions: snapshot
                    .transcript
                    .items
                    .iter()
                    .filter(|i| i.kind == TranscriptItemKind::User && i.text == INPUT)
                    .count(),
                output: Self::output(host).map(|(text, preview)| (text.len(), preview)),
                editor_bytes: host.view.editor().input.text.len(),
            };
            if self.observed.as_ref() != Some(&state) {
                eprintln!("NATIVE_STREAM_READING_WAIT {state:?}");
                self.observed = Some(state);
            }
        }
        assert!(
            self.started.elapsed() < Duration::from_secs(40),
            "stream reading deadline {}",
            self.phase
        );
        if !window.is_active() {
            return;
        }
        let previous = self.phase;
        match self.phase {
            0 if host.view.editor().input.text == INPUT => {
                let viewport = Self::run(host).ids().viewport;
                host.view
                    .tree
                    .get_mut(viewport)
                    .unwrap()
                    .component_mut::<ScrollContent>()
                    .unwrap()
                    .follow_latest();
                self.editor = host.view.editor() as *const _ as usize;
                self.phase = 1;
            }
            1 => {
                run_script(vec![(0, Self::click(host, host.view.ids().send))]);
                self.phase = 2;
            }
            2 if Self::snapshot(host)
                .run
                .as_ref()
                .is_some_and(|run| run.can_cancel)
                && Self::output(host).is_some_and(|(_, preview)| preview) =>
            {
                let viewport = Self::run(host).ids().viewport;
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
                    delta: tekes_ui_scene::Point::new(0., -600.),
                    precise: true,
                    begin: true,
                    end: true,
                    momentum: false,
                    time: 1.,
                });
                self.phase = 3;
            }
            3 => {
                self.anchor = Some(Self::anchor(host));
                self.y = Self::y(host, self.anchor.unwrap());
                let (text, preview) = Self::output(host).unwrap();
                assert!(
                    preview,
                    "upward reading must start while the source is still streaming"
                );
                self.last_len = text.len();
                let targets = Self::run(host).turn_targets().unwrap();
                assert!(
                    targets.iter().any(|turn| turn.input() != INPUT
                        && turn.row_ids().contains(&self.anchor.unwrap())),
                    "anchor must belong to a preceding historical turn"
                );
                assert!(
                    targets
                        .iter()
                        .find(|turn| turn.input() == INPUT)
                        .is_some_and(|turn| !turn.row_ids().contains(&self.anchor.unwrap())),
                    "do not mistake the active turn for old reading"
                );
                self.reading(host);
                run_script(vec![
                    (0, Self::click(host, host.view.ids().editor)),
                    (
                        100,
                        Action::PreeditRange {
                            text: DRAFT.into(),
                            selection: 2..4,
                            replacement: None,
                        },
                    ),
                ]);
                self.phase = 4;
            }
            4 if host.view.editor().input.marked.is_some() => {
                self.reading(host);
                assert_eq!(host.view.tree.focused(), Some(host.view.ids().editor));
                if let Some(baseline) = &self.baseline {
                    assert_eq!(host.view.editor().input.external_baseline(), *baseline);
                } else {
                    self.baseline = Some(host.view.editor().input.external_baseline());
                }
                let (text, preview) = Self::output(host).unwrap();
                if text.len() > self.last_len {
                    if preview {
                        self.updates += 1;
                    }
                    self.last_len = text.len();
                }
                if !preview
                    && !Self::snapshot(host)
                        .run
                        .as_ref()
                        .is_some_and(|run| run.can_cancel)
                    && text.ends_with("READING_STREAM_END")
                {
                    assert!(
                        self.updates > 0,
                        "observe a real append after anchoring old content"
                    );
                    let expected = format!(
                        "READING_STREAM_BEGIN\n{}{}READING_STREAM_END",
                        "流式第一段\n".repeat(60),
                        "流式第二段\n".repeat(60)
                    );
                    assert_eq!(text, expected);
                    Self::once(host);
                    run_script(vec![(0, Action::Unmark)]);
                    self.phase = 5;
                }
            }
            5 if host.view.editor().input.marked.is_none()
                && host.view.binding().current_run().unwrap().draft.text == DRAFT =>
            {
                self.reading(host);
                self.epoch = Self::snapshot(host).connection_epoch;
                let mut actions = vec![];
                if let Ok(path) = std::env::var("AGENT_STREAM_READING_CAPTURE") {
                    actions.push((0, Action::Capture { path }));
                }
                actions.push((100, Self::click(host, RECONNECT)));
                run_script(actions);
                self.phase = 6;
            }
            6 if Self::snapshot(host).connection_epoch > self.epoch
                && !Self::snapshot(host).transcript.is_stale =>
            {
                self.reading(host);
                Self::once(host);
                assert_eq!(host.view.editor().input.text, DRAFT);
                run_script(vec![(
                    0,
                    Self::click(host, tekes_agent_ui_demo::component_demo::SELECT_B),
                )]);
                self.phase = 7;
            }
            7 if host.view.selected() == 1 => {
                assert_eq!(host.view.editor().input.text, OTHER);
                run_script(vec![(
                    0,
                    Self::click(host, tekes_agent_ui_demo::component_demo::SELECT_A),
                )]);
                self.phase = 8;
            }
            8 if host.view.selected() == 0 => {
                self.reading(host);
                Self::once(host);
                assert_eq!(host.view.editor().input.text, DRAFT);
                self.phase = 9;
                run_script(vec![(0, Action::Close)]);
            }
            _ => {}
        }
        if previous != self.phase {
            eprintln!(
                "NATIVE_STREAM_READING_PHASE {} updates={}",
                self.phase, self.updates
            );
            window.request_redraw();
        }
    }
    pub fn receipt(&self) -> serde_json::Value {
        assert_eq!(self.phase, 9);
        assert!(self.updates > 0);
        serde_json::json!({"nativeWindow":true,"realHarness":true,"activeStreamingBeforeReading":true,"appendsObserved":self.updates,"originalItemAndLocalAnchorPreserved":true,"markedTextAndFocusPreserved":true,"originalEditorPreserved":true,"streamFinalTextExact":true,"oneSubmission":true,"reconnectNoReplay":true,"retainedReadingAfterSessionSwitch":true,"draftIsolation":true,"syntheticWheel":true,"syntheticPreedit":true,"physicalIme":false,"voiceOver":false})
    }
}
