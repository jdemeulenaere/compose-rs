//! Nested scrolling, as Compose's `NestedScrollConnection` and `Modifier.nestedScroll`, as far as
//! Material's scrolling app bars need it.
//!
//! Compose's scrollables dispatch their scrolls to the connections around them; foundation's
//! don't yet, so this is done at the pointer's level: the modifier's node sees a wheel's delta
//! before the content (taking what the connection pre-scrolls from it) and after it (what's
//! left is the content's leftover, for the post-scroll), exactly. A drag can't be shared that
//! way (the content's scrollable reads the pointer's position, not a delta that could be
//! reduced): the connection gets the drag's deltas as pre-scrolls while the content also
//! scrolls by them, and the content's part of the rest is estimated (all of it, but past the
//! start the connection has seen it scrolled to).

use std::collections::VecDeque;
use std::rc::Rc;

use compose_ui::{
    EventContext, IntSize, Modifier, ModifierNode, ModifierNodeElement, Offset, PointerEvent,
    PointerEventPass, PointerEventType, PointerId, PointerInputModifierNode,
};

/// Where a scroll comes from, as Compose's `NestedScrollSource`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NestedScrollSource {
    /// A drag or a wheel.
    UserInput,
    /// An animation or a fling.
    SideEffect,
}

/// What takes part in the scrolls of the content inside it, as Compose's
/// `NestedScrollConnection`. Offsets and velocities are in pixels (per second), positive
/// towards the bottom and the end: a finger moving down, content scrolling back to its start.
/// Each returns what it consumed. The fling callbacks aren't suspending here: one that animates
/// launches its animation.
pub trait NestedScrollConnection {
    /// Before the content scrolls by `available`, as `onPreScroll`.
    fn on_pre_scroll(&self, available: Offset, source: NestedScrollSource) -> Offset {
        let _ = (available, source);
        Offset::ZERO
    }

    /// After the content scrolled by `consumed`, with `available` left, as `onPostScroll`.
    fn on_post_scroll(
        &self,
        consumed: Offset,
        available: Offset,
        source: NestedScrollSource,
    ) -> Offset {
        let _ = (consumed, available, source);
        Offset::ZERO
    }

    /// Before the content flings at `available`, as `onPreFling`.
    fn on_pre_fling(&self, available: Offset) -> Offset {
        let _ = available;
        Offset::ZERO
    }

    /// After the content's fling, as `onPostFling`.
    fn on_post_fling(&self, consumed: Offset, available: Offset) -> Offset {
        let _ = (consumed, available);
        Offset::ZERO
    }
}

/// `Modifier.nested_scroll`, on `Modifier`.
pub trait NestedScrollModifier {
    /// Lets `connection` take part in the scrolls of the content inside, as
    /// `Modifier.nestedScroll(connection)` (see the module's notes on drags).
    fn nested_scroll(self, connection: Rc<dyn NestedScrollConnection>) -> Modifier;
}

impl NestedScrollModifier for Modifier {
    fn nested_scroll(self, connection: Rc<dyn NestedScrollConnection>) -> Modifier {
        self.then_custom(NestedScrollElement { connection })
    }
}

struct NestedScrollElement {
    connection: Rc<dyn NestedScrollConnection>,
}

impl ModifierNodeElement for NestedScrollElement {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(NestedScrollNode {
            connection: self.connection.clone(),
            wheel_offered: None,
            drag: None,
            scrolled: 0.0,
        })
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
    /// What a wheel event offered the content after the pre-scroll, between its passes.
    wheel_offered: Option<Offset>,
    drag: Option<Drag>,
    /// How far the content has scrolled from its start, as far as seen (negative: into it).
    scrolled: f32,
}

struct Drag {
    pointer: PointerId,
    start: Offset,
    last: f32,
    dragging: bool,
    /// Times (nanoseconds) and vertical positions in the last 100 ms.
    samples: VecDeque<(u64, f32)>,
}

impl Drag {
    fn add(&mut self, time: u64, position: f32) {
        self.samples.push_back((time, position));
        while self.samples.front().is_some_and(|(at, _)| time.saturating_sub(*at) > 100_000_000) {
            self.samples.pop_front();
        }
    }

    fn velocity(&self) -> f32 {
        match (self.samples.front(), self.samples.back()) {
            (Some(&(first, from)), Some(&(last, to))) if last > first => {
                (to - from) / ((last - first) as f32 / 1e9)
            }
            _ => 0.0,
        }
    }
}

fn event_time(event: &PointerEvent) -> u64 {
    if event.time_nanos != 0 { event.time_nanos } else { compose_runtime::executor::now_nanos() }
}

impl ModifierNode for NestedScrollNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
}

impl NestedScrollNode {
    /// A drag's (or a wheel's) `available` delta, after the pre-scroll: the content's part
    /// estimated, the rest offered to the post-scroll.
    fn post_scroll_estimated(&mut self, left: f32) {
        let consumed = if left > 0.0 { left.min(-self.scrolled).max(0.0) } else { left };
        self.scrolled += consumed;
        self.connection.on_post_scroll(
            Offset::new(0.0, consumed),
            Offset::new(0.0, left - consumed),
            NestedScrollSource::UserInput,
        );
    }
}

impl PointerInputModifierNode for NestedScrollNode {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        _bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        match (event.kind, pass) {
            // A wheel: the connection first (outside in), then the content, then what's left
            // to the connection again (inside out).
            (PointerEventType::Scroll, PointerEventPass::Initial) if !event.consumed => {
                let available = Offset::new(-event.scroll_delta.x, -event.scroll_delta.y);
                let used = self.connection.on_pre_scroll(available, NestedScrollSource::UserInput);
                event.scroll_delta =
                    Offset::new(event.scroll_delta.x + used.x, event.scroll_delta.y + used.y);
                if event.scroll_delta.x.abs() < 0.01 && event.scroll_delta.y.abs() < 0.01 {
                    event.consume();
                } else {
                    self.wheel_offered = Some(event.scroll_delta);
                }
            }
            (PointerEventType::Scroll, PointerEventPass::Main) => {
                let Some(offered) = self.wheel_offered.take() else { return };
                let left = if event.consumed { Offset::ZERO } else { event.scroll_delta };
                let consumed = Offset::new(-(offered.x - left.x), -(offered.y - left.y));
                self.scrolled += consumed.y;
                let used = self.connection.on_post_scroll(
                    consumed,
                    Offset::new(-left.x, -left.y),
                    NestedScrollSource::UserInput,
                );
                if !event.consumed {
                    event.scroll_delta =
                        Offset::new(event.scroll_delta.x + used.x, event.scroll_delta.y + used.y);
                    if event.scroll_delta.x.abs() < 0.01 && event.scroll_delta.y.abs() < 0.01 {
                        event.consume();
                    }
                }
            }
            (PointerEventType::Press, PointerEventPass::Initial) => {
                let mut drag = Drag {
                    pointer: event.pointer,
                    start: event.position,
                    last: event.position.y,
                    dragging: false,
                    samples: VecDeque::new(),
                };
                drag.add(event_time(event), event.position.y);
                self.drag = Some(drag);
            }
            (PointerEventType::Move, PointerEventPass::Initial) => {
                let Some(drag) = &mut self.drag else { return };
                if drag.pointer != event.pointer || !event.pressed {
                    return;
                }
                drag.add(event_time(event), event.position.y);
                let delta = if drag.dragging {
                    event.position.y - drag.last
                } else {
                    // Past the touch slop, as the content's scrollable: what moved past it.
                    let moved = event.position.y - drag.start.y;
                    let slop = context.density().to_px(compose_ui::Dp(8.0));
                    if moved.abs() <= slop {
                        return;
                    }
                    drag.dragging = true;
                    moved - slop.copysign(moved)
                };
                drag.last = event.position.y;
                let used = self
                    .connection
                    .on_pre_scroll(Offset::new(0.0, delta), NestedScrollSource::UserInput);
                self.post_scroll_estimated(delta - used.y);
            }
            (PointerEventType::Release, PointerEventPass::Initial) => {
                let Some(mut drag) = self.drag.take() else { return };
                if drag.pointer != event.pointer || !drag.dragging {
                    return;
                }
                drag.add(event_time(event), event.position.y);
                let velocity = Offset::new(0.0, drag.velocity());
                let used = self.connection.on_pre_fling(velocity);
                self.connection.on_post_fling(
                    Offset::ZERO,
                    Offset::new(velocity.x - used.x, velocity.y - used.y),
                );
            }
            _ => {}
        }
    }

    fn on_cancel_pointer_input(&mut self) {
        self.drag = None;
    }
}
