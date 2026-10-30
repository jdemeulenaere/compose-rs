//! Compose's `SwitchScreenshotTest.kt`, adapted: each golden's scenario checked as the display
//! list's shapes at density 1: the track's fill and outline, the thumb (where, how big, its
//! color), its state layer and its icon. The 52 × 32 dp track is 8 dp down its 48 dp touch
//! target.
//!
//! Not ported: `switchTest_focus_insetFocusRings` and `switchTest_focus_customInsetFocusRings`
//! (feature:ripple's inset focus rings).

use std::time::Duration;

use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::{
    Color, LOCAL_LAYOUT_DIRECTION, LayoutDirection, Modifier, Offset, PaintStyle, Rect,
};

/// A round rectangle drawn: where, filled (`stroke` 0) or stroked, its color.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Shape {
    rect: (f32, f32, f32, f32),
    stroke: f32,
    color: Color,
}

/// What a switch draws: its round rectangles and its icons' colors, in the window.
fn drawn(ui: &mut ComposeTestRule) -> (Vec<Shape>, Vec<Color>) {
    let mut stack = vec![Offset::ZERO];
    let (mut shapes, mut icons) = (Vec::new(), Vec::new());
    for op in &ui.host().display_list().ops {
        let at = *stack.last().expect("a translation");
        match op {
            DrawOp::Save | DrawOp::SaveLayerAlpha { .. } => stack.push(at),
            DrawOp::Restore => {
                stack.pop();
            }
            DrawOp::Translate(by) => *stack.last_mut().expect("a translation") = at + *by,
            DrawOp::DrawRoundRect(rect, _, paint) => shapes.push(Shape {
                rect: (rect.left + at.x, rect.top + at.y, rect.right + at.x, rect.bottom + at.y),
                stroke: match paint.style {
                    PaintStyle::Fill => 0.0,
                    PaintStyle::Stroke(width) => width,
                },
                color: paint.color,
            }),
            // The ripple's circle.
            DrawOp::DrawArc { oval, paint, .. } => shapes.push(Shape {
                rect: (oval.left + at.x, oval.top + at.y, oval.right + at.x, oval.bottom + at.y),
                stroke: 0.0,
                color: paint.color,
            }),
            DrawOp::DrawPath { paint, .. } => icons.push(paint.color),
            _ => {}
        }
    }
    (shapes, icons)
}

fn fill(rect: Rect, color: Color) -> Shape {
    Shape { rect: (rect.left, rect.top, rect.right, rect.bottom), stroke: 0.0, color }
}

/// The track, filled.
fn track(color: Color) -> Shape {
    fill(Rect::new(0.0, 8.0, 52.0, 40.0), color)
}

/// The track's 2 dp outline, 1 dp in (drawn over the thumb, as Compose's `border` draws over
/// its content).
fn outline(color: Color) -> Shape {
    Shape { rect: (1.0, 9.0, 51.0, 39.0), stroke: 2.0, color }
}

/// The thumb: `size` dp, `left` dp from the track's left, centered vertically.
fn thumb(left: f32, size: f32, color: Color) -> Shape {
    let top = 8.0 + (32.0 - size) / 2.0;
    fill(Rect::new(left, top, left + size, top + size), color)
}

fn switch(
    checked: bool,
    enabled: bool,
    direction: LayoutDirection,
    colors: Option<fn() -> SwitchColors>,
    icon: Option<fn()>,
) -> ComposeTestRule {
    ComposeTestRule::with_manual_clock(move || {
        MaterialTheme!({
            CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), || {
                let colors = colors.map_or_else(SwitchDefaults::colors, |colors| colors());
                match icon {
                    Some(icon) => Switch!(
                        checked,
                        Some(|_| {}),
                        enabled = enabled,
                        colors = colors,
                        thumb_content = Some(icon)
                    ),
                    None => Switch!(checked, Some(|_| {}), enabled = enabled, colors = colors),
                }
            });
        });
    })
}

fn plain(checked: bool) -> (Vec<Shape>, Vec<Color>) {
    drawn(&mut switch(checked, true, LayoutDirection::Ltr, None, None))
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchScreenshotTest.kt, switchTest_checked
#[test]
fn a_checked_switch_is_a_primary_track_and_a_24_dp_thumb_at_its_end() {
    let s = light_color_scheme();
    assert_eq!(plain(true).0, [track(s.primary), thumb(24.0, 24.0, s.on_primary)]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchScreenshotTest.kt, switchTest_checked_rtl
#[test]
fn right_to_left_a_checked_switchs_thumb_is_on_the_left() {
    let s = light_color_scheme();
    let drawn = drawn(&mut switch(true, true, LayoutDirection::Rtl, None, None));
    assert_eq!(drawn.0, [track(s.primary), thumb(4.0, 24.0, s.on_primary)]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchScreenshotTest.kt, switchTest_checked_customColor
#[test]
fn a_custom_thumb_color_draws_the_thumb() {
    let s = light_color_scheme();
    let red: fn() -> SwitchColors =
        || SwitchColors { checked_thumb_color: Color::RED, ..SwitchDefaults::colors() };
    let drawn = drawn(&mut switch(true, true, LayoutDirection::Ltr, Some(red), None));
    assert_eq!(drawn.0, [track(s.primary), thumb(24.0, 24.0, Color::RED)]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchScreenshotTest.kt, switchTest_unchecked
#[test]
fn an_unchecked_switch_is_an_outlined_track_and_a_16_dp_thumb_at_its_start() {
    let s = light_color_scheme();
    let shapes = plain(false).0;
    assert!(shapes.contains(&track(s.surface_container_highest)), "{shapes:?}");
    assert!(shapes.contains(&outline(s.outline)), "{shapes:?}");
    assert!(shapes.contains(&thumb(8.0, 16.0, s.outline)), "{shapes:?}");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchScreenshotTest.kt, switchTest_unchecked_rtl
#[test]
fn right_to_left_an_unchecked_switchs_thumb_is_on_the_right() {
    let s = light_color_scheme();
    let shapes = drawn(&mut switch(false, true, LayoutDirection::Rtl, None, None)).0;
    assert!(shapes.contains(&thumb(28.0, 16.0, s.outline)), "{shapes:?}");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchScreenshotTest.kt, switchTest_pressed
#[test]
fn a_pressed_switchs_thumb_grows_to_28_dp_with_its_state_layer() {
    let s = light_color_scheme();
    let mut ui = switch(false, true, LayoutDirection::Ltr, None, None);
    ui.on_node(is_toggleable()).perform_touch_input(|touch| {
        let center = touch.center();
        touch.down(center);
    });
    ui.advance_time_by(Duration::from_millis(300));
    let shapes = drawn(&mut ui).0;
    // 28 dp at the outline's width from the start, the state layer 40 dp around its center.
    assert!(shapes.contains(&thumb(2.0, 28.0, s.outline)), "{shapes:?}");
    let layer = shapes.iter().any(|shape| shape.rect == (-4.0, 4.0, 36.0, 44.0));
    assert!(layer, "{shapes:?}");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchScreenshotTest.kt, switchTest_disabled_checked
#[test]
fn a_disabled_checked_switch_is_faded() {
    let s = light_color_scheme();
    let shapes = drawn(&mut switch(true, false, LayoutDirection::Ltr, None, None)).0;
    let over = |color: Color, alpha: f32| color.with_alpha(alpha).composite_over(s.surface);
    assert_eq!(shapes, [track(over(s.on_surface, 0.12)), thumb(24.0, 24.0, s.surface)]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchScreenshotTest.kt, switchTest_disabled_unchecked
#[test]
fn a_disabled_unchecked_switch_is_faded() {
    let s = light_color_scheme();
    let shapes = drawn(&mut switch(false, false, LayoutDirection::Ltr, None, None)).0;
    let over = |color: Color, alpha: f32| color.with_alpha(alpha).composite_over(s.surface);
    assert!(shapes.contains(&track(over(s.surface_container_highest, 0.12))), "{shapes:?}");
    assert!(shapes.contains(&outline(over(s.on_surface, 0.12))), "{shapes:?}");
    assert!(shapes.contains(&thumb(8.0, 16.0, over(s.on_surface, 0.38))), "{shapes:?}");
}

/// A switch toggled by a click, `after` the click.
fn toggled(checked: bool, after: Duration) -> Vec<Shape> {
    let state = mutable_state_of(checked);
    let handle = state.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        MaterialTheme!({
            Switch!(handle.get(), Some(move |now| handle.set(now)));
        });
    });
    ui.on_node(is_toggleable()).perform_touch_input(|touch| touch.click());
    ui.advance_time_by(after);
    let shapes = drawn(&mut ui).0;
    drop(state);
    shapes
}

/// The last 16 to 24 dp square drawn: the thumb.
fn thumb_left(shapes: &[Shape]) -> f32 {
    shapes
        .iter()
        .rev()
        .find(|shape| {
            let width = shape.rect.2 - shape.rect.0;
            (16.0..=28.0).contains(&width) && shape.stroke == 0.0
        })
        .map(|shape| shape.rect.0)
        .expect("the thumb")
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchScreenshotTest.kt, switchTest_unchecked_animateToChecked
#[test]
fn a_checking_slides_the_thumb_toward_the_end() {
    let left = thumb_left(&toggled(false, Duration::from_millis(48)));
    assert!(left > 2.0 && left < 24.0, "part way: {left}");
    assert_eq!(thumb_left(&toggled(false, Duration::from_millis(1000))), 24.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchScreenshotTest.kt, switchTest_checked_animateToUnchecked
#[test]
fn an_unchecking_slides_the_thumb_toward_the_start() {
    let left = thumb_left(&toggled(true, Duration::from_millis(48)));
    assert!(left > 8.0 && left < 26.0, "part way: {left}");
    assert_eq!(thumb_left(&toggled(true, Duration::from_millis(1000))), 8.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchScreenshotTest.kt, switchTest_hover
#[test]
fn a_hovered_switch_shows_its_thumbs_state_layer() {
    let mut ui = switch(true, true, LayoutDirection::Ltr, None, None);
    ui.on_node(is_toggleable()).perform_mouse_input(|mouse| {
        let center = mouse.center();
        mouse.enter(center);
    });
    ui.advance_time_by(Duration::from_millis(100));
    let shapes = drawn(&mut ui).0;
    // Around the checked thumb's center (36, 24).
    assert!(shapes.iter().any(|shape| shape.rect == (16.0, 4.0, 56.0, 44.0)), "{shapes:?}");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchScreenshotTest.kt, switchTest_focus
#[test]
fn a_focused_switch_shows_its_thumbs_state_layer() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {
        MaterialTheme!({
            Switch!(true, Some(|_| {}), modifier = Modifier.test_tag("switch"));
        });
    });
    ui.on_node_with_tag("switch").request_focus();
    ui.advance_time_by(Duration::from_millis(100));
    let shapes = drawn(&mut ui).0;
    assert!(shapes.iter().any(|shape| shape.rect == (16.0, 4.0, 56.0, 44.0)), "{shapes:?}");
}

fn check_icon() {
    Icon!(icons::filled::check(), None, modifier = Modifier.size(SwitchDefaults::ICON_SIZE));
}

fn close_icon() {
    Icon!(icons::filled::close(), None, modifier = Modifier.size(SwitchDefaults::ICON_SIZE));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchScreenshotTest.kt, switchTest_checked_icon
#[test]
fn a_checked_switchs_icon_is_on_primary_container_in_its_thumb() {
    let s = light_color_scheme();
    let (shapes, icons) =
        drawn(&mut switch(true, true, LayoutDirection::Ltr, None, Some(check_icon)));
    assert!(shapes.contains(&thumb(24.0, 24.0, s.on_primary)), "{shapes:?}");
    assert_eq!(icons, [s.on_primary_container]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchScreenshotTest.kt, switchTest_unchecked_icon
#[test]
fn an_unchecked_switch_with_an_icon_has_a_24_dp_thumb() {
    let s = light_color_scheme();
    let (shapes, icons) =
        drawn(&mut switch(false, true, LayoutDirection::Ltr, None, Some(close_icon)));
    assert!(shapes.contains(&thumb(4.0, 24.0, s.outline)), "{shapes:?}");
    assert_eq!(icons, [s.surface_container_highest]);
}
