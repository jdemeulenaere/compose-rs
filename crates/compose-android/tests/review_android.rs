//! Adversarial review of the Android support (commit 839a8b1): what can be checked on the host,
//! through the crate's public API (serialization, the text request, resources) and the
//! contract the bridge relies on (`bridge.rs` schedules timers only after frames and ticks).

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use compose_android::serialize::{GroupCache, Resources, Writer, tag, write_display_list};
use compose_android::text::{AndroidTextMeasurer, LaidOut, TextBackend};
use compose_runtime::executor;
use compose_ui::*;

// ---- Text: the request `TextMeasurer.kt` decodes ---------------------------------------------

/// A backend recording requests (shared, so they outlive the measurer owning it); `per_line`
/// UTF-16 units per line, 10 px each, lines 20 px high.
#[derive(Clone, Default)]
struct SharedRecording {
    requests: Rc<std::cell::RefCell<Vec<Vec<u8>>>>,
    per_line: usize,
}

impl TextBackend for SharedRecording {
    fn layout(&mut self, _handle: u32, text: &str, request: &[u8]) -> LaidOut {
        self.requests.borrow_mut().push(request.to_vec());
        let units = text.encode_utf16().count();
        let per_line = self.per_line.max(1);
        let lines = units.div_ceil(per_line).max(1);
        let metrics = ParagraphMetrics {
            size: IntSize::new((units.min(per_line) * 10) as i32, (lines * 20) as i32),
            first_baseline: 16.0,
            last_baseline: (lines - 1) as f32 * 20.0 + 16.0,
            line_count: lines as u32,
        };
        LaidOut { metrics, did_exceed_max_lines: false }
    }
    fn hit_test(&self, _handle: u32, _position: Offset) -> usize {
        0
    }
    /// As StaticLayout's `getPrimaryHorizontal` and line top/bottom: offset `per_line` (a
    /// soft wrap) starts the second line.
    fn cursor(&self, _handle: u32, offset: usize) -> Rect {
        let per_line = self.per_line.max(1);
        let (line, column) = (offset / per_line, offset % per_line);
        let x = column as f32 * 10.0;
        Rect::new(x, line as f32 * 20.0, x, line as f32 * 20.0 + 20.0)
    }
    fn release(&mut self, _handle: u32) {}
}

/// `TextMeasurer.kt` reads max lines with `request.getInt()` (a Java `int`) and hands it to
/// `StaticLayout.Builder.setMaxLines`. The default options (`max_lines: u32::MAX`, i.e. no limit)
/// are encoded as `0xFFFF_FFFF`, which Java reads as -1: StaticLayout's `mMaximumLines` becomes
/// -1, so `mLineCount < mMaximumLines` (the check that adds the empty last line of an empty text
/// or of a text ending in '\n') never holds: an empty paragraph has 0 lines and 0 height (an
/// empty field's cursor is 0 px high), and a trailing newline's line is lost. Compose passes
/// `Int.MAX_VALUE` for "no limit".
#[test]
fn unlimited_max_lines_is_a_positive_java_int() {
    let backend = SharedRecording::default();
    let mut measurer = AndroidTextMeasurer::new(backend.clone());
    let _ = measurer.layout(
        "",
        &TextStyle::DEFAULT,
        TextLayoutOptions::default(),
        Constraints::new(0, 300, 0, 300),
        &Density::new(1.0),
    );
    let request = backend.requests.borrow()[0].clone();
    // Soft wrap (u8), then max lines (u32), read by Kotlin as a signed int.
    let max_lines = i32::from_le_bytes(request[1..5].try_into().unwrap());
    assert!(max_lines > 0, "StaticLayout gets setMaxLines({max_lines})");
}

/// `AndroidTextMeasurer` doesn't implement `selection_rects`, so it gets the trait's default:
/// one rectangle from the start cursor's x to `max(end.x, start.x)`, on the start's line. A
/// selection from the middle of line 1 to the middle of line 2 gets a zero-width rectangle (or
/// one spanning the wrong area): multi-line selections in text fields and `SelectionContainer`
/// show no (or a wrong) highlight on Android. The desktop measurer returns one rect per line.
#[test]
fn a_selection_over_two_lines_is_highlighted_on_both() {
    let backend = SharedRecording { per_line: 5, ..Default::default() };
    let mut measurer = AndroidTextMeasurer::new(backend);
    let handle = measurer.layout(
        "aaaaabbbbb",
        &TextStyle::DEFAULT,
        TextLayoutOptions::default(),
        Constraints::new(0, 50, 0, 300),
        &Density::new(1.0),
    );
    // From column 3 of line 1 to column 2 of line 2.
    let rects = measurer.selection_rects(handle, 3..7);
    let covers = |x: f32, y: f32| {
        rects.iter().any(|r| r.left <= x && x <= r.right && r.top <= y && y <= r.bottom)
    };
    // "aa" selected at the end of line 1, "bb" at the start of line 2.
    assert!(covers(40.0, 10.0), "line 1's selected end isn't highlighted: {rects:?}");
    assert!(covers(10.0, 30.0), "line 2's selected start isn't highlighted: {rects:?}");
}

// ---- Resources: what `takeDrawing` hands Kotlin -----------------------------------------------

fn image_op(image: ImageBitmap) -> DrawOp {
    let (w, h) = (image.width() as f32, image.height() as f32);
    DrawOp::DrawImage {
        image,
        src: Rect::new(0.0, 0.0, w, h),
        dst: Rect::new(0.0, 0.0, w.max(1.0), h.max(1.0)),
        alpha: 1.0,
    }
}

/// Definitions and drops accumulate in `pending` until `onDraw` takes them. While the view is
/// attached but not drawn (INVISIBLE, scrolled out of a parent that skips drawing it, the
/// activity stopped while Choreographer frames continue), frames keep running (an infinite
/// animation sets AGAIN), and content that makes a new image or path per frame (a
/// `drawWithCache` bitmap, `Canvas { drawPath(Path(...)) }`) appends a full DEFINE (pixels
/// included) and a DROP per frame: `pending` grows without bound. A define whose drop is also
/// pending could be cancelled out instead.
#[test]
fn pending_resources_stay_bounded_while_frames_go_undrawn() {
    let mut resources = Resources::default();
    let mut out = Writer::default();
    let frame_bytes = 64 * 64 * 4;
    for _ in 0..100 {
        let image = ImageBitmap::from_rgba_premultiplied(64, 64, vec![255; frame_bytes]);
        let mut list = DisplayList::default();
        list.push(image_op(image));
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
        drop(list);
        resources.collect();
    }
    // At most the live image's definition (plus a little) should be waiting.
    let mut pending = Writer::default();
    resources.write_pending(&mut pending);
    assert!(
        pending.bytes.len() < 3 * frame_bytes,
        "{} bytes pending after 100 undrawn frames ({} per image)",
        pending.bytes.len(),
        frame_bytes
    );
}

/// `Replayer.defineImage` makes a `max(w,1)` × `max(h,1)` ARGB_8888 bitmap and calls
/// `copyPixelsFromBuffer` with the `length` bytes sent. A 0 × 0 (or 0 × n) image is sent with
/// length 0, and `copyPixelsFromBuffer` throws "Buffer not large enough for pixels" for a 1 × 1
/// bitmap: an exception in `onDraw` crashes the app, and the resources after it in the buffer
/// were already taken from `pending`.
#[test]
fn an_empty_image_never_reaches_kotlin_as_a_short_definition() {
    let mut resources = Resources::default();
    let mut out = Writer::default();
    let mut list = DisplayList::default();
    list.push(image_op(ImageBitmap::from_rgba_premultiplied(0, 0, Vec::new())));
    write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
    let mut taken = Writer::default();
    resources.write_pending(&mut taken);
    let pending = &taken.bytes;
    let mut at = 0;
    while at < pending.len() {
        let u32_at = |i: usize| u32::from_le_bytes(pending[i..i + 4].try_into().unwrap());
        match pending[at] {
            tag::DEFINE_IMAGE => {
                let (width, height, length) =
                    (u32_at(at + 5), u32_at(at + 9), u32_at(at + 13) as usize);
                let needed = width.max(1) as usize * height.max(1) as usize * 4;
                assert!(
                    length >= needed,
                    "DEFINE_IMAGE {width}×{height} with {length} bytes: Kotlin's 1×1 bitmap needs \
                     {needed}"
                );
                at += 17 + length;
            }
            tag::DROP_IMAGE | tag::DROP_PATH => at += 5,
            other => panic!("unexpected pending tag {other}"),
        }
    }
}

// ---- The bridge's scheduling contract ---------------------------------------------------------

/// `bridge.rs` reports the next `delay` only from `frame` and `tick`; after `touch`, `key`,
/// `text` and `preedit` Kotlin schedules nothing. A pointer-input handler is polled during the
/// event itself (`poll_now`), so a timeout it starts there (`detect_tap_gestures`' long press,
/// its double-tap wait, `with_timeout`) registers a timer that wakes nothing: unless the event
/// also changed what's drawn (asking for a frame), no tick is ever posted and the long press
/// never fires. The desktop host checks `next_timer()` after every event.
#[test]
fn a_timeout_started_by_a_touch_gets_a_tick() {
    let mut host = UiHost::headless(200, 200);
    let frames = Rc::new(Cell::new(0));
    let requested = frames.clone();
    host.set_frame_waker(move || requested.set(requested.get() + 1));
    static WAKES: AtomicUsize = AtomicUsize::new(0);
    executor::set_wake_handler(|| {
        WAKES.fetch_add(1, Ordering::SeqCst);
    });
    let long_pressed = Rc::new(Cell::new(false));
    let seen = long_pressed.clone();
    host.set_content(move || {
        let seen = seen.clone();
        Layout(
            Modifier.size(100.dp()).pointer_input((), move |scope| {
                let seen = seen.clone();
                async move {
                    scope
                        .detect_tap_gestures(
                            TapGestures::default().on_long_press(move |_| seen.set(true)),
                        )
                        .await
                }
            }),
            empty_measure_policy(),
            compose_runtime::callback(|| {}),
        );
    });
    host.frame();
    // A first tap starts the handler's task (that launch does wake the host).
    let at = Offset::new(10.0, 10.0);
    host.send_pointer(PointerEvent::press(at).with_pointer(PointerId(1), PointerType::Touch));
    host.send_pointer(PointerEvent::release(at).with_pointer(PointerId(1), PointerType::Touch));
    executor::run_ready();
    host.frame();
    assert!(executor::next_timer().is_none(), "no timer left after the tap");
    frames.set(0);
    WAKES.store(0, Ordering::SeqCst);
    // A second press: the handler (already running) starts its long-press timeout.
    host.send_pointer(PointerEvent::press(at).with_pointer(PointerId(1), PointerType::Touch));
    assert!(executor::next_timer().is_some(), "the long-press timeout is pending");
    assert!(
        frames.get() > 0 || WAKES.load(Ordering::SeqCst) > 0,
        "a timer started by a touch, with no frame asked for and no wake: Android never ticks \
         for it (the long press is lost)"
    );
    assert!(!long_pressed.get());
}
