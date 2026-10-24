use serde_json::{Value, json};
use tekes_agent_ui_demo::family_seed_manifest::parse_family_seeds;

fn manifest(endpoint: &str, index: usize, raw_id: &str) -> Value {
    let parent = format!("{endpoint}-parent-{index}-{raw_id}");
    let child = format!("{raw_id}-{index}-child");
    let scope = json!({
        "endpointId": endpoint,
        "threadId": parent,
        "sessionId": parent,
    });
    let task = if endpoint == "native" {
        json!({
            "id": format!("task-{index}"),
            "scope": scope,
            "name": "Bounded proof",
            "status": "done",
            "kind": "task",
        })
    } else {
        Value::Null
    };
    let child = if endpoint == "pi" {
        json!({
            "address": {"endpointId": endpoint, "sessionId": child},
            "parentScope": scope,
            "summaryRow": {"id": format!("summary-{index}"), "sessionId": child},
            "catalogEntry": {"id": child, "kind": "child", "mode": "continuable"},
            "controlAuthorityGrantedByManifest": false,
        })
    } else if endpoint == "native" {
        json!({
            "address": {"endpointId": endpoint, "sessionId": child},
            "parentScope": scope,
            "readOnly": true,
            "independentRun": false,
            "catalogEntry": {"id": child, "kind": "child", "mode": "bounded"},
            "controlAuthorityGrantedByManifest": false,
        })
    } else {
        json!({
            "address": {"endpointId": endpoint, "sessionId": child},
            "parentScope": scope,
            "catalogEntry": {"id": child, "kind": "child", "mode": "continuable"},
            "controlAuthorityGrantedByManifest": false,
        })
    };
    json!({
        "schemaVersion": 1,
        "endpoint": endpoint,
        "evidenceScope": "paired-public-seed-dtos",
        "realHarnessAcceptance": false,
        "uiAcceptance": false,
        "pairedPublicSourceMatched": true,
        "parent": {"scope": scope},
        "task": if task.is_null() { Value::Null } else { json!({"row": task, "sourceCallId":"actual-call"}) },
        "child": child,
        // Unknown future fields are intentionally tolerated.
        "futureDtoField": {"preservedByCaller": true},
    })
}

fn six_manifests() -> Value {
    json!([
        manifest("native", 1, "same-parent"),
        manifest("native", 2, "same-parent"),
        manifest("pi", 1, "same-parent"),
        manifest("pi", 2, "same-parent"),
        manifest("dsh", 1, "same-parent"),
        manifest("dsh", 2, "same-parent"),
    ])
}

#[test]
fn accepts_six_endpoint_qualified_seeds_and_same_raw_ids_across_endpoints() {
    let parsed = parse_family_seeds(&six_manifests()).expect("six valid seeds");
    assert_eq!(parsed.len(), 6);
    assert_eq!(parsed[0].parent.endpoint_id, "native");
    assert_eq!(parsed[0].child_id, parsed[2].child_id);
    assert_ne!(parsed[0].parent, parsed[2].parent);
    assert!(parsed[0].task.is_some());
    assert!(parsed[2].task.is_none());
    assert_eq!(parsed[2].summary_subagent_id.as_deref(), Some("summary-1"));
    assert_eq!(parsed[4].catalog_mode.as_deref(), Some("continuable"));
}

#[test]
fn rejects_wrong_endpoint_scope_and_cross_parent_child_address() {
    let mut rows = six_manifests();
    rows[0]["parent"]["scope"]["endpointId"] = json!("pi");
    assert!(parse_family_seeds(&rows).is_err());

    let mut rows = six_manifests();
    rows[0]["child"]["parentScope"]["sessionId"] = json!("foreign-parent");
    assert!(parse_family_seeds(&rows).is_err());
}

#[test]
fn rejects_manifest_claimed_authority_and_endpoint_duplicate_parent() {
    let mut rows = six_manifests();
    rows[1]["child"]["controlAuthorityGrantedByManifest"] = json!(true);
    assert!(parse_family_seeds(&rows).is_err());

    let mut rows = six_manifests();
    rows[1]["parent"]["scope"] = rows[0]["parent"]["scope"].clone();
    rows[1]["child"]["parentScope"] = rows[1]["parent"]["scope"].clone();
    assert!(parse_family_seeds(&rows).is_err());
}

#[test]
fn rejects_missing_endpoint_coverage() {
    let mut rows = six_manifests();
    rows[5] = manifest("pi", 3, "same-parent");
    assert!(parse_family_seeds(&rows).is_err());
}

#[test]
fn rejects_task_in_a_non_native_manifest() {
    let mut rows = six_manifests();
    let parent_scope = rows[2]["parent"]["scope"].clone();
    rows[2]["task"] = json!({
        "id": "borrowed-task",
        "scope": parent_scope,
        "name": "Borrowed",
        "status": "done",
        "kind": "task",
    });
    assert!(parse_family_seeds(&rows).is_err());
}
