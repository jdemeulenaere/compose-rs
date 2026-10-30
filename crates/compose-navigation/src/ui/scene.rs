//! `Scene.kt`, `OverlayScene.kt`, `SceneStrategy.kt`, `SceneDecoratorStrategy.kt` and
//! `SinglePaneScene.kt`.

use std::any::Any;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use compose_runtime::Callback;

use crate::runtime::{ContentKey, IntoContentKey, NavEntry, NavMetadata};

/// A scene as what it is: to tell its type, or to read another's fields when overriding
/// [`Scene::equals`]. Of scenes only, so a handle's `.as_any()` is its scene's.
pub trait SceneAsAny<T> {
    fn as_any(&self) -> &dyn Any;
}

impl<T: Clone + PartialEq + 'static, S: Scene<T>> SceneAsAny<T> for S {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// One or more entries shown together, as `Scene`. Shown by `content`, which composes its entries'
/// [`Content`](NavEntry::Content). [`equals`](Self::equals) is Kotlin's data class's: a scene with
/// more to compare overrides it.
pub trait Scene<T: Clone + PartialEq + 'static>: SceneAsAny<T> + 'static {
    /// What tells it apart from another of its type: the same one animates into itself.
    fn key(&self) -> ContentKey;

    /// What it shows.
    fn entries(&self) -> &[NavEntry<T>];

    /// What going back from it would leave: the scene before it is made of them.
    fn previous_entries(&self) -> &[NavEntry<T>];

    /// Composes what it shows.
    fn content(&self);

    /// What it tells the display, its last entry's by default.
    fn metadata(&self) -> NavMetadata {
        self.entries().last().map_or_else(NavMetadata::new, |entry| entry.metadata().clone())
    }

    /// This, if it's overlaid on others, as `scene as? OverlayScene`.
    fn as_overlay(&self) -> Option<&dyn OverlayScene<T>> {
        None
    }

    /// Whether `other` is the same scene.
    fn equals(&self, other: &dyn Scene<T>) -> bool {
        self.as_any().type_id() == other.as_any().type_id()
            && self.key() == other.key()
            && self.entries() == other.entries()
            && self.previous_entries() == other.previous_entries()
            && self.as_overlay().map(OverlayScene::overlaid_entries)
                == other.as_overlay().map(OverlayScene::overlaid_entries)
    }
}

/// A handle to a scene: what's compared and animated.
pub type SceneRef<T> = Rc<dyn Scene<T>>;

impl<T: Clone + PartialEq + 'static> PartialEq for dyn Scene<T> {
    fn eq(&self, other: &dyn Scene<T>) -> bool {
        self.equals(other)
    }
}

impl<T: Clone + PartialEq + 'static> fmt::Debug for dyn Scene<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Scene({:?})", self.key())
    }
}

/// A scene shown above others, as `OverlayScene`: it implements [`Scene::as_overlay`], returning
/// itself. Those others stay shown under it, and are what going back from it leaves or not, by its
/// `previous_entries`.
pub trait OverlayScene<T: Clone + PartialEq + 'static>: Scene<T> {
    /// What's shown under it. A scene is made of them, which can be one too.
    fn overlaid_entries(&self) -> &[NavEntry<T>];

    /// Runs when it's been popped: it stays composed until this ends, for an animation out, as
    /// `onRemove`.
    fn on_remove(&self) -> Pin<Box<dyn Future<Output = ()>>> {
        Box::pin(std::future::ready(()))
    }
}

/// What a strategy is given, as `SceneStrategyScope`.
pub struct SceneStrategyScope<T> {
    on_back: Callback<dyn Fn()>,
    key: std::marker::PhantomData<T>,
}

/// What a decorator of scenes is given, as `SceneDecoratorStrategyScope`.
pub type SceneDecoratorStrategyScope<T> = SceneStrategyScope<T>;

impl<T> SceneStrategyScope<T> {
    pub(crate) fn of(on_back: Callback<dyn Fn()>) -> SceneStrategyScope<T> {
        SceneStrategyScope { on_back, key: std::marker::PhantomData }
    }

    /// What goes back, the latest given, as `onBack`.
    pub fn on_back(&self) -> Callback<dyn Fn()> {
        self.on_back.clone()
    }
}

impl<T> Default for SceneStrategyScope<T> {
    /// One going nowhere, as `SceneStrategyScope()`.
    fn default() -> SceneStrategyScope<T> {
        SceneStrategyScope::of(compose_runtime::callback(|| {}))
    }
}

/// What makes a scene of the entries it can show, as `SceneStrategy`: a closure is one.
pub trait SceneStrategy<T: Clone + PartialEq + 'static>: 'static {
    /// The scene of `entries`, or `None` for the next strategy to try.
    fn calculate_scene(
        &self,
        scope: &SceneStrategyScope<T>,
        entries: &[NavEntry<T>],
    ) -> Option<SceneRef<T>>;
}

impl<T, F> SceneStrategy<T> for F
where
    T: Clone + PartialEq + 'static,
    F: Fn(&SceneStrategyScope<T>, &[NavEntry<T>]) -> Option<SceneRef<T>> + 'static,
{
    fn calculate_scene(
        &self,
        scope: &SceneStrategyScope<T>,
        entries: &[NavEntry<T>],
    ) -> Option<SceneRef<T>> {
        self(scope, entries)
    }
}

/// A handle to a strategy, compared by identity.
pub type SceneStrategyRef<T> = Rc<dyn SceneStrategy<T>>;

/// `strategy` as a handle.
pub fn scene_strategy<T: Clone + PartialEq + 'static>(
    strategy: impl SceneStrategy<T>,
) -> SceneStrategyRef<T> {
    Rc::new(strategy)
}

/// What wraps a scene in another, as `SceneDecoratorStrategy`: a closure is one. An overlay isn't
/// given to it.
pub trait SceneDecoratorStrategy<T: Clone + PartialEq + 'static>: 'static {
    fn decorate_scene(
        &self,
        scope: &SceneDecoratorStrategyScope<T>,
        scene: SceneRef<T>,
    ) -> SceneRef<T>;
}

impl<T, F> SceneDecoratorStrategy<T> for F
where
    T: Clone + PartialEq + 'static,
    F: Fn(&SceneDecoratorStrategyScope<T>, SceneRef<T>) -> SceneRef<T> + 'static,
{
    fn decorate_scene(
        &self,
        scope: &SceneDecoratorStrategyScope<T>,
        scene: SceneRef<T>,
    ) -> SceneRef<T> {
        self(scope, scene)
    }
}

/// A handle to a decorator of scenes, compared by identity.
pub type SceneDecoratorStrategyRef<T> = Rc<dyn SceneDecoratorStrategy<T>>;

/// `strategy` as a handle.
pub fn scene_decorator_strategy<T: Clone + PartialEq + 'static>(
    strategy: impl SceneDecoratorStrategy<T>,
) -> SceneDecoratorStrategyRef<T> {
    Rc::new(strategy)
}

/// The last entry alone, what's left of the stack being what's before it, as `SinglePaneScene`.
pub(crate) struct SinglePaneScene<T: 'static> {
    key: ContentKey,
    entries: [NavEntry<T>; 1],
    previous_entries: Vec<NavEntry<T>>,
}

impl<T: Clone + PartialEq + 'static> Scene<T> for SinglePaneScene<T> {
    fn key(&self) -> ContentKey {
        self.key.clone()
    }

    fn entries(&self) -> &[NavEntry<T>] {
        &self.entries
    }

    fn previous_entries(&self) -> &[NavEntry<T>] {
        &self.previous_entries
    }

    fn content(&self) {
        self.entries[0].Content();
    }
}

/// Shows the last entry alone, as `SinglePaneSceneStrategy`: what a display falls back to.
#[derive(Clone, Copy, Default, Debug)]
pub struct SinglePaneSceneStrategy;

impl<T: Clone + PartialEq + 'static> SceneStrategy<T> for SinglePaneSceneStrategy {
    fn calculate_scene(
        &self,
        _: &SceneStrategyScope<T>,
        entries: &[NavEntry<T>],
    ) -> Option<SceneRef<T>> {
        let (last, previous) = entries.split_last()?;
        Some(Rc::new(SinglePaneScene {
            key: last.content_key().clone().into_content_key(),
            entries: [last.clone()],
            previous_entries: previous.to_vec(),
        }))
    }
}

/// The first scene of `scene_strategies`, or of [`SinglePaneSceneStrategy`], as
/// `calculateSceneWithSinglePaneFallback`.
pub(crate) fn calculate_scene_with_single_pane_fallback<T: Clone + PartialEq + 'static>(
    scene_strategies: &[SceneStrategyRef<T>],
    scope: &SceneStrategyScope<T>,
    entries: &[NavEntry<T>],
) -> SceneRef<T> {
    scene_strategies
        .iter()
        .find_map(|strategy| strategy.calculate_scene(scope, entries))
        .or_else(|| SinglePaneSceneStrategy.calculate_scene(scope, entries))
        .expect("a scene of no entries")
}
