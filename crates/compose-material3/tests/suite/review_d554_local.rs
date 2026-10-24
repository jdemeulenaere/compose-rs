//! Local review of D554: are texts under intrinsic probes drawn?

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_material3::*;
use compose_test::*;
use compose_ui::{IntSize, Modifier};

fn drawn(ui: &mut ComposeTestRule) -> String {
    ui.wait_for_idle();
    draw_snapshot(ui.host())
}

#[test]
fn review_d554_tab_texts_are_drawn() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 200), || {
        MaterialTheme!({
            TabRow!(0, modifier = Modifier.test_tag("row"), {
                for index in 0..3 {
                    Tab!(index == 0, || {}, text = Some(move || Text!("Tab {}", index)));
                }
            });
        });
    });
    let snapshot = drawn(&mut ui);
    for index in 0..3 {
        assert!(
            snapshot.contains(&format!("paragraph \"Tab {index}\"")),
            "Tab {index}:\n{snapshot}"
        );
    }
}

#[test]
fn review_d554_scrollable_tab_texts_are_drawn() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 200), || {
        MaterialTheme!({
            ScrollableTabRow!(0, {
                for index in 0..3 {
                    Tab!(index == 0, || {}, text = Some(move || Text!("Tab {}", index)));
                }
            });
        });
    });
    let snapshot = drawn(&mut ui);
    for index in 0..3 {
        assert!(
            snapshot.contains(&format!("paragraph \"Tab {index}\"")),
            "Tab {index}:\n{snapshot}"
        );
    }
}

#[test]
fn review_d554_segment_labels_are_drawn() {
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 400), || {
        MaterialTheme!({
            let labels = ["Day", "Week", "Month and year"];
            SingleChoiceSegmentedButtonRow!(|scope: SingleChoiceSegmentedButtonRowScope| {
                for (index, label) in labels.into_iter().enumerate() {
                    SegmentedButton!(
                        scope,
                        index == 0,
                        || {},
                        SegmentedButtonDefaults::item_shape(index, labels.len()),
                        label = { Text!("{}", label) }
                    );
                }
            });
        });
    });
    let snapshot = drawn(&mut ui);
    for label in ["Day", "Week", "Month and year"] {
        assert!(snapshot.contains(&format!("paragraph \"{label}\"")), "{label}:\n{snapshot}");
    }
}

#[test]
fn review_d554_menu_items_are_drawn() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), || {
        MaterialTheme!({
            Box!({
                Box!(modifier = Modifier.size(compose_ui::Dp(48.0)));
                DropdownMenu!(true, || {}, {
                    DropdownMenuItem!({ Text!("Edit") }, || {});
                    DropdownMenuItem!({ Text!("Rename the file") }, || {});
                });
            });
        });
    });
    let snapshot = drawn(&mut ui);
    for label in ["Edit", "Rename the file"] {
        assert!(snapshot.contains(&format!("paragraph \"{label}\"")), "{label}:\n{snapshot}");
    }
}

#[test]
fn review_d554_dialog_texts_are_drawn() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), || {
        MaterialTheme!({
            AlertDialog!(
                on_dismiss_request = || {},
                confirm_button = {
                    TextButton!(on_click = || {}, {
                        Text!("Delete");
                    });
                },
                dismiss_button = {
                    TextButton!(on_click = || {}, {
                        Text!("Cancel");
                    });
                },
                title = {
                    Text!("Delete the item?");
                },
            );
        });
    });
    let snapshot = drawn(&mut ui);
    for label in ["Delete", "Cancel", "Delete the item?"] {
        assert!(snapshot.contains(&format!("paragraph \"{label}\"")), "{label}:\n{snapshot}");
    }
}
