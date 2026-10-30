//! What `Accessibility.kt` is told by Rust (D1289-D1291): the class and actions of a node, the
//! order it's read in, live regions, what's behind a dialog, and what changed.

#![allow(non_snake_case)]

// The call macros name the runtime `compose`.
extern crate compose_runtime as compose;

use std::cell::Cell;
use std::rc::Rc;

use compose_android::accessibility::{
    Change, ChangeKind, KeptForChanges, action, actions_of, class_and_role, flag, write_tree,
};
use compose_android::serialize::Writer;
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::{MutableState, mutable_state_of};
use compose_ui::platform::{SemanticsSnapshot, UiHost};
use compose_ui::*;

fn host_of(content: impl Fn() + 'static) -> UiHost {
    let mut host = UiHost::headless(400, 800);
    host.set_content(content);
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
    by_tag(&host.accessibility_tree(), tag).expect("tagged").clone()
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
        class_and_role(&node)
    };
    assert_eq!(of("button"), ("android.widget.Button", ""));
    assert_eq!(of("checkbox"), ("android.widget.CheckBox", ""));
    assert_eq!(of("radio"), ("android.widget.RadioButton", ""));
    assert_eq!(of("image"), ("android.widget.ImageView", ""));
    assert_eq!(of("dropdown"), ("android.widget.Spinner", ""));
    assert_eq!(of("picker"), ("android.widget.NumberPicker", ""));
    assert_eq!(of("tab"), ("android.view.View", "Tab"));
    assert_eq!(of("switch"), ("android.view.View", "Switch"));
    assert_eq!(of("over a child"), ("android.view.View", ""));
    assert_eq!(of("text"), ("android.widget.TextView", ""));
    assert_eq!(of("plain"), ("android.view.View", ""));
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
    let offered = actions_of(&enabled);
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
    assert!(actions_of(&tagged(&host, "disabled")).is_empty());

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
        actions_of(&tagged(&host, tag)).iter().map(|(code, _)| *code).collect()
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
    let root = host.accessibility_tree();
    let mut out = Writer::default();
    write_tree(&root, &mut out);
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
        skip_string(&mut at);
        skip_string(&mut at);
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

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/AndroidComposeViewAccessibilityTraversalTest.kt, testMergedDescendants_respectsTraversalIndex
#[test]
fn a_nodes_children_are_written_by_their_traversal_index() {
    let host = host_of(|| {
        Row!({
            let child = |tag: &'static str, index: f32| {
                Modifier.size(20.dp()).semantics(move |config| {
                    says(config);
                    config.set_traversal_index(index);
                    config.set_test_tag(tag);
                })
            };
            Box!(modifier = child("second", 2.0), {});
            Box!(modifier = child("first", 1.0), {});
        });
    });
    let root = host.accessibility_tree();
    let mut out = Writer::default();
    write_tree(&root, &mut out);
    let ids: Vec<i32> = records(&out.bytes).iter().map(|record| record.0).collect();
    assert_eq!(ids, [tagged(&host, "first").semantics_id, tagged(&host, "second").semantics_id],);
}

#[test]
fn a_node_saying_something_over_another_is_not_read_for_itself() {
    let host = host_of(|| {
        Box!(modifier = at(0.0, 0.0, 50.0, "over a child"), {
            Box!(modifier = Modifier.size(10.dp()).semantics(says).test_tag("child"), {});
        });
    });
    assert_eq!(order(&host), ["child"]);
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

    let root = host.accessibility_tree();
    let mut out = Writer::default();
    write_tree(&root, &mut out);
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

fn changes_after(
    host: &mut UiHost,
    kept: &mut KeptForChanges,
    change: impl FnOnce(),
) -> Vec<(i32, ChangeKind, String)> {
    change();
    frames(host);
    let changes: Vec<Change> = kept.changes(&host.accessibility_tree());
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
    let mut kept = KeptForChanges::default();
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
        let mut kept = KeptForChanges::default();
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
    let mut kept = KeptForChanges::default();
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
    assert_eq!(kinds(changes_after(&mut host, &mut kept, || step.set(5))), [ChangeKind::Text]);
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
    let mut kept = KeptForChanges::default();
    changes_after(&mut host, &mut kept, || {});

    let appeared = changes_after(&mut host, &mut kept, || title.set(Some("Menu")));
    let id = tagged(&host, "pane").semantics_id;
    assert_eq!(
        appeared,
        [
            (id, ChangeKind::PaneAppeared, "Menu".to_string()),
            (-1, ChangeKind::Subtree, String::new())
        ]
    );
    assert_eq!(
        changes_after(&mut host, &mut kept, || title.set(Some("Sheet"))),
        [(id, ChangeKind::PaneTitle, "Sheet".to_string())]
    );
    assert_eq!(
        changes_after(&mut host, &mut kept, || title.set(None)),
        [
            (id, ChangeKind::PaneDisappeared, "Sheet".to_string()),
            (-1, ChangeKind::Subtree, String::new())
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
    let mut kept = KeptForChanges::default();
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
    let mut kept = KeptForChanges::default();
    changes_after(&mut host, &mut kept, || {});
    assert_eq!(
        changes_after(&mut host, &mut kept, || moved.set(5.0)),
        [(-1, ChangeKind::Subtree, String::new())]
    );
    kept.forget();
    assert!(changes_after(&mut host, &mut kept, || moved.set(10.0)).is_empty());
}
