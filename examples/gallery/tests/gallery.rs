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
    // Past a `NavDisplay`'s 700 ms fade.
    let step = std::time::Duration::from_millis(800);
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
    let menu_top = |ui: &mut ComposeTestRule| {
        ui.on_node(described("Menu")).fetch_semantics_snapshot().bounds.top
    };
    let expanded = menu_top(&mut ui);
    // The cards scroll, the bar slides partly away (enter always).
    let card = ui.on_node_with_text("Card 18").fetch_semantics_snapshot().center();
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

/// Past a `NavDisplay`'s 700 ms fade, and a drawer's or a sheet's motion.
const SETTLED: std::time::Duration = std::time::Duration::from_millis(800);

/// The gallery on its "More controls" screen, on the tab named `tab`.
fn more_controls(tab: &str) -> ComposeTestRule {
    let mut ui = ComposeTestRule::with_manual_clock(gallery::Gallery);
    ui.on_node(described("Menu")).perform_click();
    ui.advance_time_by(SETTLED);
    ui.on_node_with_text("More controls").perform_click();
    ui.advance_time_by(SETTLED);
    ui.on_node_with_text(tab).perform_click();
    ui.advance_time_by(SETTLED);
    ui
}

#[test]
fn the_drawer_opens_more_controls_and_back_returns_with_the_drawer_closed() {
    let mut ui = more_controls("Fields");
    ui.on_node_with_text("Cards: 20").assert_does_not_exist();
    ui.on_node_with_text("Fields").assert_is_selected();
    ui.on_node_with_text("Choices").assert_is_not_selected();
    assert_eq!(ui.count_nodes(&has_role(Role::Tab)), 3);
    assert!(ui.host().on_back_pressed());
    ui.advance_time_by(SETTLED);
    ui.on_node_with_text("Cards: 20").assert_exists();
    ui.on_node_with_text("About").assert_is_not_displayed();
}

#[test]
fn each_field_tells_the_keyboard_its_kind() {
    use compose::ui::{ImeAction, KeyboardType};
    let mut ui = more_controls("Fields");
    for (label, kind, action) in [
        ("Password", KeyboardType::Password, ImeAction::Next),
        ("Number", KeyboardType::Number, ImeAction::Next),
        ("Phone", KeyboardType::Phone, ImeAction::Done),
    ] {
        ui.on_node(has_text(label).and(has_set_text_action())).perform_click();
        ui.advance_time_by_frame();
        let options = ui.host().text_input_options().expect("a focused field");
        assert_eq!((options.keyboard_type, options.ime_action), (kind, action), "{label}");
        assert!(options.single_line, "{label}");
    }
    ui.on_node(has_text("Notes").and(has_set_text_action())).perform_click();
    ui.advance_time_by_frame();
    assert!(!ui.host().text_input_options().expect("a focused field").single_line);
}

#[test]
fn the_password_is_masked_until_shown() {
    let mut ui = more_controls("Fields");
    ui.on_node(has_text("Password").and(has_set_text_action())).perform_text_input("abc");
    ui.advance_time_by_frame();
    ui.on_node(has_text("abc")).assert_does_not_exist();
    ui.on_node(has_text("\u{2022}\u{2022}\u{2022}")).assert_exists();
    ui.on_node(described("Show the password")).perform_click();
    ui.advance_time_by_frame();
    ui.on_node(has_text("abc")).assert_exists();
    ui.on_node(described("Hide the password")).assert_exists();
}

#[test]
fn a_button_clears_the_focus_and_a_tap_gives_it_back() {
    let mut ui = more_controls("Fields");
    ui.on_node(has_text("Notes").and(has_set_text_action())).perform_click();
    ui.advance_time_by_frame();
    assert_eq!(ui.count_nodes(&is_focused()), 1);
    ui.on_node_with_text("Clear the focus").perform_click();
    ui.advance_time_by_frame();
    assert_eq!(ui.count_nodes(&is_focused()), 0);
    assert!(ui.host().text_input_options().is_none());
    ui.on_node(has_text("Notes").and(has_set_text_action())).perform_click();
    ui.advance_time_by_frame();
    assert_eq!(ui.count_nodes(&is_focused()), 1);
}

#[test]
fn the_parent_checkbox_shows_its_children_and_sets_them() {
    use compose::ui::{SemanticsProperties, ToggleableState};
    let state = |wanted: ToggleableState| {
        Matcher::new("toggleable state", move |config| {
            config.get(&SemanticsProperties::TOGGLEABLE_STATE) == Some(&wanted)
        })
    };
    let mut ui = more_controls("Choices");
    assert_eq!(ui.count_nodes(&state(ToggleableState::Off)), 3);
    ui.on_node(is_toggleable().and(described("Milk"))).perform_click();
    ui.advance_time_by(SETTLED);
    ui.on_node(described("Milk and eggs").and(state(ToggleableState::Indeterminate)))
        .assert_exists();
    ui.on_node(described("Milk and eggs")).perform_click();
    ui.advance_time_by(SETTLED);
    assert_eq!(ui.count_nodes(&state(ToggleableState::On)), 3);
}

#[test]
fn a_selectable_row_has_no_role_and_is_selected_alone() {
    use compose::ui::SemanticsProperties;
    let mut ui = more_controls("Choices");
    ui.on_node_with_text("Small").assert_is_selected();
    ui.on_node_with_text("Large").assert_is_not_selected().perform_click();
    ui.advance_time_by_frame();
    ui.on_node_with_text("Large").assert_is_selected();
    ui.on_node_with_text("Small").assert_is_not_selected();
    let row = ui.on_node_with_text("Large").fetch_semantics_snapshot();
    assert!(!row.config.contains(&SemanticsProperties::ROLE));
}

#[test]
fn a_message_has_actions_of_its_own_and_they_follow_the_list() {
    let mut ui = more_controls("Choices");
    ui.on_node_with_text("Message 1").perform_custom_accessibility_action_with_label("Star");
    ui.advance_time_by_frame();
    ui.on_node_with_text("Message 1, starred")
        .perform_custom_accessibility_action_with_label("Move to the end");
    ui.advance_time_by_frame();
    let tops: Vec<f32> = ["Message 2", "Message 3", "Message 1, starred"]
        .into_iter()
        .map(|text| ui.on_node_with_text(text).fetch_semantics_snapshot().bounds.top)
        .collect();
    assert!(tops[0] < tops[1] && tops[1] < tops[2], "{tops:?}");
    // The first in the list now, whose actions are its own and not the place's.
    ui.on_node_with_text("Message 2").perform_custom_accessibility_action_with_label("Star");
    ui.advance_time_by_frame();
    ui.on_node_with_text("Message 2, starred")
        .perform_custom_accessibility_action_with_label("Unstar");
    ui.advance_time_by_frame();
    ui.on_node_with_text("Message 2").assert_exists();
    // Still starred as it moves, whatever its action was made with.
    ui.on_node_with_text("Message 1, starred")
        .perform_custom_accessibility_action_with_label("Move to the end");
    ui.advance_time_by_frame();
    ui.on_node_with_text("Message 1, starred").assert_exists();
}

#[test]
fn a_traversal_group_is_read_by_its_indices_and_all_together() {
    use compose::ui::{SemanticsProperties, sorted_for_accessibility};
    let mut ui = more_controls("Choices");
    ui.on_node_with_text("Read second").assert_exists();
    let first_x = ui.on_node_with_text("Read first").fetch_semantics_snapshot().bounds.left;
    let second_x = ui.on_node_with_text("Read second").fetch_semantics_snapshot().bounds.left;
    assert!(second_x < first_x, "shown in another order than it's read");
    let root = ui.host().semantics_tree(true);
    let read: Vec<String> = sorted_for_accessibility(&root)
        .into_iter()
        .flat_map(|node| node.config.get(&SemanticsProperties::TEXT).cloned().unwrap_or_default())
        .map(|text| text.to_string())
        .filter(|text| text.starts_with("Read "))
        .collect();
    assert_eq!(read, ["Read first", "Read second", "Read third", "Read after the group"]);
}

#[test]
fn a_bottom_sheet_with_its_handle_opens_over_a_field_and_closes() {
    let mut ui = more_controls("Windows");
    ui.on_node(has_text("Behind").and(has_set_text_action())).perform_text_input("word");
    ui.on_node_with_text("Open a bottom sheet").perform_click();
    ui.advance_time_by(SETTLED);
    ui.on_node(described("Drag handle")).assert_exists();
    ui.on_node_with_text("Line 1").assert_is_displayed();
    assert!(ui.host().on_back_pressed());
    ui.advance_time_by(SETTLED);
    ui.on_node_with_text("Line 1").assert_does_not_exist();
    ui.on_node(has_text("word")).assert_exists();
}

#[test]
fn a_dialogs_field_takes_the_focus_and_the_field_behind_gets_it_back() {
    let mut ui = more_controls("Windows");
    ui.on_node(has_text("Behind").and(has_set_text_action())).perform_text_input("word");
    ui.on_node_with_text("Open a dialog with a field").perform_click();
    ui.advance_time_by(SETTLED);
    ui.on_node(has_text("In the dialog").and(is_focused())).assert_exists();
    ui.on_node(has_text("In the dialog")).perform_text_input("other");
    ui.on_node(has_text("word")).assert_exists();
    ui.on_node_with_text("OK").perform_click();
    ui.advance_time_by(SETTLED);
    ui.on_node_with_text("In the dialog").assert_does_not_exist();
    ui.on_node(has_text("word").and(is_focused())).assert_exists();
}
