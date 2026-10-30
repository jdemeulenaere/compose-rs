//! `NavEntryDecorator.kt` and `SaveableStateHolderNavEntryDecorator.kt`.

use std::cell::RefCell;
use std::rc::Rc;

use compose_runtime::{
    ComposeArg, SaveableStateHolder, composable, mutable_state_of, remember_keyed,
    remember_saveable,
};

use super::content_key::ContentKey;
use super::entry::{NavEntry, group_of, in_group};

struct DecoratorInner<T: 'static> {
    on_pop: Box<dyn Fn(ContentKey)>,
    decorate: Box<dyn Fn(NavEntry<T>)>,
    group: u64,
    /// What it does with the keys on the stack at its first pass, once.
    first_pass: RefCell<Option<FirstPass>>,
}

/// What a decorator does with the keys of the first pass, once.
type FirstPass = Box<dyn FnOnce(&[ContentKey])>;

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
            group: group_of::<F, NavEntry<T>>(),
            first_pass: RefCell::new(None),
        };
        NavEntryDecorator { inner: Rc::new(inner) }
    }

    fn with_first_pass(self, first_pass: impl FnOnce(&[ContentKey]) + 'static) -> Self {
        *self.inner.first_pass.borrow_mut() = Some(Box::new(first_pass));
        self
    }

    pub(crate) fn waits_for_first_pass(&self) -> bool {
        self.inner.first_pass.borrow().is_some()
    }

    pub(crate) fn first_pass(&self, content_keys: &[ContentKey]) {
        let first_pass = self.inner.first_pass.borrow_mut().take();
        if let Some(first_pass) = first_pass {
            first_pass(content_keys);
        }
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
/// `rememberSaveableStateHolderNavEntryDecorator`. It saves which keys it has state for: a stack that
/// failed to restore starts again, and what was kept of keys no longer on it goes at the first pass
/// (D1407), as there's no other time to know them.
#[composable]
pub fn remember_saveable_state_holder_nav_entry_decorator<T: Clone + 'static>(
    #[default(crate::remember_saveable_state_holder())] saveable_state_holder: SaveableStateHolder,
) -> NavEntryDecorator<T> {
    let known = remember_saveable(|| mutable_state_of(Vec::<i64>::new()));
    remember_keyed(saveable_state_holder, || {
        let holder = saveable_state_holder;
        NavEntryDecorator::with_on_pop(
            move |content_key: ContentKey| {
                let key = holder.state_key(&content_key) as i64;
                known.update(|known| known.iter().copied().filter(|old| *old != key).collect());
                holder.remove_state(content_key);
            },
            move |entry: NavEntry<T>| {
                let content_key = entry.content_key().clone();
                let key = holder.state_key(&content_key) as i64;
                if !known.with(|known| known.contains(&key)) {
                    known.update(|known| known.iter().copied().chain([key]).collect());
                }
                holder.SaveableStateProvider(content_key, || entry.Content());
            },
        )
        .with_first_pass(move |content_keys| {
            let on_stack: Vec<i64> =
                content_keys.iter().map(|key| holder.state_key(key) as i64).collect();
            let stale: Vec<i64> = known
                .with(|known| known.iter().copied().filter(|k| !on_stack.contains(k)).collect());
            for key in &stale {
                holder.remove_state_of_key(*key as u64);
            }
            if !stale.is_empty() {
                known
                    .update(|known| known.iter().copied().filter(|k| !stale.contains(k)).collect());
            }
        })
    })
}
