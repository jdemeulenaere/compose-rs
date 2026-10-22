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

use crate::input::{EventContext, PointerEvent, PointerEventPass, PointerEventType, PointerId};
use crate::modifier::{Modifier, ModifierNode, ModifierNodeElement, PointerInputModifierNode};
use crate::unit::{Density, IntSize, Offset};

/// One pointer event as a `pointer_input` block sees it, as Compose's `PointerInputChange`:
/// its pointer, its position in the node, when it happened, and whether a handler consumed it.
#[derive(Clone)]
pub struct PointerInputChange {
    pub kind: PointerEventType,
    pub id: PointerId,
    pub position: Offset,
    /// Where the pointer was at its previous event, in the node as it's laid out now (so the
    /// node's own moves in between don't count), as `previousPosition`.
    pub previous_position: Offset,
    /// Whether the pointer is down after this event.
    pub pressed: bool,
    pub scroll_delta: Offset,
    /// When it was delivered, by the executor's clock, as `uptimeMillis`.
    pub uptime_millis: u64,
    consumed: Rc<Cell<bool>>,
}

impl PointerInputChange {
    /// How far the pointer moved since its previous event, as `positionChange()`.
    pub fn position_change(&self) -> Offset {
        self.position - self.previous_position
    }

    /// Marks the event used: handlers after this one see it consumed, as `consume()`.
    pub fn consume(&self) {
        self.consumed.set(true);
    }

    pub fn is_consumed(&self) -> bool {
        self.consumed.get()
    }

    /// Whether it lies outside a node of `size`, as `isOutOfBounds`.
    pub fn is_out_of_bounds(&self, size: IntSize) -> bool {
        let position = self.position;
        position.x < 0.0
            || position.y < 0.0
            || position.x >= size.width as f32
            || position.y >= size.height as f32
    }
}

impl std::fmt::Debug for PointerInputChange {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?} of {:?} at {:?}", self.kind, self.id, self.position)
    }
}

/// The event being delivered, while the block's task is polled.
struct Delivery {
    event: PointerEvent,
    previous_position: Offset,
    pass: PointerEventPass,
    taken: bool,
    consumed: Rc<Cell<bool>>,
    uptime_millis: u64,
}

#[derive(Default)]
struct ScopeInner {
    delivery: RefCell<Option<Delivery>>,
    size: Cell<IntSize>,
    density: Cell<Density>,
    /// The pointers down, as the events reaching the node tell.
    pressed: RefCell<Vec<PointerId>>,
    /// Callbacks the detectors call, run once the dispatch is over (the tree is free).
    deferred: RefCell<Vec<Box<dyn FnOnce()>>>,
    /// An event is being dispatched to the block.
    dispatching: Cell<bool>,
    /// Each pointer's last position in the window.
    last_positions: RefCell<Vec<(PointerId, Offset)>>,
}

/// What a `pointer_input` block works with, as Compose's `PointerInputScope` and
/// `AwaitPointerEventScope` in one. Cloning shares it.
#[derive(Clone)]
pub struct PointerInputScope {
    inner: Rc<ScopeInner>,
}

/// As Compose's `ViewConfiguration.longPressTimeoutMillis`.
const LONG_PRESS_TIMEOUT: Duration = Duration::from_millis(400);
/// As `doubleTapTimeoutMillis`.
const DOUBLE_TAP_TIMEOUT: Duration = Duration::from_millis(300);
/// As `doubleTapMinTimeMillis`: a second press sooner than this isn't a double tap.
const DOUBLE_TAP_MIN_MILLIS: u64 = 40;

impl PointerInputScope {
    /// The node's size, as `size`.
    pub fn size(&self) -> IntSize {
        self.inner.size.get()
    }

    pub fn density(&self) -> Density {
        self.inner.density.get()
    }

    /// The distance a pointer moves before it's a drag, as `viewConfiguration.touchSlop` (8 dp).
    pub fn touch_slop(&self) -> f32 {
        self.density().to_px(crate::unit::Dp(8.0))
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

    /// Whether the event just seen in the main pass was consumed by the time it reached the
    /// final pass (a parent took it), as the detectors' checks of `PointerEventPass.Final`.
    async fn consumed_by_final(&self) -> bool {
        self.await_pointer_event(PointerEventPass::Final).await.is_consumed()
    }

    /// The next press, as `awaitFirstDown`: unconsumed if `require_unconsumed`.
    pub async fn await_first_down(&self, require_unconsumed: bool) -> PointerInputChange {
        loop {
            let change = self.await_pointer_event(PointerEventPass::Main).await;
            if change.kind == PointerEventType::Press
                && (!require_unconsumed || !change.is_consumed())
            {
                return change;
            }
        }
    }

    /// The release that leaves no pointer down, or `None` if the press is cancelled (a
    /// handler consumed its events, in the main pass or a parent's by the final pass, or it
    /// moved out of the node), as `waitForUpOrCancellation`.
    pub async fn wait_for_up_or_cancellation(&self) -> Option<PointerInputChange> {
        loop {
            let change = self.await_pointer_event(PointerEventPass::Main).await;
            match change.kind {
                PointerEventType::Release if !self.any_pressed() => {
                    return if change.is_consumed() { None } else { Some(change) };
                }
                PointerEventType::Release | PointerEventType::Move => {
                    if change.is_consumed() || change.is_out_of_bounds(self.size()) {
                        return None;
                    }
                    if self.consumed_by_final().await {
                        return None;
                    }
                }
                _ => {}
            }
        }
    }

    /// Waits until no pointer is down, as the end of `awaitEachGesture`'s gesture.
    async fn await_all_up(&self) {
        while self.any_pressed() {
            self.await_pointer_event(PointerEventPass::Main).await;
        }
    }

    /// Consumes events until no pointer is down.
    async fn consume_until_up(&self) {
        while self.any_pressed() {
            self.await_pointer_event(PointerEventPass::Main).await.consume();
        }
    }

    /// A second press after `first_up`, not sooner than 40 ms after it, as `awaitSecondDown`.
    async fn await_second_down(&self, first_up: &PointerInputChange) -> PointerInputChange {
        loop {
            let down = self.await_first_down(true).await;
            if down.uptime_millis >= first_up.uptime_millis + DOUBLE_TAP_MIN_MILLIS {
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
            let down = self.await_first_down(true).await;
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
            let Some(second) =
                self.with_timeout(DOUBLE_TAP_TIMEOUT, self.await_second_down(&up)).await
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
            self.with_timeout(LONG_PRESS_TIMEOUT, self.wait_for_up_or_cancellation()).await
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
        let slop = self.touch_slop();
        loop {
            let change = self.await_pointer_event(PointerEventPass::Main).await;
            if change.id != down.id {
                continue;
            }
            match change.kind {
                PointerEventType::Release => return None,
                PointerEventType::Move if change.is_consumed() => return None,
                PointerEventType::Move => {
                    let moved = change.position - down.position;
                    let distance = match axis {
                        None => moved.x.hypot(moved.y),
                        Some(Axis::X) => moved.x.abs(),
                        Some(Axis::Y) => moved.y.abs(),
                    };
                    if distance > slop {
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
                        return Some((change, over));
                    }
                    // A parent took the move meanwhile: not this gesture's.
                    if self.consumed_by_final().await {
                        return None;
                    }
                }
                _ => {}
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
            let down = self.await_first_down(false).await;
            let Some((start, over)) = self.await_touch_slop_or_cancellation(&down, None).await
            else {
                continue;
            };
            if let Some(on_drag_start) = gestures.on_drag_start.clone() {
                let position = start.position;
                self.run_after_dispatch(move || on_drag_start(position));
            }
            // A cancellation from outside (the block dropped) calls `on_drag_cancel` too.
            let guard = DragGuard { on_cancel: gestures.on_drag_cancel.clone(), armed: true };
            self.drag(&gestures, start.clone(), over);
            let mut last = start.position;
            let ended = loop {
                let change = self.await_pointer_event(PointerEventPass::Main).await;
                if change.id != down.id {
                    continue;
                }
                match change.kind {
                    PointerEventType::Move if !change.is_consumed() => {
                        change.consume();
                        let amount = change.position - last;
                        last = change.position;
                        self.drag(&gestures, change, amount);
                    }
                    PointerEventType::Release => break !change.is_consumed(),
                    PointerEventType::Move => break false,
                    _ => {}
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
    CoroutineScope::new().launch(async move { callback() });
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
    type Output = PointerInputChange;
    fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<PointerInputChange> {
        let mut delivery = self.inner.delivery.borrow_mut();
        match delivery.as_mut() {
            Some(delivery) if !delivery.taken && delivery.pass == self.pass => {
                delivery.taken = true;
                let event = delivery.event;
                Poll::Ready(PointerInputChange {
                    kind: event.kind,
                    id: event.pointer,
                    position: event.position,
                    previous_position: delivery.previous_position,
                    pressed: event.pressed,
                    scroll_delta: event.scroll_delta,
                    uptime_millis: delivery.uptime_millis,
                    consumed: delivery.consumed.clone(),
                })
            }
            // Woken by the next delivery's poll (the node polls the task itself).
            _ => Poll::Pending,
        }
    }
}

/// `with_timeout`'s future: the first of the work and the delay.
struct Timeout<'a, F> {
    future: Pin<&'a mut F>,
    delay: compose_runtime::executor::Delay,
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
        self.scope.inner.last_positions.borrow_mut().clear();
    }

    /// Delivers `event`, in `pass`, to the block (starting it at the first event).
    pub fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        let inner = &self.scope.inner;
        inner.size.set(bounds);
        inner.density.set(context.density());
        if pass == PointerEventPass::Initial {
            let mut pressed = inner.pressed.borrow_mut();
            match event.kind {
                PointerEventType::Press if !pressed.contains(&event.pointer) => {
                    pressed.push(event.pointer)
                }
                PointerEventType::Release => pressed.retain(|pointer| *pointer != event.pointer),
                _ => {}
            }
        }
        inner.dispatching.set(true);
        // Started at the first event, polled at once to its first wait.
        let job = match self.job {
            Some(job) => job,
            None => {
                let job = self.coroutines.launch((self.block)(self.scope.clone()));
                compose_runtime::executor::poll_now(job);
                self.job = Some(job);
                job
            }
        };
        let consumed = Rc::new(Cell::new(event.consumed));
        // The previous position where the node is now: the window's positions' difference.
        let in_window = context.pointer_position().unwrap_or(event.position);
        let previous_position = {
            let mut last = inner.last_positions.borrow_mut();
            let found =
                last.iter().find(|(pointer, _)| *pointer == event.pointer).map(|&(_, at)| at);
            if pass == PointerEventPass::Final {
                last.retain(|(pointer, _)| *pointer != event.pointer);
                if event.kind != PointerEventType::Release {
                    last.push((event.pointer, in_window));
                }
            }
            event.position - (in_window - found.unwrap_or(in_window))
        };
        inner.delivery.replace(Some(Delivery {
            event: *event,
            previous_position,
            pass,
            taken: false,
            consumed: consumed.clone(),
            uptime_millis: if event.time_nanos != 0 {
                event.time_nanos / 1_000_000
            } else {
                compose_runtime::executor::now_nanos() / 1_000_000
            },
        }));
        compose_runtime::executor::poll_now(job);
        inner.delivery.replace(None);
        inner.dispatching.set(false);
        if consumed.get() {
            event.consume();
        }
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
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(PointerInputNode {
            key: self.key.clone(),
            driver: PointerInputDriver::from_block(self.block.clone()),
        })
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<PointerInputNode>() {
            let same = node.key.downcast_ref::<K>().is_some_and(|key| key.same(&self.key));
            node.driver.block = self.block.clone();
            if !same {
                // A new key: the block starts over.
                node.key = self.key.clone();
                node.driver.stop();
            }
        }
    }
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        // The block isn't compared, as Compose's: only the key restarts it.
        other
            .downcast_ref::<PointerInputElement<K>>()
            .is_some_and(|other| other.key.same(&self.key) && Rc::ptr_eq(&other.block, &self.block))
    }
    fn name(&self) -> &'static str {
        "pointer_input"
    }
}

impl ModifierNode for PointerInputNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
    fn on_detach(&mut self, _context: &mut crate::modifier::DetachContext<'_>) {
        self.driver.stop();
    }
}

impl PointerInputModifierNode for PointerInputNode {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        self.driver.on_pointer_event(event, pass, bounds, context);
    }

    /// The gesture is over without its events: the block starts over at the next one.
    fn on_cancel_pointer_input(&mut self) {
        self.driver.stop();
    }
}
