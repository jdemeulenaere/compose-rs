//! Compose's `ProgressIndicatorScreenshotTest.kt`, adapted: each golden's scenario checked as
//! the display list's strokes and dots (positions, angles, colors, caps) at density 1.

use std::time::Duration;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::{
    Color, LOCAL_LAYOUT_DIRECTION, LayoutDirection, Modifier, PaintStyle, StrokeCap, UnitExt,
};

/// What an indicator draws, rounded to a hundredth.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Drawn {
    /// A horizontal stroke from `from` to `to` (x), `width` thick.
    Line { from: f32, to: f32, width: f32, color: Color, cap: StrokeCap },
    /// A stroked arc.
    Arc { start: f32, sweep: f32, width: f32, color: Color, cap: StrokeCap },
    /// A filled circle.
    Dot { x: f32, y: f32, radius: f32, color: Color },
    /// A filled rectangle.
    Square { left: f32, top: f32, right: f32, bottom: f32, color: Color },
}

fn round(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}

fn drawn(ui: &mut ComposeTestRule) -> Vec<Drawn> {
    ui.host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawLine(from, to, paint) => Some(Drawn::Line {
                from: round(from.x),
                to: round(to.x),
                width: match paint.style {
                    PaintStyle::Stroke(width) => width,
                    PaintStyle::Fill => 0.0,
                },
                color: paint.color,
                cap: paint.cap,
            }),
            DrawOp::DrawArc { oval, start_angle, sweep_angle, paint, .. } => match paint.style {
                PaintStyle::Stroke(width) => Some(Drawn::Arc {
                    start: round(*start_angle),
                    sweep: round(*sweep_angle),
                    width,
                    color: paint.color,
                    cap: paint.cap,
                }),
                PaintStyle::Fill => Some(Drawn::Dot {
                    x: round((oval.left + oval.right) / 2.0),
                    y: round((oval.top + oval.bottom) / 2.0),
                    radius: round(oval.width() / 2.0),
                    color: paint.color,
                }),
            },
            DrawOp::DrawRect(rect, paint) => Some(Drawn::Square {
                left: rect.left,
                top: rect.top,
                right: rect.right,
                bottom: rect.bottom,
                color: paint.color,
            }),
            _ => None,
        })
        .collect()
}

/// The indicator in `scheme`, laid out from the window's start, after `time` of its
/// animations.
fn indicator(
    scheme: ColorScheme,
    direction: LayoutDirection,
    time: Duration,
    content: impl Fn() + 'static,
) -> Vec<Drawn> {
    let content = std::rc::Rc::new(content);
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let content = content.clone();
        MaterialTheme!(color_scheme = scheme, {
            let content = content.clone();
            CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), move || {
                let content = content.clone();
                Box!({ content() });
            });
        });
    });
    ui.advance_time_by(time);
    drawn(&mut ui)
}

fn light(content: impl Fn() + 'static) -> Vec<Drawn> {
    indicator(light_color_scheme(), LayoutDirection::Ltr, Duration::ZERO, content)
}

const ROUND: StrokeCap = StrokeCap::Round;

fn line(from: f32, to: f32, color: Color) -> Drawn {
    Drawn::Line { from, to, width: 4.0, color, cap: ROUND }
}

/// The circular track's gap: `gap` dp (and the 4 dp stroke for round caps) of a `diameter` dp
/// circle's circumference, in degrees.
fn gap_sweep(gap: f32, diameter: f32) -> f32 {
    gap / (std::f32::consts::PI * diameter) * 360.0
}

fn arc(start: f32, sweep: f32, color: Color, cap: StrokeCap) -> Drawn {
    Drawn::Arc { start: round(start), sweep: round(sweep), width: 4.0, color, cap }
}

// ---- Linear ------------------------------------------------------------------------------------

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorScreenshotTest.kt, linearProgressIndicator_lightTheme_determinate
#[test]
fn a_linear_indicator_draws_its_line_its_track_past_the_gap_and_its_stop_dot() {
    let scheme = light_color_scheme();
    // The track from half way past the 4 dp gap and the 4 dp stroke (128) to the end, its cap
    // inside (238); the line from the start (its cap inside: 2) to half way; the 4 dp stop dot
    // at the end.
    assert_eq!(
        light(|| LinearProgressIndicator!(|| 0.5)),
        vec![
            line(128.0, 238.0, scheme.secondary_container),
            line(2.0, 120.0, scheme.primary),
            Drawn::Dot { x: 238.0, y: 2.0, radius: 2.0, color: scheme.primary },
        ]
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorScreenshotTest.kt, linearProgressIndicator_lightTheme_determinate_no_gap
#[test]
fn a_linear_indicator_without_a_gap_keeps_its_caps_apart() {
    let scheme = light_color_scheme();
    assert_eq!(
        light(|| LinearProgressIndicator!(|| 0.5, gap_size = 0.dp())),
        vec![
            line(124.0, 238.0, scheme.secondary_container),
            line(2.0, 120.0, scheme.primary),
            Drawn::Dot { x: 238.0, y: 2.0, radius: 2.0, color: scheme.primary },
        ]
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorScreenshotTest.kt, linearProgressIndicator_lightTheme_determinate_no_stop
#[test]
fn a_linear_indicator_can_leave_its_stop_dot_out() {
    let scheme = light_color_scheme();
    assert_eq!(
        light(|| LinearProgressIndicator!(
            || 0.5,
            draw_stop_indicator = Some(DrawStopIndicator::none())
        )),
        vec![line(128.0, 238.0, scheme.secondary_container), line(2.0, 120.0, scheme.primary)]
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorScreenshotTest.kt, linearProgressIndicator_lightTheme_determinate_stop_offset
#[test]
fn a_thick_linear_indicators_stop_dot_keeps_from_the_end() {
    let scheme = light_color_scheme();
    let found = light(|| {
        LinearProgressIndicator!(|| 0.5, modifier = Modifier.size((240.dp(), 16.dp())));
    });
    // A 4 dp dot in the 16 dp track: 6 dp from the end (the most), centered.
    assert_eq!(
        found.last(),
        Some(&Drawn::Dot { x: 232.0, y: 8.0, radius: 2.0, color: scheme.primary })
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorScreenshotTest.kt, linearProgressIndicator_lightTheme_indeterminate_start
#[test]
fn an_indeterminate_linear_indicator_starts_as_its_track_alone() {
    let scheme = light_color_scheme();
    assert_eq!(
        light(|| IndeterminateLinearProgressIndicator!()),
        vec![line(2.0, 238.0, scheme.secondary_container)]
    );
}

/// An infinite animation starts at the first frame after it's composed (a frame, 16 ms, after
/// the rule's start), as Compose's, and frames come every 16 ms; `advance_time_by` rounds up to
/// whole frames, as Compose's `MainTestClock` (D900): its play time `advance` after the start.
fn play_time(advance: f32) -> f32 {
    (advance / 16.0).ceil() * 16.0 - 16.0
}

/// The indeterminate linear indicator's lines 875 ms after the start (about half way through
/// its cycle): the first line from its tail to its head, the second just started; the tracks
/// around them past the gaps.
fn halfway(gap: f32) -> Vec<Drawn> {
    let scheme = light_color_scheme();
    let easing = compose_animation::Easing::CubicBezier(0.3, 0.0, 0.8, 0.15);
    let time = play_time(875.0);
    let at =
        |delay: f32, duration: f32| easing.transform(((time - delay) / duration).clamp(0.0, 1.0));
    let (head1, tail1) = (at(0.0, 1000.0), at(250.0, 1000.0));
    let (head2, tail2) = (at(650.0, 850.0), at(900.0, 850.0));
    assert!(head2 > 0.0 && tail2 == 0.0);
    let fraction = (gap + 4.0) / 240.0;
    let px = |fraction: f32| round((fraction * 240.0).clamp(2.0, 238.0));
    let mut expected = Vec::new();
    if head1 < 1.0 - fraction {
        expected.push(line(px(head1 + fraction), px(1.0), scheme.secondary_container));
    }
    expected.push(line(px(head1), px(tail1), scheme.primary));
    if tail1 > fraction {
        expected.push(line(px(head2 + fraction), px(tail1 - fraction), scheme.secondary_container));
    }
    expected.push(line(px(head2), px(tail2), scheme.primary));
    expected
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorScreenshotTest.kt, linearProgressIndicator_lightTheme_indeterminate
#[test]
fn an_indeterminate_linear_indicator_half_way_draws_two_lines_between_tracks() {
    let found = indicator(
        light_color_scheme(),
        LayoutDirection::Ltr,
        Duration::from_millis(875),
        || IndeterminateLinearProgressIndicator!(),
    );
    assert_eq!(found, halfway(4.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorScreenshotTest.kt, linearProgressIndicator_lightTheme_indeterminate_no_gap
#[test]
fn an_indeterminate_linear_indicator_without_a_gap_keeps_its_caps_apart() {
    let found =
        indicator(light_color_scheme(), LayoutDirection::Ltr, Duration::from_millis(875), || {
            IndeterminateLinearProgressIndicator!(gap_size = 0.dp())
        });
    assert_eq!(found, halfway(0.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorScreenshotTest.kt, linearProgressIndicator_darkTheme_determinate
#[test]
fn a_dark_linear_indicator_takes_the_dark_colors() {
    let scheme = dark_color_scheme();
    let found = indicator(scheme, LayoutDirection::Ltr, Duration::ZERO, || {
        LinearProgressIndicator!(|| 0.5)
    });
    assert_eq!(
        found,
        vec![
            line(128.0, 238.0, scheme.secondary_container),
            line(2.0, 120.0, scheme.primary),
            Drawn::Dot { x: 238.0, y: 2.0, radius: 2.0, color: scheme.primary },
        ]
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorScreenshotTest.kt, linearProgressIndicator_lightTheme_determinate_customCap
#[test]
fn a_butt_capped_linear_indicator_runs_edge_to_edge_with_a_square_stop() {
    let scheme = light_color_scheme();
    let butt = |from, to, color| Drawn::Line { from, to, width: 4.0, color, cap: StrokeCap::Butt };
    assert_eq!(
        light(|| LinearProgressIndicator!(|| 0.5, stroke_cap = StrokeCap::Butt)),
        vec![
            butt(124.0, 240.0, scheme.secondary_container),
            butt(0.0, 120.0, scheme.primary),
            Drawn::Square {
                left: 236.0,
                top: 0.0,
                right: 240.0,
                bottom: 4.0,
                color: scheme.primary
            },
        ]
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorScreenshotTest.kt, linearProgressIndicator_lightTheme_determinate_rtl
#[test]
fn right_to_left_a_linear_indicator_runs_from_the_right() {
    let scheme = light_color_scheme();
    let found = indicator(scheme, LayoutDirection::Rtl, Duration::ZERO, || {
        LinearProgressIndicator!(|| 0.5);
    });
    assert_eq!(
        found,
        vec![
            line(2.0, 112.0, scheme.secondary_container),
            line(120.0, 238.0, scheme.primary),
            Drawn::Dot { x: 2.0, y: 2.0, radius: 2.0, color: scheme.primary },
        ]
    );
}

// ---- Circular ----------------------------------------------------------------------------------

/// A determinate circular indicator at half, `diameter` dp, its track past `gap` dp (and the
/// stroke for round caps).
fn half_circle(scheme: &ColorScheme, gap: f32, diameter: f32) -> Vec<Drawn> {
    let gap = gap_sweep(gap + 4.0, diameter);
    vec![
        arc(270.0 + 180.0 + gap, 180.0 - 2.0 * gap, scheme.secondary_container, ROUND),
        arc(270.0, 180.0, scheme.primary, ROUND),
    ]
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorScreenshotTest.kt, circularProgressIndicator_lightTheme_determinate
#[test]
fn a_circular_indicator_draws_its_track_past_the_gap_then_its_arc_from_the_top() {
    let scheme = light_color_scheme();
    assert_eq!(light(|| CircularProgressIndicator!(|| 0.5)), half_circle(&scheme, 4.0, 40.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorScreenshotTest.kt, circularProgressIndicator_lightTheme_determinate_no_gap
#[test]
fn a_circular_indicator_without_a_gap_keeps_its_caps_apart() {
    let scheme = light_color_scheme();
    assert_eq!(
        light(|| CircularProgressIndicator!(|| 0.5, gap_size = 0.dp())),
        half_circle(&scheme, 0.0, 40.0)
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorScreenshotTest.kt, circularProgressIndicator_lightTheme_determinate_size
#[test]
fn a_larger_circular_indicator_has_a_narrower_gap() {
    let scheme = light_color_scheme();
    assert_eq!(
        light(|| CircularProgressIndicator!(|| 0.5, modifier = Modifier.size(88.dp()))),
        half_circle(&scheme, 4.0, 88.0)
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorScreenshotTest.kt, circularProgressIndicator_darkTheme_determinate
#[test]
fn a_dark_circular_indicator_takes_the_dark_colors() {
    let scheme = dark_color_scheme();
    let found = indicator(scheme, LayoutDirection::Ltr, Duration::ZERO, || {
        CircularProgressIndicator!(|| 0.5);
    });
    assert_eq!(found, half_circle(&scheme, 4.0, 40.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorScreenshotTest.kt, circularProgressIndicator_lightTheme_determinate_customCapAndTrack
#[test]
fn a_butt_capped_circular_indicators_gap_is_the_gap_alone() {
    let scheme = light_color_scheme();
    let gap = gap_sweep(4.0, 40.0);
    assert_eq!(
        light(|| CircularProgressIndicator!(
            || 0.5,
            track_color = Color::GRAY,
            stroke_cap = StrokeCap::Butt
        )),
        vec![
            arc(270.0 + 180.0 + gap, 180.0 - 2.0 * gap, Color::GRAY, StrokeCap::Butt),
            arc(270.0, 180.0, scheme.primary, StrokeCap::Butt),
        ]
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorScreenshotTest.kt, circularProgressIndicator_lightTheme_indeterminate_start
#[test]
fn an_indeterminate_circular_indicator_starts_as_a_tenth_of_the_circle() {
    let scheme = light_color_scheme();
    assert_eq!(
        light(|| IndeterminateCircularProgressIndicator!()),
        vec![arc(0.0, 36.0, scheme.primary, ROUND)]
    );
}

/// 8 s after the start (about 2 s into its second cycle): turned a third of 1080° and two
/// quarter turns, the arc about two thirds of the way from a tenth to 87 %.
fn at_eight_seconds(track: Option<(Color, f32)>) -> Vec<Drawn> {
    let scheme = light_color_scheme();
    let time = play_time(8000.0) - 6000.0;
    let rotation: f32 = (time / 6000.0 * 1080.0 + 180.0) % 360.0;
    let progress: f32 = 0.1 + (0.87 - 0.1) * time / 3000.0;
    let sweep = progress * 360.0;
    let mut expected = Vec::new();
    if let Some((color, gap)) = track {
        let gap = sweep.min(gap_sweep(gap + 4.0, 40.0));
        expected.push(arc(rotation + sweep + gap, 360.0 - sweep - 2.0 * gap, color, ROUND));
    }
    expected.push(arc(rotation, sweep, scheme.primary, ROUND));
    expected
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorScreenshotTest.kt, circularProgressIndicator_lightTheme_indeterminate
#[test]
fn an_indeterminate_circular_indicator_turns_and_grows() {
    let found = indicator(
        light_color_scheme(),
        LayoutDirection::Ltr,
        Duration::from_millis(8000),
        || IndeterminateCircularProgressIndicator!(),
    );
    assert_eq!(found, at_eight_seconds(None));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorScreenshotTest.kt, circularProgressIndicator_lightTheme_indeterminate_with_track
#[test]
fn an_indeterminate_circular_indicator_draws_a_given_track_past_the_gap() {
    let track = light_color_scheme().secondary_container;
    let found = indicator(
        light_color_scheme(),
        LayoutDirection::Ltr,
        Duration::from_millis(8000),
        move || IndeterminateCircularProgressIndicator!(track_color = track),
    );
    assert_eq!(found, at_eight_seconds(Some((track, 4.0))));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorScreenshotTest.kt, circularProgressIndicator_lightTheme_indeterminate_with_track_no_gap
#[test]
fn an_indeterminate_circular_indicators_track_without_a_gap_keeps_its_caps_apart() {
    let track = light_color_scheme().secondary_container;
    let found = indicator(
        light_color_scheme(),
        LayoutDirection::Ltr,
        Duration::from_millis(8000),
        move || IndeterminateCircularProgressIndicator!(track_color = track, gap_size = 0.dp()),
    );
    assert_eq!(found, at_eight_seconds(Some((track, 0.0))));
}
