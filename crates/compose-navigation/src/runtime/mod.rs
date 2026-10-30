//! Navigation 3's runtime (`androidx.navigation3.runtime`): the back stack is state the app owns,
//! its keys become [`NavEntry`]s, and [`NavEntryDecorator`]s wrap what they show (D1400).

mod back_stack;
mod content_key;
mod decorated;
mod decorator;
mod entry;
mod metadata;
mod provider;
mod result;
mod view_model;

pub use back_stack::{
    NavBackStack, NavKey, NavList, NavListArg, OwnedNavBackStack, remember_nav_back_stack,
};
pub use content_key::{ContentKey, IntoContentKey};
pub use decorated::{remember_decorated_nav_entries, remember_decorated_nav_entries_from};
pub use decorator::{
    NavEntryDecorator, SaveableStateHolderNavEntryDecorator,
    remember_saveable_state_holder_nav_entry_decorator,
};
pub use entry::NavEntry;
pub use metadata::{MetadataScope, NavMetadata, NavMetadataKey, metadata};
pub use provider::{
    ClassEntryBuilder, EntryBuilder, EntryProviderScope, KeyClass, entry_provider,
    entry_provider_with_fallback,
};
pub use result::{
    LocalResultEventBus, ResultEffect, ResultEventBus, ResultEventBusNavEntryDecorator,
    remember_result_event_bus, remember_result_event_bus_nav_entry_decorator,
};
pub use view_model::{
    ViewModelStoreNavEntryDecorator, remember_view_model_store_nav_entry_decorator,
    remember_view_model_store_nav_entry_decorator_of,
};
