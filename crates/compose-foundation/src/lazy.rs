//! Lazy lists, as Compose's `LazyColumn` and `LazyRow`: only the items in view (and a few
//! beyond, and the focused one) are composed, measured and drawn.
//!
//! Compose composes items while measuring (subcomposition). Here the list composes a range of
//! items, and its measurement, which fills the viewport from the scroll position, writes the
//! range it needs when the composed one falls short; the host then recomposes and measures again
//! within the same frame (`UiHost::frame`), so scrolling shows no empty frame. The items are
//! composed by a restartable composable of their own, each item one too: a scroll recomposes
//! neither the list's caller nor the items that stay.
//!
//! After a scroll, the next items in its direction are composed and measured between frames
//! (prefetch, as Compose's `LazyListPrefetchStrategy`), so the frame reaching them finds them
//! ready.
//!
//! An item that leaves is deactivated and kept for reuse, and an item that comes is composed in
//! the place of one of its content type, its nodes kept (D430), as Compose's
//! `LazyLayoutItemReusePolicy`.
//!
//! Lazy grids (`grid`) compose their items the same way, one slot per item, and lay them out
//! in lines.

mod grid;

pub use grid::{
    GridCells, GridItemSpan, LazyGridItemSpanScope, LazyGridScope, LazyGridState,
    LazyHorizontalGrid, LazyVerticalGrid, remember_lazy_grid_state,
};

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::{Rc, Weak};
use std::time::Instant;

use compose_runtime::{
    ComposeArg, ImmutableList, ObjectHandle, Owned, OwnedState, SaveableStateHolder, composable,
    hash_key, key, mutable_state_of, remember,
};
use compose_ui::{
    Alignment, Constraints, Dp, EventContext, HorizontalAlignment, IntSize, Layout, Measurable,
    MeasurePolicy, MeasureResult, MeasureScope, Modifier, ModifierNode, ModifierNodeElement,
    PaddingValues, Placeable, PointerEvent, PointerEventPass, PointerInputModifierNode,
    PrefetchRequest, PrefetchRequestScope, PrefetchScheduler, Rect, VerticalAlignment,
    platform::NodeId,
};

use crate::layout::{HorizontalArrangement, MainAxisArrangement, VerticalArrangement};
use crate::scroll::{DragScroller, bring_into_view_delta};

/// Items composed beyond the viewport on each side, ready when scrolling reaches them, and
/// placed (out of view) so focus can move to them.
const BEYOND: usize = 2;

/// The most items a measurement asks for beyond those in view, however small they may be.
const MOST_MISSING: usize = 200;

/// How far from its last index an item is looked for by key.
const KEY_WINDOW: usize = 100;

/// The most composed items looked for in the key window when the items change (D455): a
/// change's cost stays bounded (each of the others is checked where the one before it went).
const MOST_KEY_SEARCHES: usize = 8;

/// Items composed and measured between frames beyond those the layout wants, in the scroll
/// direction.
const PREFETCH_AHEAD: usize = 2;

/// Items kept composed (and measured) behind those the layout wants, the other way from the
/// scroll: a scroll reversing direction finds the first items it wants composed, rather than
/// composing them in its frame before prefetching could go the new way (D375). As many as
/// prefetching keeps ahead: the composed range is then the same whichever way the list goes.
const KEPT_BEHIND: usize = PREFETCH_AHEAD;

/// What a prefetch step leaves of the idle time beyond its estimate, at least: the estimate is
/// a guess, and a step running past the deadline delays the next frame (D341).
const PREFETCH_MARGIN_NANOS: u64 = 500_000;

/// Where a lazy list is scrolled, as Compose's `LazyListState`: the first visible item and how
/// far it is scrolled off. Reading them in composition recomposes on scrolls.
///
/// A `Copy` handle (D240): [`new`](Self::new) returns its owner, and
/// [`remember_lazy_list_state`] keeps that in the composition.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct LazyListState {
    object: ObjectHandle<LazyListInner>,
}

struct LazyListInner {
    /// The position the layout works from: the item, the pixels it is scrolled off (with
    /// their fraction, so slow scrolls add up).
    position: Cell<(usize, f32)>,
    /// The first visible item's key: the position follows it when items come and go before it.
    first_key: Cell<Option<u64>>,
    /// Scrolling asked for since the last measurement, in pixels.
    pending: Cell<f32>,
    /// A row laid out right to left (its items from the right), from the last measurement.
    rtl: Cell<bool>,
    /// Bumped by scrolls: the layout reads it, so a scroll measures the list again.
    remeasure: OwnedState<u64>,
    /// The items to compose: what the layout asks for, read by the items' composition only.
    composed: OwnedState<(usize, usize)>,
    /// What the items' composition last read of `composed` (it may compose another range: the
    /// keys it followed, D455), and a count it reads, bumped to compose `composed` again when
    /// it holds the range wanted already.
    composed_read: Cell<(usize, usize)>,
    compose_again: OwnedState<u64>,
    /// The focused item's key, kept composed and placed while it scrolls away, as Compose pins
    /// it, and the index it was last at.
    pinned: OwnedState<Option<u64>>,
    pinned_near: Cell<usize>,
    /// The list has had items: an empty layout before that keeps the initial position.
    had_items: Cell<bool>,
    /// Not laid out yet, its items' composition composed more than `composed` (the slots it
    /// kept in content reused): the layout makes them the composed range.
    composed_more: Cell<bool>,
    /// The items composed, in the children's order: written by their composition, read by the
    /// layout (a change of children measures it again).
    children: RefCell<Vec<usize>>,
    /// The position as state, for composition, and what was last written to it.
    first_index: OwnedState<usize>,
    first_offset: OwnedState<i32>,
    published: Cell<(usize, i32)>,
    /// Item sizes along the axis, as last measured: estimates for items not composed.
    sizes: RefCell<Sizes>,
    /// A grid's line sizes along its axis, as last measured (its `sizes` are its items', D455).
    line_sizes: RefCell<Sizes>,
    /// What scrollbars estimate the content from, as last laid out: the item count, the
    /// average item size with its spacing, the spacing, the viewport's length and the content
    /// padding.
    layout_info: OwnedState<(usize, i32, i32, i32, i32)>,
    /// The constraints its items are measured with, as last laid out: their composition's
    /// `LOCAL_CONSTRAINTS_HINT`.
    item_constraints: OwnedState<Option<Constraints>>,
    /// How far the list can scroll each way, as last laid out (infinite: not known).
    room_backward: Cell<f32>,
    room_forward: Cell<f32>,
    prefetch: Prefetch,
    /// The viewport's size less the content padding, for the items' `fill_parent_max_*`.
    parent_size: Rc<ParentSize>,
    /// The sticky header to keep composed (over the first visible item), as last laid out.
    sticky: OwnedState<Option<usize>>,
    /// What was last written to `sticky`, read without observing it.
    sticky_written: Cell<Option<usize>>,
}

impl LazyListInner {
    /// Asks the items' composition for the items `range`: a write of `composed`, or, when it
    /// holds `range` already (the composition composed another range, following the items'
    /// keys, D455), a count bumped so it composes `composed` again.
    fn compose(&self, range: (usize, usize)) {
        if self.composed_read.get() == range {
            self.compose_again.update(|count| count.wrapping_add(1));
        } else {
            self.composed.set(range);
        }
    }

    fn new(first_visible_item_index: usize, first_visible_item_scroll_offset: i32) -> Self {
        let offset = first_visible_item_scroll_offset.max(0);
        LazyListInner {
            position: Cell::new((first_visible_item_index, offset as f32)),
            first_key: Cell::new(None),
            pending: Cell::new(0.0),
            rtl: Cell::new(false),
            remeasure: mutable_state_of(0),
            composed: mutable_state_of((first_visible_item_index, first_visible_item_index + 1)),
            composed_read: Cell::new((first_visible_item_index, first_visible_item_index + 1)),
            compose_again: mutable_state_of(0),
            pinned: mutable_state_of(None),
            pinned_near: Cell::new(0),
            had_items: Cell::new(false),
            composed_more: Cell::new(false),
            children: RefCell::new(Vec::new()),
            first_index: mutable_state_of(first_visible_item_index),
            first_offset: mutable_state_of(offset),
            published: Cell::new((first_visible_item_index, offset)),
            sizes: RefCell::default(),
            line_sizes: RefCell::default(),
            layout_info: mutable_state_of((0, 1, 0, 0, 0)),
            item_constraints: mutable_state_of(None),
            room_backward: Cell::new(if first_visible_item_index > 0 {
                f32::INFINITY
            } else {
                offset as f32
            }),
            room_forward: Cell::new(f32::INFINITY),
            prefetch: Prefetch::default(),
            parent_size: Rc::default(),
            sticky: mutable_state_of(None),
            sticky_written: Cell::new(None),
        }
    }
}

/// What prefetching works from, as last laid out, and what it learned.
#[derive(Default)]
struct Prefetch {
    /// The list's node, its item count and the items its layout wants composed.
    node: Cell<Option<NodeId>>,
    count: Cell<usize>,
    wanted: Cell<(usize, usize)>,
    /// The last scroll went towards the end.
    forward: Cell<bool>,
    /// The request waiting in the scheduler, if any.
    request: RefCell<Weak<LazyListPrefetch>>,
    /// An item composed ahead, to measure next.
    to_measure: Cell<Option<usize>>,
    /// The items prefetching may compose and keep: from the first kept behind the wanted ones
    /// to the end of those composed ahead of them (a list's `PREFETCH_AHEAD` items each way, a
    /// grid's lines: set by each layout, so a frame trims the composed items to the same).
    limits: Cell<(usize, usize)>,
    /// Each item's constraints, where they differ by item (a grid's spans); else the list's
    /// `item_constraints`.
    constraints_of: RefCell<Option<ItemConstraints>>,
    /// How long composing and measuring an item take: an idle time too short for them is left
    /// alone, as Compose's.
    compose_cost: Cell<StepCost>,
    measure_cost: Cell<StepCost>,
}

/// An item's constraints by its index (`None`: no such item).
type ItemConstraints = Rc<dyn Fn(usize) -> Option<Constraints>>;

/// How long a prefetch step (composing an item, or measuring it) takes, in nanoseconds: a
/// running average (3/4 old + 1/4 new, as Compose's prefetch averages) and the recent maximum,
/// which loses an eighth with each step timed. A step is expected to take the larger: one item
/// much slower than the average (a cold text layout, a little core) means the next may be too,
/// and starting it on the average's word overruns the next frame (D341).
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct StepCost {
    average: u64,
    peak: u64,
    /// Steps timed (up to `SETTLED_STEPS`): an average of fewer may be one outlier.
    timed: u32,
}

/// Steps timed after which an average is the step's cost rather than its first samples.
const SETTLED_STEPS: u32 = 4;

impl StepCost {
    /// How long the next step is expected to take.
    fn estimate(self) -> u64 {
        self.average.max(self.peak)
    }

    /// With a step that took `nanos`.
    fn timed(self, nanos: u64) -> StepCost {
        let average = if self.timed == 0 { nanos } else { self.average / 4 * 3 + nanos / 4 };
        StepCost {
            average,
            peak: nanos.max(self.peak - self.peak / 8),
            timed: (self.timed + 1).min(SETTLED_STEPS),
        }
    }

    /// After the step didn't fit the time left, as the `first` step of its idle time or not.
    /// Kept if steps before it took the time: the next idle time may fit it whole (lowering it
    /// then let a 5 ms measurement start with 3 ms left after a composition, on the Pixel 8
    /// Pro). Otherwise lowered, so nothing keeps the list from prefetching for good, but a miss
    /// teaches nothing about the step's cost, so slowly where that cost is known: the recent
    /// maximum by a quarter, down to the average (an outlier fades within a few idle times);
    /// the average by a quarter while it rests on fewer than `SETTLED_STEPS` steps (it may be
    /// one outlier), by a sixteenth after (heavy frames' short idle times would otherwise
    /// wear a steady slow step's estimate down in two or three frames, until one it overruns
    /// starts it).
    fn missed(self, first: bool) -> StepCost {
        if !first {
            return self;
        }
        let average =
            if self.timed < SETTLED_STEPS { self.average / 4 * 3 } else { self.average / 16 * 15 };
        StepCost { average, peak: average.max(self.peak / 4 * 3), timed: self.timed }
    }
}

impl LazyListState {
    #[track_caller]
    pub fn new(
        first_visible_item_index: usize,
        first_visible_item_scroll_offset: i32,
    ) -> Owned<LazyListState> {
        Owned::new(
            LazyListInner::new(first_visible_item_index, first_visible_item_scroll_offset),
            |object| LazyListState { object },
        )
    }

    /// Its object, or `None` once its owner went (D240): then it does nothing, its reads
    /// giving the defaults.
    #[track_caller]
    fn inner(&self) -> Option<Rc<LazyListInner>> {
        self.object.get()
    }

    /// Its object, or once its owner went a detached one at the top, for its composition and
    /// layout (D240).
    #[track_caller]
    fn inner_or_detached(&self) -> Rc<LazyListInner> {
        self.inner().unwrap_or_else(|| Rc::new(LazyListInner::new(0, 0)))
    }

    /// The first item in view, as `firstVisibleItemIndex` (a state read).
    pub fn first_visible_item_index(&self) -> usize {
        self.inner().map_or(0, |inner| inner.first_index.get())
    }

    /// How many pixels of it are scrolled off, as `firstVisibleItemScrollOffset` (a state read).
    pub fn first_visible_item_scroll_offset(&self) -> i32 {
        self.inner().map_or(0, |inner| inner.first_offset.get())
    }

    /// Scrolls to `index` (and `scroll_offset` pixels into it) at the next layout, as
    /// `scrollToItem`, without animation.
    pub fn scroll_to_item(&self, index: usize, scroll_offset: i32) {
        let Some(inner) = self.inner() else { return };
        inner.position.set((index, scroll_offset.max(0) as f32));
        inner.first_key.set(None);
        inner.pending.set(0.0);
        inner.room_backward.set(f32::INFINITY);
        inner.room_forward.set(f32::INFINITY);
        inner.remeasure.update(|count| count.wrapping_add(1));
    }

    /// Scrolls by `pixels` (positive: towards the end) at the next layout, as
    /// `scrollBy`/`dispatchRawDelta`; returns whether the list can move that way.
    pub fn scroll_by(&self, pixels: f32) -> bool {
        self.dispatch(pixels) != 0.0
    }

    /// Scrolls by up to `pixels`, as far as the list goes (as last laid out); returns the
    /// pixels it will move.
    fn dispatch(&self, pixels: f32) -> f32 {
        let Some(inner) = self.inner() else { return 0.0 };
        let used = if pixels > 0.0 {
            pixels.min(inner.room_forward.get().max(0.0))
        } else {
            pixels.max(-inner.room_backward.get().max(0.0))
        };
        if used != 0.0 {
            inner.room_forward.set(inner.room_forward.get() - used);
            inner.room_backward.set(inner.room_backward.get() + used);
            inner.pending.set(inner.pending.get() + used);
            // Prefetch goes the way the list was scrolled (not the way a layout steps back).
            inner.prefetch.forward.set(used > 0.0);
            inner.remeasure.update(|count| count.wrapping_add(1));
        }
        used
    }

    /// The item count, the average item size (with its spacing), the spacing, the viewport's
    /// length and the content padding along the axis, as last laid out: a scrollbar's
    /// estimates (a state read).
    pub(crate) fn layout_info(&self) -> (usize, i32, i32, i32, i32) {
        self.inner().map_or((0, 1, 0, 0, 0), |inner| inner.layout_info.get())
    }

    /// Whether the list can scroll towards its end, as `canScrollForward` (as last laid out).
    pub fn can_scroll_forward(&self) -> bool {
        self.inner().is_some_and(|inner| inner.room_forward.get() > 0.0)
    }

    /// Whether the list can scroll towards its start, as `canScrollBackward`.
    pub fn can_scroll_backward(&self) -> bool {
        self.inner().is_some_and(|inner| inner.room_backward.get() > 0.0)
    }
}

impl ComposeArg for LazyListState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl std::fmt::Debug for LazyListState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.object.try_get() {
            Some(inner) => {
                let (index, offset) = inner.position.get();
                write!(formatter, "LazyListState({index}, {offset})")
            }
            None => formatter.write_str("LazyListState(freed)"),
        }
    }
}

impl LazyListState {
    /// Saves the first visible item and its offset, as Compose's `LazyListState.Saver`.
    pub fn saver() -> impl compose_runtime::Saver<Owned<LazyListState>> {
        use compose_runtime::Saveable;
        compose_runtime::saver(
            |state: &Owned<LazyListState>| {
                let (index, offset) = state.inner()?.position.get();
                Some((index, offset.round() as i32).save())
            },
            |saved| {
                <(usize, i32)>::restore(saved)
                    .map(|(index, offset)| LazyListState::new(index, offset))
            },
        )
    }
}

/// A lazy list state kept across recompositions and saved across the host's recreation, as
/// `rememberLazyListState()` (a `rememberSaveable`).
#[composable(non_restartable)]
pub fn remember_lazy_list_state(
    #[default(0)] initial_first_visible_item_index: usize,
    #[default(0)] initial_first_visible_item_scroll_offset: i32,
) -> LazyListState {
    compose_runtime::remember_saveable_with(LazyListState::saver(), || {
        LazyListState::new(
            initial_first_visible_item_index,
            initial_first_visible_item_scroll_offset,
        )
    })
}

/// A run of items: their count, keys, content types and content.
struct Interval {
    count: usize,
    key: Option<Rc<dyn Fn(usize) -> u64>>,
    /// The items' content types, hashed (`None`: Compose's default `{ null }`).
    content_type: Option<ContentType>,
    /// The item's full key, to tell a duplicate key from a hash collision (debug builds).
    #[cfg(debug_assertions)]
    full_key: Option<FullKey>,
    /// A sticky header (`sticky_header`).
    sticky: bool,
    /// A grid's items' spans (`None`: one cell each, Compose's default).
    span: Option<grid::SpanOf>,
    content: Rc<dyn Fn(usize)>,
}

/// An item's content type, hashed, by its index in its interval.
type ContentType = Rc<dyn Fn(usize) -> u64>;

/// An item's key, by its index in its interval.
#[cfg(debug_assertions)]
type FullKey = Rc<dyn Fn(usize) -> Box<dyn DynKey>>;

/// A key compared through `dyn Any`, for the debug check of duplicate keys.
#[cfg(debug_assertions)]
trait DynKey: std::any::Any {
    fn equals(&self, other: &dyn DynKey) -> bool;
    fn as_any(&self) -> &dyn std::any::Any;
}

#[cfg(debug_assertions)]
impl<K: PartialEq + 'static> DynKey for K {
    fn equals(&self, other: &dyn DynKey) -> bool {
        other.as_any().downcast_ref::<K>().is_some_and(|other| other == self)
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Interval {
    fn new(count: usize, content: Rc<dyn Fn(usize)>) -> Interval {
        Interval {
            count,
            key: None,
            content_type: None,
            #[cfg(debug_assertions)]
            full_key: None,
            sticky: false,
            span: None,
            content,
        }
    }

    /// With the items' content types.
    fn typed(mut self, content_type: ContentType) -> Interval {
        self.content_type = Some(content_type);
        self
    }

    fn keyed<K: Hash + PartialEq + 'static>(
        count: usize,
        key: impl Fn(usize) -> K + 'static,
        content: Rc<dyn Fn(usize)>,
    ) -> Interval {
        let key = Rc::new(key);
        Interval {
            count,
            #[cfg(debug_assertions)]
            full_key: Some({
                let key = key.clone();
                Rc::new(move |index| Box::new(key(index)) as Box<dyn DynKey>)
            }),
            key: Some(Rc::new(move |index| hash_key(&key(index)))),
            content_type: None,
            sticky: false,
            span: None,
            content,
        }
    }
}

/// A content type, hashed (a collision only lets items of two types reuse each other's slots).
fn content_type_of<C: Hash>(content_type: &C) -> u64 {
    hash_key(content_type)
}

/// What a lazy list's content declares its items with, as Compose's `LazyListScope`:
/// `scope.item(|| …)`, `scope.items(count, |index| …)`, `scope.items_keyed(count, key, …)`, and
/// over a list, `scope.items_list(list, |item_scope, item| …)` and
/// `scope.items_indexed(list, |item_scope, index, item| …)`.
///
/// Each form has a `_with` variant taking Compose's `contentType` too, after the key, as in
/// Compose's parameters (`items_keyed_with(count, key, content_type, …)`): an item composed
/// where none was reuses the composition and nodes of an item of its content type that left
/// (D430, D432). The forms without take Compose's default, `{ null }`: such items reuse each
/// other's.
#[derive(Clone, Default)]
pub struct LazyListScope {
    intervals: Rc<RefCell<Vec<Interval>>>,
    item_scope: LazyItemScope,
}

impl LazyListScope {
    /// One item, as `item { … }`.
    pub fn item(&self, content: impl Fn() + 'static) {
        self.push(Interval::new(1, Rc::new(move |_| content())));
    }

    /// One item of `content_type`, as `item(contentType = …) { … }`.
    pub fn item_with<C: Hash>(&self, content_type: C, content: impl Fn() + 'static) {
        let content_type = content_type_of(&content_type);
        self.push(
            Interval::new(1, Rc::new(move |_| content())).typed(Rc::new(move |_| content_type)),
        );
    }

    /// One item with a key, as `item(key) { … }`.
    pub fn item_keyed<K: Hash + PartialEq + Clone + 'static>(
        &self,
        key: K,
        content: impl Fn() + 'static,
    ) {
        self.push(Interval::keyed(1, move |_| key.clone(), Rc::new(move |_| content())));
    }

    /// One item with a key and a content type, as `item(key, contentType) { … }`.
    pub fn item_keyed_with<K: Hash + PartialEq + Clone + 'static, C: Hash>(
        &self,
        key: K,
        content_type: C,
        content: impl Fn() + 'static,
    ) {
        let content_type = content_type_of(&content_type);
        self.push(
            Interval::keyed(1, move |_| key.clone(), Rc::new(move |_| content()))
                .typed(Rc::new(move |_| content_type)),
        );
    }

    /// `count` items, as `items(count) { index -> … }`. Without keys, an item's state goes with
    /// its index.
    pub fn items(&self, count: usize, content: impl Fn(usize) + 'static) {
        self.push(Interval::new(count, Rc::new(content)));
    }

    /// `count` items with their content types, as `items(count, contentType = { … }) { … }`.
    pub fn items_with<C: Hash>(
        &self,
        count: usize,
        content_type: impl Fn(usize) -> C + 'static,
        content: impl Fn(usize) + 'static,
    ) {
        self.push(
            Interval::new(count, Rc::new(content))
                .typed(Rc::new(move |index| content_type_of(&content_type(index)))),
        );
    }

    /// `count` items with keys, as `items(count, key = { … }) { … }`: an item keeps its state
    /// (and the scroll position follows the first visible one) when items before it come and
    /// go. Keys must be unique in the list (debug builds check those composed).
    pub fn items_keyed<K: Hash + PartialEq + 'static>(
        &self,
        count: usize,
        key: impl Fn(usize) -> K + 'static,
        content: impl Fn(usize) + 'static,
    ) {
        self.push(Interval::keyed(count, key, Rc::new(content)));
    }

    /// `count` items with keys and content types, as
    /// `items(count, key = { … }, contentType = { … }) { … }`.
    pub fn items_keyed_with<K: Hash + PartialEq + 'static, C: Hash>(
        &self,
        count: usize,
        key: impl Fn(usize) -> K + 'static,
        content_type: impl Fn(usize) -> C + 'static,
        content: impl Fn(usize) + 'static,
    ) {
        self.push(
            Interval::keyed(count, key, Rc::new(content))
                .typed(Rc::new(move |index| content_type_of(&content_type(index)))),
        );
    }

    /// An item for each of `items`, as `items(items) { item -> … }`: `items` is a `Vec`, a
    /// slice or an array (copied), or an `ImmutableList` (shared).
    pub fn items_list<T: 'static>(
        &self,
        items: impl Into<ImmutableList<T>>,
        content: impl Fn(LazyItemScope, &T) + 'static,
    ) {
        let (items, scope) = (items.into(), self.item_scope.clone());
        let count = items.len();
        self.push(Interval::new(
            count,
            Rc::new(move |index| content(scope.clone(), &items[index])),
        ));
    }

    /// `items_list` with content types, as `items(items, contentType = { item -> … })`.
    pub fn items_list_with<T: 'static, C: Hash>(
        &self,
        items: impl Into<ImmutableList<T>>,
        content_type: impl Fn(&T) -> C + 'static,
        content: impl Fn(LazyItemScope, &T) + 'static,
    ) {
        let (items, scope) = (items.into(), self.item_scope.clone());
        let types = items.clone();
        self.push(
            Interval::new(items.len(), Rc::new(move |index| content(scope.clone(), &items[index])))
                .typed(Rc::new(move |index| content_type_of(&content_type(&types[index])))),
        );
    }

    /// `items_list` with keys, as `items(items, key = { item -> … })`.
    pub fn items_list_keyed<T: 'static, K: Hash + PartialEq + 'static>(
        &self,
        items: impl Into<ImmutableList<T>>,
        key: impl Fn(&T) -> K + 'static,
        content: impl Fn(LazyItemScope, &T) + 'static,
    ) {
        let (items, scope) = (items.into(), self.item_scope.clone());
        let keys = items.clone();
        self.push(Interval::keyed(
            items.len(),
            move |index| key(&keys[index]),
            Rc::new(move |index| content(scope.clone(), &items[index])),
        ));
    }

    /// `items_list` with keys and content types, as
    /// `items(items, key = { … }, contentType = { … })`.
    pub fn items_list_keyed_with<T: 'static, K: Hash + PartialEq + 'static, C: Hash>(
        &self,
        items: impl Into<ImmutableList<T>>,
        key: impl Fn(&T) -> K + 'static,
        content_type: impl Fn(&T) -> C + 'static,
        content: impl Fn(LazyItemScope, &T) + 'static,
    ) {
        let (items, scope) = (items.into(), self.item_scope.clone());
        let (keys, types) = (items.clone(), items.clone());
        self.push(
            Interval::keyed(
                items.len(),
                move |index| key(&keys[index]),
                Rc::new(move |index| content(scope.clone(), &items[index])),
            )
            .typed(Rc::new(move |index| content_type_of(&content_type(&types[index])))),
        );
    }

    /// An item for each of `items`, with its index, as `itemsIndexed(items) { index, item -> … }`.
    pub fn items_indexed<T: 'static>(
        &self,
        items: impl Into<ImmutableList<T>>,
        content: impl Fn(LazyItemScope, usize, &T) + 'static,
    ) {
        let (items, scope) = (items.into(), self.item_scope.clone());
        self.push(Interval::new(
            items.len(),
            Rc::new(move |index| content(scope.clone(), index, &items[index])),
        ));
    }

    /// `items_indexed` with content types, as
    /// `itemsIndexed(items, contentType = { index, item -> … })`.
    pub fn items_indexed_with<T: 'static, C: Hash>(
        &self,
        items: impl Into<ImmutableList<T>>,
        content_type: impl Fn(usize, &T) -> C + 'static,
        content: impl Fn(LazyItemScope, usize, &T) + 'static,
    ) {
        let (items, scope) = (items.into(), self.item_scope.clone());
        let types = items.clone();
        self.push(
            Interval::new(
                items.len(),
                Rc::new(move |index| content(scope.clone(), index, &items[index])),
            )
            .typed(Rc::new(move |index| content_type_of(&content_type(index, &types[index])))),
        );
    }

    /// `items_indexed` with keys, as `itemsIndexed(items, key = { index, item -> … })`.
    pub fn items_indexed_keyed<T: 'static, K: Hash + PartialEq + 'static>(
        &self,
        items: impl Into<ImmutableList<T>>,
        key: impl Fn(usize, &T) -> K + 'static,
        content: impl Fn(LazyItemScope, usize, &T) + 'static,
    ) {
        let (items, scope) = (items.into(), self.item_scope.clone());
        let keys = items.clone();
        self.push(Interval::keyed(
            items.len(),
            move |index| key(index, &keys[index]),
            Rc::new(move |index| content(scope.clone(), index, &items[index])),
        ));
    }

    /// `items_indexed` with keys and content types, as
    /// `itemsIndexed(items, key = { … }, contentType = { … })`.
    pub fn items_indexed_keyed_with<T: 'static, K: Hash + PartialEq + 'static, C: Hash>(
        &self,
        items: impl Into<ImmutableList<T>>,
        key: impl Fn(usize, &T) -> K + 'static,
        content_type: impl Fn(usize, &T) -> C + 'static,
        content: impl Fn(LazyItemScope, usize, &T) + 'static,
    ) {
        let (items, scope) = (items.into(), self.item_scope.clone());
        let (keys, types) = (items.clone(), items.clone());
        self.push(
            Interval::keyed(
                items.len(),
                move |index| key(index, &keys[index]),
                Rc::new(move |index| content(scope.clone(), index, &items[index])),
            )
            .typed(Rc::new(move |index| content_type_of(&content_type(index, &types[index])))),
        );
    }

    /// A header that sticks to the list's start while the items after it (up to the next
    /// header) scroll under it, as `stickyHeader { … }`.
    pub fn sticky_header(&self, content: impl Fn(LazyItemScope) + 'static) {
        self.push_sticky(None, None, Rc::new(content));
    }

    /// `sticky_header` with a content type, as `stickyHeader(contentType = …) { … }`.
    pub fn sticky_header_with<C: Hash>(
        &self,
        content_type: C,
        content: impl Fn(LazyItemScope) + 'static,
    ) {
        self.push_sticky(None, Some(content_type_of(&content_type)), Rc::new(content));
    }

    /// `sticky_header` with a key, as `stickyHeader(key) { … }`.
    pub fn sticky_header_keyed<K: Hash + PartialEq + Clone + 'static>(
        &self,
        key: K,
        content: impl Fn(LazyItemScope) + 'static,
    ) {
        let keyed = Interval::keyed(1, move |_| key.clone(), Rc::new(|_| {}));
        self.push_sticky(Some(keyed), None, Rc::new(content));
    }

    /// `sticky_header` with a key and a content type, as `stickyHeader(key, contentType) { … }`.
    pub fn sticky_header_keyed_with<K: Hash + PartialEq + Clone + 'static, C: Hash>(
        &self,
        key: K,
        content_type: C,
        content: impl Fn(LazyItemScope) + 'static,
    ) {
        let keyed = Interval::keyed(1, move |_| key.clone(), Rc::new(|_| {}));
        self.push_sticky(Some(keyed), Some(content_type_of(&content_type)), Rc::new(content));
    }

    /// The scope of this list's items, for the forms whose content doesn't receive it (`item`,
    /// `items`): what `fill_parent_max_width` and friends take.
    pub fn item_scope(&self) -> LazyItemScope {
        self.item_scope.clone()
    }

    fn push(&self, interval: Interval) {
        self.intervals.borrow_mut().push(interval);
    }

    /// A sticky header: its key (an interval of one item with the key, its content replaced),
    /// its content type and its content. Not generic: the shims above are.
    fn push_sticky(
        &self,
        keyed: Option<Interval>,
        content_type: Option<u64>,
        content: Rc<dyn Fn(LazyItemScope)>,
    ) {
        let scope = self.item_scope.clone();
        let content: Rc<dyn Fn(usize)> = Rc::new(move |_| content(scope.clone()));
        let mut interval = match keyed {
            Some(mut interval) => {
                interval.content = content;
                interval
            }
            None => Interval::new(1, content),
        };
        if let Some(content_type) = content_type {
            interval = interval.typed(Rc::new(move |_| content_type));
        }
        interval.sticky = true;
        self.push(interval);
    }
}

/// An unkeyed item's key, by its index: a private type no user key equals (as Compose's
/// `DefaultLazyKey`), its hash salted apart from the index's own.
#[derive(PartialEq)]
struct DefaultKey(usize);

impl Hash for DefaultKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(0x6C61_7A79_5F6B_6579);
        self.0.hash(state);
    }
}

impl ComposeArg for LazyListScope {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.intervals, &other.intervals)
    }
}

/// The scope of a lazy list's items, as Compose's `LazyItemScope`: what sizes an item to the
/// list's viewport (`Modifier.fill_parent_max_width(&scope)`, from `LazyItemModifiers`).
#[derive(Clone, Default)]
pub struct LazyItemScope {
    parent_size: Rc<ParentSize>,
}

/// A list's viewport size less its content padding, as last measured: written by the list's
/// measurement before it measures its items, whose `fill_parent_max_*` read it.
struct ParentSize {
    size: OwnedState<IntSize>,
    /// What was last written, read without observing it.
    written: Cell<IntSize>,
}

impl Default for ParentSize {
    fn default() -> ParentSize {
        let unbounded = IntSize::new(Constraints::INFINITY, Constraints::INFINITY);
        ParentSize { size: mutable_state_of(unbounded), written: Cell::new(unbounded) }
    }
}

impl ComposeArg for LazyItemScope {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.parent_size, &other.parent_size)
    }
}

/// The modifiers of a lazy list's items, as `LazyItemScope`'s: `Modifier` methods taking the
/// item scope.
pub trait LazyItemModifiers {
    /// As wide as the list's viewport (less its content padding), as `fillParentMaxWidth()`.
    fn fill_parent_max_width(self, scope: &LazyItemScope) -> Modifier;
    /// As tall as the list's viewport, as `fillParentMaxHeight()`.
    fn fill_parent_max_height(self, scope: &LazyItemScope) -> Modifier;
    /// As large as the list's viewport, as `fillParentMaxSize()`.
    fn fill_parent_max_size(self, scope: &LazyItemScope) -> Modifier;
    /// `fraction` of the viewport's width, as `fillParentMaxWidth(fraction)`.
    fn fill_parent_max_width_with(self, scope: &LazyItemScope, fraction: f32) -> Modifier;
    fn fill_parent_max_height_with(self, scope: &LazyItemScope, fraction: f32) -> Modifier;
    fn fill_parent_max_size_with(self, scope: &LazyItemScope, fraction: f32) -> Modifier;
}

impl LazyItemModifiers for Modifier {
    fn fill_parent_max_width(self, scope: &LazyItemScope) -> Modifier {
        self.fill_parent_max_width_with(scope, 1.0)
    }
    fn fill_parent_max_height(self, scope: &LazyItemScope) -> Modifier {
        self.fill_parent_max_height_with(scope, 1.0)
    }
    fn fill_parent_max_size(self, scope: &LazyItemScope) -> Modifier {
        self.fill_parent_max_size_with(scope, 1.0)
    }
    fn fill_parent_max_width_with(self, scope: &LazyItemScope, fraction: f32) -> Modifier {
        self.then_custom(ParentSizeElement {
            parent: scope.parent_size.clone(),
            width: Some(fraction),
            height: None,
        })
    }
    fn fill_parent_max_height_with(self, scope: &LazyItemScope, fraction: f32) -> Modifier {
        self.then_custom(ParentSizeElement {
            parent: scope.parent_size.clone(),
            width: None,
            height: Some(fraction),
        })
    }
    fn fill_parent_max_size_with(self, scope: &LazyItemScope, fraction: f32) -> Modifier {
        self.then_custom(ParentSizeElement {
            parent: scope.parent_size.clone(),
            width: Some(fraction),
            height: Some(fraction),
        })
    }
}

/// `fill_parent_max_*`: the fractions of the viewport's size to take, as Compose's
/// `ParentSizeElement` (its node too).
#[derive(Clone)]
struct ParentSizeElement {
    parent: Rc<ParentSize>,
    width: Option<f32>,
    height: Option<f32>,
}

impl ModifierNodeElement for ParentSizeElement {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(self.clone())
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<ParentSizeElement>() {
            *node = self.clone();
        }
    }
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<ParentSizeElement>().is_some_and(|other| {
            Rc::ptr_eq(&other.parent, &self.parent)
                && other.width == self.width
                && other.height == self.height
        })
    }
    fn name(&self) -> &'static str {
        match (self.width, self.height) {
            (Some(_), Some(_)) => "fill_parent_max_size",
            (Some(_), None) => "fill_parent_max_width",
            _ => "fill_parent_max_height",
        }
    }
}

impl ModifierNode for ParentSizeElement {
    fn as_layout(&mut self) -> Option<&mut dyn compose_ui::LayoutModifierNode> {
        Some(self)
    }
}

impl compose_ui::LayoutModifierNode for ParentSizeElement {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        // Observed: a new viewport size measures the item again.
        let parent = self.parent.size.get();
        let size = |fraction: Option<f32>, parent: i32| match fraction {
            Some(fraction) if parent != Constraints::INFINITY => {
                Some(compose_ui::platform::round_to_int(parent as f32 * fraction).max(0))
            }
            _ => None,
        };
        let width = size(self.width, parent.width);
        let height = size(self.height, parent.height);
        let child = Constraints::new(
            width.unwrap_or(constraints.min_width),
            width.unwrap_or(constraints.max_width),
            height.unwrap_or(constraints.min_height),
            height.unwrap_or(constraints.max_height),
        );
        let placeable = scope.measure(content, child);
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    }
}

/// The items a list's content declared, for one composition of the list.
struct Items {
    intervals: Vec<Interval>,
    /// Each interval's first item: a binary search finds an item's interval.
    starts: Vec<usize>,
    count: usize,
    /// The sticky headers' indices, in order.
    headers: Vec<usize>,
    /// Some items have spans (a grid's): its lines are found by walking the items.
    has_custom_spans: bool,
}

impl Items {
    fn new(intervals: Vec<Interval>) -> Items {
        let mut starts = Vec::with_capacity(intervals.len());
        let mut headers = Vec::new();
        let mut count = 0;
        let has_custom_spans = intervals.iter().any(|interval| interval.span.is_some());
        for interval in &intervals {
            starts.push(count);
            if interval.sticky {
                headers.extend(count..count + interval.count);
            }
            count += interval.count;
        }
        Items { intervals, starts, count, headers, has_custom_spans }
    }

    /// The interval holding item `index`, and the item's index in it.
    fn find(&self, index: usize) -> Option<(&Interval, usize)> {
        if index >= self.count {
            return None;
        }
        let interval = self.starts.partition_point(|start| *start <= index) - 1;
        Some((&self.intervals[interval], index - self.starts[interval]))
    }

    /// The item with `key`, looked for within a window around `near` (as Compose's
    /// `NearestRangeKeyIndexMap`): an item moved further than that is lost.
    fn index_of(&self, key: u64, near: usize) -> Option<usize> {
        let start = near.saturating_sub(KEY_WINDOW);
        let end = (near + KEY_WINDOW + 1).min(self.count);
        (start..end).find(|candidate| self.key(*candidate) == key)
    }

    /// The item's content type, hashed (`None`, Compose's `null`, if its interval has none).
    fn content_type(&self, index: usize) -> Option<u64> {
        let (interval, local) = self.find(index)?;
        interval.content_type.as_ref().map(|content_type| content_type(local))
    }

    fn key(&self, index: usize) -> u64 {
        match self.find(index) {
            Some((interval, local)) => {
                interval.key.as_ref().map_or(hash_key(&DefaultKey(index)), |key| key(local))
            }
            None => hash_key(&DefaultKey(index)),
        }
    }

    /// The sticky header at or before `index`, and the next header after it, if any.
    fn headers_around(&self, index: usize) -> Option<(usize, Option<usize>)> {
        let after = self.headers.partition_point(|header| *header <= index);
        let header = *self.headers.get(after.checked_sub(1)?)?;
        Some((header, self.headers.get(after).copied()))
    }

    /// Panics if two of `indices` have the same key, as Compose's lazy layouts do (a hash
    /// collision of different keys too, which would share their state): debug builds only.
    #[cfg(debug_assertions)]
    fn check_keys(&self, indices: &[usize]) {
        let full_key = |index: usize| -> Box<dyn DynKey> {
            match self.find(index) {
                Some((Interval { full_key: Some(key), .. }, local)) => key(local),
                _ => Box::new(DefaultKey(index)),
            }
        };
        let mut seen: HashMap<u64, usize> = HashMap::with_capacity(indices.len());
        for &index in indices {
            if let Some(&other) = seen.get(&self.key(index)) {
                if full_key(index).equals(&*full_key(other)) {
                    panic!(
                        "compose-foundation: items {other} and {index} of a lazy list have the \
                         same key: keys must be unique in the list"
                    );
                }
                panic!(
                    "compose-foundation: the keys of items {other} and {index} of a lazy list \
                     differ but hash to the same value: they would share their state"
                );
            }
            seen.insert(self.key(index), index);
        }
    }
}

/// The most deactivated items kept for reuse per content type, as Compose's
/// `MaxItemsToRetainForReuse`.
const MAX_ITEMS_TO_RETAIN_FOR_REUSE: usize = 7;

/// No slot yet.
const NO_SLOT: u64 = 0;

/// A slot an item is composed in, as the subcomposition Compose's lazy layouts compose each item
/// in: a keyed group of the items' composition holding one item at a time. When its item leaves,
/// its content is deactivated and kept for an item of the same content type to reuse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ItemSlot {
    /// Its group's key: the same whatever item it holds.
    id: u64,
    /// The key of the item it holds, or held last.
    key: u64,
    /// That item's content type, hashed (`None`: Compose's `null`).
    content_type: Option<u64>,
    /// That item's index, as last composed.
    index: usize,
}

/// What a slot holds in a pass: an item (by index), or its content deactivated, kept for reuse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SlotContent {
    Item(usize),
    Deactivated,
}

/// The key of a slot's reusable group (its data key is its item's key).
const SLOT_GROUP: u64 = compose_runtime::key_of("compose_foundation::LazyItemSlot");

/// The data key of a slot's reusable group while it holds no item.
const NO_ITEM: u64 = compose_runtime::key_of("compose_foundation::LazyItemSlot::none");

/// A slot's content, as the content of the subcomposition Compose's lazy layouts compose each
/// item in (`ReusableContentHost(active) { item }`, reused from its root when another item
/// takes it): a reusable group whose data key is its item's key, so another item reuses it,
/// or `NO_ITEM` while it's kept for reuse, its content deactivated (D430). One group where
/// Compose has two (the host, and `ReusableContent(key)` around the item): a slot passes through
/// both states the same way.
fn compose_slot(
    slot: ItemSlot,
    content: SlotContent,
    state: LazyListState,
    items: &ItemsRef,
    holder: SaveableStateHolder,
) {
    let composer = compose_runtime::platform::current();
    match content {
        SlotContent::Item(index) => {
            composer.start_reusable_group(SLOT_GROUP, slot.key);
            LazyItem(index, state, items.clone(), holder);
        }
        SlotContent::Deactivated => {
            // Its data key changes as its item leaves: the composer deactivates its content
            // then (content being reused), and skips it after.
            composer.start_reusable_group(SLOT_GROUP, NO_ITEM);
            composer.deactivate_to_end_group(false);
        }
    }
    composer.end_reusable_group();
}

/// Which slot holds which item, and the slots kept for reuse: what Compose's
/// `SubcomposeLayoutState` keeps of a lazy layout's items (its `slotIdToNode` and reusable
/// nodes).
#[derive(Default)]
struct ItemSlots {
    /// The slots holding items, in the order last composed.
    active: Vec<ItemSlot>,
    /// The slots kept for reuse, their content deactivated, oldest first.
    reusable: Vec<ItemSlot>,
    /// The last slot id given.
    last_id: u64,
    /// The items the active slots were composed from.
    items: Weak<Items>,
}

impl ItemSlots {
    /// When the items changed since the last pass (the list's caller composed again), where the
    /// `composed` range's items went: found by key (each first where the one before it moved by
    /// as much, else within the key window around where it was, `MOST_KEY_SEARCHES` times at
    /// most), the range moved by as much as the first found, and widened to hold those that
    /// moved otherwise but stayed near it (items inserted or removed among them). Those items
    /// stay composed, rather than leaving (deactivated, their state lost) for a measurement to
    /// ask for them again, as Compose's measurement finds each item's composition by its key
    /// (D453, D455). `None` if the items are the same, none was found, or the range is the
    /// same.
    fn follow(&self, items: &Rc<Items>, composed: (usize, usize)) -> Option<(usize, usize)> {
        if std::ptr::eq(self.items.as_ptr(), Rc::as_ptr(items)) {
            return None;
        }
        let (start, end) = composed;
        let mut shift = 0isize;
        let mut first_shift = None;
        let mut searches = 0;
        let mut found = Vec::new();
        for slot in &self.active {
            let old = slot.index;
            // A pinned item or a header out of the range: kept by key already.
            if old < start || old >= end {
                continue;
            }
            let guess = old.checked_add_signed(shift).filter(|guess| *guess < items.count);
            let index = match guess {
                Some(guess) if items.key(guess) == slot.key => Some(guess),
                _ if searches < MOST_KEY_SEARCHES => {
                    searches += 1;
                    items.index_of(slot.key, old)
                }
                _ => None,
            };
            if let Some(index) = index {
                shift = index as isize - old as isize;
                first_shift.get_or_insert(shift);
                found.push(index);
            }
        }
        let first_shift = first_shift?;
        let moved = |index: usize| index.saturating_add_signed(first_shift).min(items.count);
        let (moved_start, moved_end) = (moved(start), moved(end));
        let near = (end - start).max(1);
        let (mut low, mut high) = (moved_start, moved_end);
        for index in found {
            if index + near >= moved_start && index < moved_end + near {
                low = low.min(index);
                high = high.max(index + 1);
            }
        }
        let followed = (low, high.min(items.count));
        (followed != composed).then_some(followed)
    }

    /// Gives each item of `indices` a slot, and returns them in order: the slot holding the item
    /// already; else a slot kept for reuse, the item's own if it's there (Compose's search for
    /// the same slot id first), else the newest of its content type; else a new one. The slots
    /// of the items that left are kept for reuse first, so an item coming in reuses one going
    /// out in the same pass (where Compose deactivates it after the measurement, for the next
    /// one to reuse). Returns the slots to compose: those of `indices`, each with its item, then
    /// those kept for reuse.
    ///
    /// A slot whose item leaves without ever having been measured (`measured` of its index is
    /// false) is disposed rather than kept: a first layout's estimate composes items it then
    /// finds it doesn't want (the stress list's: 87 items, its rows and carousels' tiles, of
    /// which 38 stay, from the size of its 36 dp header), which Compose, composing items as its
    /// measurement reaches them, never composes; kept, they would fill its pool with items no
    /// scroll brought, and keep it full (D460).
    fn assign(
        &mut self,
        items: &Items,
        indices: &[usize],
        measured: impl Fn(usize) -> bool,
    ) -> Vec<(ItemSlot, SlotContent)> {
        let previous: HashMap<u64, usize> =
            self.active.iter().enumerate().map(|(position, slot)| (slot.key, position)).collect();
        let mut taken = vec![false; self.active.len()];
        let mut assigned = Vec::with_capacity(indices.len());
        for &index in indices {
            let (key, content_type) = (items.key(index), items.content_type(index));
            let slot = match previous.get(&key) {
                Some(&position) if !taken[position] => {
                    taken[position] = true;
                    ItemSlot { content_type, index, ..self.active[position] }
                }
                _ => ItemSlot { id: NO_SLOT, key, content_type, index },
            };
            assigned.push((index, slot));
        }
        for (position, slot) in self.active.iter().enumerate() {
            if !taken[position] && measured(slot.index) {
                self.reusable.push(*slot);
            }
        }
        for (_, slot) in &mut assigned {
            if slot.id == NO_SLOT {
                slot.id = match self.take_reusable(slot.key, slot.content_type) {
                    Some(id) => id,
                    None => {
                        self.last_id += 1;
                        self.last_id
                    }
                };
            }
        }
        self.active = assigned.iter().map(|(_, slot)| *slot).collect();
        LazyLayoutItemReusePolicy::get_slots_to_retain(&mut self.reusable);
        let mut composed: Vec<(ItemSlot, SlotContent)> =
            assigned.into_iter().map(|(index, slot)| (slot, SlotContent::Item(index))).collect();
        composed.extend(self.reusable.iter().map(|slot| (*slot, SlotContent::Deactivated)));
        composed
    }

    /// Takes a slot kept for reuse for the item with `key` and `content_type`: the one it was
    /// in, else the newest compatible one (as Compose's `takeNodeFromReusables`).
    fn take_reusable(&mut self, key: u64, content_type: Option<u64>) -> Option<u64> {
        let position = (self.reusable.iter().rposition(|slot| slot.key == key)).or_else(|| {
            self.reusable.iter().rposition(|slot| {
                LazyLayoutItemReusePolicy::are_compatible(content_type, slot.content_type)
            })
        })?;
        Some(self.reusable.remove(position).id)
    }
}

/// Which items that left are kept for reuse, and which item may reuse which, as Compose's
/// `LazyLayoutItemReusePolicy` (a `SubcomposeSlotReusePolicy`): up to
/// `MAX_ITEMS_TO_RETAIN_FOR_REUSE` of each content type; an item reuses one of its content type
/// (`null`, here `None`, being one). Compose's is an object of the item content factory; here
/// the slots know their items' content types, so it holds nothing.
enum LazyLayoutItemReusePolicy {}

impl LazyLayoutItemReusePolicy {
    /// Keeps the newest `MAX_ITEMS_TO_RETAIN_FOR_REUSE` of each content type among `reusable`
    /// (oldest first), as `getSlotsToRetain`; the others are disposed.
    fn get_slots_to_retain(reusable: &mut Vec<ItemSlot>) {
        if reusable.len() <= MAX_ITEMS_TO_RETAIN_FOR_REUSE {
            return;
        }
        let mut count_per_type: Vec<(Option<u64>, usize)> = Vec::new();
        let mut retain = vec![false; reusable.len()];
        for (position, slot) in reusable.iter().enumerate().rev() {
            let count = match count_per_type.iter_mut().find(|(kind, _)| *kind == slot.content_type)
            {
                Some((_, count)) => count,
                None => {
                    count_per_type.push((slot.content_type, 0));
                    &mut count_per_type.last_mut().expect("just pushed").1
                }
            };
            if *count < MAX_ITEMS_TO_RETAIN_FOR_REUSE {
                *count += 1;
                retain[position] = true;
            }
        }
        let mut position = 0;
        reusable.retain(|_| {
            position += 1;
            retain[position - 1]
        });
    }

    /// Whether an item of `content_type` may reuse the slot of one of `reusable_content_type`, as
    /// `areCompatible`: the same content type.
    fn are_compatible(content_type: Option<u64>, reusable_content_type: Option<u64>) -> bool {
        content_type == reusable_content_type
    }
}

/// The items, compared by identity (a new set each time the list's caller recomposes).
#[derive(Clone)]
struct ItemsRef(Rc<Items>);

impl ComposeArg for ItemsRef {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// What the layout needs from composition: a new value is a new policy, which measures the
/// list again.
#[derive(Clone)]
struct ListInfo {
    items: Rc<Items>,
    vertical: bool,
    padding: PaddingValues,
    /// Laid out from the end: the first item at the bottom (the right of a row).
    reverse: bool,
    arrangement: MainAxisArrangement,
    /// The items' alignment across the list: its horizontal bias for a column, its vertical
    /// one for a row.
    alignment: Alignment,
}

impl PartialEq for ListInfo {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.items, &other.items)
            && self.vertical == other.vertical
            && self.padding == other.padding
            && self.reverse == other.reverse
            && self.arrangement == other.arrangement
            && self.alignment == other.alignment
    }
}

/// A vertical lazy list, as Compose's `LazyColumn`. `reverse_layout` lays the items out from
/// the bottom (the first one there, scrolled from there); the arrangement stays from the top,
/// as Compose's (whose default becomes `Arrangement.Bottom` when reversed: pass
/// `Arrangement::BOTTOM` here). `user_scroll_enabled = false` keeps drags and the wheel from
/// scrolling it (the state still does).
#[composable(non_restartable)]
#[allow(non_snake_case)]
pub fn LazyColumn(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::remember_lazy_list_state(0, 0))] state: LazyListState,
    #[default(crate::layout::PaddingValues::ZERO)] content_padding: PaddingValues,
    #[default(false)] reverse_layout: bool,
    #[default(crate::layout::Arrangement::TOP)] vertical_arrangement: impl Into<VerticalArrangement>,
    #[default(crate::Alignment::START)] horizontal_alignment: HorizontalAlignment,
    #[default(true)] user_scroll_enabled: bool,
    content: impl Fn(LazyListScope) + 'static,
) {
    LazyList(
        modifier,
        state,
        content_padding,
        true,
        reverse_layout,
        vertical_arrangement.into().0,
        Alignment::new(horizontal_alignment.bias, -1.0),
        user_scroll_enabled,
        content,
    );
}

/// A horizontal lazy list, as Compose's `LazyRow`. `reverse_layout` lays the items out from
/// the end (the right, in left to right).
#[composable(non_restartable)]
#[allow(non_snake_case)]
pub fn LazyRow(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::remember_lazy_list_state(0, 0))] state: LazyListState,
    #[default(crate::layout::PaddingValues::ZERO)] content_padding: PaddingValues,
    #[default(false)] reverse_layout: bool,
    #[default(crate::layout::Arrangement::START)] horizontal_arrangement: impl Into<
        HorizontalArrangement,
    >,
    #[default(crate::Alignment::TOP)] vertical_alignment: VerticalAlignment,
    #[default(true)] user_scroll_enabled: bool,
    content: impl Fn(LazyListScope) + 'static,
) {
    LazyList(
        modifier,
        state,
        content_padding,
        false,
        reverse_layout,
        horizontal_arrangement.into().0,
        Alignment::new(-1.0, vertical_alignment.bias),
        user_scroll_enabled,
        content,
    );
}

#[composable(non_restartable)]
#[allow(non_snake_case, clippy::too_many_arguments)]
fn LazyList(
    modifier: Modifier,
    state: LazyListState,
    content_padding: PaddingValues,
    vertical: bool,
    reverse: bool,
    arrangement: MainAxisArrangement,
    alignment: Alignment,
    user_scroll_enabled: bool,
    content: impl Fn(LazyListScope) + 'static,
) {
    let scope = LazyListScope {
        intervals: Rc::default(),
        item_scope: LazyItemScope { parent_size: state.inner_or_detached().parent_size.clone() },
    };
    content(scope.clone());
    let intervals = std::mem::take(&mut *scope.intervals.borrow_mut());
    let items = Rc::new(Items::new(intervals));
    let info = ListInfo {
        items: items.clone(),
        vertical,
        padding: content_padding,
        reverse,
        arrangement,
        alignment,
    };
    let policy = {
        let scheduler = compose_ui::LOCAL_PREFETCH_SCHEDULER.current();
        compose_runtime::remember_keyed((state, InfoKey(info.clone()), scheduler.clone()), || {
            Rc::new(LazyListLayout { state: state.inner_or_detached(), info, scheduler })
                as Rc<dyn MeasurePolicy>
        })
    };
    let modifier = modifier.clip(compose_ui::RECTANGLE_SHAPE).then_custom(LazyScrollElement {
        state,
        vertical,
        reverse,
        enabled: user_scroll_enabled,
    });
    let items = ItemsRef(items);
    // The items' saveable state, kept by key while they're scrolled away, as Compose's
    // `LazySaveableStateHolder` (D290).
    let holder = compose_runtime::remember_saveable_state_holder();
    // Until the list is measured, its items' likely constraints: those around it, unbounded
    // along the list.
    let fallback = compose_ui::platform::LOCAL_CONSTRAINTS_HINT.current().map(|around| {
        let around = around.loosen();
        if vertical {
            Constraints::new(0, around.max_width, 0, Constraints::INFINITY)
        } else {
            Constraints::new(0, Constraints::INFINITY, 0, around.max_height)
        }
    });
    Layout(
        modifier,
        policy,
        compose_runtime::callback(move || LazyItems(state, items.clone(), holder, fallback)),
    );
}

/// The items composed: a restartable scope of its own, so a change of the range recomposes it
/// and not the list's caller; each item skips when it stays.
#[composable]
#[allow(non_snake_case)]
fn LazyItems(
    state: LazyListState,
    items: ItemsRef,
    holder: SaveableStateHolder,
    fallback: Option<Constraints>,
) {
    // Each item in a slot of its own, which an item of its content type reuses once it left
    // (D430). Kept when the list is in content reused (an outer list's item reused for
    // another): its items are reused too, as Compose keeps a reused node's
    // `SubcomposeLayoutState`.
    let slots = compose_runtime::platform::remember_across_reuse(|| {
        Rc::new(RefCell::new(ItemSlots::default()))
    });
    let inner = state.inner_or_detached();
    let (mut start, mut end) = inner.composed.get();
    inner.compose_again.get();
    inner.composed_read.set((start, end));
    let count = items.0.count;
    // Items came, went or moved: those composed stay composed where they went.
    if inner.had_items.get()
        && let Some(followed) = slots.borrow().follow(&items.0, (start.min(count), end.min(count)))
    {
        (start, end) = followed;
        // The layout makes it the composed range.
        inner.composed_more.set(true);
    }
    // A list not laid out yet composes its first item, and then those its layout asks for; in
    // reused content, it composes as many as it had, which keep their slots (as Compose's
    // measurement takes the active slots by key) rather than going to those kept for reuse,
    // where all but 7 would be disposed.
    if !inner.had_items.get() && start + slots.borrow().active.len() > end {
        end = start + slots.borrow().active.len();
        // The layout makes it the composed range (rather than the first item alone).
        inner.composed_more.set(true);
    }
    let mut indices: Vec<usize> = (start.min(count)..end.min(count)).collect();
    // The focused item, by key (gone with its item).
    if let Some(pinned) = inner.pinned.get()
        && let Some(index) = items.0.index_of(pinned, inner.pinned_near.get())
    {
        inner.pinned_near.set(index);
        if !indices.contains(&index) {
            indices.push(index);
        }
    }
    // The sticky header over the first visible item, wherever it is.
    if let Some(header) = inner.sticky.get()
        && header < count
        && !indices.contains(&header)
    {
        indices.push(header);
    }
    #[cfg(debug_assertions)]
    items.0.check_keys(&indices);
    *inner.children.borrow_mut() = indices.clone();
    // The items' constraints, for those composing for theirs (`BoxWithConstraints`).
    let hint = inner.item_constraints.get().or(fallback);
    drop(inner);
    let composed = {
        let inner = state.inner_or_detached();
        let sizes = inner.sizes.borrow();
        slots.borrow_mut().assign(&items.0, &indices, |index| sizes.contains_key(&index))
    };
    slots.borrow_mut().items = Rc::downgrade(&items.0);
    compose_runtime::CompositionLocalProvider(
        compose_ui::platform::LOCAL_CONSTRAINTS_HINT.provides(hint),
        || {
            // The slots holding items first, in the children's order the layout reads; then
            // those kept for reuse, deactivated: their nodes stay, unmeasured and unplaced. One
            // loop, so a slot's group moves between the two.
            for &(slot, content) in &composed {
                key(slot.id, || compose_slot(slot, content, state, &items, holder));
            }
        },
    );
}

/// One item: a layout node around its content, which pins the item while it holds the focus.
/// Its content is in a `SaveableStateProvider` of its key: scrolled away and back, it gets its
/// saveable state back (inside the item, which skips when it stays: a pass that only scrolls
/// doesn't run the provider again).
#[composable]
#[allow(non_snake_case)]
fn LazyItem(index: usize, state: LazyListState, items: ItemsRef, holder: SaveableStateHolder) {
    let policy = remember(|| Rc::new(ItemLayout) as Rc<dyn MeasurePolicy>);
    let pinned = state.inner_or_detached().pinned.handle();
    let item_key = items.0.key(index);
    let near = state;
    // An item is a repaint boundary: scrolled, or beside an animation, it's not recorded again.
    let modifier = Modifier.repaint_boundary().on_focus_changed(move |focused: bool| {
        if focused {
            if let Some(inner) = near.inner() {
                inner.pinned_near.set(index);
            }
            pinned.set(Some(item_key));
        } else if pinned.get() == Some(item_key) {
            pinned.set(None);
        }
    });
    Layout(
        modifier,
        policy,
        compose_runtime::callback(move || {
            if let Some((interval, local)) = items.0.find(index) {
                holder.SaveableStateProvider(item_key, || (interval.content)(local));
            }
        }),
    );
}

/// An item: its children stacked at the top start, as large as the largest.
struct ItemLayout;

impl MeasurePolicy for ItemLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let placeables: Vec<Placeable> =
            measurables.iter().map(|&measurable| scope.measure(measurable, constraints)).collect();
        let size = placeables.iter().fold(
            IntSize::new(constraints.min_width, constraints.min_height),
            |size, placeable| {
                IntSize::new(size.width.max(placeable.width), size.height.max(placeable.height))
            },
        );
        for placeable in &placeables {
            scope.place_relative(placeable, 0, 0);
        }
        scope.layout(size.width, size.height)
    }

    fn debug_name(&self) -> &'static str {
        "LazyItem"
    }
}

/// The list's info, compared by value (its items by identity).
#[derive(Clone)]
struct InfoKey(ListInfo);

impl ComposeArg for InfoKey {
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

/// The lazy list's measurement: fills the viewport from the scroll position with the composed
/// items, and asks for the items it lacks.
struct LazyListLayout {
    state: Rc<LazyListInner>,
    info: ListInfo,
    /// Where it asks for the items after a scroll to be prefetched.
    scheduler: Option<PrefetchScheduler>,
}

/// Measures an item by index (once), recording its size; `None` if it isn't composed.
type MeasureItem<'a> = dyn FnMut(usize, &mut Sizes) -> Option<Placeable> + 'a;

impl MeasurePolicy for LazyListLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let state = &self.state;
        // Observed: a scroll measures the list again. Nothing else this measurement writes is
        // read here.
        state.remeasure.get();
        let info = &self.info;
        let vertical = info.vertical;
        let density = scope.density();
        let node = scope.node_id();
        // A row is laid out left to right, and `place_relative` mirrors it in a right-to-left
        // layout; a column's items align in the direction and are placed as they are, as
        // Compose's (so a centered item rounds as in left to right).
        let layout_direction = scope.layout_direction();
        state.rtl.set(layout_direction == compose_ui::LayoutDirection::Rtl);
        let direction = compose_ui::LayoutDirection::Ltr;
        let px = |dp: Dp| density.round_to_px(dp);
        // Positions along the axis are worked out from the list's start (its end, reversed:
        // the padding there comes first), and mirrored when placed if reversed.
        let reverse = info.reverse;
        let (mut before, mut after, cross_before, cross_after) = if vertical {
            (
                px(info.padding.top),
                px(info.padding.bottom),
                px(info.padding.start),
                px(info.padding.end),
            )
        } else {
            (
                px(info.padding.start),
                px(info.padding.end),
                px(info.padding.top),
                px(info.padding.bottom),
            )
        };
        if reverse {
            std::mem::swap(&mut before, &mut after);
        }
        let spacing = info.arrangement.spacing(&density);
        let main_max = if vertical { constraints.max_height } else { constraints.max_width };
        assert!(
            main_max != Constraints::INFINITY,
            "compose-foundation: a lazy list was measured with an infinite size along its axis \
             (in a scrolling parent?): give it a size"
        );
        let cross_max = if vertical { constraints.max_width } else { constraints.max_height };
        let cross_room = if cross_max == Constraints::INFINITY {
            Constraints::INFINITY
        } else {
            (cross_max - cross_before - cross_after).max(0)
        };
        let item_constraints = if vertical {
            Constraints::new(0, cross_room, 0, Constraints::INFINITY)
        } else {
            Constraints::new(0, Constraints::INFINITY, 0, cross_room)
        };
        if state.item_constraints.get() != Some(item_constraints) {
            state.item_constraints.set(Some(item_constraints));
        }
        // The viewport less its padding, for `fill_parent_max_*`: written before the items are
        // measured, so they read this measurement's.
        let main_room = (main_max - before - after).max(0);
        let parent_size = if vertical {
            IntSize::new(cross_room, main_room)
        } else {
            IntSize::new(main_room, cross_room)
        };
        if state.parent_size.written.replace(parent_size) != parent_size {
            state.parent_size.size.set(parent_size);
        }
        let count = info.items.count;
        let children = state.children.borrow().clone();
        let (composed_start, composed_end) = composed_range(&children, count);
        // Each child measured once, when first needed.
        let mut placeables: Vec<Option<Placeable>> = vec![None; measurables.len()];
        let mut sizes = state.sizes.borrow_mut();
        let main_of =
            move |placeable: &Placeable| if vertical { placeable.height } else { placeable.width };
        let mut measure_child = |index: usize, sizes: &mut Sizes| -> Option<Placeable> {
            let child = children
                .iter()
                .position(|composed| *composed == index)
                .filter(|child| *child < measurables.len())?;
            if placeables[child].is_none() {
                let placeable = scope.measure(measurables[child], item_constraints);
                sizes.insert(index, main_of(&placeable));
                placeables[child] = Some(placeable);
            }
            placeables[child]
        };
        let measure: &mut MeasureItem<'_> = &mut measure_child;
        // The position, with the scrolling asked for; the first visible item found again by
        // its key if items came or went before it.
        let (mut index, offset) = state.position.get();
        let scrolled = state.pending.replace(0.0);
        let mut offset = offset + scrolled;
        // Kept to what pixel arithmetic holds (a jump to `i32::MAX` into an item, an infinite
        // scroll): the end clamps it on to where the list ends.
        const MOST_OFFSET: f32 = 1.0e9;
        offset = if offset.is_nan() { 0.0 } else { offset.clamp(-MOST_OFFSET, MOST_OFFSET) };
        if let Some(first_key) = state.first_key.get()
            && (index >= count || info.items.key(index) != first_key)
            && let Some(found) = info.items.index_of(first_key, index)
        {
            index = found;
        }
        if count == 0 {
            // Before the first items (data still loading), the initial position waits for
            // them; a list emptied starts again at its top, as Compose's.
            if state.had_items.get() {
                state.position.set((0, 0.0));
                state.first_key.set(None);
                state.had_items.set(false);
            } else {
                state.pending.set(state.pending.get() + offset - state.position.get().1);
            }
            state.room_backward.set(0.0);
            state.room_forward.set(0.0);
            return scope.layout(constraints.min_width, constraints.min_height);
        }
        state.had_items.set(true);
        index = index.min(count.saturating_sub(1));
        // Items before the composed ones are stepped over only once composed (their sizes
        // known): the scrolling left waits for them, in this frame's next round.
        let mut wanted_before = 0usize;
        let step_back = |index: &mut usize,
                         offset: &mut f32,
                         sizes: &mut Sizes,
                         measure: &mut MeasureItem<'_>|
         -> usize {
            while *offset < 0.0 && *index > 0 {
                match measure(*index - 1, sizes) {
                    Some(placeable) => {
                        *index -= 1;
                        *offset += (main_of(&placeable) + spacing) as f32;
                    }
                    None => {
                        // Far back: jumped over by the average size (an estimate, as the items
                        // between aren't composed); near: waits for them to be.
                        let average = sizes.average() as f32 + spacing as f32;
                        let skip = ((-*offset) / average) as usize;
                        if skip > MOST_MISSING {
                            let skip = skip.min(*index);
                            *index -= skip;
                            *offset += skip as f32 * average;
                            continue;
                        }
                        let wanted = ((-*offset) / average).ceil() as usize + 1;
                        state.pending.set(state.pending.get() + *offset);
                        *offset = 0.0;
                        return wanted;
                    }
                }
            }
            *offset = offset.max(0.0);
            0
        };
        wanted_before = wanted_before.max(step_back(&mut index, &mut offset, &mut sizes, measure));
        // On over the items scrolled past, while they are composed; far ahead, jumped over by
        // the average size (an estimate).
        while index + 1 < count {
            let Some(placeable) = measure(index, &mut sizes) else {
                let average = sizes.average() as f32 + spacing as f32;
                let skip = ((offset / average) as usize).min(count - 1 - index);
                if skip > MOST_MISSING {
                    index += skip;
                    offset -= skip as f32 * average;
                }
                break;
            };
            let size = (main_of(&placeable) + spacing) as f32;
            if offset >= size {
                offset -= size;
                index += 1;
            } else {
                break;
            }
        }
        // Past the end of the last item: at its end (the room left then scrolls back).
        if index + 1 == count
            && let Some(placeable) = measure(index, &mut sizes)
        {
            offset = offset.min(main_of(&placeable) as f32);
        }
        let mut placed: Vec<(Placeable, i32)> = Vec::new();
        let mut end_index;
        let mut missing;
        let mut content_end;
        let mut attempt = 0;
        loop {
            attempt += 1;
            placed.clear();
            missing = false;
            let mut main = before - offset.round() as i32;
            end_index = index;
            while end_index < count && main < main_max {
                match measure(end_index, &mut sizes) {
                    Some(placeable) => {
                        let size = main_of(&placeable);
                        placed.push((placeable, main));
                        main += size + spacing;
                        end_index += 1;
                    }
                    None => {
                        missing = true;
                        break;
                    }
                }
            }
            content_end = if placed.is_empty() { main } else { main - spacing } + after;
            // At the end with room left: scrolled back so the last item ends the viewport.
            let gap = (main_max - content_end) as f32;
            if !missing
                && end_index == count
                && gap > 0.0
                && (index > 0 || offset > 0.0)
                // Not while items before wait to be composed: the scrolling left pending holds
                // the gap already (a second measurement would step it back again, D457).
                && wanted_before == 0
                && attempt < 3
            {
                offset -= gap;
                let wanted = step_back(&mut index, &mut offset, &mut sizes, measure);
                wanted_before = wanted_before.max(wanted);
                if wanted > 0 {
                    // The items before aren't composed: the rest waits for them (pending).
                    break;
                }
                continue;
            }
            break;
        }
        // The list's length: the viewport once scrolled, else its content's, within the
        // constraints.
        let scrolled_into = index > 0 || offset > 0.0;
        let main_size = if scrolled_into { main_max } else { content_end.min(main_max) };
        let main_total = if vertical {
            constraints.constrain_height(main_size)
        } else {
            constraints.constrain_width(main_size)
        };
        // Content shorter than the list, not scrolled: placed by the arrangement, from the top
        // (the left) even when reversed, as Compose's.
        if !missing
            && end_index == count
            && index == 0
            && offset == 0.0
            && content_end < main_total
            && !placed.is_empty()
        {
            let mut item_sizes: Vec<i32> =
                placed.iter().map(|(placeable, _)| main_of(placeable)).collect();
            if reverse {
                item_sizes.reverse();
            }
            let mut positions = vec![0; item_sizes.len()];
            info.arrangement.arrange(
                main_total - before - after,
                &item_sizes,
                direction,
                &density,
                &mut positions,
            );
            let last = placed.len() - 1;
            for (position, (placeable, main)) in placed.iter_mut().enumerate() {
                *main = if reverse {
                    // Mirrored back when placed.
                    let physical = after + positions[last - position];
                    main_total - physical - main_of(placeable)
                } else {
                    before + positions[position]
                };
            }
        }
        // The items wanted composed: those in view and a few beyond, and as many more as the
        // room left would take, at the smallest size seen, if items were missing.
        // A viewport's worth of items beyond each side (at least two), so focus can move to
        // the next focusable item and scrolling finds them ready.
        let average = sizes.average();
        let beyond = ((main_max / (average + spacing).max(1)) as usize).clamp(BEYOND, MOST_MISSING);
        let wanted_start = index.saturating_sub(beyond + wanted_before);
        let mut wanted_end = (end_index + beyond).min(count);
        if missing {
            // At the size of the items in view, if any: the ones about to come are likelier
            // like them than like the list's average.
            let in_view = if placed.is_empty() {
                average
            } else {
                (placed.iter().map(|(placeable, _)| main_of(placeable)).sum::<i32>()
                    / placed.len() as i32)
                    .max(1)
            };
            let room = (main_max - content_end).max(0);
            let more = ((room / (in_view + spacing).max(1)) as usize + 1).min(MOST_MISSING);
            wanted_end = (end_index + more + beyond).min(count);
        }
        let covers = composed_start <= wanted_start && composed_end >= wanted_end;
        let too_many =
            composed_start + 2 * beyond < wanted_start || composed_end > wanted_end + 2 * beyond;
        let prefetch = &state.prefetch;
        let mut composed = (composed_start, composed_end);
        if state.composed_more.take() && covers && !too_many {
            state.composed.set(composed);
        }
        if !covers || too_many {
            // Items prefetched ahead in the scroll direction stay, and those just behind it (up
            // to `KEPT_BEHIND`), for a scroll back: those composed next to the wanted ones (a
            // jump keeps none, and composes none beyond them).
            let (before, after) = if prefetch.forward.get() {
                (KEPT_BEHIND, PREFETCH_AHEAD)
            } else {
                (PREFETCH_AHEAD, KEPT_BEHIND)
            };
            let start = if (composed_start..=composed_end).contains(&wanted_start) {
                composed_start.max(wanted_start.saturating_sub(before))
            } else {
                wanted_start
            };
            let end = if (composed_start..=composed_end).contains(&wanted_end) {
                composed_end.min(wanted_end + after)
            } else {
                wanted_end
            };
            composed = (start, end);
            state.compose(composed);
        }
        // After a scroll, the next items in its direction are composed between frames (and
        // those left behind kept for reuse), if any are to be.
        prefetch.node.set(Some(node));
        prefetch.count.set(count);
        prefetch.wanted.set((wanted_start, wanted_end));
        // As many kept behind as composed ahead (`KEPT_BEHIND` is `PREFETCH_AHEAD`).
        let limits = (wanted_start.saturating_sub(KEPT_BEHIND), wanted_end + PREFETCH_AHEAD);
        prefetch.limits.set(limits);
        if scrolled != 0.0
            && let Some(scheduler) = &self.scheduler
            && prefetch.next(composed).is_some()
            && prefetch.request.borrow().upgrade().is_none()
        {
            let request = Rc::new(LazyListPrefetch { state: Rc::downgrade(state) });
            *prefetch.request.borrow_mut() = Rc::downgrade(&request);
            scheduler.schedule_prefetch(request);
        }
        state.position.set((index, offset));
        if count > 0 {
            state.first_key.set(Some(info.items.key(index)));
        }
        let layout_info = (count, average + spacing, spacing, main_max, before + after);
        if state.layout_info.get() != layout_info {
            state.layout_info.set(layout_info);
        }
        let rounded = (index, offset.round() as i32);
        if state.published.get() != rounded {
            state.published.set(rounded);
            state.first_index.set(rounded.0);
            state.first_offset.set(rounded.1);
        }
        // The composed items out of view are placed too (clipped away), so focus can move to
        // them and a focused item scrolled away keeps its node; those composed beyond the ones
        // wanted (prefetched) aren't, as Compose's: nothing draws them until they're wanted.
        // A focused one among those is placed, next to the others: focus scrolls it in.
        let (mut placed_start, mut placed_end) =
            (composed_start.max(wanted_start), composed_end.min(wanted_end));
        if let Some(key) = state.pinned.get()
            && let Some(pinned) = info.items.index_of(key, state.pinned_near.get())
        {
            if (placed_end..composed_end).contains(&pinned) {
                placed_end = pinned + 1;
            } else if (composed_start..placed_start).contains(&pinned) {
                placed_start = pinned;
            }
        }
        let mut beyond: Vec<(Placeable, i32)> = Vec::new();
        let mut main =
            placed.last().map_or(before, |(placeable, main)| main + main_of(placeable) + spacing);
        for next in end_index..placed_end.min(count) {
            if let Some(placeable) = measure(next, &mut sizes) {
                beyond.push((placeable, main));
                main += main_of(&placeable) + spacing;
            }
        }
        // How far the list can scroll each way: known when the composed items reach its ends.
        let extent_end = main - spacing + after;
        // Those before, placed first, in the items' order: the children are then placed in
        // their order (but a sticky header), which the tree draws and hits without sorting them.
        let mut ahead: Vec<(Placeable, i32)> = Vec::new();
        let mut main = placed.first().map_or(before, |(_, main)| *main);
        for previous in (placed_start.min(index)..index).rev() {
            if let Some(placeable) = measure(previous, &mut sizes) {
                main -= main_of(&placeable) + spacing;
                ahead.push((placeable, main));
            }
        }
        ahead.reverse();
        let extent_start = main - before;
        let pending = state.pending.get();
        let reaches_start = (index == 0 || placed_start == 0) && wanted_before == 0;
        // Not reaching an end: estimated from the average size (a nested list's wheel delta
        // it can't use goes on to its parent).
        let average = sizes.average() as f32 + spacing as f32;
        state.room_backward.set(if reaches_start {
            (-extent_start) as f32 + pending
        } else {
            (placed_start.min(index) as f32 * average - extent_start as f32 + pending).max(1.0)
        });
        let reaches_end = !missing && (end_index == count || placed_end >= count);
        state.room_forward.set(if reaches_end {
            (extent_end.max(content_end) - main_max).max(0) as f32 - pending
        } else {
            let after_composed = count.saturating_sub(placed_end.max(end_index)) as f32 * average;
            (after_composed + (extent_end.max(content_end) - main_max) as f32 - pending).max(1.0)
        });
        // The sticky header over the first visible item, kept composed: at the list's start,
        // or where it is if lower, pushed out by the next header.
        let headers = info.items.headers_around(index);
        let header = headers.map(|(header, _)| header);
        if state.sticky_written.replace(header) != header {
            state.sticky.set(header);
        }
        let mut sticky = None;
        if let Some((header, next)) = headers
            && let Some(placeable) = measure(header, &mut sizes)
        {
            let natural =
                ahead.iter().chain(&placed).chain(&beyond).find(|(other, _)| *other == placeable);
            let mut at = natural.map_or(0, |(_, main)| (*main).max(0));
            if let Some(next) = next.and_then(|next| measure(next, &mut sizes))
                && let Some((_, next_main)) =
                    ahead.iter().chain(&placed).chain(&beyond).find(|(other, _)| *other == next)
            {
                at = at.min(next_main - main_of(&placeable));
            }
            placed.retain(|(other, _)| *other != placeable);
            beyond.retain(|(other, _)| *other != placeable);
            ahead.retain(|(other, _)| *other != placeable);
            sticky = Some((placeable, at));
        }
        let children = state.children.borrow().clone();
        for &pinned in &children {
            if (pinned < composed_start || pinned >= composed_end)
                && Some(pinned) != header
                && let Some(placeable) = measure(pinned, &mut sizes)
            {
                let at =
                    if pinned < index { -main_of(&placeable) - main_max } else { main_max * 2 };
                beyond.push((placeable, at));
            }
        }
        // Sizes far from view are forgotten, so the estimates follow the data.
        if sizes.len() > 4 * MOST_MISSING {
            sizes.retain(|item| item.abs_diff(index) <= MOST_MISSING);
        }
        drop(sizes);
        // Placed along the axis, aligned across it.
        let cross_size = placed
            .iter()
            .map(|(placeable, _)| if vertical { placeable.width } else { placeable.height })
            .max()
            .unwrap_or(0);
        let (width, height) = if vertical {
            (constraints.constrain_width(cross_size + cross_before + cross_after), main_total)
        } else {
            (main_total, constraints.constrain_height(cross_size + cross_before + cross_after))
        };
        let layout = |scope: &mut MeasureScope<'_>, placeable: &Placeable, main: i32| {
            let main = if reverse { main_total - main - main_of(placeable) } else { main };
            if vertical {
                let space = IntSize::new(width - cross_before - cross_after, placeable.height);
                let x = info.alignment.align(placeable.size(), space, layout_direction).x;
                // The content padding's start is on the right in a right-to-left layout.
                let before = if layout_direction == compose_ui::LayoutDirection::Rtl {
                    cross_after
                } else {
                    cross_before
                };
                scope.place(placeable, before + x, main);
            } else {
                let space = IntSize::new(placeable.width, height - cross_before - cross_after);
                let y = info.alignment.align(placeable.size(), space, direction).y;
                scope.place_relative(placeable, main, cross_before + y);
            }
        };
        // The sticky header last: drawn over the items, and hit first.
        for (placeable, main) in ahead.iter().chain(&placed).chain(&beyond).chain(&sticky) {
            layout(scope, placeable, *main);
        }
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "LazyList"
    }
}

/// Sizes along the axis by index (a list's items, a grid's items or lines), as last measured,
/// with their sum: the average is a division, not a walk over them (D456).
#[derive(Default)]
struct Sizes {
    sizes: HashMap<usize, i32>,
    sum: i64,
}

impl Sizes {
    fn insert(&mut self, index: usize, size: i32) {
        self.sum += i64::from(size) - self.sizes.insert(index, size).map_or(0, i64::from);
    }

    fn contains_key(&self, index: &usize) -> bool {
        self.sizes.contains_key(index)
    }

    fn len(&self) -> usize {
        self.sizes.len()
    }

    /// Keeps the sizes of the indices `keep` says.
    fn retain(&mut self, keep: impl Fn(usize) -> bool) {
        let sum = &mut self.sum;
        self.sizes.retain(|index, size| {
            let kept = keep(*index);
            if !kept {
                *sum -= i64::from(*size);
            }
            kept
        });
    }

    /// The average size measured (at least a pixel): what estimates of positions and of how
    /// many items fit use.
    fn average(&self) -> i32 {
        if self.sizes.is_empty() {
            return 1;
        }
        (self.sum / self.sizes.len() as i64).max(1) as i32
    }
}

/// The items a lazy list will scroll to next, composed and measured between frames, as
/// Compose's `LazyListPrefetchStrategy`: one at a time, up to `PREFETCH_AHEAD` beyond those its
/// layout wants, while the idle time left exceeds what an item took on average. Short of time,
/// it gives up until the list scrolls again (which asks anew), rather than waking the platform
/// at every frame for an item that may never fit.
struct LazyListPrefetch {
    state: Weak<LazyListInner>,
}

impl PrefetchRequest for LazyListPrefetch {
    fn execute(&self, scope: &mut PrefetchRequestScope<'_>) -> bool {
        let Some(state) = self.state.upgrade() else { return false };
        let prefetch = &state.prefetch;
        // A step that doesn't fit after others in the same idle time may fit the next one
        // whole: its estimate is lowered only when it's the first.
        let mut first = true;
        // A step slower than its estimate (a little core, low clocks) slows those after it.
        let mut pace = Pace::EXPECTED;
        loop {
            // An item composed ahead is measured first.
            if let Some(index) = prefetch.to_measure.get() {
                if !fits(scope, &prefetch.measure_cost, first, pace) {
                    return false;
                }
                let _section =
                    compose_ui::platform::trace::section(c"compose-rs: prefetch measure");
                let start = Instant::now();
                let child = state.children.borrow().iter().position(|composed| *composed == index);
                let constraints_of = prefetch.constraints_of.borrow().clone();
                let constraints = match constraints_of {
                    Some(constraints_of) => constraints_of(index),
                    None => state.item_constraints.get(),
                };
                if let (Some(node), Some(child), Some(constraints)) =
                    (prefetch.node.get(), child, constraints)
                {
                    // What measuring it wrote (a nested list's items, a `BoxWithConstraints`'
                    // constraints) is composed and measured too, as a frame's rounds do.
                    for _ in 0..3 {
                        scope.premeasure(node, child, constraints);
                        if !scope.recompose() {
                            break;
                        }
                    }
                    // Timed only when it measured: an item gone in the meantime would lower
                    // the estimate with a sample of nothing.
                    pace = time(&prefetch.measure_cost, start, pace);
                }
                prefetch.to_measure.set(None);
                first = false;
                continue;
            }
            let Some((range, ahead)) = prefetch.next(state.composed.get()) else { return false };
            if !fits(scope, &prefetch.compose_cost, first, pace) {
                return false;
            }
            let section = compose_ui::platform::trace::section(c"compose-rs: prefetch compose");
            let began = Instant::now();
            state.composed.set(range);
            scope.recompose();
            pace = time(&prefetch.compose_cost, began, pace);
            drop(section);
            prefetch.to_measure.set(ahead);
            first = false;
        }
    }
}

impl Prefetch {
    /// What to compose next between frames, from the `composed` range: one more item ahead in
    /// the scroll direction (within `limits`: `PREFETCH_AHEAD` items, or a grid's lines, beyond
    /// those the layout wants), and of those behind it doesn't want, those within `limits`
    /// only (`KEPT_BEHIND`), as a frame composing the item would have left them. The range, and
    /// the item ahead (to measure
    /// next); `None` when nothing is to be done, or when the composed items don't cover the
    /// wanted ones (the frame composes those).
    fn next(&self, composed: (usize, usize)) -> Option<((usize, usize), Option<usize>)> {
        let (start, end) = composed;
        let (wanted_start, wanted_end) = self.wanted.get();
        if start > wanted_start || end < wanted_end {
            return None;
        }
        let (low, high) = self.limits.get();
        if self.forward.get() {
            let ahead = (end < self.count.get().min(high)).then_some(end);
            let kept = start.max(low);
            (ahead.is_some() || kept > start)
                .then(|| ((kept, end + usize::from(ahead.is_some())), ahead))
        } else {
            let ahead = (start > low).then(|| start.checked_sub(1)).flatten();
            let kept = end.min(high);
            (ahead.is_some() || kept < end).then(|| ((ahead.unwrap_or(start), kept), ahead))
        }
    }
}

/// Whether the time left exceeds the step's estimate, at the `pace` of the list's steps so far
/// in this idle time, by `PREFETCH_MARGIN_NANOS`. If not, the estimate may be lowered
/// ([`StepCost::missed`]). `first`: no step of this request ran in this idle time (another list's may have: it then
/// counts as a miss of this one's).
fn fits(scope: &PrefetchRequestScope<'_>, cost: &Cell<StepCost>, first: bool, pace: Pace) -> bool {
    let available = scope.available_time_nanos();
    if available > pace.needed(cost.get().estimate()) {
        return true;
    }
    cost.set(cost.get().missed(first));
    false
}

/// Adds the time since `start` to the step's cost; returns the idle time's pace with it.
fn time(cost: &Cell<StepCost>, start: Instant, pace: Pace) -> Pace {
    let nanos = start.elapsed().as_nanos() as u64;
    let estimate = cost.get().estimate();
    cost.set(cost.get().timed(nanos));
    pace.after(nanos, estimate)
}

/// How much slower than estimated a list's steps in an idle time ran: the largest ratio of a
/// step's time to its estimate, at least 1 and at most `MOST_PACE` (D381). The steps after a
/// slow one are expected to be as slow: the main thread is on a little core, or at low clocks,
/// for the rest of the idle time. On the Pixel 8 Pro, 43 of the 46 prefetch measurements that
/// ended more than 1.2 ms past the vsync (in 32 scroll runs) followed the item's composition in
/// the same idle time, all on the little cores: they took 3.9–8.2 ms (a median of 5.1) against
/// an estimate of a median 2.4 ms, and the composition before them a median 2.0 times its
/// estimate.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Pace(f64);

/// The slowest pace: a step preempted, or a tiny estimate, says little about the next one, which
/// would never fit (the list then gives up until the next scroll).
const MOST_PACE: f64 = 4.0;

impl Pace {
    /// An idle time's first step: as estimated.
    const EXPECTED: Pace = Pace(1.0);

    /// After a step that took `nanos` against an estimate of `estimate` (none: 0).
    fn after(self, nanos: u64, estimate: u64) -> Pace {
        if estimate == 0 {
            return self;
        }
        Pace(self.0.max((nanos as f64 / estimate as f64).min(MOST_PACE)))
    }

    /// The time left a step of `estimate` needs to start: its estimate at this pace, and the
    /// margin.
    fn needed(self, estimate: u64) -> u64 {
        ((estimate as f64 * self.0) as u64).saturating_add(PREFETCH_MARGIN_NANOS)
    }
}

/// The contiguous range of composed items (the children but a pinned one out of it).
fn composed_range(children: &[usize], count: usize) -> (usize, usize) {
    let mut start = None;
    let mut end = 0;
    for (position, &index) in children.iter().enumerate() {
        if position > 0 && index != children[position - 1] + 1 {
            break;
        }
        start.get_or_insert(index);
        end = index + 1;
    }
    let start = start.unwrap_or(0).min(count);
    (start, end.min(count).max(start))
}

/// The wheel and drags scroll the list (unless disabled: `user_scroll_enabled`).
struct LazyScrollElement {
    state: LazyListState,
    vertical: bool,
    reverse: bool,
    enabled: bool,
}

struct LazyScrollNode {
    state: LazyListState,
    vertical: bool,
    reverse: bool,
    enabled: bool,
    drag: DragScroller,
}

impl ModifierNodeElement for LazyScrollElement {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(LazyScrollNode {
            state: self.state,
            vertical: self.vertical,
            reverse: self.reverse,
            enabled: self.enabled,
            drag: DragScroller::default(),
        })
    }

    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<LazyScrollNode>() {
            // A fling scrolls the state it started with: a new state stops it (the old one's
            // owner may be going, D240).
            if node.state != self.state {
                node.drag.stop_fling();
            }
            node.state = self.state;
            node.vertical = self.vertical;
            node.reverse = self.reverse;
            if node.enabled && !self.enabled {
                node.drag.cancel();
                node.drag.stop_fling();
            }
            node.enabled = self.enabled;
        }
    }

    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<LazyScrollElement>().is_some_and(|other| {
            other.vertical == self.vertical
                && other.reverse == self.reverse
                && other.enabled == self.enabled
                && other.state.same(&self.state)
        })
    }

    fn name(&self) -> &'static str {
        "lazy_scroll"
    }
}

impl LazyScrollNode {
    /// Whether the content runs the other way along the axis: a reversed list, or a row right
    /// to left (both: left to right again).
    fn mirrored(&self, rtl: bool) -> bool {
        self.reverse ^ (!self.vertical && rtl)
    }
}

impl ModifierNode for LazyScrollNode {
    fn on_detach(&mut self, _context: &mut compose_ui::DetachContext<'_>) {
        self.drag.stop_fling();
    }

    fn on_reset(&mut self) {
        self.drag.cancel();
        self.drag.stop_fling();
    }

    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }

    fn hit_clip(&self) -> Option<compose_ui::Shape> {
        Some(compose_ui::Shape::Rectangle)
    }

    /// Scrolls the least that shows `rect` (an item beyond the viewport taking the focus).
    fn bring_into_view(&mut self, rect: Rect, viewport: IntSize) -> Option<Rect> {
        let inner = self.state.inner()?;
        let mirrored = self.mirrored(inner.rtl.get());
        let (start, end, length) =
            crate::scroll::scroll_axis(rect, viewport, self.vertical, mirrored);
        // Where it will be once the scrolling asked for (not yet laid out) is.
        let pending = inner.pending.get();
        let delta = bring_into_view_delta(start - pending, end - pending, length);
        let used = if delta == 0.0 { 0.0 } else { self.state.dispatch(delta) };
        Some(crate::scroll::scrolled_rect(rect, used, self.vertical, mirrored))
    }
}

impl PointerInputModifierNode for LazyScrollNode {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        _bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        if !self.enabled {
            return;
        }
        let state = self.state;
        let reverse = self.mirrored(context.layout_direction() == compose_ui::LayoutDirection::Rtl);
        let flung = state;
        self.drag.on_pointer_event(
            event,
            pass,
            self.vertical,
            reverse,
            context,
            |delta| state.dispatch(delta),
            move || Rc::new(move |delta| flung.dispatch(delta)),
        );
    }

    fn on_cancel_pointer_input(&mut self) {
        self.drag.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::{PREFETCH_MARGIN_NANOS, Pace, StepCost};

    const MS: u64 = 1_000_000;

    #[test]
    fn an_idle_time_starts_at_the_expected_pace() {
        assert_eq!(Pace::EXPECTED.needed(3 * MS), 3 * MS + PREFETCH_MARGIN_NANOS);
        // Steps at or under their estimate change nothing.
        let pace = Pace::EXPECTED.after(MS, 2 * MS).after(2 * MS, 2 * MS);
        assert_eq!(pace.needed(3 * MS), 3 * MS + PREFETCH_MARGIN_NANOS);
    }

    #[test]
    fn a_slow_step_scales_the_next_ones_estimates() {
        // A composition that took 2.75 ms against its 1.1 ms estimate (the little cores, on the
        // Pixel 8 Pro): the measurement after it is expected to take 2.5 times its 2 ms too,
        // so 3.4 ms left no longer starts it (it took 5 ms).
        let pace = Pace::EXPECTED.after(2_750_000, 1_100_000);
        assert_eq!(pace.needed(2 * MS), 5 * MS + PREFETCH_MARGIN_NANOS);
        assert!(pace.needed(2 * MS) > 3_400_000);
        // The slowest step so far sets it.
        assert_eq!(pace.after(MS, MS).needed(2 * MS), 5 * MS + PREFETCH_MARGIN_NANOS);
        // Nothing timed before says nothing.
        assert_eq!(Pace::EXPECTED.after(5 * MS, 0).needed(MS), MS + PREFETCH_MARGIN_NANOS);
        // A step preempted for 40 times its estimate counts as 4 times.
        assert_eq!(Pace::EXPECTED.after(40 * MS, MS).needed(MS), 4 * MS + PREFETCH_MARGIN_NANOS);
    }

    #[test]
    fn a_step_is_expected_to_take_the_recent_maximum() {
        let mut cost = StepCost::default();
        assert_eq!(cost.estimate(), 0, "nothing timed yet");
        for _ in 0..8 {
            cost = cost.timed(2 * MS);
        }
        assert_eq!(cost.estimate(), 2 * MS);
        cost = cost.timed(8 * MS);
        // The average says 3.5 ms; the step that took 8 ms says the next may too.
        assert_eq!(cost.average, 3_500_000);
        assert_eq!(cost.timed, super::SETTLED_STEPS);
        assert_eq!(cost.estimate(), 8 * MS);
    }

    #[test]
    fn the_maximum_fades_as_faster_steps_are_timed() {
        let mut cost = StepCost::default().timed(2 * MS).timed(8 * MS);
        let mut estimates = Vec::new();
        for _ in 0..12 {
            cost = cost.timed(2 * MS);
            estimates.push(cost.estimate());
        }
        assert_eq!(estimates[0], 7 * MS, "an eighth less with each step");
        assert!(estimates.windows(2).all(|pair| pair[1] <= pair[0]), "{estimates:?}");
        assert!(estimates[11] < 3 * MS, "back near the steps' time: {estimates:?}");
        assert!(estimates[11] >= 2 * MS, "never below the latest step: {estimates:?}");
    }

    #[test]
    fn a_miss_lowers_an_unsettled_estimate_by_a_quarter() {
        let cost = StepCost::default().timed(4 * MS).timed(8 * MS).missed(true);
        assert_eq!((cost.average, cost.peak), (3_750_000, 6 * MS));
        // Missed enough times, it fits any idle time again.
        let mut cost = cost;
        for _ in 0..40 {
            cost = cost.missed(true);
        }
        assert!(cost.estimate() < 1_000);
    }

    #[test]
    fn a_step_short_of_time_after_others_keeps_its_estimate() {
        let cost = StepCost::default().timed(5 * MS);
        assert_eq!(cost.missed(false), cost, "the next idle time may fit it whole");
        assert!(cost.missed(true).estimate() < cost.estimate());
    }

    #[test]
    fn misses_wear_a_steady_steps_estimate_down_slowly() {
        let mut steady = StepCost::default();
        for _ in 0..8 {
            steady = steady.timed(4 * MS);
        }
        for _ in 0..4 {
            steady = steady.missed(true);
        }
        // A sixteenth each: a 3 ms idle time still doesn't start it (a quarter each: 1.3 ms).
        assert!(steady.estimate() + PREFETCH_MARGIN_NANOS > 3 * MS, "{steady:?}");
        // But it fades, however short the idle times: nothing stops prefetching for good.
        for _ in 0..100 {
            steady = steady.missed(true);
        }
        assert!(steady.estimate() < MS, "{steady:?}");
    }

    #[test]
    fn an_outliers_maximum_fades_fast() {
        let mut cost = StepCost::default();
        for _ in 0..8 {
            cost = cost.timed(MS);
        }
        cost = cost.timed(40 * MS);
        assert_eq!(cost.estimate(), 40 * MS);
        for _ in 0..6 {
            cost = cost.missed(true);
        }
        // Down to the average (10.75 ms, lowered by a sixteenth each).
        assert_eq!(cost.peak, cost.average);
        assert!(cost.estimate() < 11 * MS, "{cost:?}");
    }
}
