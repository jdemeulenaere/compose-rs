//! Android native `TextBackend` backed directly by `libhwui.so` (`Minikin` / `HarfBuzz`) and
//! `libicu.so` (`UBiDi`), bypassing `TextMeasurer.kt` completely.

use std::any::Any;
use std::collections::HashMap;
use std::rc::Rc;

use compose_ui::{IntSize, Offset, Rect, platform::ParagraphMetrics};
use jni::Env;
use jni::objects::JObject;
use jni::refs::Global;
use jni::sys::{JNIEnv, jcharArray, jfloatArray, jint, jobject, jvalue};

use crate::bridge::with_main_env;
use crate::hwui::{
    BuildMeasuredTextFn, GetFontMetricsIntFn, HwuiApi, LineBreakerInitFn, PaintGetRunCharAdvanceFn,
    TextHwuiApi, UBiDi, UBiDiLevel, UErrorCode, clear_exception_check,
};
use crate::text::{LaidOut, SharedParagraphs, TextBackend};
use crate::text_layout::{
    DIR_LEFT_TO_RIGHT, DIR_RIGHT_TO_LEFT, FontMetricsInt, LineBreakItem, LineDirections,
    MeasuredTextHandle, NativeLayoutEngine, PaintStyleKey, ParagraphBidi, RUN_LENGTH_MASK,
    RUN_LEVEL_MASK, RUN_LEVEL_SHIFT, RUN_RTL_FLAG, StrokeMetrics, StyleRunSpec, TextShaper,
};

const PAINT_CACHE_CAP: usize = 512;
const EXACT_CHAR_ARRAY_CAP: usize = 64;
const RUN_EDGE_FLAGS_MASK: i32 = 0x6000;
const TAB_INCREMENT: f32 = 20.0;
const ASCII_RUN_CACHE_BITS: usize = 9;
const ASCII_RUN_CACHE_SIZE: usize = 1 << ASCII_RUN_CACHE_BITS;
const ASCII_RUN_CACHE_MASK: usize = ASCII_RUN_CACHE_SIZE - 1;
const ASCII_RUN_INLINE_CAP: usize = 24;

/// Printable ASCII characters eligible for the verified kerning-free fast path
/// (`'0'..='9'`, `'.'`, `'+'`, `'-'`, `'%'`, `'$'`, `','`, `'/'`, `':'`, `' '`).
const FAST_NUMERIC_CHARS: [u8; 19] = *b"0123456789.+-%$,/: ";

/// Adjacent pairs tested against Minikin to prove zero pair kerning before enabling the table.
const FAST_NUMERIC_KERNING_PAIRS: &[u8; 37] = b"00112233445566778899.0+0-00%$1,0:0/0 ";

const fn build_fast_numeric_lut() -> [bool; 128] {
    let mut lut = [false; 128];
    let mut i = 0;
    while i < FAST_NUMERIC_CHARS.len() {
        lut[FAST_NUMERIC_CHARS[i] as usize] = true;
        i += 1;
    }
    lut
}

const FAST_NUMERIC_LUT: [bool; 128] = build_fast_numeric_lut();

#[inline]
fn is_fast_numeric_ascii(b: u8) -> bool {
    (b as usize) < 128 && FAST_NUMERIC_LUT[b as usize]
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct ShapingStyleKey {
    font_px_bits: u32,
    weight: i32,
    italic: bool,
    family: u8,
    family_name: Option<Rc<str>>,
    letter_spacing_px_bits: u32,
}

impl ShapingStyleKey {
    fn from_paint_style(style: &PaintStyleKey) -> Self {
        Self {
            font_px_bits: style.font_px_bits,
            weight: style.weight,
            italic: style.italic,
            family: style.family,
            family_name: style.family_name.clone(),
            letter_spacing_px_bits: style.letter_spacing_px_bits,
        }
    }
}

struct ShapingMetrics {
    id: u32,
    fmi: FontMetricsInt,
    ascii_fmi: FontMetricsInt,
    stroke: StrokeMetrics,
    fast_numeric_valid: bool,
    fast_ascii_advances: [f32; 128],
}

#[derive(Clone, Copy)]
struct AsciiRunCacheEntry {
    shaping_id: u32,
    len: u8,
    bytes: [u8; ASCII_RUN_INLINE_CAP],
    width: f32,
}

impl Default for AsciiRunCacheEntry {
    fn default() -> Self {
        Self { shaping_id: 0, len: 0, bytes: [0; ASCII_RUN_INLINE_CAP], width: 0.0 }
    }
}

#[inline]
fn hash_ascii_run(shaping_id: u32, chars: &[u16]) -> usize {
    let mut h = (shaping_id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    for &c in chars {
        h ^= c as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    (h as usize) ^ ((h >> 32) as usize)
}

struct OwnedNativePaint {
    ptr: i64,
    destroy: unsafe extern "C" fn(i64),
    _typeface: Rc<(Global<JObject<'static>>, i64)>,
}

impl Drop for OwnedNativePaint {
    fn drop(&mut self) {
        if self.ptr != 0 {
            unsafe { (self.destroy)(self.ptr) };
        }
    }
}

#[derive(Clone)]
struct CachedNativePaint {
    ptr: i64,
    owner: Rc<dyn Any>,
    shaping: Rc<ShapingMetrics>,
    base_flags: i32,
}

struct MinikinMeasuredTextOwner {
    ptr: i64,
    release: unsafe extern "C" fn(i64),
}

impl Drop for MinikinMeasuredTextOwner {
    fn drop(&mut self) {
        if self.ptr != 0 {
            unsafe { (self.release)(self.ptr) };
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct TypefaceKey {
    family: u8,
    weight: i32,
    italic: bool,
    family_name: Option<Rc<str>>,
}

pub(crate) struct MinikinShaper {
    raw_env: *mut JNIEnv,
    hwui: &'static HwuiApi,
    text: &'static TextHwuiApi,
    paints: HashMap<PaintStyleKey, CachedNativePaint>,
    paints_by_ptr: HashMap<i64, CachedNativePaint>,
    shaping_cache: HashMap<ShapingStyleKey, Rc<ShapingMetrics>>,
    next_shaping_id: u32,
    ascii_run_cache: Box<[AsciiRunCacheEntry; ASCII_RUN_CACHE_SIZE]>,
    typefaces: HashMap<TypefaceKey, Rc<(Global<JObject<'static>>, i64)>>,
    shared_char_array: Global<JObject<'static>>,
    shared_char_cap: usize,
    exact_char_arrays: HashMap<usize, Global<JObject<'static>>>,
    advances_array: Global<JObject<'static>>,
    advances_cap: usize,
    fmi_obj: Global<JObject<'static>>,
    lb_normal: i64,
    lb_justified: i64,
    para_bidi: *mut UBiDi,
    line_bidi: *mut UBiDi,
    bidi_scratch: Vec<u16>,
}

impl MinikinShaper {
    pub fn try_new(env: &mut Env) -> Option<Self> {
        let hwui = crate::hwui::get(env)?;
        let text = hwui.text.as_ref()?;
        let raw = env.get_raw();

        let shared_raw = unsafe { ((**raw).v1_2.NewCharArray)(raw, 256) };
        if shared_raw.is_null() || clear_exception_check(raw) {
            return None;
        }
        let shared_local = unsafe { JObject::from_raw(env, shared_raw as jobject) };
        let shared_char_array = env.new_global_ref(&shared_local).ok()?;
        env.delete_local_ref(shared_local);

        let adv_raw = unsafe { ((**raw).v1_2.NewFloatArray)(raw, 128) };
        if adv_raw.is_null() || clear_exception_check(raw) {
            return None;
        }
        let adv_local = unsafe { JObject::from_raw(env, adv_raw as jobject) };
        let advances_array = env.new_global_ref(&adv_local).ok()?;
        env.delete_local_ref(adv_local);

        let fmi_raw =
            unsafe { ((**raw).v1_2.NewObject)(raw, text.fmi_class.as_raw(), text.fmi_ctor) };
        if fmi_raw.is_null() || clear_exception_check(raw) {
            return None;
        }
        let fmi_local = unsafe { JObject::from_raw(env, fmi_raw) };
        let fmi_obj = env.new_global_ref(&fmi_local).ok()?;
        env.delete_local_ref(fmi_local);

        let lb_normal = init_line_breaker(raw, text, false);
        let lb_justified = init_line_breaker(raw, text, true);
        if lb_normal == 0 || lb_justified == 0 {
            if lb_normal != 0 {
                unsafe { (text.lb_destroy)(lb_normal) };
            }
            if lb_justified != 0 {
                unsafe { (text.lb_destroy)(lb_justified) };
            }
            return None;
        }

        let para_bidi = unsafe { (text.icu.open)() };
        let line_bidi = unsafe { (text.icu.open)() };
        if para_bidi.is_null() || line_bidi.is_null() {
            if !para_bidi.is_null() {
                unsafe { (text.icu.close)(para_bidi) };
            }
            if !line_bidi.is_null() {
                unsafe { (text.icu.close)(line_bidi) };
            }
            unsafe {
                (text.lb_destroy)(lb_normal);
                (text.lb_destroy)(lb_justified);
            }
            return None;
        }

        Some(Self {
            raw_env: raw,
            hwui,
            text,
            paints: HashMap::with_capacity(64),
            paints_by_ptr: HashMap::with_capacity(64),
            shaping_cache: HashMap::with_capacity(32),
            next_shaping_id: 1,
            ascii_run_cache: vec![AsciiRunCacheEntry::default(); ASCII_RUN_CACHE_SIZE]
                .into_boxed_slice()
                .try_into()
                .unwrap_or_else(|_| unreachable!()),
            typefaces: HashMap::with_capacity(16),
            shared_char_array,
            shared_char_cap: 256,
            exact_char_arrays: HashMap::with_capacity(16),
            advances_array,
            advances_cap: 128,
            fmi_obj,
            lb_normal,
            lb_justified,
            para_bidi,
            line_bidi,
            bidi_scratch: Vec::new(),
        })
    }

    fn resolve_typeface(
        &mut self,
        env: &mut Env,
        style: &PaintStyleKey,
    ) -> Rc<(Global<JObject<'static>>, i64)> {
        let key = TypefaceKey {
            family: style.family,
            weight: style.weight.clamp(1, 1000),
            italic: style.italic,
            family_name: if style.family == 4 { style.family_name.clone() } else { None },
        };
        if let Some(hit) = self.typefaces.get(&key) {
            return hit.clone();
        }
        let raw = env.get_raw();
        let text = self.text;

        let mut named_local: jobject = std::ptr::null_mut();
        let base_raw: jobject = match key.family {
            1 => text.tf_serif.as_raw(),
            2 => text.tf_monospace.as_raw(),
            3 => text.tf_cursive.as_raw(),
            4 => {
                if let Some(ref name) = key.family_name
                    && let Ok(jstr) = env.new_string(name.as_ref())
                {
                    let args = [jvalue { l: jstr.as_raw() }, jvalue { i: 0 }];
                    let created = unsafe {
                        ((**raw).v1_2.CallStaticObjectMethodA)(
                            raw,
                            text.typeface_class.as_raw(),
                            text.typeface_create_name,
                            args.as_ptr(),
                        )
                    };
                    env.delete_local_ref(jstr);
                    if !clear_exception_check(raw) && !created.is_null() {
                        named_local = created;
                        created
                    } else {
                        text.tf_default.as_raw()
                    }
                } else {
                    text.tf_default.as_raw()
                }
            }
            _ => text.tf_default.as_raw(),
        };

        let weight_args =
            [jvalue { l: base_raw }, jvalue { i: key.weight }, jvalue { z: key.italic }];
        let tf_raw = unsafe {
            ((**raw).v1_2.CallStaticObjectMethodA)(
                raw,
                text.typeface_class.as_raw(),
                text.typeface_create_weight,
                weight_args.as_ptr(),
            )
        };
        if !named_local.is_null() {
            unsafe { ((**raw).v1_2.DeleteLocalRef)(raw, named_local) };
        }
        let final_global = if !clear_exception_check(raw) && !tf_raw.is_null() {
            let local = unsafe { JObject::from_raw(env, tf_raw) };
            let global = env.new_global_ref(&local).ok();
            env.delete_local_ref(local);
            global
        } else {
            None
        }
        .unwrap_or_else(|| {
            env.new_global_ref(&*text.tf_default).unwrap_or_else(|_| unreachable!())
        });

        let tf_ptr = unsafe {
            ((**raw).v1_2.GetLongField)(raw, final_global.as_raw(), text.typeface_native_field)
        };
        let rc = Rc::new((final_global, tf_ptr));
        if self.typefaces.len() >= 128 {
            self.typefaces.clear();
        }
        self.typefaces.insert(key, rc.clone());
        rc
    }

    #[inline]
    fn upload_shared_chars_raw(&mut self, chars: &[u16]) -> jcharArray {
        if chars.len() > self.shared_char_cap {
            return with_main_env(|env| self.upload_shared_chars(env, chars));
        }
        let raw = self.raw_env;
        let copy_len = chars.len();
        let jarr = self.shared_char_array.as_raw() as jcharArray;
        if copy_len > 0 {
            unsafe {
                ((**raw).v1_2.SetCharArrayRegion)(raw, jarr, 0, copy_len as jint, chars.as_ptr());
            }
        }
        jarr
    }

    fn upload_shared_chars(&mut self, env: &mut Env, chars: &[u16]) -> jcharArray {
        let raw = env.get_raw();
        if chars.len() > self.shared_char_cap {
            let new_cap = chars.len().next_power_of_two().max(256);
            let arr = unsafe { ((**raw).v1_2.NewCharArray)(raw, new_cap as jint) };
            if !arr.is_null() && !clear_exception_check(raw) {
                let local = unsafe { JObject::from_raw(env, arr as jobject) };
                if let Ok(global) = env.new_global_ref(&local) {
                    self.shared_char_array = global;
                    self.shared_char_cap = new_cap;
                }
                env.delete_local_ref(local);
            }
        }
        let copy_len = chars.len().min(self.shared_char_cap);
        let jarr = self.shared_char_array.as_raw() as jcharArray;
        if copy_len > 0 {
            unsafe {
                ((**raw).v1_2.SetCharArrayRegion)(raw, jarr, 0, copy_len as jint, chars.as_ptr());
            }
        }
        jarr
    }

    #[inline]
    fn upload_exact_chars_raw(&mut self, chars: &[u16]) -> jcharArray {
        let len = chars.len();
        if let Some(global) = self.exact_char_arrays.get(&len) {
            let raw = self.raw_env;
            let jarr = global.as_raw() as jcharArray;
            if len > 0 {
                unsafe {
                    ((**raw).v1_2.SetCharArrayRegion)(raw, jarr, 0, len as jint, chars.as_ptr());
                }
            }
            return jarr;
        }
        with_main_env(|env| self.upload_exact_chars(env, chars))
    }

    fn upload_exact_chars(&mut self, env: &mut Env, chars: &[u16]) -> jcharArray {
        let raw = env.get_raw();
        let len = chars.len();
        if !self.exact_char_arrays.contains_key(&len) {
            if self.exact_char_arrays.len() >= EXACT_CHAR_ARRAY_CAP {
                self.exact_char_arrays.clear();
            }
            let arr = unsafe { ((**raw).v1_2.NewCharArray)(raw, len as jint) };
            if arr.is_null() || clear_exception_check(raw) {
                return std::ptr::null_mut();
            }
            let local = unsafe { JObject::from_raw(env, arr as jobject) };
            let Ok(global) = env.new_global_ref(&local) else {
                env.delete_local_ref(local);
                return std::ptr::null_mut();
            };
            env.delete_local_ref(local);
            self.exact_char_arrays.insert(len, global);
        }
        let Some(global) = self.exact_char_arrays.get(&len) else {
            return std::ptr::null_mut();
        };
        let jarr = global.as_raw() as jcharArray;
        if len > 0 {
            unsafe {
                ((**raw).v1_2.SetCharArrayRegion)(raw, jarr, 0, len as jint, chars.as_ptr());
            }
        }
        jarr
    }

    #[inline]
    fn ensure_advances_array_raw(&mut self, needed: usize) -> jfloatArray {
        if needed <= self.advances_cap {
            return self.advances_array.as_raw() as jfloatArray;
        }
        with_main_env(|env| self.ensure_advances_array(env, needed))
    }

    fn ensure_advances_array(&mut self, env: &mut Env, needed: usize) -> jfloatArray {
        let raw = env.get_raw();
        if needed > self.advances_cap {
            let new_cap = needed.next_power_of_two().max(128);
            let arr = unsafe { ((**raw).v1_2.NewFloatArray)(raw, new_cap as jint) };
            if !arr.is_null() && !clear_exception_check(raw) {
                let local = unsafe { JObject::from_raw(env, arr as jobject) };
                if let Ok(global) = env.new_global_ref(&local) {
                    self.advances_array = global;
                    self.advances_cap = new_cap;
                }
                env.delete_local_ref(local);
            }
        }
        if needed <= self.advances_cap {
            self.advances_array.as_raw() as jfloatArray
        } else {
            std::ptr::null_mut()
        }
    }

    #[inline]
    fn read_fmi_fields(&self, raw: *mut JNIEnv) -> FontMetricsInt {
        let obj = self.fmi_obj.as_raw();
        let get_int = unsafe { (**raw).v1_2.GetIntField };
        unsafe {
            FontMetricsInt {
                top: get_int(raw, obj, self.text.fmi_top),
                ascent: get_int(raw, obj, self.text.fmi_ascent),
                descent: get_int(raw, obj, self.text.fmi_descent),
                bottom: get_int(raw, obj, self.text.fmi_bottom),
                leading: get_int(raw, obj, self.text.fmi_leading),
            }
        }
    }

    #[inline]
    fn base_flags_of(&self, paint_ptr: i64) -> i32 {
        self.paints_by_ptr
            .get(&paint_ptr)
            .map_or_else(|| unsafe { (self.text.paint_get_flags)(paint_ptr) }, |c| c.base_flags)
    }

    fn build_shaping_metrics(&mut self, raw: *mut JNIEnv, ptr: i64) -> Rc<ShapingMetrics> {
        let id = self.next_shaping_id;
        self.next_shaping_id = self.next_shaping_id.wrapping_add(1).max(1);

        unsafe {
            match self.text.paint_get_fmi {
                GetFontMetricsIntFn::V29(f) => {
                    let _ = f(raw, std::ptr::null_mut(), ptr, self.fmi_obj.as_raw());
                }
                GetFontMetricsIntFn::V35(f) => {
                    let _ = f(raw, std::ptr::null_mut(), ptr, self.fmi_obj.as_raw(), 0);
                }
            }
        }
        let fmi = self.read_fmi_fields(raw);
        let stroke = unsafe {
            StrokeMetrics {
                underline_position: (self.text.paint_get_underline_position)(ptr),
                underline_thickness: (self.text.paint_get_underline_thickness)(ptr),
                strikethru_position: (self.text.paint_get_strikethru_position)(ptr),
                strikethru_thickness: (self.text.paint_get_strikethru_thickness)(ptr),
            }
        };

        // Probe buffer:
        // [0..38): 19 numeric/symbol characters separated by space (`[c, ' ']`).
        // [38..75): 37-char kerning verification string (`FAST_NUMERIC_KERNING_PAIRS`).
        const PROBE_SEP_LEN: usize = FAST_NUMERIC_CHARS.len() * 2;
        const PROBE_TOTAL_LEN: usize = PROBE_SEP_LEN + FAST_NUMERIC_KERNING_PAIRS.len();
        let mut probe_chars = [0u16; PROBE_TOTAL_LEN];
        for (i, &b) in FAST_NUMERIC_CHARS.iter().enumerate() {
            probe_chars[i * 2] = u16::from(b);
            probe_chars[i * 2 + 1] = u16::from(b' ');
        }
        for (i, &b) in FAST_NUMERIC_KERNING_PAIRS.iter().enumerate() {
            probe_chars[PROBE_SEP_LEN + i] = u16::from(b);
        }

        let jarr = self.upload_shared_chars_raw(&probe_chars);
        unsafe {
            (self.text.paint_get_fmi_for_text)(
                raw,
                std::ptr::null_mut(),
                ptr,
                jarr,
                0,
                PROBE_SEP_LEN as i32,
                0,
                PROBE_SEP_LEN as i32,
                0,
                self.fmi_obj.as_raw(),
            );
        }
        let ext = self.read_fmi_fields(raw);
        let asc = fmi.ascent.min(ext.ascent);
        let desc = fmi.descent.max(ext.descent);
        let ascii_fmi = FontMetricsInt {
            top: fmi.top.min(asc),
            ascent: asc,
            descent: desc,
            bottom: fmi.bottom.max(desc),
            leading: fmi.leading,
        };

        let mut fast_ascii_advances = [0.0f32; 128];
        let mut fast_numeric_valid = false;
        let adv_jarr = self.ensure_advances_array_raw(PROBE_TOTAL_LEN);
        if !adv_jarr.is_null() {
            let mut probe_adv = [0.0f32; PROBE_TOTAL_LEN];
            unsafe {
                match self.text.paint_get_run_char_advance {
                    PaintGetRunCharAdvanceFn::V33(f) => {
                        let _ = f(
                            raw,
                            std::ptr::null_mut(),
                            ptr,
                            jarr,
                            0,
                            PROBE_TOTAL_LEN as i32,
                            0,
                            PROBE_TOTAL_LEN as i32,
                            0,
                            PROBE_TOTAL_LEN as i32,
                            adv_jarr,
                            0,
                        );
                    }
                    PaintGetRunCharAdvanceFn::V34(f) => {
                        let _ = f(
                            raw,
                            std::ptr::null_mut(),
                            ptr,
                            jarr,
                            0,
                            PROBE_TOTAL_LEN as i32,
                            0,
                            PROBE_TOTAL_LEN as i32,
                            0,
                            PROBE_TOTAL_LEN as i32,
                            adv_jarr,
                            0,
                            std::ptr::null_mut(),
                            std::ptr::null_mut(),
                        );
                    }
                }
                ((**raw).v1_2.GetFloatArrayRegion)(
                    raw,
                    adv_jarr,
                    0,
                    PROBE_TOTAL_LEN as jint,
                    probe_adv.as_mut_ptr(),
                );
            }
            for (i, &b) in FAST_NUMERIC_CHARS.iter().enumerate() {
                fast_ascii_advances[b as usize] = probe_adv[i * 2];
            }
            // Verify that every character in `FAST_NUMERIC_KERNING_PAIRS` has bit-identical
            // advance when adjacent to digits/symbols vs isolated by spaces.
            fast_numeric_valid = true;
            for (i, &b) in FAST_NUMERIC_KERNING_PAIRS.iter().enumerate() {
                let pair_adv = probe_adv[PROBE_SEP_LEN + i];
                let iso_adv = fast_ascii_advances[b as usize];
                if pair_adv.to_bits() != iso_adv.to_bits() {
                    fast_numeric_valid = false;
                    break;
                }
            }
        }
        let _ = clear_exception_check(raw);

        Rc::new(ShapingMetrics {
            id,
            fmi,
            ascii_fmi,
            stroke,
            fast_numeric_valid,
            fast_ascii_advances,
        })
    }

    fn set_bidi_para(&mut self, chars: &[u16], para_level: UBiDiLevel) -> UErrorCode {
        let icu = &self.text.icu;
        let mut status: UErrorCode = 0;
        unsafe {
            (icu.set_para)(
                self.para_bidi,
                chars.as_ptr(),
                chars.len() as i32,
                para_level,
                std::ptr::null_mut(),
                &mut status,
            );
        }
        if status <= 0 && !chars.is_empty() && unsafe { (icu.count_paragraphs)(self.para_bidi) } > 1
        {
            self.bidi_scratch.clear();
            self.bidi_scratch.extend(chars.iter().map(|&ch| {
                if ch == 0x000D || (0x001C..=0x001E).contains(&ch) || ch == 0x0085 || ch == 0x2029 {
                    0xFFFC
                } else {
                    ch
                }
            }));
            status = 0;
            unsafe {
                (icu.set_para)(
                    self.para_bidi,
                    self.bidi_scratch.as_ptr(),
                    self.bidi_scratch.len() as i32,
                    para_level,
                    std::ptr::null_mut(),
                    &mut status,
                );
            }
        }
        status
    }
}

impl Drop for MinikinShaper {
    fn drop(&mut self) {
        if self.lb_normal != 0 {
            unsafe { (self.text.lb_destroy)(self.lb_normal) };
            self.lb_normal = 0;
        }
        if self.lb_justified != 0 {
            unsafe { (self.text.lb_destroy)(self.lb_justified) };
            self.lb_justified = 0;
        }
        if !self.para_bidi.is_null() {
            unsafe { (self.text.icu.close)(self.para_bidi) };
            self.para_bidi = std::ptr::null_mut();
        }
        if !self.line_bidi.is_null() {
            unsafe { (self.text.icu.close)(self.line_bidi) };
            self.line_bidi = std::ptr::null_mut();
        }
    }
}

impl TextShaper for MinikinShaper {
    fn resolve_paint(&mut self, style: &PaintStyleKey) -> i64 {
        if let Some(hit) = self.paints.get(style) {
            return hit.ptr;
        }
        with_main_env(|env| {
            if self.paints.len() >= PAINT_CACHE_CAP {
                self.paints.clear();
                self.paints_by_ptr.clear();
            }
            let tf = self.resolve_typeface(env, style);
            let raw = env.get_raw();
            self.raw_env = raw;
            let ptr = unsafe {
                (self.text.paint_init_with_paint)(
                    raw,
                    std::ptr::null_mut(),
                    self.text.template_paint_ptr,
                )
            };
            if ptr == 0 {
                return 0;
            }
            let font_px = style.font_px();
            let em_letter_spacing =
                if font_px > 0.0 { style.letter_spacing_px() / font_px } else { 0.0 };
            unsafe {
                (self.text.paint_set_typeface)(ptr, tf.1);
                (self.hwui.paint_set_color)(ptr, style.color as i32);
                (self.text.paint_set_text_size)(ptr, font_px);
                (self.text.paint_set_letter_spacing)(ptr, em_letter_spacing);
                (self.text.paint_set_word_spacing)(ptr, 0.0);
                (self.text.paint_set_underline)(ptr, 0);
                (self.text.paint_set_strikethru)(ptr, 0);
            }
            // Keep `RUN_EDGE_FLAGS_MASK` (`TEXT_RUN_FLAG_LEFT_EDGE | TEXT_RUN_FLAG_RIGHT_EDGE`)
            // enabled by default on cached paints so whole-run measurement and drawing never
            // need to toggle `paint_set_flags` before and after.
            let base_flags = unsafe { (self.text.paint_get_flags)(ptr) } | RUN_EDGE_FLAGS_MASK;
            unsafe { (self.hwui.paint_set_flags)(ptr, base_flags) };

            let shaping_key = ShapingStyleKey::from_paint_style(style);
            let shaping = if let Some(cached) = self.shaping_cache.get(&shaping_key) {
                cached.clone()
            } else {
                if self.shaping_cache.len() >= PAINT_CACHE_CAP {
                    self.shaping_cache.clear();
                }
                let built = self.build_shaping_metrics(raw, ptr);
                self.shaping_cache.insert(shaping_key, built.clone());
                built
            };

            let owner: Rc<dyn Any> =
                Rc::new(OwnedNativePaint { ptr, destroy: self.hwui.paint_destroy, _typeface: tf });
            let entry = CachedNativePaint { ptr, owner, shaping, base_flags };
            self.paints.insert(style.clone(), entry.clone());
            self.paints_by_ptr.insert(ptr, entry);
            ptr
        })
    }

    fn paint_owner(&self, paint_ptr: i64) -> Option<Rc<dyn Any>> {
        self.paints_by_ptr.get(&paint_ptr).map(|c| c.owner.clone())
    }

    fn font_metrics_int(&mut self, paint_ptr: i64) -> FontMetricsInt {
        if let Some(hit) = self.paints_by_ptr.get(&paint_ptr) {
            return hit.shaping.fmi;
        }
        let raw = self.raw_env;
        unsafe {
            match self.text.paint_get_fmi {
                GetFontMetricsIntFn::V29(f) => {
                    let _ = f(raw, std::ptr::null_mut(), paint_ptr, self.fmi_obj.as_raw());
                }
                GetFontMetricsIntFn::V35(f) => {
                    let _ = f(raw, std::ptr::null_mut(), paint_ptr, self.fmi_obj.as_raw(), 0);
                }
            }
        }
        self.read_fmi_fields(raw)
    }

    fn stroke_metrics(&mut self, paint_ptr: i64) -> StrokeMetrics {
        if let Some(hit) = self.paints_by_ptr.get(&paint_ptr) {
            return hit.shaping.stroke;
        }
        unsafe {
            StrokeMetrics {
                underline_position: (self.text.paint_get_underline_position)(paint_ptr),
                underline_thickness: (self.text.paint_get_underline_thickness)(paint_ptr),
                strikethru_position: (self.text.paint_get_strikethru_position)(paint_ptr),
                strikethru_thickness: (self.text.paint_get_strikethru_thickness)(paint_ptr),
            }
        }
    }

    fn font_metrics_int_for_text(
        &mut self,
        paint_ptr: i64,
        chars: &[u16],
        start: usize,
        end: usize,
        ctx_start: usize,
        ctx_end: usize,
        is_rtl: bool,
    ) -> FontMetricsInt {
        if start >= end || ctx_start > start || end > ctx_end || ctx_end > chars.len() {
            return self.font_metrics_int(paint_ptr);
        }
        if !is_rtl
            && chars[ctx_start..ctx_end].iter().all(|&c| (0x0020..=0x007E).contains(&c))
            && let Some(hit) = self.paints_by_ptr.get(&paint_ptr)
        {
            return hit.shaping.ascii_fmi;
        }
        let jarr = self.upload_shared_chars_raw(&chars[..ctx_end]);
        let raw = self.raw_env;
        unsafe {
            (self.text.paint_get_fmi_for_text)(
                raw,
                std::ptr::null_mut(),
                paint_ptr,
                jarr,
                start as i32,
                (end - start) as i32,
                ctx_start as i32,
                (ctx_end - ctx_start) as i32,
                u8::from(is_rtl),
                self.fmi_obj.as_raw(),
            );
        }
        self.read_fmi_fields(raw)
    }

    fn measure_boring_run(
        &mut self,
        paint_ptr: i64,
        chars: &[u16],
        start: usize,
        end: usize,
        run_flags: i32,
    ) -> (f32, FontMetricsInt) {
        if start >= end || end > chars.len() {
            return (0.0, self.font_metrics_int(paint_ptr));
        }
        let slice = &chars[start..end];
        let full_edges = (run_flags & RUN_EDGE_FLAGS_MASK) == RUN_EDGE_FLAGS_MASK;

        if full_edges && let Some(hit) = self.paints_by_ptr.get(&paint_ptr) {
            let shaping = &hit.shaping;
            let mut all_ascii = true;
            let mut all_fast_numeric = shaping.fast_numeric_valid;
            for &c in slice {
                if !(0x0020..=0x007E).contains(&c) {
                    all_ascii = false;
                    all_fast_numeric = false;
                    break;
                }
                if all_fast_numeric && !is_fast_numeric_ascii(c as u8) {
                    all_fast_numeric = false;
                }
            }

            if all_fast_numeric {
                let mut w = 0.0f32;
                for &c in slice {
                    w += shaping.fast_ascii_advances[c as usize];
                }
                return (w, shaping.ascii_fmi);
            }

            if all_ascii {
                let ascii_fmi = shaping.ascii_fmi;
                let shaping_id = shaping.id;
                let slen = slice.len();
                let mut cache_slot = 0usize;
                if slen <= ASCII_RUN_INLINE_CAP {
                    cache_slot = hash_ascii_run(shaping_id, slice) & ASCII_RUN_CACHE_MASK;
                    let entry = &self.ascii_run_cache[cache_slot];
                    if entry.shaping_id == shaping_id
                        && entry.len as usize == slen
                        && entry.bytes[..slen].iter().zip(slice).all(|(&b, &c)| u16::from(b) == c)
                    {
                        return (entry.width, ascii_fmi);
                    }
                }

                let jarr = self.upload_shared_chars_raw(slice);
                let raw = self.raw_env;
                let len = slen as i32;
                let width = unsafe {
                    (self.text.paint_get_run_advance)(
                        raw,
                        std::ptr::null_mut(),
                        paint_ptr,
                        jarr,
                        0,
                        len,
                        0,
                        len,
                        0,
                        len,
                    )
                };
                if slen <= ASCII_RUN_INLINE_CAP {
                    let entry = &mut self.ascii_run_cache[cache_slot];
                    entry.shaping_id = shaping_id;
                    entry.len = slen as u8;
                    for (dst, &c) in entry.bytes[..slen].iter_mut().zip(slice) {
                        *dst = c as u8;
                    }
                    entry.width = width;
                }
                return (width, ascii_fmi);
            }
        }

        let base_fmi = self.font_metrics_int(paint_ptr);
        let base_flags = self.base_flags_of(paint_ptr);
        let wanted_flags = (base_flags & !RUN_EDGE_FLAGS_MASK) | (run_flags & RUN_EDGE_FLAGS_MASK);
        let len = slice.len() as i32;

        let jarr = self.upload_shared_chars_raw(slice);
        let raw = self.raw_env;
        if wanted_flags != base_flags {
            unsafe { (self.hwui.paint_set_flags)(paint_ptr, wanted_flags) };
        }
        unsafe {
            (self.text.paint_get_fmi_for_text)(
                raw,
                std::ptr::null_mut(),
                paint_ptr,
                jarr,
                0,
                len,
                0,
                len,
                0,
                self.fmi_obj.as_raw(),
            );
        }
        let ext = self.read_fmi_fields(raw);
        let width = unsafe {
            (self.text.paint_get_run_advance)(
                raw,
                std::ptr::null_mut(),
                paint_ptr,
                jarr,
                0,
                len,
                0,
                len,
                0,
                len,
            )
        };
        if wanted_flags != base_flags {
            unsafe { (self.hwui.paint_set_flags)(paint_ptr, base_flags) };
        }
        let asc = base_fmi.ascent.min(ext.ascent);
        let desc = base_fmi.descent.max(ext.descent);
        let fmi = FontMetricsInt {
            top: base_fmi.top.min(asc),
            ascent: asc,
            descent: desc,
            bottom: base_fmi.bottom.max(desc),
            leading: base_fmi.leading,
        };
        (width, fmi)
    }

    fn analyze_bidi(&mut self, chars: &[u16], rtl: bool) -> ParagraphBidi {
        let para_level: UBiDiLevel = u8::from(rtl);
        if chars.is_empty() {
            return ParagraphBidi {
                paragraph_dir: if rtl { DIR_RIGHT_TO_LEFT } else { DIR_LEFT_TO_RIGHT },
                levels: Vec::new(),
                rtl_request: rtl,
            };
        }
        let status = self.set_bidi_para(chars, para_level);
        if status > 0 {
            return ParagraphBidi {
                paragraph_dir: if rtl { DIR_RIGHT_TO_LEFT } else { DIR_LEFT_TO_RIGHT },
                levels: vec![para_level; chars.len()],
                rtl_request: rtl,
            };
        }
        let icu = &self.text.icu;
        let resolved = unsafe { (icu.get_para_level)(self.para_bidi) };
        let paragraph_dir = if (resolved & 1) != 0 { DIR_RIGHT_TO_LEFT } else { DIR_LEFT_TO_RIGHT };
        let mut lvl_status: UErrorCode = 0;
        let levels_ptr = unsafe { (icu.get_levels)(self.para_bidi, &mut lvl_status) };
        let levels = if lvl_status <= 0 && !levels_ptr.is_null() {
            unsafe { std::slice::from_raw_parts(levels_ptr, chars.len()).to_vec() }
        } else {
            vec![resolved; chars.len()]
        };
        ParagraphBidi { paragraph_dir, levels, rtl_request: rtl }
    }

    fn line_directions(
        &mut self,
        bidi: &ParagraphBidi,
        chars: &[u16],
        start: usize,
        end: usize,
    ) -> LineDirections {
        if bidi.levels.is_empty() {
            return LineDirections::Single([0, RUN_LENGTH_MASK]);
        }
        if start >= end || end > chars.len() {
            let base_flag = if bidi.paragraph_dir == DIR_RIGHT_TO_LEFT { RUN_RTL_FLAG } else { 0 };
            return LineDirections::Single([0, RUN_LENGTH_MASK | base_flag]);
        }

        let para_level: UBiDiLevel = u8::from(bidi.rtl_request);
        let status = self.set_bidi_para(chars, para_level);
        let icu = &self.text.icu;
        let mut line_status: UErrorCode = status;
        if line_status <= 0 {
            unsafe {
                (icu.set_line)(
                    self.para_bidi,
                    start as i32,
                    end as i32,
                    self.line_bidi,
                    &mut line_status,
                );
            }
        }
        let run_count = if line_status <= 0 {
            unsafe { (icu.count_runs)(self.line_bidi, &mut line_status) }
        } else {
            0
        };
        if line_status > 0 || run_count <= 0 {
            let base_flag = if bidi.paragraph_dir == DIR_RIGHT_TO_LEFT { RUN_RTL_FLAG } else { 0 };
            return LineDirections::Single([
                0,
                ((end - start) as i32 & RUN_LENGTH_MASK) | base_flag,
            ]);
        }
        if run_count == 1 {
            let mut limit: i32 = 0;
            let mut level: UBiDiLevel = 0;
            unsafe { (icu.get_logical_run)(self.line_bidi, 0, &mut limit, &mut level) };
            let packed = ((i32::from(level) & RUN_LEVEL_MASK) << RUN_LEVEL_SHIFT)
                | ((end - start) as i32 & RUN_LENGTH_MASK);
            return LineDirections::Single([0, packed]);
        }

        let line_len = (end - start) as i32;
        let cap = run_count as usize;
        let mut run_starts = Vec::with_capacity(cap);
        let mut run_limits = Vec::with_capacity(cap);
        let mut run_levels = Vec::with_capacity(cap);
        let mut pos = 0i32;
        while pos < line_len {
            let mut limit: i32 = 0;
            let mut level: UBiDiLevel = 0;
            unsafe { (icu.get_logical_run)(self.line_bidi, pos, &mut limit, &mut level) };
            if limit <= pos {
                break;
            }
            run_starts.push(pos);
            run_limits.push(limit.min(line_len));
            run_levels.push(level);
            pos = limit;
        }
        let n = run_levels.len();
        if n <= 1 {
            let lvl = run_levels.first().copied().unwrap_or(para_level);
            let packed = ((i32::from(lvl) & RUN_LEVEL_MASK) << RUN_LEVEL_SHIFT)
                | (line_len & RUN_LENGTH_MASK);
            return LineDirections::Single([0, packed]);
        }

        let mut visual_map = vec![0i32; n];
        unsafe { (icu.reorder_visual)(run_levels.as_ptr(), n as i32, visual_map.as_mut_ptr()) };
        if bidi.paragraph_dir == DIR_RIGHT_TO_LEFT {
            visual_map.reverse();
        }
        let mut ld = Vec::with_capacity(n * 2);
        for &logical_idx in &visual_map {
            let li = logical_idx as usize;
            let s = run_starts[li];
            let len = run_limits[li] - s;
            let lvl = i32::from(run_levels[li]) & RUN_LEVEL_MASK;
            ld.push(s);
            ld.push((lvl << RUN_LEVEL_SHIFT) | (len & RUN_LENGTH_MASK));
        }
        LineDirections::Multi(ld.into_boxed_slice())
    }

    fn build_measured_text(&mut self, chars: &[u16], runs: &[StyleRunSpec]) -> MeasuredTextHandle {
        let jarr = self.upload_exact_chars_raw(chars);
        if jarr.is_null() {
            return MeasuredTextHandle { ptr: 0, owner: None };
        }
        let raw = self.raw_env;
        let text = self.text;
        let builder = unsafe { (text.mt_init_builder)() };
        if builder == 0 {
            return MeasuredTextHandle { ptr: 0, owner: None };
        }
        for run in runs {
            if run.start < run.end {
                unsafe {
                    (text.mt_add_style_run)(
                        raw,
                        std::ptr::null_mut(),
                        builder,
                        run.paint_ptr,
                        0,
                        0,
                        0,
                        run.start as i32,
                        run.end as i32,
                        u8::from(run.is_rtl),
                    );
                }
            }
        }
        let mt_ptr = unsafe {
            match text.mt_build {
                BuildMeasuredTextFn::V33(f) => {
                    f(raw, std::ptr::null_mut(), builder, 0, jarr, 0, 0, 0)
                }
                BuildMeasuredTextFn::V35(f) => {
                    f(raw, std::ptr::null_mut(), builder, 0, jarr, 0, 0, 0, 0)
                }
            }
        };
        unsafe { (text.mt_free_builder)(raw, std::ptr::null_mut(), builder) };
        let _ = clear_exception_check(raw);
        if mt_ptr == 0 {
            return MeasuredTextHandle { ptr: 0, owner: None };
        }
        MeasuredTextHandle {
            ptr: mt_ptr,
            owner: Some(Rc::new(MinikinMeasuredTextOwner {
                ptr: mt_ptr,
                release: text.mt_release,
            })),
        }
    }

    fn mt_width(&self, mt: &MeasuredTextHandle, start: usize, end: usize) -> f32 {
        if mt.ptr == 0 || start >= end {
            return 0.0;
        }
        unsafe { (self.text.mt_get_width)(mt.ptr, start as i32, end as i32) }
    }

    fn mt_char_width_at(&self, mt: &MeasuredTextHandle, offset: usize) -> f32 {
        if mt.ptr == 0 {
            return 0.0;
        }
        unsafe { (self.text.mt_get_char_width_at)(mt.ptr, offset as i32) }
    }

    fn compute_line_breaks(
        &mut self,
        mt: &MeasuredTextHandle,
        chars: &[u16],
        width: f32,
        justified: bool,
        out: &mut Vec<LineBreakItem>,
    ) {
        out.clear();
        if mt.ptr == 0 {
            return;
        }
        let lb_ptr = if justified { self.lb_justified } else { self.lb_normal };
        let w = width.max(0.0);
        let jarr = self.upload_exact_chars_raw(chars);
        if jarr.is_null() {
            return;
        }
        let raw = self.raw_env;
        let text = self.text;
        let res_ptr = unsafe {
            (text.lb_compute)(
                raw,
                std::ptr::null_mut(),
                lb_ptr,
                jarr,
                mt.ptr,
                chars.len() as i32,
                w,
                1,
                w,
                std::ptr::null_mut(),
                TAB_INCREMENT,
                0,
            )
        };
        let _ = clear_exception_check(raw);
        if res_ptr == 0 {
            return;
        }
        let count = unsafe { (text.lb_get_line_count)(res_ptr) }.max(0) as usize;
        out.reserve(count);
        for idx in 0..count {
            let i = idx as i32;
            let item = unsafe {
                LineBreakItem {
                    break_offset: (text.lb_get_line_break_offset)(res_ptr, i).max(0) as usize,
                    width: (text.lb_get_line_width)(res_ptr, i),
                    ascent: (text.lb_get_line_ascent)(res_ptr, i),
                    descent: (text.lb_get_line_descent)(res_ptr, i),
                    flags: (text.lb_get_line_flag)(res_ptr, i),
                }
            };
            out.push(item);
        }
        unsafe { (text.lb_release_result)(res_ptr) };
    }

    fn get_run_advance(
        &mut self,
        paint_ptr: i64,
        chars: &[u16],
        start: usize,
        end: usize,
        ctx_start: usize,
        ctx_end: usize,
        is_rtl: bool,
        offset: usize,
        run_flags: i32,
        word_spacing_px: f32,
        out_advances: Option<&mut [f32]>,
    ) -> f32 {
        if paint_ptr == 0 || start >= end || ctx_end > chars.len() {
            if let Some(slice) = out_advances {
                slice.fill(0.0);
            }
            return 0.0;
        }
        let base_flags = self.base_flags_of(paint_ptr);
        let wanted_flags = (base_flags & !RUN_EDGE_FLAGS_MASK) | (run_flags & RUN_EDGE_FLAGS_MASK);

        let jarr = self.upload_shared_chars_raw(&chars[..ctx_end]);
        let raw = self.raw_env;
        if wanted_flags != base_flags {
            unsafe { (self.hwui.paint_set_flags)(paint_ptr, wanted_flags) };
        }
        if word_spacing_px != 0.0 {
            unsafe { (self.text.paint_set_word_spacing)(paint_ptr, word_spacing_px) };
        }

        let advance = if let Some(dst) = out_advances {
            let run_len = end - start;
            let adv_jarr = self.ensure_advances_array_raw(run_len);
            if !adv_jarr.is_null() {
                let w = unsafe {
                    match self.text.paint_get_run_char_advance {
                        PaintGetRunCharAdvanceFn::V33(f) => f(
                            raw,
                            std::ptr::null_mut(),
                            paint_ptr,
                            jarr,
                            start as i32,
                            end as i32,
                            ctx_start as i32,
                            ctx_end as i32,
                            u8::from(is_rtl),
                            offset as i32,
                            adv_jarr,
                            0,
                        ),
                        PaintGetRunCharAdvanceFn::V34(f) => f(
                            raw,
                            std::ptr::null_mut(),
                            paint_ptr,
                            jarr,
                            start as i32,
                            end as i32,
                            ctx_start as i32,
                            ctx_end as i32,
                            u8::from(is_rtl),
                            offset as i32,
                            adv_jarr,
                            0,
                            std::ptr::null_mut(),
                            std::ptr::null_mut(),
                        ),
                    }
                };
                let copy_n = dst.len().min(run_len);
                if copy_n > 0 {
                    unsafe {
                        ((**raw).v1_2.GetFloatArrayRegion)(
                            raw,
                            adv_jarr,
                            0,
                            copy_n as jint,
                            dst.as_mut_ptr(),
                        );
                    }
                }
                w
            } else {
                0.0
            }
        } else {
            unsafe {
                (self.text.paint_get_run_advance)(
                    raw,
                    std::ptr::null_mut(),
                    paint_ptr,
                    jarr,
                    start as i32,
                    end as i32,
                    ctx_start as i32,
                    ctx_end as i32,
                    u8::from(is_rtl),
                    offset as i32,
                )
            }
        };

        if word_spacing_px != 0.0 {
            unsafe { (self.text.paint_set_word_spacing)(paint_ptr, 0.0) };
        }
        if wanted_flags != base_flags {
            unsafe { (self.hwui.paint_set_flags)(paint_ptr, base_flags) };
        }
        let _ = clear_exception_check(raw);
        advance
    }

    fn get_text_run_cursor(
        &mut self,
        paint_ptr: i64,
        chars: &[u16],
        ctx_start: usize,
        ctx_end: usize,
        is_rtl: bool,
        offset: usize,
        after: bool,
        word_spacing_px: f32,
    ) -> usize {
        if paint_ptr == 0 || ctx_start >= ctx_end || ctx_end > chars.len() {
            return offset;
        }
        let jarr = self.upload_shared_chars_raw(&chars[..ctx_end]);
        let raw = self.raw_env;
        if word_spacing_px != 0.0 {
            unsafe { (self.text.paint_set_word_spacing)(paint_ptr, word_spacing_px) };
        }
        let cursor_opt = if after { 0 } else { 2 };
        let next = unsafe {
            (self.text.paint_get_text_run_cursor)(
                raw,
                std::ptr::null_mut(),
                paint_ptr,
                jarr,
                ctx_start as i32,
                (ctx_end - ctx_start) as i32,
                i32::from(is_rtl),
                offset as i32,
                cursor_opt,
            )
        };
        if word_spacing_px != 0.0 {
            unsafe { (self.text.paint_set_word_spacing)(paint_ptr, 0.0) };
        }
        let _ = clear_exception_check(raw);
        next.max(0) as usize
    }
}

fn init_line_breaker(raw: *mut JNIEnv, text: &TextHwuiApi, justified: bool) -> i64 {
    let j = u8::from(justified);
    let ptr = unsafe {
        match text.lb_init {
            LineBreakerInitFn::V29(f) => {
                f(raw, std::ptr::null_mut(), 0, 0, j, std::ptr::null_mut())
            }
            LineBreakerInitFn::V35(f) => {
                f(raw, std::ptr::null_mut(), 0, 0, j, std::ptr::null_mut(), 0)
            }
        }
    };
    if clear_exception_check(raw) { 0 } else { ptr }
}

/// Native Rust `TextBackend` using [`NativeLayoutEngine<MinikinShaper>`].
pub(crate) struct NativeText {
    engine: NativeLayoutEngine<MinikinShaper>,
    paragraphs: SharedParagraphs,
}

impl NativeText {
    pub fn try_new(env: &mut Env, paragraphs: SharedParagraphs) -> Option<Self> {
        let shaper = MinikinShaper::try_new(env)?;
        Some(Self { engine: NativeLayoutEngine::new(shaper), paragraphs })
    }

    /// Attempts to lay out `text` natively; returns `None` when `request` encodes features routed
    /// to `JniText` (placeholders, custom loaded typefaces, brush/shadow/stroke paint extras, or
    /// start/middle ellipsis).
    pub fn try_layout(&mut self, handle: u32, text: &str, request: &[u8]) -> Option<LaidOut> {
        let is_probe = request.first().is_some_and(|&f| (f & 2) != 0);
        let out = self.engine.layout(handle, text, request)?;
        if !is_probe {
            self.paragraphs.borrow_mut().insert_native(
                handle,
                text,
                out.simple_paint_ptr,
                out.simple_paint_owner,
                out.simple_baseline,
                out.complex_draw,
                out.laid_out.metrics.size.width,
                out.laid_out.metrics.size.height,
                out.min_same_width,
                out.max_same_width,
                true,
            );
        }
        Some(out.laid_out)
    }
}

impl TextBackend for NativeText {
    fn caches_released(&self) -> bool {
        true
    }

    fn layout(&mut self, handle: u32, text: &str, request: &[u8]) -> LaidOut {
        self.try_layout(handle, text, request).unwrap_or(LaidOut {
            metrics: ParagraphMetrics {
                size: IntSize::new(0, 0),
                first_baseline: 0.0,
                last_baseline: 0.0,
                line_count: 1,
            },
            did_exceed_max_lines: false,
            max_intrinsic_width: Some(0.0),
        })
    }

    fn relayout(
        &mut self,
        previous: u32,
        handle: u32,
        text: &str,
        min_width: i32,
        max_width: i32,
        max_height: i32,
    ) -> Option<LaidOut> {
        let out = self.engine.relayout(previous, handle, min_width, max_width, max_height)?;
        self.paragraphs.borrow_mut().relayout_native_from(
            previous,
            handle,
            text,
            out.simple_paint_ptr,
            out.simple_paint_owner,
            out.simple_baseline,
            out.complex_draw,
            out.laid_out.metrics.size.width,
            out.laid_out.metrics.size.height,
            out.min_same_width,
            out.max_same_width,
        );
        Some(out.laid_out)
    }

    fn relayout_in_place(
        &mut self,
        handle: u32,
        min_width: i32,
        max_width: i32,
        max_height: i32,
    ) -> Option<LaidOut> {
        let laid_out = self.engine.relayout_in_place(handle, min_width, max_width, max_height)?;
        self.paragraphs.borrow_mut().update_width_in_place(handle, laid_out.metrics.size.width);
        Some(laid_out)
    }

    fn probe_layout(
        &mut self,
        previous: u32,
        handle: u32,
        min_width: i32,
        max_width: i32,
        max_height: i32,
    ) -> Option<LaidOut> {
        self.engine
            .probe_layout(previous, handle, min_width, max_width, max_height)
            .map(|o| o.laid_out)
    }

    fn restyle(
        &mut self,
        previous: u32,
        handle: u32,
        text: &str,
        color: u32,
        decoration: u8,
        has_extras: bool,
    ) -> bool {
        if has_extras {
            return false;
        }
        let Some(out) = self.engine.restyle(previous, handle, color, decoration) else {
            return false;
        };
        let mut paras = self.paragraphs.borrow_mut();
        paras.remove(previous);
        paras.insert_native(
            handle,
            text,
            out.simple_paint_ptr,
            out.simple_paint_owner,
            out.simple_baseline,
            out.complex_draw,
            out.laid_out.metrics.size.width,
            out.laid_out.metrics.size.height,
            out.min_same_width,
            out.max_same_width,
            true,
        );
        true
    }

    fn hit_test(&self, handle: u32, position: Offset) -> usize {
        self.engine.hit_test(handle, position)
    }

    fn cursor(&self, handle: u32, offset: usize) -> Rect {
        self.engine.cursor(handle, offset)
    }

    fn release(&mut self, handle: u32) {
        self.engine.release(handle);
        self.paragraphs.borrow_mut().remove(handle);
    }
}
