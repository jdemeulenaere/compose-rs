//! When a text keeps its paragraph as its constraints change (D550, D551): Compose's rule by
//! default (`newLayoutWillBeDifferent`: a new width bound lays it out again, a new height only
//! when it clips the paragraph), and with `set_reuse_paragraph_at_same_layout_width`, a new
//! width bound only when the width the text is laid out at changes. Each frame of a host with
//! the optimization must draw what a host without it draws.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::{FixedTextMeasurer, UiHost};
use compose_ui::{Constraints, Density, IntSize, TextAlign, TextOverflow, TextStyle};

pub(crate) const SHORT: &str = "Short label";
pub(crate) const CENTERED: &str = "Centered";
pub(crate) const CENTERED_WIDE: &str = "Centered, whole width";
pub(crate) const END_WIDE: &str = "End, whole width";
pub(crate) const WRAPPING: &str =
    "A sentence long enough to wrap onto a second line at every width the test gives it here.";
pub(crate) const ELLIPSIZED: &str =
    "A single line long enough to be ellipsized at every width the test gives it, always.";
pub(crate) const UNWRAPPED: &str = "Never wrapped, whatever the width, so it overflows when narrow";
pub(crate) const TWO_LINES: &str = "First line\nSecond";
pub(crate) const HALF_MIN: &str = "Half min";
pub(crate) const RIGHT_TO_LEFT: &str = "שלום עולם";
pub(crate) const JUSTIFIED: &str =
    "Justified text, long enough to wrap onto two or three lines at every width given here.";

fn style(align: TextAlign) -> TextStyle {
    TextStyle { text_align: Some(align), ..TextStyle::DEFAULT }
}

/// One text of each kind, in a column as wide as the root.
#[composable]
pub(crate) fn Cases() {
    Column!(modifier = Modifier.fill_max_size(), {
        BasicText!(SHORT);
        BasicText!(CENTERED, style = style(TextAlign::Center));
        BasicText!(
            CENTERED_WIDE,
            modifier = Modifier.fill_max_width(),
            style = style(TextAlign::Center)
        );
        BasicText!(END_WIDE, modifier = Modifier.fill_max_width(), style = style(TextAlign::End));
        BasicText!(WRAPPING);
        BasicText!(ELLIPSIZED, max_lines = 1, overflow = TextOverflow::Ellipsis);
        BasicText!(UNWRAPPED, soft_wrap = false);
        BasicText!(TWO_LINES);
        // At least half the root's width: wider than the text.
        BasicText!(
            HALF_MIN,
            modifier = Modifier.layout(|scope, measurable, constraints| {
                let half = Constraints::new(
                    constraints.max_width / 2,
                    constraints.max_width,
                    0,
                    constraints.max_height,
                );
                let placeable = scope.measure(measurable, half);
                scope.place(&placeable, 0, 0);
                scope.layout(placeable.width, placeable.height)
            }),
            style = style(TextAlign::Center)
        );
        BasicText!(RIGHT_TO_LEFT);
        BasicText!(JUSTIFIED, style = style(TextAlign::Justify));
    });
}

/// A rule over the fixed-advance measurer (7 px a character), counting layouts by text.
fn counted(width: i32) -> (ComposeTestRule, TextLayoutCounter) {
    let (measurer, counter) = CountingTextMeasurer::new(FixedTextMeasurer::new());
    let host = UiHost::new(IntSize::new(width, 2000), Density::new(1.0), Box::new(measurer));
    (ComposeTestRule::with_host(host, Cases), counter)
}

/// Resizes `ui` to `width` and runs its frames with the optimization `reuse` on or off.
fn resize(ui: &mut ComposeTestRule, width: i32, reuse: bool) {
    set_reuse_paragraph_at_same_layout_width(reuse);
    ui.host().resize(IntSize::new(width, 2000));
    ui.wait_for_idle();
    set_reuse_paragraph_at_same_layout_width(false);
}

/// Widths growing, shrinking past every text's own width, and back, once twice in a row.
pub(crate) const WIDTHS: [i32; 11] = [400, 390, 420, 300, 200, 150, 100, 60, 60, 400, 395];

/// Runs [`WIDTHS`] on a host without the optimization and one with it, checking each frame
/// draws the same; returns their counters (cleared after the first layout).
fn both() -> (TextLayoutCounter, TextLayoutCounter) {
    let (mut parity, parity_counts) = counted(WIDTHS[0]);
    let (mut reusing, reusing_counts) = counted(WIDTHS[0]);
    parity_counts.clear();
    reusing_counts.clear();
    for width in &WIDTHS[1..] {
        resize(&mut parity, *width, false);
        resize(&mut reusing, *width, true);
        assert_eq!(
            draw_snapshot(reusing.host()),
            draw_snapshot(parity.host()),
            "the same frame at width {width}"
        );
    }
    (parity_counts, reusing_counts)
}

#[test]
fn by_default_a_new_width_lays_every_text_out_again_as_compose() {
    let (parity, _) = both();
    // Every width but the repeated 60 is new to every text (their maximum width is the root's).
    let changes = WIDTHS.windows(2).filter(|pair| pair[0] != pair[1]).count();
    for text in [SHORT, CENTERED, WRAPPING, UNWRAPPED, TWO_LINES, RIGHT_TO_LEFT] {
        assert_eq!(parity.of(text).total(), changes, "{text:?}");
    }
}

#[test]
fn reused_paragraphs_draw_the_same_and_only_texts_that_fit_keep_theirs() {
    let (parity, reusing) = both();
    // A text that fits keeps its layout until the width falls below its own width (77 px):
    // laid out at 60, and again back at 400.
    assert_eq!(reusing.of(SHORT).total(), 2);
    assert_eq!(reusing.of(RIGHT_TO_LEFT).total(), 2);
    // Centered in its own width (56 px), which every width holds: never laid out again.
    assert_eq!(reusing.of(CENTERED).total(), 0);
    // Two lines of at most 70 px: laid out at 60 and back.
    assert_eq!(reusing.of(TWO_LINES).total(), 2);
    // Never wrapped: its width is its own, whatever the constraints.
    assert_eq!(reusing.of(UNWRAPPED).total(), 0);
    // Laid out at the whole width, wrapped, ellipsized, or wider than the text by a minimum
    // that follows the root: every new width lays them out again, as without the optimization.
    for text in [CENTERED_WIDE, END_WIDE, WRAPPING, ELLIPSIZED, JUSTIFIED] {
        assert_eq!(reusing.of(text), parity.of(text), "{text:?}");
    }
    // Half the width is more than the text's own 56 px down to 150: each of those widths lays
    // it out again. From 100 its own width wins: 60 keeps 100's layout.
    assert_eq!(reusing.of(HALF_MIN).total(), parity.of(HALF_MIN).total() - 1);
}

#[composable]
fn InBox(text: &'static str, height: MutableState<i32>, max_lines: u32) {
    Box!(
        modifier = Modifier.layout(move |scope, measurable, constraints| {
            let bounded = Constraints::new(0, constraints.max_width, 0, height.get());
            let placeable = scope.measure(measurable, bounded);
            scope.place(&placeable, 0, 0);
            scope.layout(placeable.width, placeable.height)
        }),
        {
            BasicText!(text, max_lines = max_lines);
        }
    );
}

#[test]
fn a_new_height_lays_a_text_out_again_only_when_it_clips_it_as_compose() {
    // 17.5 px lines: one line is 18 px high.
    for (text, max_lines, height_relays) in [
        ("One line", u32::MAX, [false, false, true, false]),
        // More lines than its maximum: every new height lays it out again.
        ("Cut\nlines", 1, [true, true, true, true]),
    ] {
        let owned = mutable_state_of(100);
        let height = owned.handle();
        let (measurer, counter) = CountingTextMeasurer::new(FixedTextMeasurer::new());
        let host = UiHost::new(IntSize::new(300, 300), Density::new(1.0), Box::new(measurer));
        let mut ui = ComposeTestRule::with_host(host, move || InBox(text, height, max_lines));
        for (step, (new_height, relays)) in
            [80, 18, 10, 50].into_iter().zip(height_relays).enumerate()
        {
            counter.clear();
            height.set(new_height);
            ui.wait_for_idle();
            assert_eq!(counter.of(text).total(), usize::from(relays), "{text:?}, step {step}");
        }
        // The same height again: kept.
        counter.clear();
        height.set(50);
        ui.wait_for_idle();
        assert_eq!(counter.total(), 0);
    }
}

#[test]
fn the_optimization_is_off_by_default_and_per_thread() {
    assert!(!reuse_paragraph_at_same_layout_width());
    set_reuse_paragraph_at_same_layout_width(true);
    assert!(std::thread::spawn(|| !reuse_paragraph_at_same_layout_width()).join().unwrap());
    set_reuse_paragraph_at_same_layout_width(false);
}

#[test]
fn a_new_density_or_direction_lays_the_text_out_anew_even_with_the_optimization() {
    let (mut ui, counter) = counted(400);
    set_reuse_paragraph_at_same_layout_width(true);
    counter.clear();
    ui.host().set_density(Density::new(2.0));
    ui.wait_for_idle();
    assert!(counter.of(SHORT).fresh >= 1);
    counter.clear();
    ui.host().set_layout_direction(compose_ui::LayoutDirection::Rtl);
    ui.wait_for_idle();
    assert!(counter.of(SHORT).fresh >= 1);
    set_reuse_paragraph_at_same_layout_width(false);
}

#[composable]
fn Styled(style: MutableState<TextStyle>) {
    BasicText!("Styled", style = style.get());
}

/// The paragraph the host's last frame draws.
fn drawn(ui: &mut ComposeTestRule) -> compose_ui::platform::ParagraphHandle {
    ui.host()
        .display_list()
        .ops
        .iter()
        .find_map(|op| match op {
            compose_ui::platform::DrawOp::DrawParagraph(handle, _) => Some(*handle),
            _ => None,
        })
        .expect("a paragraph drawn")
}

#[test]
fn a_paint_only_style_change_keeps_the_layout_and_draws_it_again_as_compose() {
    let owned = mutable_state_of(TextStyle::DEFAULT);
    let style = owned.handle();
    let (measurer, counter) = CountingTextMeasurer::new(FixedTextMeasurer::new());
    let host = UiHost::new(IntSize::new(300, 300), Density::new(1.0), Box::new(measurer));
    let mut ui = ComposeTestRule::with_host(host, move || Styled(style));
    let red = TextStyle { color: compose_ui::Color::RED, ..TextStyle::DEFAULT };
    let underlined =
        TextStyle { text_decoration: Some(compose_ui::TextDecoration::UNDERLINE), ..red };
    // Compose's `hasSameDrawAffectingAttributes`: drawn again in the new paint, not laid out.
    for paint in [red, underlined] {
        let before = drawn(&mut ui);
        counter.clear();
        style.set(paint);
        ui.wait_for_idle();
        assert_eq!(counter.of("Styled"), LayoutCounts { fresh: 0, relaid: 0, restyled: 1 });
        assert_ne!(drawn(&mut ui), before, "drawn again, in the new paint");
    }
    // What lays it out differently lays it out again.
    let bigger = TextStyle { font_size: compose_ui::Sp(20.0), ..underlined };
    for layout in [bigger, TextStyle { text_align: Some(TextAlign::Center), ..bigger }] {
        counter.clear();
        style.set(layout);
        ui.wait_for_idle();
        assert_eq!(counter.of("Styled"), LayoutCounts { fresh: 1, relaid: 0, restyled: 0 });
    }
}

#[test]
fn layout_affecting_attributes_are_composes() {
    let base = TextStyle::DEFAULT;
    let paint_only = TextStyle {
        color: compose_ui::Color::BLUE,
        text_decoration: Some(compose_ui::TextDecoration::LINE_THROUGH),
        ..base
    };
    assert!(base.has_same_layout_affecting_attributes(&paint_only));
    for other in [
        TextStyle { font_size: compose_ui::Sp(3.0), ..base },
        TextStyle { font_weight: Some(compose_ui::FontWeight(700)), ..base },
        TextStyle { font_style: Some(compose_ui::FontStyle::Italic), ..base },
        TextStyle { font_family: Some(compose_ui::FontFamily::Serif), ..base },
        TextStyle { letter_spacing: compose_ui::Sp(1.0), ..base },
        TextStyle { text_align: Some(TextAlign::End), ..base },
        TextStyle { line_height: compose_ui::Sp(30.0), ..base },
    ] {
        assert!(!base.has_same_layout_affecting_attributes(&other), "{other:?}");
    }
}
