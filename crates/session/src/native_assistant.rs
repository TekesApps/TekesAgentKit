//! Current public assistant lifecycle, independent of the bounded event log.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NativeAssistantProjection {
    Idle,
    Message { event: Value, reasoning_ended: bool },
}
impl NativeAssistantProjection {
    pub(crate) fn receive(current: Option<&Self>, event: &Value) -> Option<Self> {
        let kind = event["type"].as_str()?;
        if matches!(kind, "agent_start" | "session_switch" | "session_fork") {
            return Some(Self::Idle);
        }
        if event["message"]["role"] != "assistant" {
            return None;
        }
        let previous = match current {
            Some(Self::Message {
                event,
                reasoning_ended,
            }) => Some((event, *reasoning_ended)),
            _ => None,
        };
        let same = previous.is_some_and(|(old, _)| {
            old["agentkitStreamId"].as_str() == event["agentkitStreamId"].as_str()
        });
        // A tagged current lifecycle can only be replaced by message_start,
        // never by a delayed update belonging to another/unknown lifecycle.
        if kind == "message_update"
            && previous.is_some_and(|(old, _)| old["agentkitStreamId"].as_str().is_some())
            && !same
        {
            return None;
        }
        if kind == "message_end" {
            if previous.is_some() && !same {
                return None;
            }
            return Some(Self::Idle);
        }
        if !matches!(kind, "message_start" | "message_update")
            || event["message"].get("content").is_none()
        {
            return None;
        }
        let prior_ended =
            kind != "message_start" && same && previous.is_some_and(|(_, ended)| ended);
        let delta = event["assistantMessageEvent"]["type"]
            .as_str()
            .unwrap_or("");
        let ended = if matches!(delta, "thinking_start" | "thinking_delta") {
            false
        } else if delta == "thinking_end"
            || delta.starts_with("text_")
            || delta.starts_with("toolcall_")
            || matches!(delta, "done" | "error")
        {
            true
        } else {
            prior_ended
        };
        Some(Self::Message {
            event: event.clone(),
            reasoning_ended: ended,
        })
    }
}
