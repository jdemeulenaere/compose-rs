//! Material's ripple, as Compose's `ripple`, `LocalRippleConfiguration`,
//! `LocalRippleThemeConfiguration` and its `material3-ripple` module: a press grows a circle from
//! where it went down, hovers, focuses and drags show a state layer, or a focus an inset ring,
//! through an `IndicationNodeFactory` (D1025). compose-rs has one node everywhere, Compose's
//! `CommonRippleNode`; Android's draws through a `RippleDrawable`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::task::Waker;

use compose_animation::{Animatable, AnimationSpec};
use compose_runtime::{ComposeArg, CompositionLocal, CompositionLocalMap, CoroutineScope};
use compose_ui::{
    BorderRenderer, Brush, Color, Dp, DragInteraction, DragStart, DrawModifierNode, DrawScope,
    Focus, FocusInteraction, HoverEnter, HoverInteraction, Indication, IndicationNodeFactory,
    Interaction, InteractionSource, Listener, ModifierNode, NodeContext, NodeHandle, Offset,
    Outline, Press, PressInteraction, Rect, Shape,
};

use crate::theme::{LOCAL_COLOR_SCHEME, LOCAL_CONTENT_COLOR};

/// `StateTokens`' opacities.
const PRESSED_ALPHA: f32 = 0.1;
const FOCUS_ALPHA: f32 = 0.1;
const HOVER_ALPHA: f32 = 0.08;
const DRAGGED_ALPHA: f32 = 0.16;

/// A press's ripple, as `RippleAnimation`: how long it fades in, grows and fades out.
const FADE_IN_MILLIS: f32 = 75.0;
const RADIUS_MILLIS: f32 = 225.0;
const FADE_OUT_MILLIS: f32 = 150.0;
/// What a bounded ripple grows past its bounds' corners, as `BoundedRippleExtraRadius`.
const BOUNDED_EXTRA_RADIUS: Dp = Dp(10.0);

/// Where a ripple starts, as `getRippleStartRadius`: 30 % of the longer side.
pub fn ripple_start_radius(width: f32, height: f32) -> f32 {
    width.max(height) * 0.3
}

/// How far a ripple grows, as `getRippleEndRadius`: past the corners of its bounds, 10 dp more
/// when bounded.
pub fn ripple_end_radius(
    density: &compose_ui::Density,
    bounded: bool,
    width: f32,
    height: f32,
) -> f32 {
    let covering = (width * width + height * height).sqrt() / 2.0;
    if bounded { covering + density.to_px(BOUNDED_EXTRA_RADIUS) } else { covering }
}

/// The configuration of the ripples below it, as Compose's `RippleConfiguration`: `None` in
/// [`LOCAL_RIPPLE_CONFIGURATION`] turns them off. The deprecated `rippleAlpha` isn't here.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RippleConfiguration {
    /// The ripples' color, when one doesn't give its own; unspecified is `LOCAL_CONTENT_COLOR`.
    pub color: Color,
    /// How a focus shows, what [`LOCAL_RIPPLE_THEME_CONFIGURATION`] says being the rest.
    pub focus: Option<RippleConfigurationFocus>,
}

impl Default for RippleConfiguration {
    fn default() -> Self {
        RippleConfiguration { color: Color::UNSPECIFIED, focus: None }
    }
}

/// A [`RippleConfiguration`]'s focus, as `RippleConfiguration.Focus`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RippleConfigurationFocus {
    Opacity,
    InsetRing { outer_stroke_color: Color, inner_stroke_color: Color },
}

/// How the theme shows a focus, as Compose's `RippleThemeConfiguration`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RippleThemeConfiguration {
    pub focus: RippleThemeFocus,
}

/// `RippleThemeConfiguration.Focus`: a state layer, or a ring inside the shape.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RippleThemeFocus {
    Opacity,
    InsetRing {
        outer_stroke_inset: Dp,
        outer_stroke_width: Dp,
        inner_stroke_inset: Dp,
        inner_stroke_width: Dp,
    },
}

impl ComposeArg for RippleConfiguration {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl ComposeArg for RippleThemeConfiguration {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// As Compose's `RippleDefaults`.
pub struct RippleDefaults;

impl RippleDefaults {
    /// A focus as a state layer, as `OpacityFocusThemeConfiguration`.
    pub const OPACITY_FOCUS_THEME_CONFIGURATION: RippleThemeConfiguration =
        RippleThemeConfiguration { focus: RippleThemeFocus::Opacity };
    /// A focus as an inset ring, as `InsetFocusRingThemeConfiguration`.
    pub const INSET_FOCUS_RING_THEME_CONFIGURATION: RippleThemeConfiguration =
        RippleThemeConfiguration {
            focus: RippleThemeFocus::InsetRing {
                outer_stroke_inset: Dp(0.0),
                outer_stroke_width: Dp(2.0),
                inner_stroke_inset: Dp(1.0),
                inner_stroke_width: Dp(3.0),
            },
        };
    /// As `ThemeConfiguration`.
    pub const THEME_CONFIGURATION: RippleThemeConfiguration =
        RippleDefaults::OPACITY_FOCUS_THEME_CONFIGURATION;
}

/// The ripples' configuration below it, as `LocalRippleConfiguration`; `None` turns them off.
pub static LOCAL_RIPPLE_CONFIGURATION: CompositionLocal<Option<RippleConfiguration>> =
    CompositionLocal::new("LocalRippleConfiguration", || Some(RippleConfiguration::default()));

/// How the theme below it shows a focus, as `LocalRippleThemeConfiguration`.
pub static LOCAL_RIPPLE_THEME_CONFIGURATION: CompositionLocal<RippleThemeConfiguration> =
    CompositionLocal::new("LocalRippleThemeConfiguration", || RippleDefaults::THEME_CONFIGURATION);

/// A ripple's options, as `ripple`'s parameters: [`ripple_with`].
#[derive(Clone)]
pub struct RippleOptions {
    /// Whether it's clipped to the bounds.
    pub bounded: bool,
    /// Its full radius; unspecified grows it to cover the bounds.
    pub radius: Dp,
    /// Its color; unspecified is the [`RippleConfiguration`]'s, then `LOCAL_CONTENT_COLOR`.
    pub color: Color,
    /// Its color, read as it draws, as the `ColorProducer` overloads; wins over `color`.
    pub color_producer: Option<Rc<dyn Fn() -> Color>>,
    /// The inset focus ring's shape; `None` is rounded by `radius`, or a rectangle.
    pub focus_ring_shape: Option<Shape>,
    pub enable_press_indication: bool,
    pub enable_focus_indication: bool,
    pub enable_hover_indication: bool,
    pub enable_drag_indication: bool,
}

impl Default for RippleOptions {
    fn default() -> Self {
        RippleOptions {
            bounded: true,
            radius: Dp::UNSPECIFIED,
            color: Color::UNSPECIFIED,
            color_producer: None,
            focus_ring_shape: None,
            enable_press_indication: true,
            enable_focus_indication: true,
            enable_hover_indication: true,
            enable_drag_indication: true,
        }
    }
}

/// A ripple, as `ripple(bounded, radius, color)`.
pub fn ripple(bounded: bool, radius: Dp, color: Color) -> Indication {
    ripple_with(RippleOptions { bounded, radius, color, ..RippleOptions::default() })
}

/// A ripple of all its options, as `ripple`'s other parameters and overloads.
pub fn ripple_with(options: RippleOptions) -> Indication {
    let RippleOptions { bounded, radius, color, color_producer, .. } = &options;
    let plain = !radius.is_specified()
        && !color.is_specified()
        && color_producer.is_none()
        && options.focus_ring_shape.is_none()
        && options.enable_press_indication
        && options.enable_focus_indication
        && options.enable_hover_indication
        && options.enable_drag_indication;
    if plain {
        return if *bounded { default_bounded() } else { default_unbounded() };
    }
    Indication::new(RippleNodeFactory::new(options))
}

/// The default bounded ripple, shared, as `ripple()`.
pub fn default_ripple() -> Indication {
    default_bounded()
}

/// A default ripple whose inset focus ring is `shape`, as `ripple(focusRingShape = shape)`.
pub(crate) fn shaped_ripple(shape: Shape) -> Indication {
    ripple_with(RippleOptions { focus_ring_shape: Some(shape), ..RippleOptions::default() })
}

/// An unbounded ripple of `radius`, as `ripple(bounded = false, radius = radius)`.
pub(crate) fn unbounded_ripple(radius: Dp) -> Indication {
    ripple(false, radius, Color::UNSPECIFIED)
}

/// `source`'s interactions with its presses moved by `delta` into another node's coordinates, as
/// `MappedInteractionSource`: only a pill in an item shows the item's ripple. For as long as it's
/// held.
pub(crate) struct MappedInteractionSource {
    mapped: compose_ui::MutableInteractionSource,
    _listener: Rc<Listener>,
}

impl MappedInteractionSource {
    pub(crate) fn new(
        source: &InteractionSource,
        delta: impl Fn() -> Offset + 'static,
    ) -> MappedInteractionSource {
        let mapped = compose_ui::MutableInteractionSource::new();
        let target = mapped.clone();
        let mut presses: Vec<(Press, Press)> = Vec::new();
        let listener = source.listen(move |interaction| match interaction {
            Interaction::Press(PressInteraction::Press(press)) => {
                let moved = Press::new(press.press_position - delta());
                presses.push((*press, moved));
                target.try_emit(PressInteraction::Press(moved));
            }
            Interaction::Press(
                end @ (PressInteraction::Release(press) | PressInteraction::Cancel(press)),
            ) => {
                if let Some(index) = presses.iter().position(|(from, _)| from == press) {
                    let (_, moved) = presses.remove(index);
                    target.try_emit(match end {
                        PressInteraction::Release(_) => PressInteraction::Release(moved),
                        _ => PressInteraction::Cancel(moved),
                    });
                }
            }
            other => {
                target.try_emit(other.clone());
            }
        });
        MappedInteractionSource { mapped, _listener: listener }
    }

    pub(crate) fn source(&self) -> compose_ui::MutableInteractionSource {
        self.mapped.clone()
    }
}

thread_local! {
    static DEFAULT_BOUNDED: Indication =
        Indication::new(RippleNodeFactory::new(RippleOptions::default()));
    static DEFAULT_UNBOUNDED: Indication = Indication::new(RippleNodeFactory::new(RippleOptions {
        bounded: false,
        ..RippleOptions::default()
    }));
}

fn default_bounded() -> Indication {
    DEFAULT_BOUNDED.with(Clone::clone)
}

fn default_unbounded() -> Indication {
    DEFAULT_UNBOUNDED.with(Clone::clone)
}

/// As Compose's `RippleNodeFactory`, with the locals where the clickable's node is.
#[derive(Clone)]
struct RippleNodeFactory {
    bounded: bool,
    radius: Dp,
    color: Color,
    color_producer: Option<Rc<dyn Fn() -> Color>>,
    focus_ring_shape: Shape,
    enable_press: bool,
    enable_focus: bool,
    enable_hover: bool,
    enable_drag: bool,
    locals: CompositionLocalMap,
}

impl RippleNodeFactory {
    fn new(options: RippleOptions) -> RippleNodeFactory {
        let focus_ring_shape = options.focus_ring_shape.unwrap_or_else(|| {
            if options.radius.is_specified() {
                Shape::Rounded([compose_ui::CornerSize::Dp(options.radius); 4])
            } else {
                compose_ui::RECTANGLE_SHAPE
            }
        });
        RippleNodeFactory {
            bounded: options.bounded,
            radius: options.radius,
            color: options.color,
            color_producer: options.color_producer,
            focus_ring_shape,
            enable_press: options.enable_press_indication,
            enable_focus: options.enable_focus_indication,
            enable_hover: options.enable_hover_indication,
            enable_drag: options.enable_drag_indication,
            locals: CompositionLocalMap::empty(),
        }
    }
}

impl IndicationNodeFactory for RippleNodeFactory {
    fn create(&self, interaction_source: InteractionSource) -> Box<dyn ModifierNode> {
        let shared =
            Rc::new(Shared { factory: RippleNodeFactoryCell(self.clone()), ..Shared::default() });
        let mut node = RippleNode { source: interaction_source, shared, listener: None };
        node.start();
        Box::new(node)
    }

    fn same(&self, other: &dyn IndicationNodeFactory) -> bool {
        let Some(other) = other.downcast_ref::<RippleNodeFactory>() else { return false };
        self.bounded == other.bounded
            && (self.radius == other.radius
                || (!self.radius.is_specified() && !other.radius.is_specified()))
            && self.color == other.color
            && match (&self.color_producer, &other.color_producer) {
                (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && self.focus_ring_shape == other.focus_ring_shape
            && self.enable_press == other.enable_press
            && self.enable_focus == other.enable_focus
            && self.enable_hover == other.enable_hover
            && self.enable_drag == other.enable_drag
            && self.locals.same(&other.locals)
    }

    fn at_node(
        &self,
        locals: &mut dyn FnMut() -> CompositionLocalMap,
    ) -> Option<Rc<dyn IndicationNodeFactory>> {
        Some(Rc::new(RippleNodeFactory { locals: locals(), ..self.clone() }))
    }
}

/// A press's ripple, as `RippleAnimation`, its three animations one task's.
struct RippleAnimation {
    press: Press,
    /// Where it starts, in the node: the press, or (unbounded) the center, made at the first
    /// draw.
    origin: Option<Offset>,
    /// Its radii, made at the first draw, the size known.
    radii: Option<(f32, f32)>,
    alpha: f32,
    radius_fraction: f32,
    center_fraction: f32,
    finished_fading_in: bool,
    finish_requested: bool,
    /// What wakes its task, waiting to fade out.
    waker: Option<Waker>,
}

impl RippleAnimation {
    fn finish(&mut self) {
        self.finish_requested = true;
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}

/// What `InteractionSource::listen`'s callback, the tasks and the node's draw share.
#[derive(Default)]
struct Shared {
    factory: RippleNodeFactoryCell,
    handle: Cell<Option<NodeHandle>>,
    ripples: RefCell<Vec<Rc<RefCell<RippleAnimation>>>>,
    /// What was heard before the node's attachment gave it a scope.
    pending: RefCell<Vec<Interaction>>,
    /// The hovers, focuses and drags under way, oldest first; the last shows.
    interactions: RefCell<Vec<Interaction>>,
    current: RefCell<Option<Interaction>>,
    is_focused: Cell<bool>,
    /// The state layer's alpha, and what animates it now (a new animation replaces it).
    state_layer_alpha: Cell<f32>,
    state_layer_generation: Cell<u32>,
    /// The inset ring's interpolation, made when a ring first shows.
    focus_ring: RefCell<Option<Animatable<f32>>>,
}

/// The factory, a `Default` for [`Shared`]'s derive.
struct RippleNodeFactoryCell(RippleNodeFactory);

impl Default for RippleNodeFactoryCell {
    fn default() -> Self {
        RippleNodeFactoryCell(RippleNodeFactory::new(RippleOptions::default()))
    }
}

struct RippleNode {
    source: InteractionSource,
    shared: Rc<Shared>,
    listener: Option<Rc<Listener>>,
}

impl RippleNode {
    /// Listens from now on (from nothing under way, as a new collection), unless it does.
    fn start(&mut self) {
        if self.listener.is_some() {
            return;
        }
        let shared = self.shared.clone();
        self.listener = Some(self.source.listen(move |interaction| {
            if shared.handle.get().is_some() {
                shared.handle_interaction(interaction);
            } else {
                shared.pending.borrow_mut().push(interaction.clone());
            }
        }));
    }
}

impl ModifierNode for RippleNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }

    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.start();
        self.shared.handle.set(Some(context.handle()));
        let pending = std::mem::take(&mut *self.shared.pending.borrow_mut());
        for interaction in &pending {
            self.shared.handle_interaction(interaction);
        }
    }

    /// As `onDetach`: its scope goes, and all with it; attached again, it starts from nothing.
    fn on_detach(&mut self, _context: &mut NodeContext<'_>) {
        self.listener = None;
        self.shared.clear();
        self.shared.handle.set(None);
    }
}

impl Shared {
    fn factory(&self) -> &RippleNodeFactory {
        &self.factory.0
    }

    fn scope(&self) -> Option<CoroutineScope> {
        self.handle.get().and_then(|handle| handle.coroutine_scope())
    }

    fn invalidate(&self) {
        if let Some(handle) = self.handle.get() {
            handle.invalidate_draw();
        }
    }

    /// What's under way is dropped, as a node removed.
    fn clear(&self) {
        self.ripples.borrow_mut().clear();
        self.pending.borrow_mut().clear();
        self.interactions.borrow_mut().clear();
        *self.current.borrow_mut() = None;
        self.is_focused.set(false);
        self.state_layer_alpha.set(0.0);
        self.state_layer_generation.set(self.state_layer_generation.get().wrapping_add(1));
        *self.focus_ring.borrow_mut() = None;
    }

    fn configuration(&self) -> Option<RippleConfiguration> {
        self.factory().locals.get(&LOCAL_RIPPLE_CONFIGURATION)
    }

    /// As the node's collection of its source's interactions.
    fn handle_interaction(self: &Rc<Self>, interaction: &Interaction) {
        // Off (`None`), the node is as removed: it hears nothing.
        let Some(configuration) = self.configuration() else {
            self.clear();
            return;
        };
        let was_focused = self.is_focused.get();
        {
            let mut interactions = self.interactions.borrow_mut();
            match interaction {
                Interaction::Press(press) => {
                    drop(interactions);
                    self.handle_press(press);
                    return;
                }
                Interaction::Hover(HoverInteraction::Enter(_))
                | Interaction::Drag(DragInteraction::Start(_)) => {
                    interactions.push(interaction.clone());
                }
                Interaction::Focus(FocusInteraction::Focus(_)) => {
                    interactions.push(interaction.clone());
                    self.is_focused.set(true);
                }
                Interaction::Hover(HoverInteraction::Exit(enter)) => {
                    remove_hover(&mut interactions, *enter);
                }
                Interaction::Focus(FocusInteraction::Unfocus(focus)) => {
                    remove_focus(&mut interactions, *focus);
                    if !interactions.iter().any(|it| matches!(it, Interaction::Focus(_))) {
                        self.is_focused.set(false);
                    }
                }
                Interaction::Drag(DragInteraction::Stop(start))
                | Interaction::Drag(DragInteraction::Cancel(start)) => {
                    remove_drag(&mut interactions, *start);
                }
                _ => return,
            }
        }
        // The most recent interaction is the one that shows.
        let new = self.interactions.borrow().last().cloned();
        let current = self.current.borrow().clone();
        if current == new {
            return;
        }
        let theme = self.factory().locals.get(&LOCAL_RIPPLE_THEME_CONFIGURATION);
        match &new {
            Some(new) => {
                let target = self.state_layer_target(new, theme);
                let millis = match new {
                    Interaction::Focus(_) | Interaction::Drag(_) => 45.0,
                    _ => 15.0,
                };
                self.animate_state_layer(target, millis);
            }
            None => {
                let millis =
                    if matches!(current, Some(Interaction::Drag(_))) { 150.0 } else { 15.0 };
                self.animate_state_layer(0.0, millis);
            }
        }
        if matches!(interaction, Interaction::Focus(_)) {
            let ring = self.factory().enable_focus
                && matches!(
                    self.focus_kind(configuration, theme),
                    RippleThemeFocus::InsetRing { .. }
                );
            if ring {
                let focused = self.is_focused.get();
                if focused != was_focused {
                    self.animate_focus_ring(focused);
                }
            } else {
                *self.focus_ring.borrow_mut() = None;
            }
        }
        *self.current.borrow_mut() = new;
    }

    /// What shows a focus: the theme's, as `calculateRippleNodeConfiguration`.
    fn focus_kind(
        &self,
        _configuration: RippleConfiguration,
        theme: RippleThemeConfiguration,
    ) -> RippleThemeFocus {
        theme.focus
    }

    fn state_layer_target(
        &self,
        interaction: &Interaction,
        theme: RippleThemeConfiguration,
    ) -> f32 {
        let factory = self.factory();
        match interaction {
            Interaction::Hover(_) if factory.enable_hover => HOVER_ALPHA,
            Interaction::Focus(_)
                if factory.enable_focus && theme.focus == RippleThemeFocus::Opacity =>
            {
                FOCUS_ALPHA
            }
            Interaction::Drag(_) if factory.enable_drag => DRAGGED_ALPHA,
            _ => 0.0,
        }
    }

    /// `addRipple` and `removeRipple`.
    fn handle_press(self: &Rc<Self>, press: &PressInteraction) {
        match press {
            PressInteraction::Press(press) => {
                for ripple in self.ripples.borrow().iter() {
                    ripple.borrow_mut().finish();
                }
                let origin = self.factory().bounded.then_some(press.press_position);
                let ripple = Rc::new(RefCell::new(RippleAnimation {
                    press: *press,
                    origin,
                    radii: None,
                    alpha: 0.0,
                    radius_fraction: 0.0,
                    center_fraction: 0.0,
                    finished_fading_in: false,
                    finish_requested: false,
                    waker: None,
                }));
                self.ripples.borrow_mut().push(ripple.clone());
                if let Some(scope) = self.scope() {
                    let shared = self.clone();
                    scope.launch(async move { shared.animate_ripple(ripple).await });
                }
                self.invalidate();
            }
            PressInteraction::Release(press) | PressInteraction::Cancel(press) => {
                for ripple in self.ripples.borrow().iter() {
                    let mut ripple = ripple.borrow_mut();
                    if ripple.press == *press {
                        ripple.finish();
                    }
                }
            }
        }
    }

    /// `RippleAnimation.animate`: the fade in and growth, waits to be finished, fades out, and
    /// goes.
    async fn animate_ripple(self: Rc<Self>, ripple: Rc<RefCell<RippleAnimation>>) {
        let start = compose_runtime::with_frame_nanos(|time| time).await;
        loop {
            let time = compose_runtime::with_frame_nanos(|time| time).await;
            let millis = (time - start) as f32 / 1_000_000.0;
            {
                let mut ripple = ripple.borrow_mut();
                ripple.alpha = (millis / FADE_IN_MILLIS).min(1.0);
                let fraction = (millis / RADIUS_MILLIS).min(1.0);
                ripple.radius_fraction =
                    compose_animation::FAST_OUT_SLOW_IN_EASING.transform(fraction);
                ripple.center_fraction = fraction;
            }
            self.invalidate();
            if millis >= RADIUS_MILLIS {
                break;
            }
        }
        ripple.borrow_mut().finished_fading_in = true;
        std::future::poll_fn(|context| {
            let mut ripple = ripple.borrow_mut();
            if ripple.finish_requested {
                std::task::Poll::Ready(())
            } else {
                ripple.waker = Some(context.waker().clone());
                std::task::Poll::Pending
            }
        })
        .await;
        let start = compose_runtime::with_frame_nanos(|time| time).await;
        loop {
            let time = compose_runtime::with_frame_nanos(|time| time).await;
            let millis = (time - start) as f32 / 1_000_000.0;
            ripple.borrow_mut().alpha = (1.0 - millis / FADE_OUT_MILLIS).max(0.0);
            self.invalidate();
            if millis >= FADE_OUT_MILLIS {
                break;
            }
        }
        self.ripples.borrow_mut().retain(|it| !Rc::ptr_eq(it, &ripple));
        self.invalidate();
    }

    /// `animatedAlpha.animateTo(target, tween(millis, LinearEasing))`, a new one replacing the
    /// one under way.
    fn animate_state_layer(self: &Rc<Self>, target: f32, millis: f32) {
        let generation = self.state_layer_generation.get().wrapping_add(1);
        self.state_layer_generation.set(generation);
        let Some(scope) = self.scope() else { return };
        let shared = self.clone();
        scope.launch(async move {
            let from = shared.state_layer_alpha.get();
            let start = compose_runtime::with_frame_nanos(|time| time).await;
            loop {
                let time = compose_runtime::with_frame_nanos(|time| time).await;
                if shared.state_layer_generation.get() != generation {
                    return;
                }
                let fraction = ((time - start) as f32 / 1_000_000.0 / millis).min(1.0);
                shared.state_layer_alpha.set(from + (target - from) * fraction);
                shared.invalidate();
                if fraction >= 1.0 {
                    return;
                }
            }
        });
    }

    /// The ring in or out, on the motion scheme's fast spatial and effects springs.
    fn animate_focus_ring(self: &Rc<Self>, focused: bool) {
        let Some(scope) = self.scope() else { return };
        let ring = self.focus_ring.borrow_mut().get_or_insert_with(|| Animatable::new(0.0)).clone();
        let spec = AnimationSpec::Spring(if focused {
            crate::motion::FAST_SPATIAL
        } else {
            crate::motion::FAST_EFFECTS
        });
        let target = if focused { 1.0 } else { 0.0 };
        scope.launch(async move {
            let _ = ring.animate_to(target, spec).await;
        });
        self.invalidate();
    }

    /// The ripple's color, as `calculateColor`.
    fn color(&self, configuration: RippleConfiguration) -> Color {
        let factory = self.factory();
        let own = match &factory.color_producer {
            Some(producer) => producer(),
            None => factory.color,
        };
        if own.is_specified() {
            own
        } else if configuration.color.is_specified() {
            configuration.color
        } else {
            factory.locals.get(&LOCAL_CONTENT_COLOR)
        }
    }
}

fn remove_hover(interactions: &mut Vec<Interaction>, enter: HoverEnter) {
    interactions.retain(|it| *it != Interaction::Hover(HoverInteraction::Enter(enter)));
}

fn remove_focus(interactions: &mut Vec<Interaction>, focus: Focus) {
    interactions.retain(|it| *it != Interaction::Focus(FocusInteraction::Focus(focus)));
}

fn remove_drag(interactions: &mut Vec<Interaction>, start: DragStart) {
    interactions.retain(|it| *it != Interaction::Drag(DragInteraction::Start(start)));
}

impl DrawModifierNode for RippleNode {
    /// As `RippleNode.draw`: over the content, the ripples, then the state layer and the ring.
    /// At rest, nothing: not even an operation in the display list.
    fn draw_after(&mut self, scope: &mut DrawScope<'_>) {
        let shared = &self.shared;
        let ring = shared.focus_ring.borrow().as_ref().map_or(0.0, |ring| ring.value());
        let state_alpha = shared.state_layer_alpha.get();
        if shared.ripples.borrow().is_empty() && state_alpha <= 0.0 && ring <= 0.0 {
            return;
        }
        let Some(configuration) = shared.configuration() else {
            shared.clear();
            return;
        };
        let factory = shared.factory();
        let density = scope.density();
        let size = scope.size();
        let bounds = scope.bounds();
        let center = Offset::new(size.width / 2.0, size.height / 2.0);
        let target_radius = if factory.radius.is_specified() {
            density.to_px(factory.radius)
        } else {
            ripple_end_radius(&density, factory.bounded, size.width, size.height)
        };
        let color = shared.color(configuration);
        if factory.enable_press {
            for ripple in shared.ripples.borrow().iter() {
                let mut ripple = ripple.borrow_mut();
                let (start, end) = *ripple
                    .radii
                    .get_or_insert((ripple_start_radius(size.width, size.height), target_radius));
                let origin = *ripple.origin.get_or_insert(center);
                // Finished while fading in, it shows whole at once.
                let alpha = if ripple.finish_requested && !ripple.finished_fading_in {
                    1.0
                } else {
                    ripple.alpha
                };
                if alpha <= 0.0 {
                    continue;
                }
                let radius = start + (end - start) * ripple.radius_fraction;
                let at = Offset::new(
                    origin.x + (center.x - origin.x) * ripple.center_fraction,
                    origin.y + (center.y - origin.y) * ripple.center_fraction,
                );
                let paint = compose_ui::Paint::fill(color.with_alpha(PRESSED_ALPHA * alpha));
                draw_circle(scope, factory.bounded, bounds, at, radius, paint);
            }
        }
        if state_alpha > 0.0 {
            let paint = compose_ui::Paint::fill(color.with_alpha(state_alpha));
            draw_circle(scope, factory.bounded, bounds, center, target_radius, paint);
        }
        if ring > 0.0 {
            let theme = factory.locals.get(&LOCAL_RIPPLE_THEME_CONFIGURATION);
            if let RippleThemeFocus::InsetRing {
                outer_stroke_inset,
                outer_stroke_width,
                inner_stroke_inset,
                inner_stroke_width,
            } = theme.focus
            {
                let (outer_color, inner_color) = match configuration.focus {
                    Some(RippleConfigurationFocus::InsetRing {
                        outer_stroke_color,
                        inner_stroke_color,
                    }) => (outer_stroke_color, inner_stroke_color),
                    _ => {
                        let scheme = factory.locals.get(&LOCAL_COLOR_SCHEME);
                        (scheme.secondary, scheme.on_secondary)
                    }
                };
                let direction = scope.layout_direction();
                let shape = &factory.focus_ring_shape;
                draw_inset_border(
                    scope,
                    shape,
                    direction,
                    inner_stroke_inset * ring,
                    inner_stroke_width * ring,
                    inner_color,
                );
                draw_inset_border(
                    scope,
                    shape,
                    direction,
                    outer_stroke_inset * ring,
                    outer_stroke_width * ring,
                    outer_color,
                );
            }
        }
    }
}

fn draw_circle(
    scope: &mut DrawScope<'_>,
    bounded: bool,
    bounds: Rect,
    center: Offset,
    radius: f32,
    paint: compose_ui::Paint,
) {
    if bounded {
        scope.clip_rect(bounds, |scope| scope.draw_circle(center, radius, paint));
    } else {
        scope.draw_circle(center, radius, paint);
    }
}

/// A border of `width` `inset` inside `shape`'s outline, as `BorderLogic.drawBorder`. For a
/// generic outline, the band from the outline itself: the outer stroke drawn after over its
/// inset is what Compose's layer clears (D1025).
fn draw_inset_border(
    scope: &mut DrawScope<'_>,
    shape: &Shape,
    direction: compose_ui::LayoutDirection,
    inset: Dp,
    width: Dp,
    color: Color,
) {
    let density = scope.density();
    let size = scope.size();
    let inset_px = density.to_px(inset);
    let outline = shape.outline_in(size, &density, direction);
    let (outline, width) = match outline {
        Outline::Rectangle(rect) => (Outline::Rectangle(shrink(rect, inset_px)), width),
        Outline::Rounded(rect, radii) => (
            Outline::Rounded(shrink(rect, inset_px), radii.map(|it| (it - inset_px).max(0.0))),
            width,
        ),
        generic => (generic, width + inset),
    };
    BorderRenderer::new().draw_border(scope, width, &Brush::from(color), &outline);
}

fn shrink(rect: Rect, by: f32) -> Rect {
    Rect::new(rect.left + by, rect.top + by, rect.right - by, rect.bottom - by)
}
