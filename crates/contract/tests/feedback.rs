use serde_json::json;
use tekes_agent_contract::{SessionMessageFeedbackList, SessionMessageFeedbackRating};

#[test]
fn feedback_keeps_committed_versions_notes_and_normalizes_supported_rating_aliases() {
    let list = SessionMessageFeedbackList::read(json!({"items":[
        {"messageId":"a","rating":"up","version":"opaque:9","note":"中文备注"},
        {"messageId":"b","rating":"negative","version":"opaque:2","note":"","updatedAtMilliseconds":1234}
    ]})).unwrap();
    assert_eq!(
        list.get("a").unwrap().rating,
        SessionMessageFeedbackRating::Positive
    );
    assert_eq!(list.get("a").unwrap().version, "opaque:9");
    assert_eq!(list.get("a").unwrap().note.as_deref(), Some("中文备注"));
    assert_eq!(list.get("b").unwrap().note.as_deref(), Some(""));
    assert_eq!(
        serde_json::to_value(list.get("a").unwrap()).unwrap()["rating"],
        "positive"
    );
    assert!(list.get("not-present").is_none());
    assert!(
        SessionMessageFeedbackList::read(json!({"items":[]}))
            .unwrap()
            .items
            .is_empty()
    );
}
#[test]
fn malformed_or_ambiguous_feedback_never_becomes_an_empty_success() {
    for bad in [
        json!({}),
        json!({"items":null}),
        json!({"items":[{}]}),
        json!({"items":[{"messageId":"a","rating":"unknown","version":"v"}]}),
        json!({"items":[{"messageId":"a","rating":"positive","version":""}]}),
        json!({"items":[{"messageId":"","rating":"negative","version":"v"}]}),
        json!({"items":[{"messageId":"a","rating":"positive","version":"v","note":12}]}),
        json!({"items":[{"messageId":"a","rating":"positive","version":"v"},{"messageId":"a","rating":"negative","version":"v2"}]}),
    ] {
        assert!(SessionMessageFeedbackList::read(bad).is_err());
    }
}
