//! Compose's `CheckboxScreenshotTest.kt`, adapted: each golden's scenario checked as the
//! display list's shapes at density 1, for the light and the dark scheme (Compose's flag off,
//! its default). A clickable checkbox's 20 dp box is 14 dp into its 48 dp touch target, its
//! check mark the lines from (4, 10) to (8, 14) to (16, 6) in the box.
//!
//! Not ported: `checkBox_focus_insetFocusRings` (feature:ripple's inset focus ring).

use std::time::Duration;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::{
    Color, Modifier, Offset, PaintStyle, Stroke, StrokeCap, ToggleableState, UnitExt,
};

/// What a checkbox draws, in the window, rounded to a hundredth.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Drawn {
    /// A round rectangle, filled (`width` 0) or stroked.
    Box { left: f32, top: f32, size: f32, radius: f32, color: Color, width: f32 },
    /// A line of the check mark.
    Line { from: (f32, f32), to: (f32, f32), color: Color, width: f32, cap: StrokeCap },
}

fn round(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}

fn width(style: PaintStyle) -> f32 {
    match style {
        PaintStyle::Fill => 0.0,
        PaintStyle::Stroke(width) => width,
    }
}

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
            DrawOp::DrawRoundRect(rect, radii, paint) => out.push(Drawn::Box {
                left: round(rect.left + at.x),
                top: round(rect.top + at.y),
                size: round(rect.width()),
                radius: round(radii[0]),
                color: paint.color,
                width: width(paint.style),
            }),
            // The ripple's circle.
            DrawOp::DrawArc { oval, paint, .. } => out.push(Drawn::Box {
                left: round(oval.left + at.x),
                top: round(oval.top + at.y),
                size: round(oval.width()),
                radius: round(oval.width() / 2.0),
                color: paint.color,
                width: 0.0,
            }),
            DrawOp::DrawLine(from, to, paint) => out.push(Drawn::Line {
                from: (round(from.x + at.x), round(from.y + at.y)),
                to: (round(to.x + at.x), round(to.y + at.y)),
                color: paint.color,
                width: width(paint.style),
                cap: paint.cap,
            }),
            _ => {}
        }
    }
    out
}

fn schemes() -> [ColorScheme; 2] {
    [light_color_scheme(), dark_color_scheme()]
}

/// `content` in `scheme`, at the window's top left, as the goldens' wrapper.
fn rule(scheme: ColorScheme, content: impl Fn() + 'static) -> ComposeTestRule {
    let content = std::rc::Rc::new(content);
    ComposeTestRule::with_manual_clock(move || {
        let content = content.clone();
        MaterialTheme!(color_scheme = scheme, { content() });
    })
}

/// The box filled, `top` dp down (a checked box: its fill is its border's color).
fn filled(top: f32, color: Color) -> Drawn {
    Drawn::Box { left: 14.0, top: top + 14.0, size: 20.0, radius: 2.0, color, width: 0.0 }
}

/// An unchecked box: filled inside its border (nothing drawn when transparent), its border
/// stroked 1 dp in.
fn outlined(top: f32, fill: Color, border: Color) -> Vec<Drawn> {
    let mut drawn = Vec::new();
    if fill.alpha() > 0 {
        drawn.push(Drawn::Box {
            left: 16.0,
            top: top + 16.0,
            size: 16.0,
            radius: 0.0,
            color: fill,
            width: 0.0,
        });
    }
    drawn.push(Drawn::Box {
        left: 15.0,
        top: top + 15.0,
        size: 18.0,
        radius: 1.0,
        color: border,
        width: 2.0,
    });
    drawn
}

/// The state layer: a 20 dp circle around the box's center (`ripple(bounded = false, 20.dp)`),
/// its color.
fn state_layer(found: &[Drawn]) -> Option<Color> {
    found.iter().find_map(|drawn| match *drawn {
        Drawn::Box { left: 4.0, top: 4.0, size: 40.0, radius: 20.0, color, width: 0.0 } => {
            Some(color)
        }
        _ => None,
    })
}

/// The check mark, `top` dp down.
fn check(top: f32, color: Color, width: f32, cap: StrokeCap) -> [Drawn; 2] {
    let line = |from: (f32, f32), to: (f32, f32)| Drawn::Line {
        from: (from.0 + 14.0, from.1 + top + 14.0),
        to: (to.0 + 14.0, to.1 + top + 14.0),
        color,
        width,
        cap,
    };
    [line((4.0, 10.0), (8.0, 14.0)), line((8.0, 14.0), (16.0, 6.0))]
}

/// The indeterminate line, `top` dp down.
fn dash(top: f32, color: Color, width: f32, cap: StrokeCap) -> [Drawn; 2] {
    let line = |from: f32, to: f32| Drawn::Line {
        from: (from + 14.0, top + 24.0),
        to: (to + 14.0, top + 24.0),
        color,
        width,
        cap,
    };
    [line(4.0, 10.0), line(10.0, 16.0)]
}

fn concat<const N: usize>(parts: [&[Drawn]; N]) -> Vec<Drawn> {
    parts.concat()
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxScreenshotTest.kt, checkBox_checked
#[test]
fn a_checked_checkbox_is_a_filled_box_and_a_check_mark() {
    for s in schemes() {
        let mut ui = rule(s, || Checkbox!(true, Some(|_| {})));
        let expected =
            concat([&[filled(0.0, s.primary)], &check(0.0, s.on_primary, 2.0, StrokeCap::Square)]);
        assert_eq!(drawn(&mut ui), expected);
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxScreenshotTest.kt, checkBox_unchecked
#[test]
fn an_unchecked_checkbox_is_its_border() {
    for s in schemes() {
        let mut ui = rule(s, || Checkbox!(false, Some(|_| {})));
        assert_eq!(drawn(&mut ui), outlined(0.0, Color::TRANSPARENT, s.on_surface_variant));
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxScreenshotTest.kt, checkBox_pressed
#[test]
fn a_pressed_checkbox_shows_its_state_layer_around_the_box() {
    for s in schemes() {
        let mut ui = rule(s, || Checkbox!(false, Some(|_| {})));
        ui.on_node(is_toggleable()).perform_touch_input(|touch| {
            let center = touch.center();
            touch.down(center);
        });
        ui.advance_time_by_frame();
        ui.advance_time_by(Duration::from_millis(400));
        let found = drawn(&mut ui);
        // The content color where it is, as `ripple()` with Compose's flag off.
        let color = state_layer(&found).expect("a state layer");
        assert_eq!(color, Color::BLACK.with_alpha(0.1));
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxScreenshotTest.kt, checkBox_indeterminate
#[test]
fn an_indeterminate_checkbox_is_a_filled_box_and_a_line() {
    for s in schemes() {
        let mut ui = rule(s, || TriStateCheckbox!(ToggleableState::Indeterminate, Some(|| {})));
        let expected =
            concat([&[filled(0.0, s.primary)], &dash(0.0, s.on_primary, 2.0, StrokeCap::Square)]);
        assert_eq!(drawn(&mut ui), expected);
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxScreenshotTest.kt, checkBox_disabled_checked
#[test]
fn a_disabled_checked_checkbox_is_faded() {
    for s in schemes() {
        let mut ui = rule(s, || Checkbox!(true, Some(|_| {}), enabled = false));
        let disabled = s.on_surface.with_alpha(0.38);
        // The check mark keeps its enabled color, as Compose's with its styling fix off.
        let expected =
            concat([&[filled(0.0, disabled)], &check(0.0, s.on_primary, 2.0, StrokeCap::Square)]);
        assert_eq!(drawn(&mut ui), expected);
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxScreenshotTest.kt, checkBox_disabled_unchecked
#[test]
fn a_disabled_unchecked_checkbox_has_a_faded_border() {
    for s in schemes() {
        let mut ui = rule(s, || Checkbox!(false, Some(|_| {}), enabled = false));
        let disabled = s.on_surface.with_alpha(0.38);
        assert_eq!(drawn(&mut ui), outlined(0.0, Color::TRANSPARENT, disabled));
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxScreenshotTest.kt, checkBox_disabled_indeterminate
#[test]
fn a_disabled_indeterminate_checkbox_is_faded() {
    for s in schemes() {
        let mut ui = rule(s, || {
            TriStateCheckbox!(ToggleableState::Indeterminate, Some(|| {}), enabled = false)
        });
        let disabled = s.on_surface.with_alpha(0.38);
        let expected =
            concat([&[filled(0.0, disabled)], &dash(0.0, s.on_primary, 2.0, StrokeCap::Square)]);
        assert_eq!(drawn(&mut ui), expected);
    }
}

/// The check mark's drawn length (both lines).
fn check_length(found: &[Drawn]) -> f32 {
    found
        .iter()
        .filter_map(|drawn| match *drawn {
            Drawn::Line { from, to, .. } => Some((to.0 - from.0).hypot(to.1 - from.1)),
            _ => None,
        })
        .sum()
}

/// A checkbox of `checked`, then the other state, `after` that change's first frame.
fn toggled(scheme: ColorScheme, checked: bool, after: Duration) -> Vec<Drawn> {
    let state = mutable_state_of(checked);
    let handle = state.handle();
    let mut ui = rule(scheme, move || {
        Checkbox!(handle.get(), Some(move |now| handle.set(now)));
    });
    state.set(!checked);
    ui.advance_time_by_frame();
    ui.advance_time_by(after);
    let found = drawn(&mut ui);
    drop(state);
    found
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxScreenshotTest.kt, checkBox_unchecked_animateToChecked
#[test]
fn a_checking_draws_the_check_mark_in_and_fills_the_box() {
    for s in schemes() {
        let found = toggled(s, false, Duration::from_millis(100));
        // Part way: the check mark partly drawn, the box between transparent and the primary
        // color (the default effects spring).
        let full = 4.0f32.hypot(4.0) + 8.0f32.hypot(8.0);
        let length = check_length(&found);
        assert!(length > 0.0 && length < full, "{length} of {full}: {found:?}");
        let fill = found.iter().find_map(|drawn| match *drawn {
            Drawn::Box { color, width: 0.0, .. } => Some(color),
            _ => None,
        });
        assert_ne!(fill, Some(s.primary), "{found:?}");
        // At rest, checked.
        let done = toggled(s, false, Duration::from_millis(1000));
        let expected =
            concat([&[filled(0.0, s.primary)], &check(0.0, s.on_primary, 2.0, StrokeCap::Square)]);
        assert_eq!(done, expected);
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxScreenshotTest.kt, checkBox_checked_animateToUnchecked
#[test]
fn an_unchecking_keeps_the_check_mark_100_ms_then_snaps_it_away() {
    for s in schemes() {
        // The check mark stays whole for 100 ms (`SnapAnimationDelay`), the box fading.
        let early = toggled(s, true, Duration::from_millis(48));
        let full = 4.0f32.hypot(4.0) + 8.0f32.hypot(8.0);
        assert!((check_length(&early) - full).abs() < 0.01, "{early:?}");
        let late = toggled(s, true, Duration::from_millis(1000));
        assert_eq!(late, outlined(0.0, Color::TRANSPARENT, s.on_surface_variant));
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxScreenshotTest.kt, checkBox_hover
#[test]
fn a_hovered_checkbox_shows_its_state_layer() {
    for s in schemes() {
        let mut ui = rule(s, || Checkbox!(true, Some(|_| {})));
        ui.on_node(is_toggleable()).perform_mouse_input(|mouse| {
            let center = mouse.center();
            mouse.enter(center);
        });
        ui.advance_time_by(Duration::from_millis(100));
        let found = drawn(&mut ui);
        let color = state_layer(&found).expect("a state layer");
        assert_eq!(color, Color::BLACK.with_alpha(0.08));
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxScreenshotTest.kt, checkBox_focus
#[test]
fn a_focused_checkbox_shows_its_state_layer() {
    for s in schemes() {
        let mut ui = rule(s, || Checkbox!(true, Some(|_| {}), modifier = Modifier.test_tag("box")));
        ui.on_node_with_tag("box").request_focus();
        ui.advance_time_by(Duration::from_millis(100));
        let found = drawn(&mut ui);
        let color = state_layer(&found).expect("a state layer");
        assert_eq!(color, Color::BLACK.with_alpha(0.1));
    }
}

/// The ten checkboxes of the custom color goldens, 2 dp apart (`Checkboxes`).
#[composable]
fn Checkboxes(colors: CheckboxColors) {
    use ToggleableState::{Indeterminate, Off, On};
    Column!(vertical_arrangement = Arrangement::spaced_by(2.dp()), {
        for (state, enabled) in [
            (Off, true),
            (Off, false),
            (On, true),
            (On, false),
            (Indeterminate, true),
            (Indeterminate, false),
        ] {
            TriStateCheckbox!(state, Some(|| {}), enabled = enabled, colors = colors);
        }
        for (checked, enabled) in [(false, true), (false, false), (true, true), (true, false)] {
            Checkbox!(checked, Some(|_| {}), enabled = enabled, colors = colors);
        }
    });
}

/// What the ten checkboxes draw, from their colors.
fn checkboxes(colors: CheckboxColors) -> Vec<Drawn> {
    use ToggleableState::{Indeterminate, Off, On};
    let states = [
        (Off, true),
        (Off, false),
        (On, true),
        (On, false),
        (Indeterminate, true),
        (Indeterminate, false),
        (Off, true),
        (Off, false),
        (On, true),
        (On, false),
    ];
    let mut expected = Vec::new();
    for (index, (state, enabled)) in states.into_iter().enumerate() {
        let top = index as f32 * 50.0;
        let (fill, border) =
            (colors.box_color(enabled, state), colors.border_color(enabled, state));
        if fill == border {
            expected.push(filled(top, fill));
        } else {
            expected.extend(outlined(top, fill, border));
        }
        let mark = colors.checkmark_color(state);
        match state {
            On => expected.extend(check(top, mark, 2.0, StrokeCap::Square)),
            Indeterminate => expected.extend(dash(top, mark, 2.0, StrokeCap::Square)),
            Off => {}
        }
    }
    expected
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxScreenshotTest.kt, checkBox_customColors
#[test]
fn checkboxes_take_colors_from_one_color_per_state() {
    for s in schemes() {
        let colors = || {
            CheckboxDefaults::colors_with(
                Color::RED,
                Color::from_argb(0xFF888888),
                Color::GREEN,
                Color::RED.with_alpha(0.38),
                Color::from_argb(0xFF888888).with_alpha(0.38),
                Color::MAGENTA.with_alpha(0.38),
            )
        };
        let mut ui = rule(s, move || Checkboxes(colors()));
        let expected = CheckboxColors {
            checked_checkmark_color: Color::GREEN,
            unchecked_checkmark_color: Color::TRANSPARENT,
            disabled_checkmark_color: Color::GREEN,
            checked_box_color: Color::RED,
            unchecked_box_color: Color::TRANSPARENT,
            disabled_checked_box_color: Color::RED.with_alpha(0.38),
            disabled_unchecked_box_color: Color::TRANSPARENT,
            disabled_indeterminate_box_color: Color::MAGENTA.with_alpha(0.38),
            checked_border_color: Color::RED,
            unchecked_border_color: Color::from_argb(0xFF888888),
            disabled_border_color: Color::RED.with_alpha(0.38),
            disabled_unchecked_border_color: Color::from_argb(0xFF888888).with_alpha(0.38),
            disabled_indeterminate_border_color: Color::MAGENTA.with_alpha(0.38),
        };
        assert_eq!(drawn(&mut ui), checkboxes(expected));
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxScreenshotTest.kt, checkBox_customCheckboxColorsConstruct
#[test]
fn checkboxes_take_every_color_given() {
    let colors = CheckboxColors {
        checked_checkmark_color: Color::BLACK,
        unchecked_checkmark_color: Color::TRANSPARENT,
        disabled_checkmark_color: Color::WHITE,
        checked_box_color: Color::GREEN,
        unchecked_box_color: Color::YELLOW,
        disabled_checked_box_color: Color::GREEN.with_alpha(0.38),
        disabled_unchecked_box_color: Color::YELLOW.with_alpha(0.38),
        disabled_indeterminate_box_color: Color::MAGENTA.with_alpha(0.38),
        checked_border_color: Color::RED,
        unchecked_border_color: Color::BLACK,
        disabled_border_color: Color::RED.with_alpha(0.38),
        disabled_unchecked_border_color: Color::BLUE,
        disabled_indeterminate_border_color: Color::from_argb(0xFFCCCCCC),
    };
    for s in schemes() {
        let mut ui = rule(s, move || Checkboxes(colors));
        assert_eq!(drawn(&mut ui), checkboxes(colors));
    }
}

/// Compose's custom stroke: `width` wide, round ends (its round join is the renderer's miter).
fn round_stroke(width: f32) -> Stroke {
    Stroke { cap: StrokeCap::Round, ..Stroke::new(width) }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxScreenshotTest.kt, checkBox_customStroke_checked
#[test]
fn a_checked_checkbox_draws_its_check_mark_with_the_stroke_given() {
    for s in schemes() {
        let mut ui = rule(s, || {
            let stroke = Some(round_stroke(2.0));
            Checkbox!(true, Some(|_| {}), checkmark_stroke = stroke, outline_stroke = stroke);
        });
        let expected =
            concat([&[filled(0.0, s.primary)], &check(0.0, s.on_primary, 2.0, StrokeCap::Round)]);
        assert_eq!(drawn(&mut ui), expected);
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxScreenshotTest.kt, checkBox_customStroke_unchecked
#[test]
fn an_unchecked_checkbox_draws_its_border_with_the_stroke_given() {
    for s in schemes() {
        let mut ui = rule(s, || {
            let stroke = Some(round_stroke(0.5));
            Checkbox!(false, Some(|_| {}), checkmark_stroke = stroke, outline_stroke = stroke);
        });
        // The border a quarter dp in (the transparent fill draws nothing).
        let expected = [Drawn::Box {
            left: 14.25,
            top: 14.25,
            size: 19.5,
            radius: 1.75,
            color: s.on_surface_variant,
            width: 0.5,
        }];
        assert_eq!(drawn(&mut ui), expected);
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxScreenshotTest.kt, checkBox_customStroke_indeterminate
#[test]
fn an_indeterminate_checkbox_draws_its_line_with_the_stroke_given() {
    for s in schemes() {
        let mut ui = rule(s, || {
            let stroke = Some(round_stroke(2.0));
            TriStateCheckbox!(
                ToggleableState::Indeterminate,
                Some(|| {}),
                checkmark_stroke = stroke,
                outline_stroke = stroke,
            );
        });
        let expected =
            concat([&[filled(0.0, s.primary)], &dash(0.0, s.on_primary, 2.0, StrokeCap::Round)]);
        assert_eq!(drawn(&mut ui), expected);
    }
}

/// The check mark's whole length: the lines from (4, 10) to (8, 14) to (16, 6).
const FULL_CHECK: f32 = 16.970562;

/// The play time of a change's animations at Compose's golden: the change's first frame
/// recomposes, its animations start on the next, and `advanceTimeBy(100)` runs whole frames
/// (112 ms): 96 ms in.
const GOLDEN_PLAY_TIME_MILLIS: i64 = 96;

/// The check mark drawn in at the golden (`checkBox_unchecked_animateToChecked`) is exactly the
/// default spatial spring's (damping 0.9, stiffness 700) fraction of it, 96 ms in.
#[test]
fn a_checkings_check_mark_follows_the_default_spatial_spring() {
    use compose_animation::{FloatAnimationSpec, FloatSpringSpec};
    let spring = FloatSpringSpec::new(0.9, 700.0, 0.01);
    let fraction = spring.value_from_nanos(GOLDEN_PLAY_TIME_MILLIS * 1_000_000, 0.0, 1.0, 0.0);
    for s in schemes() {
        let found = toggled(s, false, Duration::from_millis(100));
        let length = check_length(&found);
        assert!(
            (length - FULL_CHECK * fraction).abs() < 0.02,
            "{length} for {fraction}: {found:?}"
        );
    }
}

/// `toggled` with an opaque unchecked check mark: unchecking, what's drawn says when the mark's
/// length goes, not the color fading away under it.
fn unchecking_opaque(scheme: ColorScheme, after: Duration) -> Vec<Drawn> {
    let state = mutable_state_of(true);
    let handle = state.handle();
    let mut ui = rule(scheme, move || {
        let colors = CheckboxColors {
            unchecked_checkmark_color: Color::GREEN,
            ..CheckboxDefaults::colors()
        };
        Checkbox!(handle.get(), Some(move |now| handle.set(now)), colors = colors);
    });
    state.set(false);
    ui.advance_time_by_frame();
    ui.advance_time_by(after);
    let found = drawn(&mut ui);
    drop(state);
    found
}

/// Unchecking, the check mark stays whole until the snap's 100 ms delay (`SnapAnimationDelay`):
/// whole at the golden (96 ms in, `checkBox_checked_animateToUnchecked`), gone a frame later.
#[test]
fn an_uncheckings_check_mark_snaps_away_at_100_ms() {
    for s in schemes() {
        let at_golden = unchecking_opaque(s, Duration::from_millis(100));
        assert!((check_length(&at_golden) - FULL_CHECK).abs() < 0.01, "{at_golden:?}");
        let a_frame_later = unchecking_opaque(s, Duration::from_millis(116));
        assert_eq!(check_length(&a_frame_later), 0.0, "{a_frame_later:?}");
    }
}
