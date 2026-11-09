//! `Modifier.graphics_layer`, as Compose's `graphicsLayer`: alpha, scale, rotation, translation,
//! a shadow, a clip to a shape and a compositing strategy applied to a node's drawing, without
//! layout. Its block runs while drawing, so state it reads (an animation's value) changes the
//! drawing without recomposing or measuring.
//!
//! Each graphics layer is a group of the display list (a render node on Android), as each of
//! Compose's is a layer: a layer that is its node's first drawing modifier gives its properties
//! to its node's repaint boundary, whose group draws the node (render.rs); a write to what its
//! block read changes them without drawing the content again. One after a drawing modifier
//! (`background(…).clip(shape)`) is a group of its own inside its node's drawing, holding what
//! follows it in the chain and the children (an inner layer, D522). `Modifier.clip`,
//! `clip_to_bounds` and `shadow` are graphics layers, as in Compose (D523, D524).
//!
//! Hit testing follows a layer's clip but not its transform yet: a scaled or translated node
//! takes pointer events at its laid-out bounds.

use std::cell::Cell;
use std::rc::Rc;

use crate::alignment::LayoutDirection;
use crate::display_list::DrawOp;
use crate::draw::DrawScope;
use crate::graphics::{Color, Outline, Shape};
use crate::modifier::{DrawModifierNode, Modifier, ModifierNode, ModifierNodeElement};
use crate::unit::{Density, Dp, Offset, Rect, Size};

/// Where a layer scales and rotates around, as fractions of its size, as `TransformOrigin`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TransformOrigin {
    pub pivot_fraction_x: f32,
    pub pivot_fraction_y: f32,
}

impl TransformOrigin {
    pub const CENTER: TransformOrigin =
        TransformOrigin { pivot_fraction_x: 0.5, pivot_fraction_y: 0.5 };
}

/// How a graphics layer's content is composited into what's under it, as Compose's
/// `CompositingStrategy`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum CompositingStrategy {
    /// Offscreen only when needed: an alpha below 1 draws the content into an offscreen buffer,
    /// then composites it with that alpha, so overlapping drawings inside fade as one (Android:
    /// `setHasOverlappingRendering(true)` without a compositing layer, HWUI's `saveLayer`).
    #[default]
    Auto,
    /// Always drawn into an offscreen buffer first, at any alpha (Android: a compositing layer,
    /// `setUseCompositingLayer(true)`): for blending its content with what's under it as one.
    Offscreen,
    /// Never offscreen: the alpha multiplies each drawing's own, so overlapping drawings inside
    /// show through each other (Android: `setHasOverlappingRendering(false)`).
    ModulateAlpha,
}

/// What a layer's content goes through as it's composited, as Compose's `RenderEffect`: a blur
/// of `radius_x` by `radius_y` pixels, as `BlurEffect`, its edges read by `edge_treatment` (a
/// `TileMode`: `Clamp` repeats the layer's edge pixels into the blur, `Decal` blurs from
/// transparent, the blur reaching past the layer).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RenderEffect {
    Blur { radius_x: f32, radius_y: f32, edge_treatment: crate::brush::TileMode },
}

impl RenderEffect {
    /// As `BlurEffect(radiusX, radiusY, edgeTreatment)`.
    pub fn blur(
        radius_x: f32,
        radius_y: f32,
        edge_treatment: crate::brush::TileMode,
    ) -> RenderEffect {
        RenderEffect::Blur { radius_x, radius_y, edge_treatment }
    }
}

/// How a blur treats its content's edges, as Compose's `BlurredEdgeTreatment`: clipped to a
/// shape, its edge pixels repeated into the blur, or (`UNBOUNDED`) blurred from transparent past
/// the bounds.
#[derive(Clone, PartialEq, Debug)]
pub struct BlurredEdgeTreatment {
    pub shape: Option<Shape>,
}

impl BlurredEdgeTreatment {
    /// As `BlurredEdgeTreatment.Rectangle`.
    pub const RECTANGLE: BlurredEdgeTreatment =
        BlurredEdgeTreatment { shape: Some(Shape::Rectangle) };
    /// As `BlurredEdgeTreatment.Unbounded`.
    pub const UNBOUNDED: BlurredEdgeTreatment = BlurredEdgeTreatment { shape: None };

    /// Clipped to `shape`, as `BlurredEdgeTreatment(shape)`.
    pub fn new(shape: Shape) -> BlurredEdgeTreatment {
        BlurredEdgeTreatment { shape: Some(shape) }
    }

    fn tile_mode(&self) -> crate::brush::TileMode {
        if self.shape.is_some() {
            crate::brush::TileMode::Clamp
        } else {
            crate::brush::TileMode::Decal
        }
    }
}

/// The shadows' default color, as Compose's `DefaultShadowColor`.
pub const DEFAULT_SHADOW_COLOR: Color = Color::BLACK;

/// A layer's rarely set properties, out of line in [`GraphicsLayerScope::extras`] and
/// [`LayerProperties::extras`] (`None` for all their defaults), so a layer without them, as most
/// are, carries and copies a pointer's worth: its shadow colors and its render effect.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct LayerExtras {
    /// The ambient shadow's color (its alpha scaled by the platform's ambient shadow alpha).
    pub ambient_shadow_color: Color,
    /// The spot shadow's color (its alpha scaled by the platform's spot shadow alpha).
    pub spot_shadow_color: Color,
    /// What its content goes through as it's composited, as `renderEffect` (a blur).
    pub render_effect: Option<RenderEffect>,
}

impl LayerExtras {
    const DEFAULT: LayerExtras = LayerExtras {
        ambient_shadow_color: DEFAULT_SHADOW_COLOR,
        spot_shadow_color: DEFAULT_SHADOW_COLOR,
        render_effect: None,
    };

    /// `extras`, changed by `change`: `None` again if it's all defaults.
    fn update(extras: &mut Option<Box<LayerExtras>>, change: impl FnOnce(&mut LayerExtras)) {
        let mut changed = extras.take().unwrap_or_else(|| Box::new(LayerExtras::DEFAULT));
        change(&mut changed);
        *extras = (*changed != LayerExtras::DEFAULT).then_some(changed);
    }
}

impl Default for LayerExtras {
    fn default() -> Self {
        LayerExtras::DEFAULT
    }
}

/// A copy of `extras`: nothing to do for the usual none, so the clone of a layer's properties,
/// made for each group each frame, stays a copy inlined where it's made (`Box`'s clone, an
/// allocation, kept out of line).
#[inline(always)]
fn clone_extras(extras: &Option<Box<LayerExtras>>) -> Option<Box<LayerExtras>> {
    #[cold]
    #[inline(never)]
    fn boxed(extras: &LayerExtras) -> Box<LayerExtras> {
        Box::new(*extras)
    }
    extras.as_deref().map(boxed)
}

/// Reading a layer's extras, their defaults when it has none.
macro_rules! extras_getters {
    () => {
        /// The ambient shadow's color: [`DEFAULT_SHADOW_COLOR`] unless set.
        pub fn ambient_shadow_color(&self) -> Color {
            self.extras.as_ref().map_or(DEFAULT_SHADOW_COLOR, |extras| extras.ambient_shadow_color)
        }

        /// The spot shadow's color: [`DEFAULT_SHADOW_COLOR`] unless set.
        pub fn spot_shadow_color(&self) -> Color {
            self.extras.as_ref().map_or(DEFAULT_SHADOW_COLOR, |extras| extras.spot_shadow_color)
        }

        /// Its render effect, if any.
        pub fn render_effect(&self) -> Option<RenderEffect> {
            self.extras.as_ref().and_then(|extras| extras.render_effect)
        }
    };
}

/// The properties a graphics layer applies, as Compose's `GraphicsLayerScope`.
#[derive(PartialEq, Debug)]
pub struct GraphicsLayerScope {
    pub alpha: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    /// Degrees, clockwise.
    pub rotation_z: f32,
    /// Pixels.
    pub translation_x: f32,
    pub translation_y: f32,
    pub transform_origin: TransformOrigin,
    /// The shadow's elevation in pixels (`density.to_px(…)` of a `Dp`): with it above 0, the
    /// shadow of `shape` is drawn under the layer, as Compose's `shadowElevation` (an Android
    /// elevation shadow, lit by the window's light).
    pub shadow_elevation: f32,
    /// The layer's outline, for its shadow and its clip.
    pub shape: Shape,
    /// Clips the content to `shape`.
    pub clip: bool,
    pub compositing_strategy: CompositingStrategy,
    /// Its shadow colors and render effect, `None` for their defaults: set them with
    /// [`set_ambient_shadow_color`](GraphicsLayerScope::set_ambient_shadow_color),
    /// [`set_spot_shadow_color`](GraphicsLayerScope::set_spot_shadow_color) and
    /// [`set_render_effect`](GraphicsLayerScope::set_render_effect).
    pub extras: Option<Box<LayerExtras>>,
    /// The layer's size, set before its block runs (as `GraphicsLayerScope.size`).
    pub size: Size,
    /// The density, set before its block runs (Compose's scope is a `Density`).
    pub density: Density,
}

impl Default for GraphicsLayerScope {
    fn default() -> GraphicsLayerScope {
        GraphicsLayerScope {
            alpha: 1.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_z: 0.0,
            translation_x: 0.0,
            translation_y: 0.0,
            transform_origin: TransformOrigin::CENTER,
            shadow_elevation: 0.0,
            shape: Shape::Rectangle,
            clip: false,
            compositing_strategy: CompositingStrategy::Auto,
            extras: None,
            size: Size::ZERO,
            density: Density::default(),
        }
    }
}

impl Clone for GraphicsLayerScope {
    #[inline]
    fn clone(&self) -> Self {
        GraphicsLayerScope {
            shape: self.shape.clone(),
            extras: clone_extras(&self.extras),
            ..*self
        }
    }
}

impl GraphicsLayerScope {
    extras_getters!();

    /// Sets the ambient shadow's color, as `ambientShadowColor`.
    pub fn set_ambient_shadow_color(&mut self, color: Color) {
        LayerExtras::update(&mut self.extras, |extras| extras.ambient_shadow_color = color);
    }

    /// Sets the spot shadow's color, as `spotShadowColor`.
    pub fn set_spot_shadow_color(&mut self, color: Color) {
        LayerExtras::update(&mut self.extras, |extras| extras.spot_shadow_color = color);
    }

    /// Sets what its content goes through as it's composited, as `renderEffect` (a blur).
    pub fn set_render_effect(&mut self, effect: Option<RenderEffect>) {
        LayerExtras::update(&mut self.extras, |extras| extras.render_effect = effect);
    }
}

/// A shadow's layer, as Compose's `ShadowGraphicsLayerElement`: its elevation in dp, made pixels
/// at the layer's density.
#[derive(Clone, PartialEq, Debug)]
struct ShadowSpec {
    elevation: Dp,
    shape: Shape,
    clip: bool,
    ambient_color: Color,
    spot_color: Color,
}

impl ShadowSpec {
    /// Its colors as a layer's extras: none for the default ones.
    fn extras(&self) -> Option<Box<LayerExtras>> {
        let extras = LayerExtras {
            ambient_shadow_color: self.ambient_color,
            spot_shadow_color: self.spot_color,
            render_effect: None,
        };
        (extras != LayerExtras::DEFAULT).then(|| Box::new(extras))
    }
}

#[derive(Clone)]
enum Layer {
    Fixed(GraphicsLayerScope),
    Block(Rc<dyn Fn(&mut GraphicsLayerScope)>),
    Shadow(ShadowSpec),
}

struct GraphicsLayerElement(Layer);

pub(crate) struct GraphicsLayerNode {
    layer: Layer,
    /// Its last path outline, with what it was made for (D1059).
    outline: std::cell::RefCell<Option<CachedOutline>>,
    /// How it transformed when a node under it last asked where it is, as Compose's
    /// `layerPositionalProperties`, with `UiTree::position_listeners` then; `None` if none did.
    positional: Cell<Option<(u32, LayerTransform)>>,
    /// Its alpha was none when last drawn.
    transparent: Cell<bool>,
    /// What its block last gave, of what coordinates and hit testing ask: they read this, as
    /// Compose's read the render node's matrix, and don't run the block (D1214).
    ran: Cell<Option<LayerRun>>,
    /// The shape its block last gave with a clip.
    ran_shape: std::cell::RefCell<Shape>,
    /// The outline it last drew with, in its own coordinates: what a press tests against.
    last_outline: std::cell::RefCell<Option<Outline>>,
}

/// What moves or clips what's in a layer, as a layer's block last gave it.
#[derive(Clone, Copy, PartialEq)]
struct LayerRun {
    scale: (f32, f32),
    rotation: f32,
    translation: Offset,
    pivot_fraction: (f32, f32),
    clip: bool,
}

impl LayerRun {
    const NONE: LayerRun = LayerRun {
        scale: (1.0, 1.0),
        rotation: 0.0,
        translation: Offset::ZERO,
        pivot_fraction: (0.5, 0.5),
        clip: false,
    };

    fn of(layer: &GraphicsLayerScope) -> LayerRun {
        LayerRun {
            scale: (layer.scale_x, layer.scale_y),
            rotation: layer.rotation_z,
            translation: Offset::new(layer.translation_x, layer.translation_y),
            pivot_fraction: (
                layer.transform_origin.pivot_fraction_x,
                layer.transform_origin.pivot_fraction_y,
            ),
            clip: layer.clip,
        }
    }
}

/// How a layer moves points of the node's first layer's coordinates, as its matrix does.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct LayerMap {
    origin: Offset,
    pivot: Offset,
    scale: (f32, f32),
    rotation: f32,
    translation: Offset,
    clip: bool,
}

impl LayerMap {
    /// Where it draws `point`, as `mapOffset`.
    pub(crate) fn map(&self, point: Offset) -> Offset {
        let (x, y) =
            ((point.x - self.pivot.x) * self.scale.0, (point.y - self.pivot.y) * self.scale.1);
        let (sin, cos) = self.rotation.to_radians().sin_cos();
        Offset::new(
            self.pivot.x + self.translation.x + x * cos - y * sin,
            self.pivot.y + self.translation.y + x * sin + y * cos,
        )
    }

    /// Whether it can be undone: not with a scale of 0.
    pub(crate) fn invertible(&self) -> bool {
        self.scale.0 != 0.0 && self.scale.1 != 0.0
    }

    /// The inverse of `map`, as `mapOffset(inverse = true)`: infinite where it can't be undone,
    /// as Compose's `Offset.Infinite`.
    pub(crate) fn unmap(&self, point: Offset) -> Offset {
        if !self.invertible() {
            return Offset::new(f32::INFINITY, f32::INFINITY);
        }
        let (x, y) = (
            point.x - self.pivot.x - self.translation.x,
            point.y - self.pivot.y - self.translation.y,
        );
        let (sin, cos) = self.rotation.to_radians().sin_cos();
        Offset::new(
            self.pivot.x + (x * cos + y * sin) / self.scale.0,
            self.pivot.y + (y * cos - x * sin) / self.scale.1,
        )
    }

    /// What `map` does, as a matrix.
    pub(crate) fn matrix(&self) -> crate::path::Matrix {
        use crate::path::Matrix;
        let mut matrix = Matrix::translation(0.0 - self.pivot.x, 0.0 - self.pivot.y);
        matrix.post_concat(&Matrix::scaling(self.scale.0, self.scale.1));
        matrix.post_concat(&Matrix::rotation(self.rotation));
        matrix.post_concat(&Matrix::translation(
            self.pivot.x + self.translation.x,
            self.pivot.y + self.translation.y,
        ));
        matrix
    }
}

/// What of a layer moves what's in it, as Compose's `LayerPositionalProperties`.
#[derive(Clone, Copy, PartialEq)]
struct LayerTransform {
    pivot: Offset,
    scale: (f32, f32),
    rotation: f32,
    translation: Offset,
    /// Not a move, but what's under it is clipped otherwise: its bounds change.
    clip: bool,
}

impl LayerTransform {
    fn of(properties: &LayerProperties) -> LayerTransform {
        LayerTransform {
            // Around its own top left: its group's place isn't what it does.
            pivot: Offset::new(
                properties.pivot.x - properties.bounds.left,
                properties.pivot.y - properties.bounds.top,
            ),
            scale: properties.scale,
            rotation: properties.rotation,
            translation: properties.translation,
            clip: properties.clip,
        }
    }
}

/// A built-in shape's path outline and what it was made for: made again only when one of them
/// changes, so an animation of a layer's other properties (an alpha over a cut shape) keeps the
/// same path, which Android doesn't define again each frame (D1059). compose-rs's own: Compose
/// makes its outline on every layer update, and a custom shape's here too (it may read state,
/// D1063).
struct CachedOutline {
    shape: Shape,
    size: Size,
    direction: LayoutDirection,
    density: Density,
    outline: Outline,
}

/// A placement's layer block, as `placeWithLayer`'s `layerBlock`: made from any closure, or kept
/// and given again (a clone) to keep its identity, so placing again with it doesn't run it again,
/// as Compose compares its blocks by identity (D1909).
#[derive(Clone)]
pub struct LayerBlock(Rc<dyn Fn(&mut GraphicsLayerScope)>);

impl LayerBlock {
    /// The block `block`.
    pub fn new(block: impl Fn(&mut GraphicsLayerScope) + 'static) -> LayerBlock {
        LayerBlock(Rc::new(block))
    }

    /// The block giving `layer`'s properties, as `placeWithLayer(layer)`: its transform, alpha,
    /// clip to its bounds, shadow and compositing strategy, read where the placement's layer
    /// computes its properties, so a change to them computes them again. The same block for the
    /// same layer.
    pub fn of_graphics_layer(layer: &crate::graphics_layer_object::GraphicsLayer) -> LayerBlock {
        layer.placement_block()
    }

    /// Whether two blocks are the same function object.
    pub fn ptr_eq(&self, other: &LayerBlock) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl<F: Fn(&mut GraphicsLayerScope) + 'static> From<F> for LayerBlock {
    fn from(block: F) -> LayerBlock {
        LayerBlock::new(block)
    }
}

impl std::fmt::Debug for LayerBlock {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "LayerBlock({:p})", Rc::as_ptr(&self.0).cast::<()>())
    }
}

impl GraphicsLayerNode {
    /// `Modifier.clip(shape)`'s layer (a built-in element's node).
    pub(crate) fn clip(shape: Shape) -> GraphicsLayerNode {
        GraphicsLayerNode {
            layer: Layer::Fixed(GraphicsLayerScope { shape, clip: true, ..Default::default() }),
            outline: Default::default(),
            positional: Cell::new(None),
            transparent: Cell::new(false),
            ran: Cell::new(None),
            ran_shape: std::cell::RefCell::new(Shape::Rectangle),
            last_outline: Default::default(),
        }
    }

    /// A placement's layer, running `block` (D1909). Each constructor builds its node in place:
    /// a `Layer` passed by value to a shared one was a copy in each `clip` and element's node.
    pub(crate) fn placed(block: LayerBlock) -> GraphicsLayerNode {
        GraphicsLayerNode {
            layer: Layer::Block(block.0),
            outline: Default::default(),
            positional: Cell::new(None),
            transparent: Cell::new(false),
            ran: Cell::new(None),
            ran_shape: std::cell::RefCell::new(Shape::Rectangle),
            last_outline: Default::default(),
        }
    }

    /// Whether it runs `block`.
    pub(crate) fn runs(&self, block: &LayerBlock) -> bool {
        matches!(&self.layer, Layer::Block(own) if Rc::ptr_eq(own, &block.0))
    }

    /// Runs `block` from now on: what it gave last, and what a node under it was told, stay
    /// until it runs. Returns the block it ran.
    pub(crate) fn replace_block(&mut self, block: LayerBlock) -> Option<LayerBlock> {
        match std::mem::replace(&mut self.layer, Layer::Block(block.0)) {
            Layer::Block(old) => Some(LayerBlock(old)),
            _ => None,
        }
    }

    /// Its properties for a layer of `size` at `density`: its block's, run now (reading state
    /// the caller observes).
    pub(crate) fn scope(&self, density: Density, size: Size) -> GraphicsLayerScope {
        match &self.layer {
            Layer::Fixed(layer) => GraphicsLayerScope { size, density, ..layer.clone() },
            Layer::Block(block) => {
                let mut layer = GraphicsLayerScope { size, density, ..Default::default() };
                block(&mut layer);
                self.ran.set(Some(LayerRun::of(&layer)));
                if layer.clip {
                    self.ran_shape.replace(layer.shape.clone());
                }
                layer
            }
            Layer::Shadow(shadow) => GraphicsLayerScope {
                shadow_elevation: density.to_px(shadow.elevation),
                extras: shadow.extras(),
                shape: shadow.shape.clone(),
                clip: shadow.clip,
                size,
                density,
                ..Default::default()
            },
        }
    }

    /// Keeps whether `properties` leave it with no alpha: whether that changed, which hides what's
    /// under it from a service or shows it, so the semantics changed (D1293).
    pub(crate) fn note_transparency(&self, properties: &LayerProperties) -> bool {
        let transparent = properties.alpha <= 0.0;
        self.transparent.replace(transparent) != transparent
    }

    /// A block no frame drew runs for a press, once: queries find the tree busy there.
    fn run_if_never_ran(&self, size: Size, density: Density) {
        if self.ran.get().is_none() {
            let _ = self.scope(density, size);
        }
    }

    /// What a press is clipped by here: the outline it last drew with.
    pub(crate) fn hit_outline(
        &self,
        size: Size,
        density: Density,
        direction: LayoutDirection,
    ) -> Option<Outline> {
        if let Layer::Block(_) = &self.layer {
            self.run_if_never_ran(size, density);
        }
        if !self.clips() {
            return None;
        }
        if let Some(outline) = &*self.last_outline.borrow() {
            return Some(outline.clone());
        }
        let shape = match &self.layer {
            Layer::Fixed(layer) => layer.shape.clone(),
            Layer::Block(_) => self.ran_shape.borrow().clone(),
            Layer::Shadow(shadow) => shadow.shape.clone(),
        };
        Some(shape.outline_in(size, &density, direction))
    }

    /// Whether it has a block: application code, that only passes that draw or look ahead run.
    pub(crate) fn has_block(&self) -> bool {
        matches!(self.layer, Layer::Block(_))
    }

    /// What it last gave, for a layer of `size` at `origin` in its node: what a query asks. A
    /// block that hasn't run moves nothing yet.
    fn last_run(&self) -> LayerRun {
        match &self.layer {
            Layer::Fixed(layer) => LayerRun::of(layer),
            Layer::Block(_) => self.ran.get().unwrap_or(LayerRun::NONE),
            Layer::Shadow(shadow) => LayerRun { clip: shadow.clip, ..LayerRun::NONE },
        }
    }

    /// How it moves points, as a query reads it: `last_run`'s, no block running.
    pub(crate) fn layer_map(&self, origin: Offset, size: Size) -> LayerMap {
        let run = self.last_run();
        LayerMap {
            origin,
            pivot: Offset::new(
                origin.x + size.width * run.pivot_fraction.0,
                origin.y + size.height * run.pivot_fraction.1,
            ),
            scale: run.scale,
            rotation: run.rotation,
            translation: run.translation,
            clip: run.clip,
        }
    }

    /// Whether it clips what's in it, as a query reads it.
    pub(crate) fn clips(&self) -> bool {
        self.last_run().clip
    }

    /// Keeps how `map` transforms, which a node under it was just told.
    pub(crate) fn note_transform(&self, map: &LayerMap, listeners: u32) {
        let transform = LayerTransform {
            pivot: Offset::new(map.pivot.x - map.origin.x, map.pivot.y - map.origin.y),
            scale: map.scale,
            rotation: map.rotation,
            translation: map.translation,
            clip: map.clip,
        };
        self.positional.set(Some((listeners, transform)));
    }

    /// Whether a node under it was told where it is, and may still listen.
    pub(crate) fn told_a_node(&self, listeners: u32) -> bool {
        self.positional.get().is_some_and(|(kept, _)| kept == listeners)
    }

    /// Whether `properties` transform differently from what a node under it was last told. Its
    /// other properties (an alpha) move nothing, as `onCommitAffectingLayerParams`.
    pub(crate) fn transform_changed(&self, properties: &LayerProperties) -> bool {
        self.positional.get().map(|(_, transform)| transform)
            != Some(LayerTransform::of(properties))
    }

    /// Its properties for a layer of `size` at `origin` (in its group), its outline laid out
    /// in `direction`.
    pub(crate) fn properties(
        &self,
        density: Density,
        direction: LayoutDirection,
        origin: Offset,
        size: Size,
    ) -> LayerProperties {
        let layer = self.scope(density, size);
        LayerProperties::new(&layer, origin, size, || {
            let mut cache = self.outline.borrow_mut();
            if let Some(cached) = &*cache
                && cached.shape == layer.shape
                && cached.size == size
                && cached.direction == direction
                && cached.density == layer.density
            {
                self.last_outline.replace(Some(cached.outline.clone()));
                return cached.outline.clone();
            }
            let outline = layer.shape.outline_in(size, &layer.density, direction);
            // Paths alone: the other outlines are values, as cheap to make again as to compare.
            // Built-in shapes alone: a custom shape's outline may read state, which must be read
            // each time to be observed, as Compose's `createOutline` runs in `observeReads`.
            let keeps =
                matches!(outline, Outline::Generic(..)) && !matches!(layer.shape, Shape::Custom(_));
            *cache = keeps.then(|| CachedOutline {
                shape: layer.shape.clone(),
                size,
                direction,
                density: layer.density,
                outline: outline.clone(),
            });
            self.last_outline.replace(Some(outline.clone()));
            outline
        })
    }
}

/// A graphics layer's properties as its group carries them
/// ([`DrawGroup::layer`](crate::platform::DrawGroup)), as a render node's: in its group's
/// coordinates.
#[derive(PartialEq, Debug)]
pub struct LayerProperties {
    /// In 0..=1.
    pub alpha: f32,
    /// Where it scales and rotates around.
    pub pivot: Offset,
    pub scale: (f32, f32),
    /// Degrees, clockwise.
    pub rotation: f32,
    pub translation: Offset,
    /// It clips its content to its outline (after its transform).
    pub clip: bool,
    /// Its shape's outline, if it clips or casts a shadow (as Compose gives a render node an
    /// outline only then).
    pub outline: Option<Outline>,
    /// Pixels; a shadow of its outline is drawn under it above 0.
    pub shadow_elevation: f32,
    pub compositing_strategy: CompositingStrategy,
    /// Its bounds: its size at its origin (a render node's position; an offscreen buffer for
    /// its alpha is bounded by them on Android, as Compose's).
    pub bounds: Rect,
    /// Pixels per dp: desktop's shadow light is placed in dp (`operations`).
    pub density: f32,
    /// Its layer's shadow colors and render effect, `None` for their defaults.
    pub extras: Option<Box<LayerExtras>>,
}

impl Clone for LayerProperties {
    #[inline(always)]
    fn clone(&self) -> Self {
        LayerProperties {
            outline: self.outline.clone(),
            extras: clone_extras(&self.extras),
            ..*self
        }
    }
}

impl LayerProperties {
    /// `layer`'s properties for a layer at `origin` in its group, of `size`, its outline (in
    /// the layer's own coordinates) from `outline`, asked only if it clips or casts a shadow.
    pub(crate) fn new(
        layer: &GraphicsLayerScope,
        origin: Offset,
        size: Size,
        outline: impl FnOnce() -> Outline,
    ) -> LayerProperties {
        let pivot = Offset::new(
            size.width * layer.transform_origin.pivot_fraction_x,
            size.height * layer.transform_origin.pivot_fraction_y,
        );
        let bounds = Rect::new(origin.x, origin.y, origin.x + size.width, origin.y + size.height);
        let shadow_elevation = layer.shadow_elevation.max(0.0);
        let outline = (layer.clip || shadow_elevation > 0.0).then(|| outline().translate(origin));
        LayerProperties {
            alpha: layer.alpha.clamp(0.0, 1.0),
            pivot: pivot + origin,
            scale: (layer.scale_x, layer.scale_y),
            rotation: layer.rotation_z,
            translation: Offset::new(layer.translation_x, layer.translation_y),
            clip: layer.clip,
            outline,
            shadow_elevation,
            compositing_strategy: layer.compositing_strategy,
            bounds,
            density: layer.density.density,
            extras: clone_extras(&layer.extras),
        }
    }

    extras_getters!();

    fn transforms(&self) -> bool {
        self.scale != (1.0, 1.0) || self.rotation != 0.0 || self.translation != Offset::ZERO
    }

    /// Whether it casts a shadow: an elevation above 0 and an outline.
    pub fn casts_shadow(&self) -> bool {
        self.shadow_elevation > 0.0 && self.outline.is_some()
    }

    /// Whether its content is drawn into an offscreen buffer, composited with its alpha: always
    /// under `Offscreen`, under `Auto` for an alpha below 1, never under `ModulateAlpha`.
    pub fn offscreen(&self) -> bool {
        match self.compositing_strategy {
            CompositingStrategy::Auto => self.alpha < 1.0,
            CompositingStrategy::Offscreen => true,
            CompositingStrategy::ModulateAlpha => false,
        }
    }

    /// The alpha a renderer multiplies each of its drawings' by, under `ModulateAlpha` (an
    /// alpha below 1): what [`operations`](LayerProperties::operations) leaves out.
    pub fn modulated_alpha(&self) -> Option<f32> {
        (self.compositing_strategy == CompositingStrategy::ModulateAlpha && self.alpha < 1.0)
            .then_some(self.alpha)
    }

    /// The operations applying it to what follows (before a group's content, a flattened
    /// group's `layer_start..start`), in this order and each only if needed: `Save` (if it
    /// transforms, clips or casts a shadow), `Transform`, `DrawShadow` (its outline's shadow,
    /// lit as Compose Desktop's), the clip to its outline (`ClipRect`, `ClipRoundRect`) and
    /// `SaveLayerAlpha` (if [`offscreen`](LayerProperties::offscreen)).
    /// [`restores`](LayerProperties::restores) restores end them. A `ModulateAlpha` alpha is
    /// left to the renderer ([`modulated_alpha`](LayerProperties::modulated_alpha)).
    pub fn operations(&self, mut push: impl FnMut(DrawOp)) {
        let transforms = self.transforms();
        let shadow = self.casts_shadow();
        if transforms || self.clip || shadow {
            push(DrawOp::Save);
        }
        if transforms {
            push(DrawOp::Transform {
                pivot: self.pivot,
                scale: self.scale,
                rotation: self.rotation,
                translation: self.translation,
            });
        }
        if shadow && let Some(outline) = &self.outline {
            let px = |dp: f32| dp * self.density;
            push(DrawOp::DrawShadow {
                outline: outline.clone(),
                elevation: self.shadow_elevation,
                light: (0.0, -px(300.0), px(600.0)),
                light_radius: px(800.0),
                // Compose Desktop's alphas times the layer's and the colors' (Skiko's
                // `RenderNode::drawShadow`).
                ambient: faded(self.ambient_shadow_color(), 0.039 * self.alpha),
                spot: faded(self.spot_shadow_color(), 0.19 * self.alpha),
                transparent_occluder: self.alpha < 1.0,
            });
        }
        if self.clip
            && let Some(outline) = &self.outline
        {
            push(match outline {
                Outline::Rectangle(rect) => DrawOp::ClipRect(*rect),
                Outline::Rounded(rect, radii) => DrawOp::ClipRoundRect(*rect, *radii),
                Outline::Generic(path, offset) => {
                    DrawOp::ClipPath { path: std::sync::Arc::clone(path), offset: *offset }
                }
            });
        }
        if let Some(effect) = self.render_effect() {
            push(DrawOp::SaveLayerEffect { bounds: self.bounds, effect });
        }
        if self.offscreen() {
            push(DrawOp::SaveLayerAlpha { bounds: self.bounds, alpha: self.alpha });
        }
    }

    /// How many restores end its [`operations`](LayerProperties::operations).
    pub fn restores(&self) -> u8 {
        u8::from(self.transforms() || self.clip || self.casts_shadow())
            + u8::from(self.offscreen())
            + u8::from(self.render_effect().is_some())
    }
}

/// `color` with its alpha multiplied by `alpha`.
fn faded(color: Color, alpha: f32) -> Color {
    color.with_alpha(color.alpha() * alpha)
}

impl ModifierNodeElement for GraphicsLayerElement {
    type Node = GraphicsLayerNode;
    fn create(&self) -> GraphicsLayerNode {
        GraphicsLayerNode {
            layer: self.0.clone(),
            outline: Default::default(),
            positional: Cell::new(None),
            transparent: Cell::new(false),
            ran: Cell::new(None),
            ran_shape: std::cell::RefCell::new(Shape::Rectangle),
            last_outline: Default::default(),
        }
    }

    fn update(&self, node: &mut GraphicsLayerNode) {
        node.layer = self.0.clone();
    }

    fn same(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Layer::Fixed(a), Layer::Fixed(b)) => a == b,
            (Layer::Block(a), Layer::Block(b)) => Rc::ptr_eq(a, b),
            (Layer::Shadow(a), Layer::Shadow(b)) => a == b,
            _ => false,
        }
    }

    fn name(&self) -> &'static str {
        match self.0 {
            Layer::Shadow(_) => "shadow",
            _ => "graphics_layer",
        }
    }
}

impl ModifierNode for GraphicsLayerNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn reads_state(&self) -> bool {
        match &self.layer {
            Layer::Block(_) => true,
            Layer::Fixed(layer) => matches!(layer.shape, Shape::Custom(_)),
            Layer::Shadow(shadow) => matches!(shadow.shape, Shape::Custom(_)),
        }
    }
    /// Its clip, for hit testing, as Compose's layer's `isInLayer`: what's outside its shape
    /// isn't hit (its block run for the layer's size and density).
    /// As a query's: its last block's, which a press doesn't run. One no frame drew runs now.
    fn hit_clip_at(&self, size: Size, density: Density) -> Option<Shape> {
        match &self.layer {
            Layer::Fixed(layer) => layer.clip.then(|| layer.shape.clone()),
            Layer::Block(_) => {
                self.run_if_never_ran(size, density);
                self.clips().then(|| self.ran_shape.borrow().clone())
            }
            Layer::Shadow(shadow) => shadow.clip.then(|| shadow.shape.clone()),
        }
    }
}

/// `Modifier.repaint_boundary`'s element.
struct RepaintBoundaryElement;

pub(crate) struct RepaintBoundaryNode;

impl crate::modifier::ModifierNodeElement for RepaintBoundaryElement {
    type Node = RepaintBoundaryNode;
    fn create(&self) -> RepaintBoundaryNode {
        RepaintBoundaryNode
    }
    fn update(&self, _node: &mut RepaintBoundaryNode) {}
    fn same(&self, _other: &Self) -> bool {
        true
    }
    fn name(&self) -> &'static str {
        "repaint_boundary"
    }
}

impl ModifierNode for RepaintBoundaryNode {
    fn is_repaint_boundary(&self) -> bool {
        true
    }
    fn reads_state(&self) -> bool {
        false
    }
}

impl Modifier {
    /// Makes what this node draws a group a renderer can keep while it doesn't change (a
    /// render node on Android), as a Compose `graphicsLayer { }` without properties: for
    /// content that often stays the same while what's around it changes (lazy items).
    pub fn repaint_boundary(self) -> Modifier {
        self.then_custom(RepaintBoundaryElement)
    }
}

/// A graphics layer's drawing is its group's (render.rs): the tree draws it, not these.
impl DrawModifierNode for GraphicsLayerNode {
    fn draw(&mut self, _scope: &mut DrawScope<'_>) {}
}

impl Modifier {
    /// Applies `block`'s layer properties to the drawing, as `Modifier.graphicsLayer { … }`.
    /// The block runs while drawing: state it reads changes the layer's properties only (or, after
    /// a drawing modifier, draws its node again). The block isn't
    /// memoized: build the modifier in a `remember` (or pass fixed values to
    /// [`graphics_layer_with`](Modifier::graphics_layer_with)) so a callee given it can skip.
    pub fn graphics_layer(self, block: impl Fn(&mut GraphicsLayerScope) + 'static) -> Modifier {
        self.then_custom(GraphicsLayerElement(Layer::Block(Rc::new(block))))
    }

    /// Fixed layer properties, as `Modifier.graphicsLayer(scaleX = …, alpha = …)`.
    pub fn graphics_layer_with(self, layer: GraphicsLayerScope) -> Modifier {
        self.then_custom(GraphicsLayerElement(Layer::Fixed(layer)))
    }

    /// Draws the content with `alpha` opacity, as `Modifier.alpha`: a layer clipping to its
    /// bounds, none at 1, as Compose's (`graphicsLayer(alpha = alpha, clip = true)`).
    /// Blurs the content by `radius` both ways, its edges a rectangle's, as `Modifier.blur(radius)`.
    pub fn blur(self, radius: crate::unit::Dp) -> Modifier {
        self.blur_with(radius, radius, BlurredEdgeTreatment::RECTANGLE)
    }

    /// Blurs the content by `radius_x` and `radius_y`, its edges by `edge_treatment`, as
    /// `Modifier.blur(radiusX, radiusY, edgeTreatment)`: a graphics layer with a blur effect,
    /// clipped to the treatment's shape (its edge pixels repeated into the blur) or not
    /// (blurring from transparent, past the bounds). For a radius that isn't above 0, no blur:
    /// only the clip to the shape, if any, as Compose's (none at all without one).
    pub fn blur_with(
        self,
        radius_x: crate::unit::Dp,
        radius_y: crate::unit::Dp,
        edge_treatment: BlurredEdgeTreatment,
    ) -> Modifier {
        if !(radius_x.0 > 0.0 && radius_y.0 > 0.0) && edge_treatment.shape.is_none() {
            return self;
        }
        let tile_mode = edge_treatment.tile_mode();
        self.graphics_layer(move |layer| {
            let (x, y) = (layer.density.to_px(radius_x), layer.density.to_px(radius_y));
            layer.set_render_effect(
                (x > 0.0 && y > 0.0).then(|| RenderEffect::blur(x, y, tile_mode)),
            );
            layer.clip = edge_treatment.shape.is_some();
            layer.shape = edge_treatment.shape.clone().unwrap_or(Shape::Rectangle);
        })
    }

    pub fn alpha(self, alpha: f32) -> Modifier {
        if alpha == 1.0 {
            return self;
        }
        self.graphics_layer_with(GraphicsLayerScope {
            alpha,
            clip: true,
            ..GraphicsLayerScope::default()
        })
    }

    /// Scales the drawing around its center, as `Modifier.scale`: no layer at 1.
    pub fn scale(self, scale: f32) -> Modifier {
        if scale == 1.0 {
            return self;
        }
        self.graphics_layer_with(GraphicsLayerScope {
            scale_x: scale,
            scale_y: scale,
            ..GraphicsLayerScope::default()
        })
    }

    /// Rotates the drawing around its center (degrees, clockwise), as `Modifier.rotate`: no
    /// layer at 0.
    pub fn rotate(self, degrees: f32) -> Modifier {
        if degrees == 0.0 {
            return self;
        }
        self.graphics_layer_with(GraphicsLayerScope {
            rotation_z: degrees,
            ..GraphicsLayerScope::default()
        })
    }

    /// A layer casting the shadow of a rectangle raised by `elevation`, clipping its content
    /// to it if there is an elevation, as `Modifier.shadow(elevation)`.
    pub fn shadow(self, elevation: Dp) -> Modifier {
        let clip = elevation.0 > 0.0;
        self.shadow_with(
            elevation,
            Shape::Rectangle,
            clip,
            DEFAULT_SHADOW_COLOR,
            DEFAULT_SHADOW_COLOR,
        )
    }

    /// A graphics layer casting the shadow of `shape` raised by `elevation`, clipping its
    /// content to `shape` if `clip`, as `Modifier.shadow(elevation, shape, clip, ambientColor,
    /// spotColor)`: no layer without an elevation or a clip.
    pub fn shadow_with(
        self,
        elevation: Dp,
        shape: Shape,
        clip: bool,
        ambient_color: Color,
        spot_color: Color,
    ) -> Modifier {
        if elevation.0 > 0.0 || clip {
            self.then_custom(GraphicsLayerElement(Layer::Shadow(ShadowSpec {
                elevation,
                shape,
                clip,
                ambient_color,
                spot_color,
            })))
        } else {
            self
        }
    }
}
