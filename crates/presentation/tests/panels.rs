use serde_json::json;
use tekes_agent_contract::*;
use tekes_agent_presentation::*;
use tekes_agent_session::*;
fn scope(id: &str) -> RunScope {
    RunScope::main("e", id)
}
fn catalog(available: bool) -> SubagentCatalog {
    serde_json::from_value(json!({"parentAvailable":available,"entries":[{"kind":"child","id":"c","mode":"continuable"},{"kind":"child","id":"one","mode":"one-shot"},{"kind":"parent","id":"p","mode":"continuable"}]})).unwrap()
}
fn task(session: &str) -> AgentTask {
    AgentTask {
        id: "task".into(),
        scope: RunScope {
            session_id: session.into(),
            ..scope("p")
        },
        name: "audit".into(),
        status: "working".into(),
        kind: "task".into(),
    }
}
#[test]
fn task_pointer_follows_live_row_and_rejects_stale_or_foreign_results() {
    let mut state = TaskPanelState::default();
    let old = state.begin(scope("p"), "task").unwrap();
    state.begin(scope("other"), "other").unwrap();
    let token = state.begin(scope("p"), "task").unwrap();
    assert!(!state.commit(&old, Ok(Some(task("old")))));
    assert!(state.commit(&token, Ok(Some(task("first")))));
    assert_eq!(
        state
            .value
            .as_ref()
            .unwrap()
            .independent_run()
            .unwrap()
            .session_id,
        "first"
    );
    assert!(state.commit(&token, Ok(Some(task("replacement")))));
    assert_eq!(
        state.value.as_ref().unwrap().scope.session_id,
        "replacement"
    );
    let mut foreign = task("c");
    foreign.scope.endpoint_id = "other".into();
    assert!(!state.commit(&token, Ok(Some(foreign))));
    assert!(state.commit(&token, Ok(None)));
    assert!(state.resolved);
    assert!(state.value.is_none());
    state.cancel();
    assert!(!state.commit(&token, Ok(Some(task("c")))));
    assert!(task("p").independent_run().is_none());
}
#[test]
fn catalog_authority_is_parent_owned_and_cleared_on_disconnect_and_failed_refresh() {
    let mut state = ChildCatalogState::default();
    let old = state.begin(scope("p"), true).unwrap();
    assert_eq!(state.availability("c"), ChildComposerAvailability::Loading);
    assert!(state.commit(&old, Ok(catalog(true))));
    assert_eq!(
        state.availability("c"),
        ChildComposerAvailability::Available
    );
    assert_eq!(
        state.availability("one"),
        ChildComposerAvailability::OneShot
    );
    assert_eq!(
        state.availability("p"),
        ChildComposerAvailability::Unavailable
    );
    state.begin(scope("p"), false);
    assert!(!state.commit(&old, Ok(catalog(true))));
    assert_eq!(
        state.availability("c"),
        ChildComposerAvailability::Unavailable
    );
    let new = state.begin(scope("p"), true).unwrap();
    state.commit(&new, Ok(catalog(false)));
    assert_eq!(
        state.availability("c"),
        ChildComposerAvailability::ParentUnavailable
    );
    state.commit(&new, Err("read failed".into()));
    assert!(state.catalog.is_none());
    assert_eq!(
        state.availability("c"),
        ChildComposerAvailability::Unavailable
    );
    assert!(catalog(true).require_continuable("one").is_err());
    assert!(catalog(false).require_continuable("c").is_err());
}
fn event(seq: i64, kind: &str, data: serde_json::Value) -> HistoryEntry {
    serde_json::from_value(json!({"event":{"seq":seq,"time":100,"type":kind,"data":data}})).unwrap()
}
#[test]
fn authoritative_todos_and_usage_survive_truncation_and_only_newer_events_replace_them() {
    let mut session = SessionProjection::default();
    session.projections.insert(
        "todos".into(),
        VersionedValue {
            sequence: 10,
            value: json!([{"content":"durable todo","status":"completed"}]),
        },
    );
    session.projections.insert("tokenUsage".into(), VersionedValue { sequence: 10, value: json!({"uncachedInputTokens":10,"cacheReadTokens":20,"cacheWriteTokens":10,"outputTokens":4}) });
    session
        .entries
        .insert(1, event(1, "todo/write", json!({"todos":[]})));
    let facts = SummaryFacts::read(&scope("p"), &session, &TranscriptSnapshot::default()).unwrap();
    assert_eq!(facts.tasks.len(), 1);
    assert!(facts.tasks[0].independent_run().is_none());
    let usage = facts.usage.unwrap();
    assert_eq!(usage.total_tokens(), 44);
    assert_eq!(usage.cache_hit_rate(), Some(0.5));
    assert_eq!(usage.known_cost_usd(), None);
    session
        .entries
        .insert(11, event(11, "todo/write", json!({"todos":[]})));
    assert!(
        SummaryFacts::read(&scope("p"), &session, &TranscriptSnapshot::default())
            .unwrap()
            .tasks
            .is_empty()
    );
    session.projections.get_mut("tokenUsage").unwrap().value = json!({"outputTokens":-1});
    assert!(SummaryFacts::read(&scope("p"), &session, &TranscriptSnapshot::default()).is_err());
}
#[test]
fn task_titles_never_use_result_brief_and_subagents_resolve_native_startup_uuid() {
    let mut session = SessionProjection::default();
    session
        .entries
        .insert(0, event(0, "turn/start", json!({"turn":1})));
    session.entries.insert(1, event(1, "tool/call", json!({"turn":1,"step":1,"callId":"t","name":"task","arguments":{"task_name":"producer name","brief":"wrong"}})));
    session.entries.insert(2, event(2, "tool/call", json!({"turn":1,"step":1,"callId":"s","name":"subagent","arguments":{"description":"audit child"}})));
    let id = "b437cf34-5661-45e2-a674-113b27c92f89";
    session.entries.insert(3, event(3, "tool/result", json!({"turn":1,"step":1,"message":{"source":{"callId":"s"},"content":[{"type":"text","text":format!("Started child {id}")}]}})));
    let transcript = TranscriptSnapshot::read(&scope("p"), &session).unwrap();
    let facts = SummaryFacts::read(&scope("p"), &session, &transcript).unwrap();
    assert_eq!(facts.tasks[0].name, "producer name");
    assert_eq!(facts.subagents[0].title, "audit child");
    assert_eq!(facts.subagents[0].session_id, None);

    let mut store = MemoryProjectionStore::new("e");
    store.sessions.insert("p".into(), session);
    store
        .sessions
        .insert(id.into(), SessionProjection::default());
    let facts = SummaryFacts::read_with_store(&scope("p"), &store, &transcript).unwrap();
    assert_eq!(facts.subagents[0].session_id.as_deref(), Some(id));
}
#[test]
fn summary_provider_order_scope_dedup_and_unknown_cost_match_clientkit() {
    let usage = SummaryUsage {
        input_tokens: 0,
        cached_input_tokens: 0,
        cache_creation_input_tokens: 0,
        output_tokens: 0,
        cost_usd: Some(2.0),
        is_cost_complete: false,
    };
    assert_eq!(usage.cache_hit_rate(), None);
    assert_eq!(usage.known_cost_usd(), None);
    let facts = SummaryFacts {
        scope: scope("p"),
        tasks: vec![task("c")],
        subagents: vec![SummarySubagent {
            id: "call".into(),
            title: "child".into(),
            session_id: Some("c".into()),
            status: "running".into(),
        }],
        artifacts: vec![
            SummaryArtifact {
                id: "src/a.rs".into(),
                source: Some(" z ".into()),
            },
            SummaryArtifact {
                id: "src/a.rs".into(),
                source: Some("a".into()),
            },
        ],
        usage: Some(usage.clone()),
    };
    assert!(facts.sections(&scope("other")).is_empty());
    let sections = facts.sections(&scope("p"));
    assert_eq!(
        sections.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        vec!["tasks", "subagents", "sources", "artifacts", "usage"]
    );
    assert_eq!(
        sections[2]
            .rows
            .iter()
            .map(|r| r.title.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "z"]
    );
    assert_eq!(sections[3].rows.len(), 1);
    assert_eq!(sections[3].rows[0].title, "a.rs");
    assert_eq!(sections[4].rows[5].value, serde_json::Value::Null);
    assert_eq!(usage.known_cost_usd(), None);
    assert_eq!(
        SummaryUsage {
            cost_usd: Some(-0.01),
            is_cost_complete: true,
            ..usage.clone()
        }
        .known_cost_usd(),
        None
    );
    assert_eq!(
        SummaryUsage {
            cost_usd: Some(f64::NAN),
            is_cost_complete: true,
            ..usage.clone()
        }
        .known_cost_usd(),
        None
    );
    let mut state = SummaryState::default();
    let old = state.begin(scope("p"));
    state.begin(scope("other"));
    let new = state.begin(scope("p"));
    assert!(!state.commit(&old, Ok(facts.clone())));
    assert!(state.commit(&new, Ok(facts)));
    assert!(state.commit(&new, Err("failed query".into())));
    assert!(state.sections(&scope("p")).is_empty());
}

#[test]
fn loaded_summary_without_usage_projection_keeps_unavailable_usage_rows() {
    let facts = SummaryFacts {
        scope: scope("p"),
        tasks: vec![],
        subagents: vec![],
        artifacts: vec![],
        usage: None,
    };

    let sections = facts.sections(&scope("p"));
    let usage = sections
        .iter()
        .find(|section| section.id == "usage")
        .unwrap();
    assert_eq!(
        usage
            .rows
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        ["total", "input", "output", "cache", "hit-rate", "cost"]
    );
    assert!(
        usage
            .rows
            .iter()
            .all(|row| { row.action.is_none() && row.value == json!("unavailable") })
    );
}

#[test]
fn summary_projection_skips_invalid_rows_and_rejects_invalid_facts_without_losing_siblings() {
    let parent = scope("p");
    let mut invalid_task_scope = task("");
    invalid_task_scope.id = "invalid-scope".into();
    let mut blank_task_id = task("c");
    blank_task_id.id = "  ".into();
    let mut blank_task_name = task("c");
    blank_task_name.id = "blank-name".into();
    blank_task_name.name = " \t ".into();
    let mut valid_sibling = task("c");
    valid_sibling.id = "valid-sibling".into();
    let mut blank_pointer = SummarySubagent {
        id: "known-call".into(),
        title: "Known subagent".into(),
        session_id: Some("  \t".into()),
        status: String::new(),
    };
    blank_pointer.status = "running".into();
    let blank_title = SummarySubagent {
        id: "blank-title".into(),
        title: " \t ".into(),
        session_id: Some("c".into()),
        status: String::new(),
    };
    let invalid_facts = SummaryFacts {
        scope: parent.clone(),
        tasks: vec![
            valid_sibling,
            invalid_task_scope,
            blank_task_id,
            blank_task_name.clone(),
        ],
        subagents: vec![
            blank_pointer,
            blank_title,
            SummarySubagent {
                id: " \t".into(),
                title: "Invalid identity".into(),
                session_id: Some("c".into()),
                status: String::new(),
            },
        ],
        artifacts: vec![
            SummaryArtifact {
                id: "".into(),
                source: Some("empty path".into()),
            },
            SummaryArtifact {
                id: " \t".into(),
                source: Some("blank path".into()),
            },
            SummaryArtifact {
                id: "src/valid.rs".into(),
                source: None,
            },
        ],
        usage: None,
    };

    let sections = invalid_facts.sections(&parent);
    let task_rows = &sections
        .iter()
        .find(|section| section.id == "tasks")
        .unwrap()
        .rows;
    let subagent_rows = &sections
        .iter()
        .find(|section| section.id == "subagents")
        .unwrap()
        .rows;
    let artifact_rows = &sections
        .iter()
        .find(|section| section.id == "artifacts")
        .unwrap()
        .rows;
    assert_eq!(task_rows.len(), 2);
    assert_eq!(task_rows[0].id, "valid-sibling");
    assert!(matches!(
        task_rows[0].action.as_ref(),
        Some(SummaryAction::OpenTask { .. })
    ));
    assert_eq!(task_rows[1].id, "blank-name");
    assert_eq!(task_rows[1].title, " \t ");
    assert!(matches!(
        task_rows[1].action.as_ref(),
        Some(SummaryAction::OpenTask { .. })
    ));
    assert_eq!(subagent_rows.len(), 2);
    assert_eq!(subagent_rows[0].id, "known-call");
    assert!(subagent_rows[0].action.is_none());
    assert_eq!(subagent_rows[1].id, "blank-title");
    assert_eq!(subagent_rows[1].title, " \t ");
    assert!(subagent_rows[1].action.is_some());
    assert_eq!(artifact_rows.len(), 1);
    assert_eq!(artifact_rows[0].id, "src/valid.rs");
    assert!(matches!(
        artifact_rows[0].action.as_ref(),
        Some(SummaryAction::OpenArtifact { path, .. }) if path == "src/valid.rs"
    ));
    assert!(invalid_facts.validate().is_err());

    let mut state = SummaryState::default();
    let old = state.begin(parent.clone());
    let current = state.begin(parent.clone());
    let mut valid_old_result = invalid_facts.clone();
    valid_old_result.tasks = vec![task("c"), blank_task_name];
    valid_old_result.subagents = vec![SummarySubagent {
        id: "blank-title".into(),
        title: " \t ".into(),
        session_id: Some("c".into()),
        status: String::new(),
    }];
    valid_old_result.artifacts = vec![];
    assert!(valid_old_result.validate().is_ok());
    assert!(!state.commit(&old, Ok(valid_old_result)));
    assert!(!state.commit(&current, Ok(invalid_facts)));
    assert!(state.facts.is_none());
    assert!(state.sections(&parent).is_empty());
}
#[test]
fn confirmed_discard_prevents_late_failure_from_resurrecting_drafts() {
    let mut authority = MemoryProjectionStore::new("e");
    authority.connection = ConnectionState::Ready;
    authority.endpoint_capabilities = EndpointCapabilities::methods(&["session.prompt"]);
    authority.sessions.insert(
        "p".into(),
        SessionProjection {
            journal_ready: true,
            ..Default::default()
        },
    );
    let destination = ComposerDestination::run(scope("p"));
    let mut store = PresentationStore::default();
    store.select(destination.clone()).unwrap();
    store
        .edit(
            &destination,
            ComposerDraft {
                text: "draft".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let ticket = store
        .prepare(&destination, PromptMode::Queue, &authority)
        .unwrap();
    store.mark_dispatched(&ticket).unwrap();
    store.forget_destination(&destination);
    store
        .finish(ticket.id(), Err("late failure".into()))
        .unwrap();
    assert_eq!(store.draft(&destination).text, "");
}

#[test]
fn child_composer_authority_is_fenced_even_when_connection_notifications_coalesce() {
    let mut authority = MemoryProjectionStore::new("e");
    authority.connection = ConnectionState::Ready;
    authority.endpoint_capabilities =
        EndpointCapabilities::methods(&["session.prompt", "subagents.prompt"]);
    authority.sessions.insert(
        "c".into(),
        SessionProjection {
            journal_ready: true,
            ..Default::default()
        },
    );
    let child = RunScope {
        session_id: "c".into(),
        ..scope("p")
    };
    let mut core = PresentationStore::default();
    let token = core
        .select(ComposerDestination::run(child.clone()))
        .unwrap();
    assert!(!core.refresh(&authority).unwrap().can_submit);
    assert!(core.commit_child_catalog(&token, authority.connection_epoch, Ok(catalog(true))));
    assert!(core.refresh(&authority).unwrap().can_submit);
    authority.disconnect();
    authority.connection = ConnectionState::Ready;
    authority.sessions.get_mut("c").unwrap().journal_ready = true;
    assert!(!core.refresh(&authority).unwrap().can_submit);
    assert!(!core.commit_child_catalog(&token, authority.connection_epoch - 1, Ok(catalog(true))));
    assert!(!core.refresh(&authority).unwrap().can_submit);
}
#[test]
fn malformed_optional_summary_does_not_disable_a_healthy_run() {
    let mut authority = MemoryProjectionStore::new("e");
    authority.connection = ConnectionState::Ready;
    authority.sessions.insert(
        "p".into(),
        SessionProjection {
            journal_ready: true,
            projections: [(
                "todos".into(),
                VersionedValue {
                    sequence: 0,
                    value: json!({"bad":true}),
                },
            )]
            .into(),
            ..Default::default()
        },
    );
    let mut core = PresentationStore::default();
    core.select(ComposerDestination::run(scope("p"))).unwrap();
    let snapshot = core.refresh(&authority).unwrap();
    assert!(snapshot.can_submit);
    assert!(snapshot.summary.is_none());
    assert!(!snapshot.transcript.diagnostics.is_empty());
}

fn summary_child_store() -> MemoryProjectionStore {
    let mut store = MemoryProjectionStore::new("e");
    store.connection = ConnectionState::Ready;
    store.inventory_ready = true;
    let child = "b437cf34-5661-45e2-a674-113b27c92f89";
    let mut parent = SessionProjection {
        journal_ready: true,
        summary: Some(json!({"status":"failed"})),
        ..Default::default()
    };
    parent
        .entries
        .insert(0, event(0, "turn/start", json!({"turn":1})));
    parent.entries.insert(1,event(1,"tool/call",json!({"turn":1,"step":1,"callId":"child-call","name":"subagent","arguments":{"brief":"真实子会话"}})));
    parent.entries.insert(2,event(2,"tool/result",json!({"turn":1,"step":1,"message":{"source":{"callId":"child-call"},"content":[{"type":"text","text":format!("Started child {child}")}]}})));
    store.sessions.insert("p".into(), parent);
    store.sessions.insert(
        child.into(),
        SessionProjection {
            summary: Some(json!({"status":"running"})),
            ..Default::default()
        },
    );
    store
}
#[test]
fn summary_child_status_comes_only_from_the_same_endpoint_session_summary() {
    let mut store = summary_child_store();
    let parent = scope("p");
    let child = "b437cf34-5661-45e2-a674-113b27c92f89";
    let mut transcript = TranscriptSnapshot::read(&parent, &store.sessions["p"]).unwrap();
    transcript.tools[0].phase = "failed".into();
    let isolated = SummaryFacts::read(&parent, &store.sessions["p"], &transcript).unwrap();
    assert_eq!(isolated.subagents[0].status, "");
    assert!(
        isolated
            .sections(&parent)
            .iter()
            .find(|s| s.id == "subagents")
            .unwrap()
            .rows[0]
            .value
            .is_null()
    );
    let read = |store: &MemoryProjectionStore| {
        SummaryFacts::read_with_store(&parent, store, &transcript)
            .unwrap()
            .subagents
            .remove(0)
    };
    assert_eq!(read(&store).status, "running");
    assert_eq!(
        read(&store).indicator(),
        Some(SummarySubagentIndicator::Running)
    );
    for (status, indicator) in [
        ("failed", Some(SummarySubagentIndicator::Failed)),
        ("interrupted", Some(SummarySubagentIndicator::Failed)),
        ("completed", None),
        ("future-status", None),
    ] {
        store.sessions.get_mut(child).unwrap().summary = Some(json!({"status":status}));
        assert_eq!(read(&store).status, status);
        assert_eq!(read(&store).indicator(), indicator);
    }
    // The public child execution flag is authoritative even without a raw status.
    store.sessions.get_mut(child).unwrap().run_open = true;
    store.sessions.get_mut(child).unwrap().summary = Some(json!({"running":true}));
    assert_eq!(read(&store).status, "running");
    store.sessions.get_mut(child).unwrap().summary = None;
    assert_eq!(read(&store).indicator(), None);
    store.sessions.remove(child);
    assert_eq!(read(&store).status, "");
    store.endpoint_id = "foreign".into();
    assert!(SummaryFacts::read_with_store(&parent, &store, &transcript).is_err());
}
#[test]
fn run_summary_refreshes_child_status_without_changing_draft_or_inventing_live_authority() {
    let mut store = summary_child_store();
    let child = "b437cf34-5661-45e2-a674-113b27c92f89";
    let mut core = PresentationStore::default();
    core.select(ComposerDestination::run(scope("p"))).unwrap();
    let first = core.refresh(&store).unwrap();
    assert_eq!(first.summary.unwrap().subagents[0].status, "running");
    store.sessions.get_mut(child).unwrap().summary = Some(json!({"status":"failed"}));
    let second = core.refresh(&store).unwrap();
    assert_eq!(
        second.summary.unwrap().subagents[0].indicator(),
        Some(SummarySubagentIndicator::Failed)
    );
    assert_eq!(second.draft, first.draft);
    assert!(second.submissions.is_empty());
    store.disconnect();
    let stale = core.refresh(&store).unwrap();
    assert_eq!(stale.summary.unwrap().subagents[0].status, "failed");
    assert!(stale.connection.history_is_stale);
    assert!(!stale.can_submit);
    assert!(stale.submissions.is_empty());
}
#[test]
fn summary_subagent_indicator_matches_clientkit_without_success_or_cancellation_inference() {
    let mut child = SummarySubagent {
        id: "call".into(),
        title: "child".into(),
        session_id: None,
        status: String::new(),
    };
    for (status, expected) in [
        ("running", Some(SummarySubagentIndicator::Running)),
        ("in_progress", Some(SummarySubagentIndicator::Running)),
        ("working", Some(SummarySubagentIndicator::Running)),
        ("failed", Some(SummarySubagentIndicator::Failed)),
        ("interrupted", Some(SummarySubagentIndicator::Failed)),
        ("", None),
        ("completed", None),
        ("succeeded", None),
        ("idle", None),
        ("cancelled", None),
        ("cancelling", None),
        ("unknown", None),
    ] {
        child.status = status.into();
        assert_eq!(child.indicator(), expected, "{status}");
    }
}

#[test]
fn summary_never_resolves_the_parent_result_uuid_as_a_child() {
    let mut store = summary_child_store();
    let id = "b437cf34-5661-45e2-a674-113b27c92f89";
    let parent = store.sessions.remove("p").unwrap();
    store.sessions.insert(id.into(), parent);
    let scope = scope(id);
    let transcript = TranscriptSnapshot::read(&scope, &store.sessions[id]).unwrap();
    let facts = SummaryFacts::read_with_store(&scope, &store, &transcript).unwrap();
    assert_eq!(facts.subagents[0].session_id, None);
    assert_eq!(facts.subagents[0].indicator(), None);
    assert!(
        facts
            .sections(&scope)
            .iter()
            .find(|s| s.id == "subagents")
            .unwrap()
            .rows[0]
            .action
            .is_none()
    );
}

#[test]
fn summary_subagent_pointer_requires_a_unique_same_endpoint_session() {
    let parent = scope("p");
    let child = "b437cf34-5661-45e2-a674-113b27c92f89";
    let mut store = summary_child_store();
    let transcript = TranscriptSnapshot::read(&parent, &store.sessions["p"]).unwrap();

    // Exact raw identity wins even if other inventory keys share its suffix.
    store
        .sessions
        .insert(format!("dsh:session:{child}"), SessionProjection::default());
    store.sessions.insert(
        format!("native:session:{child}"),
        SessionProjection::default(),
    );
    let exact = SummaryFacts::read_with_store(&parent, &store, &transcript).unwrap();
    assert_eq!(exact.subagents[0].session_id.as_deref(), Some(child));

    // A UUID in arbitrary result text is not itself authority to navigate.
    store.sessions.remove(child);
    store.sessions.remove(&format!("dsh:session:{child}"));
    store.sessions.remove(&format!("native:session:{child}"));
    let unknown = SummaryFacts::read_with_store(&parent, &store, &transcript).unwrap();
    assert_eq!(unknown.subagents[0].session_id, None);
    assert!(summary_subagent_rows(&unknown, &parent)[0].action.is_none());

    // Endpoint-qualified identities are accepted only when the raw UUID suffix is unique.
    let qualified = format!("dsh:session:{child}");
    store
        .sessions
        .insert(qualified.clone(), SessionProjection::default());
    let unique = SummaryFacts::read_with_store(&parent, &store, &transcript).unwrap();
    assert_eq!(
        unique.subagents[0].session_id.as_deref(),
        Some(qualified.as_str())
    );
    assert!(summary_subagent_rows(&unique, &parent)[0].action.is_some());

    store.sessions.insert(
        format!("native:session:{child}"),
        SessionProjection::default(),
    );
    let ambiguous = SummaryFacts::read_with_store(&parent, &store, &transcript).unwrap();
    assert_eq!(ambiguous.subagents[0].session_id, None);
    assert!(
        summary_subagent_rows(&ambiguous, &parent)[0]
            .action
            .is_none()
    );
}

fn summary_subagent_rows(facts: &SummaryFacts, scope: &RunScope) -> Vec<SummaryRow> {
    facts
        .sections(scope)
        .into_iter()
        .find(|section| section.id == "subagents")
        .expect("subagent section")
        .rows
}

#[test]
fn summary_does_not_redirect_namespaced_parent_uuid_to_another_session() {
    let raw_id = "b437cf34-5661-45e2-a674-113b27c92f89";
    let parent_id = format!("dsh:session:{raw_id}");
    let mut store = summary_child_store();
    let parent_session = store.sessions.remove("p").unwrap();
    store.sessions.insert(parent_id.clone(), parent_session);
    store.sessions.insert(
        format!("native:session:{raw_id}"),
        SessionProjection::default(),
    );
    let parent = scope(&parent_id);
    let transcript = TranscriptSnapshot::read(&parent, &store.sessions[&parent_id]).unwrap();
    let facts = SummaryFacts::read_with_store(&parent, &store, &transcript).unwrap();
    assert_eq!(facts.subagents[0].session_id, None);
    assert!(summary_subagent_rows(&facts, &parent)[0].action.is_none());
}

#[test]
fn host_projected_summary_keeps_namespaced_parent_alias_as_a_passive_row() {
    let raw_id = "b437cf34-5661-45e2-a674-113b27c92f89";
    let parent = scope(&format!("dsh:session:{raw_id}"));
    let facts = SummaryFacts {
        scope: parent.clone(),
        tasks: vec![],
        subagents: vec![SummarySubagent {
            id: "child-call".into(),
            title: "Known child".into(),
            session_id: Some(raw_id.into()),
            status: String::new(),
        }],
        artifacts: vec![],
        usage: None,
    };
    assert!(facts.validate().is_ok());
    let rows = summary_subagent_rows(&facts, &parent);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title, "Known child");
    assert!(rows[0].action.is_none());
}
