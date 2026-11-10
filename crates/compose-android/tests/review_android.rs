//! Adversarial review of the Android support (commit 839a8b1): what can be checked on the host,
//! through the crate's public API (serialization, the text request, resources) and the
//! contract the bridge relies on (`bridge.rs` schedules timers only after frames and ticks).

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use compose_android::serialize::{
    GroupCache, Resources, Writer, color_filter, operand_bytes, tag, write_display_list,
};
use compose_android::text::{AndroidTextMeasurer, LaidOut, TextBackend};
use compose_runtime::platform;
use compose_ui::platform::TextMeasurer;
use compose_ui::platform::*;
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
        LaidOut { metrics, did_exceed_max_lines: false, max_intrinsic_width: None }
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
        color_filter: None,
        filter_quality: compose_ui::FilterQuality::Low,
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
    platform::set_wake_handler(|| {
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
    platform::run_ready();
    host.frame();
    assert!(platform::next_timer().is_none(), "no timer left after the tap");
    frames.set(0);
    WAKES.store(0, Ordering::SeqCst);
    // A second press: the handler (already running) starts its long-press timeout.
    host.send_pointer(PointerEvent::press(at).with_pointer(PointerId(1), PointerType::Touch));
    assert!(platform::next_timer().is_some(), "the long-press timeout is pending");
    assert!(
        frames.get() > 0 || WAKES.load(Ordering::SeqCst) > 0,
        "a timer started by a touch, with no frame asked for and no wake: Android never ticks \
         for it (the long press is lost)"
    );
    assert!(!long_pressed.get());
}

// ---- Android's animator duration scale ---------------------------------------------------------

/// An animation on the main thread follows the setting (D836), as Compose's on Android.
#[test]
fn the_animator_duration_scale_reaches_the_threads_animations() {
    use compose_animation::{Animatable, LINEAR_EASING, tween};
    use compose_runtime::CoroutineScope;
    use compose_test::ComposeTestRule;
    use std::time::Duration;

    let mut ui = ComposeTestRule::with_manual_clock(|| {});
    compose_android::set_animator_duration_scale(2.0);
    let animatable = Animatable::new(0.0f32);
    let moved = animatable.clone();
    let scope = CoroutineScope::new();
    scope.launch(async move {
        moved.animate_to(1000.0, tween(160).easing(LINEAR_EASING)).await;
    });
    // The first frame is its start; then 160 ms of its 320.
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(160));
    assert_eq!(animatable.value(), 500.0);
    compose_android::set_animator_duration_scale(1.0);
}

/// `adb shell settings put global animator_duration_scale -1` doesn't crash the app.
#[test]
fn a_negative_animator_duration_scale_is_0() {
    compose_android::set_animator_duration_scale(-1.0);
    assert_eq!(compose_ui::motion_duration_scale(), 0.0);
    compose_android::set_animator_duration_scale(0.5);
    assert_eq!(compose_ui::motion_duration_scale(), 0.5);
    compose_android::set_animator_duration_scale(1.0);
}

/// `ComposeRsRuntime` reads the setting, and starts watching it, at the first animation's read, and
/// not before: a screen with no animation pays nothing (D839). What it reads is as a set's.
#[test]
fn the_animator_duration_scale_is_read_at_its_first_use_alone() {
    thread_local! {
        static READS: Cell<u32> = const { Cell::new(0) };
    }
    fn read() -> f32 {
        READS.with(|reads| reads.set(reads.get() + 1));
        -1.0
    }
    compose_android::read_animator_duration_scale_with(read);
    assert_eq!(READS.with(Cell::get), 0);
    // The negative setting, as `set_animator_duration_scale` has it.
    assert_eq!(compose_ui::motion_duration_scale(), 0.0);
    assert_eq!(compose_ui::motion_duration_scale(), 0.0);
    assert_eq!(READS.with(Cell::get), 1);
    // The observer's update after that, and no read again.
    compose_android::set_animator_duration_scale(2.0);
    assert_eq!(compose_ui::motion_duration_scale(), 2.0);
    assert_eq!(READS.with(Cell::get), 1);
    // The last view gone, the watch stops: the next read reads it again.
    compose_android::read_animator_duration_scale_with(read);
    assert_eq!(compose_ui::motion_duration_scale(), 0.0);
    assert_eq!(READS.with(Cell::get), 2);
    compose_android::set_animator_duration_scale(1.0);
}

/// What `AndroidXViewModels.kt` holds: a store outliving hosts until it's cleared for good, with a
/// token for each and none for 0 (a view with no owner keeps its own).
#[test]
fn a_retained_view_model_store_lives_until_it_is_cleared() {
    use compose_lifecycle::{ViewModel, ViewModelBase};

    #[derive(Default)]
    struct Model {
        base: ViewModelBase,
        cleared: Rc<Cell<bool>>,
    }
    impl ViewModel for Model {
        fn base(&self) -> &ViewModelBase {
            &self.base
        }
        fn on_cleared(&self) {
            self.cleared.set(true);
        }
    }

    assert!(compose_android::retained_view_model_store(0).is_none());
    let (token, other) = (
        compose_android::new_retained_view_model_store(),
        compose_android::new_retained_view_model_store(),
    );
    assert_ne!(token, 0);
    assert_ne!(token, other);
    let store = compose_android::retained_view_model_store(token).unwrap();
    assert_eq!(compose_android::retained_view_model_store(token).unwrap(), store);
    let cleared = Rc::new(Cell::new(false));
    store.put("key", Rc::new(Model { cleared: cleared.clone(), ..Default::default() }));
    // A host given it: it goes, the store stays.
    let mut ui = compose_test::ComposeTestRule::empty(UiHost::new(
        IntSize::new(10, 10),
        compose_ui::Density::new(1.0),
        Box::new(FixedTextMeasurer::new()),
    ));
    ui.host().set_retained_view_model_store(store.clone());
    ui.set_content(|| {});
    ui.host().set_lifecycle_state(compose_lifecycle::LifecycleState::Destroyed);
    drop(ui);
    assert!(!cleared.get());
    compose_android::clear_retained_view_model_store(token);
    assert!(cleared.get());
    assert!(compose_android::retained_view_model_store(token).is_none());
    // Cleared twice: nothing. The other: there still.
    compose_android::clear_retained_view_model_store(token);
    assert!(compose_android::retained_view_model_store(other).is_some());
    compose_android::clear_retained_view_model_store(other);
}

/// `Back.kt` passes `BackEvent.swipeEdge` on as it is.
#[test]
fn the_swipe_edges_are_android_s() {
    use compose_navigation_event::NavigationEvent;
    // `android.window.BackEvent`'s `EDGE_LEFT`, `EDGE_RIGHT` and `EDGE_NONE`.
    assert_eq!(
        (NavigationEvent::EDGE_LEFT, NavigationEvent::EDGE_RIGHT, NavigationEvent::EDGE_NONE),
        (0, 1, 2)
    );
}

/// `ComposeRsView` sends `Lifecycle.State`'s ordinals (`Lifecycle.kt`).
#[test]
fn the_lifecycle_states_are_in_androidx_s_order() {
    use compose_lifecycle::LifecycleState::*;
    let states = [Destroyed, Initialized, Created, Started, Resumed];
    for (ordinal, state) in states.iter().enumerate() {
        assert_eq!(compose_android::lifecycle_state(ordinal as i32), *state);
    }
}

// ---- A bitmap's tint on the wire ------------------------------------------------------------

/// `Replayer.kt` reads, after the tag: the id, the two rects, the alpha, and last the color
/// filter, its kind (0 none) then its values (a tint: its color and mode); and the operation
/// after it follows exactly there.
#[test]
fn an_images_color_filter_goes_last_on_the_wire() {
    let tinted = |tint: Option<Color>| {
        let mut resources = Resources::default();
        let mut out = Writer::default();
        let mut list = DisplayList::default();
        let image = ImageBitmap::from_rgba_premultiplied(1, 1, vec![255, 0, 0, 255]);
        let color_filter = tint.map(|tint| list.push_color_filter(ColorFilter::tint(tint)));
        list.push(DrawOp::DrawImage {
            image,
            src: Rect::new(0.0, 0.0, 1.0, 1.0),
            dst: Rect::new(0.0, 0.0, 1.0, 1.0),
            alpha: 1.0,
            color_filter,
            filter_quality: compose_ui::FilterQuality::Low,
        });
        list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 1.0, 1.0), Paint::fill(Color::RED)));
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
        out.bytes
    };
    const FIXED: usize = 4 + 32 + 4 + 1;
    for (tint, filter_bytes) in [(Some(Color::BLUE), 1 + 4 + 1), (None, 1)] {
        let bytes = tinted(tint);
        let at = bytes.iter().position(|byte| *byte == tag::DRAW_IMAGE).expect("the image");
        assert_eq!(operand_bytes(&bytes, at), FIXED + filter_bytes);
        let filter = at + 1 + FIXED;
        if tint.is_some() {
            assert_eq!(bytes[filter], color_filter::BLEND_MODE);
            let written = u32::from_le_bytes(bytes[filter + 1..filter + 5].try_into().unwrap());
            assert_eq!(written, Color::BLUE.to_argb());
            assert_eq!(bytes[filter + 5], BlendMode::SrcIn as u8);
        } else {
            assert_eq!(bytes[filter], color_filter::NONE);
        }
        assert_eq!(bytes[filter + filter_bytes], tag::DRAW_RECT, "the next operation follows");
    }
}

// ---- w2-graphics review: DEFINE_IMAGE_WITH, nested shaders, FilterQuality, Replayer.kt ------

#[test]
fn define_image_with_wire_formats_and_filter_quality_byte() {
    use compose_ui::colorspace::ColorSpaces;

    let serialize_single_image = |bmp: ImageBitmap, quality: FilterQuality| -> (Vec<u8>, Vec<u8>) {
        let mut resources = Resources::default();
        let mut ops = Writer::default();
        let mut defs = Writer::default();
        let mut list = DisplayList::default();
        list.push(DrawOp::DrawImage {
            image: bmp,
            src: Rect::new(0.0, 0.0, 2.0, 2.0),
            dst: Rect::new(0.0, 0.0, 2.0, 2.0),
            alpha: 1.0,
            color_filter: None,
            filter_quality: quality,
        });
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut ops);
        resources.write_pending(&mut defs);
        (defs.bytes, ops.bytes)
    };

    // Default Argb8888 + has_alpha=true + SRGB uses compact DEFINE_IMAGE, filtered=0 for None.
    let srgb_default = ImageBitmap::new(2, 2);
    let (defs, ops) = serialize_single_image(srgb_default, FilterQuality::None);
    assert_eq!(defs[0], tag::DEFINE_IMAGE);
    assert_eq!(ops[0], tag::DRAW_IMAGE);
    // tag(1) + id(4) + src+dst(32) + alpha(4) = offset 41 for `filtered` byte.
    assert_eq!(ops[41], 0, "FilterQuality::None writes filtered=0");

    for quality in [FilterQuality::Low, FilterQuality::Medium, FilterQuality::High] {
        let (_, ops) = serialize_single_image(ImageBitmap::new(2, 2), quality);
        assert_eq!(ops[41], 1, "{quality:?} writes filtered=1");
    }

    // Argb8888 with has_alpha=false uses DEFINE_IMAGE_WITH(config=0, has_alpha=0, cs=0).
    let opaque_srgb =
        ImageBitmap::new_with(2, 2, ImageBitmapConfig::Argb8888, false, &ColorSpaces::SRGB);
    let (defs, _) = serialize_single_image(opaque_srgb, FilterQuality::Low);
    assert_eq!(defs[0], tag::DEFINE_IMAGE_WITH);
    assert_eq!(&defs[13..16], &[0, 0, 0]);
    assert_eq!(u32::from_le_bytes(defs[16..20].try_into().unwrap()), 16);

    // Rgb565 DisplayP3 uses DEFINE_IMAGE_WITH(config=2, has_alpha=0, cs=7).
    let p3_565 =
        ImageBitmap::new_with(2, 2, ImageBitmapConfig::Rgb565, false, &ColorSpaces::DISPLAY_P3);
    let (defs, _) = serialize_single_image(p3_565, FilterQuality::Low);
    assert_eq!(defs[0], tag::DEFINE_IMAGE_WITH);
    assert_eq!(&defs[13..16], &[2, 0, 7]);
    assert_eq!(u32::from_le_bytes(defs[16..20].try_into().unwrap()), 8);

    // Non-Android space (OKLAB, id=19) converts to EXTENDED_SRGB (cs=2) F16 (config=3), 8 B/px.
    let oklab = ImageBitmap::new_with(2, 2, ImageBitmapConfig::Argb8888, true, &ColorSpaces::OKLAB);
    let (defs, _) = serialize_single_image(oklab, FilterQuality::Low);
    assert_eq!(defs[0], tag::DEFINE_IMAGE_WITH);
    assert_eq!(&defs[13..16], &[3, 1, 2]);
    assert_eq!(u32::from_le_bytes(defs[16..20].try_into().unwrap()), 32);
}

#[test]
fn nested_composite_and_transformed_shaders_match_operand_bytes_and_fold_alpha() {
    use compose_ui::colorspace::ColorSpaces;

    let p3_red = Color::new_with(1.0, 0.0, 0.0, 1.0, &ColorSpaces::DISPLAY_P3);
    let p3_blue = Color::new_with(0.0, 0.0, 1.0, 1.0, &ColorSpaces::DISPLAY_P3);
    let img = ImageBitmap::from_rgba_premultiplied(1, 1, vec![255, 0, 0, 255]);
    let nested = Shader::composite(
        Shader::composite(
            Shader::image(&img, TileMode::Repeated, TileMode::Mirror),
            Shader::linear_gradient(
                Offset::ZERO,
                Offset::new(10.0, 10.0),
                [(0.0, p3_red), (1.0, p3_blue)],
                TileMode::Decal,
            ),
            BlendMode::SrcIn,
        ),
        Shader::composite(
            Shader::radial_gradient(
                Offset::new(5.0, 5.0),
                5.0,
                [Color::RED, Color::GREEN],
                TileMode::Clamp,
            ),
            Shader::sweep_gradient(Offset::new(5.0, 5.0), [p3_red, Color::WHITE]),
            BlendMode::Modulate,
        ),
        BlendMode::Plus,
    );

    let mut list = DisplayList::default();
    let shader_idx = list.push_shader(PaintShader {
        shader: nested.clone(),
        transform: Some(Box::new(Matrix::translation(3.0, 4.0))),
        alpha: 0.8,
    });
    let cf_idx = list.push_color_filter(ColorFilter::lighting(Color::RED, Color::BLUE));
    let rich_paint = Paint::fill(p3_red)
        .with_blend_mode(BlendMode::Screen)
        .with_color_filter(cf_idx)
        .with_blur(2.5)
        .with_shader(shader_idx);

    list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 10.0, 10.0), rich_paint));
    list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 1.0, 1.0), Paint::fill(Color::BLACK)));

    let mut resources = Resources::default();
    let mut out = Writer::default();
    write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
    assert_eq!(out.bytes[0], tag::DRAW_RECT);
    let rich_len = operand_bytes(&out.bytes, 0);
    assert_eq!(out.bytes[1 + rich_len], tag::DRAW_RECT, "operand_bytes matches nested shader");

    // Plain shader fill inside `SaveLayerAlpha` folds under `SINGLE_DRAWING_FADE`, scaling
    // `PaintShader.alpha` (0.8 * 0.5 = 0.4).
    let mut fold_list = DisplayList::default();
    let plain_shader_idx = fold_list.push_shader(PaintShader {
        shader: nested,
        transform: Some(Box::new(Matrix::translation(3.0, 4.0))),
        alpha: 0.8,
    });
    fold_list.push(DrawOp::SaveLayerAlpha { bounds: Rect::new(0.0, 0.0, 20.0, 20.0), alpha: 0.5 });
    fold_list.push(DrawOp::DrawRect(
        Rect::new(0.0, 0.0, 10.0, 10.0),
        Paint::fill(Color::BLACK).with_shader(plain_shader_idx),
    ));
    fold_list.push(DrawOp::Restore);
    fold_list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 1.0, 1.0), Paint::fill(Color::BLACK)));

    let mut fold_out = Writer::default();
    let fade = Optimizations::NONE.with(Optimization::SINGLE_DRAWING_FADE, true);
    write_display_list(&fold_list, &mut resources, &mut GroupCache::new(fade), &mut fold_out);

    assert_eq!(fold_out.bytes[0], tag::SAVE);
    assert_eq!(fold_out.bytes[1], tag::DRAW_RECT);
    let draw_len = operand_bytes(&fold_out.bytes, 1);
    let restore_at = 1 + 1 + draw_len;
    assert_eq!(fold_out.bytes[restore_at], tag::RESTORE);
    assert_eq!(fold_out.bytes[restore_at + 1], tag::DRAW_RECT);

    let alpha_at = restore_at - 4;
    let written_alpha =
        f32::from_le_bytes(fold_out.bytes[alpha_at..alpha_at + 4].try_into().unwrap());
    assert!(
        (written_alpha - 0.4).abs() < 1e-6,
        "SINGLE_DRAWING_FADE must scale PaintShader.alpha 0.8 * 0.5 = 0.4, got {written_alpha}"
    );
}

#[test]
fn replayer_and_images_kotlin_handle_define_image_with_and_rgb565_color_space() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let kt_dir = manifest.join("../../android/compose-rs/src/main/java/rs/compose/android");
    let replayer = std::fs::read_to_string(kt_dir.join("Replayer.kt")).expect("Replayer.kt");
    let images =
        std::fs::read_to_string(kt_dir.join("ComposeRsImages.kt")).expect("ComposeRsImages.kt");

    // 1. `retireLayers` guard in `Replayer.replay` must NOT fire on `DEFINE_IMAGE_WITH` (tag 29 > 23),
    // which `Resources::write_pending` emits before `LAYERS`!
    let retire_at =
        replayer.rfind("retireLayers(buffer,").expect("retireLayers call in Replayer.kt");
    let guard_window = &replayer[retire_at.saturating_sub(320)..retire_at];
    assert!(
        guard_window.contains("DEFINE_IMAGE_WITH"),
        "Replayer.kt retireLayers guard must exclude DEFINE_IMAGE_WITH:\n{guard_window}"
    );

    // 2. `recordAhead` in `Replayer.kt` must dispatch `DEFINE_IMAGE_WITH -> defineImageWith(buffer)`.
    let ahead_at = replayer.find("fun recordAhead(").expect("recordAhead in Replayer.kt");
    let ahead_body = &replayer[ahead_at..];
    assert!(
        ahead_body.contains("DEFINE_IMAGE_WITH -> defineImageWith(buffer)"),
        "Replayer.kt recordAhead must handle DEFINE_IMAGE_WITH:\n{ahead_body}"
    );

    // 3. `ComposeRsImages.createPlatformBitmap` must pass `colorSpaceOf(colorSpaceId)` for RGB_565.
    let create_at =
        images.find("internal fun createPlatformBitmap(").expect("createPlatformBitmap");
    let create_body = &images[create_at..];
    let rgb565_at = create_body.find("Bitmap.Config.RGB_565").expect("RGB_565 branch");
    let rgb565_window = &create_body[rgb565_at..rgb565_at + 120.min(create_body.len() - rgb565_at)];
    assert!(
        rgb565_window.contains("colorSpaceOf(colorSpaceId)"),
        "ComposeRsImages.kt RGB_565 branch must pass colorSpaceOf(colorSpaceId):\n{rgb565_window}"
    );
}
