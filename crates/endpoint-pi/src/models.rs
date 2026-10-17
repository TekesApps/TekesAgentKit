use super::*;
impl PiSessionEndpoint {
    pub(crate) async fn model_catalog(&self, rpc: &Rpc) -> Result<Value> {
        let available = rpc.request("get_available_models", json!({})).await?;
        let state = rpc.request("get_state", json!({})).await?;
        let levels = rpc
            .request("get_available_thinking_levels", json!({}))
            .await?;
        let mut groups: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for model in array(&available, "models")? {
            let provider = string(model, "provider")?.to_owned();
            let mut row = json!({"id":model["id"],"name":model["name"],"contextWindow":model["contextWindow"],"native":model});
            if model["reasoning"] == true
                && model["id"] == state["model"]["id"]
                && model["provider"] == state["model"]["provider"]
            {
                row["reasoning"] = json!({"efforts":array(&levels,"levels")?.iter().map(|level|json!({"id":level,"name":level})).collect::<Vec<_>>(),"defaultEffort":state["thinkingLevel"]});
            }
            groups.entry(provider).or_default().push(row);
        }
        let providers: Vec<_> = groups.keys().cloned().collect();
        let current = if state["model"].is_object() {
            selection(&state)
        } else {
            let (provider, models) = groups
                .iter()
                .next()
                .ok_or_else(|| unsupported("Pi has no configured models"))?;
            json!({"provider":provider,"model":models[0]["id"]})
        };
        Ok(
            json!({"current":current,"defaultSelection":current,"routable":providers.iter().any(|id|current["provider"]==*id),"routableProviders":providers,"groups":groups.into_iter().map(|(provider,models)|json!({"id":provider,"name":provider,"models":models})).collect::<Vec<_>>(),"failures":[]}),
        )
    }
    pub(crate) async fn select_model(&self, rpc: &Rpc, payload: &Value) -> Result<Value> {
        let provider = string(payload, "provider")?;
        let model = string(payload, "model")?;
        let selected = rpc
            .request("set_model", json!({"provider":provider,"modelId":model}))
            .await?;
        if let Some(effort) = payload["reasoningEffort"].as_str() {
            require(
                selected["reasoning"] == true || effort == "off",
                "model does not support reasoning effort",
            )?;
            if selected["reasoning"] == true {
                rpc.request("set_thinking_level", json!({"level":effort}))
                    .await?;
            }
        }
        let state = rpc.request("get_state", json!({})).await?;
        require(
            state["model"]["provider"] == provider && state["model"]["id"] == model,
            "model selection receipt mismatch",
        )?;
        Ok(json!({"selected":selection(&state)}))
    }
}
fn selection(state: &Value) -> Value {
    let mut value = json!({"provider":state["model"]["provider"],"model":state["model"]["id"]});
    if state["model"]["reasoning"] == true {
        value["reasoningEffort"] = state["thinkingLevel"].clone();
    }
    value
}
