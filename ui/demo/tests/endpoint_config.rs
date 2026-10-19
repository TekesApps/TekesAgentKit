use serde_json::{Value, json};
use tekes_agent_presentation::RunScope;
use tekes_agent_ui_demo::endpoint_config::EndpointConfiguration;

fn native() -> Value {
    json!({"endpointId":"native-demo","url":"http://127.0.0.1:3210", "bearerEnv":"DEMO_TOKEN","sessions":["alpha","beta"]})
}
fn pi() -> Value {
    json!({"kind":"pi","endpointId":"pi-demo","executable":"/owned/node", "arguments":["/owned/pi.js"],"dataRoot":"/owned/data","sessions":["first","second"],"environment":{"HOME":"/owned/home"},"environmentFromEnv":{"API_KEY":"HOST_KEY"},"extensions":["/owned/control.mjs"],"sessionRoots":["/owned/import"]})
}
fn dsh() -> Value {
    json!({"kind":"dsh","endpointId":"dsh-demo","url":"http://127.0.0.1:3211","cookieEnv":"DSH_COOKIE","sessions":["one","two"]})
}
fn parse(value: &Value) -> tekes_agent_contract::Result<EndpointConfiguration> {
    EndpointConfiguration::parse(value, |_| Some("synthetic-only".into()))
}

#[test]
fn three_kinds_preserve_explicit_endpoint_and_session_identity() {
    for (value, endpoint, sessions) in [
        (native(), "native-demo", ["alpha", "beta"]),
        (pi(), "pi-demo", ["first", "second"]),
        (dsh(), "dsh-demo", ["one", "two"]),
    ] {
        let config = parse(&value).unwrap();
        assert_eq!(
            config.scopes(),
            &[
                RunScope::main(endpoint, sessions[0]),
                RunScope::main(endpoint, sessions[1])
            ]
        );
    }
    let mut explicit_native = native();
    explicit_native["kind"] = json!("native");
    assert!(parse(&explicit_native).is_ok());
}

#[test]
fn invalid_or_duplicate_session_slots_fail_before_any_environment_lookup() {
    for sessions in [
        json!([]),
        json!(["a"]),
        json!(["a", "b", "c"]),
        json!(["a", null]),
        json!(["a", "a"]),
        json!(["a", " "]),
    ] {
        let mut value = native();
        value["sessions"] = sessions;
        assert!(
            EndpointConfiguration::parse(&value, |_| panic!(
                "invalid scopes must fail before credential lookup"
            ))
            .is_err()
        );
    }
}

#[test]
fn invalid_kind_and_misspelled_or_inline_credentials_never_fall_back() {
    for (key, value) in [
        ("kind", json!("typo")),
        ("kind", Value::Null),
        ("bearer", json!("must-not-be-printed")),
        ("session", json!(["a", "b"])),
        ("cookieEnv", json!("DSH_COOKIE")),
    ] {
        let mut config = native();
        config[key] = value;
        let error = parse(&config)
            .err()
            .expect("bad schema accepted")
            .to_string();
        assert!(!error.contains("must-not-be-printed"));
    }
}

#[test]
fn credentials_are_explicit_and_reject_missing_empty_or_header_injection() {
    for value in [
        None,
        Some(""),
        Some(" "),
        Some("secret\r\ninjected"),
        Some("secret\0"),
    ] {
        for config in [native(), dsh()] {
            assert!(EndpointConfiguration::parse(&config, |_| value.map(str::to_owned)).is_err());
        }
    }
    EndpointConfiguration::parse(&native(), |name| {
        assert_eq!(name, "DEMO_TOKEN");
        Some("synthetic-only".into())
    })
    .unwrap();
    let mut bearer = dsh();
    bearer.as_object_mut().unwrap().remove("cookieEnv");
    bearer["bearerEnv"] = json!("DSH_TOKEN");
    assert!(parse(&bearer).is_ok());
    bearer["cookieEnv"] = json!("DSH_COOKIE");
    assert!(parse(&bearer).is_err());
    bearer.as_object_mut().unwrap().remove("cookieEnv");
    bearer.as_object_mut().unwrap().remove("bearerEnv");
    assert!(
        parse(&bearer).is_ok(),
        "an explicitly unauthenticated host may be configured"
    );
}

#[test]
fn pi_rejects_implicit_paths_bad_environment_and_duplicate_secret_targets() {
    for (key, value) in [
        ("executable", json!("node")),
        ("dataRoot", json!("data")),
        ("sessionRoots", json!(["relative"])),
        ("extensions", json!(["relative"])),
        ("arguments", json!([42])),
        ("environment", json!({"BAD=NAME":"x"})),
        ("environment", json!({"HOME":"bad\0value"})),
        ("environmentFromEnv", json!({"HOME":"HOST_HOME"})),
    ] {
        let mut config = pi();
        config[key] = value;
        assert!(parse(&config).is_err(), "accepted invalid {key}");
    }
    assert!(EndpointConfiguration::parse(&pi(), |_| None).is_err());
    EndpointConfiguration::parse(&pi(), |name| {
        assert_eq!(name, "HOST_KEY");
        Some("synthetic-only".into())
    })
    .unwrap();
}

#[test]
fn both_http_adapters_reject_credentials_in_urls() {
    for mut config in [native(), dsh()] {
        for url in [
            "file:///tmp/session",
            "http://user:secret@localhost/",
            "http://localhost/?token=secret",
            "http://localhost/#secret",
        ] {
            config["url"] = json!(url);
            assert!(parse(&config).is_err());
        }
    }
}
