//! Topology A (`tekes-agent-ui-demo`) integration acceptance tests:
//! verifies 3-column AppHost assembly, Tekes-aligned `Others` tabs, focus swap, and
//! light/dark Metal GPU 2x Retina offscreen PNG output.

#![cfg(target_os = "macos")]

use tekes_agent_ui_demo::workbench_app::{
    TopologyALaunchOptions, build_topology_a_app, build_topology_a_containers_and_options,
    initialize_topology_a_runtime, render_topology_a_offscreen_png,
};
use tekes_ui_app_host::{OthersTab, PresentationState};
use tekes_ui_tokens::Theme;

#[test]
fn topology_a_assembles_nav_focus_others_list_swap_and_gpu_pngs() {
    let base_dir =
        std::env::temp_dir().join(format!("tekes-topology-a-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base_dir);
    std::fs::create_dir_all(&base_dir).unwrap();

    // 1. Default Topology A (Light): nav_bar=agent-sidebar, focus=agent-run, others=List(Summary+TasksQueue).
    let opts_light = TopologyALaunchOptions {
        theme: Theme::LIGHT,
        swap_focus: false,
        data_dir: Some(base_dir.join("light")),
        ..TopologyALaunchOptions::default()
    };
    let rt_light = initialize_topology_a_runtime(&opts_light).unwrap();
    let (containers, pres, _factories) =
        build_topology_a_containers_and_options(&rt_light, false).unwrap();
    let mut state = PresentationState::new(&containers, pres).unwrap();
    assert_eq!(state.nav_bar_id(), Some("agent-sidebar"));
    assert_eq!(state.focus_id(), "agent-run");
    assert_eq!(
        state.navigation_toolbar_presenter_id(),
        Some("agent-sidebar")
    );
    assert!(
        state
            .tabs()
            .contains(&OthersTab::Container("agent-summary".to_owned()))
    );
    assert!(
        state
            .tabs()
            .contains(&OthersTab::Container("workspace-files".to_owned()))
    );
    assert!(
        state
            .tabs()
            .contains(&OthersTab::Container("workspace-changes".to_owned()))
    );
    state.promote("workspace-files").unwrap();
    assert_eq!(state.focus_id(), "workspace-files");
    state.promote("agent-run").unwrap();
    assert_eq!(state.focus_id(), "agent-run");

    let app_light = build_topology_a_app(&rt_light, &opts_light).unwrap();
    let light_png = base_dir.join("topo-a-light.png");
    render_topology_a_offscreen_png(app_light, &light_png, 1280, 800).unwrap();
    let bytes = std::fs::read(&light_png).unwrap();
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));

    // 2. Swapped Focus + Dark theme Topology A.
    let opts_dark_swap = TopologyALaunchOptions {
        theme: Theme::DARK,
        swap_focus: true,
        data_dir: Some(base_dir.join("dark-swap")),
        ..TopologyALaunchOptions::default()
    };
    let rt_dark = initialize_topology_a_runtime(&opts_dark_swap).unwrap();
    let (containers2, pres2, _) = build_topology_a_containers_and_options(&rt_dark, true).unwrap();
    let state2 = PresentationState::new(&containers2, pres2).unwrap();
    assert_eq!(state2.focus_id(), "workspace-files");
    let app_dark = build_topology_a_app(&rt_dark, &opts_dark_swap).unwrap();
    let dark_png = base_dir.join("topo-a-dark.png");
    render_topology_a_offscreen_png(app_dark, &dark_png, 1280, 800).unwrap();
    assert!(
        std::fs::read(&dark_png)
            .unwrap()
            .starts_with(b"\x89PNG\r\n\x1a\n")
    );

    let _ = std::fs::remove_dir_all(&base_dir);
}
