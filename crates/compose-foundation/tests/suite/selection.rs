//! `SelectionContainer`: selecting texts with the pointer and keys, and copying them.

#![allow(non_snake_case)]

use std::time::Duration;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_test::*;
use compose_ui::{Key, KeyEvent, KeyEventType, KeyModifiers, Offset, PointerEvent, UnitExt};

/// The fixed-advance measurer's character width at the default 14 sp.
const ADVANCE: f32 = 7.0;

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

fn clipboard(ui: &mut ComposeTestRule) -> Option<String> {
    ui.host().with_tree(|tree| tree.clipboard().text())
}

/// Where character `index` of the text tagged `tag` starts (in its middle, vertically).
fn at(ui: &mut ComposeTestRule, tag: &str, index: usize) -> Offset {
    let bounds = ui.on_node_with_tag(tag).fetch_semantics_node().bounds;
    Offset::new(bounds.left + ADVANCE * index as f32 + 1.0, (bounds.top + bounds.bottom) / 2.0)
}

fn texts() -> ComposeTestRule {
    ComposeTestRule::with_manual_clock(|| {
        SelectionContainer!({
            Column!({
                BasicText!("Hello world", modifier = Modifier.test_tag("first"));
                BasicText!("Second line", modifier = Modifier.test_tag("second"));
                DisableSelection!({
                    BasicText!("Not this", modifier = Modifier.test_tag("third"));
                });
            });
        });
    })
}

#[test]
fn a_drag_selects_across_texts_and_copy_takes_it() {
    let mut ui = texts();
    let from = at(&mut ui, "first", 6);
    let to = at(&mut ui, "second", 6);
    ui.host().send_pointer(PointerEvent::press(from));
    ui.host().send_pointer(PointerEvent::moved(to, true));
    ui.host().send_pointer(PointerEvent::release(to));
    ui.wait_for_idle();
    shortcut(&mut ui, 'c');
    assert_eq!(clipboard(&mut ui).as_deref(), Some("world\nSecond"));
    assert_eq!(ui.host().with_tree(|tree| tree.selected_text()).as_deref(), Some("world\nSecond"));
}

#[test]
fn a_double_click_selects_a_word() {
    let mut ui = texts();
    let point = at(&mut ui, "second", 8);
    for _ in 0..2 {
        ui.host().send_pointer(PointerEvent::press(point));
        ui.host().send_pointer(PointerEvent::release(point));
        ui.advance_time_by(Duration::from_millis(50));
    }
    assert_eq!(ui.host().with_tree(|tree| tree.selected_text()).as_deref(), Some("line"));
}

#[test]
fn select_all_leaves_out_disabled_texts() {
    let mut ui = texts();
    let point = at(&mut ui, "first", 0);
    ui.host().send_pointer(PointerEvent::press(point));
    ui.host().send_pointer(PointerEvent::release(point));
    ui.wait_for_idle();
    shortcut(&mut ui, 'a');
    shortcut(&mut ui, 'c');
    assert_eq!(clipboard(&mut ui).as_deref(), Some("Hello world\nSecond line"));
}

#[test]
fn a_selection_is_highlighted_and_a_click_clears_it() {
    let mut ui = texts();
    let from = at(&mut ui, "first", 0);
    let to = at(&mut ui, "first", 5);
    ui.host().send_pointer(PointerEvent::press(from));
    ui.host().send_pointer(PointerEvent::moved(to, true));
    ui.host().send_pointer(PointerEvent::release(to));
    ui.wait_for_idle();
    let highlighted = ui.dump_display_list().contains("#664286F4");
    assert!(highlighted, "{}", ui.dump_display_list());
    ui.advance_time_by(Duration::from_millis(600));
    ui.host().send_pointer(PointerEvent::press(to));
    ui.host().send_pointer(PointerEvent::release(to));
    ui.wait_for_idle();
    assert_eq!(ui.host().with_tree(|tree| tree.selected_text()), None);
    assert!(!ui.dump_display_list().contains("#664286F4"));
}

/// A container losing the focus (to another focus target, without a press) clears its
/// selection, as Compose's `SelectionContainer` does on blur.
#[test]
fn losing_the_focus_clears_the_selection() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {
        Column!({
            SelectionContainer!({
                BasicText!("Hello world", modifier = Modifier.test_tag("first"));
            });
            Box!(modifier = Modifier.size(20.dp()).clickable(|| {}).test_tag("other"));
        });
    });
    let from = at(&mut ui, "first", 0);
    let to = at(&mut ui, "first", 5);
    ui.host().send_pointer(PointerEvent::press(from));
    ui.host().send_pointer(PointerEvent::moved(to, true));
    ui.host().send_pointer(PointerEvent::release(to));
    ui.wait_for_idle();
    assert_eq!(ui.host().with_tree(|tree| tree.selected_text()).as_deref(), Some("Hello"));
    ui.on_node_with_tag("other").request_focus();
    ui.wait_for_idle();
    assert_eq!(ui.host().with_tree(|tree| tree.selected_text()), None);
}
