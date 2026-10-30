//! `Transition`, as Compose's: values animating together when a state changes, each from its
//! value for the state before to its value for the new one, on one play-time clock.
//!
//! As Compose's (D803): a transition runs one task, which each frame sets the play time and
//! every value still animating (`onFrame`); a value is a `TargetBasedAnimation` from where it is
//! to its target, delayed by the play time it was given at, so a value added or changed
//! mid-way starts there; child transitions (`create_child_transition`) play on their parent's
//! clock. `update_transition` follows a target; `remember_transition` a
//! [`MutableTransitionState`].

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::task::{Poll, Waker};

use compose_runtime::{
    ComposeArg, DisposableEffect, LaunchedEffect, OwnedState, State, mutable_state_of, remember,
    remember_keyed, with_frame_nanos,
};
use compose_ui::{Color, Dp, IntOffset, IntSize, Offset, Rect, Size};

use crate::animation::{Animation, TargetBasedAnimation};
use crate::spec::{AnimationConstants, AnimationSpec, spring};
use crate::vector::{Animate, AnimationVector};

/// A change of a transition: from `initial_state` to `target_state`, as Compose's
/// `Transition.Segment`.
#[derive(Clone, PartialEq, Debug)]
pub struct Segment<S> {
    pub initial_state: S,
    pub target_state: S,
}

impl<S: PartialEq> Segment<S> {
    /// Whether this change goes from `initial` to `target`, as `initial isTransitioningTo
    /// target`.
    pub fn is_transitioning_to(&self, initial: &S, target: &S) -> bool {
        *initial == self.initial_state && *target == self.target_state
    }
}

/// A transition's state a caller holds, as Compose's `MutableTransitionState`: set its
/// `target_state` and a transition remembered with it ([`remember_transition`]) animates to it;
/// its `current_state` is where the transition is.
pub struct MutableTransitionState<S: 'static> {
    inner: Rc<StateInner<S>>,
}

struct StateInner<S: 'static> {
    current: OwnedState<S>,
    target: OwnedState<S>,
    /// A plain copy of `current`, read without observing it.
    current_untracked: RefCell<S>,
    is_running: OwnedState<bool>,
}

impl<S: 'static> Clone for MutableTransitionState<S> {
    fn clone(&self) -> Self {
        MutableTransitionState { inner: self.inner.clone() }
    }
}

impl<S: 'static> ComposeArg for MutableTransitionState<S> {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

/// The same state holder, as Compose's object identity.
impl<S: 'static> PartialEq for MutableTransitionState<S> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl<S: 'static> std::fmt::Debug for MutableTransitionState<S> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("MutableTransitionState")
    }
}

impl<S: Clone + PartialEq + 'static> MutableTransitionState<S> {
    /// A state at `initial_state`, as `MutableTransitionState(initialState)`: set another target
    /// to animate.
    #[track_caller]
    pub fn new(initial_state: S) -> MutableTransitionState<S> {
        MutableTransitionState {
            inner: Rc::new(StateInner {
                current: mutable_state_of(initial_state.clone()),
                target: mutable_state_of(initial_state.clone()),
                current_untracked: RefCell::new(initial_state),
                is_running: mutable_state_of(false),
            }),
        }
    }

    /// Where the transition is, as `currentState` (a state read).
    pub fn current_state(&self) -> S {
        self.inner.current.get()
    }

    /// Where it goes, as `targetState` (a state read).
    pub fn target_state(&self) -> S {
        self.inner.target.get()
    }

    /// Animates the transition remembered with this state to `target_state`: in a handler, or
    /// in composition before the transition's call, as Compose allows.
    pub fn set_target_state(&self, target_state: S) {
        self.inner.target.set(target_state);
    }

    /// Whether the transition is at its target and not running, as `isIdle` (state reads).
    pub fn is_idle(&self) -> bool {
        self.current_state() == self.target_state() && !self.inner.is_running.get()
    }

    fn current_untracked(&self) -> S {
        self.inner.current_untracked.borrow().clone()
    }

    /// Sets the current state, in composition or out of it.
    fn set_current(&self, current: S) {
        *self.inner.current_untracked.borrow_mut() = current.clone();
        self.inner.current.set(current);
    }

    fn set_running(&self, running: bool) {
        self.inner.is_running.set(running);
    }
}

/// What a transition follows, as Compose's `TransitionState`: a [`MutableTransitionState`] or a
/// [`SeekableTransitionState`](crate::SeekableTransitionState).
pub(crate) enum StateHolder<S: 'static> {
    Mutable(MutableTransitionState<S>),
    Seekable(crate::seekable::SeekableTransitionState<S>),
}

impl<S: Clone + PartialEq + 'static> StateHolder<S> {
    fn current_state(&self) -> S {
        match self {
            StateHolder::Mutable(state) => state.current_state(),
            StateHolder::Seekable(state) => state.current_state(),
        }
    }

    fn current_untracked(&self) -> S {
        match self {
            StateHolder::Mutable(state) => state.current_untracked(),
            StateHolder::Seekable(state) => state.current_untracked(),
        }
    }

    fn set_current(&self, current: S) {
        match self {
            StateHolder::Mutable(state) => state.set_current(current),
            StateHolder::Seekable(state) => state.set_current(current),
        }
    }

    fn set_running(&self, running: bool) {
        match self {
            StateHolder::Mutable(state) => state.set_running(running),
            // Compose's `isRunning` of a seekable state is internal and never read.
            StateHolder::Seekable(_) => {}
        }
    }
}

/// A state's change animating values together, as Compose's `Transition`.
pub struct Transition<S: 'static> {
    inner: Rc<TransitionInner<S>>,
}

impl<S: 'static> Clone for Transition<S> {
    fn clone(&self) -> Self {
        Transition { inner: self.inner.clone() }
    }
}

impl<S: 'static> ComposeArg for Transition<S> {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

pub(crate) struct TransitionInner<S: 'static> {
    state: StateHolder<S>,
    label: Option<&'static str>,
    /// The transition whose clock it plays on, as `parentTransition`.
    parent: Option<Weak<dyn TransitionNode>>,
    /// Where it goes, as `targetState`.
    target: OwnedState<S>,
    /// A plain copy of `target`, read without observing it.
    target_untracked: RefCell<S>,
    segment: OwnedState<Segment<S>>,
    /// The frame time it started at, as `startTimeNanos`: `UNSPECIFIED_TIME` when not running.
    start_time_nanos: OwnedState<i64>,
    /// A plain copy of `start_time_nanos`, read without observing it (as in composition, where
    /// the transition's own reads would recompose its caller when it starts and ends).
    start_time_untracked: Cell<i64>,
    /// The play time of its last frame, as `playTimeNanos` (a child's is its parent's).
    play_time_nanos: OwnedState<i64>,
    /// A plain copy of `play_time_nanos`.
    play_time_untracked: Cell<i64>,
    /// An animation changed while the transition was at rest: it runs to play it, as
    /// `updateChildrenNeeded`.
    update_children_needed: Cell<bool>,
    animations: RefCell<Vec<Rc<dyn AnimationEntry>>>,
    transitions: RefCell<Vec<Rc<dyn TransitionNode>>>,
    /// Bumped when a value or child transition joins or leaves: what tools read the lists
    /// through, as Compose's state lists.
    lists_version: OwnedState<u32>,
    lists_count: Cell<u32>,
    /// Bumped when a value's duration changes: what `total_duration_nanos` reads through, as
    /// Compose's derived `totalDurationNanos`.
    durations_version: OwnedState<u32>,
    durations_count: Cell<u32>,
    /// Its task, waiting for something to animate.
    idle: RefCell<Option<Waker>>,
}

/// A transition, its state type erased: what a parent drives and a value asks.
trait TransitionNode {
    fn label(&self) -> Option<String>;
    /// Its values, for tools.
    fn animation_entries(&self) -> Vec<Rc<dyn AnimationEntry>>;
    /// Its child transitions, for tools.
    fn child_transitions(&self) -> Vec<Rc<dyn TransitionNode>>;
    /// Whether its values are animating (a state read).
    fn is_running(&self) -> bool;
    /// The play time (a state read).
    fn play_time_nanos(&self) -> i64;
    /// The same, without observing it: what a value changed in composition starts at.
    fn play_time_untracked(&self) -> i64;
    fn on_child_animation_updated(&self);
    /// A value's duration changed.
    fn on_duration_changed(&self);
    /// Whether its current state isn't its target.
    fn is_changing(&self) -> bool;
    fn on_frame_play_time(&self, play_time_nanos: i64, scale_to_end: bool);
    fn on_transition_end(&self);
    fn total_duration_nanos(&self) -> i64;
    /// Its values at `play_time_nanos`, seeking (`seekAnimations`).
    fn seek_animations(&self, play_time_nanos: i64);
    /// Its values continuing from where they are along `state` (`setInitialAnimations`).
    fn set_initial_animations(&self, state: &Rc<SeekingAnimationState>);
    fn reset_animation_fraction(&self, fraction: f32);
    fn clear_initial_animations(&self);
    fn update_initial_values(&self);
    /// Whether a value or a child's goes on along an initial-value animation, as
    /// `hasInitialValueAnimations`.
    fn has_initial_value_animations(&self) -> bool;
}

/// A value of a transition, its type erased, as `TransitionAnimationState`.
trait AnimationEntry: ToolingEntry {
    fn on_play_time_changed(&self, play_time_nanos: i64, scale_to_end: bool);
    fn is_finished(&self) -> bool;
    fn reset_animation(&self);
    /// Its value at `play_time_nanos`, seeking, as `seekTo`.
    fn seek_to(&self, play_time_nanos: i64);
    /// Its start moved along the initial-value animation, as `updateInitialValue`.
    fn update_initial_value(&self);
    /// Snapped to its current or target value, or to a fraction, as `resetAnimationValue`.
    fn reset_animation_value(&self, fraction: f32);
    /// Its animation continued from where it is along `state`, as `setInitialValueAnimation`.
    fn set_initial_value_animation(&self, state: &Rc<SeekingAnimationState>);
    fn clear_initial_animation(&self);
    /// Whether it goes on along an initial-value animation (`initialValueState != null`).
    fn has_initial_value_animation(&self) -> bool;
}

/// An animation of a seekable transition's fraction, as Compose's
/// `SeekableTransitionState.SeekingAnimationState`: a running `animate_to`, or an animation of
/// the values from where an interrupted change left them (an initial-value animation).
#[derive(Default)]
pub(crate) struct SeekingAnimationState {
    /// The progress along its spec, or its duration without one.
    pub(crate) progress_nanos: Cell<i64>,
    /// Its spec, of a 1-component vector, or `None`: linear over its duration.
    pub(crate) animation_spec: RefCell<Option<AnimationSpec>>,
    pub(crate) is_complete: Cell<bool>,
    /// The fraction it is at.
    pub(crate) value: Cell<f32>,
    pub(crate) start: Cell<f32>,
    pub(crate) initial_velocity: Cell<Option<f32>>,
    /// The transition's total duration when it was made.
    pub(crate) duration_nanos: Cell<i64>,
    /// Its spec's duration, kept (a spring's takes time to compute).
    pub(crate) animation_spec_duration: Cell<i64>,
}

/// A value of a transition (or of an infinite transition), its type erased: what tools see.
pub(crate) trait ToolingEntry {
    fn duration_nanos(&self) -> i64;
    fn label(&self) -> String;
    /// The value (a state read), boxed.
    fn value_any(&self) -> Box<dyn Any>;
    fn target_value_any(&self) -> Box<dyn Any>;
    fn animation_spec(&self) -> AnimationSpec;
}

/// A value of a transition, for tools (an inspector), as Compose's
/// `Transition.TransitionAnimationState`: [`Transition::animations`] lists them.
#[derive(Clone)]
pub struct TransitionAnimation {
    entry: Rc<dyn ToolingEntry>,
}

impl TransitionAnimation {
    pub(crate) fn new(entry: Rc<dyn ToolingEntry>) -> TransitionAnimation {
        TransitionAnimation { entry }
    }
}

impl TransitionAnimation {
    /// Its name, as `label`: the one its `animate_*` call gives (Compose's defaults,
    /// `FloatAnimation` and the like), or an animation container's (`Built-in alpha`).
    pub fn label(&self) -> String {
        self.entry.label()
    }

    /// Its value if it's a `T`, as `value` (a state read).
    pub fn value<T: Animate>(&self) -> Option<T> {
        self.entry.value_any().downcast::<T>().ok().map(|value| *value)
    }

    /// Its target if it's a `T`, as `targetValue`.
    pub fn target_value<T: Animate>(&self) -> Option<T> {
        self.entry.target_value_any().downcast::<T>().ok().map(|value| *value)
    }

    /// The spec it plays, as `animationSpec`.
    pub fn animation_spec(&self) -> AnimationSpec {
        self.entry.animation_spec()
    }

    /// How long its animation plays, from the play time it started at.
    pub fn duration_nanos(&self) -> i64 {
        self.entry.duration_nanos()
    }
}

impl std::fmt::Debug for TransitionAnimation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "TransitionAnimation({})", self.label())
    }
}

/// A child transition, its state type erased, for tools, as the `Transition<*>`s of Compose's
/// `transitions`.
#[derive(Clone)]
pub struct AnyTransition {
    node: Rc<dyn TransitionNode>,
}

impl AnyTransition {
    /// Its name, after its parent's, as `label`.
    pub fn label(&self) -> Option<String> {
        self.node.label()
    }

    /// Its values, as `animations`.
    pub fn animations(&self) -> Vec<TransitionAnimation> {
        let entries = self.node.animation_entries();
        entries
            .into_iter()
            .map(|entry| TransitionAnimation { entry: entry as Rc<dyn ToolingEntry> })
            .collect()
    }

    /// Its child transitions, as `transitions`.
    pub fn transitions(&self) -> Vec<AnyTransition> {
        let nodes = self.node.child_transitions();
        nodes.into_iter().map(|node| AnyTransition { node }).collect()
    }

    /// Whether its values are animating, as `isRunning` (a state read).
    pub fn is_running(&self) -> bool {
        self.node.is_running()
    }

    /// Its parent's play time, as `playTimeNanos` (a state read).
    pub fn play_time_nanos(&self) -> i64 {
        self.node.play_time_nanos()
    }

    /// The duration of its longest value or child transition, as `totalDurationNanos`.
    pub fn total_duration_nanos(&self) -> i64 {
        self.node.total_duration_nanos()
    }
}

impl<S: Clone + PartialEq + 'static> TransitionInner<S> {
    /// A value or child transition joined or left.
    fn lists_changed(&self) {
        let version = self.lists_count.get().wrapping_add(1);
        self.lists_count.set(version);
        // In composition too (a deferred animation leaving with its call).
        self.lists_version.set(version);
    }

    fn current_untracked(&self) -> S {
        self.state.current_untracked()
    }

    fn target_untracked(&self) -> S {
        self.target_untracked.borrow().clone()
    }

    fn is_running_untracked(&self) -> bool {
        self.start_time_untracked.get() != AnimationConstants::UNSPECIFIED_TIME
    }

    fn set_start_time(&self, start_time_nanos: i64) {
        self.start_time_untracked.set(start_time_nanos);
        self.start_time_nanos.set(start_time_nanos);
    }

    fn set_play_time(&self, play_time_nanos: i64) {
        if self.parent.is_none() {
            self.play_time_untracked.set(play_time_nanos);
            self.play_time_nanos.set(play_time_nanos);
        }
    }

    /// Wakes its task if it waits for something to animate.
    fn wake(&self) {
        let waker = self.idle.borrow_mut().take();
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    /// Whether its task should run frames, as `animateTo`'s `runFrameLoop`.
    fn should_run(&self) -> bool {
        self.target_untracked() != self.current_untracked()
            || self.is_running_untracked()
            || self.update_children_needed.get()
    }

    /// Its values at `play_time_nanos`, as `seekAnimations`.
    pub(crate) fn seek_animations(&self, play_time_nanos: i64) {
        if !self.is_running_untracked() {
            self.set_start_time(play_time_nanos);
        }
        self.set_play_time(play_time_nanos);
        self.update_children_needed.set(false);
        let animations = self.animations.borrow().clone();
        for animation in animations {
            animation.seek_to(play_time_nanos);
        }
        let transitions = self.transitions.borrow().clone();
        for transition in transitions {
            if transition.is_changing() {
                transition.seek_animations(play_time_nanos);
            }
        }
    }

    /// As `setInitialAnimations`.
    pub(crate) fn set_initial_animations(&self, state: &Rc<SeekingAnimationState>) {
        for animation in self.animations.borrow().clone() {
            animation.set_initial_value_animation(state);
        }
        for transition in self.transitions.borrow().clone() {
            transition.set_initial_animations(state);
        }
    }

    /// As `resetAnimationFraction`.
    pub(crate) fn reset_animation_fraction(&self, fraction: f32) {
        for animation in self.animations.borrow().clone() {
            animation.reset_animation_value(fraction);
        }
        for transition in self.transitions.borrow().clone() {
            transition.reset_animation_fraction(fraction);
        }
    }

    /// As `clearInitialAnimations`.
    pub(crate) fn clear_initial_animations(&self) {
        for animation in self.animations.borrow().clone() {
            animation.clear_initial_animation();
        }
        for transition in self.transitions.borrow().clone() {
            transition.clear_initial_animations();
        }
    }

    /// As `updateInitialValues`.
    pub(crate) fn update_initial_values(&self) {
        for animation in self.animations.borrow().clone() {
            animation.update_initial_value();
        }
        for transition in self.transitions.borrow().clone() {
            transition.update_initial_values();
        }
    }

    /// Sets the play time (a root's), as the `playTimeNanos` setter.
    pub(crate) fn set_play_time_nanos(&self, play_time_nanos: i64) {
        self.set_play_time(play_time_nanos);
    }

    /// The duration of its longest value or child, as `totalDurationNanos`.
    pub(crate) fn total_duration(&self) -> i64 {
        TransitionNode::total_duration_nanos(self)
    }

    /// A new target, as `updateTarget`: from where it was heading, every value updated.
    pub(crate) fn update_target(&self, target_state: S) {
        let current_target = self.target_untracked();
        if current_target == target_state {
            return;
        }
        self.segment.set(Segment {
            initial_state: current_target.clone(),
            target_state: target_state.clone(),
        });
        self.state.set_current(current_target);
        *self.target_untracked.borrow_mut() = target_state.clone();
        self.target.set(target_state);
        if !self.is_running_untracked() {
            self.update_children_needed.set(true);
        }
        let animations = self.animations.borrow().clone();
        for animation in animations {
            animation.reset_animation();
        }
        self.wake();
    }

    /// A frame of the root transition, as `onFrame(frameTimeNanos, durationScale)`: the play
    /// time scaled (D827); at a scale of 0 (or NaN) the values end at once.
    fn on_frame(&self, frame_time_nanos: i64, scale: f32) {
        if !self.is_running_untracked() {
            self.on_transition_start(frame_time_nanos);
        }
        let delta = frame_time_nanos - self.start_time_untracked.get();
        let to_end = scale <= 0.0 || scale.is_nan();
        let play_time = if to_end { delta } else { round_half_up(delta as f64 / f64::from(scale)) };
        self.set_play_time(play_time);
        self.frame_at(play_time, to_end);
    }

    fn on_transition_start(&self, frame_time_nanos: i64) {
        self.set_start_time(frame_time_nanos);
        self.state.set_running(true);
    }

    /// Sets every value still animating at `play_time_nanos`, as `onFrame(playTimeNanos,
    /// scaleToEnd)`; ends the transition once they all have.
    fn frame_at(&self, play_time_nanos: i64, scale_to_end: bool) {
        if !self.is_running_untracked() {
            self.on_transition_start(play_time_nanos);
        } else {
            self.state.set_running(true);
        }
        self.update_children_needed.set(false);
        let mut all_finished = true;
        let animations = self.animations.borrow().clone();
        for animation in animations {
            if !animation.is_finished() {
                animation.on_play_time_changed(play_time_nanos, scale_to_end);
            }
            if !animation.is_finished() {
                all_finished = false;
            }
        }
        let transitions = self.transitions.borrow().clone();
        for transition in transitions {
            if transition.is_changing() {
                transition.on_frame_play_time(play_time_nanos, scale_to_end);
            }
            if transition.is_changing() {
                all_finished = false;
            }
        }
        if all_finished {
            self.end();
        }
    }

    /// At its target, as `onTransitionEnd`: a `MutableTransitionState`'s current state the
    /// target (a seekable state sets its own).
    pub(crate) fn end(&self) {
        self.set_start_time(AnimationConstants::UNSPECIFIED_TIME);
        let target = self.target_untracked();
        if matches!(self.state, StateHolder::Mutable(_)) && self.current_untracked() != target {
            self.state.set_current(target);
        }
        self.set_play_time(0);
        self.state.set_running(false);
        let transitions = self.transitions.borrow().clone();
        for transition in transitions {
            transition.on_transition_end();
        }
    }

    /// Its task, as `animateTo`'s frame loop: each frame while there's something to animate.
    async fn run(self: Rc<Self>) {
        loop {
            std::future::poll_fn(|context| {
                if self.should_run() {
                    Poll::Ready(())
                } else {
                    *self.idle.borrow_mut() = Some(context.waker().clone());
                    Poll::Pending
                }
            })
            .await;
            // Read as the frame loop starts, not each frame, as Compose's: the scale of the
            // composition's effects (the thread's).
            let scale = compose_ui::motion_duration_scale();
            while self.should_run() {
                let inner = self.clone();
                with_frame_nanos(move |time| inner.on_frame(time as i64, scale)).await;
            }
        }
    }
}

impl<S: Clone + PartialEq + 'static> TransitionNode for TransitionInner<S> {
    fn animation_entries(&self) -> Vec<Rc<dyn AnimationEntry>> {
        self.lists_version.get();
        self.animations.borrow().clone()
    }

    fn child_transitions(&self) -> Vec<Rc<dyn TransitionNode>> {
        self.lists_version.get();
        self.transitions.borrow().clone()
    }

    fn is_running(&self) -> bool {
        self.start_time_nanos.get() != AnimationConstants::UNSPECIFIED_TIME
    }

    fn label(&self) -> Option<String> {
        let parent = self.parent.as_ref().and_then(Weak::upgrade).and_then(|parent| parent.label());
        match (parent, self.label) {
            (Some(parent), Some(own)) => Some(format!("{parent} > {own}")),
            (Some(parent), None) => Some(parent),
            (None, own) => own.map(str::to_string),
        }
    }

    fn play_time_nanos(&self) -> i64 {
        match self.parent.as_ref().and_then(Weak::upgrade) {
            Some(parent) => parent.play_time_nanos(),
            None => self.play_time_nanos.get(),
        }
    }

    fn play_time_untracked(&self) -> i64 {
        match self.parent.as_ref().and_then(Weak::upgrade) {
            Some(parent) => parent.play_time_untracked(),
            None => self.play_time_untracked.get(),
        }
    }

    fn on_duration_changed(&self) {
        let version = self.durations_count.get().wrapping_add(1);
        self.durations_count.set(version);
        self.durations_version.set(version);
    }

    fn on_child_animation_updated(&self) {
        self.update_children_needed.set(true);
        self.wake();
    }

    fn is_changing(&self) -> bool {
        self.target_untracked() != self.current_untracked()
    }

    fn on_frame_play_time(&self, play_time_nanos: i64, scale_to_end: bool) {
        self.frame_at(play_time_nanos, scale_to_end);
    }

    fn on_transition_end(&self) {
        self.end();
    }

    fn seek_animations(&self, play_time_nanos: i64) {
        TransitionInner::seek_animations(self, play_time_nanos);
    }

    fn set_initial_animations(&self, state: &Rc<SeekingAnimationState>) {
        TransitionInner::set_initial_animations(self, state);
    }

    fn reset_animation_fraction(&self, fraction: f32) {
        TransitionInner::reset_animation_fraction(self, fraction);
    }

    fn clear_initial_animations(&self) {
        TransitionInner::clear_initial_animations(self);
    }

    fn update_initial_values(&self) {
        TransitionInner::update_initial_values(self);
    }

    fn has_initial_value_animations(&self) -> bool {
        self.animations.borrow().iter().any(|animation| animation.has_initial_value_animation())
            || self.transitions.borrow().iter().any(|child| child.has_initial_value_animations())
    }

    fn total_duration_nanos(&self) -> i64 {
        // State reads: a value joining, leaving or changing its duration changes it.
        self.lists_version.get();
        self.durations_version.get();
        let animations = self.animations.borrow().clone();
        let transitions = self.transitions.borrow().clone();
        let longest = animations.iter().map(|animation| animation.duration_nanos()).max();
        let longest_child =
            transitions.iter().map(|transition| transition.total_duration_nanos()).max();
        longest.max(longest_child).unwrap_or(0)
    }
}

impl<S: Clone + PartialEq + 'static> Transition<S> {
    fn new(
        state: StateHolder<S>,
        parent: Option<Weak<dyn TransitionNode>>,
        label: Option<&'static str>,
    ) -> Transition<S> {
        let current = state.current_untracked();
        Transition {
            inner: Rc::new(TransitionInner {
                label,
                parent,
                target: mutable_state_of(current.clone()),
                target_untracked: RefCell::new(current.clone()),
                segment: mutable_state_of(Segment {
                    initial_state: current.clone(),
                    target_state: current,
                }),
                start_time_nanos: mutable_state_of(AnimationConstants::UNSPECIFIED_TIME),
                start_time_untracked: Cell::new(AnimationConstants::UNSPECIFIED_TIME),
                play_time_nanos: mutable_state_of(0),
                play_time_untracked: Cell::new(0),
                update_children_needed: Cell::new(false),
                animations: RefCell::new(Vec::new()),
                transitions: RefCell::new(Vec::new()),
                lists_version: mutable_state_of(0),
                lists_count: Cell::new(0),
                durations_version: mutable_state_of(0),
                durations_count: Cell::new(0),
                idle: RefCell::new(None),
                state,
            }),
        }
    }

    fn node(&self) -> Weak<dyn TransitionNode> {
        let node: Rc<dyn TransitionNode> = self.inner.clone();
        Rc::downgrade(&node)
    }

    /// Where its values are, as `currentState` (a state read: it changes when they all reach
    /// the target, and to the state a change was heading for when another interrupts it).
    pub fn current_state(&self) -> S {
        self.inner.state.current_state()
    }

    /// Where it goes, as `targetState` (a state read).
    pub fn target_state(&self) -> S {
        self.inner.target.get()
    }

    /// The change it plays, as `segment` (a state read): from the state it was at, or heading
    /// for when interrupted, to the target.
    pub fn segment(&self) -> Segment<S> {
        self.inner.segment.get()
    }

    /// Whether its values are animating, as `isRunning` (a state read).
    pub fn is_running(&self) -> bool {
        self.inner.start_time_nanos.get() != AnimationConstants::UNSPECIFIED_TIME
    }

    /// Whether a value (or a child transition's) goes on from where an interrupted seek left
    /// it, along an initial-value animation, as `hasInitialValueAnimations` (not a state read).
    pub fn has_initial_value_animations(&self) -> bool {
        TransitionNode::has_initial_value_animations(&*self.inner)
    }

    /// Its name for tools, after its parent's (`Parent > Child`), as `label`.
    pub fn label(&self) -> Option<String> {
        self.inner.label()
    }

    /// The play time of its last frame, as `playTimeNanos` (Compose's, restricted): a child's is
    /// its parent's (a state read).
    #[doc(hidden)]
    pub fn play_time_nanos(&self) -> i64 {
        self.inner.play_time_nanos()
    }

    /// The duration of its longest value or child transition, as `totalDurationNanos`.
    pub fn total_duration_nanos(&self) -> i64 {
        self.inner.total_duration_nanos()
    }

    /// Its values, for tools (an inspector), as `animations`: those of its `animate_*` calls
    /// and its animation containers' (`AnimatedVisibility`'s alpha, `Built-in alpha`), in the
    /// order they joined.
    pub fn animations(&self) -> Vec<TransitionAnimation> {
        let entries = self.inner.animation_entries();
        entries
            .into_iter()
            .map(|entry| TransitionAnimation { entry: entry as Rc<dyn ToolingEntry> })
            .collect()
    }

    /// Its child transitions, for tools, as `transitions`.
    pub fn transitions(&self) -> Vec<AnyTransition> {
        let nodes = self.inner.child_transitions();
        nodes.into_iter().map(|node| AnyTransition { node }).collect()
    }

    /// Animates to `target_state`, as Compose's internal `animateTo`: called on every pass by
    /// the composable that owns the transition.
    fn animate_to(&self, target_state: S) {
        self.inner.update_target(target_state);
        let inner = self.inner.clone();
        LaunchedEffect(self.clone(), async move {
            inner.run().await;
        });
    }

    /// A transition following this one's states through `transform`, playing on its clock, as
    /// `createChildTransition`: its values animate as this one's do. Remembered at its call
    /// site: call it on every pass.
    pub fn create_child_transition<T: Clone + PartialEq + 'static>(
        &self,
        label: impl Into<Option<&'static str>>,
        transform: impl Fn(&S) -> T,
    ) -> Transition<T> {
        let initial_parent_state = remember_keyed(self.clone(), || self.inner.current_untracked());
        let initial_state = transform(&initial_parent_state);
        let target_state = transform(&self.target_state());
        let parent = self.node();
        let label = label.into().unwrap_or("ChildTransition");
        let child = remember_keyed(self.clone(), move || {
            let state = StateHolder::Mutable(MutableTransitionState::new(initial_state));
            Transition::new(state, Some(parent), Some(label))
        });
        let (parent, added) = (self.inner.clone(), child.inner.clone());
        DisposableEffect(child.clone(), move |scope| {
            let node: Rc<dyn TransitionNode> = added;
            parent.transitions.borrow_mut().push(node.clone());
            parent.lists_changed();
            scope.on_dispose(move || {
                parent.transitions.borrow_mut().retain(|kept| !Rc::ptr_eq(kept, &node));
                parent.lists_changed();
            })
        });
        child.inner.update_target(target_state);
        child
    }

    /// A value animating to `target_value_by_state(target)` whenever the target changes, along
    /// `transition_spec(segment)`, as `Transition.animateValue`: first composed mid-way, it
    /// starts at its value for the state the transition was at and plays from then; changed
    /// mid-way, it goes on from where it is, along a spring of the type's threshold unless its
    /// spec is a spring (Compose's interruption spec). Remembered at its call site: call it on
    /// every pass, as a composable.
    pub fn animate_value<T: Animate>(
        &self,
        transition_spec: impl Fn(&Segment<S>) -> AnimationSpec,
        target_value_by_state: impl Fn(&S) -> T,
    ) -> State<T> {
        self.animate_labeled("ValueAnimation", transition_spec, target_value_by_state)
    }

    /// The same with the `label` tools show (`ValueAnimation` by default), as
    /// `Transition.animateValue(typeConverter, transitionSpec, label, targetValueByState)`.
    pub fn animate_value_with<T: Animate>(
        &self,
        transition_spec: impl Fn(&Segment<S>) -> AnimationSpec,
        label: &'static str,
        target_value_by_state: impl Fn(&S) -> T,
    ) -> State<T> {
        self.animate_labeled(label, transition_spec, target_value_by_state)
    }

    /// `animate_value` with the label tools show (Compose's default for each type).
    fn animate_labeled<T: Animate>(
        &self,
        label: &'static str,
        transition_spec: impl Fn(&Segment<S>) -> AnimationSpec,
        target_value_by_state: impl Fn(&S) -> T,
    ) -> State<T> {
        let initial_state = remember_keyed(self.clone(), || self.inner.current_untracked());
        let initial_value = target_value_by_state(&initial_state);
        let target_value = target_value_by_state(&self.target_state());
        let spec = transition_spec(&self.segment());
        let node = self.node();
        let animation = remember_keyed(self.clone(), move || {
            Rc::new(TransitionAnimationState::new(initial_value, node, label.to_string()))
        });
        animation.update_target_value(target_value, spec);
        let (inner, added) = (self.inner.clone(), animation.clone());
        DisposableEffect(animation.clone(), move |scope| {
            let entry: Rc<dyn AnimationEntry> = added;
            inner.animations.borrow_mut().push(entry.clone());
            inner.lists_changed();
            inner.wake();
            scope.on_dispose(move || {
                inner.animations.borrow_mut().retain(|kept| !Rc::ptr_eq(kept, &entry));
                inner.lists_changed();
            })
        });
        animation.value.as_state()
    }

    /// As `Transition.animateFloat`.
    pub fn animate_float(
        &self,
        transition_spec: impl Fn(&Segment<S>) -> AnimationSpec,
        target_value_by_state: impl Fn(&S) -> f32,
    ) -> State<f32> {
        self.animate_labeled("FloatAnimation", transition_spec, target_value_by_state)
    }

    /// The same with the `label` tools show (`FloatAnimation` by default), as `Transition.animateFloat(transitionSpec, label, targetValueByState)`.
    pub fn animate_float_with(
        &self,
        transition_spec: impl Fn(&Segment<S>) -> AnimationSpec,
        label: &'static str,
        target_value_by_state: impl Fn(&S) -> f32,
    ) -> State<f32> {
        self.animate_labeled(label, transition_spec, target_value_by_state)
    }

    /// As `Transition.animateDp`.
    pub fn animate_dp(
        &self,
        transition_spec: impl Fn(&Segment<S>) -> AnimationSpec,
        target_value_by_state: impl Fn(&S) -> Dp,
    ) -> State<Dp> {
        self.animate_labeled("DpAnimation", transition_spec, target_value_by_state)
    }

    /// The same with the `label` tools show (`DpAnimation` by default), as `Transition.animateDp(transitionSpec, label, targetValueByState)`.
    pub fn animate_dp_with(
        &self,
        transition_spec: impl Fn(&Segment<S>) -> AnimationSpec,
        label: &'static str,
        target_value_by_state: impl Fn(&S) -> Dp,
    ) -> State<Dp> {
        self.animate_labeled(label, transition_spec, target_value_by_state)
    }

    /// As `Transition.animateColor`: in Oklab.
    pub fn animate_color(
        &self,
        transition_spec: impl Fn(&Segment<S>) -> AnimationSpec,
        target_value_by_state: impl Fn(&S) -> Color,
    ) -> State<Color> {
        self.animate_labeled("ColorAnimation", transition_spec, target_value_by_state)
    }

    /// The same with the `label` tools show (`ColorAnimation` by default), as `Transition.animateColor(transitionSpec, label, targetValueByState)`.
    pub fn animate_color_with(
        &self,
        transition_spec: impl Fn(&Segment<S>) -> AnimationSpec,
        label: &'static str,
        target_value_by_state: impl Fn(&S) -> Color,
    ) -> State<Color> {
        self.animate_labeled(label, transition_spec, target_value_by_state)
    }

    /// As `Transition.animateOffset`.
    pub fn animate_offset(
        &self,
        transition_spec: impl Fn(&Segment<S>) -> AnimationSpec,
        target_value_by_state: impl Fn(&S) -> Offset,
    ) -> State<Offset> {
        self.animate_labeled("OffsetAnimation", transition_spec, target_value_by_state)
    }

    /// The same with the `label` tools show (`OffsetAnimation` by default), as `Transition.animateOffset(transitionSpec, label, targetValueByState)`.
    pub fn animate_offset_with(
        &self,
        transition_spec: impl Fn(&Segment<S>) -> AnimationSpec,
        label: &'static str,
        target_value_by_state: impl Fn(&S) -> Offset,
    ) -> State<Offset> {
        self.animate_labeled(label, transition_spec, target_value_by_state)
    }

    /// As `Transition.animateSize`.
    pub fn animate_size(
        &self,
        transition_spec: impl Fn(&Segment<S>) -> AnimationSpec,
        target_value_by_state: impl Fn(&S) -> Size,
    ) -> State<Size> {
        self.animate_labeled("SizeAnimation", transition_spec, target_value_by_state)
    }

    /// The same with the `label` tools show (`SizeAnimation` by default), as `Transition.animateSize(transitionSpec, label, targetValueByState)`.
    pub fn animate_size_with(
        &self,
        transition_spec: impl Fn(&Segment<S>) -> AnimationSpec,
        label: &'static str,
        target_value_by_state: impl Fn(&S) -> Size,
    ) -> State<Size> {
        self.animate_labeled(label, transition_spec, target_value_by_state)
    }

    /// As `Transition.animateInt`.
    pub fn animate_int(
        &self,
        transition_spec: impl Fn(&Segment<S>) -> AnimationSpec,
        target_value_by_state: impl Fn(&S) -> i32,
    ) -> State<i32> {
        self.animate_labeled("IntAnimation", transition_spec, target_value_by_state)
    }

    /// The same with the `label` tools show (`IntAnimation` by default), as `Transition.animateInt(transitionSpec, label, targetValueByState)`.
    pub fn animate_int_with(
        &self,
        transition_spec: impl Fn(&Segment<S>) -> AnimationSpec,
        label: &'static str,
        target_value_by_state: impl Fn(&S) -> i32,
    ) -> State<i32> {
        self.animate_labeled(label, transition_spec, target_value_by_state)
    }

    /// As `Transition.animateIntOffset`.
    pub fn animate_int_offset(
        &self,
        transition_spec: impl Fn(&Segment<S>) -> AnimationSpec,
        target_value_by_state: impl Fn(&S) -> IntOffset,
    ) -> State<IntOffset> {
        self.animate_labeled("IntOffsetAnimation", transition_spec, target_value_by_state)
    }

    /// The same with the `label` tools show (`IntOffsetAnimation` by default), as `Transition.animateIntOffset(transitionSpec, label, targetValueByState)`.
    pub fn animate_int_offset_with(
        &self,
        transition_spec: impl Fn(&Segment<S>) -> AnimationSpec,
        label: &'static str,
        target_value_by_state: impl Fn(&S) -> IntOffset,
    ) -> State<IntOffset> {
        self.animate_labeled(label, transition_spec, target_value_by_state)
    }

    /// As `Transition.animateIntSize`.
    pub fn animate_int_size(
        &self,
        transition_spec: impl Fn(&Segment<S>) -> AnimationSpec,
        target_value_by_state: impl Fn(&S) -> IntSize,
    ) -> State<IntSize> {
        self.animate_labeled("IntSizeAnimation", transition_spec, target_value_by_state)
    }

    /// The same with the `label` tools show (`IntSizeAnimation` by default), as `Transition.animateIntSize(transitionSpec, label, targetValueByState)`.
    pub fn animate_int_size_with(
        &self,
        transition_spec: impl Fn(&Segment<S>) -> AnimationSpec,
        label: &'static str,
        target_value_by_state: impl Fn(&S) -> IntSize,
    ) -> State<IntSize> {
        self.animate_labeled(label, transition_spec, target_value_by_state)
    }

    /// As `Transition.animateRect`.
    pub fn animate_rect(
        &self,
        transition_spec: impl Fn(&Segment<S>) -> AnimationSpec,
        target_value_by_state: impl Fn(&S) -> Rect,
    ) -> State<Rect> {
        self.animate_labeled("RectAnimation", transition_spec, target_value_by_state)
    }

    /// The same with the `label` tools show (`RectAnimation` by default), as `Transition.animateRect(transitionSpec, label, targetValueByState)`.
    pub fn animate_rect_with(
        &self,
        transition_spec: impl Fn(&Segment<S>) -> AnimationSpec,
        label: &'static str,
        target_value_by_state: impl Fn(&S) -> Rect,
    ) -> State<Rect> {
        self.animate_labeled(label, transition_spec, target_value_by_state)
    }

    /// A value of this transition whose targets are known later (while measuring or drawing),
    /// as Compose's `createDeferredAnimation`: it joins the transition at its first
    /// [`animate`](DeferredAnimation::animate). Remembered at its call site: call it on every
    /// pass, as a composable; it leaves the transition with its call.
    pub(crate) fn create_deferred_animation<T: Animate>(
        &self,
        label: impl FnOnce() -> String,
    ) -> DeferredAnimation<S, T> {
        let transition = self.clone();
        let deferred = remember_keyed(self.clone(), move || DeferredAnimation {
            inner: Rc::new(DeferredInner { transition, label: label(), data: RefCell::new(None) }),
        });
        let removed = deferred.inner.clone();
        DisposableEffect(deferred.clone(), move |scope| {
            scope.on_dispose(move || {
                let data = removed.data.borrow_mut().take();
                if let Some(data) = data {
                    let entry: Rc<dyn AnimationEntry> = data.animation.clone();
                    let inner = &removed.transition.inner;
                    inner.animations.borrow_mut().retain(|kept| !Rc::ptr_eq(kept, &entry));
                    inner.lists_changed();
                }
            })
        });
        deferred
    }
}

/// A value of a transition whose targets are given while measuring or drawing, as Compose's
/// `Transition.DeferredAnimation` (made by [`Transition::create_deferred_animation`]).
pub(crate) struct DeferredAnimation<S: 'static, T: Animate> {
    inner: Rc<DeferredInner<S, T>>,
}

struct DeferredInner<S: 'static, T: Animate> {
    transition: Transition<S>,
    label: String,
    data: RefCell<Option<Rc<DeferredData<S, T>>>>,
}

impl<S: 'static, T: Animate> Clone for DeferredAnimation<S, T> {
    fn clone(&self) -> Self {
        DeferredAnimation { inner: self.inner.clone() }
    }
}

impl<S: 'static, T: Animate> ComposeArg for DeferredAnimation<S, T> {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

type SpecBySegment<S> = Box<dyn Fn(&Segment<S>) -> AnimationSpec>;
type ValueByState<S, T> = Box<dyn Fn(&S) -> T>;

/// A deferred animation's value and how it's computed, as `DeferredAnimationData`: reading it
/// updates its target from the transition's segment first.
pub(crate) struct DeferredData<S: 'static, T: Animate> {
    transition: Rc<TransitionInner<S>>,
    animation: Rc<TransitionAnimationState<T>>,
    transition_spec: RefCell<SpecBySegment<S>>,
    target_value_by_state: RefCell<ValueByState<S, T>>,
}

impl<S: Clone + PartialEq + 'static, T: Animate> DeferredData<S, T> {
    fn update_animation_states(&self) {
        let segment = self.transition.segment.get();
        let target = (self.target_value_by_state.borrow())(&segment.target_state);
        let spec = (self.transition_spec.borrow())(&segment);
        self.animation.update_target_value(target, spec);
    }

    /// The value, its target brought up to date first (a state read, as `value`).
    pub fn value(&self) -> T {
        self.update_animation_states();
        self.animation.value.get()
    }
}

impl<S: Clone + PartialEq + 'static, T: Animate> DeferredAnimation<S, T> {
    /// Leaves the transition, as Compose's when its `createDeferredAnimation` call leaves the
    /// composition: the next [`animate`](Self::animate) joins it anew.
    pub(crate) fn leave(&self) {
        let data = self.inner.data.borrow_mut().take();
        if let Some(data) = data {
            let entry: Rc<dyn AnimationEntry> = data.animation.clone();
            let inner = &self.inner.transition.inner;
            inner.animations.borrow_mut().retain(|kept| !Rc::ptr_eq(kept, &entry));
            inner.lists_changed();
        }
    }

    /// The state its transition is at, read without observing it.
    pub(crate) fn current_state_untracked(&self) -> S {
        self.inner.transition.inner.current_untracked()
    }

    /// Its value animating to `target_value_by_state(target)` along `transition_spec(segment)`,
    /// as `DeferredAnimation.animate`: the first call joins the transition, at its value for the
    /// state the transition is at.
    pub fn animate(
        &self,
        transition_spec: impl Fn(&Segment<S>) -> AnimationSpec + 'static,
        target_value_by_state: impl Fn(&S) -> T + 'static,
    ) -> Rc<DeferredData<S, T>> {
        let existing = self.inner.data.borrow().clone();
        let data = match existing {
            Some(data) => {
                *data.transition_spec.borrow_mut() = Box::new(transition_spec);
                *data.target_value_by_state.borrow_mut() = Box::new(target_value_by_state);
                data
            }
            None => {
                let transition = &self.inner.transition;
                let initial = target_value_by_state(&transition.inner.current_untracked());
                let label = self.inner.label.clone();
                let animation =
                    Rc::new(TransitionAnimationState::new(initial, transition.node(), label));
                let entry: Rc<dyn AnimationEntry> = animation.clone();
                transition.inner.animations.borrow_mut().push(entry);
                transition.inner.lists_changed();
                transition.inner.wake();
                let data = Rc::new(DeferredData {
                    transition: transition.inner.clone(),
                    animation,
                    transition_spec: RefCell::new(Box::new(transition_spec)),
                    target_value_by_state: RefCell::new(Box::new(target_value_by_state)),
                });
                *self.inner.data.borrow_mut() = Some(data.clone());
                data
            }
        };
        data.update_animation_states();
        data
    }
}

/// A transition following `target_state`, remembered at its call site, as
/// `updateTransition(targetState, label)`: its values animate whenever it changes.
pub fn update_transition<S: Clone + PartialEq + 'static>(
    target_state: S,
    label: impl Into<Option<&'static str>>,
) -> Transition<S> {
    let initial = target_state.clone();
    let label = label.into();
    let transition = remember(move || {
        Transition::new(StateHolder::Mutable(MutableTransitionState::new(initial)), None, label)
    });
    transition.animate_to(target_state);
    on_disposed(&transition);
    transition
}

/// What a transition is remembered with ([`remember_transition`]), as Compose's sealed
/// `TransitionState`: a [`MutableTransitionState`] or a
/// [`SeekableTransitionState`](crate::SeekableTransitionState).
pub trait TransitionState<S: Clone + PartialEq + 'static> {
    #[doc(hidden)]
    fn remember_transition(&self, label: Option<&'static str>) -> Transition<S>;
}

impl<S: Clone + PartialEq + 'static> TransitionState<S> for MutableTransitionState<S> {
    fn remember_transition(&self, label: Option<&'static str>) -> Transition<S> {
        let state = self.clone();
        let transition = remember_keyed(self.clone(), move || {
            Transition::new(StateHolder::Mutable(state), None, label)
        });
        transition.animate_to(self.target_state());
        on_disposed(&transition);
        transition
    }
}

impl<S: Clone + PartialEq + 'static> TransitionState<S>
    for crate::seekable::SeekableTransitionState<S>
{
    fn remember_transition(&self, label: Option<&'static str>) -> Transition<S> {
        let state = self.clone();
        let transition = remember_keyed(self.clone(), move || {
            Transition::new(StateHolder::Seekable(state), None, label)
        });
        // No frame loop: the state's `seek_to`, `animate_to` and `snap_to` drive it.
        self.composed(&transition);
        let (inner, state) = (transition.inner.clone(), self.clone());
        DisposableEffect(transition.clone(), move |scope| {
            state.transition_configured(Rc::downgrade(&inner));
            scope.on_dispose(move || {
                inner.end();
                state.transition_removed();
            })
        });
        transition
    }
}

/// A transition following `transition_state`, as `rememberTransition(transitionState, label)`:
/// a [`MutableTransitionState`]'s target, the transition putting its current state at the
/// target when its values get there; or a
/// [`SeekableTransitionState`](crate::SeekableTransitionState), which seeks and animates it.
pub fn remember_transition<S: Clone + PartialEq + 'static>(
    transition_state: &impl TransitionState<S>,
    label: impl Into<Option<&'static str>>,
) -> Transition<S> {
    transition_state.remember_transition(label.into())
}

/// Ends the transition when it leaves the composition, as `onDisposed`.
fn on_disposed<S: Clone + PartialEq + 'static>(transition: &Transition<S>) {
    let inner = transition.inner.clone();
    DisposableEffect(transition.clone(), move |scope| {
        scope.on_dispose(move || {
            inner.end();
        })
    });
}

const NO_RESET: f32 = -1.0;
const RESET_NO_SNAP: f32 = -2.0;
pub(crate) const RESET_ANIMATION_SNAP: f32 = -3.0;
pub(crate) const RESET_ANIMATION_SNAP_CURRENT: f32 = -4.0;
pub(crate) const RESET_ANIMATION_SNAP_TARGET: f32 = -5.0;

/// A value of a transition, as Compose's `Transition.TransitionAnimationState`.
struct TransitionAnimationState<T: Animate> {
    transition: Weak<dyn TransitionNode>,
    label: String,
    target_value: RefCell<T>,
    animation_spec: RefCell<AnimationSpec>,
    animation: RefCell<TargetBasedAnimation<T>>,
    is_finished: Cell<bool>,
    reset_snap_value: Cell<f32>,
    value: OwnedState<T>,
    /// A plain copy of `value`, read in composition without observing it.
    value_untracked: RefCell<T>,
    velocity: Cell<AnimationVector>,
    duration_nanos: Cell<i64>,
    /// A spring of the type's threshold, as `interruptionSpec`.
    interruption_spec: AnimationSpec,
    /// Seeking: the animation an interrupted change left, continued along this state.
    initial_value_state: RefCell<Option<Rc<SeekingAnimationState>>>,
    initial_value_animation: RefCell<Option<TargetBasedAnimation<T>>>,
    /// The initial-value animation alone moves the value, as `useOnlyInitialValue`.
    use_only_initial_value: Cell<bool>,
    /// Seeked: an interruption doesn't take the interruption spring.
    is_seeking: Cell<bool>,
}

impl<T: Animate> TransitionAnimationState<T> {
    fn new(
        initial_value: T,
        transition: Weak<dyn TransitionNode>,
        label: String,
    ) -> TransitionAnimationState<T> {
        let zero = initial_value.to_vector().new_instance();
        let threshold = AnimationVector::filled(zero.size(), T::visibility_threshold());
        let interruption_spec = AnimationSpec::Spring(spring().visibility_threshold(threshold));
        let default_spec = AnimationSpec::Spring(spring());
        let animation = TargetBasedAnimation::with_velocity_vector(
            default_spec.clone(),
            initial_value.clone(),
            initial_value.clone(),
            zero,
        );
        let duration = animation.duration_nanos();
        TransitionAnimationState {
            transition,
            label,
            target_value: RefCell::new(initial_value.clone()),
            animation_spec: RefCell::new(default_spec),
            animation: RefCell::new(animation),
            is_finished: Cell::new(true),
            reset_snap_value: Cell::new(NO_RESET),
            value_untracked: RefCell::new(initial_value.clone()),
            value: compose_runtime::mutable_state_of_with(
                initial_value,
                crate::vector::SameValuePolicy,
            ),
            velocity: Cell::new(zero),
            duration_nanos: Cell::new(duration),
            interruption_spec,
            initial_value_state: RefCell::new(None),
            initial_value_animation: RefCell::new(None),
            use_only_initial_value: Cell::new(false),
            is_seeking: Cell::new(false),
        }
    }

    fn set_value(&self, value: T) {
        *self.value_untracked.borrow_mut() = value.clone();
        self.value.set(value);
    }

    /// The duration of its animation, kept, as `durationNanos` (a state of Compose's, which
    /// the transition's total duration reads).
    fn update_duration(&self) {
        let duration = self.animation.borrow().duration_nanos();
        if self.duration_nanos.replace(duration) != duration
            && let Some(transition) = self.transition.upgrade()
        {
            transition.on_duration_changed();
        }
    }

    /// A still animation at `value`, along the interruption spring.
    fn still_at(&self, value: T) -> TargetBasedAnimation<T> {
        let zero = value.to_vector().new_instance();
        TargetBasedAnimation::with_velocity_vector(
            self.interruption_spec.clone(),
            value.clone(),
            value,
            zero,
        )
    }

    fn play_time_nanos(&self) -> i64 {
        self.transition.upgrade().map_or(0, |transition| transition.play_time_untracked())
    }

    /// A new animation from `initial_value` to the target, as `updateAnimation`: along the
    /// interruption spring when it interrupts one still running (unless its spec is a spring),
    /// starting at the transition's play time.
    fn update_animation(&self, initial_value: T, is_interrupted: bool) {
        let target_value = self.target_value.borrow().clone();
        let initial_target =
            self.initial_value_animation.borrow().as_ref().map(TargetBasedAnimation::target_value);
        if initial_target.is_some_and(|initial_target| initial_target.same_value(&target_value)) {
            // The initial-value animation goes to the target: it alone moves the value.
            *self.animation.borrow_mut() = self.still_at(initial_value);
            self.use_only_initial_value.set(true);
            self.update_duration();
            return;
        }
        let spec = self.animation_spec.borrow().clone();
        let spec = match spec {
            AnimationSpec::Spring(_) => spec,
            AnimationSpec::InterruptionAware(aware) if is_interrupted && !self.is_seeking.get() => {
                aware.interruption_spec().clone()
            }
            _ if is_interrupted && !self.is_seeking.get() => self.interruption_spec.clone(),
            _ => spec,
        };
        let play_time = self.play_time_nanos();
        let target = self.target_value.borrow().clone();
        let animation = TargetBasedAnimation::with_velocity_vector(
            spec,
            initial_value,
            target,
            self.velocity.get(),
        )
        .delayed(play_time);
        *self.animation.borrow_mut() = animation;
        self.update_duration();
        self.use_only_initial_value.set(false);
        if let Some(transition) = self.transition.upgrade() {
            transition.on_child_animation_updated();
        }
    }

    /// A target and spec for this pass, as `updateTargetValue`.
    fn update_target_value(&self, target_value: T, animation_spec: AnimationSpec) {
        if self.use_only_initial_value.get() {
            let initial_target = self
                .initial_value_animation
                .borrow()
                .as_ref()
                .map(TargetBasedAnimation::target_value);
            if initial_target.is_some_and(|initial| initial.same_value(&target_value)) {
                // Already going to the target through the initial-value animation.
                return;
            }
        }
        if self.target_value.borrow().same_value(&target_value)
            && self.reset_snap_value.get() == NO_RESET
        {
            return;
        }
        *self.target_value.borrow_mut() = target_value.clone();
        *self.animation_spec.borrow_mut() = animation_spec;
        let reset = self.reset_snap_value.get();
        let initial_value = if reset == RESET_ANIMATION_SNAP {
            target_value.clone()
        } else {
            self.value_untracked.borrow().clone()
        };
        let interrupted = !self.is_finished.get();
        self.update_animation(initial_value, interrupted);
        self.is_finished.set(reset == RESET_ANIMATION_SNAP);
        // A target changed during a seekable change: at the fraction it was seeked to.
        if reset >= 0.0 {
            let duration = self.animation.borrow().duration_nanos();
            // Kotlin's `(duration * fraction).toLong()`: in floats.
            let play_time = (duration as f32 * reset) as i64;
            let value = self.animation.borrow().value_from_nanos(play_time);
            self.set_value(value);
        } else if reset == RESET_ANIMATION_SNAP {
            self.set_value(target_value);
        }
        self.use_only_initial_value.set(false);
        self.reset_snap_value.set(NO_RESET);
    }
}

impl<T: Animate> AnimationEntry for TransitionAnimationState<T> {
    fn on_play_time_changed(&self, play_time_nanos: i64, scale_to_end: bool) {
        let animation = self.animation.borrow();
        let play_time = if scale_to_end { animation.duration_nanos() } else { play_time_nanos };
        let value = animation.value_from_nanos(play_time);
        self.velocity.set(animation.velocity_vector_from_nanos(play_time));
        let finished = animation.is_finished_from_nanos(play_time);
        drop(animation);
        *self.value_untracked.borrow_mut() = value.clone();
        self.value.set(value);
        if finished {
            self.is_finished.set(true);
        }
    }

    fn is_finished(&self) -> bool {
        self.is_finished.get()
    }

    fn reset_animation(&self) {
        self.reset_snap_value.set(RESET_NO_SNAP);
    }

    fn seek_to(&self, play_time_nanos: i64) {
        if self.reset_snap_value.get() != NO_RESET {
            return;
        }
        self.is_seeking.set(true);
        let animation = self.animation.borrow();
        if animation.target_value().same_value(&animation.initial_value()) {
            // The initial-value animation alone moves it (an interrupted change).
            let target = animation.target_value();
            drop(animation);
            self.set_value(target);
        } else {
            let value = animation.value_from_nanos(play_time_nanos);
            self.velocity.set(animation.velocity_vector_from_nanos(play_time_nanos));
            drop(animation);
            self.set_value(value);
        }
    }

    fn update_initial_value(&self) {
        let Some(state) = self.initial_value_state.borrow().clone() else { return };
        let Some(initial_animation) = self.initial_value_animation.borrow().clone() else {
            return;
        };
        // In doubles, as Compose's (floats hold too few digits of nanoseconds).
        let play = state.duration_nanos.get() as f64 * f64::from(state.value.get());
        let initial_play_time = (play + 0.5).floor() as i64;
        let initial_value = initial_animation.value_from_nanos(initial_play_time);
        {
            let mut animation = self.animation.borrow_mut();
            if self.use_only_initial_value.get() {
                animation.set_target_value(initial_value.clone());
            }
            animation.set_initial_value(initial_value.clone());
        }
        self.update_duration();
        if self.reset_snap_value.get() == RESET_NO_SNAP || self.use_only_initial_value.get() {
            self.set_value(initial_value);
        } else {
            self.seek_to(self.play_time_nanos());
        }
        if initial_play_time >= state.duration_nanos.get() {
            *self.initial_value_state.borrow_mut() = None;
            *self.initial_value_animation.borrow_mut() = None;
        } else {
            state.is_complete.set(false);
        }
    }

    fn reset_animation_value(&self, fraction: f32) {
        if fraction == RESET_ANIMATION_SNAP_CURRENT || fraction == RESET_ANIMATION_SNAP_TARGET {
            let initial = self.initial_value_animation.borrow_mut().take();
            if let Some(initial) = initial {
                self.animation.borrow_mut().set_initial_value(initial.target_value());
                *self.initial_value_state.borrow_mut() = None;
            }
            let value = {
                let animation = self.animation.borrow();
                if fraction == RESET_ANIMATION_SNAP_CURRENT {
                    animation.initial_value()
                } else {
                    animation.target_value()
                }
            };
            {
                let mut animation = self.animation.borrow_mut();
                animation.set_initial_value(value.clone());
                animation.set_target_value(value.clone());
            }
            self.set_value(value);
            self.update_duration();
        } else {
            self.reset_snap_value.set(fraction);
        }
    }

    fn set_initial_value_animation(&self, state: &Rc<SeekingAnimationState>) {
        {
            let animation = self.animation.borrow();
            if !animation.target_value().same_value(&animation.initial_value()) {
                // It goes on from where it is to the target, along `state`.
                *self.initial_value_animation.borrow_mut() = Some(animation.clone());
                *self.initial_value_state.borrow_mut() = Some(state.clone());
            }
        }
        let value = self.value_untracked.borrow().clone();
        *self.animation.borrow_mut() = self.still_at(value);
        self.update_duration();
        self.use_only_initial_value.set(true);
    }

    fn clear_initial_animation(&self) {
        *self.initial_value_animation.borrow_mut() = None;
        *self.initial_value_state.borrow_mut() = None;
        self.use_only_initial_value.set(false);
    }

    fn has_initial_value_animation(&self) -> bool {
        self.initial_value_state.borrow().is_some()
    }
}

impl<T: Animate> ToolingEntry for TransitionAnimationState<T> {
    fn duration_nanos(&self) -> i64 {
        self.duration_nanos.get()
    }

    fn label(&self) -> String {
        self.label.clone()
    }

    fn value_any(&self) -> Box<dyn Any> {
        Box::new(self.value.get())
    }

    fn target_value_any(&self) -> Box<dyn Any> {
        Box::new(self.target_value.borrow().clone())
    }

    fn animation_spec(&self) -> AnimationSpec {
        self.animation_spec.borrow().clone()
    }
}

/// Kotlin's `roundToLong`: half up.
fn round_half_up(value: f64) -> i64 {
    (value + 0.5).floor() as i64
}
