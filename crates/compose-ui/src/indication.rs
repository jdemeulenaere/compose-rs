//! Indications, as Compose's `Indication`, `IndicationNodeFactory`, `LocalIndication` and
//! `Modifier.indication`: what shows a component's interactions (a press, a hover, a focus),
//! drawn by a node a factory makes for the component's interaction source.

use std::any::Any;
use std::fmt;
use std::rc::Rc;

use compose_runtime::CompositionLocal;

use crate::draw::DrawScope;
use crate::graphics::{Color, Paint};
use crate::interaction::{InteractionSource, Listener, Ongoing};
use crate::modifier::{DrawModifierNode, Modifier, ModifierNode, ModifierNodeElement};
use crate::node_context::{NodeContext, NodeHandle};
use crate::unit::{Dp, Rect};

/// Makes the node that draws an indication for one component, as Compose's
/// `IndicationNodeFactory`. Its node collects the source's
/// [`interactions`](InteractionSource::interactions) and draws what they say (its `as_draw`; it
/// may read state it writes, which draws it again). A factory equal to the one before
/// ([`same`](IndicationNodeFactory::same), Compose's `equals`) keeps the node; another makes a new
/// one.
pub trait IndicationNodeFactory: Any {
    /// The node drawing the indication of what `interaction_source` emits, as `create`.
    fn create(&self, interaction_source: InteractionSource) -> Box<dyn ModifierNode>;

    /// Whether `other` makes the same nodes, as Compose's required `equals`: a factory with no
    /// properties compares its type (`other.downcast_ref::<Self>().is_some()`).
    fn same(&self, other: &dyn IndicationNodeFactory) -> bool;
}

impl dyn IndicationNodeFactory {
    /// `self` as `T`, for [`same`](IndicationNodeFactory::same).
    pub fn downcast_ref<T: IndicationNodeFactory>(&self) -> Option<&T> {
        (self as &dyn Any).downcast_ref::<T>()
    }
}

#[derive(Clone)]
enum Kind {
    None,
    Local,
    Factory(Rc<dyn IndicationNodeFactory>),
}

/// An indication, as Compose's `Indication`: a factory of the node drawing it
/// ([`Indication::new`]), `NONE` (Compose's `null`), or `LOCAL`, the clickables' default,
/// [`LOCAL_INDICATION`]'s value where the clickable's node is (D1056).
///
/// [`Indication::bounded`] and [`Indication::unbounded`] are compose-rs's state layer, a simple
/// indication for what doesn't use Material (Material's is its `ripple`), and a `Color` converts
/// to a bounded one.
#[derive(Clone)]
pub struct Indication(Kind);

impl Indication {
    /// No indication, as `indication = null`.
    pub const NONE: Indication = Indication(Kind::None);

    /// [`LOCAL_INDICATION`]'s value where the clickable's node is, as the clickables' overloads
    /// without an `indication` (`useLocalIndication`, D1056); for `Modifier.indication`, where
    /// the modifier is made (Compose's code passes `LocalIndication.current`).
    pub const LOCAL: Indication = Indication(Kind::Local);

    /// The indication `factory` makes the nodes of.
    pub fn new(factory: impl IndicationNodeFactory) -> Indication {
        Indication(Kind::Factory(Rc::new(factory)))
    }

    /// The indication a shared `factory` makes the nodes of.
    pub fn from_factory(factory: Rc<dyn IndicationNodeFactory>) -> Indication {
        Indication(Kind::Factory(factory))
    }

    /// A state layer of `color` over the node's bounds while pressed or focused (10 %) or
    /// hovered (8 %); an unspecified color is no indication.
    pub fn bounded(color: Color) -> Indication {
        Indication::state_layer(color, None)
    }

    /// A state layer as [`bounded`](Self::bounded), in a circle of `radius` centered on the
    /// node, as Material's `ripple(bounded = false, radius)`.
    pub fn unbounded(color: Color, radius: Dp) -> Indication {
        Indication::state_layer(color, Some(radius))
    }

    fn state_layer(color: Color, radius: Option<Dp>) -> Indication {
        if color.is_specified() {
            Indication(Kind::Factory(state_layer_factory(color, radius)))
        } else {
            Indication::NONE
        }
    }

    /// Whether it's `NONE`.
    pub fn is_none(&self) -> bool {
        matches!(self.0, Kind::None)
    }

    /// The factory of a value of [`LOCAL_INDICATION`] (`LOCAL` there means the default).
    pub(crate) fn factory_or_default(self) -> Option<Rc<dyn IndicationNodeFactory>> {
        match self.0 {
            Kind::None => None,
            Kind::Factory(factory) => Some(factory),
            Kind::Local => Some(default_debug_indication()),
        }
    }

    /// Whether it's `LOCAL`.
    pub(crate) fn is_local(&self) -> bool {
        matches!(self.0, Kind::Local)
    }

    /// Its factory, `LOCAL` resolved: [`LOCAL_INDICATION`]'s while composing (where the
    /// composition is now), the default debug indication outside it.
    pub(crate) fn resolve(&self) -> Option<Rc<dyn IndicationNodeFactory>> {
        match &self.0 {
            Kind::None => None,
            Kind::Factory(factory) => Some(factory.clone()),
            Kind::Local => local_factory(),
        }
    }
}

/// [`LOCAL_INDICATION`]'s factory where the composition is now; the default debug indication
/// outside composition.
pub(crate) fn local_factory() -> Option<Rc<dyn IndicationNodeFactory>> {
    if !compose_runtime::is_composing() {
        return Some(default_debug_indication());
    }
    LOCAL_INDICATION.current().factory_or_default()
}

/// Whether two factories make the same nodes: the same one, or equal.
pub(crate) fn same_factory(
    a: &Option<Rc<dyn IndicationNodeFactory>>,
    b: &Option<Rc<dyn IndicationNodeFactory>>,
) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Rc::ptr_eq(a, b) || a.same(&**b),
        (None, None) => true,
        _ => false,
    }
}

impl PartialEq for Indication {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Kind::None, Kind::None) | (Kind::Local, Kind::Local) => true,
            (Kind::Factory(a), Kind::Factory(b)) => Rc::ptr_eq(a, b) || a.same(&**b),
            _ => false,
        }
    }
}

impl fmt::Debug for Indication {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Kind::None => formatter.write_str("Indication::NONE"),
            Kind::Local => formatter.write_str("Indication::LOCAL"),
            Kind::Factory(_) => formatter.write_str("Indication(factory)"),
        }
    }
}

impl compose_runtime::ComposeArg for Indication {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl From<Color> for Indication {
    /// A bounded state layer of the color ([`Indication::bounded`]).
    fn from(color: Color) -> Indication {
        Indication::bounded(color)
    }
}

/// The indication clickables draw by default, as `LocalIndication`: foundation's debug
/// indication (a black overlay, 30 % pressed, 10 % hovered or focused) unless something
/// provides another (Material's theme provides its ripple).
pub static LOCAL_INDICATION: CompositionLocal<Indication> =
    CompositionLocal::new("LocalIndication", || {
        Indication(Kind::Factory(default_debug_indication()))
    });

// ---- The nodes -------------------------------------------------------------------------------

/// What an indication node shows: whether its source has presses, hovers and focuses under way,
/// which its listener keeps as each is emitted (D1057: no collecting task), drawing its node
/// again when it changes, as Compose's indication nodes keep fields and call `invalidateDraw`
/// (no state written, so none while composing either). Each interaction is tracked by
/// identity, as `collect_is_pressed_as_state`'s, so an end without its start (a start dropped
/// with a detached node's queue) changes nothing. It listens from when it's made until its node
/// is detached ([`stop`](Self::stop)), and again from its node's next attachment
/// ([`attach`](Self::attach)), whose context names what draws it.
pub(crate) struct InteractionStates {
    source: InteractionSource,
    shared: Rc<StatesShared>,
    listener: Option<Rc<Listener>>,
}

#[derive(Default)]
struct StatesShared {
    /// Bits: `PRESSED`, `HOVERED`, `FOCUSED`.
    bits: std::cell::Cell<u8>,
    /// What draws the indication (its host's handle), once attached.
    handle: std::cell::Cell<Option<NodeHandle>>,
    /// A draw is asked and the node hasn't drawn since: one request a frame (a press and its
    /// release in one batch ask once).
    asked: std::cell::Cell<bool>,
}

pub(crate) const PRESSED: u8 = 1;
pub(crate) const HOVERED: u8 = 2;
pub(crate) const FOCUSED: u8 = 4;

impl InteractionStates {
    /// Listens to `source`'s interactions from now on, as Compose's indication nodes collect
    /// them from their attachment.
    pub(crate) fn collect(source: &InteractionSource) -> InteractionStates {
        let mut states =
            InteractionStates { source: source.clone(), shared: Rc::default(), listener: None };
        states.start();
        states
    }

    /// Listens from now on (from nothing under way, as a new collection), unless it does.
    fn start(&mut self) {
        if self.listener.is_some() {
            return;
        }
        let shared = self.shared.clone();
        let mut ongoing = Ongoing::default();
        let listener = self.source.listen(move |interaction| {
            ongoing.apply(interaction);
            let bits = if ongoing.pressed() { PRESSED } else { 0 }
                | if ongoing.hovered() { HOVERED } else { 0 }
                | if ongoing.focused() { FOCUSED } else { 0 };
            if shared.bits.replace(bits) != bits
                && !shared.asked.get()
                && let Some(handle) = shared.handle.get()
            {
                shared.asked.set(true);
                handle.invalidate_draw();
            }
        });
        self.listener = Some(listener);
    }

    /// Its node is attached (in `context`, its host's): it listens, and draws through it. What
    /// it heard since it was made, before it had this handle, asks now.
    pub(crate) fn attach(&mut self, context: &mut NodeContext<'_>) {
        self.start();
        self.shared.handle.set(Some(context.handle()));
        // This and `stop`'s reset are close to equivalent mutants, which no test tells from no
        // reset: a detached node hears nothing, and `get` resets it at the next draw. Kept so a
        // node attached again with a request left over can't skip its first.
        self.shared.asked.set(false);
        if self.shared.bits.get() != 0 {
            context.invalidate_draw();
        }
    }

    /// What's under way, as the node draws it (a later change asks to draw again).
    pub(crate) fn get(&self) -> u8 {
        self.shared.asked.set(false);
        self.shared.bits.get()
    }

    /// Stops listening, and shows nothing under way: its node is detached, as Compose's node
    /// scope cancelled (attached again, it starts from nothing, as a new collection).
    pub(crate) fn stop(&mut self) {
        self.listener = None;
        self.shared.bits.set(0);
        self.shared.handle.set(None);
        self.shared.asked.set(false);
    }
}

/// Foundation's default indication, as Compose's `DefaultDebugIndication`.
struct DefaultDebugIndication;

impl IndicationNodeFactory for DefaultDebugIndication {
    fn create(&self, interaction_source: InteractionSource) -> Box<dyn ModifierNode> {
        Box::new(DefaultDebugIndicationNode(InteractionStates::collect(&interaction_source)))
    }

    fn same(&self, other: &dyn IndicationNodeFactory) -> bool {
        other.downcast_ref::<DefaultDebugIndication>().is_some()
    }
}

struct DefaultDebugIndicationNode(InteractionStates);

impl ModifierNode for DefaultDebugIndicationNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.0.attach(context);
    }
    fn on_detach(&mut self, _context: &mut NodeContext<'_>) {
        self.0.stop();
    }
}

impl DrawModifierNode for DefaultDebugIndicationNode {
    /// Over the content: black at 30 % pressed, at 10 % hovered or focused.
    fn draw_after(&mut self, scope: &mut DrawScope<'_>) {
        let bits = self.0.get();
        let alpha = if bits & PRESSED != 0 {
            0.3
        } else if bits & (HOVERED | FOCUSED) != 0 {
            0.1
        } else {
            return;
        };
        let bounds = scope.bounds();
        scope.draw_rect(bounds, Paint::fill(Color::BLACK.with_alpha(alpha)));
    }
}

/// compose-rs's state layer: `color` while pressed or focused (10 %) or hovered (8 %), over the
/// bounds or in a circle of `radius`, as Material's ripple settles at.
#[derive(PartialEq)]
struct StateLayerIndication {
    color: Color,
    radius: Option<Dp>,
}

/// Material's state layer opacity for pressed and focused, as its ripple settles at.
const STATE_LAYER_ALPHA: f32 = 0.10;
/// Material's hovered state layer.
const HOVER_STATE_LAYER_ALPHA: f32 = 0.08;

impl IndicationNodeFactory for StateLayerIndication {
    fn create(&self, interaction_source: InteractionSource) -> Box<dyn ModifierNode> {
        Box::new(StateLayerNode {
            color: self.color,
            radius: self.radius,
            states: InteractionStates::collect(&interaction_source),
        })
    }

    fn same(&self, other: &dyn IndicationNodeFactory) -> bool {
        other.downcast_ref::<StateLayerIndication>().is_some_and(|other| other == self)
    }
}

struct StateLayerNode {
    color: Color,
    radius: Option<Dp>,
    states: InteractionStates,
}

impl ModifierNode for StateLayerNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.states.attach(context);
    }
    fn on_detach(&mut self, _context: &mut NodeContext<'_>) {
        self.states.stop();
    }
}

impl DrawModifierNode for StateLayerNode {
    fn draw_after(&mut self, scope: &mut DrawScope<'_>) {
        let bits = self.states.get();
        let alpha = if bits & (PRESSED | FOCUSED) != 0 {
            STATE_LAYER_ALPHA
        } else if bits & HOVERED != 0 {
            HOVER_STATE_LAYER_ALPHA
        } else {
            return;
        };
        let bounds = scope.bounds();
        let paint = Paint::fill(self.color.with_alpha(alpha));
        match self.radius {
            None => scope.draw_rect(bounds, paint),
            Some(radius) => {
                let radius = scope.density().to_px(radius);
                let (x, y) =
                    ((bounds.left + bounds.right) / 2.0, (bounds.top + bounds.bottom) / 2.0);
                let circle = Rect::new(x - radius, y - radius, x + radius, y + radius);
                scope.draw_round_rect(circle, [radius; 4], paint);
            }
        }
    }
}

// ---- Modifier.indication ---------------------------------------------------------------------

/// `Modifier.indication`'s element.
struct IndicationElement {
    source: InteractionSource,
    factory: Rc<dyn IndicationNodeFactory>,
}

/// Delegates to the factory's node, made again for a new source or a factory not the same, as
/// Compose's `IndicationModifierNode`.
pub(crate) struct IndicationHostNode {
    delegates: crate::delegation::Delegates,
    node: Option<crate::node_chain::EntryId>,
    /// What an update asks for: made when it has a context, the old one gone by then.
    next: Option<(Rc<dyn IndicationNodeFactory>, InteractionSource)>,
}

impl IndicationHostNode {
    fn make_next(&mut self) {
        if let Some((factory, source)) = self.next.take() {
            self.node = Some(self.delegates.delegate_boxed(factory.create(source)));
        }
    }
}

impl ModifierNodeElement for IndicationElement {
    type Node = IndicationHostNode;

    fn create(&self) -> IndicationHostNode {
        let mut node = IndicationHostNode {
            delegates: crate::delegation::Delegates::new(),
            node: None,
            next: Some((self.factory.clone(), self.source.clone())),
        };
        node.make_next();
        node
    }

    /// As `IndicationModifierNode.update`: undelegates, makes the new one and delegates to it.
    fn update(&self, node: &mut IndicationHostNode) {
        if let Some(old) = node.node.take() {
            node.delegates.undelegate_boxed(old);
        }
        node.next = Some((self.factory.clone(), self.source.clone()));
        node.delegates.ask_for_context();
    }

    fn same(&self, other: &Self) -> bool {
        other.source == self.source
            && (Rc::ptr_eq(&other.factory, &self.factory) || other.factory.same(&*self.factory))
    }

    fn name(&self) -> &'static str {
        "indication"
    }
}

impl ModifierNode for IndicationHostNode {
    fn delegates(&mut self) -> Option<&mut crate::delegation::Delegates> {
        Some(&mut self.delegates)
    }
    fn on_update_with_context(&mut self, _: &mut NodeContext<'_>) {
        self.make_next();
    }
    /// An update while it wasn't attached, or a reuse in its pass.
    fn on_attach(&mut self, _: &mut NodeContext<'_>) {
        self.make_next();
    }
}

impl Modifier {
    /// Draws `indication` for the interactions of `interaction_source`, as
    /// `Modifier.indication(interactionSource, indication)`: `Indication::NONE` draws nothing;
    /// `Indication::LOCAL` is [`LOCAL_INDICATION`]'s, read now.
    pub fn indication(
        self,
        interaction_source: impl Into<InteractionSource>,
        indication: Indication,
    ) -> Modifier {
        match indication.resolve() {
            Some(factory) => {
                self.then_custom(IndicationElement { source: interaction_source.into(), factory })
            }
            None => self,
        }
    }
}

// ---- Shared factories ------------------------------------------------------------------------

/// A state layer's color and radius, with its factory.
type StateLayerEntry = (Color, Option<Dp>, Rc<dyn IndicationNodeFactory>);

thread_local! {
    static DEFAULT_DEBUG_INDICATION: Rc<dyn IndicationNodeFactory> =
        Rc::new(DefaultDebugIndication);
    /// The state layers made lately, so a component composing one each pass (a checkbox's)
    /// shares it rather than allocating.
    static STATE_LAYERS: std::cell::RefCell<Vec<StateLayerEntry>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// How many state layers are kept for sharing.
const STATE_LAYERS_KEPT: usize = 32;

fn default_debug_indication() -> Rc<dyn IndicationNodeFactory> {
    DEFAULT_DEBUG_INDICATION.with(Rc::clone)
}

/// The state layer of `color` and `radius`, shared with the last ones made.
fn state_layer_factory(color: Color, radius: Option<Dp>) -> Rc<dyn IndicationNodeFactory> {
    STATE_LAYERS.with(|layers| {
        let mut layers = layers.borrow_mut();
        if let Some((_, _, factory)) =
            layers.iter().find(|(kept, kept_radius, _)| *kept == color && *kept_radius == radius)
        {
            return factory.clone();
        }
        let factory: Rc<dyn IndicationNodeFactory> =
            Rc::new(StateLayerIndication { color, radius });
        if layers.len() == STATE_LAYERS_KEPT {
            layers.remove(0);
        }
        layers.push((color, radius, factory.clone()));
        factory
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraints::Constraints;
    use crate::host::UiHost;
    use crate::interaction::{HoverInteraction, MutableInteractionSource, Press, PressInteraction};
    use crate::layout::{Layout, Measurable, MeasurePolicy, MeasureResult, MeasureScope};
    use crate::modifier::ClickableOptions;
    use crate::unit::{Offset, UnitExt};
    use compose_runtime::{MutableState, callback, mutable_state_of, remember};

    /// Places its children only while `shown`.
    struct Hiding(MutableState<bool>);

    impl MeasurePolicy for Hiding {
        fn measure(
            &self,
            scope: &mut MeasureScope<'_>,
            measurables: &[Measurable],
            constraints: Constraints,
        ) -> MeasureResult {
            let loose = constraints.loosen();
            for &measurable in measurables {
                let placeable = scope.measure(measurable, loose);
                if self.0.get() {
                    scope.place(&placeable, 0, 0);
                }
            }
            scope.layout(20, 20)
        }
    }

    /// A clickable drawing a state layer for `source`, a boundary of its own if `boundary`, in
    /// a parent placing it only while `shown`.
    fn host_of(
        source: &MutableInteractionSource,
        shown: MutableState<bool>,
        boundary: bool,
    ) -> UiHost {
        let source = source.clone();
        let host = UiHost::headless(100, 100);
        host.set_content(move || {
            let source = source.clone();
            let policy = remember(|| Rc::new(Hiding(shown)) as Rc<dyn MeasurePolicy>);
            Layout(
                Modifier,
                policy,
                callback(move || {
                    let modifier = if boundary { Modifier.repaint_boundary() } else { Modifier };
                    let options = ClickableOptions {
                        interaction_source: Some(source.clone()),
                        indication: Indication::bounded(Color::RED),
                        ..ClickableOptions::DEFAULT
                    };
                    crate::layout::emit_layout_node(
                        modifier.clickable_with(options, || {}).size(10.dp()),
                        crate::layout::empty_measure_policy(),
                        || {},
                    );
                }),
            );
        });
        host
    }

    fn shaded(host: &UiHost) -> bool {
        host.display_list().dump().contains("FF0000")
    }

    fn requests(host: &UiHost) -> usize {
        host.with_tree(|tree| tree.focus_requests.node_invalidations.borrow().len())
    }

    /// A press and its release before the node draws ask to draw once, and once it has drawn,
    /// the next interaction asks again.
    #[test]
    fn interactions_before_a_draw_ask_once_and_the_draw_lets_the_next_ask() {
        let source = MutableInteractionSource::new();
        let shown = mutable_state_of(true);
        let mut host = host_of(&source, shown.handle(), false);
        host.frame();
        assert!(!shaded(&host));
        let press = Press::new(Offset::ZERO);
        source.try_emit(PressInteraction::Press(press));
        assert_eq!(requests(&host), 1);
        source.try_emit(PressInteraction::Release(press));
        assert_eq!(requests(&host), 1, "a press and its release ask once");
        host.frame();
        assert!(!shaded(&host), "nothing under way when it drew");
        let press = Press::new(Offset::ZERO);
        source.try_emit(PressInteraction::Press(press));
        assert_eq!(requests(&host), 1, "it drew: the next change asks");
        host.frame();
        assert!(shaded(&host));
        source.try_emit(PressInteraction::Release(press));
        host.frame();
        assert!(!shaded(&host));
    }

    /// An interaction on a node nothing draws (its parent places it no more) wakes no endless
    /// frames, and what it asked isn't lost: placed again, the node draws what's under way, and
    /// its next interactions draw again.
    fn a_node_not_drawn_draws_what_changed_when_it_is(boundary: bool) {
        let source = MutableInteractionSource::new();
        let shown = mutable_state_of(true);
        let mut host = host_of(&source, shown.handle(), boundary);
        host.frame();
        shown.set(false);
        host.frame();
        assert!(!shaded(&host));

        let press = Press::new(Offset::ZERO);
        source.try_emit(PressInteraction::Press(press));
        assert!(host.settle(10) < 10, "a node not drawn asks for no endless frames");
        assert!(!shaded(&host));
        assert!(!host.needs_frame());
        // A second change while it isn't drawn: nothing more to ask.
        source.try_emit(HoverInteraction::Enter(crate::interaction::HoverEnter::new()));
        assert_eq!(requests(&host), 0);

        shown.set(true);
        host.frame();
        assert!(shaded(&host), "placed again, it draws the press\n{}", host.display_list().dump());
        assert!(host.settle(10) < 10);

        source.try_emit(PressInteraction::Release(press));
        assert_eq!(requests(&host), 1, "it drew: its next change asks again");
    }

    #[test]
    fn a_boundary_not_placed_draws_what_changed_when_placed_again() {
        a_node_not_drawn_draws_what_changed_when_it_is(true);
    }

    #[test]
    fn a_node_not_placed_draws_what_changed_when_placed_again() {
        a_node_not_drawn_draws_what_changed_when_it_is(false);
    }

    /// A boundary not placed keeps its group dirty until a frame draws it, then it's clean: what
    /// a request on it left isn't dirty for good.
    #[test]
    fn a_request_on_a_boundary_not_placed_leaves_its_group_dirty_until_it_draws() {
        fn dirty(host: &UiHost) -> Vec<bool> {
            host.with_tree(|tree| {
                let mut stack = vec![tree.root_id()];
                let mut dirty = Vec::new();
                while let Some(id) = stack.pop() {
                    let value = tree.node(id);
                    if value.chain.iter().any(|entry| {
                        entry.node.downcast_ref::<crate::clickable::ClickableNode>().is_some()
                    }) {
                        dirty.push(value.retained.as_ref().expect("a boundary").dirty);
                    }
                    stack.extend(tree.children_of(id));
                }
                dirty
            })
        }
        let source = MutableInteractionSource::new();
        let shown = mutable_state_of(true);
        let mut host = host_of(&source, shown.handle(), true);
        host.frame();
        shown.set(false);
        host.frame();
        assert_eq!(dirty(&host), [false]);
        source.try_emit(PressInteraction::Press(Press::new(Offset::ZERO)));
        host.settle(10);
        assert_eq!(dirty(&host), [true], "drawn when a frame places it (D435)");
        shown.set(true);
        host.frame();
        assert_eq!(dirty(&host), [false]);
    }

    /// What a node heard since it was made, before it was attached (a paused composition's, drawn
    /// ahead, attached when it's applied), draws when it's attached: it had no handle to ask
    /// through, and nothing else says so.
    #[test]
    fn what_a_node_heard_before_it_was_attached_draws_once_it_is() {
        let source = MutableInteractionSource::new();
        let given = source.clone();
        let mut host = UiHost::headless(100, 100);
        host.set_content(move || {
            crate::layout::emit_layout_node(
                // A boundary of its own: only what asks draws it again.
                Modifier
                    .repaint_boundary()
                    .indication(given.clone(), Indication::bounded(Color::RED))
                    .size(10.dp()),
                crate::layout::empty_measure_policy(),
                || {},
            );
        });
        host.frame();
        assert!(!shaded(&host));
        host.with_tree(|tree| {
            let mut stack = vec![tree.root_id()];
            while let Some(id) = stack.pop() {
                let found =
                    tree.node(id).chain.iter().position(|entry| {
                        entry.node.downcast_ref::<IndicationHostNode>().is_some()
                    });
                if let Some(index) = found {
                    tree.with_entry(id, index, |node, _| {
                        let host = node.downcast_mut::<IndicationHostNode>().expect("the host");
                        // A new one made and not yet attached, which hears a press.
                        let factory = state_layer_factory(Color::RED, None);
                        host.delegates.undelegate_boxed(host.node.take().expect("its node"));
                        let new = factory.create(source.as_source().clone());
                        host.node = Some(host.delegates.delegate_boxed(new));
                        source.try_emit(PressInteraction::Press(Press::new(Offset::ZERO)));
                    });
                    return;
                }
                stack.extend(tree.children_of(id));
            }
            panic!("no indication");
        });
        host.frame();
        assert!(shaded(&host), "it draws the press\n{}", host.display_list().dump());
    }

    /// Content moved under a resting mouse: the hover the frame's layout finds is drawn by the
    /// clickable's own boundary in that frame (D1053), asked through its handle or not.
    #[test]
    fn a_hover_the_layout_finds_draws_in_that_frame() {
        let source = MutableInteractionSource::new();
        let given = source.clone();
        let gap = mutable_state_of(20);
        let gap_handle = gap.handle();
        let mut host = UiHost::headless(100, 100);
        let policy = |gap: MutableState<i32>| {
            Rc::new(
                move |scope: &mut MeasureScope<'_>,
                      measurables: &[Measurable],
                      constraints: Constraints| {
                    let gap = gap.get();
                    for &measurable in measurables {
                        let placeable = scope.measure(measurable, constraints.loosen());
                        scope.place(&placeable, 0, gap);
                    }
                    scope.layout(100, 100)
                },
            ) as Rc<dyn MeasurePolicy>
        };
        host.set_content(move || {
            let given = given.clone();
            let policy = remember(|| policy(gap_handle));
            Layout(
                Modifier,
                policy,
                callback(move || {
                    let options = ClickableOptions {
                        interaction_source: Some(given.clone()),
                        indication: Indication::bounded(Color::RED),
                        ..ClickableOptions::DEFAULT
                    };
                    crate::layout::emit_layout_node(
                        Modifier.repaint_boundary().clickable_with(options, || {}).size(10.dp()),
                        crate::layout::empty_measure_policy(),
                        || {},
                    );
                }),
            );
        });
        host.frame();
        host.send_pointer(crate::input::PointerEvent::moved(Offset::new(5.0, 5.0), false));
        host.frame();
        assert!(!shaded(&host));
        gap.set(0);
        host.frame();
        assert!(source.is_hovered(), "the layout found the hover");
        assert!(shaded(&host), "not in that frame\n{}", host.display_list().dump());
    }
}
