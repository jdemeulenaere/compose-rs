//! The stress screens (docs/15) compose, scroll and animate as designed: the benchmark measures
//! what these check is there.

use compose::runtime::platform;
use compose::test::*;
use compose::ui::platform::{FixedTextMeasurer, UiHost};
use compose::ui::{Density, IntSize, Offset, PointerEvent};
use compose_rs_feed::stress::*;

/// The Pixel 8 Pro's screen, as the benchmark runs it: 1008 × 2244 px at 2.25.
fn phone(content: fn()) -> ComposeTestRule {
    let mut ui = ComposeTestRule::with_manual_clock(content);
    ui.host().resize(IntSize::new(1008, 2244));
    ui.host().set_density(Density::new(2.25));
    ui.advance_time_by_frame();
    ui
}

#[test]
fn bitmaps_are_opaque_gradients_with_rings() {
    for which in 0..BITMAPS {
        let bitmap = stress_bitmap(which);
        assert_eq!((bitmap.width(), bitmap.height()), (BITMAP_WIDTH, BITMAP_HEIGHT));
        assert!(bitmap.pixels().chunks(4).all(|pixel| pixel[3] == 0xFF));
        // Not flat: the corners differ.
        assert_ne!(
            bitmap_pixel(which, 0, 0),
            bitmap_pixel(which, BITMAP_WIDTH - 1, BITMAP_HEIGHT - 1)
        );
    }
    // The values the Kotlin app's `stressBitmapPixel` computes too (Stress.kt).
    assert_eq!(bitmap_pixel(0, 0, 0), 0xFF9780D4);
    assert_eq!(bitmap_pixel(3, 100, 50), 0xFFCF6850);
    assert_eq!(bitmap_pixel(7, 239, 179), 0xFF006A6A);
}

#[test]
fn the_kinds_alternate() {
    let kinds: Vec<StressKind> = (0..5).map(StressKind::of).collect();
    assert_eq!(
        kinds,
        [
            StressKind::Carousel,
            StressKind::RichPost,
            StressKind::ActionCard,
            StressKind::Banner,
            StressKind::Carousel
        ]
    );
}

#[test]
fn the_stress_list_shows_each_kind_under_a_header() {
    let mut ui = phone(StressScroll);
    ui.on_node_with_text("Stress").assert_exists();
    ui.on_node_with_text("Section 1").assert_exists();
    ui.on_node_with_text("Collection 0").assert_exists();
    assert!(ui.count_nodes(&has_text("Tile 1")) >= 1);
    ui.on_node_with_text("Alan Turing").assert_exists();
    ui.on_node_with_text("Task 2").assert_exists();
    assert!(ui.count_nodes(&has_text("Accept")) >= 1);
    ui.on_node_with_text("Featured 3").assert_exists();
    // A carousel composes the tiles in view and a few beyond, not all twelve.
    assert_eq!(ui.count_nodes(&has_text("Tile 12")), 0);
}

#[test]
fn a_header_sticks_while_its_items_scroll_under_it() {
    let mut ui = phone(StressScroll);
    let top = ui.on_node_with_text("Section 1").fetch_semantics_snapshot().bounds.top;
    let host = ui.host();
    let mut y = 2000.0;
    host.send_pointer(
        PointerEvent::press(Offset::new(500.0, y))
            .with_pointer(compose::ui::PointerId(0), compose::ui::PointerType::Touch),
    );
    while y > 600.0 {
        y -= 50.0;
        host.send_pointer(
            PointerEvent::moved(Offset::new(500.0, y), true)
                .with_pointer(compose::ui::PointerId(0), compose::ui::PointerType::Touch),
        );
    }
    host.send_pointer(
        PointerEvent::release(Offset::new(500.0, y))
            .with_pointer(compose::ui::PointerId(0), compose::ui::PointerType::Touch),
    );
    ui.advance_time_by_frame();
    // Item 0 scrolled away (not placed: only the items in view are); its section's header is
    // where it was.
    assert_eq!(ui.count_nodes(&has_text("Collection 0")), 0);
    assert_eq!(ui.on_node_with_text("Section 1").fetch_semantics_snapshot().bounds.top, top);
}

#[test]
fn the_stress_animation_ticks_every_frame_and_its_card_grows() {
    let mut ui = phone(StressAnimation);
    assert_eq!(ui.count_nodes(&has_text_containing("Frame ")), 1);
    let first = ticker(&mut ui);
    ui.advance_time_by_frame();
    let second = ticker(&mut ui);
    ui.advance_time_by_frame();
    assert_ne!(first, second);
    assert_ne!(second, ticker(&mut ui));
    // Each bar's percentage.
    assert_eq!(ui.count_nodes(&has_text_containing(" %")), BARS as usize);
    // The card shows one sentence, then four after a second, then one again.
    let more = "Reading about incremental computation";
    assert_eq!(ui.count_nodes(&has_text_containing(more)), 0);
    ui.advance_time_by(std::time::Duration::from_millis(1_000));
    assert_eq!(ui.count_nodes(&has_text_containing(more)), 1);
    ui.advance_time_by(std::time::Duration::from_millis(1_000));
    assert_eq!(ui.count_nodes(&has_text_containing(more)), 0);
}

fn ticker(ui: &mut ComposeTestRule) -> String {
    let node = ui.on_node(has_text_containing("Frame ")).fetch_semantics_snapshot();
    node.config.all_text().collect::<Vec<_>>().join("")
}

/// The lazy items (the list's and the carousels') composed in `host` now.
fn lazy_items(host: &mut UiHost) -> usize {
    let dump = host.with_tree(|tree| tree.dump());
    dump.lines().filter(|line| line.trim_start().starts_with("LazyItem")).count()
}

/// The stress list's first frame composes the items it shows and few more: while it knows few
/// sizes, its first layout asks for a few items at a time (D472). Its 81 px header alone once
/// made it compose 87 items, rows and the carousels' tiles, of which 38 stayed (D460). The
/// frame fills the screen: the next one composes nothing.
#[test]
fn the_stress_lists_first_frame_composes_what_it_shows() {
    platform::set_manual_clock(platform::now_nanos());
    let mut host = UiHost::new(
        IntSize::new(1008, 2244),
        Density::new(2.25),
        Box::new(FixedTextMeasurer::new()),
    );
    platform::track_recompositions(true);
    let before = platform::recomposition_counts();
    host.set_content(StressScroll);
    host.frame();
    let composed = platform::recomposition_counts().since(&before).scopes_named("LazyItem");
    let kept = lazy_items(&mut host);
    assert!(composed <= kept as u64 + 2, "{composed} items composed, {kept} kept");
    assert!(composed < 30, "{composed} items composed");
    let before = platform::recomposition_counts();
    platform::advance_clock(std::time::Duration::from_millis(8));
    host.frame();
    let more = platform::recomposition_counts().since(&before).scopes_named("LazyItem");
    assert_eq!(more, 0, "the second frame composed items");
}

/// The resource changes in `bytes` (what `takeDrawing` writes before a frame's display list):
/// the ids of the paths dropped and defined, in order.
fn path_changes(bytes: &[u8]) -> (Vec<u32>, Vec<u32>) {
    use compose_android::serialize::tag;
    let id = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let (mut at, mut dropped, mut defined) = (0, Vec::new(), Vec::new());
    while at < bytes.len() {
        match bytes[at] {
            tag::DROP_PATH => {
                assert!(defined.is_empty(), "a drop after a definition");
                dropped.push(id(at + 1));
                at += 5;
            }
            tag::DEFINE_PATH => {
                defined.push(id(at + 1));
                assert_eq!(id(at + 6), 0, "no step");
                let commands = id(at + 10);
                at += 14;
                for _ in 0..commands {
                    // A command, then its points: move and line 1, quad 2, cubic 3, close none.
                    at += 1 + 8 * [1, 1, 2, 3, 0][bytes[at] as usize];
                }
            }
            other => panic!("not a path change: {other}"),
        }
    }
    (dropped, defined)
}

/// The stress animation's 50 waves are one `Path`, reset for each wave, as Stress.kt's (D533).
/// The first frame defines each wave's contents under an id of its own; each frame after it
/// redefines the same 50 ids in place, the last frame's contents being gone, dropping none and
/// defining no new one: the replayer rewinds and refills the same 50 `android.graphics.Path`s
/// (D532), and a frame redefining a path drawn as a path flushes HWUI's layers first (D510).
#[test]
fn the_waves_redefine_their_paths_in_place_each_frame() {
    use compose_android::serialize::{GroupCache, Resources, Writer, write_display_list};
    let mut ui = phone(StressAnimation);
    let (mut resources, mut groups, mut list) =
        (Resources::default(), GroupCache::default(), Writer::default());
    let mut first = Vec::new();
    for frame in 0..6 {
        ui.advance_time_by_frame();
        ui.host().with_tree(|tree| {
            write_display_list(tree.display_list(), &mut resources, &mut groups, &mut list)
        });
        resources.collect();
        let mut changes = Writer::default();
        resources.write_pending(&mut changes);
        let (dropped, mut defined) = path_changes(&changes.bytes);
        defined.sort_unstable();
        assert_eq!(defined.len(), WAVES as usize, "frame {frame}");
        assert_eq!(dropped, [], "frame {frame}");
        if frame == 0 {
            first = defined;
        } else {
            assert_eq!(defined, first, "frame {frame}: the same ids");
        }
    }
}

/// A text measurer that records each paragraph's text, style and maximum width, laying it out
/// with the fixed-advance measurer.
struct Recording {
    inner: FixedTextMeasurer,
    laid_out: std::rc::Rc<std::cell::RefCell<Vec<(String, compose::ui::TextStyle, i32)>>>,
}

impl compose::ui::platform::TextMeasurer for Recording {
    fn layout(
        &mut self,
        text: &str,
        style: &compose::ui::TextStyle,
        options: compose::ui::platform::TextLayoutOptions,
        constraints: compose::ui::Constraints,
        density: &Density,
    ) -> compose::ui::platform::ParagraphHandle {
        self.laid_out.borrow_mut().push((text.to_string(), style.clone(), constraints.max_width));
        self.inner.layout(text, style, options, constraints, density)
    }
    fn metrics(
        &self,
        paragraph: compose::ui::platform::ParagraphHandle,
    ) -> compose::ui::platform::ParagraphMetrics {
        self.inner.metrics(paragraph)
    }
    fn hit_test(
        &self,
        paragraph: compose::ui::platform::ParagraphHandle,
        position: Offset,
    ) -> compose::ui::platform::TextPosition {
        self.inner.hit_test(paragraph, position)
    }
    fn cursor_rect(
        &self,
        paragraph: compose::ui::platform::ParagraphHandle,
        offset: usize,
    ) -> compose::ui::Rect {
        self.inner.cursor_rect(paragraph, offset)
    }
    fn text(&self, paragraph: compose::ui::platform::ParagraphHandle) -> &str {
        self.inner.text(paragraph)
    }
    fn release(&mut self, paragraph: compose::ui::platform::ParagraphHandle) {
        self.inner.release(paragraph)
    }
}

/// The rich post's text is laid out as the Kotlin app's (D570, D571): Material 3 1.4's body
/// medium (14 sp on 20 sp lines, 0.2 sp tracking, the line height centered and untrimmed) at
/// 864 px on the Pixel 8 Pro (448 dp less the card's and its column's 16 dp each side, 384 dp).
/// Where the line breaks fall needs Android's fonts: on the device both apps break "Alan wrote:
/// Reading about incremental computation and how" before "UI" (docs/15, "What differs"); before
/// D570 compose-rs, without the tracking, fit "UI" on the first line.
#[test]
fn the_posts_text_is_laid_out_in_body_medium_at_the_kotlin_apps_width() {
    use compose::material3::{DEFAULT_LINE_HEIGHT_STYLE, Typography};
    use compose::ui::TextUnit;
    let laid_out = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let measurer = Recording { inner: FixedTextMeasurer::new(), laid_out: laid_out.clone() };
    let mut ui = compose_test_with(
        IntSize::new(1008, 2244),
        Density::new(2.25),
        Box::new(measurer),
        StressScroll,
    );
    ui.wait_for_idle();
    let laid_out = laid_out.borrow();
    let (_, style, width) = laid_out
        .iter()
        .find(|(text, _, _)| text.starts_with("Alan wrote: Reading about incremental computation"))
        .expect("the first post's text");
    assert_eq!(*width, 864);
    let body_medium = Typography::default().body_medium;
    assert_eq!(
        (style.font_size, style.line_height, style.letter_spacing),
        (TextUnit::sp(14.0), TextUnit::sp(20.0), TextUnit::sp(0.2))
    );
    assert_eq!(style.line_height_style, Some(DEFAULT_LINE_HEIGHT_STYLE));
    assert!(style.has_same_layout_affecting_attributes(&body_medium));
    // The other texts take their styles' tracking too: a tile's label in body small (0.4 sp),
    // the section header in title small (0.1 sp).
    let tracking = |prefix: &str| {
        laid_out
            .iter()
            .find(|(text, _, _)| text == prefix)
            .map(|(_, style, _)| style.letter_spacing)
    };
    assert_eq!(tracking("Tile 1"), Some(TextUnit::sp(0.4)));
    assert_eq!(tracking("Section 1"), Some(TextUnit::sp(0.1)));
}

/// The `GROUP` records of a frame's bytes (`write_display_list`'s): each group's key, the
/// version the replayer keeps its recording by, and its content's bytes.
fn group_records(bytes: &[u8]) -> Vec<(u64, u64, Vec<u8>)> {
    use compose_android::serialize::{LAYER_ENTRY_BYTES, tag};
    let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let u64_at = |at: usize| u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
    let mut at = 0;
    if bytes.first() == Some(&tag::LAYERS) {
        at = 5 + u32_at(1) as usize * LAYER_ENTRY_BYTES;
    }
    let mut records = Vec::new();
    while bytes.get(at) == Some(&tag::GROUP) {
        let (key, version, length) = (u64_at(at + 1), u64_at(at + 9), u32_at(at + 17) as usize);
        records.push((key, version, bytes[at + 21..at + 21 + length].to_vec()));
        at += 21 + length;
    }
    records
}

/// The Rust side of the expanded card, and the condition D646 is about. At parity the card
/// lays its text out in four lines and draws it whole within its grown clip once its size
/// animation ends, as Compose's; and its column's record keeps the bytes and the version of the
/// frame that expanded it, when the column was still one line high (D323: only its size
/// changed), so Android keeps the recording made then. Drawn into that recording, a paragraph
/// must not depend on the recording's size: before D646 the Pixel 8 Pro's card showed the two
/// lines that fitted it then (`Layout.draw` draws only the lines within the canvas's clip
/// bounds). This test passes without D646's fix, which is Kotlin's (`TextAndroidCanvas`):
/// android/compose-rs has no Android test setup, so the fix has no automated test; it was
/// checked on the device with screenshots.
#[test]
fn the_expanded_cards_text_is_whole_and_its_record_kept_as_it_grows() {
    use compose::ui::platform::DrawOp;
    use compose::ui::platform::Optimizations;
    use compose_android::serialize::{GroupCache, Resources, Writer, write_display_list};
    use_optimizations(Optimizations::NONE);
    let mut ui = phone(StressAnimation);
    let (mut resources, mut cache, mut bytes) =
        (Resources::default(), GroupCache::default(), Writer::default());
    // Each frame's bytes, as the view writes them for the replayer.
    let mut write = |ui: &mut ComposeTestRule| {
        ui.host().with_tree(|tree| {
            write_display_list(tree.display_list(), &mut resources, &mut cache, &mut bytes)
        });
        resources.collect();
        resources.write_pending(&mut Writer::default());
        group_records(&bytes.bytes)
    };
    let more = "Reading about incremental computation";
    while ui.count_nodes(&has_text_containing(more)) == 0 {
        ui.advance_time_by_frame();
        write(&mut ui);
    }
    let text = ui.on_node(has_text_containing(more)).node_id();
    let column = ui.host().with_tree(|tree| tree.parent_of(text)).expect("the card's column");
    let height =
        |ui: &mut ComposeTestRule| ui.host().with_tree(|tree| tree.bounds_in_root(column).height());
    let record = |records: Vec<(u64, u64, Vec<u8>)>| {
        records.into_iter().find(|(key, ..)| *key == column.to_bits()).expect("its record")
    };
    let expanding = record(write(&mut ui));
    // Still the collapsed card's height: a title and a line, 12 dp above and below.
    assert_eq!(height(&mut ui), (27 + 45 + 45 + 27) as f32);
    for _ in 0..40 {
        ui.advance_time_by_frame();
        write(&mut ui);
    }
    // The text: four lines of 20 sp (45 px).
    let bounds = ui.on_node(has_text_containing(more)).fetch_semantics_snapshot().bounds;
    assert_eq!(bounds.bottom - bounds.top, 4.0 * 45.0);
    let expanded = height(&mut ui);
    assert_eq!(expanded, (27 + 45 + 4 * 45 + 27) as f32);
    // Drawn whole, within the column's clip at its grown size.
    let list = ui.host().with_tree(|tree| tree.display_list().flatten());
    let clip = list.ops.iter().find_map(|op| match op {
        DrawOp::ClipRect(rect) if rect.width() == 936.0 && rect.height() > 200.0 => Some(*rect),
        _ => None,
    });
    assert_eq!(clip.map(|clip| clip.height()), Some(expanded));
    let paragraphs = list.ops.iter().filter(|op| matches!(op, DrawOp::DrawParagraph(..)));
    assert!(paragraphs.count() > 0);
    // The record Android recorded at the collapsed size is the one it keeps.
    let grown = record(write(&mut ui));
    assert_eq!((grown.1, &grown.2), (expanding.1, &expanding.2));
}
