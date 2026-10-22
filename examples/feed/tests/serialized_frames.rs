//! What an animation frame of the feed's live header sends to Kotlin (D157): its records
//! written afresh (a new version each, so Kotlin records each again) and the bytes Kotlin
//! reads. A leaf whose drawing changes each frame changes the version of every group around
//! it, so each ancestor's record is written and recorded again. Run with `--nocapture` to see
//! the numbers.

use std::collections::HashMap;

use compose::test::*;
use compose::ui::{Density, IntSize};
use compose_android::serialize::{
    GroupCache, LAYER_ENTRY_BYTES, Resources, Writer, tag, write_display_list,
};

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

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}

/// What Kotlin does with a frame's operations from `at` to `end`, given the versions its nodes
/// hold (`kept`, updated): the bytes it reads, and the groups it records (their depth each).
fn replay(
    bytes: &[u8],
    mut at: usize,
    end: usize,
    depth: usize,
    kept: &mut HashMap<u64, u64>,
    recorded: &mut Vec<usize>,
) -> usize {
    let mut read = 0;
    while at < end {
        let op = bytes[at];
        if op == tag::GROUP {
            let (key, version) = (u64_at(bytes, at + 1), u64_at(bytes, at + 9));
            let length = u32_at(bytes, at + 17) as usize;
            read += 21;
            if kept.get(&key) != Some(&version) {
                recorded.push(depth);
                read += replay(bytes, at + 21, at + 21 + length, depth + 1, kept, recorded);
                kept.insert(key, version);
            }
            at += 21 + length;
        } else {
            read += 1 + operands(op);
            at += 1 + operands(op);
        }
    }
    read
}

#[test]
fn an_animation_frame_of_the_live_header() {
    let mut ui = ComposeTestRule::with_manual_clock(compose_rs_feed::FeedAnimated);
    // A phone's screen.
    ui.host().resize(IntSize::new(1080, 2400));
    ui.host().set_density(Density::new(2.75));
    ui.advance_time_by_frame();
    let (mut resources, mut cache, mut out) =
        (Resources::default(), GroupCache::default(), Writer::default());
    let mut kept = HashMap::new();
    let mut lines = Vec::new();
    for frame in 0..6 {
        ui.advance_time_by_frame();
        ui.host().with_tree(|tree| {
            write_display_list(tree.display_list(), &mut resources, &mut cache, &mut out)
        });
        resources.collect();
        let bytes = &out.bytes;
        let layers = if bytes.first() == Some(&tag::LAYERS) {
            5 + u32_at(bytes, 1) as usize * LAYER_ENTRY_BYTES
        } else {
            0
        };
        let mut recorded = Vec::new();
        let read = replay(bytes, layers, bytes.len(), 0, &mut kept, &mut recorded);
        lines.push(format!(
            "frame {frame}: {} bytes ({layers} of layers), records written {} copied {}, \
             Kotlin reads {read} bytes and records {} groups at depths {recorded:?}",
            bytes.len(),
            cache.written(),
            cache.copied(),
            recorded.len(),
        ));
        if frame > 0 {
            // The header's groups only: the posts are copied and kept.
            assert!(cache.copied() > 0, "{}", lines.join("\n"));
            assert_eq!(cache.written(), recorded.len(), "{}", lines.join("\n"));
        }
    }
    eprintln!("{}", lines.join("\n"));
}
