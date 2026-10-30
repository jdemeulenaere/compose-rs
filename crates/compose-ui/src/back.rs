//! Back navigation, as AndroidX's `OnBackPressedDispatcher`, `BackHandler` and
//! `PredictiveBackHandler`, on navigation events ([`compose_navigation_event`]): the host owns a
//! dispatcher (`LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER`) where handlers register, and an input a
//! back press or gesture comes through (Android's back gesture or button, Escape on desktop when
//! nothing focused takes it), which goes to the innermost enabled one. The platform asks
//! [`has_enabled_callbacks`] after each frame, so it intercepts back only while a handler wants it
//! (otherwise Android's activity finishes, as usual).
//!
//! Handlers in a dialog or a focusable popup come first, as Compose's dialogs are windows of
//! their own that get back before the window beneath; among the others, the one registered last
//! (composed last) wins, as AndroidX's dispatcher.
//!
//! [`has_enabled_callbacks`]: OnBackPressedDispatcher::has_enabled_callbacks

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use compose_lifecycle::{LifecycleStartEffect, LifecycleStartStopEffectScope};
pub(crate) use compose_navigation_event::LOCAL_BACK_LAYER;
use compose_navigation_event::{
    DirectNavigationEventInput, LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER, NavigationEvent,
    NavigationEventCallbacks, NavigationEventDispatcher, NavigationEventHandler,
    NavigationEventInfo, NavigationEventInputCallbacks,
};
use compose_runtime::{
    Callback, CompositionLocal, DisposableEffect, Job, SideEffect, composable,
    current_composite_key_hash, remember_keyed,
};

/// What `PredictiveBackHandler` gets of a gesture, as AndroidX's `BackEventCompat`.
pub type BackEventCompat = NavigationEvent;

/// How back reaches the host's handlers, as AndroidX's `OnBackPressedDispatcher` where it's a layer
/// over a `NavigationEventDispatcher`: owned by the host.
#[derive(Clone)]
pub struct OnBackPressedDispatcher {
    dispatcher: NavigationEventDispatcher,
    input: DirectNavigationEventInput,
}

impl compose_runtime::ComposeArg for OnBackPressedDispatcher {
    fn same(&self, other: &Self) -> bool {
        self.input == other.input
    }
}

/// Tells the host: an enabled handler changes outside composition too (a lifecycle's start), and
/// the platform learns of it from a frame.
struct BackHandlersChanged(Box<dyn Fn()>);

impl NavigationEventInputCallbacks for BackHandlersChanged {
    fn on_has_enabled_back_handlers_changed(&self, _: bool) {
        (self.0)();
    }
}

impl OnBackPressedDispatcher {
    /// With an input of its own on `dispatcher`, telling `on_handlers_changed` when it comes to have
    /// an enabled back handler or none.
    pub fn new(
        dispatcher: &NavigationEventDispatcher,
        on_handlers_changed: impl Fn() + 'static,
    ) -> OnBackPressedDispatcher {
        let callbacks = BackHandlersChanged(Box::new(on_handlers_changed));
        let input = DirectNavigationEventInput::with_callbacks(callbacks);
        dispatcher.add_input(&input);
        OnBackPressedDispatcher { dispatcher: dispatcher.clone(), input }
    }

    /// Whether a handler is enabled: a back press would go to it, so the platform intercepts it.
    pub fn has_enabled_callbacks(&self) -> bool {
        self.input.has_enabled_back_handlers()
    }

    /// Back is pressed, or a gesture ends, the innermost enabled handler taking it, or the one in
    /// the gesture, as `onBackPressed`. Returns whether one did.
    pub fn on_back_pressed(&self) -> bool {
        let handled = self.dispatcher.takes_back();
        self.input.back_completed();
        handled
    }

    /// A back gesture starts, as `dispatchOnBackStarted`.
    pub fn dispatch_on_back_started(&self, event: BackEventCompat) {
        self.input.back_started(event);
    }

    /// It goes on, as `dispatchOnBackProgressed`.
    pub fn dispatch_on_back_progressed(&self, event: BackEventCompat) {
        self.input.back_progressed(event);
    }

    /// It's given up, as `dispatchOnBackCancelled`.
    pub fn dispatch_on_back_cancelled(&self) {
        self.input.back_cancelled();
    }
}

/// How back reaches this content, as Compose's `LocalOnBackPressedDispatcherOwner`: the host's, or
/// none.
pub static LOCAL_ON_BACK_PRESSED_DISPATCHER: CompositionLocal<Option<OnBackPressedDispatcher>> =
    CompositionLocal::new("LocalOnBackPressedDispatcher", || None);

/// Whether the `BackHandler`s of the content may take back: `false` disables them all, as
/// Navigation Compose's back stack entries that aren't resumed (an entry fading out, under a
/// `NavHost` transition) disable theirs through their lifecycle. A container providing it
/// provides `false` inside content already disabled.
pub static LOCAL_BACK_HANDLERS_ENABLED: CompositionLocal<bool> =
    CompositionLocal::new("LocalBackHandlersEnabled", || true);

/// What a handler tells of itself, as AndroidX's `BackHandlerInfo`: its place.
#[derive(PartialEq, Debug)]
struct BackHandlerInfo(u64);

/// Keeps `handler` on the dispatcher, enabled while `enabled` and the lifecycle's at least started,
/// as `BackHandler` and `PredictiveBackHandler` do both.
fn registered(
    dispatcher: NavigationEventDispatcher,
    handler: NavigationEventHandler,
    enabled: bool,
    on_enabled: impl Fn(bool) + 'static,
) {
    let on_enabled = Rc::new(on_enabled);
    let (enabling, stopping) = (on_enabled.clone(), on_enabled);
    LifecycleStartEffect!(
        (enabled, handler.id()),
        effects = move |scope: LifecycleStartStopEffectScope| {
            enabling(enabled);
            let stopping = stopping.clone();
            scope.on_stop_or_dispose(move |_| stopping(false))
        }
    );
    let layer = LOCAL_BACK_LAYER.current();
    DisposableEffect((dispatcher.clone(), handler.id()), move |scope| {
        dispatcher.add_handler_in_layer(&handler, layer);
        scope.on_dispose(move || handler.remove())
    });
}

struct BackCallbacks {
    on_back: Rc<RefCell<Callback>>,
}

impl NavigationEventCallbacks for BackCallbacks {
    fn on_back_completed(&self, _: &NavigationEventHandler) {
        let on_back = self.on_back.borrow().clone();
        on_back();
    }
}

/// Calls `on_back` when the user goes back (Android's back gesture or button, Escape on
/// desktop) while `enabled`, as AndroidX's `BackHandler`: the innermost enabled handler gets
/// it (those in dialogs and focusable popups first, then the one composed last). While any is
/// enabled, back doesn't leave the app. It's disabled while its lifecycle isn't started, and where
/// [`LOCAL_BACK_HANDLERS_ENABLED`] is `false`.
#[composable]
#[allow(non_snake_case)]
pub fn BackHandler(#[default(true)] enabled: bool, on_back: impl Fn() + 'static) {
    let Some(dispatcher) = LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER.current() else { return };
    let enabled = enabled && LOCAL_BACK_HANDLERS_ENABLED.current();
    let key = current_composite_key_hash();
    let (handler, current) = {
        let on_back = on_back.clone();
        remember_keyed((dispatcher.clone(), key), move || {
            let current = Rc::new(RefCell::new(on_back));
            let handler = NavigationEventHandler::new(
                NavigationEventInfo::new(BackHandlerInfo(key)),
                false,
                false,
                BackCallbacks { on_back: current.clone() },
            );
            (handler, current)
        })
    };
    // Once the pass is applied, as Compose's `SideEffect` updating the callback.
    SideEffect(move || *current.borrow_mut() = on_back);
    let enabling = handler.clone();
    registered(dispatcher, handler, enabled, move |enabled| enabling.set_back_enabled(enabled));
}

/// A gesture given up, what [`BackProgress::next`] says, as Compose's `CancellationException`
/// "onBack cancelled". A handler's task ends at its next wait after that.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BackCancelled;

impl std::fmt::Display for BackCancelled {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("onBack cancelled")
    }
}

impl std::error::Error for BackCancelled {}

/// How many events wait for the task; more are dropped, as `Channel.BUFFERED` with `trySend`.
const BUFFERED: usize = 64;

/// A gesture's channel, as `activeChannel`, and the task it wakes.
#[derive(Default)]
struct BackChannel {
    events: RefCell<VecDeque<BackEventCompat>>,
    closed: Cell<bool>,
    cancelled: Cell<bool>,
    /// The collector's, which can be in another task than the gesture's.
    waker: RefCell<Option<Waker>>,
    /// The gesture's task's, which a cancellation ends whatever it waits for.
    task_waker: RefCell<Option<Waker>>,
    collected: Cell<bool>,
}

impl BackChannel {
    fn wake(&self) {
        let waker = self.waker.borrow_mut().take();
        if let Some(waker) = waker {
            waker.wake();
        }
        let waker = self.task_waker.borrow_mut().take();
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    fn send(&self, event: BackEventCompat) {
        let mut events = self.events.borrow_mut();
        if events.len() < BUFFERED {
            events.push_back(event);
            drop(events);
            self.wake();
        }
    }

    fn close(&self) {
        self.closed.set(true);
        self.wake();
    }

    fn cancel(&self) {
        self.cancelled.set(true);
        self.wake();
    }
}

/// A gesture's progress, what [`PredictiveBackHandler`]'s `on_back` is given, as Compose's
/// `Flow<BackEventCompat>`, and which it must collect: its events, ended when the gesture completes
/// (back is taken), or with [`BackCancelled`] when it's given up.
pub struct BackProgress {
    channel: Rc<BackChannel>,
}

impl BackProgress {
    /// The next event, as a collector's next value: `None` when the gesture completes.
    pub async fn next(&mut self) -> Result<Option<BackEventCompat>, BackCancelled> {
        self.channel.collected.set(true);
        std::future::poll_fn(|context| {
            let channel = &self.channel;
            if channel.cancelled.get() {
                return Poll::Ready(Err(BackCancelled));
            }
            if let Some(event) = channel.events.borrow_mut().pop_front() {
                return Poll::Ready(Ok(Some(event)));
            }
            if channel.closed.get() {
                return Poll::Ready(Ok(None));
            }
            *channel.waker.borrow_mut() = Some(context.waker().clone());
            Poll::Pending
        })
        .await
    }

    /// Runs `on_event` on each event until the gesture ends, as `collect { … }`: `Ok` when it
    /// completes, `Err` when it's given up. What follows runs in both, up to its next wait, where
    /// Compose's `collect` throws past it: commit on `Ok` alone.
    pub async fn collect(
        mut self,
        mut on_event: impl FnMut(BackEventCompat),
    ) -> Result<(), BackCancelled> {
        while let Some(event) = self.next().await? {
            on_event(event);
        }
        Ok(())
    }
}

type BoxedFuture = Pin<Box<dyn Future<Output = ()>>>;
type OnBack = Rc<dyn Fn(BackProgress) -> BoxedFuture>;

/// A gesture's task, as `launchNewGesture`: it doesn't start if the gesture's given up first, ends
/// at its next wait once it's given up (Compose's cancelled job), and panics if the handler never
/// collected.
async fn gesture(on_back: OnBack, channel: Rc<BackChannel>, enabled: Rc<dyn Fn() -> bool>) {
    if channel.cancelled.get() || !enabled() {
        return;
    }
    let mut future = on_back(BackProgress { channel: channel.clone() });
    std::future::poll_fn(move |context: &mut Context<'_>| {
        if let Poll::Ready(()) = future.as_mut().poll(context) {
            // Given up, what it waited for ended it, as Compose's cancelled wait.
            assert!(
                channel.cancelled.get() || channel.collected.get(),
                "You must collect the progress flow"
            );
            return Poll::Ready(());
        }
        if channel.cancelled.get() {
            return Poll::Ready(());
        }
        // What it waits for, or this: a gesture given up ends it whatever it waits for.
        *channel.task_waker.borrow_mut() = Some(context.waker().clone());
        Poll::Pending
    })
    .await;
}

/// `ComposePredictiveBackHandler`, its state for a gesture at a time.
struct PredictiveState {
    handler_enabled: Rc<Cell<bool>>,
    on_back: RefCell<OnBack>,
    active: RefCell<Option<(Rc<BackChannel>, Job)>>,
    is_predictive_back: Cell<bool>,
}

impl PredictiveState {
    fn launch_new_gesture(&self) {
        let channel = Rc::new(BackChannel::default());
        let enabled = self.handler_enabled.clone();
        // Not a task of the composition's scope, which would drop it unpolled when it leaves: it ends
        // by its channel, after what it does with the cancellation (Compose's `catch`, `finally`).
        let job = compose_runtime::platform::launch(gesture(
            self.on_back.borrow().clone(),
            channel.clone(),
            Rc::new(move || enabled.get()),
        ));
        *self.active.borrow_mut() = Some((channel, job));
    }

    fn cancelled(&self) {
        let active = self.active.borrow_mut().take();
        if let Some((channel, _)) = active {
            channel.cancel();
        }
        self.is_predictive_back.set(false);
    }

    /// Mirrors `enabled`: disabled with a task that's over, what's left of it goes, as its setter.
    fn set_enabled(&self, handler: &NavigationEventHandler, enabled: bool) {
        let over = self.active.borrow().as_ref().is_some_and(|(_, job)| !job.is_active());
        if !enabled && self.handler_enabled.get() && over {
            self.cancelled();
        }
        self.handler_enabled.set(enabled);
        handler.set_back_enabled(enabled);
    }
}

struct PredictiveCallbacks {
    state: Rc<PredictiveState>,
}

impl NavigationEventCallbacks for PredictiveCallbacks {
    fn on_back_started(&self, _: &NavigationEventHandler, _: &NavigationEvent) {
        // If the last gesture wasn't ended.
        self.state.cancelled();
        if self.state.handler_enabled.get() {
            self.state.is_predictive_back.set(true);
            self.state.launch_new_gesture();
        }
    }

    fn on_back_progressed(&self, _: &NavigationEventHandler, event: &NavigationEvent) {
        if let Some((channel, _)) = &*self.state.active.borrow() {
            channel.send(*event);
        }
    }

    fn on_back_completed(&self, _: &NavigationEventHandler) {
        let state = &self.state;
        // Not predictive: its completion goes through a session of its own.
        if state.active.borrow().is_some() && !state.is_predictive_back.get() {
            state.cancelled();
        }
        if state.active.borrow().is_none() {
            state.is_predictive_back.set(false);
            state.launch_new_gesture();
        }
        if let Some((channel, _)) = &*state.active.borrow() {
            channel.close();
        }
        state.is_predictive_back.set(false);
    }

    fn on_back_cancelled(&self, _: &NavigationEventHandler) {
        self.state.cancelled();
    }
}

/// Takes back with its progress, as AndroidX's `PredictiveBackHandler`: each gesture while `enabled`
/// runs `on_back` as a task of this place, with the gesture's [`BackProgress`], which it must
/// collect. Where it comes in the handlers, and when it's disabled, as [`BackHandler`]. A press with
/// no gesture runs it too, with no events.
#[composable]
#[allow(non_snake_case)]
pub fn PredictiveBackHandler<F: Future<Output = ()> + 'static>(
    #[default(true)] enabled: bool,
    on_back: impl Fn(BackProgress) -> F + 'static,
) {
    let Some(dispatcher) = LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER.current() else { return };
    let enabled = enabled && LOCAL_BACK_HANDLERS_ENABLED.current();
    let on_back: OnBack = Rc::new(move |progress| Box::pin(on_back(progress)));
    let key = current_composite_key_hash();
    let (handler, state) = {
        let on_back = on_back.clone();
        remember_keyed((dispatcher.clone(), key), move || {
            let state = Rc::new(PredictiveState {
                handler_enabled: Rc::new(Cell::new(false)),
                on_back: RefCell::new(on_back),
                active: RefCell::new(None),
                is_predictive_back: Cell::new(false),
            });
            let handler = NavigationEventHandler::new(
                NavigationEventInfo::new(BackHandlerInfo(key)),
                false,
                false,
                PredictiveCallbacks { state: state.clone() },
            );
            (handler, state)
        })
    };
    let updating = state.clone();
    SideEffect(move || *updating.on_back.borrow_mut() = on_back);
    // Its task doesn't outlive this place, gesture or no gesture.
    let leaving = state.clone();
    DisposableEffect((dispatcher.clone(), key), move |scope| {
        scope.on_dispose(move || leaving.cancelled())
    });
    let enabling = handler.clone();
    registered(dispatcher, handler, enabled, move |enabled| state.set_enabled(&enabling, enabled));
}
