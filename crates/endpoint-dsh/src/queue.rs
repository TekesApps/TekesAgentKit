//! DSH's public inbox shape to the versioned AgentKit queue projection.
//!
//! This is deliberately a narrow wire adapter. It preserves DSH's two ordered
//! groups and does not infer an order between the groups.
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use tekes_agent_contract::{Error, Result, SEQUENCED_QUEUE_PROJECTION_KEY, require, string};

fn reject_reserved(values: &Value) -> Result<()> {
    if values
        .as_object()
        .is_some_and(|values| values.contains_key(SEQUENCED_QUEUE_PROJECTION_KEY))
    {
        return Err(Error::Protocol(
            "remote DSH projection contains a reserved AgentKit key".into(),
        ));
    }
    Ok(())
}

/// Converts only the documented DSH inbox. Missing inboxes remain missing;
/// migration hints never provide sequenced queue authority.
pub(super) fn normalize_projection(projection: &Value) -> Result<Option<Value>> {
    if projection.is_null() {
        return Ok(None);
    }

    let kind = projection.get("kind");
    let values = projection.get("values").unwrap_or(&Value::Null);
    reject_reserved(values)?;

    if kind.is_some_and(|kind| kind == "migration-required") {
        return Ok(None);
    }
    if let Some(kind) = kind {
        require(kind == "sequenced", "unknown DSH projection authority")?;
    }
    let seq = projection
        .get("asOfSeq")
        .and_then(Value::as_i64)
        .ok_or_else(|| Error::Protocol("missing DSH projection sequence".into()))?;
    require(seq >= -1, "invalid DSH projection sequence")?;

    let mut values = values
        .as_object()
        .cloned()
        .ok_or_else(|| Error::Protocol("invalid DSH projection values".into()))?;
    if let Some(inbox) = values.get("inbox") {
        let Some(items) = normalize_inbox(inbox)? else {
            return Ok(None);
        };
        values.insert(
            SEQUENCED_QUEUE_PROJECTION_KEY.into(),
            json!({"items":items}),
        );
    }

    let mut normalized = projection.clone();
    normalized["values"] = Value::Object(values);
    Ok(Some(normalized))
}

/// Builds the single-sequence block used by live journal projection frames.
pub(super) fn normalize_projection_delta(key: &str, value: &Value, seq: i64) -> Result<Value> {
    require(seq >= -1, "invalid DSH projection sequence")?;
    require(
        key != SEQUENCED_QUEUE_PROJECTION_KEY,
        "remote DSH projection uses a reserved AgentKit key",
    )?;
    let mut values = Map::new();
    values.insert(key.into(), value.clone());
    if key == "inbox"
        && let Some(items) = normalize_inbox(value)?
    {
        values.insert(
            SEQUENCED_QUEUE_PROJECTION_KEY.into(),
            json!({"items":items}),
        );
    }
    Ok(json!({"asOfSeq":seq,"values":values}))
}

pub(super) fn normalize_inventory_snapshot(snapshot: &mut Value) -> Result<()> {
    reject_reserved_anywhere(snapshot)?;
    let Some(rows) = snapshot.get_mut("items").and_then(Value::as_array_mut) else {
        return Err(Error::Protocol("invalid DSH inventory".into()));
    };
    for row in rows {
        normalize_inventory_row(row)?;
    }
    Ok(())
}

pub(super) fn normalize_inventory_row(row: &mut Value) -> Result<()> {
    if let Some(projection) = row.get("projections") {
        let normalized = normalize_projection(projection)?;
        if let Some(normalized) = normalized {
            row.as_object_mut()
                .ok_or_else(|| Error::Protocol("invalid DSH inventory row".into()))?
                .insert("projections".into(), normalized);
        } else {
            row.as_object_mut()
                .ok_or_else(|| Error::Protocol("invalid DSH inventory row".into()))?
                .remove("projections");
        }
    }
    Ok(())
}

/// Rejects a forged local key even on DSH projections consumed for another
/// purpose and therefore not otherwise normalized into SessionEndpoint state.
pub(super) fn reject_remote_projection(projection: &Value) -> Result<()> {
    if let Some(values) = projection.get("values") {
        reject_reserved(values)?;
    }
    Ok(())
}

/// Checks protocol projection positions, never arbitrary message/tool JSON.
pub(super) fn reject_reserved_anywhere(value: &Value) -> Result<()> {
    reject_remote_projection(value)?;
    if value["type"] == "projection" {
        require(
            value["key"] != SEQUENCED_QUEUE_PROJECTION_KEY,
            "remote DSH projection uses a reserved AgentKit key",
        )?;
    }
    if let Some(projection) = value.get("projections") {
        if projection.get("values").is_some() {
            reject_remote_projection(projection)?;
        } else if let Some(map) = projection.as_object() {
            for projection in map.values() {
                reject_remote_projection(projection)?;
            }
        }
    }
    if value["type"] == "baseline"
        && let Some(map) = value["value"].get("projections").and_then(Value::as_object)
    {
        for projection in map.values() {
            reject_remote_projection(projection)?;
        }
    }
    if let Some(items) = value.get("items").and_then(Value::as_array) {
        for item in items {
            if let Some(projection) = item.get("projections") {
                reject_remote_projection(projection)?;
            }
        }
    }
    if value["event"] == "api-session/added"
        && let Some(projection) = value["args"][0].get("projections")
    {
        reject_remote_projection(projection)?;
    }
    Ok(())
}

fn normalize_inbox(inbox: &Value) -> Result<Option<Vec<Value>>> {
    let inbox = inbox
        .as_object()
        .ok_or_else(|| Error::Protocol("invalid DSH inbox".into()))?;
    let next_turn = inbox
        .get("next-turn")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Protocol("invalid DSH next-turn inbox".into()))?;
    let next_step = inbox
        .get("next-step")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Protocol("invalid DSH next-step inbox".into()))?;

    let mut ids = BTreeSet::new();
    let mut items = Vec::with_capacity(next_turn.len() + next_step.len());
    append_group(next_turn, "queued", "next-turn", &mut ids, &mut items)?;
    append_group(next_step, "next-step", "next-step", &mut ids, &mut items)?;
    Ok(Some(items))
}

fn append_group(
    messages: &[Value],
    placement: &str,
    target: &str,
    ids: &mut BTreeSet<String>,
    items: &mut Vec<Value>,
) -> Result<()> {
    for message in messages {
        let object = message
            .as_object()
            .ok_or_else(|| Error::Protocol("invalid DSH inbox message".into()))?;
        let id = string(message, "id")?;
        require(!id.is_empty(), "empty DSH inbox message id")?;
        require(ids.insert(id.to_owned()), "duplicate DSH inbox message id")?;

        let source = object
            .get("source")
            .ok_or_else(|| Error::Protocol("missing DSH inbox source".into()))?;
        let source_kind = string(source, "kind")?;
        require(!source_kind.is_empty(), "empty DSH inbox source kind")?;
        if let Some(rpc_id) = source.get("rpcId") {
            require(
                rpc_id.as_str().is_some_and(|rpc| !rpc.is_empty()),
                "invalid DSH inbox source rpcId",
            )?;
        }

        items.push(json!({
            "id":id,
            "placement":placement,
            "target":target,
            "message":message,
        }));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: &str, source: Value) -> Value {
        json!({"id":id,"role":"user","source":source,"content":[{"type":"text","text":id}],"extra":{"retained":true}})
    }

    fn projection(inbox: Value) -> Value {
        json!({"kind":"sequenced","asOfSeq":41,"values":{"inbox":inbox,"other":{"keep":true}}})
    }

    #[test]
    fn maps_both_public_groups_and_preserves_message_identity_and_source() {
        let user = msg("message-id", json!({"kind":"user","rpcId":"rpc-id"}));
        let context = msg("context-id", json!({"kind":"runtime-context"}));
        let normalized = normalize_projection(&projection(json!({
            "next-turn":[user.clone()],
            "next-step":[context.clone()]
        })))
        .unwrap()
        .unwrap();
        assert_eq!(normalized["asOfSeq"], 41);
        assert_eq!(normalized["values"]["inbox"]["next-turn"][0], user);
        assert_eq!(normalized["values"]["inbox"]["next-step"][0], context);
        let items = &normalized["values"][SEQUENCED_QUEUE_PROJECTION_KEY]["items"];
        assert_eq!(
            items[0],
            json!({"id":"message-id","placement":"queued","target":"next-turn","message":user})
        );
        assert_eq!(items[1]["id"], "context-id");
        assert_eq!(items[1]["placement"], "next-step");
        assert_eq!(items[1]["target"], "next-step");
        assert_eq!(items[0]["id"], "message-id");
        assert_ne!(items[0]["id"], items[0]["message"]["source"]["rpcId"]);
        assert_eq!(items[1]["message"]["source"]["kind"], "runtime-context");
    }

    #[test]
    fn only_explicit_two_group_empty_inbox_produces_empty_items() {
        let empty = normalize_projection(&projection(json!({"next-turn":[],"next-step":[]})))
            .unwrap()
            .unwrap();
        assert_eq!(
            empty["values"][SEQUENCED_QUEUE_PROJECTION_KEY]["items"],
            json!([])
        );
        let absent =
            normalize_projection(&json!({"kind":"sequenced","asOfSeq":41,"values":{"other":1}}))
                .unwrap()
                .unwrap();
        assert!(
            absent["values"]
                .get(SEQUENCED_QUEUE_PROJECTION_KEY)
                .is_none()
        );
        assert!(normalize_projection(&Value::Null).unwrap().is_none());
    }

    #[test]
    fn malformed_groups_duplicate_ids_sources_and_migration_hints_do_not_create_queue() {
        for inbox in [
            json!({"next-turn":[]}),
            json!({"next-turn":{},"next-step":[]}),
            json!({"next-turn":[{}],"next-step":[]}),
            json!({"next-turn":[msg("same",json!({"kind":"user"}))],"next-step":[msg("same",json!({"kind":"user"}))]}),
            json!({"next-turn":[msg("bad",json!("user"))],"next-step":[]}),
        ] {
            assert!(normalize_projection_delta("inbox", &inbox, 42).is_err());
            assert!(normalize_projection(&projection(inbox)).is_err());
        }
        assert!(normalize_projection(&json!({"kind":"migration-required","asOfSeq":41,"values":{"inbox":{"next-turn":[],"next-step":[]}}})).unwrap().is_none());
    }

    #[test]
    fn rejects_remote_reserved_keys_and_projects_live_inbox_atomically() {
        let mut forged = projection(json!({"next-turn":[],"next-step":[]}));
        forged["values"][SEQUENCED_QUEUE_PROJECTION_KEY] = json!({"items":[]});
        assert!(normalize_projection(&forged).is_err());

        let block =
            normalize_projection_delta("inbox", &json!({"next-turn":[],"next-step":[]}), 52)
                .unwrap();
        assert_eq!(block["asOfSeq"], 52);
        assert_eq!(block["values"]["inbox"]["next-turn"], json!([]));
        assert_eq!(
            block["values"][SEQUENCED_QUEUE_PROJECTION_KEY]["items"],
            json!([])
        );
        assert!(
            normalize_projection_delta(SEQUENCED_QUEUE_PROJECTION_KEY, &json!({"items":[]}), 52)
                .is_err()
        );
        assert!(reject_reserved_anywhere(&json!({"type":"projection","key":SEQUENCED_QUEUE_PROJECTION_KEY,"value":{"items":[]}})).is_err());
        assert!(reject_reserved_anywhere(&json!({"records":[{"data":{"nested":{(SEQUENCED_QUEUE_PROJECTION_KEY):"ordinary user JSON"}}}]})).is_ok());
    }

    #[test]
    fn inventory_reuses_projection_adapter_and_preserves_original_fields() {
        let message = msg("stable-id", json!({"kind":"user","rpcId":"submission"}));
        let mut snapshot = json!({"items":[{"sessionId":"s","title":"kept","projections":{"kind":"sequenced","asOfSeq":7,"values":{"inbox":{"next-turn":[message.clone()],"next-step":[]},"directory":{"cwd":"/work"}}}}],"nextCursor":null});
        normalize_inventory_snapshot(&mut snapshot).unwrap();
        assert_eq!(snapshot["items"][0]["title"], "kept");
        assert_eq!(snapshot["items"][0]["projections"]["asOfSeq"], 7);
        assert_eq!(
            snapshot["items"][0]["projections"]["values"]["inbox"]["next-turn"][0],
            message
        );
        assert_eq!(
            snapshot["items"][0]["projections"]["values"][SEQUENCED_QUEUE_PROJECTION_KEY]["items"]
                [0]["id"],
            "stable-id"
        );

        let mut forged = json!({"items":[{"sessionId":"s","projections":{"asOfSeq":7,"values":{(SEQUENCED_QUEUE_PROJECTION_KEY):{"items":[]}}}}]});
        assert!(normalize_inventory_snapshot(&mut forged).is_err());
    }
}
