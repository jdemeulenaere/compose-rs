//! Overscroll over the platform's edge effects: the events of Compose's
//! `AndroidEdgeEffectOverscrollEffect`, over `compose_ui::EdgeEffect`, whose physics stay the
//! platform's (docs/parity/design/stretch-overscroll.md, D1476).
//!
//! An effect is made when its side is first pulled, and only an active effect is asked anything: a
//! scroller that never hits an edge calls the platform never, and a normal scroll's delta costs a
//! few flags. The one exception is Compose's too: a release at 1 px/s or less doesn't fling, its
//! velocity is left over, and the effect absorbs it, anywhere in the list.

use std::any::Any;
use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;

use compose_runtime::composable;

use compose_ui::{
    Color, Density, DrawModifierNode, DrawScope, EdgeEffect, EdgeEffectFactory, EdgeEffectSide,
    EventContext, IntSize, Modifier, ModifierNode, ModifierNodeElement, NestedScrollSource,
    NodeContext, NodeHandle, Offset, PaddingValues, PointerEvent, PointerEventPass,
    PointerEventType, PointerId, PointerInputModifierNode, Size, Velocity,
};

use crate::gestures::LocalBoxFuture;
use crate::overscroll::{OverscrollEffect, OverscrollFactory, OverscrollNode, PerformFling};

/// A glow's color by default, Android's `EdgeEffect`'s, as `DefaultGlowColor`.
pub const DEFAULT_GLOW_COLOR: Color = Color::from_argb(0xff66_6666);

/// Inside a fling a stretch relaxes this many times faster than the fling would move: flinging a
/// stretch away looks wrong when little happens at first, as `FlingDestretchFactor`.
const FLING_DESTRETCH_FACTOR: f32 = 4.0;

/// How far across its axis a stretch keeps what's drawn past its bounds: what a scrolling
/// container's clip leaves for shadows, as `MaxSupportedElevation`.
const MAX_SUPPORTED_ELEVATION: compose_ui::Dp = compose_ui::Dp(30.0);

/// Makes effects over a platform's edge effects, as `AndroidEdgeEffectOverscrollFactory`.
pub struct EdgeEffectOverscrollFactory {
    edge_effects: Rc<dyn EdgeEffectFactory>,
    glow_color: Color,
    glow_draw_padding: PaddingValues,
    invalidation_enabled: Rc<Cell<bool>>,
}

impl EdgeEffectOverscrollFactory {
    /// The platform's `edge_effects`, with Android's glow defaults.
    pub fn new(edge_effects: Rc<dyn EdgeEffectFactory>) -> EdgeEffectOverscrollFactory {
        EdgeEffectOverscrollFactory::with_glow(
            edge_effects,
            DEFAULT_GLOW_COLOR,
            PaddingValues::ZERO,
        )
    }

    /// Where the platform glows, its color and how far in from the bounds it's drawn.
    pub fn with_glow(
        edge_effects: Rc<dyn EdgeEffectFactory>,
        glow_color: Color,
        glow_draw_padding: PaddingValues,
    ) -> EdgeEffectOverscrollFactory {
        EdgeEffectOverscrollFactory {
            edge_effects,
            glow_color,
            glow_draw_padding,
            invalidation_enabled: Rc::new(Cell::new(true)),
        }
    }

    /// Whether its effects ask for frames, for tests, as Compose's `@VisibleForTesting
    /// invalidationEnabled`: an effect that's pulled draws every frame, so a test waiting for idle
    /// with a fake that doesn't move never ends.
    #[doc(hidden)]
    pub fn set_invalidation_enabled(&self, enabled: bool) {
        self.invalidation_enabled.set(enabled);
    }
}

impl OverscrollFactory for EdgeEffectOverscrollFactory {
    fn create_overscroll_effect(&self) -> Rc<dyn OverscrollEffect> {
        Rc::new(EdgeEffectOverscrollEffect::new(
            self.edge_effects.clone(),
            self.glow_color,
            self.glow_draw_padding,
            self.invalidation_enabled.clone(),
        ))
    }
}

/// `LOCAL_OVERSCROLL_FACTORY`'s default: the composing tree's edge effects, one factory per tree.
pub(crate) fn default_overscroll_factory() -> Option<Rc<dyn OverscrollFactory>> {
    /// The `Rc<dyn …>` a tree keeps as `Any`.
    struct Kept(Rc<dyn OverscrollFactory>);
    let platform = compose_ui::platform::platform_edge_effects()?;
    let kept = platform.made_from(|| {
        Rc::new(Kept(Rc::new(EdgeEffectOverscrollFactory::new(platform.factory.clone()))))
            as Rc<dyn Any>
    });
    kept.downcast_ref::<Kept>().map(|kept| kept.0.clone())
}

/// A factory with this glow over the host's edge effects, as `rememberPlatformOverscrollFactory`,
/// for `LOCAL_OVERSCROLL_FACTORY` near the top of an app. `None` on a host without edge effects:
/// Compose's function is Android's alone.
#[composable(non_restartable)]
pub fn remember_platform_overscroll_factory(
    glow_color: Color,
    glow_draw_padding: PaddingValues,
) -> Option<Rc<dyn OverscrollFactory>> {
    let platform = compose_ui::platform::platform_edge_effects();
    compose_runtime::remember_keyed(
        (glow_color, glow_draw_padding, platform.as_ref().map(|it| Rc::as_ptr(it) as usize)),
        move || {
            platform.map(|platform| {
                Rc::new(EdgeEffectOverscrollFactory::with_glow(
                    platform.factory.clone(),
                    glow_color,
                    glow_draw_padding,
                )) as Rc<dyn OverscrollFactory>
            })
        },
    )
}

/// The edge effects on a side, made at its first pull: while it isn't `active`, the platform isn't
/// asked anything about it (it is finished, so at distance 0).
#[derive(Default)]
struct Edge {
    effect: OnceCell<Rc<dyn EdgeEffect>>,
    active: Cell<bool>,
}

impl Edge {
    fn is_stretched(&self) -> bool {
        self.active.get() && self.effect.get().is_some_and(|effect| effect.distance() != 0.0)
    }

    fn is_animating(&self) -> bool {
        self.active.get() && self.effect.get().is_some_and(|effect| !effect.is_finished())
    }

    /// What the platform said it may do, after a call that could have started or ended it.
    fn refresh(&self) {
        self.active.set(self.effect.get().is_some_and(|effect| !effect.is_finished()));
    }
}

const TOP: usize = 0;
const BOTTOM: usize = 1;
const LEFT: usize = 2;
const RIGHT: usize = 3;
const SIDES: [EdgeEffectSide; 4] =
    [EdgeEffectSide::Top, EdgeEffectSide::Bottom, EdgeEffectSide::Left, EdgeEffectSide::Right];

/// Its value, and what's kept of it by the `Rc` its `modifier` and `node` share.
pub(crate) struct EdgeEffectOverscrollEffect {
    shared: Rc<Shared>,
}

struct Shared {
    factory: Rc<dyn EdgeEffectFactory>,
    /// Its number in the display list; the host is told it's gone only once an effect was made.
    owner: u32,
    made: Cell<bool>,
    glow_color: Color,
    glow_draw_padding: PaddingValues,
    edges: [Edge; 4],
    /// The last drawn size, as `containerSize`: empty, it takes nothing.
    container_size: Cell<Size>,
    density: Cell<f32>,
    /// The last down pointer's position, kept after it's up, as `pointerPosition`.
    pointer_position: Cell<Option<Offset>>,
    pointer: Cell<Option<PointerId>>,
    scroll_cycle_in_progress: Cell<bool>,
    /// The node drawing it, as Compose's one attached `node`.
    holder: RefCell<Option<NodeHandle>>,
    invalidation_enabled: Rc<Cell<bool>>,
    /// Drawing has opened its ops, for `draw_after` to close them.
    drawing: Cell<bool>,
    /// Each active frame's: the same bytes would keep a group's recording (D1475).
    stamp: Cell<u32>,
}

impl Drop for Shared {
    fn drop(&mut self) {
        if self.made.get() {
            self.factory.release_edge_effects(self.owner);
        }
    }
}

impl EdgeEffectOverscrollEffect {
    fn new(
        factory: Rc<dyn EdgeEffectFactory>,
        glow_color: Color,
        glow_draw_padding: PaddingValues,
        invalidation_enabled: Rc<Cell<bool>>,
    ) -> EdgeEffectOverscrollEffect {
        EdgeEffectOverscrollEffect {
            shared: Rc::new(Shared {
                factory,
                owner: compose_ui::next_edge_effects_owner(),
                made: Cell::new(false),
                glow_color,
                glow_draw_padding,
                edges: Default::default(),
                container_size: Cell::new(Size::new(0.0, 0.0)),
                density: Cell::new(1.0),
                pointer_position: Cell::new(None),
                pointer: Cell::new(None),
                scroll_cycle_in_progress: Cell::new(false),
                holder: RefCell::new(None),
                invalidation_enabled,
                drawing: Cell::new(false),
                stamp: Cell::new(0),
            }),
        }
    }
}

impl Shared {
    fn is_stretched(&self, side: usize) -> bool {
        self.edges[side].is_stretched()
    }

    fn is_animating(&self, side: usize) -> bool {
        self.edges[side].is_animating()
    }

    /// The side's effect, made now if it isn't yet, sized as the others, as `getOrCreateTopEffect`.
    fn effect(&self, side: usize) -> &Rc<dyn EdgeEffect> {
        self.edges[side].effect.get_or_init(|| {
            self.made.set(true);
            let effect = self.factory.create_edge_effect(self.owner, SIDES[side], self.glow_color);
            let size = self.container_size.get();
            let (width, height) = (size.width.round() as i32, size.height.round() as i32);
            if width != 0 || height != 0 {
                if side == TOP || side == BOTTOM {
                    effect.set_size(width, height);
                } else {
                    effect.set_size(height, width);
                }
            }
            effect
        })
    }

    fn invalidate(&self) {
        if !self.invalidation_enabled.get() {
            return;
        }
        if let Some(holder) = &*self.holder.borrow() {
            holder.invalidate_draw();
        }
    }

    /// As `displacement`: the last pointer's place in the container, or its center.
    fn displacement(&self) -> Offset {
        let size = self.container_size.get();
        let pointer =
            self.pointer_position.get().unwrap_or(Offset::new(size.width / 2.0, size.height / 2.0));
        Offset::new(pointer.x / size.width, pointer.y / size.height)
    }

    /// As `pullTop` and the like: pulls `side` by `scroll`, the pointer's way. Returns what it
    /// consumed; all of it while it's showing, floats not to get in the way (b/265363356).
    fn pull(&self, side: usize, scroll: Offset) -> f32 {
        let displacement = self.displacement();
        let size = self.container_size.get();
        let (along, length, sign, at) = match side {
            TOP => (scroll.y, size.height, 1.0, displacement.x),
            BOTTOM => (scroll.y, size.height, -1.0, 1.0 - displacement.x),
            LEFT => (scroll.x, size.width, 1.0, 1.0 - displacement.y),
            _ => (scroll.x, size.width, -1.0, displacement.y),
        };
        let effect = self.effect(side);
        let consumed = sign * effect.on_pull_distance(sign * along / length, at) * length;
        let showing = effect.distance() != 0.0;
        self.edges[side].refresh();
        if showing { along } else { consumed }
    }

    /// `side`'s stretch relaxing by `delta` (`destretch` of it asked of the stretch): what it took
    /// of `delta`, the stretch finished if it's gone.
    fn relax(&self, side: usize, delta: f32, destretch: Offset, multiplier: f32) -> f32 {
        let consumed = self.pull(side, destretch);
        if !self.is_stretched(side) {
            self.effect(side).finish();
            self.edges[side].refresh();
        }
        let asked = if side == TOP || side == BOTTOM { destretch.y } else { destretch.x };
        // All of it: no rounding from dividing.
        if consumed == asked { delta } else { consumed / multiplier }
    }

    /// As `animateToReleaseIfNeeded`: what's pulled goes back. Whatever is still going is drawn.
    fn animate_to_release_if_needed(&self) {
        let mut needs_invalidation = false;
        for edge in &self.edges {
            if edge.active.get() {
                let effect = edge.effect.get().expect("an active effect exists");
                effect.on_release();
                edge.refresh();
                needs_invalidation = needs_invalidation || edge.active.get();
            }
        }
        if needs_invalidation {
            self.invalidate();
        }
    }

    /// As `releaseOppositeOverscroll`, for a glow: a stretch relaxes before the scroll.
    fn release_opposite_overscroll(&self, delta: Offset) -> bool {
        let mut needs_invalidation = false;
        for (side, opposite) in [
            (LEFT, delta.x < 0.0),
            (RIGHT, delta.x > 0.0),
            (TOP, delta.y < 0.0),
            (BOTTOM, delta.y > 0.0),
        ] {
            if opposite && self.is_animating(side) {
                let along = if side == TOP || side == BOTTOM { delta.y } else { delta.x };
                self.effect(side).on_release_with_opposite_delta(along);
                self.edges[side].refresh();
                needs_invalidation = needs_invalidation || self.is_animating(side);
            }
        }
        needs_invalidation
    }

    /// As `absorbToRelaxIfNeeded`: a fling that wouldn't fully relax `side` is all taken by it;
    /// otherwise it takes none, and the fling's scroll relaxes it.
    fn absorb_to_relax_if_needed(&self, side: usize, velocity: f32, length: f32) -> f32 {
        let effect = self.effect(side);
        if fling_distance(self.density.get(), velocity) <= effect.distance() * length {
            effect.on_absorb(velocity.round() as i32);
            self.edges[side].refresh();
            velocity
        } else {
            0.0
        }
    }

    /// As `updateSize`: the effects hear a new size, and what's pulled goes back from it.
    fn update_size(&self, size: Size, density: f32) {
        self.density.set(density);
        let old = self.container_size.get();
        if old == size {
            return;
        }
        self.container_size.set(size);
        let (width, height) = (size.width.round() as i32, size.height.round() as i32);
        for (side, edge) in self.edges.iter().enumerate() {
            if let Some(effect) = edge.effect.get() {
                if side == TOP || side == BOTTOM {
                    effect.set_size(width, height);
                } else {
                    effect.set_size(height, width);
                }
            }
        }
        if old != Size::new(0.0, 0.0) {
            self.animate_to_release_if_needed();
        }
    }

    fn take_pointer_event(&self, event: &PointerEvent, pass: PointerEventPass) {
        if pass != PointerEventPass::Main {
            return;
        }
        match event.kind_moving() {
            // `awaitFirstDown`: the first down of a gesture, a new one once it's up.
            PointerEventType::Press => {
                if self.pointer.get().is_none() {
                    self.pointer.set(Some(event.pointer));
                    self.pointer_position.set(Some(event.position));
                }
            }
            PointerEventType::Move if event.pressed => {
                // The pointer tracked, or the next that moves once it's up.
                if self.pointer.get().is_none() || self.pointer.get() == Some(event.pointer) {
                    self.pointer.set(Some(event.pointer));
                    self.pointer_position.set(Some(event.position));
                }
            }
            // Its position stays until the next down, not to change what's showing.
            PointerEventType::Release if self.pointer.get() == Some(event.pointer) => {
                self.pointer.set(None);
            }
            _ => {}
        }
    }
}

/// As `flingDistance`, from Android's `OverScroller`: how far a fling at `velocity` goes.
fn fling_distance(density: f32, velocity: f32) -> f32 {
    const INFLECTION: f64 = 0.35;
    // `ViewConfiguration.getScrollFriction()`, a constant since API 1.
    const FRICTION: f64 = 0.015;
    const GRAVITY_EARTH: f64 = 9.80665;
    const INCHES_PER_METER: f64 = 39.37;
    let deceleration_rate = 0.78f64.ln() / 0.9f64.ln();
    let coefficient = GRAVITY_EARTH * INCHES_PER_METER * f64::from(density) * 160.0 * 0.84;
    let l = (INFLECTION * f64::from(velocity.abs()) / (FRICTION * coefficient)).ln();
    (FRICTION * coefficient * (deceleration_rate / (deceleration_rate - 1.0) * l).exp()) as f32
}

impl OverscrollEffect for EdgeEffectOverscrollEffect {
    fn apply_to_scroll(
        &self,
        delta: Offset,
        source: NestedScrollSource,
        perform_scroll: &mut dyn FnMut(Offset) -> Offset,
    ) -> Offset {
        let shared = &*self.shared;
        let size = shared.container_size.get();
        if size.width == 0.0 || size.height == 0.0 {
            return perform_scroll(delta);
        }
        if !shared.scroll_cycle_in_progress.get() {
            // A new cycle catches a stretch where it is: a pull of nothing puts it back to being
            // pulled, so this scroll goes on with it rather than it fading under the finger.
            for side in [LEFT, RIGHT, TOP, BOTTOM] {
                if shared.is_stretched(side) {
                    shared.pull(side, Offset::ZERO);
                }
            }
            shared.scroll_cycle_in_progress.set(true);
        }
        // What's stretched relaxes before the scroll, faster in a fling.
        let multiplier =
            if source == NestedScrollSource::SideEffect { FLING_DESTRETCH_FACTOR } else { 1.0 };
        let destretch = Offset::new(delta.x * multiplier, delta.y * multiplier);
        let consumed_y = if delta.y == 0.0 {
            0.0
        } else if delta.y < 0.0 && shared.is_stretched(TOP) {
            shared.relax(TOP, delta.y, destretch, multiplier)
        } else if delta.y > 0.0 && shared.is_stretched(BOTTOM) {
            shared.relax(BOTTOM, delta.y, destretch, multiplier)
        } else {
            0.0
        };
        let consumed_x = if delta.x == 0.0 {
            0.0
        } else if delta.x < 0.0 && shared.is_stretched(LEFT) {
            shared.relax(LEFT, delta.x, destretch, multiplier)
        } else if delta.x > 0.0 && shared.is_stretched(RIGHT) {
            shared.relax(RIGHT, delta.x, destretch, multiplier)
        } else {
            0.0
        };
        let consumed = Offset::new(consumed_x, consumed_y);
        if consumed_x != 0.0 || consumed_y != 0.0 {
            shared.invalidate();
        }

        let left_for_delta = delta - consumed;
        let consumed_by_delta = perform_scroll(left_for_delta);
        let left_for_overscroll = left_for_delta - consumed_by_delta;

        // The scroll started to consume again (an item came at the end) with a stretch showing:
        // it goes back, not to stay stuck.
        if (left_for_delta.x != 0.0 || left_for_delta.y != 0.0)
            && (consumed_by_delta.x != 0.0 || consumed_by_delta.y != 0.0)
            && (0..4).any(|side| shared.is_stretched(side))
        {
            shared.animate_to_release_if_needed();
        }

        let mut needs_invalidation = false;
        if source == NestedScrollSource::UserInput {
            // Less than half a pixel comes from rounding, and can lock a scroll up (b/265363356).
            if left_for_overscroll.x > 0.5 {
                shared.pull(LEFT, left_for_overscroll);
                needs_invalidation = true;
            } else if left_for_overscroll.x < -0.5 {
                shared.pull(RIGHT, left_for_overscroll);
                needs_invalidation = true;
            }
            if left_for_overscroll.y > 0.5 {
                shared.pull(TOP, left_for_overscroll);
                needs_invalidation = true;
            } else if left_for_overscroll.y < -0.5 {
                shared.pull(BOTTOM, left_for_overscroll);
                needs_invalidation = true;
            }
        }
        // For a glow: a stretch consuming above left nothing here.
        if left_for_delta.x != 0.0 || left_for_delta.y != 0.0 {
            needs_invalidation = shared.release_opposite_overscroll(delta) || needs_invalidation;
        }
        if needs_invalidation {
            shared.invalidate();
        }
        consumed + consumed_by_delta
    }

    fn apply_to_fling<'a>(
        &'a self,
        velocity: Velocity,
        perform_fling: PerformFling,
    ) -> LocalBoxFuture<'a, ()> {
        Box::pin(async move {
            let shared = &*self.shared;
            let size = shared.container_size.get();
            if size.width == 0.0 || size.height == 0.0 {
                perform_fling(velocity).await;
                return;
            }
            // What's stretched relaxes first, if the fling wouldn't relax it all.
            let consumed_x = if velocity.x < 0.0 && shared.is_stretched(LEFT) {
                shared.absorb_to_relax_if_needed(LEFT, velocity.x, size.width)
            } else if velocity.x > 0.0 && shared.is_stretched(RIGHT) {
                -shared.absorb_to_relax_if_needed(RIGHT, -velocity.x, size.width)
            } else {
                0.0
            };
            let consumed_y = if velocity.y < 0.0 && shared.is_stretched(TOP) {
                shared.absorb_to_relax_if_needed(TOP, velocity.y, size.height)
            } else if velocity.y > 0.0 && shared.is_stretched(BOTTOM) {
                -shared.absorb_to_relax_if_needed(BOTTOM, -velocity.y, size.height)
            } else {
                0.0
            };
            let consumed = Velocity::new(consumed_x, consumed_y);
            if consumed_x != 0.0 || consumed_y != 0.0 {
                shared.invalidate();
            }

            let remaining = velocity - consumed;
            let consumed_by_fling = perform_fling(remaining).await;
            let left = remaining - consumed_by_fling;

            shared.scroll_cycle_in_progress.set(false);
            // What's left stretches.
            let absorb = |side: usize, velocity: f32| {
                shared.effect(side).on_absorb(velocity.round() as i32);
                shared.edges[side].refresh();
            };
            if left.x > 0.0 {
                absorb(LEFT, left.x);
            } else if left.x < 0.0 {
                absorb(RIGHT, -left.x);
            }
            if left.y > 0.0 {
                absorb(TOP, left.y);
            } else if left.y < 0.0 {
                absorb(BOTTOM, -left.y);
            }
            // For a stretch only a velocity of exactly 0 gets here with something to release; a
            // glow already showing doesn't absorb, so it's released here too.
            shared.animate_to_release_if_needed();
        })
    }

    fn is_in_progress(&self) -> bool {
        self.shared.edges.iter().any(Edge::is_stretched)
    }

    fn modifier(&self) -> Modifier {
        Modifier.then_custom(EdgeEffectElement { node: self.shared.clone() })
    }

    fn node(&self) -> Option<Rc<dyn OverscrollNode>> {
        Some(self.shared.clone())
    }
}

impl OverscrollNode for Shared {
    fn attach(&self, handle: NodeHandle) -> bool {
        let mut holder = self.holder.borrow_mut();
        if holder.is_some() {
            return false;
        }
        *holder = Some(handle);
        true
    }

    fn detach(&self) {
        *self.holder.borrow_mut() = None;
        // Not attached: it isn't drawn and takes nothing, as a node's size is known by drawing.
        self.pointer.set(None);
    }

    fn draw(&self, scope: &mut DrawScope<'_>) {
        self.update_size(scope.size(), scope.density().density);
        // The platform moves it as the frame is replayed: asked again while it says it's not over,
        // which may run a frame longer than Compose's `needsInvalidate`, read after its draw.
        let mut active = false;
        for edge in &self.edges {
            if edge.active.get() {
                edge.refresh();
                active = active || edge.active.get();
            }
        }
        let size = scope.size();
        if !active || size.width == 0.0 || size.height == 0.0 {
            return;
        }
        self.invalidate();
        let density = scope.density();
        let padding = self.glow_draw_padding;
        let (left, right) = if scope.layout_direction() == compose_ui::LayoutDirection::Rtl {
            (padding.end, padding.start)
        } else {
            (padding.start, padding.end)
        };
        self.stamp.set(self.stamp.get().wrapping_add(1));
        self.drawing.set(true);
        scope.begin_edge_effects(
            self.owner,
            self.stamp.get(),
            density.to_px(MAX_SUPPORTED_ELEVATION).round(),
            [
                density.to_px(left),
                density.to_px(padding.top),
                density.to_px(right),
                density.to_px(padding.bottom),
            ],
        );
    }

    fn draw_after(&self, scope: &mut DrawScope<'_>) {
        if self.drawing.take() {
            scope.end_edge_effects();
        }
    }

    fn on_pointer_event(
        &self,
        event: &PointerEvent,
        pass: PointerEventPass,
        bounds: IntSize,
        density: Density,
    ) {
        // A holder reaching it at a press hasn't drawn it yet: its size is known here too.
        self.update_size(Size::new(bounds.width as f32, bounds.height as f32), density.density);
        self.take_pointer_event(event, pass);
    }

    fn on_pointer_cancelled(&self) {
        self.pointer.set(None);
    }
}

/// What `Modifier.overscroll` adds for it.
struct EdgeEffectElement {
    node: Rc<Shared>,
}

impl ModifierNodeElement for EdgeEffectElement {
    type Node = EdgeEffectNode;

    fn create(&self) -> EdgeEffectNode {
        EdgeEffectNode { node: self.node.clone(), holds: false, handle: None }
    }

    fn update(&self, node: &mut EdgeEffectNode) {
        if !Rc::ptr_eq(&node.node, &self.node) {
            node.replace_with(self.node.clone());
        }
    }

    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.node, &other.node)
    }

    fn name(&self) -> &'static str {
        "overscroll"
    }
}

struct EdgeEffectNode {
    node: Rc<Shared>,
    /// It's the one drawing the effect.
    holds: bool,
    /// While attached.
    handle: Option<NodeHandle>,
}

impl EdgeEffectNode {
    /// A new effect: the handle is the one that drew the last, as it's the same node.
    fn replace_with(&mut self, node: Rc<Shared>) {
        let handle = self.handle;
        if std::mem::take(&mut self.holds) {
            self.node.detach();
        }
        self.node = node;
        if let Some(handle) = handle {
            self.holds = self.node.attach(handle);
        }
    }
}

impl ModifierNode for EdgeEffectNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }

    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }

    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        let handle = context.handle();
        self.handle = Some(handle);
        self.holds = self.node.attach(handle);
    }

    fn on_detach(&mut self, _context: &mut NodeContext<'_>) {
        self.handle = None;
        if std::mem::take(&mut self.holds) {
            self.node.detach();
        }
    }
}

impl DrawModifierNode for EdgeEffectNode {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        if self.holds {
            OverscrollNode::draw(&*self.node, scope);
        }
    }

    fn draw_after(&mut self, scope: &mut DrawScope<'_>) {
        if self.holds {
            OverscrollNode::draw_after(&*self.node, scope);
        }
    }
}

impl PointerInputModifierNode for EdgeEffectNode {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        if self.holds {
            OverscrollNode::on_pointer_event(&*self.node, event, pass, bounds, context.density());
        }
    }

    fn on_cancel_pointer_input(&mut self) {
        if self.holds {
            self.node.on_pointer_cancelled();
        }
    }
}
