//! Typed child run ownership; layout and control capabilities are independent.
use crate::{PaneSnapshot, RunBinding};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tekes_agent_components_ui::SidePanelTab;
use tekes_agent_contract::Result;
use tekes_agent_presentation::{ChildHistoryLayout, ChildHistorySource, RunScope};
use tekes_agent_session::SessionRuntime;

/// A source role proof, not a second child/capability state machine. Retiring the
/// pane revokes captured controls without cancelling remote work or removing its
/// history. A new mount gets a new lifetime even for the same child ID.
pub struct ChildPaneBinding {
    parent: RunScope,
    child_id: String,
    layout: ChildHistoryLayout,
    run: Arc<RunBinding>,
    parent_binding: Option<std::sync::Weak<RunBinding>>,
    alive: Arc<AtomicBool>,
    paging: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChildRefreshKey {
    pub connection_epoch: u64,
    pub parent_running: bool,
    pub owner_archived: bool,
}
impl ChildPaneBinding {
    pub async fn open(
        runtime: Arc<SessionRuntime>,
        direct_parent: RunScope,
        child_id: impl Into<String>,
        layout: ChildHistoryLayout,
        source: Option<Arc<dyn ChildHistorySource>>,
    ) -> Result<Self> {
        Self::open_with_page_size(runtime, direct_parent, child_id, layout, source, 50).await
    }
    /// Explicit bounded page size (1..=200), fixed for this source lifetime.
    pub async fn open_with_page_size(
        runtime: Arc<SessionRuntime>,
        direct_parent: RunScope,
        child_id: impl Into<String>,
        layout: ChildHistoryLayout,
        source: Option<Arc<dyn ChildHistorySource>>,
        page_size: usize,
    ) -> Result<Self> {
        tekes_agent_contract::require((1..=200).contains(&page_size), "invalid bounded page size")?;
        tekes_agent_contract::require(
            page_size == 50 || (layout == ChildHistoryLayout::SameSession && source.is_none()),
            "page size requires built-in bounded history",
        )?;
        let child_id = child_id.into();
        let paging = source.is_none();
        let mut run = RunBinding::open_child_with_page_size(
            runtime,
            direct_parent.clone(),
            child_id.clone(),
            layout,
            source,
            page_size,
        )
        .await?;
        let alive = Arc::new(AtomicBool::new(true));
        run.child_owner = Some(Arc::downgrade(&alive));
        Ok(Self {
            parent: direct_parent,
            child_id,
            layout,
            run: Arc::new(run),
            parent_binding: None,
            alive,
            paging,
        })
    }
    pub async fn for_run(
        parent: &RunBinding,
        child_id: impl Into<String>,
        layout: ChildHistoryLayout,
        source: Option<Arc<dyn ChildHistorySource>>,
    ) -> Result<Self> {
        Self::for_run_with_page_size(parent, child_id, layout, source, 50).await
    }
    /// Open a child through an existing parent binding with the same bounded
    /// history size as `open_with_page_size`. The parent lifetime is inherited
    /// across the history read, and is rechecked before returning.
    pub async fn for_run_with_page_size(
        parent: &RunBinding,
        child_id: impl Into<String>,
        layout: ChildHistoryLayout,
        source: Option<Arc<dyn ChildHistorySource>>,
        page_size: usize,
    ) -> Result<Self> {
        Self::for_run_inner(parent, None, child_id, layout, source, page_size).await
    }
    /// Host variant that also binds cache reuse to this exact parent object.
    /// It must be called with the binding captured before asynchronous work.
    pub async fn for_run_with_captured_parent(
        parent: &Arc<RunBinding>,
        child_id: impl Into<String>,
        layout: ChildHistoryLayout,
        source: Option<Arc<dyn ChildHistorySource>>,
        page_size: usize,
    ) -> Result<Self> {
        Self::for_run_inner(
            parent,
            Some(Arc::downgrade(parent)),
            child_id,
            layout,
            source,
            page_size,
        )
        .await
    }
    async fn for_run_inner(
        parent: &RunBinding,
        parent_binding: Option<std::sync::Weak<RunBinding>>,
        child_id: impl Into<String>,
        layout: ChildHistoryLayout,
        source: Option<Arc<dyn ChildHistorySource>>,
        page_size: usize,
    ) -> Result<Self> {
        tekes_agent_contract::require(
            parent.access() != crate::PaneAccess::Missing,
            "child parent pane retired",
        )?;
        let authority = parent.authority();
        let mut child = Self::open_with_page_size(
            parent.runtime.clone(),
            parent.scope().clone(),
            child_id,
            layout,
            source,
            page_size,
        )
        .await?;
        child.parent_binding = parent_binding;
        let run = Arc::get_mut(&mut child.run).expect("unpublished child binding");
        run.ancestors.inherit(&authority);
        tekes_agent_contract::require(
            run.access() != crate::PaneAccess::Missing,
            "child parent pane retired during history read",
        )?;
        Ok(child)
    }
    pub fn parent(&self) -> &RunScope {
        &self.parent
    }
    pub fn child_id(&self) -> &str {
        &self.child_id
    }
    pub fn layout(&self) -> ChildHistoryLayout {
        self.layout
    }
    /// Custom ChildHistorySource owns its own pagination; do not enable the
    /// endpoint page button solely because its projection has more history.
    pub fn supports_pagination(&self) -> bool {
        self.paging
    }
    pub fn binding(&self) -> &Arc<RunBinding> {
        &self.run
    }
    pub fn tab(&self, title: impl Into<String>) -> SidePanelTab {
        let mut tab = SidePanelTab::subagent(self.child_id.clone(), title, self.layout);
        tab.child_parent = Some(self.parent.clone());
        tab
    }
    pub fn retire(&self) {
        self.alive.store(false, Ordering::Release);
    }
    /// Catalog refresh is a read. On failure existing presentation removes
    /// control authority, while callers may still show the readable history.
    pub async fn refresh_controls(&self) -> Result<()> {
        tekes_agent_contract::require(
            self.run.access() != crate::PaneAccess::Missing,
            "child pane retired",
        )?;
        self.run.refresh_child_catalog().await
    }
    /// Render one original read frame. A later transport sample can only lower
    /// page authority; refresh remains explicit once the source is ready again.
    pub fn readonly_input<'a>(
        &self,
        snapshot: &'a PaneSnapshot,
        reading: bool,
        read_error: Option<&'a str>,
    ) -> Result<tekes_agent_components_ui::ReadOnlyTranscriptInput<'a>> {
        let PaneSnapshot::ReadOnlyChild {
            source: source_id,
            scope,
            publication,
            connection_epoch,
            connection,
            transcript,
        } = snapshot
        else {
            return Err(tekes_agent_contract::unsupported(
                "child is not a read-only source",
            ));
        };
        tekes_agent_contract::require(
            self.layout == ChildHistoryLayout::SameSession
                && source_id == &self.run.readonly_source
                && scope == self.run.scope()
                && *publication > 0
                && self.run.access() != crate::PaneAccess::Missing,
            "read-only child frame origin mismatch",
        )?;
        let ready = self.refresh_key();
        Ok(tekes_agent_components_ui::ReadOnlyTranscriptInput {
            source: Some(source_id),
            scope,
            publication: *publication,
            connection_epoch: *connection_epoch,
            connection,
            transcript,
            frame_current: ready.is_some_and(|key| key.connection_epoch == *connection_epoch),
            source_ready: ready.is_some(),
            supports_pagination: self.supports_pagination(),
            reading,
            read_error,
        })
    }
    /// Execute the original explicit read on this same source, checking again
    /// immediately before IO. Refresh can deliberately renew an old epoch;
    /// pagination cannot silently acquire authority in a newer connection.
    pub async fn read_intent(
        &self,
        intent: tekes_agent_components_ui::ReadOnlyTranscriptIntent,
    ) -> Result<PaneSnapshot> {
        self.admit_read_intent(intent).await?;
        self.snapshot().await
    }
    /// Execute the original read gesture and capture the accumulated bounded
    /// source from that same pane publication. This is acceptance-only and
    /// does not issue an additional refresh or page request.
    #[cfg(feature = "acceptance-source-witness")]
    pub async fn read_intent_with_paged_source_witness(
        &self,
        intent: tekes_agent_components_ui::ReadOnlyTranscriptIntent,
    ) -> Result<crate::BoundedChildPagedPaneWitness> {
        self.admit_read_intent(intent).await?;
        self.run.snapshot_with_bounded_paged_source_witness().await
    }
    async fn admit_read_intent(
        &self,
        intent: tekes_agent_components_ui::ReadOnlyTranscriptIntent,
    ) -> Result<()> {
        use tekes_agent_components_ui::ReadOnlyTranscriptRead;
        tekes_agent_contract::require(
            intent.belongs_to(&self.run.readonly_source)
                && intent.scope() == self.run.scope()
                && self.layout == ChildHistoryLayout::SameSession
                && self.run.access() != crate::PaneAccess::Missing
                && intent.publication() > 0
                && intent.publication() <= self.run.readonly_reads.load(Ordering::Acquire),
            "read-only child intent source mismatch",
        )?;
        tekes_agent_contract::require(intent.claim(), "read-only child gesture already admitted")?;
        let key = self
            .refresh_key()
            .ok_or_else(|| tekes_agent_contract::unsupported("read-only child source not ready"))?;
        if intent.read() == ReadOnlyTranscriptRead::Earlier {
            tekes_agent_contract::require(
                self.supports_pagination() && key.connection_epoch == intent.connection_epoch(),
                "read-only child page epoch expired",
            )?;
            self.load_earlier().await?;
        }
        Ok(())
    }
    pub async fn snapshot(&self) -> Result<PaneSnapshot> {
        self.run.snapshot().await
    }
    pub async fn load_earlier(&self) -> Result<bool> {
        self.run.load_earlier().await
    }
    pub fn uses_run(&self, parent: &RunBinding) -> bool {
        Arc::ptr_eq(&self.run.runtime, &parent.runtime)
    }
    /// True only for the exact parent authority captured by the host open.
    pub fn authorized_by(&self, parent: &Arc<RunBinding>) -> bool {
        self.parent_binding
            .as_ref()
            .is_some_and(|captured| captured.ptr_eq(&Arc::downgrade(parent)))
            && parent.access() != crate::PaneAccess::Missing
            && self.run.access() != crate::PaneAccess::Missing
    }
    /// Validate install ownership. Legacy sources created through `for_run`
    /// retain their runtime-scoped compatibility; host-captured sources require
    /// the exact parent object and cannot cross a replacement binding.
    pub fn accepts_parent_binding(&self, parent: &Arc<RunBinding>) -> bool {
        if self.parent_binding.is_some() {
            self.authorized_by(parent)
        } else {
            self.uses_run(parent) && parent.access() != crate::PaneAccess::Missing
        }
    }
    /// Host read scheduling only; this does not grant continuation authority.
    /// Consumers observe runtime notifications and coalesce unchanged keys.
    pub fn refresh_key(&self) -> Option<ChildRefreshKey> {
        if self.run.access() == crate::PaneAccess::Missing {
            return None;
        }
        self.run.runtime.inspect(|state| {
            (state.connection == tekes_agent_session::ConnectionState::Ready).then(|| {
                ChildRefreshKey {
                    connection_epoch: state.connection_epoch,
                    parent_running: state
                        .sessions
                        .get(&self.parent.session_id)
                        .is_some_and(|parent| parent.run_open),
                    owner_archived: [&self.parent.session_id, &self.parent.thread_id]
                        .iter()
                        .any(|id| {
                            state.archived_session_ids.contains(id)
                                || state
                                    .sessions
                                    .get(*id)
                                    .and_then(|session| session.summary.as_ref())
                                    .is_some_and(|summary| summary["status"] == "archived")
                        }),
                }
            })
        })
    }
}
