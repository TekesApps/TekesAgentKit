//! Normalize public Pi lifecycle identities and Pi 1.3 delta-only RPC updates.
use serde_json::{Value, json};
use tekes_agent_contract::{Result, require, string};
#[derive(Default)]
pub(crate) struct ReadingTracker {
    announced: Option<String>,
    active: Option<String>,
    partial: Option<Value>,
}
impl ReadingTracker {
    pub fn receive(&mut self, mut value: Value, token: &str) -> Result<Option<Value>> {
        if value["type"] == "extension_ui_request"
            && value["method"] == "notify"
            && let Some(message) = value["message"]
                .as_str()
                .and_then(|text| text.strip_prefix("agentkit-reading/v1:"))
        {
            let Ok(notice) = serde_json::from_str::<Value>(message) else {
                return Ok(None);
            };
            if notice["token"].as_str() != Some(token) {
                return Ok(None);
            }
            let id = string(&notice, "streamId")?;
            require(!id.is_empty(), "empty Pi stream identity")?;
            if let Some(message) = notice.get("message") {
                require(
                    message["role"] == "assistant",
                    "invalid Pi assistant partial",
                )?;
                self.partial = (self.active.as_deref() == Some(id)).then(|| message.clone());
            } else {
                self.announced = Some(id.into());
            }
            return Ok(None);
        }
        if matches!(
            value["type"].as_str(),
            Some("agent_start" | "session_switch" | "session_fork")
        ) {
            self.announced = None;
            self.active = None;
            self.partial = None;
        }
        if value["type"] == "message_update" {
            if let Some(partial) = self.partial.take()
                && value.get("message").is_none()
            {
                value["message"] = partial;
            }
        } else if matches!(
            value["type"].as_str(),
            Some("message_start" | "message_end")
        ) {
            self.partial = None;
        }
        if value["message"]["role"] == "assistant" {
            if value["type"] == "message_start" {
                self.active = self.announced.take();
            }
            if matches!(
                value["type"].as_str(),
                Some("message_start" | "message_update" | "message_end")
            ) && let Some(id) = &self.active
            {
                value["agentkitStreamId"] = json!(id);
            }
        }
        Ok(Some(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn notice(token: &str, id: &str) -> Value {
        json!({"type":"extension_ui_request","method":"notify","message":format!("agentkit-reading/v1:{}",json!({"token":token,"streamId":id}))})
    }
    fn event(kind: &str) -> Value {
        json!({"type":kind,"message":{"role":"assistant","timestamp":1,"content":[]}})
    }
    #[test]
    fn authenticated_notice_is_consumed_and_only_its_lifecycle_is_tagged() {
        let mut tracker = ReadingTracker::default();
        assert!(
            tracker
                .receive(notice("secret", "one"), "secret")
                .unwrap()
                .is_none()
        );
        for kind in ["message_start", "message_update", "message_end"] {
            let output = tracker.receive(event(kind), "secret").unwrap().unwrap();
            assert_eq!(output["agentkitStreamId"], "one");
            assert!(!output.to_string().contains("secret"));
        }
        // Equal timestamp/content on a later start does not reuse the receipt.
        assert!(
            tracker
                .receive(event("message_start"), "secret")
                .unwrap()
                .unwrap()
                .get("agentkitStreamId")
                .is_none()
        );
    }
    #[test]
    fn invalid_notices_and_resets_cannot_bind_a_preview() {
        let mut tracker = ReadingTracker::default();
        assert!(
            tracker
                .receive(notice("wrong", "other"), "secret")
                .unwrap()
                .is_none()
        );
        assert!(tracker.receive(json!({"type":"extension_ui_request","method":"notify","message":"agentkit-reading/v1:{"}), "secret").unwrap().is_none());
        assert!(
            tracker
                .receive(event("message_start"), "secret")
                .unwrap()
                .unwrap()
                .get("agentkitStreamId")
                .is_none()
        );
        for reset in ["agent_start", "session_switch", "session_fork"] {
            tracker.receive(notice("secret", "one"), "secret").unwrap();
            tracker.receive(json!({"type":reset}), "secret").unwrap();
            assert!(
                tracker
                    .receive(event("message_start"), "secret")
                    .unwrap()
                    .unwrap()
                    .get("agentkitStreamId")
                    .is_none()
            );
        }
    }
    #[test]
    fn pi_13_delta_only_events_use_only_the_authenticated_public_partial() {
        let mut tracker = ReadingTracker::default();
        tracker.receive(notice("secret", "one"), "secret").unwrap();
        tracker.receive(event("message_start"), "secret").unwrap();
        let delta = json!({"type":"message_update","assistantMessageEvent":{"type":"text_delta","contentIndex":0,"delta":"中文"}});
        assert!(
            tracker
                .receive(delta.clone(), "secret")
                .unwrap()
                .unwrap()
                .get("message")
                .is_none()
        );
        let mut update = notice("secret", "one");
        update["message"] = json!(format!(
            "agentkit-reading/v1:{}",
            json!({"token":"secret","streamId":"one","message":{"role":"assistant","content":[{"type":"text","text":"中文"}]}})
        ));
        assert!(tracker.receive(update, "secret").unwrap().is_none());
        let normalized = tracker.receive(delta.clone(), "secret").unwrap().unwrap();
        assert_eq!(normalized["agentkitStreamId"], "one");
        assert_eq!(normalized["message"]["content"][0]["text"], "中文");
        assert!(
            tracker
                .receive(delta, "secret")
                .unwrap()
                .unwrap()
                .get("message")
                .is_none()
        );
    }
}
