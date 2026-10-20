//! Agent composition only. Presentation owns the Run; TekesUI owns its viewport.
use crate::transcript_content::RunTranscriptLayout;
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
/// The last arranged pane and its real document clip. This is layout evidence,
/// not a second reading state. Publish it only after ContentHost::render succeeds
/// for that frame and is_pane_presented is true. Failed layout may retain changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RunViewGeometry {
    pub publication: Option<u64>,
    pub connection_epoch: Option<u64>,
    pub pane: Rect,
    pub content_column: Rect,
    pub composer: Option<Rect>,
    pub visible_document: Rect,
    pub document_top_clearance: f32,
    pub document_bottom_clearance: f32,
    pub turn_rail: TurnRailGeometry,
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
/// Borrowed, same-UI-turn Agent publication plan. It does not reserve editor
/// state or validate a reading token. Check `controlled_draft()` on the original
/// TekesUI surface before entering a reading transaction; apply rechecks state.
#[derive(Debug)]
pub struct RunViewUpdate<'a> {
    root: ComponentId,
    scope: RunScope,
    base_revision: Option<(u64, u64)>,
    base_editor: tekes_ui_controls::TextEditBaseline,
    base_gate: ComposerControlGate,
    base_generation: u64,
    snapshot: &'a RunSnapshot,
    affordances: ComposerAffordances,
    gate: ComposerControlGate,
    paging: TranscriptPagingFacts,
    actionable_controls: bool,
    transcript: TranscriptContentUpdate<'a>,
    editor: Option<ComposerDraftUpdate>,
}
impl RunViewUpdate<'_> {
    pub fn scope(&self) -> &RunScope {
        &self.scope
    }
    pub fn publication(&self) -> u64 {
        self.snapshot.revision
    }
    pub fn connection_epoch(&self) -> u64 {
        self.snapshot.connection_epoch
    }
    pub fn item_ids(&self) -> Vec<ComponentId> {
        self.transcript.item_ids()
    }
    /// None means Agent policy retains unacknowledged local input/preedit for
    /// an unchanged authority value. Do not replay or synthesize an editor write.
    pub fn controlled_draft(&self) -> Option<&ComposerDraftUpdate> {
        self.editor.as_ref()
    }
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
    column: RunColumnLayout,
    publication_generation: u64,
    pane_presented: bool,
    geometry: Option<RunViewGeometry>,
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
            column: Default::default(),
            publication_generation: 0,
            pane_presented: true,
            geometry: None,
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
    /// Change pane layout without replacing the Transcript, Composer, selection
    /// or operation receipts. Fractions normalize as ClientKit does; invalid
    /// reading maxima leave the previous policy intact.
    pub fn set_column_layout(&mut self, column: RunColumnLayout) -> Result<(), &'static str> {
        self.column = column.normalized()?;
        Ok(())
    }
    /// Last arranged values, not a successful-render receipt. A failed library
    /// render can retain application layout changes; the host must fence native
    /// geometry/input/AX publication until a subsequent render returns Ok.
    pub fn layout_geometry(&self) -> Option<RunViewGeometry> {
        self.geometry
    }
    pub fn is_pane_presented(&self) -> bool {
        self.pane_presented
    }
    pub fn column_layout(&self) -> RunColumnLayout {
        self.column
    }
    /// Host pane minimum; a full host window also includes its own sidebars,
    /// panel widths, gaps and content insets. No window policy lives in the Kit.
    pub fn minimum_pane_width(&self) -> f32 {
        self.column.minimum_pane_width()
    }
    pub fn column_geometry(&self, viewport_width: f32) -> RunColumnGeometry {
        self.column.geometry(viewport_width)
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
    /// The canonical publication actually installed in this retained Run tree.
    /// A newer binding publication may arrive after layout; hosts must not pair
    /// that newer value with this tree's geometry or editor ownership.
    pub fn displayed_snapshot(&self) -> Option<&RunSnapshot> {
        self.last.as_ref()
    }
    pub fn transcript_id(&self) -> Option<ComponentId> {
        self.transcript
    }
    /// No visibility is reported for a collapsed or not-yet-arranged Run pane.
    /// Like layout_geometry, current-frame visibility requires a successful render.
    pub fn turn_targets(&self) -> Result<Vec<TranscriptTurnTarget>, &'static str> {
        let mut targets = self
            .transcript()
            .ok_or("RunView has no Transcript")?
            .turn_targets()?;
        if !self.pane_presented {
            for target in &mut targets {
                target.visible = None;
            }
        }
        Ok(targets)
    }
    pub fn validate_turn_target(&self, target: &TranscriptTurnTarget) -> Result<(), &'static str> {
        if !self.pane_presented || self.composer().is_composing() {
            return Err("RunView navigation is unavailable during collapse or composition");
        }
        self.transcript()
            .ok_or("RunView has no Transcript")?
            .validate_turn_target(target)
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
        let mut scroll = tekes_ui_controls::ScrollArea::default();
        scroll.set_scroll_policy(
            tekes_ui_controls::ScrollPolicy::Disabled,
            tekes_ui_controls::ScrollPolicy::Auto,
        );
        let viewport = ScrollContent::new(scroll, Content::new(content.id(), content));
        // Paint/hit order: document, then chrome/controls. No detached mirror tree.
        self.body
            .children
            .insert(0, Content::new(self.ids.viewport, viewport));
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
        let generation = self
            .publication_generation
            .checked_add(1)
            .ok_or("RunView publication generation exhausted")?;
        self.transcript_mut()
            .ok_or("RunView has no Transcript")?
            .configure_feedback(labels)?;
        self.publication_generation = generation;
        Ok(())
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
        let prompt = self.check_frame(snapshot, &paging)?;
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
    fn check_frame(
        &self,
        snapshot: &RunSnapshot,
        paging: &TranscriptPagingFacts,
    ) -> Result<Option<Arc<ActionablePrompt>>, &'static str> {
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
        Ok(prompt)
    }
    /// Prepare the complete Agent frame and its controlled-value arguments
    /// without editing controls, consuming local changes or allocating rows.
    /// Initial Composer mounting still uses update() before reading attachment.
    pub fn prepare_update<'a>(
        &self,
        snapshot: &'a RunSnapshot,
        affordances: ComposerAffordances,
        gate: ComposerControlGate,
        paging: TranscriptPagingFacts,
        actionable_controls: bool,
    ) -> Result<RunViewUpdate<'a>, &'static str> {
        self.check_frame(snapshot, &paging)?;
        self.check_run_update(snapshot, gate)?;
        let editor = self.composer().prepare_current_update(snapshot)?;
        let transcript = self
            .transcript()
            .ok_or("RunView has no Transcript")?
            .prepare_update(
                &self.scope,
                snapshot.revision,
                snapshot.connection_epoch,
                &snapshot.transcript,
                gate.entries && !self.composer().is_composing(),
            )?;
        Ok(RunViewUpdate {
            root: self.ids.root,
            scope: self.scope.clone(),
            base_revision: self.last.as_ref().map(|s| (s.revision, s.connection_epoch)),
            base_editor: self.composer().editor().input.external_baseline(),
            base_gate: self.gate,
            base_generation: self.publication_generation,
            snapshot,
            affordances,
            gate,
            paging,
            actionable_controls,
            transcript,
            editor,
        })
    }
    /// Recheck a stored plan before entering the library reading transaction.
    /// This catches authority refresh/ABA even if the editor value is unchanged.
    /// It does not replace TekesUI's controlled-value check or original token.
    pub fn validate_prepared_update(
        &self,
        prepared: &RunViewUpdate<'_>,
    ) -> Result<(), &'static str> {
        if self.ids.root != prepared.root
            || self.scope != prepared.scope
            || self.last.as_ref().map(|s| (s.revision, s.connection_epoch))
                != prepared.base_revision
            || self.publication_generation != prepared.base_generation
            || self.gate != prepared.base_gate
            || self.composer().editor().input.external_baseline() != prepared.base_editor
        {
            return Err("stale or foreign RunView update plan");
        }
        let current = self.prepare_update(
            prepared.snapshot,
            prepared.affordances,
            prepared.gate,
            prepared.paging.clone(),
            prepared.actionable_controls,
        )?;
        if current.editor != prepared.editor
            || current.transcript.rows() != prepared.transcript.rows()
        {
            return Err("RunView update plan inputs changed");
        }
        Ok(())
    }
    /// Use immediately after Agent plan validation and library controlled-value
    /// preflight, inside the original reading-token callback. No event/yield or
    /// retarget may intervene; arbitrary application callbacks are not rolled back.
    pub fn apply_prepared_update(
        &mut self,
        prepared: &RunViewUpdate<'_>,
    ) -> Result<(), &'static str> {
        self.validate_prepared_update(prepared)?;
        self.update(
            prepared.snapshot,
            prepared.affordances,
            prepared.gate,
            prepared.paging.clone(),
            prepared.actionable_controls,
        )
    }
    /// One paired canonical publication updates all Run content. This method
    /// neither subscribes, dispatches nor caches a second Run state.
    pub(crate) fn update_run(
        &mut self,
        snapshot: &RunSnapshot,
        affordances: ComposerAffordances,
        gate: ComposerControlGate,
    ) -> Result<(), &'static str> {
        self.check_run_update(snapshot, gate)?;
        let scope = self.scope.clone();
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
        // A same-publication authority refresh (including close→open ABA)
        // retires older plans even when the controlled editor value is unchanged.
        self.publication_generation += 1;
        Ok(())
    }
    fn check_run_update(
        &self,
        snapshot: &RunSnapshot,
        gate: ComposerControlGate,
    ) -> Result<(), &'static str> {
        self.publication_generation
            .checked_add(3)
            .ok_or("RunView publication generation exhausted")?;
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
        if let Some(content) = self.transcript() {
            content.validate_update(
                &scope,
                snapshot.revision,
                snapshot.connection_epoch,
                &snapshot.transcript,
                available,
            )?;
        }
        self.body
            .children
            .iter()
            .find_map(|node| node.component::<Indicators>())
            .unwrap()
            .check_update(snapshot)?;
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
        let generation = self
            .publication_generation
            .checked_add(1)
            .ok_or("RunView publication generation exhausted")?;
        self.body
            .children
            .iter_mut()
            .find_map(|node| node.component_mut::<TranscriptPaging>())
            .unwrap()
            .update(facts)?;
        self.publication_generation = generation;
        Ok(())
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
        let generation = self
            .publication_generation
            .checked_add(1)
            .ok_or("RunView publication generation exhausted")?;
        let changed = self
            .body
            .children
            .iter_mut()
            .find_map(|node| node.component_mut::<ActionableCard>())
            .unwrap()
            .update(publication, epoch, prompt, controls)?;
        self.publication_generation = generation;
        Ok(changed)
    }
    /// Drain once immediately after ContentHost event/AX delivery, before update.
    /// The host resolves these intents against the original binding, not selection.
    pub fn take_intents(&mut self, actions: &[ContentAction]) -> Vec<RunViewIntent> {
        if !self.pane_presented {
            return Vec::new();
        }
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
    // Keep resident IDs/editors in the same mounted tree. The header is the
    // existing status/page/actionable/accessory composition; its final rail and
    // selection counterparts are separate outstanding ClientKit View work.
    fn with_header<T>(&mut self, layout: impl FnOnce(&mut StackContent) -> T) -> T {
        let mut held = Vec::new();
        let mut index = 0;
        while index < self.body.children.len() {
            let id = self.body.children[index].id();
            let empty_actionable = self.body.children[index]
                .component::<ActionableCard>()
                .is_some_and(|card| !card.active());
            if id == self.ids.composer.root || id == self.ids.viewport || empty_actionable {
                held.push((index + held.len(), self.body.children.remove(index)));
            } else {
                index += 1;
            }
        }
        let result = layout(&mut self.body);
        for (index, content) in held {
            self.body.children.insert(index, content);
        }
        result
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
    fn active(&self) -> bool {
        self.pane_presented
    }
    fn measure(&mut self, _text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        // Run is a bounded viewport, not the sum of docked sibling heights.
        MeasuredChild::new(available)
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        self.pane_presented = frame.size.width > 0. && frame.size.height > 0.;
        if !self.pane_presented {
            return;
        } // Preserve prior child geometry, as RunOverlay does.
        let pane_clip = frame.intersection(&clip).unwrap_or_default();
        let column = self.column.frame(frame);
        let composer = if self.composer().is_presented() {
            let bottom = Spacing::Regular.value().min(frame.size.height);
            let height = self
                .composer_mut()
                .measure(text, column.size)
                .size
                .height
                .min((frame.size.height - bottom).max(0.));
            Some(Rect::new(
                column.min_x(),
                frame.max_y() - bottom - height,
                column.size.width,
                height,
            ))
        } else {
            None
        };
        let bottom_clearance = composer.map_or(0., |composer| {
            frame.max_y() - composer.min_y() + Spacing::Loose.value()
        });
        let document_bottom = (frame.max_y() - bottom_clearance).max(frame.min_y());
        let header_height = self
            .with_header(|header| header.measure(text, column.size))
            .size
            .height
            .min((document_bottom - frame.min_y()).max(0.));
        let header = Rect::new(
            column.min_x(),
            frame.min_y(),
            column.size.width,
            header_height,
        );
        let top_clearance = (header_height
            + if header_height > 0. {
                Spacing::Regular.value()
            } else {
                0.
            })
        .min((document_bottom - frame.min_y()).max(0.));
        let document_top = frame.min_y() + top_clearance;
        let visible_document = Rect::new(
            frame.min_x(),
            document_top,
            frame.size.width,
            (document_bottom - document_top).max(0.),
        );
        let document_clip = visible_document
            .intersection(&pane_clip)
            .unwrap_or_default();
        if let Some(content) = self.transcript_mut() {
            content.set_run_layout(RunTranscriptLayout {
                column_x: column.min_x(),
                column_width: column.size.width,
                top: top_clearance,
                bottom: bottom_clearance,
            });
        }
        if let Some(viewport) = self
            .body
            .children
            .iter_mut()
            .find(|node| node.id() == self.ids.viewport)
        {
            // Full pane remains the scrolling coordinate system. The library
            // receives the actual unobstructed clip, and measures the document's
            // own clearance. No offset compensation or reading state is copied.
            viewport.arrange(text, frame, document_clip);
        }
        self.with_header(|body| {
            body.arrange(
                text,
                header,
                header.intersection(&pane_clip).unwrap_or_default(),
            )
        });
        if let Some(composer) = composer {
            self.body
                .children
                .iter_mut()
                .find(|node| node.id() == self.ids.composer.root)
                .unwrap()
                .arrange(text, composer, pane_clip);
        }
        self.geometry = Some(RunViewGeometry {
            publication: self.last.as_ref().map(|snapshot| snapshot.revision),
            connection_epoch: self.last.as_ref().map(|snapshot| snapshot.connection_epoch),
            pane: frame,
            content_column: column,
            composer,
            visible_document: document_clip,
            document_top_clearance: top_clearance,
            document_bottom_clearance: bottom_clearance,
            turn_rail: TurnRailGeometry::for_run(
                frame,
                self.column,
                self.last.as_ref().map_or(0, |snapshot| {
                    snapshot
                        .transcript
                        .turns
                        .iter()
                        .filter(|turn| !turn.input.trim().is_empty())
                        .count()
                }),
            ),
        });
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.label)
    }
}
