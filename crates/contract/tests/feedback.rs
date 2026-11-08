use serde_json::json;
use tekes_agent_contract::SessionFeedbackFailure;
use tekes_agent_contract::{SessionMessageFeedbackList, SessionMessageFeedbackRating};

#[test]
fn feedback_conflicts_preserve_authoritative_current_and_absence() {
    let current = json!({"messageId":"m","rating":"negative","version":"opaque","note":"note"});
    for error in [
        json!({"code":"version-conflict","current":current}),
        json!({"code":"version-conflict","details":{"current":current}}),
    ] {
        let SessionFeedbackFailure::VersionConflict {
            current: Some(value),
        } = SessionFeedbackFailure::read(&error, "m").unwrap()
        else {
            panic!("current")
        };
        assert_eq!(value.version, "opaque");
        assert_eq!(value.note.as_deref(), Some("note"));
    }
    assert_eq!(
        SessionFeedbackFailure::read(&json!({"code":"version-conflict","current":null}), "m")
            .unwrap(),
        SessionFeedbackFailure::VersionConflict { current: None }
    );
    assert_eq!(
        SessionFeedbackFailure::read(&json!({"code":"rejected"}), "m").unwrap(),
        SessionFeedbackFailure::Rejected {
            code: "rejected".into()
        }
    );
}
#[test]
fn incomplete_foreign_or_contradictory_conflicts_are_not_fabricated_absence() {
    for error in [
        json!({"code":"version-conflict"}),
        json!({"code":"version-conflict","current":{"messageId":"other","rating":"positive","version":"v"}}),
        json!({"code":"version-conflict","current":null,"details":{"current":{"messageId":"m","rating":"positive","version":"v"}}}),
    ] {
        assert!(SessionFeedbackFailure::read(&error, "m").is_err());
    }
}

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
