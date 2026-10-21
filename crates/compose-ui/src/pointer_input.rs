//! `Modifier.pointer_input`, as Compose's: a coroutine handling the pointer events that reach
//! its node, with Compose's gesture detectors on top (`detect_tap_gestures`,
//! `detect_drag_gestures`).
//!
//! The coroutine is a task of the UI thread's executor, polled at once as each event reaches the
//! node (as Compose resumes it undispatched), so what it consumes the handlers after it see
//! consumed. Its timeouts (a long press, a double tap) are executor timers. It starts at the
//! first event, and starts over when its key changes, when its gesture is cancelled, and when
//! the node leaves.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use std::time::Duration;

use compose_runtime::{ComposeArg, CoroutineScope, Job};

use crate::input::{EventContext, PointerEvent, PointerEventPass, PointerEventType};
use crate::modifier::{Modifier, ModifierNode, ModifierNodeElement, PointerInputModifierNode};
use crate::unit::{Density, IntSize, Offset};

/// One pointer event as a `pointer_input` block sees it, as Compose's `PointerInputChange`:
/// its position in the node, and whether a handler consumed it.
#[derive(Clone)]
pub struct PointerInputChange {
    pub kind: PointerEventType,
    pub position: Offset,
    /// Whether the pointer is down after this event.
    pub pressed: bool,
    pub scroll_delta: Offset,
    consumed: Rc<Cell<bool>>,
}

impl PointerInputChange {
    /// Marks the event used: handlers after this one see it consumed, as `consume()`.
    pub fn consume(&self) {
        self.consumed.set(true);
    }

    pub fn is_consumed(&self) -> bool {
        self.consumed.get()
    }
}

impl std::fmt::Debug for PointerInputChange {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?} at {:?}", self.kind, self.position)
    }
}

/// The event being delivered, while the block's task is polled.
struct Delivery {
    event: PointerEvent,
    pass: PointerEventPass,
    taken: bool,
    consumed: Rc<Cell<bool>>,
}

#[derive(Default)]
struct ScopeInner {
    delivery: RefCell<Option<Delivery>>,
    size: Cell<IntSize>,
    density: Cell<Density>,
    /// Callbacks the detectors call, run once the dispatch is over (the tree is free).
    deferred: RefCell<Vec<Box<dyn FnOnce()>>>,
    /// An event is being dispatched to the block.
    dispatching: Cell<bool>,
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

    /// The distance a pointer moves before it's a drag, as `viewConfiguration.touchSlop` (8 dp).
    pub fn touch_slop(&self) -> f32 {
        self.density().to_px(crate::unit::Dp(8.0))
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

    /// The release ending the press, or `None` if it's cancelled (a handler consumed it, or
    /// it moved out of the node), as `waitForUpOrCancellation`.
    pub async fn wait_for_up_or_cancellation(&self) -> Option<PointerInputChange> {
        loop {
            let change = self.await_pointer_event(PointerEventPass::Main).await;
            match change.kind {
                PointerEventType::Release => {
                    return if change.is_consumed() { None } else { Some(change) };
                }
                PointerEventType::Move => {
                    let size = self.size();
                    let position = change.position;
                    let outside = position.x < 0.0
                        || position.y < 0.0
                        || position.x >= size.width as f32
                        || position.y >= size.height as f32;
                    if change.is_consumed() || outside {
                        return None;
                    }
                }
                _ => {}
            }
        }
    }

    /// Waits until no button is held, as the end of `awaitEachGesture`'s gesture.
    async fn await_all_up(&self) {
        loop {
            let change = self.await_pointer_event(PointerEventPass::Main).await;
            if change.kind == PointerEventType::Release || !change.pressed {
                return;
            }
        }
    }

    /// Taps, double taps, long presses and presses, as Compose's `detectTapGestures`: a
    /// release inside within the long-press timeout (400 ms) taps; a second press within the
    /// double-tap timeout (300 ms) double-taps (if `on_double_tap`, else each release taps);
    /// holding past the timeout long-presses (if `on_long_press`). The presses detected are
    /// consumed. Never returns.
    pub async fn detect_tap_gestures(&self, gestures: TapGestures) {
        loop {
            let down = self.await_first_down(true).await;
            down.consume();
            if let Some(on_press) = gestures.on_press.clone() {
                let position = down.position;
                self.run_after_dispatch(move || on_press(position));
            }
            let up = match &gestures.on_long_press {
                Some(on_long_press) => {
                    match self
                        .with_timeout(LONG_PRESS_TIMEOUT, self.wait_for_up_or_cancellation())
                        .await
                    {
                        Some(up) => up,
                        None => {
                            let (on_long_press, position) = (on_long_press.clone(), down.position);
                            self.run_after_dispatch(move || on_long_press(position));
                            self.consume_until_up().await;
                            continue;
                        }
                    }
                }
                None => self.wait_for_up_or_cancellation().await,
            };
            let Some(up) = up else { continue };
            up.consume();
            let Some(on_double_tap) = gestures.on_double_tap.clone() else {
                if let Some(on_tap) = gestures.on_tap.clone() {
                    let position = up.position;
                    self.run_after_dispatch(move || on_tap(position));
                }
                continue;
            };
            // A second press soon after: a double tap once it's released.
            let second = self.with_timeout(DOUBLE_TAP_TIMEOUT, self.await_first_down(true)).await;
            match second {
                None => {
                    if let Some(on_tap) = gestures.on_tap.clone() {
                        let position = up.position;
                        self.run_after_dispatch(move || on_tap(position));
                    }
                }
                Some(second) => {
                    second.consume();
                    if let Some(on_press) = gestures.on_press.clone() {
                        let position = second.position;
                        self.run_after_dispatch(move || on_press(position));
                    }
                    if let Some(second_up) = self.wait_for_up_or_cancellation().await {
                        second_up.consume();
                        let position = second_up.position;
                        self.run_after_dispatch(move || on_double_tap(position));
                    }
                }
            }
        }
    }

    /// Consumes events until the press ends.
    async fn consume_until_up(&self) {
        loop {
            let change = self.await_pointer_event(PointerEventPass::Main).await;
            change.consume();
            if change.kind == PointerEventType::Release {
                return;
            }
        }
    }

    /// Drags, as Compose's `detectDragGestures`: a press that moves past the touch slop starts
    /// one (`on_drag_start` where it crossed), each move then (consumed) calls `on_drag` with its
    /// amount, and the release ends it (`on_drag_end`), or a cancellation (`on_drag_cancel`).
    /// Never returns.
    pub async fn detect_drag_gestures(&self, gestures: DragGestures) {
        loop {
            let down = self.await_first_down(false).await;
            // Past the touch slop, or cancelled.
            let mut last = down.position;
            let start = loop {
                let change = self.await_pointer_event(PointerEventPass::Main).await;
                match change.kind {
                    PointerEventType::Move if !change.is_consumed() => {
                        let moved = change.position - down.position;
                        if moved.x.hypot(moved.y) > self.touch_slop() {
                            change.consume();
                            break Some(change);
                        }
                    }
                    PointerEventType::Release => break None,
                    PointerEventType::Move => break None,
                    _ => {}
                }
            };
            let Some(start) = start else { continue };
            if let Some(on_drag_start) = gestures.on_drag_start.clone() {
                let position = start.position;
                self.run_after_dispatch(move || on_drag_start(position));
            }
            let amount = start.position - last;
            last = start.position;
            if let Some(on_drag) = gestures.on_drag.clone() {
                let change = start.clone();
                self.run_after_dispatch(move || on_drag(change, amount));
            }
            loop {
                let change = self.await_pointer_event(PointerEventPass::Main).await;
                match change.kind {
                    PointerEventType::Move if !change.is_consumed() => {
                        change.consume();
                        let amount = change.position - last;
                        last = change.position;
                        if let Some(on_drag) = gestures.on_drag.clone() {
                            self.run_after_dispatch(move || on_drag(change, amount));
                        }
                    }
                    PointerEventType::Release => {
                        if let Some(on_drag_end) = gestures.on_drag_end.clone() {
                            self.run_after_dispatch(move || on_drag_end());
                        }
                        break;
                    }
                    PointerEventType::Move => {
                        if let Some(on_drag_cancel) = gestures.on_drag_cancel.clone() {
                            self.run_after_dispatch(move || on_drag_cancel());
                        }
                        self.await_all_up().await;
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
}

/// As Compose's `ViewConfiguration.longPressTimeoutMillis`.
const LONG_PRESS_TIMEOUT: Duration = Duration::from_millis(400);
/// As `doubleTapTimeoutMillis`.
const DOUBLE_TAP_TIMEOUT: Duration = Duration::from_millis(300);

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
                    position: event.position,
                    pressed: event.pressed,
                    scroll_delta: event.scroll_delta,
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
    block: Block,
    scope: PointerInputScope,
    coroutines: CoroutineScope,
    job: Option<Job>,
}

impl<K: ComposeArg + Clone + 'static> ModifierNodeElement for PointerInputElement<K> {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(PointerInputNode {
            key: self.key.clone(),
            block: self.block.clone(),
            scope: PointerInputScope { inner: Rc::new(ScopeInner::default()) },
            coroutines: CoroutineScope::new(),
            job: None,
        })
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<PointerInputNode>() {
            let same = node.key.downcast_ref::<K>().is_some_and(|key| key.same(&self.key));
            node.block = self.block.clone();
            if !same {
                // A new key: the block starts over.
                node.key = self.key.clone();
                node.stop();
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

impl PointerInputNode {
    fn stop(&mut self) {
        if let Some(job) = self.job.take() {
            job.cancel();
        }
    }
}

impl ModifierNode for PointerInputNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
    fn on_detach(&mut self, _context: &mut crate::modifier::DetachContext<'_>) {
        self.stop();
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
        let inner = &self.scope.inner;
        inner.size.set(bounds);
        inner.density.set(context.density());
        inner.dispatching.set(true);
        // Started at the first event, polled at once to its first wait.
        if !self.job.is_some_and(|job| job.is_active()) {
            let job = self.coroutines.launch((self.block)(self.scope.clone()));
            compose_runtime::executor::poll_now(job);
            self.job = Some(job);
        }
        let Some(job) = self.job else {
            inner.dispatching.set(false);
            return;
        };
        let consumed = Rc::new(Cell::new(event.consumed));
        inner.delivery.replace(Some(Delivery {
            event: *event,
            pass,
            taken: false,
            consumed: consumed.clone(),
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

    /// The gesture is over without its events: the block starts over at the next one.
    fn on_cancel_pointer_input(&mut self) {
        self.stop();
    }
}
