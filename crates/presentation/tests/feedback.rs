use tekes_agent_contract::{SessionMessageFeedback, SessionMessageFeedbackRating as Rating};
use tekes_agent_presentation::{
    FeedbackChange as Change, FeedbackWritePlan as Plan, plan_feedback_change,
};
fn current(note: Option<&str>) -> SessionMessageFeedback {
    SessionMessageFeedback {
        message_id: "m".into(),
        rating: Rating::Positive,
        version: "opaque:9".into(),
        note: note.map(String::from),
        updated_at_milliseconds: None,
    }
}
#[test]
fn rating_toggle_deletes_same_choice_and_replaces_judgment_including_old_note() {
    let record = current(Some("old note"));
    assert_eq!(
        plan_feedback_change("m", Some(&record), Change::ToggleRating(Rating::Positive)).unwrap(),
        Plan::Delete {
            message_id: "m".into(),
            if_version: "opaque:9".into()
        }
    );
    let plan =
        plan_feedback_change("m", Some(&record), Change::ToggleRating(Rating::Negative)).unwrap();
    let payload = plan.payload("s").unwrap();
    assert_eq!(payload["rating"], "negative");
    assert!(payload["note"].is_null());
    assert_eq!(payload["ifVersion"], "opaque:9");
    let create = plan_feedback_change("m", None, Change::ToggleRating(Rating::Positive))
        .unwrap()
        .payload("s")
        .unwrap();
    assert!(create["ifVersion"].is_null());
}
#[test]
fn setting_rating_preserves_note_but_note_removal_preserves_rating() {
    let record = current(Some("中文备注"));
    let rating = plan_feedback_change(
        "m",
        Some(&record),
        Change::SetRating(Some(Rating::Negative)),
    )
    .unwrap()
    .payload("s")
    .unwrap();
    assert_eq!(rating["note"], "中文备注");
    let note = plan_feedback_change("m", Some(&record), Change::SetNote(None))
        .unwrap()
        .payload("s")
        .unwrap();
    assert_eq!(note["rating"], "positive");
    assert!(note["note"].is_null());
    assert_eq!(note["ifVersion"], "opaque:9");
    let empty = plan_feedback_change("m", Some(&record), Change::SetNote(Some("".into())))
        .unwrap()
        .payload("s")
        .unwrap();
    assert_eq!(empty["note"], "");
}
#[test]
fn missing_rating_is_not_created_by_note_edit_and_clear_noops_do_not_send() {
    assert!(plan_feedback_change("m", None, Change::SetNote(Some("note".into()))).is_err());
    for change in [Change::SetRating(None), Change::SetNote(None)] {
        assert_eq!(
            plan_feedback_change("m", None, change).unwrap(),
            Plan::Unchanged
        );
    }
    assert_eq!(
        plan_feedback_change("m", Some(&current(None)), Change::SetNote(None)).unwrap(),
        Plan::Unchanged
    );
    assert!(
        plan_feedback_change(
            "other",
            Some(&current(None)),
            Change::ToggleRating(Rating::Positive)
        )
        .is_err()
    );
}
