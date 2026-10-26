//! The groups' records for the Kotlin replayer (D157): a `GROUP` record carries its key and
//! version, and is the same bytes for the same key and version (copied from a cache); a lifted
//! graphics layer's operations are in no record, its properties in the `LAYERS` section, so a
//! layer-only frame changes nothing but that section.

#![allow(non_snake_case)]

// The call macros name the runtime `compose`.
extern crate compose_runtime as compose;

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use compose_android::serialize::{
    AHEAD_CAPACITY, GroupCache, KEPT_ENTRY_BYTES, LAYER_ENTRY_BYTES, Resources, Writer,
    compositing, layer_entry, operand_bytes, outline_kind, shader, tag, write_display_list,
    write_groups_ahead,
};
use compose_runtime::*;
use compose_ui::platform::*;
use compose_ui::*;

// ---- Reading the bytes back ------------------------------------------------------------------

/// What the bytes hold, as `Replayer.kt` reads them.
#[derive(Debug, PartialEq)]
enum Item {
    Operation(u8),
    Group { key: u64, version: u64, content: Vec<Item> },
}

/// A `LAYERS` entry's outline: its kind (1 rectangle, 2 rounded), its bounds, its corner radii.
type Outline = (u8, (f32, f32, f32, f32), [f32; 4]);

/// A `LAYERS` entry.
#[derive(Debug, PartialEq)]
struct Layer {
    key: u64,
    alpha: f32,
    pivot: (f32, f32),
    scale: (f32, f32),
    rotation: f32,
    translation: (f32, f32),
    /// Clips to its outline.
    clip: bool,
    /// Its outline: a kind (1 rectangle, 2 rounded), its bounds, its corner radii.
    outline: Option<Outline>,
    elevation: f32,
    ambient: u32,
    spot: u32,
    /// 0 `Auto`, 1 `Offscreen`, 2 `ModulateAlpha` (`serialize::compositing`).
    compositing: u8,
    bounds: (f32, f32, f32, f32),
}

impl Layer {
    /// A layer of `key` with Compose's defaults, at `bounds`.
    fn plain(key: u64, bounds: (f32, f32, f32, f32)) -> Layer {
        Layer {
            key,
            alpha: 1.0,
            pivot: ((bounds.0 + bounds.2) / 2.0, (bounds.1 + bounds.3) / 2.0),
            scale: (1.0, 1.0),
            rotation: 0.0,
            translation: (0.0, 0.0),
            clip: false,
            outline: None,
            elevation: 0.0,
            ambient: 0xFF00_0000,
            spot: 0xFF00_0000,
            compositing: compositing::AUTO,
            bounds,
        }
    }
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}

fn f32_at(bytes: &[u8], at: usize) -> f32 {
    f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

/// The `LAYERS` section (if any) and the operations after it.
fn read(bytes: &[u8]) -> (Vec<Layer>, Vec<Item>) {
    let mut layers = Vec::new();
    let mut at = 0;
    if bytes.first() == Some(&tag::LAYERS) {
        let count = u32_at(bytes, 1) as usize;
        at = 5;
        for _ in 0..count {
            use layer_entry as e;
            // After the key.
            let entry = at + 8;
            let f = |offset: usize| f32_at(bytes, entry + offset);
            let four = |offset: usize| (f(offset), f(offset + 4), f(offset + 8), f(offset + 12));
            let kind = bytes[entry + e::OUTLINE_KIND];
            let radii = four(e::OUTLINE_RADII);
            layers.push(Layer {
                key: u64_at(bytes, at),
                alpha: f(e::ALPHA),
                pivot: (f(e::PIVOT), f(e::PIVOT + 4)),
                scale: (f(e::SCALE), f(e::SCALE + 4)),
                rotation: f(e::ROTATION),
                translation: (f(e::TRANSLATION), f(e::TRANSLATION + 4)),
                clip: bytes[entry + e::CLIP] != 0,
                outline: (kind != outline_kind::NONE)
                    .then(|| (kind, four(e::OUTLINE_BOUNDS), [radii.0, radii.1, radii.2, radii.3])),
                elevation: f(e::ELEVATION),
                ambient: u32_at(bytes, entry + e::AMBIENT),
                spot: u32_at(bytes, entry + e::SPOT),
                compositing: bytes[entry + e::COMPOSITING],
                bounds: four(e::BOUNDS),
            });
            at += LAYER_ENTRY_BYTES;
        }
    }
    // The groups' records (D323), then the frame's operations.
    let mut records = HashMap::new();
    while bytes.get(at) == Some(&tag::GROUP) {
        let (key, version) = (u64_at(bytes, at + 1), u64_at(bytes, at + 9));
        let length = u32_at(bytes, at + 17) as usize;
        at += 21;
        assert!(records.insert(key, (version, at, at + length)).is_none(), "one record a group");
        at += length;
    }
    let mut drawn = HashSet::new();
    // The kept groups (D380), outer and nested: drawn by no operation, their records not in
    // the frame unless their version changed.
    if bytes.get(at) == Some(&tag::KEPT) {
        let count = u32_at(bytes, at + 1) as usize;
        for index in 0..count {
            assert!(drawn.insert(u64_at(bytes, at + 5 + KEPT_ENTRY_BYTES * index)), "kept twice");
        }
        at += 5 + KEPT_ENTRY_BYTES * count;
    }
    let items = read_items(bytes, at, bytes.len(), &records, &mut drawn);
    assert!(records.keys().all(|key| drawn.contains(key)), "every record is drawn once");
    (layers, items)
}

/// `bytes` without the records of the groups its `KEPT` section names (D380): a frame writes
/// them only if their version changed; with nothing cached, always.
fn without_kept_records(mut bytes: Vec<u8>) -> Vec<u8> {
    let kept: HashSet<u64> = kept_entries(&bytes).into_iter().map(|(key, _)| key).collect();
    let mut at = if bytes.first() == Some(&tag::LAYERS) {
        5 + u32_at(&bytes, 1) as usize * LAYER_ENTRY_BYTES
    } else {
        0
    };
    while bytes.get(at) == Some(&tag::GROUP) {
        let length = 21 + u32_at(&bytes, at + 17) as usize;
        if kept.contains(&u64_at(&bytes, at + 1)) {
            bytes.drain(at..at + length);
        } else {
            at += length;
        }
    }
    bytes
}

/// The keys the `KEPT` section names (D380), and whether each is outermost.
fn kept_entries(bytes: &[u8]) -> Vec<(u64, bool)> {
    let mut at = if bytes.first() == Some(&tag::LAYERS) {
        5 + u32_at(bytes, 1) as usize * LAYER_ENTRY_BYTES
    } else {
        0
    };
    while bytes.get(at) == Some(&tag::GROUP) {
        at += 21 + u32_at(bytes, at + 17) as usize;
    }
    if bytes.get(at) != Some(&tag::KEPT) {
        return Vec::new();
    }
    (0..u32_at(bytes, at + 1) as usize)
        .map(|index| {
            let entry = at + 5 + KEPT_ENTRY_BYTES * index;
            (u64_at(bytes, entry), bytes[entry + 8] != 0)
        })
        .collect()
}

/// The outermost keys the `KEPT` section names.
fn kept_keys(bytes: &[u8]) -> Vec<u64> {
    kept_entries(bytes)
        .into_iter()
        .filter(|(_, outermost)| *outermost)
        .map(|(key, _)| key)
        .collect()
}

/// The operations in `bytes[at..end]`, each `GROUP_REF` read as its group's record (from
/// `records`: version, content's start and end), which `drawn` gains.
fn read_items(
    bytes: &[u8],
    mut at: usize,
    end: usize,
    records: &HashMap<u64, (u64, usize, usize)>,
    drawn: &mut HashSet<u64>,
) -> Vec<Item> {
    let mut items = Vec::new();
    while at < end {
        let op = bytes[at];
        at += 1;
        assert_ne!(op, tag::GROUP, "a record among the operations");
        if op == tag::GROUP_REF {
            let key = u64_at(bytes, at);
            at += 8;
            let &(version, from, to) = records.get(&key).expect("a drawn group's record");
            assert!(drawn.insert(key), "a group drawn twice");
            items.push(Item::Group {
                key,
                version,
                content: read_items(bytes, from, to, records, drawn),
            });
        } else {
            items.push(Item::Operation(op));
            at += operand_bytes(bytes, at - 1);
        }
    }
    assert_eq!(at, end, "a record's length is its content's");
    items
}

/// `bytes` with each record's version zeroed: what a write with nothing cached gives too, whose
/// records carry the display list's versions where cached ones keep older ones (D323).
fn unversioned_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut bytes = bytes.to_vec();
    let mut at = if bytes.first() == Some(&tag::LAYERS) {
        5 + u32_at(&bytes, 1) as usize * LAYER_ENTRY_BYTES
    } else {
        0
    };
    while bytes.get(at) == Some(&tag::GROUP) {
        bytes[at + 9..at + 17].fill(0);
        at += 21 + u32_at(&bytes, at + 17) as usize;
    }
    bytes
}

/// Where the first `op` is, groups' contents included.
fn find(bytes: &[u8], op: u8) -> Option<usize> {
    let mut at = if bytes.first() == Some(&tag::LAYERS) {
        5 + u32_at(bytes, 1) as usize * LAYER_ENTRY_BYTES
    } else {
        0
    };
    while at < bytes.len() {
        match bytes[at] {
            found if found == op => return Some(at),
            // Into the record's content.
            tag::GROUP => at += 21,
            _ => at += 1 + operand_bytes(bytes, at),
        }
    }
    None
}

/// Every operation's tag, groups' contents included.
fn all_operations(items: &[Item]) -> Vec<u8> {
    let mut operations = Vec::new();
    for item in items {
        match item {
            Item::Operation(op) => operations.push(*op),
            Item::Group { content, .. } => operations.extend(all_operations(content)),
        }
    }
    operations
}

/// The groups' (key, version), outer before inner.
fn groups(items: &[Item]) -> Vec<(u64, u64)> {
    let mut groups = Vec::new();
    for item in items {
        if let Item::Group { key, version, content } = item {
            groups.push((*key, *version));
            groups.extend(self::groups(content));
        }
    }
    groups
}

// ---- A host and its serializer ---------------------------------------------------------------

/// A host's frames written as `bridge.rs` writes them, keeping its resources and cache.
struct Serialized {
    host: UiHost,
    resources: Resources,
    cache: GroupCache,
    out: Writer,
}

/// compose-rs's own lazy-list behaviours beyond Compose's, most of which hosts don't use by
/// default (D560-D565; drawing ahead is in the default set, D610): two ahead, D341's budget, prefetching at rest, two kept behind (their groups
/// listed, D380), drawing ahead (D350). For the tests of kept and drawn-ahead groups.
fn legacy() -> Optimizations {
    Optimizations::parse(
        "prefetch_two_ahead,prefetch_budget_recent_maximum,prefetch_at_rest,keep_items_behind,\
         draw_prefetched_ahead",
    )
    .expect("known names")
}

impl Serialized {
    fn new(content: impl Fn() + 'static) -> Serialized {
        Serialized::with(Optimizations::defaults(), content)
    }

    /// With compose-rs's `optimizations`, as the bridge sets them on its host and cache.
    fn with(optimizations: Optimizations, content: impl Fn() + 'static) -> Serialized {
        let mut host = UiHost::headless(200, 200);
        host.set_optimizations(optimizations);
        // As the bridge does (D380).
        host.set_keep_unplaced_groups(true);
        host.set_content(content);
        let mut serialized = Serialized {
            host,
            resources: Resources::default(),
            cache: GroupCache::new(optimizations),
            out: Writer::default(),
        };
        serialized.frame();
        serialized
    }

    /// A frame's bytes, the cache used.
    fn frame(&mut self) -> Vec<u8> {
        self.host.frame();
        let (resources, cache, out) = (&mut self.resources, &mut self.cache, &mut self.out);
        self.host.with_tree(|tree| write_display_list(tree.display_list(), resources, cache, out));
        self.resources.collect();
        self.out.bytes.clone()
    }

    /// The last frame's bytes written with nothing cached (the same resources: ids stay).
    fn fresh(&mut self) -> Vec<u8> {
        let mut out = Writer::default();
        let resources = &mut self.resources;
        self.host.with_tree(|tree| {
            let mut cache = GroupCache::new(tree.optimizations());
            write_display_list(tree.display_list(), resources, &mut cache, &mut out)
        });
        // The kept groups' records (D380), written with nothing cached, are cached in a frame.
        without_kept_records(out.bytes)
    }
}

fn stack() -> Rc<dyn MeasurePolicy> {
    Rc::new(StackMeasurePolicy)
}

/// A leaf of the size its modifiers give it.
fn Leaf(modifier: Modifier) {
    Layout(modifier, empty_measure_policy(), callback(|| {}));
}

/// A leaf filling itself with what `color` gives.
fn painted(color: impl Fn() -> Color + 'static) -> Modifier {
    Modifier.draw_behind(move |scope| {
        let bounds = scope.bounds();
        scope.draw_rect(bounds, Paint::fill(color()));
    })
}

// ---- Records and layers ----------------------------------------------------------------------

/// A graphics layer's group: its record carries its key and version, its content only its
/// drawing (the layer's save, transform, clip and alpha layer are the `LAYERS` entry's).
#[test]
fn a_lifted_layer_is_its_layers_entry_not_operations() {
    // Parity: Compose's `Auto` compositing, which the default's single-drawing fade would turn
    // into a modulated alpha for this one drawing.
    let mut serialized = Serialized::with(Optimizations::NONE, || {
        Leaf(
            Modifier
                .graphics_layer_with(GraphicsLayerScope {
                    alpha: 0.5,
                    rotation_z: 30.0,
                    translation_x: 4.0,
                    clip: true,
                    ..GraphicsLayerScope::default()
                })
                .then(painted(|| Color::RED))
                .size(20.dp()),
        );
    });
    let bytes = serialized.frame();
    let list = serialized.host.display_list();
    let group = list.groups.iter().find(|group| group.layer.is_some()).expect("a layer group");
    let (layers, items) = read(&bytes);
    assert_eq!(
        layers,
        [Layer {
            alpha: 0.5,
            rotation: 30.0,
            translation: (4.0, 0.0),
            // Clipped to its outline, a rectangle, as Compose's `clip` with `RectangleShape`.
            clip: true,
            outline: Some((1, (0.0, 0.0, 20.0, 20.0), [0.0; 4])),
            // Compose's `Auto`: offscreen for an alpha below 1, one drawing or more.
            compositing: compositing::AUTO,
            ..Layer::plain(group.key, (0.0, 0.0, 20.0, 20.0))
        }]
    );
    assert_eq!(groups(&items), [(group.key, group.version)]);
    // In the display list, the layer's operations are there; in the bytes, nowhere.
    assert!(
        list.ops.iter().any(|op| matches!(op, DrawOp::SaveLayerAlpha { .. })),
        "{}",
        list.dump()
    );
    let operations = all_operations(&items);
    for lifted in [tag::SAVE, tag::TRANSFORM, tag::CLIP_RECT, tag::SAVE_LAYER_ALPHA, tag::RESTORE] {
        assert!(!operations.contains(&lifted), "{lifted} written: {items:?}");
    }
    let Item::Group { content, .. } = &items[0] else { panic!("{items:?}") };
    assert_eq!(content, &[Item::Operation(tag::DRAW_RECT)]);
}

/// A layer's bounds (its render node's, bounding its alpha's offscreen layer) are its size at
/// its origin in the group: after the padding before it, and its pivot their center.
#[test]
fn a_layers_bounds_are_its_size_at_its_origin() {
    let mut serialized = Serialized::new(|| {
        Leaf(
            Modifier
                .padding(4.dp())
                .graphics_layer_with(GraphicsLayerScope {
                    alpha: 0.5,
                    ..GraphicsLayerScope::default()
                })
                .then(painted(|| Color::RED))
                .size(20.dp()),
        );
    });
    let (layers, _) = read(&serialized.frame());
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].bounds, (4.0, 4.0, 24.0, 24.0));
    assert_eq!(layers[0].pivot, (14.0, 14.0));
}

/// A faded layer drawing two things (they may overlap) or holding a group composites
/// offscreen; one at full alpha needs nothing either way.
#[test]
fn a_layer_drawing_more_than_one_thing_composites_offscreen() {
    let layer = |alpha: f32, content: fn()| {
        let fade = Optimizations::NONE.with(Optimization::SINGLE_DRAWING_FADE, true);
        let mut serialized = Serialized::with(fade, move || {
            Layout(
                Modifier.graphics_layer_with(GraphicsLayerScope {
                    alpha,
                    ..GraphicsLayerScope::default()
                }),
                stack(),
                callback(content),
            );
        });
        let (layers, _) = read(&serialized.frame());
        layers
            .iter()
            .map(|layer| layer.compositing != compositing::MODULATE_ALPHA_IN_BOUNDS)
            .collect::<Vec<_>>()
    };
    fn two() {
        Leaf(painted(|| Color::RED).size(10.dp()));
        Leaf(painted(|| Color::BLUE).size(10.dp()));
    }
    fn one_in_a_group() {
        Leaf(Modifier.repaint_boundary().then(painted(|| Color::RED)).size(10.dp()));
    }
    fn one() {
        Leaf(painted(|| Color::RED).size(10.dp()));
    }
    assert_eq!(layer(0.5, two), [true]);
    assert_eq!(layer(0.5, one_in_a_group), [true]);
    assert_eq!(layer(0.5, one), [false]);
}

/// A layer-only frame (an animated alpha and rotation, nested in a boundary) changes the
/// `LAYERS` section and nothing else: every record is the same bytes, copied from the cache.
#[test]
fn a_layer_only_frame_changes_only_the_layers_section() {
    let alpha = mutable_state_of(1.0f32);
    let handle = alpha.handle();
    let mut serialized = Serialized::new(move || {
        Layout(
            Modifier.repaint_boundary().then(painted(|| Color::BLUE)),
            stack(),
            callback(move || {
                Leaf(
                    Modifier
                        .graphics_layer(move |layer| {
                            layer.alpha = handle.get();
                            layer.rotation_z = 90.0 * (1.0 - handle.get());
                        })
                        .then(painted(|| Color::RED))
                        .size(10.dp()),
                );
            }),
        );
    });
    let before = serialized.frame();
    let (layers_before, items_before) = read(&before);
    assert_eq!(groups(&items_before).len(), 2, "{items_before:?}");
    assert_eq!(layers_before.len(), 1);
    alpha.set(0.25);
    let after = serialized.frame();
    let (layers_after, items_after) = read(&after);
    assert_eq!(items_after, items_before);
    assert_eq!((layers_after[0].alpha, layers_after[0].rotation), (0.25, 67.5));
    let section = 5 + LAYER_ENTRY_BYTES;
    assert_eq!(after[section..], before[section..], "the records' bytes changed");
    assert_eq!(serialized.cache.copied(), 2, "both records are copied");
    assert_eq!(after, serialized.fresh(), "the cached records differ from fresh ones");
}

/// Copied records are the bytes writing them again would give, across frames where one group
/// changes and its sibling doesn't; its parent, whose bytes stay (it names the group by key),
/// keeps its version (D323); a gone group leaves the cache.
#[test]
fn cached_records_are_the_bytes_written_afresh() {
    let color = mutable_state_of(Color::RED);
    let shown = mutable_state_of(true);
    let (color_handle, shown_handle) = (color.handle(), shown.handle());
    let mut serialized = Serialized::new(move || {
        Layout(
            Modifier.repaint_boundary(),
            stack(),
            callback(move || {
                Leaf(
                    Modifier
                        .repaint_boundary()
                        .then(painted(move || color_handle.get()))
                        .size(10.dp()),
                );
                Leaf(Modifier.repaint_boundary().then(painted(|| Color::BLUE)).size(10.dp()));
            }),
        );
        if shown_handle.get() {
            Leaf(Modifier.repaint_boundary().then(painted(|| Color::GREEN)).size(10.dp()));
        }
    });
    let first = serialized.frame();
    let [(outer, outer_version), (changing, changing_version), (still, still_version), (other, _)] =
        groups(&read(&first).1)[..]
    else {
        panic!("{:?}", read(&first).1)
    };
    assert_eq!(serialized.cache.len(), 4);
    color.set(Color::GREEN);
    let second = serialized.frame();
    assert_eq!(serialized.cache.copied(), 2, "the unchanged sibling and the other group");
    assert_eq!(serialized.cache.unchanged(), 1, "the parent, written again, is the same bytes");
    assert_eq!(unversioned_bytes(&second), unversioned_bytes(&serialized.fresh()));
    let now = groups(&read(&second).1);
    assert_eq!(now[0], (outer, outer_version), "a nested change keeps its parent's version");
    assert_ne!(now[1], (changing, changing_version));
    assert_eq!(now[2], (still, still_version));
    assert_eq!(now[3].0, other);
    shown.set(false);
    let third = serialized.frame();
    assert_eq!(unversioned_bytes(&third), unversioned_bytes(&serialized.fresh()));
    assert_eq!(serialized.cache.len(), 3, "the gone group's record is dropped");
}

/// A leaf clipped to a triangle and filled with `brush`.
fn clipped_gradient(brush: impl Fn() -> Brush + 'static) -> Modifier {
    Modifier.draw_behind(move |scope| {
        let triangle = Path::from_svg("M0,0 L10,0 L0,10 Z").unwrap().data();
        let paint = scope.brush_paint(&brush(), 1.0, PaintStyle::Fill);
        scope.clip_path(&triangle, |scope| {
            let bounds = scope.bounds();
            scope.draw_rect(bounds, paint);
        });
    })
}

/// The index of the shader the boundary drawn last paints with, in the flattened list (after a
/// sibling's shaders).
fn last_shader_index(serialized: &Serialized) -> u32 {
    serialized.host.with_tree(|tree| {
        let list = tree.display_list().flatten();
        list.ops
            .iter()
            .rev()
            .find_map(|op| match op {
                DrawOp::DrawRect(_, paint) => paint.shader(),
                _ => None,
            })
            .expect("a gradient")
            .0
    })
}

/// A retained boundary's gradient is in its record whole (D210), not as its index in the list:
/// spliced after a sibling that stops painting a gradient, its index changes while its version
/// doesn't, and the copied record is still the bytes a fresh write gives. Its clip path names
/// its path, which keeps its id.
#[test]
fn a_copied_record_keeps_its_gradient_when_the_lists_shaders_shift() {
    let sibling_gradient = mutable_state_of(true);
    let handle = sibling_gradient.handle();
    let mut serialized = Serialized::new(move || {
        Layout(
            Modifier,
            stack(),
            callback(move || {
                Leaf(
                    clipped_gradient(move || {
                        if handle.get() {
                            Brush::vertical_gradient_with(
                                [Color::BLACK, Color::WHITE],
                                0.0,
                                f32::INFINITY,
                                TileMode::Clamp,
                            )
                        } else {
                            Brush::solid(Color::BLACK)
                        }
                    })
                    .size(10.dp()),
                );
                Leaf(
                    Modifier
                        .repaint_boundary()
                        .then(clipped_gradient(|| {
                            Brush::linear_gradient_with(
                                [(0.0, Color::RED), (0.3, Color::GREEN), (1.0, Color::BLUE)],
                                Offset::ZERO,
                                Offset::INFINITE,
                                TileMode::Mirror,
                            )
                        }))
                        .size(10.dp()),
                );
            }),
        );
    });
    let first = serialized.frame();
    assert_eq!(last_shader_index(&serialized), 1);
    let (_, items) = read(&first);
    let [(key, version)] = groups(&items)[..] else { panic!("{items:?}") };
    let operations = all_operations(&items);
    assert_eq!(operations.iter().filter(|op| **op == tag::CLIP_PATH).count(), 2);
    sibling_gradient.set(false);
    let second = serialized.frame();
    assert_eq!(last_shader_index(&serialized), 0, "the splice shifted the boundary's index");
    assert_eq!(groups(&read(&second).1), [(key, version)], "the boundary kept its version");
    assert_eq!(serialized.cache.copied(), 1);
    assert_eq!(second, serialized.fresh());
    // The record holds the gradient: its stops, its tile mode (mirror) and its end (the leaf's
    // far corner, 10 dp at density 1).
    let record = find(&second, tag::GROUP).unwrap();
    let shader_at = find(&second[record..], tag::DRAW_RECT).unwrap() + record + 1 + 16 + 10;
    assert_eq!(&second[shader_at..shader_at + 2], [shader::LINEAR, 2]);
    let end = (f32_at(&second, shader_at + 10), f32_at(&second, shader_at + 14));
    assert_eq!(end.0 - f32_at(&second, shader_at + 2), 10.0);
    assert_eq!(end.1 - f32_at(&second, shader_at + 6), 10.0);
    let stops_at = shader_at + 18 + 4 + 3 * 4;
    assert_eq!(u32_at(&second, stops_at), 3);
    assert_eq!(f32_at(&second, stops_at + 8), 0.3);
}

/// A group of `key` and `version` drawing `content` (nothing nested), by hand.
fn group(
    key: u64,
    version: u64,
    layer: Option<LayerProperties>,
    content: DisplayList,
) -> DrawGroup {
    DrawGroup {
        key,
        version,
        layer,
        empty: content.ops.is_empty(),
        content: Arc::new(content),
        descendants: 0,
    }
}

/// A `SaveLayerAlpha` around a group isn't folded into the group's drawing: the folded alpha
/// would be in the group's record, which its version doesn't cover.
#[test]
fn a_layer_around_a_group_is_not_folded_into_it() {
    let mut content = DisplayList::default();
    content.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::BLACK)));
    let mut list = DisplayList::default();
    list.push(DrawOp::SaveLayerAlpha { bounds: Rect::new(0.0, 0.0, 10.0, 10.0), alpha: 0.5 });
    list.boundaries.push(Boundary { at: 1, key: 1 });
    list.push(DrawOp::Restore);
    list.groups.push(group(1, 1, None, content));
    let (mut resources, mut out) = (Resources::default(), Writer::default());
    write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
    let (_, items) = read(&out.bytes);
    assert_eq!(
        items,
        [
            Item::Operation(tag::SAVE_LAYER_ALPHA),
            Item::Group { key: 1, version: 1, content: vec![Item::Operation(tag::DRAW_RECT)] },
            Item::Operation(tag::RESTORE),
        ]
    );
    // The rect's color, whole: in the group's record, first (D323), after its header (21
    // bytes), the rect's tag and its bounds.
    assert_eq!(u32_at(&out.bytes, 21 + 1 + 16), 0xFF00_0000);
}

/// A group with a layer, by hand: its record is its content where its boundary is, its layer
/// its `LAYERS` entry (no operation of it in the record).
#[test]
fn a_record_is_its_content_its_layer_an_entry() {
    let rect = || DrawOp::DrawRect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::BLACK));
    let mut content = DisplayList::default();
    content.push(rect());
    let mut list = DisplayList::default();
    list.push(DrawOp::Translate(Offset::new(2.0, 2.0)));
    list.boundaries.push(Boundary { at: 1, key: 9 });
    list.push(rect());
    let layer = LayerProperties {
        alpha: 0.5,
        pivot: Offset::new(2.0, 2.0),
        scale: (2.0, 2.0),
        rotation: 0.0,
        translation: Offset::ZERO,
        clip: false,
        outline: None,
        shadow_elevation: 0.0,
        ambient_shadow_color: Color::BLACK,
        spot_shadow_color: Color::BLACK,
        compositing_strategy: CompositingStrategy::Auto,
        bounds: Rect::new(0.0, 0.0, 4.0, 4.0),
        density: 1.0,
    };
    list.groups.push(group(9, 3, Some(layer), content));
    let (mut resources, mut out) = (Resources::default(), Writer::default());
    write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
    let (layers, items) = read(&out.bytes);
    assert_eq!(layers.len(), 1);
    assert_eq!((layers[0].key, layers[0].scale, layers[0].clip), (9, (2.0, 2.0), false));
    assert_eq!(
        items,
        [
            Item::Operation(tag::TRANSLATE),
            Item::Group { key: 9, version: 3, content: vec![Item::Operation(tag::DRAW_RECT)] },
            Item::Operation(tag::DRAW_RECT),
        ]
    );
}

// ---- Resources -------------------------------------------------------------------------------

/// A copied record names the paths its group drew with the ids they were defined with: no
/// definition again while the group keeps them, and a new path (its group drawn again)
/// defined, the old one dropped.
#[test]
fn a_copied_record_keeps_its_paths_ids() {
    let square = Path::from_svg("M0,0 L10,0 L10,10 Z").unwrap().data();
    let triangle = Path::from_svg("M0,0 L10,10 L0,10 Z").unwrap().data();
    let current = mutable_state_of(square.clone());
    let color = mutable_state_of(Color::RED);
    let (path_handle, color_handle) = (current.handle(), color.handle());
    let mut serialized = Serialized::new(move || {
        Layout(
            Modifier,
            stack(),
            callback(move || {
                Leaf(
                    Modifier
                        .repaint_boundary()
                        .draw_behind(move |scope| {
                            scope.draw_path(&path_handle.get(), Paint::fill(Color::BLACK))
                        })
                        .size(10.dp()),
                );
                Leaf(
                    Modifier
                        .repaint_boundary()
                        .then(painted(move || color_handle.get()))
                        .size(10.dp()),
                );
            }),
        );
    });
    let mut pending = Writer::default();
    serialized.resources.write_pending(&mut pending);
    assert_eq!(pending.bytes[0], tag::DEFINE_PATH);
    let defined = u32_at(&pending.bytes, 1);
    // The sibling changes: the path's group is copied, its id the same, nothing to define.
    color.set(Color::GREEN);
    let bytes = serialized.frame();
    assert_eq!(bytes, serialized.fresh());
    assert!(!serialized.resources.has_pending());
    let at = find(&bytes, tag::DRAW_PATH).expect("a path drawn");
    assert_eq!(u32_at(&bytes, at + 1), defined);
    // The path changes: its group is written again with the new one's id.
    current.set(triangle.clone());
    drop(square);
    let bytes = serialized.frame();
    assert_eq!(bytes, serialized.fresh());
    let mut pending = Writer::default();
    serialized.resources.write_pending(&mut pending);
    // The old one's drop (its owner gone), then the new path's definition (D410).
    assert_eq!((pending.bytes[0], u32_at(&pending.bytes, 1)), (tag::DROP_PATH, defined));
    assert_eq!(pending.bytes[5], tag::DEFINE_PATH);
    let new_id = u32_at(&pending.bytes, 6);
    assert_ne!(new_id, defined);
    let at = find(&bytes, tag::DRAW_PATH).expect("a path drawn");
    assert_eq!(u32_at(&bytes, at + 1), new_id);
}

/// The (tag, id) of each resource change in `pending` (paths only, as drawn here).
fn path_changes(pending: &[u8]) -> Vec<(u8, u32)> {
    let (mut changes, mut at) = (Vec::new(), 0);
    while at < pending.len() {
        let (tag, id) = (pending[at], u32_at(pending, at + 1));
        changes.push((tag, id));
        at += match tag {
            tag::DROP_PATH => 5,
            // An id, a fill type, no step, a count, then each command: a kind and its points.
            tag::DEFINE_PATH => {
                let count = u32_at(pending, at + 10) as usize;
                let mut end = at + 14;
                for _ in 0..count {
                    end += 1 + match pending[end] {
                        0 | 1 => 8,
                        2 => 16,
                        3 => 24,
                        _ => 0,
                    };
                }
                end - at
            }
            other => panic!("not a path change: {other}"),
        };
    }
    changes
}

/// Fixed contents (an `Arc<PathData>`, as an icon's) keep their id while the drawing around them
/// changes each frame; a new `Path` built in the drawing block each frame (not Compose's way: the
/// guide's `Heartbeat` remembers and resets one, D533) is a new path each frame, the last
/// frame's dropped before it's defined (D410).
#[test]
fn a_remembered_path_keeps_its_id_and_a_drawn_one_is_replaced() {
    let heart = Path::from_svg("M0,0 L10,0 L10,10 Z").unwrap().data();
    let phase = mutable_state_of(0.0f32);
    let handle = phase.handle();
    let mut serialized = Serialized::new(move || {
        let heart = heart.clone();
        Leaf(
            Modifier
                .draw_behind(move |scope| {
                    scope.draw_path(&heart, Paint::fill(Color::RED));
                    let trace = Path::new();
                    trace.move_to(0.0, handle.get());
                    trace.line_to(10.0, 0.0);
                    scope.draw_path(&trace, Paint::stroke(Color::BLACK, 1.0));
                })
                .size(10.dp()),
        );
    });
    let mut pending = Writer::default();
    serialized.resources.write_pending(&mut pending);
    let first = path_changes(&pending.bytes);
    assert_eq!(first.iter().map(|change| change.0).collect::<Vec<_>>(), [tag::DEFINE_PATH; 2]);
    let (heart_id, trace_id) = (first[0].1, first[1].1);
    for frame in 1..4 {
        phase.set(frame as f32);
        serialized.frame();
        let mut pending = Writer::default();
        serialized.resources.write_pending(&mut pending);
        let changes = path_changes(&pending.bytes);
        assert_eq!(changes.len(), 2, "{changes:?}");
        assert_eq!(changes[0].0, tag::DROP_PATH);
        assert_ne!(changes[0].1, heart_id);
        assert_eq!(changes[1].0, tag::DEFINE_PATH);
        if frame == 1 {
            assert_eq!(changes[0].1, trace_id);
        }
    }
}

/// A remembered `Path` reset and rebuilt each frame, as Compose code does (D530): each frame
/// defines it again under the same id, nothing dropped, so the replayer refills the same
/// `android.graphics.Path` (D532); the group drawing it gets a new version each time.
#[test]
fn a_path_reset_each_frame_is_redefined_in_place() {
    let phase = mutable_state_of(0.0f32);
    let handle = phase.handle();
    let mut serialized = Serialized::new(move || {
        let path = remember(Path::new);
        Leaf(
            Modifier
                .repaint_boundary()
                .draw_behind(move |scope| {
                    path.reset();
                    path.move_to(0.0, handle.get());
                    path.line_to(10.0, 0.0);
                    scope.draw_path(&path, Paint::stroke(Color::BLACK, 1.0));
                })
                .size(10.dp()),
        );
    });
    let mut pending = Writer::default();
    serialized.resources.write_pending(&mut pending);
    let first = path_changes(&pending.bytes);
    assert_eq!(first.len(), 1);
    let (id, mut version) = (first[0].1, groups(&read(&serialized.out.bytes).1)[0].1);
    let mut replay = PathReplay::default();
    replay.take_bytes(&pending.bytes, &serialized.out.bytes);
    for frame in 1..4 {
        phase.set(frame as f32);
        let bytes = serialized.frame();
        assert_eq!(bytes, serialized.fresh());
        let mut pending = Writer::default();
        serialized.resources.write_pending(&mut pending);
        assert_eq!(path_changes(&pending.bytes), [(tag::DEFINE_PATH, id)], "frame {frame}");
        // The same bytes but its version would draw the new contents: a new version.
        replay.take_bytes(&pending.bytes, &bytes);
        let now = groups(&read(&bytes).1)[0].1;
        assert_ne!(now, version, "a new version for a new path");
        version = now;
    }
    // Unchanged: nothing defined, the version kept.
    let bytes = serialized.frame();
    assert!(!serialized.resources.has_pending());
    assert_eq!(groups(&read(&bytes).1)[0].1, version);
}

/// Each path a record draws: its id and its definition's bytes.
type DrawnPaths = Vec<(u32, Vec<u8>)>;

/// What the replayer holds as frames are taken: each path id's definition, and each record's
/// paths' definitions when it was first seen under its key and version.
#[derive(Default)]
struct PathReplay {
    defined: HashMap<u32, Vec<u8>>,
    recorded: HashMap<(u64, u64), DrawnPaths>,
}

impl PathReplay {
    /// Takes `serialized`'s pending changes and frame, as `takeDrawing` hands them to Kotlin,
    /// and checks that each record seen before under its key and version still draws the paths
    /// it drew then: one recorded again from these bytes would draw what it first did (D157).
    fn take(&mut self, serialized: &mut Serialized) {
        let mut pending = Writer::default();
        serialized.resources.write_pending(&mut pending);
        let frame = serialized.out.bytes.clone();
        self.take_bytes(&pending.bytes, &frame);
    }

    /// [`take`](PathReplay::take) with the pending changes and the frame given.
    fn take_bytes(&mut self, bytes: &[u8], frame: &[u8]) {
        let mut at = 0;
        while at < bytes.len() {
            let id = u32_at(bytes, at + 1);
            match bytes[at] {
                tag::DROP_PATH => {
                    self.defined.remove(&id);
                    at += 5;
                }
                tag::DEFINE_PATH => {
                    assert_eq!(u32_at(bytes, at + 6), 0, "no step");
                    let mut end = at + 14;
                    for _ in 0..u32_at(bytes, at + 10) {
                        end += 1 + 8 * [1, 1, 2, 3, 0][bytes[end] as usize];
                    }
                    self.defined.insert(id, bytes[at + 5..end].to_vec());
                    at = end;
                }
                other => panic!("not a path change: {other}"),
            }
        }
        for ((key, version), content) in records(frame) {
            let mut drawn = Vec::new();
            let mut at = 0;
            while at < content.len() {
                if matches!(content[at], tag::DRAW_PATH | tag::CLIP_PATH) {
                    let id = u32_at(&content, at + 1);
                    drawn.push((id, self.defined.get(&id).expect("a defined path").clone()));
                }
                at += 1 + operand_bytes(&content, at);
            }
            let first = self.recorded.entry((key, version)).or_insert_with(|| drawn.clone());
            assert_eq!(*first, drawn, "group {key} version {version} draws other paths");
        }
    }
}

/// Two groups drawing one `Path`, the second resetting it each time it draws: the first keeps
/// what it drew (D531), its record, copied, naming its path's id with those contents (the
/// changed path is defined under another id, D532). No record seen again under its key and
/// version names contents other than those it named first, so a group Kotlin records again from
/// cached bytes never draws a newer path.
#[test]
fn a_cached_record_never_draws_a_newer_path() {
    let path = Path::new();
    let lines = mutable_state_of(1u32);
    let handle = lines.handle();
    let mut serialized = Serialized::new(move || {
        let (first, second) = (path.clone(), path.clone());
        Layout(
            Modifier,
            stack(),
            callback(move || {
                let first = first.clone();
                Leaf(
                    Modifier
                        .repaint_boundary()
                        .draw_behind(move |scope| {
                            scope.draw_path(&first, Paint::stroke(Color::BLACK, 1.0))
                        })
                        .size(10.dp()),
                );
                let second = second.clone();
                Leaf(
                    Modifier
                        .repaint_boundary()
                        .draw_behind(move |scope| {
                            second.reset();
                            second.move_to(0.0, 0.0);
                            for line in 0..handle.get() {
                                second.line_to(line as f32, 10.0);
                            }
                            scope.draw_path(&second, Paint::stroke(Color::BLACK, 1.0))
                        })
                        .size(10.dp()),
                );
            }),
        );
    });
    let mut replay = PathReplay::default();
    replay.take(&mut serialized);
    for count in [2, 3, 4] {
        lines.set(count);
        let bytes = serialized.frame();
        assert_eq!(bytes, serialized.fresh());
        replay.take(&mut serialized);
        assert!(!serialized.host.has_pending_work(), "settled");
        let drawn: Vec<usize> = serialized.host.with_tree(|tree| {
            tree.display_list()
                .flatten()
                .ops
                .iter()
                .filter_map(|op| match op {
                    DrawOp::DrawPath { path, .. } => Some(path.commands().len()),
                    _ => None,
                })
                .collect()
        });
        assert_eq!(drawn, [0, count as usize + 1], "each group draws what it drew");
    }
}

// ---- Groups written ahead (D350) -------------------------------------------------------------

/// Each `GROUP` record in `bytes` (after a `LAYERS` section, if any): its content's bytes by
/// key and version.
fn records(bytes: &[u8]) -> HashMap<(u64, u64), Vec<u8>> {
    let mut at = if bytes.first() == Some(&tag::LAYERS) {
        5 + u32_at(bytes, 1) as usize * LAYER_ENTRY_BYTES
    } else {
        0
    };
    let mut records = HashMap::new();
    while bytes.get(at) == Some(&tag::GROUP) {
        let (key, version) = (u64_at(bytes, at + 1), u64_at(bytes, at + 9));
        let length = u32_at(bytes, at + 17) as usize;
        records.insert((key, version), bytes[at + 21..at + 21 + length].to_vec());
        at += 21 + length;
    }
    records
}

/// The `LAYERS` section's keys.
fn layer_keys(bytes: &[u8]) -> Vec<u64> {
    if bytes.first() != Some(&tag::LAYERS) {
        return Vec::new();
    }
    let count = u32_at(bytes, 1) as usize;
    (0..count).map(|index| u64_at(bytes, 5 + index * LAYER_ENTRY_BYTES)).collect()
}

impl Serialized {
    /// Prefetches, then writes ahead every item drawn ahead, as `bridge.rs` does between frames:
    /// each one's bytes.
    fn prefetch_and_write_ahead(&mut self) -> Vec<Vec<u8>> {
        self.host.prefetch(std::time::Instant::now() + std::time::Duration::from_secs(60));
        let mut written = Vec::new();
        while let Some(groups) = self.host.draw_ahead() {
            let mut out = Writer::default();
            write_groups_ahead(&groups, &mut self.resources, &mut self.cache, &mut out);
            assert!(self.cache.items_waiting_ahead() <= AHEAD_CAPACITY);
            written.push(out.bytes);
        }
        written
    }
}

/// A feed: a counter above a lazy list of items, each a text and a painted leaf, every third
/// with a faded graphics layer (a nested group with a `LAYERS` entry).
fn feed(state: compose_foundation::LazyListState, count: MutableState<u32>) {
    use compose_foundation::{layout::*, *};
    Column!({
        BasicText!("count {}", count.get());
        LazyColumn!(
            modifier = Modifier.size((160.dp(), 150.dp())),
            state = state,
            |scope: LazyListScope| {
                scope.items(300, |index| {
                    Column!({
                        BasicText!("author {index}");
                        Leaf(
                            painted(move || Color::from_argb(0xFF00_0000 | index as u32))
                                .size((100.dp(), 10.dp())),
                        );
                        if index % 3 == 0 {
                            Leaf(
                                Modifier
                                    .graphics_layer_with(GraphicsLayerScope {
                                        alpha: 0.5,
                                        ..GraphicsLayerScope::default()
                                    })
                                    .then(painted(|| Color::RED))
                                    .size(10.dp()),
                            );
                        }
                    });
                });
            }
        );
    });
}

/// Items drawn ahead between frames, written ahead (D350): each record is the bytes the frame
/// placing it writes for its key and version (that frame copies it, and a write with nothing
/// cached gives the same), kept until that frame though frames between don't draw it; the
/// layers of those with one are listed ahead too, and again by the frame.
#[test]
fn groups_written_ahead_are_what_the_frame_placing_them_writes() {
    let state = compose_foundation::LazyListState::new(0, 0);
    let count = mutable_state_of(0u32);
    let (given, shown) = (*state, count.handle());
    let mut serialized = Serialized::with(legacy(), move || feed(given, shown));
    let mut ahead: HashMap<(u64, u64), Vec<u8>> = HashMap::new();
    let (mut kept, mut copied, mut layered) = (0, 0, 0);
    for step in 0..90 {
        let delta = match step % 9 {
            6 => -40.0,
            _ => 17.0,
        };
        state.scroll_by(delta);
        let bytes = serialized.frame();
        copied += serialized.cache.copied_ahead();
        let cached = without_kept_records(bytes.clone());
        assert_eq!(unversioned_bytes(&cached), unversioned_bytes(&serialized.fresh()));
        for (group, content) in records(&bytes) {
            if let Some(written) = ahead.get(&group) {
                assert_eq!(*written, content, "step {step}: {group:x?} as written ahead");
                kept += 1;
            }
        }
        if step % 5 == 4 {
            // A frame placing no item: the records written ahead wait.
            count.set(step);
            let waiting = serialized.cache.waiting_ahead();
            serialized.frame();
            assert_eq!(serialized.cache.copied_ahead(), 0);
            assert_eq!(serialized.cache.waiting_ahead(), waiting, "step {step}");
        }
        for written in serialized.prefetch_and_write_ahead() {
            let records = records(&written);
            assert!(!records.is_empty(), "an item's records");
            layered += layer_keys(&written).len();
            for key in layer_keys(&written) {
                assert!(records.keys().any(|(group, _)| *group == key), "a layer of its own");
            }
            ahead.extend(records);
        }
    }
    assert!(kept > 40, "records written ahead kept: {kept}");
    // 15 (31 before D375): after each scroll back, fewer items are prefetched and written
    // ahead again; those kept composed beside the wanted ones (D375) are written by the frames
    // placing them again.
    assert!(copied > 10, "records copied from those written ahead: {copied}");
    assert!(layered > 5, "layers listed ahead: {layered}");
}

/// At most `AHEAD_CAPACITY` records written ahead wait, the oldest dropped first: the frame
/// drawing one dropped writes it again, with the same key, version and bytes.
#[test]
fn records_written_ahead_are_bounded() {
    let group = |key: u64| {
        let mut content = DisplayList::default();
        content
            .push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 1.0, key as f32), Paint::fill(Color::RED)));
        DrawGroup {
            key,
            version: key + 100,
            layer: None,
            content: Arc::new(content),
            descendants: 0,
            empty: false,
        }
    };
    let (mut resources, mut cache) = (Resources::default(), GroupCache::default());
    let mut written = HashMap::new();
    for key in 1..=AHEAD_CAPACITY as u64 + 4 {
        let mut out = Writer::default();
        write_groups_ahead(&[group(key)], &mut resources, &mut cache, &mut out);
        written.extend(records(&out.bytes));
    }
    assert_eq!(cache.waiting_ahead(), AHEAD_CAPACITY);
    assert_eq!(cache.len(), AHEAD_CAPACITY);
    // A frame drawing the first (dropped) and the last (waiting).
    let mut frame = DisplayList::default();
    for key in [1, AHEAD_CAPACITY as u64 + 4] {
        frame.boundaries.push(Boundary { at: 0, key });
        frame.groups.push(group(key));
    }
    let mut out = Writer::default();
    write_display_list(&frame, &mut resources, &mut cache, &mut out);
    assert_eq!((cache.copied_ahead(), cache.written()), (1, 1));
    for (group, content) in records(&out.bytes) {
        assert_eq!(written.get(&group), Some(&content), "{group:?}");
    }
    // The ones still waiting stay past this frame; the two it drew are a frame's now.
    assert_eq!(cache.waiting_ahead(), AHEAD_CAPACITY - 1);
    assert_eq!(cache.len(), AHEAD_CAPACITY + 1);

    // The bound counts items, not groups: one item with many nested groups keeps them all.
    let mut item: Vec<DrawGroup> = (200..240).map(group).collect();
    item[0].descendants = item.len() - 1;
    let mut out = Writer::default();
    write_groups_ahead(&item, &mut resources, &mut cache, &mut out);
    assert_eq!(records(&out.bytes).len(), 40);
    assert_eq!(cache.items_waiting_ahead(), AHEAD_CAPACITY);
    assert_eq!(cache.waiting_ahead(), AHEAD_CAPACITY - 1 + 40);
}

// ---- Kept groups (D380) ----------------------------------------------------------------------

/// The items a lazy list keeps composed beside the wanted ones and doesn't place (D375): each
/// frame names their groups (`KEPT`), outer and nested, without their records, which stay
/// cached, so the replayer keeps their nodes recorded; the frame placing one again draws it at
/// the version it was last drawn at, its record the same bytes, copied: nothing is recorded
/// again.
#[test]
fn a_kept_item_placed_again_is_drawn_from_its_kept_record() {
    let state = compose_foundation::LazyListState::new(0, 0);
    let count = mutable_state_of(0u32);
    let (given, shown) = (*state, count.handle());
    let mut serialized = Serialized::with(legacy(), move || feed(given, shown));
    // Each group's record as last drawn (version and bytes), and the frames that kept it since.
    let mut last_drawn: HashMap<u64, (u64, Vec<u8>)> = HashMap::new();
    let mut kept_for: HashMap<u64, usize> = HashMap::new();
    let (mut placed_again, mut nested) = (0, 0);
    for step in 0..60 {
        state.scroll_by(if step % 10 >= 7 { -23.0 } else { 19.0 });
        let bytes = serialized.frame();
        let records = records(&bytes);
        let (_, items) = read(&bytes);
        let drawn = groups(&items);
        let entries = kept_entries(&bytes);
        assert!(entries.iter().filter(|(_, outermost)| *outermost).count() <= KEPT_CAPACITY);
        for (key, version) in &drawn {
            let content = records.get(&(*key, *version)).expect("a drawn group's record");
            if let Some(frames) = kept_for.remove(key) {
                // Placed again: at its kept version, the same bytes.
                assert!(frames >= 1);
                assert_eq!(last_drawn[key], (*version, content.clone()), "step {step}: {key:x}");
                placed_again += 1;
            }
            last_drawn.insert(*key, (*version, content.clone()));
        }
        let listed: HashSet<u64> = entries.iter().map(|(key, _)| *key).collect();
        for (key, outermost) in &entries {
            assert!(!drawn.iter().any(|(group, _)| group == key), "a kept group isn't drawn");
            assert!(last_drawn.contains_key(key), "kept as an earlier frame drew it");
            // Its record stays cached, not written.
            assert!(!records.keys().any(|(group, _)| group == key), "step {step}: {key:x}");
            assert_eq!(serialized.cache.cached_written_version(*key), Some(last_drawn[key].0));
            *kept_for.entry(*key).or_default() += 1;
            nested += usize::from(!outermost);
        }
        // Those no longer named are gone (disposed): not placed again from a record.
        kept_for.retain(|key, _| listed.contains(key));
        // A group drawn ahead is drawn: an item reusing a node that left (D430) is drawn ahead
        // in that node's group, kept then as drawn ahead.
        for bytes in serialized.prefetch_and_write_ahead() {
            for ((key, version), content) in self::records(&bytes) {
                last_drawn.insert(key, (version, content));
            }
        }
    }
    assert!(placed_again >= 5, "kept groups placed again: {placed_again}");
    assert!(nested > 0, "nested kept groups named: {nested}");
}

/// A kept item that changes while not placed is no longer listed: the frame placing it draws
/// it again, as before.
#[test]
fn a_kept_item_that_changes_is_no_longer_kept() {
    let state = compose_foundation::LazyListState::new(0, 0);
    let changed = mutable_state_of(0u32);
    let (given, read_changed) = (*state, changed.handle());
    let mut serialized = Serialized::with(legacy(), move || {
        use compose_foundation::*;
        LazyColumn!(
            modifier = Modifier.size((160.dp(), 150.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(300, move |index| {
                    Leaf(
                        painted(move || {
                            Color::from_argb(0xFF00_0000 | (index as u32 + read_changed.get()))
                        })
                        .size((100.dp(), 30.dp())),
                    );
                });
            }
        );
    });
    for _ in 0..10 {
        state.scroll_by(19.0);
        serialized.frame();
        serialized.prefetch_and_write_ahead();
    }
    count_kept(&mut serialized);
    // Every item draws again: none stays kept.
    changed.set(1);
    let bytes = serialized.frame();
    assert_eq!(kept_keys(&bytes), Vec::<u64>::new());
    assert_eq!(serialized.host.with_tree(|tree| tree.kept_count()), 0);
}

/// The last frame lists kept items.
fn count_kept(serialized: &mut Serialized) {
    let listed = kept_keys(&serialized.out.bytes).len();
    assert!(listed > 0, "items kept");
    assert_eq!(serialized.host.with_tree(|tree| tree.kept_count()), listed);
}

/// A kept item whose nested graphics layer reads state written while it isn't placed is no
/// longer kept: nothing runs for an unplaced item (as in Compose), and the frame placing it
/// again draws it.
#[test]
fn a_kept_item_whose_nested_layer_changes_is_no_longer_kept() {
    let state = compose_foundation::LazyListState::new(0, 0);
    let alpha = mutable_state_of(0.5f32);
    let (given, read_alpha) = (*state, alpha.handle());
    let mut serialized = Serialized::with(legacy(), move || {
        use compose_foundation::*;
        LazyColumn!(
            modifier = Modifier.size((160.dp(), 150.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(300, move |_| {
                    Layout(
                        Modifier.size((100.dp(), 30.dp())),
                        stack(),
                        callback(move || {
                            Leaf(
                                Modifier
                                    .graphics_layer(move |layer| layer.alpha = read_alpha.get())
                                    .then(painted(|| Color::RED))
                                    .size(10.dp()),
                            );
                        }),
                    );
                });
            }
        );
    });
    for _ in 0..10 {
        state.scroll_by(19.0);
        serialized.frame();
        serialized.prefetch_and_write_ahead();
    }
    count_kept(&mut serialized);
    alpha.set(0.25);
    let bytes = serialized.frame();
    assert_eq!(kept_keys(&bytes), Vec::<u64>::new());
}

/// Without `set_keep_unplaced_groups` (a renderer drawing the tree), frames list no kept group.
#[test]
fn frames_list_kept_groups_only_when_asked() {
    let state = compose_foundation::LazyListState::new(0, 0);
    let count = mutable_state_of(0u32);
    let (given, shown) = (*state, count.handle());
    let mut serialized = Serialized::new(move || feed(given, shown));
    serialized.host.set_keep_unplaced_groups(false);
    for _ in 0..10 {
        state.scroll_by(19.0);
        let bytes = serialized.frame();
        assert!(kept_entries(&bytes).is_empty());
        assert!(serialized.host.with_tree(|tree| tree.display_list().kept.is_empty()));
        serialized.prefetch_and_write_ahead();
    }
}

/// A kept group's record is written only when the cache doesn't hold its version (it changed
/// since a frame wrote it); else it stays cached, the `KEPT` section naming it (D380).
#[test]
fn a_kept_groups_record_is_written_only_for_a_new_version() {
    let group = |key: u64, version: u64, descendants: usize| {
        let mut content = DisplayList::default();
        // The nested group draws something else at each version; the item, the same.
        let size = if descendants > 0 { 1.0 } else { version as f32 };
        content.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, size, size), Paint::fill(Color::RED)));
        if descendants > 0 {
            content.boundaries.push(Boundary { at: 1, key: key + 1 });
        }
        DrawGroup {
            key,
            version,
            layer: None,
            content: Arc::new(content),
            descendants,
            empty: false,
        }
    };
    let (mut resources, mut cache) = (Resources::default(), GroupCache::default());
    // A frame drawing an item (10) with a nested group (11).
    let mut frame = DisplayList::default();
    frame.boundaries.push(Boundary { at: 0, key: 10 });
    frame.groups = vec![group(10, 1, 1), group(11, 1, 0)];
    let mut out = Writer::default();
    write_display_list(&frame, &mut resources, &mut cache, &mut out);
    // Kept, unchanged: no record, both named, the outer one outermost.
    let mut kept = DisplayList::default();
    kept.kept = vec![group(10, 1, 1), group(11, 1, 0)];
    let mut out = Writer::default();
    write_display_list(&kept, &mut resources, &mut cache, &mut out);
    assert!(records(&out.bytes).is_empty());
    assert_eq!(kept_entries(&out.bytes), vec![(10, true), (11, false)]);
    assert_eq!((cache.cached_version(10), cache.cached_version(11)), (Some(1), Some(1)));
    // The nested one at a new version: its record, and its parent's (its version follows).
    kept.kept = vec![group(10, 2, 1), group(11, 2, 0)];
    let mut out = Writer::default();
    write_display_list(&kept, &mut resources, &mut cache, &mut out);
    let written: HashSet<(u64, u64)> = records(&out.bytes).into_keys().collect();
    assert_eq!(written, HashSet::from([(10, 1), (11, 2)]), "the parent's bytes are the same");
    assert_eq!(kept_entries(&out.bytes), vec![(10, true), (11, false)]);
    assert_eq!(cache.cached_version(11), Some(2));
    // No longer kept nor drawn: gone from the cache.
    let mut out = Writer::default();
    write_display_list(&DisplayList::default(), &mut resources, &mut cache, &mut out);
    assert_eq!((cache.cached_version(10), cache.cached_version(11)), (None, None));
}

// ---- Layers as Compose's (D522-D525) ---------------------------------------------------------

/// `Modifier.clip(shape)` is a graphics layer clipping to its outline, as Compose's
/// `graphicsLayer(shape = shape, clip = true)`: its node's group has the outline in its `LAYERS`
/// entry (a render node's `setClipToOutline`), and its record holds no clip.
#[test]
fn a_clip_is_a_layer_clipping_to_its_outline() {
    let mut serialized = Serialized::new(|| {
        Leaf(
            Modifier
                .size(20.dp())
                .clip(compose_foundation::RoundedCornerShape(4.dp()))
                .then(painted(|| Color::RED)),
        );
    });
    let (layers, items) = read(&serialized.frame());
    assert_eq!(
        layers,
        [Layer {
            clip: true,
            outline: Some((2, (0.0, 0.0, 20.0, 20.0), [4.0; 4])),
            ..Layer::plain(layers[0].key, (0.0, 0.0, 20.0, 20.0))
        }]
    );
    let operations = all_operations(&items);
    for clip in [tag::CLIP_RECT, tag::CLIP_ROUND_RECT, tag::CLIP_PATH, tag::SAVE] {
        assert!(!operations.contains(&clip), "{clip} written: {items:?}");
    }
    // `clip_to_bounds`: a rectangle.
    let mut serialized = Serialized::new(|| {
        Leaf(Modifier.size(20.dp()).clip_to_bounds().then(painted(|| Color::RED)));
    });
    let (layers, _) = read(&serialized.frame());
    assert_eq!(
        (layers[0].clip, layers[0].outline),
        (true, Some((1, (0.0, 0.0, 20.0, 20.0), [0.0; 4])))
    );
}

/// `Modifier.shadow` is a graphics layer with an elevation, an outline and the shadows' colors
/// (a render node's `setElevation`, `setOutline`, `setAmbientShadowColor`, `setSpotShadowColor`:
/// HWUI draws the shadow), clipping if asked; no drawing of it in any record.
#[test]
fn a_shadow_is_its_layers_elevation_outline_and_colors() {
    let shape = compose_foundation::RoundedCornerShape(5.dp());
    let mut serialized = Serialized::new(move || {
        Leaf(
            Modifier
                .size(30.dp())
                .shadow_with(6.dp(), shape, false, Color::RED, Color::BLUE)
                .then(painted(|| Color::WHITE)),
        );
    });
    let (layers, items) = read(&serialized.frame());
    assert_eq!(
        layers,
        [Layer {
            outline: Some((2, (0.0, 0.0, 30.0, 30.0), [5.0; 4])),
            elevation: 6.0,
            ambient: Color::RED.argb(),
            spot: Color::BLUE.argb(),
            ..Layer::plain(layers[0].key, (0.0, 0.0, 30.0, 30.0))
        }]
    );
    assert!(!all_operations(&items).contains(&tag::DRAW_SHADOW), "{items:?}");
    // Clipping too: the same outline, clipped to.
    let mut serialized = Serialized::new(move || {
        Leaf(Modifier.size(30.dp()).shadow_with(6.dp(), shape, true, Color::BLACK, Color::BLACK));
    });
    let (layers, _) = read(&serialized.frame());
    assert!(layers[0].clip && layers[0].elevation == 6.0, "{layers:?}");
    // With nothing in it, its shadow is still drawn, as a Compose layer's.
    let mut serialized = Serialized::new(move || {
        Leaf(Modifier.size(30.dp()).shadow_with(6.dp(), shape, false, Color::BLACK, Color::BLACK));
    });
    let (layers, _) = read(&serialized.frame());
    assert!(!layers[0].clip && layers[0].elevation == 6.0, "{layers:?}");
    // No elevation and no clip: no layer, as Compose's.
    let mut serialized = Serialized::new(|| {
        Leaf(Modifier.size(30.dp()).shadow_with(
            0.dp(),
            RECTANGLE_SHAPE,
            false,
            Color::BLACK,
            Color::BLACK,
        ));
    });
    assert!(read(&serialized.frame()).0.is_empty());
}

/// Each `CompositingStrategy` is its byte: `Auto` (offscreen for an alpha below 1, by HWUI),
/// `Offscreen` (a compositing layer) and `ModulateAlpha` (no offscreen buffer), whatever the
/// content; with `SINGLE_DRAWING_FADE`, an `Auto` alpha over one drawing is `ModulateAlpha` in
/// its node's bounds (the offscreen buffer's).
#[test]
fn compositing_strategies_are_their_bytes() {
    let compositing = |strategy: CompositingStrategy, optimizations: Optimizations| {
        let mut serialized = Serialized::with(optimizations, move || {
            Leaf(
                Modifier
                    .size(10.dp())
                    .graphics_layer_with(GraphicsLayerScope {
                        alpha: 0.5,
                        compositing_strategy: strategy,
                        ..GraphicsLayerScope::default()
                    })
                    .then(painted(|| Color::RED)),
            );
        });
        read(&serialized.frame()).0[0].compositing
    };
    let (none, fade) =
        (Optimizations::NONE, Optimizations::NONE.with(Optimization::SINGLE_DRAWING_FADE, true));
    assert_eq!(compositing(CompositingStrategy::Auto, none), compositing::AUTO);
    assert_eq!(compositing(CompositingStrategy::Offscreen, none), compositing::OFFSCREEN);
    assert_eq!(compositing(CompositingStrategy::ModulateAlpha, none), compositing::MODULATE_ALPHA);
    assert_eq!(compositing(CompositingStrategy::Auto, fade), compositing::MODULATE_ALPHA_IN_BOUNDS);
    assert_eq!(compositing(CompositingStrategy::Offscreen, fade), compositing::OFFSCREEN);
}

/// A layer after a drawing (`background(…).clip(shape)`, a surface's) is a group of its own
/// (an inner layer): the background is drawn by what draws the node, the layer's group by
/// reference after it, holding what follows it; its properties are its `LAYERS` entry.
#[test]
fn a_layer_after_a_drawing_is_a_group_of_its_own() {
    let shape = compose_foundation::RoundedCornerShape(6.dp());
    let mut serialized = Serialized::new(move || {
        Leaf(
            Modifier
                .size(40.dp())
                .background_with(Color::GREEN, shape)
                .clip(shape)
                .then(painted(|| Color::RED)),
        );
    });
    let bytes = serialized.frame();
    let (layers, items) = read(&bytes);
    assert_eq!(layers.len(), 1);
    assert_eq!(
        (layers[0].clip, layers[0].outline.map(|outline| outline.2)),
        (true, Some([6.0; 4]))
    );
    assert_eq!(
        items,
        [
            Item::Operation(tag::DRAW_ROUND_RECT),
            Item::Group {
                key: layers[0].key,
                version: groups(&items)[0].1,
                content: vec![Item::Operation(tag::DRAW_RECT)],
            },
        ]
    );
}

/// An inner layer keeps its drawing, as Compose's layer does, while what draws its node draws
/// again for something else: its record is copied, its version the same.
#[test]
fn an_inner_layer_keeps_its_drawing_while_around_it_draws_again() {
    let tick = mutable_state_of(0u32);
    let handle = tick.handle();
    let mut serialized = Serialized::new(move || {
        Layout(
            Modifier,
            stack(),
            callback(move || {
                Leaf(
                    Modifier
                        .size(40.dp())
                        .background(Color::GREEN)
                        .clip(compose_foundation::RoundedCornerShape(6.dp()))
                        .then(painted(|| Color::RED)),
                );
                Leaf(Modifier.size(10.dp()).then(painted(move || {
                    if handle.get().is_multiple_of(2) { Color::BLUE } else { Color::BLACK }
                })));
            }),
        );
    });
    let first = serialized.frame();
    tick.set(1);
    let second = serialized.frame();
    assert_ne!(first, second, "the sibling drew again");
    let (before, after) = (read(&first).1, read(&second).1);
    assert_eq!(groups(&after), groups(&before), "the layer kept its drawing");
    assert_eq!(serialized.cache.copied(), 1);
}

/// What the replayer reads in `Replayer.kt` and `ComposeRsOptimizations.kt` is what Rust
/// writes: a `LAYERS` entry's size, the compositing bytes, and each optimization's bit.
#[test]
fn the_kotlin_constants_are_rusts() {
    let kotlin = |file: &str| {
        let path = format!(
            "{}/../../android/compose-rs/src/main/java/rs/compose/android/{file}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
    };
    let replayer = kotlin("Replayer.kt");
    use layer_entry as e;
    for (name, value) in [
        ("LAYER_ENTRY_BYTES", LAYER_ENTRY_BYTES as u64),
        ("COMPOSITING_AUTO", compositing::AUTO as u64),
        ("COMPOSITING_OFFSCREEN", compositing::OFFSCREEN as u64),
        ("COMPOSITING_MODULATE_ALPHA", compositing::MODULATE_ALPHA as u64),
        ("COMPOSITING_MODULATE_ALPHA_IN_BOUNDS", compositing::MODULATE_ALPHA_IN_BOUNDS as u64),
        ("OUTLINE_NONE", outline_kind::NONE as u64),
        ("ENTRY_ALPHA", e::ALPHA as u64),
        ("ENTRY_PIVOT", e::PIVOT as u64),
        ("ENTRY_SCALE", e::SCALE as u64),
        ("ENTRY_ROTATION", e::ROTATION as u64),
        ("ENTRY_TRANSLATION", e::TRANSLATION as u64),
        ("ENTRY_CLIP", e::CLIP as u64),
        ("ENTRY_OUTLINE_KIND", e::OUTLINE_KIND as u64),
        ("ENTRY_OUTLINE_BOUNDS", e::OUTLINE_BOUNDS as u64),
        ("ENTRY_OUTLINE_RADII", e::OUTLINE_RADII as u64),
        ("ENTRY_ELEVATION", e::ELEVATION as u64),
        ("ENTRY_AMBIENT", e::AMBIENT as u64),
        ("ENTRY_SPOT", e::SPOT as u64),
        ("ENTRY_COMPOSITING", e::COMPOSITING as u64),
        ("ENTRY_BOUNDS", e::BOUNDS as u64),
        ("ENTRY_END", e::END as u64),
    ] {
        assert!(replayer.contains(&format!("const val {name} = {value}\n")), "{name} = {value}");
    }
    let optimizations = kotlin("ComposeRsOptimizations.kt");
    for optimization in Optimization::ALL {
        let name = optimization.name.to_uppercase();
        let bit = optimization.mask().trailing_zeros();
        assert!(
            optimizations.contains(&format!("const val {name} = 1L shl {bit}\n")),
            "{name} is bit {bit} in Kotlin"
        );
    }
    let defaults: Vec<String> = Optimization::ALL
        .iter()
        .filter(|optimization| optimization.default)
        .map(|optimization| optimization.name.to_uppercase())
        .collect();
    // The recommended set on by default is `RECOMMENDED` (D610), none `NONE` (as D581 had it).
    let default = if Optimizations::defaults() == Optimizations::recommended() {
        "RECOMMENDED".to_string()
    } else if defaults.is_empty() {
        "NONE".to_string()
    } else {
        defaults.join(" or ")
    };
    assert!(
        optimizations.contains(&format!("const val DEFAULT = {default}\n")),
        "DEFAULT is {defaults:?}"
    );
    assert!(optimizations.contains("const val NONE = 0L\n"));
    let all: Vec<String> =
        Optimization::ALL.iter().map(|optimization| optimization.name.to_uppercase()).collect();
    assert!(
        optimizations.contains(&format!("const val ALL = {}\n", all.join(" or "))),
        "ALL is {all:?}"
    );
    // The recommended ones (D601), in Rust's table's order.
    let recommended: Vec<String> = Optimization::ALL
        .iter()
        .filter(|optimization| Optimizations::recommended().contains(**optimization))
        .map(|optimization| optimization.name.to_uppercase())
        .collect();
    assert!(!recommended.is_empty());
    assert!(
        optimizations.contains(&format!("const val RECOMMENDED = {}\n", recommended.join(" or "))),
        "RECOMMENDED is {recommended:?}"
    );
}

/// The optimizations Kotlin alone acts on switch what it does where it does it: the flush layer
/// (D566), paragraphs' render nodes (D567), recording ahead (D564) and the prefetch deadline
/// (D561).
#[test]
fn the_kotlin_side_switches_its_optimizations() {
    let kotlin = |file: &str| {
        let path = format!(
            "{}/../../android/compose-rs/src/main/java/rs/compose/android/{file}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
    };
    let replayer = kotlin("Replayer.kt");
    assert!(replayer.contains(
        "if (flush && optimizations and ComposeRsOptimizations.FLUSH_LAYERS_BEFORE_DROPPED_PATHS != 0L)"
    ));
    let view = kotlin("ComposeRsView.kt");
    for flag in
        ["PARAGRAPH_RENDER_NODES", "DRAW_PREFETCHED_AHEAD", "PREFETCH_BUDGET_RECENT_MAXIMUM"]
    {
        assert!(view.contains(&format!("and ComposeRsOptimizations.{flag} ")), "{flag}");
    }
    assert!(kotlin("TextMeasurer.kt").contains("if (!hardware || !paragraphNodes)"));
}

/// Android's entry points default to the recommended set (D610): a `ComposeRsView` not given
/// optimizations takes `ComposeRsOptimizations.DEFAULT` (`RECOMMENDED`, which
/// `the_kotlin_constants_are_rusts` checks is Rust's defaults), as bench-rs without its extra and
/// the demo app, which names none.
#[test]
fn android_hosts_default_to_the_recommended_set() {
    let android = |file: &str| {
        let path = format!("{}/../../android/{file}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
    };
    assert_eq!(Optimizations::defaults(), Optimizations::recommended());
    assert!(
        android("compose-rs/src/main/java/rs/compose/android/ComposeRsOptimizations.kt")
            .contains("const val DEFAULT = RECOMMENDED\n")
    );
    assert!(
        android("compose-rs/src/main/java/rs/compose/android/ComposeRsView.kt")
            .contains("    optimizations: Long = ComposeRsOptimizations.DEFAULT,\n")
    );
    assert!(
        android("bench-rs/src/main/java/rs/compose/bench/rs/MainActivity.kt")
            .contains("?: ComposeRsOptimizations.DEFAULT\n")
    );
    let demo = android("demo/src/main/java/rs/compose/demo/MainActivity.kt");
    assert!(demo.contains("ComposeRsView(context, apps[selected])"));
    assert!(!demo.contains("optimizations"), "the demo names no optimizations");
}

// ---- Mutable paths, after a review ----------------------------------------------------------

/// Two groups drawing one `Path`, changed outside drawing, then both drawn again in the same
/// frame (they read the same state): the first group's write defines the path again under its id
/// (D532); the second's names that id with the same bytes as its cached record, but the id was
/// defined since that record was, so it gets a new version too, and the replayer records it
/// again (a review found it kept its old version, Kotlin's node drawing the old contents).
#[test]
fn groups_naming_a_path_defined_again_since_get_new_versions() {
    let path = Path::new();
    path.move_to(0.0, 0.0);
    path.line_to(10.0, 0.0);
    let shared = path.clone();
    let tick = mutable_state_of(0u32);
    let handle = tick.handle();
    let mut serialized = Serialized::new(move || {
        let (first, second) = (shared.clone(), shared.clone());
        Layout(
            Modifier,
            stack(),
            callback(move || {
                let first = first.clone();
                Leaf(
                    Modifier
                        .repaint_boundary()
                        .draw_behind(move |scope| {
                            let _ = handle.get();
                            scope.draw_path(&first, Paint::stroke(Color::BLACK, 1.0))
                        })
                        .size(10.dp()),
                );
                let second = second.clone();
                Leaf(
                    Modifier
                        .repaint_boundary()
                        .draw_behind(move |scope| {
                            let _ = handle.get();
                            scope.draw_path(&second, Paint::stroke(Color::BLACK, 1.0))
                        })
                        .size(10.dp()),
                );
            }),
        );
    });
    let mut replay = PathReplay::default();
    replay.take(&mut serialized);
    let before = groups(&read(&serialized.out.bytes).1);
    // A handler changes the path, and both groups are drawn again.
    path.line_to(10.0, 10.0);
    tick.set(1);
    let bytes = serialized.frame();
    assert_eq!(bytes, serialized.fresh());
    replay.take(&mut serialized);
    let after = groups(&read(&bytes).1);
    assert!(before.iter().zip(&after).all(|(old, new)| old.0 == new.0 && old.1 != new.1));
}

/// Timing (`--release --ignored --nocapture`): one `Path` reset for each of `n` shapes in one
/// drawing, as Compose code draws particles: linear in `n` (a review found the serializer looked
/// each contents up among the path's others, n²).
#[test]
#[ignore]
fn timing_one_path_reset_for_many_shapes() {
    for n in [50u32, 500, 2000] {
        let phase = mutable_state_of(0.0f32);
        let handle = phase.handle();
        let mut serialized = Serialized::new(move || {
            let path = remember(Path::new);
            Leaf(
                Modifier
                    .repaint_boundary()
                    .draw_behind(move |scope| {
                        for shape in 0..n {
                            path.reset();
                            path.move_to(shape as f32, handle.get());
                            path.line_to(shape as f32 + 1.0, 0.0);
                            scope.draw_path(&path, Paint::stroke(Color::BLACK, 1.0));
                        }
                    })
                    .size(100.dp()),
            );
        });
        let mut pending = Writer::default();
        serialized.resources.write_pending(&mut pending);
        let frames = 20;
        let mut serialize = std::time::Duration::ZERO;
        for frame in 0..frames {
            phase.set(frame as f32 + 1.0);
            serialized.host.frame();
            let start = std::time::Instant::now();
            let (resources, cache, out) =
                (&mut serialized.resources, &mut serialized.cache, &mut serialized.out);
            serialized
                .host
                .with_tree(|tree| write_display_list(tree.display_list(), resources, cache, out));
            serialized.resources.collect();
            serialize += start.elapsed();
            let mut pending = Writer::default();
            serialized.resources.write_pending(&mut pending);
        }
        println!("n = {n}: serialize {:?} a frame", serialize / frames);
    }
}

// ---- Review of D520-D529 ---------------------------------------------------------------------

/// A lifted layer drawing nothing whose block's shadow elevation goes from 0 to above 0 (a
/// layer-only change: no version changes): its group goes from empty to drawing. The frame must
/// write what a fresh write of the same display list writes: its parent's record naming it.
#[test]
fn review_an_empty_layers_shadow_appearing_by_its_block_is_drawn() {
    let elevation = mutable_state_of(0.0f32);
    let handle = elevation.handle();
    let mut serialized = Serialized::new(move || {
        Layout(
            Modifier.repaint_boundary().then(painted(|| Color::WHITE)).size(50.dp()),
            stack(),
            callback(move || {
                Leaf(
                    Modifier
                        .graphics_layer(move |layer| layer.shadow_elevation = handle.get())
                        .size(20.dp()),
                );
            }),
        );
    });
    serialized.frame();
    elevation.set(8.0);
    let bytes = serialized.frame();
    let fresh = serialized.fresh();
    assert_eq!(
        unversioned_bytes(&bytes),
        unversioned_bytes(&fresh),
        "the parent's cached record doesn't name the layer now casting a shadow"
    );
}

/// The same, the other way: the shadow goes (elevation back to 0), the group empty again.
#[test]
fn review_an_empty_layers_shadow_going_by_its_block_matches_a_fresh_write() {
    let elevation = mutable_state_of(8.0f32);
    let handle = elevation.handle();
    let mut serialized = Serialized::new(move || {
        Layout(
            Modifier.repaint_boundary().then(painted(|| Color::WHITE)).size(50.dp()),
            stack(),
            callback(move || {
                Leaf(
                    Modifier
                        .graphics_layer(move |layer| layer.shadow_elevation = handle.get())
                        .size(20.dp()),
                );
            }),
        );
    });
    serialized.frame();
    elevation.set(0.0);
    let bytes = serialized.frame();
    let fresh = serialized.fresh();
    assert_eq!(unversioned_bytes(&bytes), unversioned_bytes(&fresh));
}

/// A feed of card-like items (inner clip layers holding children that read state, an inner
/// layer with a block, a lifted shadow), scrolled back and forth, state written while items
/// are placed, kept and drawn ahead: each frame writes what a fresh write does, and each
/// record written ahead is the frame's.
#[test]
fn review_inner_layers_in_kept_and_ahead_items_match_fresh_writes() {
    use compose_foundation::*;
    let state = LazyListState::new(0, 0);
    let tick = mutable_state_of(0u32);
    let (given, read) = (*state, tick.handle());
    let mut serialized = Serialized::with(legacy(), move || {
        LazyColumn!(
            modifier = Modifier.size((160.dp(), 150.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(300, move |index| {
                    Layout(
                        Modifier
                            .background(Color::WHITE)
                            .clip(RoundedCornerShape(4.dp()))
                            .size((100.dp(), 30.dp())),
                        stack(),
                        callback(move || {
                            Leaf(
                                painted(move || {
                                    Color::from_argb(0xFF00_0000 | (index as u32 * 7 + read.get()))
                                })
                                .size((50.dp(), 10.dp())),
                            );
                            Leaf(
                                Modifier
                                    .background(Color::RED)
                                    .graphics_layer(move |layer| {
                                        layer.alpha = 0.25 + (read.get() % 3) as f32 / 4.0
                                    })
                                    .then(painted(|| Color::BLUE))
                                    .size(10.dp()),
                            );
                            if index % 2 == 0 {
                                Leaf(
                                    Modifier
                                        .shadow(2.dp())
                                        .then(painted(|| Color::GREEN))
                                        .size(8.dp()),
                                );
                            }
                        }),
                    );
                });
            }
        );
    });
    let mut ahead: HashMap<(u64, u64), Vec<u8>> = HashMap::new();
    let mut checked = 0;
    for step in 0..80u32 {
        state.scroll_by(if step % 10 >= 7 { -37.0 } else { 23.0 });
        if step % 4 == 1 {
            tick.set(step);
        }
        let bytes = serialized.frame();
        let cached = without_kept_records(bytes.clone());
        assert_eq!(
            unversioned_bytes(&cached),
            unversioned_bytes(&serialized.fresh()),
            "step {step}"
        );
        let _ = self::read(&bytes);
        for (group, content) in records(&bytes) {
            if let Some(written) = ahead.get(&group) {
                assert_eq!(*written, content, "step {step}: {group:x?} as written ahead");
                checked += 1;
            }
        }
        for written in serialized.prefetch_and_write_ahead() {
            ahead.extend(records(&written));
        }
    }
    assert!(checked > 0, "records written ahead met again");
}

/// Turning the performance hint off (`ComposeRsView.isPerformanceHintEnabled = false`, which
/// sets the optimizations through `setOptimizations`) changes nothing drawn: no boundary should
/// be drawn again, no record written again. The view starts with it on, as by default (D610).
#[test]
fn review_the_performance_hint_toggle_draws_nothing_again() {
    let with_hint = Optimizations::defaults().with(Optimization::PERFORMANCE_HINT, true);
    let mut serialized = Serialized::with(with_hint, || {
        Layout(
            Modifier.repaint_boundary().then(painted(|| Color::WHITE)).size(50.dp()),
            stack(),
            callback(|| {
                Leaf(Modifier.repaint_boundary().then(painted(|| Color::RED)).size(10.dp()));
            }),
        );
    });
    let before = serialized.frame();
    let without_hint = with_hint.with(Optimization::PERFORMANCE_HINT, false);
    assert_ne!(without_hint, with_hint);
    // As `Java_rs_compose_android_ComposeRsNative_setOptimizations`.
    serialized.cache.set_optimizations(without_hint);
    serialized.host.set_optimizations(without_hint);
    let after = serialized.frame();
    assert_eq!(groups(&read(&after).1), groups(&read(&before).1), "versions changed");
    assert_eq!(serialized.cache.written(), 0, "records written again");
}

// ---- Randomized incremental frames against fresh hosts (D522's review) ----------------------

mod randomized_inner_layers {
    use super::*;
    use std::cell::RefCell;

    /// Labels of live paragraphs: text and the color of the style each was laid out or restyled in.
    type Labels = Rc<RefCell<HashMap<u64, String>>>;

    struct Labeled {
        inner: FixedTextMeasurer,
        labels: Labels,
    }

    impl TextMeasurer for Labeled {
        fn layout(
            &mut self,
            text: &str,
            style: &TextStyle,
            options: TextLayoutOptions,
            constraints: Constraints,
            density: &Density,
        ) -> ParagraphHandle {
            let handle = self.inner.layout(text, style, options, constraints, density);
            self.labels.borrow_mut().insert(handle.0, format!("{text:?} {:?}", style.color));
            handle
        }
        fn metrics(&self, paragraph: ParagraphHandle) -> ParagraphMetrics {
            self.inner.metrics(paragraph)
        }
        fn did_exceed_max_lines(&self, paragraph: ParagraphHandle) -> bool {
            self.inner.did_exceed_max_lines(paragraph)
        }
        fn max_intrinsic_width(&self, paragraph: ParagraphHandle) -> Option<f32> {
            self.inner.max_intrinsic_width(paragraph)
        }
        fn relayout(
            &mut self,
            paragraph: ParagraphHandle,
            constraints: Constraints,
        ) -> Option<ParagraphHandle> {
            let new = self.inner.relayout(paragraph, constraints)?;
            let label = self.labels.borrow().get(&paragraph.0).cloned();
            if let Some(label) = label {
                self.labels.borrow_mut().insert(new.0, label);
            }
            Some(new)
        }
        fn restyle(
            &mut self,
            paragraph: ParagraphHandle,
            style: &TextStyle,
        ) -> Option<ParagraphHandle> {
            let text = self.inner.text(paragraph).to_string();
            let new = self.inner.restyle(paragraph, style)?;
            let mut labels = self.labels.borrow_mut();
            labels.remove(&paragraph.0);
            labels.insert(new.0, format!("{text:?} {:?}", style.color));
            Some(new)
        }
        fn hit_test(&self, paragraph: ParagraphHandle, position: Offset) -> TextPosition {
            self.inner.hit_test(paragraph, position)
        }
        fn cursor_rect(&self, paragraph: ParagraphHandle, offset: usize) -> Rect {
            self.inner.cursor_rect(paragraph, offset)
        }
        fn text(&self, paragraph: ParagraphHandle) -> &str {
            self.inner.text(paragraph)
        }
        fn release(&mut self, paragraph: ParagraphHandle) {
            self.labels.borrow_mut().remove(&paragraph.0);
            self.inner.release(paragraph);
        }
    }

    thread_local! {
        /// The optimizations the hosts of a run use: Compose's prefetch, or compose-rs's own
        /// (kept and drawn-ahead groups).
        static RUN_OPTIMIZATIONS: std::cell::Cell<Optimizations> =
            std::cell::Cell::new(Optimizations::defaults());
    }

    fn labeled_host() -> (UiHost, Labels) {
        let labels: Labels = Rc::default();
        let mut host = UiHost::new(
            IntSize::new(200, 200),
            Density::default(),
            Box::new(Labeled { inner: FixedTextMeasurer::new(), labels: labels.clone() }),
        );
        host.set_optimizations(RUN_OPTIMIZATIONS.with(std::cell::Cell::get));
        (host, labels)
    }

    /// The flattened dump with each paragraph handle replaced by its label (or RELEASED).
    fn normalized(list: &FlatDisplayList, labels: &Labels) -> String {
        let labels = labels.borrow();
        list.dump()
            .lines()
            .map(|line| {
                let trimmed = line.trim_start();
                if let Some(rest) = trimmed.strip_prefix("paragraph ") {
                    let (handle, at) = rest.split_once(' ').unwrap();
                    let handle: u64 = handle.parse().unwrap();
                    let label = labels.get(&handle).cloned().unwrap_or_else(|| "RELEASED".into());
                    format!("{}paragraph {label} {at}", &line[..line.len() - trimmed.len()])
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn tint(value: i32) -> Color {
        Color::from_argb(0xFF00_0000 | (value as u32).wrapping_mul(0x0012_3457) & 0x00FF_FFFF)
    }

    const KNOBS: usize = 16;
    type Knobs = [MutableState<i32>; KNOBS];

    fn corners(a: f32, b: f32, c: f32, d: f32) -> Shape {
        Shape::Rounded([
            CornerSize::Dp(a.dp()),
            CornerSize::Dp(b.dp()),
            CornerSize::Dp(c.dp()),
            CornerSize::Dp(d.dp()),
        ])
    }

    fn scene(k: Knobs, state: compose_foundation::LazyListState, path: Path) {
        use compose_foundation::{layout::*, *};
        Column!(modifier = Modifier.repaint_boundary(), {
            let path = path.clone();
            // Nested inner layers (clip, then a layer with a toggled shadow), drawings between,
            // a measure-time read sizing and moving children inside them.
            Layout(
                painted(move || tint(k[0].get()))
                    .clip(corners(4.0, 0.0, 2.0, 1.0))
                    .then(painted(move || tint(k[1].get() + 7)))
                    .graphics_layer(move |layer| {
                        layer.alpha = 0.5 + (k[2].get() % 5) as f32 / 10.0;
                        layer.shadow_elevation = (k[3].get() % 2) as f32 * 2.0;
                    })
                    .then(painted(move || tint(k[4].get() + 3)))
                    .padding((k[5].get() % 3).dp())
                    .size((60.dp(), 50.dp())),
                measure_policy(move |scope, measurables, constraints| {
                    let shift = k[6].get();
                    let mut y = 0;
                    for (index, &measurable) in measurables.iter().enumerate() {
                        let width = 5 + ((shift + index as i32) % 4) * 3;
                        let placeable = scope
                            .measure(measurable, Constraints::fixed(width, 4 + (shift % 2) * 2));
                        scope.place(&placeable, (shift % 3) * index as i32, y);
                        y += placeable.height;
                    }
                    scope.layout(constraints.max_width, constraints.max_height)
                }),
                callback(move || {
                    Leaf(painted(move || tint(k[7].get())));
                    if k[8].get() % 2 == 0 {
                        Leaf(painted(|| Color::GREEN));
                    }
                    Leaf(Modifier.z_index((k[9].get() % 3) as f32).then(painted(|| Color::BLUE)));
                    Leaf(Modifier.z_index(1.0).then(painted(|| Color::RED)));
                    Leaf(
                        Modifier
                            .background(Color::WHITE)
                            .clip(corners(1.0, 2.0, 0.0, 0.0))
                            .then(painted(move || tint(k[10].get()))),
                    );
                    Leaf(
                        Modifier
                            .repaint_boundary()
                            .then(painted(move || tint(k[11].get())))
                            .alpha(0.5)
                            .then(painted(move || tint(k[12].get()))),
                    );
                    let path = path.clone();
                    Leaf(Modifier.background(Color::BLACK).clip_to_bounds().draw_behind(
                        move |scope| {
                            path.reset();
                            path.move_to(0.0, 0.0);
                            for line in 0..(k[12].get() % 4 + 1) {
                                path.line_to(line as f32, 3.0);
                            }
                            scope.draw_path(&path, Paint::stroke(Color::BLACK, 1.0));
                        },
                    ));
                }),
            );
            // A text restyled (paint only) or changed inside an inner layer.
            BasicText!(
                "text {}",
                k[13].get() % 2,
                modifier = Modifier.background(Color::BLACK).clip_to_bounds(),
                style = TextStyle { color: tint(k[14].get()), ..TextStyle::DEFAULT },
            );
            // A lazy list whose clip is an inner layer, items with inner layers.
            LazyColumn!(
                modifier =
                    Modifier.background(Color::GREEN).clip_to_bounds().size((100.dp(), 60.dp())),
                state = state,
                |scope: LazyListScope| {
                    scope.items(80, move |index| {
                        let index = index as i32;
                        Column!(
                            modifier = painted(move || tint(index + k[15].get()))
                                .clip(corners(3.0, 0.0, 0.0, 3.0))
                                .then(painted(move || tint(k[0].get() * index))),
                            {
                                BasicText!(
                                    "item {index}",
                                    style = TextStyle {
                                        color: tint(k[14].get() + index),
                                        ..TextStyle::DEFAULT
                                    },
                                );
                                Leaf(
                                    Modifier
                                        .background(Color::RED)
                                        .graphics_layer(move |layer| {
                                            layer.alpha = (k[2].get() % 10) as f32 / 10.0;
                                        })
                                        .then(painted(move || tint(k[1].get() + index)))
                                        .size((10.dp(), ((k[5].get() + index) % 3 + 4).dp())),
                                );
                                if (index + k[8].get()) % 3 == 0 {
                                    Leaf(Modifier.shadow((k[3].get() % 2 * 2).dp()).size(5.dp()));
                                }
                            }
                        );
                    });
                }
            );
        });
    }

    fn fresh(values: [i32; KNOBS], index: usize, offset: i32, rtl: bool) -> String {
        let owned: Vec<OwnedState<i32>> =
            values.iter().map(|value| mutable_state_of(*value)).collect();
        let knobs: Knobs = std::array::from_fn(|i| owned[i].handle());
        let state = compose_foundation::LazyListState::new(index, offset);
        let given = *state;
        let (mut host, labels) = labeled_host();
        if rtl {
            host.set_layout_direction(LayoutDirection::Rtl);
        }
        let path = Path::new();
        host.set_content(move || scene(knobs, given, path.clone()));
        host.frame();
        let dump = normalized(&host.display_list(), &labels);
        drop(host);
        drop(state);
        drop(owned);
        dump
    }

    thread_local! {
        static KEPT_SEEN: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
        static INNER_SEEN: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
        static AHEAD_SEEN: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    fn run(seed: u64, steps: usize) {
        let mut rng = Rng(seed);
        let mut values = [0i32; KNOBS];
        let owned: Vec<OwnedState<i32>> =
            values.iter().map(|value| mutable_state_of(*value)).collect();
        let knobs: Knobs = std::array::from_fn(|i| owned[i].handle());
        let state = compose_foundation::LazyListState::new(0, 0);
        let given = *state;
        let (mut host, labels) = labeled_host();
        host.set_keep_unplaced_groups(true);
        let path = Path::new();
        host.set_content(move || scene(knobs, given, path.clone()));
        let optimizations = RUN_OPTIMIZATIONS.with(std::cell::Cell::get);
        let mut serialized = Serialized {
            host,
            resources: Resources::default(),
            cache: GroupCache::new(optimizations),
            out: Writer::default(),
        };
        let mut seen: HashMap<(u64, u64), Vec<u8>> = HashMap::new();
        let mut replay = PathReplay::default();
        let mut rtl = false;
        let check_records = |bytes: &[u8], seen: &mut HashMap<(u64, u64), Vec<u8>>, what: &str| {
            for (group, content) in records(bytes) {
                if let Some(before) = seen.get(&group) {
                    assert_eq!(
                        *before, content,
                        "seed {seed} {what}: {group:x?} other bytes, same version"
                    );
                }
                seen.insert(group, content);
            }
        };
        let bytes = serialized.frame();
        replay.take(&mut serialized);
        check_records(&bytes, &mut seen, "first");
        for step in 0..steps {
            // One to three changes a frame.
            for _ in 0..=rng.below(3) {
                match rng.below(10) {
                    0..=5 => {
                        let knob = rng.below(KNOBS as u64) as usize;
                        values[knob] = rng.below(12) as i32;
                        owned[knob].set(values[knob]);
                    }
                    6 | 7 => {
                        state.scroll_by(rng.below(90) as f32 - 40.0);
                    }
                    8 => {
                        rtl = !rtl;
                        serialized.host.set_layout_direction(if rtl {
                            LayoutDirection::Rtl
                        } else {
                            LayoutDirection::Ltr
                        });
                    }
                    _ => {
                        for written in serialized.prefetch_and_write_ahead() {
                            AHEAD_SEEN.with(|c| c.set(c.get() + 1));
                            check_records(&written, &mut seen, &format!("step {step} ahead"));
                        }
                    }
                }
            }
            let mut bytes = serialized.frame();
            replay.take(&mut serialized);
            check_records(&bytes, &mut seen, &format!("step {step}"));
            let mut settles = 0;
            while serialized.host.needs_frame() && settles < 8 {
                settles += 1;
                bytes = serialized.frame();
                replay.take(&mut serialized);
                check_records(&bytes, &mut seen, &format!("step {step} settling"));
            }
            let cached = without_kept_records(bytes.clone());
            assert_eq!(
                unversioned_bytes(&cached),
                unversioned_bytes(&serialized.fresh()),
                "seed {seed} step {step}: cache"
            );
            let _ = read(&bytes);
            KEPT_SEEN.with(|c| c.set(c.get() + kept_entries(&bytes).len()));
            INNER_SEEN.with(|c| {
                c.set(c.get() + records(&bytes).keys().filter(|(k, _)| k & (1 << 63) != 0).count())
            });
            let incremental = normalized(&serialized.host.display_list(), &labels);
            assert!(
                !incremental.contains("RELEASED"),
                "seed {seed} step {step}: a released paragraph drawn"
            );
            let expected = fresh(
                values,
                state.first_visible_item_index(),
                state.first_visible_item_scroll_offset(),
                rtl,
            );
            if incremental != expected {
                let dir = "/private/tmp/claude-501/-Users-jdemeulenaere-Development-compose-rs/7d277bf3-2730-4157-9603-1c5cd0f12d15/scratchpad/rev_fix6";
                std::fs::write(format!("{dir}/incremental.txt"), &incremental).unwrap();
                std::fs::write(format!("{dir}/fresh.txt"), &expected).unwrap();
                let a: Vec<&str> = incremental.lines().collect();
                let b: Vec<&str> = expected.lines().collect();
                let first =
                    a.iter().zip(&b).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
                panic!(
                    "seed {seed} step {step} values {values:?} rtl {rtl}: differs at line {first}\n--- incremental\n{}\n--- fresh\n{}",
                    a[first.saturating_sub(8)..(first + 5).min(a.len())].join("\n"),
                    b[first.saturating_sub(8)..(first + 5).min(b.len())].join("\n"),
                );
            }
        }
    }

    /// Randomized frames (a review's): each incremental frame writes what a fresh host in the
    /// same state writes. `INNER_LAYER_SEEDS` and `INNER_LAYER_STEPS` run more (40 × 120 by
    /// default, to keep `cargo test` fast).
    #[test]
    fn incremental_frames_with_inner_layers_draw_what_fresh_hosts_do() {
        let seeds: u64 =
            std::env::var("INNER_LAYER_SEEDS").ok().and_then(|s| s.parse().ok()).unwrap_or(40);
        let steps: usize =
            std::env::var("INNER_LAYER_STEPS").ok().and_then(|s| s.parse().ok()).unwrap_or(120);
        // Compose's prefetch (items deactivated in the frame that leaves them), then compose-rs's
        // own (kept behind and drawn ahead).
        for optimizations in [Optimizations::defaults(), super::legacy()] {
            RUN_OPTIMIZATIONS.with(|cell| cell.set(optimizations));
            for seed in 1..=seeds {
                run(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15), steps);
            }
        }
        let (kept, inner, ahead) = (
            KEPT_SEEN.with(|c| c.get()),
            INNER_SEEN.with(|c| c.get()),
            AHEAD_SEEN.with(|c| c.get()),
        );
        eprintln!("kept entries {kept}, inner-layer records {inner}, items written ahead {ahead}");
        assert!(kept > 0 && inner > 0 && ahead > 0);
    }
}
