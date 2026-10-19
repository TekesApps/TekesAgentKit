//! Normalized public session.models consumption; no harness-private state.
use crate::RunBinding;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use tekes_agent_components_ui::{ComposerCatalogPhase, ComposerChoice, ComposerChoiceCatalog};
use tekes_agent_contract::{Result, array, require, string};
use tekes_agent_presentation::{
    ComposerDestination, PreparedSubmission, RunScope, SubmissionPreparation,
};
use tekes_agent_session::SessionRuntime;

pub const MODEL_SELECTION_SETTING: &str = "modelSelection";
#[derive(Clone, Debug)]
pub struct ModelChoices {
    pub catalog: ComposerChoiceCatalog,
    /// Lossless explicit model selection payloads, keyed by framed UI identity.
    pub selections: BTreeMap<String, Value>,
    pub raw: Value,
}
fn identity(provider: &str, model: &str) -> String {
    json!([provider, model]).to_string()
}
impl ModelChoices {
    pub fn read(raw: Value, revision: u64, can_select: bool) -> Result<Self> {
        require(revision > 0, "zero model catalog revision")?;
        let routes = match raw.get("routableProviders") {
            None | Some(Value::Null) => None,
            Some(Value::Array(routes)) => Some(routes),
            _ => {
                return Err(tekes_agent_contract::Error::Protocol(
                    "invalid routable providers".into(),
                ));
            }
        };
        if let Some(routes) = routes {
            require(
                routes.iter().all(Value::is_string),
                "invalid routable providers",
            )?;
        }
        // The original SessionModels contract (Kernel 0.9) has no optional
        // routableProviders extension. Its groups contain configured enabled
        // models; provider readiness failures are explicit catalog facts.
        let legacy_failures = match (
            raw.get("routable").and_then(Value::as_bool),
            raw.get("failures").and_then(Value::as_array),
        ) {
            (Some(_), Some(failures))
                if failures
                    .iter()
                    .all(|failure| failure.get("id").and_then(Value::as_str).is_some()) =>
            {
                Some(failures)
            }
            _ => None,
        };
        let selected = match (
            raw["current"]["provider"].as_str(),
            raw["current"]["model"].as_str(),
        ) {
            (Some(""), Some("")) => None,
            (Some(provider), Some(model)) if !provider.is_empty() && !model.is_empty() => {
                Some(identity(provider, model))
            }
            (None, None) if raw["current"].is_null() => None,
            _ => {
                return Err(tekes_agent_contract::Error::Protocol(
                    "invalid current model".into(),
                ));
            }
        };
        let mut selections = BTreeMap::new();
        let mut choices = vec![];
        for group in array(&raw, "groups")? {
            let provider = string(group, "id")?;
            let route = routes
                .map(|routes| routes.iter().any(|r| r.as_str() == Some(provider)))
                .or_else(|| {
                    legacy_failures.map(|failures| !failures.iter().any(|f| f["id"] == provider))
                });
            for model in array(group, "models")? {
                let model_id = string(model, "id")?;
                require(
                    !provider.is_empty() && !model_id.is_empty(),
                    "empty model identity",
                )?;
                let id = identity(provider, model_id);
                require(
                    !selections.contains_key(&id),
                    "duplicate provider/model identity",
                )?;
                let mut payload = json!({"provider":provider,"model":model_id});
                if selected.as_ref() == Some(&id)
                    && let Some(effort) = raw["current"]["reasoningEffort"].as_str()
                {
                    payload["reasoningEffort"] = json!(effort);
                }
                selections.insert(id.clone(), payload);
                let disabled_reason = if !can_select {
                    Some("当前端点不支持切换模型".into())
                } else if route == Some(false) {
                    Some("供应商当前不可路由".into())
                } else if route.is_none() {
                    Some("供应商路由能力尚未确认".into())
                } else {
                    None
                };
                choices.push(ComposerChoice {
                    id,
                    title: model
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or(model_id)
                        .into(),
                    detail: group
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or(provider)
                        .into(),
                    enabled: disabled_reason.is_none(),
                    disabled_reason,
                });
            }
        }
        Ok(Self {
            catalog: ComposerChoiceCatalog {
                revision,
                phase: ComposerCatalogPhase::Ready,
                choices,
                selected,
            },
            selections,
            raw,
        })
    }
    /// Resolve against the most recently read source, not a callback's display text.
    pub fn selection(&self, id: &str) -> Result<Value> {
        require(
            self.catalog
                .choices
                .iter()
                .any(|choice| choice.id == id && choice.enabled),
            "model choice unavailable",
        )?;
        self.selections
            .get(id)
            .cloned()
            .ok_or_else(|| tekes_agent_contract::unsupported("unknown model choice"))
    }
}
impl RunBinding {
    pub fn supports_model_choices(&self) -> bool {
        self.access() == crate::PaneAccess::Ready
            && self.current_run().is_some()
            && self.runtime.inspect(|s| {
                s.connection == tekes_agent_session::ConnectionState::Ready
                    && s.endpoint_capabilities.methods.contains("session.models")
            })
    }
    pub async fn model_choices(&self, revision: u64, rpc_id: &str) -> Result<ModelChoices> {
        let destination = ComposerDestination::run(self.scope.clone());
        self.require_controls(&destination)?;
        require(self.supports_model_choices(), "model catalog unavailable")?;
        let raw = self
            .runtime
            .call(
                "session.models",
                json!({"sessionId":self.scope.session_id}),
                rpc_id,
            )
            .await?;
        self.require_controls(&destination)?;
        let can_select = self.runtime.inspect(|s| {
            s.endpoint_capabilities
                .methods
                .contains("session.selectModel")
        });
        ModelChoices::read(raw, revision, can_select)
    }
}
/// Explicit opt-in host preparation for this demo/public model-choice setting.
/// Other settings require a different host adapter, never a silent no-op.
pub struct ModelSelectionPreparation;
#[async_trait::async_trait]
impl SubmissionPreparation for ModelSelectionPreparation {
    async fn prepare(
        &self,
        runtime: &SessionRuntime,
        ticket: &PreparedSubmission,
        scope: &RunScope,
    ) -> Result<()> {
        require(
            ticket
                .settings()
                .keys()
                .all(|key| key == MODEL_SELECTION_SETTING),
            "unsupported host draft setting",
        )?;
        let Some(selection) = ticket.settings().get(MODEL_SELECTION_SETTING) else {
            return Ok(());
        };
        require(
            selection.as_object().is_some_and(|o| {
                o.len() >= 2
                    && o.keys()
                        .all(|key| matches!(key.as_str(), "provider" | "model" | "reasoningEffort"))
            }),
            "invalid model selection setting",
        )?;
        let provider = string(selection, "provider")?;
        let model = string(selection, "model")?;
        let effort = selection
            .get("reasoningEffort")
            .map(|_| string(selection, "reasoningEffort"))
            .transpose()?;
        let can_select = runtime.inspect(|s| {
            s.endpoint_capabilities
                .methods
                .contains("session.selectModel")
        });
        let raw = runtime
            .call(
                "session.models",
                json!({"sessionId":scope.session_id}),
                &format!("{}:model-catalog", ticket.id()),
            )
            .await?;
        let options = ModelChoices::read(raw, 1, can_select)?;
        options.selection(&identity(provider, model))?;
        let mut payload = selection.clone();
        payload["sessionId"] = json!(scope.session_id);
        let receipt = runtime
            .call(
                "session.selectModel",
                payload,
                &format!("{}:select-model", ticket.id()),
            )
            .await?;
        require(
            receipt["selected"]["provider"] == provider
                && receipt["selected"]["model"] == model
                && effort.is_none_or(|effort| receipt["selected"]["reasoningEffort"] == effort),
            "model selection receipt mismatch",
        )
    }
}
