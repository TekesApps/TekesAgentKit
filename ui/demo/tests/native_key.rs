#![cfg(target_os = "macos")]
#[path = "../src/native_key.rs"]
mod native;
use tekes_agent_components_ui::{AgentText, COMPOSER, RunSurface};
use tekes_ui_controls::{TextSurface, composition::*};
use tekes_ui_platform_macos::{
    KeyEvent, KeyEventKind, Modifiers, TextKeyContext, TextKeyDisposition as NativeDisposition,
    TextKeyEvent,
};
fn key() -> KeyEvent {
    KeyEvent {
        kind: KeyEventKind::Down,
        key_code: 123,
        characters: String::new(),
        characters_ignoring_modifiers: String::new(),
        is_repeat: false,
        modifiers: Modifiers {
            shift: true,
            ..Default::default()
        },
    }
}
#[test]
fn original_agent_wrapper_receives_shift_metadata_through_the_new_public_chain() {
    let mut surface = RunSurface::new();
    surface
        .tree
        .get_mut(COMPOSER)
        .unwrap()
        .component_mut::<AgentText>()
        .unwrap()
        .field
        .surface = TextSurface::new("ab😀", true, false);
    surface.tree.focus(Some(COMPOSER), true);
    surface
        .tree
        .get_mut(COMPOSER)
        .unwrap()
        .component_mut::<AgentText>()
        .unwrap()
        .field
        .surface
        .input
        .select(4, false);
    let context = TextKeyContext {
        id: 1,
        key: key(),
        composing_at_start: false,
    };
    surface.event(&native::translate(TextKeyEvent::Started(context.clone())));
    surface.event(&ContentEvent::KeyInput(native::key(key())));
    surface.event(&native::translate(TextKeyEvent::Finished {
        context,
        input_context_handled: Some(false),
        disposition: NativeDisposition::Unhandled,
        text_callbacks: 0,
        command_callbacks: 0,
    }));
    assert_eq!(surface.editor().input.selected_text(), "😀");
}
#[test]
fn agent_host_keeps_up_repeat_and_all_modifiers_and_does_not_mark_command_callbacks_as_opaque_ime()
{
    let mut k = key();
    k.kind = KeyEventKind::Up;
    k.is_repeat = true;
    k.key_code = 0;
    k.characters = "é".into();
    k.characters_ignoring_modifiers = "e".into();
    k.modifiers = Modifiers {
        shift: true,
        control: true,
        command: true,
        alt: true,
        caps_lock: true,
        function: true,
    };
    let adapted = native::key(k.clone());
    assert!(!adapted.down);
    assert!(adapted.repeat);
    assert_eq!(adapted.key, None);
    assert_eq!(adapted.characters, "é");
    assert_eq!(adapted.characters_ignoring_modifiers, "e");
    assert_eq!(
        adapted.modifiers,
        InputModifiers {
            shift: true,
            control: true,
            command: true,
            alt: true,
            caps_lock: true,
            function: true
        }
    );
    let context = TextKeyContext {
        id: 2,
        key: k,
        composing_at_start: true,
    };
    for disposition in [
        NativeDisposition::TextDispatched,
        NativeDisposition::CommandDispatched,
        NativeDisposition::InputContextConsumed,
        NativeDisposition::NotOffered,
        NativeDisposition::Unhandled,
    ] {
        let ContentEvent::TextKeyEnd {
            input_context_consumed,
            ..
        } = native::translate(TextKeyEvent::Finished {
            context: context.clone(),
            input_context_handled: Some(true),
            disposition,
            text_callbacks: 0,
            command_callbacks: 0,
        })
        else {
            panic!("end")
        };
        assert_eq!(
            input_context_consumed,
            disposition == NativeDisposition::InputContextConsumed
        );
    }
}
