use super::*;
use tekes_ui_controls::extended::{Axis, Stack};
const INFO: &str = "完整发送时间与本地化说明 👩🏽‍💻 e\u{301} 这是不可修改的完整信息。完整发送时间与本地化说明。完整发送时间与本地化说明。完整发送时间与本地化说明。完整发送时间与本地化说明。";
fn item(text: &str) -> TranscriptItem {
    TranscriptItem {
        id: "input".into(),
        record_id: Some("public-input".into()),
        reconciliation_id: None,
        turn_id: "turn".into(),
        kind: TranscriptItemKind::User,
        text: text.into(),
        reasoning: String::new(),
        attachments: if text.is_empty() {
            vec![[("name", "附件.txt")].into_iter().collect()]
        } else {
            vec![]
        },
        content_blocks: None,
        is_visible: true,
        is_final: false,
        is_provisional: false,
        reasoning_ended: true,
        source_sequence: Some(1),
        source_position: Some(TranscriptSourcePosition::Journal {
            sequence: 1,
            content_index: None,
        }),
        time: Some(1000.),
        metadata: Default::default(),
    }
}

#[test]
fn historical_input_uses_original_trailing_controls_without_taking_composer_preedit() {
    for theme in Theme::ALL {
        for scale in [1., 1.4, 2.] {
            let scope = RunScope::main("consumer", "historical-input");
            let source = item("原始输入 👩🏽‍💻，不替换当前草稿");
            let input = TranscriptInput {
                key: TranscriptMessageKey::NativeEntry("original-entry"),
                item: &source,
                footnote: TranscriptDelivery::Recorded { time: Some(1000.) },
            };
            let mut row = InputMessage::new(64211, scope.clone());
            row.update_with(&scope, 1, 1, &input, |_| INFO.into())
                .unwrap();
            assert!(row.historical_input_button_id().is_none());
            row.configure_historical_input(Some(HistoricalInputControl {
                action: HistoricalInputAction::Edit,
                label: "编辑输入".into(),
            }))
            .unwrap();
            let ids = row.ids();
            let edit = row.historical_input_button_id().unwrap();
            let copy = row.copy_button_id().unwrap();
            let composer = tekes_agent_ui_consumer_check::new_composer(64212);
            let editor = composer.ids().editor;
            let mut host = ContentHost::new(Content::new(
                ComponentId(12),
                StackContent {
                    model: Stack {
                        axis: Axis::Vertical,
                        gap: 0.,
                    },
                    flexible_child: None,
                    children: vec![
                        Content::new(ids.root, row),
                        Content::new(composer.ids().root, composer),
                    ],
                },
            ))
            .unwrap();
            host.set_action_events(true);
            let mut text = Text::new(scale, true);
            let frame = Rect::new(20., 30., 770., 1200.);
            host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                .unwrap();
            host.focus(Some(editor), true);
            host.event(&ContentEvent::Preedit {
                text: "当前组字".into(),
                selection: 1..3,
                clauses: vec![],
                replacement: None,
            });
            let baseline = host.text_surface(editor).unwrap().input.external_baseline();
            let information_owner = host
                .get(ids.footnote)
                .unwrap()
                .component::<TextContent>()
                .unwrap() as *const _ as usize;
            let edit_owner =
                host.get(edit).unwrap().component::<Button>().unwrap() as *const _ as usize;
            for width in [770., 320., 180., 770.] {
                let frame = Rect::new(20., 30., width, 1200.);
                host.get_mut(ids.root)
                    .unwrap()
                    .component_mut::<InputMessage>()
                    .unwrap()
                    .update_with(&scope, 2, 1, &input, |_| INFO.into())
                    .unwrap();
                host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                    .unwrap();
                let info = host.get(ids.footnote).unwrap();
                assert_eq!(
                    info.component::<TextContent>().unwrap() as *const _ as usize,
                    information_owner
                );
                assert_eq!(
                    host.get(edit).unwrap().component::<Button>().unwrap() as *const _ as usize,
                    edit_owner
                );
                let a = info.frame();
                let b = host.get(edit).unwrap().frame();
                let c = host.get(copy).unwrap().frame();
                assert!(a.max_x() <= b.min_x() + 0.02 && b.max_x() <= c.min_x() + 0.02);
                assert!(
                    (a.center().y - b.center().y).abs() < 0.02
                        && (b.center().y - c.center().y).abs() < 0.02
                );
                assert!((c.max_x() - frame.max_x()).abs() < 0.02);
                assert_eq!(info.component::<TextContent>().unwrap().text, INFO);
                assert_eq!(host.focused(), Some(editor));
                assert_eq!(
                    host.text_surface(editor).unwrap().input.external_baseline(),
                    baseline
                );
            }
            host.accessibility_action(edit, None, AccessibilityAction::Press);
            let actions = host.drain_actions();
            let row = host
                .get(ids.root)
                .unwrap()
                .component::<InputMessage>()
                .unwrap();
            let intent = row.take_historical_input_intents(&actions).pop().unwrap();
            assert_eq!(intent.input(), &source);
            assert_eq!(intent.turn_id(), "turn");
            assert_eq!(intent.scope(), &scope);
            assert_eq!(intent.action(), HistoricalInputAction::Edit);
            assert_eq!(host.focused(), Some(editor));
            assert_eq!(
                host.text_surface(editor).unwrap().input.external_baseline(),
                baseline
            );
            assert!(intent.claim());
            assert!(!intent.clone().claim());
            let row = host
                .get_mut(ids.root)
                .unwrap()
                .component_mut::<InputMessage>()
                .unwrap();
            row.configure_historical_input(None).unwrap();
            assert!(row.take_historical_input_intents(&actions).is_empty());
            row.configure_historical_input(Some(HistoricalInputControl {
                action: HistoricalInputAction::Rewind,
                label: "回退到此输入".into(),
            }))
            .unwrap();
            assert_ne!(row.historical_input_button_id(), Some(edit));
            assert!(row.take_historical_input_intents(&actions).is_empty());
            let pending = TranscriptInput {
                footnote: TranscriptDelivery::Pending,
                ..input
            };
            row.update(&scope, 3, 1, &pending).unwrap();
            assert!(row.historical_input_button_id().is_none());
            assert_eq!(row.copy_button_id(), Some(copy));
        }
    }
}
#[test]
fn input_aside_is_one_external_trailing_band_with_full_source_and_original_copy_owner() {
    let scope = RunScope::main("consumer", "input-aside");
    for theme in Theme::ALL {
        for scale in [1., 1.4, 2.] {
            for width in [180., 320., 770.] {
                for source in [item("原始中文输入"), item("")] {
                    let input = TranscriptInput {
                        key: TranscriptMessageKey::NativeEntry("original-entry"),
                        item: &source,
                        footnote: TranscriptDelivery::Recorded { time: Some(1000.) },
                    };
                    let mut message = InputMessage::new(64201, scope.clone());
                    message
                        .update_with(&scope, 1, 1, &input, |_| INFO.into())
                        .unwrap();
                    let ids = message.ids();
                    let aside = message.aside_id();
                    let copy = message.copy_button_id();
                    let composer = tekes_agent_ui_consumer_check::new_composer(64202);
                    let editor = composer.ids().editor;
                    let mut host = ContentHost::new(Content::new(
                        ComponentId(11),
                        StackContent {
                            model: Stack {
                                axis: Axis::Vertical,
                                gap: 0.,
                            },
                            flexible_child: None,
                            children: vec![
                                Content::new(ids.root, message),
                                Content::new(composer.ids().root, composer),
                            ],
                        },
                    ))
                    .unwrap();
                    host.set_action_events(true);
                    let mut text = Text::new(scale, true);
                    let frame = Rect::new(20., 30., width, 1200.);
                    host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                        .unwrap();
                    let original = host
                        .get(ids.footnote)
                        .unwrap()
                        .component::<TextContent>()
                        .unwrap() as *const _ as usize;
                    let note = host.get(ids.footnote).unwrap().frame();
                    let copy_width = copy.map_or(0., |id| host.get(id).unwrap().frame().size.width);
                    let gap = if copy.is_some() {
                        tekes_ui_tokens::Spacing::Regular.value()
                    } else {
                        0.
                    };
                    assert!(note.size.width <= (width - copy_width - gap).max(0.) + 0.02);
                    if width == 770. {
                        assert!(
                            note.size.width > width * 0.4,
                            "read-only information must use remaining width rather than the action-label cap"
                        );
                    }
                    assert_eq!(
                        host.get(ids.footnote)
                            .unwrap()
                            .component::<TextContent>()
                            .unwrap()
                            .text,
                        INFO
                    );
                    assert!(
                        text.labels
                            .iter()
                            .filter(|(_, layout)| layout.source() == INFO)
                            .any(|(_, layout)| layout.is_truncated())
                    );
                    assert!(
                        text.labels
                            .iter()
                            .filter(|(_, layout)| layout.source() == INFO)
                            .all(|(_, layout)| layout.paragraph().lines.len() == 1)
                    );
                    assert!(
                        (host.get(aside).unwrap().frame().max_x() - frame.max_x()).abs() < 0.02
                    );
                    let bubble = host.get(ids.bubble).unwrap().frame();
                    if let Some(copy) = copy {
                        let c = host.get(copy).unwrap().frame();
                        assert!((c.max_x() - frame.max_x()).abs() < 0.02);
                        assert!(note.max_x() <= c.min_x());
                        assert!((note.center().y - c.center().y).abs() < 0.02);
                        let row_top = note.min_y().min(c.min_y());
                        assert!(
                            (row_top - bubble.max_y() - tekes_ui_tokens::Spacing::Regular.value())
                                .abs()
                                < 0.02
                        );
                    } else {
                        assert_eq!(bubble.size.height, 0.);
                        assert!((note.max_x() - frame.max_x()).abs() < 0.02);
                        let last_detail = host
                            .get(ids.root)
                            .unwrap()
                            .component::<InputMessage>()
                            .unwrap()
                            .detail_ids()
                            .last()
                            .unwrap()
                            .root;
                        assert!(
                            (note.min_y()
                                - host.get(last_detail).unwrap().frame().max_y()
                                - tekes_ui_tokens::Spacing::Regular.value())
                            .abs()
                                < 0.02
                        );
                    }
                    host.focus(Some(editor), true);
                    host.event(&ContentEvent::Preedit {
                        text: "脚注变化时中文组字".into(),
                        selection: 1..3,
                        replacement: None,
                        clauses: vec![],
                    });
                    let baseline = host.text_surface(editor).unwrap().input.external_baseline();
                    let owner = host.text_surface(editor).unwrap() as *const _ as usize;
                    host.get_mut(ids.root)
                        .unwrap()
                        .component_mut::<InputMessage>()
                        .unwrap()
                        .update_with(&scope, 2, 1, &input, |_| "更新后的完整发送时间".into())
                        .unwrap();
                    host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                        .unwrap();
                    assert_eq!(
                        host.get(ids.footnote)
                            .unwrap()
                            .component::<TextContent>()
                            .unwrap() as *const _ as usize,
                        original
                    );
                    assert_eq!(host.focused(), Some(editor));
                    assert_eq!(
                        host.text_surface(editor).unwrap() as *const _ as usize,
                        owner
                    );
                    assert_eq!(
                        host.text_surface(editor).unwrap().input.external_baseline(),
                        baseline
                    );
                    if let Some(copy) = copy {
                        host.accessibility_action(copy, None, AccessibilityAction::Press);
                        let actions = host.drain_actions();
                        let intents = host
                            .get(ids.root)
                            .unwrap()
                            .component::<InputMessage>()
                            .unwrap()
                            .take_copy_intents(&actions);
                        assert_eq!(intents.len(), 1);
                        assert_eq!(intents[0].text(), source.text);
                        assert_eq!(intents[0].scope(), &scope);
                        assert!(intents[0].claim());
                        assert!(!intents[0].claim());
                        assert_eq!(host.focused(), Some(editor));
                        assert_eq!(
                            host.text_surface(editor).unwrap().input.external_baseline(),
                            baseline
                        );
                        host.get_mut(ids.root)
                            .unwrap()
                            .component_mut::<InputMessage>()
                            .unwrap()
                            .set_copy_label("拷贝原输入");
                        assert_eq!(
                            host.get(copy).unwrap().component::<Button>().unwrap().label,
                            "拷贝原输入"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn input_aside_rejects_missing_sdk_label_service_instead_of_silently_wrapping() {
    let scope = RunScope::main("consumer", "missing-label");
    let source = item("原输入");
    let input = TranscriptInput {
        key: TranscriptMessageKey::NativeEntry("entry"),
        item: &source,
        footnote: TranscriptDelivery::Pending,
    };
    let mut message = InputMessage::new(64203, scope.clone());
    message
        .update_with(&scope, 1, 1, &input, |_| INFO.into())
        .unwrap();
    let info = message.ids().footnote;
    let copy = message.copy_button_id().unwrap();
    let mut host = message.into_host().unwrap();
    let frame = Rect::new(0., 0., 180., 500.);
    let error = host
        .render(
            &mut Scene::new(),
            &mut Text::new(1., false),
            frame,
            frame,
            Theme::LIGHT.into(),
        )
        .unwrap_err();
    assert!(
        matches!(error, CompositionError::MissingLabelServices(id) if id == info || id == copy)
    );
}
