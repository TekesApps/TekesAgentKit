//! Host fixture for actual Kit folder sections; original SDK controls/events.
use super::*;
use tekes_agent_components_ui::{SidebarFolderIdentity, SidebarFolderKey};
impl Probe {
    fn folders_capture(&mut self, variable: &str) {
        let path = std::env::var(variable).expect("fresh explicit folder capture path");
        assert!(!std::path::Path::new(&path).exists());
        self.results_capture = Some(path.clone().into());
        run_script(vec![(100, Action::Capture { path })]);
    }
    pub(super) fn advance_folders(&mut self, host: &mut Host, window: &Window) {
        if self.phase == 50
            && self
                .clear_after
                .is_none_or(|time| time.elapsed() > Duration::from_secs(2))
        {
            let source = host.sidebar.snapshot();
            let view = host
                .view
                .tree
                .get(host.view.sidebar_id())
                .unwrap()
                .component::<Sidebar>()
                .unwrap();
            eprintln!(
                "FOLDER_PREPARE_WAIT active={} stored={:?} shown={:?} rows={:?}",
                window.is_active(),
                host.sidebar.preferences(),
                view.preferences(),
                source
                    .rows
                    .iter()
                    .map(|row| (&row.address.session_id, &row.folder_path))
                    .collect::<Vec<_>>()
            );
            self.clear_after = Some(Instant::now());
        }
        if !window.is_active() {
            run_script(vec![(50, Action::WakeRedraw)]);
            return;
        }
        let key = SidebarFolderKey {
            endpoint_id: "fixture".into(),
            identity: SidebarFolderIdentity::Path("/fixture/中文项目".into()),
        };
        let inventory = host.sidebar.snapshot();
        let sidebar = host
            .view
            .tree
            .get(host.view.sidebar_id())
            .unwrap()
            .component::<Sidebar>()
            .unwrap();
        let preferences = host.sidebar.preferences().unwrap();
        if sidebar.preferences() != &preferences
            || inventory
                .rows
                .iter()
                .find(|row| row.address.session_id == "a")
                .unwrap()
                .folder_path
                .as_deref()
                != Some("/fixture/中文项目")
            || sidebar.folder_header_id(&key).is_none() && self.phase < 55
        {
            run_script(vec![(50, Action::WakeRedraw)]);
            return;
        }
        let a = self.row.unwrap();
        assert_eq!(
            host.view
                .tree
                .get(a)
                .unwrap()
                .component::<ButtonContent>()
                .unwrap() as *const _ as usize,
            self.widget
        );
        assert_eq!(host.view.selected(), 0);
        let run = host.view.binding().current_run().unwrap();
        assert_eq!(run.draft.text, "搜索期间保留的中文草稿");
        assert!(run.submissions.is_empty());
        let b = inventory
            .rows
            .iter()
            .find(|row| row.address.session_id == "b")
            .unwrap();
        let pin = inventory
            .rows
            .iter()
            .find(|row| row.address.session_id == "late-search")
            .unwrap();
        if pin.pin.as_ref().is_none_or(|value| !value.pinned) {
            run_script(vec![(50, Action::WakeRedraw)]);
            return;
        }
        let pin_matches_filter = sidebar
            .displayed_input()
            .unwrap()
            .folder_filter
            .as_ref()
            .is_none_or(|filter| *filter == pin.folder_key());
        assert_eq!(sidebar.row_id(&pin.address).is_some(), pin_matches_filter);
        if let Some(pin_id) = sidebar.row_id(&pin.address) {
            assert!(
                host.view
                    .tree
                    .semantics()
                    .iter()
                    .any(|node| node.id == pin_id)
            );
        }
        match self.phase {
            50 => {
                if !host
                    .view
                    .tree
                    .semantics()
                    .iter()
                    .any(|node| node.semantics.name == "其他项目")
                {
                    run_script(vec![(50, Action::WakeRedraw)]);
                    return;
                }
                let header = sidebar.folder_header_id(&key).unwrap();
                assert!(
                    host.view.tree.get(a).unwrap().frame().min_x()
                        > host.view.tree.get(header).unwrap().frame().min_x(),
                    "folder leaf uses the library-inset body"
                );
                self.folders_capture("AGENT_SIDEBAR_FOLDERS_CAPTURE");
                self.phase = 51;
            }
            51 if self
                .results_capture
                .as_ref()
                .is_some_and(|path| path.is_file()) =>
            {
                let point = host
                    .view
                    .tree
                    .get(sidebar.folder_header_id(&key).unwrap())
                    .unwrap()
                    .frame()
                    .center();
                self.phase = 52;
                run_script(vec![(
                    0,
                    Action::Click {
                        x: point.x as f64,
                        y: point.y as f64,
                    },
                )]);
            }
            52 if preferences.preferences.collapsed_folders.contains(&key) => {
                assert!(!host.view.tree.semantics().iter().any(|node| node.id == a));
                assert!(sidebar.row_id(&b.address).is_some());
                self.folders_capture("AGENT_SIDEBAR_FOLDERS_COLLAPSED_CAPTURE");
                self.phase = 53;
            }
            53 if self
                .results_capture
                .as_ref()
                .is_some_and(|path| path.is_file()) =>
            {
                host.view
                    .set_sidebar_folder_filter(Some(key.clone()))
                    .unwrap();
                self.phase = 54;
            }
            54 => {
                assert!(sidebar.row_id(&b.address).is_none());
                assert_eq!(
                    sidebar.row_id(&pin.address).is_some(),
                    pin.folder_key() == key
                );
                assert!(
                    !host
                        .view
                        .tree
                        .semantics()
                        .iter()
                        .any(|node| node.semantics.name == "其他项目")
                );
                self.folders_capture("AGENT_SIDEBAR_FOLDERS_FILTERED_CAPTURE");
                self.phase = 55;
            }
            55 if self
                .results_capture
                .as_ref()
                .is_some_and(|path| path.is_file()) =>
            {
                host.view.set_sidebar_folder_filter(None).unwrap();
                let expected = host.sidebar.preferences().unwrap();
                let mut value = expected.preferences.clone();
                value.group_by_folder = false;
                host.sidebar
                    .set_preferences(&tekes_agent_components_ui::SidebarPreferenceChange {
                        expected,
                        value,
                    })
                    .unwrap();
                self.phase = 56;
            }
            56 if !preferences.preferences.group_by_folder => {
                assert!(host.view.tree.semantics().iter().any(|node| node.id == a));
                assert!(sidebar.row_id(&b.address).is_some());
                assert!(preferences.preferences.collapsed_folders.contains(&key));
                eprintln!(
                    "NATIVE_AGENT_SIDEBAR_FOLDERS_RECEIPT {}",
                    serde_json::json!({"nativeWindow":true,"windowActive":true,"publicInventoryFolders":true,"groupedLabels":true,"originalPointerDisclosure":true,"durableCollapsedFolder":true,"originalButtonRetained":true,"libraryInsetBody":true,"filterAppliesToPinnedRows":true,"ungroupRestoresRows":true,"draftRetained":true,"selectionRetained":true,"noSubmission":true,"realHarness":false,"physicalIme":false,"voiceOver":false})
                );
                if std::env::var_os("AGENT_SIDEBAR_NAVIGATION_CAPTURE").is_some() {
                    let expected = host.sidebar.preferences().unwrap();
                    let mut value = expected.preferences.clone();
                    value.group_by_folder = true;
                    host.sidebar
                        .set_preferences(&tekes_agent_components_ui::SidebarPreferenceChange {
                            expected,
                            value,
                        })
                        .unwrap();
                    self.phase = 60;
                    run_script(vec![(50, Action::WakeRedraw)]);
                    return;
                }
                self.phase = 5;
                run_script(vec![(100, Action::Close)]);
                return;
            }
            60 if preferences.preferences.group_by_folder => {
                assert!(!host.view.tree.semantics().iter().any(|node| node.id == a));
                self.navigation_preferences = Some(preferences.clone());
                let point = host
                    .view
                    .tree
                    .get(tekes_agent_ui_demo::component_demo::SELECT_A)
                    .unwrap()
                    .frame()
                    .center();
                self.phase = 61;
                run_script(vec![(
                    0,
                    Action::Click {
                        x: point.x as f64,
                        y: point.y as f64,
                    },
                )]);
            }
            61 if host.view.tree.semantics().iter().any(|node| node.id == a) => {
                assert_eq!(&preferences, self.navigation_preferences.as_ref().unwrap());
                assert!(preferences.preferences.collapsed_folders.contains(&key));
                assert!(sidebar.pending_navigation().is_none());
                self.folders_capture("AGENT_SIDEBAR_NAVIGATION_CAPTURE");
                self.phase = 62;
            }
            62 if self
                .results_capture
                .as_ref()
                .is_some_and(|path| path.is_file()) =>
            {
                let point = host
                    .view
                    .tree
                    .get(sidebar.folder_header_id(&key).unwrap())
                    .unwrap()
                    .frame()
                    .center();
                self.phase = 63;
                run_script(vec![(
                    0,
                    Action::Click {
                        x: point.x as f64,
                        y: point.y as f64,
                    },
                )]);
            }
            63 if !host.view.tree.semantics().iter().any(|node| node.id == a)
                && sidebar.preferences_current(&preferences, true) =>
            {
                assert_eq!(&preferences, self.navigation_preferences.as_ref().unwrap());
                self.folders_capture("AGENT_SIDEBAR_NAVIGATION_CLOSED_CAPTURE");
                self.phase = 64;
            }
            64 if self
                .results_capture
                .as_ref()
                .is_some_and(|path| path.is_file()) =>
            {
                let mut summary = host
                    .runtime
                    .inspect(|state| state.sessions["a"].summary.clone())
                    .unwrap();
                let sequence = summary["projections"]["asOfSeq"].as_u64().unwrap_or(0) + 100;
                summary["projections"]["asOfSeq"] = serde_json::json!(sequence);
                summary["projections"]["values"]["title"] = serde_json::json!("导航后普通刷新");
                self.fixture.publish_inventory_summary(summary).unwrap();
                self.phase = 65;
            }
            65 if sidebar
                .displayed_input()
                .unwrap()
                .rows
                .iter()
                .any(|row| row.address.session_id == "a" && row.title == "导航后普通刷新") =>
            {
                assert!(!host.view.tree.semantics().iter().any(|node| node.id == a));
                assert_eq!(&preferences, self.navigation_preferences.as_ref().unwrap());
                let point = host
                    .view
                    .tree
                    .get(tekes_agent_ui_demo::component_demo::SELECT_A)
                    .unwrap()
                    .frame()
                    .center();
                self.phase = 66;
                run_script(vec![(
                    0,
                    Action::Click {
                        x: point.x as f64,
                        y: point.y as f64,
                    },
                )]);
            }
            66 if host.view.tree.semantics().iter().any(|node| node.id == a) => {
                assert_eq!(&preferences, self.navigation_preferences.as_ref().unwrap());
                assert_eq!(
                    host.view
                        .tree
                        .get(tekes_agent_ui_demo::component_demo::SELECT_A)
                        .unwrap()
                        .component::<ButtonContent>()
                        .unwrap()
                        .activations,
                    2
                );
                self.folders_capture("AGENT_SIDEBAR_NAVIGATION_REOPENED_CAPTURE");
                self.phase = 67;
            }
            67 if self
                .results_capture
                .as_ref()
                .is_some_and(|path| path.is_file()) =>
            {
                assert_eq!(&preferences, self.navigation_preferences.as_ref().unwrap());
                eprintln!(
                    "NATIVE_AGENT_SIDEBAR_NAVIGATION_RECEIPT {}",
                    serde_json::json!({"nativeWindow":true,"windowActive":true,"originalNavigationClicks":2,"navigationRevealsSelectedFolder":true,"durablePreferencesUnchanged":true,"manualNoopCollapseAcknowledged":true,"inventoryRefreshDoesNotReopen":true,"laterExplicitNavigationReopens":true,"originalLeafRetained":true,"draftRetained":true,"noSubmission":true,"realHarness":false,"physicalIme":false,"voiceOver":false})
                );
                self.phase = 5;
                run_script(vec![(100, Action::Close)]);
                return;
            }
            _ => {}
        }
        run_script(vec![(50, Action::WakeRedraw)]);
    }
}
