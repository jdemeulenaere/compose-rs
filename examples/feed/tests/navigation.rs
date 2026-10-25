//! The navigation screens (docs/17): the launcher opens a heavy screen of new content each
//! visit, and back disposes it.

use compose::test::*;
use compose::ui::{Density, IntSize};
use compose_rs_feed::navigation::*;

/// The Pixel 8 Pro's screen, as the benchmark runs it: 1008 × 2244 px at 2.25.
fn phone() -> ComposeTestRule {
    let mut ui = ComposeTestRule::with_manual_clock(NavigationApp);
    ui.host().resize(IntSize::new(1008, 2244));
    ui.host().set_density(Density::new(2.25));
    ui.advance_time_by_frame();
    ui
}

#[test]
fn the_hash_is_the_kotlin_apps() {
    // The values Navigation.kt's `mix` and `pick` compute too.
    assert_eq!(mix(0), 0);
    assert_eq!(mix(1), 0x6889_90C0);
    assert_eq!(mix(0xDEAD_BEEF), 0xE628_C683);
    assert_eq!(pick(1, 0, 1, 1_000_000), 776_721);
    assert_eq!(pick(7, 42, 3, 1_000), 59);
}

#[test]
fn each_visit_draws_new_content_from_its_seed() {
    let first = NavigationItem::of(1, 0);
    assert_eq!(first, NavigationItem::of(1, 0), "the same seed draws the same");
    assert_ne!(first, NavigationItem::of(2, 0), "the next visit draws anew");
    assert_ne!(NavigationItem::of(1, 5), NavigationItem::of(2, 5));
    let NavigationItem::Profile { name, initials, bio, .. } = first else {
        panic!("item 0 is a profile");
    };
    // Words of two to four syllables, capitalized names.
    let words: Vec<&str> = name.split(' ').collect();
    assert_eq!(words.len(), 2);
    assert!(words.iter().all(|word| word.starts_with(|c: char| c.is_ascii_uppercase())));
    assert_eq!(initials.len(), 2);
    assert!(bio.ends_with('.'));
    assert!(bio.split(". ").count() >= 2, "two or three sentences: {bio}");
    let kinds: Vec<NavigationKind> = (0..6).map(NavigationKind::of).collect();
    assert_eq!(
        kinds,
        [
            NavigationKind::Profile,
            NavigationKind::Article,
            NavigationKind::Stats,
            NavigationKind::Gallery,
            NavigationKind::Banner,
            NavigationKind::Profile
        ]
    );
}

#[test]
fn the_launcher_opens_the_heavy_screen_and_back_disposes_it() {
    let mut ui = phone();
    ui.on_node_with_text("Navigation").assert_exists();
    ui.on_node_with_text("0 visits").assert_exists();
    ui.on_node_with_text("Open screen 1").perform_click();
    ui.advance_time_by_frame();
    ui.on_node_with_text("Screen 1").assert_exists();
    assert_eq!(ui.count_nodes(&has_text("Open screen 1")), 0);
    // Every item is composed at once (a column, not a lazy list): each gallery's title.
    assert_eq!(
        ui.count_nodes(&has_text_containing("Album ")),
        (NAVIGATION_ITEMS / NAVIGATION_KINDS) as usize
    );
    let NavigationItem::Profile { name: first_name, .. } = NavigationItem::of(1, 0) else {
        panic!("item 0 is a profile");
    };
    ui.on_node_with_text(&first_name).assert_exists();

    assert!(ui.host().on_back_pressed(), "the heavy screen takes back");
    ui.advance_time_by_frame();
    ui.on_node_with_text("1 visits").assert_exists();
    assert_eq!(ui.count_nodes(&has_text_containing("Album ")), 0);
    assert!(!ui.host().has_enabled_back_handlers(), "the launcher leaves back to the activity");

    // The second visit shows new content; the top bar's back button returns too.
    ui.on_node_with_text("Open screen 2").perform_click();
    ui.advance_time_by_frame();
    ui.on_node_with_text("Screen 2").assert_exists();
    assert_eq!(ui.count_nodes(&has_text(&first_name)), 0);
    let NavigationItem::Profile { name: second_name, .. } = NavigationItem::of(2, 0) else {
        panic!("item 0 is a profile");
    };
    ui.on_node_with_text(&second_name).assert_exists();
    ui.on_node(has_content_description("Back")).perform_click();
    ui.advance_time_by_frame();
    ui.on_node_with_text("Open screen 3").assert_exists();
}
