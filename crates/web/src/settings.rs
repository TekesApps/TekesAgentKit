//! Browser-safe settings forms. Harnesses retain storage, validation and CAS authority.
use crate::*;

impl SharedAgent {
    pub(crate) async fn settings_read(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
    ) -> Result<Value> {
        let epoch = settings_epoch(payload)?;
        validate_settings_scope(shared, epoch)?;
        let endpoint = shared.connected()?;
        require(
            endpoint.capabilities().supports("settings.describe"),
            "settings are unavailable",
        )?;
        let request = if shared.kind == "pi" {
            json!({"sessionId":string(payload, "sessionId")?})
        } else {
            json!({})
        };
        let raw = shared
            .runtime
            .call_at_epoch(
                epoch,
                "settings.describe",
                request,
                &format!("settings-read-{}", uuid::Uuid::new_v4()),
            )
            .await?;
        let namespaces = normalize_settings(
            &shared.kind,
            &raw,
            endpoint.capabilities().supports("settings.mutate"),
        )?;
        validate_settings_scope(shared, epoch)?;
        Ok(
            json!({"endpointId":endpoint.identity(),"connectionEpoch":epoch,"namespaces":namespaces}),
        )
    }

    pub(crate) async fn settings_mutate(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        let epoch = settings_epoch(payload)?;
        validate_settings_scope(shared, epoch)?;
        let endpoint = shared.connected()?;
        require(
            endpoint.capabilities().supports("settings.mutate"),
            "settings are read-only",
        )?;
        // Read the schema and writability again, retaining the editor's original revision.
        let raw = shared
            .runtime
            .call_at_epoch(
                epoch,
                "settings.describe",
                json!({}),
                &format!("settings-read-{}", uuid::Uuid::new_v4()),
            )
            .await?;
        let namespaces = normalize_settings(&shared.kind, &raw, true)?;
        let request = settings_request(&namespaces, payload)?;
        let runtime = shared.runtime.clone();
        let validate = shared.runtime.clone();
        runtime
            .call_at_epoch_with_validation(epoch, "settings.mutate", request, rpc_id, move || {
                let state = validate.snapshot();
                require(
                    state.connection_epoch == epoch
                        && matches!(state.connection, ConnectionState::Ready),
                    "settings connection epoch is stale",
                )
            })
            .await?;
        // The upstream reply may contain a new descriptor. Do not forward its raw value/schema.
        Ok(json!({"endpointId":endpoint.identity(),"connectionEpoch":epoch,"acknowledged":true}))
    }
}

fn settings_epoch(payload: &Value) -> Result<u64> {
    payload["connectionEpoch"]
        .as_u64()
        .ok_or_else(|| Error::Protocol("connectionEpoch must be an unsigned integer".into()))
}
fn validate_settings_scope(shared: &SharedEndpoint, epoch: u64) -> Result<()> {
    shared.runtime.inspect(|state| {
        require(
            state.connection_epoch == epoch && matches!(state.connection, ConnectionState::Ready),
            "settings require the current ready connection",
        )
    })
}
fn at_path<'a>(value: &'a Value, path: &[String]) -> Option<&'a Value> {
    path.iter().try_fold(value, |value, key| value.get(key))
}
fn secret_path(path: &[String], secrets: &[Vec<String>]) -> bool {
    secrets.iter().any(|secret| path.starts_with(secret))
}
fn normalize_settings(kind: &str, raw: &Value, can_mutate: bool) -> Result<Vec<Value>> {
    if kind == "pi" {
        require(raw.is_object(), "Pi settings must be an object")?;
        // The public extension reads effective settings. It has no CAS write API;
        // do not copy its unrestricted object (which can grow new credential fields).
        let safe = [
            (vec!["theme"], "string"),
            (vec!["defaultProvider"], "string"),
            (vec!["defaultModel"], "string"),
            (vec!["defaultThinkingLevel"], "string"),
            (vec!["compaction", "enabled"], "boolean"),
            (vec!["compaction", "reserveTokens"], "integer"),
            (vec!["compaction", "keepRecentTokens"], "integer"),
            (vec!["retry", "enabled"], "boolean"),
            (vec!["images", "autoResize"], "boolean"),
            (vec!["images", "blockImages"], "boolean"),
            (vec!["quietStartup"], "boolean"),
        ];
        let fields: Vec<_> = safe.into_iter().filter_map(|(path, kind)| {
            let path: Vec<String> = path.into_iter().map(str::to_owned).collect();
            let value = at_path(raw, &path)?;
            if !field_value_valid(kind, value, &json!({})) { return None; }
            Some(json!({"path":path,"label":path.join(" / "),"description":"","kind":kind,"value":value,"overridden":false,"writable":false}))
        }).collect();
        return Ok(vec![
            json!({"id":"pi","title":"Pi","applies":"effective","revision":null,"readOnlyReason":"The current Pi integration exposes effective settings without a revisioned write contract.","fields":fields}),
        ]);
    }
    require(
        matches!(kind, "native" | "tekes" | "kernel" | "dsh"),
        "unknown settings Harness",
    )?;
    let writable = raw["writable"]
        .as_bool()
        .ok_or_else(|| Error::Protocol("settings writable must be boolean".into()))?
        && can_mutate;
    let namespaces = raw["namespaces"]
        .as_array()
        .ok_or_else(|| Error::Protocol("settings namespaces must be an array".into()))?;
    let mut ids = BTreeSet::new();
    namespaces.iter().map(|ns| {
        let id = string(ns, "ns")?;
        require(!id.is_empty() && ids.insert(id), "duplicate or empty settings namespace")?;
        let revision = ns["revision"].as_u64().ok_or_else(|| Error::Protocol("settings revision must be an unsigned integer".into()))?;
        let applies = string(ns, "applies")?;
        let secret_rows = ns["secrets"].as_array().ok_or_else(|| Error::Protocol("settings secrets must be an array".into()))?;
        let secrets: Vec<Vec<String>> = secret_rows.iter().map(|secret| serde_json::from_value(secret["path"].clone()).map_err(Error::from)).collect::<Result<_>>()?;
        let mut fields = Vec::new();
        collect_fields(&ns["schema"], &ns["value"], ns.get("user"), &[], &secrets, writable, &mut fields)?;
        Ok(json!({"id":id,"title":id,"applies":applies,"revision":revision,"readOnlyReason":if writable {Value::Null} else {json!("The Harness currently exposes these settings as read-only.")},"fields":fields}))
    }).collect()
}
fn collect_fields(
    schema: &Value,
    value: &Value,
    user: Option<&Value>,
    path: &[String],
    secrets: &[Vec<String>],
    writable: bool,
    fields: &mut Vec<Value>,
) -> Result<()> {
    if schema["meta"]["role"] == "secret"
        || schema["format"] == "password"
        || secret_path(path, secrets)
    {
        return Ok(());
    }
    let schema_type = schema["type"].as_str().unwrap_or("");
    if schema_type == "object" {
        if let Some(properties) = schema
            .get("properties")
            .or_else(|| schema.get("dict"))
            .and_then(Value::as_object)
        {
            for (key, child) in properties {
                let mut next = path.to_vec();
                next.push(key.clone());
                collect_fields(
                    child,
                    value.get(key).unwrap_or(&Value::Null),
                    user.and_then(|v| v.get(key)),
                    &next,
                    secrets,
                    writable,
                    fields,
                )?;
            }
        }
        return Ok(());
    }
    if path.is_empty() {
        return Ok(());
    }
    let options = schema
        .get("enum")
        .and_then(Value::as_array)
        .cloned()
        .or_else(|| {
            if schema_type != "union" {
                return None;
            }
            schema["list"]
                .as_array()?
                .iter()
                .map(|item| {
                    item.get("value")
                        .filter(|_| item["type"] == "const")
                        .cloned()
                })
                .collect::<Option<Vec<_>>>()
        });
    let kind = if options
        .as_ref()
        .is_some_and(|options| !options.is_empty() && options.iter().all(Value::is_string))
    {
        "enum"
    } else {
        match schema_type {
            "string" => "string",
            "boolean" => "boolean",
            "integer" | "natural" => "integer",
            "number" => "number",
            "array"
                if schema
                    .get("items")
                    .or_else(|| schema.get("inner"))
                    .is_some_and(|s| s["type"] == "string") =>
            {
                "stringList"
            }
            _ => {
                fields.push(json!({"path":path,"label":path.join(" / "),"description":"This field requires a Harness-specific editor.","kind":"unavailable","value":null,"overridden":user.is_some(),"writable":false}));
                return Ok(());
            }
        }
    };
    let mut field = json!({"path":path,"label":schema["title"].as_str().or_else(||schema["meta"]["description"].as_str()).unwrap_or_else(||path.last().unwrap()),"description":schema["description"].as_str().unwrap_or(""),"kind":kind,"value":value,"overridden":user.is_some(),"writable":writable && schema["readOnly"] != true});
    if let Some(options) = options {
        field["options"] = json!(options);
    }
    if let Some(min) = schema.get("minimum").or_else(|| schema["meta"].get("min")) {
        field["minimum"] = min.clone();
    }
    if let Some(max) = schema.get("maximum").or_else(|| schema["meta"].get("max")) {
        field["maximum"] = max.clone();
    }
    // Unset fields are valid form rows. Unexpected values are not echoed to the browser.
    require(
        value.is_null() || field_value_valid(kind, value, &field),
        "settings value does not match its public schema",
    )?;
    fields.push(field);
    Ok(())
}
fn field_value_valid(kind: &str, value: &Value, field: &Value) -> bool {
    let valid = match kind {
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        "stringList" => value
            .as_array()
            .is_some_and(|v| v.iter().all(Value::is_string)),
        "enum" => field["options"]
            .as_array()
            .is_some_and(|values| values.contains(value)),
        _ => false,
    };
    valid
        && value.as_f64().is_none_or(|n| {
            field["minimum"].as_f64().is_none_or(|min| n >= min)
                && field["maximum"].as_f64().is_none_or(|max| n <= max)
        })
}
fn settings_request(namespaces: &[Value], payload: &Value) -> Result<Value> {
    let ns = string(payload, "namespace")?;
    let namespace = namespaces
        .iter()
        .find(|n| n["id"] == ns)
        .ok_or_else(|| Error::Protocol("settings namespace is unavailable".into()))?;
    let revision = payload["expectedRevision"]
        .as_u64()
        .ok_or_else(|| Error::Protocol("settings expectedRevision is required".into()))?;
    require(
        namespace["revision"] == revision,
        "settings revision changed; refresh before saving",
    )?;
    let changes = payload["changes"]
        .as_array()
        .ok_or_else(|| Error::Protocol("settings changes must be an array".into()))?;
    require(!changes.is_empty(), "settings changes are empty")?;
    let mut paths = BTreeSet::new();
    let mut operations = Vec::new();
    for change in changes {
        let path: Vec<String> = serde_json::from_value(change["path"].clone())?;
        require(paths.insert(path.clone()), "duplicate settings field")?;
        let field = namespace["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["path"] == json!(path))
            .ok_or_else(|| Error::Protocol("settings field is unavailable".into()))?;
        require(field["writable"] == true, "settings field is read-only")?;
        if change["unset"] == true {
            operations.push(json!({"op":"unset","path":path}));
        } else {
            let value = change
                .get("value")
                .ok_or_else(|| Error::Protocol("settings value is missing".into()))?;
            require(
                field_value_valid(string(field, "kind")?, value, field),
                "settings value is invalid",
            )?;
            operations.push(json!({"op":"set","path":path,"value":value}));
        }
    }
    Ok(json!({"ns":ns,"expectedRevision":revision,"operations":operations}))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn description() -> Value {
        json!({"writable":true,"hasDocument":true,"namespaces":[{"ns":"policy","revision":4,"applies":"next-launch","secrets":[],"schema":{"type":"object","properties":{"network":{"type":"boolean"},"allowed_tools":{"type":"array","items":{"type":"string"}},"max_wall_seconds":{"type":"integer","minimum":1}}},"value":{"network":false,"allowed_tools":["read"],"max_wall_seconds":10},"user":{"network":false}}]})
    }
    #[test]
    fn kernel_schema_forms_keep_cas_and_next_launch_without_raw_values() {
        let n = normalize_settings("tekes", &description(), true).unwrap();
        assert_eq!(n[0]["applies"], "next-launch");
        let request=settings_request(&n,&json!({"namespace":"policy","expectedRevision":4,"changes":[{"path":["network"],"value":true},{"path":["allowed_tools"],"value":["read","write"]}]})).unwrap();
        assert_eq!(
            request,
            json!({"ns":"policy","expectedRevision":4,"operations":[{"op":"set","path":["network"],"value":true},{"op":"set","path":["allowed_tools"],"value":["read","write"]}]})
        );
        assert!(settings_request(&n,&json!({"namespace":"policy","expectedRevision":3,"changes":[{"path":["network"],"value":true}]})).is_err());
        assert!(settings_request(&n,&json!({"namespace":"policy","expectedRevision":4,"changes":[{"path":["max_wall_seconds"],"value":0}]})).is_err());
    }
    #[test]
    fn secrets_and_unknown_fields_never_ride_the_form() {
        let raw = json!({"writable":true,"namespaces":[{"ns":"provider","revision":2,"applies":"live","secrets":[{"path":["apiKey"],"set":true}],"schema":{"type":"object","dict":{"apiKey":{"type":"string","meta":{"role":"secret","default":"SCHEMA_SECRET"}},"enabled":{"type":"boolean"}}},"value":{"apiKey":"VALUE_SECRET","unknown":"UNKNOWN_SECRET","enabled":true},"user":{"apiKey":"USER_SECRET"},"extra":"EXTRA_SECRET"}]});
        let n = normalize_settings("dsh", &raw, true).unwrap();
        assert_eq!(n[0]["fields"].as_array().unwrap().len(), 1);
        assert!(!serde_json::to_string(&n).unwrap().contains("SECRET"));
        assert!(settings_request(&n,&json!({"namespace":"provider","expectedRevision":2,"changes":[{"path":["apiKey"],"value":"changed"}]})).is_err());
    }
    #[test]
    fn pi_effective_settings_are_allowlisted_and_read_only() {
        let n=normalize_settings("pi",&json!({"apiKey":"SECRET","compaction":{"enabled":true,"reserveTokens":4,"apiKey":"SECRET"},"defaultModel":"flash"}),true).unwrap();
        assert!(
            n[0]["fields"]
                .as_array()
                .unwrap()
                .iter()
                .all(|f| f["writable"] == false)
        );
        assert!(!serde_json::to_string(&n).unwrap().contains("SECRET"));
        assert!(n[0]["revision"].is_null());
    }
}
