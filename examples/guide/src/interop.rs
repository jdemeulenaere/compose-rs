//! Android interop: properties and events, insets, dark theme (docs/guide/kotlin-to-rust.md,
//! "Android interop", "Logging"). Platform-neutral: desktop and tests provide empty properties.

use std::rc::Rc;

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

// ANCHOR: services
#[composable]
pub fn Links(url: String) {
    // `LocalUriHandler.current`: Android opens the link with an `ACTION_VIEW` intent, desktop
    // with the OS (http, https, mailto and tel only, unless widened).
    let uri_handler = LOCAL_URI_HANDLER.current();
    let clipboard = LOCAL_CLIPBOARD_MANAGER.current();
    let haptics = LOCAL_HAPTIC_FEEDBACK.current();
    Row!({
        TextButton!(on_click = move || uri_handler.open_uri(&url), {
            Text!("Open");
        });
        TextButton!(
            on_click = move || {
                clipboard.set_text("compose-rs");
                haptics.perform_haptic_feedback(HapticFeedbackType::Confirm);
            },
            {
                Text!("Copy");
            }
        );
    });
}
// ANCHOR_END: services

// ANCHOR: deep_links
#[composable]
pub fn AppLinks() {
    // The platform's handler, opening the app's own scheme too in this subtree.
    let handler: Rc<dyn UriHandler> =
        remember(|| Rc::new(PlatformUriHandler::current().with_schemes(&["https", "myapp"])));
    CompositionLocalProvider(LOCAL_URI_HANDLER.provides(handler), || {
        Links("myapp://inbox".to_string());
    });
}
// ANCHOR_END: deep_links

// ANCHOR: logging
pub fn load(items: &[&str]) {
    // `Log.i(TAG, …)`: logcat on Android (tagged `guide`, the crate), stderr on desktop.
    compose::log::info!("loaded {} items", items.len());
    if items.is_empty() {
        compose::log::warn!("nothing to show");
    }
}
// ANCHOR_END: logging
