//! Mounted run-source ownership for Agent UI hosts.
//!
//! This adapter retains history observations and projects existing presentation;
//! it does not implement a Run state machine, renderer, or window event loop.
mod models;
mod presets;
pub use presets::*;
mod sidebar;
pub use models::*;
pub use sidebar::*;
mod sidechat;
pub use sidechat::*;
mod child;
mod task;
pub use child::*;
use std::sync::Arc;
pub use task::*;
use tekes_agent_components_ui::{Composer, ComposerAffordances, ComposerControlGate};
use tekes_agent_contract::{PromptMode, Result, require, unsupported};
use tekes_agent_presentation::{
    AgentTask, ChildHistoryLayout, ChildHistorySource, ChildRun, ComposerDestination,
    ConnectionPresentation, PreparedSubmission, RunController, RunScope, RunSnapshot, SideChat,
    SideChatController, SideChatPhase, SideChatSnapshot, SubmissionPreparation, TranscriptSnapshot,
};
use tekes_agent_session::{ProjectionSubscription, SessionHistoryLease, SessionRuntime};
use tokio::sync::watch;

/// A projection of canonical pointer ownership, not another mutation state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneAccess {
    Ready,
    Closing,
    Unknown,
    Missing,
}
impl PaneAccess {
    pub fn composer_gate(self) -> ComposerControlGate {
        if self == Self::Ready {
            ComposerControlGate::OPEN
        } else {
            ComposerControlGate::CLOSED
        }
    }
}

#[derive(Clone)]
pub enum PaneSnapshot {
    Run {
        snapshot: Arc<RunSnapshot>,
        access: PaneAccess,
    },
    /// Native bounded child history has no fabricated main-run Composer state.
    ReadOnlyChild {
        scope: RunScope,
        connection: ConnectionPresentation,
        transcript: TranscriptSnapshot,
    },
    /// An absent/changed task or ephemeral pointer must not become a new conversation.
    Missing { scope: RunScope },
}

/// One mount owns its own history lease. Dropping it never cancels an agent or
/// discards a SideChat. Mount tokens in SidePanel fence installation of this value.
pub struct RunBinding {
    runtime: Arc<SessionRuntime>,
    scope: RunScope,
    controller: Option<RunController>,
    _lease: Option<SessionHistoryLease>,
    child: Option<ChildRun>,
    side_chat: Option<(Arc<SideChatController>, SideChat)>,
    task: Option<task::TaskRunGuard>,
    child_owner: Option<std::sync::Weak<std::sync::atomic::AtomicBool>>,
}
/// Captures the original controller and its selection generation for an async
/// settings/catalog gesture. Commit never looks up a newer visible binding.
pub struct ComposerSettingsRequest {
    binding: Arc<RunBinding>,
    token: tekes_agent_presentation::SelectionToken,
}
impl ComposerSettingsRequest {
    pub fn destination(&self) -> &ComposerDestination {
        &self.token.destination
    }
    /// Settings are host-supplied explicit inputs. They are applied by the
    /// existing SubmissionPreparation hook at Send, never as an implicit RPC.
    /// Consume this request exactly once; cancellation simply drops it.
    pub fn commit(
        self,
        settings: std::collections::BTreeMap<String, serde_json::Value>,
    ) -> Result<bool> {
        self.binding.require_controls(&self.token.destination)?;
        Ok(self
            .binding
            .controller
            .as_ref()
            .expect("validated controller")
            .commit_settings(&self.token, settings, false))
    }
}
struct ScopedPreparation<'a> {
    binding: &'a RunBinding,
    host: &'a dyn SubmissionPreparation,
}
#[async_trait::async_trait]
impl SubmissionPreparation for ScopedPreparation<'_> {
    async fn prepare(
        &self,
        runtime: &SessionRuntime,
        ticket: &PreparedSubmission,
        scope: &RunScope,
    ) -> Result<()> {
        self.binding.require_controls(ticket.destination())?;
        self.host.prepare(runtime, ticket, scope).await?;
        // Host preparation may await model/auth/settings work. A pane that closed
        // in that interval cannot admit its earlier click after the await.
        self.binding.require_controls(ticket.destination())
    }
}
impl RunBinding {
    pub fn supports_creation(&self) -> bool {
        self.access() == PaneAccess::Ready
            && self.runtime.inspect(|s| {
                s.connection == tekes_agent_session::ConnectionState::Ready
                    && s.endpoint_capabilities.supports("session.create")
            })
    }
    /// Explicit host creation on this endpoint. Navigation remains host-owned;
    /// the captured action carries stable session/RPC identities.
    pub async fn create_initial_session(
        &self,
        creation: &InitialSessionCreation,
    ) -> Result<tekes_agent_contract::SessionCreated> {
        require(self.supports_creation(), "session creation unavailable")?;
        creation.execute(&self.runtime).await
    }
    fn require_destination(&self, destination: &ComposerDestination) -> Result<()> {
        require(
            destination == &ComposerDestination::run(self.scope.clone()),
            "pane destination mismatch",
        )?;
        require(
            self.access() != PaneAccess::Missing,
            "pane pointer missing or changed",
        )
    }
    fn require_controls(&self, destination: &ComposerDestination) -> Result<()> {
        self.require_destination(destination)?;
        require(
            self.access() == PaneAccess::Ready,
            "pane closing or outcome unknown",
        )?;
        require(self.controller.is_some(), "read-only bounded child")
    }
    /// Apply only the text patch captured by Composer; newer artifacts/settings
    /// remain in the canonical presentation draft.
    pub fn edit_text(&self, destination: &ComposerDestination, text: String) -> Result<()> {
        self.require_destination(destination)?;
        self.controller
            .as_ref()
            .ok_or_else(|| unsupported("read-only bounded child"))?
            .edit_text(destination, text)
    }
    /// Commit a host's explicit content replacement against its original capture.
    /// A text/settings edit or stream update is retained; changed context, scope,
    /// Send or retired pointer rejects the capture. No retry/merge is inferred.
    pub fn patch_context(
        &self,
        expected: &tekes_agent_presentation::ComposerContextBaseline,
        artifacts: Vec<tekes_agent_presentation::ComposerArtifact>,
        annotations: Vec<tekes_agent_presentation::ComposerAnnotation>,
    ) -> Result<bool> {
        self.require_controls(&expected.selection.destination)?;
        self.controller
            .as_ref()
            .expect("validated controller")
            .patch_context(expected, artifacts, annotations)
    }
    pub fn remove_context(
        &self,
        intent: &tekes_agent_components_ui::ComposerContextIntent,
    ) -> Result<bool> {
        use tekes_agent_components_ui::{ComposerContextAction, ComposerContextItem};
        require(
            intent.action == ComposerContextAction::Remove,
            "not a context removal",
        )?;
        let mut artifacts = intent.baseline.artifacts.clone();
        let mut annotations = intent.baseline.annotations.clone();
        match &intent.item {
            ComposerContextItem::Artifact(item) => {
                if !artifacts.contains(item) {
                    return Ok(false);
                }
                artifacts.retain(|a| a.id() != item.id());
            }
            ComposerContextItem::Annotation(item) => {
                if !annotations.contains(item) {
                    return Ok(false);
                }
                annotations.retain(|a| a.id != item.id);
            }
        }
        self.patch_context(&intent.baseline, artifacts, annotations)
    }
    /// Begin one settings gesture, invalidating an older catalog request through
    /// the existing presentation generation. Hosts must also fence navigation
    /// before presenting results and retain this exact request for completion.
    pub fn begin_settings(
        self: &Arc<Self>,
        destination: &ComposerDestination,
    ) -> Result<ComposerSettingsRequest> {
        self.require_controls(destination)?;
        let token = self
            .controller
            .as_ref()
            .expect("validated controller")
            .select(destination.clone())?;
        Ok(ComposerSettingsRequest {
            binding: self.clone(),
            token,
        })
    }
    /// One captured admission through RunController, without automatic retry.
    /// Model/auth/preset work is an explicit host callback.
    pub async fn send_with(
        &self,
        destination: &ComposerDestination,
        mode: PromptMode,
        host: &dyn SubmissionPreparation,
    ) -> Result<()> {
        let ticket = self.prepare_send(destination, mode)?;
        self.dispatch_prepared_with(ticket, host).await
    }
    /// Freeze admission on the UI thread before accepting another native input.
    /// Hosts then sync Composer's controlled clear before spawning dispatch.
    pub fn prepare_send(
        &self,
        destination: &ComposerDestination,
        mode: PromptMode,
    ) -> Result<PreparedSubmission> {
        self.require_controls(destination)?;
        self.controller
            .as_ref()
            .expect("validated controller")
            .prepare(destination, mode)
    }
    pub async fn dispatch_prepared_with(
        &self,
        ticket: PreparedSubmission,
        host: &dyn SubmissionPreparation,
    ) -> Result<()> {
        let controller = self
            .controller
            .as_ref()
            .ok_or_else(|| unsupported("read-only bounded child"))?;
        if let Err(error) = self.require_controls(ticket.destination()) {
            // Resolve an admitted local ticket if the pointer closed before the
            // async task began; otherwise its draft would stay reserved forever.
            controller.cancel_prepared(ticket)?;
            return Err(error);
        }
        controller
            .dispatch_with(
                ticket,
                None,
                &ScopedPreparation {
                    binding: self,
                    host,
                },
            )
            .await
            .map(|_| ())
    }
    /// Release a frozen admission that the host will not dispatch. This is local
    /// preparation cleanup, not cancellation of an already admitted remote run.
    pub fn cancel_prepared(&self, ticket: PreparedSubmission) -> Result<()> {
        require(
            ticket.destination() == &ComposerDestination::run(self.scope.clone()),
            "pane destination mismatch",
        )?;
        self.controller
            .as_ref()
            .ok_or_else(|| unsupported("read-only bounded child"))?
            .cancel_prepared(ticket)
    }
    pub async fn cancel(&self, destination: &ComposerDestination) -> Result<()> {
        self.require_controls(destination)?;
        self.controller
            .as_ref()
            .expect("validated controller")
            .cancel(&self.scope)
            .await
    }
    pub async fn open_main(runtime: Arc<SessionRuntime>, scope: RunScope) -> Result<Self> {
        require(
            scope.thread_id == scope.session_id,
            "main run requires main scope",
        )?;
        Self::open_session(runtime, scope).await
    }

    /// The host must resolve the task through TaskPanelState before mounting;
    /// loading or missing task rows never call this constructor with a fallback.
    pub async fn open_task(runtime: Arc<SessionRuntime>, task: &AgentTask) -> Result<Self> {
        Self::open_session(runtime, task.scope.clone()).await
    }

    async fn open_session(runtime: Arc<SessionRuntime>, scope: RunScope) -> Result<Self> {
        scope.validate()?;
        require(
            runtime.inspect(|s| s.endpoint_id == scope.endpoint_id),
            "run endpoint mismatch",
        )?;
        let lease = runtime.acquire_history(&scope.session_id).await?;
        let controller = RunController::new(runtime.clone())?;
        controller.select(ComposerDestination::run(scope.clone()))?;
        Ok(Self {
            runtime,
            scope,
            controller: Some(controller),
            _lease: Some(lease),
            child: None,
            side_chat: None,
            task: None,
            child_owner: None,
        })
    }

    pub async fn open_child(
        runtime: Arc<SessionRuntime>,
        parent: RunScope,
        child_id: impl Into<String>,
        layout: ChildHistoryLayout,
        source: Option<Arc<dyn ChildHistorySource>>,
    ) -> Result<Self> {
        let child_id = child_id.into();
        require(
            layout == ChildHistoryLayout::SameSession || source.is_none(),
            "custom child projection requires same-session layout",
        )?;
        let child = ChildRun::open(
            runtime.clone(),
            parent.clone(),
            child_id.clone(),
            layout,
            source,
        )
        .await?;
        let scope = RunScope {
            session_id: child_id.clone(),
            ..parent.clone()
        };
        let controller = if layout == ChildHistoryLayout::IndependentSessions {
            Some(RunController::for_child(
                runtime.clone(),
                &parent,
                child_id,
            )?)
        } else {
            None
        };
        Ok(Self {
            runtime,
            scope,
            controller,
            _lease: None,
            child: Some(child),
            side_chat: None,
            task: None,
            child_owner: None,
        })
    }

    pub async fn open_side_chat(
        runtime: Arc<SessionRuntime>,
        owner: Arc<SideChatController>,
        chat: &SideChat,
    ) -> Result<Self> {
        require(
            owner.uses_runtime(&runtime),
            "SideChat runtime authority mismatch",
        )?;
        require(
            Self::chat_access(&owner, chat) != PaneAccess::Missing,
            "SideChat pointer missing or foreign",
        )?;
        let mut binding = Self::open_main(runtime, chat.scope.clone()).await?;
        // Recheck after asynchronous baseline acquisition: close may have completed.
        require(
            Self::chat_access(&owner, chat) != PaneAccess::Missing,
            "SideChat pointer changed while mounting",
        )?;
        binding.side_chat = Some((owner, chat.clone()));
        Ok(binding)
    }

    fn chat_access(owner: &SideChatController, expected: &SideChat) -> PaneAccess {
        match owner.snapshot().chats.get(&expected.scope.session_id) {
            Some(chat) if chat.parent == expected.parent && chat.scope == expected.scope => {
                match chat.phase {
                    SideChatPhase::Ready => PaneAccess::Ready,
                    SideChatPhase::Closing => PaneAccess::Closing,
                    SideChatPhase::Unknown => PaneAccess::Unknown,
                }
            }
            _ => PaneAccess::Missing,
        }
    }
    pub fn scope(&self) -> &RunScope {
        &self.scope
    }
    /// Canonical synchronous input for normal run controls. Read-only/missing
    /// bodies have no Composer snapshot and must not become a blank new run.
    pub fn current_run(&self) -> Option<Arc<RunSnapshot>> {
        (self.access() != PaneAccess::Missing)
            .then(|| self.controller.as_ref().map(RunController::snapshot))
            .flatten()
    }
    pub fn access(&self) -> PaneAccess {
        if self.child_owner.as_ref().is_some_and(|owner| {
            !owner
                .upgrade()
                .is_some_and(|alive| alive.load(std::sync::atomic::Ordering::Acquire))
        }) {
            return PaneAccess::Missing;
        }
        if self.task.as_ref().is_some_and(|task| !task.live()) {
            return PaneAccess::Missing;
        }
        self.side_chat
            .as_ref()
            .map_or(PaneAccess::Ready, |(owner, chat)| {
                Self::chat_access(owner, chat)
            })
    }
    pub fn subscribe_runtime(&self) -> ProjectionSubscription {
        self.runtime.subscribe()
    }
    pub fn subscribe_run(&self) -> Option<watch::Receiver<Arc<RunSnapshot>>> {
        self.controller.as_ref().map(RunController::subscribe)
    }
    pub fn subscribe_side_chat(&self) -> Option<watch::Receiver<SideChatSnapshot>> {
        self.side_chat.as_ref().map(|(owner, _)| owner.subscribe())
    }
    pub async fn snapshot(&self) -> Result<PaneSnapshot> {
        let access = self.access();
        if access == PaneAccess::Missing {
            return Ok(PaneSnapshot::Missing {
                scope: self.scope.clone(),
            });
        }
        if let Some(controller) = &self.controller {
            return Ok(PaneSnapshot::Run {
                snapshot: controller.snapshot(),
                access,
            });
        }
        let transcript = self
            .child
            .as_ref()
            .ok_or_else(|| unsupported("missing child source"))?
            .transcript()
            .await?;
        let connection = self
            .runtime
            .inspect(|s| ConnectionPresentation::read(s, None, false));
        Ok(PaneSnapshot::ReadOnlyChild {
            scope: self.scope.clone(),
            connection,
            transcript,
        })
    }
    /// Reuse the existing catalog authority. A failed query leaves history readable
    /// but removes send authority in the existing presentation store.
    pub async fn refresh_child_catalog(&self) -> Result<()> {
        self.controller
            .as_ref()
            .ok_or_else(|| unsupported("read-only bounded child"))?
            .refresh_child_catalog(&self.scope)
            .await
    }
    pub fn sync_composer(
        &self,
        composer: &mut Composer,
        affordances: ComposerAffordances,
    ) -> Result<()> {
        require(
            self.access() != PaneAccess::Missing,
            "SideChat pointer missing",
        )?;
        let controller = self
            .controller
            .as_ref()
            .ok_or_else(|| unsupported("read-only bounded child"))?;
        composer
            .update_gated(
                &controller.snapshot(),
                affordances,
                self.access().composer_gate(),
            )
            .map_err(unsupported)
    }
    pub async fn load_earlier(&self) -> Result<bool> {
        require(
            self.access() != PaneAccess::Missing,
            "SideChat pointer missing",
        )?;
        if let Some(child) = &self.child {
            child.load_earlier().await
        } else {
            self.runtime.load_earlier(&self.scope.session_id).await
        }
    }
}
