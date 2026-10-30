//! What `Accessibility.kt` is told by Rust (D1289-D1291): the class and actions of a node, the
//! order it's read in, live regions, what's behind a dialog, and what changed.

#![allow(non_snake_case)]

// The call macros name the runtime `compose`.
extern crate compose_runtime as compose;

use std::cell::Cell;
use std::rc::Rc;

use compose_android::accessibility::{
    Change, ChangeKind, ExtraValue, KeptCustomActions, KeptForChanges, ShapeExtra, action,
    actions_of, character_locations, class_and_role, extras_of, flag, perform_action, read_as,
    shape_extra, spoken, write_shape_extra, write_tree,
};
use compose_android::serialize::Writer;
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::{MutableState, mutable_state_of};
use compose_ui::platform::{SemanticsSnapshot, UiHost};
use compose_ui::*;

/// `content` in a column: at the root, each would be over the one before.
fn host_of(content: impl Fn() + 'static) -> UiHost {
    let mut host = UiHost::headless(400, 800);
    let content = compose_runtime::callback(content);
    host.set_content(move || {
        let content = content.clone();
        Column!({ content() });
    });
    frames(&mut host);
    host
}

fn frames(host: &mut UiHost) {
    host.settle(20);
}

fn by_tag<'a>(node: &'a SemanticsSnapshot, tag: &str) -> Option<&'a SemanticsSnapshot> {
    if node.config.get(&SemanticsProperties::TEST_TAG).is_some_and(|it| it.as_str() == tag) {
        return Some(node);
    }
    node.children.iter().find_map(|child| by_tag(child, tag))
}

fn tagged(host: &UiHost, tag: &str) -> SemanticsSnapshot {
    by_tag(&host.accessibility_tree_with_covered(), tag).expect("tagged").clone()
}

/// The tags of `sorted_for_accessibility`, in the merged tree.
fn order(host: &UiHost) -> Vec<String> {
    let root = host.accessibility_tree();
    sorted_for_accessibility(&root)
        .iter()
        .map(|node| {
            node.config.get(&SemanticsProperties::TEST_TAG).map_or("?".into(), |it| it.to_string())
        })
        .collect()
}

fn says(config: &mut SemanticsConfiguration) {
    config.set_content_description("says");
}

// ---- Classes and roles --------------------------------------------------------------------------

#[test]
fn a_nodes_class_and_role_come_from_its_own_configuration() {
    let host = host_of(|| {
        let role = |role: Role, tag: &'static str| {
            Modifier.semantics(move |config| {
                config.set_role(role);
                config.set_test_tag(tag);
            })
        };
        Box!(modifier = role(Role::Button, "button"), {});
        Box!(modifier = role(Role::Checkbox, "checkbox"), {});
        Box!(modifier = role(Role::RadioButton, "radio"), {});
        Box!(modifier = role(Role::Image, "image"), {});
        Box!(modifier = role(Role::DropdownList, "dropdown"), {});
        Box!(modifier = role(Role::ValuePicker, "picker"), {});
        Box!(modifier = role(Role::Tab, "tab"), {});
        Box!(modifier = role(Role::Switch, "switch"), {});
        // With something below it, the role isn't what's read.
        Box!(modifier = role(Role::Button, "over a child"), {
            Box!(modifier = Modifier.semantics(says), {});
        });
        Box!(modifier = Modifier.semantics(|c| c.set_text("text")).test_tag("text"), {});
        Box!(modifier = Modifier.test_tag("plain"), {});
    });
    let of = |tag| {
        let node = tagged(&host, tag);
        let (class, role) = class_and_role(&node);
        (class.to_string(), role)
    };
    assert_eq!(of("button"), ("android.widget.Button".to_string(), spoken::NONE));
    assert_eq!(of("checkbox"), ("android.widget.CheckBox".to_string(), spoken::NONE));
    assert_eq!(of("radio"), ("android.widget.RadioButton".to_string(), spoken::NONE));
    assert_eq!(of("image"), ("android.widget.ImageView".to_string(), spoken::NONE));
    assert_eq!(of("dropdown"), ("android.widget.Spinner".to_string(), spoken::NONE));
    assert_eq!(of("picker"), ("android.widget.NumberPicker".to_string(), spoken::NONE));
    assert_eq!(of("tab"), ("android.view.View".to_string(), spoken::TAB));
    assert_eq!(of("switch"), ("android.view.View".to_string(), spoken::SWITCH));
    assert_eq!(of("over a child"), ("android.view.View".to_string(), spoken::NONE));
    assert_eq!(of("text"), ("android.widget.TextView".to_string(), spoken::NONE));
    assert_eq!(of("plain"), ("android.view.View".to_string(), spoken::NONE));
}

// ---- Actions ------------------------------------------------------------------------------------

#[test]
fn a_service_is_offered_what_a_node_says_with_its_labels_and_none_when_disabled() {
    let dismissed = Rc::new(Cell::new(0));
    let (expanded, count) = (dismissed.clone(), dismissed.clone());
    let actions = move |config: &mut SemanticsConfiguration| {
        let (expanded, count) = (expanded.clone(), count.clone());
        config.expand(Some("Open it".into()), move || {
            expanded.set(1);
            true
        });
        config.collapse(None, || true);
        config.dismiss(Some("Close it".into()), move || {
            count.set(2);
            true
        });
        config.on_long_click(None, || true);
        config.page_up(None, || true);
        config.set_custom_actions(vec![
            CustomAccessibilityAction::new("First", || true),
            CustomAccessibilityAction::new("Second", || true),
        ]);
    };
    let actions_again = actions.clone();
    let mut host = host_of(move || {
        let enabled = Modifier.semantics(actions.clone()).test_tag("enabled");
        Box!(modifier = enabled, {});
        let disabled = Modifier.semantics(actions_again.clone()).semantics(|c| c.disabled());
        Box!(modifier = disabled.test_tag("disabled"), {});
    });
    let enabled = tagged(&host, "enabled");
    let mut custom = KeptCustomActions::default();
    custom.assign(&enabled);
    let offered = actions_of(&enabled, &custom);
    let labelled: Vec<_> = offered
        .iter()
        .map(|(code, label)| (*code, label.as_ref().map(|it| it.to_string())))
        .collect();
    for wanted in [
        (action::EXPAND, Some("Open it".to_string())),
        (action::COLLAPSE, None),
        (action::DISMISS, Some("Close it".to_string())),
        (action::LONG_CLICK, None),
        (action::PAGE_UP, None),
        (action::CUSTOM_FIRST, Some("First".to_string())),
        (action::CUSTOM_FIRST + 1, Some("Second".to_string())),
    ] {
        assert!(labelled.contains(&wanted), "{wanted:?} in {labelled:?}");
    }
    let disabled = tagged(&host, "disabled");
    custom.assign(&disabled);
    assert!(actions_of(&disabled, &custom).is_empty());

    assert!(host.perform_semantics_action(enabled.id, &SemanticsActions::EXPAND));
    assert_eq!(dismissed.get(), 1);
    assert!(host.perform_semantics_action(enabled.id, &SemanticsActions::DISMISS));
    assert_eq!(dismissed.get(), 2);
}

#[test]
fn a_value_steps_only_where_it_can_and_a_scroller_where_it_can() {
    let host = host_of(|| {
        let at = |current: f32, tag: &'static str| {
            Modifier.semantics(move |config| {
                config.set_progress_bar_range_info(ProgressBarRangeInfo {
                    current,
                    range: (0.0, 1.0),
                    steps: 0,
                });
                config.set_progress(None, |_| true);
                config.set_test_tag(tag);
            })
        };
        Box!(modifier = at(0.0, "at the start"), {});
        Box!(modifier = at(1.0, "at the end"), {});
    });
    let codes = |tag| -> Vec<u8> {
        actions_of(&tagged(&host, tag), &KeptCustomActions::default())
            .iter()
            .map(|(code, _)| *code)
            .collect()
    };
    assert!(codes("at the start").contains(&action::STEP_FORWARD));
    assert!(!codes("at the start").contains(&action::STEP_BACKWARD));
    assert!(codes("at the end").contains(&action::STEP_BACKWARD));
    assert!(!codes("at the end").contains(&action::STEP_FORWARD));
    assert_eq!(class_and_role(&tagged(&host, "at the end")).0, "android.widget.SeekBar");
}

// ---- Live regions and flags ---------------------------------------------------------------------

#[test]
fn a_live_region_and_a_heading_show_in_the_flags() {
    use compose_android::accessibility::flag::*;
    let host = host_of(|| {
        let live = |mode, tag: &'static str| {
            Modifier.semantics(move |config| {
                config.set_live_region(mode);
                config.set_test_tag(tag);
            })
        };
        Box!(modifier = live(LiveRegionMode::Polite, "polite"), {});
        Box!(modifier = live(LiveRegionMode::Assertive, "assertive"), {});
        let heading = Modifier.semantics(|config| config.heading());
        Box!(modifier = heading.test_tag("heading"), {});
    });
    let root = host.accessibility_tree_with_covered();
    let mut out = Writer::default();
    write_tree(&root, &mut KeptCustomActions::default(), &mut out);
    let flags = read_flags(&out.bytes);
    let of = |tag| flags.iter().find(|(id, _)| *id == tagged(&host, tag).semantics_id).unwrap().1;
    assert_ne!(of("polite") & LIVE_POLITE, 0);
    assert_eq!(of("polite") & LIVE_ASSERTIVE, 0);
    assert_ne!(of("assertive") & LIVE_ASSERTIVE, 0);
    assert_ne!(of("heading") & HEADING, 0);
}

/// Each record's id and flags, its strings and the rest stepped over, and who it's read before.
fn records(bytes: &[u8]) -> Vec<(i32, u32, i32)> {
    let mut at = 0;
    let read_u32 = |at: &mut usize| {
        let value = u32::from_le_bytes(bytes[*at..*at + 4].try_into().unwrap());
        *at += 4;
        value
    };
    let skip_string = |at: &mut usize| {
        let length = read_u32(at) as usize;
        *at += length;
    };
    let mut out = Vec::new();
    while at < bytes.len() {
        let id = read_u32(&mut at) as i32;
        read_u32(&mut at);
        at += 16;
        let flags = read_u32(&mut at);
        at += 1;
        for _ in 0..6 {
            skip_string(&mut at);
        }
        if flags & flag::RANGE != 0 {
            at += 12;
        }
        if flags & flag::SCROLL != 0 {
            at += 13;
        }
        let before = read_u32(&mut at) as i32;
        // Its drawing order, class, role, state and percent, selection and granularities.
        at += 4;
        skip_string(&mut at);
        at += 12;
        skip_string(&mut at);
        // Its extras.
        let extras = bytes[at] as usize;
        at += 1;
        for _ in 0..extras {
            skip_string(&mut at);
            let kind = bytes[at];
            at += 1;
            match kind {
                0 | 1 => at += 4,
                2 => at += 1,
                3 => skip_string(&mut at),
                _ => {}
            }
        }
        let actions = bytes[at] as usize;
        at += 1;
        for _ in 0..actions {
            at += 1;
            skip_string(&mut at);
        }
        out.push((id, flags, before));
    }
    out
}

fn read_flags(bytes: &[u8]) -> Vec<(i32, u32)> {
    records(bytes).into_iter().map(|(id, flags, _)| (id, flags)).collect()
}

// ---- The order it's read in ---------------------------------------------------------------------

/// A box at `(x, y)`, `size` square, tagged and saying something.
fn at(x: f32, y: f32, size: f32, tag: &'static str) -> Modifier {
    Modifier.offset(x.dp(), y.dp()).size(size.dp()).semantics(move |config| {
        says(config);
        config.set_test_tag(tag);
    })
}

#[test]
fn nodes_are_read_row_by_row_each_from_its_start() {
    let host = host_of(|| {
        Box!({
            Box!(modifier = at(100.0, 100.0, 50.0, "lower right"), {});
            Box!(modifier = at(0.0, 100.0, 50.0, "lower left"), {});
            Box!(modifier = at(100.0, 0.0, 50.0, "upper right"), {});
            Box!(modifier = at(0.0, 0.0, 50.0, "upper left"), {});
        });
    });
    assert_eq!(order(&host), ["upper left", "upper right", "lower left", "lower right"]);
}

#[test]
fn a_row_is_what_overlaps_vertically_whatever_the_order_in_it() {
    let host = host_of(|| {
        Box!({
            Box!(modifier = at(100.0, 20.0, 50.0, "second, lower"), {});
            Box!(modifier = at(0.0, 0.0, 50.0, "first, higher"), {});
            Box!(modifier = at(0.0, 100.0, 50.0, "next row"), {});
        });
    });
    assert_eq!(order(&host), ["first, higher", "second, lower", "next row"]);
}

#[test]
fn right_to_left_a_row_is_read_from_its_right() {
    let mut host = host_of(|| {
        Row!({
            let start = Modifier.size(50.dp()).semantics(says).test_tag("start");
            Box!(modifier = start, {});
            let end = Modifier.size(50.dp()).semantics(says).test_tag("end");
            Box!(modifier = end, {});
        });
    });
    host.set_layout_direction(LayoutDirection::Rtl);
    frames(&mut host);
    assert_eq!(order(&host), ["start", "end"]);
    let root = host.accessibility_tree();
    let start = by_tag(&root, "start").unwrap();
    assert!(start.bounds.left > by_tag(&root, "end").unwrap().bounds.left, "start is on the right");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, testSortedAccessibilityNodeInfo_traversalIndex
#[test]
fn a_traversal_index_moves_a_node_after_the_rest_and_the_rest_stay_in_place() {
    let host = host_of(|| {
        Box!({
            let last =
                at(0.0, 0.0, 50.0, "first in place").semantics(|c| c.set_traversal_index(1.0));
            Box!(modifier = last, {});
            Box!(modifier = at(0.0, 100.0, 50.0, "second in place"), {});
            let first =
                at(0.0, 200.0, 50.0, "third in place").semantics(|c| c.set_traversal_index(-1.0));
            Box!(modifier = first, {});
        });
    });
    assert_eq!(order(&host), ["third in place", "second in place", "first in place"]);
}

#[test]
fn a_traversal_group_is_read_together_and_said_only_if_it_says_something() {
    let host = host_of(|| {
        Box!({
            let group = Modifier
                .offset(0.dp(), 0.dp())
                .size((100.dp(), 300.dp()))
                .semantics(|config| config.set_is_traversal_group(true));
            Box!(modifier = group, {
                Box!(modifier = at(0.0, 0.0, 50.0, "in the group, top"), {});
                Box!(modifier = at(0.0, 200.0, 50.0, "in the group, bottom"), {});
            });
            Box!(modifier = at(200.0, 100.0, 50.0, "beside it"), {});
        });
    });
    assert_eq!(
        order(&host),
        ["in the group, top", "in the group, bottom", "beside it"],
        "without the group, the one beside it would come between them"
    );
}

/// Two side by side, each with a top and a bottom text: `modifier` on each column.
fn two_columns(modifier: impl Fn(u8) -> Modifier + 'static) -> UiHost {
    let modifier = Rc::new(modifier);
    host_of(move || {
        let modifier = modifier.clone();
        Row!({
            for column in [1, 2] {
                let modifier = modifier(column).width(50.dp());
                Column!(modifier = modifier, {
                    let text = |row: &'static str| {
                        let tag = format!("{row} {column}");
                        Modifier.size(50.dp()).semantics(says).test_tag(tag)
                    };
                    Box!(modifier = text("top"), {});
                    Box!(modifier = text("bottom"), {});
                });
            }
        });
    })
}

fn group() -> Modifier {
    Modifier.semantics(|config| config.set_is_traversal_group(true))
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, testSortedAccessibilityNodeInfo_nestedTraversalGroups_outerFalse
#[test]
fn groups_side_by_side_are_each_read_whole() {
    let host = two_columns(|_| group());
    assert_eq!(order(&host), ["top 1", "bottom 1", "top 2", "bottom 2"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, testSortedAccessibilityNodeInfo_nestedTraversalGroups_outerTrue
#[test]
fn columns_that_are_no_groups_are_read_across_by_rows() {
    let host = two_columns(|_| Modifier.semantics(|c| c.set_is_traversal_group(false)));
    assert_eq!(order(&host), ["top 1", "top 2", "bottom 1", "bottom 2"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, testSortedAccessibilityNodeInfo_peerTraversalGroups_traversalIndex
#[test]
fn a_groups_traversal_index_moves_it_whole() {
    let host = two_columns(|column| {
        let index = if column == 1 { 1.0 } else { 0.0 };
        group().semantics(move |config| config.set_traversal_index(index))
    });
    assert_eq!(order(&host), ["top 2", "bottom 2", "top 1", "bottom 1"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, testSortedAccessibilityNodeInfo_readableTraversalGroups
#[test]
fn a_group_that_is_focused_on_itself_comes_before_what_is_in_it() {
    let host = host_of(|| {
        let row =
            group().semantics_with(true, |config| config.set_test_tag("row")).clickable(|| {});
        Row!(modifier = row, {
            let button = Modifier
                .semantics_with(true, |config| config.set_test_tag("button"))
                .clickable(|| {})
                .size(20.dp());
            Box!(modifier = button, {});
        });
    });
    assert_eq!(order(&host), ["row", "button"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, testSortedAccessibilityNodeInfo_clearSemantics
#[test]
fn what_clears_its_semantics_is_not_read_and_the_rest_close_up() {
    let host = host_of(|| {
        Column!({
            Row!({
                Box!(modifier = Modifier.size(20.dp()).semantics(says).test_tag("first"), {});
                let cleared =
                    Modifier.size(20.dp()).test_tag("cleared").clear_and_set_semantics(|_| {});
                Box!(modifier = cleared, {
                    Box!(modifier = Modifier.semantics(says).size(5.dp()), {});
                });
                Box!(modifier = Modifier.size(20.dp()).semantics(says).test_tag("third"), {});
            });
            Box!(modifier = Modifier.size(20.dp()).semantics(says).test_tag("below"), {});
        });
    });
    assert_eq!(order(&host), ["first", "third", "below"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, testSortedAccessibilityNodeInfo_vertical_zIndex, testSortedAccessibilityNodeInfo_horizontal_zIndex, testSortedAccessibilityNodeInfo_vertical_offset, testSortedAccessibilityNodeInfo_horizontal_offset, testSortedAccessibilityNodeInfo_vertical_offset_overlapped
#[test]
fn where_a_node_is_shown_decides_not_its_z_index_nor_when_it_was_composed() {
    let tagged_size =
        |tag: &'static str| Modifier.required_size(50.dp()).semantics(says).test_tag(tag);
    let host = host_of(move || {
        Column!({
            Box!(modifier = tagged_size("first").z_index(1.0), {});
            Box!(modifier = tagged_size("second"), {});
        });
    });
    assert_eq!(order(&host), ["first", "second"]);
    let host = host_of(move || {
        Row!({
            Box!(modifier = tagged_size("first").z_index(1.0), {});
            Box!(modifier = tagged_size("second"), {});
        });
    });
    assert_eq!(order(&host), ["first", "second"]);
    for (x, y) in [(0.0, 50.0), (50.0, 0.0), (0.0, 20.0)] {
        let host = host_of(move || {
            Box!({
                let moved = Modifier.required_size(50.dp()).offset(x.dp(), y.dp());
                Box!(modifier = moved.semantics(says).test_tag("composed first"), {});
                Box!(modifier = tagged_size("composed second"), {});
            });
        });
        assert_eq!(order(&host), ["composed second", "composed first"], "at {x}, {y}");
    }
}

fn ids_written(host: &UiHost) -> Vec<i32> {
    let root = host.accessibility_tree();
    let mut out = Writer::default();
    write_tree(&root, &mut KeptCustomActions::default(), &mut out);
    records(&out.bytes).iter().map(|record| record.0).collect()
}

/// Two children by their traversal index, the second in place with the lower.
fn indexed_children() {
    let child = |tag: &'static str, index: f32| {
        Modifier.size(20.dp()).semantics_with(true, move |config| {
            says(config);
            config.set_traversal_index(index);
            config.set_test_tag(tag);
        })
    };
    Box!(modifier = child("second", 2.0), {});
    Box!(modifier = child("first", 1.0), {});
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/AndroidComposeViewAccessibilityTraversalTest.kt, testMergedDescendants_respectsTraversalIndex
#[test]
fn a_merging_traversal_groups_children_are_written_by_their_traversal_index() {
    let host = host_of(|| {
        let group = Modifier.semantics_with(true, |config| {
            config.set_is_traversal_group(true);
            config.set_test_tag("group");
        });
        Row!(modifier = group, {
            indexed_children();
        });
    });
    let (group, first, second) = (
        tagged(&host, "group").semantics_id,
        tagged(&host, "first").semantics_id,
        tagged(&host, "second").semantics_id,
    );
    assert_eq!(ids_written(&host), [group, first, second]);
}

#[test]
fn anything_else_keeps_its_children_in_drawing_order_whatever_their_index() {
    let host = host_of(|| {
        Row!({
            indexed_children();
        });
    });
    let (first, second) =
        (tagged(&host, "first").semantics_id, tagged(&host, "second").semantics_id);
    assert_eq!(ids_written(&host), [second, first]);
}

#[test]
fn a_rows_cover_narrows_to_what_overlapped_it_so_what_is_past_that_starts_another() {
    let host = host_of(|| {
        Box!({
            // Tall first; then at its top; then at its bottom, to its left.
            let tall = Modifier.offset(100.dp(), 0.dp()).size((50.dp(), 100.dp()));
            Box!(modifier = tall.semantics(says).test_tag("tall"), {});
            let top = Modifier.offset(200.dp(), 0.dp()).size((50.dp(), 50.dp()));
            Box!(modifier = top.semantics(says).test_tag("at its top"), {});
            let bottom = Modifier.offset(0.dp(), 60.dp()).size((50.dp(), 40.dp()));
            Box!(modifier = bottom.semantics(says).test_tag("at its bottom"), {});
        });
    });
    assert_eq!(order(&host), ["tall", "at its top", "at its bottom"]);
}

#[test]
fn what_is_clipped_away_is_not_read_and_does_not_come_first() {
    let host = host_of(|| {
        Column!({
            let bar = Modifier.size(50.dp()).semantics(says).test_tag("app bar");
            Box!(modifier = bar, {});
            let state = compose_foundation::remember_scroll_state!(0);
            Column!(modifier = Modifier.height(100.dp()).vertical_scroll(state), {
                for index in 0..6 {
                    let item =
                        Modifier.size(50.dp()).semantics(says).test_tag(format!("item {index}"));
                    Box!(modifier = item, {});
                }
            });
        });
    });
    assert_eq!(order(&host), ["app bar", "item 0", "item 1"]);
}

#[test]
fn what_a_hidden_group_holds_is_still_read_if_it_is_not_hidden_itself() {
    let host = host_of(|| {
        Column!({
            let group = Modifier.semantics(|config| {
                config.set_is_traversal_group(true);
                config.hide_from_accessibility();
            });
            Column!(modifier = group, {
                Box!(modifier = Modifier.size(20.dp()).semantics(says).test_tag("in it"), {});
            });
            Box!(modifier = Modifier.size(20.dp()).semantics(says).test_tag("below"), {});
        });
    });
    assert_eq!(order(&host), ["in it", "below"]);
}

// ---- Dispatch -----------------------------------------------------------------------------------

/// What was done, as a node with every action records it.
fn acting(done: &'static std::cell::RefCell<Vec<&'static str>>) -> Modifier {
    let did = move |what: &'static str| {
        move || {
            done.borrow_mut().push(what);
            true
        }
    };
    Modifier.size(20.dp()).test_tag("acting").semantics(move |config| {
        config.on_click(None, did("click"));
        config.on_long_click(None, did("long click"));
        config.expand(None, did("expand"));
        config.collapse(None, did("collapse"));
        config.dismiss(None, did("dismiss"));
        config.page_up(None, did("page up"));
        config.page_down(None, did("page down"));
        config.page_left(None, did("page left"));
        config.page_right(None, did("page right"));
        config.set_custom_actions(vec![
            CustomAccessibilityAction::new("custom one", did("custom one")),
            CustomAccessibilityAction::new("custom two", did("custom two")),
        ]);
    })
}

#[test]
fn each_code_a_service_asks_with_does_what_it_says() {
    let done: &'static std::cell::RefCell<Vec<&'static str>> = Box::leak(Box::default());
    let mut host = host_of(move || {
        Box!(modifier = acting(done), {});
    });
    let tree = host.accessibility_tree();
    let mut custom = KeptCustomActions::default();
    custom.assign(&tree);
    let id = tagged(&host, "acting").semantics_id;
    for (code, what) in [
        (action::CLICK, "click"),
        (action::LONG_CLICK, "long click"),
        (action::EXPAND, "expand"),
        (action::COLLAPSE, "collapse"),
        (action::DISMISS, "dismiss"),
        (action::PAGE_UP, "page up"),
        (action::PAGE_DOWN, "page down"),
        (action::PAGE_LEFT, "page left"),
        (action::PAGE_RIGHT, "page right"),
        (action::CUSTOM_FIRST, "custom one"),
        (action::CUSTOM_FIRST + 1, "custom two"),
    ] {
        done.borrow_mut().clear();
        assert!(perform_action(&mut host, &tree, &custom, id, code as i32, 0.0), "{what}");
        assert_eq!(*done.borrow(), [what]);
    }
    done.borrow_mut().clear();
    assert!(!perform_action(&mut host, &tree, &custom, id, action::CUSTOM_FIRST as i32 + 2, 0.0));
    assert!(!perform_action(&mut host, &tree, &custom, -5, action::CLICK as i32, 0.0));
    assert!(!perform_action(&mut host, &tree, &custom, id, 500, 0.0));
    assert!(done.borrow().is_empty());
}

// As `labelToActionId`: what was shown a service stays that action, or nothing.
#[test]
fn a_custom_action_keeps_its_id_as_others_come_and_go_and_a_label_gone_does_nothing() {
    let done: &'static std::cell::RefCell<Vec<&'static str>> = Box::leak(Box::default());
    let labels = Box::leak(Box::new(mutable_state_of(vec!["one", "two"]))).handle();
    let mut host = host_of(move || {
        let modifier = Modifier.size(20.dp()).test_tag("node").semantics(move |config| {
            let actions = labels.get();
            config.set_custom_actions(
                actions
                    .into_iter()
                    .map(|label| {
                        CustomAccessibilityAction::new(label, move || {
                            done.borrow_mut().push(label);
                            true
                        })
                    })
                    .collect(),
            );
        });
        Box!(modifier = modifier, {});
    });
    let mut custom = KeptCustomActions::default();
    custom.assign(&host.accessibility_tree());
    let id = tagged(&host, "node").semantics_id;
    let (one, two) = (action::CUSTOM_FIRST as i32, action::CUSTOM_FIRST as i32 + 1);

    // A service was shown "two" as its code; "one" goes, "three" comes.
    labels.set(vec!["two", "three"]);
    frames(&mut host);
    let tree = host.accessibility_tree();
    custom.assign(&tree);
    assert!(perform_action(&mut host, &tree, &custom, id, two, 0.0));
    assert_eq!(*done.borrow(), ["two"], "not what's second now");
    done.borrow_mut().clear();
    assert!(perform_action(&mut host, &tree, &custom, id, one, 0.0));
    assert_eq!(*done.borrow(), ["three"], "the id that came free");
    done.borrow_mut().clear();

    // Asked before the tree was read again: the label's gone.
    labels.set(vec!["three"]);
    frames(&mut host);
    let tree = host.accessibility_tree();
    assert!(!perform_action(&mut host, &tree, &custom, id, two, 0.0), "\"two\" is gone");
    assert!(done.borrow().is_empty());
}

#[test]
fn what_is_hidden_or_says_nothing_is_not_read_and_each_record_names_the_next() {
    let host = host_of(|| {
        Box!({
            Box!(modifier = at(0.0, 0.0, 50.0, "first"), {});
            Box!(modifier = Modifier.alpha(0.0), {
                Box!(modifier = at(0.0, 50.0, 50.0, "transparent"), {});
            });
            let hiding = at(0.0, 100.0, 50.0, "hiding").semantics(|c| c.hide_from_accessibility());
            Box!(modifier = hiding, {});
            Box!(modifier = Modifier.offset(0.dp(), 150.dp()).size(50.dp()).test_tag("silent"), {});
            Box!(modifier = at(0.0, 200.0, 50.0, "last"), {});
        });
    });
    assert_eq!(order(&host), ["first", "last"]);

    let root = host.accessibility_tree_with_covered();
    let mut out = Writer::default();
    write_tree(&root, &mut KeptCustomActions::default(), &mut out);
    let read = records(&out.bytes);
    let of = |tag| {
        let id = tagged(&host, tag).semantics_id;
        *read.iter().find(|record| record.0 == id).expect("written")
    };
    assert_eq!(of("first").2, tagged(&host, "last").semantics_id);
    assert_eq!(of("last").2, -1);
    assert_ne!(of("transparent").1 & flag::HIDDEN, 0);
    assert_ne!(of("hiding").1 & flag::HIDDEN, 0);
    assert_eq!(of("first").1 & flag::HIDDEN, 0);
    assert_ne!(of("first").1 & flag::SCREEN_READER_FOCUSABLE, 0);
    assert_eq!(of("hiding").1 & flag::SCREEN_READER_FOCUSABLE, 0);
    assert_eq!(of("silent").1 & flag::SCREEN_READER_FOCUSABLE, 0);
}

// ---- A dialog -----------------------------------------------------------------------------------

fn field_and_dialog(open: MutableState<bool>) -> UiHost {
    host_of(move || {
        Column!({
            let says_behind = Modifier.semantics(says).test_tag("behind");
            Box!(modifier = says_behind.size(10.dp()).focusable(), {});
            if open.get() {
                Dialog!(on_dismiss_request = || {}, {
                    let says_inside = Modifier.semantics(says).test_tag("inside");
                    Box!(modifier = says_inside.size(10.dp()), {});
                });
            }
        });
    })
}

// What Compose's separate windows give: a service reads the active window's nodes (ui-input 24).
#[test]
fn a_service_reads_the_dialog_and_nothing_behind_it_until_it_closes() {
    let open = Box::leak(Box::new(mutable_state_of(false))).handle();
    let mut host = field_and_dialog(open);
    assert!(by_tag(&host.accessibility_tree(), "behind").is_some());

    open.set(true);
    frames(&mut host);
    let root = host.accessibility_tree();
    assert!(by_tag(&root, "behind").is_none(), "another window's");
    assert!(by_tag(&root, "inside").is_some());
    assert!(by_tag(&host.semantics_tree(true), "behind").is_some(), "the tests still find it");

    open.set(false);
    frames(&mut host);
    assert!(by_tag(&host.accessibility_tree(), "behind").is_some());
}

// ---- What changed -------------------------------------------------------------------------------

/// As a service turning on: told only what changes from now.
fn kept_from_now() -> KeptForChanges {
    let mut kept = KeptForChanges::default();
    kept.forget();
    kept
}

fn changes_after(
    host: &mut UiHost,
    kept: &mut KeptForChanges,
    change: impl FnOnce(),
) -> Vec<(i32, ChangeKind, String)> {
    change();
    frames(host);
    let tree = host.accessibility_tree();
    let changes: Vec<Change> = kept.changes(host, &tree);
    changes.into_iter().map(|it| (it.id, it.kind, it.text)).collect()
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/accessibility/WindowContentChangeTest.kt, sendWindowContentChangeUndefinedEventByDefault_whenPropertyAdded, sendWindowContentChangeUndefinedEventByDefault_whenPropertyRemoved, sendWindowContentChangeUndefinedEventByDefault_onlyOnce_whenMultiplePropertiesChange
#[test]
fn a_property_with_no_change_type_of_its_own_tells_one_change_with_none() {
    let heading = Box::leak(Box::new(mutable_state_of(false))).handle();
    let mut host = host_of(move || {
        let modifier = Modifier.test_tag("node").size(10.dp()).semantics(|config| {
            says(config);
            if heading.get() {
                config.heading();
                config.set_is_traversal_group(true);
            }
        });
        Box!(modifier = modifier, {});
    });
    let mut kept = kept_from_now();
    assert!(changes_after(&mut host, &mut kept, || {}).is_empty(), "nothing the first time");
    assert!(changes_after(&mut host, &mut kept, || {}).is_empty(), "nothing changed");
    let id = tagged(&host, "node").semantics_id;
    assert_eq!(
        changes_after(&mut host, &mut kept, || heading.set(true)),
        [(id, ChangeKind::Undefined, String::new())],
    );
    assert_eq!(
        changes_after(&mut host, &mut kept, || heading.set(false)),
        [(id, ChangeKind::Undefined, String::new())],
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/accessibility/WindowContentChangeTest.kt, sendWindowContentChangeUndefinedEventByDefault_standardActionWithTheSameLabel, sendWindowContentChangeUndefinedEventByDefault_standardActionWithDifferentLabels, sendWindowContentChangeUndefinedEventByDefault_customActionWithTheSameLabel, sendWindowContentChangeUndefinedEventByDefault_customActionWithDifferentLabels
#[test]
fn an_actions_label_changing_tells_one_change_with_no_type_and_the_same_label_none() {
    let label = Box::leak(Box::new(mutable_state_of("one"))).handle();
    let again = Box::leak(Box::new(mutable_state_of(0))).handle();
    let host_with = move |custom: bool| {
        host_of(move || {
            again.get();
            let modifier = Modifier.test_tag("node").size(10.dp()).semantics(move |config| {
                if custom {
                    config.set_custom_actions(vec![CustomAccessibilityAction::new(
                        label.get(),
                        || true,
                    )]);
                } else {
                    config.on_click(Some(label.get().into()), || true);
                }
            });
            Box!(modifier = modifier, {});
        })
    };
    for custom in [false, true] {
        label.set("one");
        let mut host = host_with(custom);
        let mut kept = kept_from_now();
        changes_after(&mut host, &mut kept, || {});
        let id = tagged(&host, "node").semantics_id;
        let same = changes_after(&mut host, &mut kept, || again.set(again.get() + 1));
        assert!(same.is_empty(), "{same:?}");
        assert_eq!(
            changes_after(&mut host, &mut kept, || label.set("two")),
            [(id, ChangeKind::Undefined, String::new())],
        );
    }
}

#[test]
fn state_toggle_error_description_and_text_each_tell_their_kind() {
    let step = Box::leak(Box::new(mutable_state_of(0))).handle();
    let mut host = host_of(move || {
        let modifier = Modifier.test_tag("node").size(10.dp()).semantics(|config| {
            let step = step.get();
            config.set_state_description(if step >= 1 { "on" } else { "off" });
            config.set_toggleable_state(if step >= 2 {
                ToggleableState::On
            } else {
                ToggleableState::Off
            });
            if step >= 3 {
                config.error("wrong");
            }
            config.set_content_description(if step >= 4 { "changed" } else { "was" });
            config.set_text(if step >= 5 { "new" } else { "old" });
        });
        Box!(modifier = modifier, {});
    });
    let mut kept = kept_from_now();
    changes_after(&mut host, &mut kept, || {});
    let id = tagged(&host, "node").semantics_id;
    let kinds = |changes: Vec<(i32, ChangeKind, String)>| {
        assert!(changes.iter().all(|it| it.0 == id));
        changes.into_iter().map(|it| it.1).collect::<Vec<_>>()
    };
    assert_eq!(
        kinds(changes_after(&mut host, &mut kept, || step.set(1))),
        [ChangeKind::StateDescription, ChangeKind::Undefined]
    );
    assert_eq!(
        kinds(changes_after(&mut host, &mut kept, || step.set(2))),
        [ChangeKind::Checked, ChangeKind::Undefined]
    );
    assert_eq!(kinds(changes_after(&mut host, &mut kept, || step.set(3))), [ChangeKind::Error]);
    let described = changes_after(&mut host, &mut kept, || step.set(4));
    assert_eq!(described, [(id, ChangeKind::ContentDescription, "changed".to_string())]);
    // A plain text has no type of its own.
    assert_eq!(kinds(changes_after(&mut host, &mut kept, || step.set(5))), [ChangeKind::Undefined]);
}

#[test]
fn a_pane_tells_it_appears_changes_its_title_and_disappears() {
    let title = Box::leak(Box::new(mutable_state_of(None::<&'static str>))).handle();
    let mut host = host_of(move || {
        Box!(modifier = Modifier.size(10.dp()), {
            if let Some(title) = title.get() {
                let pane = Modifier.semantics(move |config| config.set_pane_title(title));
                Box!(modifier = pane.size(5.dp()).test_tag("pane"), {});
            }
        });
    });
    let mut kept = kept_from_now();
    changes_after(&mut host, &mut kept, || {});

    let appeared = changes_after(&mut host, &mut kept, || title.set(Some("Menu")));
    let id = tagged(&host, "pane").semantics_id;
    assert_eq!(
        appeared,
        [
            (-1, ChangeKind::Subtree, String::new()),
            (id, ChangeKind::PaneAppeared, "Menu".to_string()),
        ]
    );
    assert_eq!(
        changes_after(&mut host, &mut kept, || title.set(Some("Sheet"))),
        [(id, ChangeKind::PaneTitle, "Sheet".to_string())]
    );
    assert_eq!(
        changes_after(&mut host, &mut kept, || title.set(None)),
        [
            (-1, ChangeKind::Subtree, String::new()),
            (id, ChangeKind::PaneDisappeared, "Sheet".to_string()),
        ]
    );
}

#[test]
fn a_tab_selected_tells_it_with_its_text_and_one_left_with_no_type() {
    let selected = Box::leak(Box::new(mutable_state_of(false))).handle();
    let mut host = host_of(move || {
        let tab = Modifier.test_tag("tab").size(10.dp()).semantics(|config| {
            config.set_role(Role::Tab);
            config.set_text("Tab one");
            config.set_selected(selected.get());
        });
        Box!(modifier = tab, {});
    });
    let mut kept = kept_from_now();
    changes_after(&mut host, &mut kept, || {});
    let id = tagged(&host, "tab").semantics_id;
    assert_eq!(
        changes_after(&mut host, &mut kept, || selected.set(true)),
        [(id, ChangeKind::TabSelected, "Tab one".to_string())]
    );
    assert_eq!(
        changes_after(&mut host, &mut kept, || selected.set(false)),
        [(id, ChangeKind::Undefined, String::new())]
    );
}

#[test]
fn what_moved_or_came_tells_a_subtree_and_forgetting_tells_nothing_of_before() {
    let moved = Box::leak(Box::new(mutable_state_of(0.0_f32))).handle();
    let mut host = host_of(move || {
        let moving = Modifier.offset(moved.get().dp(), 0.dp());
        Box!(modifier = moving.size(10.dp()).semantics(says), {});
    });
    let mut kept = kept_from_now();
    changes_after(&mut host, &mut kept, || {});
    assert_eq!(
        changes_after(&mut host, &mut kept, || moved.set(5.0)),
        [(-1, ChangeKind::Subtree, String::new())]
    );
    kept.forget();
    assert!(changes_after(&mut host, &mut kept, || moved.set(10.0)).is_empty());
}

#[test]
fn the_first_tell_with_no_service_turning_on_tells_the_content_from_an_empty_root() {
    let mut host = host_of(|| {
        let pane = Modifier.size(5.dp()).semantics(|config| config.set_pane_title("Menu"));
        Box!(modifier = pane.test_tag("pane"), {});
    });
    let mut kept = KeptForChanges::default();
    let id = tagged(&host, "pane").semantics_id;
    assert_eq!(
        changes_after(&mut host, &mut kept, || {}),
        [
            (-1, ChangeKind::Subtree, String::new()),
            (id, ChangeKind::PaneAppeared, "Menu".to_string()),
        ]
    );
}

#[test]
fn a_pane_title_added_to_a_node_there_tells_it_appears() {
    let title = Box::leak(Box::new(mutable_state_of(false))).handle();
    let mut host = host_of(move || {
        let pane = Modifier.size(5.dp()).test_tag("node").semantics(|config| {
            if title.get() {
                config.set_pane_title("Menu");
            }
        });
        Box!(modifier = pane, {});
    });
    let mut kept = kept_from_now();
    changes_after(&mut host, &mut kept, || {});
    let id = tagged(&host, "node").semantics_id;
    let added = changes_after(&mut host, &mut kept, || title.set(true));
    assert!(added.contains(&(id, ChangeKind::PaneAppeared, "Menu".to_string())), "{added:?}");
    let gone = changes_after(&mut host, &mut kept, || title.set(false));
    assert!(gone.contains(&(id, ChangeKind::PaneDisappeared, "Menu".to_string())), "{gone:?}");
}

fn all_changes(host: &UiHost, kept: &mut KeptForChanges) -> Vec<Change> {
    let tree = host.accessibility_tree();
    kept.changes(host, &tree)
}

fn field_of(value: MutableState<String>, masked: MutableState<bool>) -> UiHost {
    host_of(move || {
        let transformation = if masked.get() {
            PasswordVisualTransformation::default().into()
        } else {
            VisualTransformation::NONE
        };
        BasicTextField!(
            value.get(),
            move |it| value.set(it),
            visual_transformation = transformation,
            modifier = Modifier.size(100.dp()).test_tag("field")
        );
    })
}

// A service echoes what's typed from this, and reads the cursor; `password` keeps a password's
// from being read out.
#[test]
fn typing_tells_the_edit_and_the_cursor_and_a_password_says_it_is_one() {
    for masked in [false, true] {
        let value = Box::leak(Box::new(mutable_state_of(String::from("hello")))).handle();
        let masked_state = Box::leak(Box::new(mutable_state_of(masked))).handle();
        let mut host = field_of(value, masked_state);
        let id = tagged(&host, "field").semantics_id;
        let field = tagged(&host, "field").id;
        host.request_focus(field);
        host.send_key(KeyEvent::down(Key::MoveEnd));
        frames(&mut host);
        let mut kept = kept_from_now();
        all_changes(&host, &mut kept);

        host.send_text("!");
        frames(&mut host);
        let changes = all_changes(&host, &mut kept);
        let edit = changes
            .iter()
            .find(|change| change.kind == ChangeKind::TextChanged)
            .unwrap_or_else(|| panic!("{changes:?}"));
        assert_eq!(edit.id, id);
        assert_eq!((edit.from, edit.removed, edit.added), (5, 0, 1));
        assert_eq!(edit.password, masked, "the event says it's a password");
        let cursor = changes
            .iter()
            .find(|change| change.kind == ChangeKind::TextSelection)
            .unwrap_or_else(|| panic!("{changes:?}"));
        assert_eq!((cursor.from, cursor.removed, cursor.added), (6, 6, 6));
        assert_eq!(cursor.password, masked);
        if masked {
            assert!(!edit.text.contains('!') && !edit.before.contains("hello"), "{edit:?}");
        } else {
            assert_eq!((edit.before.as_str(), edit.text.as_str()), ("hello", "hello!"));
        }
    }
}

#[test]
fn an_edit_in_the_middle_counts_in_utf16_from_the_common_start_and_end() {
    let value = Box::leak(Box::new(mutable_state_of(String::from("a😀b")))).handle();
    let masked = Box::leak(Box::new(mutable_state_of(false))).handle();
    let mut host = field_of(value, masked);
    let mut kept = kept_from_now();
    all_changes(&host, &mut kept);
    value.set(String::from("a😀xyb"));
    frames(&mut host);
    let changes = all_changes(&host, &mut kept);
    let edit = changes.iter().find(|it| it.kind == ChangeKind::TextChanged).expect("told");
    assert_eq!((edit.from, edit.removed, edit.added), (3, 0, 2));
}

#[test]
fn a_password_shown_or_masked_tells_two_cursors_and_no_edit() {
    let value = Box::leak(Box::new(mutable_state_of(String::from("secret")))).handle();
    let masked = Box::leak(Box::new(mutable_state_of(true))).handle();
    let mut host = field_of(value, masked);
    let mut kept = kept_from_now();
    all_changes(&host, &mut kept);
    masked.set(false);
    frames(&mut host);
    let changes = all_changes(&host, &mut kept);
    assert!(!changes.iter().any(|it| it.kind == ChangeKind::TextChanged), "{changes:?}");
    let cursors: Vec<_> =
        changes.iter().filter(|it| it.kind == ChangeKind::TextSelection).collect();
    assert_eq!(cursors.len(), 2, "{changes:?}");
    assert_eq!(cursors[0].from, 0);
}

#[test]
fn a_cursor_moved_by_keys_tells_its_selection() {
    let value = Box::leak(Box::new(mutable_state_of(String::from("hello")))).handle();
    let masked = Box::leak(Box::new(mutable_state_of(false))).handle();
    let mut host = field_of(value, masked);
    let field = tagged(&host, "field").id;
    host.request_focus(field);
    frames(&mut host);
    host.send_key(KeyEvent::down(Key::MoveEnd));
    frames(&mut host);
    let mut kept = kept_from_now();
    all_changes(&host, &mut kept);
    host.send_key(KeyEvent::down(Key::DirectionLeft));
    frames(&mut host);
    let changes = all_changes(&host, &mut kept);
    let cursor = changes.iter().find(|it| it.kind == ChangeKind::TextSelection).expect("told");
    assert_eq!((cursor.from, cursor.removed, cursor.added), (4, 4, 5));
}

#[test]
fn a_cursor_moved_by_a_key_or_a_tap_is_told_at_once() {
    let value = Box::leak(Box::new(mutable_state_of(String::from("hello")))).handle();
    let masked = Box::leak(Box::new(mutable_state_of(false))).handle();
    let mut host = field_of(value, masked);
    let field = tagged(&host, "field");
    host.request_focus(field.id);
    frames(&mut host);
    host.send_key(KeyEvent::down(Key::MoveEnd));
    assert!(host.frame().semantics_changed, "a key");
    let start = Offset::new(field.bounds.left + 1.0, field.bounds.top + 1.0);
    host.send_pointer(PointerEvent::press(start));
    host.send_pointer(PointerEvent::release(start));
    assert!(host.frame().semantics_changed, "a tap");
}

#[test]
fn a_touch_looks_through_what_says_nothing_to_what_is_around_it_not_what_is_under_it() {
    let host = host_of(|| {
        let parent = Modifier.size(50.dp()).semantics(says).test_tag("parent");
        Box!(modifier = parent, {
            let under = Modifier.size(20.dp()).semantics(says).test_tag("under");
            Box!(modifier = under, {});
            Box!(modifier = Modifier.size(20.dp()).test_tag("says nothing"), {});
        });
    });
    let root = host.accessibility_tree();
    assert_eq!(
        compose_android::accessibility::node_at(&root, 5.0, 5.0),
        Some(tagged(&host, "parent").semantics_id)
    );
}

#[test]
fn a_custom_action_is_written_with_the_id_its_label_kept() {
    let labels = Box::leak(Box::new(mutable_state_of(vec!["one", "two"]))).handle();
    let mut host = host_of(move || {
        let modifier = Modifier.size(20.dp()).test_tag("node").semantics(move |config| {
            config.set_custom_actions(
                labels
                    .get()
                    .into_iter()
                    .map(|label| CustomAccessibilityAction::new(label, || true))
                    .collect(),
            );
        });
        Box!(modifier = modifier, {});
    });
    let mut custom = KeptCustomActions::default();
    custom.assign(&host.accessibility_tree());
    labels.set(vec!["two"]);
    frames(&mut host);
    custom.assign(&host.accessibility_tree());
    let offered = actions_of(&tagged(&host, "node"), &custom);
    assert_eq!(offered.len(), 1);
    assert_eq!(offered[0].0, action::CUSTOM_FIRST + 1, "what it was, not its place");
}

#[test]
fn the_focus_coming_to_a_node_tells_it_and_a_focused_node_offers_to_clear_it() {
    let mut host = host_of(|| {
        Box!(modifier = Modifier.size(20.dp()).focusable().test_tag("node"), {});
    });
    let mut kept = kept_from_now();
    all_changes(&host, &mut kept);
    let node = tagged(&host, "node");
    let codes = |node: &SemanticsSnapshot| -> Vec<u8> {
        actions_of(node, &KeptCustomActions::default()).iter().map(|it| it.0).collect()
    };
    assert!(codes(&node).contains(&action::FOCUS));
    let tree = host.accessibility_tree();
    let custom = KeptCustomActions::default();
    assert!(perform_action(
        &mut host,
        &tree,
        &custom,
        node.semantics_id,
        action::FOCUS as i32,
        0.0
    ));
    frames(&mut host);
    let changes = all_changes(&host, &mut kept);
    assert!(
        changes.iter().any(|it| it.id == node.semantics_id && it.kind == ChangeKind::Focused),
        "{changes:?}"
    );
    let node = tagged(&host, "node");
    assert!(codes(&node).contains(&action::CLEAR_FOCUS));
    assert!(!codes(&node).contains(&action::FOCUS));
    let tree = host.accessibility_tree();
    assert!(perform_action(
        &mut host,
        &tree,
        &custom,
        node.semantics_id,
        action::CLEAR_FOCUS as i32,
        0.0
    ));
    frames(&mut host);
    assert!(codes(&tagged(&host, "node")).contains(&action::FOCUS));
}

#[test]
fn a_window_opening_or_closing_tells_it_even_with_no_pane_title() {
    let open = Box::leak(Box::new(mutable_state_of(false))).handle();
    let mut host = field_and_dialog(open);
    let mut kept = kept_from_now();
    all_changes(&host, &mut kept);
    open.set(true);
    frames(&mut host);
    let changes = all_changes(&host, &mut kept);
    let window = changes.iter().find(|it| it.kind == ChangeKind::Window).expect("told");
    assert_ne!(window.id, -1, "from the window's root");
    open.set(false);
    frames(&mut host);
    let changes = all_changes(&host, &mut kept);
    let window = changes.iter().find(|it| it.kind == ChangeKind::Window).expect("told");
    assert_eq!(window.id, -1, "the main window's root is the view");
}

#[test]
fn a_popup_letting_touches_through_hides_nothing_from_a_service() {
    let host = host_of(|| {
        Column!({
            Box!(modifier = Modifier.size(10.dp()).semantics(says).test_tag("behind"), {});
            Popup!(
                properties = PopupProperties {
                    focusable: true,
                    pass_through_outside: true,
                    ..Default::default()
                },
                {
                    Box!(modifier = Modifier.size(10.dp()).semantics(says).test_tag("inside"), {});
                }
            );
        });
    });
    let root = host.accessibility_tree();
    assert!(by_tag(&root, "behind").is_some());
    assert!(by_tag(&root, "inside").is_some());
}

#[test]
fn a_records_long_click_importance_and_drawing_order_are_written() {
    let host = host_of(|| {
        Row!({
            let plain = Modifier.size(20.dp()).test_tag("tag only");
            Box!(modifier = plain, {});
            let long = Modifier.size(20.dp()).test_tag("long").semantics(|config| {
                says(config);
                config.on_long_click(None, || true);
            });
            Box!(modifier = long, {});
        });
    });
    let root = host.accessibility_tree();
    let mut out = Writer::default();
    write_tree(&root, &mut KeptCustomActions::default(), &mut out);
    let read = records(&out.bytes);
    let of = |tag| {
        let id = tagged(&host, tag).semantics_id;
        *read.iter().find(|record| record.0 == id).expect("written")
    };
    assert_ne!(of("long").1 & flag::LONG_CLICKABLE, 0);
    assert_ne!(of("long").1 & flag::IMPORTANT, 0);
    assert_eq!(of("tag only").1 & flag::IMPORTANT, 0);
    assert_eq!(of("tag only").1 & flag::LONG_CLICKABLE, 0);
}

#[test]
fn the_kotlin_says_what_rust_chose() {
    let path = format!(
        "{}/../../android/compose-rs/src/main/java/rs/compose/android/Accessibility.kt",
        env!("CARGO_MANIFEST_DIR")
    );
    let kotlin = std::fs::read_to_string(&path).expect("there");
    for line in [
        // uiautomator's dump is no service to send events to.
        "manager\n                .getEnabledAccessibilityServiceList(AccessibilityServiceInfo.FEEDBACK_ALL_MASK)",
        "info.drawingOrder = node.drawingOrder",
        "info.isImportantForAccessibility = node.flags and IMPORTANT != 0",
        "info.isLongClickable = node.flags and LONG_CLICKABLE != 0",
        "val found = view.semanticsNodeAt(x, y)",
        "view.post(changed)",
        // A source written after sensitivity would overwrite it with the view's.
        "scrolled.setSource(view, node.id)\n            markSensitive(scrolled, node.id)",
        "else event.setSource(view, id)\n        // After the source, which sets it from the host view's.\n        markSensitive(event, id)",
        "event.setSource(view, virtualViewId)\n        event.packageName = view.context.packageName\n        markSensitive(event, virtualViewId)",
        "event.setAccessibilityDataSensitive((nodes.get(id)?.flags ?: 0) and SENSITIVE != 0)",
        // Only an accessibility tool is told of what's sensitive, or acts on it.
        "node.flags and SENSITIVE != 0 && !fromTool",
        "if (hiddenFromService(node, fromTool)) return null",
        "if (nodes.get(child)?.let { hiddenFromService(it, fromTool) } != true) info.addChild(view, child)",
        "if (hiddenFromService(node, isRequestFromAccessibilityTool())) return false",
        // Paste is offered with something to paste.
        "if (code == CODE_PASTE && !clipboardHasText()) continue",
        "?.hasMimeType(\"text/*\") == true",
        "info.isEditable = node.flags and CAN_EDIT != 0",
        // A key for what this shape hasn't sets nothing, as Compose's.
        "if (key != EXTRA_SHAPE_TYPE_KEY && key != EXTRA_SHAPE_REGION_KEY) return",
        "if (key == EXTRA_SHAPE_REGION_KEY || (key == EXTRA_SHAPE_CORNERS_KEY && kind == 0)) return",
        "if (key != EXTRA_SHAPE_CORNERS_KEY) extras.putParcelable(EXTRA_SHAPE_RECT_KEY, bounds)",
        "if (kind == 1 && key != EXTRA_SHAPE_RECT_KEY) {",
        "region.setPath(path, Region(bounds))",
    ] {
        assert!(kotlin.contains(line), "Accessibility.kt lacks {line:?}");
    }
    assert!(
        !kotlin.contains("event.isEnabled = true\n        markSensitive"),
        "before its source, sensitivity is overwritten"
    );
}

// ---- What a node says of itself beyond its info (D1652) -----------------------------------------

/// Record flags by test tag: what `write_tree` wrote for each tagged node.
fn written_flags(host: &UiHost, tags: &[&str]) -> Vec<u32> {
    let root = host.accessibility_tree();
    let mut out = Writer::default();
    write_tree(&root, &mut KeptCustomActions::default(), &mut out);
    let records = records(&out.bytes);
    tags.iter()
        .map(|tag| {
            let id = by_tag(&root, tag).expect("tagged").semantics_id;
            records.iter().find(|record| record.0 == id).expect("written").1
        })
        .collect()
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, testTestTagsAsResourceId
#[test]
fn test_tags_are_resource_ids_where_the_nearest_around_says_so() {
    let host = host_of(|| {
        Column!({
            Box!(modifier = Modifier.size(10.dp()).test_tag("none"));
            Box!(modifier = Modifier.semantics(|it| it.set_test_tags_as_resource_id(true)), {
                Box!(modifier = Modifier.size(10.dp()).test_tag("on"));
            });
            Box!(modifier = Modifier.semantics(|it| it.set_test_tags_as_resource_id(false)), {
                Box!(modifier = Modifier.size(10.dp()).test_tag("off"));
            });
            Box!(modifier = Modifier.semantics(|it| it.set_test_tags_as_resource_id(true)), {
                Box!(modifier = Modifier.semantics(|it| it.set_test_tags_as_resource_id(false)), {
                    Box!(modifier = Modifier.size(10.dp()).test_tag("off inside on"));
                });
            });
            Box!(modifier = Modifier.semantics(|it| it.set_test_tags_as_resource_id(false)), {
                Box!(modifier = Modifier.semantics(|it| it.set_test_tags_as_resource_id(true)), {
                    Box!(modifier = Modifier.size(10.dp()).test_tag("on inside off"));
                });
            });
        });
    });
    let tags = ["none", "on", "off", "off inside on", "on inside off"];
    let ids: Vec<bool> = written_flags(&host, &tags)
        .into_iter()
        .map(|flags| flags & flag::TAG_IS_RESOURCE_ID != 0)
        .collect();
    assert_eq!(ids, [false, true, false, false, true]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, testCreateAccessibilityNodeInfo_visibleToToolIfSensitiveData, testCreateAccessibilityNodeInfo_visibleToNontoolIfNotSensitiveData, testCreateAccessibilityNodeInfo_hiddenFromNontoolIfSensitiveData
#[test]
fn sensitive_data_says_so_in_its_record() {
    let host = host_of(|| {
        Column!({
            BasicText!(
                "secret",
                modifier =
                    Modifier.test_tag("sensitive").semantics(|it| it.set_is_sensitive_data(true))
            );
            BasicText!("plain", modifier = Modifier.test_tag("plain"));
            // What's merged in makes it all sensitive, as Compose reads its merged configuration.
            Box!(modifier = Modifier.test_tag("merged").clickable(|| {}), {
                BasicText!(
                    "secret",
                    modifier = Modifier.semantics(|it| it.set_is_sensitive_data(true))
                );
            });
        });
    });
    let flags = written_flags(&host, &["sensitive", "plain", "merged"]);
    let sensitive: Vec<bool> = flags.into_iter().map(|it| it & flag::SENSITIVE != 0).collect();
    assert_eq!(sensitive, [true, false, true]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidComposeViewAccessibilityDelegateCompatTest.kt, className_overwrittenBy_explicitaccessibilityClassNameProperty
#[test]
fn the_class_the_application_names_wins() {
    let host = host_of(|| {
        Box!(
            modifier = Modifier
                .size(10.dp())
                .test_tag("named")
                .clickable_with(
                    ClickableOptions { role: Some(Role::Button), ..Default::default() },
                    || {},
                )
                .semantics(|it| it.set_accessibility_class_name("com.example.Thing"))
        );
    });
    assert_eq!(class_and_role(&tagged(&host, "named")).0, "com.example.Thing");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, testCharacterLocations_withSemanticsBeforePadding_reportsCorrectBoundingBoxes
#[test]
fn characters_are_found_inside_what_is_around_their_text() {
    let mut host = host_of(|| {
        BasicText!("text", modifier = Modifier.semantics(|_| {}).padding(24.dp()).test_tag("text"));
    });
    let node = tagged(&host, "text");
    let places = character_locations(&mut host, &node, 0, 1).expect("laid out");
    let place = places[0].expect("shown");
    assert_eq!((place.left, place.top), (24.0, 24.0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, testAddExtraDataToAccessibilityNodeInfo_notMerged
#[test]
fn a_fields_characters_are_found_and_those_past_it_are_not() {
    let mut host = host_of(|| {
        BasicTextField!(String::from("texy"), |_| {}, modifier = Modifier.test_tag("field"));
    });
    let node = tagged(&host, "field");
    assert_ne!(flags_of(&node) & flag::TEXT_LAID_OUT, 0);
    let places = character_locations(&mut host, &node, 3, 2).expect("laid out");
    assert!(places[0].is_some());
    assert!(places[1].is_none(), "past the text");
    let first = character_locations(&mut host, &node, 0, 1).expect("laid out")[0].expect("shown");
    let last = places[0].expect("shown");
    assert!(first.left < last.left);
}

#[test]
fn nothing_laid_out_has_no_character_locations() {
    let mut host = host_of(|| {
        Box!(modifier = Modifier.size(10.dp()).test_tag("box").semantics(|_| {}));
    });
    let node = tagged(&host, "box");
    assert_eq!(flags_of(&node) & flag::TEXT_LAID_OUT, 0);
    assert!(character_locations(&mut host, &node, 0, 1).is_none());
}

/// A node's flags, as written: found in the record of the tree it's in.
fn flags_of(node: &SemanticsSnapshot) -> u32 {
    let mut out = Writer::default();
    let mut wrapped = node.clone();
    wrapped.children.clear();
    let root = SemanticsSnapshot { children: vec![wrapped], ..node.clone() };
    write_tree(&root, &mut KeptCustomActions::default(), &mut out);
    records(&out.bytes)[0].1
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidComposeViewAccessibilityDelegateCompatTest.kt, explicitaccessibilityClassNameProperty_childrenNotMergeable
#[test]
fn a_childs_named_class_is_not_merged_up() {
    let host = host_of(|| {
        Box!(modifier = Modifier.size(10.dp()).test_tag("merging").semantics_with(true, |_| {}), {
            Box!(
                modifier =
                    Modifier.size(10.dp()).semantics(|it| it.set_accessibility_class_name("a"))
            );
            Box!(
                modifier =
                    Modifier.size(10.dp()).semantics(|it| it.set_accessibility_class_name("b"))
            );
        });
    });
    assert_eq!(class_and_role(&tagged(&host, "merging")).0, "android.view.View");
}

#[test]
fn characters_are_counted_in_units_a_pair_being_one() {
    let mut host = host_of(|| {
        BasicText!("\u{1f600}a", modifier = Modifier.test_tag("text"));
    });
    let node = tagged(&host, "text");
    let at = |host: &mut UiHost, unit: i32| {
        character_locations(host, &node, unit, 1).expect("laid out")[0].expect("shown")
    };
    let (face, second_half, letter) = (at(&mut host, 0), at(&mut host, 1), at(&mut host, 2));
    assert_eq!(face, second_half, "the pair's");
    assert!(letter.left > face.left);
    assert!(character_locations(&mut host, &node, 3, 1).expect("laid out")[0].is_none());
    // Nothing overflows, and there's a limit to what's made.
    assert!(character_locations(&mut host, &node, i32::MAX, 2).expect("laid out")[1].is_none());
    assert_eq!(character_locations(&mut host, &node, 0, i32::MAX).expect("laid out").len(), 20_000);
}

#[test]
fn a_scrolled_fields_characters_are_where_it_shows_them() {
    let mut host = host_of(|| {
        BasicTextField!(
            "a long text in a short field".to_string(),
            |_| {},
            modifier = Modifier.size((48.dp(), 20.dp())).test_tag("field"),
            single_line = true,
        );
    });
    let node = tagged(&host, "field");
    assert!(character_locations(&mut host, &node, 0, 1).expect("laid out")[0].is_some());
    host.request_focus(node.id);
    frames(&mut host);
    host.send_key(KeyEvent::down(Key::MoveEnd));
    frames(&mut host);
    let node = tagged(&host, "field");
    let units = 28;
    assert!(character_locations(&mut host, &node, 0, 1).expect("laid out")[0].is_none(), "gone");
    let last = character_locations(&mut host, &node, units - 1, 1).expect("laid out")[0];
    assert!(last.expect("in view").right <= node.bounds.right);
}

// ---- A merging node's one record, in place of Compose's fake nodes (D1653) ----------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, testFakeNodeCreated_forContentDescriptionSemantics, testContentDescription_withFakeNode_mergedCorrectly
#[test]
fn a_merging_nodes_description_does_not_hide_its_text() {
    let host = host_of(|| {
        Column!({
            Row!(modifier = Modifier.test_tag("icon first").semantics_with(true, |_| {}), {
                Box!(
                    modifier =
                        Modifier.size(10.dp()).semantics(|it| it.set_content_description("Star"))
                );
                BasicText!("Card 19");
            });
            Row!(modifier = Modifier.test_tag("text first").semantics_with(true, |_| {}), {
                BasicText!("Card 20");
                Box!(
                    modifier =
                        Modifier.size(10.dp()).semantics(|it| it.set_content_description("Star"))
                );
            });
            Column!(
                modifier = Modifier
                    .test_tag("own")
                    .semantics_with(true, |it| it.set_content_description("Test")),
                {
                    BasicText!("Text");
                    Box!(
                        modifier = Modifier
                            .size(10.dp())
                            .semantics(|it| it.set_content_description("Hello"))
                    );
                }
            );
            Column!(
                modifier = Modifier
                    .test_tag("only descriptions")
                    .semantics_with(true, |it| it.set_content_description("Hello")),
                {
                    Box!(modifier = Modifier.semantics(|it| it.set_content_description("World")));
                }
            );
            BasicText!("alone", modifier = Modifier.test_tag("alone"));
        });
    });
    let read = |tag| read_as(&tagged(&host, tag));
    assert_eq!(read("icon first"), ("Card 19".to_string(), "Star Card 19".to_string()));
    assert_eq!(read("text first"), ("Card 20".to_string(), "Card 20 Star".to_string()));
    assert_eq!(read("own"), ("Text".to_string(), "Test Text Hello".to_string()));
    assert_eq!(read("only descriptions"), (String::new(), "Hello World".to_string()));
    assert_eq!(read("alone"), ("alone".to_string(), String::new()));
}

#[test]
fn a_fields_description_is_its_hint_and_leaves_it_its_value() {
    let host = host_of(|| {
        BasicTextField!(
            String::from("value"),
            |_| {},
            modifier = Modifier.test_tag("field"),
            decoration_box = |inner: compose_runtime::Callback| {
                Box!(modifier = Modifier.semantics(|it| it.set_content_description("Search")));
                BasicText!("Label");
                inner();
            },
        );
    });
    assert_eq!(read_as(&tagged(&host, "field")), ("Search Label".to_string(), String::new()));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, testFakeNode_createdForButton, testFakeNode_notCreatedForButton_whenNoChildren
#[test]
fn a_merging_nodes_role_is_its_class_with_or_without_what_is_beside_it() {
    let button = || ClickableOptions { role: Some(Role::Button), ..Default::default() };
    let host = host_of(move || {
        Column!({
            Box!(
                modifier = Modifier.size(10.dp()).test_tag("empty").clickable_with(button(), || {})
            );
            Column!(modifier = Modifier.test_tag("with text").clickable_with(button(), || {}), {
                BasicText!("Text");
            });
            Column!(
                modifier = Modifier.test_tag("with a child").clickable_with(button(), || {}),
                {
                    Box!(modifier = Modifier.size(10.dp()).clickable(|| {}));
                }
            );
        });
    });
    for tag in ["empty", "with text", "with a child"] {
        assert_eq!(class_and_role(&tagged(&host, tag)).0, "android.widget.Button", "{tag}");
    }
}

// ---- What's completely covered is left out (D1655) -----------------------------------------------

/// The node tagged `tag` in what a service is told of.
fn uncovered(host: &UiHost, tag: &str) -> SemanticsSnapshot {
    by_tag(&host.accessibility_tree(), tag).expect("tagged").clone()
}

/// The children of the root that are written, by text.
fn texts_of(host: &UiHost) -> Vec<String> {
    host.accessibility_tree().children.iter().map(|child| read_as(child).0).collect()
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, testAccessibilityNodeInfoTreePruned_completelyCovered
#[test]
fn a_node_a_sibling_completely_covers_is_left_out() {
    let host = host_of(|| {
        Box!(modifier = Modifier.test_tag("parent"), {
            BasicText!("Child One", modifier = Modifier.z_index(1.0).required_size(50.dp()));
            BasicText!("Child Two", modifier = Modifier.required_size(50.dp()));
        });
    });
    let parent = uncovered(&host, "parent");
    assert_eq!(parent.children.len(), 1);
    assert_eq!(read_as(&parent.children[0]).0, "Child One");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, testAccessibilityNodeInfoTreePruned_partiallyCovered
#[test]
fn a_node_partly_covered_stays_with_what_is_left_of_its_bounds() {
    let host = host_of(|| {
        Box!(modifier = Modifier.test_tag("parent"), {
            BasicText!("Child One", modifier = Modifier.z_index(1.0).required_size(100.dp()));
            BasicText!(
                "Child Two",
                modifier = Modifier.test_tag("two").required_size((200.dp(), 100.dp()))
            );
        });
    });
    assert_eq!(uncovered(&host, "parent").children.len(), 2);
    let two = uncovered(&host, "two");
    let left = two.uncovered_bounds.expect("pruned");
    assert_eq!((left.width(), left.height()), (100.0, 100.0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, testAccessibilityNodeInfoTreePruned_testTagOnlyDoesNotPrune
#[test]
fn what_over_it_means_nothing_to_a_service_covers_nothing() {
    let host = host_of(|| {
        Box!(modifier = Modifier.test_tag("parent"), {
            Box!(modifier = Modifier.z_index(1.0).test_tag("one").required_size(50.dp()));
            BasicText!("Child Two", modifier = Modifier.required_size(50.dp()));
        });
    });
    assert_eq!(uncovered(&host, "parent").children.len(), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, testAccessibilityNodeInfoTreePruned_hideFromAccessibilityDoesNotPrune
#[test]
fn what_over_it_is_hidden_covers_nothing() {
    let host = host_of(|| {
        Box!(modifier = Modifier.test_tag("parent"), {
            BasicText!(
                "Child One",
                modifier = Modifier
                    .z_index(1.0)
                    .semantics(|it| it.hide_from_accessibility())
                    .required_size(50.dp())
            );
            BasicText!("Child Two", modifier = Modifier.required_size(50.dp()));
        });
    });
    assert_eq!(uncovered(&host, "parent").children.len(), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidComposeViewAccessibilityDelegateCompatTest.kt, testUncoveredNodes_notPlacedNodes_notIncluded
#[test]
fn what_is_not_placed_is_left_out() {
    let host = host_of(|| {
        Row!(modifier = Modifier.size(10.dp()), {
            for _ in 0..3 {
                Box!(
                    modifier =
                        Modifier.size(10.dp()).semantics(|it| it.set_content_description("a"))
                );
            }
        });
    });
    assert_eq!(host.accessibility_tree().children.len(), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidComposeViewAccessibilityDelegateCompatTest.kt, testUncoveredNodes_zeroBoundsRoot_included
#[test]
fn a_root_of_no_size_is_still_visited() {
    let mut host = UiHost::headless(0, 0);
    host.set_content(|| {});
    frames(&mut host);
    let owner = host.semantics_owner();
    let root = owner.unmerged_root_semantics_node().id();
    let mut visited = Vec::new();
    owner.for_each_uncovered_semantics_node(|node, _, _| visited.push(node.id()));
    assert_eq!(visited, [root]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, clippedChildInScrollContainer_doesNotOccludeAdjacentSiblingTouchTarget
#[test]
fn a_clipped_childs_touch_target_covers_nothing_past_its_clip() {
    let offset = Box::leak(Box::new(mutable_state_of(-12i32))).handle();
    let mut host = host_of(move || {
        Column!(modifier = Modifier.size(200.dp()), {
            Box!(modifier = Modifier.fill_max_width().height(56.dp()), |scope: BoxScope| {
                Box!(
                    modifier = Modifier
                        .size(48.dp())
                        .align(scope, Alignment::BOTTOM_START)
                        .test_tag("top")
                        .clickable(|| {})
                );
            });
            Box!(modifier = Modifier.fill_max_width().height(144.dp()).clip_to_bounds(), {
                Box!(
                    modifier = Modifier
                        .offset(0.dp(), offset.get().dp())
                        .size(100.dp())
                        .test_tag("item")
                        .clickable(|| {})
                );
            });
        });
    });
    assert_eq!(uncovered(&host, "top").uncovered_bounds.expect("pruned").height(), 48.0);
    // 20 of it left in the clip: its target goes past it, and still covers nothing there.
    offset.set(-80);
    frames(&mut host);
    let owner = host.semantics_owner();
    let item = uncovered(&host, "item").semantics_id;
    let mut top_of_item = None;
    owner.unmerged_root_semantics_node();
    fn find_node(node: compose_ui::SemanticsNode, id: i32) -> Option<compose_ui::SemanticsNode> {
        if node.id() == id {
            return Some(node);
        }
        node.children().into_iter().find_map(|child| find_node(child, id))
    }
    if let Some(node) = find_node(owner.unmerged_root_semantics_node(), item) {
        top_of_item = Some(node.touch_bounds_in_root().top);
    }
    assert!(top_of_item.expect("there") < 56.0);
    assert_eq!(uncovered(&host, "top").uncovered_bounds.expect("pruned").height(), 48.0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, overlappingClippedChildrenNearContainerEdge_occludedChildIsPruned
#[test]
fn what_is_covered_inside_a_clip_is_left_out_whatever_sticks_past_it() {
    let host = host_of(|| {
        Box!(modifier = Modifier.size(100.dp()).clip_to_bounds(), {
            for tag in ["primary", "secondary"] {
                Box!(
                    modifier = Modifier
                        .offset(0.dp(), 80.dp())
                        .size((80.dp(), 0.dp()))
                        .test_tag(tag)
                        .clickable(|| {})
                );
            }
        });
    });
    let root = host.accessibility_tree();
    assert_eq!(root.children.len(), 1);
    assert!(by_tag(&root, "primary").is_none());
    assert_eq!(by_tag(&root, "secondary").expect("there").uncovered_bounds.unwrap().height(), 48.0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, overlappingClippedChildrenInContainerCorner_occludedChildIsPruned
#[test]
fn what_is_covered_in_a_corner_of_a_clip_is_left_out() {
    let host = host_of(|| {
        Box!(
            modifier = Modifier.padding(50.dp()).size(100.dp()).clip_to_bounds(),
            |scope: BoxScope| {
                for tag in ["primary", "secondary"] {
                    Box!(
                        modifier = Modifier
                            .align(scope, Alignment::BOTTOM_START)
                            .size(20.dp())
                            .test_tag(tag)
                            .clickable(|| {})
                    );
                }
            }
        );
    });
    let root = host.accessibility_tree();
    assert_eq!(root.children.len(), 1);
    let left = by_tag(&root, "secondary").expect("there").uncovered_bounds.unwrap();
    assert_eq!((left.width(), left.height()), (48.0, 48.0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, clippedChildWithPerpendicularSiblingOverlap_preservesFullTouchTargetBounds
#[test]
fn a_clipped_childs_whole_touch_target_is_told_when_a_sibling_overlaps_a_side() {
    let host = host_of(|| {
        Box!(modifier = Modifier.padding(50.dp()).size(200.dp()), {
            Box!(
                modifier = Modifier.size((100.dp(), 32.dp())).clip_to_bounds(),
                |scope: BoxScope| {
                    Box!(
                        modifier = Modifier
                            .align(scope, Alignment::CENTER_END)
                            .padding((0.dp(), 0.dp(), 4.dp(), 0.dp()))
                            .size(24.dp())
                            .test_tag("dismiss")
                            .clickable(|| {})
                    );
                }
            );
            Box!(
                modifier = Modifier
                    .offset(0.dp(), 40.dp())
                    .size((100.dp(), 32.dp()))
                    .test_tag("lower")
                    .clickable(|| {})
            );
        });
    });
    let left = uncovered(&host, "dismiss").uncovered_bounds.expect("pruned");
    assert_eq!((left.width(), left.height()), (48.0, 48.0));
}

#[test]
fn one_window_over_another_covers_nothing_in_it() {
    let host = host_of(|| {
        Box!({
            BasicText!("under", modifier = Modifier.test_tag("under").size(100.dp()));
            Popup!(properties = PopupProperties::default(), {
                BasicText!("over", modifier = Modifier.size(300.dp()));
            });
        });
    });
    assert!(by_tag(&host.accessibility_tree(), "under").is_some());
}

#[test]
fn the_tree_kept_for_a_keyboard_pays_for_no_pruning_and_a_service_gets_it_pruned() {
    let host = host_of(|| {
        Box!({
            BasicText!("One", modifier = Modifier.z_index(1.0).required_size(50.dp()));
            BasicText!("Two", modifier = Modifier.required_size(50.dp()));
        });
    });
    let mut kept = compose_android::accessibility::KeptSemanticsTree::default();
    assert_eq!(kept.get_with_covered(&host).children.len(), 2);
    assert_eq!(kept.get(&host).children.len(), 1);
    assert_eq!(kept.get_with_covered(&host).children.len(), 1, "the same tree, kept");
    assert_eq!(texts_of(&host), ["One"]);
}

#[test]
fn what_a_scroller_clips_all_of_away_is_left_out() {
    let host = host_of(|| {
        let scroll = remember_scroll_state!();
        Column!(modifier = Modifier.size(100.dp()).vertical_scroll(scroll), {
            BasicText!("shown", modifier = Modifier.size(100.dp()).test_tag("shown"));
            BasicText!("past it", modifier = Modifier.size(100.dp()).test_tag("past it"));
        });
    });
    let root = host.accessibility_tree();
    assert!(by_tag(&root, "shown").is_some());
    assert!(by_tag(&root, "past it").is_none());
}

// ---- A node's shape and its own extras (D1656) ----------------------------------------------------

fn shaped(shape: Shape) -> UiHost {
    host_of(move || {
        let shape = shape.clone();
        Box!(
            modifier = Modifier
                .padding(4.dp())
                .size(10.dp())
                .test_tag("shaped")
                .semantics(move |it| it.set_shape(shape.clone()))
        );
    })
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidComposeViewAccessibilityDelegateCompatTest.kt, testPopulateShapeExtras_requestShapeTypeExtra_rectangle, testPopulateShapeExtras_requestShapeExtra_rectangle, testPopulateShapeExtras_shapeSet_setsAllAvailableShapeExtraKeys
#[test]
fn a_rectangles_extra_is_its_bounds() {
    let host = shaped(compose_ui::RECTANGLE_SHAPE);
    let node = uncovered(&host, "shaped");
    assert_ne!(flags_of(&node) & flag::HAS_SHAPE, 0);
    let Some(ShapeExtra::Rectangle(rect)) = shape_extra(&host, &node) else {
        panic!("a rectangle")
    };
    assert_eq!(rect, Rect::new(0.0, 0.0, 10.0, 10.0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidComposeViewAccessibilityDelegateCompatTest.kt, testPopulateShapeExtras_requestShapeTypeExtra_roundedCornerRectangle, testPopulateShapeExtras_requestShapeExtras_roundedCornerRectangle
#[test]
fn a_rounded_rectangles_extra_says_its_corners_in_order() {
    let host = shaped(RoundedCornerShape((1.dp(), 2.dp(), 3.dp(), 4.dp())));
    let node = uncovered(&host, "shaped");
    let Some(ShapeExtra::Rounded(rect, radii)) = shape_extra(&host, &node) else {
        panic!("rounded")
    };
    assert_eq!(rect, Rect::new(0.0, 0.0, 10.0, 10.0));
    assert_eq!(radii, [1.0, 1.0, 2.0, 2.0, 3.0, 3.0, 4.0, 4.0]);
}

/// A triangle in its box.
struct Triangle;

impl CustomShape for Triangle {
    fn create_outline(
        &self,
        size: Size,
        _direction: LayoutDirection,
        _density: &Density,
    ) -> Outline {
        let path = Path::new();
        path.move_to(size.width / 2.0, 0.0);
        path.line_to(size.width, size.height);
        path.line_to(0.0, size.height);
        path.close();
        Outline::Generic(path.data(), Offset::ZERO)
    }

    fn same(&self, other: &dyn CustomShape) -> bool {
        (other as &dyn std::any::Any).downcast_ref::<Self>().is_some()
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidComposeViewAccessibilityDelegateCompatTest.kt, testPopulateShapeExtras_requestShapeTypeExtra_genericShape, testPopulateShapeExtras_requestShapeExtra_genericShape
#[test]
fn any_other_shapes_extra_is_its_path_where_it_shows() {
    let host = shaped(Shape::custom(Triangle));
    let node = uncovered(&host, "shaped");
    let Some(ShapeExtra::Generic(path, offset, bounds)) = shape_extra(&host, &node) else {
        panic!("a path")
    };
    assert_eq!(offset, Offset::ZERO);
    assert_eq!(bounds, Rect::new(0.0, 0.0, 10.0, 10.0));
    assert_eq!(path.bounds(), Rect::new(0.0, 0.0, 10.0, 10.0));
    let mut out = Writer::default();
    write_shape_extra(&ShapeExtra::Generic(path, offset, bounds), &mut out);
    assert_eq!(out.bytes[0], 2);
}

#[test]
fn a_shape_set_inside_what_is_around_it_is_placed_from_the_node() {
    let host = host_of(|| {
        Box!(
            modifier = Modifier
                .size(50.dp())
                .test_tag("around")
                .semantics_with(true, |_| {})
                .padding(5.dp())
                .semantics(|it| it.set_shape(compose_ui::RECTANGLE_SHAPE))
        );
    });
    let node = uncovered(&host, "around");
    let Some(ShapeExtra::Rectangle(rect)) = shape_extra(&host, &node) else {
        panic!("a rectangle")
    };
    assert_eq!(rect, Rect::new(5.0, 5.0, 45.0, 45.0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidComposeViewAccessibilityDelegateCompatTest.kt, testPopulateShapeExtras_shapeUnset_doesNotSetShapeExtraKeys
#[test]
fn no_shape_is_no_extra() {
    let host = host_of(|| {
        Box!(modifier = Modifier.size(10.dp()).test_tag("plain").semantics(|it| it.set_text("a")));
    });
    let node = uncovered(&host, "plain");
    assert_eq!(flags_of(&node) & flag::HAS_SHAPE, 0);
    assert!(shape_extra(&host, &node).is_none());
}

static COUNT_EXTRA: SemanticsPropertyKey<i32> =
    SemanticsPropertyKey::with_accessibility_extra("Count", "count.key");
static NAME_EXTRA: SemanticsPropertyKey<SharedString> =
    SemanticsPropertyKey::with_accessibility_extra("Name", "name.key");
static FLAG_EXTRA: SemanticsPropertyKey<bool> =
    SemanticsPropertyKey::with_accessibility_extra("Flag", "flag.key");

#[derive(Clone, PartialEq, Debug)]
struct NoBundleHoldsThis;

impl SemanticsValue for NoBundleHoldsThis {}

static INVALID_EXTRA: SemanticsPropertyKey<NoBundleHoldsThis> =
    SemanticsPropertyKey::with_accessibility_extra("Invalid", "invalid.key");

fn with_extras(modifier: Modifier) -> UiHost {
    host_of(move || {
        Box!(modifier = modifier.clone().size(10.dp()).test_tag("node"));
    })
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, getCustomExtrasFromExtraData
#[test]
fn a_keys_extra_goes_by_its_own_key_with_its_value() {
    let host = with_extras(Modifier.semantics(|it| {
        it.set(&COUNT_EXTRA, 123);
        it.set(&NAME_EXTRA, "test".into());
        it.set(&FLAG_EXTRA, true);
    }));
    let node = uncovered(&host, "node");
    let extras = extras_of(&node);
    assert!(extras.contains(&("count.key", ExtraValue::Int(123))));
    assert!(extras.contains(&("name.key", ExtraValue::Text("test"))));
    assert!(extras.contains(&("flag.key", ExtraValue::Bool(true))));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, getCustomExtrasFromExtraData_sameExtraSetMultipleTimes_sameModifier_lastOneWins
#[test]
fn set_twice_in_one_modifier_the_last_wins() {
    let host = with_extras(Modifier.semantics(|it| {
        it.set(&COUNT_EXTRA, 1);
        it.set(&COUNT_EXTRA, 2);
    }));
    assert_eq!(extras_of(&uncovered(&host, "node")), [("count.key", ExtraValue::Int(2))]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, getCustomExtrasFromExtraData_sameExtraSetMultipleTimes_twoModifiers_outerOneWins
#[test]
fn set_by_two_modifiers_the_outer_wins() {
    let host = with_extras(
        Modifier.semantics(|it| it.set(&COUNT_EXTRA, 1)).semantics(|it| it.set(&COUNT_EXTRA, 2)),
    );
    assert_eq!(extras_of(&uncovered(&host, "node")), [("count.key", ExtraValue::Int(1))]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, getCustomExtrasFromExtraData_multipleSemanticsModifiers_mergesExtras
#[test]
fn extras_of_several_modifiers_are_all_there() {
    let host = with_extras(
        Modifier
            .semantics(|it| it.set(&COUNT_EXTRA, 123))
            .semantics(|it| it.set(&NAME_EXTRA, "test".into())),
    );
    assert_eq!(extras_of(&uncovered(&host, "node")).len(), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, getInvalidExtraFromExtraData_throws
#[test]
fn what_no_bundle_holds_is_refused_when_asked_for() {
    let host = with_extras(Modifier.semantics(|it| it.set(&INVALID_EXTRA, NoBundleHoldsThis)));
    assert_eq!(extras_of(&uncovered(&host, "node")), [("invalid.key", ExtraValue::Unsendable)]);
    let kotlin = include_str!(
        "../../../android/compose-rs/src/main/java/rs/compose/android/Accessibility.kt"
    );
    assert!(
        kotlin
            .contains("\"Accessibility extra values must be either Serializable or Parcelable.\"")
    );
    assert!(kotlin.contains("keys.addAll(node.extras.keys)"));
}

#[test]
fn a_records_extras_are_read_past_for_the_rest_of_it() {
    let host = with_extras(Modifier.semantics(|it| {
        it.set(&COUNT_EXTRA, 7);
        it.set(&NAME_EXTRA, "seven".into());
    }));
    let node = uncovered(&host, "node");
    let mut out = Writer::default();
    let root = SemanticsSnapshot { children: vec![node], ..host.accessibility_tree() };
    write_tree(&root, &mut KeptCustomActions::default(), &mut out);
    // Once through, its bytes all accounted for.
    assert_eq!(records(&out.bytes).len(), 1);
}
