//! Mapping winit's events to compose-ui's: pointer, wheel, keys with modifiers, committed text
//! and input method commits, focus.

use compose_ui::{
    Key, KeyEvent, KeyEventType, KeyModifiers, Offset, PointerButtons, PointerEvent, PointerId,
    PointerType, WheelStep, platform::UiHost,
};
use winit::event::{
    ElementState, Ime, MouseButton, MouseScrollDelta, Touch, TouchPhase, WindowEvent,
};
use winit::keyboard::{Key as WinitKey, NamedKey};

#[derive(Default)]
pub(crate) struct InputState {
    cursor: Offset,
    /// The mouse's buttons held.
    buttons: PointerButtons,
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

/// A mouse's button among those of an event, as `PointerButtons` numbers them: a button with no
/// purpose past the stylus's two, up to what the bits hold.
fn button_bit(button: MouseButton) -> i32 {
    match button {
        MouseButton::Left => PointerButtons::PRIMARY.0,
        MouseButton::Right => PointerButtons::SECONDARY.0,
        MouseButton::Middle => PointerButtons::TERTIARY.0,
        MouseButton::Back => PointerButtons::BACK.0,
        MouseButton::Forward => PointerButtons::FORWARD.0,
        MouseButton::Other(index) => 1 << (index.clamp(5, 28) + 2),
    }
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

/// A wheel's step on this platform, as Compose Desktop's `ScrollConfig`s, **unverified**: the
/// JetBrains fork's `DesktopScrollable.desktop.kt` isn't in androidx-main, so these are from memory
/// (D1450). macOS's lines are 10 dp (its driver accelerates them); Windows's the scrollable's height
/// over 20 and Linux's the root of its height, each times AWT's `scrollAmount`, the system's lines
/// per notch: 3 by default on both (Windows's setting isn't read here).
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
            let held = state.buttons.are_any_pressed();
            host.send_pointer(
                PointerEvent::moved(state.cursor, held)
                    .with_buttons(state.buttons)
                    .with_keyboard_modifiers(state.modifiers.into()),
            );
        }
        WindowEvent::MouseInput { state: button_state, button, .. } => {
            let pressed = *button_state == ElementState::Pressed;
            let bit = button_bit(*button);
            let buttons = if pressed { state.buttons.0 | bit } else { state.buttons.0 & !bit };
            state.buttons = PointerButtons(buttons);
            let event = if pressed {
                PointerEvent::press(state.cursor)
            } else {
                PointerEvent::release(state.cursor)
            };
            host.send_pointer(
                event.with_buttons(state.buttons).with_keyboard_modifiers(state.modifiers.into()),
            );
            if !state.buttons.are_any_pressed() && std::mem::take(&mut state.left) {
                host.send_pointer_left();
            }
        }
        WindowEvent::MouseWheel { delta, .. } => {
            host.send_pointer(
                wheel_event(state.cursor, delta).with_keyboard_modifiers(state.modifiers.into()),
            );
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
            host.send_pointer(
                event
                    .with_pointer(pointer, PointerType::Touch)
                    .with_keyboard_modifiers(state.modifiers.into()),
            );
        }
        // A drag goes on outside the window (the system keeps sending its moves and its
        // release), as Compose Desktop's.
        WindowEvent::CursorLeft { .. } if state.buttons.are_any_pressed() => state.left = true,
        WindowEvent::CursorEntered { .. } => state.left = false,
        WindowEvent::CursorLeft { .. } | WindowEvent::Focused(false) => {
            state.buttons = PointerButtons::NONE;
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
            // The key's committed text, unless it's a shortcut: the focused node takes it (a
            // text field takes the typed key too, so its ancestors don't act on it).
            let text = event.text.as_ref().filter(|text| {
                pressed
                    && !state.modifiers.ctrl
                    && !state.modifiers.meta
                    && text.chars().all(|c| !c.is_control())
            });
            host.send_key_and_text(
                KeyEvent { key, kind, modifiers: state.modifiers },
                text.map(|text| text.as_str()),
            );
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
mod button_tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use compose_ui::{
        EventContext, IntSize, Modifier, ModifierNode, ModifierNodeElement, PointerEventPass,
        PointerEventType, PointerEvents, PointerInputModifierNode, UnitExt,
    };
    use winit::event::{DeviceId, WindowEvent};

    use super::*;

    type Heard = Rc<RefCell<Vec<(PointerEventType, PointerButtons, bool)>>>;

    struct Recorder(Heard);
    struct RecorderNode(Heard);

    impl ModifierNodeElement for Recorder {
        type Node = RecorderNode;
        fn create(&self) -> RecorderNode {
            RecorderNode(self.0.clone())
        }
        fn update(&self, _: &mut RecorderNode) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }

    impl ModifierNode for RecorderNode {
        fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
            Some(self)
        }
    }

    impl PointerInputModifierNode for RecorderNode {
        fn takes_whole_events(&self) -> bool {
            true
        }
        fn on_pointer_events(
            &mut self,
            event: &PointerEvents,
            pass: PointerEventPass,
            _: IntSize,
            _: &mut EventContext<'_>,
        ) {
            if pass == PointerEventPass::Main && event.kind != PointerEventType::Enter {
                let pressed = event.changes[0].pressed;
                self.0.borrow_mut().push((event.kind, event.buttons, pressed));
            }
        }
    }

    #[test]
    fn a_window_tells_each_button_and_is_down_until_the_last_is_up() {
        let heard = Heard::default();
        let seen = heard.clone();
        let mut host = UiHost::headless(100, 100);
        host.set_content(move || {
            compose_ui::Layout(
                Modifier.size(50.dp()).then_custom(Recorder(seen.clone())),
                compose_ui::empty_measure_policy(),
                compose_runtime::callback(|| {}),
            );
        });
        host.frame();
        let mut state = InputState::default();
        let device = DeviceId::dummy();
        let input = |button_state, button| WindowEvent::MouseInput {
            device_id: device,
            state: button_state,
            button,
        };
        let right = input(ElementState::Pressed, MouseButton::Right);
        let left = input(ElementState::Pressed, MouseButton::Left);
        let left_up = input(ElementState::Released, MouseButton::Left);
        let right_up = input(ElementState::Released, MouseButton::Right);
        for event in [&right, &left, &left_up, &right_up] {
            handle(&mut state, &mut host, event);
        }
        let both = PointerButtons(PointerButtons::PRIMARY.0 | PointerButtons::SECONDARY.0);
        assert_eq!(
            *heard.borrow(),
            [
                (PointerEventType::Press, PointerButtons::SECONDARY, true),
                (PointerEventType::Press, both, true),
                (PointerEventType::Release, PointerButtons::SECONDARY, true),
                (PointerEventType::Release, PointerButtons::NONE, false),
            ]
        );
    }

    #[test]
    fn buttons_have_their_bits_and_others_an_index_of_their_own() {
        assert_eq!(button_bit(MouseButton::Left), PointerButtons::PRIMARY.0);
        assert_eq!(button_bit(MouseButton::Middle), PointerButtons::TERTIARY.0);
        assert_eq!(button_bit(MouseButton::Forward), PointerButtons::FORWARD.0);
        assert!(PointerButtons(button_bit(MouseButton::Other(6))).is_pressed(6));
        // A small number is X11's for one of those it named: past them, not on them.
        assert!(PointerButtons(button_bit(MouseButton::Other(1))).is_pressed(5));
        assert!(!PointerButtons(button_bit(MouseButton::Other(1))).is_secondary_pressed());
    }
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

    // The step's numbers are unverified, from the JetBrains fork's source by memory (D1450): this
    // pins what's here, not what Compose Desktop does.
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
    fn a_lines_delta_runs_the_other_way_on_each_axis() {
        let event = wheel_event(Offset::ZERO, &MouseScrollDelta::LineDelta(2.0, -3.0));
        assert_eq!(event.scroll_delta, Offset::new(-2.0, 3.0));
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn nothing_is_precise_on_windows() {
        assert!(!is_precise_wheel(&MouseScrollDelta::LineDelta(0.0, 0.25)));
    }
}
