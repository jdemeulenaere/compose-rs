//! Compose's `SegmentedButtonTest.kt` and `SegmentedButtonScreenshotTest.kt`, adapted: the
//! multi-choice and single-choice rows, their colors, checked items, semantics, icons, sizes,
//! content padding and borders; the goldens as display-list checks (the containers' colors,
//! the borders' order, the shapes). Compose's checkable `SegmentedButton` overload is
//! `CheckableSegmentedButton`.
//!
//! Not ported: `segmentedButton_fontScale_correctSizing` (its 53 dp comes from Roboto's
//! metrics at twice the font scale; the fixed-advance measurer has none).

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::{Color, Modifier, PaddingValues, PaintStyle, RECTANGLE_SHAPE, UnitExt};

const VALUES: [&str; 3] = ["Day", "Month", "Week"];

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SegmentedButtonTest.kt, toggleableSegmentedButton_itemsDisplay
#[test]
fn a_multi_choice_row_shows_its_items() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            MultiChoiceSegmentedButtonRow!(|scope: MultiChoiceSegmentedButtonRowScope| {
                for value in VALUES {
                    CheckableSegmentedButton!(scope, false, |_| {}, RECTANGLE_SHAPE, {
                        Text!("{}", value)
                    });
                }
            });
        });
    });
    for value in VALUES {
        ui.on_node_with_text(value).assert_is_displayed();
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SegmentedButtonTest.kt, selectableSegmentedButton_itemsDisplay
#[test]
fn a_single_choice_row_shows_its_items() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            SingleChoiceSegmentedButtonRow!(|scope: SingleChoiceSegmentedButtonRowScope| {
                for value in VALUES {
                    SegmentedButton!(scope, false, || {}, RECTANGLE_SHAPE, { Text!("{}", value) });
                }
            });
        });
    });
    for value in VALUES {
        ui.on_node_with_text(value).assert_is_displayed();
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SegmentedButtonTest.kt, selectableSegmentedButton_defaultColors
#[test]
fn the_default_colors_are_the_tokens() {
    thread_local! {
        static READ: std::cell::Cell<Option<SegmentedButtonColors>> =
            const { std::cell::Cell::new(None) };
    }
    let _ui = compose_test(|| {
        MaterialTheme!({
            READ.with(|read| read.set(Some(SegmentedButtonDefaults::colors())));
        });
    });
    let s = light_color_scheme();
    assert_eq!(
        READ.with(std::cell::Cell::get),
        Some(SegmentedButtonColors {
            active_container_color: s.secondary_container,
            active_content_color: s.on_secondary_container,
            active_border_color: s.outline,
            inactive_container_color: Color::TRANSPARENT,
            inactive_content_color: s.on_surface,
            inactive_border_color: s.outline,
            disabled_active_container_color: s.secondary_container,
            disabled_active_content_color: s.on_surface.with_alpha(0.38),
            disabled_active_border_color: s.outline.with_alpha(0.12),
            disabled_inactive_container_color: Color::TRANSPARENT,
            disabled_inactive_content_color: s.on_surface.with_alpha(0.38),
            disabled_inactive_border_color: s.outline.with_alpha(0.12),
        })
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SegmentedButtonTest.kt, segmentedButton_itemsChecked
#[test]
fn checkable_segments_report_their_checks() {
    let checked = mutable_state_of(true);
    let handle = checked.handle();
    let mut ui = compose_test(move || {
        MaterialTheme!({
            MultiChoiceSegmentedButtonRow!(|scope: MultiChoiceSegmentedButtonRowScope| {
                let on = handle.get();
                CheckableSegmentedButton!(
                    scope,
                    on,
                    move |now| handle.set(now),
                    RECTANGLE_SHAPE,
                    { Text!("Day") }
                );
                CheckableSegmentedButton!(
                    scope,
                    !on,
                    move |now| handle.set(now),
                    RECTANGLE_SHAPE,
                    { Text!("Month") }
                );
            });
        });
    });
    ui.on_node_with_text("Day").assert_is_on();
    ui.on_node_with_text("Month").assert_is_off();
    checked.set(false);
    ui.on_node_with_text("Day").assert_is_off();
    ui.on_node_with_text("Month").assert_is_on();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SegmentedButtonTest.kt, selectableSegmentedButton_semantics
#[test]
fn a_single_choice_row_is_a_selectable_group() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            SingleChoiceSegmentedButtonRow!(
                modifier = Modifier.test_tag("row"),
                |scope: SingleChoiceSegmentedButtonRowScope| {
                    SegmentedButton!(scope, false, || {}, RECTANGLE_SHAPE, { Text!("Day") });
                    SegmentedButton!(scope, false, || {}, RECTANGLE_SHAPE, { Text!("Month") });
                }
            );
        });
    });
    assert!(ui.on_node_with_tag("row").fetch_semantics_node().config.selectable_group);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SegmentedButtonTest.kt, segmentedButton_icon
#[test]
fn a_segments_icon_follows_its_check() {
    let checked = mutable_state_of(false);
    let handle = checked.handle();
    let mut ui = compose_test(move || {
        MaterialTheme!({
            MultiChoiceSegmentedButtonRow!(|scope: MultiChoiceSegmentedButtonRowScope| {
                let on = handle.get();
                CheckableSegmentedButton!(
                    scope,
                    on,
                    |_| {},
                    RECTANGLE_SHAPE,
                    icon = Some(move || Text!("{}", if on { "checked" } else { "unchecked" })),
                    { Text!("Day") }
                );
            });
        });
    });
    ui.on_node_with_text_unmerged("unchecked").assert_is_displayed();
    checked.set(true);
    ui.on_node_with_text_unmerged("checked").assert_is_displayed();
}

/// The height and width of a row of two segments with 60 and 30 dp labels, padded by
/// `padding` (Compose's default if `None`), in a 300 by 100 dp box.
fn row_size(padding: [Option<PaddingValues>; 2]) -> (f32, f32) {
    let mut ui = compose_test(move || {
        MaterialTheme!({
            Box!(modifier = Modifier.width_in(0.dp(), 300.dp()).height_in(0.dp(), 100.dp()), {
                Box!(modifier = Modifier.test_tag("container"), {
                    MultiChoiceSegmentedButtonRow!(|scope: MultiChoiceSegmentedButtonRowScope| {
                        for (width, padding) in [(60.0, padding[0]), (30.0, padding[1])] {
                            let padding =
                                padding.unwrap_or(SegmentedButtonDefaults::CONTENT_PADDING);
                            CheckableSegmentedButton!(
                                scope,
                                false,
                                |_| {},
                                RECTANGLE_SHAPE,
                                content_padding = padding,
                                { Text!("Day", modifier = Modifier.width(width.dp())) }
                            );
                        }
                    });
                });
            });
        });
    });
    let bounds = ui.on_node_with_tag("container").fetch_semantics_node().bounds;
    (bounds.width(), bounds.height())
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SegmentedButtonTest.kt, segmentedButton_Sizing
#[test]
fn segments_are_as_wide_as_the_widest_and_48_dp_high() {
    let (width, height) = row_size([None, None]);
    assert!(width >= (60.0 + 12.0 * 2.0) * 2.0, "{width}");
    assert_eq!(height, 48.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SegmentedButtonTest.kt, segmentedButton_contentPadding_correctSizing
#[test]
fn a_segments_content_padding_sizes_it() {
    let (width, height) =
        row_size([Some(PaddingValues::all(24.dp())), Some(PaddingValues::all(20.dp()))]);
    assert!(width >= (60.0 + 12.0 * 2.0) * 2.0, "{width}");
    // The 20 dp label line and 24 dp on each side.
    assert_eq!(height, 68.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SegmentedButtonTest.kt, segmentedButtonBorder_default_matchesSpec
#[test]
fn the_default_border_is_the_outline_1_dp_wide() {
    thread_local! {
        static READ: std::cell::Cell<Option<compose_foundation::BorderStroke>> =
            const { std::cell::Cell::new(None) };
    }
    let _ui = compose_test(|| {
        MaterialTheme!({
            let color = SegmentedButtonDefaults::colors().border_color(true, true);
            READ.with(|read| read.set(Some(SegmentedButtonDefaults::border_stroke(color))));
        });
    });
    let border = READ.with(std::cell::Cell::get).expect("composed");
    assert_eq!((border.color, border.width), (light_color_scheme().outline, 1.dp()));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SegmentedButtonTest.kt, segmentedButtonBorderColor_customColors_resolvesCorrectly
#[test]
fn custom_border_colors_resolve_by_state() {
    let colors = SegmentedButtonColors {
        active_border_color: Color::MAGENTA,
        inactive_border_color: Color::CYAN,
        disabled_active_border_color: Color::YELLOW,
        disabled_inactive_border_color: Color::GREEN,
        ..SegmentedButtonColors {
            active_container_color: Color::BLACK,
            active_content_color: Color::BLACK,
            active_border_color: Color::BLACK,
            inactive_container_color: Color::BLACK,
            inactive_content_color: Color::BLACK,
            inactive_border_color: Color::BLACK,
            disabled_active_container_color: Color::BLACK,
            disabled_active_content_color: Color::BLACK,
            disabled_active_border_color: Color::BLACK,
            disabled_inactive_container_color: Color::BLACK,
            disabled_inactive_content_color: Color::BLACK,
            disabled_inactive_border_color: Color::BLACK,
        }
    };
    assert_eq!(colors.border_color(true, true), Color::MAGENTA);
    assert_eq!(colors.border_color(true, false), Color::CYAN);
    assert_eq!(colors.border_color(false, true), Color::YELLOW);
    assert_eq!(colors.border_color(false, false), Color::GREEN);
}

// ---- The goldens ------------------------------------------------------------------------------

/// A multi-choice row of the three values, `checked(index)` checked, in `scheme`, `colors` if
/// given, each segment's shape its `item_shape` if `shaped`.
fn golden(
    scheme: ColorScheme,
    checked: fn(usize) -> bool,
    colors: Option<fn() -> SegmentedButtonColors>,
    shaped: bool,
) -> ComposeTestRule {
    compose_test(move || {
        MaterialTheme!(color_scheme = scheme, {
            MultiChoiceSegmentedButtonRow!(|scope: MultiChoiceSegmentedButtonRowScope| {
                for (index, value) in VALUES.into_iter().enumerate() {
                    let colors = colors.map_or_else(SegmentedButtonDefaults::colors, |c| c());
                    let shape = if shaped {
                        SegmentedButtonDefaults::item_shape(index, VALUES.len())
                    } else {
                        RECTANGLE_SHAPE
                    };
                    CheckableSegmentedButton!(
                        scope,
                        checked(index),
                        |_| {},
                        shape,
                        colors = colors,
                        { Text!("{}", value) }
                    );
                }
            });
        });
    })
}

/// The fills and the strokes drawn, in order: (color, stroked).
fn painted(ui: &mut ComposeTestRule) -> Vec<(Color, bool)> {
    ui.host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawRect(_, paint) | DrawOp::DrawRoundRect(_, _, paint) => {
                Some((paint.color, matches!(paint.style, PaintStyle::Stroke(_))))
            }
            _ => None,
        })
        .collect()
}

fn fills(ui: &mut ComposeTestRule) -> Vec<Color> {
    painted(ui).into_iter().filter(|(_, stroked)| !stroked).map(|(color, _)| color).collect()
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SegmentedButtonScreenshotTest.kt, all_unselected
#[test]
fn unchecked_segments_have_no_container() {
    let mut ui = golden(light_color_scheme(), |_| false, None, false);
    assert!(fills(&mut ui).is_empty(), "{:?}", fills(&mut ui));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SegmentedButtonScreenshotTest.kt, all_selected
#[test]
fn checked_segments_are_on_the_secondary_container() {
    let s = light_color_scheme();
    let mut ui = golden(s, |_| true, None, false);
    assert_eq!(fills(&mut ui), [s.secondary_container; 3]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SegmentedButtonScreenshotTest.kt, middle_selected
#[test]
fn only_the_checked_middle_segment_has_a_container() {
    let s = light_color_scheme();
    let mut ui = golden(s, |index| index == 1, None, false);
    assert_eq!(fills(&mut ui), [s.secondary_container]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SegmentedButtonScreenshotTest.kt, middle_selected_with_icon
#[test]
fn segments_draw_the_icons_they_are_given() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            MultiChoiceSegmentedButtonRow!(|scope: MultiChoiceSegmentedButtonRowScope| {
                for (index, value) in VALUES.into_iter().enumerate() {
                    let icon = move || {
                        if index == 1 {
                            SegmentedButtonDefaults::active_icon();
                        } else {
                            Icon!(
                                icons::filled::favorite(),
                                None,
                                modifier = Modifier.size(SegmentedButtonDefaults::ICON_SIZE),
                            );
                        }
                    };
                    CheckableSegmentedButton!(
                        scope,
                        index == 1,
                        |_| {},
                        RECTANGLE_SHAPE,
                        icon = Some(icon),
                        { Text!("{}", value) }
                    );
                }
            });
        });
    });
    let icons = ui
        .host()
        .display_list()
        .ops
        .iter()
        .filter(|op| matches!(op, DrawOp::DrawPath { .. }))
        .count();
    assert_eq!(icons, 3);
}

/// Blue borders active, yellow inactive.
fn blue_and_yellow() -> SegmentedButtonColors {
    SegmentedButtonColors {
        active_border_color: Color::BLUE,
        inactive_border_color: Color::YELLOW,
        ..SegmentedButtonDefaults::colors()
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SegmentedButtonScreenshotTest.kt, stroke_zIndex
#[test]
fn the_checked_segments_border_is_drawn_over_its_neighbours() {
    let mut ui = golden(light_color_scheme(), |index| index == 1, Some(blue_and_yellow), false);
    let strokes: Vec<Color> =
        painted(&mut ui).into_iter().filter(|(_, stroked)| *stroked).map(|(c, _)| c).collect();
    assert_eq!(strokes, [Color::YELLOW, Color::YELLOW, Color::BLUE], "drawn last, over the others");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SegmentedButtonScreenshotTest.kt, button_shape
#[test]
fn item_shapes_round_the_rows_ends() {
    let mut ui = golden(light_color_scheme(), |index| index == 1, Some(blue_and_yellow), true);
    let radii: Vec<[f32; 4]> = ui
        .host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawRoundRect(_, radii, paint)
                if matches!(paint.style, PaintStyle::Stroke(_)) =>
            {
                Some(*radii)
            }
            _ => None,
        })
        .collect();
    // The first rounded at its start, the last at its end; the middle one (a rectangle) isn't
    // a round rectangle.
    assert_eq!(radii.len(), 2, "{radii:?}");
    assert!(radii[0][0] > 0.0 && radii[0][1] == 0.0, "{radii:?}");
    assert!(radii[1][0] == 0.0 && radii[1][1] > 0.0, "{radii:?}");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SegmentedButtonScreenshotTest.kt, all_unselected_darkTheme
#[test]
fn dark_unchecked_segments_have_no_container() {
    let mut ui = golden(dark_color_scheme(), |_| false, None, false);
    assert!(fills(&mut ui).is_empty());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SegmentedButtonScreenshotTest.kt, all_selected_darkTheme
#[test]
fn dark_checked_segments_are_on_the_dark_secondary_container() {
    let s = dark_color_scheme();
    let mut ui = golden(s, |_| true, None, false);
    assert_eq!(fills(&mut ui), [s.secondary_container; 3]);
}
