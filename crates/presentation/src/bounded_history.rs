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
    #[cfg(feature = "acceptance-source-witness")]
    source_chain: SourceChain,
}

#[cfg(feature = "acceptance-source-witness")]
#[derive(Clone)]
struct SourceNode {
    previous: Option<Arc<SourceNode>>,
    receipt: crate::BoundedChildPagedSourceReceipt,
}

#[cfg(feature = "acceptance-source-witness")]
#[derive(Clone, Default)]
struct SourceChain {
    tail: Option<Arc<SourceNode>>,
    receipts: usize,
    encoded_bytes: usize,
    unavailable: Option<&'static str>,
}

#[cfg(feature = "acceptance-source-witness")]
impl SourceChain {
    const MAX_RECEIPTS: usize = 128;
    const MAX_ENCODED_BYTES: usize = 8 * 1024 * 1024;

    fn append(&mut self, receipt: crate::BoundedChildPagedSourceReceipt) {
        if receipt.reset {
            *self = Self::default();
        }
        if self.unavailable.is_some() {
            return;
        }
        let encoded_size = match serde_json::to_vec(&receipt) {
            Ok(bytes) => bytes.len() + if self.receipts == 0 { 2 } else { 1 },
            Err(_) => {
                self.unavailable = Some("a bounded history receipt could not be encoded");
                return;
            }
        };
        if self.receipts >= Self::MAX_RECEIPTS
            || self.encoded_bytes.saturating_add(encoded_size) > Self::MAX_ENCODED_BYTES
        {
            self.unavailable = Some("bounded history source receipt limit exceeded");
            return;
        }
        self.encoded_bytes += encoded_size;
        self.receipts += 1;
        self.tail = Some(Arc::new(SourceNode {
            previous: self.tail.take(),
            receipt,
        }));
    }

    fn freeze(&self) -> Result<Vec<crate::BoundedChildPagedSourceReceipt>> {
        if let Some(reason) = self.unavailable {
            return Err(unsupported(reason));
        }
        let mut receipts = Vec::with_capacity(self.receipts);
        let mut node = self.tail.as_deref();
        while let Some(current) = node {
            receipts.push(current.receipt.clone());
            node = current.previous.as_deref();
        }
        receipts.reverse();
        require(
            receipts.len() == self.receipts,
            "incomplete bounded source chain",
        )?;
        Ok(receipts)
    }
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
        #[cfg(feature = "acceptance-source-witness")]
        let initial_epoch = runtime.inspect(|s| s.connection_epoch);
        let page = runtime.bounded_child_history(head_request).await?;
        let entries = page
            .entries
            .iter()
            .map(|e| (e.event.seq, e.clone()))
            .collect();
        #[cfg(feature = "acceptance-source-witness")]
        let source_chain = {
            let mut chain = SourceChain::default();
            chain.append(crate::BoundedChildPagedSourceReceipt {
                operation: crate::BoundedChildPagedSourceOperation::InitialHead,
                read_epoch: initial_epoch,
                request: initial_head_request.clone(),
                response: crate::BoundedChildPagedSourceResponse::Page(page.clone()),
                reset: true,
                merged: true,
                effective_has_more_before: page.has_more_before,
            });
            if runtime.inspect(|store| store.connection_epoch != initial_epoch) {
                chain.unavailable = Some("initial bounded source epoch changed during read");
            }
            chain
        };
        Ok(Self {
            runtime,
            parent,
            child,
            page_size,
            #[cfg(feature = "acceptance-source-witness")]
            latest_head: Some((initial_head_request, page.clone())),
            #[cfg(feature = "acceptance-source-witness")]
            source_chain,
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

    /// Read transcript and run from the same refreshed public bounded page. The page has no
    /// child control authority, so writable run actions are explicitly disabled.
    pub(crate) async fn refresh_presentation(
        &mut self,
    ) -> Result<(TranscriptSnapshot, crate::RunPresentation)> {
        let read_epoch = self.runtime.inspect(|state| state.connection_epoch);
        let transcript = self.refresh().await?;
        let scope = RunScope {
            endpoint_id: self.parent.endpoint_id.clone(),
            thread_id: self.parent.thread_id.clone(),
            session_id: self.child.clone(),
        };
        let projected = self.runtime.inspect(|state| {
            require(
                state.connection_epoch == read_epoch,
                "bounded child connection changed during read",
            )?;
            let mut projected =
                tekes_agent_session::MemoryProjectionStore::new(&self.parent.endpoint_id);
            projected.connection = state.connection.clone();
            if state.archived_session_ids.contains(&self.child) {
                projected.archived_session_ids.push(self.child.clone());
            }
            // Copy only this child's control facts, never all retained histories.
            let control = state.sessions.get(&self.child);
            projected.sessions.insert(
                self.child.clone(),
                SessionProjection {
                    journal_ready: true,
                    run_open: self.page.run_open,
                    has_more_before: self.page.has_more_before,
                    tail_sequence: self.page.through_sequence,
                    queue: control.map(|s| s.queue.clone()).unwrap_or_default(),
                    jobs: control.map(|s| s.jobs.clone()).unwrap_or_default(),
                    ..Default::default()
                },
            );
            projected.actionables = state
                .actionables
                .iter()
                .filter(|(_, action)| action["sessionId"].as_str() == Some(self.child.as_str()))
                .map(|(id, action)| (id.clone(), action.clone()))
                .collect();
            Ok::<_, tekes_agent_contract::Error>(projected)
        })?;
        let mut run = crate::RunPresentation::read(&projected, &scope, true);
        run.can_submit = false;
        run.can_cancel = false;
        run.connection.can_submit = false;
        Ok((transcript, run))
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

    #[cfg(feature = "acceptance-source-witness")]
    pub(crate) async fn refresh_with_paged_source_witness(
        &mut self,
    ) -> Result<(TranscriptSnapshot, crate::BoundedChildPagedSourceWitness)> {
        let epoch = self.runtime.inspect(|store| store.connection_epoch);
        let snapshot = self.refresh_inner(false).await?;
        require(
            self.runtime
                .inspect(|store| store.connection_epoch == epoch),
            "bounded paged source connection changed during read",
        )?;
        let receipts = self.source_chain.freeze()?;
        self.validate_source_chain(&receipts)?;
        let witness = crate::BoundedChildPagedSourceWitness {
            parent_scope: self.parent.clone(),
            child_id: self.child.clone(),
            page_size: self.page_size,
            connection_epoch: epoch,
            receipts,
            projected_entries: self.entries.values().cloned().collect(),
            through_sequence: self.page.through_sequence,
            history_revision: self.page.history_revision.clone(),
            has_more_before: self.page.has_more_before,
            run_open: self.page.run_open,
            source_call: self.page.source_call.clone(),
            evidence_scope: "built-in-bounded-paged-same-local-read",
            server_transactional_freshness_proven: false,
        };
        // Bind the returned witness to this exact projection, including its source-call metadata.
        require(
            snapshot.has_more_before == witness.has_more_before,
            "bounded source witness projection metadata mismatch",
        )?;
        Ok((snapshot, witness))
    }

    #[cfg(feature = "acceptance-source-witness")]
    fn validate_source_chain(
        &self,
        receipts: &[crate::BoundedChildPagedSourceReceipt],
    ) -> Result<()> {
        let mut entries = BTreeMap::new();
        let mut latest_page = None;
        let current_epoch = self.runtime.inspect(|store| store.connection_epoch);
        let mut previous_epoch = 0;
        require(
            receipts.first().is_some_and(|receipt| receipt.reset),
            "bounded source chain has no complete origin",
        )?;
        for receipt in receipts {
            receipt.request.validate()?;
            require(
                receipt.request.parent_session_id == self.parent.session_id
                    && receipt.request.child_session_id == self.child
                    && receipt.request.max_messages == self.page_size
                    && receipt.read_epoch >= previous_epoch
                    && receipt.read_epoch <= current_epoch,
                "bounded source receipt origin or epoch mismatch",
            )?;
            previous_epoch = receipt.read_epoch;
            use crate::BoundedChildPagedSourceOperation as Operation;
            match receipt.operation {
                Operation::InitialHead | Operation::RefreshHead => require(
                    receipt.merged
                        && receipt.request.through_sequence.is_none()
                        && receipt.request.before_sequence.is_none()
                        && receipt.request.history_revision.is_none(),
                    "invalid bounded source head operation",
                )?,
                Operation::Earlier => require(
                    receipt.merged
                        && !receipt.reset
                        && receipt.request.before_sequence == entries.keys().next().copied()
                        && receipt.request.before_sequence.is_some()
                        && receipt.request.through_sequence.is_some()
                        && receipt.request.history_revision.is_some(),
                    "invalid bounded source earlier frontier",
                )?,
                Operation::PrefixValidation => require(
                    !receipt.merged
                        && !receipt.reset
                        && receipt.request.before_sequence.is_none()
                        && receipt.request.through_sequence.is_some()
                        && receipt.request.history_revision.is_some(),
                    "invalid bounded source prefix operation",
                )?,
            }
            match &receipt.response {
                crate::BoundedChildPagedSourceResponse::Page(page) => {
                    page.validate(&receipt.request)?;
                    if receipt.reset {
                        entries.clear();
                        latest_page = None;
                    }
                    if receipt.merged {
                        entries.extend(
                            page.entries
                                .iter()
                                .map(|entry| (entry.event.seq, entry.clone())),
                        );
                        latest_page = Some(page);
                    }
                }
                crate::BoundedChildPagedSourceResponse::RemoteError(value) => {
                    require(
                        receipt.operation
                            == crate::BoundedChildPagedSourceOperation::PrefixValidation
                            && !receipt.merged
                            && value["code"] == "history-stale",
                        "invalid bounded source validation failure",
                    )?;
                }
            }
        }
        let page = latest_page.ok_or_else(|| {
            tekes_agent_contract::Error::Protocol("bounded source chain has no merged page".into())
        })?;
        require(
            entries == self.entries,
            "bounded source receipts do not reproduce entries",
        )?;
        require(
            page.through_sequence == self.page.through_sequence
                && page.history_revision == self.page.history_revision
                && page.run_open == self.page.run_open
                && page.source_call == self.page.source_call,
            "bounded source receipts do not reproduce head metadata",
        )?;
        let last_merged = receipts
            .iter()
            .rev()
            .find(|receipt| receipt.merged)
            .ok_or_else(|| {
                tekes_agent_contract::Error::Protocol("bounded source chain has no merge".into())
            })?;
        require(
            last_merged.effective_has_more_before == self.page.has_more_before
                && last_merged.operation == crate::BoundedChildPagedSourceOperation::RefreshHead
                && last_merged.read_epoch == current_epoch,
            "bounded source receipts do not reproduce effective pagination",
        )?;
        Ok(())
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
        #[cfg(feature = "acceptance-source-witness")]
        let mut receipts = vec![crate::BoundedChildPagedSourceReceipt {
            operation: crate::BoundedChildPagedSourceOperation::RefreshHead,
            read_epoch: epoch,
            request: original_head_request.clone(),
            response: crate::BoundedChildPagedSourceResponse::Page(next.clone()),
            reset: false,
            merged: true,
            effective_has_more_before: next.has_more_before,
        }];
        // Retain loaded older pages only after the server re-verifies the previous frozen prefix.
        let same = self.page.through_sequence == next.through_sequence
            && self.page.history_revision == next.history_revision;
        let prefix_preserved = if same {
            true
        } else if next.through_sequence > self.page.through_sequence {
            let validation_request = Self::request(
                &self.parent,
                &self.child,
                Some(self.page.through_sequence),
                None,
                Some(self.page.history_revision.clone()),
                self.page_size,
            );
            match self
                .runtime
                .bounded_child_history(validation_request.clone())
                .await
            {
                Ok(validation_page) => {
                    #[cfg(not(feature = "acceptance-source-witness"))]
                    let _ = validation_page;
                    #[cfg(feature = "acceptance-source-witness")]
                    receipts.push(crate::BoundedChildPagedSourceReceipt {
                        operation: crate::BoundedChildPagedSourceOperation::PrefixValidation,
                        read_epoch: epoch,
                        request: validation_request,
                        response: crate::BoundedChildPagedSourceResponse::Page(validation_page),
                        reset: false,
                        merged: false,
                        effective_has_more_before: self.page.has_more_before,
                    });
                    true
                }
                Err(Error::Remote(value)) if value["code"] == "history-stale" => {
                    #[cfg(feature = "acceptance-source-witness")]
                    receipts.push(crate::BoundedChildPagedSourceReceipt {
                        operation: crate::BoundedChildPagedSourceOperation::PrefixValidation,
                        read_epoch: epoch,
                        request: validation_request,
                        response: crate::BoundedChildPagedSourceResponse::RemoteError(value),
                        reset: false,
                        merged: false,
                        effective_has_more_before: self.page.has_more_before,
                    });
                    false
                }
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
        {
            if !prefix_preserved {
                receipts[0].reset = true;
            }
            receipts[0].effective_has_more_before = more;
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
            for receipt in receipts {
                self.source_chain.append(receipt);
            }
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
        let request = Self::request(
            &self.parent,
            &self.child,
            Some(self.page.through_sequence),
            Some(before),
            Some(self.page.history_revision.clone()),
            self.page_size,
        );
        #[cfg(feature = "acceptance-source-witness")]
        let read_epoch = self.runtime.inspect(|s| s.connection_epoch);
        let page = self.runtime.bounded_child_history(request.clone()).await?;
        require(!page.entries.is_empty(), "bounded history made no progress")?;
        self.entries
            .extend(page.entries.iter().map(|e| (e.event.seq, e.clone())));
        self.page.has_more_before = page.has_more_before;
        #[cfg(feature = "acceptance-source-witness")]
        if self
            .runtime
            .inspect(|store| store.connection_epoch != read_epoch)
        {
            self.source_chain.unavailable =
                Some("bounded earlier source epoch changed during read");
        }
        #[cfg(feature = "acceptance-source-witness")]
        self.source_chain
            .append(crate::BoundedChildPagedSourceReceipt {
                operation: crate::BoundedChildPagedSourceOperation::Earlier,
                read_epoch,
                request,
                response: crate::BoundedChildPagedSourceResponse::Page(page),
                reset: false,
                merged: true,
                effective_has_more_before: self.page.has_more_before,
            });
        Ok(true)
    }
    fn projected_session(&self) -> SessionProjection {
        SessionProjection {
            entries: self.entries.clone(),
            journal_ready: true,
            run_open: self.page.run_open,
            has_more_before: self.page.has_more_before,
            tail_sequence: self.page.through_sequence,
            ..Default::default()
        }
    }
    fn transcript(&self) -> Result<TranscriptSnapshot> {
        let scope = RunScope {
            endpoint_id: self.parent.endpoint_id.clone(),
            thread_id: self.parent.thread_id.clone(),
            session_id: self.child.clone(),
        };
        let session = self.projected_session();
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

#[cfg(all(test, feature = "acceptance-source-witness"))]
mod source_chain_tests {
    use super::*;

    fn receipt(
        reset: bool,
        source_call: Option<serde_json::Value>,
    ) -> crate::BoundedChildPagedSourceReceipt {
        let request = BoundedChildHistoryRequest {
            parent_session_id: "parent".into(),
            child_session_id: "child".into(),
            through_sequence: None,
            before_sequence: None,
            max_messages: 20,
            history_revision: None,
        };
        let page = BoundedChildHistoryPage {
            parent_session_id: "parent".into(),
            child_session_id: "child".into(),
            through_sequence: 5,
            history_revision: "a".repeat(64),
            entries: Vec::new(),
            has_more_before: false,
            run_open: false,
            source_call,
        };
        crate::BoundedChildPagedSourceReceipt {
            operation: crate::BoundedChildPagedSourceOperation::RefreshHead,
            read_epoch: 1,
            request,
            response: crate::BoundedChildPagedSourceResponse::Page(page),
            reset,
            merged: true,
            effective_has_more_before: false,
        }
    }

    #[test]
    fn source_chain_limits_mark_witness_unavailable_and_a_true_reset_restarts_it() {
        let mut chain = SourceChain::default();
        for _ in 0..SourceChain::MAX_RECEIPTS {
            chain.append(receipt(false, None));
        }
        assert_eq!(chain.freeze().unwrap().len(), SourceChain::MAX_RECEIPTS);
        chain.append(receipt(false, None));
        assert!(chain.freeze().is_err());

        chain.append(receipt(true, None));
        assert_eq!(chain.freeze().unwrap().len(), 1);

        chain.append(receipt(
            false,
            Some(serde_json::json!(
                "x".repeat(SourceChain::MAX_ENCODED_BYTES)
            )),
        ));
        assert!(chain.freeze().is_err());
        chain.append(receipt(true, None));
        assert_eq!(chain.freeze().unwrap().len(), 1);
    }
}
