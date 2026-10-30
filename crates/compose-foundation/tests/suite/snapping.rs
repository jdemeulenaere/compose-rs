//! Snapping flings: scenarios of Compose's `SnapFlingBehaviorTest`, `LazyListSnapFlingBehaviorTest`,
//! `LazyGridSnapFlingBehaviorTest` and their `SnapLayoutInfoProvider` tests, rewritten.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_animation::{
    AnimationSpec, AnimationVector, DecayAnimationSpec, FloatDecayAnimationSpec,
    SplineBasedFloatDecayAnimationSpec, VectorizedAnimationSpec, generate_decay_animation_spec,
    spring,
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

use crate::scroll_support::{launch, run};

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

/// A spring, counting the animations run on it: a target-based animation asks its duration once.
struct CountingSpring {
    inner: AnimationSpec,
    durations: Rc<Cell<u32>>,
}

impl VectorizedAnimationSpec for CountingSpring {
    fn value_from_nanos(
        &self,
        time: i64,
        value: AnimationVector,
        target: AnimationVector,
        velocity: AnimationVector,
    ) -> AnimationVector {
        self.inner.value_from_nanos(time, value, target, velocity)
    }
    fn velocity_from_nanos(
        &self,
        time: i64,
        value: AnimationVector,
        target: AnimationVector,
        velocity: AnimationVector,
    ) -> AnimationVector {
        self.inner.velocity_from_nanos(time, value, target, velocity)
    }
    fn duration_nanos(
        &self,
        value: AnimationVector,
        target: AnimationVector,
        velocity: AnimationVector,
    ) -> i64 {
        self.durations.set(self.durations.get() + 1);
        self.inner.duration_nanos(value, target, velocity)
    }
    fn end_velocity(
        &self,
        value: AnimationVector,
        target: AnimationVector,
        velocity: AnimationVector,
    ) -> AnimationVector {
        self.inner.end_velocity(value, target, velocity)
    }
}

fn counting_spring() -> (AnimationSpec, Rc<Cell<u32>>) {
    let durations = Rc::new(Cell::new(0));
    let spec = AnimationSpec::custom(CountingSpring {
        inner: spring().into(),
        durations: durations.clone(),
    });
    (spec, durations)
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
    let (springs, sprung) = counting_spring();
    let reach = decay.calculate_target_value(0.0f32, TEST_VELOCITY);
    let provider = Rc::new(TestProvider { min: MIN_OFFSET, max: 100.0, approach: reach + 1.0 });
    let (told, _) = perform(snap_fling_behavior(provider, decay, springs), TEST_VELOCITY, 1.0);
    assert_eq!((decays.get(), sprung.get()), (0, 2));
    assert_eq!(told[0], reach + 1.0);
    assert!(told.contains(&100.0), "{told:?}");
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/SnapFlingBehaviorTest.kt, approach_canDecay_useHighVelocityApproachAndSnap
#[test]
fn an_approach_the_decay_reaches_decays() {
    let (decay, decays) = counting_decay();
    let (springs, sprung) = counting_spring();
    let reach = decay.calculate_target_value(0.0f32, TEST_VELOCITY);
    let provider = Rc::new(TestProvider { min: MIN_OFFSET, max: 100.0, approach: reach - 1.0 });
    perform(snap_fling_behavior(provider, decay, springs), TEST_VELOCITY, 1.0);
    assert_eq!((decays.get(), sprung.get()), (1, 1));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/SnapFlingBehaviorTest.kt, approach_usedDefaultApproach_useHighVelocityApproachAndSnap, approach_usedDefaultApproach_shouldDecay
#[test]
fn the_default_approach_is_the_decays_target() {
    let (decay, decays) = counting_decay();
    let (springs, sprung) = counting_spring();
    let velocity = 5.0 * TEST_VELOCITY;
    let reach = decay.calculate_target_value(0.0f32, velocity);
    let (told, _) = perform(snap_fling_behavior(provider(f32::NAN), decay, springs), velocity, 1.0);
    assert_eq!((decays.get(), sprung.get()), (1, 1));
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
        let fling: Rc<dyn FlingBehavior> =
            remember_list_snap_fling_behavior(handle, SnapPosition::Center);
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
        let fling: Rc<dyn FlingBehavior> = behavior;
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
        let fling: Rc<dyn FlingBehavior> = behavior;
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

type MakeProvider = Rc<dyn Fn(LazyListState) -> Rc<dyn SnapLayoutInfoProvider>>;

/// A row of 200 items of `item` px in `viewport`, snapping to the center by `provider`, flung by
/// hand; its state.
fn snapping_row(
    viewport: i32,
    item: f32,
    provider: Option<MakeProvider>,
) -> (ComposeTestRule, LazyListState, Rc<SnapFlingBehavior>) {
    let state = LazyListState::new(0, 0);
    let handle = *state;
    std::mem::forget(state);
    let behavior = snap_fling_behavior(
        match provider {
            Some(make) => make(handle),
            None => list_snap_layout_info_provider(handle, SnapPosition::Center, density()),
        },
        compose_animation::spline_based_decay(density()),
        spring(),
    );
    let behavior = Rc::new(behavior);
    let given: Rc<dyn FlingBehavior> = behavior.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(viewport, 300), move || {
        LazyRow!(
            modifier = Modifier.fill_max_size(),
            state = handle,
            fling_behavior = Some(given.clone()),
            content = move |scope: LazyListScope| {
                scope.items(200, move |_| Box!(modifier = Modifier.size(item.dp())));
            }
        );
    });
    ui.wait_for_idle();
    (ui, handle, behavior)
}

/// Flings `behavior` by hand under `ambient`, a frame at a time; the frames it took.
fn fling_under(
    ui: &mut ComposeTestRule,
    state: LazyListState,
    behavior: Rc<SnapFlingBehavior>,
    ambient: f32,
    velocity: f32,
) -> u32 {
    ui.set_auto_advance(false);
    let job = launch(async move {
        let _ = state
            .scroll(MutatePriority::Default, move |scope| async move {
                compose_ui::with_motion_duration_scale(
                    ambient,
                    behavior.perform_fling(scope, velocity),
                )
                .await;
            })
            .await;
    });
    let mut frames = 0;
    while job.is_active() {
        ui.advance_time_by_frame();
        frames += 1;
        assert!(frames < 5000, "it ended");
    }
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    frames
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/SnapFlingBehaviorTest.kt, disableSystemAnimations_defaultFlingBehaviorShouldContinueToWork
#[test]
fn a_snap_runs_the_same_whatever_the_scale_around() {
    let mut landed = Vec::new();
    for ambient in [1.0, 0.0, 5.0] {
        let (mut ui, state, behavior) = snapping_row(600, 20.0, None);
        let frames = fling_under(&mut ui, state, behavior, ambient, 10000.0);
        landed.push((
            frames,
            state.first_visible_item_index(),
            state.first_visible_item_scroll_offset(),
        ));
    }
    assert_eq!(landed[0], landed[1], "at 0");
    assert_eq!(landed[0], landed[2], "at 5");
    assert!(landed[0].0 > 1 && landed[0].1 > 0, "{:?}", landed[0]);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/SnapFlingBehaviorTest.kt, defaultFlingBehavior_useScrollMotionDurationScale
#[test]
fn a_snap_runs_at_a_scale_of_its_own() {
    let (mut ui, state, behavior) = snapping_row(600, 150.0, None);
    behavior.set_motion_scale(0.0);
    let frames = fling_under(&mut ui, state, behavior.clone(), 1.0, 10000.0);
    assert!(frames <= 3, "at once: {frames}");
    assert!(state.first_visible_item_index() > 0);
    let before = state.first_visible_item_index();
    behavior.set_motion_scale(1.0);
    let frames = fling_under(&mut ui, state, behavior, 1.0, 10000.0);
    assert!(frames > 3, "as long as it takes: {frames}");
    assert_ne!(state.first_visible_item_index(), before);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/SnapFlingBehaviorTest.kt, performFling_cancellationShouldTriggerAnimationCancellation
#[test]
fn a_fling_cancelled_in_its_first_delta_runs_no_decay() {
    let (decay, decays) = counting_decay();
    let behavior = snap_fling_behavior(provider(0.0), decay, spring());
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), || {});
    let scrolled = Rc::new(Cell::new(0));
    let noted = scrolled.clone();
    let job = launch(async move {
        let scope = ScrollScope::new(move |_| {
            noted.set(noted.get() + 1);
            compose_runtime::platform::cancel_current_task();
            0.0
        });
        behavior.perform_fling(scope, 3000.0).await;
    });
    ui.wait_for_idle();
    assert!(!job.is_active());
    assert_eq!(decays.get(), 0);
    assert_eq!(scrolled.get(), 1, "not on");
}

/// Flings under a provider whose approach is `approach`, and asks the rest of the list's.
struct Approach {
    inner: Rc<dyn SnapLayoutInfoProvider>,
    approach: f32,
}

impl SnapLayoutInfoProvider for Approach {
    fn calculate_approach_offset(&self, _velocity: f32, _decay_offset: f32) -> f32 {
        self.approach
    }
    fn calculate_snap_offset(&self, velocity: f32) -> f32 {
        self.inner.calculate_snap_offset(velocity)
    }
    fn is_layout_behind(&self) -> bool {
        self.inner.is_layout_behind()
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/snapping/SnapFlingBehavior.kt: fling, with `applyMeasureResult`'s remeasure in the scroll: a scroll past what's laid out is laid out before the snap asks
#[test]
fn a_frames_scroll_past_all_thats_laid_out_still_lands_on_an_item() {
    let make: MakeProvider = Rc::new(|state| {
        Rc::new(Approach {
            inner: list_snap_layout_info_provider(state, SnapPosition::Center, density()),
            approach: 310.0,
        })
    });
    let (mut ui, state, behavior) = snapping_row(100, 20.0, Some(make));
    fling_under(&mut ui, state, behavior, 1.0, 30000.0);
    assert_eq!(state.first_visible_item_scroll_offset() % 20, 0, "on an item");
    assert!(state.first_visible_item_index() >= 15);
}

/// Scrolls 180 px with `end`, then lands; where.
fn drag_and(end: impl FnOnce(&mut TouchInjectionScope)) -> i32 {
    let (mut snapping, state) = snapping_list(true, 0);
    snapping.ui.wait_for_idle();
    let moved = Offset::new(0.0, -180.0);
    snapping.ui.on_root().perform_touch_input(|touch| {
        touch.down(Offset::new(300.0, 400.0));
        touch.move_by(moved);
        end(touch);
    });
    snapping.ui.wait_for_idle();
    state.first_visible_item_index() as i32 * 200 + state.first_visible_item_scroll_offset()
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/DragGestureNode.kt, processDragCancel: onDragStopped(Zero); Scrollable.kt, onDragStopped
#[test]
fn a_cancelled_drag_snaps_as_a_release_does() {
    let released = drag_and(|touch| {
        touch.advance_event_time(3000);
        touch.up();
    });
    assert_eq!(released % 200, 0, "{released}");
    assert_eq!(drag_and(|touch| touch.cancel()), released);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/snapping/SnapFlingBehavior.kt: animateWithTarget, coerceToTarget(animationState.velocity, initialVelocity)
#[test]
fn a_snap_stopped_short_leaves_no_more_velocity_than_it_got() {
    let provider = Rc::new(TestProvider { min: MIN_OFFSET, max: MAX_OFFSET, approach: 0.0 });
    let (_, left) = perform(fling_of(provider), 10.0, 0.5);
    assert_eq!(left, 10.0, "the spring is faster by then");
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/snapping/SnapFlingBehavior.kt: fling, onRemainingScrollOffsetUpdate(0f) at its end
#[test]
fn what_remains_is_told_as_none_at_the_end_even_stopped_short() {
    let provider = Rc::new(TestProvider { min: MIN_OFFSET, max: MAX_OFFSET, approach: 0.0 });
    let (told, _) = perform(fling_of(provider), TEST_VELOCITY, 0.5);
    assert_ne!(told[told.len() - 2], 0.0, "stopped short: {told:?}");
    assert_eq!(told.last(), Some(&0.0));
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/snapping/LazyGridSnapLayoutInfoProvider.kt, with the grid's remeasure in the scroll: a scroll past what's laid out is laid out before the snap asks
#[test]
fn a_frames_scroll_past_all_thats_laid_out_of_a_grid_still_lands_on_a_line() {
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    std::mem::forget(state);
    let behavior = Rc::new(snap_fling_behavior(
        Rc::new(Approach {
            inner: grid_snap_layout_info_provider(handle, SnapPosition::Center, density()),
            approach: 310.0,
        }),
        compose_animation::spline_based_decay(density()),
        spring(),
    ));
    let given: Rc<dyn FlingBehavior> = behavior.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        LazyHorizontalGrid!(
            GridCells::Fixed(1),
            modifier = Modifier.fill_max_size(),
            state = handle,
            fling_behavior = Some(given.clone()),
            content = |scope: LazyGridScope| {
                scope.items(200, |_| Box!(modifier = Modifier.size(20.dp())));
            }
        );
    });
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    let job = launch(async move {
        let _ = handle
            .scroll(MutatePriority::Default, move |scope| async move {
                behavior.perform_fling(scope, 30000.0).await;
            })
            .await;
    });
    let mut frames = 0;
    while job.is_active() {
        ui.advance_time_by_frame();
        frames += 1;
        assert!(frames < 5000, "it ended");
    }
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!(handle.first_visible_item_scroll_offset() % 20, 0, "on a line");
    assert!(handle.first_visible_item_index() >= 15);
}

/// Notes the frame it's asked where to land in.
struct Asked {
    frame: Rc<Cell<u32>>,
    asked_in: Rc<Cell<u32>>,
}

impl SnapLayoutInfoProvider for Asked {
    fn calculate_approach_offset(&self, _velocity: f32, _decay_offset: f32) -> f32 {
        100.0
    }
    fn calculate_snap_offset(&self, _velocity: f32) -> f32 {
        self.asked_in.set(self.frame.get());
        0.0
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/snapping/SnapFlingBehavior.kt: fling, nothing between the approach and calculateSnapOffset
#[test]
fn with_the_layout_up_to_date_a_snap_asks_where_to_land_in_the_approachs_last_frame() {
    let (frame, asked_in) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let behavior = fling_of(Rc::new(Asked { frame: frame.clone(), asked_in: asked_in.clone() }));
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), || {});
    ui.set_auto_advance(false);
    let (last_delta, noted, now) =
        (Rc::new(Cell::new(0)), Rc::new(Cell::new(0.0f32)), frame.clone());
    let (approach_ended, summed) = (last_delta.clone(), noted.clone());
    let state = scrollable_state(move |delta| {
        // The approach's last delta: it's used up by it.
        if summed.get() < 100.0 {
            approach_ended.set(now.get());
        }
        summed.set(summed.get() + delta);
        delta
    });
    let job = launch(async move {
        let _ = state
            .scroll(MutatePriority::Default, move |scope| async move {
                behavior.perform_fling(scope, 5000.0).await;
            })
            .await;
    });
    while job.is_active() {
        frame.set(frame.get() + 1);
        ui.advance_time_by_frame();
        assert!(frame.get() < 1000);
    }
    assert!(last_delta.get() > 1, "an approach over frames");
    assert_eq!(asked_in.get(), last_delta.get());
}
