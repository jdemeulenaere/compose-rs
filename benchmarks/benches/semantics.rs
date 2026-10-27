//! What accessibility's reads cost (the semantics design's SE-2): the merged semantics tree a
//! bridge reads, the record Android's `Accessibility.kt` takes, and finding a node's action, on
//! the feed, the big screen, the stress list, the gallery and the gallery with its drawer open,
//! on a phone-sized host (1080 × 2400 px at 2.625) with a fixed text measurer, nothing changed
//! between reads (a still screen a service explores).

#![allow(non_snake_case)]

use std::time::Duration;

use compose::runtime::platform;
use compose::ui::platform::{FixedTextMeasurer, SemanticsNode, UiHost};
use compose::ui::{Density, IntSize, SemanticsActions, SemanticsProperties};
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

fn screens() -> Vec<(&'static str, UiHost)> {
    vec![
        ("feed", host(compose_rs_feed::Feed)),
        ("big", host(compose_rs_feed::BigScreen)),
        ("stress", host(compose_rs_feed::stress::StressScroll)),
        ("gallery", host(gallery::Gallery)),
        ("gallery_drawer", gallery_with_drawer()),
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

fn semantics(c: &mut Criterion) {
    for (name, mut host) in screens() {
        let tree = host.semantics_tree(true);
        let count = tree.flatten().len();
        let clickable = find(&tree, &|node| node.config.contains(&SemanticsActions::ON_CLICK))
            .map(|node| node.id);
        eprintln!("{name}: {count} semantics nodes");
        // The fastest of many short batches, besides criterion's: steadier on a loaded machine.
        let mut writer = Writer::default();
        let tree_min = fastest(|| {
            host.semantics_tree(true);
        });
        let record_min = fastest(|| {
            write_tree(&host.semantics_tree(true), &mut writer);
        });
        eprintln!(
            "{name}: fastest tree {:.1} µs, tree and record {:.1} µs",
            tree_min.as_secs_f64() * 1e6,
            record_min.as_secs_f64() * 1e6
        );
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
