//! Compose's `DrawModifierTest` (ui, device tests, the one in `androidx.compose.ui.draw`): draw
//! modifiers on content movable content moves to another layout, or reused content composes
//! again, drawn where and as they are now; and a state written in `on_size_changed` drawing
//! once. Drawn by Skia at density 1.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose::foundation::layout::*;
use compose::runtime::*;
use compose::test::*;
use compose::ui::{Color, IntSize, Modifier, Paint, UnitExt};

fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    set_capture_renderer(compose_skia::raster_colors);
    ComposeTestRule::sized(IntSize::new(200, 200), content)
}

/// Fills its layout with `color`, as `drawBehind { drawRect(color) }`.
fn draw_behind(modifier: Modifier, color: Color) -> Modifier {
    modifier.draw_behind(move |scope| {
        let bounds = scope.bounds();
        scope.draw_rect(bounds, Paint::fill(color));
    })
}

/// A 40 pixel red box moved by movable content from (0, 0) to (60, 60) in a 100 pixel blue
/// container: red where it is, never where it was.
fn moved_between_layouts(fill: fn(Modifier, Color) -> Modifier) {
    let moved = mutable_state_of(false);
    let m = moved.handle();
    let content =
        movable_content_of(move || Box!(modifier = fill(Modifier.size(40.dp()), Color::RED)));
    let mut ui = rule(move || {
        let content = content.clone();
        Box!(modifier = fill(Modifier.size(100.dp()).test_tag("container"), Color::BLUE), {
            let content = content.clone();
            if m.get() {
                Box!(modifier = Modifier.offset(60.dp(), 60.dp()), { content() });
            } else {
                Box!(modifier = Modifier.offset(0.dp(), 0.dp()), { content() });
            }
        });
    });
    let red_in = |from: i32, to: i32| {
        move |at: compose::ui::IntOffset| {
            let inside = (from..to).contains(&at.x) && (from..to).contains(&at.y);
            Some(if inside { Color::RED } else { Color::BLUE })
        }
    };
    let image = ui.on_node_with_tag("container").capture_to_image();
    image.assert_pixels(Some(IntSize::new(100, 100)), red_in(0, 40));
    moved.set(true);
    ui.wait_for_idle();
    let image = ui.on_node_with_tag("container").capture_to_image();
    image.assert_pixels(Some(IntSize::new(100, 100)), red_in(60, 100));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DrawModifierTest.kt, testDrawBehindWhenMovedBetweenLayouts
#[test]
fn test_draw_behind_when_moved_between_layouts() {
    moved_between_layouts(draw_behind);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DrawModifierTest.kt, testBackgroundWhenMovedBetweenLayouts
#[test]
fn test_background_when_moved_between_layouts() {
    moved_between_layouts(|modifier, color| modifier.background(color));
}

/// A 50 pixel box in reusable content, red for key 0 and green after, drawn green once the key
/// changes and its node is reused.
fn reused(fill: fn(Modifier, Color) -> Modifier) {
    let key = mutable_state_of(0);
    let k = key.handle();
    let mut ui = rule(move || {
        ReusableContent(k.get(), || {
            let color = if k.get() == 0 { Color::RED } else { Color::GREEN };
            Box!(modifier = fill(Modifier.size(50.dp()).test_tag("container"), color));
        });
    });
    let image = ui.on_node_with_tag("container").capture_to_image();
    image.assert_pixels(Some(IntSize::new(50, 50)), |_| Some(Color::RED));
    key.set(1);
    ui.wait_for_idle();
    let image = ui.on_node_with_tag("container").capture_to_image();
    image.assert_pixels(Some(IntSize::new(50, 50)), |_| Some(Color::GREEN));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DrawModifierTest.kt, testDrawBehindWhenReused
#[test]
fn test_draw_behind_when_reused() {
    // Compose's lambda is made again in the reused content and reads the key while drawing, so
    // either the new element or the state read draws green: each is checked alone. First a new
    // element of the new color, reading nothing while drawing.
    reused(draw_behind);
    // Then the same element across the reuse, reading the key while drawing.
    let key = mutable_state_of(0);
    let (k1, k2) = (key.handle(), key.handle());
    let drawn = Modifier.size(50.dp()).test_tag("container").draw_behind(move |scope| {
        let color = if k1.get() == 0 { Color::RED } else { Color::GREEN };
        let bounds = scope.bounds();
        scope.draw_rect(bounds, Paint::fill(color));
    });
    let mut ui = rule(move || {
        let drawn = drawn.clone();
        ReusableContent(k2.get(), move || {
            Box!(modifier = drawn.clone());
        });
    });
    let image = ui.on_node_with_tag("container").capture_to_image();
    image.assert_pixels(Some(IntSize::new(50, 50)), |_| Some(Color::RED));
    key.set(1);
    ui.wait_for_idle();
    let image = ui.on_node_with_tag("container").capture_to_image();
    image.assert_pixels(Some(IntSize::new(50, 50)), |_| Some(Color::GREEN));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DrawModifierTest.kt, testBackgroundWhenReused
#[test]
fn test_background_when_reused() {
    reused(|modifier, color| modifier.background(color));
}

/// A state written in `on_size_changed` (before the first draw) and read in a draw: drawn once.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DrawModifierTest.kt, testInvalidationInsideOnSizeChanged
#[test]
fn test_invalidation_inside_on_size_changed() {
    let state = mutable_state_of(1);
    let draws = Rc::new(Cell::new(0));
    let (s, d) = (state.handle(), draws.clone());
    let mut ui = rule(move || {
        let (d, seen) = (d.clone(), d.clone());
        let modifier = Modifier
            .draw_behind(move |_| {
                s.get();
                d.set(d.get() + 1);
            })
            .on_size_changed(move |_: IntSize| {
                assert_eq!(seen.get(), 0, "not drawn yet");
                s.set(s.get() + 1);
            })
            .size(10.dp());
        Box!(modifier = modifier);
    });
    ui.wait_for_idle();
    assert_eq!(state.get(), 2, "on_size_changed ran once");
    assert_eq!(draws.get(), 1);
}
