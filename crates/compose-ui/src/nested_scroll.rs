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

/// What takes part in the scrolls of the scrollables inside it, as Compose's
/// `NestedScrollConnection`. Offsets and velocities are in pixels (per second) in the pointer's
/// direction: positive towards the bottom and the right (a finger moving down, content
/// scrolling back to its top). Each returns what it consumed. The fling callbacks don't
/// suspend here: one that animates launches its animation.
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
    fn on_pre_fling(&self, available: Offset) -> Offset {
        let _ = available;
        Offset::ZERO
    }

    /// After the scrollable's fling (or its drag's release, without one), as `onPostFling`.
    fn on_post_fling(&self, consumed: Offset, available: Offset) -> Offset {
        let _ = (consumed, available);
        Offset::ZERO
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
    pub fn pre_fling(&self, available: Offset) -> Offset {
        let mut consumed = Offset::ZERO;
        for connection in self.connections.iter() {
            consumed = consumed + connection.on_pre_fling(available - consumed);
        }
        consumed
    }

    /// Tells a fling's end, the innermost first, as `dispatchPostFling`.
    pub fn post_fling(&self, consumed: Offset, available: Offset) -> Offset {
        let mut taken = Offset::ZERO;
        for connection in self.connections.iter().rev() {
            taken = taken + connection.on_post_fling(consumed + taken, available - taken);
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

struct NestedScrollElement {
    connection: Rc<dyn NestedScrollConnection>,
}

impl ModifierNodeElement for NestedScrollElement {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(NestedScrollNode { connection: self.connection.clone() })
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<NestedScrollNode>() {
            node.connection = self.connection.clone();
        }
    }
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other
            .downcast_ref::<NestedScrollElement>()
            .is_some_and(|other| Rc::ptr_eq(&other.connection, &self.connection))
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
