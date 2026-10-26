use crate::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tekes_agent_presentation::{
    ChildComposerAvailability, ChildHistoryLayout, ChildRun, RunScope, SummaryFacts,
    TranscriptSnapshot,
};

pub(crate) struct FamilyChildView {
    endpoint: String,
    parent: String,
    child: String,
    epoch: u64,
    layout: ChildHistoryLayout,
    run: Arc<tokio::sync::Mutex<ChildRun>>,
    following: Arc<AtomicBool>,
    revoked: Arc<AtomicBool>,
    last_used: Instant,
}

struct ChildFollowGuard<'a> {
    views: &'a std::sync::Mutex<BTreeMap<String, FamilyChildView>>,
    handle: String,
}
impl Drop for ChildFollowGuard<'_> {
    fn drop(&mut self) {
        if let Some(view) = self
            .views
            .lock()
            .expect("family views lock")
            .remove(&self.handle)
        {
            view.revoked.store(true, Ordering::SeqCst);
        }
    }
}

impl SharedAgent {
    pub(crate) async fn family_snapshot(
        &self,
        shared: &SharedEndpoint,
        parent_id: &str,
    ) -> Result<Value> {
        require(!parent_id.is_empty(), "empty parent session")?;
        let epoch = shared.runtime.snapshot().connection_epoch;
        let endpoint = shared.connected()?;
        let catalog = if endpoint.capabilities().supports("subagents.list") {
            Some(shared.runtime.subagent_catalog(parent_id).await?)
        } else {
            None
        };
        self.family_snapshot_with_catalog(shared, parent_id, epoch, catalog)
    }

    fn family_snapshot_with_catalog(
        &self,
        shared: &SharedEndpoint,
        parent_id: &str,
        expected_epoch: u64,
        catalog: Option<SubagentCatalog>,
    ) -> Result<Value> {
        require(!parent_id.is_empty(), "empty parent session")?;
        let epoch = shared.runtime.snapshot().connection_epoch;
        require(epoch == expected_epoch, "family view connection changed")?;
        shared.connected()?;
        let summary = read_summary(&shared.runtime, parent_id)?;
        require(
            shared.runtime.snapshot().connection_epoch == epoch,
            "family snapshot connection changed during read",
        )?;
        Ok(
            json!({"endpointId":shared.runtime.snapshot().endpoint_id,"parentSessionId":parent_id,
            "connectionEpoch":epoch,"summary":summary,"subagentCatalog":catalog}),
        )
    }

    pub(crate) async fn follow_family(
        &self,
        shared: &SharedEndpoint,
        request: &Value,
        output: mpsc::Sender<Value>,
    ) -> Result<()> {
        let target = &request["target"];
        let parent_id = string(target, "parentSessionId")?.to_owned();
        let epoch = shared.runtime.snapshot().connection_epoch;
        let runtime = shared.runtime.clone();
        let mut observer = runtime.subscribe();
        let mut refresh_catalog = tokio::time::interval_at(
            tokio::time::Instant::now() + Duration::from_secs(1),
            Duration::from_secs(1),
        );
        refresh_catalog.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut catalog = None;
        let mut previous = None;
        let mut refresh = true;
        loop {
            if runtime.snapshot().connection_epoch != epoch {
                break;
            }
            if refresh {
                let endpoint = shared.connected()?;
                catalog = if endpoint.capabilities().supports("subagents.list") {
                    Some(runtime.subagent_catalog(&parent_id).await?)
                } else {
                    None
                };
            }
            let frame =
                self.family_snapshot_with_catalog(shared, &parent_id, epoch, catalog.clone())?;
            if previous.as_ref() != Some(&frame) {
                tokio::select! {
                    result = output.send(frame.clone()) => if result.is_err() { break; },
                    _ = output.closed() => break,
                }
                previous = Some(frame);
            }
            refresh = false;
            tokio::select! {
                result = observer.changed() => if result.is_err() { break; },
                _ = refresh_catalog.tick() => refresh = true,
                _ = output.closed() => break,
            }
        }
        Ok(())
    }

    pub(crate) async fn open_child(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
    ) -> Result<Value> {
        let parent_id = string(payload, "parentSessionId")?;
        let child_id = string(payload, "childId")?;
        require(parent_id != child_id, "invalid child identity")?;
        let endpoint = shared.connected()?;
        require(
            endpoint.capabilities().supports("subagents.list"),
            "subagent catalog unavailable",
        )?;
        let epoch = shared.runtime.snapshot().connection_epoch;
        let catalog = shared.runtime.subagent_catalog(parent_id).await?;
        let child = catalog
            .child(child_id)
            .ok_or_else(|| unsupported("child is not listed under this parent"))?;
        let availability = ChildComposerAvailability::resolve(Some(&catalog), child_id);
        let mut state = shared.runtime.snapshot();
        require(
            state.connection_epoch == epoch,
            "child catalog connection changed",
        )?;
        require(
            state.sessions.contains_key(parent_id),
            "child parent session unavailable",
        )?;
        let layout = if state.sessions.contains_key(child_id) {
            ChildHistoryLayout::IndependentSessions
        } else if endpoint.capabilities().supports("subagents.history") {
            ChildHistoryLayout::SameSession
        } else {
            // A catalog-authorized Pi/DSH child may not have been opened by any observer yet.
            // Ask that adapter to acquire it through its normal public session API, then check
            // that the exact row appeared in this endpoint's projection.
            shared.runtime.open_session(child_id).await?;
            state = shared.runtime.snapshot();
            require(
                state.connection_epoch == epoch,
                "child session changed endpoint during open",
            )?;
            require(
                state.sessions.contains_key(parent_id),
                "child parent session unavailable",
            )?;
            require(
                state.sessions.contains_key(child_id),
                "catalog child session did not appear",
            )?;
            ChildHistoryLayout::IndependentSessions
        };
        let parent = RunScope::main(&state.endpoint_id, parent_id);
        let opened = ChildRun::open(shared.runtime.clone(), parent, child_id, layout, None).await?;
        require(
            shared.runtime.snapshot().connection_epoch == epoch,
            "child history connection changed while opening",
        )?;
        let run = Arc::new(tokio::sync::Mutex::new(opened));
        let transcript = run.lock().await.transcript().await?;
        require(
            shared.runtime.snapshot().connection_epoch == epoch,
            "child history connection changed during read",
        )?;
        let handle = uuid::Uuid::new_v4().to_string();
        let mut views = self.family_views.lock().expect("family views lock");
        views.retain(|_, view| {
            let retain = view.following.load(Ordering::SeqCst)
                || view.last_used.elapsed() < Duration::from_secs(300);
            if !retain {
                view.revoked.store(true, Ordering::SeqCst);
            }
            retain
        });
        while views.len() >= 32 {
            let oldest = views
                .iter()
                .filter(|(_, view)| !view.following.load(Ordering::SeqCst))
                .min_by_key(|(_, view)| view.last_used)
                .map(|(handle, _)| handle.clone());
            if let Some(oldest) = oldest {
                if let Some(view) = views.remove(&oldest) {
                    view.revoked.store(true, Ordering::SeqCst);
                }
            } else {
                return Err(unsupported("too many active child views"));
            }
        }
        views.insert(
            handle.clone(),
            FamilyChildView {
                endpoint: state.endpoint_id.clone(),
                parent: parent_id.into(),
                child: child_id.into(),
                epoch,
                layout,
                run,
                following: Arc::new(AtomicBool::new(false)),
                revoked: Arc::new(AtomicBool::new(false)),
                last_used: Instant::now(),
            },
        );
        Ok(
            json!({"endpointId":state.endpoint_id,"parentSessionId":parent_id,"childId":child_id,
            "connectionEpoch":epoch,"layout":layout,"availability":availability,"mode":child.mode,
            "handle":handle,"transcript":transcript}),
        )
    }

    pub(crate) async fn child_earlier(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
    ) -> Result<Value> {
        let (handle, view) = self.child_view(shared, payload)?;
        let run = view.run.clone();
        let run = run.lock().await;
        let loaded = run.load_earlier().await?;
        let transcript = run.transcript().await?;
        self.validate_child_view(shared, &handle, &view)?;
        Ok(
            json!({"loaded":loaded,"transcript":transcript,"connectionEpoch":view.epoch,"handle":handle}),
        )
    }

    pub(crate) async fn child_prompt(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        let (handle, view) = self.child_view(shared, payload)?;
        let mode: PromptMode = serde_json::from_value(payload["mode"].clone())?;
        let content = payload["content"]
            .as_array()
            .cloned()
            .ok_or_else(|| unsupported("child prompt content must be an array"))?;
        let runtime = shared.runtime.clone();
        let guard_runtime = runtime.clone();
        let epoch = view.epoch;
        let revoked = view.revoked.clone();
        let result = runtime
            .prompt_child_with_validation(
                &view.parent,
                &view.child,
                mode,
                content,
                rpc_id,
                move || {
                    require(
                        !revoked.load(Ordering::SeqCst),
                        "child view closed before admission",
                    )?;
                    require(
                        guard_runtime.inspect(|state| state.connection_epoch == epoch),
                        "child prompt connection changed before admission",
                    )
                },
            )
            .await?;
        self.validate_child_view(shared, &handle, &view)?;
        Ok(result)
    }

    pub(crate) async fn child_cancel(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        let (handle, view) = self.child_view(shared, payload)?;
        let catalog = shared.runtime.subagent_catalog(&view.parent).await?;
        catalog.require_continuable(&view.child)?;
        let revoked = view.revoked.clone();
        shared
            .runtime
            .call_at_epoch_with_validation(
                view.epoch,
                "subagents.cancel",
                json!({"parentSessionId":view.parent,"childSessionId":view.child}),
                rpc_id,
                move || {
                    require(
                        !revoked.load(Ordering::SeqCst),
                        "child view closed before admission",
                    )
                },
            )
            .await?;
        self.validate_child_view(shared, &handle, &view)?;
        Ok(json!({}))
    }

    pub(crate) async fn close_child(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
    ) -> Result<Value> {
        let handle = string(payload, "handle")?;
        let mut key = self.family_views.lock().expect("family views lock");
        let view = key
            .get(handle)
            .ok_or_else(|| unsupported("child view is stale or already closed"))?;
        require_child_address(
            &view.endpoint,
            &view.parent,
            &view.child,
            &shared.runtime.snapshot().endpoint_id,
            string(payload, "parentSessionId")?,
            string(payload, "childId")?,
        )?;
        require(
            !view.following.load(Ordering::SeqCst),
            "close the child observation first",
        )?;
        if let Some(view) = key.remove(handle) {
            view.revoked.store(true, Ordering::SeqCst);
        }
        Ok(json!({}))
    }

    pub(crate) async fn follow_child(
        &self,
        shared: &SharedEndpoint,
        request: &Value,
        output: mpsc::Sender<Value>,
    ) -> Result<()> {
        let target = &request["target"];
        let handle = string(target, "handle")?.to_owned();
        let (view, following, epoch) = {
            let mut views = self.family_views.lock().expect("family views lock");
            let view = views
                .get_mut(&handle)
                .ok_or_else(|| unsupported("child view is stale; reopen it"))?;
            require_child_address(
                &view.endpoint,
                &view.parent,
                &view.child,
                &shared.runtime.snapshot().endpoint_id,
                string(target, "parentSessionId")?,
                string(target, "childId")?,
            )?;
            require(
                view.epoch == shared.runtime.snapshot().connection_epoch,
                "child view is stale; reopen it",
            )?;
            require(
                !view.following.swap(true, Ordering::SeqCst),
                "child view already has an observer",
            )?;
            view.last_used = Instant::now();
            (view.run.clone(), view.following.clone(), view.epoch)
        };
        let _cleanup = ChildFollowGuard {
            views: &self.family_views,
            handle,
        };
        let runtime = shared.runtime.clone();
        let mut observer = runtime.subscribe();
        let mut heartbeat = tokio::time::interval_at(
            tokio::time::Instant::now() + Duration::from_secs(1),
            Duration::from_secs(1),
        );
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            if shared.runtime.snapshot().connection_epoch != epoch {
                break;
            }
            let transcript = view.lock().await.transcript().await?;
            if output
                .send(json!({"endpointId":shared.runtime.snapshot().endpoint_id,
                "parentSessionId":string(target,"parentSessionId")?,
                "childId":string(target,"childId")?,"handle":string(target,"handle")?,
                "connectionEpoch":epoch,"transcript":transcript}))
                .await
                .is_err()
            {
                break;
            }
            tokio::select! {
                result = observer.changed() => if result.is_err() { break },
                _ = heartbeat.tick() => if shared.runtime.snapshot().connection_epoch != epoch { break },
                _ = output.closed() => break,
            }
        }
        following.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn child_view(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
    ) -> Result<(String, FamilyChildView)> {
        let handle = string(payload, "handle")?.to_owned();
        let parent = string(payload, "parentSessionId")?;
        let child = string(payload, "childId")?;
        let mut views = self.family_views.lock().expect("family views lock");
        let view = views
            .get_mut(&handle)
            .ok_or_else(|| unsupported("child view is stale or closed; reopen it"))?;
        require_child_address(
            &view.endpoint,
            &view.parent,
            &view.child,
            &shared.runtime.snapshot().endpoint_id,
            parent,
            child,
        )?;
        require(
            view.epoch == shared.runtime.snapshot().connection_epoch,
            "child view is stale; reopen it",
        )?;
        view.last_used = Instant::now();
        Ok((handle, view.clone_for_route()))
    }

    fn validate_child_view(
        &self,
        shared: &SharedEndpoint,
        handle: &str,
        expected: &FamilyChildView,
    ) -> Result<()> {
        let views = self.family_views.lock().expect("family views lock");
        let current = views
            .get(handle)
            .ok_or_else(|| unsupported("child view closed during operation"))?;
        require_child_address(
            &current.endpoint,
            &current.parent,
            &current.child,
            &expected.endpoint,
            &expected.parent,
            &expected.child,
        )?;
        require(
            current.epoch == expected.epoch
                && shared.runtime.snapshot().connection_epoch == expected.epoch,
            "child view changed during operation",
        )
    }
}

impl FamilyChildView {
    fn clone_for_route(&self) -> Self {
        Self {
            endpoint: self.endpoint.clone(),
            parent: self.parent.clone(),
            child: self.child.clone(),
            epoch: self.epoch,
            layout: self.layout,
            run: self.run.clone(),
            following: self.following.clone(),
            revoked: self.revoked.clone(),
            last_used: self.last_used,
        }
    }
}

fn read_summary(runtime: &SessionRuntime, session_id: &str) -> Result<SummaryFacts> {
    let state = runtime.snapshot();
    let scope = RunScope::main(&state.endpoint_id, session_id);
    let session = state
        .sessions
        .get(session_id)
        .ok_or_else(|| unsupported("summary parent session unavailable"))?;
    let transcript = TranscriptSnapshot::read(&scope, session)?;
    let facts = SummaryFacts::read_with_store(&scope, &state, &transcript)?;
    facts.validate()?;
    Ok(facts)
}

fn require_child_address(
    expected_endpoint: &str,
    expected_parent: &str,
    expected_child: &str,
    endpoint: &str,
    parent: &str,
    child: &str,
) -> Result<()> {
    require(
        !expected_endpoint.is_empty()
            && !expected_parent.is_empty()
            && !expected_child.is_empty()
            && expected_endpoint == endpoint
            && expected_parent == parent
            && expected_child == child,
        "child handle address mismatch",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_handle_address_rejects_other_endpoint_parent_and_child() {
        let expected = ("pi:one", "parent", "child");
        assert!(
            require_child_address(
                expected.0, expected.1, expected.2, expected.0, expected.1, expected.2
            )
            .is_ok()
        );
        assert!(
            require_child_address(
                expected.0, expected.1, expected.2, "pi:two", expected.1, expected.2
            )
            .is_err()
        );
        assert!(
            require_child_address(
                expected.0, expected.1, expected.2, expected.0, "other", expected.2
            )
            .is_err()
        );
        assert!(
            require_child_address(
                expected.0, expected.1, expected.2, expected.0, expected.1, "other"
            )
            .is_err()
        );
    }
}
