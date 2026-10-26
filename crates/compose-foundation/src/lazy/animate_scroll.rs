//! `animateScrollToItem`, as Compose's `LazyLayoutScrollScope.animateScrollToItem`: a spring
//! towards the item, 2,500 dp at a time while it's far (teleporting near it after the first
//! stretch if it's more than a hundred items away), then a seek to its offset once in view.
//!
//! compose-rs applies a scroll at the next layout (D476), so what the scope says of the items
//! in view is the last frame's: the item is found, or overshot, a frame after Compose would.

use compose_animation::{AnimationState, spring};

use crate::gestures::ScrollScope;

/// How far one stretch of the search goes, as Compose's `TargetDistance`.
const TARGET_DISTANCE_DP: f32 = 2500.0;
/// How far a stretch goes before it's cut short, as Compose's `BoundDistance`.
const BOUND_DISTANCE_DP: f32 = 1500.0;
/// The least a stretch goes, as Compose's `MinimumDistance`.
const MINIMUM_DISTANCE_DP: f32 = 50.0;

/// A lazy layout's scroll scope, as Compose's `LazyLayoutScrollScope`: a scroll scope that
/// knows the items in view and can jump to an item. `LazyListState::lazy_layout_scroll_scope`
/// and `LazyGridState::lazy_layout_scroll_scope` make one from a scroll's scope.
pub trait LazyLayoutScrollScope {
    /// The scroll's scope.
    fn scroll_scope(&self) -> &ScrollScope;
    /// Scrolls by `pixels`; returns the pixels used, as `scrollBy`.
    fn scroll_by(&self, pixels: f32) -> f32 {
        self.scroll_scope().scroll_by(pixels)
    }
    fn first_visible_item_index(&self) -> usize;
    fn first_visible_item_scroll_offset(&self) -> i32;
    /// The last item in view (0 with none), as `lastVisibleItemIndex`.
    fn last_visible_item_index(&self) -> usize;
    fn item_count(&self) -> usize;
    /// Jumps to `index`, `offset` pixels into it, as `snapToItem`.
    fn snap_to_item(&self, index: usize, offset: i32);
    /// How far `target_index` (`target_offset` into it) is from the scroll position: exact for
    /// an item in view, else estimated from the average size, as `calculateDistanceTo`.
    fn calculate_distance_to(&self, target_index: usize, target_offset: i32) -> i32;
}

fn is_item_visible(scope: &dyn LazyLayoutScrollScope, index: usize) -> bool {
    (scope.first_visible_item_index()..=scope.last_visible_item_index()).contains(&index)
}

/// Animates `scope` to `index`, `scroll_offset` into it, teleporting when more than
/// `items_for_teleport` items away; `density` in pixels per dp.
pub(crate) async fn animate_scroll_to_item(
    scope: &dyn LazyLayoutScrollScope,
    index: usize,
    scroll_offset: i32,
    items_for_teleport: usize,
    density: f32,
) {
    let target_distance = TARGET_DISTANCE_DP * density;
    let bound_distance = BOUND_DISTANCE_DP * density;
    let minimum_distance = MINIMUM_DISTANCE_DP * density;
    let mut anim = AnimationState::new(0.0, 0.0);
    let found = if is_item_visible(scope, index) {
        Some(scope.calculate_distance_to(index, 0))
    } else {
        search(
            scope,
            index,
            scroll_offset,
            items_for_teleport,
            (target_distance, bound_distance, minimum_distance),
            &mut anim,
        )
        .await
    };
    let Some(item_offset) = found else { return };
    // In view: a seek to its offset, at the search's velocity.
    let mut anim = anim.with_value(0.0);
    let target = (item_offset + scroll_offset) as f32;
    let mut previous = 0.0f32;
    let sequential = anim.velocity() != 0.0;
    anim.animate_to(target, spring(), sequential, |frame| {
        let value = frame.value();
        let coerced = if target > 0.0 {
            value.min(target)
        } else if target < 0.0 {
            value.max(target)
        } else {
            0.0
        };
        let delta = coerced - previous;
        let consumed = scope.scroll_by(delta);
        // At the end, or about to overshoot: stop.
        if delta != consumed || coerced != value {
            frame.cancel_animation();
        }
        previous += delta;
    })
    .await;
    scope.snap_to_item(index, scroll_offset);
}

/// The search for an item out of view: stretches of a spring towards it until it's in view
/// (its distance then), overshot (snapped to it) or the end is reached (`None` for both).
async fn search(
    scope: &dyn LazyLayoutScrollScope,
    index: usize,
    scroll_offset: i32,
    items_for_teleport: usize,
    (target_distance, bound_distance, minimum_distance): (f32, f32, f32),
    anim: &mut AnimationState,
) -> Option<i32> {
    let forward = index > scope.first_visible_item_index();
    let overshot = || {
        let (first, offset) =
            (scope.first_visible_item_index(), scope.first_visible_item_scroll_offset());
        if forward {
            first > index || (first == index && offset > scroll_offset)
        } else {
            first < index || (first == index && offset < scroll_offset)
        }
    };
    let mut loops = 1;
    loop {
        if scope.item_count() == 0 {
            return None;
        }
        let expected = (scope.calculate_distance_to(index, 0) + scroll_offset) as f32;
        let target = if expected.abs() < target_distance {
            let target = expected.abs().max(minimum_distance);
            if forward { target } else { -target }
        } else if forward {
            target_distance
        } else {
            -target_distance
        };
        *anim = anim.with_value(0.0);
        let mut previous = 0.0f32;
        let mut outcome: Option<Option<i32>> = None;
        let sequential = anim.velocity() != 0.0;
        anim.animate_to(target, spring(), sequential, |frame| {
            let value = frame.value();
            if !is_item_visible(scope, index) {
                let coerced = if target > 0.0 { value.min(target) } else { value.max(target) };
                let delta = coerced - previous;
                let consumed = scope.scroll_by(delta);
                if is_item_visible(scope, index) {
                    // Found right after the scroll.
                } else if !overshot() {
                    if delta != consumed {
                        // The end, without the item.
                        frame.cancel_animation();
                        outcome = Some(None);
                        return;
                    }
                    previous += delta;
                    if (forward && value > bound_distance) || (!forward && value < -bound_distance)
                    {
                        frame.cancel_animation();
                    }
                    if forward {
                        if loops >= 2
                            && index.saturating_sub(scope.last_visible_item_index())
                                > items_for_teleport
                        {
                            scope.snap_to_item(index - items_for_teleport, 0);
                        }
                    } else if loops >= 2
                        && scope.first_visible_item_index().saturating_sub(index)
                            > items_for_teleport
                    {
                        scope.snap_to_item(index + items_for_teleport, 0);
                    }
                }
            }
            if overshot() {
                scope.snap_to_item(index, scroll_offset);
                frame.cancel_animation();
                outcome = Some(None);
            } else if is_item_visible(scope, index) {
                frame.cancel_animation();
                outcome = Some(Some(scope.calculate_distance_to(index, 0)));
            }
        })
        .await;
        if let Some(outcome) = outcome {
            return outcome;
        }
        loops += 1;
    }
}
