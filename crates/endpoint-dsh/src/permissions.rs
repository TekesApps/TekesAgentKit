use super::*;
fn declared() -> Value {
    json!({"threadLevel":[{"value":"read-only","title":"Read Only"},{"value":"workspace-write","title":"Workspace Write"},{"value":"danger-full-access","title":"Full Access"}],"serverLevel":{"options":[],"currentValue":null}})
}
fn selection_args(id: &str, mode: &str, state: &Value) -> Result<Value> {
    require(
        mode != "custom"
            && state["options"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|row| row["value"] == mode)),
        "Unsupported DSH permission mode",
    )?;
    require(
        !mode.chars().any(char::is_whitespace),
        "Invalid permission command argument",
    )?;
    Ok(json!({"agentId":id,"line":format!("/permission {mode}"),"submittedAttachments":[]}))
}
fn permission_state_from_projection(value: &Value) -> Result<Value> {
    require(
        value.is_object() && (value["currentValue"].is_null() || value["currentValue"].is_string()),
        "DSH permission projection unavailable",
    )?;
    let mut state = value.clone();
    // Current DSH projects the selected value; ClientKit's DSH adapter declares the catalog.
    // An explicit catalog from a newer Harness takes precedence.
    if state.get("options").is_none() {
        state["options"] = declared()["threadLevel"].clone();
    }
    require(
        state["options"].is_array(),
        "Invalid DSH permission options",
    )?;
    Ok(state)
}
impl DshSessionEndpoint {
    async fn permission_state(&self, id: &str, _rpc_id: &str) -> Result<Value> {
        let mut stream = self
            .remote
            .open(
                "session/follow",
                json!({"request":{"address":self.history_address(id).await?,"maxMessages":1}}),
            )
            .await?;
        let value = tokio::time::timeout(Duration::from_secs(30), stream.next())
            .await
            .map_err(|_| Error::Timeout)?
            .ok_or(Error::Disconnected)??;
        drop(stream);
        require(
            value["type"] == "snapshot",
            "DSH permissions need an opening session snapshot",
        )?;
        permission_state_from_projection(&value["projections"]["values"]["permissions"])
    }
    pub(crate) async fn permission_call(
        &self,
        method: &str,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        match method {
            "approvals.policy" => {
                let mut policy = declared();
                if let Some(id) = payload["sessionId"].as_str() {
                    policy["threadLevel"] =
                        self.permission_state(id, rpc_id).await?["options"].clone();
                }
                Ok(policy)
            }
            "approvals.mode" => {
                let state = self
                    .permission_state(string(payload, "sessionId")?, rpc_id)
                    .await?;
                Ok(
                    json!({"mode":if state["currentValue"] == "custom" { Value::Null } else { state["currentValue"].clone() }}),
                )
            }
            "approvals.select" => {
                let id = string(payload, "sessionId")?;
                let mode = string(payload, "mode")?;
                let state = self.permission_state(id, rpc_id).await?;
                let args = selection_args(id, mode, &state)?;
                self.remote.call("commands/execute", args, rpc_id).await?;
                Ok(json!({"accepted":true}))
            }
            _ => Err(unsupported("unknown permission method")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modes_use_the_harness_catalog_and_native_permission_command() {
        let policy = declared();
        let _: PermissionPolicy = serde_json::from_value(policy.clone()).unwrap();
        let state =
            permission_state_from_projection(&json!({"currentValue":"workspace-write"})).unwrap();
        assert_eq!(state["options"], policy["threadLevel"]);
        assert!(permission_state_from_projection(&Value::Null).is_err());
        for mode in ["read-only", "workspace-write", "danger-full-access"] {
            assert_eq!(
                selection_args("session", mode, &state).unwrap(),
                json!({"agentId":"session","line":format!("/permission {mode}"),"submittedAttachments":[]})
            );
        }
        assert!(selection_args("session", "unknown", &state).is_err());
        assert!(
            selection_args(
                "session",
                "custom",
                &json!({"options":[{"value":"custom"}]})
            )
            .is_err()
        );
        assert!(
            selection_args(
                "session",
                "read-only\n/other",
                &json!({"options":[{"value":"read-only\n/other"}]})
            )
            .is_err()
        );
    }
}
