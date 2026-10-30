//! Nested lists' items prefetched with their parent's: scenarios of Compose's
//! `LazyListNestedPrefetchingTest` and `LazyGridNestedPrefetchingTest`, rewritten. Compose's tests
//! run its one-item strategy, through a scheduler of their own.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Constraints, Dp, IntSize, Modifier, prefetch_now};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
enum Action {
    Compose(usize, Option<usize>),
    Measure(usize, Option<usize>),
}

use Action::{Compose, Measure};

const ITEM: f32 = 30.0;

#[derive(Default)]
struct Track {
    actions: RefCell<Option<Vec<Action>>>,
    active: RefCell<HashSet<(usize, Option<usize>)>>,
}

impl Track {
    fn note(&self, action: Action) {
        if let Some(actions) = self.actions.borrow_mut().as_mut() {
            actions.push(action);
        }
    }
    fn while_doing(
        &self,
        ui: &mut ComposeTestRule,
        block: impl FnOnce(&mut ComposeTestRule),
    ) -> Vec<Action> {
        *self.actions.borrow_mut() = Some(Vec::new());
        block(ui);
        self.actions.borrow_mut().take().expect("set above")
    }
}

#[composable]
fn Tracked(track: Rc<Track>, index: usize, nested: Option<usize>) {
    DisposableEffect(index * 1000 + nested.map_or(0, |nested| nested + 1), move |scope| {
        track.active.borrow_mut().insert((index, nested));
        track.note(Compose(index, nested));
        let gone = track.clone();
        scope.on_dispose(move || {
            gone.active.borrow_mut().remove(&(index, nested));
        })
    });
}

fn measured(track: Rc<Track>, index: usize, nested: Option<usize>) -> Modifier {
    Modifier.layout(move |scope, measurable, constraints: Constraints| {
        track.note(Measure(index, nested));
        let placeable = scope.measure(measurable, constraints);
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    })
}

/// What's along and across for `vertical`.
fn sized(vertical: bool, along: f32, across: f32) -> Modifier {
    let (along, across) = (Dp(along), Dp(across));
    if vertical { Modifier.size((across, along)) } else { Modifier.size((along, across)) }
}

fn nested_list(
    vertical: bool,
    state: LazyListState,
    first_nested: usize,
    track: Rc<Track>,
) -> ComposeTestRule {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let track = track.clone();
        let content = move |scope: LazyListScope| {
            let track = track.clone();
            scope.items(100, move |index| {
                let track = track.clone();
                Tracked(track.clone(), index, None);
                let nested = remember(move || LazyListState::new(first_nested, 0));
                let nested_handle = nested;
                let modifier =
                    sized(vertical, ITEM, ITEM * 1.5).then(measured(track.clone(), index, None));
                let cells = move |scope: LazyListScope| {
                    let track = track.clone();
                    scope.items(100, move |cell| {
                        Tracked(track.clone(), index, Some(cell));
                        Spacer(sized(!vertical, ITEM, ITEM).then(measured(
                            track.clone(),
                            index,
                            Some(cell),
                        )));
                    });
                };
                if vertical {
                    LazyRow!(modifier = modifier, state = nested_handle, content = cells);
                } else {
                    LazyColumn!(modifier = modifier, state = nested_handle, content = cells);
                }
            });
        };
        let modifier = sized(vertical, ITEM * 1.5, 100.0);
        if vertical {
            LazyColumn!(modifier = modifier, state = state, content = content);
        } else {
            LazyRow!(modifier = modifier, state = state, content = content);
        }
    });
    ui.wait_for_idle();
    ui
}

/// As Compose's test: one column of 60 px grids in 75 px, each of two cells a line of 30 px.
fn nested_grid(
    vertical: bool,
    state: LazyGridState,
    first_nested: usize,
    track: Rc<Track>,
) -> ComposeTestRule {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let track = track.clone();
        let content = move |scope: LazyGridScope| {
            let track = track.clone();
            scope.items(100, move |index| {
                let track = track.clone();
                Tracked(track.clone(), index, None);
                let nested = remember(move || LazyGridState::new(first_nested, 0));
                let nested_handle = nested;
                let modifier = sized(vertical, ITEM * 2.0, ITEM * 2.0).then(measured(
                    track.clone(),
                    index,
                    None,
                ));
                let cells = move |scope: LazyGridScope| {
                    let track = track.clone();
                    scope.items(100, move |cell| {
                        Tracked(track.clone(), index, Some(cell));
                        Spacer(sized(vertical, ITEM, ITEM).then(measured(
                            track.clone(),
                            index,
                            Some(cell),
                        )));
                    });
                };
                let two = GridCells::Fixed(2);
                if vertical {
                    LazyVerticalGrid!(
                        two,
                        modifier = modifier,
                        state = nested_handle,
                        content = cells
                    );
                } else {
                    LazyHorizontalGrid!(
                        two,
                        modifier = modifier,
                        state = nested_handle,
                        content = cells
                    );
                }
            });
        };
        let modifier = sized(vertical, ITEM * 2.5, ITEM * 2.0);
        let one = GridCells::Fixed(1);
        if vertical {
            LazyVerticalGrid!(one, modifier = modifier, state = state, content = content);
        } else {
            LazyHorizontalGrid!(one, modifier = modifier, state = state, content = content);
        }
    });
    ui.wait_for_idle();
    ui
}

fn prefetch(ui: &mut ComposeTestRule) {
    while ui.host().prefetch(prefetch_now() + std::time::Duration::from_secs(60)) {
        ui.wait_for_idle();
    }
}

fn scrolled_list(state: LazyListState, ui: &mut ComposeTestRule, by: f32) {
    let handle = state;
    crate::scroll_support::run(ui, async move {
        let _ = handle.scroll_by(by).await;
    });
}

fn scrolled_grid(state: LazyGridState, ui: &mut ComposeTestRule, by: f32) {
    let handle = state;
    crate::scroll_support::run(ui, async move {
        let _ = handle.scroll_by(by).await;
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListNestedPrefetchingTest.kt, nestedPrefetchingForwardAfterSmallScroll
#[test]
fn a_nested_lists_first_items_compose_before_it_measures() {
    let _clock = compose_ui::PrefetchClock::hold();
    for vertical in [true, false] {
        let track: Rc<Track> = Rc::default();
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let mut ui = nested_list(vertical, handle, 0, track.clone());
        prefetch(&mut ui);
        let actions = track.while_doing(&mut ui, |ui| {
            scrolled_list(handle, ui, 5.0);
            prefetch(ui);
        });
        assert_eq!(
            actions,
            [
                Compose(2, None),
                Compose(2, Some(0)),
                Compose(2, Some(1)),
                Measure(2, None),
                Measure(2, Some(0)),
                Measure(2, Some(1)),
            ],
            "vertical {vertical}"
        );
        let active = track.active.borrow();
        assert!(active.contains(&(2, None)) && active.contains(&(2, Some(1))));
        assert!(!active.contains(&(2, Some(2))));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListNestedPrefetchingTest.kt, cancelingPrefetchCancelsItsNestedPrefetches
#[test]
fn a_prefetched_item_gone_takes_its_nested_items_with_it() {
    let _clock = compose_ui::PrefetchClock::hold();
    for vertical in [true, false] {
        let track: Rc<Track> = Rc::default();
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let mut ui = nested_list(vertical, handle, 0, track.clone());
        scrolled_list(handle, &mut ui, ITEM);
        prefetch(&mut ui);
        assert!(track.active.borrow().contains(&(3, Some(1))));
        scrolled_list(handle, &mut ui, ITEM * 3.0);
        scrolled_list(handle, &mut ui, ITEM);
        prefetch(&mut ui);
        scrolled_list(handle, &mut ui, ITEM * 20.0);
        // The window lets go of what the long scroll passed at the scrolls after it.
        scrolled_list(handle, &mut ui, ITEM);
        scrolled_list(handle, &mut ui, ITEM);
        let active = track.active.borrow();
        assert!(!active.contains(&(3, None)));
        assert!(!active.contains(&(3, Some(0))));
        assert!(!active.contains(&(3, Some(1))));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListNestedPrefetchingTest.kt, nestedPrefetchStartsFromFirstVisibleItemIndex
#[test]
fn a_nested_prefetch_starts_from_its_first_visible_item() {
    let _clock = compose_ui::PrefetchClock::hold();
    for vertical in [true, false] {
        let track: Rc<Track> = Rc::default();
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let mut ui = nested_list(vertical, handle, 5, track.clone());
        prefetch(&mut ui);
        let actions = track.while_doing(&mut ui, |ui| {
            scrolled_list(handle, ui, 5.0);
            prefetch(ui);
        });
        assert_eq!(
            actions,
            [
                Compose(2, None),
                Compose(2, Some(5)),
                Compose(2, Some(6)),
                Measure(2, None),
                Measure(2, Some(5)),
                Measure(2, Some(6)),
            ],
            "vertical {vertical}"
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridNestedPrefetchingTest.kt, nestedPrefetchingForwardAfterSmallScroll
#[test]
fn a_nested_grids_first_items_compose_before_it_measures() {
    let _clock = compose_ui::PrefetchClock::hold();
    for vertical in [true, false] {
        let track: Rc<Track> = Rc::default();
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let mut ui = nested_grid(vertical, handle, 0, track.clone());
        prefetch(&mut ui);
        let actions = track.while_doing(&mut ui, |ui| {
            // Compose's tests scroll 5 px, under their one-item strategy: the window asks at 40.
            scrolled_grid(handle, ui, 40.0);
            prefetch(ui);
        });
        // Then the rest of those in view: Compose's composes them as its measurement asks; here a
        // measurement short of items is followed by their composition, and measures again.
        assert_eq!(
            actions[..6],
            [
                Compose(2, None),
                Compose(2, Some(0)),
                Compose(2, Some(1)),
                Measure(2, None),
                Measure(2, Some(0)),
                Measure(2, Some(1)),
            ],
            "vertical {vertical}"
        );
        for action in
            [Compose(2, Some(2)), Measure(2, Some(2)), Compose(2, Some(3)), Measure(2, Some(3))]
        {
            assert!(actions[6..].contains(&action), "{action:?} in {actions:?}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridNestedPrefetchingTest.kt, cancelingPrefetchCancelsItsNestedPrefetches
#[test]
fn a_prefetched_grid_item_gone_takes_its_nested_items_with_it() {
    let _clock = compose_ui::PrefetchClock::hold();
    for vertical in [true, false] {
        let track: Rc<Track> = Rc::default();
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let mut ui = nested_grid(vertical, handle, 0, track.clone());
        // Items 1 and 2 in view, item 3 within the window.
        scrolled_grid(handle, &mut ui, 100.0);
        prefetch(&mut ui);
        assert!(track.active.borrow().contains(&(3, Some(1))));
        scrolled_grid(handle, &mut ui, ITEM * 3.0);
        scrolled_grid(handle, &mut ui, ITEM);
        prefetch(&mut ui);
        scrolled_grid(handle, &mut ui, ITEM * 40.0);
        // The window lets go of what the long scroll passed at the scrolls after it.
        scrolled_grid(handle, &mut ui, ITEM * 2.0);
        scrolled_grid(handle, &mut ui, ITEM * 2.0);
        scrolled_grid(handle, &mut ui, ITEM * 2.0);
        let active = track.active.borrow();
        assert!(!active.contains(&(3, None)));
        assert!(!active.contains(&(3, Some(1))));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridNestedPrefetchingTest.kt, nestedPrefetchStartsFromFirstVisibleItemIndex
#[test]
fn a_nested_grid_prefetch_starts_from_its_first_visible_item() {
    let _clock = compose_ui::PrefetchClock::hold();
    for vertical in [true, false] {
        let track: Rc<Track> = Rc::default();
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let mut ui = nested_grid(vertical, handle, 4, track.clone());
        prefetch(&mut ui);
        let actions = track.while_doing(&mut ui, |ui| {
            // Compose's tests scroll 5 px, under their one-item strategy: the window asks at 40.
            scrolled_grid(handle, ui, 40.0);
            prefetch(ui);
        });
        // Then the rest of those in view: Compose's composes them as its measurement asks; here a
        // measurement short of items is followed by their composition, and measures again.
        assert_eq!(
            actions[..6],
            [
                Compose(2, None),
                Compose(2, Some(4)),
                Compose(2, Some(5)),
                Measure(2, None),
                Measure(2, Some(4)),
                Measure(2, Some(5)),
            ],
            "vertical {vertical}"
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListNestedPrefetchingTest.kt, automaticNestedPrefetchingBasedOnNumberOfVisibleItems
#[test]
fn a_nested_list_prefetches_as_many_as_the_last_one_showed() {
    let _clock = compose_ui::PrefetchClock::hold();
    let track: Rc<Track> = Rc::default();
    let state = LazyListState::new(0, 0);
    let handle = *state;
    let mut ui = nested_list(true, handle, 4, track.clone());
    prefetch(&mut ui);
    let actions = track.while_doing(&mut ui, |ui| {
        scrolled_list(handle, ui, 5.0);
        prefetch(ui);
    });
    // Two, then the rest of those in view: here two are all a nested list of these shows.
    let first = [
        Compose(2, None),
        Compose(2, Some(4)),
        Compose(2, Some(5)),
        Measure(2, None),
        Measure(2, Some(4)),
        Measure(2, Some(5)),
    ];
    assert_eq!(actions, first);
    crate::scroll_support::run(&mut ui, async move {
        let _ = handle.scroll_to_item(10, 0).await;
    });
    prefetch(&mut ui);
    let actions = track.while_doing(&mut ui, |ui| {
        scrolled_list(handle, ui, 5.0);
        prefetch(ui);
    });
    assert_eq!(
        actions,
        [
            Compose(12, None),
            Compose(12, Some(4)),
            Compose(12, Some(5)),
            Measure(12, None),
            Measure(12, Some(4)),
            Measure(12, Some(5)),
        ]
    );
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridNestedPrefetchingTest.kt, automaticNestedPrefetchingBasedOnNumberOfVisibleItems
#[test]
fn a_nested_grid_prefetches_as_many_as_the_last_one_showed() {
    let _clock = compose_ui::PrefetchClock::hold();
    let track: Rc<Track> = Rc::default();
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let mut ui = nested_grid(true, handle, 4, track.clone());
    prefetch(&mut ui);
    let actions = track.while_doing(&mut ui, |ui| {
        scrolled_grid(handle, ui, 40.0);
        prefetch(ui);
    });
    // Two, then the rest of those in view, in a pass after the measurement short of them, where
    // Compose's measurement composes them.
    assert_eq!(
        actions,
        [
            Compose(2, None),
            Compose(2, Some(4)),
            Compose(2, Some(5)),
            Measure(2, None),
            Measure(2, Some(4)),
            Measure(2, Some(5)),
            Compose(2, Some(6)),
            Compose(2, Some(7)),
            Measure(2, None),
            Measure(2, Some(6)),
            Measure(2, Some(7)),
        ]
    );
    crate::scroll_support::run(&mut ui, async move {
        let _ = handle.scroll_to_item(10, 0).await;
    });
    prefetch(&mut ui);
    let actions = track.while_doing(&mut ui, |ui| {
        scrolled_grid(handle, ui, 40.0);
        prefetch(ui);
    });
    // Then as many as that one showed, at once.
    assert_eq!(
        actions,
        [
            Compose(12, None),
            Compose(12, Some(4)),
            Compose(12, Some(5)),
            Compose(12, Some(6)),
            Compose(12, Some(7)),
            Measure(12, None),
            Measure(12, Some(4)),
            Measure(12, Some(5)),
            Measure(12, Some(6)),
            Measure(12, Some(7)),
        ]
    );
}

/// A column of pagers, as `composeListOfPagers`: 45 px of 30 px rows, each a pager of 30 px pages in
/// 45 across.
fn nested_pagers(state: LazyListState, first_nested: usize, track: Rc<Track>) -> ComposeTestRule {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let track = track.clone();
        let content = move |scope: LazyListScope| {
            let track = track.clone();
            scope.items(100, move |index| {
                let track = track.clone();
                Tracked(track.clone(), index, None);
                let owned = remember(move || PagerState::new(first_nested, 0.0, || 100));
                let modifier =
                    sized(true, ITEM, ITEM * 1.5).then(measured(track.clone(), index, None));
                let page = move |_: PagerScope, page: usize| {
                    Tracked(track.clone(), index, Some(page));
                    Spacer(sized(false, ITEM, ITEM).then(measured(
                        track.clone(),
                        index,
                        Some(page),
                    )));
                };
                HorizontalPager!(
                    owned,
                    modifier = modifier,
                    page_size = PageSize::Fixed(Dp(ITEM)),
                    page_content = page
                );
            });
        };
        LazyColumn!(modifier = sized(true, ITEM * 1.5, 100.0), state = state, content = content);
    });
    ui.wait_for_idle();
    ui
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerNestedPrefetchingTest.kt, nestedPrefetchingForwardAfterSmallScroll, nestedPrefetchStartsFromFirstVisiblePageIndex
#[test]
fn a_nested_pagers_pages_are_prefetched_from_its_first_visible_one() {
    let _clock = compose_ui::PrefetchClock::hold();
    for first in [0, 4] {
        let track: Rc<Track> = Rc::default();
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let mut ui = nested_pagers(handle, first, track.clone());
        prefetch(&mut ui);
        let actions = track.while_doing(&mut ui, |ui| {
            scrolled_list(handle, ui, 5.0);
            prefetch(ui);
        });
        assert_eq!(
            actions[..6],
            [
                Compose(2, None),
                Compose(2, Some(first)),
                Compose(2, Some(first + 1)),
                Measure(2, None),
                Measure(2, Some(first)),
                Measure(2, Some(first + 1)),
            ],
            "{actions:?}"
        );
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerNestedPrefetchingTest.kt, cancelingPrefetchCancelsItsNestedPrefetches
#[test]
fn a_cancelled_prefetch_of_a_pager_lets_go_of_its_pages() {
    let _clock = compose_ui::PrefetchClock::hold();
    let track: Rc<Track> = Rc::default();
    let state = LazyListState::new(0, 0);
    let handle = *state;
    let mut ui = nested_pagers(handle, 0, track.clone());
    scrolled_list(handle, &mut ui, ITEM * 3.0);
    prefetch(&mut ui);
    assert!(track.active.borrow().contains(&(5, Some(0))), "{:?}", track.active);
    scrolled_list(handle, &mut ui, ITEM * 3.0);
    scrolled_list(handle, &mut ui, ITEM);
    prefetch(&mut ui);
    scrolled_list(handle, &mut ui, ITEM * 20.0);
    // The window lets go of what the long scroll passed at the scrolls after it.
    scrolled_list(handle, &mut ui, ITEM);
    scrolled_list(handle, &mut ui, ITEM);
    let active = track.active.borrow();
    assert!(!active.contains(&(5, None)), "{active:?}");
    assert!(!active.contains(&(5, Some(0))), "{active:?}");
}
