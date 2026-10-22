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
    item.content_blocks = None;
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
        let copy = host
            .get(
                host.get(ids.root)
                    .unwrap()
                    .component::<InputMessage>()
                    .unwrap()
                    .copy_button_id()
                    .unwrap(),
            )
            .unwrap()
            .frame();
        assert!((copy.max_x() - frame.max_x()).abs() < 0.01);
        assert!(copy.min_y() >= note.max_y());
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
    unsupported.content_blocks = Some(vec![
        tekes_agent_presentation::TranscriptContentBlock {
            index: 0,
            content: tekes_agent_presentation::TranscriptBlockContent::Text { text: "a".into() },
        },
        tekes_agent_presentation::TranscriptContentBlock {
            index: 0,
            content: tekes_agent_presentation::TranscriptBlockContent::Text { text: "b".into() },
        },
    ]);
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

#[tokio::test]
async fn pure_attachment_keeps_occurrences_trailing_details_and_external_footnote_without_empty_bubble()
 {
    use tekes_agent_presentation::{TranscriptBlockContent, TranscriptContentBlock};
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let snapshot = controller.snapshot();
    let doc = TranscriptDocument::read(&snapshot.transcript).unwrap();
    let mut item = doc
        .sections
        .iter()
        .flat_map(|s| &s.inputs)
        .last()
        .unwrap()
        .item
        .clone();
    item.text.clear();
    let file = serde_json::json!({"type":"file","name":"中文.txt","id":"same-file"});
    item.attachments = vec![file.clone(), file.clone()];
    item.content_blocks = Some(vec![
        TranscriptContentBlock {
            index: 0,
            content: TranscriptBlockContent::Attachment {
                content: file.clone(),
            },
        },
        TranscriptContentBlock {
            index: 1,
            content: TranscriptBlockContent::Attachment {
                content: file.clone(),
            },
        },
    ]);
    let input = TranscriptInput {
        key: TranscriptMessageKey::NativeEntry("attachment-entry"),
        item: &item,
        footnote: TranscriptDelivery::Recorded { time: None },
    };
    let scope = RunScope::main("fixture", "a");
    let mut message = InputMessage::new(603, scope.clone());
    message.update(&scope, 1, 1, &input).unwrap();
    let ids = message.ids();
    let details = message.detail_ids();
    assert_eq!(
        details.len(),
        2,
        "aggregate projection must not double the canonical occurrences"
    );
    let mut host = message.into_host().unwrap();
    assert!(host.get(ids.body).is_none());
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    let frame = Rect::new(20., 30., 520., 800.);
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    assert_eq!(host.get(ids.bubble).unwrap().frame().size.height, 0.);
    host.accessibility_action(
        details[0].disclosure.unwrap(),
        None,
        tekes_ui_controls::composition::AccessibilityAction::Press,
    );
    let actions = host.drain_actions();
    host.get_mut(ids.root)
        .unwrap()
        .component_mut::<InputMessage>()
        .unwrap()
        .handle_actions(&actions);
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::DARK.into(),
    )
    .unwrap();
    for id in [
        details[0].root,
        details[0].disclosure.unwrap(),
        details[0].body,
        details[1].root,
        ids.footnote,
    ] {
        assert!((host.get(id).unwrap().frame().max_x() - frame.max_x()).abs() < 0.01);
    }
    assert!(
        host.text_surface(details[0].body)
            .unwrap()
            .input
            .text
            .contains("中文.txt")
    );
    assert!(host.text_surface(details[0].body).unwrap().input.read_only);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn attachments_precede_the_user_bubble_and_external_caption_in_app_order() {
    use tekes_ui_controls::composition::Component;
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut snapshot = controller.snapshot().transcript.clone();
    let scope = RunScope::main("fixture", "a");
    snapshot.items[0]
        .attachments
        .push(serde_json::json!({"name":"输入附件.txt"}));
    let document = TranscriptDocument::read(&snapshot).unwrap();
    let mut message = InputMessage::new(998, scope.clone());
    message
        .update(&scope, 1, 1, &document.sections[0].inputs[0])
        .unwrap();
    let ids = message.ids();
    let order: Vec<_> = message.children().iter().map(|node| node.id()).collect();
    assert_eq!(
        order,
        vec![
            message.detail_ids()[0].root,
            ids.bubble,
            ids.footnote,
            message.copy_button_id().unwrap()
        ]
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn input_copy_captures_exact_original_text_and_retires_old_targets() {
    use tekes_ui_controls::composition::*;
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let scope = RunScope::main("fixture", "a");
    let source = controller.snapshot();
    let document = TranscriptDocument::read(&source.transcript).unwrap();
    let mut item = document.sections[0].inputs[0].item.clone();
    item.content_blocks = None;
    item.text = "中文 **源码**\n  保留空白 😀 ".into();
    fn input(item: &tekes_agent_presentation::TranscriptItem) -> TranscriptInput<'_> {
        TranscriptInput {
            key: TranscriptMessageKey::NativeEntry("original"),
            item,
            footnote: TranscriptDelivery::Recorded { time: None },
        }
    }
    let mut row = InputMessage::new(991, scope.clone());
    row.update(&scope, 1, 1, &input(&item)).unwrap();
    let old = row.copy_button_id().unwrap();
    let action = |target| ContentAction {
        target,
        kind: ContentActionKind::Activate,
        source: ContentActionSource::Programmatic,
    };
    let intents = row.take_copy_intents(&[action(old), action(old)]);
    assert_eq!(intents.len(), 1);
    let captured = intents[0].clone();
    assert_eq!(captured.scope(), &scope);
    assert_eq!(
        captured.message(),
        Some(&tekes_agent_components_ui::TranscriptMessageIdentity::NativeEntry("original".into()))
    );
    row.update(&scope, 2, 1, &input(&item)).unwrap();
    assert_eq!(row.copy_button_id(), Some(old));
    let original_text = item.text.clone();
    item.text = "新的服务端内容".into();
    row.update(&scope, 3, 1, &input(&item)).unwrap();
    assert_ne!(row.copy_button_id(), Some(old));
    assert!(row.take_copy_intents(&[action(old)]).is_empty());
    assert_eq!(captured.text(), original_text);
    assert!(captured.claim());
    assert!(
        !intents[0].claim(),
        "clones cannot replay a clipboard write"
    );
    item.text.clear();
    item.attachments
        .push(serde_json::json!({"name":"file.txt"}));
    row.update(&scope, 4, 2, &input(&item)).unwrap();
    assert!(row.copy_button_id().is_none());
    runtime.stop().await.unwrap();
}
