//! `NavEntryDecorator.kt` and `SaveableStateHolderNavEntryDecorator.kt`.

use std::rc::Rc;

use compose_runtime::{ComposeArg, SaveableStateHolder, composable, remember_keyed};

use super::content_key::ContentKey;
use super::entry::{NavEntry, group_of, in_group};

struct DecoratorInner<T: 'static> {
    on_pop: Box<dyn Fn(ContentKey)>,
    decorate: Box<dyn Fn(NavEntry<T>)>,
    group: u64,
}

/// Wraps what entries show, as `NavEntryDecorator`: a handle, compared by identity. `decorate` runs
/// only for an entry being shown, and calls its [`Content`](NavEntry::Content). `on_pop` cleans up
/// what it keeps for a content key, once the last entry with that key has left the back stack and
/// its content the composition. A subclass in Kotlin is a function returning one here.
pub struct NavEntryDecorator<T: 'static> {
    inner: Rc<DecoratorInner<T>>,
}

impl<T> Clone for NavEntryDecorator<T> {
    fn clone(&self) -> Self {
        NavEntryDecorator { inner: self.inner.clone() }
    }
}

impl<T> PartialEq for NavEntryDecorator<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl<T> ComposeArg for NavEntryDecorator<T> {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl<T: 'static> NavEntryDecorator<T> {
    /// One with nothing to clean up.
    pub fn new<F: Fn(NavEntry<T>) + 'static>(decorate: F) -> NavEntryDecorator<T> {
        NavEntryDecorator::with_on_pop(|_| {}, decorate)
    }

    pub fn with_on_pop<F: Fn(NavEntry<T>) + 'static>(
        on_pop: impl Fn(ContentKey) + 'static,
        decorate: F,
    ) -> NavEntryDecorator<T> {
        let inner = DecoratorInner {
            on_pop: Box::new(on_pop),
            decorate: Box::new(decorate),
            group: group_of::<F>(),
        };
        NavEntryDecorator { inner: Rc::new(inner) }
    }

    pub(crate) fn on_pop(&self, content_key: ContentKey) {
        (self.inner.on_pop)(content_key);
    }

    pub(crate) fn decorate(&self, entry: NavEntry<T>) {
        in_group(self.inner.group, || (self.inner.decorate)(entry));
    }
}

/// Gives each entry's `remember_saveable` calls a place in a holder by its content key, as
/// `SaveableStateHolderNavEntryDecorator`, and forgets them when it's popped. The one decorator
/// that's needed: saving state isn't optional.
pub struct SaveableStateHolderNavEntryDecorator;

impl SaveableStateHolderNavEntryDecorator {
    #[allow(clippy::new_ret_no_self)]
    pub fn new<T: Clone + 'static>(holder: SaveableStateHolder) -> NavEntryDecorator<T> {
        NavEntryDecorator::with_on_pop(
            move |content_key| holder.remove_state(content_key),
            move |entry: NavEntry<T>| {
                let content_key = entry.content_key().clone();
                holder.SaveableStateProvider(content_key, || entry.Content());
            },
        )
    }
}

/// The decorator of `saveable_state_holder`, kept while it is, as
/// `rememberSaveableStateHolderNavEntryDecorator`.
#[composable]
pub fn remember_saveable_state_holder_nav_entry_decorator<T: Clone + 'static>(
    #[default(compose_runtime::remember_saveable_state_holder())]
    saveable_state_holder: SaveableStateHolder,
) -> NavEntryDecorator<T> {
    remember_keyed(saveable_state_holder, || {
        SaveableStateHolderNavEntryDecorator::new(saveable_state_holder)
    })
}
