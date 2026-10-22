//! Adversarial review of retained drawing per repaint boundary (docs/06-ui.md, "Repaint
//! boundaries"): after a change, a frame draws what a fresh host with the final values draws.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Color, DisplayList, IntSize, Key, KeyEvent, KeyEventType, KeyModifiers, Offset, Paint,
    PointerEvent, UnitExt,
};

/// A display list's dump, without its paragraph handles (a fresh measurer numbers them anew).
fn dump(list: &DisplayList) -> String {
    list.dump()
        .lines()
        .map(|line| match line.find("paragraph ") {
            Some(at) => {
                let rest = &line[at + "paragraph ".len()..];
                let after = rest.find(' ').map_or("", |space| &rest[space..]);
                format!("{}paragraph _{after}", &line[..at])
            }
            None => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A scrollbar in its own layer (a fading scrollbar's), beside a scroll whose content grows: the
/// scroll's maximum is written while laying out, and the frame that lays it out draws the thumb
/// from it, as a fresh draw does.
#[test]
fn a_scrollbar_in_its_own_layer_follows_the_content_in_the_frame_laying_it_out() {
    fn scene(count: MutableState<usize>) {
        let state = remember(|| ScrollState::new(0));
        Row!(modifier = Modifier.fill_max_size(), |row: RowScope| {
            let scrolled = state;
            Column!(modifier = Modifier.weight(row, 1.0).vertical_scroll(scrolled), {
                for _ in 0..count.get() {
                    Box!(modifier = Modifier.size(50.dp()));
                }
            });
            let adapter = remember_scrollbar_adapter(state);
            VerticalScrollbar!(adapter, modifier = Modifier.repaint_boundary().fill_max_height());
        });
    }
    let count = mutable_state_of(20);
    let handle = count.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 200), move || scene(handle));
    count.set(40);
    ui.host().frame();
    let incremental = dump(&ui.host().display_list());
    let fresh_count = mutable_state_of(40);
    let fresh_handle = fresh_count.handle();
    let mut fresh = ComposeTestRule::sized(IntSize::new(300, 200), move || scene(fresh_handle));
    assert_eq!(incremental, dump(&fresh.host().display_list()), "the thumb of 20 items");
}

/// Lazy items drawing from the list's scroll (a parallax): a scroll writes the first visible
/// item's offset while laying out, and the frame laying it out draws the items from it, as a
/// fresh draw at that offset does.
#[test]
fn lazy_items_drawing_the_scroll_draw_it_in_the_frame_laying_it_out() {
    fn scene(state: LazyListState) {
        let list = state;
        LazyColumn!(
            modifier = Modifier.size_wh(100.dp(), 100.dp()),
            state = state,
            |scope: LazyListScope| {
                scope.items(50, move |_| {
                    Box!(
                        modifier = Modifier
                            .draw_behind(move |scope| {
                                let shift = list.first_visible_item_scroll_offset() as f32;
                                scope.draw_rect(
                                    compose_ui::Rect::new(shift, 0.0, shift + 4.0, 4.0),
                                    Paint::fill(Color::RED),
                                );
                            })
                            .size_wh(100.dp(), 20.dp())
                    );
                });
            }
        );
    }
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || scene(given));
    assert!(state.scroll_by(13.0));
    ui.host().frame();
    assert_eq!(state.first_visible_item_scroll_offset(), 13);
    let incremental = dump(&ui.host().display_list());
    let fresh_state = LazyListState::new(0, 13);
    let mut fresh = ComposeTestRule::sized(IntSize::new(200, 200), move || scene(*fresh_state));
    assert_eq!(incremental, dump(&fresh.host().display_list()), "items drawn at offset 0");
}

/// Lazy items whose content changes, one removed, one added: each frame draws what a fresh
/// list with those items does.
#[test]
fn lazy_items_changing_draw_what_a_fresh_list_does() {
    fn scene(items: MutableState<Vec<u32>>) {
        LazyColumn!(modifier = Modifier.size_wh(100.dp(), 100.dp()), |scope: LazyListScope| {
            let items = items.get();
            let keys = items.clone();
            scope.items_keyed(
                items.len(),
                move |index| keys[index],
                move |index| {
                    let value = items[index];
                    Box!(
                        modifier = Modifier
                            .background(Color::from_argb(0xFF00_0000 | (value * 0x0101)))
                            .size_wh(100.dp(), (10 + value % 3 * 5).dp()),
                        {
                            BasicText!("item {}", value);
                        }
                    );
                },
            );
        });
    }
    let items = mutable_state_of(vec![1u32, 2, 3, 4, 5]);
    let handle = items.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || scene(handle));
    for next in [vec![1, 2, 7, 4, 5], vec![1, 7, 4, 5], vec![9, 1, 7, 4, 5], vec![9, 1, 5, 4, 7]] {
        items.set(next.clone());
        ui.wait_for_idle();
        let fresh_items = mutable_state_of(next.clone());
        let fresh_handle = fresh_items.handle();
        let mut fresh = ComposeTestRule::sized(IntSize::new(200, 200), move || scene(fresh_handle));
        assert_eq!(dump(&ui.host().display_list()), dump(&fresh.host().display_list()), "{next:?}");
    }
}

fn shortcut(ui: &mut ComposeTestRule, c: char) {
    let modifiers = if cfg!(target_os = "macos") {
        KeyModifiers { meta: true, ..Default::default() }
    } else {
        KeyModifiers { ctrl: true, ..Default::default() }
    };
    let key = Key::Character(c);
    ui.host().send_key(KeyEvent { key, kind: KeyEventType::KeyDown, modifiers });
    ui.host().send_key(KeyEvent { key, kind: KeyEventType::KeyUp, modifiers });
    ui.wait_for_idle();
}

/// Texts in their own repaint boundaries in a selection container: a selection (drag, select
/// all) highlights them, a click clears it, each drawn in that frame.
#[test]
fn a_selection_across_boundaries_is_drawn_and_cleared() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {
        SelectionContainer!({
            Column!({
                BasicText!("Hello world", modifier = Modifier.repaint_boundary().test_tag("first"));
                Box!(modifier = Modifier.repaint_boundary(), {
                    BasicText!("Second line", modifier = Modifier.test_tag("second"));
                });
            });
        });
    });
    let highlights = |ui: &mut ComposeTestRule| ui.dump_display_list().matches("#664286F4").count();
    let first = ui.on_node_with_tag("first").fetch_semantics_node().bounds;
    let second = ui.on_node_with_tag("second").fetch_semantics_node().bounds;
    let from = Offset::new(first.left + 1.0, (first.top + first.bottom) / 2.0);
    let to = Offset::new(second.left + 30.0, (second.top + second.bottom) / 2.0);
    ui.host().send_pointer(PointerEvent::press(from));
    ui.host().send_pointer(PointerEvent::moved(to, true));
    ui.host().frame();
    assert!(highlights(&mut ui) >= 2, "{}", ui.dump_display_list());
    ui.host().send_pointer(PointerEvent::moved(Offset::new(first.left + 20.0, from.y), true));
    ui.host().frame();
    assert_eq!(highlights(&mut ui), 1, "{}", ui.dump_display_list());
    ui.host().send_pointer(PointerEvent::release(Offset::new(first.left + 20.0, from.y)));
    ui.wait_for_idle();
    shortcut(&mut ui, 'a');
    assert!(highlights(&mut ui) >= 2, "{}", ui.dump_display_list());
    ui.advance_time_by(std::time::Duration::from_millis(600));
    ui.host().send_pointer(PointerEvent::press(to));
    ui.host().send_pointer(PointerEvent::release(to));
    ui.host().frame();
    assert_eq!(highlights(&mut ui), 0, "{}", ui.dump_display_list());
}

/// A text field in a lazy item (a repaint boundary): typing, moving the cursor, selecting and
/// losing the focus each draw in that frame.
#[test]
fn a_text_field_in_a_boundary_draws_its_cursor_and_selection() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 200), || {
        let text = remember(|| mutable_state_of(String::from("milk")));
        Column!({
            Box!(modifier = Modifier.repaint_boundary(), {
                BasicTextField!(
                    value = text.get(),
                    on_value_change = move |value| text.set(value),
                    modifier = Modifier.fill_max_width().test_tag("field"),
                );
            });
            Box!(modifier = Modifier.focusable(true).test_tag("other").size(20.dp()));
        });
    });
    let selection = |ui: &mut ComposeTestRule| ui.dump_display_list().contains("#664286F4");
    ui.on_node_with_tag("field").request_focus();
    let focused = ui.dump_display_list();
    ui.press_key(Key::MoveEnd);
    let shift = KeyModifiers { shift: true, ..Default::default() };
    ui.host().send_key(KeyEvent {
        key: Key::DirectionLeft,
        kind: KeyEventType::KeyDown,
        modifiers: shift,
    });
    ui.host().frame();
    assert!(selection(&mut ui), "{}", ui.dump_display_list());
    ui.host().send_key(KeyEvent {
        key: Key::DirectionLeft,
        kind: KeyEventType::KeyUp,
        modifiers: shift,
    });
    ui.on_node_with_tag("other").request_focus();
    assert!(!selection(&mut ui), "{}", ui.dump_display_list());
    assert_ne!(ui.dump_display_list(), focused, "the cursor is gone");
    ui.on_node_with_tag("field").request_focus();
    ui.type_text("y");
    ui.on_node_with_tag("field").assert_text_equals(&["milky"]);
}
