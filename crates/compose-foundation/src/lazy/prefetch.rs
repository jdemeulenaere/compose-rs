//! Prefetch: what a lazy list (or grid) composes and measures between frames, as Compose
//! 1.12.1's default `LazyListPrefetchStrategy` (a grid's `LazyGridPrefetchStrategy`) and
//! `LazyLayoutPrefetchState` (docs/15, "Compose's defaults" (a), (b) and (f)):
//!
//! - after a layout that scrolled, a request for the item (a grid's line: a request per item)
//!   next to those in view in the scroll direction, none behind; cancelled when the direction
//!   turns, or once the item it holds isn't the next one any more; urgent when the same scroll
//!   again would reach it (`lastItem.offset + size + spacing - viewportEndOffset < -delta`);
//! - a request composes its item with a paused composition (Compose's default,
//!   `isPausableCompositionInPrefetchEnabled`), which pauses at the runtime's pause points once
//!   the time left is no more than the averages of a stretch of composition and a pause, and
//!   resumes after the next frame; it applies it once complete, then composes the first items of
//!   the lazy lists in it (two, then the running average of the smallest nested list's visible
//!   count, per content type), composing only, then measures it: nothing places or draws it
//!   until a frame wants it;
//! - each step runs only while the time left exceeds that step's running average for the item's
//!   content type (`current / 4 * 3 + new / 4`, in integer nanoseconds; the first sample taken
//!   as is): composing, a stretch and a pause; applying, measuring, their own; any time left
//!   for an urgent request; a step that doesn't fit leaves the request queued, to go on after
//!   the next frame.
//!
//! The composed items are those in view and those requests composed next to them: a frame's
//! layout deactivates the others (kept for reuse), as Compose's measurement does. An item whose
//! paused composition isn't applied yet (or was cancelled, its content removed) is one the
//! layout lacks: wanted, the frame composes it (completing the paused composition first), as
//! Compose's measurement completes a precomposed item's paused composition.
//!
//! Without pausable composition (`ComposeFoundationFlags::set_pausable_composition_in_prefetch_
//! enabled(false)`, as Compose's flag), a request's composition is one step, applied with it,
//! gated by the whole composition's average.
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

use compose_runtime::PausedComposition;
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

/// The items composed by requests a list remembers (`Prefetch::was_precomposed`).
const PRECOMPOSED_KEPT: usize = 8;

/// Items noted with nested lists beyond which the dead entries are forgotten.
const NESTED_KEYS_KEPT: usize = 64;

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
    /// A request composes its item with a paused composition (Compose's default).
    pub(super) pausable: bool,
}

impl Policy {
    /// Compose's.
    pub(super) const COMPOSE: Policy =
        Policy { ahead: 1, recent_maximum: false, keep_behind: 0, at_rest: false, pausable: true };

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
            pausable: crate::ComposeFoundationFlags::is_pausable_composition_in_prefetch_enabled(),
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
    /// D341's estimates (`PREFETCH_BUDGET_RECENT_MAXIMUM`), one for all content types: a whole
    /// composition's (a paused one's resumes summed), an apply's, a measurement's.
    compose_cost: Cell<StepCost>,
    apply_cost: Cell<StepCost>,
    measure_cost: Cell<StepCost>,
    /// This list's steps in the idle time under way (`PREFETCH_BUDGET_RECENT_MAXIMUM`).
    idle: Cell<IdleSteps>,
    /// The lazy lists composed in each item, by its key: a request finds its item's nested lists
    /// there, whoever composed it (Compose traverses the precomposed item's nodes for them,
    /// `traversablePrefetchState`); each notes itself through its item's `ParentList`.
    nested: RefCell<HashMap<u64, Vec<Weak<LazyListInner>>>>,
    /// The item whose paused composition a request holds, not applied yet.
    pending: RefCell<Option<(u64, Rc<PausedComposition>)>>,
    /// Items whose content isn't the layout's to measure: composed by a paused composition not
    /// applied yet, or cancelled (removed, composed again when the item is next composed). A
    /// remember observer in each item's content keeps it: noted as it's made, struck once it's
    /// remembered (D592, D594).
    unready: RefCell<Vec<u64>>,
    /// A trimming request found a paused composition pending: it's asked again once that one's
    /// applied or cancelled.
    trim_waits: Cell<bool>,
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
            limits: Cell::new((0, 0)),
            constraints_of: RefCell::new(None),
            policy: Cell::new(Policy::COMPOSE),
            items: RefCell::new(Weak::new()),
            visible: Cell::new(0),
            strategy: Strategy::default(),
            metrics: RefCell::default(),
            compose_cost: Cell::default(),
            apply_cost: Cell::default(),
            measure_cost: Cell::default(),
            idle: Cell::default(),
            nested: RefCell::default(),
            pending: RefCell::new(None),
            unready: RefCell::default(),
            trim_waits: Cell::new(false),
            cancelled: Cell::new(false),
            scheduler: RefCell::new(None),
            precomposed: RefCell::default(),
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
    /// What the last layout that ran the strategy showed (first and last units, the count):
    /// one showing the same with no scroll since has nothing new for it.
    last_laid: Cell<Option<(usize, usize, usize)>>,
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
        if let Some(scheduler) = scheduler
            && self.scheduler.borrow().is_none()
        {
            *self.scheduler.borrow_mut() = Some(scheduler.clone());
        }
        // The same units in view, the same count, no scroll since: nothing to cancel or request
        // (a frame laying the list out again for another reason).
        let shown = laid.map(|laid| (laid.first, laid.last, count));
        if delta == 0.0 && shown.is_some() && strategy.last_laid.replace(shown) == shown {
            return;
        }
        strategy.last_laid.set(shown);
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
            // Each composes next to the composed items: a line before them from its end.
            let line = items_of(unit);
            let line: Vec<usize> = if forward { line.collect() } else { line.rev().collect() };
            for item in line {
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
        let mut canceled = Vec::new();
        self.strategy.handles.borrow_mut().retain(|handle| {
            let kept = keep(handle.unit);
            if !kept {
                handle.canceled.set(true);
                canceled.push(handle.clone());
            }
            kept
        });
        // Compose's `cancel` cleans up at once (`cleanUp`): a paused composition is cancelled
        // (from a layout, its content is removed before the composition's next pass).
        for handle in canceled {
            handle.clean_up(self);
        }
    }

    /// The items that stay composed, from those the layout wants and those `composed`: the
    /// wanted ones; with `next` (the end, backward the start, of the unit after the wanted ones
    /// the last scroll's way: an item, a grid's line), those composed next to them up to it (a
    /// layout's estimate may compose one more than it places, where Compose's composes as it
    /// measures: it is the one a scroll that way requests, and disposed it would be composed
    /// again); and the items of live requests next to those, composed by the request
    /// (Compose's precomposed items, kept until their request is cancelled) or already. Any
    /// other is let go: those behind, those further ahead, and without `next` (a focus search
    /// ended, D482) those a search laid out (D577).
    pub(super) fn target(
        &self,
        wanted: (usize, usize),
        composed: (usize, usize),
        next: Option<usize>,
    ) -> (usize, usize) {
        let handles = self.strategy.handles.borrow();
        let (mut start, mut end) = wanted;
        if let Some(next) = next
            && composed.0 <= end
            && composed.1 >= start
        {
            if self.forward.get() {
                end = end.max(composed.1.min(next));
            } else {
                start = start.min(composed.0.max(next));
            }
        }
        let holds = |handle: &&Rc<Handle>| {
            !handle.canceled.get()
                && (handle.composed.get() || (composed.0..composed.1).contains(&handle.item))
        };
        loop {
            let mut grew = false;
            for handle in handles.iter().filter(holds) {
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
    /// reuse), unless a request will (its composition trims them): those the layout no longer
    /// wants stay composed, unplaced and undrawn, until then. Compose deactivates them in its
    /// measurement, which composes nothing; here leaving needs a composition, which a frame
    /// whose items are all composed doesn't run for them (D575). With `KEEP_ITEMS_BEHIND`,
    /// behind the scroll only: those ahead are the next ones (D375).
    pub(super) fn trim_between_frames(
        &self,
        state: &Rc<LazyListInner>,
        scheduler: Option<&PrefetchScheduler>,
        composed: (usize, usize),
    ) {
        let (low, high) = self.limits.get();
        let beyond = if self.policy.get().keep_behind == 0 {
            composed.0 < low || composed.1 > high
        } else if self.forward.get() {
            composed.0 < low
        } else {
            composed.1 > high
        };
        if !beyond {
            return;
        }
        let Some(scheduler) = scheduler else { return };
        if self.trim.borrow().upgrade().is_some() {
            return;
        }
        self.trim_waits.set(false);
        let request = Rc::new(TrimRequest { state: Rc::downgrade(state) });
        *self.trim.borrow_mut() = Rc::downgrade(&request);
        if self.policy.get().keep_behind == 0 {
            // First in the next idle time with any time left, whatever the queue holds: a
            // request waiting for a longer idle time, or one whose item is composed already
            // (which deactivates nothing), would leave them composed for good, and an animation
            // in one asking for frames (D575).
            scheduler.schedule_prefetch_first(request);
        } else {
            let composing = self.strategy.handles.borrow().iter().any(|handle| {
                handle.queued.get() && !handle.composed.get() && !handle.canceled.get()
            });
            if !composing {
                scheduler.schedule_prefetch(request);
            }
        }
    }

    fn averages(&self, content_type: Option<u64>) -> Averages {
        self.metrics.borrow().get(&content_type).copied().unwrap_or_default()
    }

    /// Whether the item of `key`'s content is the layout's to measure: not a paused
    /// composition's not applied yet, nor a cancelled one's (D592, D594).
    pub(super) fn content_ready(&self, key: u64) -> bool {
        !self.unready.borrow().contains(&key)
    }

    /// Whether some item's content isn't ready (`content_ready`): a cheap check first.
    pub(super) fn has_unready_content(&self) -> bool {
        !self.unready.borrow().is_empty()
    }

    /// Whether a request's paused composition was cancelled since the last layout asked: the
    /// layout has the items composed again (D594).
    pub(super) fn take_cancelled(&self) -> bool {
        self.cancelled.take()
    }

    /// A paused composition was applied or cancelled: none pending now, and a trimming request
    /// that waited for it is asked again.
    fn pending_done(&self, state: &Rc<LazyListInner>) {
        *self.pending.borrow_mut() = None;
        if self.trim_waits.replace(false) {
            let scheduler = self.scheduler.borrow().clone();
            self.trim_between_frames(state, scheduler.as_ref(), state.composed.get());
        }
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
    /// other side in the same composition (those the layout let go): with `pausable`, as a
    /// paused composition, returned not composed yet (its first resume runs the pass). `None` if
    /// it isn't next to them.
    fn compose_item(
        &self,
        state: &Rc<LazyListInner>,
        item: usize,
        key: u64,
        scope: &mut PrefetchRequestScope<'_>,
        pausable: bool,
    ) -> Option<Composed> {
        let (start, end) = state.composed.get();
        if (start..end).contains(&item) {
            // Composed already, unless a cancelled request's content was removed: composed
            // again then (its item composes it).
            if self.content_ready(key) {
                return Some(Composed::Already);
            }
            state.compose((start, end));
            return Some(self.composed_by(scope, pausable));
        }
        let (low, high) = self.limits.get();
        let range = if item == end {
            (start.max(low).min(item), item + 1)
        } else if item + 1 == start {
            (item, end.min(high).max(item + 1))
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
        state.compose(range);
        Some(self.composed_by(scope, pausable))
    }

    /// Composes what the request's writes invalidated: at once, or as a paused composition.
    fn composed_by(&self, scope: &mut PrefetchRequestScope<'_>, pausable: bool) -> Composed {
        if pausable {
            Composed::Paused(Rc::new(scope.recompose_pausable()))
        } else {
            scope.recompose();
            Composed::Now
        }
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
    /// A whole composition (without pausable composition).
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
    /// Composed and applied now.
    Now,
    /// In a paused composition, to resume until complete, then apply.
    Paused(Rc<PausedComposition>),
}

/// Compose's `Averages`: a content type's steps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Averages {
    /// A whole composition's (`compositionTimeNanos`, without pausable composition).
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
    /// The item's key when requested (`keyUsedForComposition`): another item there ends it.
    key: u64,
    content_type: Option<u64>,
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
            paused: RefCell::new(None),
            composition_nanos: Cell::new(0),
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
            self.clean_up(prefetch);
            return false;
        }
        let urgent = self.urgent.get();
        let averages = prefetch.averages(self.content_type);
        // Compose's `resetAvailableTimeTo`: the steps' times count from here.
        let mark = Instant::now();
        // A paused composition a frame completed (the frame wanted its item): composed.
        if !self.composed.get()
            && self.paused.borrow().as_ref().is_some_and(|paused| paused.is_complete())
        {
            self.composed.set(true);
        }
        if !self.composed.get() {
            let pausable = prefetch.policy.get().pausable;
            if pausable {
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
                let _section =
                    compose_ui::platform::trace::section(c"compose-rs: prefetch compose");
                if !self.perform_pausable_composition(&state, scope, mark, urgent) {
                    return false;
                }
            } else {
                if !prefetch.fits(scope, Step::Compose, averages.composition, urgent) {
                    return self.short_of_time(&state);
                }
                let _section =
                    compose_ui::platform::trace::section(c"compose-rs: prefetch compose");
                match prefetch.compose_item(&state, self.item, self.key, scope, false) {
                    // Not next to the composed items: no longer to be prefetched.
                    None => {
                        self.canceled.set(true);
                        return false;
                    }
                    Some(composed) => {
                        if !matches!(composed, Composed::Already) {
                            let nanos = mark.elapsed().as_nanos() as u64;
                            prefetch.timed(scope, Step::Compose, self.content_type, nanos);
                        }
                    }
                }
                prefetch.stepped(scope);
                self.composed.set(true);
            }
            if !self.composed.get() {
                // Paused: it goes on after the next frame.
                return true;
            }
            // Another item there now (the items changed while composing): done, as Compose's
            // check of the key it composed.
            let same =
                prefetch.items.borrow().upgrade().is_some_and(|items| {
                    self.item < items.count && items.key(self.item) == self.key
                });
            if !same {
                self.canceled.set(true);
                self.clean_up(prefetch);
                return false;
            }
        }
        // Compose's apply step, gated by its own average.
        let paused = self.paused.borrow().clone();
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
                    self.content_type,
                    began.elapsed().as_nanos() as u64,
                );
                prefetch.stepped(scope);
            }
            *self.paused.borrow_mut() = None;
            prefetch.pending_done(&state);
        }
        if self.nested.borrow().is_none() {
            // `resolveNestedPrefetchStates`: the lazy lists in the item, whether this request or
            // a frame composed it, as Compose's request finds those of its precomposed item.
            if scope.available_time_nanos() == 0 {
                return true;
            }
            *self.nested.borrow_mut() = Some(NestedPrefetch::of(prefetch.nested_lists(self.key)));
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
            prefetch.measured(self.key);
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
            None => match prefetch.compose_item(state, self.item, self.key, scope, true) {
                None => {
                    self.canceled.set(true);
                    return false;
                }
                Some(Composed::Paused(paused)) => {
                    *prefetch.pending.borrow_mut() = Some((self.key, paused.clone()));
                    *self.paused.borrow_mut() = Some(paused.clone());
                    paused
                }
                Some(Composed::Already | Composed::Now) => {
                    self.composed.set(true);
                    return true;
                }
            },
        };
        let timing = Rc::new(PauseTiming {
            state: Rc::downgrade(state),
            content_type: self.content_type,
            urgent,
            deadline: scope.deadline(),
            idle_time: scope.idle_time(),
            mark: Cell::new(mark),
            requested: Cell::new(false),
        });
        while !paused.is_complete() && !timing.requested.get() {
            let asked = timing.clone();
            let mark = timing.mark.get();
            // A resume that composed nothing (no pass could run: the composition is applying
            // a pass's end) is a pause: the next idle time tries again.
            if !paused.resume(move || asked.should_pause()) && timing.mark.get() == mark {
                timing.requested.set(true);
            }
        }
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(timing.mark.get()).as_nanos() as u64;
        let step = if timing.requested.get() { Step::Pause } else { Step::Resume };
        prefetch.timed(scope, step, self.content_type, elapsed);
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
        let Some(paused) = self.paused.borrow_mut().take() else { return };
        if paused.is_applied() {
            return;
        }
        paused.cancel();
        prefetch.cancelled.set(true);
        if let Some(state) = self.state.upgrade() {
            prefetch.pending_done(&state);
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
        // Its composition would complete the paused one first: it waits for it to be applied.
        if prefetch.pending.borrow().is_some() {
            prefetch.trim_waits.set(true);
            return false;
        }
        let (start, end) = state.composed.get();
        let (low, high) = prefetch.limits.get();
        let (wanted_start, wanted_end) = prefetch.wanted.get();
        if start > wanted_start || end < wanted_end {
            // The frame composes the wanted ones (and trims).
            return false;
        }
        let policy = prefetch.policy.get();
        // With `KEEP_ITEMS_BEHIND`, behind the scroll only: those ahead are the next ones.
        let range = if policy.keep_behind == 0 {
            (start.max(low), end.min(high))
        } else if prefetch.forward.get() {
            (start.max(low), end)
        } else {
            (start, end.min(high))
        };
        if range == (start, end) || range.0 >= range.1 {
            return false;
        }
        // Compose deactivates in its measurement, so any time left goes; D341's budget times it
        // as a composition.
        let fits = if policy.recent_maximum {
            prefetch.fits(scope, Step::Compose, prefetch.averages(None).composition, false)
        } else {
            scope.available_time_nanos() > 0
        };
        if !fits {
            return !policy.recent_maximum;
        }
        let _section = compose_ui::platform::trace::section(c"compose-rs: prefetch compose");
        state.compose(range);
        scope.recompose();
        prefetch.stepped(scope);
        false
    }
}

/// An item of a lazy list, for the lazy lists composing in it to note themselves in the list
/// under its key (Compose finds them by `traversablePrefetchState`): its slot's, kept across
/// reuse, its key set by each composition of the item.
pub(super) struct ParentList {
    list: Weak<LazyListInner>,
    pub(super) key: Cell<Option<u64>>,
}

impl ParentList {
    pub(super) fn new(list: Weak<LazyListInner>) -> ParentList {
        ParentList { list, key: Cell::new(None) }
    }

    pub(super) fn list(&self) -> Weak<LazyListInner> {
        self.list.clone()
    }
}

pub(super) static LOCAL_PARENT_LIST: compose_runtime::CompositionLocal<Option<Rc<ParentList>>> =
    compose_runtime::CompositionLocal::new("LocalParentLazyList", || None);

/// The most items noted unready: one cancelled and never composed again stays noted (harmless:
/// composed again, it's struck), and the oldest goes beyond this many.
const MOST_UNREADY: usize = 64;

/// An item's content's remember observer: its content is the layout's once remembered (at the
/// end of the pass composing it, or when its paused composition is applied); made, abandoned
/// (a paused composition cancelled), not yet (D592, D594).
pub(super) struct ContentReady {
    list: Weak<LazyListInner>,
    key: u64,
}

impl ContentReady {
    /// Made as the item's content composes: unready until remembered.
    pub(super) fn new(list: Weak<LazyListInner>, key: u64) -> ContentReady {
        if let Some(list) = list.upgrade() {
            let mut unready = list.prefetch.unready.borrow_mut();
            if !unready.contains(&key) {
                if unready.len() >= MOST_UNREADY {
                    unready.remove(0);
                }
                unready.push(key);
            }
        }
        ContentReady { list, key }
    }
}

impl compose_runtime::RememberObserver for ContentReady {
    fn on_remembered(&self) {
        if let Some(list) = self.list.upgrade() {
            list.prefetch.unready.borrow_mut().retain(|key| *key != self.key);
        }
    }
}

/// A lazy list composing: notes `state` in the list around it, under the item composing it.
pub(super) fn note_nested(state: &Rc<LazyListInner>) {
    if let Some(item) = LOCAL_PARENT_LIST.current()
        && let Some(parent) = item.list.upgrade()
        && !Rc::ptr_eq(&parent, state)
        && let Some(key) = item.key.get()
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
