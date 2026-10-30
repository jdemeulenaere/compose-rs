//! What a service reads of Material's real components (D1289-D1296): where a touch lands on a
//! drawer or a sheet, the words of a switch or a progress indicator, which selected things can be
//! clicked.

#![allow(non_snake_case)]

// The call macros name the runtime `compose`.
extern crate compose_runtime as compose;

use compose_android::accessibility::{
    KeptCustomActions, KeptForChanges, KeptSemanticsTree, action, actions_of, class_and_role, flag,
    node_at, spoken, state_description_of,
};
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_material3::*;
use compose_runtime::mutable_state_of;
use compose_ui::platform::{SemanticsSnapshot, UiHost};
use compose_ui::*;

fn host_of(content: impl Fn() + 'static) -> UiHost {
    let mut host = UiHost::headless(400, 800);
    let content = compose_runtime::callback(content);
    host.set_content(move || {
        let content = content.clone();
        Column!({ content() });
    });
    host.settle(100);
    host
}

fn by_tag<'a>(node: &'a SemanticsSnapshot, tag: &str) -> Option<&'a SemanticsSnapshot> {
    if node.config.get(&SemanticsProperties::TEST_TAG).is_some_and(|it| it.as_str() == tag) {
        return Some(node);
    }
    node.children.iter().find_map(|child| by_tag(child, tag))
}

fn by_id(node: &SemanticsSnapshot, id: i32) -> Option<&SemanticsSnapshot> {
    if node.semantics_id == id {
        return Some(node);
    }
    node.children.iter().find_map(|child| by_id(child, id))
}

fn tagged(host: &UiHost, tag: &str) -> SemanticsSnapshot {
    by_tag(&host.accessibility_tree(), tag).unwrap_or_else(|| panic!("no {tag}")).clone()
}

fn center(node: &SemanticsSnapshot) -> (f32, f32) {
    let bounds = node.bounds;
    ((bounds.left + bounds.right) / 2.0, (bounds.top + bounds.bottom) / 2.0)
}

/// What a touch at `(x, y)` finds, by what it says.
fn found_at(host: &UiHost, x: f32, y: f32) -> SemanticsSnapshot {
    let root = host.accessibility_tree();
    let id = node_at(host, x, y).expect("something's there");
    by_id(&root, id).expect("in the tree").clone()
}

fn texts(node: &SemanticsSnapshot) -> Vec<String> {
    let all = node.config.get(&SemanticsProperties::TEXT);
    all.into_iter().flatten().map(|text| text.as_str().to_string()).collect()
}

fn flags_of(node: &SemanticsSnapshot) -> u32 {
    let mut custom = KeptCustomActions::default();
    let root = SemanticsSnapshot { children: vec![node.clone()], ..node.clone() };
    let mut out = compose_android::serialize::Writer::default();
    compose_android::accessibility::write_tree(&root, &mut custom, &mut out);
    u32::from_le_bytes(out.bytes[24..28].try_into().unwrap())
}

fn offered(node: &SemanticsSnapshot) -> Vec<u8> {
    let mut custom = KeptCustomActions::default();
    custom.assign(node);
    actions_of(node, &custom).iter().map(|(code, _)| *code).collect()
}

// ---- Touches on what's over what ----------------------------------------------------------------

#[test]
fn a_touch_on_an_open_drawer_finds_what_is_in_it_and_past_it_the_scrim() {
    let state = Box::leak(Box::new(DrawerState::new(DrawerValue::Open, |_| true)));
    let given = **state;
    let host = host_of(move || {
        MaterialTheme!({
            ModalNavigationDrawer!(
                {
                    ModalDrawerSheet!({
                        NavigationDrawerItem!(
                            { Text!("In drawer") },
                            false,
                            || {},
                            modifier = Modifier.test_tag("item")
                        );
                    });
                },
                drawer_state = given,
                {
                    Box!(modifier = Modifier.fill_max_size(), {
                        Text!("Behind");
                    });
                }
            );
        });
    });
    let (x, y) = center(&tagged(&host, "item"));
    let found = found_at(&host, x, y);
    assert_eq!(texts(&found), ["In drawer"], "not the scrim the sheet is over");

    let past_it = found_at(&host, 390.0, 790.0);
    let description = past_it.config.get(&SemanticsProperties::CONTENT_DESCRIPTION);
    let said: Vec<_> = description.into_iter().flatten().map(|it| it.to_string()).collect();
    assert_eq!(said, ["Close navigation menu"]);
    assert!(
        offered(&past_it).contains(&action::DISMISS) || offered(&past_it).contains(&action::CLICK)
    );
}

#[test]
fn a_touch_on_a_modal_sheet_finds_what_is_in_it_and_past_it_the_scrim() {
    let mut ui = compose_test::compose_test(|| {
        MaterialTheme!({
            ModalBottomSheet!(|| {}, {
                Column!(modifier = Modifier.height(200.dp()), {
                    Button!(|| {}, modifier = Modifier.test_tag("in sheet"), {
                        Text!("In sheet");
                    });
                });
            });
        });
    });
    // In place, its animation over.
    for _ in 0..4 {
        ui.advance_time_by(std::time::Duration::from_secs(1));
        ui.wait_for_idle();
    }
    let host = ui.host();
    let (x, y) = center(&tagged(host, "in sheet"));
    assert_eq!(texts(&found_at(host, x, y)), ["In sheet"]);
    let past_it = found_at(host, 200.0, 10.0);
    let description = past_it.config.get(&SemanticsProperties::CONTENT_DESCRIPTION);
    let said: Vec<_> = description.into_iter().flatten().map(|it| it.to_string()).collect();
    assert_eq!(said, ["Close sheet"]);
}

// ---- The words a service says -------------------------------------------------------------------

#[test]
fn a_switch_says_on_and_off_and_what_it_is() {
    let on = Box::leak(Box::new(mutable_state_of(false))).handle();
    let mut host = host_of(move || {
        MaterialTheme!({
            Switch!(on.get(), Some(move |it| on.set(it)), modifier = Modifier.test_tag("switch"));
        });
    });
    let switch = tagged(&host, "switch");
    assert_eq!(state_description_of(&switch), (spoken::OFF, 0));
    assert_eq!(class_and_role(&switch), ("android.view.View", spoken::SWITCH));
    assert_ne!(flags_of(&switch) & flag::CHECKABLE, 0);
    assert_eq!(flags_of(&switch) & flag::CHECKED, 0);

    on.set(true);
    host.settle(100);
    let switch = tagged(&host, "switch");
    assert_eq!(state_description_of(&switch), (spoken::ON, 0));
    assert_ne!(flags_of(&switch) & flag::CHECKED, 0);
}

#[test]
fn a_tri_state_checkbox_says_partly_only_when_it_is() {
    let host = host_of(|| {
        MaterialTheme!({
            TriStateCheckbox!(
                ToggleableState::Indeterminate,
                Some(|| {}),
                modifier = Modifier.test_tag("partly")
            );
            Checkbox!(true, Some(|_| {}), modifier = Modifier.test_tag("checked"));
        });
    });
    assert_eq!(state_description_of(&tagged(&host, "partly")), (spoken::PARTIALLY_CHECKED, 0));
    // A checkbox's "checked" is the platform's own to say.
    assert_eq!(state_description_of(&tagged(&host, "checked")), (spoken::NONE, 0));
}

#[test]
fn a_progress_indicator_is_a_progress_bar_saying_its_percent_or_that_it_is_in_progress() {
    let host = host_of(|| {
        MaterialTheme!({
            // Apart: each one's semantics go past its line.
            Column!(vertical_arrangement = Arrangement::spaced_by(40.dp()), {
                LinearProgressIndicator!(|| 0.25, modifier = Modifier.test_tag("at a quarter"));
                LinearProgressIndicator!(|| 0.001, modifier = Modifier.test_tag("just started"));
                IndeterminateLinearProgressIndicator!(
                    modifier = Modifier.test_tag("indeterminate")
                );
            });
        });
    });
    let quarter = tagged(&host, "at a quarter");
    assert_eq!(class_and_role(&quarter).0, "android.widget.ProgressBar");
    assert_eq!(state_description_of(&quarter), (spoken::PERCENT, 25));
    assert_ne!(flags_of(&quarter) & flag::RANGE, 0);
    assert_eq!(state_description_of(&tagged(&host, "just started")), (spoken::PERCENT, 1));

    let indeterminate = tagged(&host, "indeterminate");
    assert_eq!(class_and_role(&indeterminate).0, "android.widget.ProgressBar");
    assert_eq!(state_description_of(&indeterminate), (spoken::IN_PROGRESS, 0));
    assert_eq!(flags_of(&indeterminate) & flag::RANGE, 0, "no range to say");
}

#[test]
fn a_slider_is_a_seek_bar() {
    let host = host_of(|| {
        MaterialTheme!({
            let state = remember_slider_state!(0.5);
            Slider!(state, |_| {}, modifier = Modifier.test_tag("slider"));
        });
    });
    let slider = tagged(&host, "slider");
    assert_eq!(class_and_role(&slider).0, "android.widget.SeekBar");
    assert_eq!(state_description_of(&slider), (spoken::PERCENT, 50));
}

#[test]
fn a_selected_row_with_no_role_is_checked_saying_selected_and_a_tab_is_selected() {
    let host = host_of(|| {
        MaterialTheme!({
            Column!({
                let row = Modifier.selectable(true, || {}).test_tag("row");
                Row!(modifier = row, {
                    Text!("Row");
                });
                let other = Modifier.selectable(false, || {}).test_tag("other row");
                Row!(modifier = other, {
                    Text!("Other");
                });
                PrimaryTabRow!(0, {
                    Tab!(
                        true,
                        || {},
                        modifier = Modifier.test_tag("tab"),
                        text = Some(|| Text!("One"))
                    );
                    Tab!(
                        false,
                        || {},
                        modifier = Modifier.test_tag("other"),
                        text = Some(|| Text!("Two"))
                    );
                });
            });
        });
    });
    let row = tagged(&host, "row");
    assert_ne!(flags_of(&row) & flag::CHECKABLE, 0);
    assert_ne!(flags_of(&row) & flag::CHECKED, 0);
    assert_eq!(flags_of(&row) & flag::SELECTED, 0);
    assert_eq!(state_description_of(&row), (spoken::SELECTED, 0));
    assert_eq!(state_description_of(&tagged(&host, "other row")), (spoken::NOT_SELECTED, 0));

    let tab = tagged(&host, "tab");
    assert_ne!(flags_of(&tab) & flag::SELECTED, 0);
    assert_eq!(flags_of(&tab) & flag::CHECKABLE, 0);
    assert_eq!(state_description_of(&tab), (spoken::NONE, 0));
    assert_eq!(class_and_role(&tab).1, spoken::TAB);
}

#[test]
fn an_empty_field_says_so_and_one_with_something_in_it_does_not() {
    let host = host_of(|| {
        MaterialTheme!({
            Column!({
                BasicTextField!(
                    String::new(),
                    |_| {},
                    modifier = Modifier.size(50.dp()).test_tag("empty")
                );
                BasicTextField!(
                    String::from("full"),
                    |_| {},
                    modifier = Modifier.size(50.dp()).test_tag("full")
                );
            });
        });
    });
    assert_eq!(state_description_of(&tagged(&host, "empty")), (spoken::EMPTY, 0));
    assert_eq!(state_description_of(&tagged(&host, "full")), (spoken::NONE, 0));
}

// ---- What's selected can't be selected again ----------------------------------------------------

#[test]
fn a_selected_tab_radio_button_or_navigation_item_offers_no_click_and_another_does() {
    let host = host_of(|| {
        MaterialTheme!({
            Column!({
                PrimaryTabRow!(0, {
                    Tab!(
                        true,
                        || {},
                        modifier = Modifier.test_tag("tab"),
                        text = Some(|| Text!("One"))
                    );
                    Tab!(
                        false,
                        || {},
                        modifier = Modifier.test_tag("other tab"),
                        text = Some(|| Text!("Two"))
                    );
                });
                RadioButton!(true, Some(|| {}), modifier = Modifier.test_tag("radio"));
                RadioButton!(false, Some(|| {}), modifier = Modifier.test_tag("other radio"));
                NavigationBar!(|row: RowScope| {
                    NavigationBarItem!(
                        row,
                        true,
                        || {},
                        { Text!("A") },
                        modifier = Modifier.test_tag("item"),
                    );
                    NavigationBarItem!(
                        row,
                        false,
                        || {},
                        { Text!("B") },
                        modifier = Modifier.test_tag("other item"),
                    );
                });
            });
        });
    });
    for tag in ["tab", "radio", "item"] {
        let node = tagged(&host, tag);
        assert_eq!(flags_of(&node) & flag::CLICKABLE, 0, "{tag}");
        assert!(!offered(&node).contains(&action::CLICK), "{tag}");
    }
    for tag in ["other tab", "other radio", "other item"] {
        let node = tagged(&host, tag);
        assert_ne!(flags_of(&node) & flag::CLICKABLE, 0, "{tag}");
        assert!(offered(&node).contains(&action::CLICK), "{tag}");
    }
}

// ---- A layer fading ------------------------------------------------------------------------------

#[test]
fn a_layer_coming_to_no_alpha_or_leaving_it_changes_what_a_service_reads_at_once() {
    let alpha = Box::leak(Box::new(mutable_state_of(0.0_f32))).handle();
    let mut host = host_of(move || {
        Box!(modifier = Modifier.graphics_layer(move |layer| layer.alpha = alpha.get()), {
            Box!(modifier = Modifier.size(10.dp()).test_tag("under"), {});
        });
    });
    let mut kept = KeptSemanticsTree::default();
    let hidden = |kept: &mut KeptSemanticsTree, host: &UiHost| {
        by_tag(kept.get(host), "under").expect("in the tree").hidden
    };
    assert!(hidden(&mut kept, &host));

    // Nothing else changes: not a recomposition nor a layout, only the layer's alpha.
    alpha.set(1.0);
    let stats = host.frame();
    assert!(kept.after_frame(&stats), "the tree is read again");
    assert!(!hidden(&mut kept, &host));

    alpha.set(0.5);
    let stats = host.frame();
    assert!(!stats.semantics_changed, "still some alpha: nothing a service reads");

    alpha.set(0.0);
    let stats = host.frame();
    assert!(kept.after_frame(&stats));
    assert!(hidden(&mut kept, &host));
}

#[test]
fn a_layer_inside_a_node_coming_to_no_alpha_tells_it_too() {
    let alpha = Box::leak(Box::new(mutable_state_of(1.0_f32))).handle();
    let mut host = host_of(move || {
        // After a drawing modifier: a layer of its own in the node's drawing, not the node's.
        let layered = Modifier
            .size(10.dp())
            .background(Color::RED)
            .graphics_layer(move |layer| layer.alpha = alpha.get())
            .background(Color::BLUE);
        Box!(modifier = layered, {
            Box!(modifier = Modifier.size(5.dp()).test_tag("under"), {});
        });
    });
    alpha.set(0.0);
    assert!(host.frame().semantics_changed, "no alpha");
    assert!(by_tag(&host.accessibility_tree(), "under").expect("in the tree").hidden);
    alpha.set(1.0);
    assert!(host.frame().semantics_changed, "some again");
    assert!(!by_tag(&host.accessibility_tree(), "under").expect("in the tree").hidden);
}

#[test]
fn what_fades_in_comes_to_a_service_as_it_does() {
    use compose_animation::*;
    let shown = Box::leak(Box::new(mutable_state_of(false))).handle();
    let mut host = host_of(move || {
        AnimatedVisibility!(shown.get(), enter = fade_in(), exit = fade_out(), move |_| {
            let says = Modifier
                .size(10.dp())
                .semantics(|config| config.set_content_description("says"))
                .test_tag("fading");
            Box!(modifier = says, {});
        });
    });
    assert!(by_tag(&host.accessibility_tree(), "fading").is_none());
    shown.set(true);
    let mut kept = KeptSemanticsTree::default();
    let mut read_visible = false;
    for _ in 0..200 {
        let stats = host.frame();
        kept.after_frame(&stats);
        if by_tag(kept.get(&host), "fading").is_some_and(|node| !node.hidden) {
            read_visible = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(read_visible, "a service sees it once it has any alpha");
}

// ---- What's told, of Material's -----------------------------------------------------------------

#[test]
fn a_snackbars_words_appearing_are_told_from_its_live_region() {
    let shown = Box::leak(Box::new(mutable_state_of(false))).handle();
    let mut host = host_of(move || {
        MaterialTheme!({
            let live = Modifier
                .semantics(|config| config.set_live_region(LiveRegionMode::Polite))
                .size(50.dp())
                .test_tag("live");
            Box!(modifier = live, {
                if shown.get() {
                    Text!("Saved");
                }
            });
        });
    });
    let mut kept = KeptForChanges::default();
    kept.forget();
    kept.changes(&host, &host.accessibility_tree());
    let live = tagged(&host, "live").semantics_id;
    shown.set(true);
    host.settle(100);
    let changes = kept.changes(&host, &host.accessibility_tree());
    assert!(
        changes.iter().any(|change| {
            change.id == live && change.kind == compose_android::accessibility::ChangeKind::Subtree
        }),
        "{changes:?}"
    );
}
