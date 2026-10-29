use serde_json::json;
use tekes_agent_contract::*;
#[test]
fn receipts_require_unique_loaded_assistant_entries() {
    let snapshot: NativeConversationSnapshot = serde_json::from_value(json!({
        "sessionId":"s","generation":"g","leafId":"a","entries":[
            {"type":"message","id":"u","message":{"role":"user","content":"input"}},
            {"type":"message","id":"a","message":{"role":"assistant","content":[]}},
            {"type":"message","id":"b","message":{"role":"assistant","content":[]}}
        ],"hasMoreBefore":false,"isStreaming":false,"state":{}
    }))
    .unwrap();
    let receipt = |stream: &str, entry: &str| NativeMessageReadingIdentity {
        stream_id: stream.into(),
        entry_id: entry.into(),
    };
    validate_native_reading_identities(&snapshot, &[receipt("live", "a")]).unwrap();
    for invalid in [
        vec![receipt("live", "missing")],
        vec![receipt("live", "u")],
        vec![receipt("", "a")],
        vec![receipt("live", "")],
        vec![receipt("live", "a"), receipt("live", "b")],
        vec![receipt("one", "a"), receipt("two", "a")],
    ] {
        assert!(
            validate_native_reading_identities(&snapshot, &invalid).is_err(),
            "{invalid:?}"
        );
    }
}
