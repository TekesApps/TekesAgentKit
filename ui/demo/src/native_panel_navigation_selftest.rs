//! Owned native host navigation through the original SidePanel controls and
//! TaskPaneBinding. Never cancels an execution or invents a task state machine.
use super::*;
use crate::acceptance_script::{Action, run_script};
use tekes_agent_components_ui::{RunPort, SidePanelNavigation, SidePanelTabKind};
use tekes_agent_presentation::AgentTask;
use tekes_agent_ui_bindings::TaskPaneBinding;

struct ChildOpenFixture {
    old_request: PanelRequest,
    new_request: PanelRequest,
    old: Arc<ChildPaneBinding>,
    new: Arc<ChildPaneBinding>,
    old_snapshot: PaneSnapshot,
    new_snapshot: PaneSnapshot,
}
pub(super) struct Probe {
    phase: u8,
    host_navigation: bool,
    side_chat_menu: bool,
    chat_ids: Vec<String>,
    chat_editor: Option<(ComponentId, usize)>,
    started: Instant,
    first_editor: Option<(ComponentId, usize)>,
    draft: String,
    admissions: usize,
    pending_child: Option<std::sync::mpsc::Receiver<ChildOpenFixture>>,
    child: Option<ChildOpenFixture>,
    child_editor: Option<ComponentId>,
    pending_close: Option<PanelRequest>,
    inactive_task_parent: Option<Arc<RunBinding>>,
}
impl Probe {
    pub fn new() -> Self {
        run_script(vec![(25_000, Action::Close)]);
        let side_chat_menu = std::env::args().any(|arg| arg == "--panel-side-chat-menu-selftest");
        Self {
            phase: if side_chat_menu { 14 } else { 0 },
            side_chat_menu,
            chat_ids: vec![],
            chat_editor: None,
            host_navigation: false,
            started: Instant::now(),
            first_editor: None,
            draft: String::new(),
            admissions: 0,
            pending_child: None,
            child: None,
            child_editor: None,
            pending_close: None,
            inactive_task_parent: None,
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
        let id = if let Some(navigation) = host.view.panel_navigation_id() {
            host.view
                .tree
                .get(navigation)
                .unwrap()
                .component::<SidePanelNavigation>()
                .unwrap()
                .close_id()
                .expect("current navigation has an original close control")
        } else {
            host.view
                .tree
                .get(host.view.panel_id())
                .unwrap()
                .component::<SidePanel>()
                .unwrap()
                .close_id()
        };
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
        if self.side_chat_menu {
            self.advance_chats(host, window);
            return;
        }
        let old = self.phase;
        match self.phase {
            0 if window.is_active() => {
                self.host_navigation = host.view.panel_navigation_id().is_some();
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
                let parent = host.view.binding().clone();
                let old_request = host.view.begin_child_open(parent.scope()).unwrap();
                let new_request = host.view.begin_child_open(parent.scope()).unwrap();
                let (sender, receiver) = std::sync::mpsc::channel();
                self.pending_child = Some(receiver);
                let wake = host.wake.clone().expect("native window wake");
                host.observers.push(host.executor.spawn(async move {
                    let old = Arc::new(
                        ChildPaneBinding::for_run(
                            &parent,
                            "child-a",
                            ChildHistoryLayout::IndependentSessions,
                            None,
                        )
                        .await
                        .unwrap(),
                    );
                    let new = Arc::new(
                        ChildPaneBinding::for_run(
                            &parent,
                            "child-a",
                            ChildHistoryLayout::IndependentSessions,
                            None,
                        )
                        .await
                        .unwrap(),
                    );
                    let old_snapshot = old.snapshot().await.unwrap();
                    let new_snapshot = new.snapshot().await.unwrap();
                    sender
                        .send(ChildOpenFixture {
                            old_request,
                            new_request,
                            old,
                            new,
                            old_snapshot,
                            new_snapshot,
                        })
                        .unwrap();
                    wake.wake();
                }));
                self.phase = 9;
            }
            9 => {
                if let Ok(child) = self.pending_child.as_ref().unwrap().try_recv() {
                    assert!(
                        host.view
                            .install_child_source(
                                child.new_request.clone(),
                                child.new.clone(),
                                "new child owner".into(),
                                child.new_snapshot.clone(),
                            )
                            .unwrap()
                    );
                    let editor = host.view.panel_composer_ids().unwrap().editor;
                    // Deliberately deliver the older original source last, then
                    // duplicate the accepted delivery. Neither may replace it.
                    for (request, source, snapshot) in [
                        (
                            child.old_request.clone(),
                            child.old.clone(),
                            child.old_snapshot.clone(),
                        ),
                        (
                            child.new_request.clone(),
                            child.new.clone(),
                            child.new_snapshot.clone(),
                        ),
                    ] {
                        assert!(
                            !host
                                .view
                                .install_child_source(
                                    request,
                                    source,
                                    "late or duplicate".into(),
                                    snapshot
                                )
                                .unwrap()
                        );
                    }
                    assert!(Arc::ptr_eq(
                        &host
                            .view
                            .cached_child_source(
                                host.view.binding().scope(),
                                child.new.parent(),
                                child.new.child_id(),
                                child.new.layout(),
                            )
                            .unwrap(),
                        &child.new
                    ));
                    assert_ne!(child.new.binding().access(), PaneAccess::Missing);
                    self.child_editor = Some(editor);
                    self.child = Some(child);
                    self.phase = 10;
                    window.request_redraw();
                }
            }
            10 if Self::run_is(host, "child-a") => {
                assert_eq!(
                    host.view.panel_composer_ids().unwrap().editor,
                    self.child_editor.unwrap()
                );
                let parent = host.view.binding().scope().clone();
                // A still-pending open newer than the accepted owner must be
                // fenced by the original close control, not just by that owner.
                self.pending_close = Some(host.view.begin_child_open(&parent).unwrap());
                Self::close(host);
                self.phase = 11;
            }
            11 if Self::input(host).selected == Some(SidePanelTabKind::Summary) => {
                let child = self.child.as_ref().unwrap();
                assert!(
                    !host
                        .view
                        .install_child_source(
                            self.pending_close.take().unwrap(),
                            child.old.clone(),
                            "closed pending child".into(),
                            child.old_snapshot.clone(),
                        )
                        .unwrap()
                );
                assert!(
                    host.view
                        .cached_child_source(
                            host.view.binding().scope(),
                            child.old.parent(),
                            child.old.child_id(),
                            child.old.layout(),
                        )
                        .is_none()
                );
                assert_eq!(Self::input(host).tabs.len(), 1);
                assert_eq!(child.new.binding().access(), PaneAccess::Missing);
                assert_eq!(
                    host.view.binding().current_run().unwrap().draft.text,
                    self.draft
                );
                assert_eq!(
                    host.view.binding().current_run().unwrap().submissions.len(),
                    self.admissions
                );
                self.phase = 12;
                self.inactive_task_parent = Some(host.view.binding().clone());
                Self::click(host, tekes_agent_ui_demo::component_demo::SELECT_B);
            }
            12 if host.view.binding().scope().session_id == "b" => {
                Self::click(host, host.view.ids().editor);
                self.phase = 13;
            }
            13 if window.is_active()
                && host.view.tree.focused() == Some(host.view.ids().editor) =>
            {
                let parent = self.inactive_task_parent.as_ref().unwrap();
                let source = Arc::new(TaskPaneBinding::for_run(parent));
                let token = source.begin(parent.scope().clone(), "late-task").unwrap();
                assert!(source.commit(
                    &token,
                    Ok(Some(AgentTask {
                        id: "late-task".into(),
                        scope: RunScope {
                            session_id: "child-a".into(),
                            ..parent.scope().clone()
                        },
                        name: "inactive parent".into(),
                        status: "running".into(),
                        kind: "task".into(),
                    }))
                ));
                let editor = host.view.ids().editor;
                let owner = host.view.tree.text_surface(editor).unwrap() as *const _ as usize;
                let baseline = host.view.editor().input.external_baseline();
                let state = host.view.binding().current_run().unwrap();
                let scope = host.view.binding().scope().clone();
                let request = host.view.begin_child_open(&scope).unwrap();
                assert!(host.view.open_task_source(source).is_err());
                assert_eq!(host.view.binding().scope(), &scope);
                assert_eq!(host.view.tree.focused(), Some(editor));
                assert_eq!(
                    host.view.tree.text_surface(editor).unwrap() as *const _ as usize,
                    owner
                );
                assert_eq!(host.view.editor().input.external_baseline(), baseline);
                assert_eq!(
                    host.view.binding().current_run().unwrap().draft,
                    state.draft
                );
                assert_eq!(
                    host.view.binding().current_run().unwrap().submissions.len(),
                    state.submissions.len()
                );
                assert!(host.view.child_request_current(&request));
                self.phase = 14;
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
        if self.side_chat_menu {
            assert_eq!(self.phase, 25, "SideChat menu did not finish");
            return serde_json::json!({"nativeWindow":true,"hostPanelNavigation":true,"nativeWindowToolbar":false,"originalPointerMenu":true,"twoActualScratchPointers":true,"duplicateVisibleLabels":true,"currentMemberChecked":true,"firstEditorRetained":true,"closedScratchRemoved":true,"mainDraftRetained":true,"noSubmission":true,"ownedFixture":true,"realHarness":false,"syntheticPointer":true,"physicalIme":false,"voiceOver":false,"processContainer":false});
        }
        assert_eq!(self.phase, 14, "panel navigation did not finish");
        serde_json::json!({"hostPanelNavigation":self.host_navigation,"nativeWindowToolbar":false,"nativeWindow":true,"originalPointerControls":true,"summaryCloseAndReopen":true,"openOrderRetained":true,"lastRemainingSelection":true,"emptyDocumentRetained":true,"originalTaskEditorRetained":true,"lateChildOwnerRejected":true,"duplicateChildDeliveryRejected":true,"pendingChildAfterCloseRejected":true,"originalChildEditorRetained":true,"inactiveTaskNavigationRejected":true,"currentParentDraftAndFocusRetained":true,"currentChildRequestRetained":true,"mainDraftRetained":true,"noSubmission":true,"ownedFixture":true,"realHarness":false,"physicalIme":false,"voiceOver":false,"processContainer":false})
    }
}

impl Probe {
    fn chat_menu(host: &Host) -> ComponentId {
        host.view
            .tree
            .get(host.view.panel_navigation_id().unwrap())
            .unwrap()
            .component::<SidePanelNavigation>()
            .unwrap()
            .side_chat_menu_id()
            .unwrap()
    }
    fn selected_chat(host: &Host) -> Option<String> {
        match &Self::input(host).selected {
            Some(SidePanelTabKind::SideChat { session_id }) => Some(session_id.clone()),
            _ => None,
        }
    }
    fn chat_count(host: &Host) -> usize {
        host.side_chats
            .snapshot()
            .chats
            .values()
            .filter(|chat| &chat.parent == host.view.binding().scope())
            .count()
    }
    fn click_menu_item(host: &Host, index: usize, checked: Option<usize>) -> bool {
        use tekes_ui_controls::composition::{ContentRole, MenuContent};
        let id = Self::chat_menu(host);
        if !host
            .view
            .tree
            .get(id)
            .unwrap()
            .component::<MenuContent>()
            .unwrap()
            .menu
            .is_open()
        {
            return false;
        }
        let nodes = host.view.tree.semantics();
        let menu = nodes.iter().find(|node| node.id == id).unwrap();
        let items: Vec<_> = menu
            .parts
            .iter()
            .filter(|part| part.semantics.role == ContentRole::MenuItem)
            .collect();
        if items.is_empty() {
            return false;
        }
        assert_eq!(items[0].semantics.name, "新建 SideChat");
        for item in &items[1..] {
            assert_eq!(item.semantics.name, "SideChat");
        }
        if let Some(checked) = checked {
            assert_eq!(items.len(), 3);
            for (position, item) in items.iter().enumerate().skip(1) {
                assert_eq!(item.semantics.selected, position == checked);
            }
        }
        let point = items[index].frame.center();
        run_script(vec![(
            0,
            Action::Click {
                x: point.x.into(),
                y: point.y.into(),
            },
        )]);
        true
    }
    fn advance_chats(&mut self, host: &mut Host, window: &Window) {
        if !window.is_active() {
            return;
        }
        let old = self.phase;
        match self.phase {
            14 => {
                assert!(host.view.panel_navigation_id().is_some());
                assert_eq!(Self::input(host).selected, Some(SidePanelTabKind::Summary));
                assert_eq!(Self::chat_count(host), 0);
                let main = host.view.binding().current_run().unwrap();
                self.draft = main.draft.text.clone();
                self.admissions = main.submissions.len();
                Self::close(host);
                self.phase = 15;
            }
            15 if Self::input(host).selected.is_none() => {
                let id = host
                    .view
                    .tree
                    .get(host.view.panel_id())
                    .unwrap()
                    .component::<SidePanel>()
                    .unwrap()
                    .launcher()
                    .unwrap()
                    .button_id(tekes_agent_components_ui::SidePanelDestination::SideChat)
                    .unwrap();
                Self::click(host, id);
                self.phase = 16;
            }
            16 if Self::selected_chat(host).is_some()
                && host.view.panel_composer_ids().is_some() =>
            {
                assert_eq!(Self::chat_count(host), 1);
                self.chat_ids.push(Self::selected_chat(host).unwrap());
                let id = host.view.panel_composer_ids().unwrap().editor;
                self.chat_editor = Some((
                    id,
                    host.view.tree.text_surface(id).unwrap() as *const _ as usize,
                ));
                Self::click(host, Self::chat_menu(host));
                self.phase = 17;
            }
            17 if Self::click_menu_item(host, 0, None) => {
                self.phase = 18;
            }
            18 if Self::selected_chat(host).is_some_and(|id| id != self.chat_ids[0])
                && host.view.panel_composer_ids().is_some() =>
            {
                assert_eq!(Self::chat_count(host), 2);
                self.chat_ids.push(Self::selected_chat(host).unwrap());
                Self::click(host, Self::chat_menu(host));
                self.phase = 19;
            }
            19 if Self::click_menu_item(host, 1, Some(2)) => {
                self.phase = 20;
            }
            20 if Self::selected_chat(host).as_ref() == Some(&self.chat_ids[0]) => {
                let (id, owner) = self.chat_editor.unwrap();
                assert_eq!(host.view.panel_composer_ids().unwrap().editor, id);
                assert_eq!(
                    host.view.tree.text_surface(id).unwrap() as *const _ as usize,
                    owner
                );
                Self::click(host, Self::chat_menu(host));
                self.phase = 21;
            }
            21 if Self::click_menu_item(host, 2, Some(1)) => {
                self.phase = 22;
            }
            22 if Self::selected_chat(host).as_ref() == Some(&self.chat_ids[1]) => {
                Self::close(host);
                self.phase = 23;
            }
            23 if Self::selected_chat(host).as_ref() == Some(&self.chat_ids[0])
                && Self::chat_count(host) == 1 =>
            {
                assert!(
                    !host
                        .side_chats
                        .snapshot()
                        .chats
                        .contains_key(&self.chat_ids[1])
                );
                let (id, owner) = self.chat_editor.unwrap();
                assert_eq!(host.view.panel_composer_ids().unwrap().editor, id);
                assert_eq!(
                    host.view.tree.text_surface(id).unwrap() as *const _ as usize,
                    owner
                );
                assert_eq!(
                    host.view.binding().current_run().unwrap().draft.text,
                    self.draft
                );
                assert_eq!(
                    host.view.binding().current_run().unwrap().submissions.len(),
                    self.admissions
                );
                Self::close(host);
                self.phase = 24;
            }
            24 if Self::input(host).selected.is_none() && Self::chat_count(host) == 0 => {
                assert!(Self::input(host).tabs.is_empty());
                assert_eq!(
                    host.view.binding().current_run().unwrap().draft.text,
                    self.draft
                );
                assert_eq!(
                    host.view.binding().current_run().unwrap().submissions.len(),
                    self.admissions
                );
                self.phase = 25;
                run_script(vec![(0, Action::Close)]);
            }
            _ => {}
        }
        if old != self.phase {
            eprintln!("NATIVE_PANEL_SIDE_CHAT_PHASE {}", self.phase);
            window.request_redraw();
        }
    }
}
