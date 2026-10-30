//! `SceneState.kt`, with the decorators it wraps entries in: `SceneSetupNavEntryDecorator.kt` and
//! `BackStackAwareLifecycleNavEntryDecorator.kt`.

use std::collections::HashSet;
use std::rc::Rc;

use compose_lifecycle::{LOCAL_LIFECYCLE_OWNER, LifecycleState, remember_lifecycle_owner};
use compose_runtime::{
    ComposeArg, CompositionLocal, CompositionLocalProvider, composable, key, remember,
    remember_keyed, remember_updated_state,
};

use super::scene::{
    SceneDecoratorStrategyRef, SceneRef, SceneStrategyRef, SceneStrategyScope,
    calculate_scene_with_single_pane_fallback,
};
use crate::runtime::{
    ContentKey, NavEntry, NavEntryDecorator, NavList, remember_decorated_nav_entries_from,
};

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

/// `rememberSceneSetupNavEntryDecorator`, but for what a scene shows moving to another keeping its
/// state, as there's no movable content yet (D1404): it keeps its shape, and an entry showing in a
/// scene after another starts anew there.
fn scene_setup_decorator<T: Clone + 'static>() -> NavEntryDecorator<T> {
    NavEntryDecorator::new(|entry: NavEntry<T>| {
        let excluded = LOCAL_ENTRIES_TO_EXCLUDE.current();
        if !excluded.0.contains(entry.content_key()) {
            key(entry.content_key().clone(), || entry.Content());
        }
    })
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
/// the strategies change, not when `on_back` does, which they're given the latest of. Compose's
/// `sharedTransitionScope` comes with shared elements.
#[composable(non_restartable)]
pub fn remember_scene_state<T: Clone + PartialEq + 'static>(
    entries: impl NavList<NavEntry<T>>,
    scene_strategies: impl NavList<SceneStrategyRef<T>>,
    #[default(Vec::new())] scene_decorator_strategies: impl NavList<SceneDecoratorStrategyRef<T>>,
    on_back: impl Fn() + 'static,
) -> SceneState<T> {
    let current_on_back = remember_updated_state(on_back);
    let entries = entries.to_nav_list();
    let setup = remember(scene_setup_decorator::<T>);
    let lifecycle = remember_back_stack_aware_lifecycle_decorator(entries.clone());
    let decorated: Vec<NavEntry<T>> =
        remember_decorated_nav_entries_from!(entries, vec![setup, lifecycle]);
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
