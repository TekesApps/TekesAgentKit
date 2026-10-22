use super::*;
const LONG_NOTE: &str = "完整备注 👩🏽‍💻 e\u{301} 这是很长的已提交中文备注，不能改写原始内容。这是很长的已提交中文备注，不能改写原始内容。这是很长的已提交中文备注，不能改写原始内容。这是很长的已提交中文备注，不能改写原始内容。";
#[test]
fn output_aside_is_one_leading_reading_band_with_original_nested_feedback() {
    use tekes_agent_ui_bindings::{SessionMessageFeedback, SessionMessageFeedbackRating};
    let scope = RunScope::main("consumer", "feedback-aside");
    let mut answer = TranscriptItem {
        id: "answer".into(),
        record_id: Some("public-answer".into()),
        reconciliation_id: None,
        turn_id: "turn".into(),
        kind: TranscriptItemKind::Assistant,
        text: String::new(),
        reasoning: String::new(),
        attachments: vec![],
        content_blocks: None,
        is_visible: true,
        is_final: true,
        is_provisional: false,
        reasoning_ended: true,
        source_sequence: Some(2),
        source_position: Some(TranscriptSourcePosition::Journal {
            sequence: 2,
            content_index: None,
        }),
        time: Some(13000.),
        metadata: Default::default(),
    };
    answer.metadata["feedbackMessageId"] = "public-answer".into();
    let mut input = answer.clone();
    input.id = "input".into();
    input.record_id = None;
    input.metadata = Default::default();
    input.kind = TranscriptItemKind::User;
    input.text = "原输入".into();
    input.source_sequence = Some(1);
    input.source_position = Some(TranscriptSourcePosition::Journal {
        sequence: 1,
        content_index: None,
    });
    let mut snapshot = TranscriptSnapshot {
        items: vec![input, answer],
        turns: vec![TranscriptTurn {
            id: "turn".into(),
            status: "completed".into(),
            input: "原输入".into(),
            started_at: Some(10000.),
            ended_at: Some(13000.),
            usage: Default::default(),
            fork_boundary: None,
            footnote_actions: vec![],
            live_usage: None,
            reported_usage: None,
        }],
        ..Default::default()
    };

    snapshot.items[1].text = "正式回答".into();
    for theme in Theme::ALL {
        for scale in [1., 1.4, 2.] {
            for width in [320., 770., 1100.] {
                for note in ["备注", LONG_NOTE] {
                    let mut content = TranscriptContent::new(
                        63000,
                        63001..63030,
                        scope.clone(),
                        TranscriptContentLabels {
                            feedback: Some(Default::default()),
                            footnote: TurnFootnoteLabels {
                                information: |_| "完成时间".into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                    )
                    .unwrap();
                    content.update(&scope, 1, 1, &snapshot, true).unwrap();
                    let root = content.id();
                    let footer = content
                        .row_id(&TranscriptContentRowKey::TurnFootnote("turn".into()))
                        .unwrap();
                    let (feedback, control) = content.feedback_controls().next().unwrap();
                    let target = control.target().unwrap().clone();
                    content
                        .feedback_mut(feedback)
                        .unwrap()
                        .update(FeedbackControlState {
                            target,
                            publication: 1,
                            connection_epoch: 1,
                            current: Some(SessionMessageFeedback {
                                message_id: "public-answer".into(),
                                rating: SessionMessageFeedbackRating::Positive,
                                note: Some(note.into()),
                                version: "v1".into(),
                                updated_at_milliseconds: None,
                            }),
                            put_available: true,
                            delete_available: true,
                        })
                        .unwrap();
                    assert!(
                        !content
                            .row_keys()
                            .iter()
                            .any(|key| matches!(key, TranscriptContentRowKey::TurnFeedback(_)))
                    );
                    let composer = tekes_agent_ui_consumer_check::new_composer(63999);
                    let editor = composer.ids().editor;
                    let mut host = ContentHost::new(Content::new(
                        ComponentId(12),
                        StackContent {
                            model: tekes_ui_controls::extended::Stack {
                                axis: tekes_ui_controls::extended::Axis::Vertical,
                                gap: 0.,
                            },
                            flexible_child: None,
                            children: vec![
                                Content::new(root, content),
                                Content::new(composer.ids().root, composer),
                            ],
                        },
                    ))
                    .unwrap();
                    host.set_action_events(true);
                    let frame = Rect::new(20., 30., width, 1800.);
                    let mut text = Text::new(scale, true);
                    host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                        .unwrap();
                    let original = host
                        .get(feedback)
                        .unwrap()
                        .component::<TurnFeedback>()
                        .unwrap() as *const _ as usize;
                    let ids = host
                        .get(feedback)
                        .unwrap()
                        .component::<TurnFeedback>()
                        .unwrap()
                        .ids();
                    assert!(
                        host.get(ids.edit_note).unwrap().frame().size.width <= width * 0.4 + 0.02
                    );
                    assert_eq!(
                        host.get(ids.edit_note)
                            .unwrap()
                            .component::<ButtonContent>()
                            .unwrap()
                            .label,
                        note
                    );
                    assert_eq!(
                        host.semantics()
                            .iter()
                            .find(|node| node.id == ids.edit_note)
                            .unwrap()
                            .semantics
                            .name,
                        note
                    );
                    assert!(
                        text.labels
                            .iter()
                            .filter(|(_, layout)| layout.source() == note)
                            .all(|(_, layout)| layout.paragraph().lines.len() == 1)
                    );
                    if note == LONG_NOTE {
                        assert!(text.labels.iter().any(|(_, layout)| layout.source() == note && layout.is_truncated()));
                    }
                    let foot = host
                        .get(footer)
                        .unwrap()
                        .component::<TurnFootnote>()
                        .unwrap();
                    let copy = foot.copy_button_id().unwrap();
                    let info = foot.information_id().unwrap();
                    let controls = [
                        copy,
                        ids.positive,
                        ids.negative,
                        ids.edit_note,
                        ids.refresh,
                        info,
                    ];
                    let rectangles: Vec<_> = controls
                        .iter()
                        .map(|id| host.get(*id).unwrap().frame())
                        .collect();
                    assert!(
                        (rectangles[0].min_x() - host.get(footer).unwrap().frame().min_x()).abs()
                            < 0.02
                    );
                    for pair in rectangles.windows(2) {
                        assert!(pair[1].min_x() >= pair[0].max_x());
                        assert!((pair[0].center().y - pair[1].center().y).abs() < 0.02);
                    }
                    host.focus(Some(editor), true);
                    host.event(&ContentEvent::Preedit {
                        text: "备注变化时中文组字".into(),
                        selection: 1..3,
                        replacement: None,
                        clauses: vec![],
                    });
                    let baseline = host.text_surface(editor).unwrap().input.external_baseline();
                    let editor_owner = host.text_surface(editor).unwrap() as *const _ as usize;
                    let original_note = host
                        .get(ids.edit_note)
                        .unwrap()
                        .component::<ButtonContent>()
                        .unwrap() as *const _ as usize;
                    let initial_note_width = host.get(ids.edit_note).unwrap().frame().size.width;
                    assert!(initial_note_width > 0.);
                    // Full column -> narrow -> full column must not compound the
                    // 40% cap against this feedback group's intrinsic width.
                    for resized_width in [180., width] {
                        let resized = Rect::new(
                            frame.min_x(),
                            frame.min_y(),
                            resized_width,
                            frame.size.height,
                        );
                        host.render(&mut Scene::new(), &mut text, resized, resized, theme.into())
                            .unwrap();
                        assert!(
                            host.get(ids.edit_note).unwrap().frame().size.width
                                <= resized_width * 0.4 + 0.02
                        );
                        assert_eq!(host.focused(), Some(editor));
                        assert_eq!(
                            host.text_surface(editor).unwrap().input.external_baseline(),
                            baseline
                        );
                        assert_eq!(
                            host.text_surface(editor).unwrap() as *const _ as usize,
                            editor_owner
                        );
                        assert_eq!(
                            host.get(ids.edit_note)
                                .unwrap()
                                .component::<ButtonContent>()
                                .unwrap() as *const _ as usize,
                            original_note
                        );
                    }
                    assert!(
                        (host.get(ids.edit_note).unwrap().frame().size.width - initial_note_width)
                            .abs()
                            < 0.02
                    );
                    let version = host
                        .get(root)
                        .unwrap()
                        .component::<TranscriptContent>()
                        .unwrap()
                        .prepare_update(&scope, 2, 1, &snapshot, true)
                        .unwrap();
                    assert_eq!(
                        version
                            .item_ids()
                            .iter()
                            .filter(|id| **id == footer)
                            .count(),
                        1
                    );
                    assert!(!version.item_ids().contains(&feedback));
                    host.get_mut(root)
                        .unwrap()
                        .component_mut::<TranscriptContent>()
                        .unwrap()
                        .update(&scope, 2, 1, &snapshot, true)
                        .unwrap();
                    host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                        .unwrap();
                    assert_eq!(
                        host.get(feedback)
                            .unwrap()
                            .component::<TurnFeedback>()
                            .unwrap() as *const _ as usize,
                        original
                    );
                    assert_eq!(
                        host.get(ids.edit_note)
                            .unwrap()
                            .component::<ButtonContent>()
                            .unwrap() as *const _ as usize,
                        original_note
                    );
                    assert_eq!(host.focused(), Some(editor));
                    assert_eq!(
                        host.text_surface(editor).unwrap().input.external_baseline(),
                        baseline
                    );
                }
            }
        }
    }
}
