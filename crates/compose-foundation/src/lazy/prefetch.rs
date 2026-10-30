//! Prefetch: what a lazy list (or grid) composes and measures between frames, as Compose's
//! `LazyLayoutPrefetchState` at the tip, its requests coming from the cache window
//! (`cache_window`, Compose's `LazyListCacheWindowStrategy` and its grid's, D892):
//!
//! - after a layout that scrolled, the window fills: requests one unit at a time (a list's item, a
//!   grid's line, a request per item), the next when the last is done, until the window has no room;
//!   those behind the (empty, by default) behind window, or out of it, are cancelled and let go;
//!   urgent when the same scroll again would reach the unit;
//! - a request composes its item with a paused composition, which pauses at the runtime's pause
//!   points once the time left is no more than the averages of a stretch of composition and a
//!   pause, and resumes after the next frame; it applies it once complete, then composes the first
//!   items of the lazy lists in it (two, then the running average of the smallest nested list's
//!   visible count, per content type), composing only, then measures it: nothing places or draws
//!   it until a frame wants it;
//! - each step runs only while the time left exceeds that step's running average for the item's
//!   content type (`current / 4 * 3 + new / 4`, in integer nanoseconds; the first sample taken
//!   as is): a stretch and a pause; applying, measuring, their own; any time left for an urgent
//!   request; a step that doesn't fit leaves the request queued, to go on after the next frame.
//!
//! The composed items are those in view and those in the window, by its bounds as a measurement
//! reads them between what's in view and the scroll, as `keepAroundItems`: what a scroll leaves
//! behind stays until the next measurement (D896), and the others are deactivated (kept for reuse).
//! An item whose paused composition isn't
//! applied yet (or was cancelled, its content removed) is one the layout lacks: wanted, the frame
//! composes it (completing the paused composition first), as Compose's measurement completes a
//! precomposed item's paused composition.
//!
//! compose-rs's own behaviours are optimizations of the host's (D560-D565), widening the default
//! window only (D893): a second item's worth ahead (`PREFETCH_TWO_AHEAD`), two items' worth kept
//! behind (`KEEP_ITEMS_BEHIND`), filling at rest (`PREFETCH_AT_REST`); and D341/D381's estimate
//! (`PREFETCH_BUDGET_RECENT_MAXIMUM`).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Range;
use std::rc::{Rc, Weak};
use std::time::Instant;

use compose_runtime::PausedComposition;
use compose_ui::platform::{NodeId, Optimization, Optimizations};
use compose_ui::{Constraints, Density, PrefetchRequest, PrefetchRequestScope, PrefetchScheduler};

use super::cache_window::{
    CacheWindowLogic, CacheWindowScope, DefaultCacheWindow, LazyLayoutCacheWindow, same_window,
};
use super::{Items, LazyListInner};

/// Items' worth the default window keeps behind, with `KEEP_ITEMS_BEHIND`: a scroll reversing
/// direction finds the first items it wants composed (D375, D893).
pub(super) const KEPT_BEHIND: i32 = 2;

/// What a prefetch step leaves of the idle time beyond its estimate, at least, with
/// `PREFETCH_BUDGET_RECENT_MAXIMUM`: the estimate is a guess, and a step running past the
/// deadline delays the next frame (D341).
pub(super) const PREFETCH_MARGIN_NANOS: u64 = 500_000;

/// The items of a nested list a request composes before its first layout of that content type,
/// as Compose's `nestedPrefetchItemCount` (2).
const INITIAL_NESTED_PREFETCH_COUNT: usize = 2;

/// The items composed by requests a list remembers (`Prefetch::was_precomposed`).
const PRECOMPOSED_KEPT: usize = 8;

/// Items noted with nested lists beyond which the dead entries are forgotten.
const NESTED_KEYS_KEPT: usize = 64;

/// Which of compose-rs's own prefetch behaviours a list uses, from the host's optimizations: the
/// default window's shape (D893), and the budget.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct Policy {
    /// The default window ahead has another item's worth (`PREFETCH_TWO_AHEAD`, D156).
    pub(super) two_ahead: bool,
    /// D341 and D381's budget (`PREFETCH_BUDGET_RECENT_MAXIMUM`).
    pub(super) recent_maximum: bool,
    /// The default window behind has two items' worth (`KEEP_ITEMS_BEHIND`, D375).
    pub(super) keep_behind: bool,
    /// The default window fills at rest too (`PREFETCH_AT_REST`, D470).
    pub(super) at_rest: bool,
}

impl Policy {
    /// Compose's.
    pub(super) const COMPOSE: Policy =
        Policy { two_ahead: false, recent_maximum: false, keep_behind: false, at_rest: false };

    pub(super) fn of(optimizations: Optimizations) -> Policy {
        Policy {
            two_ahead: optimizations.contains(Optimization::PREFETCH_TWO_AHEAD),
            recent_maximum: optimizations.contains(Optimization::PREFETCH_BUDGET_RECENT_MAXIMUM),
            keep_behind: optimizations.contains(Optimization::KEEP_ITEMS_BEHIND),
            at_rest: optimizations.contains(Optimization::PREFETCH_AT_REST),
        }
    }
}

/// An item's constraints by its index (`None`: no such item).
pub(super) type ItemConstraints = Rc<dyn Fn(usize) -> Option<Constraints>>;

/// What prefetching works from, as last laid out, and what it learned.
pub(super) struct Prefetch {
    /// The list's node, its item count and the items its layout wants composed.
    pub(super) node: Cell<Option<NodeId>>,
    pub(super) count: Cell<usize>,
    pub(super) wanted: Cell<(usize, usize)>,
    /// The last scroll went towards the end (as if, before any): the side `KEEP_ITEMS_BEHIND`
    /// keeps, and the way `PREFETCH_AT_REST` prefetches.
    pub(super) forward: Cell<bool>,
    /// Scrolled and not told the window yet, in Compose's sign: negative towards the end.
    pub(super) delta: Cell<f32>,
    /// A scroll asked for the next layout, within what's in view perhaps: Compose measures for
    /// none of those, so `kept` stays.
    pub(super) scroll_asked: Cell<bool>,
    /// The items composed may span, set by each layout: the wanted items, those in the window,
    /// and those requests composed next to them.
    pub(super) limits: Cell<(usize, usize)>,
    /// Each item's constraints, where they differ by item (a grid's spans); else the list's
    /// `item_constraints`.
    pub(super) constraints_of: RefCell<Option<ItemConstraints>>,
    /// The host's prefetch behaviours, as last laid out.
    pub(super) policy: Cell<Policy>,
    /// The items, as last composed: their keys and content types.
    pub(super) items: RefCell<Weak<Items>>,
    /// The list's axis, for an item's size along it.
    pub(super) vertical: Cell<bool>,
    /// Items in view at the last layout: a parent's nested prefetch learns its count from it
    /// (Compose's `idealNestedPrefetchCount`).
    pub(super) visible: Cell<usize>,
    /// The window's logic, and what the last layout that settled showed it.
    window: RefCell<Option<CacheWindowLogic>>,
    /// The list's composition the logic is for, as Compose's `remember(state, cacheWindow)`.
    window_owner: RefCell<Option<Rc<()>>>,
    laid: RefCell<Shown>,
    /// The window's bounds as Compose's `keepAroundItems` reads them: after what's in view is told
    /// and before the scroll is, so an item the scroll leaves behind stays until the next
    /// measurement (D894).
    kept: Cell<Option<(usize, usize)>>,
    /// A request's composition asked for the next layout, which Compose's precomposition doesn't:
    /// it's no measurement to it, and leaves `kept` alone.
    measure_by_request: Cell<bool>,
    /// Items composed apart from the composed range: those before a sticky header sticking away
    /// from it, which requests composed or the window keeps (Compose's window starts from the
    /// header, D895). A list's items in no slot's order.
    pub(super) detached: RefCell<Vec<usize>>,
    /// Some of `detached` went: the next composition lets them go.
    pub(super) detached_dirty: Cell<bool>,
    /// A buffer for the units in view, with their sizes.
    pub(super) sizes: RefCell<Vec<(usize, i32)>>,
    /// A grid's lines, for the window's requests between frames; a list's units are its items.
    pub(super) units: RefCell<Option<Rc<dyn Units>>>,
    /// Compose's `PrefetchMetrics`: the steps' averages by content type.
    metrics: RefCell<HashMap<Option<u64>, Averages>>,
    /// D341's estimates (`PREFETCH_BUDGET_RECENT_MAXIMUM`), one for all content types: a whole
    /// composition's (a paused one's resumes summed), an apply's, a measurement's.
    compose_cost: Cell<StepCost>,
    apply_cost: Cell<StepCost>,
    measure_cost: Cell<StepCost>,
    /// This list's steps in the idle time under way (`PREFETCH_BUDGET_RECENT_MAXIMUM`).
    idle: Cell<IdleSteps>,
    /// The key of the item whose content is composing now (set by `LazyItem` around its body, by
    /// a request around its resumes, by a completion asked for), and the lazy lists composed in
    /// each item, by its key: a request finds its item's nested lists there, whoever composed
    /// it (Compose traverses the precomposed item's nodes for them, `traversablePrefetchState`).
    pub(super) composing_item: Cell<Option<u64>>,
    nested: RefCell<HashMap<u64, Vec<Weak<LazyListInner>>>>,
    /// The paused composition a request holds, not applied yet (the request owns it: dropped,
    /// it's cancelled).
    pending: RefCell<Option<(u64, Weak<PausedComposition>)>>,
    /// The item whose content isn't the layout's to measure: a request's paused composition's,
    /// there unapplied (composing, complete, or cancelled with its removal waiting for the next
    /// pass), whoever holds the paused composition now (D592, D599). One at a time, as paused
    /// compositions; cleared once the content is applied or removed.
    unready: RefCell<Option<(u64, compose_runtime::PausedWatch)>>,
    /// A request's paused composition was cancelled (its content removed, the composable around
    /// it invalidated): the layout has the items composed again (`take_cancelled`), so the next
    /// pass composes the items' composition first, which lets the item go if it's no longer
    /// wanted, rather than composing its content again (D594).
    cancelled: Cell<bool>,
    /// The scheduler the last layout prefetched with.
    scheduler: RefCell<Option<PrefetchScheduler>>,
    /// Items a request composed, the latest last (a few): leaving before they're measured, they
    /// go to the reuse pool, as Compose's precomposed slots (`PrecomposedSlotHandle.dispose`),
    /// where an item never measured is disposed (D460).
    precomposed: RefCell<Vec<u64>>,
    /// The trimming request waiting in the scheduler, if any (`KEEP_ITEMS_BEHIND`).
    trim: RefCell<Weak<TrimRequest>>,
}

impl Prefetch {
    pub(super) fn new() -> Prefetch {
        Prefetch {
            node: Cell::new(None),
            count: Cell::new(0),
            wanted: Cell::new((0, 0)),
            forward: Cell::new(true),
            delta: Cell::new(0.0),
            scroll_asked: Cell::new(false),
            limits: Cell::new((0, 0)),
            constraints_of: RefCell::new(None),
            policy: Cell::new(Policy::COMPOSE),
            items: RefCell::new(Weak::new()),
            vertical: Cell::new(true),
            visible: Cell::new(0),
            window: RefCell::new(None),
            window_owner: RefCell::new(None),
            laid: RefCell::default(),
            kept: Cell::new(None),
            measure_by_request: Cell::new(false),
            detached: RefCell::default(),
            detached_dirty: Cell::new(false),
            sizes: RefCell::default(),
            units: RefCell::new(None),
            metrics: RefCell::default(),
            compose_cost: Cell::default(),
            apply_cost: Cell::default(),
            measure_cost: Cell::default(),
            idle: Cell::default(),
            composing_item: Cell::new(None),
            nested: RefCell::default(),
            pending: RefCell::new(None),
            unready: RefCell::default(),
            cancelled: Cell::new(false),
            scheduler: RefCell::new(None),
            precomposed: RefCell::default(),
            trim: RefCell::new(Weak::new()),
        }
    }
}

/// What the window's units are: a list's are its items, a grid's its lines.
pub(super) trait Units {
    /// The unit's items (`None`: no such unit).
    fn items_of(&self, unit: usize) -> Option<Range<usize>>;
    /// The unit of `item`.
    fn unit_of(&self, item: usize) -> usize;
}

/// A list's units.
struct ItemUnits(usize);

impl Units for ItemUnits {
    fn items_of(&self, unit: usize) -> Option<Range<usize>> {
        (unit < self.0).then(|| unit..unit + 1)
    }

    fn unit_of(&self, item: usize) -> usize {
        item
    }
}

/// What a layout that settled shows, for the window: the first and last units in view, as Compose's
/// `visibleItemsInfo` (a sticking header first, D895), how far the first starts before the
/// viewport and the last ends, with its spacing, past it (or short of it).
pub(super) struct Laid {
    pub(super) first: usize,
    pub(super) last: usize,
    pub(super) start_overflow: i32,
    pub(super) end_overflow: i32,
    pub(super) viewport: i32,
    pub(super) spacing: i32,
    pub(super) density: Density,
}

/// The last `Laid`, the units in view (their sizes and keys), and the count.
struct Shown {
    first: i32,
    last: i32,
    start_overflow: i32,
    end_overflow: i32,
    viewport: i32,
    density: Density,
    count: i32,
    /// The units in view, by index: index, size and key (a sticking header's index is apart).
    visible: Vec<(i32, i32, u64)>,
    average: i32,
}

impl Default for Shown {
    fn default() -> Shown {
        Shown {
            first: 0,
            last: 0,
            start_overflow: 0,
            end_overflow: 0,
            viewport: 0,
            density: Density::new(1.0),
            count: 0,
            visible: Vec::new(),
            average: 0,
        }
    }
}

/// The window's view of its layout, as Compose's `CacheWindowScope` over its layout info.
struct WindowScope<'a> {
    state: &'a Rc<LazyListInner>,
    prefetch: &'a Prefetch,
    shown: &'a Shown,
    units: &'a dyn Units,
    scheduler: Option<&'a PrefetchScheduler>,
}

impl CacheWindowScope for WindowScope<'_> {
    fn total_items_count(&self) -> i32 {
        self.shown.count
    }

    fn visible_items_count(&self) -> i32 {
        self.shown.visible.len() as i32
    }

    fn has_visible_items(&self) -> bool {
        !self.shown.visible.is_empty()
    }

    fn first_visible_item_index(&self) -> i32 {
        self.shown.first
    }

    fn density(&self) -> Density {
        self.shown.density
    }

    fn last_visible_item_index(&self) -> i32 {
        self.shown.last
    }

    fn main_axis_viewport_size(&self) -> i32 {
        self.shown.viewport
    }

    fn update_per_lane_main_axis_extra_start_space(&self, spaces: &mut [i32]) {
        spaces[0] = self.shown.start_overflow;
    }

    fn update_per_lane_main_axis_extra_end_space(&self, spaces: &mut [i32]) {
        spaces[0] = self.shown.end_overflow;
    }

    fn update_per_lane_first_visible_item_index(&self, indices: &mut [i32]) {
        indices[0] = self.shown.first;
    }

    fn update_per_lane_last_visible_item_indexes(&self, indices: &mut [i32]) {
        indices[0] = self.shown.last;
    }

    fn schedule_prefetch(&self, lane: i32, item_index: i32) -> Vec<Rc<Handle>> {
        let (Some(scheduler), Ok(unit)) = (self.scheduler, usize::try_from(item_index)) else {
            return Vec::new();
        };
        let Some(items) = self.prefetch.items.borrow().upgrade() else { return Vec::new() };
        let Some(range) = self.units.items_of(unit) else { return Vec::new() };
        // Each composes next to the composed items: a line before them from its end.
        let forward = item_index > self.shown.last;
        let range: Vec<usize> = if forward { range.collect() } else { range.rev().collect() };
        let mut handles = Vec::with_capacity(range.len());
        for item in range {
            if item >= self.shown.count as usize {
                continue;
            }
            let handle = Rc::new(Handle::new(self.state, &items, item, unit, Some(lane), true));
            handle.queued.set(true);
            handles.push(handle.clone());
            scheduler.schedule_prefetch(Rc::new(HandleRequest(handle)));
        }
        handles
    }

    fn visible_item_size(&self, position: i32) -> i32 {
        self.shown.visible[position as usize].1
    }

    fn visible_item_index(&self, position: i32) -> i32 {
        self.shown.visible[position as usize].0
    }

    fn visible_item_key(&self, position: i32) -> u64 {
        self.shown.visible[position as usize].2
    }

    fn visible_item_lane(&self, _: i32) -> i32 {
        0
    }

    fn last_item_index_in_line(&self, item: i32) -> i32 {
        usize::try_from(item)
            .ok()
            .and_then(|unit| self.units.items_of(unit))
            .map_or(-1, |range| range.end as i32 - 1)
    }

    fn last_item_index(&self) -> i32 {
        if self.shown.count == 0 {
            return -1;
        }
        self.units.unit_of(self.shown.count as usize - 1) as i32
    }
}

impl Prefetch {
    /// The list's composition: `window` is its cache window from now on (`None`: the default, as
    /// `LazyListDefaults.cacheWindow(state)`). Another one, or another list's composition (`owner`,
    /// remembered there), starts over, as Compose's `remember(state, cacheWindow)` makes another
    /// strategy.
    pub(super) fn use_window(
        &self,
        state: &Rc<LazyListInner>,
        window: Option<Rc<dyn LazyLayoutCacheWindow>>,
        owner: &Rc<()>,
    ) {
        let window = window.unwrap_or_else(|| {
            let default = Rc::as_ptr(state) as usize;
            let current = self.window.borrow();
            match current.as_ref().map(|logic| logic.cache_window.value()) {
                Some(Some(super::cache_window::WindowValue::Default(at))) if at == default => {
                    current.as_ref().expect("there").cache_window.clone()
                }
                _ => Rc::new(DefaultCacheWindow { state: Rc::downgrade(state) }),
            }
        });
        let mut current = self.window.borrow_mut();
        let mut current_owner = self.window_owner.borrow_mut();
        if current_owner.as_ref().is_some_and(|kept| Rc::ptr_eq(kept, owner))
            && current.as_ref().is_some_and(|logic| same_window(&logic.cache_window, &window))
        {
            return;
        }
        *current_owner = Some(owner.clone());
        if let Some(old) = current.as_mut() {
            old.reset_strategy();
        }
        *current = Some(CacheWindowLogic::new(window, || 1));
        *self.laid.borrow_mut() = Shown::default();
        self.kept.set(None);
        let mut detached = self.detached.borrow_mut();
        if !detached.is_empty() {
            detached.clear();
            self.detached_dirty.set(true);
        }
    }

    /// The cache window in use.
    pub(super) fn cache_window(&self) -> Option<Rc<dyn LazyLayoutCacheWindow>> {
        self.window.borrow().as_ref().map(|logic| logic.cache_window.clone())
    }

    /// The units in view's average size and spacing, as `visibleItemsAverageSize`.
    pub(super) fn visible_average(&self) -> i32 {
        self.laid.borrow().average
    }

    /// The position changed by a jump: requests are cancelled and the next scroll refills the
    /// window, as `snapToItemIndexInternal`.
    pub(super) fn reset_window(&self) {
        if let Ok(mut window) = self.window.try_borrow_mut()
            && let Some(logic) = window.as_mut()
        {
            logic.reset_strategy();
        }
        self.kept.set(None);
    }

    /// The window's first and last units as the last measurement read them (`kept`).
    pub(super) fn window_bounds(&self) -> Option<(usize, usize)> {
        self.kept.get()
    }

    fn keep_bounds(&self, logic: &mut CacheWindowLogic) {
        self.kept.set(logic.bounds().and_then(|(start, end)| {
            Some((usize::try_from(start).ok()?, usize::try_from(end).ok()?))
        }));
    }

    /// A focus search ended: what it laid out goes by the window as it is now, as a search's jump
    /// to what it found is here no scroll over items the window ever held (D482).
    pub(super) fn search_ended(&self) {
        if let Some(logic) = self.window.borrow_mut().as_mut() {
            self.keep_bounds(logic);
        }
    }

    /// The next layout is a measurement to the window, as Compose's `Remeasurement`.
    pub(super) fn measurement_forced(&self) {
        self.measure_by_request.set(false);
        self.scroll_asked.set(false);
    }

    /// The requests under way: their items, and whether they are urgent.
    pub(super) fn requests(&self) -> Vec<(usize, bool)> {
        let window = self.window.borrow();
        let Some(logic) = window.as_ref() else { return Vec::new() };
        let live = logic.handles().filter(|handle| !handle.canceled.get() && !handle.done.get());
        let mut requests: Vec<_> = live.map(|handle| (handle.item, handle.urgent.get())).collect();
        requests.sort_unstable();
        requests
    }

    /// The units with requests held, whatever they request.
    pub(super) fn units_held(&self) -> Vec<usize> {
        let window = self.window.borrow();
        let Some(logic) = window.as_ref() else { return Vec::new() };
        logic.units_held().collect()
    }

    /// The items composed apart from the range after a layout: those before what's shown first,
    /// in the window, or that a request holds; `apart`: the range starts past a sticking header,
    /// so what the range held before it comes here, as Compose keeps by index. The others go with
    /// the next composition.
    pub(super) fn settle_detached(
        &self,
        first_shown: usize,
        composed: (usize, usize),
        limits: (usize, usize),
        apart: bool,
    ) {
        let window = self.kept.get();
        let mut detached = self.detached.borrow_mut();
        if !detached.is_empty() {
            let logic = self.window.borrow();
            let holds = |item: usize| {
                logic.as_ref().is_some_and(|logic| {
                    logic.handles().any(|handle| handle.item == item && !handle.canceled.get())
                })
            };
            let dirty = &self.detached_dirty;
            detached.retain(|&item| {
                let in_range = (limits.0..limits.1).contains(&item);
                let stays = !in_range
                    && (window.is_some_and(|(first, _)| (first..first_shown).contains(&item))
                        || holds(item));
                if !stays && !in_range {
                    dirty.set(true);
                }
                stays
            });
        }
        if apart && let Some((first, _)) = window {
            for item in composed.0.max(first)..composed.1.min(first_shown) {
                if !(limits.0..limits.1).contains(&item) && !detached.contains(&item) {
                    detached.push(item);
                }
            }
        }
    }

    /// The window after a layout that settled (no items missing): its `onVisibleItemsUpdated`, if
    /// what's in view changed, as Compose's small scrolls don't call it, then its `onScroll` with
    /// the scrolling since the last such layout, once past half a pixel and rounded, as Compose
    /// holds smaller scrolls back (`scrollToBeConsumed`). The window's bounds are read between the
    /// two, as `keepAroundItems`, by a layout Compose would measure too: not one only a scroll
    /// within what's in view or a request's composition asked for. `visible` are the units in view
    /// with their main-axis sizes, `key_of` gives a unit's key; `keys_changed`: the same units may
    /// have other keys.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn after_layout(
        &self,
        state: &Rc<LazyListInner>,
        scheduler: Option<&PrefetchScheduler>,
        laid: Option<&Laid>,
        count: usize,
        keys_changed: bool,
        units: Option<&dyn Units>,
        visible: &[(usize, i32)],
        key_of: &mut dyn FnMut(usize) -> u64,
    ) {
        let scrolled = self.delta.get();
        let delta = if scrolled.abs() > 0.5 { scrolled.round() } else { 0.0 };
        self.delta.set(scrolled - delta);
        if let Some(scheduler) = scheduler {
            *self.scheduler.borrow_mut() = Some(scheduler.clone());
        }
        if keys_changed {
            let mut detached = self.detached.borrow_mut();
            if !detached.is_empty() {
                detached.clear();
                self.detached_dirty.set(true);
            }
        }
        let scroll_asked = self.scroll_asked.replace(false);
        let by_request = self.measure_by_request.replace(false);
        let changed = {
            let mut shown = self.laid.borrow_mut();
            let count = count as i32;
            let same = match laid {
                Some(_) => {
                    shown.count == count
                        && !keys_changed
                        && shown.visible.len() == visible.len()
                        && shown
                            .visible
                            .iter()
                            .zip(visible)
                            .all(|(old, new)| old.0 == new.0 as i32 && old.1 == new.1)
                }
                None => shown.visible.is_empty() && shown.count == count,
            };
            shown.count = count;
            if !same {
                shown.visible.clear();
            }
            match laid {
                Some(laid) => {
                    shown.first = laid.first as i32;
                    shown.last = laid.last as i32;
                    shown.start_overflow = laid.start_overflow;
                    shown.end_overflow = laid.end_overflow;
                    shown.viewport = laid.viewport;
                    shown.density = laid.density;
                    if !same {
                        for &(unit, size) in visible {
                            shown.visible.push((unit as i32, size, key_of(unit)));
                        }
                    }
                    let sum: i32 = visible.iter().map(|(_, size)| *size).sum();
                    shown.average = sum / visible.len().max(1) as i32 + laid.spacing;
                }
                None => {
                    shown.visible.clear();
                    shown.average = 0;
                }
            }
            !same
        };
        let items = ItemUnits(count);
        let units = units.unwrap_or(&items);
        let shown = self.laid.borrow();
        let scope = WindowScope { state, prefetch: self, shown: &shown, units, scheduler };
        let mut window = self.window.borrow_mut();
        let Some(logic) = window.as_mut() else { return };
        if changed {
            logic.on_visible_items_updated(&scope);
        }
        if changed || (!scroll_asked && !by_request) {
            self.keep_bounds(logic);
        }
        if delta != 0.0 {
            logic.on_scroll(&scope, delta);
        }
        // D341's requests short of time wait for the next scroll (or, at rest, layout).
        let policy = self.policy.get();
        if policy.recent_maximum
            && (delta != 0.0 || (changed && policy.at_rest))
            && let Some(scheduler) = scheduler
        {
            for handle in logic.handles() {
                if !handle.queued.get() && !handle.done.get() && !handle.canceled.get() {
                    handle.queued.set(true);
                    scheduler.schedule_prefetch(Rc::new(HandleRequest(handle.clone())));
                }
            }
        }
    }

    /// A request is over. Its unit's last, done, tells the window, which requests the next.
    fn request_over(&self, state: &Rc<LazyListInner>, handle: &Handle) {
        let Some(lane) = handle.lane else { return };
        let Ok(mut window) = self.window.try_borrow_mut() else { return };
        let Some(logic) = window.as_mut() else { return };
        let unit = handle.unit as i32;
        // Its unit asked again since, or let it go: not this request's to speak for.
        if !logic.holds(unit, handle) {
            return;
        }
        if handle.canceled.get() {
            logic.forget(unit);
            return;
        }
        // A line's size is its last item's, as Compose's `maxOf(getMainAxisSize(it))` (D899).
        let mut size = None;
        let mut last_item = 0;
        for other in logic.handles().filter(|other| other.unit == handle.unit) {
            if !other.done.get() {
                return;
            }
            if size.is_none() || other.item >= last_item {
                last_item = other.item;
                size = Some(other.size.get());
            }
        }
        let Some(size) = size else { return };
        let scheduler = self.scheduler.borrow();
        let shown = self.laid.borrow();
        let stored = self.units.borrow();
        let items = ItemUnits(shown.count as usize);
        let units: &dyn Units = stored.as_deref().unwrap_or(&items);
        let scope = WindowScope {
            state,
            prefetch: self,
            shown: &shown,
            units,
            scheduler: scheduler.as_ref(),
        };
        logic.on_item_prefetched(&scope, lane, unit, size);
    }

    /// The items that stay composed, from those the layout wants and those `composed`: the
    /// wanted ones; those in the window (`window`, its first and past-its-last items), as Compose's
    /// `keepAroundItems`; with `next` (the end, backward the start, of the unit after the wanted
    /// ones the last scroll's way: an item, a grid's line), those composed next to them up to it
    /// (a layout's estimate may compose one more than it places, where Compose's composes as it
    /// measures: it is the one a scroll that way requests, and disposed it would be composed
    /// again); and the items of live requests next to those, composed by the request (Compose's
    /// precomposed items, kept until their request is cancelled) or already. Any other is let go:
    /// those behind, those further ahead, and without `next` (a focus search ended, D482) those a
    /// search laid out (D577).
    pub(super) fn target(
        &self,
        wanted: (usize, usize),
        composed: (usize, usize),
        window: Option<(usize, usize)>,
        next: Option<usize>,
    ) -> (usize, usize) {
        let (mut start, mut end) = wanted;
        if composed.0 <= end && composed.1 >= start {
            if let Some(window) = window {
                start = start.min(composed.0.max(window.0));
                end = end.max(composed.1.min(window.1));
            }
            if let Some(next) = next {
                if self.forward.get() {
                    end = end.max(composed.1.min(next));
                } else {
                    start = start.min(composed.0.max(next));
                }
            }
        }
        let logic = self.window.borrow();
        let Some(logic) = logic.as_ref() else { return (start, end) };
        let holds = |handle: &&Rc<Handle>| {
            !handle.canceled.get()
                && (handle.composed.get() || (composed.0..composed.1).contains(&handle.item))
        };
        loop {
            let mut grew = false;
            for handle in logic.handles().filter(holds) {
                if handle.item == end {
                    end += 1;
                    grew = true;
                } else if handle.item + 1 == start {
                    start -= 1;
                    grew = true;
                }
            }
            if !grew {
                return (start, end);
            }
        }
    }

    /// Asks for the items composed beyond `limits` to be deactivated between frames (kept for
    /// reuse): those the layout no longer wants stay composed, unplaced and undrawn, until then.
    /// Compose deactivates them in its measurement, which composes nothing; here leaving needs a
    /// composition, which a frame whose items are all composed doesn't run for them (D575).
    pub(super) fn trim_between_frames(
        &self,
        state: &Rc<LazyListInner>,
        scheduler: Option<&PrefetchScheduler>,
        composed: (usize, usize),
    ) {
        let (low, high) = self.limits.get();
        if composed.0 >= low && composed.1 <= high && !self.detached_dirty.get() {
            return;
        }
        let Some(scheduler) = scheduler else { return };
        if self.trim.borrow().upgrade().is_some() {
            return;
        }
        let request = Rc::new(TrimRequest { state: Rc::downgrade(state) });
        *self.trim.borrow_mut() = Rc::downgrade(&request);
        // First in the next idle time with any time left, whatever the queue holds: a request
        // waiting for a longer idle time, or one whose item is composed already (which
        // deactivates nothing), would leave them composed for good, and an animation in one
        // asking for frames (D575).
        scheduler.schedule_prefetch_first(request);
    }

    fn averages(&self, content_type: Option<u64>) -> Averages {
        self.metrics.borrow().get(&content_type).copied().unwrap_or_default()
    }

    /// Whether the item of `key`'s content is the layout's to measure: not a paused
    /// composition's not applied yet, nor a cancelled one's not removed yet (D592, D599).
    pub(super) fn content_ready(&self, key: u64) -> bool {
        self.unready_item() != Some(key)
    }

    /// A layout of the list starts: the item whose content isn't ready (`unready_item`), which
    /// it checks its children against. A completion's item noted before (`complete_pending`)
    /// is composed by now, so it's no longer the item composing: no inner scope restarted later
    /// notes its lists under it.
    pub(super) fn layout_started(&self) -> Option<u64> {
        self.composing_item.set(None);
        self.unready_item()
    }

    /// The item whose content isn't ready (`content_ready`), if any.
    fn unready_item(&self) -> Option<u64> {
        let mut unready = self.unready.borrow_mut();
        let (key, watch) = unready.as_ref()?;
        if watch.holds_unapplied_content() {
            return Some(*key);
        }
        *unready = None;
        None
    }

    /// Whether a request's paused composition was cancelled since the last layout asked: the
    /// layout has the items composed again (D594).
    pub(super) fn take_cancelled(&self) -> bool {
        self.cancelled.take()
    }

    /// A layout lacked an item whose content isn't ready: the paused composition pending, if
    /// any, is completed before the next pass (its round's), as Compose's measurement completes
    /// a precomposed item's (D592). A cancelled one's item composes its content in that pass.
    pub(super) fn complete_pending(&self) {
        let pending = self.pending.borrow();
        if let Some((key, paused)) = pending.as_ref()
            && let Some(paused) = paused.upgrade()
            && !paused.is_applied()
            && !paused.is_cancelled()
        {
            // Its nested lists note themselves under its key as the completion composes them.
            self.composing_item.set(Some(*key));
            paused.request_completion();
        }
    }

    /// A paused composition was applied or cancelled: none pending now.
    fn pending_done(&self) {
        *self.pending.borrow_mut() = None;
    }

    fn update_averages(&self, content_type: Option<u64>, update: impl FnOnce(&mut Averages)) {
        update(self.metrics.borrow_mut().entry(content_type).or_default());
    }

    /// This list's steps in idle time `idle_time` so far.
    fn idle_steps(&self, idle_time: u64) -> IdleSteps {
        let mut steps = self.idle.get();
        if steps.idle_time != idle_time {
            steps = IdleSteps { idle_time, first: true, missed: false, pace: Pace::EXPECTED };
            self.idle.set(steps);
        }
        steps
    }

    /// Whether the time left fits a `step` whose average is `average`: as Compose's
    /// `shouldExecute` (more time left than the average, or any for an urgent request), or with
    /// `PREFETCH_BUDGET_RECENT_MAXIMUM` D341's (the estimate at this idle time's pace and the
    /// margin; a miss may lower the estimate).
    fn fits(
        &self,
        scope: &PrefetchRequestScope<'_>,
        step: Step,
        average: u64,
        urgent: bool,
    ) -> bool {
        let available = scope.available_time_nanos();
        if !self.policy.get().recent_maximum {
            return available > if urgent { 0 } else { average };
        }
        let mut steps = self.idle_steps(scope.idle_time());
        // After a miss, the list's prefetch stops for this idle time (D341's request gave up
        // whole): another of its steps neither starts nor lowers an estimate again.
        if steps.missed {
            return false;
        }
        let cost = self.cost(step);
        if available > steps.pace.needed(cost.get().estimate()) {
            return true;
        }
        cost.set(cost.get().missed(steps.first));
        steps.missed = true;
        self.idle.set(steps);
        false
    }

    /// Whether a paused composition goes on (resumes, or passes a pause point) with `available`
    /// left: as Compose's (more time left than a stretch's and a pause's averages for the item's
    /// content type, any for an urgent request), or with `PREFETCH_BUDGET_RECENT_MAXIMUM` those
    /// averages at this idle time's pace and the margin: an outlier's stretch would hold every
    /// pause point back, and pausing too early only waits for the next frame (D596).
    fn goes_on(&self, available: u64, idle_time: u64, averages: &Averages, urgent: bool) -> bool {
        let stretch = averages.resume.saturating_add(averages.pause);
        if !self.policy.get().recent_maximum {
            return available > if urgent { 0 } else { stretch };
        }
        let steps = self.idle_steps(idle_time);
        !steps.missed && available > steps.pace.needed(stretch)
    }

    /// A paused composition didn't resume: with `PREFETCH_BUDGET_RECENT_MAXIMUM`, the list's
    /// steps stop for this idle time, as after a miss.
    fn resume_missed(&self, scope: &PrefetchRequestScope<'_>) {
        if self.policy.get().recent_maximum {
            let mut steps = self.idle_steps(scope.idle_time());
            steps.missed = true;
            self.idle.set(steps);
        }
    }

    /// A `step` of an item of `content_type` took `nanos`.
    fn timed(
        &self,
        scope: &PrefetchRequestScope<'_>,
        step: Step,
        content_type: Option<u64>,
        nanos: u64,
    ) {
        self.timed_in(scope.idle_time(), step, content_type, nanos);
    }

    /// `timed`, in idle time `idle_time`.
    fn timed_in(&self, idle_time: u64, step: Step, content_type: Option<u64>, nanos: u64) {
        self.update_averages(content_type, |averages| {
            let average = match step {
                Step::Compose => &mut averages.composition,
                Step::Resume => &mut averages.resume,
                Step::Pause => &mut averages.pause,
                Step::Apply => &mut averages.apply,
                Step::Measure => &mut averages.measure,
            };
            *average = running_average(*average, nanos);
        });
        // D341's estimates are of whole steps: a paused composition's stretches and pauses
        // learn Compose's averages only (its whole composition is timed when complete).
        if self.policy.get().recent_maximum && !matches!(step, Step::Resume | Step::Pause) {
            let mut steps = self.idle_steps(idle_time);
            let cost = self.cost(step);
            let estimate = cost.get().estimate();
            cost.set(cost.get().timed(nanos));
            steps.pace = steps.pace.after(nanos, estimate);
            self.idle.set(steps);
        }
    }

    /// A step of this list ran in `scope`'s idle time.
    fn stepped(&self, scope: &PrefetchRequestScope<'_>) {
        let mut steps = self.idle_steps(scope.idle_time());
        steps.first = false;
        self.idle.set(steps);
    }

    fn cost(&self, step: Step) -> &Cell<StepCost> {
        match step {
            // A paused composition's are timed whole.
            Step::Compose | Step::Resume | Step::Pause => &self.compose_cost,
            Step::Apply => &self.apply_cost,
            Step::Measure => &self.measure_cost,
        }
    }

    /// Composes `item` beside the composed ones, deactivating those beyond the limits on the
    /// other side in the same composition (those the layout let go), as a paused composition,
    /// returned not composed yet (its first resume runs the pass). `None` if it isn't next to them.
    fn compose_item(
        &self,
        state: &Rc<LazyListInner>,
        item: usize,
        key: u64,
        scope: &mut PrefetchRequestScope<'_>,
    ) -> Option<Composed> {
        let (start, end) = state.composed.get();
        if (start..end).contains(&item) || state.children.borrow().contains(&item) {
            // Composed already, unless a cancelled request's content was removed: composed
            // again then (its item composes it).
            if self.content_ready(key) {
                return Some(Composed::Already);
            }
            self.measure_by_request.set(true);
            state.compose((start, end));
            return Some(self.composed_by(scope, item_target(state, key)));
        }
        let (low, high) = self.limits.get();
        let range = if item == end {
            (start.max(low).min(item), item + 1)
        } else if item + 1 == start {
            (item, end.min(high).max(item + 1))
        } else if state.sticky_written.get().is_some_and(|header| item < header) {
            // Before a sticking header, away from the range (D895).
            self.detached.borrow_mut().push(item);
            (start, end)
        } else {
            return None;
        };
        {
            let mut precomposed = self.precomposed.borrow_mut();
            precomposed.retain(|kept| *kept != key);
            if precomposed.len() >= PRECOMPOSED_KEPT {
                precomposed.remove(0);
            }
            precomposed.push(key);
        }
        // What it composes is within the limits until the next layout sets them.
        if range != (start, end) {
            self.limits.set((low.min(item), high.max(item + 1)));
        }
        self.measure_by_request.set(true);
        state.compose(range);
        Some(self.composed_by(scope, item_target(state, key)))
    }

    /// Composes what the request's writes invalidated, as a paused composition of the item's
    /// content (`target`), as Compose's item subcomposition (other lists' items its pass composes
    /// are ordinary content, D597).
    fn composed_by(&self, scope: &mut PrefetchRequestScope<'_>, target: u64) -> Composed {
        Composed::Paused(Rc::new(scope.recompose_pausable_for(target)))
    }

    /// Whether the item of `key` was composed by a request and not measured since: leaving, it
    /// is kept for reuse. Forgotten, as asked when its item leaves.
    pub(super) fn take_precomposed(&self, key: u64) -> bool {
        let mut precomposed = self.precomposed.borrow_mut();
        let found = precomposed.iter().position(|kept| *kept == key);
        found.map(|position| precomposed.remove(position)).is_some()
    }

    /// The item of `key` was measured: it is a measured item from now on.
    fn measured(&self, key: u64) {
        self.precomposed.borrow_mut().retain(|kept| *kept != key);
    }

    /// Notes `nested`, a lazy list composing in the item of `key`.
    fn note(&self, nested: &Rc<LazyListInner>, key: u64) {
        let mut lists = self.nested.borrow_mut();
        if lists.len() > NESTED_KEYS_KEPT {
            lists.retain(|_, noted| noted.iter().any(|list| list.strong_count() > 0));
        }
        let noted = lists.entry(key).or_default();
        noted.retain(|list| list.strong_count() > 0);
        if !noted.iter().any(|list| std::ptr::eq(list.as_ptr(), Rc::as_ptr(nested))) {
            noted.push(Rc::downgrade(nested));
        }
    }

    /// The live lazy lists composed in the item of `key`.
    fn nested_lists(&self, key: u64) -> Vec<Rc<LazyListInner>> {
        self.nested
            .borrow()
            .get(&key)
            .map_or(Vec::new(), |noted| noted.iter().filter_map(Weak::upgrade).collect())
    }
}

/// A prefetch step, as Compose's averages time them.
#[derive(Clone, Copy)]
enum Step {
    /// A whole composition: what D341's budget estimates.
    Compose,
    /// A stretch of paused composition, up to a pause point.
    Resume,
    /// Stopping a paused composition at a pause point.
    Pause,
    Apply,
    Measure,
}

/// How `compose_item` composed the item.
enum Composed {
    /// Composed already (the layout's, or a request's).
    Already,
    /// In a paused composition, to resume until complete, then apply.
    Paused(Rc<PausedComposition>),
}

/// Compose's `Averages`: a content type's steps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Averages {
    /// A whole composition's (`PREFETCH_BUDGET_RECENT_MAXIMUM`'s gate to start one; Compose's has
    /// no such average at the tip).
    composition: u64,
    /// A stretch of paused composition's and a pause's (`resumeTimeNanos`, `pauseTimeNanos`).
    resume: u64,
    pause: u64,
    apply: u64,
    measure: u64,
    /// The smallest nested list's visible items, on average (`None`: none measured yet).
    nested_count: Option<usize>,
}

/// Compose's `calculateAverageTime`: 3/4 the current, 1/4 the new, each divided first; the first
/// sample taken as is.
fn running_average(current: u64, new: u64) -> u64 {
    if current == 0 { new } else { current / 4 * 3 + new / 4 }
}

/// Compose's `calculateAverageCount`.
fn running_count(current: Option<usize>, new: usize) -> usize {
    match current {
        None => new,
        Some(current) => (current * 3 + new) / 4,
    }
}

/// A request for one item, as Compose's `HandleAndRequestImpl`: compose it, prefetch the lazy
/// lists in it, then (a list's own item, not a nested list's) measure it.
pub(super) struct Handle {
    state: Weak<LazyListInner>,
    pub(super) item: usize,
    /// Its unit: the item, or a grid's line.
    unit: usize,
    /// The window's lane it's for (`None`: a nested list's, no window's).
    lane: Option<i32>,
    /// Its main-axis size, once measured.
    size: Cell<i32>,
    /// The item's key when requested (`keyUsedForComposition`): another item there ends it.
    /// The item's key: as of its last run until composed (`keyUsedForComposition`), then the one
    /// composed.
    key: Cell<u64>,
    key_composed: Cell<bool>,
    content_type: Cell<Option<u64>>,
    /// Measured after composing (a list's item); a nested list's are only composed
    /// (`schedulePrecomposition`).
    premeasure: bool,
    urgent: Cell<bool>,
    canceled: Cell<bool>,
    /// Composed: complete (Compose's `isComposed`: applied, or its paused composition complete).
    composed: Cell<bool>,
    /// Its paused composition, from its compose step until applied or cancelled.
    paused: RefCell<Option<Rc<PausedComposition>>>,
    /// The time its paused composition's resumes took so far.
    composition_nanos: Cell<u64>,
    measured: Cell<bool>,
    /// It has no work left.
    done: Cell<bool>,
    /// In the scheduler's queue.
    queued: Cell<bool>,
    /// The lazy lists composed in it, once resolved, and how far their requests went.
    nested: RefCell<Option<NestedPrefetch>>,
}

/// A request's nested prefetch (`NestedPrefetchController`): each nested list's requests, made
/// when it's reached, and the next one to run.
#[derive(Default)]
struct NestedPrefetch {
    lists: Vec<Rc<LazyListInner>>,
    requests: Vec<Option<Vec<Rc<Handle>>>>,
    list: usize,
    request: usize,
}

impl Handle {
    fn new(
        state: &Rc<LazyListInner>,
        items: &Items,
        item: usize,
        unit: usize,
        lane: Option<i32>,
        premeasure: bool,
    ) -> Handle {
        Handle {
            state: Rc::downgrade(state),
            item,
            unit,
            lane,
            size: Cell::new(0),
            key: Cell::new(items.key(item)),
            key_composed: Cell::new(false),
            content_type: Cell::new(items.content_type(item)),
            premeasure,
            urgent: Cell::new(false),
            canceled: Cell::new(false),
            composed: Cell::new(false),
            paused: RefCell::new(None),
            composition_nanos: Cell::new(0),
            measured: Cell::new(false),
            done: Cell::new(false),
            queued: Cell::new(false),
            nested: RefCell::new(None),
        }
    }

    /// Cancels the request, as its handle's `cancel`.
    pub(super) fn cancel(&self) {
        if self.canceled.replace(true) {
            return;
        }
        if let Some(state) = self.state.upgrade() {
            self.clean_up(&state.prefetch);
        }
    }

    /// It has no more to do.
    pub(super) fn done_or_over(&self) -> bool {
        self.done.get() || self.canceled.get()
    }

    /// As its handle's `markAsUrgent`.
    pub(super) fn mark_as_urgent(&self) {
        self.urgent.set(true);
    }

    /// `executeRequest`: the steps not done yet, each while it fits. Returns whether work is
    /// left (the request stays queued for the next idle time).
    fn execute(&self, scope: &mut PrefetchRequestScope<'_>) -> bool {
        let Some(state) = self.state.upgrade() else { return false };
        let prefetch = &state.prefetch;
        // The item's key and type now, as `executeRequest`: only a key composed binds the request.
        let current = prefetch.items.borrow().upgrade().and_then(|items| {
            (self.item < items.count).then(|| (items.key(self.item), items.content_type(self.item)))
        });
        let valid = current.is_some_and(|(key, content_type)| {
            if self.key_composed.get() {
                key == self.key.get()
            } else {
                self.key.set(key);
                self.content_type.set(content_type);
                true
            }
        });
        if self.canceled.get() || !valid {
            self.canceled.set(true);
            self.clean_up(prefetch);
            return false;
        }
        let urgent = self.urgent.get();
        let averages = prefetch.averages(self.content_type.get());
        // Compose's `resetAvailableTimeTo`: the steps' times count from here.
        let mark = Instant::now();
        // A paused composition a frame completed (the frame wanted its item): composed.
        if !self.composed.get()
            && self.paused.borrow().as_ref().is_some_and(|paused| paused.is_complete())
        {
            self.composed.set(true);
        }
        if !self.composed.get() {
            // Compose's gate: a stretch and a pause fit. With D341's budget, an item's
            // composition starts only if the whole of it fits (its estimate of whole
            // compositions), and a paused one resumes as Compose's (D596).
            let starting = self.paused.borrow().is_none();
            let fits = if starting && prefetch.policy.get().recent_maximum {
                prefetch.fits(scope, Step::Compose, averages.composition, urgent)
            } else {
                let available = scope.available_time_nanos();
                let goes_on = prefetch.goes_on(available, scope.idle_time(), &averages, urgent);
                if !goes_on {
                    prefetch.resume_missed(scope);
                }
                goes_on
            };
            if !fits {
                return self.short_of_time(&state);
            }
            let _section = compose_ui::platform::trace::section(c"compose-rs: prefetch compose");
            if !self.perform_pausable_composition(&state, scope, mark, urgent) {
                return false;
            }
            if !self.composed.get() {
                // Paused: it goes on after the next frame.
                return true;
            }
            // Another item there now (the items changed while composing): done, as Compose's
            // check of the key it composed.
            let same = prefetch.items.borrow().upgrade().is_some_and(|items| {
                self.item < items.count && items.key(self.item) == self.key.get()
            });
            if !same {
                self.canceled.set(true);
                self.clean_up(prefetch);
                return false;
            }
        }
        // Compose's apply step, gated by its own average.
        let paused = self.paused.borrow().clone();
        if paused.as_ref().is_some_and(|paused| paused.is_cancelled()) {
            // A pass discarded it (its item left): over.
            self.canceled.set(true);
            self.clean_up(prefetch);
            return false;
        }
        if let Some(paused) = paused {
            if !paused.is_applied() {
                if !paused.is_complete() {
                    // Invalidated since it completed: composed again first.
                    self.composed.set(false);
                    return true;
                }
                if !prefetch.fits(scope, Step::Apply, averages.apply, urgent) {
                    return self.short_of_time(&state);
                }
                let _section = compose_ui::platform::trace::section(c"compose-rs: prefetch apply");
                let began = Instant::now();
                paused.apply();
                prefetch.timed(
                    scope,
                    Step::Apply,
                    self.content_type.get(),
                    began.elapsed().as_nanos() as u64,
                );
                prefetch.stepped(scope);
            }
            *self.paused.borrow_mut() = None;
            prefetch.pending_done();
        }
        if self.nested.borrow().is_none() {
            // `resolveNestedPrefetchStates`: the lazy lists in the item, whether this request or
            // a frame composed it, as Compose's request finds those of its precomposed item.
            if scope.available_time_nanos() == 0 {
                return true;
            }
            *self.nested.borrow_mut() =
                Some(NestedPrefetch::of(prefetch.nested_lists(self.key.get())));
        }
        let nested_count = averages.nested_count.unwrap_or(INITIAL_NESTED_PREFETCH_COUNT);
        if self.execute_nested(scope, nested_count, urgent) {
            return true;
        }
        if self.premeasure && !self.measured.get() {
            if !prefetch.fits(scope, Step::Measure, averages.measure, urgent) {
                return self.short_of_time(&state);
            }
            let _section = compose_ui::platform::trace::section(c"compose-rs: prefetch measure");
            let start = Instant::now();
            let child = state.children.borrow().iter().position(|composed| *composed == self.item);
            let constraints_of = prefetch.constraints_of.borrow().clone();
            let constraints = match constraints_of {
                Some(constraints_of) => constraints_of(self.item),
                None => state.item_constraints.get(),
            };
            if let (Some(node), Some(child), Some(constraints)) =
                (prefetch.node.get(), child, constraints)
            {
                // What measuring it wrote (a nested list's items, a `BoxWithConstraints`'
                // constraints) is composed and measured too, as a frame's rounds do.
                for _ in 0..3 {
                    if let Some(size) = scope.premeasure(node, child, constraints) {
                        let vertical = prefetch.vertical.get();
                        self.size.set(if vertical { size.height } else { size.width });
                    }
                    if !scope.recompose() {
                        break;
                    }
                }
                // Timed only when it measured: an item gone in the meantime would lower the
                // estimate with a sample of nothing.
                prefetch.timed(
                    scope,
                    Step::Measure,
                    self.content_type.get(),
                    start.elapsed().as_nanos() as u64,
                );
            }
            prefetch.stepped(scope);
            self.measured.set(true);
            prefetch.measured(self.key.get());
            // The nested lists measured: how many items the smallest shows, for the next item of
            // this type; a measurement that composed more of them than were prefetched isn't one
            // to go by (`clearMeasureTime`).
            if let Some(nested) = &*self.nested.borrow()
                && !nested.lists.is_empty()
            {
                let ideal =
                    nested.lists.iter().map(|list| list.prefetch.visible.get()).min().unwrap_or(0);
                let prefetched = nested
                    .requests
                    .iter()
                    .map(|requests| requests.as_ref().map_or(0, Vec::len))
                    .min()
                    .unwrap_or(0);
                prefetch.update_averages(self.content_type.get(), |averages| {
                    averages.nested_count = Some(running_count(averages.nested_count, ideal));
                    if prefetched < ideal {
                        averages.measure = 0;
                    }
                });
            }
        }
        self.done.set(true);
        false
    }

    /// Compose's `performPausableComposition`: composes the item in a paused composition (made
    /// at the first call), resumed while it isn't complete and no pause was asked for; at each
    /// pause point, the time since the last is a stretch's sample, and it pauses once the time
    /// left is no more than a stretch's and a pause's averages (`goes_on`); the time to stop is
    /// a pause's sample (the last stretch's, if it completed). Returns false if the request is
    /// over (its item no longer next to the composed ones).
    fn perform_pausable_composition(
        &self,
        state: &Rc<LazyListInner>,
        scope: &mut PrefetchRequestScope<'_>,
        mark: Instant,
        urgent: bool,
    ) -> bool {
        let prefetch = &state.prefetch;
        let existing = self.paused.borrow().clone();
        let paused = match existing {
            Some(paused) => paused,
            None => match prefetch.compose_item(state, self.item, self.key.get(), scope) {
                None => {
                    self.canceled.set(true);
                    return false;
                }
                Some(Composed::Paused(paused)) => {
                    self.key_composed.set(true);
                    let key = self.key.get();
                    *prefetch.pending.borrow_mut() = Some((key, Rc::downgrade(&paused)));
                    *prefetch.unready.borrow_mut() = Some((key, paused.watch()));
                    *self.paused.borrow_mut() = Some(paused.clone());
                    paused
                }
                Some(Composed::Already) => {
                    self.key_composed.set(true);
                    self.composed.set(true);
                    return true;
                }
            },
        };
        if paused.is_cancelled() {
            // A pass discarded it (its item left, or was deactivated): over.
            self.canceled.set(true);
            self.clean_up(prefetch);
            return false;
        }
        let timing = Rc::new(PauseTiming {
            state: Rc::downgrade(state),
            content_type: self.content_type.get(),
            urgent,
            deadline: scope.deadline(),
            idle_time: scope.idle_time(),
            mark: Cell::new(mark),
            requested: Cell::new(false),
        });
        // The lists composing in the item note themselves under its key.
        let composing = prefetch.composing_item.replace(Some(self.key.get()));
        while !paused.is_complete() && !paused.is_cancelled() && !timing.requested.get() {
            let asked = timing.clone();
            let mark = timing.mark.get();
            // A resume that composed nothing (no pass could run: the composition is applying
            // a pass's end) is a pause: the next idle time tries again.
            if !paused.resume(move || asked.should_pause()) && timing.mark.get() == mark {
                timing.requested.set(true);
            }
        }
        prefetch.composing_item.set(composing);
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(timing.mark.get()).as_nanos() as u64;
        let step = if timing.requested.get() { Step::Pause } else { Step::Resume };
        prefetch.timed(scope, step, self.content_type.get(), elapsed);
        prefetch.stepped(scope);
        let complete = paused.is_complete();
        self.composed.set(complete);
        // D341's estimate of whole compositions: this one's resumes, summed.
        let total =
            self.composition_nanos.get() + now.saturating_duration_since(mark).as_nanos() as u64;
        self.composition_nanos.set(total);
        if complete && prefetch.policy.get().recent_maximum {
            prefetch.compose_cost.set(prefetch.compose_cost.get().timed(total));
        }
        true
    }

    /// Compose's `cleanUp`: a paused composition not applied is cancelled (its content
    /// removed, before the composition's next pass if a layout runs now); its item composes its
    /// content again when composed next.
    pub(super) fn clean_up(&self, prefetch: &Prefetch) {
        // Its nested lists' requests too: one of them may hold a paused composition.
        if let Some(nested) = self.nested.borrow_mut().take() {
            for (list, requests) in nested.lists.iter().zip(nested.requests) {
                for request in requests.into_iter().flatten() {
                    request.clean_up(&list.prefetch);
                }
            }
        }
        let Some(paused) = self.paused.borrow_mut().take() else { return };
        prefetch.pending_done();
        if !paused.is_applied() && !paused.is_cancelled() {
            paused.cancel();
            prefetch.cancelled.set(true);
        }
    }

    /// `executeNestedPrefetches`: each nested list's first `count` items from its first visible
    /// one, composed one request at a time, while they fit. Returns whether work is left.
    fn execute_nested(
        &self,
        scope: &mut PrefetchRequestScope<'_>,
        count: usize,
        urgent: bool,
    ) -> bool {
        loop {
            let (list, request) = {
                let mut nested = self.nested.borrow_mut();
                let Some(nested) = nested.as_mut() else { return false };
                if nested.list >= nested.lists.len() {
                    return false;
                }
                let list = nested.lists[nested.list].clone();
                if nested.requests[nested.list].is_none() {
                    if scope.available_time_nanos() == 0 {
                        return true;
                    }
                    let first = list.position.get().0;
                    let requests =
                        list.prefetch.items.borrow().upgrade().map_or(Vec::new(), |items| {
                            (first..first + count)
                                .map(|item| {
                                    Rc::new(Handle::new(&list, &items, item, item, None, false))
                                })
                                .collect()
                        });
                    nested.requests[nested.list] = Some(requests);
                }
                let requests = nested.requests[nested.list].as_ref().expect("made");
                if nested.request >= requests.len() {
                    nested.list += 1;
                    nested.request = 0;
                    continue;
                }
                (list, requests[nested.request].clone())
            };
            drop(list);
            if urgent {
                request.urgent.set(true);
            }
            if request.execute(scope) {
                return true;
            }
            if let Some(nested) = self.nested.borrow_mut().as_mut() {
                nested.request += 1;
            }
        }
    }

    /// A step didn't fit: Compose's request stays queued for the next idle time; with
    /// `PREFETCH_BUDGET_RECENT_MAXIMUM` it gives up until the next scroll (D156, D341), which
    /// queues it again.
    fn short_of_time(&self, state: &LazyListInner) -> bool {
        // A request holding a paused composition stays queued all the same: the composition's
        // next paused composition, a request's behind it, waits for it to be applied (D596).
        if state.prefetch.policy.get().recent_maximum && self.paused.borrow().is_none() {
            self.queued.set(false);
            return false;
        }
        true
    }
}

impl NestedPrefetch {
    /// Over the lazy lists composed in an item.
    fn of(lists: Vec<Rc<LazyListInner>>) -> NestedPrefetch {
        let requests = lists.iter().map(|_| None).collect();
        NestedPrefetch { lists, requests, list: 0, request: 0 }
    }
}

/// What a paused composition's callback works from: Compose's `performPausableComposition`
/// lambda's.
struct PauseTiming {
    state: Weak<LazyListInner>,
    content_type: Option<u64>,
    urgent: bool,
    deadline: Instant,
    idle_time: u64,
    /// The last pause point (Compose's `startTime` after `updateElapsedAndAvailableTime`).
    mark: Cell<Instant>,
    /// A pause was asked for (`pauseRequested`).
    requested: Cell<bool>,
}

impl PauseTiming {
    /// At a pause point: the stretch since the last one is a sample, and it pauses once the
    /// time left doesn't fit another stretch and a pause.
    fn should_pause(&self) -> bool {
        if self.requested.get() {
            return true;
        }
        let Some(state) = self.state.upgrade() else { return true };
        let prefetch = &state.prefetch;
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.mark.get()).as_nanos() as u64;
        self.mark.set(now);
        prefetch.timed_in(self.idle_time, Step::Resume, self.content_type, elapsed);
        let available = self.deadline.saturating_duration_since(now).as_nanos() as u64;
        let averages = prefetch.averages(self.content_type);
        let pause = !prefetch.goes_on(available, self.idle_time, &averages, self.urgent);
        self.requested.set(pause);
        pause
    }
}

/// A [`Handle`] in the scheduler's queue.
struct HandleRequest(Rc<Handle>);

impl PrefetchRequest for HandleRequest {
    fn execute(&self, scope: &mut PrefetchRequestScope<'_>) -> bool {
        let handle = &self.0;
        let more = handle.execute(scope);
        if !more {
            handle.queued.set(false);
            if (handle.done.get() || handle.canceled.get())
                && let Some(state) = handle.state.upgrade()
            {
                state.prefetch.request_over(&state, handle);
            }
        }
        more
    }
}

/// Deactivates between frames the items composed beyond the limits the last layout set.
struct TrimRequest {
    state: Weak<LazyListInner>,
}

impl PrefetchRequest for TrimRequest {
    fn execute(&self, scope: &mut PrefetchRequestScope<'_>) -> bool {
        let Some(state) = self.state.upgrade() else { return false };
        let prefetch = &state.prefetch;
        let (start, end) = state.composed.get();
        let (low, high) = prefetch.limits.get();
        let (wanted_start, wanted_end) = prefetch.wanted.get();
        if start > wanted_start || end < wanted_end {
            // The frame composes the wanted ones (and trims).
            return false;
        }
        let range = (start.max(low), end.min(high));
        if (range == (start, end) && !prefetch.detached_dirty.get()) || range.0 >= range.1 {
            return false;
        }
        // Compose deactivates in its measurement, so any time left goes; D341's budget times it
        // as a composition.
        let recent_maximum = prefetch.policy.get().recent_maximum;
        let fits = if recent_maximum {
            prefetch.fits(scope, Step::Compose, prefetch.averages(None).composition, false)
        } else {
            scope.available_time_nanos() > 0
        };
        if !fits {
            return !recent_maximum;
        }
        let _section = compose_ui::platform::trace::section(c"compose-rs: prefetch compose");
        prefetch.measure_by_request.set(true);
        state.compose(range);
        scope.recompose();
        prefetch.stepped(scope);
        false
    }
}

/// The target of the item of `key`'s content in `list` (`PausableContentFor`): the request's
/// paused composition pauses it, no other list's item.
pub(super) fn item_target(list: &LazyListInner, key: u64) -> u64 {
    let address = list as *const LazyListInner as u64;
    key.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ address.rotate_left(17)
}

/// The lazy list whose items are composing, for the lazy lists in them to note themselves in
/// under the item composing (`composing_item`; Compose finds them by `traversablePrefetchState`).
/// Kept by the list's composition, and given its state's list at each (a list given another
/// state notes its nested lists in the new one).
pub(super) struct ParentList {
    list: RefCell<Weak<LazyListInner>>,
}

impl ParentList {
    pub(super) fn new(list: &Rc<LazyListInner>) -> ParentList {
        ParentList { list: RefCell::new(Rc::downgrade(list)) }
    }

    /// The list's composition: `list` is its state's now.
    pub(super) fn set(&self, list: &Rc<LazyListInner>) {
        let mut current = self.list.borrow_mut();
        if !std::ptr::eq(current.as_ptr(), Rc::as_ptr(list)) {
            *current = Rc::downgrade(list);
        }
    }
}

pub(super) static LOCAL_PARENT_LIST: compose_runtime::CompositionLocal<Option<Rc<ParentList>>> =
    compose_runtime::CompositionLocal::new("LocalParentLazyList", || None);

/// A lazy list composing: notes `state` in the list around it, under the item composing it.
pub(super) fn note_nested(state: &Rc<LazyListInner>) {
    if let Some(item) = LOCAL_PARENT_LIST.current()
        && let Some(parent) = item.list.borrow().upgrade()
        && !Rc::ptr_eq(&parent, state)
        && let Some(key) = parent.prefetch.composing_item.get()
    {
        parent.prefetch.note(state, key);
    }
}

/// This list's steps in an idle time (`PREFETCH_BUDGET_RECENT_MAXIMUM`): whether none ran yet,
/// and their pace (D381).
#[derive(Clone, Copy, Debug)]
struct IdleSteps {
    idle_time: u64,
    first: bool,
    /// A step of the list's didn't fit this idle time.
    missed: bool,
    pace: Pace,
}

impl Default for IdleSteps {
    fn default() -> IdleSteps {
        IdleSteps { idle_time: 0, first: true, missed: false, pace: Pace::EXPECTED }
    }
}

/// How long a prefetch step (composing an item, or measuring it) takes, in nanoseconds, with
/// `PREFETCH_BUDGET_RECENT_MAXIMUM`: a running average (3/4 old + 1/4 new, as Compose's prefetch
/// averages) and the recent maximum, which loses an eighth with each step timed. A step is
/// expected to take the larger: one item much slower than the average (a cold text layout, a
/// little core) means the next may be too, and starting it on the average's word overruns the
/// next frame (D341).
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(super) struct StepCost {
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

#[cfg(test)]
mod tests {
    use super::{PREFETCH_MARGIN_NANOS, Pace, StepCost, running_average, running_count};

    const MS: u64 = 1_000_000;

    #[test]
    fn composes_averages_divide_first_and_take_the_first_sample_as_is() {
        assert_eq!(running_average(0, 4_000_001), 4_000_001);
        // 4_000_001 / 4 * 3 + 2_000_003 / 4, each divided first.
        assert_eq!(running_average(4_000_001, 2_000_003), 3_000_000 + 500_000);
        assert_eq!(running_average(7, 7), 3 + 1);
        assert_eq!(running_count(None, 3), 3);
        assert_eq!(running_count(Some(2), 4), 2);
        assert_eq!(running_count(Some(3), 3), 3);
    }

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
