//! Scrollbars: the thumb's size and place from the adapter; dragging it scrolls, a press on the
//! track pages.

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_test::*;
use compose_ui::{DrawOp, IntSize, Offset, PointerEvent, Rect, UnitExt};

/// The thumb: the only rounded rect drawn.
fn thumb(ui: &mut ComposeTestRule) -> Option<Rect> {
    ui.host().display_list().ops.iter().find_map(|op| match op {
        DrawOp::DrawRoundRect(rect, _, _) => Some(*rect),
        _ => None,
    })
}

#[test]
fn a_scrollbar_shows_and_drags_a_scroll() {
    let state = ScrollState::new(0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 200), move || {
        let state = given;
        Row!(modifier = Modifier.fill_max_size(), |row: RowScope| {
            Column!(modifier = Modifier.weight(row, 1.0).vertical_scroll(state), {
                for _ in 0..20 {
                    Box!(modifier = Modifier.size(50.dp()));
                }
            });
            let adapter = remember_scrollbar_adapter(state);
            VerticalScrollbar!(adapter, modifier = Modifier.fill_max_height());
        });
    });
    // 200 of 1000 px in view: a 40 px thumb at the top, 8 px wide (in the scrollbar's
    // coordinates).
    let at_rest = thumb(&mut ui).expect("a thumb");
    assert_eq!(
        (at_rest.left, at_rest.top, at_rest.width(), at_rest.height()),
        (0.0, 0.0, 8.0, 40.0)
    );
    // Dragging it 80 px of its 160 px of room scrolls half of the 800 px.
    ui.host().send_pointer(PointerEvent::press(Offset::new(296.0, 20.0)));
    ui.host().send_pointer(PointerEvent::moved(Offset::new(296.0, 100.0), true));
    ui.host().send_pointer(PointerEvent::release(Offset::new(296.0, 100.0)));
    ui.wait_for_idle();
    assert_eq!(state.value(), 400);
    assert_eq!(thumb(&mut ui).map(|thumb| thumb.top), Some(80.0));
    // A press on the track below pages down.
    ui.host().send_pointer(PointerEvent::press(Offset::new(296.0, 180.0)));
    ui.host().send_pointer(PointerEvent::release(Offset::new(296.0, 180.0)));
    ui.wait_for_idle();
    assert_eq!(state.value(), 600);
}

#[test]
fn a_scrollbar_follows_a_lazy_list_and_hides_when_it_fits() {
    let list = LazyListState::new(0, 0);
    let given = *list;
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 200), move || {
        let list = given;
        Row!(modifier = Modifier.fill_max_size(), |row: RowScope| {
            LazyColumn!(
                modifier = Modifier.weight(row, 1.0),
                state = list,
                |scope: LazyListScope| {
                    scope.items(100, |_| Box!(modifier = Modifier.size(20.dp())));
                }
            );
            let adapter = remember_lazy_list_scrollbar_adapter(list);
            VerticalScrollbar!(adapter, modifier = Modifier.fill_max_height());
        });
    });
    // 200 of 2000 px: a 20 px thumb.
    assert_eq!(thumb(&mut ui).map(|thumb| thumb.height()), Some(20.0));
    list.scroll_to_item(50, 0);
    ui.wait_for_idle();
    // 1000 of the 1800 px of room: the thumb 100 px of its 180 down.
    assert_eq!(thumb(&mut ui).map(|thumb| thumb.top.round()), Some(100.0));
    // A list that fits has no thumb.
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 200), || {
        let list = remember_lazy_list_state(0, 0);
        Row!(modifier = Modifier.fill_max_size(), |row: RowScope| {
            LazyColumn!(
                modifier = Modifier.weight(row, 1.0),
                state = list,
                |scope: LazyListScope| {
                    scope.items(3, |_| Box!(modifier = Modifier.size(20.dp())));
                }
            );
            let adapter = remember_lazy_list_scrollbar_adapter(list);
            VerticalScrollbar!(adapter, modifier = Modifier.fill_max_height());
        });
    });
    assert_eq!(thumb(&mut ui), None);
}
