//! Fling behaviors that land on a position, as Compose's `gestures.snapping`: a
//! [`SnapLayoutInfoProvider`] says how far a fling goes before it lands (the approach, a decay if
//! the velocity gets there, else an animation to it) and where it lands (the snap, always an
//! animation, with what the approach left of the velocity).

use std::cell::Cell;
use std::rc::Rc;

use compose_animation::{
    Animate, AnimationScope, AnimationSpec, AnimationState, DecayAnimationSpec, Spring,
    spline_based_decay, spring_with,
};
use compose_runtime::composable;
use compose_ui::{Density, Dp};

use super::{FlingBehavior, LocalBoxFuture, Orientation, ScrollScope};
use crate::lazy::{LazyGridState, LazyListState};

/// The velocity past which a fling goes to the next item, not the closest, as `MinFlingVelocityDp`.
pub const MIN_FLING_VELOCITY: Dp = Dp(400.0);

/// A fling behavior telling how far it's left to go, as `TargetedFlingBehavior`.
pub trait TargetedFlingBehavior: FlingBehavior {
    /// `perform_fling`, telling `on_remaining_distance_updated` what's left at each step (the
    /// approach's, then the snap's, 0 at the end, however it ended).
    fn perform_targeted_fling<'a>(
        &'a self,
        scope: ScrollScope,
        initial_velocity: f32,
        on_remaining_distance_updated: Rc<dyn Fn(f32)>,
    ) -> LocalBoxFuture<'a, f32>;
}

/// Where a fling goes, as `SnapLayoutInfoProvider`.
pub trait SnapLayoutInfoProvider {
    /// How far to go before snapping, given the `velocity` and how far it would decay, as
    /// `calculateApproachOffset`: by default all of it.
    fn calculate_approach_offset(&self, _velocity: f32, decay_offset: f32) -> f32 {
        decay_offset
    }

    /// The distance to where it lands from here, with the `velocity` the approach left, as
    /// `calculateSnapOffset`.
    fn calculate_snap_offset(&self, velocity: f32) -> f32;
}

/// Where a [`SnapPosition::Custom`] puts an item, given the layout's size, the item's, the paddings
/// before and after it, its index and the items' count.
pub type CustomSnapPosition = Rc<dyn Fn(i32, i32, i32, i32, usize, usize) -> i32>;

/// Where in the viewport an item lands, as `SnapPosition`.
#[derive(Clone)]
pub enum SnapPosition {
    /// In the middle of the room between the paddings.
    Center,
    /// At the start of it.
    Start,
    /// At the end of it.
    End,
    /// By a function of the layout's size, the item's, the paddings before and after it, its index
    /// and the items' count, as an implementation of Compose's interface.
    Custom(CustomSnapPosition),
}

impl SnapPosition {
    /// The distance from the start of the content where the item lands, as `position`.
    pub fn position(
        &self,
        layout_size: i32,
        item_size: i32,
        before_content_padding: i32,
        after_content_padding: i32,
        item_index: usize,
        item_count: usize,
    ) -> i32 {
        let available = layout_size - before_content_padding - after_content_padding;
        match self {
            SnapPosition::Center => available / 2 - item_size / 2,
            SnapPosition::Start => 0,
            SnapPosition::End => available - item_size,
            SnapPosition::Custom(position) => position(
                layout_size,
                item_size,
                before_content_padding,
                after_content_padding,
                item_index,
                item_count,
            ),
        }
    }
}

/// The offset from an item's place to where `snap_position` wants it, as
/// `calculateDistanceToDesiredSnapPosition`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn distance_to_snap_position(
    viewport: i32,
    before: i32,
    after: i32,
    item_size: i32,
    item_offset: i32,
    item_index: usize,
    snap_position: &SnapPosition,
    item_count: usize,
) -> f32 {
    let desired =
        snap_position.position(viewport, item_size, before, after, item_index, item_count);
    (item_offset - desired) as f32
}

/// Which item a fling lands on, as `FinalSnappingItem`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum FinalSnappingItem {
    Closest,
    Next,
    Previous,
}

/// By the velocity left, as `calculateFinalSnappingItem`.
pub(crate) fn final_snapping_item(velocity: f32, density: Density) -> FinalSnappingItem {
    if velocity.abs() < density.to_px(MIN_FLING_VELOCITY) {
        FinalSnappingItem::Closest
    } else if velocity > 0.0 {
        FinalSnappingItem::Next
    } else {
        FinalSnappingItem::Previous
    }
}

/// What's chosen of the distances to the items around, 0 if there's none, as `calculateFinalOffset`.
pub(crate) fn final_offset(item: FinalSnappingItem, lower: f32, upper: f32) -> f32 {
    let distance = match item {
        FinalSnappingItem::Closest => {
            if upper.abs() <= lower.abs() {
                upper
            } else {
                lower
            }
        }
        FinalSnappingItem::Next => upper,
        FinalSnappingItem::Previous => lower,
    };
    if distance.is_infinite() { 0.0 } else { distance }
}

/// `decayOffset` less an average item, the way it goes, as the lazy providers' approach.
fn approach_less(average: i32, decay_offset: f32) -> f32 {
    let sign = if decay_offset == 0.0 { 0.0 } else { decay_offset.signum() };
    (decay_offset.abs() - average as f32).max(0.0) * sign
}

/// Snapping a list's items to `snap_position`, as `SnapLayoutInfoProvider(lazyListState, …)`.
pub fn list_snap_layout_info_provider(
    state: LazyListState,
    snap_position: SnapPosition,
    density: Density,
) -> Rc<dyn SnapLayoutInfoProvider> {
    struct ListProvider {
        state: LazyListState,
        snap_position: SnapPosition,
        density: Density,
    }
    impl SnapLayoutInfoProvider for ListProvider {
        fn calculate_approach_offset(&self, _velocity: f32, decay_offset: f32) -> f32 {
            let info = self.state.layout_info();
            let items = &info.visible_items_info;
            let average = if items.is_empty() {
                0
            } else {
                items.iter().map(|item| item.size).sum::<i32>() / items.len() as i32
            };
            approach_less(average, decay_offset)
        }

        fn calculate_snap_offset(&self, velocity: f32) -> f32 {
            let info = self.state.layout_info();
            // What's scrolled since that layout, as Compose's layout info copied with the delta.
            let pending = self.state.pending_scroll();
            let viewport = match info.orientation {
                Orientation::Vertical => info.viewport_size.height,
                Orientation::Horizontal => info.viewport_size.width,
            };
            let (mut lower, mut upper) = (f32::NEG_INFINITY, f32::INFINITY);
            for item in info.visible_items_info.iter().filter(|item| !item.non_scrollable) {
                let offset = distance_to_snap_position(
                    viewport,
                    info.before_content_padding,
                    info.after_content_padding,
                    item.size,
                    item.offset,
                    item.index,
                    &self.snap_position,
                    info.total_items_count,
                ) - pending;
                if offset <= 0.0 && offset > lower {
                    lower = offset;
                }
                if offset >= 0.0 && offset < upper {
                    upper = offset;
                }
            }
            final_offset(final_snapping_item(velocity, self.density), lower, upper)
        }
    }
    Rc::new(ListProvider { state, snap_position, density })
}

/// Snapping a grid's lines to `snap_position`, as `SnapLayoutInfoProvider(lazyGridState, …)`.
pub fn grid_snap_layout_info_provider(
    state: LazyGridState,
    snap_position: SnapPosition,
    density: Density,
) -> Rc<dyn SnapLayoutInfoProvider> {
    struct GridProvider {
        state: LazyGridState,
        snap_position: SnapPosition,
        density: Density,
    }
    impl SnapLayoutInfoProvider for GridProvider {
        fn calculate_approach_offset(&self, _velocity: f32, decay_offset: f32) -> f32 {
            let info = self.state.layout_info();
            let vertical = info.orientation == Orientation::Vertical;
            let items = &info.visible_items_info;
            let average = if items.is_empty() {
                0
            } else {
                let size = |item: &crate::lazy::LazyGridItemInfo| {
                    if vertical { item.size.height } else { item.size.width }
                };
                items.iter().map(size).sum::<i32>() / items.len() as i32
            };
            approach_less(average, decay_offset)
        }

        fn calculate_snap_offset(&self, velocity: f32) -> f32 {
            let info = self.state.layout_info();
            let vertical = info.orientation == Orientation::Vertical;
            let pending = self.state.pending_scroll();
            let viewport =
                if vertical { info.viewport_size.height } else { info.viewport_size.width };
            let (mut lower, mut upper) = (f32::NEG_INFINITY, f32::INFINITY);
            for item in &info.visible_items_info {
                let distance = distance_to_snap_position(
                    viewport,
                    info.before_content_padding,
                    info.after_content_padding,
                    if vertical { item.size.height } else { item.size.width },
                    if vertical { item.offset.y } else { item.offset.x },
                    item.index,
                    &self.snap_position,
                    info.total_items_count,
                ) - pending;
                if distance <= 0.0 && distance > lower {
                    lower = distance;
                }
                if distance >= 0.0 && distance < upper {
                    upper = distance;
                }
            }
            final_offset(final_snapping_item(velocity, self.density), lower, upper)
        }
    }
    Rc::new(GridProvider { state, snap_position, density })
}

/// A fling landing where `provider` says, as `snapFlingBehavior`.
#[derive(Clone)]
pub struct SnapFlingBehavior {
    provider: Rc<dyn SnapLayoutInfoProvider>,
    decay_spec: DecayAnimationSpec,
    snap_spec: AnimationSpec,
}

/// As `snapFlingBehavior(snapLayoutInfoProvider, decayAnimationSpec, snapAnimationSpec)`.
pub fn snap_fling_behavior(
    provider: Rc<dyn SnapLayoutInfoProvider>,
    decay_spec: DecayAnimationSpec,
    snap_spec: impl Into<AnimationSpec>,
) -> SnapFlingBehavior {
    SnapFlingBehavior { provider, decay_spec, snap_spec: snap_spec.into() }
}

/// As `rememberSnapFlingBehavior(snapLayoutInfoProvider)`: the spline's decay, a medium-low spring.
#[composable(non_restartable)]
pub fn remember_snap_fling_behavior(provider: Rc<dyn SnapLayoutInfoProvider>) -> SnapFlingBehavior {
    let density = compose_ui::LOCAL_DENSITY.current();
    compose_runtime::remember_keyed((ProviderKey(provider.clone()), density), move || {
        snap_fling_behavior(
            provider,
            spline_based_decay(density),
            spring_with(Spring::DAMPING_RATIO_NO_BOUNCY, Spring::STIFFNESS_MEDIUM_LOW),
        )
    })
}

/// As `rememberSnapFlingBehavior(lazyListState, snapPosition)`.
#[composable(non_restartable)]
pub fn remember_list_snap_fling_behavior(
    state: LazyListState,
    snap_position: SnapPosition,
) -> SnapFlingBehavior {
    let density = compose_ui::LOCAL_DENSITY.current();
    let provider = compose_runtime::remember_keyed((state, density), move || {
        list_snap_layout_info_provider(state, snap_position, density)
    });
    remember_snap_fling_behavior(provider)
}

/// As `rememberSnapFlingBehavior(lazyGridState, snapPosition)`.
#[composable(non_restartable)]
pub fn remember_grid_snap_fling_behavior(
    state: LazyGridState,
    snap_position: SnapPosition,
) -> SnapFlingBehavior {
    let density = compose_ui::LOCAL_DENSITY.current();
    let provider = compose_runtime::remember_keyed((state, density), move || {
        grid_snap_layout_info_provider(state, snap_position, density)
    });
    remember_snap_fling_behavior(provider)
}

/// A provider as a key, by identity.
#[derive(Clone)]
struct ProviderKey(Rc<dyn SnapLayoutInfoProvider>);

impl compose_runtime::ComposeArg for ProviderKey {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

fn coerce_to_target(value: f32, target: f32) -> f32 {
    if target == 0.0 {
        0.0
    } else if target > 0.0 {
        value.min(target)
    } else {
        value.max(target)
    }
}

/// Scrolls `delta` in `scope`, a step of an animation: told `on_step`, and it stops if a scroll
/// didn't use it or the fling's cancelled, as `consumeDelta`.
fn consume_delta(
    scope: &ScrollScope,
    animation: &AnimationScope<f32>,
    delta: f32,
    on_step: &dyn Fn(f32),
) -> f32 {
    let consumed = scope.scroll_by(delta);
    if scope.is_fling_cancelled() {
        animation.cancel_animation();
    }
    on_step(consumed);
    if (delta - consumed).abs() > 0.5 {
        animation.cancel_animation();
    }
    consumed
}

/// As `animateDecay`: returns what's left.
async fn animate_decay_to(
    scope: &ScrollScope,
    target_offset: f32,
    state: &AnimationState<f32>,
    decay_spec: &DecayAnimationSpec,
    on_step: &dyn Fn(f32),
) -> f32 {
    let previous = Cell::new(0.0f32);
    state
        .animate_decay(decay_spec, state.velocity() != 0.0, |animation| {
            let value = animation.value();
            if value.abs() >= target_offset.abs() {
                let last = coerce_to_target(value, target_offset);
                consume_delta(scope, animation, last - previous.get(), on_step);
                animation.cancel_animation();
                previous.set(last);
            } else {
                consume_delta(scope, animation, value - previous.get(), on_step);
                previous.set(value);
            }
        })
        .await;
    target_offset - previous.get()
}

/// As `animateWithTarget`: returns what's left, and the velocity it ended with, no faster than the
/// one it started at.
async fn animate_with_target(
    scope: &ScrollScope,
    target_offset: f32,
    cancel_offset: f32,
    state: &AnimationState<f32>,
    spec: &AnimationSpec,
    on_step: &dyn Fn(f32),
) -> (f32, f32) {
    let consumed_so_far = Cell::new(0.0f32);
    let initial_velocity = state.velocity();
    state
        .animate_to(target_offset, spec.clone(), state.velocity() != 0.0, |animation| {
            let value = animation.value();
            let real = coerce_to_target(value, cancel_offset);
            let delta = real - consumed_so_far.get();
            let consumed = scope.scroll_by(delta);
            if scope.is_fling_cancelled() {
                animation.cancel_animation();
            }
            on_step(consumed);
            if (delta - consumed).abs() > 0.5 || real != value {
                animation.cancel_animation();
            }
            consumed_so_far.set(consumed_so_far.get() + consumed);
        })
        .await;
    (target_offset - consumed_so_far.get(), coerce_to_target(state.velocity(), initial_velocity))
}

impl SnapFlingBehavior {
    /// As `fling`: what's left, and the velocity.
    async fn fling(
        &self,
        scope: &ScrollScope,
        initial_velocity: f32,
        on_remaining: &dyn Fn(f32),
    ) -> (f32, f32) {
        let decay_offset = self.decay_spec.calculate_target_value(0.0f32, initial_velocity);
        let initial_offset =
            self.provider.calculate_approach_offset(initial_velocity, decay_offset);
        assert!(
            !initial_offset.is_nan(),
            "compose-foundation: calculate_approach_offset returned NaN. Please use a valid value."
        );
        let sign = if initial_velocity == 0.0 { 0.0 } else { initial_velocity.signum() };
        let remaining = Cell::new(initial_offset.abs() * sign);
        on_remaining(remaining.get());
        let step = |delta: f32| {
            remaining.set(remaining.get() - delta);
            on_remaining(remaining.get());
        };
        // `tryApproach`.
        let approach = AnimationState::with_velocity(0.0f32, initial_velocity);
        let (offset, velocity) = (remaining.get(), initial_velocity);
        let approach_velocity = if offset == 0.0 || velocity == 0.0 {
            velocity
        } else if self.decay_spec.calculate_target_value(0.0f32, velocity).abs() >= offset.abs() {
            animate_decay_to(scope, offset, &approach, &self.decay_spec, &step).await;
            approach.velocity()
        } else {
            let target = offset.abs() * sign;
            animate_with_target(scope, target, offset, &approach, &self.snap_spec, &step).await.1
        };
        let final_snap_offset = self.provider.calculate_snap_offset(approach_velocity);
        assert!(
            !final_snap_offset.is_nan(),
            "compose-foundation: calculate_snap_offset returned NaN. Please use a valid value."
        );
        remaining.set(final_snap_offset);
        // The approach's velocity and time go on, from a value of 0.
        let snap = AnimationState::with(
            0.0f32,
            approach_velocity.to_vector(),
            approach.last_frame_time_nanos(),
            approach.finished_time_nanos(),
            false,
        );
        let result = animate_with_target(
            scope,
            final_snap_offset,
            final_snap_offset,
            &snap,
            &self.snap_spec,
            &step,
        )
        .await;
        on_remaining(0.0);
        result
    }
}

impl TargetedFlingBehavior for SnapFlingBehavior {
    fn perform_targeted_fling<'a>(
        &'a self,
        scope: ScrollScope,
        initial_velocity: f32,
        on_remaining_distance_updated: Rc<dyn Fn(f32)>,
    ) -> LocalBoxFuture<'a, f32> {
        Box::pin(async move {
            let (remaining, velocity) =
                self.fling(&scope, initial_velocity, &*on_remaining_distance_updated).await;
            if remaining == 0.0 { 0.0 } else { velocity }
        })
    }
}

impl FlingBehavior for SnapFlingBehavior {
    fn perform_fling<'a>(
        &'a self,
        scope: ScrollScope,
        initial_velocity: f32,
    ) -> LocalBoxFuture<'a, f32> {
        self.perform_targeted_fling(scope, initial_velocity, Rc::new(|_| {}))
    }
}
