//! DSH's public inbox shape to the versioned AgentKit queue projection.
//!
//! This is deliberately a narrow wire adapter. It preserves DSH's two ordered
//! groups and does not infer an order between the groups.
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use tekes_agent_contract::{Error, Result, SEQUENCED_QUEUE_PROJECTION_KEY, require, string};

/// Local inventory metadata reserved for the original, unsequenced DSH cache hint.
const DSH_CACHED_PROJECTION_HINTS_KEY: &str = "dshCachedProjectionHints";

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
        normalize_inventory_snapshot_row(row)?;
    }
    Ok(())
}

pub(super) fn normalize_inventory_row(row: &mut Value) -> Result<()> {
    normalize_inventory_row_with_cached_hints(row, false)
}

/// Inventory snapshots are header-only listings, so their cached projection
/// hints may be retained as opaque metadata without acquiring live authority.
fn normalize_inventory_snapshot_row(row: &mut Value) -> Result<()> {
    normalize_inventory_row_with_cached_hints(row, true)
}

fn normalize_inventory_row_with_cached_hints(row: &mut Value, allow_cached: bool) -> Result<()> {
    require(
        row.get(DSH_CACHED_PROJECTION_HINTS_KEY).is_none(),
        "remote DSH inventory contains a reserved AgentKit key",
    )?;
    if let Some(projection) = row.get("projections") {
        reject_remote_projection(projection)?;
        if allow_cached && projection.get("kind") == Some(&Value::String("cached".into())) {
            let seq = projection
                .get("asOfSeq")
                .and_then(Value::as_i64)
                .ok_or_else(|| Error::Protocol("missing DSH cached projection sequence".into()))?;
            require(seq >= -1, "invalid DSH cached projection sequence")?;
            let values = projection
                .get("values")
                .and_then(Value::as_object)
                .ok_or_else(|| Error::Protocol("invalid DSH cached projection values".into()))?;
            if let Some(title) = values.get("title") {
                require(
                    title.is_null() || title.is_string(),
                    "invalid DSH cached title projection",
                )?;
            }
            let cached_hints = projection.clone();
            let cached_title = values
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_owned);

            let row = row
                .as_object_mut()
                .ok_or_else(|| Error::Protocol("invalid DSH inventory row".into()))?;
            if row.get("title").is_none()
                && let Some(title) = cached_title
            {
                row.insert("title".into(), Value::String(title));
            }
            row.insert(DSH_CACHED_PROJECTION_HINTS_KEY.into(), cached_hints);
            row.remove("projections");
            return Ok(());
        }
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

    #[test]
    fn cached_inventory_hints_are_retained_without_live_authority() {
        let cached = json!({
            "kind":"cached",
            "asOfSeq":9000,
            "values":{
                "title":"persisted title",
                "inbox":{"next-turn":[{"id":"stale"}],"next-step":[]},
                "workingDirectory":"/stale"
            }
        });
        let mut snapshot = json!({"items":[
            {"sessionId":"cached","projections":cached},
            {"sessionId":"live","title":"cached fallback","projections":{"kind":"sequenced","asOfSeq":3,"values":{"title":null,"inbox":{"next-turn":[],"next-step":[]},"workingDirectory":"/live"}}}
        ]});

        normalize_inventory_snapshot(&mut snapshot).unwrap();

        let cached_row = &snapshot["items"][0];
        assert_eq!(cached_row["title"], "persisted title");
        assert_eq!(cached_row[DSH_CACHED_PROJECTION_HINTS_KEY], cached);
        assert!(cached_row.get("projections").is_none());
        assert!(cached_row.get("agentkit.queue.v1").is_none());

        let live_row = &snapshot["items"][1];
        assert_eq!(live_row["title"], "cached fallback");
        assert_eq!(live_row["projections"]["asOfSeq"], 3);
        assert_eq!(live_row["projections"]["values"]["title"], Value::Null);
        assert!(live_row[DSH_CACHED_PROJECTION_HINTS_KEY].is_null());
    }

    #[test]
    fn cached_watermark_never_poison_live_store_or_clear_authoritative_null() {
        use tekes_agent_session::MemoryProjectionStore;
        let mut raw_store = serde_json::to_value(MemoryProjectionStore::new("dsh")).unwrap();
        raw_store["sequenced_queue_projection_key"] = json!(SEQUENCED_QUEUE_PROJECTION_KEY);
        let mut store: MemoryProjectionStore = serde_json::from_value(raw_store).unwrap();
        let mut snapshot = json!({"items":[{
            "sessionId":"s","running":false,"blank":false,"updatedAt":1,
            "projections":{"kind":"cached","asOfSeq":9000,"values":{
                "title":"cached title","workingDirectory":"/stale",
                "inbox":{"next-turn":[{"id":"stale"}],"next-step":[]}
            }}
        }]});
        normalize_inventory_snapshot(&mut snapshot).unwrap();
        store
            .apply(
                "inventory",
                json!({"type":"baseline","generation":"i1","snapshot":snapshot}),
            )
            .unwrap();
        assert!(store.sessions["s"].projections.is_empty());
        assert!(store.sessions["s"].queue.is_empty());
        store
            .apply(
                "control",
                json!({"type":"baseline","generation":"c1","snapshot":{
                    "queues":[],"jobs":[],"projections":[{"sessionId":"s","projections":{
                        "asOfSeq":3,"values":{"title":null,"workingDirectory":"/live",
                            (SEQUENCED_QUEUE_PROJECTION_KEY):{"items":["live"]}}
                    }}]
                }}),
            )
            .unwrap();
        // A later header-only listing is still not a comparable live cut.
        store
            .apply(
                "inventory",
                json!({"type":"baseline","generation":"i2","snapshot":snapshot}),
            )
            .unwrap();
        let session = &store.sessions["s"];
        assert_eq!(session.projections["title"].sequence, 3);
        assert_eq!(session.projections["title"].value, Value::Null);
        assert_eq!(session.projections["workingDirectory"].value, "/live");
        assert_eq!(session.queue, vec![json!("live")]);
        assert_eq!(
            session.summary.as_ref().unwrap()[DSH_CACHED_PROJECTION_HINTS_KEY]["asOfSeq"],
            9000
        );
    }

    #[test]
    fn cached_inventory_validation_and_reserved_metadata_fail_closed() {
        for projection in [
            json!({"kind":"cached","asOfSeq":-2,"values":{}}),
            json!({"kind":"cached","asOfSeq":1,"values":[]}),
            json!({"kind":"cached","asOfSeq":1,"values":{"title":42}}),
            json!({"kind":"unknown","asOfSeq":1,"values":{}}),
            json!({"kind":"cached","asOfSeq":1,"values":{"agentkit.queue.v1":{"items":[]}}}),
        ] {
            let mut snapshot = json!({"items":[{"sessionId":"s","projections":projection}]});
            assert!(normalize_inventory_snapshot(&mut snapshot).is_err());
        }

        let mut spoofed =
            json!({"items":[{"sessionId":"s","dshCachedProjectionHints":{"kind":"cached"}}]});
        assert!(normalize_inventory_snapshot(&mut spoofed).is_err());

        let explicit_read =
            json!({"kind":"cached","asOfSeq":10000,"values":{"workingDirectory":"/stale"}});
        assert!(normalize_projection(&explicit_read).is_err());

        let mut connected_upsert =
            json!({"sessionId":"s","projections":{"kind":"cached","asOfSeq":1,"values":{}}});
        assert!(normalize_inventory_row(&mut connected_upsert).is_err());
    }

    #[test]
    fn cached_title_never_replaces_existing_top_level_title_or_null() {
        let cached = json!({"kind":"cached","asOfSeq":1,"values":{"title":"stale"}});
        let mut snapshot = json!({"items":[
            {"sessionId":"existing","title":"current","projections":cached},
            {"sessionId":"explicit-null","title":null,"projections":{"kind":"cached","asOfSeq":1,"values":{"title":"stale"}}}
        ]});

        normalize_inventory_snapshot(&mut snapshot).unwrap();

        assert_eq!(snapshot["items"][0]["title"], "current");
        assert_eq!(snapshot["items"][1]["title"], Value::Null);
    }
}
