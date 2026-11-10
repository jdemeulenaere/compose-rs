//! Shared elements, as Compose's `SharedTransitionScope.kt`, `SharedElement.kt`,
//! `SharedElementEntry.kt`, `SharedContentNode.kt`, `SharedTransitionStateMachine.kt`,
//! `BoundsAnimation.kt`, `RenderInTransitionOverlayNodeElement.kt` and `SkipToLookaheadSizeNode.kt`
//! (D1329, [design](../../../docs/parity/design/shared-elements.md)): contents matched by a key
//! across the content of a transition, their bounds animated from where the one leaving was to
//! where the pass ahead puts the one coming, drawn in the layout's overlay meanwhile.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::{Rc, Weak};

use compose_runtime::{
    ComposeArg, CoroutineScope, DisposableEffect, OwnedState, composable, mutable_state_of,
    remember, remember_keyed,
};
use compose_ui::{
    ApproachLayoutModifierNode, ApproachMeasureScope, Constraints, ContentDrawScope, CornerRadius,
    Density, DrawModifierNode, DrawScope, GraphicsLayer, IntOffset, IntSize, IntrinsicMeasureScope,
    LayerBlock, LayoutCoordinates, LayoutDirection, LayoutModifierNode, LookaheadScope, Measurable,
    MeasurePolicy, MeasureResult, MeasureScope, Modifier, ModifierLocal, ModifierLocalMap,
    ModifierNode, ModifierNodeElement, NodeContext, NodeHandle, ObserverModifierNode, Offset,
    Outline, Path, PlacementInProgress, PlacementScope, Rect, RoundRect, Shape, Size,
    TransformOrigin, Velocity,
};

use crate::animatable::Animatable;
use crate::deferred::SharedMutableTransformState;
use crate::shared::{
    BoundsTransform, PlaceholderSize, ResizeMode, SharedContentConfig, SharedTransitionDefaults,
};
use crate::spec::{AnimationSpec, Spring, spring_with};
use crate::transition::{
    DeferredAnimation, DeferredData, MutableTransitionState, Transition, remember_transition,
};
use crate::vector::{AnimationVector, VisibilityThreshold};
use crate::visibility::{
    AnimatedVisibilityScope, EnterExitState, EnterTransition, ExitTransition,
    LOCAL_TRANSFORM_STATE, create_enter_exit_modifier_enabled, fade_in, fade_out,
    update_enter_exit_modifier_state,
};

// ---- Keys ----------------------------------------------------------------------------------

/// What matches shared contents, as Compose's `Any` key: any hashable, comparable value, compared
/// with values of its own type.
#[derive(Clone)]
pub struct SharedKey {
    hash: u64,
    value: Rc<dyn Any>,
    eq: fn(&dyn Any, &dyn Any) -> bool,
    debug: fn(&dyn Any, &mut fmt::Formatter<'_>) -> fmt::Result,
}

fn key_eq<K: PartialEq + 'static>(a: &dyn Any, b: &dyn Any) -> bool {
    matches!((a.downcast_ref::<K>(), b.downcast_ref::<K>()), (Some(a), Some(b)) if a == b)
}

fn key_debug<K: fmt::Debug + 'static>(a: &dyn Any, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match a.downcast_ref::<K>() {
        Some(key) => fmt::Debug::fmt(key, f),
        None => f.write_str("?"),
    }
}

impl SharedKey {
    /// `key` as a shared key.
    pub fn new<K: Hash + Eq + fmt::Debug + 'static>(key: K) -> SharedKey {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::any::TypeId::of::<K>().hash(&mut hasher);
        key.hash(&mut hasher);
        SharedKey {
            hash: hasher.finish(),
            value: Rc::new(key),
            eq: key_eq::<K>,
            debug: key_debug::<K>,
        }
    }

    /// The key, if it's a `K`.
    pub fn downcast_ref<K: 'static>(&self) -> Option<&K> {
        self.value.downcast_ref()
    }
}

impl PartialEq for SharedKey {
    fn eq(&self, other: &SharedKey) -> bool {
        self.hash == other.hash && (self.eq)(&*self.value, &*other.value)
    }
}

impl Eq for SharedKey {}

impl Hash for SharedKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

impl fmt::Debug for SharedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (self.debug)(&*self.value, f)
    }
}

impl ComposeArg for SharedKey {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

// ---- Overlay clips -----------------------------------------------------------------------------

/// How a shared content is clipped in the overlay, as Compose's `OverlayClip`: the path to clip
/// to, given the content's state, its bounds in the layout, the direction and the density.
#[derive(Clone)]
pub struct OverlayClip(Rc<ClipPath>);

/// What an overlay clip computes its path with.
type ClipPath = dyn Fn(&SharedContentState, Rect, LayoutDirection, Density) -> Option<Path>;

impl OverlayClip {
    /// As an `OverlayClip`'s `getClipPath`.
    pub fn new(
        clip_path: impl Fn(&SharedContentState, Rect, LayoutDirection, Density) -> Option<Path>
        + 'static,
    ) -> OverlayClip {
        OverlayClip(Rc::new(clip_path))
    }

    /// The path, as `getClipPath`.
    pub fn get_clip_path(
        &self,
        state: &SharedContentState,
        bounds: Rect,
        direction: LayoutDirection,
        density: Density,
    ) -> Option<Path> {
        (self.0)(state, bounds, direction, density)
    }

    /// The parent shared content's clip, the default, as Compose's `ParentClip`. One shared
    /// value.
    pub fn parent() -> OverlayClip {
        thread_local! {
            static PARENT: OverlayClip = OverlayClip::new(|state, _, _, _| {
                state.parent_shared_content_state()?.clip_path_in_overlay()
            });
        }
        PARENT.with(OverlayClip::clone)
    }

    /// A clip to `shape` at the bounds, as `OverlayClip(clipShape)`'s `ShapeBasedClip`.
    pub fn of_shape(shape: Shape) -> OverlayClip {
        OverlayClip::new(move |_, bounds, direction, density| {
            let path = Path::new();
            match shape.outline_in(bounds.size(), &density, direction) {
                Outline::Rectangle(rect) => path.add_rect(rect),
                Outline::Rounded(rect, radii) => {
                    let corner = |r: f32| CornerRadius::new(r, r);
                    path.add_round_rect(RoundRect::with_radii(
                        rect,
                        corner(radii[0]),
                        corner(radii[1]),
                        corner(radii[2]),
                        corner(radii[3]),
                    ));
                }
                Outline::Generic(data, offset) => {
                    path.add_path(&Path::from_shared_data(data), offset)
                }
            }
            path.translate(bounds.top_left());
            Some(path)
        })
    }
}

impl PartialEq for OverlayClip {
    fn eq(&self, other: &OverlayClip) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for OverlayClip {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OverlayClip")
    }
}

// ---- Content states ----------------------------------------------------------------------------

/// A shared content's key and how it takes part, as Compose's `SharedContentState` (made by
/// [`SharedTransitionScope::remember_shared_content_state`]).
#[derive(Clone)]
pub struct SharedContentState(Rc<ContentStateInner>);

struct ContentStateInner {
    key: SharedKey,
    config: OwnedState<ConfigRef>,
    /// The entry of the modifier it's given to (a state, as `internalState`).
    internal: OwnedState<WeakEntry>,
}

/// A config, compared by identity.
#[derive(Clone)]
struct ConfigRef(Rc<dyn SharedContentConfig>);

impl PartialEq for ConfigRef {
    fn eq(&self, other: &ConfigRef) -> bool {
        std::ptr::addr_eq(Rc::as_ptr(&self.0), Rc::as_ptr(&other.0))
    }
}

#[derive(Clone, Default)]
struct WeakEntry(Option<Weak<Entry>>);

impl PartialEq for WeakEntry {
    fn eq(&self, other: &WeakEntry) -> bool {
        match (&self.0, &other.0) {
            (Some(a), Some(b)) => Weak::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
    }
}

impl SharedContentState {
    /// Its key, as `key`.
    pub fn key(&self) -> &SharedKey {
        &self.0.key
    }

    /// Whether a match is found: another content of its key, at least one of them visible, as
    /// `isMatchFound` (a state read).
    pub fn is_match_found(&self) -> bool {
        self.entry().is_some_and(|entry| entry.element().found_match())
    }

    /// The clip path in the overlay, as `clipPathInOverlay`: its clip's path at its bounds while
    /// it renders in the overlay.
    ///
    /// Panics if it hasn't been given to a shared modifier, as Compose's.
    pub fn clip_path_in_overlay(&self) -> Option<Path> {
        self.required_entry().clip_path_in_overlay.borrow().clone()
    }

    /// The state of the shared content around it, as `parentSharedContentState`.
    ///
    /// Panics if it hasn't been given to a shared modifier, as Compose's.
    pub fn parent_shared_content_state(&self) -> Option<SharedContentState> {
        let parent = self.required_entry().parent_state.borrow().clone()?.upgrade()?;
        Some(parent.user_state.borrow().clone())
    }

    /// Starts the transition with `initial_velocity`, as `prepareTransitionWithInitialVelocity`:
    /// a momentum decaying from it is added to the animated bounds. Nothing if it isn't enabled.
    ///
    /// Panics if it hasn't been given to a shared modifier, as Compose's.
    pub fn prepare_transition_with_initial_velocity(&self, initial_velocity: Velocity) {
        let entry = self.required_entry();
        if !self.config().is_enabled(self) {
            return;
        }
        entry.element().update_exit_velocity(initial_velocity);
    }

    fn config(&self) -> Rc<dyn SharedContentConfig> {
        self.0.config.get().0
    }

    fn entry(&self) -> Option<Rc<Entry>> {
        self.0.internal.get().0.and_then(|it| it.upgrade())
    }

    fn required_entry(&self) -> Rc<Entry> {
        self.entry().expect(
            "SharedContentState has not been added to a sharedElement/sharedBounds modifier yet",
        )
    }

    /// As `isAnimating`.
    fn is_animating(&self) -> bool {
        self.entry().is_some_and(|entry| entry.bounds_animation().animation_state.get().is_some())
    }

    /// As `isEnabledByUser`.
    fn is_enabled_by_user(&self) -> bool {
        let config = self.config();
        config.is_enabled(self)
            || (self.is_animating() && config.should_keep_enabled_for_ongoing_animation())
    }
}

impl PartialEq for SharedContentState {
    fn eq(&self, other: &SharedContentState) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl ComposeArg for SharedContentState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl fmt::Debug for SharedContentState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SharedContentState({:?})", self.0.key)
    }
}

// ---- Options -----------------------------------------------------------------------------------

/// What a shared modifier takes besides its state, with Compose's defaults (`Default`): the
/// bounds' animation, the size its parent sees, whether and where it's drawn in the overlay and
/// how it's clipped there; for `shared_bounds`, its enter and exit and how its content fits
/// the bounds (ignored by the others).
#[derive(Clone)]
pub struct SharedOptions {
    pub bounds_transform: BoundsTransform,
    pub placeholder_size: PlaceholderSize,
    pub render_in_overlay_during_transition: bool,
    pub z_index_in_overlay: f32,
    pub clip_in_overlay_during_transition: OverlayClip,
    /// `shared_bounds` only: `fade_in()` by default.
    pub enter: EnterTransition,
    /// `shared_bounds` only: `fade_out()` by default.
    pub exit: ExitTransition,
    /// `shared_bounds` only.
    pub resize_mode: ResizeMode,
}

impl Default for SharedOptions {
    fn default() -> SharedOptions {
        SharedOptions {
            bounds_transform: SharedTransitionDefaults::bounds_transform(),
            placeholder_size: PlaceholderSize::content_size(),
            render_in_overlay_during_transition: true,
            z_index_in_overlay: 0.0,
            clip_in_overlay_during_transition: OverlayClip::parent(),
            enter: fade_in(),
            exit: fade_out(),
            resize_mode: ResizeMode::default(),
        }
    }
}

// ---- The scope ---------------------------------------------------------------------------------

/// What a [`SharedTransitionLayout`] gives its content, as Compose's `SharedTransitionScope`: a
/// `LookaheadScope` with the shared modifiers. Compared by identity.
#[derive(Clone)]
pub struct SharedTransitionScope {
    inner: Rc<ScopeInner>,
}

struct ScopeInner {
    lookahead: LookaheadScope,
    coroutines: CoroutineScope,
    is_transition_active: OwnedState<bool>,
    active_entries: Cell<usize>,
    /// The layout's coordinates now, and ahead, as `root` and `lookaheadRoot`.
    root: Cell<Option<LayoutCoordinates>>,
    lookahead_root: Cell<Option<LayoutCoordinates>>,
    renderers: RefCell<Vec<Renderer>>,
    renderers_version: OwnedState<u64>,
    last_sorted_version: Cell<Option<u64>>,
    elements: RefCell<HashMap<SharedKey, Rc<SharedElement>>>,
    /// The root modifier's node, to draw the overlay again.
    root_node: Cell<Option<NodeHandle>>,
}

/// What draws in the overlay, as `LayerRenderer`.
#[derive(Clone)]
enum Renderer {
    Entry(Rc<Entry>),
    Overlay(Rc<OverlayRenderer>),
}

impl Renderer {
    fn z_order(&self) -> f32 {
        match self {
            // Top-level shared contents of z-index 0 draw under the rest, as Compose's
            // comparator.
            Renderer::Entry(entry) => {
                let z = entry.z_index.get();
                if z == 0.0 && entry.parent().is_none() { -1.0 } else { z }
            }
            Renderer::Overlay(overlay) => overlay.z_index.get(),
        }
    }

    fn same(&self, other: &Renderer) -> bool {
        match (self, other) {
            (Renderer::Entry(a), Renderer::Entry(b)) => Rc::ptr_eq(a, b),
            (Renderer::Overlay(a), Renderer::Overlay(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl ComposeArg for SharedTransitionScope {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl SharedTransitionScope {
    /// The `LookaheadScope` it is.
    pub fn lookahead_scope(&self) -> LookaheadScope {
        self.inner.lookahead
    }

    /// Whether a transition of its shared contents runs, as `isTransitionActive` (a state read).
    pub fn is_transition_active(&self) -> bool {
        self.inner.is_transition_active.get()
    }

    /// A content's state for `key`, remembered with the key, as `rememberSharedContentState(key)`.
    pub fn remember_shared_content_state<K: Hash + Eq + fmt::Debug + Clone + 'static>(
        &self,
        key: K,
    ) -> SharedContentState {
        self.remember_shared_content_state_with(
            key,
            Rc::new(SharedTransitionDefaults::shared_content_config()),
        )
    }

    /// The same with `config`, as `rememberSharedContentState(key, config)`.
    pub fn remember_shared_content_state_with<K: Hash + Eq + fmt::Debug + Clone + 'static>(
        &self,
        key: K,
        config: Rc<dyn SharedContentConfig>,
    ) -> SharedContentState {
        let key = SharedKey::new(key);
        let config = ConfigRef(config);
        let state = remember_keyed(key.clone(), || {
            SharedContentState(Rc::new(ContentStateInner {
                key: key.clone(),
                config: mutable_state_of(config.clone()),
                internal: mutable_state_of(WeakEntry::default()),
            }))
        });
        state.0.config.set(config);
        state
    }

    /// The default config, as `SharedContentConfig()`: one shared value.
    pub fn shared_content_config(&self) -> Rc<dyn SharedContentConfig> {
        thread_local! {
            static DEFAULT: Rc<dyn SharedContentConfig> =
                Rc::new(SharedTransitionDefaults::shared_content_config());
        }
        DEFAULT.with(Rc::clone)
    }

    /// The default config but whether a deferred transition's manual transforms move the content,
    /// as `SharedContentConfig(permitTransformDuringDeferredTransition)`: not permitted, the
    /// content stays where it was while its container is moved by hand, drawn in the overlay.
    pub fn shared_content_config_with(
        &self,
        permit_transform_during_deferred_transition: bool,
    ) -> Rc<dyn SharedContentConfig> {
        if permit_transform_during_deferred_transition {
            return self.shared_content_config();
        }
        struct NotPermitting;
        impl SharedContentConfig for NotPermitting {
            fn permit_transform_during_deferred_transition(&self) -> bool {
                false
            }
        }
        Rc::new(NotPermitting)
    }

    /// A shared element of `state`'s key in `animated_visibility_scope`'s content, as
    /// `Modifier.sharedElement(sharedContentState, animatedVisibilityScope)`: shown only while
    /// visible, its bounds animated to the visible one's. Call it on every pass of the content,
    /// as a composable; its value goes in the content's modifier.
    pub fn shared_element(
        &self,
        state: &SharedContentState,
        animated_visibility_scope: &AnimatedVisibilityScope,
    ) -> Modifier {
        self.shared_element_with(state, animated_visibility_scope, SharedOptions::default())
    }

    /// The same with `options`, as `Modifier.sharedElement(…)` with all its parameters.
    pub fn shared_element_with(
        &self,
        state: &SharedContentState,
        animated_visibility_scope: &AnimatedVisibilityScope,
        options: SharedOptions,
    ) -> Modifier {
        let source = Visibility::Parent(animated_visibility_scope.transition().clone());
        self.shared_bounds_impl(state, source, &options, true)
    }

    /// Shared bounds of `state`'s key in `animated_visibility_scope`'s content, as
    /// `Modifier.sharedBounds(sharedContentState, animatedVisibilityScope)`: the contents differ
    /// and both show, fading, while the bounds animate. Call it on every pass of the content.
    pub fn shared_bounds(
        &self,
        state: &SharedContentState,
        animated_visibility_scope: &AnimatedVisibilityScope,
    ) -> Modifier {
        self.shared_bounds_with(state, animated_visibility_scope, SharedOptions::default())
    }

    /// The same with `options`, as `Modifier.sharedBounds(…)` with all its parameters.
    pub fn shared_bounds_with(
        &self,
        state: &SharedContentState,
        animated_visibility_scope: &AnimatedVisibilityScope,
        options: SharedOptions,
    ) -> Modifier {
        let transition = animated_visibility_scope.transition();
        let source = Visibility::Parent(transition.clone());
        let shared = self.shared_bounds_impl(state, source, &options, false);
        let (enter, exit, _) =
            update_enter_exit_modifier_state(transition, options.enter, options.exit, None);
        let matched = state.clone();
        let enabled: Rc<dyn Fn() -> bool> = Rc::new(move || matched.is_match_found());
        let enter_exit = create_enter_exit_modifier_enabled(
            transition,
            enter,
            exit,
            None,
            Some(enabled.clone()),
            "enter/exit for shared bounds",
        );
        let scaled = match options.resize_mode {
            ResizeMode::ScaleToBounds { content_scale, alignment } => {
                content_scale_modifier(content_scale, alignment, enabled)
            }
            ResizeMode::RemeasureToBounds => Modifier,
        };
        shared.then(enter_exit).then(scaled)
    }

    /// A shared element shown while `visible`, which the caller animates, as
    /// `Modifier.sharedElementWithCallerManagedVisibility(sharedContentState, visible)`.
    pub fn shared_element_with_caller_managed_visibility(
        &self,
        state: &SharedContentState,
        visible: bool,
    ) -> Modifier {
        self.shared_element_with_caller_managed_visibility_with(
            state,
            visible,
            SharedOptions::default(),
        )
    }

    /// The same with `options`.
    pub fn shared_element_with_caller_managed_visibility_with(
        &self,
        state: &SharedContentState,
        visible: bool,
        options: SharedOptions,
    ) -> Modifier {
        self.shared_bounds_impl(state, Visibility::Managed(visible), &options, true)
    }

    /// Shared bounds shown while `visible`, as Compose's internal
    /// `sharedBoundsWithCallerManagedVisibility`.
    pub fn shared_bounds_with_caller_managed_visibility(
        &self,
        state: &SharedContentState,
        visible: bool,
        options: SharedOptions,
    ) -> Modifier {
        self.shared_bounds_impl(state, Visibility::Managed(visible), &options, false)
    }

    /// Measured and placed at its size ahead during a transition, as
    /// `Modifier.skipToLookaheadSize()`.
    pub fn skip_to_lookahead_size(&self) -> Modifier {
        let scope = self.clone();
        self.skip_to_lookahead_size_when(move || scope.is_transition_active())
    }

    /// The same while `enabled` says so, as `Modifier.skipToLookaheadSize(enabled)`.
    pub fn skip_to_lookahead_size_when(&self, enabled: impl Fn() -> bool + 'static) -> Modifier {
        Modifier.then_custom(SkipElement { scale: None, enabled: Rc::new(enabled) })
    }

    /// Placed at its position ahead during a transition, as `Modifier.skipToLookaheadPosition()`.
    pub fn skip_to_lookahead_position(&self) -> Modifier {
        let scope = self.clone();
        self.skip_to_lookahead_position_when(move || scope.is_transition_active())
    }

    /// The same while `enabled` says so, as `Modifier.skipToLookaheadPosition(enabled)`.
    pub fn skip_to_lookahead_position_when(
        &self,
        enabled: impl Fn() -> bool + 'static,
    ) -> Modifier {
        let enabled: Rc<dyn Fn() -> bool> = Rc::new(enabled);
        let lookahead = self.inner.lookahead;
        let in_progress = enabled.clone();
        Modifier.approach_layout(
            |_| false,
            move |_, _| in_progress(),
            move |scope, measurable, constraints| {
                let placeable = scope.measure(measurable, constraints);
                let enabled = enabled.clone();
                scope.layout_with(placeable.width, placeable.height, move |placement| {
                    let target = enabled()
                        .then(|| {
                            let it = placement.coordinates()?;
                            let root = lookahead.lookahead_scope_coordinates(placement)?;
                            let target =
                                lookahead.local_lookahead_position_of(root, it, Offset::ZERO);
                            let actual = root.local_position_of(&it, Offset::ZERO);
                            let delta = target - actual;
                            let offset = it.local_position_of(&root, delta)
                                - it.local_position_of(&root, Offset::ZERO);
                            Some(offset.round())
                        })
                        .flatten();
                    placement.place_at(&placeable, target.unwrap_or(IntOffset::ZERO));
                })
            },
        )
    }

    /// Drawn in the overlay during a transition, above the layout's content, as
    /// `Modifier.renderInSharedTransitionScopeOverlay()`.
    pub fn render_in_shared_transition_scope_overlay(&self) -> Modifier {
        let scope = self.clone();
        self.render_in_shared_transition_scope_overlay_with(0.0, move || {
            scope.is_transition_active()
        })
    }

    /// The same at `z_index_in_overlay`, while `render_in_overlay` says so.
    pub fn render_in_shared_transition_scope_overlay_with(
        &self,
        z_index_in_overlay: f32,
        render_in_overlay: impl Fn() -> bool + 'static,
    ) -> Modifier {
        Modifier.then_custom(OverlayElement {
            scope: self.clone(),
            render_in_overlay: Rc::new(render_in_overlay),
            z_index: z_index_in_overlay,
        })
    }

    /// A clip to `clip_shape` in the overlay, as `OverlayClip(clipShape)`.
    pub fn overlay_clip(&self, clip_shape: Shape) -> OverlayClip {
        OverlayClip::of_shape(clip_shape)
    }

    /// The shared element of `key`, for tests, as `sharedElementForKey`: whether it has one.
    #[doc(hidden)]
    pub fn has_shared_element_for_key<K: Hash + Eq + fmt::Debug + 'static>(&self, key: K) -> bool {
        self.inner.elements.borrow().contains_key(&SharedKey::new(key))
    }

    /// The layout's coordinates, as noted ahead, for tests, as `lookaheadRoot` (outside a pass
    /// they answer now).
    #[doc(hidden)]
    pub fn lookahead_root_for_test(&self) -> Option<LayoutCoordinates> {
        self.inner.lookahead_root.get()
    }

    /// The layout's coordinates, for tests, as `root`.
    #[doc(hidden)]
    pub fn root_for_test(&self) -> Option<LayoutCoordinates> {
        self.inner.root.get()
    }

    /// Whether an overlay renderer that's not a shared content holds a layer, for tests, as
    /// `hasRenderInOverlayLayer`.
    #[doc(hidden)]
    pub fn has_render_in_overlay_layer(&self) -> bool {
        self.inner.renderers.borrow().iter().any(|renderer| match renderer {
            Renderer::Overlay(overlay) => overlay.layer.borrow().is_some(),
            Renderer::Entry(_) => false,
        })
    }

    /// Whether `state`'s content holds a layer for the overlay, for tests.
    #[doc(hidden)]
    pub fn has_layer_for(&self, state: &SharedContentState) -> bool {
        state.entry().is_some_and(|entry| entry.layer.borrow().is_some())
    }

    fn shared_bounds_impl(
        &self,
        state: &SharedContentState,
        source: Visibility,
        options: &SharedOptions,
        render_only_when_visible: bool,
    ) -> Modifier {
        let shared_key = state.key().clone();
        let entry = compose_runtime::key(shared_key.clone(), || {
            let element = remember(|| self.inner.shared_element_for(&shared_key));
            let identity = match &source {
                Visibility::Parent(transition) => transition.identity(),
                Visibility::Managed(_) => 0,
            };
            let bounds_animation = compose_runtime::key(identity, || {
                let transition = match &source {
                    Visibility::Parent(parent) => parent
                        .create_child_transition("shared element", |state: &EnterExitState| {
                            *state == EnterExitState::Visible
                        }),
                    Visibility::Managed(visible) => {
                        let visible = *visible;
                        let transition_state = remember(|| {
                            let initial = if element.enabled_entries_untracked().is_empty() {
                                visible
                            } else {
                                !visible
                            };
                            MutableTransitionState::new(initial)
                        });
                        transition_state.set_target_state(visible);
                        remember_transition(&transition_state, "shared element")
                    }
                };
                let active = self.is_transition_active();
                let animation = compose_runtime::key(active, || {
                    transition.create_deferred_animation::<Rect>(|| "bounds".to_string())
                });
                let bounds_animation = remember_keyed(transition.clone(), || {
                    Rc::new(BoundsAnimation::new(
                        Rc::downgrade(&self.inner),
                        transition.clone(),
                        animation.clone(),
                        options.bounds_transform.clone(),
                        element.clone(),
                    ))
                });
                bounds_animation.update_animation(animation, options.bounds_transform.clone());
                bounds_animation
            });
            let entry = remember(|| {
                Rc::new(Entry::new(
                    element.clone(),
                    bounds_animation.clone(),
                    options,
                    render_only_when_visible,
                    state.clone(),
                ))
            });
            state.0.internal.set(WeakEntry(Some(Rc::downgrade(&entry))));
            entry.update(
                element.clone(),
                bounds_animation,
                options,
                render_only_when_visible,
                state,
            );
            // As `SharedElementEntry`'s `RememberObserver`.
            let (added, scope) = (entry.clone(), self.inner.clone());
            DisposableEffect(EntryArg(entry.clone()), move |effect| {
                scope.on_entry_added(&added);
                added.element().invalidate_target_bounds_provider();
                effect.on_dispose(move || {
                    scope.on_entry_removed(&added);
                    added.element().invalidate_target_bounds_provider();
                })
            });
            entry
        });
        Modifier.then_custom(SharedBoundsElement { entry })
    }
}

/// An entry, compared by identity, as an effect's key.
#[derive(Clone)]
struct EntryArg(Rc<Entry>);

impl ComposeArg for EntryArg {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// Where a shared content's visibility comes from.
enum Visibility {
    /// An `AnimatedVisibilityScope`'s transition: visible while `Visible`.
    Parent(Transition<EnterExitState>),
    /// The caller's.
    Managed(bool),
}

impl ScopeInner {
    fn shared_element_for(self: &Rc<Self>, key: &SharedKey) -> Rc<SharedElement> {
        let mut elements = self.elements.borrow_mut();
        elements
            .entry(key.clone())
            .or_insert_with(|| SharedElement::new(key.clone(), Rc::downgrade(self)))
            .clone()
    }

    fn root(&self) -> Option<LayoutCoordinates> {
        self.root.get()
    }

    fn renderers_changed(&self) {
        self.renderers_version.update(|version| version.wrapping_add(1));
    }

    /// As `onNodeTransitionActivenessChanged`.
    fn on_node_transition_activeness_changed(&self, active: bool) {
        let count = self.active_entries.get();
        let count = if active { count + 1 } else { count.saturating_sub(1) };
        self.active_entries.set(count);
        let is_active = count > 0;
        if is_active
            != compose_runtime::snapshots::Snapshot::without_read_observation(|| {
                self.is_transition_active.get()
            })
        {
            self.is_transition_active.set(is_active);
            if !is_active {
                let elements: Vec<_> = self.elements.borrow().values().cloned().collect();
                for element in elements {
                    element.on_shared_transition_finished();
                }
            }
        }
    }

    fn on_entry_added(&self, entry: &Rc<Entry>) {
        let element = entry.element();
        element.add_entry(entry.clone());
        entry.update_transition_activeness();
        let mut renderers = self.renderers.borrow_mut();
        let at = renderers.iter().position(|renderer| match renderer {
            Renderer::Entry(other) => Rc::ptr_eq(&other.element(), &element),
            Renderer::Overlay(_) => false,
        });
        match at {
            Some(at) if at + 1 < renderers.len() => {
                renderers.insert(at + 1, Renderer::Entry(entry.clone()))
            }
            _ => renderers.push(Renderer::Entry(entry.clone())),
        }
        drop(renderers);
        self.renderers_changed();
    }

    fn on_entry_removed(self: &Rc<Self>, entry: &Rc<Entry>) {
        entry.on_entry_removed();
        let element = entry.element();
        element.remove_entry(entry);
        let removed = Renderer::Entry(entry.clone());
        self.renderers.borrow_mut().retain(|renderer| !renderer.same(&removed));
        self.renderers_changed();
        if element.all_entries.borrow().is_empty() {
            let scope = Rc::downgrade(self);
            let key = element.key.clone();
            self.coroutines.launch(async move {
                let Some(scope) = scope.upgrade() else { return };
                let empty = scope
                    .elements
                    .borrow()
                    .get(&key)
                    .is_some_and(|element| element.all_entries.borrow().is_empty());
                if empty {
                    scope.elements.borrow_mut().remove(&key);
                }
            });
        }
    }

    /// As `drawInOverlay`: the renderers by z-order.
    fn draw_in_overlay(&self, scope: &mut DrawScope<'_>) {
        let version = self.renderers_version.get();
        if self.last_sorted_version.get() != Some(version) {
            // A stable insertion sort, as Compose's `sortWith`.
            let mut renderers = self.renderers.borrow_mut();
            for i in 1..renderers.len() {
                let mut j = i;
                while j > 0 && renderers[j - 1].z_order() > renderers[j].z_order() {
                    renderers.swap(j - 1, j);
                    j -= 1;
                }
            }
            self.last_sorted_version.set(Some(version));
        }
        let renderers = self.renderers.borrow().clone();
        for renderer in renderers {
            match renderer {
                Renderer::Entry(entry) => entry.draw_in_overlay(scope),
                Renderer::Overlay(overlay) => overlay.draw_in_overlay(scope),
            }
        }
    }

    fn invalidate_overlay(&self) {
        if let Some(node) = self.root_node.get() {
            node.invalidate_draw();
        }
    }
}

/// A `LookaheadScope` whose shared contents animate between its contents, drawn in its overlay,
/// as Compose's `SharedTransitionScope(content)`: `content` gets the scope and the modifier to
/// put on the layout around it (its coordinates and overlay).
#[composable]
#[allow(non_snake_case)]
pub fn SharedTransitionScope(content: impl Fn(SharedTransitionScope, Modifier) + 'static) {
    let content = Rc::new(content);
    compose_ui::LookaheadScope(compose_runtime::callback(move |lookahead| {
        let coroutines = compose_runtime::remember_coroutine_scope();
        let scope = remember(|| SharedTransitionScope {
            inner: Rc::new(ScopeInner {
                lookahead,
                coroutines,
                is_transition_active: mutable_state_of(false),
                active_entries: Cell::new(0),
                root: Cell::new(None),
                lookahead_root: Cell::new(None),
                renderers: RefCell::new(Vec::new()),
                renderers_version: mutable_state_of(0),
                last_sorted_version: Cell::new(None),
                elements: RefCell::new(HashMap::new()),
                root_node: Cell::new(None),
            }),
        });
        let root = Modifier.then_custom(RootElement { scope: scope.clone() });
        content(scope, root);
    }));
}

/// [`SharedTransitionScope`] in a box, as Compose's `SharedTransitionLayout(modifier, content)`:
/// its content stacked at the top start, the overlay drawn over it.
#[composable]
#[allow(non_snake_case)]
pub fn SharedTransitionLayout(
    #[default(crate::Modifier)] modifier: Modifier,
    content: impl Fn(SharedTransitionScope) + 'static,
) {
    let content = Rc::new(content);
    SharedTransitionScope(compose_runtime::callback(
        move |scope: SharedTransitionScope, root: Modifier| {
            let content = content.clone();
            let policy = remember(|| Rc::new(BoxLayout) as Rc<dyn MeasurePolicy>);
            let children = compose_runtime::callback(move || content(scope.clone()));
            compose_ui::Layout(modifier.clone().then(root), policy, children);
        },
    ));
}

/// A `Box`'s layout: its children at the top start, it as large as the largest.
struct BoxLayout;

impl MeasurePolicy for BoxLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let loose = constraints.loosen();
        let placeables: Vec<_> =
            measurables.iter().map(|&measurable| scope.measure(measurable, loose)).collect();
        let size = placeables.iter().fold(
            IntSize::new(constraints.min_width, constraints.min_height),
            |size, placeable| {
                IntSize::new(size.width.max(placeable.width), size.height.max(placeable.height))
            },
        );
        scope.layout_with(size.width, size.height, move |placement| {
            for placeable in &placeables {
                placement.place_relative(placeable, 0, 0);
            }
        })
    }

    fn debug_name(&self) -> &'static str {
        "SharedTransitionLayout"
    }
}

// ---- The root modifier -------------------------------------------------------------------------

/// `SharedTransitionScopeRootModifierElement`: notes the layout's coordinates now and ahead, and
/// draws the overlay after its content.
struct RootElement {
    scope: SharedTransitionScope,
}

struct RootNode {
    scope: SharedTransitionScope,
}

impl ModifierNodeElement for RootElement {
    type Node = RootNode;
    fn create(&self) -> RootNode {
        RootNode { scope: self.scope.clone() }
    }
    fn update(&self, node: &mut RootNode) {
        node.scope = self.scope.clone();
    }
    fn same(&self, other: &Self) -> bool {
        self.scope.same(&other.scope)
    }
    fn name(&self) -> &'static str {
        "SharedTransitionScopeRootModifier"
    }
}

impl ModifierNode for RootNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.scope.inner.root_node.set(Some(context.handle()));
    }
    fn on_detach(&mut self, _context: &mut NodeContext<'_>) {
        self.scope.inner.root_node.set(None);
    }
}

impl LayoutModifierNode for RootNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let placeable = scope.measure(content, constraints);
        let shared = self.scope.inner.clone();
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            if let Some(coordinates) = placement.coordinates() {
                if placement.is_looking_ahead() {
                    shared.lookahead_root.set(Some(coordinates));
                } else {
                    shared.root.set(Some(coordinates));
                }
            }
            placement.place(&placeable, 0, 0);
        })
    }
}

impl DrawModifierNode for RootNode {
    fn draws_content(&self) -> bool {
        true
    }
    fn draw_with_content(&mut self, scope: &mut ContentDrawScope<'_, '_>) {
        scope.draw_content();
        self.scope.inner.draw_in_overlay(scope);
    }
}

// ---- Shared elements and their state machine ---------------------------------------------------

/// A key's contents and their match, as Compose's `SharedElement`.
struct SharedElement {
    key: SharedKey,
    scope: Weak<ScopeInner>,
    machine: StateMachine,
    all_entries: RefCell<Vec<Rc<Entry>>>,
    /// Those enabled (a state, as `enabledEntries`).
    enabled_entries: OwnedState<EntryList>,
    momentum: Animatable<Offset>,
    spec_finalized: Cell<bool>,
}

#[derive(Clone, Default)]
struct EntryList(Vec<Rc<Entry>>);

impl PartialEq for EntryList {
    fn eq(&self, other: &EntryList) -> bool {
        self.0.len() == other.0.len() && self.0.iter().zip(&other.0).all(|(a, b)| Rc::ptr_eq(a, b))
    }
}

impl SharedElement {
    fn new(key: SharedKey, scope: Weak<ScopeInner>) -> Rc<SharedElement> {
        Rc::new_cyclic(|this: &Weak<SharedElement>| SharedElement {
            key,
            scope,
            machine: StateMachine::new(this.clone()),
            all_entries: RefCell::new(Vec::new()),
            enabled_entries: mutable_state_of(EntryList::default()),
            momentum: Animatable::new(Offset::ZERO),
            spec_finalized: Cell::new(false),
        })
    }

    fn scope(&self) -> Rc<ScopeInner> {
        self.scope.upgrade().expect("a shared element outlived its scope")
    }

    fn state(&self) -> Rc<MatchState> {
        self.machine.state.get().0
    }

    fn enabled_entries(&self) -> Vec<Rc<Entry>> {
        self.enabled_entries.get().0
    }

    fn enabled_entries_untracked(&self) -> Vec<Rc<Entry>> {
        compose_runtime::snapshots::Snapshot::without_read_observation(|| self.enabled_entries())
    }

    /// As `updateMatch`.
    fn update_match(&self) {
        let all = self.all_entries.borrow().clone();
        let mut enabled = Vec::new();
        let mut has_visible_content = false;
        for entry in &all {
            if entry.is_enabled() {
                if entry.bounds_animation().target() {
                    has_visible_content = true;
                }
                enabled.push(entry.clone());
            }
        }
        self.enabled_entries.set(EntryList(enabled));
        self.machine.check_for_and_defer_state_updates(has_visible_content);
        for entry in &all {
            entry.update_transition_activeness();
        }
    }

    fn update_exit_velocity(&self, velocity: Velocity) {
        let momentum = self.momentum.clone();
        let element = self.machine.element.clone();
        self.scope().coroutines.launch(async move {
            momentum
                .animate_to_with_velocity(
                    Offset::ZERO,
                    momentum_spring(),
                    Offset::new(velocity.x, velocity.y),
                )
                .await;
            if let Some(element) = element.upgrade() {
                element.spec_finalized.set(true);
            }
        });
    }

    /// As `momentumAnimationOffset`.
    fn momentum_offset(&self) -> Offset {
        let scope = self.scope();
        if !self.spec_finalized.get()
            && scope.is_transition_active.get()
            && self.momentum.is_running()
        {
            let target = self.enabled_entries().into_iter().find(|entry| entry.target());
            if let Some(target) = target {
                if let AnimationSpec::Spring(spec) =
                    &*target.bounds_animation().animation_spec.borrow()
                {
                    let spring = spring_with(spec.damping_ratio, spec.stiffness)
                        .visibility_threshold(Offset::new(3.0, 3.0));
                    let momentum = self.momentum.clone();
                    scope.coroutines.launch(async move {
                        momentum.animate_to(Offset::ZERO, spring).await;
                    });
                }
                self.spec_finalized.set(true);
            }
        }
        self.momentum.value()
    }

    fn on_shared_transition_finished(&self) {
        let enabled = self.enabled_entries();
        if enabled.len() <= 1 || !enabled.iter().any(|entry| entry.target()) {
            self.machine.reset_state();
        }
    }

    /// As `foundMatch` (state reads).
    fn found_match(&self) -> bool {
        let state = self.state();
        state.active_match_found()
            || state.match_is_or_has_been_configured()
            || self.machine.active_match_deferred()
    }

    fn bounds_transform_is_active(&self) -> bool {
        self.state().match_is_or_has_been_configured()
    }

    fn invalidate_target_bounds_provider(&self) {
        self.machine.invalidate_target_bounds_provider();
    }

    fn try_initializing_current_bounds(&self) -> Option<Rect> {
        self.machine.process_pending_request();
        self.machine.initialize_current_bounds()
    }

    /// As `onLookaheadPlaced`: the visible content's place ahead configures the match.
    fn on_lookahead_placed(&self, placement: &mut PlacementScope<'_>, entry: &Entry) {
        self.machine.process_pending_request();
        let state = self.state();
        if matches!(*state, MatchState::NoMatchFound) || !entry.is_enabled() {
            return;
        }
        if entry.bounds_animation().target() && state.active_match_found() {
            let scope = self.scope();
            let (Some(it), Some(root)) = (placement.coordinates(), scope.lookahead_root.get())
            else {
                return;
            };
            let size = it.size();
            let lookahead_size = Size::new(size.width as f32, size.height as f32);
            let lookahead = scope.lookahead;
            let top_left = lookahead.local_lookahead_position_of(root, it, Offset::ZERO);
            let structural =
                lookahead.local_lookahead_position_of_with(root, it, Offset::ZERO, false);
            self.machine.configure_active_match(lookahead_size, top_left, structural);
        }
    }

    fn add_entry(&self, entry: Rc<Entry>) {
        self.all_entries.borrow_mut().push(entry);
        self.update_match();
    }

    fn remove_entry(&self, entry: &Rc<Entry>) {
        self.all_entries.borrow_mut().retain(|it| !Rc::ptr_eq(it, entry));
        self.update_match();
    }
}

fn momentum_spring() -> AnimationSpec {
    AnimationSpec::Spring(
        spring_with(Spring::DAMPING_RATIO_NO_BOUNCY, Spring::STIFFNESS_MEDIUM_LOW)
            .visibility_threshold(Offset::new(3.0, 3.0)),
    )
}

/// What a match's target is, as `TargetData` (states).
struct TargetData {
    size: OwnedState<Size>,
    initial_mfr_offset: OwnedState<Offset>,
    target_structural_offset: OwnedState<Offset>,
    current_mfr_offset: OwnedState<Offset>,
}

impl TargetData {
    fn new(size: Size, initial_mfr_offset: Offset, target_structural_offset: Offset) -> TargetData {
        TargetData {
            size: mutable_state_of(size),
            initial_mfr_offset: mutable_state_of(initial_mfr_offset),
            target_structural_offset: mutable_state_of(target_structural_offset),
            current_mfr_offset: mutable_state_of(initial_mfr_offset),
        }
    }

    fn target_bounds(&self) -> Rect {
        Rect::from_offset_size(
            self.initial_mfr_offset.get() + self.target_structural_offset.get(),
            self.size.get(),
        )
    }

    /// As `calculateOffsetFromDirectManipulation`.
    fn offset_from_direct_manipulation(&self, animated: Rect) -> Offset {
        animated.top_left() - self.initial_mfr_offset.get() + self.current_mfr_offset.get()
    }

    /// As `updateTargetData`.
    fn update(&self, size: Size, top_left: Offset, structural: Offset, provider_changed: bool) {
        if self.target_structural_offset.get() != structural
            || self.size.get() != size
            || provider_changed
        {
            self.size.set(size);
            self.target_structural_offset.set(structural);
            if provider_changed {
                let initial = (top_left - structural)
                    - (self.current_mfr_offset.get() - self.initial_mfr_offset.get());
                self.initial_mfr_offset.set(initial);
            }
        }
        self.current_mfr_offset.set(top_left - structural);
    }
}

/// A match's state, as `SharedTransitionStateMachine.State`.
enum MatchState {
    NoMatchFound,
    /// `ActiveMatchFoundConfigPending`.
    Pending {
        provider_before_config: RefCell<Option<Provider>>,
        target_data: Option<Rc<TargetData>>,
        current_bounds: OwnedState<Option<Rect>>,
    },
    /// `ActiveMatchConfigured`.
    Configured {
        target_data: Rc<TargetData>,
        provider: RefCell<Provider>,
        current_bounds: OwnedState<Rect>,
    },
    /// `ActiveMatchRemovedDuringTransition`.
    Removed {
        target_data: Rc<TargetData>,
        current_bounds: OwnedState<Rect>,
    },
}

impl MatchState {
    fn active_match_found(&self) -> bool {
        matches!(self, MatchState::Pending { .. } | MatchState::Configured { .. })
    }

    fn match_is_or_has_been_configured(&self) -> bool {
        matches!(self, MatchState::Configured { .. } | MatchState::Removed { .. })
    }

    fn target_data(&self) -> Option<Rc<TargetData>> {
        match self {
            MatchState::NoMatchFound => None,
            MatchState::Pending { target_data, .. } => target_data.clone(),
            MatchState::Configured { target_data, .. }
            | MatchState::Removed { target_data, .. } => Some(target_data.clone()),
        }
    }

    /// As `currentBounds` (a state read).
    fn current_bounds(&self) -> Option<Rect> {
        match self {
            MatchState::NoMatchFound => None,
            MatchState::Pending { current_bounds, .. } => current_bounds.get(),
            MatchState::Configured { current_bounds, .. }
            | MatchState::Removed { current_bounds, .. } => Some(current_bounds.get()),
        }
    }

    fn update_bounds(&self, bounds: Rect) {
        match self {
            MatchState::NoMatchFound => {}
            MatchState::Pending { current_bounds, .. } => current_bounds.set(Some(bounds)),
            MatchState::Configured { current_bounds, .. }
            | MatchState::Removed { current_bounds, .. } => current_bounds.set(bounds),
        }
    }
}

/// A state, compared by identity.
#[derive(Clone)]
struct StateRef(Rc<MatchState>);

impl PartialEq for StateRef {
    fn eq(&self, other: &StateRef) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// What an entry's node gives the state machine, as `BoundsProvider`.
type Provider = Rc<NodeShared>;

fn same_provider(a: &Option<Provider>, b: &Option<Provider>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Request {
    None,
    MatchFound,
    VisibleContentAbsentDuringTransition,
}

/// As `SharedTransitionStateMachine`.
struct StateMachine {
    element: Weak<SharedElement>,
    state: OwnedState<StateRef>,
    last_handled_request: Cell<u32>,
    request_id: OwnedState<u32>,
    request: Cell<Request>,
    target_provider: RefCell<Option<Provider>>,
    target_provider_request: OwnedState<u32>,
    last_handled_target_provider_request: Cell<u32>,
}

impl StateMachine {
    fn new(element: Weak<SharedElement>) -> StateMachine {
        StateMachine {
            element,
            state: mutable_state_of(StateRef(Rc::new(MatchState::NoMatchFound))),
            last_handled_request: Cell::new(0),
            request_id: mutable_state_of(0),
            request: Cell::new(Request::None),
            target_provider: RefCell::new(None),
            target_provider_request: mutable_state_of(0),
            last_handled_target_provider_request: Cell::new(0),
        }
    }

    fn element(&self) -> Rc<SharedElement> {
        self.element.upgrade().expect("a state machine outlived its element")
    }

    fn set_state(&self, state: MatchState) {
        // Compose's `NoMatchFound` is an object: setting it again is no change, which a
        // composition reading `isMatchFound` would hear, again each time it resets the machine.
        if matches!(state, MatchState::NoMatchFound)
            && matches!(*self.current(), MatchState::NoMatchFound)
        {
            return;
        }
        self.state.set(StateRef(Rc::new(state)));
    }

    fn current(&self) -> Rc<MatchState> {
        compose_runtime::snapshots::Snapshot::without_read_observation(|| self.state.get().0)
    }

    fn active_match_deferred(&self) -> bool {
        self.request.get() == Request::MatchFound
    }

    fn reset_state(&self) {
        self.request.set(Request::None);
        self.last_handled_request.set(self.untracked_request_id());
        self.set_state(MatchState::NoMatchFound);
    }

    fn untracked_request_id(&self) -> u32 {
        compose_runtime::snapshots::Snapshot::without_read_observation(|| self.request_id.get())
    }

    fn defer_request(&self, request: Request) {
        self.request.set(request);
        self.request_id.set(self.last_handled_request.get() + 1);
    }

    /// As `processPendingRequest`.
    fn process_pending_request(&self) {
        let id = self.request_id.get();
        if id != self.last_handled_request.get() {
            self.last_handled_request.set(id);
            let state = self.current();
            let provider = self.target_provider.borrow().clone();
            match self.request.get() {
                Request::None => {}
                Request::MatchFound => self.on_match_found(&state, provider),
                Request::VisibleContentAbsentDuringTransition => {
                    let element = self.element();
                    let target_enabled = element
                        .enabled_entries_untracked()
                        .iter()
                        .any(|entry| same_provider(&entry.bounds_provider.borrow(), &provider));
                    if target_enabled {
                        self.set_state(MatchState::NoMatchFound);
                    } else {
                        self.on_visible_content_removed(&state);
                    }
                }
            }
            self.request.set(Request::None);
        }
        self.update_target_bounds_provider();
    }

    fn on_match_found(&self, state: &Rc<MatchState>, previous: Option<Provider>) {
        match &**state {
            MatchState::NoMatchFound => self.set_state(MatchState::Pending {
                provider_before_config: RefCell::new(previous),
                target_data: None,
                current_bounds: mutable_state_of(None),
            }),
            MatchState::Pending { provider_before_config, .. } => {
                let mut before = provider_before_config.borrow_mut();
                if before.is_none() {
                    *before = previous;
                }
            }
            MatchState::Configured { .. } => {}
            MatchState::Removed { target_data, current_bounds } => {
                self.set_state(MatchState::Pending {
                    provider_before_config: RefCell::new(previous),
                    target_data: Some(target_data.clone()),
                    current_bounds: mutable_state_of(Some(current_bounds.get())),
                })
            }
        }
    }

    fn on_visible_content_removed(&self, state: &Rc<MatchState>) {
        match &**state {
            MatchState::NoMatchFound | MatchState::Removed { .. } => {}
            MatchState::Pending { .. } => self.set_state(MatchState::NoMatchFound),
            MatchState::Configured { target_data, provider, current_bounds } => {
                let last_target = Rect::from_offset_size(
                    target_data.current_mfr_offset.get()
                        + target_data.target_structural_offset.get(),
                    target_data.size.get(),
                );
                let provider = provider.borrow().clone();
                match provider.calculate_alternative_target_bounds(last_target) {
                    None => self.set_state(MatchState::NoMatchFound),
                    Some(new_target) => {
                        let current_mfr = target_data.current_mfr_offset.get();
                        let data = TargetData::new(
                            new_target.size(),
                            target_data.initial_mfr_offset.get(),
                            new_target.top_left() - current_mfr,
                        );
                        data.current_mfr_offset.set(current_mfr);
                        self.set_state(MatchState::Removed {
                            target_data: Rc::new(data),
                            current_bounds: mutable_state_of(current_bounds.get()),
                        });
                    }
                }
            }
        }
    }

    /// As `checkForAndDeferStateUpdates`.
    fn check_for_and_defer_state_updates(&self, has_visible_content: bool) {
        let element = self.element();
        let enabled = element.enabled_entries_untracked().len();
        if enabled > 1 && has_visible_content {
            self.defer_request(Request::MatchFound);
        } else if compose_runtime::snapshots::Snapshot::without_read_observation(|| {
            element.scope().is_transition_active.get()
        }) {
            if !has_visible_content {
                self.defer_request(Request::VisibleContentAbsentDuringTransition);
            }
        } else {
            self.reset_state();
        }
        self.invalidate_target_bounds_provider();
    }

    fn invalidate_target_bounds_provider(&self) {
        let element = self.element();
        let target = element.enabled_entries_untracked().into_iter().find(|entry| entry.target());
        let current = self.target_provider.borrow().clone();
        if target.is_none() && current.is_none() {
            return;
        }
        let target_provider = target.and_then(|entry| entry.bounds_provider.borrow().clone());
        if target_provider.is_some() && same_provider(&target_provider, &current) {
            return;
        }
        self.target_provider_request.set(self.last_handled_target_provider_request.get() + 1);
    }

    fn update_target_bounds_provider(&self) {
        let request = self.target_provider_request.get();
        if request != self.last_handled_target_provider_request.get() {
            let element = self.element();
            let active = element.scope().is_transition_active.get();
            let entries = if active {
                element.enabled_entries()
            } else {
                element.all_entries.borrow().clone()
            };
            let provider = entries
                .into_iter()
                .find(|entry| entry.target())
                .and_then(|entry| entry.bounds_provider.borrow().clone());
            *self.target_provider.borrow_mut() = provider;
            self.last_handled_target_provider_request.set(request);
        }
    }

    /// As `initializeCurrentBounds`.
    fn initialize_current_bounds(&self) -> Option<Rect> {
        let state = self.state.get().0;
        if let MatchState::Pending { provider_before_config, current_bounds, .. } = &*state {
            if let Some(bounds) = current_bounds.get() {
                return Some(bounds);
            }
            let element = self.element();
            let last_target = provider_before_config
                .borrow()
                .clone()
                .or_else(|| element.first_enabled_provider());
            if let Some(bounds) = element.bounds_from_last_target(last_target) {
                current_bounds.set(Some(bounds));
            }
            return current_bounds.get();
        }
        state.current_bounds()
    }

    /// As `configureActiveMatch`.
    fn configure_active_match(&self, lookahead_size: Size, top_left: Offset, structural: Offset) {
        let state = self.current();
        let Some(provider) = self.target_provider.borrow().clone() else { return };
        match &*state {
            MatchState::Pending { provider_before_config, target_data, current_bounds } => {
                let element = self.element();
                let data = target_data.clone().unwrap_or_else(|| {
                    Rc::new(TargetData::new(lookahead_size, top_left - structural, structural))
                });
                let current = current_bounds
                    .get()
                    .or_else(|| {
                        let before = provider_before_config
                            .borrow()
                            .clone()
                            .or_else(|| element.first_enabled_provider());
                        element.bounds_from_last_target(before)
                    })
                    .unwrap_or_else(|| Rect::from_offset_size(top_left, lookahead_size));
                data.update(lookahead_size, top_left, structural, true);
                self.set_state(MatchState::Configured {
                    target_data: data,
                    provider: RefCell::new(provider),
                    current_bounds: mutable_state_of(current),
                });
            }
            MatchState::Configured { target_data, provider: current, .. } => {
                let changed = !Rc::ptr_eq(&current.borrow(), &provider);
                target_data.update(lookahead_size, top_left, structural, changed);
                *current.borrow_mut() = provider;
            }
            other => {
                debug_assert!(
                    false,
                    "an active match configured in another state: {}",
                    other.match_is_or_has_been_configured()
                );
            }
        }
    }
}

impl SharedElement {
    /// The provider of the first entry that's enabled.
    fn first_enabled_provider(&self) -> Option<Provider> {
        let enabled = self.enabled_entries_untracked();
        let all = self.all_entries.borrow().clone();
        all.into_iter()
            .find(|entry| enabled.iter().any(|it| Rc::ptr_eq(it, entry)))
            .and_then(|entry| entry.bounds_provider.borrow().clone())
    }

    /// As `obtainBoundsFromLastTarget`.
    fn bounds_from_last_target(&self, last: Option<Provider>) -> Option<Rect> {
        let last = last?;
        let present = self.all_entries.borrow().iter().any(|entry| {
            entry.bounds_provider.borrow().as_ref().is_some_and(|it| Rc::ptr_eq(it, &last))
        });
        if present { last.last_bounds_in_shared_transition_scope() } else { None }
    }
}

// ---- Bounds animations -------------------------------------------------------------------------

/// A content's bounds animating on its visibility's transition, as `BoundsAnimation`.
struct BoundsAnimation {
    scope: Weak<ScopeInner>,
    transition: Transition<bool>,
    animation: RefCell<DeferredAnimation<bool, Rect>>,
    bounds_transform: RefCell<BoundsTransform>,
    animation_spec: Rc<RefCell<AnimationSpec>>,
    /// As `animationState` (a state: read by `isAnimating`).
    animation_state: OwnedState<Option<DataRef>>,
    current_bounds: Rc<Cell<Rect>>,
    target_bounds: Rc<Cell<Rect>>,
    element: Rc<SharedElement>,
}

#[derive(Clone)]
struct DataRef(Rc<DeferredData<bool, Rect>>);

impl PartialEq for DataRef {
    fn eq(&self, other: &DataRef) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

fn default_bounds_spec() -> AnimationSpec {
    AnimationSpec::Spring(
        spring_with(Spring::DAMPING_RATIO_NO_BOUNCY, Spring::STIFFNESS_MEDIUM_LOW)
            .visibility_threshold(Rect::VISIBILITY_THRESHOLD),
    )
}

impl BoundsAnimation {
    fn new(
        scope: Weak<ScopeInner>,
        transition: Transition<bool>,
        animation: DeferredAnimation<bool, Rect>,
        bounds_transform: BoundsTransform,
        element: Rc<SharedElement>,
    ) -> BoundsAnimation {
        BoundsAnimation {
            scope,
            transition,
            animation: RefCell::new(animation),
            bounds_transform: RefCell::new(bounds_transform),
            animation_spec: Rc::new(RefCell::new(default_bounds_spec())),
            animation_state: mutable_state_of(None),
            current_bounds: Rc::new(Cell::new(Rect::ZERO)),
            target_bounds: Rc::new(Cell::new(Rect::ZERO)),
            element,
        }
    }

    fn update_animation(
        &self,
        animation: DeferredAnimation<bool, Rect>,
        bounds_transform: BoundsTransform,
    ) {
        if !self.animation.borrow().same(&animation) {
            *self.animation.borrow_mut() = animation;
            self.animation_state.set(None);
            *self.animation_spec.borrow_mut() = default_bounds_spec();
        }
        *self.bounds_transform.borrow_mut() = bounds_transform;
    }

    /// As `isRunning` (state reads).
    fn is_running(&self) -> bool {
        self.transition.root_is_changing()
    }

    fn target(&self) -> bool {
        self.transition.target_state()
    }

    fn is_transition_active(&self) -> bool {
        self.scope.upgrade().is_some_and(|scope| scope.is_transition_active.get())
    }

    /// As `value`.
    fn value(&self) -> Option<Rect> {
        if !self.is_transition_active() {
            return None;
        }
        let value = self.animation_state.get()?.0.value();
        let offset = self.element.momentum_offset();
        Some(if offset != Offset::ZERO { value.translate(offset) } else { value })
    }

    /// As `animate`.
    fn animate(
        &self,
        current: Rect,
        target: Rect,
        forced_transform: Option<BoundsTransform>,
        forced_initial_value: Option<Rect>,
        forced_initial_velocity: Option<AnimationVector>,
    ) {
        if !self.is_transition_active() {
            return;
        }
        self.current_bounds.set(current);
        self.target_bounds.set(target);
        let started = compose_runtime::snapshots::Snapshot::without_read_observation(|| {
            self.animation_state.get().is_some()
        });
        if !started {
            let transform =
                forced_transform.unwrap_or_else(|| self.bounds_transform.borrow().clone());
            *self.animation_spec.borrow_mut() = transform.create_animation_spec(current, target);
        }
        let spec = self.animation_spec.clone();
        let (current_bounds, target_bounds) =
            (self.current_bounds.clone(), self.target_bounds.clone());
        let transition_target = self.transition.clone();
        let data = self.animation.borrow().animate_forced(
            move |_| spec.borrow().clone(),
            forced_initial_value,
            forced_initial_velocity,
            move |state| {
                let target = compose_runtime::snapshots::Snapshot::without_read_observation(|| {
                    transition_target.target_state()
                });
                if *state == target { target_bounds.get() } else { current_bounds.get() }
            },
        );
        let data = DataRef(data);
        let same = compose_runtime::snapshots::Snapshot::without_read_observation(|| {
            self.animation_state.get().as_ref() == Some(&data)
        });
        if !same {
            self.animation_state.set(Some(data));
        }
    }
}

// ---- Entries -----------------------------------------------------------------------------------

/// A shared modifier's call site, as `SharedElementEntry`.
struct Entry {
    is_attached: OwnedState<bool>,
    is_transition_active: Cell<bool>,
    z_index: OwnedState<f32>,
    render_in_overlay_during_transition: OwnedState<bool>,
    element: RefCell<Rc<SharedElement>>,
    bounds_animation: RefCell<Rc<BoundsAnimation>>,
    placeholder_size: RefCell<PlaceholderSize>,
    render_only_when_visible: Cell<bool>,
    overlay_clip: RefCell<OverlayClip>,
    user_state: RefCell<SharedContentState>,
    clip_path_in_overlay: RefCell<Option<Path>>,
    parent_state: RefCell<Option<Weak<Entry>>>,
    bounds_provider: RefCell<Option<Provider>>,
    /// The layer it's recorded into for the overlay (a state, as `layer`).
    layer: RefCell<Option<GraphicsLayer>>,
    layer_version: OwnedState<u32>,
    /// The deferred state it followed in the deferred phase, kept through the hand-off, as
    /// `deferredTransformState`.
    deferred_transform_state: RefCell<Option<Rc<SharedMutableTransformState>>>,
    /// Whether its node took the hand-off's start already, as `hasHandoffOccurred`.
    has_handoff_occurred: Cell<bool>,
}

impl Entry {
    fn new(
        element: Rc<SharedElement>,
        bounds_animation: Rc<BoundsAnimation>,
        options: &SharedOptions,
        render_only_when_visible: bool,
        user_state: SharedContentState,
    ) -> Entry {
        Entry {
            is_attached: mutable_state_of(false),
            is_transition_active: Cell::new(false),
            z_index: mutable_state_of(options.z_index_in_overlay),
            render_in_overlay_during_transition: mutable_state_of(
                options.render_in_overlay_during_transition,
            ),
            element: RefCell::new(element),
            bounds_animation: RefCell::new(bounds_animation),
            placeholder_size: RefCell::new(options.placeholder_size.clone()),
            render_only_when_visible: Cell::new(render_only_when_visible),
            overlay_clip: RefCell::new(options.clip_in_overlay_during_transition.clone()),
            user_state: RefCell::new(user_state),
            clip_path_in_overlay: RefCell::new(None),
            parent_state: RefCell::new(None),
            bounds_provider: RefCell::new(None),
            layer: RefCell::new(None),
            layer_version: mutable_state_of(0),
            deferred_transform_state: RefCell::new(None),
            has_handoff_occurred: Cell::new(false),
        }
    }

    fn update(
        &self,
        element: Rc<SharedElement>,
        bounds_animation: Rc<BoundsAnimation>,
        options: &SharedOptions,
        render_only_when_visible: bool,
        state: &SharedContentState,
    ) {
        *self.element.borrow_mut() = element;
        self.render_only_when_visible.set(render_only_when_visible);
        *self.bounds_animation.borrow_mut() = bounds_animation;
        *self.placeholder_size.borrow_mut() = options.placeholder_size.clone();
        *self.overlay_clip.borrow_mut() = options.clip_in_overlay_during_transition.clone();
        if self.z_index.get() != options.z_index_in_overlay {
            self.z_index.set(options.z_index_in_overlay);
            self.element().scope().renderers_changed();
        }
        self.render_in_overlay_during_transition.set(options.render_in_overlay_during_transition);
        *self.user_state.borrow_mut() = state.clone();
    }

    fn element(&self) -> Rc<SharedElement> {
        self.element.borrow().clone()
    }

    fn bounds_animation(&self) -> Rc<BoundsAnimation> {
        self.bounds_animation.borrow().clone()
    }

    fn parent(&self) -> Option<Rc<Entry>> {
        self.parent_state.borrow().clone()?.upgrade()
    }

    fn target(&self) -> bool {
        self.bounds_animation().target()
    }

    /// As `isEnabled` (state reads).
    fn is_enabled(&self) -> bool {
        self.is_attached.get() && self.user_state.borrow().is_enabled_by_user()
    }

    fn set_layer(&self, layer: Option<GraphicsLayer>) {
        let changed = match (&*self.layer.borrow(), &layer) {
            (Some(a), Some(b)) => !a.ptr_eq(b),
            (None, None) => false,
            _ => true,
        };
        if changed {
            if let Some(old) = self.layer.borrow().as_ref() {
                old.release();
            }
            *self.layer.borrow_mut() = layer;
            self.layer_version.update(|version| version.wrapping_add(1));
        }
    }

    /// As `shouldRenderAtAll`.
    fn should_render_at_all(&self) -> bool {
        let state = self.element().state();
        self.target()
            || (state.match_is_or_has_been_configured() && !state.active_match_found())
            || !self.render_only_when_visible.get()
    }

    /// As `shouldRenderInOverlay` (state reads): also while its content is moved by hand.
    fn should_render_in_overlay(&self) -> bool {
        let element = self.element();
        self.should_render_at_all()
            && element.bounds_transform_is_active()
            && self.is_enabled()
            && self.render_in_overlay_during_transition.get()
            && (element.scope().is_transition_active.get() || self.is_mutating())
    }

    /// The deferred state of the content its node is in, as `boundsProvider`'s
    /// `modifierLocalTransformState`.
    fn container_transform_state(&self) -> Option<Rc<SharedMutableTransformState>> {
        let provider = self.bounds_provider.borrow().clone()?;
        provider.transform_state()
    }

    /// Whether the content it's in is moved by hand, as `isMutating` (a state read).
    fn is_mutating(&self) -> bool {
        self.container_transform_state().is_some_and(|state| state.is_mutating())
    }

    /// The deferred state moving it, as `activeMutableTransformState` (state reads): none unless
    /// its config permits it and its transition is a deferred one's. During the deferred phase,
    /// the leaving content's, for both contents, so they move together; kept through the
    /// hand-off.
    fn active_mutable_transform_state(&self) -> Option<Rc<SharedMutableTransformState>> {
        if !self.user_state.borrow().config().permit_transform_during_deferred_transition() {
            return None;
        }
        if !self.bounds_animation().transition.is_deferred() {
            return None;
        }
        let state = self.container_transform_state()?;
        if state.is_handoff_active()
            && let Some(kept) = self.deferred_transform_state.borrow().clone()
        {
            return Some(kept);
        }
        let mutating = state.is_mutating();
        // During the deferred phase the transition is held at its old state: the target is the
        // leaving content.
        let is_incoming = if mutating { !self.target() } else { self.target() };
        let resolved = if is_incoming {
            self.element()
                .enabled_entries()
                .into_iter()
                .find(|it| if it.is_mutating() { it.target() } else { !it.target() })
                .and_then(|it| it.container_transform_state())
                .unwrap_or_else(|| state.clone())
        } else {
            state.clone()
        };
        if mutating {
            *self.deferred_transform_state.borrow_mut() = Some(resolved.clone());
        } else if !state.is_handoff_active() {
            *self.deferred_transform_state.borrow_mut() = None;
        }
        Some(resolved)
    }

    /// As `shouldRenderInPlace`.
    fn should_render_in_place(&self) -> bool {
        !self.element().bounds_transform_is_active()
            || (!self.should_render_in_overlay() && self.should_render_at_all())
    }

    /// As `observationBlock`: what an entry's node observes to update the match.
    fn observe(&self) {
        self.target();
        self.is_enabled();
        self.bounds_animation().is_running();
        if let Some(state) = self.active_mutable_transform_state() {
            state.is_mutating();
        }
    }

    /// As `updateTransitionActiveness`.
    fn update_transition_activeness(&self) {
        let old = self.is_transition_active.get();
        let active = compose_runtime::snapshots::Snapshot::without_read_observation(|| {
            self.is_enabled()
                && self.element().found_match()
                && (self.bounds_animation().is_running()
                    || self.active_mutable_transform_state().is_some_and(|it| it.is_mutating()))
        });
        self.is_transition_active.set(active);
        if active != old {
            self.element().scope().on_node_transition_activeness_changed(active);
        }
    }

    fn on_entry_removed(&self) {
        if self.is_transition_active.replace(false) {
            self.element().scope().on_node_transition_activeness_changed(false);
        }
    }

    /// As `calculateTargetBounds`.
    fn calculate_target_bounds(&self, before_removal: Rect) -> Option<Rect> {
        let state = self.user_state.borrow().clone();
        let scope = self.element().scope();
        let size = scope.lookahead_root.get().map_or(IntSize::ZERO, |it| it.size());
        let size = Size::new(size.width as f32, size.height as f32);
        state.config().alternative_target_bounds_in_transition_scope_after_removal(
            &state,
            before_removal,
            size,
        )
    }

    /// As `drawInOverlay`.
    fn draw_in_overlay(&self, scope: &mut DrawScope<'_>) {
        self.layer_version.get();
        let render = self.should_render_in_overlay();
        if render && self.layer.borrow().is_none() {
            self.set_layer(Some(GraphicsLayer::new()));
        }
        let Some(layer) = self.layer.borrow().clone() else { return };
        let Some(bounds) = self.element().state().current_bounds() else { return };
        if render {
            // The layer holds the node's drawing in its own coordinates, the content at where it
            // was placed in the node: at the bounds, the content, as Compose's (D1341).
            let placed = self
                .bounds_provider
                .borrow()
                .as_ref()
                .map_or(IntOffset::ZERO, |it| it.content_offset.get());
            let (x, y) = (bounds.left - placed.x as f32, bounds.top - placed.y as f32);
            // Moved with its content by hand: the content's scale around its pivot in the layout,
            // and its offset.
            let mut scale = 1.0;
            let mut offset = IntOffset::ZERO;
            let mut pivot = Offset::ZERO;
            let state = self.active_mutable_transform_state();
            if let Some(state) = state.filter(|it| it.is_mutating())
                && let Some(parent) = state.parent_layout_coordinates()
                && let Some(root) = self.element().scope().root()
                && parent.is_attached()
                && root.is_attached()
            {
                scale = state.active_scale();
                offset = state.active_offset();
                pivot = calculate_pivot(&parent, &root, state.active_transform_origin());
            }
            let clip = self.clip_path_in_overlay.borrow().clone();
            scope.translate(offset.x as f32, offset.y as f32, |scope| {
                scope.scale(scale, scale, pivot, |scope| match clip {
                    Some(clip) => scope.clip_path(&clip, |scope| {
                        scope.translate(x, y, |scope| scope.draw_layer(&layer));
                    }),
                    None => scope.translate(x, y, |scope| scope.draw_layer(&layer)),
                });
            });
        }
    }
}

/// The pivot of a content's scale in the layout, as `calculatePivot`: its origin in the content's
/// bounds in the layout, so a shared element drawn in the overlay scales with its content.
fn calculate_pivot(
    parent: &LayoutCoordinates,
    root: &LayoutCoordinates,
    transform_origin: TransformOrigin,
) -> Offset {
    let bounds = root.local_bounding_box_of(parent, false);
    Offset::new(
        bounds.left + bounds.width() * transform_origin.pivot_fraction_x,
        bounds.top + bounds.height() * transform_origin.pivot_fraction_y,
    )
}

/// `point` scaled around `pivot` and moved by `offset`, as `Offset.transform`.
fn transform_point(point: Offset, pivot: Offset, scale: f32, offset: IntOffset) -> Offset {
    Offset::new(
        (point.x - pivot.x) * scale + pivot.x + offset.x as f32,
        (point.y - pivot.y) * scale + pivot.y + offset.y as f32,
    )
}

// ---- The shared node ---------------------------------------------------------------------------

/// The modifier local of the shared content around a node, as
/// `ModifierLocalSharedElementInternalState`.
static LOCAL_ENTRY: ModifierLocal<Option<EntryArg>> =
    compose_ui::modifier_local_of("SharedElementInternalState", || None);

/// `SharedBoundsNodeElement`.
struct SharedBoundsElement {
    entry: Rc<Entry>,
}

/// What a shared node and its placement blocks share: its `BoundsProvider` side.
struct NodeShared {
    entry: RefCell<Rc<Entry>>,
    handle: Cell<Option<NodeHandle>>,
    is_placed: Cell<bool>,
    bounds_before_detached: Cell<Option<Rect>>,
    /// The deferred state of the content it's in, read when attached, as
    /// `resolvedTransformState`.
    transform_state: RefCell<Option<Rc<SharedMutableTransformState>>>,
    /// Where the hand-off from a deferred phase starts, and at what velocity, as
    /// `forcedHandoffBounds` and `forcedHandoffVelocity`.
    forced_handoff_bounds: Cell<Option<Rect>>,
    forced_handoff_velocity: Cell<Option<AnimationVector>>,
    /// Where it last placed its content in itself: what its layer's recording is moved by, since
    /// compose-ui draws a node in its own coordinates where Compose draws a layout node's drawing
    /// in its content's (D1341).
    content_offset: Cell<IntOffset>,
}

impl NodeShared {
    /// As `modifierLocalTransformState`: none once detached.
    fn transform_state(&self) -> Option<Rc<SharedMutableTransformState>> {
        self.handle.get().filter(|it| it.is_attached())?;
        self.transform_state.borrow().clone()
    }

    /// Where the hand-off starts: its bounds where the deferred phase left them, as
    /// `updateDeferredHandoffValues`. Once per hand-off.
    fn update_deferred_handoff_values(&self) {
        let entry = self.entry();
        let element = entry.element();
        let state = element.state();
        if state.target_data().is_none() {
            return;
        }
        let Some(current) = state.current_bounds() else { return };
        let mutable = entry.active_mutable_transform_state();
        let Some(mutable) = mutable.filter(|it| it.is_handoff_active()) else {
            entry.has_handoff_occurred.set(false);
            return;
        };
        if entry.has_handoff_occurred.get() {
            return;
        }
        let (Some(parent), Some(root)) = (mutable.parent_layout_coordinates(), self.scope().root())
        else {
            return;
        };
        if !parent.is_attached() || !root.is_attached() {
            return;
        }
        let manual_scale = mutable.last_manual_scale();
        let manual_offset = mutable.last_manual_slide();
        let pivot = calculate_pivot(&parent, &root, mutable.last_transform_origin());
        let top_left = transform_point(current.top_left(), pivot, manual_scale, manual_offset);
        let own_container = self.transform_state().is_some_and(|it| Rc::ptr_eq(&it, &mutable));
        // Drawn in place in its own content, the content's layer scales it already.
        let container_applies = !entry.should_render_in_overlay() && own_container;
        let scale = if container_applies { 1.0 } else { manual_scale };
        self.forced_handoff_bounds.set(Some(Rect::new(
            top_left.x,
            top_left.y,
            top_left.x + current.width() * scale,
            top_left.y + current.height() * scale,
        )));
        if container_applies {
            self.forced_handoff_velocity.set(None);
        } else {
            let scale_velocity = mutable.scale_handoff_velocity().map_or(0.0, |it| it[0]);
            let (offset_x, offset_y) =
                mutable.slide_handoff_velocity().map_or((0.0, 0.0), |it| (it[0], it[1]));
            let left = (current.left - pivot.x) * scale_velocity + offset_x;
            let top = (current.top - pivot.y) * scale_velocity + offset_y;
            let right = (current.right - pivot.x) * scale_velocity + offset_x;
            let bottom = (current.bottom - pivot.y) * scale_velocity + offset_y;
            self.forced_handoff_velocity
                .set(Some(AnimationVector::from([left, top, right, bottom])));
        }
        entry.has_handoff_occurred.set(true);
    }

    fn entry(&self) -> Rc<Entry> {
        self.entry.borrow().clone()
    }

    fn scope(&self) -> Rc<ScopeInner> {
        self.entry().element().scope()
    }

    /// As `lastBoundsInSharedTransitionScope`.
    fn last_bounds_in_shared_transition_scope(&self) -> Option<Rect> {
        let handle = self.handle.get().filter(|it| it.is_attached())?;
        if !self.is_placed.get() {
            return self.bounds_before_detached.get();
        }
        let root = self.scope().root()?;
        let coordinates = handle.require_layout_coordinates();
        let size = coordinates.size();
        Some(Rect::from_offset_size(
            root.local_position_of(&coordinates, Offset::ZERO),
            Size::new(size.width as f32, size.height as f32),
        ))
    }

    fn calculate_alternative_target_bounds(&self, before: Rect) -> Option<Rect> {
        self.entry().calculate_target_bounds(before)
    }
}

struct SharedBoundsNode {
    shared: Rc<NodeShared>,
    locals: ModifierLocalMap,
    in_progress: PlacementInProgress,
}

impl ModifierNodeElement for SharedBoundsElement {
    type Node = SharedBoundsNode;

    fn create(&self) -> SharedBoundsNode {
        let shared = Rc::new(NodeShared {
            entry: RefCell::new(self.entry.clone()),
            handle: Cell::new(None),
            is_placed: Cell::new(false),
            bounds_before_detached: Cell::new(None),
            transform_state: RefCell::new(None),
            forced_handoff_bounds: Cell::new(None),
            forced_handoff_velocity: Cell::new(None),
            content_offset: Cell::new(IntOffset::ZERO),
        });
        SharedBoundsNode {
            shared,
            locals: compose_ui::modifier_local_map_of()
                .with(&LOCAL_ENTRY, Some(EntryArg(self.entry.clone()))),
            in_progress: Rc::new(|_, _| false),
        }
    }

    fn update(&self, node: &mut SharedBoundsNode) {
        let old = node.shared.entry();
        if !Rc::ptr_eq(&old, &self.entry) {
            old.is_attached.set(false);
            *node.shared.entry.borrow_mut() = self.entry.clone();
            let attached = node.shared.handle.get().is_some_and(|it| it.is_attached());
            self.entry.is_attached.set(attached);
            if attached {
                node.locals.provide(&LOCAL_ENTRY, Some(EntryArg(self.entry.clone())));
                if let Some(handle) = node.shared.handle.get() {
                    node.setup_from_handle(handle);
                    // The rest of Compose's `setup()` and `onObservedReadsChanged()` needs the
                    // node's context, which `update` (in the composition pass) hasn't: once the
                    // pass is applied (D1355, D2101).
                    handle.side_effect(move || {
                        handle.with_node::<SharedBoundsNode, _>(|node, context| {
                            node.setup(context);
                            node.on_observed_reads_changed(context);
                        });
                    });
                }
            }
        }
    }

    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.entry, &other.entry)
    }

    fn name(&self) -> &'static str {
        "sharedBounds"
    }
}

impl SharedBoundsNode {
    fn setup(&mut self, context: &mut NodeContext<'_>) {
        let entry = self.shared.entry();
        self.locals.provide(&LOCAL_ENTRY, Some(EntryArg(entry.clone())));
        let parent = context.modifier_local_current(&LOCAL_ENTRY);
        *entry.parent_state.borrow_mut() = parent.map(|it| Rc::downgrade(&it.0));
        *self.shared.transform_state.borrow_mut() =
            context.modifier_local_current(&LOCAL_TRANSFORM_STATE);
        entry.element().scope().renderers_changed();
        entry.set_layer(None);
        self.shared.is_placed.set(false);
        *entry.bounds_provider.borrow_mut() = Some(self.shared.clone());
    }

    fn setup_from_handle(&mut self, _handle: NodeHandle) {
        let entry = self.shared.entry();
        entry.set_layer(None);
        self.shared.is_placed.set(false);
        *entry.bounds_provider.borrow_mut() = Some(self.shared.clone());
    }

    fn make_in_progress(&self) -> PlacementInProgress {
        Rc::new(|_, _| false)
    }
}

impl ModifierNode for SharedBoundsNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn as_observer(&mut self) -> Option<&mut dyn compose_ui::ObserverModifierNode> {
        Some(self)
    }
    fn as_modifier_local(&mut self) -> Option<&mut dyn compose_ui::ModifierLocalModifierNode> {
        Some(self)
    }

    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        context.approach_attached();
        self.shared.handle.set(Some(context.handle()));
        self.in_progress = self.make_in_progress();
        self.setup(context);
        self.shared.entry().is_attached.set(true);
        self.on_observed_reads_changed(context);
    }

    fn on_detach(&mut self, _context: &mut NodeContext<'_>) {
        *self.shared.transform_state.borrow_mut() = None;
        let entry = self.shared.entry();
        let root = entry.element().scope().root();
        if root.is_some() {
            let bounds = (root.is_some_and(|it| it.is_attached()) && self.shared.is_placed.get())
                .then(|| {
                    let handle = self.shared.handle.get()?;
                    let coordinates = handle.require_layout_coordinates();
                    let root = root?;
                    let size = coordinates.size();
                    Some(Rect::from_offset_size(
                        coordinates.position_in_root() - root.position_in_root(),
                        Size::new(size.width as f32, size.height as f32),
                    ))
                })
                .flatten();
            self.shared.bounds_before_detached.set(bounds);
        }
        entry.set_layer(None);
        *entry.parent_state.borrow_mut() = None;
        *entry.bounds_provider.borrow_mut() = None;
        entry.is_attached.set(false);
        self.shared.is_placed.set(false);
    }

    fn on_reset(&mut self, _context: &mut NodeContext<'_>) {
        self.shared.bounds_before_detached.set(None);
        self.shared.entry().set_layer(None);
    }
}

impl compose_ui::ModifierLocalModifierNode for SharedBoundsNode {
    fn provided_values(&mut self) -> Option<&mut ModifierLocalMap> {
        Some(&mut self.locals)
    }
}

impl compose_ui::ObserverModifierNode for SharedBoundsNode {
    fn on_observed_reads_changed(&mut self, context: &mut NodeContext<'_>) {
        let entry = self.shared.entry();
        entry.element().update_match();
        context.observe_reads(|| entry.observe());
    }
}

impl LayoutModifierNode for SharedBoundsNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        compose_ui::measure_approach(self, scope, content, constraints)
    }
}

impl ApproachLayoutModifierNode for SharedBoundsNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let placeable = scope.measure(content, constraints);
        let shared = self.shared.clone();
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            placement.place(&placeable, 0, 0);
            if !placement.is_looking_ahead() {
                shared.content_offset.set(IntOffset::ZERO);
            }
            let entry = shared.entry();
            entry.element().on_lookahead_placed(placement, &entry);
        })
    }

    fn is_measurement_approach_in_progress(&mut self, _lookahead_size: IntSize) -> bool {
        let entry = self.shared.entry();
        let element = entry.element();
        // Compose's lookahead placement, which handles the deferred request, comes before this
        // pass's measurement; here placement ahead may come after it (D1335).
        element.machine.process_pending_request();
        entry.is_enabled() && element.found_match() && element.scope().is_transition_active.get()
    }

    fn placement_approach_in_progress(&self) -> PlacementInProgress {
        self.in_progress.clone()
    }

    fn approach_measure(
        &mut self,
        scope: &mut ApproachMeasureScope<'_, '_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        self.shared.update_deferred_handoff_values();
        let entry = self.shared.entry();
        let element = entry.element();
        let resolved = self
            .shared
            .forced_handoff_bounds
            .get()
            .or_else(|| entry.bounds_animation().value())
            .or_else(|| element.try_initializing_current_bounds())
            .map(|bounds| {
                let size = bounds.size().round_to_int_size();
                Constraints::fixed(size.width.max(0), size.height.max(0))
            })
            .unwrap_or(constraints);
        let placeable = scope.measure(content, resolved);
        // As `approachPlace`.
        let size = if element.state().match_is_or_has_been_configured() {
            entry.placeholder_size.borrow().calculate_size(scope.lookahead_size(), placeable.size())
        } else {
            placeable.size()
        };
        let shared = self.shared.clone();
        scope.layout_with(size.width, size.height, move |placement| {
            shared.is_placed.set(true);
            shared.bounds_before_detached.set(None);
            shared.content_offset.set(IntOffset::ZERO);
            let entry = shared.entry();
            let element = entry.element();
            let state = element.state();
            if !entry.is_enabled() {
                placement.place(&placeable, 0, 0);
            } else if state.match_is_or_has_been_configured() {
                let target_data = state.target_data().expect("a configured match has its target");
                let current = state.current_bounds().expect("a configured match has its bounds");
                if element.scope().is_transition_active.get() {
                    place_match_in_transition(
                        &shared,
                        placement,
                        &placeable,
                        &target_data,
                        current,
                    );
                } else {
                    place_match_beyond_transition(&shared, placement, &placeable, current);
                }
            } else {
                placement.place(&placeable, 0, 0);
            }
        })
    }
}

/// As `approachPlaceMatchBeyondTransition`.
fn place_match_beyond_transition(
    shared: &NodeShared,
    placement: &mut PlacementScope<'_>,
    placeable: &compose_ui::Placeable,
    current: Rect,
) {
    let entry = shared.entry();
    if !entry.target() {
        let at = placement
            .coordinates()
            .zip(shared.scope().root())
            .map(|(it, root)| {
                let in_scope = root.local_position_of(&it, Offset::ZERO);
                (current.top_left() - in_scope).round()
            })
            .unwrap_or(IntOffset::ZERO);
        placement.place_at(placeable, at);
        shared.content_offset.set(at);
    } else {
        placement.place(placeable, 0, 0);
    }
}

/// As `approachPlaceMatchInTransition`.
fn place_match_in_transition(
    shared: &NodeShared,
    placement: &mut PlacementScope<'_>,
    placeable: &compose_ui::Placeable,
    target_data: &TargetData,
    current: Rect,
) {
    let (Some(coordinates), Some(root)) = (placement.coordinates(), shared.scope().root()) else {
        placement.place(placeable, 0, 0);
        return;
    };
    let entry = shared.entry();
    let element = entry.element();
    let animation = entry.bounds_animation();
    let removed = !element.state().active_match_found();
    let in_scope = root.local_position_of(&coordinates, Offset::ZERO);
    let target_bounds = target_data.target_bounds();
    let forced = removed.then(|| {
        BoundsTransform::new(|_, _| {
            AnimationSpec::Spring(
                crate::spec::spring().visibility_threshold(Rect::VISIBILITY_THRESHOLD),
            )
        })
    });
    animation.animate(
        current,
        target_bounds,
        forced,
        shared.forced_handoff_bounds.take(),
        shared.forced_handoff_velocity.take(),
    );
    let animated = animation.value();
    let animated_top_left = animated.map(|it| target_data.offset_from_direct_manipulation(it));
    let top_left = if entry.target() || removed {
        let top_left = animated_top_left.unwrap_or(in_scope);
        let bounds = match (animated_top_left, animated) {
            (Some(top_left), Some(animated)) => Rect::from_offset_size(top_left, animated.size()),
            _ => {
                let size = coordinates.size();
                Rect::from_offset_size(in_scope, Size::new(size.width as f32, size.height as f32))
            }
        };
        element.state().update_bounds(bounds);
        top_left
    } else {
        animated_top_left.unwrap_or(current.top_left())
    };
    // Moved by hand with its content: placed where its content's scale and offset take it, which
    // the content's layer then maps back (Compose sets the position in the scope first, then
    // overwrites it when the content has coordinates).
    let mut final_top_left = top_left;
    if let Some(state) = entry.active_mutable_transform_state().filter(|it| it.is_mutating()) {
        if !animation.is_running() {
            final_top_left = in_scope;
        }
        if let Some(parent) = state.parent_layout_coordinates()
            && parent.is_attached()
            && root.is_attached()
        {
            let pivot = calculate_pivot(&parent, &root, state.active_transform_origin());
            final_top_left =
                transform_point(top_left, pivot, state.active_scale(), state.active_offset());
        }
    }
    let local = coordinates.local_position_of(&root, final_top_left);
    let at = IntOffset::new(local.x.round() as i32, local.y.round() as i32);
    placement.place_at(placeable, at);
    shared.content_offset.set(at);
}

impl DrawModifierNode for SharedBoundsNode {
    fn draws_content(&self) -> bool {
        true
    }

    fn draw_with_content(&mut self, scope: &mut ContentDrawScope<'_, '_>) {
        let entry = self.shared.entry();
        let bounds = entry.element().state().current_bounds();
        if !entry.should_render_in_overlay() {
            *entry.clip_path_in_overlay.borrow_mut() = None;
            entry.set_layer(None);
            if entry.should_render_in_place() {
                scope.draw_content();
            }
            return;
        }
        let clip = bounds.and_then(|bounds| {
            let state = entry.user_state.borrow().clone();
            let clip = entry.overlay_clip.borrow().clone();
            clip.get_clip_path(&state, bounds, scope.layout_direction(), scope.density())
        });
        *entry.clip_path_in_overlay.borrow_mut() = clip;
        if entry.layer.borrow().is_none() {
            entry.set_layer(Some(GraphicsLayer::new()));
        }
        let layer = entry.layer.borrow().clone().expect("a layer to render in the overlay");
        scope.record_layer(&layer, |content| content.draw_content());
        if entry.should_render_in_place() {
            scope.draw_layer(&layer);
        }
    }
}

// ---- Rendering in the overlay ------------------------------------------------------------------

/// `RenderInTransitionOverlayNodeElement`.
struct OverlayElement {
    scope: SharedTransitionScope,
    render_in_overlay: Rc<dyn Fn() -> bool>,
    z_index: f32,
}

/// What a `render_in_shared_transition_scope_overlay` node draws the overlay with.
struct OverlayRenderer {
    enabled: Cell<bool>,
    position: Cell<Offset>,
    z_index: OwnedState<f32>,
    layer: RefCell<Option<GraphicsLayer>>,
}

impl OverlayRenderer {
    fn obtain_layer(&self) -> GraphicsLayer {
        self.layer.borrow_mut().get_or_insert_with(GraphicsLayer::new).clone()
    }

    fn release_layer(&self) {
        if let Some(layer) = self.layer.borrow_mut().take() {
            layer.release();
        }
    }

    fn draw_in_overlay(&self, scope: &mut DrawScope<'_>) {
        if !self.enabled.get() {
            return;
        }
        let layer = self.obtain_layer();
        let at = self.position.get();
        scope.translate(at.x, at.y, |scope| scope.draw_layer(&layer));
    }
}

struct OverlayNode {
    scope: SharedTransitionScope,
    render_in_overlay: Rc<dyn Fn() -> bool>,
    renderer: Rc<OverlayRenderer>,
    handle: Rc<Cell<Option<NodeHandle>>>,
}

impl ModifierNodeElement for OverlayElement {
    type Node = OverlayNode;

    fn create(&self) -> OverlayNode {
        OverlayNode {
            scope: self.scope.clone(),
            render_in_overlay: self.render_in_overlay.clone(),
            renderer: Rc::new(OverlayRenderer {
                enabled: Cell::new(false),
                position: Cell::new(Offset::ZERO),
                z_index: mutable_state_of(self.z_index),
                layer: RefCell::new(None),
            }),
            handle: Rc::new(Cell::new(None)),
        }
    }

    fn update(&self, node: &mut OverlayNode) {
        node.scope = self.scope.clone();
        node.render_in_overlay = self.render_in_overlay.clone();
        if node.renderer.z_index.get() != self.z_index {
            node.renderer.z_index.set(self.z_index);
            node.scope.inner.renderers_changed();
        }
    }

    fn same(&self, other: &Self) -> bool {
        self.scope.same(&other.scope)
            && Rc::ptr_eq(&self.render_in_overlay, &other.render_in_overlay)
            && self.z_index == other.z_index
    }

    fn name(&self) -> &'static str {
        "renderInSharedTransitionOverlay"
    }
}

impl ModifierNode for OverlayNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.handle.set(Some(context.handle()));
        self.scope.inner.renderers.borrow_mut().push(Renderer::Overlay(self.renderer.clone()));
        self.scope.inner.renderers_changed();
    }
    fn on_detach(&mut self, _context: &mut NodeContext<'_>) {
        let removed = Renderer::Overlay(self.renderer.clone());
        self.scope.inner.renderers.borrow_mut().retain(|it| !it.same(&removed));
        self.scope.inner.renderers_changed();
        self.renderer.release_layer();
        self.handle.set(None);
    }
}

impl LayoutModifierNode for OverlayNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let placeable = scope.measure(content, constraints);
        let (shared, renderer, render, handle) = (
            self.scope.inner.clone(),
            self.renderer.clone(),
            self.render_in_overlay.clone(),
            self.handle.clone(),
        );
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            if !placement.is_looking_ahead() {
                let invalidate = |renderer: &OverlayRenderer| {
                    shared.invalidate_overlay();
                    if let Some(handle) = handle.get() {
                        handle.invalidate_draw();
                    }
                    let _ = renderer;
                };
                if render() {
                    if let (Some(_), Some(root), Some(node)) =
                        (placement.coordinates(), shared.root(), handle.get())
                    {
                        let own = node.require_layout_coordinates();
                        let position = root.local_position_of(&own, Offset::ZERO);
                        if !renderer.enabled.get() || renderer.position.get() != position {
                            renderer.enabled.set(true);
                            renderer.position.set(position);
                            invalidate(&renderer);
                        }
                    }
                } else if renderer.enabled.get() {
                    renderer.enabled.set(false);
                    invalidate(&renderer);
                }
            }
            placement.place(&placeable, 0, 0);
        })
    }
}

impl DrawModifierNode for OverlayNode {
    fn draws_content(&self) -> bool {
        true
    }
    fn draw_with_content(&mut self, scope: &mut ContentDrawScope<'_, '_>) {
        if self.renderer.enabled.get() {
            let layer = self.renderer.obtain_layer();
            scope.record_layer(&layer, |content| content.draw_content());
        } else {
            self.renderer.release_layer();
            scope.draw_content();
        }
    }
}

// ---- Skipping to the size ahead ----------------------------------------------------------------

/// `SkipToLookaheadSizeElement`: with a scale, `scaleToBounds`' content scale modifier.
struct SkipElement {
    scale: Option<(compose_ui::ContentScale, compose_ui::Alignment)>,
    enabled: Rc<dyn Fn() -> bool>,
}

struct SkipNode {
    scale: Option<(compose_ui::ContentScale, compose_ui::Alignment)>,
    enabled: Rc<dyn Fn() -> bool>,
    lookahead_constraints: Option<Constraints>,
    lookahead_size: Option<IntSize>,
}

/// `createContentScaleModifier`: a crop clips while enabled.
fn content_scale_modifier(
    content_scale: compose_ui::ContentScale,
    alignment: compose_ui::Alignment,
    enabled: Rc<dyn Fn() -> bool>,
) -> Modifier {
    let clip = if content_scale == compose_ui::ContentScale::Crop {
        let enabled = enabled.clone();
        Modifier.graphics_layer(move |layer| layer.clip = enabled())
    } else {
        Modifier
    };
    clip.then_custom(SkipElement { scale: Some((content_scale, alignment)), enabled })
}

impl ModifierNodeElement for SkipElement {
    type Node = SkipNode;
    fn create(&self) -> SkipNode {
        SkipNode {
            scale: self.scale,
            enabled: self.enabled.clone(),
            lookahead_constraints: None,
            lookahead_size: None,
        }
    }
    fn update(&self, node: &mut SkipNode) {
        node.scale = self.scale;
        node.enabled = self.enabled.clone();
    }
    fn same(&self, other: &Self) -> bool {
        self.scale == other.scale && Rc::ptr_eq(&self.enabled, &other.enabled)
    }
    fn name(&self) -> &'static str {
        "skipToLookahead"
    }
}

impl ModifierNode for SkipNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
}

impl LayoutModifierNode for SkipNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let ahead = scope.is_looking_ahead();
        if ahead {
            self.lookahead_constraints = Some(constraints);
        }
        if !(self.enabled)() {
            let placeable = scope.measure(content, constraints);
            return scope.layout_with(placeable.width, placeable.height, move |placement| {
                placement.place(&placeable, 0, 0);
            });
        }
        let placeable = if ahead {
            let placeable = scope.measure(content, constraints);
            self.lookahead_size = Some(placeable.size());
            placeable
        } else {
            scope.measure(content, self.lookahead_constraints.unwrap_or(constraints))
        };
        let lookahead_size = self.lookahead_size.unwrap_or(placeable.size());
        let size = constraints.constrain(lookahead_size);
        let scale = self.scale;
        let direction = scope.layout_direction();
        scope.layout_with(size.width, size.height, move |placement| {
            let Some((content_scale, alignment)) = scale else {
                placement.place(&placeable, 0, 0);
                return;
            };
            let (scale_x, scale_y) = if lookahead_size.width == 0 || lookahead_size.height == 0 {
                (1.0, 1.0)
            } else {
                content_scale.scale_factor(
                    Size::new(lookahead_size.width as f32, lookahead_size.height as f32),
                    Size::new(size.width as f32, size.height as f32),
                )
            };
            let scaled = IntSize::new(
                (lookahead_size.width as f32 * scale_x).round() as i32,
                (lookahead_size.height as f32 * scale_y).round() as i32,
            );
            let at = alignment.align(scaled, size, direction);
            placement.place_with_layer(
                &placeable,
                at.x,
                at.y,
                LayerBlock::new(move |layer| {
                    layer.scale_x = scale_x;
                    layer.scale_y = scale_y;
                    layer.transform_origin =
                        TransformOrigin { pivot_fraction_x: 0.0, pivot_fraction_y: 0.0 };
                }),
            );
        })
    }

    fn max_intrinsic_width(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        height: i32,
    ) -> i32 {
        match self.lookahead_size.filter(|_| !scope.is_looking_ahead()) {
            Some(size) => size.width,
            None => scope.max_intrinsic_width(content, height),
        }
    }

    fn min_intrinsic_width(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        height: i32,
    ) -> i32 {
        match self.lookahead_size.filter(|_| !scope.is_looking_ahead()) {
            Some(size) => size.width,
            None => scope.min_intrinsic_width(content, height),
        }
    }

    fn max_intrinsic_height(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        width: i32,
    ) -> i32 {
        match self.lookahead_size.filter(|_| !scope.is_looking_ahead()) {
            Some(size) => size.height,
            None => scope.max_intrinsic_height(content, width),
        }
    }

    fn min_intrinsic_height(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        width: i32,
    ) -> i32 {
        match self.lookahead_size.filter(|_| !scope.is_looking_ahead()) {
            Some(size) => size.height,
            None => scope.min_intrinsic_height(content, width),
        }
    }
}
