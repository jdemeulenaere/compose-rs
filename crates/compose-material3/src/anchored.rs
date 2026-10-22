//! An offset dragged between anchors and settling at one, as Compose's
//! `AnchoredDraggableState`: what bottom sheets and swipe-to-dismiss boxes are built on.

use std::cell::{Cell, RefCell};

use compose_animation::{Animatable, AnimationEndReason, AnimationSpec};
use compose_runtime::{OwnedState, mutable_state_of};

pub(crate) struct Anchored<T: Copy + PartialEq + 'static> {
    /// The offset while dragged or animated (pixels).
    offset: Animatable<f32>,
    /// At rest at `current`'s anchor (wherever the layout puts it), not at `offset`.
    resting: Cell<bool>,
    /// The values and their offsets, as the last layout found them.
    anchors: RefCell<Vec<(T, f32)>>,
    current: OwnedState<T>,
    target: OwnedState<T>,
    confirm_value_change: Box<dyn Fn(T) -> bool>,
    spec: AnimationSpec,
}

impl<T: Copy + PartialEq + 'static> Anchored<T> {
    pub fn new(
        initial: T,
        confirm_value_change: Box<dyn Fn(T) -> bool>,
        spec: AnimationSpec,
    ) -> Anchored<T> {
        Anchored {
            offset: Animatable::new(0.0),
            resting: Cell::new(true),
            anchors: RefCell::new(Vec::new()),
            current: mutable_state_of(initial),
            target: mutable_state_of(initial),
            confirm_value_change,
            spec,
        }
    }

    /// Where it last settled (a state read), as `currentValue`.
    pub fn current_value(&self) -> T {
        self.current.get()
    }

    /// Where it's going, or the anchor it's been dragged nearest (a state read), as
    /// `targetValue`.
    pub fn target_value(&self) -> T {
        self.target.get()
    }

    pub fn position_of(&self, value: T) -> Option<f32> {
        self.anchors.borrow().iter().find(|(anchor, _)| *anchor == value).map(|&(_, at)| at)
    }

    pub fn has_anchor(&self, value: T) -> bool {
        self.position_of(value).is_some()
    }

    /// The offset (state reads), NaN before the first layout, as `offset`.
    pub fn offset(&self) -> f32 {
        let dragged = self.offset.value();
        if self.resting.get() {
            self.position_of(self.current.get()).unwrap_or(f32::NAN)
        } else {
            dragged
        }
    }

    /// The anchors the layout found: kept for drags and animations (the offset at rest follows
    /// them).
    pub fn update_anchors(&self, anchors: Vec<(T, f32)>) {
        if *self.anchors.borrow() != anchors {
            *self.anchors.borrow_mut() = anchors;
        }
    }

    /// Moves by `delta` pixels, within the anchors, as a drag's `dispatchRawDelta`.
    pub fn drag_by(&self, delta: f32) {
        let start = self.offset();
        if start.is_nan() {
            return;
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
    }

    fn bounds(&self) -> (f32, f32) {
        let anchors = self.anchors.borrow();
        let min = anchors.iter().map(|&(_, at)| at).fold(f32::INFINITY, f32::min);
        let max = anchors.iter().map(|&(_, at)| at).fold(f32::NEG_INFINITY, f32::max);
        (min, max)
    }

    fn nearest(&self, offset: f32) -> Option<T> {
        let anchors = self.anchors.borrow();
        anchors
            .iter()
            .min_by(|a, b| (a.1 - offset).abs().total_cmp(&(b.1 - offset).abs()))
            .map(|&(value, _)| value)
    }

    /// The closest anchor at or past `offset` upwards (greater offsets) or downwards, as
    /// `closestAnchor(position, searchUpwards)`.
    fn closest(&self, offset: f32, upwards: bool) -> Option<T> {
        let anchors = self.anchors.borrow();
        anchors
            .iter()
            .filter(|&&(_, at)| if upwards { at >= offset } else { at <= offset })
            .min_by(|a, b| (a.1 - offset).abs().total_cmp(&(b.1 - offset).abs()))
            .map(|&(value, _)| value)
    }

    /// Animates to `value`'s anchor (once a layout has found it), as `animateTo`; settled
    /// there when the animation finishes.
    pub async fn animate_to(&self, value: T) {
        // Before the first layout, wait for it (a few frames at most).
        for _ in 0..8 {
            if self.position_of(value).is_some() && !self.offset().is_nan() {
                break;
            }
            compose_runtime::with_frame_nanos(|_| ()).await;
        }
        let Some(target) = self.position_of(value) else {
            // Nowhere to go: the value is taken as is, as Compose's.
            self.target.set(value);
            self.current.set(value);
            return;
        };
        self.target.set(value);
        let start = self.offset();
        if self.resting.get() {
            self.offset.snap_to(if start.is_nan() { target } else { start });
            self.resting.set(false);
        }
        let result = self.offset.animate_to(target, self.spec).await;
        if result.end_reason == AnimationEndReason::Finished {
            self.settle_at(value);
        }
    }

    /// At `value` at once, as `snapTo`.
    pub fn snap_to(&self, value: T) {
        if let Some(at) = self.position_of(value) {
            self.offset.snap_to(at);
        }
        self.target.set(value);
        self.settle_at(value);
    }

    fn settle_at(&self, value: T) {
        self.resting.set(true);
        self.target.set(value);
        self.current.set(value);
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
        let current = self.current.get();
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
        current: T,
        velocity: f32,
        positional_threshold: &dyn Fn(f32) -> f32,
        velocity_threshold: f32,
    ) -> T {
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
