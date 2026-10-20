//! Shared completed-turn action catalog for native and service consumers.
use crate::TranscriptSnapshot;
use tekes_agent_contract::EndpointCapabilities;
use tekes_agent_session::SessionProjection;

/// Exactly the source activity used by thread actions; an idle renderer does
/// not override an open run, queued work, or an authoritative running summary.
pub fn session_has_active_work(session: &SessionProjection) -> bool {
    session.run_open
        || !session.queue.is_empty()
        || session.summary.as_ref().is_some_and(|summary| {
            summary["running"] == true
                || matches!(
                    summary["status"].as_str(),
                    Some("running" | "waitingForInput" | "cancelling")
                )
        })
}
pub struct TurnActionContext<'a> {
    pub capabilities: &'a EndpointCapabilities,
    pub ready: bool,
    pub running: bool,
}
/// Rebuild only the supplied shared action definitions. Boundaries originate
/// from durable settlement records; no UI text, last-row or timestamp guessing.
pub fn present_turn_actions(context: &TurnActionContext<'_>, transcript: &mut TranscriptSnapshot) {
    let fork =
        crate::thread_actions::fork_action(context.capabilities, context.ready, context.running);
    for turn in &mut transcript.turns {
        turn.footnote_actions.clear();
        if context.capabilities.can_fork_completed_turn
            && turn.fork_boundary.is_some()
            && !transcript.is_stale
            && !matches!(turn.status.as_str(), "running" | "waitingForInput")
        {
            let mut action = fork.clone();
            action.id = format!("forkTurn:{}", turn.id);
            action.label = "从此回合分叉".into();
            turn.footnote_actions.push(action);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TokenUsage, TranscriptTurn};
    use serde_json::json;
    fn transcript() -> TranscriptSnapshot {
        TranscriptSnapshot {
            turns: vec![TranscriptTurn {
                id: "selected".into(),
                status: "completed".into(),
                input: String::new(),
                started_at: None,
                ended_at: None,
                usage: TokenUsage::default(),
                fork_boundary: Some(json!({"atSeq":2})),
                footnote_actions: vec![],
                live_usage: None,
                reported_usage: None,
            }],
            ..Default::default()
        }
    }
    #[test]
    fn turn_catalog_preserves_durable_boundary_and_shared_disabled_policy() {
        let mut caps = EndpointCapabilities::methods(&["session.fork"]);
        caps.can_fork_completed_turn = true;
        for (ready, running, enabled) in [
            (true, false, true),
            (false, false, false),
            (true, true, false),
        ] {
            let mut transcript = transcript();
            present_turn_actions(
                &TurnActionContext {
                    capabilities: &caps,
                    ready,
                    running,
                },
                &mut transcript,
            );
            let turn = &transcript.turns[0];
            assert_eq!(turn.fork_boundary, Some(json!({"atSeq":2})));
            assert_eq!(turn.footnote_actions[0].id, "forkTurn:selected");
            assert_eq!(turn.footnote_actions[0].enabled, enabled);
            assert_eq!(
                turn.footnote_actions[0].disabled_reason,
                crate::thread_actions::fork_action(&caps, ready, running).disabled_reason
            );
        }
        for invalid in 0..4 {
            let mut transcript = transcript();
            let mut caps = caps.clone();
            match invalid {
                0 => transcript.is_stale = true,
                1 => transcript.turns[0].fork_boundary = None,
                2 => transcript.turns[0].status = "running".into(),
                _ => caps.can_fork_completed_turn = false,
            };
            present_turn_actions(
                &TurnActionContext {
                    capabilities: &caps,
                    ready: true,
                    running: false,
                },
                &mut transcript,
            );
            assert!(transcript.turns[0].footnote_actions.is_empty());
        }
    }
    #[test]
    fn active_work_includes_queue_and_authoritative_summary() {
        let mut session = SessionProjection::default();
        assert!(!session_has_active_work(&session));
        session.queue.push(json!({}));
        assert!(session_has_active_work(&session));
        session.queue.clear();
        for status in ["running", "waitingForInput", "cancelling"] {
            session.summary = Some(json!({"status":status}));
            assert!(session_has_active_work(&session));
        }
        session.summary = Some(json!({"status":"idle"}));
        assert!(!session_has_active_work(&session));
        session.run_open = true;
        assert!(session_has_active_work(&session));
    }
}
