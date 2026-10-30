//! `Modifier.pointer_input`, as Compose's: a coroutine handling the pointer events that reach
//! its node, with Compose's gesture detectors on top (`detect_tap_gestures`,
//! `detect_drag_gestures`).
//!
//! The coroutine is a task of the UI thread's executor, polled at once as each event reaches the
//! node in each pass (as Compose resumes it undispatched), so what it consumes the handlers
//! after it see consumed. Its timeouts (a long press, a double tap) are executor timers. It
//! starts at the first event, and starts over when its key changes, when its gesture is
//! cancelled, and when the node leaves; a block that returns stays done.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use std::time::Duration;

use compose_runtime::{ComposeArg, CoroutineScope, Job};

use crate::input::{EventContext, PointerEventPass, PointerId, PointerType};
use crate::modifier::{Modifier, ModifierNode, ModifierNodeElement, PointerInputModifierNode};
use crate::pointer_event::{PointerEvents, PointerInputChange};
use crate::unit::{Density, IntSize, Offset, Size};

/// The event being delivered, while the block's task is polled.
struct Delivery {
    event: PointerEvents,
    pass: PointerEventPass,
}

#[derive(Default)]
struct ScopeInner {
    delivery: RefCell<Option<Delivery>>,
    size: Cell<IntSize>,
    density: Cell<Density>,
    /// The host's view configuration, as the last event found it (D907).
    view_configuration: Cell<crate::view_configuration::ViewConfiguration>,
    /// The pointers down, as the events reaching the node tell.
    pressed: RefCell<Vec<PointerId>>,
    /// Callbacks the detectors call, run once the dispatch is over (the tree is free).
    deferred: RefCell<Vec<Box<dyn FnOnce()>>>,
    /// An event is being dispatched to the block.
    dispatching: Cell<bool>,
    /// The last event to reach the node, as `currentEvent`.
    current_event: RefCell<Option<PointerEvents>>,
}

/// What a `pointer_input` block works with, as Compose's `PointerInputScope` and
/// `AwaitPointerEventScope` in one. Cloning shares it.
#[derive(Clone)]
pub struct PointerInputScope {
    inner: Rc<ScopeInner>,
}

impl PointerInputScope {
    /// The node's size, as `size`.
    pub fn size(&self) -> IntSize {
        self.inner.size.get()
    }

    pub fn density(&self) -> Density {
        self.inner.density.get()
    }

    /// The host's gesture timeouts and distances, as `viewConfiguration` (D907).
    pub fn view_configuration(&self) -> crate::view_configuration::ViewConfiguration {
        self.inner.view_configuration.get()
    }

    /// The distance a pointer moves before it's a drag, as `viewConfiguration.touchSlop` (8 dp
    /// by default).
    pub fn touch_slop(&self) -> f32 {
        self.view_configuration().touch_slop
    }

    /// Whether any pointer is down, as the events reaching the node tell.
    pub fn any_pressed(&self) -> bool {
        !self.inner.pressed.borrow().is_empty()
    }

    /// The next event reaching the node in `pass`, as `awaitPointerEvent(pass)`.
    pub fn await_pointer_event(&self, pass: PointerEventPass) -> AwaitPointerEvent {
        AwaitPointerEvent { inner: self.inner.clone(), pass }
    }

    /// Runs `callback` once the event's dispatch is over (at once when a timer resumed the
    /// block): what the detectors call their callbacks with, so they can use the UI freely.
    pub fn run_after_dispatch(&self, callback: impl FnOnce() + 'static) {
        if self.inner.dispatching.get() {
            self.inner.deferred.borrow_mut().push(Box::new(callback));
        } else {
            callback();
        }
    }

    /// `future`'s result, or `None` if it takes longer than `timeout`, as `withTimeoutOrNull`.
    pub async fn with_timeout<T>(
        &self,
        timeout: Duration,
        future: impl Future<Output = T>,
    ) -> Option<T> {
        Timeout { future: std::pin::pin!(future), delay: compose_runtime::delay(timeout) }.await
    }

    /// The last event to reach the node, as `currentEvent`.
    pub fn current_event(&self) -> Option<PointerEvents> {
        self.inner.current_event.borrow().clone()
    }

    /// How far past a node smaller than the minimum touch target a touch still counts as on it, as
    /// `extendedTouchPadding`.
    pub fn extended_touch_padding(&self) -> Size {
        let density = self.density();
        let minimum = self.view_configuration().minimum_touch_target_size;
        let size = self.size();
        Size::new(
            ((density.to_px(minimum.width) - size.width as f32) / 2.0).max(0.0),
            ((density.to_px(minimum.height) - size.height as f32) / 2.0).max(0.0),
        )
    }

    /// Whether the event just seen in the main pass was consumed by the time it reached the
    /// final pass (a parent took it), as the detectors' checks of `PointerEventPass.Final`.
    async fn consumed_by_final(&self) -> bool {
        let event = self.await_pointer_event(PointerEventPass::Final).await;
        event.changes.iter().any(|change| change.is_consumed())
    }

    /// The next time every pointer of an event comes down, as `awaitFirstDown`: unconsumed if
    /// `require_unconsumed`.
    pub async fn await_first_down(&self, require_unconsumed: bool) -> PointerInputChange {
        loop {
            let mut event = self.await_pointer_event(PointerEventPass::Main).await;
            let all = event.changes.iter().all(|change| {
                if require_unconsumed {
                    change.changed_to_down()
                } else {
                    change.changed_to_down_ignore_consumed()
                }
            });
            if all {
                return event.changes.swap_remove(0);
            }
        }
    }

    /// The first press to start a gesture of a detector: a mouse's only with its primary button
    /// (D1657).
    async fn await_gesture_down(&self, require_unconsumed: bool) -> PointerInputChange {
        loop {
            let down = self.await_first_down(require_unconsumed).await;
            let primary = self.inner.current_event.borrow().as_ref().is_none_or(|event| {
                event.buttons.is_primary_pressed() || !event.buttons.are_any_pressed()
            });
            if down.pointer_type != PointerType::Mouse || primary {
                return down;
            }
            self.await_all_up().await;
        }
    }

    /// The release of every pointer, or `None` if the press is cancelled (a handler consumed its
    /// events, in the main pass or a parent's by the final pass, or one moved out of the node), as
    /// `waitForUpOrCancellation`.
    pub async fn wait_for_up_or_cancellation(&self) -> Option<PointerInputChange> {
        loop {
            let mut event = self.await_pointer_event(PointerEventPass::Main).await;
            if event.changes.iter().all(|change| change.changed_to_up()) {
                return Some(event.changes.swap_remove(0));
            }
            let padding = self.extended_touch_padding();
            let out = event.changes.iter().any(|change| {
                change.is_consumed() || change.is_out_of_bounds_with(self.size(), padding)
            });
            if out || self.consumed_by_final().await {
                return None;
            }
        }
    }

    /// Waits until no pointer is down, as the end of `awaitEachGesture`'s gesture.
    async fn await_all_up(&self) {
        while self.any_pressed() {
            self.await_pointer_event(PointerEventPass::Final).await;
        }
    }

    /// Consumes events until no pointer is down.
    async fn consume_until_up(&self) {
        loop {
            let event = self.await_pointer_event(PointerEventPass::Main).await;
            event.changes.iter().for_each(|change| change.consume());
            if !self.any_pressed() {
                return;
            }
        }
    }

    /// A second press after `first_up`, not sooner than 40 ms after it, as `awaitSecondDown`.
    async fn await_second_down(&self, first_up: &PointerInputChange) -> PointerInputChange {
        loop {
            let down = self.await_gesture_down(true).await;
            if down.uptime_millis
                >= first_up.uptime_millis + self.view_configuration().double_tap_min_time_millis
            {
                return down;
            }
        }
    }

    /// Taps, double taps, long presses and presses, as Compose's `detectTapGestures`: a
    /// release inside within the long-press timeout (400 ms) taps; a second press within the
    /// double-tap timeout (300 ms, not sooner than 40 ms) double-taps (if `on_double_tap`, else
    /// each release taps); holding past the timeout long-presses (if `on_long_press`). The
    /// presses detected are consumed. Never returns.
    pub async fn detect_tap_gestures(&self, gestures: TapGestures) {
        self.detect_taps(gestures, &|_| {}).await
    }

    /// `detect_tap_gestures`, calling `pressed` at once (in the dispatch) when a press starts
    /// (true) and ends, released, cancelled or held to its end (false): what a clickable's
    /// indication shows.
    pub(crate) async fn detect_taps(&self, gestures: TapGestures, pressed: &dyn Fn(bool)) {
        loop {
            let down = self.await_gesture_down(true).await;
            down.consume();
            pressed(true);
            self.call(&gestures.on_press, down.position);
            let Some(up) = self.up_or_long_press(&gestures).await else {
                // Held: a long press.
                self.call(&gestures.on_long_press, down.position);
                self.consume_until_up().await;
                pressed(false);
                continue;
            };
            pressed(false);
            let Some(up) = up else {
                self.await_all_up().await;
                continue;
            };
            up.consume();
            if gestures.on_double_tap.is_none() {
                self.call(&gestures.on_tap, up.position);
                continue;
            }
            // A second press soon after: a double tap once it's released.
            let Some(second) = self
                .with_timeout(
                    Duration::from_millis(self.view_configuration().double_tap_timeout_millis),
                    self.await_second_down(&up),
                )
                .await
            else {
                self.call(&gestures.on_tap, up.position);
                continue;
            };
            second.consume();
            pressed(true);
            self.call(&gestures.on_press, second.position);
            match self.up_or_long_press(&gestures).await {
                // Released: the double tap.
                Some(Some(second_up)) => {
                    pressed(false);
                    second_up.consume();
                    self.call(&gestures.on_double_tap, second_up.position);
                }
                // Cancelled: the first tap alone.
                Some(None) => {
                    pressed(false);
                    self.call(&gestures.on_tap, up.position);
                    self.await_all_up().await;
                }
                // Held: the first tap, then a long press.
                None => {
                    self.call(&gestures.on_tap, up.position);
                    self.call(&gestures.on_long_press, second.position);
                    self.consume_until_up().await;
                    pressed(false);
                }
            }
        }
    }

    /// The press's end, or `None` if it's held past the long-press timeout (when there's a
    /// long press to detect).
    async fn up_or_long_press(&self, gestures: &TapGestures) -> Option<Option<PointerInputChange>> {
        if gestures.on_long_press.is_some() {
            self.with_timeout(
                Duration::from_millis(self.view_configuration().long_press_timeout_millis),
                self.wait_for_up_or_cancellation(),
            )
            .await
        } else {
            Some(self.wait_for_up_or_cancellation().await)
        }
    }

    /// Calls `callback` (if any) with `position` once the dispatch is over.
    fn call(&self, callback: &Option<Rc<dyn Fn(Offset)>>, position: Offset) {
        if let Some(callback) = callback.clone() {
            self.run_after_dispatch(move || callback(position));
        }
    }

    /// The move of `down`'s pointer past the touch slop (along `axis`, if any: x or y), or
    /// `None` if its press ends or another handler takes the moves first, as
    /// `awaitTouchSlopOrCancellation`. The move found is consumed; with it, the amount past the
    /// slop.
    pub async fn await_touch_slop_or_cancellation(
        &self,
        down: &PointerInputChange,
        axis: Option<Axis>,
    ) -> Option<(PointerInputChange, Offset)> {
        self.await_slop_or_cancellation(down, axis, self.touch_slop()).await
    }

    /// The same past `slop`, which Compose's detectors take from the press's pointer type
    /// (`pointerSlop`) and its public `awaitTouchSlopOrCancellation` from a finger's.
    async fn await_slop_or_cancellation(
        &self,
        down: &PointerInputChange,
        axis: Option<Axis>,
        slop: f32,
    ) -> Option<(PointerInputChange, Offset)> {
        let mut pointer = down.id;
        let mut moved = Offset::ZERO;
        loop {
            let event = self.await_pointer_event(PointerEventPass::Main).await;
            let change = event.changes.iter().find(|change| change.id == pointer)?;
            if change.is_consumed() {
                return None;
            }
            if change.changed_to_up_ignore_consumed() {
                // Another pointer down takes over; with none, it's over.
                pointer = event.changes.iter().find(|change| change.pressed)?.id;
                continue;
            }
            moved = moved + change.position_change_ignore_consumed();
            let distance = match axis {
                None => moved.x.hypot(moved.y),
                Some(Axis::X) => moved.x.abs(),
                Some(Axis::Y) => moved.y.abs(),
            };
            if distance > 0.0 && distance >= slop {
                change.consume();
                // What's past the slop, in the direction moved.
                let over = match axis {
                    None => {
                        let fraction = (distance - slop) / distance;
                        Offset::new(moved.x * fraction, moved.y * fraction)
                    }
                    Some(Axis::X) => Offset::new(moved.x - slop * moved.x.signum(), 0.0),
                    Some(Axis::Y) => Offset::new(0.0, moved.y - slop * moved.y.signum()),
                };
                return Some((change.clone(), over));
            }
            // A parent took the move meanwhile: not this gesture's.
            let after = self.await_pointer_event(PointerEventPass::Final).await;
            let taken = after.changes.iter().find(|change| change.id == pointer);
            if taken.is_none_or(|change| change.is_consumed()) {
                return None;
            }
        }
    }

    /// Drags, as Compose's `detectDragGestures`: a press that moves past the touch slop starts
    /// one (`on_drag_start` where it crossed), with its amount past the slop as the first
    /// `on_drag`, each move of its pointer then (consumed) calls `on_drag` with its amount, and
    /// the release ends it (`on_drag_end`), or a cancellation (`on_drag_cancel`, also when the
    /// gesture is cancelled from outside). Never returns.
    pub async fn detect_drag_gestures(&self, gestures: DragGestures) {
        loop {
            let down = self.await_gesture_down(false).await;
            let slop = self.view_configuration().pointer_slop(down.pointer_type);
            let Some((start, over)) = self.await_slop_or_cancellation(&down, None, slop).await
            else {
                continue;
            };
            if let Some(on_drag_start) = gestures.on_drag_start.clone() {
                let position = start.position;
                self.run_after_dispatch(move || on_drag_start(position));
            }
            // A cancellation from outside (the block dropped) calls `on_drag_cancel` too.
            let guard = DragGuard { on_cancel: gestures.on_drag_cancel.clone(), armed: true };
            let mut pointer = start.id;
            self.drag(&gestures, start, over);
            let ended = loop {
                let event = self.await_pointer_event(PointerEventPass::Main).await;
                let Some(change) = event.changes.iter().find(|change| change.id == pointer) else {
                    break false;
                };
                if change.changed_to_up_ignore_consumed() {
                    // Another pointer down takes over; with none, the drag's over.
                    match event.changes.iter().find(|change| change.pressed) {
                        Some(other) => pointer = other.id,
                        None => break !change.is_consumed(),
                    }
                } else if change.position_changed_ignore_consumed() {
                    if change.is_consumed() {
                        break false;
                    }
                    let amount = change.position_change();
                    change.consume();
                    self.drag(&gestures, change.clone(), amount);
                }
            };
            guard.disarm();
            if ended {
                if let Some(on_drag_end) = gestures.on_drag_end.clone() {
                    self.run_after_dispatch(move || on_drag_end());
                }
            } else {
                if let Some(on_drag_cancel) = gestures.on_drag_cancel.clone() {
                    self.run_after_dispatch(move || on_drag_cancel());
                }
                self.await_all_up().await;
            }
        }
    }

    fn drag(&self, gestures: &DragGestures, change: PointerInputChange, amount: Offset) {
        if let Some(on_drag) = gestures.on_drag.clone() {
            self.run_after_dispatch(move || on_drag(change, amount));
        }
    }
}

/// An axis a drag's slop is measured along.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Axis {
    X,
    Y,
}

/// Calls a drag's cancellation if the drag's future is dropped mid-drag (a cancelled gesture),
/// after the event's dispatch.
struct DragGuard {
    on_cancel: Option<Rc<dyn Fn()>>,
    armed: bool,
}

impl DragGuard {
    fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for DragGuard {
    fn drop(&mut self) {
        if self.armed
            && let Some(on_cancel) = self.on_cancel.take()
        {
            run_soon(move || on_cancel());
        }
    }
}

/// Runs `callback` as a task of its own: from a drop that may happen while composing or
/// dispatching, where user code can't run.
pub fn run_soon(callback: impl FnOnce() + 'static) {
    compose_runtime::platform::launch(async move { callback() });
}

/// `detect_tap_gestures`' callbacks, each given the position in the node.
#[derive(Clone, Default)]
pub struct TapGestures {
    pub on_double_tap: Option<Rc<dyn Fn(Offset)>>,
    pub on_long_press: Option<Rc<dyn Fn(Offset)>>,
    pub on_press: Option<Rc<dyn Fn(Offset)>>,
    pub on_tap: Option<Rc<dyn Fn(Offset)>>,
}

impl TapGestures {
    pub fn on_tap(mut self, on_tap: impl Fn(Offset) + 'static) -> Self {
        self.on_tap = Some(Rc::new(on_tap));
        self
    }
    pub fn on_double_tap(mut self, on_double_tap: impl Fn(Offset) + 'static) -> Self {
        self.on_double_tap = Some(Rc::new(on_double_tap));
        self
    }
    pub fn on_long_press(mut self, on_long_press: impl Fn(Offset) + 'static) -> Self {
        self.on_long_press = Some(Rc::new(on_long_press));
        self
    }
    pub fn on_press(mut self, on_press: impl Fn(Offset) + 'static) -> Self {
        self.on_press = Some(Rc::new(on_press));
        self
    }
}

/// `detect_drag_gestures`' callbacks.
#[derive(Clone, Default)]
pub struct DragGestures {
    pub on_drag_start: Option<Rc<dyn Fn(Offset)>>,
    pub on_drag_end: Option<Rc<dyn Fn()>>,
    pub on_drag_cancel: Option<Rc<dyn Fn()>>,
    pub on_drag: Option<Rc<dyn Fn(PointerInputChange, Offset)>>,
}

impl DragGestures {
    pub fn on_drag_start(mut self, on_drag_start: impl Fn(Offset) + 'static) -> Self {
        self.on_drag_start = Some(Rc::new(on_drag_start));
        self
    }
    pub fn on_drag_end(mut self, on_drag_end: impl Fn() + 'static) -> Self {
        self.on_drag_end = Some(Rc::new(on_drag_end));
        self
    }
    pub fn on_drag_cancel(mut self, on_drag_cancel: impl Fn() + 'static) -> Self {
        self.on_drag_cancel = Some(Rc::new(on_drag_cancel));
        self
    }
    pub fn on_drag(mut self, on_drag: impl Fn(PointerInputChange, Offset) + 'static) -> Self {
        self.on_drag = Some(Rc::new(on_drag));
        self
    }
}

/// `await_pointer_event`'s future.
pub struct AwaitPointerEvent {
    inner: Rc<ScopeInner>,
    pass: PointerEventPass,
}

impl Future for AwaitPointerEvent {
    type Output = PointerEvents;
    fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<PointerEvents> {
        let mut delivery = self.inner.delivery.borrow_mut();
        match delivery.take_if(|delivery| delivery.pass == self.pass) {
            Some(delivery) => Poll::Ready(delivery.event),
            // Woken by the next delivery's poll (the node polls the task itself).
            None => Poll::Pending,
        }
    }
}

/// `with_timeout`'s future: the first of the work and the delay.
struct Timeout<'a, F> {
    future: Pin<&'a mut F>,
    delay: compose_runtime::platform::Delay,
}

impl<F: Future> Future for Timeout<'_, F> {
    type Output = Option<F::Output>;
    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<F::Output>> {
        if let Poll::Ready(value) = self.future.as_mut().poll(context) {
            return Poll::Ready(Some(value));
        }
        match Pin::new(&mut self.delay).poll(context) {
            Poll::Ready(()) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

type Block = Rc<dyn Fn(PointerInputScope) -> Pin<Box<dyn Future<Output = ()>>>>;

/// Runs a `pointer_input` block over the events reaching a node: the part of the node that
/// custom pointer nodes (foundation's `draggable`) build on.
pub struct PointerInputDriver {
    block: Block,
    scope: PointerInputScope,
    coroutines: compose_runtime::Owned<CoroutineScope>,
    /// The block's task: `None` until the first event (and after a stop); a finished one
    /// stays, so a block that returned isn't run again.
    job: Option<Job>,
}

impl PointerInputDriver {
    pub fn new<F, Fut>(block: F) -> PointerInputDriver
    where
        F: Fn(PointerInputScope) -> Fut + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        PointerInputDriver::from_block(Rc::new(move |scope| Box::pin(block(scope))))
    }

    fn from_block(block: Block) -> PointerInputDriver {
        PointerInputDriver {
            block,
            scope: PointerInputScope { inner: Rc::new(ScopeInner::default()) },
            coroutines: CoroutineScope::new(),
            job: None,
        }
    }

    /// Cancels the block: it starts over at the next event.
    pub fn stop(&mut self) {
        if let Some(job) = self.job.take() {
            job.cancel();
        }
        self.scope.inner.pressed.borrow_mut().clear();
        self.scope.inner.current_event.replace(None);
    }

    /// Delivers `event`, in `pass`, to the block (starting it at the first event).
    pub fn on_pointer_events(
        &mut self,
        event: &PointerEvents,
        pass: PointerEventPass,
        bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        let inner = &self.scope.inner;
        inner.size.set(bounds);
        inner.density.set(context.density());
        inner.view_configuration.set(context.view_configuration());
        if pass == PointerEventPass::Initial {
            let mut pressed = inner.pressed.borrow_mut();
            for change in &event.changes {
                pressed.retain(|pointer| *pointer != change.id);
                if change.pressed {
                    pressed.push(change.id);
                }
            }
            inner.current_event.replace(Some(event.clone()));
        }
        inner.dispatching.set(true);
        // Started at the first event, polled at once to its first wait.
        let job = match self.job {
            Some(job) => job,
            None => {
                let job = self.coroutines.launch((self.block)(self.scope.clone()));
                compose_runtime::platform::poll_now(job);
                self.job = Some(job);
                job
            }
        };
        inner.delivery.replace(Some(Delivery { event: event.clone(), pass }));
        compose_runtime::platform::poll_now(job);
        inner.delivery.replace(None);
        inner.dispatching.set(false);
        for callback in inner.deferred.take() {
            context.run(callback);
        }
    }
}

impl Modifier {
    /// Handles the pointer events reaching the node with `block`, a coroutine, as
    /// `Modifier.pointerInput(key) { … }`: it starts at the first event and starts over when
    /// `key` changes.
    pub fn pointer_input<K, F, Fut>(self, key: K, block: F) -> Modifier
    where
        K: ComposeArg + Clone + 'static,
        F: Fn(PointerInputScope) -> Fut + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        let block: Block = Rc::new(move |scope| Box::pin(block(scope)));
        self.then_custom(PointerInputElement { key: Rc::new(key), block })
    }
}

struct PointerInputElement<K> {
    key: Rc<K>,
    block: Block,
}

struct PointerInputNode {
    key: Rc<dyn std::any::Any>,
    driver: PointerInputDriver,
}

impl<K: ComposeArg + Clone + 'static> ModifierNodeElement for PointerInputElement<K> {
    type Node = PointerInputNode;
    fn create(&self) -> PointerInputNode {
        PointerInputNode {
            key: self.key.clone(),
            driver: PointerInputDriver::from_block(self.block.clone()),
        }
    }
    fn update(&self, node: &mut PointerInputNode) {
        let same = node.key.downcast_ref::<K>().is_some_and(|key| key.same(&self.key));
        node.driver.block = self.block.clone();
        if !same {
            // A new key: the block starts over.
            node.key = self.key.clone();
            node.driver.stop();
        }
    }
    fn same(&self, other: &Self) -> bool {
        // The block isn't compared, as Compose's: only the key restarts it.
        other.key.same(&self.key) && Rc::ptr_eq(&other.block, &self.block)
    }
    fn name(&self) -> &'static str {
        "pointer_input"
    }
}

impl ModifierNode for PointerInputNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
    fn on_release(&mut self, _context: &mut crate::modifier::ReleaseContext<'_>) {
        self.driver.stop();
    }
    /// Reused, its block starts again at the next event, as Compose's
    /// `resetPointerInputHandler`.
    fn on_reset(&mut self, _context: &mut crate::node_context::NodeContext<'_>) {
        self.driver.stop();
    }
}

impl PointerInputModifierNode for PointerInputNode {
    fn takes_whole_events(&self) -> bool {
        true
    }

    fn on_pointer_events(
        &mut self,
        event: &PointerEvents,
        pass: PointerEventPass,
        bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        self.driver.on_pointer_events(event, pass, bounds, context);
    }

    /// The gesture is over without its events: the block starts over at the next one.
    fn on_cancel_pointer_input(&mut self) {
        self.driver.stop();
    }
}
