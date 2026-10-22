//! Foundation layouts and text, through the test rule.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Dp, IntSize, Key, Rect, UnitExt};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> Rect {
    ui.on_node_with_tag(tag).fetch_semantics_node().bounds
}

#[test]
fn a_column_arranges_and_aligns_its_children() {
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 200), || {
        Column!(
            modifier = Modifier.fill_max_size(),
            vertical_arrangement = Arrangement::spaced_by(10.dp()),
            horizontal_alignment = Alignment::CENTER_HORIZONTALLY,
            |column| {
                Spacer(Modifier.size((20.dp(), 10.dp())).test_tag("a"));
                Spacer(Modifier.size((40.dp(), 10.dp())).test_tag("b"));
                Spacer(
                    Modifier.size((40.dp(), 10.dp())).align(column, Alignment::END).test_tag("c"),
                );
            }
        );
    });
    assert_eq!(bounds(&mut ui, "a"), Rect::new(40.0, 0.0, 60.0, 10.0));
    assert_eq!(bounds(&mut ui, "b"), Rect::new(30.0, 20.0, 70.0, 30.0));
    assert_eq!(bounds(&mut ui, "c"), Rect::new(60.0, 40.0, 100.0, 50.0));
}

#[test]
fn a_row_shares_its_space_by_weight() {
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 50), || {
        Row!(
            modifier = Modifier.fill_max_width(),
            vertical_alignment = Alignment::CENTER_VERTICALLY,
            |row| {
                Spacer(Modifier.size((10.dp(), 20.dp())).test_tag("fixed"));
                Spacer(Modifier.weight(row, 1.0).height(10.dp()).test_tag("one"));
                Spacer(Modifier.weight(row, 2.0).height(10.dp()).test_tag("two"));
            }
        );
    });
    // 90 left, split 30 and 60; the row is as tall as its tallest child, centering the others.
    assert_eq!(bounds(&mut ui, "fixed"), Rect::new(0.0, 0.0, 10.0, 20.0));
    assert_eq!(bounds(&mut ui, "one"), Rect::new(10.0, 5.0, 40.0, 15.0));
    assert_eq!(bounds(&mut ui, "two"), Rect::new(40.0, 5.0, 100.0, 15.0));
}

#[test]
fn row_arrangements_follow_compose() {
    let arrangement = mutable_state_of(HorizontalArrangement::from(Arrangement::SPACE_BETWEEN));
    let handle = arrangement.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 50), move || {
        Row!(modifier = Modifier.fill_max_width(), horizontal_arrangement = handle.get(), {
            Spacer(Modifier.size(10.dp()).test_tag("a"));
            Spacer(Modifier.size(10.dp()).test_tag("b"));
        });
    });
    assert_eq!(bounds(&mut ui, "b").left, 90.0);
    arrangement.set(Arrangement::CENTER.into());
    ui.wait_for_idle();
    assert_eq!((bounds(&mut ui, "a").left, bounds(&mut ui, "b").left), (40.0, 50.0));
    arrangement.set(Arrangement::END);
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, "a").left, 80.0);
}

/// A row to the end, then by a custom arrangement (as an object implementing Compose's
/// `Arrangement.Horizontal`), and a column aligned to its bottom.
#[test]
fn arrangements_are_per_axis_and_can_be_custom() {
    struct Reversed;
    impl HorizontalArrangementPolicy for Reversed {
        fn arrange(
            &self,
            _: &compose_ui::Density,
            total_size: i32,
            sizes: &[i32],
            _: compose_ui::LayoutDirection,
            out_positions: &mut [i32],
        ) {
            let mut end = total_size;
            for (index, size) in sizes.iter().enumerate() {
                end -= size;
                out_positions[index] = end;
            }
        }
    }
    let custom = mutable_state_of(false);
    let handle = custom.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        let arrangement = remember(|| HorizontalArrangement::custom(Reversed));
        Row!(
            modifier = Modifier.fill_max_width(),
            horizontal_arrangement =
                if handle.get() { arrangement.clone() } else { Arrangement::END },
            {
                Spacer(Modifier.size(10.dp()).test_tag("a"));
                Spacer(Modifier.size(20.dp()).test_tag("b"));
            }
        );
        Column!(
            modifier = Modifier.fill_max_size(),
            vertical_arrangement = Arrangement::aligned(Alignment::BOTTOM),
            {
                Spacer(Modifier.size(10.dp()).test_tag("c"));
            }
        );
    });
    assert_eq!((bounds(&mut ui, "a").left, bounds(&mut ui, "b").left), (70.0, 80.0));
    assert_eq!(bounds(&mut ui, "c").top, 90.0);
    custom.set(true);
    ui.wait_for_idle();
    assert_eq!((bounds(&mut ui, "a").left, bounds(&mut ui, "b").left), (90.0, 70.0));
}

#[test]
fn a_box_aligns_its_children() {
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), || {
        Box!(modifier = Modifier.size(50.dp()), content_alignment = Alignment::CENTER, |scope| {
            Spacer(Modifier.size(10.dp()).test_tag("centered"));
            Spacer(Modifier.size(10.dp()).align(scope, Alignment::BOTTOM_END).test_tag("corner"));
        });
        // A box without content: the size its modifier gives it.
        Box!(modifier = Modifier.size(5.dp()).test_tag("empty"));
    });
    assert_eq!(bounds(&mut ui, "centered"), Rect::new(20.0, 20.0, 30.0, 30.0));
    assert_eq!(bounds(&mut ui, "corner"), Rect::new(40.0, 40.0, 50.0, 50.0));
    assert_eq!(bounds(&mut ui, "empty").width(), 5.0);
}

#[test]
fn basic_text_wraps_and_limits_its_lines() {
    let mut ui = ComposeTestRule::sized(IntSize::new(70, 200), || {
        Column!({
            // 7 px per character at 14 sp: "hello brave" doesn't fit in 70.
            BasicText!("hello brave world", modifier = Modifier.test_tag("wrapped"));
            BasicText!(
                "hello brave world",
                modifier = Modifier.test_tag("one line"),
                max_lines = 1
            );
            BasicText!(
                "hello brave world",
                modifier = Modifier.test_tag("no wrap"),
                soft_wrap = false
            );
        });
    });
    assert_eq!(bounds(&mut ui, "wrapped").height(), 53.0);
    assert_eq!(bounds(&mut ui, "one line").height(), 18.0);
    assert_eq!(bounds(&mut ui, "no wrap").width(), 70.0, "constrained to the width, clipped");
}

#[composable]
fn Editor(single_line: bool) {
    let text = remember(|| mutable_state_of(String::new()));
    Column!({
        BasicTextField!(
            value = text.get(),
            on_value_change = move |value| text.set(value),
            modifier = Modifier.fill_max_width().test_tag("field"),
            single_line = single_line,
        );
        BasicText!("Echo: {}", text.get());
    });
}

#[test]
fn a_text_field_takes_input_and_keys() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 200), || Editor(false));
    ui.on_node_with_tag("field").perform_text_input("milk");
    ui.on_node_with_text("Echo: milk").assert_exists();
    ui.on_node_with_tag("field").assert_is_focused().assert_text_equals(&["milk"]);
    ui.press_key(Key::Backspace);
    ui.on_node_with_text("Echo: mil").assert_exists();
    ui.press_key(Key::DirectionLeft);
    ui.press_key(Key::DirectionLeft);
    ui.type_text("X");
    ui.on_node_with_text("Echo: mXil").assert_exists();
    ui.press_key(Key::MoveEnd);
    ui.press_key(Key::Enter);
    ui.type_text("é");
    ui.on_node_with_tag("field").assert_text_equals(&["mXil\né"]);
    ui.press_key(Key::Backspace);
    ui.press_key(Key::Backspace);
    ui.on_node_with_tag("field").assert_text_equals(&["mXil"]);
}

#[test]
fn a_tap_focuses_a_text_field_and_places_the_cursor() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 200), || Editor(true));
    ui.on_node_with_tag("field").perform_text_input("abcd");
    ui.host().clear_focus();
    // A tap between "a" and "b" (7 px per character).
    ui.on_node_with_tag("field").perform_click();
    ui.on_node_with_tag("field").assert_is_focused();
    let center = ui.on_node_with_tag("field").fetch_semantics_node().bounds;
    let _ = center;
    ui.host().send_pointer(compose_ui::PointerEvent::press(compose_ui::Offset::new(7.0, 5.0)));
    ui.host().send_pointer(compose_ui::PointerEvent::release(compose_ui::Offset::new(7.0, 5.0)));
    ui.wait_for_idle();
    ui.type_text("-\n");
    ui.on_node_with_tag("field").assert_text_equals(&["a-bcd"]);
    // Single line: Enter isn't a newline, and newlines typed are dropped.
    ui.press_key(Key::Enter);
    ui.on_node_with_tag("field").assert_text_equals(&["a-bcd"]);
}

#[test]
fn a_disabled_text_field_takes_no_input() {
    #[composable]
    fn Disabled() {
        BasicTextField!(
            value = "fixed".to_string(),
            on_value_change = |_| panic!("no edits"),
            modifier = Modifier.test_tag("field"),
            enabled = false,
        );
    }
    let mut ui = compose_test(Disabled);
    ui.on_node_with_tag("field").assert_is_not_enabled();
    ui.on_node_with_tag("field").perform_click();
    ui.type_text("x");
    ui.on_node_with_tag("field").assert_text_equals(&["fixed"]);
    let _ = Dp(0.0);
}

fn shortcut(ui: &mut ComposeTestRule, key: Key, shift: bool, command: bool) {
    let modifiers = compose_ui::KeyModifiers { shift, meta: command, ..Default::default() };
    ui.host().send_key(compose_ui::KeyEvent {
        key,
        kind: compose_ui::KeyEventType::KeyDown,
        modifiers,
    });
    ui.host().send_key(compose_ui::KeyEvent {
        key,
        kind: compose_ui::KeyEventType::KeyUp,
        modifiers,
    });
    ui.wait_for_idle();
}

#[test]
fn a_text_field_selects_copies_cuts_and_pastes() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 200), || Editor(true));
    ui.on_node_with_tag("field").perform_text_input("hello world");
    // Shift+Left five times selects "world"; typing replaces it.
    for _ in 0..5 {
        shortcut(&mut ui, Key::DirectionLeft, true, false);
    }
    ui.type_text("there");
    ui.on_node_with_tag("field").assert_text_equals(&["hello there"]);
    // Select all, copy, move to the end, paste.
    shortcut(&mut ui, Key::Character('a'), false, true);
    let display = ui.dump_display_list();
    assert!(display.contains("#664286F4"), "the selection is highlighted: {display}");
    shortcut(&mut ui, Key::Character('c'), false, true);
    shortcut(&mut ui, Key::DirectionRight, false, false);
    ui.type_text(" / ");
    shortcut(&mut ui, Key::Character('v'), false, true);
    ui.on_node_with_tag("field").assert_text_equals(&["hello there / hello there"]);
    // Cut a word, then Backspace over a selection.
    shortcut(&mut ui, Key::MoveHome, false, false);
    for _ in 0..6 {
        shortcut(&mut ui, Key::DirectionRight, true, false);
    }
    shortcut(&mut ui, Key::Character('x'), false, true);
    ui.on_node_with_tag("field").assert_text_equals(&["there / hello there"]);
    shortcut(&mut ui, Key::MoveEnd, true, false);
    shortcut(&mut ui, Key::Backspace, false, false);
    ui.on_node_with_tag("field").assert_text_equals(&[""]);
    shortcut(&mut ui, Key::Character('v'), false, true);
    ui.on_node_with_tag("field").assert_text_equals(&["hello "]);
}
