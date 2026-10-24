//! Second Material review: failing tests for disabled states and icon tinting. Each test states
//! the input, what Jetpack Compose's material3 does, and what happens instead.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::sync::Arc;

use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Color, ImageVector, Modifier, Paint, Path, Rect, UnitExt, platform::DrawOp};

thread_local! {
    static SCHEME: Cell<Option<ColorScheme>> = const { Cell::new(None) };
    static SEEN: Cell<Color> = const { Cell::new(Color::UNSPECIFIED) };
}

fn remember_scheme() {
    SCHEME.with(|cell| cell.set(Some(MaterialTheme::color_scheme())));
}

fn scheme() -> ColorScheme {
    SCHEME.with(Cell::get).expect("composed")
}

fn round_rects(ui: &mut ComposeTestRule) -> Vec<(Rect, [f32; 4], Paint)> {
    ui.host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawRoundRect(rect, radii, paint) => Some((*rect, *radii, *paint)),
            _ => None,
        })
        .collect()
}

/// Compose's `Color.copy(alpha).compositeOver(background)` in sRGB, as `toArgb()` packs it.
fn composite_over(color: Color, alpha: f32, background: Color) -> Color {
    let alpha = (alpha * 255.0 + 0.5).floor() / 255.0;
    let channel = |fg: u8, bg: u8| {
        let value = f32::from(fg) / 255.0 * alpha + f32::from(bg) / 255.0 * (1.0 - alpha);
        (value * 255.0 + 0.5) as u32
    };
    Color::from_argb(
        0xFF00_0000
            | channel(color.red(), background.red()) << 16
            | channel(color.green(), background.green()) << 8
            | channel(color.blue(), background.blue()),
    )
}

fn assert_close(actual: Color, expected: Color, what: &str) {
    let near = |a: u8, b: u8| a.abs_diff(b) <= 1;
    assert!(
        near(actual.alpha(), expected.alpha())
            && near(actual.red(), expected.red())
            && near(actual.green(), expected.green())
            && near(actual.blue(), expected.blue()),
        "{what}: {actual:?}, Compose {expected:?}"
    );
}

// ---- RadioButton -----------------------------------------------------------------------------

/// Compose's `RadioButtonDefaults.colors()` has `disabledSelectedColor` and
/// `disabledUnselectedColor`, both `onSurface` at 38 % (the radio tokens' disabled icon colors).
/// Here a disabled radio button draws its enabled color at 38 %: a selected one is `primary` at
/// 38 % (a faded purple ring and dot where Compose's are gray), an unselected one
/// `on_surface_variant` at 38 %.
#[test]
fn a_disabled_radio_button_is_on_surface_at_38_percent() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            remember_scheme();
            RadioButton!(true, Some(|| {}), enabled = false);
        });
    });
    let expected = scheme().on_surface.with_alpha(0.38);
    let shapes = round_rects(&mut ui);
    let (_, _, ring) = shapes.first().expect("the ring");
    let (_, _, dot) = shapes.last().expect("the dot");
    assert_eq!(ring.color, expected, "the ring");
    assert_eq!(dot.color, expected, "the dot");

    let mut ui = compose_test(|| {
        MaterialTheme!({
            RadioButton!(false, Some(|| {}), enabled = false);
        });
    });
    let (_, _, ring) = *round_rects(&mut ui).first().expect("the ring");
    assert_eq!(ring.color, expected, "an unselected ring");
}

// ---- Switch ----------------------------------------------------------------------------------

/// Compose's `SwitchDefaults.colors()` composites each disabled color over the surface:
/// `disabledCheckedTrackColor = onSurface.copy(alpha = 0.12f).compositeOver(surface)`, and so
/// for `disabledUncheckedThumbColor` (`onSurface` at 38 %), `disabledUncheckedTrackColor`
/// (`surfaceContainerHighest` at 12 %) and `disabledUncheckedBorderColor` (`onSurface` at
/// 12 %): opaque colors, so a disabled switch over a non-surface background (a card, a colored
/// row) looks the same as over the surface. Here they are translucent (`with_alpha(0.12)`),
/// so the background shows through the track. (The first Material review's
/// `a_disabled_checked_switch_has_materials_disabled_colors` expects the translucent value.)
#[test]
fn a_disabled_switchs_colors_are_composited_over_the_surface() {
    thread_local! { static COLORS: Cell<Option<SwitchColors>> = const { Cell::new(None) }; }
    let _ui = compose_test(|| {
        MaterialTheme!({
            remember_scheme();
            COLORS.with(|cell| cell.set(Some(SwitchDefaults::colors())));
        });
    });
    let (scheme, colors) = (scheme(), COLORS.with(Cell::get).expect("composed"));
    let surface = scheme.surface;
    assert_close(colors.disabled_checked_thumb_color, surface, "disabled checked thumb");
    assert_close(
        colors.disabled_checked_track_color,
        composite_over(scheme.on_surface, 0.12, surface),
        "disabled checked track",
    );
    assert_close(
        colors.disabled_unchecked_thumb_color,
        composite_over(scheme.on_surface, 0.38, surface),
        "disabled unchecked thumb",
    );
    assert_close(
        colors.disabled_unchecked_track_color,
        composite_over(scheme.surface_container_highest, 0.12, surface),
        "disabled unchecked track",
    );
    assert_close(
        colors.disabled_unchecked_border_color,
        composite_over(scheme.on_surface, 0.12, surface),
        "disabled unchecked border",
    );
}

// ---- Slider ----------------------------------------------------------------------------------

/// Compose's `SliderDefaults.colors()` has its own disabled colors, all from `onSurface`: the
/// thumb `onSurface` at 38 % over the surface (opaque), the active track `onSurface` at 38 %,
/// the inactive track `onSurface` at 12 %. Here a disabled slider draws its enabled colors at
/// 38 %: a faded `primary` thumb and active track, and `surface_container_highest` at 38 %.
#[test]
fn a_disabled_slider_has_materials_disabled_colors() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            remember_scheme();
            Slider!(0.5, |_| {}, modifier = Modifier.width(200.dp()), enabled = false);
        });
    });
    let scheme = scheme();
    let shapes = round_rects(&mut ui);
    assert_eq!(shapes.len(), 3, "{shapes:?}");
    let mut wrong = Vec::new();
    let mut check = |actual: Color, expected: Color, what: &str| {
        let near = |a: u8, b: u8| a.abs_diff(b) <= 1;
        let close = near(actual.alpha(), expected.alpha())
            && near(actual.red(), expected.red())
            && near(actual.green(), expected.green())
            && near(actual.blue(), expected.blue());
        if !close {
            wrong.push(format!("{what}: {actual:?}, Compose {expected:?}"));
        }
    };
    check(shapes[0].2.color, scheme.on_surface.with_alpha(0.12), "inactive track");
    check(shapes[1].2.color, scheme.on_surface.with_alpha(0.38), "active track");
    let thumb = composite_over(scheme.on_surface, 0.38, scheme.surface);
    check(shapes[2].2.color, thumb, "thumb");
    assert!(wrong.is_empty(), "{wrong:#?}");
}

// ---- TextField -------------------------------------------------------------------------------

/// Compose's `TextFieldDefaults.colors()` has disabled colors for the decoration:
/// `disabledContainerColor` is `onSurface` at 4 %, `disabledIndicatorColor` `onSurface` at
/// 38 %, `disabledLabelColor` and `disabledPlaceholderColor` `onSurface` at 38 %. Here only the
/// text dims: a disabled field keeps its enabled container (`surface_container_highest`), its
/// `on_surface_variant` indicator line and its `on_surface_variant` label, so it looks enabled.
#[test]
fn a_disabled_text_fields_decoration_is_dimmed() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            remember_scheme();
            TextField!(
                value = "Ada".to_string(),
                on_value_change = |_| {},
                enabled = false,
                label = {
                    SEEN.with(|cell| cell.set(LOCAL_CONTENT_COLOR.current()));
                    Text!("Name");
                },
            );
        });
    });
    let scheme = scheme();
    let list = ui.host().display_list();
    let mut wrong = Vec::new();
    // The container: the only shape rounded at the top alone.
    let container = list.ops.iter().find_map(|op| match op {
        DrawOp::DrawRoundRect(_, radii, paint) if radii[0] > 0.0 && radii[2] == 0.0 => {
            Some(paint.color)
        }
        _ => None,
    });
    let expected = scheme.on_surface.with_alpha(0.04);
    if container != Some(expected) {
        wrong.push(format!("container: {container:?}, Compose {expected:?}"));
    }
    // The indicator line: the last plain rectangle.
    let indicator = list.ops.iter().rev().find_map(|op| match op {
        DrawOp::DrawRect(_, paint) => Some(paint.color),
        _ => None,
    });
    let expected = scheme.on_surface.with_alpha(0.38);
    if indicator != Some(expected) {
        wrong.push(format!("indicator: {indicator:?}, Compose {expected:?}"));
    }
    let label = SEEN.with(Cell::get);
    if label != expected {
        wrong.push(format!("label: {label:?}, Compose {expected:?}"));
    }
    assert!(wrong.is_empty(), "{wrong:#?}\n{}", ui.dump_display_list());
}

// ---- Icon ------------------------------------------------------------------------------------

/// Compose's `Icon` tints with `ColorFilter.tint(tint)` (`BlendMode.SrcIn`): every path is drawn
/// in the tint, whatever its own fill, which is why multicolored images pass
/// `tint = Color.Unspecified`. Here a path with its own fill keeps it under the tint: a red path
/// in an icon tinted blue (the content color) is drawn red.
#[test]
fn an_icon_tint_recolors_paths_with_their_own_fill() {
    thread_local! {
        static VECTOR: ImageVector = ImageVector {
            name: "review.RedSquare",
            default_width: 24.dp(),
            default_height: 24.dp(),
            viewport_width: 24.0,
            viewport_height: 24.0,
            paths: Arc::from(vec![(
                Path::from_svg("M4,4h16v16h-16z").unwrap().data(),
                Some(Color::RED),
            )]),
        };
    }
    let mut ui = compose_test(|| {
        CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(Color::BLUE), || {
            Icon!(VECTOR.with(Clone::clone), None);
        });
    });
    let colors: Vec<Color> = ui
        .host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawPath { paint, .. } => Some(paint.color),
            _ => None,
        })
        .collect();
    assert_eq!(colors, vec![Color::BLUE]);
}
