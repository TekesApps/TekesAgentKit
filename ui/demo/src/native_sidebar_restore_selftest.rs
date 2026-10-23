//! Native rendering probe of public inventory publications. Owned fixture only;
//! injected observation failures do not simulate a real harness outage.
use super::*;
use crate::acceptance_script::{Action, run_script};
use tekes_agent_ui_bindings::SidebarPublication;
use tekes_ui_controls::composition::ButtonContent;
const FAILURE: &str = "Sidebar 观察失败：保留最近可读列表";
pub(super) struct Probe {
    phase: u8,
    revision: u64,
    started: Instant,
    row: Option<ComponentId>,
    widget: usize,
    ime_started: bool,
    baseline: Option<tekes_ui_controls::TextEditBaseline>,
    editor: Option<ComponentId>,
    capture_hold: Option<(u8, Instant)>,
}
impl Probe {
    pub fn new() -> Self {
        run_script(vec![(15000, Action::Close)]);
        Self {
            phase: 0,
            revision: 0,
            started: Instant::now(),
            row: None,
            widget: 0,
            ime_started: false,
            baseline: None,
            editor: None,
            capture_hold: None,
        }
    }
    fn hold_capture(&mut self, key: &str, window: &Window) -> bool {
        let Ok(path) = std::env::var(key) else {
            return false;
        };
        if let Some((phase, started)) = self.capture_hold {
            assert_eq!(phase, self.phase);
            if started.elapsed() >= Duration::from_millis(1200) {
                self.capture_hold = None;
                return false;
            }
        } else {
            run_script(vec![(0, Action::Capture { path })]);
            self.capture_hold = Some((self.phase, Instant::now()));
        }
        // Owned probe only: allow the SDK's asynchronous screencapture to finish.
        window.request_redraw();
        true
    }
    pub fn source(&mut self, source: Arc<SidebarPublication>) -> Arc<SidebarPublication> {
        if !source.loaded || source.rows.is_empty() {
            return source;
        }
        self.revision = self.revision.max(source.revision).saturating_add(1);
        let mut input = (*source).clone();
        input.revision = self.revision;
        match self.phase {
            0 | 3 => input.loaded = false,
            2 => {
                input.error = Some(FAILURE.into());
                for row in &mut input.rows {
                    row.activity.is_stale = true;
                }
            }
            4 => input.rows.clear(),
            _ => {}
        }
        Arc::new(input)
    }
    pub fn advance(&mut self, host: &mut Host, window: &Window) {
        assert!(
            self.started.elapsed() < Duration::from_secs(15),
            "Sidebar probe deadline phase {}",
            self.phase
        );
        let sidebar = host
            .view
            .tree
            .get(host.view.sidebar_id())
            .unwrap()
            .component::<Sidebar>()
            .unwrap();
        let Some(input) = sidebar.displayed_input() else {
            return;
        };
        if self.revision == 0 || self.phase >= 6 {
            return;
        }
        let names = host
            .view
            .tree
            .semantics()
            .into_iter()
            .map(|node| node.semantics.name)
            .collect::<Vec<_>>();
        if self.phase == 0 && self.baseline.is_none() {
            if !window.is_active() {
                window.request_redraw();
                return;
            }
            if !self.ime_started {
                let editor = host.view.ids().editor;
                let point = host
                    .view
                    .tree
                    .get(editor)
                    .unwrap()
                    .visible()
                    .unwrap()
                    .center();
                self.editor = Some(editor);
                self.ime_started = true;
                run_script(vec![
                    (
                        0,
                        Action::Click {
                            x: point.x as f64,
                            y: point.y as f64,
                        },
                    ),
                    (
                        100,
                        Action::PreeditRange {
                            text: "独立补读组字".into(),
                            selection: 1..4,
                            replacement: None,
                        },
                    ),
                ]);
                return;
            }
            if host.view.editor().input.marked.is_none() {
                return;
            }
            self.baseline = Some(host.view.editor().input.external_baseline());
        }
        assert_eq!(host.view.tree.focused(), self.editor);
        assert_eq!(
            Some(host.view.editor().input.external_baseline()),
            self.baseline
        );
        match self.phase {
            0 | 3 => {
                assert!(!input.loaded);
                assert!(names.iter().any(|name| name == "正在恢复会话…"));
                assert!(!names.iter().any(|name| name == "暂无会话"));
                assert!(
                    input
                        .rows
                        .iter()
                        .all(|row| sidebar.row_id(&row.address).is_none())
                );
                if let Some(old) = self.row {
                    assert!(host.view.tree.get(old).is_none());
                }
            }
            1 => {
                let id = sidebar.row_id(&input.rows[0].address).unwrap();
                self.row = Some(id);
                self.widget = host
                    .view
                    .tree
                    .get(id)
                    .unwrap()
                    .component::<ButtonContent>()
                    .unwrap() as *const _ as usize;
            }
            2 => {
                assert!(
                    sidebar.visibility_target().is_none(),
                    "native host must consume explicit reveal"
                );
                let version = host
                    .view
                    .tree
                    .get(sidebar.inventory_viewport_id())
                    .unwrap()
                    .component::<tekes_ui_controls::composition::ScrollContent>()
                    .unwrap()
                    .reading_document_version()
                    .expect("native host attached original SDK reading document");
                assert_eq!(version.revision(), 1);
                assert_eq!(input.error.as_deref(), Some(FAILURE));
                assert!(names.iter().any(|name| name == FAILURE));
                let id = sidebar.row_id(&input.rows[0].address).unwrap();
                assert_eq!(Some(id), self.row);
                assert_eq!(
                    host.view
                        .tree
                        .get(id)
                        .unwrap()
                        .component::<ButtonContent>()
                        .unwrap() as *const _ as usize,
                    self.widget
                );
                if self.hold_capture("AGENT_SIDEBAR_RESTORE_CAPTURE", window) {
                    return;
                }
            }
            4 => {
                // The published inventory is empty, but a separate canonical
                // read still owns the actual selected conversation.
                assert!(input.loaded && input.rows.len() == 1);
                assert_eq!(input.selected.as_ref(), Some(&input.rows[0].address));
                assert!(!names.iter().any(|name| name == "暂无会话"));
                if self.hold_capture("AGENT_SIDEBAR_SELECTED_CAPTURE", window) {
                    return;
                }
            }
            5 => {
                let id = sidebar.row_id(&input.rows[0].address).unwrap();
                assert_ne!(Some(id), self.row);
                assert!(
                    host.view
                        .binding()
                        .current_run()
                        .unwrap()
                        .submissions
                        .is_empty()
                );
                run_script(vec![(100, Action::Close)]);
            }
            _ => unreachable!(),
        }
        if self.phase == 1 {
            // An explicit same-session reveal is not a context switch. Inject the
            // public business navigation while native preedit owns the editor.
            let selected = input.selected.clone();
            let root = host.view.sidebar_id();
            host.view
                .tree
                .get_mut(root)
                .unwrap()
                .component_mut::<Sidebar>()
                .unwrap()
                .reconcile_navigation(&tekes_agent_components_ui::SidebarSelectionNavigation {
                    revision: 1,
                    selected,
                })
                .unwrap();
        }
        self.phase += 1;
        window.request_redraw();
    }
    pub fn receipt(&self) -> serde_json::Value {
        assert_eq!(self.phase, 6, "Sidebar probe did not finish");
        serde_json::json!({"nativeWindow":true,"explicitNativeReveal":true,"restoringNotEmpty":true,"failureRetainsRows":true,"canonicalSelectedSupplement":true,"markedBaselineAndFocusPreserved":true,"noSubmission":true,"retiredIdsNotReused":true,"realHarnessOutage":false,"fullLayoutParity":false,"physicalIme":false,"voiceOver":false})
    }
}
