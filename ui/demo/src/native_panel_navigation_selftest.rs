//! Owned native host navigation through the original SidePanel controls and
//! TaskPaneBinding. Never cancels an execution or invents a task state machine.
use super::*;
use tekes_agent_components_ui::{RunPort, SidePanelTabKind};
use tekes_agent_presentation::AgentTask;
use tekes_agent_ui_bindings::TaskPaneBinding;
use tekes_ui_platform_macos::selftest::{Action, run_script};

pub(super) struct Probe {
    phase: u8,
    started: Instant,
    first_editor: Option<(ComponentId, usize)>,
    draft: String,
    admissions: usize,
}
impl Probe {
    pub fn new() -> Self {
        run_script(vec![(25_000, Action::Close)]);
        Self {
            phase: 0,
            started: Instant::now(),
            first_editor: None,
            draft: String::new(),
            admissions: 0,
        }
    }
    fn input(host: &Host) -> &tekes_agent_components_ui::SidePanelInput {
        host.view
            .tree
            .get(host.view.panel_id())
            .unwrap()
            .component::<SidePanel>()
            .unwrap()
            .displayed_input()
            .unwrap()
    }
    fn click(host: &Host, id: ComponentId) {
        let p = host
            .view
            .tree
            .get(id)
            .unwrap()
            .visible()
            .expect("original navigation control must be visible")
            .center();
        run_script(vec![(
            0,
            Action::Click {
                x: p.x.into(),
                y: p.y.into(),
            },
        )]);
    }
    fn close(host: &Host) {
        let id = host
            .view
            .tree
            .get(host.view.panel_id())
            .unwrap()
            .component::<SidePanel>()
            .unwrap()
            .close_id();
        Self::click(host, id);
    }
    fn open_task(host: &mut Host, id: &str, session: &str) {
        let parent = host.view.binding().clone();
        let source = Arc::new(TaskPaneBinding::for_run(&parent));
        let token = source.begin(parent.scope().clone(), id).unwrap();
        assert!(source.commit(
            &token,
            Ok(Some(AgentTask {
                id: id.into(),
                scope: RunScope {
                    session_id: session.into(),
                    ..parent.scope().clone()
                },
                name: id.into(),
                status: "running".into(),
                kind: "task".into()
            }))
        ));
        host.view.open_task_source(source).unwrap();
        let commands = host.view.take_background_commands();
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, DemoCommand::OpenTask { .. }))
        );
        host.commands(Ok(commands));
    }
    fn run_is(host: &Host, session: &str) -> bool {
        host.view
            .panel_composer_ids()
            .and_then(|ids| host.view.tree.get(ComponentId(ids.root.0 + 16)))
            .and_then(|node| node.component::<RunPort>())
            .is_some_and(|run| run.scope().session_id == session)
    }
    pub fn advance(&mut self, host: &mut Host, window: &Window) {
        assert!(
            self.started.elapsed() < Duration::from_secs(25),
            "panel navigation phase {}",
            self.phase
        );
        let old = self.phase;
        match self.phase {
            0 if window.is_active() => {
                let run = host.view.binding().current_run().unwrap();
                self.draft = run.draft.text.clone();
                self.admissions = run.submissions.len();
                Self::close(host);
                self.phase = 1;
            }
            1 if Self::input(host).tabs.is_empty() => {
                assert!(Self::input(host).selected.is_none());
                Self::click(host, tekes_agent_ui_demo::component_demo::OPEN_SUMMARY);
                self.phase = 2;
            }
            2 if Self::input(host).selected == Some(SidePanelTabKind::Summary) => {
                Self::open_task(host, "z-first", "child-a");
                self.phase = 3;
            }
            3 if Self::run_is(host, "child-a") => {
                let editor = host.view.panel_composer_ids().unwrap().editor;
                self.first_editor = Some((
                    editor,
                    host.view.tree.text_surface(editor).unwrap() as *const _ as usize,
                ));
                Self::open_task(host, "a-second", "child-b");
                self.phase = 4;
            }
            4 if Self::run_is(host, "child-b") => {
                assert_eq!(
                    Self::input(host)
                        .tabs
                        .iter()
                        .map(|tab| tab.kind.clone())
                        .collect::<Vec<_>>(),
                    vec![
                        SidePanelTabKind::Summary,
                        SidePanelTabKind::Task {
                            task_id: "z-first".into()
                        },
                        SidePanelTabKind::Task {
                            task_id: "a-second".into()
                        }
                    ]
                );
                Self::close(host);
                self.phase = 5;
            }
            5 if Self::run_is(host, "child-a") => {
                let (editor, owner) = self.first_editor.unwrap();
                assert_eq!(
                    host.view.tree.text_surface(editor).unwrap() as *const _ as usize,
                    owner
                );
                Self::close(host);
                self.phase = 6;
            }
            6 if Self::input(host).selected == Some(SidePanelTabKind::Summary) => {
                Self::close(host);
                self.phase = 7;
            }
            7 if Self::input(host).tabs.is_empty() => {
                assert!(Self::input(host).selected.is_none());
                let run = host.view.binding().current_run().unwrap();
                assert_eq!(run.draft.text, self.draft);
                assert_eq!(run.submissions.len(), self.admissions);
                Self::click(host, tekes_agent_ui_demo::component_demo::OPEN_SUMMARY);
                self.phase = 8;
            }
            8 if Self::input(host).selected == Some(SidePanelTabKind::Summary) => {
                self.phase = 9;
                run_script(vec![(0, Action::Close)]);
            }
            _ => {}
        }
        if old != self.phase {
            eprintln!("NATIVE_PANEL_NAVIGATION_PHASE {}", self.phase);
            window.request_redraw();
        }
    }
    pub fn receipt(&self) -> serde_json::Value {
        assert_eq!(self.phase, 9, "panel navigation did not finish");
        serde_json::json!({"nativeWindow":true,"originalPointerControls":true,"summaryCloseAndReopen":true,"openOrderRetained":true,"lastRemainingSelection":true,"emptyDocumentRetained":true,"originalTaskEditorRetained":true,"mainDraftRetained":true,"noSubmission":true,"ownedFixture":true,"realHarness":false,"physicalIme":false,"voiceOver":false,"processContainer":false})
    }
}
