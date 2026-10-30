//! Rulers, as Compose's `Ruler`, `RulerScope` and `RectRulers`: positions a layout provides to
//! what's in it (`MeasureScope::layout_with_rulers`), read while placing
//! (`PlacementScope::current`) in the reader's coordinates, a change telling only those that read
//! (D1218). They cost a tree nothing until one's provided.

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use smallvec::SmallVec;

use crate::alignment::LayoutDirection;
use crate::coordinates::LayoutCoordinates;
use crate::placement::PlacementScope;
use crate::tree::{NodeId, UiTree};
use crate::unit::{Density, IntSize, Offset};

type Derivation = Rc<dyn Fn(&mut PlacementScope<'_>, f32) -> f32>;

/// A position in a layout along one axis, as Compose's `Ruler`: made by [`VerticalRuler`] or
/// [`HorizontalRuler`], equal only to itself and its clones.
#[derive(Clone)]
pub struct Ruler(Rc<RulerData>);

struct RulerData {
    id: u64,
    vertical: bool,
    derivation: Option<Derivation>,
}

impl PartialEq for Ruler {
    fn eq(&self, other: &Self) -> bool {
        self.0.id == other.0.id
    }
}

impl Eq for Ruler {}

impl std::fmt::Debug for Ruler {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let axis = if self.0.vertical { "VerticalRuler" } else { "HorizontalRuler" };
        write!(formatter, "{axis}({})", self.0.id)
    }
}

impl Ruler {
    fn make(vertical: bool, derivation: Option<Derivation>) -> Ruler {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Ruler(Rc::new(RulerData { id: NEXT.fetch_add(1, Ordering::Relaxed), vertical, derivation }))
    }

    pub(crate) fn id(&self) -> u64 {
        self.0.id
    }

    pub(crate) fn derivation(&self) -> Option<Derivation> {
        self.0.derivation.clone()
    }

    /// What `value`, in `source`, is in `target`, as `calculateCoordinate`.
    pub(crate) fn in_coordinates(
        &self,
        value: f32,
        source: &LayoutCoordinates,
        target: &LayoutCoordinates,
    ) -> f32 {
        let size = source.size();
        if self.0.vertical {
            let at = Offset::new(value, size.height as f32 / 2.0);
            target.local_position_of(source, at).x
        } else {
            let at = Offset::new(size.width as f32 / 2.0, value);
            target.local_position_of(source, at).y
        }
    }
}

/// What `current` of `rulers` give, the greatest or least, as `mergeRulerValues`.
fn merged(
    placement: &mut PlacementScope<'_>,
    rulers: &[Ruler],
    greater: bool,
    default: f32,
) -> f32 {
    let mut value = f32::NAN;
    for ruler in rulers {
        let next = placement.current(ruler, f32::NAN);
        if value.is_nan() || (greater == (next > value)) {
            value = next;
        }
    }
    if value.is_nan() { default } else { value }
}

/// Makes vertical rulers, a position along the width, as Compose's `VerticalRuler`.
pub struct VerticalRuler;

impl VerticalRuler {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Ruler {
        Ruler::make(true, None)
    }

    /// One worked out when it's read, as `derived`, from the default it's read with.
    pub fn derived(calculation: impl Fn(&mut PlacementScope<'_>, f32) -> f32 + 'static) -> Ruler {
        Ruler::make(true, Some(Rc::new(calculation)))
    }

    /// The greatest of `rulers`, as `maxOf`.
    pub fn max_of(rulers: &[Ruler]) -> Ruler {
        let rulers = rulers.to_vec();
        VerticalRuler::derived(move |placement, default| merged(placement, &rulers, true, default))
    }

    /// The least of `rulers`, as `minOf`.
    pub fn min_of(rulers: &[Ruler]) -> Ruler {
        let rulers = rulers.to_vec();
        VerticalRuler::derived(move |placement, default| merged(placement, &rulers, false, default))
    }
}

/// Makes horizontal rulers, a position along the height, as Compose's `HorizontalRuler`.
pub struct HorizontalRuler;

impl HorizontalRuler {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Ruler {
        Ruler::make(false, None)
    }

    /// One worked out when it's read, as `derived`, from the default it's read with.
    pub fn derived(calculation: impl Fn(&mut PlacementScope<'_>, f32) -> f32 + 'static) -> Ruler {
        Ruler::make(false, Some(Rc::new(calculation)))
    }

    /// The greatest of `rulers`, as `maxOf`.
    pub fn max_of(rulers: &[Ruler]) -> Ruler {
        let rulers = rulers.to_vec();
        HorizontalRuler::derived(move |placement, default| {
            merged(placement, &rulers, true, default)
        })
    }

    /// The least of `rulers`, as `minOf`.
    pub fn min_of(rulers: &[Ruler]) -> Ruler {
        let rulers = rulers.to_vec();
        HorizontalRuler::derived(move |placement, default| {
            merged(placement, &rulers, false, default)
        })
    }
}

/// Four rulers around a rectangle, as Compose's `RectRulers`.
#[derive(Clone, Debug)]
pub struct RectRulers {
    pub left: Ruler,
    pub top: Ruler,
    pub right: Ruler,
    pub bottom: Ruler,
}

impl RectRulers {
    #[allow(clippy::new_without_default)]
    pub fn new() -> RectRulers {
        RectRulers {
            left: VerticalRuler::new(),
            top: HorizontalRuler::new(),
            right: VerticalRuler::new(),
            bottom: HorizontalRuler::new(),
        }
    }

    fn sides(rulers: &[RectRulers]) -> [Vec<Ruler>; 4] {
        [
            rulers.iter().map(|it| it.left.clone()).collect(),
            rulers.iter().map(|it| it.top.clone()).collect(),
            rulers.iter().map(|it| it.right.clone()).collect(),
            rulers.iter().map(|it| it.bottom.clone()).collect(),
        ]
    }

    /// The rectangle inside all of `rulers`, as `innermostOf`.
    pub fn innermost_of(rulers: &[RectRulers]) -> RectRulers {
        let [left, top, right, bottom] = RectRulers::sides(rulers);
        RectRulers {
            left: VerticalRuler::max_of(&left),
            top: HorizontalRuler::max_of(&top),
            right: VerticalRuler::min_of(&right),
            bottom: HorizontalRuler::min_of(&bottom),
        }
    }

    /// The rectangle around all of `rulers`, as `outermostOf`.
    pub fn outermost_of(rulers: &[RectRulers]) -> RectRulers {
        let [left, top, right, bottom] = RectRulers::sides(rulers);
        RectRulers {
            left: VerticalRuler::min_of(&left),
            top: HorizontalRuler::min_of(&top),
            right: VerticalRuler::max_of(&right),
            bottom: HorizontalRuler::max_of(&bottom),
        }
    }
}

/// What a layout provides rulers with, as Compose's `RulerScope`.
pub struct RulerScope {
    coordinates: LayoutCoordinates,
    size: IntSize,
    direction: LayoutDirection,
    density: Density,
    values: RulerValues,
}

pub(crate) type RulerValues = SmallVec<[(Ruler, f32); 2]>;

impl RulerScope {
    pub(crate) fn new(
        coordinates: LayoutCoordinates,
        size: IntSize,
        direction: LayoutDirection,
        density: Density,
    ) -> RulerScope {
        RulerScope { coordinates, size, direction, density, values: SmallVec::new() }
    }

    pub(crate) fn value_of(&self, ruler: &Ruler) -> Option<f32> {
        self.values.iter().find(|(it, _)| it == ruler).map(|(_, value)| *value)
    }

    /// The coordinates of what provides them, as `coordinates`: what's worked out from where they
    /// are follows them.
    pub fn coordinates(&self) -> LayoutCoordinates {
        self.coordinates
    }

    /// The density where the layout is composed.
    pub fn density(&self) -> Density {
        self.density
    }

    /// Gives `ruler` a value in these coordinates, as `provides`.
    pub fn provides(&mut self, ruler: &Ruler, value: f32) {
        match self.values.iter_mut().find(|(it, _)| it == ruler) {
            Some(entry) => entry.1 = value,
            None => self.values.push((ruler.clone(), value)),
        }
    }

    /// As `provides`, from the end in a right-to-left layout, as `providesRelative`.
    pub fn provides_relative(&mut self, ruler: &Ruler, value: f32) {
        let value = if self.direction == LayoutDirection::Rtl {
            self.size.width as f32 - value
        } else {
            value
        };
        self.provides(ruler, value);
    }
}

pub(crate) type RulerBlock = Rc<dyn Fn(&mut RulerScope)>;
pub(crate) type RulerIsProvided = Rc<dyn Fn(&Ruler) -> bool>;
pub(crate) type RulerProvider = Rc<dyn Fn(&mut RulerScope, &Ruler)>;

/// What a layer of a node provides.
pub(crate) struct LayerRulers {
    pub(crate) source: RulerSource,
    /// What `Block` last gave.
    pub(crate) values: RulerValues,
}

pub(crate) enum RulerSource {
    /// Worked out each time it's placed, all of them, as `rulers`.
    Block(RulerBlock),
    /// Worked out for each read, as `rulerProvider`.
    Lazily(RulerIsProvided, RulerProvider),
}

impl UiTree {
    /// `reader` read `ruler`: a change to it tells that reader, once.
    pub(crate) fn note_ruler_reader(&mut self, ruler: &Ruler, reader: NodeId) {
        self.ruler_readers.entry((ruler.id(), reader)).or_insert_with(|| ruler.clone());
    }

    /// Those under `provider` reading what `told` says, told: they place again, and read again if
    /// they still do.
    fn tell_ruler_readers(&mut self, provider: NodeId, told: impl Fn(&Ruler) -> bool) {
        if self.ruler_readers.is_empty() {
            return;
        }
        let readers: SmallVec<[(u64, NodeId); 4]> = (self.ruler_readers.iter())
            .filter(|((_, reader), ruler)| told(ruler) && self.is_at_or_under(*reader, provider))
            .map(|(key, _)| *key)
            .collect();
        for key in readers {
            self.ruler_readers.remove(&key);
            self.mark_needs_placement(key.1);
        }
    }

    fn tell_readers_of_source(&mut self, provider: NodeId, source: &RulerSource) {
        if let RulerSource::Lazily(is_provided, _) = source {
            self.tell_ruler_readers(provider, |ruler| is_provided(ruler));
        }
    }

    fn is_at_or_under(&self, node: NodeId, ancestor: NodeId) -> bool {
        let mut current = Some(node);
        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            current =
                self.get(id).and_then(|value| if value.overlay { None } else { value.parent });
        }
        false
    }

    /// What layer `layer` of `node` provides as a block, worked out again: who read what changed,
    /// came or went hears. It's in the placing of its node: what it reads places it again.
    pub(crate) fn capture_rulers(&mut self, node: NodeId, layer: usize) {
        let Some(RulerSource::Block(block)) = (self.node(node).placements.as_ref())
            .and_then(|it| it.rulers.get(layer)?.as_ref())
            .map(|it| &it.source)
        else {
            return;
        };
        let block = block.clone();
        let new = {
            let measure = crate::layout::MeasureScope::new(&mut *self, node, 0);
            let _open = measure.base.tree.open();
            let mut scope = measure.base.tree.ruler_scope(node, layer);
            block(&mut scope);
            scope.values
        };
        let rulers = (self.node_mut(node).placements.as_mut())
            .and_then(|it| it.rulers.get_mut(layer)?.as_mut())
            .expect("just found");
        let old = std::mem::replace(&mut rulers.values, new.clone());
        let same = |ruler: &Ruler| {
            let of = |values: &RulerValues| {
                values.iter().find(|(it, _)| it == ruler).map(|(_, value)| value.to_bits())
            };
            of(&old) == of(&new)
        };
        if old.iter().chain(&new).any(|(ruler, _)| !same(ruler)) {
            self.tell_ruler_readers(node, |ruler| {
                let provided = |values: &RulerValues| values.iter().any(|(it, _)| it == ruler);
                (provided(&old) || provided(&new)) && !same(ruler)
            });
        }
    }

    pub(crate) fn ruler_scope(&self, node: NodeId, layer: usize) -> RulerScope {
        let value = self.node(node);
        RulerScope::new(
            self.coordinates_of_layer(node, layer),
            value.layers[layer].measured,
            value.layout_direction,
            value.density,
        )
    }

    /// `invalidate_rulers` of what `node` provides: worked out again if a block, and those that
    /// read what's worked out when read told.
    pub(crate) fn invalidate_rulers_of(&mut self, node: NodeId) {
        let Some(placements) = self.get_mut(node).and_then(|value| value.placements.as_mut())
        else {
            return;
        };
        let mut block = false;
        let mut lazily: SmallVec<[RulerIsProvided; 1]> = SmallVec::new();
        for rulers in placements.rulers.iter().flatten() {
            match &rulers.source {
                RulerSource::Block(_) => block = true,
                RulerSource::Lazily(is_provided, _) => lazily.push(is_provided.clone()),
            }
        }
        if block {
            self.mark_needs_placement(node);
        }
        for is_provided in lazily {
            self.tell_ruler_readers(node, |ruler| is_provided(ruler));
        }
    }

    /// `layer` of `node` was measured, what it provided before being `earlier`: what's gone is
    /// told now, what's provided as a block when it's worked out, placing it, and what's worked
    /// out when read may be anything.
    pub(crate) fn rulers_measured(
        &mut self,
        node: NodeId,
        layer: usize,
        earlier: Option<LayerRulers>,
    ) {
        let now = (self.node_mut(node).placements.as_mut())
            .and_then(|it| it.rulers.get_mut(layer)?.as_mut());
        let Some(now) = now else {
            if let Some(earlier) = earlier {
                self.rulers_gone(node, earlier);
            }
            return;
        };
        match (&now.source, earlier) {
            (RulerSource::Block(_), Some(earlier)) => match earlier.source {
                RulerSource::Block(_) => now.values = earlier.values,
                RulerSource::Lazily(..) => self.tell_readers_of_source(node, &earlier.source),
            },
            (RulerSource::Block(_), None) => {}
            (RulerSource::Lazily(is_provided, _), earlier) => {
                let is_provided = is_provided.clone();
                if let Some(earlier) = earlier {
                    self.rulers_gone(node, earlier);
                }
                self.tell_ruler_readers(node, |ruler| is_provided(ruler));
            }
        }
    }

    /// `rulers` aren't provided by `node` any more.
    fn rulers_gone(&mut self, node: NodeId, rulers: LayerRulers) {
        match &rulers.source {
            RulerSource::Block(_) => self
                .tell_ruler_readers(node, |ruler| rulers.values.iter().any(|(it, _)| it == ruler)),
            RulerSource::Lazily(..) => self.tell_readers_of_source(node, &rulers.source),
        }
    }

    /// `node` has `layers` layers now, what those past them provided gone.
    pub(crate) fn rulers_of_layers_gone(&mut self, node: NodeId, layers: usize) {
        let Some(placements) = self.node_mut(node).placements.as_mut() else { return };
        if placements.rulers.len() <= layers {
            return;
        }
        let gone: SmallVec<[LayerRulers; 1]> =
            placements.rulers.drain(layers..).flatten().collect();
        for rulers in gone {
            self.rulers_gone(node, rulers);
        }
    }
}
