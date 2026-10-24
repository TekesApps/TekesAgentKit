//! Acceptance capture is an external host capability, never a demo permission.
pub(crate) use tekes_ui_platform_macos::selftest::Action;

fn external_capture(action: Action) -> Action {
    match action {
        Action::Capture { path } => Action::Log(format!(
            "AGENTKIT_EXTERNAL_CAPTURE_REQUEST {}",
            serde_json::json!({"path": path, "capturePerformed": false})
        )),
        action => action,
    }
}

pub(crate) fn run_script(steps: Vec<(u64, Action)>) {
    tekes_ui_platform_macos::selftest::run_script(
        steps
            .into_iter()
            .map(|(delay, action)| (delay, external_capture(action)))
            .collect(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_only_emits_an_external_request() {
        let path = "/tmp/中文证据.png";
        let Action::Log(message) = external_capture(Action::Capture { path: path.into() }) else {
            panic!("capture must never reach the SDK capture action");
        };
        let json = message
            .strip_prefix("AGENTKIT_EXTERNAL_CAPTURE_REQUEST ")
            .unwrap();
        let request: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(request["path"], path);
        assert_eq!(request["capturePerformed"], false);
        assert!(matches!(external_capture(Action::Close), Action::Close));
    }
}
