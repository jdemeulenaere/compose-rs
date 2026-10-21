//! Lazy lists, as Compose's `LazyColumn` and `LazyRow`: only the items in view (and a few
//! beyond, and the focused one) are composed, measured and drawn.
//!
//! Compose composes items while measuring (subcomposition). Here the list composes a range of
//! items, and its measurement, which fills the viewport from the scroll position, writes the
//! range it needs when the composed one falls short; the host then recomposes and measures again
//! within the same frame (`UiHost::frame`), so scrolling shows no empty frame. The items are
//! composed by a restartable composable of their own, each item one too: a scroll recomposes
//! neither the list's caller nor the items that stay.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;

use compose_runtime::{
    ComposeArg, OwnedState, composable, hash_key, key, mutable_state_of, remember,
};
use compose_ui::{
    Alignment, Constraints, Dp, EventContext, IntSize, Layout, Measurable, MeasurePolicy,
    MeasureResult, MeasureScope, Modifier, ModifierNode, ModifierNodeElement, PaddingValues,
    Placeable, PointerEvent, PointerEventPass, PointerInputModifierNode, Rect,
};

use crate::layout::Arrangement;
use crate::scroll::{DragScroller, bring_into_view_delta};

/// Items composed beyond the viewport on each side, ready when scrolling reaches them, and
/// placed (out of view) so focus can move to them.
const BEYOND: usize = 2;

/// The most items a measurement asks for beyond those in view, however small they may be.
const MOST_MISSING: usize = 200;

/// How far from its last index an item is looked for by key.
const KEY_WINDOW: usize = 100;

/// Where a lazy list is scrolled, as Compose's `LazyListState`: the first visible item and how
/// far it is scrolled off. Reading them in composition recomposes on scrolls.
#[derive(Clone)]
pub struct LazyListState {
    inner: Rc<LazyListInner>,
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
    /// The focused item's key, kept composed and placed while it scrolls away, as Compose pins
    /// it, and the index it was last at.
    pinned: OwnedState<Option<u64>>,
    pinned_near: Cell<usize>,
    /// The list has had items: an empty layout before that keeps the initial position.
    had_items: Cell<bool>,
    /// The items composed, in the children's order: written by their composition, read by the
    /// layout (a change of children measures it again).
    children: RefCell<Vec<usize>>,
    /// The position as state, for composition, and what was last written to it.
    first_index: OwnedState<usize>,
    first_offset: OwnedState<i32>,
    published: Cell<(usize, i32)>,
    /// Item sizes along the axis, as last measured: estimates for items not composed.
    sizes: RefCell<HashMap<usize, i32>>,
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
}

impl LazyListState {
    pub fn new(
        first_visible_item_index: usize,
        first_visible_item_scroll_offset: i32,
    ) -> LazyListState {
        let offset = first_visible_item_scroll_offset.max(0);
        LazyListState {
            inner: Rc::new(LazyListInner {
                position: Cell::new((first_visible_item_index, offset as f32)),
                first_key: Cell::new(None),
                pending: Cell::new(0.0),
                rtl: Cell::new(false),
                remeasure: mutable_state_of(0),
                composed: mutable_state_of((
                    first_visible_item_index,
                    first_visible_item_index + 1,
                )),
                pinned: mutable_state_of(None),
                pinned_near: Cell::new(0),
                had_items: Cell::new(false),
                children: RefCell::new(Vec::new()),
                first_index: mutable_state_of(first_visible_item_index),
                first_offset: mutable_state_of(offset),
                published: Cell::new((first_visible_item_index, offset)),
                sizes: RefCell::new(HashMap::new()),
                layout_info: mutable_state_of((0, 1, 0, 0, 0)),
                item_constraints: mutable_state_of(None),
                room_backward: Cell::new(if first_visible_item_index > 0 {
                    f32::INFINITY
                } else {
                    offset as f32
                }),
                room_forward: Cell::new(f32::INFINITY),
            }),
        }
    }

    /// The first item in view, as `firstVisibleItemIndex` (a state read).
    pub fn first_visible_item_index(&self) -> usize {
        self.inner.first_index.get()
    }

    /// How many pixels of it are scrolled off, as `firstVisibleItemScrollOffset` (a state read).
    pub fn first_visible_item_scroll_offset(&self) -> i32 {
        self.inner.first_offset.get()
    }

    /// Scrolls to `index` (and `scroll_offset` pixels into it) at the next layout, as
    /// `scrollToItem`, without animation.
    pub fn scroll_to_item(&self, index: usize, scroll_offset: i32) {
        self.inner.position.set((index, scroll_offset.max(0) as f32));
        self.inner.first_key.set(None);
        self.inner.pending.set(0.0);
        self.inner.room_backward.set(f32::INFINITY);
        self.inner.room_forward.set(f32::INFINITY);
        self.inner.remeasure.update(|count| count.wrapping_add(1));
    }

    /// Scrolls by `pixels` (positive: towards the end) at the next layout, as
    /// `scrollBy`/`dispatchRawDelta`; returns whether the list can move that way.
    pub fn scroll_by(&self, pixels: f32) -> bool {
        self.dispatch(pixels) != 0.0
    }

    /// Scrolls by up to `pixels`, as far as the list goes (as last laid out); returns the
    /// pixels it will move.
    fn dispatch(&self, pixels: f32) -> f32 {
        let inner = &self.inner;
        let used = if pixels > 0.0 {
            pixels.min(inner.room_forward.get().max(0.0))
        } else {
            pixels.max(-inner.room_backward.get().max(0.0))
        };
        if used != 0.0 {
            inner.room_forward.set(inner.room_forward.get() - used);
            inner.room_backward.set(inner.room_backward.get() + used);
            inner.pending.set(inner.pending.get() + used);
            inner.remeasure.update(|count| count.wrapping_add(1));
        }
        used
    }

    /// The item count, the average item size (with its spacing), the spacing, the viewport's
    /// length and the content padding along the axis, as last laid out: a scrollbar's
    /// estimates (a state read).
    pub(crate) fn layout_info(&self) -> (usize, i32, i32, i32, i32) {
        self.inner.layout_info.get()
    }

    /// Moves the position for the next layout without asking for one: for a composition that
    /// is about to lay the list out anyway (a grid whose columns changed).
    pub(crate) fn set_position_quietly(&self, index: usize, scroll_offset: i32) {
        self.inner.position.set((index, scroll_offset.max(0) as f32));
        self.inner.first_key.set(None);
        self.inner.pending.set(0.0);
        self.inner.room_backward.set(f32::INFINITY);
        self.inner.room_forward.set(f32::INFINITY);
    }

    /// The first visible item and its offset, as last laid out, read without observing it.
    pub(crate) fn position_untracked(&self) -> (usize, i32) {
        self.inner.published.get()
    }

    /// Whether the list can scroll towards its end, as `canScrollForward` (as last laid out).
    pub fn can_scroll_forward(&self) -> bool {
        self.inner.room_forward.get() > 0.0
    }

    /// Whether the list can scroll towards its start, as `canScrollBackward`.
    pub fn can_scroll_backward(&self) -> bool {
        self.inner.room_backward.get() > 0.0
    }
}

impl ComposeArg for LazyListState {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl std::fmt::Debug for LazyListState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (index, offset) = self.inner.position.get();
        write!(formatter, "LazyListState({index}, {offset})")
    }
}

/// A lazy list state kept across recompositions, as `rememberLazyListState()`.
#[composable(non_restartable)]
pub fn remember_lazy_list_state(
    #[default(0)] initial_first_visible_item_index: usize,
    #[default(0)] initial_first_visible_item_scroll_offset: i32,
) -> LazyListState {
    remember(|| {
        LazyListState::new(
            initial_first_visible_item_index,
            initial_first_visible_item_scroll_offset,
        )
    })
}

/// A run of items: their count, keys and content.
struct Interval {
    count: usize,
    key: Option<Rc<dyn Fn(usize) -> u64>>,
    content: Rc<dyn Fn(usize)>,
}

/// What a lazy list's content declares its items with, as Compose's `LazyListScope`:
/// `scope.item(|| …)`, `scope.items(count, |index| …)`, `scope.items_keyed(count, key, …)`.
#[derive(Clone, Default)]
pub struct LazyListScope {
    intervals: Rc<RefCell<Vec<Interval>>>,
}

impl LazyListScope {
    /// One item, as `item { … }`.
    pub fn item(&self, content: impl Fn() + 'static) {
        self.intervals.borrow_mut().push(Interval {
            count: 1,
            key: None,
            content: Rc::new(move |_| content()),
        });
    }

    /// `count` items, as `items(count) { index -> … }`. Without keys, an item's state goes with
    /// its index.
    pub fn items(&self, count: usize, content: impl Fn(usize) + 'static) {
        self.intervals.borrow_mut().push(Interval { count, key: None, content: Rc::new(content) });
    }

    /// `count` items with keys, as `items(count, key = { … }) { … }`: an item keeps its state
    /// (and the scroll position follows the first visible one) when items before it come and
    /// go. Keys must be unique in the list.
    pub fn items_keyed<K: Hash>(
        &self,
        count: usize,
        key: impl Fn(usize) -> K + 'static,
        content: impl Fn(usize) + 'static,
    ) {
        self.intervals.borrow_mut().push(Interval {
            count,
            key: Some(Rc::new(move |index| hash_key(&key(index)))),
            content: Rc::new(content),
        });
    }
}

impl ComposeArg for LazyListScope {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.intervals, &other.intervals)
    }
}

/// The items a list's content declared, for one composition of the list.
struct Items {
    intervals: Vec<Interval>,
    /// Each interval's first item: a binary search finds an item's interval.
    starts: Vec<usize>,
    count: usize,
}

impl Items {
    fn new(intervals: Vec<Interval>) -> Items {
        let mut starts = Vec::with_capacity(intervals.len());
        let mut count = 0;
        for interval in &intervals {
            starts.push(count);
            count += interval.count;
        }
        Items { intervals, starts, count }
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

    fn key(&self, index: usize) -> u64 {
        match self.find(index) {
            Some((interval, local)) => {
                interval.key.as_ref().map_or(hash_key(&("index", index)), |key| key(local))
            }
            None => hash_key(&("index", index)),
        }
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
    arrangement: Arrangement,
    alignment: Alignment,
}

impl PartialEq for ListInfo {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.items, &other.items)
            && self.vertical == other.vertical
            && self.padding == other.padding
            && self.arrangement == other.arrangement
            && self.alignment == other.alignment
    }
}

/// A vertical lazy list, as Compose's `LazyColumn`.
#[composable(non_restartable)]
#[allow(non_snake_case)]
pub fn LazyColumn(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::remember_lazy_list_state(0, 0))] state: LazyListState,
    #[default(crate::layout::PaddingValues::ZERO)] content_padding: PaddingValues,
    #[default(crate::layout::Arrangement::TOP)] vertical_arrangement: Arrangement,
    #[default(crate::Alignment::TOP_START)] horizontal_alignment: Alignment,
    content: impl Fn(LazyListScope) + 'static,
) {
    LazyList(
        modifier,
        state,
        content_padding,
        true,
        vertical_arrangement,
        horizontal_alignment,
        content,
    );
}

/// A horizontal lazy list, as Compose's `LazyRow`.
#[composable(non_restartable)]
#[allow(non_snake_case)]
pub fn LazyRow(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::remember_lazy_list_state(0, 0))] state: LazyListState,
    #[default(crate::layout::PaddingValues::ZERO)] content_padding: PaddingValues,
    #[default(crate::layout::Arrangement::Start)] horizontal_arrangement: Arrangement,
    #[default(crate::Alignment::TOP_START)] vertical_alignment: Alignment,
    content: impl Fn(LazyListScope) + 'static,
) {
    LazyList(
        modifier,
        state,
        content_padding,
        false,
        horizontal_arrangement,
        vertical_alignment,
        content,
    );
}

#[composable(non_restartable)]
#[allow(non_snake_case)]
fn LazyList(
    modifier: Modifier,
    state: LazyListState,
    content_padding: PaddingValues,
    vertical: bool,
    arrangement: Arrangement,
    alignment: Alignment,
    content: impl Fn(LazyListScope) + 'static,
) {
    let scope = LazyListScope::default();
    content(scope.clone());
    let intervals = std::mem::take(&mut *scope.intervals.borrow_mut());
    let items = Rc::new(Items::new(intervals));
    let info = ListInfo {
        items: items.clone(),
        vertical,
        padding: content_padding,
        arrangement,
        alignment,
    };
    let policy = {
        let state = state.clone();
        compose_runtime::remember_keyed((state.clone(), InfoKey(info.clone())), || {
            Rc::new(LazyListLayout { state: state.inner.clone(), info }) as Rc<dyn MeasurePolicy>
        })
    };
    let modifier = modifier
        .clip(compose_ui::RECTANGLE_SHAPE)
        .then_custom(LazyScrollElement { state: state.clone(), vertical });
    let items = ItemsRef(items);
    // Until the list is measured, its items' likely constraints: those around it, unbounded
    // along the list.
    let fallback = compose_ui::LOCAL_CONSTRAINTS_HINT.current().map(|around| {
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
        compose_runtime::callback(move || LazyItems(state.clone(), items.clone(), fallback)),
    );
}

/// The items composed: a restartable scope of its own, so a change of the range recomposes it
/// and not the list's caller; each item skips when it stays.
#[composable]
#[allow(non_snake_case)]
fn LazyItems(state: LazyListState, items: ItemsRef, fallback: Option<Constraints>) {
    let (start, end) = state.inner.composed.get();
    let count = items.0.count;
    let mut indices: Vec<usize> = (start.min(count)..end.min(count)).collect();
    // The focused item, by key (gone with its item).
    if let Some(pinned) = state.inner.pinned.get()
        && let Some(index) = items.0.index_of(pinned, state.inner.pinned_near.get())
    {
        state.inner.pinned_near.set(index);
        if !indices.contains(&index) {
            indices.push(index);
        }
    }
    *state.inner.children.borrow_mut() = indices.clone();
    // The items' constraints, for those composing for theirs (`BoxWithConstraints`).
    let hint = state.inner.item_constraints.get().or(fallback);
    compose_runtime::CompositionLocalProvider(
        compose_ui::LOCAL_CONSTRAINTS_HINT.provides(hint),
        || {
            for &index in &indices {
                let item_key = items.0.key(index);
                let (state, items) = (state.clone(), items.clone());
                key(item_key, move || LazyItem(index, state.clone(), items.clone()));
            }
        },
    );
}

/// One item: a layout node around its content, which pins the item while it holds the focus.
#[composable]
#[allow(non_snake_case)]
fn LazyItem(index: usize, state: LazyListState, items: ItemsRef) {
    let policy = remember(|| Rc::new(ItemLayout) as Rc<dyn MeasurePolicy>);
    let pinned = state.inner.pinned.handle();
    let item_key = items.0.key(index);
    let near = state.clone();
    // An item is a repaint boundary: scrolled, or beside an animation, it's not recorded again.
    let modifier = Modifier.repaint_boundary().on_focus_changed(move |focused: bool| {
        if focused {
            near.inner.pinned_near.set(index);
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
                (interval.content)(local);
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
}

/// Measures an item by index (once), recording its size; `None` if it isn't composed.
type MeasureItem<'a> = dyn FnMut(usize, &mut HashMap<usize, i32>) -> Option<Placeable> + 'a;

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
        // A row is laid out left to right, and `place_relative` mirrors it in a right-to-left
        // layout; a column's items align in the direction and are placed as they are, as
        // Compose's (so a centered item rounds as in left to right).
        let layout_direction = scope.layout_direction();
        state.rtl.set(layout_direction == compose_ui::LayoutDirection::Rtl);
        let direction = compose_ui::LayoutDirection::Ltr;
        let px = |dp: Dp| density.round_to_px(dp);
        let (before, after, cross_before, cross_after) = if vertical {
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
        let count = info.items.count;
        let children = state.children.borrow().clone();
        let (composed_start, composed_end) = composed_range(&children, count);
        // Each child measured once, when first needed.
        let mut placeables: Vec<Option<Placeable>> = vec![None; measurables.len()];
        let mut sizes = state.sizes.borrow_mut();
        let main_of =
            move |placeable: &Placeable| if vertical { placeable.height } else { placeable.width };
        let mut measure_child =
            |index: usize, sizes: &mut HashMap<usize, i32>| -> Option<Placeable> {
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
        let mut offset = offset + state.pending.replace(0.0);
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
                         sizes: &mut HashMap<usize, i32>,
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
                        let average = average_size(sizes) as f32 + spacing as f32;
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
                let average = average_size(&sizes) as f32 + spacing as f32;
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
        // Content shorter than the viewport, not scrolled: placed by the arrangement.
        if !missing
            && end_index == count
            && index == 0
            && offset == 0.0
            && content_end < main_max
            && !placed.is_empty()
        {
            let item_sizes: Vec<i32> =
                placed.iter().map(|(placeable, _)| main_of(placeable)).collect();
            let mut positions = vec![0; item_sizes.len()];
            info.arrangement.arrange(
                main_max - before - after,
                &item_sizes,
                direction,
                &density,
                &mut positions,
            );
            for ((_, main), position) in placed.iter_mut().zip(positions) {
                *main = before + position;
            }
        }
        // The items wanted composed: those in view and a few beyond, and as many more as the
        // room left would take, at the smallest size seen, if items were missing.
        // A viewport's worth of items beyond each side (at least two), so focus can move to
        // the next focusable item and scrolling finds them ready.
        let average = average_size(&sizes);
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
        if !covers || too_many {
            state.composed.set((wanted_start, wanted_end));
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
        // them and a focused item scrolled away keeps its node.
        let mut beyond: Vec<(Placeable, i32)> = Vec::new();
        let mut main =
            placed.last().map_or(before, |(placeable, main)| main + main_of(placeable) + spacing);
        for next in end_index..composed_end.min(count) {
            if let Some(placeable) = measure(next, &mut sizes) {
                beyond.push((placeable, main));
                main += main_of(&placeable) + spacing;
            }
        }
        // How far the list can scroll each way: known when the composed items reach its ends.
        let extent_end = main - spacing + after;
        let mut main = placed.first().map_or(before, |(_, main)| *main);
        for previous in (composed_start.min(index)..index).rev() {
            if let Some(placeable) = measure(previous, &mut sizes) {
                main -= main_of(&placeable) + spacing;
                beyond.push((placeable, main));
            }
        }
        let extent_start = main - before;
        let pending = state.pending.get();
        let reaches_start = (index == 0 || composed_start == 0) && wanted_before == 0;
        // Not reaching an end: estimated from the average size (a nested list's wheel delta
        // it can't use goes on to its parent).
        let average = average_size(&sizes) as f32 + spacing as f32;
        state.room_backward.set(if reaches_start {
            (-extent_start) as f32 + pending
        } else {
            (composed_start.min(index) as f32 * average - extent_start as f32 + pending).max(1.0)
        });
        let reaches_end = !missing && (end_index == count || composed_end >= count);
        state.room_forward.set(if reaches_end {
            (extent_end.max(content_end) - main_max).max(0) as f32 - pending
        } else {
            let after_composed = count.saturating_sub(composed_end.max(end_index)) as f32 * average;
            (after_composed + (extent_end.max(content_end) - main_max) as f32 - pending).max(1.0)
        });
        let children = state.children.borrow().clone();
        for &pinned in &children {
            if (pinned < composed_start || pinned >= composed_end)
                && let Some(placeable) = measure(pinned, &mut sizes)
            {
                let at =
                    if pinned < index { -main_of(&placeable) - main_max } else { main_max * 2 };
                beyond.push((placeable, at));
            }
        }
        // Sizes far from view are forgotten, so the estimates follow the data.
        if sizes.len() > 4 * MOST_MISSING {
            sizes.retain(|item, _| item.abs_diff(index) <= MOST_MISSING);
        }
        drop(sizes);
        // Placed along the axis, aligned across it.
        let cross_size = placed
            .iter()
            .map(|(placeable, _)| if vertical { placeable.width } else { placeable.height })
            .max()
            .unwrap_or(0);
        let scrolled = index > 0 || offset > 0.0;
        let main_size = if scrolled { main_max } else { content_end.min(main_max) };
        let (width, height) = if vertical {
            (
                constraints.constrain_width(cross_size + cross_before + cross_after),
                constraints.constrain_height(main_size),
            )
        } else {
            (
                constraints.constrain_width(main_size),
                constraints.constrain_height(cross_size + cross_before + cross_after),
            )
        };
        let layout = |scope: &mut MeasureScope<'_>, placeable: &Placeable, main: i32| {
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
        for (placeable, main) in placed.iter().chain(&beyond) {
            layout(scope, placeable, *main);
        }
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "LazyList"
    }
}

/// The average item size measured (at least a pixel): what estimates of positions and of how
/// many items fit use.
fn average_size(sizes: &HashMap<usize, i32>) -> i32 {
    if sizes.is_empty() {
        return 1;
    }
    (sizes.values().map(|size| i64::from(*size)).sum::<i64>() / sizes.len() as i64).max(1) as i32
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

/// The wheel and drags scroll the list.
struct LazyScrollElement {
    state: LazyListState,
    vertical: bool,
}

struct LazyScrollNode {
    state: LazyListState,
    vertical: bool,
    drag: DragScroller,
}

impl ModifierNodeElement for LazyScrollElement {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(LazyScrollNode {
            state: self.state.clone(),
            vertical: self.vertical,
            drag: DragScroller::default(),
        })
    }

    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<LazyScrollNode>() {
            node.state = self.state.clone();
            node.vertical = self.vertical;
        }
    }

    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other
            .downcast_ref::<LazyScrollElement>()
            .is_some_and(|other| other.vertical == self.vertical && other.state.same(&self.state))
    }

    fn name(&self) -> &'static str {
        "lazy_scroll"
    }
}

impl ModifierNode for LazyScrollNode {
    fn on_detach(&mut self, _context: &mut compose_ui::DetachContext<'_>) {
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
        let rtl = !self.vertical && self.state.inner.rtl.get();
        let (start, end, length) = crate::scroll::scroll_axis(rect, viewport, self.vertical, rtl);
        // Where it will be once the scrolling asked for (not yet laid out) is.
        let pending = self.state.inner.pending.get();
        let delta = bring_into_view_delta(start - pending, end - pending, length);
        let used = if delta == 0.0 { 0.0 } else { self.state.dispatch(delta) };
        Some(crate::scroll::scrolled_rect(rect, used, self.vertical, rtl))
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
        let state = self.state.clone();
        let reverse =
            !self.vertical && context.layout_direction() == compose_ui::LayoutDirection::Rtl;
        let flung = state.clone();
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
