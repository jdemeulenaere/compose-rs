//! The deferred phase of a transition's enter and exit, as Compose's
//! `DeferredEnterExitTransition.kt`: while a [`DeferredTransition`](crate::DeferredTransition)
//! waits on a pending target, its content can be moved by hand (a predictive back gesture), and
//! when it ends the animation takes up from there, at the velocity the content had (D828).

use std::cell::{Cell, OnceCell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::Poll;

use compose_runtime::{ComposeArg, OwnedState, mutable_state_of};
use compose_ui::{Color, IntOffset, IntSize, Offset, TransformOrigin};

use crate::animatable::Animatable;
use crate::vector::AnimationVector;
use crate::visibility::{
    EnterTransition, ExitTransition, default_alpha_spec, default_offset_spec, default_scale_spec,
    default_veil_spec, fade_in_with, fade_out_with, scale_in_with, scale_out_with, slide_in_with,
    slide_out_with, unveil_in_with, veil_out_with,
};

type TransformBlock = Rc<dyn Fn(&TransformScope, IntSize)>;
type VelocityProvider = Rc<dyn Fn() -> Offset>;

/// What moves a content by hand during a deferred transition's deferred phase, as Compose's
/// `MutableTransform`: its block runs again whenever what it reads changes, and what it sets is
/// handed to the animation when the phase ends. A value (clones share it), compared by identity.
#[derive(Clone)]
pub struct MutableTransform {
    inner: Rc<MutableTransformInner>,
}

struct MutableTransformInner {
    veil_match_parent_size: Cell<bool>,
    offset_velocity_provider: RefCell<Option<VelocityProvider>>,
    block: RefCell<Option<TransformBlock>>,
}

impl MutableTransform {
    /// One with `block`, as `MutableTransform { … }`.
    pub fn new(block: impl Fn(&TransformScope, IntSize) + 'static) -> MutableTransform {
        let transform = MutableTransform::empty();
        transform.update(block);
        transform
    }

    /// One moving nothing yet, as `MutableTransform()`.
    pub fn empty() -> MutableTransform {
        MutableTransform {
            inner: Rc::new(MutableTransformInner {
                veil_match_parent_size: Cell::new(false),
                offset_velocity_provider: RefCell::new(None),
                block: RefCell::new(None),
            }),
        }
    }

    /// Whether the veil covers the parent, as `veilMatchParentSize`.
    pub fn veil_match_parent_size(self, veil_match_parent_size: bool) -> MutableTransform {
        self.inner.veil_match_parent_size.set(veil_match_parent_size);
        self
    }

    /// The velocity of the offset in pixels per second, read once as the phase ends, as
    /// `offsetVelocityProvider`. Without one, it's computed from the offsets set.
    pub fn offset_velocity_provider(
        self,
        provider: impl Fn() -> Offset + 'static,
    ) -> MutableTransform {
        *self.inner.offset_velocity_provider.borrow_mut() = Some(Rc::new(provider));
        self
    }

    /// What it applies, as `update`.
    pub fn update(&self, block: impl Fn(&TransformScope, IntSize) + 'static) {
        *self.inner.block.borrow_mut() = Some(Rc::new(block));
    }

    fn block(&self) -> Option<TransformBlock> {
        self.inner.block.borrow().clone()
    }

    fn has_block(&self) -> bool {
        self.inner.block.borrow().is_some()
    }

    fn matches_parent_size(&self) -> bool {
        self.inner.veil_match_parent_size.get()
    }
}

impl ComposeArg for MutableTransform {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

/// What moves both contents by hand during an `AnimatedContent`'s deferred phase, as Compose's
/// `MutableContentTransform`.
#[derive(Clone)]
pub struct MutableContentTransform {
    initial_transform: MutableTransform,
    target_transform: MutableTransform,
}

impl Default for MutableContentTransform {
    fn default() -> Self {
        MutableContentTransform::new()
    }
}

impl MutableContentTransform {
    /// One moving nothing yet, as `MutableContentTransform()`.
    pub fn new() -> MutableContentTransform {
        MutableContentTransform::of(MutableTransform::empty(), MutableTransform::empty())
    }

    /// One of the exiting and the entering content's transforms, as
    /// `MutableContentTransform(initialTransform, targetTransform)`.
    pub fn of(
        initial_transform: MutableTransform,
        target_transform: MutableTransform,
    ) -> MutableContentTransform {
        MutableContentTransform { initial_transform, target_transform }
    }

    /// What moves the exiting content, as `initialContentTransform`.
    pub fn initial_content_transform(
        self,
        block: impl Fn(&TransformScope, IntSize) + 'static,
    ) -> MutableContentTransform {
        self.initial_transform.update(block);
        self
    }

    /// What moves the entering content, as `targetContentTransform`.
    pub fn target_content_transform(
        self,
        block: impl Fn(&TransformScope, IntSize) + 'static,
    ) -> MutableContentTransform {
        self.target_transform.update(block);
        self
    }

    /// The exiting content's transform (`initialVeilMatchParentSize`,
    /// `initialOffsetVelocityProvider`: its `veil_match_parent_size` and
    /// `offset_velocity_provider`).
    pub fn initial_transform(&self) -> &MutableTransform {
        &self.initial_transform
    }

    /// The entering content's.
    pub fn target_transform(&self) -> &MutableTransform {
        &self.target_transform
    }
}

impl ComposeArg for MutableContentTransform {
    fn same(&self, other: &Self) -> bool {
        self.initial_transform.same(&other.initial_transform)
            && self.target_transform.same(&other.target_transform)
    }
}

/// What a [`MutableTransform`]'s block sets, as Compose's `TransformScope`: what it doesn't set
/// stays the transition's.
pub struct TransformScope {
    alpha: OwnedState<f32>,
    is_alpha_mutated: OwnedState<bool>,
    scale: OwnedState<f32>,
    is_scale_mutated: OwnedState<bool>,
    // Set and read while placing: plain, as Compose's.
    offset: Cell<IntOffset>,
    is_offset_mutated: Cell<bool>,
    transform_origin: OwnedState<TransformOrigin>,
    is_transform_origin_mutated: OwnedState<bool>,
    veil: OwnedState<Color>,
    is_veil_mutated: OwnedState<bool>,
}

impl TransformScope {
    fn new() -> TransformScope {
        TransformScope {
            alpha: mutable_state_of(1.0),
            is_alpha_mutated: mutable_state_of(false),
            scale: mutable_state_of(1.0),
            is_scale_mutated: mutable_state_of(false),
            offset: Cell::new(IntOffset::ZERO),
            is_offset_mutated: Cell::new(false),
            transform_origin: mutable_state_of(TransformOrigin::CENTER),
            is_transform_origin_mutated: mutable_state_of(false),
            veil: mutable_state_of(Color::TRANSPARENT),
            is_veil_mutated: mutable_state_of(false),
        }
    }

    /// The alpha, as `alpha`.
    pub fn alpha(&self) -> f32 {
        self.alpha.get()
    }

    pub fn set_alpha(&self, alpha: f32) {
        self.alpha.set(alpha);
        self.is_alpha_mutated.set(true);
    }

    /// The scale, as `scale`.
    pub fn scale(&self) -> f32 {
        self.scale.get()
    }

    pub fn set_scale(&self, scale: f32) {
        self.scale.set(scale);
        self.is_scale_mutated.set(true);
    }

    /// The pivot of the scale, as `transformOrigin`.
    pub fn transform_origin(&self) -> TransformOrigin {
        self.transform_origin.get()
    }

    pub fn set_transform_origin(&self, transform_origin: TransformOrigin) {
        self.transform_origin.set(transform_origin);
        self.is_transform_origin_mutated.set(true);
    }

    /// The offset, as `offset`.
    pub fn offset(&self) -> IntOffset {
        self.offset.get()
    }

    pub fn set_offset(&self, offset: IntOffset) {
        self.offset.set(offset);
        self.is_offset_mutated.set(true);
    }

    /// The color over the content, as `veil`.
    pub fn veil(&self) -> Color {
        self.veil.get()
    }

    pub fn set_veil(&self, veil: Color) {
        self.veil.set(veil);
        self.is_veil_mutated.set(true);
    }

    fn reset(&self) {
        self.alpha.set(1.0);
        self.is_alpha_mutated.set(false);
        self.scale.set(1.0);
        self.is_scale_mutated.set(false);
        self.offset.set(IntOffset::ZERO);
        self.is_offset_mutated.set(false);
        self.transform_origin.set(TransformOrigin::CENTER);
        self.is_transform_origin_mutated.set(false);
        self.veil.set(Color::TRANSPARENT);
        self.is_veil_mutated.set(false);
    }
}

/// The phases of a deferred enter or exit, as Compose's `MutationPhase`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum MutationPhase {
    /// No manual change nor hand-off.
    Idle,
    /// A deferred phase interrupted a running transition: the catch-up starts next.
    MutatingPendingCatchUp,
    /// A spring catches the values up with the manual ones, on which changes add on.
    MutatingCatchingUp,
    /// The manual values apply.
    Mutating,
    /// The phase ended: the animation goes on from the last values until it settles.
    Handoff,
}

/// The velocity of a manual value, as Compose's `VelocityTracker` fed by `trackScaleVelocity` and
/// `trackSlideVelocity`. compose-rs's layer block can run several times a frame (a coordinates'
/// mapping evaluates it too), so a sample like the last isn't added again: copies would fill the
/// tracker's history of 20 and lose the gesture (D833).
#[derive(Default)]
struct ManualVelocity {
    tracker: compose_ui::VelocityTracker,
    last: Option<(i64, Offset)>,
}

impl ManualVelocity {
    fn add(&mut self, position: Offset) {
        let millis = (compose_runtime::platform::now_nanos() / 1_000_000) as i64;
        if self.last != Some((millis, position)) {
            self.last = Some((millis, position));
            self.tracker.add_position(millis, position);
        }
    }

    fn velocity(&self) -> compose_ui::Velocity {
        self.tracker.calculate_velocity()
    }

    fn reset(&mut self) {
        self.tracker.reset_tracking();
        self.last = None;
    }
}

/// A deferred enter or exit's manual state, as Compose's `SharedMutableTransformState` (shared with
/// shared elements once they exist). Its `combined_*` are what's shown: the manual values over the
/// transition's. A deferred transition's content has one; other transitions, none.
pub(crate) struct SharedMutableTransformState {
    pub(crate) mutation_phase: OwnedState<MutationPhase>,
    last_mutable_data: RefCell<Option<MutableTransform>>,
    mutable_data: RefCell<Option<MutableTransform>>,
    pub(crate) transform_scope: TransformScope,

    last_veil: Cell<Color>,
    last_alpha: Cell<f32>,
    last_scale: Cell<f32>,
    last_transform_origin: Cell<TransformOrigin>,
    last_slide: Rc<Cell<IntOffset>>,
    /// `slideHandoffOffset`: one function, so the slides made of it compare equal.
    slide_handoff_offset: Rc<dyn Fn(IntSize) -> IntOffset>,

    active_transition_alpha: Cell<f32>,
    active_transition_scale: Cell<f32>,
    active_transition_slide: Cell<IntOffset>,
    active_transition_veil: Cell<Color>,
    active_transition_transform_origin: Cell<TransformOrigin>,

    initial_manual_alpha: Cell<f32>,
    initial_manual_scale: Cell<f32>,
    initial_manual_slide: Cell<IntOffset>,

    catch_up: OnceCell<CatchUp>,

    scale_velocity_tracker: RefCell<ManualVelocity>,
    offset_velocity_tracker: RefCell<ManualVelocity>,
    scale_handoff_velocity: Cell<Option<AnimationVector>>,
    slide_handoff_velocity: Cell<Option<AnimationVector>>,
}

/// The catch-up animations, made when first used, as Compose's `by lazy`.
struct CatchUp {
    alpha: Animatable<f32>,
    scale: Animatable<f32>,
    slide: Animatable<IntOffset>,
    veil: Animatable<Color>,
    transform_origin: Animatable<TransformOrigin>,
}

/// Runs `futures` together until they all end, as `launch`es in a `coroutineScope`.
async fn join_all(mut futures: Vec<Pin<Box<dyn Future<Output = ()> + '_>>>) {
    std::future::poll_fn(|context| {
        futures.retain_mut(|future| future.as_mut().poll(context).is_pending());
        if futures.is_empty() { Poll::Ready(()) } else { Poll::Pending }
    })
    .await
}

impl SharedMutableTransformState {
    pub(crate) fn new() -> SharedMutableTransformState {
        let last_slide = Rc::new(Cell::new(IntOffset::ZERO));
        let read = last_slide.clone();
        SharedMutableTransformState {
            mutation_phase: mutable_state_of(MutationPhase::Idle),
            last_mutable_data: RefCell::new(None),
            mutable_data: RefCell::new(None),
            transform_scope: TransformScope::new(),
            last_veil: Cell::new(Color::TRANSPARENT),
            last_alpha: Cell::new(1.0),
            last_scale: Cell::new(1.0),
            last_transform_origin: Cell::new(TransformOrigin::CENTER),
            last_slide,
            slide_handoff_offset: Rc::new(move |_| read.get()),
            active_transition_alpha: Cell::new(1.0),
            active_transition_scale: Cell::new(1.0),
            active_transition_slide: Cell::new(IntOffset::ZERO),
            active_transition_veil: Cell::new(Color::TRANSPARENT),
            active_transition_transform_origin: Cell::new(TransformOrigin::CENTER),
            initial_manual_alpha: Cell::new(1.0),
            initial_manual_scale: Cell::new(1.0),
            initial_manual_slide: Cell::new(IntOffset::ZERO),
            catch_up: OnceCell::new(),
            scale_velocity_tracker: RefCell::default(),
            offset_velocity_tracker: RefCell::default(),
            scale_handoff_velocity: Cell::new(None),
            slide_handoff_velocity: Cell::new(None),
        }
    }

    pub(crate) fn is_mutating(&self) -> bool {
        matches!(
            self.mutation_phase.get(),
            MutationPhase::MutatingPendingCatchUp
                | MutationPhase::MutatingCatchingUp
                | MutationPhase::Mutating
        )
    }

    fn is_handoff_active(&self) -> bool {
        self.mutation_phase.get() == MutationPhase::Handoff
    }

    /// The new phase for what the transition says, as `updateMutationState`.
    pub(crate) fn update_mutation_state(&self, is_mutating: bool, is_settled: bool) {
        let phase = self.mutation_phase.get();
        if is_mutating {
            if matches!(phase, MutationPhase::Idle | MutationPhase::Handoff) {
                self.transform_scope.reset();
                self.mutation_phase.set(if is_settled {
                    MutationPhase::Mutating
                } else {
                    MutationPhase::MutatingPendingCatchUp
                });
                self.scale_velocity_tracker.borrow_mut().reset();
                self.offset_velocity_tracker.borrow_mut().reset();
                self.scale_handoff_velocity.set(None);
                self.slide_handoff_velocity.set(None);
            }
        } else if matches!(
            phase,
            MutationPhase::MutatingPendingCatchUp
                | MutationPhase::MutatingCatchingUp
                | MutationPhase::Mutating
        ) {
            self.mutation_phase.set(MutationPhase::Handoff);
            self.calculate_handoff_velocities();
        }
    }

    pub(crate) fn set_mutable_data(&self, data: Option<MutableTransform>) {
        if data.is_some() {
            self.last_mutable_data.borrow_mut().clone_from(&data);
        }
        *self.mutable_data.borrow_mut() = data;
    }

    pub(crate) fn has_mutable_data(&self) -> bool {
        self.mutable_data.borrow().is_some()
    }

    /// Whether what's set now asks the veil to cover the parent, if a transform is set.
    pub(crate) fn veil_match_parent_size(&self) -> Option<bool> {
        self.mutable_data.borrow().as_ref().map(MutableTransform::matches_parent_size)
    }

    fn has_block(&self) -> bool {
        self.mutable_data.borrow().as_ref().is_some_and(MutableTransform::has_block)
    }

    fn catch_up(&self) -> &CatchUp {
        self.catch_up.get_or_init(|| CatchUp {
            alpha: Animatable::new(1.0),
            scale: Animatable::new(1.0),
            slide: Animatable::new(IntOffset::ZERO),
            veil: Animatable::new(Color::TRANSPARENT),
            transform_origin: Animatable::new(TransformOrigin::CENTER),
        })
    }

    /// Catches the values up with the manual ones, from where the transition has them, as
    /// `startCatchUp`.
    pub(crate) async fn start_catch_up(&self) {
        let scope = &self.transform_scope;
        let catch_up = self.catch_up();
        self.initial_manual_alpha.set(if scope.is_alpha_mutated.get() {
            scope.alpha()
        } else {
            1.0
        });
        self.initial_manual_scale.set(if scope.is_scale_mutated.get() {
            scope.scale()
        } else {
            1.0
        });
        self.initial_manual_slide.set(if scope.is_offset_mutated.get() {
            scope.offset()
        } else {
            IntOffset::ZERO
        });
        let manual_veil =
            if scope.is_veil_mutated.get() { scope.veil() } else { Color::TRANSPARENT };
        let manual_origin = if scope.is_transform_origin_mutated.get() {
            scope.transform_origin()
        } else {
            TransformOrigin::CENTER
        };
        catch_up.alpha.snap_to(self.active_transition_alpha.get());
        catch_up.scale.snap_to(self.active_transition_scale.get());
        catch_up.slide.snap_to(self.active_transition_slide.get());
        catch_up.veil.snap_to(self.active_transition_veil.get());
        catch_up.transform_origin.snap_to(self.active_transition_transform_origin.get());
        if self.mutation_phase.get() != MutationPhase::MutatingPendingCatchUp {
            return;
        }
        self.mutation_phase.set(MutationPhase::MutatingCatchingUp);
        let mut animations: Vec<Pin<Box<dyn Future<Output = ()> + '_>>> = Vec::new();
        if scope.is_alpha_mutated.get() {
            let target = self.initial_manual_alpha.get();
            animations.push(Box::pin(async move {
                let spec = catch_up.alpha.default_spring_spec();
                catch_up.alpha.animate_to(target, spec).await;
            }));
        }
        if scope.is_scale_mutated.get() {
            let target = self.initial_manual_scale.get();
            animations.push(Box::pin(async move {
                let spec = catch_up.scale.default_spring_spec();
                catch_up.scale.animate_to(target, spec).await;
            }));
        }
        if scope.is_offset_mutated.get() {
            let target = self.initial_manual_slide.get();
            animations.push(Box::pin(async move {
                let spec = catch_up.slide.default_spring_spec();
                catch_up.slide.animate_to(target, spec).await;
            }));
        }
        if scope.is_veil_mutated.get() {
            animations.push(Box::pin(async move {
                let spec = catch_up.veil.default_spring_spec();
                catch_up.veil.animate_to(manual_veil, spec).await;
            }));
        }
        if scope.is_transform_origin_mutated.get() {
            animations.push(Box::pin(async move {
                let spec = catch_up.transform_origin.default_spring_spec();
                catch_up.transform_origin.animate_to(manual_origin, spec).await;
            }));
        }
        join_all(animations).await;
        if self.mutation_phase.get() == MutationPhase::MutatingCatchingUp {
            self.mutation_phase.set(MutationPhase::Mutating);
        }
    }

    pub(crate) fn veil_requires_animation(&self) -> bool {
        (self.has_block() && self.transform_scope.is_veil_mutated.get())
            || self.last_veil.get() != Color::TRANSPARENT
    }

    pub(crate) fn alpha_requires_animation(&self) -> bool {
        (self.has_block() && self.transform_scope.is_alpha_mutated.get())
            || self.last_alpha.get() != 1.0
    }

    pub(crate) fn scale_requires_animation(&self) -> bool {
        (self.has_block() && self.transform_scope.is_scale_mutated.get())
            || self.last_scale.get() != 1.0
    }

    pub(crate) fn slide_requires_animation(&self) -> bool {
        (self.has_block() && self.transform_scope.is_offset_mutated.get())
            || self.last_slide.get() != IntOffset::ZERO
    }

    pub(crate) fn veil_handoff_value(&self) -> Option<Color> {
        self.is_handoff_active().then(|| self.last_veil.get())
    }

    pub(crate) fn alpha_handoff_value(&self) -> Option<f32> {
        self.is_handoff_active().then(|| self.last_alpha.get())
    }

    pub(crate) fn scale_handoff_value(&self) -> Option<f32> {
        self.is_handoff_active().then(|| self.last_scale.get())
    }

    pub(crate) fn transform_origin_handoff_value(&self) -> Option<TransformOrigin> {
        self.is_handoff_active().then(|| self.last_transform_origin.get())
    }

    pub(crate) fn slide_handoff_value(&self) -> Option<IntOffset> {
        self.is_handoff_active().then(|| self.last_slide.get())
    }

    pub(crate) fn scale_handoff_velocity(&self) -> Option<AnimationVector> {
        self.scale_handoff_velocity.get()
    }

    pub(crate) fn slide_handoff_velocity(&self) -> Option<AnimationVector> {
        self.slide_handoff_velocity.get()
    }

    fn calculate_handoff_velocities(&self) {
        let not_nan = |velocity: f32| if velocity.is_nan() { 0.0 } else { velocity };
        let scale = not_nan(self.scale_velocity_tracker.borrow().velocity().x);
        self.scale_handoff_velocity.set(Some(AnimationVector::from([scale])));
        let last = self.last_mutable_data.borrow();
        let provided = last
            .as_ref()
            .and_then(|data| data.inner.offset_velocity_provider.borrow().clone())
            .map(|provider| provider());
        let slide = match provided {
            Some(velocity) if velocity.is_specified() => [velocity.x, velocity.y],
            _ => {
                let velocity = self.offset_velocity_tracker.borrow().velocity();
                [not_nan(velocity.x), not_nan(velocity.y)]
            }
        };
        self.slide_handoff_velocity.set(Some(AnimationVector::from(slide)));
    }

    fn evaluate_transform_block(&self, full_size: IntSize) {
        if self.is_mutating() {
            let block = self.mutable_data.borrow().as_ref().and_then(MutableTransform::block);
            if let Some(block) = block {
                block(&self.transform_scope, full_size);
            }
        }
    }

    pub(crate) fn combined_alpha(&self, transition_value: f32) -> f32 {
        self.active_transition_alpha.set(transition_value);
        let phase = self.mutation_phase.get();
        let is_mutating = self.is_mutating();
        let is_mutated = is_mutating && self.transform_scope.is_alpha_mutated.get();
        let combined = if is_mutated
            && phase == MutationPhase::MutatingCatchingUp
            && self.catch_up().alpha.is_running()
        {
            self.catch_up().alpha.value()
                + (self.transform_scope.alpha() - self.initial_manual_alpha.get())
        } else if is_mutated
            && matches!(phase, MutationPhase::MutatingCatchingUp | MutationPhase::Mutating)
        {
            self.transform_scope.alpha()
        } else {
            transition_value
        };
        if is_mutating {
            self.last_alpha.set(combined);
        }
        combined
    }

    pub(crate) fn combined_scale(&self, transition_value: f32) -> f32 {
        self.active_transition_scale.set(transition_value);
        let phase = self.mutation_phase.get();
        let is_mutating = self.is_mutating();
        let is_mutated = is_mutating && self.transform_scope.is_scale_mutated.get();
        let combined = if is_mutated
            && phase == MutationPhase::MutatingCatchingUp
            && self.catch_up().scale.is_running()
        {
            self.catch_up().scale.value()
                + (self.transform_scope.scale() - self.initial_manual_scale.get())
        } else if is_mutated
            && matches!(phase, MutationPhase::MutatingCatchingUp | MutationPhase::Mutating)
        {
            self.transform_scope.scale()
        } else {
            transition_value
        };
        if is_mutating {
            self.last_scale.set(combined);
            if is_mutated {
                self.scale_velocity_tracker.borrow_mut().add(Offset::new(combined, 0.0));
            }
        }
        combined
    }

    pub(crate) fn combined_transform_origin(
        &self,
        transition_value: TransformOrigin,
    ) -> TransformOrigin {
        self.active_transition_transform_origin.set(transition_value);
        let phase = self.mutation_phase.get();
        let is_mutating = self.is_mutating();
        let is_mutated = is_mutating && self.transform_scope.is_transform_origin_mutated.get();
        let combined = if is_mutated
            && phase == MutationPhase::MutatingCatchingUp
            && self.catch_up().transform_origin.is_running()
        {
            self.catch_up().transform_origin.value()
        } else if is_mutated
            && matches!(phase, MutationPhase::MutatingCatchingUp | MutationPhase::Mutating)
        {
            self.transform_scope.transform_origin()
        } else {
            transition_value
        };
        if is_mutating {
            self.last_transform_origin.set(combined);
        }
        combined
    }

    /// Runs the transform's block first: called while placing, as Compose's.
    pub(crate) fn combined_slide(
        &self,
        transition_value: IntOffset,
        full_size: IntSize,
    ) -> IntOffset {
        self.evaluate_transform_block(full_size);
        self.active_transition_slide.set(transition_value);
        let phase = self.mutation_phase.get();
        let is_mutating = self.is_mutating();
        let is_mutated = is_mutating && self.transform_scope.is_offset_mutated.get();
        let combined = if is_mutated
            && phase == MutationPhase::MutatingCatchingUp
            && self.catch_up().slide.is_running()
        {
            self.catch_up().slide.value()
                + (self.transform_scope.offset() - self.initial_manual_slide.get())
        } else if is_mutated
            && matches!(phase, MutationPhase::MutatingCatchingUp | MutationPhase::Mutating)
        {
            self.transform_scope.offset()
        } else {
            transition_value
        };
        if is_mutating {
            self.last_slide.set(combined);
            if is_mutated {
                let position = Offset::new(combined.x as f32, combined.y as f32);
                self.offset_velocity_tracker.borrow_mut().add(position);
            }
        }
        combined
    }

    pub(crate) fn combined_veil(&self, transition_value: Color) -> Color {
        self.active_transition_veil.set(transition_value);
        let phase = self.mutation_phase.get();
        let is_mutated = self.is_mutating() && self.transform_scope.is_veil_mutated.get();
        let combined = if is_mutated
            && phase == MutationPhase::MutatingCatchingUp
            && self.catch_up().veil.is_running()
        {
            self.catch_up().veil.value()
        } else if is_mutated
            && matches!(phase, MutationPhase::MutatingCatchingUp | MutationPhase::Mutating)
        {
            self.transform_scope.veil()
        } else {
            transition_value
        };
        if self.is_mutating() {
            self.last_veil.set(combined);
        }
        combined
    }

    pub(crate) fn clear(&self) {
        self.mutation_phase.set(MutationPhase::Idle);
        self.transform_scope.reset();
        self.last_veil.set(Color::TRANSPARENT);
        self.last_alpha.set(1.0);
        self.last_scale.set(1.0);
        self.scale_velocity_tracker.borrow_mut().reset();
        self.last_transform_origin.set(TransformOrigin::CENTER);
        self.last_slide.set(IntOffset::ZERO);
        self.offset_velocity_tracker.borrow_mut().reset();
        self.scale_handoff_velocity.set(None);
        self.slide_handoff_velocity.set(None);
        *self.last_mutable_data.borrow_mut() = None;
        *self.mutable_data.borrow_mut() = None;
    }

    fn last_block_active(&self) -> bool {
        self.is_handoff_active()
            && self.last_mutable_data.borrow().as_ref().is_some_and(MutableTransform::has_block)
    }

    fn last_veil_matches_parent(&self) -> bool {
        self.last_mutable_data.borrow().as_ref().is_some_and(MutableTransform::matches_parent_size)
    }

    /// The exit keeping what the deferred phase changed where it left it, as `getHandoffExit`.
    pub(crate) fn handoff_exit(&self) -> ExitTransition {
        let mut exit = ExitTransition::NONE;
        if self.last_block_active() {
            let scope = &self.transform_scope;
            if scope.is_scale_mutated.get() {
                exit = exit
                    + scale_out_with(
                        default_scale_spec(),
                        self.last_scale.get(),
                        TransformOrigin::CENTER,
                    );
            }
            if scope.is_alpha_mutated.get() {
                exit = exit + fade_out_with(default_alpha_spec(), self.last_alpha.get());
            }
            if scope.is_offset_mutated.get() {
                exit = exit + {
                    let offset = self.slide_handoff_offset.clone();
                    slide_out_with(default_offset_spec(), move |size| offset(size))
                };
            }
            if scope.is_veil_mutated.get() {
                exit = exit
                    + veil_out_with(
                        default_veil_spec(),
                        self.last_veil.get(),
                        self.last_veil_matches_parent(),
                    );
            }
        }
        exit
    }

    /// The enter starting from where the deferred phase left things, as `getHandoffEnter`.
    pub(crate) fn handoff_enter(&self) -> EnterTransition {
        let mut enter = EnterTransition::NONE;
        if self.last_block_active() {
            let scope = &self.transform_scope;
            if scope.is_scale_mutated.get() {
                enter = enter
                    + scale_in_with(
                        default_scale_spec(),
                        self.last_scale.get(),
                        TransformOrigin::CENTER,
                    );
            }
            if scope.is_alpha_mutated.get() {
                enter = enter + fade_in_with(default_alpha_spec(), self.last_alpha.get());
            }
            if scope.is_offset_mutated.get() {
                enter = enter + {
                    let offset = self.slide_handoff_offset.clone();
                    slide_in_with(default_offset_spec(), move |size| offset(size))
                };
            }
            if scope.is_veil_mutated.get() {
                enter = enter
                    + unveil_in_with(
                        default_veil_spec(),
                        self.last_veil.get(),
                        self.last_veil_matches_parent(),
                    );
            }
        }
        enter
    }
}

/// Compose's `SharedMutableTransformStateTest`, of the internal state machine.
#[cfg(test)]
mod tests {
    use super::*;

    // Compose: compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/SharedMutableTransformStateTest.kt, updateMutationState_fromHandoffToMutating_resetsTransformScope
    #[test]
    fn a_new_deferred_phase_during_a_hand_off_starts_clean() {
        let state = SharedMutableTransformState::new();
        state.update_mutation_state(true, false);
        state.transform_scope.set_offset(IntOffset::new(-150, 0));
        assert!(state.transform_scope.is_offset_mutated.get());
        state.update_mutation_state(false, false);
        assert_eq!(state.mutation_phase.get(), MutationPhase::Handoff);
        assert!(state.transform_scope.is_offset_mutated.get());
        state.update_mutation_state(true, false);
        assert!(!state.transform_scope.is_offset_mutated.get());
        assert_eq!(state.transform_scope.offset(), IntOffset::ZERO);
    }

    // Compose: compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/SharedMutableTransformStateTest.kt, updateMutationState_fromIdleToMutating_resetsTransformScope
    #[test]
    fn a_deferred_phase_from_idle_starts_clean() {
        let state = SharedMutableTransformState::new();
        state.transform_scope.set_offset(IntOffset::new(100, 200));
        assert!(state.transform_scope.is_offset_mutated.get());
        assert_eq!(state.mutation_phase.get(), MutationPhase::Idle);
        state.update_mutation_state(true, false);
        assert!(!state.transform_scope.is_offset_mutated.get());
        assert_eq!(state.transform_scope.offset(), IntOffset::ZERO);
    }

    // Compose: compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/SharedMutableTransformStateTest.kt, startCatchUp_transitionsPhaseToMutatingWhenComplete
    #[test]
    fn the_catch_up_ends_in_the_mutating_phase() {
        let state = SharedMutableTransformState::new();
        state.transform_scope.set_offset(IntOffset::new(100, 0));
        state.update_mutation_state(true, false);
        assert_eq!(state.mutation_phase.get(), MutationPhase::MutatingPendingCatchUp);
        // Nothing to animate (the phase's start reset the scope): it ends without a frame.
        let mut catch_up = std::pin::pin!(state.start_catch_up());
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(catch_up.as_mut().poll(&mut context).is_ready());
        assert_eq!(state.mutation_phase.get(), MutationPhase::Mutating);
    }

    // Compose: compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/SharedMutableTransformStateTest.kt, mutableContentTransform_fromTwoMutableTransforms_preservesInstances
    #[test]
    fn a_content_transform_keeps_the_transforms_it_is_given() {
        let initial = MutableTransform::new(|scope, _| scope.set_alpha(0.5));
        let target =
            MutableTransform::new(|scope, _| scope.set_scale(0.8)).veil_match_parent_size(true);
        let content = MutableContentTransform::of(initial.clone(), target.clone());
        assert!(content.initial_transform().same(&initial));
        assert!(content.target_transform().same(&target));
    }

    // Compose: compose/animation/animation/src/commonMain/kotlin/androidx/compose/animation/DeferredEnterExitTransition.kt, calculateHandoffVelocities (no test of Compose's: its layer block runs once a frame, this one several times)
    #[test]
    fn the_hand_off_velocity_is_compose_s_velocity_tracker_s_of_one_sample_a_frame() {
        compose_runtime::platform::set_manual_clock(1_000_000_000);
        let state = SharedMutableTransformState::new();
        state.update_mutation_state(true, true);
        // A curved gesture, as a finger slowing down, read several times a frame.
        let values = [1.0f32, 0.8, 0.65, 0.55, 0.5];
        let mut expected = compose_ui::VelocityTracker::new();
        for value in values {
            compose_runtime::platform::advance_clock(std::time::Duration::from_millis(16));
            state.transform_scope.set_scale(value);
            for _ in 0..7 {
                state.combined_scale(1.0);
            }
            let millis = (compose_runtime::platform::now_nanos() / 1_000_000) as i64;
            expected.add_position(millis, Offset::new(value, 0.0));
        }
        state.update_mutation_state(false, false);
        let expected = expected.calculate_velocity().x;
        assert!(expected < -1.0, "it's moving: {expected}");
        assert_eq!(state.scale_handoff_velocity().unwrap()[0], expected);
    }
}
