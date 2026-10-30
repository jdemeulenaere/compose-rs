//! Theming and Material 3 (docs/guide/kotlin-to-rust.md, "Theming and Material 3").

use compose::foundation::is_system_in_dark_theme;
use compose::foundation::layout::*;
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

// ANCHOR: theme
const BRAND: Color = Color::from_argb(0xFF006A6A);

#[composable]
pub fn AppTheme(content: impl Fn() + 'static) {
    let base = if is_system_in_dark_theme() { dark_color_scheme() } else { light_color_scheme() };
    // Struct update syntax where Kotlin names `lightColorScheme(primary = …)`'s arguments.
    let color_scheme = ColorScheme { primary: BRAND, ..base };
    let typography = Typography::default(); // Compose's `Typography()`
    let typography = Typography {
        title_large: TextStyle { font_weight: Some(FontWeight::BOLD), ..typography.title_large },
        ..typography
    };
    MaterialTheme!(color_scheme = color_scheme, typography = typography, content = content);
}
// ANCHOR_END: theme

// ANCHOR: theme_reads
#[composable]
pub fn Banner(text: SharedString) {
    // `MaterialTheme.colorScheme`, `.typography`, `.shapes`: reads of composition locals.
    let colors = MaterialTheme::color_scheme();
    Surface!(color = colors.primary_container, shape = MaterialTheme::shapes().medium.clone(), {
        Text!(
            text,
            modifier = Modifier.padding(16.dp()),
            style = MaterialTheme::typography().title_large.clone()
        );
    });
}
// ANCHOR_END: theme_reads

// ANCHOR: local
/// A composition local of the app's own: `compositionLocalOf { 8.dp }`.
pub static LOCAL_SPACING: CompositionLocal<Dp> = CompositionLocal::new("LocalSpacing", || Dp(8.0));

#[composable]
pub fn Spaced() {
    CompositionLocalProvider(LOCAL_SPACING.provides(16.dp()), || {
        Column!(vertical_arrangement = Arrangement::spaced_by(LOCAL_SPACING.current()), {
            Text!("One");
            Text!("Two");
        });
    });
}
// ANCHOR_END: local

// ANCHOR: scaffold
#[composable]
pub fn Notes() {
    let notes = remember_saveable(|| mutable_state_of(0u32));
    let snackbars = remember(SnackbarHostState::new);
    let scope = remember_coroutine_scope();
    AppTheme!({
        Scaffold!(
            top_bar = {
                TopAppBar!(
                    { Text!("Notes") },
                    actions = |_row| {
                        IconButton!(on_click = move || notes.set(0), {
                            Icon!(icons::filled::delete(), Some("Delete all"));
                        });
                    }
                );
            },
            snackbar_host = {
                SnackbarHost!(snackbars);
            },
            floating_action_button = {
                FloatingActionButton!(
                    on_click = move || {
                        notes.update(|count| count + 1);
                        scope.launch(async move {
                            snackbars.show_snackbar("Note added").await;
                        });
                    },
                    {
                        Icon!(icons::filled::add(), Some("Add a note"));
                    }
                );
            },
            |padding: PaddingValues| {
                Column!(modifier = Modifier.padding(padding).padding(16.dp()), {
                    Text!("{} notes", notes.get());
                    // Colors, as other `…Defaults`, change by struct update.
                    Button!(
                        on_click = move || notes.set(0),
                        enabled = notes.get() > 0,
                        colors = ButtonColors {
                            container_color: MaterialTheme::color_scheme().error,
                            ..ButtonDefaults::button_colors()
                        },
                        {
                            Text!("Clear");
                        }
                    );
                });
            }
        );
    });
}
// ANCHOR_END: scaffold
