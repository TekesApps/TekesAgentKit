//! Real SEP/page facade proof; no scrolling or reading-anchor implementation.
use crate::component_demo::{ComponentDemo, SELECT_A, SELECT_B};
use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use tekes_agent_contract::{Error, Result, require, unsupported};
use tekes_agent_presentation::{ComposerDestination, TranscriptSnapshot};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{HistoryPagePhase, RunBinding};
use tekes_ui_controls::composition::AccessibilityAction;
#[derive(Clone)]
enum Frontier {
    Journal {
        cut: i64,
        base: i64,
    },
    Native {
        leaf: Option<String>,
        entries: usize,
    },
}
fn frontier(runtime: &SessionRuntime, binding: &RunBinding) -> Result<Frontier> {
    runtime.inspect(|store| {
        let session = store
            .sessions
            .get(&binding.scope().session_id)
            .ok_or_else(|| unsupported("page source missing"))?;
        Ok(if let Some(native) = &session.native_conversation {
            Frontier::Native {
                leaf: native.leaf_id.clone(),
                entries: native.entries.len(),
            }
        } else {
            Frontier::Journal {
                cut: session.history_through_sequence,
                base: session.history_base_sequence,
            }
        })
    })
}
fn keys(snapshot: &TranscriptSnapshot) -> BTreeSet<&str> {
    snapshot
        .items
        .iter()
        .filter(|item| item.is_visible && !item.is_provisional)
        .map(|item| item.id.as_str())
        .collect()
}
fn retained(before: &TranscriptSnapshot, after: &TranscriptSnapshot) -> Result<()> {
    let previous = keys(before);
    let next = keys(after);
    require(
        previous.is_subset(&next),
        "page/reconnect lost stable message identities",
    )?;
    require(
        next.len()
            == after
                .items
                .iter()
                .filter(|item| item.is_visible && !item.is_provisional)
                .count(),
        "duplicate page identities",
    )?;
    for item in before
        .items
        .iter()
        .filter(|item| item.is_visible && !item.is_provisional)
    {
        let next = after
            .items
            .iter()
            .find(|next| next.id == item.id)
            .expect("verified subset");
        require(
            item.text == next.text
                && item.reasoning == next.reasoning
                && item.attachments == next.attachments,
            "page changed retained message content",
        )?;
    }
    Ok(())
}
pub async fn run(view: &mut ComponentDemo, runtime: &Arc<SessionRuntime>) -> Result<Value> {
    let a = view.bindings()[0].clone();
    let b = view.bindings()[1].clone();
    let before_history = a
        .current_run()
        .ok_or_else(|| unsupported("page source not writable"))?
        .transcript
        .clone();
    require(
        before_history.has_more_before && !before_history.is_stale,
        "history proof needs a clipped canonical baseline",
    )?;
    b.edit_text(
        &ComposerDestination::run(b.scope().clone()),
        "B 分页隔离草稿".into(),
    )?;
    let b_draft = b.current_run().unwrap().draft.clone();
    let before = frontier(runtime, &a)?;
    let mut updates = a
        .subscribe_run()
        .ok_or_else(|| unsupported("page source not observable"))?;
    let request = a.begin_history_page()?;
    require(
        a.history_page_state().phase == HistoryPagePhase::Loading,
        "missing page loading receipt",
    )?;
    view.accessibility_action(SELECT_B, None, AccessibilityAction::Press)?;
    require(request.load().await?, "page did not extend")?;
    let source_kind = match (&before, frontier(runtime, &a)?) {
        (
            Frontier::Journal { cut, base },
            Frontier::Journal {
                cut: next_cut,
                base: next_base,
            },
        ) => {
            require(
                *cut == next_cut && next_base < *base,
                "journal cut/frontier changed incorrectly",
            )?;
            "journal"
        }
        (
            Frontier::Native { leaf, entries },
            Frontier::Native {
                leaf: next_leaf,
                entries: next_entries,
            },
        ) => {
            require(
                *leaf == next_leaf && next_entries > *entries,
                "native leaf/frontier changed incorrectly",
            )?;
            "nativeConversation"
        }
        _ => return Err(unsupported("page source dialect changed")),
    };
    let after_history = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let snapshot = a.current_run().unwrap();
            if !snapshot.transcript.is_stale
                && keys(&snapshot.transcript).len() > keys(&before_history).len()
            {
                return Ok::<_, Error>(snapshot.transcript.clone());
            }
            updates.changed().await.map_err(|_| Error::Disconnected)?;
        }
    })
    .await
    .map_err(|_| Error::Timeout)??;
    retained(&before_history, &after_history)?;
    require(
        a.history_page_state().is_current
            && a.history_page_state().phase == HistoryPagePhase::Loaded { extended: true },
        "missing original page completion",
    )?;
    require(
        b.history_page_state().phase == HistoryPagePhase::Idle
            && b.current_run().unwrap().draft == b_draft
            && view.selected() == 1,
        "page completion touched B",
    )?;
    let old_epoch = runtime.inspect(|store| store.connection_epoch);
    runtime.reconnect().await?;
    let mut changes = runtime.subscribe();
    let fresh = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let snapshot = changes.inspect(|store| {
                if store.connection_epoch == old_epoch || !store.ready() {
                    return Ok(None);
                }
                let Some(session) = store
                    .sessions
                    .get(&a.scope().session_id)
                    .filter(|session| session.journal_ready)
                else {
                    return Ok(None);
                };
                TranscriptSnapshot::read(a.scope(), session).map(Some)
            })?;
            if let Some(snapshot) = snapshot {
                return Ok::<_, Error>(snapshot);
            }
            changes.changed().await.map_err(|_| Error::Disconnected)?;
        }
    })
    .await
    .map_err(|_| Error::Timeout)??;
    retained(&after_history, &fresh)?;
    require(
        !a.history_page_state().is_current,
        "old page receipt survived reconnect",
    )?;
    require(
        b.current_run().unwrap().draft == b_draft && view.selected() == 1,
        "reconnect touched B page/draft",
    )?;
    // An explicit final navigation makes the restored A data available to the
    // generic offscreen receipt; completion/reconnect themselves never navigate.
    view.accessibility_action(SELECT_A, None, AccessibilityAction::Press)?;
    view.sync()?;
    Ok(
        json!({"originalScope":true,"extended":true,"frozenCut":true,"sourceKind":source_kind,"otherDraftPreserved":true,"sourceStatusPublished":true,
        "retainedMessages":keys(&after_history).len(),"newEpochHistoryRetained":true,"noDuplicateMessages":true,"oldReceiptInvalidated":true,"nativeInteraction":false,"readingAnchorAcceptance":false}),
    )
}
