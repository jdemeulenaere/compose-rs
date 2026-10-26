//! The rest of Compose's lazy list API: lists of items (`items_list`, `items_indexed`), the
//! item scope's `fill_parent_max_*`, `reverse_layout`, `user_scroll_enabled`, sticky headers,
//! and the check of duplicate keys.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, Offset, PointerEvent, UnitExt};

fn top_of(ui: &mut ComposeTestRule, tag: &str) -> f32 {
    ui.on_node_with_tag(tag).fetch_semantics_node().bounds.top
}

#[test]
fn items_over_a_list_get_each_item_and_its_index() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        let fruits: ImmutableList<&'static str> = ImmutableList::from(["apple", "pear"]);
        LazyColumn!(modifier = Modifier.size((100.dp(), 100.dp())), |scope: LazyListScope| {
            scope.items_list(fruits.clone(), |_, fruit: &&str| {
                BasicText!("{}", fruit);
            });
            scope.items_indexed(vec![10, 20], |_, index, value: &i32| {
                BasicText!("{} at {}", value, index);
            });
            scope.items_list(&["slice"][..], |_, text: &&str| {
                BasicText!("{}", text);
            });
        });
    });
    for text in ["apple", "pear", "10 at 0", "20 at 1", "slice"] {
        ui.on_node_with_text(text).assert_exists();
    }
}

#[test]
fn keyed_list_items_keep_their_state_when_items_before_them_go() {
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
    let items = mutable_state_of(vec![String::from("a"), String::from("b")]);
    let handle = items.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        let items = handle.get();
        LazyColumn!(modifier = Modifier.size((100.dp(), 100.dp())), |scope: LazyListScope| {
            scope.items_list_keyed(
                items.clone(),
                |item: &String| item.clone(),
                |_, item: &String| Counter(item.clone()),
            );
        });
    });
    ui.on_node_with_tag("b").perform_click();
    items.edit(|items| {
        items.remove(0);
    });
    ui.wait_for_idle();
    ui.on_node_with_text("b 1").assert_exists();
    assert_eq!(top_of(&mut ui, "b"), 0.0);
}

#[test]
fn a_lazy_column_aligns_its_items_with_a_horizontal_alignment() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            horizontal_alignment = Alignment::CENTER_HORIZONTALLY,
            |scope: LazyListScope| {
                scope.item(|| Box!(modifier = Modifier.size(40.dp()).test_tag("item")));
            }
        );
        LazyRow!(
            modifier = Modifier.size((100.dp(), 100.dp())).offset(0.dp(), 100.dp()),
            vertical_alignment = Alignment::BOTTOM,
            |scope: LazyListScope| {
                scope.item(|| Box!(modifier = Modifier.size(40.dp()).test_tag("cell")));
            }
        );
    });
    assert_eq!(ui.on_node_with_tag("item").fetch_semantics_node().bounds.left, 30.0);
    assert_eq!(top_of(&mut ui, "cell"), 160.0);
}

#[test]
fn fill_parent_max_size_takes_the_viewport_less_the_padding() {
    let height = mutable_state_of(100.0f32);
    let handle = height.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), handle.get().dp())),
            content_padding = PaddingValues::all(10.dp()),
            |scope: LazyListScope| {
                let item_scope = scope.item_scope();
                scope.item(move || {
                    Box!(
                        modifier = Modifier
                            .fill_parent_max_height(&item_scope)
                            .width(10.dp())
                            .test_tag("page")
                    );
                });
                scope.items_list(vec![()], |item_scope, _| {
                    Box!(
                        modifier = Modifier
                            .fill_parent_max_width_with(&item_scope, 0.5)
                            .height(10.dp())
                            .test_tag("half")
                    );
                });
            }
        );
    });
    let page = ui.on_node_with_tag("page").fetch_semantics_node().bounds;
    assert_eq!(page.height(), 80.0);
    assert_eq!(ui.on_node_with_tag("half").fetch_semantics_node().bounds.width(), 40.0);
    // The viewport grows: the item follows in the same frame.
    height.set(200.0);
    ui.host().frame();
    let page = ui.on_node_with_tag("page").fetch_semantics_node().bounds;
    assert_eq!(page.height(), 180.0);
}

#[composable]
fn Numbers(reverse: bool, arrangement: VerticalArrangement, count: usize, state: LazyListState) {
    LazyColumn!(
        modifier = Modifier.size((100.dp(), 100.dp())),
        state = state,
        reverse_layout = reverse,
        vertical_arrangement = arrangement,
        |scope: LazyListScope| {
            scope.items(count, |index| {
                Box!(modifier = Modifier.size((100.dp(), 30.dp())).test_tag(format!("{index}")));
            });
        }
    );
}

#[test]
fn a_reversed_list_starts_at_the_bottom_and_scrolls_up() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        Numbers(true, Arrangement::TOP, 10, given)
    });
    assert_eq!(top_of(&mut ui, "0"), 70.0);
    assert_eq!(top_of(&mut ui, "1"), 40.0);
    assert_eq!(top_of(&mut ui, "2"), 10.0);
    // The wheel down goes back towards the first item: nothing to scroll.
    ui.host().send_pointer(PointerEvent::scroll(Offset::new(50.0, 50.0), Offset::new(0.0, 45.0)));
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 0);
    // Up: towards the last items, which come in from the top.
    ui.host().send_pointer(PointerEvent::scroll(Offset::new(50.0, 50.0), Offset::new(0.0, -45.0)));
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 1);
    assert_eq!(state.first_visible_item_scroll_offset(), 15);
    assert_eq!(top_of(&mut ui, "1"), 85.0);
    // A drag down pulls the items down with the finger: further on, the later items come in
    // from the top (30 px, the first 8 within the slop).
    let host = ui.host();
    host.send_pointer(PointerEvent::press(Offset::new(50.0, 10.0)));
    for y in [20.0, 30.0, 40.0] {
        host.send_pointer(PointerEvent::moved(Offset::new(50.0, y), true));
    }
    host.send_pointer(PointerEvent::release(Offset::new(50.0, 40.0)));
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 2);
    assert_eq!(state.first_visible_item_scroll_offset(), 7);
}

#[test]
fn a_short_reversed_list_is_arranged_from_the_top_in_reverse_order() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        Numbers(true, Arrangement::TOP, 2, remember(|| LazyListState::new(0, 0)))
    });
    // As Compose's: (top) 1 0 … (bottom).
    assert_eq!(top_of(&mut ui, "1"), 0.0);
    assert_eq!(top_of(&mut ui, "0"), 30.0);
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        Numbers(true, Arrangement::BOTTOM, 2, remember(|| LazyListState::new(0, 0)))
    });
    assert_eq!(top_of(&mut ui, "0"), 70.0);
    assert_eq!(top_of(&mut ui, "1"), 40.0);
}

#[test]
fn a_reversed_list_brings_a_focused_item_into_view() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            reverse_layout = true,
            |scope: LazyListScope| {
                scope.items(10, |index| {
                    Box!(
                        modifier = Modifier
                            .size((100.dp(), 30.dp()))
                            .focusable()
                            .test_tag(format!("{index}"))
                    );
                });
            }
        );
    });
    // Item 3 is partly out of view (above): focusing it scrolls it in at the top.
    assert_eq!(top_of(&mut ui, "3"), -20.0);
    ui.on_node_with_tag("3").request_focus();
    ui.wait_for_idle();
    assert_eq!(top_of(&mut ui, "3"), 0.0);
}

#[test]
fn a_list_with_user_scrolling_disabled_ignores_the_wheel_but_not_its_state() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            user_scroll_enabled = false,
            |scope: LazyListScope| {
                scope.items(10, |index| {
                    Box!(
                        modifier = Modifier.size((100.dp(), 30.dp())).test_tag(format!("{index}"))
                    );
                });
            }
        );
    });
    ui.host().send_pointer(PointerEvent::scroll(Offset::new(50.0, 50.0), Offset::new(0.0, 45.0)));
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 0);
    state.scroll_to_item(3, 0);
    ui.wait_for_idle();
    assert_eq!(top_of(&mut ui, "3"), 0.0);
}

#[composable]
fn Sections(state: LazyListState) {
    LazyColumn!(
        modifier = Modifier.size((100.dp(), 100.dp())),
        state = state,
        |scope: LazyListScope| {
            for section in ["A", "B"] {
                scope.sticky_header(move |_| {
                    Box!(
                        modifier = Modifier
                            .size((100.dp(), 20.dp()))
                            .background(Color::RED)
                            .test_tag(section)
                    );
                });
                scope.items(10, move |index| {
                    Box!(
                        modifier = Modifier
                            .size((100.dp(), 30.dp()))
                            .background(Color::GRAY)
                            .test_tag(format!("{section}{index}"))
                    );
                });
            }
        }
    );
}

#[test]
fn a_sticky_header_stays_at_the_top_until_the_next_one_pushes_it_out() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || Sections(given));
    assert_eq!(top_of(&mut ui, "A"), 0.0);
    // Scrolled into section A's items: its header stays at the top, over them.
    state.scroll_to_item(5, 10);
    ui.wait_for_idle();
    assert_eq!(top_of(&mut ui, "A"), 0.0);
    assert_eq!(top_of(&mut ui, "A4"), -10.0);
    assert!(ui.dump_tree().contains("A4"), "{}", ui.dump_tree());
    // B's header 10 px from the top pushes A's up.
    state.scroll_to_item(10, 20);
    ui.wait_for_idle();
    assert_eq!(top_of(&mut ui, "B"), 10.0);
    assert_eq!(top_of(&mut ui, "A"), -10.0);
    // Far into B: B's header at the top, A's gone.
    state.scroll_to_item(18, 5);
    ui.wait_for_idle();
    assert_eq!(top_of(&mut ui, "B"), 0.0);
    ui.on_node_with_tag("A").assert_does_not_exist();
}

#[test]
fn a_sticky_header_far_behind_is_composed_and_drawn_over_the_items() {
    let state = LazyListState::new(9, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || Sections(given));
    assert_eq!(top_of(&mut ui, "A"), 0.0);
    // Placed last: it draws over the items.
    let display = ui.dump_display_list();
    let header = display.rfind("#FFFF0000").expect("the header");
    let item = display.rfind("#FF888888").expect("an item");
    assert!(header > item, "{display}");
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "have the same key")]
fn duplicate_keys_panic_in_debug_builds() {
    let _ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        LazyColumn!(modifier = Modifier.size((100.dp(), 100.dp())), |scope: LazyListScope| {
            scope.items_keyed(
                3,
                |index| index.min(1),
                |_| {
                    Box!(modifier = Modifier.size(10.dp()));
                },
            );
        });
    });
}

/// 20 rows 20 px tall in a list 100 px tall whose content is padded 30 px above and below.
#[composable]
fn Padded(state: LazyListState, reverse: bool) {
    LazyColumn!(
        modifier = Modifier.size((100.dp(), 100.dp())),
        state = state,
        content_padding = PaddingValues::symmetric(0.dp(), 30.dp()),
        reverse_layout = reverse,
        |scope: LazyListScope| {
            scope.items(20, |index| {
                Box!(modifier = Modifier.size((100.dp(), 20.dp())).test_tag(format!("{index}")));
            });
        }
    );
}

fn position(state: &LazyListState) -> (usize, i32) {
    (state.first_visible_item_index(), state.first_visible_item_scroll_offset())
}

/// As Compose's `contentPadding`: the viewport is the list's whole size, the padding comes
/// before the first item and after the last, and items scrolled into it stay laid out and shown
/// there (`LazyListMeasure`'s `beforeContentPadding` and `afterContentPadding`).
#[test]
fn items_are_laid_out_under_the_content_padding() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || Padded(given, false));
    // The first item after the padding; the viewport takes items under the end padding.
    assert_eq!(top_of(&mut ui, "0"), 30.0);
    assert_eq!(top_of(&mut ui, "3"), 90.0);
    ui.on_node_with_tag("4").assert_does_not_exist();
    // Scrolled: the item scrolled into the start padding stays, above the first visible one.
    state.scroll_to_item(2, 10);
    ui.wait_for_idle();
    assert_eq!(top_of(&mut ui, "1"), 0.0);
    assert_eq!(top_of(&mut ui, "2"), 20.0);
    assert_eq!(top_of(&mut ui, "5"), 80.0);
    ui.on_node_with_tag("0").assert_does_not_exist();
    ui.on_node_with_tag("6").assert_does_not_exist();
    assert_eq!(position(&state), (2, 10));
    // At the end: the last item ends where the end padding starts, the items before it fill the
    // start padding, and the first visible one is the first not wholly in it.
    state.scroll_to_item(19, 0);
    ui.wait_for_idle();
    assert_eq!(top_of(&mut ui, "19"), 50.0);
    assert_eq!(top_of(&mut ui, "17"), 10.0);
    assert_eq!(top_of(&mut ui, "16"), -10.0);
    ui.on_node_with_tag("15").assert_does_not_exist();
    assert_eq!(position(&state), (18, 0));
    // Scrolled back: the items before come into the start padding.
    assert!(state.scroll_by(-45.0));
    ui.wait_for_idle();
    assert_eq!(top_of(&mut ui, "15"), 15.0);
    assert_eq!(top_of(&mut ui, "14"), -5.0);
    ui.on_node_with_tag("13").assert_does_not_exist();
    assert_eq!(position(&state), (15, 15));
}

/// Reversed, the padding at the bottom comes first and takes the items scrolled into it.
#[test]
fn a_reversed_list_lays_out_items_under_its_bottom_padding() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || Padded(given, true));
    assert_eq!(top_of(&mut ui, "0"), 50.0);
    assert_eq!(top_of(&mut ui, "3"), -10.0);
    state.scroll_to_item(2, 10);
    ui.wait_for_idle();
    assert_eq!(top_of(&mut ui, "2"), 60.0);
    assert_eq!(top_of(&mut ui, "1"), 80.0);
    ui.on_node_with_tag("0").assert_does_not_exist();
}

/// The sticky header is that of the first item laid out, under the start padding, as Compose's
/// (`applyStickyItems` from the first of `visibleItems`), at the top of the list.
#[test]
fn the_sticky_header_is_that_of_the_item_under_the_start_padding() {
    let state = LazyListState::new(11, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            content_padding = PaddingValues::new(0.dp(), 30.dp(), 0.dp(), 0.dp()),
            |scope: LazyListScope| {
                for section in ["A", "B"] {
                    scope.sticky_header(move |_| {
                        Box!(modifier = Modifier.size((100.dp(), 20.dp())).test_tag(section));
                    });
                    scope.items(10, move |index| {
                        Box!(
                            modifier = Modifier
                                .size((100.dp(), 30.dp()))
                                .test_tag(format!("{section}{index}"))
                        );
                    });
                }
            }
        );
    });
    // B's header is the first item after the padding, A's last item under it: A's header
    // sticks at the top, over it.
    assert_eq!(top_of(&mut ui, "B"), 30.0);
    assert_eq!(top_of(&mut ui, "A9"), 0.0);
    assert_eq!(top_of(&mut ui, "A"), 0.0);
}
