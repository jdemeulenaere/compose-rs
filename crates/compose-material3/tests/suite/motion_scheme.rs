//! Compose's `MotionSchemeTest.kt` and the `respectsMaterialThemeMotionScheme` tests of
//! `BottomSheetTest.kt` and `BottomSheetScaffoldTest.kt`, adapted, and what each component animates
//! with. Compose's mapping of tokens to animations has no test of its own but those: here, a
//! scheme with one token long and the others short shows, in the time an interaction takes, which
//! tokens that interaction animates with.
//!
//! `tokenValue` compares `MotionSchemeKeyTokens`' values with the scheme's; the tokens aren't a type
//! here (a component reads the scheme), so the scheme is compared with the token files' numbers.

use std::rc::Rc;
use std::time::Duration;

use compose_animation::{AnimationSpec, Easing, SpringSpec};
use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Color, CornerSize, Focus, FocusInteraction, Modifier, MutableInteractionSource, Outline,
    PaintStyle, Shape, UnitExt, platform::DrawOp,
};

#[derive(Clone, Copy, PartialEq, Debug)]
enum Token {
    DefaultSpatial,
    FastSpatial,
    SlowSpatial,
    DefaultEffects,
    FastEffects,
    SlowEffects,
}
use Token::*;

const TOKENS: [Token; 6] =
    [DefaultSpatial, FastSpatial, SlowSpatial, DefaultEffects, FastEffects, SlowEffects];

/// How long the token under test takes, and the most any other animation of a scenario does (a
/// checkbox's 100 ms snap, a state layer's 45 ms).
const LONG: u64 = 800;
const SHORT: u64 = 300;

fn tween_of(millis: u32) -> AnimationSpec {
    compose_animation::tween(millis).easing(Easing::Linear).into()
}

/// A scheme with `token` long, the others short.
fn long_only(token: Token) -> MotionScheme {
    let spec = |of: Token| tween_of(if of == token { LONG as u32 } else { 16 });
    MotionScheme {
        default_spatial: spec(DefaultSpatial),
        fast_spatial: spec(FastSpatial),
        slow_spatial: spec(SlowSpatial),
        default_effects: spec(DefaultEffects),
        fast_effects: spec(FastEffects),
        slow_effects: spec(SlowEffects),
    }
}

/// `content` in a theme of `scheme`, settled.
fn themed(scheme: MotionScheme, content: impl Fn() + 'static) -> ComposeTestRule {
    let content = Rc::new(content);
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let content = content.clone();
        MaterialTheme!(motion_scheme = scheme.clone(), { content() });
    });
    idle_after(&mut ui);
    ui
}

/// How long until it stops asking for frames, from now.
fn idle_after(ui: &mut ComposeTestRule) -> u64 {
    let mut millis = 0;
    ui.advance_time_by_frame();
    while ui.host().needs_frame() {
        ui.advance_time_by(Duration::from_millis(16));
        millis += 16;
        assert!(millis < 5_000, "it never settles");
    }
    millis
}

/// `scenario`, given a scheme with each token long in turn, takes long with `expected` only.
#[track_caller]
fn assert_animates_with(expected: &[Token], scenario: impl Fn(MotionScheme) -> u64) {
    for token in TOKENS {
        let millis = scenario(long_only(token));
        if expected.contains(&token) {
            assert!(millis >= LONG - 32, "{millis} ms with {token:?} long, one of {expected:?}");
        } else {
            assert!(millis < SHORT, "{millis} ms with {token:?} long, not one of {expected:?}");
        }
    }
}

/// `scenario` gives, `LONG / 2` into an interaction, how far along each property is (0 to 1), one
/// for each of `tokens`, the token that property animates with: with its token long it's on its way,
/// with any other it has arrived. The time alone can't say which animation uses which token.
#[track_caller]
fn assert_each_property_animates_with(
    tokens: &[Token],
    scenario: impl Fn(MotionScheme) -> Vec<f32>,
) {
    for token in TOKENS {
        let fractions = scenario(long_only(token));
        assert_eq!(fractions.len(), tokens.len());
        for (index, (fraction, its)) in fractions.iter().zip(tokens).enumerate() {
            if *its == token {
                assert!(
                    (0.1..0.9).contains(fraction),
                    "property {index} ({its:?}) at {fraction} with {token:?} long: {fractions:?}"
                );
            } else {
                assert!(
                    (*fraction - 1.0).abs() < 0.01,
                    "property {index} ({its:?}) at {fraction} with {token:?} long: {fractions:?}"
                );
            }
        }
    }
}

/// The frame that starts the animations, then half of `LONG`.
fn to_halfway(ui: &mut ComposeTestRule) {
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(LONG / 2));
}

/// How far from `start` to `end` `value` is.
fn along(value: f32, start: f32, end: f32) -> f32 {
    if start == end { 1.0 } else { (value - start) / (end - start) }
}

/// How far from `start` to `end` `color` is, by the channel that differs most (animations
/// interpolate in another space, so it isn't exact: only whether it's on its way or there).
fn color_along(color: Color, start: Color, end: Color) -> f32 {
    let channels = |color: Color| {
        [color.alpha() as f32, color.red() as f32, color.green() as f32, color.blue() as f32]
    };
    let (value, from, to) = (channels(color), channels(start), channels(end));
    let index = (0..4)
        .max_by(|a, b| {
            (to[*a] - from[*a]).abs().partial_cmp(&(to[*b] - from[*b]).abs()).expect("numbers")
        })
        .expect("channels");
    along(value[index], from[index], to[index])
}

/// The scale and alpha of the layer in the frame, 1 without one: a popup's or an
/// `AnimatedVisibility`'s.
fn layer_scale_and_alpha(ui: &mut ComposeTestRule) -> (f32, f32) {
    let ops = ui.host().display_list().ops.clone();
    let scale = ops.iter().find_map(|op| match op {
        DrawOp::Transform { scale, .. } => Some(scale.0),
        _ => None,
    });
    let alpha = ops.iter().find_map(|op| match op {
        DrawOp::SaveLayerAlpha { alpha, .. } => Some(*alpha),
        _ => None,
    });
    (scale.unwrap_or(1.0), alpha.unwrap_or(1.0))
}

fn top_of(ui: &mut ComposeTestRule, tag: &str) -> f32 {
    ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_node().bounds.top
}

fn spring(damping_ratio: f32, stiffness: f32) -> AnimationSpec {
    AnimationSpec::Spring(SpringSpec { damping_ratio, stiffness, visibility_threshold: None })
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/MotionSchemeTest.kt, tokenValue
#[test]
fn a_theme_gives_the_standard_tokens_by_default() {
    thread_local! { static READ: std::cell::RefCell<Option<Rc<MotionScheme>>> = const { std::cell::RefCell::new(None) }; }
    let _ui = compose_test(|| {
        MaterialTheme!({
            READ.with(|cell| *cell.borrow_mut() = Some(MaterialTheme::motion_scheme()));
        });
    });
    let scheme = READ.with(|cell| cell.borrow().clone()).expect("composed");
    // `StandardMotionTokens`.
    assert_eq!(scheme.default_spatial_spec(), spring(0.9, 700.0));
    assert_eq!(scheme.fast_spatial_spec(), spring(0.9, 1400.0));
    assert_eq!(scheme.slow_spatial_spec(), spring(0.9, 300.0));
    assert_eq!(scheme.default_effects_spec(), spring(1.0, 1600.0));
    assert_eq!(scheme.fast_effects_spec(), spring(1.0, 3800.0));
    assert_eq!(scheme.slow_effects_spec(), spring(1.0, 800.0));
}

/// `ExpressiveMotionTokens`.
#[test]
fn the_expressive_scheme_has_its_tokens() {
    let scheme = MotionScheme::expressive();
    assert_eq!(scheme.default_spatial_spec(), spring(0.8, 380.0));
    assert_eq!(scheme.fast_spatial_spec(), spring(0.6, 800.0));
    assert_eq!(scheme.slow_spatial_spec(), spring(0.8, 200.0));
    assert_eq!(scheme.default_effects_spec(), spring(1.0, 1600.0));
    assert_eq!(scheme.fast_effects_spec(), spring(1.0, 3800.0));
    assert_eq!(scheme.slow_effects_spec(), spring(1.0, 800.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/MotionSchemeTest.kt, readMotionScheme
#[test]
fn a_nested_theme_overrides_the_scheme_or_keeps_the_enclosing_one() {
    thread_local! {
        static READ: std::cell::RefCell<Vec<Rc<MotionScheme>>> = const { std::cell::RefCell::new(Vec::new()) };
    }
    let read = || READ.with(|cell| cell.borrow_mut().push(MaterialTheme::motion_scheme()));
    let _ui = compose_test(move || {
        MaterialTheme!({
            read();
            MaterialTheme!(motion_scheme = MotionScheme::expressive(), {
                read();
                MaterialTheme!(color_scheme = dark_color_scheme(), {
                    read();
                });
            });
            MaterialTheme!(shapes = Shapes::default(), {
                read();
            });
        });
    });
    let schemes = READ.with(|cell| cell.borrow().clone());
    let expected = [
        MotionScheme::standard(),
        MotionScheme::expressive(),
        MotionScheme::expressive(),
        MotionScheme::standard(),
    ];
    assert_eq!(schemes.iter().map(|it| (**it).clone()).collect::<Vec<_>>(), expected);
}

/// As `MaterialExpressiveTheme`: expressive by default, the enclosing expressive theme's inside
/// one, what's given wins.
#[test]
fn an_expressive_theme_is_expressive_unless_given_one() {
    thread_local! {
        static READ: std::cell::RefCell<Vec<Rc<MotionScheme>>> = const { std::cell::RefCell::new(Vec::new()) };
    }
    let read = || READ.with(|cell| cell.borrow_mut().push(MaterialTheme::motion_scheme()));
    let _ui = compose_test(move || {
        MaterialExpressiveTheme!({
            read();
            MaterialExpressiveTheme!(motion_scheme = Some(MotionScheme::standard()), {
                read();
                MaterialExpressiveTheme!({
                    read();
                });
            });
        });
        MaterialExpressiveTheme!(motion_scheme = Some(MotionScheme::standard()), {
            read();
        });
    });
    let schemes = READ.with(|cell| cell.borrow().clone());
    let expected = [
        MotionScheme::expressive(),
        MotionScheme::standard(),
        MotionScheme::standard(),
        MotionScheme::standard(),
    ];
    assert_eq!(schemes.iter().map(|it| (**it).clone()).collect::<Vec<_>>(), expected);
}

/// A new scheme equal to the last recomposes no reader, as the theme's other values (D1021).
#[test]
fn an_equal_scheme_made_anew_recomposes_no_reader() {
    use std::cell::Cell;
    thread_local! { static READS: Cell<u32> = const { Cell::new(0) }; }
    let tick = mutable_state_of(0);
    let handle = tick.handle();
    let mut ui = compose_test(move || {
        let _ = handle.get();
        MaterialTheme!(motion_scheme = MotionScheme::standard(), {
            Reader();
        });
    });
    assert_eq!(READS.with(Cell::get), 1);
    tick.set(1);
    ui.wait_for_idle();
    assert_eq!(READS.with(Cell::get), 1);

    #[composable]
    #[allow(non_snake_case)]
    fn Reader() {
        let _ = MaterialTheme::motion_scheme();
        READS.with(|cell| cell.set(cell.get() + 1));
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetTest.kt, bottomSheet_respectsMaterialThemeMotionScheme
/// Expanding and settling with the default spatial spec, hiding with the fast effects one: read
/// from how long each takes, the specs being internal in Compose's.
#[test]
fn a_standalone_sheet_animates_with_the_scheme() {
    let run = |to: SheetValue, scheme: MotionScheme| {
        let state = SheetState::new(
            &SheetValue::ALL,
            || 56.0,
            || 125.0,
            SheetValue::PartiallyExpanded,
            |_| true,
        );
        let handle = *state;
        let mut ui = themed(scheme, move || {
            BottomSheet!(state = handle, |_| {
                Box!(modifier = Modifier.fill_max_width().height(600.dp()));
            });
        });
        compose_runtime::platform::launch(async move {
            if to == SheetValue::Expanded { handle.expand().await } else { handle.hide().await }
        });
        let millis = idle_after(&mut ui);
        assert_eq!(handle.current_value(), to);
        millis
    };
    assert_animates_with(&[DefaultSpatial], |scheme| run(SheetValue::Expanded, scheme));
    assert_animates_with(&[FastEffects], |scheme| run(SheetValue::Hidden, scheme));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_respectsMaterialThemeMotionScheme
#[test]
fn a_scaffolds_sheet_animates_with_the_scheme() {
    let run = |to: SheetValue, scheme: MotionScheme| {
        let from = if to == SheetValue::Expanded {
            SheetValue::PartiallyExpanded
        } else {
            SheetValue::Expanded
        };
        let values = [SheetValue::PartiallyExpanded, SheetValue::Expanded];
        let state = SheetState::new(&values, || 56.0, || 125.0, from, |_| true);
        let handle = *state;
        let mut ui = themed(scheme, move || {
            BottomSheetScaffold!(
                |_| {
                    Box!(modifier = Modifier.fill_max_width().height(600.dp()));
                },
                scaffold_state = remember_bottom_sheet_scaffold_state!(Some(handle)),
                { Text!("Content") }
            );
        });
        compose_runtime::platform::launch(async move {
            if to == SheetValue::Expanded {
                handle.expand().await
            } else {
                handle.partial_expand().await
            }
        });
        let millis = idle_after(&mut ui);
        assert_eq!(handle.current_value(), to);
        millis
    };
    assert_animates_with(&[DefaultSpatial], |scheme| run(SheetValue::Expanded, scheme));
    assert_animates_with(&[FastEffects], |scheme| run(SheetValue::PartiallyExpanded, scheme));
}

/// A swipe let go settles on the default spatial spec.
#[test]
fn a_sheet_let_go_settles_with_the_scheme() {
    assert_animates_with(&[DefaultSpatial], |scheme| {
        let state =
            SheetState::new(&SheetValue::ALL, || 56.0, || 125.0, SheetValue::Expanded, |_| true);
        let handle = *state;
        let mut ui = themed(scheme, move || {
            BottomSheet!(state = handle, drag_handle = None::<fn()>, |_| {
                Box!(modifier = Modifier.fill_max_width().height(600.dp()).test_tag("sheet"));
            });
        });
        ui.on_node_with_tag("sheet").perform_touch_input(|touch| {
            let center = touch.center();
            touch.down(center);
            touch.move_by(compose_ui::Offset::new(0.0, 40.0));
            touch.up();
        });
        idle_after(&mut ui)
    });
}

/// The sheet's top and its scrim's alpha (of `Color::RED`, given as its color), read where drawn.
fn scrim_alpha(ui: &mut ComposeTestRule) -> f32 {
    ui.host()
        .display_list()
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::DrawRect(_, paint) if paint.color.red() == 255 && paint.color.blue() == 0 => {
                Some(paint.color.alpha() as f32 / 255.0)
            }
            _ => None,
        })
        .unwrap_or(0.0)
}

/// A modal sheet, read halfway: the sheet on its show or hide spec, its scrim on default effects.
#[test]
fn a_modal_sheet_and_its_scrim_animate_with_the_scheme() {
    let sheet_of = |handle: SheetState| {
        ModalBottomSheet!(|| {}, sheet_state = handle, scrim_color = Color::RED, |_| {
            Box!(modifier = Modifier.fill_max_width().height(600.dp()).test_tag("sheet"));
        });
    };
    // Shown: from below the window to its place, the scrim from nothing.
    assert_each_property_animates_with(&[DefaultSpatial, DefaultEffects], |scheme| {
        let state =
            SheetState::new(&SheetValue::ALL, || 56.0, || 125.0, SheetValue::Hidden, |_| true);
        let handle = *state;
        let shown = mutable_state_of(false);
        let read = shown.handle();
        let mut ui = themed(scheme, move || {
            if read.get() {
                sheet_of(handle);
            }
        });
        shown.set(true);
        ui.advance_time_by_frame();
        to_halfway(&mut ui);
        let (top, alpha) = (top_of(&mut ui, "sheet"), scrim_alpha(&mut ui));
        idle_after(&mut ui);
        let (end_top, end_alpha) = (top_of(&mut ui, "sheet"), scrim_alpha(&mut ui));
        vec![along(top, 800.0, end_top), along(alpha, 0.0, end_alpha)]
    });
    // Hidden: back below the window, the scrim going.
    assert_each_property_animates_with(&[FastEffects, DefaultEffects], |scheme| {
        let state =
            SheetState::new(&SheetValue::ALL, || 56.0, || 125.0, SheetValue::Expanded, |_| true);
        let handle = *state;
        let mut ui = themed(scheme, move || sheet_of(handle));
        let (start_top, start_alpha) = (top_of(&mut ui, "sheet"), scrim_alpha(&mut ui));
        compose_runtime::platform::launch(async move { handle.hide().await });
        to_halfway(&mut ui);
        let (top, alpha) = (top_of(&mut ui, "sheet"), scrim_alpha(&mut ui));
        idle_after(&mut ui);
        let end_top = top_of(&mut ui, "sheet");
        vec![along(top, start_top, end_top), along(alpha, start_alpha, 0.0)]
    });
}

#[test]
fn a_switch_animates_with_the_scheme() {
    assert_animates_with(&[FastSpatial], |scheme| {
        let checked = mutable_state_of(false);
        let read = checked.handle();
        let mut ui = themed(scheme, move || {
            Switch!(read.get(), Some(|_| {}));
        });
        checked.set(true);
        idle_after(&mut ui)
    });
}

/// The dot on the fast spatial spec, the color on the default effects one, as `RadioButton`.
#[test]
fn a_radio_buttons_dot_and_color_animate_with_the_scheme() {
    assert_each_property_animates_with(&[FastSpatial, DefaultEffects], |scheme| {
        let selected = mutable_state_of(false);
        let read = selected.handle();
        let colors = RadioButtonColors {
            selected_color: Color::RED,
            unselected_color: Color::BLUE,
            disabled_selected_color: Color::RED,
            disabled_unselected_color: Color::BLUE,
        };
        let mut ui = themed(scheme, move || {
            RadioButton!(read.get(), Some(|| {}), colors = colors);
        });
        selected.set(true);
        to_halfway(&mut ui);
        let ops = ui.host().display_list().ops.clone();
        let mut ring = None;
        let mut dot = 0.0;
        for op in &ops {
            if let DrawOp::DrawRoundRect(_, radii, paint) = op {
                match paint.style {
                    PaintStyle::Stroke(_) => ring = Some(paint.color),
                    PaintStyle::Fill => dot = radii[0],
                }
            }
        }
        vec![along(dot, 0.0, 5.0), color_along(ring.expect("a ring"), Color::BLUE, Color::RED)]
    });
}

/// Checking: the mark's length on the default spatial spec, the box's color on default effects.
/// Unchecking: the color on fast effects, the length gone by a snap.
#[test]
fn a_checkboxs_mark_and_color_animate_with_the_scheme() {
    /// The box's color (filled, its border the same) and the length of the mark.
    fn read(ui: &mut ComposeTestRule) -> (Color, f32) {
        let ops = ui.host().display_list().ops.clone();
        let mut fill = None;
        let mut length = 0.0;
        for op in &ops {
            match op {
                DrawOp::DrawRoundRect(_, _, paint) => fill = Some(paint.color),
                DrawOp::DrawLine(from, to, _) => {
                    length += ((to.x - from.x).powi(2) + (to.y - from.y).powi(2)).sqrt();
                }
                _ => {}
            }
        }
        (fill.expect("a box"), length)
    }
    let run = |to: bool, scheme: MotionScheme| {
        let checked = mutable_state_of(!to);
        let state = checked.handle();
        let mut ui = themed(scheme, move || {
            let colors = CheckboxColors {
                checked_box_color: Color::RED,
                unchecked_box_color: Color::BLUE,
                checked_border_color: Color::RED,
                unchecked_border_color: Color::BLUE,
                ..CheckboxDefaults::colors()
            };
            Checkbox!(state.get(), Some(|_| {}), colors = colors);
        });
        let (start_color, start_length) = read(&mut ui);
        checked.set(to);
        to_halfway(&mut ui);
        let (color, length) = read(&mut ui);
        idle_after(&mut ui);
        let (end_color, end_length) = read(&mut ui);
        vec![along(length, start_length, end_length), color_along(color, start_color, end_color)]
    };
    assert_each_property_animates_with(&[DefaultSpatial, DefaultEffects], |scheme| {
        run(true, scheme)
    });
    // The mark's snap isn't the scheme's: with any token long, it's there.
    assert_animates_with(&[FastEffects], |scheme| {
        let checked = mutable_state_of(true);
        let state = checked.handle();
        let mut ui = themed(scheme, move || {
            Checkbox!(state.get(), Some(|_| {}));
        });
        checked.set(false);
        idle_after(&mut ui)
    });
    assert_each_property_animates_with(&[FastEffects], |scheme| {
        let values = run(false, scheme);
        vec![values[1]]
    });
}

/// The indicator on the default spatial spec; a newly selected tab's color on default effects, the
/// tab it leaves on fast effects, as `TabTransition`.
#[test]
fn a_tab_rows_indicator_and_colors_animate_with_the_scheme() {
    use std::cell::Cell;
    thread_local! {
        static COLORS: [Cell<Color>; 2] = const { [Cell::new(Color::UNSPECIFIED), Cell::new(Color::UNSPECIFIED)] };
    }
    assert_each_property_animates_with(&[DefaultSpatial, DefaultEffects, FastEffects], |scheme| {
        let selected = mutable_state_of(0usize);
        let read = selected.handle();
        let mut ui = themed(scheme, move || {
            PrimaryTabRow!(
                read.get(),
                indicator = Some(move |scope: TabIndicatorScope| {
                    Box!(
                        modifier = Modifier
                            .tab_indicator_offset(&scope, read.get(), false)
                            .height(3.dp())
                            .test_tag("indicator")
                    );
                }),
                {
                    for index in 0..2 {
                        Tab!(
                            read.get() == index,
                            || {},
                            text = Some(move || {
                                COLORS.with(|colors| {
                                    colors[index].set(LOCAL_CONTENT_COLOR.current());
                                });
                                Text!("Tab {}", index);
                            }),
                            selected_content_color = Color::RED,
                            unselected_content_color = Color::BLUE
                        );
                    }
                }
            );
        });
        let start_left =
            ui.on_node_unmerged(has_test_tag("indicator")).fetch_semantics_node().bounds.left;
        selected.set(1);
        to_halfway(&mut ui);
        let left =
            ui.on_node_unmerged(has_test_tag("indicator")).fetch_semantics_node().bounds.left;
        let (leaving, coming) = COLORS.with(|colors| (colors[0].get(), colors[1].get()));
        idle_after(&mut ui);
        let end_left =
            ui.on_node_unmerged(has_test_tag("indicator")).fetch_semantics_node().bounds.left;
        vec![
            along(left, start_left, end_left),
            color_along(coming, Color::BLUE, Color::RED),
            color_along(leaving, Color::RED, Color::BLUE),
        ]
    });
}

/// A scrollable row brings the selected tab into view on the default spatial spec, as
/// `ScrollableTabData`'s `animationSpec`: not there halfway through it, there at its end.
#[test]
fn a_scrollable_tab_row_scrolls_with_the_scheme() {
    let selected = mutable_state_of(0usize);
    let read = selected.handle();
    let scroll = compose_foundation::ScrollState::new(0);
    let handle = *scroll;
    let mut ui = themed(long_only(DefaultSpatial), move || {
        PrimaryScrollableTabRow!(read.get(), scroll_state = handle, {
            for index in 0..20 {
                Tab!(read.get() == index, || {}, text = Some(move || Text!("Tab {}", index)));
            }
        });
    });
    selected.set(19);
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(LONG / 2));
    let halfway = handle.value();
    idle_after(&mut ui);
    assert!(halfway > 0, "it has started");
    assert!(halfway < handle.value(), "{halfway} is not short of {}", handle.value());
}

/// What `NavigationBarItem` and `NavigationRailItem` animate as they're selected: the icon's and
/// the label's colors and the indicator's alpha on the default effects spec, the indicator's width
/// on the fast spatial one.
#[test]
fn a_navigation_items_selection_animates_with_the_scheme() {
    use std::cell::Cell;
    thread_local! {
        static COLORS: [Cell<Color>; 2] = const { [Cell::new(Color::UNSPECIFIED), Cell::new(Color::UNSPECIFIED)] };
    }
    fn colors() -> NavigationBarItemColors {
        NavigationBarItemColors {
            selected_icon_color: Color::RED,
            unselected_icon_color: Color::BLUE,
            selected_text_color: Color::RED,
            unselected_text_color: Color::BLUE,
            selected_indicator_color: Color::GREEN,
            ..NavigationBarItemDefaults::colors()
        }
    }
    fn indicator(ui: &mut ComposeTestRule) -> (f32, f32) {
        let list = ui.host().display_list();
        let rect = list.ops.iter().find_map(|op| match op {
            DrawOp::DrawRoundRect(rect, _, paint) if paint.color == Color::GREEN => Some(*rect),
            _ => None,
        });
        // Its alpha: the layer around it, the only one with less than 1.
        let alpha = list
            .groups
            .iter()
            .filter_map(|group| group.layer.as_ref())
            .find_map(|layer| (layer.alpha < 1.0).then_some(layer.alpha));
        (rect.map_or(0.0, |it| it.width()), alpha.unwrap_or(1.0))
    }
    let run = |rail: bool, scheme: MotionScheme| {
        let selected = mutable_state_of(false);
        let read = selected.handle();
        let mut ui = themed(scheme, move || {
            let icon = || {
                COLORS.with(|it| it[0].set(LOCAL_CONTENT_COLOR.current()));
                Box!(modifier = Modifier.size(24.dp()));
            };
            let label = Some(|| {
                COLORS.with(|it| it[1].set(LOCAL_CONTENT_COLOR.current()));
                Text!("Item");
            });
            if rail {
                NavigationRail!({
                    NavigationRailItem!(read.get(), || {}, icon, label = label, colors = colors());
                });
            } else {
                NavigationBar!(|row: RowScope| {
                    NavigationBarItem!(
                        row,
                        read.get(),
                        || {},
                        icon,
                        label = label,
                        colors = colors()
                    );
                });
            }
        });
        selected.set(true);
        to_halfway(&mut ui);
        let (width, alpha) = indicator(&mut ui);
        let (icon, text) = COLORS.with(|it| (it[0].get(), it[1].get()));
        let full = 56.0;
        vec![
            along(width, 0.0, full),
            along(alpha, 0.0, 1.0),
            color_along(icon, Color::BLUE, Color::RED),
            color_along(text, Color::BLUE, Color::RED),
        ]
    };
    for rail in [false, true] {
        assert_each_property_animates_with(
            &[FastSpatial, DefaultEffects, DefaultEffects, DefaultEffects],
            |scheme| run(rail, scheme),
        );
    }
}

/// A list item's colors on the default effects spec and its corners on the fast spatial one, as
/// `InteractiveListItem`; its clip following the same shape.
#[test]
fn a_list_items_colors_and_shape_animate_with_the_scheme() {
    use std::cell::Cell;
    thread_local! {
        static CONTENT: Cell<Color> = const { Cell::new(Color::UNSPECIFIED) };
    }
    assert_each_property_animates_with(
        &[DefaultEffects, DefaultEffects, FastSpatial, FastSpatial],
        |scheme| {
            let selected = mutable_state_of(false);
            let read = selected.handle();
            let mut ui = themed(scheme, move || {
                let colors = ListItemColors {
                    container_color: Color::BLUE,
                    selected_container_color: Color::RED,
                    content_color: Color::BLUE,
                    selected_content_color: Color::RED,
                    ..ListItemDefaults::colors()
                };
                let shape = |dp: f32| Shape::Rounded([CornerSize::Dp(dp.dp()); 4]);
                let shapes = ListItemShapes {
                    shape: shape(0.0),
                    selected_shape: shape(20.0),
                    ..ListItemDefaults::shapes()
                };
                SelectableListItem!(read.get(), || {}, colors = colors, shapes = shapes, {
                    CONTENT.with(|it| it.set(LOCAL_CONTENT_COLOR.current()));
                    Text!("Item");
                });
            });
            selected.set(true);
            to_halfway(&mut ui);
            let list = ui.host().display_list();
            let background = list.ops.iter().find_map(|op| match op {
                DrawOp::DrawRoundRect(_, radii, paint) => Some((paint.color, radii[0])),
                DrawOp::DrawRect(_, paint) => Some((paint.color, 0.0)),
                _ => None,
            });
            let (container, corner) = background.expect("a background");
            let clip =
                list.groups.iter().filter_map(|group| group.layer.as_ref()).find_map(|layer| {
                    (layer.clip).then(|| match &layer.outline {
                        Some(Outline::Rounded(_, radii)) => radii[0],
                        _ => 0.0,
                    })
                });
            vec![
                color_along(container, Color::BLUE, Color::RED),
                color_along(CONTENT.with(Cell::get), Color::BLUE, Color::RED),
                along(corner, 0.0, 20.0),
                along(clip.expect("a clip"), 0.0, 20.0),
            ]
        },
    );
}

/// A list item's elevation on the fast spatial spec, as its `elevationAnimationSpec`.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/ListItem.kt, InteractiveListItem
#[test]
fn a_list_items_elevation_animates_with_the_scheme() {
    assert_each_property_animates_with(&[FastSpatial], |scheme| {
        let raised = mutable_state_of(false);
        let read = raised.handle();
        let mut ui = themed(scheme, move || {
            let elevation = ListItemElevation {
                elevation: if read.get() { 10.dp() } else { 0.dp() },
                dragged_elevation: 0.dp(),
            };
            SelectableListItem!(false, || {}, elevation = elevation, {
                Text!("Item");
            });
        });
        raised.set(true);
        to_halfway(&mut ui);
        let shadow = ui
            .host()
            .display_list()
            .groups
            .iter()
            .filter_map(|group| group.layer.as_ref())
            .map(|layer| layer.shadow_elevation)
            .fold(0.0f32, f32::max);
        vec![along(shadow, 0.0, 10.0)]
    });
}

/// The corner a cut shape draws: the start of the background's path.
fn cut_corner(ui: &mut ComposeTestRule) -> f32 {
    let list = ui.host().display_list();
    let commands = list.ops.iter().find_map(|op| match op {
        DrawOp::DrawPath { path, .. } => Some(path.commands().to_vec()),
        _ => None,
    });
    match commands.expect("a path").first() {
        Some(compose_ui::PathCommand::MoveTo(point)) => point.y,
        other => panic!("{other:?}"),
    }
}

fn cut(dp: f32) -> Shape {
    Shape::Cut([CornerSize::Dp(dp.dp()); 4])
}

/// Cut corners move too, as `CutCornerShape.lerp`, cut all the while, and end at the state's.
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/shape/CutCornerShape.kt, lerp
#[test]
fn a_list_items_cut_corners_morph() {
    assert_each_property_animates_with(&[FastSpatial], |scheme| {
        let selected = mutable_state_of(false);
        let read = selected.handle();
        let mut ui = themed(scheme, move || {
            let shapes = ListItemShapes {
                shape: cut(4.0),
                selected_shape: cut(24.0),
                pressed_shape: cut(4.0),
                focused_shape: cut(4.0),
                hovered_shape: cut(4.0),
                dragged_shape: cut(4.0),
            };
            SelectableListItem!(read.get(), || {}, shapes = shapes, {
                Text!("Item");
            });
        });
        assert_eq!(cut_corner(&mut ui), 4.0, "at rest it follows the state's");
        selected.set(true);
        to_halfway(&mut ui);
        vec![along(cut_corner(&mut ui), 4.0, 24.0)]
    });
}

/// Between families there's no way of moving corners: the nearer shape, as `Interpolatable.lerp`.
// Compose: compose/ui/ui-graphics/src/commonMain/kotlin/androidx/compose/ui/graphics/Interpolatable.kt, lerp
#[test]
fn a_list_item_between_cut_and_rounded_shows_the_nearer() {
    let selected = mutable_state_of(false);
    let read = selected.handle();
    let mut ui = themed(long_only(FastSpatial), move || {
        let shapes = ListItemShapes {
            shape: cut(10.0),
            selected_shape: Shape::Rounded([CornerSize::Dp(10.dp()); 4]),
            pressed_shape: cut(10.0),
            focused_shape: cut(10.0),
            hovered_shape: cut(10.0),
            dragged_shape: cut(10.0),
        };
        SelectableListItem!(read.get(), || {}, shapes = shapes, {
            Text!("Item");
        });
    });
    let rounded = |ui: &mut ComposeTestRule| {
        ui.host().display_list().ops.iter().any(|op| matches!(op, DrawOp::DrawRoundRect(..)))
    };
    assert!(!rounded(&mut ui));
    selected.set(true);
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(LONG / 4));
    assert!(!rounded(&mut ui), "still the nearer");
    ui.advance_time_by(Duration::from_millis(LONG));
    assert!(rounded(&mut ui));
}

/// The first corner of the background, rounded.
fn round_corner(ui: &mut ComposeTestRule) -> f32 {
    ui.host()
        .display_list()
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::DrawRoundRect(_, radii, _) => Some(radii[0]),
            DrawOp::DrawRect(..) => Some(0.0),
            _ => None,
        })
        .expect("a background")
}

fn round(dp: f32) -> Shape {
    Shape::Rounded([CornerSize::Dp(dp.dp()); 4])
}

/// New shapes start over, showing the new one at once, as `key(this)`.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/ListItemDefaults.kt, shapeForInteraction
#[test]
fn a_list_items_new_shapes_snap() {
    let corner = mutable_state_of(4.0f32);
    let read = corner.handle();
    let mut ui = themed(long_only(FastSpatial), move || {
        let shapes = ListItemShapes { shape: round(read.get()), ..ListItemDefaults::shapes() };
        SelectableListItem!(false, || {}, shapes = shapes, {
            Text!("Item");
        });
    });
    assert_eq!(round_corner(&mut ui), 4.0);
    corner.set(20.0);
    ui.advance_time_by_frame();
    assert_eq!(round_corner(&mut ui), 20.0);
}

/// What's remembered after the shape stays put whether it animates or not: shapes that stop having
/// corners keep the colors where they were.
#[test]
fn a_list_items_colors_stay_when_its_shapes_stop_having_corners() {
    let cornered = mutable_state_of(true);
    let selected = mutable_state_of(false);
    let (read, read_selected) = (cornered.handle(), selected.handle());
    let mut ui = themed(long_only(DefaultEffects), move || {
        let shapes = ListItemShapes {
            shape: if read.get() { round(4.0) } else { Shape::Rectangle },
            ..ListItemDefaults::shapes()
        };
        let colors = ListItemColors {
            container_color: Color::BLUE,
            selected_container_color: Color::RED,
            ..ListItemDefaults::colors()
        };
        SelectableListItem!(read_selected.get(), || {}, colors = colors, shapes = shapes, {
            Text!("Item");
        });
    });
    selected.set(true);
    to_halfway(&mut ui);
    let container = |ui: &mut ComposeTestRule| {
        ui.host()
            .display_list()
            .ops
            .iter()
            .find_map(|op| match op {
                DrawOp::DrawRoundRect(_, _, paint) | DrawOp::DrawRect(_, paint) => {
                    Some(paint.color)
                }
                _ => None,
            })
            .expect("a background")
    };
    let halfway = color_along(container(&mut ui), Color::BLUE, Color::RED);
    assert!((0.1..0.9).contains(&halfway), "{halfway}");
    cornered.set(false);
    ui.advance_time_by_frame();
    let after = color_along(container(&mut ui), Color::BLUE, Color::RED);
    assert!((after - halfway).abs() < 0.2 && after < 0.9, "from {halfway} to {after}");
}

/// A spring going past its end takes corners below zero: they stop there, not failing the shape's
/// check on them.
#[test]
fn a_bouncy_spring_stops_a_shrinking_corner_at_zero() {
    let mut scheme = long_only(FastSpatial);
    scheme.fast_spatial = AnimationSpec::Spring(compose_animation::spring_with(0.2, 200.0));
    let selected = mutable_state_of(false);
    let read = selected.handle();
    let mut ui = themed(scheme, move || {
        let shapes = ListItemShapes {
            shape: round(20.0),
            selected_shape: round(0.0),
            ..ListItemDefaults::shapes()
        };
        SelectableListItem!(read.get(), || {}, shapes = shapes, {
            Text!("Item");
        });
    });
    selected.set(true);
    for _ in 0..120 {
        ui.advance_time_by(Duration::from_millis(16));
        assert!(round_corner(&mut ui) >= 0.0);
    }
    assert_eq!(round_corner(&mut ui), 0.0);
}

/// A new target midway starts from what shows, not the old start or end.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/AnimatedShape.kt, animateToShape
#[test]
fn a_list_items_new_target_midway_starts_from_what_shows() {
    let source = MutableInteractionSource::new();
    let item_source = source.clone();
    let selected = mutable_state_of(false);
    let read = selected.handle();
    let mut ui = themed(long_only(FastSpatial), move || {
        let shapes = ListItemShapes {
            shape: round(0.0),
            selected_shape: round(20.0),
            pressed_shape: round(24.0),
            ..ListItemDefaults::shapes()
        };
        SelectableListItem!(
            read.get(),
            || {},
            shapes = shapes,
            interaction_source = Some(item_source.clone()),
            {
                Text!("Item");
            }
        );
    });
    selected.set(true);
    to_halfway(&mut ui);
    let midway = round_corner(&mut ui);
    assert!(midway > 2.0 && midway < 18.0, "{midway}");
    // Pressed: towards a third, from the point it's frozen at.
    source.try_emit(compose_ui::PressInteraction::Press(compose_ui::Press::new(
        compose_ui::Offset::ZERO,
    )));
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    let frozen = round_corner(&mut ui);
    assert!((frozen - midway).abs() < 2.0, "{midway} to {frozen}");
    ui.advance_time_by(Duration::from_millis(LONG / 2));
    let towards = round_corner(&mut ui);
    assert!(towards > frozen + 3.0 && towards < 23.0, "{frozen} to {towards}");
    // There in the end.
    ui.advance_time_by(Duration::from_millis(LONG));
    assert_eq!(round_corner(&mut ui), 24.0);
}

/// The placeholder leaves on the fast effects spec when a focused field empties unfocused, as
/// `placeholderOpacity`'s first case.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/TextFieldImpl.kt, placeholderOpacity
#[test]
fn a_placeholder_leaves_a_field_on_fast_effects() {
    assert_each_property_animates_with(&[FastEffects], |scheme| {
        let mut ui = themed(scheme, move || {
            Column!({
                TextField!(
                    String::new(),
                    |_| {},
                    modifier = Modifier.test_tag("field"),
                    label = Some(|| Text!("Label")),
                    placeholder = Some(|| Text!("Hint")),
                );
                Box!(modifier = Modifier.test_tag("other").focusable().size(4.dp()));
            });
        });
        ui.on_node_with_tag("field").request_focus();
        ui.advance_time_by(Duration::from_millis(2 * LONG));
        ui.on_node_with_tag("other").request_focus();
        to_halfway(&mut ui);
        let (_, placeholder) = layer_scale_and_alpha(&mut ui);
        // Gone, it's arrived: no layer to read an alpha from.
        vec![if placeholder == 1.0 { 1.0 } else { 1.0 - placeholder }]
    });
}

/// Disabled, the indicator is the unfocused 1 dp and its color snaps, as `animateBorderStrokeAsState`.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/TextFieldImpl.kt, animateBorderStrokeAsState
#[test]
fn a_disabled_fields_indicator_is_thin_and_snaps() {
    let enabled = mutable_state_of(true);
    let read = enabled.handle();
    let mut ui = themed(long_only(FastEffects), move || {
        let colors = TextFieldColors {
            focused_indicator_color: Color::BLUE,
            unfocused_indicator_color: Color::BLUE,
            disabled_indicator_color: Color::RED,
            ..TextFieldDefaults::colors()
        };
        TextField!(
            String::new(),
            |_| {},
            modifier = Modifier.test_tag("field"),
            enabled = read.get(),
            colors = colors
        );
    });
    ui.on_node_with_tag("field").request_focus();
    ui.advance_time_by(Duration::from_millis(2 * LONG));
    let line = |ui: &mut ComposeTestRule, color: Color| {
        ui.host().display_list().ops.iter().find_map(|op| match op {
            DrawOp::DrawRect(rect, paint) if paint.color == color => Some(rect.height()),
            _ => None,
        })
    };
    assert_eq!(line(&mut ui, Color::BLUE), Some(2.0), "focused");
    enabled.set(false);
    ui.advance_time_by_frame();
    assert_eq!(line(&mut ui, Color::RED), Some(1.0), "at once");
}

/// An outlined one's too, its outline the unfocused width.
#[test]
fn a_disabled_outlined_fields_outline_is_thin() {
    let enabled = mutable_state_of(true);
    let read = enabled.handle();
    let mut ui = themed(long_only(FastEffects), move || {
        OutlinedTextField!(
            String::new(),
            |_| {},
            modifier = Modifier.test_tag("field"),
            enabled = read.get()
        );
    });
    ui.on_node_with_tag("field").request_focus();
    ui.advance_time_by(Duration::from_millis(2 * LONG));
    let width = |ui: &mut ComposeTestRule| {
        ui.host()
            .display_list()
            .ops
            .iter()
            .find_map(|op| match op {
                DrawOp::DrawPath { paint, .. } | DrawOp::DrawRoundRect(_, _, paint) => {
                    match paint.style {
                        PaintStyle::Stroke(width) => Some(width),
                        _ => None,
                    }
                }
                _ => None,
            })
            .expect("an outline")
    };
    assert_eq!(width(&mut ui), 2.0, "focused");
    enabled.set(false);
    ui.advance_time_by_frame();
    assert_eq!(width(&mut ui), 1.0);
}

/// A run starts from what shows, at no speed: nothing jumps on its first frame, and interrupted it
/// goes back from where it was.
#[test]
fn a_list_items_colors_reverse_from_where_they_are() {
    let selected = mutable_state_of(false);
    let read = selected.handle();
    let mut ui = themed(long_only(DefaultEffects), move || {
        let colors = ListItemColors {
            container_color: Color::BLUE,
            selected_container_color: Color::RED,
            ..ListItemDefaults::colors()
        };
        SelectableListItem!(read.get(), || {}, colors = colors, {
            Text!("Item");
        });
    });
    let container = |ui: &mut ComposeTestRule| {
        let color = ui.host().display_list().ops.iter().find_map(|op| match op {
            DrawOp::DrawRoundRect(_, _, paint) | DrawOp::DrawRect(_, paint) => Some(paint.color),
            _ => None,
        });
        color_along(color.expect("a background"), Color::BLUE, Color::RED)
    };
    selected.set(true);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    let first = container(&mut ui);
    assert!(first < 0.1, "{first}");
    ui.advance_time_by(Duration::from_millis(LONG / 2));
    let halfway = container(&mut ui);
    assert!((0.1..0.9).contains(&halfway), "{halfway}");
    selected.set(false);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    let back = container(&mut ui);
    assert!((back - halfway).abs() < 0.1, "from {halfway} to {back}");
}

thread_local! {
    static FIELD_COMPOSED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static LABEL_COMPOSED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// A frame of a label floating composes the label, not the field around it: a trailing icon is
/// composed once for the focus, not again. As `labelProgressProducer`, `DecoratedLabel`.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/TextFieldImpl.kt, CommonDecorationBox
#[test]
fn a_fields_frames_compose_the_label_alone() {
    FIELD_COMPOSED.set(0);
    LABEL_COMPOSED.set(0);
    let mut ui = themed(long_only(FastSpatial), || {
        TextField!(
            String::new(),
            |_| {},
            modifier = Modifier.test_tag("field"),
            label = Some(|| {
                LABEL_COMPOSED.set(LABEL_COMPOSED.get() + 1);
                Text!("Label");
            }),
            trailing_icon = Some(|| {
                FIELD_COMPOSED.set(FIELD_COMPOSED.get() + 1);
                Box!(modifier = Modifier.size(24.dp()));
            }),
        );
    });
    ui.on_node_with_tag("field").request_focus();
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(32));
    let (field, label) = (FIELD_COMPOSED.get(), LABEL_COMPOSED.get());
    for _ in 0..10 {
        ui.advance_time_by(Duration::from_millis(16));
    }
    assert_eq!(FIELD_COMPOSED.get(), field, "the field");
    assert!(LABEL_COMPOSED.get() >= label + 5, "the label: {label} to {}", LABEL_COMPOSED.get());
}

/// A chip's icon coming: its width on fast spatial and its alpha on slow effects; going, on default
/// effects and fast effects, as `SelectableChip`'s, whether it was there at first or not.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Chip.kt, SelectableChip
#[test]
fn a_filter_chips_icon_comes_and_goes_with_the_scheme() {
    let run = |at_first: bool, scheme: MotionScheme| {
        let shows = mutable_state_of(at_first);
        let read = shows.handle();
        let mut ui = themed(scheme, move || {
            let leading = read.get().then_some(|| {
                Box!(modifier = Modifier.size(18.dp()).background(Color::RED));
            });
            FilterChip!(
                false,
                || {},
                || Box!(modifier = Modifier.size((30.dp(), 18.dp())).test_tag("label")),
                leading_icon = leading,
            );
        });
        let label_left = |ui: &mut ComposeTestRule| {
            ui.on_node_unmerged(has_test_tag("label")).fetch_semantics_node().bounds.left
        };
        let icon_alpha = |ui: &mut ComposeTestRule| {
            let list = ui.host().display_list();
            let layers = list.groups.iter().filter_map(|group| group.layer.as_ref());
            layers.fold(1.0f32, |alpha, layer| alpha.min(layer.alpha))
        };
        // 16 dp in with no icon, 34 with one, the width it's at telling how far along.
        let (from, to) = if at_first { (34.0, 16.0) } else { (16.0, 34.0) };
        assert_eq!(label_left(&mut ui), from);
        shows.set(!at_first);
        to_halfway(&mut ui);
        let left = label_left(&mut ui);
        let alpha = icon_alpha(&mut ui);
        vec![along(left, from, to), if at_first { 1.0 - alpha } else { alpha }]
    };
    // A spec's short end: the icon's gone, or at 1, its layer not being there.
    let going = |scheme: MotionScheme| {
        let mut along = run(true, scheme);
        if along[1] == 0.0 {
            along[1] = 1.0;
        }
        along
    };
    assert_each_property_animates_with(&[FastSpatial, SlowEffects], |scheme| run(false, scheme));
    assert_each_property_animates_with(&[DefaultEffects, FastEffects], going);
}

/// With `shapes`, coming and going on fast spatial, its alpha on default effects, as
/// `SelectableChip`'s second overload.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Chip.kt, SelectableChip
#[test]
fn a_shaped_filter_chips_icon_comes_and_goes_with_the_scheme() {
    let run = |at_first: bool, scheme: MotionScheme| {
        let shows = mutable_state_of(at_first);
        let read = shows.handle();
        let mut ui = themed(scheme, move || {
            let leading = read.get().then_some(|| {
                Box!(modifier = Modifier.size(18.dp()).background(Color::RED));
            });
            FilterChip!(
                false,
                || {},
                || Box!(modifier = Modifier.size((30.dp(), 18.dp())).test_tag("label")),
                shapes = FilterChipDefaults::shapes(),
                leading_icon = leading,
            );
        });
        shows.set(!at_first);
        to_halfway(&mut ui);
        let left = ui.on_node_unmerged(has_test_tag("label")).fetch_semantics_node().bounds.left;
        let list = ui.host().display_list();
        let layers = list.groups.iter().filter_map(|group| group.layer.as_ref());
        let alpha = layers.fold(1.0f32, |alpha, layer| alpha.min(layer.alpha));
        // Compact beside it: 8, 18 and 4, 30 in all, against the 16 with none.
        let (from, to) = if at_first { (30.0, 16.0) } else { (16.0, 30.0) };
        let faded = if at_first {
            // Gone, or at 1 with its layer not there.
            if alpha == 1.0 { 1.0 } else { 1.0 - alpha }
        } else {
            alpha
        };
        vec![along(left, from, to), faded]
    };
    for at_first in [false, true] {
        assert_each_property_animates_with(&[FastSpatial, DefaultEffects], |scheme| {
            run(at_first, scheme)
        });
    }
}

/// A sized extended FAB's width on fast spatial and its text's alpha on fast effects.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/FloatingActionButton.kt, ExtendedFloatingActionButton
#[test]
fn a_sized_extended_fab_animates_with_the_scheme() {
    assert_each_property_animates_with(&[FastSpatial, FastEffects], |scheme| {
        let expanded = mutable_state_of(true);
        let read = expanded.handle();
        let mut ui = themed(scheme, move || {
            SmallExtendedFloatingActionButton!(
                || Box!(modifier = Modifier.size((60.dp(), 18.dp())).background(Color::RED)),
                || Box!(modifier = Modifier.size(24.dp())),
                || {},
                modifier = Modifier.test_tag("fab"),
                expanded = read.get()
            );
        });
        let width = |ui: &mut ComposeTestRule| {
            ui.on_node_unmerged(has_test_tag("fab")).fetch_semantics_node().bounds.width()
        };
        let open = width(&mut ui);
        expanded.set(false);
        to_halfway(&mut ui);
        let list = ui.host().display_list();
        let layers = list.groups.iter().filter_map(|group| group.layer.as_ref());
        let alpha = layers.fold(1.0f32, |alpha, layer| alpha.min(layer.alpha));
        // Its text gone, or at 1 with no layer below it.
        vec![along(width(&mut ui), open, 56.0), if alpha == 1.0 { 1.0 } else { 1.0 - alpha }]
    });
}

/// `animate_floating_action_button`: its scale on fast spatial and its alpha on fast effects.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/FloatingActionButton.kt, FabVisibleNode
#[test]
fn a_fabs_show_and_hide_animate_with_the_scheme() {
    assert_each_property_animates_with(&[FastSpatial, FastEffects], |scheme| {
        let visible = mutable_state_of(true);
        let read = visible.handle();
        let mut ui = themed(scheme, move || {
            Box!(
                modifier = Modifier
                    .animate_floating_action_button(read.get(), compose_ui::Alignment::CENTER)
                    .size(40.dp())
                    .background(Color::RED)
            );
        });
        visible.set(false);
        to_halfway(&mut ui);
        let list = ui.host().display_list();
        let layer = list.groups.iter().filter_map(|group| group.layer.as_ref()).next();
        // Gone at alpha 0: no layer drawn, so arrived for both, its alpha not having been the long
        // one's.
        let (scale, alpha) = layer.map_or((0.2, 0.0), |layer| (layer.scale.0, layer.alpha));
        vec![along(scale, 1.0, 0.2), 1.0 - alpha]
    });
}

/// A button's and an icon button's corners on default effects, "to prevent any bounce".
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Button.kt, Button
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/IconButton.kt, shapeForInteraction
#[test]
fn a_buttons_corners_animate_with_the_scheme() {
    use compose_ui::{Offset, Press, PressInteraction};
    let run = |icon: bool, scheme: MotionScheme| {
        let source = MutableInteractionSource::new();
        let hoisted = source.clone();
        let mut ui = themed(scheme, move || {
            let hoisted = Some(hoisted.clone());
            if icon {
                IconButton!(
                    || {},
                    shapes = IconButtonShapes { shape: round(2.0), pressed_shape: round(12.0) },
                    colors = IconButtonColors {
                        container_color: Color::BLUE,
                        ..IconButtonDefaults::icon_button_colors()
                    },
                    interaction_source = hoisted,
                    {}
                );
            } else {
                Button!(
                    || {},
                    shapes = ButtonShapes { shape: round(2.0), pressed_shape: round(12.0) },
                    colors = ButtonColors {
                        container_color: Color::BLUE,
                        ..ButtonDefaults::button_colors()
                    },
                    interaction_source = hoisted,
                    |_: RowScope| {}
                );
            }
        });
        source.try_emit(PressInteraction::Press(Press::new(Offset::ZERO)));
        to_halfway(&mut ui);
        vec![along(round_corner(&mut ui), 2.0, 12.0)]
    };
    for icon in [false, true] {
        assert_each_property_animates_with(&[DefaultEffects], |scheme| run(icon, scheme));
    }
}

/// A chip's corners on the fast spatial spec, as its `defaultAnimationSpec`.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Chip.kt, SelectableChip
#[test]
fn a_chips_corners_animate_with_the_scheme() {
    assert_each_property_animates_with(&[FastSpatial], |scheme| {
        let selected = mutable_state_of(false);
        let read = selected.handle();
        let mut ui = themed(scheme, move || {
            let colors = SelectableChipColors {
                container_color: Color::BLUE,
                ..FilterChipDefaults::filter_chip_colors()
            };
            let shapes = ChipShapes {
                shape: round(2.0),
                selected_shape: round(12.0),
                pressed_shape: round(12.0),
            };
            FilterChip!(read.get(), || {}, || Text!("a"), shapes = shapes, colors = colors);
        });
        selected.set(true);
        to_halfway(&mut ui);
        vec![along(round_corner(&mut ui), 2.0, 12.0)]
    });
}

/// Reversed before it's finished, a shape goes back from where it is, not its start: as
/// `animateToShape`'s flip of the progress.
#[test]
fn a_list_items_corners_reverse_from_where_they_are() {
    let selected = mutable_state_of(false);
    let read = selected.handle();
    let mut ui = themed(long_only(FastSpatial), move || {
        let shape = |dp: f32| Shape::Rounded([CornerSize::Dp(dp.dp()); 4]);
        let shapes = ListItemShapes {
            shape: shape(0.0),
            selected_shape: shape(20.0),
            ..ListItemDefaults::shapes()
        };
        SelectableListItem!(read.get(), || {}, shapes = shapes, {
            Text!("Item");
        });
    });
    let corner = |ui: &mut ComposeTestRule| {
        ui.host()
            .display_list()
            .ops
            .iter()
            .find_map(|op| match op {
                DrawOp::DrawRoundRect(_, radii, _) => Some(radii[0]),
                _ => None,
            })
            .unwrap_or(0.0)
    };
    selected.set(true);
    to_halfway(&mut ui);
    let halfway = corner(&mut ui);
    assert!((5.0..15.0).contains(&halfway), "{halfway}");
    selected.set(false);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    let back = corner(&mut ui);
    assert!((back - halfway).abs() < 2.0, "{back} isn't near {halfway}");
    idle_after(&mut ui);
    assert_eq!(corner(&mut ui), 0.0);
}

/// A text field's container following its state on the fast effects spec, as
/// `TextFieldDefaults.Container`.
#[test]
fn a_text_fields_container_animates_with_the_scheme() {
    assert_each_property_animates_with(&[FastEffects], |scheme| {
        let enabled = mutable_state_of(true);
        let read = enabled.handle();
        let mut ui = themed(scheme, move || {
            let colors = TextFieldColors {
                container_color: Color::BLUE,
                disabled_container_color: Color::RED,
                ..TextFieldDefaults::colors()
            };
            TextField!(String::new(), |_| {}, enabled = read.get(), colors = colors);
        });
        enabled.set(false);
        to_halfway(&mut ui);
        let container = ui
            .host()
            .display_list()
            .ops
            .iter()
            .find_map(|op| match op {
                DrawOp::DrawRoundRect(_, _, paint) | DrawOp::DrawPath { paint, .. } => {
                    Some(paint.color)
                }
                _ => None,
            })
            .expect("a container");
        vec![color_along(container, Color::BLUE, Color::RED)]
    });
}

/// A text field focused: its label on the fast spatial spec and its color on fast effects, the
/// border's color on fast effects and its thickness on fast spatial, the placeholder fading in on
/// slow effects, as `TextFieldImpl`'s transition and `animateBorderStrokeAsState`.
#[test]
fn a_focused_text_fields_parts_animate_with_the_scheme() {
    use std::cell::Cell;
    thread_local! {
        static LABEL: Cell<Color> = const { Cell::new(Color::UNSPECIFIED) };
    }
    /// The outline's stroke.
    fn border(ui: &mut ComposeTestRule) -> (Color, f32) {
        ui.host()
            .display_list()
            .ops
            .iter()
            .find_map(|op| match op {
                DrawOp::DrawPath { paint, .. } | DrawOp::DrawRoundRect(_, _, paint) => {
                    match paint.style {
                        PaintStyle::Stroke(width) => Some((paint.color, width)),
                        PaintStyle::Fill => None,
                    }
                }
                _ => None,
            })
            .expect("an outline")
    }
    assert_each_property_animates_with(
        &[FastSpatial, FastEffects, FastEffects, FastSpatial, SlowEffects],
        |scheme| {
            let mut ui = themed(scheme, move || {
                let colors = TextFieldColors {
                    label_color: Color::BLUE,
                    focused_label_color: Color::RED,
                    unfocused_indicator_color: Color::BLUE,
                    focused_indicator_color: Color::RED,
                    ..OutlinedTextFieldDefaults::colors()
                };
                OutlinedTextField!(
                    String::new(),
                    |_| {},
                    modifier = Modifier.test_tag("field"),
                    label = Some(|| {
                        LABEL.with(|it| it.set(LOCAL_CONTENT_COLOR.current()));
                        Text!("Label");
                    }),
                    placeholder = Some(|| Text!("Hint")),
                    colors = colors
                );
            });
            let label_top = |ui: &mut ComposeTestRule| {
                ui.on_node_unmerged(has_text("Label")).fetch_semantics_node().bounds.top
            };
            let start_top = label_top(&mut ui);
            ui.on_node_with_tag("field").request_focus();
            to_halfway(&mut ui);
            let top = label_top(&mut ui);
            let label_color = LABEL.with(Cell::get);
            let (border_color, width) = border(&mut ui);
            let (_, placeholder) = layer_scale_and_alpha(&mut ui);
            // Its cursor blinks on: not to idle, past the long spec.
            ui.advance_time_by(Duration::from_millis(LONG));
            let end_top = label_top(&mut ui);
            vec![
                along(top, start_top, end_top),
                color_along(label_color, Color::BLUE, Color::RED),
                color_along(border_color, Color::BLUE, Color::RED),
                along(width, 1.0, 2.0),
                placeholder,
            ]
        },
    );
}

/// The field filled, unfocused: its label floating on the fast spatial spec, its suffix fading in
/// on fast effects, as `affixOpacity`.
#[test]
fn a_filled_text_fields_affix_animates_with_the_scheme() {
    assert_each_property_animates_with(&[FastSpatial, FastEffects], |scheme| {
        let value = mutable_state_of(String::new());
        let read = value.handle();
        let mut ui = themed(scheme, move || {
            TextField!(
                read.get(),
                |_| {},
                label = Some(|| Text!("Label")),
                suffix = Some(|| Text!("kg"))
            );
        });
        let label_top = |ui: &mut ComposeTestRule| {
            ui.on_node_unmerged(has_text("Label")).fetch_semantics_node().bounds.top
        };
        let start_top = label_top(&mut ui);
        value.set("1".to_string());
        to_halfway(&mut ui);
        let top = label_top(&mut ui);
        let (_, alpha) = layer_scale_and_alpha(&mut ui);
        idle_after(&mut ui);
        let end_top = label_top(&mut ui);
        vec![along(top, start_top, end_top), alpha]
    });
}

/// A segmented button's check: it fades in on the default effects spec and grows from nothing on
/// fast spatial, and the label moves over on fast spatial; unchecked, it's gone at once, as
/// `SegmentedButtonDefaults.Icon`.
#[test]
fn a_segmented_buttons_check_animates_with_the_scheme() {
    assert_each_property_animates_with(&[DefaultEffects, FastSpatial], |scheme| {
        let selected = mutable_state_of(false);
        let read = selected.handle();
        let mut ui = themed(scheme, move || {
            SingleChoiceSegmentedButtonRow!(|row| {
                SegmentedButton!(
                    row,
                    read.get(),
                    || {},
                    SegmentedButtonDefaults::item_shape(0, 1),
                    {
                        Text!("One");
                    }
                );
            });
        });
        selected.set(true);
        to_halfway(&mut ui);
        let (scale, alpha) = layer_scale_and_alpha(&mut ui);
        vec![alpha, scale]
    });
    // The label's move, and the time: all of it, with those.
    assert_animates_with(&[DefaultEffects, FastSpatial], |scheme| {
        let selected = mutable_state_of(false);
        let read = selected.handle();
        let mut ui = themed(scheme, move || {
            SingleChoiceSegmentedButtonRow!(|row| {
                SegmentedButton!(
                    row,
                    read.get(),
                    || {},
                    SegmentedButtonDefaults::item_shape(0, 1),
                    {
                        Text!("One");
                    }
                );
            });
        });
        selected.set(true);
        idle_after(&mut ui)
    });
    // Unchecked, only the label's move.
    assert_animates_with(&[FastSpatial], |scheme| {
        let selected = mutable_state_of(true);
        let read = selected.handle();
        let mut ui = themed(scheme, move || {
            SingleChoiceSegmentedButtonRow!(|row| {
                SegmentedButton!(
                    row,
                    read.get(),
                    || {},
                    SegmentedButtonDefaults::item_shape(0, 1),
                    {
                        Text!("One");
                    }
                );
            });
        });
        selected.set(false);
        idle_after(&mut ui)
    });
}

/// With content for both states, a crossfade on the default effects spec, as
/// `SegmentedButtonDefaults.Icon(active, activeContent, inactiveContent)`.
#[test]
fn a_segmented_buttons_two_icons_crossfade_with_the_scheme() {
    assert_animates_with(&[DefaultEffects], |scheme| {
        let selected = mutable_state_of(false);
        let read = selected.handle();
        let mut ui = themed(scheme, move || {
            SingleChoiceSegmentedButtonRow!(|row| {
                SegmentedButton!(
                    row,
                    read.get(),
                    || {},
                    SegmentedButtonDefaults::item_shape(0, 1),
                    icon = Some(move || {
                        SegmentedButtonDefaults::icon_with(
                            read.get(),
                            || {
                                Box!(modifier = Modifier.size(18.dp()));
                            },
                            Some(|| {
                                Box!(modifier = Modifier.size(18.dp()));
                            }),
                        );
                    }),
                    {
                        Text!("One");
                    }
                );
            });
        });
        selected.set(true);
        idle_after(&mut ui)
    });
}

/// Its scale on the fast spatial spec and its alpha on the fast effects one, as
/// `DropdownMenuContent`.
#[test]
fn a_menus_scale_and_alpha_animate_with_the_scheme() {
    assert_each_property_animates_with(&[FastSpatial, FastEffects], |scheme| {
        let expanded = mutable_state_of(false);
        let read = expanded.handle();
        let mut ui = themed(scheme, move || {
            Box!({
                DropdownMenu!(read.get(), || {}, |_| {
                    Text!("Item");
                });
            });
        });
        expanded.set(true);
        ui.advance_time_by_frame();
        to_halfway(&mut ui);
        let (scale, alpha) = layer_scale_and_alpha(&mut ui);
        vec![along(scale, 0.8, 1.0), along(alpha, 0.0, 1.0)]
    });
}

/// Opening on the default spatial spec, closing on the fast effects one, as Compose's
/// `openDrawerMotionSpec` and `closeDrawerMotionSpec`.
#[test]
fn a_drawer_animates_with_the_scheme() {
    let run = |to: DrawerValue, scheme: MotionScheme| {
        let from = if to == DrawerValue::Open { DrawerValue::Closed } else { DrawerValue::Open };
        let state = DrawerState::new(from, |_| true);
        let handle = *state;
        let mut ui = themed(scheme, move || {
            ModalNavigationDrawer!(
                {
                    ModalDrawerSheet!(|_| {
                        Text!("Sheet");
                    });
                },
                drawer_state = handle,
                {
                    Text!("Content");
                }
            );
        });
        compose_runtime::platform::launch(async move {
            if to == DrawerValue::Open { handle.open().await } else { handle.close().await }
        });
        let millis = idle_after(&mut ui);
        assert_eq!(handle.current_value(), to);
        millis
    };
    assert_animates_with(&[DefaultSpatial], |scheme| run(DrawerValue::Open, scheme));
    assert_animates_with(&[FastEffects], |scheme| run(DrawerValue::Closed, scheme));
}

/// A drawer's drag let go, either way, settles on the spatial spec: `anchoredDraggableMotionSpec`.
#[test]
fn a_drawer_let_go_settles_with_the_scheme() {
    assert_animates_with(&[DefaultSpatial], |scheme| {
        let state = DrawerState::new(DrawerValue::Open, |_| true);
        let handle = *state;
        let mut ui = themed(scheme, move || {
            ModalNavigationDrawer!(
                {
                    ModalDrawerSheet!(modifier = Modifier.test_tag("sheet"), |_| {
                        Box!(modifier = Modifier.fill_max_size());
                    });
                },
                drawer_state = handle,
                {
                    Text!("Content");
                }
            );
        });
        ui.on_node_with_tag("sheet").perform_touch_input(|touch| {
            let center = touch.center();
            touch.down(center);
            touch.move_by(compose_ui::Offset::new(-40.0, 0.0));
            touch.up();
        });
        idle_after(&mut ui)
    });
}

/// A snackbar's fade on the fast effects spec and its scale on the fast spatial one, in and out, as
/// `FadeInFadeOutWithScale`.
#[test]
fn a_snackbars_fade_and_scale_animate_with_the_scheme() {
    let run = |dismiss: bool, scheme: MotionScheme| {
        let host = SnackbarHostState::new();
        let handle = *host;
        let mut ui = themed(scheme, move || {
            SnackbarHost!(handle);
        });
        compose_runtime::platform::launch(async move {
            handle.show_snackbar("Saved").await;
        });
        if dismiss {
            idle_after(&mut ui);
            handle.current_snackbar_data().expect("shown").dismiss();
            to_halfway(&mut ui);
            let (scale, alpha) = layer_scale_and_alpha(&mut ui);
            // No layer left: it has gone, both there.
            if (scale, alpha) == (1.0, 1.0) {
                return vec![1.0, 1.0];
            }
            vec![along(alpha, 1.0, 0.0), along(scale, 1.0, 0.8)]
        } else {
            to_halfway(&mut ui);
            let (scale, alpha) = layer_scale_and_alpha(&mut ui);
            vec![along(alpha, 0.0, 1.0), along(scale, 0.8, 1.0)]
        }
    };
    assert_each_property_animates_with(&[FastEffects, FastSpatial], |scheme| run(false, scheme));
    assert_each_property_animates_with(&[FastEffects, FastSpatial], |scheme| run(true, scheme));
}

/// An extended button's text: expanding, its fade on default effects and its width on fast spatial;
/// collapsing, on fast effects and default spatial, as `extendedFabExpandAnimation` and
/// `extendedFabCollapseAnimation`.
#[test]
fn an_extended_fabs_fade_and_width_animate_with_the_scheme() {
    let run = |expand: bool, scheme: MotionScheme| {
        let expanded = mutable_state_of(!expand);
        let read = expanded.handle();
        let mut ui = themed(scheme, move || {
            ExtendedFloatingActionButton!(
                { Text!("A long enough label") },
                { Box!(modifier = Modifier.size(24.dp())) },
                || {},
                modifier = Modifier.test_tag("fab"),
                expanded = read.get()
            );
        });
        let width = |ui: &mut ComposeTestRule| {
            ui.on_node_with_tag("fab").fetch_semantics_node().bounds.width()
        };
        let start = width(&mut ui);
        expanded.set(expand);
        to_halfway(&mut ui);
        let (halfway, (_, alpha)) = (width(&mut ui), layer_scale_and_alpha(&mut ui));
        idle_after(&mut ui);
        let end = width(&mut ui);
        // Collapsing, no layer left is the text gone, at the fade's end.
        let fade = if expand {
            alpha
        } else if alpha == 1.0 {
            1.0
        } else {
            1.0 - alpha
        };
        vec![fade, along(halfway, start, end)]
    };
    assert_each_property_animates_with(&[DefaultEffects, FastSpatial], |scheme| run(true, scheme));
    assert_each_property_animates_with(&[FastEffects, DefaultSpatial], |scheme| run(false, scheme));
}

/// Until a drawer composes it, a state has no motion: opening and closing snap, as Compose's
/// `openDrawerMotionSpec = snap()`.
#[test]
fn a_drawer_state_not_yet_composed_snaps() {
    let state = DrawerState::new(DrawerValue::Closed, |_| true);
    let handle = *state;
    let mut ui = ComposeTestRule::with_manual_clock(|| {});
    compose_runtime::platform::launch(async move { handle.open().await });
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert_eq!(handle.current_value(), DrawerValue::Open);
    compose_runtime::platform::launch(async move { handle.close().await });
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert_eq!(handle.current_value(), DrawerValue::Closed);
}

/// `enterAlwaysScrollBehavior(snapAnimationSpec = ...)`: what's given, not the theme's; the same
/// connection through recompositions, a new one only when the spec changes, as Compose's
/// `remember(state, canScroll, snapAnimationSpec, ...)`.
#[test]
fn a_scroll_behaviors_snap_spec_is_kept_until_it_changes() {
    use std::cell::RefCell;
    thread_local! {
        static BEHAVIOR: RefCell<Option<TopAppBarScrollBehavior>> = const { RefCell::new(None) };
        static COMPOSED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    }
    let tick = mutable_state_of(0);
    let tick_read = tick.handle();
    let millis = mutable_state_of(LONG as u32);
    let millis_read = millis.handle();
    let mut ui = themed(long_only(SlowSpatial), move || {
        let _ = tick_read.get();
        COMPOSED.with(|it| it.set(it.get() + 1));
        let behavior =
            TopAppBarDefaults::enter_always_scroll_behavior_with(Some(tween_of(millis_read.get())));
        BEHAVIOR.with(|cell| *cell.borrow_mut() = Some(behavior.clone()));
        TopAppBar!({ Text!("Title") }, scroll_behavior = Some(behavior));
    });
    let current = || BEHAVIOR.with(|cell| cell.borrow().clone()).expect("composed");
    let first = current();
    let composed = COMPOSED.with(std::cell::Cell::get);
    tick.set(1);
    idle_after(&mut ui);
    assert!(COMPOSED.with(std::cell::Cell::get) > composed, "it composed again");
    assert!(Rc::ptr_eq(&first.nested_scroll_connection(), &current().nested_scroll_connection()));

    // What's given settles it, the theme's specs being short.
    let settle = |ui: &mut ComposeTestRule, behavior: &TopAppBarScrollBehavior| {
        behavior.state().set_height_offset(-20.0);
        let connection = behavior.nested_scroll_connection();
        compose_runtime::platform::launch(async move {
            let zero = compose_ui::Offset::ZERO;
            connection.on_post_fling(zero, zero).await;
        });
        idle_after(ui)
    };
    assert!(settle(&mut ui, &first) >= LONG - 32);

    millis.set(16);
    idle_after(&mut ui);
    let second = current();
    assert!(!Rc::ptr_eq(&first.nested_scroll_connection(), &second.nested_scroll_connection()));
    assert!(settle(&mut ui, &second) < SHORT);
}

/// `snapAnimationSpec = null`: left where a scroll leaves it.
#[test]
fn a_scroll_behavior_without_a_snap_spec_stays_where_it_is() {
    use std::cell::RefCell;
    thread_local! {
        static BEHAVIOR: RefCell<Option<TopAppBarScrollBehavior>> = const { RefCell::new(None) };
    }
    let mut ui = themed(MotionScheme::standard(), move || {
        let behavior = TopAppBarDefaults::exit_until_collapsed_scroll_behavior_with(None);
        BEHAVIOR.with(|cell| *cell.borrow_mut() = Some(behavior.clone()));
        TopAppBar!({ Text!("Title") }, scroll_behavior = Some(behavior));
    });
    let behavior = BEHAVIOR.with(|cell| cell.borrow().clone()).expect("composed");
    behavior.state().set_height_offset(-20.0);
    let connection = behavior.nested_scroll_connection();
    compose_runtime::platform::launch(async move {
        let zero = compose_ui::Offset::ZERO;
        connection.on_post_fling(zero, zero).await;
    });
    idle_after(&mut ui);
    assert_eq!(behavior.state().height_offset(), -20.0);
}

/// The container's color on the default effects spec, as `TopAppBarLayout`'s
/// `animateColorAsState`.
#[test]
fn a_top_app_bars_color_animates_with_the_scheme() {
    assert_animates_with(&[DefaultEffects], |scheme| {
        let state = TopAppBarState::new(-f32::MAX, 0.0, 0.0);
        let handle = *state;
        let mut ui = themed(scheme, move || {
            let scope = remember_coroutine_scope();
            let behavior = TopAppBarScrollBehavior::pinned(handle, scope);
            TopAppBar!({ Text!("Title") }, scroll_behavior = Some(behavior));
        });
        handle.set_content_offset(-100.0);
        idle_after(&mut ui)
    });
}

/// A bar left half collapsed settles on `TopAppBarDefaults.snapAnimationSpec`, default effects.
#[test]
fn a_top_app_bar_settles_with_the_scheme() {
    assert_animates_with(&[DefaultEffects], |scheme| {
        thread_local! {
            static BEHAVIOR: std::cell::RefCell<Option<TopAppBarScrollBehavior>> = const { std::cell::RefCell::new(None) };
        }
        let mut ui = themed(scheme, move || {
            let behavior = TopAppBarDefaults::enter_always_scroll_behavior();
            BEHAVIOR.with(|cell| *cell.borrow_mut() = Some(behavior.clone()));
            TopAppBar!({ Text!("Title") }, scroll_behavior = Some(behavior));
        });
        let behavior = BEHAVIOR.with(|cell| cell.borrow().clone()).expect("composed");
        behavior.state().set_height_offset(-20.0);
        let connection = behavior.nested_scroll_connection();
        compose_runtime::platform::launch(async move {
            let zero = compose_ui::Offset::ZERO;
            connection.on_post_fling(zero, zero).await;
        });
        idle_after(&mut ui)
    });
}

/// The label floating on the fast spatial spec, as `labelProgress`.
#[test]
fn a_text_fields_label_animates_with_the_scheme() {
    assert_animates_with(&[FastSpatial], |scheme| {
        let value = mutable_state_of(String::new());
        let read = value.handle();
        let mut ui = themed(scheme, move || {
            TextField!(read.get(), |_| {}, label = Some(|| Text!("Label")));
        });
        value.set("x".to_string());
        idle_after(&mut ui)
    });
}

/// The inset ring in on the fast spatial spec, out on the fast effects one, as
/// `calculateRippleNodeConfiguration`. A state layer's times aren't the scheme's: its tweens are
/// constants in `incomingStateLayerAnimationSpecFor`, and `a_drags_state_layer_fades_in_and_out_linearly`
/// has them.
#[test]
fn an_inset_ring_animates_with_the_scheme() {
    let run = |focus_out: bool, scheme: MotionScheme| {
        let source = MutableInteractionSource::new();
        let box_source = source.clone();
        let mut ui = themed(scheme, move || {
            let box_source = box_source.clone();
            CompositionLocalProvider(
                LOCAL_RIPPLE_THEME_CONFIGURATION
                    .provides(RippleDefaults::INSET_FOCUS_RING_THEME_CONFIGURATION),
                move || {
                    Box!(
                        modifier =
                            Modifier.size(40.dp()).indication(box_source.clone(), default_ripple())
                    );
                },
            );
        });
        let focus = Focus::new();
        source.try_emit(FocusInteraction::Focus(focus));
        let focusing = idle_after(&mut ui);
        if !focus_out {
            return focusing;
        }
        source.try_emit(FocusInteraction::Unfocus(focus));
        idle_after(&mut ui)
    };
    assert_animates_with(&[FastSpatial], |scheme| run(false, scheme));
    assert_animates_with(&[FastEffects], |scheme| run(true, scheme));
}

/// The scheme where a component reads it is the nearest theme's.
#[test]
fn a_component_uses_the_nearest_themes_scheme() {
    let checked = mutable_state_of(false);
    let read = checked.handle();
    let mut ui = themed(long_only(FastSpatial), move || {
        MaterialTheme!(motion_scheme = long_only(SlowSpatial), {
            Switch!(read.get(), Some(|_| {}));
        });
    });
    checked.set(true);
    assert!(idle_after(&mut ui) < SHORT);
}
