//! Scrollbars, as Compose Desktop's `VerticalScrollbar` and `HorizontalScrollbar`: a thumb
//! showing where a scroll or a lazy list is, dragged to scroll it; a press on the track pages
//! toward it.

use std::rc::Rc;

use compose_runtime::{ComposeArg, CompositionLocal, composable};
use compose_ui::{
    Color, Constraints, Dp, DrawModifierNode, DrawScope, EventContext, IntSize, LayoutModifierNode,
    Measurable, MeasureResult, MeasureScope, Modifier, ModifierNode, ModifierNodeElement, Offset,
    PointerEvent, PointerEventPass, PointerEventType, PointerInputModifierNode, Rect, Shape,
};

use crate::lazy::LazyListState;
use crate::scroll::ScrollState;

/// What a scrollbar shows and moves, as Compose Desktop's `ScrollbarAdapter`: lengths along
/// the scroll's axis, in pixels. Reading them may read state: a scrollbar draws again when they
/// change.
pub trait ScrollbarAdapter: 'static {
    /// How far the content is scrolled.
    fn scroll_offset(&self) -> f64;
    /// The content's whole length.
    fn content_size(&self) -> f64;
    /// The viewport's length.
    fn viewport_size(&self) -> f64;
    /// Scrolls to `scroll_offset` (clamped to the content).
    fn scroll_to(&self, scroll_offset: f64);
}

/// A scroll state's adapter: exact.
struct ScrollStateAdapter(ScrollState);

impl ScrollbarAdapter for ScrollStateAdapter {
    fn scroll_offset(&self) -> f64 {
        self.0.value() as f64
    }
    fn content_size(&self) -> f64 {
        let max = self.0.max_value();
        if max == i32::MAX { 0.0 } else { (max + self.0.viewport_size()) as f64 }
    }
    fn viewport_size(&self) -> f64 {
        self.0.viewport_size() as f64
    }
    fn scroll_to(&self, scroll_offset: f64) {
        self.0.scroll_to(scroll_offset.round() as i32);
    }
}

/// A lazy list's adapter: from the average item size, as Compose Desktop's.
struct LazyListAdapter(LazyListState);

impl ScrollbarAdapter for LazyListAdapter {
    fn scroll_offset(&self) -> f64 {
        let (_, average, _, _, _) = self.0.layout_info();
        self.0.first_visible_item_index() as f64 * average as f64
            + self.0.first_visible_item_scroll_offset() as f64
    }
    /// The items at their average size, with the spacing between them (not after the last)
    /// and the padding, as Compose Desktop's.
    fn content_size(&self) -> f64 {
        let (count, average, spacing, _, padding) = self.0.layout_info();
        if count == 0 {
            return padding as f64;
        }
        count as f64 * average as f64 - spacing as f64 + padding as f64
    }
    fn viewport_size(&self) -> f64 {
        self.0.layout_info().3 as f64
    }
    fn scroll_to(&self, scroll_offset: f64) {
        let (count, average, _, _, _) = self.0.layout_info();
        let average = average.max(1) as f64;
        let offset = scroll_offset.max(0.0);
        let index = ((offset / average).floor() as usize).min(count.saturating_sub(1));
        self.0.scroll_to_item(index, (offset - index as f64 * average).round() as i32);
    }
}

/// An adapter for a scroll state, as `rememberScrollbarAdapter(scrollState)`.
#[composable(non_restartable)]
pub fn remember_scrollbar_adapter(scroll_state: ScrollState) -> Rc<dyn ScrollbarAdapter> {
    compose_runtime::remember_keyed(scroll_state, || {
        Rc::new(ScrollStateAdapter(scroll_state)) as Rc<dyn ScrollbarAdapter>
    })
}

/// An adapter for a lazy list, as `rememberScrollbarAdapter(lazyListState)`.
#[composable(non_restartable)]
pub fn remember_lazy_list_scrollbar_adapter(
    lazy_list_state: LazyListState,
) -> Rc<dyn ScrollbarAdapter> {
    compose_runtime::remember_keyed(lazy_list_state, || {
        Rc::new(LazyListAdapter(lazy_list_state)) as Rc<dyn ScrollbarAdapter>
    })
}

/// How scrollbars look, as Compose Desktop's `ScrollbarStyle`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ScrollbarStyle {
    /// The thumb's shortest length.
    pub minimal_height: Dp,
    pub thickness: Dp,
    pub shape: Shape,
    /// The thumb's color at rest.
    pub unhover_color: Color,
    /// Its color while hovered or dragged.
    pub hover_color: Color,
}

impl ComposeArg for ScrollbarStyle {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl Default for ScrollbarStyle {
    /// As `defaultScrollbarStyle()`.
    fn default() -> Self {
        ScrollbarStyle {
            minimal_height: Dp(16.0),
            thickness: Dp(8.0),
            shape: Shape::Rounded([compose_ui::CornerSize::Dp(Dp(4.0)); 4]),
            unhover_color: Color::BLACK.with_alpha(0.12),
            hover_color: Color::BLACK.with_alpha(0.50),
        }
    }
}

/// The scrollbars' style, as `LocalScrollbarStyle`.
pub static LOCAL_SCROLLBAR_STYLE: CompositionLocal<ScrollbarStyle> =
    CompositionLocal::new("LocalScrollbarStyle", ScrollbarStyle::default);

/// A vertical scrollbar for `adapter`, as Compose Desktop's `VerticalScrollbar`: as tall as it
/// may be, `style.thickness` wide; hidden while everything fits.
#[composable]
#[allow(non_snake_case)]
pub fn VerticalScrollbar(
    adapter: Rc<dyn ScrollbarAdapter>,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(false)] reverse_layout: bool,
    #[default(crate::scrollbar::LOCAL_SCROLLBAR_STYLE.current())] style: ScrollbarStyle,
) {
    crate::layout::Spacer(modifier.then_custom(ScrollbarElement {
        adapter,
        vertical: true,
        reverse: reverse_layout,
        style,
    }));
}

/// A horizontal scrollbar for `adapter`, as Compose Desktop's `HorizontalScrollbar`.
#[composable]
#[allow(non_snake_case)]
pub fn HorizontalScrollbar(
    adapter: Rc<dyn ScrollbarAdapter>,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(false)] reverse_layout: bool,
    #[default(crate::scrollbar::LOCAL_SCROLLBAR_STYLE.current())] style: ScrollbarStyle,
) {
    crate::layout::Spacer(modifier.then_custom(ScrollbarElement {
        adapter,
        vertical: false,
        reverse: reverse_layout,
        style,
    }));
}

struct ScrollbarElement {
    adapter: Rc<dyn ScrollbarAdapter>,
    vertical: bool,
    reverse: bool,
    style: ScrollbarStyle,
}

struct ScrollbarNode {
    adapter: Rc<dyn ScrollbarAdapter>,
    vertical: bool,
    reverse: bool,
    style: ScrollbarStyle,
    /// The thumb being dragged: where the press was along the axis and the offset then.
    drag: Option<(f32, f64)>,
    /// Pixels per dp, from the last measurement.
    density: compose_ui::Density,
    /// Horizontal in a right-to-left layout: reversed, as Compose Desktop's.
    rtl: bool,
    /// A mouse is over it.
    hovered: bool,
}

impl ModifierNodeElement for ScrollbarElement {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(ScrollbarNode {
            adapter: self.adapter.clone(),
            vertical: self.vertical,
            reverse: self.reverse,
            style: self.style,
            drag: None,
            density: compose_ui::Density::default(),
            hovered: false,
            rtl: false,
        })
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<ScrollbarNode>() {
            node.adapter = self.adapter.clone();
            node.vertical = self.vertical;
            node.reverse = self.reverse;
            node.style = self.style;
        }
    }
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<ScrollbarElement>().is_some_and(|other| {
            Rc::ptr_eq(&other.adapter, &self.adapter)
                && other.vertical == self.vertical
                && other.reverse == self.reverse
                && other.style == self.style
        })
    }
    fn name(&self) -> &'static str {
        "scrollbar"
    }
}

impl ModifierNode for ScrollbarNode {
    fn on_reset(&mut self) {
        self.drag = None;
        self.hovered = false;
    }
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
}

impl ScrollbarNode {
    /// Reversed: as asked, flipped in a right-to-left layout.
    fn reversed(&self) -> bool {
        self.reverse != self.rtl
    }

    /// The thumb's start and length along a track `track` pixels long, or `None` when the
    /// content fits.
    fn thumb(&self, track: f32) -> Option<(f32, f32)> {
        let content = self.adapter.content_size();
        let viewport = self.adapter.viewport_size();
        if content <= viewport || content <= 0.0 || track <= 0.0 {
            return None;
        }
        let minimal = self.density.to_px(self.style.minimal_height).min(track);
        let length = ((viewport / content) as f32 * track).max(minimal).min(track);
        let scrollable = content - viewport;
        let fraction = (self.adapter.scroll_offset() / scrollable).clamp(0.0, 1.0) as f32;
        let fraction = if self.reversed() { 1.0 - fraction } else { fraction };
        Some(((track - length) * fraction, length))
    }

    /// Pixels of content per pixel of thumb movement.
    fn scale(&self, track: f32) -> f64 {
        let Some((_, length)) = self.thumb(track) else { return 0.0 };
        let room = (track - length).max(1.0) as f64;
        let scale = (self.adapter.content_size() - self.adapter.viewport_size()) / room;
        if self.reversed() { -scale } else { scale }
    }
}

impl LayoutModifierNode for ScrollbarNode {
    /// Its thickness across, as long as allowed along.
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        self.density = scope.density();
        self.rtl = !self.vertical && scope.layout_direction() == compose_ui::LayoutDirection::Rtl;
        let thickness = scope.density().round_to_px(self.style.thickness);
        let along = |max: i32, min: i32| if max == Constraints::INFINITY { min } else { max };
        let size = if self.vertical {
            IntSize::new(
                constraints.constrain_width(thickness),
                along(constraints.max_height, constraints.min_height),
            )
        } else {
            IntSize::new(
                along(constraints.max_width, constraints.min_width),
                constraints.constrain_height(thickness),
            )
        };
        let placeable = scope.measure(content, Constraints::fixed(size.width, size.height));
        scope.place(&placeable, 0, 0);
        scope.layout(size.width, size.height)
    }
}

impl DrawModifierNode for ScrollbarNode {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        let size = scope.size();
        let track = if self.vertical { size.height } else { size.width };
        // Reads the adapter (state): a scroll draws it again.
        let Some((start, length)) = self.thumb(track) else { return };
        let rect = if self.vertical {
            Rect::new(0.0, start, size.width, start + length)
        } else {
            Rect::new(start, 0.0, start + length, size.height)
        };
        let active = self.drag.is_some() || self.hovered;
        let color = if active { self.style.hover_color } else { self.style.unhover_color };
        let size = compose_ui::Size::new(rect.width(), rect.height());
        let outline =
            match self.style.shape.outline_in(size, &scope.density(), scope.layout_direction()) {
                compose_ui::Outline::Rounded(_, radii) => compose_ui::Outline::Rounded(rect, radii),
                compose_ui::Outline::Rectangle(_) => compose_ui::Outline::Rectangle(rect),
            };
        scope.draw_outline(outline, compose_ui::Paint::fill(color));
    }
}

impl PointerInputModifierNode for ScrollbarNode {
    /// A press on the thumb drags it; on the track, scrolls a viewport toward the press.
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        if pass != PointerEventPass::Main {
            return;
        }
        let along = |position: Offset| if self.vertical { position.y } else { position.x };
        let track = if self.vertical { bounds.height } else { bounds.width } as f32;
        match event.kind {
            PointerEventType::Enter | PointerEventType::Exit => {
                self.hovered = event.kind == PointerEventType::Enter;
                context.invalidate_draw();
            }
            PointerEventType::Press if !event.consumed => {
                let Some((start, length)) = self.thumb(track) else { return };
                let at = along(event.position);
                event.consume();
                if at >= start && at <= start + length {
                    self.drag = Some((at, self.adapter.scroll_offset()));
                    context.invalidate_draw();
                } else {
                    let page = self.adapter.viewport_size();
                    let toward_end = (at > start + length) != self.reversed();
                    let offset =
                        self.adapter.scroll_offset() + if toward_end { page } else { -page };
                    let limit = (self.adapter.content_size() - page).max(0.0);
                    self.adapter.scroll_to(offset.clamp(0.0, limit));
                }
            }
            PointerEventType::Move => {
                if let Some((from, offset)) = self.drag {
                    event.consume();
                    let moved = (along(event.position) - from) as f64 * self.scale(track);
                    let limit =
                        (self.adapter.content_size() - self.adapter.viewport_size()).max(0.0);
                    self.adapter.scroll_to((offset + moved).clamp(0.0, limit));
                }
            }
            PointerEventType::Release if self.drag.take().is_some() => {
                event.consume();
                context.invalidate_draw();
            }
            _ => {}
        }
    }

    fn on_cancel_pointer_input(&mut self) {
        self.drag = None;
    }
}
