//! Built-in Native bounded child projection through the public, read-only SEP method.
use crate::*;
use std::{collections::BTreeMap, sync::Arc};
use tekes_agent_contract::*;
use tekes_agent_session::{SessionProjection, SessionRuntime};
pub(crate) struct BoundedHistory {
    runtime: Arc<SessionRuntime>,
    parent: RunScope,
    child: String,
    page: BoundedChildHistoryPage,
    entries: BTreeMap<i64, HistoryEntry>,
}
impl BoundedHistory {
    pub(crate) async fn open(
        runtime: Arc<SessionRuntime>,
        parent: RunScope,
        child: String,
    ) -> Result<Self> {
        let page = runtime
            .bounded_child_history(Self::request(&parent, &child, None, None, None))
            .await?;
        let entries = page
            .entries
            .iter()
            .map(|e| (e.event.seq, e.clone()))
            .collect();
        Ok(Self {
            runtime,
            parent,
            child,
            page,
            entries,
        })
    }
    fn request(
        parent: &RunScope,
        child: &str,
        cut: Option<i64>,
        before: Option<i64>,
        revision: Option<String>,
    ) -> BoundedChildHistoryRequest {
        BoundedChildHistoryRequest {
            parent_session_id: parent.session_id.clone(),
            child_session_id: child.into(),
            through_sequence: cut,
            before_sequence: before,
            max_messages: 50,
            history_revision: revision,
        }
    }
    pub(crate) async fn refresh(&mut self) -> Result<TranscriptSnapshot> {
        let epoch = self.runtime.inspect(|s| s.connection_epoch);
        let next = self
            .runtime
            .bounded_child_history(Self::request(&self.parent, &self.child, None, None, None))
            .await?;
        // Retain loaded older pages only after the server re-verifies the previous frozen prefix.
        let same = self.page.through_sequence == next.through_sequence
            && self.page.history_revision == next.history_revision;
        let prefix_preserved = if same {
            true
        } else if next.through_sequence > self.page.through_sequence {
            match self
                .runtime
                .bounded_child_history(Self::request(
                    &self.parent,
                    &self.child,
                    Some(self.page.through_sequence),
                    None,
                    Some(self.page.history_revision.clone()),
                ))
                .await
            {
                Ok(_) => true,
                Err(Error::Remote(value)) if value["code"] == "history-stale" => false,
                Err(error) => return Err(error),
            }
        } else {
            false
        } && next
            .entries
            .first()
            .is_none_or(|entry| entry.event.seq <= self.page.through_sequence.saturating_add(1));
        // If the child advanced beyond one window, reset to the authoritative head. Keeping
        // old pages across a gap would silently omit the intervening transcript; pagination
        // can recover it from the new frozen cut.
        require(
            self.runtime.inspect(|s| s.connection_epoch == epoch),
            "bounded history connection changed during refresh",
        )?;
        let more = if prefix_preserved
            && self
                .entries
                .keys()
                .next()
                .zip(next.entries.first())
                .is_some_and(|(old, new)| *old < new.event.seq)
        {
            self.page.has_more_before
        } else {
            next.has_more_before
        };
        if !prefix_preserved {
            self.entries.clear();
        }
        self.entries
            .extend(next.entries.iter().map(|e| (e.event.seq, e.clone())));
        self.page = next;
        self.page.has_more_before = more;
        self.transcript()
    }
    pub(crate) async fn load_earlier(&mut self) -> Result<bool> {
        if !self.page.has_more_before {
            return Ok(false);
        }
        let before = self
            .entries
            .keys()
            .next()
            .copied()
            .ok_or_else(|| Error::Protocol("empty bounded history frontier".into()))?;
        let page = self
            .runtime
            .bounded_child_history(Self::request(
                &self.parent,
                &self.child,
                Some(self.page.through_sequence),
                Some(before),
                Some(self.page.history_revision.clone()),
            ))
            .await?;
        require(!page.entries.is_empty(), "bounded history made no progress")?;
        self.entries
            .extend(page.entries.iter().map(|e| (e.event.seq, e.clone())));
        self.page.has_more_before = page.has_more_before;
        Ok(true)
    }
    fn transcript(&self) -> Result<TranscriptSnapshot> {
        let scope = RunScope {
            endpoint_id: self.parent.endpoint_id.clone(),
            thread_id: self.parent.thread_id.clone(),
            session_id: self.child.clone(),
        };
        let session = SessionProjection {
            entries: self.entries.clone(),
            journal_ready: true,
            run_open: self.page.run_open,
            has_more_before: self.page.has_more_before,
            tail_sequence: self.page.through_sequence,
            ..Default::default()
        };
        let mut transcript = TranscriptSnapshot::read(&scope, &session)?;
        if let Some(source) = &self.page.source_call {
            let id = serde_json::json!([
                scope.endpoint_id,
                scope.session_id,
                "task-source",
                source["callId"]
            ])
            .to_string();
            let phase = source["phase"].as_str().unwrap_or("running");
            transcript.turns.insert(
                0,
                TranscriptTurn {
                    id: id.clone(),
                    status: if phase == "running" {
                        "running"
                    } else {
                        "completed"
                    }
                    .into(),
                    input: String::new(),
                    started_at: None,
                    ended_at: None,
                    usage: TokenUsage::default(),
                },
            );
            transcript.tools.insert(
                0,
                TranscriptToolCall {
                    id: format!("{id}:call"),
                    turn_id: id,
                    name: source["name"].as_str().unwrap_or("task").into(),
                    arguments: source["arguments"].clone(),
                    result: None,
                    phase: phase.into(),
                    call_view: None,
                    result_view: None,
                },
            );
        }
        transcript.is_stale = self
            .runtime
            .inspect(|s| !matches!(s.connection, tekes_agent_session::ConnectionState::Ready));
        Ok(transcript)
    }
}
