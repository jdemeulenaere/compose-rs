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
    Color, Focus, FocusInteraction, Modifier, MutableInteractionSource, PaintStyle, UnitExt,
    platform::DrawOp,
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

/// A segmented button's check mark grows on the fast spatial spec.
#[test]
fn a_segmented_button_animates_with_the_scheme() {
    assert_animates_with(&[FastSpatial], |scheme| {
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
