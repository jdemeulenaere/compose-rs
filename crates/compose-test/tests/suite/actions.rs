//! Actions and dumps on nodes: `perform_scroll_to`, `print_to_string`, the state assertions.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    ClickableOptions, Color, LOCAL_LAYOUT_DIRECTION, LayoutDirection, Modifier,
    ProgressBarRangeInfo, Role, ToggleableState, UnitExt,
};

#[track_caller]
fn expect_error(action: impl FnOnce()) -> String {
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(action));
    let panic = failed.expect_err("an assertion error");
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|text| text.to_string()))
        .unwrap_or_default()
}

// ---- perform_scroll_to -----------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
enum Orientation {
    HorizontalLtr,
    HorizontalRtl,
    Vertical,
}

impl ComposeArg for Orientation {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
#[allow(clippy::enum_variant_names)] // Compose's names: where the target starts.
enum Start {
    FullyAfter,
    PartiallyAfter,
    EndAlignedIn,
    CenterAlignedIn,
    StartAlignedIn,
    PartiallyBefore,
    FullyBefore,
}

impl Start {
    const ALL: [Start; 7] = [
        Start::FullyAfter,
        Start::PartiallyAfter,
        Start::EndAlignedIn,
        Start::CenterAlignedIn,
        Start::StartAlignedIn,
        Start::PartiallyBefore,
        Start::FullyBefore,
    ];

    /// The scroll offset that puts the target there, in a small (80 px) or big (150 px)
    /// viewport.
    fn offset(self, small: bool) -> i32 {
        let (small_offset, big_offset) = match self {
            Start::FullyAfter => (0, 0),
            Start::PartiallyAfter => (150, 100),
            Start::EndAlignedIn => (220, 150),
            Start::CenterAlignedIn => (210, 175),
            Start::StartAlignedIn => (200, 200),
            Start::PartiallyBefore => (270, 250),
            Start::FullyBefore => (420, 350),
        };
        if small { small_offset } else { big_offset }
    }

    fn expects_scrolling(self) -> bool {
        !matches!(self, Start::EndAlignedIn | Start::CenterAlignedIn | Start::StartAlignedIn)
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Alignment {
    Left,
    Right,
    Top,
    Bottom,
    Center,
}

impl Alignment {
    fn reverse(self) -> Alignment {
        match self {
            Alignment::Left => Alignment::Right,
            Alignment::Right => Alignment::Left,
            Alignment::Top => Alignment::Bottom,
            Alignment::Bottom => Alignment::Top,
            Alignment::Center => Alignment::Center,
        }
    }
}

/// Five 100 px boxes in a scrolling row or column, the third tagged "target".
#[composable]
fn Boxes(orientation: Orientation, viewport: i32, state: ScrollState) {
    let direction = if orientation == Orientation::HorizontalRtl {
        LayoutDirection::Rtl
    } else {
        LayoutDirection::Ltr
    };
    CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), move || {
        let boxes = || {
            for (index, color) in
                [Color::BLUE, Color::RED, Color::from_argb(0xFFFFFF00), Color::GREEN, Color::CYAN]
                    .into_iter()
                    .enumerate()
            {
                let tag = if index == 2 { "target" } else { "box" };
                Box!(modifier = Modifier.size(100.dp()).background(color).test_tag(tag));
            }
        };
        if orientation == Orientation::Vertical {
            Column!(
                modifier = Modifier
                    .test_tag("container")
                    .required_size((100.dp(), viewport.dp()))
                    .vertical_scroll(state),
                {
                    boxes();
                }
            );
        } else {
            Row!(
                modifier = Modifier
                    .test_tag("container")
                    .required_size((viewport.dp(), 100.dp()))
                    .horizontal_scroll(state),
                {
                    boxes();
                }
            );
        }
    });
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/actions/ScrollToTest.kt, scrollToTarget (without reverseScrolling: compose-rs's scroll modifiers have none)
/// The target, before, partly before, inside, partly after or after a scroll viewport smaller
/// or bigger than it, in a column or a row (left to right or right to left), is scrolled the
/// least to show: aligned with the viewport's nearest edge, or not moved if it shows already.
#[test]
fn scrolling_to_a_node_brings_it_into_its_viewport() {
    for orientation in
        [Orientation::HorizontalLtr, Orientation::HorizontalRtl, Orientation::Vertical]
    {
        for small in [true, false] {
            for start in Start::ALL {
                let vertical = orientation == Orientation::Vertical;
                let mut expected = match start {
                    Start::FullyAfter | Start::PartiallyAfter | Start::EndAlignedIn => {
                        if vertical {
                            Alignment::Bottom
                        } else {
                            Alignment::Right
                        }
                    }
                    Start::FullyBefore | Start::PartiallyBefore | Start::StartAlignedIn => {
                        if vertical {
                            Alignment::Top
                        } else {
                            Alignment::Left
                        }
                    }
                    Start::CenterAlignedIn => Alignment::Center,
                };
                if start.expects_scrolling() && small {
                    expected = expected.reverse();
                }
                if orientation == Orientation::HorizontalRtl {
                    expected = expected.reverse();
                }
                let case = format!("{orientation:?}, small: {small}, {start:?}");
                let viewport = if small { 80 } else { 150 };
                let state = ScrollState::new(start.offset(small));
                let given = *state;
                let mut ui = compose_test(move || Boxes(orientation, viewport, given));
                let before = ui.on_node_with_tag("target").get_unclipped_bounds_in_root();
                ui.on_node_with_tag("target").perform_scroll_to();
                ui.wait_for_idle();
                let viewport = ui.on_node_with_tag("container").get_bounds_in_root();
                let after = ui.on_node_with_tag("target").get_unclipped_bounds_in_root();
                if start.expects_scrolling() {
                    assert_ne!(after, before, "{case}: the target must have scrolled");
                } else {
                    assert_eq!(after, before, "{case}: the target must not scroll");
                }
                let center = |left: f32, right: f32| (left + right) / 2.0;
                match expected {
                    Alignment::Left => assert_eq!(after.left, viewport.left, "{case}"),
                    Alignment::Right => assert_eq!(after.right, viewport.right, "{case}"),
                    Alignment::Top => assert_eq!(after.top, viewport.top, "{case}"),
                    Alignment::Bottom => assert_eq!(after.bottom, viewport.bottom, "{case}"),
                    Alignment::Center => {
                        let (a, b) = if vertical {
                            (
                                center(after.top.0, after.bottom.0),
                                center(viewport.top.0, viewport.bottom.0),
                            )
                        } else {
                            (
                                center(after.left.0, after.right.0),
                                center(viewport.left.0, viewport.right.0),
                            )
                        };
                        assert_eq!(a, b, "{case}");
                    }
                }
            }
        }
    }
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/actions/ScrollToTest.kt, scrollToTarget_autoAdvanceDisabled
#[test]
fn scrolling_to_a_node_needs_the_clock_to_advance_by_itself() {
    let state = ScrollState::new(0);
    let given = *state;
    let mut ui = compose_test(move || Boxes(Orientation::Vertical, 150, given));
    ui.set_auto_advance(false);
    let message = expect_error(|| {
        ui.on_node_with_tag("target").perform_scroll_to();
    });
    assert!(message.contains("mainClock.autoAdvance is set to false"), "{message}");
    ui.set_auto_advance(true);
    ui.on_node_with_tag("target").perform_scroll_to();
    ui.on_node_with_tag("target").assert_is_displayed();
    // Already shown: nothing to scroll, the clock needn't advance.
    ui.set_auto_advance(false);
    ui.on_node_with_tag("target").perform_scroll_to();
}

/// A node outside any scrolling container can't be scrolled to.
#[test]
fn scrolling_to_a_node_outside_a_scroll_fails() {
    let mut ui = compose_test(|| Box!(modifier = Modifier.size(10.dp()).test_tag("plain")));
    let message = expect_error(|| {
        ui.on_node_with_tag("plain").perform_scroll_to();
    });
    assert!(message.contains("no parent with a scroll action"), "{message}");
}

// ---- print_to_string ------------------------------------------------------------------------

/// The printout with numbers (ids, pixels) replaced by X, as Compose's `obfuscateNodesInfo`.
fn obfuscated(printed: &str) -> String {
    let mut out = String::new();
    let mut in_number = false;
    for character in printed.chars() {
        if character.is_ascii_digit() || (in_number && character == '.') {
            if !in_number {
                out.push('X');
                in_number = true;
            }
        } else {
            in_number = false;
            out.push(character);
        }
    }
    out
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/PrintToStringTest.kt, printToString_nothingFound
#[test]
fn printing_a_missing_node_fails() {
    let mut ui = compose_test(|| BasicText!("Hello"));
    let message = expect_error(|| {
        ui.on_node_with_text("Oops").print_to_string(usize::MAX);
    });
    assert!(message.contains("expected exactly 1 node with text \"Oops\", found 0"), "{message}");
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/PrintToStringTest.kt, printToString_one, printToString_many
#[test]
fn printing_nodes_lists_their_properties_and_relatives() {
    let mut ui = compose_test(|| {
        BasicText!("Hello");
        BasicText!("World");
    });
    let one = ui.on_node_with_text("Hello").print_to_string(0);
    assert_eq!(
        obfuscated(&one),
        "Printing with useUnmergedTree = 'false'\n\
         Node #X at (l=X, t=X, r=X, b=X)px\n\
         Text = '[Hello]'\n\
         Has X sibling"
    );
    let many = ui.on_root().on_children().print_to_string(0);
    assert_eq!(
        obfuscated(&many),
        "Printing with useUnmergedTree = 'false'\n\
         X) Node #X at (l=X, t=X, r=X, b=X)px\n\
         Text = '[Hello]'\n\
         Has X sibling\n\
         X) Node #X at (l=X, t=X, r=X, b=X)px\n\
         Text = '[World]'\n\
         Has X sibling"
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/PrintToStringTest.kt, printHierarchy
#[test]
fn printing_the_root_draws_the_hierarchy() {
    let mut ui = compose_test(|| {
        Column!(modifier = Modifier.semantics(|config| config.disabled()).test_tag("column"), {
            Box!(modifier = Modifier.semantics(|config| config.disabled()).test_tag("box"), {
                Box!(
                    modifier = Modifier.clickable_with(
                        ClickableOptions { role: Some(Role::Button), ..ClickableOptions::DEFAULT },
                        || {}
                    ),
                    {
                        BasicText!("Button");
                    }
                );
            });
            BasicText!("Hello");
        });
    });
    let printed = ui.on_root().print_to_string(usize::MAX);
    assert_eq!(
        obfuscated(&printed),
        "Printing with useUnmergedTree = 'false'\n\
         Node #X at (l=X, t=X, r=X, b=X)px\n \
         |-Node #X at (l=X, t=X, r=X, b=X)px, Tag: 'column'\n   \
         [Disabled]\n    \
         |-Node #X at (l=X, t=X, r=X, b=X)px, Tag: 'box'\n    \
         | [Disabled]\n    \
         |  |-Node #X at (l=X, t=X, r=X, b=X)px\n    \
         |    Focused = 'false'\n    \
         |    Role = 'Button'\n    \
         |    Text = '[Button]'\n    \
         |    Actions = [OnClick, RequestFocus]\n    \
         |    MergeDescendants = 'true'\n    \
         |-Node #X at (l=X, t=X, r=X, b=X)px\n      \
         Text = '[Hello]'"
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/PrintToStringTest.kt, printMultiple_withDepth
#[test]
fn printing_stops_at_the_depth_given() {
    let mut ui = compose_test(|| {
        Box!(modifier = Modifier.test_tag("tag1"), {
            Box!(modifier = Modifier.test_tag("tag11"), {
                Box!(modifier = Modifier.test_tag("tag111"));
            });
        });
        Box!(modifier = Modifier.test_tag("tag2"), {
            Box!(modifier = Modifier.test_tag("tag22"), {
                Box!(modifier = Modifier.test_tag("tag222"));
            });
        });
    });
    let printed = ui.on_root().on_children().print_to_string(1);
    assert_eq!(
        obfuscated(&printed),
        "Printing with useUnmergedTree = 'false'\n\
         X) Node #X at (l=X, t=X, r=X, b=X)px, Tag: 'tagX'\n \
         |-Node #X at (l=X, t=X, r=X, b=X)px, Tag: 'tagX'\n   \
         Has X child\n\
         X) Node #X at (l=X, t=X, r=X, b=X)px, Tag: 'tagX'\n \
         |-Node #X at (l=X, t=X, r=X, b=X)px, Tag: 'tagX'\n   \
         Has X child"
    );
}

// ---- State assertions ------------------------------------------------------------------------

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/AssertsTest.kt, assertIsOn_forCheckedElement_isOk, assertIsOn_forUncheckedElement_throwsError, assertIsOn_forNotToggleableElement_throwsError, assertIsOff_*, assertIsSelected_*, assertIsNotSelected_*
#[test]
fn toggle_and_selection_states_are_asserted() {
    let mut ui = compose_test(|| {
        Box!(
            modifier = Modifier
                .semantics(|config| config.set_toggleable_state(ToggleableState::On))
                .test_tag("on")
        );
        Box!(
            modifier = Modifier
                .semantics(|config| config.set_toggleable_state(ToggleableState::Off))
                .test_tag("off")
        );
        Box!(
            modifier = Modifier.semantics(|config| config.set_selected(true)).test_tag("selected")
        );
        Box!(
            modifier =
                Modifier.semantics(|config| config.set_selected(false)).test_tag("unselected")
        );
        Box!(modifier = Modifier.semantics(|config| config.set_focused(false)).test_tag("plain"));
    });
    ui.on_node_with_tag("on").assert_is_on().assert_is_toggleable();
    ui.on_node_with_tag("off").assert_is_off().assert_is_toggleable();
    expect_error(|| {
        ui.on_node_with_tag("off").assert_is_on();
    });
    expect_error(|| {
        ui.on_node_with_tag("on").assert_is_off();
    });
    expect_error(|| {
        ui.on_node_with_tag("plain").assert_is_on();
    });
    expect_error(|| {
        ui.on_node_with_tag("plain").assert_is_off();
    });
    ui.on_node_with_tag("selected").assert_is_selected().assert_is_selectable();
    ui.on_node_with_tag("unselected").assert_is_not_selected().assert_is_selectable();
    expect_error(|| {
        ui.on_node_with_tag("unselected").assert_is_selected();
    });
    expect_error(|| {
        ui.on_node_with_tag("selected").assert_is_not_selected();
    });
    expect_error(|| {
        ui.on_node_with_tag("plain").assert_is_selected();
    });
    expect_error(|| {
        ui.on_node_with_tag("plain").assert_is_not_selected();
    });
    ui.on_node_with_tag("plain").assert_is_not_focused();
}

/// Texts compare as sets, editable text included unless asked; content descriptions,
/// values and ranges too.
#[test]
fn texts_descriptions_values_and_ranges_are_asserted() {
    let mut ui = compose_test(|| {
        Box!(
            modifier = Modifier
                .semantics(|config| {
                    config.is_merging_semantics_of_descendants = true;
                    config.set_content_description("first");
                    config.set_state_description("half");
                    config.set_progress_bar_range_info(ProgressBarRangeInfo {
                        current: 0.5,
                        range: (0.0, 1.0),
                        steps: 0,
                    });
                })
                .test_tag("node"),
            {
                BasicText!("a");
                BasicText!("b");
                Box!(
                    modifier =
                        Modifier.semantics(|config| config.set_content_description("second"))
                );
            }
        );
    });
    ui.on_node_with_tag("node")
        .assert_text_equals(&["b", "a"])
        .assert_text_contains("a", false, false)
        .assert_text_contains("A", false, true)
        .assert_content_description_equals(&["second", "first"])
        .assert_content_description_contains("sec", true, false)
        .assert_value_equals("half")
        .assert_range_info_equals(ProgressBarRangeInfo {
            current: 0.5,
            range: (0.0, 1.0),
            steps: 0,
        })
        .assert_has_no_click_action();
    expect_error(|| {
        ui.on_node_with_tag("node").assert_text_equals(&["a"]);
    });
    expect_error(|| {
        ui.on_node_with_tag("node").assert_text_contains("c", true, false);
    });
    ui.on_node(has_text_with("A", false, true)).assert_exists();
    ui.on_node(has_text_exactly(&["a", "b"], false)).assert_exists();
    ui.on_node(has_content_description_exactly(&["first", "second"])).assert_exists();
}

/// compose-test draws nothing itself: without a renderer installed (compose-skia's tests
/// install one; this binary never does), a capture fails saying how to install one (D904).
#[test]
fn capturing_without_a_renderer_fails_saying_how_to_install_one() {
    let mut ui = compose_test(|| Box!(modifier = Modifier.size(10.dp()).test_tag("box")));
    let message = expect_error(|| {
        ui.on_node_with_tag("box").capture_to_image();
    });
    assert!(message.contains("set_capture_renderer(compose_skia::raster_colors)"), "{message}");
}
