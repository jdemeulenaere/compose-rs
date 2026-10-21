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
/// Desktop's `TooltipPlacement.CursorPoint`; kept in the window.
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
/// the wait again), and hides when it leaves or presses.
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
    let scope = remember_coroutine_scope();
    let waiting = remember(|| Rc::new(Cell::new(None::<Job>)));
    let density = compose_ui::LOCAL_DENSITY.current();
    let offset = IntOffset::new(
        density.round_to_px(tooltip_placement.offset_x),
        density.round_to_px(tooltip_placement.offset_y),
    );
    let provider = {
        let cursor = cursor.clone();
        compose_runtime::remember_keyed(OffsetKey(offset), move || {
            Rc::new(CursorPoint { cursor, offset }) as Rc<dyn PopupPositionProvider>
        })
    };
    let on_hover = {
        let (cursor, scope, waiting) = (cursor.clone(), scope.clone(), waiting.clone());
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
    let (content, tooltip) =
        (compose_runtime::callback(content), compose_runtime::callback(tooltip));
    Box(
        modifier.then_custom(HoverElement(on_hover)),
        Alignment::TOP_START,
        false,
        compose_runtime::callback(move |_| {
            content();
            if visible.get() {
                let tooltip = tooltip.clone();
                compose_ui::PopupWithPositionProvider(
                    provider.clone(),
                    None::<Callback>,
                    PopupProperties::default(),
                    compose_runtime::callback(move || tooltip()),
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

/// At the pointer (in the anchor, the area), moved by `offset`, kept in the window.
struct CursorPoint {
    cursor: Rc<Cell<Offset>>,
    offset: IntOffset,
}

impl PopupPositionProvider for CursorPoint {
    fn position(
        &self,
        anchor: Rect,
        window: IntSize,
        _layout_direction: compose_ui::LayoutDirection,
        popup: IntSize,
    ) -> IntOffset {
        let cursor = self.cursor.get();
        let x = (anchor.left + cursor.x).round() as i32 + self.offset.x;
        let y = (anchor.top + cursor.y).round() as i32 + self.offset.y;
        IntOffset::new(
            x.min(window.width - popup.width).max(0),
            y.min(window.height - popup.height).max(0),
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

struct HoverElement(Callback<dyn Fn(HoverKind, Offset)>);

struct HoverNode(Callback<dyn Fn(HoverKind, Offset)>);

impl ModifierNodeElement for HoverElement {
    fn create(&self) -> std::boxed::Box<dyn ModifierNode> {
        std::boxed::Box::new(HoverNode(self.0.clone()))
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<HoverNode>() {
            node.0 = self.0.clone();
        }
    }
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<HoverElement>().is_some_and(|other| other.0.ptr_eq(&self.0))
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
        _bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        if pass != PointerEventPass::Initial
            && event.kind != PointerEventType::Exit
            && event.kind != PointerEventType::Enter
        {
            return;
        }
        let kind = match event.kind {
            PointerEventType::Enter | PointerEventType::Move if !event.pressed => HoverKind::Rest,
            PointerEventType::Exit | PointerEventType::Press => HoverKind::Leave,
            _ => return,
        };
        let (callback, position) = (self.0.clone(), event.position);
        context.run(move || callback(kind, position));
    }
}
