//! Actual shared search controller -> public SEP read -> retained Sidebar footer.
//! The backend is the owned in-process fixture, not a real harness search index.
use super::*;
use tekes_agent_components_ui::{SidebarSearchIntent, SidebarSearchKey, SidebarSearchPhase};
use tekes_ui_controls::composition::ButtonContent;
use tekes_ui_platform_macos::selftest::{Action, run_script};
pub(super) struct Probe {
    fixture: tekes_agent_ui_demo::fixture::Fixture,
    started: Instant,
    phase: u8,
    key: Option<SidebarSearchKey>,
    retry: Option<ComponentId>,
    row: Option<ComponentId>,
    widget: usize,
    callbacks: usize,
}
impl Probe {
    pub fn new(fixture: tekes_agent_ui_demo::fixture::Fixture) -> Self {
        run_script(vec![(15000, Action::Close)]);
        Self {
            fixture,
            started: Instant::now(),
            phase: 0,
            key: None,
            retry: None,
            row: None,
            widget: 0,
            callbacks: 0,
        }
    }
    pub fn prepare(view: &ComponentDemo) {
        let binding = view.binding();
        binding
            .edit_text(
                &tekes_agent_components_ui::ComposerDestination::run(binding.scope().clone()),
                "搜索期间保留的中文草稿".into(),
            )
            .unwrap();
    }
    pub fn before_retry(&mut self, intent: &SidebarSearchIntent) {
        assert_eq!(self.phase, 1);
        assert_eq!(
            intent,
            &SidebarSearchIntent::Retry {
                key: self.key.clone().unwrap()
            }
        );
        self.callbacks += 1;
        self.fixture.set_search_failure(false);
        self.phase = 2;
    }
    pub fn advance(&mut self, host: &Host, window: &Window) {
        assert!(
            self.started.elapsed() < Duration::from_secs(15),
            "query probe deadline phase {}",
            self.phase
        );
        let source = host.sidebar_query.as_ref().unwrap();
        let model = source.snapshot();
        let sidebar = host
            .view
            .tree
            .get(host.view.sidebar_id())
            .unwrap()
            .component::<Sidebar>()
            .unwrap();
        if self.phase >= 5 {
            return;
        }
        if sidebar.displayed_search() != Some(&model.input) {
            window.request_redraw();
            return;
        }
        match self.phase {
            0 if matches!(
                model.input.phase,
                SidebarSearchPhase::Failed {
                    retryable: true,
                    ..
                }
            ) =>
            {
                assert!(model.hits.is_empty() && model.result.is_none());
                assert_eq!(self.fixture.search_calls(), 1);
                let row = host.sidebar.snapshot().rows[0].address.clone();
                let row = sidebar.row_id(&row).unwrap();
                self.row = Some(row);
                self.widget = host
                    .view
                    .tree
                    .get(row)
                    .unwrap()
                    .component::<ButtonContent>()
                    .unwrap() as *const _ as usize;
                self.key = model.input.key.clone();
                let retry = sidebar.search_retry_id().unwrap();
                self.retry = Some(retry);
                assert!(host.view.tree.get(retry).unwrap().visible().is_some());
                let point = host.view.tree.get(retry).unwrap().frame().center();
                self.phase = 1;
                let mut actions = vec![];
                if let Ok(path) = std::env::var("AGENT_SIDEBAR_QUERY_CAPTURE") {
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
            1 => {}
            2 if model.input.key != self.key
                && matches!(
                    model.input.phase,
                    SidebarSearchPhase::Searching | SidebarSearchPhase::Complete { .. }
                ) =>
            {
                assert_ne!(model.input.key, self.key);
                assert!(host.view.tree.get(self.retry.unwrap()).is_none());
                assert_eq!(self.callbacks, 1);
                self.phase = 3;
                window.request_redraw();
            }
            3 if matches!(
                model.input.phase,
                SidebarSearchPhase::Complete { has_more: true }
            ) =>
            {
                assert_eq!(self.fixture.search_calls(), 2);
                assert_eq!(
                    model
                        .hits
                        .iter()
                        .map(|hit| hit.address.session_id.as_str())
                        .collect::<Vec<_>>(),
                    vec!["b", "a"]
                );
                assert!(
                    model
                        .hits
                        .iter()
                        .all(|hit| hit.address.endpoint_id == "fixture")
                );
                assert!(model.unresolved.is_empty());
                assert!(
                    source
                        .retry(&SidebarSearchIntent::Retry {
                            key: self.key.clone().unwrap()
                        })
                        .is_err()
                );
                source.set_query("").unwrap();
                self.phase = 4;
            }
            4 if model.input.phase == SidebarSearchPhase::Idle => {
                assert!(
                    model.input.key.is_none() && model.hits.is_empty() && model.result.is_none()
                );
                assert!(sidebar.displayed_search().unwrap().key.is_none());
                assert!(
                    !host
                        .view
                        .tree
                        .semantics()
                        .iter()
                        .any(|node| node.semantics.name == "会话搜索状态")
                );
                assert_eq!(
                    host.view
                        .tree
                        .get(self.row.unwrap())
                        .unwrap()
                        .component::<ButtonContent>()
                        .unwrap() as *const _ as usize,
                    self.widget
                );
                let run = host.view.binding().current_run().unwrap();
                assert_eq!(run.draft.text, "搜索期间保留的中文草稿");
                assert!(run.submissions.is_empty());
                assert_eq!(self.fixture.search_calls(), 2);
                self.phase = 5;
                run_script(vec![(100, Action::Close)]);
            }
            _ => {}
        }
        if self.phase == 4 {
            window.request_redraw();
        }
    }
    pub fn receipt(&self) -> serde_json::Value {
        assert_eq!(self.phase, 5);
        assert_eq!(self.callbacks, 1);
        serde_json::json!({"nativeWindow":true,"sharedPresentationController":true,"publicSearchRPC":true,"searchReads":self.fixture.search_calls(),"pointerRetry":true,"originalQueryCaptured":true,"retiredRetryRejected":true,"clearRevokesResult":true,"inventoryControlsRetained":true,"draftRetained":true,"noSubmission":true,"ownedFixture":true,"realHarness":false,"physicalIme":false,"voiceOver":false,"fullSidebarParity":false})
    }
}
