//! Compose's `ExposedDropdownMenuTest.kt`, adapted: no soft keyboard to type on and no other
//! windows, and a key goes to what has the focus, so to the popup when it has it, where Compose's
//! test injects into the anchor's window whatever has the focus.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::ScrollModifiers as _;
use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Alignment, Density, IntOffset, IntSize, Key, LayoutDirection, Modifier, Offset,
    PopupPositionProvider, Rect, Role, SemanticsActions, SemanticsProperties, UnitExt,
};

const TF: &str = "TextFieldTag";
const TRAILING_ICON: &str = "TrailingIconTag";
const EDM: &str = "ExposedDropdownMenuTag";
const ITEM: &str = "MenuItemTag";
const OPTION: &str = "Option 1";

#[composable]
fn ForTest(
    expanded: MutableState<bool>,
    #[default(false)] editable: bool,
    #[default(true)] enabled: bool,
    #[default(compose_ui::Modifier)] text_field_modifier: Modifier,
    #[default(compose_ui::Modifier)] menu_modifier: Modifier,
) {
    let text = remember(|| mutable_state_of(String::new()));
    MaterialTheme!({
        Box!(modifier = Modifier.fill_max_size(), |box_scope: BoxScope| {
            ExposedDropdownMenuBox!(
                expanded.get(),
                move |value: bool| expanded.set(value),
                modifier = Modifier.align(box_scope, Alignment::CENTER),
                |scope: ExposedDropdownMenuBoxScope| {
                    let anchor_type = if editable {
                        ExposedDropdownMenuAnchorType::PrimaryEditable
                    } else {
                        ExposedDropdownMenuAnchorType::PrimaryNotEditable
                    };
                    TextField!(
                        text.get(),
                        move |value: String| text.set(value),
                        modifier = text_field_modifier
                            .clone()
                            .menu_anchor_with(&scope, anchor_type, enabled)
                            .test_tag(TF),
                        read_only = !editable,
                        single_line = true,
                        label = Some(|| Text!("Label")),
                        trailing_icon = Some(move || {
                            Box!(modifier = Modifier.test_tag(TRAILING_ICON), {
                                ExposedDropdownMenuDefaults::trailing_icon(
                                    expanded.get(),
                                    Modifier,
                                );
                            });
                        }),
                        colors = ExposedDropdownMenuDefaults::text_field_colors()
                    );
                    ExposedDropdownMenu!(
                        scope.clone(),
                        expanded.get(),
                        move || expanded.set(false),
                        modifier = menu_modifier.clone().test_tag(EDM),
                        |_: ColumnScope| {
                            DropdownMenuItem!(
                                { Text!(OPTION) },
                                move || {
                                    text.set(OPTION.to_string());
                                    expanded.set(false);
                                },
                                modifier = Modifier.test_tag(ITEM)
                            );
                        }
                    );
                }
            );
        });
    });
}

/// Scrolls `state` to `value` when `go` turns true: its own scope, so what's around it isn't
/// composed again for that.
#[composable]
fn ScrollWhen(go: MutableState<bool>, state: compose_foundation::ScrollState, value: i32) {
    let scrolling = go.get();
    LaunchedEffect(scrolling, async move {
        if scrolling {
            let _ = state.scroll_to(value.min(state.max_value())).await;
        }
    });
}

fn rule(initial: bool, editable: bool) -> (ComposeTestRule, MutableState<bool>) {
    let state = mutable_state_of(initial);
    let expanded = state.handle();
    let ui = ComposeTestRule::sized(IntSize::new(600, 800), move || {
        let _ = &state;
        ForTest!(expanded, editable = editable);
    });
    (ui, expanded)
}

fn click(ui: &mut ComposeTestRule, tag: &str) {
    ui.on_node_with_tag_unmerged(tag).perform_touch_input(|touch| {
        let center = touch.center();
        touch.click_at(center);
    });
}

// Compose: edm_expandsOnClick_andCollapsesOnClickOutside
#[test]
fn expands_on_click_and_collapses_on_click_outside() {
    let (mut ui, _) = rule(false, false);
    ui.on_node_with_tag(TF).assert_is_displayed();
    ui.on_node_with_tag(EDM).assert_does_not_exist();
    click(&mut ui, TF);
    ui.on_node_with_tag(ITEM).assert_is_displayed();
    // Outside the menu and the field, in the window's corner.
    ui.on_root().perform_touch_input(|touch| touch.click_at(Offset::new(1.0, 1.0)));
    ui.on_node_with_tag(ITEM).assert_does_not_exist();
}

// Compose: edm_collapsesOnTextFieldClick
#[test]
fn collapses_on_text_field_click() {
    for editable in [false, true] {
        let (mut ui, expanded) = rule(true, editable);
        ui.on_node_with_tag(TF).assert_is_displayed();
        ui.on_node_with_tag(EDM).assert_is_displayed();
        ui.on_node_with_tag(ITEM).assert_is_displayed();
        click(&mut ui, TF);
        ui.on_node_with_tag(ITEM).assert_does_not_exist();
        assert!(!expanded.get(), "editable {editable}");
    }
}

/// With a frame between the press and the release: the press dismissed the menu of what's typed
/// in (the popup lets it through), so its release mustn't open it again. Compose's
/// `pointerInput` starts again for that.
#[test]
fn a_press_that_dismissed_the_menu_does_not_open_it_on_release() {
    let (mut ui, expanded) = rule(true, true);
    ui.on_node_with_tag(TF).perform_touch_input(|touch| {
        let center = touch.center();
        touch.down(center);
    });
    assert!(!expanded.get());
    ui.on_node_with_tag(TF).perform_touch_input(|touch| touch.up());
    assert!(!expanded.get());
    ui.on_node_with_tag(ITEM).assert_does_not_exist();
}

// Compose: edm_notEditable_collapsesOnBackDispatch, edm_editable_collapsesOnBackDispatch,
// edm_notEditable_collapsesOnBackPress, edm_editable_collapsesOnBackPress
#[test]
fn collapses_on_back() {
    for editable in [false, true] {
        let (mut ui, expanded) = rule(true, editable);
        ui.on_node_with_tag(ITEM).assert_is_displayed();
        ui.host().on_back_pressed();
        ui.on_node_with_tag(TF).assert_is_displayed();
        ui.on_node_with_tag(ITEM).assert_does_not_exist();
        assert!(!expanded.get(), "editable {editable}");
    }
}

// Compose: edm_notEditable_collapsesOnEscapePress
#[test]
fn not_editable_collapses_on_escape() {
    let (mut ui, _) = rule(true, false);
    ui.on_node_with_tag(ITEM).assert_is_displayed();
    ui.press_key(Key::Escape);
    ui.on_node_with_tag(TF).assert_is_displayed();
    ui.on_node_with_tag(ITEM).assert_does_not_exist();
}

// Compose: edm_editable_collapsesOnEscapePress
#[test]
fn editable_collapses_on_escape_after_tab_gave_the_menu_the_focus() {
    let (mut ui, _) = rule(true, true);
    ui.on_node_with_tag(ITEM).assert_is_displayed();
    ui.on_node_with_tag(TF).request_focus();
    ui.on_node_with_tag(TF).assert_is_focused();
    ui.press_key(Key::Tab);
    ui.press_key(Key::Escape);
    ui.on_node_with_tag(TF).assert_is_displayed();
    ui.on_node_with_tag(ITEM).assert_does_not_exist();
}

// Compose: edm_notEditable_doesNotExpand_whenDisabled, edm_editable_doesNotExpand_whenDisabled
#[test]
fn does_not_expand_when_disabled() {
    for editable in [false, true] {
        let state = mutable_state_of(false);
        let expanded = state.handle();
        let mut ui = ComposeTestRule::sized(IntSize::new(600, 800), move || {
            let _ = &state;
            ForTest!(expanded, editable = editable, enabled = false);
        });
        ui.on_node_with_tag(TF).assert_is_displayed();
        ui.on_node_with_tag(EDM).assert_does_not_exist();
        click(&mut ui, TF);
        ui.on_node_with_tag(EDM).assert_does_not_exist();
        assert_eq!(
            ui.on_node_with_tag(TF).fetch_semantics_node().config.get(&SemanticsProperties::ROLE),
            None,
            "says nothing to accessibility"
        );
    }
}

// Compose: edm_editable_focusesTextField_whenClickedViaSemantics
#[test]
fn editable_focuses_the_field_when_clicked_via_semantics() {
    // An autocomplete: empty text doesn't open it.
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 800), || {
        MaterialTheme!({
            ExposedDropdownMenuBox!(false, |_: bool| {}, |scope: ExposedDropdownMenuBoxScope| {
                TextField!(
                    String::new(),
                    |_: String| {},
                    modifier = Modifier
                        .menu_anchor(&scope, ExposedDropdownMenuAnchorType::PrimaryEditable)
                        .test_tag(TF)
                );
            });
        });
    });
    ui.on_node_with_tag(TF).perform_semantics_action(&SemanticsActions::ON_CLICK);
    ui.on_node_with_tag(TF).assert_is_focused();
}

// Compose: edm_doesNotCollapse_whenTypingOnSoftKeyboard
#[test]
fn does_not_collapse_when_typing() {
    let (mut ui, _) = rule(false, true);
    click(&mut ui, TF);
    ui.on_node_with_tag(TF).assert_is_displayed();
    ui.on_node_with_tag(TF).assert_is_focused();
    ui.on_node_with_tag(EDM).assert_is_displayed();
    ui.on_node_with_tag(ITEM).assert_is_displayed();
    ui.on_node_with_tag(TF).perform_text_input("zzz");
    ui.on_node_with_tag(TF).assert_text_contains("zzz", false, false);
    ui.on_node_with_tag(ITEM).assert_is_displayed();
}

// Compose: edm_expandsAndFocusesTextField_whenTrailingIconClicked
#[test]
fn expands_and_focuses_the_field_when_the_trailing_icon_is_clicked() {
    let (mut ui, _) = rule(false, true);
    ui.on_node_with_tag_unmerged(TRAILING_ICON).assert_is_displayed();
    click(&mut ui, TRAILING_ICON);
    ui.on_node_with_tag(TF).assert_is_focused();
    ui.on_node_with_tag(ITEM).assert_is_displayed();
}

// Compose: edm_doesNotExpand_ifTouchEndsOutsideBounds
#[test]
fn does_not_expand_if_the_touch_ends_outside() {
    let (mut ui, expanded) = rule(false, false);
    ui.on_node_with_tag(TF).perform_touch_input(|touch| {
        let center = touch.center();
        touch.down(center);
        touch.move_by(Offset::new(0.0, center.y + 1.0));
        touch.up();
    });
    assert!(!expanded.get());
    ui.on_node_with_tag(ITEM).assert_does_not_exist();
}

/// And the opposite: a touch that moves within the anchor still opens it.
#[test]
fn expands_if_the_touch_moves_inside() {
    let (mut ui, expanded) = rule(false, false);
    ui.on_node_with_tag(TF).perform_touch_input(|touch| {
        let center = touch.center();
        touch.down(center);
        touch.move_by(Offset::new(1.0, 1.0));
        touch.up();
    });
    assert!(expanded.get());
}

// Compose: edm_doesNotExpand_ifTouchIsPartOfScroll
#[test]
fn does_not_expand_if_the_touch_is_part_of_a_scroll() {
    let state = mutable_state_of(false);
    let expanded = state.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 400), move || {
        let _ = &state;
        MaterialTheme!({
            Column!(
                modifier = Modifier.fill_max_size().vertical_scroll(remember_scroll_state(0)),
                {
                    ExposedDropdownMenuBox!(
                        expanded.get(),
                        move |value: bool| expanded.set(value),
                        |scope: ExposedDropdownMenuBoxScope| {
                            TextField!(
                                String::new(),
                                |_: String| {},
                                modifier = Modifier
                                    .menu_anchor(
                                        &scope,
                                        ExposedDropdownMenuAnchorType::PrimaryEditable
                                    )
                                    .test_tag(TF)
                            );
                            ExposedDropdownMenu!(
                                scope.clone(),
                                expanded.get(),
                                move || expanded.set(false),
                                |_: ColumnScope| {
                                    DropdownMenuItem!(
                                        { Text!(OPTION) },
                                        || {},
                                        modifier = Modifier.test_tag(ITEM)
                                    );
                                }
                            );
                        }
                    );
                    Spacer!(Modifier.height(2000.dp()));
                }
            );
        });
    });
    ui.on_node_with_tag(TF).assert_is_displayed();
    let height = ui.on_node_with_tag(TF).fetch_semantics_node().bounds.height();
    // Within the anchor's bounds as the content follows the finger: only the scroll cancels it.
    ui.on_node_with_tag(TF).perform_touch_input(|touch| {
        let center = touch.center();
        touch.down(center);
        for _ in 0..4 {
            touch.move_by(Offset::new(0.0, -height / 16.0));
        }
        touch.up();
    });
    assert!(!expanded.get());
    ui.on_node_with_tag(ITEM).assert_does_not_exist();
}

/// The tree's Enter and Space go to what has the focus: for a not editable anchor it's the field
/// when the menu's closed.
// Compose: edm_expands_onEnterKey
#[test]
fn expands_on_enter() {
    let (mut ui, expanded) = rule(false, false);
    ui.on_node_with_tag(TF).request_focus();
    ui.press_key(Key::Enter);
    assert!(expanded.get());
}

/// Compose's test injects Enter into the anchor's window; here it goes where the focus is, the
/// open menu, which on a device is where it would go. So it doesn't toggle the menu.
// Compose: edm_collapses_onEnterKey
#[test]
fn an_open_focusable_menu_takes_enter_from_its_anchor() {
    let (mut ui, expanded) = rule(true, false);
    ui.on_node_with_tag(TF).request_focus();
    ui.press_key(Key::Enter);
    assert!(expanded.get());
}

// Compose: edm_editable_expands_onEnterKey
#[test]
fn editable_expands_on_enter() {
    let (mut ui, expanded) = rule(false, true);
    ui.on_node_with_tag(TF).request_focus();
    ui.press_key(Key::Enter);
    assert!(expanded.get());
}

// Compose: edm_editable_collapses_onEnterKey
#[test]
fn editable_collapses_on_enter() {
    let (mut ui, expanded) = rule(true, true);
    ui.on_node_with_tag(TF).request_focus();
    ui.press_key(Key::Enter);
    assert!(!expanded.get());
}

// Compose: edm_editable_doesNotExpand_onSpacebarKey
#[test]
fn editable_does_not_expand_on_space() {
    let (mut ui, expanded) = rule(false, true);
    ui.on_node_with_tag(TF).request_focus();
    ui.press_key(Key::Space);
    assert!(!expanded.get());
}

// Compose: edm_editable_doesNotCollapse_onSpacebarKey
#[test]
fn editable_does_not_collapse_on_space() {
    let (mut ui, expanded) = rule(true, true);
    ui.on_node_with_tag(TF).request_focus();
    ui.press_key(Key::Space);
    assert!(expanded.get());
}

/// A not editable anchor's Space opens it, as `clickable`.
#[test]
fn not_editable_expands_on_space() {
    let (mut ui, expanded) = rule(false, false);
    ui.on_node_with_tag(TF).request_focus();
    ui.press_key(Key::Space);
    assert!(expanded.get());
}

/// Types `text` where the keys go, and says if the field got it: what shows where the focus is,
/// the field keeping its own window's focus when the menu takes the keys.
fn field_takes_typing(ui: &mut ComposeTestRule, text: &str) -> bool {
    ui.host().send_text(text);
    ui.wait_for_idle();
    ui.on_node_with_tag(TF)
        .fetch_semantics_node()
        .config
        .get(&SemanticsProperties::EDITABLE_TEXT)
        .is_some_and(|value| value.contains(text))
}

/// The arrow keys of what's typed in move the keys to the menu; closed, an arrow leaves them.
#[test]
fn editable_menu_takes_the_keys_on_an_arrow() {
    for key in [Key::DirectionDown, Key::DirectionUp, Key::Tab] {
        let (mut ui, _) = rule(true, true);
        ui.on_node_with_tag(TF).request_focus();
        assert!(field_takes_typing(&mut ui, "y"));
        ui.press_key(key);
        assert!(!field_takes_typing(&mut ui, "z"), "{key:?}");
    }
    let (mut ui, _) = rule(false, true);
    ui.on_node_with_tag(TF).request_focus();
    ui.press_key(Key::DirectionDown);
    assert!(field_takes_typing(&mut ui, "z"));
}

// Compose: edm_doesNotRecomposeOnScroll
#[test]
fn does_not_recompose_on_scroll() {
    let count = Rc::new(Cell::new(0));
    let seen = count.clone();
    let scroll = Rc::new(Cell::new(None));
    let scroll_out = scroll.clone();
    let go_state = mutable_state_of(false);
    let go = go_state.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 400), move || {
        let _ = &go_state;
        let (seen, scroll_out) = (seen.clone(), scroll_out.clone());
        MaterialTheme!({
            let seen = seen.clone();
            let state = remember_scroll_state(0);
            scroll_out.set(Some(state));
            ScrollWhen!(go, state, 100);
            Column!(modifier = Modifier.vertical_scroll(state), {
                Spacer!(Modifier.height(300.dp()));
                let seen = seen.clone();
                ExposedDropdownMenuBox!(
                    false,
                    |_: bool| {},
                    |scope: ExposedDropdownMenuBoxScope| {
                        TextField!(
                            String::new(),
                            |_: String| {},
                            modifier = Modifier.menu_anchor(
                                &scope,
                                ExposedDropdownMenuAnchorType::PrimaryNotEditable
                            ),
                            read_only = true,
                            label = Some(|| Text!("Label"))
                        );
                        ExposedDropdownMenu!(scope.clone(), false, || {}, |_: ColumnScope| {});
                        let seen = seen.clone();
                        SideEffect(move || seen.set(seen.get() + 1));
                    }
                );
                Spacer!(Modifier.height(800.dp()));
            });
        });
    });
    assert_eq!(count.get(), 1);
    let state = scroll.get().unwrap();
    go.set(true);
    ui.wait_for_idle();
    assert_eq!(state.value(), 100);
    assert_eq!(count.get(), 1);
}

// Compose: edm_anchorTypeIsUpdated_evenIfTextFieldIsNotClicked
#[test]
fn anchor_type_is_updated_even_if_the_field_is_not_clicked() {
    let state = mutable_state_of(false);
    let expanded = state.handle();
    let seen = Rc::new(Cell::new(None));
    let seen_out = seen.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 800), move || {
        let _ = &state;
        let seen = seen_out.clone();
        MaterialTheme!({
            let seen = seen.clone();
            ExposedDropdownMenuBox!(
                expanded.get(),
                move |value: bool| expanded.set(value),
                |scope: ExposedDropdownMenuBoxScope| {
                    let icon = scope.clone();
                    TextField!(
                        String::new(),
                        |_: String| {},
                        modifier = Modifier
                            .menu_anchor(&scope, ExposedDropdownMenuAnchorType::PrimaryEditable),
                        single_line = true,
                        trailing_icon = Some(move || {
                            ExposedDropdownMenuDefaults::trailing_icon(
                                expanded.get(),
                                Modifier.menu_anchor(
                                    &icon,
                                    ExposedDropdownMenuAnchorType::SecondaryEditable,
                                ),
                            );
                        })
                    );
                    ExposedDropdownMenu!(
                        scope.clone(),
                        expanded.get(),
                        move || expanded.set(false),
                        |_: ColumnScope| {
                            DropdownMenuItem!({ Text!(OPTION) }, || {});
                        }
                    );
                    let (seen, scope) = (seen.clone(), scope.clone());
                    SideEffect(move || seen.set(Some(scope.anchor_type())));
                }
            );
        });
    });
    expanded.set(true);
    ui.wait_for_idle();
    assert_eq!(seen.get(), Some(ExposedDropdownMenuAnchorType::PrimaryEditable));
}

/// Clicked, a secondary anchor's the menu's, so the popup leaves the focus in the text field; and
/// its semantics say what it is and its press isn't the field's.
#[test]
fn a_secondary_anchor_keeps_the_focus_in_the_field_and_says_what_it_does() {
    let state = mutable_state_of(false);
    let expanded = state.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 800), move || {
        let _ = &state;
        MaterialTheme!({
            ExposedDropdownMenuBox!(
                expanded.get(),
                move |value: bool| expanded.set(value),
                |scope: ExposedDropdownMenuBoxScope| {
                    let icon = scope.clone();
                    TextField!(
                        String::new(),
                        |_: String| {},
                        modifier = Modifier
                            .menu_anchor(&scope, ExposedDropdownMenuAnchorType::PrimaryEditable)
                            .test_tag(TF),
                        trailing_icon = Some(move || {
                            ExposedDropdownMenuDefaults::trailing_icon(
                                expanded.get(),
                                Modifier
                                    .menu_anchor(
                                        &icon,
                                        ExposedDropdownMenuAnchorType::SecondaryEditable,
                                    )
                                    .test_tag(TRAILING_ICON),
                            );
                        })
                    );
                    ExposedDropdownMenu!(
                        scope.clone(),
                        expanded.get(),
                        move || expanded.set(false),
                        |_: ColumnScope| {
                            DropdownMenuItem!(
                                { Text!(OPTION) },
                                || {},
                                modifier = Modifier.test_tag(ITEM)
                            );
                        }
                    );
                }
            );
        });
    });
    let config = ui.on_node_with_tag_unmerged(TRAILING_ICON).fetch_semantics_node().config.clone();
    assert_eq!(config.get(&SemanticsProperties::ROLE), Some(&Role::Button));
    assert_eq!(
        config.get(&SemanticsProperties::STATE_DESCRIPTION).map(|text| &**text),
        Some("Collapsed")
    );
    assert_eq!(
        &*config.get(&SemanticsProperties::CONTENT_DESCRIPTION).unwrap()[0],
        "Toggle dropdown menu"
    );
    click(&mut ui, TRAILING_ICON);
    assert!(expanded.get());
    ui.on_node_with_tag(ITEM).assert_is_displayed();
    ui.on_node_with_tag(TF).assert_is_focused();
    let config = ui.on_node_with_tag_unmerged(TRAILING_ICON).fetch_semantics_node().config.clone();
    assert_eq!(
        config.get(&SemanticsProperties::STATE_DESCRIPTION).map(|text| &**text),
        Some("Expanded")
    );
}

// Compose: edm_widthMatchesTextFieldWidth
#[test]
fn width_matches_the_field() {
    let state = mutable_state_of(true);
    let expanded = state.handle();
    let (field, menu) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (field_out, menu_out) = (field.clone(), menu.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 800), move || {
        let _ = &state;
        let (field_out, menu_out) = (field_out.clone(), menu_out.clone());
        ForTest!(
            expanded,
            text_field_modifier = Modifier
                .fill_max_width_with(0.98)
                .on_size_changed(move |size: IntSize| field_out.set(size.width)),
            menu_modifier = Modifier.on_size_changed(move |size: IntSize| menu_out.set(size.width))
        );
    });
    ui.on_node_with_tag(ITEM).assert_is_displayed();
    assert_eq!(field.get(), 588);
    assert_eq!(menu.get(), field.get());
}

/// Not matching, the menu's as wide as its widest item.
#[test]
fn not_matching_the_width_the_menu_is_as_wide_as_its_items() {
    let menu = Rc::new(Cell::new(0));
    let menu_out = menu.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 800), move || {
        let menu_out = menu_out.clone();
        MaterialTheme!({
            let menu_out = menu_out.clone();
            ExposedDropdownMenuBox!(true, |_: bool| {}, |scope: ExposedDropdownMenuBoxScope| {
                Box!(
                    modifier = Modifier
                        .size(300.dp())
                        .menu_anchor(&scope, ExposedDropdownMenuAnchorType::PrimaryNotEditable)
                );
                let menu_out = menu_out.clone();
                ExposedDropdownMenu!(
                    scope.clone(),
                    true,
                    || {},
                    modifier =
                        Modifier.on_size_changed(move |size: IntSize| menu_out.set(size.width)),
                    match_anchor_width = false,
                    |_: ColumnScope| {
                        Box!(modifier = Modifier.size(120.dp()));
                    }
                );
            });
        });
    });
    ui.wait_for_idle();
    assert_eq!(menu.get(), 120);
}

// Compose: edm_collapsesWithSelection_whenMenuItemClicked
#[test]
fn collapses_with_the_selection_when_an_item_is_clicked() {
    let (mut ui, _) = rule(true, false);
    ui.on_node_with_tag(ITEM).assert_is_displayed();
    click(&mut ui, ITEM);
    ui.on_node_with_tag(ITEM).assert_does_not_exist();
    ui.on_node_with_tag(TF).assert_text_contains(OPTION, false, false);
}

/// The menu's height: the larger space beside the anchor, less the margins (48 dp), which is the
/// height the ten items don't fit.
// Compose: edm_resizesWithinWindowBounds_uponImeAppearance
#[test]
fn the_height_is_the_larger_space_beside_the_anchor_within_the_keyboard() {
    let height = Rc::new(Cell::new(0));
    let height_out = height.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 800), move || {
        let height_out = height_out.clone();
        MaterialTheme!({
            let height_out = height_out.clone();
            ExposedDropdownMenuBox!(true, |_: bool| {}, |scope: ExposedDropdownMenuBoxScope| {
                Box!(
                    modifier = Modifier
                        .size(100.dp())
                        .menu_anchor(&scope, ExposedDropdownMenuAnchorType::PrimaryEditable)
                );
                let height_out = height_out.clone();
                ExposedDropdownMenu!(
                    scope.clone(),
                    true,
                    || {},
                    modifier =
                        Modifier.on_size_changed(move |size: IntSize| height_out.set(size.height)),
                    |_: ColumnScope| {
                        for _ in 0..20 {
                            Box!(modifier = Modifier.size(50.dp()));
                        }
                    }
                );
            });
        });
    });
    ui.wait_for_idle();
    // 800 less the anchor's 100 and the margin at the bottom.
    assert_eq!(height.get(), 800 - 100 - 48);
    ui.host().set_window_insets(compose_ui::platform::PlatformWindowInsets {
        ime: compose_ui::platform::Insets { bottom: 300, ..Default::default() },
        ..Default::default()
    });
    ui.wait_for_idle();
    assert_eq!(height.get(), 500 - 100 - 48);
}

// Compose: edm_doesNotCrash_whenAnchorDetachedFirst
#[test]
fn does_not_crash_when_the_anchor_is_removed_with_the_menu_open() {
    let state = mutable_state_of(true);
    let shown = state.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 800), move || {
        let _ = &state;
        MaterialTheme!({
            if shown.get() {
                ExposedDropdownMenuBox!(
                    true,
                    |_: bool| {},
                    |scope: ExposedDropdownMenuBoxScope| {
                        TextField!(
                            "Text".to_string(),
                            |_: String| {},
                            modifier = Modifier.menu_anchor(
                                &scope,
                                ExposedDropdownMenuAnchorType::PrimaryEditable
                            )
                        );
                        ExposedDropdownMenu!(scope.clone(), true, || {}, |_: ColumnScope| {
                            DropdownMenuItem!(
                                { Text!(OPTION) },
                                || {},
                                modifier = Modifier.test_tag(ITEM)
                            );
                        });
                    }
                );
            }
        });
    });
    ui.on_node_with_tag(ITEM).assert_is_displayed();
    shown.set(false);
    ui.wait_for_idle();
    ui.on_node_with_tag(ITEM).assert_does_not_exist();
}

// Compose: edm_withScrolledContent
#[test]
fn with_scrolled_content() {
    let scroll = Rc::new(Cell::new(None));
    let scroll_out = scroll.clone();
    let go_state = mutable_state_of(false);
    let go = go_state.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 800), move || {
        let _ = &go_state;
        let scroll_out = scroll_out.clone();
        MaterialTheme!({
            let scroll_out = scroll_out.clone();
            Box!(modifier = Modifier.fill_max_size(), |box_scope: BoxScope| {
                let scroll_out = scroll_out.clone();
                ExposedDropdownMenuBox!(
                    true,
                    |_: bool| {},
                    modifier = Modifier.align(box_scope, Alignment::CENTER),
                    |scope: ExposedDropdownMenuBoxScope| {
                        let state = remember_scroll_state(0);
                        scroll_out.set(Some(state));
                        ScrollWhen!(go, state, i32::MAX);
                        TextField!(
                            String::new(),
                            |_: String| {},
                            modifier = Modifier.menu_anchor(
                                &scope,
                                ExposedDropdownMenuAnchorType::PrimaryEditable,
                            )
                        );
                        ExposedDropdownMenu!(
                            scope.clone(),
                            true,
                            || {},
                            scroll_state = state,
                            |_: ColumnScope| {
                                for index in 1..=100 {
                                    Text!(
                                        format!("Text {index}"),
                                        modifier =
                                            Modifier.test_tag(format!("MenuContent {index}"))
                                    );
                                }
                            }
                        );
                    }
                );
            });
        });
    });
    ui.wait_for_idle();
    assert!(scroll.get().unwrap().max_value() > 0);
    go.set(true);
    ui.wait_for_idle();
    ui.on_node_with_tag("MenuContent 1").assert_is_not_displayed();
    ui.on_node_with_tag("MenuContent 100").assert_is_displayed();
}

// Compose: edm_hasDropdownSemantics
#[test]
fn has_dropdown_semantics() {
    let (mut ui, _) = rule(false, false);
    assert_eq!(
        ui.on_node_with_tag(TF).fetch_semantics_node().config.get(&SemanticsProperties::ROLE),
        Some(&Role::DropdownList)
    );
}

fn at(x: i32, y: i32, size: IntSize) -> Rect {
    Rect::new(x as f32, y as f32, (x + size.width) as f32, (y + size.height) as f32)
}

// Compose: edm_positionProvider
#[test]
fn position_provider() {
    let top_window_insets = 50;
    let density = Density::new(1.0);
    let anchor = IntSize::new(200, 100);
    let popup = IntSize::new(200, 340);
    let window = IntSize::new(500, 500);
    let margin = 48;
    let provider = ExposedDropdownMenuPositionProvider::new(density, top_window_insets);
    let position = |rect| provider.position(rect, window, LayoutDirection::Ltr, popup);
    // A typical case.
    assert_eq!(position(at(0, 0, anchor)), IntOffset::new(0, anchor.height));
    // Off-screen above.
    assert_eq!(position(at(0, -150, anchor)), IntOffset::new(0, margin));
    // With the window insets: without their allowance it would be off-screen.
    assert_eq!(position(at(0, 100, anchor)), IntOffset::new(0, 100 + anchor.height));
    // Off-screen below.
    assert_eq!(
        position(at(0, window.height + 100, anchor)),
        IntOffset::new(0, window.height + top_window_insets - margin - popup.height)
    );
}

// Compose: edm_positionProvider_contentExceedsWindowHeight
#[test]
fn position_provider_content_exceeds_the_window() {
    let provider = ExposedDropdownMenuPositionProvider::new(Density::new(1.0), 0);
    let window = IntSize::new(500, 200);
    let popup = IntSize::new(200, 300);
    let position =
        provider.position(at(0, 50, IntSize::new(200, 50)), window, LayoutDirection::Ltr, popup);
    assert_eq!(position.y, (window.height - popup.height) / 2);
}

/// Horizontally: at the anchor's start, else its end, else the window's side nearest it; and
/// right to left the other way around.
#[test]
fn position_provider_horizontally() {
    let provider = ExposedDropdownMenuPositionProvider::new(Density::new(1.0), 0);
    let (window, popup) = (IntSize::new(500, 800), IntSize::new(300, 100));
    let anchor = IntSize::new(100, 50);
    let x = |left, direction| provider.position(at(left, 0, anchor), window, direction, popup).x;
    assert_eq!(x(100, LayoutDirection::Ltr), 100);
    // Its start would leave the window: its end.
    assert_eq!(x(300, LayoutDirection::Ltr), 100);
    assert_eq!(x(300, LayoutDirection::Rtl), 100);
    assert_eq!(x(0, LayoutDirection::Rtl), 0);
    // Too wide either way: the window's side nearer the anchor's middle.
    let wide = IntSize::new(490, 100);
    let x = |left| provider.position(at(left, 0, anchor), window, LayoutDirection::Ltr, wide).x;
    assert_eq!(x(100), 0);
    assert_eq!(x(350), 10);
}

/// Above the anchor if not below it, and it grows from the side facing it.
#[test]
fn position_provider_above_the_anchor() {
    let provider = ExposedDropdownMenuPositionProvider::new(Density::new(1.0), 0);
    let (window, popup) = (IntSize::new(500, 800), IntSize::new(200, 300));
    let anchor = IntSize::new(200, 100);
    let position = provider.position(at(0, 600, anchor), window, LayoutDirection::Ltr, popup);
    assert_eq!(position, IntOffset::new(0, 300));
}

#[test]
fn item_content_padding_is_as_compose_s() {
    let padding = ExposedDropdownMenuDefaults::ITEM_CONTENT_PADDING;
    assert_eq!(padding.start, 16.dp());
    assert_eq!(padding.end, 16.dp());
    assert_eq!(padding.top, 0.dp());
    assert_eq!(padding.bottom, 0.dp());
}

/// A box of `anchor_top` in the window at 800 tall, with a tall menu: its height.
fn menu_height_at(anchor_top: i32) -> i32 {
    let height = Rc::new(Cell::new(0));
    let height_out = height.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 800), move || {
        let height_out = height_out.clone();
        MaterialTheme!({
            let height_out = height_out.clone();
            ExposedDropdownMenuBox!(
                true,
                |_: bool| {},
                modifier = Modifier.offset(0.dp(), (anchor_top as f32).dp()),
                |scope: ExposedDropdownMenuBoxScope| {
                    Box!(
                        modifier = Modifier
                            .size(100.dp())
                            .menu_anchor(&scope, ExposedDropdownMenuAnchorType::PrimaryEditable)
                    );
                    let height_out = height_out.clone();
                    ExposedDropdownMenu!(
                        scope.clone(),
                        true,
                        || {},
                        modifier = Modifier
                            .on_size_changed(move |size: IntSize| height_out.set(size.height)),
                        |_: ColumnScope| {
                            for _ in 0..40 {
                                Box!(modifier = Modifier.size(50.dp()));
                            }
                        }
                    );
                }
            );
        });
    });
    ui.wait_for_idle();
    height.get()
}

/// The larger space, above when that's it; the window less its margins with the anchor out of it,
/// scrolled away either way.
#[test]
fn the_height_at_the_bottom_and_with_the_anchor_out_of_the_window() {
    assert_eq!(menu_height_at(600), 600 - 48);
    assert_eq!(menu_height_at(-200), 800 - 2 * 48);
    assert_eq!(menu_height_at(900), 800 - 2 * 48);
}

/// As wide as the anchor even if an item's wider.
#[test]
fn a_wider_item_does_not_widen_a_menu_matching_its_anchor() {
    let menu = Rc::new(Cell::new(0));
    let menu_out = menu.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 800), move || {
        let menu_out = menu_out.clone();
        MaterialTheme!({
            let menu_out = menu_out.clone();
            ExposedDropdownMenuBox!(true, |_: bool| {}, |scope: ExposedDropdownMenuBoxScope| {
                Box!(
                    modifier = Modifier
                        .size(100.dp())
                        .menu_anchor(&scope, ExposedDropdownMenuAnchorType::PrimaryNotEditable)
                );
                let menu_out = menu_out.clone();
                ExposedDropdownMenu!(
                    scope.clone(),
                    true,
                    || {},
                    modifier =
                        Modifier.on_size_changed(move |size: IntSize| menu_out.set(size.width)),
                    |_: ColumnScope| {
                        Box!(modifier = Modifier.required_width(300.dp()).height(10.dp()));
                    }
                );
            });
        });
    });
    ui.wait_for_idle();
    assert_eq!(menu.get(), 100);
}

/// A box of 100 by 100 as the anchor, in a box that's told `on_expanded_change`.
fn box_anchor(
    anchor_type: ExposedDropdownMenuAnchorType,
    outer: Modifier,
    inner: Modifier,
) -> (ComposeTestRule, Rc<Cell<i32>>) {
    let changes = Rc::new(Cell::new(0));
    let out = changes.clone();
    let ui = ComposeTestRule::sized(IntSize::new(600, 800), move || {
        let out = out.clone();
        let (outer, inner) = (outer.clone(), inner.clone());
        MaterialTheme!({
            let (out, outer, inner) = (out.clone(), outer.clone(), inner.clone());
            Box!(modifier = outer, {
                let (out, inner) = (out.clone(), inner.clone());
                ExposedDropdownMenuBox!(
                    false,
                    move |_: bool| out.set(out.get() + 1),
                    |scope: ExposedDropdownMenuBoxScope| {
                        Box!(
                            modifier = Modifier
                                .menu_anchor(&scope, anchor_type)
                                .then(inner.clone())
                                .test_tag(TF)
                                .focusable()
                                .size(100.dp())
                        );
                    }
                );
            });
        });
    });
    (ui, changes)
}

/// With nothing in it that takes the move, leaving the anchor is what cancels the press.
// Compose: edm_doesNotExpand_ifTouchEndsOutsideBounds
#[test]
fn a_press_dragged_out_of_the_anchor_does_nothing() {
    let (mut ui, changes) =
        box_anchor(ExposedDropdownMenuAnchorType::PrimaryNotEditable, Modifier, Modifier);
    ui.on_node_with_tag(TF).perform_touch_input(|touch| {
        touch.down(Offset::new(50.0, 50.0));
        touch.move_by(Offset::new(0.0, 60.0));
        touch.move_by(Offset::new(0.0, -60.0));
        touch.up();
    });
    assert_eq!(changes.get(), 0);
    click(&mut ui, TF);
    assert_eq!(changes.get(), 1);
}

/// A press that something around took on its way in isn't the anchor's.
#[test]
fn a_press_taken_on_the_way_in_is_not_the_anchors() {
    let (mut ui, changes) = box_anchor(
        ExposedDropdownMenuAnchorType::PrimaryNotEditable,
        Modifier.then_custom(TakesPresses),
        Modifier,
    );
    click(&mut ui, TF);
    assert_eq!(changes.get(), 0);
}

struct TakesPresses;
struct TakesPressesNode;

impl compose_ui::ModifierNodeElement for TakesPresses {
    type Node = TakesPressesNode;
    fn create(&self) -> TakesPressesNode {
        TakesPressesNode
    }
    fn update(&self, _: &mut TakesPressesNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl compose_ui::ModifierNode for TakesPressesNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn compose_ui::PointerInputModifierNode> {
        Some(self)
    }
}

impl compose_ui::PointerInputModifierNode for TakesPressesNode {
    fn on_pointer_event(
        &mut self,
        event: &mut compose_ui::PointerEvent,
        pass: compose_ui::PointerEventPass,
        _: IntSize,
        _: &mut compose_ui::EventContext<'_>,
    ) {
        if pass == compose_ui::PointerEventPass::Initial
            && event.kind == compose_ui::PointerEventType::Press
        {
            event.consume();
        }
    }
}

/// Enter's release is what toggles, and for what's typed in it goes no further; down, it's not
/// the menu's.
#[test]
fn enter_toggles_on_its_release_and_an_editable_anchor_keeps_it() {
    let keys = Rc::new(Cell::new(0));
    let heard = keys.clone();
    let (mut ui, changes) = box_anchor(
        ExposedDropdownMenuAnchorType::PrimaryEditable,
        Modifier,
        Modifier.on_key_event(move |event: compose_ui::KeyEvent| {
            if event.kind == compose_ui::KeyEventType::KeyUp {
                heard.set(heard.get() + 1);
            }
            false
        }),
    );
    ui.on_node_with_tag(TF).request_focus();
    ui.host().send_key(compose_ui::KeyEvent::down(Key::Enter));
    ui.wait_for_idle();
    assert_eq!(changes.get(), 0);
    ui.host().send_key(compose_ui::KeyEvent::up(Key::Enter));
    ui.wait_for_idle();
    assert_eq!(changes.get(), 1);
    assert_eq!(keys.get(), 0);
    // Not an arrow either, closed: that's the field's.
    ui.press_key(Key::DirectionDown);
    assert_eq!(keys.get(), 1);
}

/// A secondary anchor's press isn't the field's in it: the field gets the focus only by the menu
/// opening, here not at all.
#[test]
fn a_secondary_anchors_press_does_not_reach_the_field() {
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 800), || {
        MaterialTheme!({
            ExposedDropdownMenuBox!(false, |_: bool| {}, |scope: ExposedDropdownMenuBoxScope| {
                let icon = scope.clone();
                TextField!(
                    String::new(),
                    |_: String| {},
                    modifier = Modifier
                        .menu_anchor(&scope, ExposedDropdownMenuAnchorType::PrimaryEditable)
                        .test_tag(TF),
                    trailing_icon = Some(move || {
                        ExposedDropdownMenuDefaults::trailing_icon(
                            false,
                            Modifier
                                .menu_anchor(
                                    &icon,
                                    ExposedDropdownMenuAnchorType::SecondaryEditable,
                                )
                                .test_tag(TRAILING_ICON),
                        );
                    })
                );
            });
        });
    });
    click(&mut ui, TRAILING_ICON);
    ui.on_node_with_tag(TF).assert_is_not_focused();
    click(&mut ui, TF);
    ui.on_node_with_tag(TF).assert_is_focused();
}

/// A secondary anchor alone doesn't make the box's type its own by attaching; clicked, it does.
#[test]
fn a_secondary_anchor_is_the_type_only_when_clicked() {
    let seen = Rc::new(Cell::new(None));
    let seen_out = seen.clone();
    let state = mutable_state_of(0);
    let tick = state.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 800), move || {
        let _ = &state;
        let seen = seen_out.clone();
        MaterialTheme!({
            let seen = seen.clone();
            ExposedDropdownMenuBox!(
                false,
                move |_: bool| tick.set(tick.get() + 1),
                |scope: ExposedDropdownMenuBoxScope| {
                    let _ = tick.get();
                    Box!(
                        modifier = Modifier
                            .size(100.dp())
                            .menu_anchor(&scope, ExposedDropdownMenuAnchorType::SecondaryEditable)
                            .test_tag(TF)
                    );
                    let (seen, scope) = (seen.clone(), scope.clone());
                    SideEffect(move || seen.set(Some(scope.anchor_type())));
                }
            );
        });
    });
    assert_eq!(seen.get(), Some(ExposedDropdownMenuAnchorType::PrimaryNotEditable));
    click(&mut ui, TF);
    assert_eq!(seen.get(), Some(ExposedDropdownMenuAnchorType::SecondaryEditable));
}

/// The focus the keyboard gave the menu ends with the next key the anchor hears: opened again,
/// the menu leaves the focus in what's typed in.
#[test]
fn a_key_on_the_anchor_ends_the_menus_taking_the_focus() {
    let (mut ui, expanded) = rule(true, true);
    ui.on_node_with_tag(TF).request_focus();
    ui.press_key(Key::Tab);
    assert!(!field_takes_typing(&mut ui, "y"));
    expanded.set(false);
    ui.wait_for_idle();
    ui.on_node_with_tag(TF).request_focus();
    ui.press_key(Key::Character('a'));
    expanded.set(true);
    ui.wait_for_idle();
    assert!(field_takes_typing(&mut ui, "z"));
}

#[test]
fn right_to_left_it_starts_at_the_anchors_right() {
    let provider = ExposedDropdownMenuPositionProvider::new(Density::new(1.0), 0);
    let (window, popup) = (IntSize::new(500, 800), IntSize::new(100, 100));
    let anchor = at(200, 0, IntSize::new(200, 50));
    assert_eq!(provider.position(anchor, window, LayoutDirection::Ltr, popup).x, 200);
    assert_eq!(provider.position(anchor, window, LayoutDirection::Rtl, popup).x, 300);
}

/// The arrow turns while it's open.
#[test]
fn the_trailing_icon_turns_when_expanded() {
    let state = mutable_state_of(false);
    let expanded = state.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        let _ = &state;
        MaterialTheme!({
            ExposedDropdownMenuDefaults::trailing_icon(expanded.get(), Modifier);
        });
    });
    let closed = ui.dump_display_list();
    expanded.set(true);
    ui.wait_for_idle();
    assert_ne!(ui.dump_display_list(), closed);
}

/// Disabled, the filled field's container is as enabled (`FilledAutocompleteTokens`), not
/// `TextFieldDefaults`' faded one; the outlined one's as `OutlinedTextFieldDefaults`.
#[test]
fn the_text_field_colors() {
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), || {
        MaterialTheme!({
            let colors = ExposedDropdownMenuDefaults::text_field_colors();
            assert_eq!(
                colors.disabled_container_color,
                MaterialTheme::color_scheme().surface_container_highest
            );
            assert_eq!(colors.container_color, TextFieldDefaults::colors().container_color);
            let outlined = ExposedDropdownMenuDefaults::outlined_text_field_colors();
            assert_eq!(
                outlined.container_color,
                OutlinedTextFieldDefaults::colors().container_color
            );
        });
    });
    ui.wait_for_idle();
}

/// A window with a keyboard or bars, an anchor at `anchor_top` and `items` items of 50: where the
/// menu is. The insets come after the menu's open, so they also move and size it.
fn menu_with_insets(
    window: IntSize,
    status_top: i32,
    bottom: (i32, i32),
    (anchor_top, anchor_height): (i32, i32),
    items: usize,
) -> Rect {
    let mut ui = ComposeTestRule::sized(window, move || {
        MaterialTheme!({
            ExposedDropdownMenuBox!(
                true,
                |_: bool| {},
                modifier = Modifier.offset(0.dp(), (anchor_top as f32).dp()),
                |scope: ExposedDropdownMenuBoxScope| {
                    Box!(
                        modifier = Modifier
                            .size((100.dp(), (anchor_height as f32).dp()))
                            .menu_anchor(&scope, ExposedDropdownMenuAnchorType::PrimaryEditable)
                    );
                    ExposedDropdownMenu!(
                        scope.clone(),
                        true,
                        || {},
                        modifier = Modifier.test_tag(EDM),
                        |_: ColumnScope| {
                            for _ in 0..items {
                                Box!(modifier = Modifier.size(50.dp()));
                            }
                        }
                    );
                }
            );
        });
    });
    ui.wait_for_idle();
    let (ime, navigation) = bottom;
    let inset = |top, bottom| compose_ui::platform::Insets { top, bottom, ..Default::default() };
    ui.host().set_window_insets(compose_ui::platform::PlatformWindowInsets {
        status_bars: inset(status_top, 0),
        navigation_bars: inset(0, navigation),
        ime: inset(0, ime),
        ..Default::default()
    });
    ui.wait_for_idle();
    ui.on_node_with_tag(EDM).fetch_semantics_node().bounds
}

/// An open menu of what's typed in goes above the anchor when the keyboard is where it would go.
#[test]
fn the_menu_goes_above_the_anchor_when_the_keyboard_is_below() {
    let menu = menu_with_insets(IntSize::new(600, 800), 0, (300, 0), (420, 56), 3);
    assert_eq!(menu.top, 254.0);
    assert_eq!(menu.bottom, 420.0);
}

/// The status bars and the navigation bar leave the window Compose's popup has: the bars' 48 at
/// the bottom have no room for the menu, the status bars' 24 being added back as anchors count
/// from the very top.
#[test]
fn the_menu_goes_above_the_anchor_when_the_navigation_bar_is_below() {
    let menu = menu_with_insets(IntSize::new(411, 891), 24, (0, 48), (700, 56), 2);
    assert_eq!(menu.top, 584.0);
    assert_eq!(menu.bottom, 700.0);
}

/// A keyboard over the navigation bar counts once, the larger.
#[test]
fn the_larger_of_the_keyboard_and_the_navigation_bar_counts() {
    let menu = menu_with_insets(IntSize::new(600, 800), 0, (300, 48), (420, 56), 3);
    assert_eq!(menu.top, 254.0);
    let menu = menu_with_insets(IntSize::new(600, 800), 0, (10, 300), (420, 56), 3);
    assert_eq!(menu.top, 254.0);
}

/// The height is of the same window: from the anchor's bottom to the margin above the bars.
#[test]
fn the_height_below_the_anchor_is_within_the_bars() {
    let menu = menu_with_insets(IntSize::new(411, 891), 24, (0, 48), (100, 56), 30);
    assert_eq!(menu.height(), 639.0);
    assert_eq!(menu.top, 156.0);
}

/// And above the anchor: from the margin below the status bars.
#[test]
fn the_height_above_the_anchor_is_within_the_status_bars() {
    let menu = menu_with_insets(IntSize::new(411, 891), 24, (0, 48), (780, 56), 30);
    assert_eq!(menu.height(), 708.0);
    assert_eq!(menu.top, 72.0);
}
