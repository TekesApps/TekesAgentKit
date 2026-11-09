//! Feedback uses public response identity and committed per-message CAS versions.
use crate::*;
use sha2::{Digest, Sha256};
use tekes_agent_presentation::{
    FeedbackChange, FeedbackTargetCapture, FeedbackWritePlan, TranscriptDocument,
    TurnFeedbackTarget, plan_feedback_change,
};

impl SharedAgent {
    pub(crate) async fn feedback_read(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        let session_id = string(payload, "sessionId")?;
        let item_id = string(payload, "itemId")?;
        let epoch = shared.runtime.snapshot().connection_epoch;
        let endpoint = shared.connected()?;
        let caps = endpoint.capabilities();
        require(
            caps.supports("feedback.list"),
            "feedback reading is unavailable",
        )?;
        let target = shared.runtime.inspect(|state| {
            validate_feedback_scope(state, session_id, epoch)?;
            feedback_target(state, session_id, item_id)
        })?;
        let Some((capture, target)) = target else {
            return Ok(
                json!({"endpointId":endpoint.identity(),"sessionId":session_id,
                "connectionEpoch":epoch,"target":null,"feedback":null,"canPut":false,"canDelete":false}),
            );
        };
        let raw = shared
            .runtime
            .call_at_epoch(
                epoch,
                "feedback.list",
                json!({"sessionId":session_id}),
                rpc_id,
            )
            .await?;
        let records = SessionMessageFeedbackList::read(raw)?;
        shared.runtime.inspect(|state| {
            validate_feedback_capture(state, session_id, item_id, epoch, &capture)
        })?;
        let current = records.get(capture.message_id());
        Ok(
            json!({"endpointId":endpoint.identity(),"sessionId":session_id,
            "connectionEpoch":epoch,"target":target,"feedback":current,
            "canPut":caps.supports("feedback.put"),"canDelete":caps.supports("feedback.delete") && current.is_some()}),
        )
    }

    pub(crate) async fn feedback_action(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        let session_id = string(payload, "sessionId")?.to_owned();
        let item_id = string(payload, "itemId")?.to_owned();
        let message_id = string(payload, "messageId")?;
        let epoch = payload["connectionEpoch"].as_u64().ok_or_else(|| {
            Error::Protocol("feedback connectionEpoch must be an unsigned integer".into())
        })?;
        let (capture, target) = shared.runtime.inspect(|state| {
            validate_feedback_scope(state, &session_id, epoch)?;
            feedback_target(state, &session_id, &item_id)?
                .ok_or_else(|| Error::Protocol("feedback response is no longer available".into()))
        })?;
        require(
            capture.message_id() == message_id,
            "feedback target identity changed",
        )?;
        require(
            payload["responseRevision"] == target["responseRevision"],
            "feedback response changed; reload before editing",
        )?;
        let endpoint = shared.connected()?;
        require(
            endpoint.capabilities().supports("feedback.list"),
            "feedback reading is unavailable",
        )?;
        let raw = shared
            .runtime
            .call_at_epoch(
                epoch,
                "feedback.list",
                json!({"sessionId":session_id}),
                &format!("feedback-preflight-{}", uuid::Uuid::new_v4()),
            )
            .await?;
        let records = SessionMessageFeedbackList::read(raw)?;
        let current = records.get(message_id);
        check_observed_version(payload, current)?;
        let plan = plan_feedback_change(message_id, current, feedback_change(&payload["change"])?)?;
        let Some(method) = plan.method() else {
            return Ok(
                json!({"endpointId":endpoint.identity(),"sessionId":session_id,
                "connectionEpoch":epoch,"acknowledged":true,"unchanged":true}),
            );
        };
        require(
            endpoint.capabilities().supports(method),
            "feedback change is unavailable",
        )?;
        let request = plan
            .payload(&session_id)
            .expect("feedback mutation has a payload");
        let runtime = shared.runtime.clone();
        let validation_runtime = runtime.clone();
        let validation_session = session_id.clone();
        let receipt = runtime
            .call_at_epoch_with_validation(epoch, method, request, rpc_id, move || {
                validation_runtime.inspect(|state| {
                    validate_feedback_capture(state, &validation_session, &item_id, epoch, &capture)
                })
            })
            .await?;
        if matches!(plan, FeedbackWritePlan::Put { .. }) {
            let saved: SessionMessageFeedback = serde_json::from_value(receipt.clone())?;
            saved.validate()?;
            require(
                saved.message_id == message_id,
                "feedback receipt identity mismatch",
            )?;
        }
        Ok(
            json!({"endpointId":endpoint.identity(),"sessionId":session_id,
            "connectionEpoch":epoch,"acknowledged":true,"receipt":receipt}),
        )
    }
}

fn check_observed_version(payload: &Value, current: Option<&SessionMessageFeedback>) -> Result<()> {
    let observed = match payload.get("ifVersion") {
        Some(Value::Null) => None,
        Some(Value::String(value)) if !value.is_empty() => Some(value.as_str()),
        _ => {
            return Err(Error::Protocol(
                "feedback ifVersion must be null or a nonempty string".into(),
            ));
        }
    };
    if observed != current.map(|record| record.version.as_str()) {
        return Err(Error::Remote(
            json!({"code":"version-conflict","current":current}),
        ));
    }
    Ok(())
}
fn feedback_change(value: &Value) -> Result<FeedbackChange> {
    match string(value, "kind")? {
        "toggleRating" => Ok(FeedbackChange::ToggleRating(serde_json::from_value(
            value["rating"].clone(),
        )?)),
        "setRating" => Ok(FeedbackChange::SetRating(serde_json::from_value(
            value["rating"].clone(),
        )?)),
        "setNote" => Ok(FeedbackChange::SetNote(serde_json::from_value(
            value["note"].clone(),
        )?)),
        _ => Err(Error::Protocol("unknown feedback change".into())),
    }
}
fn validate_feedback_scope(
    state: &MemoryProjectionStore,
    session_id: &str,
    epoch: u64,
) -> Result<()> {
    require(
        state.connection_epoch == epoch,
        "feedback connection epoch is stale",
    )?;
    require(
        matches!(state.connection, ConnectionState::Ready),
        "feedback requires a ready connection",
    )?;
    let session = state
        .sessions
        .get(session_id)
        .ok_or_else(|| Error::Protocol("feedback session is unavailable".into()))?;
    require(
        session.journal_ready,
        "feedback requires loaded response history",
    )
}

/// Revision is a consistency witness for the displayed response; the public
/// messageId and feedback ifVersion remain the durable target and CAS authority.
fn target_json(target: &TurnFeedbackTarget<'_>) -> Value {
    let item = target.response.item;
    let content = json!({"turnId":target.turn_id,"messageId":target.message_id,
        "text":item.text,"reasoning":item.reasoning,"attachments":item.attachments,
        "contentBlocks":item.content_blocks,"reasoningEnded":item.reasoning_ended});
    let revision = format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(&content).expect("response witness JSON"))
    );
    json!({"itemId":item.id,"turnId":target.turn_id,"messageId":target.message_id,"responseRevision":revision})
}
fn feedback_target(
    state: &MemoryProjectionStore,
    session_id: &str,
    item_id: &str,
) -> Result<Option<(FeedbackTargetCapture, Value)>> {
    let session = state
        .sessions
        .get(session_id)
        .ok_or_else(|| Error::Protocol("feedback session is unavailable".into()))?;
    let scope = RunScope::main(&state.endpoint_id, session_id);
    let snapshot = TranscriptSnapshot::read(&scope, session)?;
    let document = TranscriptDocument::read(&snapshot)?;
    for section in &document.sections {
        if let Some(target) = TurnFeedbackTarget::read(section)
            && target.response.item.id == item_id
        {
            return Ok(Some((target.capture(&scope)?, target_json(&target))));
        }
    }
    Ok(None)
}
fn validate_feedback_capture(
    state: &MemoryProjectionStore,
    session_id: &str,
    item_id: &str,
    epoch: u64,
    capture: &FeedbackTargetCapture,
) -> Result<()> {
    validate_feedback_scope(state, session_id, epoch)?;
    let scope = RunScope::main(&state.endpoint_id, session_id);
    require(capture.scope() == &scope, "feedback response scope changed")?;
    let session = &state.sessions[session_id];
    let snapshot = TranscriptSnapshot::read(&scope, session)?;
    let document = TranscriptDocument::read(&snapshot)?;
    require(
        document
            .sections
            .iter()
            .filter_map(TurnFeedbackTarget::read)
            .any(|target| target.response.item.id == item_id && capture.matches(target)),
        "feedback response changed; reload before editing",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tekes_agent_session::SessionProjection;

    fn state(explicit_identity: bool) -> MemoryProjectionStore {
        let mut state = MemoryProjectionStore::new("feedback-endpoint");
        state.connection = ConnectionState::Ready;
        state.connection_epoch = 7;
        let mut session = SessionProjection::default();
        let mut message = json!({"content":[{"type":"text","text":"answer"}]});
        if explicit_identity {
            message["id"] = json!("assistant-public");
        }
        let rows = [
            ("turn/start", json!({"turn":1})),
            (
                "assistant/message",
                json!({"turn":1,"step":1,"message":message}),
            ),
            ("turn/end", json!({"turn":1,"reason":"completed"})),
        ];
        let entries = rows
            .into_iter()
            .enumerate()
            .map(|(seq, (kind, data))| {
                serde_json::from_value(
                    json!({"event":{"seq":seq,"time":1,"type":kind,"data":data}}),
                )
                .unwrap()
            })
            .collect();
        session
            .apply_snapshot(JournalSnapshot {
                session_id: "session".into(),
                generation: "g".into(),
                through_sequence: 2,
                window_limit: 50,
                entries,
                has_more_before: false,
                projections: None,
                history_revision: None,
            })
            .unwrap();
        state.sessions.insert("session".into(), session);
        state
    }
    #[test]
    fn targets_require_public_final_response_identity_and_reject_changed_content() {
        let state = state(true);
        let scope = RunScope::main("feedback-endpoint", "session");
        let snapshot = TranscriptSnapshot::read(&scope, &state.sessions["session"]).unwrap();
        let item_id = snapshot
            .items
            .iter()
            .find(|item| item.kind == tekes_agent_presentation::TranscriptItemKind::Assistant)
            .unwrap()
            .id
            .clone();
        let (capture, target) = feedback_target(&state, "session", &item_id)
            .unwrap()
            .unwrap();
        assert_eq!(target["messageId"], "assistant-public");
        assert!(validate_feedback_capture(&state, "session", &item_id, 7, &capture).is_ok());
        assert!(validate_feedback_capture(&state, "session", &item_id, 8, &capture).is_err());
        let mut changed = state.clone();
        changed
            .sessions
            .get_mut("session")
            .unwrap()
            .entries
            .get_mut(&1)
            .unwrap()
            .event
            .data["message"]["content"][0]["text"] = json!("changed");
        assert!(validate_feedback_capture(&changed, "session", &item_id, 7, &capture).is_err());
        assert!(
            feedback_target(&super::tests::state(false), "session", &item_id)
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn feedback_cas_preserves_authoritative_absence_and_never_rebases_an_editor() {
        let record = SessionMessageFeedback {
            message_id: "message".into(),
            rating: SessionMessageFeedbackRating::Positive,
            version: "v2".into(),
            note: Some("retained".into()),
            updated_at_milliseconds: None,
        };
        assert!(
            matches!(check_observed_version(&json!({"ifVersion":"v1"}),Some(&record)),Err(Error::Remote(value)) if value["code"]=="version-conflict" && value["current"]["version"]=="v2")
        );
        assert!(check_observed_version(&json!({"ifVersion":null}), None).is_ok());
        assert!(check_observed_version(&json!({}), None).is_err());
        let plan = plan_feedback_change(
            "message",
            Some(&record),
            feedback_change(&json!({"kind":"setNote","note":null})).unwrap(),
        )
        .unwrap();
        assert_eq!(
            plan.payload("session").unwrap(),
            json!({"sessionId":"session","messageId":"message","rating":"positive","ifVersion":"v2","note":null})
        );
    }

    #[derive(Clone)]
    struct FeedbackConnector {
        calls: Arc<std::sync::Mutex<Vec<(String, Value)>>>,
        fail_write: bool,
    }
    struct FeedbackEndpoint {
        connector: FeedbackConnector,
        host: HostDescription,
    }
    #[async_trait]
    impl EndpointConnector for FeedbackConnector {
        async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
            Ok(Arc::new(FeedbackEndpoint {
                connector: self.clone(),
                host: serde_json::from_value(
                    json!({"protocolVersion":4,"product":{"name":"feedback-fixture","version":"1"},"capabilities":["session-journal","session-inventory-sync","session-control-sync","actionable-sync"],"cwd":"/work","home":"/home","attachedSessions":0,"canOpenPath":false}),
                )?,
            }))
        }
    }
    #[async_trait]
    impl SessionEndpoint for FeedbackEndpoint {
        fn identity(&self) -> &str {
            "feedback-endpoint"
        }
        fn host(&self) -> &HostDescription {
            &self.host
        }
        fn capabilities(&self) -> EndpointCapabilities {
            EndpointCapabilities::methods(&["feedback.list", "feedback.put", "feedback.delete"])
        }
        async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
            use futures_util::stream;
            let frame = match target {
                StreamTarget::SessionInventory => {
                    json!({"type":"baseline","generation":"g","snapshot":{"items":[{"sessionId":"session","running":false,"blank":false,"updatedAt":1,"title":"Feedback"}]}})
                }
                StreamTarget::SessionControl => {
                    json!({"type":"baseline","generation":"g","snapshot":{"queues":[],"jobs":[],"projections":[]}})
                }
                StreamTarget::Actionables => json!({"type":"reset","generation":"g"}),
                StreamTarget::SessionJournal { address, .. } => {
                    json!({"type":"snapshot","snapshot":{"sessionId":address.session_id,"generation":"g","throughSequence":2,"windowLimit":50,"entries":state(true).sessions["session"].entries.values().collect::<Vec<_>>(),"hasMoreBefore":false}})
                }
                _ => return Err(unsupported("feedback fixture stream")),
            };
            Ok(Box::pin(
                stream::once(async { Ok(frame) }).chain(stream::pending()),
            ))
        }
        async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
            Err(unsupported("fixture history"))
        }
        async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
            Err(unsupported("fixture response"))
        }
        async fn call(&self, method: &str, payload: Value, _: &str) -> Result<Value> {
            self.connector
                .calls
                .lock()
                .unwrap()
                .push((method.into(), payload.clone()));
            match method {
                "feedback.list" => Ok(
                    json!({"items":[{"messageId":"assistant-public","rating":"positive","version":"v1","note":"old note"}]}),
                ),
                "feedback.put" if self.connector.fail_write => Err(Error::Timeout),
                "feedback.put" => Ok(
                    json!({"messageId":payload["messageId"],"rating":payload["rating"],"version":"v2","note":payload["note"]}),
                ),
                "feedback.delete" => Ok(Value::Null),
                _ => Err(unsupported(method)),
            }
        }
        async fn close(&self) {}
    }
    #[tokio::test]
    async fn feedback_public_calls_use_proven_identity_cas_and_do_not_retry_unknown_writes() {
        for fail_write in [false, true] {
            let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
            let mut agent = SharedAgent::default();
            agent
                .register(
                    "feedback-endpoint".into(),
                    "native".into(),
                    "Feedback".into(),
                    Arc::new(FeedbackConnector {
                        calls: calls.clone(),
                        fail_write,
                    }),
                )
                .unwrap();
            let endpoint = agent.endpoint("feedback-endpoint").unwrap();
            endpoint
                .runtime
                .wait_ready(std::time::Duration::from_secs(2))
                .await
                .unwrap();
            endpoint.runtime.open_session("session").await.unwrap();
            let projected = endpoint.runtime.snapshot();
            let scope = RunScope::main("feedback-endpoint", "session");
            let snapshot =
                TranscriptSnapshot::read(&scope, &projected.sessions["session"]).unwrap();
            let item_id = &snapshot
                .items
                .iter()
                .find(|item| item.kind == tekes_agent_presentation::TranscriptItemKind::Assistant)
                .unwrap()
                .id;
            let read = agent
                .feedback_read(
                    &endpoint,
                    &json!({"sessionId":"session","itemId":item_id}),
                    "read",
                )
                .await
                .unwrap();
            assert_eq!(read["target"]["messageId"], "assistant-public");
            let mut action = json!({"sessionId":"session","itemId":item_id,"messageId":read["target"]["messageId"],"responseRevision":read["target"]["responseRevision"],"connectionEpoch":read["connectionEpoch"],"ifVersion":"old-version","change":{"kind":"setNote","note":"edited note"}});
            assert!(
                matches!(agent.feedback_action(&endpoint,&action,"stale").await,Err(Error::Remote(value)) if value["code"]=="version-conflict")
            );
            assert!(
                !calls
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|(method, _)| method == "feedback.put")
            );
            action["ifVersion"] = json!("v1");
            let result = agent
                .feedback_action(&endpoint, &action, "write-once")
                .await;
            if fail_write {
                assert!(matches!(result, Err(Error::Timeout)));
            } else {
                assert_eq!(result.unwrap()["acknowledged"], true);
            }
            let writes: Vec<_> = calls
                .lock()
                .unwrap()
                .iter()
                .filter(|(method, _)| method == "feedback.put")
                .cloned()
                .collect();
            assert_eq!(
                writes,
                vec![(
                    "feedback.put".into(),
                    json!({"sessionId":"session","messageId":"assistant-public","rating":"positive","ifVersion":"v1","note":"edited note"})
                )]
            );
            endpoint.runtime.stop().await.unwrap();
        }
    }
}
