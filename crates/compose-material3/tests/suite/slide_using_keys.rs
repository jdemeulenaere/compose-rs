//! Compose's `SlideUsingKeysTest.kt`, adapted: the horizontal slider's keys, left to right and
//! right to left, with and without steps (Compose's tests use the deprecated value overload;
//! these, `Slider(state, …)`). Not ported: the 4 vertical slider tests (no `VerticalSlider`).

use std::cell::Cell;
use std::rc::Rc;

use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Key, LOCAL_LAYOUT_DIRECTION, LayoutDirection, Modifier};

/// A focusable slider of `state` in `direction`, telling `focused` when it has the focus.
fn screen(
    state: SliderState,
    direction: LayoutDirection,
    focused: Rc<Cell<bool>>,
) -> impl Fn() + 'static {
    move || {
        let focused = focused.clone();
        CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), || {
            let focused = focused.clone();
            Slider!(
                state,
                move |new| state.set_value(new),
                modifier = Modifier.on_focus_changed(
                    move |now: compose_ui::FocusState| focused.set(now.has_focus())
                ),
            );
        });
    }
}

fn round2(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}

/// Tabs to the slider, then checks each key moves the value as `expected` from its value.
fn check_keys(
    initial: f32,
    steps: u32,
    range: (f32, f32),
    direction: LayoutDirection,
    expected: &[(Key, f32)],
) {
    let state = SliderState::new(initial, steps, range);
    let focused = Rc::new(Cell::new(false));
    let mut ui = compose_test(screen(*state, direction, focused.clone()));
    ui.press_key(Key::Tab);
    assert!(focused.get(), "Tab focuses the slider");
    let mut current = initial;
    for &(key, delta) in expected {
        ui.press_key(key);
        current += delta;
        assert_eq!(round2(state.value()), round2(current), "after {key:?}");
    }
    ui.press_key(Key::MoveEnd);
    assert_eq!(state.value(), range.1);
    ui.press_key(Key::MoveHome);
    assert_eq!(state.value(), range.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SlideUsingKeysTest.kt, slider_ltr_0steps_change_using_keys
#[test]
fn keys_move_a_continuous_slider_by_a_hundredth_and_a_tenth() {
    let keys = [
        (Key::DirectionRight, 0.01),
        (Key::DirectionLeft, -0.01),
        (Key::PageDown, -0.1),
        (Key::PageUp, 0.1),
    ];
    check_keys(0.5, 0, (0.0, 1.0), LayoutDirection::Ltr, &keys);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SlideUsingKeysTest.kt, slider_rtl_0steps_change_using_keys
#[test]
fn right_to_left_the_arrows_go_the_other_way() {
    let keys = [
        (Key::DirectionRight, -0.01),
        (Key::DirectionLeft, 0.01),
        (Key::PageDown, -0.1),
        (Key::PageUp, 0.1),
    ];
    check_keys(0.5, 0, (0.0, 1.0), LayoutDirection::Rtl, &keys);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SlideUsingKeysTest.kt, slider_ltr_29steps_using_keys
#[test]
fn keys_move_a_stepped_slider_by_a_step_and_a_tenth_of_the_steps() {
    let keys = [
        (Key::DirectionRight, 1.0),
        (Key::DirectionLeft, -1.0),
        (Key::PageDown, -3.0),
        (Key::PageUp, 3.0),
    ];
    check_keys(15.0, 29, (0.0, 30.0), LayoutDirection::Ltr, &keys);
}
