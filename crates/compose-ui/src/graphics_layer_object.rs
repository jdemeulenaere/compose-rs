//! A graphics layer as an object, as Compose's `androidx.compose.ui.graphics.layer.GraphicsLayer`:
//! a recording not tied to a layout node, with its own transform, alpha, clip, shadow, color
//! filter and blend mode, drawn where a drawing asks ([`DrawScope::draw_layer`]), any number of
//! times.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use smallvec::SmallVec;

use crate::color_filter::{BlendMode, ColorFilter};
use crate::display_list::{Boundary, DisplayList, DrawOp};
use crate::draw::DrawScope;
use crate::draw_modifiers::ContentDrawScope;
use crate::graphics::{Color, Outline};
use crate::graphics_layer::{CompositingStrategy, LayerProperties};
use crate::unit::{IntOffset, IntSize, Offset, Rect, Size};

/// A layer's properties, Compose's defaults: opaque, untransformed, unclipped, no shadow,
/// blending `SrcOver`, unfiltered, its pivot its center.
#[derive(Clone, PartialEq, Debug)]
struct Properties {
    top_left: IntOffset,
    pivot_offset: Offset,
    alpha: f32,
    scale_x: f32,
    scale_y: f32,
    translation_x: f32,
    translation_y: f32,
    rotation_z: f32,
    shadow_elevation: f32,
    ambient_shadow_color: Color,
    spot_shadow_color: Color,
    blend_mode: BlendMode,
    color_filter: Option<ColorFilter>,
    clip: bool,
    compositing_strategy: CompositingStrategy,
    /// Set by `set_*_outline`; else its bounds.
    outline: Option<Outline>,
}

/// Parts of a layer, a bit each: its content (recording, origin, size, density, release) and
/// each property. A drawing that set one itself before drawing the layer isn't told of its
/// changes (D1337).
type Fields = u32;

mod field {
    use super::Fields;
    pub(super) const CONTENT: Fields = 1;
    pub(super) const TOP_LEFT: Fields = 1 << 1;
    pub(super) const PIVOT_OFFSET: Fields = 1 << 2;
    pub(super) const ALPHA: Fields = 1 << 3;
    pub(super) const SCALE_X: Fields = 1 << 4;
    pub(super) const SCALE_Y: Fields = 1 << 5;
    pub(super) const TRANSLATION_X: Fields = 1 << 6;
    pub(super) const TRANSLATION_Y: Fields = 1 << 7;
    pub(super) const ROTATION_Z: Fields = 1 << 8;
    pub(super) const SHADOW_ELEVATION: Fields = 1 << 9;
    pub(super) const AMBIENT_SHADOW_COLOR: Fields = 1 << 10;
    pub(super) const SPOT_SHADOW_COLOR: Fields = 1 << 11;
    pub(super) const BLEND_MODE: Fields = 1 << 12;
    pub(super) const COLOR_FILTER: Fields = 1 << 13;
    pub(super) const CLIP: Fields = 1 << 14;
    pub(super) const COMPOSITING_STRATEGY: Fields = 1 << 15;
    pub(super) const OUTLINE: Fields = 1 << 16;
}

impl Properties {
    /// The properties that differ, their floats compared bit for bit: an unspecified pivot
    /// (`NaN`, the default) equal to itself, without copying either.
    fn changes(&self, other: &Properties) -> Fields {
        let same = |a: f32, b: f32| a.to_bits() == b.to_bits();
        let bit = |differs: bool, bit: Fields| if differs { bit } else { 0 };
        let Properties {
            top_left,
            pivot_offset,
            alpha,
            scale_x,
            scale_y,
            translation_x,
            translation_y,
            rotation_z,
            shadow_elevation,
            ambient_shadow_color,
            spot_shadow_color,
            blend_mode,
            color_filter,
            clip,
            compositing_strategy,
            outline,
        } = self;
        let pivot = !same(pivot_offset.x, other.pivot_offset.x)
            || !same(pivot_offset.y, other.pivot_offset.y);
        bit(*top_left != other.top_left, field::TOP_LEFT)
            | bit(pivot, field::PIVOT_OFFSET)
            | bit(!same(*alpha, other.alpha), field::ALPHA)
            | bit(!same(*scale_x, other.scale_x), field::SCALE_X)
            | bit(!same(*scale_y, other.scale_y), field::SCALE_Y)
            | bit(!same(*translation_x, other.translation_x), field::TRANSLATION_X)
            | bit(!same(*translation_y, other.translation_y), field::TRANSLATION_Y)
            | bit(!same(*rotation_z, other.rotation_z), field::ROTATION_Z)
            | bit(!same(*shadow_elevation, other.shadow_elevation), field::SHADOW_ELEVATION)
            | bit(*ambient_shadow_color != other.ambient_shadow_color, field::AMBIENT_SHADOW_COLOR)
            | bit(*spot_shadow_color != other.spot_shadow_color, field::SPOT_SHADOW_COLOR)
            | bit(*blend_mode != other.blend_mode, field::BLEND_MODE)
            | bit(*color_filter != other.color_filter, field::COLOR_FILTER)
            | bit(*clip != other.clip, field::CLIP)
            | bit(*compositing_strategy != other.compositing_strategy, field::COMPOSITING_STRATEGY)
            | bit(*outline != other.outline, field::OUTLINE)
    }
}

impl Default for Properties {
    fn default() -> Self {
        Properties {
            top_left: IntOffset::new(0, 0),
            pivot_offset: Offset::UNSPECIFIED,
            alpha: 1.0,
            scale_x: 1.0,
            scale_y: 1.0,
            translation_x: 0.0,
            translation_y: 0.0,
            rotation_z: 0.0,
            shadow_elevation: 0.0,
            ambient_shadow_color: Color::BLACK,
            spot_shadow_color: Color::BLACK,
            blend_mode: BlendMode::SrcOver,
            color_filter: None,
            clip: false,
            compositing_strategy: CompositingStrategy::Auto,
            outline: None,
        }
    }
}

thread_local! {
    /// How deep in draw passes this thread is, with [`CHANGED_IN_PASS`] set once a layer
    /// changed in them: each pass reads and writes this alone unless one did.
    static PASS_DEPTH: Cell<u32> = const { Cell::new(0) };
    /// The layers changed in the current draw pass.
    static PASS_CHANGES: RefCell<Vec<std::rc::Weak<LayerCell>>> = const { RefCell::new(Vec::new()) };
    /// A pass's end told what drew a layer that changed in it (D1331).
    static PASS_TOLD: Cell<bool> = const { Cell::new(false) };
}

/// Whether the last draw pass's end told drawings of a layer that changed in it, which then
/// draw again in the same frame (D1331); the flag is cleared.
#[inline]
pub(crate) fn take_pass_told() -> bool {
    PASS_TOLD.replace(false)
}

/// [`PASS_DEPTH`]'s flag: a layer changed in the pass, which its end looks at.
const CHANGED_IN_PASS: u32 = 1 << 31;

/// A draw pass, as `UiTree::draw` runs one. A layer changed in it (recorded, a property set:
/// what drawing code does) tells, at the pass's end, each drawing that drew it and would draw it
/// otherwise if it ran again (D1337): one that drew it after a change in the pass if something
/// it didn't set itself changed after it drew it; any other (drawn before the pass, or in it
/// before its first change) if the pass ends with something it didn't set itself other than
/// the pass found it. They draw again in the same frame, in a second pass `UiTree::draw` runs
/// (D1331), as Compose's render nodes are current at once. So a drawing recording a layer
/// (twice, even) and then drawing it doesn't draw itself again, nor does one recording what
/// the layer holds already ([`GraphicsLayer::store`]); a drawing drawing it after changes
/// undoing each other keeps what it drew; and drawings each setting a property and drawing the
/// layer (placing one layer twice, or drawing it at two alphas) go idle.
pub(crate) struct DrawPass;

impl DrawPass {
    #[inline]
    pub(crate) fn enter() -> DrawPass {
        PASS_DEPTH.with(|depth| depth.set(depth.get() + 1));
        DrawPass
    }
}

impl Drop for DrawPass {
    #[inline]
    fn drop(&mut self) {
        let left = PASS_DEPTH.with(|depth| {
            let left = depth.get() - 1;
            depth.set(left);
            left
        });
        if left == CHANGED_IN_PASS {
            end_changed_pass();
        }
    }
}

/// The end of the outermost draw pass a layer changed in: the drawings it leaves stale draw
/// again.
#[cold]
#[inline(never)]
fn end_changed_pass() {
    PASS_DEPTH.with(|depth| depth.set(0));
    let changed = PASS_CHANGES.with(|changes| std::mem::take(&mut *changes.borrow_mut()));
    for layer in changed.iter().filter_map(std::rc::Weak::upgrade) {
        let Some(start) = layer.pass_start.take() else { continue };
        let net = start.changes(&layer);
        if layer.end_pass(net) {
            PASS_TOLD.set(true);
        }
    }
}

/// A layer as a draw pass found it, kept at its first change in the pass.
struct PassStart {
    /// The recording it had, once replaced in the pass.
    recording: Option<DisplayList>,
    recorded_origin: Offset,
    size: IntSize,
    density: f32,
    released: bool,
    properties: Properties,
}

impl PassStart {
    /// What of `layer` isn't as it was.
    fn changes(&self, layer: &LayerCell) -> Fields {
        let content = self.released != layer.released.get()
            || self.recorded_origin != layer.recorded_origin.get()
            || self.size != layer.size.get()
            || self.density != layer.density.get()
            || self
                .recording
                .as_ref()
                .is_some_and(|recording| !same_recording(recording, &layer.recording.borrow()));
        let content = if content { field::CONTENT } else { 0 };
        content | self.properties.changes(&layer.properties.borrow())
    }
}

/// An observation run: a drawing running (its observer and key), and which run of it.
type Run = compose_runtime::platform::ObservationRun;

/// A drawing that drew a layer (an observation: its observer and key), with the version it
/// reads, which is bumped to tell it alone.
struct Reader {
    /// Who, and the run of it that last drew the layer; `None` for reads outside an
    /// observation (one reader for them all).
    who: Option<Run>,
    version: compose_runtime::OwnedState<u64>,
    /// What that run set itself before drawing the layer (each time, if it drew it more than
    /// once): their changes don't tell it, as running again it sets them again.
    own: Fields,
    /// What the layer was when that run first drew it, if after the layer's first change in
    /// the current pass: the pass's end compares the layer with it, not with what the pass
    /// found. (Running again, the drawing starts from what the pass ends with, and its own
    /// changes after drawing the layer are what they were.)
    seen: Option<Box<Seen>>,
    /// Its version was bumped since it drew the layer: nothing more to tell it.
    told: bool,
}

/// What a reader saw of a layer, drawing it after its first change in a pass.
struct Seen {
    properties: Properties,
    /// The layer's `content` then.
    content: u64,
}

impl Seen {
    /// What of `layer` isn't as it was: its content compared by its count of changes.
    fn changes(&self, layer: &LayerCell) -> Fields {
        let content = if self.content == layer.content.get() { 0 } else { field::CONTENT };
        content | self.properties.changes(&layer.properties.borrow())
    }
}

/// Whether reads by `a` and by `b` are one reader's.
fn same_reader(a: Option<Run>, b: Option<Run>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.same_reader(&b),
        (None, None) => true,
        _ => false,
    }
}

struct LayerCell {
    recording: RefCell<DisplayList>,
    /// Where the recording's (0, 0) is in it: a content recorded where its node draws.
    recorded_origin: Cell<Offset>,
    size: Cell<IntSize>,
    density: Cell<f32>,
    properties: RefCell<Properties>,
    released: Cell<bool>,
    /// How many times its content changed (wrapping).
    content: Cell<u64>,
    /// What it was when the current draw pass first changed it.
    pass_start: RefCell<Option<PassStart>>,
    /// What drew it, each with a version of its own (D1337): the drawings whose latest run
    /// drew it, and those gone (or run again without drawing it) since a new one last came,
    /// which drops them (D1339).
    readers: RefCell<Vec<Reader>>,
    /// The observation run that last set something on it (`None`: code outside one), and
    /// what it set.
    writer: Cell<Option<Run>>,
    written: Cell<Fields>,
    /// The block a placement placing with it runs, made once so placing again with it is the
    /// same block (D1909).
    placement_block: RefCell<Option<crate::graphics_layer::LayerBlock>>,
}

impl LayerCell {
    /// What observation run `run` set itself, if it's the last that set something.
    fn own(&self, run: Option<Run>) -> Fields {
        match run {
            Some(run) if self.writer.get() == Some(run) => self.written.get(),
            _ => 0,
        }
    }

    /// `fields` are set (changed or not) by the code running now.
    fn note_set(&self, fields: Fields) {
        let run = compose_runtime::platform::current_observation();
        if run.is_some() && self.writer.get() == run {
            self.written.set(self.written.get() | fields);
        } else {
            self.writer.set(run);
            self.written.set(if run.is_some() { fields } else { 0 });
        }
    }

    /// Reads the version of the drawing drawing it now, so a change it would draw otherwise
    /// draws it again: what it set itself before is noted, and what it saw if it's after the
    /// layer's first change in a draw pass (D1337).
    fn read_version(&self) {
        let run = compose_runtime::platform::current_observation();
        let own = self.own(run);
        let after_change = self.pass_start.borrow().is_some();
        let mut readers = self.readers.borrow_mut();
        let index = match readers.iter().position(|reader| same_reader(reader.who, run)) {
            Some(index) => index,
            None => {
                // A new reader: those whose observation isn't current any more (their node
                // gone, or drawn again without drawing it) read it no more, told or not (D1339).
                readers.retain(|reader| reader.who.is_none_or(|who| who.is_current()));
                readers.push(Reader {
                    who: run,
                    version: new_version(),
                    own,
                    seen: None,
                    told: true,
                });
                readers.len() - 1
            }
        };
        let reader = &mut readers[index];
        if reader.told || reader.who != run {
            // A new run, or a new reader: what it drew before is replaced.
            reader.who = run;
            reader.own = own;
            reader.seen = after_change.then(|| {
                let properties = self.properties.borrow().clone();
                Box::new(Seen { properties, content: self.content.get() })
            });
            reader.told = false;
        } else {
            // Drawn again in the run, which goes on from what it saw first: told of what either
            // drawing didn't set.
            reader.own &= own;
        }
        let version = reader.version.handle();
        drop(readers);
        version.get();
    }

    /// `fields` changed outside a draw pass: the readers that didn't set them are told.
    fn tell(&self, fields: Fields) {
        let mut told: SmallVec<[compose_runtime::MutableState<u64>; 4]> = SmallVec::new();
        for reader in self.readers.borrow_mut().iter_mut() {
            if !reader.told && fields & !reader.own != 0 {
                reader.told = true;
                told.push(reader.version.handle());
            }
        }
        bump(&told);
    }

    /// The end of a draw pass that changed it, `net` what it ends other than it started: the
    /// readers that saw it other than it ends, in what they didn't set, are told. Whether any
    /// is.
    fn end_pass(&self, net: Fields) -> bool {
        let mut told: SmallVec<[compose_runtime::MutableState<u64>; 4]> = SmallVec::new();
        for reader in self.readers.borrow_mut().iter_mut().filter(|reader| !reader.told) {
            let changed = reader.seen.take().map_or(net, |seen| seen.changes(self));
            if changed & !reader.own != 0 {
                reader.told = true;
                told.push(reader.version.handle());
            }
        }
        bump(&told);
        !told.is_empty()
    }
}

/// A reader's version, counted as initialized so that its first write is a change its readers
/// hear: one made in the global snapshot (in a draw, where readers are met) isn't changed by a
/// write there until that snapshot advances (as Compose's, which takes such a write for its
/// initialization), so a reader met in a pass with nothing else written since wouldn't hear of
/// its first change. Made without a snapshot, whose taking would send the global snapshot's
/// changes in the middle of the draw (D1339).
#[cold]
fn new_version() -> compose_runtime::OwnedState<u64> {
    compose_runtime::platform::preexisting_state_of(0)
}

/// Tells what read `versions`.
fn bump(versions: &[compose_runtime::MutableState<u64>]) {
    for version in versions {
        version.update(|version| version.wrapping_add(1));
    }
}

/// A graphics layer, as Compose's `GraphicsLayer`: shared as a Kotlin reference is (a clone is
/// the same layer). Record into it with [`record`](GraphicsLayer::record) (or
/// [`ContentDrawScope::record_layer`] for a node's content), draw it with
/// [`DrawScope::draw_layer`]. A change of its recording or its properties draws again what
/// drew it.
#[derive(Clone)]
pub struct GraphicsLayer(Rc<LayerCell>);

macro_rules! properties {
    ($($(#[$doc:meta])* $field:ident, $set:ident: $ty:ty = $bit:ident;)*) => {
        $(
            $(#[$doc])*
            pub fn $field(&self) -> $ty {
                self.0.properties.borrow().$field
            }
            #[doc = concat!("Sets [`", stringify!($field), "`](GraphicsLayer::", stringify!($field), ").")]
            pub fn $set(&self, $field: $ty) {
                self.update(field::$bit, |properties| properties.$field = $field);
            }
        )*
    };
}

impl Default for GraphicsLayer {
    fn default() -> Self {
        GraphicsLayer::new()
    }
}

impl GraphicsLayer {
    /// An empty layer, as `GraphicsContext.createGraphicsLayer()`.
    pub fn new() -> GraphicsLayer {
        GraphicsLayer(Rc::new(LayerCell {
            recording: RefCell::default(),
            recorded_origin: Cell::new(Offset::ZERO),
            size: Cell::new(IntSize::ZERO),
            density: Cell::new(1.0),
            properties: RefCell::default(),
            released: Cell::new(false),
            content: Cell::new(0),
            pass_start: RefCell::new(None),
            readers: RefCell::new(Vec::new()),
            writer: Cell::new(None),
            written: Cell::new(0),
            placement_block: RefCell::new(None),
        }))
    }

    /// `fields` are about to change, `properties` giving its properties until then: in a draw
    /// pass, what it was when the pass started is kept the first time, for the pass's end
    /// ([`DrawPass`]); outside one, the readers that didn't set them are told. Whether it's in
    /// a draw pass.
    fn will_change(&self, fields: Fields, properties: impl FnOnce() -> Properties) -> bool {
        let cell = &self.0;
        if PASS_DEPTH.with(|depth| depth.get() & !CHANGED_IN_PASS == 0) {
            cell.tell(fields);
            return false;
        }
        {
            let mut start = cell.pass_start.borrow_mut();
            if start.is_none() {
                PASS_CHANGES.with(|changes| changes.borrow_mut().push(Rc::downgrade(cell)));
                PASS_DEPTH.with(|depth| depth.set(depth.get() | CHANGED_IN_PASS));
                *start = Some(PassStart {
                    recording: None,
                    recorded_origin: cell.recorded_origin.get(),
                    size: cell.size.get(),
                    density: cell.density.get(),
                    released: cell.released.get(),
                    properties: properties(),
                });
            }
        }
        true
    }

    /// Replaces its recording: in a draw pass, the one it started with is kept for its end.
    fn replace_recording(&self, recording: DisplayList, in_pass: bool) {
        let old = self.0.recording.replace(recording);
        if in_pass {
            let mut start = self.0.pass_start.borrow_mut();
            if let Some(start) = start.as_mut().filter(|start| start.recording.is_none()) {
                start.recording = Some(old);
            }
        }
    }

    /// Sets property `field` with `change`: set by the code running now, changed or not.
    fn update(&self, field: Fields, change: impl FnOnce(&mut Properties)) {
        self.0.note_set(field);
        let mut next = self.0.properties.borrow().clone();
        change(&mut next);
        let fields = next.changes(&self.0.properties.borrow());
        if fields == 0 {
            return;
        }
        let before = self.0.properties.replace(next);
        self.will_change(fields, || before);
    }

    /// Records `block` at `size`, in the layer's coordinates, with `scope`'s density and
    /// direction, as `record(density, layoutDirection, size) { … }`: what it drew before is
    /// replaced.
    pub fn record(
        &self,
        scope: &DrawScope<'_>,
        size: IntSize,
        block: impl FnOnce(&mut DrawScope<'_>),
    ) {
        let mut recording = DisplayList::default();
        let mut layer_scope = DrawScope {
            list: &mut recording,
            origin: Offset::ZERO,
            size: Size::new(size.width as f32, size.height as f32),
            density: scope.density,
            layout_direction: scope.layout_direction,
            locals: scope.locals,
        };
        block(&mut layer_scope);
        self.store(recording, Offset::ZERO, size, scope.density.density);
    }

    /// Keeps `recording` as its content: a change, unless it's what it holds already (the same
    /// operations, shaders, filters and boundaries, at the same origin, size and density), so a
    /// drawing recording the same content each time it runs doesn't draw itself, or what draws
    /// the layer, again.
    fn store(&self, recording: DisplayList, origin: Offset, size: IntSize, density: f32) {
        let cell = &self.0;
        cell.note_set(field::CONTENT);
        let same = !cell.released.get()
            && cell.recorded_origin.get() == origin
            && cell.size.get() == size
            && cell.density.get() == density
            && same_recording(&cell.recording.borrow(), &recording);
        if same {
            return;
        }
        let in_pass = self.will_change(field::CONTENT, || cell.properties.borrow().clone());
        cell.content.set(cell.content.get().wrapping_add(1));
        self.replace_recording(recording, in_pass);
        cell.recorded_origin.set(origin);
        cell.size.set(size);
        cell.density.set(density);
        cell.released.set(false);
    }

    /// Its size: what it was last recorded at, as `size`.
    pub fn size(&self) -> IntSize {
        self.0.size.get()
    }

    properties! {
        /// Where it's drawn in what draws it, as `topLeft`.
        top_left, set_top_left: IntOffset = TOP_LEFT;
        /// What it scales and rotates around, in its own coordinates; `Offset::UNSPECIFIED`
        /// (the default) for its center, as `pivotOffset`.
        pivot_offset, set_pivot_offset: Offset = PIVOT_OFFSET;
        /// Its opacity, as `alpha`.
        alpha, set_alpha: f32 = ALPHA;
        scale_x, set_scale_x: f32 = SCALE_X;
        scale_y, set_scale_y: f32 = SCALE_Y;
        translation_x, set_translation_x: f32 = TRANSLATION_X;
        translation_y, set_translation_y: f32 = TRANSLATION_Y;
        /// Degrees, clockwise, as `rotationZ`.
        rotation_z, set_rotation_z: f32 = ROTATION_Z;
        /// Pixels: a shadow of its outline under it above 0, as `shadowElevation`.
        shadow_elevation, set_shadow_elevation: f32 = SHADOW_ELEVATION;
        ambient_shadow_color, set_ambient_shadow_color: Color = AMBIENT_SHADOW_COLOR;
        spot_shadow_color, set_spot_shadow_color: Color = SPOT_SHADOW_COLOR;
        /// How it's composited with what's under it, as `blendMode`.
        blend_mode, set_blend_mode: BlendMode = BLEND_MODE;
        /// What recolors it as it's composited, as `colorFilter`.
        color_filter, set_color_filter: Option<ColorFilter> = COLOR_FILTER;
        /// Whether it clips what it draws to its outline, as `clip`.
        clip, set_clip: bool = CLIP;
        compositing_strategy, set_compositing_strategy: CompositingStrategy = COMPOSITING_STRATEGY;
    }

    /// Its outline: what it clips to and casts a shadow of, its bounds unless set, as
    /// `outline`.
    pub fn outline(&self) -> Outline {
        let size = self.size();
        self.0.properties.borrow().outline.clone().unwrap_or(Outline::Rectangle(Rect::new(
            0.0,
            0.0,
            size.width as f32,
            size.height as f32,
        )))
    }

    /// Its outline, as `setOutline`.
    pub fn set_outline(&self, outline: Outline) {
        self.update(field::OUTLINE, |properties| properties.outline = Some(outline));
    }

    /// A rectangle from `top_left` of `size` (`Size::UNSPECIFIED`: to its far corner), as
    /// `setRectOutline`.
    pub fn set_rect_outline(&self, top_left: Offset, size: Size) {
        let rect = self.outline_rect(top_left, size);
        self.set_outline(Outline::Rectangle(rect));
    }

    /// A rectangle with corners of `corner_radius`, as `setRoundRectOutline`.
    pub fn set_round_rect_outline(&self, top_left: Offset, size: Size, corner_radius: f32) {
        let rect = self.outline_rect(top_left, size);
        self.set_outline(Outline::Rounded(rect, [corner_radius; 4]));
    }

    /// `path`'s contents now, as `setPathOutline`.
    pub fn set_path_outline(&self, path: &crate::path::Path) {
        self.set_outline(Outline::generic(path));
    }

    fn outline_rect(&self, top_left: Offset, size: Size) -> Rect {
        let layer = self.size();
        let width = if size.width.is_nan() { layer.width as f32 - top_left.x } else { size.width };
        let height =
            if size.height.is_nan() { layer.height as f32 - top_left.y } else { size.height };
        Rect::new(top_left.x, top_left.y, top_left.x + width, top_left.y + height)
    }

    /// Whether it was released, as `isReleased`.
    pub fn is_released(&self) -> bool {
        self.0.released.get()
    }

    /// Its drawing, with its properties, as a bitmap of its size, as `toImageBitmap` (Compose's
    /// is a coroutine: here, the bitmap is drawn when it returns, by the platform's
    /// `BitmapRasterizer`). Its top left isn't applied: the bitmap is the layer at its origin.
    pub fn to_image_bitmap(&self) -> crate::image::ImageBitmap {
        let size = self.size();
        let image =
            crate::image::ImageBitmap::new(size.width.max(0) as u32, size.height.max(0) as u32);
        let mut canvas = crate::canvas::Canvas::new(&image);
        let top_left = self.top_left();
        crate::canvas::CanvasDrawScope::draw(
            &mut canvas,
            crate::unit::Density::new(1.0),
            crate::alignment::LayoutDirection::Ltr,
            Size::new(size.width as f32, size.height as f32),
            |scope| {
                scope.translate(-top_left.x as f32, -top_left.y as f32, |scope| {
                    scope.draw_layer(self);
                });
            },
        );
        image
    }

    /// Drops its recording: it draws nothing until recorded again, as
    /// `GraphicsContext.releaseGraphicsLayer`.
    pub fn release(&self) {
        self.0.note_set(field::CONTENT);
        if self.0.released.get() {
            return;
        }
        let in_pass = self.will_change(field::CONTENT, || self.0.properties.borrow().clone());
        self.0.content.set(self.0.content.get().wrapping_add(1));
        self.replace_recording(DisplayList::default(), in_pass);
        self.0.released.set(true);
    }

    /// How many readers it keeps (gone ones included until a new one comes): for tests.
    #[doc(hidden)]
    pub fn reader_count(&self) -> usize {
        self.0.readers.borrow().len()
    }

    /// Whether `other` is this layer.
    pub fn ptr_eq(&self, other: &GraphicsLayer) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    /// The block giving its properties to a placement's layer, the same each time.
    pub(crate) fn placement_block(&self) -> crate::graphics_layer::LayerBlock {
        let mut block = self.0.placement_block.borrow_mut();
        block
            .get_or_insert_with(|| {
                let cell = Rc::downgrade(&self.0);
                crate::graphics_layer::LayerBlock::new(move |scope| {
                    if let Some(cell) = cell.upgrade() {
                        GraphicsLayer(cell).apply_to_scope(scope);
                    }
                })
            })
            .clone()
    }

    /// Gives `scope` its properties, for a placement placing with it (D1909): its transform,
    /// alpha, shadow, clip (to the bounds) and compositing strategy; its pivot, if set, as a
    /// fraction of the scope's size. Reads its version, so a change to them runs this again.
    pub(crate) fn apply_to_scope(&self, scope: &mut crate::graphics_layer::GraphicsLayerScope) {
        self.0.read_version();
        let properties = self.0.properties.borrow();
        scope.alpha = properties.alpha;
        scope.scale_x = properties.scale_x;
        scope.scale_y = properties.scale_y;
        scope.translation_x = properties.translation_x;
        scope.translation_y = properties.translation_y;
        scope.rotation_z = properties.rotation_z;
        scope.shadow_elevation = properties.shadow_elevation;
        scope.clip = properties.clip;
        scope.compositing_strategy = properties.compositing_strategy;
        if properties.ambient_shadow_color != Color::BLACK {
            scope.set_ambient_shadow_color(properties.ambient_shadow_color);
        }
        if properties.spot_shadow_color != Color::BLACK {
            scope.set_spot_shadow_color(properties.spot_shadow_color);
        }
        let pivot = properties.pivot_offset;
        if !pivot.x.is_nan() && !pivot.y.is_nan() {
            let (width, height) = (scope.size.width, scope.size.height);
            let fraction = |at: f32, of: f32| if of == 0.0 { 0.5 } else { at / of };
            scope.transform_origin = crate::graphics_layer::TransformOrigin {
                pivot_fraction_x: fraction(pivot.x, width),
                pivot_fraction_y: fraction(pivot.y, height),
            };
        }
    }
}

/// Whether two recordings draw the same: their operations, the shaders and filters these name,
/// and the boundaries drawn among them. (Their paragraphs are named by the operations.) Floats
/// compare bit for bit, so a NaN in a recording doesn't make it a change each time
/// (`same_ops`); a shader or filter unequal by `==` is compared by its printed values, which
/// a NaN prints the same.
fn same_recording(old: &DisplayList, new: &DisplayList) -> bool {
    fn same_values<T: PartialEq + std::fmt::Debug>(old: &[T], new: &[T]) -> bool {
        old.len() == new.len()
            && old.iter().zip(new).all(|(a, b)| a == b || format!("{a:?}") == format!("{b:?}"))
    }
    crate::display_list::same_ops(&old.ops, &new.ops)
        && same_values(&old.shaders, &new.shaders)
        && same_values(&old.color_filters, &new.color_filters)
        && old.boundaries == new.boundaries
}

impl std::fmt::Debug for GraphicsLayer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "GraphicsLayer({:?})", self.size())
    }
}

impl compose_runtime::ComposeArg for GraphicsLayer {
    fn same(&self, other: &Self) -> bool {
        self.ptr_eq(other)
    }
}

/// A layer kept where it's composed, as `rememberGraphicsLayer()`. Named `remember_…` as it takes
/// a slot.
pub fn remember_graphics_layer() -> GraphicsLayer {
    compose_runtime::remember(GraphicsLayer::new)
}

impl ContentDrawScope<'_, '_> {
    /// Records into `layer` what `block` draws, the content included where it calls
    /// `draw_content`, at this drawing's size, as `layer.record { drawContent() }` in
    /// `drawWithContent`.
    pub fn record_layer(
        &mut self,
        layer: &GraphicsLayer,
        block: impl FnOnce(&mut ContentDrawScope<'_, '_>),
    ) {
        let mut recording = DisplayList::default();
        let size = self.scope.size;
        let origin = self.scope.origin;
        {
            let mut layer_scope = DrawScope {
                list: &mut recording,
                origin,
                size,
                density: self.scope.density,
                layout_direction: self.scope.layout_direction,
                locals: self.scope.locals,
            };
            let mut content =
                ContentDrawScope { scope: &mut layer_scope, content: &mut *self.content };
            block(&mut content);
        }
        let size = IntSize::new(size.width.round() as i32, size.height.round() as i32);
        layer.store(recording, origin, size, self.scope.density.density);
    }
}

impl DrawScope<'_> {
    /// Draws `layer` at its `top_left`, its properties applied, as `drawLayer`: its transform
    /// around its pivot, its shadow, its clip to its outline, then its recording in an offscreen
    /// buffer for its alpha (under `Auto` below 1, under `Offscreen` always), color filter or
    /// blend mode; under `ModulateAlpha`, its drawings' alphas multiplied instead. Nothing once
    /// released. What drew it draws again when it changes.
    pub fn draw_layer(&mut self, layer: &GraphicsLayer) {
        let cell = &layer.0;
        cell.read_version();
        if cell.released.get() {
            return;
        }
        let properties = cell.properties.borrow().clone();
        let size = cell.size.get();
        let size = Size::new(size.width as f32, size.height as f32);
        let origin =
            self.origin + Offset::new(properties.top_left.x as f32, properties.top_left.y as f32);
        let pivot = if properties.pivot_offset.x.is_nan() {
            Offset::new(size.width / 2.0, size.height / 2.0)
        } else {
            properties.pivot_offset
        };
        let shadow = properties.shadow_elevation.max(0.0);
        let outline = (properties.clip || shadow > 0.0).then(|| layer.outline().translate(origin));
        let painted =
            properties.blend_mode != BlendMode::SrcOver || properties.color_filter.is_some();
        let mut strategy = properties.compositing_strategy;
        if painted {
            strategy = CompositingStrategy::Offscreen;
        }
        let mut layer_properties = LayerProperties {
            alpha: properties.alpha.clamp(0.0, 1.0),
            pivot: pivot + origin,
            scale: (properties.scale_x, properties.scale_y),
            rotation: properties.rotation_z,
            translation: Offset::new(properties.translation_x, properties.translation_y),
            clip: properties.clip,
            outline,
            shadow_elevation: shadow,
            compositing_strategy: strategy,
            bounds: Rect::new(origin.x, origin.y, origin.x + size.width, origin.y + size.height),
            density: cell.density.get(),
            extras: None,
        };
        let colors = (properties.ambient_shadow_color, properties.spot_shadow_color);
        if colors != (Color::BLACK, Color::BLACK) {
            layer_properties.extras = Some(Box::new(crate::graphics_layer::LayerExtras {
                ambient_shadow_color: colors.0,
                spot_shadow_color: colors.1,
                render_effect: None,
            }));
        }
        layer_properties.operations(|op| match op {
            // A filter or a blend mode goes into the offscreen buffer's paint.
            DrawOp::SaveLayerAlpha { bounds, alpha } if painted => {
                let filter = properties.color_filter.as_ref();
                self.save_layer(
                    Rect::new(
                        bounds.left - self.origin.x,
                        bounds.top - self.origin.y,
                        bounds.right - self.origin.x,
                        bounds.bottom - self.origin.y,
                    ),
                    alpha,
                    filter,
                    properties.blend_mode,
                );
            }
            op => self.list.push(op),
        });
        let modulated = layer_properties.modulated_alpha();
        self.replay_layer(&cell.recording.borrow(), origin - cell.recorded_origin.get(), modulated);
        for _ in 0..layer_properties.restores() {
            self.list.push(DrawOp::Restore);
        }
    }

    /// Copies `recording` moved by `by`, its shaders, filters and the groups it draws with it,
    /// its drawings' alphas multiplied by `modulated` if any.
    fn replay_layer(&mut self, recording: &DisplayList, by: Offset, modulated: Option<f32>) {
        let list = &mut *self.list;
        let bases = (list.shaders.len() as u32, list.color_filters.len() as u32);
        list.shaders.extend_from_slice(&recording.shaders);
        list.color_filters.extend_from_slice(&recording.color_filters);
        let moved = by != Offset::ZERO;
        if moved {
            list.push(DrawOp::Translate(by));
        }
        let start = list.ops.len();
        crate::display_list::extend_shifted(&mut list.ops, &recording.ops, bases);
        if let Some(alpha) = modulated {
            for shader in &mut list.shaders[bases.0 as usize..] {
                shader.scale_alpha(alpha);
            }
            for op in &mut list.ops[start..] {
                match op {
                    DrawOp::DrawImage { alpha: image, .. } => *image *= alpha,
                    op => {
                        if let Some(paint) = op.paint_mut() {
                            *paint = paint.with_alpha(alpha);
                        }
                    }
                }
            }
        }
        list.boundaries.extend(
            recording
                .boundaries
                .iter()
                .map(|boundary| Boundary { at: boundary.at + start, key: boundary.key }),
        );
        list.laid_out_paragraphs.extend(recording.laid_out_paragraphs.iter().cloned());
        if moved {
            list.push(DrawOp::Translate(Offset::new(0.0 - by.x, 0.0 - by.y)));
        }
    }
}
