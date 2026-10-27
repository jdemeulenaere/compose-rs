//! Indications, as Compose's `Indication`, `IndicationNodeFactory`, `LocalIndication` and
//! `Modifier.indication`: what shows a component's interactions (a press, a hover, a focus),
//! drawn by a node a factory makes for the component's interaction source.

use std::any::Any;
use std::fmt;
use std::rc::Rc;

use compose_runtime::{CompositionLocal, OwnedState};

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
/// [`Indication::bounded`] and [`Indication::unbounded`] are compose-rs's state layer (Material's
/// ripple at rest, without its animation), and a `Color` converts to a bounded one.
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
    /// hovered (8 %), as Material's bounded ripple settles at; an unspecified color is no
    /// indication.
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
    match LOCAL_INDICATION.current().0 {
        Kind::None => None,
        Kind::Factory(factory) => Some(factory),
        Kind::Local => Some(default_debug_indication()),
    }
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
/// a state its listener writes as each is emitted (D1057: no collecting task) and its drawing
/// reads. Each interaction is tracked by identity, as `collect_is_pressed_as_state`'s, so an
/// end without its start (a start dropped with a detached node's queue) changes nothing. It
/// listens from when it's made until its node is detached ([`stop`](Self::stop)), and again
/// from its node's next attachment ([`start`](Self::start)).
pub(crate) struct InteractionStates {
    source: InteractionSource,
    /// Bits: `PRESSED`, `HOVERED`, `FOCUSED`.
    state: OwnedState<u8>,
    listener: Option<Rc<Listener>>,
}

pub(crate) const PRESSED: u8 = 1;
pub(crate) const HOVERED: u8 = 2;
pub(crate) const FOCUSED: u8 = 4;

impl InteractionStates {
    /// Listens to `source`'s interactions from now on, as Compose's indication nodes collect
    /// them from their attachment.
    pub(crate) fn collect(source: &InteractionSource) -> InteractionStates {
        let mut states = InteractionStates {
            source: source.clone(),
            state: compose_runtime::mutable_state_of(0u8),
            listener: None,
        };
        states.start();
        states
    }

    /// Listens from now on (from nothing under way, as a new collection), unless it does.
    pub(crate) fn start(&mut self) {
        if self.listener.is_some() {
            return;
        }
        let handle = self.state.handle();
        let mut ongoing = Ongoing::default();
        let listener = self.source.listen(move |interaction| {
            ongoing.apply(interaction);
            let bits = if ongoing.pressed() { PRESSED } else { 0 }
                | if ongoing.hovered() { HOVERED } else { 0 }
                | if ongoing.focused() { FOCUSED } else { 0 };
            if handle.try_get() == Some(bits) {
                return;
            }
            // An emission while composing (a node's update ending its interactions) writes
            // after the composition, as a collecting task would.
            if compose_runtime::is_composing() {
                compose_runtime::platform::launch(async move {
                    let _ = handle.try_set(bits);
                });
            } else {
                let _ = handle.try_set(bits);
            }
        });
        self.listener = Some(listener);
    }

    /// The bits, a state read.
    pub(crate) fn get(&self) -> u8 {
        self.state.try_get().unwrap_or(0)
    }

    /// Stops listening, and shows nothing under way: its node is detached, as Compose's node
    /// scope cancelled (attached again, it starts from nothing, as a new collection).
    pub(crate) fn stop(&mut self) {
        self.listener = None;
        crate::interaction::write_soon(&self.state, 0);
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
    fn on_attach(&mut self, _context: &mut NodeContext<'_>) {
        self.0.start();
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
    fn on_attach(&mut self, _context: &mut NodeContext<'_>) {
        self.states.start();
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

/// Hosts the factory's node, made again for a new source or a factory not the same.
pub(crate) struct IndicationHostNode {
    node: Box<dyn ModifierNode>,
    /// Whether the node draws its content itself, asked when it was made.
    draws_content: bool,
    /// Whether `node` is attached (attached with its host, as Compose's delegate).
    attached: bool,
    /// Nodes replaced while attached, detached by `run_lifecycle`.
    retired: Vec<Box<dyn ModifierNode>>,
    /// The host's handle while it's attached, for `run_lifecycle` from a task (an update has
    /// no context: MN-5's delegates will do this at once).
    handle: Option<NodeHandle>,
    lifecycle_scheduled: bool,
}

impl IndicationHostNode {
    pub(crate) fn new(mut node: Box<dyn ModifierNode>) -> IndicationHostNode {
        let draws_content = node.as_draw().is_some_and(|draw| draw.draws_content());
        IndicationHostNode {
            node,
            draws_content,
            attached: false,
            retired: Vec::new(),
            handle: None,
            lifecycle_scheduled: false,
        }
    }

    /// Hosts `node` instead, the old one detached soon, as Compose's `undelegate`.
    fn replace(&mut self, mut node: Box<dyn ModifierNode>) {
        self.draws_content = node.as_draw().is_some_and(|draw| draw.draws_content());
        let old = std::mem::replace(&mut self.node, node);
        if std::mem::take(&mut self.attached) {
            self.retired.push(old);
        }
        let Some(handle) = self.handle else { return };
        if std::mem::replace(&mut self.lifecycle_scheduled, true) {
            return;
        }
        compose_runtime::platform::launch(async move {
            handle.with_node(|host: &mut IndicationHostNode, context| {
                host.lifecycle_scheduled = false;
                host.run_lifecycle(context);
            });
        });
    }

    fn run_lifecycle(&mut self, context: &mut NodeContext<'_>) {
        for mut node in self.retired.drain(..) {
            node.on_detach(context);
        }
        if context.is_attached() && !self.attached {
            self.node.on_attach(context);
            self.attached = true;
        }
    }
}

impl ModifierNodeElement for IndicationElement {
    type Node = IndicationHostNode;

    fn create(&self) -> IndicationHostNode {
        IndicationHostNode::new(self.factory.create(self.source.clone()))
    }

    fn update(&self, node: &mut IndicationHostNode) {
        node.replace(self.factory.create(self.source.clone()));
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
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn observes_reads(&self) -> bool {
        true
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.handle = Some(context.handle());
        self.run_lifecycle(context);
    }
    fn on_detach(&mut self, context: &mut NodeContext<'_>) {
        if std::mem::take(&mut self.attached) {
            self.node.on_detach(context);
        }
        for mut node in self.retired.drain(..) {
            node.on_detach(context);
        }
        self.handle = None;
    }
    fn on_reset(&mut self, context: &mut NodeContext<'_>) {
        self.node.on_reset(context);
    }
    fn on_release(&mut self, context: &mut crate::modifier::ReleaseContext<'_>) {
        self.node.on_release(context);
    }
}

/// Draws as the hosted node does.
impl DrawModifierNode for IndicationHostNode {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        if let Some(draw) = self.node.as_draw() {
            draw.draw(scope);
        }
    }
    fn draw_after(&mut self, scope: &mut DrawScope<'_>) {
        if let Some(draw) = self.node.as_draw() {
            draw.draw_after(scope);
        }
    }
    fn draws_content(&self) -> bool {
        self.draws_content
    }
    fn draw_with_content(&mut self, scope: &mut crate::draw_modifiers::ContentDrawScope<'_, '_>) {
        if let Some(draw) = self.node.as_draw() {
            draw.draw_with_content(scope);
        }
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
