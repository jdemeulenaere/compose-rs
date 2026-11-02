//! Direct native `DisplayList` -> `libhwui.so` (`RenderNode` / `RecordingCanvas`) recorder.
//!
//! Eliminates display-list byte-buffer serialization (`serialize.rs`) and JVM `ByteBuffer`
//! parsing/JNI replay (`Replayer.kt`) when `HwuiApi` resolves on Android 10+ (API 29+).

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Weak;

use compose_ui::{
    CompositingStrategy, ImageBitmap, Offset, Outline, Paint, PaintStyle, PathCommand, PathData,
    PathFillType, PathOperation, PathStep, Rect, StrokeCap, TileMode,
    platform::{
        Boundary, DisplayList, DrawGroup, DrawOp, LayerProperties, Optimization, Optimizations,
        Shader, single_drawing,
    },
};
use jni::Env;
use jni::ids::JMethodID;
use jni::objects::{JClass, JObject};
use jni::refs::Global;
use jni::signature::{Primitive, ReturnType};
use jni::sys::{
    JNIEnv, jcharArray, jfloat, jfloatArray, jint, jlongArray, jmethodID, jobject, jvalue,
};
use jni::{JValue, jni_sig, jni_str};

use crate::bridge::{check, method};
use crate::hwui::HwuiApi;
use crate::serialize::{Shading, faded, foldable_layer, shading};
use crate::text::SharedParagraphs;
use crate::text_layout::ComplexParagraphDraw;

/// `new Paint(Paint.ANTI_ALIAS_FLAG)`: `ANTI_ALIAS_FLAG (0x01) | HIDDEN_DEFAULT_PAINT_FLAGS (0x502)`.
const DEFAULT_PAINT_FLAGS: i32 = 0x503;
/// `Canvas.MATRIX_SAVE_FLAG | Canvas.CLIP_SAVE_FLAG`.
const SAVE_MATRIX_CLIP: i32 = 0x03;
/// `Region.Op.INTERSECT.nativeInt`.
const CLIP_OP_INTERSECT: i32 = 1;
/// `PorterDuff.Mode.CLEAR.nativeInt`.
const PORTER_DUFF_CLEAR: i32 = 0;
/// `Paint.BIDI_DEFAULT_LTR`.
const BIDI_DEFAULT_LTR: i32 = 2;
/// `Path.Direction.CW` / `CCW`.
const PATH_DIR_CW: i32 = 0;
const PATH_DIR_CCW: i32 = 1;
/// Opaque black (`0xFF000000`).
const BLACK_ARGB: i32 = 0xFF00_0000u32 as i32;

const FILL_PAINT_SLOTS: usize = 64;
const FILL_PAINT_MASK: usize = FILL_PAINT_SLOTS - 1;
const SHADER_CACHE_CAP: usize = 16;
const GROUP_POOL_CAP: usize = 64;
const PARA_POOL_CAP: usize = 512;
const PATH_POOL_CAP: usize = 64;
const STEP_POOL_CAP: usize = 16;
const CANVAS_POOL_CAP: usize = 16;
const AHEAD_ITEMS_CAP: usize = 8;
const AHEAD_GROUPS_CAP: usize = 128;
const INITIAL_PARA_RECORD_NS: u64 = 30_000;
const MAX_PARA_RECORD_NS: u64 = 1_000_000;
const EXACT_CHAR_ARRAY_CAP: usize = 64;
const RUN_EDGE_FLAGS_MASK: i32 = 0x6000;

const COMP_AUTO: u8 = 0;
const COMP_OFFSCREEN: u8 = 1;
const COMP_MODULATE_ALPHA: u8 = 2;

#[inline]
fn java_round(v: f32) -> i32 {
    (f64::from(v) + 0.5).floor() as i32
}

#[derive(Default)]
struct RecordingCanvasPool {
    free: Vec<i64>,
}

impl RecordingCanvasPool {
    #[inline]
    fn begin(&mut self, api: &HwuiApi, node: i64, width: i32, height: i32) -> i64 {
        let w = width.max(1);
        let h = height.max(1);
        if let Some(canvas) = self.free.pop() {
            unsafe { (api.dlc_reset)(canvas, node, w, h) };
            canvas
        } else {
            unsafe { (api.dlc_create)(node, w, h) }
        }
    }

    #[inline]
    fn end(&mut self, api: &HwuiApi, canvas: i64, node: i64) {
        unsafe { api.finish_recording(canvas, node) };
        if self.free.len() < CANVAS_POOL_CAP {
            self.free.push(canvas);
        } else {
            unsafe { (api.canvas_destroy)(canvas) };
        }
    }

    fn destroy_all(&mut self, api: &HwuiApi) {
        for canvas in self.free.drain(..) {
            if canvas != 0 {
                unsafe { (api.canvas_destroy)(canvas) };
            }
        }
    }
}

struct GroupEntry {
    node: i64,
    version: u64,
    content_gen: u64,
    recorded_path_serial: u64,
    recorded_children: Vec<(i64, bool)>,
    frame: u32,
    layer_frame: u32,
    layered: bool,
    elevated: bool,
    outline_clips: bool,
    clip_path: i64,
    width: i32,
    height: i32,
    single_drawing: bool,
    single_drawing_version: u64,
    bounds_bits: [u32; 4],
    alpha_bits: u32,
    pivot_bits: [u32; 2],
    scale_bits: [u32; 2],
    rotation_bits: u32,
    trans_bits: [u32; 2],
    clip_bool: bool,
    outline_tag: u8,
    outline_bounds_bits: [u32; 4],
    outline_radii_bits: [u32; 4],
    elevation_bits: u32,
    ambient_argb: u32,
    spot_argb: u32,
    compositing_mode: u8,
}

impl GroupEntry {
    fn new(node: i64) -> Self {
        Self {
            node,
            version: u64::MAX,
            content_gen: u64::MAX,
            recorded_path_serial: 0,
            recorded_children: Vec::new(),
            frame: 0,
            layer_frame: 0,
            layered: false,
            elevated: false,
            outline_clips: true,
            clip_path: 0,
            width: -1,
            height: -1,
            single_drawing: false,
            single_drawing_version: u64::MAX,
            bounds_bits: [0; 4],
            alpha_bits: 0,
            pivot_bits: [0; 2],
            scale_bits: [0; 2],
            rotation_bits: 0,
            trans_bits: [0; 2],
            clip_bool: false,
            outline_tag: u8::MAX,
            outline_bounds_bits: [0; 4],
            outline_radii_bits: [0; 4],
            elevation_bits: 0,
            ambient_argb: 0,
            spot_argb: 0,
            compositing_mode: u8::MAX,
        }
    }

    #[inline]
    fn set_position(&mut self, api: &HwuiApi, width: i32, height: i32) {
        if width == self.width && height == self.height {
            return;
        }
        self.width = width;
        self.height = height;
        unsafe { (api.rn_set_ltrb)(self.node, 0, 0, width, height) };
    }
}

struct KeptPath {
    contents: Weak<PathData>,
    native_path: i64,
    defined_serial: u64,
    drawn_as_path: bool,
}

struct KeptImage {
    pixels: Weak<[u8]>,
    java_bitmap: Global<JObject<'static>>,
    native_bitmap: i64,
}

#[derive(Clone, PartialEq)]
struct ShaderKey {
    kind: u8,
    tile_mode: u8,
    geom_bits: [u32; 4],
    colors: Vec<u32>,
    stops_bits: Vec<u32>,
    local_bits: [u32; 4],
}

struct CachedShader {
    key: ShaderKey,
    ptr: i64,
}

pub(crate) struct HwuiRecorder {
    api: &'static HwuiApi,
    paragraphs: SharedParagraphs,
    measurer: Global<JObject<'static>>,
    draw_layout_direct: JMethodID,
    canvas_proxy: Global<JObject<'static>>,
    char_array: Global<JObject<'static>>,
    char_array_cap: usize,
    exact_char_arrays: HashMap<usize, Global<JObject<'static>>>,
    radii_array: Global<JObject<'static>>,
    color_long_array: Global<JObject<'static>>,
    color_long_cap: usize,

    canvas_pool: RecordingCanvasPool,
    scratch_path: i64,
    outline_path: i64,
    scratch_matrix: i64,
    kept_node: i64,
    flush_node: i64,
    chunk_nodes: Vec<i64>,
    chunk_used: usize,
    group_node_pool: Vec<i64>,
    para_node_pool: Vec<i64>,

    fill_paint_ptrs: [i64; FILL_PAINT_SLOTS],
    fill_paint_colors: [u32; FILL_PAINT_SLOTS],
    fill_paint_valid: [bool; FILL_PAINT_SLOTS],

    general_paint: i64,
    gen_color: u32,
    gen_stroke: bool,
    gen_width: f32,
    gen_cap: u8,
    gen_shader: i64,

    bitmap_paint_obj: Global<JObject<'static>>,
    bitmap_paint_set_cf: jmethodID,
    pd_cf_class: Global<JClass<'static>>,
    pd_cf_ctor: jmethodID,
    pd_src_in: Global<JObject<'static>>,
    bitmap_paint: i64,
    bitmap_alpha: i32,
    bitmap_tint: u32,

    shaders: Vec<CachedShader>,
    shader_next: usize,

    groups: HashMap<u64, GroupEntry>,
    ahead: HashMap<u64, GroupEntry>,
    ahead_items: HashSet<u64>,
    prev_kept_nodes: Vec<i64>,
    cur_kept_nodes: Vec<i64>,
    open_layers: Vec<usize>,
    folds: Vec<(usize, f32)>,
    child_scratch: Vec<(i64, bool)>,

    paths: HashMap<(u64, u64), KeptPath>,
    free_paths_by_owner: HashMap<u64, Vec<KeptPath>>,
    path_pool: Vec<i64>,
    step_paths: Vec<i64>,
    step_stack: Vec<i64>,
    path_serial: u64,
    named_path_serial_max: u64,
    flush_layers: bool,

    images: HashMap<usize, KeptImage>,

    optimizations: Optimizations,
    frame: u32,
    width: i32,
    height: i32,
    para_record_ns: u64,
    recorded_paras_last: usize,
}

impl HwuiRecorder {
    pub fn try_new(
        env: &mut Env,
        measurer: &JObject,
        paragraphs: SharedParagraphs,
        optimizations: Optimizations,
    ) -> Option<Self> {
        let api = crate::hwui::get(env)?;
        let raw = env.get_raw();

        let proxy_cls = env.find_class(jni_str!("rs/compose/android/NativeCanvasProxy")).ok()?;
        let proxy_ctor = env.get_method_id(&proxy_cls, jni_str!("<init>"), jni_sig!("()V")).ok()?;
        let proxy_local = unsafe { env.new_object_unchecked(&proxy_cls, proxy_ctor, &[]) }.ok()?;
        env.delete_local_ref(proxy_cls);
        let canvas_proxy = env.new_global_ref(&proxy_local).ok()?;
        env.delete_local_ref(proxy_local);

        let char_arr_raw = unsafe { ((**raw).v1_2.NewCharArray)(raw, 128) };
        if char_arr_raw.is_null() {
            return None;
        }
        let char_arr_obj = unsafe { JObject::from_raw(env, char_arr_raw as jobject) };
        let char_array = env.new_global_ref(&char_arr_obj).ok()?;
        env.delete_local_ref(char_arr_obj);

        let radii_raw = unsafe { ((**raw).v1_2.NewFloatArray)(raw, 8) };
        if radii_raw.is_null() {
            return None;
        }
        let radii_obj = unsafe { JObject::from_raw(env, radii_raw as jobject) };
        let radii_array = env.new_global_ref(&radii_obj).ok()?;
        env.delete_local_ref(radii_obj);

        let cl_raw = unsafe { ((**raw).v1_2.NewLongArray)(raw, 8) };
        if cl_raw.is_null() {
            return None;
        }
        let cl_obj = unsafe { JObject::from_raw(env, cl_raw as jobject) };
        let color_long_array = env.new_global_ref(&cl_obj).ok()?;
        env.delete_local_ref(cl_obj);

        let measurer_global = env.new_global_ref(measurer).ok()?;
        let draw_layout_direct = method(
            env,
            measurer,
            jni_str!("drawLayoutDirect"),
            jni_sig!("(Landroid/graphics/Canvas;IFF)V"),
        );

        let scratch_path = unsafe { (api.path_init)(raw, std::ptr::null_mut()) };
        let outline_path = unsafe { (api.path_init)(raw, std::ptr::null_mut()) };
        let scratch_matrix = unsafe { (api.matrix_create)(raw, std::ptr::null_mut(), 0) };
        let general_paint = create_default_paint(raw, api);

        let paint_cls = env.find_class(jni_str!("android/graphics/Paint")).ok()?;
        let paint_ctor =
            env.get_method_id(&paint_cls, jni_str!("<init>"), jni_sig!("(I)V")).ok()?;
        let bitmap_paint_set_cf = env
            .get_method_id(
                &paint_cls,
                jni_str!("setColorFilter"),
                jni_sig!("(Landroid/graphics/ColorFilter;)Landroid/graphics/ColorFilter;"),
            )
            .ok()?
            .into_raw();
        let bmp_paint_local = unsafe {
            env.new_object_unchecked(&paint_cls, paint_ctor, &[JValue::Int(0x01).as_jni()])
        }
        .ok()?;
        env.delete_local_ref(paint_cls);
        let bitmap_paint_obj = env.new_global_ref(&bmp_paint_local).ok()?;
        env.delete_local_ref(bmp_paint_local);
        let bitmap_paint = unsafe {
            ((**raw).v1_2.CallLongMethodA)(
                raw,
                bitmap_paint_obj.as_raw(),
                api.paint_get_native_instance,
                std::ptr::null(),
            )
        };
        if bitmap_paint == 0 {
            return None;
        }

        let pd_cf_local =
            env.find_class(jni_str!("android/graphics/PorterDuffColorFilter")).ok()?;
        let pd_cf_ctor = env
            .get_method_id(
                &pd_cf_local,
                jni_str!("<init>"),
                jni_sig!("(ILandroid/graphics/PorterDuff$Mode;)V"),
            )
            .ok()?
            .into_raw();
        let pd_cf_class = env.new_global_ref(pd_cf_local).ok()?;

        let pd_mode_cls = env.find_class(jni_str!("android/graphics/PorterDuff$Mode")).ok()?;
        let pd_src_in_local = env
            .get_static_field(
                &pd_mode_cls,
                jni_str!("SRC_IN"),
                jni_sig!("Landroid/graphics/PorterDuff$Mode;"),
            )
            .and_then(|v| v.l())
            .ok()?;
        env.delete_local_ref(pd_mode_cls);
        let pd_src_in = env.new_global_ref(&pd_src_in_local).ok()?;
        env.delete_local_ref(pd_src_in_local);

        let kept_node = create_render_node(raw, api);
        unsafe {
            (api.rn_set_alpha)(kept_node, 0.0);
            (api.rn_set_clip_to_bounds)(kept_node, 1);
            (api.rn_set_ltrb)(kept_node, 0, 0, 0, 0);
        }

        let flush_node = create_render_node(raw, api);
        unsafe {
            (api.rn_set_ltrb)(flush_node, 0, 0, 1, 1);
            (api.rn_set_layer_type)(flush_node, 2);
            (api.rn_set_layer_paint)(flush_node, 0);
        }

        Some(Self {
            api,
            paragraphs,
            measurer: measurer_global,
            draw_layout_direct,
            canvas_proxy,
            char_array,
            char_array_cap: 128,
            exact_char_arrays: HashMap::new(),
            radii_array,
            color_long_array,
            color_long_cap: 8,
            canvas_pool: RecordingCanvasPool::default(),
            scratch_path,
            outline_path,
            scratch_matrix,
            kept_node,
            flush_node,
            chunk_nodes: Vec::with_capacity(12),
            chunk_used: 0,
            group_node_pool: Vec::with_capacity(GROUP_POOL_CAP),
            para_node_pool: Vec::with_capacity(PARA_POOL_CAP),
            fill_paint_ptrs: [0; FILL_PAINT_SLOTS],
            fill_paint_colors: [0; FILL_PAINT_SLOTS],
            fill_paint_valid: [false; FILL_PAINT_SLOTS],
            general_paint,
            gen_color: BLACK_ARGB as u32,
            gen_stroke: false,
            gen_width: 0.0,
            gen_cap: 0,
            gen_shader: 0,
            bitmap_paint_obj,
            bitmap_paint_set_cf,
            pd_cf_class,
            pd_cf_ctor,
            pd_src_in,
            bitmap_paint,
            bitmap_alpha: 255,
            bitmap_tint: 0,
            shaders: Vec::with_capacity(SHADER_CACHE_CAP),
            shader_next: 0,
            groups: HashMap::with_capacity(256),
            ahead: HashMap::new(),
            ahead_items: HashSet::new(),
            prev_kept_nodes: Vec::new(),
            cur_kept_nodes: Vec::new(),
            open_layers: Vec::new(),
            folds: Vec::new(),
            child_scratch: Vec::new(),
            paths: HashMap::new(),
            free_paths_by_owner: HashMap::new(),
            path_pool: Vec::with_capacity(PATH_POOL_CAP),
            step_paths: Vec::with_capacity(STEP_POOL_CAP),
            step_stack: Vec::new(),
            path_serial: 0,
            named_path_serial_max: 0,
            flush_layers: false,
            images: HashMap::new(),
            optimizations,
            frame: 0,
            width: 1,
            height: 1,
            para_record_ns: INITIAL_PARA_RECORD_NS,
            recorded_paras_last: 0,
        })
    }

    pub fn set_optimizations(&mut self, optimizations: Optimizations) {
        let draws_differently = self.optimizations.draws_differently(optimizations);
        self.optimizations = optimizations;
        if draws_differently {
            for group in self.groups.values_mut() {
                group.version = u64::MAX;
            }
            for group in self.ahead.values_mut() {
                group.version = u64::MAX;
            }
        }
    }

    pub fn recorded_paragraphs_last(&self) -> usize {
        self.recorded_paras_last
    }

    /// Records `list` onto a hardware-accelerated Java `Canvas` (`ComposeRsView.onDraw`).
    /// Returns `false` if `canvas` has no native hardware handle (e.g. software canvas).
    pub fn record_to_canvas(
        &mut self,
        env: &mut Env,
        canvas_obj: &JObject,
        list: &DisplayList,
    ) -> bool {
        let raw = env.get_raw();
        let canvas_ptr = unsafe { (self.api.acanvas_from_java)(raw, canvas_obj.as_raw()) } as i64;
        if canvas_ptr == 0 {
            return false;
        }
        let w = unsafe { (self.api.canvas_get_width)(canvas_ptr) }.max(1);
        let h = unsafe { (self.api.canvas_get_height)(canvas_ptr) }.max(1);
        self.record_frame_onto_native_canvas(env, canvas_ptr, w, h, list);
        true
    }

    /// Records prefetched paragraphs ahead (`PARAGRAPH_RENDER_NODES`) until `deadline_nanos`.
    /// Returns whether paragraphs remain to be recorded.
    pub fn record_paragraphs_ahead(&mut self, env: &mut Env, deadline_nanos: i64) -> bool {
        self.drain_retired_paragraph_nodes();
        self.recorded_paras_last = 0;
        if !self.optimizations.contains(Optimization::PARAGRAPH_RENDER_NODES) {
            self.paragraphs.borrow_mut().laid_out_ahead.clear();
            return false;
        }
        let pending_len = self.paragraphs.borrow().laid_out_ahead.len();
        if pending_len == 0 {
            return false;
        }
        let _trace = compose_ui::platform::trace::section(c"compose-rs: record paragraphs");
        let mut index = 0usize;
        let mut recorded = 0usize;
        while index < pending_len {
            let handle = {
                let table = self.paragraphs.borrow();
                let Some(&h) = table.laid_out_ahead.get(index) else { break };
                let Some(slot) = table.get(h) else {
                    index += 1;
                    continue;
                };
                let is_empty = (slot.paint_ptr != 0 && slot.char_len == 0)
                    || (slot.native_owned && slot.paint_ptr == 0 && slot.complex_draw.is_none());
                if slot.node_ptr != 0 || is_empty {
                    index += 1;
                    continue;
                }
                h
            };
            let start_ns = monotonic_nanos();
            if start_ns >= deadline_nanos
                || (recorded > 0
                    && start_ns.saturating_add(self.para_record_ns as i64) > deadline_nanos)
            {
                break;
            }
            self.ensure_paragraph_node(env, handle);
            recorded += 1;
            let took = (monotonic_nanos() - start_ns).clamp(0, MAX_PARA_RECORD_NS as i64) as u64;
            self.para_record_ns = (self.para_record_ns * 3 + took) / 4;
            index += 1;
        }
        let mut table = self.paragraphs.borrow_mut();
        let drain_to = index.min(table.laid_out_ahead.len());
        table.laid_out_ahead.drain(..drain_to);
        let remaining = !table.laid_out_ahead.is_empty();
        drop(table);
        self.recorded_paras_last = recorded;
        remaining
    }

    /// Records a prefetched item's `DrawGroup`s ahead of placement (`D350`).
    /// Returns whether any group was recorded.
    pub fn record_groups_ahead(&mut self, env: &mut Env, groups: &[DrawGroup]) -> bool {
        self.drain_retired_paragraph_nodes();
        self.harvest_resources(env);
        if self.frame == 0 {
            self.collect_unused_resources(env);
            return false;
        }
        self.ahead_items.retain(|k| self.ahead.contains_key(k));
        if self.ahead_items.len() >= AHEAD_ITEMS_CAP || self.ahead.len() >= AHEAD_GROUPS_CAP {
            self.drop_ahead();
        }
        self.apply_layers_for_slice(env, groups, true);
        let mut recorded = false;
        let mut index = 0usize;
        while let Some(group) = groups.get(index) {
            if group.empty {
                index += 1 + group.descendants;
                continue;
            }
            if !self.groups.contains_key(&group.key) {
                if !recorded {
                    self.ahead_items.insert(group.key);
                }
                recorded = true;
                self.record_single_group(env, groups, index, true);
            }
            index += 1;
        }
        self.collect_unused_resources(env);
        recorded
    }

    fn record_frame_onto_native_canvas(
        &mut self,
        env: &mut Env,
        root_canvas: i64,
        width: i32,
        height: i32,
        list: &DisplayList,
    ) {
        let api = self.api;
        self.frame = self.frame.wrapping_add(1);
        if self.frame == 0 {
            self.frame = 1;
        }
        self.width = width;
        self.height = height;
        self.chunk_used = 0;

        self.drain_retired_paragraph_nodes();
        self.harvest_resources(env);

        let cur_frame = self.frame;

        // 1. Ensure every active group in `list.groups` has a `GroupEntry` and mark `frame`.
        let mut index = 0usize;
        while let Some(group) = list.groups.get(index) {
            if group.empty {
                index += 1 + group.descendants;
            } else {
                let entry = self.ensure_frame_group(env.get_raw(), group.key);
                entry.frame = cur_frame;
                index += 1;
            }
        }

        // 2. Apply `LAYERS` properties for `list.groups` (innermost closed first, matching `serialize.rs`).
        self.apply_layers_for_slice(env, &list.groups, false);

        // 3. Mark `list.kept` groups as alive for both `frame` and `layer_frame`.
        for group in &list.kept {
            if group.empty {
                continue;
            }
            if let Some(entry) = self.groups.get_mut(&group.key) {
                entry.frame = cur_frame;
                entry.layer_frame = cur_frame;
            }
        }

        // 4. Retire layers from groups that are no longer layered this frame.
        for entry in self.groups.values_mut() {
            if !entry.layered || entry.layer_frame == cur_frame {
                continue;
            }
            reset_layer_on_node(api, entry.node);
            entry.set_position(api, width, height);
            entry.layered = false;
            entry.outline_clips = true;
            if entry.clip_path != 0 {
                unsafe { (api.path_destroy)(entry.clip_path) };
                entry.clip_path = 0;
                entry.version = u64::MAX;
            }
            entry.elevated = false;
        }

        // 5. Record all changed non-empty groups in `list.groups`.
        index = 0;
        while let Some(group) = list.groups.get(index) {
            if group.empty {
                index += 1 + group.descendants;
            } else {
                self.record_single_group(env, &list.groups, index, false);
                index += 1;
            }
        }

        // 6. Update `list.kept` groups whose version changed and record `kept_node` if needed.
        self.cur_kept_nodes.clear();
        let mut nested_until = 0usize;
        for (k_idx, group) in list.kept.iter().enumerate() {
            if group.empty {
                continue;
            }
            let needs_record = match self.groups.get(&group.key) {
                Some(entry) => {
                    entry.version != group.version || unsafe { (api.rn_is_valid)(entry.node) == 0 }
                }
                None => true,
            };
            if needs_record {
                let entry = self.ensure_frame_group(env.get_raw(), group.key);
                entry.frame = cur_frame;
                entry.layer_frame = cur_frame;
                self.record_single_group(env, &list.kept, k_idx, false);
            }
            let outer = k_idx >= nested_until;
            if outer {
                nested_until = k_idx + 1 + group.descendants;
                if let Some(entry) = self.groups.get(&group.key) {
                    self.cur_kept_nodes.push(entry.node);
                }
            }
        }
        if !self.cur_kept_nodes.is_empty() {
            let changed = self.cur_kept_nodes != self.prev_kept_nodes
                || unsafe { (api.rn_is_valid)(self.kept_node) == 0 };
            if changed {
                let kc = self.canvas_pool.begin(api, self.kept_node, 1, 1);
                for &node in &self.cur_kept_nodes {
                    unsafe { (api.dlc_draw_render_node)(kc, node) };
                }
                self.canvas_pool.end(api, kc, self.kept_node);
                self.prev_kept_nodes.clear();
                self.prev_kept_nodes.extend_from_slice(&self.cur_kept_nodes);
            }
            unsafe { (api.dlc_draw_render_node)(root_canvas, self.kept_node) };
        } else if !self.prev_kept_nodes.is_empty() {
            unsafe { (api.rn_discard_dl)(self.kept_node) };
            self.prev_kept_nodes.clear();
        }

        // 7. Record the root display list operations onto `root_canvas`.
        self.record_display_list_ops(env, root_canvas, list, &list.groups, 0, true, false);

        // 8. Collect unused paths/images (sets `flush_layers` if a drawn path was dropped).
        self.collect_unused_resources(env);

        // 9. Emit `flush_node` if `FLUSH_LAYERS_BEFORE_DROPPED_PATHS` is active and a path was dropped.
        let flush = self.flush_layers;
        self.flush_layers = false;
        if flush && self.optimizations.contains(Optimization::FLUSH_LAYERS_BEFORE_DROPPED_PATHS) {
            let fc = self.canvas_pool.begin(api, self.flush_node, 1, 1);
            unsafe {
                (api.canvas_draw_color)(
                    env.get_raw(),
                    std::ptr::null_mut(),
                    fc,
                    0,
                    PORTER_DUFF_CLEAR,
                );
            }
            self.canvas_pool.end(api, fc, self.flush_node);
            unsafe { (api.dlc_draw_render_node)(root_canvas, self.flush_node) };
        }

        // 10. Recycle groups not drawn or kept on this frame.
        let pool = &mut self.group_node_pool;
        self.groups.retain(|_, entry| {
            if entry.frame == cur_frame {
                return true;
            }
            if entry.layered {
                reset_layer_on_node(api, entry.node);
            }
            if entry.clip_path != 0 {
                unsafe { (api.path_destroy)(entry.clip_path) };
                entry.clip_path = 0;
            }
            recycle_group_node(api, pool, entry.node);
            false
        });
    }

    fn ensure_frame_group(&mut self, raw_env: *mut JNIEnv, key: u64) -> &mut GroupEntry {
        if !self.groups.contains_key(&key) {
            let entry = if let Some(from_ahead) = self.ahead.remove(&key) {
                from_ahead
            } else {
                let node = obtain_group_node(raw_env, self.api, &mut self.group_node_pool);
                GroupEntry::new(node)
            };
            self.groups.insert(key, entry);
        }
        self.groups.get_mut(&key).unwrap_or_else(|| unreachable!())
    }

    fn ensure_ahead_group(&mut self, raw_env: *mut JNIEnv, key: u64) -> &mut GroupEntry {
        let api = self.api;
        let pool = &mut self.group_node_pool;
        self.ahead
            .entry(key)
            .or_insert_with(|| GroupEntry::new(obtain_group_node(raw_env, api, pool)))
    }

    fn apply_layers_for_slice(&mut self, env: &mut Env, groups: &[DrawGroup], ahead: bool) {
        let has_any = groups.iter().any(|g| !g.empty && g.layer.is_some());
        if !has_any {
            return;
        }
        let mut open = std::mem::take(&mut self.open_layers);
        open.clear();
        for index in 0..=groups.len() {
            while let Some(&last) = open.last()
                && (index == groups.len() || last + groups[last].descendants < index)
            {
                open.pop();
                self.apply_single_layer(env, groups, last, ahead);
            }
            if groups.get(index).is_some_and(|g| !g.empty && g.layer.is_some()) {
                open.push(index);
            }
        }
        self.open_layers = open;
    }

    fn apply_single_layer(
        &mut self,
        env: &mut Env,
        groups: &[DrawGroup],
        index: usize,
        ahead: bool,
    ) {
        let group = &groups[index];
        let Some(layer) = group.layer.as_ref() else { return };
        if ahead && self.groups.contains_key(&group.key) {
            return;
        }
        let comp = self.compositing_of(groups, index, layer);
        let cur_frame = self.frame;
        let raw_env = env.get_raw();
        let api = self.api;
        let outline_path_scratch = self.outline_path;
        let radii_jarray = self.radii_array.as_raw() as jfloatArray;

        let (out_tag, out_rect, out_radii, generic_path_ptr, generic_offset) = match &layer.outline
        {
            None => (0u8, Rect::new(0.0, 0.0, 0.0, 0.0), [0.0f32; 4], 0i64, Offset::ZERO),
            Some(Outline::Rectangle(r)) => (1u8, *r, [0.0f32; 4], 0i64, Offset::ZERO),
            Some(Outline::Rounded(r, rad)) => (2u8, *r, *rad, 0i64, Offset::ZERO),
            Some(outline @ Outline::Generic(path, offset)) => {
                let p_native = self.ensure_native_path(raw_env, path);
                let serial = self
                    .paths
                    .get(&(path.id(), path.generation()))
                    .map_or(0, |k| k.defined_serial as u32);
                (
                    3u8,
                    outline.bounds(),
                    [f32::from_bits(path.id() as u32), offset.x, offset.y, f32::from_bits(serial)],
                    p_native,
                    *offset,
                )
            }
        };

        let entry = if ahead {
            self.ensure_ahead_group(raw_env, group.key)
        } else {
            self.ensure_frame_group(raw_env, group.key)
        };
        entry.layer_frame = cur_frame;

        let node = entry.node;
        let was_layered = entry.layered;

        let b_bits = [
            layer.bounds.left.to_bits(),
            layer.bounds.top.to_bits(),
            layer.bounds.right.to_bits(),
            layer.bounds.bottom.to_bits(),
        ];
        if !was_layered || b_bits != entry.bounds_bits {
            entry.bounds_bits = b_bits;
            entry.set_position(
                api,
                layer.bounds.right.ceil() as i32,
                layer.bounds.bottom.ceil() as i32,
            );
        }

        let alpha_bits = layer.alpha.to_bits();
        if !was_layered || alpha_bits != entry.alpha_bits {
            entry.alpha_bits = alpha_bits;
            unsafe { (api.rn_set_alpha)(node, layer.alpha) };
        }

        let pivot_bits = [layer.pivot.x.to_bits(), layer.pivot.y.to_bits()];
        if !was_layered || pivot_bits[0] != entry.pivot_bits[0] {
            entry.pivot_bits[0] = pivot_bits[0];
            unsafe { (api.rn_set_pivot_x)(node, layer.pivot.x) };
        }
        if !was_layered || pivot_bits[1] != entry.pivot_bits[1] {
            entry.pivot_bits[1] = pivot_bits[1];
            unsafe { (api.rn_set_pivot_y)(node, layer.pivot.y) };
        }

        let scale_bits = [layer.scale.0.to_bits(), layer.scale.1.to_bits()];
        if !was_layered || scale_bits[0] != entry.scale_bits[0] {
            entry.scale_bits[0] = scale_bits[0];
            unsafe { (api.rn_set_scale_x)(node, layer.scale.0) };
        }
        if !was_layered || scale_bits[1] != entry.scale_bits[1] {
            entry.scale_bits[1] = scale_bits[1];
            unsafe { (api.rn_set_scale_y)(node, layer.scale.1) };
        }

        let rot_bits = layer.rotation.to_bits();
        if !was_layered || rot_bits != entry.rotation_bits {
            entry.rotation_bits = rot_bits;
            unsafe { (api.rn_set_rotation)(node, layer.rotation) };
        }

        let trans_bits = [layer.translation.x.to_bits(), layer.translation.y.to_bits()];
        if !was_layered || trans_bits[0] != entry.trans_bits[0] {
            entry.trans_bits[0] = trans_bits[0];
            unsafe { (api.rn_set_translation_x)(node, layer.translation.x) };
        }
        if !was_layered || trans_bits[1] != entry.trans_bits[1] {
            entry.trans_bits[1] = trans_bits[1];
            unsafe { (api.rn_set_translation_y)(node, layer.translation.y) };
        }

        let out_b = [
            out_rect.left.to_bits(),
            out_rect.top.to_bits(),
            out_rect.right.to_bits(),
            out_rect.bottom.to_bits(),
        ];
        let out_r = [
            out_radii[0].to_bits(),
            out_radii[1].to_bits(),
            out_radii[2].to_bits(),
            out_radii[3].to_bits(),
        ];
        let outline_changed = !was_layered
            || out_tag != entry.outline_tag
            || out_b != entry.outline_bounds_bits
            || out_r != entry.outline_radii_bits;
        if outline_changed {
            entry.outline_tag = out_tag;
            entry.outline_bounds_bits = out_b;
            entry.outline_radii_bits = out_r;
            entry.outline_clips = apply_outline_on_node(
                raw_env,
                api,
                node,
                out_tag,
                &out_rect,
                &out_radii,
                generic_path_ptr,
                generic_offset,
                layer.alpha.clamp(0.0, 1.0),
                outline_path_scratch,
                radii_jarray,
            );
        }

        let clip_wanted = layer.clip && out_tag != 0;
        if outline_changed || !was_layered || clip_wanted != entry.clip_bool {
            entry.clip_bool = clip_wanted;
            let by_path = clip_wanted && !entry.outline_clips;
            unsafe { (api.rn_set_clip_to_outline)(node, u8::from(clip_wanted && !by_path)) };
            if by_path || entry.clip_path != 0 {
                if entry.clip_path != 0 {
                    unsafe { (api.path_destroy)(entry.clip_path) };
                    entry.clip_path = 0;
                }
                if by_path {
                    let p = unsafe { (api.path_init)(raw_env, std::ptr::null_mut()) };
                    if out_tag == 3 {
                        if generic_path_ptr != 0 {
                            unsafe {
                                (api.path_set)(raw_env, std::ptr::null_mut(), p, generic_path_ptr);
                                if generic_offset.x != 0.0 || generic_offset.y != 0.0 {
                                    (api.path_offset)(
                                        raw_env,
                                        std::ptr::null_mut(),
                                        p,
                                        generic_offset.x,
                                        generic_offset.y,
                                    );
                                }
                            }
                        }
                    } else {
                        add_round_rect_to_path(
                            raw_env,
                            api,
                            p,
                            &out_rect,
                            &out_radii,
                            radii_jarray,
                            PATH_DIR_CCW,
                        );
                    }
                    entry.clip_path = p;
                }
                entry.version = u64::MAX;
            }
        }

        let elev_bits = layer.shadow_elevation.to_bits();
        if !was_layered || elev_bits != entry.elevation_bits {
            entry.elevation_bits = elev_bits;
            unsafe { (api.rn_set_elevation)(node, layer.shadow_elevation) };
            entry.elevated = layer.shadow_elevation > 0.0;
        }

        let amb = layer.ambient_shadow_color.argb();
        if !was_layered || amb != entry.ambient_argb {
            entry.ambient_argb = amb;
            unsafe { (api.rn_set_ambient_shadow_color)(node, amb as i32) };
        }

        let spot = layer.spot_shadow_color.argb();
        if !was_layered || spot != entry.spot_argb {
            entry.spot_argb = spot;
            unsafe { (api.rn_set_spot_shadow_color)(node, spot as i32) };
        }

        if !was_layered || comp != entry.compositing_mode {
            entry.compositing_mode = comp;
            match comp {
                COMP_OFFSCREEN => unsafe {
                    (api.rn_set_layer_type)(node, 2);
                    (api.rn_set_layer_paint)(node, 0);
                    (api.rn_set_overlapping)(node, 1);
                },
                COMP_MODULATE_ALPHA => unsafe {
                    (api.rn_set_layer_type)(node, 0);
                    (api.rn_set_layer_paint)(node, 0);
                    (api.rn_set_overlapping)(node, 0);
                },
                _ => unsafe {
                    (api.rn_set_layer_type)(node, 0);
                    (api.rn_set_layer_paint)(node, 0);
                    (api.rn_set_overlapping)(node, 1);
                },
            }
        }

        entry.layered = true;
    }

    fn compositing_of(
        &mut self,
        groups: &[DrawGroup],
        index: usize,
        layer: &LayerProperties,
    ) -> u8 {
        match layer.compositing_strategy {
            CompositingStrategy::Offscreen => return COMP_OFFSCREEN,
            CompositingStrategy::ModulateAlpha => return COMP_MODULATE_ALPHA,
            CompositingStrategy::Auto => {}
        }
        if !self.optimizations.contains(Optimization::SINGLE_DRAWING_FADE) || layer.alpha >= 1.0 {
            return COMP_AUTO;
        }
        let group = &groups[index];
        let nested_end = (index + 1 + group.descendants).min(groups.len());
        if groups[index + 1..nested_end].iter().any(|inner| !inner.empty) {
            return COMP_AUTO;
        }
        let single = if let Some(entry) = self.groups.get_mut(&group.key) {
            if entry.single_drawing_version == group.version {
                entry.single_drawing
            } else {
                let s = single_drawing(&group.content.ops).is_some();
                entry.single_drawing = s;
                entry.single_drawing_version = group.version;
                s
            }
        } else {
            single_drawing(&group.content.ops).is_some()
        };
        if single { COMP_MODULATE_ALPHA } else { COMP_AUTO }
    }

    fn record_single_group(
        &mut self,
        env: &mut Env,
        groups: &[DrawGroup],
        index: usize,
        ahead: bool,
    ) {
        let group = &groups[index];
        let key = group.key;
        let api = self.api;

        // Gather direct child boundary `(node_ptr_or_0, elevated)` tuples.
        let mut children = std::mem::take(&mut self.child_scratch);
        children.clear();
        let mut draws_frame_group = false;
        let mut child_idx = index + 1;
        for _ in &group.content.boundaries {
            let Some(child_group) = groups.get(child_idx) else { break };
            let pair = if child_group.empty {
                (0, false)
            } else if ahead {
                if let Some(frame_entry) = self.groups.get(&child_group.key) {
                    draws_frame_group = true;
                    (frame_entry.node, frame_entry.elevated)
                } else {
                    let entry = self.ensure_ahead_group(env.get_raw(), child_group.key);
                    (entry.node, entry.elevated)
                }
            } else {
                let entry = self.ensure_frame_group(env.get_raw(), child_group.key);
                (entry.node, entry.elevated)
            };
            children.push(pair);
            child_idx += 1 + child_group.descendants;
        }

        let existing_check = if ahead { self.ahead.get(&key) } else { self.groups.get(&key) };
        if let Some(existing) = existing_check {
            let valid = unsafe { (api.rn_is_valid)(existing.node) != 0 };
            if valid && existing.version == group.version {
                self.child_scratch = children;
                return;
            }
            // Fast skip when only a nested descendant bumped `group.version` without changing
            // `group.content` or any direct child boundary's `(node, elevated)` state.
            if valid
                && existing.version != u64::MAX
                && existing.content_gen == group.content.content_generation
                && existing.recorded_children == children
                && !self.any_path_redefined_since(&group.content.ops, existing.recorded_path_serial)
            {
                if !draws_frame_group {
                    let map = if ahead { &mut self.ahead } else { &mut self.groups };
                    if let Some(mut_entry) = map.get_mut(&key) {
                        mut_entry.version = group.version;
                    }
                }
                self.child_scratch = children;
                return;
            }
        }

        let (node_ptr, rec_w, rec_h, clip_path) = {
            let width = self.width;
            let height = self.height;
            let entry = if ahead {
                self.ensure_ahead_group(env.get_raw(), key)
            } else {
                self.ensure_frame_group(env.get_raw(), key)
            };
            entry.version = u64::MAX;
            if !entry.layered {
                entry.set_position(api, width, height);
            }
            entry.recorded_children.clear();
            entry.recorded_children.extend_from_slice(&children);
            (entry.node, entry.width.max(1), entry.height.max(1), entry.clip_path)
        };
        self.child_scratch = children;

        let saved_serial_max = self.named_path_serial_max;
        self.named_path_serial_max = 0;

        let canvas = self.canvas_pool.begin(api, node_ptr, rec_w, rec_h);
        if clip_path != 0 {
            unsafe { (api.canvas_clip_path)(canvas, clip_path, CLIP_OP_INTERSECT) };
        }
        self.record_display_list_ops(env, canvas, &group.content, groups, index + 1, false, ahead);
        self.canvas_pool.end(api, canvas, node_ptr);

        let used_serial = self.named_path_serial_max.max(self.path_serial);
        self.named_path_serial_max = saved_serial_max;

        let entry = if ahead { self.ahead.get_mut(&key) } else { self.groups.get_mut(&key) };
        if let Some(entry) = entry {
            entry.content_gen = group.content.content_generation;
            entry.recorded_path_serial = used_serial;
            if entry.single_drawing_version != group.version {
                entry.single_drawing = single_drawing(&group.content.ops).is_some();
                entry.single_drawing_version = group.version;
            }
            if !draws_frame_group {
                entry.version = group.version;
            }
        }
    }

    fn any_path_redefined_since(&self, ops: &[DrawOp], recorded_serial: u64) -> bool {
        if self.path_serial <= recorded_serial {
            return false;
        }
        for op in ops {
            let path = match op {
                DrawOp::DrawPath { path, .. } | DrawOp::ClipPath { path, .. } => path,
                _ => continue,
            };
            match self.paths.get(&(path.id(), path.generation())) {
                Some(kept) if kept.defined_serial <= recorded_serial => {}
                _ => return true,
            }
        }
        false
    }

    #[allow(clippy::too_many_arguments)]
    fn record_display_list_ops(
        &mut self,
        env: &mut Env,
        root_canvas: i64,
        recording: &DisplayList,
        groups: &[DrawGroup],
        mut child: usize,
        is_root: bool,
        ahead: bool,
    ) {
        let api = self.api;
        let ops = &recording.ops;
        let boundaries = &recording.boundaries;
        let allow_chunking = is_root && !ahead && ops.len() > 420;

        let mut active_canvas = root_canvas;
        let mut start_save_count = unsafe { (api.canvas_get_save_count)(active_canvas) };
        let mut chunk_node: i64 = 0;
        let mut chunk_ops = 0usize;

        let saved_folds = std::mem::take(&mut self.folds);
        self.folds.clear();

        let mut boundary_idx = 0usize;
        for (op_idx, op) in ops.iter().enumerate() {
            while boundaries.get(boundary_idx).is_some_and(|b| b.at <= op_idx) {
                if chunk_node != 0 {
                    unsafe { (api.canvas_restore_to_count)(active_canvas, start_save_count) };
                    self.canvas_pool.end(api, active_canvas, chunk_node);
                    unsafe { (api.dlc_draw_render_node)(root_canvas, chunk_node) };
                    chunk_node = 0;
                    active_canvas = root_canvas;
                    start_save_count = unsafe { (api.canvas_get_save_count)(active_canvas) };
                    chunk_ops = 0;
                }
                child = self.emit_group_ref(
                    env.get_raw(),
                    active_canvas,
                    groups,
                    boundaries[boundary_idx].key,
                    child,
                    ahead,
                );
                boundary_idx += 1;
            }

            while self.folds.last().is_some_and(|&(end, _)| end < op_idx) {
                self.folds.pop();
            }

            if chunk_node != 0
                && chunk_ops >= 200
                && unsafe { (api.canvas_get_save_count)(active_canvas) } == start_save_count
            {
                self.canvas_pool.end(api, active_canvas, chunk_node);
                unsafe { (api.dlc_draw_render_node)(root_canvas, chunk_node) };
                if ops.len() - op_idx > 100 {
                    let next = self.acquire_chunk_node(env.get_raw());
                    chunk_node = next;
                    active_canvas = self.canvas_pool.begin(api, next, self.width, self.height);
                    start_save_count = unsafe { (api.canvas_get_save_count)(active_canvas) };
                    chunk_ops = 0;
                } else {
                    chunk_node = 0;
                    active_canvas = root_canvas;
                    start_save_count = unsafe { (api.canvas_get_save_count)(active_canvas) };
                    chunk_ops = 0;
                }
            } else if chunk_node == 0
                && allow_chunking
                && ops.len() - op_idx > 100
                && is_chunkable_draw_op(op)
                && unsafe { (api.canvas_get_save_count)(active_canvas) } == start_save_count
            {
                let next = self.acquire_chunk_node(env.get_raw());
                chunk_node = next;
                active_canvas = self.canvas_pool.begin(api, next, self.width, self.height);
                start_save_count = unsafe { (api.canvas_get_save_count)(active_canvas) };
                chunk_ops = 0;
            }

            if chunk_node != 0 {
                chunk_ops += 1;
            }

            if let DrawOp::SaveLayerAlpha { bounds, alpha } = *op
                && self.optimizations.contains(Optimization::SINGLE_DRAWING_FADE)
                && let Some(end) = foldable_layer(ops, op_idx, bounds)
                && !draws_group_before(&boundaries[boundary_idx..], end, groups, child)
            {
                self.folds.push((end, alpha));
                unsafe { (api.canvas_save)(active_canvas, SAVE_MATRIX_CLIP) };
                continue;
            }

            let fold_alpha: f32 = if self.folds.is_empty() {
                1.0
            } else {
                self.folds.iter().map(|&(_, a)| a).product()
            };

            self.emit_draw_op(
                env,
                active_canvas,
                start_save_count,
                op,
                fold_alpha,
                &recording.shaders,
            );
        }

        for rest in &boundaries[boundary_idx..] {
            if chunk_node != 0 {
                unsafe { (api.canvas_restore_to_count)(active_canvas, start_save_count) };
                self.canvas_pool.end(api, active_canvas, chunk_node);
                unsafe { (api.dlc_draw_render_node)(root_canvas, chunk_node) };
                chunk_node = 0;
                active_canvas = root_canvas;
                start_save_count = unsafe { (api.canvas_get_save_count)(active_canvas) };
            }
            child =
                self.emit_group_ref(env.get_raw(), active_canvas, groups, rest.key, child, ahead);
        }

        unsafe { (api.canvas_restore_to_count)(active_canvas, start_save_count) };
        if chunk_node != 0 {
            self.canvas_pool.end(api, active_canvas, chunk_node);
            unsafe { (api.dlc_draw_render_node)(root_canvas, chunk_node) };
        }

        self.folds = saved_folds;
    }

    fn emit_group_ref(
        &mut self,
        raw_env: *mut JNIEnv,
        canvas: i64,
        groups: &[DrawGroup],
        key: u64,
        child: usize,
        ahead: bool,
    ) -> usize {
        let Some(group) = groups.get(child) else { return child };
        debug_assert_eq!(group.key, key);
        if !group.empty {
            let (node, elevated) = if ahead {
                if let Some(frame_entry) = self.groups.get(&key) {
                    (frame_entry.node, frame_entry.elevated)
                } else {
                    let e = self.ensure_ahead_group(raw_env, key);
                    (e.node, e.elevated)
                }
            } else {
                let e = self.ensure_frame_group(raw_env, key);
                (e.node, e.elevated)
            };
            let api = self.api;
            if elevated {
                unsafe {
                    (api.dlc_enable_z)(canvas, 1);
                    (api.dlc_draw_render_node)(canvas, node);
                    (api.dlc_enable_z)(canvas, 0);
                }
            } else {
                unsafe { (api.dlc_draw_render_node)(canvas, node) };
            }
        }
        child + 1 + group.descendants
    }

    fn acquire_chunk_node(&mut self, raw_env: *mut JNIEnv) -> i64 {
        let idx = self.chunk_used;
        self.chunk_used += 1;
        let api = self.api;
        let node = if let Some(&existing) = self.chunk_nodes.get(idx) {
            existing
        } else {
            let created = create_render_node(raw_env, api);
            unsafe {
                (api.rn_set_clip_to_bounds)(created, 0);
                (api.rn_set_overlapping)(created, 0);
            }
            self.chunk_nodes.push(created);
            created
        };
        unsafe { (api.rn_set_ltrb)(node, 0, 0, self.width, self.height) };
        node
    }

    fn emit_draw_op(
        &mut self,
        env: &mut Env,
        canvas: i64,
        start_save_count: i32,
        op: &DrawOp,
        fold_alpha: f32,
        shaders: &[Shader],
    ) {
        let api = self.api;
        let raw = env.get_raw();
        let null_obj: jobject = std::ptr::null_mut();

        match op {
            DrawOp::Save => unsafe {
                (api.canvas_save)(canvas, SAVE_MATRIX_CLIP);
            },
            DrawOp::Restore => unsafe {
                if (api.canvas_get_save_count)(canvas) > start_save_count {
                    (api.canvas_restore)(canvas);
                }
            },
            DrawOp::Translate(offset) => unsafe {
                (api.canvas_translate)(canvas, offset.x, offset.y);
            },
            DrawOp::ClipRect(rect) => unsafe {
                (api.canvas_clip_rect)(
                    canvas,
                    rect.left,
                    rect.top,
                    rect.right,
                    rect.bottom,
                    CLIP_OP_INTERSECT,
                );
            },
            DrawOp::ClipRoundRect(rect, radii) => {
                unsafe { (api.path_reset)(self.scratch_path) };
                add_round_rect_to_path(
                    raw,
                    api,
                    self.scratch_path,
                    rect,
                    radii,
                    self.radii_array.as_raw() as jfloatArray,
                    PATH_DIR_CW,
                );
                unsafe { (api.canvas_clip_path)(canvas, self.scratch_path, CLIP_OP_INTERSECT) };
            }
            DrawOp::ClipPath { path, offset } => {
                let p = self.ensure_native_path(raw, path);
                if p == 0 {
                    unsafe {
                        (api.canvas_clip_rect)(canvas, 0.0, 0.0, 0.0, 0.0, CLIP_OP_INTERSECT);
                    }
                } else if offset.x == 0.0 && offset.y == 0.0 {
                    unsafe { (api.canvas_clip_path)(canvas, p, CLIP_OP_INTERSECT) };
                } else {
                    unsafe {
                        (api.path_set)(raw, std::ptr::null_mut(), self.scratch_path, p);
                        (api.path_offset)(
                            raw,
                            std::ptr::null_mut(),
                            self.scratch_path,
                            offset.x,
                            offset.y,
                        );
                        (api.canvas_clip_path)(canvas, self.scratch_path, CLIP_OP_INTERSECT);
                    }
                }
            }
            DrawOp::DrawRect(rect, paint) => {
                let p = self.resolve_paint(env, paint, shaders, fold_alpha, 0.0, 0.0, 1.0, 1.0);
                unsafe {
                    (api.canvas_draw_rect)(
                        raw,
                        null_obj,
                        canvas,
                        rect.left,
                        rect.top,
                        rect.right,
                        rect.bottom,
                        p,
                    );
                }
            }
            DrawOp::DrawRoundRect(rect, radii, paint) => {
                let p = self.resolve_paint(env, paint, shaders, fold_alpha, 0.0, 0.0, 1.0, 1.0);
                let r0 = radii[0];
                if r0 == radii[1] && r0 == radii[2] && r0 == radii[3] {
                    unsafe {
                        (api.canvas_draw_round_rect)(
                            raw,
                            null_obj,
                            canvas,
                            rect.left,
                            rect.top,
                            rect.right,
                            rect.bottom,
                            r0,
                            r0,
                            p,
                        );
                    }
                } else {
                    unsafe { (api.path_reset)(self.scratch_path) };
                    add_round_rect_to_path(
                        raw,
                        api,
                        self.scratch_path,
                        rect,
                        radii,
                        self.radii_array.as_raw() as jfloatArray,
                        PATH_DIR_CW,
                    );
                    unsafe {
                        (api.canvas_draw_path)(raw, null_obj, canvas, self.scratch_path, p);
                    }
                }
            }
            DrawOp::DrawLine(from, to, paint) => {
                let p = self.resolve_paint(env, paint, shaders, fold_alpha, 0.0, 0.0, 1.0, 1.0);
                unsafe {
                    (api.canvas_draw_line)(raw, null_obj, canvas, from.x, from.y, to.x, to.y, p);
                }
            }
            DrawOp::DrawParagraph(handle, offset) => {
                self.emit_draw_paragraph(env, canvas, handle.0 as u32, offset.x, offset.y);
            }
            DrawOp::SaveLayerAlpha { bounds, alpha } => {
                let a = (alpha.clamp(0.0, 1.0) * 255.0 + 0.5) as i32;
                unsafe {
                    (api.canvas_save_layer_alpha)(
                        canvas,
                        bounds.left,
                        bounds.top,
                        bounds.right,
                        bounds.bottom,
                        a,
                    );
                }
            }
            DrawOp::Transform { pivot, scale, rotation, translation } => unsafe {
                (api.canvas_translate)(canvas, pivot.x + translation.x, pivot.y + translation.y);
                (api.canvas_rotate)(canvas, *rotation);
                (api.canvas_scale)(canvas, scale.0, scale.1);
                (api.canvas_translate)(canvas, -pivot.x, -pivot.y);
            },
            DrawOp::DrawImage { image, src, dst, alpha, tint } => {
                let bmp = self.ensure_native_bitmap(env, image);
                if bmp != 0 {
                    let tint_argb = tint.map_or(0, |c| c.argb());
                    if tint_argb != self.bitmap_tint {
                        self.bitmap_tint = tint_argb;
                        let cf_local = if tint_argb == 0 {
                            std::ptr::null_mut()
                        } else {
                            let cf_args = [
                                jvalue { i: tint_argb as i32 },
                                jvalue { l: self.pd_src_in.as_raw() },
                            ];
                            unsafe {
                                ((**raw).v1_2.NewObjectA)(
                                    raw,
                                    self.pd_cf_class.as_raw(),
                                    self.pd_cf_ctor,
                                    cf_args.as_ptr(),
                                )
                            }
                        };
                        let set_args = [jvalue { l: cf_local }];
                        let prev_cf = unsafe {
                            ((**raw).v1_2.CallObjectMethodA)(
                                raw,
                                self.bitmap_paint_obj.as_raw(),
                                self.bitmap_paint_set_cf,
                                set_args.as_ptr(),
                            )
                        };
                        if !prev_cf.is_null() {
                            unsafe { ((**raw).v1_2.DeleteLocalRef)(raw, prev_cf) };
                        }
                        if !cf_local.is_null() {
                            unsafe { ((**raw).v1_2.DeleteLocalRef)(raw, cf_local) };
                        }
                        let _ = crate::hwui::clear_exception_check(raw);
                        self.bitmap_paint = unsafe {
                            ((**raw).v1_2.CallLongMethodA)(
                                raw,
                                self.bitmap_paint_obj.as_raw(),
                                api.paint_get_native_instance,
                                std::ptr::null(),
                            )
                        };
                    }
                    let a = (alpha.clamp(0.0, 1.0) * 255.0 + 0.5) as i32;
                    if a != self.bitmap_alpha {
                        self.bitmap_alpha = a;
                        unsafe { (api.paint_set_alpha)(self.bitmap_paint, a) };
                    }
                    unsafe {
                        (api.canvas_draw_bitmap_rect)(
                            raw,
                            null_obj,
                            canvas,
                            bmp,
                            (src.left as i32) as f32,
                            (src.top as i32) as f32,
                            (src.right as i32) as f32,
                            (src.bottom as i32) as f32,
                            dst.left,
                            dst.top,
                            dst.right,
                            dst.bottom,
                            self.bitmap_paint,
                            0,
                            0,
                        );
                    }
                }
            }
            DrawOp::DrawShadow { .. } => {
                // Hardware shadows are cast by `RenderNode` elevation (`set_layer`).
            }
            DrawOp::DrawArc { oval, start_angle, sweep_angle, use_center, paint } => {
                let p = self.resolve_paint(env, paint, shaders, fold_alpha, 0.0, 0.0, 1.0, 1.0);
                let w = oval.right - oval.left;
                let h = oval.bottom - oval.top;
                if !*use_center && *start_angle == 0.0 && *sweep_angle == 360.0 {
                    if (w - h).abs() <= 1e-3 {
                        unsafe {
                            (api.canvas_draw_circle)(
                                raw,
                                null_obj,
                                canvas,
                                (oval.left + oval.right) * 0.5,
                                (oval.top + oval.bottom) * 0.5,
                                w * 0.5,
                                p,
                            );
                        }
                    } else {
                        unsafe {
                            (api.canvas_draw_oval)(
                                raw,
                                null_obj,
                                canvas,
                                oval.left,
                                oval.top,
                                oval.right,
                                oval.bottom,
                                p,
                            );
                        }
                    }
                } else {
                    unsafe {
                        (api.canvas_draw_arc)(
                            raw,
                            null_obj,
                            canvas,
                            oval.left,
                            oval.top,
                            oval.right,
                            oval.bottom,
                            *start_angle,
                            *sweep_angle,
                            u8::from(*use_center),
                            p,
                        );
                    }
                }
            }
            DrawOp::DrawPath { path, offset, scale, paint } => {
                let p_native = self.ensure_native_path(raw, path);
                if p_native != 0 {
                    let paint_ptr = self.resolve_paint(
                        env, paint, shaders, fold_alpha, offset.x, offset.y, scale.0, scale.1,
                    );
                    unsafe {
                        (api.canvas_save)(canvas, SAVE_MATRIX_CLIP);
                        (api.canvas_translate)(canvas, offset.x, offset.y);
                        (api.canvas_scale)(canvas, scale.0, scale.1);
                        (api.canvas_draw_path)(raw, null_obj, canvas, p_native, paint_ptr);
                        (api.canvas_restore)(canvas);
                    }
                }
            }
        }
    }

    fn emit_draw_paragraph(&mut self, env: &mut Env, canvas: i64, handle: u32, x: f32, y: f32) {
        let paragraph_nodes = self.optimizations.contains(Optimization::PARAGRAPH_RENDER_NODES);
        let api = self.api;
        let raw = env.get_raw();

        enum Action {
            Missing,
            DrawSimpleInline { paint_ptr: i64, baseline: f32, len: usize },
            DrawNativeComplexInline { plan: Rc<ComplexParagraphDraw> },
            DrawJniComplexInline,
            DrawNode { node: i64 },
            RecordThenDrawNode,
        }

        let action = {
            let mut table = self.paragraphs.borrow_mut();
            match table.get_mut(handle) {
                None => Action::Missing,
                Some(slot) => {
                    if !paragraph_nodes && !slot.drawn_once {
                        slot.drawn_once = true;
                        if slot.paint_ptr != 0 {
                            let chars = slot.chars();
                            if chars.is_empty() {
                                return;
                            }
                            upload_chars_to_array(
                                env,
                                &mut self.char_array,
                                &mut self.char_array_cap,
                                chars,
                            );
                            Action::DrawSimpleInline {
                                paint_ptr: slot.paint_ptr,
                                baseline: slot.baseline,
                                len: chars.len(),
                            }
                        } else if let Some(plan) = slot.complex_draw.clone() {
                            Action::DrawNativeComplexInline { plan }
                        } else if slot.native_owned {
                            return;
                        } else {
                            Action::DrawJniComplexInline
                        }
                    } else if slot.node_ptr != 0 && unsafe { (api.rn_is_valid)(slot.node_ptr) != 0 }
                    {
                        Action::DrawNode { node: slot.node_ptr }
                    } else if slot.native_owned
                        && slot.paint_ptr == 0
                        && slot.complex_draw.is_none()
                    {
                        return;
                    } else {
                        Action::RecordThenDrawNode
                    }
                }
            }
        };

        match action {
            Action::Missing => {
                // Fallback if a paragraph wasn't registered in `SharedParagraphTable`.
                self.call_draw_layout_direct(env, canvas, handle, x, y);
            }
            Action::DrawSimpleInline { paint_ptr, baseline, len } => unsafe {
                (api.canvas_draw_text_chars)(
                    raw,
                    std::ptr::null_mut(),
                    canvas,
                    self.char_array.as_raw() as jcharArray,
                    0,
                    len as i32,
                    x,
                    y + baseline,
                    BIDI_DEFAULT_LTR,
                    paint_ptr,
                );
            },
            Action::DrawNativeComplexInline { plan } => {
                self.draw_complex_paragraph(env, canvas, &plan, x, y);
            }
            Action::DrawJniComplexInline => {
                self.call_draw_layout_direct(env, canvas, handle, x, y);
            }
            Action::DrawNode { node } => {
                draw_node_at(api, canvas, node, x, y);
            }
            Action::RecordThenDrawNode => {
                let node = self.ensure_paragraph_node(env, handle);
                if node != 0 {
                    draw_node_at(api, canvas, node, x, y);
                }
            }
        }
    }

    /// Draws native-owned paragraph `handle` onto a Java `Canvas` (hardware or software) when
    /// `Replayer.kt` falls back to `TextMeasurer.draw`. Returns `false` for `JniText` paragraphs.
    pub fn draw_paragraph_to_java_canvas(
        &mut self,
        env: &mut Env,
        canvas_obj: &JObject,
        hardware: bool,
        handle: u32,
        x: f32,
        y: f32,
    ) -> bool {
        let raw = env.get_raw();
        if hardware {
            let is_native = {
                let table = self.paragraphs.borrow();
                let Some(slot) = table.get(handle) else { return false };
                slot.native_owned
            };
            if !is_native {
                return false;
            }
            let canvas_ptr =
                unsafe { (self.api.acanvas_from_java)(raw, canvas_obj.as_raw()) } as i64;
            if canvas_ptr != 0 {
                self.emit_draw_paragraph(env, canvas_ptr, handle, x, y);
                return true;
            }
        }

        enum SoftDraw {
            Empty,
            Simple { paint_ptr: i64, baseline: f32, len: usize },
            Complex(Rc<ComplexParagraphDraw>),
        }

        let soft = {
            let table = self.paragraphs.borrow();
            let Some(slot) = table.get(handle) else { return false };
            if !slot.native_owned {
                return false;
            }
            if slot.paint_ptr != 0 {
                let chars = slot.chars();
                if chars.is_empty() {
                    SoftDraw::Empty
                } else {
                    upload_chars_to_array(
                        env,
                        &mut self.char_array,
                        &mut self.char_array_cap,
                        chars,
                    );
                    SoftDraw::Simple {
                        paint_ptr: slot.paint_ptr,
                        baseline: slot.baseline,
                        len: chars.len(),
                    }
                }
            } else if let Some(plan) = slot.complex_draw.clone() {
                SoftDraw::Complex(plan)
            } else {
                SoftDraw::Empty
            }
        };

        let canvas_ptr = unsafe {
            ((**raw).v1_2.GetLongField)(raw, canvas_obj.as_raw(), self.api.canvas_wrapper_field)
        };
        if canvas_ptr == 0 {
            return true;
        }
        match soft {
            SoftDraw::Empty => {}
            SoftDraw::Simple { paint_ptr, baseline, len } => unsafe {
                (self.api.canvas_draw_text_chars)(
                    raw,
                    std::ptr::null_mut(),
                    canvas_ptr,
                    self.char_array.as_raw() as jcharArray,
                    0,
                    len as i32,
                    x,
                    y + baseline,
                    BIDI_DEFAULT_LTR,
                    paint_ptr,
                );
            },
            SoftDraw::Complex(plan) => {
                self.draw_complex_paragraph(env, canvas_ptr, &plan, x, y);
            }
        }
        true
    }

    fn ensure_paragraph_node(&mut self, env: &mut Env, handle: u32) -> i64 {
        let api = self.api;
        let raw = env.get_raw();

        enum ParaSource {
            Simple { paint_ptr: i64, baseline: f32, char_len: usize },
            NativeComplex(Rc<ComplexParagraphDraw>),
            JniFallback,
        }

        let (existing_node, width, height, source) = {
            let table = self.paragraphs.borrow();
            let Some(slot) = table.get(handle) else { return 0 };
            if slot.paint_ptr != 0 {
                let chars = slot.chars();
                if chars.is_empty() {
                    return 0;
                }
                upload_chars_to_array(env, &mut self.char_array, &mut self.char_array_cap, chars);
                (
                    slot.node_ptr,
                    slot.width,
                    slot.height,
                    ParaSource::Simple {
                        paint_ptr: slot.paint_ptr,
                        baseline: slot.baseline,
                        char_len: chars.len(),
                    },
                )
            } else if let Some(plan) = slot.complex_draw.clone() {
                if plan.runs.is_empty() {
                    return 0;
                }
                (slot.node_ptr, slot.width, slot.height, ParaSource::NativeComplex(plan))
            } else if slot.native_owned {
                return 0;
            } else {
                (slot.node_ptr, slot.width, slot.height, ParaSource::JniFallback)
            }
        };

        let node = if existing_node != 0 {
            existing_node
        } else {
            obtain_para_node(raw, api, &mut self.para_node_pool)
        };
        let w = width.max(1);
        let h = height.max(1);
        unsafe {
            (api.rn_set_ltrb)(node, 0, 0, w, h);
            (api.rn_set_clip_to_bounds)(node, 0);
        }
        let rec_canvas = self.canvas_pool.begin(api, node, w, h);
        match source {
            ParaSource::Simple { paint_ptr, baseline, char_len } => unsafe {
                (api.canvas_draw_text_chars)(
                    raw,
                    std::ptr::null_mut(),
                    rec_canvas,
                    self.char_array.as_raw() as jcharArray,
                    0,
                    char_len as i32,
                    0.0,
                    baseline,
                    BIDI_DEFAULT_LTR,
                    paint_ptr,
                );
            },
            ParaSource::NativeComplex(plan) => {
                self.draw_complex_paragraph(env, rec_canvas, &plan, 0.0, 0.0);
            }
            ParaSource::JniFallback => {
                self.call_draw_layout_direct(env, rec_canvas, handle, 0.0, 0.0);
            }
        }
        self.canvas_pool.end(api, rec_canvas, node);

        if let Some(slot) = self.paragraphs.borrow_mut().get_mut(handle) {
            slot.node_ptr = node;
            slot.drawn_once = true;
        }
        node
    }

    fn upload_exact_char_array(&mut self, env: &mut Env, chars: &[u16]) -> jcharArray {
        let len = chars.len();
        let raw = env.get_raw();
        if !self.exact_char_arrays.contains_key(&len) {
            if self.exact_char_arrays.len() >= EXACT_CHAR_ARRAY_CAP {
                self.exact_char_arrays.clear();
            }
            let arr = unsafe { ((**raw).v1_2.NewCharArray)(raw, len as jint) };
            if arr.is_null() || crate::hwui::clear_exception_check(raw) {
                return std::ptr::null_mut();
            }
            let local = unsafe { JObject::from_raw(env, arr as jobject) };
            if let Ok(global) = env.new_global_ref(&local) {
                self.exact_char_arrays.insert(len, global);
            }
            env.delete_local_ref(local);
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

    fn draw_complex_paragraph(
        &mut self,
        env: &mut Env,
        canvas: i64,
        plan: &ComplexParagraphDraw,
        x: f32,
        y: f32,
    ) {
        let Some(text_api) = self.api.text.as_ref() else { return };
        let api = self.api;
        let raw = env.get_raw();
        let mut last_uploaded: Option<(u32, bool, jcharArray)> = None;

        for run in &plan.runs {
            if let Some(bg) = run.background {
                let bg_paint = self.resolve_solid_fill_paint(raw, bg.color);
                unsafe {
                    (api.canvas_draw_rect)(
                        raw,
                        std::ptr::null_mut(),
                        canvas,
                        x + bg.left,
                        y + bg.top,
                        x + bg.right,
                        y + bg.bottom,
                        bg_paint,
                    );
                }
            }

            if run.count > 0 && run.paint_ptr != 0 {
                let need_exact = run.mt_ptr != 0;
                let jarr = match last_uploaded {
                    Some((b_idx, exact, cached))
                        if b_idx == run.buffer_index && exact == need_exact =>
                    {
                        cached
                    }
                    _ => {
                        let Some(buf) = plan.buffers.get(run.buffer_index as usize) else {
                            continue;
                        };
                        let arr = if need_exact {
                            self.upload_exact_char_array(env, buf)
                        } else {
                            upload_chars_to_array(
                                env,
                                &mut self.char_array,
                                &mut self.char_array_cap,
                                buf,
                            );
                            self.char_array.as_raw() as jcharArray
                        };
                        last_uploaded = Some((run.buffer_index, need_exact, arr));
                        arr
                    }
                };
                if !jarr.is_null() {
                    let prev_flags = unsafe { (text_api.paint_get_flags)(run.paint_ptr) };
                    let wanted_flags =
                        (prev_flags & !RUN_EDGE_FLAGS_MASK) | (run.run_flags & RUN_EDGE_FLAGS_MASK);
                    if wanted_flags != prev_flags {
                        unsafe { (api.paint_set_flags)(run.paint_ptr, wanted_flags) };
                    }
                    if run.word_spacing_px != 0.0 {
                        unsafe {
                            (text_api.paint_set_word_spacing)(run.paint_ptr, run.word_spacing_px);
                        }
                    }
                    unsafe {
                        (text_api.canvas_draw_text_run_chars)(
                            raw,
                            std::ptr::null_mut(),
                            canvas,
                            jarr,
                            run.start,
                            run.count,
                            run.context_start,
                            run.context_count,
                            x + run.x,
                            y + run.baseline_y,
                            u8::from(run.is_rtl),
                            run.paint_ptr,
                            run.mt_ptr,
                        );
                    }
                    if run.word_spacing_px != 0.0 {
                        unsafe { (text_api.paint_set_word_spacing)(run.paint_ptr, 0.0) };
                    }
                    if wanted_flags != prev_flags {
                        unsafe { (api.paint_set_flags)(run.paint_ptr, prev_flags) };
                    }
                }
            }

            let dec_start = run.decorations_range.start as usize;
            let dec_end = (run.decorations_range.end as usize).min(plan.decorations.len());
            for dec in &plan.decorations[dec_start..dec_end] {
                let dec_paint = self.resolve_solid_fill_paint(raw, dec.color);
                unsafe {
                    (api.canvas_draw_rect)(
                        raw,
                        std::ptr::null_mut(),
                        canvas,
                        x + dec.left,
                        y + dec.top,
                        x + dec.right,
                        y + dec.bottom,
                        dec_paint,
                    );
                }
            }
        }
        let _ = crate::hwui::clear_exception_check(raw);
    }

    fn call_draw_layout_direct(&self, env: &mut Env, canvas_ptr: i64, handle: u32, x: f32, y: f32) {
        let raw = env.get_raw();
        let proxy_raw = self.canvas_proxy.as_raw();
        let field = self.api.canvas_wrapper_field;
        let prev = unsafe { ((**raw).v1_2.GetLongField)(raw, proxy_raw, field) };
        unsafe { ((**raw).v1_2.SetLongField)(raw, proxy_raw, field, canvas_ptr) };
        let _ = unsafe {
            env.call_method_unchecked(
                &*self.measurer,
                self.draw_layout_direct,
                ReturnType::Primitive(Primitive::Void),
                &[
                    JValue::Object(&self.canvas_proxy).as_jni(),
                    JValue::Int(handle as jint).as_jni(),
                    JValue::Float(x).as_jni(),
                    JValue::Float(y).as_jni(),
                ],
            )
        };
        unsafe { ((**raw).v1_2.SetLongField)(raw, proxy_raw, field, prev) };
        check(env, "TextMeasurer.drawLayoutDirect");
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_paint(
        &mut self,
        env: &mut Env,
        paint: &Paint,
        shaders: &[Shader],
        fold_alpha: f32,
        local_x: f32,
        local_y: f32,
        scale_x: f32,
        scale_y: f32,
    ) -> i64 {
        let raw = env.get_raw();
        // Fast path: solid fill paint with Butt cap -> 64-slot native Paint* cache (0 JNI/C++ calls on hit!).
        if paint.shader().is_none()
            && matches!(paint.style, PaintStyle::Fill)
            && matches!(paint.cap, StrokeCap::Butt)
        {
            let argb = if fold_alpha >= 1.0 {
                paint.color.argb()
            } else {
                faded(paint.color, fold_alpha).argb()
            };
            return self.resolve_solid_fill_paint(raw, argb);
        }

        let resolved_shading = paint
            .shader()
            .and_then(|idx| shaders.get(idx.0 as usize))
            .map_or(Shading::Color, shading);

        let (shader_ptr, final_argb) = match resolved_shading {
            Shading::Gradient(shader) => {
                let s_ptr = self.resolve_shader(env, shader, local_x, local_y, scale_x, scale_y);
                if s_ptr != 0 {
                    let eff_alpha = (shader.alpha() * fold_alpha.clamp(0.0, 1.0)).clamp(0.0, 1.0);
                    let a_byte = ((eff_alpha * 255.0 + 0.5) as u32).min(255);
                    (s_ptr, a_byte << 24)
                } else {
                    (0, faded(paint.color, fold_alpha).argb())
                }
            }
            Shading::Solid(solid_col, opacity) => {
                let c = solid_col
                    .with_alpha(f32::from(solid_col.alpha()) / 255.0 * opacity.clamp(0.0, 1.0));
                (0, faded(c, fold_alpha).argb())
            }
            Shading::Color => (0, faded(paint.color, fold_alpha).argb()),
        };

        let (is_stroke, stroke_w) = match paint.style {
            PaintStyle::Fill => (false, 0.0f32),
            PaintStyle::Stroke(w) => (true, w),
        };
        let cap_code = match paint.cap {
            StrokeCap::Butt => 0u8,
            StrokeCap::Round => 1u8,
            StrokeCap::Square => 2u8,
        };

        let api = self.api;
        let gp = self.general_paint;
        if shader_ptr != self.gen_shader {
            self.gen_shader = shader_ptr;
            unsafe { (api.paint_set_shader)(gp, shader_ptr) };
        }
        if final_argb != self.gen_color {
            self.gen_color = final_argb;
            unsafe { (api.paint_set_color)(gp, final_argb as i32) };
        }
        if is_stroke != self.gen_stroke {
            self.gen_stroke = is_stroke;
            unsafe { (api.paint_set_style)(gp, i32::from(is_stroke)) };
        }
        if stroke_w.to_bits() != self.gen_width.to_bits() {
            self.gen_width = stroke_w;
            unsafe { (api.paint_set_stroke_width)(gp, stroke_w) };
        }
        if cap_code != self.gen_cap {
            self.gen_cap = cap_code;
            unsafe { (api.paint_set_stroke_cap)(gp, i32::from(cap_code)) };
        }
        gp
    }

    #[inline]
    fn resolve_solid_fill_paint(&mut self, raw: *mut JNIEnv, argb: u32) -> i64 {
        let slot =
            (((argb ^ (argb >> 16)).wrapping_mul(0x045d_9f3b)) >> 26) as usize & FILL_PAINT_MASK;
        if self.fill_paint_valid[slot] && self.fill_paint_colors[slot] == argb {
            return self.fill_paint_ptrs[slot];
        }
        let mut ptr = self.fill_paint_ptrs[slot];
        if ptr == 0 {
            ptr = create_default_paint(raw, self.api);
            self.fill_paint_ptrs[slot] = ptr;
        }
        unsafe { (self.api.paint_set_color)(ptr, argb as i32) };
        self.fill_paint_colors[slot] = argb;
        self.fill_paint_valid[slot] = true;
        ptr
    }

    fn resolve_shader(
        &mut self,
        env: &mut Env,
        shader: &Shader,
        local_x: f32,
        local_y: f32,
        scale_x: f32,
        scale_y: f32,
    ) -> i64 {
        let raw = env.get_raw();
        let api = self.api;
        let gradient = shader.gradient();
        let tile_mode: i32 = match gradient.tile_mode {
            TileMode::Clamp => 0,
            TileMode::Repeated => 1,
            TileMode::Mirror => 2,
            TileMode::Decal => {
                if api.sdk_int >= 31 {
                    3
                } else {
                    0
                }
            }
        };

        let (kind, geom_bits) = match shader {
            Shader::LinearGradient { from, to, .. } => {
                (1u8, [from.x.to_bits(), from.y.to_bits(), to.x.to_bits(), to.y.to_bits()])
            }
            Shader::RadialGradient { center, radius, .. } => {
                (2u8, [center.x.to_bits(), center.y.to_bits(), radius.to_bits(), 0])
            }
            Shader::SweepGradient { center, .. } => {
                (3u8, [center.x.to_bits(), center.y.to_bits(), 0, 0])
            }
        };

        let raw_colors = &gradient.colors;
        let count = raw_colors.len().max(2);
        let mut colors_vec = Vec::with_capacity(count);
        for idx in 0..count {
            colors_vec.push(raw_colors[idx.min(raw_colors.len() - 1)].argb());
        }
        let stops_slice = gradient.stops.as_deref().filter(|s| s.len() == count);
        let stops_bits: Vec<u32> = match stops_slice {
            Some(s) => s.iter().map(|v| v.to_bits()).collect(),
            None => Vec::new(),
        };

        let cacheable =
            !local_x.is_nan() && !local_y.is_nan() && !scale_x.is_nan() && !scale_y.is_nan();
        let key = ShaderKey {
            kind,
            tile_mode: tile_mode as u8,
            geom_bits,
            colors: colors_vec,
            stops_bits,
            local_bits: [
                local_x.to_bits(),
                local_y.to_bits(),
                scale_x.to_bits(),
                scale_y.to_bits(),
            ],
        };

        if cacheable {
            for cached in &self.shaders {
                if cached.key == key {
                    return cached.ptr;
                }
            }
        }

        // Prepare `@ColorLong` array (`(argb as u64) << 32` for sRGB).
        let color_jarray = self.ensure_color_long_array(env, count);
        if color_jarray.is_null() {
            return 0;
        }
        let mut packed_longs = [0i64; 16];
        let mut overflow_longs = Vec::new();
        let longs_slice: &[i64] = if count <= 16 {
            for (i, &c) in key.colors.iter().enumerate() {
                packed_longs[i] = ((c as u64) << 32) as i64;
            }
            &packed_longs[..count]
        } else {
            overflow_longs.extend(key.colors.iter().map(|&c| ((c as u64) << 32) as i64));
            &overflow_longs
        };
        unsafe {
            ((**raw).v1_2.SetLongArrayRegion)(
                raw,
                color_jarray,
                0,
                count as jint,
                longs_slice.as_ptr(),
            );
        }

        let pos_jarray: jfloatArray = if let Some(stops) = stops_slice {
            let arr = unsafe { ((**raw).v1_2.NewFloatArray)(raw, count as jint) };
            if !arr.is_null() {
                unsafe {
                    ((**raw).v1_2.SetFloatArrayRegion)(raw, arr, 0, count as jint, stops.as_ptr());
                }
            }
            arr
        } else {
            std::ptr::null_mut()
        };

        let matrix_ptr = if (local_x != 0.0 || local_y != 0.0 || scale_x != 1.0 || scale_y != 1.0)
            && scale_x != 0.0
            && scale_y != 0.0
        {
            unsafe {
                (api.matrix_set_scale)(self.scratch_matrix, 1.0 / scale_x, 1.0 / scale_y);
                (api.matrix_pre_translate)(self.scratch_matrix, -local_x, -local_y);
            }
            self.scratch_matrix
        } else {
            0
        };

        let created = match shader {
            Shader::LinearGradient { from, to, .. } => unsafe {
                (api.shader_linear_create)(
                    raw,
                    std::ptr::null_mut(),
                    matrix_ptr,
                    from.x,
                    from.y,
                    to.x,
                    to.y,
                    color_jarray,
                    pos_jarray,
                    tile_mode,
                    0,
                )
            },
            Shader::RadialGradient { center, radius, .. } => unsafe {
                (api.shader_radial_create)(
                    raw,
                    std::ptr::null_mut(),
                    matrix_ptr,
                    center.x,
                    center.y,
                    0.0,
                    center.x,
                    center.y,
                    *radius,
                    color_jarray,
                    pos_jarray,
                    tile_mode,
                    0,
                )
            },
            Shader::SweepGradient { center, .. } => unsafe {
                (api.shader_sweep_create)(
                    raw,
                    std::ptr::null_mut(),
                    matrix_ptr,
                    center.x,
                    center.y,
                    color_jarray,
                    pos_jarray,
                    0,
                )
            },
        };

        if !pos_jarray.is_null() {
            unsafe { ((**raw).v1_2.DeleteLocalRef)(raw, pos_jarray as jobject) };
        }
        let _ = crate::hwui::clear_exception_check(raw);

        if created == 0 || !cacheable {
            return created;
        }

        if self.shaders.len() < SHADER_CACHE_CAP {
            self.shaders.push(CachedShader { key, ptr: created });
        } else {
            let victim = self.shader_next;
            self.shader_next = (self.shader_next + 1) % SHADER_CACHE_CAP;
            let old_ptr = self.shaders[victim].ptr;
            if old_ptr != 0 {
                if self.gen_shader == old_ptr {
                    self.gen_shader = 0;
                    unsafe { (api.paint_set_shader)(self.general_paint, 0) };
                }
                unsafe { (api.shader_destroy)(old_ptr) };
            }
            self.shaders[victim] = CachedShader { key, ptr: created };
        }
        created
    }

    fn ensure_color_long_array(&mut self, env: &mut Env, len: usize) -> jlongArray {
        let raw = env.get_raw();
        if len > self.color_long_cap {
            let new_cap = len.next_power_of_two().max(8);
            let arr = unsafe { ((**raw).v1_2.NewLongArray)(raw, new_cap as jint) };
            if !arr.is_null() {
                let wrapped = unsafe { JObject::from_raw(env, arr as jobject) };
                if let Ok(global) = env.new_global_ref(&wrapped) {
                    self.color_long_array = global;
                    self.color_long_cap = new_cap;
                }
                env.delete_local_ref(wrapped);
            }
        }
        if len <= self.color_long_cap {
            self.color_long_array.as_raw() as jlongArray
        } else {
            std::ptr::null_mut()
        }
    }

    fn ensure_native_path(&mut self, raw: *mut JNIEnv, path: &std::sync::Arc<PathData>) -> i64 {
        let key = (path.id(), path.generation());
        if let Some(kept) = self.paths.get_mut(&key) {
            kept.drawn_as_path = true;
            if kept.defined_serial > self.named_path_serial_max {
                self.named_path_serial_max = kept.defined_serial;
            }
            return kept.native_path;
        }

        let api = self.api;
        let native_path =
            if let Some(reused) = self.free_paths_by_owner.get_mut(&path.id()).and_then(Vec::pop) {
                if reused.drawn_as_path {
                    self.flush_layers = true;
                }
                unsafe { (api.path_rewind)(reused.native_path) };
                reused.native_path
            } else if let Some(pooled) = self.path_pool.pop() {
                unsafe { (api.path_rewind)(pooled) };
                pooled
            } else {
                unsafe { (api.path_init)(raw, std::ptr::null_mut()) }
            };

        self.build_native_path_contents(raw, native_path, path);
        self.path_serial += 1;
        if self.path_serial > self.named_path_serial_max {
            self.named_path_serial_max = self.path_serial;
        }
        self.paths.insert(
            key,
            KeptPath {
                contents: std::sync::Arc::downgrade(path),
                native_path,
                defined_serial: self.path_serial,
                drawn_as_path: true,
            },
        );
        native_path
    }

    fn build_native_path_contents(&mut self, raw: *mut JNIEnv, dst: i64, data: &PathData) {
        let api = self.api;
        let steps = data.steps();
        if !steps.is_empty() {
            self.step_stack.clear();
            for step in steps {
                match step {
                    PathStep::Push(operand) => {
                        let p = if let Some(s) = self.step_paths.pop() {
                            unsafe { (api.path_rewind)(s) };
                            s
                        } else {
                            unsafe { (api.path_init)(raw, std::ptr::null_mut()) }
                        };
                        emit_path_commands(raw, api, p, operand.commands());
                        unsafe {
                            (api.path_set_fill_type)(p, fill_type_int(operand.fill_type()));
                        }
                        self.step_stack.push(p);
                    }
                    PathStep::Combine(op) => {
                        if let Some(second) = self.step_stack.pop() {
                            if let Some(&first) = self.step_stack.last() {
                                unsafe {
                                    (api.path_op)(
                                        raw,
                                        std::ptr::null_mut(),
                                        first,
                                        second,
                                        path_op_int(*op),
                                        first,
                                    );
                                }
                            }
                            self.release_step_path(second);
                        }
                    }
                    PathStep::CombineOr(op) => {
                        if let (Some(second), Some(first)) =
                            (self.step_stack.pop(), self.step_stack.pop())
                        {
                            if let Some(&target) = self.step_stack.last() {
                                unsafe {
                                    (api.path_op)(
                                        raw,
                                        std::ptr::null_mut(),
                                        first,
                                        second,
                                        path_op_int(*op),
                                        target,
                                    );
                                }
                            }
                            self.release_step_path(first);
                            self.release_step_path(second);
                        }
                    }
                    PathStep::Add => {
                        if let Some(second) = self.step_stack.pop() {
                            if let Some(&first) = self.step_stack.last() {
                                unsafe {
                                    (api.path_add_path)(raw, std::ptr::null_mut(), first, second);
                                }
                            }
                            self.release_step_path(second);
                        }
                    }
                    PathStep::SetFillType(ft) => {
                        if let Some(&top) = self.step_stack.last() {
                            unsafe { (api.path_set_fill_type)(top, fill_type_int(*ft)) };
                        }
                    }
                }
            }
            if let Some(&top) = self.step_stack.last() {
                unsafe { (api.path_set)(raw, std::ptr::null_mut(), dst, top) };
            }
            while let Some(s) = self.step_stack.pop() {
                self.release_step_path(s);
            }
        }
        emit_path_commands(raw, api, dst, data.commands());
        unsafe { (api.path_set_fill_type)(dst, fill_type_int(data.fill_type())) };
    }

    fn release_step_path(&mut self, path_ptr: i64) {
        unsafe { (self.api.path_rewind)(path_ptr) };
        if self.step_paths.len() < STEP_POOL_CAP {
            self.step_paths.push(path_ptr);
        } else {
            unsafe { (self.api.path_destroy)(path_ptr) };
        }
    }

    fn ensure_native_bitmap(&mut self, env: &mut Env, image: &ImageBitmap) -> i64 {
        if image.width() == 0 || image.height() == 0 {
            return 0;
        }
        let key = image.id();
        if let Some(kept) = self.images.get(&key)
            && kept.pixels.strong_count() > 0
        {
            return kept.native_bitmap;
        }
        if let Some(old) = self.images.remove(&key) {
            destroy_kept_image(env, self.api, old);
        }

        let api = self.api;
        let raw = env.get_raw();
        let w = (image.width() as i32).max(1);
        let h = (image.height() as i32).max(1);

        let create_args =
            [jvalue { i: w }, jvalue { i: h }, jvalue { l: api.bitmap_config_argb8888.as_raw() }];
        let bmp_local = unsafe {
            ((**raw).v1_2.CallStaticObjectMethodA)(
                raw,
                api.bitmap_class.as_raw(),
                api.bitmap_create,
                create_args.as_ptr(),
            )
        };
        if bmp_local.is_null() || crate::hwui::clear_exception_check(raw) {
            return 0;
        }
        let bmp_wrapped = unsafe { JObject::from_raw(env, bmp_local) };
        let Ok(bmp_global) = env.new_global_ref(&bmp_wrapped) else {
            env.delete_local_ref(bmp_wrapped);
            return 0;
        };
        env.delete_local_ref(bmp_wrapped);

        let pixels_slice = image.pixels();
        if let Ok(byte_buf) = unsafe {
            env.new_direct_byte_buffer(pixels_slice.as_ptr() as *mut u8, pixels_slice.len())
        } {
            let copy_args = [jvalue { l: byte_buf.as_raw() }];
            unsafe {
                ((**raw).v1_2.CallVoidMethodA)(
                    raw,
                    bmp_global.as_raw(),
                    api.bitmap_copy_from_buffer,
                    copy_args.as_ptr(),
                );
            }
            let _ = crate::hwui::clear_exception_check(raw);
            env.delete_local_ref(byte_buf);
        }

        let native_bmp = unsafe {
            ((**raw).v1_2.GetLongField)(raw, bmp_global.as_raw(), api.bitmap_native_ptr_field)
        };
        if native_bmp == 0 || crate::hwui::clear_exception_check(raw) {
            return 0;
        }
        self.images.insert(
            key,
            KeptImage {
                pixels: image.downgrade(),
                java_bitmap: bmp_global,
                native_bitmap: native_bmp,
            },
        );
        native_bmp
    }

    fn harvest_resources(&mut self, _env: &mut Env) {
        let free_by_owner = &mut self.free_paths_by_owner;
        let keys: Vec<(u64, u64)> = self
            .paths
            .iter()
            .filter_map(|(&k, kept)| (kept.contents.strong_count() == 0).then_some(k))
            .collect();
        for k in keys {
            if let Some(dead) = self.paths.remove(&k) {
                free_by_owner.entry(k.0).or_default().push(dead);
            }
        }
    }

    fn collect_unused_resources(&mut self, env: &mut Env) {
        let api = self.api;
        for (_, list) in self.free_paths_by_owner.drain() {
            for dead in list {
                if dead.drawn_as_path {
                    self.flush_layers = true;
                }
                if self.path_pool.len() < PATH_POOL_CAP {
                    unsafe { (api.path_rewind)(dead.native_path) };
                    self.path_pool.push(dead.native_path);
                } else {
                    unsafe { (api.path_destroy)(dead.native_path) };
                }
            }
        }
        let dead_img_keys: Vec<usize> = self
            .images
            .iter()
            .filter_map(|(&k, img)| (img.pixels.strong_count() == 0).then_some(k))
            .collect();
        for k in dead_img_keys {
            if let Some(dead) = self.images.remove(&k) {
                destroy_kept_image(env, api, dead);
            }
        }
    }

    fn drain_retired_paragraph_nodes(&mut self) {
        let api = self.api;
        let pool = &mut self.para_node_pool;
        let mut table = self.paragraphs.borrow_mut();
        for node in table.retired_nodes.drain(..) {
            if node != 0 {
                if pool.len() < PARA_POOL_CAP {
                    unsafe { (api.rn_discard_dl)(node) };
                    pool.push(node);
                } else {
                    unsafe { (api.rn_destroy)(node) };
                }
            }
        }
    }

    fn drop_ahead(&mut self) {
        let api = self.api;
        let pool = &mut self.group_node_pool;
        for (_, mut entry) in self.ahead.drain() {
            if entry.layered {
                reset_layer_on_node(api, entry.node);
            }
            if entry.clip_path != 0 {
                unsafe { (api.path_destroy)(entry.clip_path) };
                entry.clip_path = 0;
            }
            recycle_group_node(api, pool, entry.node);
        }
        self.ahead_items.clear();
    }

    pub fn clear(&mut self, env: &mut Env) {
        let api = self.api;
        unsafe {
            (api.rn_discard_dl)(self.kept_node);
            (api.rn_discard_dl)(self.flush_node);
        }
        self.prev_kept_nodes.clear();
        self.cur_kept_nodes.clear();
        self.flush_layers = false;

        for &cn in &self.chunk_nodes {
            unsafe { (api.rn_destroy)(cn) };
        }
        self.chunk_nodes.clear();
        self.chunk_used = 0;

        for (_, entry) in self.groups.drain() {
            if entry.clip_path != 0 {
                unsafe { (api.path_destroy)(entry.clip_path) };
            }
            unsafe { (api.rn_destroy)(entry.node) };
        }
        for (_, entry) in self.ahead.drain() {
            if entry.clip_path != 0 {
                unsafe { (api.path_destroy)(entry.clip_path) };
            }
            unsafe { (api.rn_destroy)(entry.node) };
        }
        self.ahead_items.clear();

        for node in self.group_node_pool.drain(..) {
            unsafe { (api.rn_destroy)(node) };
        }
        self.paragraphs.borrow_mut().drain_all_nodes(|node| {
            unsafe { (api.rn_destroy)(node) };
        });
        for node in self.para_node_pool.drain(..) {
            unsafe { (api.rn_destroy)(node) };
        }

        for (_, kept) in self.paths.drain() {
            unsafe { (api.path_destroy)(kept.native_path) };
        }
        for (_, list) in self.free_paths_by_owner.drain() {
            for kept in list {
                unsafe { (api.path_destroy)(kept.native_path) };
            }
        }
        for p in self.path_pool.drain(..) {
            unsafe { (api.path_destroy)(p) };
        }
        for p in self.step_paths.drain(..) {
            unsafe { (api.path_destroy)(p) };
        }
        self.step_stack.clear();

        for (_, img) in self.images.drain() {
            destroy_kept_image(env, api, img);
        }
        self.exact_char_arrays.clear();

        if self.gen_shader != 0 {
            self.gen_shader = 0;
            unsafe { (api.paint_set_shader)(self.general_paint, 0) };
        }
        for cached in self.shaders.drain(..) {
            if cached.ptr != 0 {
                unsafe { (api.shader_destroy)(cached.ptr) };
            }
        }
        self.shader_next = 0;
    }
}

impl Drop for HwuiRecorder {
    fn drop(&mut self) {
        let api = self.api;
        self.canvas_pool.destroy_all(api);

        for (_, entry) in self.groups.drain() {
            if entry.clip_path != 0 {
                unsafe { (api.path_destroy)(entry.clip_path) };
            }
            unsafe { (api.rn_destroy)(entry.node) };
        }
        for (_, entry) in self.ahead.drain() {
            if entry.clip_path != 0 {
                unsafe { (api.path_destroy)(entry.clip_path) };
            }
            unsafe { (api.rn_destroy)(entry.node) };
        }
        for node in self.group_node_pool.drain(..) {
            unsafe { (api.rn_destroy)(node) };
        }
        self.paragraphs.borrow_mut().drain_all_nodes(|node| {
            unsafe { (api.rn_destroy)(node) };
        });
        for node in self.para_node_pool.drain(..) {
            unsafe { (api.rn_destroy)(node) };
        }
        for cn in self.chunk_nodes.drain(..) {
            unsafe { (api.rn_destroy)(cn) };
        }
        if self.kept_node != 0 {
            unsafe { (api.rn_destroy)(self.kept_node) };
        }
        if self.flush_node != 0 {
            unsafe { (api.rn_destroy)(self.flush_node) };
        }

        for &p in &self.fill_paint_ptrs {
            if p != 0 {
                unsafe { (api.paint_destroy)(p) };
            }
        }
        if self.gen_shader != 0 && self.general_paint != 0 {
            unsafe { (api.paint_set_shader)(self.general_paint, 0) };
        }
        for cached in self.shaders.drain(..) {
            if cached.ptr != 0 {
                unsafe { (api.shader_destroy)(cached.ptr) };
            }
        }
        if self.general_paint != 0 {
            unsafe { (api.paint_destroy)(self.general_paint) };
        }

        for (_, kept) in self.paths.drain() {
            unsafe { (api.path_destroy)(kept.native_path) };
        }
        for (_, list) in self.free_paths_by_owner.drain() {
            for kept in list {
                unsafe { (api.path_destroy)(kept.native_path) };
            }
        }
        for p in self.path_pool.drain(..) {
            unsafe { (api.path_destroy)(p) };
        }
        for p in self.step_paths.drain(..) {
            unsafe { (api.path_destroy)(p) };
        }
        for p in self.step_stack.drain(..) {
            unsafe { (api.path_destroy)(p) };
        }
        if self.scratch_path != 0 {
            unsafe { (api.path_destroy)(self.scratch_path) };
        }
        if self.outline_path != 0 {
            unsafe { (api.path_destroy)(self.outline_path) };
        }
        if self.scratch_matrix != 0 {
            unsafe { (api.matrix_destroy)(self.scratch_matrix) };
        }
    }
}

#[inline]
fn create_render_node(raw_env: *mut JNIEnv, api: &HwuiApi) -> i64 {
    unsafe { (api.rn_create)(raw_env, std::ptr::null_mut(), std::ptr::null_mut()) }
}

fn create_default_paint(raw_env: *mut JNIEnv, api: &HwuiApi) -> i64 {
    let p = unsafe { (api.paint_init)(raw_env, std::ptr::null_mut()) };
    if p != 0 {
        unsafe { (api.paint_set_flags)(p, DEFAULT_PAINT_FLAGS) };
    }
    p
}

fn obtain_group_node(raw_env: *mut JNIEnv, api: &HwuiApi, pool: &mut Vec<i64>) -> i64 {
    if let Some(node) = pool.pop() {
        return node;
    }
    let node = create_render_node(raw_env, api);
    unsafe {
        (api.rn_set_clip_to_bounds)(node, 0);
        (api.rn_set_overlapping)(node, 1);
    }
    node
}

fn recycle_group_node(api: &HwuiApi, pool: &mut Vec<i64>, node: i64) {
    if pool.len() < GROUP_POOL_CAP {
        unsafe { (api.rn_discard_dl)(node) };
        pool.push(node);
    } else {
        unsafe { (api.rn_destroy)(node) };
    }
}

fn obtain_para_node(raw_env: *mut JNIEnv, api: &HwuiApi, pool: &mut Vec<i64>) -> i64 {
    if let Some(node) = pool.pop() { node } else { create_render_node(raw_env, api) }
}

fn reset_layer_on_node(api: &HwuiApi, node: i64) {
    unsafe {
        (api.rn_set_alpha)(node, 1.0);
        (api.rn_set_pivot_x)(node, 0.0);
        (api.rn_set_pivot_y)(node, 0.0);
        (api.rn_set_scale_x)(node, 1.0);
        (api.rn_set_scale_y)(node, 1.0);
        (api.rn_set_rotation)(node, 0.0);
        (api.rn_set_translation_x)(node, 0.0);
        (api.rn_set_translation_y)(node, 0.0);
        (api.rn_set_outline_none)(node);
        (api.rn_set_clip_to_outline)(node, 0);
        (api.rn_set_elevation)(node, 0.0);
        (api.rn_set_ambient_shadow_color)(node, BLACK_ARGB);
        (api.rn_set_spot_shadow_color)(node, BLACK_ARGB);
        (api.rn_set_layer_type)(node, 0);
        (api.rn_set_layer_paint)(node, 0);
        (api.rn_set_overlapping)(node, 1);
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_outline_on_node(
    raw_env: *mut JNIEnv,
    api: &HwuiApi,
    node: i64,
    out_tag: u8,
    rect: &Rect,
    radii: &[f32; 4],
    generic_path_ptr: i64,
    generic_offset: Offset,
    alpha: f32,
    outline_path: i64,
    radii_jarray: jfloatArray,
) -> bool {
    if out_tag == 0 {
        unsafe { (api.rn_set_outline_none)(node) };
        return true;
    }
    if out_tag == 3 {
        if generic_path_ptr == 0 {
            unsafe { (api.rn_set_outline_empty)(node) };
            return true;
        }
        unsafe {
            (api.path_set)(raw_env, std::ptr::null_mut(), outline_path, generic_path_ptr);
            if generic_offset.x != 0.0 || generic_offset.y != 0.0 {
                (api.path_offset)(
                    raw_env,
                    std::ptr::null_mut(),
                    outline_path,
                    generic_offset.x,
                    generic_offset.y,
                );
            }
        }
        if unsafe { (api.path_is_empty)(outline_path) != 0 } {
            unsafe { (api.rn_set_outline_empty)(node) };
            true
        } else {
            unsafe { api.set_outline_path(node, outline_path, alpha) };
            api.sdk_int >= 33
        }
    } else {
        let r0 = radii[0];
        let uniform = r0 == radii[1] && r0 == radii[2] && r0 == radii[3];
        if uniform {
            let l = java_round(rect.left);
            let t = java_round(rect.top);
            let r = java_round(rect.right);
            let b = java_round(rect.bottom);
            if l >= r || t >= b {
                unsafe { (api.rn_set_outline_empty)(node) };
            } else {
                unsafe { api.set_outline_round_rect(node, l, t, r, b, r0, alpha) };
            }
            true
        } else {
            unsafe { (api.path_rewind)(outline_path) };
            add_round_rect_to_path(
                raw_env,
                api,
                outline_path,
                rect,
                radii,
                radii_jarray,
                PATH_DIR_CCW,
            );
            if unsafe { (api.path_is_empty)(outline_path) != 0 } {
                unsafe { (api.rn_set_outline_empty)(node) };
                true
            } else {
                unsafe { api.set_outline_path(node, outline_path, alpha) };
                api.sdk_int >= 33
            }
        }
    }
}

fn upload_chars_to_array(
    env: &mut Env,
    char_array: &mut Global<JObject<'static>>,
    char_array_cap: &mut usize,
    chars: &[u16],
) {
    let raw = env.get_raw();
    if chars.len() > *char_array_cap {
        let new_cap = chars.len().next_power_of_two().max(128);
        let arr = unsafe { ((**raw).v1_2.NewCharArray)(raw, new_cap as jint) };
        if !arr.is_null() {
            let wrapped = unsafe { JObject::from_raw(env, arr as jobject) };
            if let Ok(global) = env.new_global_ref(&wrapped) {
                *char_array = global;
                *char_array_cap = new_cap;
            }
            env.delete_local_ref(wrapped);
        }
    }
    let copy_len = chars.len().min(*char_array_cap);
    unsafe {
        ((**raw).v1_2.SetCharArrayRegion)(
            raw,
            char_array.as_raw() as jcharArray,
            0,
            copy_len as jint,
            chars.as_ptr(),
        );
    }
}

fn add_round_rect_to_path(
    raw_env: *mut JNIEnv,
    api: &HwuiApi,
    path_ptr: i64,
    rect: &Rect,
    radii: &[f32; 4],
    radii_jarray: jfloatArray,
    dir: i32,
) {
    let corners: [jfloat; 8] =
        [radii[0], radii[0], radii[1], radii[1], radii[2], radii[2], radii[3], radii[3]];
    unsafe {
        ((**raw_env).v1_2.SetFloatArrayRegion)(raw_env, radii_jarray, 0, 8, corners.as_ptr())
    };
    unsafe {
        (api.path_add_round_rect)(
            raw_env,
            std::ptr::null_mut(),
            path_ptr,
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            radii_jarray,
            dir,
        );
    }
}

fn emit_path_commands(
    raw_env: *mut JNIEnv,
    api: &HwuiApi,
    path_ptr: i64,
    commands: &[PathCommand],
) {
    let null_cls = std::ptr::null_mut();
    for cmd in commands {
        match cmd {
            PathCommand::MoveTo(pt) => unsafe {
                (api.path_move_to)(raw_env, null_cls, path_ptr, pt.x, pt.y);
            },
            PathCommand::LineTo(pt) => unsafe {
                (api.path_line_to)(raw_env, null_cls, path_ptr, pt.x, pt.y);
            },
            PathCommand::QuadTo(ctrl, end) => unsafe {
                (api.path_quad_to)(raw_env, null_cls, path_ptr, ctrl.x, ctrl.y, end.x, end.y);
            },
            PathCommand::CubicTo(c1, c2, end) => unsafe {
                (api.path_cubic_to)(
                    raw_env, null_cls, path_ptr, c1.x, c1.y, c2.x, c2.y, end.x, end.y,
                );
            },
            PathCommand::Close => unsafe {
                (api.path_close)(raw_env, null_cls, path_ptr);
            },
        }
    }
}

#[inline]
fn fill_type_int(ft: PathFillType) -> i32 {
    match ft {
        PathFillType::NonZero => 0,
        PathFillType::EvenOdd => 1,
    }
}

#[inline]
fn path_op_int(op: PathOperation) -> i32 {
    match op {
        PathOperation::Difference => 0,
        PathOperation::Intersect => 1,
        PathOperation::Union => 2,
        PathOperation::Xor => 3,
        PathOperation::ReverseDifference => 4,
    }
}

#[inline]
fn draw_node_at(api: &HwuiApi, canvas: i64, node: i64, x: f32, y: f32) {
    if x == 0.0 && y == 0.0 {
        unsafe { (api.dlc_draw_render_node)(canvas, node) };
    } else {
        unsafe {
            (api.canvas_save)(canvas, SAVE_MATRIX_CLIP);
            (api.canvas_translate)(canvas, x, y);
            (api.dlc_draw_render_node)(canvas, node);
            (api.canvas_restore)(canvas);
        }
    }
}

#[inline]
fn is_chunkable_draw_op(op: &DrawOp) -> bool {
    matches!(
        op,
        DrawOp::DrawRect(..)
            | DrawOp::DrawRoundRect(..)
            | DrawOp::DrawLine(..)
            | DrawOp::DrawParagraph(..)
            | DrawOp::DrawImage { .. }
            | DrawOp::DrawArc { .. }
            | DrawOp::DrawPath { .. }
    )
}

fn draws_group_before(
    boundaries: &[Boundary],
    end: usize,
    groups: &[DrawGroup],
    child: usize,
) -> bool {
    let inside = boundaries.iter().take_while(|b| b.at <= end).count();
    let mut next = child;
    for _ in 0..inside {
        let Some(group) = groups.get(next) else { return false };
        if !group.empty {
            return true;
        }
        next += 1 + group.descendants;
    }
    false
}

fn destroy_kept_image(env: &mut Env, api: &HwuiApi, img: KeptImage) {
    let raw = env.get_raw();
    unsafe {
        ((**raw).v1_2.CallVoidMethodA)(
            raw,
            img.java_bitmap.as_raw(),
            api.bitmap_recycle,
            std::ptr::null(),
        );
    }
    let _ = crate::hwui::clear_exception_check(raw);
}

#[inline]
#[allow(clippy::unnecessary_cast)]
fn monotonic_nanos() -> i64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) } == 0 {
        (ts.tv_sec as i64).saturating_mul(1_000_000_000).saturating_add(ts.tv_nsec as i64)
    } else {
        0
    }
}
