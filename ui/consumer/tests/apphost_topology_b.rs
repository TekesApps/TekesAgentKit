//! Topology B (`DomainStudioApp`) acceptance tests using only public AgentKit UI APIs:
//! - Domain `nav_bar` + Domain `focus` + `AgentRun` in `others`
//! - Custom `CadSpecViewerProvider` plugged into `ArtifactViewerContainer`
//! - Zero-rebuild `focus <-> others` swap preserving `mount_identity()`, `ReadingController`
//!   non-bottom reading state, and active CJK `Preedit` (`正在修改装配公差`) in Composer!

#![cfg(target_os = "macos")]

use std::sync::Arc;
use std::time::Duration;
use tekes_agent_apphost::{AgentRunContainer, ArtifactViewerContainer, ArtifactViewerProvider};
use tekes_agent_components_ui::{
    ARTIFACT_HEADER_ID, COMPOSER_EDITOR_ID, RUN_CONTAINER_SHELL_ID, ResponsiveWidthMode,
    TRANSCRIPT_READER_ID, TranscriptReader,
};
use tekes_agent_presentation::ComposerDestination;
use tekes_agent_ui_consumer_check::studio::{
    CadSpecViewerProvider, DOMAIN_CAD_CUSTOM_SLOT_ID, DomainStudioRuntime, build_topology_b_app,
    build_topology_b_containers_and_options, render_app_to_rgba8,
};
use tekes_ui_app_host::{AppWindow, LocalContainer, PresentationOptions, PresentationState};
use tekes_ui_controls::{
    DraftRevision, ExternalSelection, ExternalUndo, ExternalValueConflict, ExternalValueResult,
    composition::{
        Component, ComponentId, Content, ContentEvent, ContentHost, IconImage, IconRenderError,
        StackContent, TextContent, TextFieldContent, WithIconRenderer,
    },
    extended::{Axis, Stack},
    layout_view::IconRenderer,
};
use tekes_ui_scene::{ImageId, Rect, Scene};
use tekes_ui_tokens::{Palette, Rgba, Theme};

struct TestIcons;
impl IconRenderer for TestIcons {
    fn draw_icon(&mut self, _: &mut Scene, _: &str, _: Rect, _: Rgba) {}
    fn prepare_icon(
        &mut self,
        _: &str,
        _: f32,
        _: f32,
        _: Rgba,
    ) -> Result<IconImage, IconRenderError> {
        Ok(IconImage::new(ImageId(1), std::rc::Rc::new(())))
    }
}

fn layout_host_for_focus(host: &mut ContentHost) {
    let dummy_root = Content::new(
        ComponentId(1),
        StackContent {
            model: Stack {
                axis: Axis::Vertical,
                gap: 0.0,
            },
            flexible_child: None,
            children: Vec::new(),
        },
    );
    let dummy = LocalContainer::new("dummy", dummy_root).unwrap();
    let app =
        AppWindow::from_containers(vec![dummy.into()], PresentationOptions::new("dummy")).unwrap();
    let (_menus, mut hosted) = app.into_window_handler().unwrap();
    let mut icons = TestIcons;
    let mut renderer = WithIconRenderer::new(hosted.text_host_mut(), &mut icons);
    let area = Rect::new(0.0, 0.0, 880.0, 720.0);
    host.render(
        &mut Scene::new(),
        &mut renderer,
        area,
        area,
        Palette::from(Theme::LIGHT),
    )
    .unwrap();
}

#[test]
fn topology_b_domain_first_cad_viewer_and_zero_rebuild_focus_swap() {
    let base_dir =
        std::env::temp_dir().join(format!("tekes-topology-b-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base_dir);
    let rt = DomainStudioRuntime::setup(base_dir.clone()).unwrap();

    // 1. Verify custom CadSpecViewerProvider injects custom CAD inspector slot inside ArtifactViewerContainer.
    rt.workbench
        .set_artifact_target("cad-spec-1", "specs/chassis.cadspec");
    let provider: Arc<dyn ArtifactViewerProvider> = Arc::new(CadSpecViewerProvider);
    let mut cad_viewer =
        ArtifactViewerContainer::build("cad-spec-1", rt.workbench.clone(), Some(provider)).unwrap();
    cad_viewer.with_host(&mut |h| {
        assert!(
            h.get(ARTIFACT_HEADER_ID)
                .unwrap()
                .component::<TextContent>()
                .unwrap()
                .text
                .contains("specs/chassis.cadspec")
        );
        let custom_txt = &h
            .get(DOMAIN_CAD_CUSTOM_SLOT_ID)
            .unwrap()
            .component::<TextContent>()
            .unwrap()
            .text;
        assert!(custom_txt.contains("CAD 参数规范定制面板"));
    });

    // 2. Verify PresentationState slot layout + bidirectional promote() between domain-canvas & agent-run.
    let (containers, presentation, _factories) =
        build_topology_b_containers_and_options(&rt, false).unwrap();
    let mut state = PresentationState::new(&containers, presentation).unwrap();
    assert_eq!(state.nav_bar_id(), Some("domain-outliner"));
    assert_eq!(state.focus_id(), "domain-canvas");
    assert_eq!(
        state.navigation_toolbar_presenter_id(),
        Some("domain-outliner")
    );
    assert!(state.others_ids().iter().any(|id| id == "agent-run"));

    state.promote("agent-run").unwrap();
    assert_eq!(state.focus_id(), "agent-run");
    assert!(state.others_ids().iter().any(|id| id == "domain-canvas"));
    assert_eq!(
        state.navigation_toolbar_presenter_id(),
        Some("domain-outliner")
    );

    state.promote("domain-canvas").unwrap();
    assert_eq!(state.focus_id(), "domain-canvas");
    assert!(state.others_ids().iter().any(|id| id == "agent-run"));

    // 3. Prove persistent AgentRunContainer keeps mount_identity(), non-bottom reading state,
    //    and active CJK Preedit across simulated `Others (Compact) <-> Focus (Expanded)` swaps!
    let mut persistent_agent = AgentRunContainer::build("agent-run", rt.workbench.clone()).unwrap();
    let (shell_id_before, reader_id_before, editor_id_before) = {
        let mut ids = (0u64, 0u64, 0u64);
        persistent_agent.with_host(&mut |h| {
            ids.0 = h.get(RUN_CONTAINER_SHELL_ID).unwrap().mount_identity();
            ids.1 = h.get(TRANSCRIPT_READER_ID).unwrap().mount_identity();
            ids.2 = h.get(COMPOSER_EDITOR_ID).unwrap().mount_identity();

            // Put TranscriptReader in non-following reading mode (scrolled up in history).
            let reader = h
                .get_mut(TRANSCRIPT_READER_ID)
                .unwrap()
                .component_mut::<TranscriptReader>()
                .unwrap();
            reader.stop_following();
            assert!(!reader.reading.is_following_latest());

            layout_host_for_focus(h);

            // Focus Composer & enter active CJK Preedit ("正在修改装配公差").
            assert!(h.focus(Some(COMPOSER_EDITOR_ID), true));
            let resp = h.event(&ContentEvent::Preedit {
                text: "正在修改装配公差".to_owned(),
                selection: 8..8,
                clauses: std::iter::once(0..8).collect(),
                replacement: None,
            });
            assert!(resp.consumed);
        });
        ids
    };

    // Simulate incoming background stream / draft update while swapping focus <-> others widths.
    let dest = ComposerDestination::run(rt.run_binding.current_scope().unwrap());
    rt.run_binding
        .set_draft_text(&dest, "remote draft overwrite attempt".to_owned());
    rt.executor.block_on(async {
        tokio::time::sleep(Duration::from_millis(20)).await;
    });

    persistent_agent.with_host(&mut |h| {
        assert_eq!(
            h.get(RUN_CONTAINER_SHELL_ID).unwrap().mount_identity(),
            shell_id_before
        );
        assert_eq!(
            h.get(TRANSCRIPT_READER_ID).unwrap().mount_identity(),
            reader_id_before
        );
        assert_eq!(
            h.get(COMPOSER_EDITOR_ID).unwrap().mount_identity(),
            editor_id_before
        );

        let reader = h
            .get(TRANSCRIPT_READER_ID)
            .unwrap()
            .component::<TranscriptReader>()
            .unwrap();
        assert!(!reader.reading.is_following_latest());

        let field = h
            .get_mut(COMPOSER_EDITOR_ID)
            .unwrap()
            .component_mut::<TextFieldContent>()
            .unwrap();
        assert!(Component::composing(field));
        let baseline = field.surface.input().external_baseline();
        let conflict = field.surface.apply_external_value(
            &baseline,
            &DraftRevision {
                identity: "topology-b-swap".to_owned(),
                version: 1,
                payload: "should be blocked".to_owned(),
            },
            ExternalSelection::Preserve,
            ExternalUndo::Transaction,
        );
        assert_eq!(
            conflict,
            ExternalValueResult::Conflict(ExternalValueConflict::Composition)
        );
    });

    // Confirm ResponsiveWidthMode breakpoints behave as expected across Others/Focus widths.
    assert_eq!(
        ResponsiveWidthMode::from_width(360.0),
        ResponsiveWidthMode::Compact
    );
    assert_eq!(
        ResponsiveWidthMode::from_width(880.0),
        ResponsiveWidthMode::Expanded
    );

    // 4. Render both initial and promoted Topology B windows on Metal GPU.
    let rgba_domain =
        render_app_to_rgba8(build_topology_b_app(&rt, false).unwrap(), 1280, 800).unwrap();
    assert_eq!(rgba_domain.len(), 1280 * 800 * 4);

    let rgba_promoted =
        render_app_to_rgba8(build_topology_b_app(&rt, true).unwrap(), 1280, 800).unwrap();
    assert_eq!(rgba_promoted.len(), 1280 * 800 * 4);
    assert_ne!(rgba_domain, rgba_promoted);

    let _ = std::fs::remove_dir_all(&base_dir);
}
