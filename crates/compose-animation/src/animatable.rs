//! `Animatable`, as Compose's: a value animated by tasks, one animation at a time.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::{ComposeArg, OwnedState, State, mutable_state_of, with_frame_nanos};

use crate::animation::{Animation, TargetBasedAnimation};
use crate::spec::{AnimationConstants, AnimationSpec};
use crate::vector::{Animate, AnimationVector};

/// Why an animation ended, as Compose's `AnimationEndReason`, and `Interrupted`: another
/// animation (or a snap, or a stop) took over, where Compose throws a cancellation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AnimationEndReason {
    /// It reached a bound of its `Animatable` (`update_bounds`), and stopped there, as
    /// `BoundReached`.
    BoundReached,
    Finished,
    Interrupted,
}

/// How an animation ended, as Compose's `AnimationResult`: the reason and the value then.
#[derive(Clone, PartialEq, Debug)]
pub struct AnimationResult<T> {
    pub end_reason: AnimationEndReason,
    pub end_value: T,
}

/// A value animated by tasks, as Compose's `Animatable`: `animate_to` runs an animation (a new one
/// interrupts the one running, keeping its velocity), `snap_to` jumps. The value is state:
/// reading it registers the reader, so each frame of an animation recomposes, re-measures or
/// redraws what read it.
pub struct Animatable<T: Animate> {
    inner: Rc<Inner<T>>,
}

struct Inner<T: Animate> {
    value: OwnedState<T>,
    is_running: OwnedState<bool>,
    target: RefCell<T>,
    /// Per component, per second.
    velocity: Cell<AnimationVector>,
    /// The frame time the running animation last set the value at, as
    /// `AnimationState.lastFrameTimeNanos`: an animation that interrupts it starts there, not
    /// at the next frame. `UNSPECIFIED_TIME` when none runs.
    last_frame_time: Cell<i64>,
    /// Bumped by each animation, snap and stop: an animation sees it is no longer current.
    generation: Cell<u64>,
    /// The bounds per component (infinite when unbounded), as `lowerBound` and `upperBound`.
    bounds: RefCell<Bounds<T>>,
    visibility_threshold: Option<T>,
    label: &'static str,
}

struct Bounds<T> {
    lower: Option<T>,
    upper: Option<T>,
    lower_vector: Option<AnimationVector>,
    upper_vector: Option<AnimationVector>,
}

impl<T: Animate> Clone for Animatable<T> {
    fn clone(&self) -> Self {
        Animatable { inner: self.inner.clone() }
    }
}

impl<T: Animate> ComposeArg for Animatable<T> {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl<T: Animate + std::fmt::Debug> std::fmt::Debug for Animatable<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Animatable({:?})", self.inner.value.with(|value| value.clone()))
    }
}

/// Ends an animation however it ends (finished, or its task cancelled), unless another one took
/// over, as Compose's `endAnimation`: no velocity, no last frame, not running.
struct Running<T: Animate> {
    inner: Rc<Inner<T>>,
    generation: u64,
}

impl<T: Animate> Drop for Running<T> {
    fn drop(&mut self) {
        if self.inner.generation.get() == self.generation {
            self.inner.end_animation();
        }
    }
}

impl<T: Animate> Inner<T> {
    fn end_animation(&self) {
        let velocity = self.velocity.get();
        self.velocity.set(velocity.new_instance());
        self.last_frame_time.set(AnimationConstants::UNSPECIFIED_TIME);
        let _ = self.is_running.try_set(false);
    }

    /// `value` within the bounds, as `clampToBounds`.
    fn clamp_to_bounds(&self, value: T) -> T {
        let bounds = self.bounds.borrow();
        if bounds.lower_vector.is_none() && bounds.upper_vector.is_none() {
            return value;
        }
        let mut vector = value.to_vector();
        let mut clamped = false;
        for index in 0..vector.size() {
            let lower = bounds.lower_vector.map_or(f32::NEG_INFINITY, |lower| lower[index]);
            let upper = bounds.upper_vector.map_or(f32::INFINITY, |upper| upper[index]);
            if vector[index] < lower || vector[index] > upper {
                clamped = true;
                vector[index] = vector[index].clamp(lower, upper);
            }
        }
        if clamped { T::from_vector(vector) } else { value }
    }
}

impl<T: Animate> Animatable<T> {
    /// An animatable at `initial`, as `Animatable(initialValue)`.
    #[track_caller]
    pub fn new(initial: T) -> Animatable<T> {
        Animatable::new_with(initial, None, "Animatable")
    }

    /// An animatable at `initial`, whose default spring ([`default_spring_spec`]) ends within
    /// `visibility_threshold` of its targets, as `Animatable(initialValue, typeConverter,
    /// visibilityThreshold, label)`.
    ///
    /// [`default_spring_spec`]: Animatable::default_spring_spec
    #[track_caller]
    pub fn new_with(
        initial: T,
        visibility_threshold: Option<T>,
        label: &'static str,
    ) -> Animatable<T> {
        let velocity = initial.to_vector().new_instance();
        Animatable {
            inner: Rc::new(Inner {
                target: RefCell::new(initial.clone()),
                value: compose_runtime::mutable_state_of_with(
                    initial,
                    crate::vector::SameValuePolicy,
                ),
                is_running: mutable_state_of(false),
                velocity: Cell::new(velocity),
                last_frame_time: Cell::new(AnimationConstants::UNSPECIFIED_TIME),
                generation: Cell::new(0),
                bounds: RefCell::new(Bounds {
                    lower: None,
                    upper: None,
                    lower_vector: None,
                    upper_vector: None,
                }),
                visibility_threshold,
                label,
            }),
        }
    }

    /// The current value (a state read).
    pub fn value(&self) -> T {
        self.inner.value.with(|value| value.clone())
    }

    /// The value as a `Copy` read-only state handle.
    pub fn as_state(&self) -> State<T> {
        self.inner.value.as_state()
    }

    /// Where the running (or last) animation goes.
    pub fn target_value(&self) -> T {
        self.inner.target.borrow().clone()
    }

    /// The velocity, per second, as `Animatable.velocity`.
    pub fn velocity(&self) -> T {
        T::from_vector(self.inner.velocity.get())
    }

    /// The velocity per component, as `velocityVector`.
    pub fn velocity_vector(&self) -> AnimationVector {
        self.inner.velocity.get()
    }

    /// Whether an animation is running (a state read).
    pub fn is_running(&self) -> bool {
        self.inner.is_running.get()
    }

    /// Its name for tools, as `label`.
    pub fn label(&self) -> &'static str {
        self.inner.label
    }

    /// The spring `animate_to` uses in Compose when given no spec: the default spring, with
    /// this animatable's visibility threshold.
    pub fn default_spring_spec(&self) -> crate::spec::SpringSpec {
        match &self.inner.visibility_threshold {
            Some(threshold) => crate::spec::spring().visibility_threshold(threshold.clone()),
            None => crate::spec::spring(),
        }
    }

    /// The lower bound, as `lowerBound`.
    pub fn lower_bound(&self) -> Option<T> {
        self.inner.bounds.borrow().lower.clone()
    }

    /// The upper bound, as `upperBound`.
    pub fn upper_bound(&self) -> Option<T> {
        self.inner.bounds.borrow().upper.clone()
    }

    /// Bounds the value per component (`None`: unbounded on that side), as `updateBounds`: an
    /// animation reaching a bound stops there (`AnimationEndReason::BoundReached`), a snap is
    /// clamped, and a value at rest out of them is clamped now. The lower bound can't be above
    /// the upper on any component.
    pub fn update_bounds(&self, lower_bound: Option<T>, upper_bound: Option<T>) {
        let lower_vector = lower_bound.as_ref().map(Animate::to_vector);
        let upper_vector = upper_bound.as_ref().map(Animate::to_vector);
        if let (Some(lower), Some(upper)) = (lower_vector, upper_vector) {
            for index in 0..lower.size() {
                assert!(
                    lower[index] <= upper[index],
                    "the lower bound must be no greater than the upper bound on all components: \
                     {lower:?} is greater than {upper:?} on component {index}"
                );
            }
        }
        *self.inner.bounds.borrow_mut() =
            Bounds { lower: lower_bound, upper: upper_bound, lower_vector, upper_vector };
        if !self.inner.is_running.get() {
            let value = self.value();
            let clamped = self.inner.clamp_to_bounds(value.clone());
            if clamped != value {
                self.inner.value.set(clamped);
            }
        }
    }

    /// Animates to `target` along `spec`, from the current value at the current velocity, as
    /// `animateTo`. An animation running is interrupted: this one starts at the frame it last
    /// set the value at, as Compose's. Each frame (`with_frame_nanos`) sets the value; the
    /// result says how it ended.
    pub async fn animate_to(
        &self,
        target: T,
        spec: impl Into<AnimationSpec>,
    ) -> AnimationResult<T> {
        let velocity = self.inner.velocity.get();
        self.animate_from(target, spec.into(), velocity, &mut |_| {}).await
    }

    /// The same, starting at `initial_velocity`, as `animateTo(…, initialVelocity)`.
    pub async fn animate_to_with_velocity(
        &self,
        target: T,
        spec: impl Into<AnimationSpec>,
        initial_velocity: T,
    ) -> AnimationResult<T> {
        self.animate_from(target, spec.into(), initial_velocity.to_vector(), &mut |_| {}).await
    }

    /// The same, `block` called with this animatable after each frame sets the value, as
    /// `animateTo(targetValue, animationSpec, initialVelocity, block)`.
    pub async fn animate_to_with(
        &self,
        target: T,
        spec: impl Into<AnimationSpec>,
        initial_velocity: T,
        mut block: impl FnMut(&Animatable<T>),
    ) -> AnimationResult<T> {
        self.animate_from(target, spec.into(), initial_velocity.to_vector(), &mut block).await
    }

    async fn animate_from(
        &self,
        target: T,
        spec: AnimationSpec,
        velocity: AnimationVector,
        block: &mut dyn FnMut(&Animatable<T>),
    ) -> AnimationResult<T> {
        // An interruption-aware spec's interruption spec when one runs, as Compose's.
        let spec = spec.resolve(self.inner.is_running.get());
        let animation =
            TargetBasedAnimation::with_velocity_vector(spec, self.value(), target, velocity);
        self.run_animation(&animation, velocity, block).await
    }

    async fn run_animation(
        &self,
        animation: &dyn Animation<T>,
        velocity: AnimationVector,
        block: &mut dyn FnMut(&Animatable<T>),
    ) -> AnimationResult<T> {
        let inner = &self.inner;
        // Read before taking over: the frame the running animation (if any) is at.
        let start_time = inner.last_frame_time.get();
        let generation = inner.generation.get() + 1;
        inner.generation.set(generation);
        let target = animation.target_value();
        *inner.target.borrow_mut() = target.clone();
        inner.velocity.set(velocity);
        inner.is_running.set(true);
        let _running = Running { inner: inner.clone(), generation };
        let interrupted = || AnimationResult {
            end_reason: AnimationEndReason::Interrupted,
            end_value: self.value(),
        };
        // A value that isn't a number (NaN, infinite) can't be animated: it's at the target at
        // once (a spring would never come to rest), at the next frame.
        let start = self.value().to_vector();
        let end = target.to_vector();
        if start.as_slice().iter().chain(end.as_slice()).any(|value| !value.is_finite()) {
            let _ = with_frame_nanos(|time| time).await;
            if inner.generation.get() != generation {
                return interrupted();
            }
            inner.value.set(target);
            return AnimationResult {
                end_reason: AnimationEndReason::Finished,
                end_value: self.value(),
            };
        }
        // Interrupting: at play time 0 at once, at the interrupted animation's last frame.
        let mut started = None;
        if start_time != AnimationConstants::UNSPECIFIED_TIME {
            started = Some(start_time);
            let ended = self.frame(animation, start_time, start_time);
            block(self);
            if let Some(reason) = ended {
                return AnimationResult { end_reason: reason, end_value: self.value() };
            }
        }
        // An infinite animation waits for its frames as one, which a test rule's policy can
        // stop (Compose's `callWithFrameNanos`, D901).
        let infinite = animation.is_infinite();
        loop {
            let time = if infinite {
                compose_runtime::with_infinite_animation_frame_nanos(|time| time).await
            } else {
                with_frame_nanos(|time| time).await
            } as i64;
            if inner.generation.get() != generation {
                return interrupted();
            }
            let begin = *started.get_or_insert(time);
            let ended = self.frame(animation, time, begin);
            block(self);
            if let Some(reason) = ended {
                return AnimationResult { end_reason: reason, end_value: self.value() };
            }
        }
    }

    /// Sets the value and velocity at `frame_time`, as Compose's `doAnimationFrame`: how the
    /// animation ended, if it did.
    fn frame(
        &self,
        animation: &dyn Animation<T>,
        frame_time: i64,
        start_time: i64,
    ) -> Option<AnimationEndReason> {
        let inner = &self.inner;
        let play_time = frame_time - start_time;
        inner.last_frame_time.set(frame_time);
        let value = animation.value_from_nanos(play_time);
        inner.velocity.set(animation.velocity_vector_from_nanos(play_time));
        let clamped = inner.clamp_to_bounds(value.clone());
        if clamped != value {
            inner.value.set(clamped);
            return Some(AnimationEndReason::BoundReached);
        }
        inner.value.set(value);
        animation.is_finished_from_nanos(play_time).then_some(AnimationEndReason::Finished)
    }

    /// Decays from the current value at `initial_velocity` along `spec`, stopping at a bound if
    /// it reaches one, as `animateDecay`: the target is where it stops.
    pub async fn animate_decay(
        &self,
        initial_velocity: T,
        spec: &crate::decay::DecayAnimationSpec,
    ) -> AnimationResult<T> {
        let velocity = initial_velocity.to_vector();
        let animation = crate::decay::DecayAnimation::with_velocity_vector(
            spec.clone(),
            self.value(),
            velocity,
        );
        self.run_animation(&animation, velocity, &mut |_| {}).await
    }

    /// The same, `block` called with this animatable after each frame sets the value, as
    /// `animateDecay(initialVelocity, animationSpec, block)`.
    pub async fn animate_decay_with(
        &self,
        initial_velocity: T,
        spec: &crate::decay::DecayAnimationSpec,
        mut block: impl FnMut(&Animatable<T>),
    ) -> AnimationResult<T> {
        let velocity = initial_velocity.to_vector();
        let animation = crate::decay::DecayAnimation::with_velocity_vector(
            spec.clone(),
            self.value(),
            velocity,
        );
        self.run_animation(&animation, velocity, &mut block).await
    }

    /// Jumps to `target` (clamped to the bounds), interrupting an animation, as `snapTo`.
    pub fn snap_to(&self, target: T) {
        let inner = &self.inner;
        inner.generation.set(inner.generation.get() + 1);
        inner.end_animation();
        let target = inner.clamp_to_bounds(target);
        *inner.target.borrow_mut() = target.clone();
        inner.value.set(target);
    }

    /// Stops the animation running, where it is, as `stop`.
    pub fn stop(&self) {
        let inner = &self.inner;
        inner.generation.set(inner.generation.get() + 1);
        inner.end_animation();
    }
}
