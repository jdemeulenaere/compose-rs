//! The groups' records for the Kotlin replayer (D157): a `GROUP` record carries its key and
//! version, and is the same bytes for the same key and version (copied from a cache); a lifted
//! graphics layer's operations are in no record, its properties in the `LAYERS` section, so a
//! layer-only frame changes nothing but that section.

#![allow(non_snake_case)]

use std::rc::Rc;
use std::sync::Arc;

use compose_android::serialize::{
    GroupCache, LAYER_ENTRY_BYTES, Resources, Writer, tag, write_display_list,
};
use compose_runtime::*;
use compose_ui::*;

// ---- Reading the bytes back ------------------------------------------------------------------

/// What the bytes hold, as `Replayer.kt` reads them.
#[derive(Debug, PartialEq)]
enum Item {
    Operation(u8),
    Group { key: u64, version: u64, content: Vec<Item> },
}

/// A `LAYERS` entry.
#[derive(Debug, PartialEq)]
struct Layer {
    key: u64,
    alpha: f32,
    pivot: (f32, f32),
    scale: (f32, f32),
    rotation: f32,
    translation: (f32, f32),
    clip: Option<(f32, f32, f32, f32)>,
    overlapping: bool,
    bounds: (f32, f32, f32, f32),
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

/// The bytes of a paint: color, style, width, cap.
const PAINT: usize = 4 + 1 + 4 + 1;

/// An operation's operand bytes.
fn operands(op: u8) -> usize {
    match op {
        tag::SAVE | tag::RESTORE => 0,
        tag::TRANSLATE => 8,
        tag::CLIP_RECT => 16,
        tag::CLIP_ROUND_RECT => 32,
        tag::DRAW_RECT | tag::DRAW_LINE => 16 + PAINT,
        tag::DRAW_ROUND_RECT => 32 + PAINT,
        tag::DRAW_PARAGRAPH => 12,
        tag::SAVE_LAYER_ALPHA => 4,
        tag::TRANSFORM => 28,
        tag::DRAW_IMAGE => 4 + 32 + 4,
        tag::DRAW_SHADOW => 1 + 32 + 4 + 12 + 4 + 8,
        tag::DRAW_ARC => 16 + 8 + 1 + PAINT,
        tag::DRAW_PATH => 4 + 16 + PAINT,
        other => panic!("unknown tag {other}"),
    }
}

/// The `LAYERS` section (if any) and the operations after it.
fn read(bytes: &[u8]) -> (Vec<Layer>, Vec<Item>) {
    let mut layers = Vec::new();
    let mut at = 0;
    if bytes.first() == Some(&tag::LAYERS) {
        let count = u32_at(bytes, 1) as usize;
        at = 5;
        for _ in 0..count {
            let f = |offset: usize| f32_at(bytes, at + offset);
            layers.push(Layer {
                key: u64_at(bytes, at),
                alpha: f(8),
                pivot: (f(12), f(16)),
                scale: (f(20), f(24)),
                rotation: f(28),
                translation: (f(32), f(36)),
                clip: (bytes[at + 40] != 0).then(|| (f(41), f(45), f(49), f(53))),
                overlapping: bytes[at + 57] != 0,
                bounds: (f(58), f(62), f(66), f(70)),
            });
            at += LAYER_ENTRY_BYTES;
        }
    }
    (layers, read_items(bytes, at, bytes.len()))
}

fn read_items(bytes: &[u8], mut at: usize, end: usize) -> Vec<Item> {
    let mut items = Vec::new();
    while at < end {
        let op = bytes[at];
        at += 1;
        if op == tag::GROUP {
            let (key, version) = (u64_at(bytes, at), u64_at(bytes, at + 8));
            let length = u32_at(bytes, at + 16) as usize;
            at += 20;
            items.push(Item::Group { key, version, content: read_items(bytes, at, at + length) });
            at += length;
        } else {
            items.push(Item::Operation(op));
            at += operands(op);
        }
    }
    assert_eq!(at, end, "a record's length is its content's");
    items
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
            // Into the group's content.
            tag::GROUP => at += 21,
            other => at += 1 + operands(other),
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

impl Serialized {
    fn new(content: impl Fn() + 'static) -> Serialized {
        let host = UiHost::headless(200, 200);
        host.set_content(content);
        let mut serialized = Serialized {
            host,
            resources: Resources::default(),
            cache: GroupCache::default(),
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
            write_display_list(tree.display_list(), resources, &mut GroupCache::default(), &mut out)
        });
        out.bytes
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
    let mut serialized = Serialized::new(|| {
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
            key: group.key,
            alpha: 0.5,
            pivot: (10.0, 10.0),
            scale: (1.0, 1.0),
            rotation: 30.0,
            translation: (4.0, 0.0),
            clip: Some((0.0, 0.0, 20.0, 20.0)),
            // One drawing: faded, no offscreen layer.
            overlapping: false,
            bounds: (0.0, 0.0, 20.0, 20.0),
        }]
    );
    assert_eq!(groups(&items), [(group.key, group.version)]);
    // In the display list, the layer's operations are there; in the bytes, nowhere.
    assert!(list.ops.iter().any(|op| matches!(op, DrawOp::SaveLayerAlpha(_))), "{}", list.dump());
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
        let mut serialized = Serialized::new(move || {
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
        layers.iter().map(|layer| layer.overlapping).collect::<Vec<_>>()
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
    assert_eq!(serialized.cache.copied(), 1, "the outer record (holding the layer's) is copied");
    assert_eq!(after, serialized.fresh(), "the cached records differ from fresh ones");
}

/// Copied records are the bytes writing them again would give, across frames where one group
/// changes (its parent's version with it) and its sibling doesn't; a gone group leaves the
/// cache.
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
    assert_eq!(second, serialized.fresh());
    let now = groups(&read(&second).1);
    assert_ne!(now[0], (outer, outer_version), "a nested change changes its parent's version");
    assert_ne!(now[1], (changing, changing_version));
    assert_eq!(now[2], (still, still_version));
    assert_eq!(now[3].0, other);
    shown.set(false);
    let third = serialized.frame();
    assert_eq!(third, serialized.fresh());
    assert_eq!(serialized.cache.len(), 3, "the gone group's record is dropped");
}

/// A `SaveLayerAlpha` around a group isn't folded into the group's drawing: the folded alpha
/// would be in the group's record, which its version doesn't cover.
#[test]
fn a_layer_around_a_group_is_not_folded_into_it() {
    let mut list = DisplayList::default();
    list.push(DrawOp::SaveLayerAlpha(0.5));
    list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::BLACK)));
    list.push(DrawOp::Restore);
    list.groups.push(DrawGroup {
        key: 1,
        version: 1,
        start: 1,
        end: 2,
        layer_start: 1,
        layer_end: 2,
        layer: None,
    });
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
    // The rect's color, whole: after the layer (5 bytes) and the header (21) and its bounds.
    assert_eq!(u32_at(&out.bytes, 5 + 21 + 1 + 16), 0xFF00_0000);
}

/// A group whose layer has operations, by hand: its record opens where they start, and the
/// operations after its content (their restores) are skipped too.
#[test]
fn a_record_takes_its_layers_operations_place() {
    let rect = || DrawOp::DrawRect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::BLACK));
    let mut list = DisplayList::default();
    list.push(DrawOp::Translate(Offset::new(2.0, 2.0)));
    list.push(DrawOp::Save);
    list.push(DrawOp::Transform {
        pivot: Offset::new(2.0, 2.0),
        scale: (2.0, 2.0),
        rotation: 0.0,
        translation: Offset::ZERO,
    });
    list.push(DrawOp::SaveLayerAlpha(0.5));
    list.push(rect());
    list.push(DrawOp::Restore);
    list.push(DrawOp::Restore);
    list.push(rect());
    list.groups.push(DrawGroup {
        key: 9,
        version: 3,
        start: 4,
        end: 5,
        layer_start: 1,
        layer_end: 7,
        layer: Some(LayerProperties {
            alpha: 0.5,
            pivot: Offset::new(2.0, 2.0),
            scale: (2.0, 2.0),
            rotation: 0.0,
            translation: Offset::ZERO,
            clip: None,
            bounds: Rect::new(0.0, 0.0, 4.0, 4.0),
        }),
    });
    let (mut resources, mut out) = (Resources::default(), Writer::default());
    write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
    let (layers, items) = read(&out.bytes);
    assert_eq!(layers.len(), 1);
    assert_eq!((layers[0].key, layers[0].scale, layers[0].clip), (9, (2.0, 2.0), None));
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
    let square = Arc::new(Path::from_svg("M0,0 L10,0 L10,10 Z").unwrap());
    let triangle = Arc::new(Path::from_svg("M0,0 L10,10 L0,10 Z").unwrap());
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
    // The new path's definition, then (its owner gone) the old one's drop.
    assert_eq!(pending.bytes[0], tag::DEFINE_PATH);
    let new_id = u32_at(&pending.bytes, 1);
    assert_ne!(new_id, defined);
    let last = pending.bytes.len() - 5;
    assert_eq!((pending.bytes[last], u32_at(&pending.bytes, last + 1)), (tag::DROP_PATH, defined));
    let at = find(&bytes, tag::DRAW_PATH).expect("a path drawn");
    assert_eq!(u32_at(&bytes, at + 1), new_id);
}
