//! Owned UI publication probe. Search protocol/aggregation is not implemented by
//! this test adapter; it only captures the ordinary public Retry callback.
use super::*;
use crate::acceptance_script::{Action, run_script};
use tekes_agent_components_ui::{
    SidebarSearchInput, SidebarSearchIntent, SidebarSearchKey, SidebarSearchPhase,
};
use tekes_ui_controls::composition::ButtonContent;
pub(super) struct Probe {
    phase: u8,
    started: Instant,
    retry: Option<ComponentId>,
    row: Option<ComponentId>,
    widget: usize,
    callbacks: usize,
    submissions: usize,
}
impl Probe {
    pub fn new() -> Self {
        run_script(vec![(15000, Action::Close)]);
        Self {
            phase: 0,
            started: Instant::now(),
            retry: None,
            row: None,
            widget: 0,
            callbacks: 0,
            submissions: 0,
        }
    }
    fn key(generation: u64) -> SidebarSearchKey {
        SidebarSearchKey {
            owner: "owned-native-search-source".into(),
            endpoint_id: "fixture".into(),
            connection_epoch: 1,
            generation,
            query: "中文搜索".into(),
        }
    }
    fn failed(revision: u64, generation: u64) -> SidebarSearchInput {
        SidebarSearchInput {
            revision,
            key: Some(Self::key(generation)),
            phase: SidebarSearchPhase::Failed {
                message: "搜索失败：最近会话仍可阅读。".into(),
                has_more: true,
                retryable: true,
            },
        }
    }
    pub fn install(&self, view: &mut ComponentDemo) {
        if self.phase >= 8 {
            return;
        }
        let revision = self.phase as u64 + 1;
        let input = match self.phase {
            0 => SidebarSearchInput {
                revision,
                key: Some(Self::key(1)),
                phase: SidebarSearchPhase::Searching,
            },
            1 | 2 => Self::failed(revision, 1),
            3 => SidebarSearchInput {
                revision,
                key: Some(Self::key(2)),
                phase: SidebarSearchPhase::Searching,
            },
            4 | 5 => SidebarSearchInput {
                revision,
                key: Some(Self::key(2)),
                phase: SidebarSearchPhase::Complete {
                    has_more: self.phase == 4,
                },
            },
            6 => SidebarSearchInput {
                revision,
                key: None,
                phase: SidebarSearchPhase::Idle,
            },
            7 => Self::failed(revision, 3),
            _ => unreachable!(),
        };
        view.set_sidebar_search(&input).unwrap();
    }
    pub fn retry(&mut self, view: &mut ComponentDemo, intent: &SidebarSearchIntent) {
        assert_eq!(self.phase, 2);
        assert_eq!(intent, &SidebarSearchIntent::Retry { key: Self::key(1) });
        assert!(
            view.tree
                .get_mut(view.sidebar_id())
                .unwrap()
                .component_mut::<Sidebar>()
                .unwrap()
                .set_search_retry_pending(&Self::key(1), true)
        );
        self.callbacks += 1;
        self.phase = 3;
    }
    pub fn advance(&mut self, host: &mut Host, window: &Window) {
        assert!(
            self.started.elapsed() < Duration::from_secs(15),
            "search footer deadline phase {}",
            self.phase
        );
        let sidebar = host
            .view
            .tree
            .get(host.view.sidebar_id())
            .unwrap()
            .component::<Sidebar>()
            .unwrap();
        let Some(input) = sidebar
            .displayed_input()
            .filter(|input| input.loaded && !input.rows.is_empty())
        else {
            return;
        };
        if self.phase >= 8 {
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
            0 => {
                assert!(names.iter().any(|name| name == "正在搜索…"));
                let row = sidebar.row_id(&input.rows[0].address).unwrap();
                self.row = Some(row);
                self.widget = host
                    .view
                    .tree
                    .get(row)
                    .unwrap()
                    .component::<ButtonContent>()
                    .unwrap() as *const _ as usize;
                self.submissions = host.view.binding().current_run().unwrap().submissions.len();
                self.phase = 1;
            }
            1 => {
                assert!(
                    names
                        .iter()
                        .any(|name| name.contains("搜索失败") && name.contains("缩小搜索范围"))
                );
                let retry = sidebar.search_retry_id().unwrap();
                self.retry = Some(retry);
                let point = host.view.tree.get(retry).unwrap().frame().center();
                assert!(host.view.tree.get(retry).unwrap().visible().is_some());
                self.phase = 2;
                let mut actions = vec![];
                if let Ok(path) = std::env::var("AGENT_SIDEBAR_SEARCH_CAPTURE") {
                    actions.push((0, Action::Capture { path }));
                }
                actions.push((
                    300,
                    Action::Click {
                        x: point.x as f64,
                        y: point.y as f64,
                    },
                ));
                run_script(actions);
            }
            2 => return, // Wait for the real pointer callback through ComponentDemo.
            3 => {
                assert!(names.iter().any(|name| name == "正在搜索…"));
                assert!(host.view.tree.get(self.retry.unwrap()).is_none());
                assert_eq!(self.callbacks, 1);
                self.phase = 4;
            }
            4 => {
                assert!(
                    names
                        .iter()
                        .any(|name| name == "还有更多匹配结果，请缩小搜索范围。")
                );
                assert!(sidebar.search_retry_id().is_none());
                self.phase = 5;
            }
            5 => {
                assert!(!names.iter().any(|name| name == "会话搜索状态"));
                self.phase = 6;
            }
            6 => {
                assert!(!names.iter().any(|name| name == "会话搜索状态"));
                assert_eq!(
                    host.view
                        .set_sidebar_search(&Self::failed(100, 1))
                        .unwrap_err()
                        .to_string(),
                    "unsupported: retired sidebar search attempt"
                );
                self.phase = 7;
            }
            7 => {
                assert_ne!(sidebar.search_retry_id(), self.retry);
                assert_eq!(
                    host.view
                        .tree
                        .get(self.row.unwrap())
                        .unwrap()
                        .component::<ButtonContent>()
                        .unwrap() as *const _ as usize,
                    self.widget
                );
                assert_eq!(
                    host.view.binding().current_run().unwrap().submissions.len(),
                    self.submissions
                );
                self.phase = 8;
                run_script(vec![(100, Action::Close)]);
            }
            _ => unreachable!(),
        }
        window.request_redraw();
    }
    pub fn receipt(&self) -> serde_json::Value {
        assert_eq!(self.phase, 8);
        assert_eq!(self.callbacks, 1);
        serde_json::json!({"nativeWindow":true,"pointerRetry":true,"originalQueryCaptured":true,"loadingFailureMoreClear":true,"originalInventoryRowsRetained":true,"noSubmission":true,"searchRPC":false,"fullLayoutParity":false,"physicalIme":false,"voiceOver":false})
    }
}
