//! `NavDisplay.kt`, `NavDisplay.android.kt` and `LocalNavAnimatedContentScope.kt`.

use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use std::rc::Rc;

use compose_animation::{
    AnimatedContentScope, AnimatedContentTransitionScope, ContentKey as AnimationContentKey,
    ContentTransform, DeferredTransition, DeferredTransitionState, SizeTransform,
    default_scale_spec, fade_in_with, fade_out_with, remember_deferred_transition, scale_out_with,
    spring_with, tween,
};
use compose_lifecycle::{LOCAL_LIFECYCLE_OWNER, LifecycleState, remember_lifecycle_owner};
use compose_navigation_event::{
    NavigationEvent, NavigationEventState, NavigationEventTransitionState,
};
use compose_runtime::snapshots::{SnapshotStateList, mutable_state_list_of, mutable_state_map_of};
use compose_runtime::{
    Callback, ComposeArg, CompositionLocal, CompositionLocalProvider, LaunchedEffect, SideEffect,
    composable, derived_state_of, key, remember, remember_keyed,
};
use compose_ui::{Alignment, Modifier, TransformOrigin};

use super::info::{SceneInfo, SceneNavigationBackHandler, remember_scene_navigation_event_state};
use super::predictive::to_mutable_transform;
use super::scene::{SceneDecoratorStrategyRef, SceneRef, SceneStrategyRef};
use super::state::{
    EqualList, ExcludedEntries, LOCAL_ENTRIES_TO_EXCLUDE, SceneState, remember_scene_state,
};
use crate::runtime::{
    ContentKey, NavEntry, NavEntryDecorator, NavList, NavListArg, NavMetadata, NavMetadataKey,
    remember_decorated_nav_entries,
};

type Scope<T> = AnimatedContentTransitionScope<SceneRef<T>>;

/// What an entry shown by a display animates in, as `LocalNavAnimatedContentScope`. Read anywhere
/// else, it panics. An overlay's is the content's under it, which does nothing for it.
pub static LOCAL_NAV_ANIMATED_CONTENT_SCOPE: CompositionLocal<AnimatedContentScope> =
    CompositionLocal::new("LocalNavAnimatedContentScope", || {
        panic!(
            "Unexpected access to LocalNavAnimatedContentScope. You should only access \
             LocalNavAnimatedContentScope inside a NavEntry passed to NavDisplay. \
             AnimatedContentScope in OverlayScenes are no-op."
        )
    });

macro_rules! transition_key {
    ($(#[$meta:meta])* $name:ident, $value:ty) => {
        $(#[$meta])*
        pub struct $name<T>(PhantomData<T>);

        impl<T> $name<T> {
            pub fn new() -> Self {
                $name(PhantomData)
            }
        }

        impl<T> Default for $name<T> {
            fn default() -> Self {
                $name::new()
            }
        }

        impl<T: Clone + PartialEq + 'static> NavMetadataKey for $name<T> {
            type Value = $value;
        }
    };
}

type Transform<T> = Rc<dyn Fn(Scope<T>) -> Option<ContentTransform>>;
type PredictiveTransform<T> = Rc<dyn Fn(Scope<T>, i32) -> Option<ContentTransform>>;

transition_key!(
    /// The metadata key of how a scene animates when it's added, as `TransitionKey`: only a scene's
    /// metadata is read, its last entry's by default. `None` falls back to the display's.
    TransitionKey,
    Transform<T>
);
transition_key!(
    /// The metadata key of how a scene animates when it's popped, as `PopTransitionKey`.
    PopTransitionKey,
    Transform<T>
);
transition_key!(
    /// The metadata key of how a scene animates when a predictive gesture pops it, given the edge
    /// swiped from, as `PredictivePopTransitionKey`.
    PredictivePopTransitionKey,
    PredictiveTransform<T>
);

/// `NavDisplay`'s functions on metadata, as Kotlin's object of that name: a type here, beside the
/// composable of that name.
pub struct NavDisplay {}

impl NavDisplay {
    /// The metadata of how a scene animates when it's added, as `transitionSpec`.
    pub fn transition_spec<T: Clone + PartialEq + 'static>(
        spec: impl Fn(Scope<T>) -> Option<ContentTransform> + 'static,
    ) -> NavMetadata {
        crate::runtime::metadata(|scope| {
            scope.put(&TransitionKey::<T>::new(), Rc::new(spec) as Transform<T>);
        })
    }

    /// The metadata of how a scene animates when it's popped, as `popTransitionSpec`.
    pub fn pop_transition_spec<T: Clone + PartialEq + 'static>(
        spec: impl Fn(Scope<T>) -> Option<ContentTransform> + 'static,
    ) -> NavMetadata {
        crate::runtime::metadata(|scope| {
            scope.put(&PopTransitionKey::<T>::new(), Rc::new(spec) as Transform<T>);
        })
    }

    /// The metadata of how a scene animates when a predictive gesture pops it, as
    /// `predictivePopTransitionSpec`.
    pub fn predictive_pop_transition_spec<T: Clone + PartialEq + 'static>(
        spec: impl Fn(Scope<T>, i32) -> Option<ContentTransform> + 'static,
    ) -> NavMetadata {
        crate::runtime::metadata(|scope| {
            let spec = Rc::new(spec) as PredictiveTransform<T>;
            scope.put(&PredictivePopTransitionKey::<T>::new(), spec);
        })
    }
}

const DEFAULT_TRANSITION_DURATION_MILLISECOND: u32 = 700;

fn fade_through() -> ContentTransform {
    ContentTransform::new(
        fade_in_with(tween(DEFAULT_TRANSITION_DURATION_MILLISECOND), 0.0),
        fade_out_with(tween(DEFAULT_TRANSITION_DURATION_MILLISECOND), 0.0),
    )
}

/// How a display animates to a scene added, by default, as `defaultTransitionSpec`.
pub fn default_transition_spec<T: Clone + PartialEq + 'static>()
-> impl Fn(Scope<T>) -> ContentTransform + Clone + 'static {
    |_| fade_through()
}

/// How a display animates to a scene popped to, by default, as `defaultPopTransitionSpec`.
pub fn default_pop_transition_spec<T: Clone + PartialEq + 'static>()
-> impl Fn(Scope<T>) -> ContentTransform + Clone + 'static {
    |_| fade_through()
}

/// How a predictive gesture moves a display's scenes, by default, as
/// `defaultPredictivePopTransitionSpec`: Material's default effects spring, and a scale.
pub fn default_predictive_pop_transition_spec<T: Clone + PartialEq + 'static>()
-> impl Fn(Scope<T>, i32) -> ContentTransform + Clone + 'static {
    |_, _| {
        ContentTransform::new(
            fade_in_with(spring_with(1.0, 1600.0), 0.0),
            scale_out_with(default_scale_spec(), 0.7, TransformOrigin::CENTER),
        )
    }
}

/// Shows the entries of `back_stack`, as `NavDisplay(backStack, …)`: its scenes, made by
/// `scene_strategies` or else a single pane, animating into one another. `on_back` is what a back
/// gesture calls, by default taking the last key off a stack that can. A scene's entry is shown once,
/// in the topmost scene it's in. Compose's `sharedTransitionScope` comes with shared elements.
#[composable]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn NavDisplay<T: Clone + PartialEq + 'static>(
    back_stack: impl Into<NavListArg<T>>,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::Alignment::TOP_START)] content_alignment: Alignment,
    #[default(None)] on_back: Option<impl Fn() + 'static>,
    #[default(vec![crate::remember_saveable_state_holder_nav_entry_decorator!()])]
    entry_decorators: impl Into<NavListArg<NavEntryDecorator<T>>>,
    #[default(vec![crate::scene_strategy(crate::SinglePaneSceneStrategy)])] scene_strategies: impl Into<
        NavListArg<SceneStrategyRef<T>>,
    >,
    #[default(Vec::new())] scene_decorator_strategies: impl Into<
        NavListArg<SceneDecoratorStrategyRef<T>>,
    >,
    #[default(None)] size_transform: Option<SizeTransform>,
    #[default(crate::default_transition_spec())] transition_spec: impl Fn(Scope<T>) -> ContentTransform
    + 'static,
    #[default(crate::default_pop_transition_spec())] pop_transition_spec: impl Fn(
        Scope<T>,
    )
        -> ContentTransform
    + 'static,
    #[default(crate::default_predictive_pop_transition_spec())]
    predictive_pop_transition_spec: impl Fn(Scope<T>, i32) -> ContentTransform + 'static,
    entry_provider: impl Fn(T) -> NavEntry<T> + 'static,
) {
    assert!(!back_stack.to_nav_list().is_empty(), "NavDisplay backstack cannot be empty");
    let entries = remember_decorated_nav_entries!(
        back_stack.clone(),
        entry_decorators,
        entry_provider = entry_provider
    );
    let on_back: Callback<dyn Fn()> = match on_back {
        Some(on_back) => on_back,
        None => callback(move || back_stack.remove_last()),
    };
    NavDisplayOfEntries(
        entries.into(),
        modifier,
        content_alignment,
        scene_strategies,
        scene_decorator_strategies,
        size_transform,
        transition_spec,
        pop_transition_spec,
        predictive_pop_transition_spec,
        on_back,
    );
}

/// Shows `entries`, decorated already, as `NavDisplay(entries, …)`: for switching between back
/// stacks with their own decorators' state, or concatenating them.
#[composable]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn NavDisplayOfEntries<T: Clone + PartialEq + 'static>(
    entries: impl Into<NavListArg<NavEntry<T>>>,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::Alignment::TOP_START)] content_alignment: Alignment,
    #[default(vec![crate::scene_strategy(crate::SinglePaneSceneStrategy)])] scene_strategies: impl Into<
        NavListArg<SceneStrategyRef<T>>,
    >,
    #[default(Vec::new())] scene_decorator_strategies: impl Into<
        NavListArg<SceneDecoratorStrategyRef<T>>,
    >,
    #[default(None)] size_transform: Option<SizeTransform>,
    #[default(crate::default_transition_spec())] transition_spec: impl Fn(Scope<T>) -> ContentTransform
    + 'static,
    #[default(crate::default_pop_transition_spec())] pop_transition_spec: impl Fn(
        Scope<T>,
    )
        -> ContentTransform
    + 'static,
    #[default(crate::default_predictive_pop_transition_spec())]
    predictive_pop_transition_spec: impl Fn(Scope<T>, i32) -> ContentTransform + 'static,
    on_back: impl Fn() + 'static,
) {
    let entries = entries.to_nav_list();
    assert!(!entries.is_empty(), "NavDisplay entries cannot be empty");
    let scene_state = remember_scene_state!(
        entries,
        scene_strategies,
        scene_decorator_strategies,
        on_back = on_back.clone()
    );
    let navigation_event_state = remember_scene_navigation_event_state(scene_state.clone());
    SceneNavigationBackHandler!(
        scene_state.clone(),
        navigation_event_state.clone(),
        on_back_completed = on_back
    );
    NavDisplayOfState(
        scene_state,
        navigation_event_state,
        modifier,
        content_alignment,
        size_transform,
        transition_spec,
        pop_transition_spec,
        predictive_pop_transition_spec,
    );
}

/// A scene's place among those shown, as `AnimatedSceneKey`.
#[derive(Clone, PartialEq, Eq, Hash)]
struct AnimatedSceneKey(TypeId, ContentKey);

fn scene_key<T: Clone + PartialEq + 'static>(scene: &SceneRef<T>) -> AnimatedSceneKey {
    AnimatedSceneKey(Any::type_id(scene.as_any()), scene.key())
}

/// Whether going from `old` to `new` left a part of it, as `isPop`.
fn is_pop(old: &[ContentKey], new: &[ContentKey]) -> bool {
    old.first() == new.first()
        && new.len() < old.len()
        && new.iter().zip(old).all(|(new, old)| new == old)
}

/// A list of scenes as a key, by `==`.
struct Scenes<T: Clone + PartialEq + 'static>(Vec<SceneRef<T>>);

impl<T: Clone + PartialEq + 'static> ComposeArg for Scenes<T> {
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

/// Shows `scene_state`, as `NavDisplay(sceneState, navigationEventState, …)`.
#[composable]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn NavDisplayOfState<T: Clone + PartialEq + 'static>(
    scene_state: SceneState<T>,
    navigation_event_state: NavigationEventState<SceneInfo<T>>,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::Alignment::TOP_START)] content_alignment: Alignment,
    #[default(None)] size_transform: Option<SizeTransform>,
    #[default(crate::default_transition_spec())] transition_spec: impl Fn(Scope<T>) -> ContentTransform
    + 'static,
    #[default(crate::default_pop_transition_spec())] pop_transition_spec: impl Fn(
        Scope<T>,
    )
        -> ContentTransform
    + 'static,
    #[default(crate::default_predictive_pop_transition_spec())]
    predictive_pop_transition_spec: impl Fn(Scope<T>, i32) -> ContentTransform + 'static,
) {
    let scene: SceneRef<T> = scene_state.current_scene().clone();
    let transition_state = remember(|| DeferredTransitionState::new(scene.clone()));
    let previous_scene: Option<SceneRef<T>> = scene_state.previous_scenes().last().cloned();
    let in_predictive = {
        let (state, has_previous) = (navigation_event_state.clone(), previous_scene.is_some());
        remember_keyed((has_previous, navigation_event_state.clone()), move || {
            derived_state_of(move || {
                matches!(
                    state.transition_state(),
                    NavigationEventTransitionState::InProgress { .. }
                ) && has_previous
            })
        })
    };
    let swipe_edge = {
        let state = navigation_event_state.clone();
        remember_keyed(navigation_event_state.clone(), move || {
            derived_state_of(move || match state.transition_state() {
                NavigationEventTransitionState::InProgress { latest_event, .. } => {
                    latest_event.swipe_edge
                }
                NavigationEventTransitionState::Idle => NavigationEvent::EDGE_NONE,
            })
        })
    };
    let predictive = in_predictive.get();
    let target_scene = match (&previous_scene, predictive) {
        (Some(previous), true) => previous.clone(),
        _ => scene.clone(),
    };
    if predictive {
        transition_state.defer(target_scene.clone());
    } else {
        transition_state.animate_to(target_scene.clone());
    }
    let transition: DeferredTransition<SceneRef<T>> =
        remember_deferred_transition(&transition_state, "scene");

    let content_keys: Vec<ContentKey> =
        scene_state.entries().iter().map(|entry| entry.content_key().clone()).collect();
    let previous_keys = remember(|| Rc::new(RefCell::new(content_keys.clone())));
    let popping = remember_keyed(EqualList(content_keys.clone()), || {
        let result = is_pop(&previous_keys.borrow(), &content_keys);
        *previous_keys.borrow_mut() = content_keys.clone();
        result
    });

    let scene_map = remember(mutable_state_map_of::<AnimatedSceneKey, SceneRef<T>>);
    let z_indices: Rc<RefCell<HashMap<AnimatedSceneKey, f32>>> = remember(Rc::default);
    let initial_key = scene_key(&transition.current_state());
    let target_key = scene_key(&target_scene);
    let initial_z = *z_indices.borrow_mut().entry(initial_key.clone()).or_insert(0.0);
    let known_target = z_indices.borrow().get(&target_key).copied();
    let target_z = match known_target {
        // A content on screen keeps its z-index, whatever's asked: this tracks what's running. Dead,
        // as upstream's: `animate_to` above has updated the transition's target already.
        Some(z) if !predictive && *transition.target_state() != *scene => z,
        _ if initial_key == target_key => initial_z,
        _ if popping || predictive => initial_z - 1.0,
        _ => initial_z + 1.0,
    };
    scene_map.insert(target_key.clone(), target_scene.clone());
    z_indices.borrow_mut().insert(target_key.clone(), target_z);

    let overlay_scenes: Vec<SceneRef<T>> = scene_state.overlay_scenes().to_vec();
    let current_overlays: SnapshotStateList<SceneRef<T>> = remember(mutable_state_list_of);
    {
        let adding = overlay_scenes.clone();
        LaunchedEffect(Scenes(overlay_scenes.clone()), async move {
            for scene in adding {
                let key = scene.key();
                if !current_overlays.with(|scenes| scenes.iter().any(|old| old.key() == key)) {
                    current_overlays.push(scene);
                }
            }
        });
    }

    let excluded = excluded_entries(
        &scene_map.values(),
        &z_indices.borrow(),
        &current_overlays.with(Vec::clone),
        &transition.target_state(),
    );
    let excluded = Rc::new(excluded);

    let content_transform: Rc<dyn Fn(Scope<T>) -> ContentTransform> = {
        let z_indices = z_indices.clone();
        Rc::new(move |scope: Scope<T>| {
            let z_of = |scene: &SceneRef<T>| {
                z_indices.borrow().get(&scene_key(scene)).copied().unwrap_or(0.0)
            };
            let initial_z = z_of(scope.initial_state());
            let target_z = z_of(scope.target_state());
            let is_pop_local = target_z < initial_z;
            let transition_scene = if initial_z >= target_z {
                scope.initial_state().clone()
            } else {
                scope.target_state().clone()
            };
            let metadata = transition_scene.metadata();
            if is_pop_local {
                metadata
                    .get(&PopTransitionKey::<T>::new())
                    .and_then(|spec| spec(scope.clone()))
                    .unwrap_or_else(|| pop_transition_spec(scope))
            } else {
                metadata
                    .get(&TransitionKey::<T>::new())
                    .and_then(|spec| spec(scope.clone()))
                    .unwrap_or_else(|| transition_spec(scope))
            }
        })
    };

    let animated_content_scope: Rc<RefCell<Option<AnimatedContentScope>>> = remember(Rc::default);
    let progress: Rc<dyn Fn() -> f32> = {
        let state = navigation_event_state.clone();
        remember_keyed(navigation_event_state.clone(), move || {
            let last = Cell::new(0.0);
            let progress: Rc<dyn Fn() -> f32> = Rc::new(move || {
                if let NavigationEventTransitionState::InProgress { latest_event, .. } =
                    state.transition_state()
                {
                    last.set(latest_event.progress);
                }
                last.get()
            });
            progress
        })
    };

    let content_size_transform = size_transform.clone();
    let content_scope = animated_content_scope.clone();
    let content_excluded = excluded.clone();
    let settled_transition = transition.clone();
    transition.DeferredAnimatedContent(
        modifier,
        move |scope: Scope<T>| {
            let transform = content_transform(scope);
            ContentTransform {
                target_content_enter: transform.target_content_enter,
                initial_content_exit: transform.initial_content_exit,
                target_content_z_index: target_z,
                size_transform: content_size_transform.clone(),
            }
        },
        content_alignment,
        AnimationContentKey::of(|scene: &SceneRef<T>| scene_key(scene)),
        move |scope: Scope<T>| {
            if !in_predictive.get() {
                return None;
            }
            let spec = scope
                .initial_state()
                .metadata()
                .get(&PredictivePopTransitionKey::<T>::new())
                .and_then(|spec| spec(scope.clone(), swipe_edge.get()))
                .unwrap_or_else(|| predictive_pop_transition_spec(scope, swipe_edge.get()));
            Some(to_mutable_transform(&spec, progress.clone()))
        },
        move |content: AnimatedContentScope, target: SceneRef<T>| {
            let is_settled = *settled_transition.current_state()
                == *settled_transition.target_state()
                && !in_predictive.get();
            let resumed = is_settled && current_overlays.is_empty();
            let lifecycle_owner = remember_lifecycle_owner(
                if resumed { LifecycleState::Resumed } else { LifecycleState::Started },
                Some(LOCAL_LIFECYCLE_OWNER.current()),
            );
            *content_scope.borrow_mut() = Some(content.clone());
            let excluded = content_excluded.get(&scene_key(&target)).cloned().unwrap_or_default();
            CompositionLocalProvider(
                (
                    LOCAL_LIFECYCLE_OWNER.provides(lifecycle_owner),
                    LOCAL_NAV_ANIMATED_CONTENT_SCOPE.provides(content),
                    LOCAL_ENTRIES_TO_EXCLUDE.provides(excluded),
                ),
                move || target.content(),
            );
        },
    );

    // What's left of the scenes shown, once the transition has ended.
    let settling = transition.clone();
    let cleaned_z = z_indices.clone();
    SideEffect(move || {
        let settled = *settling.current_state() == *settling.target_state()
            && settling.pending_target_state().is_none();
        if settled {
            let target = scene_key(&settling.target_state());
            scene_map.retain(|key, _| *key == target);
            cleaned_z.borrow_mut().retain(|key, _| *key == target);
        }
    });

    // The overlays, above the content.
    let overlays: Vec<SceneRef<T>> = current_overlays.with(Vec::clone);
    for overlay in overlays.into_iter().rev() {
        let overlay_key = overlay.key();
        key(overlay_key, || {
            ShowOverlay(
                overlay,
                Rc::from(overlay_scenes.clone()),
                excluded.clone(),
                animated_content_scope.clone(),
                current_overlays,
            );
        });
    }
}

/// One overlay, and its leaving once it's popped.
#[composable(non_restartable)]
#[allow(non_snake_case)]
fn ShowOverlay<T: Clone + PartialEq + 'static>(
    overlay: SceneRef<T>,
    overlay_scenes: Rc<[SceneRef<T>]>,
    excluded: Rc<HashMap<AnimatedSceneKey, ExcludedEntries>>,
    animated_content_scope: Rc<RefCell<Option<AnimatedContentScope>>>,
    current_overlays: SnapshotStateList<SceneRef<T>>,
) {
    let topmost = overlay_scenes.first().is_some_and(|top| **top == *overlay);
    let lifecycle_owner = remember_lifecycle_owner(
        if topmost { LifecycleState::Resumed } else { LifecycleState::Started },
        Some(LOCAL_LIFECYCLE_OWNER.current()),
    );
    let excluded = excluded.get(&scene_key(&overlay)).cloned().unwrap_or_default();
    let shown = overlay.clone();
    let scope = animated_content_scope.borrow().clone();
    let provide = move || {
        CompositionLocalProvider(
            (
                LOCAL_LIFECYCLE_OWNER.provides(lifecycle_owner.clone()),
                LOCAL_ENTRIES_TO_EXCLUDE.provides(excluded.clone()),
            ),
            || shown.content(),
        );
    };
    match scope {
        Some(scope) => {
            CompositionLocalProvider(LOCAL_NAV_ANIMATED_CONTENT_SCOPE.provides(scope), provide);
        }
        None => provide(),
    }
    let overlay_key = overlay.key();
    let popped = !overlay_scenes.iter().any(|scene| scene.key() == overlay_key);
    if popped {
        let leaving = overlay.clone();
        LaunchedEffect(overlay_key, async move {
            leaving.as_overlay().expect("an overlay scene").on_remove().await;
            current_overlays.retain(|scene| !Rc::ptr_eq(scene, &leaving));
        });
    }
}

/// Which entries each scene skips, each entry being shown in one, as `sceneToExcludedEntryMap`.
fn excluded_entries<T: Clone + PartialEq + 'static>(
    scenes: &[SceneRef<T>],
    z_indices: &HashMap<AnimatedSceneKey, f32>,
    overlays: &[SceneRef<T>],
    target: &SceneRef<T>,
) -> HashMap<AnimatedSceneKey, ExcludedEntries> {
    let mut sorted: Vec<&SceneRef<T>> = scenes.iter().collect();
    let z_of = |scene: &SceneRef<T>| z_indices.get(&scene_key(scene)).copied().unwrap_or(0.0);
    sorted.sort_by(|a, b| z_of(b).total_cmp(&z_of(a)));
    let mut unique: Vec<&SceneRef<T>> = Vec::with_capacity(sorted.len());
    for scene in sorted {
        if !unique.iter().any(|known| ***known == **scene) {
            unique.push(scene);
        }
    }
    // Popping, the destination, lowest, claims its entries first.
    let is_pop = unique.first().is_some_and(|top| **target != ***top);
    let mut in_z_order: Vec<&SceneRef<T>> = overlays.iter().chain(unique).collect();
    if is_pop {
        in_z_order.reverse();
    }
    let mut covered: HashSet<ContentKey> = HashSet::new();
    let mut excluded = HashMap::with_capacity(in_z_order.len());
    for scene in in_z_order {
        excluded.insert(scene_key(scene), ExcludedEntries(Rc::new(covered.clone())));
        covered.extend(scene.entries().iter().map(|entry| entry.content_key().clone()));
    }
    excluded
}
