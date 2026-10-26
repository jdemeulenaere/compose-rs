//! `AnimatedContent`, as Compose's: content for a state that changes with an animation, the
//! new content entering as the old one exits, the container's size animating between theirs.
//!
//! As Compose's (D805): the state drives a transition; each state still shown has its content
//! entering or exiting on it (`AnimatedEnterExit`, visible while it's the target), with the
//! enter it was given when it became the target and the exit it was given when it stopped being
//! it; a content stays until its exit ends, however many states follow; the container's size
//! animates on the transition from the old content's to the new one's (`SizeTransform`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::{
    ComposeArg, DisposableEffect, State, composable, key, mutable_state_of, remember,
    remember_keyed,
};
use compose_ui::{
    Alignment, Constraints, IntSize, LayoutDirection, LayoutModifierNode, Measurable,
    MeasurePolicy, MeasureResult, MeasureScope, Modifier, ModifierNode, ModifierNodeElement,
    RECTANGLE_SHAPE,
};

use crate::spec::{AnimationSpec, Spring, spring_with, tween};
use crate::transition::{DeferredAnimation, Transition, update_transition};
use crate::vector::VisibilityThreshold;
use crate::visibility::{
    AnimatedEnterExit, AnimatedVisibilityScope, EnterExitState, EnterTransition, ExitTransition,
    fade_in_with, fade_out_with, scale_in_with,
};

/// How the container's size animates from the old content's to the new one's, as Compose's
/// `SizeTransform`: clipped to the animated size when `clip`, along the spec it gives for the
/// two sizes.
#[derive(Clone)]
pub struct SizeTransform {
    pub clip: bool,
    size_animation_spec: Rc<dyn Fn(IntSize, IntSize) -> AnimationSpec>,
}

impl SizeTransform {
    /// The spec of a change from `initial_size` to `target_size`, as `createAnimationSpec`.
    pub fn create_animation_spec(
        &self,
        initial_size: IntSize,
        target_size: IntSize,
    ) -> AnimationSpec {
        (self.size_animation_spec)(initial_size, target_size)
    }
}

/// The default: clipped, along a spring of stiffness 400 and a pixel's threshold.
impl Default for SizeTransform {
    /// One shared value, as Compose's `AnimatedContentDefaults.SizeTransform`: equal to itself.
    fn default() -> SizeTransform {
        thread_local! {
            static DEFAULT: SizeTransform = size_transform(true, |_, _| default_size_spec());
        }
        DEFAULT.with(SizeTransform::clone)
    }
}

impl PartialEq for SizeTransform {
    fn eq(&self, other: &SizeTransform) -> bool {
        self.clip == other.clip && Rc::ptr_eq(&self.size_animation_spec, &other.size_animation_spec)
    }
}

impl std::fmt::Debug for SizeTransform {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "SizeTransform(clip={})", self.clip)
    }
}

/// A size transform, as Compose's `SizeTransform(clip, sizeAnimationSpec)`.
pub fn size_transform(
    clip: bool,
    size_animation_spec: impl Fn(IntSize, IntSize) -> AnimationSpec + 'static,
) -> SizeTransform {
    SizeTransform { clip, size_animation_spec: Rc::new(size_animation_spec) }
}

/// The spec of `AnimatedContent`'s size animation by default, as Compose's
/// `AnimatedContentDefaults.SizeAnimationSpec`.
fn default_size_spec() -> AnimationSpec {
    AnimationSpec::Spring(
        spring_with(Spring::DAMPING_RATIO_NO_BOUNCY, Spring::STIFFNESS_MEDIUM_LOW)
            .visibility_threshold(IntSize::VISIBILITY_THRESHOLD),
    )
}

/// How one state's content gives way to the next's, as Compose's `ContentTransform`: the new
/// content's enter, the old one's exit, the new one's z-index, and the container's size
/// transform (none: the container is as large as the largest content).
#[derive(Clone, PartialEq, Debug)]
pub struct ContentTransform {
    pub target_content_enter: EnterTransition,
    pub initial_content_exit: ExitTransition,
    pub target_content_z_index: f32,
    pub size_transform: Option<SizeTransform>,
}

impl ContentTransform {
    /// `enter` for the content coming, `exit` for the one leaving, at z-index 0, with the
    /// default size transform, as `ContentTransform(targetContentEnter, initialContentExit)`.
    pub fn new(enter: EnterTransition, exit: ExitTransition) -> ContentTransform {
        ContentTransform {
            target_content_enter: enter,
            initial_content_exit: exit,
            target_content_z_index: 0.0,
            size_transform: Some(SizeTransform::default()),
        }
    }

    /// The same with `size_transform` (`None`: the container takes the largest content's size),
    /// as `using`.
    pub fn using(self, size_transform: Option<SizeTransform>) -> ContentTransform {
        ContentTransform { size_transform, ..self }
    }
}

impl EnterTransition {
    /// This enter with `exit` for the content leaving, as `togetherWith`.
    pub fn together_with(self, exit: ExitTransition) -> ContentTransform {
        ContentTransform::new(self, exit)
    }
}

/// Compose's default `AnimatedContent` transition: the new content fading (220 ms, after
/// 90 ms) and scaling in from 92 %, the old one fading out (90 ms), the size animating.
pub fn default_content_transform() -> ContentTransform {
    (fade_in_with(tween(220).delay(90), 0.0)
        + scale_in_with(tween(220).delay(90), 0.92, compose_ui::TransformOrigin::CENTER))
    .together_with(fade_out_with(tween(90), 0.0))
}

/// The states shown, each with the id its content is keyed by.
struct Visible<T> {
    states: Vec<(u64, T)>,
    next_id: u64,
}

impl<T: PartialEq> Visible<T> {
    fn position(&self, state: &T) -> Option<usize> {
        self.states.iter().position(|(_, shown)| shown == state)
    }
}

/// What `AnimatedContent`'s parts share, as Compose's `AnimatedContentTransitionScopeImpl`.
struct Root<T> {
    content_alignment: Cell<Alignment>,
    /// Each content's measured size, by state.
    target_sizes: RefCell<Vec<(T, State<IntSize>)>>,
}

impl<T: PartialEq + Clone> Root<T> {
    fn target_size(&self, state: &T) -> IntSize {
        self.target_sizes
            .borrow()
            .iter()
            .find(|(shown, _)| shown == state)
            .map_or(IntSize::ZERO, |(_, size)| size.get())
    }
}

/// Shows `content(target_state)`; when the state changes, the new content enters as the old
/// one exits (`transition_spec(initial, target)`), stacked and aligned by
/// `content_alignment`, as Compose's `AnimatedContent`. States are told apart with `==`. Every
/// content still exiting stays until its exit ends.
#[composable(non_restartable)]
#[allow(non_snake_case)]
pub fn AnimatedContent<T: Clone + PartialEq + 'static>(
    target_state: T,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(|_, _| crate::default_content_transform())] transition_spec: impl Fn(
        T,
        T,
    )
        -> ContentTransform
    + 'static,
    #[default(crate::Alignment::TOP_START)] content_alignment: Alignment,
    #[default("AnimatedContent")] label: &'static str,
    content: impl Fn(T) + 'static,
) {
    let transition = update_transition(target_state, label);
    AnimatedContentOf(
        transition,
        modifier,
        transition_spec,
        content_alignment,
        compose_runtime::callback(content),
    );
}

impl<S: Clone + PartialEq + 'static> Transition<S> {
    /// `AnimatedContent` of this transition's states, as Compose's
    /// `Transition.AnimatedContent(modifier, transitionSpec, contentAlignment, content)`: the
    /// transition's target state's content enters as the others exit, on the transition's clock
    /// (`AnimatedContent(target_state, …)` is this of `update_transition(target_state)`). A
    /// method, so every parameter is given: `default_content_transform` for Compose's default
    /// spec, `Alignment::TOP_START` its default alignment.
    #[allow(non_snake_case)]
    pub fn AnimatedContent(
        &self,
        modifier: Modifier,
        transition_spec: impl Fn(S, S) -> ContentTransform + 'static,
        content_alignment: Alignment,
        content: impl Fn(S) + 'static,
    ) {
        AnimatedContentOf(
            self.clone(),
            modifier,
            compose_runtime::callback(transition_spec),
            content_alignment,
            compose_runtime::callback(content),
        );
    }
}

type Spec<T> = compose_runtime::Callback<dyn Fn(T, T) -> ContentTransform>;

/// `AnimatedContent` of a transition, as Compose's `Transition.AnimatedContentImpl`.
#[composable(non_restartable)]
#[allow(non_snake_case)]
pub(crate) fn AnimatedContentOf<T: Clone + PartialEq + 'static>(
    transition: Transition<T>,
    modifier: Modifier,
    transition_spec: Spec<T>,
    content_alignment: Alignment,
    content: compose_runtime::Callback<dyn Fn(T)>,
) {
    let root = remember_keyed(transition.clone(), || {
        Rc::new(Root {
            content_alignment: Cell::new(content_alignment),
            target_sizes: RefCell::new(Vec::new()),
        })
    });
    let visible = remember_keyed(transition.clone(), || {
        Rc::new(RefCell::new(Visible { states: vec![(0, transition.current_state())], next_id: 1 }))
    });
    // Bumped when a content leaves: the stack composes again without it.
    let removed = remember_keyed(transition.clone(), || mutable_state_of(0u64));
    let current = transition.current_state();
    let target = transition.target_state();
    {
        let mut shown = visible.borrow_mut();
        if shown.position(&current).is_none() {
            let id = shown.next_id;
            shown.next_id += 1;
            shown.states = vec![(id, current.clone())];
        }
        if current == target {
            // At rest: the current state's content only.
            if shown.states.len() != 1 || shown.states[0].1 != current {
                let kept = shown.position(&current).map(|index| shown.states[index].0);
                let id = kept.unwrap_or_else(|| {
                    shown.next_id += 1;
                    shown.next_id - 1
                });
                shown.states = vec![(id, current.clone())];
            }
            root.content_alignment.set(content_alignment);
        } else {
            match shown.position(&target) {
                None => {
                    let id = shown.next_id;
                    shown.next_id += 1;
                    shown.states.push((id, target.clone()));
                }
                Some(index) if index != shown.states.len() - 1 => {
                    let entry = shown.states.remove(index);
                    shown.states.push(entry);
                }
                Some(_) => {}
            }
        }
    }
    let segment = transition.segment();
    let content_transform = {
        let spec = transition_spec.clone();
        let (initial, target) = (segment.initial_state.clone(), segment.target_state.clone());
        remember_keyed(SegmentKey(segment.clone()), move || spec(initial, target))
    };
    let size_modifier =
        size_animation_modifier(&transition, &root, content_transform.size_transform.clone());
    let states: Vec<(u64, T)> = visible.borrow().states.clone();
    let target_index = states.iter().position(|(_, state)| *state == target);
    let policy = {
        let root = root.clone();
        remember_keyed(transition.clone(), move || {
            Rc::new(ContentMeasurePolicy { root, target: Cell::new(None) })
        })
    };
    policy.target.set(target_index);
    let erased = policy.clone() as Rc<dyn MeasurePolicy>;
    compose_ui::Layout(
        modifier.then(size_modifier),
        erased,
        compose_runtime::callback(move || {
            removed.get();
            for (id, state) in &states {
                let (transition, spec, content) =
                    (transition.clone(), transition_spec.clone(), content.clone());
                let (root, visible, state) = (root.clone(), visible.clone(), state.clone());
                key(*id, move || {
                    ContentOfState(transition, spec, root, visible, removed, state, content);
                });
            }
        }),
    );
}

#[derive(Clone, PartialEq)]
struct SegmentKey<T>(crate::transition::Segment<T>);

impl<T: PartialEq + 'static> ComposeArg for SegmentKey<T> {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// One state's content, entering and exiting on the transition, as the content lambdas of
/// Compose's `AnimatedContentImpl`.
#[composable(non_restartable)]
#[allow(non_snake_case)]
fn ContentOfState<T: Clone + PartialEq + 'static>(
    transition: Transition<T>,
    transition_spec: Spec<T>,
    root: Rc<Root<T>>,
    visible: Rc<RefCell<Visible<T>>>,
    removed: compose_runtime::MutableState<u64>,
    state: T,
    content: compose_runtime::Callback<dyn Fn(T)>,
) {
    let segment = transition.segment();
    // The enter it was given when it became the target.
    let spec_on_enter = {
        let (spec, segment, state) = (transition_spec.clone(), segment.clone(), state.clone());
        remember(move || {
            if state != segment.initial_state && state != segment.target_state {
                spec(segment.initial_state, state)
            } else {
                spec(segment.initial_state, segment.target_state)
            }
        })
    };
    // The exit it was given when it stopped being the target.
    let exit = {
        let (spec, segment, state) = (transition_spec.clone(), segment.clone(), state.clone());
        let is_target = segment.target_state == state;
        remember_keyed(is_target, move || {
            if is_target {
                ExitTransition::NONE
            } else if state != segment.initial_state && state != segment.target_state {
                spec(state, segment.initial_state).initial_content_exit
            } else {
                spec(segment.initial_state, segment.target_state).initial_content_exit
            }
        })
    };
    let hold = exit.config().hold;
    let shown = state.clone();
    AnimatedEnterExit(
        transition,
        Rc::new(move |current: &T| *current == shown),
        Modifier.z_index(spec_on_enter.target_content_z_index),
        spec_on_enter.target_content_enter.clone(),
        exit,
        Rc::new(move |current, target| {
            current == EnterExitState::PostExit && target == EnterExitState::PostExit && !hold
        }),
        compose_runtime::callback(move |scope: AnimatedVisibilityScope| {
            let (root, visible, state) = (root.clone(), visible.clone(), state.clone());
            {
                let mut sizes = root.target_sizes.borrow_mut();
                sizes.retain(|(shown, _)| *shown != state);
                sizes.push((state.clone(), scope.target_size));
            }
            let (left_root, left_state) = (root.clone(), state.clone());
            DisposableEffect(StateKey(state.clone()), move |effect| {
                effect.on_dispose(move || {
                    let mut shown = visible.borrow_mut();
                    shown.states.retain(|(_, kept)| *kept != left_state);
                    drop(shown);
                    left_root.target_sizes.borrow_mut().retain(|(kept, _)| *kept != left_state);
                    // Freed already when the transition changed (the stack's state is keyed by
                    // it): nothing to compose again.
                    if let Some(count) = removed.try_get() {
                        removed.set_in_composition(count + 1);
                    }
                })
            });
            content(state);
        }),
    );
}

#[derive(Clone, PartialEq)]
struct StateKey<T>(T);

impl<T: PartialEq + 'static> ComposeArg for StateKey<T> {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// The contents stacked, each measured with the container's constraints, as large as the
/// largest, each aligned in it (left to right, as Compose's).
struct ContentMeasurePolicy<T> {
    root: Rc<Root<T>>,
    /// The target's content, by its index among the children: measured first, as Compose's.
    target: Cell<Option<usize>>,
}

impl<T: 'static> MeasurePolicy for ContentMeasurePolicy<T> {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let mut placeables: Vec<Option<compose_ui::Placeable>> = vec![None; measurables.len()];
        if let Some(target) = self.target.get().filter(|target| *target < measurables.len()) {
            placeables[target] = Some(scope.measure(measurables[target], constraints));
        }
        for (index, measurable) in measurables.iter().enumerate() {
            if placeables[index].is_none() {
                placeables[index] = Some(scope.measure(*measurable, constraints));
            }
        }
        let placeables: Vec<_> = placeables.into_iter().flatten().collect();
        let size = placeables.iter().fold(IntSize::ZERO, |size, placeable| {
            IntSize::new(size.width.max(placeable.width), size.height.max(placeable.height))
        });
        let alignment = self.root.content_alignment.get();
        for placeable in &placeables {
            let at = alignment.align(placeable.size(), size, LayoutDirection::Ltr);
            scope.place_at(placeable, at);
        }
        scope.layout(size.width, size.height)
    }

    fn debug_name(&self) -> &'static str {
        "AnimatedContent"
    }
}

/// The container's size animation, as Compose's `createSizeAnimationModifier`: while a change
/// with a size transform plays, its size animates on the transition, clipped unless the
/// transform says not to.
fn size_animation_modifier<T: Clone + PartialEq + 'static>(
    transition: &Transition<T>,
    root: &Rc<Root<T>>,
    size_transform: Option<SizeTransform>,
) -> Modifier {
    let should_animate = remember_keyed(transition.clone(), || Rc::new(Cell::new(false)));
    if transition.current_state() == transition.target_state() {
        should_animate.set(false);
    } else if size_transform.is_some() {
        should_animate.set(true);
    }
    let deferred = transition.create_deferred_animation::<IntSize>();
    let animation = should_animate.get().then_some(deferred);
    let clip =
        animation.is_some() && size_transform.as_ref().is_none_or(|transform| transform.clip);
    let modifier = if clip { Modifier.clip(RECTANGLE_SHAPE) } else { Modifier };
    let last_size = remember_keyed(transition.clone(), || Rc::new(Cell::new(None::<IntSize>)));
    modifier.then_custom(SizeElement {
        animation,
        size_transform,
        root: root.clone(),
        transition: transition.clone(),
        last_size,
    })
}

struct SizeElement<T: 'static> {
    animation: Option<DeferredAnimation<T, IntSize>>,
    size_transform: Option<SizeTransform>,
    root: Rc<Root<T>>,
    transition: Transition<T>,
    /// The last size it took, as `lastSize`: where an interrupted size animation goes on from.
    last_size: Rc<Cell<Option<IntSize>>>,
}

struct SizeNode<T: 'static> {
    animation: Option<DeferredAnimation<T, IntSize>>,
    size_transform: Option<SizeTransform>,
    root: Rc<Root<T>>,
    transition: Transition<T>,
    last_size: Rc<Cell<Option<IntSize>>>,
}

impl<T: Clone + PartialEq + 'static> ModifierNodeElement for SizeElement<T> {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(SizeNode {
            animation: self.animation.clone(),
            size_transform: self.size_transform.clone(),
            root: self.root.clone(),
            transition: self.transition.clone(),
            last_size: self.last_size.clone(),
        })
    }

    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<SizeNode<T>>() {
            node.animation = self.animation.clone();
            node.size_transform = self.size_transform.clone();
            node.root = self.root.clone();
            node.transition = self.transition.clone();
            node.last_size = self.last_size.clone();
        }
    }

    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<SizeElement<T>>().is_some_and(|other| {
            let same_animation = match (&other.animation, &self.animation) {
                (Some(a), Some(b)) => a.same(b),
                (None, None) => true,
                _ => false,
            };
            same_animation
                && other.size_transform == self.size_transform
                && Rc::ptr_eq(&other.root, &self.root)
        })
    }

    fn name(&self) -> &'static str {
        "sizeTransform"
    }
}

impl<T: Clone + PartialEq + 'static> ModifierNode for SizeNode<T> {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }

    fn on_reset(&mut self) {
        self.last_size.set(None);
    }
}

impl<T: Clone + PartialEq + 'static> LayoutModifierNode for SizeNode<T> {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let placeable = scope.measure(content, constraints);
        let current = placeable.size();
        let size = match &self.animation {
            None => {
                self.last_size.set(Some(current));
                current
            }
            Some(animation) => {
                let last = self.last_size.get().unwrap_or(current);
                let (root, transform) = (self.root.clone(), self.size_transform.clone());
                let initial = self.transition.segment().initial_state;
                let root_for_values = self.root.clone();
                let initial_for_values = initial.clone();
                let animated = animation.animate(
                    move |segment| {
                        let from = if segment.initial_state == initial {
                            last
                        } else {
                            root.target_size(&segment.initial_state)
                        };
                        let to = root.target_size(&segment.target_state);
                        transform.as_ref().map_or_else(default_size_spec, |transform| {
                            transform.create_animation_spec(from, to)
                        })
                    },
                    move |state| {
                        if *state == initial_for_values {
                            last
                        } else {
                            root_for_values.target_size(state)
                        }
                    },
                );
                let size = animated.value();
                self.last_size.set(Some(size));
                size
            }
        };
        let at = self.root.content_alignment.get().align(current, size, LayoutDirection::Ltr);
        scope.place_at(&placeable, at);
        scope.layout(size.width, size.height)
    }
}
