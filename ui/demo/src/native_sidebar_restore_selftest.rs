//! Native rendering probe of public inventory publications. Owned fixture only;
//! injected observation failures do not simulate a real harness outage.
use super::*;
use tekes_agent_ui_bindings::SidebarPublication;
use tekes_ui_controls::composition::ButtonContent;
use tekes_ui_platform_macos::selftest::{Action, run_script};
const FAILURE: &str = "Sidebar 观察失败：保留最近可读列表";
pub(super) struct Probe {
    phase: u8,
    revision: u64,
    started: Instant,
    row: Option<ComponentId>,
    widget: usize,
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
        }
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
    pub fn advance(&mut self, host: &Host, window: &Window) {
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
                if let Ok(path) = std::env::var("AGENT_SIDEBAR_RESTORE_CAPTURE") {
                    run_script(vec![(0, Action::Capture { path })]);
                }
            }
            4 => {
                assert!(input.loaded && input.rows.is_empty());
                assert!(names.iter().any(|name| name == "暂无会话"));
            }
            5 => {
                let id = sidebar.row_id(&input.rows[0].address).unwrap();
                assert_ne!(Some(id), self.row);
                run_script(vec![(100, Action::Close)]);
            }
            _ => unreachable!(),
        }
        self.phase += 1;
        window.request_redraw();
    }
    pub fn receipt(&self) -> serde_json::Value {
        assert_eq!(self.phase, 6, "Sidebar probe did not finish");
        serde_json::json!({"nativeWindow":true,"restoringNotEmpty":true,"failureRetainsRows":true,"retiredIdsNotReused":true,"realHarnessOutage":false,"fullLayoutParity":false,"physicalIme":false,"voiceOver":false})
    }
}
