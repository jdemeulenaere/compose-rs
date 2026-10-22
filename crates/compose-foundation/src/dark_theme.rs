//! `is_system_in_dark_theme`, as Compose's `isSystemInDarkTheme`.

/// Whether the system's dark theme is on, as Compose's `isSystemInDarkTheme()`: from the host's
/// configuration (`LOCAL_CONFIGURATION`, Android's `uiMode`; the system's appearance on desktop).
/// Read in composition, a change recomposes the reader.
///
/// # Panics
///
/// Outside composition.
pub fn is_system_in_dark_theme() -> bool {
    compose_ui::LOCAL_CONFIGURATION.current().night_mode
}
