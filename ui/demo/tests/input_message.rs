#![cfg(target_os = "macos")]
use std::rc::Rc;
use tekes_agent_components_ui::InputMessage;
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
use tekes_ui_tokens::{Rgba, Theme, proposal};
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

use tekes_agent_presentation::{
    RunScope, TranscriptDelivery, TranscriptDocument, TranscriptInput, TranscriptMessageKey,
};
use tekes_ui_controls::{
    TextSurfacePresentation,
    composition::{Component, TextContent},
};

#[tokio::test]
async fn user_row_uses_library_content_surface_and_external_trailing_caption() {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let snapshot = controller.snapshot();
    let scope = RunScope::main("fixture", "a");
    let doc = TranscriptDocument::read(&snapshot.transcript).unwrap();
    let mut item = doc
        .sections
        .iter()
        .flat_map(|s| &s.inputs)
        .last()
        .unwrap()
        .item
        .clone();
    item.text = "中文 😀".into();
    let input = TranscriptInput {
        key: TranscriptMessageKey::NativeEntry("entry"),
        item: &item,
        footnote: TranscriptDelivery::Recorded { time: None },
    };
    let mut row = InputMessage::new(601, scope.clone());
    row.update(&scope, 1, 1, &input).unwrap();
    let ids = row.ids();
    let mut host = row.into_host().unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    for theme in [Theme::LIGHT, Theme::DARK] {
        let frame = Rect::new(20., 40., 600., 500.);
        host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
            .unwrap();
        let bubble = host.get(ids.bubble).unwrap().frame();
        let note = host.get(ids.footnote).unwrap().frame();
        assert!(
            bubble.size.width < frame.size.width / 2.,
            "short input must shrink to prose"
        );
        assert!((bubble.max_x() - frame.max_x()).abs() < 0.01);
        assert!((note.max_x() - frame.max_x()).abs() < 0.01);
        assert!(note.min_y() > bubble.max_y());
        assert_eq!(
            host.text_surface(ids.body).unwrap().presentation,
            TextSurfacePresentation::Content
        );
        assert!(host.text_surface(ids.body).unwrap().input.read_only);
    }
    let mut long = item.clone();
    long.text = "中文消息按可用宽度自然换行。".repeat(15);
    host.get_mut(ids.root)
        .unwrap()
        .component_mut::<InputMessage>()
        .unwrap()
        .update(
            &scope,
            2,
            1,
            &TranscriptInput {
                item: &long,
                ..input
            },
        )
        .unwrap();
    let mut heights = vec![];
    for width in [600., 180.] {
        let frame = Rect::new(20., 40., width, 1500.);
        host.render(
            &mut Scene::new(),
            &mut text,
            frame,
            frame,
            Theme::DARK.into(),
        )
        .unwrap();
        let bubble = host.get(ids.bubble).unwrap().frame();
        let note = host.get(ids.footnote).unwrap().frame();
        assert!(bubble.size.width <= width);
        assert!((bubble.max_x() - frame.max_x()).abs() < 0.01);
        assert!((note.max_x() - frame.max_x()).abs() < 0.01);
        assert!(note.min_y() > bubble.max_y());
        heights.push(bubble.size.height);
    }
    assert!(heights[1] > heights[0]);
    host.focus(Some(ids.body), true);
    host.event(&ContentEvent::Commit {
        text: "不能修改历史".into(),
        replacement: None,
    });
    assert_eq!(host.text_surface(ids.body).unwrap().input.text, long.text);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn input_row_fences_scope_publication_and_explicit_identity_migration() {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let snapshot = controller.snapshot();
    let scope = RunScope::main("fixture", "a");
    let doc = TranscriptDocument::read(&snapshot.transcript).unwrap();
    let mut item = doc
        .sections
        .iter()
        .flat_map(|s| &s.inputs)
        .last()
        .unwrap()
        .item
        .clone();
    let mut row = InputMessage::new(602, scope.clone());
    let pending = TranscriptInput {
        key: TranscriptMessageKey::Submission("receipt"),
        item: &item,
        footnote: TranscriptDelivery::Pending,
    };
    row.update(&scope, 2, 1, &pending).unwrap();
    assert!(
        row.update(&RunScope::main("other", "b"), 3, 1, &pending)
            .is_err()
    );
    assert!(row.update(&scope, 1, 1, &pending).is_err());
    let recorded = TranscriptInput {
        key: TranscriptMessageKey::NativeEntry("entry"),
        item: &item,
        footnote: TranscriptDelivery::Recorded { time: Some(123.) },
    };
    assert!(
        row.update(&scope, 3, 1, &recorded).is_err(),
        "same text does not establish identity"
    );
    item.reconciliation_id = Some("receipt".into());
    let recorded = TranscriptInput {
        key: TranscriptMessageKey::NativeEntry("entry"),
        item: &item,
        footnote: TranscriptDelivery::Recorded { time: Some(123.) },
    };
    row.update(&scope, 3, 1, &recorded).unwrap();
    assert!(row.update(&scope, 3, 2, &recorded).is_err());
    let before = row.children()[1]
        .component::<TextContent>()
        .unwrap()
        .text
        .clone();
    let mut unsupported = item.clone();
    unsupported.reasoning = "unsupported".into();
    assert!(
        row.update(
            &scope,
            4,
            1,
            &TranscriptInput {
                item: &unsupported,
                ..recorded
            }
        )
        .is_err()
    );
    assert_eq!(
        row.children()[1].component::<TextContent>().unwrap().text,
        before
    );
    runtime.stop().await.unwrap();
}
