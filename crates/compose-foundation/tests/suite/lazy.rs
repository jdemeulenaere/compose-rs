//! `LazyColumn` and `LazyRow`: only the items in view are composed; scrolling composes the next
//! ones within the frame; item state goes with keys.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, Offset, PointerEvent, UnitExt};

thread_local! {
    static COMPOSED: Cell<usize> = const { Cell::new(0) };
}

#[composable]
fn Row(index: usize) {
    COMPOSED.with(|composed| composed.set(composed.get() + 1));
    Box!(modifier = Modifier.size((100.dp(), 20.dp())).test_tag(format!("row {index}")), {
        BasicText!("item {}", index);
    });
}

#[composable]
fn List(state: LazyListState) {
    LazyColumn!(
        modifier = Modifier.size((100.dp(), 100.dp())),
        state = state,
        |scope: LazyListScope| {
            scope.items(1000, |index| Row(index));
        }
    );
}

fn list() -> (ComposeTestRule, Owned<LazyListState>) {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let ui = ComposeTestRule::sized(IntSize::new(200, 200), move || List(given));
    (ui, state)
}

#[test]
fn only_the_items_in_view_and_a_few_beyond_are_composed() {
    COMPOSED.with(|composed| composed.set(0));
    let (mut ui, _) = list();
    ui.on_node_with_text("item 0").assert_exists();
    ui.on_node_with_text("item 4").assert_exists();
    // 5 in view (100 / 20), and a viewport's worth beyond.
    let composed = COMPOSED.with(Cell::get);
    assert!((5..=12).contains(&composed), "{composed} items composed");
    ui.on_node_with_text("item 50").assert_does_not_exist();
}

#[test]
fn scrolling_composes_the_items_it_reaches_in_the_same_frame() {
    let (mut ui, state) = list();
    // A scroll of 210 px: item 10 at the top, 10 px off.
    state.dispatch_raw_delta(210.0);
    let frames = ui.host().settle(10);
    assert_eq!(state.first_visible_item_index(), 10);
    assert_eq!(state.first_visible_item_scroll_offset(), 10);
    assert!(frames <= 2, "{frames} frames to settle a scroll");
    let row = ui.on_node_with_tag("row 10").fetch_semantics_snapshot().bounds;
    assert_eq!(row.top, -10.0);
    ui.on_node_with_text("item 0").assert_does_not_exist();
}

#[test]
fn scroll_to_item_jumps_far_and_clamps_at_the_end() {
    let (mut ui, state) = list();
    state.request_scroll_to_item(500, 0);
    ui.wait_for_idle();
    ui.on_node_with_text("item 500").assert_exists();
    assert_eq!(ui.on_node_with_tag("row 500").fetch_semantics_snapshot().bounds.top, 0.0);
    // Past the end: the last item ends the viewport.
    state.request_scroll_to_item(999, 0);
    ui.wait_for_idle();
    assert_eq!(ui.on_node_with_tag("row 999").fetch_semantics_snapshot().bounds.bottom, 100.0);
    assert_eq!(state.first_visible_item_index(), 995);
    assert!(!state.can_scroll_forward());
    assert!(state.can_scroll_backward());
}

#[test]
fn a_drag_scrolls_the_list() {
    let (mut ui, state) = list();
    let host = ui.host();
    host.send_pointer(
        PointerEvent::press(Offset::new(50.0, 90.0))
            .with_pointer(compose_ui::PointerId(0), compose_ui::PointerType::Touch),
    );
    for y in [80.0, 70.0, 60.0, 50.0, 40.0] {
        host.send_pointer(
            PointerEvent::moved(Offset::new(50.0, y), true)
                .with_pointer(compose_ui::PointerId(0), compose_ui::PointerType::Touch),
        );
    }
    host.send_pointer(
        PointerEvent::release(Offset::new(50.0, 40.0))
            .with_pointer(compose_ui::PointerId(0), compose_ui::PointerType::Touch),
    );
    ui.wait_for_idle();
    // Moved 50 px, the first 8 within the slop.
    assert_eq!(state.first_visible_item_index(), 2);
}

#[test]
fn keyed_items_keep_their_state_when_items_before_them_go() {
    #[composable]
    fn Counter(label: String) {
        let count = remember(|| mutable_state_of(0));
        Box!(
            modifier = Modifier
                .size((100.dp(), 20.dp()))
                .clickable(move || count.update(|count| count + 1))
                .test_tag(label.clone()),
            {
                BasicText!("{} {}", label, count.get());
            }
        );
    }
    let items = mutable_state_of(vec![String::from("a"), String::from("b"), String::from("c")]);
    let handle = items.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        let items = Rc::new(handle.get());
        LazyColumn!(modifier = Modifier.size((100.dp(), 100.dp())), |scope: LazyListScope| {
            let (keys, labels) = (items.clone(), items.clone());
            scope.items_keyed(
                items.len(),
                move |index| keys[index].clone(),
                move |index| {
                    Counter(labels[index].clone());
                },
            );
        });
    });
    ui.on_node_with_tag("b").perform_click();
    ui.on_node_with_text("b 1").assert_exists();
    items.edit(|items| {
        items.remove(0);
    });
    ui.wait_for_idle();
    ui.on_node_with_text("b 1").assert_exists();
    assert_eq!(ui.on_node_with_tag("b").fetch_semantics_snapshot().bounds.top, 0.0);
}

#[test]
fn a_lazy_row_lays_out_sideways_with_padding_and_spacing() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 100), || {
        LazyRow!(
            modifier = Modifier.size((200.dp(), 50.dp())),
            content_padding = PaddingValues::new(10.dp(), 0.dp(), 10.dp(), 0.dp()),
            horizontal_arrangement = Arrangement::spaced_by(5.dp()),
            |scope: LazyListScope| {
                scope.item(|| {
                    Box!(modifier = Modifier.size((40.dp(), 40.dp())).test_tag("first"));
                });
                scope.items(50, |index| {
                    Box!(
                        modifier =
                            Modifier.size((40.dp(), 40.dp())).test_tag(format!("cell {index}"))
                    );
                });
            }
        );
    });
    assert_eq!(ui.on_node_with_tag("first").fetch_semantics_snapshot().bounds.left, 10.0);
    assert_eq!(ui.on_node_with_tag("cell 0").fetch_semantics_snapshot().bounds.left, 55.0);
    ui.on_node_with_tag("cell 10").assert_does_not_exist();
}

/// Scrolling moves the items without drawing them again: each item's repaint boundary keeps
/// what it drew and its version (docs/06-ui.md, "Repaint boundaries").
#[test]
fn scrolling_keeps_what_the_items_drew() {
    let (mut ui, state) = list();
    let before = ui.host().display_list();
    assert!(before.groups.len() >= 5, "{}", before.dump());
    // Less than an item: the same items, moved.
    assert!(state.dispatch_raw_delta(13.0) != 0.0);
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_scroll_offset(), 13);
    let after = ui.host().display_list();
    let mut kept = 0;
    // The items': the list's own (its clip's layer, D523) holds them, and draws them where
    // they moved.
    let holds_others = |group: &compose_ui::platform::FlatGroup| {
        after.groups.iter().any(|other| {
            other.key != group.key && other.start >= group.start && other.end <= group.end
        })
    };
    for group in after.groups.iter().filter(|group| !holds_others(group)) {
        if let Some(earlier) = before.groups.iter().find(|earlier| earlier.key == group.key) {
            assert_eq!(group.version, earlier.version, "{group:?} drawn again");
            assert_eq!(after.ops[group.start..group.end], before.ops[earlier.start..earlier.end]);
            kept += 1;
        }
    }
    assert!(kept >= 5, "{kept} items kept\n{}", after.dump());
    assert_ne!(after.dump(), before.dump());
}

thread_local! {
    static FIRST_BOTTOM: Cell<f32> = const { Cell::new(0.0) };
    static FIRST_COMPOSED: Cell<usize> = const { Cell::new(0) };
}

/// The first frame of a `height` px list of a `header` px item then `row` px rows: how far
/// down its items reach, and how many it composed.
fn first_frame(height: f32, header: f32, row: f32) -> (f32, usize) {
    let note = |coordinates: compose_ui::LayoutCoordinates| {
        let bottom = coordinates.bounds_in_root().bottom;
        FIRST_BOTTOM.with(|cell| cell.set(cell.get().max(bottom)));
    };
    let mut host = compose_ui::platform::UiHost::headless(100, height as i32);
    host.set_content(move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), height.dp())),
            move |scope: LazyListScope| {
                scope.item(move || {
                    FIRST_COMPOSED.with(|cell| cell.set(cell.get() + 1));
                    Box!(
                        modifier =
                            Modifier.size((100.dp(), header.dp())).on_globally_positioned(note)
                    );
                });
                scope.items(2000, move |_| {
                    FIRST_COMPOSED.with(|cell| cell.set(cell.get() + 1));
                    Box!(
                        modifier = Modifier.size((100.dp(), row.dp())).on_globally_positioned(note)
                    );
                });
            }
        );
    });
    FIRST_BOTTOM.with(|cell| cell.set(0.0));
    FIRST_COMPOSED.with(|cell| cell.set(0));
    host.frame();
    (FIRST_BOTTOM.with(Cell::get), FIRST_COMPOSED.with(Cell::get))
}

/// Whatever its first item and rows, a list's first frame fills its viewport, as Compose's (its
/// measurement composes items until it's full), composing few items it then doesn't show
/// (D472, D475): headers from 20 to 1,500 px, rows from 5 to 700 px, viewports of 500 to
/// 2,244 px.
#[test]
fn the_first_frame_fills_the_viewport_whatever_the_sizes() {
    let mut short = Vec::new();
    for height in [500.0, 800.0, 2244.0] {
        for header in [20.0, 81.0, 300.0, 600.0, 1500.0] {
            for row in [5.0, 20.0, 50.0, 120.0, 300.0, 700.0] {
                let (bottom, composed) = first_frame(height, header, row);
                if bottom < height {
                    short.push((height, header, row, bottom));
                }
                // The items in view, and those the estimates' last round asked for beyond.
                let in_view = 1 + ((height - header).max(0.0) / row).ceil() as usize;
                assert!(
                    composed <= in_view + (in_view / 2).max(4),
                    "{height} px, {header} px header, {row} px rows: {composed} composed, {in_view} in view"
                );
            }
        }
    }
    assert!(short.is_empty(), "first frames short (height, header, row, bottom): {short:?}");
}

// ---- Focus searches laying items out beyond a list's bounds (D477) -------------------------

/// A 100 px list of `count` clickable 20 px rows, then a clickable box after it.
fn focusable_rows(count: usize) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(200, 300), move || {
        Column!({
            LazyColumn!(
                modifier = Modifier.size((100.dp(), 100.dp())),
                move |scope: LazyListScope| {
                    scope.items(count, |index| {
                        Box!(
                            modifier = Modifier
                                .size((100.dp(), 20.dp()))
                                .clickable(|| {})
                                .test_tag(format!("row {index}"))
                        );
                    });
                }
            );
            Box!(modifier = Modifier.size((100.dp(), 20.dp())).clickable(|| {}).test_tag("after"));
        });
    })
}

fn top(ui: &mut ComposeTestRule, tag: &str) -> f32 {
    ui.on_node_with_tag(tag).fetch_semantics_snapshot().bounds.top
}

/// The rows placed now (in the semantics tree).
fn rows_placed(ui: &mut ComposeTestRule) -> usize {
    (0..1000).filter(|index| ui.count_nodes(&has_test_tag(&format!("row {index}"))) == 1).count()
}

/// Tab from the last row in view asks the list for the next row, laid out beyond its bounds
/// for the search, as Compose's `BeyondBoundsLayout`: it takes the focus and is scrolled in.
/// Once the search is over, nothing beyond the rows in view stays placed.
#[test]
fn tab_lays_out_the_next_row_beyond_the_list_and_leaves_nothing_placed_after() {
    let mut ui = focusable_rows(1000);
    assert_eq!(rows_placed(&mut ui), 5);
    for _ in 0..7 {
        ui.press_key(compose_ui::Key::Tab);
    }
    ui.on_node_with_tag("row 6").assert_is_focused();
    assert_eq!(top(&mut ui, "row 6"), 80.0);
    // Rows 2 to 6 in view, none beyond.
    assert_eq!(rows_placed(&mut ui), 5);
}

/// Down from the last row in view (the two-dimensional search) does the same.
#[test]
fn moving_the_focus_down_lays_out_the_next_row() {
    let mut ui = focusable_rows(1000);
    ui.on_node_with_tag("row 4").request_focus();
    ui.wait_for_idle();
    ui.host().with_tree(|tree| tree.move_focus_in(compose_ui::FocusDirection::Down));
    ui.wait_for_idle();
    ui.on_node_with_tag("row 5").assert_is_focused();
    assert_eq!(top(&mut ui, "row 5"), 80.0);
}

/// At the list's end there's nothing more beyond: Tab leaves it, at once.
#[test]
fn tab_from_a_lists_last_row_leaves_it() {
    let mut ui = focusable_rows(5);
    ui.on_node_with_tag("row 4").request_focus();
    ui.wait_for_idle();
    ui.press_key(compose_ui::Key::Tab);
    ui.on_node_with_tag("after").assert_is_focused();
}

/// Shift+Tab from the first row in view of a scrolled list lays out the row before it.
#[test]
fn shift_tab_lays_out_the_row_before_the_list() {
    let state = LazyListState::new(10, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(1000, |index| {
                    Box!(
                        modifier = Modifier
                            .size((100.dp(), 20.dp()))
                            .clickable(|| {})
                            .test_tag(format!("row {index}"))
                    );
                });
            }
        );
    });
    ui.on_node_with_tag("row 10").request_focus();
    ui.wait_for_idle();
    ui.host().send_key(compose_ui::KeyEvent {
        modifiers: compose_ui::KeyModifiers { shift: true, ..Default::default() },
        ..compose_ui::KeyEvent::down(compose_ui::Key::Tab)
    });
    ui.wait_for_idle();
    ui.on_node_with_tag("row 9").assert_is_focused();
    assert_eq!(top(&mut ui, "row 9"), 0.0);
}

/// A grid's focus search past its lines in view and the two beyond them lays out the next
/// lines too.
#[test]
fn tab_moves_through_a_grid_past_the_lines_laid_out() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        LazyVerticalGrid!(
            GridCells::Fixed(2),
            modifier = Modifier.size((100.dp(), 100.dp())),
            |scope: LazyGridScope| {
                scope.items(1000, |index| {
                    Box!(
                        modifier = Modifier
                            .height(20.dp())
                            .clickable(|| {})
                            .test_tag(format!("tile {index}"))
                    );
                });
            }
        );
    });
    // 5 lines in view, 2 beyond: tiles 0 to 13; the 16th Tab focuses tile 15.
    for _ in 0..16 {
        ui.press_key(compose_ui::Key::Tab);
    }
    ui.on_node_with_tag("tile 15").assert_is_focused();
}

// ---- Scroll semantics (D478) -------------------------------------------------------------------

/// The semantics node of the list tagged `tag` (its scroll's, outermost).
fn scroll_node(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::platform::SemanticsSnapshot {
    ui.on_node_with_tag(tag).fetch_semantics_snapshot()
}

/// A lazy list says how far it's scrolled and can be, and scrolls by `ScrollBy`, as Compose's
/// lazy layouts' semantics: what accessibility services scroll a page at a time with, past the
/// items in view (the only ones placed).
#[test]
fn a_lazy_list_has_scroll_semantics_and_scrolls_by_its_action() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())).test_tag("list"),
            state = given,
            |scope: LazyListScope| {
                scope.items(50, Row);
            }
        );
    });
    let range = scroll_node(&mut ui, "list")
        .config
        .get(&compose_ui::SemanticsProperties::VERTICAL_SCROLL_AXIS_RANGE)
        .copied()
        .expect("a range");
    assert_eq!(range.value, 0.0);
    // 50 rows of 20 px in 100 px: 900 px more (estimated from the average).
    assert_eq!(range.max_value, 900.0);
    assert!(!range.reverse_scrolling);
    // A page down.
    let node = scroll_node(&mut ui, "list").id;
    assert!(ui.host().perform_semantics_action_with(
        node,
        &compose_ui::SemanticsActions::SCROLL_BY,
        |scroll| scroll(0.0, 100.0)
    ));
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 5);
    let range = scroll_node(&mut ui, "list")
        .config
        .get(&compose_ui::SemanticsProperties::VERTICAL_SCROLL_AXIS_RANGE)
        .copied()
        .unwrap();
    assert_eq!((range.value, range.max_value), (100.0, 900.0));
    // At the start, nothing to scroll back by: handled, as Compose's (its action always is),
    // and nothing moves (D864).
    state.request_scroll_to_item(0, 0);
    ui.wait_for_idle();
    assert!(ui.host().perform_semantics_action_with(
        node,
        &compose_ui::SemanticsActions::SCROLL_BY,
        |scroll| scroll(0.0, -100.0)
    ));
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 0);
}

/// A scrolling column too.
#[test]
fn a_scrolling_column_has_scroll_semantics_and_scrolls_by_its_action() {
    let scroll = ScrollState::new(0);
    let given = *scroll;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        Column!(
            modifier =
                Modifier.size((100.dp(), 100.dp())).vertical_scroll(given).test_tag("column"),
            {
                for index in 0..10 {
                    Row(index);
                }
            }
        );
    });
    let range = scroll_node(&mut ui, "column")
        .config
        .get(&compose_ui::SemanticsProperties::VERTICAL_SCROLL_AXIS_RANGE)
        .copied()
        .unwrap();
    assert_eq!((range.value, range.max_value), (0.0, 100.0));
    let node = scroll_node(&mut ui, "column").id;
    assert!(ui.host().perform_semantics_action_with(
        node,
        &compose_ui::SemanticsActions::SCROLL_BY,
        |scroll| scroll(0.0, 60.0)
    ));
    ui.wait_for_idle();
    assert_eq!(scroll.value(), 60);
    // Fractions of a pixel add up with the node's own, as its drags' do.
    for _ in 0..2 {
        ui.host().perform_semantics_action_with(
            node,
            &compose_ui::SemanticsActions::SCROLL_BY,
            |scroll| scroll(0.0, 0.4),
        );
    }
    ui.wait_for_idle();
    assert_eq!(scroll.value(), 61, "0.4 and 0.4 make a pixel");
}

/// `ScrollBy` is towards the content's end, as Compose's lazy layouts' (D491): a reversed list
/// (its first item at the bottom) scrolls on through its items with a positive amount, as seen
/// going up (Android's accessibility turns its directions into amounts, `scroll_amount`).
#[test]
fn scroll_by_goes_towards_the_end_of_a_reversed_list() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())).test_tag("list"),
            state = given,
            reverse_layout = true,
            |scope: LazyListScope| {
                scope.items(50, Row);
            }
        );
    });
    let node = scroll_node(&mut ui, "list").id;
    let range = scroll_node(&mut ui, "list")
        .config
        .get(&compose_ui::SemanticsProperties::VERTICAL_SCROLL_AXIS_RANGE)
        .copied()
        .unwrap();
    assert!(range.reverse_scrolling);
    // Back towards the first item: already there (handled, nothing moves, D864).
    assert!(ui.host().perform_semantics_action_with(
        node,
        &compose_ui::SemanticsActions::SCROLL_BY,
        |scroll| scroll(0.0, -100.0)
    ));
    ui.wait_for_idle();
    assert_eq!(given.first_visible_item_index(), 0);
    assert!(ui.host().perform_semantics_action_with(
        node,
        &compose_ui::SemanticsActions::SCROLL_BY,
        |scroll| scroll(0.0, 100.0)
    ));
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 5);
}

/// The frame that lays out the row a Tab waits for also scrolls it in (D485): one frame.
#[test]
fn the_row_a_tab_finds_beyond_the_list_shows_in_the_same_frame() {
    let mut ui = focusable_rows(1000);
    ui.on_node_with_tag("row 4").request_focus();
    ui.wait_for_idle();
    ui.host().send_key(compose_ui::KeyEvent::down(compose_ui::Key::Tab));
    ui.host().frame();
    ui.on_node_with_tag("row 5").assert_is_focused();
    assert_eq!(top(&mut ui, "row 5"), 80.0);
}

/// A scroll past an estimated end leaves the list able to scroll on until laid out: its room
/// that way is still an estimate, not spent (D479).
#[test]
fn a_scroll_past_the_estimated_room_leaves_the_list_scrollable_until_laid_out() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(50, Row);
            }
        );
    });
    // 900 px estimated on; 2,000 asked.
    assert!(state.dispatch_raw_delta(2000.0) != 0.0);
    assert!(state.can_scroll_forward());
    ui.wait_for_idle();
    assert!(!state.can_scroll_forward(), "laid out at its end");
    assert_eq!(state.first_visible_item_index(), 45);
}

/// Every focus search ends within a bounded number of frames, whatever the tree (D486): Tab
/// from the last focusable row of a list whose 1,000 rows after it take no focus gives up
/// after `MOST_BEYOND_STEPS` items laid out, moves on, and the host idles.
#[test]
fn a_tab_past_nothing_focusable_ends_and_the_host_idles() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 300), || {
        Column!({
            LazyColumn!(modifier = Modifier.size((100.dp(), 100.dp())), |scope: LazyListScope| {
                scope.items(1005, |index| {
                    let modifier =
                        Modifier.size((100.dp(), 20.dp())).test_tag(format!("row {index}"));
                    let modifier = if index < 5 { modifier.clickable(|| {}) } else { modifier };
                    Box!(modifier = modifier);
                });
            });
            Box!(modifier = Modifier.size((100.dp(), 20.dp())).clickable(|| {}).test_tag("after"));
        });
    });
    ui.on_node_with_tag("row 4").request_focus();
    ui.wait_for_idle();
    ui.host().send_key(compose_ui::KeyEvent::down(compose_ui::Key::Tab));
    let mut frames = 0;
    while ui.host().has_pending_work() && frames < 100 {
        ui.host().frame();
        frames += 1;
    }
    assert!(frames < 40, "{frames} frames");
    assert!(!ui.host().with_tree(|tree| tree.has_beyond_bounds_moves()));
    ui.on_node_with_tag("after").assert_is_focused();
}

thread_local! {
    static ZERO_BODIES: Cell<usize> = const { Cell::new(0) };
}

/// A list of items with no height composes a few thousand at most in a frame (`MOST_IN_A_ROUND`
/// a round, D492), not the tens of thousands its doubling allows over 16 rounds.
#[test]
fn a_list_of_empty_items_composes_a_few_thousand_at_most_in_a_frame() {
    let mut host = compose_ui::platform::UiHost::headless(100, 500);
    host.set_content(|| {
        LazyColumn!(modifier = Modifier.size((100.dp(), 500.dp())), |scope: LazyListScope| {
            scope.items(1_000_000, |_| {
                ZERO_BODIES.with(|cell| cell.set(cell.get() + 1));
                Box!(modifier = Modifier.size((100.dp(), 0.dp())));
            });
        });
    });
    ZERO_BODIES.with(|cell| cell.set(0));
    host.frame();
    let composed = ZERO_BODIES.with(Cell::get);
    assert!(composed <= 6000, "{composed} composed in a frame");
}

/// `layout_info` read where the list is called: it goes idle, and hears of keys that do change.
#[test]
fn reading_layout_info_where_the_list_is_called_goes_idle() {
    let reads = Rc::new(Cell::new(0));
    let counted = reads.clone();
    let owned = LazyListState::new(0, 0);
    let state = *owned;
    let keys = mutable_state_of(vec![1, 2, 3]);
    let read = keys.handle();
    let seen = Rc::new(Cell::new(0u64));
    let noted = seen.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let info = state.layout_info();
        noted.set(info.visible_items_info.first().map_or(0, |item| item.key));
        counted.set(counted.get() + 1);
        let ids = read.get();
        LazyColumn!(
            modifier = Modifier.size(300.dp()),
            state = state,
            move |scope: LazyListScope| {
                scope.items_list_keyed(
                    ids.clone(),
                    |id: &i32| *id,
                    |_, _: &i32| {
                        Spacer(Modifier.height(50.dp()));
                    },
                );
            }
        );
    });
    ui.wait_for_idle();
    assert!(reads.get() < 8, "{} compositions", reads.get());
    assert_eq!(seen.get(), hash_key(&1));
    keys.set(vec![7, 2, 3]);
    ui.wait_for_idle();
    assert_eq!(seen.get(), hash_key(&7));
}
