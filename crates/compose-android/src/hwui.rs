//! Direct resolution of Android's `libhwui.so` native `RenderNode`, `RecordingCanvas`,
//! `Canvas`, `Paint`, `Path`, `Matrix` and `Shader` functions.
//!
//! When available (Android 10+ / API 29+), `compose-android` records `DisplayList`s straight
//! into native `RenderNode`s from Rust without serializing a byte buffer or crossing JNI per
//! drawing command from Kotlin. If any symbol fails to resolve on a future OS build, `get`
//! returns `None` and `ComposeRsView` falls back seamlessly to `serialize.rs` + `Replayer.kt`.

use std::ffi::{CStr, c_char, c_int, c_void};
use std::mem::transmute_copy;
use std::sync::OnceLock;

use jni::Env;
use jni::objects::{JClass, JObject};
use jni::refs::Global;
use jni::sys::{
    JNIEnv, jcharArray, jclass, jfieldID, jfloatArray, jintArray, jlongArray, jmethodID, jobject,
    jvalue,
};
use jni::{jni_sig, jni_str};

pub(crate) type UBiDi = c_void;
pub(crate) type UBiDiLevel = u8;
pub(crate) type UErrorCode = i32;

#[derive(Clone, Copy)]
pub(crate) struct IcuBidiApi {
    pub open: unsafe extern "C" fn() -> *mut UBiDi,
    pub close: unsafe extern "C" fn(*mut UBiDi),
    pub set_para: unsafe extern "C" fn(
        *mut UBiDi,
        *const u16,
        i32,
        UBiDiLevel,
        *mut UBiDiLevel,
        *mut UErrorCode,
    ),
    pub set_line: unsafe extern "C" fn(*const UBiDi, i32, i32, *mut UBiDi, *mut UErrorCode),
    pub get_para_level: unsafe extern "C" fn(*const UBiDi) -> UBiDiLevel,
    pub get_levels: unsafe extern "C" fn(*mut UBiDi, *mut UErrorCode) -> *const UBiDiLevel,
    pub count_paragraphs: unsafe extern "C" fn(*mut UBiDi) -> i32,
    pub count_runs: unsafe extern "C" fn(*mut UBiDi, *mut UErrorCode) -> i32,
    pub get_logical_run: unsafe extern "C" fn(*const UBiDi, i32, *mut i32, *mut UBiDiLevel),
    pub reorder_visual: unsafe extern "C" fn(*const UBiDiLevel, i32, *mut i32),
}

#[derive(Clone, Copy)]
pub(crate) enum GetFontMetricsIntFn {
    /// API 29..34: `(JLandroid/graphics/Paint$FontMetricsInt;)I`.
    V29(unsafe extern "C" fn(*mut JNIEnv, jobject, i64, jobject) -> i32),
    /// API 35+: `(JLandroid/graphics/Paint$FontMetricsInt;Z)I` (trailing `useLocale`).
    V35(unsafe extern "C" fn(*mut JNIEnv, jobject, i64, jobject, u8) -> i32),
}

#[derive(Clone, Copy)]
pub(crate) enum PaintGetRunCharAdvanceFn {
    /// API 33: `(J[CIIIIZI[FI)F`.
    V33(
        unsafe extern "C" fn(
            *mut JNIEnv,
            jclass,
            i64,
            jcharArray,
            i32,
            i32,
            i32,
            i32,
            u8,
            i32,
            jfloatArray,
            i32,
        ) -> f32,
    ),
    /// API 34+: `(J[CIIIIZI[FILandroid/graphics/RectF;Landroid/graphics/Paint$RunInfo;)F`.
    V34(
        unsafe extern "C" fn(
            *mut JNIEnv,
            jclass,
            i64,
            jcharArray,
            i32,
            i32,
            i32,
            i32,
            u8,
            i32,
            jfloatArray,
            i32,
            jobject,
            jobject,
        ) -> f32,
    ),
}

#[derive(Clone, Copy)]
pub(crate) enum BuildMeasuredTextFn {
    /// API 33..34: `(JJ[CZZZ)J`.
    V33(unsafe extern "C" fn(*mut JNIEnv, jclass, i64, i64, jcharArray, u8, u8, u8) -> i64),
    /// API 35+: `(JJ[CZZZZ)J` (trailing `fastHyphenationMode`).
    V35(unsafe extern "C" fn(*mut JNIEnv, jclass, i64, i64, jcharArray, u8, u8, u8, u8) -> i64),
}

#[derive(Clone, Copy)]
pub(crate) enum LineBreakerInitFn {
    /// API 29..34: `(IIZ[I)J`.
    V29(unsafe extern "C" fn(*mut JNIEnv, jclass, i32, i32, u8, jintArray) -> i64),
    /// API 35+: `(IIZ[IZ)J` (trailing `useBoundsForWidth`).
    V35(unsafe extern "C" fn(*mut JNIEnv, jclass, i32, i32, u8, jintArray, u8) -> i64),
}

pub(crate) struct TextHwuiApi {
    pub canvas_draw_text_run_chars: unsafe extern "C" fn(
        *mut JNIEnv,
        jobject,
        i64,
        jcharArray,
        i32,
        i32,
        i32,
        i32,
        f32,
        f32,
        u8,
        i64,
        i64,
    ),
    pub paint_init_with_paint: unsafe extern "C" fn(*mut JNIEnv, jclass, i64) -> i64,
    pub paint_get_text_run_cursor:
        unsafe extern "C" fn(*mut JNIEnv, jobject, i64, jcharArray, i32, i32, i32, i32, i32) -> i32,
    pub paint_get_run_advance: unsafe extern "C" fn(
        *mut JNIEnv,
        jclass,
        i64,
        jcharArray,
        i32,
        i32,
        i32,
        i32,
        u8,
        i32,
    ) -> f32,
    pub paint_get_run_char_advance: PaintGetRunCharAdvanceFn,
    pub paint_get_fmi_for_text:
        unsafe extern "C" fn(*mut JNIEnv, jclass, i64, jcharArray, i32, i32, i32, i32, u8, jobject),
    pub paint_get_fmi: GetFontMetricsIntFn,
    pub paint_get_flags: unsafe extern "C" fn(i64) -> i32,
    pub paint_set_typeface: unsafe extern "C" fn(i64, i64),
    pub paint_set_text_size: unsafe extern "C" fn(i64, f32),
    pub paint_set_letter_spacing: unsafe extern "C" fn(i64, f32),
    pub paint_set_word_spacing: unsafe extern "C" fn(i64, f32),
    pub paint_set_underline: unsafe extern "C" fn(i64, u8),
    pub paint_set_strikethru: unsafe extern "C" fn(i64, u8),
    pub paint_get_underline_position: unsafe extern "C" fn(i64) -> f32,
    pub paint_get_underline_thickness: unsafe extern "C" fn(i64) -> f32,
    pub paint_get_strikethru_position: unsafe extern "C" fn(i64) -> f32,
    pub paint_get_strikethru_thickness: unsafe extern "C" fn(i64) -> f32,

    pub mt_init_builder: unsafe extern "C" fn() -> i64,
    pub mt_add_style_run:
        unsafe extern "C" fn(*mut JNIEnv, jclass, i64, i64, i32, i32, u8, i32, i32, u8),
    pub mt_build: BuildMeasuredTextFn,
    pub mt_free_builder: unsafe extern "C" fn(*mut JNIEnv, jclass, i64),
    pub mt_get_width: unsafe extern "C" fn(i64, i32, i32) -> f32,
    pub mt_release: unsafe extern "C" fn(i64),
    pub mt_get_char_width_at: unsafe extern "C" fn(i64, i32) -> f32,

    pub lb_init: LineBreakerInitFn,
    pub lb_destroy: unsafe extern "C" fn(i64),
    pub lb_compute: unsafe extern "C" fn(
        *mut JNIEnv,
        jclass,
        i64,
        jcharArray,
        i64,
        i32,
        f32,
        i32,
        f32,
        jfloatArray,
        f32,
        i32,
    ) -> i64,
    pub lb_get_line_count: unsafe extern "C" fn(i64) -> i32,
    pub lb_get_line_break_offset: unsafe extern "C" fn(i64, i32) -> i32,
    pub lb_get_line_width: unsafe extern "C" fn(i64, i32) -> f32,
    pub lb_get_line_ascent: unsafe extern "C" fn(i64, i32) -> f32,
    pub lb_get_line_descent: unsafe extern "C" fn(i64, i32) -> f32,
    pub lb_get_line_flag: unsafe extern "C" fn(i64, i32) -> i32,
    pub lb_release_result: unsafe extern "C" fn(i64),

    pub _template_paint_obj: Global<JObject<'static>>,
    pub template_paint_ptr: i64,

    pub fmi_class: Global<JClass<'static>>,
    pub fmi_ctor: jmethodID,
    pub fmi_top: jfieldID,
    pub fmi_ascent: jfieldID,
    pub fmi_descent: jfieldID,
    pub fmi_bottom: jfieldID,
    pub fmi_leading: jfieldID,

    pub typeface_class: Global<JClass<'static>>,
    pub typeface_create_name: jmethodID,
    pub typeface_create_weight: jmethodID,
    pub typeface_native_field: jfieldID,
    pub tf_default: Global<JObject<'static>>,
    pub tf_serif: Global<JObject<'static>>,
    pub tf_monospace: Global<JObject<'static>>,
    pub tf_cursive: Global<JObject<'static>>,

    pub icu: IcuBidiApi,
}

#[derive(Clone, Copy)]
pub(crate) enum FinishRecordingFn {
    /// API 29+: `nFinishRecording(long renderer, long renderNode) -> void` (`@CriticalNative`).
    V29(unsafe extern "C" fn(i64, i64)),
    /// Legacy fallback: `nFinishRecording(long renderer) -> long` + `nSetDisplayList*(node, dl)`.
    Split { finish: unsafe extern "C" fn(i64) -> i64, set_dl: unsafe extern "C" fn(i64, i64) },
}

#[derive(Clone, Copy)]
pub(crate) enum OutlineRoundRectFn {
    /// API 29..35: `(JIIIIFF)Z` (`@CriticalNative`).
    V29(unsafe extern "C" fn(i64, i32, i32, i32, i32, f32, f32) -> u8),
    /// API 36+: `(JIIIIFFZ)Z` (`@CriticalNative`, trailing `outerShadowOnly`).
    V36(unsafe extern "C" fn(i64, i32, i32, i32, i32, f32, f32, u8) -> u8),
}

#[derive(Clone, Copy)]
pub(crate) enum OutlinePathFn {
    /// API 29..35: `(JJF)Z` (`@CriticalNative`).
    V29(unsafe extern "C" fn(i64, i64, f32) -> u8),
    /// API 36+: `(JJFZ)Z` (`@CriticalNative`, trailing `outerShadowOnly`).
    V36(unsafe extern "C" fn(i64, i64, f32, u8) -> u8),
}

pub(crate) struct HwuiApi {
    pub sdk_int: i32,

    // Exported NDK/system symbols from libhwui.so `.dynsym`.
    pub acanvas_from_java: unsafe extern "C" fn(*mut JNIEnv, jobject) -> *mut c_void,

    // DisplayListCanvas / RecordingCanvas (`@CriticalNative`).
    pub dlc_create: unsafe extern "C" fn(i64, i32, i32) -> i64,
    pub dlc_reset: unsafe extern "C" fn(i64, i64, i32, i32),
    pub dlc_enable_z: unsafe extern "C" fn(i64, u8),
    pub dlc_finish: FinishRecordingFn,
    pub dlc_draw_render_node: unsafe extern "C" fn(i64, i64),

    // RenderNode (regular JNI `nCreate`, finalizer, and `@CriticalNative` setters).
    pub rn_create: unsafe extern "C" fn(*mut JNIEnv, jclass, jobject) -> i64,
    pub rn_destroy: unsafe extern "C" fn(i64),
    pub rn_discard_dl: unsafe extern "C" fn(i64),
    pub rn_is_valid: unsafe extern "C" fn(i64) -> u8,
    pub rn_set_ltrb: unsafe extern "C" fn(i64, i32, i32, i32, i32) -> u8,
    pub rn_set_pivot_x: unsafe extern "C" fn(i64, f32) -> u8,
    pub rn_set_pivot_y: unsafe extern "C" fn(i64, f32) -> u8,
    pub rn_set_layer_type: unsafe extern "C" fn(i64, i32) -> u8,
    pub rn_set_layer_paint: unsafe extern "C" fn(i64, i64) -> u8,
    pub rn_set_clip_to_bounds: unsafe extern "C" fn(i64, u8) -> u8,
    pub rn_set_clip_to_outline: unsafe extern "C" fn(i64, u8) -> u8,
    pub rn_set_outline_none: unsafe extern "C" fn(i64) -> u8,
    pub rn_set_outline_empty: unsafe extern "C" fn(i64) -> u8,
    pub rn_set_outline_round_rect: OutlineRoundRectFn,
    pub rn_set_outline_path: OutlinePathFn,
    pub rn_set_spot_shadow_color: unsafe extern "C" fn(i64, i32) -> u8,
    pub rn_set_ambient_shadow_color: unsafe extern "C" fn(i64, i32) -> u8,
    pub rn_set_alpha: unsafe extern "C" fn(i64, f32) -> u8,
    pub rn_set_overlapping: unsafe extern "C" fn(i64, u8) -> u8,
    pub rn_set_elevation: unsafe extern "C" fn(i64, f32) -> u8,
    pub rn_set_translation_x: unsafe extern "C" fn(i64, f32) -> u8,
    pub rn_set_translation_y: unsafe extern "C" fn(i64, f32) -> u8,
    pub rn_set_rotation: unsafe extern "C" fn(i64, f32) -> u8,
    pub rn_set_scale_x: unsafe extern "C" fn(i64, f32) -> u8,
    pub rn_set_scale_y: unsafe extern "C" fn(i64, f32) -> u8,

    // Canvas state / transform / clip (`gMethods`).
    pub canvas_destroy: unsafe extern "C" fn(i64),
    pub canvas_get_width: unsafe extern "C" fn(i64) -> i32,
    pub canvas_get_height: unsafe extern "C" fn(i64) -> i32,
    pub canvas_save: unsafe extern "C" fn(i64, i32) -> i32,
    pub canvas_save_layer_alpha: unsafe extern "C" fn(i64, f32, f32, f32, f32, i32) -> i32,
    pub canvas_get_save_count: unsafe extern "C" fn(i64) -> i32,
    pub canvas_restore: unsafe extern "C" fn(i64) -> u8,
    pub canvas_restore_to_count: unsafe extern "C" fn(i64, i32),
    pub canvas_rotate: unsafe extern "C" fn(i64, f32),
    pub canvas_scale: unsafe extern "C" fn(i64, f32, f32),
    pub canvas_translate: unsafe extern "C" fn(i64, f32, f32),
    pub canvas_clip_rect: unsafe extern "C" fn(i64, f32, f32, f32, f32, i32) -> u8,
    pub canvas_clip_path: unsafe extern "C" fn(i64, i64, i32) -> u8,

    // Canvas draw operations (`gDrawMethods`, take `*mut JNIEnv, jobject`).
    pub canvas_draw_color: unsafe extern "C" fn(*mut JNIEnv, jobject, i64, i32, i32),
    pub canvas_draw_line: unsafe extern "C" fn(*mut JNIEnv, jobject, i64, f32, f32, f32, f32, i64),
    pub canvas_draw_rect: unsafe extern "C" fn(*mut JNIEnv, jobject, i64, f32, f32, f32, f32, i64),
    pub canvas_draw_round_rect:
        unsafe extern "C" fn(*mut JNIEnv, jobject, i64, f32, f32, f32, f32, f32, f32, i64),
    pub canvas_draw_circle: unsafe extern "C" fn(*mut JNIEnv, jobject, i64, f32, f32, f32, i64),
    pub canvas_draw_oval: unsafe extern "C" fn(*mut JNIEnv, jobject, i64, f32, f32, f32, f32, i64),
    pub canvas_draw_arc:
        unsafe extern "C" fn(*mut JNIEnv, jobject, i64, f32, f32, f32, f32, f32, f32, u8, i64),
    pub canvas_draw_path: unsafe extern "C" fn(*mut JNIEnv, jobject, i64, i64, i64),
    pub canvas_draw_bitmap_rect: unsafe extern "C" fn(
        *mut JNIEnv,
        jobject,
        i64,
        i64,
        f32,
        f32,
        f32,
        f32,
        f32,
        f32,
        f32,
        f32,
        i64,
        i32,
        i32,
    ),
    pub canvas_draw_text_chars:
        unsafe extern "C" fn(*mut JNIEnv, jobject, i64, jcharArray, i32, i32, f32, f32, i32, i64),

    // Paint.
    pub paint_init: unsafe extern "C" fn(*mut JNIEnv, jclass) -> i64,
    pub paint_destroy: unsafe extern "C" fn(i64),
    pub paint_set_flags: unsafe extern "C" fn(i64, i32),
    pub paint_set_style: unsafe extern "C" fn(i64, i32),
    pub paint_set_color: unsafe extern "C" fn(i64, i32),
    pub paint_set_alpha: unsafe extern "C" fn(i64, i32),
    pub paint_set_stroke_width: unsafe extern "C" fn(i64, f32),
    pub paint_set_stroke_cap: unsafe extern "C" fn(i64, i32),
    pub paint_set_shader: unsafe extern "C" fn(i64, i64) -> i64,

    // Path.
    pub path_init: unsafe extern "C" fn(*mut JNIEnv, jclass) -> i64,
    pub path_destroy: unsafe extern "C" fn(i64),
    pub path_set: unsafe extern "C" fn(*mut JNIEnv, jclass, i64, i64),
    pub path_move_to: unsafe extern "C" fn(*mut JNIEnv, jclass, i64, f32, f32),
    pub path_line_to: unsafe extern "C" fn(*mut JNIEnv, jclass, i64, f32, f32),
    pub path_quad_to: unsafe extern "C" fn(*mut JNIEnv, jclass, i64, f32, f32, f32, f32),
    pub path_cubic_to: unsafe extern "C" fn(*mut JNIEnv, jclass, i64, f32, f32, f32, f32, f32, f32),
    pub path_close: unsafe extern "C" fn(*mut JNIEnv, jclass, i64),
    pub path_add_round_rect:
        unsafe extern "C" fn(*mut JNIEnv, jclass, i64, f32, f32, f32, f32, jfloatArray, i32),
    pub path_add_path: unsafe extern "C" fn(*mut JNIEnv, jclass, i64, i64),
    pub path_offset: unsafe extern "C" fn(*mut JNIEnv, jclass, i64, f32, f32),
    pub path_op: unsafe extern "C" fn(*mut JNIEnv, jclass, i64, i64, i32, i64) -> u8,
    pub path_reset: unsafe extern "C" fn(i64),
    pub path_rewind: unsafe extern "C" fn(i64),
    pub path_is_empty: unsafe extern "C" fn(i64) -> u8,
    pub path_set_fill_type: unsafe extern "C" fn(i64, i32),

    // Matrix.
    pub matrix_create: unsafe extern "C" fn(*mut JNIEnv, jclass, i64) -> i64,
    pub matrix_destroy: unsafe extern "C" fn(i64),
    pub matrix_set_scale: unsafe extern "C" fn(i64, f32, f32),
    pub matrix_pre_translate: unsafe extern "C" fn(i64, f32, f32),

    // Shader.
    pub shader_linear_create: unsafe extern "C" fn(
        *mut JNIEnv,
        jclass,
        i64,
        f32,
        f32,
        f32,
        f32,
        jlongArray,
        jfloatArray,
        i32,
        i64,
    ) -> i64,
    pub shader_radial_create: unsafe extern "C" fn(
        *mut JNIEnv,
        jclass,
        i64,
        f32,
        f32,
        f32,
        f32,
        f32,
        f32,
        jlongArray,
        jfloatArray,
        i32,
        i64,
    ) -> i64,
    pub shader_sweep_create: unsafe extern "C" fn(
        *mut JNIEnv,
        jclass,
        i64,
        f32,
        f32,
        jlongArray,
        jfloatArray,
        i64,
    ) -> i64,
    pub shader_destroy: unsafe extern "C" fn(i64),

    // Unrestricted JNI handles (`@UnsupportedAppUsage` without `maxTargetSdk`, plus public SDK).
    pub paint_get_native_instance: jmethodID,
    pub canvas_wrapper_field: jfieldID,
    pub bitmap_class: Global<JClass<'static>>,
    pub bitmap_create: jmethodID,
    pub bitmap_copy_from_buffer: jmethodID,
    pub bitmap_recycle: jmethodID,
    pub bitmap_native_ptr_field: jfieldID,
    pub bitmap_config_argb8888: Global<JObject<'static>>,
    pub text: Option<TextHwuiApi>,
}

// SAFETY: All function pointers, method/field IDs, and `Global` JNI references are process-wide
// constants valid across threads once resolved.
unsafe impl Send for HwuiApi {}
unsafe impl Sync for HwuiApi {}

static HWUI_API: OnceLock<Option<HwuiApi>> = OnceLock::new();

#[inline]
pub(crate) fn get(env: &mut Env) -> Option<&'static HwuiApi> {
    HwuiApi::get(env)
}

impl HwuiApi {
    pub(crate) fn get(env: &mut Env) -> Option<&'static HwuiApi> {
        HWUI_API
            .get_or_init(|| {
                let resolved = Self::resolve(env);
                if env.exception_check() {
                    env.exception_clear();
                    crate::bridge::log("hwui: cleared JNI exception during symbol resolution");
                    return None;
                }
                if let Some(ref api) = resolved {
                    crate::bridge::log("hwui: direct RenderNode recorder active");
                    if api.text.is_some() {
                        crate::bridge::log("hwui: native BoringLayout/StaticLayout active");
                    } else {
                        crate::bridge::log(
                            "hwui: native text symbols unavailable; using TextMeasurer fallback",
                        );
                    }
                } else {
                    crate::bridge::log(
                        "hwui: direct RenderNode symbols unavailable; using Replayer fallback",
                    );
                }
                resolved
            })
            .as_ref()
    }

    #[inline(always)]
    pub(crate) unsafe fn finish_recording(&self, canvas: i64, node: i64) {
        match self.dlc_finish {
            FinishRecordingFn::V29(f) => unsafe { f(canvas, node) },
            FinishRecordingFn::Split { finish, set_dl } => unsafe {
                let dl = finish(canvas);
                set_dl(node, dl);
            },
        }
    }

    #[inline(always)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) unsafe fn set_outline_round_rect(
        &self,
        node: i64,
        l: i32,
        t: i32,
        r: i32,
        b: i32,
        radius: f32,
        alpha: f32,
    ) {
        match self.rn_set_outline_round_rect {
            OutlineRoundRectFn::V29(f) => unsafe {
                let _ = f(node, l, t, r, b, radius, alpha);
            },
            OutlineRoundRectFn::V36(f) => unsafe {
                let _ = f(node, l, t, r, b, radius, alpha, 0);
            },
        }
    }

    #[inline(always)]
    pub(crate) unsafe fn set_outline_path(&self, node: i64, path: i64, alpha: f32) {
        match self.rn_set_outline_path {
            OutlinePathFn::V29(f) => unsafe {
                let _ = f(node, path, alpha);
            },
            OutlinePathFn::V36(f) => unsafe {
                let _ = f(node, path, alpha, 0);
            },
        }
    }

    fn resolve(env: &mut Env) -> Option<HwuiApi> {
        let sdk_int = read_sdk_int();
        if sdk_int < 29 {
            return None;
        }

        let (entries, exports) = scan_libhwui()?;
        if entries.is_empty() || exports.acanvas_from_java == 0 {
            return None;
        }
        let acanvas_from_java: unsafe extern "C" fn(*mut JNIEnv, jobject) -> *mut c_void =
            unsafe { transmute_copy(&exports.acanvas_from_java) };

        let raw_env = env.get_raw();

        // 1. DisplayListCanvas / RecordingCanvas.
        let dlc_anchor = find_anchor(&entries, "nCreateDisplayListCanvas", "(JII)J")?;
        let dlc_win = window(entries.len(), dlc_anchor, 8, 18);
        let dlc_create = find_in(&entries, dlc_win.clone(), "nCreateDisplayListCanvas", "(JII)J")?;
        let dlc_reset = find_in(&entries, dlc_win.clone(), "nResetDisplayListCanvas", "(JJII)V")?;
        let dlc_enable_z = find_in(&entries, dlc_win.clone(), "nEnableZ", "(JZ)V")?;
        let dlc_draw_render_node = find_in(&entries, dlc_win.clone(), "nDrawRenderNode", "(JJ)V")?;

        // 2. RenderNode.
        let rn_anchor = find_anchor(&entries, "nSetLeftTopRightBottom", "(JIIII)Z")?;
        let rn_win = window(entries.len(), rn_anchor, 80, 80);
        let dlc_finish = if let Some(f) =
            find_in(&entries, dlc_win.clone(), "nFinishRecording", "(JJ)V")
        {
            FinishRecordingFn::V29(f)
        } else {
            let finish = find_in(&entries, dlc_win, "nFinishRecording", "(J)J")?;
            let set_dl = find_in(&entries, rn_win.clone(), "nSetDisplayList", "(JJ)V")
                .or_else(|| find_in(&entries, rn_win.clone(), "nSetDisplayListData", "(JJ)V"))?;
            FinishRecordingFn::Split { finish, set_dl }
        };

        let rn_create = find_in(&entries, rn_win.clone(), "nCreate", "(Ljava/lang/String;)J")?;
        let rn_get_finalizer: unsafe extern "C" fn(*mut JNIEnv, jclass) -> i64 =
            find_in(&entries, rn_win.clone(), "nGetNativeFinalizer", "()J")?;
        let rn_destroy_ptr = unsafe { rn_get_finalizer(raw_env, std::ptr::null_mut()) } as usize;
        if rn_destroy_ptr == 0 {
            return None;
        }
        let rn_destroy: unsafe extern "C" fn(i64) = unsafe { transmute_copy(&rn_destroy_ptr) };

        let rn_discard_dl = find_in(&entries, rn_win.clone(), "nDiscardDisplayList", "(J)V")?;
        let rn_is_valid = find_in(&entries, rn_win.clone(), "nIsValid", "(J)Z")?;
        let rn_set_ltrb = find_in(&entries, rn_win.clone(), "nSetLeftTopRightBottom", "(JIIII)Z")?;
        let rn_set_pivot_x = find_in(&entries, rn_win.clone(), "nSetPivotX", "(JF)Z")?;
        let rn_set_pivot_y = find_in(&entries, rn_win.clone(), "nSetPivotY", "(JF)Z")?;
        let rn_set_layer_type = find_in(&entries, rn_win.clone(), "nSetLayerType", "(JI)Z")?;
        let rn_set_layer_paint = find_in(&entries, rn_win.clone(), "nSetLayerPaint", "(JJ)Z")?;
        let rn_set_clip_to_bounds = find_in(&entries, rn_win.clone(), "nSetClipToBounds", "(JZ)Z")?;
        let rn_set_clip_to_outline =
            find_in(&entries, rn_win.clone(), "nSetClipToOutline", "(JZ)Z")?;
        let rn_set_outline_none = find_in(&entries, rn_win.clone(), "nSetOutlineNone", "(J)Z")?;
        let rn_set_outline_empty = find_in(&entries, rn_win.clone(), "nSetOutlineEmpty", "(J)Z")?;
        let rn_set_outline_round_rect = if let Some(f) =
            find_in(&entries, rn_win.clone(), "nSetOutlineRoundRect", "(JIIIIFF)Z")
        {
            OutlineRoundRectFn::V29(f)
        } else {
            OutlineRoundRectFn::V36(find_in(
                &entries,
                rn_win.clone(),
                "nSetOutlineRoundRect",
                "(JIIIIFFZ)Z",
            )?)
        };
        let rn_set_outline_path =
            if let Some(f) = find_in(&entries, rn_win.clone(), "nSetOutlinePath", "(JJF)Z") {
                OutlinePathFn::V29(f)
            } else {
                OutlinePathFn::V36(find_in(&entries, rn_win.clone(), "nSetOutlinePath", "(JJFZ)Z")?)
            };
        let rn_set_spot_shadow_color =
            find_in(&entries, rn_win.clone(), "nSetSpotShadowColor", "(JI)Z")?;
        let rn_set_ambient_shadow_color =
            find_in(&entries, rn_win.clone(), "nSetAmbientShadowColor", "(JI)Z")?;
        let rn_set_alpha = find_in(&entries, rn_win.clone(), "nSetAlpha", "(JF)Z")?;
        let rn_set_overlapping =
            find_in(&entries, rn_win.clone(), "nSetHasOverlappingRendering", "(JZ)Z")?;
        let rn_set_elevation = find_in(&entries, rn_win.clone(), "nSetElevation", "(JF)Z")?;
        let rn_set_translation_x = find_in(&entries, rn_win.clone(), "nSetTranslationX", "(JF)Z")?;
        let rn_set_translation_y = find_in(&entries, rn_win.clone(), "nSetTranslationY", "(JF)Z")?;
        let rn_set_rotation = find_in(&entries, rn_win.clone(), "nSetRotation", "(JF)Z")?;
        let rn_set_scale_x = find_in(&entries, rn_win.clone(), "nSetScaleX", "(JF)Z")?;
        let rn_set_scale_y = find_in(&entries, rn_win, "nSetScaleY", "(JF)Z")?;

        // 3. Canvas (`gMethods`).
        let c_anchor = find_anchor(&entries, "nSaveLayerAlpha", "(JFFFFI)I")?;
        let c_win = window(entries.len(), c_anchor, 20, 30);
        let c_get_finalizer: unsafe extern "C" fn(*mut JNIEnv, jclass) -> i64 =
            find_in(&entries, c_win.clone(), "nGetNativeFinalizer", "()J")?;
        let c_destroy_ptr = unsafe { c_get_finalizer(raw_env, std::ptr::null_mut()) } as usize;
        if c_destroy_ptr == 0 {
            return None;
        }
        let canvas_destroy: unsafe extern "C" fn(i64) = unsafe { transmute_copy(&c_destroy_ptr) };

        let canvas_get_width = find_in(&entries, c_win.clone(), "nGetWidth", "(J)I")?;
        let canvas_get_height = find_in(&entries, c_win.clone(), "nGetHeight", "(J)I")?;
        let canvas_save = find_in(&entries, c_win.clone(), "nSave", "(JI)I")?;
        let canvas_save_layer_alpha =
            find_in(&entries, c_win.clone(), "nSaveLayerAlpha", "(JFFFFI)I")?;
        let canvas_get_save_count = find_in(&entries, c_win.clone(), "nGetSaveCount", "(J)I")?;
        let canvas_restore = find_in(&entries, c_win.clone(), "nRestore", "(J)Z")?;
        let canvas_restore_to_count = find_in(&entries, c_win.clone(), "nRestoreToCount", "(JI)V")?;
        let canvas_rotate = find_in(&entries, c_win.clone(), "nRotate", "(JF)V")?;
        let canvas_scale = find_in(&entries, c_win.clone(), "nScale", "(JFF)V")?;
        let canvas_translate = find_in(&entries, c_win.clone(), "nTranslate", "(JFF)V")?;
        let canvas_clip_rect = find_in(&entries, c_win.clone(), "nClipRect", "(JFFFFI)Z")?;
        let canvas_clip_path = find_in(&entries, c_win, "nClipPath", "(JJI)Z")?;

        // 4. Canvas (`gDrawMethods`).
        let cd_anchor = find_anchor(&entries, "nDrawRoundRect", "(JFFFFFFJ)V")?;
        let cd_win = window(entries.len(), cd_anchor, 16, 30);
        let canvas_draw_color = find_in(&entries, cd_win.clone(), "nDrawColor", "(JII)V")?;
        let canvas_draw_line = find_in(&entries, cd_win.clone(), "nDrawLine", "(JFFFFJ)V")?;
        let canvas_draw_rect = find_in(&entries, cd_win.clone(), "nDrawRect", "(JFFFFJ)V")?;
        let canvas_draw_round_rect =
            find_in(&entries, cd_win.clone(), "nDrawRoundRect", "(JFFFFFFJ)V")?;
        let canvas_draw_circle = find_in(&entries, cd_win.clone(), "nDrawCircle", "(JFFFJ)V")?;
        let canvas_draw_oval = find_in(&entries, cd_win.clone(), "nDrawOval", "(JFFFFJ)V")?;
        let canvas_draw_arc = find_in(&entries, cd_win.clone(), "nDrawArc", "(JFFFFFFZJ)V")?;
        let canvas_draw_path = find_in(&entries, cd_win.clone(), "nDrawPath", "(JJJ)V")?;
        let canvas_draw_bitmap_rect =
            find_in(&entries, cd_win.clone(), "nDrawBitmap", "(JJFFFFFFFFJII)V")?;
        let canvas_draw_text_chars = find_in(&entries, cd_win, "nDrawText", "(J[CIIFFIJ)V")?;

        // 5. Paint.
        let p_anchor = find_anchor(&entries, "nInitWithPaint", "(J)J")?;
        let p_win = window(entries.len(), p_anchor, 8, 100);
        let paint_init = find_in(&entries, p_win.clone(), "nInit", "()J")?;
        let paint_get_finalizer: unsafe extern "C" fn(*mut JNIEnv, jclass) -> i64 =
            find_in(&entries, p_win.clone(), "nGetNativeFinalizer", "()J")?;
        let paint_destroy_ptr =
            unsafe { paint_get_finalizer(raw_env, std::ptr::null_mut()) } as usize;
        if paint_destroy_ptr == 0 {
            return None;
        }
        let paint_destroy: unsafe extern "C" fn(i64) =
            unsafe { transmute_copy(&paint_destroy_ptr) };
        let paint_set_flags = find_in(&entries, p_win.clone(), "nSetFlags", "(JI)V")?;
        let paint_set_style = find_in(&entries, p_win.clone(), "nSetStyle", "(JI)V")?;
        let paint_set_color = find_in(&entries, p_win.clone(), "nSetColor", "(JI)V")?;
        let paint_set_alpha = find_in(&entries, p_win.clone(), "nSetAlpha", "(JI)V")?;
        let paint_set_stroke_width = find_in(&entries, p_win.clone(), "nSetStrokeWidth", "(JF)V")?;
        let paint_set_stroke_cap = find_in(&entries, p_win.clone(), "nSetStrokeCap", "(JI)V")?;
        let paint_set_shader = find_in(&entries, p_win, "nSetShader", "(JJ)J")?;

        // 6. Path.
        let path_anchor = find_anchor(&entries, "nIncReserve", "(JI)V")?;
        let path_win = window(entries.len(), path_anchor, 12, 60);
        let path_init = find_in(&entries, path_win.clone(), "nInit", "()J")?;
        let path_get_finalizer: unsafe extern "C" fn(*mut JNIEnv, jclass) -> i64 =
            find_in(&entries, path_win.clone(), "nGetFinalizer", "()J")?;
        let path_destroy_ptr =
            unsafe { path_get_finalizer(raw_env, std::ptr::null_mut()) } as usize;
        if path_destroy_ptr == 0 {
            return None;
        }
        let path_destroy: unsafe extern "C" fn(i64) = unsafe { transmute_copy(&path_destroy_ptr) };
        let path_set = find_in(&entries, path_win.clone(), "nSet", "(JJ)V")?;
        let path_move_to = find_in(&entries, path_win.clone(), "nMoveTo", "(JFF)V")?;
        let path_line_to = find_in(&entries, path_win.clone(), "nLineTo", "(JFF)V")?;
        let path_quad_to = find_in(&entries, path_win.clone(), "nQuadTo", "(JFFFF)V")?;
        let path_cubic_to = find_in(&entries, path_win.clone(), "nCubicTo", "(JFFFFFF)V")?;
        let path_close = find_in(&entries, path_win.clone(), "nClose", "(J)V")?;
        let path_add_round_rect =
            find_in(&entries, path_win.clone(), "nAddRoundRect", "(JFFFF[FI)V")?;
        let path_add_path = find_in(&entries, path_win.clone(), "nAddPath", "(JJ)V")?;
        let path_offset = find_in(&entries, path_win.clone(), "nOffset", "(JFF)V")?;
        let path_op = find_in(&entries, path_win.clone(), "nOp", "(JJIJ)Z")?;
        let path_reset = find_in(&entries, path_win.clone(), "nReset", "(J)V")?;
        let path_rewind = find_in(&entries, path_win.clone(), "nRewind", "(J)V")?;
        let path_is_empty = find_in(&entries, path_win.clone(), "nIsEmpty", "(J)Z")?;
        let path_set_fill_type = find_in(&entries, path_win, "nSetFillType", "(JI)V")?;

        // 7. Matrix.
        let m_anchor = find_anchor(
            &entries,
            "nSetRectToRect",
            "(JLandroid/graphics/RectF;Landroid/graphics/RectF;I)Z",
        )?;
        let m_win = window(entries.len(), m_anchor, 12, 60);
        let m_create_idx = find_index_in(&entries, m_win.clone(), "nCreate", "(J)J")?;
        let matrix_create = unsafe { transmute_copy(&entries[m_create_idx].fn_ptr) };
        let m_fin_win = window(entries.len(), m_create_idx, 4, 4);
        let matrix_get_finalizer: unsafe extern "C" fn(*mut JNIEnv, jclass) -> i64 =
            find_in(&entries, m_fin_win, "nGetNativeFinalizer", "()J")?;
        let matrix_destroy_ptr =
            unsafe { matrix_get_finalizer(raw_env, std::ptr::null_mut()) } as usize;
        if matrix_destroy_ptr == 0 {
            return None;
        }
        let matrix_destroy: unsafe extern "C" fn(i64) =
            unsafe { transmute_copy(&matrix_destroy_ptr) };
        let matrix_set_scale = find_in(&entries, m_win.clone(), "nSetScale", "(JFF)V")?;
        let matrix_pre_translate = find_in(&entries, m_win, "nPreTranslate", "(JFF)V")?;

        // 8. Shader.
        let s_linear_idx = find_anchor(&entries, "nativeCreate", "(JFFFF[J[FIJ)J")?;
        let shader_linear_create = unsafe { transmute_copy(&entries[s_linear_idx].fn_ptr) };
        let s_win = window(entries.len(), s_linear_idx, 10, 14);
        let shader_radial_create =
            find_in(&entries, s_win.clone(), "nativeCreate", "(JFFFFFF[J[FIJ)J")?;
        let shader_sweep_create = find_in(&entries, s_win.clone(), "nativeCreate", "(JFF[J[FJ)J")?;
        let shader_get_finalizer: unsafe extern "C" fn(*mut JNIEnv, jclass) -> i64 =
            find_in(&entries, s_win, "nativeGetFinalizer", "()J")?;
        let shader_destroy_ptr =
            unsafe { shader_get_finalizer(raw_env, std::ptr::null_mut()) } as usize;
        if shader_destroy_ptr == 0 {
            return None;
        }
        let shader_destroy: unsafe extern "C" fn(i64) =
            unsafe { transmute_copy(&shader_destroy_ptr) };

        // 9. Unrestricted JNI IDs (`Paint.getNativeInstance`, `BaseCanvas.mNativeCanvasWrapper`,
        // `Rect`, `Bitmap`).
        let paint_cls = env.find_class(jni_str!("android/graphics/Paint")).ok()?;
        let paint_get_native_instance =
            jni_get_method_id(raw_env, paint_cls.as_raw(), c"getNativeInstance", c"()J")?;
        env.delete_local_ref(paint_cls);

        let base_canvas_cls = env.find_class(jni_str!("android/graphics/BaseCanvas")).ok()?;
        let canvas_wrapper_field =
            jni_get_field_id(raw_env, base_canvas_cls.as_raw(), c"mNativeCanvasWrapper", c"J")?;
        env.delete_local_ref(base_canvas_cls);

        let bmp_local = env.find_class(jni_str!("android/graphics/Bitmap")).ok()?;
        let bitmap_create = jni_get_static_method_id(
            raw_env,
            bmp_local.as_raw(),
            c"createBitmap",
            c"(IILandroid/graphics/Bitmap$Config;)Landroid/graphics/Bitmap;",
        )?;
        let bitmap_copy_from_buffer = jni_get_method_id(
            raw_env,
            bmp_local.as_raw(),
            c"copyPixelsFromBuffer",
            c"(Ljava/nio/Buffer;)V",
        )?;
        let bitmap_recycle = jni_get_method_id(raw_env, bmp_local.as_raw(), c"recycle", c"()V")?;
        let bitmap_native_ptr_field =
            jni_get_field_id(raw_env, bmp_local.as_raw(), c"mNativePtr", c"J")?;
        let bitmap_class = env.new_global_ref(bmp_local).ok()?;

        let cfg_cls = env.find_class(jni_str!("android/graphics/Bitmap$Config")).ok()?;
        let cfg_obj = env
            .get_static_field(
                &cfg_cls,
                jni_str!("ARGB_8888"),
                jni_sig!("Landroid/graphics/Bitmap$Config;"),
            )
            .and_then(|v| v.l())
            .ok()?;
        env.delete_local_ref(cfg_cls);
        if cfg_obj.is_null() {
            return None;
        }
        let bitmap_config_argb8888 = env.new_global_ref(cfg_obj).ok()?;

        let text = resolve_text_api(env, sdk_int, &entries, paint_get_native_instance);
        let _ = jni_exception_clear(raw_env);

        Some(HwuiApi {
            sdk_int,
            acanvas_from_java,
            dlc_create,
            dlc_reset,
            dlc_enable_z,
            dlc_finish,
            dlc_draw_render_node,
            rn_create,
            rn_destroy,
            rn_discard_dl,
            rn_is_valid,
            rn_set_ltrb,
            rn_set_pivot_x,
            rn_set_pivot_y,
            rn_set_layer_type,
            rn_set_layer_paint,
            rn_set_clip_to_bounds,
            rn_set_clip_to_outline,
            rn_set_outline_none,
            rn_set_outline_empty,
            rn_set_outline_round_rect,
            rn_set_outline_path,
            rn_set_spot_shadow_color,
            rn_set_ambient_shadow_color,
            rn_set_alpha,
            rn_set_overlapping,
            rn_set_elevation,
            rn_set_translation_x,
            rn_set_translation_y,
            rn_set_rotation,
            rn_set_scale_x,
            rn_set_scale_y,
            canvas_destroy,
            canvas_get_width,
            canvas_get_height,
            canvas_save,
            canvas_save_layer_alpha,
            canvas_get_save_count,
            canvas_restore,
            canvas_restore_to_count,
            canvas_rotate,
            canvas_scale,
            canvas_translate,
            canvas_clip_rect,
            canvas_clip_path,
            canvas_draw_color,
            canvas_draw_line,
            canvas_draw_rect,
            canvas_draw_round_rect,
            canvas_draw_circle,
            canvas_draw_oval,
            canvas_draw_arc,
            canvas_draw_path,
            canvas_draw_bitmap_rect,
            canvas_draw_text_chars,
            paint_init,
            paint_destroy,
            paint_set_flags,
            paint_set_style,
            paint_set_color,
            paint_set_alpha,
            paint_set_stroke_width,
            paint_set_stroke_cap,
            paint_set_shader,
            path_init,
            path_destroy,
            path_set,
            path_move_to,
            path_line_to,
            path_quad_to,
            path_cubic_to,
            path_close,
            path_add_round_rect,
            path_add_path,
            path_offset,
            path_op,
            path_reset,
            path_rewind,
            path_is_empty,
            path_set_fill_type,
            matrix_create,
            matrix_destroy,
            matrix_set_scale,
            matrix_pre_translate,
            shader_linear_create,
            shader_radial_create,
            shader_sweep_create,
            shader_destroy,
            paint_get_native_instance,
            canvas_wrapper_field,
            bitmap_class,
            bitmap_create,
            bitmap_copy_from_buffer,
            bitmap_recycle,
            bitmap_native_ptr_field,
            bitmap_config_argb8888,
            text,
        })
    }
}

fn resolve_icu_bidi() -> Option<IcuBidiApi> {
    let handle = unsafe { libc::dlopen(c"libicu.so".as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL) };
    let sym = |name: &CStr| -> Option<usize> {
        let ptr = if !handle.is_null() {
            unsafe { libc::dlsym(handle, name.as_ptr()) }
        } else {
            unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) }
        };
        if ptr.is_null() { None } else { Some(ptr as usize) }
    };
    let open = sym(c"ubidi_open")?;
    let close = sym(c"ubidi_close")?;
    let set_para = sym(c"ubidi_setPara")?;
    let set_line = sym(c"ubidi_setLine")?;
    let get_para_level = sym(c"ubidi_getParaLevel")?;
    let get_levels = sym(c"ubidi_getLevels")?;
    let count_paragraphs = sym(c"ubidi_countParagraphs")?;
    let count_runs = sym(c"ubidi_countRuns")?;
    let get_logical_run = sym(c"ubidi_getLogicalRun")?;
    let reorder_visual = sym(c"ubidi_reorderVisual")?;
    Some(unsafe {
        IcuBidiApi {
            open: transmute_copy(&open),
            close: transmute_copy(&close),
            set_para: transmute_copy(&set_para),
            set_line: transmute_copy(&set_line),
            get_para_level: transmute_copy(&get_para_level),
            get_levels: transmute_copy(&get_levels),
            count_paragraphs: transmute_copy(&count_paragraphs),
            count_runs: transmute_copy(&count_runs),
            get_logical_run: transmute_copy(&get_logical_run),
            reorder_visual: transmute_copy(&reorder_visual),
        }
    })
}

fn resolve_text_api(
    env: &mut Env,
    sdk_int: i32,
    entries: &[MethodEntry],
    paint_get_native_instance: jmethodID,
) -> Option<TextHwuiApi> {
    if sdk_int < 33 {
        return None;
    }
    let icu = resolve_icu_bidi()?;
    let raw_env = env.get_raw();

    // Canvas `nDrawTextRun`.
    let cd_anchor = find_anchor(entries, "nDrawRoundRect", "(JFFFFFFJ)V")?;
    let cd_win = window(entries.len(), cd_anchor, 16, 30);
    let canvas_draw_text_run_chars = find_in(entries, cd_win, "nDrawTextRun", "(J[CIIIIFFZJJ)V")?;

    // Paint text functions.
    let p_anchor = find_anchor(entries, "nInitWithPaint", "(J)J")?;
    let p_win = window(entries.len(), p_anchor, 4, 120);
    let paint_init_with_paint = find_in(entries, p_win.clone(), "nInitWithPaint", "(J)J")?;
    let paint_get_text_run_cursor =
        find_in(entries, p_win.clone(), "nGetTextRunCursor", "(J[CIIIII)I")?;
    let paint_get_run_advance = find_in(entries, p_win.clone(), "nGetRunAdvance", "(J[CIIIIZI)F")?;
    let paint_get_run_char_advance = if let Some(f) = find_in(
        entries,
        p_win.clone(),
        "nGetRunCharacterAdvance",
        "(J[CIIIIZI[FILandroid/graphics/RectF;Landroid/graphics/Paint$RunInfo;)F",
    ) {
        PaintGetRunCharAdvanceFn::V34(f)
    } else {
        PaintGetRunCharAdvanceFn::V33(find_in(
            entries,
            p_win.clone(),
            "nGetRunCharacterAdvance",
            "(J[CIIIIZI[FI)F",
        )?)
    };
    let paint_get_fmi_for_text = find_in(
        entries,
        p_win.clone(),
        "nGetFontMetricsIntForText",
        "(J[CIIIIZLandroid/graphics/Paint$FontMetricsInt;)V",
    )?;
    let paint_get_fmi = if let Some(f) = find_in(
        entries,
        p_win.clone(),
        "nGetFontMetricsInt",
        "(JLandroid/graphics/Paint$FontMetricsInt;Z)I",
    ) {
        GetFontMetricsIntFn::V35(f)
    } else {
        GetFontMetricsIntFn::V29(find_in(
            entries,
            p_win.clone(),
            "nGetFontMetricsInt",
            "(JLandroid/graphics/Paint$FontMetricsInt;)I",
        )?)
    };
    let paint_get_flags = find_in(entries, p_win.clone(), "nGetFlags", "(J)I")?;
    let paint_set_typeface = find_in(entries, p_win.clone(), "nSetTypeface", "(JJ)V")?;
    let paint_set_text_size = find_in(entries, p_win.clone(), "nSetTextSize", "(JF)V")?;
    let paint_set_letter_spacing = find_in(entries, p_win.clone(), "nSetLetterSpacing", "(JF)V")?;
    let paint_set_word_spacing = find_in(entries, p_win.clone(), "nSetWordSpacing", "(JF)V")?;
    let paint_set_underline = find_in(entries, p_win.clone(), "nSetUnderlineText", "(JZ)V")?;
    let paint_set_strikethru = find_in(entries, p_win.clone(), "nSetStrikeThruText", "(JZ)V")?;
    let paint_get_underline_position =
        find_in(entries, p_win.clone(), "nGetUnderlinePosition", "(J)F")?;
    let paint_get_underline_thickness =
        find_in(entries, p_win.clone(), "nGetUnderlineThickness", "(J)F")?;
    let paint_get_strikethru_position =
        find_in(entries, p_win.clone(), "nGetStrikeThruPosition", "(J)F")?;
    let paint_get_strikethru_thickness =
        find_in(entries, p_win, "nGetStrikeThruThickness", "(J)F")?;

    // MeasuredText$Builder (`gMTBuilderMethods`, exact 5-entry window `0..=4`).
    let mtb_anchor = find_anchor(entries, "nAddStyleRun", "(JJIIZIIZ)V")?;
    let mtb_win = window(entries.len(), mtb_anchor, 1, 3);
    let mt_init_builder = find_in(entries, mtb_win.clone(), "nInitBuilder", "()J")?;
    let mt_add_style_run = find_in(entries, mtb_win.clone(), "nAddStyleRun", "(JJIIZIIZ)V")?;
    let mt_build =
        if let Some(f) = find_in(entries, mtb_win.clone(), "nBuildMeasuredText", "(JJ[CZZZZ)J") {
            BuildMeasuredTextFn::V35(f)
        } else {
            BuildMeasuredTextFn::V33(find_in(
                entries,
                mtb_win.clone(),
                "nBuildMeasuredText",
                "(JJ[CZZZ)J",
            )?)
        };
    let mt_free_builder = find_in(entries, mtb_win, "nFreeBuilder", "(J)V")?;

    // MeasuredText (`gMTMethods`, exact 6-entry window `0..=5`).
    let mt_anchor = find_anchor(entries, "nGetCharWidthAt", "(JI)F")?;
    let mt_win = window(entries.len(), mt_anchor, 5, 0);
    let mt_get_width = find_in(entries, mt_win.clone(), "nGetWidth", "(JII)F")?;
    let mt_get_release: unsafe extern "C" fn() -> i64 =
        find_in(entries, mt_win.clone(), "nGetReleaseFunc", "()J")?;
    let mt_release_ptr = unsafe { mt_get_release() } as usize;
    if mt_release_ptr == 0 {
        return None;
    }
    let mt_release: unsafe extern "C" fn(i64) = unsafe { transmute_copy(&mt_release_ptr) };
    let mt_get_char_width_at = find_in(entries, mt_win, "nGetCharWidthAt", "(JI)F")?;

    // LineBreaker (`gMethods`, exact 10-entry window `0..=9`).
    let lb_anchor = find_anchor(entries, "nComputeLineBreaks", "(J[CJIFIF[FFI)J")?;
    let lb_win = window(entries.len(), lb_anchor, 2, 7);
    let lb_init = if let Some(f) = find_in(entries, lb_win.clone(), "nInit", "(IIZ[IZ)J") {
        LineBreakerInitFn::V35(f)
    } else {
        LineBreakerInitFn::V29(find_in(entries, lb_win.clone(), "nInit", "(IIZ[I)J")?)
    };
    let lb_get_release: unsafe extern "C" fn() -> i64 =
        find_in(entries, lb_win.clone(), "nGetReleaseFunc", "()J")?;
    let lb_destroy_ptr = unsafe { lb_get_release() } as usize;
    if lb_destroy_ptr == 0 {
        return None;
    }
    let lb_destroy: unsafe extern "C" fn(i64) = unsafe { transmute_copy(&lb_destroy_ptr) };
    let lb_compute = find_in(entries, lb_win.clone(), "nComputeLineBreaks", "(J[CJIFIF[FFI)J")?;
    let lb_get_line_count = find_in(entries, lb_win.clone(), "nGetLineCount", "(J)I")?;
    let lb_get_line_break_offset =
        find_in(entries, lb_win.clone(), "nGetLineBreakOffset", "(JI)I")?;
    let lb_get_line_width = find_in(entries, lb_win.clone(), "nGetLineWidth", "(JI)F")?;
    let lb_get_line_ascent = find_in(entries, lb_win.clone(), "nGetLineAscent", "(JI)F")?;
    let lb_get_line_descent = find_in(entries, lb_win.clone(), "nGetLineDescent", "(JI)F")?;
    let lb_get_line_flag = find_in(entries, lb_win.clone(), "nGetLineFlag", "(JI)I")?;
    let lb_get_release_result: unsafe extern "C" fn() -> i64 =
        find_in(entries, lb_win, "nGetReleaseResultFunc", "()J")?;
    let lb_release_result_ptr = unsafe { lb_get_release_result() } as usize;
    if lb_release_result_ptr == 0 {
        return None;
    }
    let lb_release_result: unsafe extern "C" fn(i64) =
        unsafe { transmute_copy(&lb_release_result_ptr) };

    // Template `TextPaint(Paint.ANTI_ALIAS_FLAG)` for default flags and locale.
    let tp_cls = env.find_class(jni_str!("android/text/TextPaint")).ok()?;
    let tp_ctor = jni_get_method_id(raw_env, tp_cls.as_raw(), c"<init>", c"(I)V")?;
    let args = [jvalue { i: 1 }];
    let tp_raw =
        unsafe { ((**raw_env).v1_2.NewObjectA)(raw_env, tp_cls.as_raw(), tp_ctor, args.as_ptr()) };
    env.delete_local_ref(tp_cls);
    if tp_raw.is_null() || jni_exception_clear(raw_env) {
        return None;
    }
    let template_paint_ptr = unsafe {
        ((**raw_env).v1_2.CallLongMethodA)(
            raw_env,
            tp_raw,
            paint_get_native_instance,
            std::ptr::null(),
        )
    };
    if template_paint_ptr == 0 || jni_exception_clear(raw_env) {
        return None;
    }
    let tp_wrapped = unsafe { JObject::from_raw(env, tp_raw) };
    let _template_paint_obj = env.new_global_ref(&tp_wrapped).ok()?;
    env.delete_local_ref(tp_wrapped);

    // `Paint$FontMetricsInt`.
    let fmi_local = env.find_class(jni_str!("android/graphics/Paint$FontMetricsInt")).ok()?;
    let fmi_ctor = jni_get_method_id(raw_env, fmi_local.as_raw(), c"<init>", c"()V")?;
    let fmi_top = jni_get_field_id(raw_env, fmi_local.as_raw(), c"top", c"I")?;
    let fmi_ascent = jni_get_field_id(raw_env, fmi_local.as_raw(), c"ascent", c"I")?;
    let fmi_descent = jni_get_field_id(raw_env, fmi_local.as_raw(), c"descent", c"I")?;
    let fmi_bottom = jni_get_field_id(raw_env, fmi_local.as_raw(), c"bottom", c"I")?;
    let fmi_leading = jni_get_field_id(raw_env, fmi_local.as_raw(), c"leading", c"I")?;
    let fmi_class = env.new_global_ref(fmi_local).ok()?;

    // `Typeface`.
    let tf_local = env.find_class(jni_str!("android/graphics/Typeface")).ok()?;
    let typeface_create_name = jni_get_static_method_id(
        raw_env,
        tf_local.as_raw(),
        c"create",
        c"(Ljava/lang/String;I)Landroid/graphics/Typeface;",
    )?;
    let typeface_create_weight = jni_get_static_method_id(
        raw_env,
        tf_local.as_raw(),
        c"create",
        c"(Landroid/graphics/Typeface;IZ)Landroid/graphics/Typeface;",
    )?;
    let typeface_native_field =
        jni_get_field_id(raw_env, tf_local.as_raw(), c"native_instance", c"J")?;
    let static_tf = |env: &mut Env,
                     cls: &JClass,
                     field: &jni::strings::JNIStr|
     -> Option<Global<JObject<'static>>> {
        let obj = env
            .get_static_field(cls, field, jni_sig!("Landroid/graphics/Typeface;"))
            .and_then(|v| v.l())
            .ok()?;
        if obj.is_null() {
            return None;
        }
        let g = env.new_global_ref(&obj).ok()?;
        env.delete_local_ref(obj);
        Some(g)
    };
    let tf_default = static_tf(env, &tf_local, jni_str!("DEFAULT"))?;
    let tf_serif = static_tf(env, &tf_local, jni_str!("SERIF"))?;
    let tf_monospace = static_tf(env, &tf_local, jni_str!("MONOSPACE"))?;
    let cursive_str = env.new_string("cursive").ok()?;
    let cursive_args = [jvalue { l: cursive_str.as_raw() }, jvalue { i: 0 }];
    let cursive_raw = unsafe {
        ((**raw_env).v1_2.CallStaticObjectMethodA)(
            raw_env,
            tf_local.as_raw(),
            typeface_create_name,
            cursive_args.as_ptr(),
        )
    };
    env.delete_local_ref(cursive_str);
    if cursive_raw.is_null() || jni_exception_clear(raw_env) {
        return None;
    }
    let cursive_wrapped = unsafe { JObject::from_raw(env, cursive_raw) };
    let tf_cursive = env.new_global_ref(&cursive_wrapped).ok()?;
    env.delete_local_ref(cursive_wrapped);
    let typeface_class = env.new_global_ref(tf_local).ok()?;

    Some(TextHwuiApi {
        canvas_draw_text_run_chars,
        paint_init_with_paint,
        paint_get_text_run_cursor,
        paint_get_run_advance,
        paint_get_run_char_advance,
        paint_get_fmi_for_text,
        paint_get_fmi,
        paint_get_flags,
        paint_set_typeface,
        paint_set_text_size,
        paint_set_letter_spacing,
        paint_set_word_spacing,
        paint_set_underline,
        paint_set_strikethru,
        paint_get_underline_position,
        paint_get_underline_thickness,
        paint_get_strikethru_position,
        paint_get_strikethru_thickness,
        mt_init_builder,
        mt_add_style_run,
        mt_build,
        mt_free_builder,
        mt_get_width,
        mt_release,
        mt_get_char_width_at,
        lb_init,
        lb_destroy,
        lb_compute,
        lb_get_line_count,
        lb_get_line_break_offset,
        lb_get_line_width,
        lb_get_line_ascent,
        lb_get_line_descent,
        lb_get_line_flag,
        lb_release_result,
        _template_paint_obj,
        template_paint_ptr,
        fmi_class,
        fmi_ctor,
        fmi_top,
        fmi_ascent,
        fmi_descent,
        fmi_bottom,
        fmi_leading,
        typeface_class,
        typeface_create_name,
        typeface_create_weight,
        typeface_native_field,
        tf_default,
        tf_serif,
        tf_monospace,
        tf_cursive,
        icu,
    })
}

fn jni_get_method_id(raw: *mut JNIEnv, cls: jclass, name: &CStr, sig: &CStr) -> Option<jmethodID> {
    let id = unsafe { ((**raw).v1_2.GetMethodID)(raw, cls, name.as_ptr(), sig.as_ptr()) };
    if id.is_null() || jni_exception_clear(raw) { None } else { Some(id) }
}

fn jni_get_static_method_id(
    raw: *mut JNIEnv,
    cls: jclass,
    name: &CStr,
    sig: &CStr,
) -> Option<jmethodID> {
    let id = unsafe { ((**raw).v1_2.GetStaticMethodID)(raw, cls, name.as_ptr(), sig.as_ptr()) };
    if id.is_null() || jni_exception_clear(raw) { None } else { Some(id) }
}

fn jni_get_field_id(raw: *mut JNIEnv, cls: jclass, name: &CStr, sig: &CStr) -> Option<jfieldID> {
    let id = unsafe { ((**raw).v1_2.GetFieldID)(raw, cls, name.as_ptr(), sig.as_ptr()) };
    if id.is_null() || jni_exception_clear(raw) { None } else { Some(id) }
}

pub(crate) fn clear_exception_check(raw: *mut JNIEnv) -> bool {
    jni_exception_clear(raw)
}

fn jni_exception_clear(raw: *mut JNIEnv) -> bool {
    if unsafe { ((**raw).v1_2.ExceptionCheck)(raw) } {
        unsafe { ((**raw).v1_2.ExceptionClear)(raw) };
        true
    } else {
        false
    }
}

fn read_sdk_int() -> i32 {
    type PropGet = unsafe extern "C" fn(*const c_char, *mut c_char) -> c_int;
    let Some(prop_get): Option<PropGet> = dlsym_fn(c"__system_property_get") else { return 0 };
    let mut buf = [0 as c_char; 92];
    let len = unsafe { prop_get(c"ro.build.version.sdk".as_ptr(), buf.as_mut_ptr()) };
    if len <= 0 {
        return 0;
    }
    let slice = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), len as usize) };
    std::str::from_utf8(slice).ok().and_then(|s| s.trim().parse::<i32>().ok()).unwrap_or(0)
}

fn dlsym_fn<F: Copy>(symbol: &CStr) -> Option<F> {
    let ptr = unsafe { libc::dlsym(libc::RTLD_DEFAULT, symbol.as_ptr()) };
    if ptr.is_null() {
        return None;
    }
    let addr = ptr as usize;
    Some(unsafe { transmute_copy(&addr) })
}

struct MethodEntry {
    name: &'static str,
    sig: &'static str,
    fn_ptr: usize,
}

#[derive(Default, Copy, Clone)]
struct HwuiExports {
    acanvas_from_java: usize,
}

#[derive(Default)]
struct HwuiSegments {
    found: bool,
    base: usize,
    dyn_addr: usize,
    dyn_size: usize,
    exec: Vec<(usize, usize)>,
    read: Vec<(usize, usize)>,
    write: Vec<(usize, usize)>,
}

#[inline(always)]
fn untag(ptr: usize) -> usize {
    #[cfg(target_pointer_width = "64")]
    {
        ptr & 0x00ff_ffff_ffff_ffff
    }
    #[cfg(not(target_pointer_width = "64"))]
    {
        ptr
    }
}

#[inline(always)]
fn in_ranges(ranges: &[(usize, usize)], addr: usize) -> bool {
    ranges.iter().any(|&(lo, hi)| addr >= lo && addr < hi)
}

fn resolve_dyn_ptr(segs: &HwuiSegments, val: usize) -> Option<usize> {
    let v = untag(val);
    if in_ranges(&segs.read, v) {
        Some(v)
    } else {
        let rel = segs.base.wrapping_add(v);
        in_ranges(&segs.read, rel).then_some(rel)
    }
}

fn read_ro_ascii(ranges: &[(usize, usize)], addr: usize, max_len: usize) -> Option<&'static str> {
    let hi = ranges.iter().find(|&&(lo, hi)| addr >= lo && addr < hi).map(|&(_, hi)| hi)?;
    let limit = (hi - addr).min(max_len + 1);
    if limit < 2 {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(addr as *const u8, limit) };
    let nul = bytes.iter().position(|&b| b == 0)?;
    if nul == 0 {
        return None;
    }
    let slice = &bytes[..nul];
    if !slice.iter().all(|&b| (0x20..=0x7E).contains(&b)) {
        return None;
    }
    std::str::from_utf8(slice).ok()
}

extern "C" fn phdr_callback(
    info: *mut libc::dl_phdr_info,
    _size: libc::size_t,
    data: *mut c_void,
) -> c_int {
    let Some(info) = (unsafe { info.as_ref() }) else { return 0 };
    if info.dlpi_name.is_null() {
        return 0;
    }
    let name_bytes = unsafe { CStr::from_ptr(info.dlpi_name) }.to_bytes();
    if !(name_bytes == b"libhwui.so" || name_bytes.ends_with(b"/libhwui.so")) {
        return 0;
    }
    let segs = unsafe { &mut *(data as *mut HwuiSegments) };
    segs.found = true;
    let base = untag(info.dlpi_addr as usize);
    segs.base = base;
    let phdrs = unsafe { std::slice::from_raw_parts(info.dlpi_phdr, info.dlpi_phnum as usize) };
    const ELF_PF_X: u32 = 1;
    const ELF_PF_W: u32 = 2;
    const ELF_PF_R: u32 = 4;
    for ph in phdrs {
        if ph.p_type == libc::PT_DYNAMIC && ph.p_memsz > 0 {
            segs.dyn_addr = base.wrapping_add(ph.p_vaddr as usize);
            segs.dyn_size = ph.p_memsz as usize;
            continue;
        }
        if ph.p_type != libc::PT_LOAD || ph.p_memsz == 0 {
            continue;
        }
        let start = base.wrapping_add(ph.p_vaddr as usize);
        let mem_end = start.wrapping_add(ph.p_memsz as usize);
        let file_end = start.wrapping_add(ph.p_filesz as usize);
        let flags = ph.p_flags;
        if flags & ELF_PF_X != 0 {
            segs.exec.push((start, mem_end));
        }
        if flags & ELF_PF_R != 0 {
            segs.read.push((start, mem_end));
        }
        if flags & ELF_PF_W != 0 && file_end > start {
            segs.write.push((start, file_end));
        }
    }
    1
}

fn resolve_elf_exports(segs: &HwuiSegments) -> HwuiExports {
    #[repr(C)]
    #[derive(Copy, Clone)]
    struct ElfDyn {
        d_tag: isize,
        d_val: usize,
    }
    #[cfg(target_pointer_width = "64")]
    #[repr(C)]
    #[derive(Copy, Clone)]
    struct ElfSym {
        st_name: u32,
        st_info: u8,
        st_other: u8,
        st_shndx: u16,
        st_value: usize,
        st_size: usize,
    }
    #[cfg(not(target_pointer_width = "64"))]
    #[repr(C)]
    #[derive(Copy, Clone)]
    struct ElfSym {
        st_name: u32,
        st_value: usize,
        st_size: usize,
        st_info: u8,
        st_other: u8,
        st_shndx: u16,
    }

    let mut out = HwuiExports::default();
    if segs.dyn_addr == 0 || segs.dyn_size < std::mem::size_of::<ElfDyn>() {
        return out;
    }
    let count = segs.dyn_size / std::mem::size_of::<ElfDyn>();
    let dyns = unsafe { std::slice::from_raw_parts(segs.dyn_addr as *const ElfDyn, count) };

    let mut symtab_raw = 0usize;
    let mut strtab_raw = 0usize;
    let mut strsz = 0usize;
    let mut gnu_hash_raw = 0usize;
    let mut sysv_hash_raw = 0usize;

    for d in dyns {
        match d.d_tag {
            0 => break,
            4 => sysv_hash_raw = d.d_val,
            5 => strtab_raw = d.d_val,
            6 => symtab_raw = d.d_val,
            10 => strsz = d.d_val,
            t if t as usize == 0x6fff_fef5 => gnu_hash_raw = d.d_val,
            _ => {}
        }
    }

    let (Some(symtab), Some(strtab)) =
        (resolve_dyn_ptr(segs, symtab_raw), resolve_dyn_ptr(segs, strtab_raw))
    else {
        return out;
    };
    if strsz == 0 {
        return out;
    }

    let nsyms = if gnu_hash_raw != 0
        && let Some(gh) = resolve_dyn_ptr(segs, gnu_hash_raw)
    {
        let hdr = unsafe { std::slice::from_raw_parts(gh as *const u32, 4) };
        let nbuckets = hdr[0] as usize;
        let symoffset = hdr[1] as usize;
        let bloom_size = hdr[2] as usize;
        let buckets_addr = gh + 16 + bloom_size * std::mem::size_of::<usize>();
        let buckets = unsafe { std::slice::from_raw_parts(buckets_addr as *const u32, nbuckets) };
        let mut max_sym = buckets.iter().copied().max().unwrap_or(0) as usize;
        if max_sym < symoffset {
            return out;
        }
        let chains_addr = buckets_addr + nbuckets * 4;
        loop {
            let chain_val = unsafe { *((chains_addr + (max_sym - symoffset) * 4) as *const u32) };
            max_sym += 1;
            if chain_val & 1 != 0 {
                break;
            }
        }
        max_sym
    } else if sysv_hash_raw != 0
        && let Some(sh) = resolve_dyn_ptr(segs, sysv_hash_raw)
    {
        unsafe { *((sh + 4) as *const u32) as usize }
    } else {
        0
    };

    if nsyms == 0 {
        return out;
    }

    let syms = unsafe { std::slice::from_raw_parts(symtab as *const ElfSym, nsyms) };
    let str_bytes = unsafe { std::slice::from_raw_parts(strtab as *const u8, strsz) };

    for sym in syms {
        let name_off = sym.st_name as usize;
        if sym.st_value == 0 || name_off >= strsz {
            continue;
        }
        let tail = &str_bytes[name_off..];
        let Some(nul) = tail.iter().position(|&b| b == 0) else { continue };
        let name = &tail[..nul];
        let addr = segs.base.wrapping_add(sym.st_value);
        if !in_ranges(&segs.exec, addr & !1) {
            continue;
        }
        if name == b"ACanvas_getNativeHandleFromJava" {
            out.acanvas_from_java = addr;
            break;
        }
    }
    out
}

fn scan_libhwui() -> Option<(Vec<MethodEntry>, HwuiExports)> {
    let mut segs = HwuiSegments::default();
    unsafe {
        libc::dl_iterate_phdr(
            Some(phdr_callback),
            (&mut segs as *mut HwuiSegments).cast::<c_void>(),
        );
    }
    if !segs.found || segs.exec.is_empty() || segs.read.is_empty() || segs.write.is_empty() {
        return None;
    }
    let exports = resolve_elf_exports(&segs);
    let step = std::mem::size_of::<usize>();
    let triple_bytes = step * 3;
    let mut out = Vec::with_capacity(512);
    for &(w_start, w_end) in &segs.write {
        let aligned = (w_start + step - 1) & !(step - 1);
        if aligned + triple_bytes > w_end {
            continue;
        }
        let mut cursor = aligned;
        while cursor + triple_bytes <= w_end {
            let raw_name = unsafe { *(cursor as *const usize) };
            let raw_sig = unsafe { *((cursor + step) as *const usize) };
            let raw_fn = unsafe { *((cursor + step * 2) as *const usize) };
            let fn_addr = untag(raw_fn);
            let name_addr = untag(raw_name);
            let sig_addr = untag(raw_sig);
            if fn_addr != 0
                && in_ranges(&segs.exec, fn_addr & !1)
                && in_ranges(&segs.read, name_addr)
                && in_ranges(&segs.read, sig_addr)
                && let Some(sig) = read_ro_ascii(&segs.read, sig_addr, 128)
                && sig.starts_with('(')
                && sig.contains(')')
                && let Some(name) = read_ro_ascii(&segs.read, name_addr, 64)
                && name.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
            {
                out.push(MethodEntry { name, sig, fn_ptr: raw_fn });
                cursor += triple_bytes;
                continue;
            }
            cursor += step;
        }
    }
    Some((out, exports))
}

fn window(len: usize, anchor: usize, before: usize, after: usize) -> std::ops::Range<usize> {
    anchor.saturating_sub(before)..(anchor + after + 1).min(len)
}

fn find_anchor(entries: &[MethodEntry], name: &str, sig: &str) -> Option<usize> {
    entries.iter().position(|e| e.name == name && e.sig == sig)
}

fn find_index_in(
    entries: &[MethodEntry],
    range: std::ops::Range<usize>,
    name: &str,
    sig: &str,
) -> Option<usize> {
    let slice = entries.get(range.clone())?;
    slice.iter().position(|e| e.name == name && e.sig == sig).map(|off| range.start + off)
}

fn find_in<F: Copy>(
    entries: &[MethodEntry],
    range: std::ops::Range<usize>,
    name: &str,
    sig: &str,
) -> Option<F> {
    let idx = find_index_in(entries, range, name, sig)?;
    Some(unsafe { transmute_copy(&entries[idx].fn_ptr) })
}
