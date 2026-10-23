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
        tekes_agent_contract::require(
            parent.access() != crate::PaneAccess::Missing,
            "child parent pane retired",
        )?;
        let authority = parent.authority();
        let mut child = Self::open(
            parent.runtime.clone(),
            parent.scope().clone(),
            child_id,
            layout,
            source,
        )
        .await?;
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
        self.snapshot().await
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
