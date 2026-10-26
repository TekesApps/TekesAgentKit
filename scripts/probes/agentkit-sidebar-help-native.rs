// Candidate-only native fixture on the real Sidebar. All events go through the
// original SDK AppKit selftest entry; no tooltip timing or routing is copied.
impl Probe {
    pub(super) fn help_observe_input(&mut self, event: &tekes_ui_controls::composition::ContentEvent) {
        use tekes_ui_controls::composition::{ContentEvent, PointerPhase};
        if self.phase == 22 && matches!(event, ContentEvent::TextCommand(command) if command == "cancelOperation:") {
            self.help_cancel_observed = true;
        }
        if matches!(self.phase, 15 | 18)
            && let ContentEvent::Pointer(pointer) = event
            && pointer.is_primary()
            && matches!(pointer.phase, PointerPhase::Down | PointerPhase::Up)
        {
            self.help_pointer_events += 1;
        }
    }
    fn help_editor_value(host: &Host) -> String {
        let input = &host.view.editor().input;
        format!("{:?}", (&input.text, &input.selection(), &input.marked))
    }
    fn help_editor_state(host: &Host) -> String {
        let input = &host.view.editor().input;
        format!("{:?}", (&input.text, &input.selection(), &input.marked, input.external_baseline()))
    }
    fn help_wake() { run_script(vec![(50, Action::WakeRedraw)]); }
    fn help_capture(&mut self, variable: &str) {
        let path = std::env::var(variable).expect("explicit fresh screenshot path");
        assert!(!std::path::Path::new(&path).exists());
        self.results_capture = Some(path.clone().into());
        self.clear_after = Some(Instant::now());
        run_script(vec![(300, Action::Capture { path })]);
    }
    fn help_captured(&self) -> bool {
        self.results_capture.as_ref().is_some_and(|p| p.is_file())
            && self.clear_after.unwrap().elapsed() >= Duration::from_millis(500)
    }
    fn help_assert_passive(&self, host: &Host) {
        assert_eq!(Self::help_editor_state(host), self.help_editor, "passive phase {} cancel observed {}", self.phase, self.help_cancel_observed);
        assert_eq!(host.view.tree.capture_focus(), self.help_focus);
        assert!(host.view.binding().current_run().unwrap().submissions.is_empty());
        assert_eq!(host.view.selected(), 0);
        let row = self.row.unwrap();
        assert_eq!(host.view.tree.get(row).unwrap().component::<ButtonContent>().unwrap() as *const _ as usize, self.widget);
    }
    fn help_rehover(&self, host: &Host) {
        let point = host.view.tree.get(self.row.unwrap()).unwrap().visible().unwrap().center();
        run_script(vec![(0, Action::MouseMove { x: -1., y: -1. }), (100, Action::MouseMove { x: point.x as f64, y: point.y as f64 })]);
    }
    fn advance_help(&mut self, host: &Host, window: &Window) {
        assert!(window.is_active(), "native help requires an actually active window");
        let row = self.row.unwrap();
        let source = host.sidebar_query.as_ref().unwrap();
        let address = tekes_agent_contract::SessionAddress { endpoint_id: "fixture".into(), session_id: "a".into() };
        match self.phase {
            10 => {
                if host.view.editor().input.marked.is_none() { Self::help_wake(); return; }
                if self.help_editor.is_empty() {
                    self.help_editor = Self::help_editor_state(host); self.help_value = Self::help_editor_value(host);
                    self.help_focus = host.view.tree.capture_focus();
                    assert_eq!(host.view.tree.focused(), Some(host.view.ids().editor));
                }
                let Some((owner, text, _)) = host.view.tree.help_tooltip() else { Self::help_wake(); return; };
                assert_eq!(owner, row); assert_eq!(text, "命中 A");
                self.help_assert_passive(host);
                assert_eq!(host.view.tree.semantics().iter().find(|n| n.id == row).unwrap().semantics.description.as_deref(), Some("命中 A"));
                self.help_capture("AGENT_SIDEBAR_HELP_CAPTURE"); self.phase = 11;
            }
            11 if self.help_captured() => {
                self.help_assert_passive(host);
                self.fixture.set_search_result("help-b", serde_json::json!({"items":[{"sessionId":"a","snippet":"新查询 B · 中文 👩🏽‍💻 e\u{301} 混排与原始摘要 ".repeat(8)}],"hasMore":false}));
                source.set_query("help-b").unwrap(); self.phase = 12;
            }
            12 if source.snapshot().query == "help-b" && matches!(source.snapshot().input.phase, SidebarSearchPhase::Complete { .. }) => {
                if host.view.sidebar_search_snippet(&address).is_none_or(|text| !text.starts_with("新查询 B")) { Self::help_wake(); return; }
                let previous = self.row.unwrap();
                let returned = host.view.tree.get(host.view.sidebar_id()).unwrap().component::<Sidebar>().unwrap().row_id(&address).unwrap();
                assert_ne!(returned, previous, "Searching retired the old result owner");
                assert!(host.view.tree.get(previous).is_none());
                self.row = Some(returned);
                self.widget = host.view.tree.get(returned).unwrap().component::<ButtonContent>().unwrap() as *const _ as usize;
                self.help_assert_passive(host);
                assert!(host.view.tree.help_tooltip().is_none_or(|(_, text, _)| text.starts_with("新查询 B")), "old query text is revoked");
                self.help_rehover(host); self.phase = 13;
            }
            13 => {
                let Some((owner, text, _)) = host.view.tree.help_tooltip() else { Self::help_wake(); return; };
                assert_eq!(owner, row); assert!(text.starts_with("新查询 B"));
                self.help_assert_passive(host);
                self.help_capture("AGENT_SIDEBAR_HELP_UPDATED_CAPTURE"); self.phase = 14;
            }
            14 if self.help_captured() => {
                self.help_assert_passive(host);
                assert_eq!(host.view.tree.get(row).unwrap().component::<ButtonContent>().unwrap().activations, 0);
                let point = host.view.tree.get(row).unwrap().visible().unwrap().center();
                self.clear_after = Some(Instant::now()); self.phase = 15;
                run_script(vec![(0, Action::Click { x: point.x as f64, y: point.y as f64 })]);
            }
            15 if self.help_pointer_events >= 2 => {
                self.help_assert_passive(host);
                assert!(host.view.editor().input.marked.is_some());
                assert_eq!(host.view.tree.get(row).unwrap().component::<ButtonContent>().unwrap().activations, 0, "IME barrier blocks the actual row action");
                self.phase = 16; run_script(vec![(0, Action::Unmark)]);
            }
            16 if host.view.editor().input.marked.is_none() => {
                if host.view.binding().current_run().unwrap().draft.text != host.view.editor().input.text { Self::help_wake(); return; }
                self.help_editor = Self::help_editor_state(host); self.help_value = Self::help_editor_value(host); self.help_focus = host.view.tree.capture_focus();
                self.help_rehover(host); self.phase = 17;
            }
            17 if host.view.tree.help_tooltip().is_some() => {
                self.help_assert_passive(host);
                let point = host.view.tree.get(row).unwrap().visible().unwrap().center();
                self.clear_after = Some(Instant::now()); self.phase = 18;
                run_script(vec![(0, Action::Click { x: point.x as f64, y: point.y as f64 })]);
            }
            18 if self.help_pointer_events >= 4 => {
                assert_eq!(host.view.tree.get(row).unwrap().component::<ButtonContent>().unwrap().activations, 1);
                assert!(host.view.binding().current_run().unwrap().submissions.is_empty());
                self.help_editor = Self::help_editor_state(host); self.help_value = Self::help_editor_value(host); self.help_focus = host.view.tree.capture_focus();
                source.set_query("").unwrap(); self.phase = 19;
            }
            19 if source.snapshot().input.phase == SidebarSearchPhase::Idle => {
                if host.view.tree.get(row).unwrap().help_text() != Some("/fixture") { Self::help_wake(); return; }
                self.help_assert_passive(host);
                assert_eq!(host.view.sidebar_search_snippet(&address), None);
                assert!(host.view.tree.help_tooltip().is_none());
                self.help_rehover(host); self.phase = 20;
            }
            20 => {
                let Some((owner, text, _)) = host.view.tree.help_tooltip() else { Self::help_wake(); return; };
                assert_eq!(owner, row); assert_eq!(text, "/fixture"); self.help_assert_passive(host);
                self.help_capture("AGENT_SIDEBAR_HELP_PATH_CAPTURE"); self.phase = 21;
            }
            21 if self.help_captured() => {
                self.help_assert_passive(host); self.phase = 22;
                run_script(vec![(0, Action::TextCommand("cancelOperation:"))]);
            }
            22 if self.help_cancel_observed && host.view.tree.help_tooltip().is_none() => {
                // An explicit text command may advance the SDK mutator epoch.
                // Verify value/selection/marked/focus first, then fence subsequent
                // passive metadata updates against this acknowledged baseline.
                assert_eq!(Self::help_editor_value(host), self.help_value);
                assert_eq!(host.view.tree.capture_focus(), self.help_focus);
                self.help_editor = Self::help_editor_state(host);
                self.help_assert_passive(host);
                let mut summary = host.runtime.inspect(|state| state.sessions["a"].summary.clone()).unwrap();
                summary["cwd"] = serde_json::json!("/fixture-updated");
                self.fixture.publish_inventory_summary(summary).unwrap(); self.phase = 23;
            }
            23 if host.view.tree.get(row).unwrap().help_text() == Some("/fixture-updated") => {
                // The public canonical inventory notification also advances the
                // Run publication: its controlled-draft acknowledgement may
                // advance the SDK epoch, unlike a help-only publication.
                assert_eq!(Self::help_editor_value(host), self.help_value);
                assert_eq!(host.view.tree.capture_focus(), self.help_focus);
                self.help_editor = Self::help_editor_state(host);
                self.help_assert_passive(host); self.clear_after = Some(Instant::now()); self.phase = 24;
            }
            24 if self.clear_after.unwrap().elapsed() >= Duration::from_millis(1200) => {
                self.help_assert_passive(host);
                assert!(host.view.tree.help_tooltip().is_none(), "same-owner path metadata must not undo Escape dismissal");
                self.help_rehover(host); self.phase = 25;
            }
            25 => {
                let Some((owner, text, _)) = host.view.tree.help_tooltip() else { Self::help_wake(); return; };
                assert_eq!(owner, row); assert_eq!(text, "/fixture-updated"); self.help_assert_passive(host);
                self.help_capture("AGENT_SIDEBAR_HELP_PATH_UPDATED_CAPTURE"); self.phase = 26;
            }
            26 if self.help_captured() => {
                self.help_assert_passive(host);
                assert_eq!(self.fixture.search_calls(), 3);
                assert_eq!(host.view.tree.get(row).unwrap().component::<ButtonContent>().unwrap().activations, 1);
                eprintln!("NATIVE_AGENT_SIDEBAR_HELP_RECEIPT {}", serde_json::json!({"nativeWindow":true,"windowActive":true,"originalTooltip":true,"queryUpdateAndPathFallback":true,"queryLoadingRetiresAndRebindsOwner":true,"sameOwnerPathMetadataRetained":true,"canonicalRunAcknowledgementMayAdvanceEpoch":true,"nativeCancelOperationSurvivesMetadata":true,"physicalEscape":false,"syntheticKeyIssue":37,"freshHoverCycle":true,"preeditSelectionAndBaselinePreservedDuringHelp":true,"imeBlockedRowActivation":true,"explicitUnmarkCommitted":true,"originalActionAfterUnmark":1,"nativePointerEvents":self.help_pointer_events,"noSubmission":true,"typedSemanticHelp":true,"physicalIme":false,"voiceOver":false,"realHarness":false}));
                self.phase = 5; run_script(vec![(100, Action::Close)]);
            }
            _ => {}
        }
        if self.phase != 5 { Self::help_wake(); }
    }
}
