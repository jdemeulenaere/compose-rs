//! `Scrim`, as Compose's Material 3: a layer dimming what's behind a modal surface.

use compose_runtime::composable;
use compose_ui::{Color, Modifier, Paint};

use crate::theme::MaterialTheme;

/// As Compose's `ScrimDefaults`.
pub struct ScrimDefaults;

impl ScrimDefaults {
    /// The scrim color at 32 %, as `color` (`ScrimTokens`).
    pub fn color() -> Color {
        MaterialTheme::color_scheme().scrim.with_alpha(0.32)
    }
}

/// A layer filling its container in `color` at `alpha()` (clamped to 0..1, read while
/// drawing), as Compose's `Scrim`: nothing without a color. With `on_click`, a tap or the
/// click action calls it, and `content_description` says what it dismisses; without, it has
/// neither and takes no input.
#[composable]
#[allow(non_snake_case)]
pub fn Scrim(
    content_description: Option<&'static str>,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::scrim_default_on_click())] on_click: Option<impl Fn() + 'static>,
    #[default(|| 1.0)] alpha: impl Fn() -> f32 + 'static,
    #[default(crate::ScrimDefaults::color())] color: Color,
) {
    if !color.is_specified() {
        return;
    }
    let modifier = modifier.fill_max_size();
    let modifier = match on_click {
        Some(on_click) => modifier.then_custom(crate::drawer::ScrimElement {
            close: compose_runtime::callback(move || on_click()),
            enabled: true,
            description: content_description,
        }),
        None => modifier,
    };
    compose_foundation::layout::Canvas(modifier, move |scope| {
        let alpha = alpha().clamp(0.0, 1.0);
        let color = color.with_alpha(color.alpha() as f32 / 255.0 * alpha);
        scope.draw_rect(scope.bounds(), Paint::fill(color));
    });
}
