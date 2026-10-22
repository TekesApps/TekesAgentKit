use super::*;
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
                            note: Some("备注".into()),
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
                let mut host = content.into_host().unwrap();
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
            }
        }
    }
}
