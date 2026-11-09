//! Read-only context samples. Context occupancy and cumulative billing have separate authority.
use crate::*;
use tekes_agent_session::VersionedValue;

impl SharedAgent {
    pub(crate) async fn context_details_read(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
    ) -> Result<Value> {
        let id = string(payload, "sessionId")?;
        let epoch = payload["connectionEpoch"]
            .as_u64()
            .ok_or_else(|| Error::Protocol("connectionEpoch must be an unsigned integer".into()))?;
        let endpoint = shared.connected()?;
        let caps = endpoint.capabilities();
        let projections = shared.runtime.inspect(|state| {
            validate_scope(state, id, epoch)?;
            Ok::<_, Error>(state.sessions[id].projections.clone())
        })?;
        let mut normalized = normalize_projections(&projections, &shared.kind)?;
        let mut billing = json!({"supported":false,"summary":null,"cacheAttribution":null});
        if caps.supports("pi.getSessionStats") {
            let stats = shared
                .runtime
                .call_at_epoch(
                    epoch,
                    "pi.getSessionStats",
                    json!({"sessionId":id}),
                    &format!("context-stats-{}", uuid::Uuid::new_v4()),
                )
                .await?;
            normalized["usage"] = pi_usage(&stats)?;
            normalized["details"] = Value::Null;
            normalized["usageSequence"] = Value::Null;
            normalized["detailsSequence"] = Value::Null;
            if let Some(tokens) = stats.get("tokens") {
                require(tokens.is_object(), "Pi cumulative tokens must be an object")?;
                let mut summary = serde_json::Map::new();
                for (source, target) in [
                    ("input", "inputTokens"),
                    ("output", "outputTokens"),
                    ("cacheRead", "cacheReadTokens"),
                    ("cacheWrite", "cacheWriteTokens"),
                ] {
                    if let Some(n) = count(tokens.get(source), false)? {
                        summary.insert(target.into(), json!(n));
                    }
                }
                billing = json!({"supported":true,"summary":summary,"cacheAttribution":null});
            }
        } else if caps.supports("usage.summary") {
            let raw = shared
                .runtime
                .call_at_epoch(
                    epoch,
                    "usage.summary",
                    json!({"sessionId":id}),
                    &format!("context-summary-{}", uuid::Uuid::new_v4()),
                )
                .await?;
            billing["supported"] = json!(true);
            billing["summary"] = normalize_billing(&raw, id)?;
            if caps.supports("usage.cacheAttribution") {
                let raw = shared
                    .runtime
                    .call_at_epoch(
                        epoch,
                        "usage.cacheAttribution",
                        json!({"sessionId":id}),
                        &format!("context-cache-{}", uuid::Uuid::new_v4()),
                    )
                    .await?;
                billing["cacheAttribution"] = normalize_cache(&raw, id)?;
            }
        }
        shared
            .runtime
            .inspect(|state| validate_scope(state, id, epoch))?;
        normalized["endpointId"] = json!(endpoint.identity());
        normalized["sessionId"] = json!(id);
        normalized["connectionEpoch"] = json!(epoch);
        normalized["billing"] = billing;
        Ok(normalized)
    }
}

fn validate_scope(state: &MemoryProjectionStore, id: &str, epoch: u64) -> Result<()> {
    require(
        state.connection_epoch == epoch,
        "context connection epoch is stale",
    )?;
    require(
        matches!(state.connection, ConnectionState::Ready),
        "context requires a ready connection",
    )?;
    require(
        state.control_ready,
        "context requires ready control projections",
    )?;
    require(
        state.sessions.contains_key(id),
        "context session is unavailable",
    )
}
const MAX: u64 = 9_007_199_254_740_991;
fn count(value: Option<&Value>, positive: bool) -> Result<Option<u64>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            let n = value.as_u64().ok_or_else(|| {
                Error::Protocol("context token count must be an unsigned integer".into())
            })?;
            require(
                n <= MAX && (!positive || n > 0),
                "context token count is out of range",
            )?;
            Ok(Some(n))
        }
    }
}
fn usage(raw: &Value) -> Result<Value> {
    require(raw.is_object(), "contextUsage must be an object")?;
    let basis = string(raw, "basis")?;
    require(
        ["reported", "estimated", "upperBound", "unknown"].contains(&basis),
        "unknown context usage basis",
    )?;
    let used = count(raw.get("usedTokens"), false)?;
    let window = count(raw.get("contextWindow"), true)?;
    require(
        (basis == "unknown") == used.is_none(),
        "context usage basis and total disagree",
    )?;
    for key in ["provider", "model"] {
        require(
            raw.get(key).is_none_or(|v| v.is_null() || v.is_string()),
            "invalid context route identity",
        )?;
    }
    Ok(
        json!({"usedTokens":used,"contextWindow":window,"basis":basis,"provider":raw.get("provider"),"model":raw.get("model")}),
    )
}
fn pi_usage(stats: &Value) -> Result<Value> {
    require(stats.is_object(), "Pi session stats must be an object")?;
    let raw = stats.get("contextUsage").unwrap_or(&Value::Null);
    require(
        raw.is_null() || raw.is_object(),
        "Pi contextUsage must be an object",
    )?;
    let used = count(raw.get("tokens"), false)?;
    let window = count(raw.get("contextWindow"), true)?;
    Ok(
        json!({"usedTokens":used,"contextWindow":window,"basis":if used.is_some(){"estimated"}else{"unknown"},"provider":null,"model":null}),
    )
}
fn dsh_usage(raw: &Value) -> Result<Value> {
    require(raw.is_object(), "DSH contextPressure must be an object")?;
    let projected = count(raw.get("projectedTokens"), false)?;
    let reported = count(raw.get("pressureTokens"), false)?;
    let used = projected.or(reported);
    let window = count(raw.get("contextWindow"), true)?;
    Ok(
        json!({"usedTokens":used,"contextWindow":window,"basis":if projected.is_some(){"estimated"}else if reported.is_some(){"reported"}else{"unknown"},"provider":null,"model":null}),
    )
}
fn validate_rows(rows: &Value, ids: &mut BTreeSet<String>, depth: usize) -> Result<()> {
    require(depth <= 16, "context details rows are too deep")?;
    for row in rows
        .as_array()
        .ok_or_else(|| Error::Protocol("context rows must be an array".into()))?
    {
        let id = string(row, "id")?;
        require(
            !id.is_empty() && ids.insert(id.into()),
            "duplicate or empty context row identity",
        )?;
        require(
            !string(row, "category")?.is_empty(),
            "context row category is empty",
        )?;
        string(row, "title")?;
        require(
            ["occupied", "reserved", "available", "deferred", "inventory"]
                .contains(&string(row, "role")?),
            "invalid context row role",
        )?;
        if let Some(t) = row.get("tokens").filter(|v| !v.is_null()) {
            require(
                count(t.get("value"), false)?.is_some(),
                "context row tokens are missing",
            )?;
            require(
                [
                    "reported",
                    "tokenized",
                    "estimated",
                    "upperBound",
                    "lowerBound",
                ]
                .contains(&string(t, "basis")?),
                "invalid context row token basis",
            )?;
        }
        if let Some(c) = row.get("itemCount").filter(|v| !v.is_null()) {
            require(
                count(c.get("value"), false)?.is_some(),
                "context item count is missing",
            )?;
        }
        if let Some(children) = row.get("children") {
            validate_rows(children, ids, depth + 1)?;
        }
    }
    Ok(())
}
fn details(raw: &Value) -> Result<Value> {
    require(
        raw["schemaVersion"] == 1 && !string(raw, "revision")?.is_empty(),
        "invalid context details revision",
    )?;
    let evidence = &raw["evidence"];
    require(
        !string(evidence, "sourceKey")?.is_empty(),
        "context evidence source is missing",
    )?;
    require(
        [
            "preparedRequest",
            "nextRequest",
            "retainedSurface",
            "loadedHistory",
            "catalog",
        ]
        .contains(&string(evidence, "scope")?),
        "invalid context evidence scope",
    )?;
    require(
        ["harness", "mirror", "derived"].contains(&string(evidence, "origin")?),
        "invalid context evidence origin",
    )?;
    require(
        ["complete", "partial", "unknown"].contains(&string(raw, "coverage")?),
        "invalid context coverage",
    )?;
    require(
        ["reconciled", "independentEstimate", "unknown"].contains(&string(raw, "relation")?),
        "invalid context relation",
    )?;
    validate_rows(&raw["rows"], &mut BTreeSet::new(), 0)?;
    if let Some(total) = raw.get("usage").filter(|v| !v.is_null()) {
        usage(total)?;
        require(
            route_matches(total, evidence),
            "context detail route and usage disagree",
        )?;
    }
    if let Some(reserve) = raw.get("reserveOutsideUsed").filter(|v| !v.is_null()) {
        require(
            count(reserve.get("value"), false)?.is_some(),
            "invalid context reserve",
        )?;
    }
    Ok(raw.clone())
}
fn route_matches(total: &Value, evidence: &Value) -> bool {
    ["provider", "model"].iter().all(|key| {
        total.get(key).is_none_or(Value::is_null)
            || evidence.get(key).is_none_or(Value::is_null)
            || total[*key] == evidence[*key]
    })
}
fn request_bound(sample: &Value) -> bool {
    sample.get("usage").is_some_and(|v| !v.is_null())
        || sample["evidence"]
            .get("requestID")
            .is_some_and(|v| !v.is_null())
        || matches!(
            sample["evidence"]["scope"].as_str(),
            Some("preparedRequest" | "nextRequest")
        )
}
fn legacy_details(raw: &Value, seq: i64) -> Result<Value> {
    require(raw.is_object(), "contextBreakdown must be an object")?;
    let mut rows = Vec::new();
    for (key, category, title) in [
        ("systemTokens", "systemPrompt", "System prompt"),
        ("toolsTokens", "tools", "Tools"),
        ("messageTokens", "messages", "Messages"),
    ] {
        if let Some(n) = count(raw.get(key), false)? {
            rows.push(json!({"id":format!("legacy:{category}"),"category":category,"title":title,"role":"occupied","tokens":{"value":n,"basis":"estimated","accountingID":"retained-surface-heuristic"},"children":[]}));
        }
    }
    Ok(
        json!({"schemaVersion":1,"revision":format!("legacy:{seq}"),"evidence":{"origin":"harness","scope":"retainedSurface","sourceKey":"contextBreakdown","sourceRevision":seq.to_string(),"clock":"contextBreakdown"},"usage":null,"coverage":"partial","relation":"independentEstimate","rows":rows,"unavailableReason":"Independent retained-surface estimate; not a decomposition of the occupied total"}),
    )
}
fn normalize_projections(values: &BTreeMap<String, VersionedValue>, kind: &str) -> Result<Value> {
    let mut notices = Vec::new();
    let canonical = values.get("contextUsage");
    let pressure = values.get("contextPressure").filter(|_| kind == "dsh");
    let (total, is_canonical) = match (canonical, pressure) {
        (Some(a), Some(b)) if b.sequence > a.sequence => (Some(b), false),
        (Some(a), _) => (Some(a), true),
        (None, b) => (b, false),
    };
    let mut used = match total {
        Some(value) if value.value.is_null() => Value::Null,
        Some(value) if is_canonical => usage(&value.value)?,
        Some(value) => dsh_usage(&value.value)?,
        None => Value::Null,
    };
    let raw_details = values.get("contextDetails");
    let mut sample = match raw_details.filter(|v| !v.value.is_null()) {
        Some(value) => match details(&value.value) {
            Ok(v) => Some(v),
            Err(_) => {
                notices.push("malformed-details");
                None
            }
        },
        None => None,
    };
    if let (Some(value), Some(raw)) = (&sample, raw_details) {
        if request_bound(value) && total.is_some_and(|u| u.sequence > raw.sequence) {
            sample = None;
            notices.push("newer-total-invalidates-details");
        } else if let Some(embedded) = value.get("usage").filter(|v| !v.is_null()) {
            let normalized = usage(embedded)?;
            if total.is_some_and(|u| u.sequence == raw.sequence) && used != normalized {
                sample = None;
                notices.push("same-sequence-total-conflict");
            } else {
                used = normalized;
            }
        }
    }
    if sample
        .as_ref()
        .is_some_and(|v| !route_matches(&used, &v["evidence"]))
    {
        sample = None;
        notices.push("route-changed");
    }
    if sample.is_none()
        && raw_details.is_none()
        && let Some(raw) = values
            .get("contextBreakdown")
            .filter(|v| !v.value.is_null())
    {
        sample = match legacy_details(&raw.value, raw.sequence) {
            Ok(v) => Some(v),
            Err(_) => {
                notices.push("malformed-breakdown");
                None
            }
        };
    }
    let invalidated = total.is_some() && (used.is_null() || used["basis"] == "unknown");
    if let Some(raw) = values.get("contextManifest").filter(|v| !v.value.is_null()) {
        match details(&raw.value) {
            Ok(manifest) if route_matches(&used, &manifest["evidence"]) => {
                let rows = manifest_rows(&manifest["rows"], &manifest["evidence"], invalidated);
                if !rows.is_empty() {
                    let current = sample.get_or_insert_with(|| json!({"schemaVersion":1,"revision":format!("manifest:{}",manifest["revision"].as_str().unwrap_or("unknown")),"evidence":manifest["evidence"],"coverage":"partial","relation":"unknown","rows":[]}));
                    current["rows"]
                        .as_array_mut()
                        .expect("validated rows")
                        .extend(rows);
                }
            }
            Ok(_) => notices.push("manifest-route-changed"),
            Err(_) => notices.push("malformed-manifest"),
        }
    }
    if invalidated && let Some(value) = &mut sample {
        value["rows"]
            .as_array_mut()
            .expect("validated rows")
            .retain(|r| r["role"] == "inventory" || r["role"] == "deferred");
        value["relation"] = json!("unknown");
    }
    if sample
        .as_ref()
        .is_some_and(|v| v["relation"] == "reconciled")
    {
        let value = sample.as_mut().expect("present");
        let rows = value["rows"].as_array().expect("validated");
        let occupied: Vec<_> = rows.iter().filter(|r| r["role"] == "occupied").collect();
        let sum = occupied
            .iter()
            .try_fold(0u64, |n, r| n.checked_add(r["tokens"]["value"].as_u64()?));
        let accounting: BTreeSet<_> = occupied
            .iter()
            .filter_map(|r| r["tokens"]["accountingID"].as_str())
            .collect();
        let basis = used["basis"].as_str();
        let coherent = value["coverage"] == "complete"
            && value["evidence"]
                .get("requestID")
                .is_some_and(|v| !v.is_null())
            && matches!(
                value["evidence"]["scope"].as_str(),
                Some("preparedRequest" | "nextRequest")
            )
            && !occupied.is_empty()
            && accounting.len() == 1
            && occupied.iter().all(|r| {
                r["tokens"]["accountingID"].is_string()
                    && r.get("evidence")
                        .is_none_or(|e| e.is_null() || e == &value["evidence"])
                    && match basis {
                        Some("reported") => matches!(
                            r["tokens"]["basis"].as_str(),
                            Some("reported" | "tokenized")
                        ),
                        Some("estimated") => r["tokens"]["basis"] == "estimated",
                        Some("upperBound") => r["tokens"]["basis"] == "upperBound",
                        _ => false,
                    }
            })
            && sum == used["usedTokens"].as_u64();
        if !coherent {
            value["relation"] = json!("independentEstimate");
        }
    }
    Ok(
        json!({"usage":used,"details":sample,"usageSequence":total.map(|v|v.sequence),"detailsSequence":raw_details.map(|v|v.sequence),"notices":notices}),
    )
}
fn manifest_rows(rows: &Value, parent: &Value, invalidated: bool) -> Vec<Value> {
    rows.as_array()
        .expect("validated manifest rows")
        .iter()
        .filter_map(|row| {
            let evidence = row
                .get("evidence")
                .filter(|v| !v.is_null())
                .unwrap_or(parent);
            let bound = evidence.get("requestID").is_some_and(|v| !v.is_null())
                || matches!(
                    evidence["scope"].as_str(),
                    Some("preparedRequest" | "nextRequest")
                );
            if invalidated && bound {
                return None;
            }
            let mut copy = row.clone();
            copy["id"] = json!(format!(
                "manifest:{}",
                row["id"].as_str().expect("validated identity")
            ));
            copy["role"] = if row["role"] == "deferred" {
                json!("deferred")
            } else {
                json!("inventory")
            };
            copy["evidence"] = evidence.clone();
            if let Some(children) = row.get("children") {
                let kept = manifest_rows(children, evidence, invalidated);
                if kept.len() != children.as_array().expect("validated children").len() {
                    copy["childrenCoverage"] = json!("partial");
                    copy["itemCount"] = Value::Null;
                }
                copy["children"] = json!(kept);
            }
            Some(copy)
        })
        .collect()
}
fn normalize_billing(raw: &Value, id: &str) -> Result<Value> {
    require(
        raw["sessionId"] == id && raw["format"] == 1,
        "billing summary identity is invalid",
    )?;
    let mut result = serde_json::Map::new();
    for key in ["inputTokens", "outputTokens", "totalTokens"] {
        require(
            count(raw.get(key), false)?.is_some(),
            "billing total is missing",
        )?;
        result.insert(key.into(), raw[key].clone());
    }
    Ok(Value::Object(result))
}
fn normalize_cache(raw: &Value, id: &str) -> Result<Value> {
    require(
        raw["sessionId"] == id && raw["format"] == 1,
        "cache attribution identity is invalid",
    )?;
    let rows = raw["entries"]
        .as_array()
        .ok_or_else(|| Error::Protocol("cache attribution entries must be an array".into()))?;
    rows.iter()
        .map(|row| {
            require(
                count(row.get("attempts"), false)?.is_some(),
                "cache attribution attempts missing",
            )?;
            let mut r = serde_json::Map::new();
            for key in ["inputTokens", "outputTokens", "cacheReadTokens"] {
                let n = string(row, key)?;
                require(
                    !n.is_empty()
                        && n.bytes().all(|c| c.is_ascii_digit())
                        && (n == "0" || !n.starts_with('0')),
                    "cache attribution count is invalid",
                )?;
                r.insert(key.into(), json!(n));
            }
            for key in ["dialectId", "modelProfileId", "exactSku"] {
                r.insert(key.into(), json!(string(&row["target"], key)?));
            }
            r.insert("attempts".into(), row["attempts"].clone());
            Ok(Value::Object(r))
        })
        .collect::<Result<Vec<_>>>()
        .map(Value::Array)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn value(sequence: i64, value: Value) -> VersionedValue {
        VersionedValue { sequence, value }
    }
    fn total(tokens: u64) -> Value {
        json!({"usedTokens":tokens,"contextWindow":1000,"basis":"reported"})
    }
    fn sample(tokens: u64) -> Value {
        json!({"schemaVersion":1,"revision":"sample","evidence":{"origin":"harness","scope":"preparedRequest","sourceKey":"context","requestID":"request"},"usage":total(tokens),"coverage":"complete","relation":"reconciled","rows":[{"id":"messages","category":"messages","title":"Messages","role":"occupied","tokens":{"value":tokens,"basis":"reported","accountingID":"request"},"children":[]}]})
    }
    #[test]
    fn details_first_uses_embedded_total_and_newer_total_invalidates_old_details() {
        let mut p = BTreeMap::new();
        p.insert("contextDetails".into(), value(4, sample(50)));
        let n = normalize_projections(&p, "native").unwrap();
        assert_eq!(n["usage"]["usedTokens"], 50);
        assert!(n["details"].is_object());
        p.insert("contextUsage".into(), value(5, total(60)));
        let n = normalize_projections(&p, "native").unwrap();
        assert_eq!(n["usage"]["usedTokens"], 60);
        assert!(n["details"].is_null());
    }
    #[test]
    fn same_sequence_conflict_and_explicit_clear_cannot_revive_embedded_total() {
        let mut p = BTreeMap::from([
            ("contextDetails".into(), value(4, sample(50))),
            ("contextUsage".into(), value(4, total(60))),
        ]);
        let n = normalize_projections(&p, "native").unwrap();
        assert!(n["details"].is_null());
        assert_eq!(n["usage"]["usedTokens"], 60);
        p.insert("contextUsage".into(), value(5, Value::Null));
        let n = normalize_projections(&p, "native").unwrap();
        assert!(n["usage"].is_null());
        assert!(n["details"].is_null());
    }
    #[test]
    fn independent_breakdown_keeps_its_clock_but_unknown_usage_removes_occupied_rows() {
        let mut p = BTreeMap::from([
            (
                "contextPressure".into(),
                value(
                    8,
                    json!({"projectedTokens":70,"pressureTokens":60,"contextWindow":1000}),
                ),
            ),
            (
                "contextBreakdown".into(),
                value(3, json!({"messageTokens":40,"systemTokens":20})),
            ),
        ]);
        let n = normalize_projections(&p, "dsh").unwrap();
        assert_eq!(n["usage"]["basis"], "estimated");
        assert_eq!(n["details"]["relation"], "independentEstimate");
        assert_eq!(n["details"]["rows"].as_array().unwrap().len(), 2);
        p.insert("contextPressure".into(), value(9, Value::Null));
        assert!(
            normalize_projections(&p, "dsh").unwrap()["details"]["rows"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn static_manifest_survives_clear_but_request_inventory_is_removed() {
        let mut static_manifest = sample(50);
        static_manifest["evidence"] =
            json!({"origin":"harness","scope":"catalog","sourceKey":"skills"});
        static_manifest["usage"] = Value::Null;
        static_manifest["rows"][0]["evidence"] = json!({"origin":"harness","scope":"preparedRequest","sourceKey":"bound","requestID":"old"});
        static_manifest["rows"].as_array_mut().unwrap().push(json!({"id":"skill","category":"skills","title":"Skill inventory","role":"inventory","children":[]}));
        let p = BTreeMap::from([
            ("contextUsage".into(), value(9, Value::Null)),
            ("contextManifest".into(), value(2, static_manifest)),
        ]);
        let n = normalize_projections(&p, "native").unwrap();
        assert!(n["usage"].is_null());
        let rows = n["details"]["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], "manifest:skill");
        assert_eq!(rows[0]["role"], "inventory");
    }
    #[test]
    fn optional_malformed_details_are_explicit_and_cannot_hide_valid_pressure() {
        let p = BTreeMap::from([
            ("contextUsage".into(), value(2, total(30))),
            (
                "contextDetails".into(),
                value(2, json!({"schemaVersion":9})),
            ),
        ]);
        let n = normalize_projections(&p, "native").unwrap();
        assert_eq!(n["usage"]["usedTokens"], 30);
        assert!(n["details"].is_null());
        assert_eq!(n["notices"][0], "malformed-details");
        assert!(count(Some(&json!(9_007_199_254_740_992u64)), false).is_err());
    }
    #[test]
    fn pi_current_context_never_uses_cumulative_billing_and_malformed_is_not_unknown() {
        let stats = json!({"tokens":{"input":99999,"output":88888},"contextUsage":{"tokens":12,"contextWindow":1000}});
        assert_eq!(pi_usage(&stats).unwrap()["usedTokens"], 12);
        assert_eq!(
            pi_usage(&json!({"tokens":{"input":99999}})).unwrap()["basis"],
            "unknown"
        );
        assert!(pi_usage(&json!({"contextUsage":{"tokens":-1}})).is_err());
        assert!(usage(&json!({"basis":"reported"})).is_err());
    }
}
