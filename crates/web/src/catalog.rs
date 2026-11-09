use crate::*;
use tekes_agent_presentation::{ComposerArtifact, ComposerDraft, SubmissionPlan};

impl SharedAgent {
    pub(crate) async fn resource_catalog(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        let session_id = string(payload, "sessionId")?.to_owned();
        let epoch = catalog_epoch(payload)?;
        shared
            .runtime
            .inspect(|s| validate_catalog_session(s, &session_id, epoch, false))?;
        let endpoint = shared.connected()?;
        let supports_commands = endpoint.capabilities().supports("commands/list");
        let supports_skills = endpoint.capabilities().supports("skills/list");
        require(
            supports_commands || supports_skills,
            "resource catalogs are unavailable",
        )?;
        let commands = if supports_commands {
            let raw = shared
                .runtime
                .call_at_epoch(
                    epoch,
                    "commands/list",
                    json!({"sessionId":session_id}),
                    rpc_id,
                )
                .await?;
            normalize_commands(&raw)?
        } else {
            Vec::new()
        };
        let skills = if supports_skills {
            let raw = shared
                .runtime
                .call_at_epoch(
                    epoch,
                    "skills/list",
                    json!({"sessionId":session_id}),
                    rpc_id,
                )
                .await?;
            normalize_skills(&raw)?
        } else {
            Vec::new()
        };
        let can_run = shared.runtime.inspect(|s| {
            validate_catalog_session(s, &session_id, epoch, false)?;
            Ok::<_, Error>(validate_catalog_session(s, &session_id, epoch, true).is_ok())
        })? && endpoint.capabilities().supports("commands/run");
        Ok(
            json!({"endpointId":shared.runtime.snapshot().endpoint_id,"sessionId":session_id,
            "connectionEpoch":epoch,"canRunCommands":can_run,"commands":commands,"skills":skills}),
        )
    }

    pub(crate) async fn run_catalog_command(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        let session_id = string(payload, "sessionId")?.to_owned();
        let epoch = catalog_epoch(payload)?;
        let name = string(payload, "name")?.to_owned();
        let arguments = payload.get("arguments").map_or(Ok(""), |v| {
            v.as_str()
                .ok_or_else(|| Error::Protocol("command arguments must be text".into()))
        })?;
        let attachments = match payload.get("attachments") {
            None => Vec::new(),
            Some(Value::Array(items)) => items.clone(),
            Some(_) => {
                return Err(Error::Protocol(
                    "command attachments must be an array".into(),
                ));
            }
        };
        shared
            .runtime
            .inspect(|s| validate_catalog_session(s, &session_id, epoch, true))?;
        let endpoint = shared.connected()?;
        require(
            endpoint.capabilities().supports("commands/list")
                && endpoint.capabilities().supports("commands/run"),
            "commands are unavailable for this session",
        )?;
        let raw = shared
            .runtime
            .call_at_epoch(
                epoch,
                "commands/list",
                json!({"sessionId":session_id}),
                rpc_id,
            )
            .await?;
        let commands = normalize_commands(&raw)?;
        let command = commands
            .iter()
            .find(|c| c["name"] == name)
            .ok_or_else(|| Error::Protocol("command is no longer in the current catalog".into()))?;
        let draft = ComposerDraft {
            text: arguments.into(),
            artifacts: std::iter::once(ComposerArtifact::Command {
                id: "command".into(),
                name: name.clone(),
                supports_attachments: command["supportsAttachments"] == true,
            })
            .chain(attachment_artifacts(&attachments)?)
            .collect(),
            ..Default::default()
        };
        let SubmissionPlan::Command {
            name,
            arguments,
            attachments,
        } = draft.compile(true)?
        else {
            unreachable!("one command artifact")
        };
        let runtime = shared.runtime.clone();
        let checked = runtime.clone();
        let checked_id = session_id.clone();
        let receipt = runtime.call_at_epoch_with_validation(epoch, "commands/run", json!({"sessionId":session_id,"name":name,"arguments":arguments,"attachments":attachments}), rpc_id,
            move || checked.inspect(|s| validate_catalog_session(s, &checked_id, epoch, true))).await?;
        normalize_command_receipt(receipt)
    }

    /// Compile selected public skills through the shared Composer contract. This does not submit.
    pub(crate) async fn compile_catalog_skills(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        let epoch = catalog_epoch(payload)?;
        let session_id = string(payload, "sessionId")?;
        shared
            .runtime
            .inspect(|s| validate_catalog_session(s, session_id, epoch, true))?;
        let ids = payload["skillIds"]
            .as_array()
            .ok_or_else(|| Error::Protocol("skillIds must be an array".into()))?;
        require(
            shared.connected()?.capabilities().supports("skills/list"),
            "skills are unavailable for this session",
        )?;
        let raw = shared
            .runtime
            .call_at_epoch(
                epoch,
                "skills/list",
                json!({"sessionId":session_id}),
                rpc_id,
            )
            .await?;
        let skills = normalize_skills(&raw)?;
        let mut artifacts = Vec::new();
        let mut seen = BTreeSet::new();
        for id in ids {
            let id = id
                .as_str()
                .ok_or_else(|| Error::Protocol("skill ID must be text".into()))?;
            require(seen.insert(id), "duplicate selected skill")?;
            let skill = skills
                .iter()
                .find(|s| s["id"] == id && s["selectable"] == true)
                .ok_or_else(|| {
                    Error::Protocol("selected skill is unavailable in the current catalog".into())
                })?;
            artifacts.push(ComposerArtifact::Skill {
                id: format!("skill:{id}"),
                name: string(skill, "name")?.into(),
                skill_id: id.into(),
            });
        }
        let attachments = match payload.get("attachments") {
            None => &[][..],
            Some(Value::Array(a)) => a.as_slice(),
            Some(_) => return Err(Error::Protocol("attachments must be an array".into())),
        };
        artifacts.extend(attachment_artifacts(attachments)?);
        let draft = ComposerDraft {
            text: string(payload, "text")?.into(),
            artifacts,
            ..Default::default()
        };
        let plan = draft.compile(false)?;
        shared
            .runtime
            .inspect(|s| validate_catalog_session(s, session_id, epoch, true))?;
        serde_json::to_value(plan).map_err(|e| Error::Protocol(e.to_string()))
    }
}

impl SharedAgent {
    pub(crate) async fn tool_catalog(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        let session_id = string(payload, "sessionId")?;
        let epoch = catalog_epoch(payload)?;
        let raw = read_resource_call(
            shared,
            payload,
            "tools/list",
            json!({"sessionId":session_id}),
            rpc_id,
        )
        .await?;
        require(raw["format"] == 1, "unsupported tool catalog format")?;
        let digest = tool_catalog_digest(&raw)?;
        let rows = raw["tools"]
            .as_array()
            .ok_or_else(|| Error::Protocol("tool catalog must contain tools".into()))?;
        let mut names = BTreeSet::new();
        let tools: Vec<Value> = rows
            .iter()
            .map(|row| {
                let tool = validated_tool(row)?;
                require(
                    names.insert(string(&tool, "name")?.to_owned()),
                    "duplicate tool catalog name",
                )?;
                Ok(tool)
            })
            .collect::<Result<_>>()?;
        Ok(
            json!({"endpointId":shared.runtime.snapshot().endpoint_id,"sessionId":session_id,"connectionEpoch":epoch,"catalogDigest":digest,"tools":tools}),
        )
    }
    pub(crate) async fn resolve_catalog_tool(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        let session_id = string(payload, "sessionId")?;
        let name = string(payload, "name")?;
        let expected_digest = string(payload, "catalogDigest")?;
        let raw = read_resource_call(
            shared,
            payload,
            "tools/resolve",
            json!({"sessionId":session_id,"name":name}),
            rpc_id,
        )
        .await?;
        require(raw["format"] == 1, "unsupported tool resolution format")?;
        let digest = tool_catalog_digest(&raw)?;
        require(
            digest == expected_digest,
            "tool catalog changed; reload the catalog",
        )?;
        let tool = validated_tool(&raw["tool"])?;
        require(
            tool["name"] == name,
            "resolved tool does not match the selected name",
        )?;
        Ok(
            json!({"endpointId":shared.runtime.snapshot().endpoint_id,"sessionId":session_id,"connectionEpoch":catalog_epoch(payload)?,"catalogDigest":digest,"tool":tool}),
        )
    }
    pub(crate) async fn skill_resource_page(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        let session_id = string(payload, "sessionId")?;
        let limit = match payload.get("limit") {
            None => 50,
            Some(v) => v
                .as_u64()
                .filter(|limit| (1..=100).contains(limit))
                .ok_or_else(|| Error::Protocol("resource limit must be 1...100".into()))?,
        };
        let mut request = json!({"sessionId":session_id,"source":"skill","limit":limit});
        if let Some(cursor) = payload.get("after") {
            require(
                cursor.as_str().is_some_and(|value| !value.is_empty()),
                "resource cursor must be nonempty text",
            )?;
            request["after"] = cursor.clone();
        }
        let raw = read_resource_call(shared, payload, "resources/list", request, rpc_id).await?;
        require(raw["format"] == 1, "unsupported resource catalog format")?;
        let items = raw["items"]
            .as_array()
            .ok_or_else(|| Error::Protocol("resource page must contain items".into()))?;
        for item in items {
            validate_skill_reference(&item["reference"], session_id)?;
            require(
                !string(item, "uri")?.is_empty(),
                "resource URI must be nonempty",
            )?;
            let _ = string(item, "name")?;
            let _ = optional_text(item, "mimeType")?;
            let _ = optional_text(item, "description")?;
        }
        let next = optional_text(&raw, "next")?;
        require(
            next.is_none_or(|cursor| !cursor.is_empty()),
            "resource cursor must be nonempty text",
        )?;
        Ok(
            json!({"endpointId":shared.runtime.snapshot().endpoint_id,"sessionId":session_id,"connectionEpoch":catalog_epoch(payload)?,"items":items,"next":next}),
        )
    }
    pub(crate) async fn read_skill_resource(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        let session_id = string(payload, "sessionId")?;
        validate_skill_reference(&payload["reference"], session_id)?;
        let uri = string(payload, "uri")?;
        require(!uri.is_empty(), "resource URI must be nonempty")?;
        let raw = read_resource_call(
            shared,
            payload,
            "resources/read",
            json!({"sessionId":session_id,"reference":payload["reference"],"uri":uri}),
            rpc_id,
        )
        .await?;
        require(raw["format"] == 1, "unsupported resource read format")?;
        let content = normalize_resource_content(&raw["content"])?;
        Ok(
            json!({"endpointId":shared.runtime.snapshot().endpoint_id,"sessionId":session_id,"connectionEpoch":catalog_epoch(payload)?,"reference":payload["reference"],"uri":uri,"content":content}),
        )
    }
}

async fn read_resource_call(
    shared: &SharedEndpoint,
    scope: &Value,
    method: &str,
    request: Value,
    rpc_id: &str,
) -> Result<Value> {
    let session_id = string(scope, "sessionId")?;
    let epoch = catalog_epoch(scope)?;
    require(
        shared.connected()?.capabilities().supports(method),
        "resource operation is unavailable",
    )?;
    shared
        .runtime
        .inspect(|s| validate_catalog_session(s, session_id, epoch, false))?;
    let value = shared
        .runtime
        .call_at_epoch(epoch, method, request, rpc_id)
        .await?;
    shared
        .runtime
        .inspect(|s| validate_catalog_session(s, session_id, epoch, false))?;
    Ok(value)
}
fn tool_catalog_digest(raw: &Value) -> Result<&str> {
    let digest = string(raw, "catalogDigest")?;
    require(
        digest.strip_prefix("sha256-").is_some_and(is_hex_digest),
        "invalid tool catalog digest",
    )?;
    Ok(digest)
}
fn is_hex_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn validated_tool(row: &Value) -> Result<Value> {
    let _ = resource_name(row)?;
    let _ = string(row, "description")?;
    let _ = string(row, "availability")?;
    let _ = string(row, "backend")?;
    require(
        row["arguments"].is_object() || row["arguments"].is_boolean(),
        "tool arguments must be a JSON schema",
    )?;
    require(
        row["source"].is_object() && row["source"]["kind"].is_string(),
        "tool source must identify its kind",
    )?;
    require(!row["effect"].is_null(), "tool effect is missing")?;
    Ok(row.clone())
}
fn validate_skill_reference(reference: &Value, session_id: &str) -> Result<()> {
    require(
        reference
            .as_object()
            .is_some_and(|fields| fields.len() == 4)
            && reference["kind"] == "skill"
            && reference["sessionId"] == session_id
            && reference["name"]
                .as_str()
                .is_some_and(|name| !name.is_empty())
            && reference["contentDigest"]
                .as_str()
                .is_some_and(is_hex_digest),
        "skill resource reference does not match this session and package digest",
    )
}
fn normalize_resource_content(raw: &Value) -> Result<Value> {
    let mime_type = optional_text(raw, "mimeType")?.unwrap_or("application/octet-stream");
    require(
        raw["text"].is_string() != raw["blobBase64"].is_string(),
        "resource must contain exactly one text or binary body",
    )?;
    if let Some(text) = raw["text"].as_str() {
        Ok(
            json!({"kind":"text","text":text,"mimeType":if raw.get("mimeType").is_none() {"text/plain"} else {mime_type}}),
        )
    } else {
        Ok(json!({"kind":"binary","blobBase64":string(raw,"blobBase64")?,"mimeType":mime_type}))
    }
}

fn normalize_command_receipt(raw: Value) -> Result<Value> {
    if raw["result"]["kind"] == "error" || raw["accepted"] == false {
        return Err(Error::Protocol(
            raw["result"]["text"]
                .as_str()
                .unwrap_or("command was not accepted")
                .into(),
        ));
    }
    let command_id = string(&raw, "commandId")?;
    let text = optional_text(&raw["result"], "text")?;
    Ok(json!({"commandId":command_id,"accepted":true,"text":text}))
}
fn catalog_epoch(payload: &Value) -> Result<u64> {
    payload["connectionEpoch"]
        .as_u64()
        .ok_or_else(|| Error::Protocol("connectionEpoch must be an unsigned integer".into()))
}
fn validate_catalog_session(
    state: &MemoryProjectionStore,
    id: &str,
    epoch: u64,
    write: bool,
) -> Result<()> {
    require(
        state.connection_epoch == epoch,
        "resource catalog connection epoch is stale",
    )?;
    require(
        matches!(state.connection, ConnectionState::Ready) && state.control_ready,
        "resource catalog requires a ready connection",
    )?;
    let session = state
        .sessions
        .get(id)
        .ok_or_else(|| Error::Protocol("resource catalog session is unavailable".into()))?;
    require(
        session.journal_ready,
        "resource catalog requires a ready session journal",
    )?;
    if write {
        let scope = RunScope {
            endpoint_id: state.endpoint_id.clone(),
            thread_id: id.into(),
            session_id: id.into(),
        };
        require(
            !state.archived_session_ids.iter().any(|s| s == id)
                && session
                    .summary
                    .as_ref()
                    .is_none_or(|s| s["status"] != "archived")
                && RunPresentation::read(state, &scope, true).can_submit,
            "resource submission is unavailable for this session",
        )?;
    }
    Ok(())
}
fn catalog_rows<'a>(raw: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    raw.as_array()
        .or_else(|| raw.get(key).and_then(Value::as_array))
        .ok_or_else(|| Error::Protocol(format!("{key} catalog must be an array")))
}
fn optional_text<'a>(row: &'a Value, key: &str) -> Result<Option<&'a str>> {
    match row.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(v)) => Ok(Some(v)),
        Some(_) => Err(Error::Protocol(format!("catalog {key} must be text"))),
    }
}
fn optional_bool(row: &Value, key: &str) -> Result<Option<bool>> {
    match row.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(Error::Protocol(format!("catalog {key} must be boolean"))),
    }
}
fn resource_name(row: &Value) -> Result<&str> {
    let name = string(row, "name")?;
    require(
        !name.is_empty() && !name.starts_with('/') && !name.contains(char::is_whitespace),
        "invalid catalog resource name",
    )?;
    Ok(name)
}
fn normalize_commands(raw: &Value) -> Result<Vec<Value>> {
    let mut names = BTreeSet::new();
    catalog_rows(raw,"commands")?.iter().map(|r| {
        let name = resource_name(r)?;
        require(names.insert(name), "duplicate command catalog name")?;
        let title = optional_text(r,"title")?.unwrap_or(name);
        let description = optional_text(r,"description")?.unwrap_or("");
        let hint = optional_text(r,"argumentHint")?.or(optional_text(&r["input"],"hint")?);
        let attachments = optional_bool(r, "supportsAttachments")?.or(optional_bool(&r["input"], "attachments")?).unwrap_or(false);
        Ok(json!({"name":name,"title":title,"description":description,"argumentHint":hint,"supportsAttachments":attachments}))
    }).collect()
}
fn normalize_skills(raw: &Value) -> Result<Vec<Value>> {
    let mut names = BTreeSet::new();
    catalog_rows(raw,"skills")?.iter().map(|r| {
        let name = resource_name(r)?;
        require(names.insert(name), "duplicate skill catalog name")?;
        let user = optional_bool(&r["invocation"], "userInvocable")?.unwrap_or(true);
        let model = optional_bool(&r["invocation"], "modelInvocable")?.or(optional_bool(r, "modelInvocable")?).unwrap_or(true);
        Ok(json!({"id":name,"name":name,"title":optional_text(r,"title")?.unwrap_or(name),"description":optional_text(r,"description")?.unwrap_or(""),
            "selectable":user && model,"unavailableReason":if user && model {None} else {Some("This skill requires a Harness-specific invocation path")}}))
    }).collect()
}
fn attachment_artifacts(items: &[Value]) -> Result<Vec<ComposerArtifact>> {
    items
        .iter()
        .enumerate()
        .map(|(i, v)| {
            require(
                matches!(v["type"].as_str(), Some("file" | "image")),
                "invalid resource attachment",
            )?;
            Ok(ComposerArtifact::Attachment {
                id: format!("attachment:{i}"),
                name: v["name"].as_str().unwrap_or("Attachment").into(),
                content: v.clone(),
                facts: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalizes_kernel_pi_and_dsh_catalog_shapes_without_inventing_attachment_support() {
        let wrapped = json!({"commands":[{"name":"review","description":"Review","argumentHint":"path","supportsAttachments":true}]});
        let dsh = json!([{"name":"review","description":"Review","input":{"hint":"path","attachments":true}}]);
        assert_eq!(
            normalize_commands(&wrapped).unwrap(),
            normalize_commands(&dsh).unwrap()
        );
        assert_eq!(
            normalize_commands(
                &json!({"commands":[{"name":"clear","description":"Clear","source":"extension"}]})
            )
            .unwrap()[0]["supportsAttachments"],
            false
        );
        assert!(normalize_commands(&json!({"commands":{}})).is_err());
        assert!(normalize_commands(&json!([{"name":"x"},{"name":"x"}])).is_err());
    }
    #[test]
    fn command_domain_errors_are_not_successful_ui_receipts() {
        assert!(
            normalize_command_receipt(
                json!({"commandId":"x","result":{"kind":"error","text":"invalid path"}})
            )
            .is_err()
        );
        assert!(normalize_command_receipt(json!({"commandId":"x","accepted":false})).is_err());
        assert_eq!(
            normalize_command_receipt(
                json!({"commandId":"x","result":{"kind":"success","text":"Cleared"}})
            )
            .unwrap(),
            json!({"commandId":"x","accepted":true,"text":"Cleared"})
        );
    }
    #[test]
    fn invocation_policy_survives_normalization() {
        let skills=normalize_skills(&json!([{"name":"both","description":"yes","invocation":{"userInvocable":true,"modelInvocable":true}},{"name":"model-only","invocation":{"userInvocable":false,"modelInvocable":true}},{"name":"user-only","invocation":{"userInvocable":true,"modelInvocable":false}}])).unwrap();
        assert_eq!(skills[0]["selectable"], true);
        assert_eq!(skills[1]["selectable"], false);
        assert_eq!(skills[2]["selectable"], false);
        assert_eq!(
            normalize_skills(&json!({"skills":[{"name":"kernel","modelInvocable":true}]})).unwrap()
                [0]["id"],
            "kernel"
        );
    }
    #[test]
    fn rejects_stale_missing_archived_and_historical_mutation_targets() {
        let mut s = MemoryProjectionStore::new("endpoint");
        s.connection = ConnectionState::Ready;
        s.connection_epoch = 3;
        s.control_ready = true;
        s.sessions
            .entry("session".into())
            .or_default()
            .journal_ready = true;
        assert!(validate_catalog_session(&s, "session", 3, false).is_ok());
        assert!(validate_catalog_session(&s, "session", 3, true).is_ok());
        assert!(validate_catalog_session(&s, "session", 2, false).is_err());
        assert!(validate_catalog_session(&s, "missing", 3, false).is_err());
        s.archived_session_ids.push("session".into());
        assert!(validate_catalog_session(&s, "session", 3, true).is_err());
        s.archived_session_ids.clear();
        s.sessions.get_mut("session").unwrap().projections.insert("historyAccess".into(),tekes_agent_session::VersionedValue {sequence:1,value:json!({"mode":"read-only","reason":"legacy-tool-catalog-unavailable","sourceDigest":format!("sha256-{}","a".repeat(64))})});
        assert!(validate_catalog_session(&s, "session", 3, true).is_err());
    }

    #[derive(Clone)]
    struct CatalogConnector {
        kind: &'static str,
        command_error: bool,
        calls: Arc<std::sync::Mutex<Vec<(String, Value)>>>,
    }
    struct CatalogEndpoint {
        connector: CatalogConnector,
        host: HostDescription,
    }
    #[async_trait]
    impl EndpointConnector for CatalogConnector {
        async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
            Ok(Arc::new(CatalogEndpoint {
                connector: self.clone(),
                host: serde_json::from_value(json!({
                    "protocolVersion":4,"product":{"name":"catalog-fixture","version":"1"},
                    "capabilities":["session-journal","session-inventory-sync","session-control-sync","actionable-sync"],
                    "cwd":"/work","home":"/home","attachedSessions":0,"canOpenPath":false
                }))?,
            }))
        }
    }
    #[async_trait]
    impl SessionEndpoint for CatalogEndpoint {
        fn identity(&self) -> &str {
            "catalog-endpoint"
        }
        fn host(&self) -> &HostDescription {
            &self.host
        }
        fn capabilities(&self) -> EndpointCapabilities {
            EndpointCapabilities::methods(&[
                "commands/list",
                "commands/run",
                "skills/list",
                "session.prompt",
                "tools/list",
                "tools/resolve",
                "resources/list",
                "resources/read",
            ])
        }
        async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
            use futures_util::stream;
            let frame = match target {
                StreamTarget::SessionInventory => {
                    json!({"type":"baseline","generation":"g","snapshot":{"items":[{
                        "sessionId":"session","running":false,"blank":false,"updatedAt":1,"title":"Catalog test"
                    }]}})
                }
                StreamTarget::SessionControl => {
                    json!({"type":"baseline","generation":"g","snapshot":{"queues":[],"jobs":[],"projections":[]}})
                }
                StreamTarget::Actionables => json!({"type":"reset","generation":"g"}),
                StreamTarget::SessionJournal { address, .. } => {
                    json!({"type":"snapshot","snapshot":{
                        "sessionId":address.session_id,"generation":"g","throughSequence":-1,"windowLimit":50,"entries":[],"hasMoreBefore":false
                    }})
                }
                _ => return Err(unsupported("catalog fixture stream")),
            };
            Ok(Box::pin(
                stream::once(async { Ok(frame) }).chain(stream::pending()),
            ))
        }
        async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
            Err(unsupported("catalog fixture history"))
        }
        async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
            Err(unsupported("catalog fixture response"))
        }
        async fn call(&self, method: &str, payload: Value, _: &str) -> Result<Value> {
            self.connector
                .calls
                .lock()
                .unwrap()
                .push((method.into(), payload.clone()));
            match method {
                "commands/list" => Ok(match self.connector.kind {
                    "native" => {
                        json!({"commands":[{"name":"review","description":"Review","argumentHint":"path","supportsAttachments":true}]})
                    }
                    "pi" => {
                        json!({"commands":[{"name":"review","description":"Review","source":"extension"}]})
                    }
                    "dsh" => {
                        json!([{"name":"review","description":"Review","input":{"hint":"path","attachments":true}}])
                    }
                    _ => unreachable!(),
                }),
                "skills/list" => Ok(if self.connector.kind == "dsh" {
                    json!([{"name":"audit","description":"Audit","invocation":{"modelInvocable":true,"userInvocable":true},"provider":"fixture","source":"runtime"}])
                } else {
                    json!({"skills":[{"name":"audit","description":"Audit","modelInvocable":true}]})
                }),
                "commands/run" if self.connector.command_error => Ok(
                    json!({"commandId":"command-1","result":{"kind":"error","text":"invalid path"}}),
                ),
                "commands/run" => Ok(match self.connector.kind {
                    "native" => {
                        json!({"commandId":"command-1","format":1,"accepted":true,"command":"review","content_digest":"digest","seq":1,"deduplicated":false})
                    }
                    "pi" => json!({"commandId":"command-1","native":{"disposition":"handled"}}),
                    "dsh" => {
                        json!({"commandId":"command-1","result":{"kind":"success","text":"Reviewed"}})
                    }
                    _ => unreachable!(),
                }),
                "tools/list" => Ok(
                    json!({"format":1,"catalogDigest":format!("sha256-{}","a".repeat(64)),"tools":[fixture_tool("read")]}),
                ),
                "tools/resolve" => {
                    let name = string(&payload, "name")?;
                    Ok(
                        json!({"format":1,"catalogDigest":format!("sha256-{}",if name=="changed" {"c".repeat(64)} else {"a".repeat(64)}),"tool":fixture_tool(name)}),
                    )
                }
                "resources/list" if payload["after"] == "stale" => {
                    Err(Error::Protocol("resource-stale: generation changed".into()))
                }
                "resources/list" => Ok(if payload["after"] == "opaque-next" {
                    json!({"format":1,"items":[{"reference":fixture_reference(),"uri":"binary.bin","name":"binary.bin","mimeType":"application/octet-stream"}]})
                } else {
                    json!({"format":1,"items":[{"reference":fixture_reference(),"uri":"SKILL.md","name":"SKILL.md","mimeType":"text/markdown","description":"Audit guide"}],"next":"opaque-next"})
                }),
                "resources/read" if payload["reference"]["contentDigest"] != "b".repeat(64) => {
                    Err(Error::Protocol("resource-stale: package changed".into()))
                }
                "resources/read" => Ok(if payload["uri"] == "binary.bin" {
                    json!({"format":1,"content":{"blobBase64":"AA==","mimeType":"application/octet-stream"}})
                } else {
                    json!({"format":1,"content":{"text":"# Audit guide","mimeType":"text/markdown"}})
                }),
                _ => Err(unsupported("catalog fixture RPC")),
            }
        }
        async fn close(&self) {}
    }

    fn fixture_tool(name: &str) -> Value {
        json!({"name":name,"description":"Read a workspace file","arguments":{"type":"object","properties":{"path":{"type":"string"}}},"availability":"always","backend":"worker","effect":"read-only","source":{"kind":"builtin"}})
    }
    fn fixture_reference() -> Value {
        json!({"kind":"skill","sessionId":"session","name":"audit","contentDigest":"b".repeat(64)})
    }

    #[tokio::test]
    async fn public_catalog_dispatch_compiles_skills_and_executes_each_command_only_once() {
        for kind in ["native", "pi", "dsh"] {
            for command_error in [false, true] {
                let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
                let connector = CatalogConnector {
                    kind,
                    command_error,
                    calls: calls.clone(),
                };
                let mut agent = SharedAgent::default();
                agent
                    .register(
                        "catalog-endpoint".into(),
                        kind.into(),
                        "Catalog".into(),
                        Arc::new(connector),
                    )
                    .unwrap();
                let endpoint = agent.endpoint("catalog-endpoint").unwrap();
                endpoint
                    .runtime
                    .wait_ready(std::time::Duration::from_secs(2))
                    .await
                    .unwrap();
                endpoint.runtime.open_session("session").await.unwrap();
                let epoch = endpoint.runtime.snapshot().connection_epoch;
                let request = |method: &str, payload: Value| json!({"op":"call","endpoint":"catalog-endpoint","method":method,"rpcId":"public-catalog","payload":payload});
                let catalog = agent
                    .request(&request(
                        "agent.catalog",
                        json!({"sessionId":"session","connectionEpoch":epoch}),
                    ))
                    .await
                    .unwrap();
                assert_eq!(catalog["endpointId"], "catalog-endpoint");
                assert_eq!(catalog["sessionId"], "session");
                assert_eq!(catalog["canRunCommands"], true);
                assert_eq!(catalog["commands"][0]["name"], "review");
                assert_eq!(catalog["commands"][0]["supportsAttachments"], kind != "pi");
                assert_eq!(catalog["skills"][0]["id"], "audit");

                let image = json!({"type":"image","data":"aGVsbG8=","mimeType":"image/png"});
                let file = json!({"type":"file","receiptId":"owned-file-receipt"});
                let command_attachments = if kind == "pi" {
                    vec![]
                } else {
                    vec![file.clone()]
                };
                let run = agent.request(&request("agent.command.run", json!({"sessionId":"session","connectionEpoch":epoch,"name":"review","arguments":" current/path ","attachments":command_attachments}))).await;
                if command_error {
                    assert!(
                        matches!(run, Err(Error::Protocol(ref message)) if message == "invalid path")
                    );
                } else {
                    let receipt = run.unwrap();
                    assert_eq!(receipt["accepted"], true);
                    assert_eq!(receipt["commandId"], "command-1");
                    assert_eq!(
                        receipt["text"],
                        if kind == "dsh" {
                            json!("Reviewed")
                        } else {
                            Value::Null
                        }
                    );
                }
                let compiled = agent.request(&request("agent.skills.compile", json!({"sessionId":"session","connectionEpoch":epoch,"text":"inspect changes","skillIds":["audit"],"attachments":[image.clone(),file.clone()]}))).await.unwrap();
                assert_eq!(compiled["kind"], "prompt");
                assert_eq!(
                    compiled["content"][0]["text"],
                    "<requested_skills>\n- Use the skill audit (id: audit)\n</requested_skills>\n\ninspect changes"
                );
                assert_eq!(compiled["content"][1], image);
                assert_eq!(compiled["content"][2], file);
                assert_eq!(compiled["displayText"], "inspect changes");

                let captured = calls.lock().unwrap().clone();
                let writes: Vec<_> = captured
                    .iter()
                    .filter(|(method, _)| method == "commands/run")
                    .collect();
                assert_eq!(
                    writes.len(),
                    1,
                    "failed command dispatch is never automatically retried"
                );
                assert_eq!(
                    writes[0].1,
                    json!({"sessionId":"session","name":"review","arguments":"current/path","attachments":command_attachments})
                );
                assert!(
                    captured
                        .iter()
                        .all(|(method, payload)| method != "session.prompt"
                            && payload["sessionId"] == "session")
                );
                assert!(agent.request(&request("agent.command.run", json!({"sessionId":"session","connectionEpoch":epoch + 1,"name":"review"}))).await.is_err());
                assert_eq!(
                    calls.lock().unwrap().len(),
                    captured.len(),
                    "stale epoch is rejected before any remote catalog read or command"
                );
                endpoint.runtime.stop().await.unwrap();
            }
        }
    }
    #[tokio::test]
    async fn public_read_only_tool_and_skill_resources_keep_scope_digest_and_stale_errors() {
        let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut agent = SharedAgent::default();
        agent
            .register(
                "catalog-endpoint".into(),
                "native".into(),
                "Catalog".into(),
                Arc::new(CatalogConnector {
                    kind: "native",
                    command_error: false,
                    calls: calls.clone(),
                }),
            )
            .unwrap();
        let endpoint = agent.endpoint("catalog-endpoint").unwrap();
        endpoint
            .runtime
            .wait_ready(std::time::Duration::from_secs(2))
            .await
            .unwrap();
        endpoint.runtime.open_session("session").await.unwrap();
        let epoch = endpoint.runtime.snapshot().connection_epoch;
        let request = |method: &str, extra: Value| {
            let mut payload = json!({"sessionId":"session","connectionEpoch":epoch});
            payload
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            json!({"op":"call","endpoint":"catalog-endpoint","method":method,"rpcId":"read-resource","payload":payload})
        };
        let tools = agent
            .request(&request("agent.tools.list", json!({})))
            .await
            .unwrap();
        assert_eq!(tools["tools"][0], fixture_tool("read"));
        let resolved = agent
            .request(&request(
                "agent.tools.resolve",
                json!({"name":"read","catalogDigest":tools["catalogDigest"]}),
            ))
            .await
            .unwrap();
        assert_eq!(resolved["tool"], fixture_tool("read"));
        assert!(
            agent
                .request(&request(
                    "agent.tools.resolve",
                    json!({"name":"changed","catalogDigest":tools["catalogDigest"]})
                ))
                .await
                .is_err()
        );
        let first = agent
            .request(&request("agent.resources.list", json!({"limit":1})))
            .await
            .unwrap();
        assert_eq!(first["items"][0]["reference"], fixture_reference());
        assert_eq!(first["next"], "opaque-next");
        let second = agent
            .request(&request(
                "agent.resources.list",
                json!({"after":first["next"],"limit":1}),
            ))
            .await
            .unwrap();
        assert_eq!(second["items"][0]["uri"], "binary.bin");
        assert!(second["next"].is_null());
        let text = agent
            .request(&request(
                "agent.resources.read",
                json!({"reference":fixture_reference(),"uri":"SKILL.md"}),
            ))
            .await
            .unwrap();
        assert_eq!(
            text["content"],
            json!({"kind":"text","text":"# Audit guide","mimeType":"text/markdown"})
        );
        let binary = agent
            .request(&request(
                "agent.resources.read",
                json!({"reference":fixture_reference(),"uri":"binary.bin"}),
            ))
            .await
            .unwrap();
        assert_eq!(
            binary["content"],
            json!({"kind":"binary","blobBase64":"AA==","mimeType":"application/octet-stream"})
        );
        let count = calls.lock().unwrap().len();
        let mut foreign = fixture_reference();
        foreign["sessionId"] = json!("another-session");
        assert!(
            agent
                .request(&request(
                    "agent.resources.read",
                    json!({"reference":foreign,"uri":"SKILL.md"})
                ))
                .await
                .is_err()
        );
        assert_eq!(
            calls.lock().unwrap().len(),
            count,
            "cross-session reference is rejected before forwarding"
        );
        let mut stale = fixture_reference();
        stale["contentDigest"] = json!("d".repeat(64));
        assert!(
            matches!(agent.request(&request("agent.resources.read",json!({"reference":stale,"uri":"SKILL.md"}))).await,Err(Error::Protocol(ref message)) if message.contains("resource-stale"))
        );
        assert_eq!(
            calls.lock().unwrap().len(),
            count + 1,
            "stale package reads are not retried"
        );
        assert!(
            matches!(agent.request(&request("agent.resources.list",json!({"after":"stale"}))).await,Err(Error::Protocol(ref message)) if message.contains("resource-stale"))
        );
        assert_eq!(
            calls.lock().unwrap().len(),
            count + 2,
            "stale cursor reads are not retried"
        );
        let captured = calls.lock().unwrap().clone();
        assert!(captured.iter().all(|(method, payload)| !matches!(
            method.as_str(),
            "commands/run" | "session.prompt"
        ) && payload["sessionId"] == "session"));
        assert!(
            captured
                .iter()
                .filter(|(method, _)| method == "resources/list")
                .all(|(_, payload)| payload["source"] == "skill")
        );
        for value in [&tools, &resolved, &first, &second, &text, &binary] {
            assert_eq!(value["endpointId"], "catalog-endpoint");
            assert_eq!(value["sessionId"], "session");
            assert_eq!(value["connectionEpoch"], epoch);
        }
        endpoint.runtime.stop().await.unwrap();
    }
}
