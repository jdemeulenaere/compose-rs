//! Lazy layouts on subcomposition, as Compose's `LazyLayout` (C8, D1312): each item is a slot of a
//! `SubcomposeLayout`, composed when the layout's measurement asks for it and deactivated, kept
//! for reuse, when a measurement no longer does. Lists, grids and staggered grids share this: their
//! measurements ask for items through an [`ItemComposer`].

use std::any::Any;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use compose_runtime::{Callback, SaveableStateHolder, composable};
use compose_ui::subcompose::{
    SlotIdsSet, SubcomposeLayoutState, SubcomposeLayoutWithState, SubcomposeMeasureScope,
    SubcomposeSlotReusePolicy, subcompose_policy,
};
use compose_ui::{Constraints, Measurable, MeasureResult, Modifier};

use super::{ItemStacking, Items, ItemsRef, LazyItem, LazyListState, prefetch};

/// A hasher for item indices and keys: a multiply, where `HashMap`'s default hashes each byte
/// through SipHash, per item per measurement.
#[derive(Default)]
pub(super) struct IndexHasher(u64);

impl std::hash::Hasher for IndexHasher {
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0.rotate_left(8) ^ u64::from(*byte)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        }
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0 ^ value).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// A map by item index or key, with [`IndexHasher`].
pub(super) type IndexMap<K, V> = HashMap<K, V, std::hash::BuildHasherDefault<IndexHasher>>;

/// A set of item indices or keys, with [`IndexHasher`].
pub(super) type IndexSet<K> = HashSet<K, std::hash::BuildHasherDefault<IndexHasher>>;

/// When the item contents kept by key are trimmed to those of the keys still composed: past `most`
/// and past twice what stayed the last time, so a measurement composing thousands of new items
/// trims a few times, not once per item (trimming per item, each trim going over all those
/// composed, a frame composing 2,000 new items beside 8,000 took 13 s in a release build, D1477),
/// and a list showing more than half `most` doesn't trim at each measurement.
#[derive(Default)]
struct TrimAt {
    at: std::cell::Cell<usize>,
    /// How many trims there were, for tests.
    trims: std::cell::Cell<usize>,
}

impl TrimAt {
    /// Whether `held` contents are to be trimmed now, past at least `most`.
    fn due(&self, held: usize, most: usize) -> bool {
        held >= self.at.get().max(most)
    }

    /// Trimmed: `left` stayed.
    fn trimmed(&self, left: usize) {
        self.at.set(2 * left);
        self.trims.set(self.trims.get() + 1);
    }
}

/// A map by item index for a measurement: the items it reaches are mostly next to each other, so
/// a list from the first one holds them (a look-up is an index), the others (a pinned item far
/// away) in a map.
pub(super) struct ItemMap<V> {
    base: usize,
    dense: Vec<Option<V>>,
    sparse: IndexMap<usize, V>,
}

/// How far past its first item the list of an `ItemMap` goes, and how far before.
const DENSE_ITEMS: usize = 512;
const DENSE_BEFORE: usize = 16;

impl<V> Default for ItemMap<V> {
    fn default() -> Self {
        ItemMap { base: usize::MAX, dense: Vec::new(), sparse: IndexMap::default() }
    }
}

impl<V> ItemMap<V> {
    pub(super) fn reserve(&mut self, additional: usize) {
        self.dense.reserve(additional);
    }

    fn slot(&self, index: usize) -> Option<usize> {
        let at = index.checked_sub(self.base)?;
        (at < DENSE_ITEMS).then_some(at)
    }

    pub(super) fn get(&self, index: &usize) -> Option<&V> {
        match self.slot(*index) {
            Some(at) => self.dense.get(at).and_then(Option::as_ref),
            None => self.sparse.get(index),
        }
    }

    pub(super) fn contains_key(&self, index: &usize) -> bool {
        self.get(index).is_some()
    }

    pub(super) fn insert(&mut self, index: usize, value: V) {
        if self.base == usize::MAX {
            // Room before the first: a measurement may go back from where it starts.
            self.base = index.saturating_sub(DENSE_BEFORE);
        }
        match self.slot(index) {
            Some(at) => {
                if self.dense.len() <= at {
                    self.dense.resize_with(at + 1, || None);
                }
                self.dense[at] = Some(value);
            }
            None => {
                self.sparse.insert(index, value);
            }
        }
    }
}

/// The most deactivated items kept for reuse per content type, as Compose's
/// `MaxItemsToRetainForReuse`.
const MAX_ITEMS_TO_RETAIN_FOR_REUSE: usize = 7;

/// An item's slot id, as Compose's: its key. Its content type rides along for the reuse policy,
/// which Compose's asks of the item content factory.
#[derive(Clone, Copy, Debug)]
pub(super) struct LazySlot {
    pub(super) key: u64,
    pub(super) content_type: Option<u64>,
}

impl PartialEq for LazySlot {
    fn eq(&self, other: &LazySlot) -> bool {
        self.key == other.key
    }
}

/// The content type of a slot id: none for one whose layout's content went (Compose's
/// `ReusedSlotId`, which any item takes).
fn content_type_of(slot: &dyn Any) -> Option<u64> {
    slot.downcast_ref::<LazySlot>().and_then(|slot| slot.content_type)
}

/// Which items that left are kept for reuse, and which item may reuse which, as Compose's
/// `LazyLayoutItemReusePolicy`: up to `MAX_ITEMS_TO_RETAIN_FOR_REUSE` of each content type, the
/// last used first; an item reuses one of its content type (`None`, Compose's `null`, being one).
pub(super) struct LazyItemReusePolicy;

impl SubcomposeSlotReusePolicy for LazyItemReusePolicy {
    fn get_slots_to_retain(&self, slots: &mut SlotIdsSet) {
        let mut count_per_type: Vec<(Option<u64>, usize)> = Vec::new();
        slots.retain(|slot| {
            let content_type = content_type_of(slot);
            match count_per_type.iter_mut().find(|(kind, _)| *kind == content_type) {
                Some((_, count)) if *count == MAX_ITEMS_TO_RETAIN_FOR_REUSE => false,
                Some((_, count)) => {
                    *count += 1;
                    true
                }
                None => {
                    count_per_type.push((content_type, 1));
                    true
                }
            }
        });
    }

    fn are_compatible(&self, slot: &dyn Any, reusable: &dyn Any) -> bool {
        content_type_of(slot) == content_type_of(reusable)
    }
}

/// What composes each item of one set of items, as Compose's `LazyLayoutItemContentFactory`.
pub(super) struct ItemFactory {
    pub(super) state: LazyListState,
    /// The list's contents by key, `ItemContents`, held here not to look the list up per item.
    contents: Rc<ItemContents>,
    pub(super) items: ItemsRef,
    pub(super) holder: SaveableStateHolder,
    pub(super) stacking: ItemStacking,
}

/// The items' contents, by key, as Compose's `CachedItemContent`s: an item's content is the same
/// `Callback` while its key is there, so a measurement asking for it again composes nothing; it
/// reads the items as they are when it composes (Compose's reads `itemProvider()`), so an item
/// recomposing for a state of its own with the list's content changed sees the new items.
/// A `LazyLayout`'s provider keeps its own (`layout::ProviderSource`).
#[derive(Default)]
pub(super) struct ItemContents {
    pub(super) contents: RefCell<IndexMap<u64, ItemContent>>,
    /// Each item's content's scope, by key: run again when the items change. One per key
    /// composed, trimmed with `contents`.
    pub(super) scopes: RefCell<IndexMap<u64, compose_runtime::RecomposeScope>>,
    /// Each `LazyItem`'s own scope, by key: run again only when the list's holder or stacking
    /// change, which its skipped `SlotContent` wouldn't pass it (D2393, D2449). A change of the
    /// items alone runs the content's scope, as before D2393. Trimmed with `contents`.
    pub(super) item_scopes: RefCell<IndexMap<u64, compose_runtime::RecomposeScope>>,
    /// When `contents` and `scopes` are trimmed next.
    trim_at: TrimAt,
}

/// An item's content, and the index its key was last found at.
pub(super) type ItemContent = (Rc<std::cell::Cell<usize>>, Callback);

/// The contents kept before any trim; past it and `TrimAt`'s bound, those of keys no longer
/// composed go.
const MOST_CONTENTS: usize = 256;

impl ItemContents {
    /// At a measurement's end: past `MOST_CONTENTS` and `TrimAt`'s bound, the contents and scopes
    /// of the keys not in `keys()`, those it composed, go, a key's scope and content together (one
    /// composed again takes a new content, which composes with the items as they are then). Not
    /// while it composes: the contents of those it composed before would go too, and they'd
    /// compose again when it moves them (D1477).
    pub(super) fn trim(&self, keys: impl FnOnce() -> IndexSet<u64>) {
        let mut contents = self.contents.borrow_mut();
        let mut scopes = self.scopes.borrow_mut();
        if !self.trim_at.due(contents.len().max(scopes.len()), MOST_CONTENTS) {
            return;
        }
        let keys = keys();
        contents.retain(|key, _| keys.contains(key));
        scopes.retain(|key, _| keys.contains(key));
        self.item_scopes.borrow_mut().retain(|key, _| keys.contains(key));
        self.trim_at.trimmed(contents.len().max(scopes.len()));
    }

    /// How many contents and scopes are kept, and how many trims there were, for tests.
    pub(super) fn held(&self) -> (usize, usize, usize) {
        (self.contents.borrow().len(), self.scopes.borrow().len(), self.trim_at.trims.get())
    }
}

impl ItemFactory {
    /// The slot id of item `index`.
    pub(super) fn slot(&self, index: usize) -> LazySlot {
        let items = &self.items.0;
        LazySlot { key: items.key(index), content_type: items.content_type(index) }
    }

    /// Item `index`'s content.
    pub(super) fn content(&self, index: usize) -> Callback {
        self.content_of(index, self.items.0.key(index))
    }

    /// Item `index`'s content, its key `key`.
    fn content_of(&self, index: usize, key: u64) -> Callback {
        if let Some((near, content)) = self.contents.contents.borrow().get(&key) {
            near.set(index);
            return content.clone();
        }
        let inner = self.state.inner_or_detached();
        let mut contents = inner.contents.contents.borrow_mut();
        if let Some((near, content)) = contents.get(&key) {
            near.set(index);
            return content.clone();
        }
        let state = self.state;
        let near = Rc::new(std::cell::Cell::new(index));
        let at = near.clone();
        // Its item found by its key in the items as they are now: its slot is its key's.
        let content = Callback::new(move || {
            let inner = state.inner_or_detached();
            let Some(factory) = inner.factory.borrow().clone() else { return };
            drop(inner);
            let items = &factory.items.0;
            let guess = at.get();
            let found = if guess < items.count && items.key(guess) == key {
                Some(guess)
            } else {
                items.index_of(key, guess)
            };
            if let Some(found) = found {
                at.set(found);
                LazyItem(found, key, state);
            }
        });
        contents.insert(key, (near, content.clone()));
        content
    }

    /// At a measurement's end, `composed` the items it composed: the contents and scopes of the
    /// others go, past `ItemContents::trim`'s bound.
    pub(super) fn trim_contents(&self, composed: &[usize]) {
        self.contents.trim(|| composed.iter().map(|item| self.items.0.key(*item)).collect());
    }

    pub(super) fn count(&self) -> usize {
        self.items.0.count
    }
}

/// What a lazy layout's items are composed from, as Compose's item content factory sees its item
/// provider: the lists' [`ItemFactory`] (statically dispatched) and the public `LazyLayout`'s
/// provider (`layout::ProviderSource`, one virtual call per item).
pub(super) trait SlotSource: 'static {
    fn count(&self) -> usize;
    fn key(&self, index: usize) -> u64;
    fn content_type(&self, index: usize) -> Option<u64>;
    /// Item `index`'s content, its key `key`: the same `Callback` while its key is there.
    fn content_of(&self, index: usize, key: u64) -> Callback;
    /// Panics if two of `indices` have the same key, saying whether their full keys are equal
    /// (the lists' `Items` know them; a provider's keys are `u64`s already).
    #[cfg(debug_assertions)]
    fn check_keys(&self, indices: &[usize]);
}

impl SlotSource for ItemFactory {
    #[inline]
    fn count(&self) -> usize {
        self.items.0.count
    }

    #[inline]
    fn key(&self, index: usize) -> u64 {
        self.items.0.key(index)
    }

    #[inline]
    fn content_type(&self, index: usize) -> Option<u64> {
        self.items.0.content_type(index)
    }

    #[inline]
    fn content_of(&self, index: usize, key: u64) -> Callback {
        ItemFactory::content_of(self, index, key)
    }

    #[cfg(debug_assertions)]
    fn check_keys(&self, indices: &[usize]) {
        self.items.0.check_keys(indices);
    }
}

/// The items a measurement composes, as Compose's `LazyLayoutMeasureScope.compose`: each asked for
/// once, its layout returned; in the order asked, which is their slots'. Lists ask for an item's
/// one layout (`measurable`, their `ItemStacking` making one of its nodes), the public
/// `LazyLayout` for all its top-level ones (`measurables`).
pub(super) struct ItemComposer<S: SlotSource = ItemFactory> {
    factory: Rc<S>,
    composed: ItemMap<Option<Measurable>>,
    /// What `measurables` composed, by index.
    all: ItemMap<Vec<Measurable>>,
    order: Vec<usize>,
    #[cfg(debug_assertions)]
    keys: IndexMap<u64, usize>,
}

impl<S: SlotSource> ItemComposer<S> {
    pub(super) fn new(factory: Rc<S>, expected: usize) -> ItemComposer<S> {
        let mut composed = ItemMap::default();
        composed.reserve(expected);
        ItemComposer {
            factory,
            composed,
            all: ItemMap::default(),
            order: Vec::with_capacity(expected),
            #[cfg(debug_assertions)]
            keys: IndexMap::default(),
        }
    }

    /// Notes `key` composed for item `index`: two items of one key would share a slot, and
    /// Compose's lazy layouts panic, as this does (debug builds).
    #[inline]
    fn note_key(&mut self, _key: u64, _index: usize) {
        #[cfg(debug_assertions)]
        {
            if let Some(&other) = self.keys.get(&_key) {
                self.factory.check_keys(&[other, _index]);
            }
            self.keys.insert(_key, _index);
        }
    }

    /// Item `index`'s layout, composed if this measurement hasn't yet. `None` past the items.
    pub(super) fn measurable(
        &mut self,
        scope: &mut SubcomposeMeasureScope<'_, '_>,
        index: usize,
    ) -> Option<Measurable> {
        if let Some(found) = self.composed.get(&index) {
            return *found;
        }
        if index >= self.factory.count() {
            return None;
        }
        let key = self.factory.key(index);
        self.note_key(key, index);
        // An item's content is the same for its key while it's there (`ItemContents`): the slot
        // composed in place needs neither its content type nor its content.
        let probe = LazySlot { key, content_type: None };
        let factory = &self.factory;
        let measurable = scope.subcompose_first_with(&probe, || {
            let slot = LazySlot { key, content_type: factory.content_type(index) };
            (slot, factory.content_of(index, key))
        });
        self.composed.insert(index, measurable);
        self.order.push(index);
        measurable
    }

    /// Item `index`'s top-level layouts, composed if this measurement hasn't yet, as Compose's
    /// `LazyLayoutMeasureScope.compose`. Panics past the items, as Compose's provider's `getKey`
    /// would be asked for an index it hasn't.
    pub(super) fn measurables(
        &mut self,
        scope: &mut SubcomposeMeasureScope<'_, '_>,
        index: usize,
    ) -> Vec<Measurable> {
        if let Some(found) = self.all.get(&index) {
            return found.clone();
        }
        let count = self.factory.count();
        assert!(
            index < count,
            "compose-foundation: item {index} composed in a lazy layout of {count} items"
        );
        let key = self.factory.key(index);
        self.note_key(key, index);
        let content_type = self.factory.content_type(index);
        let content = self.factory.content_of(index, key);
        let measurables = scope.subcompose(LazySlot { key, content_type }, content);
        self.all.insert(index, measurables.clone());
        self.order.push(index);
        measurables
    }

    /// The keys of the items this measurement composed.
    pub(super) fn composed_keys(&self) -> IndexSet<u64> {
        self.order.iter().map(|index| self.factory.key(*index)).collect()
    }
}

impl ItemComposer {
    /// Keeps item `index` composed through this measurement, measured or not.
    pub(super) fn keep(&mut self, scope: &mut SubcomposeMeasureScope<'_, '_>, index: usize) {
        self.measurable(scope, index);
    }

    /// The items this measurement composed, in order.
    pub(super) fn composed(&self) -> impl Iterator<Item = usize> + '_ {
        self.order.iter().copied()
    }

    /// Whether this measurement composed item `index`.
    pub(super) fn has(&self, index: usize) -> bool {
        self.composed.contains_key(&index)
    }

    pub(super) fn items(&self) -> &Rc<Items> {
        &self.factory.items.0
    }

    /// Panics if two items composed have the same key, as Compose's lazy layouts do.
    #[cfg(debug_assertions)]
    pub(super) fn check_keys(&self) {
        self.factory.items.0.check_keys(&self.order);
    }
}

/// A lazy layout's measurement, its items composed as it asks for them.
pub(super) trait ItemsMeasurePolicy: 'static {
    fn measure(
        &self,
        scope: &mut SubcomposeMeasureScope<'_, '_>,
        composer: &mut ItemComposer,
        constraints: Constraints,
    ) -> MeasureResult;
}

/// The layout of a lazy list, grid or staggered grid: a `SubcomposeLayout` whose slots are its
/// items, measured by `policy`. The items' composition reads what they're composed under: the
/// list for nested prefetch, and the constraints their lazy lists guess at before they're measured.
#[composable]
#[allow(non_snake_case)]
pub(super) fn LazyItemsLayout(
    modifier: Modifier,
    state: LazyListState,
    items: ItemsRef,
    holder: SaveableStateHolder,
    fallback: Option<Constraints>,
    stacking: ItemStacking,
    policy: Rc<dyn ItemsMeasurePolicy>,
) {
    let inner = state.inner_or_detached();
    // Kept when the layout's in content reused, as Compose keeps a reused node's subcompositions.
    let layout = compose_runtime::platform::remember_across_reuse(|| {
        SubcomposeLayoutState::with_policy(LazyItemReusePolicy)
    });
    *inner.layout_state.borrow_mut() = Some(layout.clone());
    *inner.prefetch.items.borrow_mut() = Rc::downgrade(&items.0);
    let factory = compose_runtime::remember_keyed((state, items.clone(), holder, stacking), || {
        // The items' contents run again with these items, in this frame after this composition;
        // the items themselves too if what they're given besides the items changed (D2449).
        for (_, scope) in inner.contents.scopes.borrow_mut().drain() {
            scope.invalidate();
        }
        let given_changed = inner
            .factory
            .borrow()
            .as_ref()
            .is_none_or(|last| last.holder != holder || last.stacking != stacking);
        if given_changed {
            for (_, scope) in inner.contents.item_scopes.borrow_mut().drain() {
                scope.invalidate();
            }
        }
        Rc::new(ItemFactory {
            state,
            items: items.clone(),
            holder,
            stacking,
            contents: inner.contents.clone(),
        })
    });
    *inner.factory.borrow_mut() = Some(factory.clone());
    // One per list, kept: the local stays the same, its readers aren't invalidated; its list is
    // the state's now.
    let parent = compose_runtime::platform::remember_across_reuse(|| {
        Rc::new(prefetch::ParentList::new(&inner))
    });
    parent.set(&inner);
    let measure = compose_runtime::remember_keyed((policy.clone(), factory.clone()), || {
        let (policy, factory) = (policy.clone(), factory.clone());
        subcompose_policy(move |scope, constraints| {
            let expected = factory.state.inner_or_detached().children.borrow().len() + 8;
            let mut composer = ItemComposer::new(factory.clone(), expected);
            let result = policy.measure(scope, &mut composer, constraints);
            factory.trim_contents(&composer.order);
            let inner = factory.state.inner_or_detached();
            *inner.children.borrow_mut() = composer.order;
            result
        })
    });
    // The items' constraints, for what's composed in them that guesses at its own (a nested list).
    let hint = inner.item_constraints.get().or(fallback);
    drop(inner);
    compose_runtime::CompositionLocalProvider(
        compose_ui::platform::LOCAL_CONSTRAINTS_HINT.provides(hint),
        || {
            compose_runtime::CompositionLocalProvider(
                prefetch::LOCAL_PARENT_LIST.provides(Some(parent)),
                || {
                    SubcomposeLayoutWithState!(
                        layout.clone(),
                        modifier = modifier.clone(),
                        measure_policy = measure.clone()
                    );
                },
            );
        },
    );
}
