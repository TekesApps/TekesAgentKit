//! Host routing only: original widget/binding, committed frame and delayed receipt.
use super::*;
use tekes_agent_ui_bindings::{
    FeedbackFrame, FeedbackUiAdoption, FeedbackUiCompletion, FeedbackUiRequest,
};
pub(super) struct FeedbackPane {
    pub source: std::sync::Weak<RunBinding>,
    pub frame: Option<FeedbackFrame>,
    attempted_epoch: Option<u64>,
    loading_epoch: Option<u64>,
    failure: Option<String>,
    initial_live: Option<Arc<()>>,
}
/// Original-binding read, scheduled once per mounted source/connection epoch.
/// Execution never follows the host's current selection.
pub struct FeedbackInitialRequest {
    pane: ComponentId,
    source: Arc<RunBinding>,
    epoch: u64,
    live: std::sync::Weak<()>,
}
pub struct FeedbackInitialCompletion {
    request: FeedbackInitialRequest,
    result: std::result::Result<FeedbackFrame, String>,
}
impl FeedbackInitialRequest {
    pub async fn execute(self) -> FeedbackInitialCompletion {
        let result = if self.live.upgrade().is_none()
            || self.source.feedback_read_epoch() != Some(self.epoch)
        {
            Err("feedback initial source changed before execution".into())
        } else {
            self.source
                .read_feedback_at_epoch(self.epoch)
                .await
                .map_err(|e| e.to_string())
        };
        FeedbackInitialCompletion {
            request: self,
            result,
        }
    }
}
pub struct FeedbackDemoRequest {
    pane: ComponentId,
    control: ComponentId,
    request: FeedbackUiRequest,
}
pub struct FeedbackDemoCompletion {
    pane: ComponentId,
    control: ComponentId,
    completion: FeedbackUiCompletion,
}
impl FeedbackDemoRequest {
    pub async fn execute(self) -> FeedbackDemoCompletion {
        FeedbackDemoCompletion {
            pane: self.pane,
            control: self.control,
            completion: self.request.execute().await,
        }
    }
}
impl ComponentDemo {
    pub(super) fn feedback_labels(
        &mut self,
        ids: ComposerIds,
        source: &Arc<RunBinding>,
    ) -> Option<tekes_agent_components_ui::FeedbackLabels> {
        let current = self
            .feedback_panes
            .get(&ids.root)
            .and_then(|pane| pane.source.upgrade())
            .is_some_and(|original| Arc::ptr_eq(&original, source));
        if source.supports_feedback() || current {
            if !current {
                self.feedback_panes.insert(
                    ids.root,
                    FeedbackPane {
                        source: Arc::downgrade(source),
                        frame: None,
                        attempted_epoch: None,
                        loading_epoch: None,
                        failure: None,
                        initial_live: None,
                    },
                );
            }
            Some(Default::default())
        } else {
            self.feedback_panes.remove(&ids.root);
            None
        }
    }
    pub(super) fn prepare_feedback(
        &mut self,
        ids: ComposerIds,
        source: &Arc<RunBinding>,
        control: ComponentId,
        gesture: tekes_agent_components_ui::FeedbackGesture,
    ) -> Result<DemoCommand> {
        let pane = self
            .feedback_panes
            .get_mut(&ids.root)
            .ok_or_else(|| unsupported("feedback source unavailable"))?;
        require(
            pane.source
                .upgrade()
                .is_some_and(|original| Arc::ptr_eq(&original, source)),
            "feedback binding changed",
        )?;
        // Explicit gestures supersede the queued initial read, including a
        // failure that could otherwise overwrite a successful manual refresh.
        pane.loading_epoch = None;
        pane.initial_live = None;
        pane.failure = None;
        self.background_commands.retain(|command| {
            !matches!(command, DemoCommand::ReadInitialFeedback(request) if request.pane == ids.root)
        });
        let request = source.prepare_feedback_gesture(pane.frame.as_ref(), gesture);
        Ok(DemoCommand::Feedback(Box::new(FeedbackDemoRequest {
            pane: ids.root,
            control,
            request,
        })))
    }
    /// Hosts producing a static snapshot may execute just the initial metadata
    /// reads. Leave every unrelated background operation in its original queue.
    pub fn take_initial_feedback_requests(&mut self) -> Vec<FeedbackInitialRequest> {
        let mut requests = vec![];
        for command in std::mem::take(&mut self.background_commands) {
            match command {
                DemoCommand::ReadInitialFeedback(request) => requests.push(*request),
                other => self.background_commands.push(other),
            }
        }
        requests
    }
    pub fn complete_initial_feedback(&mut self, result: FeedbackInitialCompletion) -> Result<()> {
        let request = result.request;
        let Some(pane) = self.feedback_panes.get_mut(&request.pane) else {
            return Ok(());
        };
        if pane
            .initial_live
            .as_ref()
            .is_none_or(|live| !std::sync::Weak::ptr_eq(&Arc::downgrade(live), &request.live))
            || pane.loading_epoch != Some(request.epoch)
            || request.source.feedback_read_epoch() != Some(request.epoch)
            || !pane
                .source
                .upgrade()
                .is_some_and(|source| Arc::ptr_eq(&source, &request.source))
        {
            return Ok(());
        }
        pane.loading_epoch = None;
        pane.initial_live = None;
        match result.result {
            Ok(frame) if frame.is_current(&request.source) => {
                pane.frame = Some(frame);
                pane.failure = None;
            }
            Ok(_) => {} // A newer publication owns the result.
            Err(error) if pane.frame.is_none() => pane.failure = Some(error),
            Err(_) => {}
        }
        self.sync()
    }
    pub fn complete_feedback(&mut self, result: FeedbackDemoCompletion) -> Result<()> {
        self.feedback_completions.push(result);
        self.sync()
    }
    pub(super) fn sync_feedback(
        &mut self,
        ids: ComposerIds,
        source: &Arc<RunBinding>,
    ) -> Result<()> {
        let Some(root) = self.transcript_content_id(ids) else {
            return Ok(());
        };
        let completions = std::mem::take(&mut self.feedback_completions);
        self.feedback_panes
            .retain(|_, pane| pane.source.strong_count() > 0);
        for result in completions {
            if !self.feedback_panes.contains_key(&result.pane) {
                continue;
            }
            if result.pane != ids.root {
                self.feedback_completions.push(result);
                continue;
            }
            if self.tree.get(result.control).is_none() {
                continue;
            }
            let Some(pane) = self.feedback_panes.get_mut(&ids.root) else {
                continue;
            };
            if !pane
                .source
                .upgrade()
                .is_some_and(|original| Arc::ptr_eq(&original, source))
            {
                continue;
            }
            let Some(control) = self
                .tree
                .get_mut(result.control)
                .and_then(|node| node.component_mut::<tekes_agent_components_ui::TurnFeedback>())
            else {
                continue;
            };
            if let FeedbackUiAdoption::Adopted(frame) = result.completion.apply(control) {
                pane.frame = frame;
            }
        }
        let Some(pane) = self.feedback_panes.get_mut(&ids.root) else {
            return Ok(());
        };
        if pane
            .frame
            .as_ref()
            .is_some_and(|frame| !frame.is_current(source))
        {
            pane.frame = None;
        }
        if let Some(epoch) = source.feedback_read_epoch()
            && pane.attempted_epoch != Some(epoch)
        {
            pane.attempted_epoch = Some(epoch);
            pane.loading_epoch = Some(epoch);
            pane.failure = None;
            let live = Arc::new(());
            pane.initial_live = Some(live.clone());
            self.background_commands
                .push(DemoCommand::ReadInitialFeedback(Box::new(
                    FeedbackInitialRequest {
                        pane: ids.root,
                        source: source.clone(),
                        epoch,
                        live: Arc::downgrade(&live),
                    },
                )));
        }
        if pane.frame.is_none() {
            let content = self
                .tree
                .get_mut(root)
                .unwrap()
                .component_mut::<tekes_agent_components_ui::TranscriptContent>()
                .unwrap();
            let ids: Vec<_> = content
                .feedback_controls()
                .filter_map(|(id, control)| (!control.is_pending()).then_some(id))
                .collect();
            for id in ids {
                let control = content.feedback_mut(id).unwrap();
                control.invalidate_authority();
                if let Some(error) = &pane.failure {
                    control.initial_read_failed(error.clone());
                }
            }
        }
        if let Some(frame) = &pane.frame {
            let content = self
                .tree
                .get_mut(root)
                .unwrap()
                .component_mut::<tekes_agent_components_ui::TranscriptContent>()
                .unwrap();
            let targets: Vec<_> = content
                .feedback_controls()
                // Preserve retained widgets without repeating the full protocol
                // projection for every output on each native status/draw tick.
                .filter(|(_, control)| {
                    control.state().is_none_or(|state| {
                        state.publication != frame.publication()
                            || state.connection_epoch != frame.connection_epoch()
                    })
                })
                .filter_map(|(id, control)| control.target().cloned().map(|target| (id, target)))
                .collect();
            for (id, target) in targets {
                if let Ok(state) = frame.controls(source, &target) {
                    content
                        .feedback_mut(id)
                        .unwrap()
                        .update(state)
                        .map_err(unsupported)?;
                }
            }
        }
        Ok(())
    }
}
