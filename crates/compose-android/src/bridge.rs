//! The JNI functions `ComposeRsNative.kt` declares, and the calls back into Kotlin.
//!
//! Everything runs on Android's main thread, where Kotlin calls in, except the executor's wake
//! (a task woken from another thread), which attaches its thread to post a `Handler` message.
//! A host is a `Box` whose pointer Kotlin keeps as a `long`.

use std::rc::Rc;
use std::sync::OnceLock;

use compose_runtime::platform;
use compose_ui::{
    Density, IntSize, Key, KeyEvent, KeyEventType, KeyModifiers, LayoutDirection, Offset,
    PointerEvent, PointerId, PointerType, Rect,
    platform::{ParagraphMetrics, UiHost},
};
use jni::ids::{JMethodID, JStaticMethodID};
use jni::objects::{JByteArray, JByteBuffer, JClass, JFloatArray, JIntArray, JObject, JString};
use jni::refs::Global;
use jni::signature::{MethodSignature, Primitive, ReturnType};
use jni::strings::JNIStr;
use jni::sys::{jboolean, jdouble, jfloat, jint, jlong};
use jni::{Env, EnvUnowned, JValue, JavaVM, Outcome, jni_sig, jni_str};

use crate::serialize::{GroupCache, Resources, Writer, write_display_list, write_groups_ahead};
use crate::text::{AndroidTextMeasurer, LaidOut, TextBackend};
use compose_ui::platform::Optimizations;

static VM: OnceLock<JavaVM> = OnceLock::new();
/// `ComposeRsNative`'s class and its `wake` method: what a task woken on any thread calls.
static WAKE: OnceLock<(Global<JClass<'static>>, JStaticMethodID)> = OnceLock::new();

pub(crate) fn set_vm(vm: JavaVM) {
    install_panic_hook();
    #[cfg(target_os = "android")]
    compose_ui::platform::trace::set_tracer(trace_begin, trace_end);
    let _ = VM.set(vm);
    // `JNI_OnLoad` runs with the app's class loader: the decoder's class is found now, for
    // decodes on any thread later.
    let _ = self::vm().with_top_local_frame(|env| -> jni::errors::Result<()> {
        crate::image::install(env);
        check(env, "ComposeRsImages");
        Ok(())
    });
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

/// Runs `f` with the main thread's environment (attached, as Kotlin called in on it), in its
/// top local frame.
pub(crate) fn with_main_env<T>(f: impl FnOnce(&mut Env) -> T) -> T {
    vm().with_top_local_frame(|env| Ok::<T, jni::errors::Error>(f(env)))
        .expect("compose-android: called on a thread attached to the VM")
}

/// Runs a native method's body with its environment. A panic isn't caught: it aborts at the JNI
/// boundary (D361).
fn with_env<'local, T>(env: &mut EnvUnowned<'local>, f: impl FnOnce(&mut Env<'local>) -> T) -> T {
    match env.with_env_no_catch(|env| Ok::<T, jni::errors::Error>(f(env))).into_outcome() {
        Outcome::Ok(value) => value,
        Outcome::Err(_) | Outcome::Panic(_) => unreachable!("the body returns Ok and isn't caught"),
    }
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
pub(crate) fn check(env: &Env, what: &str) {
    if env.exception_check() {
        env.exception_describe();
        env.exception_clear();
        log(&format!("an exception in {what}"));
    }
}

// ---- Text, through `TextMeasurer.kt` ---------------------------------------------------------

/// `TextMeasurer.kt`, called back with JNI.
struct JniText {
    measurer: Global<JObject<'static>>,
    /// The Kotlin measurer's results: a direct buffer of floats it writes before returning.
    results: *const f32,
    layout: JMethodID,
    relayout: JMethodID,
    restyle: JMethodID,
    probe_layout: JMethodID,
    min_intrinsic_width: JMethodID,
    hit_test: JMethodID,
    cursor: JMethodID,
    paragraph_layout: JMethodID,
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
        with_main_env(|env| {
            let _ = env.with_local_frame(4, |env| -> jni::errors::Result<()> {
                let text = env.new_string(text)?;
                // SAFETY: Kotlin only reads the request, during the call.
                let request = unsafe {
                    env.new_direct_byte_buffer(request.as_ptr() as *mut u8, request.len())?
                };
                // SAFETY: the method's signature is `(ILjava/lang/String;Ljava/nio/ByteBuffer;)V`.
                unsafe {
                    env.call_method_unchecked(
                        &*self.measurer,
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
            check(env, "TextMeasurer.layout");
        });
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
        let relaid = with_main_env(|env| {
            // SAFETY: the method's signature is `(IIII)Z`.
            let relaid = unsafe {
                env.call_method_unchecked(
                    &*self.measurer,
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
            check(env, "TextMeasurer.relayout");
            relaid.and_then(|value| value.z()).unwrap_or(false)
        });
        // False (nothing kept) or an exception: Rust lays the text out anew.
        relaid.then(|| self.laid_out())
    }

    fn probe_layout(
        &mut self,
        previous: u32,
        handle: u32,
        min_width: i32,
        max_width: i32,
    ) -> Option<LaidOut> {
        self.clear_results();
        let laid = with_main_env(|env| {
            // SAFETY: the method's signature is `(IIII)Z`.
            let laid = unsafe {
                env.call_method_unchecked(
                    &*self.measurer,
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
            check(env, "TextMeasurer.probeLayout");
            laid.and_then(|value| value.z()).unwrap_or(false)
        });
        laid.then(|| self.laid_out())
    }

    fn restyle(&mut self, previous: u32, handle: u32, color: u32, decoration: u8) -> bool {
        with_main_env(|env| {
            // SAFETY: the method's signature is `(IIII)Z`.
            let restyled = unsafe {
                env.call_method_unchecked(
                    &*self.measurer,
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
            check(env, "TextMeasurer.restyle");
            restyled.and_then(|value| value.z()).unwrap_or(false)
        })
    }

    fn min_intrinsic_width(&self, handle: u32) -> Option<f32> {
        with_main_env(|env| {
            // SAFETY: the method's signature is `(I)F`.
            let width = unsafe {
                env.call_method_unchecked(
                    &*self.measurer,
                    self.min_intrinsic_width,
                    ReturnType::Primitive(Primitive::Float),
                    &[JValue::Int(handle as jint).as_jni()],
                )
            };
            check(env, "TextMeasurer.minIntrinsicWidth");
            width.and_then(|value| value.f()).ok().filter(|width| !width.is_nan())
        })
    }

    fn hit_test(&self, handle: u32, position: Offset) -> usize {
        with_main_env(|env| {
            // SAFETY: the method's signature is `(IFF)I`.
            let index = unsafe {
                env.call_method_unchecked(
                    &*self.measurer,
                    self.hit_test,
                    ReturnType::Primitive(Primitive::Int),
                    &[
                        JValue::Int(handle as jint).as_jni(),
                        JValue::Float(position.x).as_jni(),
                        JValue::Float(position.y).as_jni(),
                    ],
                )
            };
            check(env, "TextMeasurer.hitTest");
            index.and_then(|value| value.i()).unwrap_or(0).max(0) as usize
        })
    }

    fn cursor(&self, handle: u32, offset: usize) -> Rect {
        self.clear_results();
        with_main_env(|env| {
            // SAFETY: the method's signature is `(II)V`.
            let _ = unsafe {
                env.call_method_unchecked(
                    &*self.measurer,
                    self.cursor,
                    ReturnType::Primitive(Primitive::Void),
                    &[JValue::Int(handle as jint).as_jni(), JValue::Int(offset as jint).as_jni()],
                )
            };
            check(env, "TextMeasurer.cursor");
        });
        Rect::new(self.result(0), self.result(1), self.result(2), self.result(3))
    }

    fn paragraph_layout(&self, handle: u32) -> Option<Vec<f32>> {
        with_main_env(|env| {
            let values = env.with_local_frame(2, |env| -> jni::errors::Result<Option<Vec<f32>>> {
                // SAFETY: the method's signature is `(I)[F`.
                let array = unsafe {
                    env.call_method_unchecked(
                        &*self.measurer,
                        self.paragraph_layout,
                        ReturnType::Object,
                        &[JValue::Int(handle as jint).as_jni()],
                    )
                }?
                .l()?;
                if array.is_null() {
                    return Ok(None);
                }
                // SAFETY: `paragraphLayout` returns a `float[]`, a local reference of this frame.
                let array = unsafe { JFloatArray::from_raw(env, array.into_raw()) };
                let mut values = vec![0.0; array.len(env)?];
                array.get_region(env, 0, &mut values)?;
                Ok(Some(values))
            });
            // An exception: the queries come from the cursors instead.
            check(env, "TextMeasurer.paragraphLayout");
            values.ok().flatten()
        })
    }

    fn release(&mut self, handle: u32) {
        with_main_env(|env| {
            // SAFETY: the method's signature is `(I)V`.
            let _ = unsafe {
                env.call_method_unchecked(
                    &*self.measurer,
                    self.release,
                    ReturnType::Primitive(Primitive::Void),
                    &[JValue::Int(handle as jint).as_jni()],
                )
            };
            check(env, "TextMeasurer.release");
        });
    }
}

// ---- The clipboard, through the view --------------------------------------------------------

struct JniClipboard {
    view: Rc<Global<JObject<'static>>>,
}

impl compose_ui::ClipboardManager for JniClipboard {
    fn get_text(&self) -> Option<String> {
        with_main_env(|env| {
            let text = env
                .call_method(
                    &**self.view,
                    jni_str!("clipboardText"),
                    jni_sig!("()Ljava/lang/String;"),
                    &[],
                )
                .and_then(|value| value.l());
            check(env, "ComposeRsView.clipboardText");
            let text = text.ok().filter(|text| !text.is_null())?;
            // SAFETY: `clipboardText` returns a `String`.
            let text = unsafe { JString::from_raw(env, text.into_raw()) };
            let value: Option<String> = text.mutf8_chars(env).ok().map(Into::into);
            env.delete_local_ref(text);
            value
        })
    }

    fn set_text(&self, text: &str) {
        with_main_env(|env| {
            if let Ok(text) = env.new_string(text) {
                let _ = env.call_method(
                    &**self.view,
                    jni_str!("setClipboardText"),
                    jni_sig!("(Ljava/lang/String;)V"),
                    &[JValue::Object(&text)],
                );
                env.delete_local_ref(text);
            }
            check(env, "ComposeRsView.setClipboardText");
        });
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
    /// The merged semantics tree, for the record, the actions and the input method's options.
    semantics_tree: crate::accessibility::KeptSemanticsTree,
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

pub(crate) fn method(
    env: &mut Env,
    object: &JObject,
    name: &JNIStr,
    signature: MethodSignature,
) -> JMethodID {
    let class = env.get_object_class(object).expect("compose-android: a class");
    let id = env
        .get_method_id(&class, name, &signature)
        .unwrap_or_else(|_| panic!("compose-android: {name}{}", signature.sig()));
    env.delete_local_ref(class);
    id
}

/// `ComposeRsNative.create(view, measurer, results, app, width, height, density, fontScale,
/// rtl, saved)`: a composition of the app named `app`, in a view `width` × `height` pixels,
/// its `remember_saveable`s given back what an earlier host saved (`saved`, or null).
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_create<'local>(
    mut env: EnvUnowned<'local>,
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
    lifecycle_state: jint,
    view_model_store: jlong,
) -> jlong {
    with_env(&mut env, |env| {
        create(
            env,
            &class,
            &view,
            &measurer,
            &results,
            &app,
            IntSize::new(width, height),
            Density { density, font_scale },
            rtl,
            &saved,
            optimizations,
            lifecycle_state,
            view_model_store,
        )
    })
}

#[allow(clippy::too_many_arguments)]
fn create(
    env: &mut Env,
    class: &JClass,
    view: &JObject,
    measurer: &JObject,
    results: &JByteBuffer,
    app: &JString,
    size: IntSize,
    density: Density,
    rtl: bool,
    saved: &JByteArray,
    optimizations: jlong,
    lifecycle_state: jint,
    view_model_store: jlong,
) -> jlong {
    if WAKE.get().is_none() {
        let class_ref = env.new_global_ref(class).expect("compose-android: a global ref");
        let wake = env
            .get_static_method_id(class, jni_str!("wake"), jni_sig!("()V"))
            .expect("compose-android: ComposeRsNative.wake");
        let _ = WAKE.set((class_ref, wake));
        // A task woken on any thread posts a tick to the main thread.
        platform::set_wake_handler(|| {
            let (Some(vm), Some((class, wake))) = (VM.get(), WAKE.get()) else { return };
            // Attached for good if it isn't (a pool's thread): detached when it ends.
            let _ = vm.attach_current_thread(|env| -> jni::errors::Result<()> {
                // SAFETY: `wake` is `()V`, static, of `class`.
                let _ = unsafe {
                    env.call_static_method_unchecked(
                        class,
                        *wake,
                        ReturnType::Primitive(Primitive::Void),
                        &[],
                    )
                };
                check(env, "ComposeRsNative.wake");
                Ok(())
            });
        });
    }
    let name: String = app.mutf8_chars(env).map(Into::into).unwrap_or_default();
    let content = crate::app(&name).unwrap_or_else(|| {
        log(&format!("no app named {name:?} (export_apps!)"));
        || {}
    });
    let results = env.get_direct_buffer_address(results).expect("compose-android: a direct buffer");
    let text = JniText {
        layout: method(
            env,
            measurer,
            jni_str!("layout"),
            jni_sig!("(ILjava/lang/String;Ljava/nio/ByteBuffer;)V"),
        ),
        relayout: method(env, measurer, jni_str!("relayout"), jni_sig!("(IIII)Z")),
        restyle: method(env, measurer, jni_str!("restyle"), jni_sig!("(IIII)Z")),
        probe_layout: method(env, measurer, jni_str!("probeLayout"), jni_sig!("(IIII)Z")),
        min_intrinsic_width: method(env, measurer, jni_str!("minIntrinsicWidth"), jni_sig!("(I)F")),
        hit_test: method(env, measurer, jni_str!("hitTest"), jni_sig!("(IFF)I")),
        cursor: method(env, measurer, jni_str!("cursor"), jni_sig!("(II)V")),
        paragraph_layout: method(env, measurer, jni_str!("paragraphLayout"), jni_sig!("(I)[F")),
        release: method(env, measurer, jni_str!("release"), jni_sig!("(I)V")),
        measurer: env.new_global_ref(measurer).expect("compose-android: a global ref"),
        results: results as *const f32,
    };
    let view_ref = Rc::new(env.new_global_ref(view).expect("compose-android: a global ref"));
    let mut ui = UiHost::new(size, density, Box::new(AndroidTextMeasurer::new(text)));
    ui.with_tree(|tree| tree.set_clipboard(Rc::new(JniClipboard { view: view_ref.clone() })));
    // The replayer keeps a render node per group: those of unplaced items too (D380).
    ui.set_keep_unplaced_groups(true);
    let optimizations = Optimizations::from_bits(optimizations as u64);
    ui.set_optimizations(optimizations);
    // Before the content: its effects hear no resume the owner hasn't had.
    ui.set_lifecycle_state(crate::lifecycle_state(lifecycle_state));
    // Kept by the view tree's `ViewModelStoreOwner` across a configuration change, if it has one.
    if let Some(store) = crate::retained_view_model_store(view_model_store) {
        ui.set_retained_view_model_store(store);
    }
    crate::services::install(env, view, &view_ref, &ui);
    if rtl {
        ui.set_layout_direction(LayoutDirection::Rtl);
    }
    // A state change asks the view for a frame.
    let request_frame = method(env, view, jni_str!("requestFrame"), jni_sig!("()V"));
    let waker_view = view_ref.clone();
    ui.set_frame_waker(move || {
        with_main_env(|env| {
            // SAFETY: `requestFrame` is `()V`.
            let _ = unsafe {
                env.call_method_unchecked(
                    &**waker_view,
                    request_frame,
                    ReturnType::Primitive(Primitive::Void),
                    &[],
                )
            };
            check(env, "ComposeRsView.requestFrame");
        });
    });
    // The content's events go to the view, which posts them to its listener: Kotlin code
    // never runs inside this call into Rust (it could call into the host again).
    let dispatch_event = method(
        env,
        view,
        jni_str!("dispatchHostEvent"),
        jni_sig!("(Ljava/lang/String;Ljava/lang/String;)V"),
    );
    let event_view = view_ref;
    ui.set_event_listener(move |name, payload| {
        with_main_env(|env| {
            let _ = env.with_local_frame(4, |env| -> jni::errors::Result<()> {
                let name = env.new_string(name)?;
                let payload = env.new_string(payload)?;
                // SAFETY: `dispatchHostEvent` is `(Ljava/lang/String;Ljava/lang/String;)V`.
                unsafe {
                    env.call_method_unchecked(
                        &**event_view,
                        dispatch_event,
                        ReturnType::Primitive(Primitive::Void),
                        &[JValue::Object(&name).as_jni(), JValue::Object(&payload).as_jni()],
                    )?;
                }
                Ok(())
            });
            check(env, "ComposeRsView.dispatchHostEvent");
        });
    });
    // What an earlier host saved (a configuration change, the process recreated).
    if !saved.is_null()
        && let Ok(bytes) = env.convert_byte_array(saved)
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
        semantics_tree: Default::default(),
        first: true,
        ime_options: None,
        hint: false,
    });
    Box::into_raw(host) as jlong
}

/// `ComposeRsNative.destroy(host)`: the composition goes (its effects disposed).
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_destroy(
    _env: EnvUnowned,
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
    _env: EnvUnowned,
    _class: JClass,
    host: jlong,
    shown: jboolean,
) {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
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
    _env: EnvUnowned,
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
/// `recommended`, `all`, `default`, or names separated by commas, `-name` taking one out:
/// `Optimizations::parse`), or -1 if a name is unknown (logged).
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_parseOptimizations<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    text: JString<'local>,
) -> jlong {
    let text: String =
        with_env(&mut env, |env| text.mutf8_chars(env).map(Into::into)).unwrap_or_default();
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
    _env: EnvUnowned,
    _class: JClass,
    host: jlong,
    width: jint,
    height: jint,
) {
    // SAFETY: a live host, on the main thread.
    unsafe { self::host(host) }.ui.resize(IntSize::new(width, height));
}

/// `ComposeRsNative.setLifecycleState(host, state)`: where the view's lifecycle owner is, by the
/// ordinal of `Lifecycle.State` (`crate::lifecycle_state`).
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_setLifecycleState(
    _env: EnvUnowned,
    _class: JClass,
    host: jlong,
    state: jint,
) {
    // SAFETY: a live host, on the main thread.
    unsafe { self::host(host) }.ui.set_lifecycle_state(crate::lifecycle_state(state));
}

/// `ComposeRsNative.newViewModelStore()`: a store for a holder in a view tree's
/// `ViewModelStoreOwner` (`AndroidXViewModels.kt`), by its token.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_newViewModelStore(
    _env: EnvUnowned,
    _class: JClass,
) -> jlong {
    crate::new_retained_view_model_store()
}

/// `ComposeRsNative.clearViewModelStore(token)`: that holder's cleared for good.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_clearViewModelStore(
    _env: EnvUnowned,
    _class: JClass,
    token: jlong,
) {
    crate::clear_retained_view_model_store(token);
}

/// `ComposeRsNative.setAnimatorDurationScale(scale)`: the setting's value (`ComposeRsRuntime.kt`
/// watches it), on the main thread, whose animations follow it.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_setAnimatorDurationScale(
    _env: EnvUnowned,
    _class: JClass,
    scale: jfloat,
) {
    crate::set_animator_duration_scale(scale);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_setDensity(
    _env: EnvUnowned,
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
    /// The semantics may have changed (a recomposition or a layout): accessibility reads them
    /// again when next asked, and the services on hear it (D630).
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
    _env: EnvUnowned,
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
    if host.semantics_tree.after_frame(&stats) {
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
    _env: EnvUnowned,
    _class: JClass,
    host: jlong,
    available_nanos: jlong,
    unbounded_first: jboolean,
) -> jboolean {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    let available = std::time::Duration::from_nanos(available_nanos.max(0) as u64);
    host.ui.prefetch_with(std::time::Instant::now() + available, unbounded_first)
}

/// `ComposeRsNative.hasDrawAhead(host)`: whether an item prefetched waits to be drawn ahead
/// (`drawAhead`).
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_hasDrawAhead(
    _env: EnvUnowned,
    _class: JClass,
    host: jlong,
) -> jboolean {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    host.ui.has_draw_ahead_work()
}

/// `ComposeRsNative.cancelDrawAhead(host)`: gives up drawing ahead the items waiting (an idle
/// time with nothing else to do was too short for one): the frames placing them draw them.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_cancelDrawAhead(
    _env: EnvUnowned,
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
    _env: EnvUnowned,
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
    _env: EnvUnowned,
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
    _env: EnvUnowned,
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
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_drawingBuffer<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    host: jlong,
) -> JByteBuffer<'local> {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    let bytes = &mut host.drawing.bytes;
    host.drawing_given = (bytes.as_ptr() as usize, bytes.capacity());
    with_env(&mut env, |env| {
        // SAFETY: the memory lives in the host until `takeDrawing` moves it (and says so) or
        // `destroy`; Kotlin reads it during `onDraw` only, up to the length `takeDrawing`
        // returned.
        unsafe { env.new_direct_byte_buffer(bytes.as_mut_ptr(), bytes.capacity()) }
            .unwrap_or_default()
    })
}

/// `ComposeRsNative.touch(host, action, pointer, x, y, age)`: action 0 down, 1 move, 2 up, 3
/// cancel; positions in the view's pixels; `age` how long ago it happened (nanoseconds), so a
/// batch of samples keeps their times (a fling's velocity comes from them).
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_touch(
    _env: EnvUnowned,
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
    _env: EnvUnowned,
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
    _env: EnvUnowned,
    _class: JClass,
    host: jlong,
    rtl: jboolean,
) {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    host.ui.set_layout_direction(if rtl { LayoutDirection::Rtl } else { LayoutDirection::Ltr });
}

/// `ComposeRsNative.textInputOptions(host)`: the focused text field's `EditorInfo`, its
/// `imeOptions` in the high 32 bits and its `inputType` in the low ones (`input_method.rs`).
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_textInputOptions(
    _env: EnvUnowned,
    _class: JClass,
    host: jlong,
) -> jlong {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    fn password(node: &compose_ui::platform::SemanticsNode) -> Option<bool> {
        use compose_ui::SemanticsProperties as P;
        let config = &node.config;
        if config.get(&P::FOCUSED) == Some(&true) && config.contains(&P::EDITABLE_TEXT) {
            return Some(config.contains(&P::PASSWORD));
        }
        node.children.iter().find_map(password)
    }
    let options = host.ui.text_input_options().unwrap_or_default();
    let masks_password = password(host.semantics_tree.get(&host.ui)).unwrap_or(false);
    crate::input_method::editor_info(options, masks_password).pack()
}

/// `ComposeRsNative.performEditorAction(host, action)`: the soft keyboard's action key
/// (`EditorInfo.IME_ACTION_*`), for the focused field. Returns whether it took it.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_performEditorAction(
    _env: EnvUnowned,
    _class: JClass,
    host: jlong,
    action: jint,
) -> jboolean {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    host.ui.perform_ime_action(crate::input_method::ime_action(action))
}

/// `ComposeRsNative.nextTimer()`: the milliseconds to the next `delay`'s end (rounded up), or
/// -1.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_nextTimer(
    _env: EnvUnowned,
    _class: JClass,
) -> jlong {
    platform::next_timer().map_or(-1, |wait| wait.as_nanos().div_ceil(1_000_000) as jlong)
}

/// `ComposeRsNative.key(host, key, character, down, meta)`: a key (see `Keys` in
/// `ComposeRsView.kt`; 0 for a character), with its typed text for a character key down.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_key(
    _env: EnvUnowned,
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
        15 => Key::DirectionCenter,
        _ => match character {
            Some(c) => Key::Character(c),
            None => return false,
        },
    };
    // Android's meta state: shift 1, alt 2, ctrl 0x1000, meta 0x10000.
    let modifiers = KeyModifiers {
        shift: meta & 0x1 != 0,
        alt: meta & 0x2 != 0,
        ctrl: meta & 0x1000 != 0,
        meta: meta & 0x10000 != 0,
    };
    let kind = if down { KeyEventType::KeyDown } else { KeyEventType::KeyUp };
    let consumed = host.ui.send_key(KeyEvent { key, kind, modifiers });
    // A typed character's text, unless it's a shortcut, as the desktop host does.
    if down
        && !modifiers.ctrl
        && !modifiers.meta
        && let Some(c) = character.filter(|c| !c.is_control())
    {
        return host.ui.send_text(&c.to_string()) || consumed;
    }
    consumed
}

/// `ComposeRsNative.text(host, text)`: text an input method committed.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_text(
    mut env: EnvUnowned,
    _class: JClass,
    host: jlong,
    text: JString,
) {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    if let Ok(text) = with_env(&mut env, |env| text.mutf8_chars(env).map(String::from)) {
        host.ui.send_text(&text);
    }
}

/// `ComposeRsNative.preedit(host, text, cursor)`: the text an input method is composing (empty
/// when it's cancelled), its cursor at UTF-16 `cursor` (-1 at its end).
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_preedit(
    mut env: EnvUnowned,
    _class: JClass,
    host: jlong,
    text: JString,
    cursor: jint,
) {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    if let Ok(text) = with_env(&mut env, |env| text.mutf8_chars(env).map(String::from)) {
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
    _env: EnvUnowned,
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
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_semantics<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    host: jlong,
) -> JByteBuffer<'local> {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    crate::accessibility::write_tree(host.semantics_tree.get(&host.ui), &mut host.semantics);
    host.semantics.bytes.reserve(1);
    let bytes = &mut host.semantics.bytes;
    with_env(&mut env, |env| {
        // SAFETY: the buffer lives in the host until the next `semantics` (or `destroy`), and
        // Kotlin reads it before calling again.
        unsafe { env.new_direct_byte_buffer(bytes.as_mut_ptr(), bytes.len()) }.unwrap_or_default()
    })
}

/// `ComposeRsNative.performAction(host, id, action, value)`: an accessibility service's action
/// on virtual view `id`: 0 click, 1 focus, 2 set progress to `value`, 3 to 8 scroll a page
/// (the viewport as shown) forward, backward, up, down, left or right, as Compose's
/// accessibility delegate (`accessibility::scroll_amount`, D491, D496).
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_performAction(
    _env: EnvUnowned,
    _class: JClass,
    host: jlong,
    id: jint,
    action: jint,
    value: jfloat,
) -> jboolean {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    let tree = host.semantics_tree.get(&host.ui);
    let Some((found, shown)) = crate::accessibility::find_shown(tree, id) else { return false };
    let node = found.id;
    use compose_ui::SemanticsActions as A;
    match action {
        0 => host.ui.perform_semantics_action(node, &A::ON_CLICK),
        1 => host.ui.perform_semantics_action(node, &A::REQUEST_FOCUS),
        2 => host.ui.perform_semantics_action_with(node, &A::SET_PROGRESS, |set| set(value)),
        3..=8 => {
            let direction = 1u8 << (action - 3);
            let Some((x, y)) = crate::accessibility::scroll_amount(found, direction, shown) else {
                return false;
            };
            host.ui.perform_semantics_action_with(node, &A::SCROLL_BY, |scroll| scroll(x, y))
        }
        _ => false,
    }
}

/// `ComposeRsNative.saveState(host)`: what the content's `remember_saveable`s hold, as bytes
/// for the view's saved instance state (null when there's nothing to save).
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_saveState<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    host: jlong,
) -> JByteArray<'local> {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    let state = host.ui.save_state();
    if state.is_empty() {
        return JByteArray::default();
    }
    with_env(&mut env, |env| env.byte_array_from_slice(&state.to_bytes()).unwrap_or_default())
}

/// `ComposeRsNative.onBackPressed(host)`: the user went back; the innermost enabled
/// `BackHandler` gets it. Returns whether one did.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_onBackPressed(
    _env: EnvUnowned,
    _class: JClass,
    host: jlong,
) -> jboolean {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    host.ui.on_back_pressed()
}

/// `ComposeRsNative.setConfiguration(host, widthDp, heightDp, landscape, fontScale, density,
/// night)`: Android's `Configuration`, as `LOCAL_CONFIGURATION`.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_setConfiguration(
    _env: EnvUnowned,
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
        orientation: if landscape {
            compose_ui::ScreenOrientation::Landscape
        } else {
            compose_ui::ScreenOrientation::Portrait
        },
        font_scale,
        density,
        night_mode: night,
    });
}

/// `ComposeRsNative.setWindowInsets(host, insets)`: the window's insets in pixels, 4 per insets
/// (left, top, right, bottom): the 10 types in `InsetsType::ALL`'s order, the 5 ignoring
/// visibility (status bars, navigation bars, caption bar, system bars, tappable element), the
/// keyboard animation's source and target; then the types hidden, by their index
/// (`ComposeRsInsets.values`).
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_setWindowInsets<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    host: jlong,
    insets: JIntArray<'local>,
) {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    let mut values = [0; WINDOW_INSETS_VALUES];
    if with_env(&mut env, |env| insets.get_region(env, 0, &mut values)).is_err() {
        return;
    }
    host.ui.set_window_insets(window_insets(&values));
}

/// The ints `ComposeRsInsets.values` holds.
const WINDOW_INSETS_VALUES: usize = 17 * 4 + 1;

/// The window's insets from `ComposeRsInsets.values`.
fn window_insets(
    values: &[i32; WINDOW_INSETS_VALUES],
) -> compose_ui::platform::PlatformWindowInsets {
    use compose_ui::platform::{Insets, InsetsType, PlatformWindowInsets};
    let side = |index: usize| {
        let at = index * 4;
        Insets::new(values[at], values[at + 1], values[at + 2], values[at + 3])
    };
    let mut insets = PlatformWindowInsets::default();
    for (index, kind) in InsetsType::ALL.into_iter().enumerate() {
        insets.set(kind, side(index));
        let hidden = values[17 * 4] & (1 << index) != 0;
        if kind != InsetsType::Waterfall {
            insets.set_visible(kind, !hidden);
        }
    }
    insets.status_bars_ignoring_visibility = side(10);
    insets.navigation_bars_ignoring_visibility = side(11);
    insets.caption_bar_ignoring_visibility = side(12);
    insets.system_bars_ignoring_visibility = side(13);
    insets.tappable_element_ignoring_visibility = side(14);
    insets.ime_animation_source = side(15);
    insets.ime_animation_target = side(16);
    insets
}

/// `ComposeRsNative.setWindowCutoutPath(host, points)`: the display cutout's outline, as
/// `android.graphics.Path.approximate` gives it, null for none.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_setWindowCutoutPath<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    host: jlong,
    points: JFloatArray<'local>,
) {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    let path = with_env(&mut env, |env| -> jni::errors::Result<_> {
        if points.is_null() {
            return Ok(None);
        }
        let mut values = vec![0.0; points.len(env)?];
        points.get_region(env, 0, &mut values)?;
        Ok(Some(values))
    });
    let Ok(points) = path else { return };
    host.ui.set_window_cutout_path(points.map(|points| path_of_approximation(&points)));
}

/// The path `android.graphics.Path.approximate` describes, a (fraction along the path, x, y) per
/// point: lines from point to point, a fraction repeated standing for a jump (where a contour
/// ends and the next starts).
fn path_of_approximation(points: &[f32]) -> compose_ui::Path {
    let path = compose_ui::Path::new();
    let mut fraction = None;
    for &[at, x, y] in points.as_chunks::<3>().0 {
        if fraction.is_none() || fraction == Some(at) {
            path.move_to(x, y);
        } else {
            path.line_to(x, y);
        }
        fraction = Some(at);
    }
    path
}

/// `ComposeRsNative.setProp(host, name, kind, text, number)`: a property for the content
/// (`LOCAL_HOST_PROPS`): kind 0 removes it, 1 is `text`, 2 `number`, 3 a boolean (`number`
/// not 0).
#[unsafe(no_mangle)]
pub extern "system" fn Java_rs_compose_android_ComposeRsNative_setProp<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    host: jlong,
    name: JString<'local>,
    kind: jint,
    text: JString<'local>,
    number: jdouble,
) {
    // SAFETY: a live host, on the main thread.
    let host = unsafe { self::host(host) };
    let (name, text) = with_env(&mut env, |env| {
        let name = name.mutf8_chars(env).map(String::from);
        // Read only for text properties.
        let text = if kind != 1 || text.is_null() {
            String::new()
        } else {
            text.mutf8_chars(env).map(Into::into).unwrap_or_default()
        };
        (name, text)
    });
    let Ok(name) = name else { return };
    let value = match kind {
        1 => Some(compose_ui::PropValue::Text(text.into())),
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

    #[test]
    fn the_listeners_values_are_the_insets_of_each_type() {
        use compose_ui::platform::{Insets, InsetsType};
        let mut values = [0; WINDOW_INSETS_VALUES];
        for (index, value) in values.iter_mut().enumerate() {
            *value = index as i32;
        }
        // The keyboard (index 3) and the system bars (index 5) hidden.
        values[17 * 4] = (1 << 3) | (1 << 5);
        let insets = window_insets(&values);
        let at = |index: i32| Insets::new(index * 4, index * 4 + 1, index * 4 + 2, index * 4 + 3);
        for (index, kind) in InsetsType::ALL.into_iter().enumerate() {
            assert_eq!(insets.get(kind), at(index as i32), "{kind:?}");
            assert_eq!(insets.is_visible(kind), index != 3 && index != 5, "{kind:?}");
        }
        assert_eq!(insets.status_bars_ignoring_visibility, at(10));
        assert_eq!(insets.navigation_bars_ignoring_visibility, at(11));
        assert_eq!(insets.caption_bar_ignoring_visibility, at(12));
        assert_eq!(insets.system_bars_ignoring_visibility, at(13));
        assert_eq!(insets.tappable_element_ignoring_visibility, at(14));
        assert_eq!(insets.ime_animation_source, at(15));
        assert_eq!(insets.ime_animation_target, at(16));
    }

    #[test]
    fn an_approximated_path_jumps_where_a_fraction_repeats() {
        use compose_ui::{Offset, PathCommand as Command};
        let path = path_of_approximation(&[
            0.0, 1.0, 2.0, 0.25, 3.0, 4.0, 0.5, 5.0, 6.0, //
            0.5, 7.0, 8.0, 1.0, 9.0, 10.0,
        ]);
        let data = path.data();
        assert_eq!(
            data.commands(),
            [
                Command::MoveTo(Offset::new(1.0, 2.0)),
                Command::LineTo(Offset::new(3.0, 4.0)),
                Command::LineTo(Offset::new(5.0, 6.0)),
                Command::MoveTo(Offset::new(7.0, 8.0)),
                Command::LineTo(Offset::new(9.0, 10.0)),
            ]
        );
    }
}
