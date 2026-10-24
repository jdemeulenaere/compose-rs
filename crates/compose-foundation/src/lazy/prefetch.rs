//! Prefetch: what a lazy list (or grid) composes and measures between frames, as Compose
//! 1.12.1's default `LazyListPrefetchStrategy` (a grid's `LazyGridPrefetchStrategy`) and
//! `LazyLayoutPrefetchState` (docs/15, "Compose's defaults" (a), (b) and (f)):
//!
//! - after a layout that scrolled, a request for the item (a grid's line: a request per item)
//!   next to those in view in the scroll direction, none behind; cancelled when the direction
//!   turns, or once the item it holds isn't the next one any more; urgent when the same scroll
//!   again would reach it (`lastItem.offset + size + spacing - viewportEndOffset < -delta`);
//! - a request composes its item, then the first items of the lazy lists in it (two, then the
//!   running average of the smallest nested list's visible count, per content type), composing
//!   only, then measures it: nothing places or draws it until a frame wants it;
//! - each step runs only while the time left exceeds that step's running average for the item's
//!   content type (`current / 4 * 3 + new / 4`, in integer nanoseconds; the first sample taken
//!   as is), or any time left for an urgent request; a step that doesn't fit leaves the request
//!   queued, to go on after the next frame.
//!
//! The composed items are those in view and those requests composed next to them: a frame's
//! layout deactivates the others (kept for reuse), as Compose's measurement does.
//!
//! compose-rs has no pausable composition: a request's composition is one step, where Compose
//! pauses a composition that runs out of time and resumes it after the next frame.
//!
//! compose-rs's own behaviours are optimizations of the host's (D560-D565): two items ahead
//! (`PREFETCH_TWO_AHEAD`), D341/D381's estimate (`PREFETCH_BUDGET_RECENT_MAXIMUM`), prefetching
//! at rest (`PREFETCH_AT_REST`), and two items kept behind, disposed between frames
//! (`KEEP_ITEMS_BEHIND`).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Range;
use std::rc::{Rc, Weak};
use std::time::Instant;

use compose_runtime::ComposeArg;
use compose_ui::platform::{NodeId, Optimization, Optimizations};
use compose_ui::{Constraints, PrefetchRequest, PrefetchRequestScope, PrefetchScheduler};

use super::{Items, LazyListInner};

/// Items kept composed behind those the layout wants, the other way from the scroll, with
/// `KEEP_ITEMS_BEHIND`: a scroll reversing direction finds the first items it wants composed
/// (D375).
pub(super) const KEPT_BEHIND: usize = 2;

/// What a prefetch step leaves of the idle time beyond its estimate, at least, with
/// `PREFETCH_BUDGET_RECENT_MAXIMUM`: the estimate is a guess, and a step running past the
/// deadline delays the next frame (D341).
pub(super) const PREFETCH_MARGIN_NANOS: u64 = 500_000;

/// The items of a nested list a request composes before its first layout of that content type,
/// as Compose's `nestedPrefetchItemCount` (2).
const INITIAL_NESTED_PREFETCH_COUNT: usize = 2;

/// Which of compose-rs's own prefetch behaviours a list uses, from the host's optimizations.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct Policy {
    /// Items (a grid's lines) prefetched ahead: one, as Compose's; two with
    /// `PREFETCH_TWO_AHEAD` (D156).
    pub(super) ahead: usize,
    /// D341 and D381's budget (`PREFETCH_BUDGET_RECENT_MAXIMUM`).
    pub(super) recent_maximum: bool,
    /// Items (a grid's lines) kept composed behind the scroll (`KEEP_ITEMS_BEHIND`, D375).
    pub(super) keep_behind: usize,
    /// Prefetch after a layout at rest too (`PREFETCH_AT_REST`, D470).
    pub(super) at_rest: bool,
}

impl Policy {
    /// Compose's.
    pub(super) const COMPOSE: Policy =
        Policy { ahead: 1, recent_maximum: false, keep_behind: 0, at_rest: false };

    pub(super) fn of(optimizations: Optimizations) -> Policy {
        Policy {
            ahead: if optimizations.contains(Optimization::PREFETCH_TWO_AHEAD) { 2 } else { 1 },
            recent_maximum: optimizations.contains(Optimization::PREFETCH_BUDGET_RECENT_MAXIMUM),
            keep_behind: if optimizations.contains(Optimization::KEEP_ITEMS_BEHIND) {
                KEPT_BEHIND
            } else {
                0
            },
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
    /// Scrolled since the last layout that settled, in Compose's sign: negative towards the end.
    pub(super) delta: Cell<f32>,
    /// The items composed may span, set by each layout: with `KEEP_ITEMS_BEHIND`, from the
    /// first kept behind the wanted ones to the last prefetched ahead (a frame and prefetch trim
    /// to them); else the wanted items and those requests composed next to them.
    pub(super) limits: Cell<(usize, usize)>,
    /// Each item's constraints, where they differ by item (a grid's spans); else the list's
    /// `item_constraints`.
    pub(super) constraints_of: RefCell<Option<ItemConstraints>>,
    /// The host's prefetch behaviours, as last laid out.
    pub(super) policy: Cell<Policy>,
    /// The items, as last composed: their keys and content types.
    pub(super) items: RefCell<Weak<Items>>,
    /// Items in view at the last layout: a parent's nested prefetch learns its count from it
    /// (Compose's `idealNestedPrefetchCount`).
    pub(super) visible: Cell<usize>,
    strategy: Strategy,
    /// Compose's `PrefetchMetrics`: the steps' averages by content type.
    metrics: RefCell<HashMap<Option<u64>, Averages>>,
    /// D341's estimates (`PREFETCH_BUDGET_RECENT_MAXIMUM`), one for all content types.
    compose_cost: Cell<StepCost>,
    measure_cost: Cell<StepCost>,
    /// This list's steps in the idle time under way (`PREFETCH_BUDGET_RECENT_MAXIMUM`).
    idle: Cell<IdleSteps>,
    /// A request composes an item: the lazy lists composed in it note themselves in `nested`.
    pub(super) collecting: Cell<bool>,
    pub(super) nested: RefCell<Vec<Weak<LazyListInner>>>,
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
            limits: Cell::new((0, 0)),
            constraints_of: RefCell::new(None),
            policy: Cell::new(Policy::COMPOSE),
            items: RefCell::new(Weak::new()),
            visible: Cell::new(0),
            strategy: Strategy::default(),
            metrics: RefCell::default(),
            compose_cost: Cell::default(),
            measure_cost: Cell::default(),
            idle: Cell::default(),
            collecting: Cell::new(false),
            nested: RefCell::default(),
            trim: RefCell::new(Weak::new()),
        }
    }
}

/// Compose's strategy's state: the item (a grid's line) its requests are for, the way the
/// list last scrolled, and what the last layout saw.
#[derive(Default)]
struct Strategy {
    /// `indexToPrefetch` (a grid's `lineToPrefetch`): the nearest unit requested.
    target: Cell<Option<usize>>,
    /// `wasScrollingForward`.
    was_forward: Cell<bool>,
    /// `previousPassItemCount` and `previousPassDelta`.
    previous_count: Cell<Option<usize>>,
    previous_delta: Cell<f32>,
    /// The requests held (`currentPrefetchHandle`, a grid's line's), each for one item.
    handles: RefCell<Vec<Rc<Handle>>>,
}

/// What a layout that settled shows, for the strategy: the first and last units in view (items;
/// a grid's lines), their first and last items, and how far the first starts before the
/// viewport (`viewportStartOffset - firstItem.offset`) and the last ends, with its spacing,
/// past it (`lastItem.offset + lastItem.size + spacing - viewportEndOffset`).
pub(super) struct Laid {
    pub(super) first: usize,
    pub(super) last: usize,
    pub(super) first_item: usize,
    pub(super) last_item: usize,
    pub(super) start_distance: i32,
    pub(super) end_distance: i32,
}

impl Laid {
    /// `calculateIndexToPrefetch` (a grid's `calculateLineIndexToPrefetch`): the unit after the
    /// last in view, or before the first (`None`: before the list's start).
    fn unit_to_prefetch(&self, forward: bool) -> Option<usize> {
        if forward { Some(self.last + 1) } else { self.first.checked_sub(1) }
    }

    /// Compose's check before requesting: the item next to those in view that way exists.
    fn next_item_exists(&self, forward: bool, count: usize) -> bool {
        if forward { self.last_item + 1 < count } else { self.first_item > 0 }
    }
}

/// Whether `unit` is among the `ahead` units from `nearest` the `forward` way.
fn in_window(unit: usize, nearest: usize, forward: bool, ahead: usize) -> bool {
    if forward {
        unit >= nearest && unit < nearest + ahead
    } else {
        unit <= nearest && unit + ahead > nearest
    }
}

impl Prefetch {
    /// Compose's strategy after a layout that settled (no items missing): its
    /// `onVisibleItemsUpdated`, then its `onScroll` with the scrolling since the last such
    /// layout. `items_of` gives a unit's items (a list's: itself; a grid's line's).
    pub(super) fn after_layout(
        &self,
        state: &Rc<LazyListInner>,
        scheduler: Option<&PrefetchScheduler>,
        laid: Option<&Laid>,
        count: usize,
        items_of: &mut dyn FnMut(usize) -> Range<usize>,
    ) {
        let policy = self.policy.get();
        let strategy = &self.strategy;
        let delta = self.delta.replace(0.0);
        // A request for anything but the next unit the last scroll's way is cancelled (with two
        // ahead, those out of the next two), as `evaluatePrefetchForCancellation`.
        if let (Some(target), Some(laid)) = (strategy.target.get(), laid) {
            let forward = strategy.was_forward.get();
            let expected = laid.unit_to_prefetch(forward);
            if expected != Some(target) {
                strategy.target.set(None);
                self.retain_handles(|unit| {
                    expected.is_some_and(|nearest| in_window(unit, nearest, forward, policy.ahead))
                });
            }
        }
        // Items came or went after a scroll: the next unit the last scroll's way.
        if let Some(previous) = strategy.previous_count.get()
            && strategy.previous_delta.get() != 0.0
            && previous != count
            && let Some(laid) = laid
        {
            let forward = strategy.previous_delta.get() < 0.0;
            if let Some(unit) = laid.unit_to_prefetch(forward)
                && laid.next_item_exists(forward, count)
                && strategy.target.get() != Some(unit)
            {
                strategy.target.set(Some(unit));
                self.request(state, scheduler, unit, forward, count, items_of);
            }
        }
        strategy.previous_count.set(Some(count));
        if delta != 0.0 {
            if let Some(laid) = laid {
                let forward = delta < 0.0;
                if let Some(unit) = laid.unit_to_prefetch(forward)
                    && laid.next_item_exists(forward, count)
                {
                    if strategy.target.get() != Some(unit) {
                        // The way turned: what was requested won't be reached soon.
                        if strategy.was_forward.get() != forward {
                            self.retain_handles(|_| false);
                        }
                        strategy.was_forward.set(forward);
                        strategy.target.set(Some(unit));
                        self.request(state, scheduler, unit, forward, count, items_of);
                    }
                    // The same scroll again would reach it: any time left is worth it.
                    let urgent = if forward {
                        (laid.end_distance as f32) < -delta
                    } else {
                        (laid.start_distance as f32) < delta
                    };
                    if urgent {
                        for handle in strategy.handles.borrow().iter() {
                            if handle.unit == unit {
                                handle.urgent.set(true);
                            }
                        }
                    }
                }
            }
            strategy.previous_delta.set(delta);
        } else if policy.at_rest
            && strategy.handles.borrow().is_empty()
            && let Some(laid) = laid
        {
            let forward = self.forward.get();
            if let Some(unit) = laid.unit_to_prefetch(forward)
                && laid.next_item_exists(forward, count)
            {
                strategy.was_forward.set(forward);
                strategy.target.set(Some(unit));
                self.request(state, scheduler, unit, forward, count, items_of);
            }
        }
        // D341's requests short of time wait for the next scroll (or, at rest, layout).
        if policy.recent_maximum
            && (delta != 0.0 || policy.at_rest)
            && let Some(scheduler) = scheduler
        {
            for handle in strategy.handles.borrow().iter() {
                if !handle.queued.get() && !handle.done.get() && !handle.canceled.get() {
                    handle.queued.set(true);
                    scheduler.schedule_prefetch(Rc::new(HandleRequest(handle.clone())));
                }
            }
        }
    }

    /// Requests the items of `unit` and of the units after it the `forward` way, `ahead` in
    /// all, those not requested already, as `schedulePrefetch`.
    fn request(
        &self,
        state: &Rc<LazyListInner>,
        scheduler: Option<&PrefetchScheduler>,
        unit: usize,
        forward: bool,
        count: usize,
        items_of: &mut dyn FnMut(usize) -> Range<usize>,
    ) {
        let Some(scheduler) = scheduler else { return };
        let Some(items) = self.items.borrow().upgrade() else { return };
        let ahead = self.policy.get().ahead;
        for step in 0..ahead {
            let Some(unit) =
                (if forward { unit.checked_add(step) } else { unit.checked_sub(step) })
            else {
                break;
            };
            for item in items_of(unit) {
                if item >= count
                    || self.strategy.handles.borrow().iter().any(|handle| handle.item == item)
                {
                    continue;
                }
                let handle = Rc::new(Handle::new(state, &items, item, unit, true));
                handle.queued.set(true);
                self.strategy.handles.borrow_mut().push(handle.clone());
                scheduler.schedule_prefetch(Rc::new(HandleRequest(handle)));
            }
        }
    }

    /// Cancels the requests whose unit `keep` doesn't keep, as `resetPrefetchState`.
    fn retain_handles(&self, keep: impl Fn(usize) -> bool) {
        self.strategy.handles.borrow_mut().retain(|handle| {
            let kept = keep(handle.unit);
            if !kept {
                handle.canceled.set(true);
            }
            kept
        });
    }

    /// The items the layout composes, from those it wants and those `composed`: the wanted
    /// ones, the items live requests composed next to them (Compose's precomposed items, kept
    /// until their request is cancelled), and those composed ahead of them in the scroll
    /// direction. Compose's measurement composes the items it places, one at a time; a
    /// measurement here asks for the items it lacks by an estimate of their size, so it may
    /// compose one more than it places: kept, that one is the next item (disposed, it would be
    /// composed again when it comes). Those behind are deactivated in the frame, as Compose's.
    pub(super) fn target(
        &self,
        wanted: (usize, usize),
        composed: (usize, usize),
    ) -> (usize, usize) {
        let handles = self.strategy.handles.borrow();
        let (mut start, mut end) = wanted;
        let overlaps = composed.0 <= end && composed.1 >= start;
        if overlaps && self.forward.get() {
            end = end.max(composed.1);
        } else if overlaps {
            start = start.min(composed.0);
        }
        loop {
            let mut grew = false;
            for handle in
                handles.iter().filter(|handle| handle.composed.get() && !handle.canceled.get())
            {
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

    /// With `KEEP_ITEMS_BEHIND`, asks for the items composed beyond `limits` behind the scroll
    /// to be disposed between frames, unless a request will (its composition trims them).
    pub(super) fn trim_between_frames(
        &self,
        state: &Rc<LazyListInner>,
        scheduler: Option<&PrefetchScheduler>,
        composed: (usize, usize),
    ) {
        let (low, high) = self.limits.get();
        // Behind the scroll only: those ahead are the next ones (D375).
        let beyond = if self.forward.get() { composed.0 < low } else { composed.1 > high };
        if !beyond {
            return;
        }
        let Some(scheduler) = scheduler else { return };
        let composing =
            self.strategy.handles.borrow().iter().any(|handle| {
                handle.queued.get() && !handle.composed.get() && !handle.canceled.get()
            });
        if composing || self.trim.borrow().upgrade().is_some() {
            return;
        }
        let request = Rc::new(TrimRequest { state: Rc::downgrade(state) });
        *self.trim.borrow_mut() = Rc::downgrade(&request);
        scheduler.schedule_prefetch(request);
    }

    fn averages(&self, content_type: Option<u64>) -> Averages {
        self.metrics.borrow().get(&content_type).copied().unwrap_or_default()
    }

    fn update_averages(&self, content_type: Option<u64>, update: impl FnOnce(&mut Averages)) {
        update(self.metrics.borrow_mut().entry(content_type).or_default());
    }

    /// This list's steps in `scope`'s idle time so far.
    fn idle_steps(&self, scope: &PrefetchRequestScope<'_>) -> IdleSteps {
        let mut steps = self.idle.get();
        if steps.idle_time != scope.idle_time() {
            steps = IdleSteps { idle_time: scope.idle_time(), first: true, pace: Pace::EXPECTED };
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
        let steps = self.idle_steps(scope);
        let cost = self.cost(step);
        if available > steps.pace.needed(cost.get().estimate()) {
            return true;
        }
        cost.set(cost.get().missed(steps.first));
        false
    }

    /// A `step` of an item of `content_type` took `nanos`.
    fn timed(
        &self,
        scope: &PrefetchRequestScope<'_>,
        step: Step,
        content_type: Option<u64>,
        nanos: u64,
    ) {
        self.update_averages(content_type, |averages| match step {
            Step::Compose => averages.composition = running_average(averages.composition, nanos),
            Step::Measure => averages.measure = running_average(averages.measure, nanos),
        });
        if self.policy.get().recent_maximum {
            let mut steps = self.idle_steps(scope);
            let cost = self.cost(step);
            let estimate = cost.get().estimate();
            cost.set(cost.get().timed(nanos));
            steps.pace = steps.pace.after(nanos, estimate);
            self.idle.set(steps);
        }
    }

    /// A step of this list ran in `scope`'s idle time.
    fn stepped(&self, scope: &PrefetchRequestScope<'_>) {
        let mut steps = self.idle_steps(scope);
        steps.first = false;
        self.idle.set(steps);
    }

    fn cost(&self, step: Step) -> &Cell<StepCost> {
        match step {
            Step::Compose => &self.compose_cost,
            Step::Measure => &self.measure_cost,
        }
    }

    /// Composes `item` beside the composed ones (with `KEEP_ITEMS_BEHIND`, trimming those
    /// beyond the limits), noting the lazy lists composed in it. `None` if it isn't next to
    /// them; else whether it composed anything.
    fn compose_item(
        &self,
        state: &Rc<LazyListInner>,
        item: usize,
        scope: &mut PrefetchRequestScope<'_>,
    ) -> Option<bool> {
        let (start, end) = state.composed.get();
        if (start..end).contains(&item) {
            return Some(false);
        }
        let (low, high) =
            if self.policy.get().keep_behind > 0 { self.limits.get() } else { (0, usize::MAX) };
        let range = if item == end {
            (start.max(low).min(item), item + 1)
        } else if item + 1 == start {
            (item, end.min(high).max(item + 1))
        } else {
            return None;
        };
        self.collecting.set(true);
        self.nested.borrow_mut().clear();
        state.compose(range);
        scope.recompose();
        self.collecting.set(false);
        Some(true)
    }
}

/// A prefetch step, as Compose's averages time them.
#[derive(Clone, Copy)]
enum Step {
    Compose,
    Measure,
}

/// Compose's `Averages`: a content type's steps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Averages {
    composition: u64,
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
    /// The item's key when requested (`keyUsedForComposition`): another item there ends it.
    key: u64,
    content_type: Option<u64>,
    /// Measured after composing (a list's item); a nested list's are only composed
    /// (`schedulePrecomposition`).
    premeasure: bool,
    urgent: Cell<bool>,
    canceled: Cell<bool>,
    composed: Cell<bool>,
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
        premeasure: bool,
    ) -> Handle {
        Handle {
            state: Rc::downgrade(state),
            item,
            unit,
            key: items.key(item),
            content_type: items.content_type(item),
            premeasure,
            urgent: Cell::new(false),
            canceled: Cell::new(false),
            composed: Cell::new(false),
            measured: Cell::new(false),
            done: Cell::new(false),
            queued: Cell::new(false),
            nested: RefCell::new(None),
        }
    }

    /// `executeRequest`: the steps not done yet, each while it fits. Returns whether work is
    /// left (the request stays queued for the next idle time).
    fn execute(&self, scope: &mut PrefetchRequestScope<'_>) -> bool {
        let Some(state) = self.state.upgrade() else { return false };
        let prefetch = &state.prefetch;
        let exists = prefetch
            .items
            .borrow()
            .upgrade()
            .is_some_and(|items| self.item < items.count && items.key(self.item) == self.key);
        if self.canceled.get() || !exists {
            self.canceled.set(true);
            return false;
        }
        let urgent = self.urgent.get();
        let averages = prefetch.averages(self.content_type);
        if !self.composed.get() {
            if !prefetch.fits(scope, Step::Compose, averages.composition, urgent) {
                return self.short_of_time(&state);
            }
            let section = compose_ui::platform::trace::section(c"compose-rs: prefetch compose");
            let began = Instant::now();
            match prefetch.compose_item(&state, self.item, scope) {
                // Not next to the composed items: no longer to be prefetched.
                None => {
                    self.canceled.set(true);
                    return false;
                }
                Some(composed) => {
                    if composed {
                        let nanos = began.elapsed().as_nanos() as u64;
                        prefetch.timed(scope, Step::Compose, self.content_type, nanos);
                        // What composed in it: `resolveNestedPrefetchStates`, which runs only
                        // with time left (as the next step).
                        *self.nested.borrow_mut() = Some(NestedPrefetch::of(&prefetch.nested));
                    }
                }
            }
            drop(section);
            prefetch.stepped(scope);
            self.composed.set(true);
        }
        if self.nested.borrow().is_none() {
            // Composed before this request (a frame's), or nothing composed in it.
            *self.nested.borrow_mut() = Some(NestedPrefetch::default());
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
                    scope.premeasure(node, child, constraints);
                    if !scope.recompose() {
                        break;
                    }
                }
                // Timed only when it measured: an item gone in the meantime would lower the
                // estimate with a sample of nothing.
                prefetch.timed(
                    scope,
                    Step::Measure,
                    self.content_type,
                    start.elapsed().as_nanos() as u64,
                );
            }
            prefetch.stepped(scope);
            self.measured.set(true);
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
                prefetch.update_averages(self.content_type, |averages| {
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
                                .map(|item| Rc::new(Handle::new(&list, &items, item, item, false)))
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
        if state.prefetch.policy.get().recent_maximum {
            self.queued.set(false);
            return false;
        }
        true
    }
}

impl NestedPrefetch {
    /// The lazy lists noted while composing an item (each once, live).
    fn of(noted: &RefCell<Vec<Weak<LazyListInner>>>) -> NestedPrefetch {
        let mut lists: Vec<Rc<LazyListInner>> = Vec::new();
        for list in noted.borrow_mut().drain(..) {
            if let Some(list) = list.upgrade()
                && !lists.iter().any(|known| Rc::ptr_eq(known, &list))
            {
                lists.push(list);
            }
        }
        let requests = lists.iter().map(|_| None).collect();
        NestedPrefetch { lists, requests, list: 0, request: 0 }
    }
}


/// A [`Handle`] in the scheduler's queue.
struct HandleRequest(Rc<Handle>);

impl PrefetchRequest for HandleRequest {
    fn execute(&self, scope: &mut PrefetchRequestScope<'_>) -> bool {
        let more = self.0.execute(scope);
        if !more {
            self.0.queued.set(false);
        }
        more
    }
}

/// With `KEEP_ITEMS_BEHIND`, disposes between frames the items composed beyond the limits the
/// last layout set, when no request composes (which would trim them).
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
        // Behind the scroll only: those ahead are the next ones.
        let range =
            if prefetch.forward.get() { (start.max(low), end) } else { (start, end.min(high)) };
        if range == (start, end) {
            return false;
        }
        let averages = prefetch.averages(None);
        if !prefetch.fits(scope, Step::Compose, averages.composition, false) {
            return !prefetch.policy.get().recent_maximum;
        }
        let _section = compose_ui::platform::trace::section(c"compose-rs: prefetch compose");
        state.compose(range);
        scope.recompose();
        prefetch.stepped(scope);
        false
    }
}

/// The lazy list whose items are composing, for the lazy lists in them to note themselves in
/// while a request composes one (Compose finds them by `traversablePrefetchState`).
#[derive(Clone)]
pub(super) struct ParentList(pub(super) Weak<LazyListInner>);

impl ComposeArg for ParentList {
    fn same(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.0, &other.0)
    }
}

pub(super) static LOCAL_PARENT_LIST: compose_runtime::CompositionLocal<Option<ParentList>> =
    compose_runtime::CompositionLocal::new("LocalParentLazyList", || None);

/// A lazy list composing: notes `state` in the list around it while a request composes that
/// list's item.
pub(super) fn note_nested(state: &Rc<LazyListInner>) {
    if let Some(ParentList(parent)) = LOCAL_PARENT_LIST.current()
        && let Some(parent) = parent.upgrade()
        && parent.prefetch.collecting.get()
        && !Rc::ptr_eq(&parent, state)
    {
        parent.prefetch.nested.borrow_mut().push(Rc::downgrade(state));
    }
}

/// This list's steps in an idle time (`PREFETCH_BUDGET_RECENT_MAXIMUM`): whether none ran yet,
/// and their pace (D381).
#[derive(Clone, Copy, Debug)]
struct IdleSteps {
    idle_time: u64,
    first: bool,
    pace: Pace,
}

impl Default for IdleSteps {
    fn default() -> IdleSteps {
        IdleSteps { idle_time: 0, first: true, pace: Pace::EXPECTED }
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
