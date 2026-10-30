//! Nested scrolling, as Compose's `androidx.compose.ui.input.nestedscroll`: a
//! `NestedScrollConnection` given to `Modifier.nested_scroll` takes part in the scrolls of the
//! scrollables inside it, before them (`on_pre_scroll`, `on_pre_fling`) and after them
//! (`on_post_scroll`, `on_post_fling`).
//!
//! A scrollable finds the connections around it from the pointer's hit path: each nested-scroll
//! node on it adds its connection as an event starts (the initial pass runs from the outside
//! in), and the scrollable takes the ones before it when a gesture or a wheel turn reaches it
//! ([`NestedScrollChain::current`]), then dispatches through them as Compose's
//! `NestedScrollDispatcher`: the outermost first before the scroll, the innermost first after.

use std::cell::RefCell;
use std::rc::Rc;

use crate::input::{EventContext, PointerEvent, PointerEventPass, PointerEventType};
use crate::modifier::{Modifier, ModifierNode, ModifierNodeElement, PointerInputModifierNode};
use crate::unit::{IntSize, Offset};

/// Where a scroll comes from, as Compose's `NestedScrollSource`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NestedScrollSource {
    /// A drag or a wheel.
    UserInput,
    /// A fling or an animation.
    SideEffect,
}

/// A boxed future borrowing for `'a`, as the object-safe suspending functions of the traits here
/// and in foundation's scrolling return.
pub type LocalBoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + 'a>>;

/// What takes part in the scrolls of the scrollables inside it, as Compose's
/// `NestedScrollConnection`. Offsets and velocities are in pixels (per second) in the pointer's
/// direction: positive towards the bottom and the right (a finger moving down, content
/// scrolling back to its top). Each returns what it consumed. The fling callbacks suspend, as
/// Compose's: the fling's end waits for them, and what they leave goes on to the next.
pub trait NestedScrollConnection {
    /// Before the scrollable scrolls by `available`, as `onPreScroll`.
    fn on_pre_scroll(&self, available: Offset, source: NestedScrollSource) -> Offset {
        let _ = (available, source);
        Offset::ZERO
    }

    /// After the scrollable (and the connections inside this one) consumed `consumed`, with
    /// `available` left, as `onPostScroll`.
    fn on_post_scroll(
        &self,
        consumed: Offset,
        available: Offset,
        source: NestedScrollSource,
    ) -> Offset {
        let _ = (consumed, available, source);
        Offset::ZERO
    }

    /// Before the scrollable flings at `available`, as `onPreFling`.
    fn on_pre_fling(&self, available: Offset) -> LocalBoxFuture<'_, Offset> {
        let _ = available;
        Box::pin(async { Offset::ZERO })
    }

    /// After the scrollable's fling (or its drag's release, without one), as `onPostFling`.
    fn on_post_fling(&self, consumed: Offset, available: Offset) -> LocalBoxFuture<'_, Offset> {
        let _ = (consumed, available);
        Box::pin(async { Offset::ZERO })
    }
}

thread_local! {
    /// The connections the pointer event being dispatched has passed, outermost first.
    static PASSED: RefCell<Vec<Rc<dyn NestedScrollConnection>>> = const { RefCell::new(Vec::new()) };
}

/// A pointer event's dispatch starts: no connection passed yet.
pub(crate) fn begin_pointer_event() {
    PASSED.with(|passed| passed.borrow_mut().clear());
}

/// The connections around a scrollable, outermost first, as Compose's
/// `NestedScrollDispatcher` reaches them through the parents of its node.
#[derive(Clone, Default)]
pub struct NestedScrollChain {
    connections: Rc<[Rc<dyn NestedScrollConnection>]>,
}

impl NestedScrollChain {
    /// The connections around the node handling the pointer event, during its initial pass:
    /// those the event passed before reaching it.
    pub fn current() -> NestedScrollChain {
        let connections: Vec<_> = PASSED.with(|passed| passed.borrow().clone());
        NestedScrollChain { connections: connections.into() }
    }

    /// Adds `connection` for the nodes inside the one handling the pointer event, during its
    /// initial pass: a scrollable, a nested-scroll parent of the scrollables inside it, as
    /// Compose's `ScrollingLogic`.
    pub fn enter(connection: Rc<dyn NestedScrollConnection>) {
        PASSED.with(|passed| passed.borrow_mut().push(connection));
    }

    pub fn is_empty(&self) -> bool {
        self.connections.is_empty()
    }

    /// Offers `available` before scrolling, the outermost connection first (each gets what the
    /// ones around it left); returns what they consumed, as `dispatchPreScroll`.
    pub fn pre_scroll(&self, available: Offset, source: NestedScrollSource) -> Offset {
        let mut consumed = Offset::ZERO;
        for connection in self.connections.iter() {
            let used = connection.on_pre_scroll(available - consumed, source);
            consumed = consumed + used;
        }
        consumed
    }

    /// Tells what was scrolled and offers what's left, the innermost connection first; returns
    /// what they consumed, as `dispatchPostScroll`.
    pub fn post_scroll(
        &self,
        consumed: Offset,
        available: Offset,
        source: NestedScrollSource,
    ) -> Offset {
        let mut taken = Offset::ZERO;
        for connection in self.connections.iter().rev() {
            let used = connection.on_post_scroll(consumed + taken, available - taken, source);
            taken = taken + used;
        }
        taken
    }

    /// Offers a fling's velocity before it starts, the outermost first, as `dispatchPreFling`.
    pub async fn pre_fling(&self, available: Offset) -> Offset {
        let mut consumed = Offset::ZERO;
        for connection in self.connections.iter() {
            consumed = consumed + connection.on_pre_fling(available - consumed).await;
        }
        consumed
    }

    /// Tells a fling's end, the innermost first, as `dispatchPostFling`.
    pub async fn post_fling(&self, consumed: Offset, available: Offset) -> Offset {
        let mut taken = Offset::ZERO;
        for connection in self.connections.iter().rev() {
            taken = taken + connection.on_post_fling(consumed + taken, available - taken).await;
        }
        taken
    }
}

impl Modifier {
    /// Lets `connection` take part in the scrolls of the scrollables inside, as
    /// `Modifier.nestedScroll(connection)`.
    pub fn nested_scroll(self, connection: Rc<dyn NestedScrollConnection>) -> Modifier {
        self.then_custom(NestedScrollElement { connection })
    }
}

/// The node of `Modifier::nested_scroll`, for a node to delegate to, as
/// `nestedScrollModifierNode(connection, null)`: there's no dispatcher here.
pub fn nested_scroll_modifier_node(
    connection: Rc<dyn NestedScrollConnection>,
) -> impl ModifierNode {
    NestedScrollNode { connection }
}

struct NestedScrollElement {
    connection: Rc<dyn NestedScrollConnection>,
}

impl ModifierNodeElement for NestedScrollElement {
    type Node = NestedScrollNode;
    fn create(&self) -> NestedScrollNode {
        NestedScrollNode { connection: self.connection.clone() }
    }
    fn update(&self, node: &mut NestedScrollNode) {
        node.connection = self.connection.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&other.connection, &self.connection)
    }
    fn name(&self) -> &'static str {
        "nested_scroll"
    }
}

struct NestedScrollNode {
    connection: Rc<dyn NestedScrollConnection>,
}

impl ModifierNode for NestedScrollNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
    /// Not hit itself: it hears the events of what's hit inside it.
    fn is_pointer_observer(&self) -> bool {
        true
    }
}

impl PointerInputModifierNode for NestedScrollNode {
    /// On the way in, adds its connection for the scrollables inside to find. It handles
    /// nothing itself.
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        _bounds: IntSize,
        _context: &mut EventContext<'_>,
    ) {
        if pass == PointerEventPass::Initial
            && matches!(event.kind, PointerEventType::Press | PointerEventType::Scroll)
        {
            let connection = self.connection.clone();
            PASSED.with(|passed| passed.borrow_mut().push(connection));
        }
    }
}
