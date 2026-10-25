//! A standalone presentation mount over an existing RunBinding.
//!
//! This adapter deliberately owns no draft, settings, or submission ledger. A
//! detached mount can be discarded while another mount retains the same Run.
use crate::{ComposerSettingsRequest, PaneAccess, RunBinding};
use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use tekes_agent_contract::{PromptMode, Result, require};
use tekes_agent_presentation::{
    ComposerContextBaseline, ComposerDestination, ComposerDraft, PreparedSubmission, RunScope,
    RunSnapshot, SubmissionPreparation, SubmissionState,
};
use tokio::sync::watch;

struct MountLifetime {
    attached: AtomicBool,
    detached: watch::Sender<bool>,
}
impl MountLifetime {
    fn new() -> Arc<Self> {
        let (detached, _) = watch::channel(false);
        Arc::new(Self {
            attached: AtomicBool::new(true),
            detached,
        })
    }
    fn retire(&self) -> bool {
        if self.attached.swap(false, Ordering::AcqRel) {
            self.detached.send_replace(true);
            true
        } else {
            false
        }
    }
}

/// Durable owner for standalone Composer mounts bound to one existing Run.
/// Construct it from the exact RunBinding also used by the Run surface.
pub struct StandaloneComposerBinding {
    run: Arc<RunBinding>,
    next_mount_id: AtomicU64,
}

/// One attach lifetime. Detaching this value only retires this presentation
/// mount; it does not drop the shared RunBinding or discard its Run draft.
pub struct StandaloneComposerMount {
    owner: Arc<StandaloneComposerBinding>,
    id: u64,
    lifetime: Arc<MountLifetime>,
}

/// A settings response remains bound to the exact live mount that requested it.
pub struct StandaloneComposerSettingsRequest {
    request: Option<ComposerSettingsRequest>,
    lifetime: Weak<MountLifetime>,
    mount_id: u64,
}

/// Filtered publications for this exact Run destination. A shared controller
/// may publish another selected Run; those snapshots are never surfaced here.
pub struct StandaloneComposerUpdates {
    receiver: watch::Receiver<Arc<RunSnapshot>>,
    destination: ComposerDestination,
    lifetime: Weak<MountLifetime>,
    detached: watch::Receiver<bool>,
}

impl StandaloneComposerBinding {
    /// Capture a live, writable Run. Read-only bounded history and retired
    /// pointers cannot be promoted into standalone Composer surfaces.
    pub fn open(run: Arc<RunBinding>) -> Result<Arc<Self>> {
        require(
            run.access() == PaneAccess::Ready,
            "standalone Composer requires a ready Run",
        )?;
        let snapshot = run.current_run().ok_or_else(|| {
            tekes_agent_contract::Error::Protocol("standalone Composer Run is unavailable".into())
        })?;
        require(
            snapshot.selection.as_ref().is_some_and(|selection| {
                selection.destination == ComposerDestination::run(run.scope().clone())
            }),
            "standalone Composer Run is not the selected presentation",
        )?;
        Ok(Arc::new(Self {
            run,
            next_mount_id: AtomicU64::new(1),
        }))
    }

    pub fn scope(&self) -> &RunScope {
        self.run.scope()
    }

    pub fn destination(&self) -> ComposerDestination {
        ComposerDestination::run(self.scope().clone())
    }

    /// Create a new presentation lifetime for this same Run. Reopening the
    /// binding for another Run is required to change destination identity.
    pub fn mount(self: &Arc<Self>) -> Result<StandaloneComposerMount> {
        require(
            self.run.access() == PaneAccess::Ready,
            "standalone Composer Run is unavailable",
        )?;
        require(
            self.run.current_run().is_some_and(|snapshot| {
                snapshot
                    .selection
                    .as_ref()
                    .is_some_and(|selection| selection.destination == self.destination())
            }),
            "standalone Composer Run is not the selected presentation",
        )?;
        let id = self
            .next_mount_id
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current.checked_add(1)
            })
            .map_err(|_| {
                tekes_agent_contract::Error::Protocol(
                    "standalone Composer mount id exhausted".into(),
                )
            })?;
        Ok(StandaloneComposerMount {
            owner: self.clone(),
            id,
            lifetime: MountLifetime::new(),
        })
    }
}

impl StandaloneComposerMount {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn scope(&self) -> &RunScope {
        self.owner.scope()
    }

    pub fn destination(&self) -> ComposerDestination {
        self.owner.destination()
    }

    pub fn is_attached(&self) -> bool {
        self.lifetime.attached.load(Ordering::Acquire)
    }

    fn require_attached(&self) -> Result<()> {
        require(self.is_attached(), "standalone Composer mount detached")?;
        require(
            self.owner.run.access() == PaneAccess::Ready,
            "standalone Composer Run unavailable",
        )
    }

    /// Read the original published Run, including its destination-owned draft,
    /// settings and unresolved submission IDs.
    pub fn snapshot(&self) -> Result<Arc<RunSnapshot>> {
        self.require_attached()?;
        let snapshot = self.owner.run.current_run().ok_or_else(|| {
            tekes_agent_contract::Error::Protocol("standalone Composer Run unavailable".into())
        })?;
        require(
            snapshot
                .selection
                .as_ref()
                .is_some_and(|selection| selection.destination == self.destination()),
            "standalone Composer Run is not the selected presentation",
        )?;
        Ok(snapshot)
    }

    pub fn subscribe(&self) -> Result<StandaloneComposerUpdates> {
        self.snapshot()?;
        let receiver = self.owner.run.subscribe_run().ok_or_else(|| {
            tekes_agent_contract::Error::Protocol("standalone Composer has no writable Run".into())
        })?;
        require(
            receiver
                .borrow()
                .selection
                .as_ref()
                .is_some_and(|selection| selection.destination == self.destination()),
            "standalone Composer Run selection changed while subscribing",
        )?;
        Ok(StandaloneComposerUpdates {
            receiver,
            destination: self.destination(),
            lifetime: Arc::downgrade(&self.lifetime),
            detached: self.lifetime.detached.subscribe(),
        })
    }

    pub fn edit_text(&self, text: String) -> Result<()> {
        self.require_attached()?;
        self.owner.run.edit_text(&self.destination(), text)
    }

    pub fn patch_context(
        &self,
        baseline: &ComposerContextBaseline,
        artifacts: Vec<tekes_agent_presentation::ComposerArtifact>,
        annotations: Vec<tekes_agent_contract::ComposerAnnotation>,
    ) -> Result<bool> {
        self.require_attached()?;
        require(
            baseline.selection.destination == self.destination(),
            "standalone Composer context destination changed",
        )?;
        self.owner
            .run
            .patch_context(baseline, artifacts, annotations)
    }

    pub fn begin_settings(&self) -> Result<StandaloneComposerSettingsRequest> {
        self.require_attached()?;
        let request = self.owner.run.begin_settings(&self.destination())?;
        Ok(StandaloneComposerSettingsRequest {
            request: Some(request),
            lifetime: Arc::downgrade(&self.lifetime),
            mount_id: self.id,
        })
    }

    pub fn prepare_send(&self, mode: PromptMode) -> Result<PreparedSubmission> {
        self.require_attached()?;
        self.owner.run.prepare_send(&self.destination(), mode)
    }

    /// Dispatch the exact ticket returned for this Run. The RunBinding keeps
    /// its original submission id and admission state if the future is dropped.
    pub async fn dispatch_prepared_with(
        &self,
        ticket: PreparedSubmission,
        preparation: &dyn SubmissionPreparation,
    ) -> Result<()> {
        require(
            ticket.destination() == &self.destination(),
            "standalone Composer submission destination changed",
        )?;
        if !self.is_attached() {
            // This ticket has not started transport. Release only its local
            // reservation so a detached pre-dispatch click cannot wedge Send.
            self.owner.run.cancel_prepared(ticket)?;
            return Err(tekes_agent_contract::Error::Protocol(
                "standalone Composer mount detached".into(),
            ));
        }
        self.require_attached()?;
        self.owner
            .run
            .dispatch_prepared_with(ticket, preparation)
            .await
    }

    pub fn cancel_prepared(&self, ticket: PreparedSubmission) -> Result<()> {
        require(
            ticket.destination() == &self.destination(),
            "standalone Composer submission destination changed",
        )?;
        // A prepared ticket has not entered transport and may be safely
        // released during unmount. The controller rejects an already claimed
        // ticket, so this cannot cancel or retry a remote operation.
        self.owner.run.cancel_prepared(ticket)
    }

    /// Explicit remote cancellation remains a host action against this Run.
    pub async fn cancel(&self) -> Result<()> {
        self.require_attached()?;
        self.owner.run.cancel(&self.destination()).await
    }

    /// Detach only this surface lifetime. Pending remote work and its canonical
    /// destination state remain owned by RunBinding.
    pub fn detach(&self) -> bool {
        self.lifetime.retire()
    }

    /// Reattach the same durable owner with a fresh mount identity.
    pub fn reattach(&self) -> Result<StandaloneComposerMount> {
        self.owner.mount()
    }

    /// Current submission facts are read from the canonical Run publication.
    pub fn submissions(&self) -> Result<Vec<SubmissionState>> {
        Ok(self.snapshot()?.submissions.clone())
    }

    /// Acknowledge only an original unresolved ID after the host reconciles it
    /// against authoritative history. The RunBinding validates ownership too.
    pub fn acknowledge_reconciliation(&self, submission_id: &str) -> Result<()> {
        self.require_attached()?;
        let snapshot = self.snapshot()?;
        require(
            snapshot
                .submissions
                .iter()
                .any(|submission| submission.id == submission_id && !submission.reconciled),
            "submission is not unresolved in this Run",
        )?;
        self.owner.run.acknowledge_reconciliation(submission_id)
    }

    /// A snapshot convenience for hosts that only render the Composer region.
    pub fn draft(&self) -> Result<ComposerDraft> {
        Ok(self.snapshot()?.draft.clone())
    }

    /// Exposes whether this mount still has mutation authority.
    pub fn can_submit(&self) -> Result<bool> {
        Ok(self.snapshot()?.can_submit)
    }
}

impl StandaloneComposerSettingsRequest {
    pub fn mount_id(&self) -> u64 {
        self.mount_id
    }

    pub fn destination(&self) -> Result<&ComposerDestination> {
        require(
            self.lifetime
                .upgrade()
                .is_some_and(|state| state.attached.load(Ordering::Acquire)),
            "standalone Composer mount detached",
        )?;
        self.request
            .as_ref()
            .map(ComposerSettingsRequest::destination)
            .ok_or_else(|| {
                tekes_agent_contract::Error::Protocol("settings request already consumed".into())
            })
    }

    pub fn commit(
        mut self,
        settings: std::collections::BTreeMap<String, serde_json::Value>,
    ) -> Result<bool> {
        require(
            self.lifetime
                .upgrade()
                .is_some_and(|state| state.attached.load(Ordering::Acquire)),
            "standalone Composer mount detached",
        )?;
        self.request
            .take()
            .expect("single-use settings request")
            .commit(settings)
    }
}

impl StandaloneComposerUpdates {
    pub fn current(&self) -> Result<Arc<RunSnapshot>> {
        require(
            self.lifetime
                .upgrade()
                .is_some_and(|state| state.attached.load(Ordering::Acquire))
                && !*self.detached.borrow(),
            "standalone Composer mount detached",
        )?;
        let snapshot = self.receiver.borrow().clone();
        require(
            snapshot
                .selection
                .as_ref()
                .is_some_and(|selection| selection.destination == self.destination),
            "standalone Composer Run is not the selected presentation",
        )?;
        Ok(snapshot)
    }

    pub async fn changed(&mut self) -> Result<Arc<RunSnapshot>> {
        loop {
            require(
                self.lifetime
                    .upgrade()
                    .is_some_and(|state| state.attached.load(Ordering::Acquire))
                    && !*self.detached.borrow(),
                "standalone Composer mount detached",
            )?;
            tokio::select! {
                result = self.receiver.changed() => {
                    result.map_err(|_| tekes_agent_contract::Error::Disconnected)?;
                }
                result = self.detached.changed() => {
                    if result.is_err() || *self.detached.borrow() {
                        return Err(tekes_agent_contract::Error::Protocol("standalone Composer mount detached".into()));
                    }
                    continue;
                }
            }
            let snapshot = self.receiver.borrow_and_update().clone();
            require(
                self.lifetime
                    .upgrade()
                    .is_some_and(|state| state.attached.load(Ordering::Acquire))
                    && !*self.detached.borrow(),
                "standalone Composer mount detached",
            )?;
            if snapshot
                .selection
                .as_ref()
                .is_some_and(|selection| selection.destination == self.destination)
            {
                return Ok(snapshot);
            }
        }
    }
}

impl Drop for StandaloneComposerMount {
    fn drop(&mut self) {
        self.lifetime.retire();
    }
}

// Keep imports and APIs exercised by the compilation boundary when this module
// is registered in lib.rs. Reconciliation is exposed by the narrow RunBinding
// method owned by the coordinating session.
