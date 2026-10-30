//! Navigation 3's UI (`androidx.navigation3.ui`, `androidx.navigation3.scene`): `NavDisplay` shows
//! its entries as [`Scene`]s, animated into one another (D1405).

mod dialog;
mod display;
mod info;
mod predictive;
mod scene;
mod state;

pub use dialog::{DialogKey, DialogSceneStrategy};
pub use display::{
    LOCAL_NAV_ANIMATED_CONTENT_SCOPE, NavDisplay, NavDisplayOfEntries, NavDisplayOfState,
    PopTransitionKey, PredictivePopTransitionKey, TransitionKey, default_pop_transition_spec,
    default_predictive_pop_transition_spec, default_transition_spec,
};
pub use info::{
    SceneInfo, SceneNavigationBackHandler, TitleMetadataKey, UrlMetadataKey,
    remember_scene_navigation_event_state,
};
pub use scene::{
    OverlayScene, Scene, SceneAsAny, SceneDecoratorStrategy, SceneDecoratorStrategyRef,
    SceneDecoratorStrategyScope, SceneRef, SceneStrategy, SceneStrategyRef, SceneStrategyScope,
    SinglePaneSceneStrategy, scene_decorator_strategy, scene_strategy,
};
pub use state::{SceneState, remember_scene_state};
