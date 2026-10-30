//! Compose's `SurfaceTest.kt`, adapted: tonal elevation, absolute elevation, and the clickable,
//! selectable and toggleable overloads. The screenshot checks of the surface's color read the
//! display list's fills instead.
//!
//! Not ported: the three `*_smallTouchTarget_clickOutsideShapeBounds` tests (a click outside a
//! 40 dp circle, in its 48 dp touch target: compose-ui hit tests a node's bounds only, ui's
//! feature:hit-test-extensions) and `clickable_interactionSource` (dep:interaction-stream).

use std::cell::Cell;

use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::{Color, Dp, Modifier, UnitExt};

/// The colors of the rectangles the display list fills, in drawing order.
fn rect_fills(ui: &mut ComposeTestRule) -> Vec<Color> {
    ui.host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawRect(_, paint) => Some(paint.color),
            _ => None,
        })
        .collect()
}

thread_local! {
    static ELEVATION: Cell<Option<Dp>> = const { Cell::new(None) };
}

fn record_elevation() {
    ELEVATION.with(|cell| cell.set(Some(LOCAL_ABSOLUTE_TONAL_ELEVATION.current())));
}

fn recorded_elevation() -> Dp {
    ELEVATION.with(Cell::get).expect("composed")
}

/// The baseline light scheme with a surface tint of its own, so a tint taken from `primary`
/// shows.
fn tinted_scheme() -> ColorScheme {
    ColorScheme { surface_tint: Color::from_argb(0xFF00FF00), ..light_color_scheme() }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SurfaceTest.kt, noTonalElevationColorIsSetOnNonElevatedSurfaceColor
#[test]
fn a_surface_without_elevation_keeps_the_surface_color() {
    let mut ui = compose_test(|| {
        MaterialTheme!(color_scheme = tinted_scheme(), {
            Surface!(modifier = Modifier.size(10.dp()), {
                record_elevation();
            });
        });
    });
    assert_eq!(recorded_elevation(), Dp(0.0));
    assert_eq!(rect_fills(&mut ui), vec![tinted_scheme().surface]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SurfaceTest.kt, tonalElevationColorIsSetOnElevatedSurfaceColor
// (with a surface tint differing from the primary color, so the tint's role shows)
#[test]
fn an_elevated_surface_is_tinted_with_the_surface_tint() {
    let mut ui = compose_test(|| {
        MaterialTheme!(color_scheme = tinted_scheme(), {
            Surface!(tonal_elevation = 2.dp(), modifier = Modifier.size(10.dp()), {
                record_elevation();
            });
        });
    });
    assert_eq!(recorded_elevation(), Dp(2.0));
    let scheme = tinted_scheme();
    let alpha = (4.5 * 3.0f32.ln() + 2.0) / 100.0;
    let expected = scheme.surface_tint.with_alpha(alpha).composite_over(scheme.surface);
    assert_eq!(scheme.surface_color_at_elevation(2.dp()), expected);
    assert_eq!(rect_fills(&mut ui), vec![expected]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SurfaceTest.kt, noTonalElevationColorIsSetOnElevatedSurfaceColor_tonalElevationDisabled
#[test]
fn with_tonal_elevation_turned_off_an_elevated_surface_keeps_the_surface_color() {
    let mut ui = compose_test(|| {
        MaterialTheme!(color_scheme = tinted_scheme(), {
            CompositionLocalProvider(LOCAL_TONAL_ELEVATION_ENABLED.provides(false), || {
                Surface!(tonal_elevation = 2.dp(), modifier = Modifier.size(10.dp()), {
                    record_elevation();
                });
            });
        });
    });
    // The elevation still adds up; the color isn't tinted.
    assert_eq!(recorded_elevation(), Dp(2.0));
    assert_eq!(rect_fills(&mut ui), vec![tinted_scheme().surface]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SurfaceTest.kt, tonalElevationColorIsNotSetOnNonSurfaceColor
#[test]
fn an_elevated_surface_of_another_color_is_not_tinted() {
    let green = Color::from_argb(0xFF00FF00);
    let mut ui = compose_test(move || {
        MaterialTheme!({
            Surface!(color = green, tonal_elevation = 2.dp(), modifier = Modifier.size(10.dp()), {
                record_elevation();
            });
        });
    });
    assert_eq!(recorded_elevation(), Dp(2.0));
    assert_eq!(rect_fills(&mut ui), vec![green]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SurfaceTest.kt, absoluteElevationCompositionLocalIsSet
#[test]
fn nested_surfaces_add_their_elevations_up() {
    thread_local! { static OUTER: Cell<Option<Dp>> = const { Cell::new(None) }; }
    compose_test(|| {
        MaterialTheme!({
            Surface!(tonal_elevation = 2.dp(), {
                OUTER.with(|cell| cell.set(Some(LOCAL_ABSOLUTE_TONAL_ELEVATION.current())));
                Surface!(tonal_elevation = 4.dp(), {
                    record_elevation();
                });
            });
        });
    });
    assert_eq!(OUTER.with(Cell::get), Some(Dp(2.0)));
    assert_eq!(recorded_elevation(), Dp(6.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SurfaceTest.kt, contentColorSetBeforeModifier
// (compose-rs has no composed modifiers: the content color is read in the content)
#[test]
fn a_surface_provides_its_content_color() {
    thread_local! { static CONTENT: Cell<Option<Color>> = const { Cell::new(None) }; }
    compose_test(|| {
        MaterialTheme!({
            CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(Color::RED), || {
                Surface!(tonal_elevation = 2.dp(), content_color = Color::BLUE, {
                    CONTENT.with(|cell| cell.set(Some(LOCAL_CONTENT_COLOR.current())));
                });
            });
        });
    });
    assert_eq!(CONTENT.with(Cell::get), Some(Color::BLUE));
}

// ---- The clickable, selectable and toggleable overloads -------------------------------------

/// Which overload a test exercises, as the test file's three groups.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Clickable,
    Selectable,
    Toggleable,
}

/// A surface of `kind` tagged "surface", in `shape`, `role` given by its modifier if any,
/// showing its state as text (the clicks for a clickable one, on or off for the others) over a
/// `size` spacer; each click counts.
fn overload(
    kind: Kind,
    shape: compose_ui::Shape,
    role: Option<compose_ui::Role>,
    size: f32,
) -> (ComposeTestRule, OwnedState<i32>) {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let ui = compose_test(move || {
        let shape = shape.clone();
        MaterialTheme!({
            let shape = shape.clone();
            let modifier = match role {
                Some(role) => Modifier.semantics(move |config| config.set_role(role)),
                None => Modifier,
            }
            .test_tag("surface");
            let on = handle.get() % 2 == 1;
            let content = move || {
                let label =
                    if kind == Kind::Clickable { handle.get().to_string() } else { on.to_string() };
                Text!("{}", label);
                compose_foundation::layout::Spacer(Modifier.size(size.dp()));
            };
            let click = move || handle.set(handle.get() + 1);
            match kind {
                Kind::Clickable => {
                    ClickableSurface!(click, modifier = modifier, shape = shape.clone(), {
                        content()
                    });
                }
                Kind::Selectable => {
                    SelectableSurface!(on, click, modifier = modifier, shape = shape.clone(), {
                        content()
                    });
                }
                Kind::Toggleable => {
                    ToggleableSurface!(
                        on,
                        move |_| click(),
                        modifier = modifier,
                        shape = shape.clone(),
                        { content() }
                    );
                }
            }
        });
    });
    (ui, state)
}

/// The surface clicks, has no role of its own (or the one its modifier gives) and merges its
/// text, which follows the clicks.
fn check_semantics(kind: Kind, role: Option<compose_ui::Role>) {
    let (mut ui, _state) = overload(kind, compose_ui::RECTANGLE_SHAPE, role, 30.0);
    let (before, after) = if kind == Kind::Clickable { ("0", "1") } else { ("false", "true") };
    let node = ui.on_node_with_tag("surface").fetch_semantics_snapshot();
    assert_eq!(node.config.get(&compose_ui::SemanticsProperties::ROLE).copied(), role);
    ui.on_node_with_tag("surface")
        .assert_has_click_action()
        .assert_is_enabled()
        .assert_text_equals(&[before])
        .perform_click()
        .assert_text_equals(&[after]);
}

/// A click in the shape's bounds clicks; one at (10, 10), outside a 100 dp circle's, doesn't.
fn check_click_outside_the_shape(kind: Kind) {
    let (mut ui, state) = overload(kind, compose_foundation::CIRCLE_SHAPE, None, 100.0);
    ui.on_node_with_tag("surface").perform_click();
    assert_eq!(state.get(), 1);
    ui.on_node_with_tag("surface")
        .perform_touch_input(|touch| touch.click_at(compose_ui::Offset::new(10.0, 10.0)));
    assert_eq!(state.get(), 1);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SurfaceTest.kt, clickable_semantics
#[test]
fn a_clickable_surface_clicks_and_merges_its_content() {
    check_semantics(Kind::Clickable, None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SurfaceTest.kt, clickable_withCustomSemantics
#[test]
fn a_clickable_surface_takes_the_role_its_modifier_gives() {
    check_semantics(Kind::Clickable, Some(compose_ui::Role::Checkbox));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SurfaceTest.kt, clickable_clickAction
#[test]
fn a_clickable_surface_calls_back_on_each_click() {
    let (mut ui, state) = overload(Kind::Clickable, compose_ui::RECTANGLE_SHAPE, None, 30.0);
    ui.on_node_with_tag("surface").perform_click();
    assert_eq!(state.get(), 1);
    ui.on_node_with_tag("surface").perform_click().perform_click();
    assert_eq!(state.get(), 3);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SurfaceTest.kt, clickable_clickOutsideShapeBounds
#[test]
fn a_click_outside_a_round_clickable_surface_misses() {
    check_click_outside_the_shape(Kind::Clickable);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SurfaceTest.kt, clickable_enabled_disabled
#[test]
fn a_disabled_clickable_modifier_on_a_surface_doesnt_click() {
    let count = mutable_state_of(0);
    let enabled = mutable_state_of(true);
    let (count_handle, enabled_handle) = (count.handle(), enabled.handle());
    let mut ui = compose_test(move || {
        MaterialTheme!({
            let options = compose_ui::ClickableOptions {
                enabled: enabled_handle.get(),
                indication: compose_ui::Indication::NONE,
                ..compose_ui::ClickableOptions::DEFAULT
            };
            Surface!(
                modifier = Modifier
                    .test_tag("surface")
                    .clickable_with(options, move || count_handle.set(count_handle.get() + 1)),
                {
                    compose_foundation::layout::Spacer(Modifier.size(30.dp()));
                }
            );
        });
    });
    ui.on_node_with_tag("surface").assert_is_enabled().perform_click();
    assert_eq!(count.get(), 1);
    enabled.set(false);
    ui.on_node_with_tag("surface").assert_is_not_enabled().perform_click().perform_click();
    assert_eq!(count.get(), 1);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SurfaceTest.kt, selectable_semantics
#[test]
fn a_selectable_surface_clicks_and_merges_its_content() {
    check_semantics(Kind::Selectable, None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SurfaceTest.kt, selectable_customSemantics
#[test]
fn a_selectable_surface_takes_the_role_its_modifier_gives() {
    check_semantics(Kind::Selectable, Some(compose_ui::Role::Switch));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SurfaceTest.kt, selectable_clickAction
#[test]
fn a_selectable_surface_flips_on_each_click() {
    let (mut ui, state) = overload(Kind::Selectable, compose_ui::RECTANGLE_SHAPE, None, 30.0);
    ui.on_node_with_tag("surface").perform_click();
    assert_eq!(state.get() % 2, 1);
    ui.on_node_with_tag("surface").perform_click();
    assert_eq!(state.get() % 2, 0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SurfaceTest.kt, selectable_clickOutsideShapeBounds
#[test]
fn a_click_outside_a_round_selectable_surface_misses() {
    check_click_outside_the_shape(Kind::Selectable);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SurfaceTest.kt, toggleable_semantics
#[test]
fn a_toggleable_surface_clicks_and_merges_its_content() {
    check_semantics(Kind::Toggleable, None);
    let (mut ui, _state) = overload(Kind::Toggleable, compose_ui::RECTANGLE_SHAPE, None, 30.0);
    ui.on_node_with_tag("surface").assert_is_off().perform_click().assert_is_on();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SurfaceTest.kt, toggleable_customSemantics
#[test]
fn a_toggleable_surface_takes_the_role_its_modifier_gives() {
    check_semantics(Kind::Toggleable, Some(compose_ui::Role::Tab));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SurfaceTest.kt, toggleable_toggleAction
#[test]
fn a_toggleable_surface_toggles_on_each_click() {
    let (mut ui, state) = overload(Kind::Toggleable, compose_ui::RECTANGLE_SHAPE, None, 30.0);
    ui.on_node_with_tag("surface").perform_click();
    assert_eq!(state.get() % 2, 1);
    ui.on_node_with_tag("surface").perform_click();
    assert_eq!(state.get() % 2, 0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SurfaceTest.kt, toggleable_clickOutsideShapeBounds
#[test]
fn a_click_outside_a_round_toggleable_surface_misses() {
    check_click_outside_the_shape(Kind::Toggleable);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SurfaceTest.kt, clickable_interactionSource
#[test]
fn a_clickable_surface_emits_its_press_and_release() {
    use compose_ui::{Interaction, MutableInteractionSource, PressInteraction};
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let given = given.clone();
        MaterialTheme!({
            ClickableSurface!(
                || {},
                modifier = Modifier.test_tag("surface"),
                interaction_source = Some(given.clone()),
                {
                    compose_foundation::layout::Spacer(Modifier.size(30.dp()));
                }
            );
        });
    });
    let seen: std::rc::Rc<std::cell::RefCell<Vec<Interaction>>> = Default::default();
    let sink = seen.clone();
    let interactions = source.interactions();
    compose_runtime::platform::launch(async move {
        interactions.collect(move |interaction| sink.borrow_mut().push(interaction)).await
    });
    ui.advance_time_by_frame();
    assert!(seen.borrow().is_empty());
    ui.on_node_with_tag("surface").perform_touch_input(|touch| {
        let center = touch.center();
        touch.down(center);
    });
    ui.advance_time_by(std::time::Duration::from_millis(100));
    let press = match seen.borrow().as_slice() {
        [Interaction::Press(PressInteraction::Press(press))] => *press,
        other => panic!("{other:?}"),
    };
    ui.on_node_with_tag("surface").perform_touch_input(|touch| touch.up());
    ui.advance_time_by_frame();
    match seen.borrow().as_slice() {
        [_, Interaction::Press(PressInteraction::Release(released))] => {
            assert_eq!(*released, press)
        }
        other => panic!("{other:?}"),
    };
}
