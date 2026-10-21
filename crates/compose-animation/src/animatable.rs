//! `Animatable`, as Compose's: a value animated by tasks, one animation at a time.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::{ComposeArg, OwnedState, State, mutable_state_of, with_frame_nanos};

use crate::spec::AnimationSpec;
use crate::vector::Animate;

/// Why an animation ended, as Compose's `AnimationEndReason`, and `Interrupted`: another
/// animation (or a snap, or a stop) took over, where Compose throws a cancellation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AnimationEndReason {
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
    velocity: Cell<[f32; 4]>,
    /// Bumped by each animation, snap and stop: an animation sees it is no longer current.
    generation: Cell<u64>,
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

/// Clears `is_running` when an animation ends however it ends (finished, or its task
/// cancelled), unless another one took over.
struct Running<T: Animate> {
    inner: Rc<Inner<T>>,
    generation: u64,
}

impl<T: Animate> Drop for Running<T> {
    fn drop(&mut self) {
        if self.inner.generation.get() == self.generation {
            self.inner.velocity.set([0.0; 4]);
            let _ = self.inner.is_running.try_set(false);
        }
    }
}

impl<T: Animate> Animatable<T> {
    /// An animatable at `initial`, as `Animatable(initialValue)`.
    #[track_caller]
    pub fn new(initial: T) -> Animatable<T> {
        Animatable {
            inner: Rc::new(Inner {
                target: RefCell::new(initial.clone()),
                value: mutable_state_of(initial),
                is_running: mutable_state_of(false),
                velocity: Cell::new([0.0; 4]),
                generation: Cell::new(0),
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

    /// Whether an animation is running (a state read).
    pub fn is_running(&self) -> bool {
        self.inner.is_running.get()
    }

    /// Animates to `target` along `spec`, from the current value at the current velocity, as
    /// `animateTo`. An animation running is interrupted. Each frame (`with_frame_nanos`) sets the
    /// value; the result says how it ended.
    pub async fn animate_to(
        &self,
        target: T,
        spec: impl Into<AnimationSpec>,
    ) -> AnimationResult<T> {
        let velocity = self.inner.velocity.get();
        self.animate_from(target, spec.into(), velocity).await
    }

    /// The same, starting at `initial_velocity`, as `animateTo(…, initialVelocity)`.
    pub async fn animate_to_with_velocity(
        &self,
        target: T,
        spec: impl Into<AnimationSpec>,
        initial_velocity: T,
    ) -> AnimationResult<T> {
        self.animate_from(target, spec.into(), initial_velocity.to_vector()).await
    }

    async fn animate_from(
        &self,
        target: T,
        spec: AnimationSpec,
        velocity: [f32; 4],
    ) -> AnimationResult<T> {
        let inner = &self.inner;
        let generation = inner.generation.get() + 1;
        inner.generation.set(generation);
        *inner.target.borrow_mut() = target.clone();
        let start = self.value().to_vector();
        let end = target.to_vector();
        inner.velocity.set(velocity);
        inner.is_running.set(true);
        let _running = Running { inner: inner.clone(), generation };
        let threshold = T::visibility_threshold();
        let mut started = None;
        loop {
            let time = with_frame_nanos(|time| time).await;
            if inner.generation.get() != generation {
                return AnimationResult {
                    end_reason: AnimationEndReason::Interrupted,
                    end_value: self.value(),
                };
            }
            let begin = *started.get_or_insert(time);
            let frame = spec.frame(time - begin, start, end, velocity, threshold);
            inner.value.set(T::from_vector(frame.values));
            inner.velocity.set(frame.velocities);
            if frame.finished {
                return AnimationResult {
                    end_reason: AnimationEndReason::Finished,
                    end_value: self.value(),
                };
            }
        }
    }

    /// Jumps to `target`, interrupting an animation, as `snapTo`.
    pub fn snap_to(&self, target: T) {
        let inner = &self.inner;
        inner.generation.set(inner.generation.get() + 1);
        *inner.target.borrow_mut() = target.clone();
        inner.velocity.set([0.0; 4]);
        inner.value.set(target);
        inner.is_running.set(false);
    }

    /// Stops the animation running, where it is, as `stop`.
    pub fn stop(&self) {
        let inner = &self.inner;
        inner.generation.set(inner.generation.get() + 1);
        inner.velocity.set([0.0; 4]);
        inner.is_running.set(false);
    }
}
