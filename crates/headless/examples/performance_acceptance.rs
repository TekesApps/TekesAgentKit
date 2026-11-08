//! Common public-API consumer for the frozen #55/#56 performance profile.
//!
//! The external runner owns provider gates, seeded dataset creation, process
//! lifetime/RSS sampling, and cold-process iteration pairing. This process only
//! drives a supplied connector and two already-seeded session pointers.
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{
    future::Future,
    io::Read,
    pin::Pin,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};
use tekes_agent_contract::*;
use tekes_agent_endpoint_dsh::{DshConfiguration, DshConnector};
use tekes_agent_endpoint_kernel::KernelConnector;
use tekes_agent_endpoint_pi::{PiConfiguration, PiConnector};
use tekes_agent_presentation::{ComposerDestination, ComposerDraft, RunController, RunScope};
use tekes_agent_session::{MemoryProjectionStore, SessionRuntime};

#[path = "support/reconnect_source.rs"]
mod reconnect_source;
use reconnect_source::{DshSourceWitness, PiFileObservation};

fn origin() -> &'static Instant {
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    ORIGIN.get_or_init(Instant::now)
}

fn monotonic_ns() -> u128 {
    origin().elapsed().as_nanos()
}

fn emit(value: Value) {
    println!("{value}");
    use std::io::Write;
    let _ = std::io::stdout().flush();
}

fn error_text(error: &Error) -> String {
    error.to_string()
}

async fn connector(config: &Value) -> Result<(String, Arc<dyn EndpointConnector>)> {
    let kind = string(config, "kind")?;
    let identity = config["identity"].as_str().unwrap_or(kind).to_owned();
    let connector: Arc<dyn EndpointConnector> = match kind {
        "native" => Arc::new(KernelConnector::new(
            &identity,
            string(config, "url")?,
            string(config, "bearer")?.to_owned(),
        )?),
        "pi" => {
            let mut pi = PiConfiguration::new(
                string(config, "executable")?.into(),
                string(config, "dataRoot")?.into(),
            );
            pi.identity = identity.clone();
            pi.arguments =
                serde_json::from_value(config.get("arguments").cloned().unwrap_or(json!([])))?;
            pi.environment =
                serde_json::from_value(config.get("environment").cloned().unwrap_or(json!({})))?;
            pi.extensions =
                serde_json::from_value(config.get("extensions").cloned().unwrap_or(json!([])))?;
            pi.session_roots =
                serde_json::from_value(config.get("sessionRoots").cloned().unwrap_or(json!([])))?;
            Arc::new(PiConnector::new(pi).await?) as Arc<dyn EndpointConnector>
        }
        "dsh" => {
            let mut dsh = DshConfiguration::new(&identity, string(config, "url")?)?;
            if let Some(cookie) = config["cookie"].as_str() {
                dsh.authorization = Some(("Cookie".into(), cookie.into()));
            } else if let Some(token) = config["bearer"].as_str() {
                dsh.authorization = Some(("Authorization".into(), format!("Bearer {token}")));
            }
            Arc::new(DshConnector::new(dsh))
        }
        _ => return Err(unsupported("unknown endpoint kind")),
    };
    Ok((identity, connector))
}

/// Shares the exact endpoint owned by SessionRuntime with the page20 sampler.
/// This avoids a second Pi/DSH subprocess and follows Runtime reconnects.
struct ObservedConnector {
    inner: Arc<dyn EndpointConnector>,
    current: Mutex<Option<Arc<dyn SessionEndpoint>>>,
}

impl ObservedConnector {
    fn current(&self) -> Result<Arc<dyn SessionEndpoint>> {
        self.current
            .lock()
            .map_err(|_| Error::Disconnected)?
            .clone()
            .ok_or(Error::Disconnected)
    }
}

impl EndpointConnector for ObservedConnector {
    fn connect<'life0, 'async_trait>(
        &'life0 self,
    ) -> Pin<Box<dyn Future<Output = Result<Arc<dyn SessionEndpoint>>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            let endpoint = self.inner.connect().await?;
            *self.current.lock().map_err(|_| Error::Disconnected)? = Some(endpoint.clone());
            Ok(endpoint)
        })
    }
}

fn usable_snapshot(store: &MemoryProjectionStore, id: &str) -> bool {
    store.connection == tekes_agent_session::ConnectionState::Ready
        && store.inventory_ready
        && store
            .sessions
            .get(id)
            .is_some_and(|session| session.journal_ready && session.summary.is_some())
}

fn record_ids(scope: &RunScope, session: &tekes_agent_session::SessionProjection) -> Vec<Value> {
    if let Some(conversation) = &session.native_conversation {
        return conversation
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                json!({
                    "recordIdentity":{"endpointId":scope.endpoint_id,"sessionId":scope.session_id,"nativeEntryId":entry["id"]},
                    "position":index,
                })
            })
            .collect();
    }
    session
        .entries
        .iter()
        .map(|(sequence, entry)| {
            json!({
                "sequence": sequence,
                "recordIdentity": {"endpointId":scope.endpoint_id,"sessionId":scope.session_id,"sequence":sequence},
                "eventType": entry.event.kind,
            })
        })
        .collect()
}

fn ordered_records(records: &[Value]) -> bool {
    if records
        .first()
        .is_some_and(|row| row.get("position").is_some())
    {
        records
            .iter()
            .enumerate()
            .all(|(index, row)| row["position"] == index)
    } else {
        records
            .windows(2)
            .all(|pair| pair[0]["sequence"].as_i64() < pair[1]["sequence"].as_i64())
    }
}

fn snapshot_sample(
    runtime: &SessionRuntime,
    scope: &RunScope,
    intent_ns: u128,
    purpose: &str,
) -> Value {
    let snapshot = runtime.snapshot();
    let session = snapshot.sessions.get(&scope.session_id);
    let usable = usable_snapshot(&snapshot, &scope.session_id);
    let observed_ns = monotonic_ns();
    let snapshot_error = if usable {
        Value::Null
    } else {
        json!(snapshot.last_error.clone().unwrap_or_else(|| {
            "projection not usable: connection, inventory, session summary, or history incomplete".into()
        }))
    };
    json!({
        "event": "snapshot",
        "purpose": purpose,
        "scope": scope,
        "epoch": snapshot.connection_epoch,
        "intentMonotonicNs": intent_ns,
        "observedMonotonicNs": observed_ns,
        "latencyNs": observed_ns.saturating_sub(intent_ns),
        "usable": usable,
        "connection": snapshot.connection,
        "inventoryReady": snapshot.inventory_ready,
        "sessionSummary": session.and_then(|s| s.summary.clone()),
        "historyThroughSequence": session.map(|s| s.history_through_sequence),
        "recordIdentities": session.map(|s| record_ids(scope, s)).unwrap_or_default(),
        "error": snapshot_error
    })
}

async fn close_open_sample(
    runtime: &SessionRuntime,
    controller: &RunController,
    scope: &RunScope,
    reason: &str,
) -> Result<Value> {
    let intent_ns = monotonic_ns();
    let before = runtime.snapshot();
    let before_records = before
        .sessions
        .get(&scope.session_id)
        .map(|session| record_ids(scope, session))
        .unwrap_or_default();
    runtime.close_session(&scope.session_id).await?;
    let reset_observed = runtime.inspect(|store| {
        store
            .sessions
            .get(&scope.session_id)
            .is_some_and(|session| !session.journal_ready)
    });
    require(
        reset_observed,
        "public close did not publish history-not-ready state",
    )?;
    controller.open(scope.clone()).await?;
    let after = runtime.snapshot();
    let after_session = after.sessions.get(&scope.session_id);
    let usable = usable_snapshot(&after, &scope.session_id);
    let after_records = after_session
        .map(|session| record_ids(scope, session))
        .unwrap_or_default();
    let publication_ns = monotonic_ns();
    require(
        usable,
        "public history reopen did not publish a usable original pointer",
    )?;
    Ok(json!({
        "event":"history_close_open_snapshot",
        "reason":reason,
        "scope":scope,
        "epoch":after.connection_epoch,
        "originalPointer":scope.address(),
        "intentMonotonicNs":intent_ns,
        "publicationMonotonicNs":publication_ns,
        "latencyNs":publication_ns.saturating_sub(intent_ns),
        "closeResetObserved":reset_observed,
        "usable":usable,
        "cacheRecordCountBefore":before_records.len(),
        "cacheRecordCountAfter":after_records.len(),
        "historyRecordIdentitiesUnchangedAcrossPublicCloseOpen":before_records == after_records,
        "recordIdentities":after_records,
        "error":Value::Null
    }))
}

async fn fresh_follow_projection(
    endpoint: &dyn SessionEndpoint,
    scope: &RunScope,
) -> Result<(MemoryProjectionStore, FrameStream, u128, u128)> {
    let start_ns = monotonic_ns();
    let mut projection = MemoryProjectionStore::new(scope.endpoint_id.clone());
    projection.begin_connection(endpoint.host().clone());
    let target = match endpoint.content_kind() {
        SessionContentKind::Journal => StreamTarget::SessionJournal {
            address: JournalAddress {
                session_id: scope.session_id.clone(),
            },
            max_messages: 20,
        },
        SessionContentKind::NativeConversation => StreamTarget::SessionConversation {
            address: JournalAddress {
                session_id: scope.session_id.clone(),
            },
            max_messages: 20,
        },
    };
    let mut frames = endpoint.follow(target).await?;
    let first = tokio::time::timeout(Duration::from_secs(30), frames.next())
        .await
        .map_err(|_| Error::Timeout)?
        .ok_or(Error::Disconnected)??;
    projection.apply(&format!("journal:{}", scope.session_id), first)?;
    require(
        projection
            .sessions
            .get(&scope.session_id)
            .is_some_and(|session| session.journal_ready),
        "fresh public follow did not publish a session history baseline",
    )?;
    let ready_ns = monotonic_ns();
    Ok((
        projection,
        frames,
        ready_ns.saturating_sub(start_ns),
        ready_ns,
    ))
}

async fn page20_sample(
    runtime: &SessionRuntime,
    endpoint: Arc<dyn SessionEndpoint>,
    scope: &RunScope,
) -> Result<Value> {
    let source_epoch = runtime.inspect(|store| store.connection_epoch);
    let (mut projection, mut frames, bootstrap_latency_ns, bootstrap_ns) =
        fresh_follow_projection(endpoint.as_ref(), scope).await?;
    let session = projection
        .sessions
        .get(&scope.session_id)
        .ok_or_else(|| Error::Protocol("fresh follow omitted original pointer".into()))?;
    let prior_ids = record_ids(scope, session);
    if endpoint.content_kind() == SessionContentKind::NativeConversation {
        let before = session
            .native_conversation
            .as_ref()
            .ok_or_else(|| Error::Protocol("native conversation baseline missing".into()))?;
        require(
            before.has_more_before,
            "page20 unavailable: no older native conversation",
        )?;
        let before_entries = before.entries.clone();
        let before_message_ids: Vec<_> = before_entries
            .iter()
            .filter(|entry| entry["type"] == "message")
            .filter_map(|entry| entry["id"].as_str().map(str::to_owned))
            .collect();
        let initial_ids_unique = {
            let unique: std::collections::BTreeSet<_> = before_message_ids.iter().collect();
            unique.len() == before_message_ids.len()
        };
        require(
            before_message_ids.len() == 20
                && initial_ids_unique
                && before_entries
                    .first()
                    .is_some_and(|entry| entry["type"] == "message"),
            "fresh native follow source window must begin at exactly 20 unique message records",
        )?;
        let before_entry_ids: Vec<_> = before_entries
            .iter()
            .filter_map(|entry| entry["id"].as_str().map(str::to_owned))
            .collect();
        let intent_ns = monotonic_ns();
        require(
            endpoint
                .load_earlier_conversation(&scope.session_id, 20)
                .await?,
            "native conversation has no earlier page",
        )?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        tokio::time::timeout_at(deadline, async {
            loop {
                let frame = frames.next().await.ok_or(Error::Disconnected)??;
                projection.apply(&format!("journal:{}", scope.session_id), frame)?;
                if projection
                    .sessions
                    .get(&scope.session_id)
                    .and_then(|session| session.native_conversation.as_ref())
                    .is_some_and(|conversation| {
                        let old_ids: std::collections::BTreeSet<_> =
                            before_entry_ids.iter().map(String::as_str).collect();
                        conversation
                            .entries
                            .iter()
                            .filter(|entry| {
                                entry["type"] == "message"
                                    && entry["id"].as_str().is_some_and(|id| !old_ids.contains(id))
                            })
                            .count()
                            >= 20
                    })
                {
                    return Ok::<_, Error>(());
                }
            }
        })
        .await
        .map_err(|_| Error::Timeout)??;
        let session = projection
            .sessions
            .get(&scope.session_id)
            .expect("pointer retained");
        let projected = session
            .native_conversation
            .as_ref()
            .expect("native projection published");
        let identities = record_ids(scope, session);
        let entry_ids: Vec<_> = projected
            .entries
            .iter()
            .filter_map(|entry| entry["id"].as_str().map(str::to_owned))
            .collect();
        let entry_ids_unique = {
            let unique: std::collections::BTreeSet<_> = entry_ids.iter().collect();
            unique.len() == entry_ids.len()
        };
        let old_ids: std::collections::BTreeSet<_> =
            before_entry_ids.iter().map(String::as_str).collect();
        let added_entries: Vec<_> = projected
            .entries
            .iter()
            .filter(|entry| entry["id"].as_str().is_some_and(|id| !old_ids.contains(id)))
            .collect();
        let added_message_entries: Vec<_> = added_entries
            .iter()
            .filter(|entry| entry["type"] == "message")
            .collect();
        let added_custom_entry_count = added_entries.len() - added_message_entries.len();
        let old_content_and_order_preserved = projected.entries.len() >= before_entries.len()
            && projected.entries[added_entries.len()..]
                .iter()
                .eq(before_entries.iter());
        let added_message_ids: Vec<_> = added_message_entries
            .iter()
            .filter_map(|entry| entry["id"].as_str().map(str::to_owned))
            .collect();
        let added_message_identities: Vec<_> = added_message_ids
            .iter()
            .map(|id| json!({"recordIdentity":{"endpointId":scope.endpoint_id,"sessionId":scope.session_id,"nativeEntryId":id}}))
            .collect();
        let retained = before_entry_ids
            .iter()
            .all(|old| entry_ids.iter().any(|new| new == old));
        let added = added_message_entries.len();
        let ordered = ordered_records(&identities);
        require(
            retained
                && ordered
                && entry_ids_unique
                && added == 20
                && old_content_and_order_preserved,
            "native follow/page20 projection identity/order mismatch",
        )?;
        let publication_ns = monotonic_ns();
        return Ok(json!({
            "event":"history_page",
            "pageMethod":"fresh SessionConversation(max_messages=20) follow + public load_earlier_conversation(20) + MemoryProjectionStore::apply",
            "scope":scope, "epoch":source_epoch, "projectionEpoch":projection.connection_epoch,
            "generation":projected.generation, "originalPointer":scope.address(),
            "bootstrapLatencyNs":bootstrap_latency_ns, "bootstrapMonotonicNs":bootstrap_ns,
            "intentMonotonicNs":intent_ns, "publicationMonotonicNs":publication_ns,
            "latencyNs":publication_ns.saturating_sub(intent_ns), "requestedMessages":20,
            "initialWindowMessageCount":before_message_ids.len(),
            "initialWindowFirstMessageId":before_message_ids.first(),
            "initialWindowLastMessageId":before_message_ids.last(),
            "priorRecordIdentities":prior_ids, "addedMessageRecordCount":added,
            "addedCustomEntryCount":added_custom_entry_count,
            "addedMessageRecordIdentities":added_message_identities,
            "projectedRecordIdentities":identities, "priorIdentityRetained":retained,
            "oldRecordContentAndOrderPreserved":old_content_and_order_preserved,
            "uniqueNativeEntryIdentities":entry_ids_unique,
            "strictOrder":ordered, "hasMoreBefore":projected.has_more_before, "error":Value::Null
        }));
    }

    let session = projection
        .sessions
        .get(&scope.session_id)
        .expect("pointer retained");
    let prior_entries: Vec<_> = session
        .entries
        .iter()
        .map(|(sequence, entry)| (*sequence, entry.clone()))
        .collect();
    let prior_sequences: Vec<_> = prior_entries
        .iter()
        .map(|(sequence, _)| *sequence)
        .collect();
    require(
        session.has_more_before,
        "page20 unavailable: no older journal history",
    )?;
    let request = HistoryRequest {
        session_id: scope.session_id.clone(),
        through_sequence: session.history_through_sequence,
        before_sequence: Some(session.history_base_sequence),
        max_messages: 20,
    };
    request.validate()?;
    let intent_ns = monotonic_ns();
    let page = endpoint.history_page(request.clone()).await?;
    page.validate(&request)?;
    // max_messages counts original message appends, not journal records.
    // The public page also retains control/tool records and source groups.
    let page_message_count = page
        .entries
        .iter()
        .filter(|entry| {
            matches!(
                entry.event.kind.as_str(),
                "user/message" | "assistant/message"
            ) && entry
                .event
                .metadata
                .get("surfaceOp")
                .is_none_or(|op| op == "append")
        })
        .count();
    require(
        page_message_count >= 20,
        "journal page did not include the requested 20 original message appends",
    )?;
    let source_ns = monotonic_ns();
    let accepted = projection
        .sessions
        .get_mut(&scope.session_id)
        .expect("session retained")
        .apply_page(page.clone(), &request)?;
    let identities = record_ids(
        scope,
        projection
            .sessions
            .get(&scope.session_id)
            .expect("session retained"),
    );
    let projected_entries: Vec<_> = projection
        .sessions
        .get(&scope.session_id)
        .unwrap()
        .entries
        .iter()
        .map(|(sequence, entry)| (*sequence, entry.clone()))
        .collect();
    let prior_sequence_set: std::collections::BTreeSet<_> =
        prior_sequences.iter().copied().collect();
    let newly_projected_sequences: Vec<_> = projected_entries
        .iter()
        .map(|(sequence, _)| *sequence)
        .filter(|sequence| !prior_sequence_set.contains(sequence))
        .collect();
    let page_sequences: Vec<_> = page.entries.iter().map(|entry| entry.event.seq).collect();
    let old_content_and_order_preserved =
        prior_entries.iter().all(|old| {
            projected_entries
                .iter()
                .any(|current| current.0 == old.0 && current.1 == old.1)
        }) && prior_sequences.windows(2).all(|pair| pair[0] < pair[1]);
    let page_identities_exactly_match_new_projection = newly_projected_sequences == page_sequences;
    let new_sequences_unique = {
        let unique: std::collections::BTreeSet<_> = newly_projected_sequences.iter().collect();
        unique.len() == newly_projected_sequences.len()
    };
    let page_precedes_old_window = prior_sequences
        .first()
        .is_some_and(|oldest| page_sequences.iter().all(|sequence| sequence < oldest));
    let retained = prior_ids
        .iter()
        .all(|old| identities.iter().any(|new| new == old));
    let ordered = ordered_records(&identities);
    let added = newly_projected_sequences.len();
    require(
        accepted
            && retained
            && ordered
            && added == page.entries.len()
            && page_identities_exactly_match_new_projection
            && new_sequences_unique
            && page_precedes_old_window
            && old_content_and_order_preserved,
        "journal follow/page20 projection identity/order mismatch",
    )?;
    let publication_ns = monotonic_ns();
    Ok(json!({
        "event":"history_page",
        "pageMethod":"fresh SessionJournal(max_messages=20) follow + SEP history_page(20) + public SessionProjection::apply_page",
        "scope":scope, "epoch":source_epoch, "projectionEpoch":projection.connection_epoch,
        "generation":projection.sessions.get(&scope.session_id).and_then(|s|s.history_revision.clone()),
        "originalPointer":scope.address(), "bootstrapLatencyNs":bootstrap_latency_ns,
        "bootstrapMonotonicNs":bootstrap_ns, "intentMonotonicNs":intent_ns,
        "sourcePublicationMonotonicNs":source_ns, "projectionPublicationMonotonicNs":publication_ns,
        "latencyNs":publication_ns.saturating_sub(intent_ns), "bootstrapRecordIdentities":prior_ids,
        "pageRecordIdentities":page.entries.iter().map(|entry| json!({
            "sequence":entry.event.seq,
            "recordIdentity":{"endpointId":scope.endpoint_id,"sessionId":scope.session_id,"sequence":entry.event.seq},
            "eventType":entry.event.kind,
        })).collect::<Vec<_>>(),
        "projectedRecordIdentities":identities, "priorIdentityRetained":retained,
        "newlyProjectedSequences":newly_projected_sequences,
        "pageIdentitiesExactlyMatchNewProjection":page_identities_exactly_match_new_projection,
        "newSequencesUnique":new_sequences_unique, "pagePrecedesOldWindow":page_precedes_old_window,
        "oldRecordContentAndOrderPreserved":old_content_and_order_preserved,
        "strictOrder":ordered, "addedRecordCount":added,
        "requestedMessages":20, "actualAppendMessageCount":page_message_count,
        "pageCountSemantics":"max_messages counts original message appends; full journal/source groups remain intact",
        "error":Value::Null
    }))
}

fn added_original_message_count(
    before_records: &[Value],
    session: &tekes_agent_session::SessionProjection,
) -> usize {
    let prior_sequences: std::collections::BTreeSet<_> = before_records
        .iter()
        .filter_map(|row| row["sequence"].as_i64())
        .collect();
    if let Some(conversation) = &session.native_conversation {
        let prior_ids: std::collections::BTreeSet<_> = before_records
            .iter()
            .filter_map(|row| row["recordIdentity"]["nativeEntryId"].as_str())
            .collect();
        conversation
            .entries
            .iter()
            .filter(|entry| {
                entry["type"] == "message"
                    && entry["id"]
                        .as_str()
                        .is_some_and(|id| !prior_ids.contains(id))
            })
            .count()
    } else {
        session
            .entries
            .iter()
            .filter(|(sequence, entry)| {
                !prior_sequences.contains(sequence)
                    && matches!(
                        entry.event.kind.as_str(),
                        "user/message" | "assistant/message"
                    )
                    && entry
                        .event
                        .metadata
                        .get("surfaceOp")
                        .is_none_or(|op| op == "append")
            })
            .count()
    }
}

async fn run_controller_page_sample(
    runtime: &SessionRuntime,
    controller: &RunController,
    scope: &RunScope,
) -> Result<Value> {
    if runtime
        .snapshot()
        .sessions
        .get(&scope.session_id)
        .is_some_and(|session| session.native_conversation.is_some())
    {
        // Live Pi events can append rows beyond its configured follow50
        // window before a full refresh. Reset through the public API so this
        // separately labelled fixed50 sample starts at a true window50.
        let mut preparation = close_open_sample(
            runtime,
            controller,
            scope,
            "prepare_original_RunController50_window",
        )
        .await?;
        preparation["event"] = json!("run_controller_history_page_preparation");
        emit(preparation);
    }
    let mut publications = runtime.subscribe();
    let intent_ns = monotonic_ns();
    let before = runtime.snapshot();
    let session = before.sessions.get(&scope.session_id).ok_or_else(|| {
        Error::Protocol("seeded pointer missing before RunController page".into())
    })?;
    let before_records = record_ids(scope, session);
    let has_more_before = session
        .native_conversation
        .as_ref()
        .map(|conversation| conversation.has_more_before)
        .unwrap_or(session.has_more_before);
    require(
        has_more_before,
        "RunController fixed-50 history page unavailable",
    )?;
    let loaded = controller.load_earlier(scope).await?;
    require(
        loaded,
        "RunController fixed-50 history page returned no earlier rows",
    )?;
    // Pi's public loadEarlier reply acknowledges the request; the original
    // conversation page arrives on its follow subscription. Time that source
    // publication, not the RPC acknowledgement or a guessed sleep.
    let after = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let snapshot = publications.borrow_and_update();
            require(
                snapshot.connection_epoch == before.connection_epoch,
                "RunController page crossed its original connection epoch",
            )?;
            if snapshot
                .sessions
                .get(&scope.session_id)
                .is_some_and(|session| added_original_message_count(&before_records, session) >= 50)
            {
                return Ok::<_, Error>(snapshot);
            }
            publications
                .changed()
                .await
                .map_err(|_| Error::Disconnected)?;
        }
    })
    .await
    .map_err(|_| Error::Timeout)??;
    let session = after
        .sessions
        .get(&scope.session_id)
        .ok_or_else(|| Error::Protocol("seeded pointer lost after RunController page".into()))?;
    let after_records = record_ids(scope, session);
    let retained = before_records.iter().all(|old| {
        after_records
            .iter()
            .any(|new| new["recordIdentity"] == old["recordIdentity"])
    });
    let old_content_and_order_preserved =
        before.sessions.get(&scope.session_id).is_some_and(|old| {
            match (&old.native_conversation, &session.native_conversation) {
                (Some(prior), Some(current)) => {
                    let old_ids: Vec<_> = prior
                        .entries
                        .iter()
                        .map(|entry| entry["id"].clone())
                        .collect();
                    let retained_ids: Vec<_> = current
                        .entries
                        .iter()
                        .filter(|entry| old_ids.contains(&entry["id"]))
                        .map(|entry| entry["id"].clone())
                        .collect();
                    prior
                        .entries
                        .iter()
                        .all(|entry| current.entries.contains(entry))
                        && retained_ids == old_ids
                }
                (None, None) => old
                    .entries
                    .iter()
                    .all(|(sequence, entry)| session.entries.get(sequence) == Some(entry)),
                _ => false,
            }
        });
    let ordered = ordered_records(&after_records);
    let grew = after_records.len() > before_records.len();
    let added_count = after_records.len().saturating_sub(before_records.len());
    let added_messages = added_original_message_count(&before_records, session);
    require(
        retained && ordered && grew && added_messages >= 50 && old_content_and_order_preserved,
        "RunController page did not publish its requested 50 original messages",
    )?;
    let end_ns = monotonic_ns();
    Ok(json!({
        "event":"run_controller_history_page",
        "pageMethod":"RunController::load_earlier -> SessionRuntime actual fixed max_messages=50",
        "scope":scope,
        "epoch":after.connection_epoch,
        "originalPointer":scope.address(),
        "intentMonotonicNs":intent_ns,
        "projectionPublicationMonotonicNs":end_ns,
        "latencyNs":end_ns.saturating_sub(intent_ns),
        "requestedMessages":50,
        "beforeCount":before_records.len(),
        "afterCount":after_records.len(),
        "addedCount":added_count,
        "actualAddedMessageCount":added_messages,
        "recordIdentities":after_records,
        "priorIdentityRetained":retained,
        "oldRecordContentAndOrderPreserved":old_content_and_order_preserved,
        "strictOrder":ordered,
        "error":Value::Null
    }))
}

async fn reconnect_sample(
    runtime: &SessionRuntime,
    connector: &ObservedConnector,
    controller: &RunController,
    scopes: &[RunScope],
    connector_kind: &str,
    config: &Value,
) -> Result<Value> {
    let intent_ns = monotonic_ns();
    let mut old = runtime.snapshot();
    let old_epoch = old.connection_epoch;
    let require_adapter_queue = if connector_kind == "dsh" {
        match config.get("observerProductSource").and_then(Value::as_str) {
            Some("f1daa0ffcf6ced41e21f351e22845455ca78006c") => false,
            Some("10263722b5fb128922c49a31b5942c41333cd6e2") => true,
            _ => {
                return Err(Error::Protocol(
                    "DSH observer requires a registered exact baseline/candidate source".into(),
                ));
            }
        }
    } else {
        false
    };
    let old_endpoint = connector.current()?;

    let mut dsh_source_before = std::collections::BTreeMap::new();
    let mut dsh_history_floors = std::collections::BTreeMap::new();
    let mut pi_file_before = std::collections::BTreeMap::new();
    let source_started_ns = monotonic_ns();
    if connector_kind == "dsh" {
        for scope in scopes {
            let witness = fresh_dsh_witness(
                old_endpoint.as_ref(),
                scope,
                "before_reconnect",
                require_adapter_queue,
            )
            .await?;
            dsh_source_before.insert(scope.session_id.clone(), witness);
        }
    } else if connector_kind == "pi" {
        for scope in scopes {
            let before = old
                .sessions
                .get(&scope.session_id)
                .ok_or_else(|| Error::Protocol("Pi pre-reconnect session is missing".into()))?;
            let conversation = before.native_conversation.as_ref().ok_or_else(|| {
                Error::Protocol("Pi pre-reconnect native conversation is missing".into())
            })?;
            require(
                conversation
                    .state
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .is_some_and(|id| !id.is_empty()),
                "Pi public state omitted native sessionId",
            )?;
            let path = conversation
                .state
                .get("sessionFile")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::Protocol("Pi public state omitted sessionFile".into()))?;
            let owned_root = pi_owned_evidence_root(config)?;
            let first = PiFileObservation::capture(std::path::Path::new(path), &owned_root)?;
            emit(
                json!({"event":"reconnect_file_metadata","scope":scope,"phase":"before_reconnect","boundary":"start","observation":first.evidence()}),
            );
            let source_conversation =
                fresh_pi_native_conversation(old_endpoint.as_ref(), scope, "before_reconnect")
                    .await?;
            require(
                source_conversation.state["sessionFile"] == path,
                "Pi fresh source changed sessionFile pointer",
            )?;
            require(
                source_conversation.state["sessionId"] == conversation.state["sessionId"],
                "Pi fresh source changed native session id",
            )?;
            require(
                source_conversation.entries == conversation.entries,
                "Pi fresh source differs from pre-reconnect native entries",
            )?;
            let second = PiFileObservation::capture(std::path::Path::new(path), &owned_root)?;
            emit(
                json!({"event":"reconnect_file_metadata","scope":scope,"phase":"before_reconnect","boundary":"end","observation":second.evidence()}),
            );
            require(
                first == second,
                "Pi session file changed during pre-reconnect source observation",
            )?;
            pi_file_before.insert(scope.session_id.clone(), first);
        }
    }
    if connector_kind == "dsh" {
        // The durable witness is the oracle; ClientKit does not require the old cache to catch up before disconnect.
        old = runtime.snapshot();
        require(
            old.connection_epoch == old_epoch,
            "DSH source freeze crossed connection epoch",
        )?;
        for scope in scopes {
            let witness = &dsh_source_before[&scope.session_id];
            let session = old
                .sessions
                .get(&scope.session_id)
                .ok_or_else(|| Error::Protocol("DSH source freeze lost original session".into()))?;
            require(
                session
                    .projections
                    .values()
                    .all(|value| value.sequence <= witness.through_sequence),
                "DSH pre-reconnect cache watermark exceeds independent fresh source",
            )?;
            let floor = *session
                .entries
                .keys()
                .next()
                .ok_or_else(|| Error::Protocol("DSH retained history is empty".into()))?;
            let retained_source = witness.retained_entries(floor)?;
            require(
                runtime_journal_entries(session)? == retained_source,
                "DSH retained before history differs from independent complete source",
            )?;
            dsh_history_floors.insert(scope.session_id.clone(), floor);
            emit(json!({"event":"reconnect_cache_before","scope":scope,
                "epoch":old.connection_epoch,"sourceCut":witness.through_sequence,
                "runtimeProjectionValuesMatchFreshSource":witness.matches_runtime_values(&session.projections, None),
                "runtimeFullHistoryMatchesRetainedSource":runtime_journal_entries(session)? == retained_source,
                "retainedHistoryFloor":floor,"completeSourceEntryCount":witness.entries.len(),
                "runtimeSession":session}));
        }
    }
    let mut old_pointers = Vec::new();
    for scope in scopes {
        old_pointers.push(json!({
            "scope": scope,
            "inventoryIdentity": scope.address(),
            "history": old.sessions.get(&scope.session_id).map(|s| record_ids(scope, s)).unwrap_or_default(),
        }));
    }
    let source_before_done_ns = monotonic_ns();
    runtime.reconnect().await?;
    let new_endpoint = connector.current()?;
    let endpoint_replaced = !Arc::ptr_eq(&old_endpoint, &new_endpoint);
    for scope in scopes {
        runtime.open_session(&scope.session_id).await?;
    }
    let after = runtime.snapshot();
    let new_epoch = after.connection_epoch;
    let source_after_started_ns = monotonic_ns();
    let mut converged = true;
    let mut pointers = Vec::new();
    for scope in scopes {
        let before = old.sessions.get(&scope.session_id);
        let mut after = runtime.snapshot();
        let mut source_after = None;
        let mut pi_file_after = None;
        let source_after_started_ns = monotonic_ns();
        if connector_kind == "dsh" {
            let witness = fresh_dsh_witness(
                new_endpoint.as_ref(),
                scope,
                "after_reconnect",
                require_adapter_queue,
            )
            .await?;
            wait_for_dsh_runtime(
                runtime,
                scope,
                &witness,
                dsh_history_floors[&scope.session_id],
                new_epoch,
                Duration::from_secs(5),
            )
            .await?;
            after = runtime.snapshot();
            source_after = Some(witness);
        } else if connector_kind == "pi" {
            let now = after
                .sessions
                .get(&scope.session_id)
                .ok_or_else(|| Error::Protocol("Pi recovered session is missing".into()))?;
            let conversation = now.native_conversation.as_ref().ok_or_else(|| {
                Error::Protocol("Pi recovered native conversation is missing".into())
            })?;
            require(
                conversation
                    .state
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .is_some_and(|id| !id.is_empty()),
                "Pi public state omitted native sessionId",
            )?;
            let path = conversation
                .state
                .get("sessionFile")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    Error::Protocol("Pi recovered public state omitted sessionFile".into())
                })?;
            let owned_root = pi_owned_evidence_root(config)?;
            let first = PiFileObservation::capture(std::path::Path::new(path), &owned_root)?;
            emit(
                json!({"event":"reconnect_file_metadata","scope":scope,"phase":"after_reconnect","boundary":"start","observation":first.evidence()}),
            );
            let source_conversation =
                fresh_pi_native_conversation(new_endpoint.as_ref(), scope, "after_reconnect")
                    .await?;
            require(
                source_conversation.state["sessionFile"] == path,
                "Pi recovered fresh source changed sessionFile pointer",
            )?;
            require(
                source_conversation.state["sessionId"] == conversation.state["sessionId"],
                "Pi recovered fresh source changed native session id",
            )?;
            require(
                source_conversation.entries == conversation.entries,
                "Pi recovered fresh source differs from runtime native entries",
            )?;
            let second = PiFileObservation::capture(std::path::Path::new(path), &owned_root)?;
            emit(
                json!({"event":"reconnect_file_metadata","scope":scope,"phase":"after_reconnect","boundary":"end","observation":second.evidence()}),
            );
            require(
                first == second,
                "Pi session file changed during recovered source observation",
            )?;
            pi_file_after = Some(first);
            after = runtime.snapshot();
        }
        let now = after.sessions.get(&scope.session_id);
        let before_summary = before.and_then(|s| s.summary.as_ref());
        let now_summary = now.and_then(|s| s.summary.as_ref());
        let raw_summary_equal = before_summary == now_summary;
        let (same_inventory, source_matches, source_evidence) = match connector_kind {
            "native" => (raw_summary_equal, raw_summary_equal, Value::Null),
            "pi" => {
                let old_file = pi_file_before.get(&scope.session_id).ok_or_else(|| {
                    Error::Protocol("Pi pre-reconnect file witness is missing".into())
                })?;
                let new_file = pi_file_after.as_ref().ok_or_else(|| {
                    Error::Protocol("Pi recovered file witness is missing".into())
                })?;
                let old_conversation = before
                    .and_then(|session| session.native_conversation.as_ref())
                    .ok_or_else(|| {
                        Error::Protocol("Pi pre-reconnect conversation is missing".into())
                    })?;
                let new_conversation = now
                    .and_then(|session| session.native_conversation.as_ref())
                    .ok_or_else(|| {
                        Error::Protocol("Pi recovered conversation is missing".into())
                    })?;
                let entry_content_equal = old_conversation.entries == new_conversation.entries;
                let same_file_location = old_file.canonical_path == new_file.canonical_path
                    && old_conversation.state["sessionId"] == new_conversation.state["sessionId"];
                let stable_summary = match (before_summary, now_summary) {
                    (Some(before), Some(now)) => {
                        reconnect_source::summaries_match_except_pi_updated_at(before, now)?
                    }
                    _ => false,
                };
                let exact_updated_at =
                    now_summary
                        .zip(pi_file_after.as_ref())
                        .is_some_and(|(summary, file)| {
                            reconnect_source::pi_summary_matches_file(summary, file)
                        });
                let pointer_fields_stable = old_conversation.state["sessionId"]
                    == new_conversation.state["sessionId"]
                    && old_conversation.state["sessionFile"]
                        == new_conversation.state["sessionFile"];
                let passed = stable_summary
                    && exact_updated_at
                    && pointer_fields_stable
                    && same_file_location
                    && entry_content_equal;
                (
                    passed,
                    passed,
                    json!({
                        "oracleVersion":"pi-session-file-metadata-v1",
                        "beforeFile":{"path":old_file.canonical_path,"device":old_file.device,"inode":old_file.inode,"bytes":old_file.bytes,"mtimeSeconds":old_file.modified_seconds,"mtimeNanos":old_file.modified_nanos},
                        "afterFile":{"path":new_file.canonical_path,"device":new_file.device,"inode":new_file.inode,"bytes":new_file.bytes,"mtimeSeconds":new_file.modified_seconds,"mtimeNanos":new_file.modified_nanos},
                        "nativeSessionIdUnchanged":old_conversation.state["sessionId"] == new_conversation.state["sessionId"],
                        "sessionFileUnchanged":old_conversation.state["sessionFile"] == new_conversation.state["sessionFile"],
                        "sameCanonicalSessionFileLocation":same_file_location,
                        "fullNativeEntryContentAndOrderUnchanged":entry_content_equal,
                        "fullEntriesBefore":old_conversation.entries,
                        "fullEntriesAfter":new_conversation.entries,
                        "summaryStableFieldsUnchanged":stable_summary,
                        "beforeSummary":before_summary,
                        "recoveredSummary":now_summary,
                        "updatedAtMatchesExactStableFileMtime":exact_updated_at,
                        "rawSummaryEqual":raw_summary_equal
                    }),
                )
            }
            "dsh" => {
                let old_source = dsh_source_before.get(&scope.session_id).ok_or_else(|| {
                    Error::Protocol("DSH pre-reconnect source witness is missing".into())
                })?;
                let new_source = source_after.as_ref().ok_or_else(|| {
                    Error::Protocol("DSH recovered source witness is missing".into())
                })?;
                let old_entries = before
                    .map(runtime_journal_entries)
                    .transpose()?
                    .unwrap_or_default();
                let now_entries = now
                    .map(runtime_journal_entries)
                    .transpose()?
                    .unwrap_or_default();
                let source_same = old_source.matches_recovered_source(new_source);
                let generation_changed = old_source.generation != new_source.generation;
                let runtime_values_match = now.is_some_and(|session| {
                    new_source.matches_runtime_values(
                        &session.projections,
                        before.map(|session| &session.projections),
                    )
                });
                let summary_matches_source = match now_summary {
                    Some(summary) => {
                        reconnect_source::summary_matches_dsh_witness(summary, new_source)?
                    }
                    None => false,
                };
                let summary_projection_kind_is_sequenced = now_summary
                    .and_then(|summary| summary["projections"]["kind"].as_str())
                    == Some("sequenced");
                let stable_summary = match (before_summary, now_summary) {
                    (Some(before), Some(now)) => {
                        reconnect_source::without_dynamic_summary_fields(before, true)?
                            == reconnect_source::without_dynamic_summary_fields(now, true)?
                    }
                    _ => false,
                };
                let floor = dsh_history_floors[&scope.session_id];
                let history_matches_source = old_entries == old_source.retained_entries(floor)?
                    && now_entries == new_source.retained_entries(floor)?;
                let history_unchanged = old_entries == now_entries;
                let passed = source_same
                    && generation_changed
                    && runtime_values_match
                    && summary_projection_kind_is_sequenced
                    && summary_matches_source
                    && stable_summary
                    && history_matches_source
                    && history_unchanged;
                (
                    passed,
                    passed,
                    json!({
                        "oracleVersion":"dsh-fresh-session-journal-v2",
                        "before":{"generation":old_source.generation,"throughSequence":old_source.through_sequence,"projections":{"asOfSeq":old_source.projections.as_of_seq,"values":old_source.projections.values},"entries":old_source.entries},
                        "after":{"generation":new_source.generation,"throughSequence":new_source.through_sequence,"projections":{"asOfSeq":new_source.projections.as_of_seq,"values":new_source.projections.values},"entries":new_source.entries},
                        "freshSourceUnchanged":source_same,
                        "newSourceGeneration":new_source.generation,
                        "sourceGenerationChanged":generation_changed,
                        "runtimeCompleteProjectionValuesMatchFreshSource":runtime_values_match,
                        "summaryProjectionAndUpdatedAtMatchFreshSource":summary_matches_source,
                        "summaryProjectionKindIsSequenced":summary_projection_kind_is_sequenced,
                        "summaryStableFieldsUnchanged":stable_summary,
                        "beforeSummary":before_summary,
                        "recoveredSummary":now_summary,
                        "runtimeFullHistoryMatchesFreshSources":history_matches_source,
                    "retainedHistoryFloor":floor,
                        "runtimeFullHistoryUnchanged":history_unchanged,
                        "rawSummaryEqual":raw_summary_equal
                    }),
                )
            }
            _ => (
                false,
                false,
                json!({"error":"unknown reconnect source contract"}),
            ),
        };
        let before_records = before.map(|s| record_ids(scope, s)).unwrap_or_default();
        let now_records = now.map(|s| record_ids(scope, s)).unwrap_or_default();
        let same_history_window = before_records == now_records;
        let unique_history_records = {
            let mut ids = std::collections::BTreeSet::new();
            now_records
                .iter()
                .all(|row| ids.insert(row["recordIdentity"].to_string()))
        };
        let strictly_ordered_history = ordered_records(&now_records);
        let usable = usable_snapshot(&after, &scope.session_id);
        let source_after_completed_ns = monotonic_ns();
        require(
            after.connection_epoch == new_epoch,
            "source observation crossed recovered connection epoch",
        )?;
        converged &= usable
            && same_inventory
            && same_history_window
            && unique_history_records
            && strictly_ordered_history;
        pointers.push(json!({
            "scope": scope,
            "originalPointer": scope.address(),
            "usable": usable,
            "sameInventory": same_inventory,
            "rawSummaryEqual": raw_summary_equal,
            "sourceMatches": source_matches,
            "sourceObservationStartedMonotonicNs": source_after_started_ns,
            "sourceObservationCompletedMonotonicNs": source_after_completed_ns,
            "sourceObservationDurationNs": source_after_completed_ns.saturating_sub(source_after_started_ns),
            "sourceEvidence": source_evidence,
            "sameHistoryWindow": same_history_window,
            "uniqueHistoryRecordIdentities": unique_history_records,
            "strictlyOrderedHistory": strictly_ordered_history,
            "historyRecordIdentities": now.map(|s| record_ids(scope, s)).unwrap_or_default(),
        }));
    }
    let publication_ns = monotonic_ns();
    let elapsed_ns = publication_ns.saturating_sub(intent_ns);
    let converged = converged && new_epoch > old_epoch && endpoint_replaced;
    controller.snapshot(); // Capture the public presentation publication after projection convergence.
    Ok(json!({
        "event": "reconnect",
        "intentMonotonicNs": intent_ns,
        "publicationMonotonicNs": publication_ns,
        "latencyNs": elapsed_ns,
        "oldEpoch": old_epoch,
        "newEpoch": new_epoch,
        "epochAdvanced": new_epoch > old_epoch,
        "converged": converged,
        "endpointGenerationReplaced": endpoint_replaced,
        "oldPointers": old_pointers,
        "pointers": pointers,
        "sourceObservationStartedMonotonicNs": source_started_ns,
        "sourceBeforeCompletedMonotonicNs": source_before_done_ns,
        "sourceBeforeObservationDurationNs": source_before_done_ns.saturating_sub(source_started_ns),
        "sourceAfterStartedMonotonicNs": source_after_started_ns,
        "error": if converged { Value::Null } else { json!("same-pointer inventory/history did not converge") }
    }))
}

async fn fresh_dsh_witness(
    endpoint: &dyn SessionEndpoint,
    scope: &RunScope,
    phase: &str,
    require_adapter_queue: bool,
) -> Result<DshSourceWitness> {
    tokio::time::timeout(Duration::from_secs(30), async {
        let mut stream = endpoint
            .follow(StreamTarget::SessionJournal {
                address: JournalAddress {
                    session_id: scope.session_id.clone(),
                },
                max_messages: 200,
            })
            .await?;
        let frame = stream.next().await.ok_or(Error::Disconnected)??;
        drop(stream);
        emit(
            json!({"event":"reconnect_source_frame","scope":scope,"phase":phase,
            "observedMonotonicNs":monotonic_ns(),"frame":frame}),
        );
        DshSourceWitness::from_first_frame(&frame, &scope.session_id, require_adapter_queue)
    })
    .await
    .map_err(|_| Error::Timeout)?
}

async fn fresh_pi_native_conversation(
    endpoint: &dyn SessionEndpoint,
    scope: &RunScope,
    phase: &str,
) -> Result<tekes_agent_contract::NativeConversationSnapshot> {
    let mut projection = MemoryProjectionStore::new(scope.endpoint_id.clone());
    projection.begin_connection(endpoint.host().clone());
    tokio::time::timeout(Duration::from_secs(30), async {
        let mut stream = endpoint
            .follow(StreamTarget::SessionConversation {
                address: JournalAddress {
                    session_id: scope.session_id.clone(),
                },
                max_messages: 50,
            })
            .await?;
        let frame = stream.next().await.ok_or(Error::Disconnected)??;
        drop(stream);
        emit(
            json!({"event":"reconnect_source_frame","scope":scope,"phase":phase,
            "observedMonotonicNs":monotonic_ns(),"frame":frame}),
        );
        projection.apply(&format!("journal:{}", scope.session_id), frame)?;
        projection
            .sessions
            .get(&scope.session_id)
            .and_then(|session| session.native_conversation.clone())
            .ok_or_else(|| {
                Error::Protocol("fresh Pi native source omitted conversation snapshot".into())
            })
    })
    .await
    .map_err(|_| Error::Timeout)?
}

fn pi_owned_evidence_root(config: &Value) -> Result<std::path::PathBuf> {
    let root = config
        .get("ownedEvidenceRoot")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            Error::Protocol("Pi observer requires the registered benchmark-owned case root".into())
        })?;
    let data = config
        .get("connector")
        .and_then(|connector| connector.get("dataRoot"))
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Protocol("Pi connector omitted its owned dataRoot".into()))?;
    let root = std::path::Path::new(root)
        .canonicalize()
        .map_err(|error| Error::Protocol(error.to_string()))?;
    let data = std::path::Path::new(data)
        .canonicalize()
        .map_err(|error| Error::Protocol(error.to_string()))?;
    require(
        data != root && data.starts_with(&root),
        "Pi dataRoot is outside the registered owned case root",
    )?;
    Ok(root)
}

async fn wait_for_dsh_runtime(
    runtime: &SessionRuntime,
    scope: &RunScope,
    witness: &DshSourceWitness,
    history_floor: i64,
    expected_epoch: u64,
    timeout: Duration,
) -> Result<()> {
    let expected_entries = witness.retained_entries(history_floor)?;
    let mut updates = runtime.subscribe();
    let result = tokio::time::timeout(timeout, async {
        loop {
            let snapshot = updates.borrow_and_update();
            require(
                snapshot.connection_epoch == expected_epoch,
                "DSH source wait crossed connection epoch",
            )?;
            if usable_snapshot(&snapshot, &scope.session_id)
                && snapshot.control_ready
                && let Some(session) = snapshot.sessions.get(&scope.session_id)
                && witness.matches_runtime_values(&session.projections, None)
                && runtime_journal_entries(session)? == expected_entries
            {
                return Ok::<(), Error>(());
            }
            updates.changed().await.map_err(|_| Error::Disconnected)?;
        }
    })
    .await
    .map_err(|_| Error::Timeout)
    .and_then(|result| result);
    let snapshot = runtime.snapshot();
    let session = snapshot.sessions.get(&scope.session_id);
    let projection_matches =
        session.is_some_and(|session| witness.matches_runtime_values(&session.projections, None));
    let history_matches = session
        .map(runtime_journal_entries)
        .transpose()?
        .is_some_and(|entries| entries == expected_entries);
    emit(
        json!({"event":"reconnect_runtime_source_check","scope":scope,
        "epoch":snapshot.connection_epoch,"expectedEpoch":expected_epoch,
        "usable":usable_snapshot(&snapshot, &scope.session_id),"controlReady":snapshot.control_ready,
        "projectionValuesMatch":projection_matches,"fullHistoryMatches":history_matches,
        "sourceCut":witness.through_sequence,"retainedHistoryFloor":history_floor,
        "expectedRetainedEntries":expected_entries,"runtimeSession":session,
        "waitError":result.as_ref().err().map(error_text)}),
    );
    result
}

fn runtime_journal_entries(session: &tekes_agent_session::SessionProjection) -> Result<Vec<Value>> {
    Ok(session
        .entries
        .values()
        .map(serde_json::to_value)
        .collect::<std::result::Result<_, _>>()?)
}

fn matching_user_and_assistant(
    controller: &RunController,
    ticket_id: &str,
    expected_prompt: &str,
    expected_assistant: &str,
) -> (Option<Value>, Option<Value>, Option<Value>) {
    let snapshot = controller.snapshot();
    let user = snapshot.transcript.items.iter().find(|item| {
        item.kind == tekes_agent_presentation::TranscriptItemKind::User
            && item.reconciliation_id.as_deref() == Some(ticket_id)
            && !item.is_provisional
            && item.text == expected_prompt
            && item.record_id.as_deref().is_some_and(|id| !id.is_empty())
    });
    let assistant = user.and_then(|user| {
        snapshot.transcript.items.iter().find(|item| {
            item.kind == tekes_agent_presentation::TranscriptItemKind::Assistant
                && item.turn_id == user.turn_id
                && (!item.text.is_empty() || !item.reasoning.is_empty())
        })
    });
    let settled = user.and_then(|user| {
        snapshot.transcript.items.iter().find(|item| {
            item.kind == tekes_agent_presentation::TranscriptItemKind::Assistant
                && item.turn_id == user.turn_id
                && !item.is_provisional
                && item.is_final
                && item.text == expected_assistant
        })
    });
    (
        user.map(|item| json!({"itemId":item.id,"recordId":item.record_id,"turnId":item.turn_id,"textBytes":item.text.len()})),
        assistant.map(|item| json!({"itemId":item.id,"recordId":item.record_id,"turnId":item.turn_id,"textBytes":item.text.len(),"reasoningBytes":item.reasoning.len(),"isFinal":item.is_final})),
        settled.map(|item| json!({"itemId":item.id,"recordId":item.record_id,"turnId":item.turn_id,"textBytes":item.text.len(),"isFinal":item.is_final,"isProvisional":item.is_provisional})),
    )
}

async fn send_sample(
    controller: &RunController,
    scope: &RunScope,
    session_index: usize,
    round: usize,
    prompt: &str,
    expected_assistant: &str,
) -> Result<Value> {
    let destination = ComposerDestination::run(scope.clone());
    require(
        prompt.len() == 1024,
        "iteration prompt must contain exactly 1024 UTF-8 bytes",
    )?;
    require(
        expected_assistant.len() == 1024,
        "iteration reply must contain exactly 1024 UTF-8 bytes",
    )?;
    controller.open(scope.clone()).await?;
    controller.edit(
        &destination,
        ComposerDraft {
            text: prompt.to_owned(),
            ..Default::default()
        },
    )?;
    let ticket = controller.prepare(&destination, PromptMode::Queue)?;
    let ticket_id = ticket.id().to_owned();
    let mut changes = controller.subscribe();
    let intent_ns = monotonic_ns();
    let dispatch_future = controller.dispatch(ticket, None);
    tokio::pin!(dispatch_future);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    let (
        user,
        user_observed_ns,
        assistant,
        assistant_observed_ns,
        settled_assistant,
        dispatch,
        admitted_ns,
    ) = tokio::time::timeout_at(deadline, async {
        let mut user = None;
        let mut user_observed_ns = None;
        let mut assistant = None;
        let mut assistant_observed_ns = None;
        let mut settled_assistant = None;
        let mut dispatch = None;
        let mut admitted_ns = None;
        loop {
            let pair =
                matching_user_and_assistant(controller, &ticket_id, prompt, expected_assistant);
            if user.is_none() && pair.0.is_some() {
                user = pair.0;
                user_observed_ns = Some(monotonic_ns());
            }
            if assistant.is_none() && pair.1.is_some() {
                assistant = pair.1;
                assistant_observed_ns = Some(monotonic_ns());
            }
            if settled_assistant.is_none() && pair.2.is_some() {
                settled_assistant = pair.2;
            }
            if user.is_some()
                && assistant.is_some()
                && settled_assistant.is_some()
                && dispatch.is_some()
            {
                return Ok::<_, Error>((
                    user,
                    user_observed_ns,
                    assistant,
                    assistant_observed_ns,
                    settled_assistant,
                    dispatch.take().unwrap(),
                    admitted_ns.unwrap(),
                ));
            }
            tokio::select! {
                receipt = &mut dispatch_future, if dispatch.is_none() => {
                    dispatch = Some(receipt?);
                    admitted_ns = Some(monotonic_ns());
                }
                changed = changes.changed() => {
                    changed.map_err(|_| Error::Disconnected)?;
                }
            }
        }
    })
    .await
    .map_err(|_| Error::Timeout)??;
    let assistant_ns = assistant_observed_ns.unwrap_or_else(monotonic_ns);
    Ok(json!({
        "event": "send",
        "scope": scope,
        "sessionIndex": session_index,
        "round": round,
        "ticketId": ticket_id,
        "intentMonotonicNs": intent_ns,
        "acceptedMonotonicNs": admitted_ns,
        "durableUserObservedMonotonicNs": user_observed_ns,
        "firstAssistantContentMonotonicNs": assistant_ns,
        "assistantContentExpectation": "first non-empty matching-turn assistant content frame; separate durable final must equal iterationReplies",
        "expectedPromptBytes": prompt.len(),
        "expectedAssistantReplyBytes": expected_assistant.len(),
        "dispatchLatencyNs": admitted_ns.saturating_sub(intent_ns),
        "durableUserObservedLatencyNs": user_observed_ns.map(|at| at.saturating_sub(intent_ns)),
        "assistantContentObservedLatencyNs": assistant_ns.saturating_sub(intent_ns),
        "durableUser": user,
        "firstMatchingAssistantContent": assistant,
        "durableMatchingAssistantSettlement": settled_assistant,
        "dispatchReceipt": dispatch,
        "dispatchReceiptSource": "RunController dispatch admission receipt; provider request receipts and separate prompt/reply body digests are produced by the performance fixture helper.",
        "error": Value::Null
    }))
}

#[derive(Clone, Copy)]
struct IterationContext<'a> {
    runtime: &'a SessionRuntime,
    connector: &'a ObservedConnector,
    controller: &'a RunController,
    scopes: &'a [RunScope],
    connector_kind: &'a str,
    config: &'a Value,
}

async fn iteration(
    context: &IterationContext<'_>,
    round: usize,
    phase: &str,
    iteration: usize,
) -> Result<bool> {
    let IterationContext {
        runtime,
        connector,
        controller,
        scopes,
        connector_kind,
        config,
    } = *context;
    emit(json!({
        "event":"iteration_started",
        "mode":phase,
        "iteration":iteration,
        "expectedBodyMessagesBeforePerSession": if phase == "cold" { 100 } else { 100 + 2 * round },
        "growthAfterIteration": "one exact 1024-byte durable user plus one matching 1024-byte final assistant per session; no additional tool requests"
    }));
    let steady = phase.starts_with("warm");
    let mut snapshots_usable = true;
    for scope in scopes {
        if phase != "cold" {
            match close_open_sample(runtime, controller, scope, phase).await {
                Ok(sample) => {
                    snapshots_usable &= sample["usable"] == true;
                    emit(sample);
                }
                Err(error) => {
                    snapshots_usable = false;
                    emit(
                        json!({"event":"history_close_open_snapshot","scope":scope,"iteration":iteration,"error":error_text(&error),"status":"failed"}),
                    );
                }
            }
        } else {
            let start = monotonic_ns();
            let sample = snapshot_sample(runtime, scope, start, phase);
            snapshots_usable &= sample["usable"] == true;
            emit(sample);
        }
    }
    if !snapshots_usable {
        emit(
            json!({"event":"iteration_failed","iteration":iteration,"failedStep":"snapshot","status":"failed"}),
        );
        return Ok(false);
    }
    let mut pages_ok = true;
    for scope in scopes {
        match page20_sample(runtime, connector.current()?, scope).await {
            Ok(sample) => emit(sample),
            Err(error) => {
                pages_ok = false;
                emit(json!({
                    "event":"history_page", "scope":scope, "iteration":iteration,
                    "error":error_text(&error), "status":"failed"
                }));
            }
        }
    }
    if connector_kind == "pi" {
        for scope in scopes {
            match close_open_sample(
                runtime,
                controller,
                scope,
                "restore_pi_runtime_window_50_after_page_probe",
            )
            .await
            {
                Ok(sample) => emit(sample),
                Err(error) => {
                    emit(
                        json!({"event":"history_close_open_snapshot","scope":scope,"iteration":iteration,"error":error_text(&error),"status":"failed"}),
                    );
                    return Ok(false);
                }
            }
        }
    }
    if !pages_ok {
        emit(
            json!({"event":"iteration_failed","iteration":iteration,"failedStep":"history_page","status":"failed"}),
        );
        return Ok(false);
    }
    if !steady {
        match reconnect_sample(
            runtime,
            connector,
            controller,
            scopes,
            connector_kind,
            config,
        )
        .await
        {
            Ok(sample) => {
                let converged = sample["converged"] == true;
                emit(sample);
                if !converged {
                    emit(
                        json!({"event":"iteration_failed","iteration":iteration,"failedStep":"reconnect_convergence","status":"failed"}),
                    );
                    return Ok(false);
                }
            }
            Err(error) => {
                emit(json!({
                    "event":"reconnect", "iteration":iteration,
                    "error":error_text(&error), "status":"failed"
                }));
                return Ok(false);
            }
        }
    }
    let prompts: Vec<String> = serde_json::from_value(config["iterationPrompts"][round].clone())?;
    let replies: Vec<String> = serde_json::from_value(config["iterationReplies"][round].clone())?;
    require(
        prompts.len() == 2 && replies.len() == 2,
        "each iteration requires exactly two explicit prompt and reply bodies",
    )?;
    for (session_index, scope) in scopes.iter().enumerate() {
        match send_sample(
            controller,
            scope,
            session_index,
            round,
            &prompts[session_index],
            &replies[session_index],
        )
        .await
        {
            Ok(sample) => emit(sample),
            Err(error) => {
                emit(json!({
                    "event":"send", "scope":scope, "round":round,
                    "error":error_text(&error), "status":"failed"
                }));
                emit(
                    json!({"event":"iteration_failed","iteration":iteration,"failedStep":"send","failedSessionIndex":session_index,"status":"failed"}),
                );
                return Ok(false);
            }
        }
    }
    Ok(true)
}

async fn run(config: Value) -> Result<()> {
    let connector_config = config
        .get("connector")
        .ok_or_else(|| Error::Protocol("missing connector configuration".into()))?;
    let session_ids: Vec<String> =
        serde_json::from_value(config.get("sessionIds").cloned().unwrap_or(Value::Null))?;
    require(
        session_ids.len() == 2,
        "exactly two seeded sessionIds required",
    )?;
    require(
        session_ids[0] != session_ids[1],
        "seeded session IDs must be distinct",
    )?;
    let mode = string(&config, "mode")?;
    require(
        matches!(mode, "cold" | "warm" | "reconnectLifecycle"),
        "mode must be cold, warm, or reconnectLifecycle",
    )?;
    let expected_rounds = if mode == "cold" { 1 } else { 11 };
    let iteration_prompts: Vec<Vec<String>> = serde_json::from_value(
        config
            .get("iterationPrompts")
            .cloned()
            .unwrap_or(Value::Null),
    )?;
    require(
        iteration_prompts.len() == expected_rounds,
        "iterationPrompts count does not match frozen mode trajectory",
    )?;
    require(
        iteration_prompts
            .iter()
            .all(|pair| pair.len() == 2 && pair.iter().all(|body| body.len() == 1024)),
        "each iteration requires two exact 1024-byte UTF-8 bodies",
    )?;
    let iteration_replies: Vec<Vec<String>> = serde_json::from_value(
        config
            .get("iterationReplies")
            .cloned()
            .unwrap_or(Value::Null),
    )?;
    require(
        iteration_replies.len() == expected_rounds,
        "iterationReplies count does not match frozen mode trajectory",
    )?;
    require(
        iteration_replies
            .iter()
            .all(|pair| pair.len() == 2 && pair.iter().all(|body| body.len() == 1024)),
        "each iteration requires two exact 1024-byte UTF-8 reply bodies",
    )?;
    // The cold first-usable interval begins before connector setup and runtime spawn.
    let start_ns = monotonic_ns();
    let (identity, inner) = connector(connector_config).await?;
    let connector_kind = string(connector_config, "kind")?.to_owned();
    let observed = Arc::new(ObservedConnector {
        inner,
        current: Mutex::new(None),
    });
    let spawn_intent_ns = monotonic_ns();
    let runtime = Arc::new(SessionRuntime::spawn(identity.clone(), observed.clone()));
    let mut output = json!({
        "event":"consumer_started", "mode":mode, "endpointId":identity,
        "sessionIds":session_ids, "intentMonotonicNs":start_ns,
        "runtimeOwnership":"external wrapper owns harness processes and RSS sampling",
        "processSemantics": if mode == "warm" && connector_kind == "pi" {
            "no endpoint reconnect in steady warm; public history close/open resets Pi follow window50 while Context::process may reuse live Pi RPC. Consumer cannot read PID; wrapper must prove same PID/start identity."
        } else if mode == "reconnectLifecycle" && connector_kind == "pi" {
            "explicit endpoint reconnect closes and recreates per-session Pi RPC children; this is reconnectLifecycle, never same-harness warm. Wrapper records process close/probe/start identity and RSS."
        } else {
            "consumer process remains one PID; endpoint/harness process identity and RSS are measured by the external wrapper."
        },
        "historyWindowDisclosure": if connector_kind == "pi" {
            "Runtime initial follow remains its public fixed window (50). Each page probe creates an independent public NativeConversation follow(max_messages=20), requests load_earlier_conversation(20), and applies that original follow publication to a fresh MemoryProjectionStore."
        } else {
            "Runtime initial follow remains its public fixed window (50). Each page probe creates an independent public SessionJournal follow(max_messages=20), issues SEP history_page(20), and applies the result to that fresh public SessionProjection. It does not slice the Runtime page50."
        },
        "runtimeHistoryCacheSemantics":"Public close/open confirms not-ready then a new usable follow publication. It may retain older Journal records; emitted pre/post counts and identities show the actual cache, and this consumer never claims public close trimmed it.",
        "promptTrajectory":"iterationPrompts and iterationReplies are independent frozen inputs: cold has one two-session round; warm and reconnectLifecycle each have excluded round0 plus measured rounds1-10. The performance provider fixture helper supplies role-specific AKPERF headers and distinct user/assistant bodies, each exactly 1024 UTF-8 bytes. The consumer treats supplied bodies as opaque, checks durable user content against its prompt and ticket, and checks final assistant content against its distinct reply. Provider request receipts report separate user and assistant body digests.",
        "providerFixtureHeaderSource":"scripts/performance_provider_fixture.py body_text/body_bytes helper; the consumer does not synthesize the AKPERF role header.",
        "metrics":{
            "implemented":["first usable projection snapshot", "journal SEP history_page(20) plus public SessionProjection::apply_page; Pi native load_earlier_conversation(20) plus public projection publication", "explicit reconnect with same-pointer inventory/raw-history identity/order convergence", "RunController send through observed durable user and first matching assistant content", "one separate RunController history page using its true fixed 50 semantics per consumer process"],
            "notImplemented":["OS process RSS sampling", "provider/runtime startup timeline", "fixture seed corpus content/digest verification", "Native busy queue-specific case", "family parent-child case", "attachments/queue workload"]
        }
    });
    if mode == "warm" {
        output["warmMeasuredIterations"] = json!(10);
        output["warmInitializationExcluded"] = json!(true);
        output["steadyWarmNoReconnect"] = json!(true);
    } else if mode == "reconnectLifecycle" {
        output["reconnectLifecycleMeasuredIterations"] = json!(10);
        output["initializationExcluded"] = json!(true);
        output["sameHarnessProcessClaimed"] = json!(false);
    } else {
        output["coldMeasuredIterations"] = json!(1);
    }
    emit(output);
    let ready_intent = spawn_intent_ns;
    runtime.wait_ready(Duration::from_secs(30)).await?;
    let runtime_ready_ns = monotonic_ns();
    emit(json!({
        "event":"runtime_ready",
        "intentMonotonicNs":ready_intent,
        "coldStartIntentMonotonicNs":start_ns,
        "connectorSetupNs":spawn_intent_ns.saturating_sub(start_ns),
        "readyMonotonicNs":runtime_ready_ns,
        "startupToRuntimeReadyNs":runtime_ready_ns.saturating_sub(ready_intent),
        "status":"connectivity_ready_only_not_session_snapshot_acceptance"
    }));
    let scopes: Vec<RunScope> = session_ids
        .iter()
        .map(|id| RunScope::main(identity.clone(), id.clone()))
        .collect();
    let controller = RunController::new(runtime.clone())?;
    for scope in &scopes {
        let open_intent_ns = monotonic_ns();
        controller.open(scope.clone()).await?;
        let purpose = if mode == "cold" {
            "cold_first_usable_original_projection"
        } else {
            "initial_usable_original_projection"
        };
        let mut sample = snapshot_sample(runtime.as_ref(), scope, start_ns, purpose);
        require(
            sample["usable"] == true,
            "initial original-pointer projection was not usable",
        )?;
        sample["openIntentMonotonicNs"] = json!(open_intent_ns);
        sample["openLatencyNs"] = json!(
            sample["observedMonotonicNs"]
                .as_u64()
                .unwrap_or_default()
                .saturating_sub(open_intent_ns as u64)
        );
        emit(sample);
    }
    let iteration_context = IterationContext {
        runtime: runtime.as_ref(),
        connector: observed.as_ref(),
        controller: &controller,
        scopes: &scopes,
        connector_kind: &connector_kind,
        config: &config,
    };
    if mode == "cold" {
        let _ = iteration(&iteration_context, 0, "cold", 0).await?;
    } else {
        // Frozen recipe: one complete warm-up iteration is retained but excluded
        // by the wrapper; measured rounds then start at 102 body records/session.
        let phase = if mode == "warm" {
            "warm_initialization_excluded"
        } else {
            "reconnectLifecycle_initialization_excluded"
        };
        let initialized = iteration(&iteration_context, 0, phase, 0).await?;
        if !initialized {
            for measured in 1..=10 {
                emit(
                    json!({"event":"iteration_not_run","mode":mode,"iteration":measured,"expectedBodyMessagesBeforePerSession":100 + 2 * measured,"reason":"excluded initialization iteration failed; preserve dependent growth trajectory","status":"not_run"}),
                );
            }
        } else {
            for measured in 0..10 {
                let completed = iteration(
                    &iteration_context,
                    measured + 1,
                    if mode == "warm" {
                        "warm"
                    } else {
                        "reconnectLifecycle"
                    },
                    measured + 1,
                )
                .await?;
                if !completed {
                    for dependent in measured + 2..=10 {
                        emit(
                            json!({"event":"iteration_not_run","mode":mode,"iteration":dependent,"expectedBodyMessagesBeforePerSession":100 + 2 * dependent,"reason":"prior iteration did not reach its preregistered growth state","status":"not_run"}),
                        );
                    }
                    break;
                }
            }
        }
    }
    for scope in &scopes {
        match run_controller_page_sample(runtime.as_ref(), &controller, scope).await {
            Ok(sample) => emit(sample),
            Err(error) => emit(json!({
                "event":"run_controller_history_page", "scope":scope,
                "error":error_text(&error), "status":"failed_or_not_run"
            })),
        }
    }
    let cleanup_intent_ns = monotonic_ns();
    emit(json!({"event":"cleanup_intent","intentMonotonicNs":cleanup_intent_ns}));
    runtime.stop().await?;
    let cleanup_completed_ns = monotonic_ns();
    emit(
        json!({"event":"runtime_cleanup_completed","intentMonotonicNs":cleanup_intent_ns,
        "completedMonotonicNs":cleanup_completed_ns,
        "latencyNs":cleanup_completed_ns.saturating_sub(cleanup_intent_ns),
        "osProcessExitVerified":false,"externalOwnedHarnessCleanupVerified":false}),
    );
    Ok(())
}

#[tokio::main]
async fn main() {
    let mut input = String::new();
    let result = std::io::stdin()
        .take(1024 * 1024)
        .read_to_string(&mut input)
        .map_err(|_| Error::Transport("stdin unavailable".into()))
        .and_then(|_| serde_json::from_str::<Value>(&input).map_err(Error::from));
    let result = match result {
        Ok(config) => run(config).await,
        Err(error) => Err(error),
    };
    if let Err(error) = result {
        emit(
            json!({"event":"consumer_error","error":error_text(&error),"status":"failed_or_not_run"}),
        );
        std::process::exit(2);
    }
}

#[cfg(test)]
mod reconnect_input_tests {
    use super::*;

    #[test]
    fn pi_owned_root_uses_explicit_case_and_nested_connector_data() {
        let temp = std::env::temp_dir().join(format!(
            "agentkit-owned-input-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(temp.join("case/data")).unwrap();
        let root = temp.join("case").canonicalize().unwrap();
        let data = root.join("data");
        assert_eq!(
            pi_owned_evidence_root(
                &json!({"ownedEvidenceRoot":root,"connector":{"dataRoot":data}})
            )
            .unwrap(),
            root
        );
        assert!(pi_owned_evidence_root(&json!({"connector":{"dataRoot":data}})).is_err());
        assert!(
            pi_owned_evidence_root(&json!({"ownedEvidenceRoot":root,"dataRoot":data})).is_err()
        );
        assert!(
            pi_owned_evidence_root(
                &json!({"ownedEvidenceRoot":data,"connector":{"dataRoot":root}})
            )
            .is_err()
        );
        std::fs::remove_dir_all(temp).unwrap();
    }
}
