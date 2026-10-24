//! Shared host boundary for Summary and FamilyCatalog child-pane opens.
//! The parent is captured synchronously before dispatch and is never replaced
//! by a scope lookup after the history read begins.
use std::sync::Arc;

use tekes_agent_contract::{Result, require};
use tekes_agent_presentation::{ChildHistoryLayout, RunScope};
use tekes_agent_ui_bindings::{ChildPaneBinding, PaneAccess, PaneSnapshot, RunBinding};

/// Open or reuse a child pane using the original captured parent authority.
/// Existing cached sources are reused only when they were created by this exact
/// parent binding and remain live. Parent retirement during history loading
/// rejects the result before a caller can install it.
pub async fn open_child_for_parent(
    parent: Arc<RunBinding>,
    direct_parent: &RunScope,
    child_id: &str,
    layout: ChildHistoryLayout,
    page_size: usize,
    cached: Option<Arc<ChildPaneBinding>>,
    history_source: Option<Arc<dyn tekes_agent_presentation::ChildHistorySource>>,
) -> Result<(Arc<ChildPaneBinding>, PaneSnapshot)> {
    require(
        (1..=200).contains(&page_size)
            && (page_size == 50
                || (layout == ChildHistoryLayout::SameSession && history_source.is_none())),
        "invalid bounded child page size",
    )?;
    require(
        parent.scope() == direct_parent && parent.access() != PaneAccess::Missing,
        "child parent binding is missing, retired, or has the wrong scope",
    )?;
    let source = match cached.filter(|source| {
        source.parent() == direct_parent
            && source.child_id() == child_id
            && source.layout() == layout
            && source.authorized_by(&parent)
    }) {
        Some(source) => source,
        None => Arc::new(
            ChildPaneBinding::for_run_with_captured_parent(
                &parent,
                child_id,
                layout,
                history_source,
                page_size,
            )
            .await?,
        ),
    };
    require(
        source.authorized_by(&parent),
        "child pane source is not authorized by the captured parent",
    )?;
    let snapshot = source.snapshot().await?;
    require(
        parent.access() != PaneAccess::Missing
            && source.authorized_by(&parent)
            && source.binding().access() != PaneAccess::Missing,
        "child parent pane retired during history read",
    )?;
    Ok((source, snapshot))
}
