//! Phase F review: failing tests for compose-skia's text measurer and renderer
//! (docs/07-platforms.md). Each test states the input, what Jetpack Compose (Desktop, on Skia)
//! does, and what happens instead.
//!
//! Checked and sound (no test kept): UTF-16 conversions for emoji ("a😀b") in hit tests and
//! cursors; cursors at soft-wrap points and on empty middle lines ("a\n\nb"); trailing spaces;
//! density changes and 300 counter clicks keep `SkiaTextMeasurer::live` constant; 200 rounds of
//! typing multibyte text, Enter, Backspace and adding rows in a Material form at density 2 keep
//! live paragraphs and node counts constant.

#![allow(non_snake_case)]

use compose::foundation::*;
use compose::runtime::*;
use compose::test::*;
use compose::ui::{
    Color, Constraints, Density, IntSize, Key, Offset, Paint, Rect, TextStyle,
    platform::{DisplayList, DrawOp, Renderer, TextLayoutOptions, TextMeasurer},
};
use compose_skia::{RasterRenderer, SkiaTextMeasurer};

fn layout(measurer: &mut SkiaTextMeasurer, text: &str) -> compose::ui::platform::ParagraphHandle {
    measurer.layout(
        text,
        &TextStyle::DEFAULT,
        TextLayoutOptions::default(),
        Constraints::unbounded(),
        &Density::default(),
    )
}

// ---- Cursor rectangles ----------------------------------------------------------------------

/// "a\n" has two lines, the second empty. Compose's `getCursorRect(2)` is on the second line
/// (`getLineForOffset(2)` is 1), which is where a text field draws the cursor after Enter at
/// the end of its text. Here SkParagraph's `getLineNumberAt(2)` finds no line (the empty last
/// line holds no character), the fallback is line 0, and the cursor stays on the first line
/// (top 0.3 instead of about 17).
#[test]
fn the_cursor_after_a_trailing_newline_is_on_the_new_line() {
    let mut measurer = SkiaTextMeasurer::new();
    let one_line = layout(&mut measurer, "a");
    let line_height = measurer.metrics(one_line).size.height as f32;
    let paragraph = layout(&mut measurer, "a\n");
    let cursor = measurer.cursor_rect(paragraph, 2);
    assert!(
        cursor.top >= line_height - 1.0,
        "the cursor after \"a\\n\" is on the first line: {cursor:?} (a line is {line_height} px)"
    );
}

/// The same through a text field: type "a", press Enter. The cursor Compose draws is on the
/// new, second line. Here it's drawn on the first.
#[test]
fn a_text_field_draws_the_cursor_on_the_line_enter_made() {
    #[composable]
    fn Editor() {
        let text = remember(|| mutable_state_of(String::new()));
        BasicTextField!(
            value = text.get(),
            on_value_change = move |value| text.set(value),
            modifier = Modifier.test_tag("field"),
        );
    }
    let mut ui = compose_test_with(
        IntSize::new(400, 300),
        Density::default(),
        Box::new(SkiaTextMeasurer::new()),
        Editor,
    );
    ui.on_node_with_tag("field").perform_text_input("a");
    ui.press_key(Key::Enter);
    ui.on_node_with_tag("field").assert_text_equals(&["a\n"]);
    let field = ui.on_node_with_tag("field").fetch_semantics_node().bounds;
    // The cursor is the last rectangle drawn (after the paragraph).
    let list = ui.host().display_list();
    let cursor = list
        .ops
        .iter()
        .rev()
        .find_map(|op| match op {
            DrawOp::DrawRect(rect, _) => Some(*rect),
            _ => None,
        })
        .expect("a cursor");
    let middle = (field.top + field.bottom) / 2.0;
    assert!(
        cursor.top >= middle - 1.0,
        "the cursor is on the first line of a two-line field: {cursor:?} in {field:?}"
    );
}

/// In right-to-left text the cursor before the first character is at the right edge, and the
/// one after the last character at the left edge: Compose's `getCursorRect` takes the
/// horizontal position in the run's direction (`getHorizontalPosition(offset,
/// usePrimaryDirection = true)`). Here the measurer always takes the left edge of the
/// character after the offset, or the right edge of the one before it, as for left-to-right
/// text: for "שלום" (29 px wide) the cursor at 0 is at 18.3 (inside the text, at the first
/// letter's left edge) and the one at the end at 8.4 (the last letter's right edge), instead
/// of 29 and 0. The hit test is right (x = 0 is the end), so a tap shows the cursor elsewhere.
#[test]
fn right_to_left_cursors_follow_the_text_direction() {
    let mut measurer = SkiaTextMeasurer::new();
    let text = "שלום";
    let paragraph = layout(&mut measurer, text);
    let width = measurer.metrics(paragraph).size.width as f32;
    let start = measurer.cursor_rect(paragraph, 0);
    let end = measurer.cursor_rect(paragraph, text.len());
    assert!(
        // Within a pixel: the paragraph is laid out at its width rounded up, as Compose's.
        start.left >= width - 1.5 && end.left <= 1.0,
        "start {start:?}, end {end:?} in a {width} px paragraph"
    );
    // Where a tap at the far left lands, the cursor is drawn there.
    let tapped = measurer.hit_test(paragraph, Offset::new(0.5, 5.0)).offset;
    assert_eq!(tapped, text.len());
}

// ---- Measured width ---------------------------------------------------------------------------

/// A paragraph that wraps takes the whole width it was given in Compose:
/// `ParagraphLayoutCache` lays it out at `maxIntrinsicWidth.coerceIn(minWidth, maxWidth)` and
/// the layout's width is the paragraph's (`paragraph.width`, the constraint), not its longest
/// line. So "hello brave new world" in 60 px is 60 px wide, and a `Text` that wraps in a `Row`
/// pushes what follows to the edge. Here the size is the longest line (36 px), and the
/// comment claims this is Compose's behaviour. (`FixedTextMeasurer` does the same.)
#[test]
fn a_wrapped_paragraph_takes_the_width_it_was_given() {
    let mut measurer = SkiaTextMeasurer::new();
    let paragraph = measurer.layout(
        "hello brave new world",
        &TextStyle::DEFAULT,
        TextLayoutOptions::default(),
        Constraints::at_most(60, 1000),
        &Density::default(),
    );
    let metrics = measurer.metrics(paragraph);
    assert!(metrics.line_count > 1, "{metrics:?}");
    assert_eq!(metrics.size.width, 60, "{metrics:?}");
}

// ---- The renderer -----------------------------------------------------------------------------

/// A renderer is kept from frame to frame (the desktop window keeps one), and Skia's canvas
/// keeps its save stack across them. Compose starts every frame from a fresh canvas state, so
/// one frame's unbalanced `save` (a custom draw modifier that forgets its `restore`) can't
/// reach the next. Here `render` doesn't restore to the count it started at: its closing
/// `restore` pops the stray save instead of its own, so its scale stays on the canvas and
/// every later frame is scaled again (2, then 4, then 8 on a HiDPI window).
#[test]
fn an_unbalanced_frame_does_not_scale_the_next_ones() {
    let mut renderer = RasterRenderer::new(100, 100);
    let measurer = SkiaTextMeasurer::new();
    let mut leaky = DisplayList::default();
    leaky.push(DrawOp::Save);
    leaky.push(DrawOp::ClipRect(Rect::new(0.0, 0.0, 10.0, 10.0)));
    renderer.render(&leaky, Color::WHITE, 2.0, &measurer);
    // 20 layout px at scale 2: device pixels 0 to 40.
    let mut next = DisplayList::default();
    next.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 20.0, 20.0), Paint::fill(Color::BLACK)));
    renderer.render(&next, Color::WHITE, 2.0, &measurer);
    assert_eq!(renderer.pixel(30, 30), Color::BLACK);
    assert_eq!(renderer.pixel(60, 60), Color::WHITE, "the next frame is drawn at scale 4");
}

/// The other way round: a `restore` without its `save` pops the renderer's own scale, so the
/// rest of that frame draws at scale 1 on a HiDPI surface. Compose's canvas ignores an
/// unmatched restore (`restoreToCount` keeps the base level).
#[test]
fn an_unmatched_restore_keeps_the_scale() {
    let mut renderer = RasterRenderer::new(100, 100);
    let measurer = SkiaTextMeasurer::new();
    let mut list = DisplayList::default();
    list.push(DrawOp::Restore);
    list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 40.0, 40.0), Paint::fill(Color::BLACK)));
    renderer.render(&list, Color::WHITE, 2.0, &measurer);
    assert_eq!(renderer.pixel(60, 60), Color::BLACK, "drawn at scale 1");
}
