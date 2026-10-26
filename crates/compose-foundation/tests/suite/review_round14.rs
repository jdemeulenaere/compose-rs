//! Review round 14: `SelectionContainer` and `DisableSelection` against Compose's
//! `SelectionContainer` (foundation's `SelectionManager`, `SelectionRegistrar`,
//! `SelectionController`). Each test states the input, what Compose does, and what happens here.
//!
//! The fixed-advance measurer: 7 px per character at 14 sp, lines 17.5 px high.

#![allow(non_snake_case)]

use std::time::Duration;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    IntSize, Key, KeyEvent, KeyEventType, KeyModifiers, Offset, PointerEvent, UnitExt,
    platform::DrawOp,
};

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

fn selected(ui: &mut ComposeTestRule) -> Option<String> {
    ui.host().with_tree(|tree| tree.selected_text())
}

/// Where character `index` of the text tagged `tag` starts (in its first line's middle).
fn at(ui: &mut ComposeTestRule, tag: &str, index: usize) -> Offset {
    let bounds = ui.on_node_with_tag(tag).fetch_semantics_node().bounds;
    Offset::new(bounds.left + ADVANCE * index as f32 + 1.0, bounds.top + 8.0)
}

fn double_click(ui: &mut ComposeTestRule, point: Offset) {
    for _ in 0..2 {
        ui.host().send_pointer(PointerEvent::press(point));
        ui.host().send_pointer(PointerEvent::release(point));
        ui.advance_time_by(Duration::from_millis(50));
    }
    ui.wait_for_idle();
}

fn drag(ui: &mut ComposeTestRule, from: Offset, to: Offset) {
    ui.host().send_pointer(PointerEvent::press(from));
    ui.host().send_pointer(PointerEvent::moved(to, true));
    ui.host().send_pointer(PointerEvent::release(to));
    ui.wait_for_idle();
}

/// Two texts side by side in a `Row` inside a `SelectionContainer` (a label and its value, a
/// table row), double-clicking the second text's word "Right". Compose's selection hit-tests
/// each selectable by its bounds (both coordinates, the selectables sorted by position), so it
/// selects "Right". Here `point_at` only looks at y: the first text whose vertical band holds
/// the pointer wins, so every press on the second text resolves to the first one (past its
/// end): nothing in the second text of a row can be selected, by drag or by double-click.
///
/// Severity: high. Texts in a `Row` are everywhere; selecting in all but the first is broken.
#[test]
fn a_double_click_on_the_second_text_of_a_row_selects_its_word() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 200), || {
        SelectionContainer!({
            Row!({
                BasicText!("Left ", modifier = Modifier.test_tag("left"));
                BasicText!("Right", modifier = Modifier.test_tag("right"));
            });
        });
    });
    let point = at(&mut ui, "right", 2);
    double_click(&mut ui, point);
    assert_eq!(selected(&mut ui).as_deref(), Some("Right"));
}

/// A `SelectionContainer` nested in another (a dialog's or a card's own container inside a
/// page-wide one), double-clicking the inner text's word "Inner". Compose's inner container
/// handles the gesture (its selection gesture consumes the press), so "Inner" is selected.
/// Here both containers see the unconsumed press and both ask to start a selection: the
/// outer's request, handled second, clears the inner's selection (`clear_other_selection`)
/// and selects at the nearest of the outer's own texts; the outer one also takes the focus.
/// Nothing in a nested container can be selected.
///
/// Severity: medium. D151 leaves nested containers' texts out of the outer one, so nesting is
/// meant to work, and a page-wide container (common on desktop) around any other breaks it.
#[test]
fn a_nested_container_selects_its_own_text() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 200), || {
        SelectionContainer!({
            Column!({
                BasicText!("Outer text", modifier = Modifier.test_tag("outer"));
                SelectionContainer!({
                    BasicText!("Inner text", modifier = Modifier.test_tag("inner"));
                });
            });
        });
    });
    let point = at(&mut ui, "inner", 2);
    double_click(&mut ui, point);
    assert_eq!(selected(&mut ui).as_deref(), Some("Inner"));
}

/// A word of the third text selected (a double-click on "Gamma"), then the first text goes
/// away (a list item removed, a notice dismissed, a lazy list's items above disposed as it
/// scrolls). Compose's selection is per selectable (by its id), so "Gamma" stays selected and
/// Cmd+C copies it. Here the selection is kept as positions in the container's order: once
/// the first text is gone, the third one's place is the fourth's, so the selection (and what
/// Cmd+C copies) moves to "Delta" while the highlight stays on "Gamma" (its node keeps it).
///
/// Severity: medium. What's copied isn't what's highlighted, after any change above it.
#[compose_runtime::composable]
fn Greek(shown: compose_runtime::MutableState<bool>) {
    SelectionContainer!({
        Column!({
            if shown.get() {
                BasicText!("Alpha");
            }
            BasicText!("Beta");
            BasicText!("Gamma", modifier = Modifier.test_tag("gamma"));
            BasicText!("Delta");
        });
    });
}

#[test]
fn removing_a_text_above_the_selection_keeps_it_on_its_text() {
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    // A composable function (region groups keep each text's node), as an app's.
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 200), move || Greek(handle));
    let point = at(&mut ui, "gamma", 1);
    double_click(&mut ui, point);
    assert_eq!(selected(&mut ui).as_deref(), Some("Gamma"));
    shown.set(false);
    ui.wait_for_idle();
    assert_eq!(selected(&mut ui).as_deref(), Some("Gamma"));
}

/// The same in a `LazyColumn` inside a `SelectionContainer`: "item 3" selected (a drag over
/// its text), then the list scrolls by three items (the items scrolled out are disposed).
/// Compose keeps the selection on item 3's selectable. Here the container's order shifts as
/// items are disposed: the selection, and what Cmd+C copies, is now "item 6".
///
/// Severity: medium. Scrolling a selectable list (logs, chat) changes what Cmd+C copies.
#[test]
fn scrolling_a_lazy_list_keeps_the_selection_on_its_item() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        SelectionContainer!({
            LazyColumn!(
                modifier = Modifier.size((100.dp(), 100.dp())),
                state = given,
                |scope: LazyListScope| {
                    scope.items(100, |index| {
                        BasicText!(
                            "item {}",
                            index,
                            modifier = Modifier.height(20.dp()).test_tag(format!("item {index}"))
                        );
                    });
                }
            );
        });
    });
    // Select "item 3" whole: from its start to past its end.
    let from = at(&mut ui, "item 3", 0);
    let to = at(&mut ui, "item 3", 9);
    drag(&mut ui, from, to);
    assert_eq!(selected(&mut ui).as_deref(), Some("item 3"));
    state.request_scroll_to_item(3, 0);
    ui.wait_for_idle();
    assert_eq!(selected(&mut ui).as_deref(), Some("item 3"));
}

/// A selected text whose content changes (a clock, a counter, a status line): "world" of
/// "Hello world" selected, then the text becomes "Goodbye". Compose's `SelectionManager`
/// clears the selection when a selected selectable's text changes
/// (`onSelectableChangeCallback`: `selection = null`). Here the old offsets stay and are
/// applied to the new text: Cmd+C copies "e" (bytes 6..11 of "Goodbye", clamped), which was
/// never selected, and the node's stale range keeps highlighting it.
///
/// Severity: medium. Copies text the user didn't select.
#[test]
fn a_selected_text_that_changes_drops_the_selection() {
    let text = mutable_state_of("Hello world");
    let handle = text.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 200), move || {
        SelectionContainer!({
            BasicText!("{}", handle.get(), modifier = Modifier.test_tag("text"));
        });
    });
    let point = at(&mut ui, "text", 7);
    double_click(&mut ui, point);
    assert_eq!(selected(&mut ui).as_deref(), Some("world"));
    text.set("Goodbye");
    ui.wait_for_idle();
    assert_eq!(selected(&mut ui), None);
}

/// The same with a text whose new content is multi-byte: "Hello world" all selected (Cmd+A),
/// then the text becomes "ééééééé" (14 bytes). Compose clears the selection. Here
/// `TextElement::update` keeps the node's selected range (0..11) and the next measure asks the
/// measurer for the highlight of bytes 0..11 of the new text: byte 11 is inside an "é", and
/// the fixed-advance measurer's `selection_rects` slices the text there and panics (the Skia
/// measurer's `utf16_index` slices the same way). A text changing under a selection crashes
/// the app.
///
/// Severity: high. A panic from ordinary use (any non-ASCII text that updates while selected).
#[test]
fn a_selected_text_changing_to_multibyte_text_does_not_panic() {
    let text = mutable_state_of("Hello world");
    let handle = text.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 200), move || {
        SelectionContainer!({
            BasicText!("{}", handle.get(), modifier = Modifier.test_tag("text"));
        });
    });
    let point = at(&mut ui, "text", 0);
    ui.host().send_pointer(PointerEvent::press(point));
    ui.host().send_pointer(PointerEvent::release(point));
    ui.wait_for_idle();
    shortcut(&mut ui, 'a');
    assert_eq!(selected(&mut ui).as_deref(), Some("Hello world"));
    text.set("ééééééé");
    ui.wait_for_idle();
    ui.on_node_with_text("ééééééé").assert_exists();
}

/// A text cut by its bounds (`soft_wrap = false` in a 35 px wide box, `overflow = Clip`, the
/// default), all selected. Compose's `SelectionController.draw` clips the highlight to the
/// text's bounds when the text is clipped (`clipRect(0, 0, width, height)` around the
/// selection path, as the text itself), so nothing is highlighted past 35 px. Here the
/// highlight rectangles are drawn before (and outside) the clip the paragraph is drawn in:
/// the highlight runs 77 px wide, over whatever is beside the text.
///
/// Severity: low. A visual leak beside every clipped, selected text.
#[test]
fn the_highlight_of_a_clipped_text_is_clipped_too() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 200), || {
        SelectionContainer!({
            BasicText!(
                "Hello world",
                modifier = Modifier.width(35.dp()).test_tag("text"),
                soft_wrap = false,
            );
        });
    });
    let point = at(&mut ui, "text", 0);
    ui.host().send_pointer(PointerEvent::press(point));
    ui.host().send_pointer(PointerEvent::release(point));
    ui.wait_for_idle();
    shortcut(&mut ui, 'a');
    let ops = ui.host().display_list().ops.clone();
    // Any highlight drawn while no clip is in effect must stay within the text's 35 px.
    let mut clips = 0usize;
    let mut saves = Vec::new();
    let mut leaking = Vec::new();
    for op in &ops {
        match op {
            DrawOp::Save => saves.push(clips),
            DrawOp::Restore => clips = saves.pop().unwrap_or(0),
            DrawOp::ClipRect(_) => clips += 1,
            DrawOp::DrawRect(rect, paint)
                if paint.color == compose_ui::Color::from_argb(0x664286F4)
                    && clips == 0
                    && rect.right > 35.5 =>
            {
                leaking.push(*rect)
            }
            _ => {}
        }
    }
    assert!(leaking.is_empty(), "highlight past the clipped text: {leaking:?}\n{ops:?}");
}

/// A double-click on "quick" then a drag right to the middle of "brown": Compose extends a
/// double-click's drag by words (`SelectionAdjustment.Word`), selecting "quick brown". Here
/// the drag after a double-click extends by characters, "quick br".
///
/// Severity: low. A desktop text-selection convention (every platform's) missing.
#[test]
fn a_drag_after_a_double_click_extends_by_words() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 200), || {
        SelectionContainer!({
            BasicText!("The quick brown fox", modifier = Modifier.test_tag("text"));
        });
    });
    let word = at(&mut ui, "text", 5);
    let to = at(&mut ui, "text", 12);
    ui.host().send_pointer(PointerEvent::press(word));
    ui.host().send_pointer(PointerEvent::release(word));
    ui.advance_time_by(Duration::from_millis(50));
    ui.host().send_pointer(PointerEvent::press(word));
    ui.host().send_pointer(PointerEvent::moved(to, true));
    ui.host().send_pointer(PointerEvent::release(to));
    ui.wait_for_idle();
    assert_eq!(selected(&mut ui).as_deref(), Some("quick brown"));
}

/// A `SelectionContainer` inside `DisableSelection` (a non-selectable screen with one
/// selectable block, an error's details say). Compose's `DisableSelection` takes the
/// registrar away and the inner `SelectionContainer` provides its own, so its texts are
/// selectable: a double-click on "Details" selects it. Here the container doesn't reset
/// `LOCAL_SELECTION_DISABLED`, so its texts are still under `DisableSelection` and nothing in
/// it can be selected.
///
/// Severity: low. Selection can't be re-enabled below a `DisableSelection`.
#[test]
fn a_container_inside_disable_selection_selects_its_texts() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 200), || {
        DisableSelection!({
            SelectionContainer!({
                BasicText!("Details here", modifier = Modifier.test_tag("details"));
            });
        });
    });
    let point = at(&mut ui, "details", 2);
    double_click(&mut ui, point);
    assert_eq!(selected(&mut ui).as_deref(), Some("Details"));
}
