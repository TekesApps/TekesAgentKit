use serde_json::{Value, json};
use tekes_agent_presentation::RunScope;
use tekes_agent_ui_demo::endpoint_config::HostEndpointConfiguration;

fn native(id: &str, sessions: [&str; 2]) -> Value {
    json!({"kind":"native","endpointId":id,"url":"http://127.0.0.1:3210","bearerEnv":"NATIVE_TOKEN","sessions":sessions})
}

fn pi(id: &str, sessions: [&str; 2]) -> Value {
    json!({"kind":"pi","endpointId":id,"executable":"/owned/node","arguments":["/owned/pi.js"],"dataRoot":"/owned/pi-data","sessions":sessions})
}

fn dsh(id: &str, sessions: [&str; 2]) -> Value {
    json!({"kind":"dsh","endpointId":id,"url":"http://127.0.0.1:3211","sessions":sessions})
}

fn parse(value: &Value) -> tekes_agent_contract::Result<HostEndpointConfiguration> {
    HostEndpointConfiguration::parse(value, |_| Some("test-only-token".into()))
}

#[test]
fn accepts_legacy_single_endpoint_and_preserves_its_scope_order() {
    let config = parse(&native("one", ["alpha", "beta"])).unwrap();
    assert_eq!(
        config.scopes(),
        &[
            RunScope::main("one", "alpha"),
            RunScope::main("one", "beta")
        ]
    );
}

#[test]
fn parses_mixed_three_kind_endpoint_array_in_configuration_and_session_order() {
    let input = json!({"endpoints":[
        pi("pi", ["p1", "p2"]),
        native("native", ["n1", "n2"]),
        dsh("dsh", ["d1", "d2"])
    ]});
    let config = parse(&input).unwrap();
    assert_eq!(
        config.scopes(),
        &[
            RunScope::main("pi", "p1"),
            RunScope::main("pi", "p2"),
            RunScope::main("native", "n1"),
            RunScope::main("native", "n2"),
            RunScope::main("dsh", "d1"),
            RunScope::main("dsh", "d2"),
        ]
    );
}

#[test]
fn rejects_empty_non_array_mixed_top_level_and_extra_top_level_fields() {
    for input in [
        json!({"endpoints":[]}),
        json!({"endpoints":native("one", ["a", "b"])}),
        json!({"endpoints":[native("one", ["a", "b"])],"kind":"native"}),
        json!({"endpoints":[native("one", ["a", "b"])],"endpointId":"one"}),
    ] {
        assert!(parse(&input).is_err(), "accepted {input}");
    }
}

#[test]
fn rejects_duplicate_ids_and_any_invalid_child_before_returning_a_host_config() {
    let duplicate = json!({"endpoints":[
        native("same", ["a", "b"]), dsh("same", ["c", "d"])
    ]});
    assert!(parse(&duplicate).is_err());

    let bad_child = json!({"endpoints":[
        native("valid", ["a", "b"]),
        {"kind":"pi","endpointId":"bad","executable":"relative","dataRoot":"/owned/data","sessions":["x","y"]}
    ]});
    assert!(parse(&bad_child).is_err());
}

#[test]
fn environment_authority_failures_do_not_disclose_secret_values() {
    let secret = "do-not-print-this-token";
    let error =
        HostEndpointConfiguration::parse(&json!({"endpoints":[native("one", ["a", "b"])]}), |_| {
            Some(format!("{secret}\r\ninjected"))
        })
        .err()
        .expect("invalid credential accepted");
    assert!(!error.to_string().contains(secret));

    let error = HostEndpointConfiguration::parse(
        &json!({"endpoints":[native("one", ["a", "b"]), native("two", ["c", "d"])]}),
        |name| {
            assert_eq!(name, "NATIVE_TOKEN");
            None
        },
    )
    .err()
    .expect("missing credential accepted");
    assert!(!error.to_string().contains(secret));
}
