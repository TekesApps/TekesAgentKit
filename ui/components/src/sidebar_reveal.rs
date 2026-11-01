//! Business selection fencing over the original SDK item reveal.
use crate::{Sidebar, SidebarVisibilityTarget};
use tekes_ui_controls::composition::{ContentHost, ContentRevealError, ContentRevealResult};
use tekes_ui_controls::{ButtonTextRenderer, ReadingDocumentVersion, ScrollRevealOptions};

#[derive(Debug)]
pub enum SidebarRevealError {
    Selection(&'static str),
    Reveal(ContentRevealError),
}
/// Consume the original explicit-navigation capture against the host's original
/// reading token. Never replace its document, focus an editor or repair pixels.
/// An unready SDK layout is deferred before claiming the logical capture.
/// After a claim, rejected library commands stay consumed and need a new
/// explicit navigation; this helper never replays a failed command.
pub fn reveal_sidebar_selection(
    host: &mut ContentHost,
    target: &SidebarVisibilityTarget,
    version: &ReadingDocumentVersion,
    options: ScrollRevealOptions,
    text: &mut dyn ButtonTextRenderer,
) -> Result<ContentRevealResult, SidebarRevealError> {
    let sidebar = host
        .get(target.root())
        .and_then(|node| node.component::<Sidebar>())
        .ok_or(SidebarRevealError::Selection(
            "Sidebar reveal owner is not mounted",
        ))?;
    if !sidebar.visibility_reveal_ready() {
        return Err(SidebarRevealError::Selection(
            "Sidebar reveal layout is not ready",
        ));
    }
    sidebar
        .claim_visibility(target)
        .map_err(SidebarRevealError::Selection)?;
    host.reveal_scroll_reading_item(target.viewport(), version, target.item(), options, text)
        .map_err(SidebarRevealError::Reveal)
}
