//! Fifteenth review round: `ModalNavigationDrawer`, `DrawerState`, `ModalDrawerSheet` and
//! `NavigationDrawerItem`, against Jetpack Compose's Material 3 (1.2/1.3). Each test states the
//! input, what Compose does, and what happens instead.

#![allow(non_snake_case)]

use std::time::Duration;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::{composable, remember_coroutine_scope};
use compose_test::*;
use compose_ui::{DrawOp, IntSize, LayoutDirection, Modifier, Offset, PointerEvent, UnitExt};

#[composable]
fn Sheet() {
    ModalDrawerSheet!(modifier = Modifier.test_tag("sheet"), {
        NavigationDrawerItem!(
            { Text!("Inbox") },
            true,
            || {},
            modifier = Modifier.test_tag("inbox")
        );
    });
}

#[composable]
fn Drawer(state: DrawerState, gestures_enabled: bool) {
    MaterialTheme!({
        ModalNavigationDrawer!(
            { Sheet() },
            drawer_state = state.clone(),
            gestures_enabled = gestures_enabled,
            {
                Box!(modifier = Modifier.fill_max_size().test_tag("content"));
            }
        );
    });
}

/// A drawer whose content is a button opening it.
#[composable]
fn Opener(state: DrawerState) {
    let scope = remember_coroutine_scope();
    MaterialTheme!({
        ModalNavigationDrawer!({ Sheet() }, drawer_state = state.clone(), {
            Box!(modifier = Modifier.fill_max_size(), {
                let (state, scope) = (state.clone(), scope.clone());
                Button!(
                    move || {
                        let state = state.clone();
                        scope.launch(async move { state.open().await });
                    },
                    modifier = Modifier.test_tag("open"),
                    { Text!("Open") }
                );
            });
        });
    });
}

fn sheet_bounds(ui: &mut ComposeTestRule) -> compose_ui::Rect {
    ui.on_node_with_tag("sheet").fetch_semantics_node().bounds
}

fn click(ui: &mut ComposeTestRule, at: Offset) {
    ui.host().send_pointer(PointerEvent::press(at));
    ui.host().send_pointer(PointerEvent::release(at));
}

/// Input: a closed drawer (gestures enabled) over an 800 x 600 window; a drag on the content
/// from x = 20 to x = 420, then released.
///
/// Compose: `ModalNavigationDrawer` puts `anchoredDraggable(drawerState.anchoredDraggableState,
/// enabled = gesturesEnabled)` on its outer box whatever the drawer's value, so a horizontal
/// drag on the content pulls the closed drawer out, and released past half it settles open
/// (the reason apps pass `gesturesEnabled = drawerState.isOpen` beside a pager).
///
/// Instead: the drag is only installed while the drawer is shown (`gestures_enabled && shown`):
/// a closed drawer ignores the drag and stays closed.
#[test]
fn dragging_the_content_opens_a_closed_drawer() {
    let state = DrawerState::new(DrawerValue::Closed);
    let given = state.clone();
    let mut ui =
        ComposeTestRule::sized(IntSize::new(800, 600), move || Drawer(given.clone(), true));
    ui.set_auto_advance(false);
    ui.host().send_pointer(PointerEvent::press(Offset::new(20.0, 300.0)));
    for x in [60.0, 140.0, 260.0, 420.0] {
        ui.host().send_pointer(PointerEvent::moved(Offset::new(x, 300.0), true));
        ui.advance_time_by_frame();
    }
    ui.host().send_pointer(PointerEvent::release(Offset::new(420.0, 300.0)));
    ui.advance_time_by(Duration::from_millis(500));
    assert!(state.is_open());
    assert_eq!(sheet_bounds(&mut ui).left, 0.0);
}

/// Input: an open drawer (360 px sheet); a quick flick on the sheet towards the start, 60 px
/// in three 16 ms frames (over 1000 px/s), then released.
///
/// Compose: the drawer's `AnchoredDraggableState` has a 400 dp/s velocity threshold
/// (`DrawerVelocityThreshold`): released faster than that, it settles at the next anchor in the
/// fling's direction, closed, however short the drag.
///
/// Instead: the drag's stop ignores its velocity (`on_drag_stopped` takes none, `settle` reads
/// only the position): at 83 % open the drawer springs back open.
#[test]
fn a_fast_fling_towards_the_start_closes_the_drawer() {
    let state = DrawerState::new(DrawerValue::Open);
    let given = state.clone();
    let mut ui =
        ComposeTestRule::sized(IntSize::new(800, 600), move || Drawer(given.clone(), true));
    ui.set_auto_advance(false);
    ui.host().send_pointer(PointerEvent::press(Offset::new(300.0, 500.0)));
    for x in [280.0, 260.0, 240.0] {
        ui.advance_time_by_frame();
        ui.host().send_pointer(PointerEvent::moved(Offset::new(x, 500.0), true));
    }
    ui.host().send_pointer(PointerEvent::release(Offset::new(240.0, 500.0)));
    ui.advance_time_by(Duration::from_millis(500));
    assert!(state.is_closed());
}

/// Input: an open drawer with `gestures_enabled = false`; a click on the scrim, beside the sheet.
///
/// Compose: the scrim's `onClose` is
/// `if (gesturesEnabled && confirmValueChange(Closed)) scope.launch { drawerState.close() }`:
/// with gestures disabled, a tap on the scrim leaves the drawer open (the app closes it).
///
/// Instead: the scrim's click always closes the drawer.
#[test]
fn the_scrim_does_not_close_the_drawer_when_gestures_are_disabled() {
    let state = DrawerState::new(DrawerValue::Open);
    let given = state.clone();
    let mut ui =
        ComposeTestRule::sized(IntSize::new(800, 600), move || Drawer(given.clone(), false));
    ui.set_auto_advance(false);
    click(&mut ui, Offset::new(600.0, 300.0));
    ui.advance_time_by(Duration::from_millis(500));
    assert!(state.is_open());
    assert_eq!(sheet_bounds(&mut ui).left, 0.0);
}

/// Input: a closed drawer; `open()` launched, looked at 64 ms into its 256 ms slide.
///
/// Compose: `DrawerState.currentValue` is the anchored-draggable state's `currentValue`, which
/// changes only once the offset reaches an anchor: "if a swipe or an animation is in progress,
/// this corresponds the state drawer was in before the swipe or animation started". Midway
/// through opening it is `Closed`, `isOpen` false and `isClosed` true, while `targetValue` is
/// `Open`.
///
/// Instead: `current_value` is `Open` from the moment opening starts (it reports the sheet being
/// shown), so `is_open()` is true before the drawer has opened.
#[test]
fn the_current_value_stays_closed_while_the_drawer_opens() {
    let state = DrawerState::new(DrawerValue::Closed);
    let given = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 600), move || Opener(given.clone()));
    ui.set_auto_advance(false);
    ui.on_node_with_tag("open").perform_click();
    ui.advance_time_by(Duration::from_millis(64));
    assert!(sheet_bounds(&mut ui).left < 0.0);
    assert_eq!(state.target_value(), DrawerValue::Open);
    assert_eq!(state.current_value(), DrawerValue::Closed);
    assert!(!state.is_open());
}

#[composable]
fn TwoRoots(state: DrawerState) {
    MaterialTheme!({
        ModalNavigationDrawer!({ Sheet() }, drawer_state = state.clone(), {
            Box!(modifier = Modifier.fill_max_size());
            Box!(modifier = Modifier.size(20.dp()).test_tag("badge"));
        });
    });
}

/// Input: a drawer whose content emits two nodes side by side in Compose's box (a full-size box
/// and a small one), opened.
///
/// Compose: `ModalNavigationDrawer` wraps `content()` in its own `Box { content() }`, so any
/// number of root nodes stack under the scrim, and the sheet slides in as usual.
///
/// Instead: `DrawerLayout` takes the first child as the content and expects exactly
/// `[scrim, sheet]` after it; the content's second node makes that three children, so neither
/// the scrim nor the sheet is placed (the sheet is composed but never laid out).
#[test]
fn content_with_two_root_nodes_still_shows_the_sheet() {
    let state = DrawerState::new(DrawerValue::Open);
    let given = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 600), move || TwoRoots(given.clone()));
    ui.wait_for_idle();
    let sheet = sheet_bounds(&mut ui);
    assert_eq!((sheet.left, sheet.width(), sheet.height()), (0.0, 360.0, 600.0));
}

#[composable]
fn InColumn(state: DrawerState) {
    MaterialTheme!({
        Column!({
            ModalNavigationDrawer!({ Sheet() }, drawer_state = state.clone(), {
                Text!("Hello");
            });
        });
    });
}

/// Input: an open drawer inside a `Column` (loose constraints), its content a small text.
///
/// Compose: `ModalNavigationDrawer`'s box is `modifier.fillMaxSize()`: it takes the whole
/// 800 x 600 space whatever its content, so the sheet is the window's height and 360 dp wide.
///
/// Instead: `DrawerLayout` sizes itself to its content: the drawer is the text's size, and the
/// sheet is squeezed to it.
#[test]
fn the_drawer_fills_the_space_whatever_its_content() {
    let state = DrawerState::new(DrawerValue::Open);
    let given = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 600), move || InColumn(given.clone()));
    ui.wait_for_idle();
    let sheet = sheet_bounds(&mut ui);
    assert_eq!((sheet.width(), sheet.height()), (360.0, 600.0));
}

/// Input: right to left, an open drawer: the sheet at the window's right edge (440..800).
///
/// Compose: `DrawerDefaults.shape` rounds the sheet's end corners (`shapes.large.end()`, a
/// `RoundedCornerShape(topEnd, bottomEnd)`), and a corner-based shape mirrors in right to left:
/// the rounded corners are the sheet's left ones, facing the content, its right edge square
/// against the window's.
///
/// Instead: `Shape::Rounded` corners are never mirrored (`Shape::outline` takes no layout
/// direction): the sheet is drawn with its right corners rounded, against the window's edge,
/// and square ones facing the content.
#[test]
fn right_to_left_the_sheets_rounded_corners_face_the_content() {
    let state = DrawerState::new(DrawerValue::Open);
    let given = state.clone();
    let mut ui =
        ComposeTestRule::sized(IntSize::new(800, 600), move || Drawer(given.clone(), true));
    ui.host().set_layout_direction(LayoutDirection::Rtl);
    ui.wait_for_idle();
    assert_eq!(sheet_bounds(&mut ui).left, 440.0);
    let radii: Vec<[f32; 4]> = ui
        .host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            // The sheet's surface (drawn in its own coordinates).
            DrawOp::DrawRoundRect(rect, radii, _)
                if rect.width() == 360.0 && rect.height() == 600.0 =>
            {
                Some(*radii)
            }
            _ => None,
        })
        .collect();
    // Top left, top right, bottom right, bottom left.
    assert_eq!(radii, vec![[16.0, 0.0, 0.0, 16.0]]);
}

#[composable]
fn TallItem() {
    MaterialTheme!({
        NavigationDrawerItem!(
            { Text!("One\nTwo\nThree\nFour") },
            false,
            || {},
            modifier = Modifier.test_tag("item")
        );
    });
}

/// Input: a `NavigationDrawerItem` whose label is four lines of text.
///
/// Compose: the item is `heightIn(min = 56.dp)`: 56 dp at least, taller when its label needs
/// it, so the four lines show.
///
/// Instead: the item is `height(56.dp())`: fixed at 56, its label cut off.
#[test]
fn a_drawer_item_grows_with_a_tall_label() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 600), TallItem);
    ui.wait_for_idle();
    let height = ui.on_node_with_tag("item").fetch_semantics_node().bounds.height();
    assert!(height > 56.0, "{height}");
}

#[composable]
fn Clicked(state: DrawerState, clicks: compose_runtime::MutableState<u32>) {
    MaterialTheme!({
        ModalNavigationDrawer!(
            {
                ModalDrawerSheet!(modifier = Modifier.test_tag("sheet"), {
                    NavigationDrawerItem!(
                        { Text!("Inbox") },
                        true,
                        move || clicks.set(clicks.get() + 1),
                        modifier = Modifier.test_tag("inbox")
                    );
                });
            },
            drawer_state = state.clone(),
            {
                Box!(modifier = Modifier.fill_max_size());
            }
        );
    });
}

/// Input: an open drawer; a drag that starts on its first item (at 200, 28) and swipes the
/// sheet 200 px towards the start, then is released over the item (at 0, 28).
///
/// Compose: past the touch slop the drawer's `anchoredDraggable` (on the drawer's box, an
/// ancestor) consumes the moves; the item's `clickable` tap detection
/// (`waitForUpOrCancellation`) checks the final pass and sees the moves consumed, so the press
/// is cancelled: the drawer closes and the item is not clicked.
///
/// Instead: the drawer closes, and the item's `on_click` runs too, as if tapped (navigating
/// while swiping the drawer away). `ClickableNode` (crates/compose-ui/src/nodes.rs) only looks
/// at moves in the main pass, before its ancestors' drag consumes them.
#[test]
fn swiping_the_drawer_closed_from_an_item_does_not_click_it() {
    let state = DrawerState::new(DrawerValue::Open);
    let clicks = compose_runtime::mutable_state_of(0u32);
    let (given, counted) = (state.clone(), *clicks);
    let mut ui =
        ComposeTestRule::sized(IntSize::new(800, 600), move || Clicked(given.clone(), counted));
    ui.set_auto_advance(false);
    ui.host().send_pointer(PointerEvent::press(Offset::new(200.0, 28.0)));
    for x in [180.0, 120.0, 60.0, 0.0] {
        ui.host().send_pointer(PointerEvent::moved(Offset::new(x, 28.0), true));
        ui.advance_time_by_frame();
    }
    ui.host().send_pointer(PointerEvent::release(Offset::new(0.0, 28.0)));
    ui.advance_time_by(Duration::from_millis(500));
    assert!(state.is_closed());
    assert_eq!(clicks.get(), 0);
}

/// Input: an open drawer over content with nothing focusable, its sheet holding one item;
/// Tab pressed twice.
///
/// Compose: the scrim is a `Canvas` with `pointerInput { detectTapGestures { onClose() } }` and
/// an `onClick` semantics action: it is not focusable, so Tab only cycles through the item.
///
/// Instead: the scrim is `clickable_with(…)`, a focus target: the second Tab moves focus to the
/// window-wide scrim, which draws no focus indication (focus seems to vanish), and Enter there
/// closes the drawer.
#[test]
fn tab_never_focuses_the_scrim() {
    let state = DrawerState::new(DrawerValue::Open);
    let clicks = compose_runtime::mutable_state_of(0u32);
    let (given, counted) = (state.clone(), *clicks);
    let mut ui =
        ComposeTestRule::sized(IntSize::new(800, 600), move || Clicked(given.clone(), counted));
    ui.wait_for_idle();
    for _ in 0..2 {
        ui.press_key(compose_ui::Key::Tab);
        let scrim =
            ui.on_node_with_content_description("Close navigation menu").fetch_semantics_node();
        assert!(!scrim.config.focused);
    }
}
