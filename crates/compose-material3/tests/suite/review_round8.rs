//! Eighth review round: chips and badges, against Jetpack Compose's material3. Each test
//! states the input, what Compose does, and what happens instead.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::Modifier;

/// `BadgedBox(badge = { if (count > 0) Badge { Text("$count") } }) { Icon(home) }` with a
/// count of 0, the usual way to hide a badge: Compose wraps the badge and the content each in a
/// `Box` (layout ids "badge" and "anchor"), so an empty badge lambda leaves an empty box and the
/// icon is laid out as ever, 24 x 24. Here the layout takes the badge's and content's nodes
/// directly and expects exactly two: with no badge node it lays out nothing, so the icon isn't
/// placed and the box is 0 x 0.
#[test]
fn badged_box_without_a_badge_still_shows_its_content() {
    let count = mutable_state_of(0);
    let handle = count.handle();
    let mut ui = compose_test(move || {
        MaterialTheme!({
            BadgedBox!(
                {
                    if handle.get() > 0 {
                        Badge!(content = Some(move || Text!("{}", handle.get())));
                    }
                },
                modifier = Modifier.test_tag("badged"),
                { Icon!(icons::filled::home(), Some("Home")) }
            );
        });
    });
    let bounds = ui.on_node_with_tag("badged").fetch_semantics_node().bounds;
    assert_eq!((bounds.width(), bounds.height()), (24.0, 24.0));
    let icon = ui.on_node_with_content_description("Home").fetch_semantics_node().bounds;
    assert_eq!((icon.width(), icon.height()), (24.0, 24.0));
}

/// `FilterChip(selected = true, onClick, label = { Text("Filter") })` with no leading icon:
/// Compose's `FilterChip` shows only what it's given (its samples add a check mark themselves,
/// `leadingIcon = if (selected) { … } else null`), so a selected chip is as wide as an
/// unselected one, its label 16 dp in. Here a selected chip without a leading icon gets a check
/// mark of its own: 18 dp wider (8 dp padding, an 18 dp icon, 8 dp spacing instead of 16 dp),
/// and an icon the caller can't turn off.
#[test]
fn selected_filter_chip_adds_no_icon_of_its_own() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Column!({
                FilterChip!(true, || {}, { Text!("Filter") }, modifier = Modifier.test_tag("on"));
                FilterChip!(false, || {}, { Text!("Filter") }, modifier = Modifier.test_tag("off"));
            });
        });
    });
    let on = ui.on_node_with_tag("on").fetch_semantics_node().bounds;
    let off = ui.on_node_with_tag("off").fetch_semantics_node().bounds;
    assert_eq!(on.width(), off.width());
}
