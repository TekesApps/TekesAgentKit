//! Frozen #56 Native bounded-child family observer. This is a consumer of the public
//! Kernel/SessionRuntime/ChildRun APIs; the wrapper records transport calls without changing them.
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read, sync::Arc, time::Duration};
use tekes_agent_contract::*;
use tekes_agent_endpoint_kernel::KernelConnector;
use tekes_agent_presentation::{
    ChildComposerAvailability, ChildHistoryLayout, ChildRun, ComposerDestination, ComposerDraft,
    RunController, RunScope, TranscriptSnapshot,
};
use tekes_agent_session::{SessionProjection, SessionRuntime};

#[path = "support/native_family_observer.rs"]
mod observer;
use observer::{
    CallLog, ObservedConnector, insert_raw_entry, monotonic_ns, validate_page_cut,
    validate_role_path,
};

fn emit(value: Value) {
    println!("{value}");
    use std::io::Write;
    let _ = std::io::stdout().flush();
}

async fn cleanup_runtime(runtime: &SessionRuntime, error: Option<&str>) -> Result<()> {
    let intent = monotonic_ns();
    emit(
        json!({"event":"cleanup_intent","originalIntentMonotonicNs":intent,"intentMonotonicNs":intent,"monotonicNs":intent,"error":error}),
    );
    let stop = runtime.stop().await;
    let completed = monotonic_ns();
    emit(
        json!({"event":"runtime_cleanup_completed","monotonicNs":completed,"completedMonotonicNs":completed,"intentMonotonicNs":intent,
        "latencyNs":completed.saturating_sub(intent),"runtimeStopped":stop.is_ok(),
        "error":stop.as_ref().err().map(|error|error.to_string())}),
    );
    stop
}

fn field_string(value: &Value, key: &str) -> Result<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| Error::Protocol(format!("missing string {key}")))
}

fn call_events(log: &CallLog, context: &Value) -> Vec<Value> {
    let calls = log.drain();
    calls.into_iter().map(|call| json!({
        "event":"bounded_call", "round":context["round"], "caseIndex":context["caseIndex"], "measured":context["measured"],
        "path":context["path"], "role":context["role"], "parentScope":context["parentScope"],
        "childId":context["childId"], "connectionOrdinal":call.connection_ordinal,"sourceReadOrdinal":call.call_ordinal,
        "phase":call.phase, "intentMonotonicNs":call.intent_ns, "returnMonotonicNs":call.return_ns,
        "latencyNs":call.return_ns.saturating_sub(call.intent_ns), "request":call.request,
        "response":match call.response { Ok(value)=>json!({"value":value}), Err(error)=>json!({"error":error}) },
        "passthroughUnmodified":true
    })).inspect(|event| emit(event.clone())).collect()
}

fn pages_in_calls(calls: &[Value]) -> Result<Vec<BoundedChildHistoryPage>> {
    calls
        .iter()
        .map(|event| {
            require(
                event["request"]["method"] == "subagents.history",
                "unexpected bounded source method",
            )?;
            let response = event["response"].get("value").ok_or_else(|| {
                Error::Protocol("failed bounded read cannot be hidden by later success".into())
            })?;
            let request: BoundedChildHistoryRequest =
                serde_json::from_value(event["request"]["payload"].clone())?;
            let page: BoundedChildHistoryPage = serde_json::from_value(response.clone())?;
            page.validate(&request)?;
            Ok(page)
        })
        .collect()
}

fn union_from_calls(calls: &[Value]) -> Result<Vec<HistoryEntry>> {
    let mut union = BTreeMap::new();
    for page in pages_in_calls(calls)? {
        for entry in &page.entries {
            insert_raw_entry(&mut union, entry)?;
        }
    }
    Ok(union.into_values().collect())
}

fn has_earlier_request(calls: &[Value]) -> bool {
    calls
        .iter()
        .any(|call| !call["request"]["payload"]["beforeSequence"].is_null())
}

fn require_product_earlier(
    head_calls: &[Value],
    earlier_calls: &[Value],
    expected_page_size: u64,
) -> Result<()> {
    require_page_size(head_calls, expected_page_size)?;
    require_page_size(earlier_calls, expected_page_size)?;
    let head = head_calls
        .iter()
        .filter(|call| call["request"]["payload"]["beforeSequence"].is_null())
        .find_map(|call| {
            serde_json::from_value::<BoundedChildHistoryPage>(call["response"]["value"].clone())
                .ok()
        })
        .ok_or_else(|| {
            Error::Protocol("product path omitted a successful bounded head response".into())
        })?;
    require(
        head.has_more_before,
        "bounded head did not advertise Earlier history",
    )?;
    let (request, earlier) = earlier_calls
        .iter()
        .filter(|call| !call["request"]["payload"]["beforeSequence"].is_null())
        .find_map(|call| {
            let request = serde_json::from_value::<BoundedChildHistoryRequest>(
                call["request"]["payload"].clone(),
            )
            .ok()?;
            let page = serde_json::from_value::<BoundedChildHistoryPage>(
                call["response"]["value"].clone(),
            )
            .ok()?;
            Some((request, page))
        })
        .ok_or_else(|| Error::Protocol("product path omitted a successful Earlier page".into()))?;
    request.validate()?;
    earlier.validate(&request)?;
    require(
        request.max_messages as u64 == expected_page_size,
        "Earlier request page size changed",
    )?;
    require(
        request.through_sequence == Some(head.through_sequence)
            && request.history_revision.as_deref() == Some(head.history_revision.as_str()),
        "Earlier request moved the original history cut",
    )?;
    require(
        request.before_sequence == head.entries.first().map(|entry| entry.event.seq),
        "Earlier request does not follow the true source frontier",
    )?;
    require(!earlier.entries.is_empty(), "Earlier response was empty")
}

fn require_page_size(calls: &[Value], expected: u64) -> Result<()> {
    require(!calls.is_empty(), "bounded history request log is empty")?;
    for call in calls {
        require(
            call["request"]["method"] == "subagents.history",
            "unexpected request in bounded call log",
        )?;
        require(
            call["request"]["payload"]["maxMessages"] == expected,
            "actual subagents.history request used the wrong page size",
        )?;
    }
    Ok(())
}

fn require_page_cut_revision(
    actual: &BoundedChildHistoryPage,
    expected: &BoundedChildHistoryPage,
) -> Result<()> {
    validate_page_cut(actual.through_sequence, expected.through_sequence)?;
    require(
        actual.history_revision == expected.history_revision,
        "history revision changed across reconnect",
    )?;
    require(
        actual.source_call == expected.source_call,
        "sourceCall changed across reconnect",
    )
}

fn product_projection_evidence(
    parent: &RunScope,
    child: &str,
    actual: &TranscriptSnapshot,
    pages: &[BoundedChildHistoryPage],
    has_more: bool,
) -> Result<Value> {
    let first = pages
        .first()
        .ok_or_else(|| Error::Protocol("product snapshot lacks original source pages".into()))?;
    let mut entries = BTreeMap::new();
    for page in pages {
        require(
            page.parent_session_id == parent.session_id && page.child_session_id == child,
            "product snapshot source owner changed",
        )?;
        require_page_cut_revision(page, first)?;
        for entry in &page.entries {
            insert_raw_entry(&mut entries, entry)?;
        }
    }
    let projection = SessionProjection {
        entries,
        journal_ready: true,
        run_open: first.run_open,
        has_more_before: has_more,
        tail_sequence: first.through_sequence,
        ..Default::default()
    };
    let expected = TranscriptSnapshot::read(&child_scope(parent, child), &projection)?;
    let raw = serde_json::to_value(&expected)?;
    let mut core = serde_json::to_value(actual)?;
    let source = first
        .source_call
        .as_ref()
        .filter(|value| value.is_object())
        .ok_or_else(|| Error::Protocol("product sourceCall missing".into()))?;
    let call_id = source
        .get("callId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| Error::Protocol("product sourceCall missing original callId".into()))?;
    let task_id = json!([parent.endpoint_id, child, "task-source", call_id]).to_string();
    let turn = core
        .get("turns")
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .cloned()
        .ok_or_else(|| Error::Protocol("product source Task turn missing".into()))?;
    let tool = core
        .get("tools")
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .cloned()
        .ok_or_else(|| Error::Protocol("product source Task tool missing".into()))?;
    require(
        turn["id"] == task_id && turn["input"] == "" && turn["status"] == "completed",
        "normal source Task turn identity/status changed",
    )?;
    require(
        tool["id"] == format!("{task_id}:call")
            && tool["turnId"] == task_id
            && tool.get("name") == source.get("name")
            && tool.get("arguments") == source.get("arguments")
            && tool.get("phase") == source.get("phase"),
        "product Task source decoration differs from original sourceCall",
    )?;
    core["turns"]
        .as_array_mut()
        .ok_or_else(|| Error::Protocol("product turns malformed".into()))?
        .remove(0);
    core["tools"]
        .as_array_mut()
        .ok_or_else(|| Error::Protocol("product tools malformed".into()))?
        .remove(0);
    require(
        core == raw,
        "product ChildRun projection differs from complete loaded raw source",
    )?;
    Ok(
        json!({"rawProjectionTranscript":raw,"productSourceDecoration":{"sourceCall":source,
        "turnId":task_id,"toolId":format!("{task_id}:call"),"turn":turn,"tool":tool},
        "productProjectionMatchesRawSource":true}),
    )
}

async fn open_child(
    runtime: Arc<SessionRuntime>,
    parent: RunScope,
    child: String,
    path: &str,
) -> Result<ChildRun> {
    if path == "product20" {
        #[cfg(feature = "acceptance-native-family-page20")]
        {
            return ChildRun::open_with_page_size(
                runtime,
                parent,
                child,
                ChildHistoryLayout::SameSession,
                None,
                20,
            )
            .await;
        }
        #[cfg(not(feature = "acceptance-native-family-page20"))]
        {
            return Err(Error::Protocol(
                "product20 requires acceptance-native-family-page20".into(),
            ));
        }
    }
    ChildRun::open(
        runtime,
        parent,
        child,
        ChildHistoryLayout::SameSession,
        None,
    )
    .await
}

fn child_scope(parent: &RunScope, child: &str) -> RunScope {
    RunScope {
        endpoint_id: parent.endpoint_id.clone(),
        thread_id: parent.thread_id.clone(),
        session_id: child.into(),
    }
}

fn require_parent_draft(controller: &RunController) -> Result<String> {
    let text = controller.snapshot().draft.text.clone();
    require(
        text == "AKFAMILY_PARENT_DRAFT_KEEP",
        "fixed parent draft changed or leaked",
    )?;
    Ok(text)
}

fn bounded_child_availability(catalog: &SubagentCatalog, child: &str) -> Result<Value> {
    let entry = catalog
        .child(child)
        .ok_or_else(|| Error::Protocol("bounded child absent from catalog".into()))?;
    require(
        entry.mode.as_deref() == Some("bounded"),
        "catalog child is not Native bounded mode",
    )?;
    let resolved = ChildComposerAvailability::resolve(Some(catalog), child);
    require(
        resolved == ChildComposerAvailability::Unavailable,
        "bounded child presentation exposes unsupported controls",
    )?;
    Ok(json!({
        "resolvedAvailability":resolved,
        "catalogMode":entry.mode,
        "history":"readOnlyBounded",
        "send":"unavailableByBoundedChildContract",
        "prompt":"unavailableByBoundedChildContract",
        "cancel":"unavailableByBoundedChildContract",
        "parentAvailable":catalog.parent_available,
        "childId":entry.id,
        "kind":entry.kind
    }))
}

fn bounded_request(
    parent: &str,
    child: &str,
    cut: Option<i64>,
    before: Option<i64>,
    revision: Option<String>,
) -> BoundedChildHistoryRequest {
    BoundedChildHistoryRequest {
        parent_session_id: parent.into(),
        child_session_id: child.into(),
        through_sequence: cut,
        before_sequence: before,
        max_messages: 20,
        history_revision: revision,
    }
}

async fn sep20_projection(
    runtime: &SessionRuntime,
    parent: &RunScope,
    child: &str,
    log: &CallLog,
    context: &Value,
) -> Result<Value> {
    log.set_phase("sep20-head");
    let intent = monotonic_ns();
    let head_request = bounded_request(&parent.session_id, child, None, None, None);
    let head = runtime.bounded_child_history(head_request.clone()).await?;
    let head_done = monotonic_ns();
    let mut projection = SessionProjection::default();
    projection.apply_snapshot(JournalSnapshot {
        session_id: child.into(),
        generation: head.history_revision.clone(),
        through_sequence: head.through_sequence,
        window_limit: 20,
        entries: head.entries.clone(),
        has_more_before: head.has_more_before,
        projections: None,
        history_revision: Some(head.history_revision.clone()),
    })?;
    require(
        projection.journal_ready,
        "SEP20 head did not initialize public projection",
    )?;

    log.set_phase("sep20-earlier");
    let earlier_intent = monotonic_ns();
    let earlier_request = bounded_request(
        &parent.session_id,
        child,
        Some(head.through_sequence),
        Some(projection.history_base_sequence),
        Some(head.history_revision.clone()),
    );
    let earlier = runtime
        .bounded_child_history(earlier_request.clone())
        .await?;
    require(
        head.has_more_before && !earlier.entries.is_empty(),
        "SEP20 path requires a real nonempty Earlier page",
    )?;
    let page_request = HistoryRequest {
        session_id: child.into(),
        through_sequence: head.through_sequence,
        before_sequence: Some(projection.history_base_sequence),
        max_messages: 20,
    };
    let page = HistoryPage {
        session_id: child.into(),
        through_sequence: earlier.through_sequence,
        entries: earlier.entries.clone(),
        has_more_before: earlier.has_more_before,
    };
    let accepted = projection.apply_page(page, &page_request)?;
    require(accepted, "SEP20 Earlier page was not applied")?;
    let mut expected_entries = BTreeMap::new();
    for entry in head.entries.iter().chain(earlier.entries.iter()) {
        insert_raw_entry(&mut expected_entries, entry)?;
    }
    require(
        projection.entries == expected_entries,
        "SEP20 projection differs from raw page union",
    )?;
    let snapshot = TranscriptSnapshot::read(&child_scope(parent, child), &projection)?;
    let publication = monotonic_ns();
    let raw_calls = call_events(log, context);
    require_page_size(&raw_calls, 20)?;
    let payload = json!({
        "event":"family_history_page", "round":context["round"], "caseIndex":context["caseIndex"], "measured":context["measured"],
        "path":context["path"], "role":context["role"], "parentScope":context["parentScope"],
        "childId":child, "mechanism":"sep20-projection", "requestedMessages":20,
        "rawEntryCountIsNotRequestedMessageCount":true,
        "headRequest":head_request, "rawHeadResponse":head, "earlierRequest":earlier_request,
        "rawEarlierResponse":earlier, "rawCalls":raw_calls, "unionEntries":projection.entries.values().collect::<Vec<_>>(),
        "projection":projection, "transcript":snapshot, "headIntentMonotonicNs":intent,
        "headPublicationMonotonicNs":head_done, "headLatencyNs":head_done.saturating_sub(intent),
        "earlierIntentMonotonicNs":earlier_intent, "earlierPublicationMonotonicNs":publication,
        "earlierLatencyNs":publication.saturating_sub(earlier_intent),
        "sourceChecks":{"address":true,"cutAndRevision":true,"actualEarlierApplied":accepted}
    });
    Ok(payload)
}

fn history_content_text(entry: &HistoryEntry) -> String {
    let blocks = &entry.event.data["message"]["content"];
    if let Some(text) = blocks.as_str() {
        return text.into();
    }
    blocks
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter(|block| block["type"] == "text")
                .map(|block| block["text"].as_str().unwrap_or_default())
                .collect()
        })
        .unwrap_or_default()
}

fn call_id(data: &Value) -> Option<&Value> {
    data.get("callId")
        .filter(|value| !value.is_null())
        .or_else(|| {
            data.get("message")
                .and_then(|message| message.get("source"))
                .and_then(|source| source.get("callId"))
                .filter(|value| !value.is_null())
        })
}

fn validate_groundtruth(config: &Value, pages: &[BoundedChildHistoryPage]) -> Result<Value> {
    let parent = field_string(config, "parentSessionId")?;
    let child = field_string(config, "childSessionId")?;
    let raw = config
        .get("sourceHistory")
        .ok_or_else(|| Error::Protocol("sourceHistory required".into()))?;
    let source: BoundedChildHistoryPage = serde_json::from_value(raw.clone())?;
    let request = BoundedChildHistoryRequest {
        parent_session_id: parent.clone(),
        child_session_id: child.clone(),
        through_sequence: None,
        before_sequence: None,
        max_messages: 200,
        history_revision: None,
    };
    source.validate(&request)?;
    require(
        !source.has_more_before && !source.run_open,
        "full200 source must be EOF and closed",
    )?;
    require(
        source.source_call.is_some(),
        "groundtruth omitted sourceCall",
    )?;
    let mut union = BTreeMap::new();
    for page in pages {
        require(
            page.parent_session_id == parent && page.child_session_id == child,
            "page address differs from source",
        )?;
        require(
            !page.run_open,
            "Native bounded child unexpectedly remains open",
        )?;
        validate_page_cut(page.through_sequence, source.through_sequence)?;
        require(
            page.history_revision == source.history_revision,
            "page revision differs from full200 source",
        )?;
        require(
            page.source_call == source.source_call,
            "sourceCall changed across page reads",
        )?;
        for entry in &page.entries {
            insert_raw_entry(&mut union, entry)?;
        }
    }
    let actual: Vec<_> = union.values().cloned().collect();
    require(
        actual == source.entries,
        "raw page union does not equal full200 source entries",
    )?;
    let commentary: Vec<_> = source
        .entries
        .iter()
        .filter(|entry| {
            entry.event.kind == "assistant/message"
                && entry.event.metadata.get("surfaceOp") == Some(&json!("append"))
                && entry.event.data["message"]["role"] == "assistant"
                && history_content_text(entry).starts_with("AKFAMILY|native|")
        })
        .map(history_content_text)
        .collect();
    require(
        commentary.len() == 61,
        "groundtruth must retain all 61 commentary messages",
    )?;
    for (index, text) in commentary.iter().enumerate() {
        let marker = format!("AKFAMILY|native|{:02}|commentary|", index + 1);
        let expected = format!("{marker}{}", "x".repeat(1024 - marker.len()));
        require(
            text == &expected && text.len() == 1024,
            "commentary body/order/byte length mismatch",
        )?;
    }
    let read_calls: Vec<_> = source
        .entries
        .iter()
        .filter(|entry| entry.event.kind == "tool/call" && entry.event.data["name"] == "read")
        .collect();
    let read_results: Vec<_> = source
        .entries
        .iter()
        .filter(|entry| entry.event.kind == "tool/result")
        .collect();
    require(
        read_calls.len() == 61 && read_results.len() == 61,
        "groundtruth must preserve 61 read call/result groups",
    )?;
    for (call, result) in read_calls.iter().zip(read_results.iter()) {
        let source_id = call_id(&call.event.data)
            .ok_or_else(|| Error::Protocol("read call omitted original callId".into()))?;
        let result_id = call_id(&result.event.data)
            .ok_or_else(|| Error::Protocol("read result omitted original callId".into()))?;
        require(
            source_id == result_id,
            "read call/result source identity mismatch",
        )?;
        let arguments = &call.event.data["arguments"];
        let arguments = if let Some(text) = arguments.as_str() {
            serde_json::from_str::<Value>(text).unwrap_or(Value::Null)
        } else {
            arguments.clone()
        };
        require(
            arguments["path"] == "proof.txt" || arguments["file_path"] == "proof.txt",
            "read call did not target the owned proof file",
        )?;
    }
    let final_messages: Vec<_> = source
        .entries
        .iter()
        .filter(|entry| {
            entry.event.kind == "assistant/message"
                && entry.event.metadata.get("surfaceOp") == Some(&json!("append"))
                && entry.event.data["message"]["role"] == "assistant"
                && entry.event.data["sessionFinal"] == true
        })
        .collect();
    require(
        final_messages
            .last()
            .is_some_and(|entry| history_content_text(entry) == "NATIVE_FAMILY_SEED_COMPLETE"),
        "groundtruth final assistant content mismatch",
    )?;
    Ok(
        json!({"event":"family_groundtruth","sourceHistory":raw,"rawPageUnion":actual,
        "parentSessionId":parent,"childSessionId":child,"entryCount":actual.len(),
        "rawEntryCountIsNotRequestedMessageCount":true,"commentaryCount":commentary.len(),
        "commentaryBytesEach":1024,"readCallCount":read_calls.len(),"readResultCount":read_results.len(),
        "sourceCallStable":true,"eof":true,"fullEntryContentAndOrderEqual":true}),
    )
}

async fn run(config: Value) -> Result<()> {
    require(
        config["familyFormat"] == 1,
        "Native family ABI version mismatch",
    )?;
    let mode = field_string(&config, "mode")?;
    let path = field_string(&config, "path")?;
    let role = field_string(&config, "role")?;
    require(
        matches!(mode.as_str(), "cold" | "warm" | "reconnectLifecycle"),
        "invalid mode",
    )?;
    #[cfg(feature = "acceptance-native-family-page20")]
    const PAGE20_ENABLED: bool = true;
    #[cfg(not(feature = "acceptance-native-family-page20"))]
    const PAGE20_ENABLED: bool = false;
    validate_role_path(&role, &path, PAGE20_ENABLED)?;
    let connector_config = config
        .get("connector")
        .ok_or_else(|| Error::Protocol("missing connector".into()))?;
    require(
        connector_config["identity"] == "native",
        "Native family requires native connector identity",
    )?;
    require(
        connector_config["expectedVersion"] == "0.9.3",
        "expected Native runtime must be 0.9.3",
    )?;
    let parent_id = field_string(&config, "parentSessionId")?;
    let child_id = field_string(&config, "childSessionId")?;
    require(
        !parent_id.is_empty() && !child_id.is_empty() && parent_id != child_id,
        "parent and child pointers must differ",
    )?;
    require(
        config["parentDraft"] == "AKFAMILY_PARENT_DRAFT_KEEP",
        "fixed parent draft token mismatch",
    )?;
    require(
        config["caseIndex"].as_u64().is_some(),
        "caseIndex is required",
    )?;
    let source_history: BoundedChildHistoryPage = serde_json::from_value(
        config
            .get("sourceHistory")
            .cloned()
            .ok_or_else(|| Error::Protocol("sourceHistory required".into()))?,
    )?;
    source_history.validate(&BoundedChildHistoryRequest {
        parent_session_id: parent_id.clone(),
        child_session_id: child_id.clone(),
        through_sequence: None,
        before_sequence: None,
        max_messages: 200,
        history_revision: None,
    })?;
    require(
        !source_history.has_more_before && !source_history.run_open,
        "input sourceHistory must be full200 EOF for a closed child",
    )?;

    let start_ns = monotonic_ns();
    let parent_scope = RunScope::main("native", parent_id.clone());
    parent_scope.validate()?;
    let connector = Arc::new(KernelConnector::new(
        "native",
        &field_string(connector_config, "url")?,
        field_string(connector_config, "bearer")?,
    )?);
    let calls = Arc::new(CallLog::default());
    let observed = Arc::new(ObservedConnector::new(connector, calls.clone()));
    let runtime = Arc::new(SessionRuntime::spawn("native", observed.clone()));
    let controller = match RunController::new(runtime.clone()) {
        Ok(controller) => controller,
        Err(error) => {
            let message = error.to_string();
            let _ = cleanup_runtime(&runtime, Some(&message)).await;
            return Err(error);
        }
    };
    let parent_destination = ComposerDestination::run(parent_scope.clone());
    let prepared = controller.select(parent_destination.clone()).and_then(|_| {
        controller.edit(
            &parent_destination,
            ComposerDraft {
                text: "AKFAMILY_PARENT_DRAFT_KEEP".into(),
                ..Default::default()
            },
        )
    });
    if let Err(error) = prepared {
        let message = error.to_string();
        let _ = cleanup_runtime(&runtime, Some(&message)).await;
        return Err(error);
    }

    let is_cold = mode == "cold";
    let rounds: Vec<(u64, bool)> = if is_cold {
        vec![(0, true)]
    } else {
        (0..=10).map(|round| (round, round > 0)).collect()
    };
    emit(
        json!({"event":"consumer_started","mode":mode,"path":path,"role":role,"caseIndex":config["caseIndex"],"endpointId":"native","intentMonotonicNs":start_ns,"topology":"native-bounded-family"}),
    );
    let mut emitted_runtime_ready = false;

    let mut successful_pages = Vec::new();
    let mut failed = None;
    for (round, measured) in rounds {
        let context = json!({"round":round,"caseIndex":config["caseIndex"],"measured":measured,"path":path,"role":role,
            "parentScope":parent_scope,"childId":child_id,"parentScopeId":parent_id});
        let round_page_start = successful_pages.len();
        let start = monotonic_ns();
        emit(
            json!({"event":"iteration_started","iteration":round,"round":round,"measured":measured,
            "mode":mode,"path":path,"role":role,"caseIndex":config["caseIndex"],"topology":"native-bounded-family","monotonicNs":start}),
        );
        emit(
            json!({"event":"family_iteration_started","round":round,"measured":measured,"path":path,"role":role,
            "caseIndex":config["caseIndex"],"parentScope":parent_scope,"childId":child_id,"intentMonotonicNs":start}),
        );
        calls.set_phase("parent-open-and-catalog");
        let operation: Result<()> = async {
            let ready_intent = monotonic_ns();
            runtime.wait_ready(Duration::from_secs(30)).await?;
            if !emitted_runtime_ready {
                let host = runtime.inspect(|store| store.host.clone());
                require(host.as_ref().is_some_and(|host| host.product.version == "0.9.3"),
                    "connected Native runtime version differs from frozen 0.9.3")?;
                emit(json!({"event":"family_consumer_started","host":host,"expectedVersion":"0.9.3","mode":mode,"path":path,"role":role,
                    "caseIndex":config["caseIndex"],"page20Feature":PAGE20_ENABLED,"readOnly":true,"processSemantics":"one consumer PID; parent task and same-session bounded child; no child prompt/cancel"}));
                emit(json!({"event":"runtime_ready","intentMonotonicNs":ready_intent,
                    "readyMonotonicNs":monotonic_ns(),"status":"connectivity-ready-only-not-child-history-acceptance"}));
                emitted_runtime_ready = true;
            }
            controller.open(parent_scope.clone()).await?;
            let catalog = runtime.subagent_catalog(&parent_id).await?;
            require(catalog.entries.iter().filter(|entry| entry.kind == "child").count() == 1, "Native family seed must expose exactly one child")?;
            require(catalog.child(&child_id).is_some_and(|entry| entry.mode.as_deref() == Some("bounded")), "catalog child pointer is not the fixed bounded child")?;
            let availability = bounded_child_availability(&catalog, &child_id)?;
            emit(json!({"event":"family_catalog","round":round,"caseIndex":config["caseIndex"],"measured":measured,
                "path":path,"role":role,"parentScope":parent_scope,"parentSessionId":parent_id,
                "childId":child_id,"publicCatalog":catalog,"childEntry":catalog.child(&child_id),
                "childAvailability":availability}));
            require_parent_draft(&controller)?;
            let intent = if is_cold { start_ns } else { monotonic_ns() };
            calls.set_phase("childrun-open-and-first-snapshot");
            let childrun = open_child(runtime.clone(), parent_scope.clone(), child_id.clone(), &path).await?;
            let snapshot = childrun.transcript().await?;
            let publication = monotonic_ns();
            let parent_draft = require_parent_draft(&controller)?;
            require(!snapshot.is_stale, "child transcript is stale")?;
            require(!serde_json::to_string(&snapshot)?.contains("AKFAMILY_PARENT_DRAFT_KEEP"),
                "parent draft leaked into read-only child transcript")?;
            let child_calls = call_events(&calls, &context);
            require_page_size(&child_calls, if path == "common50" { 50 } else { 20 })?;
            let first_pages = pages_in_calls(&child_calls)?;
            let head_more = first_pages.first().ok_or_else(|| Error::Protocol("product head missing".into()))?.has_more_before;
            let first_projection = product_projection_evidence(&parent_scope, &child_id, &snapshot, &first_pages, head_more)?;
            emit(json!({"event":"family_first_snapshot","round":round,"caseIndex":config["caseIndex"],"measured":measured,"path":path,"role":role,
                "parentScope":parent_scope,"childId":child_id,"intentMonotonicNs":intent,"publicationMonotonicNs":publication,
                "latencyNs":publication.saturating_sub(intent),"transcript":snapshot,"parentDraft":parent_draft,
                "publicCatalog":catalog,"childAvailability":availability,
                "sourceCall":first_pages.first().and_then(|page|page.source_call.clone()),
                "rawProjectionTranscript":first_projection["rawProjectionTranscript"],
                "productSourceDecoration":first_projection["productSourceDecoration"],
                "productProjectionMatchesRawSource":true,
                "sourceChecks":{"catalogPointer":true,"boundedReadOnly":true,"freshTranscript":true,"parentDraftIsolated":true},
                "boundedCalls":child_calls}));

            if path == "common50" {
                let sep = sep20_projection(&runtime, &parent_scope, &child_id, &calls, &context).await?;
                let sep_pages = pages_in_calls(sep["rawCalls"].as_array().map(Vec::as_slice).unwrap_or(&[]))?;
                for page in sep_pages { successful_pages.push(page); }
                emit(sep);
                let earlier_intent = monotonic_ns();
                calls.set_phase("product-childrun-earlier50");
                let loaded = childrun.load_earlier().await?;
                let render_earlier_calls = call_events(&calls, &context);
                calls.set_phase("product-childrun-refresh-after-earlier50");
                let transcript = childrun.transcript().await?;
                let publication = monotonic_ns();
                require(loaded, "ChildRun50 did not load Earlier")?;
                require_parent_draft(&controller)?;
                let refresh_calls = call_events(&calls, &context);
                require_product_earlier(&child_calls, &render_earlier_calls, 50)?;
                let render_pages = pages_in_calls(&render_earlier_calls)?;
                let expected_more = render_pages.last().ok_or_else(|| Error::Protocol("render Earlier source missing".into()))?.has_more_before;
                let mut loaded_pages = first_pages.clone();
                loaded_pages.extend(render_pages);
                let product_projection = product_projection_evidence(&parent_scope, &child_id, &transcript, &loaded_pages, expected_more)?;
                let mut page_calls = render_earlier_calls.clone();
                page_calls.extend(refresh_calls);
                // Prefix-validation replies are recorded, validated, and timed but are not render pages.
                let _ = pages_in_calls(&page_calls)?;
                require(has_earlier_request(&page_calls), "ChildRun50 did not issue a real Earlier request")?;
                let round_pages = loaded_pages;
                successful_pages.extend(round_pages.iter().cloned());
                emit(json!({"event":"family_product_earlier50","round":round,"caseIndex":config["caseIndex"],"measured":measured,"path":path,"role":role,
                    "parentScope":parent_scope,"childId":child_id,"mechanism":"ChildRun::open default50 -> actual load_earlier",
                    "requestedMessages":50,"intentMonotonicNs":earlier_intent,"publicationMonotonicNs":publication,
                    "latencyNs":publication.saturating_sub(earlier_intent),"loaded":loaded,"transcript":transcript,
                    "rawRequests":page_calls.iter().map(|call|call["request"].clone()).collect::<Vec<_>>(),
                    "rawResponses":page_calls.iter().map(|call|call["response"].clone()).collect::<Vec<_>>(),
                    "boundedCalls":page_calls,"renderEarlierCalls":render_earlier_calls,
                    "rawProjectionTranscript":product_projection["rawProjectionTranscript"],
                    "productSourceDecoration":product_projection["productSourceDecoration"],
                    "productProjectionMatchesRawSource":true,"sourceChecks":{"loadedHistoryRetained":true,"noRawEntryCountAssumption":true}}));
                do_reconnect_if_needed(ReconnectContext { runtime: &runtime, controller: &controller, childrun: &childrun,
                    parent: &parent_scope, parent_id: &parent_id, child_id: &child_id, calls: &calls, observed: &observed,
                    context: &context, mode: &mode, round, measured }, &mut successful_pages, &round_pages, &transcript).await?;
            } else {
                let earlier_intent = monotonic_ns();
                calls.set_phase("product-childrun-earlier20");
                let loaded = childrun.load_earlier().await?;
                let render_earlier_calls = call_events(&calls, &context);
                calls.set_phase("product-childrun-refresh-after-earlier20");
                let transcript = childrun.transcript().await?;
                let publication = monotonic_ns();
                require(loaded, "ChildRun20 did not load Earlier")?;
                require_parent_draft(&controller)?;
                let refresh_calls = call_events(&calls, &context);
                require_product_earlier(&child_calls, &render_earlier_calls, 20)?;
                let render_pages = pages_in_calls(&render_earlier_calls)?;
                let expected_more = render_pages.last().ok_or_else(|| Error::Protocol("render Earlier source missing".into()))?.has_more_before;
                let mut loaded_pages = first_pages.clone();
                loaded_pages.extend(render_pages);
                let product_projection = product_projection_evidence(&parent_scope, &child_id, &transcript, &loaded_pages, expected_more)?;
                let mut page_calls = render_earlier_calls.clone();
                page_calls.extend(refresh_calls);
                // Prefix-validation replies are recorded, validated, and timed but are not render pages.
                let _ = pages_in_calls(&page_calls)?;
                require(has_earlier_request(&page_calls), "ChildRun20 did not issue a real Earlier request")?;
                let round_pages = loaded_pages;
                successful_pages.extend(round_pages.iter().cloned());
                let mut product_calls = child_calls.clone();
                product_calls.extend(render_earlier_calls.iter().cloned());
                let raw_union = union_from_calls(&product_calls)?;
                emit(json!({"event":"family_history_page","round":round,"caseIndex":config["caseIndex"],"measured":measured,"path":path,"role":role,
                    "parentScope":parent_scope,"childId":child_id,"mechanism":"childrun20","requestedMessages":20,
                    "intentMonotonicNs":earlier_intent,"publicationMonotonicNs":publication,
                    "latencyNs":publication.saturating_sub(earlier_intent),"transcript":transcript,
                    "rawHeadRequests":child_calls.iter().map(|call|call["request"].clone()).collect::<Vec<_>>(),
                    "rawHeadResponses":child_calls.iter().map(|call|call["response"].clone()).collect::<Vec<_>>(),
                    "rawEarlierRequests":page_calls.iter().map(|call|call["request"].clone()).collect::<Vec<_>>(),
                    "rawEarlierResponses":page_calls.iter().map(|call|call["response"].clone()).collect::<Vec<_>>(),
                    "rawUnion":raw_union,"projection":transcript.clone(),"rawCalls":page_calls,
                    "rawHeadCalls":child_calls,"renderEarlierCalls":render_earlier_calls,
                    "rawProjectionTranscript":product_projection["rawProjectionTranscript"],
                    "productSourceDecoration":product_projection["productSourceDecoration"],
                    "productProjectionMatchesRawSource":true,
                    "sourceChecks":{"actualEarlierApplied":true,"loadedHistoryRetained":true,
                        "rawEntryCountIsNotRequestedMessageCount":true}}));
                do_reconnect_if_needed(ReconnectContext { runtime: &runtime, controller: &controller, childrun: &childrun,
                    parent: &parent_scope, parent_id: &parent_id, child_id: &child_id, calls: &calls, observed: &observed,
                    context: &context, mode: &mode, round, measured }, &mut successful_pages, &round_pages, &transcript).await?;
            }
            validate_groundtruth(&config, &successful_pages[round_page_start..])?;
            Ok(())
        }.await;
        match operation {
            Ok(()) => emit(
                json!({"event":"family_iteration_completed","round":round,"caseIndex":config["caseIndex"],"measured":measured,"path":path,"role":role,
                "parentScope":parent_scope,"childId":child_id,"completionMonotonicNs":monotonic_ns()}),
            ),
            Err(error) => {
                let error = error.to_string();
                for event in call_events(&calls, &context) {
                    emit(event);
                }
                emit(
                    json!({"event":"family_iteration_failed","round":round,"caseIndex":config["caseIndex"],"measured":measured,"path":path,"role":role,
                    "parentScope":parent_scope,"childId":child_id,"failureMonotonicNs":monotonic_ns(),"error":error}),
                );
                failed = Some(error);
                break;
            }
        }
    }
    if failed.is_none() {
        match validate_groundtruth(&config, &successful_pages) {
            Ok(event) => emit(event),
            Err(error) => {
                let message = error.to_string();
                let round = if mode == "cold" { 0 } else { 10 };
                emit(
                    json!({"event":"family_iteration_failed","round":round,"caseIndex":config["caseIndex"],
                    "measured":true,"path":path,"role":role,"parentScope":parent_scope,"childId":child_id,
                    "failureMonotonicNs":monotonic_ns(),"phase":"post-measurement-groundtruth","error":message}),
                );
                failed = Some(message);
            }
        }
    }
    if let Err(error) = cleanup_runtime(&runtime, failed.as_deref()).await
        && failed.is_none()
    {
        failed = Some(format!("runtime cleanup failed: {error}"));
    }
    if let Some(error) = failed {
        return Err(Error::Protocol(error));
    }
    Ok(())
}

struct ReconnectContext<'a> {
    runtime: &'a SessionRuntime,
    controller: &'a RunController,
    childrun: &'a ChildRun,
    parent: &'a RunScope,
    parent_id: &'a str,
    child_id: &'a str,
    calls: &'a CallLog,
    observed: &'a ObservedConnector,
    context: &'a Value,
    mode: &'a str,
    round: u64,
    measured: bool,
}

async fn do_reconnect_if_needed(
    inputs: ReconnectContext<'_>,
    successful_pages: &mut Vec<BoundedChildHistoryPage>,
    source_before: &[BoundedChildHistoryPage],
    before: &TranscriptSnapshot,
) -> Result<()> {
    let ReconnectContext {
        runtime,
        controller,
        childrun,
        parent,
        parent_id,
        child_id,
        calls,
        observed,
        context,
        mode,
        round,
        measured,
    } = inputs;
    if mode != "reconnectLifecycle" {
        return Ok(());
    }
    let intent = monotonic_ns();
    let before_ordinal = observed.connection_ordinal();
    let before_epoch = runtime.inspect(|store| store.connection_epoch);
    calls.set_phase("reconnect-and-same-child-refresh");
    runtime.reconnect().await?;
    runtime.wait_ready(Duration::from_secs(30)).await?;
    let after_ordinal = observed.connection_ordinal();
    let after_epoch = runtime.inspect(|store| store.connection_epoch);
    require(
        after_ordinal > before_ordinal,
        "runtime reconnect did not create a new endpoint connection",
    )?;
    require(
        after_epoch > before_epoch,
        "runtime reconnect did not advance connection epoch",
    )?;
    controller.open(parent.clone()).await?;
    let catalog = runtime.subagent_catalog(parent_id).await?;
    require(
        catalog
            .child(child_id)
            .is_some_and(|entry| entry.mode.as_deref() == Some("bounded")),
        "bounded child disappeared across reconnect",
    )?;
    let availability = bounded_child_availability(&catalog, child_id)?;
    emit(
        json!({"event":"family_catalog","round":round,"caseIndex":context["caseIndex"],"measured":measured,
        "path":context["path"],"role":context["role"],"parentScope":parent,"parentSessionId":parent_id,
        "childId":child_id,"publicCatalog":catalog,"childEntry":catalog.child(child_id),
        "childAvailability":availability}),
    );
    let after = childrun.transcript().await?;
    let publication = monotonic_ns();
    require(
        before == &after,
        "same ChildRun history changed across reconnect",
    )?;
    let parent_draft = require_parent_draft(controller)?;
    let reconnect_calls = call_events(calls, context);
    let source_after = pages_in_calls(&reconnect_calls)?;
    require(
        !source_before.is_empty() && !source_after.is_empty(),
        "reconnect lacked raw before/after bounded source pages",
    )?;
    for page in &source_after {
        require(
            page.parent_session_id == parent_id && page.child_session_id == child_id,
            "reconnect source pointer changed",
        )?;
        require_page_cut_revision(page, &source_before[0])?;
    }
    let mut restored_pages = source_before.to_vec();
    restored_pages.extend(source_after.iter().cloned());
    let source_more = source_before
        .last()
        .ok_or_else(|| Error::Protocol("loaded source pages missing".into()))?
        .has_more_before;
    let projection =
        product_projection_evidence(parent, child_id, &after, &restored_pages, source_more)?;
    successful_pages.extend(source_after.iter().cloned());
    emit(
        json!({"event":"family_reconnect","round":round,"caseIndex":context["caseIndex"],"measured":measured,"path":context["path"],"role":context["role"],
        "parentScope":parent,"childId":child_id,"intentMonotonicNs":intent,"publicationMonotonicNs":publication,
        "latencyNs":publication.saturating_sub(intent),"childRunInstanceRetained":true,"endpointOrdinalBeforeReconnect":before_ordinal,
        "endpointOrdinalAfterReconnect":after_ordinal,"connectionEpochBeforeReconnect":before_epoch,
        "connectionEpochAfterReconnect":after_epoch,"beforeTranscript":before,"afterTranscript":after,"parentDraft":parent_draft,
        "sourceBefore":source_before,"sourceAfter":source_after,
        "rawProjectionTranscript":projection["rawProjectionTranscript"],
        "productSourceDecoration":projection["productSourceDecoration"],"productProjectionMatchesRawSource":true,
        "publicCatalog":catalog,"childAvailability":availability,
        "sourceCall":source_after.first().and_then(|page|page.source_call.clone()),
        "sourceChecks":{"catalogPointerRetained":true,"loadedHistoryRetained":true,"sourceCallRetained":true,"cutRetained":true},
        "converged":true,"boundedCalls":reconnect_calls}),
    );
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut input = String::new();
    let result = std::io::stdin()
        .take(2 * 1024 * 1024)
        .read_to_string(&mut input)
        .map_err(|error| Error::Protocol(error.to_string()))
        .map(|_| ())
        .and_then(|_| serde_json::from_str::<Value>(&input).map_err(Error::from))
        .map_err(|error| (error, Value::Null));
    let result = match result {
        Ok(config) => run(config.clone()).await.map_err(|error| (error, config)),
        Err(error) => Err(error),
    };
    if let Err((error, _)) = result {
        emit(json!({"event":"consumer_error","error":error.to_string()}));
        std::process::exit(1);
    }
}

#[cfg(test)]
mod projection_tests {
    use super::*;

    #[test]
    fn product_snapshot_must_match_raw_projection_after_only_task_decoration() {
        let parent = RunScope {
            endpoint_id: "native".into(),
            thread_id: "parent".into(),
            session_id: "parent".into(),
        };
        let source =
            json!({"callId":"source", "name":"task", "arguments":{"goal":"proof"}, "phase":"done"});
        let page = BoundedChildHistoryPage {
            parent_session_id: "parent".into(),
            child_session_id: "child".into(),
            through_sequence: -1,
            history_revision: "a".repeat(64),
            entries: vec![],
            has_more_before: false,
            run_open: false,
            source_call: Some(source.clone()),
        };
        let projection = SessionProjection {
            journal_ready: true,
            tail_sequence: -1,
            ..Default::default()
        };
        let mut actual = serde_json::to_value(
            TranscriptSnapshot::read(&child_scope(&parent, "child"), &projection).unwrap(),
        )
        .unwrap();
        let id = json!(["native", "child", "task-source", "source"]).to_string();
        actual["turns"] = json!([{"id":id,"input":"","status":"completed","startedAt":null,"endedAt":null,"usage":{"inputTokens":0,"outputTokens":0}}]);
        actual["tools"] = json!([{"id":format!("{id}:call"),"turnId":id,"name":"task","arguments":source["arguments"],"phase":"done","result":null}]);
        let mut actual: TranscriptSnapshot = serde_json::from_value(actual).unwrap();
        assert!(
            product_projection_evidence(
                &parent,
                "child",
                &actual,
                std::slice::from_ref(&page),
                false
            )
            .is_ok()
        );
        actual.has_more_before = true;
        assert!(product_projection_evidence(&parent, "child", &actual, &[page], false).is_err());
    }
}
