use std::sync::Arc;
use std::time::Duration;
use tekes_agent_apphost::AppHostWakeHub;
use tekes_agent_components_ui::{
    COMPOSER_SEND_BTN_ID, ComposerIntent, ComposerSurface, RUN_CONTAINER_STACK_ID,
    ResponsiveWidthMode, TRANSCRIPT_ROOT_ID, TranscriptSurface,
};
use tekes_agent_presentation::{ComposerDestination, RunController, RunScope};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{
    DEFAULT_MAIN_SESSION_ID, FixtureSessionEndpoint, RunWorkbenchBinding, UiWakeNotifier,
};
use tekes_ui_controls::composition::{
    Content, ContentEvent, ContentHost, PointerButton, PointerEvent, PointerModifiers,
    PointerPhase, StackContent,
};
use tekes_ui_scene::{Point, Rect, Scene};
use tekes_ui_text::{
    coretext::{CoreTextFonts, CoreTextShaper},
    traits::Shaper,
};
use tekes_ui_tokens::{Palette, Spacing, Theme, proposal};

struct TestTextBackend(CoreTextShaper);

impl TestTextBackend {
    fn new() -> Self {
        Self(CoreTextShaper::new(CoreTextFonts::new()))
    }
}

impl tekes_ui_controls::TextSurfaceTextRenderer for TestTextBackend {
    fn push_visible(
        &mut self,
        _: &mut Scene,
        _: &tekes_ui_text::Paragraph,
        _: Point,
        _: tekes_ui_tokens::Rgba,
        _: Rect,
    ) {
    }
}

impl tekes_ui_controls::TextSurfaceHost for TestTextBackend {
    fn shape_surface(
        &mut self,
        text: &str,
        role: proposal::TextRole,
        width: Option<f32>,
    ) -> std::rc::Rc<tekes_ui_text::Paragraph> {
        std::rc::Rc::new(self.0.shape(
            text,
            &tekes_ui_text::TextStyle::system(role.style().size),
            &tekes_ui_text::ShapeOptions {
                width: width.map(f64::from),
                line_spacing: 0.0,
            },
        ))
    }
}

impl tekes_ui_controls::ButtonTextRenderer for TestTextBackend {
    fn measure(
        &mut self,
        text: &str,
        role: proposal::TextRole,
        width: f32,
    ) -> tekes_ui_scene::Size {
        let p = tekes_ui_controls::TextSurfaceHost::shape_surface(self, text, role, Some(width));
        tekes_ui_scene::Size::new(p.width as f32, p.height as f32)
    }

    fn draw(
        &mut self,
        _: &mut Scene,
        _: &str,
        _: proposal::TextRole,
        _: Rect,
        _: tekes_ui_controls::ButtonTextAlign,
        _: tekes_ui_tokens::Rgba,
    ) {
    }

    fn text_surface_host(&mut self) -> Option<&mut dyn tekes_ui_controls::TextSurfaceHost> {
        Some(self)
    }

    fn icon_renderer(&mut self) -> Option<&mut dyn tekes_ui_controls::layout_view::IconRenderer> {
        Some(self)
    }
}

impl tekes_ui_controls::layout_view::IconRenderer for TestTextBackend {
    fn draw_icon(&mut self, _: &mut Scene, _: &str, _: Rect, _: tekes_ui_tokens::Rgba) {}

    fn prepare_icon(
        &mut self,
        _: &str,
        _: f32,
        _: f32,
        _: tekes_ui_tokens::Rgba,
    ) -> Result<
        tekes_ui_controls::composition::IconImage,
        tekes_ui_controls::composition::IconRenderError,
    > {
        Ok(tekes_ui_controls::composition::IconImage::new(
            tekes_ui_scene::ImageId(1),
            std::rc::Rc::new(()),
        ))
    }
}

#[test]
fn blocker1_pointer_stop_after_rendered_turn_never_clears_geometry_and_cancels_at_epoch() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let endpoint = FixtureSessionEndpoint::with_timing(Duration::ZERO, Duration::from_millis(250));
    let runtime = rt.block_on(async {
        let runtime = Arc::new(SessionRuntime::spawn("fixture", Arc::new(endpoint.clone())));
        runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
        runtime
    });
    let controller =
        rt.block_on(async { Arc::new(RunController::new(Arc::clone(&runtime)).unwrap()) });
    let wake: Arc<dyn UiWakeNotifier> = AppHostWakeHub::new();
    let binding = RunWorkbenchBinding::new(
        "fixture",
        Arc::clone(&runtime),
        controller,
        rt.handle().clone(),
        wake,
    );
    binding
        .open_initial_session(DEFAULT_MAIN_SESSION_ID)
        .unwrap();

    // Mount TranscriptSurface + ComposerSurface in one ContentHost matching AgentRunContainer.
    let root = Content::new(
        RUN_CONTAINER_STACK_ID,
        StackContent::vertical(
            Spacing::Regular.value(),
            vec![
                TranscriptSurface::build_content(),
                ComposerSurface::build_content(),
            ],
        )
        .with_flexible_child(TRANSCRIPT_ROOT_ID),
    );
    let mut host = ContentHost::new(root).unwrap();
    host.set_action_events(true);
    let mut transcript = TranscriptSurface::new();
    let mut composer = ComposerSurface::new();

    let mut text = TestTextBackend::new();
    let mut scene = Scene::new();
    let palette = Palette::from(Theme::LIGHT);
    let area = Rect::new(0.0, 0.0, 960.0, 680.0);

    // Send a turn so an active running stream starts with rendered assistant RichContent present.
    let dest = ComposerDestination::run(RunScope::main("fixture", DEFAULT_MAIN_SESSION_ID));
    binding.set_draft_text(&dest, "trigger running turn".into());
    binding.submit_destination(&dest);

    for _ in 0..80 {
        let snap = binding.run_snapshot();
        if snap.run.as_ref().is_some_and(|r| r.can_cancel)
            && snap.transcript.items.iter().any(|i| i.is_provisional)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    let snap = binding.run_snapshot();
    assert!(snap.run.as_ref().is_some_and(|r| r.can_cancel));

    transcript.sync(&mut host, &snap, &[], ResponsiveWidthMode::Expanded);
    composer.sync(&mut host, &snap, ResponsiveWidthMode::Expanded);
    host.render(&mut scene, &mut text, area, area, palette)
        .unwrap();

    let stop_rect = host
        .focus_targets()
        .into_iter()
        .find(|(id, _)| *id == COMPOSER_SEND_BTN_ID)
        .map(|(_, rect)| rect)
        .expect("Stop button focus target bounds must be present");
    let center = Point::new(
        stop_rect.origin.x + stop_rect.size.width * 0.5,
        stop_rect.origin.y + stop_rect.size.height * 0.5,
    );

    // Simulate LocalContainer event -> on_action sequence WITHOUT any intermediate render.
    let mut cancel_intent = None;
    for phase in [PointerPhase::Move, PointerPhase::Down, PointerPhase::Up] {
        let button = if matches!(phase, PointerPhase::Move) {
            None
        } else {
            Some(PointerButton::Primary)
        };
        let resp = host
            .event_with_text(
                &ContentEvent::Pointer(PointerEvent {
                    phase,
                    position: center,
                    button,
                    modifiers: PointerModifiers::default(),
                    click_count: u32::from(!matches!(phase, PointerPhase::Move)),
                }),
                &mut text,
            )
            .unwrap();
        let records = host.drain_action_records();
        let _ = transcript.handle_actions(&mut host, &snap, &records);
        for intent in composer.handle_actions(&host, &snap, &records) {
            if let ComposerIntent::Cancel {
                scope,
                connection_epoch,
            } = intent
            {
                cancel_intent = Some((scope, connection_epoch));
            }
        }
        if matches!(phase, PointerPhase::Down | PointerPhase::Up) {
            assert!(
                resp.consumed || resp.activated,
                "pointer phase {phase:?} must NOT be dropped by geometry invalidation"
            );
        }
        assert!(
            host.geometry_snapshot().is_some(),
            "handle_actions must NOT invalidate ContentHost geometry during pointer sequence"
        );
    }

    let (scope, epoch) = cancel_intent.expect("PointerUp on Stop must emit ComposerIntent::Cancel");
    binding.cancel_scope_at_epoch(epoch, &scope);

    for _ in 0..80 {
        if !binding
            .run_snapshot()
            .run
            .as_ref()
            .is_some_and(|r| r.can_cancel)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    assert!(
        !binding
            .run_snapshot()
            .run
            .as_ref()
            .is_some_and(|r| r.can_cancel),
        "Cancel must end the running state cleanly"
    );
    rt.block_on(runtime.stop()).unwrap();
}
