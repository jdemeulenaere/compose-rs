//! `remember_infinite_transition`, as Compose's `rememberInfiniteTransition`: values that animate
//! forever while in the composition (a pulsing badge, a spinner).
//!
//! As in Compose, a transition runs one task for all its values: each frame, it sets every
//! value still animating (`InfiniteTransition.run`'s frame loop). A screen of many transitions
//! resumes a task per transition, not per value (D420).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::task::{Poll, Waker};

use compose_runtime::{
    ComposeArg, DisposableEffect, LaunchedEffect, ObjectHandle, Owned, OwnedState, SideEffect,
    State, composable, remember, with_infinite_animation_frame_nanos,
};
use compose_ui::Color;

use crate::spec::{RepeatableSpec, VectorizedAnimationSpec};
use crate::vector::{Animate, AnimationVector};

/// Runs infinite animations while it is in the composition, as Compose's `InfiniteTransition`:
/// one task sets all its values each frame. A `Copy` handle to the transition its
/// [`remember_infinite_transition`] call remembers (D240).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InfiniteTransition {
    inner: ObjectHandle<TransitionInner>,
}

impl ComposeArg for InfiniteTransition {
    fn same(&self, other: &InfiniteTransition) -> bool {
        self == other
    }
}

struct TransitionInner {
    /// Its values, as `_animations`: added and removed as their calls enter and leave the
    /// composition.
    animations: RefCell<Vec<Rc<dyn TransitionAnimation>>>,
    /// Its task, waiting for a value to animate (none yet, or all finished): it asks for no
    /// frames meanwhile, as Compose's transition stops running (`isRunning`).
    idle: RefCell<Option<Waker>>,
}

/// A value of a transition, as `InfiniteTransition.TransitionAnimationState`, its type erased.
trait TransitionAnimation {
    /// Sets the value at this frame. Compose passes the transition's play time; this is the
    /// frame's time, so a value whose call moves to another transition goes on where it was.
    fn on_play_time_changed(&self, frame_time_nanos: u64);
    fn is_finished(&self) -> bool;
}

impl TransitionInner {
    /// Wakes the task if it waits for a value to animate.
    fn refresh(&self) {
        // Not borrowed while waking: the waker may run the host's wake handler.
        let waker = self.idle.borrow_mut().take();
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    fn add_animation(&self, animation: Rc<dyn TransitionAnimation>) {
        self.animations.borrow_mut().push(animation);
        self.refresh();
    }

    fn remove_animation(&self, animation: &Rc<dyn TransitionAnimation>) {
        self.animations.borrow_mut().retain(|kept| !Rc::ptr_eq(kept, animation));
    }

    fn is_running(&self) -> bool {
        self.animations.borrow().iter().any(|animation| !animation.is_finished())
    }

    /// Sets each value still animating, as `onFrame`.
    fn on_frame(&self, frame_time_nanos: u64) {
        // By index, without holding the list: a value's write runs no user code today, but a
        // borrow held across it would make any that adds a value panic.
        let mut index = 0;
        loop {
            let animation = self.animations.borrow().get(index).cloned();
            let Some(animation) = animation else { break };
            if !animation.is_finished() {
                animation.on_play_time_changed(frame_time_nanos);
            }
            index += 1;
        }
    }

    /// The transition's task, as `InfiniteTransition.run`'s `LaunchedEffect`: each frame, the
    /// values still animating move on; with none, it waits for one.
    async fn run(self: Rc<TransitionInner>) {
        loop {
            std::future::poll_fn(|context| {
                if self.is_running() {
                    Poll::Ready(())
                } else {
                    *self.idle.borrow_mut() = Some(context.waker().clone());
                    Poll::Pending
                }
            })
            .await;
            // An infinite animation's frame: a test rule advancing its clock by itself stops
            // it, as Compose's test `InfiniteAnimationPolicy` (D901).
            let time = with_infinite_animation_frame_nanos(|time| time).await;
            self.on_frame(time);
        }
    }
}

/// An infinite transition, as `rememberInfiniteTransition()`.
#[composable]
pub fn remember_infinite_transition(
    #[default("InfiniteTransition")] label: &'static str,
) -> InfiniteTransition {
    let _ = label;
    let transition = remember(|| {
        Owned::new(
            TransitionInner { animations: RefCell::new(Vec::new()), idle: RefCell::new(None) },
            |inner| InfiniteTransition { inner },
        )
    });
    let inner = transition.inner.try_get();
    LaunchedEffect(transition, async move {
        if let Some(inner) = inner {
            inner.run().await;
        }
    });
    transition
}

/// A value of a transition, as `TransitionAnimationState`.
struct TransitionAnimationState<T: Animate> {
    initial_value: RefCell<T>,
    target_value: RefCell<T>,
    animation_spec: RefCell<RepeatableSpec>,
    /// The values as vectors, converted once (a color's conversion is the dearest part of its
    /// frame).
    vectors: Cell<(AnimationVector, AnimationVector)>,
    /// The spec's duration for the values, computed when they change, not each frame.
    duration_nanos: Cell<i64>,
    value: OwnedState<T>,
    is_finished: Cell<bool>,
    /// Its play time starts at the next frame: it was just added or changed.
    start_on_the_next_frame: Cell<bool>,
    /// The frame time of its start.
    play_time_nanos_offset: Cell<u64>,
}

impl<T: Animate> TransitionAnimationState<T> {
    /// New values: it starts again from `initial_value` at the next frame, as `updateValues`.
    fn update_values(&self, initial_value: T, target_value: T, animation_spec: RepeatableSpec) {
        let (start, end) = (initial_value.to_vector(), target_value.to_vector());
        self.vectors.set((start, end));
        self.duration_nanos.set(animation_spec.duration_nanos(start, end, start.new_instance()));
        *self.initial_value.borrow_mut() = initial_value;
        *self.target_value.borrow_mut() = target_value;
        *self.animation_spec.borrow_mut() = animation_spec;
        self.is_finished.set(false);
        self.start_on_the_next_frame.set(true);
    }
}

impl<T: Animate> TransitionAnimation for TransitionAnimationState<T> {
    fn on_play_time_changed(&self, frame_time_nanos: u64) {
        if self.start_on_the_next_frame.replace(false) {
            self.play_time_nanos_offset.set(frame_time_nanos);
        }
        let play_time = frame_time_nanos.saturating_sub(self.play_time_nanos_offset.get());
        let (start, end) = self.vectors.get();
        // A value that isn't a number (NaN, infinite) can't be animated: it's at the target
        // at its first frame, as an `Animatable`'s.
        if start.as_slice().iter().chain(end.as_slice()).any(|value| !value.is_finite()) {
            let target = self.target_value.borrow().clone();
            self.value.set(target);
            self.is_finished.set(true);
            return;
        }
        // As Compose's `TargetBasedAnimation.getValueFromNanos`: at the target once it ends (a
        // finite repeatable's).
        let spec = self.animation_spec.borrow();
        let play_time = play_time.min(i64::MAX as u64) as i64;
        let velocity = start.new_instance();
        if play_time >= self.duration_nanos.get() {
            let target = self.target_value.borrow().clone();
            self.value.set(target);
            self.is_finished.set(true);
            return;
        }
        let value = spec.value_from_nanos(play_time, start, end, velocity);
        self.value.set(T::from_vector(value));
    }

    fn is_finished(&self) -> bool {
        self.is_finished.get()
    }
}

impl InfiniteTransition {
    /// A value going from `initial_value` to `target_value` along `animation_spec` forever, as
    /// `InfiniteTransition.animateValue`. Its play time starts at the first frame after its call
    /// enters the composition, and again when the values or the spec change (D420).
    pub fn animate_value<T: Animate + ComposeArg>(
        &self,
        initial_value: T,
        target_value: T,
        animation_spec: RepeatableSpec,
    ) -> State<T> {
        let animation = remember(|| {
            Rc::new(TransitionAnimationState {
                initial_value: RefCell::new(initial_value.clone()),
                target_value: RefCell::new(target_value.clone()),
                animation_spec: RefCell::new(animation_spec.clone()),
                vectors: Cell::new((initial_value.to_vector(), target_value.to_vector())),
                duration_nanos: Cell::new({
                    let start = initial_value.to_vector();
                    animation_spec.duration_nanos(
                        start,
                        target_value.to_vector(),
                        start.new_instance(),
                    )
                }),
                value: compose_runtime::mutable_state_of_with(
                    initial_value.clone(),
                    crate::vector::SameValuePolicy,
                ),
                is_finished: Cell::new(false),
                start_on_the_next_frame: Cell::new(true),
                play_time_nanos_offset: Cell::new(0),
            })
        });
        let state = animation.value.as_state();
        let transition = self.inner;
        let added: Rc<dyn TransitionAnimation> = animation.clone();
        // Keyed on the transition too: a call given another transition moves its value there
        // (on where it was), rather than staying with one that may leave the composition.
        DisposableEffect((added.clone(), *self), move |scope| {
            if let Some(inner) = transition.try_get() {
                inner.add_animation(added.clone());
            }
            scope.on_dispose(move || {
                if let Some(inner) = transition.try_get() {
                    inner.remove_animation(&added);
                }
            })
        });
        SideEffect(move || {
            let changed = !initial_value.same(&animation.initial_value.borrow())
                || !target_value.same(&animation.target_value.borrow())
                || animation_spec != *animation.animation_spec.borrow();
            if changed {
                animation.update_values(initial_value, target_value, animation_spec);
                if let Some(inner) = transition.try_get() {
                    inner.refresh();
                }
            }
        });
        state
    }

    /// As `InfiniteTransition.animateFloat`.
    pub fn animate_float(
        &self,
        initial_value: f32,
        target_value: f32,
        animation_spec: RepeatableSpec,
    ) -> State<f32> {
        self.animate_value(initial_value, target_value, animation_spec)
    }

    /// As `InfiniteTransition.animateColor`.
    pub fn animate_color(
        &self,
        initial_value: Color,
        target_value: Color,
        animation_spec: RepeatableSpec,
    ) -> State<Color> {
        self.animate_value(initial_value, target_value, animation_spec)
    }
}
