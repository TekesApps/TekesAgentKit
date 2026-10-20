//! Original-run completed-turn actions over the existing SharedAgent dispatcher.
use crate::RunBinding;
use serde_json::json;
use std::sync::Arc;
use tekes_agent_components_ui::{TurnActionResult, TurnFootnoteIntent};
pub use tekes_agent_contract::ThreadActionOutcome;
use tekes_agent_contract::{Result, require, unsupported};
use tekes_agent_presentation::{
    ComposerDestination, SubmissionPhase, TranscriptSnapshot, TurnActionContext,
    present_turn_actions, session_has_active_work,
};
use tekes_agent_session::ConnectionState;
use tekes_agent_web::SharedAgent;

#[derive(Clone)]
pub struct TurnActionDispatcher {
    agent: Arc<SharedAgent>,
}
impl TurnActionDispatcher {
    pub fn new(agent: Arc<SharedAgent>) -> Self {
        Self { agent }
    }
    /// One gesture admits at most one owned request. Callers retain the RPC ID
    /// for explicit outcome reconciliation; neither preparation nor Drop retries.
    pub fn prepare(
        &self,
        source: Arc<RunBinding>,
        intent: &TurnFootnoteIntent,
        rpc_id: String,
    ) -> Result<TurnActionRequest> {
        require(
            intent.claim_dispatch(),
            "turn action gesture already claimed",
        )?;
        let result: Result<TurnActionRequest> = (|| {
            require(
                !rpc_id.trim().is_empty(),
                "turn action requires RPC identity",
            )?;
            validate(&self.agent, &source, intent)?;
            Ok(TurnActionRequest {
                agent: self.agent.clone(),
                source,
                intent: intent.clone(),
                rpc_id,
                dispatched: false,
            })
        })();
        if let Err(error) = &result {
            intent.complete(TurnActionResult::Rejected(error.to_string()));
        }
        result
    }
}
pub struct TurnActionRequest {
    agent: Arc<SharedAgent>,
    source: Arc<RunBinding>,
    intent: TurnFootnoteIntent,
    rpc_id: String,
    dispatched: bool,
}
impl TurnActionRequest {
    pub fn rpc_id(&self) -> &str {
        &self.rpc_id
    }
    pub async fn execute(mut self) -> Result<ThreadActionOutcome> {
        if let Err(error) = validate(&self.agent, &self.source, &self.intent) {
            self.intent
                .complete(TurnActionResult::Rejected(error.to_string()));
            return Err(error);
        }
        self.dispatched = true;
        let result:Result<ThreadActionOutcome>=self.agent.request(&json!({"endpoint":self.source.scope.endpoint_id,"op":"call","method":"thread.performAction","payload":{"sessionId":self.source.scope.session_id,"actionId":self.intent.action().id},"rpcId":self.rpc_id})).await.and_then(|value| serde_json::from_value(value).map_err(Into::into));
        match &result {
            Ok(outcome) => {
                self.intent
                    .complete(TurnActionResult::Completed(outcome.clone()));
            }
            Err(error) => {
                self.intent
                    .complete(TurnActionResult::Unknown(error.to_string()));
            }
        }
        result
    }
}
impl Drop for TurnActionRequest {
    fn drop(&mut self) {
        self.intent.complete(if self.dispatched {
            TurnActionResult::Unknown("操作派发后未取得结果".into())
        } else {
            TurnActionResult::Rejected("操作尚未派发".into())
        });
    }
}
fn validate(agent: &SharedAgent, source: &RunBinding, intent: &TurnFootnoteIntent) -> Result<()> {
    source.require_controls(&ComposerDestination::run(intent.scope().clone()))?;
    let endpoint = agent.endpoint(&source.scope.endpoint_id)?;
    require(
        Arc::ptr_eq(&endpoint.runtime, &source.runtime),
        "turn action runtime authority mismatch",
    )?;
    let run = source
        .current_run()
        .ok_or_else(|| unsupported("turn action run missing"))?;
    require(
        run.selection
            .as_ref()
            .is_some_and(|s| s.destination == ComposerDestination::run(source.scope.clone())),
        "turn action selection changed",
    )?;
    require(
        run.revision >= intent.publication() && run.connection_epoch == intent.epoch(),
        "turn action publication expired",
    )?;
    require(
        !run.submissions.iter().any(|s| {
            !s.reconciled
                && matches!(
                    s.phase,
                    SubmissionPhase::Prepared
                        | SubmissionPhase::Dispatched
                        | SubmissionPhase::Unknown
                )
        }),
        "submission outcome pending",
    )?;
    let state = source.runtime.snapshot();
    require(
        state.connection == ConnectionState::Ready && state.connection_epoch == intent.epoch(),
        "turn action connection expired",
    )?;
    let session = state
        .sessions
        .get(&source.scope.session_id)
        .ok_or_else(|| unsupported("turn action session missing"))?;
    let mut transcript = TranscriptSnapshot::read(&source.scope, session)?;
    present_turn_actions(
        &TurnActionContext {
            capabilities: &state.endpoint_capabilities,
            ready: true,
            running: session_has_active_work(session),
        },
        &mut transcript,
    );
    let turn = transcript
        .turns
        .iter()
        .find(|turn| turn.id == intent.turn().id)
        .ok_or_else(|| unsupported("turn action target missing"))?;
    require(
        turn.fork_boundary.is_some() && turn.fork_boundary == intent.turn().fork_boundary,
        "turn settlement boundary changed",
    )?;
    require(
        turn.footnote_actions
            .iter()
            .any(|definition| definition == intent.action() && definition.enabled),
        "turn action no longer available",
    )
}
