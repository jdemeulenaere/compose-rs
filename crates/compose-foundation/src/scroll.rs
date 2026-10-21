//! Scrolling, as Compose's `Modifier.verticalScroll(state)` and `horizontalScroll(state)`: the
//! content measured without a bound along the axis, clipped, and moved by the scroll offset,
//! which the wheel and drags change.
//!
//! The offset is state: the node reads it while measuring (observed, so a write by the
//! application scrolls), and writes it when the wheel or a drag scrolls.

use std::rc::Rc;

use compose_runtime::{
    ComposeArg, MutableState, OwnedState, composable, mutable_state_of, remember,
};
use compose_ui::{
    Constraints, DrawModifierNode, DrawScope, EventContext, IntSize, LayoutModifierNode,
    Measurable, MeasureResult, MeasureScope, Modifier, ModifierNode, ModifierNodeElement, Offset,
    Outline, PointerEvent, PointerEventPass, PointerEventType, PointerId, PointerInputModifierNode,
    Rect,
};

/// How far content is scrolled, in pixels, and how far it can be, as Compose's `ScrollState`:
/// state, so reading it in composition recomposes on scrolls, and setting it scrolls.
#[derive(Clone)]
pub struct ScrollState {
    owner: Rc<(OwnedState<i32>, OwnedState<i32>)>,
}

impl ScrollState {
    pub fn new(initial: i32) -> ScrollState {
        ScrollState {
            owner: Rc::new((mutable_state_of(initial.max(0)), mutable_state_of(i32::MAX))),
        }
    }

    fn value_state(&self) -> MutableState<i32> {
        self.owner.0.handle()
    }

    fn max_state(&self) -> MutableState<i32> {
        self.owner.1.handle()
    }

    /// The scroll offset, in pixels.
    pub fn value(&self) -> i32 {
        self.value_state().get()
    }

    /// The largest offset, known once laid out.
    pub fn max_value(&self) -> i32 {
        self.max_state().get()
    }

    /// Scrolls to `value` (clamped to the content at the next layout), as `scrollTo`, without
    /// animation.
    pub fn scroll_to(&self, value: i32) {
        self.value_state().set(value.max(0));
    }
}

impl ComposeArg for ScrollState {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.owner, &other.owner)
    }
}

/// A scroll state kept across recompositions, as `rememberScrollState()`.
#[composable(non_restartable)]
pub fn remember_scroll_state() -> ScrollState {
    remember(|| ScrollState::new(0))
}

/// The scrolling modifiers, on `Modifier`.
pub trait ScrollModifiers {
    /// Scrolls the content vertically, as `Modifier.verticalScroll(state)`.
    fn vertical_scroll(self, state: ScrollState) -> Modifier;
    /// Scrolls the content horizontally, as `Modifier.horizontalScroll(state)`.
    fn horizontal_scroll(self, state: ScrollState) -> Modifier;
}

impl ScrollModifiers for Modifier {
    fn vertical_scroll(self, state: ScrollState) -> Modifier {
        self.then_custom(ScrollElement { state, vertical: true })
    }

    fn horizontal_scroll(self, state: ScrollState) -> Modifier {
        self.then_custom(ScrollElement { state, vertical: false })
    }
}

struct ScrollElement {
    state: ScrollState,
    vertical: bool,
}

impl ModifierNodeElement for ScrollElement {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(ScrollNode {
            state: self.state.clone(),
            vertical: self.vertical,
            drag: DragScroller::default(),
            fraction: 0.0,
        })
    }

    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<ScrollNode>() {
            node.state = self.state.clone();
            node.vertical = self.vertical;
        }
    }

    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other
            .downcast_ref::<ScrollElement>()
            .is_some_and(|other| other.vertical == self.vertical && other.state.same(&self.state))
    }

    fn name(&self) -> &'static str {
        "scroll"
    }
}

/// A drag in progress: the pointer, where it last was, and whether it moved past the touch slop
/// (then it scrolls, and clicks under it are cancelled).
struct Drag {
    pointer: PointerId,
    last: Offset,
    travelled: f32,
    scrolling: bool,
}

/// The distance a pointer moves before a drag scrolls, as Compose's touch slop (8 dp).
const TOUCH_SLOP: f32 = 8.0;

struct ScrollNode {
    state: ScrollState,
    vertical: bool,
    drag: DragScroller,
    /// The fraction of a pixel scrolled but not yet shown.
    fraction: f32,
}

impl ModifierNode for ScrollNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
    fn hit_clip(&self) -> Option<compose_ui::Shape> {
        Some(compose_ui::Shape::Rectangle)
    }
    /// Scrolls the least that shows `rect` (its start, if it's longer than the viewport).
    fn bring_into_view(&mut self, rect: Rect, viewport: IntSize) -> Option<Rect> {
        let (start, end, length) = if self.vertical {
            (rect.top, rect.bottom, viewport.height as f32)
        } else {
            (rect.left, rect.right, viewport.width as f32)
        };
        let delta = bring_into_view_delta(start, end, length);
        let used = scroll_state_by(&self.state, delta, &mut self.fraction);
        Some(if self.vertical {
            Rect::new(rect.left, rect.top - used, rect.right, rect.bottom - used)
        } else {
            Rect::new(rect.left - used, rect.top, rect.right - used, rect.bottom)
        })
    }
}

/// How far to scroll so `[start, end)` shows in `[0, length)`: none if it does, else the least,
/// its start first if it's longer.
pub(crate) fn bring_into_view_delta(start: f32, end: f32, length: f32) -> f32 {
    if start >= 0.0 && end <= length {
        0.0
    } else if start < 0.0 || end - start > length {
        start
    } else {
        end - length
    }
}

impl LayoutModifierNode for ScrollNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let content_constraints = if self.vertical {
            Constraints::new(constraints.min_width, constraints.max_width, 0, Constraints::INFINITY)
        } else {
            Constraints::new(
                0,
                Constraints::INFINITY,
                constraints.min_height,
                constraints.max_height,
            )
        };
        let placeable = scope.measure(content, content_constraints);
        let width = constraints.constrain_width(placeable.width);
        let height = constraints.constrain_height(placeable.height);
        let max =
            if self.vertical { placeable.height - height } else { placeable.width - width }.max(0);
        if self.state.max_state().get() != max {
            self.state.max_state().set(max);
        }
        let value = self.state.value_state().get().min(max);
        if value != self.state.value_state().get() {
            self.state.value_state().set(value);
        }
        if self.vertical {
            scope.place(&placeable, 0, -value);
        } else {
            scope.place(&placeable, -value, 0);
        }
        scope.layout(width, height)
    }
}

impl DrawModifierNode for ScrollNode {
    /// Clipped to its bounds.
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        scope.save();
        let size = scope.size();
        scope.clip_outline(Outline::Rectangle(Rect::new(0.0, 0.0, size.width, size.height)));
    }

    fn draw_after(&mut self, scope: &mut DrawScope<'_>) {
        scope.restore();
    }
}

impl PointerInputModifierNode for ScrollNode {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        _bounds: IntSize,
        _context: &mut EventContext<'_>,
    ) {
        let (vertical, state) = (self.vertical, self.state.clone());
        let fraction = &mut self.fraction;
        self.drag.on_pointer_event(event, pass, vertical, |delta| {
            scroll_state_by(&state, delta, fraction)
        });
    }

    fn on_cancel_pointer_input(&mut self) {
        self.drag.cancel();
    }
}

/// Scrolls `state` by `delta` pixels, within its range, carrying the fraction of a pixel left
/// over to the next delta (slow drags and fine wheels add up); returns the pixels used.
fn scroll_state_by(state: &ScrollState, delta: f32, fraction: &mut f32) -> f32 {
    let old = state.value_state().get();
    let max = state.max_state().get().max(0);
    let start = old as f32 + *fraction;
    let clamped = (start + delta).clamp(0.0, max as f32);
    let new = clamped.round();
    *fraction = clamped - new;
    if new as i32 != old {
        // The layout read the value: the write measures it again.
        state.value_state().set(new as i32);
    }
    clamped - start
}

/// The wheel and drags of a scrollable node, as Compose's `scrollable`: `scroll_by` gets the
/// deltas (positive towards the end) and returns the part it used.
///
/// A drag is taken by the innermost scrollable once past the touch slop (its Main pass, which
/// runs from the target outward: an outer one sees it consumed), then in the Initial pass, so
/// what's under it sees it consumed (a click there is cancelled). A wheel delta goes to the
/// innermost first; what it doesn't use goes on to the scrollables around it.
#[derive(Default)]
pub(crate) struct DragScroller {
    drag: Option<Drag>,
}

impl DragScroller {
    pub(crate) fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        vertical: bool,
        mut scroll_by: impl FnMut(f32) -> f32,
    ) {
        match (event.kind, pass) {
            (PointerEventType::Scroll, PointerEventPass::Main) if !event.consumed => {
                let delta = if vertical { event.scroll_delta.y } else { event.scroll_delta.x };
                if delta == 0.0 {
                    return;
                }
                let used = scroll_by(delta);
                let left = delta - used;
                if vertical {
                    event.scroll_delta.y = left;
                } else {
                    event.scroll_delta.x = left;
                }
                if left.abs() < 0.01 {
                    event.consume();
                }
            }
            (PointerEventType::Press, PointerEventPass::Initial) => {
                self.drag = Some(Drag {
                    pointer: event.pointer,
                    last: event.position,
                    travelled: 0.0,
                    scrolling: false,
                });
            }
            (PointerEventType::Move, PointerEventPass::Initial) => {
                let Some(drag) = &mut self.drag else { return };
                if drag.pointer != event.pointer || !event.pressed {
                    return;
                }
                let moved = event.position - drag.last;
                let along = if vertical { moved.y } else { moved.x };
                if drag.scrolling {
                    drag.last = event.position;
                    event.consume();
                    // Dragging content up scrolls down.
                    scroll_by(-along);
                }
            }
            (PointerEventType::Move, PointerEventPass::Main) => {
                let Some(drag) = &mut self.drag else { return };
                if drag.scrolling || drag.pointer != event.pointer || !event.pressed {
                    return;
                }
                let moved = event.position - drag.last;
                let along = if vertical { moved.y } else { moved.x };
                if event.consumed {
                    // An inner scrollable or handler took it.
                    drag.last = event.position;
                    return;
                }
                drag.travelled += along.abs();
                drag.last = event.position;
                if drag.travelled > TOUCH_SLOP {
                    drag.scrolling = true;
                    event.consume();
                    // What moved past the slop scrolls.
                    let past = (drag.travelled - TOUCH_SLOP).copysign(along);
                    scroll_by(-past);
                }
            }
            (PointerEventType::Release, PointerEventPass::Initial) => {
                if let Some(drag) = self.drag.take()
                    && drag.scrolling
                {
                    event.consume();
                }
            }
            _ => {}
        }
    }

    pub(crate) fn cancel(&mut self) {
        self.drag = None;
    }
}
