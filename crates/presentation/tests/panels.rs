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
        usage: Some(usage),
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
