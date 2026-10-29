use crate::*;

impl SharedAgent {
    pub(crate) async fn remove_queued_input(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        let session_id = string(payload, "sessionId")?.to_owned();
        let item_id = string(payload, "itemId")?.to_owned();
        let expected_epoch = payload["connectionEpoch"]
            .as_u64()
            .ok_or_else(|| Error::Protocol("connectionEpoch must be an unsigned integer".into()))?;
        let endpoint = shared.connected()?;
        if !endpoint.capabilities().supports("session.updateQueue") {
            return Err(unsupported("agent.queue.remove"));
        }

        shared
            .runtime
            .inspect(|state| validate_remove(state, &session_id, &item_id, expected_epoch))?;
        let runtime = shared.runtime.clone();
        let validate_runtime = runtime.clone();
        let validate_session = session_id.clone();
        let validate_item = item_id.clone();
        runtime
            .call_at_epoch_with_validation(
                expected_epoch,
                "session.updateQueue",
                normalized_remove_payload(&session_id, &item_id),
                rpc_id,
                move || {
                    validate_runtime.inspect(|state| {
                        validate_remove(state, &validate_session, &validate_item, expected_epoch)
                    })
                },
            )
            .await
    }
}

fn normalized_remove_payload(session_id: &str, item_id: &str) -> Value {
    json!({
        "sessionId": session_id,
        "itemId": item_id,
        "action": {"kind": "remove"}
    })
}

fn validate_remove(
    state: &MemoryProjectionStore,
    session_id: &str,
    item_id: &str,
    expected_epoch: u64,
) -> Result<()> {
    require(
        state.connection_epoch == expected_epoch,
        "queue removal connection epoch is stale",
    )?;
    require(
        matches!(state.connection, ConnectionState::Ready),
        "queue removal requires a ready connection",
    )?;
    require(
        state.control_ready,
        "queue removal requires a ready queue projection",
    )?;
    let session = state
        .sessions
        .get(session_id)
        .ok_or_else(|| Error::Protocol("queue removal session is unavailable".into()))?;
    let status = session
        .summary
        .as_ref()
        .and_then(|summary| summary["status"].as_str());
    require(
        session.journal_ready,
        "queue removal requires a ready session journal",
    )?;
    require(
        !state.archived_session_ids.iter().any(|id| id == session_id) && status != Some("archived"),
        "queue removal is unavailable for archived sessions",
    )?;
    let scope = RunScope {
        endpoint_id: state.endpoint_id.clone(),
        thread_id: session_id.to_owned(),
        session_id: session_id.to_owned(),
    };
    require(
        RunPresentation::read(state, &scope, true).can_update_queue,
        "queue removal is unavailable for this run",
    )?;
    require(
        session
            .queue
            .iter()
            .any(|item| item.get("id").and_then(Value::as_str) == Some(item_id)),
        "queued input is no longer present",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready_store() -> MemoryProjectionStore {
        let mut state = MemoryProjectionStore::new("endpoint");
        state.connection = ConnectionState::Ready;
        state.connection_epoch = 4;
        state.control_ready = true;
        state.endpoint_capabilities = EndpointCapabilities::methods(&["session.updateQueue"]);
        let session = state.sessions.entry("session".into()).or_default();
        session.journal_ready = true;
        session.queue = vec![json!({"id":"queued-1","text":"pending"})];
        state
    }

    #[test]
    fn remove_requires_supported_fresh_live_session_and_current_queue_item() {
        let state = ready_store();
        assert!(validate_remove(&state, "session", "queued-1", 4).is_ok());

        let mut unsupported = state.clone();
        unsupported.endpoint_capabilities = EndpointCapabilities::default();
        assert!(validate_remove(&unsupported, "session", "queued-1", 4).is_err());

        let mut not_ready = state.clone();
        not_ready.connection = ConnectionState::Connecting;
        assert!(validate_remove(&not_ready, "session", "queued-1", 4).is_err());

        let mut journal_pending = state.clone();
        journal_pending
            .sessions
            .get_mut("session")
            .unwrap()
            .journal_ready = false;
        assert!(validate_remove(&journal_pending, "session", "queued-1", 4).is_err());

        let mut queue_pending = state.clone();
        queue_pending.control_ready = false;
        assert!(validate_remove(&queue_pending, "session", "queued-1", 4).is_err());

        let mut archived = state.clone();
        archived.archived_session_ids.push("session".into());
        assert!(validate_remove(&archived, "session", "queued-1", 4).is_err());

        let mut historical = state.clone();
        historical
            .sessions
            .get_mut("session")
            .unwrap()
            .projections
            .insert(
                "historyAccess".into(),
                tekes_agent_session::VersionedValue {
                    sequence: 1,
                    value: json!({
                        "mode":"read-only",
                        "reason":"legacy-tool-catalog-unavailable",
                        "sourceDigest":format!("sha256-{}", "a".repeat(64))
                    }),
                },
            );
        assert!(validate_remove(&historical, "session", "queued-1", 4).is_err());
    }
}
