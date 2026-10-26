//! Adversarial review of D157 (groups kept by key and version, layers in a `LAYERS` section).
//!
//! Each scenario runs frames through the serializer with a cache, as `bridge.rs` does, and checks
//! what `Replayer.kt` relies on:
//! - a (key, version) pair always comes with the same record bytes, across every frame seen
//!   (Kotlin keeps a node by them; Rust copies a record by them);
//! - the cached bytes equal a fresh write;
//! - every `LAYERS` key has a `GROUP` record in the frame (else Kotlin creates a node for it that
//!   no frame draws);
//! - the incremental display list draws what a fresh tree in the same state draws.

#![allow(non_snake_case)]

use std::collections::HashMap;
use std::rc::Rc;

use compose_android::serialize::{
    GroupCache, KEPT_ENTRY_BYTES, LAYER_ENTRY_BYTES, Resources, Writer, operand_bytes, tag,
    write_display_list,
};
use compose_foundation::ScrollableState;
use compose_runtime::*;
use compose_ui::platform::*;
use compose_ui::*;

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}

/// A record: its key, its version, its bytes from its tag to its end.
type Record = (u64, u64, Vec<u8>);

/// The `LAYERS` keys, and each record.
fn parse(bytes: &[u8]) -> (Vec<u64>, Vec<Record>) {
    let mut keys = Vec::new();
    let mut at = 0;
    if bytes.first() == Some(&tag::LAYERS) {
        let count = u32_at(bytes, 1) as usize;
        at = 5;
        for _ in 0..count {
            keys.push(u64_at(bytes, at));
            at += LAYER_ENTRY_BYTES;
        }
    }
    let mut records = Vec::new();
    records_in(bytes, at, bytes.len(), &mut records);
    (keys, records)
}

fn records_in(bytes: &[u8], mut at: usize, end: usize, out: &mut Vec<Record>) {
    while at < end {
        let op = bytes[at];
        if op == tag::GROUP {
            let (key, version) = (u64_at(bytes, at + 1), u64_at(bytes, at + 9));
            let length = u32_at(bytes, at + 17) as usize;
            out.push((key, version, bytes[at..at + 21 + length].to_vec()));
            records_in(bytes, at + 21, at + 21 + length, out);
            at += 21 + length;
        } else {
            at += 1 + operand_bytes(bytes, at);
        }
    }
    assert_eq!(at, end, "a record's length is its content's");
}

/// `bytes` without the records of the groups its `KEPT` section names (D380): a frame writes
/// them only if their version changed; with nothing cached, always.
fn without_kept_records(mut bytes: Vec<u8>) -> Vec<u8> {
    let start = if bytes.first() == Some(&tag::LAYERS) {
        5 + u32_at(&bytes, 1) as usize * LAYER_ENTRY_BYTES
    } else {
        0
    };
    let mut at = start;
    while bytes.get(at) == Some(&tag::GROUP) {
        at += 21 + u32_at(&bytes, at + 17) as usize;
    }
    if bytes.get(at) != Some(&tag::KEPT) {
        return bytes;
    }
    let kept: Vec<u64> = (0..u32_at(&bytes, at + 1) as usize)
        .map(|index| u64_at(&bytes, at + 5 + KEPT_ENTRY_BYTES * index))
        .collect();
    let mut at = start;
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

/// `bytes` with every `GROUP` record's version zeroed.
fn without_versions(bytes: &[u8]) -> Vec<u8> {
    let mut bytes = bytes.to_vec();
    let start = if bytes.first() == Some(&tag::LAYERS) {
        5 + u32_at(&bytes, 1) as usize * LAYER_ENTRY_BYTES
    } else {
        0
    };
    fn zero(bytes: &mut [u8], mut at: usize, end: usize) {
        while at < end {
            if bytes[at] == tag::GROUP {
                let length = u32_at(bytes, at + 17) as usize;
                bytes[at + 9..at + 17].fill(0);
                zero(bytes, at + 21, at + 21 + length);
                at += 21 + length;
            } else if bytes[at] == tag::KEPT {
                return;
            } else {
                at += 1 + operand_bytes(bytes, at);
            }
        }
    }
    let end = bytes.len();
    zero(&mut bytes, start, end);
    bytes
}

/// A host's frames, serialized as `bridge.rs` does, checked against every earlier frame.
struct Checked {
    host: UiHost,
    resources: Resources,
    cache: GroupCache,
    out: Writer,
    /// Every (key, version) seen, and its record's bytes.
    seen: HashMap<(u64, u64), Vec<u8>>,
}

impl Checked {
    fn new(content: impl Fn() + 'static) -> Checked {
        Checked::with(Optimizations::defaults(), content)
    }

    /// With compose-rs's `optimizations`, as the bridge sets them.
    fn with(optimizations: Optimizations, content: impl Fn() + 'static) -> Checked {
        let mut host = UiHost::headless(200, 200);
        host.set_optimizations(optimizations);
        // As the bridge does (D380).
        host.set_keep_unplaced_groups(true);
        host.set_content(content);
        let mut checked = Checked {
            host,
            resources: Resources::default(),
            cache: GroupCache::default(),
            out: Writer::default(),
            seen: HashMap::new(),
        };
        checked.frame();
        checked
    }

    fn frame(&mut self) -> Vec<u8> {
        self.host.frame();
        let (resources, cache, out) = (&mut self.resources, &mut self.cache, &mut self.out);
        self.host.with_tree(|tree| write_display_list(tree.display_list(), resources, cache, out));
        self.resources.collect();
        let bytes = self.out.bytes.clone();
        // Cached == fresh.
        let mut fresh = Writer::default();
        let resources = &mut self.resources;
        self.host.with_tree(|tree| {
            write_display_list(
                tree.display_list(),
                resources,
                &mut GroupCache::default(),
                &mut fresh,
            )
        });
        // Versions aside: a record whose bytes stayed the same keeps the version it was cached
        // with (its nested group changed, D323), where a fresh cache writes the group's.
        assert_eq!(
            without_versions(&bytes),
            without_versions(&without_kept_records(fresh.bytes)),
            "cached records differ from fresh ones"
        );
        let (keys, records) = parse(&bytes);
        for key in keys {
            assert!(
                records.iter().any(|(k, _, _)| *k == key),
                "LAYERS lists {key:#x} but no GROUP record has it"
            );
        }
        for (key, version, record) in records {
            if let Some(earlier) = self.seen.insert((key, version), record.clone()) {
                assert_eq!(
                    earlier, record,
                    "group {key:#x} version {version}: other bytes than an earlier frame's"
                );
            }
        }
        bytes
    }
}

/// The incremental host's operations equal a fresh host's in the same state.
fn same_as_fresh(checked: &Checked, content: impl Fn() + 'static) {
    let mut fresh = UiHost::headless(200, 200);
    fresh.set_content(content);
    fresh.frame();
    let (now, expected) = (checked.host.display_list(), fresh.display_list());
    assert_eq!(now.dump(), expected.dump(), "the incremental frame draws something else");
    assert_eq!(now.groups.len(), expected.groups.len(), "groups: {:?}", now.groups);
}

fn stack() -> Rc<dyn MeasurePolicy> {
    Rc::new(StackMeasurePolicy)
}

fn Leaf(modifier: Modifier) {
    Layout(modifier, empty_measure_policy(), callback(|| {}));
}

fn painted(color: Color) -> Modifier {
    Modifier.draw_behind(move |scope| {
        let bounds = scope.bounds();
        scope.draw_rect(bounds, Paint::fill(color));
    })
}

// ---- A child becoming or ceasing to be a repaint boundary, inside one ----------------------

fn toggled_boundary(boundary: bool) {
    Layout(
        Modifier.repaint_boundary().then(painted(Color::BLUE)),
        stack(),
        callback(move || {
            let modifier = if boundary { Modifier.repaint_boundary() } else { Modifier };
            Leaf(modifier.then(painted(Color::RED)).size(10.dp()));
        }),
    );
}

#[test]
fn a_child_becoming_a_boundary_inside_one() {
    let on = mutable_state_of(false);
    let handle = on.handle();
    let mut checked = Checked::new(move || toggled_boundary(handle.get()));
    on.set(true);
    checked.frame();
    same_as_fresh(&checked, || toggled_boundary(true));
    on.set(false);
    checked.frame();
    same_as_fresh(&checked, || toggled_boundary(false));
}

fn toggled_layer(layer: bool, alpha: f32) {
    Layout(
        Modifier.repaint_boundary().then(painted(Color::BLUE)),
        stack(),
        callback(move || {
            let modifier = if layer {
                Modifier.graphics_layer_with(GraphicsLayerScope {
                    alpha,
                    ..GraphicsLayerScope::default()
                })
            } else {
                Modifier
            };
            Leaf(modifier.then(painted(Color::RED)).size(10.dp()));
        }),
    );
}

#[test]
fn a_child_gaining_or_losing_a_lifted_layer_inside_a_boundary() {
    let on = mutable_state_of(false);
    let handle = on.handle();
    let mut checked = Checked::new(move || toggled_layer(handle.get(), 0.5));
    on.set(true);
    checked.frame();
    same_as_fresh(&checked, || toggled_layer(true, 0.5));
    on.set(false);
    checked.frame();
    same_as_fresh(&checked, || toggled_layer(false, 0.5));
}

// ---- A boundary unplaced then placed again, inside a boundary ------------------------------

struct PlaceIf(Rc<dyn Fn() -> bool>);

impl MeasurePolicy for PlaceIf {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let loose = constraints.loosen();
        let place = (self.0)();
        for &measurable in measurables {
            let placeable = scope.measure(measurable, loose);
            if place {
                scope.place_relative(&placeable, 0, 0);
            }
        }
        scope.layout(50, 50)
    }
}

#[test]
fn a_boundary_unplaced_inside_a_boundary() {
    let placed = mutable_state_of(true);
    let handle = placed.handle();
    let content = move |place: Rc<dyn Fn() -> bool>| {
        Layout(
            Modifier.repaint_boundary(),
            stack(),
            callback(move || {
                let place = place.clone();
                Layout(
                    Modifier,
                    Rc::new(PlaceIf(place)),
                    callback(|| {
                        Leaf(Modifier.repaint_boundary().then(painted(Color::RED)).size(10.dp()))
                    }),
                );
            }),
        );
    };
    let live = content;
    let mut checked = Checked::new(move || live(Rc::new(move || handle.get())));
    placed.set(false);
    checked.frame();
    same_as_fresh(&checked, move || content(Rc::new(|| false)));
}

// ---- Layer properties crossing their thresholds in a nested, kept group --------------------

#[test]
fn a_nested_layer_crossing_its_thresholds() {
    let alpha = mutable_state_of(1.0f32);
    let rotation = mutable_state_of(0.0f32);
    let clip = mutable_state_of(false);
    let (a, r, c) = (alpha.handle(), rotation.handle(), clip.handle());
    let mut checked = Checked::new(move || {
        Layout(
            Modifier.repaint_boundary().then(painted(Color::BLUE)),
            stack(),
            callback(move || {
                Leaf(
                    Modifier
                        .graphics_layer(move |layer| {
                            layer.alpha = a.get();
                            layer.rotation_z = r.get();
                            layer.clip = c.get();
                        })
                        .then(painted(Color::RED))
                        .size(10.dp()),
                );
            }),
        );
    });
    for (next_alpha, next_rotation, next_clip) in [
        (0.5, 0.0, false),
        (0.5, 10.0, false),
        (0.0, 10.0, true),
        (1.0, 0.0, false),
        (0.3, 0.0, true),
    ] {
        alpha.set(next_alpha);
        rotation.set(next_rotation);
        clip.set(next_clip);
        checked.frame();
        assert_eq!(checked.cache.copied(), 2, "a layer-only frame copies both records");
    }
}

// ---- A content layer (after a drawing) around a nested lifted layer ------------------------

#[test]
fn a_content_layer_around_a_nested_lifted_layer() {
    let alpha = mutable_state_of(0.5f32);
    let handle = alpha.handle();
    let mut checked = Checked::new(move || {
        Layout(
            Modifier
                .repaint_boundary()
                .then(painted(Color::BLUE))
                .graphics_layer_with(GraphicsLayerScope { alpha: 0.5, ..Default::default() }),
            stack(),
            callback(move || {
                Leaf(
                    Modifier
                        .graphics_layer(move |layer| layer.alpha = handle.get())
                        .then(painted(Color::RED))
                        .size(10.dp()),
                );
            }),
        );
    });
    for next in [1.0, 0.25, 0.0, 0.75] {
        alpha.set(next);
        checked.frame();
    }
}

// ---- A lazy list scrolled, its items laid out again and reused ------------------------------

#[test]
fn a_scrolled_lazy_list_keeps_its_records_sound() {
    use compose_foundation::LazyListState;
    let state = LazyListState::new(0, 0);
    let given = *state;
    let tick = mutable_state_of(0u32);
    let tick_handle = tick.handle();
    let mut checked = Checked::new(move || {
        compose_foundation::LazyColumn(
            Modifier.size((100.dp(), 100.dp())),
            given,
            compose_foundation::layout::PaddingValues::ZERO,
            false,
            compose_foundation::layout::Arrangement::TOP,
            compose_foundation::Alignment::START,
            true,
            callback(move |scope: compose_foundation::LazyListScope| {
                scope.items(100, move |index| {
                    let color = if index % 3 == 0 { Color::RED } else { Color::BLUE };
                    Leaf(
                        Modifier
                            .graphics_layer(move |layer| {
                                layer.alpha =
                                    if (tick_handle.get() + index as u32).is_multiple_of(2) {
                                        0.5
                                    } else {
                                        1.0
                                    };
                            })
                            .then(painted(color))
                            .size((100.dp(), 20.dp())),
                    );
                });
            }),
        );
    });
    for step in 0..40 {
        state.dispatch_raw_delta(if step % 7 == 6 { -45.0 } else { 13.0 });
        tick.set(step);
        let (layers, records) = parse(&checked.frame());
        assert!(layers.len() >= 5 && records.len() >= 5, "{} {}", layers.len(), records.len());
    }
}

// ---- The layout direction changing under a boundary ------------------------------------------

fn directional() {
    Layout(
        Modifier,
        stack(),
        callback(|| {
            Leaf(
                Modifier
                    .repaint_boundary()
                    .draw_behind(|scope| {
                        let size = scope.bounds();
                        let left = scope.layout_direction() == LayoutDirection::Ltr;
                        let x = if left { 0.0 } else { size.right - 2.0 };
                        scope.draw_rect(
                            Rect::new(x, 0.0, x + 2.0, size.bottom),
                            Paint::fill(Color::RED),
                        );
                    })
                    .size(10.dp()),
            );
        }),
    );
}

#[test]
fn a_boundary_drawing_by_layout_direction_follows_it() {
    let mut checked = Checked::new(directional);
    checked.host.set_layout_direction(LayoutDirection::Rtl);
    checked.frame();
    let mut fresh = UiHost::headless(200, 200);
    fresh.set_layout_direction(LayoutDirection::Rtl);
    fresh.set_content(directional);
    fresh.frame();
    assert_eq!(checked.host.display_list().dump(), fresh.display_list().dump());
}
