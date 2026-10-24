//! Business fixture only: public pin actions and search publications, original
//! Sidebar controls and SDK capture/wake. No generic input/layout implementation.
use super::*;
impl Probe {
    fn pin_query_invariants(
        &self,
        host: &Host,
        search: &tekes_agent_presentation::SidebarSearchPublication,
    ) {
        let sidebar = host
            .view
            .tree
            .get(host.view.sidebar_id())
            .unwrap()
            .component::<Sidebar>()
            .unwrap();
        assert!(sidebar.displayed_input().unwrap().loaded);
        let pins: Vec<_> = sidebar
            .displayed_input()
            .unwrap()
            .rows
            .iter()
            .filter(|row| row.pin.as_ref().is_some_and(|pin| pin.pinned))
            .collect();
        let expected: &[&str] = match &search.input.phase {
            SidebarSearchPhase::Idle => &["a", "b"],
            SidebarSearchPhase::Complete { .. } if search.query == "pin-query" => &["b"],
            _ => &[],
        };
        assert_eq!(pins.len(), expected.len());
        for id in expected {
            assert_eq!(
                pins.iter()
                    .filter(|row| row.address.session_id == *id)
                    .count(),
                1
            );
        }
        // Visibility is query membership; pin preferences remain authoritative
        // in the full inventory even while unmatched controls are unmounted.
        let inventory = host.sidebar.snapshot();
        for id in ["a", "b"] {
            assert!(inventory.rows.iter().any(|row| row.address.session_id == id
                && row.pin.as_ref().is_some_and(|pin| pin.pinned)));
        }
        if search.input.key.is_some() {
            assert!(
                !host
                    .view
                    .tree
                    .semantics()
                    .iter()
                    .any(|node| node.id == self.row.unwrap()),
                "unmatched pin control must be unmounted"
            );
        }
        assert_eq!(host.view.selected(), 0);
        let run = host.view.binding().current_run().unwrap();
        assert_eq!(run.draft.text, "搜索期间保留的中文草稿");
        assert!(run.submissions.is_empty());
    }
    fn pin_query_capture(&mut self, variable: &str) {
        let path = std::env::var(variable).expect("explicit screenshot destination");
        assert!(
            !std::path::Path::new(&path).exists(),
            "fresh capture path required"
        );
        self.results_capture = Some(path.clone().into());
        run_script(vec![(100, Action::Capture { path })]);
    }
    pub(super) fn advance_pin_query(&mut self, host: &Host) {
        let source = host.sidebar_query.as_ref().unwrap();
        let search = source.snapshot();
        let sidebar = host
            .view
            .tree
            .get(host.view.sidebar_id())
            .unwrap()
            .component::<Sidebar>()
            .unwrap();
        if sidebar.displayed_search() != Some(&search.input) {
            run_script(vec![(50, Action::WakeRedraw)]);
            return;
        }
        self.pin_query_invariants(host, &search);
        match self.phase {
            40 if search.input.phase == SidebarSearchPhase::Searching => {
                assert!(
                    !sidebar
                        .displayed_input()
                        .unwrap()
                        .search_help
                        .as_ref()
                        .unwrap()
                        .results_ready
                );
                assert!(sidebar.displayed_input().unwrap().rows.is_empty());
                self.phase = 41;
            }
            41 if matches!(search.input.phase, SidebarSearchPhase::Complete { .. }) => {
                assert_eq!(sidebar.displayed_input().unwrap().rows.len(), 1);
                assert_eq!(
                    host.view
                        .sidebar_search_snippet(&tekes_agent_contract::SessionAddress {
                            endpoint_id: "fixture".into(),
                            session_id: "a".into()
                        }),
                    None
                );
                assert_eq!(
                    host.view
                        .sidebar_search_snippet(&tekes_agent_contract::SessionAddress {
                            endpoint_id: "fixture".into(),
                            session_id: "b".into()
                        }),
                    Some("只有 b 命中，a 的置顶偏好保留但不显示")
                );
                self.pin_query_capture("AGENT_SIDEBAR_PIN_QUERY_CAPTURE");
                self.phase = 42;
            }
            42 if self.results_capture.as_ref().is_some_and(|p| p.is_file()) => {
                source.set_query("pin-empty").unwrap();
                self.phase = 43;
            }
            43 if search.query == "pin-empty"
                && matches!(search.input.phase, SidebarSearchPhase::Complete { .. }) =>
            {
                assert!(sidebar.displayed_input().unwrap().rows.is_empty());
                assert!(
                    host.view
                        .tree
                        .semantics()
                        .iter()
                        .any(|node| node.semantics.name == "没有匹配的会话")
                );
                self.pin_query_capture("AGENT_SIDEBAR_PIN_EMPTY_CAPTURE");
                self.phase = 44;
            }
            44 if self.results_capture.as_ref().is_some_and(|p| p.is_file()) => {
                source.set_query("").unwrap();
                self.phase = 45;
            }
            45 if search.input.phase == SidebarSearchPhase::Idle => {
                assert_eq!(
                    sidebar.displayed_input().unwrap().rows,
                    host.sidebar.snapshot().rows
                );
                assert_eq!(self.fixture.search_calls(), 4);
                eprintln!(
                    "NATIVE_AGENT_SIDEBAR_PIN_QUERY_RECEIPT {}",
                    serde_json::json!({"nativeWindow":true,"ownedFixture":true,"realHarness":false,"pinsFilteredDuringSearching":true,"unmatchedPinHidden":true,"matchedPinNotDuplicated":true,"emptyQueryResultsHidePins":true,"pinPreferencesRetained":true,"clearRestoresInventory":true,"hiddenRowsNotMounted":true,"draftRetained":true,"selectionRetained":true,"noSubmission":true,"physicalIme":false,"voiceOver":false})
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
