//! The invariant `Replayer.kt` and `TextMeasurer.kt` rely on to reuse a released paragraph's
//! render node (D195): once Rust releases a paragraph, no group drawn afterwards keeps the
//! (key, version) of a recording that drew it. A kept group's node is drawn without reading its
//! bytes, so if one kept a released handle, its recording would draw whatever the reused node
//! now holds.
//!
//! Each frame's bytes are read back as the replayer reads them: every paragraph a group's
//! content draws (its own operations, not its nested groups', which have their own versions)
//! must be live in the measurer, and a (key, version) seen before must draw the same handles.
//! After `prefetch` (which composes and disposes items between frames), the last frame's
//! bytes, which the view may draw again without a new frame, must still be live too.

#![allow(non_snake_case)]

// The call macros name the runtime `compose`.
extern crate compose_runtime as compose;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use compose_android::serialize::{
    GroupCache, LAYER_ENTRY_BYTES, Resources, Writer, tag, write_display_list,
};
use compose_android::text::{AndroidTextMeasurer, TextBackend};
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::{mutable_state_of, remember};
use compose_ui::{Density, IntSize, Offset, ParagraphMetrics, PointerEvent, Rect, UiHost, UnitExt};

// ---- A backend tracking the live paragraphs --------------------------------------------------

/// The paragraphs laid out and not released, as `TextMeasurer.kt` holds them.
#[derive(Clone, Default)]
struct Live(Rc<RefCell<HashSet<u32>>>);

impl TextBackend for Live {
    /// 8 px per UTF-16 unit, wrapped at 160 px, lines 20 px high.
    fn layout(&mut self, handle: u32, text: &str, _request: &[u8]) -> ParagraphMetrics {
        self.0.borrow_mut().insert(handle);
        let units = text.encode_utf16().count().max(1);
        let lines = units.div_ceil(20);
        ParagraphMetrics {
            size: IntSize::new((units.min(20) * 8) as i32, (lines * 20) as i32),
            first_baseline: 16.0,
            last_baseline: (lines - 1) as f32 * 20.0 + 16.0,
            line_count: lines as u32,
        }
    }
    fn hit_test(&self, _handle: u32, _position: Offset) -> usize {
        0
    }
    fn cursor(&self, _handle: u32, offset: usize) -> Rect {
        let x = offset as f32 * 8.0;
        Rect::new(x, 0.0, x, 20.0)
    }
    fn release(&mut self, handle: u32) {
        assert!(self.0.borrow_mut().remove(&handle), "paragraph {handle} released twice");
    }
}

// ---- Reading the bytes back ------------------------------------------------------------------

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}

const PAINT: usize = 4 + 1 + 4 + 1;

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

/// What a recording draws: the group's (key, version) (`None`: the view's own recording), and
/// the paragraphs its own operations draw.
type Recording = (Option<(u64, u64)>, Vec<u32>);

fn recordings(bytes: &[u8]) -> Vec<Recording> {
    let mut at = 0;
    if bytes.first() == Some(&tag::LAYERS) {
        at = 5 + u32_at(bytes, 1) as usize * LAYER_ENTRY_BYTES;
    }
    let mut out = Vec::new();
    read(bytes, at, bytes.len(), None, &mut out);
    out
}

fn read(
    bytes: &[u8],
    mut at: usize,
    end: usize,
    group: Option<(u64, u64)>,
    out: &mut Vec<Recording>,
) {
    let index = out.len();
    out.push((group, Vec::new()));
    while at < end {
        let op = bytes[at];
        if op == tag::GROUP {
            let inner = (u64_at(bytes, at + 1), u64_at(bytes, at + 9));
            let length = u32_at(bytes, at + 17) as usize;
            read(bytes, at + 21, at + 21 + length, Some(inner), out);
            at += 21 + length;
        } else {
            if op == tag::DRAW_PARAGRAPH {
                out[index].1.push(u32_at(bytes, at + 1));
            }
            at += 1 + operands(op);
        }
    }
    assert_eq!(at, end, "a record's length is its content's");
}

// ---- A host checked frame by frame -----------------------------------------------------------

struct Checked {
    host: UiHost,
    live: Live,
    resources: Resources,
    cache: GroupCache,
    out: Writer,
    /// Each (key, version) seen, and the paragraphs its recording draws.
    seen: HashMap<(u64, u64), Vec<u32>>,
    /// Groups whose (key, version) a later frame drew again: the invariant was exercised.
    kept: usize,
    /// Paragraphs released so far.
    released: usize,
}

impl Checked {
    fn new(content: impl Fn() + 'static) -> Checked {
        let live = Live::default();
        let host = UiHost::new(
            IntSize::new(200, 200),
            Density::new(1.0),
            Box::new(AndroidTextMeasurer::new(live.clone())),
        );
        host.set_content(content);
        let mut checked = Checked {
            host,
            live,
            resources: Resources::default(),
            cache: GroupCache::default(),
            out: Writer::default(),
            seen: HashMap::new(),
            kept: 0,
            released: 0,
        };
        checked.frame();
        checked
    }

    fn frame(&mut self) {
        let before = self.live.0.borrow().clone();
        self.host.frame();
        self.count_released(&before);
        let (resources, cache, out) = (&mut self.resources, &mut self.cache, &mut self.out);
        self.host.with_tree(|tree| write_display_list(tree.display_list(), resources, cache, out));
        self.resources.collect();
        self.check("a frame");
    }

    /// Prefetching between frames, then the last frame's bytes drawn again.
    fn prefetch(&mut self) {
        let before = self.live.0.borrow().clone();
        self.host.prefetch(Instant::now() + Duration::from_secs(60));
        self.count_released(&before);
        self.check("prefetch, then the last frame drawn again");
    }

    fn count_released(&mut self, before: &HashSet<u32>) {
        let live = self.live.0.borrow();
        self.released += before.iter().filter(|handle| !live.contains(handle)).count();
    }

    fn check(&mut self, when: &str) {
        let live = self.live.0.borrow();
        for (group, handles) in recordings(&self.out.bytes) {
            for handle in &handles {
                assert!(
                    live.contains(handle),
                    "after {when}: {group:x?} draws released paragraph {handle}"
                );
            }
            let Some(group) = group else { continue };
            match self.seen.get(&group) {
                Some(earlier) => {
                    assert_eq!(*earlier, handles, "{group:x?} drew other paragraphs before");
                    self.kept += 1;
                }
                None => {
                    self.seen.insert(group, handles);
                }
            }
        }
    }

    fn click(&mut self, x: f32, y: f32) {
        self.host.send_pointer(PointerEvent::press(Offset::new(x, y)));
        self.host.send_pointer(PointerEvent::release(Offset::new(x, y)));
        self.frame();
    }

    /// The invariant was exercised: paragraphs were released while groups were kept.
    fn assert_exercised(&self) {
        assert!(self.released > 0, "no paragraph was released");
        assert!(self.kept > 0, "no group was kept");
    }
}

/// Text in a repaint boundary of its own.
fn BoundedText(text: String) {
    Box!(modifier = Modifier.repaint_boundary(), {
        BasicText!(text.clone());
    });
}

// ---- Scenarios -------------------------------------------------------------------------------

/// A text changing in one boundary, beside one that doesn't, inside an outer boundary.
#[test]
fn a_changed_text_gives_its_groups_new_versions() {
    let count = mutable_state_of(0u32);
    let given = count.handle();
    let mut checked = Checked::new(move || {
        let count = given.get();
        Column!(modifier = Modifier.repaint_boundary(), {
            BoundedText("still".to_string());
            BoundedText(format!("count {count}"));
            BasicText!("outer {count}");
        });
    });
    for step in 1..10 {
        count.set(step);
        checked.frame();
        // A frame where nothing changes keeps every group.
        checked.frame();
    }
    checked.assert_exercised();
}

/// Items removed from a column of boundaries: their paragraphs go with them.
#[test]
fn a_removed_item_takes_its_paragraphs_out_of_every_kept_group() {
    let items = mutable_state_of((0..8).collect::<Vec<u32>>());
    let given = items.handle();
    let mut checked = Checked::new(move || {
        let items = given.get();
        Column!(modifier = Modifier.repaint_boundary(), {
            for item in &items {
                compose_runtime::key(*item, || BoundedText(format!("item {item}")));
            }
        });
    });
    for removed in [3, 0, 7, 5] {
        items.set(items.get().into_iter().filter(|item| *item != removed).collect());
        checked.frame();
        checked.frame();
    }
    checked.assert_exercised();
}

/// A lazy list scrolled both ways, prefetching between frames: items composed ahead, disposed
/// behind, reused.
#[test]
fn a_scrolled_lazy_list_with_prefetch_draws_only_live_paragraphs() {
    let state = LazyListState::new(0, 0);
    let given = state.clone();
    let mut checked = Checked::new(move || {
        LazyColumn!(
            modifier = Modifier.size_wh(160.dp(), 200.dp()),
            state = given.clone(),
            |scope: LazyListScope| {
                scope.items(300, |index| {
                    Column!({
                        BasicText!("author {index}");
                        BasicText!("a post long enough to wrap onto a second line, {index}");
                    });
                });
            }
        );
    });
    for step in 0..60 {
        state.scroll_by(if step % 6 == 5 { -90.0 } else { 37.0 });
        checked.frame();
        checked.prefetch();
        checked.frame();
    }
    checked.assert_exercised();
}

/// A text field edited: its text laid out again on every change, beside a kept boundary.
#[test]
fn an_edited_text_field_draws_only_live_paragraphs() {
    let mut checked = Checked::new(|| {
        let text = remember(|| mutable_state_of(String::from("hello")));
        Column!({
            BoundedText("a label".to_string());
            Box!(modifier = Modifier.repaint_boundary(), {
                BasicTextField!(
                    text.get(),
                    move |new: String| text.set(new),
                    modifier = Modifier.size_wh(160.dp(), 40.dp())
                );
            });
        });
    });
    // Focus the field (below the label's 20 px).
    checked.click(10.0, 30.0);
    for word in [" there", ",", " world", "!"] {
        assert!(checked.host.send_text(word), "the field took {word:?}");
        checked.frame();
        checked.frame();
    }
    checked.assert_exercised();
}
