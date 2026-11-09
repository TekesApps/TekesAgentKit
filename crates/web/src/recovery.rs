//! Explicit recovery delegates once to SessionRuntime; connection readiness is not recovery success.
use crate::*;
use tekes_agent_session::SessionRecoveryOutcome;

impl SharedAgent {
    pub(crate) fn recovery_read(&self, shared: &SharedEndpoint, payload: &Value) -> Result<Value> {
        let id = string(payload, "sessionId")?;
        let epoch = recovery_epoch(payload)?;
        let endpoint = shared.connected()?;
        require(
            endpoint.supports_recovery() && endpoint.capabilities().supports("sessions.recover"),
            "explicit recovery is unavailable",
        )?;
        shared.runtime.inspect(|state| {
            validate_recovery_scope(state, id, epoch, false)?;
            Ok(
                json!({"endpointId":state.endpoint_id,"sessionId":id,"connectionEpoch":epoch,
                "canRecover":validate_recovery_scope(state,id,epoch,true).is_ok(),
                "outcome":selected_outcome(state.recovery.as_ref(),id,epoch)}),
            )
        })
    }

    pub(crate) async fn recovery_run(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        let id = string(payload, "sessionId")?.to_owned();
        let epoch = recovery_epoch(payload)?;
        let endpoint = shared.connected()?;
        require(
            endpoint.supports_recovery() && endpoint.capabilities().supports("sessions.recover"),
            "explicit recovery is unavailable",
        )?;
        shared
            .runtime
            .inspect(|state| validate_recovery_scope(state, &id, epoch, true))?;
        let runtime = shared.runtime.clone();
        let validation_runtime = runtime.clone();
        let validation_id = id.clone();
        let receipt = runtime
            .call_at_epoch_with_validation(
                epoch,
                "sessions.recover",
                json!({"sessionIds":[id]}),
                rpc_id,
                move || {
                    validation_runtime.inspect(|state| {
                        validate_recovery_scope(state, &validation_id, epoch, true)
                    })
                },
            )
            .await?;
        // SessionRuntime validates and stores the returned IDs before replying.
        Ok(
            json!({"endpointId":endpoint.identity(),"sessionId":id,"connectionEpoch":epoch,"receipt":receipt}),
        )
    }
}
fn recovery_epoch(payload: &Value) -> Result<u64> {
    payload["connectionEpoch"].as_u64().ok_or_else(|| {
        Error::Protocol("recovery connectionEpoch must be an unsigned integer".into())
    })
}
fn validate_recovery_scope(
    state: &MemoryProjectionStore,
    id: &str,
    epoch: u64,
    write: bool,
) -> Result<()> {
    require(
        state.connection_epoch == epoch,
        "recovery connection epoch is stale",
    )?;
    require(
        matches!(state.connection, ConnectionState::Ready) && state.control_ready,
        "recovery requires a ready connection",
    )?;
    let session = state
        .sessions
        .get(id)
        .ok_or_else(|| Error::Protocol("recovery session is unavailable".into()))?;
    require(
        !id.is_empty() && session.summary.is_some(),
        "recovery session summary is unavailable",
    )?;
    if write {
        require(
            session.journal_ready && session.journal_error.is_none(),
            "recovery requires the selected session history to be open",
        )?;
        require(
            !state
                .archived_session_ids
                .iter()
                .any(|archived| archived == id)
                && session
                    .summary
                    .as_ref()
                    .is_none_or(|summary| summary["status"] != "archived"),
            "archived sessions cannot resume execution",
        )?;
        require(
            !session.projections.contains_key("historyAccess"),
            "read-only historical sessions cannot resume execution",
        )?;
    }
    Ok(())
}
fn selected_outcome(
    outcome: Option<&SessionRecoveryOutcome>,
    id: &str,
    epoch: u64,
) -> Option<SessionRecoveryOutcome> {
    let outcome = outcome.filter(|outcome| outcome.connection_epoch == epoch)?;
    let belongs = outcome
        .requested_session_ids
        .iter()
        .chain(&outcome.not_retried_session_ids)
        .any(|entry| entry == id);
    if !belongs {
        return None;
    }
    let filter = |ids: &[String]| {
        ids.iter()
            .filter(|entry| entry.as_str() == id)
            .cloned()
            .collect()
    };
    Some(SessionRecoveryOutcome {
        connection_epoch: epoch,
        requested_session_ids: filter(&outcome.requested_session_ids),
        recovered_session_ids: filter(&outcome.recovered_session_ids),
        unresolved_session_ids: filter(&outcome.unresolved_session_ids),
        not_retried_session_ids: filter(&outcome.not_retried_session_ids),
        error: outcome.error.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recovery_read_is_scoped_and_open_running_history_is_not_mistaken_for_read_only() {
        let mut state = MemoryProjectionStore::new("endpoint");
        state.connection_epoch = 2;
        state.connection = ConnectionState::Ready;
        state.control_ready = true;
        let session = state.sessions.entry("selected".into()).or_default();
        session.summary = Some(json!({"running":true,"blank":false,"updatedAt":1}));
        session.journal_ready = true;
        session.run_open = true;
        assert!(validate_recovery_scope(&state, "selected", 2, true).is_ok());
        assert!(validate_recovery_scope(&state, "selected", 1, true).is_err());
        state.archived_session_ids.push("selected".into());
        assert!(validate_recovery_scope(&state, "selected", 2, false).is_ok());
        assert!(validate_recovery_scope(&state, "selected", 2, true).is_err());
        state.archived_session_ids.clear();
        state
            .sessions
            .get_mut("selected")
            .unwrap()
            .projections
            .insert(
                "historyAccess".into(),
                tekes_agent_session::VersionedValue {
                    sequence: 1,
                    value: json!({"mode":"read-only"}),
                },
            );
        assert!(validate_recovery_scope(&state, "selected", 2, true).is_err());
    }
    #[test]
    fn recovery_receipt_does_not_expose_another_session_or_relabel_a_stale_epoch() {
        let outcome = SessionRecoveryOutcome {
            connection_epoch: 2,
            requested_session_ids: vec!["selected".into(), "other".into()],
            recovered_session_ids: vec!["other".into()],
            unresolved_session_ids: vec!["selected".into()],
            error: Some("unconfirmed".into()),
            ..Default::default()
        };
        let selected = selected_outcome(Some(&outcome), "selected", 2).unwrap();
        assert_eq!(selected.requested_session_ids, ["selected"]);
        assert!(selected.recovered_session_ids.is_empty());
        assert_eq!(selected.unresolved_session_ids, ["selected"]);
        assert!(selected_outcome(Some(&outcome), "missing", 2).is_none());
        assert!(selected_outcome(Some(&outcome), "selected", 3).is_none());
    }
}
