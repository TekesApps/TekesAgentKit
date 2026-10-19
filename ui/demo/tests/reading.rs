#![cfg(target_os = "macos")]
use std::rc::Rc;
use tekes_agent_components_ui::{AgentText, RunSurface, TRANSCRIPT};
use tekes_ui_controls::{
    ButtonTextAlign, ButtonTextRenderer, TextSurfaceHost, TextSurfaceTextRenderer,
    composition::ContentEvent,
};
use tekes_ui_scene::{Point, Rect, Scene, Size};
use tekes_ui_text::{
    Paragraph,
    coretext::{CoreTextFonts, CoreTextShaper},
    model::ShapeOptions,
    style::TextStyle,
    traits::Shaper,
};
use tekes_ui_tokens::{Palette, Rgba, Theme, proposal};
struct Text(CoreTextShaper, f32);
impl TextSurfaceTextRenderer for Text {
    fn push_visible(&mut self, _: &mut Scene, _: &Paragraph, _: Point, _: Rgba, _: Rect) {}
}
impl TextSurfaceHost for Text {
    fn type_scale(&self) -> f32 {
        self.1
    }
    fn shape_surface(
        &mut self,
        s: &str,
        role: proposal::TextRole,
        width: Option<f32>,
    ) -> Rc<Paragraph> {
        Rc::new(self.0.shape(
            s,
            &TextStyle::system(role.style().size * self.1),
            &ShapeOptions {
                width: width.map(f64::from),
                line_spacing: 0.,
            },
        ))
    }
}
impl ButtonTextRenderer for Text {
    fn measure(&mut self, s: &str, r: proposal::TextRole, w: f32) -> Size {
        let p = self.shape_surface(s, r, Some(w));
        Size::new(p.width as f32, p.height as f32)
    }
    fn draw(
        &mut self,
        _: &mut Scene,
        _: &str,
        _: proposal::TextRole,
        _: Rect,
        _: ButtonTextAlign,
        _: Rgba,
    ) {
    }
    fn text_surface_host(&mut self) -> Option<&mut dyn TextSurfaceHost> {
        Some(self)
    }
}
#[tokio::test]
async fn append_and_reflow_preserve_reading_intent_without_stealing_focus() {
    let (runtime, c) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut ui = RunSurface::new();
    ui.sync(&c.snapshot()).unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    let mut scene = Scene::new();
    let render = |ui: &mut RunSurface, text: &mut Text, scene: &mut Scene, w| {
        let f = Rect::new(0., 0., w, 700.);
        ui.render(scene, text, f, f, Palette::from(Theme::LIGHT))
            .unwrap();
    };
    render(&mut ui, &mut text, &mut scene, 900.);
    ui.follow_latest();
    render(&mut ui, &mut text, &mut scene, 900.);
    let before = ui.transcript().field.surface.scroll.offset.y;
    assert!(before > 0.);
    let position = ui.transcript().field.surface.inner.center();
    ui.event(&ContentEvent::Scroll {
        position,
        delta: Point::new(0., -90.),
        precise: true,
        begin: true,
        end: true,
        momentum: false,
        time: 1.,
    });
    render(&mut ui, &mut text, &mut scene, 900.);
    assert!(
        !ui.transcript()
            .field
            .reading
            .as_ref()
            .unwrap()
            .is_following_latest()
    );
    let offset = ui.transcript().field.surface.scroll.offset.y;
    let old_focus = ui.tree.focused();
    // Incoming tail text is a document input, not a custom scroll correction.
    ui.tree
        .get_mut(TRANSCRIPT)
        .unwrap()
        .component_mut::<AgentText>()
        .unwrap()
        .field
        .surface
        .input
        .text
        .push_str("\n追加消息一\n追加消息二");
    render(&mut ui, &mut text, &mut scene, 900.);
    assert!((ui.transcript().field.surface.scroll.offset.y - offset).abs() < 1.);
    assert_eq!(ui.tree.focused(), old_focus);
    render(&mut ui, &mut text, &mut scene, 600.);
    assert!(
        !ui.transcript()
            .field
            .reading
            .as_ref()
            .unwrap()
            .is_following_latest()
    );
    ui.follow_latest();
    render(&mut ui, &mut text, &mut scene, 600.);
    assert_eq!(
        ui.transcript().field.surface.scroll.offset.y,
        ui.transcript().field.surface.scroll.limit().y
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn actual_earlier_pages_use_stable_message_ranges_and_preserve_composer_focus() {
    use std::time::Duration;
    use tekes_agent_components_ui::COMPOSER;
    use tekes_agent_presentation::RunScope;
    let (runtime, c) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut ui = RunSurface::new();
    ui.sync(&c.snapshot()).unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    let mut scene = Scene::new();
    let render =
        |ui: &mut RunSurface, text: &mut Text, scene: &mut Scene, width: f32, type_scale: f32| {
            text.1 = type_scale;
            let f = Rect::new(0., 0., width, 450.);
            ui.render(scene, text, f, f, Palette::from(Theme::LIGHT))
                .unwrap();
        };
    render(&mut ui, &mut text, &mut scene, 600., 1.);
    {
        let transcript = ui
            .tree
            .get_mut(TRANSCRIPT)
            .unwrap()
            .component_mut::<AgentText>()
            .unwrap();
        transcript
            .field
            .reading
            .as_mut()
            .unwrap()
            .stop_following(&mut transcript.field.surface);
        transcript.field.surface.scroll.scroll_by(0., 80.25);
        transcript.field.surface.input.select_range(6..10);
    }
    render(&mut ui, &mut text, &mut scene, 600., 1.);
    assert!(ui.tree.focus(Some(COMPOSER), true));
    let draft = ui.editor().input.text.clone();
    for page in 0..2 {
        let old = ui.transcript().field.surface.input.text.clone();
        let anchor = ui.transcript().field.surface.reflow_anchor.unwrap();
        let before = ui
            .transcript()
            .field
            .surface
            .caret_doc(anchor.utf16)
            .min_y()
            - ui.transcript().field.surface.scroll.offset.y;
        let selection = ui.transcript().field.surface.input.selection();
        let count = c.snapshot().transcript.items.len();
        assert!(
            c.load_earlier(&RunScope::main("fixture", "a"))
                .await
                .unwrap()
        );
        let mut changes = c.subscribe();
        tokio::time::timeout(Duration::from_secs(3), async {
            while c.snapshot().transcript.items.len() <= count {
                changes.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
        ui.sync(&c.snapshot()).unwrap();
        let next = &ui.transcript().field.surface.input.text;
        let byte = next.find(&old).expect("retained message block");
        let delta = next[..byte].encode_utf16().count();
        assert!(delta > 0);
        let (width, type_scale) = if page == 0 { (600., 1.) } else { (400., 1.5) };
        render(&mut ui, &mut text, &mut scene, width, type_scale);
        let view = &ui.transcript().field.surface;
        let after = view.caret_doc(anchor.utf16 + delta).min_y() - view.scroll.offset.y;
        println!(
            "ACTUAL_EARLIER page={} width={} type_scale={} error_pt={} focus={:?}",
            page + 1,
            width,
            type_scale,
            (after - before).abs(),
            ui.tree.focused()
        );
        assert_eq!(view.reflow_anchor.unwrap().utf16, anchor.utf16 + delta);
        assert!(
            (after - before).abs() <= 1.,
            "actual Earlier drift: {}",
            after - before
        );
        assert_eq!(
            view.input.selection(),
            selection.start + delta..selection.end + delta
        );
        assert_eq!(ui.tree.focused(), Some(COMPOSER));
        assert_eq!(ui.editor().input.text, draft);
        assert!(
            !ui.transcript()
                .field
                .reading
                .as_ref()
                .unwrap()
                .is_following_latest()
        );
    }
    let history = ui.transcript().field.surface.input.text.clone();
    let offset = ui.transcript().field.surface.scroll.offset.y;
    c.select(tekes_agent_presentation::ComposerDestination::run(
        RunScope::main("fixture", "b"),
    ))
    .unwrap();
    ui.sync(&c.snapshot()).unwrap();
    render(&mut ui, &mut text, &mut scene, 400., 1.5);
    c.select(tekes_agent_presentation::ComposerDestination::run(
        RunScope::main("fixture", "a"),
    ))
    .unwrap();
    ui.sync(&c.snapshot()).unwrap();
    render(&mut ui, &mut text, &mut scene, 400., 1.5);
    assert_eq!(ui.transcript().field.surface.input.text, history);
    assert!((ui.transcript().field.surface.scroll.offset.y - offset).abs() <= 1.);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn run_surface_wrapper_preserves_pointer_metadata_and_composition_barrier() {
    use tekes_agent_components_ui::{COMPOSER, SEND};
    use tekes_ui_controls::composition::{
        PointerButton, PointerEvent, PointerModifiers, PointerPhase,
    };
    let (runtime, c) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let destination = c.snapshot().selection.as_ref().unwrap().destination.clone();
    let mut draft = c.snapshot().draft.clone();
    draft.text = "Hello 中文 😀 e\u{301} draft".into();
    c.edit(&destination, draft).unwrap();
    let mut ui = RunSurface::new();
    ui.sync(&c.snapshot()).unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    let f = Rect::new(0., 0., 900., 700.);
    ui.render(
        &mut Scene::new(),
        &mut text,
        f,
        f,
        Palette::from(Theme::LIGHT),
    )
    .unwrap();
    let field = ui.editor();
    let point = |i| {
        let rect = field.caret_doc(i);
        Point::new(
            field.origin().x + rect.min_x() + 0.1,
            field.caret().center().y,
        )
    };
    let a = point(2);
    let b = point(9);
    let event = |phase, position, shift, button| {
        ContentEvent::Pointer(PointerEvent {
            position,
            phase,
            button: Some(button),
            modifiers: PointerModifiers {
                shift,
                ..Default::default()
            },
            click_count: 1,
        })
    };
    for (phase, p, shift) in [
        (PointerPhase::Down, a, false),
        (PointerPhase::Up, a, false),
        (PointerPhase::Down, b, true),
        (PointerPhase::Up, b, true),
    ] {
        ui.event(&event(phase, p, shift, PointerButton::Primary));
    }
    assert_eq!(ui.editor().input.anchor, 2);
    assert!(ui.editor().input.caret > 2);
    assert_eq!(ui.tree.focused(), Some(COMPOSER));
    let selection = ui.editor().input.selection();
    let send = ui.tree.get(SEND).unwrap().frame().center();
    for phase in [PointerPhase::Down, PointerPhase::Up] {
        assert!(
            ui.event(&event(phase, send, false, PointerButton::Secondary))
                .is_empty()
        );
    }
    assert_eq!(ui.editor().input.selection(), selection);
    ui.event(&ContentEvent::Preedit {
        text: "组字".into(),
        selection: 2..2,
        clauses: vec![],
        replacement: None,
    });
    assert!(ui.composing());
    assert!(
        ui.event(&event(
            PointerPhase::Down,
            send,
            false,
            PointerButton::Primary
        ))
        .is_empty()
    );
    assert!(ui.composing());
    assert_eq!(ui.tree.focused(), Some(COMPOSER));
    runtime.stop().await.unwrap();
}
