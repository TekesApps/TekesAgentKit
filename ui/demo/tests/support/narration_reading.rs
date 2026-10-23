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

#[test]
fn original_tool_group_reading_point_survives_result_updates_with_composer_preedit() {
    let mut source = origin_tool_run();
    let answer = source
        .transcript
        .items
        .iter_mut()
        .find(|item| item.is_final && item.kind == TranscriptItemKind::Assistant)
        .unwrap();
    answer.text = "用于阅读锚点的完整后续回答。".repeat(300);
    if let TranscriptBlockContent::Text { text } =
        &mut answer.content_blocks.as_mut().unwrap()[0].content
    {
        *text = answer.text.clone();
    }
    let scope = RunScope::main("sep", "s");
    let owners = tool_owners(&source);
    let (_, inner, before, final_segment) = embedded_segments(&source);
    let mut run = RunPort::standard(35000, 35010..35500, scope.clone()).unwrap();
    run.update(
        &source,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope),
        true,
    )
    .unwrap();
    for segment in [&before, &final_segment] {
        run.set_activity_segment_expanded(segment, true).unwrap();
    }
    run.update(
        &source,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope),
        true,
    )
    .unwrap();
    let ids = run.ids();
    let fragments = tool_fragments(run.transcript().unwrap(), &owners);
    assert_eq!(fragments.len(), 2);
    let target = fragments[1].1;
    let inner = run
        .transcript()
        .unwrap()
        .row_id(&TranscriptContentRowKey::Message(inner))
        .unwrap();
    let items = run
        .transcript()
        .unwrap()
        .row_keys()
        .iter()
        .map(|key| run.transcript().unwrap().row_id(key).unwrap())
        .collect();
    let mut host = run.into_host().unwrap();
    let (version, ()) = host
        .set_scroll_reading_document(ids.viewport, "original-tools", 0, items, |_| ())
        .unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    render(&mut host, &mut text);
    // Actual CoreText/SDK layout, independent from the public row-key assertions.
    assert!(
        host.get(fragments[0].1).unwrap().frame().max_y()
            <= host.get(inner).unwrap().frame().min_y()
    );
    assert!(host.get(inner).unwrap().frame().max_y() <= host.get(target).unwrap().frame().min_y());
    host.reveal_scroll_reading_item(
        ids.viewport,
        &version,
        target,
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
    let y = host.get(target).unwrap().frame().min_y();
    let group = host
        .get(target)
        .unwrap()
        .component::<OutputMessageFragment>()
        .unwrap()
        .children()[0]
        .id();
    let group_owner = host.get(group).unwrap().component::<ToolRun>().unwrap() as *const _ as usize;
    assert!(host.focus(Some(ids.composer.editor), true));
    host.event(&ContentEvent::Preedit {
        text: "保留工具阅读".into(),
        selection: 6..6,
        clauses: vec![],
        replacement: None,
    });
    let baseline = host
        .text_surface(ids.composer.editor)
        .unwrap()
        .input
        .external_baseline();
    let mut next = source.clone();
    next.revision += 1;
    next.transcript.tools[1].phase = "done".into();
    next.transcript.tools[1].result =
        Some(serde_json::json!({"content":[{"type":"text","text":"新结果"}]}));
    let plan = host
        .get(ids.root)
        .unwrap()
        .component::<RunPort>()
        .unwrap()
        .prepare_update(
            &next,
            Default::default(),
            ComposerControlGate::OPEN,
            paging(&scope),
            true,
        )
        .unwrap();
    assert!(plan.item_ids().contains(&target));
    let updated = tekes_agent_ui_consumer_check::publish_original_run(
        &mut host, ids.root, &version, 1, &plan,
    )
    .unwrap();
    render(&mut host, &mut text);
    assert_eq!(
        scroll(&host, ids.viewport).reading_result().unwrap().anchor,
        ContentReadingAnchorUpdate::Preserved(target)
    );
    assert!((host.get(target).unwrap().frame().min_y() - y).abs() <= 1.);
    assert!(!scroll(&host, ids.viewport).is_following_latest());
    assert_eq!(
        host.get(group).unwrap().component::<ToolRun>().unwrap() as *const _ as usize,
        group_owner
    );
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

    // Inserting original narration before the calls shifts canonical block
    // indices and content above the anchor. Sole tool identities and their
    // existing controls must survive; the SDK compensates the reading position.
    let original_parts = host
        .get(target)
        .unwrap()
        .component::<OutputMessageFragment>()
        .unwrap()
        .content_ids();
    let mut shifted = next.clone();
    shifted.revision += 1;
    let item_id = match &owners[1] {
        TranscriptMessageIdentity::Item(id) => id,
        _ => unreachable!(),
    };
    let item = shifted
        .transcript
        .items
        .iter_mut()
        .find(|item| &item.id == item_id)
        .unwrap();
    item.text = "新增的原旁白。".repeat(12);
    let blocks = item.content_blocks.as_mut().unwrap();
    for block in blocks.iter_mut() {
        block.index += 1;
    }
    blocks.insert(
        0,
        TranscriptContentBlock {
            index: 0,
            content: TranscriptBlockContent::Text {
                text: item.text.clone(),
            },
        },
    );
    for block in blocks {
        if let TranscriptBlockContent::ToolCall { tool_id } = &block.content
            && let Some(TranscriptSourcePosition::Journal { content_index, .. }) = &mut shifted
                .transcript
                .tools
                .iter_mut()
                .find(|tool| &tool.id == tool_id)
                .unwrap()
                .source_position
        {
            *content_index = Some(block.index);
        }
    }
    let plan = host
        .get(ids.root)
        .unwrap()
        .component::<RunPort>()
        .unwrap()
        .prepare_update(
            &shifted,
            Default::default(),
            ComposerControlGate::OPEN,
            paging(&scope),
            true,
        )
        .unwrap();
    assert!(
        tekes_agent_ui_consumer_check::publish_original_run(
            &mut host, ids.root, &version, 2, &plan
        )
        .is_err()
    );
    tekes_agent_ui_consumer_check::publish_original_run(&mut host, ids.root, &updated, 2, &plan)
        .unwrap();
    render(&mut host, &mut text);
    assert_eq!(
        host.get(target)
            .unwrap()
            .component::<OutputMessageFragment>()
            .unwrap()
            .content_ids(),
        original_parts
    );
    assert_eq!(
        scroll(&host, ids.viewport).reading_result().unwrap().anchor,
        ContentReadingAnchorUpdate::Preserved(target)
    );
    assert!((host.get(target).unwrap().frame().min_y() - y).abs() <= 1.);
    assert_eq!(
        host.get(group).unwrap().component::<ToolRun>().unwrap() as *const _ as usize,
        group_owner
    );
    assert_eq!(host.focused(), Some(ids.composer.editor));
    assert_eq!(
        host.text_surface(ids.composer.editor)
            .unwrap()
            .input
            .external_baseline(),
        baseline
    );
}

#[test]
fn readonly_original_tool_fragments_keep_local_segment_selection_and_publication() {
    let source = origin_tool_run();
    let scope = RunScope::main("sep", "s");
    let owners = tool_owners(&source);
    let (_, _, before, final_segment) = embedded_segments(&source);
    let mut pane =
        ReadOnlyTranscript::standard(36000, 36010..36500, scope.clone(), Default::default())
            .unwrap();
    let input = ReadOnlyTranscriptInput {
        scope: &scope,
        publication: 900,
        connection_epoch: source.connection_epoch,
        connection: &source.connection,
        transcript: &source.transcript,
        frame_current: true,
        source_ready: true,
        supports_pagination: false,
        reading: false,
        read_error: None,
    };
    pane.update(input).unwrap();
    let stale = pane.prepare_update(input).unwrap();
    for segment in [&before, &final_segment] {
        pane.set_activity_segment_expanded(segment, true).unwrap();
    }
    assert!(pane.validate_prepared_update(&stale).is_err());
    let plan = pane.prepare_update(input).unwrap();
    let predicted = plan.item_ids();
    pane.apply_prepared_update(&plan).unwrap();
    let fragments = tool_fragments(pane.content(), &owners);
    assert_eq!(fragments.len(), 2);
    assert!(fragments.iter().all(|(_, id)| predicted.contains(id)));
    assert!(!pane.content().feedback_enabled());
    let ids = pane.ids();
    let mut host = pane.into_host().unwrap();
    let (version, ()) = host
        .set_scroll_reading_document(ids.viewport, "readonly-tools", 0, predicted, |_| ())
        .unwrap();
    let next = ReadOnlyTranscriptInput {
        publication: 901,
        ..input
    };
    let plan = host
        .get(ids.root)
        .unwrap()
        .component::<ReadOnlyTranscript>()
        .unwrap()
        .prepare_update(next)
        .unwrap();
    tekes_agent_ui_consumer_check::publish_original_readonly(
        &mut host, ids.root, &version, 1, &plan,
    )
    .unwrap();
    let pane = host
        .get(ids.root)
        .unwrap()
        .component::<ReadOnlyTranscript>()
        .unwrap();
    assert_eq!(tool_fragments(pane.content(), &owners), fragments);
    assert!(pane.prepare_update(input).is_err());
    assert!(host.focused().is_none());
}
