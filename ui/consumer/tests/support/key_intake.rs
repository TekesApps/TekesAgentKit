//! Original Composer over the SDK's shared native-key callback boundary.
use super::*;
use tekes_ui_controls::ControlKey;

#[test]
fn candidate_confirming_arrow_keeps_original_composer_commit_but_plain_arrow_moves() {
    for (key, code, command, at_start) in [
        (ControlKey::Up, 126, "moveUp:", false),
        (ControlKey::Down, 125, "moveDown:", true),
    ] {
        let composer = tekes_agent_ui_consumer_check::new_composer(64500);
        let ids = composer.ids();
        let mut host = composer.into_host().unwrap();
        let mut text = Text::new(1., true);
        let frame = Rect::new(0., 0., 700., 800.);
        host.render(
            &mut Scene::new(),
            &mut text,
            frame,
            frame,
            Theme::LIGHT.into(),
        )
        .unwrap();
        assert!(host.focus(Some(ids.editor), true));
        host.event(&ContentEvent::Commit {
            text: "第一行原始草稿\n第二行原始草稿".into(),
            replacement: None,
        });
        if at_start {
            host.accessibility_action(ids.editor, None, AccessibilityAction::SetSelection(0..0));
        }
        host.event(&ContentEvent::Preedit {
            text: "zhongwen".into(),
            selection: 0..8,
            replacement: None,
            clauses: vec![],
        });
        let input = ContentKeyEvent {
            key: Some(key),
            down: true,
            repeat: false,
            modifiers: Default::default(),
            native_code: code,
            characters: String::new(),
            characters_ignoring_modifiers: String::new(),
        };
        let owner = host.text_surface(ids.editor).unwrap() as *const _ as usize;
        host.event(&ContentEvent::TextKeyStart {
            id: 1,
            key: input.clone(),
            composing: true,
        });
        host.event(&ContentEvent::Commit {
            text: "中文".into(),
            replacement: None,
        });
        host.render(
            &mut Scene::new(),
            &mut text,
            frame,
            frame,
            Theme::LIGHT.into(),
        )
        .unwrap();
        let committed_text = host.text_surface(ids.editor).unwrap().input.text.clone();
        let committed = host
            .text_surface(ids.editor)
            .unwrap()
            .input
            .external_baseline();
        assert!(
            host.text_surface(ids.editor)
                .unwrap()
                .input
                .marked
                .is_none()
        );
        host.event(&ContentEvent::TextCommand(command.into()));
        host.event(&ContentEvent::KeyInput(input.clone()));
        let outcome = host.event(&ContentEvent::TextKeyEnd {
            id: 1,
            input_context_consumed: false,
        });
        assert_eq!(
            outcome.text_key.unwrap().disposition,
            TextKeyDisposition::InputConsumed
        );
        assert!(!outcome.activated);
        assert_eq!(host.focused(), Some(ids.editor));
        assert_eq!(
            host.text_surface(ids.editor).unwrap() as *const _ as usize,
            owner
        );
        assert_eq!(
            host.text_surface(ids.editor)
                .unwrap()
                .input
                .external_baseline(),
            committed
        );
        assert!(
            host.drain_actions()
                .iter()
                .all(|a| a.kind != ContentActionKind::Activate)
        );

        // The guard must not disable ordinary editing after composition ends.
        let before = host.text_surface(ids.editor).unwrap().input.caret;
        host.event(&ContentEvent::TextKeyStart {
            id: 2,
            key: input.clone(),
            composing: false,
        });
        host.event(&ContentEvent::TextCommand(command.into()));
        host.event(&ContentEvent::KeyInput(input));
        let outcome = host.event(&ContentEvent::TextKeyEnd {
            id: 2,
            input_context_consumed: false,
        });
        assert_eq!(
            outcome.text_key.unwrap().disposition,
            TextKeyDisposition::EditHandled
        );
        assert_ne!(host.text_surface(ids.editor).unwrap().input.caret, before);
        assert_eq!(
            host.text_surface(ids.editor).unwrap().input.text,
            committed_text
        );
        assert_eq!(host.focused(), Some(ids.editor));
    }
}
