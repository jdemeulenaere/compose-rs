//! Phase E review: failing tests for compose-foundation's layouts and text (docs/06-ui.md).
//! Each test states the input, what Jetpack Compose does, and what happens instead.

#![allow(non_snake_case)]

use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Constraints, Density, Dp, IntSize, Key, LayoutDirection, Measurable, MeasurePolicy,
    MeasureResult, MeasureScope, Rect, UnitExt,
};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> Rect {
    ui.on_node_with_tag(tag).fetch_semantics_node().bounds
}

fn arrange(arrangement: Arrangement, total: i32, sizes: &[i32], rtl: bool) -> Vec<i32> {
    let mut positions = vec![0; sizes.len()];
    let direction = if rtl { LayoutDirection::Rtl } else { LayoutDirection::Ltr };
    arrangement.arrange(total, sizes, direction, &Density::default(), &mut positions);
    positions
}

// ---- Row and Column ----------------------------------------------------------------------------

/// Compose's `RowColumnMeasurePolicy` measures each child without weight with the space left
/// *after the spacing* (`remaining - spaceAfterLastNoWeight`), so children fit. Here the
/// spacing is ignored when measuring: a filling second child gets 40 px (100 - 60), not 30, and
/// with the 10 px gap ends at x = 110, past the row's 100 px.
#[test]
fn a_spaced_row_leaves_room_for_its_spacing_when_measuring() {
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 50), || {
        Row!(
            modifier = Modifier.fill_max_width(),
            horizontal_arrangement = Arrangement::spaced_by(10.dp()),
            {
                Spacer(Modifier.size_wh(60.dp(), 10.dp()).test_tag("a"));
                Spacer(Modifier.fill_max_width().height(10.dp()).test_tag("b"));
            }
        );
    });
    assert_eq!(bounds(&mut ui, "b"), Rect::new(70.0, 0.0, 100.0, 10.0));
}

/// Compose sizes a row with weighted children as `max(fixedSpace + weightedSpace, minWidth)`:
/// with `weight(1f, fill = false)` a 10 px child leaves the row 10 px wide. Here any weighted
/// child makes the row take the whole maximum width (100 px).
#[test]
fn a_row_with_only_unfilled_weights_wraps_its_children() {
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 50), || {
        Row!(modifier = Modifier.test_tag("row"), |row| {
            Spacer(Modifier.weight_filled(row, 1.0, false).size(10.dp()));
        });
    });
    assert_eq!(bounds(&mut ui, "row").width(), 10.0);
}

/// Measures its children with an unbounded width, as a horizontal scroll does.
struct UnboundedWidth;

impl MeasurePolicy for UnboundedWidth {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let child = Constraints::new(0, Constraints::INFINITY, 0, constraints.max_height);
        let (mut width, mut height) = (0, 0);
        for &measurable in measurables {
            let placeable = scope.measure(measurable, child);
            scope.place(&placeable, 0, 0);
            width = width.max(placeable.width);
            height = height.max(placeable.height);
        }
        scope.layout(constraints.constrain_width(width), constraints.constrain_height(height))
    }
}

/// In an unbounded row (a horizontal scroll), Compose still measures weighted children by
/// weight, against the minimum width (`targetSpace = mainAxisMin`, here 0), so a
/// `weight(1f)` child gets exactly 0 px. Here weights are dropped when the main axis is
/// unbounded and the child takes its own 20 px.
#[test]
fn weights_in_an_unbounded_row_share_the_minimum_width() {
    thread_local! {
        static POLICY: Rc<dyn MeasurePolicy> = Rc::new(UnboundedWidth);
    }
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 50), || {
        compose_ui::Layout(
            Modifier,
            POLICY.with(Rc::clone),
            callback(|| {
                Row!(|row| {
                    Spacer(Modifier.size(10.dp()).test_tag("fixed"));
                    Spacer(Modifier.weight(row, 1.0).size(20.dp()).test_tag("weighted"));
                });
            }),
        );
    });
    assert_eq!(bounds(&mut ui, "fixed").width(), 10.0);
    assert_eq!(bounds(&mut ui, "weighted").width(), 0.0);
}

// ---- Arrangement -------------------------------------------------------------------------------

/// Compose's `spacedBy` never places a child past the end (`min(occupied, totalSize - size)`)
/// and shrinks the last gaps to what's left; its alignment applies only when the children
/// don't fill the space. Here overflowing children run past the end (`[0, 70]` for 60 + 50 in
/// 100), and a centered group starts at a negative position (`[-15, 55]`).
#[test]
fn spaced_by_keeps_overflowing_children_inside() {
    assert_eq!(arrange(Arrangement::spaced_by(10.dp()), 100, &[60, 50], false), [0, 50]);
    assert_eq!(
        arrange(
            Arrangement::spaced_by_aligned(10.dp(), SpacedAlignment::Center),
            100,
            &[60, 60],
            false
        ),
        [0, 40]
    );
}

/// Compose's `SpaceBetween` in a right-to-left row puts a single child at the end
/// (`if (reverseInput && size.size == 1) current = gapSize`). Here it stays at the start.
#[test]
fn space_between_puts_a_single_rtl_child_at_the_end() {
    assert_eq!(arrange(Arrangement::SpaceBetween, 100, &[10], true), [90]);
}

// ---- BasicText ---------------------------------------------------------------------------------

/// A text measured once with a minimum height (50 px), then without: Compose sizes a text from
/// its paragraph each time (`constraints.constrain(paragraph size)`), so it shrinks back to its
/// one line (18 px). `TextNode` keeps the paragraph when only the height constraint changes,
/// and the fixed measurer's paragraph was sized with the old minimum, so the text stays 50 px.
#[test]
fn a_text_shrinks_when_its_minimum_height_goes_away() {
    let tall = mutable_state_of(true);
    let handle = tall.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        let min = if handle.get() { 50.dp() } else { Dp::UNSPECIFIED };
        BasicText!(
            "hello",
            // A fully specified style, so the element compares equal (see the next test).
            style = TextStyle { font_size: 14.sp(), line_height: 17.5.sp(), ..TextStyle::DEFAULT },
            modifier = Modifier
                .size_in(compose_ui::SizeSpec::bounds(
                    Dp::UNSPECIFIED,
                    min,
                    Dp::UNSPECIFIED,
                    Dp::UNSPECIFIED
                ))
                .test_tag("text")
        );
    });
    assert_eq!(bounds(&mut ui, "text").height(), 50.0);
    tall.set(false);
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, "text").height(), 18.0);
}

thread_local! {
    static LAYOUTS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// The fixed measurer, counting paragraph layouts.
struct CountingMeasurer(compose_ui::platform::FixedTextMeasurer);

impl compose_ui::platform::TextMeasurer for CountingMeasurer {
    fn layout(
        &mut self,
        text: &str,
        style: &TextStyle,
        options: compose_ui::platform::TextLayoutOptions,
        constraints: Constraints,
        density: &Density,
    ) -> compose_ui::platform::ParagraphHandle {
        LAYOUTS.with(|count| count.set(count.get() + 1));
        self.0.layout(text, style, options, constraints, density)
    }
    fn metrics(
        &self,
        paragraph: compose_ui::platform::ParagraphHandle,
    ) -> compose_ui::platform::ParagraphMetrics {
        self.0.metrics(paragraph)
    }
    fn hit_test(
        &self,
        paragraph: compose_ui::platform::ParagraphHandle,
        position: compose_ui::Offset,
    ) -> compose_ui::platform::TextPosition {
        self.0.hit_test(paragraph, position)
    }
    fn cursor_rect(&self, paragraph: compose_ui::platform::ParagraphHandle, offset: usize) -> Rect {
        self.0.cursor_rect(paragraph, offset)
    }
    fn text(&self, paragraph: compose_ui::platform::ParagraphHandle) -> &str {
        self.0.text(paragraph)
    }
    fn release(&mut self, paragraph: compose_ui::platform::ParagraphHandle) {
        self.0.release(paragraph)
    }
}

/// docs/06: "a recomposition with the same text measures nothing". A `BasicText` with the
/// default style whose modifier changes (a new background color) keeps its text, style and
/// width, so Compose lays nothing out again. Here `TextElement::same` and `update` compare
/// styles with `==`, and `TextStyle::DEFAULT`'s unspecified sizes are NaN, never equal: every
/// recomposition of a default-styled `BasicText` (or `BasicTextField`) drops its paragraph and
/// lays it out again.
#[test]
fn a_default_styled_text_is_not_laid_out_again_when_only_its_modifier_changes() {
    let red = mutable_state_of(true);
    let handle = red.handle();
    let mut ui = compose_test_with(
        IntSize::new(200, 200),
        Density::default(),
        Box::new(CountingMeasurer(compose_ui::platform::FixedTextMeasurer::new())),
        move || {
            let color = if handle.get() { Color::RED } else { Color::BLUE };
            BasicText!("hello", modifier = Modifier.background(color));
        },
    );
    let before = LAYOUTS.with(std::cell::Cell::get);
    red.set(false);
    ui.wait_for_idle();
    assert_eq!(LAYOUTS.with(std::cell::Cell::get) - before, 0, "paragraph layouts");
}

// ---- BasicTextField ----------------------------------------------------------------------------

/// A field whose `on_value_change` rejects every edit (an input filter): Compose's
/// value-and-callback `BasicTextField` shows `value`, always (`textFieldValueState.copy(text =
/// value)`), so the field stays empty. Here the node keeps its own edited copy, and since
/// `value` never changes nothing replaces it: the field shows "a" while the app holds "".
#[test]
fn a_text_field_shows_its_value_when_the_app_rejects_an_edit() {
    let mut ui = compose_test(|| {
        BasicTextField!(
            value = String::new(),
            on_value_change = |_| {},
            modifier = Modifier.test_tag("field"),
        );
    });
    ui.on_node_with_tag("field").perform_text_input("a");
    ui.on_node_with_tag("field").assert_text_equals(&[""]);
}

/// The same with a length limit, the usual filter: at "abc" the app drops a fourth letter.
/// Compose shows "abc"; here "abcd".
#[test]
fn a_text_field_shows_its_value_when_the_app_truncates_an_edit() {
    #[composable]
    fn Limited() {
        let text = remember(|| mutable_state_of(String::from("abc")));
        BasicTextField!(
            value = text.get(),
            on_value_change = move |value: String| {
                if value.chars().count() <= 3 {
                    text.set(value)
                }
            },
            modifier = Modifier.test_tag("field"),
        );
    }
    let mut ui = compose_test(Limited);
    ui.on_node_with_tag("field").perform_text_input("d");
    ui.on_node_with_tag("field").assert_text_equals(&["abc"]);
}

#[composable]
fn Editor() {
    let text = remember(|| mutable_state_of(String::new()));
    BasicTextField!(
        value = text.get(),
        on_value_change = move |value| text.set(value),
        modifier = Modifier.test_tag("field"),
    );
}

/// Backspace deletes the previous grapheme cluster in Compose (`getPrecedingCharacterIndex`,
/// a `BreakIterator` character instance): a flag (two regional indicators) goes at once. Here
/// it deletes one `char`, leaving half a flag ("🇫").
#[test]
fn backspace_deletes_a_whole_grapheme() {
    let mut ui = compose_test(Editor);
    ui.on_node_with_tag("field").perform_text_input("a🇫🇷");
    ui.press_key(Key::Backspace);
    ui.on_node_with_tag("field").assert_text_equals(&["a"]);
}

/// Arrow keys move by grapheme in Compose too: Left from the end of "ae\u{301}" (an e and a
/// combining acute) lands before the e,
/// so typed text goes there. Here the cursor stops between the e and its accent.
#[test]
fn arrow_keys_move_over_a_whole_grapheme() {
    let mut ui = compose_test(Editor);
    ui.on_node_with_tag("field").perform_text_input("ae\u{301}");
    ui.press_key(Key::DirectionLeft);
    ui.type_text("x");
    ui.on_node_with_tag("field").assert_text_equals(&["axe\u{301}"]);
}

/// Compose's value-and-callback `BasicTextField` starts with `TextFieldValue(text = value)`,
/// whose selection is `TextRange.Zero`: focusing a field holding "abc" (by Tab) and typing
/// inserts at the start ("xabc"). Here the cursor starts at the end ("abcx"), an undocumented
/// deviation.
#[test]
fn a_text_field_starts_with_the_cursor_at_the_start() {
    #[composable]
    fn Prefilled() {
        let text = remember(|| mutable_state_of(String::from("abc")));
        BasicTextField!(
            value = text.get(),
            on_value_change = move |value| text.set(value),
            modifier = Modifier.test_tag("field"),
        );
    }
    let mut ui = compose_test(Prefilled);
    ui.press_key(Key::Tab);
    ui.on_node_with_tag("field").assert_is_focused();
    ui.type_text("x");
    ui.on_node_with_tag("field").assert_text_equals(&["xabc"]);
}

/// Events arrive between frames: a platform delivers a tap and the next typed text before it
/// runs a frame. Compose handles them in order, so text typed after a tap between "a" and "b"
/// goes there ("axbcd"). Here the tap only records `pending_tap`, resolved at the next
/// measurement, so the text goes in at the old cursor (the end) and the cursor moves after.
#[test]
fn text_typed_right_after_a_tap_goes_where_the_tap_put_the_cursor() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 100), Editor);
    ui.on_node_with_tag("field").perform_text_input("abcd");
    // 7 px per character: between "a" and "b".
    let host = ui.host();
    host.send_pointer(compose_ui::PointerEvent::press(compose_ui::Offset::new(7.0, 5.0)));
    host.send_pointer(compose_ui::PointerEvent::release(compose_ui::Offset::new(7.0, 5.0)));
    host.send_text("x");
    ui.wait_for_idle();
    ui.on_node_with_tag("field").assert_text_equals(&["axbcd"]);
}

/// Compose's `weight` coerces an infinite weight to `Float.MAX_VALUE`
/// (`weight.coerceAtMost(Float.MAX_VALUE)`), so `weight(Float.POSITIVE_INFINITY)` takes all
/// the remaining space next to a `weight(1f)` sibling. Here the infinite weight makes every
/// share NaN (rounded to 0) and the remainder is handed out a pixel at a time: both children
/// end up 1 px wide in a 100 px row.
#[test]
fn an_infinite_weight_takes_all_the_space() {
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 50), || {
        Row!(modifier = Modifier.fill_max_width(), |row| {
            Spacer(Modifier.weight(row, f32::INFINITY).height(10.dp()).test_tag("all"));
            Spacer(Modifier.weight(row, 1.0).height(10.dp()).test_tag("none"));
        });
    });
    assert_eq!(bounds(&mut ui, "all").width(), 100.0);
    assert_eq!(bounds(&mut ui, "none").width(), 0.0);
}
