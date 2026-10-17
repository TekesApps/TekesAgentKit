use serde_json::Value;
use tekes_agent_contract::*;
#[test]
fn same_wire_cases_as_current_clientkit() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("fixtures/parity.json")).unwrap();
    for case in cases {
        let value = case["value"].clone();
        let result: Result<()> = (|| match case["kind"].as_str().unwrap() {
            "host" => serde_json::from_value::<HostDescription>(value)?.validate_kernel(),
            "event" => serde_json::from_value::<SessionEvent>(value)?.validate(),
            "snapshot" => serde_json::from_value::<JournalSnapshot>(value)?.validate(),
            "page" => serde_json::from_value::<HistoryPage>(value)?
                .validate(&serde_json::from_value(case["request"].clone())?),
            _ => panic!("unknown fixture"),
        })();
        assert_eq!(
            result.is_ok(),
            case["valid"].as_bool().unwrap(),
            "{}: {result:?}",
            case["name"]
        );
    }
}
#[test]
fn unknown_ignorable_metadata_roundtrips_and_title_null_is_preserved() {
    let value = serde_json::json!({"type":"future/event","seq":1,"time":1.0,"data":null,"ignorable":true,"surfaceOp":{"opaque":"value"},"sourceEventSeqs":"opaque"});
    let event: SessionEvent = serde_json::from_value(value.clone()).unwrap();
    event.validate().unwrap();
    assert_eq!(serde_json::to_value(event).unwrap(), value);
    let mut value = serde_json::json!({"asOfSeq":1,"values":{"sessionTitle":null}});
    normalize_title(&mut value);
    assert!(value["values"].get("title").unwrap().is_null());
    assert!(value["values"].get("sessionTitle").is_none());
}
