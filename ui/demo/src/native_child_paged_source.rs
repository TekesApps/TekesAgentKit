//! Explicit offscreen proof of the original Native child's paged read controls.
use super::*;
use tekes_agent_components_ui::{
    ReadOnlyTranscript, ReadOnlyTranscriptIntent, ReadOnlyTranscriptRead, SidePanel,
    SidePanelTabKind, SummaryView,
};
use tekes_agent_ui_bindings::BoundedChildPagedPaneWitness;

struct Proof {
    parent: RunScope,
    child: String,
    marker: String,
    parent_binding: Arc<RunBinding>,
    draft: String,
    selection: Option<tekes_agent_presentation::SelectionToken>,
    source: Arc<ChildPaneBinding>,
    root: ComponentId,
    source_id: String,
    rows: std::collections::BTreeMap<String, u64>,
    task_tab: SidePanelTabKind,
    summary_source_call_id: Option<String>,
    summary_task: Option<(String, String, u64, String, usize, usize)>,
    summary_running: Value,
    summary_settled: Value,
}

impl Proof {
    fn capture(&self, host: &mut Host, earlier: bool) -> ReadOnlyTranscriptIntent {
        let (refresh, more) = host
            .view
            .bounded_child_buttons()
            .expect("original read controls");
        let commands = press(host, if earlier { more } else { refresh });
        let [
            DemoCommand::ReadChild {
                source,
                panel_parent,
                page,
                read_intent,
            },
        ] = commands.as_slice()
        else {
            panic!(
                "one original ReadOnlyTranscript command required: {}",
                commands.len()
            );
        };
        assert!(Arc::ptr_eq(source, &self.source));
        assert_eq!(panel_parent, &self.parent);
        assert_eq!(*page, earlier);
        let intent = read_intent.clone().expect("original read intent");
        assert_eq!(intent.scope(), self.source.binding().scope());
        assert_eq!(
            intent.read(),
            if earlier {
                ReadOnlyTranscriptRead::Earlier
            } else {
                ReadOnlyTranscriptRead::Refresh
            }
        );
        intent
    }

    fn read(&self, host: &mut Host, earlier: bool) -> (Value, BoundedChildPagedPaneWitness) {
        let intent = self.capture(host, earlier);
        let origin = json!({
            "read":if earlier { "earlier" } else { "refresh" },
            "scope":intent.scope(),"publication":intent.publication(),
            "connectionEpoch":intent.connection_epoch()
        });
        let witness = host
            .executor
            .block_on(self.source.read_intent_with_paged_source_witness(intent))
            .expect("original intent same-read cumulative witness");
        (origin, witness)
    }

    fn install(&self, host: &mut Host, witness: &BoundedChildPagedPaneWitness) {
        host.apply_result(
            HostResult::ChildRead {
                panel_parent: self.parent.clone(),
                source: self.source.clone(),
                result: Ok(witness.snapshot.clone()),
                catalog_error: None,
            },
            None,
        );
        assert!(
            host.failures.is_empty(),
            "host installation: {:?}",
            host.failures
        );
        // apply_result uses the original view installer, which synchronously updates
        // the mounted component. Do not request a second source snapshot here.
        assert!(host.view.child_pane_active(&self.parent, &self.child));
        assert_eq!(host.view.bounded_child_view_id(), Some(self.root));
        let (source, snapshot) = host
            .view
            .acceptance_child_pane_snapshot(&self.parent, &self.child)
            .expect("mounted original source cache");
        assert!(Arc::ptr_eq(source, &self.source));
        assert_eq!(
            pane_snapshot_json(snapshot),
            pane_snapshot_json(&witness.snapshot)
        );
        self.parent_preserved(host);
    }

    fn parent_preserved(&self, host: &Host) {
        assert!(Arc::ptr_eq(
            host.view.current_binding().unwrap(),
            &self.parent_binding
        ));
        let run = self.parent_binding.current_run().unwrap();
        assert_eq!(run.draft.text, self.draft);
        assert_eq!(run.selection, self.selection);
    }

    fn select_summary(&self, host: &mut Host, summary: bool) {
        let panel_id = host.view.panel_id();
        let tab = if summary {
            SidePanelTabKind::Summary
        } else {
            self.task_tab.clone()
        };
        let id = host
            .view
            .tree
            .get(panel_id)
            .and_then(|node| node.component::<SidePanel>())
            .and_then(|panel| panel.tab_id(&tab))
            .expect("original Summary and Task tabs");
        let commands = press(host, id);
        host.commands(Ok(commands));
        host.view.sync().expect("switch original Summary tab");
        assert!(host.failures.is_empty(), "tab switch: {:?}", host.failures);
    }

    #[cfg(feature = "acceptance-source-witness")]
    fn summary_sample(
        &self,
        host: &Host,
        phase: &str,
        source_call_id: &str,
        witness: &tekes_agent_presentation::RunSourceWitness,
    ) -> Value {
        if !witness.projection_succeeded() {
            return json!({"available":false,"phase":phase,"reason":"parent source witness reports degraded projection"});
        }
        let run = witness.snapshot();
        let Some(facts) = run.summary.as_ref() else {
            return json!({"available":false,"phase":phase,"reason":"parent Run has no authoritative Summary facts"});
        };
        let authority = witness.authority();
        if authority.endpoint_id != self.parent.endpoint_id
            || authority.connection_epoch != run.connection_epoch
        {
            return json!({"available":false,"phase":phase,"reason":"parent source witness scope/epoch mismatch"});
        }
        let Some(parent_source) = authority.sessions.get(&self.parent.session_id) else {
            return json!({"available":false,"phase":phase,"reason":"parent source witness omitted its public session projection"});
        };
        let mut matches = Vec::new();
        for tool in run
            .transcript
            .tools
            .iter()
            .filter(|tool| tool.name == "task")
        {
            let position = match tool.source_position.as_ref() {
                Some(position) => position,
                None => continue,
            };
            let (raw_call_id, source_entry) = match position {
                tekes_agent_presentation::TranscriptSourcePosition::Journal {
                    sequence, ..
                } => {
                    let Some(entry) = parent_source.entries.get(sequence) else {
                        continue;
                    };
                    (
                        entry.event.data["callId"].as_str(),
                        serde_json::to_value(entry).ok(),
                    )
                }
                tekes_agent_presentation::TranscriptSourcePosition::Native {
                    entry_id,
                    entry_ordinal,
                    content_index,
                } => {
                    let Some(entry) = parent_source
                        .native_conversation
                        .as_ref()
                        .and_then(|conversation| conversation.entries.get(*entry_ordinal))
                        .filter(|entry| entry["id"] == *entry_id)
                    else {
                        continue;
                    };
                    let Some(call) =
                        content_index.and_then(|index| entry["message"]["content"].get(index))
                    else {
                        continue;
                    };
                    (call["id"].as_str(), Some(call.clone()))
                }
            };
            if raw_call_id == Some(source_call_id) {
                let Some(task) = facts.tasks.iter().find(|task| task.id == tool.id) else {
                    continue;
                };
                matches.push((tool, task, source_entry, position));
            }
        }
        if matches.len() != 1 {
            return json!({"available":false,"phase":phase,
                "reason":"parent source witness did not resolve exactly one Task without decoding its opaque ID",
                "matchedCalls":matches.len()});
        }
        let (tool, task, source_entry, position) = matches.pop().expect("one matching source Task");
        assert_eq!(
            task.scope, self.parent,
            "Summary Task must retain parent scope"
        );
        assert_eq!(
            tool.name, "task",
            "Summary row must originate from a Task call"
        );
        let adapted = if tool.phase == "doing" {
            "in_progress"
        } else {
            tool.phase.as_str()
        };
        assert_eq!(
            task.status, adapted,
            "Summary status must adapt only its original tool source"
        );
        let arguments = match &tool.arguments {
            Value::Object(_) => Some(tool.arguments.clone()),
            Value::String(encoded) => serde_json::from_str::<Value>(encoded).ok(),
            _ => None,
        };
        let Some(title) = arguments
            .as_ref()
            .and_then(|arguments| arguments["task_name"].as_str())
        else {
            return json!({"available":false,"phase":phase,"reason":"public Task call has no source task_name","sourceCallId":source_call_id});
        };
        assert_eq!(
            title, task.name,
            "Summary title must come from original Task arguments"
        );
        let Some(view) = host
            .view
            .tree
            .get(host.view.summary_id())
            .and_then(|node| node.component::<SummaryView>())
        else {
            return json!({"available":false,"phase":phase,"reason":"original SummaryView is not mounted"});
        };
        let Some(installed_input) = view.displayed_input() else {
            return json!({"available":false,"phase":phase,"reason":"SummaryView has no Run input","sourceCallId":source_call_id});
        };
        assert_eq!(
            installed_input.scope, self.parent,
            "Summary owner scope changed"
        );
        if installed_input.revision != run.revision {
            // The source capture and UI installation are separate domains. A
            // newer watch publication is a retry, never joined to this witness.
            return json!({"available":false,"phase":phase,
                "reason":"Summary advanced beyond the captured parent publication",
                "capturedRevision":run.revision,"installedRevision":installed_input.revision});
        }
        assert_eq!(
            installed_input.facts.as_ref(),
            Some(facts),
            "Summary facts differ from the same Run publication"
        );
        let installed = installed_input
            .facts
            .as_ref()
            .and_then(|facts| facts.tasks.iter().find(|candidate| candidate.id == task.id));
        let Some(installed) = installed else {
            return json!({"available":false,"phase":phase,"reason":"mounted SummaryView has no original Task row","sourceCallId":source_call_id});
        };
        assert_eq!(
            installed, task,
            "mounted row must be the same parent publication"
        );
        let row = view
            .row_id("tasks", &task.id)
            .expect("original Summary task row");
        assert!(
            host.view.tree.get(row).is_some(),
            "Summary row must be mounted"
        );
        assert!(
            view.action_id("tasks", &task.id).is_none(),
            "parent Task is passive"
        );
        assert!(
            task.independent_run().is_none(),
            "parent Task cannot navigate as a child Run"
        );
        let title_id = view
            .passive_text_id("tasks", &task.id)
            .expect("original passive Task title");
        let title_node = host
            .view
            .tree
            .get(title_id)
            .expect("original passive Summary title");
        let installed_field = title_node
            .component::<tekes_ui_controls::composition::TextFieldContent>()
            .expect("original passive Summary field");
        let installed_title = installed_field.field.label.clone();
        assert_eq!(
            installed_title, task.name,
            "Summary must render the original Task title"
        );
        let canonical = task.canonical_status();
        if let Some((old_task, _, old_row, old_title, old_view, old_field)) = &self.summary_task {
            assert_eq!(old_task, &task.id, "Summary call identity changed");
            assert_eq!(*old_row, row.0, "original Summary row identity changed");
            assert_eq!(old_title, &task.name, "original Summary title changed");
            assert_eq!(
                *old_view, view as *const SummaryView as usize,
                "SummaryView instance changed"
            );
            assert_eq!(
                *old_field, installed_field as *const _ as usize,
                "passive Task field instance changed"
            );
        }
        json!({"available":true,"phase":phase,"publication":run.revision,
            "connectionEpoch":run.connection_epoch,
            "scope":facts.scope,"taskId":task.id,"title":task.name,"rawStatus":tool.phase,"adaptedStatus":task.status,
            "canonicalStatus":canonical,"rowId":row.0,"mounted":true,
            "viewInstance":view as *const SummaryView as usize,
            "titleInstance":installed_field as *const _ as usize,
            "passive":true,"actionable":false,"independentRun":false,
            "tool":tool,
            "sourceCallId":source_call_id,"sourcePosition":position,
            "sourceEntry":source_entry,
            "witness":{"snapshot":run.as_ref(),"authority":authority,
                "projectionSucceeded":witness.projection_succeeded(),
                "evidenceScope":"original-publication-and-same-memory-input",
                "serverTransactionalFreshnessProven":false}})
    }

    #[cfg(feature = "acceptance-source-witness")]
    fn sample_summary_until(
        &self,
        host: &mut Host,
        phase: &str,
        source_call_id: &str,
        expected_status: &str,
    ) -> Value {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let witness = self
                .parent_binding
                .refresh_with_source_witness()
                .expect("original parent Run source witness");
            host.view
                .sync()
                .expect("publish original parent Run to Summary");
            let mut latest = self.summary_sample(host, phase, source_call_id, &witness);
            if latest["available"] == true && latest["canonicalStatus"] == expected_status {
                return latest;
            }
            if Instant::now() >= deadline {
                if latest["available"] == true {
                    latest["reason"] = json!(format!(
                        "parent Summary remained {} while waiting for {expected_status}",
                        latest["canonicalStatus"]
                    ));
                }
                return latest;
            }
            host.drain_results(None);
            sync(host);
            tick(host);
        }
    }

    #[cfg(not(feature = "acceptance-source-witness"))]
    fn sample_summary_until(
        &self,
        _host: &mut Host,
        phase: &str,
        source_call_id: &str,
        _expected_status: &str,
    ) -> Value {
        json!({"available":false,"phase":phase,
            "reason":"acceptance-source-witness feature is disabled",
            "sourceCallId":source_call_id})
    }

    fn frame(
        &mut self,
        host: &mut Host,
        phase: &str,
        intent: Value,
        witness: &BoundedChildPagedPaneWitness,
        old_earlier: Option<Value>,
    ) {
        self.install(host, witness);
        let mounted = host
            .view
            .tree
            .get(self.root)
            .unwrap()
            .component::<ReadOnlyTranscript>()
            .unwrap();
        assert_eq!(mounted.content().scope(), self.source.binding().scope());
        let row_keys: Vec<_> = mounted
            .content()
            .row_keys()
            .iter()
            .map(|key| format!("{key:?}"))
            .collect();
        let row_ids: Vec<_> = mounted
            .content()
            .row_keys()
            .iter()
            .map(|key| mounted.content().row_id(key).expect("mounted row ID").0)
            .collect();
        assert!(!row_keys.is_empty());
        for (key, id) in row_keys.iter().zip(&row_ids) {
            if let Some(previous) = self.rows.get(key) {
                assert_eq!(previous, id, "retained reading row changed ID");
            }
            self.rows.insert(key.clone(), *id);
        }
        let snapshot = pane_snapshot_json(&witness.snapshot);
        let mut frame = json!({
            "phase":phase,"parent":self.parent,"childId":self.child,"marker":self.marker,
            "sourceInstance":self.source_id,"intent":intent,
            "witness":{"snapshot":snapshot,"source":witness.source},
            "mountedSnapshot":snapshot,
            "mounted":{"control":"original-ReadOnlyTranscript","transcriptId":self.root.0,
                "rowKeys":row_keys,"rowIds":row_ids,"transcript":snapshot["transcript"],
                "parentDraft":self.draft,"parentSelection":self.selection,
                "sourceIdentity":self.source_id,"parentBindingPreserved":true}
        });
        if phase == "running_head" {
            frame["summaryRunning"] = self.summary_running.clone();
        }
        if phase == "settled_refresh" {
            frame["summarySettled"] = self.summary_settled.clone();
            let reason = if self.summary_running["available"] != true {
                self.summary_running["reason"].clone()
            } else if self.summary_running["canonicalStatus"] != "in_progress" {
                json!("parent Summary did not publish in_progress")
            } else if self.summary_settled["available"] != true {
                self.summary_settled["reason"].clone()
            } else if self.summary_settled["canonicalStatus"] != "completed" {
                json!("parent spawn-tool Summary did not publish completed")
            } else if self.summary_running["rowId"] != self.summary_settled["rowId"] {
                json!("parent Summary row identity changed")
            } else {
                Value::Null
            };
            frame["summaryTransition"] = json!({
                "accepted":self.summary_running["available"] == true
                    && self.summary_running["canonicalStatus"] == "in_progress"
                    && self.summary_settled["available"] == true
                    && self.summary_settled["canonicalStatus"] == "completed"
                    && self.summary_running["rowId"] == self.summary_settled["rowId"]
                    && self.summary_running["taskId"] == self.summary_settled["taskId"]
                    && self.summary_running["sourceCallId"] == self.summary_settled["sourceCallId"]
                    && self.summary_running["sourcePosition"] == self.summary_settled["sourcePosition"]
                    && self.summary_running["viewInstance"] == self.summary_settled["viewInstance"]
                    && self.summary_running["titleInstance"] == self.summary_settled["titleInstance"],
                "unavailableReason":reason,
                "profile":"native-passive-spawn-tool","formalTaskLifecycleAcceptance":false,
                "parentToolFailureAcceptance":false,
                "parentProjectionOnly":true,"statusComesFromChildWitness":false,
                "running":self.summary_running,"settled":self.summary_settled
            });
        }
        if let Some(old) = old_earlier {
            frame["oldEarlier"] = old;
        }
        use std::io::Write;
        println!("AGENTKIT_NATIVE_PAGED_SOURCE {frame}");
        std::io::stdout().flush().expect("flush paged source frame");
    }
}

pub(super) fn run(
    host: &mut Host,
    manifest: &Value,
    parent_binding: Arc<RunBinding>,
    draft: String,
    source: Arc<ChildPaneBinding>,
) -> Value {
    let parent = scope_from_manifest(manifest);
    let child = manifest["childId"].as_str().expect("child ID").to_owned();
    let marker = manifest["marker"]
        .as_str()
        .expect("child marker")
        .to_owned();
    let earlier_reads = manifest["expectedEarlierReads"]
        .as_u64()
        .expect("public page plan length");
    assert!(
        (1..=16).contains(&earlier_reads),
        "bounded public page plan required"
    );
    assert_eq!(parent.endpoint_id, "native");
    let mut proof = Proof {
        selection: parent_binding.current_run().unwrap().selection.clone(),
        source_id: format!("{:p}", Arc::as_ptr(&source)),
        root: host
            .view
            .bounded_child_view_id()
            .expect("original readonly pane"),
        parent,
        child,
        marker,
        parent_binding,
        draft,
        source,
        rows: Default::default(),
        task_tab: host
            .view
            .tree
            .get(host.view.panel_id())
            .and_then(|node| node.component::<SidePanel>())
            .and_then(|panel| panel.displayed_input())
            .and_then(|input| input.selected.clone())
            .expect("active child pane tab"),
        summary_source_call_id: None,
        summary_task: None,
        summary_running: Value::Null,
        summary_settled: Value::Null,
    };
    // Opening the original pane may still have a scheduled ordinary initial read.
    // Drain it before activating an explicit control; its real reads stay in the chain.
    wait(host, "initial readonly read", |host| {
        host.view
            .tree
            .get(proof.root)
            .and_then(|node| node.component::<ReadOnlyTranscript>())
            .is_some_and(|pane| {
                host.view
                    .tree
                    .get(pane.ids().refresh)
                    .and_then(|node| node.component::<tekes_ui_controls::composition::Button>())
                    .is_some_and(|button| !button.button.state.disabled)
            })
    });
    let (intent, head) = proof.read(host, false);
    assert_eq!(head.source.page_size, 1);
    assert!(head.source.has_more_before && head.source.run_open);
    let original_epoch = head.source.connection_epoch;
    if let Some(call_id) = head
        .source
        .source_call
        .as_ref()
        .and_then(|call| call["callId"].as_str())
    {
        proof.summary_source_call_id = Some(call_id.to_owned());
        proof.select_summary(host, true);
        let sample = proof.sample_summary_until(host, "running", call_id, "in_progress");
        if sample["available"] == true && sample["canonicalStatus"] == "in_progress" {
            proof.summary_task = Some((
                sample["taskId"].as_str().expect("task identity").to_owned(),
                call_id.to_owned(),
                sample["rowId"].as_u64().expect("row ID"),
                sample["title"].as_str().expect("title").to_owned(),
                usize::try_from(sample["viewInstance"].as_u64().expect("view instance"))
                    .expect("pointer width"),
                usize::try_from(sample["titleInstance"].as_u64().expect("title instance"))
                    .expect("pointer width"),
            ));
        }
        proof.summary_running = sample;
        proof.select_summary(host, false);
        assert!(host.view.child_pane_active(&proof.parent, &proof.child));
        assert_eq!(host.view.bounded_child_view_id(), Some(proof.root));
    } else {
        proof.summary_running = json!({"available":false,"phase":"running",
            "reason":"public child witness omitted original Task call identity"});
    }
    proof.frame(host, "running_head", intent, &head, None);

    let stale = proof.capture(host, true);
    let captured_epoch = stale.connection_epoch();
    let runtime = host
        .sidebar
        .runtime_for(&proof.parent.endpoint_id)
        .unwrap()
        .clone();
    host.executor.block_on(async {
        runtime.reconnect().await.unwrap();
        runtime.wait_ready(Duration::from_secs(10)).await.unwrap();
    });
    let current_epoch = runtime.inspect(|state| state.connection_epoch);
    assert!(current_epoch > original_epoch);
    let error = host
        .executor
        .block_on(proof.source.read_intent_with_paged_source_witness(stale))
        .err()
        .expect("old Earlier must be rejected")
        .to_string();
    assert!(
        error.contains("epoch expired"),
        "unexpected old intent rejection: {error}"
    );
    // Use the original error-completion lane to clear the gesture's loading state.
    // The expected rejection is evidence, not an unexpected host failure.
    host.view
        .child_read_failed(&proof.parent, &proof.source, error.clone())
        .unwrap();
    let background = host.view.take_background_commands();
    host.commands(Ok(background));
    wait(host, "reconnected readonly controls", |host| {
        host.view
            .tree
            .get(proof.root)
            .and_then(|node| node.component::<ReadOnlyTranscript>())
            .is_some_and(|pane| {
                host.view
                    .tree
                    .get(pane.ids().refresh)
                    .and_then(|node| node.component::<tekes_ui_controls::composition::Button>())
                    .is_some_and(|button| !button.button.state.disabled)
            })
    });
    let (intent, refreshed) = proof.read(host, false);
    assert_eq!(refreshed.source.connection_epoch, current_epoch);
    assert!(
        refreshed
            .source
            .receipts
            .iter()
            .any(|receipt| receipt.read_epoch == original_epoch)
    );
    proof.frame(host,"reconnected_refresh",intent,&refreshed,Some(json!({
        "rejected":true,"capturedEpoch":captured_epoch,"currentEpoch":current_epoch,"error":error
    })));
    for index in 1..=earlier_reads {
        let (intent, witness) = proof.read(host, true);
        assert!(witness.source.run_open);
        assert_eq!(witness.source.has_more_before, index < earlier_reads);
        proof.frame(
            host,
            &format!("reconnected_page_{index}"),
            intent,
            &witness,
            None,
        );
    }
    // The strict independent validator releases the held fifth request only
    // after every original Earlier matches the frozen public page plan.
    let deadline = Instant::now() + Duration::from_secs(45);
    let settled = loop {
        let (intent, witness) = proof.read(host, false);
        let settled = witness
            .source
            .source_call
            .as_ref()
            .is_some_and(|call| call["phase"] == "error")
            && !witness.source.run_open;
        if settled {
            break (intent, witness);
        }
        proof.install(host, &witness);
        assert!(
            Instant::now() < deadline,
            "provider did not settle after paged frames"
        );
        tick(host);
    };
    assert!(
        settled
            .1
            .source
            .receipts
            .iter()
            .any(|receipt| receipt.operation
                == tekes_agent_presentation::BoundedChildPagedSourceOperation::PrefixValidation
                && !receipt.merged)
    );
    if let Some(source_call_id) = &proof.summary_source_call_id {
        proof.select_summary(host, true);
        let settled_summary =
            proof.sample_summary_until(host, "settled", source_call_id, "completed");
        proof.summary_settled = settled_summary;
        proof.select_summary(host, false);
    }
    proof.frame(host, "settled_refresh", settled.0, &settled.1, None);
    let close = host
        .view
        .tree
        .get(host.view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap()
        .close_id();
    let commands = press(host, close);
    host.commands(Ok(commands));
    sync(host);
    assert!(!host.view.child_pane_active(&proof.parent, &proof.child));
    assert_eq!(proof.source.binding().access(), PaneAccess::Missing);
    assert!(
        host.executor
            .block_on(
                proof
                    .source
                    .binding()
                    .snapshot_with_bounded_paged_source_witness()
            )
            .is_err()
    );
    proof.parent_preserved(host);
    json!({"accepted":true,"evidenceScope":"actual-native-paged-child-original-intents",
        "parent":proof.parent,"childId":proof.child,"childScope":proof.source.binding().scope(),
        "openedViaOriginalControl":true,"closedViaOriginalControl":true,"sameSource":true,
        "oldEarlierRejected":true,"publicPagePlanMatched":true,"earlierReadCount":earlier_reads,
        "parentDraftPreserved":true,"parentSelectionPreserved":true,"retiredWitnessRejected":true,
        "desktopInteraction":false,"physicalIme":false,"readingAnchorAcceptance":false})
}
