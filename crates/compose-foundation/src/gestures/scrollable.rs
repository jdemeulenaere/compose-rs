//! Scrolling as Compose's `foundation.gestures` has it: `ScrollableState` (what scrolls, one
//! mutation at a time), `ScrollScope` (a scroll's deltas), `FlingBehavior` (what a release
//! does), and `Modifier.scrollable`, which turns drags, the wheel and flings into deltas for a
//! state. `vertical_scroll`, the lazy lists and grids drive their own states the same way.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use compose_animation::{AnimationSpec, animate};
use compose_runtime::{ComposeArg, CoroutineScope, Job, MutableState, OwnedState, composable};
use compose_ui::{
    EventContext, IntSize, Modifier, ModifierNode, ModifierNodeElement, MutableInteractionSource,
    NestedScrollSource, PointerEvent, PointerEventPass, PointerInputModifierNode,
    platform::NestedScrollChain,
};

use crate::gestures::Orientation;
use crate::mutator::{MutatePriority, MutationInterrupted, MutatorMutex};
use crate::scroll::same_fling;

/// A boxed future borrowing for `'a`, as the object-safe parts of the scrolling traits return.
pub type LocalBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// A scroll's block, boxed: what [`ScrollableState::scroll_boxed`] runs with the scope.
pub type ScrollBlock<'a> = Box<dyn FnOnce(ScrollScope) -> LocalBoxFuture<'a, ()> + 'a>;

/// What a scroll's block scrolls with, as Compose's `ScrollScope`: `scroll_by` consumes a delta
/// (pixels, positive towards the end) and returns the part it used. Cheap to clone; a clone
/// scrolls the same.
#[derive(Clone)]
pub struct ScrollScope {
    scroll_by: Rc<dyn Fn(f32) -> f32>,
}

impl ScrollScope {
    /// A scope whose `scroll_by` calls `scroll_by`.
    pub fn new(scroll_by: impl Fn(f32) -> f32 + 'static) -> ScrollScope {
        ScrollScope { scroll_by: Rc::new(scroll_by) }
    }

    /// Scrolls by `pixels` (positive: towards the end); returns the pixels used, as
    /// `ScrollScope.scrollBy`.
    pub fn scroll_by(&self, pixels: f32) -> f32 {
        (self.scroll_by)(pixels)
    }
}

impl std::fmt::Debug for ScrollScope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ScrollScope")
    }
}

/// What scrolls, as Compose's `ScrollableState`: [`scroll`](ScrollableStateExt::scroll) runs a
/// block that scrolls with a [`ScrollScope`], one block at a time by priority (a drag
/// interrupts an animated scroll; a block of a higher priority refuses one of a lower), and
/// [`dispatch_raw_delta`](Self::dispatch_raw_delta) scrolls at once, outside any.
///
/// `ScrollState`, `LazyListState`, `LazyGridState` and [`DefaultScrollableState`] (from
/// [`scrollable_state`]) are scrollable states. The convenience methods (`scroll`, `scroll_by`,
/// `animate_scroll_by`, `stop_scroll`) are in [`ScrollableStateExt`], on every state.
///
/// Where Compose's suspend functions are cancelled by a mutation interrupting them, compose-rs's
/// return [`MutationInterrupted`] (D850).
pub trait ScrollableState {
    /// Runs `block` with this state's scroll scope, as a mutation of `priority` (see
    /// [`MutatorMutex`]): Compose's `scroll`, boxed so the trait is object-safe; call
    /// [`ScrollableStateExt::scroll`].
    fn scroll_boxed<'a>(
        &'a self,
        priority: MutatePriority,
        block: ScrollBlock<'a>,
    ) -> LocalBoxFuture<'a, Result<(), MutationInterrupted>>;

    /// Scrolls by `delta` pixels at once, whatever scroll is going on, as `dispatchRawDelta`;
    /// returns the pixels used.
    fn dispatch_raw_delta(&self, delta: f32) -> f32;

    /// Runs `block` in place in a scroll of `priority` that ends with it (a wheel turn's, D879),
    /// with the state's scroll scope: `Some(whether it ran)`, or `None` where the state has no
    /// such scroll (then a scroll in a task polled at once runs it). compose-rs's states have
    /// one. Temporary: it goes with `MutatorMutex::mutate_now` when `MouseWheelScrollingLogic` is
    /// ported, whose one task per gesture removes the cost per turn that justifies it.
    #[doc(hidden)]
    fn scroll_now(
        &self,
        priority: MutatePriority,
        block: &mut dyn FnMut(&ScrollScope),
    ) -> Option<bool> {
        let _ = (priority, block);
        None
    }

    /// Whether a scroll (a block, a drag, a fling) is going on, as `isScrollInProgress` (a
    /// state read).
    fn is_scroll_in_progress(&self) -> bool;

    /// Whether it can scroll towards its end, as `canScrollForward` (a state read).
    fn can_scroll_forward(&self) -> bool {
        true
    }

    /// Whether it can scroll towards its start, as `canScrollBackward` (a state read).
    fn can_scroll_backward(&self) -> bool {
        true
    }

    /// Whether the last scroll in a scope moved towards the end, as `lastScrolledForward` (a
    /// state read).
    fn last_scrolled_forward(&self) -> bool {
        false
    }

    /// Whether the last scroll in a scope moved towards the start, as `lastScrolledBackward`.
    fn last_scrolled_backward(&self) -> bool {
        false
    }

    /// The interaction source a state's own scrolling container reports its drags to (Compose's
    /// `internalInteractionSource` of `ScrollState` and the lazy states), when the modifier is
    /// given none.
    #[doc(hidden)]
    fn internal_interaction_source(&self) -> Option<MutableInteractionSource> {
        None
    }
}

/// The scrolling functions every [`ScrollableState`] has, as Compose's `scroll` and the
/// `ScrollableState` extensions (`scrollBy`, `animateScrollBy`, `stopScroll`).
pub trait ScrollableStateExt: ScrollableState {
    /// Runs `block` with the state's scroll scope as a mutation of `priority`, as `scroll`:
    /// the scroll going on of the same or a lower priority is interrupted first, one of a
    /// higher priority refuses this one. `is_scroll_in_progress` while it runs.
    fn scroll<'a, F, Fut>(
        &'a self,
        priority: MutatePriority,
        block: F,
    ) -> LocalBoxFuture<'a, Result<(), MutationInterrupted>>
    where
        F: FnOnce(ScrollScope) -> Fut + 'a,
        Fut: Future<Output = ()> + 'a,
    {
        self.scroll_boxed(priority, Box::new(move |scope| Box::pin(block(scope))))
    }

    /// Scrolls by `value` pixels in a scroll of the default priority, as `scrollBy`; returns the
    /// pixels used.
    fn scroll_by(&self, value: f32) -> LocalBoxFuture<'_, Result<f32, MutationInterrupted>> {
        Box::pin(async move {
            let consumed = Rc::new(Cell::new(0.0));
            let into = consumed.clone();
            self.scroll(MutatePriority::Default, move |scope| async move {
                into.set(scope.scroll_by(value));
            })
            .await?;
            Ok(consumed.get())
        })
    }

    /// Scrolls by `value` pixels along `spec`, a step a frame, in a scroll of the default
    /// priority, as `animateScrollBy`; returns the pixels used.
    fn animate_scroll_by(
        &self,
        value: f32,
        spec: impl Into<AnimationSpec>,
    ) -> LocalBoxFuture<'_, Result<f32, MutationInterrupted>> {
        let spec = spec.into();
        Box::pin(async move {
            let consumed = Rc::new(Cell::new(0.0f32));
            let into = consumed.clone();
            self.scroll(MutatePriority::Default, move |scope| async move {
                animate(0.0, value, 0.0, spec, |current, _| {
                    into.set(into.get() + scope.scroll_by(current - into.get()));
                })
                .await;
            })
            .await?;
            Ok(consumed.get())
        })
    }

    /// Stops the scroll going on (of `priority` or lower) with an empty one, as `stopScroll`.
    fn stop_scroll(
        &self,
        priority: MutatePriority,
    ) -> LocalBoxFuture<'_, Result<(), MutationInterrupted>> {
        self.scroll(priority, |_| async {})
    }
}

impl<T: ScrollableState + ?Sized> ScrollableStateExt for T {}

/// What a scroll's states share, as Compose's `DefaultScrollableState`: the mutex, whether a
/// scroll goes on, which way the last went, and the interaction source of the state's own
/// container.
pub(crate) struct ScrollCore {
    mutex: MutatorMutex,
    scrolling: OwnedState<bool>,
    last_forward: OwnedState<bool>,
    last_backward: OwnedState<bool>,
    /// Bumped by each scroll that runs: an interrupted one, ending later, leaves the flag to
    /// the one that interrupted it.
    generation: Cell<u64>,
    pub(crate) interaction_source: MutableInteractionSource,
    /// The scope of the scrolls run in place (`scroll_now`), made once: a wheel turn's.
    now_scope: std::cell::OnceCell<ScrollScope>,
}

impl ScrollCore {
    #[track_caller]
    pub(crate) fn new() -> ScrollCore {
        ScrollCore {
            mutex: MutatorMutex::new(),
            scrolling: compose_runtime::mutable_state_of(false),
            last_forward: compose_runtime::mutable_state_of(false),
            last_backward: compose_runtime::mutable_state_of(false),
            generation: Cell::new(0),
            interaction_source: MutableInteractionSource::new(),
            now_scope: std::cell::OnceCell::new(),
        }
    }

    /// A scroll in place says so without writing the state: it ends before anything that
    /// observes the state could run (D879).
    pub(crate) fn is_scroll_in_progress(&self) -> bool {
        self.scrolling.get() || self.mutex.is_mutating_now()
    }

    /// Whether a scroll is going on, without reading state (what reads it isn't told when it
    /// changes): a jump asked from composition doesn't subscribe its caller.
    pub(crate) fn is_scroll_in_progress_untracked(&self) -> bool {
        self.mutex.is_mutating()
    }

    pub(crate) fn last_scrolled_forward(&self) -> bool {
        self.last_forward.get()
    }

    pub(crate) fn last_scrolled_backward(&self) -> bool {
        self.last_backward.get()
    }

    /// The scope a scroll's block gets: `consume` scrolls, the direction is noted (a NaN
    /// scrolls nothing), as `DefaultScrollableState`'s scope.
    pub(crate) fn scope(&self, consume: Rc<dyn Fn(f32) -> f32>) -> ScrollScope {
        let (forward, backward) = (self.last_forward.handle(), self.last_backward.handle());
        ScrollScope::new(move |pixels| {
            if pixels.is_nan() {
                return 0.0;
            }
            let used = consume(pixels);
            note_direction(forward, backward, used);
            used
        })
    }

    /// Runs `block` now with `scope` (the state's), as a mutation of `priority` that ends with
    /// it, `is_scroll_in_progress` meanwhile: a wheel turn's scroll (D879). Returns whether it
    /// ran (a scroll of a higher priority going on refuses it).
    pub(crate) fn scroll_now(
        &self,
        priority: MutatePriority,
        consume: impl FnOnce() -> Rc<dyn Fn(f32) -> f32>,
        block: &mut dyn FnMut(&ScrollScope),
    ) -> bool {
        let scope = self.now_scope.get_or_init(|| self.scope(consume()));
        self.mutex
            .mutate_now(priority, || {
                // A scroll interrupted leaves the flag to this one, and this one, ending in
                // place, clears it.
                let generation = self.generation.get().wrapping_add(1);
                self.generation.set(generation);
                if self.scrolling.try_get() == Some(true) {
                    let _ = self.scrolling.try_set(false);
                }
                block(scope);
            })
            .is_some()
    }

    /// Runs `block` with a scope consuming through `consume`, as a mutation of `priority`.
    pub(crate) async fn scroll(
        &self,
        priority: MutatePriority,
        consume: Rc<dyn Fn(f32) -> f32>,
        block: ScrollBlock<'_>,
    ) -> Result<(), MutationInterrupted> {
        let scope = self.scope(consume);
        self.mutex
            .mutate(priority, async move {
                let generation = self.generation.get().wrapping_add(1);
                self.generation.set(generation);
                let _ = self.scrolling.try_set(true);
                let _scrolling = ScrollingFlag { core: self, generation };
                block(scope).await;
            })
            .await
    }
}

/// Notes which way a scroll went (`used` pixels), unless the states went.
fn note_direction(forward: MutableState<bool>, backward: MutableState<bool>, used: f32) {
    if forward.try_get() != Some(used > 0.0) {
        let _ = forward.try_set(used > 0.0);
    }
    if backward.try_get() != Some(used < 0.0) {
        let _ = backward.try_set(used < 0.0);
    }
}

/// Clears `is_scroll_in_progress` when a scroll's block ends however it ends, unless a later
/// scroll took over.
struct ScrollingFlag<'a> {
    core: &'a ScrollCore,
    generation: u64,
}

impl Drop for ScrollingFlag<'_> {
    fn drop(&mut self) {
        if self.core.generation.get() != self.generation {
            return;
        }
        let _ = self.core.scrolling.try_set(false);
    }
}

/// The scrollable state [`scrollable_state`] makes, as Compose's `ScrollableState(consume)`:
/// its deltas go to the function given. Clones share it.
#[derive(Clone)]
pub struct DefaultScrollableState {
    inner: Rc<DefaultInner>,
}

struct DefaultInner {
    core: ScrollCore,
    on_delta: Rc<dyn Fn(f32) -> f32>,
}

/// A scrollable state whose deltas go to `consume_scroll_delta` (pixels, positive towards the
/// end; it returns the part it used), as Compose's `ScrollableState(consumeScrollDelta)`.
#[track_caller]
pub fn scrollable_state(
    consume_scroll_delta: impl Fn(f32) -> f32 + 'static,
) -> DefaultScrollableState {
    DefaultScrollableState {
        inner: Rc::new(DefaultInner {
            core: ScrollCore::new(),
            on_delta: Rc::new(consume_scroll_delta),
        }),
    }
}

/// A scrollable state remembered at its call site, as `rememberScrollableState`: the newest
/// `consume_scroll_delta` gets the deltas.
#[composable(non_restartable)]
pub fn remember_scrollable_state(
    consume_scroll_delta: impl Fn(f32) -> f32 + 'static,
) -> DefaultScrollableState {
    let latest = compose_runtime::remember(|| {
        Rc::new(RefCell::new(None::<compose_runtime::Callback<dyn Fn(f32) -> f32>>))
    });
    *latest.borrow_mut() = Some(consume_scroll_delta);
    compose_runtime::remember(move || {
        let latest = latest.clone();
        scrollable_state(move |delta| {
            let consume = latest.borrow().clone();
            consume.map_or(0.0, |consume| consume(delta))
        })
    })
}

impl ScrollableState for DefaultScrollableState {
    fn scroll_boxed<'a>(
        &'a self,
        priority: MutatePriority,
        block: ScrollBlock<'a>,
    ) -> LocalBoxFuture<'a, Result<(), MutationInterrupted>> {
        Box::pin(self.inner.core.scroll(priority, self.inner.on_delta.clone(), block))
    }

    fn dispatch_raw_delta(&self, delta: f32) -> f32 {
        (self.inner.on_delta)(delta)
    }

    fn scroll_now(
        &self,
        priority: MutatePriority,
        block: &mut dyn FnMut(&ScrollScope),
    ) -> Option<bool> {
        Some(self.inner.core.scroll_now(priority, || self.inner.on_delta.clone(), block))
    }

    fn is_scroll_in_progress(&self) -> bool {
        self.inner.core.is_scroll_in_progress()
    }

    fn last_scrolled_forward(&self) -> bool {
        self.inner.core.last_scrolled_forward()
    }

    fn last_scrolled_backward(&self) -> bool {
        self.inner.core.last_scrolled_backward()
    }
}

impl ComposeArg for DefaultScrollableState {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl PartialEq for DefaultScrollableState {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl std::fmt::Debug for DefaultScrollableState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("DefaultScrollableState")
    }
}

/// What a scroll does when the pointer lets go while moving, as Compose's `FlingBehavior`:
/// `perform_fling` scrolls with `scope` from `initial_velocity` (pixels per second, towards the
/// end) and returns the velocity it didn't use (where an edge stopped it), which the nested
/// scroll parents get.
pub trait FlingBehavior {
    fn perform_fling<'a>(
        &'a self,
        scope: ScrollScope,
        initial_velocity: f32,
    ) -> LocalBoxFuture<'a, f32>;
}

/// Compose's default fling on Android: the content glides along Android's spline
/// (`OverScroller`'s, Compose's `splineBasedDecay`), a step a frame, until its distance is
/// covered or an edge stops it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DefaultFlingBehavior {
    /// Pixels per dp: the spline's deceleration is in physical units.
    density: f32,
}

impl DefaultFlingBehavior {
    pub fn new(density: f32) -> DefaultFlingBehavior {
        DefaultFlingBehavior { density }
    }
}

impl FlingBehavior for DefaultFlingBehavior {
    fn perform_fling<'a>(
        &'a self,
        scope: ScrollScope,
        initial_velocity: f32,
    ) -> LocalBoxFuture<'a, f32> {
        let density = self.density;
        Box::pin(async move {
            // As Compose's: slower than a pixel a second doesn't move.
            if initial_velocity.abs() <= 1.0 {
                return initial_velocity;
            }
            let course = crate::fling::Fling::new(initial_velocity, density);
            let start = compose_runtime::with_frame_nanos(|time| time).await;
            let mut covered = 0.0f32;
            loop {
                let now = compose_runtime::with_frame_nanos(|time| time).await;
                let fraction = if course.duration_nanos == 0 {
                    1.0
                } else {
                    now.saturating_sub(start) as f32 / course.duration_nanos as f32
                };
                let target = course.distance_at(fraction);
                let delta = target - covered;
                covered = target;
                let used = if delta == 0.0 { 0.0 } else { scope.scroll_by(delta) };
                if fraction >= 1.0 {
                    return 0.0;
                }
                // An edge took less than asked: the fling ends there, with its velocity there
                // (the spline's slope), as Compose's `velocityLeft`.
                if (delta - used).abs() > 0.5 {
                    let step = 0.01f32.min(fraction);
                    let seconds = course.duration_nanos as f32 / 1e9;
                    return (course.distance_at(fraction) - course.distance_at(fraction - step))
                        / (step * seconds).max(f32::EPSILON);
                }
            }
        })
    }
}

/// Compose's `ScrollableDefaults`.
pub struct ScrollableDefaults;

impl ScrollableDefaults {
    /// The default fling, at the composition's density, as `ScrollableDefaults.flingBehavior()`
    /// (call it in composition): the same one for the same density.
    pub fn fling_behavior() -> Rc<dyn FlingBehavior> {
        thread_local! {
            static BEHAVIORS: RefCell<Vec<(u32, Rc<dyn FlingBehavior>)>> =
                const { RefCell::new(Vec::new()) };
        }
        let density = compose_ui::LOCAL_DENSITY.current().density;
        BEHAVIORS.with(|behaviors| {
            let mut behaviors = behaviors.borrow_mut();
            if let Some((_, behavior)) =
                behaviors.iter().find(|(bits, _)| *bits == density.to_bits())
            {
                return behavior.clone();
            }
            let behavior: Rc<dyn FlingBehavior> = Rc::new(DefaultFlingBehavior::new(density));
            behaviors.push((density.to_bits(), behavior.clone()));
            behavior
        })
    }

    /// Whether a scrollable reverses its deltas, as `ScrollableDefaults.reverseDirection`:
    /// content follows the finger (a drag up scrolls down), a horizontal scroll right to left
    /// the other way, and `reverse_scrolling` the other way again.
    pub fn reverse_direction(
        layout_direction: compose_ui::LayoutDirection,
        orientation: Orientation,
        reverse_scrolling: bool,
    ) -> bool {
        let mut reverse = !reverse_scrolling;
        if layout_direction == compose_ui::LayoutDirection::Rtl
            && orientation != Orientation::Vertical
        {
            reverse = !reverse;
        }
        reverse
    }
}

// ---- Drag sessions -------------------------------------------------------------------------

/// A drag going on, as Compose's `scroll(UserInput) { forEachDelta … }`: a task holding the
/// state's scroll for the drag (interrupting a fling or an animated scroll), whose scope the
/// drag's deltas go through, until the drag ends or a scroll of a higher priority interrupts it.
pub(crate) struct DragSession {
    job: Job,
    shared: Rc<SessionShared>,
}

#[derive(Default)]
struct SessionShared {
    scope: RefCell<Option<ScrollScope>>,
    done: Cell<bool>,
    waker: RefCell<Option<Waker>>,
}

impl DragSession {
    /// Starts a drag's scroll on `state`, at once (the task is polled now: the scroll going on
    /// is interrupted before the drag's first delta).
    pub(crate) fn start(state: Rc<dyn ScrollableState>) -> DragSession {
        let shared = Rc::new(SessionShared::default());
        let held = shared.clone();
        let job = CoroutineScope::new().launch(async move {
            let _ = state
                .scroll(MutatePriority::UserInput, |scope| async move {
                    *held.scope.borrow_mut() = Some(scope);
                    let _clear = ClearScope(&held);
                    UntilDone(&held).await;
                })
                .await;
        });
        compose_runtime::platform::poll_now(job);
        DragSession { job, shared }
    }

    /// The scope the drag scrolls with, while it holds the state's scroll.
    pub(crate) fn scope(&self) -> Option<ScrollScope> {
        self.shared.scope.borrow().clone()
    }

    /// The drag ended: the scroll ends now (its task polled at once), so what follows (a
    /// fling's scroll) doesn't depend on the order the executor runs tasks in.
    pub(crate) fn end(self) {
        self.shared.done.set(true);
        let waker = self.shared.waker.borrow_mut().take();
        match waker {
            Some(waker) => {
                waker.wake();
                compose_runtime::platform::poll_now(self.job);
            }
            None => self.job.cancel(),
        }
    }
}

/// Forgets a session's scope when its block ends however it ends (interrupted too).
struct ClearScope<'a>(&'a SessionShared);

impl Drop for ClearScope<'_> {
    fn drop(&mut self) {
        self.0.scope.borrow_mut().take();
    }
}

/// Waits until the drag ends.
struct UntilDone<'a>(&'a SessionShared);

impl Future for UntilDone<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        if self.0.done.get() {
            return Poll::Ready(());
        }
        *self.0.waker.borrow_mut() = Some(context.waker().clone());
        Poll::Pending
    }
}

// ---- Modifier.scrollable --------------------------------------------------------------------

/// `Modifier.scrollable`, on `Modifier`.
pub trait ScrollableModifier {
    /// Drags and the wheel along `orientation` scroll `state`, and a finger or a stylus let go
    /// flings it, as `Modifier.scrollable(state, orientation)`: the content isn't moved (the
    /// state's consumer does what it wants with the deltas). A mouse doesn't drag it, its wheel
    /// scrolls it. The deltas are the pointer's: a drag down or to the right gives positive ones,
    /// as Compose's `scrollable` (`vertical_scroll` negates them, as
    /// `ScrollableDefaults.reverseDirection`, so that the content follows the finger).
    fn scrollable<S>(self, state: S, orientation: Orientation) -> Modifier
    where
        S: ScrollableState + ComposeArg + Clone + 'static;

    /// The same with Compose's optional parameters, as
    /// `Modifier.scrollable(state, orientation, enabled, reverseDirection, flingBehavior,
    /// interactionSource)`: `enabled = false` ignores input, `reverse_direction` negates the
    /// deltas, `fling_behavior` (`None`: the default) is what a release does, and the drags are
    /// reported to `interaction_source`.
    fn scrollable_with<S>(
        self,
        state: S,
        orientation: Orientation,
        enabled: bool,
        reverse_direction: bool,
        fling_behavior: Option<Rc<dyn FlingBehavior>>,
        interaction_source: Option<MutableInteractionSource>,
    ) -> Modifier
    where
        S: ScrollableState + ComposeArg + Clone + 'static;
}

impl ScrollableModifier for Modifier {
    fn scrollable<S>(self, state: S, orientation: Orientation) -> Modifier
    where
        S: ScrollableState + ComposeArg + Clone + 'static,
    {
        self.scrollable_with(state, orientation, true, false, None, None)
    }

    fn scrollable_with<S>(
        self,
        state: S,
        orientation: Orientation,
        enabled: bool,
        reverse_direction: bool,
        fling_behavior: Option<Rc<dyn FlingBehavior>>,
        interaction_source: Option<MutableInteractionSource>,
    ) -> Modifier
    where
        S: ScrollableState + ComposeArg + Clone + 'static,
    {
        self.then_custom(ScrollableElement {
            dyn_state: Rc::new(state.clone()),
            state,
            orientation,
            enabled,
            reverse_direction,
            fling_behavior,
            interaction_source,
        })
    }
}

struct ScrollableElement<S> {
    state: S,
    dyn_state: Rc<dyn ScrollableState>,
    orientation: Orientation,
    enabled: bool,
    reverse_direction: bool,
    fling_behavior: Option<Rc<dyn FlingBehavior>>,
    interaction_source: Option<MutableInteractionSource>,
}

struct ScrollableNode {
    /// The state as given, to compare with the next one.
    state: Box<dyn std::any::Any>,
    dyn_state: Rc<dyn ScrollableState>,
    vertical: bool,
    enabled: bool,
    reverse_direction: bool,
    fling_behavior: Option<Rc<dyn FlingBehavior>>,
    interaction_source: Option<MutableInteractionSource>,
    drag: crate::scroll::DragScroller,
}

impl<S: ScrollableState + ComposeArg + Clone + 'static> ModifierNodeElement
    for ScrollableElement<S>
{
    type Node = ScrollableNode;

    fn create(&self) -> ScrollableNode {
        ScrollableNode {
            state: Box::new(self.state.clone()),
            dyn_state: self.dyn_state.clone(),
            vertical: self.orientation == Orientation::Vertical,
            enabled: self.enabled,
            reverse_direction: self.reverse_direction,
            fling_behavior: self.fling_behavior.clone(),
            interaction_source: self.interaction_source.clone(),
            drag: crate::scroll::DragScroller::default(),
        }
    }

    fn update(&self, node: &mut ScrollableNode) {
        let same_state = node.state.downcast_ref::<S>().is_some_and(|old| old.same(&self.state));
        let vertical = self.orientation == Orientation::Vertical;
        // A new state, axis or direction starts the gesture handling over, as Compose's
        // `resetPointerInputHandling`; disabled, a drag going on is cancelled.
        if !same_state
            || node.vertical != vertical
            || node.reverse_direction != self.reverse_direction
            || (node.enabled && !self.enabled)
        {
            node.drag.cancel();
            if !same_state {
                node.drag.stop_fling();
            }
        }
        node.state = Box::new(self.state.clone());
        node.dyn_state = self.dyn_state.clone();
        node.vertical = vertical;
        node.enabled = self.enabled;
        node.reverse_direction = self.reverse_direction;
        node.fling_behavior = self.fling_behavior.clone();
        node.interaction_source = self.interaction_source.clone();
    }

    fn same(&self, other: &Self) -> bool {
        other.state.same(&self.state)
            && other.orientation == self.orientation
            && other.enabled == self.enabled
            && other.reverse_direction == self.reverse_direction
            && same_fling(&other.fling_behavior, &self.fling_behavior)
            && other.interaction_source == self.interaction_source
    }

    fn name(&self) -> &'static str {
        "scrollable"
    }
}

impl ModifierNode for ScrollableNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }

    fn as_semantics(&mut self) -> Option<&mut dyn compose_ui::SemanticsModifierNode> {
        Some(self)
    }

    fn on_release(&mut self, _context: &mut compose_ui::ReleaseContext<'_>) {
        self.drag.cancel();
        self.drag.stop_fling();
    }

    fn on_reset(&mut self, _context: &mut compose_ui::NodeContext<'_>) {
        self.drag.cancel();
        self.drag.stop_fling();
    }
}

impl compose_ui::SemanticsModifierNode for ScrollableNode {
    /// Scrolls by the action's delta along the axis, as Compose's scrollable's `scrollBy`
    /// action (compose-rs scrolls at once, D491).
    fn apply_semantics(&self, config: &mut compose_ui::SemanticsConfiguration) {
        if !self.enabled {
            return;
        }
        let (state, vertical) = (self.dyn_state.clone(), self.vertical);
        config.scroll_by(None, move |x, y| {
            // The state's own way, as Compose's `semanticsScrollBy` (its two reversals cancel);
            // handled even where nothing moves (an edge), as Compose's action returns true
            // (D877).
            let delta = if vertical { y } else { x };
            if delta != 0.0 {
                state.dispatch_raw_delta(delta);
            }
            true
        });
    }
}

impl PointerInputModifierNode for ScrollableNode {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        _bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        if !self.enabled {
            return;
        }
        let state = self.dyn_state.clone();
        let interaction_source = self.interaction_source.clone();
        let driver = crate::scroll::ScrollDriver {
            state: &self.dyn_state,
            fling_behavior: self.fling_behavior.as_ref(),
            interaction_source: interaction_source.as_ref(),
            state_source: false,
            set_outer: None,
        };
        self.drag.on_pointer_event(
            event,
            pass,
            self.vertical,
            // A drag's deltas are the pointer's (a drag down gives positive ones), negated if
            // `reverse_direction`, as Compose's `scrollable`; the scroller's are the content's.
            !self.reverse_direction,
            context,
            &driver,
            // As the nested-scroll parent of the scrollables inside: what they leave scrolls it,
            // unless a scroll of its own goes on, as Compose's `performRawScroll`.
            move |_: &NestedScrollChain, _: NestedScrollSource| {
                let state = state.clone();
                Rc::new(move |delta| {
                    if state.is_scroll_in_progress() {
                        0.0
                    } else {
                        state.dispatch_raw_delta(delta)
                    }
                })
            },
        );
    }

    fn on_cancel_pointer_input(&mut self) {
        self.drag.cancel();
    }
}
