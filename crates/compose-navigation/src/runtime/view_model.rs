//! `lifecycle-viewmodel-navigation3`'s `ViewModelStoreNavEntryDecorator.kt`.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use compose_lifecycle::{
    LOCAL_VIEW_MODEL_STORE_OWNER, ViewModelStoreOwnerRef, ViewModelStoreProvider,
    remember_view_model_store_owner_in, remember_view_model_store_provider,
};
use compose_runtime::{CompositionLocalProvider, composable, remember_keyed};

use super::content_key::ContentKey;
use super::decorator::NavEntryDecorator;
use super::entry::NavEntry;

/// The number a content key's store goes by.
fn store_key(content_key: &ContentKey) -> u64 {
    let mut hasher = DefaultHasher::new();
    content_key.hash(&mut hasher);
    hasher.finish()
}

/// Gives each entry a `ViewModelStore` by its content key, out of `provider`, as
/// `ViewModelStoreNavEntryDecorator`: its `view_model`s live while it's on the stack, shown or not,
/// and are cleared when it's popped.
pub struct ViewModelStoreNavEntryDecorator;

impl ViewModelStoreNavEntryDecorator {
    #[allow(clippy::new_ret_no_self)]
    pub fn new<T: Clone + 'static>(provider: ViewModelStoreProvider) -> NavEntryDecorator<T> {
        let popping = provider.clone();
        NavEntryDecorator::with_on_pop(
            move |content_key: ContentKey| popping.clear_key(store_key(&content_key)),
            move |entry: NavEntry<T>| {
                let key = store_key(entry.content_key());
                let owner = remember_view_model_store_owner_in(provider.clone(), Some(key));
                CompositionLocalProvider(
                    LOCAL_VIEW_MODEL_STORE_OWNER.provides(Some(owner)),
                    || {
                        entry.Content();
                    },
                );
            },
        )
    }
}

/// The decorator of `provider`, kept while it is, as
/// `rememberViewModelStoreNavEntryDecorator(viewModelStoreProvider)`: a provider hoisted keeps its
/// view models while the display goes away, for back stacks to swap.
#[composable]
pub fn remember_view_model_store_nav_entry_decorator_of<T: Clone + 'static>(
    view_model_store_provider: ViewModelStoreProvider,
) -> NavEntryDecorator<T> {
    remember_keyed(view_model_store_provider.clone(), || {
        ViewModelStoreNavEntryDecorator::new(view_model_store_provider.clone())
    })
}

/// A decorator with a provider under `view_model_store_owner`, the local one by default, as
/// `rememberViewModelStoreNavEntryDecorator(viewModelStoreOwner)`.
#[composable]
pub fn remember_view_model_store_nav_entry_decorator<T: Clone + 'static>(
    #[default(crate::local_view_model_store_owner())]
    view_model_store_owner: ViewModelStoreOwnerRef,
) -> NavEntryDecorator<T> {
    let provider =
        remember_view_model_store_provider(Some(view_model_store_owner), None, None, None);
    remember_view_model_store_nav_entry_decorator_of(provider)
}
