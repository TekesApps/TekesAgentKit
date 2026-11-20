//! Renders Topology B (`DomainStudioApp`) in both initial (`domain-canvas` focus + `agent-run`
//! others) and promoted (`agent-run` focus + `domain-canvas` others) offscreen Metal PNG states.

#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let domain_focus_png = args
        .next()
        .expect("usage: render_topology_b <domain-focus.png> <agent-promoted.png>");
    let agent_promoted_png = args
        .next()
        .expect("usage: render_topology_b <domain-focus.png> <agent-promoted.png>");

    let data_dir =
        std::env::temp_dir().join(format!("tekes-topology-b-render-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);

    let rt = tekes_agent_ui_consumer_check::studio::DomainStudioRuntime::setup(data_dir.clone())?;

    let write_png = |path: &str, rgba: &[u8], w: u32, h: u32| -> anyhow::Result<()> {
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::File::create(path)?;
        let mut encoder = png::Encoder::new(file, w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header()?.write_image_data(rgba)?;
        Ok(())
    };

    let scale: u32 = 2;
    let app_domain = tekes_agent_ui_consumer_check::studio::build_topology_b_app(&rt, false)?;
    let rgba_domain = tekes_agent_ui_consumer_check::studio::render_app_to_rgba8_scaled(
        app_domain, 1440, 900, scale,
    )?;
    write_png(&domain_focus_png, &rgba_domain, 1440 * scale, 900 * scale)?;

    let app_promoted = tekes_agent_ui_consumer_check::studio::build_topology_b_app(&rt, true)?;
    let rgba_promoted = tekes_agent_ui_consumer_check::studio::render_app_to_rgba8_scaled(
        app_promoted,
        1440,
        900,
        scale,
    )?;
    write_png(
        &agent_promoted_png,
        &rgba_promoted,
        1440 * scale,
        900 * scale,
    )?;

    let _ = std::fs::remove_dir_all(&data_dir);
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("Topology B offscreen renderer requires macOS Metal.");
}
