//! Local fixture lifecycle through the normal runtime and retained native Run.
use super::*;
use crate::acceptance_script::{Action, run_script};
use tekes_agent_components_ui::{Composer, RunPort};
use tekes_agent_presentation::RunPhase;
const PREEDIT: &str = "中文归档草稿";
pub(super) struct Probe {
    failure_fixture: Option<tekes_agent_ui_demo::fixture::Fixture>,
    started: Instant,
    phase: u8,
    deadline: bool,
    editor: Option<usize>,
    viewport_bottom: f32,
    composer_height: f32,
    items: usize,
}
impl Probe {
    pub fn new(failure_fixture: Option<tekes_agent_ui_demo::fixture::Fixture>) -> Self {
        Self {
            failure_fixture,
            started: Instant::now(),
            phase: 0,
            deadline: false,
            editor: None,
            viewport_bottom: 0.,
            composer_height: 0.,
            items: 0,
        }
    }
    fn lifecycle(host: &Host, method: &'static str) {
        let runtime = host.runtime.clone();
        let scope = host.view.binding().scope().clone();
        host.executor.spawn(async move {
            runtime
                .call(
                    method,
                    serde_json::json!({"sessionId":scope.session_id.clone()}),
                    &format!("visibility-{method}"),
                )
                .await
                .expect("owned fixture lifecycle");
            if method == "session.unarchive" {
                // Archive explicitly closes the journal. Reopening a pane uses
                // the public history lease API; unarchive alone is not a read.
                // Keep the original binding/editor and its existing owner alive.
                runtime
                    .acquire_history(&scope.session_id)
                    .await
                    .expect("restored history baseline")
                    .release()
                    .await
                    .expect("release temporary read owner");
            }
        });
    }
    fn composer(view: &ComponentDemo) -> &Composer {
        view.tree
            .get(view.ids().root)
            .unwrap()
            .component::<Composer>()
            .unwrap()
    }
    pub fn advance(&mut self, host: &mut Host, window: &Window) {
        if !self.deadline {
            self.deadline = true;
            run_script(vec![(30_000, Action::Close)]);
        }
        assert!(
            self.started.elapsed() < Duration::from_secs(30),
            "visibility timeout phase {}",
            self.phase
        );
        let ids = host.view.ids();
        let run_id = ComponentId(ids.root.0 + 16);
        let snapshot = host
            .view
            .tree
            .get(run_id)
            .unwrap()
            .component::<RunPort>()
            .unwrap()
            .displayed_snapshot()
            .expect("rendered Run publication")
            .clone();
        match self.phase {
            0 if snapshot.can_submit => {
                assert!(Self::composer(&host.view).is_presented());
                self.editor = Some(Self::composer(&host.view).editor() as *const _ as usize);
                self.items = snapshot.transcript.items.len();
                self.viewport_bottom = host
                    .view
                    .tree
                    .get(run_id)
                    .unwrap()
                    .component::<RunPort>()
                    .unwrap()
                    .layout_geometry()
                    .unwrap()
                    .visible_document
                    .max_y();
                self.composer_height = host.view.tree.get(ids.root).unwrap().frame().size.height;
                let p = host.view.tree.get(ids.editor).unwrap().frame().center();
                self.phase = 1;
                run_script(vec![
                    (
                        100,
                        Action::Click {
                            x: p.x as f64,
                            y: p.y as f64,
                        },
                    ),
                    (
                        200,
                        Action::PreeditRange {
                            text: PREEDIT.into(),
                            selection: 6..6,
                            replacement: None,
                        },
                    ),
                ]);
            }
            1 if Self::composer(&host.view).is_composing() => {
                assert_eq!(snapshot.draft.text, "", "preedit must stay local");
                // Capture the settled preedit frame, not the initial window frame.
                self.viewport_bottom = host
                    .view
                    .tree
                    .get(run_id)
                    .unwrap()
                    .component::<RunPort>()
                    .unwrap()
                    .layout_geometry()
                    .unwrap()
                    .visible_document
                    .max_y();
                self.composer_height = host.view.tree.get(ids.root).unwrap().frame().size.height;
                if let Some(fixture) = &self.failure_fixture {
                    fixture
                        .publish_projection_failure(&host.view.binding().scope().session_id)
                        .unwrap();
                    self.phase = 10;
                } else {
                    self.phase = 2;
                    Self::lifecycle(host, "session.archive");
                }
            }
            10 if snapshot.connection.phase
                == tekes_agent_presentation::ConnectionPhase::Degraded =>
            {
                assert!(
                    snapshot
                        .connection
                        .message
                        .as_deref()
                        .is_some_and(|message| message
                            .contains("promoted assistant is outside loaded history")),
                    "must exercise semantic projection fallback, not a transport error"
                );
                assert!(Self::composer(&host.view).is_presented());
                assert!(!snapshot.can_submit);
                assert_eq!(snapshot.transcript.items.len(), self.items);
                self.phase = 2;
                Self::lifecycle(host, "session.archive");
            }
            2 if snapshot
                .run
                .as_ref()
                .is_some_and(|r| r.phase == RunPhase::Archived) =>
            {
                if self.failure_fixture.is_some() {
                    assert!(
                        snapshot
                            .connection
                            .message
                            .as_deref()
                            .is_some_and(|message| message
                                .contains("promoted assistant is outside loaded history"))
                    );
                }
                let composer = Self::composer(&host.view);
                assert!(!composer.is_presented());
                assert!(!composer.is_composing());
                assert_eq!(composer.editor() as *const _ as usize, self.editor.unwrap());
                assert_eq!(composer.editor().input.text, PREEDIT);
                assert!(
                    composer.editor().input.marked.is_none(),
                    "library owns blur/unmark"
                );
                assert!(host.view.tree.focused().is_none());
                assert!(host.view.tree.focused_text().is_none());
                assert!(
                    !host
                        .view
                        .tree
                        .semantics()
                        .iter()
                        .any(|n| n.id == ids.root || n.id == ids.editor)
                );
                assert!(
                    !host
                        .view
                        .tree
                        .accessibility_action(ids.send, None, AccessibilityAction::Press)
                        .activated
                );
                assert_eq!(snapshot.transcript.items.len(), self.items);
                // Archive also changes paging readiness and can grow its status
                // row. The freed lower edge, not total viewport height, proves
                // that the original Composer and its gap leave no bottom slot.
                let bottom = host
                    .view
                    .tree
                    .get(run_id)
                    .unwrap()
                    .component::<RunPort>()
                    .unwrap()
                    .layout_geometry()
                    .unwrap()
                    .visible_document
                    .max_y();
                assert!(
                    (bottom
                        - self.viewport_bottom
                        - self.composer_height
                        - tekes_ui_tokens::Spacing::Regular.value()
                        - tekes_ui_tokens::Spacing::Loose.value())
                    .abs()
                        < 0.1
                );
                assert!(
                    host.view
                        .tree
                        .get_mut(run_id)
                        .unwrap()
                        .component_mut::<RunPort>()
                        .unwrap()
                        .take_intents(&[])
                        .is_empty()
                );
                self.phase = 3;
                Self::lifecycle(host, "session.unarchive");
            }
            3 if self.failure_fixture.is_some()
                && snapshot
                    .run
                    .as_ref()
                    .is_some_and(|r| r.phase != RunPhase::Archived)
                && snapshot.connection.phase
                    == tekes_agent_presentation::ConnectionPhase::Degraded =>
            {
                assert!(Self::composer(&host.view).is_presented());
                assert!(!snapshot.can_submit);
                self.failure_fixture
                    .as_ref()
                    .unwrap()
                    .repair_projection_failure(&host.view.binding().scope().session_id)
                    .unwrap();
                let runtime = host.runtime.clone();
                host.executor.spawn(async move {
                    runtime
                        .reconnect()
                        .await
                        .expect("explicit repair reconnect");
                });
                self.phase = 11;
            }
            3 | 11 if snapshot.can_submit => {
                let composer = Self::composer(&host.view);
                assert!(composer.is_presented());
                assert_eq!(composer.editor() as *const _ as usize, self.editor.unwrap());
                assert_eq!(composer.editor().input.text, PREEDIT);
                assert!(
                    host.view.tree.focused().is_none(),
                    "restore cannot steal focus"
                );
                assert!(
                    host.view
                        .tree
                        .semantics()
                        .iter()
                        .any(|n| n.id == ids.editor)
                );
                assert_eq!(
                    snapshot.transcript.items.len(),
                    self.items,
                    "lifecycle must not submit"
                );
                assert!(
                    host.view.bindings()[1]
                        .current_run()
                        .unwrap()
                        .draft
                        .text
                        .is_empty()
                );
                self.phase = 4;
                run_script(vec![(100, Action::Close)]);
            }
            _ => {}
        }
        window.set_animating(false);
    }
    pub fn receipt(&self, view: &ComponentDemo) -> serde_json::Value {
        assert_eq!(self.phase, 4);
        assert_eq!(
            view.binding().current_run().unwrap().transcript.items.len(),
            self.items
        );
        serde_json::json!({"nativeWindow":true,"formalRunPort":true,"runtimeArchiveUnarchive":true,"projectionFailureRecovery":self.failure_fixture.is_some(),
            "nativeSyntheticMarkedText":true,"hiddenFromAccessibility":true,"focusReconciledByLibrary":true,
            "noHiddenLayoutSlot":true,"sameEditorRetained":true,"draftIsolation":true,"noSubmission":true,
            "physicalIme":false,"voiceOver":false,"realHarness":false,"fullLayoutParity":false})
    }
}
