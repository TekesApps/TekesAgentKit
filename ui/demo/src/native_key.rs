//! Thin host conversion only; editor policy and acknowledgement live in ContentHost.
use tekes_ui_controls::{
    ControlKey,
    composition::{ContentEvent, ContentKeyEvent, InputModifiers},
};
use tekes_ui_platform_macos::{KeyEvent, KeyEventKind, TextKeyDisposition, TextKeyEvent};
pub fn key(k: KeyEvent) -> ContentKeyEvent {
    ContentKeyEvent {
        key: match k.key_code {
            36 | 76 => Some(ControlKey::Enter),
            49 => Some(ControlKey::Space),
            48 => Some(ControlKey::Tab {
                reverse: k.modifiers.shift,
            }),
            53 => Some(ControlKey::Escape),
            123 => Some(ControlKey::Left),
            124 => Some(ControlKey::Right),
            125 => Some(ControlKey::Down),
            126 => Some(ControlKey::Up),
            115 => Some(ControlKey::Home),
            119 => Some(ControlKey::End),
            _ => None,
        },
        down: k.kind == KeyEventKind::Down,
        repeat: k.is_repeat,
        modifiers: InputModifiers {
            shift: k.modifiers.shift,
            control: k.modifiers.control,
            alt: k.modifiers.alt,
            command: k.modifiers.command,
            caps_lock: k.modifiers.caps_lock,
            function: k.modifiers.function,
        },
        native_code: k.key_code,
        characters: k.characters,
        characters_ignoring_modifiers: k.characters_ignoring_modifiers,
    }
}
pub fn translate(event: TextKeyEvent) -> ContentEvent {
    match event {
        TextKeyEvent::Started(context) => ContentEvent::TextKeyStart {
            id: context.id,
            key: key(context.key),
            composing: context.composing_at_start,
        },
        TextKeyEvent::Finished {
            context,
            disposition,
            ..
        } => ContentEvent::TextKeyEnd {
            id: context.id,
            input_context_consumed: disposition == TextKeyDisposition::InputContextConsumed,
        },
    }
}
