//! Third phase F review: failing tests for the text field's input method composition, measured
//! by compose-skia's SkParagraph measurer (docs/06-ui.md, "Keyboard and text";
//! docs/07-platforms.md). Each test states the input, what Jetpack Compose (Desktop) does, and
//! what happens instead.

#![allow(non_snake_case)]

use compose::foundation::*;
use compose::runtime::*;
use compose::test::*;
use compose::ui::{
    Constraints, Density, IntSize, Key, KeyEvent, KeyEventType, KeyModifiers, Offset, PointerEvent,
    TextLayoutOptions, TextMeasurer, TextStyle, UnitExt,
};
use compose_skia::SkiaTextMeasurer;

#[composable]
fn Accents() {
    let text = remember(|| mutable_state_of(String::from("éé")));
    BasicTextField!(
        value = text.get(),
        on_value_change = move |value| text.set(value),
        modifier = Modifier.size_wh(200.dp(), 40.dp()).test_tag("field"),
        single_line = true,
    );
}

/// A click in a field while an input method composes (winit delivers clicks during a
/// composition and doesn't end it) is an ordinary click in Compose: it ends the composition and
/// places the cursor, and nothing panics. Here the click moves the cursor without the paragraph
/// being laid out again, though the shown text (the composition in place of the selection)
/// moved with it; the next measurement asks SkParagraph for the cursor in the stale paragraph
/// at an offset inside a character, and `utf16_index` panics. With "éé", the cursor at the end
/// and "a" composed there ("ééa" shown), a click between the two "é"s puts the cursor at byte
/// 2; the composition's cursor is then at 2 + 1 = 3, inside the second "é" (bytes 2..4) of the
/// stale "ééa": "byte index 3 is not a char boundary".
#[test]
fn a_click_during_a_composition_does_not_panic() {
    let mut ui = compose_test_with(
        IntSize::new(300, 100),
        Density::default(),
        Box::new(SkiaTextMeasurer::new()),
        Accents,
    );
    ui.on_node_with_tag("field").request_focus();
    ui.press_key(Key::MoveEnd);
    ui.host().send_preedit("a", Some((1, 1)));
    ui.wait_for_idle();
    // Where the first "é" ends, measured on its own.
    let mut measurer = SkiaTextMeasurer::new();
    let one = measurer.layout(
        "é",
        &TextStyle::DEFAULT,
        TextLayoutOptions { soft_wrap: false, max_lines: 1, ..Default::default() },
        Constraints::new(0, i32::MAX, 0, i32::MAX),
        &Density::default(),
    );
    let x = measurer.metrics(one).size.width as f32;
    ui.host().send_pointer(PointerEvent::press(Offset::new(x, 8.0)));
    ui.host().send_pointer(PointerEvent::release(Offset::new(x, 8.0)));
    // The frame measures the field: the panic.
    ui.wait_for_idle();
    ui.on_node_with_tag("field").assert_exists();
}

// ---- A randomized run ------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

#[composable]
fn Two() {
    let first = remember(|| mutable_state_of(String::from("héllo 日本 👍🏽 שלום")));
    let second = remember(|| mutable_state_of(String::from("e\u{301}x")));
    compose::foundation::layout::Column!({
        BasicTextField!(
            value = first.get(),
            on_value_change = move |value| first.set(value),
            modifier = Modifier.size_wh(150.dp(), 40.dp()).test_tag("first"),
        );
        BasicTextField!(
            value = second.get(),
            // At most 8 characters: longer edits are rejected (and undone by the field).
            on_value_change = move |value: String| if value.chars().count() <= 8 {
                second.set(value)
            },
            modifier = Modifier.size_wh(150.dp(), 40.dp()).test_tag("second"),
            single_line = true,
        );
    });
}

const POOL: &[&str] = &["a", "é", "日本", "👍🏽", "e\u{301}", "ש", "\n", " ", "ab cd", "🇫🇷", "ك"];

/// 30 seeds of 300 random events over two fields: keys with any modifiers (motion, deletion,
/// shortcuts, Tab, Enter), committed text, preedits (with and without a cursor), commits, taps,
/// drags, focus moves, with and without a frame between them, multibyte, emoji, flags,
/// combining and right-to-left text. Nothing should panic, as nothing does in Compose. Here
/// seed 1 panics in `SkiaTextMeasurer` (a byte index inside a character) from the stale
/// paragraph of the test above. With every composition committed before an event that moves
/// the cursor (and a frame before each preedit), 300 seeds pass.
#[test]
fn random_editing_with_an_input_method_does_not_panic() {
    for seed in 1..=30u64 {
        let mut log = Vec::new();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut ui = compose_test_with(
                IntSize::new(300, 100),
                Density::default(),
                Box::new(SkiaTextMeasurer::new()),
                Two,
            );
            ui.on_node_with_tag("first").request_focus();
            let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
            let keys = [
                Key::DirectionLeft,
                Key::DirectionRight,
                Key::DirectionUp,
                Key::DirectionDown,
                Key::Backspace,
                Key::Delete,
                Key::MoveHome,
                Key::MoveEnd,
                Key::Tab,
                Key::Enter,
                Key::Character('a'),
                Key::Character('z'),
                Key::Character('x'),
                Key::Character('v'),
                Key::Character('c'),
                Key::Character('y'),
            ];
            for _ in 0..300 {
                match rng.below(12) {
                    0..=2 => {
                        let key = keys[rng.below(keys.len())];
                        let bits = rng.next();
                        let modifiers = KeyModifiers {
                            shift: bits & 1 != 0,
                            alt: bits & 2 != 0 && bits & 8 != 0,
                            ctrl: bits & 4 != 0 && bits & 16 != 0,
                            meta: bits & 32 != 0 && bits & 64 != 0,
                        };
                        log.push(format!("key {key:?} {modifiers:?}"));
                        ui.host().send_key(KeyEvent {
                            key,
                            kind: KeyEventType::KeyDown,
                            modifiers,
                        });
                        ui.host().send_key(KeyEvent { key, kind: KeyEventType::KeyUp, modifiers });
                    }
                    3 | 4 => {
                        let text = POOL[rng.below(POOL.len())];
                        log.push(format!("text {text:?}"));
                        ui.host().send_text(text);
                    }
                    5 | 6 => {
                        let text = POOL[rng.below(POOL.len())].repeat(1 + rng.below(2));
                        let boundaries: Vec<usize> = text
                            .char_indices()
                            .map(|(index, _)| index)
                            .chain([text.len()])
                            .collect();
                        let cursor = if rng.below(4) == 0 {
                            None
                        } else {
                            let at = boundaries[rng.below(boundaries.len())];
                            Some((at, at))
                        };
                        log.push(format!("preedit {text:?} {cursor:?}"));
                        ui.host().send_preedit(&text, cursor);
                    }
                    7 => {
                        let text = POOL[rng.below(POOL.len())];
                        log.push(format!("commit {text:?}"));
                        ui.host().send_preedit("", None);
                        ui.host().send_text(text);
                    }
                    8 => {
                        let at = Offset::new(rng.below(320) as f32 - 10.0, rng.below(90) as f32);
                        log.push(format!("tap {at:?}"));
                        ui.host().send_pointer(PointerEvent::press(at));
                        ui.host().send_pointer(PointerEvent::release(at));
                    }
                    9 => {
                        let from = Offset::new(rng.below(160) as f32, rng.below(80) as f32);
                        let to = Offset::new(rng.below(320) as f32 - 10.0, rng.below(100) as f32);
                        log.push(format!("drag {from:?} {to:?}"));
                        ui.host().send_pointer(PointerEvent::press(from));
                        ui.host().send_pointer(PointerEvent::moved(
                            Offset::new((from.x + to.x) / 2.0, (from.y + to.y) / 2.0),
                            true,
                        ));
                        ui.host().send_pointer(PointerEvent::moved(to, true));
                        ui.host().send_pointer(PointerEvent::release(to));
                    }
                    10 => {
                        log.push("frame".into());
                        ui.wait_for_idle();
                    }
                    _ => {
                        let tag = if rng.below(2) == 0 { "first" } else { "second" };
                        log.push(format!("focus {tag}"));
                        ui.wait_for_idle();
                        ui.on_node_with_tag(tag).request_focus();
                    }
                }
                if rng.below(2) == 0 {
                    ui.wait_for_idle();
                    let _ = ui.host().text_input_area();
                }
            }
            ui.wait_for_idle();
        }));
        if let Err(error) = result {
            let message = error
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| error.downcast_ref::<&str>().map(|message| message.to_string()));
            panic!(
                "seed {seed}: {message:?}\nthe last events: {:#?}",
                &log[log.len().saturating_sub(12)..]
            );
        }
    }
}
