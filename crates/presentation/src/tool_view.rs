//! Read public tool view envelopes and settlement signals, without UI state.
use serde_json::Value;

pub(crate) fn payload<'a>(value: Option<&'a Value>, expected: &str) -> Option<&'a Value> {
    let value = value?;
    (value.get("for").and_then(Value::as_str) == Some(expected))
        .then(|| value.get("view"))
        .flatten()
}
fn exit_failed(value: &Value) -> bool {
    // ClientKit's JSON number -> Int(exactly:) admits integral values such as
    // 1.0, but not fractions, strings or a Double outside its signed 64-bit range.
    value["exitCode"].as_f64().is_some_and(|code| {
        code.is_finite()
            && code != 0.0
            && code.trunc() == code
            && ((i64::MIN as f64)..-(i64::MIN as f64)).contains(&code)
    }) || value["signal"].as_str().is_some()
}
/// ClientKit's public journal settlement facts. The Rust transcript represents
/// failed/done and interrupted as error/done and stopped, respectively. A tool
/// failure does not decide its parent turn's settlement.
pub(crate) fn result_phase(data: &Value, view: Option<&Value>) -> &'static str {
    if data["error"]["code"].as_str() == Some("interrupted") {
        return "stopped";
    }
    let message = &data["message"];
    let content_failed = message["content"]
        .as_array()
        .is_some_and(|parts| parts.iter().any(|part| part["isError"] == true));
    let meta = &data["meta"];
    let status = &meta["sessionStatus"];
    let terminal_failed = exit_failed(meta)
        || meta["timedOut"] == true
        || (status["kind"] == "exited" && exit_failed(status))
        || payload(view, "result").is_some_and(exit_failed);
    if data["error"].is_object() || message["isError"] == true || content_failed || terminal_failed
    {
        "error"
    } else {
        "done"
    }
}
