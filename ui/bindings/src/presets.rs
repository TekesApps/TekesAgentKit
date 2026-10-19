//! Initial presets are creation inputs, never an existing-session mutation.
use serde_json::{Value, json};
use std::collections::BTreeSet;
use tekes_agent_components_ui::{
    ComposerCatalogPhase, ComposerChoice, ComposerChoiceCatalog, ComposerChoiceKind,
    ComposerChoiceSelection,
};
use tekes_agent_contract::{CreateSessionRequest, Result, array, require, string};
use tekes_agent_presentation::ComposerDestination;
use tekes_agent_session::SessionRuntime;

#[derive(Clone, Debug)]
pub struct InitialPresetChoices {
    pub catalog: ComposerChoiceCatalog,
    /// None preserves harness automatic selection. A default is not a user gesture.
    pub default_id: Option<String>,
    /// Source remains lossless; configuration text is not used as an instruction here.
    pub raw: Value,
    source_endpoint: Option<String>,
}
fn optional_text<'a>(value: &'a Value, key: &str) -> Result<Option<&'a str>> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text)),
        _ => Err(tekes_agent_contract::Error::Protocol(format!(
            "invalid preset {key}"
        ))),
    }
}
impl InitialPresetChoices {
    /// Accepts session.initialPresets and DSH presets.list roster vocabulary.
    pub fn read(raw: Value, revision: u64, can_create: bool) -> Result<Self> {
        require(revision > 0, "zero preset catalog revision")?;
        let rows = array(&raw, "presets")?;
        require(rows.len() <= 4096, "preset catalog too large")?;
        let mut ids = BTreeSet::new();
        let mut defaults = vec![];
        let mut choices = vec![];
        for row in rows {
            let id = string(row, "id")?;
            require(
                !id.is_empty() && ids.insert(id),
                "empty or duplicate preset",
            )?;
            let title = optional_text(row, "title")?
                .or(optional_text(row, "name")?)
                .unwrap_or(id);
            let detail = optional_text(row, "description")?.unwrap_or("");
            // Invalid source fields are errors, never silently selectable rows.
            optional_text(row, "configurationText")?;
            let broken = optional_text(row, "unavailableReason")?.or(optional_text(row, "broken")?);
            match row.get("isDefault") {
                Some(Value::Bool(true)) => defaults.push(id.to_owned()),
                None | Some(Value::Null) | Some(Value::Bool(false)) => {}
                _ => return Err(tekes_agent_contract::unsupported("invalid preset default")),
            }
            let disabled_reason = if !can_create {
                Some("当前端点不支持创建会话".to_owned())
            } else {
                broken.map(str::to_owned)
            };
            choices.push(ComposerChoice {
                id: id.into(),
                title: title.into(),
                detail: detail.into(),
                enabled: disabled_reason.is_none(),
                disabled_reason,
            });
        }
        require(defaults.len() <= 1, "ambiguous default preset")?;
        let default_id = optional_text(&raw, "defaultID")?
            .map(str::to_owned)
            .or_else(|| defaults.first().cloned());
        require(
            default_id
                .as_ref()
                .is_none_or(|id| ids.contains(id.as_str()))
                && defaults
                    .first()
                    .is_none_or(|id| default_id.as_ref() == Some(id)),
            "unknown or conflicting default preset",
        )?;
        Ok(Self {
            catalog: ComposerChoiceCatalog {
                revision,
                phase: ComposerCatalogPhase::Ready,
                choices,
                selected: None,
            },
            default_id,
            raw,
            source_endpoint: None,
        })
    }
    /// Read only advertised public methods. The host fences navigation and
    /// catalog-request lifetime before mounting the asynchronous result.
    pub async fn load(runtime: &SessionRuntime, revision: u64, rpc_id: &str) -> Result<Self> {
        let (endpoint, epoch, ready) = runtime.inspect(|s| {
            (
                s.endpoint_id.clone(),
                s.connection_epoch,
                s.connection == tekes_agent_session::ConnectionState::Ready,
            )
        });
        require(ready, "preset connection not ready")?;
        let capabilities = runtime.capabilities().await?;
        let method = if capabilities.supports("session.initialPresets") {
            "session.initialPresets"
        } else {
            require(
                capabilities.supports("presets.list"),
                "initial presets unavailable",
            )?;
            "presets.list"
        };
        let raw = runtime.call_optional(method, json!({}), rpc_id).await?;
        require(
            runtime.inspect(|s| {
                s.connection_epoch == epoch
                    && s.connection == tekes_agent_session::ConnectionState::Ready
            }),
            "preset connection changed",
        )?;
        let mut source = Self::read(raw, revision, capabilities.supports("session.create"))?;
        source.source_endpoint = Some(endpoint);
        Ok(source)
    }
    /// Turn an explicit, still-current choice into the existing controller's
    /// create input. This performs no RPC and never selects a preset on a Run.
    /// Preserve this request/RPC identity on uncertain creation; do not retry it
    /// with a fresh session ID. Before dispatch, revalidate against a fresh source.
    pub fn create_request(
        &self,
        selection: &ComposerChoiceSelection,
        mut input: CreateSessionRequest,
    ) -> Result<CreateSessionRequest> {
        require(
            selection.request.kind == ComposerChoiceKind::Preset
                && selection.request.generation > 0
                && self.catalog.phase == ComposerCatalogPhase::Ready
                && selection.catalog_revision == self.catalog.revision,
            "obsolete or mismatched preset choice",
        )?;
        let ComposerDestination::NewThread { workspace_id, .. } = &selection.request.destination
        else {
            return Err(tekes_agent_contract::unsupported(
                "existing session retains its initial preset",
            ));
        };
        selection.request.destination.validate()?;
        require(
            self.source_endpoint
                .as_ref()
                .is_none_or(|id| id == selection.request.destination.endpoint_id()),
            "preset endpoint mismatch",
        )?;
        require(
            input
                .workspace_id
                .as_ref()
                .is_none_or(|id| id == workspace_id),
            "create workspace mismatch",
        )?;
        require(
            self.catalog.choices.iter().any(|row| {
                row == &selection.choice && row.enabled && row.disabled_reason.is_none()
            }),
            "preset choice unavailable",
        )?;
        require(
            input
                .agent_preset
                .as_ref()
                .is_none_or(|id| id == &selection.choice.id),
            "conflicting host preset input",
        )?;
        input.agent_preset = Some(selection.choice.id.clone());
        Ok(input)
    }
    /// Re-read immediately before controller dispatch; never silently replace a
    /// removed/disabled/changed choice with a default or another preset.
    pub async fn revalidate_create(
        runtime: &SessionRuntime,
        selection: &ComposerChoiceSelection,
        input: CreateSessionRequest,
        rpc_id: &str,
    ) -> Result<CreateSessionRequest> {
        require(
            runtime.inspect(|s| s.endpoint_id == selection.request.destination.endpoint_id()),
            "preset endpoint mismatch",
        )?;
        let fresh = Self::load(runtime, selection.catalog_revision, rpc_id).await?;
        fresh.create_request(selection, input)
    }
}
