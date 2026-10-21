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
        let pin_id = sidebar.row_id(&pin.address).unwrap();
        assert!(
            host.view
                .tree
                .semantics()
                .iter()
                .any(|node| node.id == pin_id)
        );
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
                assert!(sidebar.row_id(&pin.address).is_some());
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
                    serde_json::json!({"nativeWindow":true,"windowActive":true,"publicInventoryFolders":true,"groupedLabels":true,"originalPointerDisclosure":true,"durableCollapsedFolder":true,"originalButtonRetained":true,"libraryInsetBody":true,"filterKeepsGlobalPin":true,"ungroupRestoresRows":true,"draftRetained":true,"selectionRetained":true,"noSubmission":true,"realHarness":false,"physicalIme":false,"voiceOver":false})
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
