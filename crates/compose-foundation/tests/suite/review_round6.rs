//! Sixth review round: `BoxWithConstraints`, `LazyVerticalGrid`, scrollbars and hover, against
//! Jetpack Compose and Compose Desktop. Each test states the input, what Compose does, and what
//! happens instead.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{DrawOp, IntSize, Offset, PointerEvent, Rect, UnitExt};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> Rect {
    ui.on_node_with_tag(tag).fetch_semantics_node().bounds
}

// ---- LazyVerticalGrid ----------------------------------------------------------------------

/// `rememberLazyGridState(initialFirstVisibleItemIndex = 10)` in a 3-column grid: Compose's
/// `LazyGridState` holds an item index, the grid shows the line holding item 10 (items 9 to
/// 11) at the top, and `firstVisibleItemIndex` is 9. Here the index is given to the inner
/// `LazyListState` as a row index: row 10 (items 30 to 32) shows, `firstVisibleItemIndex` 30.
#[test]
fn a_grid_state_initial_index_is_an_item_index() {
    let state = LazyGridState::new(10, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 200), move || {
        LazyVerticalGrid!(GridCells::Fixed(3), state = given, |scope: LazyGridScope| {
            scope.items(100, |index| {
                Box!(modifier = Modifier.height(50.dp()).test_tag(format!("{index}")));
            });
        });
    });
    assert_eq!(state.first_visible_item_index(), 9);
    assert_eq!(bounds(&mut ui, "10").top, 0.0);
}

/// A grid scrolled to item 30 (3 columns: row 10) gets wider and holds 4 columns: Compose's
/// `LazyGridState` keeps item 30 in view (its line, items 28 to 31, at the top,
/// `firstVisibleItemIndex` 28). Here the inner list looks for row 10's key (item 30's), which
/// starts no row with 4 columns, and keeps row 10: items 40 to 43, the grid jumping 10 items on
/// a window resize. (With 6 columns it happens to find it: item 30 starts row 5.)
#[test]
fn a_grid_keeps_its_first_item_when_its_column_count_changes() {
    let state = LazyGridState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 200), move || {
        LazyVerticalGrid!(GridCells::Adaptive(100.dp()), state = given, |scope: LazyGridScope| {
            scope.items(300, |index| {
                Box!(modifier = Modifier.height(50.dp()).test_tag(format!("{index}")));
            });
        });
    });
    state.scroll_to_item(30, 0);
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 30);
    ui.host().resize(IntSize::new(400, 200));
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 28, "item 30's line first");
    ui.on_node_with_tag("30").assert_exists();
    assert_eq!(bounds(&mut ui, "30").top, 0.0);
}

thread_local! {
    static CREATED: Cell<usize> = const { Cell::new(0) };
}

#[composable]
fn Counted(key: String) {
    remember(|| CREATED.with(|created| created.set(created.get() + 1)));
    Box!(modifier = Modifier.height(50.dp()).test_tag(key.clone()));
}

/// A keyed grid of a, b, c, d in 2 columns gets z inserted first: Compose's lazy grid keeps
/// each item's composition by its key, so only z is composed anew (its `remember` runs once).
/// Here a row is keyed by its first item and its cells by position: rows [z a] [b c] [d] match
/// none of [a b] [c d], and every item is composed again, its remembered state lost.
#[test]
#[ignore = "known difference, D144: a grid keeps item state by row, not by item key"]
fn a_keyed_grid_keeps_each_items_state_by_its_key() {
    let keys = mutable_state_of(vec!["a", "b", "c", "d"]);
    let handle = keys.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 400), move || {
        let keys = handle.get();
        LazyVerticalGrid!(GridCells::Fixed(2), |scope: LazyGridScope| {
            let (by_key, by_content) = (keys.clone(), keys.clone());
            scope.items_keyed(
                by_key.len(),
                move |index| by_key[index],
                move |index| Counted(by_content[index].to_string()),
            );
        });
    });
    assert_eq!(CREATED.with(Cell::get), 4);
    keys.set(vec!["z", "a", "b", "c", "d"]);
    ui.wait_for_idle();
    ui.on_node_with_tag("z").assert_exists();
    assert_eq!(CREATED.with(Cell::get), 5, "only z composed anew");
}

// ---- Scrollbars ------------------------------------------------------------------------------

fn thumb(ui: &mut ComposeTestRule) -> Option<Rect> {
    ui.host().display_list().ops.iter().find_map(|op| match op {
        DrawOp::DrawRoundRect(rect, _, _) => Some(*rect),
        _ => None,
    })
}

/// A lazy column of 4 items 50 px high, 10 px apart, in a 230 px viewport: everything fits
/// (4 × 50 + 3 × 10 = 230). Compose Desktop's lazy adapter counts the spacing between items
/// (`averageVisibleLineSize * count + lineSpacing * (count - 1)`): 230, no thumb. Here the
/// content is `count × (average + spacing)` = 240, one spacing too many: a thumb is drawn over
/// a list that can't scroll.
#[test]
fn a_lazy_scrollbar_hides_when_items_with_spacing_fit() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 230), || {
        let list = remember_lazy_list_state(0, 0);
        Row!(modifier = Modifier.fill_max_size(), |row: RowScope| {
            LazyColumn!(
                modifier = Modifier.weight(row, 1.0),
                state = list,
                vertical_arrangement = Arrangement::spaced_by(10.dp()),
                |scope: LazyListScope| {
                    scope.items(4, |_| Box!(modifier = Modifier.size(50.dp())));
                }
            );
            let adapter = remember_lazy_list_scrollbar_adapter(list);
            VerticalScrollbar!(adapter, modifier = Modifier.fill_max_height());
        });
    });
    assert_eq!(thumb(&mut ui), None, "the list can't scroll");
}

/// The same list of 50 px items 10 px apart, 20 of them, scrolled to its end: the thumb ends
/// at the track's end, as Compose Desktop's (its scroll offset is then content − viewport).
/// Here the content is overestimated by a spacing, the thumb stops short of the end.
#[test]
fn a_lazy_scrollbar_thumb_reaches_the_end_with_spacing() {
    let list = LazyListState::new(0, 0);
    let given = *list;
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 200), move || {
        let list = given;
        Row!(modifier = Modifier.fill_max_size(), |row: RowScope| {
            LazyColumn!(
                modifier = Modifier.weight(row, 1.0),
                state = list,
                vertical_arrangement = Arrangement::spaced_by(40.dp()),
                |scope: LazyListScope| {
                    scope.items(20, |_| Box!(modifier = Modifier.size(50.dp())));
                }
            );
            let adapter = remember_lazy_list_scrollbar_adapter(list);
            VerticalScrollbar!(adapter, modifier = Modifier.fill_max_height());
        });
    });
    list.scroll_to_item(19, 0);
    ui.wait_for_idle();
    assert!(!list.can_scroll_forward());
    let thumb = thumb(&mut ui).expect("a thumb");
    assert_eq!(thumb.bottom.round(), 200.0, "at the end: {thumb:?}");
}

// ---- Hover -----------------------------------------------------------------------------------

fn layer_alpha(ui: &mut ComposeTestRule) -> Option<(u8, Rect)> {
    ui.host().display_list().ops.iter().find_map(|op| match op {
        DrawOp::DrawRect(rect, paint)
            if paint.color.red() == 255 && paint.color.alpha() < 255 && paint.color.alpha() > 0 =>
        {
            Some((paint.color.alpha(), *rect))
        }
        _ => None,
    })
}

/// A hovered clickable whose modifier chain changes (a border added while hovered, a common
/// `Modifier.then(if (hovered) … else Modifier)`), then the mouse leaves it: Compose's hover
/// tracks the node, which hears `Exit`, its hover state layer gone. Here the chain change
/// bumps the node's chain version; the hovered path (unlike the press captures,
/// `remap_captures`) keeps the old version, so `Exit` is dropped by `deliver_pointer` and the
/// clickable stays hovered for good.
#[test]
fn a_hovered_clickable_whose_modifiers_change_still_hears_exit() {
    let bordered = mutable_state_of(false);
    let handle = bordered.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        Column!({
            let modifier = Modifier.size(50.dp());
            let modifier = if handle.get() { modifier.padding(1.dp()) } else { modifier };
            Box!(
                modifier =
                    modifier.clickable_indicated(true, None, Color::RED, || {}).size(48.dp())
            );
            Box!(modifier = Modifier.size(50.dp()));
        });
    });
    ui.host().send_pointer(PointerEvent::moved(Offset::new(10.0, 10.0), false));
    ui.wait_for_idle();
    assert_eq!(layer_alpha(&mut ui).map(|(alpha, _)| alpha), Some(20));
    bordered.set(true);
    ui.wait_for_idle();
    ui.host().send_pointer(PointerEvent::moved(Offset::new(10.0, 150.0), false));
    ui.wait_for_idle();
    assert_eq!(layer_alpha(&mut ui), None, "no longer hovered");
}

/// The mouse rests over the first item of a list of clickables and the wheel scrolls the list
/// by one item: Compose Desktop sends a synthetic move after the layout changes under the
/// pointer, so the item that left hears `Exit` and the one now under the pointer `Enter`: the
/// hover layer is on the item under the pointer. Here a scroll event updates no hover (only
/// moves do): the item scrolled away keeps its hover layer, the one under the pointer has none.
#[test]
fn scrolling_under_a_resting_mouse_moves_the_hover() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        LazyColumn!(modifier = Modifier.fill_max_size(), |scope: LazyListScope| {
            scope.items(50, |index| {
                // Each item's state layer told apart by its green.
                let color = Color::from_rgb(255, index as u8, 0);
                Box!(
                    modifier = Modifier.fill_max_width().height(50.dp()).clickable_indicated(
                        true,
                        None,
                        color,
                        || {}
                    )
                );
            });
        });
    });
    let hovered_item = |ui: &mut ComposeTestRule| {
        ui.host().display_list().ops.iter().find_map(|op| match op {
            DrawOp::DrawRect(_, paint)
                if paint.color.red() == 255
                    && paint.color.alpha() > 0
                    && paint.color.alpha() < 255 =>
            {
                Some(paint.color.green())
            }
            _ => None,
        })
    };
    ui.host().send_pointer(PointerEvent::moved(Offset::new(10.0, 25.0), false));
    ui.wait_for_idle();
    assert_eq!(hovered_item(&mut ui), Some(0));
    ui.host().send_pointer(PointerEvent::scroll(Offset::new(10.0, 25.0), Offset::new(0.0, 50.0)));
    ui.wait_for_idle();
    // Item 1 is now at the top, under the pointer.
    assert_eq!(hovered_item(&mut ui), Some(1), "the item under the pointer is hovered");
}

// ---- BoxWithConstraints --------------------------------------------------------------------

/// A window's first frame with a `BoxWithConstraints` choosing a branch by width: Compose
/// composes its content while measuring, with the real constraints: the narrow branch is never
/// composed in a wide window, its effects never run. Here the content composes first with zero
/// constraints: the narrow branch is composed (its `remember`s run, effects launched).
#[test]
fn box_with_constraints_never_composes_for_constraints_it_was_not_given() {
    let narrow = Rc::new(Cell::new(0));
    let counted = narrow.clone();
    let _ui = ComposeTestRule::sized(IntSize::new(400, 300), move || {
        let counted = counted.clone();
        BoxWithConstraints!(
            modifier = Modifier.fill_max_size(),
            move |scope: BoxWithConstraintsScope| {
                if scope.max_width() > 300.dp() {
                    BasicText!("wide");
                } else {
                    let counted = counted.clone();
                    remember(move || counted.set(counted.get() + 1));
                    BasicText!("narrow");
                }
            }
        );
    });
    assert_eq!(narrow.get(), 0, "the narrow branch composed");
}

thread_local! {
    static SQUARES: Cell<usize> = const { Cell::new(0) };
}

#[composable]
fn Square() {
    remember(|| SQUARES.with(|created| created.set(created.get() + 1)));
    BoxWithConstraints!(modifier = Modifier.fill_max_width(), |scope: BoxWithConstraintsScope| {
        Box!(modifier = Modifier.size(scope.max_width()));
    });
}

/// A lazy column of 1000 square items, each a `BoxWithConstraints` as tall as it is wide (200
/// px in a 200 × 400 window): Compose composes the two items in view and a few beyond. Here an
/// item's content first composes with zero constraints, 0 px high: the list sees a viewport of
/// empty items and composes as many as it may (up to 200 at once) before they get their size,
/// a composition far bigger than what shows.
#[test]
fn lazy_items_with_box_with_constraints_compose_only_what_shows() {
    let _ui = ComposeTestRule::sized(IntSize::new(200, 400), || {
        LazyColumn!(modifier = Modifier.fill_max_size(), |scope: LazyListScope| {
            scope.items(1000, |_| Square());
        });
    });
    let composed = SQUARES.with(Cell::get);
    assert!(composed <= 10, "{composed} items composed for 2 in view");
}

/// A horizontal scroll and its `HorizontalScrollbar` in a right-to-left layout, at scroll 0:
/// the scroll shows its start, on the right, and Compose Desktop's `HorizontalScrollbar`
/// reverses itself in RTL (`reverseLayout` flipped when `LocalLayoutDirection` is Rtl), its
/// thumb on the right. Here the thumb stays on the left, pointing at the content's end.
#[test]
fn a_horizontal_scrollbar_starts_on_the_right_in_rtl() {
    let state = ScrollState::new(0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 100), move || {
        let state = given;
        Column!(modifier = Modifier.fill_max_size(), {
            Row!(modifier = Modifier.fill_max_width().horizontal_scroll(state), {
                for _ in 0..20 {
                    Box!(modifier = Modifier.size(50.dp()));
                }
            });
            let adapter = remember_scrollbar_adapter(state);
            HorizontalScrollbar!(adapter, modifier = Modifier.fill_max_width());
        });
    });
    ui.host().set_layout_direction(compose_ui::LayoutDirection::Rtl);
    ui.wait_for_idle();
    // 200 of 1000 px: a 40 px thumb, at the right end.
    let thumb = thumb(&mut ui).expect("a thumb");
    assert_eq!((thumb.left, thumb.width()), (160.0, 40.0));
}

/// A grid of square cells, each a `BoxWithConstraints` as tall as it is wide (the usual
/// photo grid): Compose composes each cell while measuring it, with its real constraints, and
/// the grid opens at its top (`firstVisibleItemIndex` 0, cell 0 at y 0). Here the cells' first
/// composition has zero constraints, so the first row is 0 px high; the lazy list steps its
/// first visible item over a zero-size row (as Compose's does, `currentMainAxisOffset <= 0`),
/// and when the cells get their size the grid stays scrolled a row down: cell 0 at y −100,
/// `firstVisibleItemIndex` 5.
#[test]
fn a_grid_of_box_with_constraints_cells_opens_at_its_top() {
    let state = LazyGridState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(500, 400), move || {
        LazyVerticalGrid!(GridCells::Adaptive(100.dp()), state = given, |grid: LazyGridScope| {
            grid.items(100, |index| {
                BoxWithConstraints!(move |cell: BoxWithConstraintsScope| {
                    Box!(
                        modifier =
                            Modifier.size(cell.max_width()).test_tag(format!("cell {index}"))
                    );
                });
            });
        });
    });
    assert_eq!(state.first_visible_item_index(), 0);
    assert_eq!(bounds(&mut ui, "cell 0").top, 0.0);
}

/// Edge cases that must not panic: an empty grid, `GridCells::Fixed(0)` (Compose throws;
/// here one column), an adaptive grid narrower than its spacing, and scrollbars with
/// zero-length tracks. A lock-in.
#[test]
fn passes_degenerate_grids_and_scrollbars_do_not_panic() {
    let mut ui = ComposeTestRule::sized(IntSize::new(5, 300), || {
        Column!({
            LazyVerticalGrid!(
                GridCells::Fixed(0),
                modifier = Modifier.height(50.dp()),
                |scope: LazyGridScope| {
                    scope.items(3, |index| {
                        Box!(modifier = Modifier.height(10.dp()).test_tag(format!("{index}")));
                    });
                }
            );
            LazyVerticalGrid!(
                GridCells::Adaptive(100.dp()),
                modifier = Modifier.height(50.dp()),
                horizontal_arrangement = Arrangement::spaced_by(20.dp()),
                |scope: LazyGridScope| {
                    scope.items(3, |_| Box!(modifier = Modifier.height(10.dp())));
                }
            );
            LazyVerticalGrid!(
                GridCells::Fixed(3),
                modifier = Modifier.height(50.dp()),
                |_scope: LazyGridScope| {}
            );
            let state = remember(|| ScrollState::new(0));
            Box!(modifier = Modifier.height(0.dp()).vertical_scroll(state), {
                Box!(modifier = Modifier.size(100.dp()));
            });
            let adapter = remember_scrollbar_adapter(state);
            VerticalScrollbar!(adapter.clone(), modifier = Modifier.height(0.dp()));
            HorizontalScrollbar!(adapter, modifier = Modifier.width(0.dp()));
        });
    });
    assert_eq!(bounds(&mut ui, "1").top, 10.0, "one column");
    ui.host().send_pointer(PointerEvent::press(Offset::new(1.0, 150.0)));
    ui.host().send_pointer(PointerEvent::release(Offset::new(1.0, 150.0)));
    ui.wait_for_idle();
}

/// A mouse pressing a clickable, dragging onto another and releasing there: as Compose, the
/// first keeps its hover during the drag (its pointer is captured) and hears `Exit` at the
/// release, the second `Enter`. A disabled clickable shows no hover layer. A lock-in.
#[test]
fn passes_hover_moves_at_the_release_of_a_drag() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        Column!({
            Box!(
                modifier = Modifier.size(50.dp()).clickable_indicated(
                    true,
                    None,
                    Color::from_rgb(255, 1, 0),
                    || {}
                )
            );
            Box!(
                modifier = Modifier.size(50.dp()).clickable_indicated(
                    true,
                    None,
                    Color::from_rgb(255, 2, 0),
                    || {}
                )
            );
            Box!(
                modifier = Modifier.size(50.dp()).clickable_indicated(
                    false,
                    None,
                    Color::from_rgb(255, 3, 0),
                    || {}
                )
            );
        });
    });
    let layers = |ui: &mut ComposeTestRule| -> Vec<(u8, u8)> {
        ui.host()
            .display_list()
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::DrawRect(_, paint)
                    if paint.color.red() == 255
                        && paint.color.alpha() > 0
                        && paint.color.alpha() < 255 =>
                {
                    Some((paint.color.green(), paint.color.alpha()))
                }
                _ => None,
            })
            .collect()
    };
    ui.host().send_pointer(PointerEvent::moved(Offset::new(10.0, 10.0), false));
    ui.host().send_pointer(PointerEvent::press(Offset::new(10.0, 10.0)));
    ui.host().send_pointer(PointerEvent::moved(Offset::new(10.0, 60.0), true));
    ui.wait_for_idle();
    // Out of its bounds the press is cancelled (as Compose's tap detection), the hover kept.
    assert_eq!(layers(&mut ui), vec![(1, 20)], "still hovered, the second not");
    ui.host().send_pointer(PointerEvent::release(Offset::new(10.0, 60.0)));
    ui.wait_for_idle();
    assert_eq!(layers(&mut ui), vec![(2, 20)]);
    ui.host().send_pointer(PointerEvent::moved(Offset::new(10.0, 110.0), false));
    ui.wait_for_idle();
    assert_eq!(layers(&mut ui), vec![], "disabled: no hover layer");
}
