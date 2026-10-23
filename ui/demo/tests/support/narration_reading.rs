//! CoreText/public reading transactions, not native-window acceptance.
use super::*;
use std::rc::Rc;
use tekes_agent_presentation::RunSnapshot;
use tekes_ui_controls::{
    ButtonTextAlign, ButtonTextRenderer, TextSurfaceHost, TextSurfaceTextRenderer,
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

fn render(host: &mut ContentHost, text: &mut Text) {
    let frame = Rect::new(0., 0., 760., 500.);
    host.render(&mut Scene::new(), text, frame, frame, Theme::LIGHT.into())
        .unwrap();
}
fn scroll(host: &ContentHost, viewport: ComponentId) -> &ScrollContent {
    host.get(viewport)
        .unwrap()
        .component::<ScrollContent>()
        .unwrap()
}
fn longer_carrier(source: &RunSnapshot, second: &str) -> RunSnapshot {
    let mut next = source.clone();
    next.revision += 1;
    let item = next
        .transcript
        .items
        .iter_mut()
        .find(|row| row.id == second)
        .unwrap();
    item.text = "新载体继续显示原帧的完整旁白内容。".repeat(300);
    if let TranscriptBlockContent::Text { text } =
        &mut item.content_blocks.as_mut().unwrap()[1].content
    {
        *text = item.text.clone();
    }
    next
}
#[test]
fn original_reading_transaction_preserves_frame_point_across_carrier_switch_and_restore() {
    let (mut source, first, second) = narration_overlap();
    let original = source
        .transcript
        .items
        .iter_mut()
        .find(|item| item.id == first)
        .unwrap();
    original.text = "完整旁白内容".repeat(200);
    if let TranscriptBlockContent::Text { text } =
        &mut original.content_blocks.as_mut().unwrap()[1].content
    {
        *text = original.text.clone();
    }
    let scope = RunScope::main("sep", "s");
    let view = narration_view(&source, true);
    let ids = view.ids();
    let content = view.transcript().unwrap();
    let first_key =
        TranscriptContentRowKey::Message(TranscriptMessageIdentity::Item(first.clone()));
    let second_key =
        TranscriptContentRowKey::Message(TranscriptMessageIdentity::Item(second.clone()));
    let frame_id = content.row_id(&first_key).unwrap();
    let original_id = content
        .message_id(TranscriptMessageKey::Item(&first))
        .unwrap();
    assert_ne!(
        frame_id, original_id,
        "span and canonical message are independent mounted controls"
    );
    let items = content
        .row_keys()
        .iter()
        .map(|key| content.row_id(key).unwrap())
        .collect();
    let mut host = view.into_host().unwrap();
    let (version, ()) = host
        .set_scroll_reading_document(ids.viewport, "narration-test", 0, items, |_| ())
        .unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    render(&mut host, &mut text);
    host.reveal_scroll_reading_item(
        ids.viewport,
        &version,
        frame_id,
        tekes_ui_controls::ScrollRevealOptions {
            placement: tekes_ui_controls::ScrollRevealPlacement::Start(
                tekes_ui_controls::extended::Axis::Vertical,
            ),
            animated: false,
            ..Default::default()
        },
        &mut text,
    )
    .unwrap();
    let point = host.get(ids.viewport).unwrap().visible().unwrap().center();
    host.event(&ContentEvent::Scroll {
        position: point,
        delta: Point::new(0., 30.),
        precise: true,
        begin: true,
        end: true,
        momentum: false,
        time: 1.,
    });
    render(&mut host, &mut text);
    assert!(!scroll(&host, ids.viewport).is_following_latest());
    assert_eq!(
        scroll(&host, ids.viewport).reading_result().unwrap().anchor,
        ContentReadingAnchorUpdate::Preserved(frame_id)
    );
    let y = host.get(frame_id).unwrap().frame().min_y();
    let original_body = host
        .get(original_id)
        .unwrap()
        .component::<OutputMessage>()
        .unwrap() as *const _ as usize;
    assert!(host.focus(Some(ids.composer.editor), true));
    host.event(&ContentEvent::Preedit {
        text: "阅读组字".into(),
        selection: 4..4,
        clauses: vec![],
        replacement: None,
    });
    let baseline = host
        .text_surface(ids.composer.editor)
        .unwrap()
        .input
        .external_baseline();
    let next = longer_carrier(&source, &second);
    let run = host.get(ids.root).unwrap().component::<RunPort>().unwrap();
    let plan = run
        .prepare_update(
            &next,
            Default::default(),
            ComposerControlGate::OPEN,
            paging(&scope),
            true,
        )
        .unwrap();
    assert!(plan.item_ids().contains(&frame_id));
    let next_version = tekes_agent_ui_consumer_check::publish_original_run(
        &mut host, ids.root, &version, 2, &plan,
    )
    .unwrap();
    render(&mut host, &mut text);
    let run = host.get(ids.root).unwrap().component::<RunPort>().unwrap();
    assert_eq!(
        run.transcript().unwrap().row_id(&second_key),
        Some(frame_id)
    );
    assert!(run.transcript().unwrap().row_id(&first_key).is_none());
    assert_eq!(
        scroll(&host, ids.viewport).reading_result().unwrap().anchor,
        ContentReadingAnchorUpdate::Preserved(frame_id)
    );
    assert!((host.get(frame_id).unwrap().frame().min_y() - y).abs() <= 1.);
    assert!(!scroll(&host, ids.viewport).is_following_latest());
    assert_eq!(host.focused(), Some(ids.composer.editor));
    assert_eq!(
        host.text_surface(ids.composer.editor)
            .unwrap()
            .input
            .external_baseline(),
        baseline
    );
    assert!(run.composer().is_composing());
    let mut restored = source.clone();
    restored.revision = next.revision + 1;
    let restore = run
        .prepare_update(
            &restored,
            Default::default(),
            ComposerControlGate::OPEN,
            paging(&scope),
            true,
        )
        .unwrap();
    assert!(
        tekes_agent_ui_consumer_check::publish_original_run(
            &mut host, ids.root, &version, 3, &restore
        )
        .is_err()
    );
    assert_eq!(
        scroll(&host, ids.viewport).reading_document_version(),
        Some(&next_version)
    );
    tekes_agent_ui_consumer_check::publish_original_run(
        &mut host,
        ids.root,
        &next_version,
        3,
        &restore,
    )
    .unwrap();
    render(&mut host, &mut text);
    assert_eq!(
        host.get(original_id)
            .unwrap()
            .component::<OutputMessage>()
            .unwrap() as *const _ as usize,
        original_body
    );
    assert_eq!(
        scroll(&host, ids.viewport).reading_result().unwrap().anchor,
        ContentReadingAnchorUpdate::Preserved(frame_id)
    );
    assert!((host.get(frame_id).unwrap().frame().min_y() - y).abs() <= 1.);
    assert_eq!(host.focused(), Some(ids.composer.editor));
    assert_eq!(
        host.text_surface(ids.composer.editor)
            .unwrap()
            .input
            .external_baseline(),
        baseline
    );
    assert!(
        host.get(ids.root)
            .unwrap()
            .component::<RunPort>()
            .unwrap()
            .composer()
            .is_composing()
    );
}

#[test]
fn readonly_frame_uses_its_own_publication_and_original_sdk_attachment() {
    let (mut source, first, second) = narration_overlap();
    let original = source
        .transcript
        .items
        .iter_mut()
        .find(|row| row.id == first)
        .unwrap();
    original.text = "只读原帧的完整旁白内容。".repeat(150);
    if let TranscriptBlockContent::Text { text } =
        &mut original.content_blocks.as_mut().unwrap()[1].content
    {
        *text = original.text.clone();
    }
    let scope = RunScope::main("sep", "s");
    fn input<'a>(
        scope: &'a RunScope,
        snapshot: &'a RunSnapshot,
        publication: u64,
    ) -> ReadOnlyTranscriptInput<'a> {
        ReadOnlyTranscriptInput {
            scope,
            publication,
            connection_epoch: snapshot.connection_epoch,
            connection: &snapshot.connection,
            transcript: &snapshot.transcript,
            frame_current: true,
            source_ready: true,
            supports_pagination: false,
            reading: false,
            read_error: None,
        }
    }
    let mut pane =
        ReadOnlyTranscript::standard(30000, 30010..30400, scope.clone(), Default::default())
            .unwrap();
    pane.update(input(&scope, &source, 800)).unwrap();
    let ids = pane.ids();
    let original_key = TranscriptContentRowKey::Message(TranscriptMessageIdentity::Item(first));
    let frame_id = pane.content().row_id(&original_key).unwrap();
    let items = pane
        .content()
        .row_keys()
        .iter()
        .map(|key| pane.content().row_id(key).unwrap())
        .collect();
    let mut host = pane.into_host().unwrap();
    let (version, ()) = host
        .set_scroll_reading_document(ids.viewport, "readonly-frame", 0, items, |_| ())
        .unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    render(&mut host, &mut text);
    host.reveal_scroll_reading_item(
        ids.viewport,
        &version,
        frame_id,
        tekes_ui_controls::ScrollRevealOptions {
            placement: tekes_ui_controls::ScrollRevealPlacement::Start(
                tekes_ui_controls::extended::Axis::Vertical,
            ),
            animated: false,
            ..Default::default()
        },
        &mut text,
    )
    .unwrap();
    let y = host.get(frame_id).unwrap().frame().min_y();
    let next = longer_carrier(&source, &second);
    let plan = host
        .get(ids.root)
        .unwrap()
        .component::<ReadOnlyTranscript>()
        .unwrap()
        .prepare_update(input(&scope, &next, 801))
        .unwrap();
    assert!(plan.item_ids().contains(&frame_id));
    tekes_agent_ui_consumer_check::publish_original_readonly(
        &mut host, ids.root, &version, 1, &plan,
    )
    .unwrap();
    render(&mut host, &mut text);
    let pane = host
        .get(ids.root)
        .unwrap()
        .component::<ReadOnlyTranscript>()
        .unwrap();
    assert_eq!(
        pane.content().row_id(&TranscriptContentRowKey::Message(
            TranscriptMessageIdentity::Item(second)
        )),
        Some(frame_id)
    );
    assert!(pane.prepare_update(input(&scope, &source, 800)).is_err());
    assert_ne!(next.revision, 801);
    assert!(!pane.content().feedback_enabled());
    assert_eq!(
        scroll(&host, ids.viewport).reading_result().unwrap().anchor,
        ContentReadingAnchorUpdate::Preserved(frame_id)
    );
    assert!((host.get(frame_id).unwrap().frame().min_y() - y).abs() <= 1.);
    assert!(!scroll(&host, ids.viewport).is_following_latest());
    assert!(host.focused().is_none());
}
