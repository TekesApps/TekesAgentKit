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
        "feedback.put" => {
            let mut request = payload.clone();
            if request.get("note").is_some_and(Value::is_null) {
                request
                    .as_object_mut()
                    .ok_or_else(|| Error::Protocol("feedback request must be an object".into()))?
                    .remove("note");
            }
            ("messageFeedback/put", json!({"request":request}))
        }
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
/// ClientKit's DSH feedbackCall boundary: errors inside an otherwise successful
/// remote call are failures, and public feedback methods return unwrapped data.
pub(crate) fn normalize_feedback_reply(method: &str, reply: Value) -> Result<Value> {
    if !matches!(method, "feedback.list" | "feedback.put" | "feedback.delete") {
        return Ok(reply);
    }
    match reply["ok"].as_bool() {
        Some(false) => {
            let mut error = reply
                .get("error")
                .filter(|error| error.is_object())
                .ok_or_else(|| Error::Protocol("invalid DSH feedback error envelope".into()))?
                .clone();
            let code = string(&error, "code")?;
            if code == "version-conflict" {
                let current = error.get("current").ok_or_else(|| {
                    Error::Protocol("feedback conflict is missing current".into())
                })?;
                if !current.is_null() {
                    error["current"] = normalize_feedback_item(current.clone())?;
                }
            }
            Err(Error::Remote(error))
        }
        Some(true) => {
            if method == "feedback.delete" {
                return Ok(Value::Null);
            }
            let mut value = reply
                .get("value")
                .ok_or_else(|| Error::Protocol("DSH feedback value is missing".into()))?
                .clone();
            if method == "feedback.list" {
                let items = value
                    .get_mut("items")
                    .and_then(Value::as_array_mut)
                    .ok_or_else(|| Error::Protocol("DSH feedback list is missing items".into()))?;
                for item in items {
                    *item = normalize_feedback_item(item.clone())?;
                }
                SessionMessageFeedbackList::read(value.clone())?;
            } else {
                value = normalize_feedback_item(value)?;
            }
            Ok(value)
        }
        None => Err(Error::Protocol(
            "DSH feedback result must have boolean ok".into(),
        )),
    }
}
fn normalize_feedback_item(mut value: Value) -> Result<Value> {
    if let Some(timestamp) = value
        .get("updatedAt")
        .filter(|value| value.is_number())
        .cloned()
    {
        if let Some(canonical) = value.get("updatedAtMilliseconds") {
            require(
                canonical == &timestamp,
                "conflicting DSH feedback timestamps",
            )?;
        }
        value["updatedAtMilliseconds"] = timestamp;
    }
    let record: SessionMessageFeedback = serde_json::from_value(value.clone())?;
    record.validate()?;
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn feedback_note_clearing_and_server_timestamps_use_the_common_contract() {
        let(_,request)=route("feedback.put",&json!({"sessionId":"s","messageId":"m","rating":"positive","ifVersion":null,"note":null})).unwrap().unwrap();
        assert!(request["request"].get("note").is_none());
        let (_, request) = route("feedback.put", &json!({"sessionId":"s","note":"中文"}))
            .unwrap()
            .unwrap();
        assert_eq!(request["request"]["note"], "中文");
        let record = json!({"messageId":"m","rating":"positive","version":"v","updatedAt":1234});
        let saved =
            normalize_feedback_reply("feedback.put", json!({"ok":true,"value":record.clone()}))
                .unwrap();
        assert_eq!(saved["updatedAtMilliseconds"], 1234);
        assert_eq!(saved["updatedAt"], 1234);
        let listed = normalize_feedback_reply(
            "feedback.list",
            json!({"ok":true,"value":{"items":[record.clone()]}}),
        )
        .unwrap();
        assert_eq!(listed["items"][0]["updatedAtMilliseconds"], 1234);
        let Err(Error::Remote(conflict)) = normalize_feedback_reply(
            "feedback.put",
            json!({"ok":false,"error":{"code":"version-conflict","current":record}}),
        ) else {
            panic!("conflict")
        };
        assert_eq!(conflict["current"]["updatedAtMilliseconds"], 1234);
    }
    #[test]
    fn feedback_success_is_unwrapped_without_changing_other_extension_results() {
        let record = json!({"messageId":"m","rating":"positive","version":"v","note":"中文"});
        assert_eq!(
            normalize_feedback_reply(
                "feedback.list",
                json!({"ok":true,"value":{"items":[record.clone()]}})
            )
            .unwrap(),
            json!({"items":[record.clone()]})
        );
        assert_eq!(
            normalize_feedback_reply("feedback.put", json!({"ok":true,"value":record.clone()}))
                .unwrap(),
            record
        );
        assert_eq!(
            normalize_feedback_reply("feedback.delete", json!({"ok":true})).unwrap(),
            Value::Null
        );
        let opaque = json!({"ok":false,"error":{"code":"vendor"}});
        assert_eq!(
            normalize_feedback_reply("goals.get", opaque.clone()).unwrap(),
            opaque
        );
    }
    #[test]
    fn feedback_conflicts_and_rejections_are_remote_failures_with_current_preserved() {
        for error in [
            json!({"code":"version-conflict","current":null}),
            json!({"code":"version-conflict","current":{"messageId":"m","rating":"negative","version":"new"}}),
            json!({"code":"rejected","detail":"denied"}),
        ] {
            match normalize_feedback_reply(
                "feedback.put",
                json!({"ok":false,"error":error.clone()}),
            ) {
                Err(Error::Remote(actual)) => assert_eq!(actual, error),
                other => panic!("expected preserved remote feedback failure, got {other:?}"),
            }
        }
    }
    #[test]
    fn malformed_feedback_envelopes_never_become_empty_or_successful() {
        for (method, reply) in [
            ("feedback.list", json!({"value":{"items":[]}})),
            ("feedback.list", json!({"ok":true})),
            ("feedback.list", json!({"ok":true,"value":{"items":null}})),
            (
                "feedback.put",
                json!({"ok":true,"value":{"messageId":"m","rating":"positive"}}),
            ),
            ("feedback.delete", json!({"ok":false})),
            (
                "feedback.delete",
                json!({"ok":false,"error":{"code":"version-conflict"}}),
            ),
        ] {
            assert!(normalize_feedback_reply(method, reply).is_err());
        }
    }
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
