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

#[test]
fn source_outcome_never_marks_retained_history_as_a_fresh_projection() {
    let mut authority = authority();
    let (mut core, original) = selected(&authority);
    history(&mut authority, "a", true);
    let (retained, succeeded) = refreshed_snapshot_outcome(&mut core, &authority, &original);
    assert!(!succeeded);
    assert_closed(&retained);
    assert_eq!(retained.transcript.items, original.transcript.items);
    assert!(retained.revision > original.revision);

    history(&mut authority, "a", false);
    let (recovered, succeeded) = refreshed_snapshot_outcome(&mut core, &authority, &retained);
    assert!(succeeded);
    assert!(!recovered.transcript.is_stale);
    assert_eq!(recovered.connection_epoch, authority.connection_epoch);
    assert_eq!(recovered.selection, core.selection());
}

// A delayed authority watch notification must not repeat a synchronous projection
// of the same authority and unchanged local core. No scheduler timing is needed.
#[tokio::test(flavor = "current_thread")]
async fn repeated_authority_refresh_reuses_snapshot_and_retains_local_changes() {
    struct PendingConnector;
    #[async_trait]
    impl EndpointConnector for PendingConnector {
        async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
            std::future::pending().await
        }
    }
    let runtime = Arc::new(SessionRuntime::spawn(
        "fallback",
        Arc::new(PendingConnector),
    ));
    let controller = RunController::new(runtime).unwrap();
    controller.select(destination("a")).unwrap();
    controller
        .edit_text(&destination("a"), "first".into())
        .unwrap();
    let first = controller.snapshot();
    controller.shared.refresh();
    let repeated = controller.snapshot();
    assert!(
        Arc::ptr_eq(&first, &repeated),
        "same authority/local state must not be recomputed or republished"
    );
    assert_eq!(first.revision, repeated.revision);
    // Even without a synchronous publish, a real local mutation invalidates the key.
    controller
        .shared
        .core
        .lock()
        .unwrap()
        .edit_text(&destination("a"), "second".into())
        .unwrap();
    controller.shared.refresh();
    let second = controller.snapshot();
    assert_eq!(second.draft.text, "second");
    assert!(second.revision > first.revision);
    controller.shared.refresh();
    assert!(Arc::ptr_eq(&second, &controller.snapshot()));
}

#[tokio::test(flavor = "current_thread")]
async fn cached_projection_tracks_runtime_epoch_and_reconnection_without_local_edit() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct EpochConnector {
        attempts: AtomicUsize,
        reconnect: Arc<tokio::sync::Notify>,
        entered: Arc<tokio::sync::Notify>,
    }
    struct EpochEndpoint(HostDescription);
    #[async_trait]
    impl EndpointConnector for EpochConnector {
        async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
            if self.attempts.fetch_add(1, Ordering::SeqCst) > 0 {
                self.entered.notify_one();
                self.reconnect.notified().await;
            }
            let host = serde_json::from_value(
                json!({"protocolVersion":4,"product":{"name":"epoch-test","version":"1"},"capabilities":[],"cwd":"/test","home":"/test","attachedSessions":0,"canOpenPath":false}),
            )?;
            Ok(Arc::new(EpochEndpoint(host)))
        }
    }
    #[async_trait]
    impl SessionEndpoint for EpochEndpoint {
        async fn close(&self) {}
        fn identity(&self) -> &str {
            "fallback"
        }
        fn host(&self) -> &HostDescription {
            &self.0
        }
        fn capabilities(&self) -> EndpointCapabilities {
            EndpointCapabilities::default()
        }
        async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
            use futures_util::{StreamExt, stream};
            let frame = match target {
                StreamTarget::SessionInventory => {
                    json!({"type":"baseline","generation":"epoch","snapshot":{"items":[]}})
                }
                StreamTarget::SessionControl => {
                    json!({"type":"baseline","generation":"epoch","snapshot":{"queues":[],"jobs":[],"projections":[]}})
                }
                StreamTarget::Actionables => json!({"type":"reset","generation":"epoch"}),
                _ => unreachable!(),
            };
            Ok(Box::pin(
                stream::once(async { Ok(frame) }).chain(stream::pending()),
            ))
        }
        async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
            unreachable!()
        }
        async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
            unreachable!()
        }
        async fn call(&self, _: &str, _: Value, _: &str) -> Result<Value> {
            unreachable!()
        }
    }
    let reconnect = Arc::new(tokio::sync::Notify::new());
    let entered = Arc::new(tokio::sync::Notify::new());
    let runtime = Arc::new(SessionRuntime::spawn(
        "fallback",
        Arc::new(EpochConnector {
            attempts: AtomicUsize::new(0),
            reconnect: reconnect.clone(),
            entered: entered.clone(),
        }),
    ));
    let controller = RunController::new(runtime.clone()).unwrap();
    // Drive the same watcher callback explicitly so authority changes are tested
    // against a populated cache without relying on task scheduling order.
    controller.observer.abort();
    controller.shared.refresh();
    runtime
        .wait_ready(std::time::Duration::from_secs(2))
        .await
        .unwrap();
    controller.shared.refresh();
    let ready = controller.snapshot();
    assert_eq!(ready.connection.phase, ConnectionPhase::Connected);
    controller.shared.refresh();
    assert!(Arc::ptr_eq(&ready, &controller.snapshot()));
    let reconnect_runtime = runtime.clone();
    let reconnecting = tokio::spawn(async move { reconnect_runtime.reconnect().await });
    tokio::time::timeout(std::time::Duration::from_secs(2), entered.notified())
        .await
        .unwrap();
    controller.shared.refresh();
    let connecting = controller.snapshot();
    assert_eq!(connecting.connection.phase, ConnectionPhase::Reconnecting);
    assert!(connecting.connection_epoch > ready.connection_epoch);
    assert!(connecting.revision > ready.revision);
    reconnect.notify_one();
    reconnecting.await.unwrap().unwrap();
    controller.shared.refresh();
    let recovered = controller.snapshot();
    assert_eq!(recovered.connection.phase, ConnectionPhase::Connected);
    assert!(recovered.connection_epoch > connecting.connection_epoch);
    assert!(recovered.revision > connecting.revision);
    assert_eq!(recovered.connection.message, None);
    controller.shared.refresh();
    assert!(Arc::ptr_eq(&recovered, &controller.snapshot()));
    runtime.stop().await.unwrap();
}
