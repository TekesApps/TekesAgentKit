//! Built-in Native bounded child projection through the public, read-only SEP method.
use crate::*;
use std::{collections::BTreeMap, sync::Arc};
use tekes_agent_contract::*;
use tekes_agent_session::{SessionProjection, SessionRuntime};
pub(crate) struct BoundedHistory {
    runtime: Arc<SessionRuntime>,
    parent: RunScope,
    child: String,
    page_size: usize,
    page: BoundedChildHistoryPage,
    entries: BTreeMap<i64, HistoryEntry>,
    #[cfg(feature = "acceptance-source-witness")]
    latest_head: Option<(BoundedChildHistoryRequest, BoundedChildHistoryPage)>,
}
impl BoundedHistory {
    pub(crate) async fn open(
        runtime: Arc<SessionRuntime>,
        parent: RunScope,
        child: String,
        page_size: usize,
    ) -> Result<Self> {
        let head_request = Self::request(&parent, &child, None, None, None, page_size);
        #[cfg(feature = "acceptance-source-witness")]
        let initial_head_request = head_request.clone();
        let page = runtime.bounded_child_history(head_request).await?;
        let entries = page
            .entries
            .iter()
            .map(|e| (e.event.seq, e.clone()))
            .collect();
        Ok(Self {
            runtime,
            parent,
            child,
            page_size,
            #[cfg(feature = "acceptance-source-witness")]
            latest_head: Some((initial_head_request, page.clone())),
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
        page_size: usize,
    ) -> BoundedChildHistoryRequest {
        BoundedChildHistoryRequest {
            parent_session_id: parent.session_id.clone(),
            child_session_id: child.into(),
            through_sequence: cut,
            before_sequence: before,
            max_messages: page_size,
            history_revision: revision,
        }
    }
    pub(crate) async fn refresh(&mut self) -> Result<TranscriptSnapshot> {
        #[cfg(feature = "acceptance-source-witness")]
        let snapshot = self.refresh_inner(false).await?;
        #[cfg(not(feature = "acceptance-source-witness"))]
        let snapshot = self.refresh_inner().await?;
        Ok(snapshot)
    }

    #[cfg(feature = "acceptance-source-witness")]
    pub(crate) async fn refresh_with_source_witness(
        &mut self,
    ) -> Result<(TranscriptSnapshot, crate::BoundedChildSourceWitness)> {
        let snapshot = self.refresh_inner(true).await?;
        let (head_request, raw_head) = self.latest_head.clone().ok_or_else(|| {
            Error::Protocol("bounded head source witness was not retained".into())
        })?;
        Ok((
            snapshot,
            crate::BoundedChildSourceWitness {
                parent_scope: self.parent.clone(),
                child_id: self.child.clone(),
                page_size: self.page_size,
                head_request,
                raw_head,
                evidence_scope: "built-in-bounded-head-only-same-local-read",
                server_transactional_freshness_proven: false,
            },
        ))
    }

    async fn refresh_inner(
        &mut self,
        #[cfg(feature = "acceptance-source-witness")] require_head_only: bool,
    ) -> Result<TranscriptSnapshot> {
        let epoch = self.runtime.inspect(|s| s.connection_epoch);
        let head_request =
            Self::request(&self.parent, &self.child, None, None, None, self.page_size);
        #[cfg(feature = "acceptance-source-witness")]
        let original_head_request = head_request.clone();
        let next = self.runtime.bounded_child_history(head_request).await?;
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
                    self.page_size,
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
        #[cfg(feature = "acceptance-source-witness")]
        if require_head_only {
            // New head rows overwrite every overlap during the original merge.
            // Extra retained keys are the only possible rows outside that head.
            // Validate without cloning the accumulated history on normal reads.
            let has_extra_retained_rows = prefix_preserved
                && self.entries.keys().any(|sequence| {
                    next.entries
                        .binary_search_by_key(sequence, |entry| entry.event.seq)
                        .is_err()
                });
            if has_extra_retained_rows || more != next.has_more_before {
                return Err(unsupported(
                    "bounded source witness requires an unaggregated head-only projection",
                ));
            }
        }
        #[cfg(feature = "acceptance-source-witness")]
        let raw_head = next.clone();
        if !prefix_preserved {
            self.entries.clear();
        }
        self.entries.extend(
            next.entries
                .iter()
                .map(|entry| (entry.event.seq, entry.clone())),
        );
        self.page = next;
        self.page.has_more_before = more;
        let snapshot = self.transcript()?;
        #[cfg(feature = "acceptance-source-witness")]
        {
            self.latest_head = Some((original_head_request, raw_head));
        }
        Ok(snapshot)
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
                self.page_size,
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
                    status: match phase {
                        "running" => "running",
                        "error" => "error",
                        "stopped" => "aborted",
                        _ => "completed",
                    }
                    .into(),
                    input: String::new(),
                    started_at: None,
                    ended_at: None,
                    usage: TokenUsage::default(),
                    fork_boundary: None,
                    footnote_actions: Vec::new(),
                    live_usage: None,
                    reported_usage: None,
                },
            );
            transcript.tools.insert(
                0,
                TranscriptToolCall {
                    result_meta: None,
                    result_error: None,
                    id: format!("{id}:call"),
                    turn_id: id,
                    name: source["name"].as_str().unwrap_or("task").into(),
                    arguments: source["arguments"].clone(),
                    result: None,
                    phase: phase.into(),
                    call_view: None,
                    result_view: None,
                    source_position: None,
                    result_position: None,
                },
            );
        }
        transcript.is_stale = self
            .runtime
            .inspect(|s| !matches!(s.connection, tekes_agent_session::ConnectionState::Ready));
        Ok(transcript)
    }
}
