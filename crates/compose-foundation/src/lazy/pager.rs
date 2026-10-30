//! Pagers, as Compose's `foundation.pager`: `HorizontalPager` and `VerticalPager` showing pages of
//! one size, one at a time or several, moved by drags snapping to them and by `PagerState`.
//!
//! Compose's pager is a lazy layout with a measurement of its own. Here it's compose-rs's lazy list
//! (D1467): its items are the pages, each in a node giving it the page's size, and the list tells
//! the state of each measurement, where the state turns a page and a fraction into the list's first
//! item and offset, and the laid-out items back into a current page. The offset a page's snap
//! position takes is the list's zero: `currentAbsoluteScrollOffset` less the snap position, which
//! is why the bounds of Compose's scroll (`calculateNewMinScrollOffset` and
//! `calculateNewMaxScrollOffset`) are the list's own.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::task::{Poll, Waker};

use compose_animation::{
    AnimationSpec, DecayAnimationSpec, Spring, animate, spline_based_decay, spring_with,
};
use compose_runtime::{
    ComposeArg, CoroutineScope, DerivedState, ObjectHandle, Owned, OwnedState, composable,
    derived_state_of, mutable_state_of,
};
use compose_ui::{
    Alignment, Constraints, Density, Dp, Layout, Measurable, MeasurePolicy, MeasureResult,
    MeasureScope, Modifier, NestedScrollConnection, NestedScrollSource, Offset, PointerEventPass,
    PointerEventType,
};

use super::{
    LazyLayoutCacheWindow, LazyLayoutScrollScope, LazyList, LazyListLayoutInfo, LazyListScope,
    LazyListState,
};
use crate::gestures::{
    FinalSnappingItem, FlingBehavior, LocalBoxFuture, Orientation, ScrollBlock, ScrollScope,
    ScrollableState, ScrollableStateExt, SnapLayoutInfoProvider, SnapPosition,
    TargetedFlingBehavior, distance_to_snap_position, final_snapping_item, snap_fling_behavior,
};
use crate::layout::{Arrangement, PaddingValues};
use crate::{MutatePriority, MutationInterrupted, OverscrollEffect};

/// As `DefaultPositionThreshold`: how far a page moves before a slow drag targets the next.
const DEFAULT_POSITION_THRESHOLD: Dp = Dp(56.0);
/// As `MaxPagesForAnimateScroll`: further than this an animated scroll jumps closer first.
const MAX_PAGES_FOR_ANIMATE_SCROLL: i64 = 3;

/// How large a page is, as Compose's `PageSize`.
#[derive(Clone)]
pub enum PageSize {
    /// As large as the room between the paddings, as `Fill`.
    Fill,
    /// Of this size, as `Fixed`.
    Fixed(Dp),
    /// From the room and the spacing, as an implementation of the interface.
    Custom(Rc<dyn Fn(Density, i32, i32) -> i32>),
}

impl PageSize {
    /// As `calculateMainAxisPageSize`.
    pub fn calculate_main_axis_page_size(
        &self,
        density: Density,
        available_space: i32,
        page_spacing: i32,
    ) -> i32 {
        match self {
            PageSize::Fill => available_space,
            PageSize::Fixed(size) => density.round_to_px(*size),
            PageSize::Custom(size) => size(density, available_space, page_spacing),
        }
    }
}

/// How far a fling may take a pager, as Compose's `PagerSnapDistance`.
pub trait PagerSnapDistance {
    /// The page to land on from `start_page`, given the one the decay suggests, as
    /// `calculateTargetPage`.
    fn calculate_target_page(
        &self,
        start_page: usize,
        suggested_target_page: usize,
        velocity: f32,
        page_size: i32,
        page_spacing: i32,
    ) -> usize;
}

struct PagerSnapDistanceMaxPages(usize);

impl PagerSnapDistance for PagerSnapDistanceMaxPages {
    fn calculate_target_page(
        &self,
        start_page: usize,
        suggested_target_page: usize,
        _velocity: f32,
        _page_size: i32,
        _page_spacing: i32,
    ) -> usize {
        suggested_target_page
            .clamp(start_page.saturating_sub(self.0), start_page.saturating_add(self.0))
    }
}

/// At most `pages` away, as `PagerSnapDistance.atMost`.
pub fn pager_snap_distance_at_most(pages: usize) -> Rc<dyn PagerSnapDistance> {
    Rc::new(PagerSnapDistanceMaxPages(pages))
}

/// What a page is told, as Compose's `PagerScope`, which holds nothing yet.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct PagerScope;

/// A page laid out, as `PageInfo`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PageInfo {
    pub index: usize,
    /// From the start of the content, its start padding left out, as `offset`.
    pub offset: i32,
    /// Its identity, as `key`: from `pager_key`'s hash, or from its index without one.
    pub key: u64,
}

/// What a pager laid out last, as `PagerLayoutInfo`.
#[derive(Clone)]
pub struct PagerLayoutInfo {
    pub visible_pages_info: Vec<PageInfo>,
    pub page_size: i32,
    pub page_spacing: i32,
    pub viewport_start_offset: i32,
    pub viewport_end_offset: i32,
    pub before_content_padding: i32,
    pub after_content_padding: i32,
    pub viewport_size: compose_ui::IntSize,
    pub orientation: Orientation,
    pub reverse_layout: bool,
    pub beyond_viewport_page_count: usize,
    pub snap_position: SnapPosition,
}

/// A state's value, read without subscribing.
fn peek<T: Clone + 'static>(state: &OwnedState<T>) -> T {
    compose_runtime::snapshots::Snapshot::without_read_observation(|| state.get())
}

struct PagerInner {
    list: Owned<LazyListState>,
    /// The page count: asked by what recomposes when `count_changes` does.
    page_count: RefCell<Rc<dyn Fn() -> usize>>,
    count_changes: OwnedState<u64>,
    current_page: OwnedState<usize>,
    current_fraction: OwnedState<f32>,
    /// A page and fraction asked, for the next measurement to turn into the list's position.
    requested: Cell<Option<(usize, f32)>>,
    page_size: OwnedState<i32>,
    page_spacing: Cell<i32>,
    /// What the last measurement worked with, to keep the page when it changes: the viewport, the
    /// paddings, the page's size and the spacing.
    laid: Cell<(i32, i32, i32, i32, i32)>,
    /// A new snap position or page size was given since: the page stays where the new one says.
    relayout: Cell<bool>,
    /// Laid out once, as `pagerLayoutInfoState !== EmptyLayoutInfo`, and what waits for it.
    had_layout: Cell<bool>,
    layout_waiters: RefCell<Vec<Waker>>,
    /// Of the scroll or fling going on: only the latest lets go of the programmatic target.
    scroll_generation: Cell<u64>,
    /// The current page's key, as `lastKnownCurrentPageKey`.
    current_key: Cell<Option<u64>>,
    snap_position: RefCell<SnapPosition>,
    size_of_page: RefCell<PageSize>,
    beyond_viewport: Cell<usize>,
    vertical: Cell<bool>,
    density: Cell<Density>,
    programmatic_target: OwnedState<i32>,
    settled_page: OwnedState<usize>,
    target_page: RefCell<Option<DerivedState<usize>>>,
    settled_derived: RefCell<Option<DerivedState<usize>>>,
    /// As `upDownDifference`: the last gesture, from its press to its release.
    up_down: Cell<Offset>,
    /// As `isGestureInProgress`.
    gesture: Cell<bool>,
}

/// A pager's state, as Compose's `PagerState`: the current page, the one it's going to and the one
/// it settled at, moved by drags and by `scroll_to_page` and `animate_scroll_to_page`.
///
/// A `Copy` handle (D240): [`new`](Self::new) returns its owner, and [`remember_pager_state`] keeps
/// that in the composition.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PagerState {
    object: ObjectHandle<PagerInner>,
}

impl ComposeArg for PagerState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl std::fmt::Debug for PagerState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("PagerState")
    }
}

struct Observer(PagerState);

impl super::ListObserver for Observer {
    fn before_measure(
        &self,
        list: &super::LazyListInner,
        items: &super::Items,
        density: Density,
        viewport: i32,
        before: i32,
        after: i32,
        spacing: i32,
    ) {
        if let Some(inner) = self.0.inner() {
            inner.before_measure(list, items, density, viewport, before, after, spacing);
        }
    }

    fn after_dispatch(&self, used: f32) {
        if let Some(inner) = self.0.inner() {
            inner.after_dispatch(used);
        }
    }

    fn after_measure(&self) {
        if let Some(inner) = self.0.inner() {
            inner.after_measure();
        }
    }

    fn collection_info(&self) -> compose_ui::CollectionInfo {
        let count = self.0.page_count() as i32;
        let vertical = self.0.inner().is_some_and(|inner| inner.vertical.get());
        if vertical {
            compose_ui::CollectionInfo { row_count: count, column_count: 1 }
        } else {
            compose_ui::CollectionInfo { row_count: 1, column_count: count }
        }
    }

    fn scroll_to_index(&self, index: usize) {
        let state = self.0;
        CoroutineScope::new().launch(async move {
            let _ = state.scroll_to_page(index, 0.0).await;
        });
    }
}

impl PagerInner {
    #[allow(clippy::too_many_arguments)]
    fn before_measure(
        &self,
        list: &super::LazyListInner,
        items: &super::Items,
        density: Density,
        viewport: i32,
        before: i32,
        after: i32,
        spacing: i32,
    ) {
        self.density.set(density);
        let available = (viewport - before - after).max(0);
        let size =
            self.size_of_page.borrow().calculate_main_axis_page_size(density, available, spacing);
        let size = size.max(0);
        self.page_spacing.set(spacing);
        if peek(&self.page_size) != size {
            self.page_size.set(size);
        }
        let laid = (viewport, before, after, size, spacing);
        let changed = self.laid.replace(laid) != laid || self.relayout.replace(false);
        // The position is the page's, whatever else goes on: not the first visible item's.
        list.first_key.set(None);
        // As `matchPageWithKey`: the current page's key may have moved.
        let moved = self.current_key.get().and_then(|key| {
            let current = peek(&self.current_page);
            let found = items.index_of(key, current)?;
            (found != current).then_some(found)
        });
        if let Some(found) = moved {
            self.current_page.set(found);
        }
        // A new size keeps the page and the fraction, as Compose's position is kept in those.
        let asked = self.requested.take().or_else(|| {
            (changed || moved.is_some())
                .then(|| (peek(&self.current_page), peek(&self.current_fraction)))
        });
        let Some((page, fraction)) = asked else { return };
        let with_spacing = (size + spacing).max(0);
        let count = (self.page_count.borrow())();
        let snap = self.snap_position.borrow().position(viewport, size, before, after, page, count);
        // As `currentPageOffset`, then as the measurement's first visible page.
        let mut offset = (snap as f32 - fraction * with_spacing as f32).round() as i32;
        let mut first = page;
        while first > 0 && offset > 0 && with_spacing > 0 {
            first -= 1;
            offset -= with_spacing;
        }
        list.jump_to(first, (-offset).max(0) as f32);
    }

    /// A scroll, in the page and fraction at once, as `performScroll`'s fraction: all pages are one
    /// size. The measurement corrects what this rounds differently.
    fn after_dispatch(&self, used: f32) {
        let with_spacing = peek(&self.page_size) + self.page_spacing.get();
        if with_spacing <= 0 || used == 0.0 {
            return;
        }
        let mut page = peek(&self.current_page) as i64;
        let mut fraction = peek(&self.current_fraction) + used / with_spacing as f32;
        while fraction > 0.5 {
            page += 1;
            fraction -= 1.0;
        }
        while fraction < -0.5 {
            page -= 1;
            fraction += 1.0;
        }
        let count = (self.page_count.borrow())() as i64;
        let page = page.clamp(0, (count - 1).max(0)) as usize;
        if peek(&self.current_page) != page {
            self.current_page.set(page);
            // The key known is another page's until the measurement.
            self.current_key.set(None);
        }
        if peek(&self.current_fraction) != fraction {
            self.current_fraction.set(fraction);
        }
    }

    /// The closest page to the snap position, and how far it is from it, as the measurement's end.
    fn after_measure(&self) {
        let list = *self.list;
        let info = list.layout_info_untracked();
        let size = peek(&self.page_size);
        let spacing = self.page_spacing.get();
        let with_spacing = size + spacing;
        let count = (self.page_count.borrow())();
        let snap_position = self.snap_position.borrow();
        let viewport = main_size(&info);
        self.had_layout.set(true);
        for waker in self.layout_waiters.borrow_mut().drain(..) {
            waker.wake();
        }
        // Pages still of the old size (their measurement saw it): the next pass, which this
        // size's write brings, tells where they are.
        if info.visible_items_info.iter().any(|page| page.size != size) {
            return;
        }
        let closest =
            info.visible_items_info
                .iter()
                .filter(|page| !page.non_scrollable)
                .map(|page| {
                    let distance = distance_to_snap_position(
                        viewport,
                        info.before_content_padding,
                        info.after_content_padding,
                        size,
                        page.offset,
                        page.index,
                        &snap_position,
                        count,
                    );
                    (page, distance.abs())
                })
                .fold(None, |best: Option<(&super::LazyListItemInfo, f32)>, (page, distance)| {
                    match best {
                        Some((_, least)) if least <= distance => best,
                        _ => Some((page, distance)),
                    }
                });
        if let Some((page, _)) = closest {
            let snap = snap_position.position(
                viewport,
                size,
                info.before_content_padding,
                info.after_content_padding,
                page.index,
                count,
            );
            let fraction = if with_spacing == 0 {
                0.0
            } else {
                ((snap - page.offset) as f32 / with_spacing as f32).clamp(-0.5, 0.5)
            };
            self.current_key.set(Some(page.key));
            // What's told at each scroll stays when it's this within a pixel: a second write per
            // frame for what reads it, otherwise.
            let told = peek(&self.current_fraction);
            let same_page = peek(&self.current_page) == page.index;
            let close = with_spacing != 0 && (told - fraction).abs() * with_spacing as f32 <= 1.0;
            if !same_page {
                self.current_page.set(page.index);
            }
            if told != fraction && !(same_page && close && fraction != 0.0) {
                self.current_fraction.set(fraction);
            }
        } else if count == 0 {
            self.current_key.set(None);
            if peek(&self.current_page) != 0 {
                self.current_page.set(0);
            }
            if peek(&self.current_fraction) != 0.0 {
                self.current_fraction.set(0.0);
            }
        }
        if !list.is_scroll_in_progress_untracked() {
            let current = peek(&self.current_page);
            if peek(&self.settled_page) != current {
                self.settled_page.set(current);
            }
        }
    }
}

fn main_size(info: &LazyListLayoutInfo) -> i32 {
    match info.orientation {
        Orientation::Vertical => info.viewport_size.height,
        Orientation::Horizontal => info.viewport_size.width,
    }
}

impl PagerState {
    /// At `current_page`, `current_page_offset_fraction` of a page past it (within ±0.5), of
    /// `page_count()` pages, as `PagerState(currentPage, currentPageOffsetFraction, pageCount)`.
    #[track_caller]
    pub fn new(
        current_page: usize,
        current_page_offset_fraction: f32,
        page_count: impl Fn() -> usize + 'static,
    ) -> Owned<PagerState> {
        assert!(
            (-0.5..=0.5).contains(&current_page_offset_fraction),
            "currentPageOffsetFraction {current_page_offset_fraction} is not within the range -0.5 \
             to 0.5"
        );
        let owned = Owned::new(
            PagerInner {
                list: LazyListState::new(current_page, 0),
                page_count: RefCell::new(Rc::new(page_count)),
                count_changes: mutable_state_of(0),
                current_page: mutable_state_of(current_page),
                current_fraction: mutable_state_of(current_page_offset_fraction),
                requested: Cell::new(Some((current_page, current_page_offset_fraction))),
                page_size: mutable_state_of(0),
                page_spacing: Cell::new(0),
                laid: Cell::new((-1, 0, 0, 0, 0)),
                relayout: Cell::new(false),
                had_layout: Cell::new(false),
                layout_waiters: RefCell::default(),
                scroll_generation: Cell::new(0),
                current_key: Cell::new(None),
                snap_position: RefCell::new(SnapPosition::Start),
                size_of_page: RefCell::new(PageSize::Fill),
                beyond_viewport: Cell::new(0),
                vertical: Cell::new(false),
                density: Cell::new(Density::new(1.0)),
                programmatic_target: mutable_state_of(-1),
                settled_page: mutable_state_of(current_page),
                target_page: RefCell::new(None),
                settled_derived: RefCell::new(None),
                up_down: Cell::new(Offset::ZERO),
                gesture: Cell::new(false),
            },
            |object| PagerState { object },
        );
        let state = *owned;
        if let Some(inner) = state.inner() {
            inner.list.set_observer(Rc::new(Observer(state)));
            // As `isGestureActionMatchesScroll`: a page bouncing back prefetches nothing.
            inner.list.set_scroll_allowed(Rc::new(move |delta| {
                let Some(inner) = state.inner() else { return true };
                let up_down = inner.up_down.get();
                if up_down.x as i32 == 0 && up_down.y as i32 == 0 {
                    return true;
                }
                let vertical =
                    inner.list.layout_info_untracked().orientation == Orientation::Vertical;
                let gesture = if vertical { up_down.y } else { up_down.x };
                delta.signum() == (-gesture).signum()
            }));
            *inner.target_page.borrow_mut() =
                Some(derived_state_of(move || state.compute_target()));
            *inner.settled_derived.borrow_mut() = Some(derived_state_of(move || {
                let Some(inner) = state.inner() else { return 0 };
                if inner.list.is_scroll_in_progress() {
                    inner.settled_page.get()
                } else {
                    inner.current_page.get()
                }
            }));
        }
        owned
    }

    #[track_caller]
    fn inner(&self) -> Option<Rc<PagerInner>> {
        self.object.get()
    }

    fn list(&self) -> Option<LazyListState> {
        self.inner().map(|inner| *inner.list)
    }

    /// How many pages it has, as `pageCount` (a state read when it's remembered).
    pub fn page_count(&self) -> usize {
        self.inner().map_or(0, |inner| {
            inner.count_changes.get();
            let count = inner.page_count.borrow().clone();
            count()
        })
    }

    /// The page closest to the snap position, as `currentPage` (a state read).
    pub fn current_page(&self) -> usize {
        self.inner().map_or(0, |inner| inner.current_page.get())
    }

    /// How far the current page is from the snap position, in pages, within ±0.5, as
    /// `currentPageOffsetFraction` (a state read).
    pub fn current_page_offset_fraction(&self) -> f32 {
        self.inner().map_or(0.0, |inner| inner.current_fraction.get())
    }

    /// Where a scroll is going, as `targetPage` (a state read).
    pub fn target_page(&self) -> usize {
        self.inner()
            .and_then(|inner| inner.target_page.borrow().as_ref().map(|it| it.get()))
            .unwrap_or(0)
    }

    /// The page it last rested on, as `settledPage` (a state read).
    pub fn settled_page(&self) -> usize {
        self.inner()
            .and_then(|inner| inner.settled_derived.borrow().as_ref().map(|it| it.get()))
            .unwrap_or(0)
    }

    fn coerce_in_page_range(&self, page: i64) -> usize {
        let count = self.page_count_untracked();
        if count > 0 { page.clamp(0, count as i64 - 1) as usize } else { 0 }
    }

    fn page_count_untracked(&self) -> usize {
        self.inner().map_or(0, |inner| {
            let count = inner.page_count.borrow().clone();
            count()
        })
    }

    fn compute_target(&self) -> usize {
        let Some(inner) = self.inner() else { return 0 };
        let list = *inner.list;
        let current = inner.current_page.get();
        let last_programmatic = inner.programmatic_target.get();
        let page = if !list.is_scroll_in_progress() {
            current as i64
        } else if last_programmatic != -1 {
            last_programmatic as i64
        } else if inner.current_fraction.get().abs() >= self.position_threshold_fraction() {
            let first = list.first_visible_item_index() as i64;
            if list.last_scrolled_forward() { first + 1 } else { first }
        } else {
            current as i64
        };
        self.coerce_in_page_range(page)
    }

    /// As `positionThresholdFraction`.
    fn position_threshold_fraction(&self) -> f32 {
        let Some(inner) = self.inner() else { return 0.0 };
        let size = inner.page_size.get() as f32;
        let threshold = inner.density.get().to_px(DEFAULT_POSITION_THRESHOLD).min(size / 2.0);
        if size == 0.0 { 0.0 } else { threshold / size }
    }

    /// How many pages away `page` is, as `getOffsetDistanceInPages`. Panics out of the pages.
    pub fn get_offset_distance_in_pages(&self, page: usize) -> f32 {
        assert!(
            page <= self.page_count(),
            "page {page} is not within the range 0 to {}",
            self.page_count()
        );
        page as f32 - self.current_page() as f32 - self.current_page_offset_fraction()
    }

    /// What it laid out last, as `layoutInfo` (a state read).
    pub fn layout_info(&self) -> PagerLayoutInfo {
        let Some(inner) = self.inner() else {
            return PagerLayoutInfo {
                visible_pages_info: Vec::new(),
                page_size: 0,
                page_spacing: 0,
                viewport_start_offset: 0,
                viewport_end_offset: 0,
                before_content_padding: 0,
                after_content_padding: 0,
                viewport_size: compose_ui::IntSize::new(0, 0),
                orientation: Orientation::Horizontal,
                reverse_layout: false,
                beyond_viewport_page_count: 0,
                snap_position: SnapPosition::Start,
            };
        };
        let info = inner.list.layout_info();
        PagerLayoutInfo {
            visible_pages_info: info
                .visible_items_info
                .iter()
                .map(|page| PageInfo { index: page.index, offset: page.offset, key: page.key })
                .collect(),
            page_size: inner.page_size.get(),
            page_spacing: info.main_axis_item_spacing,
            viewport_start_offset: info.viewport_start_offset,
            viewport_end_offset: info.viewport_end_offset,
            before_content_padding: info.before_content_padding,
            after_content_padding: info.after_content_padding,
            viewport_size: info.viewport_size,
            orientation: info.orientation,
            reverse_layout: info.reverse_layout,
            beyond_viewport_page_count: inner.beyond_viewport.get().min(self.page_count()),
            snap_position: inner.snap_position.borrow().clone(),
        }
    }

    /// The interaction source its drags report to, as `interactionSource`.
    pub fn interaction_source(&self) -> compose_ui::MutableInteractionSource {
        self.list().map(|list| list.interaction_source()).unwrap_or_default()
    }

    /// Goes to `page` at the next layout, from a scroll: as `snapToItem`.
    fn snap_to_page(&self, page: usize, fraction: f32) {
        let Some(inner) = self.inner() else { return };
        inner.requested.set(Some((page, fraction)));
        inner.current_page.set(page);
        inner.current_fraction.set(fraction);
        inner.current_key.set(None);
        inner.list.clear_scroll_position_key();
        // What was scrolled up to here is over; a scroll after this counts from here.
        inner.list.clear_pending();
        inner.list.force_remeasure();
    }

    /// Jumps to `page`, `page_offset_fraction` of a page past it, in a scroll, as `scrollToPage`.
    pub async fn scroll_to_page(
        &self,
        page: usize,
        page_offset_fraction: f32,
    ) -> Result<(), MutationInterrupted> {
        assert!(
            (-0.5..=0.5).contains(&page_offset_fraction),
            "pageOffsetFraction {page_offset_fraction} is not within the range -0.5 to 0.5"
        );
        let state = *self;
        self.scroll(MutatePriority::Default, move |_| async move {
            let page = state.coerce_in_page_range(page as i64);
            state.snap_to_page(page, page_offset_fraction);
        })
        .await
    }

    /// In a scroll, the page it's at, as `ScrollScope.updateCurrentPage`.
    pub fn update_current_page(&self, page: usize, page_offset_fraction: f32) {
        self.snap_to_page(page, page_offset_fraction);
    }

    /// In a scroll, the page it's going to, as `ScrollScope.updateTargetPage`.
    pub fn update_target_page(&self, target_page: usize) {
        if let Some(inner) = self.inner() {
            inner.programmatic_target.set(self.coerce_in_page_range(target_page as i64) as i32);
        }
    }

    /// Asks for `page` at the next layout, a scroll going on stopped, as `requestScrollToPage`.
    pub fn request_scroll_to_page(&self, page: usize, page_offset_fraction: f32) {
        let Some(list) = self.list() else { return };
        if list.is_scroll_in_progress_untracked() {
            CoroutineScope::new().launch(async move {
                let _ = list.stop_scroll(MutatePriority::Default).await;
            });
        }
        self.snap_to_page(page, page_offset_fraction);
    }

    /// Animates to `page` by a spring, jumping closer first if it's far, as `animateScrollToPage`.
    pub async fn animate_scroll_to_page(
        &self,
        page: usize,
        page_offset_fraction: f32,
    ) -> Result<(), MutationInterrupted> {
        self.animate_scroll_to_page_with(page, page_offset_fraction, compose_animation::spring())
            .await
    }

    /// [`animate_scroll_to_page`](Self::animate_scroll_to_page) along `animation_spec`.
    pub async fn animate_scroll_to_page_with(
        &self,
        page: usize,
        page_offset_fraction: f32,
        animation_spec: impl Into<AnimationSpec>,
    ) -> Result<(), MutationInterrupted> {
        let count = self.page_count_untracked();
        if (page == self.current_page()
            && self.current_page_offset_fraction() == page_offset_fraction)
            || count == 0
        {
            return Ok(());
        }
        assert!(
            (-0.5..=0.5).contains(&page_offset_fraction),
            "pageOffsetFraction {page_offset_fraction} is not within the range -0.5 to 0.5"
        );
        let target = self.coerce_in_page_range(page as i64);
        let spec = animation_spec.into();
        let state = *self;
        self.scroll(MutatePriority::Default, move |scope| async move {
            let Some(inner) = state.inner() else { return };
            state.update_target_page(target);
            let scope = state.lazy_layout_scroll_scope(scope);
            let (first, last) = (scope.first_visible_item_index(), scope.last_visible_item_index());
            let forward = target > first;
            let visible = last + 1 - first;
            if ((forward && target > last) || (!forward && target < first))
                && (target as i64 - first as i64).abs() >= MAX_PAGES_FOR_ANIMATE_SCROLL
            {
                let pre_jump = if forward {
                    target.saturating_sub(visible).max(first)
                } else {
                    (target + visible).min(first)
                };
                scope.snap_to_item(pre_jump, 0);
                compose_runtime::with_frame_nanos(|_| ()).await;
            }
            let size = peek(&inner.page_size) + inner.page_spacing.get();
            let to_snapped = page_offset_fraction * size as f32;
            let displacement =
                scope.calculate_distance_to(target, to_snapped.round() as i32) as f32;
            let previous = Cell::new(0.0f32);
            animate(0.0, displacement, 0.0, spec, |value, _| {
                let delta = value - previous.get();
                let used = scope.scroll_by(delta);
                previous.set(previous.get() + used);
            })
            .await;
        })
        .await
    }

    /// A lazy layout scroll scope over this pager and a scroll's `scroll_scope`, as
    /// `LazyLayoutScrollScope(state, scrollScope)`.
    pub fn lazy_layout_scroll_scope(
        &self,
        scroll_scope: ScrollScope,
    ) -> impl LazyLayoutScrollScope {
        PagerScrollScope { state: *self, scope: scroll_scope }
    }

    /// As `currentAbsoluteScrollOffset`.
    fn current_absolute_scroll_offset(&self) -> i64 {
        let Some(inner) = self.inner() else { return 0 };
        let with_spacing = (peek(&inner.page_size) + inner.page_spacing.get()) as i64;
        peek(&inner.current_page) as i64 * with_spacing
            + (peek(&inner.current_fraction) * with_spacing as f32).round() as i64
    }

    /// As `minScrollOffset` and `maxScrollOffset`: `calculateNewMinScrollOffset` and
    /// `calculateNewMaxScrollOffset`.
    fn scroll_bounds(&self) -> (i64, i64) {
        let Some(inner) = self.inner() else { return (0, 0) };
        let (layout, before, after, size, spacing) = inner.laid.get();
        let count = self.page_count_untracked();
        let snap = inner.snap_position.borrow();
        let position = |page: usize| {
            snap.position(layout, size, before, after, page, count).clamp(0, layout.max(0)) as i64
        };
        let most = (count as i64 * (size + spacing) as i64 + (before + after - spacing) as i64
            - (layout as i64 - position(count.saturating_sub(1))))
        .max(0);
        (position(0).min(most), most)
    }
    /// What drags and flings snap by: the pixels from the first page's start, as `firstVisiblePage`
    /// and its offset with what's scrolled since the layout.
    fn first_visible_page_now(&self) -> usize {
        let Some(inner) = self.inner() else { return 0 };
        let list = *inner.list;
        let size = (peek(&inner.page_size) + inner.page_spacing.get()).max(1) as f32;
        let (index, offset) = list.position_untracked();
        let pending = list.pending_scroll();
        (index as i64 + ((offset + pending) / size).floor() as i64).max(0) as usize
    }

    /// As `isScrollingForward`.
    fn is_scrolling_forward(&self, velocity: f32) -> bool {
        let Some(inner) = self.inner() else { return false };
        let info = inner.list.layout_info_untracked();
        let up_down = inner.up_down.get();
        let not_gesture = up_down.x as i32 == 0 && up_down.y as i32 == 0;
        let gesture =
            if info.orientation == Orientation::Horizontal { up_down.x } else { up_down.y };
        let forward = (if not_gesture { -velocity } else { gesture }) > 0.0;
        (forward && info.reverse_layout) || (!forward && !info.reverse_layout)
    }
}

impl ScrollableState for PagerState {
    fn scroll_indicator_state(&self) -> Option<std::rc::Rc<dyn crate::ScrollIndicatorState>> {
        Some(std::rc::Rc::new(*self))
    }

    fn scroll_boxed<'a>(
        &'a self,
        priority: MutatePriority,
        block: ScrollBlock<'a>,
    ) -> LocalBoxFuture<'a, Result<(), MutationInterrupted>> {
        Box::pin(async move {
            let Some(inner) = self.inner() else { return Ok(()) };
            let list = *inner.list;
            // As `awaitScrollDependencies`: its first layout tells it the page's size.
            std::future::poll_fn(|context| {
                if inner.had_layout.get() {
                    return Poll::Ready(());
                }
                let mut waiters = inner.layout_waiters.borrow_mut();
                if !waiters.iter().any(|waker| waker.will_wake(context.waker())) {
                    waiters.push(context.waker().clone());
                }
                Poll::Pending
            })
            .await;
            if !list.is_scroll_in_progress_untracked() {
                inner.settled_page.set(peek(&inner.current_page));
            }
            let mine = inner.scroll_generation.get().wrapping_add(1);
            inner.scroll_generation.set(mine);
            let result = list.scroll_owned(priority, block).await;
            // A scroll that took over has a target of its own.
            if inner.scroll_generation.get() == mine {
                inner.programmatic_target.set(-1);
            }
            result
        })
    }

    fn dispatch_raw_delta(&self, delta: f32) -> f32 {
        self.list().map_or(0.0, |list| list.dispatch_raw_delta(delta))
    }

    fn is_scroll_in_progress(&self) -> bool {
        self.list().is_some_and(|list| list.is_scroll_in_progress())
    }

    fn can_scroll_forward(&self) -> bool {
        self.list().is_some_and(|list| list.can_scroll_forward())
    }

    fn can_scroll_backward(&self) -> bool {
        self.list().is_some_and(|list| list.can_scroll_backward())
    }

    fn last_scrolled_forward(&self) -> bool {
        self.list().is_some_and(|list| list.last_scrolled_forward())
    }

    fn last_scrolled_backward(&self) -> bool {
        self.list().is_some_and(|list| list.last_scrolled_backward())
    }
}

/// A state of `page_count()` pages remembered at its call site and saved with the composition, as
/// `rememberPagerState`: the newest `page_count` counts.
#[composable(non_restartable)]
pub fn remember_pager_state(
    initial_page: usize,
    initial_page_offset_fraction: f32,
    page_count: impl Fn() -> usize + 'static,
) -> PagerState {
    let state = compose_runtime::remember_saveable_with(saver(), || {
        PagerState::new(initial_page, initial_page_offset_fraction, || 0)
    });
    if let Some(inner) = state.inner() {
        *inner.page_count.borrow_mut() = Rc::new(move || page_count());
        // Told once a composition: what reads the count sees this one's.
        let count = (inner.page_count.borrow())() as u64;
        if peek(&inner.count_changes) != count {
            inner.count_changes.set(count);
        }
    }
    state
}

/// What's kept of a remembered state, as `DefaultPagerState.Saver`: its page and fraction.
fn saver() -> impl compose_runtime::Saver<Owned<PagerState>> {
    use compose_runtime::Saveable;
    compose_runtime::saver(
        |state: &Owned<PagerState>| {
            let inner = state.inner()?;
            let page = peek(&inner.current_page) as i64;
            let fraction = peek(&inner.current_fraction);
            Some((page, fraction).save())
        },
        |saved| {
            <(i64, f32)>::restore(saved)
                .map(|(page, fraction)| PagerState::new(page as usize, fraction, || 0))
        },
    )
}

/// Snapping a pager's pages, as `SnapLayoutInfoProvider(pagerState, pagerSnapDistance, …)`.
struct PagerSnapProvider {
    state: PagerState,
    snap_distance: Rc<dyn PagerSnapDistance>,
    snap_positional_threshold: f32,
    rtl: bool,
}

impl PagerSnapProvider {
    /// The distances to the pages around, as `searchForSnappingBounds`.
    fn bounds(&self, velocity: f32) -> (f32, f32) {
        let Some(inner) = self.state.inner() else { return (0.0, 0.0) };
        let list = *inner.list;
        let info = list.layout_info_untracked();
        let pending = list.pending_scroll();
        let snap_position = inner.snap_position.borrow();
        let size = peek(&inner.page_size);
        let count = self.state.page_count_untracked();
        let (mut lower, mut upper) = (f32::NEG_INFINITY, f32::INFINITY);
        for page in &info.visible_items_info {
            let offset = distance_to_snap_position(
                main_size(&info),
                info.before_content_padding,
                info.after_content_padding,
                size,
                page.offset,
                page.index,
                &snap_position,
                count,
            ) - pending;
            if offset <= 0.0 && offset > lower {
                lower = offset;
            }
            if offset >= 0.0 && offset < upper {
                upper = offset;
            }
        }
        if lower == f32::NEG_INFINITY {
            lower = upper;
        }
        if upper == f32::INFINITY {
            upper = lower;
        }
        let forward = self.state.is_scrolling_forward(velocity);
        if !list.can_scroll_forward_untracked() {
            upper = 0.0;
            if forward {
                lower = 0.0;
            }
        }
        if !list.can_scroll_backward_untracked() {
            lower = 0.0;
            if !forward {
                upper = 0.0;
            }
        }
        (lower, upper)
    }

    /// As `calculateFinalSnappingBound`.
    fn final_bound(&self, velocity: f32, lower: f32, upper: f32) -> f32 {
        let Some(inner) = self.state.inner() else { return 0.0 };
        let info = inner.list.layout_info_untracked();
        let scrolling_forward = self.state.is_scrolling_forward(velocity);
        let is_forward = if info.orientation == Orientation::Vertical {
            scrolling_forward
        } else if self.rtl {
            !scrolling_forward
        } else {
            scrolling_forward
        };
        let size = peek(&inner.page_size);
        let up_down = inner.up_down.get();
        let gesture =
            if info.orientation == Orientation::Horizontal { up_down.x } else { up_down.y };
        let from_snapped = if size == 0 { 0.0 } else { gesture / size as f32 };
        let overflow = from_snapped - from_snapped.trunc();
        match final_snapping_item(velocity, inner.density.get()) {
            FinalSnappingItem::Closest => {
                if overflow.abs() > self.snap_positional_threshold {
                    if is_forward { upper } else { lower }
                } else if from_snapped.abs() >= self.state.position_threshold_fraction().abs() {
                    if is_forward { lower } else { upper }
                } else if lower.abs() < upper.abs() {
                    lower
                } else {
                    upper
                }
            }
            FinalSnappingItem::Next => upper,
            FinalSnappingItem::Previous => lower,
        }
    }
}

impl SnapLayoutInfoProvider for PagerSnapProvider {
    fn calculate_approach_offset(&self, velocity: f32, decay_offset: f32) -> f32 {
        let Some(inner) = self.state.inner() else { return 0.0 };
        let effective = peek(&inner.page_size) + inner.page_spacing.get();
        if effective == 0 {
            return 0.0;
        }
        let first = self.state.first_visible_page_now();
        let start = if velocity < 0.0 { first + 1 } else { first };
        let in_offset = (decay_offset / effective as f32) as i64;
        let count = self.state.page_count_untracked() as i64;
        let target = (start as i64 + in_offset).clamp(0, count) as usize;
        let corrected = self
            .snap_distance
            .calculate_target_page(
                start,
                target,
                velocity,
                peek(&inner.page_size),
                inner.page_spacing.get(),
            )
            .min(count as usize);
        let proposed = (corrected as i64 - start as i64) * effective as i64;
        let approach = (proposed.abs() - effective as i64).max(0);
        if approach == 0 { 0.0 } else { approach as f32 * velocity.signum() }
    }

    fn calculate_snap_offset(&self, velocity: f32) -> f32 {
        let (lower, upper) = self.bounds(velocity);
        let distance = self.final_bound(velocity, lower, upper);
        if distance.is_infinite() || distance.is_nan() { 0.0 } else { distance }
    }

    fn is_layout_behind(&self) -> bool {
        let Some(list) = self.state.list() else { return false };
        let pending = list.pending_scroll();
        if pending == 0.0 {
            return false;
        }
        let info = list.layout_info_untracked();
        let last = info.visible_items_info.last();
        let first = info.visible_items_info.first();
        match (first, last) {
            (Some(first), Some(last)) => {
                let viewport = main_size(&info);
                (pending > 0.0
                    && (last.offset + last.size) as f32 - pending < viewport as f32
                    && last.index + 1 < info.total_items_count)
                    || (pending < 0.0 && first.offset as f32 - pending > 0.0 && first.index > 0)
            }
            _ => false,
        }
    }
}

/// Pager's defaults, as Compose's `PagerDefaults`.
pub struct PagerDefaults;

impl PagerDefaults {
    /// As `BeyondViewportPageCount`.
    pub const BEYOND_VIEWPORT_PAGE_COUNT: usize = 0;

    /// As `flingBehavior(state, pagerSnapDistance, decayAnimationSpec, snapAnimationSpec,
    /// snapPositionalThreshold)`. Panics if the threshold isn't within 0 to 1.
    pub fn fling_behavior(
        state: PagerState,
        pager_snap_distance: Rc<dyn PagerSnapDistance>,
        decay_animation_spec: DecayAnimationSpec,
        snap_animation_spec: AnimationSpec,
        snap_positional_threshold: f32,
        rtl: bool,
    ) -> Rc<dyn TargetedFlingBehavior> {
        assert!(
            (0.0..=1.0).contains(&snap_positional_threshold),
            "snapPositionalThreshold should be a number between 0 and 1. You've specified \
             {snap_positional_threshold}"
        );
        Rc::new(snap_fling_behavior(
            Rc::new(PagerSnapProvider {
                state,
                snap_distance: pager_snap_distance,
                snap_positional_threshold,
                rtl,
            }),
            decay_animation_spec,
            snap_animation_spec,
        ))
    }

    /// The default's snap spring, as `flingBehavior`'s `snapAnimationSpec`.
    pub fn snap_animation_spec() -> AnimationSpec {
        spring_with(Spring::DAMPING_RATIO_NO_BOUNCY, Spring::STIFFNESS_MEDIUM_LOW).into()
    }

    /// As `pageNestedScrollConnection(state, orientation)`.
    pub fn page_nested_scroll_connection(
        state: PagerState,
        orientation: Orientation,
        rtl: bool,
    ) -> Rc<dyn NestedScrollConnection> {
        Rc::new(DefaultPagerNestedScrollConnection { state, orientation, rtl })
    }
}

/// The default fling, remembered, as `PagerDefaults.flingBehavior(state)`.
#[composable(non_restartable)]
pub fn remember_pager_fling_behavior(state: PagerState) -> Rc<dyn TargetedFlingBehavior> {
    let density = compose_ui::LOCAL_DENSITY.current();
    let rtl = compose_ui::LOCAL_LAYOUT_DIRECTION.current() == compose_ui::LayoutDirection::Rtl;
    compose_runtime::remember_keyed((state, density, rtl), move || {
        PagerDefaults::fling_behavior(
            state,
            pager_snap_distance_at_most(1),
            spline_based_decay(density),
            PagerDefaults::snap_animation_spec(),
            0.5,
            rtl,
        )
    })
}

struct DefaultPagerNestedScrollConnection {
    state: PagerState,
    orientation: Orientation,
    rtl: bool,
}

impl DefaultPagerNestedScrollConnection {
    fn along(&self, offset: Offset) -> f32 {
        if self.orientation == Orientation::Horizontal { offset.x } else { offset.y }
    }
}

impl NestedScrollConnection for DefaultPagerNestedScrollConnection {
    /// A page moved off its place takes what's dragged first, up to the next page.
    fn on_pre_scroll(&self, available: Offset, source: NestedScrollSource) -> Offset {
        let fraction = self.state.current_page_offset_fraction();
        let delta = self.along(available);
        if source != NestedScrollSource::UserInput || fraction.abs() <= 1e-6 || delta == 0.0 {
            return Offset::ZERO;
        }
        let Some(inner) = self.state.inner() else { return Offset::ZERO };
        let size = inner.page_size.get();
        let available_space = (size + inner.page_spacing.get()) as f32;
        let current = fraction * size as f32;
        let next = current + available_space * -fraction.signum();
        let (min, max) = if fraction > 0.0 { (next, current) } else { (current, next) };
        let coerced = delta.clamp(min, max);
        let reversed = inner.list.layout_info_untracked().reverse_layout;
        let consumed = if self.orientation == Orientation::Horizontal && (self.rtl != reversed) {
            self.state.dispatch_raw_delta(coerced)
        } else {
            -self.state.dispatch_raw_delta(-coerced)
        };
        if self.orientation == Orientation::Horizontal {
            Offset::new(consumed, available.y)
        } else {
            Offset::new(available.x, consumed)
        }
    }

    fn on_post_scroll(&self, _: Offset, available: Offset, source: NestedScrollSource) -> Offset {
        // A nested fling hitting its bound ends there, as Compose's cancellation.
        if source == NestedScrollSource::SideEffect && self.along(available) != 0.0 {
            crate::scroll::cancel_fling_scroll();
        }
        let gesture = self.state.inner().is_some_and(|inner| inner.gesture.get());
        if !gesture && self.along(available) != 0.0 {
            return match self.orientation {
                Orientation::Horizontal => Offset::new(available.x, 0.0),
                Orientation::Vertical => Offset::new(0.0, available.y),
            };
        }
        Offset::ZERO
    }

    fn on_post_fling(&self, _: Offset, available: Offset) -> LocalBoxFuture<'_, Offset> {
        let orientation = self.orientation;
        Box::pin(async move {
            match orientation {
                Orientation::Horizontal => Offset::new(available.x, 0.0),
                Orientation::Vertical => Offset::new(0.0, available.y),
            }
        })
    }
}

/// The fling behavior a pager gets, as `PagerWrapperFlingBehavior`: its target told as it goes, and
/// a rounding left corrected at its end.
struct PagerWrapperFlingBehavior {
    fling: Rc<dyn TargetedFlingBehavior>,
    state: PagerState,
}

/// Lets go of the target however the fling ends, unless a scroll took over: a drag has no target
/// of its own, an animation has.
struct ClearTarget(PagerState, u64);

impl Drop for ClearTarget {
    fn drop(&mut self) {
        if let Some(inner) = self.0.inner()
            && inner.scroll_generation.get() == self.1
        {
            inner.programmatic_target.set(-1);
        }
    }
}

impl FlingBehavior for PagerWrapperFlingBehavior {
    fn perform_fling<'a>(
        &'a self,
        scope: ScrollScope,
        initial_velocity: f32,
    ) -> LocalBoxFuture<'a, f32> {
        Box::pin(async move {
            let state = self.state;
            let generation = state.inner().map_or(0, |inner| {
                let mine = inner.scroll_generation.get().wrapping_add(1);
                inner.scroll_generation.set(mine);
                mine
            });
            let _clear = ClearTarget(state, generation);
            let on_remaining: Rc<dyn Fn(f32)> = Rc::new(move |remaining| {
                let Some(inner) = state.inner() else { return };
                let with_spacing = peek(&inner.page_size) + inner.page_spacing.get();
                let pages = if with_spacing != 0 { remaining / with_spacing as f32 } else { 0.0 };
                let target = pages.round() as i64 + peek(&inner.current_page) as i64;
                state.update_target_page(target.max(0) as usize);
            });
            let left =
                self.fling.perform_targeted_fling(scope, initial_velocity, on_remaining).await;
            let fraction = state.inner().map_or(0.0, |it| peek(&it.current_fraction));
            if fraction != 0.0 && fraction.abs() < 1e-3 {
                let page = state.inner().map_or(0, |it| peek(&it.current_page));
                state.request_scroll_to_page(page, 0.0);
            }
            left
        })
    }
}

/// A page, as `MeasuredPage`: its slot is the page's size along the axis; its children measured to
/// at most that, one after the other from its start (its end, reversed), each aligned across on its
/// own.
struct PageLayout {
    state: PagerState,
    vertical: bool,
    reverse: bool,
    alignment: Alignment,
}

impl MeasurePolicy for PageLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        // Observed: a new page size measures the page again.
        let size = self.state.inner().map_or(0, |inner| inner.page_size.get());
        let child = if self.vertical {
            Constraints::new(0, constraints.max_width, 0, size)
        } else {
            Constraints::new(0, size, 0, constraints.max_height)
        };
        let placeables: Vec<_> =
            measurables.iter().map(|measurable| scope.measure(*measurable, child)).collect();
        let cross = placeables
            .iter()
            .map(|it| if self.vertical { it.width } else { it.height })
            .max()
            .unwrap_or(0);
        let direction = scope.layout_direction();
        let mut main = 0;
        for placeable in &placeables {
            let along = if self.vertical { placeable.height } else { placeable.width };
            let at = if self.reverse { size - main - along } else { main };
            if self.vertical {
                let room = compose_ui::IntSize::new(cross, placeable.height);
                let x = self.alignment.align(placeable.size(), room, direction).x;
                scope.place(placeable, x, at);
            } else {
                let room = compose_ui::IntSize::new(placeable.width, cross);
                let y = self
                    .alignment
                    .align(placeable.size(), room, compose_ui::LayoutDirection::Ltr)
                    .y;
                scope.place_relative(placeable, at, y);
            }
            main += along;
        }
        if self.vertical { scope.layout(cross, size) } else { scope.layout(size, cross) }
    }

    fn debug_name(&self) -> &'static str {
        "Page"
    }
}

/// `LazyLayoutScrollScope(state, scrollScope)`.
struct PagerScrollScope {
    state: PagerState,
    scope: ScrollScope,
}

impl LazyLayoutScrollScope for PagerScrollScope {
    fn scroll_scope(&self) -> &ScrollScope {
        &self.scope
    }

    fn first_visible_item_index(&self) -> usize {
        self.state.first_visible_page_now()
    }

    fn first_visible_item_scroll_offset(&self) -> i32 {
        let Some(list) = self.state.list() else { return 0 };
        let Some(inner) = self.state.inner() else { return 0 };
        let with_spacing = (peek(&inner.page_size) + inner.page_spacing.get()).max(1) as f32;
        let (_, offset) = list.position_untracked();
        (offset + list.pending_scroll()).rem_euclid(with_spacing).round() as i32
    }

    fn last_visible_item_index(&self) -> usize {
        let Some(list) = self.state.list() else { return 0 };
        let info = list.layout_info_untracked();
        info.visible_items_info.last().map_or(0, |it| it.index)
    }

    fn item_count(&self) -> usize {
        self.state.page_count_untracked()
    }

    fn snap_to_item(&self, index: usize, offset: i32) {
        let Some(inner) = self.state.inner() else { return };
        let with_spacing = (peek(&inner.page_size) + inner.page_spacing.get()) as f32;
        let fraction = if with_spacing == 0.0 { 0.0 } else { offset as f32 / with_spacing };
        self.state.snap_to_page(index, fraction);
    }

    fn calculate_distance_to(&self, target_index: usize, target_offset: i32) -> i32 {
        let Some(inner) = self.state.inner() else { return 0 };
        let with_spacing = peek(&inner.page_size) + inner.page_spacing.get();
        let displacement = ((target_index as f32 - peek(&inner.current_page) as f32)
            * with_spacing as f32
            - peek(&inner.current_fraction) * with_spacing as f32
            + target_offset as f32)
            .round() as i64;
        let (min, max) = self.state.scroll_bounds();
        let current = self.state.current_absolute_scroll_offset();
        ((current + displacement).clamp(min, max) - current) as i32
    }
}

/// A pager's cache window, as `pagerCacheWindow`: a page ahead, none behind, none without a scroll.
struct PagerCacheWindow(PagerState);

impl LazyLayoutCacheWindow for PagerCacheWindow {
    fn is_non_scroll_caching_enabled(&self) -> bool {
        false
    }

    fn calculate_ahead_window(&self, _density: Density, _viewport: i32) -> i32 {
        self.0.inner().map_or(0, |inner| peek(&inner.page_size) + inner.page_spacing.get())
    }
}

/// What `HorizontalPager` and `VerticalPager` share, as `Pager`.
#[allow(clippy::too_many_arguments)]
#[composable(non_restartable)]
#[allow(non_snake_case)]
fn Pager(
    state: PagerState,
    modifier: Modifier,
    content_padding: PaddingValues,
    page_size: PageSize,
    beyond_viewport_page_count: usize,
    page_spacing: Dp,
    alignment: Alignment,
    fling_behavior: Option<Rc<dyn TargetedFlingBehavior>>,
    user_scroll_enabled: bool,
    reverse_layout: bool,
    key: Option<Rc<dyn Fn(usize) -> u64>>,
    page_nested_scroll_connection: Option<Rc<dyn NestedScrollConnection>>,
    snap_position: SnapPosition,
    overscroll_effect: Option<Rc<dyn OverscrollEffect>>,
    vertical: bool,
    page_content: compose_runtime::Callback<dyn Fn(PagerScope, usize)>,
) {
    let Some(inner) = state.inner() else { return };
    let list = *inner.list;
    // A new one lays the pages out again, with the page where it says.
    let size_same = match (&*inner.size_of_page.borrow(), &page_size) {
        (PageSize::Fill, PageSize::Fill) => true,
        (PageSize::Fixed(one), PageSize::Fixed(other)) => one == other,
        (PageSize::Custom(one), PageSize::Custom(other)) => Rc::ptr_eq(one, other),
        _ => false,
    };
    let snap_same = same_snap_position(&inner.snap_position.borrow(), &snap_position);
    *inner.size_of_page.borrow_mut() = page_size;
    *inner.snap_position.borrow_mut() = snap_position;
    if !size_same || !snap_same {
        inner.relayout.set(true);
        list.force_remeasure();
    }
    inner.vertical.set(vertical);
    inner.beyond_viewport.set(beyond_viewport_page_count);
    list.set_beyond_viewport(beyond_viewport_page_count.min(state.page_count()));
    let rtl = compose_ui::LOCAL_LAYOUT_DIRECTION.current() == compose_ui::LayoutDirection::Rtl;
    let orientation = if vertical { Orientation::Vertical } else { Orientation::Horizontal };
    let fling = fling_behavior.unwrap_or_else(|| remember_pager_fling_behavior(state));
    let wrapped: Rc<dyn FlingBehavior> =
        compose_runtime::remember_keyed((state, TargetedKey(fling.clone())), move || {
            Rc::new(PagerWrapperFlingBehavior { fling, state }) as Rc<dyn FlingBehavior>
        });
    let connection = page_nested_scroll_connection.unwrap_or_else(|| {
        compose_runtime::remember_keyed((state, vertical, rtl), move || {
            PagerDefaults::page_nested_scroll_connection(state, orientation, rtl)
        })
    });
    let scope = compose_runtime::remember_coroutine_scope();
    let semantics_modifier = pager_semantics(state, vertical, scope, user_scroll_enabled);
    let inner_modifier = drag_direction_detector(state).nested_scroll(connection);
    let window: Rc<dyn LazyLayoutCacheWindow> = compose_runtime::remember_keyed(state, move || {
        Rc::new(PagerCacheWindow(state)) as Rc<dyn LazyLayoutCacheWindow>
    });
    let count = state.page_count();
    let policy = compose_runtime::remember_keyed(
        (
            state,
            vertical,
            reverse_layout,
            alignment.horizontal_bias.to_bits(),
            alignment.vertical_bias.to_bits(),
        ),
        move || {
            Rc::new(PageLayout { state, vertical, reverse: reverse_layout, alignment })
                as Rc<dyn MeasurePolicy>
        },
    );
    LazyList(
        modifier.then(semantics_modifier),
        list,
        content_padding,
        vertical,
        reverse_layout,
        Arrangement::spaced_by(page_spacing).main_axis(),
        alignment,
        Some(wrapped),
        overscroll_effect,
        user_scroll_enabled,
        Some(window),
        inner_modifier,
        compose_runtime::callback(move |scope: LazyListScope| {
            let content = page_content.clone();
            let policy = policy.clone();
            let item = move |index: usize| {
                let content = content.clone();
                Layout(
                    Modifier,
                    policy.clone(),
                    compose_runtime::callback(move || content(PagerScope, index)),
                );
            };
            match &key {
                Some(key) => {
                    // Each page's key asked once, as `NearestRangeKeyIndexMap` keeps them.
                    let key = key.clone();
                    let known = RefCell::new(std::collections::HashMap::new());
                    let keyed = move |index: usize| {
                        *known.borrow_mut().entry(index).or_insert_with(|| key(index))
                    };
                    scope.items_keyed(count, keyed, item);
                }
                None => scope.items(count, item),
            }
        }),
    );
}

/// A targeted fling as a key, by identity.
#[derive(Clone)]
struct TargetedKey(Rc<dyn TargetedFlingBehavior>);

fn same_snap_position(one: &SnapPosition, other: &SnapPosition) -> bool {
    match (one, other) {
        (SnapPosition::Start, SnapPosition::Start)
        | (SnapPosition::Center, SnapPosition::Center)
        | (SnapPosition::End, SnapPosition::End) => true,
        (SnapPosition::Custom(one), SnapPosition::Custom(other)) => Rc::ptr_eq(one, other),
        _ => false,
    }
}

impl ComposeArg for TargetedKey {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// Paging actions for accessibility, as `pagerSemantics`.
fn pager_semantics(
    state: PagerState,
    vertical: bool,
    scope: CoroutineScope,
    user_scroll_enabled: bool,
) -> Modifier {
    if !user_scroll_enabled {
        return Modifier;
    }
    let forward = move || {
        if state.can_scroll_forward() {
            scope.launch(async move {
                if state.current_page() + 1 < state.page_count() {
                    let _ = state.animate_scroll_to_page(state.current_page() + 1, 0.0).await;
                }
            });
            true
        } else {
            false
        }
    };
    let backward = move || {
        if state.can_scroll_backward() {
            scope.launch(async move {
                if state.current_page() > 0 {
                    let _ = state.animate_scroll_to_page(state.current_page() - 1, 0.0).await;
                }
            });
            true
        } else {
            false
        }
    };
    Modifier.semantics(move |config| {
        if vertical {
            config.page_up(None, backward);
            config.page_down(None, forward);
        } else {
            config.page_left(None, backward);
            config.page_right(None, forward);
        }
    })
}

/// Notes a gesture and where it went, as `dragDirectionDetector`.
fn drag_direction_detector(state: PagerState) -> Modifier {
    Modifier.pointer_input(state, move |input| async move {
        loop {
            let down = input.await_pointer_event(PointerEventPass::Initial).await;
            if down.kind != PointerEventType::Press {
                continue;
            }
            /// Ends the gesture however it ends.
            struct InProgress(PagerState);
            impl Drop for InProgress {
                fn drop(&mut self) {
                    if let Some(inner) = self.0.inner() {
                        inner.gesture.set(false);
                    }
                }
            }
            if let Some(inner) = state.inner() {
                inner.gesture.set(true);
                inner.up_down.set(Offset::ZERO);
            }
            let _in_progress = InProgress(state);
            let start = down.position;
            let up = loop {
                let event = input.await_pointer_event(PointerEventPass::Initial).await;
                if event.kind == PointerEventType::Release && !input.any_pressed() {
                    break event;
                }
            };
            if let Some(inner) = state.inner() {
                inner.up_down.set(up.position - start);
            }
        }
    })
}

/// A key of a page from what `key` gives, for `key`.
pub fn pager_key<K: std::hash::Hash + 'static>(
    key: impl Fn(usize) -> K + 'static,
) -> Rc<dyn Fn(usize) -> u64> {
    Rc::new(move |index| {
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&key(index), &mut hasher);
        hasher.finish()
    })
}

/// Pages side by side, as Compose's `HorizontalPager`.
#[composable(non_restartable)]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn HorizontalPager(
    state: PagerState,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(PaddingValues::ZERO)] content_padding: PaddingValues,
    #[default(PageSize::Fill)] page_size: PageSize,
    #[default(0)] beyond_viewport_page_count: usize,
    #[default(Dp(0.0))] page_spacing: Dp,
    #[default(crate::Alignment::CENTER_VERTICALLY)]
    vertical_alignment: compose_ui::VerticalAlignment,
    #[default(None)] fling_behavior: Option<Rc<dyn TargetedFlingBehavior>>,
    #[default(true)] user_scroll_enabled: bool,
    #[default(false)] reverse_layout: bool,
    #[default(None)] key: Option<Rc<dyn Fn(usize) -> u64>>,
    #[default(None)] page_nested_scroll_connection: Option<Rc<dyn NestedScrollConnection>>,
    #[default(SnapPosition::Start)] snap_position: SnapPosition,
    #[default(crate::remember_overscroll_effect())] overscroll_effect: Option<
        Rc<dyn OverscrollEffect>,
    >,
    page_content: impl Fn(PagerScope, usize) + 'static,
) {
    Pager(
        state,
        modifier,
        content_padding,
        page_size,
        beyond_viewport_page_count,
        page_spacing,
        Alignment::new(-1.0, vertical_alignment.bias),
        fling_behavior,
        user_scroll_enabled,
        reverse_layout,
        key,
        page_nested_scroll_connection,
        snap_position,
        overscroll_effect,
        false,
        page_content,
    );
}

/// Pages one over the other, as Compose's `VerticalPager`.
#[composable(non_restartable)]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn VerticalPager(
    state: PagerState,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(PaddingValues::ZERO)] content_padding: PaddingValues,
    #[default(PageSize::Fill)] page_size: PageSize,
    #[default(0)] beyond_viewport_page_count: usize,
    #[default(Dp(0.0))] page_spacing: Dp,
    #[default(crate::Alignment::CENTER_HORIZONTALLY)]
    horizontal_alignment: compose_ui::HorizontalAlignment,
    #[default(None)] fling_behavior: Option<Rc<dyn TargetedFlingBehavior>>,
    #[default(true)] user_scroll_enabled: bool,
    #[default(false)] reverse_layout: bool,
    #[default(None)] key: Option<Rc<dyn Fn(usize) -> u64>>,
    #[default(None)] page_nested_scroll_connection: Option<Rc<dyn NestedScrollConnection>>,
    #[default(SnapPosition::Start)] snap_position: SnapPosition,
    #[default(crate::remember_overscroll_effect())] overscroll_effect: Option<
        Rc<dyn OverscrollEffect>,
    >,
    page_content: impl Fn(PagerScope, usize) + 'static,
) {
    Pager(
        state,
        modifier,
        content_padding,
        page_size,
        beyond_viewport_page_count,
        page_spacing,
        Alignment::new(horizontal_alignment.bias, -1.0),
        fling_behavior,
        user_scroll_enabled,
        reverse_layout,
        key,
        page_nested_scroll_connection,
        snap_position,
        overscroll_effect,
        true,
        page_content,
    );
}
