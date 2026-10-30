//! Sticky headers, as Compose's `LazyListHeadersTest`: shown, sticking on scroll, replaced by
//! the next, in the content padding, two in a row, empty. Columns and rows; density 1, so
//! Compose's dps are pixels.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_test::*;
use compose_ui::platform::Optimizations;
use compose_ui::{IntSize, UnitExt};

use crate::lazy_support::{Lazy, LazyColumnOrRow, no_padding};
use crate::scroll_support::run;

const LIST: &str = "LazyList";
const FIRST_HEADER: &str = "firstHeaderTag";
const SECOND_HEADER: &str = "secondHeaderTag";

fn orientations() -> [Lazy; 2] {
    [Lazy::column(), Lazy::row()]
}

fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    compose_test::use_optimizations(Optimizations::NONE);
    ComposeTestRule::sized(IntSize::new(1000, 1000), content)
}

/// A spacer `main` long along the axis, filling the list across, tagged `tag` (none if empty).
fn spacer(lazy: Lazy, scope: &LazyItemScope, main: f32, tag: &str) {
    let modifier = lazy.main_axis_size(main.dp()).then(lazy.fill_parent_max_cross_axis(scope));
    Spacer(if tag.is_empty() { modifier } else { modifier.test_tag(tag.to_string()) });
}

/// The order of Compose's tests' content: a header, "1" and "2", a second header (or the two
/// headers first), each 101 px, in a list of 300.
fn headers_and_two_items(lazy: Lazy, state: LazyListState, headers_first: bool) -> ComposeTestRule {
    rule(move || {
        LazyColumnOrRow(
            lazy,
            lazy.main_axis_size(300.dp()).test_tag(LIST),
            state,
            no_padding(),
            move |scope: LazyListScope| {
                scope.sticky_header(move |item| spacer(lazy, &item, 101.0, FIRST_HEADER));
                if headers_first {
                    scope.sticky_header(move |item| spacer(lazy, &item, 101.0, SECOND_HEADER));
                }
                scope.items_list(vec!["1", "2"], move |item, tag: &&str| {
                    spacer(lazy, &item, 101.0, tag)
                });
                if !headers_first {
                    scope.sticky_header(move |item| spacer(lazy, &item, 101.0, SECOND_HEADER));
                }
            },
        );
    })
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListHeadersTest.kt, lazyColumnShowsHeader_withoutBeyondBoundsItemCount
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListHeadersTest.kt, lazyRowShowsHeader_withoutOffscreenItens
#[test]
fn a_header_shows_and_one_out_of_view_is_not_composed() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let mut ui = headers_and_two_items(lazy, *state, false);
        for tag in [FIRST_HEADER, "1", "2"] {
            assert!(ui.on_node_with_tag(tag).is_displayed(), "{lazy:?}: {tag}");
        }
        ui.on_node_with_tag(SECOND_HEADER).assert_does_not_exist();
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListHeadersTest.kt, lazyColumnShowsHeadersOnScroll
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListHeadersTest.kt, lazyRowShowsHeadersOnScroll
#[test]
fn a_scrolled_header_sticks_at_the_start() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let mut ui = headers_and_two_items(lazy, *state, false);
        lazy.drag_main_axis_by(&mut ui, LIST, 102.0);
        assert!(ui.on_node_with_tag(FIRST_HEADER).is_displayed(), "{lazy:?}");
        assert_eq!(lazy.main_start(&mut ui, FIRST_HEADER), 0.0, "{lazy:?}");
        let first = state.layout_info().visible_items_info[0];
        assert_eq!((first.index, first.offset), (0, 0), "{lazy:?}");
        assert!(
            ui.on_node_with_tag("2").is_displayed()
                && ui.on_node_with_tag(SECOND_HEADER).is_displayed(),
            "{lazy:?}"
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListHeadersTest.kt, lazyColumnHeaderIsReplaced
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListHeadersTest.kt, lazyRowHeaderIsReplaced
#[test]
fn the_next_header_replaces_the_sticking_one() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let mut ui = headers_and_two_items(lazy, *state, true);
        lazy.drag_main_axis_by(&mut ui, LIST, 105.0);
        assert!(!ui.on_node_with_tag(FIRST_HEADER).is_displayed(), "{lazy:?}");
        for tag in [SECOND_HEADER, "1", "2"] {
            assert!(ui.on_node_with_tag(tag).is_displayed(), "{lazy:?}: {tag}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListHeadersTest.kt, headerIsDisplayedWhenItIsFullyInContentPadding
#[test]
fn a_header_wholly_in_the_padding_shows() {
    let lazy = Lazy::column();
    let state = LazyListState::new(0, 0);
    let handle = *state;
    let mut ui = rule(move || {
        LazyColumnOrRow(
            lazy,
            Modifier.required_size(400.dp()),
            handle,
            lazy.padding(200.dp(), 0.dp(), 0.dp(), 0.dp()),
            |scope: LazyListScope| {
                scope
                    .sticky_header(|_| Spacer(Modifier.required_size(100.dp()).test_tag("header")));
                scope.items(5, |index| {
                    Spacer(Modifier.required_size(100.dp()).test_tag(index.to_string()))
                });
            },
        );
    });
    let _ = run(&mut ui, async move { handle.scroll_to_item(1, 50).await });
    assert_eq!(lazy.main_start(&mut ui, "header"), 50.0);
    let first = state.layout_info().visible_items_info[0];
    assert_eq!((first.index, first.offset), (0, 50 - 200));
    assert_eq!(lazy.main_start(&mut ui, "0"), 150.0);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListHeadersTest.kt, lazyColumnShowsHeader_withoutBeyondBoundsItemCount2
#[test]
fn two_headers_in_a_row_scroll_with_the_items() {
    let lazy = Lazy::column();
    let state = LazyListState::new(0, 0);
    let handle = *state;
    let mut ui = rule(move || {
        LazyColumnOrRow(
            lazy,
            lazy.main_axis_size(350.dp()),
            handle,
            no_padding(),
            move |scope: LazyListScope| {
                scope.sticky_header(move |item| spacer(lazy, &item, 100.0, FIRST_HEADER));
                scope.sticky_header(move |item| spacer(lazy, &item, 100.0, SECOND_HEADER));
                let item_scope = scope.item_scope();
                scope.items(100, move |index| spacer(lazy, &item_scope, 100.0, &index.to_string()));
            },
        );
    });
    let _ = run(&mut ui, async move { handle.scroll_by(20.0).await });
    assert_eq!(lazy.main_start(&mut ui, FIRST_HEADER), -20.0);
    assert_eq!(lazy.main_start(&mut ui, SECOND_HEADER), 80.0);
    assert_eq!(lazy.main_start(&mut ui, "0"), 180.0);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListHeadersTest.kt, lazyColumn_withEmptyHeader_shouldNotCrash
#[test]
fn an_empty_header_is_fine() {
    let lazy = Lazy::column();
    let mut ui = rule(move || {
        LazyColumnOrRow(
            lazy,
            lazy.main_axis_size(300.dp()),
            remember_lazy_list_state!(),
            no_padding(),
            move |scope: LazyListScope| {
                scope.sticky_header(|_| {});
                scope.items_list(vec!["1", "2"], move |item, tag: &&str| {
                    spacer(lazy, &item, 101.0, tag)
                });
            },
        );
    });
    ui.on_node_with_tag("1").assert_exists();
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListHeadersTest.kt, lazyColumn_withEmptyHeader_showsHeadersOnScroll
#[test]
fn empty_headers_do_not_unstick_the_real_one() {
    let lazy = Lazy::column();
    let state = LazyListState::new(0, 0);
    let handle = *state;
    let mut ui = rule(move || {
        LazyColumnOrRow(
            lazy,
            lazy.main_axis_size(300.dp()).test_tag(LIST),
            handle,
            no_padding(),
            |scope: LazyListScope| {
                scope.sticky_header(|_| {
                    Spacer(Modifier.height(101.dp()).fill_max_width().test_tag("headerTag"))
                });
                for _ in 0..10 {
                    scope.item(|| Spacer(Modifier.height(101.dp()).fill_max_width()));
                    // Not affecting the real header.
                    scope.sticky_header(|_| {});
                }
            },
        );
    });
    lazy.drag_main_axis_by(&mut ui, LIST, 10.0);
    assert!(ui.on_node_with_tag("headerTag").is_displayed());
    assert_eq!(lazy.main_start(&mut ui, "headerTag"), 0.0);
    let first = state.layout_info().visible_items_info[0];
    assert_eq!((first.index, first.offset), (0, 0));
}

/// A column or a row in 35 px, a 5 px header (or two) and 10 px focusable items.
fn focusable_list(
    lazy: crate::lazy_support::Lazy,
    headers: usize,
    padding: f32,
    direction: compose_ui::LayoutDirection,
    state: LazyListState,
    requesters: std::rc::Rc<std::cell::RefCell<Vec<compose_ui::FocusRequester>>>,
) -> ComposeTestRule {
    use compose_runtime::remember;
    use_optimizations(Optimizations::NONE);
    ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let requesters = requesters.clone();
        compose_runtime::CompositionLocalProvider(
            compose_ui::LOCAL_LAYOUT_DIRECTION.provides(direction),
            move || {
                let requesters = requesters.clone();
                crate::lazy_support::LazyColumnOrRow(
                    lazy,
                    lazy.main_axis_size(35.dp()),
                    state,
                    PaddingValues::all(compose_ui::Dp(padding)),
                    move |scope: LazyListScope| {
                        for _ in 0..headers {
                            scope.sticky_header(move |_| {
                                Spacer(
                                    lazy.main_axis_size(5.dp()).then(lazy.cross_axis_size(50.dp())),
                                );
                            });
                        }
                        let requesters = requesters.clone();
                        scope.items(10, move |index| {
                            let requester = remember(compose_ui::FocusRequester::new);
                            let mut all = requesters.borrow_mut();
                            if all.len() <= index {
                                all.resize(index + 1, requester);
                            }
                            all[index] = requester;
                            drop(all);
                            Spacer(
                                lazy.main_axis_size(10.dp())
                                    .then(lazy.cross_axis_size(50.dp()))
                                    .focus_requester(requester)
                                    .focusable(),
                            );
                        });
                    },
                );
            },
        );
    })
}

fn list_offset(state: &LazyListState, item: usize) -> i32 {
    let info = state.layout_info();
    info.visible_items_info.iter().find(|it| it.index == item).expect("in view").offset
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListHeadersTest.kt, lazyList_withHeader_focusScrollsRevealsEntireItemUnderHeaderIgnoringContentPadding: without its pinning, the item focused in view
#[test]
fn a_focused_item_comes_out_from_under_its_header() {
    use crate::lazy_support::Lazy;
    for padding in [0.0, 5.0] {
        let lazy = Lazy::column();
        let state = LazyListState::new(2, 0);
        let requesters: std::rc::Rc<std::cell::RefCell<Vec<compose_ui::FocusRequester>>> =
            Default::default();
        let mut ui = focusable_list(
            lazy,
            1,
            padding,
            compose_ui::LayoutDirection::Ltr,
            *state,
            requesters.clone(),
        );
        ui.wait_for_idle();
        assert!(requesters.borrow()[1].request_focus());
        ui.wait_for_idle();
        assert_eq!(list_offset(&state, 2), 5 - padding as i32, "padding {padding}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListHeadersTest.kt, lazyList_withMultipleHeaders_focusScrollsRevealsEntireItemUnderHeaders: only the one sticking counts
#[test]
fn a_focused_item_comes_out_from_under_the_header_sticking() {
    use crate::lazy_support::Lazy;
    let lazy = Lazy::column();
    let state = LazyListState::new(3, 0);
    let requesters: std::rc::Rc<std::cell::RefCell<Vec<compose_ui::FocusRequester>>> =
        Default::default();
    let mut ui =
        focusable_list(lazy, 2, 0.0, compose_ui::LayoutDirection::Ltr, *state, requesters.clone());
    ui.wait_for_idle();
    assert!(requesters.borrow()[1].request_focus());
    ui.wait_for_idle();
    assert_eq!(list_offset(&state, 3), 5);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListHeadersTest.kt, lazyList_withHeader_layoutOrientations_focusScrollsRevealsEntireItemUnderHeader
#[test]
fn a_focused_item_comes_out_from_under_its_header_in_any_layout() {
    use crate::lazy_support::Lazy;
    for reverse_layout in [true, false] {
        for direction in [compose_ui::LayoutDirection::Ltr, compose_ui::LayoutDirection::Rtl] {
            for lazy in [Lazy::column(), Lazy::row()] {
                let lazy = Lazy { reverse_layout, ..lazy };
                let state = LazyListState::new(2, 0);
                let requesters: std::rc::Rc<std::cell::RefCell<Vec<compose_ui::FocusRequester>>> =
                    Default::default();
                let mut ui = focusable_list(lazy, 1, 0.0, direction, *state, requesters.clone());
                ui.wait_for_idle();
                assert!(requesters.borrow()[1].request_focus());
                ui.wait_for_idle();
                assert_eq!(list_offset(&state, 2), 5, "{lazy:?} {direction:?}");
            }
        }
    }
}
