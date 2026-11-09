use crate::*;

impl SharedAgent {
    pub(crate) async fn remove_queued_input(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        self.mutate_queued_input(shared, payload, rpc_id, false)
            .await
    }

    pub(crate) async fn edit_queued_input(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        self.mutate_queued_input(shared, payload, rpc_id, true)
            .await
    }

    async fn mutate_queued_input(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        rpc_id: &str,
        editing: bool,
    ) -> Result<Value> {
        let session_id = string(payload, "sessionId")?.to_owned();
        let item_id = string(payload, "itemId")?.to_owned();
        let expected_epoch = payload["connectionEpoch"]
            .as_u64()
            .ok_or_else(|| Error::Protocol("connectionEpoch must be an unsigned integer".into()))?;
        let action = if editing {
            let text = string(payload, "text")?;
            require(
                !text.trim().is_empty(),
                "queue edit requires non-whitespace text",
            )?;
            json!({"kind":"edit","content":[{"type":"text","text":text}]})
        } else {
            json!({"kind":"remove"})
        };
        let endpoint = shared.connected()?;
        if !endpoint.capabilities().supports("session.updateQueue") {
            return Err(unsupported(if editing {
                "agent.queue.edit"
            } else {
                "agent.queue.remove"
            }));
        }
        shared.runtime.inspect(|state| {
            validate_mutation(
                state,
                &shared.kind,
                &session_id,
                &item_id,
                expected_epoch,
                editing,
            )
        })?;
        let runtime = shared.runtime.clone();
        let validate_runtime = runtime.clone();
        let validate_session = session_id.clone();
        let validate_item = item_id.clone();
        let kind = shared.kind.clone();
        runtime
            .call_at_epoch_with_validation(
                expected_epoch,
                "session.updateQueue",
                json!({"sessionId":session_id,"itemId":item_id,"action":action}),
                rpc_id,
                move || {
                    validate_runtime.inspect(|state| {
                        validate_mutation(
                            state,
                            &kind,
                            &validate_session,
                            &validate_item,
                            expected_epoch,
                            editing,
                        )
                    })
                },
            )
            .await
    }
}

fn validate_mutation(
    state: &MemoryProjectionStore,
    kind: &str,
    session_id: &str,
    item_id: &str,
    expected_epoch: u64,
    editing: bool,
) -> Result<()> {
    require(
        state.connection_epoch == expected_epoch,
        "queue mutation connection epoch is stale",
    )?;
    require(
        matches!(state.connection, ConnectionState::Ready),
        "queue mutation requires a ready connection",
    )?;
    require(
        state.control_ready,
        "queue mutation requires a ready queue projection",
    )?;
    let session = state
        .sessions
        .get(session_id)
        .ok_or_else(|| Error::Protocol("queue mutation session is unavailable".into()))?;
    let status = session
        .summary
        .as_ref()
        .and_then(|summary| summary["status"].as_str());
    require(
        session.journal_ready,
        "queue mutation requires a ready session journal",
    )?;
    require(
        !state.archived_session_ids.iter().any(|id| id == session_id) && status != Some("archived"),
        "queue mutation is unavailable for archived sessions",
    )?;
    let scope = RunScope {
        endpoint_id: state.endpoint_id.clone(),
        thread_id: session_id.to_owned(),
        session_id: session_id.to_owned(),
    };
    require(
        RunPresentation::read(state, &scope, true).can_update_queue,
        "queue mutation is unavailable for this run",
    )?;
    let item = session
        .queue
        .iter()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(item_id))
        .ok_or_else(|| Error::Protocol("queued input is no longer present".into()))?;
    if editing {
        let placement = item["placement"].as_str().unwrap_or("queued");
        require(
            placement == "queued" || (kind == "dsh" && placement == "next-step"),
            "this queue placement cannot be edited",
        )?;
        let content = item["message"]["content"]
            .as_array()
            .ok_or_else(|| Error::Protocol("queued input content is unavailable".into()))?;
        // Pi explicitly retains original non-text parts. Kernel/DSH's public
        // text-only edit replaces the full body, so editing those rows would lose attachments.
        require(
            kind == "pi"
                || content
                    .iter()
                    .all(|part| part["type"] == "text" && part["text"].is_string()),
            "this Harness cannot preserve attachments while editing queued input",
        )?;
    }
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
        session.queue = vec![
            json!({"id":"queued-1","placement":"queued","message":{"content":[{"type":"text","text":"pending"}]}}),
        ];
        state
    }

    #[test]
    fn remove_requires_supported_fresh_live_session_and_current_queue_item() {
        let state = ready_store();
        assert!(validate_mutation(&state, "tekes", "session", "queued-1", 4, false).is_ok());

        let mut unsupported = state.clone();
        unsupported.endpoint_capabilities = EndpointCapabilities::default();
        assert!(validate_mutation(&unsupported, "tekes", "session", "queued-1", 4, false).is_err());

        let mut not_ready = state.clone();
        not_ready.connection = ConnectionState::Connecting;
        assert!(validate_mutation(&not_ready, "tekes", "session", "queued-1", 4, false).is_err());

        let mut journal_pending = state.clone();
        journal_pending
            .sessions
            .get_mut("session")
            .unwrap()
            .journal_ready = false;
        assert!(
            validate_mutation(&journal_pending, "tekes", "session", "queued-1", 4, false).is_err()
        );

        let mut queue_pending = state.clone();
        queue_pending.control_ready = false;
        assert!(
            validate_mutation(&queue_pending, "tekes", "session", "queued-1", 4, false).is_err()
        );

        let mut archived = state.clone();
        archived.archived_session_ids.push("session".into());
        assert!(validate_mutation(&archived, "tekes", "session", "queued-1", 4, false).is_err());

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
        assert!(validate_mutation(&historical, "tekes", "session", "queued-1", 4, false).is_err());
    }
    #[test]
    fn edits_preserve_attachment_contract_and_reject_consumed_or_steering_items() {
        let mut state = ready_store();
        assert!(validate_mutation(&state, "tekes", "session", "queued-1", 4, true).is_ok());
        assert!(validate_mutation(&state, "dsh", "session", "queued-1", 5, true).is_err());
        assert!(validate_mutation(&state, "pi", "session", "missing", 4, true).is_err());
        state.sessions.get_mut("session").unwrap().queue[0]["message"]["content"]
            .as_array_mut()
            .unwrap()
            .push(json!({"type":"file","attachment":{"attachmentId":"kept"}}));
        assert!(validate_mutation(&state, "pi", "session", "queued-1", 4, true).is_ok());
        for kind in ["tekes", "dsh"] {
            assert!(validate_mutation(&state, kind, "session", "queued-1", 4, true).is_err());
            assert!(validate_mutation(&state, kind, "session", "queued-1", 4, false).is_ok());
        }
        state.sessions.get_mut("session").unwrap().queue[0]["placement"] = json!("dispatching");
        assert!(validate_mutation(&state, "pi", "session", "queued-1", 4, true).is_err());
        state.sessions.get_mut("session").unwrap().queue[0]["placement"] = json!("steering");
        assert!(validate_mutation(&state, "tekes", "session", "queued-1", 4, true).is_err());
        state.sessions.get_mut("session").unwrap().queue[0]["message"]["content"] =
            json!([{"type":"text","text":"pending"}]);
        state.sessions.get_mut("session").unwrap().queue[0]["placement"] = json!("next-step");
        assert!(validate_mutation(&state, "dsh", "session", "queued-1", 4, true).is_ok());
        assert!(validate_mutation(&state, "tekes", "session", "queued-1", 4, true).is_err());
    }
}
