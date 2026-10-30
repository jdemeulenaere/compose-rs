//! Compose's `ChipTest.kt` and `ChipScreenshotTest.kt`, adapted: every chip's semantics, clicks,
//! height, paddings, arrangements and colors, the goldens as the display list's shapes and colors.
//! Compose's `assertShape` on a captured image is the background's color here.
//!
//! Not ported: the `darkTheme` twins of the goldens (a scheme's other values, the same code).

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::BorderStroke;
use compose_foundation::layout::*;
use compose_foundation::{ScrollModifiers, remember_scroll_state};
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Color, CornerSize, Dp, IntSize, IntrinsicSize, Modifier, Outline, Role, SemanticsProperties,
    Shape, UnitExt, platform::DrawOp,
};

const TAG: &str = "chip";

fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    let content = Rc::new(content);
    ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let content = content.clone();
        MaterialTheme!({
            content();
        });
    })
}

fn role(ui: &mut ComposeTestRule, tag: &str) -> Option<Role> {
    ui.on_node_with_tag(tag)
        .fetch_semantics_snapshot()
        .config
        .get(&SemanticsProperties::ROLE)
        .copied()
}

fn left(ui: &mut ComposeTestRule, tag: &str) -> f32 {
    ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_snapshot().bounds.left
}

fn width(ui: &mut ComposeTestRule, tag: &str) -> f32 {
    ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_snapshot().bounds.width()
}

/// A leading or trailing icon of the chips' 18 dp.
#[composable]
fn Icon18(tag: &'static str) {
    Box!(modifier = Modifier.size(18.dp()).test_tag(tag));
}

// ---- Semantics --------------------------------------------------------------------------------

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, defaultSemantics_assistChip
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, disabledSemantics_assistChip
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, defaultSemantics_suggestionChip
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, disabledSemantics_suggestionChip
#[test]
fn a_chip_is_a_button_clickable_while_enabled() {
    let mut ui = rule(|| {
        Column!({
            AssistChip!(|| {}, || Text!("a"), modifier = Modifier.test_tag("a"));
            AssistChip!(|| {}, || Text!("b"), modifier = Modifier.test_tag("b"), enabled = false);
            SuggestionChip!(|| {}, || Text!("c"), modifier = Modifier.test_tag("c"));
            SuggestionChip!(
                || {},
                || Text!("d"),
                modifier = Modifier.test_tag("d"),
                enabled = false
            );
            ElevatedAssistChip!(|| {}, || Text!("e"), modifier = Modifier.test_tag("e"));
            ElevatedSuggestionChip!(|| {}, || Text!("f"), modifier = Modifier.test_tag("f"));
        });
    });
    for tag in ["a", "c", "e", "f"] {
        assert_eq!(role(&mut ui, tag), Some(Role::Button), "{tag}");
        ui.on_node_with_tag(tag).assert_is_enabled().assert_has_click_action();
    }
    for tag in ["b", "d"] {
        assert_eq!(role(&mut ui, tag), Some(Role::Button), "{tag}");
        ui.on_node_with_tag(tag).assert_is_not_enabled().assert_has_click_action();
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, unselectedSemantics_filterChip
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, selectedSemantics_filterChip
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, disabledSemantics_filterChip
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, defaultSemantics_inputChip
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, unselectedSemantics_inputChip
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, selectedSemantics_inputChip
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, disabledSemantics_inputChip
#[test]
fn a_selectable_chip_is_a_checkbox_with_its_selection() {
    let mut ui = rule(|| {
        Column!({
            FilterChip!(false, || {}, || Text!("a"), modifier = Modifier.test_tag("a"));
            FilterChip!(true, || {}, || Text!("b"), modifier = Modifier.test_tag("b"));
            FilterChip!(
                false,
                || {},
                || Text!("c"),
                modifier = Modifier.test_tag("c"),
                enabled = false
            );
            InputChip!(false, || {}, || Text!("d"), modifier = Modifier.test_tag("d"));
            InputChip!(true, || {}, || Text!("e"), modifier = Modifier.test_tag("e"));
            InputChip!(
                false,
                || {},
                || Text!("f"),
                modifier = Modifier.test_tag("f"),
                enabled = false
            );
            ElevatedFilterChip!(true, || {}, || Text!("g"), modifier = Modifier.test_tag("g"));
        });
    });
    for tag in ["a", "d"] {
        assert_eq!(role(&mut ui, tag), Some(Role::Checkbox), "{tag}");
        ui.on_node_with_tag(tag).assert_is_enabled().assert_is_not_selected();
    }
    for tag in ["b", "e", "g"] {
        assert_eq!(role(&mut ui, tag), Some(Role::Checkbox), "{tag}");
        ui.on_node_with_tag(tag).assert_is_enabled().assert_is_selected().assert_has_click_action();
    }
    for tag in ["c", "f"] {
        assert_eq!(role(&mut ui, tag), Some(Role::Checkbox), "{tag}");
        ui.on_node_with_tag(tag).assert_is_not_enabled().assert_is_not_selected();
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, onClick_assistChip
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, onClick_suggestionChip
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, canBeDisabled
#[test]
fn a_click_reaches_an_enabled_chip_only() {
    let clicks = Rc::new(Cell::new(0));
    let (a, b, c) = (clicks.clone(), clicks.clone(), clicks.clone());
    let mut ui = rule(move || {
        let (a, b, c) = (a.clone(), b.clone(), c.clone());
        Column!({
            let (a, b, c) = (a.clone(), b.clone(), c.clone());
            AssistChip!(
                move || a.set(a.get() + 1),
                || Text!("a"),
                modifier = Modifier.test_tag("a")
            );
            SuggestionChip!(
                move || b.set(b.get() + 1),
                || Text!("b"),
                modifier = Modifier.test_tag("b")
            );
            SuggestionChip!(
                move || c.set(c.get() + 100),
                || Text!("c"),
                modifier = Modifier.test_tag("c"),
                enabled = false
            );
        });
    });
    ui.on_node_with_tag("a").perform_click();
    ui.on_node_with_tag("b").perform_click();
    ui.on_node_with_tag("c").perform_click();
    assert_eq!(clicks.get(), 2);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, toggle_filterChip
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, toggle_inputChip
#[test]
fn a_selectable_chip_toggles_on_a_click() {
    let (filter, input) = (mutable_state_of(false), mutable_state_of(false));
    let (a, b) = (filter.handle(), input.handle());
    let mut ui = rule(move || {
        Column!({
            FilterChip!(
                a.get(),
                move || a.update(|on| !on),
                || Text!("a"),
                modifier = Modifier.test_tag("a")
            );
            InputChip!(
                b.get(),
                move || b.update(|on| !on),
                || Text!("b"),
                modifier = Modifier.test_tag("b")
            );
        });
    });
    for tag in ["a", "b"] {
        ui.on_node_with_tag(tag).assert_is_not_selected().perform_click();
        ui.on_node_with_tag(tag).assert_is_selected().perform_click();
        ui.on_node_with_tag(tag).assert_is_not_selected();
    }
}

// ---- Sizes ------------------------------------------------------------------------------------

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, heightIsFromSpec_assistChip
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, heightIsFromSpec_filterChip
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, heightIsFromSpec_inputChip
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, heightIsFromSpec_suggestionChip
#[test]
fn each_chip_is_32_dp_high() {
    assert_eq!(AssistChipDefaults::HEIGHT, 32.dp());
    assert_eq!(FilterChipDefaults::HEIGHT, 32.dp());
    assert_eq!(InputChipDefaults::HEIGHT, 32.dp());
    assert_eq!(SuggestionChipDefaults::HEIGHT, 32.dp());
    let mut ui = rule(|| {
        Column!({
            AssistChip!(|| {}, || Text!("a"), modifier = Modifier.test_tag("a"));
            FilterChip!(false, || {}, || Text!("b"), modifier = Modifier.test_tag("b"));
            InputChip!(false, || {}, || Text!("c"), modifier = Modifier.test_tag("c"));
            SuggestionChip!(|| {}, || Text!("d"), modifier = Modifier.test_tag("d"));
            ElevatedAssistChip!(|| {}, || Text!("e"), modifier = Modifier.test_tag("e"));
            ElevatedFilterChip!(false, || {}, || Text!("f"), modifier = Modifier.test_tag("f"));
            ElevatedSuggestionChip!(|| {}, || Text!("g"), modifier = Modifier.test_tag("g"));
        });
    });
    for tag in ["a", "b", "c", "d", "e", "f", "g"] {
        ui.on_node_with_tag(tag).assert_height_is_equal_to(32.dp());
    }
}

/// With no icons, 8 dp of padding and 8 dp of spacing each side of the label.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, horizontalPadding_assistChip
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, horizontalPadding_unselected_filterChip
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, horizontalPadding_selected_filterChip
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, horizontalPadding_suggestionChip
#[test]
fn with_no_icons_the_label_is_16_dp_in() {
    let label = || Text!("Label", modifier = Modifier.test_tag("label"));
    let check = |ui: &mut ComposeTestRule| {
        assert_eq!(left(ui, "label"), 16.0);
        assert_eq!(width(ui, TAG), width(ui, "label") + 32.0);
    };
    check(&mut rule(move || AssistChip!(|| {}, label, modifier = Modifier.test_tag(TAG))));
    check(&mut rule(move || FilterChip!(false, || {}, label, modifier = Modifier.test_tag(TAG))));
    check(&mut rule(move || FilterChip!(true, || {}, label, modifier = Modifier.test_tag(TAG))));
    check(&mut rule(move || SuggestionChip!(|| {}, label, modifier = Modifier.test_tag(TAG))));
}

/// An input chip has less: 4 dp, and the 8 of spacing.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, horizontalPadding_inputChip
#[test]
fn with_no_icons_an_input_chips_label_is_12_dp_in() {
    let mut ui = rule(move || {
        InputChip!(
            false,
            || {},
            || Text!("Label", modifier = Modifier.test_tag("label")),
            modifier = Modifier.test_tag(TAG)
        );
    });
    assert_eq!(left(&mut ui, "label"), 12.0);
    assert_eq!(width(&mut ui, TAG), width(&mut ui, "label") + 24.0);
}

/// Beside an icon: 8, the icon, 8, the label.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, horizontalPadding_assistChip_withLeadingIcon
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, horizontalPadding_suggestionChip_withLeadingIcon
#[test]
fn beside_a_leading_icon_the_label_is_34_dp_in() {
    let label = || Text!("Label", modifier = Modifier.test_tag("label"));
    let check = |ui: &mut ComposeTestRule| {
        assert_eq!(left(ui, "leading"), 8.0);
        assert_eq!(left(ui, "label"), 34.0);
        assert_eq!(width(ui, TAG), width(ui, "label") + 8.0 * 4.0 + 18.0);
    };
    check(&mut rule(move || {
        AssistChip!(
            || {},
            label,
            modifier = Modifier.test_tag(TAG),
            leading_icon = Some(|| Icon18("leading"))
        );
    }));
    check(&mut rule(move || {
        SuggestionChip!(
            || {},
            label,
            modifier = Modifier.test_tag(TAG),
            icon = Some(|| Icon18("leading"))
        );
    }));
}

/// A chip wider than it needs: the leading icon and the label from its start, the trailing icon at
/// its end.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, horizontalPadding_assistChip_customWidth
#[test]
fn a_wide_chip_keeps_its_trailing_icon_at_its_end() {
    let mut ui = rule(|| {
        AssistChip!(
            || {},
            || Text!("Label", modifier = Modifier.test_tag("label")),
            modifier = Modifier.width(200.dp()),
            leading_icon = Some(|| Icon18("leading")),
            trailing_icon = Some(|| Icon18("trailing"))
        );
    });
    assert_eq!(left(&mut ui, "leading"), 8.0);
    assert_eq!(left(&mut ui, "label"), 34.0);
    assert_eq!(left(&mut ui, "trailing"), 200.0 - 8.0 - 18.0);
}

/// `content_padding` and `horizontal_arrangement` take the place of the defaults.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, horizontalPadding_assistChip_withContentPaddingAndSpacing
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, horizontalPadding_filterChip_withContentPaddingAndSpacing
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, horizontalPadding_inputChip_withContentPaddingAndSpacing
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, horizontalPadding_suggestionChip_withContentPaddingAndSpacing
#[test]
fn a_chips_padding_and_spacing_are_its_own() {
    let padding = PaddingValues::symmetric(6.dp(), 0.dp());
    let label = || Text!("Label", modifier = Modifier.test_tag("label"));
    let check = |ui: &mut ComposeTestRule| {
        assert_eq!(left(ui, "leading"), 6.0);
        assert_eq!(left(ui, "label"), 6.0 + 18.0 + 6.0);
        assert_eq!(width(ui, TAG), width(ui, "label") + 6.0 * 4.0 + 18.0);
    };
    check(&mut rule(move || {
        AssistChip!(
            || {},
            label,
            modifier = Modifier.test_tag(TAG),
            leading_icon = Some(|| Icon18("leading")),
            horizontal_arrangement = AssistChipDefaults::horizontal_arrangement_with(6.dp()),
            content_padding = padding
        );
    }));
    check(&mut rule(move || {
        FilterChip!(
            false,
            || {},
            label,
            modifier = Modifier.test_tag(TAG),
            leading_icon = Some(|| Icon18("leading")),
            horizontal_arrangement = FilterChipDefaults::horizontal_arrangement_with(6.dp()),
            content_padding = padding
        );
    }));
    check(&mut rule(move || {
        InputChip!(
            false,
            || {},
            label,
            modifier = Modifier.test_tag(TAG),
            leading_icon = Some(|| Icon18("leading")),
            horizontal_arrangement = InputChipDefaults::horizontal_arrangement_with(6.dp()),
            content_padding = padding
        );
    }));
    check(&mut rule(move || {
        SuggestionChip!(
            || {},
            label,
            modifier = Modifier.test_tag(TAG),
            icon = Some(|| Icon18("leading")),
            horizontal_arrangement = SuggestionChipDefaults::horizontal_arrangement_with(6.dp()),
            content_padding = padding
        );
    }));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, horizontalPadding_filterChip_withIcons
#[test]
fn a_filter_chips_two_icons_each_have_8_dp_around() {
    let mut ui = rule(|| {
        FilterChip!(
            false,
            || {},
            || Text!("Label", modifier = Modifier.test_tag("label")),
            modifier = Modifier.test_tag(TAG),
            leading_icon = Some(|| Icon18("leading")),
            trailing_icon = Some(|| Icon18("trailing"))
        );
    });
    assert_eq!(left(&mut ui, "label"), 34.0);
    assert_eq!(width(&mut ui, TAG), width(&mut ui, "label") + 8.0 * 4.0 + 36.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, horizontalPadding_inputChip_withLeadingIcon
#[test]
fn an_input_chips_leading_icon_is_8_dp_in() {
    let mut ui = rule(|| {
        InputChip!(
            false,
            || {},
            || Text!("Label", modifier = Modifier.test_tag("label")),
            modifier = Modifier.test_tag(TAG),
            leading_icon = Some(|| Icon18("leading"))
        );
    });
    assert_eq!(left(&mut ui, "leading"), 8.0);
    assert_eq!(left(&mut ui, "label"), 34.0);
    assert_eq!(width(&mut ui, TAG), width(&mut ui, "label") + 8.0 * 3.0 + 18.0 + 4.0);
}

/// Its avatar 4 dp in, 24 dp wide, and clipped round.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, horizontalPadding_inputChip_withAvatar
#[test]
fn an_input_chips_avatar_is_4_dp_in() {
    assert_eq!(InputChipDefaults::AVATAR_SIZE, 24.dp());
    let mut ui = rule(|| {
        InputChip!(
            false,
            || {},
            || Text!("Label", modifier = Modifier.test_tag("label")),
            modifier = Modifier.test_tag(TAG),
            avatar = Some(|| {
                Box!(modifier = Modifier.size(24.dp()).test_tag("avatar").background(Color::RED));
            })
        );
    });
    assert_eq!(left(&mut ui, "avatar"), 4.0);
    assert_eq!(left(&mut ui, "label"), 4.0 + 24.0 + 8.0);
    assert_eq!(width(&mut ui, TAG), width(&mut ui, "label") + 4.0 * 2.0 + 24.0 + 8.0 * 2.0);
    let list = ui.host().display_list();
    let layer =
        list.groups.iter().filter_map(|group| group.layer.as_ref()).find(|layer| layer.clip);
    let layer = layer.expect("the avatar's layer");
    assert!(matches!(layer.outline, Some(Outline::Rounded(_, radii)) if radii[0] == 12.0));
}

/// Disabled, the avatar fades to 0.38.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Chip.kt, InputChip
#[test]
fn a_disabled_input_chips_avatar_fades() {
    let ui_alpha = |enabled: bool| {
        let mut ui = rule(move || {
            InputChip!(
                false,
                || {},
                || Text!("Label"),
                enabled = enabled,
                avatar = Some(|| Box!(modifier = Modifier.size(24.dp()).background(Color::RED)))
            );
        });
        let list = ui.host().display_list();
        list.groups
            .iter()
            .filter_map(|group| group.layer.as_ref())
            .find(|it| it.clip)
            .map(|it| it.alpha)
    };
    assert_eq!(ui_alpha(true), Some(1.0));
    assert_eq!(ui_alpha(false), Some(0.38));
}

/// A label's width in a row that can scroll: its 64 dp and the 32 dp around.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, correctDimensionsInScrollableRow_filterChip
#[test]
fn a_filter_chip_in_a_scrolling_row_is_as_wide_as_its_label() {
    let mut ui = rule(|| {
        Row!(modifier = Modifier.horizontal_scroll(remember_scroll_state!()), {
            FilterChip!(
                false,
                || {},
                || Box!(modifier = Modifier.width(64.dp())),
                modifier = Modifier.test_tag(TAG)
            );
        });
    });
    ui.on_node_with_tag(TAG).assert_height_is_equal_to(32.dp()).assert_width_is_equal_to(96.dp());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, intrinsicSize_filterChip
#[test]
fn a_filter_chips_intrinsic_sizes_are_those_of_its_parts() {
    let chip = |label: &'static str| {
        FilterChip!(
            false,
            || {},
            move || Text!("Text", modifier = Modifier.test_tag(label)),
            leading_icon = Some(|| Box!(modifier = Modifier.size(24.dp()))),
            trailing_icon = Some(|| Box!(modifier = Modifier.size(24.dp())))
        );
    };
    let mut ui = rule(move || {
        Column!({
            Box!(modifier = Modifier.height(IntrinsicSize::Max).test_tag("max"), {
                chip("label max");
            });
            Box!(modifier = Modifier.height(IntrinsicSize::Min).test_tag("min"), {
                chip("label min");
            });
        });
    });
    for (tag, label) in [("max", "label max"), ("min", "label min")] {
        let expected = 24.0 * 2.0 + width(&mut ui, label) + 8.0 * 4.0;
        ui.on_node_with_tag(tag)
            .assert_height_is_equal_to(48.dp())
            .assert_width_is_equal_to(Dp(expected));
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, longLabelDoesNotHideTrailingIcon_filterChip
#[test]
fn a_long_label_leaves_room_for_the_trailing_icon() {
    let mut ui = rule(|| {
        FilterChip!(
            false,
            || {},
            || Text!("{}", "Long ".repeat(40)),
            leading_icon = Some(|| Icon18("leading")),
            trailing_icon = Some(|| Icon18("trailing"))
        );
    });
    ui.on_node_unmerged(has_test_tag("trailing")).assert_is_displayed();
    assert_eq!(left(&mut ui, "trailing"), 400.0 - 8.0 - 18.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, withLargeFontSizeIsLargerThenHeight
#[test]
fn a_large_label_makes_a_chip_taller() {
    let mut ui = rule(|| {
        SuggestionChip!(
            || {},
            // A tall label: the test's text measurer has one height for every font size.
            || Box!(modifier = Modifier.size((10.dp(), 50.dp()))),
            modifier = Modifier.test_tag(TAG)
        );
    });
    ui.on_node_with_tag(TAG).assert_height_is_at_least(33.dp());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, propagateDefaultTextStyle
#[test]
fn a_label_has_the_label_large_style() {
    thread_local! {
        static STYLE: Cell<Option<compose_ui::TextUnit>> = const { Cell::new(None) };
    }
    let expected = Rc::new(Cell::new(None));
    let large = expected.clone();
    let _ui = rule(move || {
        large.set(Some(MaterialTheme::typography().label_large.font_size));
        SuggestionChip!(|| {}, || STYLE.set(Some(LOCAL_TEXT_STYLE.current().font_size)));
    });
    assert!(STYLE.get().is_some());
    assert_eq!(STYLE.get(), expected.get());
}

/// Its label's children in a row, centered down it.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, contentIsRow
#[test]
fn a_chips_label_is_a_row() {
    let mut ui = rule(|| {
        SuggestionChip!(
            || {},
            || {
                Spacer(Modifier.size(10.dp()).test_tag("first"));
                Spacer(Modifier.size((10.dp(), 5.dp())).test_tag("second"));
            },
            modifier = Modifier.test_tag(TAG)
        );
    });
    let bounds = |ui: &mut ComposeTestRule, tag: &str| {
        ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_snapshot().bounds
    };
    let chip = bounds(&mut ui, TAG);
    let (first, second) = (bounds(&mut ui, "first"), bounds(&mut ui, "second"));
    let (center_x, center_y) = ((chip.left + chip.right) / 2.0, (chip.top + chip.bottom) / 2.0);
    assert!(((first.top + first.bottom) / 2.0 - center_y).abs() <= 1.0);
    assert!(((second.top + second.bottom) / 2.0 - center_y).abs() <= 1.0);
    assert!((first.right - center_x).abs() <= 1.0);
    assert!((second.left - center_x).abs() <= 1.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, clickableInMinimumTouchTarget
#[test]
fn a_small_chip_keeps_its_size_and_is_clickable() {
    let clicked = Rc::new(Cell::new(false));
    let heard = clicked.clone();
    let mut ui = rule(move || {
        let heard = heard.clone();
        Box!(modifier = Modifier.fill_max_size(), |scope: BoxScope| {
            let heard = heard.clone();
            SuggestionChip!(
                move || heard.set(true),
                || Box!(modifier = Modifier.size(10.dp())),
                modifier = Modifier
                    .align(scope, compose_ui::Alignment::CENTER)
                    .test_tag(TAG)
                    .required_size(10.dp())
            );
        });
    });
    // Compose's touch bounds of 48 dp and a click at (-1, -1) wait on hit-test-extensions
    // (docs/parity/ui-input.md): the 10 dp chip takes one on itself here.
    ui.on_node_with_tag(TAG).assert_width_is_equal_to(10.dp()).assert_height_is_equal_to(10.dp());
    ui.on_node_with_tag(TAG).perform_click();
    assert!(clicked.get());
}

// ---- Arrangements of the shapes overloads -----------------------------------------------------

/// With `shapes`, spacing is compact beside an icon: nothing changes without one.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, expressiveFilterChip_arrangement_noIcons
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, expressiveFilterChip_arrangement_leadingIcon
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, expressiveFilterChip_arrangement_trailingIcon
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, expressiveFilterChip_arrangement_bothIcons
#[test]
fn a_shaped_filter_chips_spacing_is_compact_beside_an_icon() {
    let label = || Text!("Label", modifier = Modifier.test_tag("label"));
    let none = rule(move || {
        FilterChip!(false, || {}, label, shapes = FilterChipDefaults::shapes());
    });
    let mut none = none;
    assert_eq!(left(&mut none, "label"), 16.0);
    let mut leading = rule(move || {
        FilterChip!(
            false,
            || {},
            label,
            shapes = FilterChipDefaults::shapes(),
            leading_icon = Some(|| Icon18("leading"))
        );
    });
    assert_eq!(left(&mut leading, "leading"), 8.0);
    assert_eq!(left(&mut leading, "label"), 8.0 + 18.0 + 4.0);
    let mut trailing = rule(move || {
        FilterChip!(
            false,
            || {},
            label,
            shapes = FilterChipDefaults::shapes(),
            trailing_icon = Some(|| Icon18("trailing"))
        );
    });
    // No icon before it: the second spacing, 4 dp.
    assert_eq!(left(&mut trailing, "label"), 8.0 + 4.0);
    let mut both = rule(move || {
        FilterChip!(
            false,
            || {},
            label,
            shapes = FilterChipDefaults::shapes(),
            leading_icon = Some(|| Icon18("leading")),
            trailing_icon = Some(|| Icon18("trailing"))
        );
    });
    assert_eq!(left(&mut both, "label"), 8.0 + 18.0 + 4.0);
    assert_eq!(
        left(&mut both, "trailing"),
        left(&mut both, "label") + width(&mut both, "label") + 4.0
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, expressiveFilterChip_largeWidth_trailingIconPlacement
#[test]
fn a_wide_shaped_filter_chip_keeps_its_trailing_icon_at_its_end() {
    let mut ui = rule(|| {
        FilterChip!(
            false,
            || {},
            || Text!("Label"),
            modifier = Modifier.width(200.dp()),
            shapes = FilterChipDefaults::shapes(),
            leading_icon = Some(|| Icon18("leading")),
            trailing_icon = Some(|| Icon18("trailing"))
        );
    });
    assert_eq!(left(&mut ui, "leading"), 8.0);
    assert_eq!(left(&mut ui, "trailing"), 200.0 - 8.0 - 18.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, expressiveInputChip_arrangement_noIcons
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, expressiveInputChip_arrangement_leadingIcon
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, expressiveInputChip_arrangement_avatar
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, expressiveInputChip_arrangement_trailingIcon
#[test]
fn a_shaped_input_chips_spacing_is_compact_beside_an_icon() {
    let label = || Text!("Label", modifier = Modifier.test_tag("label"));
    let mut none = rule(move || {
        InputChip!(false, || {}, label, shapes = InputChipDefaults::shapes());
    });
    assert_eq!(left(&mut none, "label"), 4.0 + 8.0);
    let mut leading = rule(move || {
        InputChip!(
            false,
            || {},
            label,
            shapes = InputChipDefaults::shapes(),
            leading_icon = Some(|| Icon18("leading"))
        );
    });
    assert_eq!(left(&mut leading, "leading"), 8.0);
    assert_eq!(left(&mut leading, "label"), 8.0 + 18.0 + 4.0);
    let mut avatar = rule(move || {
        InputChip!(
            false,
            || {},
            label,
            shapes = InputChipDefaults::shapes(),
            avatar = Some(|| Box!(modifier = Modifier.size(24.dp()).test_tag("avatar")))
        );
    });
    assert_eq!(left(&mut avatar, "avatar"), 4.0);
    assert_eq!(left(&mut avatar, "label"), 4.0 + 24.0 + 4.0);
    let mut trailing = rule(move || {
        InputChip!(
            false,
            || {},
            label,
            shapes = InputChipDefaults::shapes(),
            trailing_icon = Some(|| Icon18("trailing"))
        );
    });
    assert_eq!(left(&mut trailing, "label"), 4.0 + 4.0);
}

// ---- Colors -----------------------------------------------------------------------------------

/// The content color at the label.
fn label_color(content: impl Fn(Rc<Cell<Color>>) + 'static) -> Color {
    let seen = Rc::new(Cell::new(Color::UNSPECIFIED));
    let write = seen.clone();
    let _ui = rule(move || content(write.clone()));
    seen.get()
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, labelContentColor_assistChip
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, labelContentColor_unselectedFilterChip
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, labelContentColor_selectedFilterChip
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, labelContentColor_inputChip
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, labelContentColor_suggestionChip
#[test]
fn a_labels_content_color_is_the_tokens() {
    let scheme = light_color_scheme();
    let assist = label_color(|seen| {
        AssistChip!(|| {}, move || seen.set(LOCAL_CONTENT_COLOR.current()));
    });
    assert_eq!(assist, scheme.on_surface);
    let filter = label_color(|seen| {
        FilterChip!(false, || {}, move || seen.set(LOCAL_CONTENT_COLOR.current()));
    });
    assert_eq!(filter, scheme.on_surface_variant);
    let selected = label_color(|seen| {
        FilterChip!(true, || {}, move || seen.set(LOCAL_CONTENT_COLOR.current()));
    });
    assert_eq!(selected, scheme.on_secondary_container);
    let input = label_color(|seen| {
        InputChip!(false, || {}, move || seen.set(LOCAL_CONTENT_COLOR.current()));
    });
    assert_eq!(input, scheme.on_surface_variant);
    let suggestion = label_color(|seen| {
        SuggestionChip!(|| {}, move || seen.set(LOCAL_CONTENT_COLOR.current()));
    });
    assert_eq!(suggestion, scheme.on_surface_variant);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, defaultColors_elevatedFilterChip
#[test]
fn the_defaults_are_the_tokens() {
    let seen = Rc::new(Cell::new(None));
    let write = seen.clone();
    let _ui = rule(move || {
        write.set(Some((
            FilterChipDefaults::elevated_filter_chip_colors(),
            InputChipDefaults::input_chip_colors(),
            AssistChipDefaults::assist_chip_colors(),
            SuggestionChipDefaults::elevated_suggestion_chip_colors(),
        )));
    });
    let (elevated, input, assist, suggestion) = seen.get().expect("composed");
    let scheme = light_color_scheme();
    let faint = scheme.on_surface.with_alpha(0.12);
    let dim = scheme.on_surface.with_alpha(0.38);
    assert_eq!(
        elevated,
        SelectableChipColors {
            container_color: scheme.surface_container_low,
            label_color: scheme.on_surface_variant,
            leading_icon_color: scheme.primary,
            trailing_icon_color: scheme.on_surface_variant,
            disabled_container_color: faint,
            disabled_label_color: dim,
            disabled_leading_icon_color: dim,
            disabled_trailing_icon_color: dim,
            selected_container_color: scheme.secondary_container,
            disabled_selected_container_color: faint,
            selected_label_color: scheme.on_secondary_container,
            selected_leading_icon_color: scheme.on_secondary_container,
            selected_trailing_icon_color: scheme.on_secondary_container,
        }
    );
    assert_eq!(
        input,
        SelectableChipColors {
            container_color: Color::TRANSPARENT,
            label_color: scheme.on_surface_variant,
            leading_icon_color: scheme.on_surface_variant,
            trailing_icon_color: scheme.on_surface_variant,
            disabled_container_color: Color::TRANSPARENT,
            disabled_label_color: dim,
            disabled_leading_icon_color: dim,
            disabled_trailing_icon_color: dim,
            selected_container_color: scheme.secondary_container,
            disabled_selected_container_color: faint,
            selected_label_color: scheme.on_secondary_container,
            selected_leading_icon_color: scheme.primary,
            selected_trailing_icon_color: scheme.on_secondary_container,
        }
    );
    assert_eq!(
        assist,
        ChipColors {
            container_color: Color::TRANSPARENT,
            label_color: scheme.on_surface,
            leading_icon_content_color: scheme.primary,
            trailing_icon_content_color: scheme.primary,
            disabled_container_color: Color::TRANSPARENT,
            disabled_label_color: dim,
            disabled_leading_icon_content_color: dim,
            disabled_trailing_icon_content_color: dim,
        }
    );
    assert_eq!(
        suggestion,
        ChipColors {
            container_color: scheme.surface_container_low,
            label_color: scheme.on_surface_variant,
            leading_icon_content_color: scheme.primary,
            trailing_icon_content_color: Color::UNSPECIFIED,
            disabled_container_color: faint,
            disabled_label_color: dim,
            disabled_leading_icon_content_color: dim,
            disabled_trailing_icon_content_color: Color::UNSPECIFIED,
        }
    );
}

/// Each `ColorScheme` role for each state, as `SelectableChipColors`' functions.
#[test]
fn a_selectable_chips_colors_go_by_its_state() {
    let colors = SelectableChipColors {
        container_color: Color::from_argb(0xFF00_0001),
        label_color: Color::from_argb(0xFF00_0002),
        leading_icon_color: Color::from_argb(0xFF00_0003),
        trailing_icon_color: Color::from_argb(0xFF00_0004),
        disabled_container_color: Color::from_argb(0xFF00_0005),
        disabled_label_color: Color::from_argb(0xFF00_0006),
        disabled_leading_icon_color: Color::from_argb(0xFF00_0007),
        disabled_trailing_icon_color: Color::from_argb(0xFF00_0008),
        selected_container_color: Color::from_argb(0xFF00_0009),
        disabled_selected_container_color: Color::from_argb(0xFF00_000A),
        selected_label_color: Color::from_argb(0xFF00_000B),
        selected_leading_icon_color: Color::from_argb(0xFF00_000C),
        selected_trailing_icon_color: Color::from_argb(0xFF00_000D),
    };
    assert_eq!(colors.container_color(true, false), colors.container_color);
    assert_eq!(colors.container_color(true, true), colors.selected_container_color);
    assert_eq!(colors.container_color(false, false), colors.disabled_container_color);
    assert_eq!(colors.container_color(false, true), colors.disabled_selected_container_color);
    assert_eq!(colors.label_color(true, false), colors.label_color);
    assert_eq!(colors.label_color(true, true), colors.selected_label_color);
    assert_eq!(colors.label_color(false, true), colors.disabled_label_color);
    assert_eq!(colors.leading_icon_content_color(true, false), colors.leading_icon_color);
    assert_eq!(colors.leading_icon_content_color(true, true), colors.selected_leading_icon_color);
    assert_eq!(colors.leading_icon_content_color(false, true), colors.disabled_leading_icon_color);
    assert_eq!(colors.trailing_icon_content_color(true, false), colors.trailing_icon_color);
    assert_eq!(colors.trailing_icon_content_color(true, true), colors.selected_trailing_icon_color);
    assert_eq!(
        colors.trailing_icon_content_color(false, true),
        colors.disabled_trailing_icon_color
    );
}

// ---- The goldens, as the display list ---------------------------------------------------------

/// The chip's background and border: the first filled color, and the stroke's color and width.
fn drawn(ui: &mut ComposeTestRule) -> (Option<Color>, Option<(Color, f32)>) {
    let mut fill = None;
    let mut stroke = None;
    for op in &ui.host().display_list().ops {
        let (paint, is_container) = match op {
            DrawOp::DrawRoundRect(_, _, paint) | DrawOp::DrawRect(_, paint) => (paint, true),
            DrawOp::DrawPath { paint, .. } => (paint, false),
            _ => continue,
        };
        match paint.style {
            compose_ui::PaintStyle::Stroke(width) => {
                stroke = stroke.or(Some((paint.color, width)));
            }
            compose_ui::PaintStyle::Fill => {
                if is_container {
                    fill = fill.or(Some(paint.color));
                }
            }
        }
    }
    (fill, stroke)
}

fn the_shadow(ui: &mut ComposeTestRule) -> f32 {
    let list = ui.host().display_list();
    list.groups
        .iter()
        .filter_map(|group| group.layer.as_ref())
        .map(|it| it.shadow_elevation)
        .fold(0.0, f32::max)
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipScreenshotTest.kt, assistChip_flat_lightTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipScreenshotTest.kt, assistChip_flat_disabled_lightTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipScreenshotTest.kt, suggestionChip_flat_lightTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipScreenshotTest.kt, suggestionChip_flat_disabled_lightTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipScreenshotTest.kt, filterChip_flat_notSelected
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipScreenshotTest.kt, filterChip_flat_disabled_notSelected
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipScreenshotTest.kt, inputChip_lightTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipScreenshotTest.kt, inputChip_disabled_lightTheme
#[test]
fn a_flat_chip_is_an_outline() {
    let scheme = light_color_scheme();
    let check = |enabled: bool, mut ui: ComposeTestRule| {
        let color =
            if enabled { scheme.outline_variant } else { scheme.on_surface.with_alpha(0.12) };
        let (fill, stroke) = drawn(&mut ui);
        assert_eq!(fill, None, "no container");
        assert_eq!(stroke, Some((color, 1.0)));
    };
    for enabled in [true, false] {
        check(enabled, rule(move || AssistChip!(|| {}, || Text!("a"), enabled = enabled)));
        check(enabled, rule(move || SuggestionChip!(|| {}, || Text!("a"), enabled = enabled)));
        check(enabled, rule(move || FilterChip!(false, || {}, || Text!("a"), enabled = enabled)));
        check(enabled, rule(move || InputChip!(false, || {}, || Text!("a"), enabled = enabled)));
    }
}

/// Selected: the secondary container, disabled faint, and no outline to see (a hairline in
/// transparent, as Compose's `BorderStroke(0.dp, Color.Transparent)`).
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipScreenshotTest.kt, filterChip_flat_selected_lightTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipScreenshotTest.kt, filterChip_flat_disabled_selected
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipScreenshotTest.kt, inputChip_selected_lightTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipScreenshotTest.kt, inputChip_disabled_selected_lightTheme
#[test]
fn a_selected_flat_chip_is_filled() {
    let scheme = light_color_scheme();
    let check = |enabled: bool, mut ui: ComposeTestRule| {
        let (fill, stroke) = drawn(&mut ui);
        let color =
            if enabled { scheme.secondary_container } else { scheme.on_surface.with_alpha(0.12) };
        assert_eq!(fill, Some(color));
        assert!(stroke.is_none_or(|(color, _)| color == Color::TRANSPARENT), "{stroke:?}");
    };
    for enabled in [true, false] {
        check(enabled, rule(move || FilterChip!(true, || {}, || Text!("a"), enabled = enabled)));
        check(enabled, rule(move || InputChip!(true, || {}, || Text!("a"), enabled = enabled)));
    }
}

/// Elevated: a container and a 1 dp shadow, none disabled, no outline.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipScreenshotTest.kt, assistChip_elevated_lightTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipScreenshotTest.kt, assistChip_elevated_disabled_lightTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipScreenshotTest.kt, suggestionChip_elevated_lightTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipScreenshotTest.kt, suggestionChip_elevated_disabled_lightTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipScreenshotTest.kt, filterChip_elevated_selected_darkTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, elevatedDisabled_assistChip
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipTest.kt, elevatedDisabled_suggestionChip
#[test]
fn an_elevated_chip_has_a_container_and_a_shadow() {
    let scheme = light_color_scheme();
    let check = |enabled: bool, color: Color, mut ui: ComposeTestRule| {
        let (fill, stroke) = drawn(&mut ui);
        assert_eq!(fill, Some(if enabled { color } else { scheme.on_surface.with_alpha(0.12) }));
        assert_eq!(stroke, None);
        assert_eq!(the_shadow(&mut ui), if enabled { 1.0 } else { 0.0 });
    };
    let low = scheme.surface_container_low;
    for enabled in [true, false] {
        check(
            enabled,
            low,
            rule(move || ElevatedAssistChip!(|| {}, || Text!("a"), enabled = enabled)),
        );
        check(
            enabled,
            low,
            rule(move || ElevatedSuggestionChip!(|| {}, || Text!("a"), enabled = enabled)),
        );
        check(
            enabled,
            low,
            rule(move || ElevatedFilterChip!(false, || {}, || Text!("a"), enabled = enabled)),
        );
        check(
            enabled,
            scheme.secondary_container,
            rule(move || ElevatedFilterChip!(true, || {}, || Text!("a"), enabled = enabled)),
        );
    }
}

/// `border`: none, or one's own.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Chip.kt, AssistChip
#[test]
fn a_chips_border_can_be_its_own_or_none() {
    let mut none = rule(|| AssistChip!(|| {}, || Text!("a"), border = None));
    assert_eq!(drawn(&mut none).1, None);
    let mut own = rule(|| {
        AssistChip!(|| {}, || Text!("a"), border = Some(BorderStroke::new(3.dp(), Color::RED)));
    });
    assert_eq!(drawn(&mut own).1, Some((Color::RED, 3.0)));
    let mut elevated = rule(|| {
        ElevatedAssistChip!(
            || {},
            || Text!("a"),
            border = Some(BorderStroke::new(3.dp(), Color::RED))
        );
    });
    assert_eq!(drawn(&mut elevated).1, Some((Color::RED, 3.0)));
}

/// `colors` and `shape` take the place of the defaults.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Chip.kt, FilterChip
#[test]
fn a_chips_colors_and_shape_are_its_own() {
    let mut ui = rule(|| {
        let colors = SelectableChipColors {
            selected_container_color: Color::RED,
            ..FilterChipDefaults::filter_chip_colors()
        };
        FilterChip!(true, || {}, || Text!("a"), colors = colors, shape = Shape::Rectangle);
    });
    assert_eq!(drawn(&mut ui).0, Some(Color::RED));
    assert!(ui.host().display_list().ops.iter().any(|op| matches!(op, DrawOp::DrawRect(..))));
}

/// Its elevation by its state: the hover's, and none at all with `elevation = None`.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Chip.kt, SelectableChipElevation
#[test]
fn a_chips_shadow_follows_its_interactions() {
    use compose_ui::{Press, PressInteraction};
    let source = compose_ui::MutableInteractionSource::new();
    let chip_source = source.clone();
    let mut ui = rule(move || {
        ElevatedFilterChip!(
            false,
            || {},
            || Text!("a"),
            elevation = Some(ChipElevation {
                elevation: 2.dp(),
                pressed_elevation: 5.dp(),
                focused_elevation: 7.dp(),
                hovered_elevation: 9.dp(),
                dragged_elevation: 11.dp(),
                disabled_elevation: 0.dp(),
            }),
            interaction_source = Some(chip_source.clone())
        );
    });
    assert_eq!(the_shadow(&mut ui), 2.0);
    let press = Press::new(compose_ui::Offset::ZERO);
    source.try_emit(PressInteraction::Press(press));
    ui.wait_for_idle();
    assert_eq!(the_shadow(&mut ui), 5.0);
    source.try_emit(PressInteraction::Release(press));
    ui.wait_for_idle();
    assert_eq!(the_shadow(&mut ui), 2.0);
    let start = compose_ui::DragStart::new();
    source.try_emit(compose_ui::DragInteraction::Start(start));
    ui.wait_for_idle();
    assert_eq!(the_shadow(&mut ui), 11.0);
    source.try_emit(compose_ui::DragInteraction::Stop(start));
    ui.wait_for_idle();
    assert_eq!(the_shadow(&mut ui), 2.0);
    let mut flat = rule(|| ElevatedFilterChip!(false, || {}, || Text!("a"), elevation = None));
    assert_eq!(the_shadow(&mut flat), 0.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipScreenshotTest.kt, inputChip_withAvatar_lightTheme
#[test]
fn an_input_chips_slots_are_in_their_colors() {
    thread_local! {
        static COLORS: Cell<[Color; 3]> = const { Cell::new([Color::UNSPECIFIED; 3]) };
    }
    let write = |index: usize| {
        let mut colors = COLORS.get();
        colors[index] = LOCAL_CONTENT_COLOR.current();
        COLORS.set(colors);
    };
    let scheme = light_color_scheme();
    let chip = move |selected: bool, enabled: bool| {
        rule(move || {
            InputChip!(
                selected,
                || {},
                move || write(0),
                enabled = enabled,
                leading_icon = Some(move || write(1)),
                trailing_icon = Some(move || write(2))
            );
        })
    };
    chip(false, true);
    assert_eq!(
        COLORS.get(),
        [scheme.on_surface_variant, scheme.on_surface_variant, scheme.on_surface_variant]
    );
    chip(true, true);
    assert_eq!(
        COLORS.get(),
        [scheme.on_secondary_container, scheme.primary, scheme.on_secondary_container]
    );
    chip(true, false);
    assert_eq!(COLORS.get(), [scheme.on_surface.with_alpha(0.38); 3]);
}

/// A corner of the background.
fn corner(ui: &mut ComposeTestRule) -> f32 {
    ui.host()
        .display_list()
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::DrawRoundRect(_, radii, paint)
                if matches!(paint.style, compose_ui::PaintStyle::Fill) =>
            {
                Some(radii[0])
            }
            _ => None,
        })
        .expect("a filled background")
}

/// `shapes`: the shape for the state, once it's settled, the corners moving to it; a press's over
/// selection.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipScreenshotTest.kt, filterChip_shapes_selected_lightTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipScreenshotTest.kt, filterChip_shapes_notSelected_lightTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipScreenshotTest.kt, elevatedFilterChip_shapes_selected_darkTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipScreenshotTest.kt, elevatedFilterChip_shapes_notSelected_darkTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipScreenshotTest.kt, inputChip_shapes_selected_lightTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ChipScreenshotTest.kt, inputChip_shapes_notSelected_lightTheme
#[test]
fn a_chips_shapes_go_by_its_state() {
    use compose_ui::{Press, PressInteraction};
    let round = |dp: f32| Shape::Rounded([CornerSize::Dp(dp.dp()); 4]);
    let shapes =
        ChipShapes { shape: round(2.0), selected_shape: round(6.0), pressed_shape: round(10.0) };
    let selected = mutable_state_of(false);
    let read = selected.handle();
    let source = compose_ui::MutableInteractionSource::new();
    let chip_source = source.clone();
    let chip_shapes = shapes.clone();
    let mut ui = rule(move || {
        let colors = SelectableChipColors {
            container_color: Color::BLUE,
            ..FilterChipDefaults::filter_chip_colors()
        };
        FilterChip!(
            read.get(),
            || {},
            || Text!("a"),
            shapes = chip_shapes.clone(),
            colors = colors,
            interaction_source = Some(chip_source.clone())
        );
    });
    assert_eq!(corner(&mut ui), 2.0);
    selected.set(true);
    ui.wait_for_idle();
    assert_eq!(corner(&mut ui), 6.0);
    let press = Press::new(compose_ui::Offset::ZERO);
    source.try_emit(PressInteraction::Press(press));
    ui.wait_for_idle();
    assert_eq!(corner(&mut ui), 10.0);
    source.try_emit(PressInteraction::Release(press));
    ui.wait_for_idle();
    assert_eq!(corner(&mut ui), 6.0);
}

/// The defaults: medium, full and small, whichever the colors are the tonal ones.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Chip.kt, defaultChipShapes
#[test]
fn the_default_shapes_and_tonal_colors_are_the_tokens() {
    let seen = Rc::new(std::cell::RefCell::new(None));
    let write = seen.clone();
    let _ui = rule(move || {
        *write.borrow_mut() = Some((
            FilterChipDefaults::shapes(),
            InputChipDefaults::shapes(),
            MaterialTheme::shapes().medium.clone(),
            MaterialTheme::shapes().small.clone(),
            FilterChipDefaults::tonal_filter_chip_colors(),
            FilterChipDefaults::tonal_elevated_filter_chip_colors(),
            InputChipDefaults::tonal_input_chip_colors(),
            FilterChipDefaults::shape(),
        ));
    });
    let seen = seen.borrow();
    let (filter, input, medium, small, tonal, elevated, tonal_input, shape) =
        seen.as_ref().expect("composed");
    assert_eq!(filter, input);
    assert_eq!(filter.shape, *medium);
    assert_eq!(filter.selected_shape, compose_foundation::CIRCLE_SHAPE);
    assert_eq!(filter.pressed_shape, *small);
    assert_eq!(*shape, *small);
    let scheme = light_color_scheme();
    assert_eq!(tonal.leading_icon_color, scheme.on_surface_variant);
    assert_eq!(elevated.leading_icon_color, scheme.on_surface_variant);
    assert_eq!(elevated.container_color, scheme.surface_container_low);
    assert_eq!(tonal_input.leading_icon_color, scheme.on_surface_variant);
}

/// Right to left, the ends swap, a wide chip's trailing icon at the left.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Chip.kt, ChipArrangement
#[test]
fn right_to_left_a_chips_ends_swap() {
    use compose_ui::{LOCAL_LAYOUT_DIRECTION, LayoutDirection};
    let mut ui = rule(|| {
        CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Rtl), || {
            Box!(modifier = Modifier.fill_max_size(), {
                AssistChip!(
                    || {},
                    || Text!("Label", modifier = Modifier.test_tag("label")),
                    modifier = Modifier.width(200.dp()),
                    leading_icon = Some(|| Icon18("leading")),
                    trailing_icon = Some(|| Icon18("trailing"))
                );
            });
        });
    });
    assert_eq!(left(&mut ui, "leading"), 400.0 - 8.0 - 18.0);
    assert_eq!(left(&mut ui, "trailing"), 200.0 + 8.0);
}

/// A row is as wide as the two spacings' mean twice: 4 and 8 compact beside a leading icon.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Chip.kt, ChipArrangement
#[test]
fn a_chips_width_has_both_spacings() {
    let mut ui = rule(|| {
        FilterChip!(
            false,
            || {},
            || Text!("Label", modifier = Modifier.test_tag("label")),
            modifier = Modifier.test_tag(TAG),
            shapes = FilterChipDefaults::shapes(),
            leading_icon = Some(|| Icon18("leading"))
        );
    });
    assert_eq!(width(&mut ui, TAG), 8.0 + 18.0 + 4.0 + width(&mut ui, "label") + 8.0 + 8.0);
}

/// An input chip's padding at its end is 8 with a trailing icon.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Chip.kt, inputChipPadding
#[test]
fn an_input_chips_trailing_icon_is_8_dp_from_its_end() {
    let mut ui = rule(|| {
        InputChip!(
            false,
            || {},
            || Text!("Label"),
            modifier = Modifier.test_tag(TAG),
            trailing_icon = Some(|| Icon18("trailing"))
        );
    });
    assert_eq!(width(&mut ui, TAG) - left(&mut ui, "trailing") - 18.0, 8.0);
}

/// Flat, a filter chip rises by 1 dp on a hover, any chip by 8 dp on a drag; elevated, by 3 on a hover.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Chip.kt, filterChipElevation
#[test]
fn the_default_elevations_are_the_tokens() {
    use compose_ui::{DragInteraction, DragStart, HoverEnter, HoverInteraction};
    let hovered = |chip: fn(compose_ui::MutableInteractionSource)| {
        let source = compose_ui::MutableInteractionSource::new();
        let hoisted = source.clone();
        let mut ui = rule(move || chip(hoisted.clone()));
        source.try_emit(HoverInteraction::Enter(HoverEnter::new()));
        ui.wait_for_idle();
        the_shadow(&mut ui)
    };
    assert_eq!(
        hovered(|source| {
            FilterChip!(false, || {}, || Text!("a"), interaction_source = Some(source));
        }),
        1.0
    );
    assert_eq!(
        hovered(|source| {
            ElevatedFilterChip!(false, || {}, || Text!("a"), interaction_source = Some(source));
        }),
        3.0
    );
    assert_eq!(
        hovered(|source| {
            AssistChip!(|| {}, || Text!("a"), interaction_source = Some(source));
        }),
        0.0
    );
    let source = compose_ui::MutableInteractionSource::new();
    let hoisted = source.clone();
    let mut ui = rule(move || {
        AssistChip!(|| {}, || Text!("a"), interaction_source = Some(hoisted.clone()));
    });
    source.try_emit(DragInteraction::Start(DragStart::new()));
    ui.wait_for_idle();
    assert_eq!(the_shadow(&mut ui), 8.0);
}

/// With `shapes`, the colors are the tonal ones: the leading icon not in the primary color.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Chip.kt, tonalFilterChipColors
#[test]
fn a_shaped_chips_colors_are_tonal() {
    thread_local! {
        static COLOR: Cell<Color> = const { Cell::new(Color::UNSPECIFIED) };
    }
    let scheme = light_color_scheme();
    let _plain = rule(|| {
        FilterChip!(
            false,
            || {},
            || Text!("a"),
            leading_icon = Some(|| COLOR.set(LOCAL_CONTENT_COLOR.current()))
        );
    });
    assert_eq!(COLOR.get(), scheme.primary);
    let _shaped = rule(|| {
        FilterChip!(
            false,
            || {},
            || Text!("a"),
            shapes = FilterChipDefaults::shapes(),
            leading_icon = Some(|| COLOR.set(LOCAL_CONTENT_COLOR.current()))
        );
    });
    assert_eq!(COLOR.get(), scheme.on_surface_variant);
    let _elevated = rule(|| {
        ElevatedFilterChip!(
            false,
            || {},
            || Text!("a"),
            shapes = FilterChipDefaults::shapes(),
            leading_icon = Some(|| COLOR.set(LOCAL_CONTENT_COLOR.current()))
        );
    });
    assert_eq!(COLOR.get(), scheme.on_surface_variant);
}

/// A plain chip's ends each in their color.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Chip.kt, Chip
#[test]
fn a_plain_chips_ends_are_in_their_colors() {
    thread_local! {
        static COLORS: Cell<(Color, Color)> = const { Cell::new((Color::RED, Color::RED)) };
    }
    let _ui = rule(|| {
        let colors = ChipColors {
            leading_icon_content_color: Color::GREEN,
            trailing_icon_content_color: Color::BLUE,
            ..AssistChipDefaults::assist_chip_colors()
        };
        AssistChip!(
            || {},
            || Text!("a"),
            colors = colors,
            leading_icon = Some(|| COLORS.set((LOCAL_CONTENT_COLOR.current(), COLORS.get().1))),
            trailing_icon = Some(|| COLORS.set((COLORS.get().0, LOCAL_CONTENT_COLOR.current())))
        );
    });
    assert_eq!(COLORS.get(), (Color::GREEN, Color::BLUE));
}

/// The last interaction to begin wins, not a fixed order: a hover, then a focus, is the focus's,
/// and the hover's again when it ends.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Chip.kt, animateElevation
#[test]
fn the_last_interaction_to_begin_wins() {
    use compose_ui::{Focus, FocusInteraction, HoverEnter, HoverInteraction};
    let source = compose_ui::MutableInteractionSource::new();
    let hoisted = source.clone();
    let mut ui = rule(move || {
        ElevatedFilterChip!(
            false,
            || {},
            || Text!("a"),
            elevation = Some(ChipElevation {
                elevation: 2.dp(),
                pressed_elevation: 5.dp(),
                focused_elevation: 7.dp(),
                hovered_elevation: 9.dp(),
                dragged_elevation: 11.dp(),
                disabled_elevation: 0.dp(),
            }),
            interaction_source = Some(hoisted.clone())
        );
    });
    let enter = HoverEnter::new();
    source.try_emit(HoverInteraction::Enter(enter));
    ui.wait_for_idle();
    assert_eq!(the_shadow(&mut ui), 9.0);
    let focus = Focus::new();
    source.try_emit(FocusInteraction::Focus(focus));
    ui.wait_for_idle();
    assert_eq!(the_shadow(&mut ui), 7.0);
    // The focus over, the hover's again.
    source.try_emit(FocusInteraction::Unfocus(focus));
    ui.wait_for_idle();
    assert_eq!(the_shadow(&mut ui), 9.0);
    source.try_emit(HoverInteraction::Exit(enter));
    ui.wait_for_idle();
    assert_eq!(the_shadow(&mut ui), 2.0);
}

/// Disabled, the shadow's at the disabled elevation at once, not by the way back from a press.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Chip.kt, animateElevation
#[test]
fn disabling_a_chip_snaps_its_shadow() {
    use compose_ui::{Press, PressInteraction};
    let enabled = mutable_state_of(true);
    let read = enabled.handle();
    let source = compose_ui::MutableInteractionSource::new();
    let hoisted = source.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let hoisted = hoisted.clone();
        MaterialTheme!({
            ElevatedAssistChip!(
                || {},
                || Text!("a"),
                enabled = read.get(),
                elevation = Some(ChipElevation {
                    elevation: 2.dp(),
                    pressed_elevation: 5.dp(),
                    focused_elevation: 2.dp(),
                    hovered_elevation: 2.dp(),
                    dragged_elevation: 2.dp(),
                    disabled_elevation: 0.dp(),
                }),
                interaction_source = Some(hoisted.clone())
            );
        });
    });
    source.try_emit(PressInteraction::Press(Press::new(compose_ui::Offset::ZERO)));
    ui.advance_time_by(std::time::Duration::from_millis(500));
    assert_eq!(the_shadow(&mut ui), 5.0);
    enabled.set(false);
    // A frame to compose it, a frame for its effect: not the 150 ms of the way back.
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert_eq!(the_shadow(&mut ui), 0.0);
}

/// An icon that stays takes the chip's new colors: it isn't the one kept from before.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Chip.kt, rememberRetainedState
#[test]
fn an_icons_color_follows_the_state_while_its_there() {
    thread_local! {
        static COLOR: Cell<Color> = const { Cell::new(Color::UNSPECIFIED) };
    }
    let selected = mutable_state_of(false);
    let read = selected.handle();
    let mut ui = rule(move || {
        FilterChip!(
            read.get(),
            || {},
            || Text!("a"),
            leading_icon = Some(|| COLOR.set(LOCAL_CONTENT_COLOR.current()))
        );
    });
    let scheme = light_color_scheme();
    assert_eq!(COLOR.get(), scheme.primary);
    selected.set(true);
    ui.wait_for_idle();
    assert_eq!(COLOR.get(), scheme.on_secondary_container);
}

/// An icon there from the start is simply there on the first frame.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Chip.kt, AnimatingChipContent
#[test]
fn an_icon_there_at_first_doesnt_animate_in() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {
        MaterialTheme!({
            FilterChip!(
                false,
                || {},
                || Text!("Label", modifier = Modifier.test_tag("label")),
                leading_icon = Some(|| Icon18("leading"))
            );
        });
    });
    assert_eq!(left(&mut ui, "label"), 34.0);
    assert_eq!(the_shadow(&mut ui), 0.0);
    let list = ui.host().display_list();
    let faded =
        list.groups.iter().filter_map(|group| group.layer.as_ref()).any(|it| it.alpha < 1.0);
    assert!(!faded);
}

/// `flat` raises all the states but the drag's, as `assistChipElevation(elevation = 4.dp)`.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Chip.kt, assistChipElevation
#[test]
fn a_flat_elevation_raises_every_state() {
    assert_eq!(
        ChipElevation::flat(4.dp()),
        ChipElevation {
            elevation: 4.dp(),
            pressed_elevation: 4.dp(),
            focused_elevation: 4.dp(),
            hovered_elevation: 4.dp(),
            dragged_elevation: 8.dp(),
            disabled_elevation: 4.dp(),
        }
    );
}

/// The borders' colors by state and their widths.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Chip.kt, assistChipBorder
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Chip.kt, filterChipBorder
#[test]
fn a_borders_parameters_go_by_the_state() {
    let (a, b, c, d) = (Color::RED, Color::GREEN, Color::BLUE, Color::BLACK);
    for border in [
        AssistChipDefaults::assist_chip_border_with,
        SuggestionChipDefaults::suggestion_chip_border_with,
    ] {
        assert_eq!(border(true, a, b, 2.dp()), BorderStroke::new(2.dp(), a));
        assert_eq!(border(false, a, b, 2.dp()), BorderStroke::new(2.dp(), b));
    }
    for border in
        [FilterChipDefaults::filter_chip_border_with, InputChipDefaults::input_chip_border_with]
    {
        let of = |enabled, selected| border(enabled, selected, a, b, c, d, 2.dp(), 3.dp());
        assert_eq!(of(true, false), BorderStroke::new(2.dp(), a));
        assert_eq!(of(true, true), BorderStroke::new(3.dp(), b));
        assert_eq!(of(false, false), BorderStroke::new(2.dp(), c));
        assert_eq!(of(false, true), BorderStroke::new(3.dp(), d));
    }
}
