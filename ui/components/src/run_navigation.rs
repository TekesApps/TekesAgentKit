//! Agent gesture admission and history hooks over the original SDK viewport.
use crate::{
    RunTurnNavigation, RunTurnNavigationHost, RunTurnNavigationKind, RunView, TurnRailRevealReason,
};
use tekes_ui_controls::{
    ButtonTextRenderer, ReadingDocumentVersion, ReadingUpdateError, ScrollRevealOptions,
    ScrollRevealPlacement, composition::*, extended::Axis,
};

#[derive(Debug)]
pub enum RunNavigationError {
    Agent(&'static str),
    Reading(ReadingUpdateError),
    Reveal(ContentRevealError),
}
#[derive(Debug)]
pub enum RunNavigationResult {
    TurnStart(ContentRevealResult),
    Latest { changed: bool },
}
/// Hosts explicitly choose their navigation role. Main history remains a host
/// capability; auxiliary panes never manufacture main-window history entries.
pub enum RunNavigationHost<'a> {
    Main(&'a mut dyn RunTurnNavigationHost),
    Auxiliary,
}
fn run(host: &ContentHost, root: ComponentId) -> Result<&RunView, RunNavigationError> {
    host.get(root)
        .and_then(|node| node.component::<RunView>())
        .ok_or(RunNavigationError::Agent(
            "Run navigation owner is not mounted",
        ))
}
fn check_version(
    host: &ContentHost,
    viewport: ComponentId,
    version: &ReadingDocumentVersion,
) -> Result<(), RunNavigationError> {
    let current = host
        .get(viewport)
        .and_then(|node| node.component::<ScrollContent>())
        .and_then(ScrollContent::reading_document_version)
        .ok_or(RunNavigationError::Agent(
            "Run navigation reading document is not attached",
        ))?;
    if current != version {
        return Err(RunNavigationError::Agent(
            "stale Run navigation reading token",
        ));
    }
    Ok(())
}
/// Supply the original event's reading token and invoke synchronously. No
/// callback runs for a stale attachment/gesture. A claimed SDK failure is not
/// rolled back or retried; callback panics are not arbitrary state transactions.
/// This never replaces the Transcript reading document or touches its editor.
pub fn navigate_run_turn(
    host: &mut ContentHost,
    root: ComponentId,
    intent: &RunTurnNavigation,
    version: &ReadingDocumentVersion,
    navigation: RunNavigationHost<'_>,
    animated: bool,
    text: &mut dyn ButtonTextRenderer,
) -> Result<RunNavigationResult, RunNavigationError> {
    check_version(host, intent.viewport(), version)?;
    let view = run(host, root)?;
    let kind = match navigation {
        RunNavigationHost::Main(history) => view.claim_turn_navigation_with_host(intent, history),
        RunNavigationHost::Auxiliary => {
            if intent.host_jump().is_some() {
                return Err(RunNavigationError::Agent(
                    "main navigation requires host history callbacks",
                ));
            }
            view.claim_turn_navigation(intent)
        }
    }
    .map_err(RunNavigationError::Agent)?;
    match kind {
        RunTurnNavigationKind::TurnStart(item) => host
            .reveal_scroll_reading_item(
                intent.viewport(),
                version,
                item,
                ScrollRevealOptions {
                    placement: ScrollRevealPlacement::Start(Axis::Vertical),
                    animated,
                    ..Default::default()
                },
                text,
            )
            .map(RunNavigationResult::TurnStart)
            .map_err(RunNavigationError::Reveal),
        RunTurnNavigationKind::Latest => {
            let changed = host
                .get_mut(intent.viewport())
                .unwrap()
                .component_mut::<ScrollContent>()
                .unwrap()
                .follow_latest();
            Ok(RunNavigationResult::Latest { changed })
        }
    }
}
/// The rail has its own original SDK reading attachment, distinct from the
/// Transcript's full document. Reject a foreign host document rather than reset
/// it. Call after a canonical Run update and before the next successful render.
pub fn sync_run_rail_reading(
    host: &mut ContentHost,
    root: ComponentId,
) -> Result<ReadingDocumentVersion, RunNavigationError> {
    let view = run(host, root)?;
    let rail = view
        .turn_rail()
        .ok_or(RunNavigationError::Agent("Run rail is not installed"))?;
    let viewport = rail.viewport_id();
    let revision = rail.document_revision();
    let items = rail.item_ids();
    let key = format!(
        "agentkit-turn-rail:{}:{}",
        root.0,
        crate::ComposerDestination::run(view.scope().clone()).storage_key()
    );
    let version = host
        .get(viewport)
        .and_then(|node| node.component::<ScrollContent>())
        .and_then(ScrollContent::reading_document_version)
        .cloned();
    match version {
        Some(version) if version.key() != key => Err(RunNavigationError::Agent(
            "foreign Run rail reading document",
        )),
        Some(version) if version.revision() == revision => Ok(version),
        Some(version) => host
            .update_scroll_reading_document(viewport, &version, revision, items, |_| ())
            .map(|(version, ())| version)
            .map_err(RunNavigationError::Reading),
        None => host
            .set_scroll_reading_document(viewport, key, revision, items, |_| ())
            .map(|(version, ())| version)
            .map_err(RunNavigationError::Reading),
    }
}
/// After a successful current layout, honor the original rail's initial/follow
/// request using its separate viewport. No Transcript navigation/history hook.
pub fn reveal_run_rail_follow(
    host: &mut ContentHost,
    root: ComponentId,
    version: &ReadingDocumentVersion,
    animated: bool,
    text: &mut dyn ButtonTextRenderer,
) -> Result<Option<ContentRevealResult>, RunNavigationError> {
    let rail_id = run(host, root)?
        .turn_rail()
        .ok_or(RunNavigationError::Agent("Run rail is not installed"))?
        .id();
    let request = host
        .get_mut(rail_id)
        .unwrap()
        .component_mut::<crate::TurnRail>()
        .unwrap()
        .next_reveal();
    let Some(request) = request else {
        return Ok(None);
    };
    host.get(rail_id)
        .unwrap()
        .component::<crate::TurnRail>()
        .unwrap()
        .claim_reveal(&request)
        .map_err(RunNavigationError::Agent)?;
    if let Err(error) = check_version(host, request.viewport(), version) {
        host.get_mut(rail_id)
            .unwrap()
            .component_mut::<crate::TurnRail>()
            .unwrap()
            .finish_reveal(&request, false);
        return Err(error);
    }
    let result = host.reveal_scroll_reading_item(
        request.viewport(),
        version,
        request.item(),
        ScrollRevealOptions {
            placement: match request.reason() {
                TurnRailRevealReason::InitialLatest => ScrollRevealPlacement::Nearest,
                TurnRailRevealReason::FollowChanged => ScrollRevealPlacement::Centered,
            },
            animated,
            ..Default::default()
        },
        text,
    );
    host.get_mut(rail_id)
        .unwrap()
        .component_mut::<crate::TurnRail>()
        .unwrap()
        .finish_reveal(&request, result.is_ok());
    result.map(Some).map_err(RunNavigationError::Reveal)
}

/// Jump is explicit local navigation, with no main-window history callback.
pub fn jump_run_latest(
    host: &mut ContentHost,
    root: ComponentId,
    intent: &crate::RunJumpLatest,
    version: &ReadingDocumentVersion,
) -> Result<bool, RunNavigationError> {
    check_version(host, intent.viewport(), version)?;
    run(host, root)?
        .claim_jump_to_latest(intent)
        .map_err(RunNavigationError::Agent)?;
    Ok(host
        .get_mut(intent.viewport())
        .unwrap()
        .component_mut::<ScrollContent>()
        .unwrap()
        .follow_latest())
}
