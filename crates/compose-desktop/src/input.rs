//! Mapping winit's events to compose-ui's: pointer, wheel, keys with modifiers, committed text
//! and input method commits, focus.

use compose_ui::{
    Key, KeyEvent, KeyEventType, KeyModifiers, Offset, PointerEvent, PointerId, PointerType,
    WheelStep, platform::UiHost,
};
use winit::event::{
    ElementState, Ime, MouseButton, MouseScrollDelta, Touch, TouchPhase, WindowEvent,
};
use winit::keyboard::{Key as WinitKey, NamedKey};

#[derive(Default)]
pub(crate) struct InputState {
    cursor: Offset,
    pressed: bool,
    modifiers: KeyModifiers,
    /// The cursor left the window during a drag: its hover ends at the release.
    left: bool,
}

pub(crate) enum Outcome {
    Handled,
    Ignored,
    /// Close the window (Cmd+W, Ctrl+W).
    Close,
    /// Quit the application (Cmd+Q on macOS).
    Quit,
}

fn key_of(key: &WinitKey) -> Option<Key> {
    Some(match key {
        WinitKey::Named(named) => match named {
            NamedKey::Enter => Key::Enter,
            NamedKey::Space => Key::Space,
            NamedKey::Tab => Key::Tab,
            NamedKey::Backspace => Key::Backspace,
            NamedKey::Delete => Key::Delete,
            NamedKey::Escape => Key::Escape,
            NamedKey::ArrowLeft => Key::DirectionLeft,
            NamedKey::ArrowRight => Key::DirectionRight,
            NamedKey::ArrowUp => Key::DirectionUp,
            NamedKey::ArrowDown => Key::DirectionDown,
            NamedKey::Home => Key::MoveHome,
            NamedKey::End => Key::MoveEnd,
            NamedKey::PageUp => Key::PageUp,
            NamedKey::PageDown => Key::PageDown,
            _ => return None,
        },
        WinitKey::Character(text) => Key::Character(text.chars().next()?),
        _ => return None,
    })
}

/// Whether a wheel's event follows the fingers or a freely turning wheel, so it applies at once, as
/// Compose Desktop's `isPreciseWheelScroll`: pixels (a macOS or Wayland trackpad), or a fraction of
/// a line (X11's smooth scrolling). Not on Windows, whose ordinary wheels can report fractions.
fn is_precise_wheel(delta: &MouseScrollDelta) -> bool {
    if cfg!(target_os = "windows") {
        return false;
    }
    match delta {
        MouseScrollDelta::PixelDelta(_) => true,
        MouseScrollDelta::LineDelta(x, y) => x.fract() != 0.0 || y.fract() != 0.0,
    }
}

/// A wheel's step on this platform, as Compose Desktop's `ScrollConfig`s (the JetBrains fork's
/// `DesktopScrollable.desktop.kt`, not in androidx-main; D1450): macOS's lines are 10 dp (its
/// driver accelerates them); Windows's the scrollable's height over 20 and Linux's the root of its
/// height, each times AWT's `scrollAmount`, the system's lines per notch: 3 by default on both
/// (Windows's setting isn't read here).
fn wheel_step() -> WheelStep {
    if cfg!(target_os = "macos") {
        WheelStep::Dp(10.0)
    } else if cfg!(target_os = "windows") {
        WheelStep::HeightOverTwenty { scroll_amount: 3.0 }
    } else {
        WheelStep::SquareRootOfHeight { scroll_amount: 3.0 }
    }
}

/// A wheel's event: lines, in its platform's step; pixels, as they are.
fn wheel_event(position: Offset, delta: &MouseScrollDelta) -> PointerEvent {
    let event = match delta {
        MouseScrollDelta::LineDelta(x, y) => {
            PointerEvent::scroll(position, Offset::new(-x, -y)).with_wheel_step(wheel_step())
        }
        MouseScrollDelta::PixelDelta(pixels) => {
            PointerEvent::scroll(position, Offset::new(-pixels.x as f32, -pixels.y as f32))
        }
    };
    event.with_precise_wheel(is_precise_wheel(delta))
}

pub(crate) fn handle(state: &mut InputState, host: &mut UiHost, event: &WindowEvent) -> Outcome {
    match event {
        WindowEvent::CursorMoved { position, .. } => {
            state.cursor = Offset::new(position.x as f32, position.y as f32);
            host.send_pointer(PointerEvent::moved(state.cursor, state.pressed));
        }
        WindowEvent::MouseInput { state: button_state, button: MouseButton::Left, .. } => {
            let pressed = *button_state == ElementState::Pressed;
            state.pressed = pressed;
            let event = if pressed {
                PointerEvent::press(state.cursor)
            } else {
                PointerEvent::release(state.cursor)
            };
            host.send_pointer(event);
            if !pressed && std::mem::take(&mut state.left) {
                host.send_pointer_left();
            }
        }
        WindowEvent::MouseWheel { delta, .. } => {
            host.send_pointer(wheel_event(state.cursor, delta));
        }
        WindowEvent::Touch(Touch { phase, location, id, .. }) => {
            let position = Offset::new(location.x as f32, location.y as f32);
            let pointer = PointerId(*id + 1);
            let event = match phase {
                TouchPhase::Started => PointerEvent::press(position),
                TouchPhase::Moved => PointerEvent::moved(position, true),
                TouchPhase::Ended => PointerEvent::release(position),
                // A touch the system took back isn't a tap: its gesture ends without a release.
                TouchPhase::Cancelled => {
                    host.with_tree(|tree| tree.cancel_pointer(pointer));
                    return Outcome::Handled;
                }
            };
            host.send_pointer(event.with_pointer(pointer, PointerType::Touch));
        }
        // A drag goes on outside the window (the system keeps sending its moves and its
        // release), as Compose Desktop's.
        WindowEvent::CursorLeft { .. } if state.pressed => state.left = true,
        WindowEvent::CursorEntered { .. } => state.left = false,
        WindowEvent::CursorLeft { .. } | WindowEvent::Focused(false) => {
            state.pressed = false;
            host.with_tree(|tree| tree.cancel_pointers());
            host.send_pointer_left();
        }
        WindowEvent::ModifiersChanged(modifiers) => {
            let modifiers = modifiers.state();
            state.modifiers = KeyModifiers {
                shift: modifiers.shift_key(),
                ctrl: modifiers.control_key(),
                alt: modifiers.alt_key(),
                meta: modifiers.super_key(),
            };
        }
        WindowEvent::KeyboardInput { event, .. } => {
            let pressed = event.state == ElementState::Pressed;
            // Closing shortcuts: Cmd+W and Cmd+Q on macOS, Ctrl+W elsewhere.
            let command =
                if cfg!(target_os = "macos") { state.modifiers.meta } else { state.modifiers.ctrl };
            if pressed
                && command
                && let WinitKey::Character(text) = &event.logical_key
                && (text.as_str() == "w" || (cfg!(target_os = "macos") && text.as_str() == "q"))
            {
                return if text.as_str() == "q" { Outcome::Quit } else { Outcome::Close };
            }
            let Some(key) = key_of(&event.logical_key) else { return Outcome::Ignored };
            let kind = if pressed { KeyEventType::KeyDown } else { KeyEventType::KeyUp };
            let consumed = host.send_key(KeyEvent { key, kind, modifiers: state.modifiers });
            // The key's committed text, unless it's a shortcut: the focused node takes it (a
            // text field takes the typed key too, so its ancestors don't act on it).
            let _ = consumed;
            if pressed
                && !state.modifiers.ctrl
                && !state.modifiers.meta
                && let Some(text) = &event.text
                && text.chars().all(|c| !c.is_control())
            {
                host.send_text(text);
            }
        }
        WindowEvent::Ime(Ime::Commit(text)) => {
            // The composition ends with the commit.
            host.send_preedit("", None);
            host.send_text(text);
        }
        WindowEvent::Ime(Ime::Preedit(text, cursor)) => {
            host.send_preedit(text, *cursor);
        }
        WindowEvent::Ime(Ime::Disabled) => {
            host.send_preedit("", None);
        }
        _ => return Outcome::Ignored,
    }
    Outcome::Handled
}

#[cfg(test)]
mod wheel_tests {
    use compose_ui::IntSize;
    use winit::dpi::PhysicalPosition;

    use super::*;

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn pixels_and_fractions_of_a_line_are_precise() {
        assert!(is_precise_wheel(&MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, 3.0))));
        assert!(is_precise_wheel(&MouseScrollDelta::LineDelta(0.0, 0.25)));
        assert!(is_precise_wheel(&MouseScrollDelta::LineDelta(0.5, 2.0)));
        assert!(!is_precise_wheel(&MouseScrollDelta::LineDelta(0.0, -2.0)));
    }

    #[test]
    fn a_line_is_the_platforms_step_and_pixels_are_pixels() {
        let bounds = IntSize::new(100, 400);
        let line = wheel_event(Offset::ZERO, &MouseScrollDelta::LineDelta(0.0, -2.0));
        // 10 dp on macOS; 3 lines of the height over 20 on Windows and of its root on Linux, which
        // a height of 400 makes the same.
        let step = if cfg!(target_os = "macos") { 10.0 * 2.0 } else { 20.0 * 3.0 };
        assert_eq!(line.wheel_pixels(2.0, bounds), Offset::new(0.0, 2.0 * step));
        let pixels = wheel_event(
            Offset::ZERO,
            &MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, -3.0)),
        );
        assert_eq!(pixels.wheel_pixels(2.0, bounds), Offset::new(0.0, 3.0));
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn nothing_is_precise_on_windows() {
        assert!(!is_precise_wheel(&MouseScrollDelta::LineDelta(0.0, 0.25)));
    }
}
