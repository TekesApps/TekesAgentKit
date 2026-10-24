// Candidate-only test of the ACTUAL AgentKit wrapper and its retained control.
#[cfg(test)]
mod forwarded_focus_candidate_tests {
    use super::*;

    #[test]
    fn actual_turn_rail_delegates_library_focus_decoration_state() {
        let mut action = TurnRailAction::new(ComponentId(999));
        action.button.action.button.frame_policy = tekes_ui_controls::ButtonFramePolicy::Exact;
        assert!(!action.focus_decoration_above_siblings());
        action.set_focus(true, true);
        assert!(action.button.focus_decoration_above_siblings());
        assert!(action.focus_decoration_above_siblings());
        action.button.action.button.state.disabled = true;
        assert!(!action.button.focus_decoration_above_siblings());
        assert!(!action.focus_decoration_above_siblings());
        action.button.action.button.state.disabled = false;
        action.set_focus(true, false);
        assert!(!action.button.focus_decoration_above_siblings());
        assert!(!action.focus_decoration_above_siblings());
        action.set_focus(false, false);
        assert!(!action.focus_decoration_above_siblings());
    }
}
