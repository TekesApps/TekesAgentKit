use super::*;
// Exact remote argument surfaces read from current ClientKit DSHHostEndpoint.
pub(crate) const DIRECT_ROUTES: &[&str] = &[
    "agentPresets/copy",
    "agentPresets/deletePreset",
    "agentPresets/read",
    "credentials/describe",
    "credentials/set",
    "credentials/unset",
    "directoryPicker/createDirectory",
    "directoryPicker/list",
    "directoryPicker/pick",
    "dynamicCordisRunner/getClientCode",
    "dynamicCordisRunner/inventory",
    "dynamicCordisRunner/invoke",
    "dynamicCordisRunner/reportClientGuardFailure",
    "dynamicCordisRunner/reportRenderFailure",
    "dynamicCordisRunner/resolveInspectQuery",
    "dynamicCordisRunner/resolveRequestRun",
    "dynamicCordisRunner/runHostHalf",
    "dynamicCordisRunner/settleUserRun",
    "dynamicCordisRunner/stopFromPanel",
    "dynamicCordisRunner/syncInspectManifest",
    "dynamicCordisRunner/undefineFromPanel",
    "fileReferences/list",
    "goals/clear",
    "goals/edit",
    "goals/get",
    "llm/discoverModels",
    "llm/listConfigurableProviders",
    "llm/listProviders",
    "pluginInventory/list",
    "session/search",
    "sessionReferenceResolver/candidates",
    "settings/canOpenAgentPresetDirectory",
    "settings/describe",
    "settings/mutate",
    "settings/openAgentPresetDirectory",
    "settings/openSettingsDocument",
    "settings/update",
    "subagents/interruptByParent",
    "subagents/list",
    "subagents/prompt",
    "workspaceFiles/list",
    "workspaceFiles/read",
    "workspaceFiles/readBytes",
    "workspaceFiles/stat",
];
pub(crate) fn route(method: &str, payload: &Value) -> Result<Option<(String, Value)>> {
    if DIRECT_ROUTES.contains(&method) {
        return Ok(Some((method.into(), payload.clone())));
    }
    let direct = match method {
        "settings.describe" => Some("settings/describe"),
        "settings.mutate" => Some("settings/mutate"),
        "settings.update" => Some("settings/update"),
        "settings.document" => Some("settings/openSettingsDocument"),
        "credentials.describe" => Some("credentials/describe"),
        "credentials.set" => Some("credentials/set"),
        "credentials.unset" => Some("credentials/unset"),
        "providers.list" => Some("llm/listProviders"),
        "providers.configurable" => Some("llm/listConfigurableProviders"),
        "models.discover" => Some("llm/discoverModels"),
        "plugins.list" => Some("pluginInventory/list"),
        "presets.read" => Some("agentPresets/read"),
        "presets.copy" => Some("agentPresets/copy"),
        "presets.delete" => Some("agentPresets/deletePreset"),
        "directories.pick" => Some("directoryPicker/pick"),
        "directories.list" => Some("directoryPicker/list"),
        "directories.create" => Some("directoryPicker/createDirectory"),
        _ => None,
    };
    if let Some(endpoint) = direct {
        let mut args = payload.clone();
        if method.starts_with("settings.") {
            if let Some(namespace) = args.as_object_mut().and_then(|map| map.remove("namespace")) {
                args["ns"] = namespace;
            }
            if let Some(ops) = args
                .as_object_mut()
                .and_then(|map| map.remove("operations"))
            {
                args["ops"] = ops;
            }
        }
        return Ok(Some((endpoint.into(), args)));
    }
    let agent = || -> Result<Value> { Ok(json!(string(payload, "sessionId")?)) };
    let result = match method {
        "workspace.create" => ("workspace/create", json!({"request":payload})),
        "workspace.rename" => ("workspace/rename", json!({"request":payload})),
        "workspace.delete" => ("workspace/delete", json!({"request":payload})),
        "workspace.insertBefore" => ("workspace/insertBefore", json!({"request":payload})),
        "workspace.insertSessionBefore" => {
            ("workspace/insertSessionBefore", json!({"request":payload}))
        }
        "skills/list" | "skills.list" => ("skills/list", json!({"request":payload})),
        "session.search" => ("session/search", json!({"request":payload})),
        "session.attachment" => ("session/attachment", json!({"request":payload})),
        "commands/list" | "commands.list" => ("commands/list", json!({"agentId":agent()?})),
        "commands/run" | "commands/execute" | "commands.execute" => {
            let name = string(payload, "name")?;
            require(
                !name.is_empty() && !name.contains(char::is_whitespace),
                "invalid command name",
            )?;
            let line = format!("/{name} {}", payload["arguments"].as_str().unwrap_or(""));
            (
                "commands/execute",
                json!({"agentId":agent()?,"line":line.trim_end(),"submittedAttachments":payload.get("attachments").cloned().unwrap_or(json!([]))}),
            )
        }
        "presets.list" => ("agentPresets/list", json!({})),
        "presets.select" => (
            "agentPresets/select",
            json!({"agentId":agent()?,"agentPreset":payload["agentPreset"]}),
        ),
        "references.files" => (
            "fileReferences/list",
            json!({"agentId":agent()?,"query":payload["query"]}),
        ),
        "references.sessions" => (
            "sessionReferenceResolver/candidates",
            json!({"agentId":agent()?,"query":payload["query"]}),
        ),
        "goals.get" => ("goals/get", json!({"agentId":agent()?})),
        "goals.edit" => (
            "goals/edit",
            json!({"agentId":agent()?,"ref":payload["ref"],"request":{"objective":payload["objective"]}}),
        ),
        "goals.clear" => (
            "goals/clear",
            json!({"agentId":agent()?,"ref":payload["ref"]}),
        ),
        "goals.pause" => (
            "goals/pause",
            json!({"agentId":agent()?,"ref":payload["ref"]}),
        ),
        "goals.resume" => (
            "goals/resume",
            json!({"agentId":agent()?,"ref":payload["ref"]}),
        ),
        "feedback.list" => ("messageFeedback/list", json!({"request":payload})),
        "feedback.put" => ("messageFeedback/put", json!({"request":payload})),
        "feedback.delete" => ("messageFeedback/delete", json!({"request":payload})),
        "subagents.list" => ("subagents/list", payload.clone()),
        _ => return Ok(None),
    };
    Ok(Some((result.0.into(), result.1)))
}
pub(crate) const ALIASES: &[&str] = &[
    "workspace.create",
    "workspace.rename",
    "workspace.delete",
    "workspace.insertBefore",
    "workspace.insertSessionBefore",
    "skills/list",
    "session.search",
    "session.attachment",
    "commands/list",
    "commands/run",
    "presets.list",
    "presets.select",
    "presets.read",
    "presets.copy",
    "presets.delete",
    "references.files",
    "references.sessions",
    "goals.get",
    "goals.edit",
    "goals.clear",
    "goals.pause",
    "goals.resume",
    "feedback.list",
    "feedback.put",
    "feedback.delete",
    "subagents.list",
    "settings.describe",
    "settings.update",
    "settings.mutate",
    "settings.document",
    "credentials.describe",
    "credentials.set",
    "credentials.unset",
    "providers.list",
    "providers.configurable",
    "models.discover",
    "plugins.list",
    "directories.pick",
    "directories.list",
    "directories.create",
];
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn routes_preserve_request_wrappers_and_agent_authority() {
        let (_, args) = route(
            "goals.edit",
            &json!({"sessionId":"s","ref":{"id":"g","revision":4},"objective":"new"}),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            args,
            json!({"agentId":"s","ref":{"id":"g","revision":4},"request":{"objective":"new"}})
        );
        let (_, args) = route(
            "commands/run",
            &json!({"sessionId":"s","name":"compact","arguments":"now","attachments":[]}),
        )
        .unwrap()
        .unwrap();
        assert_eq!(args["line"], "/compact now");
        let (_, args) = route(
            "settings.mutate",
            &json!({"namespace":"x","operations":[],"expectedRevision":2}),
        )
        .unwrap()
        .unwrap();
        assert_eq!(args, json!({"ns":"x","ops":[],"expectedRevision":2}));
        assert!(route("missing", &json!({})).unwrap().is_none());
    }
}
