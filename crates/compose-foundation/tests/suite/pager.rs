//! `HorizontalPager` and `VerticalPager`: scenarios of Compose's `pager` tests, rewritten, on a
//! 400 by 400 pager at density 1.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::gestures::{Orientation, SnapPosition, TargetedFlingBehavior};
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Dp, IntSize, LOCAL_LAYOUT_DIRECTION, LayoutDirection, Offset, UnitExt};

use crate::scroll_support::{launch, run};

const COUNT: usize = 20;
const SIZE: f32 = 400.0;
const MIN_FLING: f32 = 400.0;

#[derive(Clone)]
struct Setup {
    vertical: bool,
    page: usize,
    fraction: f32,
    count: usize,
    page_size: PageSize,
    spacing: f32,
    padding: f32,
    beyond: usize,
    reverse: bool,
    rtl: bool,
    user_scroll: bool,
    snap_position: SnapPosition,
    window: i32,
}

impl Default for Setup {
    fn default() -> Setup {
        Setup {
            vertical: false,
            page: 0,
            fraction: 0.0,
            count: COUNT,
            page_size: PageSize::Fill,
            spacing: 0.0,
            padding: 0.0,
            beyond: 0,
            reverse: false,
            rtl: false,
            user_scroll: true,
            snap_position: SnapPosition::Start,
            window: 400,
        }
    }
}

struct Pagers {
    ui: ComposeTestRule,
    state: PagerState,
    composed: Rc<RefCell<Vec<usize>>>,
    setup: Setup,
}

impl Setup {
    fn rule(self) -> Pagers {
        let count = self.count;
        let owned = PagerState::new(self.page, self.fraction, move || count);
        let state = *owned;
        std::mem::forget(owned);
        let composed: Rc<RefCell<Vec<usize>>> = Rc::default();
        let (noted, setup) = (composed.clone(), self.clone());
        let mut ui = ComposeTestRule::sized(IntSize::new(self.window, self.window), move || {
            let (noted, setup) = (noted.clone(), setup.clone());
            let direction = if setup.rtl { LayoutDirection::Rtl } else { LayoutDirection::Ltr };
            CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), move || {
                let page = {
                    let noted = noted.clone();
                    move |_: PagerScope, index: usize| {
                        let noted = noted.clone();
                        DisposableEffect(index, move |scope| {
                            noted.borrow_mut().push(index);
                            let gone = noted.clone();
                            scope.on_dispose(move || gone.borrow_mut().retain(|it| *it != index))
                        });
                        Box!(modifier = Modifier.fill_max_size().test_tag(index.to_string()));
                    }
                };
                let padding = PaddingValues::all(Dp(setup.padding));
                let modifier = Modifier.fill_max_size().test_tag("pager");
                if setup.vertical {
                    VerticalPager!(
                        state,
                        modifier = modifier,
                        content_padding = padding,
                        page_size = setup.page_size.clone(),
                        beyond_viewport_page_count = setup.beyond,
                        page_spacing = Dp(setup.spacing),
                        user_scroll_enabled = setup.user_scroll,
                        reverse_layout = setup.reverse,
                        snap_position = setup.snap_position.clone(),
                        page_content = page
                    );
                } else {
                    HorizontalPager!(
                        state,
                        modifier = modifier,
                        content_padding = padding,
                        page_size = setup.page_size.clone(),
                        beyond_viewport_page_count = setup.beyond,
                        page_spacing = Dp(setup.spacing),
                        user_scroll_enabled = setup.user_scroll,
                        reverse_layout = setup.reverse,
                        snap_position = setup.snap_position.clone(),
                        page_content = page
                    );
                }
            });
        });
        ui.wait_for_idle();
        Pagers { ui, state, composed, setup: self }
    }
}

impl Pagers {
    fn at(&self, main: f32) -> Offset {
        if self.setup.vertical { Offset::new(200.0, main) } else { Offset::new(main, 200.0) }
    }

    /// A swipe by `delta` along the axis, ending at `velocity`, as
    /// `swipeWithVelocityAcrossMainAxis`.
    fn swipe(&mut self, delta: f32, velocity: f32) {
        let (from, to) = if delta >= 0.0 { (10.0, 10.0 + delta) } else { (390.0, 390.0 + delta) };
        let (from, to) = (self.at(from), self.at(to));
        self.ui.on_root().perform_touch_input(|touch| {
            touch.swipe_with_velocity(from, to, velocity, None);
        });
        self.ui.wait_for_idle();
    }

    fn page(&self) -> usize {
        self.state.current_page()
    }

    fn fraction(&self) -> f32 {
        self.state.current_page_offset_fraction()
    }

    fn assert_at(&self, page: usize) {
        assert_eq!((self.page(), self.fraction()), (page, 0.0));
    }

    fn pages(&self) -> Vec<(usize, i32)> {
        self.state.layout_info().visible_pages_info.iter().map(|it| (it.index, it.offset)).collect()
    }

    fn scroll_to(&mut self, page: usize, fraction: f32) {
        let state = self.state;
        run(&mut self.ui, async move {
            state.scroll_to_page(page, fraction).await.expect("free");
        });
    }

    fn animate_to(&mut self, page: usize, fraction: f32) {
        let state = self.state;
        run(&mut self.ui, async move {
            state.animate_scroll_to_page(page, fraction).await.expect("free");
        });
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateTest.kt, scrollToPage_shouldPlacePagesCorrectly
#[test]
fn a_jump_places_the_page() {
    for vertical in [false, true] {
        let mut pager = Setup { vertical, ..Setup::default() }.rule();
        for page in 1..3 {
            pager.scroll_to(page, 0.0);
            pager.assert_at(page);
            assert_eq!(pager.pages(), [(page, 0)]);
        }
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateTest.kt, scrollToPage_usedOffset_shouldPlacePagesCorrectly
#[test]
fn a_jump_with_a_fraction_places_it_that_far_off() {
    let mut pager = Setup::default().rule();
    pager.scroll_to(5, 0.2);
    assert_eq!(pager.page(), 5);
    assert!((pager.fraction() - 0.2).abs() < 0.01);
    assert_eq!(pager.pages(), [(5, -80), (6, 320)]);
    pager.scroll_to(5, -0.2);
    assert_eq!(pager.page(), 5);
    assert!((pager.fraction() + 0.2).abs() < 0.01);
    assert_eq!(pager.pages(), [(4, -320), (5, 80)]);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateTest.kt, scrollToPage_longSkipShouldNotPlaceIntermediatePages
#[test]
fn a_long_jump_composes_nothing_between() {
    let mut pager = Setup::default().rule();
    pager.scroll_to(15, 0.0);
    assert!(
        pager.composed.borrow().iter().all(|page| *page == 0 || *page >= 14),
        "{:?}",
        pager.composed
    );
    pager.assert_at(15);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateTest.kt, animateScrollToPage_shouldPlacePagesCorrectly, animateScrollToPage_fixedPageSize_shouldPlacePagesCorrectly
#[test]
fn an_animation_places_the_page() {
    for page_size in [PageSize::Fill, PageSize::Fixed(Dp(100.0))] {
        let mut pager = Setup { page_size, ..Setup::default() }.rule();
        pager.animate_to(1, 0.0);
        pager.assert_at(1);
        pager.animate_to(2, 0.0);
        pager.assert_at(2);
        assert_eq!(pager.pages()[0], (2, 0));
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateTest.kt, animateScrollToPage_usedOffset_shouldPlacePagesCorrectly, animateScrollToPage_moveToSamePageWithOffset_shouldScroll
#[test]
fn an_animation_with_a_fraction_lands_that_far_off_on_the_same_page_too() {
    let mut pager = Setup::default().rule();
    pager.animate_to(3, 0.25);
    assert_eq!(pager.page(), 3);
    assert!((pager.fraction() - 0.25).abs() < 0.01, "{}", pager.fraction());
    pager.animate_to(3, -0.25);
    assert_eq!(pager.page(), 3);
    assert!((pager.fraction() + 0.25).abs() < 0.01, "{}", pager.fraction());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateTest.kt, animateScrollToPage_longSkipShouldNotPlaceIntermediatePages
#[test]
fn a_long_animation_jumps_over_the_pages_between() {
    let mut pager = Setup::default().rule();
    let seen: Rc<RefCell<Vec<usize>>> = Rc::default();
    let (state, noted) = (pager.state, seen.clone());
    pager.ui.set_auto_advance(false);
    let job = launch(async move {
        let _ = state.animate_scroll_to_page(15, 0.0).await;
    });
    let mut frames = 0;
    while job.is_active() {
        pager.ui.advance_time_by_frame();
        noted.borrow_mut().extend(pager.pages().iter().map(|it| it.0));
        frames += 1;
        assert!(frames < 1000);
    }
    assert!(seen.borrow().iter().all(|page| *page == 0 || *page >= 14), "{seen:?}");
    pager.assert_at(15);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateTest.kt, scrollToPage_shouldCoerceWithinRange, animateScrollToPage_shouldCoerceWithinRange
#[test]
fn a_page_out_of_range_is_the_last_one() {
    let mut pager = Setup::default().rule();
    pager.scroll_to(COUNT + 5, 0.0);
    pager.assert_at(COUNT - 1);
    pager.scroll_to(0, 0.0);
    pager.animate_to(COUNT + 5, 0.0);
    pager.assert_at(COUNT - 1);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateTest.kt, currentPage_shouldChangeWhenClosestPageToSnappedPositionChanges
#[test]
fn the_current_page_is_the_one_closest_to_the_snap_position() {
    let mut pager = Setup::default().rule();
    let state = pager.state;
    run(&mut pager.ui, async move {
        state.scroll_by(150.0).await.expect("free");
    });
    assert_eq!(pager.page(), 0);
    assert!((pager.fraction() - 0.375).abs() < 0.01);
    run(&mut pager.ui, async move {
        state.scroll_by(100.0).await.expect("free");
    });
    assert_eq!(pager.page(), 1);
    assert!((pager.fraction() + 0.375).abs() < 0.01, "{}", pager.fraction());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateTest.kt, targetPage_performScrollBelowMinThreshold_shouldNotShowNextPage, targetPage_performScroll_shouldShowNextPage
#[test]
fn a_drag_targets_the_next_page_past_the_position_threshold() {
    let mut pager = Setup { page: 5, ..Setup::default() }.rule();
    let down = pager.at(300.0);
    pager.ui.on_root().perform_touch_input(|touch| {
        touch.down(down);
        touch.move_by(Offset::new(-30.0, 0.0));
    });
    pager.ui.wait_for_idle();
    assert!(pager.state.is_scroll_in_progress());
    assert_eq!(pager.state.target_page(), 5, "under 56 dp");
    pager.ui.on_root().perform_touch_input(|touch| touch.move_by(Offset::new(-100.0, 0.0)));
    pager.ui.wait_for_idle();
    assert_eq!(pager.state.target_page(), 6);
    pager.ui.on_root().perform_touch_input(|touch| {
        touch.advance_event_time(3000);
        touch.up();
    });
    pager.ui.wait_for_idle();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateTest.kt, targetPage_shouldReflectTargetWithAnimation
#[test]
fn an_animation_targets_where_it_goes_from_its_start() {
    let mut pager = Setup::default().rule();
    let state = pager.state;
    pager.ui.set_auto_advance(false);
    launch(async move {
        let _ = state.animate_scroll_to_page(5, 0.0).await;
    });
    pager.ui.advance_time_by_frame();
    pager.ui.advance_time_by_frame();
    assert_eq!(state.target_page(), 5);
    pager.ui.set_auto_advance(true);
    pager.ui.wait_for_idle();
    assert_eq!(state.target_page(), 5);
    pager.assert_at(5);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateTest.kt, targetPage_shouldReflectTargetWithCustomAnimation
#[test]
fn a_custom_scroll_tells_its_target() {
    let mut pager = Setup::default().rule();
    let state = pager.state;
    pager.ui.set_auto_advance(false);
    let job = launch(async move {
        let _ = state
            .scroll(MutatePriority::Default, move |scope| async move {
                state.update_target_page(3);
                scope.scroll_by(1.0);
                std::future::pending::<()>().await;
            })
            .await;
    });
    pager.ui.advance_time_by_frame();
    assert_eq!(state.target_page(), 3);
    job.cancel();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateTest.kt, settledPage_onAnimationScroll_shouldChangeOnScrollFinishedOnly, settledPage_onGestureScroll_shouldChangeOnScrollFinishedOnly
#[test]
fn the_settled_page_changes_when_a_scroll_ends() {
    let mut pager = Setup::default().rule();
    let state = pager.state;
    pager.ui.set_auto_advance(false);
    let job = launch(async move {
        let _ = state.animate_scroll_to_page(3, 0.0).await;
    });
    let mut changed_early = false;
    while job.is_active() {
        pager.ui.advance_time_by_frame();
        if job.is_active() && state.settled_page() != 0 {
            changed_early = true;
        }
    }
    assert!(!changed_early);
    pager.ui.set_auto_advance(true);
    pager.ui.wait_for_idle();
    assert_eq!(state.settled_page(), 3);
    // By a drag.
    let at = pager.at(300.0);
    pager.ui.on_root().perform_touch_input(|touch| {
        touch.down(at);
        touch.move_by(Offset::new(-300.0, 0.0));
    });
    pager.ui.wait_for_idle();
    assert_eq!(state.current_page(), 4);
    assert_eq!(state.settled_page(), 3);
    pager.ui.on_root().perform_touch_input(|touch| {
        touch.advance_event_time(3000);
        touch.up();
    });
    pager.ui.wait_for_idle();
    assert_eq!(state.settled_page(), 4);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateTest.kt, currentPageOffset_shouldReflectScrollingOfCurrentPage
#[test]
fn the_fraction_follows_a_scroll() {
    let mut pager = Setup { page: 5, ..Setup::default() }.rule();
    let state = pager.state;
    for delta in [40.0, 40.0, 40.0] {
        let before = state.current_page_offset_fraction();
        run(&mut pager.ui, async move {
            state.scroll_by(delta).await.expect("free");
        });
        assert!((state.current_page_offset_fraction() - before - 0.1).abs() < 0.01);
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateNonGestureScrollingTest.kt, getOffsetDistanceInPages_shouldBeBasedOnCurrentPage
#[test]
fn the_offset_distance_counts_pages_and_the_fraction() {
    let mut pager = Setup::default().rule();
    pager.scroll_to(5, 0.25);
    assert!((pager.state.get_offset_distance_in_pages(7) - 1.75).abs() < 0.01);
    assert!((pager.state.get_offset_distance_in_pages(5) + 0.25).abs() < 0.01);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PageCountTest.kt, pageCountIsZero_checkNoPagesArePlaced
#[test]
fn no_pages_places_nothing() {
    let pager = Setup { count: 0, ..Setup::default() }.rule();
    assert!(pager.pages().is_empty());
    assert_eq!(pager.page(), 0);
    assert!(!pager.state.can_scroll_forward());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PageCountTest.kt, pageCountIsMax_shouldAllowScroll, pageCountIsMax_shouldNotAllowScrollBeyondMax_fullPages, pageCountIsMax_shouldSettleAfterSmallScroll
#[test]
fn as_many_pages_as_can_be_scroll_and_end() {
    let count = i32::MAX as usize;
    let mut pager = Setup { count, page: count / 2, ..Setup::default() }.rule();
    pager.assert_at(count / 2);
    pager.swipe(-100.0, 1000.0);
    pager.assert_at(count / 2 + 1);
    pager.scroll_to(count - 1, 0.0);
    pager.assert_at(count - 1);
    assert!(!pager.state.can_scroll_forward());
    pager.swipe(-300.0, 2000.0);
    pager.assert_at(count - 1);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PageSizeTest.kt, pageSizeFill_onlySnappedItemIsDisplayed, pagerSizeCustom_visibleItemsAreWithinViewport
#[test]
fn a_page_fills_or_is_of_the_size_asked() {
    let pager = Setup::default().rule();
    assert_eq!(pager.pages(), [(0, 0)]);
    let custom = PageSize::Custom(Rc::new(|_, available, _| available / 3));
    let pager = Setup { page_size: custom, ..Setup::default() }.rule();
    assert_eq!(pager.state.layout_info().page_size, 133);
    assert_eq!(pager.pages(), [(0, 0), (1, 133), (2, 266), (3, 399)]);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PageSizeTest.kt, pageSize_0dp_doesNotMoveOnScroll
#[test]
fn pages_of_no_size_dont_move() {
    let mut pager =
        Setup { page_size: PageSize::Fixed(Dp(0.0)), count: 1, ..Setup::default() }.rule();
    pager.swipe(-100.0, 0.0);
    assert_eq!(pager.page(), 0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerSwipeEdgeTest.kt, swipePageTowardsEdge_shouldNotMove, scrollForwardAtTheLastPage_withSpacing_shouldNotMovePage, scrollForwardAtTheLastPage_withSpacing_pageSettlesCorrectly
#[test]
fn a_swipe_at_an_edge_doesnt_move() {
    let mut pager = Setup { count: 3, spacing: 10.0, ..Setup::default() }.rule();
    pager.swipe(160.0, 600.0);
    pager.assert_at(0);
    pager.scroll_to(2, 0.0);
    pager.swipe(-160.0, 600.0);
    pager.assert_at(2);
    assert_eq!(pager.pages(), [(2, 0)]);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerGestureTest.kt, swipePagerWithSmallContent_shouldNotCrash
#[test]
fn a_swipe_over_small_pages_settles() {
    let mut pager =
        Setup { page_size: PageSize::Fixed(Dp(10.0)), count: 3, ..Setup::default() }.rule();
    pager.swipe(-100.0, 1000.0);
    assert_eq!(pager.page(), 0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerScrollingTest.kt, swipeWithLowVelocity_positionalThresholdLessThanDefaultThreshold_shouldBounceBack_ltr, swipeWithLowVelocity_positionalThresholdLessThanDefaultThreshold_shouldBounceBack_rtl
#[test]
fn a_short_slow_swipe_comes_back() {
    for rtl in [false, true] {
        for vertical in [false, true] {
            let mut pager = Setup { page: 5, rtl, vertical, ..Setup::default() }.rule();
            let way = if rtl && !vertical { 1.0 } else { -1.0 };
            pager.swipe(40.0 * way, 0.0);
            pager.assert_at(5);
        }
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerScrollingTest.kt, swipeWithLowVelocity_positionalThresholdOverDefaultThreshold_shouldGoToNextPage
#[test]
fn a_slow_swipe_past_half_goes_on() {
    for vertical in [false, true] {
        let mut pager = Setup { page: 5, vertical, ..Setup::default() }.rule();
        pager.swipe(-(SIZE * 0.6), 0.0);
        pager.assert_at(6);
        pager.swipe(SIZE * 0.6, 0.0);
        pager.assert_at(5);
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerScrollingTest.kt, swipeWithHighVelocity_defaultVelocityThreshold_shouldGoToNextPage, swipeWithHighVelocity_overHalfPage_shouldGoToNextPage
#[test]
fn a_fast_swipe_goes_on_a_page_at_most() {
    for (delta, velocity) in [(20.0, MIN_FLING * 1.1), (240.0, MIN_FLING * 4.0)] {
        let mut pager = Setup { page: 5, ..Setup::default() }.rule();
        pager.swipe(-delta, velocity);
        pager.assert_at(6);
        pager.swipe(delta, velocity);
        pager.assert_at(5);
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerScrollingTest.kt, scrollForwardAndBackwards_shouldSettleInZeroPageFraction
#[test]
fn a_scroll_there_and_back_settles_on_a_page() {
    let mut pager = Setup { page: 5, ..Setup::default() }.rule();
    let at = pager.at(300.0);
    pager.ui.on_root().perform_touch_input(|touch| {
        touch.down(at);
        touch.move_by(Offset::new(-150.0, 0.0));
        touch.move_by(Offset::new(120.0, 0.0));
        touch.advance_event_time(3000);
        touch.up();
    });
    pager.ui.wait_for_idle();
    assert_eq!(pager.fraction(), 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerScrollingTest.kt, scrollWithSameVelocity_shouldYieldSameResult_forward, scrollWithSameVelocity_shouldYieldSameResult_backward
#[test]
fn the_same_fling_lands_the_same_wherever_it_starts() {
    let mut landed = Vec::new();
    for offset in [0.0, 30.0, 90.0] {
        let mut pager = Setup { page: 5, ..Setup::default() }.rule();
        let state = pager.state;
        run(&mut pager.ui, async move {
            state.scroll_by(offset).await.expect("free");
        });
        pager.swipe(-20.0, MIN_FLING * 2.0);
        landed.push(pager.page());
    }
    assert!(landed.iter().all(|page| *page == 6), "{landed:?}");
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerTest.kt, userScrollEnabledIsOff_shouldNotAllowGestureScroll, userScrollEnabledIsOff_shouldAllowAnimationScroll, userScrollEnabledIsOn_shouldAllowGestureScroll
#[test]
fn scrolling_by_gestures_can_be_turned_off_but_not_by_code() {
    let mut pager = Setup { user_scroll: false, ..Setup::default() }.rule();
    pager.swipe(-300.0, 2000.0);
    pager.assert_at(0);
    pager.animate_to(2, 0.0);
    pager.assert_at(2);
    let mut pager = Setup::default().rule();
    pager.swipe(-300.0, 2000.0);
    pager.assert_at(1);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerTest.kt, userScrollEnabledIsOn_shouldAllowMouseWheelScroll
#[test]
fn a_wheel_turns_a_page() {
    let mut pager = Setup { vertical: true, ..Setup::default() }.rule();
    pager.ui.on_root().perform_mouse_input(|mouse| {
        mouse.move_to(Offset::new(200.0, 200.0));
        mouse.scroll(100.0, ScrollWheel::Vertical);
    });
    pager.ui.wait_for_idle();
    assert_ne!(pager.page(), 0);
    assert_eq!(pager.fraction(), 0.0, "on a page");
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerTest.kt, pageCount_pagerOnlyContainsGivenPageCountItems, mutablePageCount_assertPagesAreChangedIfCountIsChanged, pageCountDecreased_currentPageIsAdjustedAccordingly
#[test]
fn a_new_page_count_changes_the_pages_and_the_current_one() {
    let count = mutable_state_of(10usize);
    let read = count.handle();
    let kept: Rc<Cell<Option<PagerState>>> = Rc::default();
    let noted = kept.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let state = remember_pager_state!(0, 0.0, move || read.get());
        noted.set(Some(state));
        HorizontalPager!(
            state,
            modifier = Modifier.fill_max_size(),
            page_content = |_, index: usize| {
                Box!(modifier = Modifier.fill_max_size().test_tag(index.to_string()));
            }
        );
    });
    ui.wait_for_idle();
    let state = kept.get().expect("composed");
    assert_eq!(state.page_count(), 10);
    run(&mut ui, async move {
        state.scroll_to_page(9, 0.0).await.expect("free");
    });
    ui.on_node_with_tag("9").assert_exists();
    count.set(5);
    ui.wait_for_idle();
    assert_eq!(state.page_count(), 5);
    assert_eq!(state.current_page(), 4);
    ui.on_node_with_tag("4").assert_exists();
    ui.on_node_with_tag("9").assert_does_not_exist();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerTest.kt, pagerCreation_sumOfPageSizeIsSmallerThanPager_makeSurePagesAreAlignedToStartTop
#[test]
fn pages_smaller_than_the_pager_start_at_its_start() {
    let pager =
        Setup { page_size: PageSize::Fixed(Dp(100.0)), count: 2, ..Setup::default() }.rule();
    assert_eq!(pager.pages(), [(0, 0), (1, 100)]);
    assert!(!pager.state.can_scroll_forward());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerTest.kt, contentPadding_largerThanConstraints_measuresAsZero
#[test]
fn a_padding_larger_than_the_pager_leaves_a_page_of_nothing() {
    let pager = Setup { padding: 300.0, ..Setup::default() }.rule();
    assert_eq!(pager.state.layout_info().page_size, 0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerLayoutInfoTest.kt, visiblePagesAreCorrect, visiblePagesAreCorrectAfterScroll, visiblePagesAreCorrectWithSpacing
#[test]
fn the_visible_pages_are_those_in_view() {
    let mut pager =
        Setup { page_size: PageSize::Fixed(Dp(150.0)), spacing: 10.0, ..Setup::default() }.rule();
    assert_eq!(pager.pages(), [(0, 0), (1, 160), (2, 320)]);
    let state = pager.state;
    run(&mut pager.ui, async move {
        state.scroll_by(10.0).await.expect("free");
    });
    assert_eq!(pager.pages(), [(0, -10), (1, 150), (2, 310)]);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerLayoutInfoTest.kt, viewportOffsetsAndSizeAreCorrect, viewportOffsetsAndSizeAreCorrectWithContentPadding, reverseLayoutIsCorrect, orientationIsCorrect
#[test]
fn the_layout_info_tells_the_viewport_paddings_and_axis() {
    let pager = Setup { vertical: true, reverse: true, padding: 20.0, ..Setup::default() }.rule();
    let info = pager.state.layout_info();
    assert_eq!(info.orientation, Orientation::Vertical);
    assert!(info.reverse_layout);
    assert_eq!(info.viewport_start_offset, -20);
    assert_eq!(info.viewport_end_offset, 380);
    assert_eq!((info.before_content_padding, info.after_content_padding), (20, 20));
    assert_eq!(info.viewport_size, IntSize::new(400, 400));
    assert_eq!(info.page_size, 360);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerLayoutInfoTest.kt, emptyContent
#[test]
fn no_pages_tell_an_empty_layout() {
    let pager = Setup { count: 0, ..Setup::default() }.rule();
    let info = pager.state.layout_info();
    assert!(info.visible_pages_info.is_empty());
    assert_eq!((info.viewport_start_offset, info.viewport_end_offset), (0, 400));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerOffscreenPageLimitPlacingTest.kt, offscreenPageLimitIsUsed_shouldPlaceMoreItemsThanVisibleOnes, offscreenPageLimitIsNotUsed_shouldNotPlaceMoreItemsThanVisibleOnes, offscreenPageLimitIsUsed_shouldPlaceMoreItemsThanVisibleOnesAsWeScroll
#[test]
fn pages_beyond_the_viewport_are_composed_as_asked() {
    let mut pager = Setup { page: 5, beyond: 2, ..Setup::default() }.rule();
    let mut composed = pager.composed.borrow().clone();
    composed.sort();
    assert_eq!(composed, [3, 4, 5, 6, 7]);
    assert_eq!(pager.pages(), [(5, 0)], "not visible");
    pager.scroll_to(9, 0.0);
    // Those left stay until the next scroll's measurement (D896).
    for page in [7, 8, 9, 10, 11] {
        assert!(pager.composed.borrow().contains(&page), "{page}: {:?}", pager.composed);
    }
    let pager = Setup { page: 5, ..Setup::default() }.rule();
    assert_eq!(*pager.composed.borrow(), [5]);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerRequestScrollTest.kt, requestScrollToPage_currentPageUpdatesInstantly, requestScrollToPage_withoutChangingData_scrollsToRequestedPage
#[test]
fn a_request_goes_there_at_the_next_layout_its_page_told_at_once() {
    let mut pager = Setup::default().rule();
    pager.state.request_scroll_to_page(5, 0.0);
    assert_eq!(pager.page(), 5);
    pager.ui.wait_for_idle();
    pager.assert_at(5);
    assert_eq!(pager.pages(), [(5, 0)]);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerRequestScrollTest.kt, requestScrollToPage_whileScrolling_cancelsScroll
#[test]
fn a_request_stops_a_scroll_going_on() {
    let mut pager = Setup::default().rule();
    let state = pager.state;
    pager.ui.set_auto_advance(false);
    let job = launch(async move {
        let _ = state.animate_scroll_to_page_with(10, 0.0, compose_animation::tween(5000)).await;
    });
    pager.ui.advance_time_by_frame();
    pager.ui.advance_time_by_frame();
    assert!(state.is_scroll_in_progress());
    state.request_scroll_to_page(3, 0.0);
    pager.ui.advance_time_by_frame();
    pager.ui.advance_time_by_frame();
    assert!(!job.is_active());
    pager.ui.set_auto_advance(true);
    pager.ui.wait_for_idle();
    pager.assert_at(3);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerSnapPositionTest.kt, snapPosition_shouldNotInfluenceMaxScroll, pagerAtBounds_flingTowardsBound_doesNotMove
#[test]
fn a_snap_position_keeps_the_ends_of_the_scroll() {
    for snap_position in [SnapPosition::Start, SnapPosition::Center, SnapPosition::End] {
        let mut pager = Setup {
            page_size: PageSize::Fixed(Dp(100.0)),
            count: 10,
            snap_position,
            ..Setup::default()
        }
        .rule();
        let state = pager.state;
        run(&mut pager.ui, async move {
            state.scroll_by(100000.0).await.expect("free");
        });
        assert!(!state.can_scroll_forward());
        let last = *pager.pages().last().expect("laid out");
        assert_eq!(last, (9, 300));
        pager.swipe(-100.0, MIN_FLING * 2.0);
        assert_eq!(*pager.pages().last().expect("laid out"), (9, 300));
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerTest.kt, snapPositionChanges_shouldReLayoutPages
#[test]
fn a_page_lands_at_its_snap_position() {
    let mut pager = Setup {
        page_size: PageSize::Fixed(Dp(100.0)),
        snap_position: SnapPosition::Center,
        page: 5,
        ..Setup::default()
    }
    .rule();
    assert_eq!(pager.pages().iter().find(|it| it.0 == 5), Some(&(5, 150)));
    pager.assert_at(5);
    pager.swipe(-20.0, MIN_FLING * 1.1);
    assert_eq!(pager.page(), 6);
    assert_eq!(pager.pages().iter().find(|it| it.0 == 6), Some(&(6, 150)));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerCrossAxisTest.kt, pagerOnInfiniteCrossAxisLayout_shouldWrapContentSize
#[test]
fn a_pager_with_room_across_wraps_its_pages() {
    let kept: Rc<Cell<Option<PagerState>>> = Rc::default();
    let noted = kept.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let state = remember_pager_state!(0, 0.0, || 3);
        noted.set(Some(state));
        Column!(modifier = Modifier.vertical_scroll(remember_scroll_state(0)), {
            HorizontalPager!(
                state,
                modifier = Modifier.test_tag("pager"),
                page_content = |_, _| {
                    Box!(modifier = Modifier.height(100.dp()).fill_max_width());
                }
            );
        });
    });
    ui.wait_for_idle();
    let bounds = ui.on_node_with_tag("pager").fetch_semantics_snapshot().bounds;
    assert_eq!(bounds.bottom - bounds.top, 100.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateNonGestureScrollingTest.kt, testStateRestoration
#[test]
fn a_remembered_state_is_restored_at_its_page_and_fraction() {
    let kept: Rc<Cell<Option<PagerState>>> = Rc::default();
    let noted = kept.clone();
    let mut tester = StateRestorationTester::new();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), || {});
    tester.set_content(&mut ui, move || {
        let state = remember_pager_state!(0, 0.0, || 20);
        noted.set(Some(state));
        HorizontalPager!(
            state,
            modifier = Modifier.fill_max_size(),
            page_content = |_, _| {
                Box!(modifier = Modifier.fill_max_size());
            }
        );
    });
    ui.wait_for_idle();
    let state = kept.get().expect("composed");
    run(&mut ui, async move {
        state.scroll_to_page(5, 0.25).await.expect("free");
    });
    tester.emulate_save_and_restore(&mut ui);
    ui.wait_for_idle();
    let restored = kept.get().expect("composed");
    assert_eq!(restored.current_page(), 5);
    assert!((restored.current_page_offset_fraction() - 0.25).abs() < 0.01);
}

/// A fling of its own going to page 3.
struct ToPage(PagerState);

impl FlingBehavior for ToPage {
    fn perform_fling<'a>(
        &'a self,
        _scope: ScrollScope,
        _initial_velocity: f32,
    ) -> compose_foundation::gestures::LocalBoxFuture<'a, f32> {
        Box::pin(async { 0.0 })
    }
}

impl TargetedFlingBehavior for ToPage {
    fn perform_targeted_fling<'a>(
        &'a self,
        _scope: ScrollScope,
        _initial_velocity: f32,
        on_remaining: Rc<dyn Fn(f32)>,
    ) -> compose_foundation::gestures::LocalBoxFuture<'a, f32> {
        Box::pin(async move {
            on_remaining(1200.0);
            assert_eq!(self.0.target_page(), 3);
            0.0
        })
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateTest.kt, targetPage_performingFlingWithCustomFling_shouldGoToPredictedPage
#[test]
fn a_fling_of_ones_own_targets_by_what_it_says_remains() {
    let owned = PagerState::new(0, 0.0, || 20);
    let state = *owned;
    std::mem::forget(owned);
    let fling: Rc<dyn TargetedFlingBehavior> = Rc::new(ToPage(state));
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        HorizontalPager!(
            state,
            modifier = Modifier.fill_max_size(),
            fling_behavior = Some(fling.clone()),
            page_content = |_, _| {
                Box!(modifier = Modifier.fill_max_size());
            }
        );
    });
    ui.wait_for_idle();
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe_with_velocity(
            Offset::new(390.0, 200.0),
            Offset::new(300.0, 200.0),
            2000.0,
            None,
        );
    });
    ui.wait_for_idle();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerTest.kt, customOverscroll
#[test]
fn a_pager_gives_an_effect_its_deltas_past_the_end() {
    struct Effect(Rc<Cell<f32>>);
    impl OverscrollEffect for Effect {
        fn apply_to_scroll(
            &self,
            delta: Offset,
            _: compose_ui::NestedScrollSource,
            perform: &mut dyn FnMut(Offset) -> Offset,
        ) -> Offset {
            let used = perform(delta);
            self.0.set(self.0.get() + delta.x - used.x);
            delta
        }
        fn apply_to_fling<'a>(
            &'a self,
            velocity: compose_ui::Velocity,
            perform: PerformFling,
        ) -> compose_foundation::gestures::LocalBoxFuture<'a, ()> {
            Box::pin(async move {
                perform(velocity).await;
            })
        }
        fn is_in_progress(&self) -> bool {
            false
        }
    }
    let left = Rc::new(Cell::new(0.0));
    let given: Rc<dyn OverscrollEffect> = Rc::new(Effect(left.clone()));
    let owned = PagerState::new(0, 0.0, || 3);
    let state = *owned;
    std::mem::forget(owned);
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        HorizontalPager!(
            state,
            modifier = Modifier.fill_max_size(),
            overscroll_effect = Some(given.clone()),
            page_content = |_, _| {
                Box!(modifier = Modifier.fill_max_size());
            }
        );
    });
    ui.wait_for_idle();
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe(Offset::new(10.0, 200.0), Offset::new(200.0, 200.0), 200);
    });
    ui.wait_for_idle();
    assert!(left.get() > 0.0, "{}", left.get());
}

/// A pager of `count` pages each holding a list of 10 items of 100, along or across its axis.
fn nested_pager(vertical: bool, along: bool) -> (ComposeTestRule, PagerState) {
    nested_pager_of(vertical, along, 10)
}

fn nested_pager_of(vertical: bool, along: bool, items: usize) -> (ComposeTestRule, PagerState) {
    let owned = PagerState::new(0, 0.0, || COUNT);
    let state = *owned;
    std::mem::forget(owned);
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let page = move |_: PagerScope, _: usize| {
            let items = move |scope: lazy::LazyListScope| {
                scope.items(items, |_| Box!(modifier = Modifier.size(100.dp())));
            };
            if vertical == along {
                LazyColumn!(modifier = Modifier.fill_max_size(), content = items);
            } else {
                LazyRow!(modifier = Modifier.fill_max_size(), content = items);
            }
        };
        if vertical {
            VerticalPager!(state, modifier = Modifier.fill_max_size(), page_content = page);
        } else {
            HorizontalPager!(state, modifier = Modifier.fill_max_size(), page_content = page);
        }
    });
    ui.wait_for_idle();
    (ui, state)
}

fn along(vertical: bool, main: f32, cross: f32) -> Offset {
    if vertical { Offset::new(cross, main) } else { Offset::new(main, cross) }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerNestedScrollContentTest.kt, nestedScrollContent_shouldNotPropagateUnconsumedFlings
#[test]
fn a_fling_of_a_list_in_a_page_doesnt_turn_the_page() {
    for vertical in [false, true] {
        let (mut ui, state) = nested_pager(vertical, true);
        ui.on_root().perform_touch_input(|touch| {
            touch.swipe_with_velocity(
                along(vertical, 390.0, 200.0),
                along(vertical, 190.0, 200.0),
                10000.0,
                None,
            );
        });
        ui.wait_for_idle();
        assert_eq!(state.current_page(), 0);
        assert!(state.current_page_offset_fraction().abs() < 0.01);
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerNestedScrollContentTest.kt, nestedScrollContent_shouldPropagateScrollCorrectly
#[test]
fn a_drag_a_list_in_a_page_cant_use_moves_the_pager() {
    let (mut ui, state) = nested_pager_of(false, true, 2);
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe_with_velocity(
            Offset::new(390.0, 200.0),
            Offset::new(150.0, 200.0),
            1000.0,
            None,
        );
    });
    ui.wait_for_idle();
    assert_eq!(state.current_page(), 1);
    assert_eq!(state.current_page_offset_fraction(), 0.0);
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe_with_velocity(
            Offset::new(10.0, 200.0),
            Offset::new(250.0, 200.0),
            1000.0,
            None,
        );
    });
    ui.wait_for_idle();
    assert_eq!(state.current_page(), 0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerNestedScrollContentTest.kt, nestedScrollContent_shouldEnsurePagerIsSettled_WhenDirectionChanges
#[test]
fn a_page_moved_off_its_place_takes_a_drag_before_the_list_in_it() {
    let (mut ui, state) = nested_pager(false, true);
    let moved = state;
    run(&mut ui, async move {
        moved.scroll_by(100.0).await.expect("free");
    });
    assert!(state.current_page_offset_fraction() > 0.0);
    let at_rest = Offset::new(200.0, 200.0);
    ui.on_root().perform_touch_input(|touch| {
        touch.down(at_rest);
        touch.move_by(Offset::new(150.0, 0.0));
    });
    ui.wait_for_idle();
    assert_eq!(state.current_page(), 0);
    assert!(state.current_page_offset_fraction().abs() < 0.1);
    ui.on_root().perform_touch_input(|touch| {
        touch.advance_event_time(3000);
        touch.up();
    });
    ui.wait_for_idle();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerNestedScrollContentTest.kt, nestedScrollContent_shouldPropagateCrossAxisUnconsumedFlings
#[test]
fn a_fling_of_a_list_across_a_page_doesnt_move_the_pager() {
    let (mut ui, state) = nested_pager(false, false);
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe_with_velocity(
            Offset::new(200.0, 390.0),
            Offset::new(200.0, 190.0),
            10000.0,
            None,
        );
    });
    ui.wait_for_idle();
    assert_eq!(state.current_page(), 0);
    assert_eq!(state.current_page_offset_fraction(), 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateNonGestureScrollingTest.kt, pagerStateNotAttached_shouldReturnDefaultValues_andChangeAfterAttached, initialPageOnPagerState_shouldDisplayThatPageFirst
#[test]
fn before_its_first_layout_a_state_tells_what_it_was_given() {
    let owned = PagerState::new(5, 0.25, || COUNT);
    assert_eq!(owned.current_page(), 5);
    assert_eq!(owned.current_page_offset_fraction(), 0.25);
    assert_eq!(owned.target_page(), 5);
    assert_eq!(owned.settled_page(), 5);
    assert_eq!(owned.page_count(), COUNT);
    let pager = Setup { page: 5, ..Setup::default() }.rule();
    pager.assert_at(5);
    assert_eq!(pager.pages(), [(5, 0)]);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateNonGestureScrollingTest.kt, pageSizeIsZero_offsetFractionShouldNotBeNan, currentPageOffsetFraction_shouldNeverBeNan
#[test]
fn a_page_of_no_size_has_a_fraction_of_nothing() {
    let pager = Setup { page_size: PageSize::Fixed(Dp(0.0)), ..Setup::default() }.rule();
    assert_eq!(pager.fraction(), 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateNonGestureScrollingTest.kt, scrollTo_beforeFirstLayout_shouldWaitForStateAndLayoutSetting, animatedScrollToPage_emptyPager_shouldNotReact
#[test]
fn a_jump_before_the_first_layout_waits_for_it() {
    let owned = PagerState::new(0, 0.0, || COUNT);
    let state = *owned;
    std::mem::forget(owned);
    let done = Rc::new(Cell::new(false));
    let noted = done.clone();
    CoroutineScope::new().launch(async move {
        state.scroll_to_page(3, 0.0).await.expect("free");
        noted.set(true);
    });
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        HorizontalPager!(
            state,
            modifier = Modifier.fill_max_size(),
            page_content = |_, _| {
                Box!(modifier = Modifier.fill_max_size());
            }
        );
    });
    ui.wait_for_idle();
    assert!(done.get());
    assert_eq!(state.current_page(), 3);
    let mut empty = Setup { count: 0, ..Setup::default() }.rule();
    empty.animate_to(3, 0.0);
    assert_eq!(empty.page(), 0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateNonGestureScrollingTest.kt, updateCurrentPage_shouldUpdateCurrentPageImmediately, updateCurrentPage_shouldUpdateCurrentPageOffsetFractionImmediately, updateTargetPage_shouldUpdateTargetPageImmediately_andResetIfNotMoved
#[test]
fn in_a_scroll_the_current_and_target_pages_can_be_told() {
    let mut pager = Setup::default().rule();
    let state = pager.state;
    run(&mut pager.ui, async move {
        state
            .scroll(MutatePriority::Default, move |_| async move {
                state.update_current_page(5, 0.2);
                assert_eq!(state.current_page(), 5);
                assert_eq!(state.current_page_offset_fraction(), 0.2);
                state.update_target_page(9);
                assert_eq!(state.target_page(), 9);
            })
            .await
            .expect("free");
    });
    assert_eq!(state.target_page(), 5, "reset, not moved");
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateNonGestureScrollingTest.kt, snapPositionInLayout_startToStart_currentPageShouldBeCloserToStartOfLayout, snapPositionInLayout_centerToCenter_currentPageShouldBeCloserToMiddleOfLayout, snapPositionInLayout_endToEnd_currentPageShouldBeCloserToEndOfLayout
#[test]
fn the_current_page_is_where_the_snap_position_is() {
    // Pages of 100 in 400: a page at its snap position starts at 0, 150 and 300.
    for (snap_position, at) in
        [(SnapPosition::Start, 0), (SnapPosition::Center, 150), (SnapPosition::End, 300)]
    {
        let mut pager = Setup {
            page_size: PageSize::Fixed(Dp(100.0)),
            snap_position,
            page: 8,
            ..Setup::default()
        }
        .rule();
        pager.assert_at(8);
        assert_eq!(pager.pages().iter().find(|it| it.0 == 8), Some(&(8, at)));
        // Scrolled a little, it stays the closest, its fraction told.
        let state = pager.state;
        run(&mut pager.ui, async move {
            state.scroll_by(20.0).await.expect("free");
        });
        assert_eq!(pager.page(), 8);
        assert!((pager.fraction() - 0.2).abs() < 0.01, "{}", pager.fraction());
        run(&mut pager.ui, async move {
            state.scroll_by(40.0).await.expect("free");
        });
        assert_eq!(pager.page(), 9, "past half way");
        assert!((pager.fraction() + 0.4).abs() < 0.01, "{}", pager.fraction());
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateNonGestureScrollingTest.kt, canScrollForwardAndBackward_afterSmallScrollFromStart, canScrollForwardAndBackward_afterSmallScrollFromEnd
#[test]
fn it_can_scroll_as_far_as_its_pages_go() {
    let mut pager = Setup::default().rule();
    let state = pager.state;
    assert!(state.can_scroll_forward());
    assert!(!state.can_scroll_backward());
    run(&mut pager.ui, async move {
        state.scroll_by(1.0).await.expect("free");
    });
    assert!(state.can_scroll_forward());
    assert!(state.can_scroll_backward());
    pager.scroll_to(COUNT - 1, 0.0);
    assert!(!state.can_scroll_forward());
    assert!(state.can_scroll_backward());
    run(&mut pager.ui, async move {
        state.scroll_by(-1.0).await.expect("free");
    });
    assert!(state.can_scroll_forward());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerContentPaddingTest.kt, contentPaddingIsApplied, contentPaddingIsNotAffectingScrollPosition, scrollForwardTillTheEnd
#[test]
fn a_padding_moves_the_pages_in_and_the_page_stays_current() {
    let mut pager = Setup {
        page_size: PageSize::Fixed(Dp(100.0)),
        padding: 50.0,
        count: 3,
        ..Setup::default()
    }
    .rule();
    assert_eq!(pager.pages()[0], (0, 0));
    pager.assert_at(0);
    let bounds = pager.ui.on_node_with_tag("0").fetch_semantics_snapshot().bounds;
    assert_eq!((bounds.left, bounds.top), (50.0, 50.0));
    assert_eq!(bounds.right - bounds.left, 100.0);
    let state = pager.state;
    run(&mut pager.ui, async move {
        state.scroll_by(1000.0).await.expect("free");
    });
    assert!(!state.can_scroll_forward());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerContentPaddingTest.kt, contentPaddingAndNoContent
#[test]
fn a_padding_with_no_pages_is_all_that_is_there() {
    let pager = Setup { count: 0, padding: 20.0, ..Setup::default() }.rule();
    assert!(pager.pages().is_empty());
    assert!(!pager.state.can_scroll_backward());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PageLayoutPositionOnScrollingTest.kt, swipeForwardAndBackward_verifyPagesAreLaidOutCorrectly
#[test]
fn reversed_or_right_to_left_a_swipe_goes_the_other_way() {
    let mut pager = Setup { page: 5, reverse: true, ..Setup::default() }.rule();
    pager.swipe(240.0, MIN_FLING * 4.0);
    pager.assert_at(6);
    let bounds = pager.ui.on_node_with_tag("6").fetch_semantics_snapshot().bounds;
    assert_eq!(bounds.left, 0.0);
    let mut pager = Setup { page: 5, rtl: true, ..Setup::default() }.rule();
    pager.swipe(240.0, MIN_FLING * 4.0);
    pager.assert_at(6);
    let bounds = pager.ui.on_node_with_tag("6").fetch_semantics_snapshot().bounds;
    assert_eq!(bounds.left, 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateNonGestureScrollingTest.kt, currentPage_pagerWithKeys_shouldBeTheSameAfterDatasetUpdate; PagerRequestScrollTest.kt, moveCurrentPage_afterScrollRequestIsProcessed_maintainsScrollByCurrentPageKey
#[test]
fn with_keys_the_current_page_follows_its_key() {
    let list = mutable_state_of((0..10).collect::<Vec<u64>>());
    let read = list.handle();
    let kept: Rc<Cell<Option<PagerState>>> = Rc::default();
    let noted = kept.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let items = read.get();
        let count = items.len();
        let state = remember_pager_state!(0, 0.0, move || count);
        noted.set(Some(state));
        HorizontalPager!(
            state,
            modifier = Modifier.fill_max_size(),
            key = Some(pager_key(move |page| items[page])),
            page_content = |_, _| {
                Box!(modifier = Modifier.fill_max_size());
            }
        );
    });
    ui.wait_for_idle();
    let state = kept.get().expect("composed");
    run(&mut ui, async move {
        state.scroll_to_page(5, 0.0).await.expect("free");
    });
    assert_eq!(state.current_page(), 5);
    // Two prepended: the page with key 5 is now at 7.
    list.set(vec![100, 101, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    ui.wait_for_idle();
    assert_eq!(state.current_page(), 7);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerContentTest.kt, pageContent_makeSureContainerOwnsOutsideModifiers, pageContent_makeSureInnerModifiersAreAppliedToPages
#[test]
fn the_modifier_is_the_pagers_and_a_pages_are_its_own() {
    let mut pager = Setup::default().rule();
    let bounds = pager.ui.on_node_with_tag("pager").fetch_semantics_snapshot().bounds;
    assert_eq!(bounds.right - bounds.left, 400.0);
    let page = pager.ui.on_node_with_tag("0").fetch_semantics_snapshot().bounds;
    assert_eq!(page.right - page.left, 400.0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/pager/PagerMeasure.kt, currentPageOffsetFraction: coerceIn(MinPageOffset, MaxPageOffset)
#[test]
fn a_snap_position_out_of_reach_keeps_the_fraction_in_its_range() {
    let pager = Setup {
        page_size: PageSize::Fixed(Dp(100.0)),
        count: 2,
        snap_position: SnapPosition::End,
        ..Setup::default()
    }
    .rule();
    assert_eq!((pager.page(), pager.fraction()), (1, 0.5));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerTest.kt, pagerSizeChanges_shouldReLayoutPagesAccordingToSnapPosition, pageSizeChanges_shouldReLayoutPagesAccordingToSnapPosition
#[test]
fn a_new_size_keeps_the_page_at_its_snap_position() {
    let owned = PagerState::new(5, 0.0, || COUNT);
    let state = *owned;
    std::mem::forget(owned);
    let width = mutable_state_of(400.0f32);
    let read = width.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 400), move || {
        HorizontalPager!(
            state,
            modifier = Modifier.size((Dp(read.get()), Dp(400.0))),
            page_size = PageSize::Fixed(Dp(100.0)),
            snap_position = SnapPosition::Center,
            page_content = |_, _| {
                Box!(modifier = Modifier.fill_max_size());
            }
        );
    });
    ui.wait_for_idle();
    let at = |state: PagerState| {
        state.layout_info().visible_pages_info.iter().find(|it| it.index == 5).map(|it| it.offset)
    };
    assert_eq!(at(state), Some(150));
    width.set(200.0);
    ui.wait_for_idle();
    assert_eq!(state.current_page(), 5);
    assert_eq!(at(state), Some(50));
}

/// Scrolls 50 px a frame, ten times, noting how many it got to.
struct KeepsGoing(Rc<Cell<u32>>);

impl FlingBehavior for KeepsGoing {
    fn perform_fling<'a>(
        &'a self,
        scope: ScrollScope,
        _initial_velocity: f32,
    ) -> compose_foundation::gestures::LocalBoxFuture<'a, f32> {
        Box::pin(async move {
            for _ in 0..10 {
                compose_runtime::with_frame_nanos(|_| ()).await;
                self.0.set(self.0.get() + 1);
                scope.scroll_by(50.0);
            }
            0.0
        })
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerNestedScrollContentTest.kt, nestedScrollContent_shouldCancelFlingIfOnEdge
#[test]
fn a_fling_at_the_edge_of_a_list_in_a_page_ends_there() {
    let count = Rc::new(Cell::new(0));
    let behavior: Rc<dyn FlingBehavior> = Rc::new(KeepsGoing(count.clone()));
    let owned = PagerState::new(0, 0.0, || COUNT);
    let state = *owned;
    std::mem::forget(owned);
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let behavior = behavior.clone();
        HorizontalPager!(
            state,
            modifier = Modifier.fill_max_size(),
            page_content = move |_, _| {
                LazyRow!(
                    modifier = Modifier.fill_max_size(),
                    fling_behavior = Some(behavior.clone()),
                    content = |scope: lazy::LazyListScope| {
                        scope.items(1, |_| Box!(modifier = Modifier.size(100.dp())));
                    }
                );
            }
        );
    });
    ui.wait_for_idle();
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe_with_velocity(
            Offset::new(390.0, 200.0),
            Offset::new(300.0, 200.0),
            1000.0,
            None,
        );
    });
    ui.wait_for_idle();
    assert_eq!(count.get(), 1, "cancelled by its first delta at the edge");
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/pager/Pager.kt, DefaultPagerNestedScrollConnection.onPostScroll: isAbsorbNonGestureNestedScrollInPagerFixEnabled
#[test]
fn a_wheel_past_the_end_of_a_list_in_a_page_doesnt_move_the_pager() {
    // Room for 100 px of the wheel's 320: the rest is absorbed, not a gesture's.
    let (mut ui, state) = nested_pager_of(true, true, 5);
    ui.on_root().perform_mouse_input(|mouse| {
        mouse.move_to(Offset::new(200.0, 200.0));
        mouse.scroll(5.0, ScrollWheel::Vertical);
    });
    ui.wait_for_idle();
    assert_eq!(state.current_page(), 0);
    assert_eq!(state.current_page_offset_fraction(), 0.0);
}

/// A pager of `page_size` in `window` flinging by `fling`.
fn with_fling(
    window: i32,
    page_size: PageSize,
    fling: impl Fn(PagerState) -> Rc<dyn compose_foundation::gestures::TargetedFlingBehavior> + 'static,
) -> (ComposeTestRule, PagerState) {
    let owned = PagerState::new(5, 0.0, || 200);
    let state = *owned;
    std::mem::forget(owned);
    let behavior = fling(state);
    let mut ui = ComposeTestRule::sized(IntSize::new(window, window), move || {
        HorizontalPager!(
            state,
            modifier = Modifier.fill_max_size(),
            page_size = page_size.clone(),
            fling_behavior = Some(behavior.clone()),
            page_content = |_, _| {
                Box!(modifier = Modifier.fill_max_size());
            }
        );
    });
    ui.wait_for_idle();
    (ui, state)
}

fn fling_of(
    state: PagerState,
    pages: usize,
) -> Rc<dyn compose_foundation::gestures::TargetedFlingBehavior> {
    PagerDefaults::fling_behavior(
        state,
        pager_snap_distance_at_most(pages),
        compose_animation::spline_based_decay(compose_ui::Density::new(1.0)),
        PagerDefaults::snap_animation_spec(),
        0.5,
        false,
    )
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/pager/PagerSnapDistance.kt, atMost
#[test]
fn a_fling_goes_as_far_as_the_snap_distance_lets_it() {
    let (mut ui, state) = with_fling(400, PageSize::Fixed(Dp(50.0)), |state| fling_of(state, 1));
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe_with_velocity(
            Offset::new(390.0, 200.0),
            Offset::new(100.0, 200.0),
            8000.0,
            None,
        );
    });
    ui.wait_for_idle();
    assert_eq!(state.current_page_offset_fraction(), 0.0);
    let one = state.current_page();
    let (mut ui, state) = with_fling(400, PageSize::Fixed(Dp(50.0)), |state| fling_of(state, 3));
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe_with_velocity(
            Offset::new(390.0, 200.0),
            Offset::new(100.0, 200.0),
            8000.0,
            None,
        );
    });
    ui.wait_for_idle();
    assert_eq!(state.current_page(), one + 2, "two pages further");
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/snapping/PagerSnapLayoutInfoProvider.kt, calculateSnapOffset, with a frame for the layout: a scroll past the pages laid out lands on a page
#[test]
fn a_long_fling_past_what_is_laid_out_lands_on_a_page() {
    let (mut ui, state) = with_fling(100, PageSize::Fixed(Dp(20.0)), |state| fling_of(state, 50));
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe_with_velocity(Offset::new(95.0, 50.0), Offset::new(5.0, 50.0), 3000.0, None);
    });
    ui.wait_for_idle();
    assert!(state.current_page() > 5);
    assert_eq!(state.current_page_offset_fraction(), 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateTest.kt, targetPage_valueAfterScrollingForwardAndBackward
#[test]
fn a_drag_back_targets_the_page_before() {
    let mut pager = Setup { page: 5, ..Setup::default() }.rule();
    let at = pager.at(50.0);
    pager.ui.on_root().perform_touch_input(|touch| {
        touch.down(at);
        touch.move_by(Offset::new(100.0, 0.0));
    });
    pager.ui.wait_for_idle();
    assert_eq!(pager.state.target_page(), 4);
    pager.ui.on_root().perform_touch_input(|touch| {
        touch.advance_event_time(3000);
        touch.up();
    });
    pager.ui.wait_for_idle();
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/pager/PagerState.kt, updateTargetPage: coerceInPageRange
#[test]
fn a_target_out_of_range_is_the_last_page() {
    let mut pager = Setup::default().rule();
    let state = pager.state;
    run(&mut pager.ui, async move {
        state
            .scroll(MutatePriority::Default, move |_| async move {
                state.update_target_page(COUNT + 5);
                assert_eq!(state.target_page(), COUNT - 1);
            })
            .await
            .expect("free");
    });
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/snapping/PagerSnapLayoutInfoProvider.kt, calculateFinalSnappingBound: the gesture past the position threshold goes back, wherever the page is
#[test]
fn a_slow_drag_past_the_position_threshold_but_not_half_way_goes_back() {
    let mut pager = Setup { page: 5, ..Setup::default() }.rule();
    let state = pager.state;
    run(&mut pager.ui, async move {
        state.scroll_by(160.0).await.expect("free");
    });
    let at = pager.at(300.0);
    pager.ui.on_root().perform_touch_input(|touch| {
        touch.down(at);
        touch.move_by(Offset::new(-120.0 - 8.0, 0.0));
        touch.advance_event_time(3000);
        touch.up();
    });
    pager.ui.wait_for_idle();
    assert_eq!(pager.page(), 5, "0.7 of the way, the gesture 0.3 of a page");
    assert_eq!(pager.fraction(), 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerNestedScrollContentTest.kt, nestedScrollContent_shouldEnsurePagerIsSettled_WhenCrossDirectionScrolls
#[test]
fn a_page_off_its_place_settles_before_the_list_in_it_scrolls() {
    let inner = LazyListState::new(3, 0);
    let list = *inner;
    std::mem::forget(inner);
    let owned = PagerState::new(0, 0.0, || COUNT);
    let state = *owned;
    std::mem::forget(owned);
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        HorizontalPager!(
            state,
            modifier = Modifier.fill_max_size(),
            page_content = move |_, page: usize| {
                let items = |scope: lazy::LazyListScope| {
                    scope.items(10, |_| Box!(modifier = Modifier.size(100.dp())));
                };
                if page == 0 {
                    LazyRow!(modifier = Modifier.fill_max_size(), state = list, content = items);
                } else {
                    Box!(modifier = Modifier.fill_max_size());
                }
            }
        );
    });
    ui.wait_for_idle();
    run(&mut ui, async move {
        state.scroll_by(80.0).await.expect("free");
    });
    let (index, offset) =
        (list.first_visible_item_index(), list.first_visible_item_scroll_offset());
    ui.on_root().perform_touch_input(|touch| {
        touch.down(Offset::new(100.0, 200.0));
        touch.move_by(Offset::new(60.0 + 8.0, 0.0));
    });
    ui.wait_for_idle();
    assert_eq!(
        (list.first_visible_item_index(), list.first_visible_item_scroll_offset()),
        (index, offset)
    );
    assert!(
        (state.current_page_offset_fraction() - 0.05).abs() < 0.01,
        "{}",
        state.current_page_offset_fraction()
    );
    ui.on_root().perform_touch_input(|touch| {
        touch.advance_event_time(3000);
        touch.up();
    });
    ui.wait_for_idle();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerNestedScrollContentTest.kt, nestedScrollContent_shouldNotPropagateUnconsumedFlings; DefaultPagerNestedScrollConnection.onPostFling
#[test]
fn what_a_fling_in_a_page_leaves_isnt_for_the_pager() {
    let owned = PagerState::new(5, 0.0, || COUNT);
    let state = *owned;
    std::mem::forget(owned);
    let behavior = fling_of(state, 5);
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        HorizontalPager!(
            state,
            modifier = Modifier.fill_max_size(),
            page_size = PageSize::Fixed(Dp(50.0)),
            fling_behavior = Some(behavior.clone()),
            page_content = |_, _| {
                LazyRow!(
                    modifier = Modifier.fill_max_size(),
                    content = |scope: lazy::LazyListScope| {
                        scope.items(1, |_| Box!(modifier = Modifier.size(10.dp())));
                    }
                );
            }
        );
    });
    ui.wait_for_idle();
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe_with_velocity(
            Offset::new(390.0, 200.0),
            Offset::new(340.0, 200.0),
            1000.0,
            None,
        );
    });
    ui.wait_for_idle();
    assert!(state.current_page() <= 6, "{}", state.current_page());
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/pager/MeasuredPage.kt: a page's slot is the page's size, whatever its content's
#[test]
fn a_page_smaller_than_its_size_keeps_its_slot() {
    let owned = PagerState::new(0, 0.0, || COUNT);
    let state = *owned;
    std::mem::forget(owned);
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        HorizontalPager!(
            state,
            modifier = Modifier.fill_max_size(),
            page_content = |_, _| {
                Box!(modifier = Modifier.size(100.dp()));
            }
        );
    });
    ui.wait_for_idle();
    let pages: Vec<_> = state.layout_info().visible_pages_info.iter().map(|it| it.index).collect();
    assert_eq!(pages, [0]);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/snapping/PagerSnapLayoutInfoProvider.kt, calculateSnapOffset, with a frame for the layout: a frame's scroll past all the pages laid out lands on a page
#[test]
fn a_frames_scroll_past_all_the_pages_laid_out_lands_on_a_page() {
    let (mut ui, state) = with_fling(100, PageSize::Fixed(Dp(20.0)), |state| fling_of(state, 50));
    let fling = fling_of(state, 50);
    run(&mut ui, async move {
        state
            .scroll(MutatePriority::Default, move |scope| async move {
                fling.perform_fling(scope, 30000.0).await;
            })
            .await
            .expect("free");
    });
    assert!(state.current_page() >= 54, "{}", state.current_page());
    assert_eq!(state.current_page_offset_fraction(), 0.0);
}
