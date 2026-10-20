//! Agent composition only. Presentation owns the Run; TekesUI owns its viewport.
use crate::*;
use std::ops::Range;
use std::sync::Arc;
use tekes_agent_presentation::ActionablePrompt;
use tekes_ui_controls::layout::MeasuredChild;
use tekes_ui_scene::Size;

/// Reserve the whole Composer namespace plus the three extra IDs, and distinct
/// namespaces for paging/actionables/context/transcript. Hosts allocate these IDs.
#[derive(Clone, Copy, Debug)]
pub struct RunViewIds {
    pub root: ComponentId,
    pub indicators: ComponentId,
    pub viewport: ComponentId,
    pub composer: ComposerIds,
}
#[derive(Debug)]
pub enum RunViewIntent {
    Composer(ComposerIntent),
    LoadEarlier {
        scope: RunScope,
    },
    Actionable(ActionableResponseIntent),
    TurnAction(Box<TurnFootnoteIntent>),
    CopyTranscript(TranscriptCopyIntent),
    Feedback {
        control: ComponentId,
        gesture: Box<FeedbackGesture>,
    },
}
/// Experimental reusable Run composition. Mount one per original binding; it
/// cannot be retargeted. A complete Transcript requires upstream reading/rich APIs.
#[derive(Debug)]
pub struct RunView {
    ids: RunViewIds,
    scope: RunScope,
    paging: ComponentId,
    actionable: ComponentId,
    transcript: Option<ComponentId>,
    body: StackContent,
    last: Option<RunSnapshot>,
    gate: ComposerControlGate,
    label: String,
}
impl RunView {
    /// Four consecutive namespaces for controls/paging/actionables/transcript;
    /// the host supplies a separate exclusive row pool. Default style is TekesUI.
    pub fn standard(
        namespace: u32,
        rows: Range<u32>,
        scope: RunScope,
    ) -> Result<Self, &'static str> {
        let transcript = namespace
            .checked_add(3)
            .ok_or("RunView namespace overflow")?;
        let base = (namespace as u64) << 32;
        let composer = ComposerIds::new(base + 16);
        Self::new(
            RunViewIds {
                root: ComponentId(base),
                indicators: ComponentId(base + 1),
                viewport: ComponentId(base + 2),
                composer,
            },
            scope,
            Composer::new(composer),
            namespace + 1,
            namespace + 2,
        )?
        .with_transcript(transcript, rows, Default::default())
    }
    pub fn new(
        ids: RunViewIds,
        scope: RunScope,
        composer: Composer,
        paging_namespace: u32,
        actionable_namespace: u32,
    ) -> Result<Self, &'static str> {
        scope.validate().map_err(|_| "invalid RunView scope")?;
        if composer
            .destination()
            .is_some_and(|destination| destination != &ComposerDestination::run(scope.clone()))
        {
            return Err("RunView composer source mismatch");
        }
        if ids.composer != composer.ids() {
            return Err("RunView composer IDs mismatch");
        }
        let composer_namespace = ids.composer.root.0 >> 32;
        if ids.composer.preset.0 >> 32 != composer_namespace
            || paging_namespace == actionable_namespace
            || [paging_namespace as u64, actionable_namespace as u64].contains(&composer_namespace)
        {
            return Err("RunView namespace overlap");
        }
        let reserved = [ids.root, ids.indicators, ids.viewport];
        if reserved.iter().enumerate().any(|(i, id)| {
            reserved[..i].contains(id)
                || (ids.composer.root.0..=ids.composer.preset.0).contains(&id.0)
                || [paging_namespace as u64, actionable_namespace as u64].contains(&(id.0 >> 32))
        }) {
            return Err("RunView control IDs overlap");
        }
        // Include a configured context's hidden IDs, even while its cards are empty.
        if composer.context().is_some_and(|context| {
            let ns = context.id().0 >> 32;
            [paging_namespace as u64, actionable_namespace as u64].contains(&ns)
                || reserved.iter().any(|id| id.0 >> 32 == ns)
        }) {
            return Err("RunView context namespace overlap");
        }
        let paging = TranscriptPaging::new(paging_namespace, scope.clone(), Default::default());
        let paging_id = ComponentId((paging_namespace as u64) << 32);
        let actionable = ActionableCard::new(actionable_namespace, Default::default());
        let actionable_id = ComponentId((actionable_namespace as u64) << 32);
        Ok(Self {
            ids,
            scope,
            paging: paging_id,
            actionable: actionable_id,
            transcript: None,
            last: None,
            gate: ComposerControlGate::CLOSED,
            label: "Agent Run".into(),
            body: StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Regular.value(),
                },
                flexible_child: None,
                children: vec![
                    Content::new(ids.indicators, Indicators::default()),
                    Content::new(paging_id, paging),
                    Content::new(actionable_id, actionable),
                    Content::new(ids.composer.root, composer),
                ],
            },
        })
    }
    pub fn set_label(&mut self, label: impl Into<String>) {
        self.label = label.into();
    }
    pub fn ids(&self) -> RunViewIds {
        self.ids
    }
    pub fn scope(&self) -> &RunScope {
        &self.scope
    }
    pub fn transcript_id(&self) -> Option<ComponentId> {
        self.transcript
    }
    pub fn composer(&self) -> &Composer {
        self.body
            .children
            .iter()
            .find_map(|node| node.component::<Composer>())
            .unwrap()
    }
    /// Before delivering pointer/AX focus changes, hosts keep composition with
    /// its current editor. Text/key events still go through TekesUI unchanged.
    pub fn allows_focus_target(&self, target: ComponentId) -> bool {
        !self.composer().is_composing() || target == self.ids.composer.editor
    }
    pub fn composer_mut(&mut self) -> &mut Composer {
        self.body
            .children
            .iter_mut()
            .find_map(|node| node.component_mut::<Composer>())
            .unwrap()
    }
    pub fn transcript(&self) -> Option<&TranscriptContent> {
        self.body
            .children
            .iter()
            .find_map(|node| node.component::<ScrollContent>())?
            .children()
            .iter()
            .find_map(|node| node.component::<TranscriptContent>())
    }
    fn transcript_mut(&mut self) -> Option<&mut TranscriptContent> {
        self.body
            .children
            .iter_mut()
            .find_map(|node| node.component_mut::<ScrollContent>())?
            .children_mut()?
            .iter_mut()
            .find_map(|node| node.component_mut::<TranscriptContent>())
    }
    /// Install before rendering; never replace a mounted transcript (selection,
    /// disclosure and operation receipts belong to that component's lifetime).
    pub fn install_transcript(
        &mut self,
        mut content: TranscriptContent,
    ) -> Result<(), &'static str> {
        if self.transcript.is_some() {
            return Err("RunView transcript already mounted");
        }
        if content.scope() != &self.scope {
            return Err("RunView transcript scope mismatch");
        }
        let mut reserved: Vec<_> = self
            .body
            .children
            .iter()
            .map(|node| (node.id().0 >> 32) as u32)
            .collect();
        reserved.extend(
            [self.ids.root, self.ids.indicators, self.ids.viewport].map(|id| (id.0 >> 32) as u32),
        );
        if let Some(context) = self.composer().context() {
            reserved.push((context.id().0 >> 32) as u32);
        }
        if !content.namespaces_disjoint(&reserved) {
            return Err("RunView transcript namespace overlap");
        }
        if let Some(snapshot) = &self.last {
            content.update(
                &self.scope,
                snapshot.revision,
                snapshot.connection_epoch,
                &snapshot.transcript,
                self.gate.entries && !self.composer().is_composing(),
            )?;
        }
        self.transcript = Some(content.id());
        let viewport = ScrollContent::new(
            tekes_ui_controls::ScrollArea::default(),
            Content::new(content.id(), content),
        );
        let index = self
            .body
            .children
            .iter()
            .position(|node| node.id() == self.ids.composer.root)
            .unwrap();
        self.body
            .children
            .insert(index, Content::new(self.ids.viewport, viewport));
        self.body.flexible_child = Some(self.ids.viewport);
        Ok(())
    }
    /// Convenience construction with an exclusive host-supplied transcript pool.
    pub fn with_transcript(
        mut self,
        namespace: u32,
        rows: Range<u32>,
        labels: TranscriptContentLabels,
    ) -> Result<Self, &'static str> {
        let content = TranscriptContent::new(namespace, rows, self.scope.clone(), labels)?;
        self.install_transcript(content)?;
        Ok(self)
    }
    /// Opt into evaluation controls for independently addressable sources with
    /// public feedback capability. Keep None for bounded read-only child history.
    pub fn configure_feedback(
        &mut self,
        labels: Option<FeedbackLabels>,
    ) -> Result<(), &'static str> {
        self.transcript_mut()
            .ok_or("RunView has no Transcript")?
            .configure_feedback(labels)
    }
    /// Apply one frame, including per-binding read/response authority. The
    /// controls flag only restricts canonical actions; dispatch still revalidates.
    pub fn update(
        &mut self,
        snapshot: &RunSnapshot,
        affordances: ComposerAffordances,
        gate: ComposerControlGate,
        paging: TranscriptPagingFacts,
        actionable_controls: bool,
    ) -> Result<(), &'static str> {
        if paging.scope != self.scope
            || paging.has_more_before != snapshot.transcript.has_more_before
            || paging.is_stale != snapshot.transcript.is_stale
        {
            return Err("RunView paging input mismatch");
        }
        let prompt = snapshot
            .run
            .as_ref()
            .and_then(|run| run.actionables.first())
            .map(|value| ActionablePrompt::read(self.scope.clone(), value.clone()).map(Arc::new))
            .transpose()
            .map_err(|_| "invalid RunView actionable")?;
        self.body
            .children
            .iter()
            .find_map(|node| node.component::<ActionableCard>())
            .unwrap()
            .check_update(
                snapshot.revision,
                snapshot.connection_epoch,
                prompt.as_ref(),
            )?;
        self.update_run(snapshot, affordances, gate)?;
        self.update_paging(paging)?;
        self.update_actionable(
            snapshot.revision,
            snapshot.connection_epoch,
            prompt,
            actionable_controls,
        )?;
        Ok(())
    }
    /// One paired canonical publication updates all Run content. This method
    /// neither subscribes, dispatches nor caches a second Run state.
    pub(crate) fn update_run(
        &mut self,
        snapshot: &RunSnapshot,
        affordances: ComposerAffordances,
        gate: ComposerControlGate,
    ) -> Result<(), &'static str> {
        if snapshot.revision == 0
            || self.last.as_ref().is_some_and(|old| {
                snapshot.revision < old.revision
                    || snapshot.connection_epoch < old.connection_epoch
                    || (snapshot.revision == old.revision && snapshot != old)
            })
        {
            return Err("stale or conflicting RunView input");
        }
        let target = ComposerDestination::run(self.scope.clone());
        if snapshot
            .selection
            .as_ref()
            .is_none_or(|selection| selection.destination != target)
            || snapshot
                .run
                .as_ref()
                .is_some_and(|run| run.scope != self.scope)
        {
            return Err("RunView source mismatch");
        }
        let scope = self.scope.clone();
        let available = gate.entries && !self.composer().is_composing();
        if let Some(content) = self.transcript_mut() {
            content.validate_update(
                &scope,
                snapshot.revision,
                snapshot.connection_epoch,
                &snapshot.transcript,
                available,
            )?;
        }
        self.composer_mut()
            .update_gated(snapshot, affordances, gate)?;
        let available = gate.entries && !self.composer().is_composing();
        self.body
            .children
            .iter_mut()
            .find_map(|node| node.component_mut::<Indicators>())
            .unwrap()
            .update(snapshot)?;
        if let Some(content) = self.transcript_mut() {
            content.update(
                &scope,
                snapshot.revision,
                snapshot.connection_epoch,
                &snapshot.transcript,
                available,
            )?;
        }
        self.last = Some(snapshot.clone());
        self.gate = gate;
        Ok(())
    }
    pub fn update_paging(&mut self, mut facts: TranscriptPagingFacts) -> Result<(), &'static str> {
        if self.last.as_ref().is_none_or(|snapshot| {
            facts.scope != self.scope
                || facts.has_more_before != snapshot.transcript.has_more_before
                || facts.is_stale != snapshot.transcript.is_stale
        }) {
            return Err("RunView paging input mismatch");
        }
        facts.source_ready &= self.gate.entries;
        self.body
            .children
            .iter_mut()
            .find_map(|node| node.component_mut::<TranscriptPaging>())
            .unwrap()
            .update(facts)
    }
    /// Advanced authority refresh against the already installed canonical frame.
    pub fn update_actionable(
        &mut self,
        publication: u64,
        epoch: u64,
        prompt: Option<Arc<ActionablePrompt>>,
        controls: bool,
    ) -> Result<bool, &'static str> {
        if prompt
            .as_ref()
            .is_some_and(|prompt| prompt.scope() != &self.scope)
        {
            return Err("RunView actionable source mismatch");
        }
        if self.last.as_ref().is_none_or(|snapshot| {
            snapshot.revision != publication || snapshot.connection_epoch != epoch
        }) {
            return Err("RunView actionable publication mismatch");
        }
        let canonical = self
            .last
            .as_ref()
            .and_then(|snapshot| snapshot.run.as_ref())
            .and_then(|run| run.actionables.first());
        if prompt.as_ref().map(|prompt| prompt.source()) != canonical {
            return Err("RunView actionable facts mismatch");
        }
        let controls = controls && self.gate.entries && !self.composer().is_composing();
        self.body
            .children
            .iter_mut()
            .find_map(|node| node.component_mut::<ActionableCard>())
            .unwrap()
            .update(publication, epoch, prompt, controls)
    }
    /// Drain once immediately after ContentHost event/AX delivery, before update.
    /// The host resolves these intents against the original binding, not selection.
    pub fn take_intents(&mut self, actions: &[ContentAction]) -> Vec<RunViewIntent> {
        if self
            .transcript()
            .is_some_and(|content| content.composing_feedback_editor().is_some())
        {
            self.transcript_mut().unwrap().take_feedback_intents(&[]);
            return vec![];
        }
        let mut intents: Vec<_> = self
            .composer_mut()
            .take_intents(actions)
            .into_iter()
            .map(RunViewIntent::Composer)
            .collect();
        if self.composer().is_composing() {
            return intents;
        }
        if let Some(content) = self.transcript_mut() {
            intents.extend(content.take_feedback_intents(actions).into_iter().map(
                |(control, gesture)| RunViewIntent::Feedback {
                    control,
                    gesture: Box::new(gesture),
                },
            ));
        }
        for node in &mut self.body.children {
            if node.id() == self.paging {
                if let Some(scope) = node
                    .component::<TranscriptPaging>()
                    .unwrap()
                    .activation(actions)
                {
                    intents.push(RunViewIntent::LoadEarlier {
                        scope: scope.clone(),
                    });
                }
            } else if node.id() == self.actionable {
                intents.extend(
                    node.component_mut::<ActionableCard>()
                        .unwrap()
                        .take_intents(actions)
                        .into_iter()
                        .map(RunViewIntent::Actionable),
                );
            }
        }
        if let Some(content) = self.transcript_mut() {
            intents.extend(
                content
                    .handle_actions(actions)
                    .into_iter()
                    .map(|intent| RunViewIntent::TurnAction(Box::new(intent))),
            );
            intents.extend(
                content
                    .take_copy_intents(actions)
                    .into_iter()
                    .map(RunViewIntent::CopyTranscript),
            );
        }
        intents
    }
    /// Host-specific capabilities may add controls without forking Run content.
    pub fn append_accessory(&mut self, content: Content) -> Result<(), &'static str> {
        if (self.ids.composer.root.0..=self.ids.composer.preset.0).contains(&content.id().0)
            || self.transcript().is_some_and(|transcript| {
                !transcript.namespaces_disjoint(&[(content.id().0 >> 32) as u32])
            })
            || [self.ids.root, self.ids.indicators, self.ids.viewport].contains(&content.id())
            || self
                .body
                .children
                .iter()
                .any(|node| node.id() == content.id())
        {
            return Err("duplicate RunView accessory");
        }
        self.body.children.push(content);
        Ok(())
    }
    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let id = self.ids.root;
        let mut host = ContentHost::new(Content::new(id, self))?;
        host.set_action_events(true);
        Ok(host)
    }
}
impl Component for RunView {
    fn children(&self) -> &[Content] {
        &self.body.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.body.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        self.body.measure(text, available)
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        self.body.arrange(text, frame, clip);
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.label)
    }
}
