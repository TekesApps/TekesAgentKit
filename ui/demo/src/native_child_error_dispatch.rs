//! Pure admission decision shared by native child-open error dispatch and its regression.

use tekes_agent_ui_demo::component_demo::{ComponentDemo, PanelRequest};

/// Returns the host notice only while the request still owns the current child-open slot.
/// The caller remains responsible for delivering the notice through its normal host path.
pub(super) fn current_child_open_error(
    view: &ComponentDemo,
    request: &PanelRequest,
    error: String,
) -> Option<String> {
    view.child_request_current(request).then_some(error)
}
