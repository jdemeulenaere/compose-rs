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
