//! `SceneState.kt`, with the decorators it wraps entries in: `SharedEntryInSceneNavEntryDecorator.kt`,
//! `SceneSetupNavEntryDecorator.kt` and `BackStackAwareLifecycleNavEntryDecorator.kt`.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use compose_animation::SharedTransitionScope;
use compose_lifecycle::{LOCAL_LIFECYCLE_OWNER, LifecycleState, remember_lifecycle_owner};
use compose_runtime::{
    Callback, ComposeArg, CompositionLocal, CompositionLocalProvider, composable, key,
    movable_content_of_with, remember, remember_keyed, remember_updated_state,
};
use compose_ui::{Constraints, IntSize, Measurable, MeasurePolicy, MeasureResult, MeasureScope};

use super::display::LOCAL_NAV_ANIMATED_CONTENT_SCOPE;
use super::scene::{
    SceneDecoratorStrategyRef, SceneRef, SceneStrategyRef, SceneStrategyScope,
    calculate_scene_with_single_pane_fallback,
};
use crate::runtime::{
    ContentKey, NavEntry, NavEntryDecorator, NavList, remember_decorated_nav_entries_from,
};

/// Which scene is composing its content, and whether it's an overlay, as `LocalCurrentScene`:
/// `None` outside a display.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct CurrentScene {
    ptr: usize,
    is_overlay: bool,
}

impl CurrentScene {
    pub(crate) fn of<T: Clone + PartialEq + 'static>(scene: &SceneRef<T>) -> Self {
        CurrentScene {
            ptr: Rc::as_ptr(scene).cast::<()>() as usize,
            is_overlay: scene.as_overlay().is_some(),
        }
    }
}

impl ComposeArg for CurrentScene {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

pub(crate) static LOCAL_CURRENT_SCENE: CompositionLocal<Option<CurrentScene>> =
    CompositionLocal::new("LocalCurrentScene", || None);

/// The keys of entries another scene is showing, to skip here, as
/// `LocalEntriesToExcludeFromCurrentScene`: an entry is shown once, in the topmost scene.
#[derive(Clone, Default, PartialEq)]
pub(crate) struct ExcludedEntries(pub(crate) Rc<HashSet<ContentKey>>);

impl ComposeArg for ExcludedEntries {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}

pub(crate) static LOCAL_ENTRIES_TO_EXCLUDE: CompositionLocal<ExcludedEntries> =
    CompositionLocal::new("LocalEntriesToExcludeFromCurrentScene", ExcludedEntries::default);

/// A `Box`'s layout for [`DecorateSharedEntry`], so `compose-navigation` stays independent of
/// `compose-foundation` (D292).
struct SharedEntryBox;

impl MeasurePolicy for SharedEntryBox {
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
        "SharedEntryInScene"
    }
}

#[composable(non_restartable)]
#[allow(non_snake_case)]
fn DecorateSharedEntry<T: Clone + 'static>(scope: SharedTransitionScope, entry: NavEntry<T>) {
    let in_scene = LOCAL_CURRENT_SCENE.current().is_some_and(|scene| !scene.is_overlay);
    if in_scene {
        let state = scope.remember_shared_content_state_with(
            entry.content_key().clone(),
            scope.shared_content_config_with(false),
        );
        let animated = LOCAL_NAV_ANIMATED_CONTENT_SCOPE.current();
        let modifier = scope.shared_element(&state, &animated);
        let policy = remember(|| Rc::new(SharedEntryBox) as Rc<dyn MeasurePolicy>);
        compose_ui::Layout!(
            modifier = modifier,
            measure_policy = policy,
            content = move || entry.Content(),
        );
    } else {
        entry.Content();
    }
}

/// `rememberSharedEntryInSceneNavEntryDecorator`: wraps each entry shown in a non-overlay scene in
/// a `shared_element` of its content key, kept out of a predictive gesture's container transform,
/// so an entry moving between scenes animates its bounds across the display.
#[composable(non_restartable)]
fn remember_shared_entry_in_scene_decorator<T: Clone + 'static>(
    scope: SharedTransitionScope,
) -> NavEntryDecorator<T> {
    let kept = scope.clone();
    remember_keyed(kept, move || {
        NavEntryDecorator::new(move |entry: NavEntry<T>| {
            DecorateSharedEntry(scope.clone(), entry);
        })
    })
}

type MovableEntry<T> = Callback<dyn Fn(NavEntry<T>)>;

/// `rememberSceneSetupNavEntryDecorator`: wraps each entry in a `movable_content_of_with` by its
/// content key so an entry moving between scenes or panes keeps its state and nodes, and skips it
/// where another scene claims it (`LOCAL_ENTRIES_TO_EXCLUDE`).
fn scene_setup_decorator<T: Clone + 'static>() -> NavEntryDecorator<T> {
    let map: Rc<RefCell<HashMap<ContentKey, MovableEntry<T>>>> = Rc::default();
    let popped = map.clone();
    NavEntryDecorator::with_on_pop(
        move |content_key| {
            popped.borrow_mut().remove(&content_key);
        },
        move |entry: NavEntry<T>| {
            let content_key = entry.content_key().clone();
            let excluded = LOCAL_ENTRIES_TO_EXCLUDE.current();
            if !excluded.0.contains(&content_key) {
                let map = map.clone();
                key(content_key.clone(), move || {
                    let movable = remember(move || {
                        map.borrow_mut()
                            .entry(content_key)
                            .or_insert_with(|| {
                                movable_content_of_with(|entry: NavEntry<T>| entry.Content())
                            })
                            .clone()
                    });
                    movable(entry);
                });
            }
        },
    )
}

/// The keys of a back stack, for what's shown of it to read, as a state.
#[derive(Clone)]
struct BackStackKeys(Rc<Vec<ContentKey>>);

impl ComposeArg for BackStackKeys {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}

/// `rememberBackStackAwareLifecycleNavEntryDecorator`: an entry off the back stack, leaving, is
/// created at most. Kept while the display is, what it reads being a state.
#[composable(non_restartable)]
fn remember_back_stack_aware_lifecycle_decorator<T: Clone + 'static>(
    entries: Vec<NavEntry<T>>,
) -> NavEntryDecorator<T> {
    let keys =
        BackStackKeys(Rc::new(entries.iter().map(|entry| entry.content_key().clone()).collect()));
    let updated = remember_updated_state(keys);
    remember(move || {
        NavEntryDecorator::new(move |entry: NavEntry<T>| {
            let in_back_stack = updated.get().0.contains(entry.content_key());
            let max_lifecycle =
                if in_back_stack { LifecycleState::Resumed } else { LifecycleState::Created };
            let owner =
                remember_lifecycle_owner(max_lifecycle, Some(LOCAL_LIFECYCLE_OWNER.current()));
            CompositionLocalProvider(LOCAL_LIFECYCLE_OWNER.provides(owner), move || {
                entry.Content();
            });
        })
    })
}

struct SceneStateInner<T: 'static> {
    entries: Vec<NavEntry<T>>,
    overlay_scenes: Vec<SceneRef<T>>,
    current_scene: SceneRef<T>,
    previous_scenes: Vec<SceneRef<T>>,
}

/// What a display shows, as `SceneState`: a handle.
pub struct SceneState<T: 'static> {
    inner: Rc<SceneStateInner<T>>,
}

impl<T> Clone for SceneState<T> {
    fn clone(&self) -> Self {
        SceneState { inner: self.inner.clone() }
    }
}

impl<T> ComposeArg for SceneState<T> {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl<T: Clone + PartialEq + 'static> PartialEq for SceneState<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
            || (self.inner.entries == other.inner.entries
                && self.inner.overlay_scenes == other.inner.overlay_scenes
                && *self.inner.current_scene == *other.inner.current_scene
                && self.inner.previous_scenes == other.inner.previous_scenes)
    }
}

impl<T> SceneState<T> {
    /// The entries, wrapped with what scenes need.
    pub fn entries(&self) -> &[NavEntry<T>] {
        &self.inner.entries
    }

    /// The overlays over the current scene, the topmost first.
    pub fn overlay_scenes(&self) -> &[SceneRef<T>] {
        &self.inner.overlay_scenes
    }

    /// The scene to show.
    pub fn current_scene(&self) -> &SceneRef<T> {
        &self.inner.current_scene
    }

    /// What going back would show, the last being the next.
    pub fn previous_scenes(&self) -> &[SceneRef<T>] {
        &self.inner.previous_scenes
    }
}

/// A list as a key, by `==`.
pub(crate) struct EqualList<T>(pub(crate) Vec<T>);

impl<T: PartialEq + 'static> ComposeArg for EqualList<T> {
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

/// A list as a key, by identity.
struct SameList<T>(Vec<T>);

impl<T: ComposeArg> ComposeArg for SameList<T> {
    fn same(&self, other: &Self) -> bool {
        self.0.len() == other.0.len() && self.0.iter().zip(&other.0).all(|(a, b)| a.same(b))
    }
}

/// The scenes of `entries` by `scene_strategies`, as `rememberSceneState`: made again when those or
/// the strategies change, not when `on_back` does, which they're given the latest of. With
/// `shared_transition_scope`, each entry in a non-overlay scene is wrapped in a shared element of
/// its content key, which reads [`LOCAL_NAV_ANIMATED_CONTENT_SCOPE`].
#[composable(non_restartable)]
pub fn remember_scene_state<T: Clone + PartialEq + 'static>(
    entries: impl NavList<NavEntry<T>>,
    scene_strategies: impl NavList<SceneStrategyRef<T>>,
    #[default(Vec::new())] scene_decorator_strategies: impl NavList<SceneDecoratorStrategyRef<T>>,
    #[default(None)] shared_transition_scope: Option<SharedTransitionScope>,
    on_back: impl Fn() + 'static,
) -> SceneState<T> {
    let current_on_back = remember_updated_state(on_back);
    let entries = entries.to_nav_list();
    let shared = if let Some(scope) = shared_transition_scope {
        Some(remember_shared_entry_in_scene_decorator(scope))
    } else {
        None
    };
    let setup = remember(scene_setup_decorator::<T>);
    let lifecycle = remember_back_stack_aware_lifecycle_decorator(entries.clone());
    let entry_decorators: Vec<NavEntryDecorator<T>> =
        shared.into_iter().chain([setup, lifecycle]).collect();
    let decorated: Vec<NavEntry<T>> =
        remember_decorated_nav_entries_from!(entries, entry_decorators);
    let strategies = scene_strategies.to_nav_list();
    let decorators = scene_decorator_strategies.to_nav_list();
    let kept = (SameList(strategies.clone()), EqualList(decorated.clone()));
    remember_keyed(kept, move || {
        let scope = SceneStrategyScope::of(compose_runtime::callback(move || {
            (current_on_back.get())();
        }));
        calculate_scene_state(&scope, decorated, &strategies, &decorators)
    })
}

fn provide_scene<T: Clone + PartialEq + 'static>(
    scope: &SceneStrategyScope<T>,
    entries: &[NavEntry<T>],
    strategies: &[SceneStrategyRef<T>],
    decorators: &[SceneDecoratorStrategyRef<T>],
) -> SceneRef<T> {
    let scene = calculate_scene_with_single_pane_fallback(strategies, scope, entries);
    decorators.iter().fold(scene, |scene, decorator| {
        if scene.as_overlay().is_some() { scene } else { decorator.decorate_scene(scope, scene) }
    })
}

fn calculate_scene_state<T: Clone + PartialEq + 'static>(
    scope: &SceneStrategyScope<T>,
    entries: Vec<NavEntry<T>>,
    strategies: &[SceneStrategyRef<T>],
    decorators: &[SceneDecoratorStrategyRef<T>],
) -> SceneState<T> {
    let mut all_scenes = vec![provide_scene(scope, &entries, strategies, decorators)];
    while let Some(overlaid) = all_scenes
        .last()
        .and_then(|scene| scene.as_overlay())
        .map(|overlay| overlay.overlaid_entries().to_vec())
    {
        assert!(
            !overlaid.is_empty(),
            "Overlaid entries from {:?} must not be empty",
            all_scenes.last().expect("not empty")
        );
        all_scenes.push(provide_scene(scope, &overlaid, strategies, decorators));
    }
    let current_scene = all_scenes.pop().expect("a scene at least");
    let mut previous_scenes = Vec::new();
    let mut previous_entries = current_scene.previous_entries().to_vec();
    while !previous_entries.is_empty() {
        let scene = provide_scene(scope, &previous_entries, strategies, decorators);
        previous_entries = scene.previous_entries().to_vec();
        previous_scenes.insert(0, scene);
    }
    SceneState {
        inner: Rc::new(SceneStateInner {
            entries,
            overlay_scenes: all_scenes,
            current_scene,
            previous_scenes,
        }),
    }
}
