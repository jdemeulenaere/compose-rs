//! Phase F review: failing tests for compose-foundation's text field as the desktop host drives
//! it (docs/06-ui.md, docs/07-platforms.md). Each test states the input, what Jetpack Compose
//! does, and what happens instead.

#![allow(non_snake_case)]

extern crate compose_runtime as compose;

use std::cell::Cell;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, Key, KeyEvent, UiHost, UnitExt};

/// What compose-desktop does for a key that also types text (input.rs): the key down goes to
/// the focused node and its ancestors, then the key's text is committed (since the fix,
/// whether or not the key was consumed: a focused text field consumes typed keys itself), then
/// the key up.
fn type_like_the_desktop(host: &mut UiHost, key: Key, text: &str) {
    host.send_key(KeyEvent::down(key));
    host.send_text(text);
    host.send_key(KeyEvent::up(key));
}

fn type_word(ui: &mut ComposeTestRule, word: &str) {
    for c in word.chars() {
        let key = if c == ' ' { Key::Space } else { Key::Character(c) };
        type_like_the_desktop(ui.host(), key, &c.to_string());
        ui.wait_for_idle();
    }
}

thread_local! {
    static CLICKS: Cell<u32> = const { Cell::new(0) };
    static SHORTCUTS: Cell<u32> = const { Cell::new(0) };
}

/// A text field in a clickable container (a list row, a card that opens on click). In
/// Compose the focused text field handles a typed key itself (`TextFieldKeyInput` consumes the
/// key down of a typed character), so its ancestors never see it: typing "a b" types "a b",
/// and the row doesn't click. Here the field ignores key events for typed characters and waits
/// for the text, so the Space key down travels on to the clickable ancestor, which consumes it
/// (Space clicks a focused clickable) and clicks on the key up; the desktop host then commits
/// no text. The field holds "ab" and the row clicked.
#[test]
fn space_types_a_space_in_a_field_inside_a_clickable() {
    #[composable]
    fn Row() {
        let text = remember(|| mutable_state_of(String::new()));
        Box!(
            modifier = Modifier.clickable(|| CLICKS.with(|clicks| clicks.set(clicks.get() + 1))),
            {
                BasicTextField!(
                    value = text.get(),
                    on_value_change = move |value| text.set(value),
                    modifier = Modifier.size_wh(200.dp(), 40.dp()).test_tag("field"),
                );
            }
        );
    }
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 100), Row);
    // The clickable merges the field's semantics: find it in the unmerged tree.
    ui.on_node_unmerged(has_test_tag("field")).request_focus();
    type_word(&mut ui, "a b");
    let typed = ui.on_node_unmerged(has_test_tag("field")).fetch_semantics_node();
    let typed: Vec<&str> = typed.config.all_text().collect();
    assert_eq!((typed, CLICKS.with(Cell::get)), (vec!["a b"], 0), "the text, and the row's clicks");
}

/// A screen with single-key shortcuts (`j`/`k` to move through a list, as mail apps have) on
/// an ancestor's `on_key_event`. Compose: typing "jk" into a focused field types "jk"; the
/// shortcuts don't fire, as the field consumed the keys. Here the keys reach the ancestor's
/// handler first, which consumes them, and the desktop host commits no text: the field stays
/// empty and the shortcut fires twice.
#[test]
fn an_ancestors_shortcut_does_not_take_keys_typed_into_a_field() {
    #[composable]
    fn Screen() {
        let text = remember(|| mutable_state_of(String::new()));
        Column!(
            modifier = Modifier.on_key_event(|event: KeyEvent| {
                if matches!(event.key, Key::Character('j' | 'k')) {
                    SHORTCUTS.with(|count| count.set(count.get() + 1));
                    true
                } else {
                    false
                }
            }),
            {
                BasicTextField!(
                    value = text.get(),
                    on_value_change = move |value| text.set(value),
                    modifier = Modifier.size_wh(200.dp(), 40.dp()).test_tag("field"),
                );
            }
        );
    }
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 100), Screen);
    ui.on_node_with_tag("field").request_focus();
    type_word(&mut ui, "jk");
    ui.on_node_with_tag("field").assert_text_equals(&["jk"]);
    assert_eq!(SHORTCUTS.with(Cell::get), 0);
}

/// End moves to the end of the *visual* line in Compose's text field
/// (`TextFieldPreparedSelection.moveCursorToLineEnd`: `layoutResult.getLineEnd` of the line the
/// cursor is on), so in a field 7 characters wide holding "hello world" (wrapped as "hello " /
/// "world"), End from the start goes to the end of "hello". Here Home and End look for '\n'
/// only, so End goes to the end of the whole text and the typed "!" lands after "world".
#[test]
fn end_goes_to_the_end_of_the_wrapped_line() {
    #[composable]
    fn Narrow() {
        let text = remember(|| mutable_state_of(String::from("hello world")));
        BasicTextField!(
            value = text.get(),
            on_value_change = move |value| text.set(value),
            // 7 px per character (the fixed measurer at 14 sp), and the 2 px cursor.
            modifier = Modifier.width(51.dp()).test_tag("field"),
        );
    }
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 100), Narrow);
    ui.on_node_with_tag("field").request_focus();
    ui.press_key(Key::MoveEnd);
    ui.type_text("!");
    ui.on_node_with_tag("field").assert_text_equals(&["hello! world"]);
}

/// Compose's text field is a semantics node of its own: `CoreTextField` applies
/// `Modifier.semantics(mergeDescendants = true)`, so a clickable row around it (which merges its
/// descendants) doesn't swallow it. `onNodeWithTag("field")` finds the field, and the row's
/// merged node is neither editable nor disabled because a disabled field sits in it. Here
/// `BasicTextField` doesn't merge, so the clickable row takes it over: the field can't be found
/// in the merged tree (what tests and accessibility read), and the row reports itself editable,
/// with the field's text, and disabled.
#[test]
fn a_text_field_in_a_clickable_row_is_a_node_of_its_own() {
    #[composable]
    fn Row() {
        Box!(modifier = Modifier.clickable(|| {}).test_tag("row"), {
            BasicTextField!(
                value = String::from("note"),
                on_value_change = |_| {},
                modifier = Modifier.size_wh(200.dp(), 40.dp()).test_tag("field"),
                enabled = false,
            );
        });
    }
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 100), Row);
    let row = ui.on_node_with_tag("row").fetch_semantics_node().config;
    assert!(!row.editable && !row.disabled, "the row took the field over: {row:?}");
    ui.on_node_with_tag("field").assert_exists();
}

// ---- Overflow ----------------------------------------------------------------------------------

/// The bounds of every drawing operation of the last frame, in the root, with the clip in force
/// (translations followed, clips intersected): what would reach the screen.
fn painted(list: &compose_ui::DisplayList) -> Vec<(compose_ui::Rect, compose_ui::Rect)> {
    use compose_ui::{DrawOp, Rect};
    let everything = Rect::new(-1e9, -1e9, 1e9, 1e9);
    let (mut x, mut y, mut clip) = (0.0f32, 0.0f32, everything);
    let mut saved = Vec::new();
    let mut out = Vec::new();
    let intersect = |a: Rect, b: Rect| {
        Rect::new(
            a.left.max(b.left),
            a.top.max(b.top),
            a.right.min(b.right),
            a.bottom.min(b.bottom),
        )
    };
    let moved = |rect: &Rect, x: f32, y: f32| {
        Rect::new(rect.left + x, rect.top + y, rect.right + x, rect.bottom + y)
    };
    for op in &list.ops {
        match op {
            DrawOp::Save => saved.push(clip),
            DrawOp::Restore => clip = saved.pop().unwrap_or(everything),
            DrawOp::Translate(offset) => {
                x += offset.x;
                y += offset.y;
            }
            DrawOp::ClipRect(rect) | DrawOp::ClipRoundRect(rect, _) => {
                clip = intersect(clip, moved(rect, x, y))
            }
            DrawOp::DrawRect(rect, _) | DrawOp::DrawRoundRect(rect, _, _) => {
                out.push((moved(rect, x, y), clip))
            }
            DrawOp::DrawParagraph(_, offset) => {
                let at = Rect::new(offset.x + x, offset.y + y, offset.x + x, offset.y + y);
                out.push((at, clip))
            }
            DrawOp::DrawLine(..) => {}
        }
    }
    out
}

/// A text that doesn't fit is clipped to its bounds in Compose: `BasicText`'s default
/// `overflow` is `TextOverflow.Clip`, and its node clips its drawing when the paragraph
/// overflowed (`TextStringSimpleNode.draw`: `if (willClip) clipRect(…)`). So a long line with
/// `soft_wrap = false` in a 50 px column draws nothing right of x = 50, and the text beside it
/// stays readable. Here nothing clips the paragraph: it is laid out 203 px wide and drawn whole,
/// over whatever is to its right.
#[test]
fn text_that_overflows_is_clipped_to_its_bounds() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 100), || {
        Row!({
            Box!(modifier = Modifier.width(50.dp()), {
                BasicText!("a long line that will not fit", soft_wrap = false);
            });
            BasicText!("beside");
        });
    });
    let list = ui.host().display_list();
    let (_, clip) = painted(&list)[0];
    assert!(clip.right <= 50.0, "the first paragraph is drawn unclipped: {clip:?}");
}

/// A single-line text field scrolls its text in Compose: it lays the text out unbounded and
/// scrolls it horizontally so the cursor stays visible (`TextFieldScrollerPosition`), clipping
/// it to the field. Typing a long title into a 50 px field (the todo app's draft field, next to
/// its Add button) keeps the cursor inside the field. Here there is no scrolling and no clip:
/// the text and the cursor run out of the field (the cursor is drawn at x = 133).
#[test]
fn a_single_line_field_keeps_its_cursor_in_view() {
    #[composable]
    fn Narrow() {
        let text = remember(|| mutable_state_of(String::new()));
        BasicTextField!(
            value = text.get(),
            on_value_change = move |value| text.set(value),
            modifier = Modifier.width(50.dp()).test_tag("field"),
            single_line = true,
        );
    }
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 100), Narrow);
    ui.on_node_with_tag("field").perform_text_input("a long title to add");
    let field = ui.on_node_with_tag("field").fetch_semantics_node().bounds;
    let list = ui.host().display_list();
    // The cursor is the last rectangle drawn.
    let (cursor, clip) = *painted(&list).last().expect("a cursor");
    let visible = cursor.left.max(clip.left) < cursor.right.min(clip.right);
    assert!(
        visible && cursor.left >= field.left && cursor.right <= field.right,
        "the cursor {cursor:?} (clipped to {clip:?}) is outside the field {field:?}"
    );
}

/// Tab in a multi-line field types a tab in Compose (`TextFieldKeyInput`: `KeyCommand.TAB`
/// commits "\t" unless the field is single-line, which lets Tab move focus). Here the field
/// never takes Tab, so the host moves focus to the next field and the first stays empty.
#[test]
fn tab_types_a_tab_in_a_multi_line_field() {
    #[composable]
    fn Two() {
        let first = remember(|| mutable_state_of(String::new()));
        let second = remember(|| mutable_state_of(String::new()));
        Column!({
            BasicTextField!(
                value = first.get(),
                on_value_change = move |value| first.set(value),
                modifier = Modifier.size_wh(200.dp(), 40.dp()).test_tag("first"),
            );
            BasicTextField!(
                value = second.get(),
                on_value_change = move |value| second.set(value),
                modifier = Modifier.size_wh(200.dp(), 40.dp()).test_tag("second"),
            );
        });
    }
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 100), Two);
    ui.on_node_with_tag("first").request_focus();
    ui.press_key(Key::Tab);
    ui.on_node_with_tag("first").assert_is_focused().assert_text_equals(&["\t"]);
}
