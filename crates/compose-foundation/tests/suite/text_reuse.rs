//! When a text keeps its paragraph as its constraints change (D550, D551): Compose's rule
//! without the optimization (`newLayoutWillBeDifferent`: a new width bound lays it out again, a
//! new height only when it clips the paragraph), and with the host's
//! `REUSE_PARAGRAPH_AT_SAME_LAYOUT_WIDTH` (on by default, D610), a new width bound only when the
//! width the text is laid out at changes. Each frame of a host with
//! the optimization must draw what a host without it draws.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::{FixedTextMeasurer, UiHost};
use compose_ui::{
    Constraints, Density, IntSize, TextAlign, TextOverflow, TextStyle, UnitExt,
    remember_text_measurer,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

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

/// Turns compose-rs's `REUSE_PARAGRAPH_AT_SAME_LAYOUT_WIDTH` on or off on `host` (D551).
fn reuse_at_same_width(host: &mut compose_ui::platform::UiHost, on: bool) {
    let optimizations = host
        .optimizations()
        .with(compose_ui::platform::Optimization::REUSE_PARAGRAPH_AT_SAME_LAYOUT_WIDTH, on);
    host.set_optimizations(optimizations);
}

/// A rule over the fixed-advance measurer (7 px a character), counting layouts by text.
fn counted(width: i32) -> (ComposeTestRule, TextLayoutCounter) {
    let (measurer, counter) = CountingTextMeasurer::new(FixedTextMeasurer::new());
    let host = UiHost::new(IntSize::new(width, 2000), Density::new(1.0), Box::new(measurer));
    (ComposeTestRule::with_host(host, Cases), counter)
}

/// Resizes `ui` to `width` and runs its frames with the optimization `reuse` on or off.
fn resize(ui: &mut ComposeTestRule, width: i32, reuse: bool) {
    reuse_at_same_width(ui.host(), reuse);
    ui.host().resize(IntSize::new(width, 2000));
    ui.wait_for_idle();
    reuse_at_same_width(ui.host(), false);
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
fn without_the_optimization_a_new_width_lays_every_text_out_again_as_compose() {
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
fn the_optimization_is_on_by_default_and_per_host() {
    use compose_ui::platform::{Optimization, Optimizations};
    let flag = Optimization::REUSE_PARAGRAPH_AT_SAME_LAYOUT_WIDTH;
    // In the recommended set, the default (D610).
    assert!(Optimizations::defaults().contains(flag));
    let (mut ui, _) = counted(400);
    let (mut other, _) = counted(400);
    assert!(ui.host().optimizations().contains(flag));
    reuse_at_same_width(ui.host(), false);
    assert!(!ui.host().optimizations().contains(flag));
    assert!(other.host().optimizations().contains(flag));
}

#[test]
fn a_new_density_or_direction_lays_the_text_out_anew_even_with_the_optimization() {
    let (mut ui, counter) = counted(400);
    reuse_at_same_width(ui.host(), true);
    counter.clear();
    ui.host().set_density(Density::new(2.0));
    ui.wait_for_idle();
    assert!(counter.of(SHORT).fresh >= 1);
    counter.clear();
    ui.host().set_layout_direction(compose_ui::LayoutDirection::Rtl);
    ui.wait_for_idle();
    assert!(counter.of(SHORT).fresh >= 1);
    reuse_at_same_width(ui.host(), false);
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
        TextStyle { text_decoration: Some(compose_ui::TextDecoration::UNDERLINE), ..red.clone() };
    // Compose's `hasSameDrawAffectingAttributes`: drawn again in the new paint, not laid out.
    for paint in [red, underlined.clone()] {
        let before = drawn(&mut ui);
        counter.clear();
        style.set(paint);
        ui.wait_for_idle();
        assert_eq!(
            counter.of("Styled"),
            LayoutCounts { fresh: 0, relaid: 0, restyled: 1, probes: 0 }
        );
        assert_ne!(drawn(&mut ui), before, "drawn again, in the new paint");
    }
    // What lays it out differently lays it out again.
    let bigger = TextStyle { font_size: compose_ui::TextUnit::sp(20.0), ..underlined };
    for layout in [bigger.clone(), TextStyle { text_align: Some(TextAlign::Center), ..bigger }] {
        counter.clear();
        style.set(layout);
        ui.wait_for_idle();
        assert_eq!(
            counter.of("Styled"),
            LayoutCounts { fresh: 1, relaid: 0, restyled: 0, probes: 0 }
        );
    }
}

#[test]
fn layout_affecting_attributes_are_composes() {
    let base = TextStyle::DEFAULT;
    let paint_only = TextStyle {
        color: compose_ui::Color::BLUE,
        text_decoration: Some(compose_ui::TextDecoration::LINE_THROUGH),
        ..base.clone()
    };
    assert!(base.has_same_layout_affecting_attributes(&paint_only));
    for other in [
        TextStyle { font_size: compose_ui::TextUnit::sp(3.0), ..base.clone() },
        TextStyle { font_weight: Some(compose_ui::FontWeight::new(700)), ..base.clone() },
        TextStyle { font_style: Some(compose_ui::FontStyle::Italic), ..base.clone() },
        TextStyle { font_family: Some(compose_ui::FontFamily::Serif), ..base.clone() },
        TextStyle { letter_spacing: compose_ui::TextUnit::sp(1.0), ..base.clone() },
        TextStyle { text_align: Some(TextAlign::End), ..base.clone() },
        TextStyle { line_height: compose_ui::TextUnit::sp(30.0), ..base.clone() },
    ] {
        assert!(!base.has_same_layout_affecting_attributes(&other), "{other:?}");
    }
}

/// A host counting its layouts, whose one node draws `draw` on each frame `tick` asks.
fn counted_drawing(
    tick: compose_runtime::MutableState<u32>,
    draw: impl Fn(&mut compose_ui::DrawScope<'_>, &compose_ui::TextMeasurer, u32) + 'static,
) -> (UiHost, compose_test::TextLayoutCounter) {
    let (measurer, counter) = CountingTextMeasurer::new(FixedTextMeasurer::new());
    let mut host = UiHost::new(IntSize::new(400, 400), Density::new(1.0), Box::new(measurer));
    let draw = std::rc::Rc::new(draw);
    host.set_content(move || {
        let measurer = remember_text_measurer!();
        let draw = draw.clone();
        Box!(
            modifier = Modifier.fill_max_size().draw_behind(move |scope| {
                draw(scope, &measurer, tick.get());
            })
        );
    });
    host.frame();
    (host, counter)
}

fn colored(step: u32) -> compose_ui::Color {
    compose_ui::Color::from_argb(0xFF00_0000 | step)
}

#[test]
fn drawing_a_measured_text_in_a_new_color_keeps_its_layout() {
    let owner = compose_runtime::mutable_state_of(1u32);
    let tick = owner.handle();
    let (mut host, counter) = counted_drawing(tick, |scope, measurer, step| {
        scope.draw_text(
            measurer,
            "Hello",
            compose_ui::DrawTextOptions {
                style: TextStyle { color: colored(step), ..TextStyle::DEFAULT },
                ..Default::default()
            },
        );
    });
    for step in 2..6 {
        tick.set(step);
        host.frame();
    }
    let counts = counter.of("Hello");
    assert_eq!(counts.fresh, 1, "{counts:?}");
    assert_eq!(counts.restyled, 4, "{counts:?}");
}

#[test]
fn drawing_a_layout_again_in_the_same_paint_paints_nothing_new() {
    let owner = compose_runtime::mutable_state_of(1u32);
    let tick = owner.handle();
    let (mut host, counter) = counted_drawing(tick, |scope, measurer, _| {
        let result = measurer.measure("Hello", compose_ui::TextMeasureOptions::default());
        let gradient = compose_ui::Brush::linear_gradient(vec![
            compose_ui::Color::RED,
            compose_ui::Color::BLUE,
        ]);
        // NaN's alpha, a gradient and a solid brush: each the same paint again the next time.
        scope.draw_text_layout_with_brush(&result, gradient, Default::default());
    });
    for step in 2..6 {
        tick.set(step);
        host.frame();
    }
    let counts = counter.of("Hello");
    assert_eq!((counts.fresh, counts.restyled), (1, 1), "{counts:?}");
}

#[test]
fn a_solid_brush_is_the_same_paint_again_the_next_time() {
    let owner = compose_runtime::mutable_state_of(1u32);
    let tick = owner.handle();
    let (mut host, counter) = counted_drawing(tick, |scope, measurer, _| {
        let result = measurer.measure("Hello", compose_ui::TextMeasureOptions::default());
        let solid = compose_ui::Brush::solid(compose_ui::Color::RED);
        scope.draw_text_layout_with_brush(&result, solid, Default::default());
    });
    for step in 2..6 {
        tick.set(step);
        host.frame();
    }
    let counts = counter.of("Hello");
    assert_eq!((counts.fresh, counts.restyled), (1, 1), "{counts:?}");
}

#[test]
fn a_few_colors_drawn_in_turn_paint_each_once() {
    let owner = compose_runtime::mutable_state_of(0u32);
    let tick = owner.handle();
    let (mut host, counter) = counted_drawing(tick, |scope, measurer, step| {
        let result = measurer.measure("Hello", compose_ui::TextMeasureOptions::default());
        scope.draw_text_layout(&result, colored(step % 3 + 1), Default::default());
    });
    for step in 1..30 {
        tick.set(step);
        host.frame();
    }
    let counts = counter.of("Hello");
    assert_eq!((counts.fresh, counts.restyled), (1, 3), "{counts:?}");
}

#[test]
fn more_colors_than_it_keeps_evict_the_least_recently_used() {
    let owner = compose_runtime::mutable_state_of(1u32);
    let tick = owner.handle();
    let (mut host, counter) = counted_drawing(tick, |scope, measurer, step| {
        let result = measurer.measure("Hello", compose_ui::TextMeasureOptions::default());
        scope.draw_text_layout(&result, colored(step), Default::default());
    });
    let drawn = [2, 3, 4, 1, 5, 1, 2];
    for step in drawn {
        tick.set(step);
        host.frame();
    }
    // 1 to 4; 1 is found and is the most recent; 5 takes 2's place; 1 is found; 2 is made again.
    let counts = counter.of("Hello");
    assert_eq!((counts.fresh, counts.restyled), (1, 6), "{counts:?}");
}

/// A text keeping a measurer's result, which `offset_at` and `set_selection` ask.
struct QueryingSelectable {
    result: compose_ui::TextLayoutResult,
    measurer: compose_ui::TextMeasurer,
    asked: Rc<Cell<u32>>,
}

impl compose_ui::Selectable for QueryingSelectable {
    fn text(&self) -> compose_ui::SharedString {
        compose_ui::SharedString::from("Hello")
    }
    fn offset_at(
        &self,
        position: compose_ui::Offset,
        _: &compose_ui::platform::SharedTextMeasurer,
    ) -> Option<usize> {
        self.asked.set(self.asked.get() + 1);
        // Both reach the measurer the tree shares.
        let _ = self.measurer.measure("Again", Default::default());
        Some(self.result.get_offset_for_position(position))
    }
    fn word_at(&self, _: usize) -> std::ops::Range<usize> {
        0..5
    }
    fn set_selection(
        &mut self,
        _: Option<std::ops::Range<usize>>,
        measurer: &compose_ui::platform::SharedTextMeasurer,
    ) -> bool {
        self.asked.set(self.asked.get() + 1);
        let _ = self.result.get_line_end(0, false);
        let _ = measurer.ptr_eq(measurer);
        false
    }
}

impl compose_ui::ModifierNode for QueryingSelectable {
    fn as_selectable(&mut self) -> Option<&mut dyn compose_ui::Selectable> {
        Some(self)
    }
}

struct QueryingElement {
    measurer: compose_ui::TextMeasurer,
    asked: Rc<Cell<u32>>,
}

impl compose_ui::ModifierNodeElement for QueryingElement {
    type Node = QueryingSelectable;
    fn create(&self) -> QueryingSelectable {
        QueryingSelectable {
            result: self.measurer.measure("Hello", Default::default()),
            measurer: self.measurer.clone(),
            asked: self.asked.clone(),
        }
    }
    fn update(&self, _: &mut QueryingSelectable) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
    fn name(&self) -> &'static str {
        "QueryingSelectable"
    }
}

#[test]
fn a_selectable_may_measure_and_query_a_result_when_asked() {
    let asked = Rc::new(Cell::new(0));
    let given = asked.clone();
    let mut host = UiHost::headless(200, 100);
    host.set_content(move || {
        let measurer = remember_text_measurer!();
        let given = given.clone();
        SelectionContainer!({
            let element = QueryingElement { measurer: measurer.clone(), asked: given.clone() };
            Box!(modifier = Modifier.size((200.0.dp(), 100.0.dp())).then_custom(element));
        });
    });
    host.frame();
    let at = compose_ui::Offset::new(10.0, 10.0);
    host.send_pointer(compose_ui::PointerEvent::press(at));
    host.send_pointer(compose_ui::PointerEvent::release(at));
    host.frame();
    assert!(asked.get() > 0, "the selection asked it nothing");
}

// ---- A direction left unspecified is the layout's, for all text ------------------------------

/// The paragraph direction `text` is laid out in under `direction`, through `BasicText` and
/// through a measurer: the two paths lay out the same.
fn directions_of(
    text: &'static str,
    direction: compose_ui::LayoutDirection,
) -> (compose_ui::ResolvedTextDirection, compose_ui::ResolvedTextDirection) {
    let heard: Rc<RefCell<Option<compose_ui::TextLayoutResult>>> = Rc::default();
    let measured: Rc<RefCell<Option<compose_ui::TextLayoutResult>>> = Rc::default();
    let (keep, keep_measured) = (heard.clone(), measured.clone());
    let mut host = UiHost::headless(400, 400);
    host.set_content(move || {
        let (keep, keep_measured) = (keep.clone(), keep_measured.clone());
        CompositionLocalProvider(compose_ui::LOCAL_LAYOUT_DIRECTION.provides(direction), || {
            let keep = keep.clone();
            let measurer = remember_text_measurer!();
            *keep_measured.borrow_mut() = Some(measurer.measure(text, Default::default()));
            BasicText!(
                text,
                on_text_layout =
                    move |result: compose_ui::TextLayoutResult| *keep.borrow_mut() = Some(result)
            );
        });
    });
    host.frame();
    let heard = heard.borrow().clone().expect("laid out").get_paragraph_direction(0);
    let measured = measured.borrow().clone().expect("measured").get_paragraph_direction(0);
    (heard, measured)
}

#[test]
fn an_unspecified_direction_is_the_layouts_through_basic_text_and_a_measurer() {
    use compose_ui::{LayoutDirection as Layout, ResolvedTextDirection as Resolved};
    assert_eq!(directions_of("Latin", Layout::Rtl), (Resolved::Rtl, Resolved::Rtl));
    assert_eq!(directions_of("Latin", Layout::Ltr), (Resolved::Ltr, Resolved::Ltr));
    assert_eq!(directions_of("\u{5E9}\u{5DC}", Layout::Ltr), (Resolved::Ltr, Resolved::Ltr));
    assert_eq!(directions_of("\u{5E9}\u{5DC}", Layout::Rtl), (Resolved::Rtl, Resolved::Rtl));
}

// ---- A font arriving ---------------------------------------------------------------------------

/// A font that arrives when `answer` is given a typeface.
struct ArrivingFont {
    typeface: std::rc::Rc<RefCell<Option<compose_ui::Typeface>>>,
}

impl compose_ui::TypefaceLoader for ArrivingFont {
    fn load_blocking(
        &self,
        _: &dyn compose_ui::platform::PlatformFontLoader,
        _: &compose_ui::Font,
    ) -> Result<Option<compose_ui::Typeface>, String> {
        Err("it's async".to_string())
    }

    fn await_load(
        self: Rc<Self>,
        _: Rc<dyn compose_ui::platform::PlatformFontLoader>,
        _: compose_ui::Font,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<compose_ui::Typeface>, String>>>,
    > {
        Box::pin(async move {
            loop {
                if let Some(typeface) = self.typeface.borrow_mut().take() {
                    return Ok(Some(typeface));
                }
                compose_runtime::delay(std::time::Duration::from_millis(10)).await;
            }
        })
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntrinsicsAsyncTypefaceTest.kt, hasStaleResolvedFonts_trueOnTypefaceUpdate_mainTypeface
#[test]
fn a_text_is_laid_out_again_when_its_font_arrives() {
    let arriving: Rc<RefCell<Option<compose_ui::Typeface>>> = Rc::default();
    let font = compose_ui::Font::from_loader(
        Rc::new(ArrivingFont { typeface: arriving.clone() }),
        compose_ui::FontOptions {
            loading_strategy: Some(compose_ui::FontLoadingStrategy::Async),
            ..Default::default()
        },
    );
    let family = font.to_font_family();
    let (measurer, counter) = CountingTextMeasurer::new(FixedTextMeasurer::new());
    let mut host = UiHost::new(IntSize::new(400, 400), Density::new(1.0), Box::new(measurer));
    let style = TextStyle { font_family: Some(family), ..TextStyle::DEFAULT };
    host.set_content(move || {
        BasicText!("Hello", style = style.clone());
    });
    host.frame();
    assert_eq!(counter.of("Hello").fresh, 1);
    // Nothing yet: frames lay nothing out.
    std::thread::sleep(std::time::Duration::from_millis(20));
    host.frame();
    assert_eq!(counter.of("Hello").fresh, 1);
    *arriving.borrow_mut() = Some(compose_ui::Typeface::new("arrived"));
    std::thread::sleep(std::time::Duration::from_millis(20));
    host.frame();
    host.frame();
    assert_eq!(counter.of("Hello").fresh, 2);
    // It's what shows from now on: no more layouts.
    host.frame();
    assert_eq!(counter.of("Hello").fresh, 2);
}

#[test]
fn a_measurers_cached_layout_is_not_found_when_its_font_arrived() {
    let arriving: Rc<RefCell<Option<compose_ui::Typeface>>> = Rc::default();
    let font = compose_ui::Font::from_loader(
        Rc::new(ArrivingFont { typeface: arriving.clone() }),
        compose_ui::FontOptions {
            loading_strategy: Some(compose_ui::FontLoadingStrategy::Async),
            ..Default::default()
        },
    );
    let shared = compose_ui::platform::SharedTextMeasurer::new(Box::new(FixedTextMeasurer::new()));
    let measurer = compose_ui::TextMeasurer::new(
        compose_ui::create_font_family_resolver(&shared),
        compose_ui::LocaleList::empty(),
        Density::new(1.0),
        compose_ui::LayoutDirection::Ltr,
        8,
    );
    let options = || compose_ui::TextMeasureOptions {
        style: TextStyle { font_family: Some(font.to_font_family()), ..TextStyle::DEFAULT },
        ..Default::default()
    };
    let first = measurer.measure("Hello", options());
    let second = measurer.measure("Hello", options());
    assert!(Rc::ptr_eq(&first.multi_paragraph, &second.multi_paragraph));
    *arriving.borrow_mut() = Some(compose_ui::Typeface::new("arrived"));
    std::thread::sleep(std::time::Duration::from_millis(20));
    compose_runtime::platform::fire_timers();
    while compose_runtime::platform::run_ready() {}
    let third = measurer.measure("Hello", options());
    assert!(!Rc::ptr_eq(&first.multi_paragraph, &third.multi_paragraph));
}
