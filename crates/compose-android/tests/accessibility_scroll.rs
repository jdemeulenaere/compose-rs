//! The semantics records `Accessibility.kt` reads (D478): scrolling containers say how far they
//! are scrolled and can be, and what they clip away isn't visible to the user.

#![allow(non_snake_case)]

// The call macros name the runtime `compose`.
extern crate compose_runtime as compose;

use compose_android::accessibility::{
    find_shown, flag, scroll_action, scroll_actions, scroll_amount, show_on_screen, virtual_id,
    write_tree,
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
        for _ in 0..6 {
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
        // Who it's read before, its drawing order, its class, role, state and percent, selection,
        // granularities, its actions.
        at += 8;
        let length = u32_at(&mut at) as usize;
        at += length + 12;
        let tag = u32_at(&mut at) as usize;
        at += tag;
        // No extras in these scenes.
        at += 1;
        let actions = bytes[at] as usize;
        at += 1;
        for _ in 0..actions {
            at += 1;
            let length = u32_at(&mut at) as usize;
            at += length;
        }
        out.push(Record { bounds, flags, text: strings.swap_remove(0), scroll });
    }
    out
}

fn written(content: fn()) -> Vec<Record> {
    let mut host = UiHost::headless(200, 200);
    host.set_content(content);
    host.settle(10);
    let mut writer = Writer::default();
    write_tree(
        &host.semantics_tree(true),
        &mut compose_android::accessibility::KeptCustomActions::default(),
        &mut writer,
    );
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
fn scrolling_node(content: fn()) -> compose_ui::platform::SemanticsSnapshot {
    let mut host = UiHost::headless(200, 200);
    host.set_content(content);
    host.settle(10);
    fn find(
        node: &compose_ui::platform::SemanticsSnapshot,
    ) -> Option<compose_ui::platform::SemanticsSnapshot> {
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
        node: &compose_ui::platform::SemanticsSnapshot,
    ) -> Option<&compose_ui::platform::SemanticsSnapshot> {
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
    let (found, shown) = find_shown(&root, virtual_id(node)).unwrap();
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
    write_tree(
        &root,
        &mut compose_android::accessibility::KeptCustomActions::default(),
        &mut writer,
    );
    let records = records(&writer.bytes);
    let record = records.iter().find(|record| record.flags & flag::CLICKABLE != 0).unwrap();
    assert_eq!(record.flags & flag::SCROLL, 0, "{record:?}");
    assert_eq!(record.scroll, None);
    assert_eq!(scroll_actions(card), 0);
    let (found, shown) = find_shown(&root, virtual_id(card)).unwrap();
    for action in [scroll_action::FORWARD, scroll_action::RIGHT] {
        assert_eq!(scroll_amount(found, action, shown), None);
    }
}

// ---- Compose's grids' `LazyScrollAccessibilityTest` --------------------------------------------

/// Compose's `TestConfig`.
#[derive(Clone, Copy, Debug)]
struct GridConfig {
    horizontal: bool,
    rtl: bool,
    reversed: bool,
}

fn grid_configs() -> Vec<GridConfig> {
    let mut configs = Vec::new();
    for horizontal in [false, true] {
        for rtl in [false, true] {
            for reversed in [false, true] {
                configs.push(GridConfig { horizontal, rtl, reversed });
            }
        }
    }
    configs
}

/// A 200 px grid, padded by 50, of 100 items of 21 px with their indices: the viewport's 100 px, the
/// content's 2,100 px, from the position given.
fn accessible_grid(config: GridConfig, first: usize, offset: i32) -> UiHost {
    let mut host = UiHost::headless(200, 200);
    let owned = LazyGridState::new(first, offset);
    let state = *owned;
    host.set_content(move || {
        let _ = &owned;
        let direction = if config.rtl {
            compose_ui::LayoutDirection::Rtl
        } else {
            compose_ui::LayoutDirection::Ltr
        };
        compose_runtime::CompositionLocalProvider(
            compose_ui::LOCAL_LAYOUT_DIRECTION.provides(direction),
            move || {
                let content = |scope: LazyGridScope| {
                    scope.items(100, |index| {
                        Box!(modifier = Modifier.required_size(21.dp()), {
                            BasicText!("{}", index);
                        });
                    });
                };
                let modifier = Modifier.size(200.dp());
                if config.horizontal {
                    LazyHorizontalGrid!(
                        GridCells::Fixed(1),
                        modifier = modifier,
                        state = state,
                        content_padding = PaddingValues::all(50.dp()),
                        reverse_layout = config.reversed,
                        content = content
                    );
                } else {
                    LazyVerticalGrid!(
                        GridCells::Fixed(1),
                        modifier = modifier,
                        state = state,
                        content_padding = PaddingValues::all(50.dp()),
                        reverse_layout = config.reversed,
                        content = content
                    );
                }
            },
        );
    });
    host.settle(10);
    host
}

fn the_scrolling_node(host: &UiHost) -> compose_ui::platform::SemanticsSnapshot {
    fn find(
        node: &compose_ui::platform::SemanticsSnapshot,
    ) -> Option<compose_ui::platform::SemanticsSnapshot> {
        let config = &node.config;
        if config.get(&compose_ui::SemanticsProperties::VERTICAL_SCROLL_AXIS_RANGE).is_some()
            || config.get(&compose_ui::SemanticsProperties::HORIZONTAL_SCROLL_AXIS_RANGE).is_some()
        {
            return Some(node.clone());
        }
        node.children.iter().find_map(find)
    }
    find(&host.semantics_tree(true)).expect("a scrolling node")
}

fn shows_text(host: &UiHost, text: &str) -> bool {
    fn find(node: &compose_ui::platform::SemanticsSnapshot, text: &str) -> bool {
        node.config
            .get(&compose_ui::SemanticsProperties::TEXT)
            .is_some_and(|texts| texts.iter().any(|it| it.as_str() == text))
            || node.children.iter().any(|child| find(child, text))
    }
    find(&host.semantics_tree(true), text)
}

/// Compose's `testScrollAction`, from the middle: 47 items and 13 px in, 1,000 px either side.
fn grid_action_reaches(config: GridConfig, action: u8, target: usize, succeeds: bool) {
    let mut host = accessible_grid(config, 47, 13);
    assert!(!shows_text(&host, &target.to_string()), "{config:?}: {target} before");
    let node = the_scrolling_node(&host);
    let amount = scroll_amount(&node, action, node.bounds);
    assert_eq!(amount.is_some(), succeeds, "{config:?}");
    if let Some((x, y)) = amount {
        let scrolled = host.perform_semantics_action_with(
            node.id,
            &compose_ui::SemanticsActions::SCROLL_BY,
            |scroll| scroll(x, y),
        );
        assert!(scrolled, "{config:?}");
        host.settle(10);
    }
    assert_eq!(shows_text(&host, &target.to_string()), succeeds, "{config:?}: {target} after");
}

/// `testRelativeDirection`: forward and backward, whatever the direction, the reversal mirrors.
fn grid_relative(canonical: usize, action: u8) {
    for config in grid_configs() {
        let target = if config.reversed { 100 - canonical - 1 } else { canonical };
        grid_action_reaches(config, action, target, true);
    }
}

/// `testAbsoluteDirection`: on the axis it scrolls, an absolute direction, mirrored right to left
/// on a horizontal grid and when reversed; on the other, nothing.
fn grid_absolute(canonical: usize, action: u8, horizontal_action: bool) {
    for config in grid_configs() {
        let mut target = canonical;
        if config.horizontal && config.rtl {
            target = 100 - target - 1;
        }
        if config.reversed {
            target = 100 - target - 1;
        }
        grid_action_reaches(config, action, target, config.horizontal == horizontal_action);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollAccessibilityTest.kt, scrollForward, scrollBackward
#[test]
fn a_grid_scrolls_forward_and_backward_by_its_actions() {
    grid_relative(58, scroll_action::FORWARD);
    grid_relative(41, scroll_action::BACKWARD);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollAccessibilityTest.kt, scrollRight, scrollLeft, scrollDown, scrollUp
#[test]
fn a_grid_scrolls_in_absolute_directions_on_its_axis() {
    grid_absolute(58, scroll_action::RIGHT, true);
    grid_absolute(41, scroll_action::LEFT, true);
    grid_absolute(58, scroll_action::DOWN, false);
    grid_absolute(41, scroll_action::UP, false);
}

/// Compose's `verifyNodeInfoScrollActions`.
fn verify_grid_actions(
    config: GridConfig,
    first: usize,
    offset: i32,
    forward: bool,
    backward: bool,
) {
    use scroll_action::*;
    let host = accessible_grid(config, first, offset);
    let actions = scroll_actions(&the_scrolling_node(&host));
    assert_eq!(actions & FORWARD != 0, forward, "{config:?}");
    assert_eq!(actions & BACKWARD != 0, backward, "{config:?}");
    if config.horizontal {
        let (left, right) = if config.rtl { (forward, backward) } else { (backward, forward) };
        assert_eq!(actions & LEFT != 0, left, "{config:?}");
        assert_eq!(actions & RIGHT != 0, right, "{config:?}");
        assert_eq!(actions & (UP | DOWN), 0, "{config:?}");
    } else {
        assert_eq!(actions & (LEFT | RIGHT), 0, "{config:?}");
        assert_eq!(actions & DOWN != 0, forward, "{config:?}");
        assert_eq!(actions & UP != 0, backward, "{config:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollAccessibilityTest.kt, verifyScrollActionsAtStart, verifyScrollActionsInMiddle, verifyScrollActionsAtEnd
#[test]
fn a_grid_offers_the_actions_its_position_has() {
    for config in grid_configs() {
        verify_grid_actions(config, 0, 0, !config.reversed, config.reversed);
        verify_grid_actions(config, 47, 13, true, true);
        verify_grid_actions(config, 95, 5, config.reversed, !config.reversed);
    }
}

// ---- ACTION_SHOW_ON_SCREEN ------------------------------------------------------------------------

fn tagged_id(host: &UiHost, tag: &str) -> i32 {
    fn visit(node: &compose_ui::platform::SemanticsSnapshot, tag: &str) -> Option<i32> {
        if node.config.get(&SemanticsProperties::TEST_TAG).is_some_and(|it| it.as_str() == tag) {
            return Some(virtual_id(node));
        }
        node.children.iter().find_map(|child| visit(child, tag))
    }
    visit(&host.semantics_tree(true), tag).expect("tagged")
}

fn show(host: &mut UiHost, tag: &str) -> Option<bool> {
    let id = tagged_id(host, tag);
    let tree = host.semantics_tree(true);
    let shown = show_on_screen(host, &tree, id);
    host.settle(20);
    shown
}

fn list_state() -> LazyListState {
    **Box::leak(Box::new(LazyListState::new(0, 0)))
}

#[composable]
fn Square(tag: &'static str, side: i32) {
    BasicText!(tag, modifier = Modifier.test_tag(tag).size(side.dp()));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, testPerformAction_showOnScreen
#[test]
fn showing_a_node_scrolls_its_column_the_least_that_shows_it() {
    let mut host = UiHost::headless(400, 800);
    let state = **Box::leak(Box::new(ScrollState::new(0)));
    host.set_content(move || {
        Box!({
            Column!(modifier = Modifier.size(200.dp()).vertical_scroll(state), {
                Square("backward", 150);
                Square("forward", 150);
            });
        });
    });
    host.settle(20);
    assert_eq!(state.value(), 0);
    assert_eq!(show(&mut host, "forward"), Some(true));
    assert_eq!(state.value(), 100);
    assert_eq!(show(&mut host, "backward"), Some(true));
    assert_eq!(state.value(), 0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, testPerformAction_showOnScreen_lazy
#[test]
fn showing_a_node_scrolls_its_lazy_list() {
    let state = list_state();
    let mut host = UiHost::headless(400, 800);
    host.set_content(move || {
        Box!({
            LazyColumn!(
                modifier = Modifier.size(200.dp()),
                state = state,
                |scope: LazyListScope| {
                    scope.item(|| Square("backward", 150));
                    scope.item(|| Square("forward", 150));
                }
            );
        });
    });
    host.settle(20);
    assert_eq!(show(&mut host, "forward"), Some(true));
    assert_eq!(state.first_visible_item_index(), 0);
    assert_eq!(state.first_visible_item_scroll_offset(), 100);
    assert_eq!(show(&mut host, "backward"), Some(true));
    assert_eq!(state.first_visible_item_scroll_offset(), 0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, testPerformAction_showOnScreen_lazy_withContentPadding
#[test]
fn showing_a_node_clears_a_lazy_lists_content_padding() {
    let state = list_state();
    let mut host = UiHost::headless(400, 800);
    host.set_content(move || {
        Box!({
            LazyColumn!(
                modifier = Modifier.size(200.dp()),
                state = state,
                content_padding = PaddingValues::new(0.dp(), 40.dp(), 0.dp(), 60.dp()),
                |scope: LazyListScope| {
                    scope.item(|| Square("first", 80));
                    scope.item(|| Square("second", 80));
                    scope.item(|| Square("third", 80));
                }
            );
        });
    });
    host.settle(20);
    // Within the list, but under its padding at the bottom.
    assert_eq!(show(&mut host, "second"), Some(true));
    assert_eq!(state.first_visible_item_scroll_offset(), 60);
    assert_eq!(show(&mut host, "first"), Some(true));
    assert_eq!(state.first_visible_item_scroll_offset(), 0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, testPerformAction_showOnScreen_lazy_reverseLayout_withContentPadding
#[test]
fn showing_a_node_of_a_reversed_list_goes_the_other_way() {
    let state = list_state();
    let mut host = UiHost::headless(400, 800);
    host.set_content(move || {
        Box!({
            LazyColumn!(
                modifier = Modifier.size(200.dp()),
                state = state,
                reverse_layout = true,
                content_padding = PaddingValues::new(0.dp(), 60.dp(), 0.dp(), 40.dp()),
                |scope: LazyListScope| {
                    scope.item(|| Square("first", 70));
                    scope.item(|| Square("second", 70));
                    scope.item(|| Square("third", 70));
                }
            );
        });
    });
    host.settle(20);
    assert_eq!(show(&mut host, "second"), Some(true));
    assert_eq!(state.first_visible_item_scroll_offset(), 40);
    assert_eq!(show(&mut host, "first"), Some(true));
    assert_eq!(state.first_visible_item_scroll_offset(), 0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, testPerformAction_showOnScreen_lazyRow_rtl_withContentPadding
#[test]
fn showing_a_node_of_a_right_to_left_row_goes_the_other_way() {
    let state = list_state();
    let mut host = UiHost::headless(400, 800);
    host.set_content(move || {
        compose_runtime::CompositionLocalProvider(
            compose_ui::LOCAL_LAYOUT_DIRECTION.provides(compose_ui::LayoutDirection::Rtl),
            move || {
                Box!({
                    LazyRow!(
                        modifier = Modifier.size(200.dp()),
                        state = state,
                        content_padding = PaddingValues::new(40.dp(), 0.dp(), 60.dp(), 0.dp()),
                        |scope: LazyListScope| {
                            scope.item(|| Square("first", 70));
                            scope.item(|| Square("second", 70));
                            scope.item(|| Square("third", 70));
                        }
                    );
                });
            },
        );
    });
    host.settle(20);
    assert_eq!(show(&mut host, "second"), Some(true));
    assert_eq!(state.first_visible_item_scroll_offset(), 40);
    assert_eq!(show(&mut host, "first"), Some(true));
    assert_eq!(state.first_visible_item_scroll_offset(), 0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, testPerformAction_showOnScreen_lazynested
#[test]
fn showing_a_node_in_nested_lists_leaves_the_outer_one_where_it_shows_already() {
    let outer = list_state();
    let inner = list_state();
    let mut host = UiHost::headless(400, 800);
    host.set_content(move || {
        Box!({
            LazyRow!(modifier = Modifier.size(250.dp()), state = outer, |scope: LazyListScope| {
                scope.item(move || {
                    LazyColumn!(
                        modifier = Modifier.size(200.dp()),
                        state = inner,
                        |scope: LazyListScope| {
                            scope.item(|| Square("backward", 150));
                            scope.item(|| Square("forward", 150));
                        }
                    );
                });
            });
        });
    });
    host.settle(20);
    assert_eq!(show(&mut host, "forward"), Some(true));
    assert_eq!(inner.first_visible_item_scroll_offset(), 100);
    assert_eq!(outer.first_visible_item_scroll_offset(), 0);
    assert_eq!(show(&mut host, "backward"), Some(true));
    assert_eq!(inner.first_visible_item_scroll_offset(), 0);
    assert_eq!(outer.first_visible_item_scroll_offset(), 0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/ShowOnScreenAccessibilityTest.kt, testPerformAction_showOnScreen_noScrollableComposeParent
#[test]
fn with_nothing_around_it_scrolling_the_view_is_asked() {
    let mut host = UiHost::headless(400, 800);
    host.set_content(|| Square("alone", 20));
    host.settle(5);
    assert_eq!(show(&mut host, "alone"), None);
    let kotlin = include_str!(
        "../../../android/compose-rs/src/main/java/rs/compose/android/Accessibility.kt"
    );
    assert!(kotlin.contains("-1 -> view.requestRectangleOnScreen(node.bounds)"));
}

#[test]
fn a_page_of_a_lazy_list_leaves_out_its_content_padding() {
    let state = list_state();
    let mut host = UiHost::headless(400, 800);
    host.set_content(move || {
        LazyColumn!(
            modifier = Modifier.size(200.dp()).test_tag("list"),
            state = state,
            content_padding = PaddingValues::new(0.dp(), 40.dp(), 0.dp(), 60.dp()),
            |scope: LazyListScope| {
                for _ in 0..10 {
                    scope.item(|| Square("item", 80));
                }
            }
        );
    });
    host.settle(20);
    let tree = host.accessibility_tree();
    let id = tagged_id(&host, "list");
    let forward = i32::from(compose_android::accessibility::action::SCROLL_FIRST);
    let mut custom = compose_android::accessibility::KeptCustomActions::default();
    custom.assign(&tree);
    assert!(compose_android::accessibility::perform_action(
        &mut host, &tree, &custom, id, forward, 0.0
    ));
    host.settle(20);
    assert_eq!(state.first_visible_item_index(), 1);
    assert_eq!(state.first_visible_item_scroll_offset(), 20, "100 of the 200");
}
