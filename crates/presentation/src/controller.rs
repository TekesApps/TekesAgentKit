use crate::*;
use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tekes_agent_contract::*;
use tekes_agent_session::SessionRuntime;
use tokio::sync::watch;

/// Product authentication/model/preset preparation is explicit and receives captured ownership.
#[async_trait]
pub trait SubmissionPreparation: Send + Sync {
    async fn prepare(
        &self,
        runtime: &SessionRuntime,
        ticket: &PreparedSubmission,
        scope: &RunScope,
    ) -> Result<()>;
}
struct NoPreparation;
#[async_trait]
impl SubmissionPreparation for NoPreparation {
    async fn prepare(
        &self,
        _: &SessionRuntime,
        ticket: &PreparedSubmission,
        _: &RunScope,
    ) -> Result<()> {
        require(
            ticket.settings().is_empty(),
            "draft settings require an explicit host preparation callback",
        )
    }
}
struct Shared {
    runtime: Arc<SessionRuntime>,
    core: Mutex<PresentationStore>,
    snapshots: watch::Sender<Arc<RunSnapshot>>,
    child_owner: Option<(RunScope, String)>,
}
impl Shared {
    fn validate_destination(&self, destination: &ComposerDestination) -> Result<()> {
        if let Some((scope, _)) = &self.child_owner {
            require(
                destination == &ComposerDestination::run(scope.clone()),
                "child controller target mismatch",
            )?;
        }
        Ok(())
    }
    fn child_parent<'a>(&'a self, scope: &'a RunScope) -> Result<Option<&'a str>> {
        if let Some((target, parent)) = &self.child_owner {
            require(target == scope, "child controller target mismatch")?;
            Ok(Some(parent))
        } else {
            Ok((scope.thread_id != scope.session_id).then_some(scope.thread_id.as_str()))
        }
    }
    fn refresh(&self) {
        let mut core = self.core.lock().expect("presentation lock poisoned");
        let snapshot = self.runtime.inspect(|authority| core.refresh(authority));
        let snapshot = match snapshot {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let mut old = (**self.snapshots.borrow()).clone();
                if old.selection != core.selection() {
                    old.transcript = TranscriptSnapshot::default();
                    old.summary = None;
                    old.run = None;
                }
                old.revision = core.next_snapshot_revision();
                old.selection = core.selection();
                old.draft = core
                    .selected
                    .as_ref()
                    .map(|d| core.draft(d))
                    .unwrap_or_default();
                old.draft_context_revision = core
                    .selected
                    .as_ref()
                    .map(|d| core.context_revision(d))
                    .unwrap_or_default();
                old.can_submit = false;
                old.connection.phase = ConnectionPhase::Degraded;
                old.connection.can_submit = false;
                old.connection.history_is_stale = true;
                old.connection.message = Some(error.to_string());
                old.transcript.is_stale = true;
                if let Some(run) = &mut old.run {
                    run.connection = old.connection.clone();
                    run.can_submit = false;
                    run.can_cancel = false;
                    run.phase = RunPhase::Unavailable;
                    run.actionables.clear();
                }
                old
            }
        };
        self.snapshots.send_replace(Arc::new(snapshot));
    }
    fn finish(&self, id: &str, outcome: std::result::Result<(), String>) {
        let _ = self
            .core
            .lock()
            .expect("presentation lock poisoned")
            .finish(id, outcome);
        self.refresh();
    }
}
/// Owns observation, not the SessionRuntime. Stopping a presentation leaves the harness connected.
pub struct RunController {
    shared: Arc<Shared>,
    observer: tokio::task::JoinHandle<()>,
}
impl Drop for RunController {
    fn drop(&mut self) {
        self.observer.abort();
    }
}
impl RunController {
    pub fn new(runtime: Arc<SessionRuntime>) -> Result<Self> {
        Self::new_with_child_owner(runtime, None)
    }
    /// An independent child may be owned by a nested parent, not the navigation thread root.
    /// History observation is separately owned by ChildRun/SessionHistoryLease.
    pub fn for_child(
        runtime: Arc<SessionRuntime>,
        parent: &RunScope,
        child: impl Into<String>,
    ) -> Result<Self> {
        parent.validate()?;
        let scope = RunScope {
            endpoint_id: parent.endpoint_id.clone(),
            thread_id: parent.thread_id.clone(),
            session_id: child.into(),
        };
        scope.validate()?;
        require(
            scope.endpoint_id == runtime.inspect(|store| store.endpoint_id.clone())
                && scope.session_id != parent.session_id
                && scope.session_id != scope.thread_id,
            "invalid child controller identity",
        )?;
        let controller =
            Self::new_with_child_owner(runtime, Some((scope.clone(), parent.session_id.clone())))?;
        controller.select(ComposerDestination::run(scope))?;
        Ok(controller)
    }
    fn new_with_child_owner(
        runtime: Arc<SessionRuntime>,
        child_owner: Option<(RunScope, String)>,
    ) -> Result<Self> {
        let mut core = PresentationStore::default();
        let initial = runtime.inspect(|authority| core.refresh(authority))?;
        let (snapshots, _) = watch::channel(Arc::new(initial));
        let shared = Arc::new(Shared {
            runtime: runtime.clone(),
            core: Mutex::new(core),
            snapshots,
            child_owner,
        });
        let weak = Arc::downgrade(&shared);
        let mut changes = runtime.subscribe();
        let observer = tokio::spawn(async move {
            while changes.changed().await.is_ok() {
                let Some(shared) = weak.upgrade() else {
                    break;
                };
                shared.refresh();
            }
        });
        Ok(Self { shared, observer })
    }
    pub fn subscribe(&self) -> watch::Receiver<Arc<RunSnapshot>> {
        self.shared.snapshots.subscribe()
    }
    pub fn snapshot(&self) -> Arc<RunSnapshot> {
        self.shared.snapshots.borrow().clone()
    }
    pub fn select(&self, destination: ComposerDestination) -> Result<SelectionToken> {
        self.shared.validate_destination(&destination)?;
        require(
            destination.endpoint_id() == self.shared.runtime.inspect(|s| s.endpoint_id.clone()),
            "controller endpoint mismatch",
        )?;
        let token = self
            .shared
            .core
            .lock()
            .expect("presentation lock poisoned")
            .select(destination)?;
        self.shared.refresh();
        Ok(token)
    }
    pub async fn open(&self, scope: RunScope) -> Result<()> {
        self.select(ComposerDestination::run(scope.clone()))?;
        self.shared.runtime.open_session(&scope.session_id).await?;
        if scope.thread_id != scope.session_id {
            self.refresh_child_catalog(&scope).await?;
        }
        Ok(())
    }
    pub async fn refresh_child_catalog(&self, scope: &RunScope) -> Result<()> {
        let token = self
            .snapshot()
            .selection
            .clone()
            .ok_or_else(|| unsupported("no selected child"))?;
        require(
            token.destination == ComposerDestination::run(scope.clone())
                && scope.thread_id != scope.session_id,
            "child catalog scope mismatch",
        )?;
        let epoch = self.shared.runtime.inspect(|s| s.connection_epoch);
        let parent = self
            .shared
            .child_parent(scope)?
            .ok_or_else(|| unsupported("no child owner"))?;
        let result = self.shared.runtime.subagent_catalog(parent).await;
        if epoch != self.shared.runtime.inspect(|s| s.connection_epoch) {
            return Err(Error::Disconnected);
        }
        self.shared.refresh();
        self.shared
            .core
            .lock()
            .expect("presentation lock poisoned")
            .commit_child_catalog(
                &token,
                epoch,
                result
                    .as_ref()
                    .map(Clone::clone)
                    .map_err(ToString::to_string),
            );
        self.shared.refresh();
        result.map(|_| ())
    }
    pub fn edit(&self, destination: &ComposerDestination, draft: ComposerDraft) -> Result<()> {
        self.shared.validate_destination(destination)?;
        self.shared
            .core
            .lock()
            .expect("presentation lock poisoned")
            .edit(destination, draft)?;
        self.shared.refresh();
        Ok(())
    }
    pub fn edit_text(&self, destination: &ComposerDestination, text: String) -> Result<()> {
        self.shared.validate_destination(destination)?;
        self.shared
            .core
            .lock()
            .expect("presentation lock poisoned")
            .edit_text(destination, text)?;
        self.shared.refresh();
        Ok(())
    }
    pub fn patch_context(
        &self,
        expected: &ComposerContextBaseline,
        artifacts: Vec<ComposerArtifact>,
        annotations: Vec<ComposerAnnotation>,
    ) -> Result<bool> {
        self.shared
            .validate_destination(&expected.selection.destination)?;
        let result = self
            .shared
            .core
            .lock()
            .expect("presentation lock poisoned")
            .patch_context(expected, artifacts, annotations)?;
        self.shared.refresh();
        Ok(result)
    }
    /// Commit an asynchronous upload's context only within its original
    /// connection epoch. Lock order matches refresh: core, then authority.
    /// The epoch check and context CAS share the same authority read guard.
    pub fn patch_context_at_epoch(
        &self,
        expected: &ComposerContextBaseline,
        artifacts: Vec<ComposerArtifact>,
        annotations: Vec<ComposerAnnotation>,
        connection_epoch: u64,
    ) -> Result<bool> {
        self.shared
            .validate_destination(&expected.selection.destination)?;
        let result = {
            let mut core = self.shared.core.lock().expect("presentation lock poisoned");
            self.shared.runtime.inspect(|authority| {
                if authority.connection_epoch != connection_epoch
                    || authority.connection != tekes_agent_session::ConnectionState::Ready
                {
                    return Ok(false);
                }
                core.patch_context(expected, artifacts, annotations)
            })?
        };
        self.shared.refresh();
        Ok(result)
    }
    pub fn commit_settings(
        &self,
        token: &SelectionToken,
        settings: std::collections::BTreeMap<String, Value>,
        cancelled: bool,
    ) -> bool {
        let result = self
            .shared
            .core
            .lock()
            .expect("presentation lock poisoned")
            .commit_settings(token, settings, cancelled);
        self.shared.refresh();
        result
    }
    pub fn prepare(
        &self,
        destination: &ComposerDestination,
        mode: PromptMode,
    ) -> Result<PreparedSubmission> {
        self.shared.validate_destination(destination)?;
        let result = {
            let mut core = self.shared.core.lock().expect("presentation lock poisoned");
            self.shared
                .runtime
                .inspect(|authority| core.prepare(destination, mode, authority))
        };
        self.shared.refresh();
        result
    }
    pub fn cancel_prepared(&self, ticket: PreparedSubmission) -> Result<()> {
        self.shared
            .core
            .lock()
            .expect("presentation lock poisoned")
            .cancel_prepared(&ticket)?;
        self.shared.refresh();
        Ok(())
    }
    pub fn acknowledge_reconciliation(&self, id: &str) -> Result<()> {
        self.shared
            .core
            .lock()
            .expect("presentation lock poisoned")
            .acknowledge_reconciliation(id)?;
        self.shared.refresh();
        Ok(())
    }
    /// The caller has confirmed discard or pointer absence, including any uncertain mutation.
    pub fn forget_destination(&self, destination: &ComposerDestination) {
        self.shared
            .core
            .lock()
            .expect("presentation lock poisoned")
            .forget_destination(destination);
        self.shared.refresh();
    }
    pub async fn dispatch(
        &self,
        ticket: PreparedSubmission,
        create: Option<CreateSessionRequest>,
    ) -> Result<Value> {
        self.dispatch_with(ticket, create, &NoPreparation).await
    }
    pub async fn dispatch_with(
        &self,
        ticket: PreparedSubmission,
        create: Option<CreateSessionRequest>,
        host: &dyn SubmissionPreparation,
    ) -> Result<Value> {
        self.shared
            .core
            .lock()
            .expect("presentation lock poisoned")
            .claim(&ticket)?;
        // A dropped/cancelled future also finishes on its captured owner. It never retries transport.
        let mut completion = Completion {
            shared: self.shared.clone(),
            id: ticket.id.clone(),
            finished: false,
        };
        let result=async {
            require(ticket.destination.endpoint_id()==self.shared.runtime.inspect(|s|s.endpoint_id.clone()),"dispatch endpoint mismatch")?;
            let scope=match &ticket.destination {
                ComposerDestination::Run{scope}=>{require(create.is_none(),"existing Run cannot create another session")?;scope.clone()},
                ComposerDestination::NewThread{workspace_id,endpoint_id}=>{
                    let mut request=create.ok_or_else(||unsupported("new thread requires explicit host create inputs"))?;
                    require(request.workspace_id.as_ref().is_none_or(|id|id==workspace_id),"create workspace mismatch")?;
                    if request.session_id.is_none(){request.session_id=Some(uuid::Uuid::new_v4().to_string());}
                    let created=self.shared.runtime.create(request,&format!("create:{}",ticket.id)).await?;
                    RunScope::main(endpoint_id.clone(),created.session_id)
                }
            };
            self.shared.core.lock().expect("presentation lock poisoned").materialize(&ticket,scope.clone())?;
            self.shared.refresh();
            host.prepare(&self.shared.runtime,&ticket,&scope).await?;
            self.shared.runtime.open_session(&scope.session_id).await?;
            let child_parent = self.shared.child_parent(&scope)?;
            if let Some(parent) = child_parent {
                require(matches!(ticket.plan, SubmissionPlan::Prompt { .. }), "child commands unavailable")?;
                self.shared.runtime.subagent_catalog(parent).await?.require_continuable(&scope.session_id)?;
            }
            self.shared.core.lock().expect("presentation lock poisoned").mark_dispatched(&ticket)?;
            let value=match &ticket.plan {
                SubmissionPlan::Prompt{content,..} if child_parent.is_some() => self.shared.runtime.prompt_child(child_parent.expect("child owner"), &scope.session_id, ticket.mode, content.clone(), &ticket.id).await?,
                SubmissionPlan::Prompt{content,..}=>self.shared.runtime.submit(PromptRequest{session_id:scope.session_id.clone(),mode:ticket.mode,content:content.clone(),client_time_zone:None},&ticket.id).await?,
                SubmissionPlan::Command{name,arguments,attachments}=>self.shared.runtime.call_optional("commands/run",json!({"sessionId":scope.session_id,"name":name,"arguments":arguments,"attachments":attachments}),&ticket.id).await?,
            };
            if matches!(ticket.plan,SubmissionPlan::Prompt{..}) {require(value["accepted"]==true,"prompt admission was not confirmed")?;}
            Ok(value)
        }.await;
        completion.finished = true;
        self.shared.finish(
            &ticket.id,
            result.as_ref().map(|_| ()).map_err(ToString::to_string),
        );
        result
    }
    pub async fn load_earlier(&self, scope: &RunScope) -> Result<bool> {
        require(
            scope.endpoint_id == self.shared.runtime.inspect(|s| s.endpoint_id.clone()),
            "history endpoint mismatch",
        )?;
        self.shared.runtime.load_earlier(&scope.session_id).await
    }
    pub async fn cancel(&self, scope: &RunScope) -> Result<()> {
        let valid = self
            .shared
            .runtime
            .inspect(|s| RunPresentation::read(s, scope, true).can_cancel);
        require(valid, "Run is not cancellable")?;
        if let Some(parent) = self.shared.child_parent(scope)? {
            self.shared
                .runtime
                .cancel_child(parent, &scope.session_id, &uuid::Uuid::new_v4().to_string())
                .await
        } else {
            self.shared
                .runtime
                .cancel(&scope.session_id, &uuid::Uuid::new_v4().to_string())
                .await
        }
    }
    pub async fn respond(
        &self,
        scope: &RunScope,
        id: &str,
        revision: i64,
        outcome: Value,
    ) -> Result<()> {
        require(
            scope.endpoint_id == self.shared.runtime.inspect(|s| s.endpoint_id.clone()),
            "actionable endpoint mismatch",
        )?;
        require(
            self.shared.runtime.inspect(|s| {
                s.actionables
                    .get(id)
                    .is_some_and(|a| a["sessionId"] == scope.session_id)
            }),
            "actionable scope mismatch",
        )?;
        self.shared.runtime.respond(id, revision, outcome).await
    }
}
struct Completion {
    shared: Arc<Shared>,
    id: String,
    finished: bool,
}
impl Drop for Completion {
    fn drop(&mut self) {
        if !self.finished {
            self.shared.finish(
                &self.id,
                Err("submission task cancelled; reconcile if dispatched".into()),
            );
        }
    }
}
