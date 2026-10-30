//! What accessibility's reads cost (the semantics design's SE-2): the merged semantics tree a
//! bridge reads, the record Android's `Accessibility.kt` takes, and finding a node's action, on
//! the feed, the big screen, the stress list, the gallery and the gallery with its drawer open,
//! on a phone-sized host (1080 × 2400 px at 2.625) with a fixed text measurer, nothing changed
//! between reads (a still screen a service explores), then what the first read and a read after a
//! scroll cost. Each screen's `RESULT` lines give the fastest of many short batches, steadier on
//! a loaded machine than criterion's: run with a filter matching no benchmark (`-- none`) for
//! them alone.

#![allow(non_snake_case)]

use std::time::Duration;

use compose::runtime::platform;
use compose::ui::platform::{FixedTextMeasurer, SemanticsNode, UiHost};
use compose::ui::{
    Density, IntSize, Offset, PointerEvent, Role, SemanticsActions, SemanticsProperties,
};
use compose_android::accessibility::write_tree;
use compose_android::serialize::Writer;
use criterion::{Criterion, criterion_group, criterion_main};

/// A phone-sized host showing `content`, laid out.
fn host(content: fn()) -> UiHost {
    platform::set_manual_clock(platform::now_nanos());
    let mut host = UiHost::new(
        IntSize::new(1080, 2400),
        Density::new(2.625),
        Box::new(FixedTextMeasurer::new()),
    );
    host.set_content(content);
    settle(&mut host);
    host
}

/// Frames until the animations end.
fn settle(host: &mut UiHost) {
    for _ in 0..120 {
        platform::advance_clock(Duration::from_millis(16));
        host.frame();
        if !host.needs_frame() {
            break;
        }
    }
}

fn find<'a>(
    node: &'a SemanticsNode,
    test: &dyn Fn(&SemanticsNode) -> bool,
) -> Option<&'a SemanticsNode> {
    if test(node) {
        return Some(node);
    }
    node.children.iter().find_map(|child| find(child, test))
}

/// The gallery with its navigation drawer open (its top bar's menu clicked).
fn gallery_with_drawer() -> UiHost {
    let mut host = host(gallery::Gallery);
    let tree = host.semantics_tree(true);
    let menu = find(&tree, &|node| {
        node.config
            .get(&SemanticsProperties::CONTENT_DESCRIPTION)
            .is_some_and(|descriptions| descriptions.iter().any(|d| d.as_str() == "Menu"))
    })
    .expect("the menu")
    .id;
    assert!(host.perform_semantics_action(menu, &SemanticsActions::ON_CLICK));
    settle(&mut host);
    host
}

/// A screen's name, and how to show it on a new host.
type Screen = (&'static str, fn() -> UiHost);

fn screens() -> Vec<Screen> {
    vec![
        ("feed", || host(compose_rs_feed::Feed)),
        ("big", || host(compose_rs_feed::BigScreen)),
        ("stress", || host(compose_rs_feed::stress::StressScroll)),
        ("gallery", || host(gallery::Gallery)),
        ("gallery_drawer", gallery_with_drawer),
    ]
}

/// The fastest call of `f` over 200 batches of 10, per call.
fn fastest(mut f: impl FnMut()) -> Duration {
    (0..200)
        .map(|_| {
            let start = std::time::Instant::now();
            for _ in 0..10 {
                f();
            }
            start.elapsed() / 10
        })
        .min()
        .unwrap()
}

fn report(name: &str, metric: &str, duration: Duration) {
    eprintln!("RESULT {name} {metric} {:.1}", duration.as_secs_f64() * 1e9);
}

/// The first read of a screen just shown: every configuration computed, on a new host each.
fn first_read(make: fn() -> UiHost) -> Duration {
    (0..20)
        .map(|_| {
            let host = make();
            let start = std::time::Instant::now();
            host.semantics_tree(true);
            start.elapsed()
        })
        .min()
        .unwrap()
}

/// The read after a frame that scrolled what's under the screen's middle, back and forth: what a
/// service pays per changed frame. Only the read is timed.
fn read_after_scroll(host: &mut UiHost) -> Duration {
    host.semantics_tree(true);
    (0..100)
        .map(|round| {
            let delta = if round % 2 == 0 { 120.0 } else { -120.0 };
            host.send_pointer(PointerEvent::scroll(
                Offset::new(540.0, 1200.0),
                Offset::new(0.0, delta),
            ));
            settle(host);
            let start = std::time::Instant::now();
            host.semantics_tree(true);
            start.elapsed()
        })
        .min()
        .unwrap()
}

/// A frame toggling one of 1,000 todos (press, release, the frames until idle), whether a tree
/// is read between them (untimed: a service reading after each frame, or a test) or never (no
/// service on: what keeping the configurations must not make slower). The median, in the frame's
/// own terms, and the fastest.
fn toggle_frames(reads: bool) -> (Duration, Duration) {
    fn show() -> UiHost {
        platform::set_manual_clock(platform::now_nanos());
        let mut host = UiHost::new(
            IntSize::new(400, 100_000),
            Density::new(1.0),
            Box::new(FixedTextMeasurer::new()),
        );
        host.set_content(|| todo::TodoAppWith(1000));
        settle(&mut host);
        host
    }
    fn checkboxes(node: &SemanticsNode, out: &mut Vec<Offset>) {
        if node.config.get(&SemanticsProperties::ROLE) == Some(&Role::Checkbox) {
            let bounds = node.bounds;
            out.push(Offset::new(
                (bounds.left + bounds.right) / 2.0,
                (bounds.top + bounds.bottom) / 2.0,
            ));
        }
        node.children.iter().for_each(|child| checkboxes(child, out));
    }
    // Found on another host: this one, without reads, never builds a tree.
    let mut found = Vec::new();
    checkboxes(&show().semantics_tree(true), &mut found);
    let center = found[500];
    let mut host = show();
    let mut samples: Vec<Duration> = (0..400)
        .map(|_| {
            let start = std::time::Instant::now();
            host.send_pointer(PointerEvent::press(center));
            host.send_pointer(PointerEvent::release(center));
            settle(&mut host);
            let elapsed = start.elapsed();
            if reads {
                host.semantics_tree(true);
            }
            elapsed
        })
        .collect();
    samples.sort();
    (samples[samples.len() / 2], samples[0])
}

fn semantics(c: &mut Criterion) {
    for (metric, reads) in [("toggle_no_reads", false), ("toggle_reads", true)] {
        let (median, fastest) = toggle_frames(reads);
        report("todos", &format!("{metric}_median"), median);
        report("todos", &format!("{metric}_fastest"), fastest);
    }
    for (name, make) in screens() {
        report(name, "first_read", first_read(make));
        let mut host = make();
        let tree = host.semantics_tree(true);
        let count = tree.flatten().len();
        let clickable = find(&tree, &|node| node.config.contains(&SemanticsActions::ON_CLICK))
            .map(|node| node.id);
        eprintln!("{name}: {count} semantics nodes");
        let mut writer = Writer::default();
        report(
            name,
            "tree",
            fastest(|| {
                host.semantics_tree(true);
            }),
        );
        report(
            name,
            "tree_and_record",
            fastest(|| {
                write_tree(&host.semantics_tree(true), &mut writer);
            }),
        );
        if let Some(node) = clickable {
            report(
                name,
                "action",
                fastest(|| {
                    assert!(host.semantics_action(node, &SemanticsActions::ON_CLICK).is_some());
                }),
            );
        }
        report(
            name,
            "frame_then_tree",
            fastest(|| {
                platform::advance_clock(Duration::from_millis(16));
                host.frame();
                host.semantics_tree(true);
            }),
        );
        report(name, "after_scroll", read_after_scroll(&mut host));
        c.bench_function(&format!("semantics/{name}/tree"), |b| {
            b.iter(|| host.semantics_tree(true))
        });
        let mut writer = Writer::default();
        c.bench_function(&format!("semantics/{name}/record"), |b| {
            b.iter(|| {
                let tree = host.semantics_tree(true);
                write_tree(&tree, &mut writer);
                writer.bytes.len()
            })
        });
        if let Some(node) = clickable {
            c.bench_function(&format!("semantics/{name}/action"), |b| {
                b.iter(|| host.semantics_action(node, &SemanticsActions::ON_CLICK).is_some())
            });
        }
        // A frame that changes nothing, then the tree: what a service reading after each frame
        // pays.
        c.bench_function(&format!("semantics/{name}/frame_then_tree"), |b| {
            b.iter(|| {
                platform::advance_clock(Duration::from_millis(16));
                host.frame();
                host.semantics_tree(true)
            })
        });
    }
}

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(30).measurement_time(Duration::from_secs(2));
    targets = semantics
);
criterion_main!(benches);
