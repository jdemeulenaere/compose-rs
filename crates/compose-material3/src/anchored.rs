//! An offset dragged between anchors and settling at one, as Compose's
//! `AnchoredDraggableState`: what bottom sheets and swipe-to-dismiss boxes are built on.

use std::cell::{Cell, RefCell};

use compose_animation::{Animatable, AnimationEndReason, AnimationSpec};
use compose_runtime::{OwnedState, mutable_state_of};

/// The values are keys (`u8`, each component's enum as its discriminant), so this is compiled
/// once.
pub(crate) type Key = u8;

pub(crate) struct Anchored {
    /// The offset while dragged or animated (pixels).
    offset: Animatable<f32>,
    /// At rest at `settled`'s anchor (wherever the layout puts it), not at `offset`.
    resting: Cell<bool>,
    /// The values and their offsets, as the last layout found them.
    anchors: RefCell<Vec<(Key, f32)>>,
    /// As Compose's `currentValue`: where it last settled, or, while dragged, the anchor it was
    /// dragged half way to.
    current: OwnedState<Key>,
    /// As Compose's `settledValue`: where it last settled.
    settled: OwnedState<Key>,
    target: OwnedState<Key>,
    confirm_value_change: Box<dyn Fn(Key) -> bool>,
    /// Drags' settling animation, as `anchoredDraggableMotionSpec` (its owner can change it).
    spec: RefCell<AnimationSpec>,
}

impl Anchored {
    pub fn new(
        initial: Key,
        confirm_value_change: Box<dyn Fn(Key) -> bool>,
        spec: AnimationSpec,
    ) -> Anchored {
        Anchored {
            offset: Animatable::new(0.0),
            resting: Cell::new(true),
            anchors: RefCell::new(Vec::new()),
            current: mutable_state_of(initial),
            settled: mutable_state_of(initial),
            target: mutable_state_of(initial),
            confirm_value_change,
            spec: RefCell::new(spec),
        }
    }

    /// Where it last settled, or, while dragged, the anchor it was dragged half way to (a state
    /// read), as `currentValue`.
    pub fn current_value(&self) -> Key {
        self.current.get()
    }

    /// Where it last settled (a state read), as `settledValue`.
    pub fn settled_value(&self) -> Key {
        self.settled.get()
    }

    /// Whether it's at rest at its settled anchor (not dragged nor animating).
    pub fn is_resting(&self) -> bool {
        self.resting.get()
    }

    /// Where it's going, or the anchor it's been dragged nearest (a state read), as
    /// `targetValue`.
    pub fn target_value(&self) -> Key {
        self.target.get()
    }

    pub fn position_of(&self, value: Key) -> Option<f32> {
        self.anchors.borrow().iter().find(|(anchor, _)| *anchor == value).map(|&(_, at)| at)
    }

    pub fn has_anchor(&self, value: Key) -> bool {
        self.position_of(value).is_some()
    }

    /// The offset (state reads), NaN before the first layout, as `offset`.
    pub fn offset(&self) -> f32 {
        let dragged = self.offset.value();
        if self.resting.get() {
            self.position_of(self.settled.get()).unwrap_or(f32::NAN)
        } else {
            dragged
        }
    }

    /// The anchors the layout found: kept for drags and animations (the offset at rest follows
    /// them).
    /// Returns whether they changed.
    pub fn update_anchors(&self, anchors: Vec<(Key, f32)>) -> bool {
        let changed = *self.anchors.borrow() != anchors;
        if changed {
            *self.anchors.borrow_mut() = anchors;
        }
        changed
    }

    /// Moves by `delta` pixels, within the anchors, as a drag's `dispatchRawDelta`; returns
    /// how far it moved.
    pub fn drag_by(&self, delta: f32) -> f32 {
        let start = self.offset();
        if start.is_nan() {
            return 0.0;
        }
        let (min, max) = self.bounds();
        let offset = (start + delta).clamp(min, max);
        self.resting.set(false);
        self.offset.snap_to(offset);
        if let Some(nearest) = self.nearest(offset)
            && nearest != self.target.get()
        {
            self.target.set(nearest);
        }
        self.update_current(start, offset);
        offset - start
    }

    /// While dragged from `previous` to `offset`: the current value becomes the anchor in the
    /// drag's direction once the offset is half way to it from the current one, if
    /// `confirm_value_change` lets it, as Compose's `AnchoredDragScope.dragTo` (`updateIfNeeded`).
    fn update_current(&self, previous: f32, offset: f32) {
        let forward = offset >= previous;
        let current = self.current.get();
        let Some(current_at) = self.position_of(current) else { return };
        let (left, right) = if offset == current_at {
            let from = offset + if forward { 1.0 } else { -1.0 };
            let other = self.closest(from, forward).unwrap_or(current);
            if forward { (current, other) } else { (other, current) }
        } else {
            (
                self.closest(offset, false).unwrap_or(current),
                self.closest(offset, true).unwrap_or(current),
            )
        };
        let position = |value| self.position_of(value).unwrap_or(current_at);
        let distance = (position(left) - position(right)).abs();
        if (offset - current_at).abs() >= distance / 2.0 {
            let closest = if forward { right } else { left };
            if closest != current && (self.confirm_value_change)(closest) {
                self.current.set(closest);
            }
        }
    }

    /// The lowest and highest anchors.
    pub fn bounds(&self) -> (f32, f32) {
        let anchors = self.anchors.borrow();
        let min = anchors.iter().map(|&(_, at)| at).fold(f32::INFINITY, f32::min);
        let max = anchors.iter().map(|&(_, at)| at).fold(f32::NEG_INFINITY, f32::max);
        (min, max)
    }

    fn nearest(&self, offset: f32) -> Option<Key> {
        let anchors = self.anchors.borrow();
        anchors
            .iter()
            .min_by(|a, b| (a.1 - offset).abs().total_cmp(&(b.1 - offset).abs()))
            .map(|&(value, _)| value)
    }

    /// The closest anchor at or past `offset` upwards (greater offsets) or downwards, as
    /// `closestAnchor(position, searchUpwards)`.
    fn closest(&self, offset: f32, upwards: bool) -> Option<Key> {
        let anchors = self.anchors.borrow();
        anchors
            .iter()
            .filter(|&&(_, at)| if upwards { at >= offset } else { at <= offset })
            .min_by(|a, b| (a.1 - offset).abs().total_cmp(&(b.1 - offset).abs()))
            .map(|&(value, _)| value)
    }

    /// What drags settle with from now on.
    pub fn set_spec(&self, spec: AnimationSpec) {
        *self.spec.borrow_mut() = spec;
    }

    /// Whether an animation to an anchor runs (a state read), as `isAnimationRunning`.
    pub fn is_animation_running(&self) -> bool {
        self.offset.is_running()
    }

    /// Animates to `value`'s anchor (once a layout has found it), as `animateTo`; settled
    /// there when the animation finishes.
    pub async fn animate_to(&self, value: Key) {
        let spec = self.spec.borrow().clone();
        self.animate_to_with(value, spec).await;
    }

    /// [`animate_to`](Self::animate_to) along `spec`.
    pub async fn animate_to_with(&self, value: Key, spec: AnimationSpec) {
        // Before the first layout, wait for it (a few frames at most).
        for _ in 0..8 {
            if self.position_of(value).is_some() && !self.offset().is_nan() {
                break;
            }
            compose_runtime::with_frame_nanos(|_| ()).await;
        }
        let Some(target) = self.position_of(value) else {
            // Nowhere to go: the value is taken as is, the offset where it is, as Compose's.
            let at = self.offset();
            self.settle_at(value);
            if !at.is_nan() {
                self.offset.snap_to(at);
                self.resting.set(false);
            }
            return;
        };
        self.target.set(value);
        let start = self.offset();
        if self.resting.get() {
            self.offset.snap_to(if start.is_nan() { target } else { start });
            self.resting.set(false);
        }
        let result = self.offset.animate_to(target, spec).await;
        if result.end_reason == AnimationEndReason::Finished {
            self.settle_at(value);
        }
    }

    /// At `value` at once, as `snapTo`.
    pub fn snap_to(&self, value: Key) {
        if let Some(at) = self.position_of(value) {
            self.offset.snap_to(at);
        }
        self.target.set(value);
        self.settle_at(value);
    }

    fn settle_at(&self, value: Key) {
        self.resting.set(true);
        self.target.set(value);
        self.current.set(value);
        self.settled.set(value);
    }

    /// After a drag let go at `velocity` (pixels per second): to the next anchor in its
    /// direction if fast enough or dragged `positional_threshold` (of the distance to it) past
    /// the current one, else back, as Compose's `settle` (`computeTarget`); a value
    /// `confirm_value_change` refuses goes back to the current one.
    pub async fn settle(
        &self,
        velocity: f32,
        positional_threshold: &dyn Fn(f32) -> f32,
        velocity_threshold: f32,
    ) {
        let current = self.settled.get();
        let offset = self.offset();
        let target = self.compute_target(
            offset,
            current,
            velocity,
            positional_threshold,
            velocity_threshold,
        );
        let target = if (self.confirm_value_change)(target) { target } else { current };
        self.animate_to(target).await;
    }

    fn compute_target(
        &self,
        offset: f32,
        current: Key,
        velocity: f32,
        positional_threshold: &dyn Fn(f32) -> f32,
        velocity_threshold: f32,
    ) -> Key {
        let Some(at) = self.position_of(current) else { return current };
        if offset.is_nan() || at == offset {
            return current;
        }
        if at < offset {
            // Towards greater offsets.
            if velocity >= velocity_threshold {
                return self.closest(offset, true).unwrap_or(current);
            }
            let Some(upper) = self.closest(offset, true) else { return current };
            let distance = (self.position_of(upper).unwrap_or(at) - at).abs();
            let threshold = (at + positional_threshold(distance).abs()).abs();
            if offset < threshold { current } else { upper }
        } else {
            if velocity <= -velocity_threshold {
                return self.closest(offset, false).unwrap_or(current);
            }
            let Some(lower) = self.closest(offset, false) else { return current };
            let distance = (at - self.position_of(lower).unwrap_or(at)).abs();
            let threshold = (at - positional_threshold(distance).abs()).abs();
            if offset < 0.0 {
                if offset.abs() < threshold { current } else { lower }
            } else if offset > threshold {
                current
            } else {
                lower
            }
        }
    }
}
