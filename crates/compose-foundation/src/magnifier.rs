//! `Modifier.magnifier`, as Compose's `Magnifier.android.kt`: the platform's magnifier
//! (`compose_ui::PlatformMagnifierFactory`, Android's `android.widget.Magnifier`) shown over a
//! point of the node, kept where the node and its source say, its content copied again after
//! each drawing of the node. On a platform without one (desktop) the modifier is a no-op, as
//! Compose's below API 28 (D2072).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::{Callback, CoroutineScope, IntoCallback, IntoOptionalCallback};
use compose_ui::{
    Delegates, Density, Dp, DpSize, DrawModifierNode, DrawScope, GlobalPositionAwareModifierNode,
    InspectorInfo, IntSize, LayoutCoordinates, Modifier, ModifierNode, ModifierNodeElement,
    NodeContext, Offset, PlatformMagnifier, PlatformMagnifierConfig, PlatformMagnifierFactory,
    SemanticsConfiguration, SemanticsModifierNode, SemanticsPropertyKey, SemanticsValue, Size,
};

/// Where a magnifier's source center is in the root, as Compose's internal
/// `MagnifierPositionInRoot`'s function: read when asked, unspecified while it shows nothing.
#[derive(Clone)]
pub struct MagnifierPositionInRoot(Rc<Cell<Offset>>);

impl MagnifierPositionInRoot {
    /// The position now (Compose's function called).
    pub fn get(&self) -> Offset {
        self.0.get()
    }
}

impl PartialEq for MagnifierPositionInRoot {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl std::fmt::Debug for MagnifierPositionInRoot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "MagnifierPositionInRoot({:?})", self.0.get())
    }
}

impl SemanticsValue for MagnifierPositionInRoot {}

/// The key of a magnified node's [`MagnifierPositionInRoot`], as Compose's internal
/// `MagnifierPositionInRoot`: what tests find magnifiers by (compose-rs's tests are outside the
/// crate).
#[doc(hidden)]
pub static MAGNIFIER_POSITION_IN_ROOT: SemanticsPropertyKey<MagnifierPositionInRoot> =
    SemanticsPropertyKey::new("MagnifierPositionInRoot");

/// A function of the density returning an offset in pixels, as Compose's `Density.() -> Offset`.
pub type DensityOffset = dyn Fn(Density) -> Offset;

/// All of the magnifier's arguments, with Compose's internal ones (`useTextDefault`,
/// `platformMagnifierFactory`): what text selection and tests make it with.
#[doc(hidden)]
#[derive(Clone)]
pub struct MagnifierArgs {
    pub source_center: Callback<DensityOffset>,
    pub magnifier_center: Option<Callback<DensityOffset>>,
    pub on_size_changed: Option<Callback<dyn Fn(DpSize)>>,
    pub zoom: f32,
    pub use_text_default: bool,
    pub size: DpSize,
    pub corner_radius: Dp,
    pub elevation: Dp,
    pub clipping_enabled: bool,
    /// `None`: the tree's (the platform's).
    pub platform_magnifier_factory: Option<Rc<dyn PlatformMagnifierFactory>>,
}

impl MagnifierArgs {
    /// Compose's defaults around `source_center`.
    pub fn new(source_center: impl IntoCallback<DensityOffset>) -> MagnifierArgs {
        MagnifierArgs {
            source_center: source_center.into_callback(),
            magnifier_center: None,
            on_size_changed: None,
            zoom: f32::NAN,
            use_text_default: false,
            size: DpSize::UNSPECIFIED,
            corner_radius: Dp::UNSPECIFIED,
            elevation: Dp::UNSPECIFIED,
            clipping_enabled: true,
            platform_magnifier_factory: None,
        }
    }
}

/// `magnifier`, on `Modifier`.
pub trait MagnifierModifiers {
    /// Shows the platform's magnifier over `source_center` (pixels from the node's top left), at
    /// the platform's default place, as `Modifier.magnifier(sourceCenter)`. A no-op where the
    /// platform has none (desktop; Android below API 28).
    fn magnifier(self, source_center: impl IntoCallback<DensityOffset>) -> Modifier;

    /// With Compose's other parameters, as `Modifier.magnifier(sourceCenter, magnifierCenter,
    /// onSizeChanged, zoom, size, cornerRadius, elevation, clip)`: `magnifier_center` (in pixels
    /// from the node's top left; `None` or unspecified: the platform's place) is where it's
    /// drawn, `on_size_changed` is told its actual size once made, and the others are the
    /// platform's (`f32::NAN`, `DpSize::UNSPECIFIED`, `Dp::UNSPECIFIED`: its defaults).
    #[allow(clippy::too_many_arguments)]
    fn magnifier_with(
        self,
        source_center: impl IntoCallback<DensityOffset>,
        magnifier_center: impl IntoOptionalCallback<DensityOffset>,
        on_size_changed: impl IntoOptionalCallback<dyn Fn(DpSize)>,
        zoom: f32,
        size: DpSize,
        corner_radius: Dp,
        elevation: Dp,
        clip: bool,
    ) -> Modifier;

    /// With all its arguments, Compose's internal ones too (`useTextDefault`, the factory).
    #[doc(hidden)]
    fn magnifier_of(self, args: MagnifierArgs) -> Modifier;
}

impl MagnifierModifiers for Modifier {
    fn magnifier(self, source_center: impl IntoCallback<DensityOffset>) -> Modifier {
        self.magnifier_of(MagnifierArgs::new(source_center))
    }

    fn magnifier_with(
        self,
        source_center: impl IntoCallback<DensityOffset>,
        magnifier_center: impl IntoOptionalCallback<DensityOffset>,
        on_size_changed: impl IntoOptionalCallback<dyn Fn(DpSize)>,
        zoom: f32,
        size: DpSize,
        corner_radius: Dp,
        elevation: Dp,
        clip: bool,
    ) -> Modifier {
        self.magnifier_of(MagnifierArgs {
            magnifier_center: magnifier_center.into_optional_callback(),
            on_size_changed: on_size_changed.into_optional_callback(),
            zoom,
            size,
            corner_radius,
            elevation,
            clipping_enabled: clip,
            ..MagnifierArgs::new(source_center)
        })
    }

    fn magnifier_of(self, args: MagnifierArgs) -> Modifier {
        // As Compose's `isPlatformMagnifierSupported()` check: nothing where the tree composing
        // has no magnifier. A modifier made outside composition has its node check (D2072).
        if args.platform_magnifier_factory.is_none()
            && compose_ui::is_platform_magnifier_supported() == Some(false)
        {
            return self;
        }
        self.then_custom(MagnifierElement(args))
    }
}

/// Whether two optional callbacks are the same one.
fn same_callback<F: ?Sized>(a: &Option<Callback<F>>, b: &Option<Callback<F>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.ptr_eq(b),
        (None, None) => true,
        _ => false,
    }
}

/// NaN equal to NaN, as Compose's `equalsIncludingNaN` (NaN is its unspecified).
fn equals_including_nan(a: f32, b: f32) -> bool {
    (a.is_nan() && b.is_nan()) || a == b
}

fn same_factory(
    a: &Option<Rc<dyn PlatformMagnifierFactory>>,
    b: &Option<Rc<dyn PlatformMagnifierFactory>>,
) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

/// `Modifier.magnifier`'s element, as Compose's `MagnifierElement`.
struct MagnifierElement(MagnifierArgs);

impl ModifierNodeElement for MagnifierElement {
    type Node = MagnifierNode;
    fn create(&self) -> MagnifierNode {
        MagnifierNode::new(self.0.clone())
    }
    fn update(&self, node: &mut MagnifierNode) {
        node.update(self.0.clone());
    }
    fn same(&self, other: &Self) -> bool {
        let (a, b) = (&self.0, &other.0);
        a.source_center.ptr_eq(&b.source_center)
            && same_callback(&a.magnifier_center, &b.magnifier_center)
            && equals_including_nan(a.zoom, b.zoom)
            && a.use_text_default == b.use_text_default
            && a.size == b.size
            && a.corner_radius == b.corner_radius
            && a.elevation == b.elevation
            && a.clipping_enabled == b.clipping_enabled
            && same_callback(&a.on_size_changed, &b.on_size_changed)
            && same_factory(&a.platform_magnifier_factory, &b.platform_magnifier_factory)
    }
    fn name(&self) -> &'static str {
        "magnifier"
    }
    fn inspectable_properties(&self, info: &mut InspectorInfo) {
        let args = &self.0;
        info.name = Some("magnifier");
        let callback = |present: bool| if present { "Callback" } else { "null" }.to_string();
        info.properties.push(("sourceCenter", callback(true)));
        info.properties.push(("magnifierCenter", callback(args.magnifier_center.is_some())));
        info.properties.push(("zoom", format!("{:?}", args.zoom)));
        info.properties.push(("size", format!("{:?}", args.size)));
        info.properties.push(("cornerRadius", format!("{:?}", args.corner_radius)));
        info.properties.push(("elevation", format!("{:?}", args.elevation)));
        info.properties.push(("clippingEnabled", args.clipping_enabled.to_string()));
    }
}

/// The magnifier a node shows, shared with the task copying its content after a drawing.
type SharedMagnifier = Rc<RefCell<Option<Rc<dyn PlatformMagnifier>>>>;

/// `Modifier.magnifier`'s node, as Compose's `MagnifierNode`: makes its magnifier at the first
/// specified source center, shows it where the node's position in the root and its centers say
/// (observing what they read), hides it while the source center is unspecified, makes it anew
/// when what it was made with changes, and dismisses it when detached.
pub(crate) struct MagnifierNode {
    args: MagnifierArgs,
    /// The factory its magnifier was made with: the element's, or the tree's.
    factory: Option<Rc<dyn PlatformMagnifierFactory>>,
    /// The density it was made at.
    density: Option<Density>,
    magnifier: SharedMagnifier,
    /// The node's position in the root after its last layout, as Compose's
    /// `anchorPositionInRoot` (unspecified before).
    anchor_position_in_root: Offset,
    /// Its position also read from its coordinates at each update: a selection's magnifier
    /// delegated to during a gesture, not told its position until the next layout or layer move
    /// (D2073; told them since D2105).
    reads_position: bool,
    /// Where its source center is in the root, unspecified while it shows nothing: its
    /// semantics' (`MagnifierPositionInRoot`).
    source_center_in_root: Rc<Cell<Offset>>,
    /// The size last told `on_size_changed`.
    previous_size: Option<IntSize>,
    /// A copy of its content waits for the next frame (Compose's draw signal received).
    content_update_pending: Rc<Cell<bool>>,
    /// It shows (shown, not dismissed since): only then is its content copied again (D2073).
    showing: bool,
    scope: Option<CoroutineScope>,
    /// An update asked for by the element: done with a context.
    update_pending: Option<UpdateAsked>,
    delegates: Delegates,
}

/// What an element's update left to do with a context: whether what the magnifier is made with
/// changed (Compose's `shouldRecreate` before it reads the view and density).
#[derive(Clone, Copy)]
struct UpdateAsked {
    recreate: bool,
}

impl MagnifierNode {
    /// A node at `anchor` in the root, laid out already: delegated to while a selection's
    /// gesture shows its magnifier (D2073).
    pub(crate) fn new_at(args: MagnifierArgs, anchor: Offset) -> MagnifierNode {
        MagnifierNode {
            anchor_position_in_root: anchor,
            reads_position: true,
            ..MagnifierNode::new(args)
        }
    }

    pub(crate) fn new(args: MagnifierArgs) -> MagnifierNode {
        MagnifierNode {
            args,
            factory: None,
            density: None,
            magnifier: Rc::new(RefCell::new(None)),
            anchor_position_in_root: Offset::UNSPECIFIED,
            reads_position: false,
            source_center_in_root: Rc::new(Cell::new(Offset::UNSPECIFIED)),
            previous_size: None,
            content_update_pending: Rc::new(Cell::new(false)),
            showing: false,
            scope: None,
            update_pending: None,
            delegates: Delegates::new(),
        }
    }

    fn update(&mut self, args: MagnifierArgs) {
        let previous = std::mem::replace(&mut self.args, args);
        let args = &self.args;
        let can_update_zoom =
            self.factory.as_ref().is_some_and(|factory| factory.can_update_zoom());
        let recreate = self.magnifier.borrow().is_some()
            && ((!equals_including_nan(args.zoom, previous.zoom) && !can_update_zoom)
                || args.size != previous.size
                || args.corner_radius != previous.corner_radius
                || args.elevation != previous.elevation
                || args.use_text_default != previous.use_text_default
                || args.clipping_enabled != previous.clipping_enabled
                || !same_factory(
                    &args.platform_magnifier_factory,
                    &previous.platform_magnifier_factory,
                ));
        let recreate = recreate || self.update_pending.is_some_and(|asked| asked.recreate);
        self.update_pending = Some(UpdateAsked { recreate });
        self.delegates.ask_for_context();
    }

    /// The factory to make its magnifier with: the element's, else the tree's.
    fn resolve_factory(
        &self,
        context: &NodeContext<'_>,
    ) -> Option<Rc<dyn PlatformMagnifierFactory>> {
        self.args
            .platform_magnifier_factory
            .clone()
            .or_else(|| context.platform_magnifier_factory())
    }

    /// As `recreateMagnifier`: the one shown dismissed, a new one made.
    fn recreate_magnifier(&mut self, context: &NodeContext<'_>) {
        if let Some(magnifier) = self.magnifier.borrow_mut().take() {
            magnifier.dismiss();
        }
        self.showing = false;
        let factory = self.resolve_factory(context);
        let density = context.require_density();
        self.density = Some(density);
        self.factory = factory.clone();
        let Some(factory) = factory else { return };
        let config = PlatformMagnifierConfig {
            use_text_default: self.args.use_text_default,
            size: self.args.size,
            corner_radius: self.args.corner_radius,
            elevation: self.args.elevation,
            clipping_enabled: self.args.clipping_enabled,
            density,
            initial_zoom: self.args.zoom,
        };
        *self.magnifier.borrow_mut() = Some(factory.create(&config));
        self.update_size_if_necessary();
    }

    /// As `updateMagnifier`: shown where its centers say, or hidden while its source center or
    /// its position is unspecified. What the centers read is observed.
    fn update_magnifier(&mut self, context: &mut NodeContext<'_>) {
        // The density the shown magnifier was made at; with none shown, the current one, so the
        // centers and the one then made agree (D2107).
        let density = if self.magnifier.borrow().is_none() {
            *self.density.insert(context.require_density())
        } else {
            *self.density.get_or_insert_with(|| context.require_density())
        };
        if self.reads_position {
            self.anchor_position_in_root = context.require_layout_coordinates().position_in_root();
        }
        let anchor = self.anchor_position_in_root;
        let (source_center, magnifier_center) = {
            let source_center = self.args.source_center.clone();
            let magnifier_center = self.args.magnifier_center.clone();
            context.observe_reads(move || {
                let source = source_center(density);
                // Compose doesn't call the magnifier's center while the source's unspecified.
                let center = (source.is_specified() && anchor.is_specified())
                    .then(|| magnifier_center.map(|center| center(density)))
                    .flatten();
                (source, center)
            })
        };
        if source_center.is_specified() && anchor.is_specified() {
            let source_in_root = anchor + source_center;
            self.set_source_center_in_root(source_in_root, context);
            let magnifier_center = magnifier_center
                .filter(|center| center.is_specified())
                .map_or(Offset::UNSPECIFIED, |center| anchor + center);
            if self.magnifier.borrow().is_none() {
                self.recreate_magnifier(context);
            }
            let magnifier = self.magnifier.borrow().clone();
            if let Some(magnifier) = magnifier {
                magnifier.update(source_in_root, magnifier_center, self.args.zoom);
                self.showing = true;
            }
            self.update_size_if_necessary();
            return;
        }
        // It can't be placed where asked: hidden, so it doesn't show somewhere wrong.
        self.set_source_center_in_root(Offset::UNSPECIFIED, context);
        let magnifier = self.magnifier.borrow().clone();
        if let Some(magnifier) = magnifier {
            magnifier.dismiss();
        }
        self.showing = false;
        // A copy waiting for a frame isn't needed any more.
        self.content_update_pending.set(false);
    }

    /// Where its source center is in the root: its semantics say it while it's specified, and
    /// only then (Compose's say it always), so a node whose magnifier shows nothing adds nothing to
    /// its semantics (D2073).
    fn set_source_center_in_root(&mut self, center: Offset, context: &mut NodeContext<'_>) {
        let was = self.source_center_in_root.replace(center);
        if was.is_specified() != center.is_specified() {
            context.invalidate_semantics();
        }
    }

    /// As `updateSizeIfNecessary`: `on_size_changed` told a new size.
    fn update_size_if_necessary(&mut self) {
        let Some(magnifier) = self.magnifier.borrow().clone() else { return };
        let Some(density) = self.density else { return };
        let size = magnifier.size();
        if self.previous_size != Some(size) {
            self.previous_size = Some(size);
            if let Some(on_size_changed) = &self.args.on_size_changed {
                let size = Size::new(size.width as f32, size.height as f32);
                on_size_changed(density.to_dp_size(size));
            }
        }
    }
}

impl ModifierNode for MagnifierNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
    fn as_global_position_aware(&mut self) -> Option<&mut dyn GlobalPositionAwareModifierNode> {
        Some(self)
    }
    fn as_observer(&mut self) -> Option<&mut dyn compose_ui::ObserverModifierNode> {
        Some(self)
    }
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.delegates)
    }
    /// An update shows it again itself; its drawing and semantics don't change with it.
    fn should_auto_invalidate(&self) -> bool {
        false
    }
    fn on_update_with_context(&mut self, context: &mut NodeContext<'_>) {
        let Some(asked) = self.update_pending.take() else { return };
        // The factory or the density it was made with changed (Compose's view and density).
        let changed_platform = self.magnifier.borrow().is_some()
            && (!same_factory(&self.factory, &self.resolve_factory(context))
                || self.density != Some(context.require_density()));
        if asked.recreate || changed_platform {
            self.recreate_magnifier(context);
        }
        self.update_magnifier(context);
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.update_pending = None;
        self.scope = Some(context.coroutine_scope());
        self.update_magnifier(context);
    }
    fn on_detach(&mut self, _context: &mut NodeContext<'_>) {
        if let Some(magnifier) = self.magnifier.borrow_mut().take() {
            magnifier.dismiss();
        }
        self.showing = false;
        self.scope = None;
        self.content_update_pending.set(false);
        self.update_pending = None;
    }
    fn on_density_change(&mut self, context: &mut NodeContext<'_>) {
        if self.density == Some(context.require_density()) {
            return;
        }
        // A shown one is made anew; a hidden one's centers are asked again at the new density.
        if self.magnifier.borrow().is_some() {
            self.recreate_magnifier(context);
        }
        self.update_magnifier(context);
    }
}

impl compose_ui::ObserverModifierNode for MagnifierNode {
    fn on_observed_reads_changed(&mut self, context: &mut NodeContext<'_>) {
        self.update_magnifier(context);
    }
}

impl GlobalPositionAwareModifierNode for MagnifierNode {
    /// Its position in the root followed: a change shows it there (Compose's
    /// `anchorPositionInRoot`, derived from the coordinates' state).
    fn on_globally_positioned(
        &mut self,
        coordinates: LayoutCoordinates,
        context: &mut NodeContext<'_>,
    ) {
        let position = coordinates.position_in_root();
        if position != self.anchor_position_in_root {
            self.anchor_position_in_root = position;
            self.update_magnifier(context);
        }
    }
}

impl DrawModifierNode for MagnifierNode {
    /// Its content copied again at the next frame, once what's drawn now is on the screen, as
    /// Compose's draw signal: several drawings before it copy once. Only while it shows, where
    /// Compose copies into a dismissed one too (a no-op): a hidden magnifier waits for no frame
    /// (D2073).
    fn draw(&mut self, _scope: &mut DrawScope<'_>) {
        if !self.showing || self.content_update_pending.get() {
            return;
        }
        let Some(scope) = self.scope else { return };
        self.content_update_pending.set(true);
        let magnifier = self.magnifier.clone();
        let pending = self.content_update_pending.clone();
        scope.launch(async move {
            compose_runtime::with_frame_millis(|_| ()).await;
            if !pending.replace(false) {
                return;
            }
            let magnifier = magnifier.borrow().clone();
            if let Some(magnifier) = magnifier {
                magnifier.update_content();
            }
        });
    }
}

impl SemanticsModifierNode for MagnifierNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        if self.source_center_in_root.get().is_unspecified() {
            return;
        }
        config.set(
            &MAGNIFIER_POSITION_IN_ROOT,
            MagnifierPositionInRoot(self.source_center_in_root.clone()),
        );
    }
}
