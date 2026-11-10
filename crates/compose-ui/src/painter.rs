//! Painters, as Compose's `androidx.compose.ui.graphics.painter`: something drawn into a box of
//! any size, with an intrinsic size it'd like, faded and filtered on request; and
//! `Modifier.paint`, which sizes a node by one and draws it.

use std::any::Any;
use std::cell::Cell;
use std::rc::Rc;

use crate::alignment::{Alignment, LayoutDirection};
use crate::brush::Brush;
use crate::color_filter::{BlendMode, ColorFilter};
use crate::constraints::Constraints;
use crate::content_scale::ContentScale;
use crate::draw::DrawScope;
use crate::graphics::{Paint, PaintStyle};
use crate::image::{FilterQuality, ImageBitmap};
use crate::layout::{IntrinsicMeasureScope, Measurable, MeasureResult, MeasureScope};
use crate::modifier::{
    DrawModifierNode, InspectorInfo, LayoutModifierNode, Modifier, ModifierNode,
    ModifierNodeElement,
};
use crate::unit::{IntOffset, IntSize, Rect, Size};

/// What a painter draws, as a subclass of Compose's `Painter` implements: its intrinsic size,
/// its drawing, and whether it applies an alpha, a color filter or a layout direction itself
/// (`true`), or leaves the alpha and the filter to a layer [`Painter::draw`] puts around it.
/// The hooks take `&self`, as a Kotlin object changes its own fields: keep what they set in a
/// `Cell`. Wrap it in a [`Painter`] to draw it ([`Painter::new`]).
pub trait CustomPainter: Any {
    /// The size it'd like, as `intrinsicSize`: `Size::UNSPECIFIED` (or an infinite side) for
    /// none, as a color fills any size.
    fn intrinsic_size(&self) -> Size;

    /// Draws it into `scope`, whose size is the size it's drawn at, as `DrawScope.onDraw`.
    fn on_draw(&self, scope: &mut DrawScope<'_>);

    /// Takes `alpha` for its drawing, as `applyAlpha`: `false` (the default) has a layer fade
    /// it.
    fn apply_alpha(&self, _alpha: f32) -> bool {
        false
    }

    /// Takes `color_filter` for its drawing, as `applyColorFilter`: `false` (the default) has
    /// a layer filter it.
    fn apply_color_filter(&self, _color_filter: Option<&ColorFilter>) -> bool {
        false
    }

    /// Learns the direction it's drawn in, as `applyLayoutDirection`.
    fn apply_layout_direction(&self, _layout_direction: LayoutDirection) -> bool {
        false
    }

    /// Whether `other` paints the same, as a painter's `equals` (a data class's): `false` by
    /// default, a painter being equal to itself only.
    fn same(&self, _other: &dyn CustomPainter) -> bool {
        false
    }
}

impl dyn CustomPainter {
    /// `self` as `T`, for [`same`](CustomPainter::same).
    pub fn downcast_ref<T: CustomPainter>(&self) -> Option<&T> {
        (self as &dyn Any).downcast_ref::<T>()
    }
}

/// What a painter was last drawn with, as Compose's `Painter` keeps it: a hook runs only for a
/// change, and what the painter didn't take goes into a layer.
#[derive(Clone, Copy)]
struct DrawState {
    alpha: f32,
    color_filter: Option<ColorFilter>,
    layout_direction: LayoutDirection,
    /// The alpha and the filter the layer applies (the painter didn't take them).
    layer_alpha: f32,
    layer_filter: Option<ColorFilter>,
}

struct PainterCell<P: ?Sized> {
    state: Cell<DrawState>,
    painter: P,
}

/// A painter, as Compose's `Painter`: shared as a Kotlin reference is (a clone is the same
/// painter, which keeps what it was drawn with), equal to itself and to what its
/// [`CustomPainter::same`] says. Make one from a [`CustomPainter`], a [`BitmapPainter`] or a
/// [`ColorPainter`] (`.into()`).
#[derive(Clone)]
pub struct Painter(Rc<PainterCell<dyn CustomPainter>>);

impl Painter {
    pub fn new(painter: impl CustomPainter) -> Painter {
        let state = DrawState {
            alpha: 1.0,
            color_filter: None,
            layout_direction: LayoutDirection::Ltr,
            layer_alpha: 1.0,
            layer_filter: None,
        };
        Painter(Rc::new(PainterCell { state: Cell::new(state), painter }))
    }

    /// Its intrinsic size, as `intrinsicSize`.
    pub fn intrinsic_size(&self) -> Size {
        self.0.painter.intrinsic_size()
    }

    /// What it paints with, to read its fields: `None` if it isn't a `T`.
    pub fn downcast_ref<T: CustomPainter>(&self) -> Option<&T> {
        self.0.painter.downcast_ref::<T>()
    }

    /// Whether `other` is this painter, not only an equal one.
    pub fn ptr_eq(&self, other: &Painter) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    /// Draws it at `size`, from the scope's top left, at `alpha` and through `color_filter`, as
    /// `DrawScope.draw(size, alpha, colorFilter)`: each given to the painter's hook when it
    /// changes (and the scope's layout direction), what the painter doesn't take applied by a
    /// layer of `size`. Nothing for an alpha of 0 or an empty size.
    pub fn draw(
        &self,
        scope: &mut DrawScope<'_>,
        size: Size,
        alpha: f32,
        color_filter: Option<&ColorFilter>,
    ) {
        let painter = &self.0.painter;
        let mut state = self.0.state.get();
        if state.alpha != alpha {
            state.layer_alpha = if painter.apply_alpha(alpha) { 1.0 } else { alpha };
            state.alpha = alpha;
        }
        if state.color_filter.as_ref() != color_filter {
            let taken = painter.apply_color_filter(color_filter);
            state.layer_filter = if taken { None } else { color_filter.copied() };
            state.color_filter = color_filter.copied();
        }
        let direction = scope.layout_direction();
        if state.layout_direction != direction {
            painter.apply_layout_direction(direction);
            state.layout_direction = direction;
        }
        self.0.state.set(state);
        let whole = scope.size();
        scope.inset(0.0, 0.0, whole.width - size.width, whole.height - size.height, |scope| {
            if !(alpha > 0.0 && size.width > 0.0 && size.height > 0.0) {
                return;
            }
            if state.layer_alpha < 1.0 || state.layer_filter.is_some() {
                let bounds = Rect::new(0.0, 0.0, size.width, size.height);
                let filter = state.layer_filter.as_ref();
                scope.save_layer(bounds, state.layer_alpha, filter, BlendMode::SrcOver);
                painter.on_draw(scope);
                scope.restore();
            } else {
                painter.on_draw(scope);
            }
        });
    }
}

impl<P: CustomPainter> From<P> for Painter {
    fn from(painter: P) -> Painter {
        Painter::new(painter)
    }
}

impl PartialEq for Painter {
    fn eq(&self, other: &Painter) -> bool {
        self.ptr_eq(other) || self.0.painter.same(&other.0.painter)
    }
}

impl std::fmt::Debug for Painter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Painter(intrinsic size {:?})", self.intrinsic_size())
    }
}

impl compose_runtime::ComposeArg for Painter {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A bitmap, or a part of it, as Compose's `BitmapPainter`: its intrinsic size the part's,
/// stretched to the size it's drawn at (sampled as its filter quality says), faded and filtered
/// itself. Equal to another of the same image, part and filter quality.
pub struct BitmapPainter {
    image: ImageBitmap,
    src_offset: IntOffset,
    src_size: IntSize,
    filter_quality: FilterQuality,
    alpha: Cell<f32>,
    color_filter: Cell<Option<ColorFilter>>,
}

impl BitmapPainter {
    /// All of `image`, as `BitmapPainter(image)`.
    pub fn new(image: ImageBitmap) -> BitmapPainter {
        let size = IntSize::new(image.width() as i32, image.height() as i32);
        BitmapPainter::with_source(image, IntOffset::new(0, 0), size)
    }

    /// The part of `image` of `src_size` at `src_offset`, as `BitmapPainter(image, srcOffset,
    /// srcSize)`.
    ///
    /// # Panics
    ///
    /// If the offset or the size is negative, or the size larger than the image, as Compose's
    /// `require`.
    pub fn with_source(image: ImageBitmap, src_offset: IntOffset, src_size: IntSize) -> Self {
        assert!(
            src_offset.x >= 0
                && src_offset.y >= 0
                && src_size.width >= 0
                && src_size.height >= 0
                && src_size.width as u32 <= image.width()
                && src_size.height as u32 <= image.height(),
            "compose-ui: a bitmap painter's source ({src_offset:?}, {src_size:?}) must be in its \
             image ({} × {})",
            image.width(),
            image.height()
        );
        BitmapPainter {
            image,
            src_offset,
            src_size,
            filter_quality: FilterQuality::Low,
            alpha: Cell::new(1.0),
            color_filter: Cell::new(None),
        }
    }

    /// `with_source`, sampled as `filter_quality` says, as `BitmapPainter(image, srcOffset,
    /// srcSize, filterQuality)` (Compose's default: `FilterQuality::Low`).
    pub fn with_filter_quality(
        image: ImageBitmap,
        src_offset: IntOffset,
        src_size: IntSize,
        filter_quality: FilterQuality,
    ) -> Self {
        BitmapPainter { filter_quality, ..BitmapPainter::with_source(image, src_offset, src_size) }
    }
}

impl CustomPainter for BitmapPainter {
    fn intrinsic_size(&self) -> Size {
        self.src_size.into()
    }

    fn on_draw(&self, scope: &mut DrawScope<'_>) {
        let (x, y) = (self.src_offset.x as f32, self.src_offset.y as f32);
        let src = Rect::new(x, y, x + self.src_size.width as f32, y + self.src_size.height as f32);
        let size = scope.size();
        let dst = Rect::new(0.0, 0.0, size.width.round(), size.height.round());
        let filter = self.color_filter.get();
        let (image, alpha, quality) = (&self.image, self.alpha.get(), self.filter_quality);
        scope.draw_image_with(image, src, dst, alpha, filter.as_ref(), quality);
    }

    fn apply_alpha(&self, alpha: f32) -> bool {
        self.alpha.set(alpha);
        true
    }

    fn apply_color_filter(&self, color_filter: Option<&ColorFilter>) -> bool {
        self.color_filter.set(color_filter.copied());
        true
    }

    fn same(&self, other: &dyn CustomPainter) -> bool {
        other.downcast_ref::<BitmapPainter>().is_some_and(|other| {
            other.image.same_image(&self.image)
                && other.src_offset == self.src_offset
                && other.src_size == self.src_size
                && other.filter_quality == self.filter_quality
        })
    }
}

/// A brush filling the size it's drawn at, as Compose's `BrushPainter`: its intrinsic size the
/// brush's ([`Brush::intrinsic_size`]), faded and filtered itself; equal to another of an equal
/// brush.
#[derive(Debug)]
pub struct BrushPainter {
    pub brush: Brush,
    alpha: Cell<f32>,
    color_filter: Cell<Option<ColorFilter>>,
}

impl BrushPainter {
    /// As `BrushPainter(brush)`.
    pub fn new(brush: impl Into<Brush>) -> BrushPainter {
        BrushPainter { brush: brush.into(), alpha: Cell::new(1.0), color_filter: Cell::new(None) }
    }
}

impl CustomPainter for BrushPainter {
    fn intrinsic_size(&self) -> Size {
        self.brush.intrinsic_size()
    }

    fn on_draw(&self, scope: &mut DrawScope<'_>) {
        let paint = scope.brush_paint(&self.brush, self.alpha.get(), PaintStyle::Fill);
        let paint = scope.paint_with_color_filter(paint, self.color_filter.get().as_ref());
        let bounds = scope.bounds();
        scope.draw_rect(bounds, paint);
    }

    fn apply_alpha(&self, alpha: f32) -> bool {
        self.alpha.set(alpha);
        true
    }

    fn apply_color_filter(&self, color_filter: Option<&ColorFilter>) -> bool {
        self.color_filter.set(color_filter.copied());
        true
    }

    fn same(&self, other: &dyn CustomPainter) -> bool {
        other.downcast_ref::<BrushPainter>().is_some_and(|other| other.brush == self.brush)
    }
}

/// A color filling the size it's drawn at, as Compose's `ColorPainter`: no intrinsic size,
/// faded and filtered itself; equal to another of the same color.
pub struct ColorPainter {
    pub color: crate::graphics::Color,
    alpha: Cell<f32>,
    color_filter: Cell<Option<ColorFilter>>,
}

impl ColorPainter {
    pub fn new(color: crate::graphics::Color) -> ColorPainter {
        ColorPainter { color, alpha: Cell::new(1.0), color_filter: Cell::new(None) }
    }
}

impl CustomPainter for ColorPainter {
    fn intrinsic_size(&self) -> Size {
        Size::UNSPECIFIED
    }

    fn on_draw(&self, scope: &mut DrawScope<'_>) {
        let paint = Paint::fill(self.color).with_alpha(self.alpha.get());
        let paint = scope.paint_with_color_filter(paint, self.color_filter.get().as_ref());
        let bounds = scope.bounds();
        scope.draw_rect(bounds, paint);
    }

    fn apply_alpha(&self, alpha: f32) -> bool {
        self.alpha.set(alpha);
        true
    }

    fn apply_color_filter(&self, color_filter: Option<&ColorFilter>) -> bool {
        self.color_filter.set(color_filter.copied());
        true
    }

    fn same(&self, other: &dyn CustomPainter) -> bool {
        other.downcast_ref::<ColorPainter>().is_some_and(|other| other.color == self.color)
    }
}

impl Modifier {
    /// Sizes the node by `painter` and draws it, as `Modifier.paint(painter)`: sized to its
    /// intrinsic size, centered, scaled down to fit inside (`ContentScale::Inside`).
    pub fn paint(self, painter: impl Into<Painter>) -> Modifier {
        self.paint_with(
            painter,
            true,
            Alignment::CENTER,
            ContentScale::Inside,
            1.0,
            None::<ColorFilter>,
        )
    }

    /// `paint` with each of Compose's parameters: whether the node takes the painter's
    /// intrinsic size within its constraints (`size_to_intrinsics`), where the painter goes in
    /// it, how its intrinsic size scales to the node's, and the alpha and color filter it's
    /// drawn with, as `Modifier.paint(painter, sizeToIntrinsics, alignment, contentScale,
    /// alpha, colorFilter)`. Its content is drawn over it.
    pub fn paint_with(
        self,
        painter: impl Into<Painter>,
        size_to_intrinsics: bool,
        alignment: Alignment,
        content_scale: ContentScale,
        alpha: f32,
        color_filter: impl Into<Option<ColorFilter>>,
    ) -> Modifier {
        self.then_custom(PainterElement {
            painter: painter.into(),
            size_to_intrinsics,
            alignment,
            content_scale,
            alpha,
            color_filter: color_filter.into(),
        })
    }
}

#[derive(Clone)]
struct PainterElement {
    painter: Painter,
    size_to_intrinsics: bool,
    alignment: Alignment,
    content_scale: ContentScale,
    alpha: f32,
    color_filter: Option<ColorFilter>,
}

/// `Modifier.paint`'s node, as Compose's `PainterNode`: it invalidates itself on an update, its
/// measurement only when its painter's intrinsics change (`should_auto_invalidate` is false),
/// so a new alpha or filter (an animated tint) draws it again without measuring it.
struct PainterNode {
    params: PainterElement,
    delegates: crate::delegation::Delegates,
    /// An update changed what its measurement reads.
    remeasure: bool,
}

impl std::ops::Deref for PainterNode {
    type Target = PainterElement;
    fn deref(&self) -> &PainterElement {
        &self.params
    }
}

impl ModifierNodeElement for PainterElement {
    type Node = PainterNode;

    fn create(&self) -> PainterNode {
        PainterNode {
            params: self.clone(),
            delegates: crate::delegation::Delegates::new(),
            remeasure: false,
        }
    }

    fn update(&self, node: &mut PainterNode) {
        let intrinsics_changed = node.size_to_intrinsics != self.size_to_intrinsics
            || (self.size_to_intrinsics
                && !same_size(node.painter.intrinsic_size(), self.painter.intrinsic_size()));
        node.remeasure |= intrinsics_changed || node.content_scale != self.content_scale;
        node.params = self.clone();
        node.delegates.ask_for_context();
    }

    fn same(&self, other: &Self) -> bool {
        self.painter == other.painter
            && self.size_to_intrinsics == other.size_to_intrinsics
            && self.alignment == other.alignment
            && self.content_scale == other.content_scale
            && self.alpha == other.alpha
            && self.color_filter == other.color_filter
    }

    fn name(&self) -> &'static str {
        "paint"
    }

    fn inspectable_properties(&self, info: &mut InspectorInfo) {
        info.name = Some("paint");
        info.properties = vec![
            ("painter", format!("{:?}", self.painter)),
            ("sizeToIntrinsics", self.size_to_intrinsics.to_string()),
            ("alignment", format!("{:?}", self.alignment)),
            ("contentScale", format!("{:?}", self.content_scale)),
            ("alpha", self.alpha.to_string()),
            ("colorFilter", format!("{:?}", self.color_filter)),
        ];
    }
}

/// Whether a side of a size is specified and finite.
fn specified(side: f32) -> bool {
    side.is_finite()
}

impl PainterElement {
    fn uses_intrinsic_size(&self) -> bool {
        let size = self.painter.intrinsic_size();
        self.size_to_intrinsics && !(size.width.is_nan() && size.height.is_nan())
    }

    /// The painter's size scaled into `destination`, as Compose's `calculateScaledSize`.
    fn scaled_size(&self, destination: Size) -> Size {
        if !self.uses_intrinsic_size() {
            return destination;
        }
        let intrinsic = self.painter.intrinsic_size();
        let source = Size::new(
            if specified(intrinsic.width) { intrinsic.width } else { destination.width },
            if specified(intrinsic.height) { intrinsic.height } else { destination.height },
        );
        if destination.width == 0.0 || destination.height == 0.0 {
            return Size::ZERO;
        }
        let (x, y) = self.content_scale.scale_factor(source, destination);
        Size::new(source.width * x, source.height * y)
    }

    /// `constraints` with their minimum raised to the painter's scaled size, as Compose's
    /// `modifyConstraints`; tight to their maximum when bounded and not sized to intrinsics,
    /// or already fixed.
    fn modify_constraints(&self, constraints: Constraints) -> Constraints {
        let bounded = constraints.has_bounded_width() && constraints.has_bounded_height();
        let fixed = constraints.has_fixed_width() && constraints.has_fixed_height();
        if (!self.uses_intrinsic_size() && bounded) || fixed {
            return Constraints::new(
                constraints.max_width,
                constraints.max_width,
                constraints.max_height,
                constraints.max_height,
            );
        }
        let intrinsic = self.painter.intrinsic_size();
        let width = if specified(intrinsic.width) {
            intrinsic.width.round() as i32
        } else {
            constraints.min_width
        };
        let height = if specified(intrinsic.height) {
            intrinsic.height.round() as i32
        } else {
            constraints.min_height
        };
        let constrained = Size::new(
            constraints.constrain_width(width) as f32,
            constraints.constrain_height(height) as f32,
        );
        let scaled = self.scaled_size(constrained);
        Constraints::new(
            constraints.constrain_width(scaled.width.round() as i32),
            constraints.max_width,
            constraints.constrain_height(scaled.height.round() as i32),
            constraints.max_height,
        )
    }
}

/// Equal sizes, an unspecified (NaN) side equal to another.
fn same_size(a: Size, b: Size) -> bool {
    let side = |x: f32, y: f32| x == y || (x.is_nan() && y.is_nan());
    side(a.width, b.width) && side(a.height, b.height)
}

impl ModifierNode for PainterNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }

    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }

    fn delegates(&mut self) -> Option<&mut crate::delegation::Delegates> {
        Some(&mut self.delegates)
    }

    fn should_auto_invalidate(&self) -> bool {
        false
    }

    fn on_update_with_context(&mut self, context: &mut crate::node_context::NodeContext<'_>) {
        if std::mem::take(&mut self.remeasure) {
            context.invalidate_measurement();
        }
        context.invalidate_draw();
    }
}

impl LayoutModifierNode for PainterNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let placeable = scope.measure(content, self.modify_constraints(constraints));
        let (width, height) = (placeable.width, placeable.height);
        scope.place_relative(&placeable, 0, 0);
        scope.layout(width, height)
    }

    fn min_intrinsic_width(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        height: i32,
    ) -> i32 {
        let layout = scope.min_intrinsic_width(content, height);
        if !self.uses_intrinsic_size() {
            return layout;
        }
        let constraints =
            self.modify_constraints(Constraints::new(0, Constraints::INFINITY, 0, height));
        constraints.min_width.max(layout)
    }

    fn max_intrinsic_width(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        height: i32,
    ) -> i32 {
        let layout = scope.max_intrinsic_width(content, height);
        if !self.uses_intrinsic_size() {
            return layout;
        }
        let constraints =
            self.modify_constraints(Constraints::new(0, Constraints::INFINITY, 0, height));
        constraints.min_width.max(layout)
    }

    fn min_intrinsic_height(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        width: i32,
    ) -> i32 {
        let layout = scope.min_intrinsic_height(content, width);
        if !self.uses_intrinsic_size() {
            return layout;
        }
        let constraints =
            self.modify_constraints(Constraints::new(0, width, 0, Constraints::INFINITY));
        constraints.min_height.max(layout)
    }

    fn max_intrinsic_height(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        width: i32,
    ) -> i32 {
        let layout = scope.max_intrinsic_height(content, width);
        if !self.uses_intrinsic_size() {
            return layout;
        }
        let constraints =
            self.modify_constraints(Constraints::new(0, width, 0, Constraints::INFINITY));
        constraints.min_height.max(layout)
    }
}

impl DrawModifierNode for PainterNode {
    /// The painter at its intrinsic size scaled into the node, placed by the alignment, as
    /// Compose's `PainterNode.draw`; the content over it.
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        let size = scope.size();
        let intrinsic = self.painter.intrinsic_size();
        let source = Size::new(
            if specified(intrinsic.width) { intrinsic.width } else { size.width },
            if specified(intrinsic.height) { intrinsic.height } else { size.height },
        );
        let scaled = if size.width != 0.0 && size.height != 0.0 {
            let (x, y) = self.content_scale.scale_factor(source, size);
            Size::new(source.width * x, source.height * y)
        } else {
            Size::ZERO
        };
        let at = self.alignment.align(
            IntSize::new(scaled.width.round() as i32, scaled.height.round() as i32),
            IntSize::new(size.width.round() as i32, size.height.round() as i32),
            scope.layout_direction(),
        );
        let (painter, alpha, filter) = (&self.painter, self.alpha, self.color_filter);
        scope.translate(at.x as f32, at.y as f32, |scope| {
            painter.draw(scope, scaled, alpha, filter.as_ref());
        });
    }
}
