//! A pager's prefetch and its pages' pins: scenarios of Compose's `PagerCacheWindowTest`,
//! `PagerPrefetcherTest`, `PagerGestureTest` and `PagerPinnableContainerTest`, rewritten.
//! `PagerPrefetcherTest` is skipped at the tip, whose flag turns the window on: its scenarios run
//! on the window here, for what they share.

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Dp, IntSize, Offset, UnitExt, prefetch_now};

use crate::scroll_support::run;

const PAGE: f32 = 30.0;

struct Pages {
    ui: ComposeTestRule,
    state: PagerState,
    composed: Rc<RefCell<BTreeSet<usize>>>,
    vertical: bool,
    _clock: compose_ui::PrefetchClock,
}

fn pages(vertical: bool, beyond: usize, page: usize, fraction: f32) -> Pages {
    pages_of(vertical, beyond, page, fraction, 45)
}

fn pages_of(vertical: bool, beyond: usize, page: usize, fraction: f32, window: i32) -> Pages {
    let clock = compose_ui::PrefetchClock::hold();
    let owned = PagerState::new(page, fraction, || 1000);
    let state = *owned;
    std::mem::forget(owned);
    let composed: Rc<RefCell<BTreeSet<usize>>> = Rc::default();
    let noted = composed.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(window, window), move || {
        let noted = noted.clone();
        let content = move |_: PagerScope, index: usize| {
            let noted = noted.clone();
            DisposableEffect(index, move |scope| {
                noted.borrow_mut().insert(index);
                let gone = noted.clone();
                scope.on_dispose(move || {
                    gone.borrow_mut().remove(&index);
                })
            });
            Box!(modifier = Modifier.fill_max_size());
        };
        if vertical {
            VerticalPager!(
                state,
                modifier = Modifier.fill_max_size(),
                page_size = PageSize::Fixed(Dp(PAGE)),
                beyond_viewport_page_count = beyond,
                page_content = content
            );
        } else {
            HorizontalPager!(
                state,
                modifier = Modifier.fill_max_size(),
                page_size = PageSize::Fixed(Dp(PAGE)),
                beyond_viewport_page_count = beyond,
                page_content = content
            );
        }
    });
    ui.wait_for_idle();
    Pages { ui, state, composed, vertical, _clock: clock }
}

impl Pages {
    fn prefetch(&mut self) {
        while self.ui.host().prefetch(prefetch_now() + std::time::Duration::from_secs(60)) {
            self.ui.wait_for_idle();
        }
    }

    fn scroll(&mut self, by: f32) {
        let state = self.state;
        run(&mut self.ui, async move {
            state.scroll_by(by).await.expect("free");
        });
    }

    fn has(&self, page: usize) -> bool {
        self.composed.borrow().contains(&page)
    }
}

fn each(check: impl Fn(bool, usize)) {
    for vertical in [false, true] {
        for beyond in [0, 1] {
            check(vertical, beyond);
        }
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerCacheWindowTest.kt, doNotPrefetchingForwardInitially, notPrefetchingBackwardInitially
#[test]
fn nothing_is_prefetched_before_a_scroll() {
    each(|vertical, beyond| {
        let mut pager = pages(vertical, beyond, 0, 0.0);
        pager.prefetch();
        assert!(!pager.has(2 + beyond), "{vertical} {beyond}");
        let mut pager = pages(vertical, beyond, 2 + beyond, 0.0);
        pager.prefetch();
        assert!(!pager.has(0), "{vertical} {beyond}");
    });
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerCacheWindowTest.kt, scrollForward_shouldFillEntireWindow
#[test]
fn a_scroll_forward_fills_the_window_past_the_pages_beyond() {
    each(|vertical, beyond| {
        let mut pager = pages(vertical, beyond, 0, 0.0);
        pager.scroll(PAGE);
        pager.prefetch();
        assert!(pager.has(2));
        assert!(pager.has(3), "{vertical} {beyond}");
        if beyond == 1 {
            assert!(pager.has(4));
        }
        assert!(!pager.has(4 + beyond), "{vertical} {beyond}: {:?}", pager.composed);
    });
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerCacheWindowTest.kt, smallScrollBackwardShouldFillEntireWindow
#[test]
fn a_small_scroll_back_fills_the_window_behind() {
    each(|vertical, beyond| {
        let mut pager = pages(vertical, beyond, 4, 0.0);
        pager.scroll(-5.0);
        pager.prefetch();
        let first = 3 - beyond;
        assert!(pager.has(first));
        assert!(pager.has(first - 1), "{vertical} {beyond}: {:?}", pager.composed);
        assert!(!pager.has(first - 2), "{vertical} {beyond}: {:?}", pager.composed);
    });
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerCacheWindowTest.kt, scrollForward_shouldNotDisposeItemsInWindow
#[test]
fn a_scroll_forward_keeps_what_the_window_holds() {
    each(|vertical, beyond| {
        let mut pager = pages(vertical, beyond, 3, 0.0);
        pager.scroll(PAGE * 2.5);
        pager.prefetch();
        for page in [5, 6, 7] {
            assert!(pager.has(page), "{page} {:?}", pager.composed);
        }
        assert!(!pager.has(2), "{:?}", pager.composed);
        assert!(!pager.has(9 + beyond), "{:?}", pager.composed);
    });
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerCacheWindowTest.kt, scrollBackward_shouldDisposeItemsInWindow
#[test]
fn a_scroll_back_lets_go_of_what_the_window_doesnt_hold() {
    each(|vertical, beyond| {
        let mut pager = pages(vertical, beyond, 6, -0.5);
        pager.scroll(-PAGE * 2.5);
        pager.state.layout_info();
        pager.ui.wait_for_idle();
        pager.scroll(-0.5);
        pager.prefetch();
        assert!(pager.has(2), "{:?}", pager.composed);
        assert!(pager.has(3) && pager.has(4));
        assert!(!pager.has(6 + beyond), "{:?}", pager.composed);
    });
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerPrefetcherTest.kt, notPrefetchingForwardInitially, notPrefetchingBackwardInitially, prefetchingForwardAfterSmallScroll_programmatically, prefetchingBackwardAfterSmallScroll_programmatically
#[test]
fn a_small_scroll_prefetches_the_way_it_goes() {
    each(|vertical, beyond| {
        // Pages of 30 in 45: 0 and 1 in view, so 2 next.
        let mut pager = pages(vertical, beyond, 0, 0.0);
        assert!(!pager.has(2 + beyond));
        pager.scroll(5.0);
        pager.prefetch();
        assert!(pager.has(2 + beyond), "{:?}", pager.composed);
        assert!(!pager.has(3 + beyond + 1), "{:?}", pager.composed);
        let mut pager = pages(vertical, beyond, 5, 10.0 / PAGE);
        pager.scroll(-5.0);
        pager.prefetch();
        assert!(pager.has(4 - beyond), "{:?}", pager.composed);
        assert!(!pager.has(4 - beyond - 2), "{:?}", pager.composed);
    });
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerPrefetcherTest.kt, prefetchingForwardAfterSmallScroll_withGesture, prefetchingBackwardAfterSmallScroll_withGesture
#[test]
fn a_small_drag_prefetches_the_way_it_goes() {
    let drag = |pager: &mut Pages, by: f32| {
        let vertical = pager.vertical;
        pager.ui.on_root().perform_touch_input(|touch| {
            touch.down(Offset::new(22.0, 22.0));
            let moved = if vertical { Offset::new(0.0, by) } else { Offset::new(by, 0.0) };
            touch.move_by(moved);
        });
        pager.ui.wait_for_idle();
        pager.prefetch();
        assert!(pager.state.is_scroll_in_progress());
    };
    for vertical in [false, true] {
        let mut pager = pages(vertical, 0, 0, 0.0);
        drag(&mut pager, -13.0);
        assert!(pager.has(2), "{:?}", pager.composed);
        let mut pager = pages(vertical, 0, 5, 10.0 / PAGE);
        drag(&mut pager, 13.0);
        assert!(pager.has(4), "{:?}", pager.composed);
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerPrefetcherTest.kt, prefetchingForwardTwice, prefetchingBackwardTwice
#[test]
fn scrolling_on_prefetches_on() {
    let mut pager = pages(false, 0, 0, 0.0);
    pager.scroll(5.0);
    pager.prefetch();
    pager.scroll(PAGE / 2.0);
    pager.scroll(PAGE / 2.0);
    pager.prefetch();
    assert!(pager.has(3), "{:?}", pager.composed);
    let mut pager = pages(false, 0, 5, 0.0);
    pager.scroll(-5.0);
    pager.prefetch();
    pager.scroll(-PAGE / 2.0);
    pager.scroll(-PAGE / 2.0);
    pager.prefetch();
    assert!(pager.has(2) || pager.has(3), "{:?}", pager.composed);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerPrefetcherTest.kt, snappingToOtherPositionWhilePrefetchIsScheduled
#[test]
fn a_jump_while_a_prefetch_is_scheduled_doesnt_compose_it() {
    let mut pager = pages(false, 0, 0, 0.0);
    pager.scroll(PAGE);
    let state = pager.state;
    run(&mut pager.ui, async move {
        state.scroll_to_page(100, 0.0).await.expect("free");
    });
    pager.prefetch();
    assert!(!pager.has(3), "{:?}", pager.composed);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerGestureTest.kt, swipeWithLowVelocity_bouncesBack_shouldNotRunThePrefetch
#[test]
fn a_page_bouncing_back_prefetches_nothing_behind() {
    let mut pager = pages_of(false, 0, 5, 0.0, 30);
    pager.ui.on_root().perform_touch_input(|touch| {
        touch.down(Offset::new(25.0, 15.0));
        // Under half the page in all, slowly.
        touch.move_by(Offset::new(-14.0, 0.0));
        touch.advance_event_time(3000);
    });
    pager.ui.wait_for_idle();
    pager.ui.on_root().perform_touch_input(|touch| touch.up());
    pager.ui.wait_for_idle();
    pager.prefetch();
    assert_eq!(pager.state.current_page(), 5, "bounced back");
    assert!(!pager.has(4), "{:?}", pager.composed);
}

/// The pinnable container of a page, kept when it's composed.
#[derive(Clone, Default)]
struct Kept(Rc<RefCell<Option<Rc<dyn compose_ui::PinnableContainer>>>>);

impl Kept {
    fn pin(&self) -> compose_ui::PinnedHandle {
        self.0.borrow().as_ref().expect("composed").pin()
    }
}

struct Pinned {
    ui: ComposeTestRule,
    state: MutableState<PagerState>,
    composed: Rc<RefCell<BTreeSet<usize>>>,
    kept: Kept,
    pinned_page: MutableState<usize>,
    states: [PagerState; 2],
}

fn pinned_pages(count: usize) -> Pinned {
    let (first, second) =
        (PagerState::new(0, 0.0, move || count), PagerState::new(0, 0.0, move || count));
    let states = [*first, *second];
    std::mem::forget(first);
    std::mem::forget(second);
    let which = mutable_state_of(states[0]);
    let pinned = mutable_state_of(1usize);
    let (state, page) = (which.handle(), pinned.handle());
    std::mem::forget(which);
    std::mem::forget(pinned);
    let composed: Rc<RefCell<BTreeSet<usize>>> = Rc::default();
    let kept = Kept::default();
    let (noted, given) = (composed.clone(), kept.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        let (noted, given) = (noted.clone(), given.clone());
        HorizontalPager!(
            state.get(),
            modifier = Modifier.size(20.dp()),
            page_size = PageSize::Fixed(Dp(10.0)),
            page_content = move |_, index: usize| {
                if index == page.get() {
                    *given.0.borrow_mut() = compose_ui::LOCAL_PINNABLE_CONTAINER.current();
                }
                let noted = noted.clone();
                DisposableEffect(index, move |scope| {
                    noted.borrow_mut().insert(index);
                    let gone = noted.clone();
                    scope.on_dispose(move || {
                        gone.borrow_mut().remove(&index);
                    })
                });
                Box!(modifier = Modifier.size(10.dp()).test_tag(index.to_string()));
            }
        );
    });
    ui.wait_for_idle();
    Pinned { ui, state, composed, kept, pinned_page: page, states }
}

impl Pinned {
    fn jump(&mut self, page: usize) {
        let state = self.state.get();
        run(&mut self.ui, async move {
            state.scroll_to_page(page, 0.0).await.expect("free");
        });
        self.ui.wait_for_idle();
    }

    fn has(&self, page: usize) -> bool {
        self.composed.borrow().contains(&page)
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerPinnableContainerTest.kt, pinnedPageIsComposedAndPlacedWhenScrolledOut, pagesBetweenPinnedAndCurrentVisibleAreNotComposed
#[test]
fn a_pinned_page_stays_composed_when_scrolled_out_and_those_between_dont() {
    let mut pager = pinned_pages(100);
    let _pin = pager.kept.pin();
    pager.jump(5);
    assert!(pager.has(1));
    assert!(!pager.has(0) && !pager.has(2) && !pager.has(3), "{:?}", pager.composed);
    pager.ui.on_node_with_tag("1").assert_exists();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerPinnableContainerTest.kt, pinnedPageAfterVisibleOnesIsComposedAndPlacedWhenScrolledOut
#[test]
fn a_pinned_page_stays_composed_when_scrolled_out_the_other_way() {
    let mut pager = pinned_pages(100);
    pager.pinned_page.set(4);
    pager.jump(4);
    let _pin = pager.kept.pin();
    pager.jump(0);
    assert!(pager.has(4));
    assert!(!pager.has(3), "{:?}", pager.composed);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerPinnableContainerTest.kt, pinnedPageCanBeUnpinned, pinnedPageIsRemovedWhenNotVisible, pinnedPageIsRemovedWhenVisible
#[test]
fn an_unpinned_page_goes_when_its_out_of_view_and_stays_in_it() {
    let mut pager = pinned_pages(100);
    let pin = pager.kept.pin();
    pager.jump(5);
    assert!(pager.has(1));
    pin.release();
    pager.ui.wait_for_idle();
    assert!(!pager.has(1), "{:?}", pager.composed);
    let mut pager = pinned_pages(100);
    let pin = pager.kept.pin();
    pin.release();
    pager.ui.wait_for_idle();
    assert!(pager.has(1), "still in view");
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerPinnableContainerTest.kt, pinnedMultipleTimes
#[test]
fn a_page_pinned_twice_goes_when_both_let_go() {
    let mut pager = pinned_pages(100);
    let first = pager.kept.pin();
    let second = pager.kept.pin();
    pager.jump(5);
    first.release();
    pager.ui.wait_for_idle();
    assert!(pager.has(1));
    second.release();
    pager.ui.wait_for_idle();
    assert!(!pager.has(1));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerPinnableContainerTest.kt, pagesArePinnedBasedOnGlobalIndexes
#[test]
fn pages_are_pinned_by_their_index() {
    let mut pager = pinned_pages(100);
    pager.jump(3);
    pager.pinned_page.set(4);
    pager.ui.wait_for_idle();
    let _pin = pager.kept.pin();
    pager.jump(10);
    assert!(pager.has(4));
    assert!(!pager.has(1), "{:?}", pager.composed);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerPinnableContainerTest.kt, unpinnedWhenPagerStateChanges, pinAfterPagerStateChange
#[test]
fn a_new_state_lets_go_of_the_pinned_page_and_pins_in_the_new_one() {
    let mut pager = pinned_pages(100);
    let _pin = pager.kept.pin();
    pager.jump(5);
    assert!(pager.has(1));
    pager.state.set(pager.states[1]);
    pager.ui.wait_for_idle();
    let state = pager.state.get();
    assert_eq!(state.current_page(), 0);
    assert!(pager.has(0) && pager.has(1));
    pager.jump(5);
    assert!(!pager.has(1), "not pinned in the new state: {:?}", pager.composed);
    // A pin after the change is in it.
    pager.pinned_page.set(6);
    pager.ui.wait_for_idle();
    let _pin = pager.kept.pin();
    pager.jump(20);
    assert!(pager.has(6), "{:?}", pager.composed);
}

/// A container noting its pins.
#[derive(Default)]
struct Parent(Rc<Cell<i32>>);

impl compose_ui::PinnableContainer for Parent {
    fn pin(&self) -> compose_ui::PinnedHandle {
        self.0.set(self.0.get() + 1);
        let count = self.0.clone();
        compose_ui::PinnedHandle::new(move || count.set(count.get() - 1))
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerPinnableContainerTest.kt, pinningIsPropagatedToParentContainer, parentContainerChange_pinningIsMaintained
#[test]
fn pinning_goes_on_to_the_container_around_and_a_new_one_takes_over() {
    let parents = [Rc::new(Parent::default()), Rc::new(Parent::default())];
    let which = mutable_state_of(0usize);
    let read = which.handle();
    let given = parents.clone();
    let owned = PagerState::new(0, 0.0, || 100);
    let state = *owned;
    std::mem::forget(owned);
    let kept = Kept::default();
    let keep = kept.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        let parent: Rc<dyn compose_ui::PinnableContainer> = given[read.get()].clone();
        let keep = keep.clone();
        CompositionLocalProvider(
            compose_ui::LOCAL_PINNABLE_CONTAINER.provides(Some(parent)),
            move || {
                let keep = keep.clone();
                HorizontalPager!(
                    state,
                    modifier = Modifier.size(20.dp()),
                    page_content = move |_, index: usize| {
                        if index == 0 {
                            *keep.0.borrow_mut() = compose_ui::LOCAL_PINNABLE_CONTAINER.current();
                        }
                        Box!(modifier = Modifier.size(10.dp()));
                    }
                );
            },
        );
    });
    ui.wait_for_idle();
    let pin = kept.pin();
    assert_eq!(parents[0].0.get(), 1);
    which.set(1);
    ui.wait_for_idle();
    assert_eq!((parents[0].0.get(), parents[1].0.get()), (0, 1));
    pin.release();
    assert_eq!(parents[1].0.get(), 0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerPinnableContainerTest.kt, pinnedPageIsStillPinnedWhenReorderedAndNotVisibleAnymore
#[test]
fn a_pinned_page_follows_its_key_when_reordered() {
    let list = mutable_state_of((0..20).collect::<Vec<u64>>());
    let read = list.handle();
    let owned = PagerState::new(0, 0.0, || 20);
    let state = *owned;
    std::mem::forget(owned);
    let composed: Rc<RefCell<BTreeSet<u64>>> = Rc::default();
    let kept = Kept::default();
    let (noted, keep) = (composed.clone(), kept.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        let (noted, keep) = (noted.clone(), keep.clone());
        let items = read.get();
        let keys = items.clone();
        HorizontalPager!(
            state,
            modifier = Modifier.size(20.dp()),
            page_size = PageSize::Fixed(Dp(10.0)),
            key = Some(pager_key(move |page| keys[page])),
            page_content = move |_, index: usize| {
                let item = items[index];
                if item == 1 {
                    *keep.0.borrow_mut() = compose_ui::LOCAL_PINNABLE_CONTAINER.current();
                }
                let noted = noted.clone();
                DisposableEffect(item, move |scope| {
                    noted.borrow_mut().insert(item);
                    let gone = noted.clone();
                    scope.on_dispose(move || {
                        gone.borrow_mut().remove(&item);
                    })
                });
                Box!(modifier = Modifier.size(10.dp()));
            }
        );
    });
    ui.wait_for_idle();
    let _pin = kept.pin();
    let moved: Vec<u64> = std::iter::once(0).chain(2..15).chain([1]).chain(15..20).collect();
    list.set(moved);
    ui.wait_for_idle();
    run(&mut ui, async move {
        state.scroll_to_page(5, 0.0).await.expect("free");
    });
    assert!(composed.borrow().contains(&1), "{composed:?}");
}
