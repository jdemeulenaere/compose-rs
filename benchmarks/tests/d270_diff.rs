//! D157 over many frames of real scenes, from D270's review: a group's record (its `GROUP`
//! header and content) is the same bytes whenever its key and version are, nested records
//! included, across frames that draw groups again, copy them from the cache, add and remove
//! boundaries, move them and change their layers. (The review also fingerprinted each frame's
//! bytes, dump and Skia pixels to diff against c666311; those hash system-font text and Skia's
//! rasterizer, so they aren't kept as goldens.)

use std::collections::HashMap;
use std::time::Duration;

use compose::foundation::layout::{Box, Column, Row};
use compose::runtime::platform;
use compose::runtime::*;
use compose::ui::platform::{FixedTextMeasurer, UiHost};
use compose::ui::*;
use compose_android::serialize::{
    GroupCache, LAYER_ENTRY_BYTES, Resources, Writer, operand_bytes, tag, write_display_list,
};

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}

/// Every `GROUP` record in `bytes[at..end]`, nested ones included, checked against `seen`.
fn records(bytes: &[u8], mut at: usize, end: usize, seen: &mut HashMap<(u64, u64), Vec<u8>>) {
    while at < end {
        if bytes[at] == tag::GROUP {
            let (key, version) = (u64_at(bytes, at + 1), u64_at(bytes, at + 9));
            let length = u32_at(bytes, at + 17) as usize;
            let record = bytes[at..at + 21 + length].to_vec();
            if let Some(previous) = seen.get(&(key, version)) {
                assert_eq!(previous, &record, "D157: key {key:x} version {version} changed bytes");
            }
            seen.insert((key, version), record);
            records(bytes, at + 21, at + 21 + length, seen);
            at += 21 + length;
        } else {
            at += 1 + operand_bytes(bytes, at);
        }
    }
}

/// Serializes each frame as the Android host does (one cache, one resource table) and checks
/// its records against every earlier frame's.
#[derive(Default)]
struct Probe {
    resources: Resources,
    cache: GroupCache,
    seen: HashMap<(u64, u64), Vec<u8>>,
    copied: usize,
}

impl Probe {
    fn record(&mut self, host: &mut UiHost) {
        let mut out = Writer::default();
        let (resources, cache) = (&mut self.resources, &mut self.cache);
        host.with_tree(|tree| write_display_list(tree.display_list(), resources, cache, &mut out));
        self.resources.collect();
        self.copied += self.cache.copied();
        let bytes = &out.bytes;
        let layers = if bytes.first() == Some(&tag::LAYERS) {
            5 + u32_at(bytes, 1) as usize * LAYER_ENTRY_BYTES
        } else {
            0
        };
        records(bytes, layers, bytes.len(), &mut self.seen);
    }
}

#[test]
fn feed_animated_and_scrolled() {
    platform::set_manual_clock(1_000_000_000);
    let mut host = UiHost::new(
        IntSize::new(540, 1200),
        Density::new(1.5),
        std::boxed::Box::new(FixedTextMeasurer::new()),
    );
    host.set_content(compose_rs_feed::FeedAnimated);
    let mut probe = Probe::default();
    for frame in 0..60 {
        platform::advance_clock(Duration::from_millis(16));
        if frame >= 10 {
            host.send_pointer(PointerEvent::scroll(
                Offset::new(270.0, 800.0),
                Offset::new(0.0, if frame < 40 { 90.0 } else { -60.0 }),
            ));
        }
        host.frame();
        probe.record(&mut host);
    }
    assert!(probe.copied > 0, "kept groups are copied");
}

fn gradient(colors: [Color; 2]) -> Brush {
    Brush::horizontal_gradient(colors)
}

#[composable]
#[allow(non_snake_case)]
fn Scene(step: MutableState<i32>) {
    let s = step.get();
    Column!(modifier = Modifier.background(gradient([Color::GREEN, Color::WHITE])), {
        // A boundary holding a layered child, a nested boundary appearing and disappearing,
        // and children swapping z order.
        Row!(modifier = Modifier.repaint_boundary().background(Color::GRAY), {
            Box!(
                modifier = Modifier
                    .size(12.dp())
                    .graphics_layer(move |scope| {
                        scope.alpha = 0.3 + 0.1 * (step.get() % 5) as f32;
                        scope.rotation_z = 10.0 * step.get() as f32;
                    })
                    .background(gradient([Color::BLUE, Color::GREEN]))
            );
            if s % 3 != 0 {
                Box!(
                    modifier = Modifier
                        .size(10.dp())
                        .repaint_boundary()
                        .background(gradient([Color::RED, Color::BLUE])),
                    {
                        if s % 2 == 0 {
                            Box!(
                                modifier = Modifier
                                    .size(5.dp())
                                    .graphics_layer_with(GraphicsLayerScope {
                                        alpha: 0.5,
                                        clip: true,
                                        ..GraphicsLayerScope::default()
                                    })
                                    .background(gradient([Color::YELLOW, Color::BLACK]))
                            );
                        }
                    }
                );
            }
            Box!(
                modifier = Modifier
                    .size(14.dp())
                    .z_index(if s % 2 == 0 { 1.0 } else { -1.0 })
                    .offset((-6).dp(), 0.dp())
                    .repaint_boundary()
                    .background(Color::MAGENTA)
            );
            Box!(
                modifier =
                    Modifier.size(14.dp()).offset((-12).dp(), 2.dp()).background(Color::CYAN)
            );
        });
        // An alpha layer (not a boundary) around a boundary: never folded.
        Box!(modifier = Modifier.size(8.dp()).alpha(0.5).background(Color::BLACK), {
            Box!(modifier = Modifier.size(4.dp()).repaint_boundary().background(Color::RED));
        });
        // An alpha layer around a boundary drawing nothing on odd steps, and one drawing.
        Box!(modifier = Modifier.size(8.dp()).alpha(0.5), {
            Box!(
                modifier = Modifier.size(4.dp()).repaint_boundary().draw_behind(move |scope| {
                    if step.get() % 2 == 0 {
                        let bounds = scope.bounds();
                        scope.draw_rect(bounds, Paint::fill(Color::RED));
                    }
                })
            );
        });
        // A clip from outside ops around a boundary.
        Box!(modifier = Modifier.size(10.dp()).clip_to_bounds(), {
            Box!(
                modifier = Modifier
                    .size(20.dp())
                    .offset(if s % 2 == 0 { 3.dp() } else { (-3).dp() }, 2.dp())
                    .repaint_boundary()
                    .background(gradient([Color::BLACK, Color::RED]))
            );
        });
        // A boundary that is a boundary only on some steps.
        Box!(
            modifier = if s % 4 < 2 {
                Modifier.size(6.dp()).repaint_boundary().background(Color::BLUE)
            } else {
                Modifier.size(6.dp()).background(Color::BLUE)
            }
        );
        // A graphics-layer boundary with alpha holding just one boundary (overlapping: true).
        Box!(
            modifier = Modifier
                .size(9.dp())
                .graphics_layer(move |scope| scope.alpha = 0.4 + 0.05 * (step.get() % 4) as f32),
            {
                Box!(modifier = Modifier.size(9.dp()).repaint_boundary().background(Color::GREEN));
            }
        );
        // A graphics-layer with alpha holding one drawing (overlapping: false), its content
        // drawn again on some steps.
        Box!(
            modifier = Modifier
                .size(9.dp())
                .graphics_layer(move |scope| scope.alpha = 0.6)
                .draw_behind(move |scope| {
                    let bounds = scope.bounds();
                    let color = if step.get() % 3 == 0 { Color::RED } else { Color::BLUE };
                    scope.draw_rect(bounds, Paint::fill(color));
                    if step.get() % 5 == 0 {
                        scope.draw_rect(bounds, Paint::fill(Color::BLACK));
                    }
                })
        );
        if s % 4 != 3 {
            Popup!({
                Box!(
                    modifier = Modifier
                        .size(7.dp())
                        .repaint_boundary()
                        .background(gradient([Color::MAGENTA, Color::CYAN]))
                );
            });
        }
    });
}

#[test]
fn a_scene_of_edge_cases() {
    let step = mutable_state_of(0);
    let handle = step.handle();
    let mut host = UiHost::new(
        IntSize::new(120, 200),
        Density::new(2.0),
        std::boxed::Box::new(FixedTextMeasurer::new()),
    );
    host.set_content(move || Scene(handle));
    let mut probe = Probe::default();
    for frame in 0..40 {
        host.frame();
        probe.record(&mut host);
        // Some frames change nothing, others change the step.
        if frame % 3 != 2 {
            step.set(step.get() + 1);
        }
    }
}
