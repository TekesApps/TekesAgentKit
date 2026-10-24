#![cfg(target_os = "macos")]
#[path = "../src/native_pointer.rs"]
mod independent;
use tekes_ui_controls::composition::*;
use tekes_ui_platform_macos::{Modifiers, MouseButton, MouseEvent, MouseEventKind, Point};
#[test]
fn agentkit_host_adapter_retain_all_fields_for_every_mouse_phase_and_button() {
    let modifiers = Modifiers {
        shift: true,
        control: true,
        alt: true,
        command: true,
        caps_lock: true,
        function: true,
    };
    for (button, expected) in [
        (MouseButton::Left, PointerButton::Primary),
        (MouseButton::Right, PointerButton::Secondary),
        (MouseButton::Middle, PointerButton::Middle),
        (MouseButton::Other(17), PointerButton::Other(17)),
    ] {
        for (kind, phase, count, pressed) in [
            (
                MouseEventKind::Down {
                    button,
                    click_count: 3,
                },
                PointerPhase::Down,
                3,
                Some(expected),
            ),
            (
                MouseEventKind::Up {
                    button,
                    click_count: 2,
                },
                PointerPhase::Up,
                2,
                Some(expected),
            ),
            (
                MouseEventKind::Dragged { button },
                PointerPhase::Drag,
                0,
                Some(expected),
            ),
            (MouseEventKind::Moved, PointerPhase::Move, 0, None),
            (MouseEventKind::Entered, PointerPhase::Enter, 0, None),
            (MouseEventKind::Exited, PointerPhase::Exit, 0, None),
        ] {
            let native = MouseEvent {
                kind,
                position: Point { x: 23.25, y: 48.5 },
                modifiers,
            };
            {
                let translate = independent::translate;
                let ContentEvent::Pointer(p) = translate(native) else {
                    panic!("lost pointer input")
                };
                assert_eq!(p.position, tekes_ui_scene::Point::new(23.25, 48.5));
                assert_eq!(p.phase, phase);
                assert_eq!(p.button, pressed);
                assert_eq!(p.click_count, count);
                assert_eq!(
                    p.modifiers,
                    PointerModifiers {
                        shift: true,
                        control: true,
                        alt: true,
                        command: true,
                        caps_lock: true,
                        function: true
                    }
                );
            }
        }
    }
}
