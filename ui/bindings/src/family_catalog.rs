//! Read-only parent catalog adapter over the existing ChildCatalogState.
//! Control/send authority remains in RunBinding/ChildPaneBinding and the SEP.
use crate::{PaneAccess, RunBinding};
use std::sync::{Arc, Mutex, Weak};
use tekes_agent_components_ui::{FamilyCatalogInput, FamilyCatalogIntent};
use tekes_agent_contract::{Result, require, unsupported};
use tekes_agent_presentation::{ChildCatalogState, RunScope};
use tekes_agent_session::{ConnectionState, SessionRuntime};
use tokio::sync::watch;

struct State {
    model: ChildCatalogState,
    input: FamilyCatalogInput,
}
pub struct FamilyCatalogBinding {
    parent: Weak<RunBinding>,
    scope: RunScope,
    runtime: Arc<SessionRuntime>,
    state: Mutex<State>,
    changes: watch::Sender<FamilyCatalogInput>,
}
impl FamilyCatalogBinding {
    pub fn for_run(parent: &Arc<RunBinding>) -> Self {
        let input = FamilyCatalogInput {
            source: Default::default(),
            parent: parent.scope().clone(),
            connection_epoch: 0,
            generation: 0,
            revision: 0,
            refreshing: false,
            catalog: None,
            error: None,
        };
        Self {
            parent: Arc::downgrade(parent),
            scope: parent.scope().clone(),
            runtime: parent.runtime.clone(),
            changes: watch::channel(input.clone()).0,
            state: Mutex::new(State {
                model: ChildCatalogState::default(),
                input,
            }),
        }
    }
    pub fn belongs_to(&self, parent: &Arc<RunBinding>) -> bool {
        Weak::ptr_eq(&self.parent, &Arc::downgrade(parent))
    }
    pub fn parent(&self) -> &RunScope {
        &self.scope
    }
    fn ready_epoch(&self) -> Option<u64> {
        let parent = self.parent.upgrade()?;
        if parent.access() != PaneAccess::Ready || parent.current_run().is_none() {
            return None;
        }
        self.runtime.inspect(|state| {
            matches!(state.connection, ConnectionState::Ready).then_some(state.connection_epoch)
        })
    }
    fn publish(&self, state: &mut State, epoch: u64) {
        state.input = FamilyCatalogInput {
            source: state.input.source.clone(),
            parent: self.scope.clone(),
            connection_epoch: epoch,
            generation: state.model.generation(),
            revision: state
                .input
                .revision
                .checked_add(1)
                .expect("family publication overflow"),
            refreshing: state.model.refreshing,
            catalog: state.model.catalog.clone(),
            error: state.model.error.clone(),
        };
        self.changes.send_replace(state.input.clone());
    }
    pub fn snapshot(&self) -> FamilyCatalogInput {
        let epoch = self.ready_epoch();
        let mut state = self.state.lock().expect("family catalog lock");
        if state.model.generation() > 0
            && (epoch != Some(state.input.connection_epoch))
            && (state.model.catalog.is_some()
                || state.model.refreshing
                || state.model.error.is_none())
        {
            state.model.begin(self.scope.clone(), false);
            state.model.error = Some("Family catalog source changed; refresh explicitly".into());
            let current = self.runtime.inspect(|snapshot| snapshot.connection_epoch);
            self.publish(&mut state, current);
        }
        state.input.clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<FamilyCatalogInput> {
        self.changes.subscribe()
    }
    /// One explicit read. No polling, retry, child creation or remote cancellation.
    pub async fn refresh(&self) -> Result<()> {
        let epoch = self.ready_epoch().ok_or_else(|| {
            self.snapshot();
            unsupported("family parent source unavailable")
        })?;
        let token = {
            let mut state = self.state.lock().expect("family catalog lock");
            let token = state
                .model
                .begin(self.scope.clone(), true)
                .expect("connected catalog");
            self.publish(&mut state, epoch);
            token
        };
        let result = self.runtime.subagent_catalog(&self.scope.session_id).await;
        let live = self.ready_epoch() == Some(epoch);
        let mut state = self.state.lock().expect("family catalog lock");
        let committed = state.model.commit(
            &token,
            if live {
                result
                    .as_ref()
                    .map(Clone::clone)
                    .map_err(ToString::to_string)
            } else {
                Err("Family parent or connection changed during read".into())
            },
        );
        if committed {
            self.publish(&mut state, epoch);
        }
        require(live && committed, "stale family catalog read")?;
        result.map(|_| ())
    }
    /// Additional source admission after the original widget validates its
    /// instance/gesture. This method grants no child send/cancel capability.
    pub fn accepts(&self, intent: &FamilyCatalogIntent) -> bool {
        let input = self.snapshot();
        self.ready_epoch() == Some(input.connection_epoch)
            && input.source == intent.source
            && input.parent == intent.parent
            && input.connection_epoch == intent.connection_epoch
            && input.generation == intent.generation
            && input.revision == intent.revision
            && !input.refreshing
            && input.error.is_none()
            && input
                .catalog
                .as_ref()
                .and_then(|catalog| catalog.child(&intent.child_id))
                .is_some()
    }
}
