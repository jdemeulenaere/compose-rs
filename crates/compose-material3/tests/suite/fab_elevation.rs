//! Compose's `FloatingActionButtonTest`'s elevation tests: the shadow's elevation follows what
//! the interaction source says, at the value given now.

use std::cell::Cell;

use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Dp, MutableInteractionSource, Offset, Press, PressInteraction, UnitExt};

thread_local! {
    static TONAL: Cell<Dp> = const { Cell::new(Dp(-1.0)) };
    static SHADOW: Cell<Dp> = const { Cell::new(Dp(-1.0)) };
}

/// A rule reading the elevations made of `default` and `pressed`.
fn rule(
    source: &MutableInteractionSource,
    default: MutableState<Dp>,
    pressed: MutableState<Dp>,
) -> ComposeTestRule {
    let source = source.clone();
    compose_test(move || {
        let elevation = FloatingActionButtonElevation {
            default_elevation: default.get(),
            pressed_elevation: pressed.get(),
            hovered_elevation: 3.dp(),
            focused_elevation: 4.dp(),
        };
        TONAL.with(|cell| cell.set(elevation.tonal_elevation()));
        SHADOW.with(|cell| cell.set(elevation.shadow_elevation(&source)));
    })
}

fn elevations() -> (Dp, Dp) {
    (TONAL.with(Cell::get), SHADOW.with(Cell::get))
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/FloatingActionButtonTest.kt, floatingActionButtonElevation_newInteraction
#[test]
fn a_press_raises_the_shadow_and_not_the_tonal_elevation() {
    let source = MutableInteractionSource::new();
    let (default, pressed) = (mutable_state_of(1.dp()), mutable_state_of(2.dp()));
    let mut ui = rule(&source, default.handle(), pressed.handle());
    assert_eq!(elevations(), (1.dp(), 1.dp()));
    source.try_emit(PressInteraction::Press(Press::new(Offset::ZERO)));
    ui.wait_for_idle();
    assert_eq!(elevations(), (1.dp(), 2.dp()));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/FloatingActionButtonTest.kt, floatingActionButtonElevation_newValue
#[test]
fn a_new_default_elevation_shows() {
    let source = MutableInteractionSource::new();
    let (default, pressed) = (mutable_state_of(1.dp()), mutable_state_of(2.dp()));
    let mut ui = rule(&source, default.handle(), pressed.handle());
    assert_eq!(elevations(), (1.dp(), 1.dp()));
    default.set(5.dp());
    ui.wait_for_idle();
    assert_eq!(elevations(), (5.dp(), 5.dp()));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/FloatingActionButtonTest.kt, floatingActionButtonElevation_newValueDuringInteraction
#[test]
fn a_new_pressed_elevation_shows_while_pressed() {
    let source = MutableInteractionSource::new();
    let (default, pressed) = (mutable_state_of(1.dp()), mutable_state_of(2.dp()));
    let mut ui = rule(&source, default.handle(), pressed.handle());
    source.try_emit(PressInteraction::Press(Press::new(Offset::ZERO)));
    ui.wait_for_idle();
    assert_eq!(elevations(), (1.dp(), 2.dp()));
    pressed.set(5.dp());
    ui.wait_for_idle();
    assert_eq!(elevations(), (1.dp(), 5.dp()));
}
