use serde_json::{Value, json};
use tekes_agent_presentation::TranscriptToolCall;
fn tool() -> TranscriptToolCall {
    TranscriptToolCall {
        id: "owned-call".into(),
        turn_id: "owned-turn".into(),
        name: "read".into(),
        arguments: json!({}),
        result: None,
        phase: "doing".into(),
        call_view: None,
        result_view: None,
        source_position: None,
        result_position: None,
    }
}
#[test]
fn nonterminal_producer_result_precedes_call_without_reading_output_or_arbitrary_arguments() {
    let mut t = tool();
    t.arguments = json!({"secret":"must not become the title"});
    t.result = Some(json!({"text":"stdout is not the action"}));
    assert_eq!(t.presentation_title(), "read");
    t.call_view = Some(json!({"for":"call","view":{"title":"  Read\n a.rs  "}}));
    assert_eq!(t.presentation_title(), "Read a.rs");
    t.result_view = Some(json!({"for":"result","view":{"title":" Read a.rs · 12 lines "}}));
    let before = t.clone();
    assert_eq!(t.presentation_title(), "Read a.rs · 12 lines");
    assert_eq!(t, before, "derivation must not rewrite source facts");
}
#[test]
fn terminal_prefers_call_description_then_argument_description_over_command_and_result() {
    let mut t = tool();
    t.name = "shell.run".into();
    t.call_view = Some(
        json!({"for":"call","view":{"card":"terminal","title":"command\nhere doc body","description":"  Check\n workspace "}}),
    );
    t.arguments = json!({"description":"argument description","command":"fallback\nsecret body"});
    t.result_view = Some(json!({"for":"result","view":{"title":"stdout title"}}));
    assert_eq!(t.presentation_title(), "Check workspace");
    t.call_view.as_mut().unwrap()["view"]["description"] = Value::Null;
    assert_eq!(t.presentation_title(), "argument description");
    t.arguments["description"] = json!(" \n ");
    assert_eq!(t.presentation_title(), "command");
}
#[test]
fn terminal_command_fallback_takes_first_nonempty_newline_segment_without_flattening_body() {
    let mut t = tool();
    t.name = "shell".into();
    t.call_view = Some(
        json!({"for":"call","view":{"card":"terminal","title":"\n\r\n echo 中文\nnot part of title"}}),
    );
    assert_eq!(t.presentation_title(), "echo 中文");
    t.call_view.as_mut().unwrap()["view"]["title"] = Value::Null;
    t.arguments = json!({"command":"\u{2028}echo first\u{2029}not the title"});
    assert_eq!(t.presentation_title(), "echo first");
}
#[test]
fn whitespace_first_terminal_line_falls_through_instead_of_selecting_a_later_command_line() {
    let mut t = tool();
    t.call_view =
        Some(json!({"for":"call","view":{"card":"terminal","title":" \nshould not win"}}));
    t.arguments = json!({"command":"fallback\nbody"});
    assert_eq!(t.presentation_title(), "fallback");
}
#[test]
fn terminal_encoded_argument_json_matches_object_input_and_malformed_json_stays_unknown() {
    let mut t = tool();
    t.call_view = Some(json!({"for":"call","view":{"card":"terminal"}}));
    t.arguments = json!(r#"{"description":"Run checks","command":"cargo test"}"#);
    assert_eq!(t.presentation_title(), "Run checks");
    t.arguments = json!(r#"{"command":"unfinished"#);
    assert_eq!(t.presentation_title(), "read");
}
#[test]
fn wrong_or_future_envelopes_and_nonstring_titles_never_supply_display_authority() {
    let mut t = tool();
    for call in [
        json!({"for":"result","view":{"title":"wrong slot"}}),
        json!({"for":"future","view":{"title":"future"}}),
        json!({"title":"untyped"}),
        json!({"for":"call","view":{"title":7}}),
        json!({"for":"call","view":null}),
    ] {
        t.call_view = Some(call);
        assert_eq!(t.presentation_title(), "read");
    }
    t.result_view = Some(json!({"for":"call","view":{"title":"wrong result slot"}}));
    assert_eq!(t.presentation_title(), "read");
}
#[test]
fn blank_titles_fall_back_to_normalized_name_with_distinct_empty_name_preparing_fallback() {
    let mut t = tool();
    t.name = "  custom_tool \t action  ".into();
    t.call_view = Some(json!({"for":"call","view":{"title":" \n "}}));
    assert_eq!(t.presentation_title(), "custom_tool action");
    t.name.clear();
    assert_eq!(t.presentation_title(), "Preparing");
    t.name = " \t ".into();
    assert_eq!(t.presentation_title(), "Tool");
}
