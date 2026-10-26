//! Compose's `SliderScreenshotTest.kt`, adapted: each golden's scenario for the horizontal
//! slider checked as the display list's shapes (positions in the slider, corners, colors) at
//! density 1. The slider is 200 dp wide, as the goldens' wrapper: 48 dp high (the touch
//! target), its 4 by 44 dp handle 2 dp down, its 16 dp track from 2 to 198 and from 16 to 32,
//! the track's corners 8 dp (2 dp by the handle), the gap on each side of the handle 6 dp past
//! its half (8 dp from its center).
//!
//! Not ported: `slider_focused_insetFocusRings` (no inset focus ring),
//! `sliderTest_withSteps_rtl_lookaheadScope` (no `LookaheadScope`),
//! `sliderTest_middle_custom_corners_track_icons` (icons drawn over the track by a draw
//! modifier: covered by the corners of `a_track_with_its_own_corners…`), and the vertical,
//! centered and range sliders' goldens (not in compose-rs yet).

use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::{Color, Dp, IntSize, LOCAL_LAYOUT_DIRECTION, LayoutDirection, Modifier, Offset};

/// What a slider draws, in the slider's coordinates, rounded to a hundredth.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Drawn {
    /// A part of the track or the handle: its left and right, top and bottom, and its corners
    /// (top left, top right, bottom right, bottom left).
    Part { left: f32, right: f32, top: f32, bottom: f32, radii: [f32; 4], color: Color },
    /// A stop dot or a tick.
    Dot { x: f32, y: f32, radius: f32, color: Color },
}

fn round(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}

/// The shapes drawn, moved by the translations around them.
fn drawn(ui: &mut ComposeTestRule) -> Vec<Drawn> {
    let mut stack = vec![Offset::ZERO];
    let mut out = Vec::new();
    for op in &ui.host().display_list().ops {
        let at = *stack.last().expect("a translation");
        match op {
            DrawOp::Save | DrawOp::SaveLayerAlpha { .. } => stack.push(at),
            DrawOp::Restore => {
                stack.pop();
            }
            DrawOp::Translate(by) => *stack.last_mut().expect("a translation") = at + *by,
            DrawOp::DrawRoundRect(rect, radii, paint) => out.push(Drawn::Part {
                left: round(rect.left + at.x),
                right: round(rect.right + at.x),
                top: round(rect.top + at.y),
                bottom: round(rect.bottom + at.y),
                radii: radii.map(round),
                color: paint.color,
            }),
            DrawOp::DrawArc { oval, paint, .. } => out.push(Drawn::Dot {
                x: round((oval.left + oval.right) / 2.0 + at.x),
                y: round((oval.top + oval.bottom) / 2.0 + at.y),
                radius: round(oval.width() / 2.0),
                color: paint.color,
            }),
            _ => {}
        }
    }
    out
}

/// A 200 dp slider at `value` with `steps`, in `scheme` and `direction`, `track` drawing it if
/// given.
fn slider(
    value: f32,
    steps: u32,
    scheme: ColorScheme,
    direction: LayoutDirection,
    enabled: bool,
    colors: Option<fn() -> SliderColors>,
    track: Option<fn(SliderState)>,
) -> Vec<Drawn> {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 48), move || {
        MaterialTheme!(color_scheme = scheme, {
            CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), || {
                let state = remember_slider_state!(value, steps);
                let colors = colors.map_or_else(SliderDefaults::colors, |colors| colors());
                match track {
                    Some(track) => Slider!(
                        state,
                        |_| {},
                        enabled = enabled,
                        colors = colors,
                        track = Some(track)
                    ),
                    None => Slider!(state, |_| {}, enabled = enabled, colors = colors),
                }
            });
        });
    });
    drawn(&mut ui)
}

fn plain(value: f32, steps: u32) -> Vec<Drawn> {
    slider(value, steps, light_color_scheme(), LayoutDirection::Ltr, true, None, None)
}

/// A track part from `left` to `right`, its left corners `left_radius`, its right ones
/// `right_radius`.
fn part(left: f32, right: f32, left_radius: f32, right_radius: f32, color: Color) -> Drawn {
    Drawn::Part {
        left,
        right,
        top: 16.0,
        bottom: 32.0,
        radii: [left_radius, right_radius, right_radius, left_radius],
        color,
    }
}

/// The handle, from `left`.
fn handle(left: f32, color: Color) -> Drawn {
    Drawn::Part { left, right: left + 4.0, top: 2.0, bottom: 46.0, radii: [2.0; 4], color }
}

fn dot(x: f32, radius: f32, color: Color) -> Drawn {
    Drawn::Dot { x, y: 24.0, radius, color }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, sliderTest_origin
#[test]
fn at_the_start_the_inactive_track_runs_from_the_gap_to_the_stop_dot() {
    let scheme = light_color_scheme();
    assert_eq!(
        plain(0.0, 0),
        [
            part(10.0, 198.0, 2.0, 8.0, scheme.secondary_container),
            dot(190.0, 2.0, scheme.primary),
            handle(0.0, scheme.primary),
        ]
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, sliderTest_origin_rtl
#[test]
fn right_to_left_at_the_start_everything_is_mirrored() {
    let scheme = light_color_scheme();
    let drawn = slider(0.0, 0, scheme, LayoutDirection::Rtl, true, None, None);
    assert_eq!(
        drawn,
        [
            part(2.0, 190.0, 8.0, 2.0, scheme.secondary_container),
            dot(10.0, 2.0, scheme.primary),
            handle(196.0, scheme.primary),
        ]
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, sliderTest_withSteps_rtl
#[test]
fn right_to_left_the_ticks_are_mirrored() {
    let scheme = light_color_scheme();
    let drawn = slider(0.2, 4, scheme, LayoutDirection::Rtl, true, None, None);
    // At 20 % of 4 steps (stops every 36 dp from 8 to 188, mirrored): the handle at 46 from
    // the right, the active track to 8 dp before it, the inactive from 8 dp after it.
    assert_eq!(
        drawn,
        [
            part(2.0, 146.0, 8.0, 2.0, scheme.secondary_container),
            dot(10.0, 2.0, scheme.primary),
            part(162.0, 198.0, 2.0, 8.0, scheme.primary),
            dot(190.0, 2.0, scheme.secondary_container),
            dot(118.0, 2.0, scheme.primary),
            dot(82.0, 2.0, scheme.primary),
            dot(46.0, 2.0, scheme.primary),
            handle(152.0, scheme.primary),
        ]
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, sliderTest_origin_disabled
#[test]
fn a_disabled_slider_at_the_start_takes_the_disabled_colors() {
    let scheme = light_color_scheme();
    let drawn = slider(0.0, 0, scheme, LayoutDirection::Ltr, false, None, None);
    let on_surface = scheme.on_surface;
    assert_eq!(
        drawn,
        [
            part(10.0, 198.0, 2.0, 8.0, on_surface.with_alpha(0.12)),
            dot(190.0, 2.0, on_surface.with_alpha(0.38)),
            handle(0.0, on_surface.with_alpha(0.38).composite_over(scheme.surface)),
        ]
    );
}

/// The middle (half of 196 dp, at 100): the active track to 92, the inactive from 108.
fn middle(scheme: ColorScheme, enabled: bool) -> [Drawn; 4] {
    let colors = if enabled {
        (scheme.secondary_container, scheme.primary, scheme.primary)
    } else {
        let on_surface = scheme.on_surface;
        (
            on_surface.with_alpha(0.12),
            on_surface.with_alpha(0.38),
            on_surface.with_alpha(0.38).composite_over(scheme.surface),
        )
    };
    [
        part(108.0, 198.0, 2.0, 8.0, colors.0),
        dot(190.0, 2.0, colors.1),
        part(2.0, 92.0, 8.0, 2.0, colors.1),
        handle(98.0, colors.2),
    ]
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, sliderTest_middle
#[test]
fn in_the_middle_the_track_splits_around_the_handle() {
    assert_eq!(plain(0.5, 0), middle(light_color_scheme(), true));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, sliderTest_middle_dark
#[test]
fn a_dark_slider_takes_the_dark_colors() {
    let scheme = dark_color_scheme();
    let drawn = slider(0.5, 0, scheme, LayoutDirection::Ltr, true, None, None);
    assert_eq!(drawn, middle(scheme, true));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, sliderTest_middle_dark_disabled
#[test]
fn a_dark_disabled_slider_takes_the_dark_disabled_colors() {
    let scheme = dark_color_scheme();
    let drawn = slider(0.5, 0, scheme, LayoutDirection::Ltr, false, None, None);
    assert_eq!(drawn, middle(scheme, false));
}

/// The default track with `gap`, `inside` corners and the stop dot or not.
fn custom_track(state: SliderState, gap: f32, inside: f32, stop_dot: bool) {
    let colors = SliderDefaults::colors();
    SliderDefaults::track_with(
        state,
        Modifier,
        true,
        colors,
        stop_dot.then(|| SliderDefaults::default_draw_stop_indicator(true, colors)),
        SliderDefaults::default_draw_tick(),
        Dp(gap),
        Dp(inside),
        None,
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, sliderTest_middle_no_gap
#[test]
fn without_a_gap_the_track_parts_meet_at_the_handle() {
    let scheme = light_color_scheme();
    let track: fn(SliderState) = |state| custom_track(state, 0.0, 2.0, true);
    let drawn = slider(0.5, 0, scheme, LayoutDirection::Ltr, true, None, Some(track));
    assert_eq!(
        drawn,
        [
            part(100.0, 198.0, 2.0, 8.0, scheme.secondary_container),
            dot(190.0, 2.0, scheme.primary),
            part(2.0, 100.0, 8.0, 2.0, scheme.primary),
            handle(98.0, scheme.primary),
        ]
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, sliderTest_middle_no_inside_corner
#[test]
fn without_inside_corners_the_track_parts_are_square_by_the_handle() {
    let scheme = light_color_scheme();
    let track: fn(SliderState) = |state| custom_track(state, 6.0, 0.0, true);
    let drawn = slider(0.5, 0, scheme, LayoutDirection::Ltr, true, None, Some(track));
    assert_eq!(
        drawn,
        [
            part(108.0, 198.0, 0.0, 8.0, scheme.secondary_container),
            dot(190.0, 2.0, scheme.primary),
            part(2.0, 92.0, 8.0, 0.0, scheme.primary),
            handle(98.0, scheme.primary),
        ]
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, sliderTest_middle_no_stop_indicator
#[test]
fn without_a_stop_indicator_no_dot_is_drawn() {
    let scheme = light_color_scheme();
    let track: fn(SliderState) = |state| custom_track(state, 6.0, 2.0, false);
    let drawn = slider(0.5, 0, scheme, LayoutDirection::Ltr, true, None, Some(track));
    assert_eq!(
        drawn,
        [
            part(108.0, 198.0, 2.0, 8.0, scheme.secondary_container),
            part(2.0, 92.0, 8.0, 2.0, scheme.primary),
            handle(98.0, scheme.primary),
        ]
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, sliderTest_end
#[test]
fn at_the_end_only_the_active_track_is_drawn() {
    let scheme = light_color_scheme();
    assert_eq!(
        plain(1.0, 0),
        [part(2.0, 190.0, 8.0, 2.0, scheme.primary), handle(196.0, scheme.primary)]
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, sliderTest_end_rtl
#[test]
fn right_to_left_at_the_end_the_active_track_is_on_the_left() {
    let scheme = light_color_scheme();
    let drawn = slider(1.0, 0, scheme, LayoutDirection::Rtl, true, None, None);
    assert_eq!(drawn, [part(10.0, 198.0, 2.0, 8.0, scheme.primary), handle(0.0, scheme.primary)]);
}

/// Half of 5 steps (stops every 30 dp from 10 to 190): the track as in the middle, the ticks
/// before the gap in the active tick color, those after it in the inactive one, none in the
/// gap (at 100) nor at the stop dot (at 190).
fn middle_steps(colors: SliderColors, enabled: bool) -> [Drawn; 9] {
    let track = |active| colors.track_color(enabled, active);
    let tick = |active| colors.tick_color(enabled, active);
    [
        part(108.0, 198.0, 2.0, 8.0, track(false)),
        dot(190.0, 2.0, track(true)),
        part(2.0, 92.0, 8.0, 2.0, track(true)),
        dot(10.0, 2.0, tick(true)),
        dot(40.0, 2.0, tick(true)),
        dot(70.0, 2.0, tick(true)),
        dot(130.0, 2.0, tick(false)),
        dot(160.0, 2.0, tick(false)),
        handle(98.0, colors.thumb_color(enabled)),
    ]
}

/// `SliderDefaults::colors()` in `scheme`.
fn scheme_colors(scheme: ColorScheme) -> SliderColors {
    let on_surface = scheme.on_surface;
    SliderColors {
        thumb_color: scheme.primary,
        active_track_color: scheme.primary,
        active_tick_color: scheme.secondary_container,
        inactive_track_color: scheme.secondary_container,
        inactive_tick_color: scheme.primary,
        disabled_thumb_color: on_surface.with_alpha(0.38).composite_over(scheme.surface),
        disabled_active_track_color: on_surface.with_alpha(0.38),
        disabled_active_tick_color: on_surface.with_alpha(0.12),
        disabled_inactive_track_color: on_surface.with_alpha(0.12),
        disabled_inactive_tick_color: on_surface.with_alpha(0.38),
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, sliderTest_middle_steps
#[test]
fn steps_draw_ticks_but_in_the_gap_and_at_the_stop_dot() {
    let scheme = light_color_scheme();
    assert_eq!(plain(0.5, 5), middle_steps(scheme_colors(scheme), true));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, sliderTest_middle_steps_dark
#[test]
fn dark_steps_take_the_dark_colors() {
    let scheme = dark_color_scheme();
    let drawn = slider(0.5, 5, scheme, LayoutDirection::Ltr, true, None, None);
    assert_eq!(drawn, middle_steps(scheme_colors(scheme), true));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, sliderTest_middle_steps_disabled
#[test]
fn disabled_steps_take_the_disabled_colors() {
    let scheme = light_color_scheme();
    let drawn = slider(0.5, 5, scheme, LayoutDirection::Ltr, false, None, None);
    assert_eq!(drawn, middle_steps(scheme_colors(scheme), false));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, sliderTest_first_steps
#[test]
fn at_the_first_of_9_steps_one_tick_is_active() {
    let scheme = light_color_scheme();
    // Stops every 18 dp from 10 to 190; the handle on the second, at 28.
    let mut expected = vec![
        part(36.0, 198.0, 2.0, 8.0, scheme.secondary_container),
        dot(190.0, 2.0, scheme.primary),
        part(2.0, 20.0, 8.0, 2.0, scheme.primary),
        dot(10.0, 2.0, scheme.secondary_container),
    ];
    expected.extend((2..10).map(|stop| dot(10.0 + 18.0 * stop as f32, 2.0, scheme.primary)));
    expected.push(handle(26.0, scheme.primary));
    assert_eq!(plain(0.1, 9), expected);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, sliderTest_last_steps
#[test]
fn at_the_last_of_9_steps_the_ticks_before_are_active() {
    let scheme = light_color_scheme();
    // The handle on the ninth stop, at 172: 9 active ticks before the gap, none after.
    let mut expected = vec![
        part(180.0, 198.0, 2.0, 8.0, scheme.secondary_container),
        dot(190.0, 2.0, scheme.primary),
        part(2.0, 164.0, 8.0, 2.0, scheme.primary),
    ];
    expected
        .extend((0..9).map(|stop| dot(10.0 + 18.0 * stop as f32, 2.0, scheme.secondary_container)));
    expected.push(handle(170.0, scheme.primary));
    assert_eq!(plain(0.9, 9), expected);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, sliderTest_middle_steps_custom_ticks
#[test]
fn custom_ticks_draw_instead_of_the_dots() {
    let scheme = light_color_scheme();
    let track: fn(SliderState) = |state| {
        let colors = SliderDefaults::colors();
        SliderDefaults::track_with(
            state,
            Modifier,
            true,
            colors,
            Some(SliderDefaults::default_draw_stop_indicator(true, colors)),
            DrawSliderTick::new(|scope, offset, _| {
                let radius = scope.density().to_px(SliderDefaults::TICK_SIZE) / 4.0;
                scope.draw_circle(offset, radius, compose_ui::Paint::fill(Color::RED));
            }),
            Dp(6.0),
            Dp(2.0),
            None,
        );
    };
    let drawn = slider(0.5, 5, scheme, LayoutDirection::Ltr, true, None, Some(track));
    let red = |x| dot(x, 1.0, Color::RED);
    assert_eq!(
        drawn,
        [
            part(108.0, 198.0, 2.0, 8.0, scheme.secondary_container),
            dot(190.0, 2.0, scheme.primary),
            part(2.0, 92.0, 8.0, 2.0, scheme.primary),
            red(10.0),
            red(40.0),
            red(70.0),
            red(130.0),
            red(160.0),
            handle(98.0, scheme.primary),
        ]
    );
}

fn custom_colors() -> SliderColors {
    SliderColors {
        thumb_color: Color::RED,
        active_track_color: Color::BLUE,
        active_tick_color: Color::YELLOW,
        inactive_tick_color: Color::MAGENTA,
        ..SliderDefaults::colors()
    }
}

fn custom_disabled_colors() -> SliderColors {
    SliderColors {
        disabled_thumb_color: Color::BLUE,
        disabled_active_track_color: Color::RED,
        disabled_inactive_track_color: Color::YELLOW,
        disabled_active_tick_color: Color::MAGENTA,
        disabled_inactive_tick_color: Color::CYAN,
        ..SliderDefaults::colors()
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, sliderTest_customColors
#[test]
fn custom_colors_draw_the_handle_track_and_ticks() {
    let scheme = light_color_scheme();
    let colors = SliderColors {
        thumb_color: Color::RED,
        active_track_color: Color::BLUE,
        active_tick_color: Color::YELLOW,
        inactive_tick_color: Color::MAGENTA,
        ..scheme_colors(scheme)
    };
    let drawn = slider(0.5, 5, scheme, LayoutDirection::Ltr, true, Some(custom_colors), None);
    assert_eq!(drawn, middle_steps(colors, true));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, sliderTest_customColors_disabled
#[test]
fn custom_disabled_colors_draw_a_disabled_slider() {
    let scheme = light_color_scheme();
    let colors = SliderColors {
        disabled_thumb_color: Color::BLUE,
        disabled_active_track_color: Color::RED,
        disabled_inactive_track_color: Color::YELLOW,
        disabled_active_tick_color: Color::MAGENTA,
        disabled_inactive_tick_color: Color::CYAN,
        ..scheme_colors(scheme)
    };
    let drawn =
        slider(0.5, 5, scheme, LayoutDirection::Ltr, false, Some(custom_disabled_colors), None);
    assert_eq!(drawn, middle_steps(colors, false));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, sliderTest_min_corner
#[test]
fn near_the_end_the_inactive_track_is_shorter_than_its_corners() {
    let scheme = light_color_scheme();
    // At 91 % (178.36 dp along the track): the inactive part, 9.64 dp, under its 10 dp of
    // corners (the renderer shrinks them to fit, as Compose's `RoundRect`).
    assert_eq!(
        plain(0.91, 0),
        [
            part(188.36, 198.0, 2.0, 8.0, scheme.secondary_container),
            dot(190.0, 2.0, scheme.primary),
            part(2.0, 172.36, 8.0, 2.0, scheme.primary),
            handle(178.0, scheme.primary),
        ]
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, sliderTest_middle_custom_corners_track_icons
#[test]
fn a_track_with_its_own_corners_and_height_draws_them() {
    let scheme = light_color_scheme();
    // The golden's track without its icons: 36 dp high, 12 dp corners, no stop dot.
    let track: fn(SliderState) = |state| {
        let colors = SliderDefaults::colors();
        SliderDefaults::track_with(
            state,
            Modifier.height(Dp(36.0)),
            true,
            colors,
            None,
            SliderDefaults::default_draw_tick(),
            Dp(6.0),
            Dp(2.0),
            Some(Dp(12.0)),
        );
    };
    let drawn = slider(0.5, 0, scheme, LayoutDirection::Ltr, true, None, Some(track));
    let part = |left, right, radii: [f32; 4], color| Drawn::Part {
        left,
        right,
        top: 6.0,
        bottom: 42.0,
        radii,
        color,
    };
    assert_eq!(
        drawn,
        [
            part(108.0, 198.0, [2.0, 12.0, 12.0, 2.0], scheme.secondary_container),
            part(2.0, 92.0, [12.0, 2.0, 2.0, 12.0], scheme.primary),
            handle(98.0, scheme.primary),
        ]
    );
}
