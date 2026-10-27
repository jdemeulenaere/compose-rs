//! Accessibility and semantics (docs/guide/kotlin-to-rust.md, "Accessibility and semantics").

use compose::foundation::layout::*;
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

// ANCHOR: semantics
#[composable]
pub fn Rating(stars: u32) {
    let colors = MaterialTheme::color_scheme();
    Row!(
        // One description for the row instead of five unlabeled icons.
        modifier = Modifier.clear_and_set_semantics(move |config| {
            config.set_content_description(format!("{stars} of 5 stars"));
        }),
        {
            for index in 0..5 {
                let tint = if index < stars { colors.primary } else { colors.outline_variant };
                Icon!(icons::filled::star(), None, tint = tint);
            }
        }
    );
}

#[composable]
pub fn Setting(label: SharedString, on: bool, on_change: impl Fn(bool) + 'static) {
    Row!(
        // A row read as one control, toggled as a switch: `Modifier.toggleable(role = Switch)`.
        modifier = Modifier.fill_max_width().toggleable_with(
            on,
            ClickableOptions { role: Some(Role::Switch), ..ClickableOptions::DEFAULT },
            on_change,
        ),
        vertical_alignment = Alignment::CENTER_VERTICALLY,
        |row| {
            Text!(label, modifier = Modifier.weight(row, 1.0));
            Switch!(on, None::<fn(bool)>);
        }
    );
}
// ANCHOR_END: semantics
