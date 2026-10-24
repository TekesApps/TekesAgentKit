use serde_json::{Value, json};
use tekes_agent_contract::{HistoryEntry, JournalSnapshot};
use tekes_agent_presentation::{RunScope, SummaryAction, SummaryFacts, TranscriptSnapshot};
use tekes_agent_session::{MemoryProjectionStore, SessionProjection};

fn scope() -> RunScope {
    RunScope::main("dsh", "parent")
}

fn transcript_with_tools(tools: Vec<(&str, Value)>, result: Option<&str>) -> SessionProjection {
    let content: Vec<Value> = tools
        .iter()
        .map(|(name, arguments)| {
            json!({
                "type":"tool-call",
                "id": name,
                "name": if name.starts_with("task-call") { "task" } else { "subagent" },
                "arguments": arguments,
            })
        })
        .collect();
    let mut entries: Vec<HistoryEntry> = vec![
        serde_json::from_value(json!({
            "event":{"type":"turn/start","seq":0,"time":1,"data":{"turn":1}}
        }))
        .unwrap(),
        serde_json::from_value(json!({
            "event":{
                "type":"assistant/message","seq":1,"time":2,
                "data":{"turn":1,"step":1,"message":{"id":"assistant-1","content":content}}
            }
        }))
        .unwrap(),
    ];
    if let Some(child_id) = result {
        entries.push(
            serde_json::from_value(json!({
                "event":{
                    "type":"tool/result","seq":2,"time":3,
                    "data":{"turn":1,"message":{"source":{"callId":"subagent-call"},"content":[{"type":"text","text":format!("Started child {child_id}")}]}}
                }
            }))
            .unwrap(),
        );
    }
    let mut session = SessionProjection::default();
    session
        .apply_snapshot(JournalSnapshot {
            session_id: "parent".into(),
            generation: "g1".into(),
            through_sequence: entries.last().unwrap().event.seq,
            window_limit: 50,
            entries,
            has_more_before: false,
            projections: None,
            history_revision: None,
        })
        .unwrap();
    session
}

fn read(
    tools: Vec<(&str, Value)>,
    result: Option<&str>,
) -> (SessionProjection, TranscriptSnapshot, SummaryFacts) {
    let session = transcript_with_tools(tools, result);
    let transcript = TranscriptSnapshot::read(&scope(), &session).unwrap();
    let facts = SummaryFacts::read(&scope(), &session, &transcript).unwrap();
    (session, transcript, facts)
}

#[test]
fn encoded_dsh_arguments_produce_summary_title_without_rewriting_transcript_or_history() {
    let raw = json!(r#"{"description":"Catalog proof child","prompt":"CHILD_INITIAL_PROOF"}"#);
    let (session, transcript, facts) = read(vec![("subagent-call", raw.clone())], None);

    assert_eq!(facts.subagents[0].title, "Catalog proof child");
    assert_eq!(transcript.tools[0].arguments, raw);
    assert_eq!(
        session.entries[&1].event.data["message"]["content"][0]["arguments"],
        raw
    );
}

#[test]
fn object_arguments_remain_supported_and_brief_precedes_trimmed_description() {
    let (_, transcript, facts) = read(
        vec![
            (
                "subagent-call",
                json!({"brief":"  brief title  ","description":"description title"}),
            ),
            (
                "task-call",
                json!({"task_name":"  producer task  ","description":"not the task title"}),
            ),
            (
                "task-call-encoded",
                json!(r#"{"task_name":"encoded producer task"}"#),
            ),
        ],
        None,
    );

    assert_eq!(facts.subagents[0].title, "brief title");
    assert_eq!(facts.tasks[0].name, "  producer task  ");
    assert_eq!(facts.tasks[1].name, "encoded producer task");
    assert!(transcript.tools[0].arguments.is_object());
    assert_eq!(
        transcript.tools[0].arguments["description"],
        "description title"
    );
}

#[test]
fn malformed_non_object_and_double_encoded_arguments_do_not_create_summary_fields() {
    let invalid = vec![
        ("bad-json", json!(r#"{"description":"unterminated}"#)),
        ("array", json!([])),
        ("scalar", json!(17)),
        (
            "double-encoded",
            json!(r#""{\"description\":\"nested title\"}""#),
        ),
        (
            "task-call",
            json!(r#"{"task_name":"task despite malformed input"#),
        ),
    ];
    let originals: Vec<_> = invalid.iter().map(|(_, value)| value.clone()).collect();
    let (_, transcript, facts) = read(invalid, None);

    assert_eq!(facts.subagents.len(), 4);
    assert!(facts.subagents.iter().all(|item| item.title == "subagent"));
    assert!(facts.tasks.is_empty());
    assert_eq!(
        transcript
            .tools
            .iter()
            .map(|tool| tool.arguments.clone())
            .collect::<Vec<_>>(),
        originals
    );
}

#[test]
fn decoded_arguments_do_not_bypass_same_endpoint_child_pointer_and_status_gate() {
    let child_id = "b437cf34-5661-45e2-a674-113b27c92f89";
    let raw = json!(r#"{"description":"Catalog proof child","prompt":"CHILD_INITIAL_PROOF"}"#);
    let (session, transcript, _) = read(vec![("subagent-call", raw.clone())], Some(child_id));

    let mut store = MemoryProjectionStore::new("dsh");
    store.sessions.insert("parent".into(), session.clone());
    store.sessions.insert(
        child_id.into(),
        SessionProjection {
            summary: Some(json!({"sessionId":child_id,"status":"running"})),
            ..SessionProjection::default()
        },
    );
    let facts = SummaryFacts::read_with_store(&scope(), &store, &transcript).unwrap();
    assert_eq!(facts.subagents[0].title, "Catalog proof child");
    assert_eq!(facts.subagents[0].session_id.as_deref(), Some(child_id));
    assert_eq!(facts.subagents[0].status, "running");
    assert!(matches!(
        facts.sections(&scope()).iter().find(|section| section.id == "subagents").unwrap().rows[0].action.as_ref(),
        Some(SummaryAction::OpenSubagent { child_id: action_child, .. }) if action_child == child_id
    ));

    let mut without_child = MemoryProjectionStore::new("dsh");
    without_child.sessions.insert("parent".into(), session);
    let facts = SummaryFacts::read_with_store(&scope(), &without_child, &transcript).unwrap();
    assert_eq!(facts.subagents[0].session_id, None);
    assert!(facts.subagents[0].status.is_empty());
    assert!(
        facts
            .sections(&scope())
            .iter()
            .find(|section| section.id == "subagents")
            .unwrap()
            .rows[0]
            .action
            .is_none()
    );
    assert_eq!(transcript.tools[0].arguments, raw);
}

#[test]
fn missing_argument_label_uses_the_original_producer_headline_policy() {
    let (session, mut transcript, _) = read(vec![("subagent-call", json!("invalid-json"))], None);
    transcript.tools[0].call_view =
        Some(json!({"for":"call","view":{"title":"  Producer call\nheadline  "}}));
    transcript.tools[0].result_view = Some(json!({"for":"call","view":{"title":"wrong envelope"}}));
    let facts = SummaryFacts::read(&scope(), &session, &transcript).unwrap();
    assert_eq!(facts.subagents[0].title, "Producer call headline");
    transcript.tools[0].result_view =
        Some(json!({"for":"result","view":{"title":"  Producer result headline  "}}));
    let facts = SummaryFacts::read(&scope(), &session, &transcript).unwrap();
    assert_eq!(facts.subagents[0].title, "Producer result headline");
    transcript.tools[0].arguments =
        json!(r#"{"brief":"  source label  ","description":"fallback"}"#);
    let facts = SummaryFacts::read(&scope(), &session, &transcript).unwrap();
    assert_eq!(facts.subagents[0].title, "source label");
}
