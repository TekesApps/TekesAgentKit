//! Structured preset describe and dry-run over the existing public Kernel API.
use crate::*;

impl SharedAgent {
    pub(crate) async fn preset_describe(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
    ) -> Result<Value> {
        let epoch = preset_epoch(payload)?;
        validate_preset_scope(shared, epoch)?;
        let endpoint = shared.connected()?;
        require(
            endpoint.capabilities().supports("session.presets.describe"),
            "structured presets are unavailable",
        )?;
        let description = self.read_preset_description(shared, epoch).await?;
        validate_preset_scope(shared, epoch)?;
        Ok(
            json!({"endpointId":endpoint.identity(),"connectionEpoch":epoch,"description":description}),
        )
    }
    pub(crate) async fn preset_preview(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        let epoch = preset_epoch(payload)?;
        validate_preset_scope(shared, epoch)?;
        let endpoint = shared.connected()?;
        require(
            endpoint.capabilities().supports("session.presets.resolve")
                && endpoint.capabilities().supports("session.presets.describe"),
            "structured preset previews are unavailable",
        )?;
        // Execution arrives only through the embedding host's trusted private service pipe.
        // It is deliberately absent from the returned browser-safe preview.
        let request: SessionPresetPreviewRequest = serde_json::from_value(
            json!({"execution":payload["execution"],"preset":payload["preset"]}),
        )?;
        request.execution.validate()?;
        let description = self.read_preset_description(shared, epoch).await?;
        request.preset.validate_description(&description)?;
        let raw = shared
            .runtime
            .call_at_epoch(
                epoch,
                "session.presets.resolve",
                serde_json::to_value(&request)?,
                rpc_id,
            )
            .await?;
        let resolution: SessionPresetResolution = serde_json::from_value(raw)?;
        require(!resolution.digest.is_empty(), "invalid preset resolution")?;
        validate_preset_scope(shared, epoch)?;
        Ok(
            json!({"endpointId":endpoint.identity(),"connectionEpoch":epoch,"preset":request.preset,"resolution":resolution}),
        )
    }
    async fn read_preset_description(
        &self,
        shared: &SharedEndpoint,
        epoch: u64,
    ) -> Result<SessionPresetDescription> {
        let raw = shared
            .runtime
            .call_at_epoch(
                epoch,
                "session.presets.describe",
                json!({}),
                &format!("preset-description-{}", uuid::Uuid::new_v4()),
            )
            .await?;
        let description: SessionPresetDescription = serde_json::from_value(raw)?;
        require(
            description.format == 1 && description.render_version > 0,
            "unsupported preset format",
        )?;
        Ok(description)
    }
}
fn preset_epoch(payload: &Value) -> Result<u64> {
    payload["connectionEpoch"]
        .as_u64()
        .ok_or_else(|| Error::Protocol("connectionEpoch must be an unsigned integer".into()))
}
fn validate_preset_scope(shared: &SharedEndpoint, epoch: u64) -> Result<()> {
    require(
        matches!(shared.kind.as_str(), "native" | "tekes" | "kernel"),
        "structured Kernel presets are unavailable for this Harness",
    )?;
    shared.runtime.inspect(|state| {
        require(
            state.connection_epoch == epoch && matches!(state.connection, ConnectionState::Ready),
            "preset connection epoch is stale",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn description() -> SessionPresetDescription {
        serde_json::from_value(json!({"format":1,"renderVersion":1,"bases":[{"id":"general"}],"kernelLayers":[{"kind":"kernel","id":"coding","version":"1"}],"limits":{"maxLayers":2,"maxIdBytes":64,"maxVersionBytes":32,"maxIdentityBytes":6,"maxDutyBytes":12,"maxTextBytes":1000,"maxTools":2,"maxMcpServers":1,"maxMcpToolsPerServer":1,"maxSkills":2},"invalidReasons":[],"selectableTools":["read","write"]})).unwrap()
    }
    #[test]
    fn inline_limits_use_utf8_bytes_and_avoid_role_tools() {
        let good:SessionPreset=serde_json::from_value(json!({"layers":[{"kind":"inline","id":"role","version":"1","identity":"角色","duty":"说明"}],"tools":["read"]})).unwrap();
        assert!(good.validate_description(&description()).is_ok());
        let bad: SessionPreset = serde_json::from_value(
            json!({"layers":[{"kind":"inline","id":"role","version":"1","identity":"三个字"}]}),
        )
        .unwrap();
        assert!(bad.validate_description(&description()).is_err());
        let role_tool: SessionPreset = serde_json::from_value(json!({"tools":["plan"]})).unwrap();
        assert!(role_tool.validate_description(&description()).is_err());
    }
    #[test]
    fn duplicate_layers_and_multiple_inline_identities_are_rejected() {
        for value in [
            json!({"layers":[{"kind":"kernel","id":"coding"},{"kind":"inline","id":"coding","version":"1"}]}),
            json!({"layers":[{"kind":"inline","id":"one","version":"1","identity":"one"},{"kind":"inline","id":"two","version":"1","identity":"two"}]}),
        ] {
            let preset: SessionPreset = serde_json::from_value(value).unwrap();
            assert!(preset.validate_description(&description()).is_err());
        }
    }
    #[test]
    fn empty_mcp_readiness_is_unknown_and_resolution_contains_no_execution_authority() {
        let resolution:SessionPresetResolution=serde_json::from_value(json!({"base":"general","layers":[],"presetText":"preview","digest":"sha256:test","mcp":[{"server":"registered","digest":"sha256:definition","tools":["read"]}],"mcpReadiness":[]})).unwrap();
        assert!(resolution.mcp_readiness.unwrap().is_empty());
        let request:SessionPresetPreviewRequest=serde_json::from_value(json!({"execution":{"cwd":"/project","mcp":[{"name":"registered","transport":{"type":"http","url":"https://example.com","credential_id":"host-reference"}}]},"preset":{"mcp":[{"server":"registered","tools":["read"]}]}})).unwrap();
        assert!(request.execution.validate().is_ok());
        assert!(
            !serde_json::to_value(request.preset)
                .unwrap()
                .to_string()
                .contains("host-reference")
        );
    }
}
