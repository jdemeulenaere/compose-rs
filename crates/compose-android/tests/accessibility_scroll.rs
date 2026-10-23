//! The semantics records `Accessibility.kt` reads (D478): scrolling containers say how far they
//! are scrolled and can be, and what they clip away isn't visible to the user.

#![allow(non_snake_case)]

// The call macros name the runtime `compose`.
extern crate compose_runtime as compose;

use compose_android::accessibility::{flag, write_tree};
use compose_android::serialize::Writer;
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::composable;
use compose_ui::platform::UiHost;
use compose_ui::*;

/// A record, as `Accessibility.kt` reads it.
#[derive(Debug)]
struct Record {
    bounds: [f32; 4],
    flags: u32,
    text: String,
    scroll: Option<(f32, f32)>,
}

fn records(bytes: &[u8]) -> Vec<Record> {
    let mut at = 0;
    let u32_at = |at: &mut usize| {
        let value = u32::from_le_bytes(bytes[*at..*at + 4].try_into().unwrap());
        *at += 4;
        value
    };
    let mut out = Vec::new();
    while at < bytes.len() {
        u32_at(&mut at);
        u32_at(&mut at);
        let bounds = [0; 4].map(|_| f32::from_bits(u32_at(&mut at)));
        let flags = u32_at(&mut at);
        at += 1;
        let mut strings = Vec::new();
        for _ in 0..5 {
            let length = u32_at(&mut at) as usize;
            strings.push(String::from_utf8(bytes[at..at + length].to_vec()).unwrap());
            at += length;
        }
        if flags & flag::RANGE != 0 {
            at += 12;
        }
        let scroll = (flags & flag::SCROLL != 0).then(|| {
            let value = f32::from_bits(u32_at(&mut at));
            (value, f32::from_bits(u32_at(&mut at)))
        });
        out.push(Record { bounds, flags, text: strings.swap_remove(0), scroll });
    }
    out
}

fn written(content: fn()) -> Vec<Record> {
    let mut host = UiHost::headless(200, 200);
    host.set_content(content);
    host.settle(10);
    let mut writer = Writer::default();
    write_tree(&host.semantics_tree(true), &mut writer);
    records(&writer.bytes)
}

#[composable]
fn Rows() {
    LazyColumn!(modifier = Modifier.size((100.dp(), 100.dp())), |scope: LazyListScope| {
        scope.items(50, |index| {
            BasicText!("row {}", index, modifier = Modifier.height(20.dp()));
        });
    });
}

#[composable]
fn ScrollingRows() {
    Column!(
        modifier = Modifier.size((100.dp(), 100.dp())).vertical_scroll(remember_scroll_state!()),
        {
            for index in 0..10 {
                BasicText!("row {}", index, modifier = Modifier.height(20.dp()));
            }
        }
    );
}

#[test]
fn a_lazy_list_is_a_scrolling_container_with_its_items_in_it() {
    let records = written(Rows);
    let list = records.iter().find(|record| record.scroll.is_some()).expect("a scrolling node");
    assert_eq!(list.flags & flag::SCROLL_HORIZONTAL, 0);
    assert_eq!(list.scroll, Some((0.0, 900.0)));
    assert_eq!(list.bounds, [0.0, 0.0, 100.0, 100.0]);
    // Only the rows in view are placed, all visible.
    let rows: Vec<&Record> =
        records.iter().filter(|record| record.text.starts_with("row")).collect();
    assert_eq!(rows.len(), 5);
    assert!(rows.iter().all(|row| row.flags & flag::HIDDEN == 0));
}

#[test]
fn what_a_scrolling_column_clips_away_is_hidden() {
    let records = written(ScrollingRows);
    let column = records.iter().find(|record| record.scroll.is_some()).expect("a scrolling node");
    assert_eq!(column.scroll, Some((0.0, 100.0)));
    let hidden: Vec<&str> = records
        .iter()
        .filter(|record| record.flags & flag::HIDDEN != 0)
        .map(|record| record.text.as_str())
        .collect();
    assert_eq!(hidden, ["row 5", "row 6", "row 7", "row 8", "row 9"]);
    // The last row in view is visible.
    let row = records.iter().find(|record| record.text == "row 4").unwrap();
    assert_eq!(row.flags & flag::HIDDEN, 0);
    assert_eq!((row.bounds[1], row.bounds[3]), (80.0, 100.0));
}
