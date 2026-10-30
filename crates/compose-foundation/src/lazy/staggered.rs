//! Lazy staggered grids, as Compose's `LazyVerticalStaggeredGrid` and
//! `LazyHorizontalStaggeredGrid`: lanes of items of their own sizes, each item going in the lane
//! that ends first, or taking them all (`StaggeredGridItemSpan::FULL_LINE`).
//!
//! The position is per lane, and which lane an item is in depends on the sizes of those before it,
//! so its measurement is Compose's `measureStaggeredGrid` line by line. What differs is where items
//! come from. Compose composes one as its measurement asks for it; here items are composed in a
//! range, by the lazy list's item composition (`LazyItems`), and a measurement reaching one that
//! isn't composed stops, asks for it and is run again in the frame's next round (D1469). Nothing
//! of a measurement that stopped is kept but the lanes it found, which it would find again.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::hash::Hash;
use std::rc::Rc;

use compose_runtime::{ComposeArg, ImmutableList, ObjectHandle, Owned, composable};
use compose_ui::{
    Constraints, Density, Dp, IntOffset, IntSize, Layout, Measurable, MeasurePolicy, MeasureResult,
    MeasureScope, Modifier, PaddingValues, Placeable, PrefetchScheduler,
};

use super::grid::{CrossAxisCells, GridItemSpan, Slots};
use super::layout_info::{LaidItem, LaidViewport};
use super::prefetch::Policy;
use super::{
    FIRST_STEP, Interval, ItemStacking, Items, ItemsRef, LazyItems, LazyListInner, LazyListState,
    LazyScrollElement, ListObserver, MOST_IN_A_ROUND, MOST_MISSING, composed_range,
    content_type_of,
};
use crate::OverscrollModifier;
use crate::gestures::{
    LocalBoxFuture, Orientation, ScrollBlock, ScrollScope, ScrollableState, ScrollableStateExt,
};
use crate::layout::{HorizontalArrangement, MainAxisArrangement, VerticalArrangement};
use crate::mutator::{MutatePriority, MutationInterrupted};

/// How a staggered grid's cross axis is cut into lanes, as Compose's `StaggeredGridCells`: a
/// vertical grid's columns, a horizontal one's rows.
#[derive(Clone)]
pub enum StaggeredGridCells {
    /// This many lanes, as `Fixed`. Panics at measurement at 0, as Compose's at its construction.
    Fixed(usize),
    /// As many lanes as fit at least this large, as `Adaptive`.
    Adaptive(Dp),
    /// As many lanes as fit of exactly this size, one as large as the room if not even one fits,
    /// as `FixedSize`.
    FixedSize(Dp),
    /// Lanes of your own, as an implementation of the interface.
    Custom(Rc<dyn CrossAxisCells>),
}

impl PartialEq for StaggeredGridCells {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Fixed(one), Self::Fixed(other)) => one == other,
            (Self::Adaptive(one), Self::Adaptive(other)) => one == other,
            (Self::FixedSize(one), Self::FixedSize(other)) => one == other,
            (Self::Custom(one), Self::Custom(other)) => Rc::ptr_eq(one, other),
            _ => false,
        }
    }
}

impl std::fmt::Debug for StaggeredGridCells {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fixed(count) => write!(formatter, "Fixed({count})"),
            Self::Adaptive(min) => write!(formatter, "Adaptive({min:?})"),
            Self::FixedSize(size) => write!(formatter, "FixedSize({size:?})"),
            Self::Custom(_) => formatter.write_str("Custom"),
        }
    }
}

impl ComposeArg for StaggeredGridCells {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl StaggeredGridCells {
    /// As `calculateCrossAxisCellSizes`.
    fn sizes(&self, available: i32, spacing: i32, density: Density) -> Vec<i32> {
        match self {
            Self::Fixed(count) => {
                assert!(*count > 0, "grid with no rows/columns");
                even_sizes(available, *count as i32, spacing)
            }
            Self::Adaptive(min) => {
                assert!(min.0 > 0.0, "invalid minSize");
                let count = (available + spacing) / (density.round_to_px(*min) + spacing);
                even_sizes(available, count.max(1), spacing)
            }
            Self::FixedSize(size) => {
                let size = density.round_to_px(*size);
                if size + spacing < available + spacing {
                    vec![size; ((available + spacing) / (size + spacing)) as usize]
                } else {
                    vec![available]
                }
            }
            Self::Custom(cells) => {
                cells.calculate_cross_axis_cell_sizes(density, available, spacing)
            }
        }
    }
}

/// As `calculateCellsCrossAxisSizeImpl`: the first ones a pixel larger for what's left.
fn even_sizes(available: i32, count: i32, spacing: i32) -> Vec<i32> {
    let without_spacing = available - spacing * (count - 1);
    let (size, remaining) = (without_spacing / count, without_spacing % count);
    (0..count).map(|lane| if size < 0 { 0 } else { size + i32::from(lane < remaining) }).collect()
}

/// How much of a line an item takes, as Compose's `StaggeredGridItemSpan`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StaggeredGridItemSpan {
    /// Every lane, as `FullLine`.
    FullLine,
    /// One lane, as `SingleLane`, the default.
    SingleLane,
}

impl StaggeredGridItemSpan {
    /// In the terms of a grid's interval: 0 lanes is a whole line.
    fn cells(self) -> GridItemSpan {
        GridItemSpan(usize::from(self == StaggeredGridItemSpan::SingleLane))
    }
}

/// An item in view in a staggered grid, as Compose's `LazyStaggeredGridItemInfo`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct LazyStaggeredGridItemInfo {
    pub index: usize,
    /// Its key, hashed (D170).
    pub key: u64,
    /// Its position from the start of the content (after the start padding).
    pub offset: IntOffset,
    pub size: IntSize,
    /// The column of a vertical grid, the row of a horizontal one.
    pub lane: usize,
    pub content_type: Option<u64>,
}

/// What a staggered grid laid out last, as Compose's `LazyStaggeredGridLayoutInfo`.
#[derive(Clone, PartialEq, Debug)]
pub struct LazyStaggeredGridLayoutInfo {
    /// The items in view, by index.
    pub visible_items_info: Vec<LazyStaggeredGridItemInfo>,
    pub viewport_start_offset: i32,
    pub viewport_end_offset: i32,
    pub total_items_count: usize,
    pub viewport_size: IntSize,
    pub orientation: Orientation,
    pub reverse_layout: bool,
    pub before_content_padding: i32,
    pub after_content_padding: i32,
    pub main_axis_item_spacing: i32,
}

impl Default for LazyStaggeredGridLayoutInfo {
    fn default() -> LazyStaggeredGridLayoutInfo {
        LazyStaggeredGridLayoutInfo {
            visible_items_info: Vec::new(),
            viewport_start_offset: 0,
            viewport_end_offset: 0,
            total_items_count: 0,
            viewport_size: IntSize::new(0, 0),
            orientation: Orientation::Horizontal,
            reverse_layout: false,
            before_content_padding: 0,
            after_content_padding: 0,
            main_axis_item_spacing: 0,
        }
    }
}

/// Which lane each item went in, as Compose's `LazyStaggeredGridLaneInfo`: a sliding window of them
/// around where the grid is, and the gaps a full line left in the lanes it evened.
#[doc(hidden)]
pub struct LazyStaggeredGridLaneInfo {
    anchor: i32,
    /// The lanes from `anchor`, plus 1: 0 for none (`LANE_UNSET`).
    lanes: Vec<i32>,
    /// By index.
    spanned_items: VecDeque<(i32, Vec<i32>)>,
}

impl Default for LazyStaggeredGridLaneInfo {
    fn default() -> Self {
        LazyStaggeredGridLaneInfo { anchor: 0, lanes: vec![0; 16], spanned_items: VecDeque::new() }
    }
}

/// `MaxCapacity`.
const MAX_CAPACITY: i32 = 131_072;

impl LazyStaggeredGridLaneInfo {
    /// `LaneUnset`.
    pub const LANE_UNSET: i32 = -1;
    /// `LaneFullSpan`.
    pub const LANE_FULL_SPAN: i32 = -2;

    pub fn new() -> Self {
        Self::default()
    }

    /// As `setLane`.
    pub fn set_lane(&mut self, item: i32, lane: i32) {
        assert!(item >= 0, "Negative lanes are not supported");
        self.ensure_valid_index(item);
        let at = (item - self.anchor) as usize;
        self.lanes[at] = lane + 1;
    }

    /// As `getLane`.
    pub fn get_lane(&self, item: i32) -> i32 {
        if item < self.lower_bound() || item >= self.upper_bound() {
            return Self::LANE_UNSET;
        }
        self.lanes[(item - self.anchor) as usize] - 1
    }

    /// As `assignedToLane`.
    pub fn assigned_to_lane(&self, item: i32, target: i32) -> bool {
        let lane = self.get_lane(item);
        lane == target || lane == Self::LANE_UNSET || lane == Self::LANE_FULL_SPAN
    }

    /// As `upperBound`.
    pub fn upper_bound(&self) -> i32 {
        self.anchor + self.lanes.len() as i32
    }

    /// As `lowerBound`.
    pub fn lower_bound(&self) -> i32 {
        self.anchor
    }

    /// As `reset`.
    pub fn reset(&mut self) {
        self.lanes.fill(0);
        self.spanned_items.clear();
    }

    /// As `findPreviousItemIndex`: -1 if none.
    pub fn find_previous_item_index(&self, item: i32, target: i32) -> i32 {
        (0..item).rev().find(|&index| self.assigned_to_lane(index, target)).unwrap_or(-1)
    }

    /// As `findNextItemIndex`: `upper_bound` if none.
    pub fn find_next_item_index(&self, item: i32, target: i32) -> i32 {
        ((item + 1)..self.upper_bound())
            .find(|&index| self.assigned_to_lane(index, target))
            .unwrap_or_else(|| self.upper_bound())
    }

    /// As `ensureValidIndex`.
    pub fn ensure_valid_index(&mut self, requested: i32) {
        let capacity = requested - self.anchor;
        if (0..MAX_CAPACITY).contains(&capacity) {
            self.ensure_capacity(capacity + 1, 0);
        } else {
            // Beyond the window: the anchor moves so the requested index is in its middle.
            let old = self.anchor;
            let size = self.lanes.len() as i32;
            self.anchor = (requested - size / 2).max(0);
            let delta = self.anchor - old;
            if delta >= 0 {
                if delta < size {
                    self.lanes.copy_within(delta as usize.., 0);
                }
                self.lanes[(size - delta).max(0) as usize..].fill(0);
            } else {
                let delta = -delta;
                if size + delta < MAX_CAPACITY {
                    self.ensure_capacity(size + delta + 1, delta);
                } else {
                    if delta < size {
                        self.lanes.copy_within(..(size - delta) as usize, delta as usize);
                    }
                    self.lanes[..size.min(delta) as usize].fill(0);
                }
            }
        }
        // The full lines out of the window are forgotten.
        while self.spanned_items.front().is_some_and(|item| item.0 < self.lower_bound()) {
            self.spanned_items.pop_front();
        }
        while self.spanned_items.back().is_some_and(|item| item.0 > self.upper_bound()) {
            self.spanned_items.pop_back();
        }
    }

    /// As `setGaps`.
    pub fn set_gaps(&mut self, item: i32, gaps: Option<Vec<i32>>) {
        match (self.spanned_items.binary_search_by_key(&item, |item| item.0), gaps) {
            (Err(_), None) => {}
            (Err(at), Some(gaps)) => self.spanned_items.insert(at, (item, gaps)),
            (Ok(at), None) => {
                self.spanned_items.remove(at);
            }
            (Ok(at), Some(gaps)) => self.spanned_items[at].1 = gaps,
        }
    }

    /// As `getGaps`.
    pub fn get_gaps(&self, item: i32) -> Option<&[i32]> {
        let at = self.spanned_items.binary_search_by_key(&item, |item| item.0).ok()?;
        Some(&self.spanned_items[at].1)
    }

    fn ensure_capacity(&mut self, capacity: i32, new_offset: i32) {
        assert!(
            capacity <= MAX_CAPACITY,
            "Requested item capacity {capacity} is larger than max supported: {MAX_CAPACITY}!"
        );
        let size = self.lanes.len() as i32;
        if size < capacity {
            let mut new_size = size;
            while new_size < capacity {
                new_size *= 2;
            }
            let mut lanes = vec![0; new_size as usize];
            let copied = (new_size - new_offset).min(size) as usize;
            lanes[new_offset as usize..new_offset as usize + copied]
                .copy_from_slice(&self.lanes[..copied]);
            self.lanes = lanes;
        }
    }
}

/// `calculateFirstVisibleIndex`: a lane empty (more lanes than items), or the first, is item 0.
fn first_visible_index(indices: &[i32]) -> i32 {
    let mut least = i32::MAX;
    for &index in indices {
        if index <= 0 {
            return 0;
        }
        least = least.min(index);
    }
    if least == i32::MAX { 0 } else { least }
}

/// `calculateFirstVisibleScrollOffset`.
fn first_visible_offset(indices: &[i32], offsets: &[i32]) -> i32 {
    let first = first_visible_index(indices);
    let found = offsets
        .iter()
        .zip(indices)
        .filter(|(_, index)| **index == first)
        .map(|(offset, _)| *offset)
        .min();
    found.unwrap_or(0)
}

/// Where a staggered grid is scrolled, as Compose's `LazyStaggeredGridState`.
///
/// A `Copy` handle (D240): [`new`](Self::new) returns its owner, and
/// [`remember_lazy_staggered_grid_state`] keeps that in the composition.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct LazyStaggeredGridState {
    list: LazyListState,
    object: ObjectHandle<StaggeredInner>,
}

/// The list state its scrolling, composition and prefetch are in, and the position per lane.
struct StaggeredInner {
    _list: Owned<LazyListState>,
    /// As `laneInfo`.
    lane_info: RefCell<LazyStaggeredGridLaneInfo>,
    /// As `scrollPosition.indices` and `scrollOffsets`.
    position: RefCell<(Vec<i32>, Vec<i32>)>,
    /// A jump since the last layout: what was laid out says nothing of where it is. A layout comes
    /// after each jump in Compose's (`forceRemeasure`), at the next frame here (D1469).
    jumped: Cell<bool>,
}

impl LazyStaggeredGridState {
    /// At item `first_visible_item_index`, `first_visible_item_scroll_offset` pixels into it.
    #[track_caller]
    pub fn new(
        first_visible_item_index: usize,
        first_visible_item_scroll_offset: i32,
    ) -> Owned<LazyStaggeredGridState> {
        Self::with_lanes(
            vec![first_visible_item_index as i32],
            vec![first_visible_item_scroll_offset],
        )
    }

    #[track_caller]
    fn with_lanes(indices: Vec<i32>, offsets: Vec<i32>) -> Owned<LazyStaggeredGridState> {
        let first = first_visible_index(&indices) as usize;
        let offset = first_visible_offset(&indices, &offsets);
        let list = LazyListState::new(first, offset);
        let handle = *list;
        Owned::new(
            StaggeredInner {
                _list: list,
                lane_info: RefCell::default(),
                position: RefCell::new((indices, offsets)),
                jumped: Cell::new(false),
            },
            |object| LazyStaggeredGridState { list: handle, object },
        )
    }

    fn inner(&self) -> Option<Rc<StaggeredInner>> {
        self.object.try_get()
    }

    /// The first item across the lanes, as `firstVisibleItemIndex` (a state read).
    pub fn first_visible_item_index(&self) -> usize {
        self.list.first_visible_item_index()
    }

    /// How far it's scrolled off, as `firstVisibleItemScrollOffset` (a state read).
    pub fn first_visible_item_scroll_offset(&self) -> i32 {
        self.list.first_visible_item_scroll_offset()
    }

    /// What it laid out last, as `layoutInfo` (a state read).
    pub fn layout_info(&self) -> LazyStaggeredGridLayoutInfo {
        let Some(inner) = self.list.inner() else {
            return LazyStaggeredGridLayoutInfo::default();
        };
        let items = inner.laid_items.borrow().clone();
        inner.laid_out.read(|laid, viewport| LazyStaggeredGridLayoutInfo {
            visible_items_info: laid
                .iter()
                .map(|item| {
                    let (offset, size) = if viewport.vertical {
                        (
                            IntOffset::new(item.cross, item.main),
                            IntSize::new(item.cross_size, item.main_size),
                        )
                    } else {
                        (
                            IntOffset::new(item.main, item.cross),
                            IntSize::new(item.main_size, item.cross_size),
                        )
                    };
                    LazyStaggeredGridItemInfo {
                        index: item.index,
                        key: items.as_ref().map_or(0, |items| items.key(item.index)),
                        offset,
                        size,
                        lane: item.lane as usize,
                        content_type: items
                            .as_ref()
                            .and_then(|items| items.content_type(item.index)),
                    }
                })
                .collect(),
            viewport_start_offset: viewport.start,
            viewport_end_offset: viewport.end,
            total_items_count: viewport.count,
            viewport_size: viewport.size,
            orientation: if viewport.vertical {
                Orientation::Vertical
            } else {
                Orientation::Horizontal
            },
            reverse_layout: viewport.reverse,
            before_content_padding: viewport.before,
            after_content_padding: viewport.after,
            main_axis_item_spacing: viewport.spacing,
        })
    }

    /// As `snapToItemInternal`: an item in view moves the lanes by as much, keeping them; another
    /// starts them again around it.
    fn snap_to_item(&self, index: usize, scroll_offset: i32) {
        let (Some(inner), Some(list)) = (self.inner(), self.list.inner()) else { return };
        let changed = list.published.get() != (index, scroll_offset);
        let visible = list.laid_out.read_untracked(|laid, _| {
            laid.iter().find(|item| item.index == index).map(|item| item.main)
        });
        let jumped = inner.jumped.replace(true);
        let visible = visible.filter(|_| !jumped);
        let mut position = inner.position.borrow_mut();
        match visible {
            Some(main) if changed => {
                let key = list.first_key.get();
                let delta = main + scroll_offset;
                position.1.iter_mut().for_each(|offset| *offset += delta);
                let first = first_visible_index(&position.0) as usize;
                self.list.snap_to_item(first, first_visible_offset(&position.0, &position.1));
                list.first_key.set(key);
            }
            _ => {
                let lanes = position.0.len();
                let indices = self.fill_indices(&inner, &list, index as i32, lanes);
                let offsets = vec![scroll_offset; lanes];
                *position = (indices, offsets);
                self.list.snap_to_item(index, scroll_offset);
            }
        }
    }

    /// As `fillNearestIndices`: the items around `item` in each lane, as far as they're known.
    fn fill_indices(
        &self,
        inner: &StaggeredInner,
        list: &LazyListInner,
        item: i32,
        lanes: usize,
    ) -> Vec<i32> {
        let items = list.laid_items.borrow();
        if items.as_ref().is_some_and(|items| items.is_full_span(item as usize)) {
            return vec![item; lanes];
        }
        let mut info = inner.lane_info.borrow_mut();
        info.ensure_valid_index(item + lanes as i32);
        let target = match info.get_lane(item) {
            LazyStaggeredGridLaneInfo::LANE_UNSET | LazyStaggeredGridLaneInfo::LANE_FULL_SPAN => 0,
            lane => (lane as usize).min(lanes.saturating_sub(1)),
        };
        let mut indices = vec![0; lanes];
        let mut current = item;
        for lane in (0..target).rev() {
            indices[lane] = info.find_previous_item_index(current, lane as i32);
            if indices[lane] == LazyStaggeredGridLaneInfo::LANE_UNSET {
                indices[..lane].fill(-1);
                break;
            }
            current = indices[lane];
        }
        indices[target] = item;
        current = item;
        for (lane, index) in indices.iter_mut().enumerate().skip(target + 1) {
            *index = info.find_next_item_index(current, lane as i32);
            current = *index;
        }
        indices
    }

    /// As `requestScrollToItem`: at the next layout, a scroll going on being stopped.
    pub fn request_scroll_to_item(&self, index: usize, scroll_offset: i32) {
        let Some(list) = self.list.inner() else { return };
        if list.core.is_scroll_in_progress_untracked() {
            let state = *self;
            compose_runtime::CoroutineScope::new().launch(async move {
                let _ = state.stop_scroll(MutatePriority::Default).await;
            });
        }
        self.snap_to_item(index, scroll_offset);
    }

    /// As `scrollToItem`.
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

    /// As `animateScrollToItem`.
    pub async fn animate_scroll_to_item(
        &self,
        index: usize,
        scroll_offset: i32,
    ) -> Result<(), MutationInterrupted> {
        let state = *self;
        self.scroll(MutatePriority::Default, move |scope| async move {
            let density = state.list.inner().map_or(1.0, |inner| inner.density.get());
            let lanes = state.lane_count().max(1);
            let scope = state.lazy_layout_scroll_scope(scope);
            super::animate_scroll::animate_scroll_to_item(
                &scope,
                index,
                scroll_offset,
                100 * lanes,
                density,
            )
            .await;
        })
        .await
    }

    /// The lanes, as last measured.
    fn lane_count(&self) -> usize {
        self.list.inner().map_or(0, |inner| inner.laid_out.read_untracked(|_, it| it.max_span))
    }

    /// As `LazyLayoutScrollScope(state, scrollScope)`.
    pub fn lazy_layout_scroll_scope(
        &self,
        scroll_scope: ScrollScope,
    ) -> impl super::LazyLayoutScrollScope {
        StaggeredScrollScope { state: *self, scope: scroll_scope }
    }

    /// The interaction source it reports its drags to, as `interactionSource`.
    pub fn interaction_source(&self) -> compose_ui::MutableInteractionSource {
        self.list.interaction_source()
    }

    /// As `saver`: each lane's index and offset.
    pub fn saver() -> impl compose_runtime::Saver<Owned<LazyStaggeredGridState>> {
        use compose_runtime::Saveable;
        compose_runtime::saver(
            |state: &Owned<LazyStaggeredGridState>| {
                let inner = state.inner()?;
                let position = inner.position.borrow();
                Some(position.clone().save())
            },
            |saved| {
                <(Vec<i32>, Vec<i32>)>::restore(saved)
                    .map(|(indices, offsets)| LazyStaggeredGridState::with_lanes(indices, offsets))
            },
        )
    }
}

impl ComposeArg for LazyStaggeredGridState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl ScrollableState for LazyStaggeredGridState {
    fn scroll_boxed<'a>(
        &'a self,
        priority: MutatePriority,
        block: ScrollBlock<'a>,
    ) -> LocalBoxFuture<'a, Result<(), MutationInterrupted>> {
        self.list.scroll_boxed(priority, block)
    }

    fn dispatch_raw_delta(&self, delta: f32) -> f32 {
        self.list.dispatch_raw_delta(delta)
    }

    fn is_scroll_in_progress(&self) -> bool {
        self.list.is_scroll_in_progress()
    }

    fn can_scroll_forward(&self) -> bool {
        self.list.can_scroll_forward()
    }

    fn can_scroll_backward(&self) -> bool {
        self.list.can_scroll_backward()
    }

    fn last_scrolled_forward(&self) -> bool {
        self.list.last_scrolled_forward()
    }

    fn last_scrolled_backward(&self) -> bool {
        self.list.last_scrolled_backward()
    }

    fn internal_interaction_source(&self) -> Option<compose_ui::MutableInteractionSource> {
        self.list.internal_interaction_source()
    }
}

/// A staggered grid remembered at its call site and saved, as `rememberLazyStaggeredGridState`.
#[composable]
pub fn remember_lazy_staggered_grid_state(
    #[default(0)] initial_first_visible_item_index: usize,
    #[default(0)] initial_first_visible_item_scroll_offset: i32,
) -> LazyStaggeredGridState {
    compose_runtime::remember_saveable_with(LazyStaggeredGridState::saver(), || {
        LazyStaggeredGridState::new(
            initial_first_visible_item_index,
            initial_first_visible_item_scroll_offset,
        )
    })
}

/// A grid's scroll scope, as Compose's for a `LazyStaggeredGridState`. A scroll is applied at the
/// next layout (D476): what's scrolled since counts in what it says, as a list's (D858).
struct StaggeredScrollScope {
    state: LazyStaggeredGridState,
    scope: ScrollScope,
}

impl StaggeredScrollScope {
    /// The position ahead once the scrolling asked since the last layout is laid out, the items
    /// taken one by one in the order of their indices (where items are in lanes, an estimate).
    fn ahead<R>(
        &self,
        read: impl FnOnce(&super::layout_info::Ahead, &[LaidItem], f32, usize) -> R,
    ) -> Option<R> {
        let inner = self.state.list.inner()?;
        let pending = inner.pending.get();
        inner.laid_out.read_untracked(|laid, viewport| {
            let lines = super::layout_info::laid_lines(laid, false);
            let ahead = super::layout_info::ahead(
                &lines,
                pending,
                viewport.spacing,
                viewport.end,
                viewport.count,
                viewport.max_span,
            )?;
            Some(read(&ahead, laid, pending, viewport.max_span.max(1)))
        })
    }
}

impl super::LazyLayoutScrollScope for StaggeredScrollScope {
    fn scroll_scope(&self) -> &ScrollScope {
        &self.scope
    }

    fn first_visible_item_index(&self) -> usize {
        self.ahead(|ahead, _, _, _| ahead.first)
            .unwrap_or_else(|| self.state.first_visible_item_index())
    }

    fn first_visible_item_scroll_offset(&self) -> i32 {
        self.ahead(|ahead, _, _, _| ahead.offset)
            .unwrap_or_else(|| self.state.first_visible_item_scroll_offset())
    }

    fn last_visible_item_index(&self) -> usize {
        self.ahead(|ahead, _, _, _| ahead.last).unwrap_or(0)
    }

    fn item_count(&self) -> usize {
        self.state.list.inner().map_or(0, |inner| inner.laid_out.read_untracked(|_, it| it.count))
    }

    fn snap_to_item(&self, index: usize, offset: i32) {
        self.state.snap_to_item(index, offset);
    }

    fn calculate_distance_to(&self, target_index: usize, target_offset: i32) -> i32 {
        self.ahead(|ahead, laid, pending, lanes| {
            let distance = match laid.iter().find(|item| item.index == target_index) {
                Some(item) => (item.main as f32 - pending).round() as i32,
                None => {
                    let lanes = lanes as i32;
                    let lines = target_index as i32 / lanes - ahead.first as i32 / lanes;
                    ahead.average * lines - ahead.offset
                }
            };
            distance + target_offset
        })
        .unwrap_or(0)
    }
}

/// What a staggered grid's content declares its items with, as Compose's
/// `LazyStaggeredGridScope`. The `_with_span` forms take Compose's `span` and `contentType`; the
/// others its defaults, a single lane and the content type `{ null }`.
#[derive(Clone, Default)]
pub struct LazyStaggeredGridScope {
    intervals: Rc<RefCell<Vec<Interval>>>,
}

impl ComposeArg for LazyStaggeredGridScope {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.intervals, &other.intervals)
    }
}

impl LazyStaggeredGridScope {
    fn push(&self, interval: Interval) {
        self.intervals.borrow_mut().push(interval);
    }

    /// One item, as `item { … }`.
    pub fn item(&self, content: impl Fn() + 'static) {
        self.push(Interval::new(1, Rc::new(move |_| content())));
    }

    /// One item with a key, as `item(key) { … }`.
    pub fn item_keyed<K: Hash + PartialEq + Clone + 'static>(
        &self,
        key: K,
        content: impl Fn() + 'static,
    ) {
        self.push(Interval::keyed(1, move |_| key.clone(), Rc::new(move |_| content())));
    }

    /// One item with a span and a content type, as `item(contentType, span) { … }`.
    pub fn item_with_span<C: Hash>(
        &self,
        content_type: C,
        span: StaggeredGridItemSpan,
        content: impl Fn() + 'static,
    ) {
        let content_type = content_type_of(&content_type);
        self.push(
            Interval::new(1, Rc::new(move |_| content()))
                .typed(Rc::new(move |_| content_type))
                .spanned(Rc::new(move |_, _| span.cells())),
        );
    }

    /// As `item(key, contentType, span) { … }`.
    pub fn item_keyed_with_span<K: Hash + PartialEq + Clone + 'static, C: Hash>(
        &self,
        key: K,
        content_type: C,
        span: StaggeredGridItemSpan,
        content: impl Fn() + 'static,
    ) {
        let content_type = content_type_of(&content_type);
        self.push(
            Interval::keyed(1, move |_| key.clone(), Rc::new(move |_| content()))
                .typed(Rc::new(move |_| content_type))
                .spanned(Rc::new(move |_, _| span.cells())),
        );
    }

    /// `count` items, as `items(count) { index -> … }`.
    pub fn items(&self, count: usize, content: impl Fn(usize) + 'static) {
        self.push(Interval::new(count, Rc::new(content)));
    }

    /// `count` items with keys, as `items(count, key = { … }) { … }`.
    pub fn items_keyed<K: Hash + PartialEq + 'static>(
        &self,
        count: usize,
        key: impl Fn(usize) -> K + 'static,
        content: impl Fn(usize) + 'static,
    ) {
        self.push(Interval::keyed(count, key, Rc::new(content)));
    }

    /// As `items(count, contentType = { … }, span = { … }) { … }`.
    pub fn items_with_span<C: Hash>(
        &self,
        count: usize,
        content_type: impl Fn(usize) -> C + 'static,
        span: impl Fn(usize) -> StaggeredGridItemSpan + 'static,
        content: impl Fn(usize) + 'static,
    ) {
        self.push(
            Interval::new(count, Rc::new(content))
                .typed(Rc::new(move |index| content_type_of(&content_type(index))))
                .spanned(Rc::new(move |_, index| span(index).cells())),
        );
    }

    /// As `items(count, key = { … }, contentType = { … }, span = { … }) { … }`.
    pub fn items_keyed_with_span<K: Hash + PartialEq + 'static, C: Hash>(
        &self,
        count: usize,
        key: impl Fn(usize) -> K + 'static,
        content_type: impl Fn(usize) -> C + 'static,
        span: impl Fn(usize) -> StaggeredGridItemSpan + 'static,
        content: impl Fn(usize) + 'static,
    ) {
        self.push(
            Interval::keyed(count, key, Rc::new(content))
                .typed(Rc::new(move |index| content_type_of(&content_type(index))))
                .spanned(Rc::new(move |_, index| span(index).cells())),
        );
    }

    /// An item for each of `items`, as `items(items) { item -> … }`.
    pub fn items_list<T: 'static>(
        &self,
        items: impl Into<ImmutableList<T>>,
        content: impl Fn(&T) + 'static,
    ) {
        let items = items.into();
        self.push(Interval::new(items.len(), Rc::new(move |index| content(&items[index]))));
    }

    /// `items_list` with keys, as `items(items, key = { … })`.
    pub fn items_list_keyed<T: 'static, K: Hash + PartialEq + 'static>(
        &self,
        items: impl Into<ImmutableList<T>>,
        key: impl Fn(&T) -> K + 'static,
        content: impl Fn(&T) + 'static,
    ) {
        let items = items.into();
        let keys = items.clone();
        self.push(Interval::keyed(
            items.len(),
            move |index| key(&keys[index]),
            Rc::new(move |index| content(&items[index])),
        ));
    }

    /// `items_list` with keys, content types and spans, as
    /// `items(items, key = { … }, contentType = { … }, span = { … })`.
    pub fn items_list_keyed_with_span<T: 'static, K: Hash + PartialEq + 'static, C: Hash>(
        &self,
        items: impl Into<ImmutableList<T>>,
        key: impl Fn(&T) -> K + 'static,
        content_type: impl Fn(&T) -> C + 'static,
        span: impl Fn(&T) -> StaggeredGridItemSpan + 'static,
        content: impl Fn(&T) + 'static,
    ) {
        let items = items.into();
        let (keys, types, spans) = (items.clone(), items.clone(), items.clone());
        self.push(
            Interval::keyed(
                items.len(),
                move |index| key(&keys[index]),
                Rc::new(move |index| content(&items[index])),
            )
            .typed(Rc::new(move |index| content_type_of(&content_type(&types[index]))))
            .spanned(Rc::new(move |_, index| span(&spans[index]).cells())),
        );
    }

    /// An item for each of `items`, with its index, as `itemsIndexed(items) { index, item -> … }`.
    pub fn items_indexed<T: 'static>(
        &self,
        items: impl Into<ImmutableList<T>>,
        content: impl Fn(usize, &T) + 'static,
    ) {
        let items = items.into();
        self.push(Interval::new(items.len(), Rc::new(move |index| content(index, &items[index]))));
    }

    /// `items_indexed` with keys, as `itemsIndexed(items, key = { … })`.
    pub fn items_indexed_keyed<T: 'static, K: Hash + PartialEq + 'static>(
        &self,
        items: impl Into<ImmutableList<T>>,
        key: impl Fn(usize, &T) -> K + 'static,
        content: impl Fn(usize, &T) + 'static,
    ) {
        let items = items.into();
        let keys = items.clone();
        self.push(Interval::keyed(
            items.len(),
            move |index| key(index, &keys[index]),
            Rc::new(move |index| content(index, &items[index])),
        ));
    }
}

/// A vertically scrolling staggered grid, as Compose's `LazyVerticalStaggeredGrid`.
#[composable]
#[allow(non_snake_case)]
pub fn LazyVerticalStaggeredGrid(
    columns: StaggeredGridCells,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::remember_lazy_staggered_grid_state(0, 0))] state: LazyStaggeredGridState,
    #[default(crate::layout::PaddingValues::ZERO)] content_padding: PaddingValues,
    #[default(false)] reverse_layout: bool,
    #[default(Dp(0.0))] vertical_item_spacing: Dp,
    #[default(crate::layout::Arrangement::spaced_by(Dp(0.0)))] horizontal_arrangement: impl Into<
        HorizontalArrangement,
    >,
    #[default(None)] fling_behavior: Option<Rc<dyn crate::gestures::FlingBehavior>>,
    #[default(true)] user_scroll_enabled: bool,
    #[default(crate::remember_overscroll_effect())] overscroll_effect: Option<
        Rc<dyn crate::OverscrollEffect>,
    >,
    content: impl Fn(LazyStaggeredGridScope) + 'static,
) {
    LazyStaggeredGrid(
        modifier,
        state,
        columns,
        content_padding,
        true,
        reverse_layout,
        vertical_item_spacing,
        horizontal_arrangement.0,
        fling_behavior,
        overscroll_effect,
        user_scroll_enabled,
        content,
    );
}

/// A horizontally scrolling staggered grid, as Compose's `LazyHorizontalStaggeredGrid`.
#[composable]
#[allow(non_snake_case)]
pub fn LazyHorizontalStaggeredGrid(
    rows: StaggeredGridCells,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::remember_lazy_staggered_grid_state(0, 0))] state: LazyStaggeredGridState,
    #[default(crate::layout::PaddingValues::ZERO)] content_padding: PaddingValues,
    #[default(false)] reverse_layout: bool,
    #[default(crate::layout::Arrangement::spaced_by(Dp(0.0)))] vertical_arrangement: impl Into<
        VerticalArrangement,
    >,
    #[default(Dp(0.0))] horizontal_item_spacing: Dp,
    #[default(None)] fling_behavior: Option<Rc<dyn crate::gestures::FlingBehavior>>,
    #[default(true)] user_scroll_enabled: bool,
    #[default(crate::remember_overscroll_effect())] overscroll_effect: Option<
        Rc<dyn crate::OverscrollEffect>,
    >,
    content: impl Fn(LazyStaggeredGridScope) + 'static,
) {
    LazyStaggeredGrid(
        modifier,
        state,
        rows,
        content_padding,
        false,
        reverse_layout,
        horizontal_item_spacing,
        vertical_arrangement.0,
        fling_behavior,
        overscroll_effect,
        user_scroll_enabled,
        content,
    );
}

/// What's told of its list state: what its semantics scroll with.
struct StaggeredObserver(LazyStaggeredGridState);

impl ListObserver for StaggeredObserver {
    fn before_measure(
        &self,
        _: &LazyListInner,
        _: &Items,
        _: Density,
        _: i32,
        _: i32,
        _: i32,
        _: i32,
    ) {
    }

    fn after_measure(&self) {}

    fn after_dispatch(&self, _: f32) {}

    fn collection_info(&self) -> compose_ui::CollectionInfo {
        compose_ui::CollectionInfo { row_count: -1, column_count: -1 }
    }

    fn scroll_to_index(&self, index: usize) {
        let state = self.0;
        compose_runtime::CoroutineScope::new().launch(async move {
            let _ = state.scroll_to_item(index, 0).await;
        });
    }
}

/// What the layout needs from composition: a new value is a new policy, which measures again.
#[derive(Clone)]
struct StaggeredInfo {
    items: Rc<Items>,
    cells: StaggeredGridCells,
    vertical: bool,
    reverse: bool,
    padding: PaddingValues,
    main_spacing: Dp,
    cross_arrangement: MainAxisArrangement,
}

impl ComposeArg for StaggeredInfo {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.items, &other.items)
            && self.cells == other.cells
            && self.vertical == other.vertical
            && self.reverse == other.reverse
            && self.padding == other.padding
            && self.main_spacing == other.main_spacing
            && self.cross_arrangement == other.cross_arrangement
    }
}

#[composable(non_restartable)]
#[allow(non_snake_case, clippy::too_many_arguments)]
fn LazyStaggeredGrid(
    modifier: Modifier,
    state: LazyStaggeredGridState,
    cells: StaggeredGridCells,
    content_padding: PaddingValues,
    vertical: bool,
    reverse: bool,
    main_spacing: Dp,
    cross_arrangement: MainAxisArrangement,
    fling_behavior: Option<Rc<dyn crate::gestures::FlingBehavior>>,
    overscroll_effect: Option<Rc<dyn crate::OverscrollEffect>>,
    user_scroll_enabled: bool,
    content: impl Fn(LazyStaggeredGridScope) + 'static,
) {
    let list = state.list;
    let inner = list.inner_or_detached();
    inner.is_grid.set(true);
    let observer = compose_runtime::remember_keyed(state, || {
        Rc::new(StaggeredObserver(state)) as Rc<dyn ListObserver>
    });
    list.set_observer(observer);
    let scope = LazyStaggeredGridScope::default();
    content(scope.clone());
    let intervals = std::mem::take(&mut *scope.intervals.borrow_mut());
    let items = Rc::new(Items::new(intervals));
    super::prefetch::note_nested(&inner);
    let density = compose_ui::LOCAL_DENSITY.current();
    let cross_spacing = cross_arrangement.spacing(&density);
    let info = StaggeredInfo {
        items: items.clone(),
        cells: cells.clone(),
        vertical,
        reverse,
        padding: content_padding,
        main_spacing,
        cross_arrangement,
    };
    let policy = {
        let scheduler = compose_ui::LOCAL_PREFETCH_SCHEDULER.current();
        compose_runtime::remember_keyed((state, info.clone(), scheduler.clone()), || {
            Rc::new(StaggeredLayout {
                state,
                list: list.inner_or_detached(),
                info,
                scheduler,
                slots: RefCell::new(None),
            }) as Rc<dyn MeasurePolicy>
        })
    };
    let modifier = modifier
        .clip(compose_ui::RECTANGLE_SHAPE)
        .overscroll(overscroll_effect.clone())
        .then_custom(LazyScrollElement {
            state: list,
            vertical,
            reverse,
            enabled: user_scroll_enabled,
            fling_behavior,
            overscroll: overscroll_effect,
        })
        .focus_group();
    let items = ItemsRef(items);
    let holder = compose_runtime::remember_saveable_state_holder();
    // Until it's measured, its items' likely constraints: a lane's, in the space around.
    let hinted = matches!(cells, StaggeredGridCells::Custom(_))
        || inner.item_constraints_written.get().is_some();
    let around = if hinted { None } else { compose_ui::platform::LOCAL_CONSTRAINTS_HINT.current() };
    let fallback = around.and_then(|around| {
        let px = |dp: Dp| density.round_to_px(dp);
        let (available, padding) = if vertical {
            (around.max_width, px(content_padding.start) + px(content_padding.end))
        } else {
            (around.max_height, px(content_padding.top) + px(content_padding.bottom))
        };
        if available == Constraints::INFINITY {
            return None;
        }
        let sizes = cells.sizes(available - padding, cross_spacing, density);
        sizes.first().map(|size| super::grid::cell_constraints(vertical, *size))
    });
    let stacking =
        if reverse { ItemStacking::AtTheEnd { vertical } } else { ItemStacking::AtTheOrigin };
    Layout(
        modifier,
        policy,
        compose_runtime::callback(move || {
            LazyItems(list, items.clone(), holder, fallback, stacking)
        }),
    );
}

/// The cells' cache key: the room across, the spacing between them and the density.
type SlotsKey = (i32, i32, Density);

/// The grid's measurement.
struct StaggeredLayout {
    state: LazyStaggeredGridState,
    list: Rc<LazyListInner>,
    info: StaggeredInfo,
    scheduler: Option<PrefetchScheduler>,
    /// The lanes as last worked out, as `LazyStaggeredGridSlotCache`.
    slots: RefCell<Option<(SlotsKey, Rc<Slots>)>>,
}

/// An item measured, as `LazyStaggeredGridMeasuredItem`.
struct MeasuredItem {
    index: i32,
    lane: usize,
    span: usize,
    placeable: Placeable,
    main_size: i32,
    size_with_spacing: i32,
    visible: bool,
    /// Where `position` put it.
    main: i32,
    cross: i32,
}

/// A measurement reached an item that isn't composed.
struct Missing;

/// What's needed to work out, as `LazyStaggeredGridMeasureContext`.
struct Context<'a, 'scope> {
    scope: &'a mut MeasureScope<'scope>,
    measurables: &'a [Measurable],
    children: &'a [usize],
    composed_start: usize,
    /// Each child's placeable, as Compose's `getPlaceables` keeps those measured.
    placeables: Vec<Option<Placeable>>,
    lane_info: &'a mut LazyStaggeredGridLaneInfo,
    slots: &'a Slots,
    items: &'a Items,
    prefetch: &'a super::prefetch::Prefetch,
    unready: Option<u64>,
    lacked_unready: bool,
    sizes: &'a mut super::Sizes,
    vertical: bool,
    count: i32,
    lanes: usize,
    spacing: i32,
    before: i32,
    after: i32,
    available: i32,
    pending: f32,
    /// The items it lacked, first and last.
    lacked: Option<(usize, usize)>,
    /// More it may need after the last one lacked: the room left, in items of the average size.
    lacked_room: usize,
}

/// A lane range, as `SpanRange`: the first lane and how many.
type SpanRange = (usize, usize);

/// A measurement's outcome.
struct Measured {
    first_indices: Vec<i32>,
    first_offsets: Vec<i32>,
    current_indices: Vec<i32>,
    current_offsets: Vec<i32>,
    /// `-first_offsets` before they left out the items under the padding: where items are put from.
    scroll_offsets: Vec<i32>,
    arena: Vec<MeasuredItem>,
    lanes_of_items: Vec<VecDeque<usize>>,
    consumed: f32,
    min_offset: i32,
    max_offset: i32,
}

fn offset_by(values: &mut [i32], delta: i32) {
    values.iter_mut().for_each(|value| *value += delta);
}

/// `maxInRange`.
fn max_in_range(values: &[i32], (start, size): SpanRange) -> i32 {
    values[start..start + size].iter().copied().fold(i32::MIN, i32::max)
}

/// `indexOfMinValue`: -1 if none is above `min_bound`.
fn index_of_min_value(values: &[i32], min_bound: i32) -> i32 {
    let (mut result, mut least) = (-1, i32::MAX);
    for (at, &value) in values.iter().enumerate() {
        if value > min_bound && value < least {
            least = value;
            result = at as i32;
        }
    }
    result
}

/// `indexOfMaxValue`.
fn index_of_max_value(values: &[i32]) -> usize {
    let (mut result, mut most) = (0, i32::MIN);
    for (at, &value) in values.iter().enumerate() {
        if most < value {
            most = value;
            result = at;
        }
    }
    result
}

impl Context<'_, '_> {
    fn span_range(&self, item: i32, lane: usize) -> SpanRange {
        if self.items.is_full_span(item as usize) { (0, self.lanes) } else { (lane, 1) }
    }

    /// `SpanRange.laneInfo`: with one lane, a full line is its lane.
    fn lane_value(&self, (start, size): SpanRange) -> i32 {
        if size != 1 { LazyStaggeredGridLaneInfo::LANE_FULL_SPAN } else { start as i32 }
    }

    fn child_of(&mut self, item: usize) -> Option<usize> {
        if self.unready.is_some_and(|key| key == self.items.key(item)) {
            self.prefetch.complete_pending();
            self.lacked_unready = true;
            return None;
        }
        let child = match item.checked_sub(self.composed_start) {
            Some(position) if self.children.get(position) == Some(&item) => Some(position),
            _ => self.children.iter().position(|composed| *composed == item),
        };
        child.filter(|child| *child < self.measurables.len())
    }

    /// Item `item` in those lanes, as `getAndMeasure`, if it's composed.
    fn try_measure(&mut self, item: i32, span: SpanRange) -> Option<MeasuredItem> {
        let child = self.child_of(item as usize)?;
        let start = span.0.min(self.lanes - 1);
        let size = span.1.min(self.lanes - start);
        let (_, cross) = self.slots.of(start, size);
        let placeable = match self.placeables[child] {
            Some(placeable) => placeable,
            None => {
                let constraints = super::grid::cell_constraints(self.vertical, cross);
                let placeable = self.scope.measure(self.measurables[child], constraints);
                self.placeables[child] = Some(placeable);
                placeable
            }
        };
        let main_size = if self.vertical { placeable.height } else { placeable.width };
        self.sizes.insert(item as usize, main_size);
        Some(MeasuredItem {
            index: item,
            lane: start,
            span: size,
            placeable,
            main_size,
            size_with_spacing: (main_size + self.spacing).max(0),
            visible: true,
            main: 0,
            cross: 0,
        })
    }

    /// `try_measure`, noting it lacked the item.
    fn measure(&mut self, item: i32, span: SpanRange) -> Result<MeasuredItem, Missing> {
        self.try_measure(item, span).ok_or_else(|| {
            let item = item as usize;
            self.lacked = Some(match self.lacked {
                Some((first, last)) => (first.min(item), last.max(item)),
                None => (item, item),
            });
            Missing
        })
    }

    /// `ensureIndicesInRange`.
    fn ensure_indices_in_range(&mut self, indices: &mut [i32]) {
        for lane in (0..indices.len()).rev() {
            while indices[lane] >= self.count
                || !self.lane_info.assigned_to_lane(indices[lane], lane as i32)
            {
                indices[lane] = self.lane_info.find_previous_item_index(indices[lane], lane as i32);
            }
            if indices[lane] >= 0 {
                let item = indices[lane];
                // A full line that's no more: back to the first lane it held.
                if !self.items.is_full_span(item as usize) {
                    let target = if self.lane_info.get_lane(item)
                        == LazyStaggeredGridLaneInfo::LANE_FULL_SPAN
                    {
                        let target = indices.iter().position(|it| *it == item).unwrap_or(lane);
                        for (other, index) in
                            indices.iter_mut().enumerate().take(lane + 1).skip(target + 1)
                        {
                            if *index == item {
                                *index =
                                    self.lane_info.find_previous_item_index(item, other as i32);
                            }
                        }
                        target
                    } else {
                        lane
                    };
                    self.lane_info.set_lane(item, target as i32);
                }
            }
        }
    }

    /// `misalignedStart`.
    fn misaligned_start(&self, indices: &[i32], offsets: &[i32], reference: usize) -> bool {
        let previous =
            |lane: usize| self.lane_info.find_previous_item_index(indices[lane], lane as i32);
        // Each lane has laid out all its items, but they don't start together.
        for lane in 0..indices.len() {
            if previous(lane) == LazyStaggeredGridLaneInfo::LANE_UNSET
                && offsets[lane] != offsets[reference]
            {
                return true;
            }
        }
        // Some lanes lack items, with no room left for them.
        for lane in 0..indices.len() {
            if previous(lane) != LazyStaggeredGridLaneInfo::LANE_UNSET
                && offsets[lane] >= offsets[reference]
            {
                return true;
            }
        }
        // The first item isn't in the first lane.
        let first = self.lane_info.get_lane(0);
        first != 0
            && first != LazyStaggeredGridLaneInfo::LANE_UNSET
            && first != LazyStaggeredGridLaneInfo::LANE_FULL_SPAN
    }

    /// The lanes fill up to the end, or this measurement stops at an item it lacks.
    #[allow(clippy::too_many_lines)]
    fn run(
        &mut self,
        initial_delta: i32,
        initial_indices: &[i32],
        initial_offsets: &[i32],
        can_restart: bool,
    ) -> Result<Measured, Missing> {
        let (lanes, count, spacing) = (self.lanes, self.count, self.spacing);
        let (before, after, available) = (self.before, self.after, self.available);
        // The real amount of scroll applied.
        let mut scroll_delta = initial_delta;
        let mut first_indices = initial_indices.to_vec();
        let mut first_offsets = initial_offsets.to_vec();
        self.ensure_indices_in_range(&mut first_indices);
        offset_by(&mut first_offsets, -scroll_delta);
        let mut arena: Vec<MeasuredItem> = Vec::new();
        let mut measured: Vec<VecDeque<usize>> = vec![VecDeque::new(); lanes];
        // The start padding counts, so items under it are composed.
        offset_by(&mut first_offsets, -before);
        let mut lane_to_check = -1i32;
        // Scrolled backward, or items under the padding: those before the first ones.
        while first_indices
            .iter()
            .zip(&first_offsets)
            .any(|(index, offset)| *offset < (-spacing).max(0) && *index > 0)
        {
            // The first item with something before it has the largest index.
            let lane = index_of_max_value(&first_indices);
            let item = first_indices[lane];
            for other in 0..lanes {
                if first_indices[other] != first_indices[lane]
                    && first_offsets[other] < first_offsets[lane]
                {
                    first_offsets[other] = first_offsets[lane];
                }
            }
            let previous = self.lane_info.find_previous_item_index(item, lane as i32);
            if previous < 0 {
                lane_to_check = lane as i32;
                break;
            }
            let span = self.span_range(previous, lane);
            let value = self.lane_value(span);
            self.lane_info.set_lane(previous, value);
            let measured_item = self.measure(previous, span)?;
            let offset = max_in_range(&first_offsets, span);
            let gaps = (span.1 != 1)
                .then(|| self.lane_info.get_gaps(previous).map(<[i32]>::to_vec))
                .flatten();
            for other in span.0..span.0 + span.1 {
                first_indices[other] = previous;
                let gap = gaps.as_ref().map_or(0, |gaps| gaps[other]);
                first_offsets[other] = offset + measured_item.size_with_spacing + gap;
            }
        }
        let min_offset = -before;
        // Scrolled backward past the first items: the rest of the scroll is left.
        if first_offsets[0] < min_offset {
            let not_consumed = min_offset - first_offsets[0];
            offset_by(&mut first_offsets, not_consumed);
            scroll_delta -= not_consumed;
        }
        offset_by(&mut first_offsets, before);
        let lane_to_check = if lane_to_check == -1 {
            first_indices.iter().position(|index| *index == 0).map_or(-1, |lane| lane as i32)
        } else {
            lane_to_check
        };
        // The lanes don't start together: from the top again, in lanes found anew.
        if lane_to_check != -1 {
            let lane = lane_to_check as usize;
            if self.misaligned_start(&first_indices, &first_offsets, lane) && can_restart {
                self.lane_info.reset();
                return self.run(
                    scroll_delta,
                    &vec![-1; lanes],
                    &vec![first_offsets[lane]; lanes],
                    false,
                );
            }
        }
        // Down from those, measuring those again (they're kept): lanes depend on what's before.
        let mut current_indices = first_indices.clone();
        let mut current_offsets: Vec<i32> = first_offsets.iter().map(|offset| -offset).collect();
        let min_visible = min_offset + spacing;
        let max_offset = (available + after).max(0);
        let mut initial_measured = 0;
        let mut initial_lane = index_of_min_value(&current_indices, i32::MIN);
        while initial_lane != -1 && initial_measured < lanes {
            let lane = initial_lane as usize;
            let item = current_indices[lane];
            initial_lane = index_of_min_value(&current_indices, item);
            initial_measured += 1;
            if item < 0 {
                continue;
            }
            let span = self.span_range(item, lane);
            let measured_item = self.measure(item, span)?;
            let value = self.lane_value(span);
            self.lane_info.set_lane(item, value);
            let offset = max_in_range(&current_offsets, span);
            let mut gaps = (span.1 != 1).then(|| {
                self.lane_info.get_gaps(item).map_or_else(|| vec![0; lanes], <[i32]>::to_vec)
            });
            let at = arena.len();
            for other in span.0..span.0 + span.1 {
                if let Some(gaps) = &mut gaps {
                    gaps[other] = offset - current_offsets[other];
                }
                current_offsets[other] = offset + measured_item.size_with_spacing;
                current_indices[other] = item;
                measured[other].push_back(at);
            }
            let hidden = offset < min_visible && current_offsets[span.0] <= min_visible;
            arena.push(measured_item);
            // Scrolled past: measured for what's after it, not placed.
            if hidden {
                arena[at].visible = false;
            }
            if span.1 != 1 {
                self.lane_info.set_gaps(item, gaps);
                // A full line takes the other lanes' slots too.
                initial_measured = lanes;
            }
        }
        // Forward until the viewport is full: at least one item, if the padding is larger.
        while current_offsets.iter().any(|offset| *offset < max_offset || *offset <= 0)
            || measured.iter().all(VecDeque::is_empty)
        {
            let lane = index_of_min_value(&current_offsets, i32::MIN).max(0) as usize;
            let item = current_indices.iter().copied().max().unwrap_or(-1) + 1;
            if item >= count {
                break;
            }
            let span = self.span_range(item, lane);
            let value = self.lane_value(span);
            self.lane_info.set_lane(item, value);
            let measured_item = match self.measure(item, span) {
                Ok(measured_item) => measured_item,
                Err(missing) => {
                    // As many as cover what's left in the lanes, for the next round.
                    let average = self.sizes.average().max(1) + spacing.max(0);
                    let room: i32 =
                        current_offsets.iter().map(|offset| (max_offset - *offset).max(0)).sum();
                    self.lacked_room = (room as usize).div_ceil(average as usize);
                    return Err(missing);
                }
            };
            let offset = max_in_range(&current_offsets, span);
            let mut gaps = (span.1 != 1).then(|| {
                self.lane_info.get_gaps(item).map_or_else(|| vec![0; lanes], <[i32]>::to_vec)
            });
            let at = arena.len();
            for other in span.0..span.0 + span.1 {
                if let Some(gaps) = &mut gaps {
                    gaps[other] = offset - current_offsets[other];
                }
                current_indices[other] = item;
                current_offsets[other] = offset + measured_item.size_with_spacing;
                measured[other].push_back(at);
            }
            self.lane_info.set_gaps(item, gaps);
            let hidden = offset < min_visible && current_offsets[span.0] <= min_visible;
            arena.push(measured_item);
            if hidden {
                arena[at].visible = false;
            }
        }
        // Those measured that are off screen leave their lanes.
        for lane in 0..lanes {
            while measured[lane].len() > 1 && !arena[measured[lane][0]].visible {
                let at = measured[lane].pop_front().expect("two or more");
                let item = &arena[at];
                let gap = if item.span != 1 {
                    self.lane_info.get_gaps(item.index).map_or(0, |gaps| gaps[lane])
                } else {
                    0
                };
                first_offsets[lane] -= item.size_with_spacing + gap;
            }
            first_indices[lane] = measured[lane].front().map_or(-1, |at| arena[*at].index);
        }
        // No spacing after the last item.
        if current_indices.contains(&(count - 1)) {
            offset_by(&mut current_offsets, -spacing);
        }
        // Not filled from there: scrolled back over those before, if any.
        if current_offsets.iter().all(|offset| *offset < available) {
            let lane = index_of_max_value(&current_offsets);
            let to_scroll_back = available - current_offsets[lane];
            offset_by(&mut first_offsets, -to_scroll_back);
            offset_by(&mut current_offsets, to_scroll_back);
            let mut gap_detected = false;
            while first_offsets.iter().any(|offset| *offset < before) {
                // The least offset, so what's distributed stays: not the largest index as above.
                let mut lane = index_of_min_value(&first_offsets, i32::MIN).max(0) as usize;
                let next_lane = index_of_max_value(&first_indices);
                if lane != next_lane {
                    if first_offsets[lane] == first_offsets[next_lane] {
                        lane = next_lane;
                    } else {
                        gap_detected = true;
                    }
                }
                let current = if first_indices[lane] == -1 { count } else { first_indices[lane] };
                let previous = self.lane_info.find_previous_item_index(current, lane as i32);
                if previous < 0 {
                    if (gap_detected || self.misaligned_start(&first_indices, &first_offsets, lane))
                        && can_restart
                    {
                        self.lane_info.reset();
                        return self.run(
                            scroll_delta,
                            &vec![-1; lanes],
                            &vec![first_offsets[lane]; lanes],
                            false,
                        );
                    }
                    break;
                }
                let span = self.span_range(previous, lane);
                let value = self.lane_value(span);
                self.lane_info.set_lane(previous, value);
                let measured_item = self.measure(previous, span)?;
                let offset = max_in_range(&first_offsets, span);
                let gaps = (span.1 != 1)
                    .then(|| self.lane_info.get_gaps(previous).map(<[i32]>::to_vec))
                    .flatten();
                let size = measured_item.size_with_spacing;
                let at = arena.len();
                arena.push(measured_item);
                for other in span.0..span.0 + span.1 {
                    if first_offsets[other] != offset {
                        gap_detected = true;
                    }
                    measured[other].push_front(at);
                    first_indices[other] = previous;
                    let gap = gaps.as_ref().map_or(0, |gaps| gaps[other]);
                    first_offsets[other] = offset + size + gap;
                }
            }
            // Lanes found offset: from here again, the lanes given out again going down.
            if gap_detected && can_restart {
                self.lane_info.reset();
                return self.run(scroll_delta, &first_indices, &first_offsets, false);
            }
            scroll_delta += to_scroll_back;
            let lane = index_of_min_value(&first_offsets, i32::MIN).max(0) as usize;
            if first_offsets[lane] < 0 {
                let value = first_offsets[lane];
                scroll_delta += value;
                offset_by(&mut current_offsets, value);
                offset_by(&mut first_offsets, -value);
            }
        }
        // Less than asked at an end, more if items changed: what it asked for to be consumed, else
        // all that was, as Compose's `consumedScroll`.
        let to_consume = -self.pending;
        let rounded = kotlin_round(to_consume);
        let consumed =
            if rounded.signum() == scroll_delta.signum() && rounded.abs() >= scroll_delta.abs() {
                scroll_delta as f32
            } else {
                to_consume
            };
        let scroll_offsets: Vec<i32> = first_offsets.iter().map(|offset| -offset).collect();
        // Items wholly under the padding don't count for the position.
        if before > spacing {
            for lane in 0..lanes {
                let in_lane = &measured[lane];
                for position in 0..in_lane.len() {
                    let item = &arena[in_lane[position]];
                    let gap = self.lane_info.get_gaps(item.index).map_or(0, |gaps| gaps[lane]);
                    let size = item.size_with_spacing + gap;
                    if position + 1 != in_lane.len()
                        && first_offsets[lane] != 0
                        && first_offsets[lane] >= size
                    {
                        first_offsets[lane] -= size;
                        first_indices[lane] = arena[in_lane[position + 1]].index;
                    } else {
                        break;
                    }
                }
            }
        }
        Ok(Measured {
            first_indices,
            first_offsets,
            current_indices,
            current_offsets,
            scroll_offsets,
            arena,
            lanes_of_items: measured,
            consumed,
            min_offset,
            max_offset,
        })
    }
}

/// How many items back a layout looks for how far it can scroll: past them, it estimates.
const MOST_ROOM_STEPS: usize = 32;

/// `fastRoundToInt`.
fn kotlin_round(value: f32) -> i32 {
    (value + 0.5).floor() as i32
}

impl StaggeredLayout {
    /// The lanes for `room` pixels across, as `LazyStaggeredGridSlotCache`.
    fn slots(&self, room: i32, spacing: i32, density: Density) -> Rc<Slots> {
        let key = (room, spacing, density);
        let mut cached = self.slots.borrow_mut();
        if let Some((cached_key, slots)) = &*cached
            && *cached_key == key
        {
            return slots.clone();
        }
        let sizes = self.info.cells.sizes(room, spacing, density);
        let slots = Rc::new(Slots::new(sizes, &self.info.cross_arrangement, room, &density));
        *cached = Some((key, slots.clone()));
        slots
    }

    /// The items pinned and those a focus search asked for: laid out beyond those in view.
    fn extra_items(&self, first: i32, last: i32) -> Vec<i32> {
        let state = &self.list;
        let mut extra = Vec::new();
        for key in state.pinned.get() {
            if let Some(pinned) = self.info.items.index_of(key, state.pinned_near.get()) {
                extra.push(pinned as i32);
            }
        }
        let (before, after) = state.beyond_bounds.get();
        extra.extend((first - before as i32).max(0)..first);
        extra.extend((last + 1)..(last + 1 + after as i32).min(self.info.items.count as i32));
        extra.sort_unstable();
        extra.dedup();
        extra
    }
}

impl MeasurePolicy for StaggeredLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let state = &self.list;
        // Observed: a scroll measures the grid again.
        state.remeasure.get();
        let info = &self.info;
        let (vertical, reverse) = (info.vertical, info.reverse);
        let density = scope.density();
        let node = scope.node_id();
        state.rtl.set(scope.layout_direction() == compose_ui::LayoutDirection::Rtl);
        state.density.set(density.density);
        let px = |dp: Dp| density.round_to_px(dp);
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
        let (main_max, cross_max) = if vertical {
            (constraints.max_height, constraints.max_width)
        } else {
            (constraints.max_width, constraints.max_height)
        };
        assert!(
            main_max != Constraints::INFINITY,
            "compose-foundation: a lazy staggered grid was measured with an infinite size along \
             its axis (in a scrolling parent?): give it a size"
        );
        assert!(
            cross_max != Constraints::INFINITY,
            "{}'s {} should be bound by parent.",
            if vertical { "LazyVerticalStaggeredGrid" } else { "LazyHorizontalStaggeredGrid" },
            if vertical { "width" } else { "height" },
        );
        let spacing = px(info.main_spacing);
        let cross_spacing = info.cross_arrangement.spacing(&density);
        let cross_room = cross_max - cross_before - cross_after;
        let slots = self.slots(cross_room, cross_spacing, density);
        let lanes = slots.count();
        let available = main_max - before - after;
        // As large as its padding at least.
        let (horizontal_padding, vertical_padding) = if vertical {
            (cross_before + cross_after, before + after)
        } else {
            (before + after, cross_before + cross_after)
        };
        let min_width = constraints.constrain_width(horizontal_padding);
        let min_height = constraints.constrain_height(vertical_padding);
        let count = info.items.count;
        let children = state.children.borrow().clone();
        let (composed_start, composed_end) = composed_range(&children, count);
        let viewport = |size: IntSize| LaidViewport {
            start: -before,
            end: available + after,
            count,
            size,
            vertical,
            reverse,
            before,
            after,
            spacing,
            max_span: lanes,
        };
        if lanes > 0 {
            let hint = super::grid::cell_constraints(vertical, slots.sizes[0]);
            if state.item_constraints_written.replace(Some(hint)) != Some(hint) {
                state.item_constraints.set(Some(hint));
            }
        }
        if count == 0 || lanes == 0 {
            let size = IntSize::new(min_width, min_height);
            state.room_backward.set(0.0);
            state.room_forward.set(0.0);
            state.publish_can_scroll();
            state.room_estimated.set((false, false));
            state.beyond_more.set((false, false));
            state.laid_out.begin();
            let keys_changed = state.note_laid_items(&info.items);
            state.laid_out.end(viewport(size), keys_changed);
            // Its lanes as asked keep waiting for items.
            state.pending.set(0.0);
            return scope.layout(size.width, size.height);
        }
        state.had_items.set(true);
        let Some(staggered) = self.state.inner() else {
            return scope.layout(min_width, min_height);
        };
        let mut lane_info = staggered.lane_info.borrow_mut();
        let mut sizes = state.sizes.borrow_mut();
        // The position, after a first item that moved: `updateScrollPositionIfTheFirstItemWasMoved`.
        let (mut indices, offsets) = staggered.position.borrow().clone();
        if let Some(key) = state.first_key.get() {
            let last_known = indices.first().copied().unwrap_or(0).max(0) as usize;
            let found = if last_known < count && info.items.key(last_known) == key {
                Some(last_known)
            } else {
                info.items.index_of(key, last_known)
            };
            if let Some(found) = found
                && !indices.contains(&(found as i32))
            {
                drop(lane_info);
                let filled =
                    self.state.fill_indices(&staggered, state, found as i32, indices.len());
                lane_info = staggered.lane_info.borrow_mut();
                indices = filled.clone();
                staggered.position.borrow_mut().0 = filled;
            }
        }
        let initial_indices = if indices.len() == lanes {
            indices
        } else {
            // Resized, or the first time: the lanes as far as they're known.
            lane_info.reset();
            let mut new = vec![0; lanes];
            for lane in 0..lanes {
                new[lane] = if lane < indices.len() && indices[lane] != -1 {
                    indices[lane]
                } else if lane == 0 {
                    0
                } else {
                    new[..lane].iter().copied().max().unwrap_or(-1) + 1
                };
                lane_info.set_lane(new[lane], lane as i32);
            }
            new
        };
        let initial_offsets = if offsets.len() == lanes {
            offsets
        } else {
            let mut new = vec![0; lanes];
            for lane in 0..lanes {
                new[lane] = if lane < offsets.len() {
                    offsets[lane]
                } else if lane == 0 {
                    0
                } else {
                    new[lane - 1]
                };
            }
            new
        };
        let pending = state.pending.get();
        const MOST_DELTA: f32 = 1.0e9;
        let pending = if pending.is_nan() { 0.0 } else { pending.clamp(-MOST_DELTA, MOST_DELTA) };
        let unready = state.prefetch.layout_started();
        let mut context = Context {
            scope,
            measurables,
            children: &children,
            composed_start,
            placeables: vec![None; measurables.len()],
            lane_info: &mut lane_info,
            slots: &slots,
            items: &info.items,
            prefetch: &state.prefetch,
            unready,
            lacked_unready: false,
            sizes: &mut sizes,
            vertical,
            count: count as i32,
            lanes,
            spacing,
            before,
            after,
            available,
            pending,
            lacked: None,
            lacked_room: 0,
        };
        let c = &mut context;
        let result = c.run(kotlin_round(-pending), &initial_indices, &initial_offsets, true);
        let mut done = match result {
            Ok(done) => done,
            Err(Missing) => {
                // Composed in the frame's next round, which measures again.
                let (first, last) = c.lacked.expect("noted");
                let step = FIRST_STEP.max(2 * state.asked.get()).max(lanes).min(MOST_IN_A_ROUND);
                let more = c.lacked_room.max(1).min(step).min(MOST_MISSING);
                state.asked.set(more);
                let start = if first < composed_start || composed_start == composed_end {
                    first.saturating_sub(if c.lacked_room == 0 { step - 1 } else { 0 })
                } else {
                    composed_start
                };
                let end = composed_end.max(last + more).min(count);
                state.compose((start, end.max(last + 1)));
                let (width, height) = if vertical {
                    (cross_max, constraints.constrain_height(main_max))
                } else {
                    (constraints.constrain_width(main_max), cross_max)
                };
                return c.scope.layout(width, height);
            }
        };
        state.asked.set(0);
        let content_padding = before + after;
        let max_current = done.current_offsets.iter().copied().max().unwrap_or(0);
        let layout_main = if vertical {
            let min = Constraints::new(min_width, cross_max, min_height, main_max);
            min.constrain_height(max_current + content_padding)
        } else {
            let min = Constraints::new(min_width, main_max, min_height, cross_max);
            min.constrain_width(max_current + content_padding)
        };
        let main_layout_size = layout_main.min(available) - before + after;
        // The items pinned, or a focus search's, around those in view.
        let first_item = first_visible_index(&done.first_indices);
        let last_visible = done
            .lanes_of_items
            .iter()
            .filter_map(|lane| lane.back())
            .map(|at| done.arena[*at].index)
            .max()
            .unwrap_or(first_item);
        let extra = self.extra_items(first_item, last_visible);
        let mut positioned: Vec<(Placeable, i32, i32)> = Vec::new();
        let mut scroll_offsets = done.scroll_offsets.clone();
        let mut extra_offset = scroll_offsets[0];
        let lanes_of_items = &done.lanes_of_items;
        let arena = &done.arena;
        let first_of = |lane: usize| lanes_of_items[lane].front().map_or(-1, |at| arena[*at].index);
        for &item in extra.iter().rev() {
            let lane = c.lane_info.get_lane(item);
            let before_first = if lane < 0 {
                (0..lanes).all(|lane| first_of(lane) > item)
            } else {
                first_of((lane as usize).min(lanes - 1)) > item
            };
            if before_first {
                let span = c.span_range(item, 0);
                if let Some(measured) = c.try_measure(item, span) {
                    extra_offset -= measured.size_with_spacing;
                    positioned.push((measured.placeable, extra_offset, 0));
                }
            }
        }
        // `calculateVisibleItems`.
        let mut queues = done.lanes_of_items.clone();
        let mut visible: Vec<usize> = Vec::with_capacity(queues.iter().map(VecDeque::len).sum());
        loop {
            let next = (0..lanes)
                .filter_map(|lane| queues[lane].front().map(|at| (done.arena[*at].index, lane)))
                .min();
            let Some((_, lane)) = next else { break };
            let at = queues[lane].pop_front().expect("found");
            let item = &done.arena[at];
            if item.lane != lane {
                continue;
            }
            let span = (item.lane, item.span);
            let main = max_in_range(&scroll_offsets, span);
            let (size, with_spacing) = (item.main_size, item.size_with_spacing);
            if main + size >= done.min_offset && main <= done.max_offset {
                let cross = slots.positions[lane];
                let item = &mut done.arena[at];
                item.main = main;
                item.cross = cross;
                visible.push(at);
            }
            scroll_offsets[span.0..span.0 + span.1].fill(main + with_spacing);
        }
        let mut extra_offset = done.scroll_offsets[0];
        // The items placed: those before in the items' order.
        positioned.reverse();
        let current_indices = &done.current_indices;
        let mut after_visible: Vec<(Placeable, i32, i32)> = Vec::new();
        for &item in &extra {
            if item >= count as i32 {
                continue;
            }
            let lane = c.lane_info.get_lane(item);
            let after_current = if lane < 0 {
                current_indices.iter().all(|index| *index < item)
            } else {
                current_indices[(lane as usize).min(lanes - 1)] < item
            };
            if after_current {
                let span = c.span_range(item, 0);
                if let Some(measured) = c.try_measure(item, span) {
                    after_visible.push((measured.placeable, extra_offset, 0));
                    extra_offset += measured.size_with_spacing;
                }
            }
        }
        for at in &visible {
            let item = &done.arena[*at];
            positioned.push((item.placeable, item.main, item.cross));
        }
        positioned.extend(after_visible);
        // What stays composed: those measured, whose lanes a next measurement starts from.
        let least =
            done.first_indices.iter().copied().filter(|index| *index >= 0).min().unwrap_or(0);
        let most = done.current_indices.iter().copied().max().unwrap_or(least);
        let (beyond_before, beyond_after) = state.beyond_bounds.get();
        let wanted = (
            (least as usize).saturating_sub(beyond_before),
            ((most as usize) + 1 + beyond_after).min(count),
        );
        let optimizations = c.scope.optimizations();
        let prefetch = &state.prefetch;
        prefetch.policy.set(Policy::of(optimizations));
        prefetch.vertical.set(vertical);
        prefetch.visible.set(visible.len());
        let beyond_ended = state.beyond_ended.take();
        if beyond_ended {
            prefetch.search_ended();
        }
        let next = if prefetch.forward.get() { wanted.1 + 1 } else { wanted.0.saturating_sub(1) };
        let limits = prefetch.target(
            wanted,
            (composed_start, composed_end),
            None,
            (!beyond_ended).then_some(next),
        );
        let covers = composed_start <= wanted.0 && composed_end >= wanted.1;
        let past_limits = composed_start < limits.0 || composed_end > limits.1;
        let search_ended = beyond_ended && past_limits;
        if search_ended {
            state.dispose_leaving.set(true);
        }
        let mut composed = (composed_start, composed_end);
        let composed_more = state.composed_more.take();
        if !covers || search_ended || c.lacked_unready || prefetch.take_cancelled() {
            composed = limits;
            state.compose(composed);
        } else if composed_more {
            state.composed.set(composed);
        }
        prefetch.node.set(Some(node));
        prefetch.count.set(count);
        prefetch.wanted.set(wanted);
        prefetch.limits.set(limits);
        prefetch.trim_between_frames(state, self.scheduler.as_ref(), composed);
        // `applyMeasureResult`: the position, what's left of the scroll and how far it can go.
        let first_offset = first_visible_offset(&done.first_indices, &done.first_offsets);
        let first_key = visible
            .iter()
            .map(|at| &done.arena[*at])
            .find(|item| item.index == first_item)
            .map(|item| info.items.key(item.index as usize));
        state.first_key.set(first_key);
        state.position.set((first_item as usize, first_offset as f32));
        *staggered.position.borrow_mut() = (done.first_indices.clone(), done.first_offsets.clone());
        staggered.jumped.set(false);
        let rounded = (first_item as usize, first_offset);
        if state.published.get() != rounded {
            state.published.set(rounded);
            state.first_index.set(rounded.0);
            state.first_offset.set(rounded.1);
        }
        let left = (-pending) - done.consumed;
        let at_end = left.abs() > 0.5;
        state.pending.set(if at_end { 0.0 } else { -left });
        let mut post_scroll: Option<Box<dyn FnOnce()>> = None;
        if let Some(overshoot) = state.overshoot.borrow_mut().take() {
            let moved = if at_end { overshoot.asked + left } else { overshoot.asked };
            post_scroll = overshoot.rest(moved);
        }
        let pending = state.pending.get();
        let average = if visible.is_empty() {
            0
        } else {
            visible.iter().map(|at| done.arena[*at].main_size).sum::<i32>() / visible.len() as i32
                + spacing
        };
        let reaches_end = done.current_indices.contains(&(count as i32 - 1));
        let forward_room = (max_current - available) as f32;
        state.room_forward.set(if reaches_end {
            if forward_room > 0.0 { forward_room - pending } else { 0.0 }
        } else {
            let remaining = (count as i32 - 1 - most).max(1) as f32;
            (forward_room.max(0.0) + average.max(1) as f32 * remaining / lanes as f32 - pending)
                .max(1.0)
        });
        // Exact if what's before the first lane's first item is known: its lane and sizes.
        let known_room = {
            let (mut item, mut room) = (done.first_indices[0], done.first_offsets[0]);
            for _ in 0..MOST_ROOM_STEPS {
                if item <= 0 {
                    break;
                }
                let previous = c.lane_info.find_previous_item_index(item, 0);
                if previous < 0
                    || c.lane_info.get_lane(previous) == LazyStaggeredGridLaneInfo::LANE_UNSET
                {
                    break;
                }
                let Some(size) = c.sizes.get(previous as usize) else { break };
                let gap = c.lane_info.get_gaps(previous).map_or(0, |gaps| gaps[0]);
                room += (size + spacing).max(0) + gap;
                item = previous;
            }
            (item == 0).then_some(room)
        };
        let reaches_start = known_room.is_some();
        state.room_backward.set(if let Some(room) = known_room {
            if room > 0 { room as f32 + pending } else { 0.0 }
        } else {
            let first_room = done.first_offsets[0] as f32;
            let remaining = least.max(1) as f32;
            (first_room.max(0.0) + average.max(1) as f32 * remaining / lanes as f32 + pending)
                .max(1.0)
        });
        state.room_estimated.set((!reaches_start, !reaches_end));
        state.publish_can_scroll();
        state.beyond_more.set((wanted.0 > 0, wanted.1 < count));
        let per_item = average as f32 / lanes as f32;
        let layout_info = (count, per_item, spacing, main_max, before + after);
        if state.layout_info_written.replace(layout_info) != layout_info {
            state.layout_info.set(layout_info);
        }
        if c.sizes.len() > 4 * MOST_MISSING * lanes {
            let near = MOST_MISSING * lanes;
            c.sizes.retain(|item| item.abs_diff(first_item as usize) <= near);
        }
        let (width, height) =
            if vertical { (cross_max, layout_main) } else { (layout_main, cross_max) };
        // `MeasuredItem.place`.
        for (placeable, main, cross) in &positioned {
            let size = if vertical { placeable.height } else { placeable.width };
            let main = if reverse { main_layout_size - main - size } else { *main } + before;
            let cross = cross + cross_before;
            if vertical {
                c.scope.place_relative(placeable, cross, main);
            } else {
                c.scope.place_relative(placeable, main, cross);
            }
        }
        let mut laid = state.laid_out.begin();
        for at in &visible {
            let item = &done.arena[*at];
            let (main_size, cross_size) = if vertical {
                (item.placeable.height, item.placeable.width)
            } else {
                (item.placeable.width, item.placeable.height)
            };
            laid.push(LaidItem {
                index: item.index as usize,
                main: item.main,
                cross: item.cross,
                main_size,
                cross_size,
                line: 0,
                lane: item.lane as i32,
                span: item.span,
                sticky: false,
            });
        }
        drop(laid);
        let keys_changed = state.note_laid_items(&info.items);
        state.laid_out.end(viewport(IntSize::new(width, height)), keys_changed);
        if let Some(post_scroll) = post_scroll {
            c.scope.run_after_layout(post_scroll);
        }
        c.scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "LazyStaggeredGrid"
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
