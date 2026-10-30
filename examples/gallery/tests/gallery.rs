use compose::test::*;
use compose::ui::{Offset, PointerEvent, Role};

fn described(description: &'static str) -> Matcher {
    Matcher::new("described", move |config| {
        config
            .get(&compose::ui::SemanticsProperties::CONTENT_DESCRIPTION)
            .into_iter()
            .flatten()
            .any(|text| text.as_str() == description)
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
    // The new card is first, and a snackbar says so. The list keeps its first visible card
    // (by key, as Compose's), so the new one is above it, out of view: scrolled up, it shows.
    ui.on_node_with_text("Card 20 added at the top").assert_exists();
    ui.host()
        .send_pointer(PointerEvent::scroll(Offset::new(200.0, 500.0), Offset::new(0.0, -100.0)));
    ui.advance_time_by_frame();
    ui.on_node_with_text("Card 20").assert_exists();
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
    ui.on_node_with_text("About").assert_is_not_displayed();
    ui.on_node(described("Menu")).perform_click();
    ui.advance_time_by(std::time::Duration::from_millis(400));
    ui.on_node_with_text("About").perform_click();
    ui.advance_time_by(std::time::Duration::from_millis(400));
    ui.on_node_with_text("OK").assert_exists();
}

#[test]
fn a_card_opens_its_screen_and_back_returns_to_the_tour_as_it_was() {
    let mut ui = ComposeTestRule::with_manual_clock(gallery::Gallery);
    let step = std::time::Duration::from_millis(500);
    ui.on_node(described("Add a card")).perform_click();
    ui.advance_time_by(step);
    ui.on_node_with_text("Card 19").perform_click();
    ui.advance_time_by(step);
    // The card's screen replaces the tour.
    ui.on_node_with_text("Cards: 21").assert_does_not_exist();
    ui.on_node_with_text("Not liked yet").assert_exists();
    ui.on_node(described("Like")).perform_click();
    ui.advance_time_by(step);
    ui.on_node_with_text("Liked").assert_exists();
    // Back returns to the tour, its saved state (the added card) kept.
    assert!(ui.host().on_back_pressed());
    ui.advance_time_by(step);
    ui.on_node_with_text("Cards: 21").assert_exists();
    // So does the top bar's arrow; the card's screen opens anew.
    ui.on_node_with_text("Card 19").perform_click();
    ui.advance_time_by(step);
    ui.on_node_with_text("Not liked yet").assert_exists();
    ui.on_node(described("Back")).perform_click();
    ui.advance_time_by(step);
    ui.on_node_with_text("Cards: 21").assert_exists();
    // At the tour, back isn't taken: it leaves the app.
    assert!(!ui.host().has_enabled_back_handlers());
}

#[test]
fn the_top_bar_comes_back_from_a_card_as_it_was_left() {
    use compose::ui::{Offset, PointerEvent};
    let mut ui = ComposeTestRule::with_manual_clock(gallery::Gallery);
    let step = std::time::Duration::from_millis(500);
    let menu_top =
        |ui: &mut ComposeTestRule| ui.on_node(described("Menu")).fetch_semantics_node().bounds.top;
    let expanded = menu_top(&mut ui);
    // The cards scroll, the bar slides partly away (enter always).
    let card = ui.on_node_with_text("Card 18").fetch_semantics_node().center();
    ui.host().send_pointer(PointerEvent::scroll(card, Offset::new(0.0, 30.0)));
    ui.advance_time_by(step);
    let scrolled = menu_top(&mut ui);
    assert!(scrolled < expanded, "{scrolled} < {expanded}");
    ui.on_node_with_text("Card 17").perform_click();
    ui.advance_time_by(step);
    ui.on_node_with_text("Not liked yet").assert_exists();
    assert!(ui.host().on_back_pressed());
    ui.advance_time_by(step);
    // The bar's state (`remember_top_app_bar_state`) is saveable, as Compose's: kept.
    assert_eq!(menu_top(&mut ui), scrolled);
}
