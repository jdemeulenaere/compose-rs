//! The JNI functions `ComposeRsNative.kt` declares, and the calls back into Kotlin.
//!
//! Everything runs on Android's main thread, where Kotlin calls in, except the executor's wake
//! (a task woken from another thread), which attaches its thread to post a `Handler` message.
//! A host is a `Box` whose pointer Kotlin keeps as a `long`.

use std::sync::OnceLock;

use compose_runtime::platform;
use compose_ui::{
    Density, IntSize, Key, KeyEvent, KeyEventType, KeyModifiers, LayoutDirection, Offset,
    PointerEvent, PointerId, PointerType, Rect,
    platform::{ParagraphMetrics, UiHost},
};
use jni::JNIEnv;
use jni::JavaVM;
use jni::objects::{
    GlobalRef, JByteArray, JByteBuffer, JClass, JIntArray, JMethodID, JObject, JStaticMethodID,
    JString, JValue,
};
use jni::signature::{Primitive, ReturnType};
use jni::sys::{jboolean, jdouble, jfloat, jint, jlong, jobject};

use crate::serialize::{GroupCache, Resources, Writer, write_display_list, write_groups_ahead};
use crate::text::{AndroidTextMeasurer, LaidOut, TextBackend};
use compose_ui::platform::Optimizations;

static VM: OnceLock<JavaVM> = OnceLock::new();
/// `ComposeRsNative`'s class and its `wake` method: what a task woken on any thread calls.
static WAKE: OnceLock<(GlobalRef, JStaticMethodID)> = OnceLock::new();

pub(crate) fn set_vm(vm: JavaVM) {
    install_panic_hook();
    #[cfg(target_os = "android")]
    compose_ui::platform::trace::set_tracer(trace_begin, trace_end);
    let _ = VM.set(vm);
    // `JNI_OnLoad` runs with the app's class loader: the decoder's class is found now, for
    // decodes on any thread later.
    if let Ok(mut env) = self::vm().get_env() {
        crate::image::install(&mut env);
        check(&mut env, "ComposeRsImages");
    }
}

#[cfg(target_os = "android")]
#[link(name = "android")]
unsafe extern "C" {
    fn ATrace_beginSection(name: *const std::ffi::c_char);
    fn ATrace_endSection();
}

#[cfg(target_os = "android")]
fn trace_begin(name: &'static std::ffi::CStr) {
    // SAFETY: a NUL-terminated name.
    unsafe { ATrace_beginSection(name.as_ptr()) };
}

#[cfg(target_os = "android")]
fn trace_end() {
    // SAFETY: ends the section `trace_begin` began on this thread.
    unsafe { ATrace_endSection() };
}

pub(crate) fn vm() -> &'static JavaVM {
    VM.get().expect("compose-android: JNI_OnLoad ran (export_apps!)")
}

/// The main thread's environment (attached, as Kotlin called in on it).
fn main_env() -> JNIEnv<'static> {
    vm().get_env().expect("compose-android: called on a thread attached to the VM")
}

// ---- Logging --------------------------------------------------------------------------------

#[cfg(target_os = "android")]
#[link(name = "log")]
unsafe extern "C" {
    fn __android_log_write(
        priority: std::ffi::c_int,
        tag: *const std::ffi::c_char,
        text: *const std::ffi::c_char,
    ) -> std::ffi::c_int;
}

/// Writes `message` to logcat (tag `compose-rs`), or stderr off Android.
pub(crate) fn log(message: &str) {
    #[cfg(target_os = "android")]
    {
        let text = std::ffi::CString::new(message.replace('\0', " ")).unwrap_or_default();
        // 6: ERROR.
        unsafe { __android_log_write(6, c"compose-rs".as_ptr(), text.as_ptr()) };
    }
    #[cfg(not(target_os = "android"))]
    eprintln!("compose-rs: {message}");
}

/// A panic says what it was in logcat first. Under the Android profiles (`android` and
/// `android-release`: `panic = "abort"`, D361), the process then aborts where the panic happened,
/// on whichever thread, caught or not. Built with unwinding (another profile), a panic reaching a
/// JNI function aborts at that boundary, and one caught on the way (a background task's, an
/// app's own `catch_unwind`) is logged and lets the process go on.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log(&format!("panic: {info}"));
        previous(info);
    }));
}

/// Logs and clears a pending Java exception (a Kotlin callback that threw).
pub(crate) fn check(env: &mut JNIEnv, what: &str) {
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
        log(&format!("an exception in {what}"));
    }
}

// ---- Text, through `TextMeasurer.kt` ---------------------------------------------------------

/// `TextMeasurer.kt`, called back with JNI.
struct JniText {
    measurer: GlobalRef,
    /// The Kotlin measurer's results: a direct buffer of floats it writes before returning.
    results: *const f32,
    layout: JMethodID,
    relayout: JMethodID,
    restyle: JMethodID,
    probe_layout: JMethodID,
    hit_test: JMethodID,
    cursor: JMethodID,
    release: JMethodID,
}

impl JniText {
    fn result(&self, index: usize) -> f32 {
        // SAFETY: the Kotlin measurer's direct buffer holds 8 floats and outlives it.
        unsafe { self.results.add(index).read_unaligned() }
    }

    /// A layout's results: its metrics, whether it cut lines, and the text's own width (NaN
    /// when Kotlin didn't measure it).
    fn laid_out(&self) -> LaidOut {
        let own = self.result(6);
        LaidOut {
            metrics: ParagraphMetrics {
                size: IntSize::new(self.result(0) as i32, self.result(1) as i32),
                first_baseline: self.result(2),
                last_baseline: self.result(3),
                line_count: self.result(4) as u32,
            },
            did_exceed_max_lines: self.result(5) != 0.0,
            max_intrinsic_width: if own.is_nan() { None } else { Some(own) },
        }
    }

    /// Zeroes the results first (the text's own width: NaN, unknown): a call that fails (an
    /// exception, a missing layout) reads them rather than the last call's.
    fn clear_results(&self) {
        for index in 0..8 {
            let value = if index == 6 { f32::NAN } else { 0.0 };
            // SAFETY: as `result`; Kotlin writes it only during calls, on this thread.
            unsafe { (self.results as *mut f32).add(index).write_unaligned(value) };
        }
    }
}

impl TextBackend for JniText {
    fn layout(&mut self, handle: u32, text: &str, request: &[u8]) -> LaidOut {
        self.clear_results();
        let mut env = main_env();
        let _ = env.with_local_frame(4, |env| -> jni::errors::Result<()> {
            let text = env.new_string(text)?;
            // SAFETY: Kotlin only reads the request, during the call.
            let request =
                unsafe { env.new_direct_byte_buffer(request.as_ptr() as *mut u8, request.len())? };
            // SAFETY: the method's signature is `(ILjava/lang/String;Ljava/nio/ByteBuffer;)V`.
            unsafe {
                env.call_method_unchecked(
                    &self.measurer,
                    self.layout,
                    ReturnType::Primitive(Primitive::Void),
                    &[
                        JValue::Int(handle as jint).as_jni(),
                        JValue::Object(&text).as_jni(),
                        JValue::Object(&request).as_jni(),
                    ],
                )?;
            }
            Ok(())
        });
        check(&mut env, "TextMeasurer.layout");
        self.laid_out()
    }

    fn relayout(
        &mut self,
        previous: u32,
        handle: u32,
        min_width: i32,
        max_width: i32,
    ) -> Option<LaidOut> {
        self.clear_results();
        let mut env = main_env();
        // SAFETY: the method's signature is `(IIII)Z`.
        let relaid = unsafe {
            env.call_method_unchecked(
                &self.measurer,
                self.relayout,
                ReturnType::Primitive(Primitive::Boolean),
                &[
                    JValue::Int(previous as jint).as_jni(),
                    JValue::Int(handle as jint).as_jni(),
                    JValue::Int(min_width).as_jni(),
                    JValue::Int(max_width).as_jni(),
                ],
            )
        };
        check(&mut env, "TextMeasurer.relayout");
        // False (nothing kept) or an exception: Rust lays the text out anew.
        relaid.and_then(|value| value.z()).unwrap_or(false).then(|| self.laid_out())
    }

    fn probe_layout(
        &mut self,
        previous: u32,
        handle: u32,
        min_width: i32,
        max_width: i32,
    ) -> Option<LaidOut> {
        self.clear_results();
        let mut env = main_env();
        // SAFETY: the method's signature is `(IIII)Z`.
        let laid = unsafe {
            env.call_method_unchecked(
                &self.measurer,
                self.probe_layout,
                ReturnType::Primitive(Primitive::Boolean),
                &[
                    JValue::Int(previous as jint).as_jni(),
                    JValue::Int(handle as jint).as_jni(),
                    JValue::Int(min_width).as_jni(),
                    JValue::Int(max_width).as_jni(),
                ],
            )
        };
        check(&mut env, "TextMeasurer.probeLayout");
        laid.and_then(|value| value.z()).unwrap_or(false).then(|| self.laid_out())
    }

    fn restyle(&mut self, previous: u32, handle: u32, color: u32, decoration: u8) -> bool {
        let mut env = main_env();
        // SAFETY: the method's signature is `(IIII)Z`.
        let restyled = unsafe {
            env.call_method_unchecked(
                &self.measurer,
                self.restyle,
                ReturnType::Primitive(Primitive::Boolean),
                &[
                    JValue::Int(previous as jint).as_jni(),
                    JValue::Int(handle as jint).as_jni(),
                    JValue::Int(color as jint).as_jni(),
                    JValue::Int(jint::from(decoration)).as_jni(),
                ],
            )
        };
        check(&mut env, "TextMeasurer.restyle");
        restyled.and_then(|value| value.z()).unwrap_or(false)
    }

    fn hit_test(&self, handle: u32, position: Offset) -> usize {
        let mut env = main_env();
        // SAFETY: the method's signature is `(IFF)I`.
        let index = unsafe {
            env.call_method_unchecked(
                &self.measurer,
                self.hit_test,
                ReturnType::Primitive(Primitive::Int),
                &[
                    JValue::Int(handle as jint).as_jni(),
                    JValue::Float(position.x).as_jni(),
                    JValue::Float(position.y).as_jni(),
                ],
            )
        };
        check(&mut env, "TextMeasurer.hitTest");
        index.and_then(|value| value.i()).unwrap_or(0).max(0) as usize
    }

    fn cursor(&self, handle: u32, offset: usize) -> Rect {
        self.clear_results();
        let mut env = main_env();
        // SAFETY: the method's signature is `(II)V`.
        let _ = unsafe {
            env.call_method_unchecked(
                &self.measurer,
                self.cursor,
                ReturnType::Primitive(Primitive::Void),
                &[JValue::Int(handle as jint).as_jni(), JValue::Int(offset as jint).as_jni()],
            )
        };
        check(&mut env, "TextMeasurer.cursor");
        Rect::new(self.result(0), self.result(1), self.result(2), self.result(3))
    }

    fn release(&mut self, handle: u32) {
        let mut env = main_env();
        // SAFETY: the method's signature is `(I)V`.
        let _ = unsafe {
            env.call_method_unchecked(
                &self.measurer,
                self.release,
                ReturnType::Primitive(Primitive::Void),
                &[JValue::Int(handle as jint).as_jni()],
            )
        };
        check(&mut env, "TextMeasurer.release");
    }
}

// ---- The clipboard, through the view --------------------------------------------------------

struct JniClipboard {
    view: GlobalRef,
}

impl compose_ui::ClipboardManager for JniClipboard {
    fn get_text(&self) -> Option<String> {
        let mut env = main_env();
        let text = env
            .call_method(&self.view, "clipboardText", "()Ljava/lang/String;", &[])
            .and_then(|value| value.l());
        check(&mut env, "ComposeRsView.clipboardText");
        let text = text.ok().filter(|text| !text.is_null())?;
        let text = JString::from(text);
        let value: Option<String> = env.get_string(&text).ok().map(Into::into);
        let _ = env.delete_local_ref(text);
        value
    }

    fn set_text(&self, text: &str) {
        let mut env = main_env();
        if let Ok(text) = env.new_string(text) {
            let _ = env.call_method(
                &self.view,
                "setClipboardText",
                "(Ljava/lang/String;)V",
                &[JValue::Object(&text)],
            );
            let _ = env.delete_local_ref(text);
        }
        check(&mut env, "ComposeRsView.setClipboardText");
    }
}

// ---- A view's host ---------------------------------------------------------------------------

/// A `ComposeRsView`'s composition, and the buffers its frames are drawn from.
struct Host {
    ui: UiHost,
    resources: Resources,
    /// The last frame's display list.
    list: Writer,
    /// The groups' records the last frame wrote, copied while their version stays.
    groups: GroupCache,
    /// What `take_drawing` last gave Kotlin: resource changes, then the display list (or what
    /// `draw_ahead` last gave it: resource changes, then the records of groups drawn ahead).
    drawing: Writer,
    /// The records of the groups `draw_ahead` drew last, kept for its capacity.
    ahead: Writer,
    /// The memory (address, capacity) of `drawing` that Kotlin's buffer wraps.
    drawing_given: (usize, usize),
    /// What `semantics` last gave Kotlin: the semantics tree.
    semantics: Writer,
    /// Nothing has been drawn yet.
    first: bool,
    /// The focused field's options the last frame saw: a change restarts the input method.
    ime_options: Option<compose_ui::ImeOptions>,
    /// Shown with performance hints enabled: its frames report the RenderThread's work
    /// (`performance_hint.rs`, D390).
    hint: bool,
}

/// The host behind a pointer Kotlin keeps.
///
/// # Safety
/// `host` came from `create` and wasn't destroyed; only the main thread uses it.
unsafe fn host<'a>(host: jlong) -> &'a mut Host {
    unsafe { &mut *(host as *mut Host) }
}

fn method(env: &mut JNIEnv, object: &JObject, name: &str, signature: &str) -> JMethodID {
    let class = env.get_object_class(object).expect("compose-android: a class");
    let id = env
        .get_method_id(&class, name, signature)
        .unwrap_or_else(|_| panic!("compose-android: {name}{signature}"));
    let _ = env.delete_local_ref(class);
    id
}

/// `ComposeRsNative.create(view, measurer, results, app, width, height, density, fontScale,
/// rtl, saved)`: a composition of the app named `app`, in a view `width` × `height` pixels,
/// its `remember_saveable`s given back what an earlier host saved (`saved`, or null).
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_create<'local>(
    mut env: JNIEnv<'local>,
    class: JClass<'local>,
    view: JObject<'local>,
    measurer: JObject<'local>,
    results: JByteBuffer<'local>,
    app: JString<'local>,
    width: jint,
    height: jint,
    density: jfloat,
    font_scale: jfloat,
    rtl: jboolean,
    saved: JByteArray<'local>,
    optimizations: jlong,
) -> jlong {
    if WAKE.get().is_none() {
        let class_ref = env.new_global_ref(&class).expect("compose-android: a global ref");
        let wake = env
            .get_static_method_id(&class, "wake", "()V")
            .expect("compose-android: ComposeRsNative.wake");
        let _ = WAKE.set((class_ref, wake));
        // A task woken on any thread posts a tick to the main thread.
        platform::set_wake_handler(|| {
            let (Some(vm), Some((class, wake))) = (VM.get(), WAKE.get()) else { return };
            let Ok(mut env) = vm.attach_current_thread_as_daemon() else { return };
            // SAFETY: `wake` is `()V`, static, of `class`.
            let _ = unsafe {
                env.call_static_method_unchecked(
                    <&JClass>::from(class.as_obj()),
                    *wake,
                    ReturnType::Primitive(Primitive::Void),
                    &[],
                )
            };
            check(&mut env, "ComposeRsNative.wake");
        });
    }
    let name: String = env.get_string(&app).map(Into::into).unwrap_or_default();
    let content = crate::app(&name).unwrap_or_else(|| {
        log(&format!("no app named {name:?} (export_apps!)"));
        || {}
    });
    let results =
        env.get_direct_buffer_address(&results).expect("compose-android: a direct buffer");
    let text = JniText {
        layout: method(
            &mut env,
            &measurer,
            "layout",
            "(ILjava/lang/String;Ljava/nio/ByteBuffer;)V",
        ),
        relayout: method(&mut env, &measurer, "relayout", "(IIII)Z"),
        restyle: method(&mut env, &measurer, "restyle", "(IIII)Z"),
        probe_layout: method(&mut env, &measurer, "probeLayout", "(IIII)Z"),
        hit_test: method(&mut env, &measurer, "hitTest", "(IFF)I"),
        cursor: method(&mut env, &measurer, "cursor", "(II)V"),
        release: method(&mut env, &measurer, "release", "(I)V"),
        measurer: env.new_global_ref(&measurer).expect("compose-android: a global ref"),
        results: results as *const f32,
    };
    let view_ref = env.new_global_ref(&view).expect("compose-android: a global ref");
    let mut ui = UiHost::new(
        IntSize::new(width, height),
        Density { density, font_scale },
        Box::new(AndroidTextMeasurer::new(text)),
    );
    ui.with_tree(|tree| {
        tree.set_clipboard(std::rc::Rc::new(JniClipboard { view: view_ref.clone() }))
    });
    // The replayer keeps a render node per group: those of unplaced items too (D380).
    ui.set_keep_unplaced_groups(true);
    let optimizations = Optimizations::from_bits(optimizations as u64);
    ui.set_optimizations(optimizations);
    crate::services::install(&mut env, &view, &view_ref, &ui);
    if rtl != 0 {
        ui.set_layout_direction(LayoutDirection::Rtl);
    }
    // A state change asks the view for a frame.
    let request_frame = method(&mut env, &view, "requestFrame", "()V");
    let waker_view = view_ref.clone();
    ui.set_frame_waker(move || {
        let mut env = main_env();
        // SAFETY: `requestFrame` is `()V`.
        let _ = unsafe {
            env.call_method_unchecked(
                &waker_view,
                request_frame,
                ReturnType::Primitive(Primitive::Void),
                &[],
            )
        };
        check(&mut env, "ComposeRsView.requestFrame");
    });
    // The content's events go to the view, which posts them to its listener: Kotlin code
    // never runs inside this call into Rust (it could call into the host again).
    let dispatch_event =
        method(&mut env, &view, "dispatchHostEvent", "(Ljava/lang/String;Ljava/lang/String;)V");
    let event_view = view_ref.clone();
    ui.set_event_listener(move |name, payload| {
        let mut env = main_env();
        let _ = env.with_local_frame(4, |env| -> jni::errors::Result<()> {
            let name = env.new_string(name)?;
            let payload = env.new_string(payload)?;
            // SAFETY: `dispatchHostEvent` is `(Ljava/lang/String;Ljava/lang/String;)V`.
            unsafe {
                env.call_method_unchecked(
                    &event_view,
                    dispatch_event,
                    ReturnType::Primitive(Primitive::Void),
                    &[JValue::Object(&name).as_jni(), JValue::Object(&payload).as_jni()],
                )?;
            }
            Ok(())
        });
        check(&mut env, "ComposeRsView.dispatchHostEvent");
    });
    // What an earlier host saved (a configuration change, the process recreated).
    if !saved.is_null()
        && let Ok(bytes) = env.convert_byte_array(&saved)
        && let Some(state) = compose_runtime::SavedState::from_bytes(&bytes)
    {
        ui.restore_state(state);
    }
    ui.set_content(content);
    let host = Box::new(Host {
        ui,
        resources: Resources::default(),
        list: Writer::default(),
        groups: GroupCache::new(optimizations),
        ahead: Writer::default(),
        drawing: Writer { bytes: Vec::with_capacity(4096) },
        drawing_given: (0, 0),
        semantics: Writer::default(),
        first: true,
        ime_options: None,
        hint: false,
    });
    Box::into_raw(host) as jlong
}

/// `ComposeRsNative.destroy(host)`: the composition goes (its effects disposed).
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_destroy(
    _env: JNIEnv,
    _class: JClass,
    host: jlong,
) {
    if host != 0 {
        // SAFETY: `host` came from `create`, and Kotlin forgets it now.
        let host = unsafe { Box::from_raw(host as *mut Host) };
        if host.hint {
            crate::performance_hint::hide();
        }
    }
}

/// `ComposeRsNative.setPerformanceHint(host, shown)`: whether the view shows with performance
/// hints enabled, so that its frames report the RenderThread's work to the process's hint
/// session (`performance_hint.rs`, D390).
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_setPerformanceHint(
    _env: JNIEnv,
    _class: JClass,
    host: jlong,
    shown: jboolean,
) {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    let shown = shown != 0;
    if shown != host.hint {
        host.hint = shown;
        if shown {
            crate::performance_hint::show();
        } else {
            crate::performance_hint::hide();
        }
    }
}

/// `ComposeRsNative.setOptimizations(host, bits)`: compose-rs's optimizations beyond Compose's
/// rendering (`ComposeRsView.optimizations`, D520), from the next frame on.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_setOptimizations(
    _env: JNIEnv,
    _class: JClass,
    host: jlong,
    bits: jlong,
) {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    let optimizations = Optimizations::from_bits(bits as u64);
    host.groups.set_optimizations(optimizations);
    host.ui.set_optimizations(optimizations);
}

/// `ComposeRsNative.parseOptimizations(text)`: the bits of a list of optimizations (`none`,
/// `all`, `default`, or names separated by commas, `-name` taking one out:
/// `Optimizations::parse`), or -1 if a name is unknown (logged).
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_parseOptimizations<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    text: JString<'local>,
) -> jlong {
    let text: String = env.get_string(&text).map(Into::into).unwrap_or_default();
    match Optimizations::parse(&text) {
        Ok(optimizations) => optimizations.bits() as jlong,
        Err(error) => {
            log(&format!("optimizations {text:?}: {error}"));
            -1
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_resize(
    _env: JNIEnv,
    _class: JClass,
    host: jlong,
    width: jint,
    height: jint,
) {
    // SAFETY: a live host, on the main thread.
    unsafe { self::host(host) }.ui.resize(IntSize::new(width, height));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_setDensity(
    _env: JNIEnv,
    _class: JClass,
    host: jlong,
    density: jfloat,
    font_scale: jfloat,
) {
    // SAFETY: a live host, on the main thread.
    unsafe { self::host(host) }.ui.set_density(Density { density, font_scale });
}

/// What `frame` returns: flags in the low 16 bits, and the milliseconds to the next `delay`'s
/// end plus one (0: none) above them.
mod flags {
    /// The display list changed: the view invalidates.
    pub const DREW: i64 = 1;
    /// Another frame is wanted (an animation, work left).
    pub const AGAIN: i64 = 2;
    /// A text field has focus: the soft keyboard shows.
    pub const TEXT_INPUT: i64 = 4;
    /// A field committed or dropped its composition itself, or the focused field's options
    /// changed: the input method restarts.
    pub const RESET_INPUT: i64 = 8;
    /// The semantics may have changed (a recomposition or a layout): accessibility hears it.
    pub const SEMANTICS: i64 = 16;
    /// Prefetch work waits for idle time: the view calls `prefetch` after the frame.
    pub const PREFETCH: i64 = 32;
    /// A `BackHandler` is enabled: the view intercepts back (otherwise the activity gets it).
    pub const BACK: i64 = 64;
    /// The content asked for the soft keyboard (`SoftwareKeyboardController::show`).
    pub const SHOW_KEYBOARD: i64 = 128;
    /// The content asked to hide the soft keyboard (a field's `Done`, `hide()`).
    pub const HIDE_KEYBOARD: i64 = 256;
    /// Where the timer's milliseconds start.
    pub const TIMER_SHIFT: u32 = 16;
}

fn next_timer_bits() -> i64 {
    // Rounded up: a tick never comes before the timer ends (and spins).
    platform::next_timer()
        .map_or(0, |wait| (wait.as_nanos().div_ceil(1_000_000) as i64 + 1) << flags::TIMER_SHIFT)
}

/// `ComposeRsNative.frame(host)`: recomposes, lays out and draws what changed.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_frame(
    _env: JNIEnv,
    _class: JClass,
    host: jlong,
) -> jlong {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    if host.hint {
        // The RenderThread's work for the frame before, read before this one's.
        crate::performance_hint::frame();
    }
    let _frame = compose_ui::platform::trace::section(c"compose-rs: frame");
    let stats = host.ui.frame();
    let mut result = 0;
    if stats.drew || host.first {
        let _serialize = compose_ui::platform::trace::section(c"compose-rs: serialize");
        host.first = false;
        let (list, resources, groups) = (&mut host.list, &mut host.resources, &mut host.groups);
        host.ui.with_tree(|tree| write_display_list(tree.display_list(), resources, groups, list));
        host.resources.collect();
        result |= flags::DREW;
    }
    if host.ui.has_pending_work() || platform::has_frame_awaiters() {
        result |= flags::AGAIN;
    }
    let input = crate::input_method::take_text_input_state(&host.ui, &mut host.ime_options);
    if input.active {
        result |= flags::TEXT_INPUT;
    }
    if input.restart {
        result |= flags::RESET_INPUT;
    }
    match input.keyboard {
        Some(true) => result |= flags::SHOW_KEYBOARD,
        Some(false) => result |= flags::HIDE_KEYBOARD,
        None => {}
    }
    if stats.recomposed || stats.measured {
        result |= flags::SEMANTICS;
    }
    if host.ui.has_enabled_back_handlers() {
        result |= flags::BACK;
    }
    if host.ui.has_prefetch_work() || host.ui.has_draw_ahead_work() {
        result |= flags::PREFETCH;
    }
    result | next_timer_bits()
}

/// `ComposeRsNative.prefetch(host, availableNanos, unboundedFirst)`: between frames, lazy lists
/// that scrolled compose and measure the items they'll reach next, for at most `available_nanos`
/// (the time to the next frame); with `unbounded_first`, the first request without a deadline
/// (Compose's scheduler when no frame was drawn for two intervals). Returns whether work is left,
/// for the next idle time.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_prefetch(
    _env: JNIEnv,
    _class: JClass,
    host: jlong,
    available_nanos: jlong,
    unbounded_first: jboolean,
) -> jboolean {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    let available = std::time::Duration::from_nanos(available_nanos.max(0) as u64);
    u8::from(host.ui.prefetch_with(std::time::Instant::now() + available, unbounded_first != 0))
}

/// `ComposeRsNative.hasDrawAhead(host)`: whether an item prefetched waits to be drawn ahead
/// (`drawAhead`).
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_hasDrawAhead(
    _env: JNIEnv,
    _class: JClass,
    host: jlong,
) -> jboolean {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    u8::from(host.ui.has_draw_ahead_work())
}

/// `ComposeRsNative.cancelDrawAhead(host)`: gives up drawing ahead the items waiting (an idle
/// time with nothing else to do was too short for one): the frames placing them draw them.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_cancelDrawAhead(
    _env: JNIEnv,
    _class: JClass,
    host: jlong,
) {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    host.ui.cancel_draw_ahead();
}

/// In what `drawAhead` returns: another item waits to be drawn ahead.
const MORE_AHEAD: jlong = 1 << 33;

/// `ComposeRsNative.drawAhead(host)`: between frames, draws ahead the next item prefetched and
/// not placed yet (`UiHost::draw_ahead`) and writes its groups' records (`write_groups_ahead`)
/// into the drawing's memory, after the resource changes not taken yet, for Kotlin to record
/// their render nodes now: the frame placing the item copies the records, versions unchanged,
/// and Kotlin keeps the nodes (D350). Returns the length as `takeDrawing` does (0: nothing
/// drawn), with `MORE_AHEAD` if another item waits. The next `takeDrawing` writes the frame's
/// drawing there again.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_drawAhead(
    _env: JNIEnv,
    _class: JClass,
    host: jlong,
) -> jlong {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    let Some(groups) = host.ui.draw_ahead() else { return 0 };
    {
        let _serialize = compose_ui::platform::trace::section(c"compose-rs: serialize ahead");
        write_groups_ahead(&groups, &mut host.resources, &mut host.groups, &mut host.ahead);
    }
    // The recordings go back to their boundaries alone: drawn again, they record in place.
    drop(groups);
    host.drawing.bytes.clear();
    host.resources.write_pending(&mut host.drawing);
    host.drawing.bytes.extend_from_slice(&host.ahead.bytes);
    host.drawing.bytes.reserve(1);
    let more = if host.ui.has_draw_ahead_work() { MORE_AHEAD } else { 0 };
    taken_drawing(&host.drawing.bytes, host.drawing_given) | more
}

/// `ComposeRsNative.nextFrameNanos(frameTimeNanos, intervalNanos, startedNanos)`: when the next
/// frame should start, from the last Choreographer callback's frame time and start, on the
/// vsync's phase (`frame_time.rs`, D340); 0 without a frame time.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_nextFrameNanos(
    _env: JNIEnv,
    _class: JClass,
    frame_time_nanos: jlong,
    interval_nanos: jlong,
    started_nanos: jlong,
) -> jlong {
    crate::frame_time::next_frame_nanos(frame_time_nanos, interval_nanos, started_nanos)
}

/// `ComposeRsNative.takeDrawing(host)`: what `onDraw` replays: the resources defined or dropped
/// since it last took them, then the last frame's display list. Returns its length, with
/// `NEW_DRAWING_BUFFER` set when its memory moved: Kotlin then asks for a new buffer
/// (`drawingBuffer`) instead of creating one each frame (`NewDirectByteBuffer` calls a Java
/// constructor).
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_takeDrawing(
    _env: JNIEnv,
    _class: JClass,
    host: jlong,
) -> jlong {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    let _take = compose_ui::platform::trace::section(c"compose-rs: take drawing");
    host.drawing.bytes.clear();
    host.resources.write_pending(&mut host.drawing);
    host.drawing.bytes.extend_from_slice(&host.list.bytes);
    host.drawing.bytes.reserve(1);
    taken_drawing(&host.drawing.bytes, host.drawing_given)
}

/// In what `takeDrawing` returns: the drawing's memory moved since Kotlin's buffer wrapped it.
const NEW_DRAWING_BUFFER: jlong = 1 << 32;

/// What `takeDrawing` returns for a drawing in `bytes`, Kotlin's buffer wrapping `given`
/// (address, capacity): the length, with `NEW_DRAWING_BUFFER` if the memory isn't that one.
/// The length fits in 31 bits (Kotlin reads it as an `Int`): a display list is kilobytes.
fn taken_drawing(bytes: &Vec<u8>, given: (usize, usize)) -> jlong {
    let memory = (bytes.as_ptr() as usize, bytes.capacity());
    let moved = if memory == given { 0 } else { NEW_DRAWING_BUFFER };
    (bytes.len() as jlong & i32::MAX as jlong) | moved
}

/// `ComposeRsNative.drawingBuffer(host)`: a direct buffer over the drawing's memory (all of its
/// capacity), valid until `takeDrawing` says it moved (or the host is destroyed).
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_drawingBuffer(
    mut env: JNIEnv,
    _class: JClass,
    host: jlong,
) -> jobject {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    let bytes = &mut host.drawing.bytes;
    host.drawing_given = (bytes.as_ptr() as usize, bytes.capacity());
    // SAFETY: the memory lives in the host until `takeDrawing` moves it (and says so) or
    // `destroy`; Kotlin reads it during `onDraw` only, up to the length `takeDrawing` returned.
    let buffer = unsafe { env.new_direct_byte_buffer(bytes.as_mut_ptr(), bytes.capacity()) };
    buffer.map_or(std::ptr::null_mut(), |buffer| buffer.into_raw())
}

/// `ComposeRsNative.touch(host, action, pointer, x, y, age)`: action 0 down, 1 move, 2 up, 3
/// cancel; positions in the view's pixels; `age` how long ago it happened (nanoseconds), so a
/// batch of samples keeps their times (a fling's velocity comes from them).
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_touch(
    _env: JNIEnv,
    _class: JClass,
    host: jlong,
    action: jint,
    pointer: jint,
    x: jfloat,
    y: jfloat,
    age: jlong,
) {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    let position = Offset::new(x, y);
    // Pointer 0 is the mouse's: touches start at 1.
    let pointer = PointerId(pointer as u64 + 1);
    let event = match action {
        0 => PointerEvent::press(position),
        1 => PointerEvent::moved(position, true),
        2 => PointerEvent::release(position),
        _ => {
            host.ui.with_tree(|tree| tree.cancel_pointer(pointer));
            return;
        }
    };
    let time = platform::now_nanos().saturating_sub(age.max(0) as u64).max(1);
    host.ui.send_pointer(event.with_pointer(pointer, PointerType::Touch).at_time(time));
}

/// `ComposeRsNative.scroll(host, x, y, dx, dy)`: a mouse wheel at (`x`, `y`), by pixels.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_scroll(
    _env: JNIEnv,
    _class: JClass,
    host: jlong,
    x: jfloat,
    y: jfloat,
    dx: jfloat,
    dy: jfloat,
) {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    host.ui.send_pointer(PointerEvent::scroll(Offset::new(x, y), Offset::new(dx, dy)));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_setRtl(
    _env: JNIEnv,
    _class: JClass,
    host: jlong,
    rtl: jboolean,
) {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    host.ui.set_layout_direction(if rtl != 0 {
        LayoutDirection::Rtl
    } else {
        LayoutDirection::Ltr
    });
}

/// `ComposeRsNative.textInputOptions(host)`: the focused text field's `EditorInfo`, its
/// `imeOptions` in the high 32 bits and its `inputType` in the low ones (`input_method.rs`).
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_textInputOptions(
    _env: JNIEnv,
    _class: JClass,
    host: jlong,
) -> jlong {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    fn password(node: &compose_ui::platform::SemanticsNode) -> Option<bool> {
        if node.config.focused && node.config.editable {
            return Some(node.config.password);
        }
        node.children.iter().find_map(password)
    }
    let options = host.ui.text_input_options().unwrap_or_default();
    let masks_password = password(&host.ui.semantics_tree(true)).unwrap_or(false);
    crate::input_method::editor_info(options, masks_password).pack()
}

/// `ComposeRsNative.performEditorAction(host, action)`: the soft keyboard's action key
/// (`EditorInfo.IME_ACTION_*`), for the focused field. Returns whether it took it.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_performEditorAction(
    _env: JNIEnv,
    _class: JClass,
    host: jlong,
    action: jint,
) -> jboolean {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    u8::from(host.ui.perform_ime_action(crate::input_method::ime_action(action)))
}

/// `ComposeRsNative.nextTimer()`: the milliseconds to the next `delay`'s end (rounded up), or
/// -1.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_nextTimer(
    _env: JNIEnv,
    _class: JClass,
) -> jlong {
    platform::next_timer().map_or(-1, |wait| wait.as_nanos().div_ceil(1_000_000) as jlong)
}

/// `ComposeRsNative.key(host, key, character, down, meta)`: a key (see `Keys` in
/// `ComposeRsView.kt`; 0 for a character), with its typed text for a character key down.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_key(
    _env: JNIEnv,
    _class: JClass,
    host: jlong,
    key: jint,
    character: jint,
    down: jboolean,
    meta: jint,
) -> jboolean {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    let character = char::from_u32(character as u32).filter(|c| *c != '\0');
    let key = match key {
        1 => Key::Enter,
        2 => Key::Space,
        3 => Key::Tab,
        4 => Key::Backspace,
        5 => Key::Delete,
        6 => Key::Escape,
        7 => Key::DirectionLeft,
        8 => Key::DirectionRight,
        9 => Key::DirectionUp,
        10 => Key::DirectionDown,
        11 => Key::MoveHome,
        12 => Key::MoveEnd,
        13 => Key::PageUp,
        14 => Key::PageDown,
        _ => match character {
            Some(c) => Key::Character(c),
            None => return 0,
        },
    };
    // Android's meta state: shift 1, alt 2, ctrl 0x1000, meta 0x10000.
    let modifiers = KeyModifiers {
        shift: meta & 0x1 != 0,
        alt: meta & 0x2 != 0,
        ctrl: meta & 0x1000 != 0,
        meta: meta & 0x10000 != 0,
    };
    let kind = if down != 0 { KeyEventType::KeyDown } else { KeyEventType::KeyUp };
    let consumed = host.ui.send_key(KeyEvent { key, kind, modifiers });
    // A typed character's text, unless it's a shortcut, as the desktop host does.
    if down != 0
        && !modifiers.ctrl
        && !modifiers.meta
        && let Some(c) = character.filter(|c| !c.is_control())
    {
        return u8::from(host.ui.send_text(&c.to_string()) || consumed);
    }
    u8::from(consumed)
}

/// `ComposeRsNative.text(host, text)`: text an input method committed.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_text(
    mut env: JNIEnv,
    _class: JClass,
    host: jlong,
    text: JString,
) {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    if let Ok(text) = env.get_string(&text) {
        let text: String = text.into();
        host.ui.send_text(&text);
    }
}

/// `ComposeRsNative.preedit(host, text, cursor)`: the text an input method is composing (empty
/// when it's cancelled), its cursor at UTF-16 `cursor` (-1 at its end).
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_preedit(
    mut env: JNIEnv,
    _class: JClass,
    host: jlong,
    text: JString,
    cursor: jint,
) {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    if let Ok(text) = env.get_string(&text) {
        let text: String = text.into();
        let cursor = (cursor >= 0).then(|| {
            let at = crate::text::byte_offset(&text, cursor as usize);
            (at, at)
        });
        host.ui.send_preedit(&text, cursor);
    }
}

/// `ComposeRsNative.tick()`: runs the `delay`s that ended and the tasks woken. Returns the
/// milliseconds to the next `delay`'s end plus one (0: none), shifted left once, with bit 0
/// set if a task waits for a frame.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_tick(
    _env: JNIEnv,
    _class: JClass,
) -> jlong {
    platform::fire_timers();
    platform::run_ready();
    let frames = i64::from(platform::has_frame_awaiters());
    let timer =
        platform::next_timer().map_or(0, |wait| wait.as_nanos().div_ceil(1_000_000) as i64 + 1);
    (timer << 1) | frames
}

/// `ComposeRsNative.semantics(host)`: the merged semantics tree, for `Accessibility.kt`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_semantics(
    mut env: JNIEnv,
    _class: JClass,
    host: jlong,
) -> jobject {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    let root = host.ui.semantics_tree(true);
    crate::accessibility::write_tree(&root, &mut host.semantics);
    host.semantics.bytes.reserve(1);
    // SAFETY: the buffer lives in the host until the next `semantics` (or `destroy`), and
    // Kotlin reads it before calling again.
    let buffer = unsafe {
        env.new_direct_byte_buffer(host.semantics.bytes.as_mut_ptr(), host.semantics.bytes.len())
    };
    buffer.map_or(std::ptr::null_mut(), |buffer| buffer.into_raw())
}

/// `ComposeRsNative.performAction(host, id, action, value)`: an accessibility service's action
/// on virtual view `id`: 0 click, 1 focus, 2 set progress to `value`, 3 to 8 scroll a page
/// (the viewport as shown) forward, backward, up, down, left or right, as Compose's
/// accessibility delegate (`accessibility::scroll_amount`, D491, D496).
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_performAction(
    _env: JNIEnv,
    _class: JClass,
    host: jlong,
    id: jint,
    action: jint,
    value: jfloat,
) -> jboolean {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    let root = host.ui.semantics_tree(true);
    let Some((found, shown)) = crate::accessibility::find_shown(&root, id) else { return 0 };
    let node = found.id;
    let action = match action {
        0 => compose_ui::SemanticsAction::OnClick,
        1 => compose_ui::SemanticsAction::RequestFocus,
        2 => compose_ui::SemanticsAction::SetProgress(value),
        3..=8 => {
            let direction = 1u8 << (action - 3);
            let Some((x, y)) = crate::accessibility::scroll_amount(found, direction, shown) else {
                return 0;
            };
            compose_ui::SemanticsAction::ScrollBy { x, y }
        }
        _ => return 0,
    };
    u8::from(host.ui.perform_semantics_action(node, action))
}

/// `ComposeRsNative.saveState(host)`: what the content's `remember_saveable`s hold, as bytes
/// for the view's saved instance state (null when there's nothing to save).
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_saveState<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    host: jlong,
) -> jobject {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    let state = host.ui.save_state();
    if state.is_empty() {
        return std::ptr::null_mut();
    }
    env.byte_array_from_slice(&state.to_bytes())
        .map_or(std::ptr::null_mut(), |array| array.into_raw())
}

/// `ComposeRsNative.onBackPressed(host)`: the user went back; the innermost enabled
/// `BackHandler` gets it. Returns whether one did.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_onBackPressed(
    _env: JNIEnv,
    _class: JClass,
    host: jlong,
) -> jboolean {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    u8::from(host.ui.on_back_pressed())
}

/// `ComposeRsNative.setConfiguration(host, widthDp, heightDp, landscape, fontScale, density,
/// night)`: Android's `Configuration`, as `LOCAL_CONFIGURATION`.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_setConfiguration(
    _env: JNIEnv,
    _class: JClass,
    host: jlong,
    width_dp: jint,
    height_dp: jint,
    landscape: jboolean,
    font_scale: jfloat,
    density: jfloat,
    night: jboolean,
) {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    host.ui.set_configuration(compose_ui::Configuration {
        screen_width_dp: width_dp as f32,
        screen_height_dp: height_dp as f32,
        orientation: if landscape != 0 {
            compose_ui::ScreenOrientation::Landscape
        } else {
            compose_ui::ScreenOrientation::Portrait
        },
        font_scale,
        density,
        night_mode: night != 0,
    });
}

/// `ComposeRsNative.setWindowInsets(host, insets)`: the window's insets in pixels, 4 per kind
/// (left, top, right, bottom): status bars, navigation bars, caption bar, keyboard, cutout.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_setWindowInsets<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    host: jlong,
    insets: JIntArray<'local>,
) {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    let mut values = [0; 20];
    if env.get_int_array_region(&insets, 0, &mut values).is_err() {
        return;
    }
    let side = |kind: usize| {
        let at = kind * 4;
        compose_ui::platform::Insets::new(
            values[at],
            values[at + 1],
            values[at + 2],
            values[at + 3],
        )
    };
    host.ui.set_window_insets(compose_ui::platform::PlatformWindowInsets {
        status_bars: side(0),
        navigation_bars: side(1),
        caption_bar: side(2),
        ime: side(3),
        display_cutout: side(4),
    });
}

/// `ComposeRsNative.setProp(host, name, kind, text, number)`: a property for the content
/// (`LOCAL_HOST_PROPS`): kind 0 removes it, 1 is `text`, 2 `number`, 3 a boolean (`number`
/// not 0).
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_setProp<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    host: jlong,
    name: JString<'local>,
    kind: jint,
    text: JString<'local>,
    number: jdouble,
) {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    let Ok(name) = env.get_string(&name).map(String::from) else { return };
    let value = match kind {
        1 => {
            let text: String = if text.is_null() {
                String::new()
            } else {
                env.get_string(&text).map(Into::into).unwrap_or_default()
            };
            Some(compose_ui::PropValue::Text(text.into()))
        }
        2 => Some(compose_ui::PropValue::Number(number)),
        3 => Some(compose_ui::PropValue::Bool(number != 0.0)),
        _ => None,
    };
    host.ui.props().set(&name, value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_taken_drawing_says_when_its_memory_moved() {
        let mut bytes = Vec::with_capacity(16);
        bytes.extend_from_slice(&[1, 2, 3]);
        let given = (bytes.as_ptr() as usize, bytes.capacity());
        assert_eq!(taken_drawing(&bytes, given), 3);
        assert_eq!(taken_drawing(&bytes, (0, 0)), 3 | NEW_DRAWING_BUFFER);
        // Grown past its capacity: a new allocation, or the same one larger.
        bytes.extend_from_slice(&[0; 64]);
        assert_eq!(taken_drawing(&bytes, given), 67 | NEW_DRAWING_BUFFER);
    }
}
