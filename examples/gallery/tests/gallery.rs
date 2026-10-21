use compose::test::*;
use compose::ui::Role;

#[test]
fn the_gallery_shows_its_controls_and_counts_clicks() {
    // A manual clock: the gallery's indeterminate spinner never settles.
    let mut ui = ComposeTestRule::with_manual_clock(gallery::Gallery);
    ui.on_node_with_text("Count: 0").assert_exists();
    ui.on_node(has_text("Add").or(Matcher::new("described Add", |config| {
        config.content_description.iter().any(|description| description.as_str() == "Add")
    })))
    .perform_click();
    ui.on_node_with_text("Count: 1").assert_exists();
    assert_eq!(ui.count_nodes(&has_role(Role::RadioButton)), 3);
    ui.on_node(has_text_containing("Card 0")).assert_exists();
    ui.on_node(has_role(Role::Switch)).perform_click();
    ui.advance_time_by(std::time::Duration::from_secs(1));
    ui.on_node_with_text("Shown while the switch is on").assert_does_not_exist();
}
