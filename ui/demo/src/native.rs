//! Topology A (`tekes-agent-ui-demo`) native AppHost desktop launcher.
//! Zero `unsafe` code; Metal rendering, CoreText, and macOS AX live strictly in TekesUI.

use std::time::Duration;
use tekes_agent_ui_demo::showcase::build_showcase_app;
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
    let app = if options.showcase || options.component.is_some() {
        build_showcase_app(&rt, &options).expect("build showcase AppWindow")
    } else {
        build_topology_a_app(&rt, &options).expect("build Topology A AppWindow")
    };

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
            (
                2500,
                Action::ExpectLifecycle {
                    label: "topology_a_initial",
                    primary_visible: Some(true),
                    primary_dormant: Some(false),
                    visible_windows: Some(1),
                    close_menu_title: Some("Close Tab"),
                    close_menu_enabled: Some(true),
                    new_window_enabled: Some(true),
                    redraws_delta_zero_since: None,
                },
            ),
            (
                2700,
                Action::KeyEquivalent {
                    chars: "w",
                    key_code: 13,
                    control: false,
                    alt: false,
                    shift: false,
                    command: true,
                },
            ),
            (
                2900,
                Action::ExpectLifecycle {
                    label: "topology_a_after_cmd_w_tab",
                    primary_visible: Some(true),
                    primary_dormant: Some(false),
                    visible_windows: Some(1),
                    close_menu_title: Some("Close Tab"),
                    close_menu_enabled: Some(true),
                    new_window_enabled: Some(true),
                    redraws_delta_zero_since: None,
                },
            ),
            (3050, Action::Close),
            (
                3250,
                Action::ExpectLifecycle {
                    label: "topology_a_after_red_close",
                    primary_visible: Some(false),
                    primary_dormant: Some(true),
                    visible_windows: Some(0),
                    close_menu_title: Some("Close Window"),
                    close_menu_enabled: Some(false),
                    new_window_enabled: Some(true),
                    redraws_delta_zero_since: None,
                },
            ),
            (3400, Action::Reopen),
            (
                3650,
                Action::ExpectLifecycle {
                    label: "topology_a_after_dock_reopen",
                    primary_visible: Some(true),
                    primary_dormant: Some(false),
                    visible_windows: Some(1),
                    close_menu_title: Some("Close Tab"),
                    close_menu_enabled: Some(true),
                    new_window_enabled: Some(true),
                    redraws_delta_zero_since: None,
                },
            ),
            (3800, Action::Log("SELFTEST_OK".into())),
            (3950, Action::Quit),
        ]);
        app.run_with_args(&[args[0].clone()])
            .expect("Topology A native selftest AppWindow");
        rt.executor.block_on(async {
            let _ = tokio::time::timeout(Duration::from_secs(2), rt.runtime.stop()).await;
        });
        drop(rt);
        println!("SELFTEST_OK");
        return;
    }

    app.run_with_args(&[args[0].clone()])
        .expect("Topology A AppWindow run");
    rt.executor.block_on(async {
        let _ = tokio::time::timeout(Duration::from_secs(2), rt.runtime.stop()).await;
    });
}
