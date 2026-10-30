//! The FABs of 1.4 and on: Compose's `FloatingActionButtonTest.kt`'s medium and sized extended FABs
//! and `animateFloatingActionButton`, its `FloatingActionButtonScreenshotTest.kt`'s as sizes, shapes
//! and the layer's properties.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compose_animation::{AnimationSpec, Easing, TweenSpec};
use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Alignment, Color, IntSize, LOCAL_DENSITY, Modifier, UnitExt, platform::DrawOp};

const TAG: &str = "fab";

fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    let content = Rc::new(content);
    ComposeTestRule::sized(IntSize::new(600, 600), move || {
        let content = content.clone();
        MaterialTheme!({
            let content = content.clone();
            Box!({
                content();
            });
        });
    })
}

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_node().bounds
}

/// The radius of the corners of the FAB's container.
fn corner(ui: &mut ComposeTestRule) -> f32 {
    ui.host()
        .display_list()
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::DrawRoundRect(_, radii, _) => Some(radii[0]),
            _ => None,
        })
        .expect("a container")
}

/// The left and right of something that's not a semantics node, the text of an extended FAB being
/// cleared from them: its drawing in `Color::RED`, in the tree's `translate`s.
fn red_bounds(ui: &mut ComposeTestRule) -> (f32, f32) {
    let mut at = 0.0;
    let mut stack = Vec::new();
    for op in &ui.host().display_list().ops {
        match op {
            DrawOp::Save | DrawOp::SaveLayerAlpha { .. } => stack.push(at),
            DrawOp::Restore => at = stack.pop().unwrap_or(0.0),
            DrawOp::Translate(by) => at += by.x,
            DrawOp::DrawRect(rect, paint) if paint.color == Color::RED => {
                return (rect.left + at, rect.right + at);
            }
            _ => {}
        }
    }
    panic!("nothing in red: {}", ui.dump_display_list());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/FloatingActionButtonScreenshotTest.kt, mediumFab
#[test]
fn a_medium_fab_is_80_dp_with_the_large_increased_corners() {
    let mut ui = rule(|| {
        MediumFloatingActionButton!(|| {}, modifier = Modifier.test_tag(TAG), {
            Box!(modifier = Modifier.size(FloatingActionButtonDefaults::MEDIUM_ICON_SIZE));
        });
    });
    ui.on_node_with_tag(TAG).assert_width_is_equal_to(80.dp()).assert_height_is_equal_to(80.dp());
    assert_eq!(corner(&mut ui), 20.0);
    assert_eq!(FloatingActionButtonDefaults::MEDIUM_ICON_SIZE, 28.dp());
}

thread_local! {
    static STYLE: Cell<Option<compose_ui::TextUnit>> = const { Cell::new(None) };
    static EXPECTED: Cell<Option<compose_ui::TextUnit>> = const { Cell::new(None) };
}

fn read_style() {
    STYLE.set(Some(LOCAL_TEXT_STYLE.current().font_size));
}

/// The style of what's in `fab`, and that of the theme's `expected`.
#[track_caller]
fn check_style(fab: fn(), expected: fn(&Typography) -> compose_ui::TextUnit) {
    let _ui = rule(move || {
        EXPECTED.set(Some(expected(&MaterialTheme::typography())));
        fab();
    });
    assert!(STYLE.get().is_some());
    assert_eq!(STYLE.get(), EXPECTED.get());
}

/// Each size's text and icon: its height and square when collapsed, its padding, its spacing, its
/// style.
fn check_extended(size: fn(bool, Modifier), minimum: f32, start: f32, spacing: f32, corners: f32) {
    let mut expanded = rule(move || size(true, Modifier.test_tag(TAG)));
    let (fab, icon) = (bounds(&mut expanded, TAG), bounds(&mut expanded, "icon"));
    let (text_left, text_right) = red_bounds(&mut expanded);
    assert_eq!(fab.height(), minimum);
    assert_eq!(icon.left - fab.left, start);
    assert_eq!(text_left - icon.right, spacing);
    assert_eq!(fab.right - text_right, start);
    assert_eq!(corner(&mut expanded), corners);
    let mut collapsed = rule(move || size(false, Modifier.test_tag(TAG)));
    let fab = bounds(&mut collapsed, TAG);
    assert_eq!((fab.width(), fab.height()), (minimum, minimum));
    assert!(!collapsed.dump_tree().contains("cleared"), "{}", collapsed.dump_tree());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/FloatingActionButtonScreenshotTest.kt, smallExtendedFabTextAndIcon
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/FloatingActionButtonScreenshotTest.kt, mediumExtendedFabTextAndIcon
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/FloatingActionButtonScreenshotTest.kt, largeExtendedFabTextAndIcon
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/FloatingActionButtonTest.kt, expandedLargeExtendedFabTextAndIconHaveSizeFromSpecAndVisible
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/FloatingActionButtonTest.kt, collapsedLargeExtendedFabTextAndIconHaveSizeFromSpecAndTextNotVisible
#[test]
fn each_extended_size_has_its_tokens() {
    #[composable]
    fn Icon24() {
        Box!(modifier = Modifier.size(24.dp()).test_tag("icon"));
    }
    #[composable]
    fn Label() {
        Box!(modifier = Modifier.size((30.dp(), 18.dp())).background(Color::RED));
    }
    check_extended(
        |expanded, modifier| {
            SmallExtendedFloatingActionButton!(
                || Label(),
                || Icon24(),
                || {},
                modifier = modifier,
                expanded = expanded
            );
        },
        56.0,
        16.0,
        8.0,
        16.0,
    );
    check_extended(
        |expanded, modifier| {
            MediumExtendedFloatingActionButton!(
                || Label(),
                || Icon24(),
                || {},
                modifier = modifier,
                expanded = expanded
            );
        },
        80.0,
        26.0,
        12.0,
        20.0,
    );
    check_extended(
        |expanded, modifier| {
            LargeExtendedFloatingActionButton!(
                || Label(),
                || Icon24(),
                || {},
                modifier = modifier,
                expanded = expanded
            );
        },
        96.0,
        28.0,
        16.0,
        28.0,
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/FloatingActionButtonScreenshotTest.kt, smallExtendedFabTextOnly
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/FloatingActionButtonScreenshotTest.kt, mediumExtendedFabTextOnly
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/FloatingActionButtonScreenshotTest.kt, largeExtendedFabTextOnly
#[test]
fn each_extended_sizes_content_is_a_row_with_its_padding() {
    let check = |mut ui: ComposeTestRule, minimum: f32, padding: f32| {
        let (fab, text) = (bounds(&mut ui, TAG), bounds(&mut ui, "text"));
        assert_eq!(fab.height(), minimum);
        assert_eq!(text.left - fab.left, padding);
        assert_eq!(fab.right - text.right, padding);
    };
    check(
        rule(|| {
            SmallExtendedFloatingActionButtonContent!(
                || {},
                modifier = Modifier.test_tag(TAG),
                |_: RowScope| {
                    Text!("A longer text", modifier = Modifier.test_tag("text"));
                }
            );
        }),
        56.0,
        16.0,
    );
    check(
        rule(|| {
            MediumExtendedFloatingActionButtonContent!(
                || {},
                modifier = Modifier.test_tag(TAG),
                |_: RowScope| {
                    Text!("A longer text", modifier = Modifier.test_tag("text"));
                }
            );
        }),
        80.0,
        26.0,
    );
    check(
        rule(|| {
            LargeExtendedFloatingActionButtonContent!(
                || {},
                modifier = Modifier.test_tag(TAG),
                |_: RowScope| {
                    Text!("A longer text", modifier = Modifier.test_tag("text"));
                }
            );
        }),
        96.0,
        28.0,
    );
    check(
        rule(|| {
            ExtendedFloatingActionButtonContent!(
                || {},
                modifier = Modifier.test_tag(TAG),
                |_: RowScope| {
                    Text!("A longer text", modifier = Modifier.test_tag("text"));
                }
            );
        }),
        56.0,
        20.0,
    );
}

/// Short content: at least 80 dp wide, centered.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/FloatingActionButtonTest.kt, extendedFabShortTextHasMinimumSizeFromSpec
#[test]
fn a_short_extended_fab_is_80_dp_wide() {
    let mut ui = rule(|| {
        ExtendedFloatingActionButtonContent!(
            || {},
            modifier = Modifier.test_tag(TAG),
            |_: RowScope| {
                Box!(modifier = Modifier.size(2.dp()).test_tag("text"));
            }
        );
    });
    let (fab, text) = (bounds(&mut ui, TAG), bounds(&mut ui, "text"));
    assert_eq!(fab.width(), 80.0);
    assert_eq!(text.left - fab.left, 39.0);
}

/// Each size's style.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/FloatingActionButton.kt, SmallExtendedFabTextStyle
#[test]
fn each_extended_sizes_text_has_its_style() {
    check_style(
        || SmallExtendedFloatingActionButtonContent!(|| {}, |_: RowScope| read_style()),
        |typography| typography.title_medium.font_size,
    );
    check_style(
        || MediumExtendedFloatingActionButtonContent!(|| {}, |_: RowScope| read_style()),
        |typography| typography.title_large.font_size,
    );
    check_style(
        || LargeExtendedFloatingActionButton!(|| read_style(), || {}, || {}),
        |typography| typography.headline_small.font_size,
    );
    check_style(
        || FloatingActionButton!(|| {}, || read_style()),
        |typography| typography.label_large.font_size,
    );
}

/// The width and the text follow `expanded`: gone when it's collapsed, there as it expands.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/FloatingActionButtonTest.kt, largeExtendedFabAnimates
#[test]
fn a_sized_extended_fab_animates() {
    let expanded = mutable_state_of(true);
    let read = expanded.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        MaterialTheme!({
            Box!({
                LargeExtendedFloatingActionButton!(
                    || Text!("FAB", modifier = Modifier.test_tag("text")),
                    || Box!(modifier = Modifier.size(24.dp())),
                    || {},
                    modifier = Modifier.test_tag(TAG),
                    expanded = read.get()
                );
            });
        });
    });
    let open = bounds(&mut ui, TAG).width();
    assert!(open > 96.0, "{open}");
    expanded.set(false);
    ui.advance_time_by(Duration::from_millis(2000));
    let fab = bounds(&mut ui, TAG);
    assert_eq!((fab.width(), fab.height()), (96.0, 96.0));
    assert!(!ui.dump_tree().contains("cleared"), "{}", ui.dump_tree());
    expanded.set(true);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert!(ui.dump_tree().contains("cleared"), "{}", ui.dump_tree());
    ui.advance_time_by(Duration::from_millis(2000));
    assert_eq!(bounds(&mut ui, TAG).width(), open);
}

// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/FloatingActionButton.kt, mediumExtendedFabShape
#[test]
fn the_shapes_are_the_tokens() {
    let _ui = rule(|| {
        let shapes = MaterialTheme::shapes();
        assert_eq!(FloatingActionButtonDefaults::medium_shape(), ShapeDefaults::LARGE_INCREASED);
        assert_eq!(FloatingActionButtonDefaults::small_extended_fab_shape(), shapes.large);
        assert_eq!(
            FloatingActionButtonDefaults::medium_extended_fab_shape(),
            ShapeDefaults::LARGE_INCREASED
        );
        assert_eq!(FloatingActionButtonDefaults::large_extended_fab_shape(), shapes.extra_large);
    });
}

/// `mediumShape` and `mediumExtendedFabShape` are the constant, whatever the theme's shapes
/// (Compose's "TODO: update to use token").
#[test]
fn the_medium_shapes_do_not_follow_the_theme() {
    let _ui = rule(|| {
        let shapes = Shapes { large_increased: ShapeDefaults::EXTRA_LARGE, ..Shapes::default() };
        MaterialTheme!(shapes = shapes, {
            assert_eq!(
                FloatingActionButtonDefaults::medium_shape(),
                ShapeDefaults::LARGE_INCREASED
            );
            assert_eq!(
                FloatingActionButtonDefaults::medium_extended_fab_shape(),
                ShapeDefaults::LARGE_INCREASED
            );
        });
    });
}

/// `ElevationTokens.Level1`, `Level2` hovered.
#[test]
fn the_lowered_elevation_is_the_tokens() {
    let lowered = FloatingActionButtonDefaults::lowered_elevation();
    assert_eq!(lowered.default_elevation, compose_ui::Dp(1.0));
    assert_eq!(lowered.pressed_elevation, compose_ui::Dp(1.0));
    assert_eq!(lowered.focused_elevation, compose_ui::Dp(1.0));
    assert_eq!(lowered.hovered_elevation, compose_ui::Dp(3.0));
}

// ---- animate_floating_action_button -----------------------------------------------------------

fn linear(millis: u32) -> AnimationSpec {
    AnimationSpec::Tween(TweenSpec {
        duration_millis: millis,
        delay_millis: 0,
        easing: Easing::Linear,
    })
}

/// A box filling a 100 dp one, shown or hidden towards `alignment`.
fn shown(visible: bool, alignment: Alignment) -> (ComposeTestRule, OwnedState<bool>) {
    let state = mutable_state_of(visible);
    let read = state.handle();
    let ui = ComposeTestRule::with_manual_clock(move || {
        MaterialTheme!({
            Box!(modifier = Modifier.size(100.dp()), {
                Box!(
                    modifier = Modifier
                        .animate_floating_action_button_with(
                            read.get(),
                            alignment,
                            0.2,
                            Some(linear(100)),
                            Some(linear(100)),
                        )
                        .background(Color::BLUE)
                        .fill_max_size()
                );
            });
        });
    });
    (ui, state)
}

/// How the FAB's drawn: its alpha, scale and pivot in its node's pixels, `(1, 1, ..)` as it is.
fn layer(ui: &mut ComposeTestRule) -> (f32, f32, f32, f32) {
    let mut found = (1.0, 1.0, 0.0, 0.0);
    for op in &ui.host().display_list().ops {
        match op {
            DrawOp::Transform { scale, pivot, .. } => {
                (found.1, found.2, found.3) = (scale.0, pivot.x, pivot.y);
            }
            DrawOp::SaveLayerAlpha { alpha, .. } => found.0 = *alpha,
            _ => {}
        }
    }
    found
}

fn near(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.03, "{actual} is not {expected}");
}

/// Halfway through hiding: half faded, 0.6 scaled, around the alignment's pixel.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/FloatingActionButtonTest.kt, animateFloatingActionButton_hideBottomEnd_scalesAndFadesCorrectly
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/FloatingActionButtonTest.kt, animateFloatingActionButton_hideCenter_scalesAndFadesCorrectly
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/FloatingActionButtonTest.kt, animateFloatingActionButton_hideTopStart_scalesAndFadesCorrectly
#[test]
fn hiding_scales_and_fades_towards_its_alignment() {
    for (alignment, pivot) in
        [(Alignment::BOTTOM_END, 99.0), (Alignment::CENTER, 50.0), (Alignment::TOP_START, 0.0)]
    {
        let (mut ui, visible) = shown(true, alignment);
        visible.set(false);
        ui.advance_time_by_frame();
        ui.advance_time_by(Duration::from_millis(50));
        let (alpha, scale, x, y) = layer(&mut ui);
        near(alpha, 0.5);
        near(scale, 0.6);
        assert_eq!((x.round(), y.round()), (pivot, pivot), "{alignment:?}");
    }
}

/// Right to left, the end is the left.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/FloatingActionButton.kt, FabVisibleNode
#[test]
fn right_to_left_the_alignment_follows() {
    use compose_ui::{LOCAL_LAYOUT_DIRECTION, LayoutDirection};
    let visible = mutable_state_of(true);
    let read = visible.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Rtl), || {
            MaterialTheme!({
                Box!(modifier = Modifier.size(100.dp()), {
                    Box!(
                        modifier = Modifier
                            .animate_floating_action_button_with(
                                read.get(),
                                Alignment::BOTTOM_END,
                                0.2,
                                Some(linear(100)),
                                Some(linear(100)),
                            )
                            .fill_max_size()
                            .background(Color::BLUE)
                    );
                });
            });
        });
    });
    visible.set(false);
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(50));
    let (_, _, x, y) = layer(&mut ui);
    assert_eq!((x.round(), y.round()), (0.0, 99.0));
}

/// Shown at the end: nothing scaled or faded. Hidden at first: no room.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/FloatingActionButtonTest.kt, animateFloatingActionButton_show_noScaleOrFadeAfterAnimation
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/FloatingActionButtonTest.kt, animateFloatingActionButton_show_noScaleOrFadeBeforeAnimation
#[test]
fn showing_ends_as_it_was() {
    let (mut ui, visible) = shown(false, Alignment::BOTTOM_END);
    let drawn = |ui: &mut ComposeTestRule| {
        ui.host().display_list().ops.iter().find_map(|op| match op {
            DrawOp::DrawRect(rect, paint) if paint.color == Color::BLUE => Some(rect.width()),
            _ => None,
        })
    };
    assert_eq!(drawn(&mut ui).unwrap_or(0.0), 0.0, "nothing drawn");
    visible.set(true);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(100));
    let (alpha, scale, _, _) = layer(&mut ui);
    assert_eq!((alpha, scale), (1.0, 1.0));
    assert_eq!(drawn(&mut ui), Some(100.0));
}

/// A FAB's on what's beneath it while shown, and takes the click.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/FloatingActionButtonTest.kt, animateFloatingActionButton_show_consumesClick
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/FloatingActionButtonTest.kt, animateFloatingActionButton_hide_doesNotConsumesClick
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/FloatingActionButtonTest.kt, animateFloatingActionButton_hide_doesNotCrashWithDensityRounding
#[test]
fn only_a_shown_fab_takes_the_click() {
    let check = |visible: bool, density: f32, fab_clicked: bool| {
        let content = Rc::new(Cell::new(false));
        let fab = Rc::new(Cell::new(false));
        let (c, f) = (content.clone(), fab.clone());
        let state = mutable_state_of(true);
        let read = state.handle();
        let mut ui = rule(move || {
            let (c, f) = (c.clone(), f.clone());
            CompositionLocalProvider(
                LOCAL_DENSITY.provides(compose_ui::Density::new(density)),
                move || {
                    let (c, f) = (c.clone(), f.clone());
                    Box!(modifier = Modifier.size(100.dp()).test_tag(TAG), {
                        let (c, f) = (c.clone(), f.clone());
                        Box!(modifier = Modifier.fill_max_size().clickable(move || c.set(true)));
                        Box!(
                            modifier = Modifier
                                .animate_floating_action_button_with(
                                    read.get(),
                                    Alignment::BOTTOM_END,
                                    0.2,
                                    Some(linear(100)),
                                    Some(linear(100)),
                                )
                                .fill_max_size()
                                .clickable(move || f.set(true))
                        );
                    });
                },
            );
        });
        state.set(visible);
        ui.wait_for_idle();
        ui.on_node_with_tag(TAG).perform_click();
        assert_eq!((content.get(), fab.get()), (!fab_clicked, fab_clicked));
    };
    check(true, 1.0, true);
    check(false, 1.0, false);
    check(false, 1.1, false);
}

/// With no specs, the motion scheme's, read where it's composed.
#[test]
fn a_show_or_hide_with_no_specs_takes_the_schemes() {
    let visible = mutable_state_of(true);
    let read = visible.handle();
    let mut ui = rule(move || {
        Box!(
            modifier = Modifier
                .size(50.dp())
                .animate_floating_action_button(read.get(), Alignment::CENTER)
                .background(Color::BLUE)
        );
    });
    ui.wait_for_idle();
    visible.set(false);
    ui.wait_for_idle();
    let width = ui.host().display_list().ops.iter().find_map(|op| match op {
        DrawOp::DrawRect(rect, paint) if paint.color == Color::BLUE => Some(rect.width()),
        _ => None,
    });
    assert_eq!(width.unwrap_or(0.0), 0.0);
}
