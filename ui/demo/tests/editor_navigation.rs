#![cfg(target_os = "macos")]
use std::rc::Rc;
use tekes_agent_components_ui::RunSurface;
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
    fn shape_label(
        &mut self,
        text: &str,
        role: proposal::TextRole,
        width: f32,
        options: tekes_ui_text::LabelOptions,
    ) -> Option<Rc<tekes_ui_text::LabelLayout>> {
        Some(tekes_agent_ui_demo::label_host::shape_label(
            &mut self.0,
            text,
            role,
            width,
            self.1,
            options,
        ))
    }

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
async fn real_composer_uses_current_paragraph_without_intermediate_render_and_keeps_column_through_ack()
 {
    use tekes_agent_components_ui::{COMPOSER, Intent};
    use tekes_ui_controls::composition::AccessibilityAction as A;
    let (runtime, c) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut ui = RunSurface::new();
    ui.sync(&c.snapshot()).unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    let frame = Rect::new(40., 50., 900., 700.);
    ui.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Palette::from(Theme::LIGHT),
    )
    .unwrap();
    ui.tree.focus(Some(COMPOSER), true);
    let actions = ui
        .event_with_text(
            &ContentEvent::Commit {
                text: "abcdef\nx\nabcdef".into(),
                replacement: None,
            },
            &mut text,
        )
        .unwrap();
    for intent in actions {
        if let Intent::Edit { destination, text } = intent {
            let mut d = c.snapshot().draft.clone();
            d.text = text;
            c.edit(&destination, d).unwrap();
        }
    }
    ui.sync(&c.snapshot()).unwrap();
    ui.accessibility_action(COMPOSER, None, A::SetSelection(5..5));
    assert!(
        ui.event_with_text(&ContentEvent::TextCommand("moveDown:".into()), &mut text)
            .unwrap()
            .is_empty()
    );
    assert_eq!(ui.editor().input.caret, 8);
    let destination = c.snapshot().selection.as_ref().unwrap().destination.clone();
    c.edit(&destination, c.snapshot().draft.clone()).unwrap();
    ui.sync(&c.snapshot()).unwrap();
    assert!(
        ui.event_with_text(&ContentEvent::TextCommand("moveDown:".into()), &mut text)
            .unwrap()
            .is_empty()
    );
    assert_eq!(ui.editor().input.caret, 14);
    assert_eq!(ui.editor().shaped, ui.editor().input.text);
    assert_eq!(ui.tree.focused(), Some(COMPOSER));
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn actual_input_backend_preserves_ime_and_does_not_turn_navigation_into_business_actions() {
    use tekes_agent_components_ui::{COMPOSER, Intent};
    use tekes_ui_controls::{
        ControlKey,
        composition::{ContentKeyEvent, InputModifiers},
    };
    let (runtime, c) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut ui = RunSurface::new();
    ui.sync(&c.snapshot()).unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.5);
    let frame = Rect::new(40., 50., 650., 600.);
    ui.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Palette::from(Theme::LIGHT),
    )
    .unwrap();
    ui.tree.focus(Some(COMPOSER), true);
    ui.event_with_text(
        &ContentEvent::Commit {
            text: "first\n中文 😀 e\u{301}".into(),
            replacement: None,
        },
        &mut text,
    )
    .unwrap();
    let key = ContentKeyEvent {
        key: Some(ControlKey::Left),
        down: true,
        repeat: false,
        modifiers: InputModifiers {
            command: true,
            shift: true,
            ..Default::default()
        },
        native_code: 123,
        characters: String::new(),
        characters_ignoring_modifiers: String::new(),
    };
    let actions = ui
        .event_with_text(&ContentEvent::KeyInput(key), &mut text)
        .unwrap();
    assert!(!actions.iter().any(|i| matches!(i, Intent::Send { .. })));
    assert_eq!(ui.editor().input.caret, 6);
    ui.event_with_text(
        &ContentEvent::Preedit {
            text: "拼".into(),
            selection: 1..1,
            clauses: vec![],
            replacement: None,
        },
        &mut text,
    )
    .unwrap();
    let before = format!("{:?}", ui.editor().input);
    assert!(
        ui.event_with_text(&ContentEvent::TextCommand("moveUp:".into()), &mut text)
            .unwrap()
            .is_empty()
    );
    assert_eq!(format!("{:?}", ui.editor().input), before);
    assert!(ui.composing());
    runtime.stop().await.unwrap();
}
