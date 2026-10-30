//! `DecoratedNavEntries.kt`.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use compose_runtime::{
    Callback, ComposeArg, DisposableEffect, RecomposeScope, composable, current_recompose_scope,
    key, remember, remember_keyed,
};

use super::back_stack::NavList;
use super::content_key::ContentKey;
use super::decorator::NavEntryDecorator;
use super::entry::NavEntry;

/// The group of a decorated entry's content: one place in the source, as Kotlin's lambda.
const CONTENT_GROUP: u64 = compose_runtime::__private::key_of("compose_navigation::decorated");

/// A list as a key, by `==`.
struct Equal<T>(Rc<Vec<T>>);

impl<T> Clone for Equal<T> {
    fn clone(&self) -> Self {
        Equal(self.0.clone())
    }
}

impl<T: PartialEq + 'static> ComposeArg for Equal<T> {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}

/// What a place's content shows.
struct Decoration<T: 'static> {
    entry: NavEntry<T>,
    decorators: Rc<Vec<NavEntryDecorator<T>>>,
}

impl<T> Clone for Decoration<T> {
    fn clone(&self) -> Self {
        Decoration { entry: self.entry.clone(), decorators: self.decorators.clone() }
    }
}

/// What differs is what's written: the entry by `==`, the decorators by identity.
impl<T: PartialEq + 'static> ComposeArg for Decoration<T> {
    fn same(&self, other: &Self) -> bool {
        self.entry == other.entry
            && (Rc::ptr_eq(&self.decorators, &other.decorators)
                || self.decorators == other.decorators)
    }
}

/// A place among a call's entries: one content, as `ComposableLambdaImpl` is one lambda, whose
/// `update` here is `state`'s write. `last` is what's compared: reading the state there would
/// subscribe what decorates to what it writes.
struct Place<T: 'static> {
    shown: Rc<Shown<T>>,
    content: Rc<dyn Fn(T)>,
    decorated: RefCell<NavEntry<T>>,
}

/// What a place's content shows, as a `ComposableLambda`'s block: changing it invalidates the scopes
/// that read it (`trackWrite`), not as a state's write, heard by whatever read it since as well.
struct Shown<T: 'static> {
    decoration: RefCell<Decoration<T>>,
    readers: RefCell<Vec<RecomposeScope>>,
}

/// Keys by `==`, as Kotlin's `remember(backStack.toList())`.
struct Keys<T>(Vec<T>);

impl<T: PartialEq + 'static> ComposeArg for Keys<T> {
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

/// The provider last used, and the entries it made, by key.
struct Provided<T: 'static> {
    provider: Callback<dyn Fn(T) -> NavEntry<T>>,
    entries: Vec<(T, NavEntry<T>)>,
}

/// What the effects of a call share, as its `keysInBackstack`, `keysInComposition` and its updated
/// states.
struct Tracking<T: 'static> {
    in_back_stack: RefCell<HashSet<ContentKey>>,
    in_composition: RefCell<HashSet<ContentKey>>,
    latest_keys: RefCell<Vec<ContentKey>>,
    latest_decorators: RefCell<Rc<Vec<NavEntryDecorator<T>>>>,
}

impl<T> Tracking<T> {
    /// In the reverse of their order, as each one's `onDispose` would.
    fn pop(&self, content_key: &ContentKey) {
        let decorators = self.latest_decorators.borrow().clone();
        for decorator in decorators.iter().rev() {
            decorator.on_pop(content_key.clone());
        }
    }
}

/// The entries of `back_stack`, made by `entry_provider` while it holds the same keys and decorated
/// with `entry_decorators`, the first going outermost, as `rememberDecoratedNavEntries`. A key that
/// stays keeps its entry while the provider's the same, as Kotlin's closures being stable keep
/// what's shown from showing again. Hoist the back stack and the decorators' state: what's returned
/// survives recompositions only. Make one call for each back stack, whose results can be
/// concatenated.
#[composable(non_restartable)]
pub fn remember_decorated_nav_entries<T: Clone + PartialEq + 'static>(
    back_stack: impl NavList<T>,
    #[default(Vec::new())] entry_decorators: impl NavList<NavEntryDecorator<T>>,
    entry_provider: impl Fn(T) -> NavEntry<T> + 'static,
) -> Vec<NavEntry<T>> {
    let keys = back_stack.to_nav_list();
    let made = keys.clone();
    let kept: Rc<RefCell<Option<Provided<T>>>> = remember(Rc::default);
    let entries: Rc<Vec<NavEntry<T>>> = remember_keyed(Keys(keys), move || {
        let mut kept = kept.borrow_mut();
        let reusable = kept
            .as_ref()
            .filter(|provided| provided.provider.same(&entry_provider))
            .map(|provided| &provided.entries);
        let entries: Vec<(T, NavEntry<T>)> = made
            .into_iter()
            .map(|key| {
                let entry = reusable
                    .and_then(|entries| entries.iter().find(|(old, _)| *old == key))
                    .map_or_else(|| entry_provider(key.clone()), |(_, entry)| entry.clone());
                (key, entry)
            })
            .collect();
        let shown = entries.iter().map(|(_, entry)| entry.clone()).collect();
        *kept = Some(Provided { provider: entry_provider, entries });
        Rc::new(shown)
    });
    remember_decorated_nav_entries_from(entries.to_vec(), entry_decorators)
}

/// [`remember_decorated_nav_entries`] of entries, decorated already or not, as its other overload:
/// `entry_decorators` go outside what they have.
#[composable(non_restartable)]
pub fn remember_decorated_nav_entries_from<T: Clone + PartialEq + 'static>(
    entries: impl NavList<NavEntry<T>>,
    #[default(Vec::new())] entry_decorators: impl NavList<NavEntryDecorator<T>>,
) -> Vec<NavEntry<T>> {
    let decorators = Rc::new(entry_decorators.to_nav_list());
    let tracking: Rc<Tracking<T>> = remember(|| {
        Rc::new(Tracking {
            in_back_stack: RefCell::default(),
            in_composition: RefCell::default(),
            latest_keys: RefCell::default(),
            latest_decorators: RefCell::new(decorators.clone()),
        })
    });
    *tracking.latest_decorators.borrow_mut() = decorators.clone();
    let entries = entries.to_nav_list();
    let content_keys = std::cell::OnceCell::new();
    for decorator in decorators.iter() {
        if decorator.waits_for_first_pass() {
            let keys = content_keys.get_or_init(|| {
                entries.iter().map(|entry| entry.content_key().clone()).collect::<Vec<_>>()
            });
            decorator.first_pass(keys);
        }
    }
    let decorated: Vec<NavEntry<T>> =
        entries.iter().map(|entry| decorate_entry(entry, &decorators, &tracking)).collect();
    prepare_back_stack(&decorated, &tracking);
    decorated
}

/// `entry` with `decorators`, by its place among those with its content key, as `decorateEntry`.
/// A place has one content, however it's decorated: `rememberComposableLambda` keeps one lambda
/// for a place and updates what it calls, so what's shown follows a state, and entries of a place
/// are equal whenever their key and metadata are. It's what calls `on_pop` for an entry popped while
/// it was still shown (leaving with its pop animation, say): [`prepare_back_stack`] can't know of
/// those. An entry can outlive its place, so the content holds its state rather than the slot.
fn decorate_entry<T: Clone + PartialEq + 'static>(
    entry: &NavEntry<T>,
    decorators: &Rc<Vec<NavEntryDecorator<T>>>,
    tracking: &Rc<Tracking<T>>,
) -> NavEntry<T> {
    key(entry.content_key().clone(), || {
        let decoration = Decoration { entry: entry.clone(), decorators: decorators.clone() };
        let place: Rc<Place<T>> = remember(|| {
            let shown = Rc::new(Shown {
                decoration: RefCell::new(decoration.clone()),
                readers: RefCell::new(Vec::new()),
            });
            let content: Rc<dyn Fn(T)> = {
                let (shown, tracking) = (shown.clone(), tracking.clone());
                Rc::new(move |_| {
                    let scope = current_recompose_scope();
                    let mut readers = shown.readers.borrow_mut();
                    if !readers.contains(&scope) {
                        readers.push(scope);
                    }
                    drop(readers);
                    let decoration = shown.decoration.borrow().clone();
                    show_decorated(&decoration, &tracking)
                })
            };
            let decorated =
                RefCell::new(NavEntry::with_content(entry, content.clone(), CONTENT_GROUP));
            Rc::new(Place { shown, content, decorated })
        });
        if !place.shown.decoration.borrow().same(&decoration) {
            if place.shown.decoration.borrow().entry != decoration.entry {
                *place.decorated.borrow_mut() =
                    NavEntry::with_content(entry, place.content.clone(), CONTENT_GROUP);
            }
            *place.shown.decoration.borrow_mut() = decoration;
            let readers = std::mem::take(&mut *place.shown.readers.borrow_mut());
            for reader in readers {
                reader.invalidate();
            }
        }
        place.decorated.borrow().clone()
    })
}

fn show_decorated<T: Clone + 'static>(decoration: &Decoration<T>, tracking: &Rc<Tracking<T>>) {
    let Decoration { entry, decorators } = decoration;
    let content_key = entry.content_key().clone();
    let tracking = tracking.clone();
    DisposableEffect(content_key.clone(), move |scope| {
        tracking.in_composition.borrow_mut().insert(content_key.clone());
        scope.on_dispose(move || {
            let left = tracking.in_composition.borrow_mut().remove(&content_key);
            let popped = !tracking.in_back_stack.borrow().contains(&content_key);
            if popped && left {
                tracking.pop(&content_key);
            }
        })
    });
    let mut distinct: Vec<&NavEntryDecorator<T>> = Vec::with_capacity(decorators.len());
    for decorator in decorators.iter() {
        if !distinct.contains(&decorator) {
            distinct.push(decorator);
        }
    }
    let mut wrapped = entry.clone();
    for decorator in distinct.into_iter().rev() {
        let (decorator, inner) = (decorator.clone(), wrapped.clone());
        wrapped = NavEntry::from_entry(&inner.clone(), move |_| decorator.decorate(inner.clone()));
    }
    wrapped.Content();
}

/// Follows the back stack, and calls `on_pop` for entries popped that aren't being shown, or never
/// were, as `PrepareBackStack`.
fn prepare_back_stack<T: Clone + PartialEq + 'static>(
    entries: &[NavEntry<T>],
    tracking: &Rc<Tracking<T>>,
) {
    *tracking.latest_keys.borrow_mut() =
        entries.iter().map(|entry| entry.content_key().clone()).collect();
    let list = Equal(Rc::new(entries.to_vec()));
    for entry in entries {
        let content_key = entry.content_key().clone();
        tracking.in_back_stack.borrow_mut().insert(content_key.clone());
        let tracking = tracking.clone();
        DisposableEffect((content_key.clone(), list.clone()), move |scope| {
            scope.on_dispose(move || {
                let gone = !tracking.latest_keys.borrow().contains(&content_key);
                let popped = gone && tracking.in_back_stack.borrow_mut().remove(&content_key);
                if popped && !tracking.in_composition.borrow().contains(&content_key) {
                    tracking.pop(&content_key);
                }
            })
        });
    }
}
