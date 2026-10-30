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
    ComposeArg, DisposableEffect, State, composable, key, mutable_state_of, remember_keyed,
};
use std::hash::{Hash, Hasher};

use compose_ui::{
    Alignment, Constraints, IntOffset, IntSize, LOCAL_LAYOUT_DIRECTION, LayoutDirection,
    LayoutModifierNode, Measurable, MeasurePolicy, MeasureResult, MeasureScope, Modifier,
    ModifierNode, ModifierNodeElement, RECTANGLE_SHAPE,
};

use crate::deferred::MutableContentTransform;
use crate::spec::{AnimationSpec, Spring, spring, spring_with, tween};
use crate::transition::{
    DeferredAnimation, DeferredTransition, Segment, Transition, update_transition,
};
use crate::vector::VisibilityThreshold;
use crate::visibility::{
    AnimatedEnterExit, AnimatedVisibilityScope, EnterExitState, EnterTransition, ExitTransition,
    fade_in_with, fade_out_with, scale_in_with, slide_in_horizontally_with,
    slide_in_vertically_with, slide_out_horizontally_with, slide_out_vertically_with,
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

/// Where a content slides into or out of its container, as Compose's
/// `AnimatedContentTransitionScope.SlideDirection`: `Start` and `End` follow the layout
/// direction.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SlideDirection {
    Left,
    Right,
    Up,
    Down,
    Start,
    End,
}

/// What a `transition_spec` is given, as Compose's `AnimatedContentTransitionScope`: the change
/// (its `initial_state` and `target_state`, as a `Segment`), the content alignment, and the
/// slides into and out of the container.
pub struct AnimatedContentTransitionScope<S: 'static> {
    initial_state: S,
    target_state: S,
    root: Rc<Root<S>>,
}

impl<S: Clone + 'static> Clone for AnimatedContentTransitionScope<S> {
    fn clone(&self) -> Self {
        AnimatedContentTransitionScope {
            initial_state: self.initial_state.clone(),
            target_state: self.target_state.clone(),
            root: self.root.clone(),
        }
    }
}

impl<S: Clone + PartialEq + 'static> AnimatedContentTransitionScope<S> {
    /// The state the change starts from, as `initialState`.
    pub fn initial_state(&self) -> &S {
        &self.initial_state
    }

    /// The state the change goes to, as `targetState`.
    pub fn target_state(&self) -> &S {
        &self.target_state
    }

    /// Whether the change goes from `initial` to `target`, as `isTransitioningTo`.
    pub fn is_transitioning_to(&self, initial: &S, target: &S) -> bool {
        self.initial_state == *initial && self.target_state == *target
    }

    /// The change, as the scope's `Segment`.
    pub fn segment(&self) -> Segment<S> {
        Segment {
            initial_state: self.initial_state.clone(),
            target_state: self.target_state.clone(),
        }
    }

    /// How the contents are aligned in the container, as `contentAlignment`.
    pub fn content_alignment(&self) -> Alignment {
        self.root.content_alignment.get()
    }

    /// The entering content slides in from the container's edge `towards` the other, along a
    /// spring of a pixel's threshold, as `slideIntoContainer(towards)`.
    pub fn slide_into_container(&self, towards: SlideDirection) -> EnterTransition {
        self.slide_into_container_with(towards, default_container_slide_spec(), |offset| offset)
    }

    /// The same along `animation_spec`, from `initial_offset(the offset of a full slide)`, as
    /// `slideIntoContainer(towards, animationSpec, initialOffset)`: the slide accounts for the
    /// container's size (animated) and the content alignment.
    pub fn slide_into_container_with(
        &self,
        towards: SlideDirection,
        animation_spec: impl Into<AnimationSpec>,
        initial_offset: impl Fn(i32) -> i32 + 'static,
    ) -> EnterTransition {
        let root = self.root.clone();
        let spec = animation_spec.into();
        match self.root.horizontal(towards) {
            Some(true) => slide_in_horizontally_with(spec, move |width| {
                let current = root.current_size.get();
                initial_offset(current.width - root.offset(IntSize::new(width, width), current).x)
            }),
            Some(false) => slide_in_horizontally_with(spec, move |width| {
                let current = root.current_size.get();
                initial_offset(-root.offset(IntSize::new(width, width), current).x - width)
            }),
            None => match towards {
                SlideDirection::Up => slide_in_vertically_with(spec, move |height| {
                    let current = root.current_size.get();
                    let offset = root.offset(IntSize::new(height, height), current);
                    initial_offset(current.height - offset.y)
                }),
                SlideDirection::Down => slide_in_vertically_with(spec, move |height| {
                    let current = root.current_size.get();
                    initial_offset(-root.offset(IntSize::new(height, height), current).y - height)
                }),
                _ => EnterTransition::NONE,
            },
        }
    }

    /// The exiting content slides out of the container `towards` its edge, along a spring of
    /// a pixel's threshold, as `slideOutOfContainer(towards)`.
    pub fn slide_out_of_container(&self, towards: SlideDirection) -> ExitTransition {
        self.slide_out_of_container_with(towards, default_container_slide_spec(), |offset| offset)
    }

    /// The same along `animation_spec`, to `target_offset(the offset of a full slide)`, as
    /// `slideOutOfContainer(towards, animationSpec, targetOffset)`: the slide accounts for the
    /// target content's size and the content alignment.
    pub fn slide_out_of_container_with(
        &self,
        towards: SlideDirection,
        animation_spec: impl Into<AnimationSpec>,
        target_offset: impl Fn(i32) -> i32 + 'static,
    ) -> ExitTransition {
        let root = self.root.clone();
        let spec = animation_spec.into();
        // The transition's target when the offset is computed, as Compose's (a change of target
        // after this exit was made counts).
        match self.root.horizontal(towards) {
            Some(true) => slide_out_horizontally_with(spec, move |width| {
                let size = root.target_size(&(root.target_state)());
                target_offset(-root.offset(IntSize::new(width, width), size).x - width)
            }),
            Some(false) => slide_out_horizontally_with(spec, move |width| {
                let size = root.target_size(&(root.target_state)());
                target_offset(-root.offset(IntSize::new(width, width), size).x + size.width)
            }),
            None => match towards {
                SlideDirection::Up => slide_out_vertically_with(spec, move |height| {
                    let size = root.target_size(&(root.target_state)());
                    target_offset(-root.offset(IntSize::new(height, height), size).y - height)
                }),
                SlideDirection::Down => slide_out_vertically_with(spec, move |height| {
                    let size = root.target_size(&(root.target_state)());
                    target_offset(-root.offset(IntSize::new(height, height), size).y + size.height)
                }),
                _ => ExitTransition::NONE,
            },
        }
    }
}

/// The spec of a slide into or out of the container by default: `spring()` with a pixel's
/// threshold, as Compose's.
fn default_container_slide_spec() -> AnimationSpec {
    AnimationSpec::Spring(spring().visibility_threshold(IntOffset::VISIBILITY_THRESHOLD))
}

/// What tells a state's content from another's, as Compose's `contentKey`: the state itself by
/// default ([`ContentKey::state`]), or a key of it ([`ContentKey::of`]); a new target state of
/// the same key as a content shown takes that content over, without animating.
pub struct ContentKey<T> {
    key: Option<KeyOf<T>>,
}

/// A state's key, as the hash of what `ContentKey::of` was given.
type KeyOf<T> = Rc<dyn Fn(&T) -> u64>;

impl<T> Clone for ContentKey<T> {
    fn clone(&self) -> Self {
        ContentKey { key: self.key.clone() }
    }
}

impl<T: 'static> ContentKey<T> {
    /// The state itself, as `contentKey = { it }`.
    pub fn state() -> ContentKey<T> {
        ContentKey { key: None }
    }

    /// `key(state)`, as `contentKey = { … }`. Keys are compared by their hash.
    pub fn of<K: Hash>(key: impl Fn(&T) -> K + 'static) -> ContentKey<T> {
        ContentKey {
            key: Some(Rc::new(move |state| {
                let mut hasher = std::hash::DefaultHasher::new();
                key(state).hash(&mut hasher);
                hasher.finish()
            })),
        }
    }
}

impl<T: 'static> ComposeArg for ContentKey<T> {
    fn same(&self, other: &Self) -> bool {
        match (&self.key, &other.key) {
            (None, None) => true,
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// What a content of `AnimatedContent` is given, as Compose's `AnimatedContentScope`: an
/// `AnimatedVisibilityScope` (its `transition`, `animate_enter_exit`).
#[derive(Clone)]
pub struct AnimatedContentScope {
    scope: AnimatedVisibilityScope,
}

impl std::ops::Deref for AnimatedContentScope {
    type Target = AnimatedVisibilityScope;
    fn deref(&self) -> &AnimatedVisibilityScope {
        &self.scope
    }
}

impl ComposeArg for AnimatedContentScope {
    fn same(&self, other: &Self) -> bool {
        self.scope.same(&other.scope)
    }
}

/// The states shown, each with the id its content is keyed by: its content key's (keyed), or
/// one of its own.
pub(crate) struct Visible<T> {
    pub(crate) states: Vec<(u64, T)>,
    pub(crate) next_id: u64,
    /// The ids given by state while the transition runs: a state dropped from `states` and added
    /// back in one change keeps its content, as Compose's `key(contentKey(it))`.
    pub(crate) ids: Vec<(T, u64)>,
}

impl<T> Visible<T> {
    pub(crate) fn new() -> Visible<T> {
        Visible { states: Vec::new(), next_id: 0, ids: Vec::new() }
    }
}

impl<T: 'static> ContentKey<T> {
    /// Whether the two states have the same key (with the default key: are equal).
    fn is_same(&self, a: &T, b: &T) -> bool
    where
        T: PartialEq,
    {
        match &self.key {
            None => a == b,
            Some(key) => key(a) == key(b),
        }
    }
}

impl<T: PartialEq + Clone> Visible<T> {
    /// Where `state`'s content is: the state's, or one of the same key.
    pub(crate) fn position(&self, key: &ContentKey<T>, state: &T) -> Option<usize> {
        match &key.key {
            None => self.states.iter().position(|(_, shown)| shown == state),
            Some(key) => {
                let wanted = key(state);
                self.states.iter().position(|(_, shown)| key(shown) == wanted)
            }
        }
    }

    /// The id of a new content for `state`.
    pub(crate) fn new_id(&mut self, key: &ContentKey<T>, state: &T) -> u64 {
        match &key.key {
            None => {
                if let Some((_, id)) = self.ids.iter().find(|(known, _)| known == state) {
                    return *id;
                }
                self.next_id += 1;
                self.ids.push((state.clone(), self.next_id - 1));
                self.next_id - 1
            }
            Some(key) => key(state),
        }
    }
}

/// What `AnimatedContent`'s parts share, as Compose's `AnimatedContentTransitionScopeImpl`.
struct Root<T> {
    content_alignment: Cell<Alignment>,
    layout_direction: Cell<LayoutDirection>,
    /// The container's size as last measured (animated, while it animates), as `currentSize`.
    current_size: Cell<IntSize>,
    /// Each content's measured size, by state.
    target_sizes: RefCell<Vec<(T, State<IntSize>)>>,
    /// The transition's target state when read, as the `transition.targetState` Compose's
    /// slides out of the container read when they compute their offsets.
    target_state: Box<dyn Fn() -> T>,
}

impl<T> Root<T> {
    /// Whether a slide `towards` goes left (`Some(true)`), right (`Some(false)`) or neither.
    fn horizontal(&self, towards: SlideDirection) -> Option<bool> {
        let ltr = self.layout_direction.get() == LayoutDirection::Ltr;
        match towards {
            SlideDirection::Left => Some(true),
            SlideDirection::Right => Some(false),
            SlideDirection::Start => Some(ltr),
            SlideDirection::End => Some(!ltr),
            SlideDirection::Up | SlideDirection::Down => None,
        }
    }

    /// Where a content of `full` is placed in `current`, as `calculateOffset`.
    fn offset(&self, full: IntSize, current: IntSize) -> IntOffset {
        self.content_alignment.get().align(full, current, LayoutDirection::Ltr)
    }
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

/// Shows `content(scope, target_state)`; when the state changes, the new content enters as the
/// old one exits (`transition_spec(scope)`, the scope holding the change), stacked and aligned
/// by `content_alignment`, as Compose's `AnimatedContent`. States are told apart by
/// `content_key` (the state itself, with `==`, by default). Every content still exiting stays
/// until its exit ends.
#[composable(non_restartable)]
#[allow(non_snake_case)]
pub fn AnimatedContent<T: Clone + PartialEq + 'static>(
    target_state: T,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(|_| crate::default_content_transform())] transition_spec: impl Fn(
        AnimatedContentTransitionScope<T>,
    ) -> ContentTransform
    + 'static,
    #[default(crate::Alignment::TOP_START)] content_alignment: Alignment,
    #[default("AnimatedContent")] label: &'static str,
    #[default(crate::ContentKey::state())] content_key: ContentKey<T>,
    content: impl Fn(AnimatedContentScope, T) + 'static,
) {
    let transition = update_transition(target_state, label);
    AnimatedContentOf(
        transition,
        modifier,
        transition_spec,
        content_alignment,
        content_key,
        None,
        compose_runtime::callback(content),
    );
}

impl<S: Clone + PartialEq + 'static> DeferredTransition<S> {
    /// `AnimatedContent` of this transition's states with a deferred phase, as Compose's
    /// `DeferredTransition.DeferredAnimatedContent(modifier, transitionSpec, contentAlignment,
    /// contentKey, mutableTransformSpec, content)`: a pending target's content is composed early,
    /// and both contents can be moved by what `mutable_transform_spec` gives for the change (its
    /// scope's states the transition's target and the pending one; `None`: not moved); when the
    /// phase ends, their enter and exit take up from there. A method, so every parameter is given:
    /// `|_| default_content_transform()` for Compose's default spec, `Alignment::TOP_START` its
    /// default alignment, `ContentKey::state()` its default key.
    #[allow(non_snake_case)]
    pub fn DeferredAnimatedContent(
        &self,
        modifier: Modifier,
        transition_spec: impl Fn(AnimatedContentTransitionScope<S>) -> ContentTransform + 'static,
        content_alignment: Alignment,
        content_key: ContentKey<S>,
        mutable_transform_spec: impl Fn(
            AnimatedContentTransitionScope<S>,
        ) -> Option<MutableContentTransform>
        + 'static,
        content: impl Fn(AnimatedContentScope, S) + 'static,
    ) {
        AnimatedContentOf(
            self.transition.clone(),
            modifier,
            compose_runtime::callback(transition_spec),
            content_alignment,
            content_key,
            Some(compose_runtime::callback(mutable_transform_spec)),
            compose_runtime::callback(content),
        );
    }
}

impl<S: Clone + PartialEq + 'static> Transition<S> {
    /// `AnimatedContent` of this transition's states, as Compose's
    /// `Transition.AnimatedContent(modifier, transitionSpec, contentAlignment, contentKey,
    /// content)`: the transition's target state's content enters as the others exit, on the
    /// transition's clock (`AnimatedContent(target_state, …)` is this of
    /// `update_transition(target_state)`). A method, so every parameter is given:
    /// `|_| default_content_transform()` for Compose's default spec, `Alignment::TOP_START` its
    /// default alignment, `ContentKey::state()` its default key.
    #[allow(non_snake_case)]
    pub fn AnimatedContent(
        &self,
        modifier: Modifier,
        transition_spec: impl Fn(AnimatedContentTransitionScope<S>) -> ContentTransform + 'static,
        content_alignment: Alignment,
        content_key: ContentKey<S>,
        content: impl Fn(AnimatedContentScope, S) + 'static,
    ) {
        AnimatedContentOf(
            self.clone(),
            modifier,
            compose_runtime::callback(transition_spec),
            content_alignment,
            content_key,
            None,
            compose_runtime::callback(content),
        );
    }
}

/// How many of the most recent states a deferred transition's `AnimatedContent` keeps while
/// interrupted, as Compose's `MaxInterruptionRetention`.
const MAX_INTERRUPTION_RETENTION: usize = 3;

type MutableSpec<T> = compose_runtime::Callback<
    dyn Fn(AnimatedContentTransitionScope<T>) -> Option<MutableContentTransform>,
>;

type Spec<T> =
    compose_runtime::Callback<dyn Fn(AnimatedContentTransitionScope<T>) -> ContentTransform>;
type Content<T> = compose_runtime::Callback<dyn Fn(AnimatedContentScope, T)>;

/// `AnimatedContent` of a transition, as Compose's `Transition.AnimatedContentImpl`.
#[composable(non_restartable)]
#[allow(non_snake_case)]
pub(crate) fn AnimatedContentOf<T: Clone + PartialEq + 'static>(
    transition: Transition<T>,
    modifier: Modifier,
    transition_spec: Spec<T>,
    content_alignment: Alignment,
    content_key: ContentKey<T>,
    mutable_transform_spec: Option<MutableSpec<T>>,
    content: Content<T>,
) {
    let layout_direction = LOCAL_LAYOUT_DIRECTION.current();
    let root = remember_keyed(transition.clone(), || {
        let read = transition.clone();
        Rc::new(Root {
            content_alignment: Cell::new(content_alignment),
            layout_direction: Cell::new(layout_direction),
            current_size: Cell::new(IntSize::ZERO),
            target_sizes: RefCell::new(Vec::new()),
            target_state: Box::new(move || read.target_state()),
        })
    });
    let visible = remember_keyed(transition.clone(), || {
        let current = transition.current_state();
        let mut visible = Visible::new();
        let id = visible.new_id(&content_key, &current);
        visible.states.push((id, current));
        Rc::new(RefCell::new(visible))
    });
    // Bumped when a content leaves: the stack composes again without it.
    let removed = remember_keyed(transition.clone(), || mutable_state_of(0u64));
    let current = transition.current_state();
    let target = transition.target_state();
    let pending = transition.pending_target_state();
    {
        let mut shown = visible.borrow_mut();
        if !shown.states.iter().any(|(_, state)| *state == current) {
            let id = shown.new_id(&content_key, &current);
            shown.states = vec![(id, current.clone())];
        }
        if current == target && pending.is_none() {
            // At rest: the current state's content only.
            if shown.states.len() != 1 || shown.states[0].1 != current {
                let kept = shown.position(&ContentKey::state(), &current);
                let id = match kept {
                    Some(index) => shown.states[index].0,
                    None => shown.new_id(&content_key, &current),
                };
                shown.states = vec![(id, current.clone())];
            }
            shown.ids.retain(|(state, _)| *state == current);
            root.content_alignment.set(content_alignment);
            root.layout_direction.set(layout_direction);
        }
        // A pending target's content, composed early, replacing one of the same key.
        if let Some(pending) = pending.as_ref().filter(|pending| **pending != current)
            && !content_key.is_same(pending, &target)
        {
            match shown.position(&content_key, pending) {
                None => {
                    let id = shown.new_id(&content_key, pending);
                    shown.states.push((id, pending.clone()));
                }
                Some(index) if shown.states[index].1 != *pending => {
                    shown.states[index].1 = pending.clone();
                }
                Some(_) => {}
            }
        }
        if current != target {
            match shown.position(&content_key, &target) {
                None => {
                    let id = shown.new_id(&content_key, &target);
                    shown.states.push((id, target.clone()));
                }
                // The target last; a state of the same key replaced by the target, its content
                // taken over (Compose's `currentlyVisible[id] = targetState`).
                Some(index)
                    if index != shown.states.len() - 1 || shown.states[index].1 != target =>
                {
                    let (id, _) = shown.states.remove(index);
                    shown.states.push((id, target.clone()));
                }
                Some(_) => {}
            }
        }
    }
    let segment = transition.segment();
    // The change a deferred phase previews, from the transition's target to the pending one, as
    // `PendingAnimatedContentTransitionScope`: what's given for it stays while it's pending.
    let pending_change = {
        let (pending, target) = (pending.clone(), target.clone());
        remember_keyed(StateKey(pending.clone()), move || pending.map(|pending| (target, pending)))
    };
    let mutable_content_transform = mutable_transform_spec.and_then(|spec| {
        let (change, root, key) = (pending_change.clone(), root.clone(), content_key.clone());
        remember_keyed(StateKey(pending.clone()), move || {
            let (initial_state, target_state) = change?;
            if key.is_same(&initial_state, &target_state) {
                Some(MutableContentTransform::new())
            } else {
                spec(AnimatedContentTransitionScope { initial_state, target_state, root })
            }
        })
    });
    let content_transform = {
        let spec = transition_spec.clone();
        let (initial_state, target_state) = pending_change
            .clone()
            .unwrap_or((segment.initial_state.clone(), segment.target_state.clone()));
        let scope =
            AnimatedContentTransitionScope { initial_state, target_state, root: root.clone() };
        let key = (SegmentKey(segment.clone()), StateKey(pending.clone()));
        remember_keyed(key, move || spec(scope))
    };
    let size_modifier =
        size_animation_modifier(&transition, &root, content_transform.size_transform.clone());
    let states: Vec<(u64, T)> = visible.borrow().states.clone();
    let policy = {
        let (root, visible) = (root.clone(), visible.clone());
        remember_keyed(transition.clone(), move || {
            Rc::new(ContentMeasurePolicy {
                root,
                visible,
                target: RefCell::new(None),
                pending_target: RefCell::new(None),
            })
        })
    };
    *policy.target.borrow_mut() = Some(target.clone());
    // A pending content doesn't count in the container's size, unless it's the same as one that
    // does.
    *policy.pending_target.borrow_mut() = pending
        .clone()
        .filter(|pending| {
            !content_key.is_same(pending, &target) && !content_key.is_same(pending, &current)
        })
        .map(|pending| (pending, content_key.clone()));
    let erased = policy.clone() as Rc<dyn MeasurePolicy>;
    let deferred = transition.is_deferred();
    compose_ui::Layout(
        modifier.then(size_modifier),
        erased,
        compose_runtime::callback(move || {
            removed.get();
            let count = states.len();
            for (index, (id, state)) in states.iter().enumerate() {
                let (transition, spec, content) =
                    (transition.clone(), transition_spec.clone(), content.clone());
                let (root, visible, state) = (root.clone(), visible.clone(), state.clone());
                let (pending_change, transform) =
                    (pending_change.clone(), mutable_content_transform.clone());
                let force_visible = deferred && index + MAX_INTERRUPTION_RETENTION >= count;
                key(*id, move || {
                    ContentOfState(
                        transition,
                        spec,
                        root,
                        visible,
                        removed,
                        state,
                        pending_change,
                        transform,
                        force_visible,
                        content,
                    );
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
    pending_change: Option<(T, T)>,
    mutable_content_transform: Option<MutableContentTransform>,
    force_visible: bool,
    content: Content<T>,
) {
    let segment = transition.segment();
    let target = transition.target_state();
    let is_pending = pending_change.as_ref().is_some_and(|(_, pending)| *pending == state);
    let scope_of = {
        let root = root.clone();
        move |initial_state: T, target_state: T| AnimatedContentTransitionScope {
            initial_state,
            target_state,
            root: root.clone(),
        }
    };
    // The enter it was given when it became the target.
    let spec_on_enter = {
        let (spec, segment, state) = (transition_spec.clone(), segment.clone(), state.clone());
        let (scope_of, change) = (scope_of.clone(), pending_change.clone());
        remember_keyed(is_pending, move || {
            if let Some((initial, pending)) = change.filter(|_| is_pending) {
                spec(scope_of(initial, pending))
            } else if state != segment.initial_state && state != segment.target_state {
                spec(scope_of(segment.initial_state, state))
            } else {
                spec(scope_of(segment.initial_state, segment.target_state))
            }
        })
    };
    // The exit it was given when it stopped being the target.
    let exit = {
        let (spec, segment, state) = (transition_spec.clone(), segment.clone(), state.clone());
        let is_target = segment.target_state == state;
        remember_keyed((is_target, is_pending), move || {
            if is_target || is_pending {
                ExitTransition::NONE
            } else if state != segment.initial_state && state != segment.target_state {
                spec(scope_of(state, segment.initial_state)).initial_content_exit
            } else {
                spec(scope_of(segment.initial_state, segment.target_state)).initial_content_exit
            }
        })
    };
    let hold = exit.config().hold;
    let shown = state.clone();
    let mutable_transform = mutable_content_transform.and_then(|transform| {
        if is_pending {
            Some(transform.target_transform().clone())
        } else if state == target {
            Some(transform.initial_transform().clone())
        } else {
            None
        }
    });
    AnimatedEnterExit(
        transition,
        Rc::new(move |current: &T| *current == shown),
        Modifier.z_index(spec_on_enter.target_content_z_index),
        spec_on_enter.target_content_enter.clone(),
        exit,
        Rc::new(move |current, target| {
            current == EnterExitState::PostExit && target == EnterExitState::PostExit && !hold
        }),
        mutable_transform,
        force_visible,
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
                        removed.set(count + 1);
                    }
                })
            });
            content(AnimatedContentScope { scope }, state);
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
/// largest but a pending content, each aligned in it (left to right, as Compose's).
struct ContentMeasurePolicy<T> {
    root: Rc<Root<T>>,
    /// The states shown: at measuring, one to a child, those whose contents left being gone.
    visible: Rc<RefCell<Visible<T>>>,
    /// The target's state: its content is measured first, as Compose's.
    target: RefCell<Option<T>>,
    /// The state of a content not counting in the size, as `isPendingTarget`, and what tells it.
    pending_target: RefCell<Option<(T, ContentKey<T>)>>,
}

impl<T: PartialEq + 'static> MeasurePolicy for ContentMeasurePolicy<T> {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let (target, pending) = {
            let visible = self.visible.borrow();
            let target = self.target.borrow();
            let pending = self.pending_target.borrow();
            let in_step = visible.states.len() == measurables.len();
            let position = |wanted: &T| visible.states.iter().position(|(_, s)| s == wanted);
            (
                target.as_ref().and_then(position).filter(|_| in_step),
                pending
                    .as_ref()
                    .and_then(|(wanted, key)| {
                        visible.states.iter().position(|(_, s)| key.is_same(s, wanted))
                    })
                    .filter(|_| in_step),
            )
        };
        let mut placeables: Vec<Option<compose_ui::Placeable>> = vec![None; measurables.len()];
        if let Some(target) = target {
            placeables[target] = Some(scope.measure(measurables[target], constraints));
        }
        for (index, measurable) in measurables.iter().enumerate() {
            if placeables[index].is_none() {
                placeables[index] = Some(scope.measure(*measurable, constraints));
            }
        }
        let placeables: Vec<_> = placeables.into_iter().flatten().collect();
        let mut size = IntSize::ZERO;
        for (index, placeable) in placeables.iter().enumerate() {
            if pending != Some(index) {
                size = IntSize::new(
                    size.width.max(placeable.width),
                    size.height.max(placeable.height),
                );
            }
        }
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
    let deferred =
        transition.create_deferred_animation::<IntSize>(|| "DeferredAnimation".to_string());
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
    type Node = SizeNode<T>;

    fn create(&self) -> SizeNode<T> {
        SizeNode {
            animation: self.animation.clone(),
            size_transform: self.size_transform.clone(),
            root: self.root.clone(),
            transition: self.transition.clone(),
            last_size: self.last_size.clone(),
        }
    }

    fn update(&self, node: &mut SizeNode<T>) {
        node.animation = self.animation.clone();
        node.size_transform = self.size_transform.clone();
        node.root = self.root.clone();
        node.transition = self.transition.clone();
        node.last_size = self.last_size.clone();
    }

    fn same(&self, other: &Self) -> bool {
        let same_animation = match (&other.animation, &self.animation) {
            (Some(a), Some(b)) => a.same(b),
            (None, None) => true,
            _ => false,
        };
        same_animation
            && other.size_transform == self.size_transform
            && Rc::ptr_eq(&other.root, &self.root)
    }

    fn name(&self) -> &'static str {
        "sizeTransform"
    }
}

impl<T: Clone + PartialEq + 'static> ModifierNode for SizeNode<T> {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }

    fn on_reset(&mut self, _context: &mut compose_ui::NodeContext<'_>) {
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
        self.root.current_size.set(size);
        let at = self.root.content_alignment.get().align(current, size, LayoutDirection::Ltr);
        scope.place_at(&placeable, at);
        scope.layout(size.width, size.height)
    }
}
