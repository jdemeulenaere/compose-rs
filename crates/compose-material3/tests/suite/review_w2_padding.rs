//! Adversarial review tests for `PaddingValues::absolute` in `compose-material3` components
//! (`TopAppBar`, `ListItem`, `SplitButtonLeadingButton`, and `OutlinedTextField`) across LTR and RTL.

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::{Box, PaddingValues, WindowInsets};
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::Optimizations;
use compose_ui::{IntSize, LOCAL_LAYOUT_DIRECTION, LayoutDirection, Modifier, UnitExt};

#[test]
#[ignore = "review: TopAppBar, ListItem, SplitButton, and OutlinedTextField read PaddingValues.start/end directly instead of resolving absolute sides"]
fn material3_components_resolve_absolute_content_padding_in_ltr_and_rtl() {
    compose_test::use_optimizations(Optimizations::NONE);

    for direction in [LayoutDirection::Ltr, LayoutDirection::Rtl] {
        let lead_min = Rc::new(Cell::new(0.dp()));
        let trail_min = Rc::new(Cell::new(0.dp()));
        let lm = lead_min.clone();
        let tm = trail_min.clone();

        let mut ui = ComposeTestRule::sized(IntSize::new(300, 400), move || {
            let lm = lm.clone();
            let tm = tm.clone();
            CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), move || {
                let lm = lm.clone();
                let tm = tm.clone();
                MaterialTheme!({
                    let lm = lm.clone();
                    let tm = tm.clone();
                    TopAppBar!(
                        || Box!(modifier = Modifier.size((40.dp(), 16.dp())).test_tag("bar_title")),
                        modifier = Modifier.width(200.dp()),
                        navigation_icon =
                            Some(|| Box!(modifier = Modifier.size(16.dp()).test_tag("bar_nav"))),
                        window_insets = WindowInsets::new(0, 0, 0, 0),
                        content_padding = PaddingValues::absolute(24.dp(), 4.dp(), 40.dp(), 4.dp()),
                    );

                    ListItem!(
                        modifier = Modifier.width(200.dp()),
                        leading_content = Some(move || {
                            lm.set(LOCAL_MINIMUM_INTERACTIVE_COMPONENT_SIZE.current());
                            Box!(modifier = Modifier.size(16.dp()));
                        }),
                        trailing_content = Some(move || {
                            tm.set(LOCAL_MINIMUM_INTERACTIVE_COMPONENT_SIZE.current());
                            Box!(modifier = Modifier.size(16.dp()));
                        }),
                        content_padding = PaddingValues::absolute(8.dp(), 4.dp(), 24.dp(), 4.dp()),
                        || Box!(modifier = Modifier.size((40.dp(), 16.dp())))
                    );

                    OutlinedTextField!(
                        "hello".to_string(),
                        |_| {},
                        modifier = Modifier.width(240.dp()),
                        label = Some(|| {
                            Box!(modifier = Modifier.size((30.dp(), 12.dp())).test_tag("otf_label"))
                        }),
                        content_padding =
                            Some(PaddingValues::absolute(28.dp(), 8.dp(), 44.dp(), 8.dp())),
                    );
                });
            });
        });

        let nav_bounds = ui.on_node_with_tag("bar_nav").get_unclipped_bounds_in_root();
        let otf_label_bounds =
            ui.on_node_with_tag_unmerged("otf_label").get_unclipped_bounds_in_root();
        if direction == LayoutDirection::Ltr {
            // TopAppBar adds 4.dp() horizontal icon inset inside content_padding (24 + 4 = 28).
            assert_eq!(nav_bounds.left, 28.dp());
            assert_eq!((lead_min.get(), trail_min.get()), (28.dp(), 12.dp()));
            assert_eq!(otf_label_bounds.left, 28.dp());
        } else {
            // In RTL (200-wide bar), start=right=40 + 4 icon inset -> 16-wide nav sits at [140, 156].
            assert_eq!(nav_bounds.left, 140.dp());
            assert_eq!(nav_bounds.right, 156.dp());
            // Leading gets start=right=24 (48 - 24 - 12 = 12); trailing gets end=left=8 (48 - 8 - 12 = 28).
            assert_eq!((lead_min.get(), trail_min.get()), (12.dp(), 28.dp()));
            // In RTL (240-wide field), floated label aligns to start=right=44 -> right edge at 196, left at 166.
            assert_eq!(otf_label_bounds.right, 196.dp());
            assert_eq!(otf_label_bounds.left, 166.dp());
        }
    }
}
