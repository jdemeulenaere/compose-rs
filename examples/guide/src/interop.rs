//! Android interop: properties and events, insets, dark theme (docs/guide/kotlin-to-rust.md,
//! "Android interop"). Platform-neutral: desktop and tests provide empty properties.

use compose::foundation::is_system_in_dark_theme;
use compose::foundation::layout::*;
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

// ANCHOR: embedded
#[composable]
pub fn Embedded() {
    // What Kotlin passes with `ComposeRsView.setProp`, and the listener of `setOnEventListener`.
    let props = LOCAL_HOST_PROPS.current();
    let events = LOCAL_HOST_EVENTS.current();
    // A read of a property recomposes this scope when Kotlin sets it again.
    let user = props.string("user");
    let color_scheme =
        if is_system_in_dark_theme() { dark_color_scheme() } else { light_color_scheme() };
    MaterialTheme!(color_scheme = color_scheme, {
        // The view reports the window's insets: keep the content clear of the system bars.
        Surface!(modifier = Modifier.fill_max_size(), {
            Column!(modifier = Modifier.safe_drawing_padding().padding(16.dp()), {
                Text!("Signed in as {}", user.as_deref().unwrap_or("nobody"));
                Button!(
                    on_click = move || {
                        events.send("sign_out", "");
                    },
                    {
                        Text!("Sign out");
                    }
                );
            });
        });
    });
}
// ANCHOR_END: embedded

// ANCHOR: configuration
#[composable]
pub fn Adaptive() {
    // `LocalConfiguration.current`: the window's size in dp, orientation, font scale, night mode.
    let configuration = LOCAL_CONFIGURATION.current();
    if configuration.screen_width_dp >= 600.0 {
        Text!("Two panes");
    } else {
        Text!("One pane");
    }
}
// ANCHOR_END: configuration
