use super::*;
use tekes_ui_controls::extended::{Axis, Stack};
#[test]
fn bounded_output_append_and_scroll_leave_the_original_composer_marked_owner_untouched() {
    for scale in [1., 1.4] {
        let scope = RunScope::main("consumer", "bounded-details");
        let mut message = TranscriptItem {
            id: "reasoning".into(),
            record_id: Some("reasoning".into()),
            reconciliation_id: None,
            turn_id: "turn".into(),
            kind: TranscriptItemKind::Assistant,
            text: String::new(),
            reasoning: "中文长推理 👩🏽‍💻 e\u{301}\n".repeat(100),
            attachments: vec![],
            content_blocks: None,
            is_visible: true,
            is_final: false,
            is_provisional: true,
            reasoning_ended: false,
            source_sequence: None,
            source_position: None,
            time: None,
            metadata: Default::default(),
        };
        let mut output =
            OutputMessage::with_bounded_details(64301, scope.clone(), Default::default());
        output
            .update(
                &scope,
                1,
                1,
                &TranscriptOutput {
                    key: TranscriptMessageKey::NativeEntry("reasoning"),
                    item: &message,
                },
                None,
            )
            .unwrap();
        let root = output.id();
        let detail = output.content_ids()[0];
        let composer = tekes_agent_ui_consumer_check::new_composer(64302);
        let composer_root = composer.ids().root;
        let editor = composer.ids().editor;
        let mut host = ContentHost::new(Content::new(
            ComponentId(13),
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: 0.,
                },
                flexible_child: None,
                children: vec![
                    Content::new(root, output),
                    Content::new(composer_root, composer),
                ],
            },
        ))
        .unwrap();
        host.set_action_events(true);
        host.accessibility_action(detail.disclosure.unwrap(), None, AccessibilityAction::Press);
        let actions = host.drain_actions();
        assert!(
            host.get_mut(root)
                .unwrap()
                .component_mut::<OutputMessage>()
                .unwrap()
                .handle_actions(&actions)
        );
        let mut text = Text::new(scale, true);
        let frame = Rect::new(0., 0., 640., 1000.);
        host.render(
            &mut Scene::new(),
            &mut text,
            frame,
            frame,
            Theme::LIGHT.into(),
        )
        .unwrap();
        assert!(host.focus(Some(editor), true));
        host.event(&ContentEvent::Preedit {
            text: "当前中文组字".into(),
            selection: 1..3,
            clauses: vec![],
            replacement: None,
        });
        let baseline = host.text_surface(editor).unwrap().input.external_baseline();
        let editor_owner = host.text_surface(editor).unwrap() as *const _ as usize;
        let detail_owner = host.text_surface(detail.body).unwrap() as *const _ as usize;
        for (index, width) in [640., 320., 640.].into_iter().enumerate() {
            message.reasoning.push_str("后台追加，不修改输入\n");
            host.get_mut(root)
                .unwrap()
                .component_mut::<OutputMessage>()
                .unwrap()
                .update(
                    &scope,
                    index as u64 + 2,
                    1,
                    &TranscriptOutput {
                        key: TranscriptMessageKey::NativeEntry("reasoning"),
                        item: &message,
                    },
                    None,
                )
                .unwrap();
            let frame = Rect::new(0., 0., width, 1000.);
            host.render(
                &mut Scene::new(),
                &mut text,
                frame,
                frame,
                Theme::DARK.into(),
            )
            .unwrap();
            let position = host.get(detail.body).unwrap().visible().unwrap().center();
            host.event(&ContentEvent::Scroll {
                position,
                delta: Point::new(0., 40.),
                precise: true,
                begin: true,
                end: true,
                momentum: false,
                time: index as f64 + 1.,
            });
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
                host.text_surface(detail.body).unwrap() as *const _ as usize,
                detail_owner
            );
            assert!(
                host.text_surface(detail.body).unwrap().outer.size.height <= 240. * scale + 0.5
            );
            assert!(host.text_surface(detail.body).unwrap().input.read_only);
            assert!(
                host.text_surface(detail.body)
                    .unwrap()
                    .input
                    .marked
                    .is_none()
            );
        }
        let actions = host.drain_actions();
        let intents = host
            .get_mut(composer_root)
            .unwrap()
            .component_mut::<Composer>()
            .unwrap()
            .take_intents(&actions);
        assert!(intents.iter().all(|intent| !matches!(
            intent,
            ComposerIntent::Send { .. } | ComposerIntent::Cancel { .. }
        )));
    }
}
