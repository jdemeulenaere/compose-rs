//! compose-rs's own lazy-list behaviours beyond Compose's (D560-D565), which hosts don't use by
//! default: for the tests written for them, before the lists followed Compose's prefetch.

use compose_ui::platform::Optimizations;

/// Two ahead, D341's budget, prefetching at rest, two kept behind, drawing ahead and a grid's
/// lines beyond: lazy lists as before D560.
pub fn optimizations() -> Optimizations {
    Optimizations::parse(
        "prefetch_two_ahead,prefetch_budget_recent_maximum,prefetch_at_rest,keep_items_behind,\
         draw_prefetched_ahead,grid_lines_beyond",
    )
    .expect("known names")
}

/// The rules this test makes use them (`compose_test::use_optimizations`).
pub fn lazy_lists() {
    compose_test::use_optimizations(optimizations());
}
