//! The semantics records `Accessibility.kt` reads (D478): scrolling containers say how far they
//! are scrolled and can be, and what they clip away isn't visible to the user.

#![allow(non_snake_case)]

// The call macros name the runtime `compose`.
extern crate compose_runtime as compose;

use compose_android::accessibility::{
    find_shown, flag, scroll_action, scroll_actions, scroll_amount, virtual_id, write_tree,
};
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
    scroll: Option<(f32, f32, f32)>,
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
            let max = f32::from_bits(u32_at(&mut at));
            let page = f32::from_bits(u32_at(&mut at));
            at += 1;
            (value, max, page)
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
    assert_eq!(list.scroll, Some((0.0, 900.0, 100.0)));
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
    assert_eq!(column.scroll, Some((0.0, 100.0, 100.0)));
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

/// A page of 100 px each way.
fn page() -> compose_ui::Rect {
    compose_ui::Rect::new(0.0, 0.0, 100.0, 100.0)
}

/// The scrolling node of `content`'s merged semantics tree.
fn scrolling_node(content: fn()) -> compose_ui::platform::SemanticsNode {
    let mut host = UiHost::headless(200, 200);
    host.set_content(content);
    host.settle(10);
    fn find(
        node: &compose_ui::platform::SemanticsNode,
    ) -> Option<compose_ui::platform::SemanticsNode> {
        if node
            .config
            .get(&compose_ui::SemanticsProperties::VERTICAL_SCROLL_AXIS_RANGE)
            .copied()
            .is_some()
            || node
                .config
                .get(&compose_ui::SemanticsProperties::HORIZONTAL_SCROLL_AXIS_RANGE)
                .copied()
                .is_some()
        {
            return Some(node.clone());
        }
        node.children.iter().find_map(find)
    }
    find(&host.semantics_tree(true)).expect("a scrolling node")
}

fn rtl(content: impl Fn() + 'static) {
    compose_runtime::CompositionLocalProvider(
        compose_ui::LOCAL_LAYOUT_DIRECTION.provides(compose_ui::LayoutDirection::Rtl),
        content,
    );
}

#[composable]
fn RightToLeftRow() {
    rtl(|| {
        LazyRow!(modifier = Modifier.size((100.dp(), 20.dp())), |scope: LazyListScope| {
            scope.items(50, |index| {
                BasicText!("tile {}", index, modifier = Modifier.width(20.dp()));
            });
        });
    });
}

#[composable]
fn ReversedRightToLeftRow() {
    rtl(|| {
        LazyRow!(
            modifier = Modifier.size((100.dp(), 20.dp())),
            reverse_layout = true,
            |scope: LazyListScope| {
                scope.items(50, |index| {
                    BasicText!("tile {}", index, modifier = Modifier.width(20.dp()));
                });
            }
        );
    });
}

#[composable]
fn ReversedColumn() {
    LazyColumn!(
        modifier = Modifier.size((100.dp(), 100.dp())),
        reverse_layout = true,
        |scope: LazyListScope| {
            scope.items(50, |index| {
                BasicText!("row {}", index, modifier = Modifier.height(20.dp()));
            });
        }
    );
}

/// A right-to-left row at its start, as Compose's delegate (D491): forward goes towards its
/// end, left as seen, paired with left; right (back) is nothing yet.
#[test]
fn a_right_to_left_row_scrolls_forward_to_the_left() {
    use scroll_action::*;
    let node = scrolling_node(RightToLeftRow);
    assert!(node.rtl);
    assert_eq!(scroll_actions(&node), FORWARD | LEFT);
    assert_eq!(scroll_amount(&node, FORWARD, page()), Some((100.0, 0.0)));
    assert_eq!(scroll_amount(&node, LEFT, page()), Some((100.0, 0.0)));
    assert_eq!(scroll_amount(&node, RIGHT, page()), None);
    assert_eq!(scroll_amount(&node, BACKWARD, page()), None);
    let records = written(RightToLeftRow);
    let row = records.iter().find(|record| record.scroll.is_some()).unwrap();
    assert_ne!(row.flags & flag::RTL, 0);
    assert_ne!(row.flags & flag::SCROLL_HORIZONTAL, 0);
}

/// A reversed right-to-left row (its first item at the left) at its start: forward would go
/// back past it; backward goes on through its items, paired with right, as Compose's.
#[test]
fn a_reversed_right_to_left_row_scrolls_backward_to_the_right() {
    use scroll_action::*;
    let node = scrolling_node(ReversedRightToLeftRow);
    assert_eq!(scroll_actions(&node), BACKWARD | RIGHT);
    assert_eq!(scroll_amount(&node, BACKWARD, page()), Some((100.0, 0.0)));
    assert_eq!(scroll_amount(&node, RIGHT, page()), Some((100.0, 0.0)));
    assert_eq!(scroll_amount(&node, FORWARD, page()), None);
    assert_eq!(scroll_amount(&node, LEFT, page()), None);
}

/// A reversed column at its start (its first item at the bottom): backward and up go on
/// through its items, as Compose's; forward and down have nothing.
#[test]
fn a_reversed_column_scrolls_backward_up() {
    use scroll_action::*;
    let node = scrolling_node(ReversedColumn);
    assert_eq!(scroll_actions(&node), BACKWARD | UP);
    assert_eq!(scroll_amount(&node, UP, page()), Some((0.0, 100.0)));
    assert_eq!(scroll_amount(&node, BACKWARD, page()), Some((0.0, 100.0)));
    assert_eq!(scroll_amount(&node, DOWN, page()), None);
}

/// A plain column at its start: forward and down.
#[test]
fn a_column_scrolls_forward_down() {
    use scroll_action::*;
    let node = scrolling_node(Rows);
    assert_eq!(scroll_actions(&node), FORWARD | DOWN);
    assert_eq!(scroll_amount(&node, DOWN, page()), Some((0.0, 100.0)));
}

#[composable]
fn BothAxes() {
    Column!(
        modifier = Modifier
            .size((100.dp(), 50.dp()))
            .vertical_scroll(remember_scroll_state!())
            .horizontal_scroll(remember_scroll_state!()),
        {
            Spacer(Modifier.size((400.dp(), 400.dp())));
        }
    );
}

/// A node scrolling both ways pages horizontally by its width, vertically by its height, as
/// shown (D496).
#[test]
fn a_node_scrolling_both_ways_pages_by_its_width_and_its_height() {
    use scroll_action::*;
    let mut host = UiHost::headless(200, 200);
    host.set_content(BothAxes);
    host.settle(10);
    let root = host.semantics_tree(true);
    fn find(
        node: &compose_ui::platform::SemanticsNode,
    ) -> Option<&compose_ui::platform::SemanticsNode> {
        if node
            .config
            .get(&compose_ui::SemanticsProperties::HORIZONTAL_SCROLL_AXIS_RANGE)
            .copied()
            .is_some()
            && node
                .config
                .get(&compose_ui::SemanticsProperties::VERTICAL_SCROLL_AXIS_RANGE)
                .copied()
                .is_some()
        {
            return Some(node);
        }
        node.children.iter().find_map(find)
    }
    let node = find(&root).expect("a node scrolling both ways");
    let (found, shown) = find_shown(&root, virtual_id(node.id)).unwrap();
    assert_eq!((shown.width(), shown.height()), (100.0, 50.0));
    assert_eq!(scroll_amount(found, RIGHT, shown), Some((100.0, 0.0)));
    assert_eq!(scroll_amount(found, DOWN, shown), Some((0.0, 50.0)));
}

/// A clickable card around a horizontally scrolling row, as a `Card(on_click)` holding a
/// carousel.
#[composable]
fn CardAroundARow() {
    Box!(modifier = Modifier.clickable(|| {}).size((100.dp(), 40.dp())), {
        Row!(
            modifier =
                Modifier.size((100.dp(), 40.dp())).horizontal_scroll(remember_scroll_state!()),
            {
                for index in 0..10 {
                    BasicText!("tile {}", index, modifier = Modifier.size((40.dp(), 40.dp())));
                }
            }
        );
    });
}

// Compose: compose/ui/ui/src/androidMain/kotlin/androidx/compose/ui/platform/AndroidComposeViewAccessibilityDelegateCompat.android.kt, the scroll ranges read from `unmergedConfig`
/// A merging node's record takes its scroll from its own configuration, not the merged one
/// (D1252): the card around a scrolling row offers no scroll, though Compose's policy merges
/// the row's range into the card's merged configuration.
#[test]
fn a_card_around_a_scrolling_row_offers_no_scroll() {
    use compose_ui::{SemanticsActions, SemanticsProperties};
    let mut host = UiHost::headless(200, 200);
    host.set_content(CardAroundARow);
    host.settle(10);
    let root = host.semantics_tree(true);
    let card = root
        .children
        .iter()
        .find(|node| node.config.contains(&SemanticsActions::ON_CLICK))
        .expect("the card");
    assert!(
        card.config.contains(&SemanticsProperties::HORIZONTAL_SCROLL_AXIS_RANGE),
        "the merged configuration has the row's range: {:?}",
        card.config
    );
    let mut writer = Writer::default();
    write_tree(&root, &mut writer);
    let records = records(&writer.bytes);
    let record = records.iter().find(|record| record.flags & flag::CLICKABLE != 0).unwrap();
    assert_eq!(record.flags & flag::SCROLL, 0, "{record:?}");
    assert_eq!(record.scroll, None);
    assert_eq!(scroll_actions(card), 0);
    let (found, shown) = find_shown(&root, virtual_id(card.id)).unwrap();
    for action in [scroll_action::FORWARD, scroll_action::RIGHT] {
        assert_eq!(scroll_amount(found, action, shown), None);
    }
}
