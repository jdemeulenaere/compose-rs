//! A scrollable's mouse wheel, as Compose's `MouseWheelScrollingLogic`: a turn too large to apply
//! at once moves by a step, then along a linear tween; the state's `scroll(UserInput)` is held
//! until no turn came for a while, later turns joining the one scroll; when it ends, a custom
//! fling behavior runs. One task per node that was turned, parked between gestures.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::task::Poll;

use compose_animation::{AnimationState, LINEAR_EASING, tween};
use compose_runtime::{CoroutineScope, Job};
use compose_ui::{Offset, PointerEvent, PointerEventPass, VelocityTracker1D};

use crate::gestures::{ScrollScope, ScrollableStateExt};
use crate::mutator::MutatePriority;
use crate::scroll::{ScrollTarget, event_time};

/// A delta at least this long (dp) animates; a shorter one applies at once. Compose's
/// `AnimationThreshold`: the speed over a 60 Hz frame, at the longest animation.
const ANIMATION_THRESHOLD_DP: f32 = 6.0;
/// How fast a turn animates, in dp per millisecond, as `AnimationSpeed`.
const ANIMATION_SPEED_DP_PER_MILLI: f32 = 1.0;
/// The longest a turn's animation runs, as `MaxAnimationDuration`.
const MAX_ANIMATION_MILLIS: f32 = 100.0;
/// How long the scroll stays held for another turn, as `ScrollProgressTimeout`.
const SCROLL_PROGRESS_TIMEOUT_MILLIS: i64 = 50;
/// A delta too small to see, as `isLowScrollingDelta`.
const LOW_DELTA: f32 = 0.5;

/// What a turn asks for, along its axis: the dominant one of `value`, if it's the scrollable's, as
/// `OneDimensionalScrollValueAdapter.toScrollValue`.
fn along_axis(value: Offset, vertical: bool) -> f32 {
    match (value.y.abs() >= value.x.abs(), vertical) {
        (true, true) => value.y,
        (false, false) => value.x,
        _ => 0.0,
    }
}

fn is_low(delta: f32) -> bool {
    delta.abs() < LOW_DELTA
}

/// A wheel's turn: its delta on both axes, when it came, and whether it applies at once (a
/// high-resolution wheel or a trackpad, as `isPreciseWheelScroll`).
#[derive(Clone, Copy)]
struct Turn {
    value: Offset,
    time_millis: i64,
    precise: bool,
}

/// A node's wheel. `stop` it when the node leaves: its task lives until then.
pub(crate) struct MouseWheel {
    shared: Rc<Shared>,
    job: Cell<Job>,
}

struct Shared {
    /// The turns not taken yet, as Compose's unlimited channel.
    turns: RefCell<VecDeque<Turn>>,
    /// The task waits for a first turn: a turn runs it at once, so its step applies in the event.
    parked: Cell<bool>,
    /// The task is there (a delta's consumer panicking takes it with it).
    running: Cell<bool>,
    /// A gesture holds the state's scroll, as `isScrolling`.
    scrolling: Cell<bool>,
    /// What it scrolls, as of the last event.
    target: RefCell<ScrollTarget>,
    /// How fast the turns came, on each axis, as `DifferentialVelocityTracker`: kept across
    /// gestures, its samples ageing out, as Compose's.
    velocity: RefCell<[VelocityTracker1D; 2]>,
    /// The fling a stopped gesture started: it lives on with the node.
    fling: Cell<Option<Job>>,
}

impl MouseWheel {
    pub(crate) fn new(target: ScrollTarget) -> MouseWheel {
        let shared = Rc::new(Shared {
            turns: RefCell::default(),
            parked: Cell::new(true),
            running: Cell::new(true),
            scrolling: Cell::new(false),
            target: RefCell::new(target),
            velocity: RefCell::new([VelocityTracker1D::new(true), VelocityTracker1D::new(true)]),
            fling: Cell::new(None),
        });
        let job = Cell::new(CoroutineScope::new().launch(run(shared.clone())));
        MouseWheel { shared, job }
    }

    pub(crate) fn set_target(&self, target: ScrollTarget) {
        // Not while its deltas are applied (an event sent from a state's consumer).
        if let Ok(mut current) = self.shared.target.try_borrow_mut() {
            *current = target;
        }
    }

    /// An event of the wheel, as `NonTouchScrollingLogic.onPointerEvent`: while a gesture goes on
    /// it's taken on the way in, giving this scrollable priority over those inside; otherwise on
    /// the way out, if this scrollable can use it.
    pub(crate) fn on_pointer_event(&self, event: &mut PointerEvent, pass: PointerEventPass) {
        if event.consumed {
            return;
        }
        let scrolling = self.shared.scrolling.get();
        if pass == PointerEventPass::Initial && scrolling {
            self.on_turn(event);
            event.consume();
        }
        if pass == PointerEventPass::Main && !scrolling && self.on_turn(event) {
            event.consume();
        }
    }

    /// Takes the turn if the state can use it, as `onScrollingEvent` with `canConsumeDelta`;
    /// returns whether the event is to be consumed.
    fn on_turn(&self, event: &PointerEvent) -> bool {
        let can_consume = {
            let target = self.shared.target.borrow();
            let delta = along_axis(event.scroll_delta, target.vertical);
            let toward_state = if target.reverse { -delta } else { delta };
            // Nothing on this axis: something else may handle the other one (a NaN too).
            if toward_state < 0.0 {
                target.state.can_scroll_backward()
            } else if toward_state > 0.0 {
                target.state.can_scroll_forward()
            } else {
                false
            }
        };
        if !can_consume {
            return self.shared.scrolling.get();
        }
        let time_millis = (event_time(event) / 1_000_000) as i64;
        self.shared.turns.borrow_mut().push_back(Turn {
            value: event.scroll_delta,
            time_millis,
            precise: event.precise_wheel,
        });
        if !self.shared.running.get() {
            self.shared.running.set(true);
            self.job.set(CoroutineScope::new().launch(run(self.shared.clone())));
        }
        if self.shared.parked.get() {
            compose_runtime::platform::poll_now(self.job.get());
        }
        true
    }

    /// The node leaves, or another state takes over: the gesture ends at once.
    pub(crate) fn stop(&self) {
        self.job.get().cancel();
        self.shared.scrolling.set(false);
    }

    /// The fling a stopped gesture started ends now, for another state.
    pub(crate) fn cancel_fling(&self) {
        if let Some(fling) = self.shared.fling.take() {
            fling.cancel();
        }
    }
}

async fn run(shared: Rc<Shared>) {
    let _ended = Ended(shared.clone());
    loop {
        let first = shared.receive().await;
        shared.gesture(first).await;
    }
}

/// The task ended, cancelled or by a panic in a delta's consumer: the next turn starts another.
struct Ended(Rc<Shared>);

impl Drop for Ended {
    fn drop(&mut self) {
        self.0.running.set(false);
        self.0.parked.set(true);
        self.0.scrolling.set(false);
    }
}

impl Shared {
    /// The next turn, waiting for one: `MouseWheel::on_turn` runs the task when it comes.
    async fn receive(&self) -> Turn {
        std::future::poll_fn(|_| {
            if let Some(turn) = self.turns.borrow_mut().pop_front() {
                self.parked.set(false);
                return Poll::Ready(turn);
            }
            self.parked.set(true);
            Poll::Pending
        })
        .await
    }

    /// A turn within `timeout_millis`, frames passing meanwhile (Compose's `busyReceive`: what
    /// waits for idle sees the gesture going on).
    async fn next_turn(&self, timeout_millis: i64) -> Option<Turn> {
        if timeout_millis <= 0 {
            return None;
        }
        let deadline = compose_runtime::platform::now_nanos() + timeout_millis as u64 * 1_000_000;
        loop {
            if let Some(turn) = self.turns.borrow_mut().pop_front() {
                return Some(turn);
            }
            if compose_runtime::platform::now_nanos() >= deadline {
                return None;
            }
            compose_runtime::with_frame_nanos(|_| ()).await;
        }
    }

    /// The turns pending, summed and tracked as one at the latest's time, as `sumOrNull`: several
    /// become one target.
    fn take_pending(&self) -> Option<Offset> {
        let mut sum: Option<(Offset, i64)> = None;
        while let Some(turn) = self.turns.borrow_mut().pop_front() {
            let (value, time) = sum.unwrap_or((Offset::ZERO, turn.time_millis));
            sum = Some((value + turn.value, time.max(turn.time_millis)));
        }
        let (value, time_millis) = sum?;
        self.track(value, time_millis);
        Some(value)
    }

    fn track(&self, value: Offset, time_millis: i64) {
        let mut velocity = self.velocity.borrow_mut();
        velocity[0].add_data_point(time_millis, value.x);
        velocity[1].add_data_point(time_millis, value.y);
    }

    fn vertical(&self) -> bool {
        self.target.borrow().vertical
    }

    /// Scrolls by `delta` through the connections around, as `dispatchMouseWheelScroll`; returns
    /// what they and the state used.
    fn dispatch(&self, scope: &ScrollScope, delta: f32) -> f32 {
        self.target.borrow().user_scroll(scope, delta)
    }

    /// A gesture from its first turn: the state's scroll held until no turn came for
    /// `ScrollProgressTimeout`, then the fling behavior.
    async fn gesture(self: &Rc<Self>, first: Turn) {
        self.track(first.value, first.time_millis);
        let mut value = first.value;
        if let Some(more) = self.take_pending() {
            value = value + more;
        }
        let target = along_axis(value, self.vertical());
        if is_low(target) {
            return;
        }
        let state = self.target.borrow().state.clone();
        let last_target = Rc::new(Cell::new(target));
        self.scrolling.set(true);
        let (shared, kept) = (self.clone(), last_target.clone());
        let scrolled = state
            .scroll(MutatePriority::UserInput, move |scope| async move {
                shared.play(&scope, value, first.precise, &kept).await;
            })
            .await;
        self.scrolling.set(false);
        // Interrupted, what interrupted it goes on: nothing to fling, as Compose's cancellation.
        if scrolled.is_ok() {
            self.stopped(last_target.get());
        }
    }

    /// The turns of a gesture, as the block of `dispatchMouseWheelScroll`. `precise` is the first
    /// turn's, for all of them: one that looks otherwise isn't taken for what it isn't.
    async fn play(
        &self,
        scope: &ScrollScope,
        first: Offset,
        precise: bool,
        last_target: &Cell<f32>,
    ) {
        let (threshold, speed) = {
            let density = self.target.borrow().density;
            (ANIMATION_THRESHOLD_DP * density, ANIMATION_SPEED_DP_PER_MILLI * density)
        };
        let mut value = first;
        let mut target = along_axis(value, self.vertical());
        // Until a turn needs a tween, none: most apply at once.
        let mut animation: Option<AnimationState<f32>> = None;
        let mut go_on = true;
        while go_on {
            go_on = false;
            let at = animation.as_ref().map_or(0.0, AnimationState::value);
            let left = target - at;
            let wait = if precise || left.abs() < threshold {
                self.dispatch(scope, left);
                SCROLL_PROGRESS_TIMEOUT_MILLIS
            } else {
                // The animation starts at the next frame: a step now, so nothing lags.
                let step = threshold.copysign(left);
                self.dispatch(scope, step);
                let current = at + step;
                let moving = match animation.take() {
                    Some(animation) => AnimationState::with(
                        current,
                        animation.velocity_vector(),
                        animation.last_frame_time_nanos(),
                        animation.finished_time_nanos(),
                        animation.is_running(),
                    ),
                    None => AnimationState::new(current),
                };
                let duration = ((target - current).abs() / speed).round().min(MAX_ANIMATION_MILLIS);
                let mut last = current;
                let spec = tween(duration as u32).easing(LINEAR_EASING);
                moving
                    .animate_to(target, spec, true, |frame| {
                        let delta = frame.value() - last;
                        if !is_low(delta) {
                            let used = self.dispatch(scope, delta);
                            if !is_low(delta - used) {
                                frame.cancel_animation();
                                return;
                            }
                            last += delta;
                        }
                        // The turns that came: the target moves, the tween starts over from here.
                        if let Some(more) = self.take_pending() {
                            value = value + more;
                            target = along_axis(value, self.vertical());
                            go_on = !is_low(target - last);
                            frame.cancel_animation();
                        }
                    })
                    .await;
                animation = Some(moving);
                SCROLL_PROGRESS_TIMEOUT_MILLIS - duration as i64
            };
            if !go_on && let Some(turn) = self.next_turn(wait).await {
                // A turn after the last finished: a target of its own, from its own start.
                value = turn.value;
                target = along_axis(value, self.vertical());
                animation = None;
                self.track(turn.value, turn.time_millis);
                go_on = !is_low(target);
            }
        }
        last_target.set(target);
    }

    /// The gesture ended: the fling behavior at the turns' speed, or, of one turn, the animation's
    /// the way it went, as `onScrollStopped(velocity, isMouseWheel = true)`. The default inertia
    /// doesn't run; a custom behavior does.
    fn stopped(&self, last_target: f32) {
        let target = self.target.borrow().clone();
        let default = target.fling_behavior.is_none();
        let triggers = target
            .fling_behavior
            .as_ref()
            .is_some_and(|behavior| behavior.should_be_triggered_by_mouse_wheel());
        if default || !triggers {
            return;
        }
        let (x, y) = {
            let velocity = self.velocity.borrow();
            (
                velocity[0].calculate_velocity_with(f32::MAX),
                velocity[1].calculate_velocity_with(f32::MAX),
            )
        };
        let velocity = if x == 0.0 && y == 0.0 {
            // A single turn: its size over the animation's longest, at most the animation's speed.
            let speed = ANIMATION_SPEED_DP_PER_MILLI * target.density;
            let per_milli = (last_target.abs() / MAX_ANIMATION_MILLIS).min(speed);
            if last_target == 0.0 { 0.0 } else { (per_milli * 1000.0).copysign(last_target) }
        } else if target.vertical {
            y
        } else {
            x
        };
        self.fling.set(Some(target.launch_fling(velocity)));
    }
}
