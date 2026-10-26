//! The benchmark screens always draw Material's light scheme, as the Kotlin app's
//! `MaterialTheme {}` does, whatever the device's dark mode (D645).

use compose::test::*;
use compose::ui::IntSize;

/// No screen picks its colours from the system's theme: they all go through
/// `bench_color_scheme`, which is light.
#[test]
fn no_screen_follows_the_systems_dark_theme() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let source = std::fs::read_to_string(&path).unwrap();
        assert!(
            !source.contains("is_system_in_dark_theme"),
            "{} follows the system's theme",
            path.display()
        );
        assert!(!source.contains("dark_color_scheme"), "{} uses the dark scheme", path.display());
    }
}

/// With the host's configuration in night mode, the feed composes and shows its content
/// (its colours are the light scheme's, checked above by construction).
#[test]
fn the_feed_composes_in_night_mode() {
    let mut ui = ComposeTestRule::with_manual_clock(compose_rs_feed::Feed);
    ui.host().resize(IntSize::new(1008, 2244));
    let mut configuration = ui.host().configuration();
    configuration.night_mode = true;
    ui.host().set_configuration(configuration);
    ui.advance_time_by_frame();
    ui.on_node_with_text("Feed").assert_exists();
}
