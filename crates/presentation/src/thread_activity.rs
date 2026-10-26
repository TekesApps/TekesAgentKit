//! Harness-neutral execution facts only. Group labels/order/preferences belong to consumers.
use tekes_agent_contract::ThreadActivity;
use tekes_agent_session::{ConnectionState, MemoryProjectionStore};

pub fn thread_activity(store: &MemoryProjectionStore, id: &str) -> ThreadActivity {
    let Some(session) = store.sessions.get(id) else {
        return ThreadActivity {
            status: "unknown".into(),
            is_stale: true,
        };
    };
    let summary = session.summary.as_ref();
    let raw = summary.and_then(|s| s["status"].as_str()).unwrap_or("");
    let run_state = summary.and_then(|s| s["runState"].as_str()).unwrap_or("");
    let waiting = store.actionables.values().any(|a| a["sessionId"] == id)
        || matches!(raw, "waitingForInput" | "waiting_for_input")
        || run_state == "parked";
    let running = session.run_open
        || summary.is_some_and(|s| s["running"] == true)
        || matches!(raw, "running" | "cancelling")
        || run_state == "running";
    let terminal = |value: &str| match value {
        "completed" | "succeeded" | "success" => Some("completed"),
        "failed" | "error" => Some("failed"),
        "cancelled" | "canceled" | "aborted" | "interrupted" => Some("cancelled"),
        _ => None,
    };
    // Inspect already retained terminal evidence; never hydrate every thread just to group a list.
    let history_terminal = if session.native_conversation.is_some() {
        native_failure_terminal(session, id)
    } else {
        session
            .entries
            .values()
            .rev()
            .find(|e| matches!(e.event.kind.as_str(), "turn/start" | "turn/end"))
            .filter(|e| e.event.kind == "turn/end")
            .and_then(|e| {
                e.event.data["reason"]
                    .as_str()
                    .or_else(|| e.event.data["reason"]["kind"].as_str())
            })
            .and_then(terminal)
    };
    let status = if waiting {
        "waitingForInput"
    } else if running {
        "running"
    } else if !session.queue.is_empty() {
        "queued"
    } else if let Some(terminal) = terminal(raw).or(history_terminal) {
        terminal
    } else if summary.is_some_and(|s| s["blank"] == true) || matches!(raw, "new" | "notStarted") {
        "notStarted"
    } else if summary.is_some_and(|s| s["running"].as_bool().is_some()) || raw == "idle" {
        "idle"
    } else {
        "unknown"
    };
    ThreadActivity {
        status: status.into(),
        is_stale: store.connection != ConnectionState::Ready
            || !store.inventory_ready
            || !store.control_ready
            || !store.actionables_ready
            || session.journal_error.is_some(),
    }
}

// Read bounded, already retained Native facts; do not hydrate a conversation or
// promote a generic stop to completion. Explicit run failure/cancellation follows
// ClientKit's run-local accumulation, with cancellation taking precedence.
fn native_failure_terminal(
    session: &tekes_agent_session::SessionProjection,
    session_id: &str,
) -> Option<&'static str> {
    let native = session.native_conversation.as_ref()?;
    if !session.journal_ready
        || session.journal_error.is_some()
        || session.run_open
        || native.is_streaming
        || native.session_id != session_id
        || native.generation.is_empty()
    {
        return None;
    }
    let entries = &native.entries;
    let run_marker = |entry: &serde_json::Value| {
        entry["type"] == "custom"
            && matches!(
                entry["customType"].as_str(),
                Some("agentkit.run.v1" | "tekes.run.v1")
            )
    };
    let authored_message = |entry: &serde_json::Value| {
        entry["type"] == "message"
            && matches!(
                entry["message"]["role"].as_str(),
                Some("user" | "assistant")
            )
    };
    let current = if let Some(end) = entries.iter().rposition(run_marker) {
        let settled = &entries[end]["data"];
        if settled["phase"] != "settled" {
            return None;
        }
        let run_id = settled["id"].as_str().filter(|id| !id.is_empty())?;
        let start = entries[..end].iter().rposition(run_marker)?;
        let started = &entries[start]["data"];
        if started["phase"] != "started"
            || started["id"].as_str() != Some(run_id)
            || entries[end + 1..].iter().any(authored_message)
        {
            return None;
        }
        if settled["aborted"] == true {
            return Some("cancelled");
        }
        &entries[start + 1..end]
    } else {
        // Legacy conversations have no explicit run marker. The newest authored
        // user bounds the evidence; a clipped error alone cannot identify a turn.
        let start = entries
            .iter()
            .rposition(|entry| entry["type"] == "message" && entry["message"]["role"] == "user")?;
        &entries[start + 1..]
    };
    let has_stop = |reason: &str| {
        current.iter().any(|entry| {
            entry["type"] == "message"
                && entry["message"]["role"] == "assistant"
                && entry["message"]["stopReason"] == reason
        })
    };
    if has_stop("aborted") {
        Some("cancelled")
    } else if has_stop("error") {
        Some("failed")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tekes_agent_session::SessionProjection;
    #[test]
    fn terminal_journal_evidence_does_not_treat_cancellation_as_completion() {
        let mut store = MemoryProjectionStore::new("e");
        store
            .sessions
            .insert("s".into(), SessionProjection::default());
        for (reason, expected) in [
            (json!("completed"), "completed"),
            (json!({"kind":"aborted"}), "cancelled"),
            (json!("failed"), "failed"),
            (json!(null), "unknown"),
        ] {
            let entry = serde_json::from_value(
                json!({"event":{"type":"turn/end","seq":1,"time":1,"data":{"reason":reason}}}),
            )
            .unwrap();
            store
                .sessions
                .get_mut("s")
                .unwrap()
                .entries
                .insert(1, entry);
            assert_eq!(thread_activity(&store, "s").status, expected);
        }
    }
    #[test]
    fn activity_reports_facts_without_inventing_completion_or_layout() {
        let mut store = MemoryProjectionStore::new("endpoint");
        store.connection = ConnectionState::Ready;
        store
            .sessions
            .insert("s".into(), SessionProjection::default());
        assert_eq!(thread_activity(&store, "s").status, "unknown");
        store.sessions.get_mut("s").unwrap().summary = Some(json!({"running":false}));
        assert_eq!(thread_activity(&store, "s").status, "idle");
        for (raw, expected) in [
            ("completed", "completed"),
            ("failed", "failed"),
            ("aborted", "cancelled"),
            ("waitingForInput", "waitingForInput"),
            ("new", "notStarted"),
        ] {
            store.sessions.get_mut("s").unwrap().summary = Some(json!({"status":raw}));
            assert_eq!(thread_activity(&store, "s").status, expected);
        }
        store.sessions.get_mut("s").unwrap().summary = Some(json!({"status":"completed"}));
        store
            .sessions
            .get_mut("s")
            .unwrap()
            .queue
            .push(json!({"text":"next"}));
        assert_eq!(thread_activity(&store, "s").status, "queued");
        store.sessions.get_mut("s").unwrap().run_open = true;
        assert_eq!(thread_activity(&store, "s").status, "running");
        store
            .actionables
            .insert("a".into(), json!({"sessionId":"other"}));
        assert_eq!(thread_activity(&store, "s").status, "running");
        store
            .actionables
            .insert("b".into(), json!({"sessionId":"s"}));
        assert_eq!(thread_activity(&store, "s").status, "waitingForInput");
        store.connection = ConnectionState::Stopped;
        assert!(thread_activity(&store, "s").is_stale);
    }
}
