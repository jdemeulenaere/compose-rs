//! Snapping flings: scenarios of Compose's `SnapFlingBehaviorTest`, `LazyListSnapFlingBehaviorTest`,
//! `LazyGridSnapFlingBehaviorTest` and their `SnapLayoutInfoProvider` tests, rewritten.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_animation::{
    DecayAnimationSpec, FloatDecayAnimationSpec, SplineBasedFloatDecayAnimationSpec,
    generate_decay_animation_spec, spring,
};
use compose_foundation::gestures::{
    FlingBehavior, LocalBoxFuture, MIN_FLING_VELOCITY, SnapFlingBehavior, SnapLayoutInfoProvider,
    SnapPosition, TargetedFlingBehavior, grid_snap_layout_info_provider,
    list_snap_layout_info_provider, remember_grid_snap_fling_behavior,
    remember_list_snap_fling_behavior, snap_fling_behavior,
};
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_test::*;
use compose_ui::{Density, IntSize, NestedScrollConnection, Offset, PointerEvent, UnitExt};

use crate::scroll_support::run;

const MIN_OFFSET: f32 = -200.0;
const MAX_OFFSET: f32 = 300.0;
const TEST_VELOCITY: f32 = 1000.0;
const ITEM: f32 = 200.0;
fn density() -> Density {
    Density::new(1.0)
}

/// Lands `min` back or `max` on by the velocity's way, its approach what's given (`NaN`: the decay).
struct TestProvider {
    min: f32,
    max: f32,
    approach: f32,
}

fn provider(approach: f32) -> Rc<TestProvider> {
    Rc::new(TestProvider { min: MIN_OFFSET, max: MAX_OFFSET, approach })
}

impl SnapLayoutInfoProvider for TestProvider {
    fn calculate_approach_offset(&self, _velocity: f32, decay_offset: f32) -> f32 {
        if self.approach.is_nan() { decay_offset } else { self.approach }
    }
    fn calculate_snap_offset(&self, velocity: f32) -> f32 {
        if velocity == 0.0 {
            if self.max.abs() <= self.min.abs() { self.max } else { self.min }
        } else if velocity > 0.0 {
            self.max
        } else {
            self.min
        }
    }
}

/// The spline's decay, counting the animations run on it.
struct CountingDecay {
    inner: SplineBasedFloatDecayAnimationSpec,
    durations: Rc<Cell<u32>>,
}

impl FloatDecayAnimationSpec for CountingDecay {
    fn abs_velocity_threshold(&self) -> f32 {
        self.inner.abs_velocity_threshold()
    }
    fn value_from_nanos(&self, time: i64, value: f32, velocity: f32) -> f32 {
        self.inner.value_from_nanos(time, value, velocity)
    }
    fn duration_nanos(&self, value: f32, velocity: f32) -> i64 {
        self.durations.set(self.durations.get() + 1);
        self.inner.duration_nanos(value, velocity)
    }
    fn velocity_from_nanos(&self, time: i64, value: f32, velocity: f32) -> f32 {
        self.inner.velocity_from_nanos(time, value, velocity)
    }
    fn target_value(&self, value: f32, velocity: f32) -> f32 {
        self.inner.target_value(value, velocity)
    }
}

fn counting_decay() -> (DecayAnimationSpec, Rc<Cell<u32>>) {
    let durations = Rc::new(Cell::new(0));
    let spec = generate_decay_animation_spec(CountingDecay {
        inner: SplineBasedFloatDecayAnimationSpec::new(density()),
        durations: durations.clone(),
    });
    (spec, durations)
}

fn fling_of(provider: Rc<dyn SnapLayoutInfoProvider>) -> SnapFlingBehavior {
    snap_fling_behavior(provider, compose_animation::spline_based_decay(density()), spring())
}

/// Flings `behavior` at `velocity` on a state consuming `fraction`; what's told of the remaining
/// distance and what's left of the velocity.
fn perform(behavior: SnapFlingBehavior, velocity: f32, fraction: f32) -> (Vec<f32>, f32) {
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), || {});
    let state = scrollable_state(move |delta| delta * fraction);
    let told: Rc<RefCell<Vec<f32>>> = Rc::default();
    let noted = told.clone();
    let left = run(&mut ui, async move {
        let answer = Rc::new(Cell::new(0.0));
        let given = answer.clone();
        let _ = state
            .scroll(MutatePriority::Default, move |scope| async move {
                let on_remaining: Rc<dyn Fn(f32)> =
                    Rc::new(move |left| noted.borrow_mut().push(left));
                given.set(behavior.perform_targeted_fling(scope, velocity, on_remaining).await);
            })
            .await;
        answer.get()
    });
    let told = told.borrow().clone();
    (told, left)
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/SnapFlingBehaviorTest.kt, remainingScrollOffset_cannotApproach_shouldRepresentJustSnappingOffsets
#[test]
fn with_no_approach_what_remains_is_the_snap() {
    let (told, _) = perform(fling_of(provider(0.0)), 0.0, 1.0);
    assert_eq!(told[0], 0.0);
    assert_eq!(told[1], MIN_OFFSET);
    assert_eq!(*told.last().unwrap(), 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/SnapFlingBehaviorTest.kt, remainingScrollOffset_canApproach_shouldRepresentApproachAndSnapOffsets, remainingScrollOffset_targetShouldChangeInAccordanceWithAnimation
#[test]
fn with_an_approach_what_remains_is_it_then_the_snap() {
    let (told, _) = perform(fling_of(provider(50.0)), TEST_VELOCITY, 1.0);
    assert_eq!(told[0], 50.0);
    assert!(told.contains(&MAX_OFFSET), "{told:?}");
    assert_eq!(*told.last().unwrap(), 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/SnapFlingBehaviorTest.kt, performFling_afterSnappingVelocity_everythingWasConsumed_shouldReturnNoVelocity
#[test]
fn all_consumed_no_velocity_is_left() {
    let (_, left) = perform(fling_of(provider(0.0)), 5000.0, 1.0);
    assert_eq!(left, 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/SnapFlingBehaviorTest.kt, performFling_afterSnappingVelocity_didNotConsumeAllScroll_shouldReturnRemainingVelocity
#[test]
fn half_consumed_velocity_is_left() {
    let (_, left) = perform(fling_of(provider(0.0)), 5000.0, 0.5);
    assert_ne!(left, 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/SnapFlingBehaviorTest.kt, performFling_invalidOffsets_shouldNotPropagateNans_calculateSnapOffset
#[test]
#[should_panic(expected = "calculate_snap_offset returned NaN")]
fn a_snap_offset_of_nan_is_refused() {
    struct Nan;
    impl SnapLayoutInfoProvider for Nan {
        fn calculate_snap_offset(&self, _: f32) -> f32 {
            f32::NAN
        }
    }
    perform(fling_of(Rc::new(Nan)), TEST_VELOCITY, 1.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/SnapFlingBehaviorTest.kt, performFling_invalidOffsets_shouldNotPropagateNans_calculateApproachOffset
#[test]
#[should_panic(expected = "calculate_approach_offset returned NaN")]
fn an_approach_offset_of_nan_is_refused() {
    struct Nan;
    impl SnapLayoutInfoProvider for Nan {
        fn calculate_approach_offset(&self, _: f32, _: f32) -> f32 {
            f32::NAN
        }
        fn calculate_snap_offset(&self, _: f32) -> f32 {
            0.0
        }
    }
    perform(fling_of(Rc::new(Nan)), TEST_VELOCITY, 1.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/SnapFlingBehaviorTest.kt, approach_cannotDecay_useLowVelocityApproachAndSnap, approach_cannotDecay_shouldJustSnapToBound
#[test]
fn an_approach_the_decay_cant_reach_animates() {
    let (decay, decays) = counting_decay();
    let reach = decay.calculate_target_value(0.0f32, TEST_VELOCITY);
    let provider = Rc::new(TestProvider { min: MIN_OFFSET, max: 100.0, approach: reach + 1.0 });
    let (told, _) = perform(snap_fling_behavior(provider, decay, spring()), TEST_VELOCITY, 1.0);
    assert_eq!(decays.get(), 0);
    assert_eq!(told[0], reach + 1.0);
    assert!(told.contains(&100.0), "{told:?}");
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/SnapFlingBehaviorTest.kt, approach_canDecay_useHighVelocityApproachAndSnap
#[test]
fn an_approach_the_decay_reaches_decays() {
    let (decay, decays) = counting_decay();
    let reach = decay.calculate_target_value(0.0f32, TEST_VELOCITY);
    let provider = Rc::new(TestProvider { min: MIN_OFFSET, max: 100.0, approach: reach - 1.0 });
    perform(snap_fling_behavior(provider, decay, spring()), TEST_VELOCITY, 1.0);
    assert!(decays.get() >= 1);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/SnapFlingBehaviorTest.kt, approach_usedDefaultApproach_useHighVelocityApproachAndSnap, approach_usedDefaultApproach_shouldDecay
#[test]
fn the_default_approach_is_the_decays_target() {
    let (decay, decays) = counting_decay();
    let velocity = 5.0 * TEST_VELOCITY;
    let reach = decay.calculate_target_value(0.0f32, velocity);
    let (told, _) =
        perform(snap_fling_behavior(provider(f32::NAN), decay, spring()), velocity, 1.0);
    assert!(decays.get() >= 1);
    assert!((told[0] - reach).abs() < 0.1, "{} {reach}", told[0]);
}

/// A list or a grid of 200 px items in a 600 px window, `orientation`, snapping to the center.
struct Snapping {
    ui: ComposeTestRule,
    vertical: bool,
    first: Box<dyn Fn() -> Vec<(usize, i32, i32)>>,
    viewport: i32,
}

impl Snapping {
    fn snapped(&self) -> usize {
        let items = (self.first)();
        let center = self.viewport / 2 - ITEM as i32 / 2;
        items
            .iter()
            .min_by_key(|(_, offset, _)| (offset - center).abs())
            .map(|(index, ..)| *index)
            .expect("items in view")
    }

    /// As `swipeMainAxisWithVelocity`: from the end of the viewport, backward if `by` is negative.
    fn swipe(&mut self, by: f32, velocity: f32) {
        let end = self.viewport as f32 - 1.0;
        let (from, to) = if by >= 0.0 { (end, end - by) } else { (end + by, end) };
        let at = |main: f32, vertical: bool| {
            if vertical { Offset::new(300.0, main) } else { Offset::new(main, 300.0) }
        };
        let vertical = self.vertical;
        self.ui.on_root().perform_touch_input(|touch| {
            touch.swipe_with_velocity(at(from, vertical), at(to, vertical), velocity, None);
        });
        self.ui.wait_for_idle();
    }

    fn scroll_a_bit(&mut self) {
        let vertical = self.vertical;
        self.ui.on_root().perform_touch_input(|touch| {
            if vertical { touch.swipe_up() } else { touch.swipe_left() }
        });
        self.ui.wait_for_idle();
    }
}

fn snapping_list(vertical: bool, first: usize) -> (Snapping, LazyListState) {
    let state = LazyListState::new(first, 0);
    let handle = *state;
    let ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
        let behavior = remember_list_snap_fling_behavior(handle, SnapPosition::Center);
        let fling: Rc<dyn FlingBehavior> = Rc::new(behavior);
        let content = |scope: LazyListScope| {
            scope.items(200, |_| Box!(modifier = Modifier.size(200.dp())));
        };
        if vertical {
            LazyColumn!(
                modifier = Modifier.fill_max_size(),
                state = handle,
                fling_behavior = Some(fling),
                content = content
            );
        } else {
            LazyRow!(
                modifier = Modifier.fill_max_size(),
                state = handle,
                fling_behavior = Some(fling),
                content = content
            );
        }
    });
    let items = Box::new(move || {
        handle
            .layout_info()
            .visible_items_info
            .iter()
            .map(|item| (item.index, item.offset, item.size))
            .collect()
    });
    std::mem::forget(state);
    (Snapping { ui, vertical, first: items, viewport: 600 }, handle)
}

fn snapping_grid(vertical: bool, first: usize) -> (Snapping, LazyGridState) {
    let state = LazyGridState::new(first, 0);
    let handle = *state;
    let ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
        let behavior = remember_grid_snap_fling_behavior(handle, SnapPosition::Center);
        let fling: Rc<dyn FlingBehavior> = Rc::new(behavior);
        let cells = GridCells::FixedSize(200.dp());
        let content = |scope: LazyGridScope| {
            scope.items(200, |_| Box!(modifier = Modifier.size(200.dp())));
        };
        if vertical {
            LazyVerticalGrid!(
                cells,
                modifier = Modifier.fill_max_size(),
                state = handle,
                fling_behavior = Some(fling),
                content = content
            );
        } else {
            LazyHorizontalGrid!(
                cells,
                modifier = Modifier.fill_max_size(),
                state = handle,
                fling_behavior = Some(fling),
                content = content
            );
        }
    });
    let items = Box::new(move || {
        handle
            .layout_info()
            .visible_items_info
            .iter()
            .map(|item| {
                let (offset, size) = if vertical {
                    (item.offset.y, item.size.height)
                } else {
                    (item.offset.x, item.size.width)
                };
                (item.index, offset, size)
            })
            .collect()
    });
    std::mem::forget(state);
    (Snapping { ui, vertical, first: items, viewport: 600 }, handle)
}

fn threshold() -> f32 {
    density().to_px(MIN_FLING_VELOCITY)
}

fn each_list(check: impl Fn(&mut Snapping, usize)) {
    for vertical in [true, false] {
        let (mut snapping, _) = snapping_list(vertical, 0);
        snapping.ui.wait_for_idle();
        snapping.scroll_a_bit();
        let current = snapping.snapped();
        check(&mut snapping, current);
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/LazyListSnapFlingBehaviorTest.kt, belowThresholdVelocity_lessThanAnItemScroll_shouldStayInSamePage
#[test]
fn a_slow_short_swipe_stays_on_the_item() {
    each_list(|snapping, current| {
        snapping.swipe(ITEM / 2.0, threshold() / 2.0);
        assert_eq!(snapping.snapped(), current);
    });
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/LazyListSnapFlingBehaviorTest.kt, belowThresholdVelocity_moreThanAnItemScroll_shouldGoToNextPage
#[test]
fn a_slow_long_swipe_goes_to_the_next() {
    each_list(|snapping, current| {
        snapping.swipe(ITEM, threshold() / 2.0);
        assert_eq!(snapping.snapped(), current + 1);
    });
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/LazyListSnapFlingBehaviorTest.kt, aboveThresholdVelocityForward_notLargeEnoughScroll_shouldGoToNextPage
#[test]
fn a_fast_short_swipe_goes_to_the_next() {
    each_list(|snapping, current| {
        snapping.swipe(ITEM / 2.0, threshold() * 2.0);
        assert_eq!(snapping.snapped(), current + 1);
    });
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/LazyListSnapFlingBehaviorTest.kt, aboveThresholdVelocityBackward_notLargeEnoughScroll_shouldGoToPreviousPage
#[test]
fn a_fast_short_swipe_back_goes_to_the_previous() {
    each_list(|snapping, current| {
        snapping.swipe(-ITEM / 2.0, threshold() * 2.0);
        assert_eq!(snapping.snapped(), current - 1);
    });
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/LazyListSnapFlingBehaviorTest.kt, aboveThresholdVelocity_largeEnoughScroll_shouldGoToNextNextPage
#[test]
fn a_fast_long_swipe_goes_two_on() {
    each_list(|snapping, current| {
        snapping.swipe(1.5 * ITEM, threshold() * 3.0);
        assert_eq!(snapping.snapped(), current + 2);
    });
}

fn each_grid(check: impl Fn(&mut Snapping, usize)) {
    for vertical in [true, false] {
        let (mut snapping, _) = snapping_grid(vertical, 0);
        snapping.ui.wait_for_idle();
        snapping.scroll_a_bit();
        let current = snapping.snapped();
        check(&mut snapping, current);
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/LazyGridSnapFlingBehaviorTest.kt, belowThresholdVelocity_lessThanAnItemScroll_shouldStayInSamePage, belowThresholdVelocity_moreThanAnItemScroll_shouldGoToNextPage, aboveThresholdVelocityForward_notLargeEnoughScroll_shouldGoToNextPage, aboveThresholdVelocityBackward_notLargeEnoughScroll_shouldGoToPreviousPage, aboveThresholdVelocity_largeEnoughScroll_shouldGoToNextNextPage
#[test]
fn a_grid_snaps_as_a_list() {
    let lines = 3; // Fixed-size cells: three a line, across 600.
    each_grid(|snapping, current| {
        snapping.swipe(ITEM / 2.0, threshold() / 2.0);
        assert_eq!(snapping.snapped() / lines, current / lines);
    });
    each_grid(|snapping, current| {
        snapping.swipe(ITEM, threshold() / 2.0);
        assert_eq!(snapping.snapped() / lines, current / lines + 1);
    });
    each_grid(|snapping, current| {
        snapping.swipe(ITEM / 2.0, threshold() * 2.0);
        assert_eq!(snapping.snapped() / lines, current / lines + 1);
    });
    each_grid(|snapping, current| {
        snapping.swipe(-ITEM / 2.0, threshold() * 2.0);
        assert_eq!(snapping.snapped() / lines, current / lines - 1);
    });
    each_grid(|snapping, current| {
        snapping.swipe(1.5 * ITEM, threshold() * 3.0);
        assert_eq!(snapping.snapped() / lines, current / lines + 2);
    });
}

/// Tells the velocity a post fling is left.
struct LeftVelocity(Rc<Cell<f32>>);

impl NestedScrollConnection for LeftVelocity {
    fn on_post_fling(&self, _: Offset, available: Offset) -> LocalBoxFuture<'_, Offset> {
        Box::pin(async move {
            self.0.set(available.x.abs().max(available.y.abs()));
            Offset::ZERO
        })
    }
}

/// In 650 px, for an item at the end not to be at the center by chance.
fn nested_list(first: usize, left: Rc<Cell<f32>>) -> (ComposeTestRule, LazyListState) {
    let state = LazyListState::new(first, 0);
    let handle = *state;
    let connection: Rc<dyn NestedScrollConnection> = Rc::new(LeftVelocity(left));
    let ui = ComposeTestRule::sized(IntSize::new(650, 650), move || {
        let behavior = remember_list_snap_fling_behavior(handle, SnapPosition::Center);
        let fling: Rc<dyn FlingBehavior> = Rc::new(behavior);
        Box!(modifier = Modifier.fill_max_size().nested_scroll(connection.clone()), {
            let fling = fling.clone();
            LazyColumn!(
                modifier = Modifier.fill_max_size(),
                state = handle,
                fling_behavior = Some(fling),
                |scope: LazyListScope| {
                    scope.items(200, |_| Box!(modifier = Modifier.size(200.dp())));
                }
            );
        });
    });
    std::mem::forget(state);
    (ui, handle)
}

fn swipe_column(ui: &mut ComposeTestRule, by: f32, velocity: f32) {
    let (from, to) = if by >= 0.0 { (599.0, 599.0 - by) } else { (599.0 + by, 599.0) };
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe_with_velocity(Offset::new(300.0, from), Offset::new(300.0, to), velocity, None);
    });
    ui.wait_for_idle();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/LazyListSnapFlingBehaviorTest.kt, performFling_shouldPropagateVelocityIfHitEdges
#[test]
fn a_fling_hitting_an_edge_leaves_velocity_to_the_parents() {
    let left = Rc::new(Cell::new(0.0f32));
    let (mut ui, handle) = nested_list(180, left.clone());
    ui.wait_for_idle();
    swipe_column(&mut ui, 1.5 * ITEM, 15000.0);
    assert_ne!(left.get(), 0.0);
    let scrolling = handle;
    run(&mut ui, async move {
        let _ = scrolling.scroll_to_item(10, 0).await;
    });
    left.set(0.0);
    swipe_column(&mut ui, -1.5 * ITEM, 15000.0);
    assert_ne!(left.get(), 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/LazyListSnapFlingBehaviorTest.kt, performFling_shouldConsumeAllVelocityIfInTheMiddleOfTheList
#[test]
fn a_fling_in_the_middle_uses_all_its_velocity() {
    let left = Rc::new(Cell::new(1.0f32));
    let (mut ui, handle) = nested_list(100, left.clone());
    ui.wait_for_idle();
    swipe_column(&mut ui, 1.5 * ITEM, 10000.0);
    assert_eq!(left.get(), 0.0);
    let scrolling = handle;
    run(&mut ui, async move {
        let _ = scrolling.scroll_to_item(100, 0).await;
    });
    left.set(1.0);
    swipe_column(&mut ui, -1.5 * ITEM, 10000.0);
    assert_eq!(left.get(), 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/LazyListSnapFlingBehaviorTest.kt, performFling_mouseWheelShouldSnapToItem
#[test]
fn a_wheels_turn_snaps_too() {
    let (mut snapping, _) = snapping_list(true, 0);
    snapping.ui.wait_for_idle();
    snapping.scroll_a_bit();
    let current = snapping.snapped();
    snapping.ui.host().send_pointer(
        PointerEvent::scroll(Offset::new(300.0, 300.0), Offset::new(0.0, ITEM * 0.8))
            .with_precise_wheel(true),
    );
    snapping.ui.wait_for_idle();
    assert_eq!(snapping.snapped(), current + 1);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/LazyListSnapLayoutInfoProviderTest.kt, calculateSnappingOffset_velocityIsZero_shouldReturnClosestItemOffset, calculateSnappingOffset_velocityPositive_moreThanMinThreshold_shouldReturnNextItemOffset, calculateSnappingOffset_velocityNegative_moreThanMinThreshold_shouldReturnPreviousItemOffset
#[test]
fn a_lists_provider_gives_the_offset_of_the_item_by_velocity() {
    for (vertical, header) in [(true, true), (true, false), (false, true), (false, false)] {
        let state = LazyListState::new(100, 0);
        let handle = *state;
        let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
            let content = move |scope: LazyListScope| {
                if header {
                    scope.sticky_header(|_| Box!(modifier = Modifier.size(400.dp())));
                }
                scope.items(200, |_| Box!(modifier = Modifier.size(200.dp())));
            };
            if vertical {
                LazyColumn!(modifier = Modifier.fill_max_size(), state = handle, content = content);
            } else {
                LazyRow!(modifier = Modifier.fill_max_size(), state = handle, content = content);
            }
        });
        ui.wait_for_idle();
        let scrolling = handle;
        // Out of its snap.
        run(&mut ui, async move {
            let _ = scrolling.scroll_by(-ITEM * 0.2).await;
        });
        let provider = list_snap_layout_info_provider(handle, SnapPosition::Start, density());
        let info = handle.layout_info();
        let offset_of = |index: usize| {
            info.visible_items_info
                .iter()
                .find(|item| item.index == index && !item.non_scrollable)
                .map_or(0, |item| item.offset)
        };
        let first = handle.first_visible_item_index();
        let velocity = threshold() * 2.0;
        assert_eq!(
            provider.calculate_snap_offset(0.0).round() as i32,
            offset_of(first + 1),
            "closest, header {header}"
        );
        assert_eq!(provider.calculate_snap_offset(velocity).round() as i32, offset_of(first + 1));
        assert_eq!(provider.calculate_snap_offset(-velocity).round() as i32, offset_of(first));
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/LazyGridSnapLayoutInfoProviderTest.kt, calculateSnappingOffset_velocityIsZero_shouldReturnClosestItemOffset, calculateSnappingOffset_velocityPositive_moreThanMinThreshold_shouldReturnNextItemOffset, calculateSnappingOffset_velocityNegative_moreThanMinThreshold_shouldReturnPreviousItemOffset
#[test]
fn a_grids_provider_gives_the_offset_of_the_line_by_velocity() {
    for vertical in [true, false] {
        let state = LazyGridState::new(99, 0);
        let handle = *state;
        let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
            let cells = GridCells::Fixed(3);
            let content = |scope: LazyGridScope| {
                scope.items(600, |_| Box!(modifier = Modifier.size(200.dp())));
            };
            if vertical {
                LazyVerticalGrid!(
                    cells,
                    modifier = Modifier.fill_max_size(),
                    state = handle,
                    content = content
                );
            } else {
                LazyHorizontalGrid!(
                    cells,
                    modifier = Modifier.fill_max_size(),
                    state = handle,
                    content = content
                );
            }
        });
        ui.wait_for_idle();
        let scrolling = handle;
        run(&mut ui, async move {
            let _ = scrolling.scroll_by(-ITEM * 0.2).await;
        });
        let provider = grid_snap_layout_info_provider(handle, SnapPosition::Start, density());
        let info = handle.layout_info();
        let offset_of = |index: usize| {
            info.visible_items_info
                .iter()
                .find(|item| item.index == index)
                .map_or(0, |item| if vertical { item.offset.y } else { item.offset.x })
        };
        let first = handle.first_visible_item_index();
        let velocity = threshold() * 2.0;
        assert_eq!(provider.calculate_snap_offset(0.0).round() as i32, offset_of(first + 3));
        assert_eq!(provider.calculate_snap_offset(velocity).round() as i32, offset_of(first + 3));
        assert_eq!(provider.calculate_snap_offset(-velocity).round() as i32, offset_of(first));
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/snapping/SnapPosition.kt: Center, Start, End
#[test]
fn a_snap_position_is_where_in_the_room_between_the_paddings() {
    let custom = SnapPosition::Custom(Rc::new(|layout, item, before, after, index, count| {
        layout + item + before + after + index as i32 + count as i32
    }));
    let cases = [
        (SnapPosition::Center, 340),
        (SnapPosition::Start, 0),
        (SnapPosition::End, 680),
        (custom, 1000 + 200 + 70 + 50 + 3 + 9),
    ];
    for (position, expected) in cases {
        assert_eq!(position.position(1000, 200, 70, 50, 3, 9), expected);
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/snapping/SnapFlingBehavior.kt: fling, abs(initialOffset) * sign(initialVelocity)
#[test]
fn the_approach_goes_the_way_of_the_velocity() {
    let (told, _) = perform(fling_of(provider(50.0)), -TEST_VELOCITY, 1.0);
    assert_eq!(told[0], -50.0);
}

/// A list of 200 px items in 600 px, `scrolled` from its start, or empty.
fn scrolled_list(items: usize, scrolled: f32) -> (ComposeTestRule, LazyListState) {
    let state = LazyListState::new(0, 0);
    let handle = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
        LazyColumn!(
            modifier = Modifier.fill_max_size(),
            state = handle,
            move |scope: LazyListScope| {
                scope.items(items, |_| Box!(modifier = Modifier.size(200.dp())));
            }
        );
    });
    ui.wait_for_idle();
    if scrolled != 0.0 {
        run(&mut ui, async move {
            let _ = handle.scroll_by(scrolled).await;
        });
    }
    std::mem::forget(state);
    (ui, handle)
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/snapping/SnapFlingBehavior.kt: calculateFinalOffset, abs(upperBound) <= abs(lowerBound)
#[test]
fn halfway_between_two_items_the_closest_is_the_next() {
    let (_ui, handle) = scrolled_list(40, ITEM / 2.0);
    let provider = list_snap_layout_info_provider(handle, SnapPosition::Start, density());
    assert_eq!(provider.calculate_snap_offset(0.0), ITEM / 2.0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/snapping/SnapFlingBehavior.kt: calculateFinalOffset, isValidDistance
#[test]
fn with_no_item_to_land_on_there_is_nothing_to_go() {
    let (_ui, handle) = scrolled_list(0, 0.0);
    let provider = list_snap_layout_info_provider(handle, SnapPosition::Start, density());
    assert_eq!(provider.calculate_snap_offset(threshold() * 2.0), 0.0);
    assert_eq!(provider.calculate_snap_offset(-threshold() * 2.0), 0.0);
}
