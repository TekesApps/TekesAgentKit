//! Bounded, side-effect-free Sidebar lookup from the canonical memory projection.
use super::*;
#[derive(Clone, Debug, PartialEq)]
pub struct ThreadInventoryRead {
    pub address: SessionAddress,
    pub connection_epoch: u64,
    pub inventory_ready: bool,
    pub connection: ConnectionState,
    /// Same item schema as the public inventory. No journal/history is cloned.
    pub item: Option<Value>,
    pub actions: Option<ThreadActionsPresentation>,
}
impl SharedAgent {
    pub fn read_thread_inventory(
        &self,
        shared: &SharedEndpoint,
        session_id: &str,
    ) -> Result<ThreadInventoryRead> {
        require(!session_id.is_empty(), "empty Sidebar selected session")?;
        let epoch = shared.runtime.inspect(|s| s.connection_epoch);
        let inputs = self.thread_action_inputs(shared, session_id);
        shared.runtime.inspect(|state| {
            require(
                state.connection_epoch == epoch,
                "Sidebar source changed during host metadata read",
            )?;
            let item = inventory_summary(state, session_id)
                .map(|row| self.decorate_inventory_row_with_pin(state, row, inputs.pin.as_ref()));
            let actions = item
                .as_ref()
                .map(|_| self.thread_actions_in_state(shared, session_id, state, &inputs))
                .transpose()?;
            Ok(ThreadInventoryRead {
                address: SessionAddress {
                    endpoint_id: state.endpoint_id.clone(),
                    session_id: session_id.into(),
                },
                connection_epoch: state.connection_epoch,
                inventory_ready: state.inventory_ready,
                connection: state.connection.clone(),
                item,
                actions,
            })
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use tekes_agent_session::{SessionProjection, VersionedValue};
    #[test]
    fn selected_and_inventory_reads_share_archive_membership_and_canonical_projection_clears() {
        let mut state = MemoryProjectionStore::new("fixture");
        let mut session = SessionProjection {
            summary: Some(json!({"sessionId":"a","title":"old summary","status":"active"})),
            ..Default::default()
        };
        session.projections.insert(
            "title".into(),
            VersionedValue {
                sequence: 2,
                value: Value::Null,
            },
        );
        state.sessions.insert("a".into(), session);
        let selected = inventory_summary(&state, "a").unwrap();
        assert_eq!(active_inventory(&state), vec![selected.clone()]);
        assert!(selected["projections"]["title"].is_null());
        assert!(selected.get("entries").is_none());
        state
            .sessions
            .get_mut("a")
            .unwrap()
            .summary
            .as_mut()
            .unwrap()["status"] = json!("archived");
        assert!(inventory_summary(&state, "a").is_none());
        assert!(active_inventory(&state).is_empty());
        assert!(
            state.sessions["a"].summary.is_some(),
            "display exclusion does not delete retained authority"
        );
    }
}
