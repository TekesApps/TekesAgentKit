//! Invalid acceptance flags must fail before host/app/window construction.
use std::process::Command;

fn rejected(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_tekes-agent-ui-demo"))
        .args(args)
        .env_clear()
        .env("LANG", "en_US.UTF-8")
        .output()
        .expect("explicit demo executable");
    assert!(!output.status.success());
    String::from_utf8(output.stderr).expect("Rust rejection text")
}

#[test]
fn either_source_flag_requires_offscreen_before_host_dispatch() {
    for flag in [
        "--child-pane-source-proof",
        "--child-pane-source-receipt",
        "--child-pane-source-proof=not-read.json",
        "--child-pane-source-unknown",
    ] {
        let error = rejected(&["--components", flag, "not-read.json"]);
        assert!(
            error.contains(if cfg!(feature = "acceptance-source-witness") {
                "offscreen"
            } else {
                "acceptance-source-witness feature"
            })
        );
    }
}

#[cfg(not(feature = "acceptance-source-witness"))]
#[test]
fn default_program_rejects_even_complete_source_flags_before_reading_files() {
    let error = rejected(&[
        "--components",
        "--child-pane-source-proof",
        "not-read.json",
        "--child-pane-source-receipt",
        "not-written.json",
        "--offscreen",
        "not-rendered.png",
        "--endpoint-config",
        "not-read-endpoints.json",
    ]);
    assert!(error.contains("acceptance-source-witness feature"));
}
