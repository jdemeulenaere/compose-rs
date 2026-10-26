//! Material review: failing tests for the icons, icon buttons, the floating action button,
//! `Switch`, `RadioButton`, `Slider`, `Card`, `TopAppBar` and `Scaffold` (docs/06-ui.md). Each
//! test states the input, what Jetpack Compose's material3 does, and what happens instead.
//!
//! Checked and sound (no test kept): the icons' path data (rendered through Skia, all fourteen
//! look right), `Icon`'s 24 dp default and semantics, `IconButton`'s 40 dp in 48 dp and disabled
//! color, the FAB's 56 dp, shape and colors, the switch's 52 x 32 dp track, its thumb sizes
//! (16 dp, 24 dp) and positions, its enabled colors and `on_checked_change = None` (no
//! toggleable, no touch target), the radio button's 20 dp ring and 2 dp padding, the slider's
//! stops and key steps, the dividers, the card's shape and enabled colors, the top app bar's
//! height, colors and title inset without a navigation icon, the scaffold's content padding and
//! FAB placement (16 dp from the end and above the bottom bar), and the scaffold's padding
//! loop, which converges within the first frame (the gallery is laid out in one frame). The SVG
//! path parser (`Path::from_svg`) handles number forms (`.5.5`, `1.e1`, `1-2`, exponents),
//! implicit commands after `M`, compact arc flags, `S`/`T` reflections after other commands, and
//! malformed input without panicking; its arcs match the SVG spec's conversion.
//!
//! Not tested (Skia allocates outside Rust's allocator): compose-skia's `DrawPath` replay builds
//! a new Skia path from the commands at every replay, so every icon on screen allocates a path
//! per frame whenever the display list is drawn (a cache keyed by the `Arc<Path>` would avoid it).

#![allow(non_snake_case)]

use std::cell::Cell;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Color, IntSize, Key, KeyEvent, Modifier, Offset, PaddingValues, PointerEvent, UnitExt,
    platform::DrawOp,
};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_with_tag(tag).fetch_semantics_node().bounds
}

fn round_rects(ui: &mut ComposeTestRule) -> Vec<(compose_ui::Rect, compose_ui::Paint)> {
    ui.host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawRoundRect(rect, _, paint) => Some((*rect, *paint)),
            _ => None,
        })
        .collect()
}

// ---- Scaffold --------------------------------------------------------------------------------

/// Compose's `Scaffold` places the content first and the top bar after it, and a layout's
/// children draw in placement order: content that runs under the top bar (a list given the
/// padding as its content padding, scrolled; or here a plain background) is covered by the bar.
/// Here the slots draw in composition order (top bar, content, bottom bar, FAB), so the content
/// draws over the top bar and hides it (rendered through Skia: a scrolled list in the gallery's
/// scaffold covers the whole bar).
#[test]
fn the_top_bar_is_drawn_over_the_content() {
    #[composable]
    fn Screen() {
        MaterialTheme!({
            Scaffold!(
                top_bar = {
                    TopAppBar!({ Text!("Title") });
                },
                |_padding: PaddingValues| {
                    Box!(modifier = Modifier.fill_max_size().background(Color::RED));
                }
            );
        });
    }
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 800), Screen);
    let ops = ui.host().display_list().ops;
    let content = ops
        .iter()
        .position(|op| matches!(op, DrawOp::DrawRect(_, paint) if paint.color == Color::RED))
        .expect("the content's background");
    let bar = ops
        .iter()
        .rposition(|op| matches!(op, DrawOp::DrawRect(rect, _) if rect.height() == 64.0))
        .expect("the top bar's background");
    assert!(bar > content, "the top bar (op {bar}) is drawn before the content (op {content})");
}

/// The same order decides hit testing in Compose: the top bar, placed last, takes the pointer
/// before the content under it. A list given the scaffold's padding as its content padding (as
/// Compose's samples do) fills the whole scaffold; a click on the navigation icon clicks it.
/// Here the content slot, the later child, is hit first, its list's scroll handler is under the
/// pointer, so the top bar is never hit and the click is lost.
#[test]
fn the_top_bar_takes_clicks_over_a_list_under_it() {
    thread_local! { static CLICKS: Cell<u32> = const { Cell::new(0) }; }
    #[composable]
    fn Screen() {
        MaterialTheme!({
            Scaffold!(
                top_bar = {
                    TopAppBar!(
                        { Text!("Title") },
                        navigation_icon = {
                            IconButton!(
                                on_click = || CLICKS.with(|clicks| clicks.set(clicks.get() + 1)),
                                modifier = Modifier.test_tag("navigation"),
                                {
                                    Icon!(icons::filled::menu(), Some("Menu"));
                                }
                            );
                        }
                    );
                },
                |padding: PaddingValues| {
                    LazyColumn!(
                        modifier = Modifier.fill_max_size(),
                        content_padding = padding,
                        |scope: LazyListScope| {
                            scope.items(50, |index| {
                                Text!("Item {}", index, modifier = Modifier.height(40.dp()));
                            });
                        }
                    );
                }
            );
        });
    }
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 800), Screen);
    let icon = bounds(&mut ui, "navigation");
    let center = Offset::new((icon.left + icon.right) / 2.0, (icon.top + icon.bottom) / 2.0);
    ui.host().send_pointer(PointerEvent::press(center));
    ui.host().send_pointer(PointerEvent::release(center));
    ui.wait_for_idle();
    assert_eq!(CLICKS.with(Cell::get), 1);
}

// ---- TopAppBar -------------------------------------------------------------------------------

/// Compose's `TopAppBarLayout` places the title box at `max(TopAppBarTitleInset (12 dp),
/// navigation icon's width)`, and the title box has its own 4 dp horizontal padding: with an
/// `IconButton` as the navigation icon (4 dp start padding + 48 dp) the title text starts at
/// 52 + 4 = 56 dp. Here the title box has no padding when there is a navigation icon, so the
/// text starts at 52 dp, 4 dp too close to the icon.
#[test]
fn the_title_starts_56_dp_in_after_a_navigation_icon() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            TopAppBar!(
                {
                    Text!("Title", modifier = Modifier.test_tag("title"));
                },
                navigation_icon = {
                    IconButton!(on_click = || {}, {
                        Icon!(icons::filled::menu(), Some("Menu"));
                    });
                }
            );
        });
    });
    assert_eq!(bounds(&mut ui, "title").left, 56.0);
}

// ---- Slider ----------------------------------------------------------------------------------

/// Compose's `Slider` is controlled: the thumb is drawn at the `value` it is given. A slider
/// whose `on_value_change` doesn't update its value (read-only, or one clamping to a bound it
/// is already at) keeps its thumb where it was. Here the node moves its own copy of the value on
/// a press and draws the thumb there, and nothing puts it back since the caller didn't
/// recompose: a read-only slider at 25 % shows its thumb at 75 % after a tap there.
#[test]
fn a_slider_whose_value_does_not_change_keeps_its_thumb() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 100), || {
        let state = remember_slider_state!(0.25);
        Slider!(
            state,
            |_| {},
            modifier = Modifier.width(220.dp()),
            thumb =
                Some(|_| Box!(modifier = Modifier.width(4.dp()).height(44.dp()).test_tag("thumb"))),
        );
    });
    // The handle, centered on its value along the 2..218 track.
    let thumb = |ui: &mut ComposeTestRule| {
        let rect = ui.on_node_unmerged(has_test_tag("thumb")).fetch_semantics_node().bounds;
        (rect.left + rect.right) / 2.0
    };
    assert_eq!(thumb(&mut ui), 56.0);
    ui.host().send_pointer(PointerEvent::press(Offset::new(160.0, 24.0)));
    ui.host().send_pointer(PointerEvent::release(Offset::new(160.0, 24.0)));
    ui.wait_for_idle();
    assert_eq!(thumb(&mut ui), 56.0, "the value is still 0.25");
}

/// In Compose a press on a slider only records where it is (`SliderState.onPress`); the value
/// moves on a tap or on a horizontal drag past the touch slop (`draggable(Horizontal)`). A
/// vertical swipe that starts on a slider in a vertically scrolling column scrolls the column
/// and leaves the slider alone. Here the press sets the value at once and the slider consumes
/// every move while pressed, in its Main pass before the column's: the swipe drags the slider
/// (its value follows the pointer's x) and the column never scrolls.
#[test]
fn a_vertical_swipe_on_a_slider_scrolls_its_column() {
    thread_local! { static VALUE: Cell<f32> = const { Cell::new(0.2) }; }
    #[composable]
    fn Screen() {
        let state = remember_scroll_state!();
        let slider = remember_slider_state!(VALUE.with(Cell::get));
        Column!(modifier = Modifier.fill_max_size().vertical_scroll(state), {
            Spacer(Modifier.height(100.dp()));
            Slider!(
                slider,
                |value| VALUE.with(|cell| cell.set(value)),
                modifier = Modifier.width(220.dp()).test_tag("slider")
            );
            Spacer(Modifier.height(1000.dp()));
        });
    }
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), Screen);
    let before = bounds(&mut ui, "slider");
    // From the middle of the track (50 %), straight up by 100 px.
    let host = ui.host();
    host.send_pointer(
        PointerEvent::press(Offset::new(110.0, before.top + 24.0))
            .with_pointer(compose_ui::PointerId(0), compose_ui::PointerType::Touch),
    );
    for step in 1..=10 {
        let y = before.top + 24.0 - 10.0 * step as f32;
        host.send_pointer(
            PointerEvent::moved(Offset::new(110.0, y), true)
                .with_pointer(compose_ui::PointerId(0), compose_ui::PointerType::Touch),
        );
    }
    host.send_pointer(
        PointerEvent::release(Offset::new(110.0, before.top - 76.0))
            .with_pointer(compose_ui::PointerId(0), compose_ui::PointerType::Touch),
    );
    ui.wait_for_idle();
    assert_eq!(VALUE.with(Cell::get), 0.2, "the slider kept its value");
    let after = bounds(&mut ui, "slider");
    assert!(after.top < before.top, "the column scrolled: {before:?} -> {after:?}");
}

/// Compose's slider draws its steps: a tick (a 4 dp dot) at each stop but the last (where the
/// stop dot is) and those in the gap around the handle (`SliderDefaults.Track`'s `drawTick`
/// and `drawStopIndicator`). At 40 % of 4 steps (stops at 0, 20, 40, 60, 80 and 100 %): the
/// stop dot and the ticks at 0, 20, 60 and 80 % (40 % is in the gap).
#[test]
fn a_slider_with_steps_draws_its_ticks() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 100), || {
        let state = remember_slider_state!(0.4, 4);
        Slider!(state, |_| {}, modifier = Modifier.width(220.dp()));
    });
    let dots: Vec<f32> = ui
        .host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawArc { oval, sweep_angle, .. } if *sweep_angle == 360.0 => {
                Some((oval.left + oval.right) / 2.0)
            }
            _ => None,
        })
        .collect();
    assert_eq!(dots, [208.0, 8.0, 48.0, 128.0, 168.0], "the stop dot and 4 ticks");
}

/// Compose's `slideOnKeyEvents` moves the value on `KeyDown` and calls
/// `onValueChangeFinished` on the `KeyUp` of the same keys: holding an arrow (repeated key
/// downs) moves the value step by step and finishes once, when released. Here every key down
/// finishes, so a held arrow finishes at every repeat (and a key down alone already did).
#[test]
fn a_slider_finishes_a_key_change_on_key_up() {
    thread_local! { static FINISHED: Cell<u32> = const { Cell::new(0) }; }
    thread_local! { static STATE: Cell<Option<SliderState>> = const { Cell::new(None) }; }
    #[composable]
    fn Screen() {
        let state = remember_slider_state!(0.0, 9);
        STATE.with(|cell| cell.set(Some(state)));
        Slider!(
            state,
            move |new| state.set_value(new),
            modifier = Modifier.width(220.dp()).test_tag("slider"),
            on_value_change_finished =
                Some(|| FINISHED.with(|finished| finished.set(finished.get() + 1))),
        );
    }
    let value = || STATE.with(Cell::get).expect("composed").value();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 100), Screen);
    ui.on_node_with_tag("slider").request_focus();
    // A held arrow: three key downs, then the key up.
    for _ in 0..3 {
        ui.host().send_key(KeyEvent::down(Key::DirectionRight));
        ui.wait_for_idle();
    }
    assert!((value() - 0.3).abs() < 0.001, "{}", value());
    assert_eq!(FINISHED.with(Cell::get), 0, "finished before the key was released");
    ui.host().send_key(KeyEvent::up(Key::DirectionRight));
    ui.wait_for_idle();
    assert_eq!(FINISHED.with(Cell::get), 1);
}

// ---- Card ------------------------------------------------------------------------------------

/// Compose's `CardColors` has disabled colors: a disabled clickable card
/// (`Card(onClick, enabled = false)`) shows its content in `disabledContentColor`
/// (`contentColor.copy(alpha = 0.38f)` by default, `CardDefaults.cardColors()`) on
/// `disabledContainerColor`. Here `CardColors` has no disabled colors and `enabled` only
/// disables the click: the content keeps `on_surface` at full alpha.
#[test]
fn a_disabled_card_dims_its_content() {
    thread_local! { static CONTENT: Cell<Color> = const { Cell::new(Color::UNSPECIFIED) }; }
    thread_local! { static EXPECTED: Cell<Color> = const { Cell::new(Color::UNSPECIFIED) }; }
    let _ui = compose_test(|| {
        MaterialTheme!({
            EXPECTED
                .with(|cell| cell.set(MaterialTheme::color_scheme().on_surface.with_alpha(0.38)));
            Card!(on_click = || {}, enabled = false, {
                CONTENT.with(|cell| cell.set(LOCAL_CONTENT_COLOR.current()));
                Text!("A card");
            });
        });
    });
    assert_eq!(CONTENT.with(Cell::get), EXPECTED.with(Cell::get));
}

// ---- FloatingActionButton --------------------------------------------------------------------

/// Compose's `FloatingActionButton` is at least 56 dp (`Modifier.defaultMinSize(56.dp, 56.dp)`
/// around its content), so wider content (an icon and a label in a row) widens it. Here it is
/// exactly 56 dp (`size((56, 56))`): a 120 dp wide content is clipped to 56 dp.
#[test]
fn a_floating_action_button_grows_with_its_content() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            FloatingActionButton!(on_click = || {}, modifier = Modifier.test_tag("fab"), {
                Box!(modifier = Modifier.size((120.dp(), 24.dp())));
            });
        });
    });
    assert_eq!(bounds(&mut ui, "fab").width(), 120.0);
}

// ---- RadioButton -----------------------------------------------------------------------------

/// Compose's `RadioButton` draws its dot with radius `dotRadius - strokeWidth / 2`, the dot
/// radius animating to `RadioButtonDotSize / 2` (6 dp): a 5 dp radius, 10 dp across, leaving a
/// 3 dp gap inside the 2 dp ring. Here the dot is drawn with the full 6 dp radius (12 dp
/// across, a 2 dp gap).
#[test]
fn a_selected_radio_buttons_dot_is_10_dp() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            RadioButton!(true, Some(|| {}));
        });
    });
    let (dot, _) = *round_rects(&mut ui).last().expect("the dot");
    assert_eq!(dot.width(), 10.0);
}

// ---- Switch ----------------------------------------------------------------------------------

/// Compose's `SwitchDefaults.colors()` has its own disabled colors: a disabled checked switch's
/// track is `onSurface` at 12 % (`disabledCheckedTrackColor`) and its thumb opaque `surface`
/// (`disabledCheckedThumbColor`). Here every color is drawn at 38 % alpha: the track is
/// `primary` at 38 %, the thumb `on_primary` at 38 % (a purple switch, faded, where Compose's
/// is gray).
#[test]
fn a_disabled_checked_switch_has_materials_disabled_colors() {
    thread_local! { static SCHEME: Cell<Option<ColorScheme>> = const { Cell::new(None) }; }
    let mut ui = compose_test(|| {
        MaterialTheme!({
            SCHEME.with(|cell| cell.set(Some(MaterialTheme::color_scheme())));
            Switch!(true, Some(|_| {}), enabled = false);
        });
    });
    let scheme = SCHEME.with(Cell::get).expect("composed");
    let shapes = round_rects(&mut ui);
    let (_, track) = shapes.first().expect("the track");
    let (_, thumb) = shapes.last().expect("the thumb");
    assert_eq!(
        track.color,
        scheme.on_surface.with_alpha(0.12).composite_over(scheme.surface),
        "the track, opaque"
    );
    assert_eq!(thumb.color, scheme.surface, "the thumb");
}

// ---- Icon ------------------------------------------------------------------------------------

/// Compose's `Icon` paints its vector with `ContentScale.Fit`: given a 48 x 24 dp size, the
/// 24 dp icon is drawn 24 x 24, centered (its aspect ratio kept). Here the viewport is scaled
/// to the size on each axis: the icon is stretched to twice its width.
#[test]
fn an_icon_keeps_its_aspect_ratio() {
    let mut ui = compose_test(|| {
        Icon!(icons::filled::add(), None, modifier = Modifier.size((48.dp(), 24.dp())));
    });
    let (offset, scale) = ui
        .host()
        .display_list()
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::DrawPath { offset, scale, .. } => Some((*offset, *scale)),
            _ => None,
        })
        .expect("the icon's path");
    assert_eq!(scale, (1.0, 1.0));
    assert_eq!(offset.x, 12.0);
}

/// Compose's `Alignment.Center` rounds the fitted icon's offset to whole pixels (its sizes
/// rounded, then the half of what's left): 37 x 30 fits the 24 dp viewport at 1.25, 30 x 30,
/// 3.5 px from the left, drawn at 4. On whole pixels, Android draws its mask unfiltered (D190).
#[test]
fn an_icon_is_centered_on_whole_pixels() {
    let mut ui = compose_test(|| {
        Icon!(icons::filled::add(), None, modifier = Modifier.size((37.dp(), 30.dp())));
    });
    let (offset, scale) = ui
        .host()
        .display_list()
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::DrawPath { offset, scale, .. } => Some((*offset, *scale)),
            _ => None,
        })
        .expect("the icon's path");
    assert_eq!(scale, (1.25, 1.25));
    assert_eq!((offset.x, offset.y), (4.0, 0.0));
}
