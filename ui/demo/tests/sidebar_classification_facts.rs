use serde_json::{Value, json};
use tekes_agent_ui_bindings::{
    SidebarClassificationFact as Fact, SidebarClassificationFacts as Facts,
    SidebarClassificationOrigin as Origin,
};

#[test]
fn missing_fields_remain_missing() {
    let facts = Facts::from_summary(&json!({})).unwrap();
    assert_eq!(facts.origin, Fact::Missing);
    assert_eq!(facts.parent_session_id, Fact::Missing);
    assert_eq!(facts.ephemeral, Fact::Missing);
}

#[test]
fn explicit_boolean_values_remain_known() {
    assert_eq!(
        Facts::from_summary(&json!({"ephemeral": false}))
            .unwrap()
            .ephemeral,
        Fact::Known(false)
    );
    assert_eq!(
        Facts::from_summary(&json!({"ephemeral": true}))
            .unwrap()
            .ephemeral,
        Fact::Known(true)
    );
}

#[test]
fn established_origin_values_are_typed_without_collapsing_fork_into_subagent() {
    assert_eq!(
        Facts::from_summary(&json!({"origin": "subagent"}))
            .unwrap()
            .origin,
        Fact::Known(Origin::Subagent)
    );
    assert_eq!(
        Facts::from_summary(&json!({"origin": "fork"}))
            .unwrap()
            .origin,
        Fact::Known(Origin::Fork)
    );
}

#[test]
fn parent_identity_alone_is_preserved_as_a_fact_without_classifying_origin() {
    let facts = Facts::from_summary(&json!({"parentSessionId": "parent-1"})).unwrap();
    assert_eq!(facts.parent_session_id, Fact::Known("parent-1".into()));
    assert_eq!(facts.origin, Fact::Missing);
    assert_eq!(facts.ephemeral, Fact::Missing);
}

#[test]
fn null_future_values_wrong_types_and_empty_identity_remain_unknown() {
    let summary = json!({
        "origin": null,
        "parentSessionId": "",
        "ephemeral": "true"
    });
    let facts = Facts::from_summary(&summary).unwrap();
    assert_eq!(facts.origin, Fact::Unknown(Value::Null));
    assert_eq!(facts.parent_session_id, Fact::Unknown(json!("")));
    assert_eq!(facts.ephemeral, Fact::Unknown(json!("true")));

    let future = json!({
        "origin": "worker-v2",
        "parentSessionId": 42,
        "ephemeral": null
    });
    let facts = Facts::from_summary(&future).unwrap();
    assert_eq!(facts.origin, Fact::Unknown(json!("worker-v2")));
    assert_eq!(facts.parent_session_id, Fact::Unknown(json!(42)));
    assert_eq!(facts.ephemeral, Fact::Unknown(Value::Null));
    let unknown = json!({"origin":"main","parentSessionId":"  ","ephemeral":{"future":true}});
    let facts = Facts::from_summary(&unknown).unwrap();
    assert_eq!(facts.origin, Fact::Unknown(json!("main")));
    assert_eq!(facts.parent_session_id, Fact::Unknown(json!("  ")));
    assert_eq!(facts.ephemeral, Fact::Unknown(json!({"future":true})));
}

#[test]
fn non_object_summary_is_rejected() {
    for summary in [Value::Null, json!("summary"), json!([])] {
        assert!(Facts::from_summary(&summary).is_err());
    }
}
