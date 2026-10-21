//! Actual shared search controller -> public SEP read -> filtered Sidebar rows/footer.
//! The backend is the owned in-process fixture, not a real harness search index.
use super::*;
#[path = "native_sidebar_folders_selftest.rs"]
mod folders;
#[path = "native_sidebar_pin_query_selftest.rs"]
mod pin_query;
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
    clear_after: Option<Instant>,
    results_capture: Option<std::path::PathBuf>,
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
            clear_after: None,
            results_capture: None,
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
        self.fixture
            .install_unannounced_session("late-search", "搜索刷新恢复的会话");
        self.fixture.set_search_result(
            "中文检索",
            serde_json::json!({"items":[
            {"sessionId":"b","snippet":"命中 B"},
            {"sessionId":"late-search","snippet":"尚无库存通知的正文命中"},
            {"sessionId":"a","snippet":"命中 A"}],"hasMore":true}),
        );
        self.phase = 2;
    }
    pub fn advance(&mut self, host: &mut Host, window: &Window) {
        assert!(
            self.started.elapsed() < Duration::from_secs(15),
            "query probe deadline phase {}",
            self.phase
        );
        if self.phase >= 50 {
            self.advance_folders(host, window);
            return;
        }
        let source = host.sidebar_query.as_ref().unwrap();
        let model = source.snapshot();
        let sidebar = host
            .view
            .tree
            .get(host.view.sidebar_id())
            .unwrap()
            .component::<Sidebar>()
            .unwrap();
        if self.phase >= 40 {
            self.advance_pin_query(host);
            return;
        }
        // CANDIDATE_SIDEBAR_HELP_NATIVE_ADVANCE
        if self.phase == 5 {
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
                assert!(sidebar.displayed_input().unwrap().rows.is_empty());
                assert!(
                    host.sidebar
                        .snapshot()
                        .rows
                        .iter()
                        .all(|row| sidebar.row_id(&row.address).is_none())
                );
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
                let late = tekes_agent_contract::SessionAddress {
                    endpoint_id: "fixture".into(),
                    session_id: "late-search".into(),
                };
                if sidebar.row_id(&late).is_none() {
                    window.request_redraw();
                    return;
                }
                assert_eq!(self.fixture.search_calls(), 2);
                assert_eq!(self.fixture.inventory_reads(), 2);
                assert_eq!(
                    model.input.key.as_ref().unwrap().connection_epoch,
                    self.key.as_ref().unwrap().connection_epoch
                );
                assert_eq!(
                    model
                        .hits
                        .iter()
                        .map(|hit| hit.address.session_id.as_str())
                        .collect::<Vec<_>>(),
                    vec!["b", "late-search", "a"]
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
                let inventory = host.sidebar.snapshot();
                let expected: Vec<_> = inventory
                    .rows
                    .iter()
                    .filter(|row| model.hits.iter().any(|hit| hit.address == row.address))
                    .map(|row| row.address.clone())
                    .collect();
                assert_eq!(
                    sidebar
                        .displayed_input()
                        .unwrap()
                        .rows
                        .iter()
                        .map(|row| row.address.clone())
                        .collect::<Vec<_>>(),
                    expected
                );
                assert_eq!(expected.len(), 3);
                let address = tekes_agent_contract::SessionAddress {
                    endpoint_id: "fixture".into(),
                    session_id: "a".into(),
                };
                assert_eq!(host.view.sidebar_search_snippet(&address), Some("命中 A"));
                let row = sidebar.row_id(&address).unwrap();
                self.row = Some(row);
                self.widget = host
                    .view
                    .tree
                    .get(row)
                    .unwrap()
                    .component::<ButtonContent>()
                    .unwrap() as *const _ as usize;
                // CANDIDATE_SIDEBAR_HELP_NATIVE_BEGIN
                if let Ok(path) = std::env::var("AGENT_SIDEBAR_QUERY_RESULTS_CAPTURE") {
                    let file = std::path::PathBuf::from(&path);
                    assert!(!file.exists(), "use a fresh results screenshot path");
                    self.results_capture = Some(file);
                    run_script(vec![(100, Action::Capture { path })]);
                }
                self.clear_after = Some(Instant::now());
                self.phase = 6;
                window.request_redraw();
            }
            6 => {
                if self.clear_after.unwrap().elapsed() >= Duration::from_millis(500)
                    && self
                        .results_capture
                        .as_ref()
                        .is_none_or(|path| path.is_file())
                {
                    source.set_query("").unwrap();
                    self.phase = 4;
                }
                // Waiting for a file is not a display change. Yield the main
                // queue instead of recursively requesting another draw; use the
                // SDK's existing bounded test wake for the next observation.
                run_script(vec![(50, Action::WakeRedraw)]);
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
                assert_eq!(
                    sidebar.displayed_input().unwrap().rows,
                    host.sidebar.snapshot().rows
                );
                let address = tekes_agent_contract::SessionAddress {
                    endpoint_id: "fixture".into(),
                    session_id: "a".into(),
                };
                assert_eq!(host.view.sidebar_search_snippet(&address), None);
                let run = host.view.binding().current_run().unwrap();
                assert_eq!(run.draft.text, "搜索期间保留的中文草稿");
                assert!(run.submissions.is_empty());
                assert_eq!(self.fixture.search_calls(), 2);
                if std::env::var_os("AGENT_SIDEBAR_FOLDERS_CAPTURE").is_some() {
                    for (id, path) in [("a", "/fixture/中文项目"), ("b", "/fixture/其他项目")]
                    {
                        let mut summary = host
                            .runtime
                            .inspect(|state| state.sessions[id].summary.clone())
                            .unwrap();
                        summary["cwd"] = serde_json::json!(path);
                        self.fixture.publish_inventory_summary(summary).unwrap();
                    }
                    let inventory = host.sidebar.snapshot();
                    let row = inventory
                        .rows
                        .iter()
                        .find(|row| row.address.session_id == "late-search")
                        .unwrap();
                    let action = row
                        .actions
                        .items
                        .iter()
                        .find(|action| action.id == "pin")
                        .unwrap();
                    host.executor
                        .block_on(host.sidebar.perform(
                            &row.address,
                            action,
                            None,
                            "native-folder-pin",
                        ))
                        .unwrap();
                    let expected = host.sidebar.preferences().unwrap();
                    let mut value = expected.preferences.clone();
                    value.group_by_folder = true;
                    host.sidebar
                        .set_preferences(&tekes_agent_components_ui::SidebarPreferenceChange {
                            expected,
                            value,
                        })
                        .unwrap();
                    self.phase = 50;
                    run_script(vec![(50, Action::WakeRedraw)]);
                    return;
                }
                if std::env::var_os("AGENT_SIDEBAR_PINNED_CAPTURE").is_some()
                    || std::env::var_os("AGENT_SIDEBAR_PIN_QUERY_CAPTURE").is_some()
                {
                    for id in ["a", "b"] {
                        let inventory = host.sidebar.snapshot();
                        let row = inventory
                            .rows
                            .iter()
                            .find(|row| row.address.session_id == id)
                            .unwrap();
                        let action = row
                            .actions
                            .items
                            .iter()
                            .find(|action| action.id == "pin")
                            .unwrap();
                        host.executor
                            .block_on(host.sidebar.perform(
                                &row.address,
                                action,
                                None,
                                &format!("native-layout-pin-{id}"),
                            ))
                            .unwrap();
                    }
                    self.phase = 8;
                    window.request_redraw();
                    return;
                }
                if std::env::var_os("AGENT_SIDEBAR_INVENTORY_CAPTURE").is_some() {
                    self.fixture.publish_inventory_summary(serde_json::json!({
                        "sessionId":"a", "updatedAt":42000.5, "running":false, "blank":false,
                        "projections":{"asOfSeq":1000,"values":{"title":"投影中文标题 · 最新活动"}}
                    })).unwrap();
                    self.phase = 7;
                    window.request_redraw();
                    return;
                }
                self.phase = 5;
                let mut actions = vec![];
                if let Ok(path) = std::env::var("AGENT_SIDEBAR_QUERY_REFRESH_CAPTURE") {
                    actions.push((300, Action::Capture { path }));
                }
                actions.push((500, Action::Close));
                run_script(actions);
            }
            8 => {
                if sidebar
                    .displayed_input()
                    .unwrap()
                    .rows
                    .iter()
                    .filter(|row| row.pin.as_ref().is_some_and(|pin| pin.pinned))
                    .count()
                    != 2
                {
                    run_script(vec![(50, Action::WakeRedraw)]);
                    return;
                }
                if std::env::var_os("AGENT_SIDEBAR_PIN_QUERY_CAPTURE").is_some() {
                    self.fixture.set_search_result("pin-query", serde_json::json!({"items":[{"sessionId":"b","snippet":"只有 b 命中，a 继续置顶"}],"hasMore":false}));
                    self.fixture.set_search_result(
                        "pin-empty",
                        serde_json::json!({"items":[],"hasMore":false}),
                    );
                    self.phase = 40;
                    source.set_query("pin-query").unwrap();
                    run_script(vec![(50, Action::WakeRedraw)]);
                    return;
                }
                let header = sidebar.pinned_header_id().unwrap();
                assert!(
                    host.sidebar
                        .preferences()
                        .unwrap()
                        .preferences
                        .pinned_expanded
                );
                let point = host.view.tree.get(header).unwrap().frame().center();
                self.phase = 9;
                run_script(vec![(
                    200,
                    Action::Click {
                        x: point.x as f64,
                        y: point.y as f64,
                    },
                )]);
            }
            9 => {
                let preferences = host.sidebar.preferences().unwrap();
                if preferences.preferences.pinned_expanded || sidebar.preferences() != &preferences
                {
                    run_script(vec![(50, Action::WakeRedraw)]);
                    return;
                }
                assert_eq!(preferences.revision, 2);
                let row = self.row.unwrap();
                assert!(!host.view.tree.semantics().iter().any(|node| node.id == row));
                assert_eq!(
                    host.view
                        .tree
                        .get(row)
                        .unwrap()
                        .component::<ButtonContent>()
                        .unwrap() as *const _ as usize,
                    self.widget
                );
                let run = host.view.binding().current_run().unwrap();
                assert_eq!(run.draft.text, "搜索期间保留的中文草稿");
                assert!(run.submissions.is_empty());
                let header = sidebar.pinned_header_id().unwrap();
                assert_eq!(
                    host.view
                        .tree
                        .semantics()
                        .iter()
                        .find(|node| node.id == header)
                        .unwrap()
                        .semantics
                        .expanded,
                    Some(false)
                );
                let path = std::env::var("AGENT_SIDEBAR_PINNED_CAPTURE").unwrap();
                assert!(
                    !std::path::Path::new(&path).exists(),
                    "use a fresh pinned screenshot path"
                );
                eprintln!(
                    "NATIVE_AGENT_SIDEBAR_PINNED_RECEIPT {}",
                    serde_json::json!({"originalPointerDisclosure":true,"durableRevision":preferences.revision,"originalButtonRetained":true,"collapsedRowsInaccessible":true,"draftRetained":true,"selectionRetained":true,"noSubmission":true,"ownedFixture":true,"physicalIme":false,"nativePinReorder":false})
                );
                self.phase = 5;
                run_script(vec![(300, Action::Capture { path }), (700, Action::Close)]);
            }
            7 => {
                let shown = sidebar.displayed_input().unwrap();
                if shown.rows.first().is_none_or(|row| {
                    row.address.session_id != "a" || row.title != "投影中文标题 · 最新活动"
                }) {
                    window.request_redraw();
                    return;
                }
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
                let path = std::env::var("AGENT_SIDEBAR_INVENTORY_CAPTURE").unwrap();
                assert!(
                    !std::path::Path::new(&path).exists(),
                    "use a fresh inventory screenshot path"
                );
                eprintln!(
                    "NATIVE_AGENT_SIDEBAR_INVENTORY_RECEIPT {}",
                    serde_json::json!({
                        "canonicalTitle":true,"recentActivityOrder":true,"retainedOriginalButton":true,
                        "draftRetained":true,"selectionRetained":true,"noSubmission":true,
                        "ownedFixture":true,"physicalIme":false,"nativeRunningFreeze":false
                    })
                );
                self.phase = 5;
                run_script(vec![(300, Action::Capture { path }), (700, Action::Close)]);
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
        serde_json::json!({"nativeWindow":true,"sharedPresentationController":true,"publicSearchRPC":true,"searchReads":self.fixture.search_calls(),"unknownHitRefresh":true,"inventoryReads":self.fixture.inventory_reads(),"sameConnectionEpoch":true,"pointerRetry":true,"originalQueryCaptured":true,"retiredRetryRejected":true,"clearRevokesResult":true,"retainedVisibleControl":true,"resultMembership":true,"inventorySortRetained":true,"queryScopedSnippet":true,"draftRetained":true,"noSubmission":true,"ownedFixture":true,"realHarness":false,"physicalIme":false,"voiceOver":false,"fullSidebarParity":false})
    }
}
