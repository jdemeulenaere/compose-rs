//! `SceneInfo.kt`, `RememberNavigationEventState.kt` and `NavigationEventHandler.kt`.

use std::fmt;
use std::rc::Rc;

use compose_navigation_event::{
    NavigationBackHandler, NavigationEventState, remember_navigation_event_state,
};
use compose_runtime::composable;

use super::scene::SceneRef;
use super::state::SceneState;
use crate::runtime::NavMetadataKey;

/// The metadata key of what a scene's title is, as `TitleMetadataKey`.
#[derive(Clone, Copy, Default, Debug)]
pub struct TitleMetadataKey;

impl NavMetadataKey for TitleMetadataKey {
    type Value = Rc<dyn Fn() -> String>;
}

/// The metadata key of what a scene's URL is, as `UrlMetadataKey`.
#[derive(Clone, Copy, Default, Debug)]
pub struct UrlMetadataKey;

impl NavMetadataKey for UrlMetadataKey {
    type Value = Rc<dyn Fn() -> String>;
}

/// Where a display is, for what listens to its navigation events, as `SceneInfo`.
pub struct SceneInfo<T: Clone + PartialEq + 'static> {
    pub scene: SceneRef<T>,
    pub title: Option<String>,
    pub url: Option<String>,
}

impl<T: Clone + PartialEq + 'static> SceneInfo<T> {
    /// `scene`'s, with its metadata's title and URL.
    pub fn of(scene: SceneRef<T>) -> SceneInfo<T> {
        let metadata = scene.metadata();
        let title = metadata.get(&TitleMetadataKey).map(|title| title());
        let url = metadata.get(&UrlMetadataKey).map(|url| url());
        SceneInfo { scene, title, url }
    }

    /// `scene`, with no title or URL, as `SceneInfo(scene)`.
    pub fn new(scene: SceneRef<T>) -> SceneInfo<T> {
        SceneInfo { scene, title: None, url: None }
    }
}

impl<T: Clone + PartialEq + 'static> Clone for SceneInfo<T> {
    fn clone(&self) -> Self {
        SceneInfo { scene: self.scene.clone(), title: self.title.clone(), url: self.url.clone() }
    }
}

impl<T: Clone + PartialEq + 'static> PartialEq for SceneInfo<T> {
    fn eq(&self, other: &Self) -> bool {
        *self.scene == *other.scene && self.title == other.title && self.url == other.url
    }
}

impl<T: Clone + PartialEq + 'static> fmt::Debug for SceneInfo<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "SceneInfo(scene={:?}, title={:?}, url={:?})",
            self.scene, self.title, self.url
        )
    }
}

/// What tells of a display's navigation events, following `scene_state`, as
/// `rememberNavigationEventState(sceneState)`.
#[composable(non_restartable)]
pub fn remember_scene_navigation_event_state<T: Clone + PartialEq + 'static>(
    scene_state: SceneState<T>,
) -> NavigationEventState<SceneInfo<T>> {
    let current_info = SceneInfo::of(scene_state.current_scene().clone());
    let back_info: Vec<SceneInfo<T>> =
        scene_state.previous_scenes().iter().map(|scene| SceneInfo::of(scene.clone())).collect();
    remember_navigation_event_state!(current_info, back_info = back_info)
}

/// Takes back gestures when a scene has something before it, popping what its scene shows, as
/// `NavigationBackHandler(sceneState, …)`.
#[composable]
pub fn SceneNavigationBackHandler<T: Clone + PartialEq + 'static>(
    scene_state: SceneState<T>,
    state: NavigationEventState<SceneInfo<T>>,
    #[default(|| {})] on_back_cancelled: impl Fn() + 'static,
    on_back_completed: impl Fn() + 'static,
) {
    let is_back_enabled = !scene_state.current_scene().previous_entries().is_empty();
    let popping = scene_state.clone();
    NavigationBackHandler!(
        state,
        is_back_enabled = is_back_enabled,
        on_back_cancelled = on_back_cancelled,
        on_back_completed = compose_runtime::callback(move || {
            // If it's stale, enabled when it shouldn't be, this pops nothing, as Kotlin's.
            let previous = popping.current_scene().previous_entries().len();
            for _ in previous..popping.entries().len() {
                on_back_completed();
            }
        })
    );
}
