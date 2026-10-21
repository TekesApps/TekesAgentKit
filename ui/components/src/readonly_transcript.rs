//! Read-only child content without a fabricated RunController or Composer.
use crate::*;
use tekes_agent_presentation::ConnectionPresentation;
use tekes_ui_controls::layout::MeasuredChild;
use tekes_ui_scene::Size;

#[derive(Clone, Copy, Debug)]
pub struct ReadOnlyTranscriptIds {
    pub root: ComponentId,
    pub status: ComponentId,
    pub refresh: ComponentId,
    pub viewport: ComponentId,
}
#[derive(Clone, Debug)]
pub struct ReadOnlyTranscriptLabels {
    pub group: String,
    pub refresh: String,
    pub stale: String,
    pub loading: String,
    pub information: fn(&ConnectionPresentation, usize, bool) -> String,
    pub content: TranscriptContentLabels,
    pub paging: TranscriptPagingLabels,
}
impl Default for ReadOnlyTranscriptLabels {
    fn default() -> Self {
        Self {
            group: "有界子历史（只读）".into(),
            refresh: "刷新子历史".into(),
            stale: "历史待同步".into(),
            loading: "正在读取子历史".into(),
            information: |connection, count, current| {
                let mut text = format!("有界子历史 · 只读 · {count} 项");
                if current {
                    text.push_str(&format!(" · {}", connection_status_text(&connection.phase)));
                }
                if let Some(error) = &connection.message {
                    text.push_str(&format!("\n{error}"));
                }
                text
            },
            content: Default::default(),
            paging: Default::default(),
        }
    }
}
/// Canonical frame metadata is separate from transient read/availability facts.
/// publication is original binding read order, never an endpoint journal cursor.
#[derive(Clone, Copy, Debug)]
pub struct ReadOnlyTranscriptInput<'a> {
    pub scope: &'a RunScope,
    pub publication: u64,
    pub connection_epoch: u64,
    pub connection: &'a ConnectionPresentation,
    pub transcript: &'a TranscriptSnapshot,
    pub frame_current: bool,
    pub source_ready: bool,
    pub supports_pagination: bool,
    pub reading: bool,
    pub read_error: Option<&'a str>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadOnlyTranscriptRead {
    Refresh,
    Earlier,
}
#[derive(Clone, Debug)]
pub struct ReadOnlyTranscriptIntent {
    scope: RunScope,
    publication: u64,
    connection_epoch: u64,
    read: ReadOnlyTranscriptRead,
    claimed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl ReadOnlyTranscriptIntent {
    /// Executor admission is once per explicit gesture, including cloned handles.
    /// A failed or abandoned read requires another explicit user activation.
    pub fn claim(&self) -> bool {
        !self.claimed.swap(true, std::sync::atomic::Ordering::AcqRel)
    }
    pub fn scope(&self) -> &RunScope {
        &self.scope
    }
    pub fn publication(&self) -> u64 {
        self.publication
    }
    pub fn connection_epoch(&self) -> u64 {
        self.connection_epoch
    }
    pub fn read(&self) -> ReadOnlyTranscriptRead {
        self.read
    }
}
/// Same-UI-turn plan; not authority for a network history request.
#[derive(Debug)]
pub struct ReadOnlyTranscriptUpdate<'a> {
    root: ComponentId,
    owner: std::sync::Arc<()>,
    generation: u64,
    input: ReadOnlyTranscriptInput<'a>,
    content: TranscriptContentUpdate<'a>,
    status: String,
}
impl ReadOnlyTranscriptUpdate<'_> {
    pub fn item_ids(&self) -> Vec<ComponentId> {
        self.content.item_ids()
    }
}
/// Independently mounted history pane, with default library viewport/controls.
/// It cannot produce Send, Cancel, actionable responses or completed-turn actions.
#[derive(Debug)]
pub struct ReadOnlyTranscript {
    ids: ReadOnlyTranscriptIds,
    scope: RunScope,
    labels: ReadOnlyTranscriptLabels,
    paging: ComponentId,
    content: ComponentId,
    body: StackContent,
    publication: u64,
    epoch: u64,
    connection: Option<ConnectionPresentation>,
    refresh_enabled: bool,
    plan_owner: std::sync::Arc<()>,
    generation: u64,
}
impl ReadOnlyTranscript {
    /// Three consecutive namespaces for controls/paging/content; reserve the
    /// exclusive row pool separately across the embedding ContentHost.
    pub fn standard(
        namespace: u32,
        rows: std::ops::Range<u32>,
        scope: RunScope,
        labels: ReadOnlyTranscriptLabels,
    ) -> Result<Self, &'static str> {
        let content_namespace = namespace
            .checked_add(2)
            .ok_or("read-only namespace overflow")?;
        let base = (namespace as u64) << 32;
        let content = TranscriptContent::new(
            content_namespace,
            rows,
            scope.clone(),
            labels.content.clone(),
        )?;
        Self::new(
            ReadOnlyTranscriptIds {
                root: ComponentId(base),
                status: ComponentId(base + 1),
                refresh: ComponentId(base + 2),
                viewport: ComponentId(base + 3),
            },
            scope,
            namespace + 1,
            content,
            labels,
        )
    }
    pub fn new(
        ids: ReadOnlyTranscriptIds,
        scope: RunScope,
        paging_namespace: u32,
        content: TranscriptContent,
        labels: ReadOnlyTranscriptLabels,
    ) -> Result<Self, &'static str> {
        if content.feedback_enabled() {
            return Err("bounded read-only history cannot mount evaluation controls");
        }
        if content.scope() != &scope {
            return Err("read-only transcript scope mismatch");
        }
        let controls = [ids.root, ids.status, ids.refresh, ids.viewport];
        if controls
            .iter()
            .enumerate()
            .any(|(i, id)| controls[..i].contains(id) || id.0 >> 32 == paging_namespace as u64)
        {
            return Err("read-only transcript control overlap");
        }
        let mut reserved: Vec<_> = controls.iter().map(|id| (id.0 >> 32) as u32).collect();
        reserved.push(paging_namespace);
        if !content.namespaces_disjoint(&reserved) {
            return Err("read-only transcript namespace overlap");
        }
        let content_id = content.id();
        let paging = ComponentId((paging_namespace as u64) << 32);
        let viewport = ScrollContent::new(
            tekes_ui_controls::ScrollArea::default(),
            Content::new(content_id, content),
        );
        let mut refresh = ButtonContent::new(&labels.refresh);
        refresh.button.state.disabled = true;
        Ok(Self {
            ids,
            scope: scope.clone(),
            labels: labels.clone(),
            paging,
            content: content_id,
            body: StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Regular.value(),
                },
                flexible_child: Some(ids.viewport),
                children: vec![
                    Content::new(ids.status, TextContent::new(&labels.group)),
                    Content::new(ids.refresh, refresh),
                    Content::new(
                        paging,
                        TranscriptPaging::new(paging_namespace, scope, labels.paging),
                    ),
                    Content::new(ids.viewport, viewport),
                ],
            },
            publication: 0,
            epoch: 0,
            connection: None,
            refresh_enabled: false,
            plan_owner: std::sync::Arc::new(()),
            generation: 0,
        })
    }
    pub fn ids(&self) -> ReadOnlyTranscriptIds {
        self.ids
    }
    pub fn scope(&self) -> &RunScope {
        &self.scope
    }
    pub fn content_id(&self) -> ComponentId {
        self.content
    }
    pub fn paging_facts(&self) -> Option<&TranscriptPagingFacts> {
        self.body
            .children
            .iter()
            .find(|node| node.id() == self.paging)
            .unwrap()
            .component::<TranscriptPaging>()
            .unwrap()
            .facts()
    }
    pub fn paging_button_id(&self) -> ComponentId {
        ComponentId(self.paging.0 + 1)
    }
    pub fn content(&self) -> &TranscriptContent {
        self.body
            .children
            .iter()
            .find_map(|node| node.component::<ScrollContent>())
            .unwrap()
            .children()[0]
            .component::<TranscriptContent>()
            .unwrap()
    }
    fn content_mut(&mut self) -> &mut TranscriptContent {
        self.body
            .children
            .iter_mut()
            .find_map(|node| node.component_mut::<ScrollContent>())
            .unwrap()
            .children_mut()
            .unwrap()[0]
            .component_mut::<TranscriptContent>()
            .unwrap()
    }
    /// Validate the proposed frame before a host replaces its last readable cache.
    pub fn validate_update(&self, input: &ReadOnlyTranscriptInput<'_>) -> Result<(), &'static str> {
        if input.scope != &self.scope {
            return Err("read-only transcript source mismatch");
        }
        if input.publication == 0
            || input.publication < self.publication
            || input.connection_epoch < self.epoch
        {
            return Err("stale read-only transcript publication");
        }
        if input.publication == self.publication
            && (input.connection_epoch != self.epoch
                || self.connection.as_ref() != Some(input.connection))
        {
            return Err("conflicting read-only transcript publication");
        }
        self.content().validate_update(
            input.scope,
            input.publication,
            input.connection_epoch,
            input.transcript,
            false,
        )
    }
    pub fn prepare_update<'a>(
        &self,
        input: ReadOnlyTranscriptInput<'a>,
    ) -> Result<ReadOnlyTranscriptUpdate<'a>, &'static str> {
        self.validate_update(&input)?;
        self.generation
            .checked_add(1)
            .ok_or("read-only display generation exhausted")?;
        let content = self.content().prepare_update(
            input.scope,
            input.publication,
            input.connection_epoch,
            input.transcript,
            false,
        )?;
        let mut text = (self.labels.information)(
            input.connection,
            input.transcript.items.len(),
            input.frame_current,
        );
        if !input.frame_current || input.transcript.is_stale {
            text.push_str(&format!("\n{}", self.labels.stale));
        }
        if input.reading {
            text.push_str(&format!("\n{}", self.labels.loading));
        }
        if let Some(error) = input.read_error {
            text.push_str(&format!("\n{error}"));
        }
        Ok(ReadOnlyTranscriptUpdate {
            root: self.ids.root,
            owner: self.plan_owner.clone(),
            generation: self.generation,
            input,
            content,
            status: text,
        })
    }
    pub fn validate_prepared_update(
        &self,
        plan: &ReadOnlyTranscriptUpdate<'_>,
    ) -> Result<(), &'static str> {
        if self.ids.root != plan.root
            || !std::sync::Arc::ptr_eq(&self.plan_owner, &plan.owner)
            || self.generation != plan.generation
        {
            return Err("stale or foreign read-only update plan");
        }
        self.validate_update(&plan.input)?;
        self.content().validate_prepared_update(&plan.content)
    }
    pub fn update(&mut self, input: ReadOnlyTranscriptInput<'_>) -> Result<(), &'static str> {
        let plan = self.prepare_update(input)?;
        self.apply_prepared_update(&plan)
    }
    pub fn apply_prepared_update(
        &mut self,
        plan: &ReadOnlyTranscriptUpdate<'_>,
    ) -> Result<(), &'static str> {
        self.validate_prepared_update(plan)?;
        let input = &plan.input;
        self.content_mut().apply_prepared_update(&plan.content)?;
        self.body.children[0]
            .component_mut::<TextContent>()
            .unwrap()
            .text = plan.status.clone();
        self.refresh_enabled = input.source_ready && !input.reading;
        self.body.children[1]
            .component_mut::<ButtonContent>()
            .unwrap()
            .button
            .state
            .disabled = !self.refresh_enabled;
        self.body.children[2]
            .component_mut::<TranscriptPaging>()
            .unwrap()
            .update(TranscriptPagingFacts {
                scope: self.scope.clone(),
                supported: input.supports_pagination,
                has_more_before: input.transcript.has_more_before,
                is_stale: input.transcript.is_stale || !input.frame_current,
                source_ready: input.source_ready && input.frame_current,
                read: if input.reading {
                    TranscriptPageRead::Loading
                } else if let Some(error) = input.read_error {
                    TranscriptPageRead::Failed(error.into())
                } else {
                    TranscriptPageRead::Idle
                },
            })?;
        self.generation += 1;
        self.publication = input.publication;
        self.epoch = input.connection_epoch;
        self.connection = Some(input.connection.clone());
        Ok(())
    }
    pub fn take_copy_intents(&self, actions: &[ContentAction]) -> Vec<crate::TranscriptCopyIntent> {
        self.content().take_copy_intents(actions)
    }
    pub fn take_intents(&mut self, actions: &[ContentAction]) -> Vec<ReadOnlyTranscriptIntent> {
        // Local disclosures are shared. This lane never dispatches a mutation.
        let _ = self.content_mut().handle_actions(actions);
        let mut result = vec![];
        if self.refresh_enabled
            && actions.iter().any(|action| {
                action.target == self.ids.refresh && action.kind == ContentActionKind::Activate
            })
        {
            result.push(ReadOnlyTranscriptRead::Refresh);
        }
        if self.body.children[2]
            .component::<TranscriptPaging>()
            .unwrap()
            .activation(actions)
            .is_some()
        {
            result.push(ReadOnlyTranscriptRead::Earlier);
        }
        result
            .into_iter()
            .map(|read| ReadOnlyTranscriptIntent {
                scope: self.scope.clone(),
                publication: self.publication,
                connection_epoch: self.epoch,
                read,
                claimed: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            })
            .collect()
    }
    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let mut host = ContentHost::new(Content::new(self.ids.root, self))?;
        host.set_action_events(true);
        Ok(host)
    }
}
impl Component for ReadOnlyTranscript {
    fn lifecycle(&mut self, _: Lifecycle) {
        self.plan_owner = std::sync::Arc::new(());
    }
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
        ContentSemantics::new(ContentRole::Group, &self.labels.group)
    }
}
