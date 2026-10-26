use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tekes_agent_contract::*;
use tekes_agent_presentation::*;
use tekes_agent_session::*;
fn scope(id: &str) -> RunScope {
    RunScope::main("e", id)
}
fn destination(id: &str) -> ComposerDestination {
    ComposerDestination::run(scope(id))
}
fn authority() -> MemoryProjectionStore {
    let mut store = MemoryProjectionStore::new("e");
    store.connection = ConnectionState::Ready;
    store.inventory_ready = true;
    store.control_ready = true;
    store.actionables_ready = true;
    store.endpoint_capabilities =
        EndpointCapabilities::methods(&["session.prompt", "session.create", "commands/run"]);
    for id in ["a", "b"] {
        store.sessions.insert(
            id.into(),
            SessionProjection {
                journal_ready: true,
                ..Default::default()
            },
        );
    }
    store
}
fn event(seq: i64, kind: &str, data: Value) -> HistoryEntry {
    serde_json::from_value(json!({"event":{"type":kind,"seq":seq,"time":100+seq,"data":data}}))
        .unwrap()
}
fn session(entries: Vec<HistoryEntry>) -> SessionProjection {
    let mut session = SessionProjection::default();
    session
        .apply_snapshot(JournalSnapshot {
            session_id: "a".into(),
            generation: "g".into(),
            through_sequence: entries.last().map(|e| e.event.seq).unwrap_or(-1),
            window_limit: 50,
            entries,
            has_more_before: false,
            projections: None,
            history_revision: None,
        })
        .unwrap();
    session
}
fn assistant(seq: i64, text: &str) -> HistoryEntry {
    event(
        seq,
        "assistant/message",
        json!({"turn":1,"step":1,"message":{"id":"answer","content":[{"type":"text","text":text}]},"usage":{"inputTokens":10,"outputTokens":3}}),
    )
}
#[test]
fn live_frame_keeps_identity_and_waits_for_turn_settlement() {
    let mut session = session(vec![
        event(0, "turn/start", json!({"turn":1})),
        event(
            1,
            "assistant/chunk",
            json!({"turn":1,"step":1,"chunk":{"type":"reasoning-delta","text":"think"}}),
        ),
    ]);
    let live = TranscriptSnapshot::read(&scope("a"), &session).unwrap();
    let id = live.items[0].id.clone();
    assert!(!live.items[0].reasoning_ended);
    assert!(live.items[0].is_provisional);
    session
        .apply_live(event(
            2,
            "assistant/chunk",
            json!({"turn":1,"step":1,"chunk":{"type":"reasoning-end"}}),
        ))
        .unwrap();
    assert!(
        TranscriptSnapshot::read(&scope("a"), &session)
            .unwrap()
            .items[0]
            .reasoning_ended
    );
    session.apply_live(assistant(3, "answer")).unwrap();
    let committed = TranscriptSnapshot::read(&scope("a"), &session).unwrap();
    assert_eq!(committed.items[0].id, id);
    assert!(!committed.items[0].is_provisional);
    assert!(!committed.items[0].is_final);
    session
        .apply_live(event(
            4,
            "turn/end",
            json!({"turn":1,"reason":"completed","usage":{"inputTokens":40,"outputTokens":8}}),
        ))
        .unwrap();
    let settled = TranscriptSnapshot::read(&scope("a"), &session).unwrap();
    assert_eq!(settled.items[0].id, id);
    assert!(settled.items[0].is_final);
    assert_eq!(settled.turns[0].usage.input_tokens, 40);
}
#[test]
fn abort_clears_only_last_transport_error_and_stops_unfinished_tools() {
    let session = session(vec![
        event(0, "turn/start", json!({"turn":1})),
        event(
            1,
            "response/error",
            json!({"turn":1,"message":"keep","classification":"transport"}),
        ),
        event(
            2,
            "tool/call",
            json!({"turn":1,"step":1,"callId":"call","name":"bash","arguments":{"command":"echo"}}),
        ),
        assistant(3, "partial"),
        event(
            4,
            "response/error",
            json!({"turn":1,"message":"stop","classification":"transport"}),
        ),
        event(5, "turn/end", json!({"turn":1,"reason":{"kind":"aborted"}})),
    ]);
    let result = TranscriptSnapshot::read(&scope("a"), &session).unwrap();
    assert_eq!(result.turns[0].status, "aborted");
    assert!(!result.items.iter().any(|i| i.is_final));
    assert!(result.items.iter().any(|i| i.text == "keep"));
    assert!(!result.items.iter().any(|i| i.text == "stop"));
    assert_eq!(result.tools[0].phase, "stopped");
}
#[test]
fn explicit_final_survives_commentary_and_usage_is_not_doubled() {
    let mut first = assistant(0, "answer");
    first.event.data["sessionFinal"] = json!(true);
    let mut commentary = assistant(1, "commentary");
    commentary.event.data["step"] = json!(2);
    commentary.event.data["message"]["id"] = json!("commentary");
    commentary.event.data["sessionFinal"] = json!(false);
    let result = TranscriptSnapshot::read(
        &scope("a"),
        &session(vec![
            first,
            commentary,
            event(2, "turn/end", json!({"turn":1,"reason":"completed"})),
        ]),
    )
    .unwrap();
    assert_eq!(
        result.items.iter().find(|i| i.is_final).unwrap().text,
        "answer"
    );
    assert_eq!(result.turns[0].usage.input_tokens, 20);
}
#[test]
fn human_replacements_are_atomic_and_model_only_replacements_do_not_erase_rows() {
    let mut replacement = assistant(2, "replacement");
    replacement.event.metadata.insert(
        "surfaceOp".into(),
        json!({"op":"replace","start":0,"end":1}),
    );
    let mut original = assistant(0, "old");
    original.event.data["step"] = json!(0);
    let mut second = assistant(1, "second");
    second.event.data["step"] = json!(1);
    let result = TranscriptSnapshot::read(
        &scope("a"),
        &session(vec![original.clone(), second, replacement]),
    )
    .unwrap();
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].text, "replacement");
    let mut model = assistant(1, "model-only");
    model.is_model_only = Some(true);
    model.event.metadata.insert(
        "surfaceOp".into(),
        json!({"op":"replace","startSeq":0,"endSeq":0}),
    );
    assert_eq!(
        TranscriptSnapshot::read(&scope("a"), &session(vec![original, model]))
            .unwrap()
            .items[0]
            .text,
        "old"
    );
    let mut bad = assistant(8, "outside");
    bad.event.metadata.insert(
        "surfaceOp".into(),
        json!({"op":"replace","start":1,"end":7}),
    );
    assert!(TranscriptSnapshot::read(&scope("a"), &session(vec![bad])).is_err());
}
#[test]
fn runtime_context_is_hidden_and_opening_input_stays_immutable() {
    let result=TranscriptSnapshot::read(&scope("a"),&session(vec![event(0,"user/message",json!({"turn":1,"id":"runtime","source":{"kind":"runtime-context"},"content":[{"type":"text","text":"context"}]})),event(1,"user/message",json!({"turn":1,"id":"first","source":{"kind":"user","rpcId":"r1"},"content":[{"type":"text","text":"first"}]})),event(2,"user/message",json!({"turn":1,"id":"steer","source":{"kind":"user","mode":"steer","rpcId":"r2"},"content":[{"type":"text","text":"steer"}]}))])).unwrap();
    assert_eq!(result.turns[0].input, "first");
    assert_eq!(result.items[0].metadata["hidden"], true);
    assert_eq!(
        result.reconciliation_ids().collect::<Vec<_>>(),
        ["r1", "r2"]
    );
}
fn native(entries: Vec<Value>, events: Vec<Value>, streaming: bool) -> SessionProjection {
    SessionProjection {
        journal_ready: true,
        native_conversation: Some(NativeConversationSnapshot {
            session_id: "a".into(),
            generation: "g".into(),
            leaf_id: entries
                .last()
                .and_then(|e| e["id"].as_str())
                .map(String::from),
            entries,
            has_more_before: false,
            is_streaming: streaming,
            state: json!({}),
        }),
        native_events: events.into(),
        ..Default::default()
    }
}
#[test]
fn pi_live_to_committed_identity_and_receipts_are_native_and_exact() {
    let user = json!({"id":"u","type":"message","message":{"role":"user","content":"same text","timestamp":1}});
    let message = json!({"role":"assistant","content":[{"type":"text","text":"reply"}],"timestamp":2,"stopReason":"stop"});
    let live = native(
        vec![user.clone()],
        vec![json!({"type":"message_update","message":message})],
        true,
    );
    let before = TranscriptSnapshot::read(&scope("a"), &live).unwrap();
    let live_id = before.items.last().unwrap().id.clone();
    let receipt = json!({"id":"r","type":"custom","customType":"agentkit.inputReceipt.v1","data":{"rpcId":"rpc","timestamp":1,"hash":format!("{:x}",Sha256::digest(b"same text"))}});
    let after = TranscriptSnapshot::read(
        &scope("a"),
        &native(
            vec![
                user,
                json!({"id":"m","type":"message","message":message}),
                receipt,
            ],
            vec![],
            false,
        ),
    )
    .unwrap();
    assert_eq!(after.items.last().unwrap().id, live_id);
    assert!(after.items.last().unwrap().is_final);
    assert_eq!(after.items.last().unwrap().record_id.as_deref(), Some("m"));
    assert_eq!(after.reconciliation_ids().collect::<Vec<_>>(), ["rpc"]);
}
#[test]
fn pi_settled_marker_does_not_promote_aborted_or_tool_call_messages() {
    let entries = vec![
        json!({"id":"start","customType":"agentkit.run.v1","data":{"id":"run","phase":"started"}}),
        json!({"id":"u","type":"message","message":{"role":"user","content":"input","timestamp":1}}),
        json!({"id":"m","type":"message","message":{"role":"assistant","content":[{"type":"text","text":"partial"}],"timestamp":2,"stopReason":"aborted"}}),
        json!({"id":"end","customType":"agentkit.run.v1","data":{"id":"run","phase":"settled","aborted":true,"timestamp":3}}),
    ];
    let result = TranscriptSnapshot::read(&scope("a"), &native(entries, vec![], false)).unwrap();
    assert_eq!(result.turns[0].status, "aborted");
    assert!(!result.items.iter().any(|i| i.is_final));
}
#[test]
fn compiler_uses_explicit_commands_and_preserves_artifacts() {
    let draft = ComposerDraft {
        text: "/literal text".into(),
        ..Default::default()
    };
    assert!(matches!(
        draft.compile(true).unwrap(),
        SubmissionPlan::Prompt { .. }
    ));
    let mut command = ComposerDraft {
        text: " args ".into(),
        artifacts: vec![ComposerArtifact::Command {
            id: "cmd".into(),
            name: "test".into(),
            supports_attachments: false,
        }],
        ..Default::default()
    };
    assert!(
        matches!(command.compile(true).unwrap(),SubmissionPlan::Command{arguments,..} if arguments=="args")
    );
    assert!(command.compile(false).is_err());
    command.artifacts.push(ComposerArtifact::Skill {
        id: "skill".into(),
        name: "skill".into(),
        skill_id: "s".into(),
    });
    assert!(command.compile(true).is_err());
}
fn text(value: &str) -> ComposerDraft {
    ComposerDraft {
        text: value.into(),
        ..Default::default()
    }
}
#[test]
fn destination_isolation_stale_a_b_a_reads_and_ambiguous_namespace_boundaries() {
    let mut store = PresentationStore::default();
    let a = destination("a");
    let b = destination("b");
    let stale = store.select(a.clone()).unwrap();
    store.edit(&a, text("A")).unwrap();
    store.select(b.clone()).unwrap();
    store.edit(&b, text("B")).unwrap();
    store.select(a.clone()).unwrap();
    assert!(!store.commit_settings(&stale, [("model".into(), json!("old"))].into(), false));
    assert_eq!(store.draft(&a).text, "A");
    assert_eq!(store.draft(&b).text, "B");
    let left = ComposerDestination::run(RunScope {
        endpoint_id: "e:x".into(),
        thread_id: "t".into(),
        session_id: "s".into(),
    });
    let right = ComposerDestination::run(RunScope {
        endpoint_id: "e".into(),
        thread_id: "x:t".into(),
        session_id: "s".into(),
    });
    assert_ne!(left.storage_key(), right.storage_key());
}
#[test]
fn ambiguous_submission_locks_its_owner_and_new_typing_wins_failure_recovery() {
    let mut store = PresentationStore::default();
    let authority = authority();
    let a = destination("a");
    store.select(a.clone()).unwrap();
    let attachment = ComposerArtifact::Attachment {
        id: "file".into(),
        name: "proof".into(),
        content: json!({"type":"file","receiptId":"file"}),
        facts: None,
    };
    store
        .edit(
            &a,
            ComposerDraft {
                text: "old".into(),
                artifacts: vec![attachment.clone()],
                ..Default::default()
            },
        )
        .unwrap();
    let ticket = store.prepare(&a, PromptMode::Queue, &authority).unwrap();
    assert!(
        store.refresh(&authority).unwrap().transcript.items[0]
            .id
            .starts_with("echo-item:")
    );
    store.mark_dispatched(&ticket).unwrap();
    store.select(destination("b")).unwrap();
    store.edit(&a, text("new typing")).unwrap();
    store.finish(ticket.id(), Err("timeout".into())).unwrap();
    store.finish(ticket.id(), Ok(())).unwrap();
    assert_eq!(store.draft(&a).text, "new typing");
    assert_eq!(store.draft(&a).artifacts, [attachment]);
    assert!(store.draft(&destination("b")).text.is_empty());
    assert!(store.prepare(&a, PromptMode::Queue, &authority).is_err());
    store.acknowledge_reconciliation(ticket.id()).unwrap();
    store.prepare(&a, PromptMode::Queue, &authority).unwrap();
}
#[test]
fn echo_retires_only_by_rpc_identity_and_late_failure_does_not_restore_acknowledged_input() {
    let mut store = PresentationStore::default();
    let mut authority = authority();
    let a = destination("a");
    store.select(a.clone()).unwrap();
    store.edit(&a, text("same")).unwrap();
    let first = store.prepare(&a, PromptMode::Queue, &authority).unwrap();
    store.mark_dispatched(&first).unwrap();
    store.finish(first.id(), Ok(())).unwrap();
    store.edit(&a, text("same")).unwrap();
    let second = store.prepare(&a, PromptMode::Queue, &authority).unwrap();
    authority.sessions.insert("a".into(),session(vec![event(0,"user/message",json!({"turn":1,"id":"user","source":{"kind":"user","rpcId":first.id()},"content":[{"type":"text","text":"same"}]}))]));
    let result = store.refresh(&authority).unwrap();
    assert_eq!(
        result
            .transcript
            .items
            .iter()
            .filter(|i| i.id.starts_with("echo-item:"))
            .count(),
        1
    );
    assert!(
        result
            .transcript
            .items
            .iter()
            .any(|i| i.reconciliation_id.as_deref() == Some(second.id()))
    );
    authority.sessions.get_mut("a").unwrap().queue =
        vec![json!({"id":"queue","rpcId":second.id()})];
    store.refresh(&authority).unwrap();
    store.finish(second.id(), Err("late".into())).unwrap();
    assert!(store.draft(&a).text.is_empty());
}
#[test]
fn native_queued_user_source_retires_only_its_scoped_submission_echo() {
    let mut store = PresentationStore::default();
    let mut authority = authority();
    let a = destination("a");
    store.select(a.clone()).unwrap();
    store.edit(&a, text("queued native input")).unwrap();
    let ticket = store.prepare(&a, PromptMode::Queue, &authority).unwrap();
    store.mark_dispatched(&ticket).unwrap();
    let queued = json!({"id":"input:42","placement":"queued","message":{
        "id":"input-42","role":"user","content":[{"type":"text","text":"queued native input"}],
        "source":{"kind":"user","rpcId":ticket.id()}}});
    // Another session carrying the same request text/identity cannot settle this owner.
    authority.sessions.get_mut("b").unwrap().queue = vec![queued.clone()];
    assert!(!store.refresh(&authority).unwrap().submissions[0].reconciled);
    for (role, kind) in [("assistant", "user"), ("user", "tool")] {
        let mut foreign = queued.clone();
        foreign["message"]["role"] = json!(role);
        foreign["message"]["source"]["kind"] = json!(kind);
        authority.sessions.get_mut("a").unwrap().queue = vec![foreign];
        assert!(!store.refresh(&authority).unwrap().submissions[0].reconciled);
    }
    let mut other_request = queued.clone();
    other_request["message"]["source"]["rpcId"] = json!("another-request");
    authority.sessions.get_mut("a").unwrap().queue = vec![other_request];
    assert!(!store.refresh(&authority).unwrap().submissions[0].reconciled);
    authority.sessions.get_mut("a").unwrap().queue = vec![queued];
    let accepted = store.refresh(&authority).unwrap();
    assert!(accepted.submissions[0].reconciled);
    assert!(
        !accepted
            .transcript
            .items
            .iter()
            .any(|item| item.is_provisional
                && item.reconciliation_id.as_deref() == Some(ticket.id()))
    );
    // A late failed ACK must not resurrect input already admitted by the public queue.
    store
        .finish(ticket.id(), Err("late transport failure".into()))
        .unwrap();
    assert!(store.draft(&a).text.is_empty());
    store.edit(&a, text("next explicit input")).unwrap();
    store.prepare(&a, PromptMode::Queue, &authority).unwrap();
}
#[test]
fn new_thread_materialization_moves_draft_and_echo_without_touching_selected_other_run() {
    let mut store = PresentationStore::default();
    let authority = authority();
    let source = ComposerDestination::NewThread {
        endpoint_id: "e".into(),
        workspace_id: "workspace".into(),
    };
    store.select(source.clone()).unwrap();
    store.edit(&source, text("first input")).unwrap();
    let ticket = store
        .prepare(&source, PromptMode::Queue, &authority)
        .unwrap();
    store.edit(&source, text("later input")).unwrap();
    store.select(destination("b")).unwrap();
    store.materialize(&ticket, scope("a")).unwrap();
    assert_eq!(store.selection().unwrap().destination, destination("b"));
    assert_eq!(store.draft(&destination("a")).text, "later input");
    store.select(destination("a")).unwrap();
    assert_eq!(store.refresh(&authority).unwrap().transcript.items.len(), 1);
}
#[test]
fn connection_and_thread_status_never_invent_running_or_cross_session_authority() {
    let mut authority = authority();
    authority.sessions.get_mut("a").unwrap().run_open = true;
    let running = RunPresentation::read(&authority, &scope("a"), false);
    assert_eq!(running.phase, RunPhase::Running);
    assert!(running.can_cancel);
    authority.sessions.get_mut("a").unwrap().summary = Some(json!({"status":"interrupted"}));
    let interrupted = RunPresentation::read(&authority, &scope("a"), true);
    assert_eq!(interrupted.phase, RunPhase::Interrupted);
    assert!(interrupted.can_submit);
    authority.sessions.get_mut("a").unwrap().journal_error = Some("bad pointer".into());
    assert_eq!(
        RunPresentation::read(&authority, &scope("a"), true)
            .connection
            .phase,
        ConnectionPhase::Degraded
    );
    assert!(RunPresentation::read(&authority, &scope("b"), true).can_submit);
    authority.connection = ConnectionState::Connecting;
    let reconnect = RunPresentation::read(&authority, &scope("a"), true);
    assert_eq!(reconnect.connection.phase, ConnectionPhase::Reconnecting);
    assert!(!reconnect.can_cancel);
    assert!(!reconnect.can_submit);
    assert!(reconnect.actionables.is_empty());
    authority.connection = ConnectionState::Ready;
    authority.archived_session_ids.push("b".into());
    assert!(!RunPresentation::read(&authority, &scope("b"), true).can_submit);
}

#[test]
fn legacy_history_is_readable_but_shared_presentation_blocks_writes() {
    let mut authority = authority();
    let session = authority.sessions.get_mut("a").unwrap();
    session.journal_ready = true;
    session.run_open = true;
    session.queue.push(json!({"id":"queued"}));
    session.jobs.push(json!({"id":"job"}));
    authority
        .actionables
        .insert("approval".into(), json!({"id":"approval","sessionId":"a"}));
    authority.sessions.get_mut("a").unwrap().projections.insert(
        "historyAccess".into(),
        VersionedValue {
            sequence: 12,
            value: json!({
                "mode":"read-only",
                "reason":"legacy-tool-catalog-unavailable",
                "sourceDigest":format!("sha256-{}", "a".repeat(64))
            }),
        },
    );

    let run = RunPresentation::read(&authority, &scope("a"), true);
    assert_eq!(run.connection.phase, ConnectionPhase::Connected);
    assert!(!run.connection.history_is_stale);
    assert!(
        run.connection
            .message
            .as_deref()
            .unwrap()
            .contains("历史只读")
    );
    assert_eq!(run.phase, RunPhase::Unavailable);
    assert!(!run.can_submit);
    assert!(!run.can_cancel);
    assert!(run.actionables.is_empty());
    assert!(run.queue.is_empty());
    assert!(run.jobs.is_empty());
    assert!(run.error.as_deref().unwrap().contains("历史只读"));

    let transcript =
        TranscriptSnapshot::read(&scope("a"), authority.sessions.get("a").unwrap()).unwrap();
    assert!(!transcript.is_stale);
}

#[test]
fn malformed_historical_access_marker_fails_closed_without_marking_history_stale() {
    let mut authority = authority();
    let session = authority.sessions.get_mut("a").unwrap();
    session.journal_ready = true;
    session.projections.insert(
        "historyAccess".into(),
        VersionedValue {
            sequence: 1,
            value: json!({"mode":"future-mode"}),
        },
    );

    let run = RunPresentation::read(&authority, &scope("a"), false);
    assert_eq!(run.connection.phase, ConnectionPhase::Connected);
    assert!(!run.connection.history_is_stale);
    assert!(!run.can_submit);
    assert!(
        run.connection
            .message
            .as_deref()
            .unwrap()
            .contains("标记无效")
    );
}

#[test]
fn workspace_can_start_another_new_thread_after_first_materializes() {
    let mut store = PresentationStore::default();
    let authority = authority();
    let source = ComposerDestination::NewThread {
        endpoint_id: "e".into(),
        workspace_id: "workspace".into(),
    };
    store.select(source.clone()).unwrap();
    store.edit(&source, text("first")).unwrap();
    let ticket = store
        .prepare(&source, PromptMode::Queue, &authority)
        .unwrap();
    store.materialize(&ticket, scope("a")).unwrap();
    store.mark_dispatched(&ticket).unwrap();
    store.select(destination("a")).unwrap();
    store.select(source.clone()).unwrap();
    assert_eq!(store.selection().unwrap().destination, source);
    assert!(store.draft(&source).text.is_empty());
    store.edit(&source, text("next conversation")).unwrap();
    store
        .finish(ticket.id(), Err("late timeout".into()))
        .unwrap();
    assert_eq!(store.draft(&source).text, "next conversation");
    assert_eq!(store.draft(&destination("a")).text, "first");
    store.select(destination("a")).unwrap();
    assert_eq!(
        store.refresh(&authority).unwrap().submissions[0].phase,
        SubmissionPhase::Unknown
    );
    assert!(!RunPresentation::read(&authority, &scope("missing"), false).can_submit);
}

#[test]
fn pi_only_child_creation_is_a_summary_subagent() {
    let child = "b437cf34-5661-45e2-a674-113b27c92f89";
    let entries = vec![
        json!({"id":"u","type":"message","message":{"role":"user","content":"delegate","timestamp":1}}),
        json!({"id":"a","type":"message","message":{"role":"assistant","timestamp":2,"content":[
            {"type":"toolCall","id":"create","name":"tekes_subagent","arguments":{"action":"create","label":"Review"}},
            {"type":"toolCall","id":"read","name":"tekes_subagent","arguments":{"action":"read","childSessionId":child}}
        ]}}),
        json!({"id":"r","type":"message","message":{"role":"toolResult","toolCallId":"create","toolName":"tekes_subagent","content":[{"type":"text","text":json!({"childSessionId":child}).to_string()}]}}),
    ];
    let projection = native(entries, vec![], false);
    let transcript = TranscriptSnapshot::read(&scope("a"), &projection).unwrap();
    assert_eq!(transcript.tools[0].name, "subagent");
    assert_eq!(transcript.tools[0].arguments["description"], "Review");
    assert_eq!(transcript.tools[1].name, "tekes_subagent");
    let facts = SummaryFacts::read(&scope("a"), &projection, &transcript).unwrap();
    assert_eq!(facts.subagents.len(), 1);
    assert_eq!(facts.subagents[0].session_id, None);
    assert!(
        facts
            .sections(&scope("a"))
            .into_iter()
            .find(|section| section.id == "subagents")
            .unwrap()
            .rows[0]
            .action
            .is_none()
    );

    let mut store = MemoryProjectionStore::new("e");
    store.sessions.insert("a".into(), projection);
    store
        .sessions
        .insert(child.into(), SessionProjection::default());
    let facts = SummaryFacts::read_with_store(&scope("a"), &store, &transcript).unwrap();
    assert_eq!(facts.subagents[0].session_id.as_deref(), Some(child));
    assert!(
        facts
            .sections(&scope("a"))
            .into_iter()
            .find(|section| section.id == "subagents")
            .unwrap()
            .rows[0]
            .action
            .is_some()
    );
}

#[test]
fn provider_output_usage_is_cumulative_partial_and_never_inferred() {
    let mut s = session(vec![event(0, "turn/start", json!({"turn":1}))]);
    let output = |s: &SessionProjection| {
        TranscriptSnapshot::read(&scope("a"), s).unwrap().turns[0]
            .live_usage
            .as_ref()
            .map(|u| u.output_tokens)
    };
    assert_eq!(output(&s), None);
    let reports = [
        json!({"outputTokens":0}),
        json!({"outputTokens":7}),
        json!({"outputTokens":7}),
        json!({"inputTokens":19}),
        json!({"outputTokens":-1}),
        json!({"outputTokens":1.5}),
    ];
    for (i, report) in reports.into_iter().enumerate() {
        s.apply_live(event(
            i as i64 + 1,
            "assistant/chunk",
            json!({"turn":1,"step":1,"chunk":{"type":"usage","usage":report}}),
        ))
        .unwrap();
        assert_eq!(output(&s), Some(if i == 0 { 0 } else { 7 }));
    }
    s.apply_live(event(7,"assistant/chunk",json!({"turn":1,"step":1,"chunk":{"type":"text-delta","text":"many characters do not change usage"}}))).unwrap();
    assert_eq!(output(&s), Some(7));
    s.apply_live(assistant(8, "answer")).unwrap();
    assert_eq!(output(&s), Some(3)); // sealed usage replaces, never adds the same step twice
    s.apply_live(event(
        9,
        "assistant/chunk",
        json!({"turn":1,"step":1,"chunk":{"type":"usage","usage":{"outputTokens":999}}}),
    ))
    .unwrap();
    assert_eq!(output(&s), Some(3));
    s.apply_live(event(
        10,
        "assistant/chunk",
        json!({"turn":1,"step":2,"chunk":{"type":"usage","usage":{"outputTokens":11}}}),
    ))
    .unwrap();
    assert_eq!(output(&s), Some(14));
    s.apply_live(event(
        11,
        "turn/end",
        json!({"turn":1,"reason":"completed","usage":{"inputTokens":10,"outputTokens":15}}),
    ))
    .unwrap();
    assert_eq!(output(&s), None);
    s.apply_live(event(
        12,
        "assistant/chunk",
        json!({"turn":1,"step":2,"chunk":{"type":"usage","usage":{"outputTokens":999}}}),
    ))
    .unwrap();
    assert_eq!(output(&s), None);
    assert_eq!(
        TranscriptSnapshot::read(&scope("a"), &s).unwrap().turns[0]
            .usage
            .output_tokens,
        15
    );
    s.apply_live(event(13, "turn/start", json!({"turn":2})))
        .unwrap();
    s.apply_live(event(
        14,
        "assistant/chunk",
        json!({"turn":1,"step":3,"chunk":{"type":"usage","usage":{"outputTokens":999}}}),
    ))
    .unwrap();
    let snapshot = TranscriptSnapshot::read(&scope("a"), &s).unwrap();
    assert!(snapshot.turns.iter().all(|t| t.live_usage.is_none()));
}

#[test]
fn missing_durable_usage_does_not_become_a_real_zero_or_partial_turn_total() {
    let mut missing = assistant(1, "unreported");
    missing.event.data.as_object_mut().unwrap().remove("usage");
    let mut s = session(vec![event(0, "turn/start", json!({"turn":1})), missing]);
    assert!(
        TranscriptSnapshot::read(&scope("a"), &s).unwrap().turns[0]
            .live_usage
            .is_none()
    );
    s.apply_live(event(
        2,
        "assistant/chunk",
        json!({"turn":1,"step":2,"chunk":{"type":"usage","usage":{"outputTokens":8}}}),
    ))
    .unwrap();
    assert!(
        TranscriptSnapshot::read(&scope("a"), &s).unwrap().turns[0]
            .live_usage
            .is_none()
    );
}

#[test]
fn durable_response_reports_accumulate_replace_and_survive_settlement_and_restore() {
    let mut s = session(vec![event(0, "turn/start", json!({"turn":1}))]);
    let reported = |s: &SessionProjection| {
        TranscriptSnapshot::read(&scope("a"), s).unwrap().turns[0]
            .reported_usage
            .clone()
    };
    assert_eq!(reported(&s), None);
    s.apply_live(assistant(1, "first")).unwrap();
    assert_eq!(
        reported(&s).unwrap(),
        TranscriptReportedUsage {
            output_tokens: 3,
            is_complete: true
        }
    );
    // Corrected copies of the same durable response replace rather than add.
    let mut corrected = assistant(2, "first");
    corrected.event.data["usage"]["outputTokens"] = json!(5);
    s.apply_live(corrected).unwrap();
    assert_eq!(reported(&s).unwrap().output_tokens, 5);
    // DSH keeps usage inside the compact stream when there is no top-level usage.
    s.apply_live(event(3,"assistant/message",json!({"turn":1,"step":2,"message":{"id":"second","content":[]},"stream":[{"type":"chunk","time":1,"chunk":{"type":"usage","usage":{"outputTokens":7}}}]}))).unwrap();
    assert_eq!(reported(&s).unwrap().output_tokens, 12);
    s.apply_live(event(4,"assistant/attempt",json!({"turn":1,"step":3,"stream":[{"type":"chunk","time":1,"chunk":{"type":"usage","usage":{"outputTokens":2}}}]}))).unwrap();
    assert_eq!(reported(&s).unwrap().output_tokens, 14);
    s.apply_live(event(
        5,
        "turn/end",
        json!({"turn":1,"reason":{"kind":"completed"}}),
    ))
    .unwrap();
    assert_eq!(
        reported(&s).unwrap(),
        TranscriptReportedUsage {
            output_tokens: 14,
            is_complete: true
        }
    );
    let mut restored: SessionProjection =
        serde_json::from_value(serde_json::to_value(&s).unwrap()).unwrap();
    assert_eq!(reported(&restored), reported(&s));
    restored.journal_ready = false;
    assert_eq!(reported(&restored), reported(&s)); // cached durable facts remain historical facts
    restored.journal_ready = true;
    restored
        .apply_live(event(6, "turn/start", json!({"turn":2})))
        .unwrap();
    let snapshot = TranscriptSnapshot::read(&scope("a"), &restored).unwrap();
    assert_eq!(
        snapshot.turns[0]
            .reported_usage
            .as_ref()
            .unwrap()
            .output_tokens,
        14
    );
    assert!(snapshot.turns[1].reported_usage.is_none());
}

#[test]
fn durable_usage_missing_zero_partial_history_and_authoritative_totals_are_distinct() {
    let mut missing = assistant(1, "missing");
    missing.event.data.as_object_mut().unwrap().remove("usage");
    let mut s = session(vec![event(0, "turn/start", json!({"turn":1})), missing]);
    assert!(
        TranscriptSnapshot::read(&scope("a"), &s).unwrap().turns[0]
            .reported_usage
            .is_none()
    );
    s.apply_live(event(
        2,
        "assistant/message",
        json!({"turn":1,"step":2,"message":{"id":"zero","content":[]},"usage":{"outputTokens":0}}),
    ))
    .unwrap();
    let reported = TranscriptSnapshot::read(&scope("a"), &s).unwrap().turns[0]
        .reported_usage
        .clone()
        .unwrap();
    assert_eq!(
        reported,
        TranscriptReportedUsage {
            output_tokens: 0,
            is_complete: false
        }
    );
    s.apply_live(event(
        3,
        "turn/end",
        json!({"turn":1,"reason":"completed","usage":{"outputTokens":19}}),
    ))
    .unwrap();
    assert_eq!(
        TranscriptSnapshot::read(&scope("a"), &s).unwrap().turns[0]
            .reported_usage
            .clone()
            .unwrap(),
        TranscriptReportedUsage {
            output_tokens: 19,
            is_complete: true
        }
    );
    // A frozen history page may begin in the middle of a turn. Never label it a complete sum.
    let mut limited = session(vec![
        event(
            2,
            "assistant/message",
            json!({"turn":1,"step":2,"message":{"id":"last","content":[]},"usage":{"outputTokens":7}}),
        ),
        event(3, "turn/end", json!({"turn":1,"reason":"completed"})),
    ]);
    assert!(
        !TranscriptSnapshot::read(&scope("a"), &limited)
            .unwrap()
            .turns[0]
            .reported_usage
            .as_ref()
            .unwrap()
            .is_complete
    );
    limited
        .apply_page(
            HistoryPage {
                session_id: "a".into(),
                through_sequence: 3,
                entries: vec![
                    event(0, "turn/start", json!({"turn":1})),
                    assistant(1, "first"),
                ],
                has_more_before: false,
            },
            &HistoryRequest {
                session_id: "a".into(),
                through_sequence: 3,
                before_sequence: Some(2),
                max_messages: 50,
            },
        )
        .unwrap();
    assert_eq!(
        TranscriptSnapshot::read(&scope("a"), &limited)
            .unwrap()
            .turns[0]
            .reported_usage
            .clone()
            .unwrap(),
        TranscriptReportedUsage {
            output_tokens: 10,
            is_complete: true
        }
    );
}

#[test]
fn a_settled_attempt_is_counted_without_blocking_the_next_attempt_in_the_same_step() {
    let mut s = session(vec![event(0, "turn/start", json!({"turn":1}))]);
    s.apply_live(event(1,"assistant/attempt",json!({"turn":1,"step":1,"stream":[{"type":"chunk","time":1,"chunk":{"type":"usage","usage":{"outputTokens":2}}}]}))).unwrap();
    s.apply_live(event(
        2,
        "assistant/chunk",
        json!({"turn":1,"step":1,"chunk":{"type":"usage","usage":{"outputTokens":3}}}),
    ))
    .unwrap();
    let snapshot = TranscriptSnapshot::read(&scope("a"), &s).unwrap();
    assert_eq!(
        snapshot.turns[0]
            .reported_usage
            .as_ref()
            .unwrap()
            .output_tokens,
        2
    );
    assert_eq!(
        snapshot.turns[0].live_usage.as_ref().unwrap().output_tokens,
        5
    );
}

#[test]
fn worker_launch_notice_recovers_only_after_later_durable_turn_start() {
    let notice = |seq, class| {
        event(
            seq,
            "session/notice",
            json!({
                "severity":"error", "classification":class,"operation":"prompt",
                "message":"The worker could not be started: worker hello timed out"
            }),
        )
    };
    let mut state = session(vec![notice(0, "worker_launch"), notice(1, "other")]);
    let failed = TranscriptSnapshot::read(&scope("a"), &state).unwrap();
    assert_eq!(failed.items[0].metadata["status"], "error");
    let original = failed.items[0].clone();
    state
        .apply_live(event(2, "user/message", json!({"message":{"content":[]}})))
        .unwrap();
    assert_eq!(
        TranscriptSnapshot::read(&scope("a"), &state).unwrap().items[0].metadata["status"],
        "error"
    );
    state
        .apply_live(event(3, "turn/start", json!({"turn":1})))
        .unwrap();
    let recovered = TranscriptSnapshot::read(&scope("a"), &state).unwrap();
    assert_eq!(recovered.items[0].id, original.id);
    assert_eq!(recovered.items[0].text, original.text);
    assert_eq!(recovered.items[0].metadata["status"], "recovered");
    assert_eq!(recovered.items[0].metadata["recoveredAtSequence"], 3);
    assert_eq!(recovered.items[1].metadata["status"], "error");
    // A second failure remains active until its own later turn starts.
    state.apply_live(notice(4, "worker_launch")).unwrap();
    let out = TranscriptSnapshot::read(&scope("a"), &state).unwrap();
    assert_eq!(out.items.last().unwrap().metadata["status"], "error");
    let serialized = serde_json::to_value(&state).unwrap();
    let replay = serde_json::from_value(serialized).unwrap();
    assert_eq!(out, TranscriptSnapshot::read(&scope("a"), &replay).unwrap());
}

#[test]
fn attachment_facts_are_backward_compatible_and_never_enter_prompt() {
    let value = json!({"kind":"attachment","id":"a","name":"中文.txt","content":{"type":"file","receiptId":"receipt"}});
    let mut artifact: ComposerArtifact = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(&artifact).unwrap(), value);
    if let ComposerArtifact::Attachment { facts, .. } = &mut artifact {
        *facts = Some(AttachmentFacts {
            attachment_id: Some("read-id-distinct-from-receipt".into()),
            byte_count: 12,
            media_type: Some("text/plain".into()),
        });
    }
    let serialized = serde_json::to_value(&artifact).unwrap();
    assert_eq!(serialized["facts"]["byteCount"], 12);
    assert_eq!(
        serialized["facts"]["attachmentId"],
        "read-id-distinct-from-receipt"
    );
    assert_eq!(
        serde_json::from_value::<ComposerArtifact>(serialized).unwrap(),
        artifact
    );
    let draft = ComposerDraft {
        artifacts: vec![artifact],
        ..Default::default()
    };
    let SubmissionPlan::Prompt { content, .. } = draft.compile(false).unwrap() else {
        panic!()
    };
    assert_eq!(content, vec![json!({"type":"file","receiptId":"receipt"})]);
}

#[test]
fn run_publication_retains_its_own_authority_epoch_across_reconnect_and_legacy_decode() {
    let mut authority = authority();
    authority.connection_epoch = 7;
    let mut store = PresentationStore::default();
    store.select(destination("a")).unwrap();
    let old = store.refresh(&authority).unwrap();
    authority.connection_epoch = 8;
    authority.connection = ConnectionState::Connecting;
    let next = store.refresh(&authority).unwrap();
    assert!(next.revision > old.revision);
    assert_eq!(old.connection_epoch, 7);
    assert_eq!(next.connection_epoch, 8);
    assert_eq!(next.connection.phase, ConnectionPhase::Reconnecting);
    let mut legacy = serde_json::to_value(old).unwrap();
    legacy.as_object_mut().unwrap().remove("connectionEpoch");
    assert_eq!(
        serde_json::from_value::<RunSnapshot>(legacy)
            .unwrap()
            .connection_epoch,
        0
    );
}

#[test]
fn a_new_native_stream_cannot_replace_a_committed_message_with_the_same_timestamp() {
    let entries = vec![
        json!({"id":"u","type":"message","message":{"role":"user","timestamp":1,"content":"input"}}),
        json!({"id":"old","type":"message","message":{"role":"assistant","timestamp":2,"content":"retained answer","stopReason":"toolUse"}}),
    ];
    let preview = json!({"type":"message_update","message":{"role":"assistant","timestamp":2,"content":"new stream"}});
    let projected =
        TranscriptSnapshot::read(&scope("a"), &native(entries.clone(), vec![preview], true))
            .unwrap();
    let retained = projected
        .items
        .iter()
        .find(|row| row.record_id.as_deref() == Some("old"))
        .expect("committed row must survive");
    assert_eq!(retained.text, "retained answer");
    assert!(!retained.is_provisional);
    let live = projected
        .items
        .iter()
        .find(|row| row.is_provisional)
        .unwrap();
    assert_eq!(live.text, "new stream");
    assert_ne!(retained.id, live.id);
    let live_id = live.id.clone();
    let mut completed = entries;
    completed.push(json!({"id":"new","type":"message","message":{"role":"assistant","timestamp":2,"content":"new stream","stopReason":"stop"}}));
    let completed =
        TranscriptSnapshot::read(&scope("a"), &native(completed, vec![], false)).unwrap();
    assert_eq!(
        completed
            .items
            .iter()
            .find(|row| row.record_id.as_deref() == Some("new"))
            .unwrap()
            .id,
        live_id
    );
}

#[test]
fn an_assistant_end_boundary_retires_its_preview_while_the_agent_is_still_running() {
    let message =
        json!({"role":"assistant","timestamp":2,"content":"tool step","stopReason":"toolUse"});
    let source = native(
        vec![
            json!({"id":"u","type":"message","message":{"role":"user","timestamp":1,"content":"input"}}),
            json!({"id":"answer","type":"message","message":message}),
        ],
        vec![
            json!({"type":"message_update","message":message}),
            json!({"type":"message_end","message":message}),
        ],
        true,
    );
    let snapshot = TranscriptSnapshot::read(&scope("a"), &source).unwrap();
    assert_eq!(
        snapshot
            .items
            .iter()
            .filter(|row| row.kind == TranscriptItemKind::Assistant)
            .count(),
        1
    );
    assert!(!snapshot.items.iter().any(|row| row.is_provisional));
    assert_eq!(
        snapshot.items.last().unwrap().record_id.as_deref(),
        Some("answer")
    );
    assert!(snapshot.items.last().unwrap().reasoning_ended);
    assert_eq!(snapshot.turns.last().unwrap().status, "running");
}

#[test]
fn a_tool_result_end_does_not_retire_an_active_assistant_preview() {
    let source = native(
        vec![
            json!({"id":"u","type":"message","message":{"role":"user","timestamp":1,"content":"input"}}),
        ],
        vec![
            json!({"type":"message_update","message":{"role":"assistant","timestamp":2,"content":"still streaming"}}),
            json!({"type":"message_end","message":{"role":"toolResult","toolCallId":"call","content":"done"}}),
        ],
        true,
    );
    let snapshot = TranscriptSnapshot::read(&scope("a"), &source).unwrap();
    assert_eq!(
        snapshot
            .items
            .iter()
            .filter(|row| row.is_provisional && row.text == "still streaming")
            .count(),
        1
    );
}

#[test]
fn pi_provider_error_retains_its_public_message_and_is_not_a_final_answer() {
    let result=TranscriptSnapshot::read(&scope("a"),&native(vec![
        json!({"id":"u","type":"message","message":{"role":"user","content":"input","timestamp":1}}),
        json!({"id":"failure","type":"message","message":{"role":"assistant","content":[],"timestamp":2,"stopReason":"error","errorMessage":"controlled provider error"}}),
    ],vec![],false)).unwrap();
    assert_eq!(result.turns[0].status, "error");
    assert_eq!(
        result.items.last().unwrap().metadata["errorMessage"],
        "controlled provider error"
    );
    assert!(!result.items.iter().any(|item| item.is_final));
    assert_eq!(
        result.latest_failure().unwrap().message,
        Some("controlled provider error")
    );
    for status in ["running", "completed", "aborted"] {
        let mut newer = result.clone();
        let mut turn = newer.turns[0].clone();
        turn.id = "newer".into();
        turn.status = status.into();
        newer.turns.push(turn);
        assert!(newer.latest_failure().is_none());
        assert_eq!(
            newer.items.last().unwrap().failure_message(),
            Some("controlled provider error")
        );
    }
    let mut unknown = result.clone();
    unknown.items.last_mut().unwrap().metadata["errorMessage"] = Value::Null;
    assert_eq!(unknown.latest_failure().unwrap().message, None);
    let mut cancelled = result.clone();
    cancelled.turns[0].status = "aborted".into();
    cancelled.items.last_mut().unwrap().metadata["stopReason"] = "aborted".into();
    assert!(cancelled.latest_failure().is_none());
    assert!(cancelled.items.last().unwrap().failure_message().is_none());
}
