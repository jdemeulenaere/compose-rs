use compose::test::*;
use compose::ui::Role;

fn described(description: &'static str) -> Matcher {
    Matcher::new("described", move |config| {
        config.content_description.iter().any(|text| text.as_str() == description)
    })
}

#[test]
fn the_gallery_shows_its_controls_and_adds_cards() {
    // A manual clock: the gallery's indeterminate spinner never settles.
    let mut ui = ComposeTestRule::with_manual_clock(gallery::Gallery);
    ui.on_node_with_text("Cards: 20").assert_exists();
    ui.on_node(described("Add a card")).perform_click();
    ui.advance_time_by(std::time::Duration::from_millis(500));
    ui.on_node_with_text("Cards: 21").assert_exists();
    // The new card is first, and a snackbar says so.
    ui.on_node_with_text("Card 20").assert_exists();
    ui.on_node_with_text("Card 20 added at the top").assert_exists();
    assert_eq!(ui.count_nodes(&has_role(Role::RadioButton)), 3);
    ui.on_node(has_role(Role::Switch)).perform_click();
    ui.advance_time_by(std::time::Duration::from_secs(1));
    ui.on_node_with_text("Shown while the switch is on").assert_does_not_exist();
}

#[test]
fn search_filters_the_cards() {
    let mut ui = ComposeTestRule::with_manual_clock(gallery::Gallery);
    ui.on_node(described("Search")).perform_click();
    ui.advance_time_by(std::time::Duration::from_millis(100));
    ui.on_node(has_text("Search the cards").and(is_focused())).assert_exists();
    ui.on_node(has_text("Search the cards")).perform_text_input("13");
    ui.advance_time_by(std::time::Duration::from_millis(100));
    ui.on_node_with_text("Card 13").assert_exists();
    ui.on_node_with_text("Card 12").assert_does_not_exist();
    ui.on_node(described("Close the search")).perform_click();
    ui.advance_time_by(std::time::Duration::from_millis(100));
    ui.on_node_with_text("Card 12").assert_exists();
}

#[test]
fn the_menu_opens_the_drawer() {
    let mut ui = ComposeTestRule::with_manual_clock(gallery::Gallery);
    ui.on_node_with_text("About").assert_does_not_exist();
    ui.on_node(described("Menu")).perform_click();
    ui.advance_time_by(std::time::Duration::from_millis(400));
    ui.on_node_with_text("About").perform_click();
    ui.advance_time_by(std::time::Duration::from_millis(400));
    ui.on_node_with_text("OK").assert_exists();
}
