use super::*;
use tekes_agent_session::{ConnectionState, MemoryProjectionStore, SessionProjection};
fn scope(id: &str) -> RunScope {
    RunScope::main("fallback", id)
}
fn destination(id: &str) -> ComposerDestination {
    ComposerDestination::run(scope(id))
}
fn entry(seq: i64, kind: &str, data: Value) -> HistoryEntry {
    serde_json::from_value(json!({"event":{"seq":seq,"time":seq+1,"type":kind,"data":data}}))
        .unwrap()
}
fn history(authority: &mut MemoryProjectionStore, id: &str, invalid: bool) {
    let mut entries = vec![entry(
        0,
        "user/message",
        json!({"turn":1,"message":{"content":[{"type":"text","text":format!("history {id}")}]}}),
    )];
    if invalid {
        entries.push(entry(
            1,
            "turn/end",
            json!({"turn":1,"reason":"completed","promotedMessageID":"missing"}),
        ));
    }
    authority
        .sessions
        .entry(id.into())
        .or_default()
        .apply_snapshot(JournalSnapshot {
            session_id: id.into(),
            generation: "g".into(),
            through_sequence: entries.last().unwrap().event.seq,
            window_limit: 50,
            entries,
            has_more_before: false,
            projections: None,
            history_revision: Some(if invalid { "bad" } else { "valid" }.into()),
        })
        .unwrap();
}
fn authority() -> MemoryProjectionStore {
    let mut a = MemoryProjectionStore::new("fallback");
    a.connection = ConnectionState::Ready;
    a.connection_epoch = 7;
    a.inventory_ready = true;
    a.workspace_ready = true;
    a.control_ready = true;
    a.actionables_ready = true;
    a.endpoint_capabilities = EndpointCapabilities::methods(&["session.prompt"]);
    for id in ["a", "b"] {
        a.sessions.insert(
            id.into(),
            SessionProjection {
                summary: Some(json!({"sessionId":id,"status":"active"})),
                ..Default::default()
            },
        );
        history(&mut a, id, false);
    }
    a
}
fn selected(a: &MemoryProjectionStore) -> (PresentationStore, RunSnapshot) {
    let mut core = PresentationStore::default();
    core.select(destination("a")).unwrap();
    core.edit_text(&destination("a"), "A local draft".into())
        .unwrap();
    let previous = core.refresh(a).unwrap();
    (core, previous)
}
fn assert_closed(snapshot: &RunSnapshot) {
    assert!(!snapshot.can_submit);
    assert_eq!(snapshot.connection.phase, ConnectionPhase::Degraded);
    assert!(snapshot.connection.history_is_stale);
    assert!(snapshot.transcript.is_stale);
    assert!(
        snapshot
            .connection
            .message
            .as_ref()
            .unwrap()
            .contains("promoted assistant")
    );
    let run = snapshot.run.as_ref().unwrap();
    assert!(!run.can_submit && !run.can_cancel);
    assert!(run.actionables.is_empty());
    assert_eq!(run.connection, snapshot.connection);
}
#[test]
fn malformed_history_keeps_archive_from_current_summary_or_workspace_and_recovers_explicitly() {
    for workspace in [false, true] {
        let mut a = authority();
        let (mut core, active) = selected(&a);
        assert_eq!(active.run.as_ref().unwrap().phase, RunPhase::Idle);
        if workspace {
            a.archived_session_ids.push("a".into());
        } else {
            a.sessions.get_mut("a").unwrap().summary.as_mut().unwrap()["status"] =
                json!("archived");
        }
        // Archive and corruption can arrive together. Do not copy the old Idle phase.
        history(&mut a, "a", true);
        assert!(core.refresh(&a).is_err());
        let archived = refreshed_snapshot(&mut core, &a, &active);
        assert_closed(&archived);
        assert_eq!(archived.run.as_ref().unwrap().phase, RunPhase::Archived);
        assert_eq!(archived.transcript.items, active.transcript.items);
        assert_eq!(archived.draft.text, "A local draft");
        assert!(archived.revision > active.revision);
        // Continued failure keeps Archive; a published unarchive changes it even
        // while the history remains invalid. It never grants submit authority.
        let still = refreshed_snapshot(&mut core, &a, &archived);
        assert_eq!(still.run.as_ref().unwrap().phase, RunPhase::Archived);
        a.archived_session_ids.clear();
        a.sessions.get_mut("a").unwrap().summary.as_mut().unwrap()["status"] = json!("interrupted");
        let resumable = refreshed_snapshot(&mut core, &a, &still);
        assert_closed(&resumable);
        assert_eq!(resumable.run.as_ref().unwrap().phase, RunPhase::Unavailable);
        history(&mut a, "a", false);
        let recovered = refreshed_snapshot(&mut core, &a, &resumable);
        assert_eq!(recovered.run.as_ref().unwrap().phase, RunPhase::Interrupted);
        assert!(recovered.can_submit);
        assert!(recovered.connection.message.is_none());
        assert!(!recovered.transcript.is_stale);
        assert!(recovered.revision > resumable.revision);
        assert!(recovered.submissions.is_empty());
    }
}
#[test]
fn failed_new_destination_drops_the_previous_history_summary_and_submission_receipts() {
    let mut a = authority();
    let (mut core, _) = selected(&a);
    let _ticket = core
        .prepare(&destination("a"), PromptMode::Queue, &a)
        .unwrap();
    core.edit_text(&destination("a"), "A next draft".into())
        .unwrap();
    let previous = core.refresh(&a).unwrap();
    assert!(!previous.transcript.items.is_empty());
    assert!(!previous.submissions.is_empty());
    core.select(destination("b")).unwrap();
    core.edit_text(&destination("b"), "B draft".into()).unwrap();
    history(&mut a, "b", true);
    a.sessions.get_mut("b").unwrap().summary.as_mut().unwrap()["status"] = json!("archived");
    let changed = refreshed_snapshot(&mut core, &a, &previous);
    assert_closed(&changed);
    assert!(changed.transcript.items.is_empty());
    assert!(changed.summary.is_none());
    assert!(changed.submissions.is_empty() && changed.events.is_empty());
    assert_eq!(changed.run.as_ref().unwrap().scope, scope("b"));
    assert_eq!(changed.run.as_ref().unwrap().phase, RunPhase::Archived);
    assert_eq!(changed.draft.text, "B draft");
    assert_eq!(core.draft(&destination("a")).text, "A next draft");
}
#[test]
fn failed_epoch_refresh_uses_current_authority_epoch_without_reusing_child_catalog_facts() {
    let mut a = authority();
    let (mut core, mut previous) = selected(&a);
    previous.child_availability = Some(ChildComposerAvailability::Available);
    history(&mut a, "a", true);
    a.connection_epoch += 1;
    a.archived_session_ids.push("a".into());
    let next = refreshed_snapshot(&mut core, &a, &previous);
    assert_closed(&next);
    assert_eq!(next.connection_epoch, a.connection_epoch);
    assert!(next.child_availability.is_none());
    assert_eq!(next.run.as_ref().unwrap().phase, RunPhase::Archived);
    assert_eq!(next.transcript.items, previous.transcript.items);
}
