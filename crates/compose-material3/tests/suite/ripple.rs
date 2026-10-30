//! Compose's `RippleTest`, `RippleModifierNodeTest` and `RippleAnimationTest`, adapted: what a
//! ripple draws, read from the display list instead of pixels. A press grows a circle; hovers,
//! focuses and drags show a state layer, or a focus an inset ring. Compose's goldens' expected
//! colors are each interaction's opacity on the ripple's color.
//!
//! Not ported: `RippleModifierNodeTest.pressed_rippleLazilyDelegatedTo` (its test node delegating
//! to a ripple later, which compose-rs's nodes can't yet), `RippleContainerTest` and
//! `RippleHostViewTest` (Android's views).

use std::rc::Rc;
use std::time::Duration;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::{
    Color, Density, DragInteraction, DragStart, Focus, FocusInteraction, HoverEnter,
    HoverInteraction, Indication, Modifier, MutableInteractionSource, Offset, Press,
    PressInteraction, Shape, UnitExt,
};

const TAG: &str = "ripple";
/// A 40 dp box's half diagonal, and with a bounded ripple's 10 dp.
const HALF_DIAGONAL: f32 = 28.284271;
const BOUNDED_RADIUS: f32 = HALF_DIAGONAL + 10.0;

/// What a ripple drew: a circle (in the ripple box's coordinates) of a color, clipped or not.
#[derive(Clone, Debug)]
struct Circle {
    center: (f32, f32),
    radius: f32,
    color: Color,
    clipped: bool,
}

/// The 40 dp box showing `indication`, in a theme of `content`'s color.
fn themed(
    source: &MutableInteractionSource,
    indication: impl Fn() -> Indication + 'static,
    configuration: impl Fn() -> Option<RippleConfiguration> + 'static,
    theme: impl Fn() -> RippleThemeConfiguration + 'static,
    content: impl Fn() -> Color + 'static,
) -> ComposeTestRule {
    let source = source.clone();
    let indication = Rc::new(indication);
    let (configuration, theme, content) =
        (Rc::new(configuration), Rc::new(theme), Rc::new(content));
    ComposeTestRule::with_manual_clock(move || {
        let (source, indication, configuration, theme, content) = (
            source.clone(),
            indication.clone(),
            configuration.clone(),
            theme.clone(),
            content.clone(),
        );
        MaterialTheme!({
            let (source, indication) = (source.clone(), indication.clone());
            CompositionLocalProvider(
                (
                    LOCAL_CONTENT_COLOR.provides(content()),
                    LOCAL_RIPPLE_CONFIGURATION.provides(configuration()),
                    LOCAL_RIPPLE_THEME_CONFIGURATION.provides(theme()),
                ),
                move || {
                    Box!(
                        modifier = Modifier
                            .size(40.dp())
                            .test_tag(TAG)
                            .indication(source.clone(), indication())
                    );
                },
            );
        });
    })
}

/// `themed` in black, with the default configurations.
fn plain(source: &MutableInteractionSource, indication: Indication) -> ComposeTestRule {
    themed(
        source,
        move || indication.clone(),
        || Some(RippleConfiguration::default()),
        || RippleDefaults::THEME_CONFIGURATION,
        || Color::BLACK,
    )
}

fn circles(ui: &mut ComposeTestRule) -> Vec<Circle> {
    let ops = ui.host().display_list().ops;
    let mut found = Vec::new();
    let mut clips = 0;
    for op in &ops {
        match op {
            DrawOp::ClipRect(_) => clips += 1,
            DrawOp::Save => {}
            DrawOp::Restore => clips = 0,
            DrawOp::DrawArc { oval, paint, .. } => found.push(Circle {
                center: ((oval.left + oval.right) / 2.0, (oval.top + oval.bottom) / 2.0),
                radius: (oval.right - oval.left) / 2.0,
                color: paint.color,
                clipped: clips > 0,
            }),
            _ => {}
        }
    }
    found
}

fn one_circle(ui: &mut ComposeTestRule) -> Circle {
    let found = circles(ui);
    assert_eq!(found.len(), 1, "{found:?}: {:?}", ui.host().display_list().ops);
    found[0].clone()
}

fn assert_near(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.01, "{actual} is not {expected}");
}

fn assert_circle(ui: &mut ComposeTestRule, radius: f32, color: Color, clipped: bool) {
    let circle = one_circle(ui);
    assert_near(circle.radius, radius);
    assert_near(circle.center.0, 20.0);
    assert_near(circle.center.1, 20.0);
    assert_eq!(circle.color, color, "{circle:?}");
    assert_eq!(circle.clipped, clipped, "{circle:?}");
}

fn press_at(source: &MutableInteractionSource, at: Offset) -> Press {
    let press = Press::new(at);
    source.try_emit(PressInteraction::Press(press));
    press
}

fn settle(ui: &mut ComposeTestRule) {
    ui.advance_time_by(Duration::from_millis(500));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RippleTest.kt, bounded_lightTheme_pressed
// Compose: compose/material3/material3-ripple/src/androidDeviceTest/kotlin/androidx/compose/material3/ripple/RippleModifierNodeTest.kt, pressed
#[test]
fn a_bounded_press_ends_centered_at_its_full_radius() {
    let source = MutableInteractionSource::new();
    let mut ui = plain(&source, default_ripple());
    press_at(&source, Offset::new(10.0, 10.0));
    settle(&mut ui);
    assert_circle(&mut ui, BOUNDED_RADIUS, Color::BLACK.with_alpha(0.1), true);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RippleTest.kt, bounded_lightTheme_hovered
#[test]
fn a_bounded_hover_shows_a_state_layer() {
    let source = MutableInteractionSource::new();
    let mut ui = plain(&source, default_ripple());
    source.try_emit(HoverInteraction::Enter(HoverEnter::new()));
    settle(&mut ui);
    assert_circle(&mut ui, BOUNDED_RADIUS, Color::BLACK.with_alpha(0.08), true);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RippleTest.kt, bounded_lightTheme_focused
#[test]
fn a_bounded_focus_shows_a_state_layer() {
    let source = MutableInteractionSource::new();
    let mut ui = plain(&source, default_ripple());
    source.try_emit(FocusInteraction::Focus(Focus::new()));
    settle(&mut ui);
    assert_circle(&mut ui, BOUNDED_RADIUS, Color::BLACK.with_alpha(0.1), true);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RippleTest.kt, bounded_lightTheme_dragged
#[test]
fn a_bounded_drag_shows_a_state_layer() {
    let source = MutableInteractionSource::new();
    let mut ui = plain(&source, default_ripple());
    source.try_emit(DragInteraction::Start(DragStart::new()));
    settle(&mut ui);
    assert_circle(&mut ui, BOUNDED_RADIUS, Color::BLACK.with_alpha(0.16), true);
}

fn unbounded() -> Indication {
    ripple(false, compose_ui::Dp::UNSPECIFIED, Color::UNSPECIFIED)
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RippleTest.kt, unbounded_lightTheme_pressed
#[test]
fn an_unbounded_press_covers_its_bounds_unclipped() {
    let source = MutableInteractionSource::new();
    let mut ui = plain(&source, unbounded());
    press_at(&source, Offset::new(10.0, 10.0));
    settle(&mut ui);
    assert_circle(&mut ui, HALF_DIAGONAL, Color::BLACK.with_alpha(0.1), false);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RippleTest.kt, unbounded_lightTheme_hovered
#[test]
fn an_unbounded_hover_shows_a_state_layer_unclipped() {
    let source = MutableInteractionSource::new();
    let mut ui = plain(&source, unbounded());
    source.try_emit(HoverInteraction::Enter(HoverEnter::new()));
    settle(&mut ui);
    assert_circle(&mut ui, HALF_DIAGONAL, Color::BLACK.with_alpha(0.08), false);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RippleTest.kt, unbounded_lightTheme_focused
#[test]
fn an_unbounded_focus_shows_a_state_layer_unclipped() {
    let source = MutableInteractionSource::new();
    let mut ui = plain(&source, unbounded());
    source.try_emit(FocusInteraction::Focus(Focus::new()));
    settle(&mut ui);
    assert_circle(&mut ui, HALF_DIAGONAL, Color::BLACK.with_alpha(0.1), false);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RippleTest.kt, unbounded_lightTheme_dragged
#[test]
fn an_unbounded_drag_shows_a_state_layer_unclipped() {
    let source = MutableInteractionSource::new();
    let mut ui = plain(&source, unbounded());
    source.try_emit(DragInteraction::Start(DragStart::new()));
    settle(&mut ui);
    assert_circle(&mut ui, HALF_DIAGONAL, Color::BLACK.with_alpha(0.16), false);
}

/// An explicit radius is the state layer's and the ripple's.
#[test]
fn a_given_radius_is_the_ripples() {
    let source = MutableInteractionSource::new();
    let mut ui = plain(&source, ripple(false, 15.dp(), Color::UNSPECIFIED));
    source.try_emit(HoverInteraction::Enter(HoverEnter::new()));
    settle(&mut ui);
    assert_circle(&mut ui, 15.0, Color::BLACK.with_alpha(0.08), false);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RippleTest.kt, contentColorChangeDuringRipple_dragged
// Compose: compose/material3/material3-ripple/src/androidDeviceTest/kotlin/androidx/compose/material3/ripple/RippleModifierNodeTest.kt, colorChangeDuringRipple_dragged
#[test]
fn a_content_color_change_shows_at_once_during_a_drag() {
    let source = MutableInteractionSource::new();
    let color = mutable_state_of(Color::RED);
    let handle = color.handle();
    let mut ui = themed(
        &source,
        default_ripple,
        || Some(RippleConfiguration::default()),
        || RippleDefaults::THEME_CONFIGURATION,
        move || handle.get(),
    );
    source.try_emit(DragInteraction::Start(DragStart::new()));
    settle(&mut ui);
    assert_eq!(one_circle(&mut ui).color, Color::RED.with_alpha(0.16));
    color.set(Color::GREEN);
    ui.advance_time_by_frame();
    assert_eq!(one_circle(&mut ui).color, Color::GREEN.with_alpha(0.16));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RippleTest.kt, rippleConfiguration_color_dragged
#[test]
fn a_configurations_color_wins_over_the_content_color() {
    let source = MutableInteractionSource::new();
    let mut ui = themed(
        &source,
        default_ripple,
        || Some(RippleConfiguration { color: Color::RED, focus: None }),
        || RippleDefaults::THEME_CONFIGURATION,
        || Color::BLACK,
    );
    source.try_emit(DragInteraction::Start(DragStart::new()));
    settle(&mut ui);
    assert_eq!(one_circle(&mut ui).color, Color::RED.with_alpha(0.16));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RippleTest.kt, rippleConfiguration_color_explicitColorSet_dragged
#[test]
fn a_ripples_own_color_wins_over_the_configurations() {
    let source = MutableInteractionSource::new();
    let mut ui = themed(
        &source,
        || ripple(true, compose_ui::Dp::UNSPECIFIED, Color::BLUE),
        || Some(RippleConfiguration { color: Color::RED, focus: None }),
        || RippleDefaults::THEME_CONFIGURATION,
        || Color::BLACK,
    );
    source.try_emit(DragInteraction::Start(DragStart::new()));
    settle(&mut ui);
    assert_eq!(one_circle(&mut ui).color, Color::BLUE.with_alpha(0.16));
}

/// The `ColorProducer` overloads: read as it draws, so a state it reads draws again.
#[test]
fn a_color_producer_is_read_as_it_draws() {
    let source = MutableInteractionSource::new();
    let color = mutable_state_of(Color::RED);
    let handle = color.handle();
    let producer: Rc<dyn Fn() -> Color> = Rc::new(move || handle.get());
    let indication =
        ripple_with(RippleOptions { color_producer: Some(producer), ..RippleOptions::default() });
    let mut ui = plain(&source, indication);
    source.try_emit(DragInteraction::Start(DragStart::new()));
    settle(&mut ui);
    assert_eq!(one_circle(&mut ui).color, Color::RED.with_alpha(0.16));
    color.set(Color::GREEN);
    ui.advance_time_by_frame();
    assert_eq!(one_circle(&mut ui).color, Color::GREEN.with_alpha(0.16));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RippleTest.kt, rippleConfiguration_disabled_dragged
#[test]
fn no_configuration_draws_no_ripple() {
    let source = MutableInteractionSource::new();
    let mut ui = themed(
        &source,
        default_ripple,
        || None,
        || RippleDefaults::THEME_CONFIGURATION,
        || Color::BLACK,
    );
    source.try_emit(DragInteraction::Start(DragStart::new()));
    press_at(&source, Offset::new(10.0, 10.0));
    settle(&mut ui);
    assert!(circles(&mut ui).is_empty());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RippleTest.kt, rippleConfigurationChangeDuringRipple_dragged
#[test]
fn a_configuration_change_shows_at_once_during_a_drag() {
    let source = MutableInteractionSource::new();
    let configuration = mutable_state_of(RippleConfiguration { color: Color::RED, focus: None });
    let handle = configuration.handle();
    let mut ui = themed(
        &source,
        default_ripple,
        move || Some(handle.get()),
        || RippleDefaults::THEME_CONFIGURATION,
        || Color::BLACK,
    );
    source.try_emit(DragInteraction::Start(DragStart::new()));
    settle(&mut ui);
    assert_eq!(one_circle(&mut ui).color, Color::RED.with_alpha(0.16));
    configuration.set(RippleConfiguration { color: Color::GREEN, focus: None });
    ui.advance_time_by_frame();
    assert_eq!(one_circle(&mut ui).color, Color::GREEN.with_alpha(0.16));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RippleTest.kt, rippleConfigurationToggleBetweenEnabledAndDisabled
#[test]
fn a_configuration_turned_off_and_on_again_starts_from_nothing() {
    let source = MutableInteractionSource::new();
    let enabled = mutable_state_of(true);
    let handle = enabled.handle();
    let mut ui = themed(
        &source,
        default_ripple,
        move || handle.get().then(RippleConfiguration::default),
        || RippleDefaults::THEME_CONFIGURATION,
        || Color::BLACK,
    );
    let start = DragStart::new();
    source.try_emit(DragInteraction::Start(start));
    settle(&mut ui);
    assert_eq!(one_circle(&mut ui).color, Color::BLACK.with_alpha(0.16));
    enabled.set(false);
    settle(&mut ui);
    assert!(circles(&mut ui).is_empty());
    enabled.set(true);
    settle(&mut ui);
    assert!(circles(&mut ui).is_empty(), "what's under way isn't heard again");
    let again = DragStart::new();
    source.try_emit(DragInteraction::Start(again));
    settle(&mut ui);
    assert_eq!(one_circle(&mut ui).color, Color::BLACK.with_alpha(0.16));
}

// Compose: compose/material3/material3-ripple/src/androidDeviceTest/kotlin/androidx/compose/material3/ripple/RippleModifierNodeTest.kt, focusedHoveredThenUnhovered
#[test]
fn the_most_recent_interaction_shows() {
    let source = MutableInteractionSource::new();
    let mut ui = plain(&source, default_ripple());
    source.try_emit(FocusInteraction::Focus(Focus::new()));
    settle(&mut ui);
    assert_eq!(one_circle(&mut ui).color, Color::BLACK.with_alpha(0.1));
    let enter = HoverEnter::new();
    source.try_emit(HoverInteraction::Enter(enter));
    settle(&mut ui);
    assert_eq!(one_circle(&mut ui).color, Color::BLACK.with_alpha(0.08));
    source.try_emit(HoverInteraction::Exit(enter));
    settle(&mut ui);
    assert_eq!(one_circle(&mut ui).color, Color::BLACK.with_alpha(0.1));
}

/// A state layer's fade: a drag in over 45 ms, out over 150 ms, and then nothing.
#[test]
fn a_drags_state_layer_fades_in_and_out_linearly() {
    let source = MutableInteractionSource::new();
    let mut ui = plain(&source, default_ripple());
    let start = DragStart::new();
    source.try_emit(DragInteraction::Start(start));
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(32));
    let partial = one_circle(&mut ui).color.alpha() as f32 / 255.0;
    assert!(partial > 0.0 && partial < 0.16, "{partial}");
    settle(&mut ui);
    assert_eq!(one_circle(&mut ui).color, Color::BLACK.with_alpha(0.16));
    source.try_emit(DragInteraction::Stop(start));
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(64));
    let fading = one_circle(&mut ui).color.alpha() as f32 / 255.0;
    assert!(fading > 0.0 && fading < 0.16, "{fading}");
    settle(&mut ui);
    assert!(circles(&mut ui).is_empty());
}

/// `RippleAnimation`: from where it pressed, 30 % of the longer side, growing to the center and
/// full radius, its fade in over 75 ms; held; on release, gone after 150 ms.
#[test]
fn a_ripple_grows_from_the_press_to_the_center_and_fades_on_release() {
    let source = MutableInteractionSource::new();
    let mut ui = plain(&source, default_ripple());
    let press = press_at(&source, Offset::new(10.0, 10.0));
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    let early = one_circle(&mut ui);
    assert!(early.center.0 < 12.0, "{early:?}");
    assert!(early.radius < 16.0 && early.radius >= 12.0, "{early:?}");
    assert!(early.color.alpha() < 26, "{early:?}");
    settle(&mut ui);
    let held = one_circle(&mut ui);
    assert_near(held.radius, BOUNDED_RADIUS);
    assert_near(held.center.0, 20.0);
    assert!(!ui.host().needs_frame(), "held, it asks for no frames");
    source.try_emit(PressInteraction::Release(press));
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(80));
    let fading = one_circle(&mut ui);
    assert!(fading.color.alpha() > 0 && fading.color.alpha() < 26, "{fading:?}");
    ui.advance_time_by(Duration::from_millis(200));
    assert!(circles(&mut ui).is_empty());
    assert!(!ui.host().needs_frame());
}

/// Released while fading in, it shows whole at once, and finishes growing before it fades.
#[test]
fn a_ripple_released_while_fading_in_shows_whole() {
    let source = MutableInteractionSource::new();
    let mut ui = plain(&source, default_ripple());
    let press = press_at(&source, Offset::new(10.0, 10.0));
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    source.try_emit(PressInteraction::Release(press));
    ui.advance_time_by_frame();
    let whole = one_circle(&mut ui);
    assert_eq!(whole.color, Color::BLACK.with_alpha(0.1), "{whole:?}");
    assert!(whole.radius < BOUNDED_RADIUS);
}

/// A second press finishes the first: both show while it fades.
#[test]
fn a_second_press_finishes_the_first() {
    let source = MutableInteractionSource::new();
    let mut ui = plain(&source, default_ripple());
    press_at(&source, Offset::new(10.0, 10.0));
    settle(&mut ui);
    assert_eq!(circles(&mut ui).len(), 1);
    let second = press_at(&source, Offset::new(30.0, 30.0));
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(32));
    assert_eq!(circles(&mut ui).len(), 2);
    settle(&mut ui);
    assert_eq!(circles(&mut ui).len(), 1, "the first has gone, the second held");
    source.try_emit(PressInteraction::Release(second));
    settle(&mut ui);
    assert!(circles(&mut ui).is_empty());
}

// Compose: compose/material3/material3-ripple/src/androidDeviceTest/kotlin/androidx/compose/material3/ripple/RippleModifierNodeTest.kt, pressed_rippleCreatedBeforeDraw
#[test]
fn a_press_emitted_before_the_first_frame_still_shows() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let shown = mutable_state_of(false);
    let handle = shown.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let given = given.clone();
        MaterialTheme!({
            if handle.get() {
                Box!(modifier = Modifier.size(40.dp()).indication(given.clone(), default_ripple()));
            }
        });
    });
    shown.set(true);
    ui.advance_time_by_frame();
    press_at(&source, Offset::new(10.0, 10.0));
    settle(&mut ui);
    assert_near(one_circle(&mut ui).radius, BOUNDED_RADIUS);
}

// Compose: compose/material3/material3-ripple/src/androidDeviceTest/kotlin/androidx/compose/material3/ripple/RippleModifierNodeTest.kt, boundsIncreaseDuringRipple_pressed
#[test]
fn bounds_growing_during_a_press_keep_the_ripples_radius_and_move_the_state_layer() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let size = mutable_state_of(40.0f32);
    let handle = size.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let given = given.clone();
        MaterialTheme!({
            Box!(
                modifier =
                    Modifier.size(handle.get().dp()).indication(given.clone(), default_ripple())
            );
        });
    });
    press_at(&source, Offset::new(10.0, 10.0));
    settle(&mut ui);
    assert_near(one_circle(&mut ui).radius, BOUNDED_RADIUS);
    size.set(80.0);
    settle(&mut ui);
    let grown = one_circle(&mut ui);
    assert_near(grown.center.0, 40.0);
    // The press's own radius, fixed when it first drew, as `targetRadius` given to `addRipple`.
    assert_near(grown.radius, BOUNDED_RADIUS);
}

/// Left and come back, it starts from nothing, as a new collection.
#[test]
fn a_removed_ripple_shows_nothing_when_it_comes_back() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let given = given.clone();
        MaterialTheme!({
            if handle.get() {
                Box!(modifier = Modifier.size(40.dp()).indication(given.clone(), default_ripple()));
            }
        });
    });
    source.try_emit(DragInteraction::Start(DragStart::new()));
    settle(&mut ui);
    assert_eq!(circles(&mut ui).len(), 1);
    shown.set(false);
    settle(&mut ui);
    assert!(circles(&mut ui).is_empty());
    shown.set(true);
    settle(&mut ui);
    assert!(circles(&mut ui).is_empty());
}

fn inset_theme() -> RippleThemeConfiguration {
    RippleDefaults::INSET_FOCUS_RING_THEME_CONFIGURATION
}

/// The colors of the ring's borders: what's drawn apart from a state layer.
fn ring_colors(ui: &mut ComposeTestRule) -> Vec<Color> {
    ui.host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawRect(_, paint)
            | DrawOp::DrawRoundRect(_, _, paint)
            | DrawOp::DrawPath { paint, .. } => Some(paint.color),
            _ => None,
        })
        .collect()
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RippleTest.kt, insetFocusRing_focused_defaultColors
#[test]
fn an_inset_ring_shows_in_the_schemes_secondary_colors_instead_of_a_state_layer() {
    let source = MutableInteractionSource::new();
    let mut ui = themed(
        &source,
        default_ripple,
        || Some(RippleConfiguration::default()),
        inset_theme,
        || Color::BLACK,
    );
    let focus = Focus::new();
    source.try_emit(FocusInteraction::Focus(focus));
    settle(&mut ui);
    assert!(circles(&mut ui).is_empty());
    let scheme = light_color_scheme();
    assert_eq!(ring_colors(&mut ui), [scheme.on_secondary, scheme.secondary]);
    source.try_emit(FocusInteraction::Unfocus(focus));
    settle(&mut ui);
    assert!(ring_colors(&mut ui).is_empty());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RippleTest.kt, rippleConfiguration_insetFocusRing_focused
#[test]
fn a_configurations_ring_colors_win() {
    let source = MutableInteractionSource::new();
    let mut ui = themed(
        &source,
        default_ripple,
        || {
            Some(RippleConfiguration {
                color: Color::UNSPECIFIED,
                focus: Some(RippleConfigurationFocus::InsetRing {
                    outer_stroke_color: Color::RED,
                    inner_stroke_color: Color::BLUE,
                }),
            })
        },
        inset_theme,
        || Color::BLACK,
    );
    source.try_emit(FocusInteraction::Focus(Focus::new()));
    settle(&mut ui);
    assert_eq!(ring_colors(&mut ui), [Color::BLUE, Color::RED]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RippleTest.kt, rippleConfiguration_insetFocusRing_genericShape_focused
#[test]
fn a_generic_shapes_ring_is_a_path() {
    let source = MutableInteractionSource::new();
    let shape = Shape::Cut([compose_ui::CornerSize::Dp(8.dp()); 4]);
    let indication =
        ripple_with(RippleOptions { focus_ring_shape: Some(shape), ..RippleOptions::default() });
    let mut ui = themed(
        &source,
        move || indication.clone(),
        || Some(RippleConfiguration::default()),
        inset_theme,
        || Color::BLACK,
    );
    source.try_emit(FocusInteraction::Focus(Focus::new()));
    settle(&mut ui);
    let scheme = light_color_scheme();
    assert_eq!(ring_colors(&mut ui), [scheme.on_secondary, scheme.secondary]);
}

/// A rounded shape's ring: rounded rectangles inset as the theme says.
#[test]
fn a_round_shapes_ring_is_inset_by_the_theme() {
    let source = MutableInteractionSource::new();
    let shape = Shape::Rounded([compose_ui::CornerSize::Dp(10.dp()); 4]);
    let indication =
        ripple_with(RippleOptions { focus_ring_shape: Some(shape), ..RippleOptions::default() });
    let mut ui = themed(
        &source,
        move || indication.clone(),
        || Some(RippleConfiguration::default()),
        inset_theme,
        || Color::BLACK,
    );
    source.try_emit(FocusInteraction::Focus(Focus::new()));
    settle(&mut ui);
    let rects: Vec<(f32, f32)> = ui
        .host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawRoundRect(rect, radii, _) => Some((rect.left, radii[0])),
            _ => None,
        })
        .collect();
    // A stroke's rect is centered in its band: inner 1 + 3 / 2, outer 0 + 2 / 2.
    assert_eq!(rects.len(), 2, "{rects:?}");
    assert_near(rects[0].0, 2.5);
    assert_near(rects[1].0, 1.0);
}

/// The enable flags: what's off shows nothing, the rest as usual.
#[test]
fn an_interaction_turned_off_shows_nothing() {
    type Case = (fn(&mut RippleOptions), fn(&MutableInteractionSource));
    let cases: [Case; 4] = [
        (
            |options| options.enable_press_indication = false,
            |source| {
                press_at(source, Offset::new(10.0, 10.0));
            },
        ),
        (
            |options| options.enable_hover_indication = false,
            |source| {
                source.try_emit(HoverInteraction::Enter(HoverEnter::new()));
            },
        ),
        (
            |options| options.enable_focus_indication = false,
            |source| {
                source.try_emit(FocusInteraction::Focus(Focus::new()));
            },
        ),
        (
            |options| options.enable_drag_indication = false,
            |source| {
                source.try_emit(DragInteraction::Start(DragStart::new()));
            },
        ),
    ];
    for (turn_off, interact) in cases {
        let source = MutableInteractionSource::new();
        let mut options = RippleOptions::default();
        turn_off(&mut options);
        let mut ui = plain(&source, ripple_with(options));
        interact(&source);
        settle(&mut ui);
        assert!(circles(&mut ui).is_empty());
    }
}

// Compose: compose/material3/material3-ripple/src/androidHostTest/kotlin/androidx/compose/material3/ripple/RippleAnimationTest.kt, testStartRadius
#[test]
fn a_ripple_starts_at_30_percent_of_the_longer_side() {
    assert_near(ripple_start_radius(100.0, 160.0), 48.0);
    assert_near(ripple_start_radius(160.0, 100.0), 48.0);
}

// Compose: compose/material3/material3-ripple/src/androidHostTest/kotlin/androidx/compose/material3/ripple/RippleAnimationTest.kt, testEndRadiusBounded
#[test]
fn a_bounded_ripple_ends_10_dp_past_the_corners() {
    let density = Density::new(2.0);
    let half = (100.0f32 * 100.0 + 160.0 * 160.0).sqrt() / 2.0;
    assert_near(ripple_end_radius(&density, true, 100.0, 160.0), half + 20.0);
}

// Compose: compose/material3/material3-ripple/src/androidHostTest/kotlin/androidx/compose/material3/ripple/RippleAnimationTest.kt, testEndRadiusUnbounded
#[test]
fn an_unbounded_ripple_ends_at_the_corners() {
    let density = Density::new(1.0);
    let half = (140.0f32 * 140.0 + 100.0 * 100.0).sqrt() / 2.0;
    assert_near(ripple_end_radius(&density, false, 140.0, 100.0), half);
}

/// At rest a clickable under Material's theme draws what one without an indication does: not an
/// operation more, so a screen's display list doesn't change.
#[test]
fn a_clickable_at_rest_draws_nothing_of_its_ripple() {
    fn screen(indication: Indication) -> Vec<String> {
        let mut ui = compose_test(move || {
            let indication = indication.clone();
            MaterialTheme!({
                Box!(
                    modifier = Modifier.size(40.dp()).clickable_with(
                        compose_ui::ClickableOptions {
                            indication: indication.clone(),
                            ..compose_ui::ClickableOptions::DEFAULT
                        },
                        || {},
                    )
                );
            });
        });
        ui.host().display_list().ops.iter().map(|op| format!("{op:?}")).collect()
    }
    assert_eq!(screen(default_ripple()), screen(Indication::NONE));
    assert_eq!(screen(Indication::LOCAL), screen(Indication::NONE));
}

/// `MaterialTheme` provides the ripple: a plain clickable in it shows one, in the content color
/// where it is.
#[test]
fn a_clickable_in_the_theme_shows_a_ripple_in_the_content_color_where_it_is() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {
        MaterialTheme!({
            CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(Color::MAGENTA), || {
                Box!(modifier = Modifier.size(40.dp()).test_tag(TAG).clickable(|| {}));
            });
        });
    });
    let center = ui.on_node_with_tag(TAG).fetch_semantics_node().center();
    let touch = compose_ui::PointerEvent::press(center)
        .with_pointer(compose_ui::PointerId(0), compose_ui::PointerType::Touch);
    ui.host().send_pointer(touch);
    settle(&mut ui);
    assert_eq!(one_circle(&mut ui).color, Color::MAGENTA.with_alpha(0.1));
}

/// `content` in the inset ring's theme, focused by the node tagged `tag`: what it draws of the
/// ring, the scheme's two colors in that order.
fn assert_rings_when_focused(tag: &str, content: impl Fn() + 'static) {
    let content = Rc::new(content);
    let mut ui = compose_test(move || {
        let content = content.clone();
        MaterialTheme!({
            let content = content.clone();
            CompositionLocalProvider(
                LOCAL_RIPPLE_THEME_CONFIGURATION
                    .provides(RippleDefaults::INSET_FOCUS_RING_THEME_CONFIGURATION),
                move || content(),
            );
        });
    });
    let scheme = light_color_scheme();
    let count = |ui: &mut ComposeTestRule, color: Color| {
        ui.host()
            .display_list()
            .ops
            .iter()
            .filter(|op| match op {
                DrawOp::DrawRect(_, paint)
                | DrawOp::DrawRoundRect(_, _, paint)
                | DrawOp::DrawPath { paint, .. } => paint.color == color,
                _ => false,
            })
            .count()
    };
    let before = (count(&mut ui, scheme.on_secondary), count(&mut ui, scheme.secondary));
    let circles_before = circles(&mut ui).len();
    ui.on_node_with_tag(tag).request_focus();
    ui.wait_for_idle();
    assert_eq!(
        (count(&mut ui, scheme.on_secondary), count(&mut ui, scheme.secondary)),
        (before.0 + 1, before.1 + 1),
        "{tag}: {:#?}",
        ui.host().display_list().ops
    );
    // And a ring is all that shows of the focus: no state layer.
    assert_eq!(circles(&mut ui).len(), circles_before, "{tag}");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ButtonScreenshotTest.kt, default_Button_lightTheme_focused_insetFocusRings
#[test]
fn a_focused_button_shows_the_inset_ring() {
    assert_rings_when_focused("it", || {
        Button!(on_click = || {}, modifier = Modifier.test_tag("it"), { Text!("Button") });
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ButtonScreenshotTest.kt, outlinedButton_lightTheme_focused_insetFocusRings
#[test]
fn a_focused_outlined_button_shows_the_inset_ring() {
    assert_rings_when_focused("it", || {
        OutlinedButton!(on_click = || {}, modifier = Modifier.test_tag("it"), { Text!("Button") });
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxScreenshotTest.kt, checkBox_focus_insetFocusRings
#[test]
fn a_focused_checkbox_shows_the_inset_ring() {
    assert_rings_when_focused("it", || {
        Checkbox!(true, Some(|_| {}), modifier = Modifier.test_tag("it"));
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CardScreenshotTest.kt, filledCard_focused_insetFocusRings
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CardScreenshotTest.kt, elevatedCard_focused_insetFocusRings
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CardScreenshotTest.kt, outlinedCard_focused_insetFocusRings
#[test]
fn a_focused_card_shows_the_inset_ring() {
    assert_rings_when_focused("it", || {
        Card!(on_click = Some(|| {}), modifier = Modifier.test_tag("it").size(40.dp()), {});
    });
    assert_rings_when_focused("it", || {
        ElevatedCard!(on_click = Some(|| {}), modifier = Modifier.test_tag("it").size(40.dp()), {});
    });
    assert_rings_when_focused("it", || {
        OutlinedCard!(on_click = Some(|| {}), modifier = Modifier.test_tag("it").size(40.dp()), {});
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/FloatingActionButtonScreenshotTest.kt, fab_focused_insetFocusRings
#[test]
fn a_focused_fab_shows_the_inset_ring() {
    assert_rings_when_focused("it", || {
        FloatingActionButton!(|| {}, modifier = Modifier.test_tag("it"), {});
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonScreenshotTest.kt, iconButton_lightTheme_focused_insetFocusRings
#[test]
fn a_focused_icon_button_shows_the_inset_ring() {
    assert_rings_when_focused("it", || {
        IconButton!(|| {}, modifier = Modifier.test_tag("it"), {});
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RadioButtonScreenshotTest.kt, radioButton_lightTheme_focused_insetFocusRings
#[test]
fn a_focused_radio_button_shows_the_inset_ring() {
    assert_rings_when_focused("it", || {
        RadioButton!(true, Some(|| {}), modifier = Modifier.test_tag("it"));
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/NavigationBarScreenshotTest.kt, navigationBar_firstItem_focused_insetFocusRings
#[test]
fn a_focused_navigation_bar_item_shows_the_inset_ring() {
    assert_rings_when_focused("it", || {
        NavigationBar!(|row: RowScope| {
            NavigationBarItem!(row, false, || {}, {}, modifier = Modifier.test_tag("it"));
        });
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/NavigationRailScreenshotTest.kt, navigationRail_firstItem_focused_insetFocusRings
#[test]
fn a_focused_navigation_rail_item_shows_the_inset_ring() {
    assert_rings_when_focused("it", || {
        NavigationRail!({
            NavigationRailItem!(false, || {}, {}, modifier = Modifier.test_tag("it"));
        });
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchScreenshotTest.kt, switchTest_focus_insetFocusRings
#[test]
fn a_focused_switch_shows_the_inset_ring_on_its_track() {
    assert_rings_when_focused("it", || {
        Switch!(true, Some(|_| {}), modifier = Modifier.test_tag("it"));
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, slider_focused_insetFocusRings
#[test]
fn a_focused_slider_shows_the_inset_ring() {
    let state = SliderState::new(0.5, 0, (0.0, 1.0));
    let handle = *state;
    assert_rings_when_focused("it", move || {
        Slider!(handle, move |new| handle.set_value(new), modifier = Modifier.test_tag("it"));
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ListItemScreenshotTest.kt, listItem_focused_insetFocusRings
#[test]
fn a_focused_list_item_shows_the_inset_ring() {
    assert_rings_when_focused("it", || {
        ClickableListItem!(|| {}, modifier = Modifier.test_tag("it"), { Text!("Item") });
    });
}
