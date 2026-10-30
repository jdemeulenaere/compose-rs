//! Dragging between anchors, as Compose's `Modifier.anchoredDraggable`, `AnchoredDraggableState` and
//! `DraggableAnchors`: a state holding where it is and the anchor it's at, what's dragged clamped
//! between the anchors, and a release landing on one.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::rc::Rc;
use std::task::{Poll, Waker};

use compose_animation::{
    AnimationSpec, AnimationState, DecayAnimationSpec, FloatDecayAnimationSpec, animate,
    exponential_decay, generate_decay_animation_spec, tween,
};
use compose_runtime::{ComposeArg, CoroutineScope, OwnedState, composable, mutable_state_of};
use compose_ui::{
    Density, Dp, EventContext, Modifier, ModifierNodeElement, MutableInteractionSource,
    NestedScrollSource, Offset, Velocity,
};

use super::drag_gesture::{Deltas, DragBehavior, DragGestureNode};
use super::scrollable::LocalBoxFuture;
use crate::overscroll::{OverscrollEffect, PerformFling, same_overscroll};

use super::snapping::{SnapLayoutInfoProvider, snap_fling_behavior};
use super::{FlingBehavior, Orientation, ScrollScope};
use crate::mutator::{MutatePriority, MutationInterrupted, MutatorMutex};

/// The velocity past which a release goes to the next anchor, as `AnchoredDraggableMinFlingVelocity`.
const MIN_FLING_VELOCITY: Dp = Dp(125.0);

/// The anchors of an [`AnchoredDraggableState`] and where they are, as `DraggableAnchors`. Several
/// may be at the same position. Compose's is an interface with one implementation.
#[derive(Clone, PartialEq, Debug)]
pub struct DraggableAnchors<T> {
    keys: Vec<T>,
    positions: Vec<f32>,
}

/// Where a [`DraggableAnchors`] is built, as `DraggableAnchorsConfig`.
pub struct DraggableAnchorsConfig<T> {
    anchors: DraggableAnchors<T>,
}

impl<T: PartialEq> DraggableAnchorsConfig<T> {
    /// `anchor` at `position`, as `anchor at position`. Given twice, the last counts.
    pub fn at(&mut self, anchor: T, position: f32) {
        match self.anchors.keys.iter().position(|key| *key == anchor) {
            Some(index) => self.anchors.positions[index] = position,
            None => {
                self.anchors.keys.push(anchor);
                self.anchors.positions.push(position);
            }
        }
    }
}

impl<T: Clone + PartialEq> DraggableAnchors<T> {
    /// As `DraggableAnchors { A at 0f; B at 100f }`.
    pub fn new(build: impl FnOnce(&mut DraggableAnchorsConfig<T>)) -> DraggableAnchors<T> {
        let mut config = DraggableAnchorsConfig { anchors: DraggableAnchors::empty() };
        build(&mut config);
        config.anchors
    }

    fn empty() -> DraggableAnchors<T> {
        DraggableAnchors { keys: Vec::new(), positions: Vec::new() }
    }

    /// How many anchors there are, as `size`.
    pub fn size(&self) -> usize {
        self.keys.len()
    }

    /// Where `anchor` is, NaN if it isn't one, as `positionOf`.
    pub fn position_of(&self, anchor: &T) -> f32 {
        self.keys
            .iter()
            .position(|key| key == anchor)
            .map_or(f32::NAN, |index| self.positions[index])
    }

    /// Whether `anchor` is one, as `hasPositionFor`.
    pub fn has_position_for(&self, anchor: &T) -> bool {
        self.keys.contains(anchor)
    }

    /// The anchor closest to `position`, the last of those as close, as `closestAnchor(position)`.
    pub fn closest_anchor(&self, position: f32) -> Option<T> {
        let mut closest = None;
        let mut least = f32::INFINITY;
        for (index, anchor) in self.positions.iter().enumerate() {
            let distance = (position - anchor).abs();
            if distance <= least {
                closest = Some(index);
                least = distance;
            }
        }
        closest.map(|index| self.keys[index].clone())
    }

    /// The anchor closest to `position` that way, as `closestAnchor(position, searchUpwards)`.
    pub fn closest_anchor_towards(&self, position: f32, search_upwards: bool) -> Option<T> {
        let mut closest = None;
        let mut least = f32::INFINITY;
        for (index, anchor) in self.positions.iter().enumerate() {
            let delta = if search_upwards { anchor - position } else { position - anchor };
            let distance = if delta < 0.0 { f32::INFINITY } else { delta };
            if distance <= least {
                closest = Some(index);
                least = distance;
            }
        }
        closest.map(|index| self.keys[index].clone())
    }

    /// The lowest position, NaN without an anchor, as `minPosition`.
    pub fn min_position(&self) -> f32 {
        let mut positions = self.positions.iter().copied();
        let first = positions.next().unwrap_or(f32::NAN);
        positions.fold(first, f32::min)
    }

    /// The highest position, NaN without an anchor, as `maxPosition`.
    pub fn max_position(&self) -> f32 {
        let mut positions = self.positions.iter().copied();
        let first = positions.next().unwrap_or(f32::NAN);
        positions.fold(first, f32::max)
    }

    /// The anchor at `index`, as `anchorAt`.
    pub fn anchor_at(&self, index: usize) -> Option<T> {
        self.keys.get(index).cloned()
    }

    /// The position at `index`, NaN out of them, as `positionAt`.
    pub fn position_at(&self, index: usize) -> f32 {
        self.positions.get(index).copied().unwrap_or(f32::NAN)
    }

    /// Each anchor and its position, as `forEach`.
    pub fn for_each(&self, mut block: impl FnMut(&T, f32)) {
        for (key, position) in self.keys.iter().zip(&self.positions) {
            block(key, *position);
        }
    }

    /// The anchor a release lands on, as `computeTarget`.
    fn compute_target(
        &self,
        offset: f32,
        velocity: f32,
        positional_threshold: &dyn Fn(f32) -> f32,
        velocity_threshold: f32,
    ) -> Option<T> {
        assert!(
            !offset.is_nan(),
            "compose-foundation: the offset of computeTarget must not be NaN"
        );
        let moving = velocity.abs() > 0.0;
        let forward = moving && velocity > 0.0;
        if !moving {
            self.closest_anchor(offset)
        } else if velocity.abs() >= velocity_threshold.abs() {
            self.closest_anchor_towards(offset, forward)
        } else {
            let left = self.closest_anchor_towards(offset, false)?;
            let left_position = self.position_of(&left);
            let right = self.closest_anchor_towards(offset, true)?;
            let right_position = self.position_of(&right);
            let distance = (left_position - right_position).abs();
            let threshold = positional_threshold(distance).abs();
            let from = if forward { left_position } else { right_position };
            let past = (from - offset).abs() >= threshold;
            Some(if past == forward { right } else { left })
        }
    }
}

/// What an [`AnchoredDraggableState`]'s drag moves it with, as `AnchoredDragScope`.
pub struct AnchoredDragScope<T: Clone + PartialEq + 'static> {
    inner: Rc<Inner<T>>,
}

impl<T: Clone + PartialEq + 'static> Clone for AnchoredDragScope<T> {
    fn clone(&self) -> Self {
        AnchoredDragScope { inner: self.inner.clone() }
    }
}

impl<T: Clone + PartialEq + 'static> AnchoredDragScope<T> {
    /// Moves to `new_offset`, as `dragTo`: past half the way to an anchor, it's the current value.
    pub fn drag_to(&self, new_offset: f32, last_known_velocity: f32) {
        self.inner.drag_to(new_offset, last_known_velocity);
    }
}

struct Inner<T: Clone + PartialEq + 'static> {
    mutex: MutatorMutex,
    confirm_value_change: Rc<dyn Fn(&T) -> bool>,
    current_value: OwnedState<T>,
    settled_value: OwnedState<T>,
    offset: OwnedState<f32>,
    last_velocity: OwnedState<f32>,
    drag_target: OwnedState<Option<T>>,
    anchors: OwnedState<DraggableAnchors<T>>,
    /// What a drag restarts on: new anchors, or a new target.
    restarts: Cell<u64>,
    restart_wakers: RefCell<Vec<Waker>>,
    left_bound: RefCell<Option<T>>,
    right_bound: RefCell<Option<T>>,
}

impl<T: Clone + PartialEq + 'static> Inner<T> {
    fn restart_drag(&self) {
        self.restarts.set(self.restarts.get() + 1);
        for waker in self.restart_wakers.borrow_mut().drain(..) {
            waker.wake();
        }
    }

    fn drag_to(&self, new_offset: f32, last_known_velocity: f32) {
        let previous = self.offset.get();
        self.offset.set(new_offset);
        self.last_velocity.set(last_known_velocity);
        if previous.is_nan() {
            return;
        }
        self.update_if_needed(new_offset >= previous);
    }

    /// As `updateIfNeeded`: past half the way between the anchors around, the anchor ahead.
    fn update_if_needed(&self, forward: bool) {
        let distance = self.update_bounds(forward);
        let anchors = self.anchors.get();
        let current = self.current_value.get();
        let offset = self.offset.get();
        let from_current = (offset - anchors.position_of(&current)).abs();
        if from_current >= distance / 2.0 {
            let closest = if forward { &self.right_bound } else { &self.left_bound };
            let closest = closest.borrow().clone().unwrap_or(current);
            if (self.confirm_value_change)(&closest) {
                self.current_value.set(closest);
            }
        }
    }

    /// As `updateBounds`; returns the distance between them.
    fn update_bounds(&self, forward: bool) -> f32 {
        let anchors = self.anchors.get();
        let current = self.current_value.get();
        let offset = self.offset.get();
        let (left, right) = if offset == anchors.position_of(&current) {
            let from = offset + if forward { 1.0 } else { -1.0 };
            let beyond = anchors.closest_anchor_towards(from, forward).unwrap_or(current.clone());
            if forward { (current, beyond) } else { (beyond, current) }
        } else {
            let left = anchors.closest_anchor_towards(offset, false).unwrap_or(current.clone());
            let right = anchors.closest_anchor_towards(offset, true).unwrap_or(current);
            (left, right)
        };
        let distance = (anchors.position_of(&left) - anchors.position_of(&right)).abs();
        *self.left_bound.borrow_mut() = Some(left);
        *self.right_bound.borrow_mut() = Some(right);
        distance
    }

    fn new_offset_for_delta(&self, delta: f32) -> f32 {
        let anchors = self.anchors.get();
        let offset = self.offset.get();
        let offset = if offset.is_nan() { 0.0 } else { offset } + delta;
        let (min, max) = (anchors.min_position(), anchors.max_position());
        // `coerceIn`, which NaN bounds leave as it is.
        if offset < min {
            min
        } else if offset > max {
            max
        } else {
            offset
        }
    }
}

/// Clears the drag's target however it ends.
struct ClearTarget<T: Clone + PartialEq + 'static>(Rc<Inner<T>>);

impl<T: Clone + PartialEq + 'static> Drop for ClearTarget<T> {
    fn drop(&mut self) {
        self.0.drag_target.set(None);
    }
}

/// Runs `block`, again with the new inputs when they change, as `restartable`.
async fn restartable<T, Fut>(inner: &Rc<Inner<T>>, mut block: impl FnMut() -> Fut)
where
    T: Clone + PartialEq + 'static,
    Fut: Future<Output = ()>,
{
    loop {
        let started = inner.restarts.get();
        let mut running = std::pin::pin!(block());
        let finished = std::future::poll_fn(|context| {
            if inner.restarts.get() != started {
                return Poll::Ready(false);
            }
            match running.as_mut().poll(context) {
                Poll::Ready(()) => Poll::Ready(true),
                Poll::Pending => {
                    inner.restart_wakers.borrow_mut().push(context.waker().clone());
                    Poll::Pending
                }
            }
        })
        .await;
        if finished {
            return;
        }
    }
}

/// Where an anchored drag is, as `AnchoredDraggableState`.
pub struct AnchoredDraggableState<T: Clone + PartialEq + 'static> {
    inner: Rc<Inner<T>>,
}

impl<T: Clone + PartialEq + 'static> Clone for AnchoredDraggableState<T> {
    fn clone(&self) -> Self {
        AnchoredDraggableState { inner: self.inner.clone() }
    }
}

impl<T: Clone + PartialEq + 'static> ComposeArg for AnchoredDraggableState<T> {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl<T: Clone + PartialEq + compose_runtime::Saveable + 'static> AnchoredDraggableState<T> {
    /// What `remember_saveable_with` keeps of it, the current value, as Compose's `Saver`. Anchors
    /// come again from the content, a veto (the deprecated `confirmValueChange`) doesn't survive.
    pub fn saver() -> impl compose_runtime::Saver<AnchoredDraggableState<T>> {
        compose_runtime::saver(
            |state: &AnchoredDraggableState<T>| Some(state.current_value().save()),
            |saved| T::restore(saved).map(AnchoredDraggableState::new),
        )
    }
}

impl<T: Clone + PartialEq + 'static> AnchoredDraggableState<T> {
    /// At `initial_value`, with no anchors yet, as `AnchoredDraggableState(initialValue)`.
    pub fn new(initial_value: T) -> AnchoredDraggableState<T> {
        AnchoredDraggableState::with_confirm(initial_value, |_| true)
    }

    /// With a veto on what it moves to, as `AnchoredDraggableState(initialValue, confirmValueChange)`.
    pub fn with_confirm(
        initial_value: T,
        confirm_value_change: impl Fn(&T) -> bool + 'static,
    ) -> AnchoredDraggableState<T> {
        AnchoredDraggableState {
            inner: Rc::new(Inner {
                mutex: MutatorMutex::new(),
                confirm_value_change: Rc::new(confirm_value_change),
                current_value: mutable_state_of(initial_value.clone()),
                settled_value: mutable_state_of(initial_value),
                offset: mutable_state_of(f32::NAN),
                last_velocity: mutable_state_of(0.0),
                drag_target: mutable_state_of(None),
                anchors: mutable_state_of(DraggableAnchors::empty()),
                restarts: Cell::new(0),
                restart_wakers: RefCell::default(),
                left_bound: RefCell::default(),
                right_bound: RefCell::default(),
            }),
        }
    }

    /// With its anchors, at the position of `initial_value`, as
    /// `AnchoredDraggableState(initialValue, anchors)`.
    pub fn with_anchors(
        initial_value: T,
        anchors: DraggableAnchors<T>,
    ) -> AnchoredDraggableState<T> {
        AnchoredDraggableState::with_anchors_and_confirm(initial_value, anchors, |_| true)
    }

    /// As `AnchoredDraggableState(initialValue, anchors, confirmValueChange)`.
    pub fn with_anchors_and_confirm(
        initial_value: T,
        anchors: DraggableAnchors<T>,
        confirm_value_change: impl Fn(&T) -> bool + 'static,
    ) -> AnchoredDraggableState<T> {
        let state =
            AnchoredDraggableState::with_confirm(initial_value.clone(), confirm_value_change);
        state.inner.anchors.set(anchors);
        state.try_snap_to(initial_value);
        state
    }

    /// The anchor it's at or closest to while dragged, as `currentValue`.
    pub fn current_value(&self) -> T {
        self.inner.current_value.get()
    }

    /// The last anchor it settled at, as `settledValue`.
    pub fn settled_value(&self) -> T {
        self.inner.settled_value.get()
    }

    /// The anchor it's going to: what's animated to, else the closest, as `targetValue`.
    pub fn target_value(&self) -> T {
        self.inner.drag_target.get().unwrap_or_else(|| {
            let offset = self.inner.offset.get();
            let current = self.inner.current_value.get();
            if offset.is_nan() {
                return current;
            }
            let anchors = self.inner.anchors.get();
            let at_current = anchors.position_of(&current);
            if at_current.is_nan() || offset == at_current {
                current
            } else {
                anchors.closest_anchor(offset).unwrap_or(current)
            }
        })
    }

    /// Where it is, NaN before its anchors are known, as `offset`.
    pub fn offset(&self) -> f32 {
        self.inner.offset.get()
    }

    /// Where it is, as `requireOffset`. Panics before its anchors are known.
    pub fn require_offset(&self) -> f32 {
        let offset = self.offset();
        assert!(
            !offset.is_nan(),
            "compose-foundation: the offset was read before being initialized. Did you access the \
             offset in a phase before layout, like effects or composition?"
        );
        offset
    }

    /// Whether it's animating to an anchor, as `isAnimationRunning`.
    pub fn is_animation_running(&self) -> bool {
        self.inner.drag_target.get().is_some()
    }

    /// How far it is from `from` to `to`, from 0 to 1, as `progress(from, to)`.
    pub fn progress(&self, from: &T, to: &T) -> f32 {
        let anchors = self.inner.anchors.get();
        let (from_offset, to_offset) = (anchors.position_of(from), anchors.position_of(to));
        let (low, high) = (from_offset.min(to_offset), from_offset.max(to_offset));
        let offset = self.offset();
        let offset = if offset < low {
            low
        } else if offset > high {
            high
        } else {
            offset
        };
        let fraction = (offset - from_offset) / (to_offset - from_offset);
        if fraction.is_nan() {
            1.0
        } else if fraction < 1e-6 {
            0.0
        } else if fraction > 1.0 - 1e-6 {
            1.0
        } else {
            fraction.abs()
        }
    }

    /// The velocity of the last `drag_to`, as `lastVelocity`.
    pub fn last_velocity(&self) -> f32 {
        self.inner.last_velocity.get()
    }

    /// Its anchors, as `anchors`.
    pub fn anchors(&self) -> DraggableAnchors<T> {
        self.inner.anchors.get()
    }

    /// New anchors, going to the closest one or what it's going to, as `updateAnchors(newAnchors)`.
    pub fn update_anchors(&self, new_anchors: DraggableAnchors<T>) {
        let offset = self.inner.offset.get();
        let target = if offset.is_nan() { None } else { new_anchors.closest_anchor(offset) };
        let target = target.unwrap_or_else(|| self.target_value());
        self.update_anchors_to(new_anchors, target);
    }

    /// New anchors, going to `new_target`, as `updateAnchors(newAnchors, newTarget)`: at once if
    /// nothing's dragging it, else restarting what is.
    pub fn update_anchors_to(&self, new_anchors: DraggableAnchors<T>, new_target: T) {
        if self.inner.anchors.get() != new_anchors {
            self.inner.anchors.set(new_anchors);
            if !self.try_snap_to(new_target.clone()) {
                self.inner.drag_target.set(Some(new_target));
            }
            self.inner.restart_drag();
        }
    }

    fn try_snap_to(&self, target: T) -> bool {
        let inner = self.inner.clone();
        self.inner.mutex.try_mutate(move || {
            let position = inner.anchors.get().position_of(&target);
            if !position.is_nan() {
                inner.drag_to(position, 0.0);
                inner.drag_target.set(None);
            }
            inner.current_value.set(target.clone());
            inner.settled_value.set(target);
        })
    }

    /// Goes to the closest anchor, or back if that's vetoed, as `settle(animationSpec)`.
    pub async fn settle(
        &self,
        animation_spec: impl Into<AnimationSpec>,
    ) -> Result<(), MutationInterrupted> {
        let previous = self.inner.current_value.get();
        let closest = self.inner.anchors.get().closest_anchor(self.require_offset());
        let target =
            closest.filter(|target| (self.inner.confirm_value_change)(target)).unwrap_or(previous);
        self.animate_to_with(target, animation_spec.into()).await
    }

    /// Takes it for `block`, given the scope and its anchors, again with new ones when they change,
    /// as `anchoredDrag(dragPriority, block)`; ends at the anchor it's within half a pixel of.
    pub async fn anchored_drag<F, Fut>(
        &self,
        priority: MutatePriority,
        mut block: F,
    ) -> Result<(), MutationInterrupted>
    where
        F: FnMut(AnchoredDragScope<T>, DraggableAnchors<T>) -> Fut,
        Fut: Future<Output = ()>,
    {
        let inner = self.inner.clone();
        self.inner
            .mutex
            .mutate(priority, async {
                restartable(&inner, || {
                    block(AnchoredDragScope { inner: inner.clone() }, inner.anchors.get())
                })
                .await;
                let anchors = inner.anchors.get();
                let offset = inner.offset.get();
                if let Some(closest) = anchors.closest_anchor(offset) {
                    let at_closest = (offset - anchors.position_of(&closest)).abs() < 0.5;
                    if at_closest && (inner.confirm_value_change)(&closest) {
                        inner.settled_value.set(closest.clone());
                        inner.current_value.set(closest);
                    }
                }
            })
            .await
    }

    /// Takes it for `block` going to `target_value`, given the scope, its anchors and the latest
    /// target, as `anchoredDrag(targetValue, dragPriority, block)`. An anchor not among them is its
    /// value at once.
    pub async fn anchored_drag_to<F, Fut>(
        &self,
        target_value: T,
        priority: MutatePriority,
        mut block: F,
    ) -> Result<(), MutationInterrupted>
    where
        F: FnMut(AnchoredDragScope<T>, DraggableAnchors<T>, T) -> Fut,
        Fut: Future<Output = ()>,
    {
        let inner = self.inner.clone();
        if !inner.anchors.get().has_position_for(&target_value) {
            if (inner.confirm_value_change)(&target_value) {
                inner.settled_value.set(target_value.clone());
                inner.current_value.set(target_value);
            }
            return Ok(());
        }
        let _clear = ClearTarget(inner.clone());
        let this = self.clone();
        self.inner
            .mutex
            .mutate(priority, async {
                inner.drag_target.set(Some(target_value.clone()));
                inner.restart_drag();
                restartable(&inner, || {
                    block(
                        AnchoredDragScope { inner: inner.clone() },
                        inner.anchors.get(),
                        this.target_untracked(),
                    )
                })
                .await;
                if (inner.confirm_value_change)(&target_value) {
                    let position = inner.anchors.get().position_of(&target_value);
                    let velocity = inner.last_velocity.get();
                    inner.drag_to(position, velocity);
                    inner.settled_value.set(target_value.clone());
                    inner.current_value.set(target_value.clone());
                }
            })
            .await
    }

    fn target_untracked(&self) -> T {
        self.inner.drag_target.get().unwrap_or_else(|| self.inner.current_value.get())
    }

    /// Moves it by `delta`, clamped between its anchors, as `dispatchRawDelta`; returns what it moved.
    pub fn dispatch_raw_delta(&self, delta: f32) -> f32 {
        let new_offset = self.inner.new_offset_for_delta(delta);
        let consumed = new_offset - self.require_offset();
        self.inner.drag_to(new_offset, 0.0);
        consumed
    }

    /// Jumps to `target_value`, as `snapTo`.
    pub async fn snap_to(&self, target_value: T) -> Result<(), MutationInterrupted> {
        self.anchored_drag_to(
            target_value,
            MutatePriority::Default,
            |scope, anchors, latest| async move {
                let position = anchors.position_of(&latest);
                if !position.is_nan() {
                    scope.drag_to(position, 0.0);
                }
            },
        )
        .await
    }

    /// Animates to `target_value`, as `animateTo(targetValue)`, by the default spec.
    pub async fn animate_to(&self, target_value: T) -> Result<(), MutationInterrupted> {
        self.animate_to_with(target_value, AnchoredDraggableDefaults::snap_animation_spec()).await
    }

    /// Animates to `target_value`, as `animateTo(targetValue, animationSpec)`.
    pub async fn animate_to_with(
        &self,
        target_value: T,
        animation_spec: impl Into<AnimationSpec>,
    ) -> Result<(), MutationInterrupted> {
        let spec = animation_spec.into();
        let inner = self.inner.clone();
        self.anchored_drag_to(target_value, MutatePriority::Default, |scope, anchors, latest| {
            let velocity = inner.last_velocity.get();
            let inner = inner.clone();
            let spec = spec.clone();
            async move { animate_scope_to(&inner, velocity, &scope, &anchors, &latest, spec).await }
        })
        .await
    }

    /// Goes to `target_value` at `velocity`: by decay if it gets there, else by `snap_spec`, as
    /// `animateToWithDecay`; returns the velocity used.
    pub async fn animate_to_with_decay(
        &self,
        target_value: T,
        velocity: f32,
        snap_spec: impl Into<AnimationSpec>,
        decay_spec: DecayAnimationSpec,
    ) -> Result<f32, MutationInterrupted> {
        let snap_spec = snap_spec.into();
        let remaining = Rc::new(Cell::new(velocity));
        let inner = self.inner.clone();
        let left = remaining.clone();
        self.anchored_drag_to(target_value, MutatePriority::Default, |scope, anchors, latest| {
            let (inner, left, snap_spec, decay_spec) =
                (inner.clone(), left.clone(), snap_spec.clone(), decay_spec.clone());
            async move {
                let target = anchors.position_of(&latest);
                if target.is_nan() {
                    return;
                }
                let offset = inner.offset.get();
                let start = if offset.is_nan() { 0.0 } else { offset };
                if start == target {
                    return;
                }
                let projected = decay_spec.calculate_target_value(start, velocity);
                let reaches =
                    if velocity > 0.0 { projected >= target } else { projected <= target };
                if velocity * (target - start) < 0.0 || velocity == 0.0 || !reaches {
                    animate_scope_to(&inner, velocity, &scope, &anchors, &latest, snap_spec).await;
                    left.set(0.0);
                    return;
                }
                let previous = Cell::new(start);
                let state = AnimationState::with_velocity(start, velocity);
                state
                    .animate_decay(&decay_spec, false, |animation| {
                        let value = animation.value();
                        let before = previous.get();
                        if (value < target && before > target)
                            || (value > target && before < target)
                        {
                            let last = coerce_past(value, target, before);
                            scope.drag_to(last, animation.velocity());
                            let velocity = animation.velocity();
                            left.set(if velocity.is_nan() { 0.0 } else { velocity });
                            previous.set(last);
                            animation.cancel_animation();
                        } else {
                            scope.drag_to(value, animation.velocity());
                            left.set(animation.velocity());
                            previous.set(value);
                        }
                    })
                    .await;
            }
        })
        .await?;
        Ok(velocity - remaining.get())
    }
}

/// The value, at most at the target coming from `before`, as `coerceToTarget`.
fn coerce_past(value: f32, target: f32, before: f32) -> f32 {
    if before < target { value.min(target) } else { value.max(target) }
}

async fn animate_scope_to<T: Clone + PartialEq + 'static>(
    inner: &Rc<Inner<T>>,
    velocity: f32,
    scope: &AnchoredDragScope<T>,
    anchors: &DraggableAnchors<T>,
    latest: &T,
    spec: AnimationSpec,
) {
    let target = anchors.position_of(latest);
    let offset = inner.offset.get();
    let start = if offset.is_nan() { 0.0 } else { offset };
    if !target.is_nan() && start != target {
        animate(start, target, velocity, spec, |value, velocity| scope.drag_to(value, velocity))
            .await;
    }
}

/// The defaults of `anchoredDraggable`, as `AnchoredDraggableDefaults`.
pub struct AnchoredDraggableDefaults;

impl AnchoredDraggableDefaults {
    /// As `SnapAnimationSpec`.
    pub fn snap_animation_spec() -> AnimationSpec {
        tween(300).into()
    }

    /// As `PositionalThreshold`: half the distance.
    pub fn positional_threshold(distance: f32) -> f32 {
        distance / 2.0
    }

    /// As `DecayAnimationSpec`.
    pub fn decay_animation_spec() -> DecayAnimationSpec {
        exponential_decay(1.0, 0.1)
    }
}

/// A decay that goes nowhere: an anchored fling never decays.
struct NoDecay;

impl FloatDecayAnimationSpec for NoDecay {
    fn abs_velocity_threshold(&self) -> f32 {
        0.0
    }
    fn value_from_nanos(&self, _: i64, _: f32, _: f32) -> f32 {
        0.0
    }
    fn duration_nanos(&self, _: f32, _: f32) -> i64 {
        0
    }
    fn velocity_from_nanos(&self, _: i64, _: f32, _: f32) -> f32 {
        0.0
    }
    fn target_value(&self, _: f32, _: f32) -> f32 {
        0.0
    }
}

struct AnchoredProvider<T: Clone + PartialEq + 'static> {
    state: AnchoredDraggableState<T>,
    positional_threshold: Rc<dyn Fn(f32) -> f32>,
    velocity_threshold: f32,
}

impl<T: Clone + PartialEq + 'static> SnapLayoutInfoProvider for AnchoredProvider<T> {
    fn calculate_approach_offset(&self, _velocity: f32, _decay_offset: f32) -> f32 {
        0.0
    }

    fn calculate_snap_offset(&self, velocity: f32) -> f32 {
        let inner = &self.state.inner;
        let offset = inner.offset.get();
        assert!(!offset.is_nan(), "compose-foundation: a fling before the offset's known");
        let anchors = inner.anchors.get();
        let proposed = anchors.compute_target(
            offset,
            velocity,
            &*self.positional_threshold,
            self.velocity_threshold,
        );
        let target = proposed
            .filter(|target| (inner.confirm_value_change)(target))
            .unwrap_or_else(|| inner.settled_value.get());
        anchors.position_of(&target) - offset
    }
}

/// The fling of an anchored drag, as `anchoredDraggableFlingBehavior`.
pub fn anchored_fling_behavior<T: Clone + PartialEq + 'static>(
    state: AnchoredDraggableState<T>,
    density: Density,
    positional_threshold: Rc<dyn Fn(f32) -> f32>,
    spec: AnimationSpec,
) -> Rc<dyn FlingBehavior> {
    Rc::new(snap_fling_behavior(
        Rc::new(AnchoredProvider {
            state,
            positional_threshold,
            velocity_threshold: density.to_px(MIN_FLING_VELOCITY),
        }),
        generate_decay_animation_spec(NoDecay),
        spec,
    ))
}

/// A fling landing on an anchor past `positional_threshold`, as
/// `AnchoredDraggableDefaults.flingBehavior(state, positionalThreshold, animationSpec)`.
#[composable(non_restartable)]
pub fn remember_anchored_fling_behavior<T: Clone + PartialEq + 'static>(
    state: AnchoredDraggableState<T>,
    positional_threshold: Rc<dyn Fn(f32) -> f32>,
    animation_spec: AnimationSpec,
) -> Rc<dyn FlingBehavior> {
    let density = compose_ui::LOCAL_DENSITY.current();
    compose_runtime::remember_keyed((state.clone(), density), move || {
        anchored_fling_behavior(state, density, positional_threshold, animation_spec)
    })
}

/// What `Modifier::anchored_draggable` takes beyond its state and axis, as its default parameters.
#[derive(Clone)]
pub struct AnchoredDraggableOptions {
    pub enabled: bool,
    /// `None`: a horizontal drag in a right-to-left layout goes the other way.
    pub reverse_direction: Option<bool>,
    pub interaction_source: Option<MutableInteractionSource>,
    /// `None`: while it's animating.
    pub start_drag_immediately: Option<bool>,
    /// `None`: the default, by half the distance and 125 dp a second, a tween.
    pub fling_behavior: Option<Rc<dyn FlingBehavior>>,
    /// Given the deltas and velocities past the anchors; not drawn here: as Compose's, apply
    /// [`overscroll`](crate::OverscrollModifier::overscroll) too.
    pub overscroll_effect: Option<Rc<dyn OverscrollEffect>>,
}

impl Default for AnchoredDraggableOptions {
    fn default() -> AnchoredDraggableOptions {
        AnchoredDraggableOptions {
            enabled: true,
            reverse_direction: None,
            interaction_source: None,
            start_drag_immediately: None,
            fling_behavior: None,
            overscroll_effect: None,
        }
    }
}

/// Drags between anchors, as `Modifier.anchoredDraggable`.
pub trait AnchoredDraggableModifier {
    fn anchored_draggable<T: Clone + PartialEq + 'static>(
        self,
        state: AnchoredDraggableState<T>,
        orientation: Orientation,
        options: AnchoredDraggableOptions,
    ) -> Modifier;
}

impl AnchoredDraggableModifier for Modifier {
    fn anchored_draggable<T: Clone + PartialEq + 'static>(
        self,
        state: AnchoredDraggableState<T>,
        orientation: Orientation,
        options: AnchoredDraggableOptions,
    ) -> Modifier {
        self.then_custom(AnchoredElement { state, orientation, options })
    }
}

struct AnchoredInputs<T: Clone + PartialEq + 'static> {
    state: AnchoredDraggableState<T>,
    orientation: Orientation,
    reverse_direction: Option<bool>,
    start_drag_immediately: Option<bool>,
    fling_behavior: Option<Rc<dyn FlingBehavior>>,
    overscroll_effect: Option<Rc<dyn OverscrollEffect>>,
}

struct AnchoredBehavior<T: Clone + PartialEq + 'static> {
    inputs: RefCell<AnchoredInputs<T>>,
    rtl: Cell<bool>,
    density: Cell<Density>,
}

impl<T: Clone + PartialEq + 'static> AnchoredBehavior<T> {
    /// A delta or velocity on the axis, the way it goes.
    fn one_directional(&self, along: Offset) -> f32 {
        let inputs = self.inputs.borrow();
        let horizontal = inputs.orientation == Orientation::Horizontal;
        let reverse = inputs.reverse_direction.unwrap_or(self.rtl.get() && horizontal);
        let value = if horizontal { along.x } else { along.y };
        if reverse { -value } else { value }
    }

    /// A delta along the axis as an offset, as `toOffset`.
    fn offset_of(&self, value: f32) -> Offset {
        if self.inputs.borrow().orientation == Orientation::Horizontal {
            Offset::new(value, 0.0)
        } else {
            Offset::new(0.0, value)
        }
    }

    /// An offset's value along the axis, as `toFloat`.
    fn on_axis_of(&self, offset: Offset) -> f32 {
        if self.inputs.borrow().orientation == Orientation::Horizontal {
            offset.x
        } else {
            offset.y
        }
    }

    fn fling(&self) -> Rc<dyn FlingBehavior> {
        let inputs = self.inputs.borrow();
        inputs.fling_behavior.clone().unwrap_or_else(|| {
            anchored_fling_behavior(
                inputs.state.clone(),
                self.density.get(),
                Rc::new(AnchoredDraggableDefaults::positional_threshold),
                AnchoredDraggableDefaults::snap_animation_spec(),
            )
        })
    }
}

impl<T: Clone + PartialEq + 'static> DragBehavior for AnchoredBehavior<T> {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn on_pointer_context(&self, context: &EventContext<'_>) {
        self.rtl.set(context.layout_direction() == compose_ui::LayoutDirection::Rtl);
        self.density.set(context.density());
    }

    fn drag(
        self: Rc<Self>,
        deltas: Deltas,
    ) -> LocalBoxFuture<'static, Result<(), MutationInterrupted>> {
        let state = self.inputs.borrow().state.clone();
        Box::pin(async move {
            let (deltas, behavior) = (Rc::new(deltas), self.clone());
            let inner = state.inner.clone();
            state
                .anchored_drag(MutatePriority::Default, |scope, _| {
                    let (deltas, behavior, inner) =
                        (deltas.clone(), behavior.clone(), inner.clone());
                    async move {
                        let effect = behavior.inputs.borrow().overscroll_effect.clone();
                        while let Some(delta) = deltas.next().await {
                            let delta = behavior.one_directional(delta);
                            let Some(effect) = &effect else {
                                scope.drag_to(inner.new_offset_for_delta(delta), 0.0);
                                continue;
                            };
                            let mut perform = |delta: Offset| {
                                let new_offset =
                                    inner.new_offset_for_delta(behavior.on_axis_of(delta));
                                let consumed = new_offset - inner.offset.get();
                                scope.drag_to(new_offset, 0.0);
                                behavior.offset_of(consumed)
                            };
                            effect.apply_to_scroll(
                                behavior.offset_of(delta),
                                NestedScrollSource::UserInput,
                                &mut perform,
                            );
                        }
                    }
                })
                .await
        })
    }

    fn on_drag_started(&self, _scope: CoroutineScope, _position: Offset) {}

    fn on_drag_stopped(&self, scope: CoroutineScope, velocity: Velocity) {
        let velocity = self.one_directional(Offset::new(velocity.x, velocity.y));
        let (fling, state, effect, horizontal) = {
            let inputs = self.inputs.borrow();
            (
                self.fling(),
                inputs.state.clone(),
                inputs.overscroll_effect.clone(),
                inputs.orientation == Orientation::Horizontal,
            )
        };
        scope.launch(async move {
            let Some(effect) = effect else {
                let _ = anchored_fling(&state, &fling, velocity).await;
                return;
            };
            let on_axis = move |value: f32| {
                if horizontal { Velocity::new(value, 0.0) } else { Velocity::new(0.0, value) }
            };
            let perform: PerformFling = Rc::new(move |available: Velocity| {
                let (state, fling) = (state.clone(), fling.clone());
                Box::pin(async move {
                    let available = if horizontal { available.x } else { available.y };
                    let left = anchored_fling(&state, &fling, available).await.unwrap_or(0.0);
                    let offset = state.require_offset();
                    let anchors = state.anchors();
                    // Its velocity for the effect only at the first or last anchor, as Compose's,
                    // which names what `fling` leaves "consumed" here.
                    on_axis(
                        if offset >= anchors.max_position() || offset <= anchors.min_position() {
                            left
                        } else {
                            available
                        },
                    )
                })
            });
            effect.apply_to_fling(on_axis(velocity), perform).await;
        });
    }

    fn start_drag_immediately(&self) -> bool {
        let inputs = self.inputs.borrow();
        inputs.start_drag_immediately.unwrap_or_else(|| inputs.state.is_animation_running())
    }
}

/// Flings to an anchor in a drag of `state`'s, as the node's `fling`; returns the velocity left.
async fn anchored_fling<T: Clone + PartialEq + 'static>(
    state: &AnchoredDraggableState<T>,
    fling: &Rc<dyn FlingBehavior>,
    velocity: f32,
) -> Result<f32, MutationInterrupted> {
    let left = Rc::new(Cell::new(velocity));
    let inner = state.inner.clone();
    let noted = left.clone();
    state
        .anchored_drag(MutatePriority::Default, |drag, _| {
            let (inner, fling, noted) = (inner.clone(), fling.clone(), noted.clone());
            async move {
                let moving = ScrollScope::new(move |pixels| {
                    let new_offset = inner.new_offset_for_delta(pixels);
                    let consumed = new_offset - inner.offset.get();
                    drag.drag_to(new_offset, 0.0);
                    consumed
                });
                noted.set(fling.perform_fling(moving, velocity).await);
            }
        })
        .await?;
    Ok(left.get())
}

struct AnchoredElement<T: Clone + PartialEq + 'static> {
    state: AnchoredDraggableState<T>,
    orientation: Orientation,
    options: AnchoredDraggableOptions,
}

impl<T: Clone + PartialEq + 'static> AnchoredElement<T> {
    fn inputs(&self) -> AnchoredInputs<T> {
        AnchoredInputs {
            state: self.state.clone(),
            orientation: self.orientation,
            reverse_direction: self.options.reverse_direction,
            start_drag_immediately: self.options.start_drag_immediately,
            fling_behavior: self.options.fling_behavior.clone(),
            overscroll_effect: self.options.overscroll_effect.clone(),
        }
    }
}

impl<T: Clone + PartialEq + 'static> ModifierNodeElement for AnchoredElement<T> {
    type Node = DragGestureNode;

    fn create(&self) -> DragGestureNode {
        DragGestureNode::new(
            Rc::new(AnchoredBehavior {
                inputs: RefCell::new(self.inputs()),
                rtl: Cell::new(false),
                density: Cell::new(Density::new(1.0)),
            }),
            self.options.enabled,
            Some(self.orientation),
            self.options.interaction_source.clone(),
        )
    }

    fn update(&self, node: &mut DragGestureNode) {
        let behavior = node.behavior.clone();
        let behavior =
            behavior.as_any().downcast_ref::<AnchoredBehavior<T>>().expect("its element's");
        let reset = {
            let inputs = behavior.inputs.borrow();
            !inputs.state.same(&self.state)
                || inputs.reverse_direction != self.options.reverse_direction
        };
        *behavior.inputs.borrow_mut() = self.inputs();
        node.update(
            self.options.enabled,
            Some(self.orientation),
            self.options.interaction_source.clone(),
            reset,
        );
    }

    fn same(&self, other: &Self) -> bool {
        let fling_same = match (&self.options.fling_behavior, &other.options.fling_behavior) {
            (None, None) => true,
            (Some(one), Some(other)) => Rc::ptr_eq(one, other),
            _ => false,
        };
        let source_same =
            match (&self.options.interaction_source, &other.options.interaction_source) {
                (None, None) => true,
                (Some(one), Some(other)) => one.same(other),
                _ => false,
            };
        self.state.same(&other.state)
            && self.orientation == other.orientation
            && self.options.enabled == other.options.enabled
            && self.options.reverse_direction == other.options.reverse_direction
            && self.options.start_drag_immediately == other.options.start_drag_immediately
            && fling_same
            && source_same
            && same_overscroll(&self.options.overscroll_effect, &other.options.overscroll_effect)
    }

    fn name(&self) -> &'static str {
        "anchoredDraggable"
    }
}
