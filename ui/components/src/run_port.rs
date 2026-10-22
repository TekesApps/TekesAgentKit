//! Agent composition only. Presentation owns the Run; TekesUI owns its viewport.
use crate::transcript_content::RunTranscriptLayout;
use crate::*;
#[path = "run_jump.rs"]
mod jump;
pub use jump::*;
use std::ops::Range;
use std::sync::Arc;
use tekes_agent_presentation::ActionablePrompt;
use tekes_ui_controls::layout::MeasuredChild;
use tekes_ui_scene::Size;

/// Reserve the whole Composer namespace plus the three extra IDs, and distinct
/// namespaces for paging/actionables/context/transcript. Hosts allocate these IDs.
#[derive(Clone, Copy, Debug)]
pub struct RunPortIds {
    pub root: ComponentId,
    pub indicators: ComponentId,
    pub viewport: ComponentId,
    pub composer: ComposerIds,
}
/// The last arranged pane and its real document clip. This is layout evidence,
/// not a second reading state. Publish it only after ContentHost::render succeeds
/// for that frame and is_pane_presented is true. Failed layout may retain changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RunPortGeometry {
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
pub enum RunPortIntent {
    /// Local layout changed; republish the captured Run through its original reading transaction.
    TranscriptLayoutChanged {
        scope: RunScope,
    },
    Composer(ComposerIntent),
    LoadEarlier {
        scope: RunScope,
    },
    Actionable(ActionableResponseIntent),
    TurnAction(Box<TurnFootnoteIntent>),
    CopyTranscript(TranscriptCopyIntent),
    TurnNavigation(Box<RunTurnNavigation>),
    JumpLatest(Box<RunJumpLatest>),
    Feedback {
        control: ComponentId,
        gesture: Box<FeedbackGesture>,
    },
}
/// Borrowed, same-UI-turn Agent publication plan. It does not reserve editor
/// state or validate a reading token. Check `controlled_draft()` on the original
/// TekesUI surface before entering a reading transaction; apply rechecks state.
#[derive(Debug)]
pub struct RunPortUpdate<'a> {
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
impl RunPortUpdate<'_> {
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
pub struct RunPort {
    ids: RunPortIds,
    scope: RunScope,
    paging: ComponentId,
    actionable: ComponentId,
    transcript: Option<ComponentId>,
    rail: Option<ComponentId>,
    jump: Option<ComponentId>,
    jump_visibility: RunJumpVisibility,
    body: StackContent,
    last: Option<RunSnapshot>,
    gate: ComposerControlGate,
    label: String,
    column: RunColumnLayout,
    publication_generation: u64,
    pane_presented: bool,
    navigation_lifetime: Arc<()>,
    navigation_mounted: bool,
    navigation_origin: Option<(u64, Option<String>)>,
    geometry: Option<RunPortGeometry>,
    footnote_clock: Option<f64>,
    footnote_clock_revision: u64,
}
impl RunPort {
    /// Four consecutive namespaces for controls/paging/actionables/transcript;
    /// the host supplies a separate exclusive row pool. Default style is TekesUI.
    pub fn standard(
        namespace: u32,
        rows: Range<u32>,
        scope: RunScope,
    ) -> Result<Self, &'static str> {
        let transcript = namespace
            .checked_add(3)
            .ok_or("RunPort namespace overflow")?;
        let base = (namespace as u64) << 32;
        let composer = ComposerIds::new(base + 16);
        Self::new(
            RunPortIds {
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
        ids: RunPortIds,
        scope: RunScope,
        composer: Composer,
        paging_namespace: u32,
        actionable_namespace: u32,
    ) -> Result<Self, &'static str> {
        scope.validate().map_err(|_| "invalid RunPort scope")?;
        if composer
            .destination()
            .is_some_and(|destination| destination != &ComposerDestination::run(scope.clone()))
        {
            return Err("RunPort composer source mismatch");
        }
        if ids.composer != composer.ids() {
            return Err("RunPort composer IDs mismatch");
        }
        let composer_namespace = ids.composer.root.0 >> 32;
        if ids.composer.input_band.0 >> 32 != composer_namespace
            || paging_namespace == actionable_namespace
            || [paging_namespace as u64, actionable_namespace as u64].contains(&composer_namespace)
        {
            return Err("RunPort namespace overlap");
        }
        let reserved = [ids.root, ids.indicators, ids.viewport];
        if reserved.iter().enumerate().any(|(i, id)| {
            reserved[..i].contains(id)
                || (ids.composer.root.0..=ids.composer.input_band.0).contains(&id.0)
                || [paging_namespace as u64, actionable_namespace as u64].contains(&(id.0 >> 32))
        }) {
            return Err("RunPort control IDs overlap");
        }
        // Include a configured context's hidden IDs, even while its cards are empty.
        if composer.context().is_some_and(|context| {
            let ns = context.id().0 >> 32;
            [paging_namespace as u64, actionable_namespace as u64].contains(&ns)
                || reserved.iter().any(|id| id.0 >> 32 == ns)
        }) {
            return Err("RunPort context namespace overlap");
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
            rail: None,
            jump: None,
            jump_visibility: Default::default(),
            last: None,
            gate: ComposerControlGate::CLOSED,
            label: "Agent Run".into(),
            column: Default::default(),
            publication_generation: 0,
            pane_presented: true,
            navigation_lifetime: Arc::new(()),
            navigation_mounted: false,
            navigation_origin: None,
            geometry: None,
            footnote_clock: None,
            footnote_clock_revision: 0,
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
    pub fn layout_geometry(&self) -> Option<RunPortGeometry> {
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
    pub fn ids(&self) -> RunPortIds {
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
            .ok_or("RunPort has no Transcript")?
            .turn_targets()?;
        if !self.pane_presented {
            for target in &mut targets {
                target.visible = None;
            }
        }
        Ok(targets)
    }
    /// Read-only preview of a captured original turn after a successful current
    /// render. It may preserve an active Composer preedit; it does not activate
    /// navigation, claim a gesture, invoke host history or display an overlay.
    pub fn turn_preview<'a>(
        &self,
        target: &'a TranscriptTurnTarget,
    ) -> Result<&'a TranscriptTurnPreview, &'static str> {
        if !self.navigation_mounted || !self.pane_presented || target.is_visible().is_none() {
            return Err("RunPort preview requires a presented arranged pane");
        }
        self.transcript()
            .ok_or("RunPort has no Transcript")?
            .validate_turn_target(target)?;
        let geometry = self.geometry.ok_or("RunPort is not arranged")?;
        if geometry.publication != Some(target.publication())
            || geometry.connection_epoch != Some(target.connection_epoch())
        {
            return Err("RunPort preview geometry belongs to another publication");
        }
        Ok(target.preview())
    }
    pub fn validate_turn_target(&self, target: &TranscriptTurnTarget) -> Result<(), &'static str> {
        if !self.pane_presented || self.composer().is_composing() {
            return Err("RunPort navigation is unavailable during collapse or composition");
        }
        self.transcript()
            .ok_or("RunPort has no Transcript")?
            .validate_turn_target(target)
    }
    /// Capture a non-focusing local navigation gesture from the installed,
    /// arranged content. Hosts must call this after a successful current render;
    /// layout alone is not proof that native geometry is publishable. Unlike a
    /// focus-changing control, this path may preserve an active Composer preedit.
    pub fn prepare_turn_navigation(
        &self,
        target: &TranscriptTurnTarget,
    ) -> Result<RunTurnNavigation, &'static str> {
        if !self.navigation_mounted || !self.pane_presented || target.is_visible().is_none() {
            return Err("RunPort navigation requires a presented arranged pane");
        }
        self.transcript()
            .ok_or("RunPort has no Transcript")?
            .validate_turn_target(target)?;
        let geometry = self.geometry.ok_or("RunPort is not arranged")?;
        if geometry.publication != Some(target.publication())
            || geometry.connection_epoch != Some(target.connection_epoch())
        {
            return Err("RunPort navigation geometry belongs to another publication");
        }
        let latest = self
            .turn_targets()?
            .last()
            .is_some_and(|last| last.turn_id() == target.turn_id());
        Ok(RunTurnNavigation {
            jump: self.navigation_jump(target),
            target: target.clone(),
            kind: if latest {
                RunTurnNavigationKind::Latest
            } else {
                RunTurnNavigationKind::TurnStart(
                    *target.row_ids().first().ok_or("turn has no mounted row")?,
                )
            },
            viewport: self.ids.viewport,
            generation: self.publication_generation,
            lifetime: self.navigation_lifetime.clone(),
            claimed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }
    /// Revalidate immediately before a public TekesUI navigation call. No
    /// scroll offset, focus change, editor write or network request occurs here.
    pub fn claim_turn_navigation(
        &self,
        navigation: &RunTurnNavigation,
    ) -> Result<RunTurnNavigationKind, &'static str> {
        if navigation.viewport != self.ids.viewport
            || navigation.generation != self.publication_generation
            || !Arc::ptr_eq(&navigation.lifetime, &self.navigation_lifetime)
        {
            return Err("stale or foreign RunPort navigation gesture");
        }
        let current = self.prepare_turn_navigation(&navigation.target)?;
        if current.kind != navigation.kind {
            return Err("RunPort navigation target changed");
        }
        navigation
            .claimed
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .map_err(|_| "RunPort navigation gesture already dispatched")?;
        Ok(navigation.kind)
    }
    /// Main hosts opt in with an actual bottom-visibility fact after render.
    /// Passing None retires host-history gestures (e.g. when changing pane role).
    /// Task/subagent/SideChat hosts leave this disabled and use ordinary claim.
    pub fn set_turn_navigation_context(
        &mut self,
        context: Option<RunTurnNavigationContext>,
    ) -> Result<(), &'static str> {
        let origin = if let Some(context) = context {
            if !self.navigation_mounted || !self.pane_presented || self.geometry.is_none() {
                return Err("navigation context needs a presented rendered Run");
            }
            let targets = self.turn_targets()?;
            if targets.iter().any(|t| t.is_visible().is_none()) {
                return Err("navigation context needs current arranged turns");
            }
            let from = if context.bottom_is_visible {
                targets.last()
            } else {
                targets.iter().find(|t| t.is_visible() == Some(true))
            };
            Some((
                self.publication_generation,
                from.map(|t| t.turn_id().to_owned()),
            ))
        } else {
            None
        };
        if self.navigation_origin.is_some() != origin.is_some() {
            self.navigation_lifetime = Arc::new(());
        }
        self.navigation_origin = origin;
        self.refresh_rail_contents(true)
    }
    fn navigation_jump(&self, target: &TranscriptTurnTarget) -> Option<RunTurnJump> {
        let (generation, from) = self.navigation_origin.as_ref()?;
        (*generation == self.publication_generation).then(|| RunTurnJump {
            scope: self.scope.clone(),
            from: from.clone(),
            to: target.turn_id().to_owned(),
        })
    }
    /// Check SDK attachment first, then claim and call these synchronous host
    /// hooks immediately before SDK navigation. No callback runs for a rejected
    /// capture. A callback panic or later SDK failure is not a rollback transaction.
    pub fn claim_turn_navigation_with_host(
        &self,
        navigation: &RunTurnNavigation,
        host: &mut (impl RunTurnNavigationHost + ?Sized),
    ) -> Result<RunTurnNavigationKind, &'static str> {
        let jump = navigation
            .jump
            .as_ref()
            .ok_or("main navigation context was not captured")?;
        let kind = self.claim_turn_navigation(navigation)?;
        host.record_turn_jump(jump);
        let _ = host.take_pending_turn_reveal(&self.scope);
        Ok(kind)
    }
    /// Mount shared compact actions using the exact published SDK controls.
    pub fn install_turn_rail(&mut self, namespace: u32) -> Result<(), &'static str> {
        if !turn_rail_supported() {
            return Err("TekesUI exact compact actions unavailable");
        }
        if self.rail.is_some() {
            return Err("RunPort rail already installed");
        }
        let transcript = self.transcript().ok_or("RunPort rail needs a Transcript")?;
        if !transcript.namespaces_disjoint(&[namespace])
            || self
                .body
                .children
                .iter()
                .any(|node| node.id().0 >> 32 == namespace as u64)
            || self.ids.root.0 >> 32 == namespace as u64
            || self
                .composer()
                .context()
                .is_some_and(|c| c.id().0 >> 32 == namespace as u64)
        {
            return Err("RunPort rail namespace overlap");
        }
        let generation = self
            .publication_generation
            .checked_add(1)
            .ok_or("RunPort publication generation exhausted")?;
        let rail = TurnRail::new(namespace, self.scope.clone());
        self.rail = Some(rail.id());
        self.publication_generation = generation;
        self.body.children.push(Content::new(rail.id(), rail));
        self.geometry = None;
        Ok(())
    }
    pub fn with_turn_rail(mut self, namespace: u32) -> Result<Self, &'static str> {
        self.install_turn_rail(namespace)?;
        Ok(self)
    }
    pub fn turn_rail(&self) -> Option<&TurnRail> {
        self.body
            .children
            .iter()
            .find_map(|n| n.component::<TurnRail>())
    }
    pub fn turn_rail_mut(&mut self) -> Option<&mut TurnRail> {
        self.body
            .children
            .iter_mut()
            .find_map(|n| n.component_mut::<TurnRail>())
    }
    pub fn set_rail_navigation_gate(
        &mut self,
        gate: TurnRailNavigationGate,
    ) -> Result<(), &'static str> {
        if self
            .turn_rail()
            .ok_or("RunPort has no rail")?
            .navigation_gate()
            != gate
        {
            self.navigation_lifetime = Arc::new(());
            self.turn_rail_mut().unwrap().set_gate(gate);
        }
        Ok(())
    }
    /// Drain immutable event-time captures before changing the source/frame.
    /// Validation at dispatch still rejects a publication/authority/pane ABA.
    pub fn take_turn_navigation(&mut self) -> Vec<RunTurnNavigation> {
        self.turn_rail_mut()
            .map_or_else(Vec::new, TurnRail::take_navigation)
    }
    /// Same-publication original clipboard controls; no endpoint capability.
    /// A hidden pane cannot admit these controls through a host IME barrier.
    pub fn copy_action_ids(&self) -> Vec<ComponentId> {
        if self.pane_presented {
            self.transcript()
                .map_or_else(Vec::new, TranscriptContent::copy_action_ids)
        } else {
            vec![]
        }
    }
    pub fn activity_action_ids(&self) -> Vec<ComponentId> {
        if self.pane_presented {
            self.transcript()
                .map_or_else(Vec::new, TranscriptContent::activity_action_ids)
        } else {
            vec![]
        }
    }
    pub fn is_nonfocusing_navigation_target(&self, id: ComponentId) -> bool {
        self.jump_permits(id)
            || (self.pane_presented
                && self.turn_rail().is_some_and(|rail| {
                    rail.permits_id(id)
                        && rail.prototype(id).is_some_and(|p| {
                            p.generation == self.publication_generation
                                && Arc::ptr_eq(&p.lifetime, &self.navigation_lifetime)
                                && p.target().is_visible().is_some()
                        })
                }))
    }
    pub fn is_nonfocusing_navigation_at(&self, point: tekes_ui_scene::Point) -> bool {
        self.jump_at(point)
            || self.turn_rail().is_some_and(|rail| {
                rail.action_at(point)
                    .is_some_and(|id| self.is_nonfocusing_navigation_target(id))
            })
    }
    /// Library reading state supplies following_latest; AgentKit chooses the
    /// semantic last or first actually visible turn, never scroll offsets.
    pub fn update_rail_follow_target(
        &mut self,
        following_latest: bool,
    ) -> Result<(), &'static str> {
        let targets = self.turn_targets()?;
        let target = if following_latest {
            targets.last()
        } else {
            targets.iter().find(|t| t.is_visible() == Some(true))
        }
        .map(|t| t.turn_id().to_owned());
        self.turn_rail_mut()
            .ok_or("RunPort has no rail")?
            .set_follow_target(target);
        Ok(())
    }
    fn refresh_rail_contents(&mut self, arranged: bool) -> Result<(), &'static str> {
        if self.rail.is_none() {
            return Ok(());
        }
        let targets = self.turn_targets()?;
        let count = targets.len();
        let prototypes = targets
            .into_iter()
            .enumerate()
            .map(|(index, target)| RunTurnNavigation {
                jump: self.navigation_jump(&target),
                kind: if index + 1 == count {
                    RunTurnNavigationKind::Latest
                } else {
                    RunTurnNavigationKind::TurnStart(target.row_ids()[0])
                },
                target,
                viewport: self.ids.viewport,
                generation: self.publication_generation,
                lifetime: self.navigation_lifetime.clone(),
                claimed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            })
            .collect();
        let pane = self.geometry.map_or_else(Rect::default, |g| g.pane);
        let geometry = TurnRailGeometry::for_run(pane, self.column, count);
        let rail = self.turn_rail_mut().unwrap();
        rail.refresh(prototypes, geometry)?;
        if !arranged {
            rail.clear_geometry();
        }
        Ok(())
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
    /// Configure shared reasoning display before preparing/publishing this Run.
    /// The existing Transcript plan fences preference/tick changes; no Run
    /// revision, editor write, or new source read is synthesized here.
    pub fn set_thinking_display(
        &mut self,
        input: TranscriptThinkingDisplayInput,
    ) -> Result<(), &'static str> {
        self.transcript_mut()
            .ok_or("RunPort has no Transcript")?
            .set_thinking_display(input)
    }
    /// Controlled local disclosure; the next original reading publication installs it.
    pub fn set_activity_expanded(
        &mut self,
        turn: &str,
        expanded: bool,
    ) -> Result<(), &'static str> {
        self.transcript_mut()
            .ok_or("transcript is not mounted")?
            .set_activity_expanded(turn, expanded)
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
            return Err("RunPort transcript already mounted");
        }
        if content.scope() != &self.scope {
            return Err("RunPort transcript scope mismatch");
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
            return Err("RunPort transcript namespace overlap");
        }
        if let Some(snapshot) = &self.last {
            // Late installation must seed the same display inputs as a normal
            // Run publication, including goal facts and the explicit host clock.
            content.update_with_timing(
                &self.scope,
                snapshot.revision,
                snapshot.connection_epoch,
                &snapshot.transcript,
                self.gate.entries && !self.composer().is_composing(),
                &self.footnote_timing(snapshot),
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
            .ok_or("RunPort publication generation exhausted")?;
        self.transcript_mut()
            .ok_or("RunPort has no Transcript")?
            .configure_feedback(labels)?;
        self.publication_generation = generation;
        Ok(())
    }
    /// Explicit host callback registration plus endpoint capability. The host
    /// supplies None when either is absent. Update/render before using new IDs.
    pub fn configure_historical_input(
        &mut self,
        control: Option<HistoricalInputControl>,
    ) -> Result<(), &'static str> {
        if self
            .transcript()
            .ok_or("RunPort has no Transcript")?
            .historical_input_control()
            == control.as_ref()
        {
            return Ok(());
        }
        if self.composer().is_composing() {
            return Err("finish Composer composition before changing controls");
        }
        let generation = self
            .publication_generation
            .checked_add(1)
            .ok_or("RunPort publication generation exhausted")?;
        self.transcript_mut()
            .ok_or("RunPort has no Transcript")?
            .configure_historical_input(control)?;
        self.publication_generation = generation;
        Ok(())
    }
    /// Separate opt-in host callback channel. Historical editing is excluded
    /// from copy/navigation exceptions during preedit and from hidden panes.
    /// Resolve with the original RunBinding::dispatch_historical_input.
    pub fn take_historical_input_intents(
        &self,
        actions: &[ContentAction],
    ) -> Vec<HistoricalInputIntent> {
        if !self.pane_presented
            || !self.gate.entries
            || self.composer().is_composing()
            || self
                .transcript()
                .is_some_and(|content| content.composing_feedback_editor().is_some())
        {
            return vec![];
        }
        self.transcript().map_or_else(Vec::new, |content| {
            content.take_historical_input_intents(actions)
        })
    }
    /// Apply one frame, including per-binding read/response authority. The
    /// controls flag only restricts canonical actions; dispatch still revalidates.
    /// Supply a quantized host Unix-ms clock before preparing a reading frame.
    /// This only stages display input; original controls change in update/apply.
    /// Host clock scheduling follows the accepted, presented footnote input.
    pub fn has_live_footnote(&self) -> bool {
        self.pane_presented
            && self.footnote_clock.is_some()
            && self
                .transcript()
                .is_some_and(|content| content.footnote_timing().live_turn_id.is_some())
    }
    pub fn set_footnote_clock(&mut self, now_ms: Option<f64>) -> Result<(), &'static str> {
        if now_ms.is_some_and(|n| !n.is_finite() || n <= 0.) {
            return Err("invalid footnote display clock");
        }
        if self.footnote_clock != now_ms {
            let revision = self
                .footnote_clock_revision
                .checked_add(1)
                .ok_or("footnote display revision exhausted")?;
            let generation = self
                .publication_generation
                .checked_add(1)
                .ok_or("RunPort publication generation exhausted")?;
            self.footnote_clock = now_ms;
            self.footnote_clock_revision = revision;
            self.publication_generation = generation;
        }
        Ok(())
    }
    fn footnote_timing(&self, snapshot: &RunSnapshot) -> crate::TranscriptFootnoteTiming {
        crate::TranscriptFootnoteTiming::for_run(
            snapshot,
            self.footnote_clock_revision,
            self.footnote_clock,
        )
    }
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
            return Err("RunPort paging input mismatch");
        }
        let prompt = snapshot
            .run
            .as_ref()
            .and_then(|run| run.actionables.first())
            .map(|value| ActionablePrompt::read(self.scope.clone(), value.clone()).map(Arc::new))
            .transpose()
            .map_err(|_| "invalid RunPort actionable")?;
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
    ) -> Result<RunPortUpdate<'a>, &'static str> {
        self.check_frame(snapshot, &paging)?;
        self.check_run_update(snapshot, gate)?;
        let editor = self.composer().prepare_current_update(snapshot)?;
        let transcript = self
            .transcript()
            .ok_or("RunPort has no Transcript")?
            .prepare_update_with_timing(
                &self.scope,
                snapshot.revision,
                snapshot.connection_epoch,
                &snapshot.transcript,
                gate.entries && !self.composer().is_composing(),
                &self.footnote_timing(snapshot),
            )?;
        Ok(RunPortUpdate {
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
        prepared: &RunPortUpdate<'_>,
    ) -> Result<(), &'static str> {
        if self.ids.root != prepared.root
            || self.scope != prepared.scope
            || self.last.as_ref().map(|s| (s.revision, s.connection_epoch))
                != prepared.base_revision
            || self.publication_generation != prepared.base_generation
            || self.gate != prepared.base_gate
            || self.composer().editor().input.external_baseline() != prepared.base_editor
        {
            return Err("stale or foreign RunPort update plan");
        }
        self.transcript()
            .ok_or("RunPort has no Transcript")?
            .validate_prepared_update(&prepared.transcript)?;
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
            return Err("RunPort update plan inputs changed");
        }
        Ok(())
    }
    /// Use immediately after Agent plan validation and library controlled-value
    /// preflight, inside the original reading-token callback. No event/yield or
    /// retarget may intervene; arbitrary application callbacks are not rolled back.
    pub fn apply_prepared_update(
        &mut self,
        prepared: &RunPortUpdate<'_>,
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
        let footnote_timing = self.footnote_timing(snapshot);
        if let Some(content) = self.transcript_mut() {
            content.update_with_timing(
                &scope,
                snapshot.revision,
                snapshot.connection_epoch,
                &snapshot.transcript,
                available,
                &footnote_timing,
            )?;
        }
        self.last = Some(snapshot.clone());
        self.gate = gate;
        // A same-publication authority refresh (including close→open ABA)
        // retires older plans even when the controlled editor value is unchanged.
        self.publication_generation += 1;
        self.refresh_rail_contents(false)?;
        Ok(())
    }
    fn check_run_update(
        &self,
        snapshot: &RunSnapshot,
        gate: ComposerControlGate,
    ) -> Result<(), &'static str> {
        self.publication_generation
            .checked_add(3)
            .ok_or("RunPort publication generation exhausted")?;
        if snapshot.revision == 0
            || self.last.as_ref().is_some_and(|old| {
                snapshot.revision < old.revision
                    || snapshot.connection_epoch < old.connection_epoch
                    || (snapshot.revision == old.revision && snapshot != old)
            })
        {
            return Err("stale or conflicting RunPort input");
        }
        if let Some(goal) = &snapshot.goal {
            goal.validate().map_err(|_| "invalid RunPort goal facts")?;
            if goal.scope != self.scope {
                return Err("RunPort goal source mismatch");
            }
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
            return Err("RunPort source mismatch");
        }
        if let Some(rail) = self.turn_rail() {
            rail.check_membership(
                snapshot
                    .transcript
                    .turns
                    .iter()
                    .filter(|t| !t.input.trim().is_empty())
                    .map(|t| t.id.as_str()),
            )?;
        }
        let scope = self.scope.clone();
        let available = gate.entries && !self.composer().is_composing();
        if let Some(content) = self.transcript() {
            content.validate_update_with_timing(
                &scope,
                snapshot.revision,
                snapshot.connection_epoch,
                &snapshot.transcript,
                available,
                &self.footnote_timing(snapshot),
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
            return Err("RunPort paging input mismatch");
        }
        facts.source_ready &= self.gate.entries;
        let generation = self
            .publication_generation
            .checked_add(1)
            .ok_or("RunPort publication generation exhausted")?;
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
            return Err("RunPort actionable source mismatch");
        }
        if self.last.as_ref().is_none_or(|snapshot| {
            snapshot.revision != publication || snapshot.connection_epoch != epoch
        }) {
            return Err("RunPort actionable publication mismatch");
        }
        let canonical = self
            .last
            .as_ref()
            .and_then(|snapshot| snapshot.run.as_ref())
            .and_then(|run| run.actionables.first());
        if prompt.as_ref().map(|prompt| prompt.source()) != canonical {
            return Err("RunPort actionable facts mismatch");
        }
        let controls = controls && self.gate.entries && !self.composer().is_composing();
        let generation = self
            .publication_generation
            .checked_add(1)
            .ok_or("RunPort publication generation exhausted")?;
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
    pub fn take_intents(&mut self, actions: &[ContentAction]) -> Vec<RunPortIntent> {
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
            .map(RunPortIntent::Composer)
            .collect();
        intents.extend(
            self.take_turn_navigation()
                .into_iter()
                .map(|n| RunPortIntent::TurnNavigation(Box::new(n))),
        );
        intents.extend(
            self.take_jump()
                .into_iter()
                .map(|intent| RunPortIntent::JumpLatest(Box::new(intent))),
        );
        // Original local clipboard controls preserve the current editor and do
        // not dispatch an endpoint mutation. Admit them before the send/action
        // preedit barrier; feedback/turn/page actions remain below it.
        if let Some(content) = self.transcript() {
            intents.extend(
                content
                    .take_copy_intents(actions)
                    .into_iter()
                    .map(RunPortIntent::CopyTranscript),
            );
        }
        if let Some(content) = self.transcript_mut()
            && content.handle_activity_actions(actions)
        {
            intents.push(RunPortIntent::TranscriptLayoutChanged {
                scope: content.scope().clone(),
            });
        }
        if self.composer().is_composing() {
            return intents;
        }
        if let Some(content) = self.transcript_mut() {
            intents.extend(content.take_feedback_intents(actions).into_iter().map(
                |(control, gesture)| RunPortIntent::Feedback {
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
                    intents.push(RunPortIntent::LoadEarlier {
                        scope: scope.clone(),
                    });
                }
            } else if node.id() == self.actionable {
                intents.extend(
                    node.component_mut::<ActionableCard>()
                        .unwrap()
                        .take_intents(actions)
                        .into_iter()
                        .map(RunPortIntent::Actionable),
                );
            }
        }
        if let Some(content) = self.transcript_mut() {
            intents.extend(
                content
                    .handle_actions(actions)
                    .into_iter()
                    .map(|intent| RunPortIntent::TurnAction(Box::new(intent))),
            );
        }
        intents
    }
    /// Host-specific capabilities may add controls without forking Run content.
    pub fn append_accessory(&mut self, content: Content) -> Result<(), &'static str> {
        if (self.ids.composer.root.0..=self.ids.composer.input_band.0).contains(&content.id().0)
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
            return Err("duplicate RunPort accessory");
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
            if id == self.ids.composer.root
                || id == self.ids.viewport
                || Some(id) == self.rail
                || Some(id) == self.jump
                || empty_actionable
            {
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
impl Component for RunPort {
    fn lifecycle(&mut self, event: Lifecycle) {
        let mounted = event == Lifecycle::Mounted;
        if self.navigation_mounted != mounted {
            self.navigation_lifetime = Arc::new(());
        }
        self.navigation_mounted = mounted;
        if !mounted {
            self.jump_visibility.incoming_document();
            self.navigation_origin = None;
        }
    }
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
        let presented = frame.size.width > 0. && frame.size.height > 0.;
        if presented != self.pane_presented {
            // Retire local gestures across collapse/reopen, including same-
            // publication ABA. Reading attachment/editor state remains upstream.
            self.navigation_lifetime = Arc::new(());
        }
        self.pane_presented = presented;
        if !self.pane_presented {
            self.jump_visibility.incoming_document();
            self.navigation_origin = None;
            if let Some(rail) = self.turn_rail_mut() {
                rail.invalidate_layout();
            }
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
        self.geometry = Some(RunPortGeometry {
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
        if self.rail.is_some() {
            self.refresh_rail_contents(true)
                .expect("validated rail membership");
            let geometry = self.geometry.unwrap().turn_rail;
            let rail_id = self.rail.unwrap();
            self.body
                .children
                .iter_mut()
                .find(|n| n.id() == rail_id)
                .unwrap()
                .arrange(text, geometry.rail, pane_clip);
        }
        self.arrange_jump(text, frame, pane_clip, composer);
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.label)
    }
}
