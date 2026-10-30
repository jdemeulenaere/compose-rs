//! Eleventh review round: keyboard focus traversal into lazy lists, against Jetpack Compose
//! (Desktop)'s one-dimensional focus search. Each test states the input, what Compose does,
//! and what happens instead.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_test::*;
use compose_ui::{IntSize, Key, UnitExt};

/// Input: a clickable box "before", then a 100 dp `LazyColumn` of 1000 clickable 20 dp rows
/// scrolled to row 50 (`LazyListState(50, 0)`: rows 50 to 54 in view), then a box "after".
/// "before" is focused and Tab pressed.
///
/// Compose: Tab's one-dimensional search goes to the next focus target in placement order that
/// is placed (`isEligibleForFocusSearch`). A lazy list places only the items in view (prefetched
/// items are composed, not placed; beyond-bounds items are composed only while a search from
/// inside the list asks for them), so the focus lands on row 50, the first row in view, and the
/// list doesn't move.
///
/// Here: the list keeps a viewport's worth of rows composed and placed (out of view) on each
/// side, and they come first in tree order, so Tab lands on row 45, above the viewport, and
/// bringing it into view scrolls the list back five rows. Shift+Tab from "after" likewise
/// lands on the last row composed below the viewport and scrolls down. Entering a scrolled
/// list from the keyboard jumps it.
#[test]
fn tab_into_a_scrolled_list_focuses_its_first_row_in_view() {
    let state = LazyListState::new(50, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 300), move || {
        Column!({
            Box!(modifier = Modifier.size((100.dp(), 20.dp())).clickable(|| {}).test_tag("before"));
            LazyColumn!(
                modifier = Modifier.size((100.dp(), 100.dp())),
                state = given,
                |scope: LazyListScope| {
                    scope.items(1000, |index| {
                        Box!(
                            modifier = Modifier
                                .size((100.dp(), 20.dp()))
                                .clickable(|| {})
                                .test_tag(format!("row {index}"))
                        );
                    });
                }
            );
            Box!(modifier = Modifier.size((100.dp(), 20.dp())).clickable(|| {}).test_tag("after"));
        });
    });
    ui.on_node_with_tag("before").request_focus().assert_is_focused();
    assert_eq!(state.first_visible_item_index(), 50);
    ui.press_key(Key::Tab);
    let focused: Vec<usize> = (40..60)
        .filter(|index| {
            let tag = format!("row {index}");
            ui.count_nodes(&has_test_tag(&tag)) > 0
                && ui
                    .on_node_with_tag(&tag)
                    .fetch_semantics_snapshot()
                    .config
                    .get(&compose_ui::SemanticsProperties::FOCUSED)
                    .copied()
                    .unwrap_or(false)
        })
        .collect();
    assert_eq!(
        (focused, state.first_visible_item_index()),
        (vec![50], 50),
        "(the focused rows, the first row in view) after Tab into the list"
    );
}
