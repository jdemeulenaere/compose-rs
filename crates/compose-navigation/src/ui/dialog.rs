//! `DialogScene.kt`.

use std::rc::Rc;

use compose_lifecycle::{LOCAL_LIFECYCLE_OWNER, LifecycleState, remember_lifecycle_owner};
use compose_runtime::{Callback, CompositionLocalProvider, composable};
use compose_ui::{Dialog, DialogProperties};

use super::scene::{OverlayScene, Scene, SceneRef, SceneStrategy, SceneStrategyScope};
use crate::runtime::{ContentKey, NavEntry, NavMetadata, NavMetadataKey, metadata};

struct DialogScene<T: 'static> {
    key: ContentKey,
    entries: [NavEntry<T>; 1],
    previous_entries: Vec<NavEntry<T>>,
    properties: DialogProperties,
    on_back: Callback<dyn Fn()>,
}

impl<T: Clone + PartialEq + 'static> Scene<T> for DialogScene<T> {
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
        DialogSceneContent(self.entries[0].clone(), self.properties, self.on_back.clone());
    }

    fn as_overlay(&self) -> Option<&dyn OverlayScene<T>> {
        Some(self)
    }
}

impl<T: Clone + PartialEq + 'static> OverlayScene<T> for DialogScene<T> {
    fn overlaid_entries(&self) -> &[NavEntry<T>] {
        &self.previous_entries
    }
}

#[composable(non_restartable)]
#[allow(non_snake_case)]
fn DialogSceneContent<T: Clone + 'static>(
    entry: NavEntry<T>,
    properties: DialogProperties,
    on_back: Callback<dyn Fn()>,
) {
    let lifecycle_owner =
        remember_lifecycle_owner(LifecycleState::Resumed, Some(LOCAL_LIFECYCLE_OWNER.current()));
    Dialog!(
        on_dismiss_request = on_back,
        properties = properties,
        content = {
            let (lifecycle_owner, entry) = (lifecycle_owner.clone(), entry.clone());
            CompositionLocalProvider(LOCAL_LIFECYCLE_OWNER.provides(lifecycle_owner), move || {
                entry.Content();
            });
        }
    );
}

/// The metadata key of an entry shown as a dialog, as `DialogKey`.
#[derive(Clone, Copy, Default, Debug)]
pub struct DialogKey;

impl NavMetadataKey for DialogKey {
    type Value = DialogProperties;
}

/// Shows an entry with [`DialogSceneStrategy::dialog`]'s metadata in a dialog, over what's before
/// it, as `DialogSceneStrategy`.
#[derive(Clone, Copy, Default, Debug)]
pub struct DialogSceneStrategy;

impl DialogSceneStrategy {
    /// The metadata of an entry to show in a dialog, as `dialog(dialogProperties)`.
    pub fn dialog(properties: DialogProperties) -> NavMetadata {
        metadata(|scope| scope.put(&DialogKey, properties))
    }
}

impl<T: Clone + PartialEq + 'static> SceneStrategy<T> for DialogSceneStrategy {
    fn calculate_scene(
        &self,
        scope: &SceneStrategyScope<T>,
        entries: &[NavEntry<T>],
    ) -> Option<SceneRef<T>> {
        let (last, previous) = entries.split_last()?;
        let properties = *last.metadata().get(&DialogKey)?;
        Some(Rc::new(DialogScene {
            key: last.content_key().clone(),
            entries: [last.clone()],
            previous_entries: previous.to_vec(),
            properties,
            on_back: scope.on_back(),
        }))
    }
}
