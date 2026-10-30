//! The cache window: what lazy layouts prefetch and keep beyond the viewport, as Compose's
//! `LazyLayoutCacheWindow` and `CacheWindowLogic` (its `MultiLaneCacheWindow`). Ahead of the scroll
//! the window fills with items prefetched one at a time; behind it, items that were visible stay
//! composed while the window has room for them. A list has one lane; so has a grid, whose units are
//! lines. D892.

use std::collections::HashMap;
use std::hash::BuildHasherDefault;
use std::rc::{Rc, Weak};

use compose_ui::{Density, Dp};

use super::prefetch::Handle;
use super::{LazyGridState, LazyListInner, LazyListState};

/// The area out of the viewport where a lazy layout keeps items, as Compose's
/// `LazyLayoutCacheWindow`: they're prepared ahead of the scroll, and stay behind it, to be there
/// when they're needed.
pub trait LazyLayoutCacheWindow {
    /// Whether the window fills without a scroll (the first layout, a dataset's change), as
    /// `isNonScrollCachingEnabled`. The default windows of lazy layouts say no.
    fn is_non_scroll_caching_enabled(&self) -> bool {
        true
    }

    /// The ahead window in pixels, for a viewport of `viewport` pixels along the axis, as
    /// `calculateAheadWindow`.
    fn calculate_ahead_window(&self, density: Density, viewport: i32) -> i32 {
        let _ = (density, viewport);
        0
    }

    /// The behind window in pixels, as `calculateBehindWindow`.
    fn calculate_behind_window(&self, density: Density, viewport: i32) -> i32 {
        let _ = (density, viewport);
        0
    }

    /// What this window is by value, for the windows made by the factories, which Compose compares
    /// with `equals`: a window equal to the last one given doesn't start over.
    #[doc(hidden)]
    fn value(&self) -> Option<WindowValue> {
        None
    }
}

/// A window by value (`LazyLayoutCacheWindow::value`).
#[doc(hidden)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum WindowValue {
    Dp {
        ahead: Dp,
        behind: Dp,
    },
    Fraction {
        ahead: f32,
        behind: f32,
    },
    /// The default window of a list or a grid's state, by its address.
    Default(usize),
}

struct DpCacheWindow {
    ahead: Dp,
    behind: Dp,
    non_scroll: bool,
}

impl LazyLayoutCacheWindow for DpCacheWindow {
    fn is_non_scroll_caching_enabled(&self) -> bool {
        self.non_scroll
    }

    fn calculate_ahead_window(&self, density: Density, _: i32) -> i32 {
        density.round_to_px(self.ahead)
    }

    fn calculate_behind_window(&self, density: Density, _: i32) -> i32 {
        density.round_to_px(self.behind)
    }

    fn value(&self) -> Option<WindowValue> {
        Some(WindowValue::Dp { ahead: self.ahead, behind: self.behind })
    }
}

struct FractionCacheWindow {
    ahead: f32,
    behind: f32,
    non_scroll: bool,
}

impl LazyLayoutCacheWindow for FractionCacheWindow {
    fn is_non_scroll_caching_enabled(&self) -> bool {
        self.non_scroll
    }

    fn calculate_ahead_window(&self, _: Density, viewport: i32) -> i32 {
        (viewport as f32 * self.ahead).round() as i32
    }

    fn calculate_behind_window(&self, _: Density, viewport: i32) -> i32 {
        (viewport as f32 * self.behind).round() as i32
    }

    fn value(&self) -> Option<WindowValue> {
        Some(WindowValue::Fraction { ahead: self.ahead, behind: self.behind })
    }
}

/// A window of `ahead` and `behind`, as `LazyLayoutCacheWindow(ahead, behind)`: it also fills
/// without a scroll.
pub fn lazy_layout_cache_window(ahead: Dp, behind: Dp) -> Rc<dyn LazyLayoutCacheWindow> {
    lazy_layout_cache_window_with(ahead, behind, true)
}

/// The same, as `LazyLayoutCacheWindow(ahead, behind, isNonScrollCachingEnabled)`.
pub fn lazy_layout_cache_window_with(
    ahead: Dp,
    behind: Dp,
    is_non_scroll_caching_enabled: bool,
) -> Rc<dyn LazyLayoutCacheWindow> {
    Rc::new(DpCacheWindow { ahead, behind, non_scroll: is_non_scroll_caching_enabled })
}

/// A window of fractions of the viewport, as `LazyLayoutCacheWindow(aheadFraction, behindFraction)`:
/// it also fills without a scroll.
pub fn lazy_layout_cache_window_fraction(
    ahead_fraction: f32,
    behind_fraction: f32,
) -> Rc<dyn LazyLayoutCacheWindow> {
    lazy_layout_cache_window_fraction_with(ahead_fraction, behind_fraction, true)
}

/// The same, as `LazyLayoutCacheWindow(aheadFraction, behindFraction, isNonScrollCachingEnabled)`.
pub fn lazy_layout_cache_window_fraction_with(
    ahead_fraction: f32,
    behind_fraction: f32,
    is_non_scroll_caching_enabled: bool,
) -> Rc<dyn LazyLayoutCacheWindow> {
    Rc::new(FractionCacheWindow {
        ahead: ahead_fraction,
        behind: behind_fraction,
        non_scroll: is_non_scroll_caching_enabled,
    })
}

/// A lazy layout's default window, as `DynamicLazyLayoutCacheWindow`: ahead, the items in view's
/// average size and spacing, between a tenth and a half of the viewport; nothing behind; no filling
/// without a scroll. compose-rs's own optimizations widen it (D893), for this window only.
pub(super) struct DefaultCacheWindow {
    pub(super) state: Weak<LazyListInner>,
}

impl LazyLayoutCacheWindow for DefaultCacheWindow {
    fn is_non_scroll_caching_enabled(&self) -> bool {
        self.state.upgrade().is_some_and(|state| state.prefetch.policy.get().at_rest)
    }

    fn calculate_ahead_window(&self, _: Density, viewport: i32) -> i32 {
        let Some(state) = self.state.upgrade() else { return 0 };
        let average = state.prefetch.visible_average();
        if average == 0 {
            return 0;
        }
        let window = average.clamp(viewport * 10 / 100, (viewport / 2).max(viewport * 10 / 100));
        if state.prefetch.policy.get().two_ahead { window + average } else { window }
    }

    fn calculate_behind_window(&self, _: Density, _: i32) -> i32 {
        let Some(state) = self.state.upgrade() else { return 0 };
        if state.prefetch.policy.get().keep_behind {
            state.prefetch.visible_average() * super::prefetch::KEPT_BEHIND
        } else {
            0
        }
    }

    fn value(&self) -> Option<WindowValue> {
        Some(WindowValue::Default(self.state.as_ptr() as usize))
    }
}

/// Compose's `LazyListDefaults`.
pub struct LazyListDefaults;

impl LazyListDefaults {
    /// The default window of `state`'s list, as `LazyListDefaults.cacheWindow(state)`.
    pub fn cache_window(state: LazyListState) -> Rc<dyn LazyLayoutCacheWindow> {
        Rc::new(DefaultCacheWindow {
            state: state.inner().map_or(Weak::new(), |it| Rc::downgrade(&it)),
        })
    }
}

/// Compose's `LazyGridDefaults`.
pub struct LazyGridDefaults;

impl LazyGridDefaults {
    /// The default window of `state`'s grid, as `LazyGridDefaults.cacheWindow(state)`.
    pub fn cache_window(state: LazyGridState) -> Rc<dyn LazyLayoutCacheWindow> {
        LazyListDefaults::cache_window(state.list)
    }
}

/// Whether two windows are the same one, as Compose's `remember(state, cacheWindow)` compares: by
/// value for the factories', else by identity.
pub(super) fn same_window(
    a: &Rc<dyn LazyLayoutCacheWindow>,
    b: &Rc<dyn LazyLayoutCacheWindow>,
) -> bool {
    Rc::ptr_eq(a, b) || a.value().is_some_and(|value| Some(value) == b.value())
}

const INVALID_INDEX: i32 = -1;
const UNSET_ITEM_COUNT: i32 = -1;
const INVALID_ITEM_SIZE: i32 = -1;

/// A lazy layout's state for the window's logic, as Compose's `CacheWindowScope`: indices are
/// units (a list's items; a grid's lines).
pub(super) trait CacheWindowScope {
    fn total_items_count(&self) -> i32;
    fn visible_items_count(&self) -> i32;
    fn has_visible_items(&self) -> bool;
    fn first_visible_item_index(&self) -> i32;
    fn density(&self) -> Density;
    fn last_visible_item_index(&self) -> i32;
    fn main_axis_viewport_size(&self) -> i32;
    fn update_per_lane_main_axis_extra_start_space(&self, spaces: &mut [i32]);
    fn update_per_lane_main_axis_extra_end_space(&self, spaces: &mut [i32]);
    fn update_per_lane_first_visible_item_index(&self, indices: &mut [i32]);
    fn update_per_lane_last_visible_item_indexes(&self, indices: &mut [i32]);
    /// Requests the unit's items; what's learned comes back through
    /// [`CacheWindowLogic::on_item_prefetched`].
    fn schedule_prefetch(&self, lane: i32, item_index: i32) -> Vec<Rc<Handle>>;
    fn visible_item_size(&self, index_in_visible_items: i32) -> i32;
    fn visible_item_index(&self, index_in_visible_items: i32) -> i32;
    /// The key, `None` for a grid's line with no item in view, Compose's `NoKey`.
    fn visible_item_key(&self, index_in_visible_items: i32) -> Option<u64>;
    fn visible_item_lane(&self, index_in_visible_items: i32) -> i32;
    fn last_item_index_in_line(&self, current_item_index: i32) -> i32;
    fn last_item_index(&self) -> i32;
    fn next_end_item_index_in_lane(&self, _lane: i32, current_item_index: i32) -> i32 {
        current_item_index + 1
    }
    fn next_start_item_index_in_lane(&self, _lane: i32, current_item_index: i32) -> i32 {
        current_item_index - 1
    }
    fn is_span_item(&self, _item_index: i32) -> bool {
        false
    }
}

/// A unit in the window: its size, and its key (`None`: one prefetched, Compose's `NoKey`).
#[derive(Clone, Copy)]
struct CachedItem {
    size: i32,
    key: Option<u64>,
}

/// Hashes a unit's index, as Compose's `IntIntMap`: a multiplication, no more.
#[derive(Default)]
struct UnitHasher(u64);

impl std::hash::Hasher for UnitHasher {
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 << 8) | u64::from(*byte);
        }
    }

    fn write_i32(&mut self, unit: i32) {
        self.0 = (unit as u32 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// The window's logic, as Compose's `MultiLaneCacheWindow`.
pub(super) struct CacheWindowLogic {
    pub(super) cache_window: Rc<dyn LazyLayoutCacheWindow>,
    lane_count: Box<dyn Fn() -> usize>,
    /// The requests for units in the window, by unit.
    handles: Vec<(i32, Vec<Rc<Handle>>)>,
    /// What's known of the units in the window, visible or not: few, unless a grid's header sticks
    /// far from the start, when Compose's walk has every line before it here (D1451).
    cached: HashMap<i32, CachedItem, BuildHasherDefault<UnitHasher>>,
    previous_pass_delta: f32,
    previous_pass_item_count: i32,
    has_updated_visible_items_once: bool,
    previous_lane_count: usize,
    /// The units from the window's start to its end, in each lane, are visible, cached, requested
    /// or not requested yet.
    start_index: Vec<i32>,
    end_index: Vec<i32>,
    /// What's left of the window in each lane; starts less what the visible units there peek out.
    start_space: Vec<i32>,
    end_space: Vec<i32>,
    first_visible: Vec<i32>,
    last_visible: Vec<i32>,
    extra_start: Vec<i32>,
    extra_end: Vec<i32>,
    /// Refill the window from the start at the next update: the first layout, a resized or
    /// replaced visible unit, another item count, another lane count.
    should_refill: bool,
    items_count: i32,
}

/// The lane with the most left (the first such), or none.
fn index_of_max(values: &[i32]) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (index, value) in values.iter().enumerate() {
        if best.is_none_or(|best| *value > values[best]) {
            best = Some(index);
        }
    }
    best
}

impl CacheWindowLogic {
    pub(super) fn new(
        cache_window: Rc<dyn LazyLayoutCacheWindow>,
        lane_count: impl Fn() -> usize + 'static,
    ) -> CacheWindowLogic {
        let lanes = lane_count().max(1);
        CacheWindowLogic {
            cache_window,
            lane_count: Box::new(lane_count),
            handles: Vec::new(),
            cached: HashMap::default(),
            previous_pass_delta: 0.0,
            previous_pass_item_count: UNSET_ITEM_COUNT,
            has_updated_visible_items_once: false,
            previous_lane_count: lanes,
            start_index: vec![i32::MAX; lanes],
            end_index: vec![i32::MIN; lanes],
            start_space: vec![0; lanes],
            end_space: vec![0; lanes],
            first_visible: vec![0; lanes],
            last_visible: vec![0; lanes],
            extra_start: vec![0; lanes],
            extra_end: vec![0; lanes],
            should_refill: false,
            items_count: 0,
        }
    }

    /// The requests held, of every unit.
    pub(super) fn handles(&self) -> impl Iterator<Item = &Rc<Handle>> {
        self.handles.iter().flat_map(|(_, handles)| handles)
    }

    /// The units with requests, done or not.
    pub(super) fn units_held(&self) -> impl Iterator<Item = usize> + '_ {
        self.handles
            .iter()
            .filter(|(_, handles)| handles.iter().all(|it| !it.done_or_over()))
            .map(|(unit, _)| *unit as usize)
    }

    /// The window's first and last units, if it has bounds (lane 0: lists and grids have one).
    pub(super) fn bounds(&mut self) -> Option<(i32, i32)> {
        self.has_valid_bounds().then(|| (self.start_index[0], self.end_index[0]))
    }

    fn cached(&self, index: i32) -> Option<&CachedItem> {
        self.cached.get(&index)
    }

    fn cache(&mut self, index: i32, size: i32, key: Option<u64>) {
        self.cached.insert(index, CachedItem { size, key });
    }

    fn has_handle(&self, index: i32) -> bool {
        self.handles.iter().any(|(unit, _)| *unit == index)
    }

    fn handle_lane_resize(&mut self) {
        let lanes = (self.lane_count)().max(1);
        if self.previous_lane_count != lanes {
            self.reset_strategy();
            self.should_refill = true;
        }
    }

    fn lanes(&self) -> usize {
        self.start_index.len()
    }

    /// A scroll by `delta` (negative: towards the end), as `onScroll`.
    pub(super) fn on_scroll(&mut self, scope: &dyn CacheWindowScope, delta: f32) {
        self.handle_lane_resize();
        self.fill_backward(scope, delta);
        self.fill_forward(scope, delta);
        self.previous_pass_delta = delta;
    }

    /// The layout changed what's visible, as `onVisibleItemsUpdated`.
    pub(super) fn on_visible_items_updated(&mut self, scope: &dyn CacheWindowScope) {
        self.handle_lane_resize();
        if !self.has_updated_visible_items_once {
            if self.cache_window.is_non_scroll_caching_enabled() {
                let window = self
                    .cache_window
                    .calculate_ahead_window(scope.density(), scope.main_axis_viewport_size());
                // No window, nothing to fill.
                if window != 0 {
                    self.should_refill = true;
                }
            }
            self.has_updated_visible_items_once = true;
        }
        let total = scope.total_items_count();
        if self.previous_pass_item_count != UNSET_ITEM_COUNT
            && self.previous_pass_item_count != total
        {
            self.should_refill = true;
            self.on_dataset_changed(scope);
        }
        self.items_count = total;
        if scope.has_visible_items() {
            for position in 0..scope.visible_items_count() {
                let index = scope.visible_item_index(position);
                if index == INVALID_INDEX {
                    continue;
                }
                let (size, key) =
                    (scope.visible_item_size(position), scope.visible_item_key(position));
                let lane = scope.visible_item_lane(position);
                if let Some(cached) = self.cached(index)
                    && (cached.size != size || cached.key.is_some_and(|cached| Some(cached) != key))
                {
                    self.should_refill = true;
                }
                self.cache(index, size, key);
                let lane = lane as usize;
                self.start_index[lane] = self.start_index[lane].min(index);
                self.end_index[lane] = self.end_index[lane].max(index);
                // Visible: the request isn't needed any more.
                if let Some(at) = self.handles.iter().position(|(unit, _)| *unit == index) {
                    let (_, handles) = self.handles.swap_remove(at);
                    handles.iter().for_each(|handle| handle.cancel());
                }
            }
            if self.should_refill {
                // In accordance with the last scroll's way.
                let forward = self.previous_pass_delta <= 0.0;
                if self.cache_window.is_non_scroll_caching_enabled() {
                    self.refill(scope, forward);
                } else {
                    self.reset_ahead_window_bounds(scope, forward);
                }
                self.should_refill = false;
            }
        } else {
            // Nothing visible: the dataset is empty. The next update starts over.
            self.reset_strategy();
        }
        self.previous_pass_item_count = total;
    }

    fn on_dataset_changed(&mut self, scope: &dyn CacheWindowScope) {
        if !scope.has_visible_items() {
            return;
        }
        let last = scope.last_item_index();
        for lane in 0..self.lanes() {
            self.start_index[lane] = self.start_index[lane].max(0);
            if last != INVALID_INDEX {
                self.end_index[lane] = self.end_index[lane].min(last);
            }
        }
        // The window starts over, the last scroll's way.
        if self.previous_pass_delta <= 0.0 {
            self.remove_out_of_bounds(scope.last_visible_item_index() + 1, self.items_count - 1);
        } else {
            self.remove_out_of_bounds(0, scope.first_visible_item_index() - 1);
        }
    }

    /// Whether the window has bounds in every lane, as `hasValidBounds`.
    pub(super) fn has_valid_bounds(&mut self) -> bool {
        self.handle_lane_resize();
        (0..self.lanes())
            .all(|lane| self.start_index[lane] != i32::MAX && self.end_index[lane] != i32::MIN)
    }

    fn fill_backward(&mut self, scope: &dyn CacheWindowScope, delta: f32) {
        if scope.has_visible_items() {
            let window = self
                .cache_window
                .calculate_behind_window(scope.density(), scope.main_axis_viewport_size());
            self.items_count = scope.total_items_count();
            self.on_keep_around(scope, window, delta, self.items_count);
        }
    }

    fn fill_forward(&mut self, scope: &dyn CacheWindowScope, delta: f32) {
        if scope.has_visible_items() {
            let window = self
                .cache_window
                .calculate_ahead_window(scope.density(), scope.main_axis_viewport_size());
            self.on_prefetch_forward(scope, window, delta, delta <= 0.0);
        }
    }

    fn refill(&mut self, scope: &dyn CacheWindowScope, forward: bool) {
        if scope.has_visible_items() {
            let window = self
                .cache_window
                .calculate_ahead_window(scope.density(), scope.main_axis_viewport_size());
            self.on_prefetch_forward(scope, window, 0.0, forward);
        }
    }

    fn reset_ahead_window_bounds(&mut self, scope: &dyn CacheWindowScope, forward: bool) {
        if !scope.has_visible_items() {
            return;
        }
        let window = self
            .cache_window
            .calculate_ahead_window(scope.density(), scope.main_axis_viewport_size());
        if forward {
            scope.update_per_lane_last_visible_item_indexes(&mut self.last_visible);
            scope.update_per_lane_main_axis_extra_end_space(&mut self.extra_end);
            for lane in 0..self.lanes() {
                self.end_index[lane] = self.last_visible[lane];
                self.end_space[lane] = window - self.extra_end[lane];
            }
            let end = self.end_index.iter().copied().max().unwrap_or(i32::MIN);
            self.remove_out_of_bounds(end + 1, self.items_count - 1);
        } else {
            scope.update_per_lane_first_visible_item_index(&mut self.first_visible);
            scope.update_per_lane_main_axis_extra_start_space(&mut self.extra_start);
            for lane in 0..self.lanes() {
                self.start_index[lane] = self.first_visible[lane];
                self.start_space[lane] = window - self.extra_start[lane];
            }
            let start = self.start_index.iter().copied().min().unwrap_or(i32::MAX);
            self.remove_out_of_bounds(0, start - 1);
        }
    }

    /// Starts over, cancelling every request, as `resetStrategy`.
    pub(super) fn reset_strategy(&mut self) {
        let lanes = (self.lane_count)().max(1);
        self.previous_lane_count = lanes;
        if self.start_index.len() != lanes {
            self.start_index = vec![i32::MAX; lanes];
            self.end_index = vec![i32::MIN; lanes];
            self.start_space = vec![0; lanes];
            self.end_space = vec![0; lanes];
            self.first_visible = vec![0; lanes];
            self.last_visible = vec![0; lanes];
            self.extra_start = vec![0; lanes];
            self.extra_end = vec![0; lanes];
        } else {
            self.start_index.fill(i32::MAX);
            self.end_index.fill(i32::MIN);
            self.start_space.fill(0);
            self.end_space.fill(0);
        }
        self.should_refill = false;
        self.cached.clear();
        for (_, handles) in self.handles.drain(..) {
            handles.iter().for_each(|handle| handle.cancel());
        }
    }

    /// Fills the window the scroll's way with what's known, requesting the next unit not known, as
    /// `onPrefetchForward`; the next is requested when it's done (`on_item_prefetched`).
    fn on_prefetch_forward(
        &mut self,
        scope: &dyn CacheWindowScope,
        window: i32,
        scroll_delta: f32,
        apply_forward: bool,
    ) {
        // Kotlin's `sign`: -1, 0 or 1.
        let sign = |value: f32| {
            if value > 0.0 {
                1
            } else if value < 0.0 {
                -1
            } else {
                0
            }
        };
        let changed_direction = sign(scroll_delta) != sign(self.previous_pass_delta);
        let moved = scroll_delta.abs().round() as i32;
        if apply_forward {
            scope.update_per_lane_last_visible_item_indexes(&mut self.last_visible);
            scope.update_per_lane_main_axis_extra_end_space(&mut self.extra_end);
            for lane in 0..self.lanes() {
                let remaining = window - self.extra_end[lane];
                if changed_direction || self.should_refill {
                    self.end_index[lane] = self.last_visible[lane];
                    self.end_space[lane] = remaining;
                } else {
                    self.end_space[lane] =
                        self.end_space[lane].saturating_add(moved).min(remaining);
                }
            }
            while let Some(lane) = index_of_max(&self.end_space)
                && self.end_space[lane] > 0
            {
                let final_in_line = scope.last_item_index_in_line(self.end_index[lane]);
                if final_in_line == INVALID_INDEX || final_in_line >= self.items_count - 1 {
                    self.end_space[lane] = 0;
                    continue;
                }
                let lane_id = lane as i32;
                let item = scope.next_end_item_index_in_lane(lane_id, self.end_index[lane]);
                // The same delta next frame would need it: urgent.
                let urgent = item
                    == scope.next_end_item_index_in_lane(lane_id, self.last_visible[lane])
                    && scroll_delta != 0.0
                    && scroll_delta.abs() >= self.extra_end[lane] as f32;
                let size = self.size_or_prefetch(scope, lane_id, urgent, item);
                if size == INVALID_ITEM_SIZE {
                    break;
                }
                self.update_end_state(scope, lane, item, size);
            }
        } else {
            scope.update_per_lane_first_visible_item_index(&mut self.first_visible);
            scope.update_per_lane_main_axis_extra_start_space(&mut self.extra_start);
            for lane in 0..self.lanes() {
                let remaining = window - self.extra_start[lane];
                if changed_direction || self.should_refill {
                    self.start_space[lane] = remaining;
                    self.start_index[lane] = self.first_visible[lane];
                } else {
                    self.start_space[lane] =
                        self.start_space[lane].saturating_add(moved).min(remaining);
                }
            }
            while let Some(lane) = index_of_max(&self.start_space)
                && self.start_space[lane] > 0
            {
                if self.start_index[lane] <= 0 {
                    self.start_space[lane] = 0;
                    continue;
                }
                let lane_id = lane as i32;
                let item = scope.next_start_item_index_in_lane(lane_id, self.start_index[lane]);
                let urgent = item
                    == scope.next_start_item_index_in_lane(lane_id, self.first_visible[lane])
                    && scroll_delta != 0.0
                    && scroll_delta.abs() >= self.extra_start[lane] as f32;
                let size = self.size_or_prefetch(scope, lane_id, urgent, item);
                if size == INVALID_ITEM_SIZE {
                    break;
                }
                self.update_start_state(scope, lane, item, size);
            }
        }
    }

    /// Keeps what was visible behind the scroll while the window has room, requesting nothing, as
    /// `onKeepAround`; what's beyond it is let go.
    fn on_keep_around(
        &mut self,
        scope: &dyn CacheWindowScope,
        window: i32,
        scroll_delta: f32,
        items_count: i32,
    ) {
        if scroll_delta <= 0.0 {
            scope.update_per_lane_first_visible_item_index(&mut self.first_visible);
            scope.update_per_lane_main_axis_extra_start_space(&mut self.extra_start);
            for lane in 0..self.lanes() {
                self.start_space[lane] = window - self.extra_start[lane];
                self.start_index[lane] = self.first_visible[lane];
            }
            while let Some(lane) = index_of_max(&self.start_space)
                && self.start_space[lane] > 0
            {
                let next = scope.next_start_item_index_in_lane(lane as i32, self.start_index[lane]);
                if next == INVALID_INDEX {
                    self.start_space[lane] = 0;
                    continue;
                }
                let Some(size) = self.cached(next).map(|item| item.size) else {
                    self.start_space[lane] = 0;
                    continue;
                };
                self.update_start_state(scope, lane, next, size);
            }
            let start = self.start_index.iter().copied().min().unwrap_or(i32::MAX);
            self.remove_out_of_bounds(0, start - 1);
        } else {
            scope.update_per_lane_last_visible_item_indexes(&mut self.last_visible);
            scope.update_per_lane_main_axis_extra_end_space(&mut self.extra_end);
            for lane in 0..self.lanes() {
                self.end_space[lane] = window - self.extra_end[lane];
                self.end_index[lane] = self.last_visible[lane];
            }
            while let Some(lane) = index_of_max(&self.end_space)
                && self.end_space[lane] > 0
            {
                let next = scope.next_end_item_index_in_lane(lane as i32, self.end_index[lane]);
                if next == INVALID_INDEX
                    || scope.last_item_index_in_line(self.end_index[lane]) >= items_count - 1
                {
                    self.end_space[lane] = 0;
                    continue;
                }
                let Some(size) = self.cached(next).map(|item| item.size) else {
                    self.end_space[lane] = 0;
                    continue;
                };
                self.update_end_state(scope, lane, next, size);
            }
            let end = self.end_index.iter().copied().max().unwrap_or(i32::MIN);
            self.remove_out_of_bounds(end + 1, items_count - 1);
        }
    }

    fn size_or_prefetch(
        &mut self,
        scope: &dyn CacheWindowScope,
        lane: i32,
        urgent: bool,
        item: i32,
    ) -> i32 {
        if let Some(cached) = self.cached(item) {
            return cached.size;
        }
        if !self.has_handle(item) {
            let handles = scope.schedule_prefetch(lane, item);
            self.handles.push((item, handles));
        }
        if urgent && let Some((_, handles)) = self.handles.iter().find(|(unit, _)| *unit == item) {
            handles.iter().for_each(|handle| handle.mark_as_urgent());
        }
        INVALID_ITEM_SIZE
    }

    fn cache_prefetched(&mut self, scope: &dyn CacheWindowScope, lane: i32, item: i32, size: i32) {
        self.cache(item, size, None);
        let lane = lane as usize;
        if item > self.end_index[lane] {
            self.update_end_state(scope, lane, item, size);
        } else if item < self.start_index[lane] {
            self.update_start_state(scope, lane, item, size);
        }
    }

    fn remove_out_of_bounds(&mut self, start: i32, end: i32) {
        let bounds = start..=end;
        let handles = &mut self.handles;
        let mut index = 0;
        while index < handles.len() {
            if bounds.contains(&handles[index].0) {
                let (_, removed) = handles.swap_remove(index);
                removed.iter().for_each(|handle| handle.cancel());
            } else {
                index += 1;
            }
        }
        self.cached.retain(|index, _| !bounds.contains(index));
    }

    /// A requested unit is done, of main-axis size `size`: known now, and the next is requested if
    /// the window has room, as `onItemPrefetched`.
    pub(super) fn on_item_prefetched(
        &mut self,
        scope: &dyn CacheWindowScope,
        lane: i32,
        item: i32,
        size: i32,
    ) {
        self.cache_prefetched(scope, lane, item, size);
        self.schedule_next_item_if_needed(scope);
    }

    /// Whether `handle` is one of `item`'s requests still.
    pub(super) fn holds(&self, item: i32, handle: &Handle) -> bool {
        self.handles.iter().any(|(unit, handles)| {
            *unit == item && handles.iter().any(|held| std::ptr::eq(Rc::as_ptr(held), handle))
        })
    }

    /// A request ended without its unit: forgotten, the next scroll asks again.
    pub(super) fn forget(&mut self, item: i32) {
        self.handles.retain(|(unit, _)| *unit != item);
    }

    fn schedule_next_item_if_needed(&mut self, scope: &dyn CacheWindowScope) {
        let mut next = INVALID_INDEX;
        let mut next_lane = INVALID_INDEX;
        if self.previous_pass_delta <= 0.0 {
            while let Some(lane) = index_of_max(&self.end_space)
                && self.end_space[lane] > 0
            {
                let index = scope.next_end_item_index_in_lane(lane as i32, self.end_index[lane]);
                let final_in_line = scope.last_item_index_in_line(index);
                if final_in_line == INVALID_INDEX || final_in_line >= self.items_count {
                    self.end_space[lane] = 0;
                    continue;
                }
                next = index;
                next_lane = lane as i32;
                break;
            }
        } else {
            while let Some(lane) = index_of_max(&self.start_space)
                && self.start_space[lane] > 0
            {
                if self.start_index[lane] <= 0 {
                    self.start_space[lane] = 0;
                    continue;
                }
                let index =
                    scope.next_start_item_index_in_lane(lane as i32, self.start_index[lane]);
                if scope.last_item_index_in_line(index) == INVALID_INDEX {
                    self.start_space[lane] = 0;
                    continue;
                }
                next = index;
                next_lane = lane as i32;
                break;
            }
        }
        if next >= 0 && !self.has_handle(next) && self.cached(next).is_none() {
            let handles = scope.schedule_prefetch(next_lane, next);
            self.handles.push((next, handles));
        }
    }

    fn update_end_state(
        &mut self,
        scope: &dyn CacheWindowScope,
        lane: usize,
        item: i32,
        size: i32,
    ) {
        if self.lanes() > 1 && scope.is_span_item(item) {
            let least = self.end_space.iter().copied().min().unwrap_or(0);
            self.end_index.fill(item);
            self.end_space.fill(least - size);
        } else {
            self.end_index[lane] = item;
            self.end_space[lane] -= size;
        }
    }

    fn update_start_state(
        &mut self,
        scope: &dyn CacheWindowScope,
        lane: usize,
        item: i32,
        size: i32,
    ) {
        if self.lanes() > 1 && scope.is_span_item(item) {
            let least = self.start_space.iter().copied().min().unwrap_or(0);
            self.start_index.fill(item);
            self.start_space.fill(least - size);
        } else {
            self.start_index[lane] = item;
            self.start_space[lane] -= size;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    struct FakeScope;

    impl CacheWindowScope for FakeScope {
        fn total_items_count(&self) -> i32 {
            10
        }
        fn visible_items_count(&self) -> i32 {
            2
        }
        fn has_visible_items(&self) -> bool {
            true
        }
        fn first_visible_item_index(&self) -> i32 {
            0
        }
        fn density(&self) -> Density {
            Density::new(1.0)
        }
        fn last_visible_item_index(&self) -> i32 {
            1
        }
        fn main_axis_viewport_size(&self) -> i32 {
            100
        }
        fn update_per_lane_main_axis_extra_start_space(&self, _: &mut [i32]) {}
        fn update_per_lane_main_axis_extra_end_space(&self, _: &mut [i32]) {}
        fn update_per_lane_first_visible_item_index(&self, _: &mut [i32]) {}
        fn update_per_lane_last_visible_item_indexes(&self, _: &mut [i32]) {}
        fn schedule_prefetch(&self, _: i32, _: i32) -> Vec<Rc<Handle>> {
            Vec::new()
        }
        fn visible_item_size(&self, _: i32) -> i32 {
            10
        }
        fn visible_item_index(&self, position: i32) -> i32 {
            position
        }
        fn visible_item_key(&self, position: i32) -> Option<u64> {
            Some(position as u64)
        }
        fn visible_item_lane(&self, position: i32) -> i32 {
            position
        }
        fn last_item_index_in_line(&self, item: i32) -> i32 {
            item
        }
        fn last_item_index(&self) -> i32 {
            9
        }
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/lazy/layout/CacheWindowLogicTest.kt, handleLaneResizeResetsStrategy
    #[test]
    fn a_change_of_lanes_starts_the_window_over() {
        let lanes = Rc::new(Cell::new(2usize));
        let counted = lanes.clone();
        let mut logic =
            CacheWindowLogic::new(lazy_layout_cache_window_fraction(1.0, 1.0), move || {
                counted.get()
            });
        assert_eq!((logic.start_index.len(), logic.end_index.len()), (2, 2));
        logic.on_visible_items_updated(&FakeScope);
        assert_ne!(logic.start_index[0], i32::MAX);
        assert_ne!(logic.start_index[1], i32::MAX);
        lanes.set(3);
        logic.has_valid_bounds();
        assert_eq!((logic.start_index.len(), logic.end_index.len()), (3, 3));
        assert_eq!(logic.start_index, [i32::MAX; 3]);
    }

    // Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/layout/LazyLayoutCacheWindow.kt, the factories' equals
    #[test]
    fn equal_windows_are_the_same_window() {
        use compose_ui::UnitExt;
        let dp = lazy_layout_cache_window(10.dp(), 0.dp());
        assert!(same_window(&dp, &lazy_layout_cache_window(10.dp(), 0.dp())));
        assert!(!same_window(&dp, &lazy_layout_cache_window(20.dp(), 0.dp())));
        let fraction = lazy_layout_cache_window_fraction(0.5, 0.0);
        assert!(same_window(&fraction, &lazy_layout_cache_window_fraction(0.5, 0.0)));
        assert!(!same_window(&fraction, &dp));
    }
}
