//! Owned-fixture native pointer proof of the original Menu and preference store.
use super::*;
use crate::acceptance_script::{Action, run_script};
use tekes_agent_components_ui::{SidebarOptions, SidebarSort};
use tekes_ui_controls::composition::{ButtonContent, MenuContent, PointerPhase};
pub(super) struct Probe {
    phase: u8,
    session_menu: bool,
    other_draft: String,
    started: Instant,
    baseline: Option<tekes_ui_controls::TextEditBaseline>,
    draft: String,
    row: Option<ComponentId>,
    widget: usize,
    revision: u64,
    barrier_target: Option<Rect>,
    barrier_seen: bool,
}
impl Probe {
    pub fn new() -> Self {
        run_script(vec![(20_000, Action::Close)]);
        Self {
            phase: 0,
            session_menu: false,
            other_draft: String::new(),
            started: Instant::now(),
            baseline: None,
            draft: String::new(),
            row: None,
            widget: 0,
            revision: 0,
            barrier_target: None,
            barrier_seen: false,
        }
    }
    pub fn new_session_menu() -> Self {
        let mut probe = Self::new();
        probe.session_menu = true;
        probe
    }
    fn menu(&self, host: &Host) -> ComponentId {
        if self.session_menu {
            let sidebar = host
                .view
                .tree
                .get(host.view.sidebar_id())
                .unwrap()
                .component::<Sidebar>()
                .unwrap();
            return host
                .view
                .tree
                .get(sidebar.session_menu_id().unwrap())
                .unwrap()
                .component::<tekes_agent_components_ui::SidebarSessionMenu>()
                .unwrap()
                .menu_id()
                .unwrap();
        }

        host.view
            .tree
            .get(host.view.sidebar_options_id())
            .unwrap()
            .component::<SidebarOptions>()
            .unwrap()
            .menu_id()
    }
    fn click(host: &Host, id: ComponentId) -> Action {
        let point = host.view.tree.get(id).unwrap().visible().unwrap().center();
        Action::Click {
            x: point.x as f64,
            y: point.y as f64,
        }
    }
    fn item(&self, host: &Host, label: &str) -> Option<Action> {
        let menu = self.menu(host);
        let part = host
            .view
            .tree
            .semantics()
            .into_iter()
            .find(|n| n.id == menu)?
            .parts
            .into_iter()
            .find(|p| p.semantics.name == label)?;
        let point = part.frame.center();
        Some(Action::Click {
            x: point.x as f64,
            y: point.y as f64,
        })
    }
    fn preserved(&self, host: &Host) {
        assert_eq!(host.view.editor().input.text, self.draft);
        assert_eq!(
            host.view
                .tree
                .get(self.row.unwrap())
                .unwrap()
                .component::<ButtonContent>()
                .unwrap() as *const _ as usize,
            self.widget
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
    pub fn observe(&mut self, event: &ContentEvent, composing: bool) {
        if self.phase != 2 || !composing {
            return;
        }
        let point = match event {
            ContentEvent::Pointer(pointer) if pointer.phase == PointerPhase::Up => {
                Some(pointer.position)
            }
            ContentEvent::PointerUp(point) => Some(*point),
            _ => None,
        };
        if point.is_some_and(|point| self.barrier_target.is_some_and(|rect| rect.contains(point))) {
            self.barrier_seen = true;
        }
    }
    pub fn advance(&mut self, host: &mut Host, window: &Window) {
        assert!(
            self.started.elapsed() < Duration::from_secs(20),
            "Sidebar options deadline phase {}",
            self.phase
        );
        if !window.is_active() {
            return;
        }
        let old = self.phase;
        match self.phase {
            0 if host.sidebar.snapshot().loaded => {
                let sidebar = host
                    .view
                    .tree
                    .get(host.view.sidebar_id())
                    .unwrap()
                    .component::<Sidebar>()
                    .unwrap();
                self.row = sidebar.row_id(&host.view.binding().scope().address());
                self.widget = host
                    .view
                    .tree
                    .get(self.row.unwrap())
                    .unwrap()
                    .component::<ButtonContent>()
                    .unwrap() as *const _ as usize;
                self.revision = host.sidebar.preferences().unwrap().revision;
                self.other_draft = host.view.bindings()[1]
                    .current_run()
                    .unwrap()
                    .draft
                    .text
                    .clone();
                run_script(vec![
                    (0, Self::click(host, host.view.ids().editor)),
                    (
                        100,
                        Action::PreeditRange {
                            text: "选项中文组字".into(),
                            selection: 1..3,
                            replacement: None,
                        },
                    ),
                ]);
                self.phase = 1;
            }
            1 if host.view.editor().input.marked.is_some() => {
                self.baseline = Some(host.view.editor().input.external_baseline());
                self.barrier_target = host.view.tree.get(self.menu(host)).unwrap().visible();
                run_script(vec![
                    (0, Self::click(host, self.menu(host))),
                    (100, Action::Log("SIDEBAR_OPTIONS_IME_BARRIER".into())),
                ]);
                self.phase = 2;
            }
            2 if self.barrier_seen => {
                assert_eq!(host.view.tree.focused(), Some(host.view.ids().editor));
                assert_eq!(
                    Some(host.view.editor().input.external_baseline()),
                    self.baseline
                );
                assert!(
                    !host
                        .view
                        .tree
                        .get(self.menu(host))
                        .unwrap()
                        .component::<MenuContent>()
                        .unwrap()
                        .menu
                        .is_open()
                );
                assert_eq!(host.sidebar.preferences().unwrap().revision, self.revision);
                run_script(vec![(150, Action::Unmark)]);
                self.phase = 3;
            }
            3 if host.view.editor().input.marked.is_none() => {
                self.draft = host.view.editor().input.text.clone();
                assert!(self.draft.contains("选项中文组字"));
                run_script(vec![(0, Self::click(host, self.menu(host)))]);
                self.phase = 4;
            }
            _ if self.session_menu && self.phase >= 4 => self.advance_session_menu(host),
            4 => {
                if let Some(action) = self.item(host, "排序方式") {
                    run_script(vec![(0, action)]);
                    self.phase = 5;
                }
            }
            5 => {
                if let Some(action) = self.item(host, "标题 A–Z") {
                    run_script(vec![(0, action)]);
                    self.phase = 6;
                }
            }
            6 if host.sidebar.preferences().unwrap().preferences.sort
                == SidebarSort::TitleAscending =>
            {
                assert!(host.sidebar.preferences().unwrap().revision > self.revision);
                self.preserved(host);
                run_script(vec![(0, Self::click(host, self.menu(host)))]);
                self.phase = 7;
            }
            7 => {
                if let Some(action) = self.item(host, "分组方式") {
                    run_script(vec![(0, action)]);
                    self.phase = 8;
                }
            }
            8 => {
                if let Some(action) = self.item(host, "主文件夹") {
                    run_script(vec![(0, action)]);
                    self.phase = 9;
                }
            }
            9 if host
                .sidebar
                .preferences()
                .unwrap()
                .preferences
                .group_by_folder =>
            {
                self.preserved(host);
                let mut actions = vec![];
                if let Ok(path) = std::env::var("AGENT_SIDEBAR_OPTIONS_CAPTURE") {
                    actions.push((0, Action::Capture { path }));
                }
                actions.push((500, Action::Close));
                run_script(actions);
                self.phase = 10;
            }
            _ => {}
        }
        if old != self.phase {
            eprintln!("NATIVE_SIDEBAR_OPTIONS_PHASE {}", self.phase);
            window.request_redraw();
        }
    }
    fn is_pinned(host: &Host, session: &str) -> bool {
        host.sidebar
            .snapshot()
            .rows
            .iter()
            .find(|row| row.address.session_id == session)
            .is_some_and(|row| row.pin.as_ref().is_some_and(|pin| pin.pinned))
    }
    fn advance_session_menu(&mut self, host: &mut Host) {
        match self.phase {
            4 => {
                if let Some(action) = self.item(host, "置顶") {
                    run_script(vec![(0, action)]);
                    self.phase = 5;
                }
            }
            5 if Self::is_pinned(host, "a") => {
                self.preserved(host);
                run_script(vec![(
                    0,
                    Self::click(host, tekes_agent_ui_demo::component_demo::SELECT_B),
                )]);
                self.phase = 6;
            }
            6 if host.view.selected() == 1 => {
                assert_eq!(host.view.editor().input.text, self.other_draft);
                click_thread_action_script(host, "pin");
                self.phase = 7;
            }
            7 if Self::is_pinned(host, "b") => {
                run_script(vec![(
                    0,
                    Self::click(host, tekes_agent_ui_demo::component_demo::SELECT_A),
                )]);
                self.phase = 8;
            }
            8 if host.view.selected() == 0 => {
                self.preserved(host);
                run_script(vec![(0, Self::click(host, self.menu(host)))]);
                self.phase = 9;
            }
            9 => {
                if let Some(action) = self.item(host, "下移置顶") {
                    run_script(vec![(0, action)]);
                    self.phase = 10;
                }
            }
            10 if host.sidebar.preferences().unwrap().revision > self.revision => {
                self.preserved(host);
                let value = host.sidebar.preferences().unwrap();
                assert_eq!(
                    value
                        .preferences
                        .pin_order
                        .iter()
                        .map(|a| a.session_id.as_str())
                        .collect::<Vec<_>>(),
                    ["b", "a"]
                );
                assert_eq!(
                    host.view.bindings()[1].current_run().unwrap().draft.text,
                    self.other_draft
                );
                assert!(
                    host.view.bindings()[1]
                        .current_run()
                        .unwrap()
                        .submissions
                        .is_empty()
                );
                let mut actions = vec![];
                if let Ok(path) = std::env::var("AGENT_SIDEBAR_SESSION_MENU_CAPTURE") {
                    actions.push((0, Action::Capture { path }));
                }
                actions.push((500, Action::Close));
                run_script(actions);
                self.phase = 11;
            }
            _ => {}
        }
    }
    pub fn receipt(&self) -> serde_json::Value {
        assert_eq!(self.phase, if self.session_menu { 11 } else { 10 });
        serde_json::json!({"nativeWindow":true,"nativePointer":true,"originalSdkMenu":true,"sessionMenu":self.session_menu,"sortAndGroupPersisted":!self.session_menu,"sharedPinAndMovePersisted":self.session_menu,"originalRowRetained":true,"draftPreserved":true,"markedTextBarrier":true,"noSubmission":true,"physicalIme":false,"voiceOver":false})
    }
}
