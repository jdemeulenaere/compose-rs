//! Review of D486-D491 (17ecc4e): tests that failed there, fixed by D492, D493 and D495.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_test::*;
use compose_ui::{
    IntSize, NestedScrollConnection, NestedScrollSource, Offset, PointerEvent, PointerId,
    PointerType, UnitExt,
};

/// A 500 px list of `row` px rows, each tagged.
fn rows(state: LazyListState, row: f32) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(100, 500), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 500.dp())),
            state = state,
            move |scope: LazyListScope| {
                scope.items(5000, move |index| {
                    Box!(
                        modifier =
                            Modifier.size((100.dp(), row.dp())).test_tag(format!("row {index}"))
                    );
                });
            }
        );
    })
}

fn shown(ui: &mut ComposeTestRule, index: usize) -> bool {
    ui.count_nodes(&has_test_tag(&format!("row {index}"))) == 1
}

/// Input: a 500 px list of 10 px rows at its top (rows 0 to 51 composed: in view and the two
/// prefetched); `scroll_by(1800)`, then one frame.
///
/// Compose: `scrollBy` measures synchronously; the frame shows rows 180 to 229.
#[test]
fn a_programmatic_scroll_past_the_composed_items_fills_its_frame() {
    let state = LazyListState::new(0, 0);
    let mut ui = rows(*state, 10.0);
    ui.wait_for_idle();
    state.dispatch_raw_delta(1800.0);
    ui.host().frame();
    let first = state.first_visible_item_index();
    let both = shown(&mut ui, 180) && shown(&mut ui, 229);
    assert_eq!((first, both), (180, true));
}

/// As above, 50 px rows, `scroll_by(8000)` (160 rows on, short of the 200-item jump).
#[test]
fn a_programmatic_scroll_of_160_rows_fills_its_frame() {
    let state = LazyListState::new(0, 0);
    let mut ui = rows(*state, 50.0);
    ui.wait_for_idle();
    state.dispatch_raw_delta(8000.0);
    ui.host().frame();
    let first = state.first_visible_item_index();
    let both = shown(&mut ui, 160) && shown(&mut ui, 169);
    assert_eq!((first, both), (160, true));
}

fn touch(event: PointerEvent) -> PointerEvent {
    event.with_pointer(PointerId(1), PointerType::Touch)
}

/// Records the sources of what's offered after the scroll.
#[derive(Default)]
struct Sources {
    post: RefCell<Vec<(Offset, NestedScrollSource)>>,
}

impl NestedScrollConnection for Sources {
    fn on_post_scroll(&self, _: Offset, available: Offset, source: NestedScrollSource) -> Offset {
        self.post.borrow_mut().push((available, source));
        Offset::ZERO
    }
}

/// Input: a 500 px list of 10 px rows at row 150 (start estimated) in a nested-scroll parent
/// (a bottom sheet's, which drags on `UserInput` only); a fast fling back towards the start,
/// passing it in a frame whose delta outruns the prefetched rows.
///
/// Compose: what the fling leaves is offered as `SideEffect` (a sheet doesn't move; a
/// pull-to-refresh doesn't start).
#[test]
fn a_flings_rest_is_offered_as_a_side_effect() {
    let state = LazyListState::new(150, 0);
    let records = Rc::new(Sources::default());
    let connection: Rc<dyn NestedScrollConnection> = records.clone();
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 500), move || {
        Box!(modifier = Modifier.size((100.dp(), 500.dp())).nested_scroll(connection.clone()), {
            LazyColumn!(
                modifier = Modifier.size((100.dp(), 500.dp())),
                state = given,
                |scope: LazyListScope| {
                    scope.items(400, |_| Box!(modifier = Modifier.size((100.dp(), 10.dp()))));
                }
            );
        });
    });
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    let from = Offset::new(50.0, 10.0);
    let mut event = touch(PointerEvent::press(from));
    event.time_nanos = 1;
    ui.host().send_pointer(event);
    for step in 1..=5u64 {
        let mut event =
            touch(PointerEvent::moved(from + Offset::new(0.0, 10.0 + 40.0 * step as f32), true));
        event.time_nanos = 1 + step * 4_000_000;
        ui.host().send_pointer(event);
    }
    let mut event = touch(PointerEvent::release(from + Offset::new(0.0, 210.0)));
    event.time_nanos = 1 + 6 * 4_000_000;
    ui.host().send_pointer(event);
    for _ in 0..120 {
        ui.advance_time_by(std::time::Duration::from_millis(16));
    }
    assert_eq!(
        (state.first_visible_item_index(), state.first_visible_item_scroll_offset()),
        (0, 0)
    );
    let user_input: Vec<_> = records
        .post
        .borrow()
        .iter()
        .filter(|(available, source)| {
            *source == NestedScrollSource::UserInput && available.y.abs() > 0.0
        })
        .copied()
        .collect();
    assert!(user_input.is_empty(), "offered as user input after the drag: {user_input:?}");
}

/// Input: a 100 px list whose first row is focusable, then 300 unfocusable rows; a focusable
/// field and a box after it. The first row focused, Tab (the search waits for rows laid out
/// beyond the list), then, before the search ends, a click focuses the field; frames.
///
/// Compose: the Tab's search runs within the key event: it was over before the click; the
/// field keeps the focus the click gave it.
#[test]
fn a_click_during_a_tabs_search_keeps_its_focus() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 300), || {
        Column!({
            LazyColumn!(modifier = Modifier.size((100.dp(), 100.dp())), |scope: LazyListScope| {
                scope.items(301, |row| {
                    let modifier =
                        Modifier.size((100.dp(), 50.dp())).test_tag(format!("row {row}"));
                    let modifier = if row == 0 { modifier.clickable(|| {}) } else { modifier };
                    Box!(modifier = modifier);
                });
            });
            Box!(modifier = Modifier.size((100.dp(), 20.dp())).clickable(|| {}).test_tag("field"));
            Box!(modifier = Modifier.size((100.dp(), 20.dp())).clickable(|| {}).test_tag("after"));
        });
    });
    ui.on_node_with_tag("row 0").request_focus();
    ui.wait_for_idle();
    ui.host().send_key(compose_ui::KeyEvent::down(compose_ui::Key::Tab));
    ui.host().frame();
    ui.host().frame();
    ui.on_node_with_tag("field").request_focus();
    for _ in 0..60 {
        ui.host().frame();
    }
    ui.on_node_with_tag("field").assert_is_focused();
}
