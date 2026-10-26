//! Thin host adapter: preserves native pointer metadata at the public composition boundary.
use tekes_ui_controls::composition::{
    ContentEvent, PointerButton, PointerEvent, PointerModifiers, PointerPhase,
};
use tekes_ui_platform_macos::{MouseButton, MouseEvent, MouseEventKind};
pub fn translate(m: MouseEvent) -> ContentEvent {
    let button = |b| match b {
        MouseButton::Left => PointerButton::Primary,
        MouseButton::Right => PointerButton::Secondary,
        MouseButton::Middle => PointerButton::Middle,
        MouseButton::Other(n) => PointerButton::Other(n),
    };
    let (phase, pressed, click_count) = match m.kind {
        MouseEventKind::Down {
            button: b,
            click_count,
        } => (PointerPhase::Down, Some(button(b)), click_count),
        MouseEventKind::Up {
            button: b,
            click_count,
        } => (PointerPhase::Up, Some(button(b)), click_count),
        MouseEventKind::Dragged { button: b } => (PointerPhase::Drag, Some(button(b)), 0),
        MouseEventKind::Moved => (PointerPhase::Move, None, 0),
        MouseEventKind::Entered => (PointerPhase::Enter, None, 0),
        MouseEventKind::Exited => (PointerPhase::Exit, None, 0),
    };
    ContentEvent::Pointer(PointerEvent {
        position: tekes_ui_scene::Point::new(m.position.x as f32, m.position.y as f32),
        phase,
        button: pressed,
        click_count,
        modifiers: PointerModifiers {
            shift: m.modifiers.shift,
            control: m.modifiers.control,
            alt: m.modifiers.alt,
            command: m.modifiers.command,
            caps_lock: m.modifiers.caps_lock,
            function: m.modifiers.function,
        },
    })
}
