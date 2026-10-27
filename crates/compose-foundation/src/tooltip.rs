//! `TooltipArea`, as Compose Desktop's: content whose tooltip shows in a popup at the mouse
//! pointer after it rests there, and hides when it leaves or presses.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compose_runtime::{
    Callback, CoroutineScope, Job, composable, mutable_state_of, remember, remember_coroutine_scope,
};
use compose_ui::{
    Alignment, Dp, EventContext, IntOffset, IntSize, Modifier, ModifierNode, ModifierNodeElement,
    Offset, PointerEvent, PointerEventPass, PointerEventType, PointerInputModifierNode,
    PopupPositionProvider, PopupProperties, Rect,
};

use crate::layout::Box;

/// A tooltip at the mouse pointer, `offset` from it (16 dp below by default), as Compose
/// Desktop's `TooltipPlacement.CursorPoint`: on the other side of the pointer where there's
/// no room, and kept in the window.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TooltipPlacement {
    pub offset_x: Dp,
    pub offset_y: Dp,
}

impl Default for TooltipPlacement {
    fn default() -> Self {
        TooltipPlacement { offset_x: Dp(0.0), offset_y: Dp(16.0) }
    }
}

impl compose_runtime::ComposeArg for TooltipPlacement {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// `content` with a tooltip, as Compose Desktop's `TooltipArea`: `tooltip` shows in a popup
/// at the mouse pointer once it has rested over the content for `delay_millis` (moving starts
/// the wait again), and hides when it leaves the area (the pointer may rest on the tooltip
/// while inside it) or presses.
#[composable]
#[allow(non_snake_case)]
pub fn TooltipArea(
    tooltip: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(500)] delay_millis: u32,
    #[default(crate::tooltip::TooltipPlacement::default())] tooltip_placement: TooltipPlacement,
    content: impl Fn() + 'static,
) {
    let visible = remember(|| mutable_state_of(false));
    // Where the pointer is, in the area: where the tooltip goes.
    let cursor = remember(|| Rc::new(Cell::new(Offset::ZERO)));
    // The area's bounds in the window, as the tooltip was last placed.
    let area = remember(|| Rc::new(Cell::new(Rect::default())));
    let scope = remember_coroutine_scope();
    let waiting = remember(|| Rc::new(Cell::new(None::<Job>)));
    let density = compose_ui::LOCAL_DENSITY.current();
    let offset = IntOffset::new(
        density.round_to_px(tooltip_placement.offset_x),
        density.round_to_px(tooltip_placement.offset_y),
    );
    let provider = {
        let (cursor, area) = (cursor.clone(), area.clone());
        compose_runtime::remember_keyed(OffsetKey(offset), move || {
            Rc::new(CursorPoint { cursor, offset, area }) as Rc<dyn PopupPositionProvider>
        })
    };
    let on_hover = {
        let (cursor, scope, waiting) = (cursor.clone(), scope, waiting.clone());
        compose_runtime::callback(move |kind: HoverKind, position: Offset| {
            cancel(&waiting);
            match kind {
                HoverKind::Rest => {
                    cursor.set(position);
                    if !visible.get() {
                        wait_then_show(&scope, &waiting, delay_millis, visible);
                    }
                }
                HoverKind::Leave => visible.set(false),
            }
        })
    };
    // The pointer on the tooltip: it hides once that's outside the area.
    let on_tooltip_hover = compose_runtime::callback(move |_: HoverKind, position: Offset| {
        if !area.get().contains(position) {
            visible.set(false);
        }
    });
    let (content, tooltip) =
        (compose_runtime::callback(content), compose_runtime::callback(tooltip));
    Box(
        modifier.then_custom(HoverElement { on_hover, in_window: false }),
        Alignment::TOP_START,
        false,
        compose_runtime::callback(move |_| {
            content();
            if visible.get() {
                let (tooltip, on_tooltip_hover) = (tooltip.clone(), on_tooltip_hover.clone());
                compose_ui::PopupWithPositionProvider(
                    provider.clone(),
                    None::<Callback>,
                    PopupProperties::default(),
                    compose_runtime::callback(move || {
                        let tooltip = tooltip.clone();
                        Box(
                            Modifier.then_custom(HoverElement {
                                on_hover: on_tooltip_hover.clone(),
                                in_window: true,
                            }),
                            Alignment::TOP_START,
                            false,
                            compose_runtime::callback(move |_| tooltip()),
                        );
                    }),
                );
            }
        }),
    );
}

fn cancel(waiting: &Cell<Option<Job>>) {
    if let Some(job) = waiting.take() {
        job.cancel();
    }
}

fn wait_then_show(
    scope: &CoroutineScope,
    waiting: &Rc<Cell<Option<Job>>>,
    delay_millis: u32,
    visible: compose_runtime::MutableState<bool>,
) {
    let job = scope.launch(async move {
        compose_runtime::delay(Duration::from_millis(delay_millis as u64)).await;
        visible.set(true);
    });
    waiting.set(Some(job));
}

#[derive(Clone, Copy, PartialEq)]
struct OffsetKey(IntOffset);

impl compose_runtime::ComposeArg for OffsetKey {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// At the pointer (in the anchor, the area), moved by `offset` (or as far the other way where
/// there's no room), kept in the window, as Compose Desktop's
/// `PopupPositionProviderAtPosition`. Keeps the anchor's bounds.
struct CursorPoint {
    cursor: Rc<Cell<Offset>>,
    offset: IntOffset,
    area: Rc<Cell<Rect>>,
}

impl PopupPositionProvider for CursorPoint {
    fn position(
        &self,
        anchor: Rect,
        window: IntSize,
        _layout_direction: compose_ui::LayoutDirection,
        popup: IntSize,
    ) -> IntOffset {
        self.area.set(anchor);
        let cursor = self.cursor.get();
        let place = |at: i32, offset: i32, size: i32, room: i32| {
            let after = at + offset;
            let before = at - offset - size;
            let at = if after + size > room && before >= 0 { before } else { after };
            at.min(room - size).max(0)
        };
        IntOffset::new(
            place(
                (anchor.left + cursor.x).round() as i32,
                self.offset.x,
                popup.width,
                window.width,
            ),
            place(
                (anchor.top + cursor.y).round() as i32,
                self.offset.y,
                popup.height,
                window.height,
            ),
        )
    }
}

/// What the pointer did over the area.
#[derive(Clone, Copy, PartialEq, Debug)]
enum HoverKind {
    /// It came or moved over it (at a position in it).
    Rest,
    /// It left, or pressed.
    Leave,
}

/// Tells `on_hover` what the pointer does over the node, at positions in it (in the window if
/// `in_window`). An `Exit` while the pointer is still within the node's bounds (on a popup over
/// it) isn't a leave.
struct HoverElement {
    on_hover: Callback<dyn Fn(HoverKind, Offset)>,
    in_window: bool,
}

struct HoverNode {
    on_hover: Callback<dyn Fn(HoverKind, Offset)>,
    in_window: bool,
}

impl ModifierNodeElement for HoverElement {
    type Node = HoverNode;
    fn create(&self) -> HoverNode {
        HoverNode { on_hover: self.on_hover.clone(), in_window: self.in_window }
    }
    fn update(&self, node: &mut HoverNode) {
        node.on_hover = self.on_hover.clone();
        node.in_window = self.in_window;
    }
    fn same(&self, other: &Self) -> bool {
        other.on_hover.ptr_eq(&self.on_hover) && other.in_window == self.in_window
    }
    fn name(&self) -> &'static str {
        "tooltip_area"
    }
}

impl ModifierNode for HoverNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for HoverNode {
    /// Watches the pointer in the initial pass, taking nothing from the content.
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        if pass != PointerEventPass::Initial {
            return;
        }
        let position = event.position;
        let inside = position.x >= 0.0
            && position.y >= 0.0
            && position.x < bounds.width as f32
            && position.y < bounds.height as f32;
        let kind = match event.kind {
            PointerEventType::Enter | PointerEventType::Move if !event.pressed => HoverKind::Rest,
            // Onto a popup over the node (its tooltip): still over it.
            PointerEventType::Exit if inside && !self.in_window => return,
            PointerEventType::Exit | PointerEventType::Press => HoverKind::Leave,
            _ => return,
        };
        let position =
            if self.in_window { context.pointer_position().unwrap_or(position) } else { position };
        let callback = self.on_hover.clone();
        context.run(move || callback(kind, position));
    }
}
