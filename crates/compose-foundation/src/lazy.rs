//! Lazy lists, as Compose's `LazyColumn` and `LazyRow`: only the items in view are placed and
//! drawn; they, a few next to them and the focused one are composed and measured.
//!
//! Compose composes items while measuring (subcomposition). Here the list composes a range of
//! items, and its measurement, which fills the viewport from the scroll position, writes the
//! range it needs when the composed one falls short; the host then recomposes and measures again
//! within the same frame (`UiHost::frame`), so scrolling shows no empty frame. The items are
//! composed by a restartable composable of their own, each item one too: a scroll recomposes
//! neither the list's caller nor the items that stay.
//!
//! After each scroll, the next items in the scroll direction are composed and measured between
//! frames, as many as its cache window holds (prefetch, as Compose's `LazyLayoutCacheWindow`), so
//! the frame reaching them finds them ready. A focus search leaving the list asks it for the
//! next items, laid out beyond its bounds for the search (D477).
//!
//! An item that leaves is deactivated and kept for reuse, and an item that comes is composed in
//! the place of one of its content type, its nodes kept (D430), as Compose's
//! `LazyLayoutItemReusePolicy`.
//!
//! Lazy grids (`grid`) compose their items the same way, one slot per item, and lay them out
//! in lines.

mod animate_scroll;
mod cache_window;
mod grid;
mod item_animator;
mod layout_info;
pub(crate) mod pager;
mod prefetch;
mod staggered;

pub use animate_scroll::LazyLayoutScrollScope;
pub use cache_window::{
    LazyGridDefaults, LazyLayoutCacheWindow, LazyListDefaults, LazyStaggeredGridDefaults,
    WindowValue, lazy_layout_cache_window, lazy_layout_cache_window_fraction,
    lazy_layout_cache_window_fraction_with, lazy_layout_cache_window_with,
};
pub use grid::{
    CrossAxisCells, GridCells, GridItemSpan, LazyGridItemScope, LazyGridItemSpanScope,
    LazyGridScope, LazyGridState, LazyHorizontalGrid, LazyVerticalGrid, remember_lazy_grid_state,
};
pub use item_animator::{AnimateItemModifier, AnimateItemScope};
pub use layout_info::{LazyGridItemInfo, LazyGridLayoutInfo, LazyListItemInfo, LazyListLayoutInfo};
pub use staggered::{
    LazyHorizontalStaggeredGrid, LazyStaggeredGridItemInfo, LazyStaggeredGridLaneInfo,
    LazyStaggeredGridLayoutInfo, LazyStaggeredGridScope, LazyStaggeredGridState,
    LazyVerticalStaggeredGrid, NO_SPACING, StaggeredGridCells, StaggeredGridItemSpan,
    remember_lazy_staggered_grid_state,
};

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::{Rc, Weak};

use compose_runtime::{
    ComposeArg, CoroutineScope, ImmutableList, ObjectHandle, Owned, OwnedState,
    SaveableStateHolder, composable, hash_key, key, mutable_state_of,
};
use compose_ui::{
    Alignment, Constraints, Dp, EventContext, HorizontalAlignment, IntOffset, IntSize, Layout,
    Measurable, MeasurePolicy, MeasureResult, MeasureScope, Modifier, ModifierNode,
    ModifierNodeElement, PaddingValues, Placeable, PointerEvent, PointerEventPass,
    PointerInputModifierNode, PrefetchScheduler, Rect, VerticalAlignment,
};

use crate::OverscrollModifier;
use crate::gestures::{
    LocalBoxFuture, ScrollBlock, ScrollScope, ScrollableState, ScrollableStateExt,
};
use crate::layout::{HorizontalArrangement, MainAxisArrangement, VerticalArrangement};
use crate::mutator::{MutatePriority, MutationInterrupted};
use crate::scroll::{DragScroller, ScrollDriver, bring_into_view_delta, same_fling};
use item_animator::{ChildRecord, ItemAnimator, ItemChildren};
use prefetch::{Laid, Policy, Prefetch};

/// The lines a grid composes and places beyond each side of its viewport (D456), and the least
/// "viewport's worth" of items a list's trimming counts in. A list itself composes and places
/// only the items in view, as Compose's (`beyondBoundsItemCount` 0): prefetch composes the next
/// ones between frames, from its first layout on, a frame keeps those next to the ones in view
/// (D470), and a focus search leaving the list asks it for the next items, one at a time
/// (`lay_out_beyond_bounds`, D477).
const BEYOND: usize = 2;

/// The most items a measurement lacking items asks for in a round: this many, or as many as it
/// placed or asked for last round, twice, if more, so the items asked for double round to round
/// while it lacks them (D487, D492), up to `MOST_IN_A_ROUND`.
const FIRST_STEP: usize = 4;

/// The most items a measurement asks for in a round: a frame's 16 rounds compose about 5,000 at
/// most (zero-height items; anything taller fills the viewport first, D492).
const MOST_IN_A_ROUND: usize = 512;

/// How far a scroll steps over items it lacks by their estimated size rather than composing them
/// (a jump), and the most a viewport's worth of items counts (a list's trimming slack).
const MOST_MISSING: usize = 200;

/// How far from its last index an item is looked for by key.
const KEY_WINDOW: usize = 100;

/// The most composed items looked for in the key window when the items change (D455): a
/// change's cost stays bounded (each of the others is checked where the one before it went).
const MOST_KEY_SEARCHES: usize = 8;

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
    /// What runs `animate_item`, shared with its items' nodes.
    item_animator: Rc<ItemAnimator>,
    /// What `scroll_indicator_state` gives, one object as Compose's.
    indicator: std::cell::OnceCell<Rc<dyn crate::ScrollIndicatorState>>,
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
    /// What `remeasure` holds, to bump it without reading it.
    remeasure_written: Cell<u64>,
    /// The items to compose: what the layout asks for, read by the items' composition only.
    composed: OwnedState<(usize, usize)>,
    /// What the items' composition last read of `composed` (it may compose another range: the
    /// keys it followed, D455), and a count it reads, bumped to compose `composed` again when
    /// it holds the range wanted already.
    composed_read: Cell<(usize, usize)>,
    compose_again: OwnedState<u64>,
    /// The focused item's key, kept composed and placed while it scrolls away, as Compose pins
    /// it, and the index it was last at.
    pinned: OwnedState<Vec<u64>>,
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
    layout_info: OwnedState<(usize, f32, i32, i32, i32)>,
    /// The constraints its items are measured with, as last laid out: their composition's
    /// `LOCAL_CONSTRAINTS_HINT`.
    item_constraints: OwnedState<Option<Constraints>>,
    /// What was last written to `layout_info` and `item_constraints`, read by the layout
    /// without observing them: a measurement reading the state it writes is measured again in
    /// the same layout, and asks for another frame, which holds prefetch back until it ran
    /// (each item measured changes the average size, D471).
    layout_info_written: Cell<(usize, f32, i32, i32, i32)>,
    item_constraints_written: Cell<Option<Constraints>>,
    /// How far the list can scroll each way, as last laid out (infinite: not known), and
    /// whether that's an estimate (the end isn't laid out: from the average size). A delta is
    /// cut at an estimate only for a parent to take the rest (nested scrolling), as Compose's
    /// measurement consumes it up to the real end; otherwise the measurement takes it all and
    /// stops at the end it reaches (D476).
    room_backward: Cell<f32>,
    room_forward: Cell<f32>,
    room_estimated: Cell<(bool, bool)>,
    /// Whether the list can scroll towards its end and its start, as `canScrollForward` and
    /// `canScrollBackward`: false until it's laid out, written when a measurement changes them.
    can_scroll_forward: OwnedState<bool>,
    can_scroll_backward: OwnedState<bool>,
    /// What those hold (forward, backward), to compare without reading state.
    can_scroll_written: Cell<(bool, bool)>,
    /// The items the last measurement lacking items asked for (0: it lacked none): the next
    /// round asks for up to twice as many (D492).
    asked: Cell<usize>,
    /// User input taken past an estimated end with nested-scroll parents around: once the
    /// measurement knows how far the list went, what it couldn't use is offered to them, as
    /// Compose's measurement consumes a delta and the rest goes to post-scroll (D479).
    overshoot: RefCell<Option<Overshoot>>,
    prefetch: Prefetch,
    /// The viewport's size less the content padding, for the items' `fill_parent_max_*`.
    parent_size: Rc<ParentSize>,
    /// The items (a grid's lines) a focus search asked to lay out beyond those in view, before
    /// and after them, until it's over (D477); and whether there are any more that way, as
    /// last laid out.
    beyond_bounds: OwnedState<(usize, usize)>,
    /// Items composed and placed beyond those in view at all times, a pager's
    /// `beyondViewportPageCount`.
    beyond_viewport: Cell<usize>,
    /// A grid's: its semantics tell no counts.
    is_grid: Cell<bool>,
    /// Told of each measurement: a pager (D1467).
    observer: RefCell<Option<Rc<dyn ListObserver>>>,
    beyond_more: Cell<(bool, bool)>,
    /// A search that laid items out beyond the bounds ended: the next layout leaves the
    /// items composed for it, but those next to the ones wanted, and they're disposed rather
    /// than kept for reuse (no scroll brought them, D482).
    beyond_ended: Cell<bool>,
    dispose_leaving: Cell<bool>,
    /// The sticking header's size, as last measured: what a focused item is scrolled out from under.
    sticking_size: Cell<i32>,
    /// The largest size across of the items last laid out (a list's).
    cross_size: Cell<i32>,
    /// The cells across a grid, as last measured: what its items' scope says.
    cross_cells: OwnedState<usize>,
    cross_cells_written: Cell<usize>,
    /// Whether the composition seeded it, for what's composed before the first measurement.
    cross_cells_seeded: Cell<bool>,
    /// The sticky header to keep composed (over the first visible item), as last laid out.
    sticky: OwnedState<Option<usize>>,
    /// What was last written to `sticky`, read without observing it.
    sticky_written: Cell<Option<usize>>,
    /// Its scrolls' mutex and flags.
    core: crate::gestures::ScrollCore,
    /// What the last layout placed in view, for `layout_info`.
    laid_out: layout_info::LaidOut,
    /// Pixels per dp, as last laid out (an animated scroll's distances are in dp).
    density: Cell<f32>,
    /// The items as last laid out: `layout_info`'s keys and content types are theirs.
    laid_items: RefCell<Option<Rc<Items>>>,
    /// The nested-scroll parents a drag's or a fling's delta going through the scroll scope
    /// comes through, set around it by the list's node: what an estimated end leaves of it goes
    /// to them (D479).
    scroll_outer: RefCell<Option<Outer>>,
}

/// The nested-scroll parents a delta came through, the list's axis, whether it runs against the
/// pointer, and the delta's source.
type Outer = (compose_ui::platform::NestedScrollChain, bool, bool, compose_ui::NestedScrollSource);

/// What's told of a list's measurements: a pager, whose pages and position depend on its size.
trait ListObserver {
    /// With the sizes known, before the position is read.
    #[allow(clippy::too_many_arguments)]
    fn before_measure(
        &self,
        list: &LazyListInner,
        items: &Items,
        density: compose_ui::Density,
        viewport: i32,
        before: i32,
        after: i32,
        spacing: i32,
    );
    /// With what's laid out known.
    fn after_measure(&self);
    /// `used` pixels were asked of the list, for the next measurement.
    fn after_dispatch(&self, used: f32);
    /// What semantics tell of it, as `LazyLayoutSemanticState.collectionInfo`.
    fn collection_info(&self) -> compose_ui::CollectionInfo;
    /// Scrolls to `index` for a service, as `LazyLayoutSemanticState.scrollToItem`.
    fn scroll_to_index(&self, index: usize);
}

impl LazyListInner {
    /// Goes to `index`, `offset` pixels past its start, in this measurement.
    fn jump_to(&self, index: usize, offset: f32) {
        self.position.set((index, offset));
        self.first_key.set(None);
    }

    /// `can_scroll_*` after a measurement found how far it can scroll: their readers hear when they
    /// change, and only then.
    fn publish_can_scroll(&self) {
        let (forward, backward) = (self.room_forward.get() > 0.0, self.room_backward.get() > 0.0);
        let (old_forward, old_backward) = self.can_scroll_written.replace((forward, backward));
        if forward != old_forward {
            self.can_scroll_forward.set(forward);
        }
        if backward != old_backward {
            self.can_scroll_backward.set(backward);
        }
    }

    /// Measures the list again, with no read of the state (what asks while composing isn't told
    /// of each later scroll).
    fn request_remeasure(&self) {
        let count = self.remeasure_written.get().wrapping_add(1);
        self.remeasure_written.set(count);
        self.remeasure.set(count);
    }

    /// Notes `items` as those laid out; returns whether they're others than last time.
    /// Whether `items` aren't the items last laid out.
    fn laid_items_changed(&self, items: &Rc<Items>) -> bool {
        !self.laid_items.borrow().as_ref().is_some_and(|laid| Rc::ptr_eq(laid, items))
    }

    /// Other items may say the same of those in view: a caller reading `layout_info` runs its
    /// content again, with new closures but the same keys, where Compose's lambda is memoized, and
    /// hears of nothing new (D1471). Compared only with a reader.
    fn note_laid_items(&self, items: &Rc<Items>) -> bool {
        let mut laid = self.laid_items.borrow_mut();
        let same = match laid.as_ref() {
            Some(laid) if Rc::ptr_eq(laid, items) => return false,
            Some(laid) if self.laid_out.has_reader() => self.laid_out.laying_out(|index| {
                index < laid.count
                    && index < items.count
                    && laid.key(index) == items.key(index)
                    && laid.content_type(index) == items.content_type(index)
            }),
            _ => false,
        };
        *laid = Some(items.clone());
        !same
    }

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
        // A negative offset: before the item, as Compose's; the first layout coerces it.
        let offset = first_visible_item_scroll_offset;
        LazyListInner {
            item_animator: Rc::default(),
            indicator: Default::default(),
            position: Cell::new((first_visible_item_index, offset as f32)),
            first_key: Cell::new(None),
            pending: Cell::new(0.0),
            rtl: Cell::new(false),
            remeasure: mutable_state_of(0),
            remeasure_written: Cell::new(0),
            composed: mutable_state_of((first_visible_item_index, first_visible_item_index + 1)),
            composed_read: Cell::new((first_visible_item_index, first_visible_item_index + 1)),
            compose_again: mutable_state_of(0),
            pinned: mutable_state_of(Vec::new()),
            pinned_near: Cell::new(0),
            had_items: Cell::new(false),
            composed_more: Cell::new(false),
            children: RefCell::new(Vec::new()),
            first_index: mutable_state_of(first_visible_item_index),
            first_offset: mutable_state_of(offset),
            published: Cell::new((first_visible_item_index, offset)),
            sizes: RefCell::default(),
            line_sizes: RefCell::default(),
            layout_info: mutable_state_of((0, 1.0, 0, 0, 0)),
            item_constraints: mutable_state_of(None),
            layout_info_written: Cell::new((0, 1.0, 0, 0, 0)),
            item_constraints_written: Cell::new(None),
            room_backward: Cell::new(if first_visible_item_index > 0 {
                f32::INFINITY
            } else {
                offset.max(0) as f32
            }),
            room_forward: Cell::new(f32::INFINITY),
            room_estimated: Cell::new((false, false)),
            can_scroll_forward: mutable_state_of(false),
            can_scroll_backward: mutable_state_of(false),
            can_scroll_written: Cell::new((false, false)),
            asked: Cell::new(0),
            overshoot: RefCell::new(None),
            prefetch: Prefetch::new(),
            parent_size: Rc::default(),
            sticking_size: Cell::new(0),
            cross_size: Cell::new(0),
            cross_cells: mutable_state_of(1),
            cross_cells_written: Cell::new(1),
            cross_cells_seeded: Cell::new(false),
            sticky: mutable_state_of(None),
            sticky_written: Cell::new(None),
            beyond_bounds: mutable_state_of((0, 0)),
            beyond_viewport: Cell::new(0),
            is_grid: Cell::new(false),
            observer: RefCell::new(None),
            beyond_more: Cell::new((false, false)),
            beyond_ended: Cell::new(false),
            dispose_leaving: Cell::new(false),
            core: crate::gestures::ScrollCore::new(),
            laid_out: layout_info::LaidOut::new(),
            density: Cell::new(1.0),
            laid_items: RefCell::new(None),
            scroll_outer: RefCell::new(None),
        }
    }
}

/// A delta taken past an estimated end with nested-scroll parents around (D479): the parents,
/// the axis and whether the list runs against the pointer, where the list was before it, and
/// the pixels asked since.
struct Overshoot {
    chain: compose_ui::platform::NestedScrollChain,
    vertical: bool,
    reverse: bool,
    from: (usize, f32),
    /// The first visible item's key then: `from` follows it when items come and go before it
    /// (D488).
    from_key: Option<u64>,
    asked: f32,
    /// `UserInput` (a drag, a wheel), or `SideEffect` (a fling's scroll, or a child's fling
    /// going on through this list as its parent), as Compose's post-scroll says (D493).
    source: compose_ui::NestedScrollSource,
}

impl Overshoot {
    /// Where it started, found again by its key among `items`.
    fn start(&self, items: &Items) -> (usize, f32) {
        let index =
            self.from_key.and_then(|key| items.index_of(key, self.from.0)).unwrap_or(self.from.0);
        (index, self.from.1)
    }

    /// Offering its parents what wasn't used, `moved` of what was asked: their post-scroll,
    /// to run once the layout is over; `None` if nothing is left.
    fn rest(self, moved: f32) -> Option<Box<dyn FnOnce()>> {
        let left = self.asked - moved;
        if left * self.asked <= 0.0 || left.abs() < 0.5 {
            return None;
        }
        Some(Box::new(move || {
            // The pointer's way: the content's other way.
            let along = if self.reverse { left } else { -left };
            let available = if self.vertical {
                compose_ui::Offset::new(0.0, along)
            } else {
                compose_ui::Offset::new(along, 0.0)
            };
            self.chain.post_scroll(compose_ui::Offset::ZERO, available, self.source);
        }))
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

    /// The cache window its list uses, if composed.
    #[doc(hidden)]
    pub fn cache_window(&self) -> Option<Rc<dyn LazyLayoutCacheWindow>> {
        self.inner()?.prefetch.cache_window()
    }

    /// Measures the list again, as Compose's `Remeasurement.forceRemeasure`, for tests: what the
    /// window let go since the last measurement goes.
    #[doc(hidden)]
    pub fn force_remeasure(&self) {
        if let Some(inner) = self.inner() {
            inner.prefetch.measurement_forced();
            inner.request_remeasure();
        }
    }

    /// The window's units with requests under way, for tests, whatever they request.
    #[doc(hidden)]
    pub fn prefetch_units_held(&self) -> Vec<usize> {
        self.inner().map_or_else(Vec::new, |inner| inner.prefetch.units_held())
    }

    /// The window's requests under way, for tests: their items, and whether they are urgent.
    #[doc(hidden)]
    pub fn prefetch_requests(&self) -> Vec<(usize, bool)> {
        self.inner().map_or_else(Vec::new, |inner| inner.prefetch.requests())
    }

    /// The first item in view, as `firstVisibleItemIndex` (a state read).
    pub fn first_visible_item_index(&self) -> usize {
        self.inner().map_or(0, |inner| inner.first_index.get())
    }

    /// How many pixels of it are scrolled off, as `firstVisibleItemScrollOffset` (a state read).
    pub fn first_visible_item_scroll_offset(&self) -> i32 {
        self.inner().map_or(0, |inner| inner.first_offset.get())
    }

    /// What's scrolled since the last layout, for what reads its offsets before the next.
    pub(crate) fn pending_scroll(&self) -> f32 {
        self.inner().map_or(0.0, |inner| inner.pending.get())
    }

    /// `scroll_boxed`, the future holding what it needs.
    fn scroll_owned<'a>(
        self,
        priority: MutatePriority,
        block: ScrollBlock<'a>,
    ) -> LocalBoxFuture<'a, Result<(), MutationInterrupted>> {
        let Some(inner) = self.inner() else { return Box::pin(async { Ok(()) }) };
        let state = self;
        // A drag's or a fling's deltas learn the parents around from the node (D479).
        let consume = Rc::new(move |delta: f32| {
            let outer = state.inner().and_then(|inner| inner.scroll_outer.borrow().clone());
            state.dispatch(delta, outer)
        });
        Box::pin(async move { inner.core.scroll(priority, consume, block).await })
    }

    fn set_observer(&self, observer: Rc<dyn ListObserver>) {
        if let Some(inner) = self.inner() {
            *inner.observer.borrow_mut() = Some(observer);
        }
    }

    fn set_beyond_viewport(&self, count: usize) {
        if let Some(inner) = self.inner()
            && inner.beyond_viewport.replace(count) != count
        {
            inner.request_remeasure();
        }
    }

    fn set_scroll_allowed(&self, allowed: Rc<dyn Fn(f32) -> bool>) {
        if let Some(inner) = self.inner() {
            *inner.prefetch.scroll_allowed.borrow_mut() = Some(allowed);
        }
    }

    fn clear_pending(&self) {
        if let Some(inner) = self.inner() {
            inner.pending.set(0.0);
        }
    }

    fn clear_scroll_position_key(&self) {
        if let Some(inner) = self.inner() {
            inner.first_key.set(None);
        }
    }

    /// Its first item and pixels past its start, read without subscribing.
    fn position_untracked(&self) -> (usize, f32) {
        self.inner().map_or((0, 0.0), |inner| inner.position.get())
    }

    fn is_scroll_in_progress_untracked(&self) -> bool {
        self.inner().is_some_and(|inner| inner.core.is_scroll_in_progress_untracked())
    }

    fn can_scroll_forward_untracked(&self) -> bool {
        self.inner().is_some_and(|inner| inner.can_scroll_written.get().0)
    }

    fn can_scroll_backward_untracked(&self) -> bool {
        self.inner().is_some_and(|inner| inner.can_scroll_written.get().1)
    }

    fn layout_info_untracked(&self) -> LazyListLayoutInfo {
        compose_runtime::snapshots::Snapshot::without_read_observation(|| self.layout_info())
    }

    /// Scrolls to `index` (and `scroll_offset` pixels into it) at the next layout, without
    /// animation, as `requestScrollToItem`: a scroll going on (a fling) is stopped.
    pub fn request_scroll_to_item(&self, index: usize, scroll_offset: i32) {
        let Some(inner) = self.inner() else { return };
        if inner.core.is_scroll_in_progress_untracked() {
            let state = *self;
            CoroutineScope::new().launch(async move {
                let _ = state.stop_scroll(MutatePriority::Default).await;
            });
        }
        self.snap_to_item(index, scroll_offset);
    }

    /// Moves the position to `index`, `scroll_offset` into it (a negative offset: before it,
    /// as Compose's), for the next layout.
    pub(crate) fn snap_to_item(&self, index: usize, scroll_offset: i32) {
        let Some(inner) = self.inner() else { return };
        // A jump is no user input: nothing it passes goes to nested-scroll parents.
        inner.overshoot.borrow_mut().take();
        if inner.published.get() != (index, scroll_offset) {
            inner.prefetch.reset_window();
            // A jump isn't a change of its items' places to animate.
            inner.item_animator.reset();
        }
        inner.prefetch.delta.set(0.0);
        inner.position.set((index, scroll_offset as f32));
        inner.first_key.set(None);
        inner.pending.set(0.0);
        inner.room_backward.set(f32::INFINITY);
        inner.room_forward.set(f32::INFINITY);
        // The position is the state's at once, as Compose's `requestPositionAndForgetLastKnownKey`;
        // the layout coerces it into the items.
        inner.published.set((index, scroll_offset));
        inner.first_index.set(index);
        inner.first_offset.set(scroll_offset);
        inner.request_remeasure();
    }

    /// Scrolls to `index` (and `scroll_offset` pixels into it), without animation, in a scroll
    /// of the default priority, as `scrollToItem`: the list is there from its next layout.
    pub async fn scroll_to_item(
        &self,
        index: usize,
        scroll_offset: i32,
    ) -> Result<(), MutationInterrupted> {
        let state = *self;
        self.scroll(MutatePriority::Default, move |_| async move {
            state.snap_to_item(index, scroll_offset);
        })
        .await
    }

    /// Animates a scroll to `index`, `scroll_offset` pixels into it, in a scroll of the default
    /// priority, as `animateScrollToItem`: a spring towards it, 2,500 dp at a time, jumping to
    /// a hundred items before it when it's further, then to its offset.
    pub async fn animate_scroll_to_item(
        &self,
        index: usize,
        scroll_offset: i32,
    ) -> Result<(), MutationInterrupted> {
        let state = *self;
        self.scroll(MutatePriority::Default, move |scope| async move {
            let density = state.inner().map_or(1.0, |inner| inner.density.get());
            let scope = state.lazy_layout_scroll_scope(scope);
            let animator = state.inner().map(|inner| inner.item_animator.clone());
            let _skipping = animator.map(item_animator::SkipAnimation::new);
            animate_scroll::animate_scroll_to_item(&scope, index, scroll_offset, 100, density)
                .await;
        })
        .await
    }

    /// A lazy layout scroll scope over this list and a scroll's `scroll_scope`, as Compose's
    /// `LazyLayoutScrollScope(state, scrollScope)`.
    pub fn lazy_layout_scroll_scope(
        &self,
        scroll_scope: ScrollScope,
    ) -> impl LazyLayoutScrollScope {
        ListScrollScope { state: *self, scope: scroll_scope }
    }

    /// The interaction source the list reports its drags to, as `interactionSource`.
    pub fn interaction_source(&self) -> compose_ui::MutableInteractionSource {
        self.inner().map_or_else(compose_ui::MutableInteractionSource::new, |inner| {
            inner.core.interaction_source.clone()
        })
    }

    /// Scrolls by up to `pixels`, as far as the list goes (as last laid out); returns the
    /// pixels it will move. Where the end isn't laid out, the room is an estimate, which never
    /// cuts the delta: the measurement takes it all, composing in the frame what it reaches,
    /// and stops at the end it finds (D476); what it couldn't use then goes to the nested-scroll
    /// parents `outer` names (the axis, and whether the list runs against the pointer), as
    /// Compose's post-scroll (D479).
    fn dispatch(
        &self,
        pixels: f32,
        outer: Option<(
            compose_ui::platform::NestedScrollChain,
            bool,
            bool,
            compose_ui::NestedScrollSource,
        )>,
    ) -> f32 {
        let Some(inner) = self.inner() else { return 0.0 };
        let (backward_estimated, forward_estimated) = inner.room_estimated.get();
        let (room, estimated) = if pixels > 0.0 {
            (inner.room_forward.get(), forward_estimated)
        } else {
            (inner.room_backward.get(), backward_estimated)
        };
        let used =
            if estimated { pixels } else { pixels.signum() * pixels.abs().min(room.max(0.0)) };
        if estimated && let Some((chain, vertical, reverse, source)) = outer {
            let mut overshoot = inner.overshoot.borrow_mut();
            match &mut *overshoot {
                Some(overshoot) => overshoot.asked += used,
                None => {
                    *overshoot = Some(Overshoot {
                        chain,
                        vertical,
                        reverse,
                        from: inner.position.get(),
                        from_key: inner.first_key.get(),
                        asked: inner.pending.get() + used,
                        source,
                    })
                }
            }
        }
        if used != 0.0 {
            // An estimated room stays one (still more that way until laid out): what reads it
            // (`can_scroll_forward`) isn't told the list ended where it may not.
            let left = |room: f32, estimated: bool| if estimated { room.max(1.0) } else { room };
            inner.room_forward.set(left(inner.room_forward.get() - used, forward_estimated));
            inner.room_backward.set(left(inner.room_backward.get() + used, backward_estimated));
            inner.pending.set(inner.pending.get() + used);
            // Prefetch goes the way the list was scrolled (not the way a layout steps back),
            // as Compose's strategy hears of each scroll's delta (negative towards the end).
            inner.prefetch.scroll_asked.set(true);
            let scrolled = inner.prefetch.delta.get() - used;
            inner.prefetch.delta.set(scrolled);
            // Less than half a pixel is no scroll to prefetch yet, as Compose's `onScroll`.
            if scrolled.abs() > 0.5 {
                inner.prefetch.forward.set(scrolled < 0.0);
            }
            inner.request_remeasure();
            if let Some(observer) = inner.observer.borrow().clone() {
                observer.after_dispatch(used);
            }
        }
        used
    }

    /// What the list laid out last, as `layoutInfo`: the items in view, the viewport and the
    /// padding (a state read: a layout that changes them tells the reader).
    pub fn layout_info(&self) -> LazyListLayoutInfo {
        let Some(inner) = self.inner() else { return LazyListLayoutInfo::default() };
        let items = inner.laid_items.borrow().clone();
        inner.laid_out.read(|laid, viewport| LazyListLayoutInfo {
            visible_items_info: laid
                .iter()
                .map(|item| LazyListItemInfo {
                    index: item.index,
                    key: items.as_ref().map_or(0, |items| items.key(item.index)),
                    offset: item.main,
                    size: item.main_size,
                    content_type: items.as_ref().and_then(|items| items.content_type(item.index)),
                    non_scrollable: item.sticky,
                })
                .collect(),
            viewport_start_offset: viewport.start,
            viewport_end_offset: viewport.end,
            total_items_count: viewport.count,
            viewport_size: viewport.size,
            orientation: if viewport.vertical {
                crate::gestures::Orientation::Vertical
            } else {
                crate::gestures::Orientation::Horizontal
            },
            reverse_layout: viewport.reverse,
            before_content_padding: viewport.before,
            after_content_padding: viewport.after,
            main_axis_item_spacing: viewport.spacing,
        })
    }

    pub(crate) fn scroll_estimates(&self) -> (usize, f32, i32, i32, i32) {
        self.inner().map_or((0, 1.0, 0, 0, 0), |inner| inner.layout_info.get())
    }
}

impl ScrollableState for LazyListState {
    fn scroll_indicator_state(&self) -> Option<std::rc::Rc<dyn crate::ScrollIndicatorState>> {
        let inner = self.inner()?;
        Some(inner.indicator.get_or_init(|| Rc::new(*self)).clone())
    }

    fn scroll_boxed<'a>(
        &'a self,
        priority: MutatePriority,
        block: ScrollBlock<'a>,
    ) -> LocalBoxFuture<'a, Result<(), MutationInterrupted>> {
        self.scroll_owned(priority, block)
    }

    /// Scrolls by up to `delta` pixels (positive: towards the end) at the next layout, as far
    /// as the list goes as last laid out (an end not laid out isn't known: it takes it all,
    /// D476); returns the pixels it will move.
    fn dispatch_raw_delta(&self, delta: f32) -> f32 {
        self.dispatch(delta, None)
    }

    fn is_scroll_in_progress(&self) -> bool {
        self.inner().is_some_and(|inner| inner.core.is_scroll_in_progress())
    }

    /// Whether the list can scroll towards its end, as `canScrollForward` (a state read, as last
    /// laid out).
    fn can_scroll_forward(&self) -> bool {
        self.inner().is_some_and(|inner| inner.can_scroll_forward.get())
    }

    /// Whether the list can scroll towards its start, as `canScrollBackward` (a state read).
    fn can_scroll_backward(&self) -> bool {
        self.inner().is_some_and(|inner| inner.can_scroll_backward.get())
    }

    fn last_scrolled_forward(&self) -> bool {
        self.inner().is_some_and(|inner| inner.core.last_scrolled_forward())
    }

    fn last_scrolled_backward(&self) -> bool {
        self.inner().is_some_and(|inner| inner.core.last_scrolled_backward())
    }

    fn internal_interaction_source(&self) -> Option<compose_ui::MutableInteractionSource> {
        self.inner().map(|inner| inner.core.interaction_source.clone())
    }
}

/// A list's lazy layout scroll scope, as the one Compose's `LazyLayoutScrollScope(state, …)`
/// makes for a `LazyListState`. Compose measures a scroll at once; compose-rs at the next
/// layout (D476), so this scope sees the last layout's items moved by the scrolling asked since,
/// as Compose's `copyWithScrollDeltaWithoutRemeasure` does for a small scroll (D858).
struct ListScrollScope {
    state: LazyListState,
    scope: ScrollScope,
}

impl ListScrollScope {
    /// `read` of the position ahead, the items in view as last laid out and the scrolling asked
    /// since (`None` before a layout with items).
    fn ahead<R>(
        &self,
        read: impl FnOnce(&layout_info::Ahead, &[layout_info::LaidItem], f32) -> R,
    ) -> Option<R> {
        let inner = self.state.inner()?;
        let pending = inner.pending.get();
        inner.laid_out.read_untracked(|laid, viewport| {
            let lines = layout_info::laid_lines(laid, false);
            let ahead = layout_info::ahead(
                &lines,
                pending,
                viewport.spacing,
                viewport.end,
                viewport.count,
                1,
            )?;
            Some(read(&ahead, laid, pending))
        })
    }
}

impl LazyLayoutScrollScope for ListScrollScope {
    fn scroll_scope(&self) -> &ScrollScope {
        &self.scope
    }

    fn first_visible_item_index(&self) -> usize {
        self.ahead(|ahead, _, _| ahead.first)
            .unwrap_or_else(|| self.state.first_visible_item_index())
    }

    fn first_visible_item_scroll_offset(&self) -> i32 {
        self.ahead(|ahead, _, _| ahead.offset)
            .unwrap_or_else(|| self.state.first_visible_item_scroll_offset())
    }

    fn last_visible_item_index(&self) -> usize {
        self.ahead(|ahead, _, _| ahead.last).unwrap_or(0)
    }

    fn item_count(&self) -> usize {
        let Some(inner) = self.state.inner() else { return 0 };
        inner.laid_out.read_untracked(|_, viewport| viewport.count)
    }

    fn snap_to_item(&self, index: usize, offset: i32) {
        self.state.snap_to_item(index, offset);
    }

    fn calculate_distance_to(&self, target_index: usize, target_offset: i32) -> i32 {
        self.ahead(|ahead, laid, pending| {
            let visible = (ahead.first..=ahead.last).contains(&target_index);
            let item = laid.iter().find(|item| item.index == target_index && !item.sticky);
            let distance = match item {
                Some(item) if visible => (item.main as f32 - pending).round() as i32,
                _ => ahead.average * (target_index as i32 - ahead.first as i32) - ahead.offset,
            };
            distance + target_offset
        })
        .unwrap_or(0)
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
    type Node = Self;
    fn create(&self) -> Self {
        self.clone()
    }
    fn update(&self, node: &mut Self) {
        *node = self.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&other.parent, &self.parent)
            && other.width == self.width
            && other.height == self.height
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
    /// The interval found last, looked at first, as `MutableIntervalList.lastInterval`: a walk
    /// over the items, as a grid's spans, stays in it for a while.
    last_found: Cell<usize>,
    count: usize,
    /// The sticky headers' indices, in order.
    headers: Vec<usize>,
    /// Some items have spans (a grid's): its lines are found by walking the items.
    has_custom_spans: bool,
    /// The keys of keyed intervals asked for, by index modulo its length: a key function runs
    /// once per item while the items stay, as Compose's key-index map calls it (the composition,
    /// the layout and the layout info ask for a key several times a frame); one that read state
    /// isn't kept. Empty with no key function: a default key is a hash of its index.
    keys: Box<[Cell<(usize, u64)>]>,
}

/// How many keys `Items` keeps at most: more than the items in view of most lists; made per run of
/// the content lambda, as few as the list has (D886).
const KEY_CACHE: usize = 64;

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
        let keyed = intervals.iter().any(|interval| interval.key.is_some());
        let cached = if keyed { count.min(KEY_CACHE) } else { 0 };
        let keys = (0..cached).map(|_| Cell::new((usize::MAX, 0))).collect();
        Items {
            intervals,
            starts,
            last_found: Cell::new(0),
            count,
            headers,
            has_custom_spans,
            keys,
        }
    }

    /// The interval holding item `index`, and the item's index in it.
    fn find(&self, index: usize) -> Option<(&Interval, usize)> {
        if index >= self.count {
            return None;
        }
        let last = self.last_found.get();
        let in_last = self.starts[last] <= index
            && self.starts.get(last + 1).is_none_or(|next| index < *next);
        let interval =
            if in_last { last } else { self.starts.partition_point(|start| *start <= index) - 1 };
        self.last_found.set(interval);
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
        let default = || hash_key(&DefaultKey(index));
        let Some(cached) = self.keys.get(index.checked_rem(self.keys.len()).unwrap_or(0)) else {
            return default();
        };
        if let (at, key) = cached.get()
            && at == index
        {
            return key;
        }
        let Some((key, local)) =
            self.find(index).and_then(|(interval, local)| Some((interval.key.as_ref()?, local)))
        else {
            return default();
        };
        // A key function reading state isn't kept: its keys follow the state, read each time
        // (observed by whoever asks), as Compose's key map is a derived state of what they read.
        let reads = compose_runtime::platform::state_reads();
        let key = key(local);
        if compose_runtime::platform::state_reads() == reads {
            cached.set((index, key));
        }
        key
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
    stacking: ItemStacking,
) {
    let composer = compose_runtime::platform::current();
    match content {
        SlotContent::Item(index) => {
            composer.start_reusable_group(SLOT_GROUP, slot.key);
            LazyItem(index, state, items.clone(), holder, stacking);
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
    /// `composed` range's items went. The range moves by as much as the first visible item
    /// (`anchor`: its key and the index it was at) moved, found by key: the layout follows that
    /// key (Compose's position follows `firstVisibleItem`'s), so the range composed is the one
    /// the next measurement wants (D458). It's widened to hold the other composed items found
    /// near it (each looked for first where the one before it went, else within the key
    /// window, `MOST_KEY_SEARCHES` times at most); when that cap leaves some unsearched, it
    /// holds the old range too, so `assign` still finds them by key. Those items stay composed,
    /// rather than leaving (deactivated, their state lost) for a measurement to ask for them
    /// again, as Compose's measurement finds each item's composition by its key (D453, D455).
    /// `composed` may hold indices past the new count (items removed). `None` if the items are
    /// the same, or the range is the same.
    /// The items composed apart from the range, `apart`, where their keys went among new `items`.
    fn follow_apart(&self, items: &Rc<Items>, apart: &mut Vec<usize>) {
        if std::ptr::eq(self.items.as_ptr(), Rc::as_ptr(items)) {
            return;
        }
        let keys: HashMap<usize, u64> =
            self.active.iter().map(|slot| (slot.index, slot.key)).collect();
        let mut moved = false;
        for item in apart.iter_mut() {
            if let Some(key) = keys.get(item)
                && let Some(found) = items.index_of(*key, *item)
                && found != *item
            {
                *item = found;
                moved = true;
            }
        }
        if moved {
            apart.sort_unstable();
            apart.dedup();
        }
    }

    fn follow(
        &self,
        items: &Rc<Items>,
        composed: (usize, usize),
        anchor: Option<(u64, usize)>,
    ) -> Option<(usize, usize)> {
        if std::ptr::eq(self.items.as_ptr(), Rc::as_ptr(items)) {
            return None;
        }
        let count = items.count;
        let (start, end) = composed;
        let find = |key: u64, guess: Option<usize>, near: usize, searches: &mut usize| match guess
            .filter(|guess| *guess < count)
        {
            Some(guess) if items.key(guess) == key => Some(guess),
            _ if *searches < MOST_KEY_SEARCHES => {
                *searches += 1;
                items.index_of(key, near.min(count.saturating_sub(1)))
            }
            _ => None,
        };
        let mut searches = 0;
        // The first visible item's move (none if it went: the range stays where it was).
        let anchor_shift = anchor
            .and_then(|(key, old)| {
                let index = find(key, Some(old), old, &mut searches)?;
                Some(index as isize - old as isize)
            })
            .unwrap_or(0);
        let moved = |index: usize| index.saturating_add_signed(anchor_shift).min(count);
        let (moved_start, moved_end) = (moved(start), moved(end));
        let near = (end - start).max(1);
        let (mut low, mut high) = (moved_start, moved_end);
        let mut shift = anchor_shift;
        let mut unsearched = false;
        for slot in &self.active {
            let old = slot.index;
            // A pinned item or a header out of the range: kept by key already.
            if old < start || old >= end {
                continue;
            }
            let guess = old.checked_add_signed(shift);
            let searched = searches < MOST_KEY_SEARCHES;
            match find(slot.key, guess, old, &mut searches) {
                Some(index) => {
                    shift = index as isize - old as isize;
                    if index + near >= moved_start && index < moved_end + near {
                        low = low.min(index);
                        high = high.max(index + 1);
                    }
                }
                None => unsearched |= !searched,
            }
        }
        if unsearched {
            low = low.min(start.min(count));
            high = high.max(end.min(count));
        }
        let followed = (low, high.min(count));
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
        measured: impl Fn(&ItemSlot) -> bool,
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
            if !taken[position] && measured(slot) {
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
/// as Compose's, and so does its default: from the top, from the bottom when reversed.
/// `user_scroll_enabled = false` keeps drags and the wheel from scrolling it (the state still
/// does).
#[composable(non_restartable)]
#[allow(non_snake_case)]
pub fn LazyColumn(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::remember_lazy_list_state(0, 0))] state: LazyListState,
    #[default(crate::layout::PaddingValues::ZERO)] content_padding: PaddingValues,
    #[default(false)] reverse_layout: bool,
    #[default(None)] vertical_arrangement: impl Into<Option<VerticalArrangement>>,
    #[default(crate::Alignment::START)] horizontal_alignment: HorizontalAlignment,
    #[default(None)] fling_behavior: Option<Rc<dyn crate::gestures::FlingBehavior>>,
    #[default(crate::remember_overscroll_effect())] overscroll_effect: Option<
        Rc<dyn crate::OverscrollEffect>,
    >,
    #[default(true)] user_scroll_enabled: bool,
    #[default(None)] cache_window: Option<Rc<dyn LazyLayoutCacheWindow>>,
    content: impl Fn(LazyListScope) + 'static,
) {
    LazyList(
        modifier,
        state,
        content_padding,
        true,
        reverse_layout,
        // As Compose's default: from the top, from the bottom when reversed.
        vertical_arrangement
            .into()
            .unwrap_or(if reverse_layout {
                crate::layout::Arrangement::BOTTOM
            } else {
                crate::layout::Arrangement::TOP
            })
            .0,
        Alignment::new(horizontal_alignment.bias, -1.0),
        fling_behavior,
        overscroll_effect,
        user_scroll_enabled,
        cache_window,
        Modifier,
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
    #[default(None)] horizontal_arrangement: impl Into<Option<HorizontalArrangement>>,
    #[default(crate::Alignment::TOP)] vertical_alignment: VerticalAlignment,
    #[default(None)] fling_behavior: Option<Rc<dyn crate::gestures::FlingBehavior>>,
    #[default(crate::remember_overscroll_effect())] overscroll_effect: Option<
        Rc<dyn crate::OverscrollEffect>,
    >,
    #[default(true)] user_scroll_enabled: bool,
    #[default(None)] cache_window: Option<Rc<dyn LazyLayoutCacheWindow>>,
    content: impl Fn(LazyListScope) + 'static,
) {
    LazyList(
        modifier,
        state,
        content_padding,
        false,
        reverse_layout,
        // As Compose's default: from the start, from the end when reversed.
        horizontal_arrangement
            .into()
            .unwrap_or(if reverse_layout {
                crate::layout::Arrangement::END
            } else {
                crate::layout::Arrangement::START
            })
            .0,
        Alignment::new(-1.0, vertical_alignment.bias),
        fling_behavior,
        overscroll_effect,
        user_scroll_enabled,
        cache_window,
        Modifier,
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
    fling_behavior: Option<Rc<dyn crate::gestures::FlingBehavior>>,
    overscroll_effect: Option<Rc<dyn crate::OverscrollEffect>>,
    user_scroll_enabled: bool,
    cache_window: Option<Rc<dyn LazyLayoutCacheWindow>>,
    inner_modifier: Modifier,
    content: impl Fn(LazyListScope) + 'static,
) {
    let inner = state.inner_or_detached();
    let owner = compose_runtime::remember(|| Rc::new(()));
    inner.prefetch.use_window(&inner, cache_window, &owner);
    // In an item a request composes, the list is one to prefetch the first items of (Compose's
    // nested prefetch).
    prefetch::note_nested(&inner);
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
    let modifier = modifier
        .clip(compose_ui::RECTANGLE_SHAPE)
        .overscroll(overscroll_effect.clone())
        .then_custom(LazyScrollElement {
            state,
            vertical,
            reverse,
            enabled: user_scroll_enabled,
            fling_behavior,
            overscroll: overscroll_effect,
        })
        // Its items a group for focus searches, as Compose's `scrollingContainer`.
        .focus_group()
        .then(inner_modifier);
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
        compose_runtime::callback(move || {
            let stacking = ItemStacking::Along { vertical, reverse };
            LazyItems(state, items.clone(), holder, fallback, stacking)
        }),
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
    stacking: ItemStacking,
) {
    // Each item in a slot of its own, which an item of its content type reuses once it left
    // (D430). Kept when the list is in content reused (an outer list's item reused for
    // another): its items are reused too, as Compose keeps a reused node's
    // `SubcomposeLayoutState`.
    let slots = compose_runtime::platform::remember_across_reuse(|| {
        Rc::new(RefCell::new(ItemSlots::default()))
    });
    let inner = state.inner_or_detached();
    // A completion's item (`complete_pending`), noted for the completion before this pass: not
    // the item of an inner scope restarted in this one.
    inner.prefetch.composing_item.set(None);
    let (mut start, mut end) = inner.composed.get();
    inner.compose_again.get();
    inner.composed_read.set((start, end));
    let count = items.0.count;
    // Items came, went or moved: those composed stay composed where they went.
    if inner.had_items.get()
        && let Some(followed) = slots.borrow().follow(
            &items.0,
            (start, end),
            inner.first_key.get().map(|key| (key, inner.position.get().0)),
        )
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
    for pinned in inner.pinned.get() {
        if let Some(index) = items.0.index_of(pinned, inner.pinned_near.get()) {
            inner.pinned_near.set(index);
            if !indices.contains(&index) {
                indices.push(index);
            }
        }
    }
    // The sticky header over the first visible item, wherever it is.
    if let Some(header) = inner.sticky.get()
        && header < count
        && !indices.contains(&header)
    {
        indices.push(header);
    }
    // Those moving out of view with `animate_item`, by key (gone with their items).
    if inner.item_animator.has_kept() {
        for &(kept, near) in inner.item_animator.kept().iter() {
            if let Some(index) = items.0.index_of(kept, near)
                && !indices.contains(&index)
            {
                indices.push(index);
            }
        }
    }
    // Those composed apart from the range: before a sticking header (D895), a staggered grid's
    // (D1470), where their items went if the items changed. Each once in `detached`, so a search
    // through what's before them alone.
    inner.prefetch.detached_dirty.set(false);
    {
        let mut detached = inner.prefetch.detached.borrow_mut();
        if !detached.is_empty() {
            slots.borrow().follow_apart(&items.0, &mut detached);
        }
        let (range_start, range_end) = (start.min(count), end.min(count));
        let (in_range, others) =
            ((range_end - range_start.min(range_end)).min(indices.len()), indices.len());
        for &item in detached.iter() {
            if item < count
                && !(range_start..range_end).contains(&item)
                && !indices[in_range..others].contains(&item)
            {
                indices.push(item);
            }
        }
    }
    #[cfg(debug_assertions)]
    items.0.check_keys(&indices);
    // Other items composed: the list measures again. A slot can take another item without its
    // node moving or its size changing (an empty item after another), which wouldn't measure
    // the list again by itself.
    if *inner.children.borrow() != indices
        && let Some(list) = inner.prefetch.node.get()
    {
        compose_runtime::platform::current()
            .with_applier(|tree: &mut compose_ui::platform::UiTree| tree.invalidate_measure(list));
    }
    *inner.children.borrow_mut() = indices.clone();
    // The items' constraints, for those composing for theirs (`BoxWithConstraints`).
    let hint = inner.item_constraints.get().or(fallback);
    drop(inner);
    let composed = {
        let inner = state.inner_or_detached();
        let sizes = inner.sizes.borrow();
        let keep = !inner.dispose_leaving.replace(false);
        // Measured, or composed by a prefetch request (Compose's precomposed slot goes to the
        // reuse pool when its request is cancelled).
        slots.borrow_mut().assign(&items.0, &indices, |slot| {
            // The slot's own item: its key, its index when last composed.
            let precomposed = inner.prefetch.take_precomposed(slot.key);
            keep && (sizes.contains_key(slot.index) || precomposed)
        })
    };
    slots.borrow_mut().items = Rc::downgrade(&items.0);
    let parent = {
        let inner = state.inner_or_detached();
        *inner.prefetch.items.borrow_mut() = Rc::downgrade(&items.0);
        // One per list, kept: the local stays the same, its readers aren't invalidated; its
        // list is the state's now.
        let parent = compose_runtime::platform::remember_across_reuse(|| {
            Rc::new(prefetch::ParentList::new(&inner))
        });
        parent.set(&inner);
        parent
    };
    compose_runtime::CompositionLocalProvider(
        compose_ui::platform::LOCAL_CONSTRAINTS_HINT.provides(hint),
        || {
            compose_runtime::CompositionLocalProvider(
                prefetch::LOCAL_PARENT_LIST.provides(Some(parent)),
                || {
                    // The slots holding items first, in the children's order the layout reads;
                    // then those kept for reuse, deactivated: their nodes stay, unmeasured and
                    // unplaced. One loop, so a slot's group moves between the two.
                    for &(slot, content) in &composed {
                        key(slot.id, || {
                            compose_slot(slot, content, state, &items, holder, stacking)
                        });
                    }
                },
            );
        },
    );
}

/// One item: a layout node around its content, which pins the item while it holds the focus.
/// Its content is in a `SaveableStateProvider` of its key: scrolled away and back, it gets its
/// saveable state back (inside the item, which skips when it stays: a pass that only scrolls
/// doesn't run the provider again).
#[composable]
#[allow(non_snake_case)]
fn LazyItem(
    index: usize,
    state: LazyListState,
    items: ItemsRef,
    holder: SaveableStateHolder,
    stacking: ItemStacking,
) {
    let inner = state.inner_or_detached();
    let policy = compose_runtime::remember_keyed((stacking, state), || {
        Rc::new_cyclic(|this| ItemLayout {
            this: this.clone(),
            stacking,
            animator: inner.item_animator.clone(),
            children: ItemChildren::new(),
        })
    });
    let item_key = items.0.key(index);
    inner.item_animator.item_composed(&policy, item_key);
    // The lazy lists composing in the item note themselves in this list under its key, whoever
    // composes it (a frame here, a request's resumes, a completion), as Compose's request finds
    // those of its precomposed item (`traversablePrefetchState`).
    let composing = inner.prefetch.composing_item.replace(Some(item_key));
    let target = prefetch::item_target(&inner, item_key);
    // What's in it pins it while it has the focus, as `LazyLayoutPinnableItem`.
    let parent = compose_ui::LOCAL_PINNABLE_CONTAINER.current();
    let pin = compose_runtime::remember_keyed((item_key, state), || {
        Rc::new_cyclic(|this| ItemPin {
            this: this.clone(),
            key: item_key,
            near: Cell::new(index),
            pinned: inner.pinned.handle(),
            state,
            parent: RefCell::new(None),
            parent_handle: RefCell::new(None),
            count: Cell::new(0),
        })
    });
    pin.near.set(index);
    pin.set_parent(parent);
    let container: Rc<dyn compose_ui::PinnableContainer> = pin.clone();
    // An item is a repaint boundary: scrolled, or beside an animation, it's not recorded again.
    let modifier = Modifier.lazy_item_boundary();
    Layout(
        modifier,
        policy as Rc<dyn MeasurePolicy>,
        compose_runtime::callback(move || {
            if let Some((interval, local)) = items.0.find(index) {
                // The item's content, as Compose's item subcomposition: a request's paused
                // composition may pause in it, and applies it whole (D590).
                compose_runtime::platform::PausableContentFor(target, || {
                    compose_runtime::CompositionLocalProvider(
                        compose_ui::LOCAL_PINNABLE_CONTAINER.provides(Some(container.clone())),
                        || holder.SaveableStateProvider(item_key, || (interval.content)(local)),
                    );
                });
            }
        }),
    );
    inner.prefetch.composing_item.set(composing);
}

/// A lazy item as a pinnable container, as `LazyLayoutPinnableItem`: pinned, its list keeps it
/// composed and placed, and the list's own item around it too. It lets go when it leaves the
/// composition, as `onDisposed`.
struct ItemPin {
    /// For its handles, which do nothing once it has left.
    this: std::rc::Weak<ItemPin>,
    key: u64,
    near: Cell<usize>,
    pinned: compose_runtime::MutableState<Vec<u64>>,
    state: LazyListState,
    parent: RefCell<Option<Rc<dyn compose_ui::PinnableContainer>>>,
    parent_handle: RefCell<Option<compose_ui::PinnedHandle>>,
    /// As `pinsCount`.
    count: Cell<u32>,
}

impl ItemPin {
    /// As `parentPinnableContainer`'s setter: a new parent takes over the pin.
    fn set_parent(&self, parent: Option<Rc<dyn compose_ui::PinnableContainer>>) {
        let same = match (&*self.parent.borrow(), &parent) {
            (None, None) => true,
            (Some(one), Some(other)) => Rc::ptr_eq(one, other),
            _ => false,
        };
        if same {
            return;
        }
        if self.count.get() > 0 {
            let handle = self.parent_handle.borrow_mut().take();
            if let Some(handle) = handle {
                handle.release();
            }
            *self.parent_handle.borrow_mut() = parent.as_ref().map(|parent| parent.pin());
        }
        *self.parent.borrow_mut() = parent;
    }

    fn release_one(&self) {
        let count = self.count.get();
        if count == 0 {
            return;
        }
        self.count.set(count - 1);
        if count == 1 {
            let (key, pinned) = (self.key, self.pinned);
            if let Some(mut keys) = pinned.try_get() {
                keys.retain(|it| *it != key);
                let _ = pinned.try_set(keys);
            }
            let handle = self.parent_handle.borrow_mut().take();
            if let Some(handle) = handle {
                handle.release();
            }
        }
    }
}

impl Drop for ItemPin {
    fn drop(&mut self) {
        while self.count.get() > 0 {
            self.release_one();
        }
    }
}

impl compose_ui::PinnableContainer for ItemPin {
    fn pin(&self) -> compose_ui::PinnedHandle {
        if self.count.get() == 0 {
            if let Some(inner) = self.state.inner() {
                inner.pinned_near.set(self.near.get());
            }
            let mut keys = self.pinned.get();
            if !keys.contains(&self.key) {
                keys.push(self.key);
                self.pinned.set(keys);
            }
            let parent = self.parent.borrow().as_ref().map(|parent| parent.pin());
            *self.parent_handle.borrow_mut() = parent;
        }
        self.count.set(self.count.get() + 1);
        let this = self.this.clone();
        compose_ui::PinnedHandle::new(move || {
            if let Some(this) = this.upgrade() {
                this.release_one();
            }
        })
    }
}

/// How an item's node stacks its children.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum ItemStacking {
    /// At the top start, as large as the largest: a grid's, as Compose's `LazyGridMeasuredItem`.
    AtTheOrigin,
    /// At the end along the axis, as large as the largest: a reversed grid's, as Compose's
    /// `mainAxisLayoutSize - offset - placeable.mainAxisSize`.
    AtTheEnd { vertical: bool },
    /// One after the other along the axis (the last first, reversed), as Compose's
    /// `LazyListMeasuredItem`: a list's.
    Along { vertical: bool, reverse: bool },
}

impl ComposeArg for ItemStacking {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// An item: its children stacked as its layout's `ItemStacking` says.
struct ItemLayout {
    /// What the animator looks its children up through.
    this: Weak<ItemLayout>,
    stacking: ItemStacking,
    animator: Rc<ItemAnimator>,
    children: ItemChildren,
}

impl ItemLayout {
    /// For the animator, when one of its children has the modifier or had it: how they stand in
    /// the item of `width`, and their data.
    #[cold]
    fn note_children(
        &self,
        scope: &MeasureScope<'_>,
        measurables: &[Measurable],
        placeables: &[Placeable],
        width: i32,
        height: i32,
    ) {
        let rtl = scope.layout_direction() == compose_ui::LayoutDirection::Rtl;
        let main_total: i32 = match self.stacking {
            ItemStacking::Along { vertical: true, .. } => {
                placeables.iter().map(|it| it.height).sum()
            }
            ItemStacking::Along { vertical: false, .. } => {
                placeables.iter().map(|it| it.width).sum()
            }
            _ => 0,
        };
        let mut along = 0;
        let mut children = Vec::with_capacity(placeables.len());
        for (measurable, placeable) in measurables.iter().zip(placeables) {
            let (logical, x, y) = match self.stacking {
                ItemStacking::Along { vertical, reverse } => {
                    let size = if vertical { placeable.height } else { placeable.width };
                    let at = if reverse { main_total - along - size } else { along };
                    let logical =
                        if vertical { IntOffset::new(0, along) } else { IntOffset::new(along, 0) };
                    along += size;
                    if vertical { (logical, 0, at) } else { (logical, at, 0) }
                }
                ItemStacking::AtTheOrigin => (IntOffset::ZERO, 0, 0),
                ItemStacking::AtTheEnd { vertical: true } => {
                    (IntOffset::ZERO, 0, height - placeable.height)
                }
                ItemStacking::AtTheEnd { vertical: false } => {
                    (IntOffset::ZERO, width - placeable.width, 0)
                }
            };
            let x = if rtl { width - x - placeable.width } else { x };
            children.push(ChildRecord {
                data: scope
                    .parent_data_any(*measurable)
                    .and_then(|data| data.downcast::<item_animator::AnimateItemData>().ok()),
                logical,
                physical: IntOffset::new(x, y),
                size: placeable.size(),
            });
        }
        self.children.set(children);
        self.animator.register(self.children.key(), self.this.clone());
    }
}

impl MeasurePolicy for ItemLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let placeables: Vec<Placeable> =
            measurables.iter().map(|&measurable| scope.measure(measurable, constraints)).collect();
        // `animate_item` on a child, or had there.
        let animated = measurables.iter().any(|&child| scope.parent_data_any(child).is_some())
            || !self.children.is_empty();
        if let (ItemStacking::Along { vertical, reverse }, [_, _, ..]) =
            (self.stacking, placeables.as_slice())
        {
            let main_of = |placeable: &Placeable| {
                if vertical { placeable.height } else { placeable.width }
            };
            let cross_of = |placeable: &Placeable| {
                if vertical { placeable.width } else { placeable.height }
            };
            let main: i32 = placeables.iter().map(main_of).sum();
            let cross = placeables.iter().map(cross_of).max().unwrap_or(0);
            let (width, height) = if vertical {
                (constraints.constrain_width(cross), constraints.constrain_height(main))
            } else {
                (constraints.constrain_width(main), constraints.constrain_height(cross))
            };
            let mut offset = 0;
            for placeable in &placeables {
                let size = main_of(placeable);
                let at = if reverse { main - offset - size } else { offset };
                let (x, y) = if vertical { (0, at) } else { (at, 0) };
                scope.place_relative(placeable, x, y);
                offset += size;
            }
            if animated {
                self.note_children(scope, measurables, &placeables, width, height);
            }
            return scope.layout(width, height);
        }
        let size = placeables.iter().fold(
            IntSize::new(constraints.min_width, constraints.min_height),
            |size, placeable| {
                IntSize::new(size.width.max(placeable.width), size.height.max(placeable.height))
            },
        );
        for placeable in &placeables {
            let (x, y) = match self.stacking {
                ItemStacking::AtTheEnd { vertical: true } => (0, size.height - placeable.height),
                ItemStacking::AtTheEnd { vertical: false } => (size.width - placeable.width, 0),
                _ => (0, 0),
            };
            scope.place_relative(placeable, x, y);
        }
        if animated {
            self.note_children(scope, measurables, &placeables, size.width, size.height);
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
        let policy = Policy::of(scope.optimizations());
        // A row is laid out left to right, and `place_relative` mirrors it in a right-to-left
        // layout; a column's items align in the direction and are placed as they are, as
        // Compose's (so a centered item rounds as in left to right).
        let layout_direction = scope.layout_direction();
        state.rtl.set(layout_direction == compose_ui::LayoutDirection::Rtl);
        state.density.set(density.density);
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
        if state.item_constraints_written.replace(Some(item_constraints)) != Some(item_constraints)
        {
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
        // An item whose content isn't applied (a request's paused composition) or was removed (a
        // cancelled one's) is one it lacks: composed in this frame's next round (D592).
        let unready = state.prefetch.layout_started();
        let lacked_unready = Cell::new(false);
        let mut measure_child = |index: usize, sizes: &mut Sizes| -> Option<Placeable> {
            if unready.is_some_and(|key| key == info.items.key(index)) {
                // Not applied (a request's paused composition): completed before the round's
                // pass; cancelled, its item composes it in that pass.
                state.prefetch.complete_pending();
                lacked_unready.set(true);
                return None;
            }
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
        if let Some(observer) = state.observer.borrow().clone() {
            compose_runtime::snapshots::Snapshot::without_read_observation(|| {
                observer.before_measure(
                    state,
                    &info.items,
                    density,
                    main_max,
                    before,
                    after,
                    spacing,
                );
            });
        }
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
        let animator = &state.item_animator;
        // Where a scroll started from, for an item's place not to animate what it moved.
        let scrolled_from =
            (!animator.idle() && scrolled != 0.0).then_some((index, (offset - scrolled).round()));
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
            // Nothing visible: the window starts over.
            state.prefetch.after_layout(
                state,
                self.scheduler.as_ref(),
                None,
                0,
                false,
                None,
                &[],
                &mut |_| None,
            );
            // Known: an empty list takes no scroll (D480).
            state.room_backward.set(0.0);
            state.room_forward.set(0.0);
            state.publish_can_scroll();
            state.room_estimated.set((false, false));
            state.beyond_more.set((false, false));
            if let Some(observer) = state.observer.borrow().clone() {
                compose_runtime::snapshots::Snapshot::without_read_observation(|| {
                    observer.after_measure();
                });
            }
            // As large as its padding, as Compose's (its measurement's size excludes it).
            let (main_padding, cross_padding) = (before + after, cross_before + cross_after);
            let size = if vertical {
                IntSize::new(
                    constraints.constrain_width(cross_padding),
                    constraints.constrain_height(main_padding),
                )
            } else {
                IntSize::new(
                    constraints.constrain_width(main_padding),
                    constraints.constrain_height(cross_padding),
                )
            };
            // Its items gone, what they drew may fade out, and it holds their room for that.
            let size = if animator.idle() {
                animator.note_items(&info.items, 0);
                size
            } else {
                let pass = item_animator::Pass {
                    items: &info.items,
                    vertical,
                    reverse,
                    mirror: (1, 1),
                    main_size: if vertical { size.height } else { size.width },
                    lane_count: 1,
                    layout_min: 0,
                    layout_max: 0,
                };
                animator.on_measured(state, &pass, &[], &mut |_, _| None);
                let min = animator.min_size_to_fit_disappearing();
                IntSize::new(
                    constraints.constrain_width(size.width.max(min.width)),
                    constraints.constrain_height(size.height.max(min.height)),
                )
            };
            state.laid_out.begin();
            let keys_changed = state.note_laid_items(&info.items);
            state.laid_out.end(
                layout_info::LaidViewport {
                    start: -before,
                    end: main_max - before,
                    count: 0,
                    size,
                    vertical,
                    reverse,
                    before,
                    after,
                    spacing,
                    max_span: 1,
                },
                keys_changed,
            );
            return scope.layout(size.width, size.height);
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
                        *offset += (main_of(&placeable) + spacing).max(0) as f32;
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
                        // As many as cover the offset, as forward: a next round asks for more
                        // if they're smaller (D576).
                        let wanted = ((-*offset) / average).ceil().max(1.0) as usize;
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
            let size = (main_of(&placeable) + spacing).max(0) as f32;
            if offset >= size {
                offset -= size;
                index += 1;
            } else {
                break;
            }
        }
        // Past the end of the last item: at its end (the room left then scrolls back), or as
        // far beyond it as padding larger than the viewport lets it go, as Compose's (kept
        // small: a huge offset's scroll back would lose pixels to `f32`).
        if index + 1 == count
            && let Some(placeable) = measure(index, &mut sizes)
        {
            let beyond = (before + after - main_max).max(0);
            offset = offset.min((main_of(&placeable) + beyond) as f32);
        }
        let mut placed: Vec<(Placeable, i32)> = Vec::new();
        let mut end_index;
        let mut missing;
        let mut content_end;
        let mut attempt = 0;
        // How far the items were scrolled back to fill a gap at the end.
        let mut scrolled_back = 0;
        // The items laid out before a scroll back stay laid out, as Compose's `visibleItems` (an
        // empty item at the very end, D863).
        let mut laid_end = index;
        loop {
            attempt += 1;
            placed.clear();
            missing = false;
            let mut main = before - offset.round() as i32;
            end_index = index;
            // At least one item, as Compose's, even with padding larger than the viewport.
            while end_index < count
                && (main < main_max || end_index < laid_end || placed.is_empty())
            {
                match measure(end_index, &mut sizes) {
                    Some(placeable) => {
                        let size = main_of(&placeable);
                        placed.push((placeable, main));
                        main += (size + spacing).max(0);
                        end_index += 1;
                    }
                    None => {
                        missing = true;
                        break;
                    }
                }
            }
            content_end = if placed.is_empty() { main } else { main - spacing } + after;
            laid_end = laid_end.max(end_index);
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
                scrolled_back += gap as i32;
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
        // The items before the first visible one under the start padding (or overlapping it at
        // a negative spacing) are laid out and shown too, as Compose's `measureLazyList` composes
        // them to fill `beforeContentPadding`: `lead` is the first item laid out, `index` stays
        // the first visible one (the first not wholly in the padding). Those not composed are
        // composed in this frame's next round, as many as the room takes at the average size.
        // The bound is Compose's `minOffset`, the spacing taken in when negative; after a scroll
        // back to fill a gap at the end, Compose fills the start padding to
        // `beforeContentPadding` only (`LazyListMeasure.kt:249-251`), keeping the items its first
        // loop added before the scroll back: the spacing plus the gap, at most 0.
        let bound = (spacing.min(0) + scrolled_back).min(0);
        let mut lead = index;
        if let Some((_, first_main)) = placed.first() {
            let mut start = *first_main;
            let mut leading: Vec<(Placeable, i32)> = Vec::new();
            while start > bound && lead > 0 {
                let Some(placeable) = measure(lead - 1, &mut sizes) else {
                    let room = (start - bound) as usize;
                    let wanted = room.div_ceil((sizes.average() + spacing).max(1) as usize);
                    wanted_before += wanted.min(lead);
                    break;
                };
                start -= (main_of(&placeable) + spacing).max(0);
                leading.push((placeable, start));
                lead -= 1;
            }
            leading.reverse();
            placed.splice(0..0, leading);
        }
        // At a negative spacing, Compose's walk back to fill it makes the first item laid out
        // the first visible one, but for those wholly before the content's start at their size
        // with the spacing (at least 0, `mainAxisSizeWithSpacings`), unless at the start itself:
        // items overlapping by more than their size stack, the first of them first (D873).
        if spacing < 0 && lead < index && scrolled_back == 0 {
            let mut first = 0;
            let mut into = before - placed[0].1;
            while into > 0 && first + 1 < placed.len() {
                let size = (main_of(&placed[first].0) + spacing).max(0);
                if size > into {
                    break;
                }
                into -= size;
                first += 1;
            }
            if into >= 0 {
                index = lead + first;
                offset = into as f32;
            }
        }
        // The list's length: the viewport once scrolled, else its content's, within the
        // constraints.
        let scrolled_into = index > 0 || offset > 0.0;
        let main_size = if scrolled_into { main_max } else { content_end.min(main_max) };
        let mut main_total = if vertical {
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
        // The items wanted composed: those in view, those a focus search asked for beyond them
        // (D477), and as many more as the room left would take if items were missing.
        let average = sizes.average();
        let (beyond_before, beyond_after) = state.beyond_bounds.get();
        let (beyond_before, beyond_after) = (
            beyond_before + state.beyond_viewport.get(),
            beyond_after + state.beyond_viewport.get(),
        );
        let wanted_start = lead.saturating_sub(beyond_before + wanted_before);
        let mut wanted_end = (end_index + beyond_after).min(count);
        if missing {
            // The items the room left takes at the average size of those in view (the first
            // left out when others follow it: a header; the list's average if none): an estimate,
            // so no more than `FIRST_STEP`, or as many as are placed, a round. Whatever the sizes,
            // what's asked for beyond what the measurement turns out to need is then at most
            // about as much again, and the rounds are as many as doublings (D487); the estimate
            // only saves rounds (each estimate alone broke on some sizes: a header, D475; a
            // divider after a card, D481; a spacer and a divider, D487).
            let in_view = match placed.len() {
                0 => average,
                1 => main_of(&placed[0].0).max(1),
                placed_count => {
                    (placed[1..].iter().map(|(placeable, _)| main_of(placeable)).sum::<i32>()
                        / (placed_count as i32 - 1))
                        .max(1)
                }
            };
            // The room the next items fill (the end padding's included: the items fill the
            // viewport under it), each with its spacing: as many as cover it, no more.
            let room = (main_max - content_end + after).max(1) as usize;
            // Doubling from what was asked last round too, so a scroll past the composed items
            // (none placed) grows 4, 8, 16, … (D492); at most `MOST_IN_A_ROUND`, so zero-height
            // items don't compose tens of thousands in a frame.
            let cap = placed.len().max(FIRST_STEP).max(2 * state.asked.get()).min(MOST_IN_A_ROUND);
            let more = room.div_ceil((in_view + spacing).max(1) as usize).min(cap);
            state.asked.set(more);
            wanted_end = (end_index + more + beyond_after).min(count);
        } else {
            state.asked.set(0);
        }
        let covers = composed_start <= wanted_start && composed_end >= wanted_end;
        let prefetch = &state.prefetch;
        prefetch.policy.set(policy);
        prefetch.vertical.set(info.vertical);
        prefetch.visible.set(end_index - lead);
        // The header sticking over the first item in view, if it's composed: its index, size and
        // where it sticks, as below. Compose's `visibleItemsInfo` holds it, by index (D895).
        let headers = info.items.headers_around(lead);
        let sticking = headers.and_then(|(header, next)| {
            let placeable = measure(header, &mut sizes)?;
            let main_at = |item: usize| {
                item.checked_sub(lead).and_then(|position| placed.get(position)).map(|(_, m)| *m)
            };
            let mut at = main_at(header).map_or(0, |main| main.max(0));
            if let Some(next_main) = next.and_then(main_at) {
                at = at.min(next_main - main_of(&placeable) - spacing);
            }
            Some((header, placeable, at))
        });
        // The cache window, once the layout settled (nothing missing, nothing waiting to be composed
        // before, its header too): what it shows, and the scrolling since.
        if !missing && wanted_before == 0 && (headers.is_none() || sticking.is_some()) {
            let laid = match (placed.first(), placed.last()) {
                (Some((_, first_main)), Some((last, last_main))) => Some(Laid {
                    // A pager's pages beyond the viewport are composed: the window starts past
                    // them, as `PagerCacheWindowScope`.
                    first: sticking
                        .map_or(lead, |(header, _, _)| header)
                        .saturating_sub(state.beyond_viewport.get())
                        as i32,
                    last: (end_index - 1 + state.beyond_viewport.get()).min(count - 1),
                    start_overflow: (-sticking.map_or(*first_main, |(_, _, at)| at)).max(0),
                    end_overflow: (last_main + main_of(last) + spacing - main_max).abs(),
                    viewport: main_max,
                    average: {
                        // `visibleItemsInfo`'s: the sticking header once, itself or apart.
                        let apart = sticking.filter(|(header, _, _)| *header < lead);
                        let of_header = apart.map_or(0, |(_, placeable, _)| main_of(&placeable));
                        let in_view: i32 = placed.iter().map(|(it, _)| main_of(it)).sum();
                        let count = placed.len() + usize::from(apart.is_some());
                        (of_header + in_view) / count.max(1) as i32 + spacing
                    },
                    density,
                    lanes: None,
                }),
                _ => None,
            };
            let items = &info.items;
            let mut visible = prefetch.sizes.borrow_mut();
            visible.clear();
            if let Some((header, placeable, _)) = sticking {
                visible.push((header, main_of(&placeable)));
            }
            let in_view = placed
                .iter()
                .enumerate()
                .map(|(position, (placeable, _))| (lead + position, main_of(placeable)));
            visible.extend(in_view.filter(|(item, _)| Some(*item) != headers.map(|(it, _)| it)));
            prefetch.after_layout(
                state,
                self.scheduler.as_ref(),
                laid.as_ref(),
                count,
                state.laid_items_changed(&info.items),
                None,
                &visible,
                &mut |unit| Some(items.key(unit)),
            );
        }
        // The items that may stay composed: those wanted, those in the window, and those requests
        // composed next to them.
        // A focus search that laid items out beyond the bounds is over.
        let beyond_ended = state.beyond_ended.take();
        if beyond_ended {
            prefetch.search_ended();
        }
        let header_apart =
            headers.map(|(header, _)| header).filter(|header| *header < wanted_start);
        let limits = {
            let next = if prefetch.forward.get() {
                wanted_end + 1
            } else {
                wanted_start.saturating_sub(1)
            };
            let window = prefetch.window_bounds().map(|(first, last)| (first, last + 1));
            // A header sticking away from the range: the window is its, what's between goes (D895).
            let window = window.map(|(first, last)| {
                (if header_apart.is_some() { first.max(wanted_start) } else { first }, last)
            });
            prefetch.target(
                (wanted_start, wanted_end),
                (composed_start, composed_end),
                window,
                (!beyond_ended).then_some(next),
            )
        };
        prefetch.settle_detached(
            header_apart.unwrap_or(lead),
            (composed_start, composed_end),
            limits,
            header_apart.is_some(),
        );
        // After a focus search that laid items out beyond the bounds, those it composed go
        // too, but the ones next to those wanted, as after a scroll (D482).
        let past_limits = composed_start < limits.0 || composed_end > limits.1;
        let search_ended = beyond_ended && past_limits;
        if search_ended {
            state.dispose_leaving.set(true);
        }
        let mut composed = (composed_start, composed_end);
        let composed_more = state.composed_more.take();
        // A paused composition cancelled: the items composed again first (D594).
        let recompose = lacked_unready.get() || prefetch.take_cancelled();
        if !covers || search_ended || recompose {
            // Items to compose (or a focus search's to let go): this composition deactivates
            // those the layout no longer wants too (kept for reuse), as Compose's measurement.
            composed = limits;
            state.compose(composed);
        } else if composed_more {
            state.composed.set(composed);
        }
        // What prefetching works from.
        prefetch.node.set(Some(node));
        prefetch.count.set(count);
        prefetch.wanted.set((wanted_start, wanted_end));
        prefetch.limits.set(limits);
        // Those no longer wanted, unplaced, are deactivated by the next composition: a
        // request's, or one between frames; a frame with nothing to compose runs none for them
        // (D575).
        prefetch.trim_between_frames(state, self.scheduler.as_ref(), composed);
        state.position.set((index, offset));
        if count > 0 {
            state.first_key.set(Some(info.items.key(index)));
        }
        let layout_info = (count, (average + spacing) as f32, spacing, main_max, before + after);
        if state.layout_info_written.replace(layout_info) != layout_info {
            state.layout_info.set(layout_info);
        }
        let rounded = (index, offset.round() as i32);
        if state.published.get() != rounded {
            state.published.set(rounded);
            state.first_index.set(rounded.0);
            state.first_offset.set(rounded.1);
        }
        // The items wanted are placed: those in view, and those a focus search asked for beyond
        // them (clipped away), so focus can move to them; those composed beyond the ones wanted
        // (prefetched, kept behind) aren't, as Compose's: nothing draws or hits them until
        // they're wanted. A focused one among those is placed, next to the others: focus
        // scrolls it in.
        let (mut placed_start, mut placed_end) =
            (composed_start.max(wanted_start), composed_end.min(wanted_end));
        for key in state.pinned.get() {
            if let Some(pinned) = info.items.index_of(key, state.pinned_near.get()) {
                if (placed_end..composed_end).contains(&pinned) {
                    placed_end = pinned + 1;
                } else if (composed_start..placed_start).contains(&pinned) {
                    placed_start = pinned;
                }
            }
        }
        let mut beyond: Vec<(Placeable, i32)> = Vec::new();
        let mut main =
            placed.last().map_or(before, |(placeable, main)| main + main_of(placeable) + spacing);
        for next in end_index..placed_end.min(count) {
            if let Some(placeable) = measure(next, &mut sizes) {
                beyond.push((placeable, main));
                main += (main_of(&placeable) + spacing).max(0);
            }
        }
        // How far the list can scroll each way: known when the composed items reach its ends.
        let extent_end = main - spacing + after;
        // Those before, placed first, in the items' order: the children are then placed in
        // their order (but a sticky header), which the tree draws and hits without sorting them.
        let mut ahead: Vec<(Placeable, i32)> = Vec::new();
        let mut main = placed.first().map_or(before, |(_, main)| *main);
        for previous in (placed_start.min(lead)..lead).rev() {
            if let Some(placeable) = measure(previous, &mut sizes) {
                main -= (main_of(&placeable) + spacing).max(0);
                ahead.push((placeable, main));
            }
        }
        ahead.reverse();
        let extent_start = main - before;
        let pending = state.pending.get();
        // What input taken past an estimated end couldn't use goes to the parents (D479), once
        // the layout is over.
        let mut post_scroll: Option<Box<dyn FnOnce()>> = None;
        // Once it settled: no scroll left waiting for items, none missing.
        if pending == 0.0
            && !missing
            && wanted_before == 0
            && let Some(overshoot) = state.overshoot.borrow_mut().take()
            && let Some(moved) =
                sizes.distance(overshoot.start(&info.items), (index, offset), spacing)
        {
            post_scroll = overshoot.rest(moved);
        }
        let reaches_start = (lead == 0 || placed_start == 0) && wanted_before == 0;
        // Not reaching an end: estimated from the average size (a nested list's wheel delta
        // it can't use goes on to its parent).
        let average = sizes.average() as f32 + spacing as f32;
        let reaches_end = !missing && (end_index == count || placed_end >= count);
        state.room_estimated.set((!reaches_start, !reaches_end));
        state.beyond_more.set((placed_start.min(lead) > 0, placed_end.max(end_index) < count));
        state.room_backward.set(if reaches_start {
            (-extent_start) as f32 + pending
        } else {
            (placed_start.min(lead) as f32 * average - extent_start as f32 + pending).max(1.0)
        });
        state.room_forward.set(if reaches_end {
            (extent_end.max(content_end) - main_max).max(0) as f32 - pending
        } else {
            let after_composed = count.saturating_sub(placed_end.max(end_index)) as f32 * average;
            (after_composed + (extent_end.max(content_end) - main_max) as f32 - pending).max(1.0)
        });
        state.publish_can_scroll();
        // The sticky header over the first item laid out (under the start padding, if any, as
        // Compose's `applyStickyItems`), kept composed: at the list's start, or where it is if
        // lower, pushed out by the next header, its size and the spacing after it ending where
        // that one starts (Compose's `nextStickyItemOffset - mainAxisSizeWithSpacings`, D649).
        // The items in view for `layout_info`, as Compose's `visibleItemsInfo`: those laid out
        // from the first under the start padding to the last in view (not those beyond, placed
        // for a focus search or pinned), then the sticky header where it sticks.
        let mut laid = state.laid_out.begin();
        for (position, (placeable, main)) in placed.iter().enumerate() {
            laid.push(layout_info::LaidItem {
                index: lead + position,
                main: main - before,
                main_size: main_of(placeable),
                ..Default::default()
            });
        }
        let header = headers.map(|(header, _)| header);
        if state.sticky_written.replace(header) != header {
            state.sticky.set(header);
        }
        state.sticking_size.set(sticking.map_or(0, |(_, placeable, _)| main_of(&placeable)));
        // Where the items are for `animate_item`, before a sticking header leaves them.
        let animated_items = (!animator.idle()).then(|| {
            let at = |base: usize| {
                move |(position, (placeable, main)): (usize, &(Placeable, i32))| {
                    (base + position, *placeable, *main)
                }
            };
            ahead
                .iter()
                .enumerate()
                .map(at(lead - ahead.len()))
                .chain(placed.iter().enumerate().map(at(lead)))
                .chain(beyond.iter().enumerate().map(at(end_index)))
                .collect::<Vec<_>>()
        });
        // Those laid out in view, as Compose's `currentFirstItemScrollOffset` and
        // `currentMainAxisOffset`: where those moving out of view go.
        let layout_min = placed.first().map_or(0, |(_, main)| (before - main).max(0));
        let layout_max =
            placed.iter().map(|(it, _)| (main_of(it) + spacing).max(0)).sum::<i32>() - layout_min;
        let mut sticky = None;
        if let Some((header, next)) = headers
            && let Some((_, placeable, _)) = sticking
        {
            let natural =
                ahead.iter().chain(&placed).chain(&beyond).find(|(other, _)| *other == placeable);
            let mut at = natural.map_or(0, |(_, main)| (*main).max(0));
            if let Some(next) = next.and_then(|next| measure(next, &mut sizes))
                && let Some((_, next_main)) =
                    ahead.iter().chain(&placed).chain(&beyond).find(|(other, _)| *other == next)
            {
                at = at.min(next_main - main_of(&placeable) - spacing);
            }
            placed.retain(|(other, _)| *other != placeable);
            beyond.retain(|(other, _)| *other != placeable);
            ahead.retain(|(other, _)| *other != placeable);
            laid.retain(|item| item.index != header);
            laid.push(layout_info::LaidItem {
                index: header,
                main: at - before,
                main_size: main_of(&placeable),
                sticky: true,
                ..Default::default()
            });
            sticky = Some((placeable, at));
        }
        let children = state.children.borrow().clone();
        for &pinned in &children {
            if (pinned < composed_start || pinned >= composed_end)
                && Some(pinned) != header
                && !prefetch.detached.borrow().contains(&pinned)
                && let Some(placeable) = measure(pinned, &mut sizes)
                // Not one the window's room already placed: a child is placed once (D1210).
                && !ahead.iter().chain(&placed).chain(&beyond).any(|(other, _)| *other == placeable)
            {
                let at =
                    if pinned < index { -main_of(&placeable) - main_max } else { main_max * 2 };
                beyond.push((placeable, at));
            }
        }
        // Placed along the axis, aligned across it.
        let cross_size = placed
            .iter()
            .map(|(placeable, _)| if vertical { placeable.width } else { placeable.height })
            .max()
            .unwrap_or(0);
        // Short of items, the composition's to follow before anything's drawn: the size across stays
        // what it was. Compose's items compose in its measurement, so a parent never sees less: a
        // jump there has nothing laid out yet, and a list around this one would take an item of no
        // size for one scrolled past (D1456).
        let cross_size = if missing { cross_size.max(state.cross_size.get()) } else { cross_size };
        state.cross_size.set(cross_size);
        let constrain_main = |main: i32| {
            if vertical {
                constraints.constrain_height(main)
            } else {
                constraints.constrain_width(main)
            }
        };
        let constrain_cross = |cross: i32| {
            if vertical {
                constraints.constrain_width(cross)
            } else {
                constraints.constrain_height(cross)
            }
        };
        let mut cross_total = constrain_cross(cross_size + cross_before + cross_after);
        // Across the list, in its padding: where an item is aligned.
        let aligned = |placeable: &Placeable, cross_total: i32| {
            let room = cross_total - cross_before - cross_after;
            if vertical {
                let space = IntSize::new(room, placeable.height);
                info.alignment.align(placeable.size(), space, layout_direction).x
            } else {
                let space = IntSize::new(placeable.width, room);
                info.alignment.align(placeable.size(), space, direction).y
            }
        };
        let mut moved_away = Vec::new();
        if let Some(items) = &animated_items {
            if let Some((from_index, from_offset)) = scrolled_from {
                let moved =
                    sizes.distance((from_index, from_offset), (index, offset.round()), spacing);
                let consumed = moved.unwrap_or(scrolled - pending);
                animator.add_consumed_scroll(-(consumed.round() as i32));
            }
            let settled = !missing
                && wanted_before == 0
                && !lacked_unready.get()
                && (headers.is_none() || sticking.is_some());
            if settled {
                let positioned: Vec<item_animator::Positioned> = items
                    .iter()
                    .map(|&(item, placeable, main)| item_animator::Positioned {
                        index: item,
                        key: info.items.key(item),
                        main: main - before,
                        cross: aligned(&placeable, cross_total),
                        main_size_with_spacing: (main_of(&placeable) + spacing).max(0),
                        lane: 0,
                        span: 1,
                        constraints: None,
                    })
                    .collect();
                let pass = item_animator::Pass {
                    items: &info.items,
                    vertical,
                    reverse,
                    mirror: (
                        if !vertical
                            && reverse != (layout_direction == compose_ui::LayoutDirection::Rtl)
                        {
                            -1
                        } else {
                            1
                        },
                        if vertical && reverse { -1 } else { 1 },
                    ),
                    main_size: main_total,
                    lane_count: 1,
                    layout_min,
                    layout_max,
                };
                let moved = animator.on_measured(state, &pass, &positioned, &mut |item, _| {
                    measure(item, &mut sizes).map(|it| (main_of(&it) + spacing).max(0))
                });
                let mut before_ahead = 0;
                for away in moved {
                    if let Some(placeable) = measure(away.index, &mut sizes) {
                        // Placed once, here: the items kept apart were out of the way so far.
                        beyond.retain(|(other, _)| *other != placeable);
                        let entry = (placeable, away.main + before);
                        if away.to_start {
                            ahead.insert(before_ahead, entry);
                            before_ahead += 1;
                        } else {
                            beyond.push(entry);
                        }
                        moved_away.push((away.key, placeable, away.main + before));
                    }
                }
                let min = animator.min_size_to_fit_disappearing();
                if min != IntSize::ZERO {
                    let (main_min, cross_min) =
                        if vertical { (min.height, min.width) } else { (min.width, min.height) };
                    main_total = constrain_main(main_total.max(main_min));
                    cross_total = constrain_cross(cross_total.max(cross_min));
                }
            }
        } else if !missing && wanted_before == 0 {
            animator.note_items(&info.items, index);
        }
        // Sizes far from view are forgotten, so the estimates follow the data.
        if sizes.len() > 4 * MOST_MISSING {
            sizes.retain(|item| item.abs_diff(index) <= MOST_MISSING);
        }
        drop(sizes);
        let (width, height) =
            if vertical { (cross_total, main_total) } else { (main_total, cross_total) };
        let position_of = |placeable: &Placeable, main: i32| {
            let main = if reverse { main_total - main - main_of(placeable) } else { main };
            let cross = aligned(placeable, cross_total);
            if vertical {
                // The content padding's start is on the right in a right-to-left layout.
                let before = if layout_direction == compose_ui::LayoutDirection::Rtl {
                    cross_after
                } else {
                    cross_before
                };
                (before + cross, main)
            } else {
                (main, cross_before + cross)
            }
        };
        let layout = |scope: &mut MeasureScope<'_>, placeable: &Placeable, main: i32| {
            let (x, y) = position_of(placeable, main);
            if vertical {
                scope.place(placeable, x, y);
            } else {
                scope.place_relative(placeable, x, y);
            }
        };
        // The sticky header last: drawn over the items, and hit first.
        // Under a motion frame of reference, as `LazyListMeasure`: a scroll isn't a move of its own.
        scope.set_motion_frame_of_reference_placement(true);
        for (placeable, main) in ahead.iter().chain(&placed).chain(&beyond).chain(&sticky) {
            layout(scope, placeable, *main);
        }
        scope.set_motion_frame_of_reference_placement(false);
        if let Some(items) = &animated_items {
            let bounds = (-before, main_total + after, vertical);
            let note_placed = |key: u64, placeable: &Placeable, main: i32| {
                let (x, y) = position_of(placeable, main);
                let x = if !vertical && layout_direction == compose_ui::LayoutDirection::Rtl {
                    width - x - placeable.width
                } else {
                    x
                };
                animator.note_placed(key, x, y, bounds);
            };
            let sticking_header = headers.map(|(header, _)| header).filter(|_| sticky.is_some());
            for (item, placeable, main) in items {
                if Some(*item) != sticking_header {
                    note_placed(info.items.key(*item), placeable, *main);
                }
            }
            if let (Some(header), Some((placeable, at))) = (sticking_header, sticky) {
                note_placed(info.items.key(header), &placeable, at);
            }
            for (key, placeable, main) in &moved_away {
                note_placed(*key, placeable, *main);
            }
        }
        drop(laid);
        let keys_changed = state.note_laid_items(&info.items);
        state.laid_out.end(
            layout_info::LaidViewport {
                start: -before,
                end: main_max - before,
                count,
                size: IntSize::new(width, height),
                vertical,
                reverse,
                before,
                after,
                spacing,
                max_span: 1,
            },
            keys_changed,
        );
        if let Some(observer) = state.observer.borrow().clone() {
            compose_runtime::snapshots::Snapshot::without_read_observation(|| {
                observer.after_measure();
            });
        }
        if let Some(post_scroll) = post_scroll {
            scope.run_after_layout(post_scroll);
        }
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "LazyList"
    }

    // As Compose's `SubcomposeLayout`: it composes while measuring, so it has no intrinsics.
    fn min_intrinsic_width(
        &self,
        _: &mut compose_ui::IntrinsicMeasureScope<'_>,
        _: &[compose_ui::IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        panic!("{}", compose_ui::platform::NO_INTRINSICS_MESSAGE)
    }

    fn min_intrinsic_height(
        &self,
        _: &mut compose_ui::IntrinsicMeasureScope<'_>,
        _: &[compose_ui::IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        panic!("{}", compose_ui::platform::NO_INTRINSICS_MESSAGE)
    }

    fn max_intrinsic_width(
        &self,
        _: &mut compose_ui::IntrinsicMeasureScope<'_>,
        _: &[compose_ui::IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        panic!("{}", compose_ui::platform::NO_INTRINSICS_MESSAGE)
    }

    fn max_intrinsic_height(
        &self,
        _: &mut compose_ui::IntrinsicMeasureScope<'_>,
        _: &[compose_ui::IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        panic!("{}", compose_ui::platform::NO_INTRINSICS_MESSAGE)
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

    fn contains_key(&self, index: usize) -> bool {
        self.sizes.contains_key(&index)
    }

    fn get(&self, index: usize) -> Option<i32> {
        self.sizes.get(&index).copied()
    }

    /// How far it is from `from` to `to` (an item and the pixels into it), items `spacing`
    /// apart, an item not measured at the average size (D494).
    fn distance(&self, from: (usize, f32), to: (usize, f32), spacing: i32) -> Option<f32> {
        let (low, high, sign) =
            if to.0 >= from.0 { (from.0, to.0, 1.0) } else { (to.0, from.0, -1.0) };
        let average = self.average();
        let mut sum = 0.0;
        for index in low..high {
            sum += (self.sizes.get(&index).copied().unwrap_or(average) + spacing) as f32;
        }
        Some(sign * sum + to.1 - from.1)
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
    fling_behavior: Option<Rc<dyn crate::gestures::FlingBehavior>>,
    overscroll: Option<Rc<dyn crate::OverscrollEffect>>,
}

pub(crate) struct LazyScrollNode {
    state: LazyListState,
    /// The state as a scrollable state, for drags and flings.
    dyn_state: Rc<dyn ScrollableState>,
    vertical: bool,
    reverse: bool,
    enabled: bool,
    fling_behavior: Option<Rc<dyn crate::gestures::FlingBehavior>>,
    overscroll: Option<Rc<dyn crate::OverscrollEffect>>,
    drag: DragScroller,
    /// Tells the state's scroll scope the parents around a drag's, a fling's or a wheel turn's
    /// deltas: built once (not on each pass of each event), the axis and the mirroring it names
    /// them with set by each event.
    set_outer: crate::scroll::OuterSetter,
    outer_axis: Rc<Cell<(bool, bool)>>,
}

/// The node's `set_outer`: the parents around the deltas, with the axis and mirroring the
/// events set in `axis`.
fn outer_setter(state: LazyListState, axis: Rc<Cell<(bool, bool)>>) -> crate::scroll::OuterSetter {
    Rc::new(move |chain_and_source| {
        if let Some(inner) = state.inner() {
            let (vertical, reverse) = axis.get();
            *inner.scroll_outer.borrow_mut() =
                chain_and_source.map(|(chain, source)| (chain, vertical, reverse, source));
        }
    })
}

impl ModifierNodeElement for LazyScrollElement {
    type Node = LazyScrollNode;
    fn create(&self) -> LazyScrollNode {
        let axis = Rc::new(Cell::new((self.vertical, self.reverse)));
        LazyScrollNode {
            state: self.state,
            dyn_state: Rc::new(self.state),
            vertical: self.vertical,
            reverse: self.reverse,
            enabled: self.enabled,
            fling_behavior: self.fling_behavior.clone(),
            overscroll: self.overscroll.clone(),
            drag: DragScroller::along(self.vertical),
            set_outer: outer_setter(self.state, axis.clone()),
            outer_axis: axis,
        }
    }

    fn update(&self, node: &mut LazyScrollNode) {
        // A fling scrolls the state it started with: a new state stops it (the old one's
        // owner may be going, D240).
        if node.state != self.state {
            node.drag.cancel_leaving();
            node.drag.stop_fling();
            node.dyn_state = Rc::new(self.state);
            node.set_outer = outer_setter(self.state, node.outer_axis.clone());
        }
        node.state = self.state;
        node.vertical = self.vertical;
        node.drag.set_vertical(self.vertical);
        node.reverse = self.reverse;
        node.fling_behavior = self.fling_behavior.clone();
        node.overscroll = self.overscroll.clone();
        if node.enabled && !self.enabled {
            node.drag.cancel_leaving();
            node.drag.stop_fling();
        }
        node.enabled = self.enabled;
    }

    fn same(&self, other: &Self) -> bool {
        other.vertical == self.vertical
            && other.reverse == self.reverse
            && other.enabled == self.enabled
            && other.state.same(&self.state)
            && same_fling(&other.fling_behavior, &self.fling_behavior)
            && crate::overscroll::same_overscroll(&other.overscroll, &self.overscroll)
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

impl compose_ui::SemanticsModifierNode for LazyScrollNode {
    /// How far it's scrolled and can be, along its axis, as Compose's lazy layouts' semantics:
    /// accessibility services scroll it a page at a time (D478). Where an end isn't laid out,
    /// the room that way is estimated from the average item size.
    fn apply_semantics(&self, config: &mut compose_ui::SemanticsConfiguration) {
        let Some(inner) = self.state.inner() else { return };
        let (count, average, ..) = inner.layout_info.get();
        let (index, _) = inner.published.get();
        let estimate = |room: f32, items: usize| {
            if room.is_finite() { room.max(0.0) } else { items as f32 * average }
        };
        let value = estimate(inner.room_backward.get(), index);
        let range = compose_ui::ScrollAxisRange {
            value,
            max_value: value + estimate(inner.room_forward.get(), count.saturating_sub(index)),
            reverse_scrolling: self.reverse,
        };
        if self.vertical {
            config.set_vertical_scroll_axis_range(range);
        } else {
            config.set_horizontal_scroll_axis_range(range);
        }
        // As `LazyLayoutSemanticState`: a list's count isn't told, a grid's neither.
        let observer = inner.observer.borrow().clone();
        config.set_is_traversal_group(true);
        config.set_collection_info(match &observer {
            Some(observer) => observer.collection_info(),
            None if inner.is_grid.get() => {
                compose_ui::CollectionInfo { row_count: -1, column_count: -1 }
            }
            None if self.vertical => compose_ui::CollectionInfo { row_count: -1, column_count: 1 },
            None => compose_ui::CollectionInfo { row_count: 1, column_count: -1 },
        });
        // The viewport without the content padding, and the padding before and after, as
        // Compose's `LazyLayoutSemantics`: a page, and what a shown item must clear.
        {
            let (state, vertical) = (self.state, self.vertical);
            config.get_scroll_viewport_length(None, move |out| {
                let info = state.layout_info_untracked();
                let size = info.viewport_size;
                let length = if vertical { size.height } else { size.width };
                let (before, after) = (info.before_content_padding, info.after_content_padding);
                out.push((length - before - after) as f32);
                out.push(before as f32);
                out.push(after as f32);
                true
            });
        }
        // Not offered with user scrolling off, as Compose's (the range stays).
        if !self.enabled {
            return;
        }
        let (state, vertical) = (self.state, self.vertical);
        config.scroll_to_index(None, move |index| {
            let count = state.inner().map_or(0, |inner| inner.layout_info.get().0);
            assert!(
                index >= 0 && (index as usize) < count,
                "Can't scroll to index {index}, it is out of bounds [0, {count})"
            );
            let observer = state.inner().and_then(|inner| inner.observer.borrow().clone());
            match observer {
                Some(observer) => observer.scroll_to_index(index as usize),
                None => {
                    CoroutineScope::new().launch(async move {
                        let _ = state.scroll_to_item(index as usize, 0).await;
                    });
                }
            }
            true
        });
        // As a scrollable's `scrollByOffset`: animated, along the axis, what it used returned.
        config.scroll_by_offset(move |offset| {
            Box::pin(async move {
                let delta = if vertical { offset.y } else { offset.x };
                let used = Rc::new(Cell::new(0.0f32));
                let total = used.clone();
                let _ = state
                    .scroll(MutatePriority::Default, move |scope| async move {
                        let last = Cell::new(0.0f32);
                        compose_animation::animate(
                            0.0,
                            delta,
                            0.0,
                            compose_animation::spring(),
                            |value, _| {
                                let consumed = scope.scroll_by(value - last.get());
                                last.set(last.get() + consumed);
                                total.set(last.get());
                            },
                        )
                        .await;
                    })
                    .await;
                if vertical {
                    compose_ui::Offset::new(0.0, used.get())
                } else {
                    compose_ui::Offset::new(used.get(), 0.0)
                }
            })
        });
        config.scroll_by(None, move |x, y| {
            // Towards the content's end, as Compose's lazy layouts' `scrollBy` (D491); handled
            // even where the list can't move that way, as Compose's (its action always returns
            // true).
            let delta = if vertical { y } else { x };
            if delta != 0.0 {
                state.dispatch(delta, None);
            }
            true
        });
    }
}

impl LazyScrollNode {
    pub(crate) fn drag_gesture(
        &self,
    ) -> (Option<crate::gestures::Orientation>, crate::gestures::GestureState) {
        use crate::gestures::Orientation;
        let orientation =
            if self.vertical { Orientation::Vertical } else { Orientation::Horizontal };
        let state = if self.enabled {
            self.drag.gesture_state()
        } else {
            crate::gestures::GestureState::Idle
        };
        (Some(orientation), state)
    }
}

impl compose_ui::KeyInputModifierNode for LazyScrollNode {
    fn on_key_event(
        &mut self,
        event: &compose_ui::KeyEvent,
        context: &mut compose_ui::EventContext<'_>,
    ) -> bool {
        if !self.enabled {
            return false;
        }
        let reverse = self.mirrored(context.layout_direction() == compose_ui::LayoutDirection::Rtl);
        let dyn_state = self.dyn_state.clone();
        let driver = ScrollDriver {
            state: &dyn_state,
            fling_behavior: self.fling_behavior.as_ref(),
            interaction_source: None,
            state_source: true,
            set_outer: Some(&self.set_outer),
            overscroll: self.overscroll.as_ref(),
        };
        self.drag.on_key_event(event, self.vertical, reverse, context, &driver)
    }
}

impl compose_ui::TraversableNode for LazyScrollNode {
    fn traverse_key(&self) -> &'static compose_ui::TraverseKey {
        &crate::gestures::DRAG_GESTURES
    }
}

/// What items removed last drew fades out over the list's content, as
/// `DisplayingDisappearingItemsNode`: this list's node, so a list with no such item has a node to
/// ask, and no other.
impl compose_ui::DrawModifierNode for LazyScrollNode {
    fn draw_after(&mut self, scope: &mut compose_ui::DrawScope<'_>) {
        if let Some(inner) = self.state.inner()
            && !inner.item_animator.idle()
        {
            inner.item_animator.draw_disappearing(scope);
        }
    }
}

impl ModifierNode for LazyScrollNode {
    fn as_draw(&mut self) -> Option<&mut dyn compose_ui::DrawModifierNode> {
        Some(self)
    }

    fn on_attach(&mut self, context: &mut compose_ui::NodeContext<'_>) {
        if let Some(inner) = self.state.inner() {
            inner.item_animator.set_display(Some(context.handle()));
        }
    }

    fn as_key_input(&mut self) -> Option<&mut dyn compose_ui::KeyInputModifierNode> {
        Some(self)
    }

    fn as_traversable(&mut self) -> Option<&mut dyn compose_ui::TraversableNode> {
        Some(self)
    }

    fn as_semantics(&mut self) -> Option<&mut dyn compose_ui::SemanticsModifierNode> {
        Some(self)
    }

    fn on_release(&mut self, _context: &mut compose_ui::ReleaseContext<'_>) {
        self.drag.cancel_leaving();
        self.drag.detach();
        if let Some(inner) = self.state.inner() {
            inner.item_animator.on_list_left();
        }
    }

    fn on_reset(&mut self, _context: &mut compose_ui::NodeContext<'_>) {
        self.drag.cancel_leaving();
        self.drag.detach();
    }

    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }

    fn hit_clip(&self) -> Option<compose_ui::Shape> {
        Some(compose_ui::Shape::Rectangle)
    }

    fn is_beyond_bounds_layout(&self) -> bool {
        true
    }

    /// One more item (a grid's line) beyond those laid out, the way `direction` goes along the
    /// list, for a focus search, as Compose's `LazyLayoutBeyondBoundsModifierNode` (D477).
    fn lay_out_beyond_bounds(
        &mut self,
        direction: compose_ui::FocusDirection,
        layout_direction: compose_ui::LayoutDirection,
    ) -> bool {
        use compose_ui::FocusDirection;
        let Some(inner) = self.state.inner() else { return false };
        let mirrored = self.mirrored(layout_direction == compose_ui::LayoutDirection::Rtl);
        let forward = match direction {
            FocusDirection::Next => true,
            FocusDirection::Previous => false,
            FocusDirection::Down if self.vertical => !mirrored,
            FocusDirection::Up if self.vertical => mirrored,
            FocusDirection::Right if !self.vertical => !mirrored,
            FocusDirection::Left if !self.vertical => mirrored,
            _ => return false,
        };
        let (before, after) = inner.beyond_more.get();
        if !(if forward { after } else { before }) {
            return false;
        }
        inner.beyond_bounds.update(
            |&(before, after)| {
                if forward { (before, after + 1) } else { (before + 1, after) }
            },
        );
        true
    }

    fn end_beyond_bounds(&mut self) {
        if let Some(inner) = self.state.inner()
            && inner.beyond_bounds.get() != (0, 0)
        {
            inner.beyond_bounds.set((0, 0));
            inner.beyond_ended.set(true);
        }
    }

    /// Scrolls the least that shows `rect` (an item beyond the viewport taking the focus).
    fn bring_into_view(&mut self, rect: Rect, viewport: IntSize) -> Option<Rect> {
        let inner = self.state.inner()?;
        let mirrored = self.mirrored(inner.rtl.get());
        let (start, end, length) =
            crate::scroll::scroll_axis(rect, viewport, self.vertical, mirrored);
        // Where it will be once the scrolling asked for (not yet laid out) is, out from under a
        // sticking header at the list's start, as Compose's `StickyHeaderBringIntoViewSpec`.
        let pending = inner.pending.get();
        let sticking = inner.sticking_size.get() as f32;
        let delta = bring_into_view_delta(
            start - pending - sticking,
            end - pending - sticking,
            length - sticking,
        );
        let used = if delta == 0.0 { 0.0 } else { self.state.dispatch(delta, None) };
        Some(crate::scroll::scrolled_rect(rect, used, self.vertical, mirrored))
    }
}

impl PointerInputModifierNode for LazyScrollNode {
    /// Scrolling by drags, it waits for a touch's past a press: a clickable inside delays its press
    /// (not a mouse's, as Compose's scrollables).
    fn waits_for_gesture(&self, press: &PointerEvent) -> bool {
        self.enabled && press.pointer_type != compose_ui::PointerType::Mouse
    }

    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        if !self.enabled {
            return;
        }
        let reverse = self.mirrored(context.layout_direction() == compose_ui::LayoutDirection::Rtl);
        let flung = self.state;
        let vertical = self.vertical;
        // A drag's, a fling's and a wheel turn's deltas, through the scroll scope, carry the
        // parents around, so what an estimated end leaves goes to them once laid out (D479,
        // D489).
        self.outer_axis.set((vertical, reverse));
        let driver = ScrollDriver {
            state: &self.dyn_state,
            fling_behavior: self.fling_behavior.as_ref(),
            interaction_source: None,
            state_source: true,
            set_outer: Some(&self.set_outer),
            overscroll: self.overscroll.as_ref(),
        };
        self.drag.on_pointer_event(
            event,
            pass,
            bounds,
            self.vertical,
            reverse,
            context,
            &driver,
            // As the nested-scroll parent of the scrollables inside: its own parents get what
            // an estimated end leaves of it too (D489).
            move |chain: &compose_ui::platform::NestedScrollChain,
                  source: compose_ui::NestedScrollSource| {
                let outer = (!chain.is_empty()).then(|| (chain.clone(), vertical, reverse, source));
                // Unless a scroll of its own goes on, as Compose's `performRawScroll`.
                Rc::new(move |delta| {
                    if flung.is_scroll_in_progress() {
                        0.0
                    } else {
                        flung.dispatch(delta, outer.clone())
                    }
                })
            },
        );
    }

    fn on_cancel_pointer_input(&mut self) {
        self.drag.cancel();
    }
}

#[cfg(test)]
mod items_tests {
    use super::*;

    #[test]
    fn only_lists_with_key_functions_keep_keys() {
        let unkeyed = Items::new(vec![Interval::new(1000, Rc::new(|_| {}))]);
        assert!(unkeyed.keys.is_empty());
        assert_eq!(unkeyed.key(5), unkeyed.key(5));
        assert_ne!(unkeyed.key(5), unkeyed.key(6));
        let few = Items::new(vec![Interval::keyed(3, |index| index, Rc::new(|_| {}))]);
        assert_eq!(few.keys.len(), 3);
        let many = Items::new(vec![Interval::keyed(1000, |index| index, Rc::new(|_| {}))]);
        assert_eq!(many.keys.len(), KEY_CACHE);
    }

    #[test]
    fn a_default_key_in_a_keyed_list_is_not_kept() {
        let items = Items::new(vec![
            Interval::new(1, Rc::new(|_| {})),
            Interval::keyed(1, |_| 7, Rc::new(|_| {})),
        ]);
        assert_eq!(items.key(0), hash_key(&DefaultKey(0)));
        assert_eq!(items.keys[0].get().0, usize::MAX);
        assert_eq!(items.key(1), hash_key(&7));
    }
}
