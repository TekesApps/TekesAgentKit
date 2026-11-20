//! Topology A (`tekes-agent-ui-demo`) native AppHost desktop launcher.
//! Zero `unsafe` code; Metal rendering, CoreText, and macOS AX live strictly in TekesUI.

use std::time::Duration;
use tekes_agent_ui_demo::workbench_app::{
    TopologyALaunchOptions, build_topology_a_app, initialize_topology_a_runtime,
    render_topology_a_offscreen_png,
};
use tekes_ui_platform_macos::selftest::{Action, run_script};

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    if tekes_ui_app_host::run_component_worker_from_args(&args)
        .expect("AppHost component worker dispatch")
    {
        return;
    }

    let options = TopologyALaunchOptions::parse_args(&args).expect("parse demo CLI flags");
    let rt = initialize_topology_a_runtime(&options).expect("initialize Topology A runtime");
    let app = build_topology_a_app(&rt, &options).expect("build Topology A AppWindow");

    if let Some(path) = &options.offscreen {
        render_topology_a_offscreen_png(app, path, 1440, 900)
            .expect("render Topology A offscreen PNG");
        rt.executor.block_on(async {
            let _ = tokio::time::timeout(Duration::from_secs(2), rt.runtime.stop()).await;
        });
        return;
    }

    if options.native_selftest {
        run_script(vec![
            (
                600,
                Action::Log("SELFTEST_START Topology A native macOS AppWindow".into()),
            ),
            (
                900,
                Action::Resize {
                    w: 1280.0,
                    h: 820.0,
                },
            ),
            (1200, Action::WindowDark(true)),
            (1500, Action::WindowLight),
            (
                1800,
                Action::ClickPublished {
                    label: "输入消息",
                    secondary: false,
                },
            ),
            (2100, Action::Preedit("nihao".into())),
            (2300, Action::Text("你好世界".into())),
            (2600, Action::Log("SELFTEST_OK".into())),
            (2900, Action::Close),
        ]);
        app.run_with_args(&[args[0].clone()])
            .expect("Topology A native selftest AppWindow");
        println!("SELFTEST_OK");
        return;
    }

    app.run_with_args(&[args[0].clone()])
        .expect("Topology A AppWindow run");
}
